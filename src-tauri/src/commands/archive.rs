//! Opening an archive in the commander (Task 4 Step 4).
//!
//! Thin adapters over `core::archive`, and the same shape `commands::iso` has
//! for a disc — deliberately, because an archive and a CD are the same thing
//! to a commander: a container you walk into, list, and copy out of.
//!
//! An archive is **read-only** here, in every direction. There is no command
//! in this module that changes one, ever. The two directions that exist both
//! write somewhere that is not the archive: `archive_extract` to a local
//! folder, `archive_copy_to_volume` into an Amiga volume.
//!
//! ## What the pane sees is not what the file holds
//!
//! An archive stores a flat list of names like `Tools/Shell.lha`; the folders
//! exist only because those names say so. [`core::archive::tree`] does that
//! translation, and it refuses to *show* a name that is not a plain relative
//! path — `../escape`, `C:\…`, a name that is only separators. Those entries
//! are counted and reported rather than hidden, so a listing that shows seven
//! of ten entries says so.
//!
//! Nothing in that display is trusted afterwards. Every byte still leaves
//! through `core::archive::extract`, where `safe_join` decides what a name
//! means on disk.

use std::path::PathBuf;
use std::sync::Arc;

use serde::Serialize;
use tauri::{AppHandle, Emitter, State};

use super::jobs::{spawn_job, JobRegistry};
use super::oplog::{user_operation, write_result};
use super::panel::PanelEntry;
use super::volume_write::{
    folder_destination, run_copy_in_folder_with, CopyOptions, OnCancel, VolumeWriteResult,
    VOLUME_WRITE_EVENT,
};
use crate::core::archive::extract::{extract_selection, ExtractOutcome, Wanted};
use crate::core::archive::tree::ArchiveTree;
use crate::core::archive::{ArchiveBackend, ArchiveEntry};
use crate::core::error::{CoreError, CoreResult};
use crate::core::jobs::{JobId, ProgressSink};
use crate::core::oplog::{JsonlOperationLog, OperationOutcome};
use crate::core::volume::write::copy::{HostFolder, OverwritePolicy};
use crate::error::AppResult;

/// What opening an archive reports: enough to show the pane, and enough to be
/// honest about what it is not showing.
#[derive(Debug, Clone, Serialize)]
pub struct ArchiveInfo {
    /// `"lha"`, `"zip"` or `"7z"` — from the file's own bytes, not its name.
    pub format: String,
    pub entry_count: usize,
    pub total_bytes: u64,
    /// Entries whose names are not paths, so the pane cannot show them.
    pub unusable_names: usize,
    /// Entries dropped because another had claimed that name already.
    pub duplicates: usize,
}

/// Open an archive, list a directory, walk a subtree — all three need the
/// backend and the tree together, and both come from the file each time.
///
/// Rebuilt per call, like `adf_open`/`adf_list` and `iso_open`/`iso_list`
/// before it. Reading an archive's directory is cheap next to decompressing
/// it, and a cached tree would have to be invalidated by something.
fn open_tree(
    path: &std::path::Path,
) -> CoreResult<(Box<dyn ArchiveBackend>, Vec<ArchiveEntry>, ArchiveTree)> {
    let mut backend = crate::core::archive::open(path)?;
    let entries = backend.entries()?;
    let tree = ArchiveTree::build(&entries);
    Ok((backend, entries, tree))
}

/// The folder inside the archive that `dir` + `name` names.
///
/// Joined here rather than by the caller, for the same reason
/// `folder_destination` joins a host path here: a name that came out of an
/// archive is a name ART did not write.
fn child_of(dir: &str, name: &str) -> String {
    let dir = dir.trim_matches('/');
    if dir.is_empty() {
        name.to_string()
    } else {
        format!("{dir}/{name}")
    }
}

/// Open an archive and report enough to show the pane.
#[tauri::command]
pub fn archive_open(path: String) -> AppResult<ArchiveInfo> {
    let file = PathBuf::from(path.trim());
    let (backend, entries, tree) = open_tree(&file)?;

    Ok(ArchiveInfo {
        format: backend.format().to_string(),
        entry_count: entries.len(),
        total_bytes: entries.iter().map(|e| e.declared_bytes).sum(),
        unusable_names: tree.refused().len(),
        duplicates: tree.duplicates(),
    })
}

