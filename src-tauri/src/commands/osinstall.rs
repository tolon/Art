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
//!
//! ## Fix round 1 — the outbound direction, pinned for real
//!
//! Review found a live wire mismatch (`VerifyReport::not_checked` had no
//! `camelCase` rename, so `src/lib/osinstall.ts`'s `report.notChecked` was
//! always `undefined`) that the Task 12 wire test could not have caught: that
//! test only deserialises a payload the frontend *sends* — the inbound
//! direction. Nothing pinned what Rust *serialises back out*. The
//! `wire_shapes` test module below fixes that: every response type is
//! checked with `serde_json::to_value` against the exact key names
//! `src/lib/osinstall.ts` declares, so a missing or wrong `rename_all` (or a
//! field renamed on one side only) fails a test instead of shipping.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, State};

use crate::core::error::CoreError;
use crate::core::oplog::{JsonlOperationLog, OperationOutcome, OperationRecord};
use crate::core::osinstall::apply::{
    apply, ApplyOutcome, DistributionManifest, MANIFEST_FILE_NAME,
};
use crate::core::osinstall::plan::{plan, InstallPlan, InstallRequest};
use crate::core::osinstall::recipe;
use crate::core::osinstall::scan::{find_media, FoundMedia};
use crate::core::osinstall::verify::{verify_volume, VerifyReport};
use crate::error::{AppError, AppResult};

use super::jobs::{spawn_job, JobRegistry};
use super::oplog::{user_operation, write, write_to_path};

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
///
/// Only `find_media`'s own `CoreError::Io` becomes `FolderUnreadable` — that
/// is the only variant it can actually produce (`std::fs::read_dir` and the
/// directory-listing iterator are its sole fallible steps; every other file
/// it looks at is skipped, never propagated — see the module doc on
/// `core/osinstall/scan.rs`). A blanket `Err(_)` would have silently
/// relabelled any future, differently-shaped error the same way; matching
/// the one variant that can occur means a new kind of failure surfaces as
/// itself instead of being folded into "bad folder".
#[tauri::command]
pub fn osinstall_scan_media(folder: PathBuf) -> AppResult<MediaScanResult> {
    match find_media(&folder) {
        Ok(media) => Ok(MediaScanResult::Found { media }),
        Err(CoreError::Io(_)) => Ok(MediaScanResult::FolderUnreadable {
            folder: folder.display().to_string(),
        }),
        Err(other) => Err(other.into()),
    }
}

// ---------------------------------------------------------------------------
// osinstall_plan
// ---------------------------------------------------------------------------

/// What planning an install found, or why the media folder itself could not
/// be looked at.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "outcome", rename_all = "kebab-case")]
pub enum PlanResult {
    // `Box`ed: G9's `paired_rom` pushed `InstallPlan` past clippy's
    // `large_enum_variant` threshold next to `FolderUnreadable`'s bare
    // `String`. Serde serialises `Box<T>` exactly as it would `T`, so this
    // changes nothing on the wire.
    Planned { plan: Box<InstallPlan> },
    FolderUnreadable { folder: String },
}

