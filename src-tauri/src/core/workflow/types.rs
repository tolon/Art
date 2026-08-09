//! Workflow trait and supporting types.

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::core::detect::Detection;
use crate::core::error::CoreResult;

/// What a workflow does to user data. Drives the confirmation UI (see the
/// spec's destructive-operation classification: READ ONLY / SAFE /
/// REQUIRES BACKUP / DESTRUCTIVE / EXPERIMENTAL).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Safety {
    /// No writes anywhere.
    ReadOnly,
    /// Writes only to new/derivative files; originals untouched.
    Safe,
    /// Modifies the original after an automatic backup.
    RequiresBackup,
    /// Destructive; requires explicit, double confirmation.
    Destructive,
    /// Unproven; clearly flagged.
    Experimental,
}

/// Coarse grouping used for the "What can I do?" UI panel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowCategory {
    /// Recommended / primary action (shown first).
    Recommended,
    /// Common secondary action.
    Standard,
    /// Power-user / advanced action.
    Advanced,
}

/// Confidence level for recommendations and compatibility.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum Confidence {
    High,
    Medium,
    Low,
    Unknown,
}

/// How an action is carried out once the user picks it.
///
/// Most "what can I do?" actions open a studio with the object loaded; a few
/// are pure engine work. Keeping the distinction here — rather than letting the
/// UI infer it from the id — is what stops routing knowledge from leaking into
/// React (spec §95).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum WorkflowKind {
    /// The UI opens `route`, passing the object's path as context.
    Navigate { route: &'static str },
    /// The engine performs the work and returns a [`WorkflowOutcome`].
    Execute,
}

/// A short, UI-facing description of a workflow. Cheap to serialise to the
/// frontend. The actual `Workflow` trait object lives only on the Rust side.
///
/// Only `Serialize`: these flow Rust → frontend, never back. `&'static str`
/// fields are intentionally static (they come from code, not from input).
#[derive(Debug, Clone, Serialize)]
pub struct WorkflowInfo {
    /// Stable id, e.g. `"adf.launch_winuae"`.
    pub id: &'static str,
    pub name: &'static str,
    pub description: &'static str,
    pub category: WorkflowCategory,
    pub safety: Safety,
    /// Lower sorts first.
    pub priority: u32,
    /// True when this workflow is implemented and safe to run in the current
    /// phase. When `false`, it is surfaced as "Coming Later" (spec §96).
    pub available: bool,
    /// Whether picking this opens a studio or runs engine work.
    pub kind: WorkflowKind,
}

/// Result of running a workflow (later phases). For Phase 0, workflows return
/// `NotImplemented`. Rust → frontend only.
#[derive(Debug, Clone, Serialize)]
pub struct WorkflowOutcome {
    pub workflow_id: String,
    pub success: bool,
    pub message: String,
    pub verification: Option<bool>,
}

/// The central trait every concrete workflow implements.
///
/// `Send + Sync` so it can live behind an `Arc<dyn Workflow>` inside Tauri's
/// `State`.
pub trait Workflow: Send + Sync {
    fn info(&self) -> &WorkflowInfo;

    /// Whether this workflow is a candidate for the given detection.
    fn can_handle(&self, detection: &Detection) -> bool;

    /// Execute the workflow. `input` is the source object path.
    ///
    /// The default rejects the call, which is correct for every
    /// [`WorkflowKind::Navigate`] workflow — those are carried out by the UI
    /// opening a route, not by the engine. Only `Execute` workflows override it.
    fn run(&self, _input: &Path, _detection: &Detection) -> CoreResult<WorkflowOutcome> {
        Err(crate::core::error::CoreError::InvalidInput(format!(
            "'{}' is opened by the user interface and has nothing to execute",
            self.info().id
        )))
    }
}

/// A detection plus a compatibility recommendation. Used to build a `Plan`.
#[derive(Debug, Clone)]
pub struct DetectionRef {
    pub detection: Detection,
}

/// One ranked workflow suggestion. Rust → frontend only.
#[derive(Debug, Clone, Serialize)]
pub struct Recommendation {
    pub info: WorkflowInfo,
    pub confidence: Confidence,
    pub reason: String,
}

/// The full plan returned to the frontend for a dropped object: what it is,
/// what we recommend, and the raw list of candidate workflows. Rust → frontend.
#[derive(Debug, Clone, Serialize)]
pub struct Plan {
    pub detection: Detection,
    pub recommendations: Vec<Recommendation>,
    /// All candidate workflows (including advanced), already sorted by priority.
    pub candidates: Vec<WorkflowInfo>,
}

/// Helper: default confidence mapping for a detection when no compatibility
/// engine answer is available yet (Phase 0).
pub fn default_confidence(_detection: &Detection) -> Confidence {
    // Phase 0: we never claim HIGH compatibility because real format parsing
    // and the compatibility engine arrive in later phases.
    Confidence::Unknown
}
