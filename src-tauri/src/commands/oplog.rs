//! Operation Log commands, and the helper every mutating command uses to
//! record what it did.

use tauri::State;

use crate::core::oplog::{JsonlOperationLog, OperationLog, OperationOrigin, OperationRecord};
use crate::error::{AppError, AppResult};

/// Append a record, swallowing any logging failure.
///
/// Logging must never turn a successful operation into a reported failure — a
/// full disk should not make a completed write look broken. Problems go to the
/// application log instead.
pub fn write(oplog: &State<'_, JsonlOperationLog>, record: OperationRecord) {
    if let Err(e) = oplog.record(&record) {
        log::warn!("operation log write failed: {e}");
    }
}

/// Record the result of an operation that returns `Result`.
///
/// Keeps the success/failure branches out of every command body: pass the part
/// of the record that is known up front, plus a closure that adds whatever the
/// successful outcome contributes (a backup path, a file count).
pub fn write_result<T>(
    oplog: &State<'_, JsonlOperationLog>,
    record: OperationRecord,
    result: &Result<T, AppError>,
    on_success: impl FnOnce(OperationRecord, &T) -> OperationRecord,
) {
    let record = match result {
        Ok(value) => on_success(record, value),
        Err(e) => record.failure(e.code(), e.to_string()),
    };
    write(oplog, record);
}

/// Record an operation when only the log's path is available.
///
/// Background jobs run on their own thread and cannot carry a Tauri `State`
/// across it, so they take the path instead. Best-effort like every other log
/// write: a failure is logged and the operation it describes still stands.
pub fn write_to_path(log_path: &std::path::Path, record: &OperationRecord) {
    use crate::core::oplog::OperationLog as _;
    let log = JsonlOperationLog::new(log_path.to_path_buf());
    if let Err(err) = log.record(record) {
        log::warn!("operation log write failed: {err}");
    }
}

/// Start a record for a user-initiated operation.
pub fn user_operation(name: &str) -> OperationRecord {
    OperationRecord::new(name, OperationOrigin::UserInterface)
}

/// The most recent operations, newest first (spec §53).
#[tauri::command]
pub fn oplog_recent(
    limit: Option<usize>,
    oplog: State<'_, JsonlOperationLog>,
) -> AppResult<Vec<OperationRecord>> {
    Ok(oplog.recent(limit.unwrap_or(100))?)
}

/// The operation history as readable text, for export (spec §53).
#[tauri::command]
pub fn oplog_export(
    limit: Option<usize>,
    oplog: State<'_, JsonlOperationLog>,
) -> AppResult<String> {
    let records = oplog.recent(limit.unwrap_or(1000))?;
    let mut out = String::from("AMIGA RETRO TOOLKIT — OPERATION LOG\n");
    for record in records {
        out.push_str("\n----------------------------------------\n\n");
        out.push_str(&record.render());
    }
    Ok(out)
}

/// Write the readable history to `path`.
///
/// Export happens here rather than in the frontend so it goes through the same
/// atomic write every other file operation uses — and so ART does not need
/// blanket filesystem permission in the webview.
#[tauri::command]
pub fn oplog_export_to(
    path: String,
    limit: Option<usize>,
    oplog: State<'_, JsonlOperationLog>,
) -> AppResult<()> {
    let text = oplog_export(limit, oplog)?;
    crate::core::safety::atomic_write(&std::path::PathBuf::from(&path), text.as_bytes())?;
    Ok(())
}

/// Where the log file lives, so the UI can reveal it.
#[tauri::command]
pub fn oplog_path(oplog: State<'_, JsonlOperationLog>) -> String {
    oplog.path().to_string_lossy().into_owned()
}
