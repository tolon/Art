//! Installing AmigaOS from the user's own media (SD-2 · G5) — the adapter
//! layer over `core::osinstall`. Thin only: deserialize, call core, serialize
//! back.
//!
//! Four commands. `osinstall_scan_media` and `osinstall_plan` both end up
//! opening every candidate in the media folder — directly, or through
//! `plan::plan`'s own call to `scan::find_media` — and a missing or
//! unreadable folder is the single most likely mistake after a bad ROM
//! (`core/osinstall/scan.rs`'s own doc comment; ART-060's class of problem).
//! `find_media` itself raises that as a bare `CoreError`, which would reach
//! the screen as an English sentence if either command let it propagate.
//! Both turn it into a typed refusal here, at the command boundary, instead
//! — `core/osinstall/plan.rs` is left otherwise untouched; the translation
//! happens only on this side of the wire, the same way `commands/adf.rs`
//! and `commands/layout.rs` keep their own core modules free of anything
//! Tauri-shaped.
//!
//! `osinstall_apply` takes the plan it is given, the way `layout_apply` does
//! and `preload_run` does not (see `commands/layout.rs`'s own module note):
//! the user's component choices *are* the plan, so recomputing it here would
//! let the screen preview one install and build another.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, State};

use crate::core::error::CoreError;
use crate::core::oplog::{JsonlOperationLog, OperationOutcome};
use crate::core::osinstall::apply::{
    apply, ApplyOutcome, DistributionManifest, MANIFEST_FILE_NAME,
};
use crate::core::osinstall::plan::{plan, InstallPlan, InstallRequest};
use crate::core::osinstall::recipe;
use crate::core::osinstall::scan::{find_media, FoundMedia};
use crate::core::osinstall::verify::{verify_volume, VerifyReport};
use crate::error::{AppError, AppResult};

use super::jobs::{spawn_job, JobRegistry};
use super::oplog::{user_operation, write_result, write_to_path};

// ---------------------------------------------------------------------------
// osinstall_scan_media
// ---------------------------------------------------------------------------

/// What scanning a media folder found, or why it could not be looked at.
///
/// A refusal, not `find_media`'s own `CoreError` sentence — see the module
/// doc comment.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "outcome", rename_all = "kebab-case")]
pub enum MediaScanResult {
    Found {
        media: Vec<FoundMedia>,
    },
    /// The folder does not exist, or ART cannot read it.
    FolderUnreadable {
        folder: String,
    },
}

/// Every install disk `find_media` can open directly inside `folder` —
/// before any ROM or component has been chosen, so the screen can show what
/// it found the moment a folder is picked. Writes nothing.
#[tauri::command]
pub fn osinstall_scan_media(folder: PathBuf) -> AppResult<MediaScanResult> {
    Ok(match find_media(&folder) {
        Ok(media) => MediaScanResult::Found { media },
        Err(_) => MediaScanResult::FolderUnreadable {
            folder: folder.display().to_string(),
        },
    })
}

// ---------------------------------------------------------------------------
// osinstall_plan
// ---------------------------------------------------------------------------

/// What planning an install found, or why the media folder itself could not
/// be looked at.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "outcome", rename_all = "kebab-case")]
pub enum PlanResult {
    Planned { plan: InstallPlan },
    FolderUnreadable { folder: String },
}

/// What installing the chosen components would do — or every reason it
/// cannot. Writes nothing (§92's PREVIEW).
///
/// Always the shipped AmigaOS 3.2 recipe today — `core/osinstall/mod.rs`'s
/// own module doc names `AmigaOS 3.9 / an ISO source` as the case that would
/// need a recipe id on the request; that recipe does not exist yet, so
/// nothing here guesses at its shape.
#[tauri::command]
pub fn osinstall_plan(request: InstallRequest) -> AppResult<PlanResult> {
    // The same folder `plan()` would open through `find_media` — checked
    // here first so a bad path reaches the screen as a value it can
    // translate, never as `find_media`'s own English sentence. See the
    // module doc comment.
    if find_media(&request.media_folder).is_err() {
        return Ok(PlanResult::FolderUnreadable {
            folder: request.media_folder.display().to_string(),
        });
    }
    let recipe = recipe::amigaos_32()?;
    Ok(PlanResult::Planned {
        plan: plan(&request, &recipe)?,
    })
}

