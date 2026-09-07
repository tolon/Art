//! Rehearsing a first boot under WinUAE.
//!
//! One difference from [`super::run`]: **the tree copy is the boot device.**
//! ART's own work volume is not mounted, so the chain that runs is the one a
//! real machine runs — the release's `Startup-Sequence` → `S:User-Startup` →
//! the block → the dispatcher. The host polls `S/FirstBoot.log` inside the
//! copy; a write into a `filesystem2=rw` directory mount is visible on the
//! host while the emulator runs (measured 2026-08-20, `super::run`'s module
//! documentation).
//!
//! What it proves and does not: under UAE `10-hardware` writes `skipped uae`
//! and copies nothing, so the rehearsal proves the mechanism and never the
//! Pi3/Pi4 branch. That closes only on a real card.

use std::path::Path;
use std::time::Duration;

use serde::Serialize;

use super::run::{deadline_secs, end_session, Clock, EmulatorLauncher, EmulatorSession, RunLimits};
use crate::core::error::{CoreError, CoreResult};
use crate::core::firstboot::report::{parse_bytes, Ending, FirstBootReport, StepOutcome};
use crate::core::firstboot::REPORT_PATH;
use crate::core::jobs::ProgressSink;
use crate::core::profile::AmigaProfile;
use crate::core::winuae::{generate_uae_config, DirMount, LaunchMedia};

/// The WinUAE device name and Amiga label for the booted copy. The label is
/// what `SYS:` resolves to; the release's own sequence uses `SYS:` throughout,
/// so the label's exact text does not matter to it.
const BOOT_DEVICE: &str = "DH0";
const BOOT_LABEL: &str = "ARTSys";

/// Boot priority for the tree copy — the highest an `i8` holds, so nothing
/// else mounted could outrank it. There is nothing else mounted in a
/// rehearsal, but the number is the same one [`super::run::WORK_BOOT_PRIORITY`]
/// uses for "this is what boots," rather than an unrelated value that happens
/// to also be positive.
const BOOT_PRIORITY: i8 = 127;

#[derive(Debug, Clone, Copy)]
pub struct RehearseRequest<'a> {
    /// The **copy** of the distribution tree — never the original. It is the
    /// only mount, and it boots.
    pub tree_copy: &'a Path,
    /// Where the generated emulator configuration is written (ART-196).
    pub scratch_root: &'a Path,
    /// The hardware the rehearsal runs on.
    pub profile: &'a AmigaProfile,
    /// The user's own licensed Kickstart. Never shipped by ART.
    pub kickstart_path: &'a Path,
    /// The emulator ART will start.
    ///
    /// **Read by nothing in `core/`.** [`rehearse_with`] takes an
    /// `EmulatorLauncher` directly, and building the real one is a
    /// process-spawning decision that `core/` may not make (ART-274) — the
    /// caller builds `tools::winuae_launcher::WinUaeLauncher` from this same
    /// path itself. The field stays here so a `RehearseRequest` still says
    /// everything a rehearsal needs in one place.
    pub winuae_path: &'a Path,
    pub limits: RunLimits,
}

/// The four endings a rehearsal can have. Distinct on purpose (spec's
/// "the failure that does not crash"): a step that refused is not the same
/// sentence as one that finished, and a closed window is not a timeout.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum RehearsalOutcome {
    /// `done all`, and no step refused.
    Finished { report: FirstBootReport },
    /// `done all` or `done partial` with at least one step refused — the
    /// dispatcher reached its end, but not every step said yes.
    StepRefused { report: FirstBootReport },
    /// The deadline passed with no `done` line. Nobody answered in time; it
    /// is not a claim that anything is broken.
    TimedOut {
        waited: Duration,
        report: FirstBootReport,
    },
    /// The emulator process ended before the dispatcher wrote `done`. Carries
    /// whatever was written up to that point, because a run that got partway
    /// still told the host something.
    EmulatorClosed {
        waited: Duration,
        report: FirstBootReport,
    },
}

