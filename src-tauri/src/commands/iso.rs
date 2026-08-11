//! Opening a disc in the commander (Task 3 brief).
//!
//! Thin adapters over `core::iso`, same as every other module here:
//! deserialize, call core, serialize back. The one thing worth stating up
//! front is what "thin" does *not* mean — a disc is read-only, so there is no
//! `iso_write` command of any kind, ever. The two directions that do exist
//! both write somewhere that is not the disc: `iso_extract` writes to a local
//! folder, `iso_copy_to_volume` writes into an Amiga volume through the
//! existing, tested copy engine (`core::iso::IsoSource` is the `CopySource`
//! that makes that possible — see its doc comment for why no second engine
//! was needed).
//!
//! ## Reopened per call, like every other image command
//!
//! `iso_open`/`iso_list`/`iso_extract` each open the file fresh, the same as
//! `commands::adf`'s `adf_open`/`adf_list` do for an ADF. `IsoImage` holds
//! only a path and a sector layout — opening it costs a handful of sector
//! reads, not a load of the disc — so there is nothing to cache.

use std::path::PathBuf;
use std::sync::Arc;

use serde::Serialize;
use tauri::{AppHandle, Emitter, State};

use super::jobs::{spawn_job, JobRegistry};
use super::oplog::{user_operation, write_result};
use super::panel::PanelEntry;
use super::volume_write::{
    run_copy_in_folder_with, CopyOptions, OnCancel, VolumeWriteResult, VOLUME_WRITE_EVENT,
};
use crate::core::error::CoreResult;
use crate::core::iso::{IsoImage, IsoSource, SectorLayout};
use crate::core::jobs::JobId;
use crate::core::oplog::{JsonlOperationLog, OperationOutcome};
use crate::error::AppResult;

/// Open a disc, working out its sector layout from where `CD001` sits when
/// `format_hint` is not one `core::detect` already reported for it.
///
/// The drop panel already knows the layout by the time a pane opens — its
/// `Detection::format_hint` is exactly what `SectorLayout::from_format_hint`
/// turns back into one — so a caller that has it should pass it rather than
/// asking ART to find `CD001` a second time. A caller that does not (a saved
/// path, a recent-files entry) gets the same self-probing `IsoImage::open`
/// every other command in this module uses.
fn open_image(path: &std::path::Path, format_hint: Option<&str>) -> CoreResult<IsoImage> {
    match format_hint.and_then(SectorLayout::from_format_hint) {
        Some(layout) => IsoImage::open_with_layout(path, layout),
        None => IsoImage::open(path),
    }
}

/// What opening a disc reports: enough to show the pane and to start
/// navigating it.
#[derive(Debug, Clone, Serialize)]
pub struct IsoInfo {
    pub volume_name: String,
    /// Whether the tree being read is the Joliet one — Power User Mode shows
    /// it, the same way the pane footer shows a volume's filesystem string.
    pub joliet: bool,
    /// The root directory's `(extent, length)`, ready to hand to
    /// [`iso_list`]. An ISO directory is addressed by this pair, not by a
    /// block number — there is no `dirBlock` here to overload.
    pub root_extent: u32,
    pub root_length: u32,
}

/// Open a disc and report enough to show the pane.
#[tauri::command]
pub fn iso_open(path: String, format_hint: Option<String>) -> AppResult<IsoInfo> {
    let file = PathBuf::from(path.trim());
    let image = open_image(&file, format_hint.as_deref())?;
    let (root_extent, root_length) = image.root();
    Ok(IsoInfo {
        volume_name: image.volume_name().to_string(),
        joliet: image.is_joliet(),
        root_extent,
        root_length,
    })
}

/// List one directory of a disc as panel rows.
///
/// `extent`/`length` come from [`iso_open`]'s `root_extent`/`root_length` or
/// from a previous listing's directory entry (`iso_extent`, `bytes`) —
/// never from a block number, because a disc does not have one.
#[tauri::command]
pub fn iso_list(path: String, extent: u32, length: u32) -> AppResult<Vec<PanelEntry>> {
    let file = PathBuf::from(path.trim());
    let image = IsoImage::open(&file)?;
    let entries = image.list(extent, length)?;

    Ok(entries
        .into_iter()
        .map(|entry| PanelEntry {
            name: entry.name,
            is_dir: entry.is_dir,
            bytes: entry.bytes,
            path: None,
            header_block: None,
            iso_extent: Some(entry.extent),
            is_link: false,
            date: entry.date,
            attrs: None,
        })
        .collect())
}

