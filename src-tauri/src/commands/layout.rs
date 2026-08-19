//! Laying content out into a staging tree (SD-2 · G11).
//!
//! Two commands and one difference from `commands/preload.rs` worth stating:
//! **`layout_apply` takes the plan it is given rather than recomputing it.**
//! Preload recomputes because a screen must not be able to preview one card
//! and format another. Here the user's edits *are* the plan — retargeting rows
//! is the feature — so recomputing would throw away exactly what they came to
//! do. What protects the tree instead is the applier: `safe_join` on every
//! destination, and a refusal on anything already there.

use std::path::PathBuf;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, State};

use crate::core::layout::apply::apply;
use crate::core::layout::policy::Policy;
use crate::core::layout::{collisions_in, plan, Collision, LayoutPlan};
use crate::core::oplog::{JsonlOperationLog, OperationOutcome};
use crate::error::AppResult;

use super::jobs::{spawn_job, JobRegistry};
use super::oplog::{user_operation, write_to_path};

#[derive(Debug, Clone, Deserialize)]
pub struct LayoutRequest {
    pub root: PathBuf,
    pub paths: Vec<PathBuf>,
    pub policy: Policy,
}

/// What laying these out would do. Writes nothing (§92's PREVIEW).
#[tauri::command]
pub fn layout_plan(request: LayoutRequest) -> AppResult<LayoutPlan> {
    Ok(plan(&request.root, &request.paths, &request.policy)?)
}

/// Recompute collisions for `plan`'s **current** destinations against disk.
///
/// Not a replan: no walking, no classifying, no policy — exactly the check
/// `plan()` runs at the end (`core::layout::collisions_in`), re-asked after
/// the screen retargets a row. `retarget` (`src/lib/layout.ts`) can only
/// recompute the collisions *within* the plan; whether a new destination
/// already exists on disk is a fact only this command has looked at, so the
/// screen calls it after every retarget rather than blocking Apply on
/// staleness it cannot itself resolve.
#[tauri::command]
pub fn layout_recheck(plan: LayoutPlan) -> AppResult<Vec<Collision>> {
    Ok(collisions_in(&plan.root, &plan.items))
}

pub const LAYOUT_EVENT: &str = "layout-result";

#[derive(Debug, Clone, Serialize)]
pub struct LayoutResult {
    pub job_id: u64,
    pub root: String,
    pub outcome: crate::core::layout::apply::ApplyOutcome,
}

/// Build the staging tree. Returns a job id (§54).
#[tauri::command]
pub fn layout_apply(
    plan: LayoutPlan,
    app: AppHandle,
    registry: State<'_, Arc<JobRegistry>>,
    oplog: State<'_, JsonlOperationLog>,
) -> AppResult<u64> {
    let root = plan.root.display().to_string();
    let log_path = oplog.path().to_path_buf();
    let registry = Arc::clone(&registry);
    let emit_app = app.clone();
    let title = format!("Laying {} item(s) out in {root}", plan.items.len());
    let for_log = root.clone();

    let id = spawn_job(&app, registry, &title, move |job_id, progress| {
        let outcome = apply(&plan, progress);

        let record = user_operation("Lay content out into a staging folder")
            .destination(&for_log)
            .detail("Items", plan.items.len().to_string());
        let record = match &outcome {
            Ok(done) => record
                .detail("Placed", done.placed.to_string())
                .detail("Bytes", done.bytes.to_string())
                // Every file is read back by nothing: the tree is on the PC
                // and the user can open it. Verification belongs to whatever
                // puts it on the card.
                .outcome(OperationOutcome::verified(false)),
            Err(err) => record.failure(err.code(), err.to_string()),
        };
        write_to_path(&log_path, &record);

        let outcome = outcome?;
        let _ = emit_app.emit(
            LAYOUT_EVENT,
            LayoutResult {
                job_id,
                root: for_log,
                outcome,
            },
        );
        Ok(())
    });

    Ok(id)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// **The wire, written down.** `src/lib/layout.ts` builds this object by
    /// hand; nothing else in either build checks that the two agree.
    #[test]
    fn the_payload_the_frontend_sends_deserialises() {
        let request: LayoutRequest = serde_json::from_str(
            r#"{"root":"E:\\amiga\\ProjeART\\staging",
                "paths":["E:\\amiga\\Games"],
                "policy":{"whdload":"unpack","games":"Games","floppies":"Floppies",
                          "hard_disks":"HardDisks","discs":"CDs","unsorted":"Unsorted"}}"#,
        )
        .expect("the shape src/lib/layout.ts sends");

        assert_eq!(request.paths.len(), 1);
        assert_eq!(request.policy.games, "Games");
        assert_eq!(
            request.policy.whdload,
            crate::core::layout::policy::WhdloadPlacement::Unpack
        );
    }

    /// `layout_recheck` answers with a fresh on-disk collision, without
    /// touching anything the caller did not ask about — the whole point of
    /// re-asking rather than replanning.
    #[test]
    fn layout_recheck_finds_a_destination_that_now_exists_on_disk() {
        use crate::core::layout::{ItemKind, LayoutItem, Placement};

        let dir = std::env::temp_dir().join(format!("art-layout-recheck-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let root = dir.join("staging");
        std::fs::create_dir_all(root.join("Floppies")).unwrap();
        std::fs::write(root.join("Floppies").join("Disk.adf"), b"already here").unwrap();

        let plan = LayoutPlan {
            root: root.clone(),
            items: vec![LayoutItem {
                source: dir.join("Disk.adf"),
                kind: ItemKind::FloppyImage,
                destination: "Floppies/Disk.adf".into(),
                placement: Placement::CopyFile,
                bytes: 10,
            }],
            refused: Vec::new(),
            collisions: Vec::new(),
            too_deep: Default::default(),
            duplicates: Default::default(),
            bytes: 10,
        };

        let collisions = layout_recheck(plan).unwrap();

        assert_eq!(collisions.len(), 1, "{collisions:?}");
        assert_eq!(collisions[0].destination, "Floppies/Disk.adf");

        let _ = std::fs::remove_dir_all(&dir);
    }
}