/// Build the one-mount [`LaunchMedia`] a rehearsal boots from.
///
/// Unlike [`super::run::media_for`], there is exactly one mount and it *is*
/// the boot device: no ART work volume, no package wrapper, because a
/// rehearsal is checking whether the tree itself boots and runs its own
/// first-boot block, not running an installer against it.
pub fn media_for(request: &RehearseRequest) -> CoreResult<LaunchMedia> {
    // ART ships no Kickstart. `generate_uae_config` would silently fall back
    // to AROS for a missing one, and a rehearsal under a ROM the user did not
    // choose could report on a machine nobody meant to boot.
    if !request.kickstart_path.is_file() {
        return Err(CoreError::InvalidInput(format!(
            "an Amiga-side rehearsal needs the user's own Kickstart ROM; '{}' is not a file",
            request.kickstart_path.display()
        )));
    }

    Ok(LaunchMedia {
        kickstart_path: Some(request.kickstart_path.to_string_lossy().to_string()),
        directories: vec![DirMount {
            host_path: request.tree_copy.to_string_lossy().to_string(),
            volume: BOOT_DEVICE.to_string(),
            label: BOOT_LABEL.to_string(),
            boot_priority: BOOT_PRIORITY,
            read_only: false,
        }],
        ..LaunchMedia::default()
    })
}

/// The rehearsal, with its emulator and its clock supplied.
///
/// There is no thin `rehearse()` wrapper choosing a real `WinUaeLauncher`
/// any more (ART-274): building one is a process-spawning decision, so it
/// belongs in `tools::winuae_launcher`, and `core/` may not make it. Every
/// caller — `commands/firstboot.rs`'s command and its gated real-material
/// hook alike — constructs `tools::winuae_launcher::WinUaeLauncher` itself
/// and calls this directly with it and a `core::amigainstall::run::RealClock`.
///
/// Returns [`CoreError::Cancelled`] when the user stopped it — cancellation is
/// not a fifth outcome, because a rehearsal that was stopped produced no
/// answer to report.
pub fn rehearse_with(
    request: &RehearseRequest,
    launcher: &dyn EmulatorLauncher,
    clock: &dyn Clock,
    sink: &dyn ProgressSink,
) -> CoreResult<RehearsalOutcome> {
    let media = media_for(request)?;
    let config = generate_uae_config(request.profile, &media)?;
    let report_file = request.tree_copy.join(REPORT_PATH);

    // Before the launch is the cheapest place to stop: nothing has started,
    // so there is nothing to terminate and nothing to leave behind.
    if sink.is_cancelled() {
        return Err(CoreError::Cancelled);
    }
    sink.report(0, deadline_secs(&request.limits), "Starting the emulator");
    let mut session = launcher.launch(&config)?;

    // Every ending goes through the two lines below, the same shape
    // `run_with` uses and for the same reason: keeping the session alive
    // until `end_session` runs is what stops a transient read error from
    // orphaning a WinUAE window on the owner's desktop.
    let ending = poll(request, session.as_mut(), clock, sink, &report_file);
    end_session(session.as_mut(), sink);
    ending
}

/// The poll loop. Everything it returns — including an error — reaches the
/// caller through [`rehearse_with`]'s single `end_session`.
fn poll(
    request: &RehearseRequest,
    session: &mut dyn EmulatorSession,
    clock: &dyn Clock,
    sink: &dyn ProgressSink,
    report_file: &Path,
) -> CoreResult<RehearsalOutcome> {
    loop {
        // The report is read first, every time round, for the same reason
        // `run_with` reads its result file first: an answer that landed
        // during the last sleep outranks both the deadline and a
        // cancellation.
        let report = read_report(report_file)?;
        if let Some(done) = finished(&report) {
            return Ok(done);
        }

        // Between whole polls, never inside one.
        if sink.is_cancelled() {
            return Err(CoreError::Cancelled);
        }

        // The emulator going away without a `done` line is its own ending,
        // and deliberately not the deadline's — see `run.rs`'s module
        // documentation for why the two carry different advice. One final
        // read first, in case the report finished in the instant before the
        // process went.
        if !session.is_running()? {
            let report = read_report(report_file)?;
            if let Some(done) = finished(&report) {
                return Ok(done);
            }
            return Ok(RehearsalOutcome::EmulatorClosed {
                waited: clock.elapsed(),
                report,
            });
        }

        let waited = clock.elapsed();
        if waited >= request.limits.deadline {
            return Ok(RehearsalOutcome::TimedOut { waited, report });
        }

        let so_far = report.steps.len();
        sink.report(
            waited.as_secs(),
            deadline_secs(&request.limits),
            &format!("Waiting for the Amiga — {so_far} steps reported so far"),
        );
        clock.sleep(request.limits.poll_interval);
    }
}

