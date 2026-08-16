//! Formatting a card's Amiga volumes and filling them (SD-2 · G3/G5, route E
//! and native).
//!
//! Three commands and one rule: **nothing is formatted until the user has seen
//! what would be** (§92). `preload_plan` writes nothing and answers what would
//! run; `preload_run` does it as a job.
//!
//! Formatting is `Destructive` — a partition that had something in it does not
//! afterwards. There is no backup step and there deliberately is not one: the
//! volumes this is aimed at are tens of gigabytes and ART's own build made
//! them empty a moment ago. The guard is the preview and the confirmation, and
//! the plan says how many partitions it would erase.
//!
//! ## ART-120 — native by default, `hst-imager` a named fallback
//!
//! Until this fix, `preload_run` constructed `HstImager::at(...)`
//! unconditionally: `core::preload::native::NativeFormatter` existed, had its
//! own tests and its own oracle, and was unreachable from the product — every
//! preload still needed `hst.imager.exe` on the machine. The decision this
//! module now carries out: the native path runs unless ART already knows it
//! cannot do the job, and when it cannot, ART falls back and says which tool
//! did which step and why.
//!
//! **Two, and only two, known capability gaps make the native path fall
//! back** — both filed, neither guessed at:
//!
//! - **ART-113**: `libpfs3` 0.1.3 writes a name as UTF-8 and reads it back as
//!   Latin-1, so any non-ASCII AmigaDOS name cannot round-trip.
//!   `NativeFormatter::copy_in`'s own pre-flight check
//!   (`core::preload::native::non_ascii_entries`) refuses with
//!   [`CoreError::NonAsciiPfs3Names`] before the PFS3 volume is even opened —
//!   nothing is written by the failed attempt.
//! - **ART-117**: embedding a filesystem driver into an existing card's RDB
//!   in place needs to edit a partition table ART did not build.
//!   `NativeFormatter::import_filesystem` refuses unconditionally, for every
//!   card, with [`CoreError::ForeignRdbEmbedNotSupported`], before touching
//!   the image at all.
//!
//! Both refusals are therefore safe to treat as "try the other tool": the
//! attempt that failed left nothing behind to clean up or roll back.
//!
//! **The choice is made per step, not per run.** A preload's three kinds of
//! step have different needs — see [`run_with_fallback`]'s own doc comment
//! for why a single whole-run choice would either waste the native path or
//! force it aside more often than the two real gaps require. `core/` stays
//! free of the choice itself (CLAUDE.md): [`VolumeFormatter`] is unchanged,
//! and `core::preload::run` (the single-formatter runner) is untouched and
//! still what a caller with only one tool in hand — a test, or `hst-imager`
//! alone — uses. This module's own [`run_with_fallback`] is the
//! `commands/`-level orchestration that picks between two formatters, one
//! step at a time.

use std::path::Path;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, State};

use crate::core::error::{CoreError, CoreResult};
use crate::core::jobs::ProgressSink;
use crate::core::oplog::{JsonlOperationLog, OperationOutcome};
use crate::core::preload::native::NativeFormatter;
use crate::core::preload::VolumeFormatter;
use crate::core::preload::{
    plan, step_label, CopySummary, PreloadOutcome, PreloadPlan, PreloadRequest, PreloadStep,
    ToolVersion,
};
use crate::error::AppResult;
use crate::tools::hst_imager::{HstImager, TESTED_VERSION};

use super::jobs::{spawn_job, JobRegistry};
use super::oplog::{user_operation, write_to_path};

/// What the configured tool is, and whether it is the one ART was written
/// against.
#[derive(Debug, Clone, Serialize)]
pub struct FormatterReport {
    pub version: ToolVersion,
    /// False when the version is not [`TESTED_VERSION`]. **Not a refusal** —
    /// ART's command set came from that version's own scripts, and saying so
    /// beats turning away a tool that would have worked.
    pub is_tested_version: bool,
    pub tested_version: String,
}

/// Ask the configured formatter what it is. Runs it with `--version` and
/// nothing else.
#[tauri::command]
pub fn preload_probe(tool_path: String) -> AppResult<FormatterReport> {
    let version = HstImager::at(tool_path.trim()).probe()?;
    Ok(FormatterReport {
        is_tested_version: version.raw.contains(TESTED_VERSION),
        version,
        tested_version: TESTED_VERSION.to_string(),
    })
}

