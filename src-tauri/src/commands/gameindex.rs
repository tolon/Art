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
use crate::core::gameindex::scan::{scan_titles_with, CatalogueEntry};
use crate::core::gameindex::store;
use crate::core::jobs::JobId;
use crate::error::AppResult;

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