/// Read `S/FirstBoot.log` from inside the tree copy. A missing file is not an
/// Amiga writing an unrecognised answer; it is "has not reached the marker
/// yet," and [`parse_bytes`] already reports that as [`Ending::NotBooted`]
/// for an empty slice.
fn read_report(path: &Path) -> CoreResult<FirstBootReport> {
    match std::fs::read(path) {
        Ok(bytes) => Ok(parse_bytes(&bytes)),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(parse_bytes(b"")),
        Err(err) => Err(CoreError::Io(err)),
    }
}

/// `done` is the only ending the Amiga writes; which of the two outcomes it
/// is comes from whether any step refused, not from the `all`/`partial` word
/// alone — a `partial` with no refusal is a reboot request, phase 3's case.
fn finished(report: &FirstBootReport) -> Option<RehearsalOutcome> {
    match report.ending {
        Ending::DoneAll | Ending::DonePartial => {
            let refused = report
                .steps
                .iter()
                .any(|s| matches!(s.outcome, StepOutcome::Refused { .. }));
            Some(if refused {
                RehearsalOutcome::StepRefused {
                    report: report.clone(),
                }
            } else {
                RehearsalOutcome::Finished {
                    report: report.clone(),
                }
            })
        }
        Ending::NotBooted | Ending::Unfinished => None,
    }
}

#[cfg(test)]
mod tests {
    use super::super::run::fakes::{CancelAfter, FakeLauncher, TestClock};
    use super::*;
    use crate::core::jobs::NoProgress;
    use crate::core::profile::AmigaProfile;
    use crate::core::ScratchDir;
    use std::fs;
    use std::path::Path;

    fn copy_with_report(tag: &str, report: &str) -> ScratchDir {
        let d = ScratchDir::new("art-firstboot-rehearse", tag);
        fs::create_dir_all(d.join("S")).unwrap();
        fs::write(d.join("S/FirstBoot.log"), report).unwrap();
        d
    }

