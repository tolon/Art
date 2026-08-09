//! LHA Studio commands: open / extract / WHDLoad detection.

use std::path::PathBuf;
use std::sync::Arc;

use serde::Serialize;
use tauri::{AppHandle, Emitter, State};

use super::jobs::{spawn_job, JobRegistry};
use super::oplog::{user_operation, write_result};
use crate::core::jobs::JobId;
use crate::core::lha::safe_extract::extract_archive_with;
use crate::core::lha::{
    detect_whdload, extract_archive, open_archive, ExtractOutcome, LhaInfo, OverwritePolicy,
};
use crate::core::oplog::{JsonlOperationLog, OperationOutcome};
use crate::core::workflow::types::Confidence;
use crate::error::AppResult;

/// Open an LHA archive and list entries.
#[tauri::command]
pub fn lha_open(path: String) -> AppResult<LhaInfo> {
    Ok(open_archive(&PathBuf::from(&path))?)
}

/// Extract an entire archive into `dest`.
///
/// Safe by construction: path traversal is rejected, output is capped against
/// decompression bombs, and existing files are left alone unless the caller
/// passes an explicit `overwrite` policy.
#[tauri::command]
pub fn lha_extract(
    path: String,
    dest: String,
    overwrite: Option<OverwritePolicy>,
    oplog: State<'_, JsonlOperationLog>,
) -> AppResult<ExtractOutcome> {
    let policy = overwrite.unwrap_or_default();
    let result =
        extract_archive(&PathBuf::from(&path), &PathBuf::from(&dest), policy).map_err(Into::into);

    write_result(
        &oplog,
        user_operation("Extract archive")
            .source(&path)
            .destination(&dest)
            .detail("If a file exists", format!("{policy:?}")),
        &result,
        |record, outcome: &ExtractOutcome| {
            record
                .detail("Files", outcome.total_files.to_string())
                .detail("Bytes", outcome.total_bytes.to_string())
                .detail(
                    "Skipped (already present)",
                    outcome.skipped_existing.to_string(),
                )
                .outcome(if outcome.aborted {
                    OperationOutcome::verified(false)
                } else {
                    OperationOutcome::verified(outcome.errors.is_empty())
                })
        },
    );

    result
}

/// WHDLoad detection result for the frontend.
#[derive(Debug, Serialize)]
pub struct WhdloadResult {
    pub confidence: Confidence,
    pub slave: Option<String>,
    pub executable: Option<String>,
    pub has_data_dir: bool,
    pub has_icon: bool,
    pub notes: String,
}

/// Detect whether an archive looks like a WHDLoad package.
#[tauri::command]
pub fn lha_detect_whdload(path: String) -> AppResult<WhdloadResult> {
    let info = open_archive(&PathBuf::from(&path))?;
    let verdict = detect_whdload(&info.entries);
    Ok(WhdloadResult {
        confidence: verdict.confidence,
        slave: verdict.slave,
        executable: verdict.executable,
        has_data_dir: verdict.has_data_dir,
        has_icon: verdict.has_icon,
        notes: verdict.notes,
    })
}

/// Emitted when a background extraction finishes.
pub const EXTRACT_RESULT_EVENT: &str = "lha-extract-result";

/// What a finished extraction delivers to the UI.
#[derive(Debug, Clone, Serialize)]
pub struct ExtractResult {
    pub job_id: JobId,
    pub outcome: ExtractOutcome,
}

/// Start extracting an archive in the background.
///
/// Returns a job id immediately. A WHDLoad package can hold thousands of files,
/// so extraction belongs on a job the user can watch and stop (spec §54, §55).
/// The outcome arrives in an `lha-extract-result` event.
#[tauri::command]
pub fn lha_extract_job(
    path: String,
    dest: String,
    overwrite: Option<OverwritePolicy>,
    app: AppHandle,
    registry: State<'_, Arc<JobRegistry>>,
    oplog: State<'_, JsonlOperationLog>,
) -> AppResult<JobId> {
    let policy = overwrite.unwrap_or_default();
    let source = PathBuf::from(&path);
    let target = PathBuf::from(&dest);
    let registry = Arc::clone(&registry);
    let emit_app = app.clone();

    // The log entry is built here, where the request is understood, and filled
    // in on the worker once the outcome is known.
    let record = user_operation("Extract archive")
        .source(&path)
        .destination(&dest)
        .detail("If a file exists", format!("{policy:?}"));
    let log_path = oplog.path().to_path_buf();

    let id = spawn_job(
        &app,
        registry,
        "Extracting archive",
        move |job_id, progress| {
            let outcome = extract_archive_with(&source, &target, policy, progress)?;

            let record = record
                .detail("Files", outcome.total_files.to_string())
                .detail("Bytes", outcome.total_bytes.to_string())
                .detail(
                    "Skipped (already present)",
                    outcome.skipped_existing.to_string(),
                )
                .outcome(if outcome.aborted {
                    OperationOutcome::verified(false)
                } else {
                    OperationOutcome::verified(outcome.errors.is_empty())
                });
            // The worker has no `State`, so it writes through its own handle to the
            // same file rather than borrowing the managed one across threads.
            let log = crate::core::oplog::JsonlOperationLog::new(log_path);
            write_to(&log, &record);

            let _ = emit_app.emit(EXTRACT_RESULT_EVENT, ExtractResult { job_id, outcome });
            Ok(())
        },
    );

    Ok(id)
}

/// Append a record to a log handle, swallowing failures (see `oplog::write`).
fn write_to(
    log: &crate::core::oplog::JsonlOperationLog,
    record: &crate::core::oplog::OperationRecord,
) {
    use crate::core::oplog::OperationLog as _;
    if let Err(e) = log.record(record) {
        log::warn!("operation log write failed: {e}");
    }
}