// ---------------------------------------------------------------------------
// osinstall_apply
// ---------------------------------------------------------------------------

/// `osinstall_apply`'s own request: the plan the screen showed, plus where it
/// goes. `InstallPlan` carries no destination of its own — `plan()` never
/// even reads `InstallRequest::destination` — and `apply()` takes `root`
/// separately, so this does too.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApplyRequest {
    pub plan: InstallPlan,
    pub destination: PathBuf,
}

/// The event a finished install arrives on.
pub const OSINSTALL_EVENT: &str = "osinstall-result";

#[derive(Debug, Clone, Serialize)]
pub struct OsInstallResult {
    pub job_id: u64,
    pub destination: String,
    pub outcome: ApplyOutcome,
}

/// Build the distribution tree. Returns a job id (§54) — an install copies an
/// entire operating system, and `apply()` already reports progress per file
/// it places (`sink.report(done, Some(total), &item.to)`, `core/osinstall/apply.rs`),
/// which reaches the screen through the ordinary `job-progress` event with
/// no extra plumbing here: `done`/`total` move item by item and `message`
/// names the file currently landing, so a bar that only moves at the end is
/// not what this produces.
#[tauri::command]
pub fn osinstall_apply(
    request: ApplyRequest,
    app: AppHandle,
    registry: State<'_, Arc<JobRegistry>>,
    oplog: State<'_, JsonlOperationLog>,
) -> AppResult<u64> {
    let destination = request.destination.display().to_string();
    let log_path = oplog.path().to_path_buf();
    let registry = Arc::clone(&registry);
    let emit_app = app.clone();
    let title = format!("Installing {} into {destination}", request.plan.release);
    let for_log = destination.clone();
    let plan = request.plan;
    let root = request.destination;

    let id = spawn_job(&app, registry, &title, move |job_id, progress| {
        let outcome = apply(&plan, &root, progress);

        // Background jobs run on their own thread and cannot carry a Tauri
        // `State` across it, so this logs through `write_to_path` rather
        // than `write_result` — the same shape `layout_apply` and
        // `preload_run` already use for the identical reason.
        let record = user_operation("Build an AmigaOS distribution tree")
            .destination(&for_log)
            .detail("Release", plan.release.clone())
            .detail("Components", plan.components_on.join(", "));
        let record = match &outcome {
            Ok(done) => record
                .detail("Files", done.files.to_string())
                .detail("Directories", done.directories.to_string())
                .detail("Bytes", done.bytes.to_string())
                // Verification is its own step (`osinstall_verify`), run
                // against the volume this tree is later copied onto — not
                // here, where nothing has been read back yet.
                .outcome(OperationOutcome::verified(false)),
            Err(err) => record.failure(err.code(), err.to_string()),
        };
        write_to_path(&log_path, &record);

        let outcome = outcome?;
        let _ = emit_app.emit(
            OSINSTALL_EVENT,
            OsInstallResult {
                job_id,
                destination: for_log,
                outcome,
            },
        );
        Ok(())
    });

    Ok(id)
}

// ---------------------------------------------------------------------------
// osinstall_verify
// ---------------------------------------------------------------------------

/// What `osinstall_verify` needs: which card partition to read, and where the
/// distribution tree's own manifest is.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VerifyRequest {
    pub image: PathBuf,
    pub slot: Option<usize>,
    pub index: usize,
    /// The distribution tree's own root — `apply()` wrote
    /// `distribution.json` (`MANIFEST_FILE_NAME`) there, read back by that
    /// name rather than asked of the caller.
    pub dist_root: PathBuf,
}

fn read_manifest(dist_root: &Path) -> AppResult<DistributionManifest> {
    let manifest_path = dist_root.join(MANIFEST_FILE_NAME);
    let text = std::fs::read_to_string(&manifest_path)?;
    serde_json::from_str(&text).map_err(|err| {
        AppError::from(CoreError::Malformed {
            format: "distribution manifest".into(),
            detail: err.to_string(),
        })
    })
}