    fn request<'a>(
        copy: &'a Path,
        scratch: &'a Path,
        profile: &'a AmigaProfile,
        rom: &'a Path,
    ) -> RehearseRequest<'a> {
        RehearseRequest {
            tree_copy: copy,
            scratch_root: scratch,
            profile,
            kickstart_path: rom,
            winuae_path: rom, // unused by rehearse_with
            limits: RunLimits {
                deadline: Duration::from_secs(10),
                poll_interval: Duration::from_millis(1),
            },
        }
    }

    #[test]
    fn the_tree_copy_is_the_only_mount_and_it_boots() {
        let d = ScratchDir::new("art-firstboot-rehearse", "media");
        let profile = AmigaProfile::a1200_aga();
        let rom = d.join("kick.rom");
        fs::write(&rom, vec![0u8; 16]).unwrap();
        let req = request(d.path(), d.path(), &profile, &rom);

        let media = media_for(&req).unwrap();

        assert_eq!(media.directories.len(), 1);
        assert_eq!(media.directories[0].boot_priority, 127);
        assert!(
            !media.directories[0].read_only,
            "the steps delete themselves and write the report"
        );
        assert_eq!(media.directories[0].host_path, d.path().to_string_lossy());
    }

    #[test]
    fn a_missing_kickstart_is_refused_before_anything_launches() {
        let d = ScratchDir::new("art-firstboot-rehearse", "no-kick");
        let profile = AmigaProfile::a1200_aga();
        let missing = d.join("nothing-here.rom");
        let req = request(d.path(), d.path(), &profile, &missing);

        let err = media_for(&req).unwrap_err();

        assert!(
            matches!(err, CoreError::InvalidInput(ref m) if m.contains("Kickstart")),
            "got {err:?}"
        );
    }

    #[test]
    fn done_all_ends_as_finished_and_done_partial_with_a_refusal_as_step_refused() {
        let profile = AmigaProfile::a1200_aga();
        for (text, expect_finished) in [
            (
                "art-firstboot 1\nstep 10-hardware started\nstep 10-hardware skipped uae\nstep 10-hardware ok\ndone all\n",
                true,
            ),
            (
                "art-firstboot 1\nstep 10-hardware started\nstep 10-hardware refused rc=20\ndone partial\n",
                false,
            ),
        ] {
            let d = copy_with_report(if expect_finished { "fin" } else { "ref" }, text);
            let rom = d.join("kick.rom");
            fs::write(&rom, vec![0u8; 16]).unwrap();
            let req = request(d.path(), d.path(), &profile, &rom);

            let outcome = rehearse_with(
                &req,
                &FakeLauncher::running_forever(),
                &TestClock::idle(),
                &NoProgress,
            )
            .unwrap();

            match (outcome, expect_finished) {
                (RehearsalOutcome::Finished { report }, true) => {
                    assert_eq!(report.ending, Ending::DoneAll)
                }
                (RehearsalOutcome::StepRefused { report }, false) => {
                    assert_eq!(report.ending, Ending::DonePartial)
                }
                (other, _) => panic!("wrong ending: {other:?}"),
            }
        }
    }

    #[test]
    fn a_closed_window_before_done_is_emulator_closed_and_carries_what_was_written() {
        let profile = AmigaProfile::a1200_aga();
        let d = copy_with_report("closed", "art-firstboot 1\nstep 10-hardware started\n");
        let rom = d.join("kick.rom");
        fs::write(&rom, vec![0u8; 16]).unwrap();
        let req = request(d.path(), d.path(), &profile, &rom);

        let outcome = rehearse_with(
            &req,
            &FakeLauncher::exits_immediately(),
            &TestClock::idle(),
            &NoProgress,
        )
        .unwrap();

        match outcome {
            RehearsalOutcome::EmulatorClosed { report, .. } => {
                assert_eq!(report.ending, Ending::Unfinished)
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn the_deadline_is_a_timeout_not_a_failure() {
        let profile = AmigaProfile::a1200_aga();
        let d = copy_with_report("late", "art-firstboot 1\n");
        let rom = d.join("kick.rom");
        fs::write(&rom, vec![0u8; 16]).unwrap();
        let mut req = request(d.path(), d.path(), &profile, &rom);
        // A deadline equal to the poll interval so the fake clock's very
        // first sleep already reaches it — nothing here waits for real time,
        // the loop simply runs one iteration.
        req.limits = RunLimits {
            deadline: Duration::from_secs(1),
            poll_interval: Duration::from_secs(1),
        };

        let outcome = rehearse_with(
            &req,
            &FakeLauncher::running_forever(),
            &TestClock::idle(),
            &NoProgress,
        )
        .unwrap();

        assert!(matches!(outcome, RehearsalOutcome::TimedOut { .. }));
    }

    #[test]
    fn stop_before_launch_is_cancelled_and_launches_nothing() {
        let profile = AmigaProfile::a1200_aga();
        let d = copy_with_report("stop", "");
        let rom = d.join("kick.rom");
        fs::write(&rom, vec![0u8; 16]).unwrap();
        let req = request(d.path(), d.path(), &profile, &rom);

        let launcher = FakeLauncher::running_forever();
        // Cancelled from the very first check, i.e. before the launch.
        let sink = CancelAfter::new(0);

        let err = rehearse_with(&req, &launcher, &TestClock::idle(), &sink).unwrap_err();

        assert!(matches!(err, CoreError::Cancelled));
        assert_eq!(launcher.launches(), 0);
    }
}
