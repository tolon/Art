//! Opening a Commodore 8-bit disk or tape in the commander (Task 5 Step 7).
//!
//! The same container model as a disc and an archive, over media that are
//! **flat**: a 1541 directory has no subdirectories, and neither does a T64.
//! So this pane has no trail and no "up" — what you see when it opens is all
//! of it.
//!
//! Read-only, in every direction. ART reads these; nothing here writes one,
//! and there is no command in this module that could.
//!
//! ## Sizes are in blocks, and the pane says so
//!
//! A directory entry carries a size in 254-byte blocks — that is the number a
//! real `LOAD"$"` listing shows, and it is what ART reports (`blocks × 254`).
//! It is an *allocation*, not the file's length: the last block is rarely
//! full. The exact length is known only once the sector chain has been walked,
//! which is what extraction does, so the extracted file's size is the true
//! one and the listing's is the disk's own claim.

use std::path::PathBuf;
use std::sync::Arc;

use serde::Serialize;
use tauri::{AppHandle, Emitter, State};

use super::jobs::{spawn_job, JobRegistry};
use super::oplog::{user_operation, write_result};
use super::panel::PanelEntry;
use super::volume_write::{CopyOptions, VolumeWriteResult, VOLUME_WRITE_EVENT};
use crate::core::archive::extract::ExtractOutcome;
use crate::core::cbm::d64::{CbmEntry, D64Image};
use crate::core::cbm::t64::{T64Archive, T64Entry};
use crate::core::error::{CoreError, CoreResult};
use crate::core::jobs::{JobId, ProgressSink};
use crate::core::oplog::{JsonlOperationLog, OperationOutcome};
use crate::core::volume::write::copy::OverwritePolicy;
use crate::error::AppResult;

/// Bytes one directory block accounts for: 256 per sector, less the two that
/// point at the next one.
const BYTES_PER_BLOCK: u64 = 254;

/// What opening a Commodore image reports.
#[derive(Debug, Clone, Serialize)]
pub struct CbmInfo {
    /// `"d64"`, `"d71"`, `"d81"` or `"t64"` — from the file itself.
    pub format: String,
    /// The disk's name, or a tape archive's container name.
    pub volume_name: String,
    /// A disk's two-character id. Empty for a tape.
    pub disk_id: String,
    pub entry_count: usize,
    /// Things the image says about itself that ART had to work around, in
    /// plain sentences for the pane to show. A T64 whose header count is
    /// wrong is the common one, and it is common enough that silence would be
    /// the surprising choice.
    pub notes: Vec<String>,
}

/// Whichever kind of Commodore container this path holds.
///
/// One enum rather than two command sets: a disk and a tape archive answer the
/// same two questions for a pane — what is in you, and give me that file's
/// bytes — and the pane should not have to know which it opened to ask.
enum Container {
    Disk(D64Image, Vec<CbmEntry>),
    Tape(T64Archive),
}

impl Container {
    fn open(path: &std::path::Path) -> CoreResult<Self> {
        // A T64 says what it is; a disk image has no signature at all, so the
        // tape is asked first and the disk is what is left.
        match T64Archive::open(path) {
            Ok(tape) => Ok(Self::Tape(tape)),
            Err(_) => {
                let disk = D64Image::open(path)?;
                let entries = disk.list()?;
                Ok(Self::Disk(disk, entries))
            }
        }
    }

    fn info(&self) -> CoreResult<CbmInfo> {
        Ok(match self {
            Self::Disk(disk, entries) => {
                let (name, id) = disk.disk_name()?;
                CbmInfo {
                    format: match disk.geometry().drive {
                        crate::core::cbm::geometry::Drive::D64 => "d64",
                        crate::core::cbm::geometry::Drive::D71 => "d71",
                        crate::core::cbm::geometry::Drive::D81 => "d81",
                    }
                    .to_string(),
                    volume_name: name,
                    disk_id: id,
                    entry_count: entries.len(),
                    notes: Vec::new(),
                }
            }
            Self::Tape(tape) => {
                let mut notes = Vec::new();
                if tape.header_disagreed {
                    notes.push(
                        "This tape archive's header disagrees with the records inside it. ART \
                         listed the records, which is what the files actually are."
                            .to_string(),
                    );
                }
                let repaired = tape
                    .entries()
                    .iter()
                    .filter(|e| e.length_was_repaired)
                    .count();
                if repaired > 0 {
                    notes.push(format!(
                        "{repaired} entries declare a length the file cannot hold; ART uses what \
                         is actually there."
                    ));
                }
                CbmInfo {
                    format: "t64".to_string(),
                    volume_name: tape.container_name().to_string(),
                    disk_id: String::new(),
                    entry_count: tape.entries().len(),
                    notes,
                }
            }
        })
    }

