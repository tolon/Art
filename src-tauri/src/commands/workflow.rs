//! Workflow commands: turn a dropped path into a `Plan` of candidate actions.

use std::path::PathBuf;

use tauri::State;

use crate::core::error::CoreError;
use crate::core::workflow::types::{Safety, WorkflowOutcome};
use crate::core::workflow::{Plan, WorkflowEngine};
use crate::error::AppResult;

/// Analyse a single dropped path and return the recommended plan.
///
/// Frontend usage:
/// ```ts
/// const plan = await invoke<Plan>('plan_path', { path: 'C:/disk.adf' });
/// ```
#[tauri::command]
pub fn plan_path(path: String, engine: State<'_, WorkflowEngine>) -> AppResult<Plan> {
    let plan = engine.plan(&PathBuf::from(&path))?;
    Ok(plan)
}

/// Run an `Execute`-kind workflow against a path.
///
/// Only workflows the engine itself offered for this object can run, and only
/// read-only ones may run without a confirmation gate. Anything that modifies
/// data goes through its studio's own preview/backup/verify flow instead —
/// spec §92 forbids changing data straight off a dropped-file panel.
#[tauri::command]
pub fn run_workflow(
    path: String,
    workflow_id: String,
    engine: State<'_, WorkflowEngine>,
) -> AppResult<WorkflowOutcome> {
    let target = PathBuf::from(&path);
    let detection = crate::core::detect::detect(&target)?;

    let workflow = engine
        .registry
        .candidates_for(&detection)
        .into_iter()
        .find(|w| w.info().id == workflow_id)
        .ok_or_else(|| {
            CoreError::InvalidInput(format!(
                "'{workflow_id}' is not an available action for this object"
            ))
        })?;

    let info = workflow.info();
    if !info.available {
        return Err(CoreError::NotImplemented(info.name.to_string()).into());
    }
    if info.safety != Safety::ReadOnly {
        return Err(CoreError::SafetyRefused(format!(
            "'{}' changes data and must be run from its studio, where it can be previewed and backed up",
            info.name
        ))
        .into());
    }

    Ok(workflow.run(&target, &detection)?)
}

/// List every registered workflow (used by the command palette / settings).
#[tauri::command]
pub fn list_workflows(engine: State<'_, WorkflowEngine>) -> Vec<String> {
    engine
        .registry
        .list()
        .iter()
        .map(|w| w.id.to_string())
        .collect()
}