/// What installing the chosen components would do — or every reason it
/// cannot. Writes nothing (§92's PREVIEW).
///
/// `request.release` names which shipped recipe to plan from
/// (`recipe::by_release`) — refused, never defaulted, when it names a
/// release ART ships no recipe for: a default here would mean a caller
/// asking for one operating system and getting another written onto the
/// user's volume.
///
/// **Opens the media folder twice** — once in the guard below, once again
/// inside `plan()`'s own call to `find_media`. Accepted rather than fixed:
/// the alternative is threading a pre-scanned `Vec<FoundMedia>` into `plan()`
/// itself, which is `core/osinstall/plan.rs`'s call to make, not this
/// adapter's, and a floppy-sized image's "windowed" read is cheap enough
/// (see `scan.rs`'s own module doc: the window *is* the whole file for
/// anything ADF-sized) that a real media folder costs a second pass of a few
/// milliseconds, not a second scan of the disk.
#[tauri::command]
pub fn osinstall_plan(request: InstallRequest) -> AppResult<PlanResult> {
    // The same folder `plan()` would open through `find_media` — checked
    // here first so a bad path reaches the screen as a value it can
    // translate, never as `find_media`'s own English sentence. See the
    // module doc comment. Narrowed to `CoreError::Io` for the same reason
    // `osinstall_scan_media` narrows it — see that command's own comment.
    if let Err(CoreError::Io(_)) = find_media(&request.media_folder) {
        return Ok(PlanResult::FolderUnreadable {
            folder: request.media_folder.display().to_string(),
        });
    }
    let recipe = recipe::by_release(&request.release)?;
    Ok(PlanResult::Planned {
        plan: Box::new(plan(&request, &recipe)?),
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
        //
        // `source` is every medium the plan actually resolved, not the media
        // folder as a whole — `plan.media_paths` already names exactly the
        // files this run read from, so there is no reason to guess at a
        // single "the source" the way a one-image operation would.
        let source = plan
            .media_paths
            .values()
            .map(|path| path.display().to_string())
            .collect::<Vec<_>>()
            .join(", ");
        let record = user_operation("Build an AmigaOS distribution tree")
            .source(source)
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

/// The record `osinstall_verify` writes, built from the outcome alone.
///
/// Factored out from the command (fix round 1) so requirement 5's own
/// property — `verified` is `failed == 0 && not_checked == 0`, **never**
/// `failed == 0` alone, because "ART did not look" is not "ART found nothing
/// wrong" (§89) — can be tested directly against a real `VerifyReport`
/// without a live Tauri `State` to write through. The record carries all
/// three counts as details too, so the log agrees with what the screen
/// shows, not just with the one boolean.
fn verify_record(
    dist_root: &str,
    image: &str,
    result: &AppResult<VerifyReport>,
) -> OperationRecord {
    let record = user_operation("Verify an AmigaOS install against its manifest")
        .source(dist_root)
        .destination(image);
    match result {
        Ok(report) => record
            .detail("Passed", report.passed.to_string())
            .detail("Failed", report.failed.to_string())
            .detail("Not checked", report.not_checked.to_string())
            .outcome(OperationOutcome::verified(
                report.failed == 0 && report.not_checked == 0,
            )),
        Err(err) => record.failure(err.code(), err.to_string()),
    }
}

/// Read the volume back and check it against the manifest `osinstall_apply`
/// wrote (§92's VERIFY step, Task 10's `verify_volume`). See
/// [`verify_record`] for what gets logged and why.
#[tauri::command]
pub fn osinstall_verify(
    request: VerifyRequest,
    oplog: State<'_, JsonlOperationLog>,
) -> AppResult<VerifyReport> {
    let image = request.image.display().to_string();
    let dist_root = request.dist_root.display().to_string();
    let result = verify_at(&request);

    write(&oplog, verify_record(&dist_root, &image, &result));

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
            "excluded": ["modules-a1200"],
            "destination": "E:\\dist",
            "release": "AmigaOS 3.2"
        }"#;
        let request: InstallRequest = serde_json::from_str(json).unwrap();
        assert_eq!(request.chosen.len(), 2);
        assert_eq!(request.excluded, vec!["modules-a1200".to_string()]);
    }

    /// The request carries the release, and an unknown one is refused before
    /// any media is touched.
    #[test]
    fn a_plan_request_names_the_release_it_wants() {
        let json = r#"{
            "mediaFolder": "C:\\nowhere",
            "rom": null,
            "chosen": [],
            "excluded": [],
            "destination": "C:\\nowhere\\dist",
            "release": "AmigaOS 3.9"
        }"#;
        let request: InstallRequest =
            serde_json::from_str(json).expect("the wire shape must carry release");
        assert_eq!(request.release, "AmigaOS 3.9");
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
            release: "AmigaOS 3.2".to_string(),
            media_folder: missing.clone(),
            rom: None,
            chosen: vec!["workbench-base".to_string()],
            excluded: Vec::new(),
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
            release: "AmigaOS 3.2".to_string(),
            media_folder: dir.clone(),
            rom: None,
            chosen: vec!["workbench-base".to_string()],
            excluded: Vec::new(),
            destination: dir.join("dist"),
        })
        .unwrap();

        match result {
            PlanResult::Planned { plan } => assert_eq!(plan.release, "AmigaOS 3.2"),
            other => panic!("expected Planned, got {other:?}"),
        }
    }

    // ---- Requirement 5, tested directly (fix round 1) ----
    //
    // The earlier version of this coverage called `verify_at` and inspected
    // `VerifyReport` alone — that is `verify_volume`, which Task 10 already
    // covers, and its own final assertion followed from the two
    // `assert_eq!`s above it. Requirement 5 names the oplog *record*, not
    // the report, so these test `verify_record` — the function that decides
    // what `osinstall_verify` actually logs — directly, against every one
    // of the three shapes a `VerifyReport` can take.

    fn one_file_report(state: crate::core::osinstall::verify::CheckState) -> VerifyReport {
        use crate::core::osinstall::verify::FileVerdict;
        let (passed, failed, not_checked) = match state {
            crate::core::osinstall::verify::CheckState::Pass => (1, 0, 0),
            crate::core::osinstall::verify::CheckState::Fail => (0, 1, 0),
            crate::core::osinstall::verify::CheckState::NotChecked => (0, 0, 1),
        };
        VerifyReport {
            files: vec![FileVerdict {
                path: "C/LoadModule".into(),
                state,
                detail: Some("detail".into()),
            }],
            passed,
            failed,
            not_checked,
        }
    }

    /// The property §89 is about: a report with nothing failed but something
    /// not-checked must not be logged as verified. A `verified(failed == 0)`
    /// regression passes every other test in this module but flips this one.
    #[test]
    fn a_report_with_something_not_checked_is_not_verified_in_the_log() {
        use crate::core::osinstall::verify::CheckState;

        let report = one_file_report(CheckState::NotChecked);
        let record = verify_record("dist", "image.hdf", &Ok(report));

        assert_eq!(
            record.outcome,
            OperationOutcome::Success {
                verification: Some(false)
            },
            "{record:?}"
        );
    }

    #[test]
    fn a_fully_passing_report_is_verified_in_the_log() {
        use crate::core::osinstall::verify::CheckState;

        let report = one_file_report(CheckState::Pass);
        let record = verify_record("dist", "image.hdf", &Ok(report));

        assert_eq!(
            record.outcome,
            OperationOutcome::Success {
                verification: Some(true)
            },
            "{record:?}"
        );
    }

    #[test]
    fn a_report_with_a_real_failure_is_not_verified_in_the_log() {
        use crate::core::osinstall::verify::CheckState;

        let report = one_file_report(CheckState::Fail);
        let record = verify_record("dist", "image.hdf", &Ok(report));

        assert_eq!(
            record.outcome,
            OperationOutcome::Success {
                verification: Some(false)
            },
            "{record:?}"
        );
    }

    /// `verify_at` itself still gets one end-to-end exercise — a genuinely
    /// unreadable filesystem family really does come back `not_checked`, not
    /// `failed` — but the property under test here is only that `verify_at`
    /// runs and returns a real report; requirement 5's own claim is proved
    /// by the three tests above, against `verify_record` directly.
    #[test]
    fn verify_at_reports_an_unreadable_family_as_not_checked() {
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
            paired_rom: None,
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
    }

    // -------------------------------------------------------------------
    // Outbound wire shapes (fix round 1) — every response type, checked
    // with `serde_json::to_value` against the exact keys and tag spellings
    // `src/lib/osinstall.ts` declares. This is what would have caught
    // `VerifyReport::not_checked` shipping without its `camelCase` rename:
    // the Task 12 wire test only pins the inbound direction (what Rust
    // *deserialises*), and a round trip through the same Rust types (as
    // `the_plan_the_frontend_sends_back_deserialises_into_an_apply_request`
    // does) cannot catch a `rename_all` mistake either, because both sides
    // of that round trip move together. These tests instead compare against
    // literal strings, the same way the coordinator's own review did by
    // hand.
    // -------------------------------------------------------------------
    mod wire_shapes {
        use super::*;
        use std::collections::BTreeSet;

        fn key_set(value: &serde_json::Value) -> BTreeSet<String> {
            value
                .as_object()
                .unwrap_or_else(|| panic!("expected a JSON object, got {value}"))
                .keys()
                .cloned()
                .collect()
        }

        fn expect_keys(value: &serde_json::Value, expected: &[&str]) {
            let got = key_set(value);
            let want: BTreeSet<String> = expected.iter().map(|s| s.to_string()).collect();
            assert_eq!(got, want, "value was: {value}");
        }

        /// The TS mirror (`src/lib/osinstall.ts`) is maintained by hand, not
        /// generated from this type — nothing here can see it, so this test
        /// only pins the Rust side's own wire shape, not a cross-check
        /// against the frontend's declared keys.
        #[test]
        fn found_media_serializes_with_the_keys_this_test_pins() {
            use crate::core::osinstall::scan::MediaKind;

            let media = FoundMedia {
                path: PathBuf::from("E:\\wb.adf"),
                volume_name: "Workbench3.2".into(),
                kind: MediaKind::Floppy,
            };
            let value = serde_json::to_value(&media).unwrap();
            expect_keys(&value, &["path", "volumeName", "kind"]);
            assert_eq!(value["kind"], "floppy");
        }

        /// The regression this whole module exists to prevent: without
        /// `VerifyReport`'s `rename_all = "camelCase"`, this key would be
        /// `not_checked` and `value.get("notChecked")` would be `None`.
        #[test]
        fn verify_report_serializes_not_checked_as_camelcase() {
            use crate::core::osinstall::verify::{CheckState, FileVerdict};

            let report = VerifyReport {
                files: vec![FileVerdict {
                    path: "C/LoadModule".into(),
                    state: CheckState::NotChecked,
                    detail: Some("why".into()),
                }],
                passed: 0,
                failed: 0,
                not_checked: 1,
            };
            let value = serde_json::to_value(&report).unwrap();

            expect_keys(&value, &["files", "passed", "failed", "notChecked"]);
            assert_eq!(value["notChecked"], 1);
            assert!(
                value.get("not_checked").is_none(),
                "the un-camelCased name must not leak onto the wire: {value}"
            );

            let verdict = &value["files"][0];
            expect_keys(verdict, &["path", "state", "detail"]);
            assert_eq!(verdict["state"], "not-checked");
        }

        #[test]
        fn apply_outcome_serializes_with_the_keys_the_frontend_declares() {
            let outcome = ApplyOutcome {
                root: PathBuf::from("E:\\dist"),
                files: 3,
                directories: 1,
                bytes: 42,
            };
            let value = serde_json::to_value(&outcome).unwrap();
            expect_keys(&value, &["root", "files", "directories", "bytes"]);
        }

        /// Deliberately **not** camelCased — `job_id` matches `LayoutResult`
        /// and `PreloadResult`, which do the same, and `src/lib/osinstall.ts`
        /// declares `job_id` to match. Pinned so a future edit cannot drift
        /// one side without the other noticing.
        #[test]
        fn os_install_result_keeps_job_id_unrenamed_like_its_siblings() {
            let result = OsInstallResult {
                job_id: 7,
                destination: "E:\\dist".into(),
                outcome: ApplyOutcome::default(),
            };
            let value = serde_json::to_value(&result).unwrap();
            expect_keys(&value, &["job_id", "destination", "outcome"]);
        }

        #[test]
        fn install_plan_top_level_keys_are_camelcase() {
            let (plan, _dir) = crate::core::osinstall::fixtures::planned_with(
                &["workbench-base"],
                &["Workbench3.2"],
                Some(47),
            );
            let value = serde_json::to_value(&plan).unwrap();
            expect_keys(
                &value,
                &[
                    "release",
                    "items",
                    "refusals",
                    "totalBytes",
                    "componentsOn",
                    "pairedRom",
                    "mediaPaths",
                    "userStartup",
                ],
            );
        }

        #[test]
        fn plan_item_serializes_is_dir_as_camelcase() {
            use crate::core::osinstall::plan::PlanItem;

            let item = PlanItem {
                component: "workbench-base".into(),
                media: "Workbench3.2".into(),
                from: "C".into(),
                to: "C".into(),
                is_dir: true,
                bytes: 0,
            };
            let value = serde_json::to_value(&item).unwrap();
            expect_keys(
                &value,
                &["component", "media", "from", "to", "isDir", "bytes"],
            );
            assert_eq!(value["isDir"], true);
        }

        #[test]
        fn user_startup_contribution_serializes_with_the_keys_the_frontend_declares() {
            use crate::core::osinstall::plan::UserStartupContribution;

            let contribution = UserStartupContribution {
                component: "amissl".into(),
                lines: vec!["Assign AmiSSL: SYS:".into()],
            };
            let value = serde_json::to_value(&contribution).unwrap();
            expect_keys(&value, &["component", "lines"]);
        }

        #[test]
        fn media_scan_result_tag_and_field_spellings() {
            let found = MediaScanResult::Found { media: vec![] };
            let value = serde_json::to_value(&found).unwrap();
            assert_eq!(value["outcome"], "found");
            expect_keys(&value, &["outcome", "media"]);

            let unreadable = MediaScanResult::FolderUnreadable {
                folder: "E:\\x".into(),
            };
            let value = serde_json::to_value(&unreadable).unwrap();
            assert_eq!(value["outcome"], "folder-unreadable");
            expect_keys(&value, &["outcome", "folder"]);
        }

        #[test]
        fn plan_result_tag_and_field_spellings() {
            let (plan, _dir) = crate::core::osinstall::fixtures::planned_with(
                &["workbench-base"],
                &["Workbench3.2"],
                Some(47),
            );
            let value = serde_json::to_value(&PlanResult::Planned {
                plan: Box::new(plan),
            })
            .unwrap();
            assert_eq!(value["outcome"], "planned");
            assert!(value.get("plan").is_some());

            let value = serde_json::to_value(&PlanResult::FolderUnreadable {
                folder: "E:\\x".into(),
            })
            .unwrap();
            assert_eq!(value["outcome"], "folder-unreadable");
            expect_keys(&value, &["outcome", "folder"]);
        }

        /// **G9 fix round.** `install_plan_top_level_keys_are_camelcase`
        /// only checks `InstallPlan`'s own top-level keys, so it never looked
        /// inside `pairedRom` at all — `rename_all = "camelCase"` on a
        /// container does not propagate to a nested struct's own fields, and
        /// `PairedRom` shipped without its own `rename_all` attribute in
        /// review. Checked in both places `PairedRom` is nested — `InstallPlan`
        /// (the plan the frontend receives from `osinstall_plan`) and
        /// `DistributionManifest` (`distribution.json`, read back by a later
        /// task's own card-time check, which is exactly why the coordinator
        /// wants this camelCase everywhere `PairedRom` lands) — so a fix that
        /// only renamed one container's field would still be caught here.
        #[test]
        fn paired_rom_nested_inside_a_plan_or_a_manifest_serialises_with_camelcase_keys() {
            let (plan, _dir) = crate::core::osinstall::fixtures::planned_with(
                &["workbench-base"],
                &["Workbench3.2"],
                Some(47),
            );
            let paired = plan
                .paired_rom
                .clone()
                .expect("planned_with a ROM records the pairing");

            let plan_value = serde_json::to_value(&plan).unwrap();
            expect_keys(
                &plan_value["pairedRom"],
                &[
                    "name",
                    "sha256",
                    "statedMajor",
                    "compatibleModels",
                    "requiresMajor",
                ],
            );

            let manifest = DistributionManifest {
                release: "AmigaOS 3.2".into(),
                built_from: Vec::new(),
                files: Vec::new(),
                paired_rom: Some(paired),
            };
            let manifest_value = serde_json::to_value(&manifest).unwrap();
            expect_keys(
                &manifest_value["pairedRom"],
                &[
                    "name",
                    "sha256",
                    "statedMajor",
                    "compatibleModels",
                    "requiresMajor",
                ],
            );
        }

        #[test]
        fn rule_kind_spellings() {
            use crate::core::osinstall::RuleKind;

            assert_eq!(serde_json::to_value(RuleKind::File).unwrap(), "file");
            assert_eq!(serde_json::to_value(RuleKind::Subtree).unwrap(), "subtree");
        }

        /// `rename_all = "kebab-case"` on `RefusalReason` renames the
        /// **variant** (the `refusal` tag) only — struct-variant field names
        /// (`volume_name`, and so on) are untouched by it, which is the one
        /// place this whole wire does *not* use `camelCase`. Confirmed here
        /// against literal strings rather than assumed, for every variant.
        #[test]
        fn refusal_reason_tag_and_field_spellings_for_every_variant() {
            use crate::core::osinstall::{RefusalReason, RuleKind};

            let cases: Vec<(RefusalReason, &str, &[&str])> = vec![
                (
                    RefusalReason::MediaMissing {
                        component: "extras".into(),
                        volume_name: "Extras3.2".into(),
                    },
                    "media-missing",
                    &["refusal", "component", "volume_name"],
                ),
                (
                    RefusalReason::MediaPathMissing {
                        component: "extras".into(),
                        media: "Extras3.2".into(),
                        path: "L".into(),
                    },
                    "media-path-missing",
                    &["refusal", "component", "media", "path"],
                ),
                (RefusalReason::RomUnknown, "rom-unknown", &["refusal"]),
                (
                    RefusalReason::DestinationCollision {
                        path: "C/Assign".into(),
                        components: vec!["a".into(), "b".into()],
                    },
                    "destination-collision",
                    &["refusal", "path", "components"],
                ),
                (
                    RefusalReason::MediaAmbiguous {
                        component: "workbench-base".into(),
                        volume_name: "Workbench3.2".into(),
                        paths: vec!["a".into()],
                    },
                    "media-ambiguous",
                    &["refusal", "component", "volume_name", "paths"],
                ),
                (
                    RefusalReason::ExclusiveGroupConflict {
                        group: "modules".into(),
                        components: vec!["a".into(), "b".into()],
                    },
                    "exclusive-group-conflict",
                    &["refusal", "group", "components"],
                ),
                (
                    RefusalReason::RuleKindMismatch {
                        component: "a".into(),
                        from: "C".into(),
                        expected: RuleKind::File,
                        found: RuleKind::Subtree,
                    },
                    "rule-kind-mismatch",
                    &["refusal", "component", "from", "expected", "found"],
                ),
            ];

            for (reason, tag, fields) in cases {
                let value = serde_json::to_value(&reason).unwrap();
                assert_eq!(value["refusal"], tag, "{value}");
                expect_keys(&value, fields);
            }
        }
    }
}