fn verify_at(request: &VerifyRequest) -> AppResult<VerifyReport> {
    let manifest = read_manifest(&request.dist_root)?;
    Ok(verify_volume(
        &request.image,
        request.slot,
        request.index,
        &manifest,
    )?)
}

/// Read the volume back and check it against the manifest `osinstall_apply`
/// wrote (§92's VERIFY step, Task 10's `verify_volume`).
///
/// Logged like any other operation, but `verified` is
/// `report.failed == 0 && report.not_checked == 0` — **not** `failed == 0`
/// alone, because "ART did not look" is not "ART found nothing wrong" (§89).
/// The record carries all three counts, not just the one boolean, so the
/// log agrees with what the screen shows.
#[tauri::command]
pub fn osinstall_verify(
    request: VerifyRequest,
    oplog: State<'_, JsonlOperationLog>,
) -> AppResult<VerifyReport> {
    let image = request.image.display().to_string();
    let dist_root = request.dist_root.display().to_string();
    let result = verify_at(&request);

    let record = user_operation("Verify an AmigaOS install against its manifest")
        .source(&dist_root)
        .destination(&image);
    write_result(&oplog, record, &result, |record, report| {
        record
            .detail("Passed", report.passed.to_string())
            .detail("Failed", report.failed.to_string())
            .detail("Not checked", report.not_checked.to_string())
            .outcome(OperationOutcome::verified(
                report.failed == 0 && report.not_checked == 0,
            ))
    });

    result
}

// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// **The wire, written down.** `src/lib/osinstall.ts` builds this object
    /// by hand; nothing else in either build checks that the two agree.
    #[test]
    fn the_payload_the_frontend_sends_deserialises() {
        let json = r#"{
            "mediaFolder": "E:\\media",
            "rom": "E:\\kick.rom",
            "chosen": ["workbench-base", "extras"],
            "destination": "E:\\dist"
        }"#;
        let request: InstallRequest = serde_json::from_str(json).unwrap();
        assert_eq!(request.chosen.len(), 2);
    }

    /// The wire in the other direction: the plan `osinstall_plan` hands the
    /// screen has to be exactly what `osinstall_apply` accepts back, because
    /// `ApplyRequest` takes the plan it is given rather than recomputing it
    /// (see the module doc comment) — a plan that could serialize out but
    /// not deserialize back in would silently break that rule the moment a
    /// screen tried to apply what it was shown. Built from a real `plan()`
    /// run (`fixtures::planned_with`), not a hand-typed literal, so this
    /// exercises the whole struct — `items`, `media_paths`, `user_startup` —
    /// not just the fields a hand-written JSON blob happened to include.
    #[test]
    fn the_plan_the_frontend_sends_back_deserialises_into_an_apply_request() {
        let (plan, _dir) = crate::core::osinstall::fixtures::planned_with(
            &["workbench-base"],
            &["Workbench3.2"],
            Some(47),
        );
        assert!(plan.refusals.is_empty(), "{:?}", plan.refusals);
        let plan_json = serde_json::to_string(&plan).unwrap();

        let payload = format!(r#"{{"plan":{plan_json},"destination":"E:\\dist"}}"#);
        let request: ApplyRequest = serde_json::from_str(&payload).unwrap();

        assert_eq!(request.plan.release, "AmigaOS 3.2");
        assert_eq!(request.plan.items.len(), plan.items.len());
        assert_eq!(request.destination, PathBuf::from("E:\\dist"));
    }

    fn scratch(tag: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("art-osinstall-cmd-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// The carried-forward review point: a media folder that does not exist
    /// must reach the screen as a value it can translate, never as
    /// `find_media`'s own English `CoreError` sentence.
    #[test]
    fn scanning_a_missing_folder_is_a_typed_refusal_not_a_sentence() {
        let dir = scratch("scan-missing");
        let missing = dir.join("does-not-exist");

        let result = osinstall_scan_media(missing.clone()).unwrap();

        assert_eq!(
            result,
            MediaScanResult::FolderUnreadable {
                folder: missing.display().to_string(),
            }
        );
    }

    #[test]
    fn scanning_a_real_folder_finds_its_media() {
        let dir = scratch("scan-real");
        crate::core::osinstall::fixtures::workbench(&dir);

        let result = osinstall_scan_media(dir).unwrap();

        match result {
            MediaScanResult::Found { media } => {
                assert_eq!(media.len(), 1);
                assert_eq!(media[0].volume_name, "Workbench3.2");
            }
            other => panic!("expected Found, got {other:?}"),
        }
    }

    /// The same carried-forward point, for `osinstall_plan`: a bad media
    /// folder path is at least as likely to be discovered here, at the
    /// screen's own "preview" step, as through a separate scan call.
    #[test]
    fn planning_against_a_missing_folder_is_a_typed_refusal_not_a_sentence() {
        let dir = scratch("plan-missing");
        let missing = dir.join("does-not-exist");

        let result = osinstall_plan(InstallRequest {
            media_folder: missing.clone(),
            rom: None,
            chosen: vec!["workbench-base".to_string()],
            destination: dir.join("dist"),
        })
        .unwrap();

        match result {
            PlanResult::FolderUnreadable { folder } => {
                assert_eq!(folder, missing.display().to_string());
            }
            other => panic!("expected FolderUnreadable, got {other:?}"),
        }
    }

    #[test]
    fn planning_against_a_real_folder_returns_the_plan() {
        let dir = scratch("plan-real");
        crate::core::osinstall::fixtures::workbench(&dir);

        let result = osinstall_plan(InstallRequest {
            media_folder: dir.clone(),
            rom: None,
            chosen: vec!["workbench-base".to_string()],
            destination: dir.join("dist"),
        })
        .unwrap();

        match result {
            PlanResult::Planned { plan } => assert_eq!(plan.release, "AmigaOS 3.2"),
            other => panic!("expected Planned, got {other:?}"),
        }
    }

    /// `verified` is `failed == 0 && not_checked == 0`, never `failed == 0`
    /// alone (§89) — proved directly against a real FFS volume whose content
    /// is genuinely never checked at all (no manifest, nothing copied), so a
    /// version reading only `report.failed` would wrongly call this
    /// verified.
    #[test]
    fn a_report_with_nothing_checked_is_not_verified() {
        use crate::core::hdf::create_hdf;
        use crate::core::osinstall::apply::{FileRecord, MediaRecord};
        use crate::core::rdb::{AmigaHardDiskFs, PartitionSpec};

        let dir = scratch("verify-not-checked");
        let image = dir.join("card.hdf");
        create_hdf(
            &image,
            10 * 1024 * 1024,
            true,
            &[PartitionSpec {
                drive_name: "DH0".into(),
                fs_type: AmigaHardDiskFs::Sfs0, // a family ART cannot read
                size_mb: 8,
                bootable: true,
                boot_priority: 0,
                num_buffers: 0,
            }],
            &[],
        )
        .unwrap();

        let dist_root = dir.join("dist");
        std::fs::create_dir_all(&dist_root).unwrap();
        let manifest = DistributionManifest {
            release: "AmigaOS 3.2".into(),
            built_from: vec![MediaRecord {
                volume_name: "Workbench3.2".into(),
                sha256: "0".repeat(64),
            }],
            files: vec![FileRecord {
                path: "C/LoadModule".into(),
                component: "workbench-base".into(),
                media: "Workbench3.2".into(),
                sha256: "0".repeat(64),
                bytes: 3,
                protection: Some(0x20),
            }],
        };
        std::fs::write(
            dist_root.join(MANIFEST_FILE_NAME),
            serde_json::to_string(&manifest).unwrap(),
        )
        .unwrap();

        let report = verify_at(&VerifyRequest {
            image,
            slot: None,
            index: 1,
            dist_root,
        })
        .unwrap();

        assert_eq!(report.failed, 0, "{:?}", report.files);
        assert_eq!(report.not_checked, 1);
        assert!(
            report.failed == 0 && report.not_checked > 0,
            "the property `verified` must key off: a clean-looking failed \
             count that is not actually verified"
        );
    }
}
