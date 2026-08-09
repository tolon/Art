//! Drag & drop commands.
//!
//! The frontend's `onDragDropEvent` handler passes raw OS file paths here.
//! We normalise them, then forward to the Workflow Engine to produce a plan
//! per object. Results are returned as an array so multi-file drops work.

use std::path::{Path, PathBuf};

use tauri::State;

use crate::core::workflow::{Plan, WorkflowEngine};
use crate::error::{AppError, AppResult};

/// Result of analysing one dropped path.
#[derive(Debug, serde::Serialize)]
pub struct DroppedAnalysis {
    /// The original path as received from the OS.
    pub path: String,
    /// Whether this object could be analysed.
    pub ok: bool,
    /// When `ok`, the workflow plan. When not, an error message.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub plan: Option<Plan>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Normalise a raw path: trim quotes, convert separators, reject obviously
/// unsafe forms. This is a *minimal* Phase 0 guard; full path-traversal
/// protection for archive extraction arrives with the LHA module (Phase 1).
fn normalise(raw: &str) -> AppResult<PathBuf> {
    let trimmed = raw.trim().trim_matches('"');
    if trimmed.is_empty() {
        return Err(AppError::from("empty path"));
    }
    Ok(PathBuf::from(trimmed))
}

/// Analyse a batch of dropped paths. Each is handled independently so one bad
/// file does not abort the whole drop.
#[tauri::command]
pub fn analyze_paths(
    paths: Vec<String>,
    engine: State<'_, WorkflowEngine>,
) -> AppResult<Vec<DroppedAnalysis>> {
    let mut out = Vec::with_capacity(paths.len());
    for raw in paths {
        let analysis = match normalise(&raw) {
            Err(e) => DroppedAnalysis {
                path: raw,
                ok: false,
                plan: None,
                error: Some(e.to_string()),
            },
            Ok(path) => analyse_one(&path, &raw, &engine),
        };
        out.push(analysis);
    }
    Ok(out)
}

fn analyse_one(path: &Path, raw: &str, engine: &WorkflowEngine) -> DroppedAnalysis {
    match engine.plan(path) {
        Ok(plan) => DroppedAnalysis {
            path: raw.to_string(),
            ok: true,
            plan: Some(plan),
            error: None,
        },
        Err(e) => DroppedAnalysis {
            path: raw.to_string(),
            ok: false,
            plan: None,
            error: Some(e.to_string()),
        },
    }
}
