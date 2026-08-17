//! Game index Tauri commands (SD-2 · G10).
//!
//! A thin adapter, as CLAUDE.md requires: deserialize, call core, serialize
//! back. The scanning, the four readers and every decision about which source
//! wins live in `core/gameindex`.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};

use super::jobs::{spawn_job, JobRegistry};
use super::oplog::{user_operation, write_result};
use crate::core::error::CoreError;
use crate::core::gameindex::cleanup;
use crate::core::gameindex::scan::{scan_titles_with, CatalogueEntry};
use crate::core::gameindex::store;
use crate::core::jobs::JobId;
use crate::core::oplog::{JsonlOperationLog, OperationOutcome};
use crate::error::{AppError, AppResult};

/// Where the catalogue lives.
///
/// **Resolved here and nowhere else.** `core/gameindex/store` takes the
/// directory as an argument because `core/` is platform-independent and
/// `%APPDATA%` is not. The temp-directory fallback is the same one `lib.rs`
/// uses for the software catalog, and for the same reason: a catalogue ART
/// cannot place is recoverable with one rescan, refusing to start is not.
fn catalogue_dir(app: &AppHandle) -> PathBuf {
    app.path()
        .app_data_dir()
        .unwrap_or_else(|_| std::env::temp_dir())
        .join("catalogue")
}

/// A timestamp for `scanned_at`.
///
/// `core` has no clock, so the command layer supplies one — the same split
/// `CardManifest::built_at` uses. Seconds since the epoch rather than a
/// formatted date: ART has no date library and the screen can format it.
fn now_stamp() -> Option<String> {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now().duration_since(UNIX_EPOCH).ok()?.as_secs();
    Some(secs.to_string())
}

/// Emitted when an index job finishes with its catalogue.
pub const INDEX_RESULT_EVENT: &str = "gameindex-result";

/// Emitted when a root has been refreshed. The screen reloads on it.
pub const REFRESHED_EVENT: &str = "catalogue-refreshed";

/// Which root a finished refresh was for.
#[derive(Debug, Clone, Serialize)]
pub struct RefreshedRoot {
    pub job_id: JobId,
    pub root: String,
}

/// The saved catalogue, with the user's own corrections applied.
///
/// **Starts nothing.** Opening the Collection screen calls this and no more;
/// scanning is an explicit ask.
#[tauri::command]
pub fn catalogue_load(app: AppHandle) -> AppResult<Vec<store::RootView>> {
    Ok(store::load(&catalogue_dir(&app))?)
}

#[tauri::command]
pub fn catalogue_add_root(root: String, app: AppHandle) -> AppResult<()> {
    Ok(store::add_root(&catalogue_dir(&app), Path::new(&root))?)
}

#[tauri::command]
pub fn catalogue_remove_root(root: String, app: AppHandle) -> AppResult<()> {
    Ok(store::remove_root(&catalogue_dir(&app), Path::new(&root))?)
}

/// Refresh one root on a job.
///
/// `mode` is `"update"` or `"rescan"`; anything else is refused rather than
/// guessed at — a third word arriving from the screen would otherwise silently
/// become whichever branch the `match` fell through to.
#[tauri::command]
pub fn catalogue_refresh(
    root: String,
    mode: String,
    app: AppHandle,
    registry: State<'_, Arc<JobRegistry>>,
) -> AppResult<JobId> {
    let refresh = match mode.as_str() {
        "update" => store::Refresh::Update,
        "rescan" => store::Refresh::Rescan,
        other => {
            return Err(crate::core::error::CoreError::InvalidInput(format!(
                "'{other}' is not a refresh mode"
            ))
            .into())
        }
    };

    let dir = catalogue_dir(&app);
    let root_path = PathBuf::from(&root);
    let registry = Arc::clone(&registry);
    let emit_app = app.clone();
    let stamped = now_stamp();

    let id = spawn_job(
        &app,
        registry,
        "Refreshing the catalogue",
        move |job_id, progress| {
            store::refresh_root(&dir, &root_path, refresh, stamped, progress)?;
            // The screen reloads the whole catalogue rather than patching one
            // root: the user layer and availability both apply across roots, and
            // one reload is cheaper than keeping two views in step.
            let _ = emit_app.emit(REFRESHED_EVENT, RefreshedRoot { job_id, root });
            Ok(())
        },
    );

    Ok(id)
}