/// List one folder of an archive as panel rows.
///
/// `dir` is `""` for the root, or a slash-separated path from a previous
/// listing. A folder that is not in the tree is an error rather than an empty
/// listing — an empty listing would be indistinguishable from an empty folder.
#[tauri::command]
pub fn archive_list(path: String, dir: String) -> AppResult<Vec<PanelEntry>> {
    let file = PathBuf::from(path.trim());
    let (_, _, tree) = open_tree(&file)?;

    let rows = tree.list(&dir).ok_or_else(|| {
        CoreError::InvalidInput(format!("this archive has no folder called '{dir}'"))
    })?;

    Ok(rows
        .iter()
        .map(|row| PanelEntry {
            name: row.name.clone(),
            is_dir: row.is_dir,
            bytes: row.bytes,
            path: None,
            header_block: None,
            iso_extent: None,
            is_link: false,
            // An archive's timestamps are three different formats across the
            // three backends and none of them is checked against anything, so
            // ART shows no date rather than a wrong one (§10, §89).
            date: None,
            attrs: None,
        })
        .collect())
}

/// The body of [`archive_extract_file`], without the `State` a Tauri command
/// needs, so a test can call it directly.
fn extract_one(
    file: &std::path::Path,
    dir: &str,
    name: &str,
    destination: &std::path::Path,
    overwrite: Option<bool>,
) -> CoreResult<super::panel::ExtractedTo> {
    let (mut backend, entries, tree) = open_tree(file)?;
    let index = tree.index_of(dir, name).ok_or_else(|| {
        CoreError::InvalidInput(format!(
            "this archive has no file called '{name}' in '{dir}'"
        ))
    })?;

    let target = crate::core::security::safe_join(destination, name).map_err(|err| {
        CoreError::SafetyRefused(format!("'{name}' cannot be written here: {err}"))
    })?;

    if target.exists() && !overwrite.unwrap_or(false) {
        return Ok(super::panel::ExtractedTo {
            bytes: std::fs::metadata(&target).map(|m| m.len()).unwrap_or(0),
            path: target.to_string_lossy().to_string(),
            skipped_existing: true,
        });
    }

    // Through the gate, not around it: one file is still an entry whose
    // declared size is a claim and whose name is not a path until `safe_join`
    // says so.
    let selection = [Wanted {
        index,
        name: name.to_string(),
    }];
    let outcome = extract_selection(
        &mut *backend,
        &entries,
        &selection,
        destination,
        if overwrite.unwrap_or(false) {
            OverwritePolicy::Overwrite
        } else {
            OverwritePolicy::Skip
        },
        &crate::core::jobs::NoProgress,
    )?;

    if outcome.total_files == 0 {
        let reason = outcome
            .errors
            .first()
            .cloned()
            .unwrap_or_else(|| format!("'{name}' was not written"));
        return Err(CoreError::SafetyRefused(reason));
    }

    Ok(super::panel::ExtractedTo {
        path: target.to_string_lossy().to_string(),
        bytes: outcome.total_bytes,
        skipped_existing: false,
    })
}

