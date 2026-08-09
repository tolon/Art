//! Collection Organizer Tauri commands.

use std::path::PathBuf;
use std::sync::Arc;

use serde::Serialize;
use tauri::{AppHandle, Emitter, State};

use super::jobs::{spawn_job, JobRegistry};
use crate::core::collection::{scan_collection_directory_with, CollectionItem};
use crate::core::jobs::JobId;
use crate::error::AppResult;

/// Emitted when a scan job finishes with results.
pub const SCAN_RESULT_EVENT: &str = "collection-scan-result";

/// What a finished scan delivers to the UI.
#[derive(Debug, Clone, Serialize)]
pub struct ScanResult {
    pub job_id: JobId,
    pub dir_path: String,
    pub items: Vec<CollectionItem>,
}

/// Start scanning a directory for Amiga titles.
///
/// Returns a job id immediately. A collection can hold tens of thousands of
/// files, so the walk runs on a background thread and the UI follows it through
/// `job-progress` events (spec §54, §55). The titles themselves arrive in a
/// `collection-scan-result` event when the job finishes.
#[tauri::command]
pub fn collection_scan(
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
        "Scanning collection",
        move |job_id, progress| {
            let items = scan_collection_directory_with(&dir, progress)?;
            let _ = emit_app.emit(
                SCAN_RESULT_EVENT,
                ScanResult {
                    job_id,
                    dir_path,
                    items,
                },
            );
            Ok(())
        },
    );

    Ok(id)
}