/// Record — or clear — one title's hand corrections.
///
/// Returns where the previous overrides were backed up, which the screen
/// surfaces the way every mutating command in ART does.
#[tauri::command]
pub fn catalogue_set_override(
    id: String,
    edit: store::RecordOverride,
    app: AppHandle,
) -> AppResult<Option<String>> {
    let backup = store::set_override(&catalogue_dir(&app), &id, edit)?;
    Ok(backup.map(|path| path.to_string_lossy().to_string()))
}

/// What ART would propose for one entry's name, if anything.
///
/// Both fields are `None` far more often than not, and that is the point: the
/// screen shows a button only where there is something to propose. A tool that
/// offers to fix a name already right teaches people to click without reading.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NameSuggestion {
    /// A cleaner catalogue title. Applying it is a user override.
    pub title: Option<String>,
    /// A cleaner filename, extension included. Applying it renames a real file.
    pub file_name: Option<String>,
}

/// What ART would propose for these paths, one answer each, in order.
///
/// Read-only and free: nothing is applied, nothing is written, and the screen
/// can ask about a whole library without a confirmation in sight.
/// **Every title is asked about at once, and that is the point.** `dune2-2` is
/// Dune II's second disk, but nothing in that name says so — `dune2` itself
/// ends in a digit. What settles it is `dune2-1` lying beside it, so the whole
/// list decides and no single name is guessed at.
#[tauri::command]
pub fn name_suggestions(paths: Vec<String>, titles: Vec<String>) -> Vec<NameSuggestion> {
    let sets = cleanup::disk_sets(&titles);

    paths
        .iter()
        .zip(titles.iter())
        .map(|(path, title)| {
            let path = Path::new(path);
            let stem = path
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_default();
            let extension = path
                .extension()
                .map(|e| format!(".{}", e.to_string_lossy()))
                .unwrap_or_default();

            NameSuggestion {
                title: cleanup::suggest_in_set(title, &sets),
                file_name: cleanup::suggest_stem(&stem).map(|s| format!("{s}{extension}")),
            }
        })
        .collect()
}

/// Rename a title's file on disk.
///
/// This is the half of the tool that changes the user's data, so it behaves
/// like every other mutating command in ART:
///
/// - the new name is a **file name**, never a path — a caller cannot move a
///   file into another folder, or out of one, by renaming it;
/// - an existing target is **refused rather than replaced** (`SAFE_CREATE`);
/// - the operation is logged, success or failure.
///
/// The catalogue is not rewritten here. An entry's id is derived from its
/// content, so the next refresh recognises the file at its new path as the same
/// title rather than losing one and finding another — which is the behaviour a
/// rename on screen already proved.
#[tauri::command]
pub fn rename_title_file(
    path: String,
    new_name: String,
    oplog: State<'_, JsonlOperationLog>,
) -> AppResult<String> {
    let result = rename_on_disk(&path, &new_name).map_err(AppError::from);

    write_result(
        &oplog,
        user_operation("Rename a title's file")
            .source(&path)
            .detail("New name", &new_name),
        &result,
        |record, renamed: &String| {
            record
                .destination(renamed)
                .outcome(OperationOutcome::verified(true))
        },
    );

    result
}

fn rename_on_disk(path: &str, new_name: &str) -> Result<String, CoreError> {
    let from = Path::new(path);
    let trimmed = new_name.trim();

    if trimmed.is_empty() {
        return Err(CoreError::InvalidInput("the new name is empty".into()));
    }
    // A name, not a path. Anything that could climb out of the folder is
    // refused here rather than sanitised into something the user did not ask
    // for.
    if trimmed
        != Path::new(trimmed)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
    {
        return Err(CoreError::InvalidInput(format!(
            "'{trimmed}' is a path, not a file name"
        )));
    }

    let parent = from.parent().ok_or_else(|| {
        CoreError::InvalidInput(format!("'{path}' has no folder to rename inside"))
    })?;
    let to = parent.join(trimmed);

    if !from.is_file() {
        return Err(CoreError::InvalidInput(format!("'{path}' is not a file")));
    }
    // SAFE_CREATE: refuse when the target exists rather than replacing it.
    // Renaming one title over another would destroy the second, and the user
    // asked to tidy a name, not to lose a disk.
    if to.exists() && to != from {
        return Err(CoreError::InvalidInput(format!(
            "'{trimmed}' already exists in that folder"
        )));
    }

    std::fs::rename(from, &to)?;
    Ok(to.to_string_lossy().to_string())
}