/// Copy one file out of an archive to a local folder — the single-entry fast
/// path of F5, the same asymmetry `volume_extract_to` and `iso_extract_file`
/// give a volume and a disc.
#[tauri::command]
pub fn archive_extract_file(
    path: String,
    dir: String,
    name: String,
    dest_dir: String,
    overwrite: Option<bool>,
    oplog: State<'_, JsonlOperationLog>,
) -> AppResult<super::panel::ExtractedTo> {
    let file = PathBuf::from(path.trim());
    let destination = PathBuf::from(dest_dir.trim());

    let result = extract_one(&file, &dir, &name, &destination, overwrite)
        .map_err(crate::error::AppError::from);

    write_result(
        &oplog,
        user_operation("Copy file out of an archive")
            .source(format!("{path}:{}", child_of(&dir, &name)))
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

/// The whole of what [`archive_extract`]'s job runs: resolve the destination
/// from `dest_dir` + `name`, walk the subtree, and extract it through the gate.
///
/// Its own function so a test can call exactly what the command runs — the
/// same reason `copy_out_folder` and `copy_out_tree` are.
fn copy_out_folder(
    file: &std::path::Path,
    dir: &str,
    name: &str,
    dest_dir: &std::path::Path,
    policy: OverwritePolicy,
    progress: &dyn ProgressSink,
) -> CoreResult<ExtractOutcome> {
    // Before the archive is opened: a name that cannot be written is a
    // refusal, not work done and then thrown away.
    let destination = folder_destination(dest_dir, name)?;

    let (mut backend, entries, tree) = open_tree(file)?;
    let inside = child_of(dir, name);
    if !tree.has_dir(&inside) {
        return Err(CoreError::InvalidInput(format!(
            "this archive has no folder called '{inside}'"
        )));
    }

    let selection: Vec<Wanted> = tree
        .subtree(&inside)
        .into_iter()
        .map(|(index, name)| Wanted { index, name })
        .collect();

    extract_selection(
        &mut *backend,
        &entries,
        &selection,
        &destination,
        policy,
        progress,
    )
}

/// F5 out of an archive, to a local folder — a job, because an archive's tree
/// can be thousands of files (§54, §55).
///
/// `dest_dir` and `name` are joined *here*, by `folder_destination`, never by
/// the caller.
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub fn archive_extract(
    path: String,
    dir: String,
    name: String,
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
        let outcome = copy_out_folder(&file, &dir, &name, &destination, policy, progress);

        let record = user_operation("Copy folder out of an archive")
            .source(format!("{}:{}", file.display(), child_of(&dir, &name)))
            .destination(format!("{}/{name}", destination.display()));
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

/// F5 out of an archive, the other way — into an Amiga volume.
///
/// Unpacks the chosen subtree into a scratch folder and hands that folder to
/// the Stage W copy engine, which is exactly what installing a downloaded
/// package already does (`core::sources::install`). An unpacked archive is a
/// folder, and copying a folder into a volume is a tested operation — so
/// there is no second copy engine here either.
// Eight arguments because both ends of the copy have to be named: three for
// which part of which archive, three for which folder of which volume of which
// image. Folding either end into a struct would move the same fields one level
// down and make the call site longer, not shorter — the same judgement
// `volume_copy_out` records above its own allow.
#[allow(clippy::too_many_arguments)]
fn copy_into_volume(
    file: &std::path::Path,
    dir: &str,
    name: &str,
    image: &std::path::Path,
    volume_index: usize,
    parent: u32,
    policy: OverwritePolicy,
    progress: &dyn ProgressSink,
) -> CoreResult<(crate::core::volume::write::copy::CopyReport, Option<String>)> {
    let scratch = crate::core::sources::install::Scratch::new()?;

    let (mut backend, entries, tree) = open_tree(file)?;
    let inside = child_of(dir, name);

    // A folder copies its subtree; a file copies exactly itself.
    let selection: Vec<Wanted> = if tree.has_dir(&inside) {
        tree.subtree(&inside)
            .into_iter()
            .map(|(index, name)| Wanted { index, name })
            .collect()
    } else {
        let index = tree.index_of(dir, name).ok_or_else(|| {
            CoreError::InvalidInput(format!("this archive holds nothing called '{inside}'"))
        })?;
        vec![Wanted {
            index,
            name: name.to_string(),
        }]
    };

    let unpacked = extract_selection(
        &mut *backend,
        &entries,
        &selection,
        scratch.path(),
        OverwritePolicy::Overwrite,
        progress,
    )?;
    if unpacked.aborted {
        return Err(CoreError::SafetyRefused(
            unpacked
                .abort_reason
                .unwrap_or_else(|| "the archive was refused".into()),
        ));
    }

    // Sidecars on: an archive may carry `.uaem` files, and a WHDLoad slave's
    // protection bits are the difference between a game that starts and one
    // that does not (§7.2) — the same choice `sources::install` makes.
    let folder = HostFolder::new(scratch.path(), true);
    run_copy_in_folder_with(
        image,
        volume_index,
        parent,
        &folder,
        policy,
        OnCancel::KeepWhatLanded,
        progress,
    )
}

/// F5 out of an archive, into an Amiga volume. Returns a job id.
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub fn archive_copy_to_volume(
    archive_path: String,
    dir: String,
    name: String,
    path: String,
    volume_index: usize,
    dir_block: Option<u32>,
    options: Option<CopyOptions>,
    app: AppHandle,
    registry: State<'_, Arc<JobRegistry>>,
    oplog: State<'_, JsonlOperationLog>,
) -> AppResult<JobId> {
    let source = PathBuf::from(archive_path.trim());
    let image = PathBuf::from(path.trim());
    let policy = options.unwrap_or_default().overwrite.unwrap_or_default();
    let parent = dir_block.unwrap_or(0);

    let log_path = oplog.path().to_path_buf();
    let registry = Arc::clone(&registry);
    let emit_app = app.clone();
    let title = format!("Copying an archive into {}", image.display());

    let id = spawn_job(&app, registry, &title, move |job_id, progress| {
        let outcome = copy_into_volume(
            &source,
            &dir,
            &name,
            &image,
            volume_index,
            parent,
            policy,
            progress,
        );

        let record = user_operation("Copy an archive into volume")
            .source(format!("{}:{}", source.display(), child_of(&dir, &name)))
            .destination(format!("{}:{volume_index}", image.display()));
        let record = match &outcome {
            Ok((report, _)) => record
                .detail("Files", report.files_copied.to_string())
                .detail("Folders", report.directories_created.to_string())
                .detail("Skipped", report.skipped.len().to_string())
                .outcome(OperationOutcome::verified(
                    report.files_verified == report.files_copied,
                )),
            Err(err) => record.failed(err),
        };
        super::oplog::write_to_path(&log_path, &record);

        let (report, backup) = outcome?;
        let _ = emit_app.emit(
            VOLUME_WRITE_EVENT,
            VolumeWriteResult::CopyIn {
                job_id,
                report,
                backup,
            },
        );
        Ok(())
    });

    Ok(id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::jobs::NoProgress;

    fn scratch(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "art-archive-cmd-{tag}-{}-{}",
            crate::core::test_scratch_id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// A ZIP is the cheapest of the three to build and the tree is format
    /// independent, so the command-layer tests use one. The formats
    /// themselves are covered together in `core::archive`.
    fn sample(dir: &std::path::Path) -> PathBuf {
        let archive = dir.join("pack.zip");
        std::fs::write(
            &archive,
            crate::core::archive::zip::tests::make_zip_with(&[
                ("ReadMe.txt", b"hello" as &[u8]),
                ("Tools/Shell.lha", b"shell"),
                ("Tools/Sub/Deep.txt", b"deep"),
            ]),
        )
        .unwrap();
        archive
    }

    #[test]
    fn opening_then_listing_walks_the_tree() {
        let dir = scratch("open");
        let archive = sample(&dir);
        let path = archive.to_string_lossy().to_string();

        let info = archive_open(path.clone()).unwrap();
        assert_eq!(info.format, "zip");
        assert_eq!(info.entry_count, 3);
        assert_eq!(info.unusable_names, 0);

        let root = archive_list(path.clone(), String::new()).unwrap();
        let names: Vec<&str> = root.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, ["Tools", "ReadMe.txt"], "folders first");
        assert!(root[0].is_dir);
        assert!(
            root[0].header_block.is_none() && root[0].iso_extent.is_none(),
            "an archive row borrows no other pane kind's address"
        );

        let tools = archive_list(path, "Tools".to_string()).unwrap();
        let names: Vec<&str> = tools.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, ["Sub", "Shell.lha"]);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn listing_a_folder_that_is_not_there_is_an_error_not_an_empty_pane() {
        let dir = scratch("no-folder");
        let archive = sample(&dir);

        let err =
            archive_list(archive.to_string_lossy().to_string(), "Nope".to_string()).unwrap_err();
        assert!(err.to_string().contains("no folder"), "{err}");

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_file_copies_out_on_its_own_and_a_second_run_leaves_it_alone() {
        let dir = scratch("one-file");
        let archive = sample(&dir);
        let out = dir.join("out");
        std::fs::create_dir_all(&out).unwrap();

        let first = extract_one(&archive, "Tools", "Shell.lha", &out, None).unwrap();
        assert!(!first.skipped_existing);
        assert_eq!(std::fs::read(&first.path).unwrap(), b"shell");
        assert!(
            !out.join("Sub").exists(),
            "one file, not the folder around it"
        );

        let second = extract_one(&archive, "Tools", "Shell.lha", &out, None).unwrap();
        assert!(second.skipped_existing, "SAFE_CREATE");
        assert_eq!(std::fs::read(&first.path).unwrap(), b"shell");

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_folder_copies_out_under_the_name_the_user_picked() {
        let dir = scratch("folder");
        let archive = sample(&dir);
        let out = dir.join("out");

        let report = copy_out_folder(
            &archive,
            "",
            "Tools",
            &out,
            OverwritePolicy::Skip,
            &NoProgress,
        )
        .unwrap();

        assert_eq!(report.total_files, 2, "{:?}", report.extracted);
        assert_eq!(
            std::fs::read(out.join("Tools").join("Shell.lha")).unwrap(),
            b"shell"
        );
        assert_eq!(
            std::fs::read(out.join("Tools").join("Sub").join("Deep.txt")).unwrap(),
            b"deep"
        );
        assert!(!out.join("Tools").join("ReadMe.txt").exists());

        std::fs::remove_dir_all(&dir).ok();
    }

    /// The destination is built from the folder the user picked and the row's
    /// own name, in Rust — so a name out of an archive cannot arrive
    /// pre-joined to somewhere else.
    #[test]
    fn a_name_that_leaves_the_chosen_folder_is_refused_before_anything_is_read() {
        let dir = scratch("escape");
        let archive = sample(&dir);
        let out = dir.join("out");
        std::fs::create_dir_all(&out).unwrap();

        let err = copy_out_folder(
            &archive,
            "",
            r"..\..\Startup",
            &out,
            OverwritePolicy::Skip,
            &NoProgress,
        )
        .unwrap_err();

        assert_eq!(err.code(), "ART-SAFETY-REFUSED", "{err}");
        assert!(!dir.join("Startup").exists());

        std::fs::remove_dir_all(&dir).ok();
    }

    /// The direction the feature exists for: a folder inside an archive
    /// reaching an Amiga volume, through the Stage W writer rather than
    /// anything new. A ZIP is used deliberately — the old path could only do
    /// this for LHA.
    #[test]
    fn a_folder_copies_from_an_archive_into_an_amiga_volume() {
        use crate::core::volume::fixture::ffs_volume;
        use crate::core::volume::DosType;

        let dir = scratch("into-volume");
        let archive = sample(&dir);
        let image = dir.join("disk.adf");
        let (bytes, _) = ffs_volume(1760, DosType::new(*b"DOS\x01"));
        std::fs::write(&image, &bytes).unwrap();

        let (report, _backup) = copy_into_volume(
            &archive,
            "",
            "Tools",
            &image,
            0,
            0,
            OverwritePolicy::Skip,
            &NoProgress,
        )
        .unwrap();

        assert_eq!(report.files_copied, 2, "{report:?}");
        assert_eq!(report.files_verified, 2, "each file is read back");

        // And the tree kept its shape inside the volume.
        let entry = crate::core::volume::mount::scan_image(&image)
            .unwrap()
            .volumes
            .remove(0);
        let (device, geometry) = crate::core::volume::mount::mount(&image, &entry).unwrap();
        let set = crate::core::volume::write::layout::BlockSet::new(geometry.block_size);
        let root = crate::core::volume::write::dir::entries_in(
            &device,
            &set,
            &geometry,
            geometry.root_block,
        )
        .unwrap();
        let names: Vec<&str> = root.iter().map(|e| e.name.as_str()).collect();
        assert!(names.contains(&"Shell.lha"), "{names:?}");
        assert!(names.contains(&"Sub"), "{names:?}");
        assert!(
            !names.contains(&"ReadMe.txt"),
            "only the chosen folder crosses: {names:?}"
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    /// A single file picked in an archive pane copies as that file, not as the
    /// folder around it — the same rule Task 3's review settled for a disc.
    #[test]
    fn a_single_file_copies_into_a_volume_on_its_own() {
        use crate::core::volume::fixture::ffs_volume;
        use crate::core::volume::DosType;

        let dir = scratch("one-into-volume");
        let archive = sample(&dir);
        let image = dir.join("disk.adf");
        let (bytes, _) = ffs_volume(1760, DosType::new(*b"DOS\x01"));
        std::fs::write(&image, &bytes).unwrap();

        let (report, _) = copy_into_volume(
            &archive,
            "Tools",
            "Shell.lha",
            &image,
            0,
            0,
            OverwritePolicy::Skip,
            &NoProgress,
        )
        .unwrap();

        assert_eq!(report.files_copied, 1, "{report:?}");

        std::fs::remove_dir_all(&dir).ok();
    }

    /// An archive whose names are hostile lists what it safely can and says
    /// how much it is not showing, rather than quietly holding entries back.
    #[test]
    fn unusable_names_are_counted_in_what_opening_reports() {
        let dir = scratch("hostile");
        let archive = dir.join("hostile.zip");
        std::fs::write(
            &archive,
            crate::core::archive::zip::tests::make_zip_with(&[
                ("../../outside.txt", b"escaped" as &[u8]),
                ("ok.txt", b"fine"),
            ]),
        )
        .unwrap();

        let info = archive_open(archive.to_string_lossy().to_string()).unwrap();
        assert_eq!(info.entry_count, 2);
        assert_eq!(info.unusable_names, 1);

        let rows = archive_list(archive.to_string_lossy().to_string(), String::new()).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].name, "ok.txt");

        std::fs::remove_dir_all(&dir).ok();
    }
}