/// The body of [`iso_extract_file`], without the `State` a Tauri command
/// needs — so a test can call it directly rather than standing up a real
/// app just to reach a plain file read and write.
fn extract_file(
    file: &std::path::Path,
    extent: u32,
    bytes: u64,
    name: &str,
    destination: &std::path::Path,
    overwrite: Option<bool>,
) -> CoreResult<super::panel::ExtractedTo> {
    // Untrusted the moment it left the disc's own directory record and
    // crossed into a Tauri argument — the same round trip
    // `HostFolder::resolve` makes for a plan the frontend has seen.
    let target = crate::core::security::safe_join(destination, name).map_err(|err| {
        crate::core::error::CoreError::SafetyRefused(format!(
            "'{name}' cannot be written here: {err}"
        ))
    })?;

    if target.exists() && !overwrite.unwrap_or(false) {
        return Ok(super::panel::ExtractedTo {
            bytes: std::fs::metadata(&target).map(|m| m.len()).unwrap_or(0),
            path: target.to_string_lossy().to_string(),
            skipped_existing: true,
        });
    }

    let image = IsoImage::open(file)?;
    let data = image.read_file(extent, bytes)?;
    crate::core::safety::atomic::atomic_write(&target, &data)?;
    Ok(super::panel::ExtractedTo {
        path: target.to_string_lossy().to_string(),
        bytes: data.len() as u64,
        skipped_existing: false,
    })
}

