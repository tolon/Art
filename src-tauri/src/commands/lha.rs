//! LHA Studio commands: open / extract / WHDLoad detection.

use std::path::PathBuf;

use serde::Serialize;
use tauri::State;

use super::oplog::{user_operation, write_result};
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
