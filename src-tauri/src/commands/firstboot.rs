//! First boot: thin adapters over `core::firstboot`.
//!
//! Two of the three commands here write nothing to the Amiga side at all.
//! The third, [`firstboot_rehearse`], is a **job** (§54): it opens WinUAE
//! against a *copy* of the tree and waits for the Amiga's own report.
//!
//! ## A rehearsal promotes nothing
//!
//! `core::amigainstall::stage` exists to let an install run against a copy
//! and then replace the original with it. A rehearsal uses only the first
//! half: `commit` and `settle` are never called from this module, because
//! the copy is a machine ART booted, not a tree anyone asked for. On the one
//! ending where the copy has nothing left to say — [`RehearsalOutcome::Finished`]
//! — it is discarded; on the other three it is **kept**, and the result says
//! where, because a user told "it timed out" and not told where the log went
//! has been given nothing (CLAUDE.md, "never claim what you did not do").
//!
//! And when the discard itself fails, `discarded` stays `false` and the copy's
//! path is reported: no screen may say a removal happened that did not.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, State};

use super::amigainstall::profile_for;
use super::jobs::{spawn_job, JobRegistry};
use super::oplog::{user_operation, write_result, write_to_path};
use crate::core::amigainstall::rehearse::{rehearse_with, RehearsalOutcome, RehearseRequest};
use crate::core::amigainstall::run::{RealClock, RunLimits};
use crate::core::amigainstall::stage::stage_with;
use crate::core::error::{CoreError, CoreResult};
use crate::core::firstboot::cardread::{read_card_report, CardFirstBootReport};
use crate::core::firstboot::plan::{plan, FirstBootPlan, FirstBootRequest};
use crate::core::firstboot::write::{write, Written};
use crate::core::firstboot::DISPATCHER_PATH;
use crate::core::jobs::{JobId, ProgressSink};
use crate::core::oplog::{JsonlOperationLog, OperationOutcome};
use crate::core::profile::AmigaProfile;
use crate::core::winuae::detect_winuae;
use crate::error::{AppError, AppResult};
use crate::tools::winuae_launcher::WinUaeLauncher;

/// §92 PREVIEW: what a first boot would run. Writes nothing.
#[tauri::command]
pub fn firstboot_preview(tree: String) -> AppResult<FirstBootPlan> {
    Ok(plan(&FirstBootRequest {
        tree: PathBuf::from(tree.trim()),
    })?)
}

/// §92 APPLY: put the files into the tree. `Safe` — nothing of the user's is
/// overwritten except `S/User-Startup`, which is merged and backed up first.
#[tauri::command]
pub fn firstboot_write(tree: String, oplog: State<'_, JsonlOperationLog>) -> AppResult<Written> {
    let request = FirstBootRequest {
        tree: PathBuf::from(tree.trim()),
    };
    let result = plan(&request)
        .and_then(|p| write(&p))
        .map_err(AppError::from);
    write_result(
        &oplog,
        user_operation("Write the first-boot files into a tree").destination(&tree),
        &result,
        |record, done: &Written| {
            let record = record.detail("Files written", done.files.join(", "));
            match &done.user_startup_backup {
                Some(backup) => record.detail("User-Startup backup", backup.display().to_string()),
                None => record,
            }
        },
    );
    result
}

/// §9: read a card's first-boot report back — what the Amiga said the last
/// time it actually booted, straight off the card image. No card operation,
/// no boot, just files already there. `path` is never opened for writing.
///
/// The Amiga volume's own copy wins where both it and the FAT boot
/// partition's copy exist (spec §9) — see
/// [`crate::core::firstboot::cardread::read_card_report`] for the fallback
/// order.
#[tauri::command]
pub fn card_firstboot_report(path: String) -> AppResult<CardFirstBootReport> {
    Ok(read_card_report(Path::new(path.trim()))?)
}

// ---------------------------------------------------------------------------
// Rehearsing the first boot
// ---------------------------------------------------------------------------

/// What the frontend sends. Three strings, because everything else a
/// rehearsal needs — the scratch root, the emulator, the limits — is ART's
/// own to resolve.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RehearseRequestWire {
    /// The distribution tree. **Never written to**: the rehearsal boots a
    /// copy of it.
    pub tree: String,
    /// The user's own licensed Kickstart. ART ships none.
    pub kickstart: String,
    /// A machine preset id, or `None` for [`super::amigainstall::DEFAULT_PROFILE_ID`].
    pub profile: Option<String>,
}

