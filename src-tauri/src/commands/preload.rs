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
//! ## ART-122 — the unit of that choice is a partition, not a step
//!
//! **The choice is made per partition, not per run and not per step.** The
//! first shipped version chose per step, and that is how a format could run
//! natively while the copy into the volume it had just made fell back — the
//! one combination that does not work. `hst-imager`'s first write into a
//! volume `NativeFormatter` formatted dies `ERROR_DISK_FULL`: the two
//! implementations size the PFS3 reserved area differently, and ART's number
//! is the one `pfs3aio`'s own algorithm produces, so this is not ART's
//! arithmetic being unusual. A volume is therefore formatted and filled by
//! one implementation or by neither, decided **before** the destructive step
//! through [`VolumeFormatter::can_copy_in`], which writes nothing. See
//! [`run_with_fallback`]'s own doc comment for why a whole-run choice is
//! still wrong.
//!
//! `core/` stays free of the choice itself (CLAUDE.md): [`VolumeFormatter`]
//! gained a capability question, not a policy, and `core::preload::run` (the
//! single-formatter runner) is untouched and still what a caller with only
//! one tool in hand — a test, or `hst-imager` alone — uses. This module's own
//! [`run_with_fallback`] is the `commands/`-level orchestration that picks
//! between two formatters.

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
    /// ART-122. **Not a capability gap of its own** — the format step is one
    /// the native path can always do. It falls back because the *copy* into
    /// the same volume must, and `hst-imager`'s first write into a volume
    /// `NativeFormatter` formatted fails: a partition is formatted and filled
    /// by one tool or by neither. Carries the drive whose copy forced it, so
    /// the report names the pairing rather than repeating the copy's own
    /// reason against a step that reason is not about.
    PairedWithFallbackCopy { drive: String },
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
            Self::PairedWithFallbackCopy { drive } => format!(
                "the copy into {drive} has to run on this tool, and a volume is formatted and \
                 filled by one tool: hst-imager cannot write into a volume ART's own writer \
                 formatted (ART-122)"
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
        StepEffect::Copied(summary) => outcome.copied.absorb(&summary),
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

/// The name a report gives the tool that ran a step — the fallback's own
/// probed version, so a mismatched or untested `hst.imager.exe` is visible in
/// the report rather than hidden behind a fixed label.
fn tool_name_of(fallback: Option<&dyn VolumeFormatter>) -> String {
    fallback
        .and_then(|tool| tool.probe().ok())
        .map(|version| version.raw)
        .unwrap_or_else(|| "the configured tool".into())
}

/// Whether a `FormatPartition` step must go to the fallback because the
/// `CopyIn` into the *same* volume will (ART-122) — asked before the format,
/// answered without writing.
///
/// `None` for every other kind of step, for a format with no copy after it
/// (nothing to be paired with), and for a copy the native path can do. A
/// native error that is **not** one of the known capability gaps is `None`
/// too: it is a real failure, and the format is left to run natively so the
/// copy can report it as itself rather than having it surface early against
/// a step it is not about.
fn paired_copy_forces_fallback(
    made: &PreloadPlan,
    step: &PreloadStep,
    native: &dyn VolumeFormatter,
) -> Option<FallbackReason> {
    let PreloadStep::FormatPartition {
        slot, drive_name, ..
    } = step
    else {
        return None;
    };

    let source = made.steps.iter().find_map(|other| match other {
        PreloadStep::CopyIn {
            slot: copy_slot,
            drive_name: copy_drive,
            source,
        } if copy_slot == slot && copy_drive == drive_name => Some(source),
        _ => None,
    })?;

    let err = native
        .can_copy_in(&made.image, *slot, drive_name, source)
        .err()?;
    FallbackReason::from_native_error(&err)?;
    Some(FallbackReason::PairedWithFallbackCopy {
        drive: drive_name.clone(),
    })
}

/// Run a plan, giving the native path first refusal on every step and
/// falling back to `hst-imager` only for the two known capability gaps
/// (ART-113, ART-117) — never for a real failure.
///
/// **Per partition, chosen here rather than once for the whole run.** A
/// preload's three kinds of step have different needs: `ImportFilesystem`
/// always needs the fallback (`NativeFormatter` refuses it unconditionally,
/// for every card — ART-117), while `FormatPartition` and almost every
/// `CopyIn` run natively; only a `CopyIn` whose source tree carries a
/// non-ASCII AmigaDOS name onto a PFS3 partition needs the fallback too
/// (ART-113), and that is a fact about *that partition's own content*, not
/// about the run as a whole.
///
/// A run-level choice — probe once, then use one formatter for every step —
/// was the alternative, and it is simpler, but it is wrong in both
/// directions: a single accented folder name anywhere in a large tree would
/// force every other partition's steps onto `hst-imager` too; and a run that
/// has to import one driver would waste the native path for every step after
/// it, even though only that one step needed the fallback.
///
/// **The unit is a partition, not a step (ART-122).** Formatting a volume
/// with one implementation and filling it with the other does not work:
/// `hst-imager`'s first write into a volume `NativeFormatter` formatted dies
/// `ERROR_DISK_FULL`, because the two size the PFS3 reserved area
/// differently — ART's number is `pfs3aio`'s own, computed by that
/// implementation's own algorithm, so it is not ART's arithmetic that is
/// unusual. The choice therefore has to be made *before* the format, from a
/// fact about the copy that follows it: [`VolumeFormatter::can_copy_in`]
/// answers exactly that, without writing. When it refuses with a known gap,
/// the partition's format goes to the fallback as well, reported as
/// [`FallbackReason::PairedWithFallbackCopy`]; when no fallback tool is
/// configured, the run refuses **before** the partition is erased rather
/// than after.
///
/// This is safe specifically because both known gaps are refused **before**
/// any byte is written: `import_filesystem` never opens the image, and the
/// ART-113 check runs before `FileRegionMut::open` (see
/// `core::preload::native`'s own module docs) whether it is reached through
/// `copy_in` or `can_copy_in`. So trying native first and reacting to exactly
/// these two typed errors never leaves a half-written step behind — the
/// failed attempt touched nothing.
fn run_with_fallback(
    made: &PreloadPlan,
    native: &dyn VolumeFormatter,
    fallback: Option<&dyn VolumeFormatter>,
    sink: &dyn ProgressSink,
) -> CoreResult<(PreloadOutcome, Vec<StepReport>)> {
    let mut outcome = PreloadOutcome::default();
    let mut reports = Vec::new();
    // Which tool(s) actually did work, so the summary line above the
    // per-step list can never claim a single tool when the steps disagree
    // (fix-wave finding 4: this used to be `native.probe().ok()`
    // unconditionally, so a run where *every* step fell back still printed
    // "By libpfs3 … (native)" — the one line on the panel that contradicted
    // the per-step list sitting right beneath it).
    let mut used_native = false;
    let mut used_fallback = false;
    let total = made.steps.len() as u64;

    for (done, step) in made.steps.iter().enumerate() {
        if sink.is_cancelled() {
            return Err(CoreError::Cancelled);
        }
        sink.report(done as u64, Some(total), &step_label(step));

        // ART-122: a format whose own partition's copy must fall back goes to
        // the fallback with it, decided before the destructive step rather
        // than discovered after it.
        if let Some(reason) = paired_copy_forces_fallback(made, step, native) {
            let Some(fallback) = fallback else {
                return Err(missing_tool_error(&reason, step));
            };
            let effect = run_step(&made.image, step, fallback, sink)?;
            used_fallback = true;
            reports.push(StepReport {
                step: step.clone(),
                tool: tool_name_of(Some(fallback)),
                fallback_reason: Some(reason),
            });
            apply_effect(&mut outcome, effect);
            continue;
        }

        let native_err = match run_step(&made.image, step, native, sink) {
            Ok(effect) => {
                reports.push(StepReport {
                    step: step.clone(),
                    tool: "native".into(),
                    fallback_reason: None,
                });
                used_native = true;
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
        used_fallback = true;
        reports.push(StepReport {
            step: step.clone(),
            tool: tool_name_of(Some(fallback)),
            fallback_reason: Some(reason),
        });
        apply_effect(&mut outcome, effect);
    }

    // Only claim a tool here when every step that ran agreed on one — a
    // plan mixing native and fallback steps says so through `reports`
    // already, not through this line pretending to speak for both.
    outcome.tool = match (used_native, used_fallback) {
        (true, false) => native.probe().ok(),
        (false, true) => fallback.and_then(|f| f.probe().ok()),
        _ => None,
    };

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
        /// What this formatter says it cannot copy — answered by both
        /// `can_copy_in` (asked before the format, ART-122) and `copy_in`
        /// itself, from one field, so a double cannot claim it could do a
        /// copy it then refuses.
        copy_fails_with: Option<fn() -> CoreError>,
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
            match self.copy_fails_with {
                Some(make_err) => Err(make_err()),
                None => Ok(CopySummary {
                    files: 1,
                    ..Default::default()
                }),
            }
        }
        fn can_copy_in(
            &self,
            _i: &Path,
            _slot: Option<usize>,
            _drive: &str,
            _src: &Path,
        ) -> CoreResult<()> {
            match self.copy_fails_with {
                Some(make_err) => Err(make_err()),
                None => Ok(()),
            }
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
        assert!(
            outcome
                .tool
                .as_ref()
                .is_some_and(|t| t.raw.contains("libpfs3")),
            "the summary line should say native, not the unreachable configured tool, {outcome:?}"
        );
        assert!(reports.iter().all(|r| r.tool == "native"), "{reports:?}");
        assert!(
            reports.iter().all(|r| r.fallback_reason.is_none()),
            "{reports:?}"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// **ART-122: the tool is chosen per partition, and a partition is
    /// formatted and filled by one tool.** A `CopyIn` that must fall back
    /// takes its own partition's `FormatPartition` with it, because
    /// `hst-imager` cannot write into a volume `NativeFormatter` formatted —
    /// its first attempt dies `ERROR_DISK_FULL` (the two implementations size
    /// the PFS3 reserved area differently; ART's number is `pfs3aio`'s own).
    /// Before this, the format ran natively and the copy fell back, which is
    /// exactly the combination that fails — and it failed *after* the
    /// destructive step, leaving an empty formatted volume and an error.
    #[test]
    fn a_partition_whose_copy_must_fall_back_is_formatted_by_the_same_tool() {
        use crate::core::rdb::{AmigaHardDiskFs, PartitionSpec};

        let dir = scratch("paired-fallback");
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
        std::fs::create_dir_all(tree.join("español")).unwrap();
        std::fs::write(tree.join("español").join("Readme"), b"hola\n").unwrap();

        let made = PreloadPlan {
            image: image.clone(),
            steps: vec![
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
            ],
        };

        let recorder = Recorder::default();
        let (outcome, reports) = run_with_fallback(
            &made,
            &NativeFormatter,
            Some(&recorder as &dyn VolumeFormatter),
            &crate::core::jobs::NoProgress,
        )
        .unwrap();

        assert_eq!(reports.len(), 2, "{reports:?}");
        assert_eq!(reports[0].tool, "recorder-tool", "{reports:?}");
        assert!(
            matches!(
                reports[0].fallback_reason,
                Some(FallbackReason::PairedWithFallbackCopy { .. })
            ),
            "the format must say *why* it is not native, and the reason is \
             the copy it is paired with — not the copy's own reason, which is \
             not a fact about formatting: {reports:?}"
        );
        assert_eq!(reports[1].tool, "recorder-tool", "{reports:?}");

        // Both steps really went through the one tool, in order — and the
        // native path never formatted this partition.
        assert_eq!(*recorder.calls.borrow(), vec!["format 1 Work", "copy DH0"]);
        assert!(
            outcome
                .tool
                .as_ref()
                .is_some_and(|t| t.raw == "recorder-tool"),
            "every step fell back, so the summary follows it: {outcome:?}"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// **ART-122's other half: the choice is still per partition, not per
    /// run.** Two partitions in one plan — one whose tree is plain ASCII,
    /// one whose tree is not. The first must be formatted *and* filled
    /// natively; only the second goes to the fallback. A run-level choice
    /// would have pushed all four steps onto the external tool the moment any
    /// one of them needed it, which is what `run_with_fallback`'s own doc
    /// comment rejects.
    #[test]
    fn one_partitions_fallback_does_not_pull_another_partition_with_it() {
        use crate::core::rdb::{AmigaHardDiskFs, PartitionSpec};

        let dir = scratch("per-partition");
        let image = dir.join("card.hdf");
        crate::core::hdf::create_hdf(
            &image,
            64 * 1024 * 1024,
            true,
            &[
                PartitionSpec {
                    drive_name: "DH0".into(),
                    fs_type: AmigaHardDiskFs::Pfs3Standard,
                    size_mb: 20,
                    bootable: true,
                    boot_priority: 0,
                    num_buffers: 0,
                },
                PartitionSpec {
                    drive_name: "DH1".into(),
                    fs_type: AmigaHardDiskFs::Pfs3Standard,
                    size_mb: 20,
                    bootable: false,
                    boot_priority: 0,
                    num_buffers: 0,
                },
            ],
            &[],
        )
        .unwrap();

        let ascii = dir.join("ascii");
        std::fs::create_dir_all(&ascii).unwrap();
        std::fs::write(ascii.join("Readme"), b"plain\n").unwrap();
        let accented = dir.join("accented");
        std::fs::create_dir_all(accented.join("türkçe")).unwrap();
        std::fs::write(accented.join("türkçe").join("Readme"), b"merhaba\n").unwrap();

        let made = PreloadPlan {
            image: image.clone(),
            steps: vec![
                PreloadStep::FormatPartition {
                    slot: None,
                    index: 1,
                    drive_name: "DH0".into(),
                    volume_name: "Work".into(),
                },
                PreloadStep::CopyIn {
                    slot: None,
                    drive_name: "DH0".into(),
                    source: ascii,
                },
                PreloadStep::FormatPartition {
                    slot: None,
                    index: 2,
                    drive_name: "DH1".into(),
                    volume_name: "Games".into(),
                },
                PreloadStep::CopyIn {
                    slot: None,
                    drive_name: "DH1".into(),
                    source: accented,
                },
            ],
        };

        let recorder = Recorder::default();
        let (outcome, reports) = run_with_fallback(
            &made,
            &NativeFormatter,
            Some(&recorder as &dyn VolumeFormatter),
            &crate::core::jobs::NoProgress,
        )
        .unwrap();

        assert_eq!(reports.len(), 4, "{reports:?}");
        assert_eq!(reports[0].tool, "native", "DH0's format: {reports:?}");
        assert_eq!(reports[1].tool, "native", "DH0's copy: {reports:?}");
        assert_eq!(
            reports[2].tool, "recorder-tool",
            "DH1's format: {reports:?}"
        );
        assert_eq!(reports[3].tool, "recorder-tool", "DH1's copy: {reports:?}");
        // The external tool touched DH1 and nothing else.
        assert_eq!(*recorder.calls.borrow(), vec!["format 2 Games", "copy DH1"]);
        assert!(
            outcome.tool.is_none(),
            "a mixed run attributes itself to neither tool: {outcome:?}"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// **A partition that cannot be filled is not formatted first.** With no
    /// fallback tool configured, the run must refuse *before* the destructive
    /// step — the whole point of asking the question ahead of the format
    /// (ART-122). Previously the format succeeded and the copy then failed,
    /// which left the user with an erased partition and an error.
    #[test]
    fn a_partition_whose_copy_needs_a_missing_tool_is_not_formatted_first() {
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
                source: std::path::PathBuf::from("tree"),
            },
        ]);

        let native = Recorder {
            copy_fails_with: Some(|| CoreError::NonAsciiPfs3Names {
                paths: vec!["español".into()],
                more: 0,
            }),
            ..Default::default()
        };

        let err =
            run_with_fallback(&made, &native, None, &crate::core::jobs::NoProgress).unwrap_err();

        assert_eq!(err.code(), "ART-INPUT-INVALID", "{err}");
        assert!(
            native.calls.borrow().is_empty(),
            "nothing may be formatted when the copy that follows it cannot run: {:?}",
            native.calls.borrow()
        );
    }

    /// A `CopyIn` whose partition has no `FormatPartition` in the same plan —
    /// there is no such plan today, since `core::preload::plan` always emits
    /// the format, but `run_with_fallback` takes the plan it is given — still
    /// falls back for itself alone.
    #[test]
    fn a_copy_with_no_format_of_its_own_still_falls_back_by_itself() {
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

        let made = PreloadPlan {
            image: image.clone(),
            steps: vec![PreloadStep::CopyIn {
                slot: None,
                drive_name: "DH0".into(),
                source: tree,
            }],
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

        assert_eq!(reports.len(), 1, "{reports:?}");
        assert_eq!(reports[0].tool, "recorder-tool", "{reports:?}");
        assert!(
            matches!(
                reports[0].fallback_reason,
                Some(FallbackReason::NonAsciiPfs3Names { .. })
            ),
            "its own reason, not a paired one: {reports:?}"
        );
        // The fallback tool actually ran the copy — not simulated.
        assert_eq!(*recorder.calls.borrow(), vec!["copy DH0"]);
        assert_eq!(outcome.copied.files, 1, "the recorder's own summary");
        assert!(
            outcome
                .tool
                .as_ref()
                .is_some_and(|t| t.raw == "recorder-tool"),
            "the one step that ran fell back, so the summary follows it: {outcome:?}"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// **The summary line follows the fallback, not the default —
    /// mutation-checked for fix-wave finding 4.** A plan where *every* step
    /// needs the fallback (two `ImportFilesystem` steps; `NativeFormatter`
    /// refuses this unconditionally, for every card — ART-117) must report
    /// `outcome.tool` as the fallback's own probed version, never
    /// `native`'s. The bug this guards: `run_with_fallback` used to set
    /// `outcome.tool = native.probe().ok()` before the loop even ran, so the
    /// result panel printed "By libpfs3 … (native)" for a run where every
    /// single step actually went through `hst-imager` — contradicting the
    /// per-step list rendered right beneath it.
    #[test]
    fn every_step_falling_back_makes_the_summary_follow_the_fallback_tool() {
        let made = plan_of(vec![
            PreloadStep::ImportFilesystem {
                slot: None,
                driver: std::path::PathBuf::from("pfs3aio.lha"),
                dostype: "PDS3".into(),
                name: "pfs3aio".into(),
            },
            PreloadStep::ImportFilesystem {
                slot: None,
                driver: std::path::PathBuf::from("pfs3aio.lha"),
                dostype: "PDS3".into(),
                name: "pfs3aio-2".into(),
            },
        ]);

        // The real formatter: it refuses `import_filesystem` unconditionally,
        // for every card (see its own module doc comment), so both steps are
        // genuinely known capability gaps rather than a canned failure.
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
        assert!(
            reports.iter().all(|r| r.tool == "recorder-tool"),
            "{reports:?}"
        );
        assert!(
            reports.iter().all(|r| r.fallback_reason.is_some()),
            "{reports:?}"
        );
        assert_eq!(
            outcome.tool.as_ref().map(|v| v.raw.as_str()),
            Some("recorder-tool"),
            "the summary line must follow the tool that actually did the \
             work, not the default that never ran a single step, {outcome:?}"
        );
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

    /// Count what a real tree holds the way [`NativeFormatter`] counts it: a
    /// `.uaem` sidecar is metadata beside an entry, never an entry of its own
    /// (`core::preload::native::collect_entries` skips them), so counting the
    /// folder with Explorer or PowerShell gives roughly twice the number and
    /// makes any comparison against a copy's own summary meaningless.
    fn count_tree(dir: &std::path::Path, files: &mut u64, directories: &mut u64) {
        for entry in std::fs::read_dir(dir).expect("the tree must be readable") {
            let path = entry.expect("a readable directory entry").path();
            if path.is_dir() {
                *directories += 1;
                count_tree(&path, files, directories);
            } else if !path
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("uaem"))
            {
                *files += 1;
            }
        }
    }

    /// **The real distribution tree, carried through the product's own
    /// path** — [`run_with_fallback`], native first, `hst-imager` only where
    /// a known gap forces it.
    ///
    /// This exists because the figures the project quotes for the real
    /// AmigaOS 3.2 tree — "969 of 4030 files and 106 of 330 directories never
    /// reached a volume" — were measured *before* ART-120 made
    /// [`NativeFormatter`] reachable and `hst-imager` a per-step fallback, by
    /// a one-off manual pass that excluded every non-ASCII subtree by hand.
    /// Nobody had since carried the whole tree through the path that now
    /// ships, so nobody knew whether the fallback closes that gap or merely
    /// reaches it more cleanly. This is the run that answers it, and unlike
    /// the pass it replaces it is a committed one.
    ///
    /// ```text
    /// cd src-tauri
    /// ART_OSINSTALL_DEST="E:\amiga\ProjeART\dist-3.2" \
    /// ART_CARD_OUT="E:\amiga\ProjeART\dist-3.2-fallback.hdf" \
    /// ART_PFS3="E:\amiga\Amigatolon\hstimager\pfs3aio" \
    /// ART_HST="E:\amiga\Amigatolon\hstimager\hst.imager.exe" \
    ///   cargo test carry_the_real_dist_tree_through_the_fallback_path_when_asked \
    ///   -- --nocapture --ignored
    /// ```
    ///
    /// `ART_HST` is deliberately **optional**: without it the run measures
    /// what the native path alone does with the real tree, which is the other
    /// half of the same question — and the refusal it ends on is the one the
    /// product would show a user who has never configured `hst-imager`.
    /// `ART_PFS3` names the raw `pfs3aio` binary (not the `.lha`), because
    /// the driver is embedded by ART's own RDB writer at create time rather
    /// than by an `ImportFilesystem` step, which would only be measuring
    /// ART-117 a second time.
    ///
    /// **What it measured, 2026-08-16** (`E:\amiga\ProjeART\dist-3.2`, 3933
    /// files / 280 directories / 12.2 MB): the fallback carries the *whole*
    /// tree — non-ASCII names included — where the previous, manual
    /// measurement had to exclude about a quarter of it. It also found
    /// [ART-122](../../../../docs/ISSUES.md): `hst-imager`'s **first** write
    /// into a volume `NativeFormatter` has just formatted dies with
    /// `ERROR_DISK_FULL`, and the run therefore ends on a refusal *after* the
    /// destructive format step has already run. Until that is fixed, this
    /// hook's honest result is the refusal — the full-tree figures above came
    /// from repeating the copy by hand, which is exactly what the product
    /// must not have to do.
    #[test]
    #[ignore = "reads a real distribution tree and writes a multi-hundred-MB image; run explicitly"]
    fn carry_the_real_dist_tree_through_the_fallback_path_when_asked() {
        use crate::core::hdf::create_hdf;
        use crate::core::jobs::NoProgress;
        use crate::core::rdb::{
            version_from_ver_string, AmigaHardDiskFs, FileSystemSpec, PartitionSpec,
        };

        let (Ok(dist), Ok(card_out), Ok(driver_path)) = (
            std::env::var("ART_OSINSTALL_DEST"),
            std::env::var("ART_CARD_OUT"),
            std::env::var("ART_PFS3"),
        ) else {
            return;
        };
        let tree = std::path::PathBuf::from(&dist);
        let image = std::path::PathBuf::from(&card_out);
        assert!(tree.is_dir(), "'{}' is not a folder", tree.display());
        assert!(
            !image.exists(),
            "'{}' already exists — SAFE_CREATE: remove it yourself first, \
             or point ART_CARD_OUT somewhere new",
            image.display()
        );

        let mut source_files = 0;
        let mut source_dirs = 0;
        count_tree(&tree, &mut source_files, &mut source_dirs);
        println!(
            "source={} files={source_files} directories={source_dirs}",
            tree.display()
        );

        // The driver goes in at create time, by ART's own RDB writer (G4), so
        // the plan below carries no `ImportFilesystem` step and the run
        // measures the copy rather than ART-117.
        let driver = std::fs::read(&driver_path).expect("the pfs3aio binary named by ART_PFS3");
        let (version, revision) =
            version_from_ver_string(&driver).expect("pfs3aio states its own version");
        println!(
            "driver={driver_path} version={version}.{revision} bytes={}",
            driver.len()
        );

        create_hdf(
            &image,
            512 * 1024 * 1024,
            true,
            &[PartitionSpec {
                drive_name: "DH0".into(),
                fs_type: AmigaHardDiskFs::Pfs3DirectScsi,
                size_mb: 400,
                bootable: true,
                boot_priority: 0,
                num_buffers: 0,
            }],
            &[FileSystemSpec {
                dos_type: u32::from_be_bytes([b'P', b'D', b'S', 3]),
                version,
                revision,
                data: driver,
            }],
        )
        .expect("a fresh PFS3 image with the driver embedded");

        let request = PreloadRequest {
            image: image.clone(),
            driver: None,
            partitions: vec![PreloadPartition {
                area: 1,
                index: 1,
                volume_name: "Workbench".into(),
                content: Some(tree.clone()),
            }],
        };
        let made = plan(&request).expect("a plan for the real tree");
        for step in &made.steps {
            println!("  step: {}", step_label(step));
        }

        let fallback = std::env::var("ART_HST").ok().map(HstImager::at);
        println!(
            "fallback={}",
            fallback
                .as_ref()
                .map(|tool| format!("{:?}", tool.probe()))
                .unwrap_or_else(|| "not configured".into())
        );

        let outcome = run_with_fallback(
            &made,
            &NativeFormatter,
            fallback.as_ref().map(|tool| tool as &dyn VolumeFormatter),
            &NoProgress,
        );

        match outcome {
            Ok((outcome, reports)) => {
                for report in &reports {
                    println!(
                        "  ran: {} by {}{}",
                        step_label(&report.step),
                        report.tool,
                        report
                            .fallback_reason
                            .as_ref()
                            .map(|reason| format!(" — fallback: {}", reason.detail()))
                            .unwrap_or_default()
                    );
                }
                // Machine-readable, the same `key=value` convention the two
                // PFS3 oracle hooks and the card hook in
                // `core::preload::native` already print.
                println!(
                    "copied files={} directories={} bytes={} comments_lost={} dates_lost={}",
                    outcome.copied.files,
                    outcome.copied.directories,
                    outcome
                        .copied
                        .bytes
                        .map_or("not answered".into(), |n| n.to_string()),
                    outcome.copied.comments_lost,
                    outcome.copied.dates_lost,
                );
                println!("source files={source_files} directories={source_dirs}");
            }
            Err(err) => {
                // Not a panic: a refusal *is* a result here. Without
                // `hst-imager` configured this is the expected end of the run
                // for any tree carrying a non-ASCII AmigaDOS name, and it is
                // exactly what the product would tell the user.
                println!("refused={} code={}", err, err.code());
            }
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

            // ART-122's own variant, which the format step now carries.
            let paired = serde_json::to_value(FallbackReason::PairedWithFallbackCopy {
                drive: "DH0".into(),
            })
            .unwrap();
            expect_keys(&paired, &["reason", "drive"]);
            assert_eq!(paired["reason"], "paired-with-fallback-copy");
            assert_eq!(paired["drive"], "DH0");
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