    fn rows(&self) -> Vec<PanelEntry> {
        match self {
            Self::Disk(_, entries) => entries
                .iter()
                .map(|entry| PanelEntry {
                    // The type is part of how a C64 user reads a directory —
                    // two files may differ only by it.
                    name: format!("{}  [{}]", entry.name, entry.file_type.as_str()),
                    is_dir: false,
                    bytes: entry.blocks as u64 * BYTES_PER_BLOCK,
                    path: None,
                    header_block: None,
                    iso_extent: None,
                    is_link: false,
                    date: None,
                    attrs: None,
                })
                .collect(),
            Self::Tape(tape) => tape
                .entries()
                .iter()
                .map(|entry| PanelEntry {
                    name: entry.name.clone(),
                    is_dir: false,
                    bytes: entry.bytes as u64,
                    path: None,
                    header_block: None,
                    iso_extent: None,
                    is_link: false,
                    date: None,
                    attrs: None,
                })
                .collect(),
        }
    }

    /// The bytes of the entry shown as `name`, and the name to write it under.
    ///
    /// Matched on the row's displayed name, because that is what the pane has
    /// and what the user picked. A C64 directory can hold two entries with the
    /// same name — the first match wins, which is also what a real drive does.
    fn read(&self, name: &str) -> CoreResult<(String, Vec<u8>)> {
        match self {
            Self::Disk(disk, entries) => {
                let entry = entries
                    .iter()
                    .find(|e| row_name(&e.name, Some(e.file_type.as_str())) == name)
                    .ok_or_else(|| not_found(name))?;
                Ok((
                    host_name(&entry.name, entry.file_type.as_str()),
                    disk.read_file(entry)?,
                ))
            }
            Self::Tape(tape) => {
                let entry: &T64Entry = tape
                    .entries()
                    .iter()
                    .find(|e| e.name == name)
                    .ok_or_else(|| not_found(name))?;
                Ok((host_name(&entry.name, "prg"), tape.read(entry)?))
            }
        }
    }

    fn names(&self) -> Vec<String> {
        match self {
            Self::Disk(_, entries) => entries
                .iter()
                .map(|e| row_name(&e.name, Some(e.file_type.as_str())))
                .collect(),
            Self::Tape(tape) => tape.entries().iter().map(|e| e.name.clone()).collect(),
        }
    }
}

/// How a row is labelled in the pane — the name, then the Commodore file type.
fn row_name(name: &str, file_type: Option<&str>) -> String {
    match file_type {
        Some(t) => format!("{name}  [{t}]"),
        None => name.to_string(),
    }
}

/// What the file is called once it reaches a Windows folder.
///
/// The Commodore type becomes an extension, because a folder full of files
/// with no extension and one called `SPACE  [PRG]` helps nobody. Characters
/// Windows refuses are replaced here, and `safe_join` still decides whether
/// the result may be written — this is presentation, not the security check.
fn host_name(name: &str, file_type: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| match c {
            '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*' => '_',
            c if (c as u32) < 0x20 => '_',
            c => c,
        })
        .collect();
    let trimmed = cleaned.trim().trim_end_matches('.');
    let stem = if trimmed.is_empty() {
        "unnamed"
    } else {
        trimmed
    };
    format!("{stem}.{}", file_type.to_lowercase())
}

fn not_found(name: &str) -> CoreError {
    CoreError::InvalidInput(format!("this image holds nothing called '{name}'"))
}

/// Open a Commodore disk or tape and report enough to show the pane.
#[tauri::command]
pub fn cbm_open(path: String) -> AppResult<CbmInfo> {
    let file = PathBuf::from(path.trim());
    Ok(Container::open(&file)?.info()?)
}

