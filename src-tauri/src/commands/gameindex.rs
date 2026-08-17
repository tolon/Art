//! Game index Tauri commands (SD-2 · G10).
//!
//! A thin adapter, as CLAUDE.md requires: deserialize, call core, serialize
//! back. The scanning, the four readers and every decision about which source
//! wins live in `core/gameindex`.

use std::path::PathBuf;
use std::sync::Arc;

use serde::Serialize;
use tauri::{AppHandle, Emitter, State};

use super::jobs::{spawn_job, JobRegistry};
use crate::core::gameindex::scan::{scan_titles_with, CatalogueEntry};
use crate::core::jobs::JobId;
use crate::error::AppResult;

/// Emitted when an index job finishes with its catalogue.
pub const INDEX_RESULT_EVENT: &str = "gameindex-result";

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

    let id = spawn_job(&app, registry, "Indexing titles", move |job_id, progress| {
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
    });

    Ok(id)
}