/// The request a screen sends, with where the tool is.
///
/// `tool_path` is no longer required to be set (ART-120): it names where
/// `hst-imager` is, needed only as a fallback for the two gaps
/// [`FallbackReason`] enumerates. An empty or blank string means "not
/// configured", the same convention the rest of ART's `hst-imager` settings
/// already use.
#[derive(Debug, Clone, Deserialize)]
pub struct PreloadCommand {
    #[serde(flatten)]
    pub request: PreloadRequest,
    /// Where `hst.imager` is. ART does not ship it — 137 MB beside a Tauri
    /// app is not a trade worth making, and SD-0 said as much.
    pub tool_path: String,
}

/// What would be run. Writes nothing (§92's PREVIEW).
#[tauri::command]
pub fn preload_plan(command: PreloadCommand) -> AppResult<PreloadPlan> {
    Ok(plan(&command.request)?)
}

/// The event a finished preload arrives on.
pub const PRELOAD_EVENT: &str = "preload-result";

/// Which tool actually performed one step, and why, if it was not the
/// default. Always present for every step the run reached — a plain,
/// unqualified `"native"` is exactly as much a report as a fallback is
/// (CLAUDE.md's "the fallback is never silent" applies to the default too:
/// a screen that only ever mentions the exceptional case invites the reader
/// to wonder what happened the rest of the time).
#[derive(Debug, Clone, Serialize)]
pub struct StepReport {
    pub step: PreloadStep,
    /// `"native"`, or the fallback tool's own probed version string —
    /// deliberately not a fixed `"hst-imager"` label, so a mismatched or
    /// untested `hst.imager.exe` is visible here too.
    pub tool: String,
    pub fallback_reason: Option<FallbackReason>,
}

/// Why one step ran on the fallback tool instead of natively. A value, never
/// a sentence (ART-060) — `src/lib/preload.ts::fallbackPhrase` translates it.
///
/// Exactly two variants, matching the two capability gaps this module's own
/// doc comment names. Nothing else ever falls back: a real failure (out of
/// space, a malformed image) is returned as-is rather than silently retried
/// on another tool, which would risk running the same destructive step twice.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "reason", rename_all = "kebab-case")]
pub enum FallbackReason {
    /// ART-117.
    ForeignRdbEmbed,
    /// ART-113. Carries the same detail the refused
    /// [`CoreError::NonAsciiPfs3Names`] did, so the screen can say which
    /// names without re-deriving them.
    NonAsciiPfs3Names { paths: Vec<String>, more: usize },
}

impl FallbackReason {
    /// Whether a native failure is one of the two known capability gaps —
    /// and so safe to retry with the fallback tool — or a real failure that
    /// must not be silently retried.
    ///
    /// **This match is the whole policy.** Widening it to catch more
    /// [`CoreError`] variants (a full disk, a malformed image,
    /// [`CoreError::UnsupportedFormat`] from an exotic DosType) would turn
    /// "the native path runs unless ART already knows it cannot do the job"
    /// into "the native path runs until it hits any error at all", which is
    /// not what was decided — those are real failures, not capability gaps,
    /// and retrying them on another tool risks running a destructive step
    /// twice.
    fn from_native_error(err: &CoreError) -> Option<Self> {
        match err {
            CoreError::ForeignRdbEmbedNotSupported => Some(Self::ForeignRdbEmbed),
            CoreError::NonAsciiPfs3Names { paths, more } => Some(Self::NonAsciiPfs3Names {
                paths: paths.clone(),
                more: *more,
            }),
            _ => None,
        }
    }