#[cfg(test)]
mod rename_tests {
    use super::rename_on_disk;

    fn tempdir(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("art-rename-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn a_file_is_renamed_and_its_bytes_are_untouched() {
        let dir = tempdir("ok");
        let from = dir.join("ADPro_D1.adf");
        std::fs::write(&from, b"DISKDATA").unwrap();

        let to = rename_on_disk(from.to_str().unwrap(), "ADPro (Disk 1).adf").unwrap();

        assert!(!from.exists());
        assert_eq!(std::fs::read(&to).unwrap(), b"DISKDATA");
        assert_eq!(
            std::path::Path::new(&to).parent(),
            Some(dir.as_path()),
            "the file must stay in its folder"
        );
    }

    /// SAFE_CREATE. Renaming one title over another would destroy the second,
    /// and the user asked to tidy a name, not to lose a disk.
    #[test]
    fn an_existing_target_is_refused_and_both_files_survive() {
        let dir = tempdir("exists");
        let from = dir.join("A-Train Disk 1.adf");
        let occupied = dir.join("A-Train (Disk 1).adf");
        std::fs::write(&from, b"ONE").unwrap();
        std::fs::write(&occupied, b"TWO").unwrap();

        let refused = rename_on_disk(from.to_str().unwrap(), "A-Train (Disk 1).adf");

        assert!(refused.is_err());
        assert_eq!(std::fs::read(&from).unwrap(), b"ONE");
        assert_eq!(std::fs::read(&occupied).unwrap(), b"TWO");
    }

    /// A name, not a path. A rename must not be able to move a file somewhere
    /// else — least of all somewhere above its own folder.
    #[test]
    fn a_name_that_is_really_a_path_is_refused() {
        let dir = tempdir("traversal");
        let from = dir.join("game.adf");
        std::fs::write(&from, b"X").unwrap();

        for attempt in [
            "../escaped.adf",
            "..\\escaped.adf",
            "sub/escaped.adf",
            "C:/Windows/escaped.adf",
        ] {
            let refused = rename_on_disk(from.to_str().unwrap(), attempt);
            assert!(refused.is_err(), "{attempt:?} was accepted");
        }
        assert!(from.is_file(), "the original must be where it was");
    }

    #[test]
    fn an_empty_name_is_refused() {
        let dir = tempdir("empty");
        let from = dir.join("game.adf");
        std::fs::write(&from, b"X").unwrap();
        assert!(rename_on_disk(from.to_str().unwrap(), "   ").is_err());
        assert!(from.is_file());
    }

    #[test]
    fn a_missing_file_is_refused_rather_than_creating_one() {
        let dir = tempdir("missing");
        let from = dir.join("not-here.adf");
        assert!(rename_on_disk(from.to_str().unwrap(), "renamed.adf").is_err());
        assert!(!dir.join("renamed.adf").exists());
    }
}

/// What a finished index delivers to the UI.
#[derive(Debug, Clone, Serialize)]
pub struct IndexResult {
    pub job_id: JobId,
    pub dir_path: String,
    pub entries: Vec<CatalogueEntry>,
}

/// Start indexing a folder of Amiga titles.
///
/// Returns a job id immediately. Indexing reads inside every hardfile and
/// hashes every file, which for the collection this was built against is 1699
/// files and 3.74 GB — firmly §54/§55 territory, and the reason the user needs
/// a Stop rather than a spinner.
#[tauri::command]
pub fn gameindex_scan(
    dir_path: String,
    app: AppHandle,
    registry: State<'_, Arc<JobRegistry>>,
) -> AppResult<JobId> {
    let dir = PathBuf::from(&dir_path);
    let registry = Arc::clone(&registry);
    let emit_app = app.clone();

    let id = spawn_job(
        &app,
        registry,
        "Indexing titles",
        move |job_id, progress| {
            let entries = scan_titles_with(&dir, progress)?;
            let _ = emit_app.emit(
                INDEX_RESULT_EVENT,
                IndexResult {
                    job_id,
                    dir_path,
                    entries,
                },
            );
            Ok(())
        },
    );

    Ok(id)
}
