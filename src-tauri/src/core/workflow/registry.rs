//! Workflow registry and engine.
//!
//! The registry holds all known workflows; the engine binds detection to
//! candidate workflows and builds a `Plan` for the frontend.

use std::path::Path;
use std::sync::Arc;

use super::types::{
    default_confidence, Plan, Recommendation, Workflow, WorkflowCategory, WorkflowInfo,
};
use crate::core::detect::detect;
use crate::core::error::CoreResult;

/// Holds every registered workflow.
pub struct WorkflowRegistry {
    workflows: Vec<Arc<dyn Workflow>>,
}

impl WorkflowRegistry {
    pub fn new() -> Self {
        Self { workflows: vec![] }
    }

    pub fn register(&mut self, w: impl Workflow + 'static) {
        self.workflows.push(Arc::new(w));
    }

    /// Workflows whose `can_handle` returns true for `detection`, sorted by
    /// priority (ascending). Exposed as `Arc` so the engine can call `run`.
    pub fn candidates_for(
        &self,
        detection: &crate::core::detect::Detection,
    ) -> Vec<Arc<dyn Workflow>> {
        let mut matched: Vec<_> = self
            .workflows
            .iter()
            .filter(|w| w.can_handle(detection))
            .cloned()
            .collect();
        matched.sort_by_key(|w| w.info().priority);
        matched
    }

    /// All known workflow infos, sorted by priority.
    pub fn list(&self) -> Vec<&WorkflowInfo> {
        let mut all: Vec<_> = self.workflows.iter().map(|w| w.info()).collect();
        all.sort_by_key(|i| i.priority);
        all
    }
}

impl Default for WorkflowRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// The engine wires detection to workflows. Stored in Tauri `State`.
pub struct WorkflowEngine {
    pub registry: WorkflowRegistry,
}

impl WorkflowEngine {
    pub fn new(registry: WorkflowRegistry) -> Self {
        Self { registry }
    }

    /// Detect an object and produce a `Plan` of candidate workflows.
    pub fn plan(&self, path: &Path) -> CoreResult<Plan> {
        let detection = detect(path)?;

        let candidates: Vec<WorkflowInfo> = self
            .registry
            .candidates_for(&detection)
            .iter()
            .map(|w| w.info().clone())
            .collect();

        // Build recommendations from the Recommended-category candidates.
        let confidence = default_confidence(&detection);
        let recommendations: Vec<Recommendation> = candidates
            .iter()
            .filter(|c| c.category == WorkflowCategory::Recommended)
            .map(|info| Recommendation {
                info: info.clone(),
                confidence,
                reason: reason_for(info, &detection),
            })
            .collect();

        Ok(Plan {
            detection,
            recommendations,
            candidates,
        })
    }
}

/// Why a workflow is being suggested, in the user's language.
///
/// Falls back to the workflow's own description, so a new workflow reads
/// sensibly without needing an entry here.
fn reason_for(info: &WorkflowInfo, detection: &crate::core::detect::Detection) -> String {
    if !info.available {
        return format!("{} (arrives in a later phase)", info.description);
    }
    match info.id {
        "adf.browse" => format!(
            "This looks like an Amiga floppy image ({}); open it to see what is on it.",
            detection.format_hint
        ),
        "lha.browse" => {
            "Amiga archives often contain WHDLoad packages — check before extracting.".to_string()
        }
        "dir.scan_collection" => {
            "Scanning the folder catalogues every Amiga file in it and finds duplicates."
                .to_string()
        }
        _ => info.description.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::workflow::builtin;

    #[test]
    fn registry_sorts_by_priority() {
        let mut reg = WorkflowRegistry::new();
        builtin::register_all(&mut reg);

        let list = reg.list();
        assert!(list.len() > 2);
        for pair in list.windows(2) {
            assert!(
                pair[0].priority <= pair[1].priority,
                "registry list must be ordered by priority"
            );
        }
    }

    #[test]
    fn hash_workflow_runs_on_file() {
        let d = std::env::temp_dir().join(format!(
            "art-wf-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&d).unwrap();
        let p = d.join("x.rom");
        std::fs::write(&p, b"abc").unwrap();

        let det = crate::core::detect::detect(&p).unwrap();
        let hash = builtin::Hash;
        assert!(hash.can_handle(&det));
        let out = hash.run(&p, &det).unwrap();
        assert!(out.success);
        assert_eq!(
            out.message,
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );

        std::fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn engine_plan_returns_candidates() {
        let mut reg = WorkflowRegistry::new();
        builtin::register_all(&mut reg);
        let engine = WorkflowEngine::new(reg);

        let d = std::env::temp_dir().join(format!(
            "art-wf-plan-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&d).unwrap();
        let p = d.join("disk.adf");
        std::fs::write(&p, vec![0u8; crate::core::detect::sizes::ADF_DD as usize]).unwrap();

        let plan = engine.plan(&p).unwrap();
        assert!(!plan.candidates.is_empty());
        // Opening the disk is the primary suggestion for a floppy image.
        assert!(plan
            .recommendations
            .iter()
            .any(|r| r.info.id == "adf.browse"));

        std::fs::remove_dir_all(&d).ok();
    }

    /// §96: a planned action is registered `available: false` so it renders as
    /// "Coming Later" rather than vanishing — and every one of those has to say
    /// what it is, or the label conveys nothing.
    ///
    /// Asserts that property rather than that unimplemented actions exist. The
    /// earlier version required at least one ADF action to be unfinished, which
    /// turned finishing them into a test failure — and Stage W finished the
    /// last one.
    #[test]
    fn unavailable_workflows_say_so_in_their_reason() {
        let mut reg = WorkflowRegistry::new();
        builtin::register_all(&mut reg);
        let engine = WorkflowEngine::new(reg);

        let d = std::env::temp_dir().join(format!(
            "art-wf-later-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&d).unwrap();
        let p = d.join("disk.adf");
        std::fs::write(&p, vec![0u8; crate::core::detect::sizes::ADF_DD as usize]).unwrap();

        let plan = engine.plan(&p).unwrap();
        // A floppy must offer something, whether or not any of it is pending.
        assert!(!plan.candidates.is_empty());

        // The property across the whole catalogue, not just what a floppy
        // happens to offer today: a "Coming Later" chip with an empty name or
        // description tells the user nothing about what is coming.
        let mut registry = WorkflowRegistry::new();
        builtin::register_all(&mut registry);
        for info in registry.list() {
            if info.available {
                continue;
            }
            assert!(
                !info.name.trim().is_empty() && !info.description.trim().is_empty(),
                "{} is registered unavailable with nothing to show for it",
                info.id
            );
        }

        std::fs::remove_dir_all(&d).ok();
    }
}