    /// English detail for a log line or a refusal — never shown to the user
    /// as-is; `src/lib/preload.ts::fallbackPhrase` is the translated form.
    fn detail(&self) -> String {
        match self {
            Self::ForeignRdbEmbed => {
                "embedding a filesystem driver into this card's existing RDB in place needs to \
                 edit a partition table ART did not build, which ART's own writer cannot do \
                 safely (ART-117)"
                    .to_string()
            }
            Self::NonAsciiPfs3Names { paths, more } => format!(
                "{} non-ASCII name(s) cannot be written to a PFS3 volume by this version of \
                 libpfs3 (ART-113): {}{}",
                paths.len() + more,
                paths.join(", "),
                if *more > 0 {
                    format!(", and {more} more")
                } else {
                    String::new()
                }
            ),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct PreloadResult {
    pub job_id: u64,
    pub image: String,
    pub outcome: PreloadOutcome,
    /// Which tool performed each step, and why, when it was not the default.
    /// See this module's own doc comment: never silent, for every step.
    pub steps: Vec<StepReport>,
}

/// What one step changed, so [`run_with_fallback`] can fold it into a
/// [`PreloadOutcome`] without caring which formatter produced it.
enum StepEffect {
    None,
    Formatted(String),
    Copied(CopySummary),
}

fn apply_effect(outcome: &mut PreloadOutcome, effect: StepEffect) {
    match effect {
        StepEffect::None => {}
        StepEffect::Formatted(drive_name) => outcome.formatted.push(drive_name),
        StepEffect::Copied(summary) => {
            outcome.copied.files += summary.files;
            outcome.copied.directories += summary.directories;
            outcome.copied.bytes += summary.bytes;
            outcome.copied.comments_lost += summary.comments_lost;
            outcome.copied.dates_lost += summary.dates_lost;
        }
    }
}

/// Run one step against one formatter. The only place a [`PreloadStep`] is
/// turned into a [`VolumeFormatter`] call, so both the native attempt and a
/// fallback attempt go through the identical mapping.
fn run_step(
    image: &Path,
    step: &PreloadStep,
    formatter: &dyn VolumeFormatter,
    sink: &dyn ProgressSink,
) -> CoreResult<StepEffect> {
    match step {
        PreloadStep::ImportFilesystem {
            slot,
            driver,
            dostype,
            name,
        } => {
            formatter.import_filesystem(image, *slot, driver, dostype, name, sink)?;
            Ok(StepEffect::None)
        }
        PreloadStep::FormatPartition {
            slot,
            index,
            drive_name,
            volume_name,
        } => {
            formatter.format_partition(image, *slot, *index, volume_name, sink)?;
            Ok(StepEffect::Formatted(drive_name.clone()))
        }
        PreloadStep::CopyIn {
            slot,
            drive_name,
            source,
        } => {
            let summary = formatter.copy_in(image, *slot, drive_name, source, sink)?;
            Ok(StepEffect::Copied(summary))
        }
    }
}

/// Refuse clearly when a step needs the fallback tool and none is configured
/// — named as its own function because "do not half-run" (the brief's own
/// words) means this has to fire *before* the step's own formatter call, not
/// after a partial attempt.
fn missing_tool_error(reason: &FallbackReason, step: &PreloadStep) -> CoreError {
    CoreError::InvalidInput(format!(
        "{} needs hst-imager and no hst.imager.exe is configured — {}. Point ART at hst-imager \
         in Settings, or in this screen.",
        step_label(step),
        reason.detail(),
    ))
}

/// Run a plan, giving the native path first refusal on every step and
/// falling back to `hst-imager` only for the two known capability gaps
/// (ART-113, ART-117) — never for a real failure.
///
/// **Per step, chosen here rather than once for the whole run.** A preload's
/// three kinds of step have different needs: `ImportFilesystem` always needs
/// the fallback (`NativeFormatter` refuses it unconditionally, for every
/// card — ART-117), while `FormatPartition` and almost every `CopyIn` run
/// natively; only a `CopyIn` whose source tree carries a non-ASCII AmigaDOS
/// name onto a PFS3 partition needs the fallback too (ART-113), and that is
/// a fact about *that step's own content*, not about the run as a whole.
///
/// A run-level choice — probe once, then use one formatter for every step —
/// was the alternative, and it is simpler, but it is wrong in both
/// directions: a single accented folder name anywhere in a large tree would
/// force every other step, including every `FormatPartition`, onto
/// `hst-imager` too; and a run that has to import one driver would waste the
/// native path for every step after it, even though only that one step
/// needed the fallback. Per-step costs one extra (cheap) call when a step
/// needs no fallback — trying native first — in exchange for never using the
/// slower, external tool for work the native path can already do.
///
/// This is safe specifically because both known gaps are refused **before**
/// any byte is written: `import_filesystem` never opens the image, and the
/// ART-113 check inside `copy_in` runs before `FileRegionMut::open` (see
/// `core::preload::native`'s own module docs). So trying native first and
/// reacting to exactly these two typed errors never leaves a half-written
/// step behind — the failed attempt touched nothing.
fn run_with_fallback(
    made: &PreloadPlan,
    native: &dyn VolumeFormatter,
    fallback: Option<&dyn VolumeFormatter>,
    sink: &dyn ProgressSink,
) -> CoreResult<(PreloadOutcome, Vec<StepReport>)> {
    let mut outcome = PreloadOutcome {
        tool: native.probe().ok(),
        ..Default::default()
    };
    let mut reports = Vec::new();
    let total = made.steps.len() as u64;

    for (done, step) in made.steps.iter().enumerate() {
        if sink.is_cancelled() {
            return Err(CoreError::Cancelled);
        }
        sink.report(done as u64, Some(total), &step_label(step));

        let native_err = match run_step(&made.image, step, native, sink) {
            Ok(effect) => {
                reports.push(StepReport {
                    step: step.clone(),
                    tool: "native".into(),
                    fallback_reason: None,
                });
                apply_effect(&mut outcome, effect);
                continue;
            }
            Err(err) => err,
        };

        let Some(reason) = FallbackReason::from_native_error(&native_err) else {
            // A real failure, not a known capability gap: surface it as-is.
            // Retrying on another tool here would risk running a destructive
            // step (a format already begun, a copy already partway) twice.
            return Err(native_err);
        };

        let Some(fallback) = fallback else {
            return Err(missing_tool_error(&reason, step));
        };

        let effect = run_step(&made.image, step, fallback, sink)?;
        let tool_name = fallback
            .probe()
            .map(|v| v.raw)
            .unwrap_or_else(|_| "the configured tool".into());
        reports.push(StepReport {
            step: step.clone(),
            tool: tool_name,
            fallback_reason: Some(reason),
        });
        apply_effect(&mut outcome, effect);
    }

    sink.report(total, Some(total), "done");
    Ok((outcome, reports))
}

/// One line per step where the fallback fired, for the operation log — the
/// same "say which tool did which step and why" rule the in-app report
/// follows, kept true in the audit trail too. `None` when every step ran
/// natively, so the log does not carry an empty "Fallback" detail.
fn fallback_summary(reports: &[StepReport]) -> Option<String> {
    let lines: Vec<String> = reports
        .iter()
        .filter_map(|report| {
            report
                .fallback_reason
                .as_ref()
                .map(|reason| format!("{} ({})", step_label(&report.step), reason.detail()))
        })
        .collect();
    if lines.is_empty() {
        None
    } else {
        Some(lines.join("; "))
    }
}

/// Format the partitions and copy the content in. Returns a job id (§54).
///
/// The plan is recomputed here rather than taken from the caller: a screen
/// that previewed one thing must not be able to run another, and the card may
/// have changed since.
#[tauri::command]
pub fn preload_run(
    command: PreloadCommand,
    app: AppHandle,
    registry: State<'_, Arc<JobRegistry>>,
    oplog: State<'_, JsonlOperationLog>,
) -> AppResult<u64> {
    let image = command.request.image.display().to_string();
    let tool_path = command.tool_path.trim().to_string();
    let hst = if tool_path.is_empty() {
        None
    } else {
        Some(HstImager::at(tool_path))
    };

    // Refuse here, on the command thread, rather than inside the job: a bad
    // partition number is not something to discover after the first format.
    let made = plan(&command.request)?;

    let log_path = oplog.path().to_path_buf();
    let registry = Arc::clone(&registry);
    let emit_app = app.clone();
    let title = format!("Preparing {} volume(s) on {image}", made.formats());
    let for_log = image.clone();

    let id = spawn_job(&app, registry, &title, move |job_id, progress| {
        let native = NativeFormatter;
        let run_result = run_with_fallback(
            &made,
            &native,
            hst.as_ref().map(|h| h as &dyn VolumeFormatter),
            progress,
        );

        // §53. A format destroys what was there, so what was formatted and
        // with which tool are the two things the log has to carry — and now,
        // which tool did which step, when it was not the default.
        let record = user_operation("Format and fill Amiga volumes")
            .source(&for_log)
            .destination(&for_log)
            .detail("Partitions formatted", made.formats().to_string());
        let record = match &run_result {
            Ok((done, reports)) => {
                let record = record
                    .detail("Volumes", done.formatted.join(", "))
                    .detail("Files copied", done.copied.files.to_string())
                    .detail(
                        "Tool",
                        done.tool
                            .as_ref()
                            .map(|t| t.raw.clone())
                            .unwrap_or_else(|| "unknown".into()),
                    );
                let record = match fallback_summary(reports) {
                    Some(summary) => record.detail("Fallback", summary),
                    None => record,
                };
                // **Not verified, and it says so.** ART has no PFS3 reader
                // here, so the files inside the volume cannot be read back.
                // Claiming verification here would be the one thing §89
                // forbids.
                record.outcome(OperationOutcome::verified(false))
            }
            Err(err) => record.failure(err.code(), err.to_string()),
        };
        write_to_path(&log_path, &record);

        let (outcome, steps) = run_result?;
        let _ = emit_app.emit(
            PRELOAD_EVENT,
            PreloadResult {
                job_id,
                image: for_log,
                outcome,
                steps,
            },
        );
        Ok(())
    });

    Ok(id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::preload::{PreloadPartition, ToolVersion};

    fn scratch(tag: &str) -> std::path::PathBuf {
        let dir =
            std::env::temp_dir().join(format!("art-preload-cmd-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// **The wire, written down.** `src/lib/preload.ts` builds this object by
    /// hand — the request's own fields spread flat, with `tool_path` beside
    /// them — and `#[serde(flatten)]` is the only thing making the two agree.
    /// Nothing else in the build checks it: a renamed field would compile on
    /// both sides and fail at the moment the user pressed Preview, which is
    /// the worst place to find out.
    #[test]
    fn the_payload_the_frontend_sends_deserialises() {
        let command: PreloadCommand = serde_json::from_str(
            r#"{"image":"E:\\amiga\\ProjeART\\card.img",
                "driver":null,
                "partitions":[{"area":1,"index":1,"volume_name":"Work","content":null}],
                "tool_path":"E:\\amiga\\hstimager\\hst.imager.exe"}"#,
        )
        .expect("the shape src/lib/preload.ts sends");

        assert_eq!(command.tool_path, "E:\\amiga\\hstimager\\hst.imager.exe");
        assert_eq!(command.request.driver, None);
        assert_eq!(command.request.partitions.len(), 1);
        assert_eq!(command.request.partitions[0].area, 1);
        assert_eq!(command.request.partitions[0].volume_name, "Work");
        assert_eq!(command.request.partitions[0].content, None);

        // And the two the screen fills in when the user does: a driver to
        // embed, and a folder whose tree goes in.
        let filled: PreloadCommand = serde_json::from_str(
            r#"{"image":"card.img",
                "driver":"E:\\amiga\\pfs3aio.lha",
                "partitions":[{"area":2,"index":3,"volume_name":"Games","content":"E:\\tree"}],
                "tool_path":"hst.imager.exe"}"#,
        )
        .expect("the same shape with every optional filled");

        assert_eq!(
            filled.request.driver,
            Some(std::path::PathBuf::from("E:\\amiga\\pfs3aio.lha"))
        );
        assert_eq!(filled.request.partitions[0].index, 3);
        assert_eq!(
            filled.request.partitions[0].content,
            Some(std::path::PathBuf::from("E:\\tree"))
        );

        // **ART-120**: an empty `tool_path` deserialises fine — the field is
        // no longer required for a preload that never needs the fallback.
        let no_tool: PreloadCommand = serde_json::from_str(
            r#"{"image":"card.img",
                "driver":null,
                "partitions":[{"area":1,"index":1,"volume_name":"Work","content":null}],
                "tool_path":""}"#,
        )
        .expect("an empty tool_path is a valid payload");
        assert_eq!(no_tool.tool_path, "");
    }

    /// The adapter is thin, and this is what it must not get wrong: the plan
    /// the screen is shown comes from the card, through the same `plan` the
    /// run recomputes.
    #[test]
    fn the_plan_command_answers_from_the_card() {
        use crate::core::rdb::{AmigaHardDiskFs, PartitionSpec};

        let dir = scratch("plan");
        let image = dir.join("card.hdf");
        crate::core::hdf::create_hdf(
            &image,
            32 * 1024 * 1024,
            true,
            &[PartitionSpec {
                drive_name: "DH0".into(),
                fs_type: AmigaHardDiskFs::FfsStandard,
                size_mb: 10,
                bootable: true,
                boot_priority: 0,
                num_buffers: 0,
            }],
            &[],
        )
        .unwrap();

        let made = preload_plan(PreloadCommand {
            request: PreloadRequest {
                image: image.clone(),
                driver: None,
                partitions: vec![PreloadPartition {
                    area: 1,
                    index: 1,
                    volume_name: "Work".into(),
                    content: None,
                }],
            },
            tool_path: "hst.imager".into(),
        })
        .unwrap();

        assert_eq!(
            made.steps,
            vec![PreloadStep::FormatPartition {
                slot: None,
                index: 1,
                drive_name: "DH0".into(),
                volume_name: "Work".into(),
            }]
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    // -----------------------------------------------------------------------
    // ART-120 — the formatter choice
    // -----------------------------------------------------------------------

    /// A formatter that records what it was asked to do, succeeds at
    /// everything, and can be told to fail one call in a specific,
    /// [`CoreError`]-typed way. The same shape `core::preload::mod`'s own
    /// `Recorder` uses, kept local here because this module's tests need to
    /// fail with *specific* variants (`ForeignRdbEmbedNotSupported`,
    /// `NonAsciiPfs3Names`) that one does not need to produce.
    #[derive(Default)]
    struct Recorder {
        calls: std::cell::RefCell<Vec<String>>,
        import_fails_with: Option<fn() -> CoreError>,
    }

    impl VolumeFormatter for Recorder {
        fn probe(&self) -> CoreResult<ToolVersion> {
            Ok(ToolVersion {
                raw: "recorder-tool".into(),
            })
        }
        fn import_filesystem(
            &self,
            _i: &Path,
            _slot: Option<usize>,
            _d: &Path,
            _dostype: &str,
            name: &str,
            _s: &dyn ProgressSink,
        ) -> CoreResult<()> {
            self.calls.borrow_mut().push(format!("import {name}"));
            match self.import_fails_with {
                Some(make_err) => Err(make_err()),
                None => Ok(()),
            }
        }
        fn format_partition(
            &self,
            _i: &Path,
            _slot: Option<usize>,
            index: usize,
            volume: &str,
            _s: &dyn ProgressSink,
        ) -> CoreResult<()> {
            self.calls
                .borrow_mut()
                .push(format!("format {index} {volume}"));
            Ok(())
        }
        fn copy_in(
            &self,
            _i: &Path,
            _slot: Option<usize>,
            drive: &str,
            _src: &Path,
            _s: &dyn ProgressSink,
        ) -> CoreResult<CopySummary> {
            self.calls.borrow_mut().push(format!("copy {drive}"));
            Ok(CopySummary {
                files: 1,
                ..Default::default()
            })
        }
    }

    fn plan_of(steps: Vec<PreloadStep>) -> PreloadPlan {
        PreloadPlan {
            image: std::path::PathBuf::from("card.img"),
            steps,
        }
    }

    /// **Native is chosen by default — mutation-checked against exactly the
    /// bug ART-120 was filed for.** The fallback tool here is a real
    /// `HstImager` pointed at a path that does not exist, so if the run ever
    /// used it — even once, even for a step the native path handles fine —
    /// this test fails with an I/O error rather than passing. A version of
    /// `preload_run`/`run_with_fallback` that constructs `HstImager`
    /// unconditionally (ART-120's original bug) fails this test; one that
    /// tries native first and only reaches the fallback for the two known
    /// gaps passes it.
    #[test]
    fn native_is_chosen_by_default_over_a_configured_but_unreachable_tool() {
        use crate::core::rdb::{AmigaHardDiskFs, PartitionSpec};

        let dir = scratch("default-native");
        let image = dir.join("card.hdf");
        crate::core::hdf::create_hdf(
            &image,
            32 * 1024 * 1024,
            true,
            &[PartitionSpec {
                drive_name: "DH0".into(),
                fs_type: AmigaHardDiskFs::FfsStandard,
                size_mb: 10,
                bootable: true,
                boot_priority: 0,
                num_buffers: 0,
            }],
            &[],
        )
        .unwrap();
        let tree = dir.join("tree");
        std::fs::create_dir_all(&tree).unwrap();
        std::fs::write(tree.join("Readme"), b"hi\n").unwrap();

        let made = plan(&PreloadRequest {
            image: image.clone(),
            driver: None,
            partitions: vec![PreloadPartition {
                area: 1,
                index: 1,
                volume_name: "Work".into(),
                content: Some(tree),
            }],
        })
        .unwrap();

        let native = NativeFormatter;
        let unreachable = HstImager::at(dir.join("does-not-exist.exe"));
        let (outcome, reports) = run_with_fallback(
            &made,
            &native,
            Some(&unreachable as &dyn VolumeFormatter),
            &crate::core::jobs::NoProgress,
        )
        .unwrap();

        assert_eq!(outcome.formatted, vec!["DH0"]);
        assert_eq!(outcome.copied.files, 1);
        assert!(reports.iter().all(|r| r.tool == "native"), "{reports:?}");
        assert!(
            reports.iter().all(|r| r.fallback_reason.is_none()),
            "{reports:?}"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// **The fallback fires per step, not for the whole run.** One plan
    /// carries both a `FormatPartition` step (native can always do this) and
    /// a `CopyIn` step whose source tree has a non-ASCII name onto a PFS3
    /// partition (ART-113, native cannot). The `FormatPartition` report must
    /// still say `"native"` — a run-level choice would have forced it onto
    /// the fallback too, along with everything else, the moment any step
    /// needed it.
    #[test]
    fn a_non_ascii_source_tree_falls_back_only_for_the_step_that_needs_it() {
        use crate::core::rdb::{AmigaHardDiskFs, PartitionSpec};

        let dir = scratch("fallback-pfs3");
        let image = dir.join("card.hdf");
        crate::core::hdf::create_hdf(
            &image,
            32 * 1024 * 1024,
            true,
            &[PartitionSpec {
                drive_name: "DH0".into(),
                fs_type: AmigaHardDiskFs::Pfs3Standard,
                size_mb: 20,
                bootable: true,
                boot_priority: 0,
                num_buffers: 0,
            }],
            &[],
        )
        .unwrap();
        let tree = dir.join("tree");
        std::fs::create_dir_all(&tree).unwrap();
        // Not ASCII — the ART-113 shape, on a directory name.
        std::fs::create_dir_all(tree.join("español")).unwrap();
        std::fs::write(tree.join("español").join("Readme"), b"hola\n").unwrap();

        let made = plan_of(vec![
            PreloadStep::FormatPartition {
                slot: None,
                index: 1,
                drive_name: "DH0".into(),
                volume_name: "Work".into(),
            },
            PreloadStep::CopyIn {
                slot: None,
                drive_name: "DH0".into(),
                source: tree,
            },
        ]);
        let made = PreloadPlan {
            image: image.clone(),
            ..made
        };

        let native = NativeFormatter;
        let recorder = Recorder::default();
        let (outcome, reports) = run_with_fallback(
            &made,
            &native,
            Some(&recorder as &dyn VolumeFormatter),
            &crate::core::jobs::NoProgress,
        )
        .unwrap();

        assert_eq!(reports.len(), 2, "{reports:?}");
        assert_eq!(reports[0].tool, "native", "{reports:?}");
        assert!(reports[0].fallback_reason.is_none(), "{reports:?}");
        assert_eq!(reports[1].tool, "recorder-tool", "{reports:?}");
        assert!(
            matches!(
                reports[1].fallback_reason,
                Some(FallbackReason::NonAsciiPfs3Names { .. })
            ),
            "{reports:?}"
        );
        // The fallback tool actually ran the copy — not simulated.
        assert_eq!(*recorder.calls.borrow(), vec!["copy DH0"]);
        assert_eq!(outcome.copied.files, 1, "the recorder's own summary");

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// **A missing tool refuses, rather than half-running.** The plan needs
    /// `ImportFilesystem` (native always refuses it — ART-117), no fallback
    /// is configured, and the recorder proves the steps after it — a
    /// destructive format, then a copy — never ran.
    #[test]
    fn a_missing_fallback_tool_refuses_before_the_rest_of_the_plan_runs() {
        let recorder = Recorder {
            import_fails_with: Some(|| CoreError::ForeignRdbEmbedNotSupported),
            ..Default::default()
        };
        let made = plan_of(vec![
            PreloadStep::ImportFilesystem {
                slot: None,
                driver: std::path::PathBuf::from("pfs3aio.lha"),
                dostype: "PDS3".into(),
                name: "pfs3aio".into(),
            },
            PreloadStep::FormatPartition {
                slot: None,
                index: 1,
                drive_name: "DH0".into(),
                volume_name: "Work".into(),
            },
            PreloadStep::CopyIn {
                slot: None,
                drive_name: "DH0".into(),
                source: std::path::PathBuf::from("tree"),
            },
        ]);

        let err =
            run_with_fallback(&made, &recorder, None, &crate::core::jobs::NoProgress).unwrap_err();

        assert_eq!(err.code(), "ART-INPUT-INVALID", "{err}");
        assert!(err.to_string().contains("hst-imager"), "{err}");
        assert!(err.to_string().contains("ART-117"), "{err}");
        assert_eq!(
            *recorder.calls.borrow(),
            vec!["import pfs3aio"],
            "format and copy must not run after the refusal"
        );
    }

    /// A real failure — not one of the two known gaps — is surfaced as-is,
    /// never silently retried on the fallback tool.
    #[test]
    fn a_real_failure_is_not_treated_as_a_reason_to_fall_back() {
        let recorder = Recorder {
            import_fails_with: Some(|| CoreError::Io(std::io::Error::other("disk yanked"))),
            ..Default::default()
        };
        let other = Recorder::default();
        let made = plan_of(vec![PreloadStep::ImportFilesystem {
            slot: None,
            driver: std::path::PathBuf::from("pfs3aio.lha"),
            dostype: "PDS3".into(),
            name: "pfs3aio".into(),
        }]);

        let err = run_with_fallback(
            &made,
            &recorder,
            Some(&other as &dyn VolumeFormatter),
            &crate::core::jobs::NoProgress,
        )
        .unwrap_err();

        assert_eq!(err.code(), "ART-IO", "{err}");
        assert!(
            other.calls.borrow().is_empty(),
            "the fallback must not run for a real failure"
        );
    }

    /// The real tool, on a real card, when asked.
    ///
    /// ```text
    /// ART_HST=E:\amiga\Amigatolon\hstimager\hst.imager.exe \
    ///   cargo test preload_a_real_card_when_asked -- --nocapture
    /// ```
    ///
    /// `ART_PFS3` may name a `pfs3aio.lha` to embed first; without it the card
    /// is built with FFS, which Kickstart carries.
    #[test]
    fn preload_a_real_card_when_asked() {
        use crate::core::card::build::{build_card, AreaSpec, CardSpec};
        use crate::core::jobs::NoProgress;
        use crate::core::preload::run;
        use crate::core::rdb::{AmigaHardDiskFs, PartitionSpec};

        let Ok(tool) = std::env::var("ART_HST") else {
            return;
        };
        let dir = std::path::PathBuf::from(
            std::env::var("ART_SCRATCH")
                .unwrap_or_else(|_| std::env::temp_dir().display().to_string()),
        )
        .join("art-preload-real");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let image = dir.join("preload.img");

        let driver = std::env::var("ART_PFS3").ok().map(std::path::PathBuf::from);
        let fs = match driver {
            Some(_) => AmigaHardDiskFs::Pfs3DirectScsi,
            None => AmigaHardDiskFs::FfsStandard,
        };

        build_card(
            &image,
            &CardSpec {
                total_bytes: 2 * 1024 * 1024 * 1024,
                boot_bytes: 0,
                label: "ART CARD".into(),
                boot_files: Vec::new(),
                areas: vec![AreaSpec {
                    size_bytes: 0,
                    partitions: vec![PartitionSpec {
                        drive_name: "DH0".into(),
                        fs_type: fs,
                        size_mb: 512,
                        bootable: true,
                        boot_priority: 0,
                        num_buffers: 0,
                    }],
                    file_systems: Vec::new(),
                }],
            },
            &NoProgress,
        )
        .unwrap();

        // Something to copy in, so the whole chain is exercised.
        let tree = dir.join("tree");
        std::fs::create_dir_all(tree.join("S")).unwrap();
        std::fs::write(tree.join("S").join("Startup-Sequence"), b"echo hello\n").unwrap();
        std::fs::write(tree.join("Readme"), b"from ART\n").unwrap();

        let request = PreloadRequest {
            image: image.clone(),
            driver: std::env::var("ART_PFS3").ok().map(std::path::PathBuf::from),
            partitions: vec![PreloadPartition {
                area: 1,
                index: 1,
                volume_name: "Work".into(),
                content: Some(tree),
            }],
        };

        let made = plan(&request).unwrap();
        for step in &made.steps {
            println!("  step: {step:?}");
        }

        let formatter = HstImager::at(&tool);
        println!("tool: {:?}", formatter.probe());

        match run(&made, &formatter, &NoProgress) {
            Ok(outcome) => println!("outcome: {outcome:?}"),
            Err(err) => panic!("preload failed: {err}"),
        }
    }

    // -----------------------------------------------------------------------
    // Outbound wire shapes — pinned against src/lib/preload.ts, the same
    // discipline commands/osinstall.rs's own `wire_shapes` module follows.
    // -----------------------------------------------------------------------
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

        #[test]
        fn step_report_serializes_with_the_keys_the_frontend_declares() {
            let report = StepReport {
                step: PreloadStep::FormatPartition {
                    slot: None,
                    index: 1,
                    drive_name: "DH0".into(),
                    volume_name: "Work".into(),
                },
                tool: "native".into(),
                fallback_reason: None,
            };
            let value = serde_json::to_value(&report).unwrap();
            expect_keys(&value, &["step", "tool", "fallback_reason"]);
            assert_eq!(value["fallback_reason"], serde_json::Value::Null);
        }

        #[test]
        fn fallback_reason_serializes_with_the_tags_the_frontend_declares() {
            let embed = serde_json::to_value(FallbackReason::ForeignRdbEmbed).unwrap();
            expect_keys(&embed, &["reason"]);
            assert_eq!(embed["reason"], "foreign-rdb-embed");

            let names = serde_json::to_value(FallbackReason::NonAsciiPfs3Names {
                paths: vec!["Locale/español".into()],
                more: 3,
            })
            .unwrap();
            expect_keys(&names, &["reason", "paths", "more"]);
            assert_eq!(names["reason"], "non-ascii-pfs3-names");
            assert_eq!(names["more"], 3);
        }

        #[test]
        fn preload_result_carries_a_steps_array_alongside_its_siblings() {
            let result = PreloadResult {
                job_id: 7,
                image: "card.img".into(),
                outcome: PreloadOutcome::default(),
                steps: vec![StepReport {
                    step: PreloadStep::FormatPartition {
                        slot: None,
                        index: 1,
                        drive_name: "DH0".into(),
                        volume_name: "Work".into(),
                    },
                    tool: "native".into(),
                    fallback_reason: None,
                }],
            };
            let value = serde_json::to_value(&result).unwrap();
            // Deliberately not camelCased — `job_id` matches `LayoutResult`
            // and `OsInstallResult`, which do the same.
            expect_keys(&value, &["job_id", "image", "outcome", "steps"]);
            assert_eq!(value["steps"].as_array().unwrap().len(), 1);
        }
    }
}
