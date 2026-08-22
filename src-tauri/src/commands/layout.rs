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
use crate::core::layout::{Collision, LayoutPlan};
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

pub const LAYOUT_PLAN_EVENT: &str = "layout-plan-result";

#[derive(Debug, Clone, Serialize)]
pub struct LayoutPlanResult {
    pub job_id: u64,
    pub plan: LayoutPlan,
}

/// What laying these out would do. Writes nothing (§92's PREVIEW).
///
/// **A job, and the reason is a number** (§54). Planning is read-only, which
/// is why it ran on the command thread for as long as it did; what changed is
/// how much reading it does. Since ART-177's G1, `presence_of` compares
/// **content**, so a plan whose destinations already exist — the resume case,
/// which is the one this feature was built for — reads every one of them in
/// full.
///
/// Measured on the owner's own collection (1 697 WHDLoad HDFs, 3.74 GB):
/// **797 ms** for the first plan and **138 898 ms** for the plan over a
/// staging tree that already held it. Two and a quarter minutes on the
/// command thread is a frozen window, so this is a job with progress, like
/// every other long operation here — `archives_plan_install` took the same
/// route for the same reason (ART-066).
///
/// The plan arrives on [`LAYOUT_PLAN_EVENT`]; a cancelled or failed job never
/// sends one, and the screen learns that from `onJobProgress`.
#[tauri::command]
pub fn layout_plan(
    request: LayoutRequest,
    app: AppHandle,
    registry: State<'_, Arc<JobRegistry>>,
) -> AppResult<u64> {
    let registry = Arc::clone(&registry);
    let emit_app = app.clone();
    let title = format!("Working out what {} sources need", request.paths.len());

    let id = spawn_job(&app, registry, &title, move |job_id, progress| {
        let plan = crate::core::layout::plan_with(
            &request.root,
            &request.paths,
            &request.policy,
            progress,
        )?;
        // Nothing is logged: §53 is about operations that change user data,
        // and this one writes nothing at all.
        let _ = emit_app.emit(LAYOUT_PLAN_EVENT, LayoutPlanResult { job_id, plan });
        Ok(())
    });

    Ok(id)
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
/// Both answers, from one walk: what clashes, and what is already exactly
/// right (ART-177). They are the same question asked of the same paths, and
/// returning them separately is how the screen came to call a stopped run's
/// own output a collision.
#[derive(Debug, Clone, Serialize)]
pub struct RecheckResult {
    pub collisions: Vec<Collision>,
    pub already_in_place: Vec<String>,
}

#[tauri::command]
pub fn layout_recheck(plan: LayoutPlan) -> AppResult<RecheckResult> {
    let (collisions, already_in_place) = crate::core::layout::settled_in(&plan.root, &plan.items);
    Ok(RecheckResult {
        collisions,
        already_in_place,
    })
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

        let dir = std::env::temp_dir().join(format!(
            "art-layout-recheck-{}",
            crate::core::test_scratch_id()
        ));
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
                writes_icon: false,
            }],
            refused: Vec::new(),
            collisions: Vec::new(),
            too_deep: Default::default(),
            duplicates: Default::default(),
            already_in_place: Vec::new(),
            bytes: 10,
        };

        let collisions = layout_recheck(plan).unwrap();

        assert_eq!(collisions.collisions.len(), 1, "{collisions:?}");
        assert_eq!(collisions.collisions[0].destination, "Floppies/Disk.adf");
        assert!(
            collisions.already_in_place.is_empty(),
            "the file on disk is not this item's own output — the fixture writes `already here` where the source has different bytes"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }
}