// Deliberately **not** camelCased, exactly like `AmigaInstallResult`:
// `job_id` is what every job result in ART carries on the wire and what
// `src/lib/jobs.ts::awaitJobResult` matches on. The other three fields are
// single words with nothing to rename.
#[derive(Debug, Clone, Serialize)]
pub struct RehearsalResult {
    pub job_id: JobId,
    /// Which of the four endings it was. Mirrored exactly in TypeScript.
    pub outcome: RehearsalOutcome,
    /// The copy the rehearsal booted — where it still is when `discarded` is
    /// false, and where it *was* when it is true.
    pub copy: PathBuf,
    /// True only when the copy really was removed.
    pub discarded: bool,
}

/// The event a finished rehearsal's own answer arrives on.
pub const REHEARSAL_EVENT: &str = "firstboot-rehearsal-result";

/// What [`perform`] left on disk, beside the Amiga's answer.
#[derive(Debug)]
struct Rehearsed {
    outcome: RehearsalOutcome,
    copy: PathBuf,
    discarded: bool,
}

/// The ending, for the operation log. Four strings for four endings — the
/// same rule the screen follows, one layer down.
fn ending_of(outcome: &RehearsalOutcome) -> &'static str {
    match outcome {
        RehearsalOutcome::Finished { .. } => "every step ran",
        RehearsalOutcome::StepRefused { .. } => "a step refused",
        RehearsalOutcome::TimedOut { .. } => "timed out",
        RehearsalOutcome::EmulatorClosed { .. } => "the emulator was closed",
    }
}

/// Copy the tree, boot the copy, and decide what happens to the copy.
///
/// Split out of the command for the same reason `amigainstall::perform` is:
/// it is the part with the decisions in it, and it takes its emulator and
/// its tree as arguments so nothing here has to open a window to be read.
fn perform(
    tree: &Path,
    scratch_root: &Path,
    profile: &AmigaProfile,
    kickstart: &Path,
    emulator: &Path,
    sink: &dyn ProgressSink,
    copy_out: &mut Option<PathBuf>,
) -> CoreResult<Rehearsed> {
    let staged = stage_with(tree, sink)?;
    let copy = staged.copy_path().to_path_buf();
    // Set as soon as a copy exists, regardless of what `perform` returns —
    // an `Err` below still leaves the copy on disk (comment further down),
    // and the caller has no other way to learn where it is (I2, final
    // review). `stage_with` failing above never reaches this line, which is
    // correct: no copy was ever made, so there is nothing to report.
    *copy_out = Some(copy.clone());

    let request = RehearseRequest {
        tree_copy: &copy,
        scratch_root,
        profile,
        kickstart_path: kickstart,
        winuae_path: emulator,
        limits: RunLimits::default(),
    };

    // Built here, not in `core/`: constructing the real launcher is a
    // process-spawning decision, and `core/amigainstall::rehearse` may not
    // make it (ART-274) — it takes an `EmulatorLauncher` directly instead.
    let launcher = WinUaeLauncher::new(emulator, scratch_root);
    match rehearse_with(&request, &launcher, &RealClock::new(), sink) {
        // The one ending where the copy has told us everything it can.
        Ok(outcome @ RehearsalOutcome::Finished { .. }) => match staged.discard() {
            Ok(()) => Ok(Rehearsed {
                outcome,
                copy,
                discarded: true,
            }),
            // Never claim a discard that did not happen: the copy is still
            // there, the flag says so, and the reason reaches the job bar.
            Err(err) => {
                sink.report(
                    0,
                    None,
                    &format!(
                        "The rehearsal finished, but the copy at {} could not be removed: {err}",
                        copy.display()
                    ),
                );
                Ok(Rehearsed {
                    outcome,
                    copy,
                    discarded: false,
                })
            }
        },
        // Refused, timed out, closed: the copy is the evidence and it stays.
        Ok(outcome) => Ok(Rehearsed {
            outcome,
            copy,
            discarded: false,
        }),
        // A stopped rehearsal produced no answer, so there is no evidence to
        // keep — the same decision `amigainstall::perform` makes, and the one
        // path where a copy goes without an outcome having been reported.
        Err(CoreError::Cancelled) => {
            // Leftover round: this used to swallow a failed discard with
            // `let _ = staged.discard()`. The same rule the `Finished` arm's
            // own discard failure already follows above applies here too —
            // never claim a discard that did not happen, and a discard that
            // is never even reported is exactly that, by omission: the copy
            // is still on disk and nothing on screen says where.
            if let Err(err) = staged.discard() {
                sink.report(
                    0,
                    None,
                    &format!(
                        "the rehearsal was cancelled, but the copy at {} could not be removed: \
                         {err}",
                        copy.display()
                    ),
                );
            }
            Err(CoreError::Cancelled)
        }
        // An error part way is not nothing: whatever the Amiga wrote before
        // it is in the copy, so the copy stays. I2, final review: that used
        // to be true and unsaid — the copy survived, but neither the job bar
        // nor the oplog ever named it. Reported here the same way the three
        // non-`Finished` `Ok` endings already are.
        Err(err) => {
            sink.report(
                0,
                None,
                &format!(
                    "'{}' was not touched; the copy the rehearsal booted is kept at '{}'",
                    tree.display(),
                    copy.display()
                ),
            );
            Err(err)
        }
    }
}