/// List the whole image — these media are flat, so there is only ever one
/// listing and no folder to ask for.
#[tauri::command]
pub fn cbm_list(path: String) -> AppResult<Vec<PanelEntry>> {
    let file = PathBuf::from(path.trim());
    Ok(Container::open(&file)?.rows())
}

/// The body of [`cbm_extract_file`], callable from a test.
fn extract_one(
    file: &std::path::Path,
    name: &str,
    dest_dir: &std::path::Path,
    overwrite: Option<bool>,
) -> CoreResult<super::panel::ExtractedTo> {
    let container = Container::open(file)?;
    let (host, data) = container.read(name)?;

    let target = crate::core::security::safe_join(dest_dir, &host).map_err(|err| {
        CoreError::SafetyRefused(format!("'{host}' cannot be written here: {err}"))
    })?;

    if target.exists() && !overwrite.unwrap_or(false) {
        return Ok(super::panel::ExtractedTo {
            bytes: std::fs::metadata(&target).map(|m| m.len()).unwrap_or(0),
            path: target.to_string_lossy().to_string(),
            skipped_existing: true,
        });
    }

    std::fs::create_dir_all(dest_dir)?;
    crate::core::safety::atomic::atomic_write(&target, &data)?;
    Ok(super::panel::ExtractedTo {
        path: target.to_string_lossy().to_string(),
        bytes: data.len() as u64,
        skipped_existing: false,
    })
}

/// Copy one file out of a Commodore image to a local folder.
#[tauri::command]
pub fn cbm_extract_file(
    path: String,
    name: String,
    dest_dir: String,
    overwrite: Option<bool>,
    oplog: State<'_, JsonlOperationLog>,
) -> AppResult<super::panel::ExtractedTo> {
    let file = PathBuf::from(path.trim());
    let destination = PathBuf::from(dest_dir.trim());

    let result =
        extract_one(&file, &name, &destination, overwrite).map_err(crate::error::AppError::from);

    write_result(
        &oplog,
        user_operation("Copy file out of a Commodore image")
            .source(format!("{path}:{name}"))
            .destination(destination.display().to_string()),
        &result,
        |record, extracted: &super::panel::ExtractedTo| {
            record
                .detail("Bytes", extracted.bytes.to_string())
                .outcome(OperationOutcome::verified(true))
        },
    );

    result
}

/// The whole of what [`cbm_extract`]'s job runs: write the rows `wanted`
/// names into `dest_dir`, or every row when it is empty.
///
/// Straight into the folder the user picked, with no subfolder of the image's
/// own name — the same shape a multi-selection copied out of an Amiga volume
/// takes, because it is the same gesture.
fn extract_all(
    file: &std::path::Path,
    dest_dir: &std::path::Path,
    wanted: &[String],
    policy: OverwritePolicy,
    progress: &dyn ProgressSink,
) -> CoreResult<ExtractOutcome> {
    let destination = dest_dir.to_path_buf();
    std::fs::create_dir_all(&destination)?;

    let container = Container::open(file)?;
    let all = container.names();
    let names: Vec<String> = if wanted.is_empty() {
        all
    } else {
        // Only rows this image actually has: a name the caller made up is
        // reported, not searched for on disk.
        wanted.to_vec()
    };
    let mut outcome = ExtractOutcome::default();

    for (index, row) in names.iter().enumerate() {
        if progress.is_cancelled() {
            return Err(crate::core::jobs::cancelled_error());
        }
        progress.report(index as u64, Some(names.len() as u64), row);

        let (host, data) = match container.read(row) {
            Ok(read) => read,
            Err(e) => {
                // One unreadable file does not end the copy: a disk with a
                // broken chain still has every other file on it, and saying
                // which one failed is more use than refusing the lot.
                outcome
                    .errors
                    .push(format!("'{row}' could not be read: {e}"));
                continue;
            }
        };

        let target = match crate::core::security::safe_join(&destination, &host) {
            Ok(target) => target,
            Err(e) => {
                outcome.errors.push(format!("'{host}' was refused: {e}"));
                continue;
            }
        };
        if target.exists() && policy == OverwritePolicy::Skip {
            outcome.skipped_existing += 1;
            continue;
        }

        crate::core::safety::atomic::atomic_write(&target, &data)?;
        outcome.total_files += 1;
        outcome.total_bytes += data.len() as u64;
    }

    Ok(outcome)
}