/// Copy one file out of a disc to a local folder — the single-entry fast
/// path of F5, the same asymmetry `volume_extract_to`/`volume_copy_out` give
/// an ADF or HDF: a lone file copies straight through, synchronously, and
/// only a whole directory needs a job.
///
/// `name` comes from the listing (`iso_list`'s `PanelEntry.name`) rather
/// than being re-read here: unlike an Amiga volume's header block, a disc's
/// directory *record* — which is what would carry the name back — is not
/// addressable by `extent` alone once split from the listing that produced
/// it, so the caller passes the one it already has.
#[tauri::command]
pub fn iso_extract_file(
    path: String,
    extent: u32,
    bytes: u64,
    name: String,
    dest_dir: String,
    overwrite: Option<bool>,
    oplog: State<'_, JsonlOperationLog>,
) -> AppResult<super::panel::ExtractedTo> {
    let file = PathBuf::from(path.trim());
    let destination = PathBuf::from(dest_dir.trim());

    let result = extract_file(&file, extent, bytes, &name, &destination, overwrite)
        .map_err(crate::error::AppError::from);

    write_result(
        &oplog,
        user_operation("Copy file out of a disc")
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

/// F5 out of a disc, to a local folder — a job because a disc's directory
/// tree can be thousands of files (§54, §55).
///
/// `extent`/`bytes` name a directory the same way [`iso_list`] does; its
/// *contents* land inside `dest`, not a folder named after it, the same
/// shape `volume_copy_out` gives for an Amiga volume's `dir_block`. Reuses
/// [`VolumeWriteResult::CopyOut`] and [`VOLUME_WRITE_EVENT`] rather than
/// inventing a second event: the frontend's one `onVolumeWriteResult`
/// listener already knows how to read an `ExtractReport`.
#[tauri::command]
pub fn iso_extract(
    path: String,
    extent: u32,
    bytes: u64,
    dest: String,
    app: AppHandle,
    registry: State<'_, Arc<JobRegistry>>,
    oplog: State<'_, JsonlOperationLog>,
) -> AppResult<JobId> {
    let iso_path = PathBuf::from(path.trim());
    let destination = PathBuf::from(dest.trim());
    let length = bytes.min(u32::MAX as u64) as u32;

    let log_path = oplog.path().to_path_buf();
    let registry = Arc::clone(&registry);
    let emit_app = app.clone();
    let title = format!("Copying out of {}", iso_path.display());

    let id = spawn_job(&app, registry, &title, move |job_id, progress| {
        let outcome = (|| -> CoreResult<_> {
            let image = IsoImage::open(&iso_path)?;
            image.extract_tree(extent, length, &destination, progress)
        })();

        let record = user_operation("Copy folder out of a disc")
            .source(iso_path.display().to_string())
            .destination(destination.display().to_string());
        let record = match &outcome {
            Ok(report) => record
                .detail("Files", report.files_written.to_string())
                .detail("Folders", report.directories_created.to_string())
                .outcome(OperationOutcome::verified(report.is_complete())),
            Err(err) => record.failed(err),
        };
        super::oplog::write_to_path(&log_path, &record);

        let report = outcome?;
        let _ = emit_app.emit(
            VOLUME_WRITE_EVENT,
            VolumeWriteResult::CopyOut { job_id, report },
        );
        Ok(())
    });

    Ok(id)
}

/// F5 out of a disc, the other way — into an Amiga volume.
///
/// This is the whole point of the feature (Task 3 brief): an AmigaOS install
/// CD is only useful once its contents reach an HDF. [`IsoSource`] answers
/// `CopySource`'s three questions for the subtree at `extent`/`bytes`, and
/// [`run_copy_in_folder_with`] — the exact function `volume_copy_in` calls —
/// does the rest. No second copy engine; a disc is just another source.
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub fn iso_copy_to_volume(
    iso_path: String,
    extent: u32,
    bytes: u64,
    path: String,
    volume_index: usize,
    dir_block: Option<u32>,
    options: Option<CopyOptions>,
    app: AppHandle,
    registry: State<'_, Arc<JobRegistry>>,
    oplog: State<'_, JsonlOperationLog>,
) -> AppResult<JobId> {
    let source_iso = PathBuf::from(iso_path.trim());
    let image = PathBuf::from(path.trim());
    let length = bytes.min(u32::MAX as u64) as u32;
    let options = options.unwrap_or_default();
    let policy = options.overwrite.unwrap_or_default();
    let parent = dir_block.unwrap_or(0);

    let log_path = oplog.path().to_path_buf();
    let registry = Arc::clone(&registry);
    let emit_app = app.clone();
    let title = format!("Copying a disc into {}", image.display());

    let id = spawn_job(&app, registry, &title, move |job_id, progress| {
        let outcome = (|| -> CoreResult<_> {
            let disc = IsoImage::open(&source_iso)?;
            let source = IsoSource::new(disc, extent, length)?;
            // Same choice `volume_copy_in` makes for a plain host folder: a
            // cancel keeps whatever already landed rather than abandoning it.
            run_copy_in_folder_with(
                &image,
                volume_index,
                parent,
                &source,
                policy,
                OnCancel::KeepWhatLanded,
                progress,
            )
        })();

        let record = user_operation("Copy a disc into volume")
            .source(source_iso.display().to_string())
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
    use crate::core::iso::fixture::{dir, file, IsoBuilder};

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!(
            "art-iso-cmd-{name}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn sample_disc() -> Vec<u8> {
        IsoBuilder {
            volume: "AMIGA_TEST".to_string(),
            children: vec![
                file("README.TXT", "ReadMe.txt", b"hello"),
                dir("TOOLS", "Tools", vec![file("A.LHA", "A.lha", b"x")]),
            ],
            ..Default::default()
        }
        .build()
    }

    /// A path that is not an ISO at all is a readable error, not a panic —
    /// the routing case the brief asks for at the command layer.
    #[test]
    fn opening_a_file_that_is_not_a_disc_is_an_error() {
        let d = tmp("not-a-disc");
        let p = d.join("plain.iso");
        std::fs::write(&p, vec![0u8; 4096]).unwrap();

        let err = iso_open(p.to_string_lossy().to_string(), None).unwrap_err();
        assert!(err.to_string().to_lowercase().contains("iso9660"));

        std::fs::remove_dir_all(&d).ok();
    }

    /// A bad extent is an error rather than a panic — same case, for `iso_list`.
    #[test]
    fn listing_a_bad_extent_is_an_error_not_a_panic() {
        let d = tmp("bad-extent");
        let p = d.join("disc.iso");
        std::fs::write(&p, sample_disc()).unwrap();

        let err = iso_list(p.to_string_lossy().to_string(), 900_000, 2048).unwrap_err();
        assert!(err.to_string().contains("past the end"), "{err}");

        std::fs::remove_dir_all(&d).ok();
    }

    /// `iso_open` reports a root that `iso_list` can then read — the
    /// round-trip a pane makes on first opening a disc.
    #[test]
    fn opening_then_listing_the_root_returns_what_the_disc_holds() {
        let d = tmp("open-list");
        let p = d.join("disc.iso");
        std::fs::write(&p, sample_disc()).unwrap();
        let path = p.to_string_lossy().to_string();

        let info = iso_open(path.clone(), None).unwrap();
        assert_eq!(info.volume_name, "AMIGA_TEST");
        assert!(!info.joliet);

        let entries = iso_list(path, info.root_extent, info.root_length).unwrap();
        let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, ["README.TXT", "TOOLS"]);
        let tools = entries.iter().find(|e| e.name == "TOOLS").unwrap();
        assert!(tools.is_dir);
        assert!(
            tools.iso_extent.is_some(),
            "an ISO entry carries its extent"
        );
        assert!(
            tools.header_block.is_none(),
            "header_block stays ADF/HDF-only"
        );

        std::fs::remove_dir_all(&d).ok();
    }

    /// The single-file fast path reads a file's bytes out and writes them
    /// byte-for-byte, and a second run leaves the first copy alone rather
    /// than silently replacing it (`SAFE_CREATE`).
    #[test]
    fn extract_file_writes_one_file_and_refuses_to_overwrite_it_by_default() {
        let d = tmp("extract-file");
        let p = d.join("disc.iso");
        std::fs::write(&p, sample_disc()).unwrap();
        let out = d.join("out");
        std::fs::create_dir_all(&out).unwrap();

        let path = p.to_string_lossy().to_string();
        let info = iso_open(path.clone(), None).unwrap();
        let entries = iso_list(path, info.root_extent, info.root_length).unwrap();
        let readme = entries.iter().find(|e| e.name == "README.TXT").unwrap();

        let extracted = extract_file(
            &p,
            readme.iso_extent.unwrap(),
            readme.bytes,
            &readme.name,
            &out,
            None,
        )
        .unwrap();
        assert!(!extracted.skipped_existing);
        assert_eq!(std::fs::read(&extracted.path).unwrap(), b"hello");

        // A second run without `overwrite` leaves the first copy standing.
        let second = extract_file(
            &p,
            readme.iso_extent.unwrap(),
            readme.bytes,
            &readme.name,
            &out,
            None,
        )
        .unwrap();
        assert!(second.skipped_existing);
        assert_eq!(std::fs::read(&extracted.path).unwrap(), b"hello");

        std::fs::remove_dir_all(&d).ok();
    }

    /// A bad extent is an error rather than a panic, for the single-file
    /// path too.
    #[test]
    fn extract_file_reports_a_bad_extent_instead_of_panicking() {
        let d = tmp("extract-file-bad");
        let p = d.join("disc.iso");
        std::fs::write(&p, sample_disc()).unwrap();
        let out = d.join("out");
        std::fs::create_dir_all(&out).unwrap();

        let err = extract_file(&p, u32::MAX, 2048, "x.txt", &out, None).unwrap_err();
        assert_eq!(err.code(), "ART-FORMAT-MALFORMED");
        std::fs::remove_dir_all(&d).ok();
    }

    /// A format hint from detection opens the same disc `IsoImage::open`
    /// would probe for itself — the contract `open_image` exists to keep.
    #[test]
    fn a_format_hint_opens_the_same_disc_as_self_probing() {
        let d = tmp("hint");
        let p = d.join("disc.iso");
        std::fs::write(&p, sample_disc()).unwrap();
        let path = p.to_string_lossy().to_string();

        let probed = iso_open(path.clone(), None).unwrap();
        let hinted = iso_open(path, Some("iso9660".to_string())).unwrap();
        assert_eq!(probed.volume_name, hinted.volume_name);
        assert_eq!(probed.root_extent, hinted.root_extent);
        assert_eq!(probed.root_length, hinted.root_length);

        std::fs::remove_dir_all(&d).ok();
    }
}