/// Boot a copy of the tree under WinUAE and read the Amiga's own first-boot
/// report. Returns a job id (§54); the answer arrives on [`REHEARSAL_EVENT`].
///
/// Everything refusable is refused **here**, before the job starts: a tree
/// with no first boot in it, an unknown machine id, a missing emulator, a
/// scratch root that has gone away. A rehearsal of a tree that carries no
/// dispatcher would boot, find nothing to run, and report `done all` about
/// nothing — a confident wrong sentence, which is the class of defect this
/// project pays most for.
#[tauri::command]
pub fn firstboot_rehearse(
    request: RehearseRequestWire,
    winuae_path: Option<String>,
    app: AppHandle,
    registry: State<'_, Arc<JobRegistry>>,
    oplog: State<'_, JsonlOperationLog>,
) -> AppResult<JobId> {
    let tree = PathBuf::from(request.tree.trim());
    if !tree.join(DISPATCHER_PATH).is_file() {
        return Err(CoreError::InvalidInput(
            "the tree carries no first boot yet — write it first".to_string(),
        )
        .into());
    }

    let profile = profile_for(request.profile.as_deref())?;
    let kickstart = PathBuf::from(request.kickstart.trim());
    let emulator = detect_winuae(winuae_path.as_deref())
        .executable_path
        .ok_or_else(|| {
            CoreError::InvalidInput(
                "WinUAE was not found in a standard install location — set its path in Settings"
                    .to_string(),
            )
        })?;
    let emulator = PathBuf::from(emulator);

    // Resolved here rather than inside the job: a scratch root that has gone
    // away is the user's to fix, and they should hear it from the button they
    // pressed (ART-196).
    let scratch_root = crate::scratch::root()?;

    let log_path = oplog.path().to_path_buf();
    let emit_app = app.clone();
    let for_log = tree.display().to_string();
    let machine = profile.id.clone();
    let title = format!("Rehearsing the first boot on {}", profile.name);

    let id = spawn_job(
        &app,
        Arc::clone(&registry),
        &title,
        move |job_id, progress| {
            let mut copy_path: Option<PathBuf> = None;
            let result = perform(
                &tree,
                &scratch_root,
                &profile,
                &kickstart,
                &emulator,
                progress,
                &mut copy_path,
            );

            // §53. Best-effort, and never able to fail the operation it
            // describes. The destination is the **copy**: nothing here writes
            // to the tree at `source`.
            let record = user_operation("Rehearse a first boot under WinUAE")
                .source(for_log.clone())
                .detail("Machine", machine);
            let record = match &result {
                Ok(done) => {
                    let record = record
                        .destination(done.copy.display().to_string())
                        .detail("Ending", ending_of(&done.outcome))
                        .detail(
                            "Copy",
                            if done.discarded {
                                "discarded".to_string()
                            } else {
                                format!("kept at {}", done.copy.display())
                            },
                        );
                    // `verified`, never `success`: the answer is the Amiga's
                    // own report, not something ART inspected (§89).
                    record.outcome(OperationOutcome::verified(matches!(
                        done.outcome,
                        RehearsalOutcome::Finished { .. }
                    )))
                }
                // I2, final review: a failed rehearsal still made a copy in
                // every case except staging itself failing — `copy_path` is
                // `perform`'s own answer to "where", set the moment one
                // exists regardless of what it later returns. Naming it here
                // is what makes the failure record match the three
                // non-`Finished` `Ok` records above, which already do.
                Err(err) => match &copy_path {
                    Some(copy) => record.destination(copy.display().to_string()).failed(err),
                    None => record.failed(err),
                },
            };
            write_to_path(&log_path, &record);

            let done = result?;
            let _ = emit_app.emit(
                REHEARSAL_EVENT,
                RehearsalResult {
                    job_id,
                    outcome: done.outcome,
                    copy: done.copy,
                    discarded: done.discarded,
                },
            );
            Ok(())
        },
    );

    Ok(id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::firstboot::plan::{FatMount, PlannedStep};
    use crate::core::firstboot::report::{Ending, FirstBootReport, StepOutcome};
    use crate::core::firstboot::{REPORT_PATH, STEP_DIR};
    use crate::core::jobs::NoProgress;
    use crate::core::ScratchDir;
    use std::time::Duration;

    #[test]
    fn the_plan_crosses_the_wire_in_camel_case_with_kebab_tags() {
        let p = FirstBootPlan {
            tree: "x".into(),
            steps: vec![PlannedStep {
                name: "10-hardware".into(),
                tree_path: "S/FirstBoot/10-hardware".into(),
                fixed: true,
            }],
            fat_mount: FatMount::Unavailable { needs: "fat95" },
            user_startup_exists: false,
            already_written: false,
            bytes_added: 1,
        };
        let json = serde_json::to_value(&p).unwrap();
        assert_eq!(json["fatMount"]["kind"], "unavailable");
        assert_eq!(json["fatMount"]["needs"], "fat95");
        assert_eq!(json["userStartupExists"], false);
        assert_eq!(json["alreadyWritten"], false);
        assert_eq!(json["bytesAdded"], 1);
        assert_eq!(json["steps"][0]["treePath"], "S/FirstBoot/10-hardware");
    }

    /// The result's own wire shape. `job_id` stays snake_case — `jobs.ts`
    /// matches on it — while the outcome keeps `rehearse.rs`'s kebab tags,
    /// which is what `src/lib/firstboot.ts` switches on.
    #[test]
    fn the_result_crosses_the_wire_with_a_snake_case_job_id_and_a_kebab_outcome() {
        let result = RehearsalResult {
            job_id: 7,
            outcome: RehearsalOutcome::TimedOut {
                waited: Duration::from_secs(90),
                report: FirstBootReport {
                    version: Some(1),
                    system: None,
                    steps: Vec::new(),
                    ending: Ending::Unfinished,
                    fat_copy_failed: false,
                    reboot_requested_by: None,
                    unknown: Vec::new(),
                },
            },
            copy: PathBuf::from("E:/amiga/tree.art-staged-1"),
            discarded: false,
        };

        let json = serde_json::to_value(&result).unwrap();

        assert_eq!(json["job_id"], 7, "jobs.ts matches on job_id");
        assert!(json.get("jobId").is_none(), "and never on jobId");
        assert_eq!(json["outcome"]["kind"], "timed-out");
        assert_eq!(json["outcome"]["waited"]["secs"], 90);
        assert_eq!(json["outcome"]["report"]["ending"], "unfinished");
        assert_eq!(json["copy"], "E:/amiga/tree.art-staged-1");
        assert_eq!(json["discarded"], false);
    }

    /// Each ending gets its own sentence in the log, for the same reason the
    /// screen does.
    #[test]
    fn every_ending_is_logged_as_a_different_sentence() {
        let report = FirstBootReport {
            version: Some(1),
            system: None,
            steps: Vec::new(),
            ending: Ending::DoneAll,
            fat_copy_failed: false,
            reboot_requested_by: None,
            unknown: Vec::new(),
        };
        let endings = [
            RehearsalOutcome::Finished {
                report: report.clone(),
            },
            RehearsalOutcome::StepRefused {
                report: report.clone(),
            },
            RehearsalOutcome::TimedOut {
                waited: Duration::from_secs(1),
                report: report.clone(),
            },
            RehearsalOutcome::EmulatorClosed {
                waited: Duration::from_secs(1),
                report,
            },
        ];
        let said: Vec<&str> = endings.iter().map(ending_of).collect();
        assert_eq!(
            said.iter().collect::<std::collections::BTreeSet<_>>().len(),
            4,
            "got {said:?}"
        );
    }

    /// **Phase 2's closing measurement** (spec §11 item 2), and the only test
    /// in ART that opens WinUAE against the owner's own material.
    ///
    /// It never touches the tree it is pointed at: `stage_with` copies it,
    /// the first-boot files are written into the **copy**, and the copy is
    /// what boots. A failure keeps the copy and prints where it is.
    ///
    /// ```text
    /// ART_FIRSTBOOT_TREE=E:/amiga/ProjeART/art205-32 \
    /// ART_FIRSTBOOT_ROM="E:/amiga/Amigatolon/paketler/3.2/AmigaOs 3.2/ROM/kicka1200.rom" \
    /// ART_WINUAE="C:/Program Files/WinUAE/winuae64.exe" \
    /// cargo test rehearse_the_real_tree_when_asked -- --ignored --nocapture
    /// ```
    #[test]
    #[ignore = "opens WinUAE against the owner's own tree and ROM; run explicitly"]
    fn rehearse_the_real_tree_when_asked() {
        let (tree, rom, winuae) = match (
            std::env::var("ART_FIRSTBOOT_TREE"),
            std::env::var("ART_FIRSTBOOT_ROM"),
            std::env::var("ART_WINUAE"),
        ) {
            (Ok(tree), Ok(rom), Ok(winuae)) => (
                PathBuf::from(tree),
                PathBuf::from(rom),
                PathBuf::from(winuae),
            ),
            _ => {
                println!(
                    "skipped: set ART_FIRSTBOOT_TREE, ART_FIRSTBOOT_ROM and ART_WINUAE \
                     (and optionally ART_FIRSTBOOT_PROFILE) to rehearse a real tree"
                );
                return;
            }
        };
        // `a1200` is not a preset id — the AGA A1200 is `a1200-aga`, which is
        // also `profile_for`'s own default, so `None` lands on it.
        let profile = profile_for(std::env::var("ART_FIRSTBOOT_PROFILE").ok().as_deref()).unwrap();
        let scratch = ScratchDir::new("art-firstboot-real", "rehearse");

        let staged = stage_with(&tree, &NoProgress).expect("copying the tree");
        let copy = staged.copy_path().to_path_buf();
        println!("tree: {}", tree.display());
        println!("copy: {}", copy.display());
        println!("rom: {}", rom.display());
        println!("machine: {} ({})", profile.name, profile.id);

        let planned = plan(&FirstBootRequest { tree: copy.clone() }).expect("planning");
        let written = write(&planned).expect("writing the first boot into the copy");
        println!("wrote {} files", written.files.len());

        let launcher = WinUaeLauncher::new(&winuae, scratch.path());
        let outcome = rehearse_with(
            &RehearseRequest {
                tree_copy: &copy,
                scratch_root: scratch.path(),
                profile: &profile,
                kickstart_path: &rom,
                winuae_path: &winuae,
                limits: RunLimits::default(),
            },
            &launcher,
            &RealClock::new(),
            &NoProgress,
        )
        .expect("the rehearsal itself");

        println!("--- {REPORT_PATH} ---");
        println!(
            "{}",
            std::fs::read_to_string(copy.join(REPORT_PATH))
                .unwrap_or_else(|err| format!("(unreadable: {err})"))
        );
        println!("--- as parsed ---");
        println!("{outcome:#?}");
        println!("the copy is at {} until this test passes", copy.display());
        // `done all` ends with the dispatcher deleting itself while it is
        // still being executed; whether AmigaDOS allows that is measured
        // here, not assumed (the plan's `already_written` reads the file).
        println!(
            "dispatcher still present after done all: {}",
            copy.join(DISPATCHER_PATH).is_file()
        );

        let report = match &outcome {
            RehearsalOutcome::Finished { report }
            | RehearsalOutcome::StepRefused { report }
            | RehearsalOutcome::TimedOut { report, .. }
            | RehearsalOutcome::EmulatorClosed { report, .. } => report,
        };
        let names: Vec<&str> = report.steps.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(
            names,
            ["10-hardware", "20-aux", "30-datatypes"],
            "the three steps, in order"
        );
        // Under an emulator the two hardware steps say so themselves, and
        // `30-datatypes` skips for want of `C:spatch` (or of patches to apply
        // when the tree carries one). Nothing here is the Pi3/Pi4 branch:
        // that closes on a real card only.
        for (step, expected) in
            report
                .steps
                .iter()
                .zip([vec!["uae"], vec!["uae"], vec!["no-spatch", "no-patches"]])
        {
            match &step.outcome {
                StepOutcome::Skipped { reason } => assert!(
                    expected.contains(&reason.as_str()),
                    "{} skipped '{reason}', expected one of {expected:?}",
                    step.name
                ),
                other => panic!("{} ended as {other:?}, expected a skip", step.name),
            }
        }
        assert!(
            matches!(outcome, RehearsalOutcome::Finished { .. }),
            "expected every step to run and the dispatcher to write `done all`"
        );
        assert!(
            !copy.join(STEP_DIR).exists(),
            "`done all` removes S/FirstBoot/, which is what makes the next boot inert"
        );

        staged.discard().expect("removing the copy");
        println!("the copy has been discarded");
    }
}