/// F5 out of a Commodore image with several rows picked: all of them, into
/// the folder the user chose, as one job.
///
/// An empty `names` means the whole image, which is what "select all then
/// copy" amounts to and what a caller with nothing selected means.
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub fn cbm_extract(
    path: String,
    names: Vec<String>,
    dest_dir: String,
    options: Option<CopyOptions>,
    app: AppHandle,
    registry: State<'_, Arc<JobRegistry>>,
    oplog: State<'_, JsonlOperationLog>,
) -> AppResult<JobId> {
    let file = PathBuf::from(path.trim());
    let destination = PathBuf::from(dest_dir.trim());
    let policy = options.unwrap_or_default().overwrite.unwrap_or_default();

    let log_path = oplog.path().to_path_buf();
    let registry = Arc::clone(&registry);
    let emit_app = app.clone();
    let title = format!("Copying out of {}", file.display());

    let id = spawn_job(&app, registry, &title, move |job_id, progress| {
        let outcome = extract_all(&file, &destination, &names, policy, progress);

        let record = user_operation("Copy files out of a Commodore image")
            .source(file.display().to_string())
            .destination(destination.display().to_string());
        let record = match &outcome {
            Ok(report) => record
                .detail("Files", report.total_files.to_string())
                .detail("Bytes", report.total_bytes.to_string())
                .outcome(OperationOutcome::verified(report.errors.is_empty())),
            Err(err) => record.failed(err),
        };
        super::oplog::write_to_path(&log_path, &record);

        let report = outcome?;
        let _ = emit_app.emit(
            VOLUME_WRITE_EVENT,
            VolumeWriteResult::ArchiveOut { job_id, report },
        );
        Ok(())
    });

    Ok(id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::cbm::d64::fixture::D64Builder;
    use crate::core::cbm::t64::fixture::{build, record};
    use crate::core::jobs::NoProgress;

    fn scratch(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "art-cbm-cmd-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn sample_disk(dir: &std::path::Path) -> PathBuf {
        let mut builder = D64Builder::new(35);
        builder.write_header("GAMES DISK", "01");
        builder.add_file("LOADER", b"loader bytes", &[(17, 0)]);
        builder.add_file("LEVEL 1", b"level one", &[(17, 1)]);
        let path = dir.join("games.d64");
        std::fs::write(&path, builder.build()).unwrap();
        path
    }

    #[test]
    fn a_disk_opens_and_lists_what_it_holds() {
        let dir = scratch("disk");
        let disk = sample_disk(&dir);

        let info = cbm_open(disk.to_string_lossy().to_string()).unwrap();
        assert_eq!(info.format, "d64");
        assert_eq!(info.volume_name, "GAMES DISK");
        assert_eq!(info.disk_id, "01");
        assert_eq!(info.entry_count, 2);
        assert!(info.notes.is_empty());

        let rows = cbm_list(disk.to_string_lossy().to_string()).unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].name, "LOADER  [PRG]", "the type is part of the row");
        assert!(!rows[0].is_dir, "a C64 directory is flat");
        assert_eq!(rows[0].bytes, 254, "one block, as the directory claims");

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_file_copies_out_with_its_type_as_an_extension() {
        let dir = scratch("one-file");
        let disk = sample_disk(&dir);
        let out = dir.join("out");
        std::fs::create_dir_all(&out).unwrap();

        let extracted = extract_one(&disk, "LOADER  [PRG]", &out, None).unwrap();
        assert!(!extracted.skipped_existing);
        assert_eq!(std::fs::read(&extracted.path).unwrap(), b"loader bytes");
        assert!(extracted.path.ends_with("LOADER.prg"), "{}", extracted.path);

        // A second run leaves the first copy standing (SAFE_CREATE).
        let again = extract_one(&disk, "LOADER  [PRG]", &out, None).unwrap();
        assert!(again.skipped_existing);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn the_whole_disk_copies_out_into_a_folder_of_its_own() {
        let dir = scratch("whole");
        let disk = sample_disk(&dir);
        let out = dir.join("out");

        let report = extract_all(&disk, &out, &[], OverwritePolicy::Skip, &NoProgress).unwrap();

        assert_eq!(report.total_files, 2, "{report:?}");
        assert_eq!(
            std::fs::read(out.join("LEVEL 1.prg")).unwrap(),
            b"level one"
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    /// Only the rows the user picked, and a made-up name is reported rather
    /// than looked for on the disk.
    #[test]
    fn a_selection_copies_out_exactly_what_was_named() {
        let dir = scratch("selection");
        let disk = sample_disk(&dir);
        let out = dir.join("out");

        let report = extract_all(
            &disk,
            &out,
            &["LEVEL 1  [PRG]".to_string(), "NO SUCH FILE".to_string()],
            OverwritePolicy::Skip,
            &NoProgress,
        )
        .unwrap();

        assert_eq!(report.total_files, 1);
        assert!(out.join("LEVEL 1.prg").exists());
        assert!(!out.join("LOADER.prg").exists(), "not picked");
        assert_eq!(report.errors.len(), 1, "{:?}", report.errors);
        assert!(report.errors[0].contains("NO SUCH FILE"));

        std::fs::remove_dir_all(&dir).ok();
    }

    /// One broken file does not cost the user the rest of the disk.
    #[test]
    fn a_file_that_cannot_be_read_is_reported_and_the_others_still_land() {
        let dir = scratch("broken");
        let mut builder = D64Builder::new(35);
        builder.add_file("GOOD", b"fine", &[(17, 0)]);
        builder.add_directory_entry(
            "BROKEN",
            crate::core::cbm::d64::FileType::Prg,
            (99, 0), // a track this disk does not have
            1,
        );
        let disk = dir.join("mixed.d64");
        std::fs::write(&disk, builder.build()).unwrap();
        let out = dir.join("out");

        let report = extract_all(&disk, &out, &[], OverwritePolicy::Skip, &NoProgress).unwrap();

        assert_eq!(report.total_files, 1);
        assert_eq!(report.errors.len(), 1, "{:?}", report.errors);
        assert!(report.errors[0].contains("BROKEN"), "{:?}", report.errors);
        assert!(out.join("GOOD.prg").exists());

        std::fs::remove_dir_all(&dir).ok();
    }

    /// A tape archive opens through the same commands, and a header that
    /// disagrees with its records is reported rather than obeyed.
    #[test]
    fn a_tape_archive_opens_and_says_when_its_header_was_wrong() {
        let dir = scratch("tape");
        let tape = dir.join("games.t64");
        // `used = 0` with a real record: the common broken T64.
        std::fs::write(&tape, build(&[record("GAME", 0x0801, b"tape bytes")], 0)).unwrap();

        let info = cbm_open(tape.to_string_lossy().to_string()).unwrap();
        assert_eq!(info.format, "t64");
        assert_eq!(info.entry_count, 1);
        assert!(
            info.notes.iter().any(|n| n.contains("header disagrees")),
            "{:?}",
            info.notes
        );

        let rows = cbm_list(tape.to_string_lossy().to_string()).unwrap();
        assert_eq!(rows[0].name, "GAME");

        let out = dir.join("out");
        std::fs::create_dir_all(&out).unwrap();
        let extracted = extract_one(&tape, "GAME", &out, None).unwrap();
        assert_eq!(std::fs::read(&extracted.path).unwrap(), b"tape bytes");

        std::fs::remove_dir_all(&dir).ok();
    }

    /// A name a Windows folder cannot hold is made safe, and never by
    /// silently dropping the file.
    #[test]
    fn a_name_windows_refuses_still_lands_under_a_usable_one() {
        assert_eq!(host_name("LOAD*ME", "PRG"), "LOAD_ME.prg");
        assert_eq!(host_name("A:B", "SEQ"), "A_B.seq");
        assert_eq!(host_name("   ", "PRG"), "unnamed.prg");
        assert_eq!(host_name("DOTS...", "PRG"), "DOTS.prg");
    }
}
