//! The run itself: launch, poll, deadline, terminate.
//!
//! Everything above this module produced *data* — a work volume with one
//! generated script ([`super::workvol`]) and a [`PlannedRun`] saying what the
//! Amiga should execute. This module is the only part of the round that starts
//! a machine, and it is written so that **none of its behaviour needs one**.
//!
//! ## What the design measured, and what follows from it
//!
//! On 2026-08-20 a file the Amiga wrote into a `filesystem2=rw` directory
//! mount was seen on the host **while WinUAE was still running**, with the pid
//! confirmed alive at that moment. So the host *polls*: it does not wait for
//! the emulator to exit, and nothing anywhere needs the Amiga to be able to
//! quit it. That single measurement is why this module is a loop around
//! [`super::RESULT_FILE`] rather than a `wait()`.
//!
//! ## Three rules, and they are the whole module
//!
//! 1. **Poll, do not busy-wait.** A sleep between reads, and
//!    `is_cancelled()` checked *between* polls — never inside one.
//! 2. **The deadline is not optional.** An Amiga Installer is interactive, so
//!    a run stopped on a requester would otherwise wait for ever. When the
//!    deadline expires ART terminates the emulator and returns
//!    [`RunOutcome::TimedOut`] — never [`RunOutcome::Failed`]. A failure means
//!    the installer said no; a timeout means nobody was there to answer it,
//!    and the two are fixed by different things.
//! 3. **ART owns the process it started, and only that one.** The emulator is
//!    ended through the handle [`crate::core::winuae::launch_winuae_process`]
//!    returned, never by name and never by a bare number: the owner may have
//!    their own WinUAE open, and ending it would be ART destroying something
//!    it does not own.
//!
//! ## Why there are two seams
//!
//! [`EmulatorLauncher`] and [`Clock`] are traits for the reason
//! `core/preload`'s `VolumeFormatter` is one: the platform-specific half of an
//! operation lives behind a trait so the operation itself stays testable, and
//! `core/` stays free of anything but `std` (the core-independence rule).
//!
//! Here that has a second, sharper purpose. **The tests must not open an
//! emulator window on the owner's desktop**, and a deadline test that sleeps
//! for real and hopes is the same coin toss ART-182 was — a race that failed
//! three runs in six on one machine and none in six on another. With both the
//! launcher and the clock injected, every test in this file asserts a
//! *property* of the loop and not one machine's timing: no process is started,
//! no wall-clock time passes, and the outcome is the same on every run.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use super::{
    claims_package_volume, claims_work_volume, PlannedRun, RunOutcome, MARK_FAILED, MARK_OK,
    MARK_STARTED, PACKAGE_VOLUME, WORK_VOLUME,
};
use crate::core::error::{CoreError, CoreResult};
use crate::core::jobs::ProgressSink;
use crate::core::profile::AmigaProfile;
use crate::core::winuae::{
    generate_uae_config, launch_winuae_process, DirMount, LaunchMedia, WinUaeProcess,
};

/// How long a run may go without an answer before ART ends it.
///
/// The design (§6) says picking this is *"a judgement about real installers on
/// a real machine, and it should be measured against the owner's own packages
/// rather than chosen — a BoingBag's `Updater` on this hardware has a running
/// time, and the deadline should be a multiple of it, recorded with what it
/// was measured from."*
///
/// ## What was measured, on 2026-08-21
///
/// Against the owner's own `BoingBag39-1 (1).lha` (`Updater` 45.15), their
/// licensed Kickstart 40.68, WinUAE on the A1200/AGA preset (`cpu_speed=real`,
/// so the emulated 68020 runs at A1200 speed), and a real 3 795-file AmigaOS
/// 3.9 tree:
///
/// | What | Measured |
/// |---|---|
/// | Boot, `SetPatch`, its reset, second boot, up to the installer | ~25 s |
/// | An `Updater` that **refuses** — answer written and read on the host | 16.0 s end to end |
/// | An `Updater` that **starts work** | ran 414 s and 422 s in two runs and produced nothing; both ended by terminating the emulator |
///
/// ## What was *not* measured, and why this is not the multiple §6 asks for
///
/// **No successful install has been observed**, so there is no successful
/// running time to take a multiple of. The reason is
/// [`CoreError`](crate::core::error::CoreError)-shaped only in the report:
/// ART-193 — a BoingBag's `Updater` checks for the original AmigaOS 3.9
/// CD-ROM on a volume ART does not mount, and stalls. Until that is fixed,
/// §6's sentence cannot be honoured, and writing a number here as though it
/// had been would be exactly the *"guessed number wearing the clothes of a
/// measured one"* this round was told not to produce.
///
/// So the value below is anchored to the one real over-run rather than to a
/// successful install: 20 minutes was the guess, and the first real package to
/// meet it exceeded it. Tripling it puts the deadline well clear of the
/// running times the Amiga community reports for a BoingBag on real A1200
/// hardware (tens of minutes, not hours), which is the class of machine this
/// preset emulates at its own clock. **It is a bound, not a measurement of a
/// finished install**, and the day ART-193 closes, this constant gets the
/// multiple §6 actually asks for.
///
/// The cost of erring high is small and the cost of erring low is not: a
/// deadline that is too long makes a genuinely stuck run take longer to be
/// reported as stuck, while one that is too short reports a *working* install
/// as a timeout and throws its copy away — a confidently wrong sentence, which
/// is the failure this whole round exists to stop (§89).
///
/// It is a default, not a policy: every run takes its deadline from
/// [`RunLimits`], so the value travels as data.
pub const PROVISIONAL_DEADLINE: Duration = Duration::from_secs(60 * 60);

/// How long to wait between two reads of the result file.
///
/// Short enough that a finished install is noticed promptly, long enough that
/// the loop costs nothing: an install measured in minutes is not made faster
/// by reading a file more often.
pub const POLL_INTERVAL: Duration = Duration::from_secs(2);

/// The Amiga device name ART's own work volume is mounted under.
///
/// Deliberately the same string as the volume label ([`WORK_VOLUME`]) rather
/// than a `DH` number: the generated script addresses it as `ARTWork:`, and
/// one name that is true both as a device and as a label is one thing that can
/// be wrong instead of two.
pub const WORK_DEVICE: &str = WORK_VOLUME;

/// The Amiga device name the package's own unpacked wrapper is mounted under,
/// for the same reason [`WORK_DEVICE`] is the same string as its label.
pub const PACKAGE_DEVICE: &str = PACKAGE_VOLUME;

/// Boot priority for ART's own volume — the highest an `i8` holds, so nothing
/// the user's tree could carry outranks it. This is the same mechanism as
/// "one click starts the game" (`commands/launch.rs`), which gives ART's boot
/// directory the highest priority of anything mounted for exactly this reason.
const WORK_BOOT_PRIORITY: i8 = 127;

/// Boot priority for the distribution tree. `-128` is the RDB convention for
/// *not bootable*, and it is what `commands/launch.rs` already uses for a
/// directory mounted as data. The design's rule is that the user's tree is
/// mounted as data and never as the boot device; this is that rule as a
/// number.
const TREE_BOOT_PRIORITY: i8 = -128;

/// Boot priority for the package's own unpacked wrapper. Data, exactly like
/// the tree: it holds a program ART runs *from* a shell that has already
/// booted, and an unpacked BoingBag has no `S/Startup-Sequence` of its own to
/// boot from anyway.
const PACKAGE_BOOT_PRIORITY: i8 = -128;

/// The clock a run measures its deadline against.
///
/// A trait rather than [`Instant`] because a deadline test that waits for real
/// time is a test that sometimes passes. See the module documentation.
pub trait Clock {
    /// How long the run has been going. Monotonic: it never goes backwards.
    fn elapsed(&self) -> Duration;

    /// Wait before the next poll. Advances [`elapsed`](Self::elapsed) by at
    /// least `interval`.
    fn sleep(&self, interval: Duration);
}

/// The real one: wall-clock time, and a thread that actually sleeps.
#[derive(Debug)]
pub struct RealClock {
    started: Instant,
}

impl Default for RealClock {
    fn default() -> Self {
        Self::new()
    }
}

impl RealClock {
    pub fn new() -> Self {
        Self {
            started: Instant::now(),
        }
    }
}

impl Clock for RealClock {
    fn elapsed(&self) -> Duration {
        self.started.elapsed()
    }

    fn sleep(&self, interval: Duration) {
        std::thread::sleep(interval);
    }
}

/// An emulator process ART started and may therefore end.
///
/// The methods take `&mut self` because the real implementation reaps the
/// child, which is a mutation of the handle rather than of the world.
pub trait EmulatorSession {
    /// The process id — for reporting only. Ending the process is
    /// [`terminate`](Self::terminate)'s job precisely so that no caller is
    /// ever handed a number to kill.
    fn pid(&self) -> u32;

    /// Whether the emulator is still alive.
    fn is_running(&mut self) -> CoreResult<bool>;

    /// End it. Must be idempotent: a run terminates on every ending it has,
    /// including the ones where the process is already gone.
    fn terminate(&mut self) -> CoreResult<()>;
}

/// Starts an emulator. The seam a test substitutes so that no window opens.
pub trait EmulatorLauncher {
    fn launch(&self, config_text: &str) -> CoreResult<Box<dyn EmulatorSession>>;
}

/// The real launcher: WinUAE, at the path the user configured.
#[derive(Debug, Clone)]
pub struct WinUaeLauncher {
    executable: PathBuf,
}

impl WinUaeLauncher {
    pub fn new(executable: impl Into<PathBuf>) -> Self {
        Self {
            executable: executable.into(),
        }
    }
}

/// A newtype rather than an `impl` on [`WinUaeProcess`] itself, because the
/// trait's method names and the inherent ones are identical: written directly
/// on the type, every trait body would be a call that resolves by precedence
/// rules instead of by saying what it means.
struct WinUaeSession(WinUaeProcess);

impl EmulatorSession for WinUaeSession {
    fn pid(&self) -> u32 {
        self.0.pid()
    }

    fn is_running(&mut self) -> CoreResult<bool> {
        self.0.is_running()
    }

    fn terminate(&mut self) -> CoreResult<()> {
        self.0.terminate()
    }
}

impl EmulatorLauncher for WinUaeLauncher {
    fn launch(&self, config_text: &str) -> CoreResult<Box<dyn EmulatorSession>> {
        Ok(Box::new(WinUaeSession(launch_winuae_process(
            &self.executable,
            config_text,
        )?)))
    }
}

/// How long a run may take, and how often it is asked.
///
/// Data, not constants baked into the loop, so the deadline Task 8 measures
/// arrives here without touching [`run_with`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RunLimits {
    pub deadline: Duration,
    pub poll_interval: Duration,
}

impl Default for RunLimits {
    fn default() -> Self {
        Self {
            deadline: PROVISIONAL_DEADLINE,
            poll_interval: POLL_INTERVAL,
        }
    }
}

/// Everything a run needs that is not the plan itself.
///
/// A struct rather than a long argument list because every field is a *place*
/// and three of them are directories that must not be confused: `tree_dir` is
/// the copy being installed into, `work_volume_dir` is ART's own volume
/// carrying the script and the result file, and `package_volume_dir` is the
/// package's own wrapper, unpacked.
#[derive(Debug, Clone, Copy)]
pub struct RunRequest<'a> {
    /// What the Amiga will execute. Validated when the work volume was built.
    pub plan: &'a PlannedRun,
    /// ART's own boot volume, as written by [`super::workvol::build`].
    pub work_volume_dir: &'a Path,
    /// The **copy** of the distribution tree. Never the original: the copy
    /// replaces it only when the result says the run succeeded (§92).
    pub tree_dir: &'a Path,
    /// The package's own wrapper, unpacked, as written by
    /// [`super::packagevol::unpack`] — the host side of the third mount.
    ///
    /// **Added by ART-185**, deliberately amending a type an earlier task of
    /// this round shipped. Without it `media_for` mounted two volumes and the
    /// program the whole round exists to run was in neither, which does not
    /// fail loudly: `CD` fails, the shell finds nothing, the script's
    /// `If Warn` writes [`MARK_FAILED`], and ART reports that the installer
    /// ran and said no about a program that never started.
    pub package_volume_dir: &'a Path,
    /// The hardware the installer runs on.
    pub profile: &'a AmigaProfile,
    /// The user's **own** licensed Kickstart. ART ships none and never will,
    /// so a run without one is refused rather than quietly falling back to
    /// AROS — an installer that failed under a ROM the user did not choose
    /// would be a failure ART invented.
    pub kickstart_path: &'a Path,
    /// The emulator ART will start. Unused by [`run_with`], which is given a
    /// launcher directly.
    pub winuae_path: &'a Path,
    pub limits: RunLimits,
}

/// The **three** volumes a run mounts, in the order the config lists them.
///
/// Split out from [`run_with`] because it is the half of the launch that can
/// be asserted without any emulator at all: which directory boots, which ones
/// are data, and that none of them can shadow another.
///
/// It listed two until ART-185. The third — the package's own unpacked
/// wrapper — is where the installer actually lives, and its absence is the
/// defect this function is named in: the run started, `CD` failed, and ART
/// reported an answer from a program that was never on any mounted volume.
pub fn media_for(request: &RunRequest) -> CoreResult<LaunchMedia> {
    // ART ships no Kickstart. `generate_uae_config` would silently fall back
    // to AROS for a missing one, and a run under a ROM the user did not choose
    // could fail for reasons that have nothing to do with the package.
    if !request.kickstart_path.is_file() {
        return Err(CoreError::InvalidInput(format!(
            "an Amiga-side install needs the user's own Kickstart ROM; '{}' is not a file",
            request.kickstart_path.display()
        )));
    }

    // Without the script there is nothing to run and nothing to report, so the
    // run could only ever end in a deadline — twenty minutes to discover that
    // a directory was empty. Refusing costs one `is_file`.
    let script = request.work_volume_dir.join("S").join("Startup-Sequence");
    if !script.is_file() {
        return Err(CoreError::InvalidInput(format!(
            "ART's work volume at '{}' carries no Startup-Sequence; build it first",
            request.work_volume_dir.display()
        )));
    }

    // Two mounts under one device name is one mount: the second would shadow
    // the first, and the shadowed one is ART's own volume — the run would then
    // poll for a result file inside the user's tree, where nothing writes it.
    //
    // It asks `super::claims_work_volume` rather than comparing here. A second
    // comparison written in this function did the case-insensitive half and
    // missed the `ARTWork:` trailing-colon form, so a name `workvol::build`
    // refuses would have passed *this* guard and produced exactly the
    // shadowing mount it exists to prevent.
    if claims_work_volume(&request.plan.system_volume) {
        return Err(CoreError::SafetyRefused(format!(
            "the system tree may not be mounted as '{WORK_DEVICE}'; that is ART's own work volume"
        )));
    }

    // The same rule for the third mount, and it is not the same rule twice:
    // a tree mounted as `ARTPkg` shadows the *package*, and a shadowed package
    // is the installer unreachable — ART-185 arriving through a name instead
    // of through a missing mount.
    if claims_package_volume(&request.plan.system_volume) {
        return Err(CoreError::SafetyRefused(format!(
            "the system tree may not be mounted as '{PACKAGE_DEVICE}'; that is where ART mounts \
             the package's own files"
        )));
    }

    // The package has to actually be there. An empty or absent directory
    // mounts perfectly well and produces exactly the silent shape: `CD`
    // fails, the shell finds no program, and the script reports that the
    // installer said no. `packagevol::unpack` proves the installer itself
    // arrived; this is the last gate before the emulator, and it costs one
    // `is_dir`.
    if !request.package_volume_dir.is_dir() {
        return Err(CoreError::InvalidInput(format!(
            "the package's own files must be unpacked before the run; '{}' is not a directory",
            request.package_volume_dir.display()
        )));
    }

    Ok(LaunchMedia {
        kickstart_path: Some(request.kickstart_path.to_string_lossy().to_string()),
        directories: vec![
            // The user's tree: data, explicitly not bootable.
            DirMount {
                host_path: request.tree_dir.to_string_lossy().to_string(),
                volume: request.plan.system_volume.clone(),
                label: request.plan.system_volume.clone(),
                boot_priority: TREE_BOOT_PRIORITY,
                read_only: false,
            },
            // The package's own wrapper, unpacked. Data, like the tree.
            //
            // **Writable, deliberately.** Nothing of the user's is here — it
            // is ART's own scratch copy, discarded when the job ends — and a
            // read-only mount would make an installer that writes a log or a
            // temporary file beside itself fail for a reason ART invented,
            // which is the same mistake as booting AROS because no Kickstart
            // was given. What the run is *not* allowed to reach is ART's work
            // volume, and that is guarded above by name.
            DirMount {
                host_path: request.package_volume_dir.to_string_lossy().to_string(),
                volume: PACKAGE_DEVICE.to_string(),
                label: PACKAGE_VOLUME.to_string(),
                boot_priority: PACKAGE_BOOT_PRIORITY,
                read_only: false,
            },
            // ART's own volume: the highest priority of anything mounted, so
            // AmigaDOS boots this and reads ART's script rather than the
            // user's Startup-Sequence.
            DirMount {
                host_path: request.work_volume_dir.to_string_lossy().to_string(),
                volume: WORK_DEVICE.to_string(),
                label: WORK_VOLUME.to_string(),
                boot_priority: WORK_BOOT_PRIORITY,
                read_only: false,
            },
        ],
        ..LaunchMedia::default()
    })
}

/// Run the installer on the Amiga and report which of the three endings it had.
///
/// The thin wrapper: a real WinUAE and a real clock. Everything it does beyond
/// choosing those two is in [`run_with`].
pub fn run(request: &RunRequest, sink: &dyn ProgressSink) -> CoreResult<RunOutcome> {
    let launcher = WinUaeLauncher::new(request.winuae_path);
    run_with(request, &launcher, &RealClock::new(), sink)
}

/// The run, with its emulator and its clock supplied.
///
/// Returns [`CoreError::Cancelled`] when the user stopped it — cancellation is
/// not a fourth outcome, because a run that was stopped produced no answer to
/// report.
pub fn run_with(
    request: &RunRequest,
    launcher: &dyn EmulatorLauncher,
    clock: &dyn Clock,
    sink: &dyn ProgressSink,
) -> CoreResult<RunOutcome> {
    let media = media_for(request)?;
    let config = generate_uae_config(request.profile, &media)?;
    let result_file = super::workvol::result_path(request.work_volume_dir);

    // Before the launch is the cheapest place to stop: nothing has started, so
    // there is nothing to terminate and nothing to leave behind.
    if sink.is_cancelled() {
        return Err(CoreError::Cancelled);
    }

    sink.report(0, deadline_units(request), "Starting the emulator");
    let mut session = launcher.launch(&config)?;

    // **Every** ending goes through the two lines below — an answer, the
    // deadline, a cancellation, and an I/O error mid-poll alike. That is why
    // the loop is a separate function: inside it `?` is free to propagate,
    // because the only way out of it is through `end_session` here.
    //
    // Written inline, the two `?`s in `poll_until_ending` dropped the session
    // without terminating it, and `WinUaeProcess` has no `Drop`. A transient
    // read error on the mount would then have left a WinUAE window ART opened
    // running on the owner's desktop with the handle to it gone — nothing but
    // the owner could ever close it. The owner is sitting at this machine; an
    // emulator window appearing unbidden was a real annoyance in an earlier
    // round, and one that cannot be closed is worse. Keeping the `Child`
    // rather than a pid stops ART ending a process it did not start; this is
    // the same care in the other direction, so ART always ends the one it did.
    let ending = poll_until_ending(request, session.as_mut(), clock, sink, &result_file);
    end_session(session.as_mut(), sink);
    ending
}

/// The poll loop. Everything it returns — including an error — reaches the
/// caller through [`run_with`]'s single `end_session`, so `?` is safe here in
/// a way it is not one level up.
fn poll_until_ending(
    request: &RunRequest,
    session: &mut dyn EmulatorSession,
    clock: &dyn Clock,
    sink: &dyn ProgressSink,
    result_file: &Path,
) -> CoreResult<RunOutcome> {
    let deadline = request.limits.deadline;
    loop {
        // The result is read first, every time round. An answer that landed
        // during the last sleep outranks both the deadline below it and a
        // cancellation: it is the thing the whole run was waiting for, and
        // discarding it would report "no answer" about a run that gave one.
        if let Some(outcome) = read_outcome(result_file)? {
            return Ok(outcome);
        }

        // Between whole polls, never inside one: the read above is finished
        // and the sleep below has not begun, so stopping here leaves nothing
        // half-done on either side.
        if sink.is_cancelled() {
            return Err(CoreError::Cancelled);
        }

        // The emulator going away without an answer is its own ending, and
        // deliberately not the deadline's. The deadline says "nobody answered
        // a question it asked", which tells the user to watch the window next
        // time; a window they closed themselves is not fixed by that advice,
        // and §3 of the design says these endings exist precisely because they
        // carry different advice. One final read first, in case the result
        // landed in the instant before the process went.
        if !session.is_running()? {
            if let Some(outcome) = read_outcome(result_file)? {
                return Ok(outcome);
            }
            return Ok(RunOutcome::EmulatorClosed {
                waited: clock.elapsed(),
            });
        }

        let waited = clock.elapsed();
        if waited >= deadline {
            return Ok(RunOutcome::TimedOut { waited });
        }

        sink.report(
            waited.as_secs(),
            deadline_units(request),
            "Waiting for the Amiga to report",
        );
        clock.sleep(request.limits.poll_interval);
    }
}

/// The deadline in whole seconds, as a progress total — `None` when there is
/// no deadline to measure against, so the UI shows an unbounded indicator
/// rather than a bar that is always full.
fn deadline_units(request: &RunRequest) -> Option<u64> {
    let secs = request.limits.deadline.as_secs();
    (secs > 0).then_some(secs)
}

/// End the emulator ART started.
///
/// Called from exactly one place — [`run_with`], on the single path every
/// ending takes — so a run cannot leave WinUAE on the owner's desktop with the
/// handle to it dropped. It is called even when the emulator has already gone,
/// because [`EmulatorSession::terminate`] is required to be idempotent and
/// asking a process that has exited to exit costs nothing; the alternative is
/// a branch that has to *remember* to skip it, which is how the error paths
/// came to skip it altogether.
///
/// Failing to terminate is reported and swallowed: the run has its answer
/// already, and losing it because the process was gone a moment earlier than
/// expected would be the report ART owes the user thrown away for nothing.
fn end_session(session: &mut dyn EmulatorSession, sink: &dyn ProgressSink) {
    let pid = session.pid();
    if let Err(err) = session.terminate() {
        sink.report(
            0,
            None,
            &format!("Could not stop the emulator ({pid}): {err}"),
        );
    }
}

/// Read the result file, and answer only when it says something conclusive.
///
/// `Ok(None)` means *keep polling*, and it covers three different states on
/// purpose:
///
/// - **No file.** The Amiga has not reached the marker yet.
/// - **[`MARK_STARTED`] alone.** The run began and did not finish. It is not
///   an outcome and must never be read as one — that is exactly the state a
///   package that reboots the machine leaves behind, because the script's
///   `If EXISTS` re-run guard deliberately does not overwrite the first pass's
///   answer. Treating it as a success would report an install nobody observed
///   finishing (§89).
/// - **Anything else.** Empty, partial or unrecognised. `Echo >file` truncates
///   before it writes, so a read can land between the two and see nothing; the
///   honest reading of a byte sequence ART does not recognise is that the
///   answer has not arrived, not that it failed.
fn read_outcome(result_file: &Path) -> CoreResult<Option<RunOutcome>> {
    let bytes = match std::fs::read(result_file) {
        Ok(bytes) => bytes,
        // The file not being there yet is the normal state of a run in
        // progress, not an error. Anything else — a permission problem, a
        // mount that went away — is real and is reported.
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(err) => return Err(CoreError::Io(err)),
    };

    // Lossy rather than strict: the Amiga writes this file and a stray byte in
    // it must not turn into an error about encoding when the marker it carries
    // is plain ASCII either way.
    let text = String::from_utf8_lossy(&bytes);
    let marker = text.lines().next().unwrap_or("").trim();

    // The marker is matched, not carried. See `RunOutcome`'s own
    // documentation for why neither answer has a message: echoing "failed"
    // back into a field called `message` tells a reader nothing the variant
    // did not already say.
    Ok(match marker {
        MARK_OK => Some(RunOutcome::Succeeded),
        MARK_FAILED => Some(RunOutcome::Failed),
        MARK_STARTED => None,
        _ => None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::amigainstall::RESULT_FILE;
    use crate::core::jobs::{CancelToken, NoProgress};
    use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
    use std::sync::{Arc, Mutex};

    /// ART-184: the directory removes itself on `Drop`, so a panicking test
    /// cleans up too. The previous shape returned a bare `PathBuf` with no
    /// guard at all, and was measured leaking eighteen directories per run of
    /// this module alone — in code written *after* ART-184 was filed.
    fn scratch(tag: &str) -> crate::core::ScratchDir {
        crate::core::ScratchDir::new("art-amigainstall-run", tag)
    }

    /// A clock that never waits.
    ///
    /// `sleep` advances a counter instead of a thread, so the deadline tests
    /// below assert what the loop *does* rather than what one machine's
    /// scheduler happened to do. `on_sleep` is how a test plays the Amiga:
    /// it writes the result file between two polls, which is precisely the
    /// live-write the design measured.
    struct TestClock {
        elapsed: Mutex<Duration>,
        sleeps: AtomicU32,
        on_sleep: Box<dyn Fn(u32) + Send + Sync>,
    }

    impl TestClock {
        fn new(on_sleep: impl Fn(u32) + Send + Sync + 'static) -> Self {
            Self {
                elapsed: Mutex::new(Duration::ZERO),
                sleeps: AtomicU32::new(0),
                on_sleep: Box::new(on_sleep),
            }
        }

        fn idle() -> Self {
            Self::new(|_| {})
        }

        fn sleeps(&self) -> u32 {
            self.sleeps.load(Ordering::Relaxed)
        }
    }

    impl Clock for TestClock {
        fn elapsed(&self) -> Duration {
            *self.elapsed.lock().unwrap()
        }

        fn sleep(&self, interval: Duration) {
            *self.elapsed.lock().unwrap() += interval;
            let n = self.sleeps.fetch_add(1, Ordering::Relaxed) + 1;
            (self.on_sleep)(n);
        }
    }

    /// What the fake emulator did, readable after the run.
    struct SessionLog {
        running: AtomicBool,
        terminated: Mutex<Vec<u32>>,
        launched_with: Mutex<Vec<String>>,
        liveness_checks: AtomicU32,
        /// When set, `is_running` returns `Err` instead of an answer — the
        /// transient I/O failure that used to orphan the emulator.
        liveness_fails: AtomicBool,
        /// Run at the start of each liveness check, given the number of checks
        /// so far.
        ///
        /// This hook exists because of a mutation that survived: a test meant
        /// to prove the *final* read after the emulator exits was in fact
        /// satisfied by the read at the top of the next iteration, so deleting
        /// the final read did not fail it. To reach that branch the answer has
        /// to arrive **between** the top-of-loop read and the liveness check,
        /// and this is the only place a test can put it.
        on_liveness: Box<dyn Fn(u32) + Send + Sync>,
    }

    impl SessionLog {
        fn new(on_liveness: impl Fn(u32) + Send + Sync + 'static) -> Self {
            Self {
                running: AtomicBool::new(true),
                terminated: Mutex::new(Vec::new()),
                launched_with: Mutex::new(Vec::new()),
                liveness_checks: AtomicU32::new(0),
                liveness_fails: AtomicBool::new(false),
                on_liveness: Box::new(on_liveness),
            }
        }
    }

    struct FakeSession {
        pid: u32,
        log: Arc<SessionLog>,
    }

    impl EmulatorSession for FakeSession {
        fn pid(&self) -> u32 {
            self.pid
        }

        fn is_running(&mut self) -> CoreResult<bool> {
            let n = self.log.liveness_checks.fetch_add(1, Ordering::Relaxed) + 1;
            // Before the flag is read, so a hook that clears it is answered on
            // this very check rather than the next one.
            (self.log.on_liveness)(n);
            if self.log.liveness_fails.load(Ordering::Relaxed) {
                return Err(CoreError::Io(std::io::Error::other("the handle went away")));
            }
            Ok(self.log.running.load(Ordering::Relaxed))
        }

        fn terminate(&mut self) -> CoreResult<()> {
            self.log.terminated.lock().unwrap().push(self.pid);
            self.log.running.store(false, Ordering::Relaxed);
            Ok(())
        }
    }

    /// A launcher that starts nothing. No emulator window ever opens for a
    /// test in this file, which is a requirement of this round and not a
    /// convenience: the owner is sitting at the machine.
    struct FakeLauncher {
        pid: u32,
        log: Arc<SessionLog>,
    }

    impl FakeLauncher {
        fn new() -> Self {
            Self::with_liveness_hook(|_| {})
        }

        fn with_liveness_hook(on_liveness: impl Fn(u32) + Send + Sync + 'static) -> Self {
            Self {
                pid: 4242,
                log: Arc::new(SessionLog::new(on_liveness)),
            }
        }
    }

    impl EmulatorLauncher for FakeLauncher {
        fn launch(&self, config_text: &str) -> CoreResult<Box<dyn EmulatorSession>> {
            self.log
                .launched_with
                .lock()
                .unwrap()
                .push(config_text.to_string());
            Ok(Box::new(FakeSession {
                pid: self.pid,
                log: Arc::clone(&self.log),
            }))
        }
    }

    /// A sink that stops the run after `after` cancellation checks.
    struct CancelAfter {
        checks: AtomicU32,
        after: u32,
        token: CancelToken,
    }

    impl CancelAfter {
        fn new(after: u32) -> Self {
            Self {
                checks: AtomicU32::new(0),
                after,
                token: CancelToken::new(),
            }
        }
    }

    impl ProgressSink for CancelAfter {
        fn report(&self, _done: u64, _total: Option<u64>, _message: &str) {}

        fn is_cancelled(&self) -> bool {
            let n = self.checks.fetch_add(1, Ordering::Relaxed) + 1;
            if n > self.after {
                self.token.cancel();
            }
            self.token.is_cancelled()
        }
    }

    /// A whole run's worth of directories and files, with nothing running.
    struct Fixture {
        /// Owns the scratch guard rather than a bare path, so the directory
        /// goes when the fixture does — including when a test panics between
        /// here and its last line (ART-184).
        root: crate::core::ScratchDir,
        plan: PlannedRun,
        profile: AmigaProfile,
    }

    impl Fixture {
        fn new(tag: &str) -> Self {
            let root = scratch(tag);
            std::fs::create_dir_all(root.join("work").join("S")).unwrap();
            std::fs::write(
                root.join("work").join("S").join("Startup-Sequence"),
                b"; art",
            )
            .unwrap();
            std::fs::create_dir_all(root.join("tree")).unwrap();
            // The package's own wrapper, unpacked — the third mount, and the
            // one whose absence was ART-185. It carries the drawer and the
            // program the plan below names, because an empty directory would
            // mount just as well and produce exactly the silent failure.
            std::fs::create_dir_all(root.join("pkg").join("BoingBag3.9-2").join("C")).unwrap();
            std::fs::write(
                root.join("pkg")
                    .join("BoingBag3.9-2")
                    .join("C")
                    .join("Updater"),
                b"the updater",
            )
            .unwrap();
            std::fs::write(root.join("kick.rom"), b"rom").unwrap();
            std::fs::write(root.join("winuae.exe"), b"exe").unwrap();
            Self {
                root,
                plan: PlannedRun {
                    package_id: "boingbag-39-2".to_string(),
                    system_volume: "DH0".to_string(),
                    program: format!("{PACKAGE_DEVICE}:BoingBag3.9-2/C/Updater"),
                    args: Vec::new(),
                    working_directory: Some(format!("{PACKAGE_DEVICE}:BoingBag3.9-2")),
                },
                profile: AmigaProfile::a1200_aga(),
            }
        }

        fn work(&self) -> PathBuf {
            self.root.join("work")
        }

        fn package(&self) -> PathBuf {
            self.root.join("pkg")
        }

        fn result_file(&self) -> PathBuf {
            self.work().join(RESULT_FILE)
        }

        fn request(&self) -> RunRequest<'_> {
            RunRequest {
                plan: &self.plan,
                work_volume_dir: Path::new("placeholder"),
                tree_dir: Path::new("placeholder"),
                package_volume_dir: Path::new("placeholder"),
                profile: &self.profile,
                kickstart_path: Path::new("placeholder"),
                winuae_path: Path::new("placeholder"),
                limits: RunLimits {
                    deadline: Duration::from_secs(60),
                    poll_interval: Duration::from_secs(2),
                },
            }
        }
    }

    /// Borrowing rules mean the paths have to outlive the request, so every
    /// test builds it through here rather than inline.
    macro_rules! request {
        ($fx:expr) => {{
            let work = $fx.work();
            let tree = $fx.root.join("tree");
            let pkg = $fx.package();
            let kick = $fx.root.join("kick.rom");
            let exe = $fx.root.join("winuae.exe");
            (work, tree, pkg, kick, exe)
        }};
    }

    fn with_paths<'a>(
        base: RunRequest<'a>,
        work: &'a Path,
        tree: &'a Path,
        pkg: &'a Path,
        kick: &'a Path,
        exe: &'a Path,
    ) -> RunRequest<'a> {
        RunRequest {
            work_volume_dir: work,
            tree_dir: tree,
            package_volume_dir: pkg,
            kickstart_path: kick,
            winuae_path: exe,
            ..base
        }
    }

    /// The happy path: a result file appears **while the run is polling**, and
    /// the run reports what it said.
    ///
    /// The file is written from inside `sleep`, not before the call, because
    /// writing it up front would let a `run` that never polled at all pass —
    /// and "it read the file once before doing anything" is not the behaviour
    /// the design measured.
    #[test]
    fn a_result_written_while_running_is_read_and_reported() {
        let fx = Fixture::new("happy");
        let (work, tree, pkg, kick, exe) = request!(fx);
        let request = with_paths(fx.request(), &work, &tree, &pkg, &kick, &exe);

        let result_file = fx.result_file();
        let clock = TestClock::new(move |n| {
            if n == 3 {
                std::fs::write(&result_file, b"ok\n").unwrap();
            }
        });
        let launcher = FakeLauncher::new();

        let outcome = run_with(&request, &launcher, &clock, &NoProgress).unwrap();

        assert_eq!(outcome, RunOutcome::Succeeded);
        assert_eq!(clock.sleeps(), 3, "it should have polled, not read once");
        assert_eq!(
            *launcher.log.terminated.lock().unwrap(),
            vec![launcher.pid],
            "ART ends the process it started, by its own handle"
        );
    }

    /// The deadline produces a third outcome, not a failure.
    ///
    /// Nothing here waits: the clock is fake, so this asserts the invariant
    /// ("a run with no answer ends at the deadline, as a timeout") rather than
    /// hoping a real sleep lands on the right side of a comparison.
    #[test]
    fn a_run_that_never_reports_times_out_rather_than_failing() {
        let fx = Fixture::new("deadline");
        let (work, tree, pkg, kick, exe) = request!(fx);
        let request = with_paths(fx.request(), &work, &tree, &pkg, &kick, &exe);

        let clock = TestClock::idle();
        let launcher = FakeLauncher::new();

        let outcome = run_with(&request, &launcher, &clock, &NoProgress).unwrap();

        match outcome {
            RunOutcome::TimedOut { waited } => {
                assert!(
                    waited >= request.limits.deadline,
                    "a timeout must have actually reached the deadline, waited {waited:?}"
                );
            }
            other => panic!("a run nobody answered must time out, not report {other:?}"),
        }
        assert_eq!(
            *launcher.log.terminated.lock().unwrap(),
            vec![launcher.pid],
            "the deadline terminates the emulator ART started"
        );
        assert!(
            !fx.result_file().exists(),
            "the run must not invent a result of its own"
        );
    }

    /// "started" alone is not an outcome — it means the run began and did not
    /// finish, which is a timeout, not a success.
    ///
    /// This is also the reboot case: the script's `If EXISTS` guard leaves the
    /// first pass's `started` in place on a second boot, so a poller that read
    /// it as an answer would report an install nobody watched finish.
    #[test]
    fn a_started_marker_alone_is_not_treated_as_an_outcome() {
        let fx = Fixture::new("started");
        let (work, tree, pkg, kick, exe) = request!(fx);
        let request = with_paths(fx.request(), &work, &tree, &pkg, &kick, &exe);

        std::fs::write(fx.result_file(), format!("{MARK_STARTED}\n")).unwrap();

        let clock = TestClock::idle();
        let launcher = FakeLauncher::new();

        let outcome = run_with(&request, &launcher, &clock, &NoProgress).unwrap();

        assert!(
            matches!(outcome, RunOutcome::TimedOut { .. }),
            "a started marker is not an outcome, got {outcome:?}"
        );
        assert_eq!(
            std::fs::read_to_string(fx.result_file()).unwrap().trim(),
            MARK_STARTED,
            "the Amiga's own answer must be left exactly as it was"
        );
    }

    /// A run that began, was answered, and only *then* rebooted still reports
    /// the answer: `started` becoming `ok` is the ordinary happy path, and the
    /// re-run guard only protects what is already there.
    #[test]
    fn a_started_marker_that_becomes_ok_is_reported_as_success() {
        let fx = Fixture::new("started-then-ok");
        let (work, tree, pkg, kick, exe) = request!(fx);
        let request = with_paths(fx.request(), &work, &tree, &pkg, &kick, &exe);

        std::fs::write(fx.result_file(), format!("{MARK_STARTED}\n")).unwrap();

        let result_file = fx.result_file();
        let clock = TestClock::new(move |n| {
            if n == 2 {
                std::fs::write(&result_file, format!("{MARK_OK}\n")).unwrap();
            }
        });

        let outcome = run_with(&request, &FakeLauncher::new(), &clock, &NoProgress).unwrap();

        assert_eq!(outcome, RunOutcome::Succeeded);
    }

    /// Cancellation between polls stops the run and leaves nothing behind.
    #[test]
    fn cancelling_between_polls_terminates_and_reports_cancelled() {
        let fx = Fixture::new("cancel");
        let (work, tree, pkg, kick, exe) = request!(fx);
        let request = with_paths(fx.request(), &work, &tree, &pkg, &kick, &exe);

        let clock = TestClock::idle();
        let launcher = FakeLauncher::new();
        // The first check is the one before the launch; cancelling after two
        // means the run is stopped inside the loop, between two polls.
        let sink = CancelAfter::new(2);

        let err = run_with(&request, &launcher, &clock, &sink).unwrap_err();

        assert!(
            matches!(err, CoreError::Cancelled),
            "cancellation is Cancelled, not a failure: {err:?}"
        );
        assert_eq!(
            *launcher.log.terminated.lock().unwrap(),
            vec![launcher.pid],
            "a cancelled run must not leave the emulator running"
        );
        assert!(
            clock.elapsed() < request.limits.deadline,
            "it must stop when asked, not at the deadline"
        );
        assert!(
            !fx.result_file().exists(),
            "a cancelled run leaves nothing behind"
        );
    }

    /// Cancelling before the launch starts nothing at all.
    #[test]
    fn cancelling_before_the_launch_starts_no_emulator() {
        let fx = Fixture::new("cancel-early");
        let (work, tree, pkg, kick, exe) = request!(fx);
        let request = with_paths(fx.request(), &work, &tree, &pkg, &kick, &exe);

        let launcher = FakeLauncher::new();
        let sink = CancelAfter::new(0);

        let err = run_with(&request, &launcher, &TestClock::idle(), &sink).unwrap_err();

        assert!(matches!(err, CoreError::Cancelled));
        assert!(
            launcher.log.launched_with.lock().unwrap().is_empty(),
            "nothing should have been launched"
        );
    }

    /// A failure is the installer saying no, and must not wear a timeout's
    /// clothes — or the other way round.
    #[test]
    fn a_failed_marker_is_reported_as_failed_and_not_as_a_timeout() {
        let fx = Fixture::new("failed");
        let (work, tree, pkg, kick, exe) = request!(fx);
        let request = with_paths(fx.request(), &work, &tree, &pkg, &kick, &exe);

        let result_file = fx.result_file();
        let clock = TestClock::new(move |n| {
            if n == 1 {
                std::fs::write(&result_file, format!("{MARK_FAILED}\n")).unwrap();
            }
        });

        let outcome = run_with(&request, &FakeLauncher::new(), &clock, &NoProgress).unwrap();

        assert_eq!(outcome, RunOutcome::Failed);
    }

    /// An answer that lands during the very last sleep still wins: the loop
    /// reads before it checks the deadline, so a run is never reported as
    /// unanswered when it was answered.
    #[test]
    fn an_answer_on_the_last_poll_beats_the_deadline() {
        let fx = Fixture::new("last-poll");
        let (work, tree, pkg, kick, exe) = request!(fx);
        let mut request = with_paths(fx.request(), &work, &tree, &pkg, &kick, &exe);
        request.limits = RunLimits {
            deadline: Duration::from_secs(4),
            poll_interval: Duration::from_secs(2),
        };

        let result_file = fx.result_file();
        // Two sleeps take the fake clock to exactly the deadline. The answer
        // is written on that second one, so the next thing the loop does is
        // choose between the answer and the timeout.
        let clock = TestClock::new(move |n| {
            if n == 2 {
                std::fs::write(&result_file, format!("{MARK_OK}\n")).unwrap();
            }
        });

        let outcome = run_with(&request, &FakeLauncher::new(), &clock, &NoProgress).unwrap();

        assert_eq!(clock.elapsed(), request.limits.deadline);
        assert_eq!(outcome, RunOutcome::Succeeded);
    }

    /// Bytes ART does not recognise are not an answer. `Echo >file` truncates
    /// before it writes, so a poll can land on an empty or half-written file.
    #[test]
    fn an_unrecognised_result_is_not_an_answer() {
        let fx = Fixture::new("partial");
        let (work, tree, pkg, kick, exe) = request!(fx);
        let request = with_paths(fx.request(), &work, &tree, &pkg, &kick, &exe);

        std::fs::write(fx.result_file(), b"").unwrap();
        let outcome = run_with(
            &request,
            &FakeLauncher::new(),
            &TestClock::idle(),
            &NoProgress,
        );
        assert!(matches!(outcome, Ok(RunOutcome::TimedOut { .. })));

        std::fs::write(fx.result_file(), b"o").unwrap();
        let outcome = run_with(
            &request,
            &FakeLauncher::new(),
            &TestClock::idle(),
            &NoProgress,
        );
        assert!(matches!(outcome, Ok(RunOutcome::TimedOut { .. })));
    }

    /// The owner closing the emulator window is its own ending: not a
    /// success, not a failure, and **not a timeout**. A timeout tells the user
    /// to watch the window and answer next time, which is the wrong advice for
    /// a window they closed themselves (design §3).
    #[test]
    fn an_emulator_closed_without_reporting_is_its_own_ending() {
        let fx = Fixture::new("exited");
        let (work, tree, pkg, kick, exe) = request!(fx);
        let request = with_paths(fx.request(), &work, &tree, &pkg, &kick, &exe);

        let launcher = FakeLauncher::new();
        let log = Arc::clone(&launcher.log);
        let clock = TestClock::new(move |n| {
            if n == 2 {
                log.running.store(false, Ordering::Relaxed);
            }
        });

        let outcome = run_with(&request, &launcher, &clock, &NoProgress).unwrap();

        match outcome {
            RunOutcome::EmulatorClosed { waited } => {
                assert!(
                    waited < request.limits.deadline,
                    "it should stop when the emulator goes, not sit out the deadline"
                );
            }
            other => panic!("a closed emulator is its own ending, got {other:?}"),
        }
    }

    /// An answer written just before the emulator exited is still the answer.
    ///
    /// The answer lands **at the liveness check**, after this iteration's read
    /// of the result file and before the next one — the only window in which
    /// the final read is the thing that finds it. Written the obvious way (the
    /// clock writes it during a sleep) this test passed with the final read
    /// deleted, because the top of the next iteration read it anyway: a test
    /// that agreed with the defect it was written to catch.
    #[test]
    fn a_result_written_just_before_the_emulator_exits_is_still_read() {
        let fx = Fixture::new("exit-race");
        let (work, tree, pkg, kick, exe) = request!(fx);
        let request = with_paths(fx.request(), &work, &tree, &pkg, &kick, &exe);

        let result_file = fx.result_file();
        let launcher = FakeLauncher::with_liveness_hook(move |n| {
            if n == 2 {
                std::fs::write(&result_file, format!("{MARK_OK}\n")).unwrap();
            }
        });
        // The emulator goes away during the first sleep, so the check that
        // notices it gone is the same one that finds the answer — which is
        // what "wrote its result and exited" looks like from the host.
        let log = Arc::clone(&launcher.log);
        let clock = TestClock::new(move |n| {
            if n == 1 {
                log.running.store(false, Ordering::Relaxed);
            }
        });

        let outcome = run_with(&request, &launcher, &clock, &NoProgress).unwrap();

        assert_eq!(outcome, RunOutcome::Succeeded);
    }

    /// ART's own volume boots; the user's tree is mounted as data and cannot.
    #[test]
    fn arts_own_volume_boots_and_the_tree_is_data() {
        let fx = Fixture::new("mounts");
        let (work, tree, pkg, kick, exe) = request!(fx);
        let request = with_paths(fx.request(), &work, &tree, &pkg, &kick, &exe);

        let media = media_for(&request).unwrap();

        let art = media
            .directories
            .iter()
            .find(|d| d.label == WORK_VOLUME)
            .expect("ART's own volume");
        let user = media
            .directories
            .iter()
            .find(|d| d.volume == fx.plan.system_volume)
            .expect("the tree");

        assert!(
            art.boot_priority > user.boot_priority,
            "ART's volume must outrank the tree"
        );
        // `i8::MIN` and **not** `TREE_BOOT_PRIORITY` (last round's review):
        // `user` *is* the tree, so comparing its priority to the constant
        // `media_for` assigned it reads that constant back to itself and would
        // pass whatever value it were given. What is actually claimed is that
        // nothing can be mounted below the tree, which is what makes ART's own
        // volume the boot device however many volumes the run ends up with —
        // and that claim is about the number, so the number is written here.
        // Line 1296's `TREE_BOOT_PRIORITY` is a different claim and stays: the
        // package volume is asserted to be *in the tree's class*, not to be a
        // particular number.
        assert_eq!(
            user.boot_priority,
            i8::MIN,
            "the tree is mounted as data and must sit at the very bottom"
        );
        assert_eq!(art.host_path, work.to_string_lossy());
        assert_eq!(user.host_path, tree.to_string_lossy());
        assert_eq!(
            media.kickstart_path.as_deref(),
            Some(&*kick.to_string_lossy())
        );
    }

    /// ART-185: **three** volumes, and the package's own is one of them.
    ///
    /// Written so that it cannot pass against the defect. A test that found
    /// the tree and ART's volume and stopped there is satisfied by two mounts
    /// — which is exactly what `media_for` produced for four tasks — so this
    /// asserts the *count*, three distinct device names, and the package
    /// mount's own host path. Delete the third `DirMount` and the first
    /// assertion fails; give it the same device name as either of the others
    /// and the second does.
    #[test]
    fn the_package_is_mounted_as_its_own_volume_beside_the_other_two() {
        let fx = Fixture::new("three-mounts");
        let (work, tree, pkg, kick, exe) = request!(fx);
        let request = with_paths(fx.request(), &work, &tree, &pkg, &kick, &exe);

        let media = media_for(&request).unwrap();

        assert_eq!(
            media.directories.len(),
            3,
            "the tree, the package and ART's own volume: {:?}",
            media
                .directories
                .iter()
                .map(|d| d.volume.as_str())
                .collect::<Vec<_>>()
        );
        let mut devices: Vec<String> = media
            .directories
            .iter()
            .map(|d| d.volume.to_ascii_lowercase())
            .collect();
        devices.sort();
        devices.dedup();
        assert_eq!(
            devices.len(),
            3,
            "no two mounts may share a device name; the second would shadow the first"
        );

        let package = media
            .directories
            .iter()
            .find(|d| d.volume == PACKAGE_DEVICE)
            .expect("the package's own volume");
        assert_eq!(
            package.host_path,
            pkg.to_string_lossy(),
            "and it must be the directory the wrapper was unpacked into"
        );
        assert_eq!(package.label, PACKAGE_VOLUME);

        // Against ART's own volume, not against `PACKAGE_BOOT_PRIORITY` — a
        // constant compared with itself is a tautology, and raising it to 127
        // survived exactly that assertion in the mutation run. What matters is
        // the *relationship*: ART's script must boot, and a wrapper that
        // outranked it would hand the machine to whatever the package's own
        // drawer happens to look like.
        let art = media
            .directories
            .iter()
            .find(|d| d.volume == WORK_DEVICE)
            .expect("ART's own volume");
        assert!(
            package.boot_priority < art.boot_priority,
            "the package must never outrank ART's own volume: {} vs {}",
            package.boot_priority,
            art.boot_priority
        );
        assert_eq!(
            package.boot_priority, TREE_BOOT_PRIORITY,
            "data, exactly like the tree"
        );
        assert!(
            !package.read_only,
            "writable: nothing of the user's is here, and a read-only mount would fail an \
             installer that writes beside itself for a reason ART invented"
        );
    }

    /// The program the script names is really on the volume that was mounted.
    ///
    /// The mount is only half of ART-185; the other half is that the drawer
    /// and the installer actually arrived in it. This asserts the join between
    /// the two ends — what `media_for` mounted, and what
    /// `PlannedRun::working_directory` and `program` say — against the real
    /// filesystem, which is what the emulator will see.
    #[test]
    fn the_program_the_plan_names_is_on_the_volume_that_was_mounted() {
        let fx = Fixture::new("program-present");
        let (work, tree, pkg, kick, exe) = request!(fx);
        let request = with_paths(fx.request(), &work, &tree, &pkg, &kick, &exe);

        let media = media_for(&request).unwrap();
        let package = media
            .directories
            .iter()
            .find(|d| d.volume == PACKAGE_DEVICE)
            .expect("the package's own volume");

        // `ARTPkg:BoingBag3.9-2/C/Updater` → the host path below the mount.
        let tail = fx
            .plan
            .program
            .strip_prefix(&format!("{PACKAGE_DEVICE}:"))
            .expect("the plan names the package volume");
        let on_host = Path::new(&package.host_path).join(tail.replace('/', "\\"));
        assert!(
            on_host.is_file(),
            "the installer the script names must exist under the mount: {}",
            on_host.display()
        );
    }

    /// A run whose package was never unpacked is refused **before** the
    /// emulator starts.
    ///
    /// This is ART-185's own shape: an absent or empty directory mounts
    /// perfectly well, `CD` then fails, the shell finds no program, and the
    /// script's `If Warn` reports that the installer said no about a program
    /// that never started. Twenty minutes and a wrong sentence, or one
    /// `is_dir`.
    #[test]
    fn a_run_whose_package_was_not_unpacked_is_refused_before_launching() {
        let fx = Fixture::new("no-package");
        let (work, tree, _pkg, kick, exe) = request!(fx);
        let missing = fx.root.join("never-unpacked");
        let request = with_paths(fx.request(), &work, &tree, &missing, &kick, &exe);

        let launcher = FakeLauncher::new();
        let err = run_with(&request, &launcher, &TestClock::idle(), &NoProgress).unwrap_err();

        assert!(
            matches!(err, CoreError::InvalidInput(ref m) if m.contains("unpacked")),
            "got {err:?}"
        );
        assert!(
            launcher.log.launched_with.lock().unwrap().is_empty(),
            "nothing should have been launched"
        );
    }

    /// The tree may not take the package's device name: the second mount
    /// would shadow the first, and the shadowed one carries the installer.
    #[test]
    fn a_tree_mounted_under_the_package_device_name_is_refused() {
        for hostile in [PACKAGE_DEVICE, "artpkg", "ARTPkg:"] {
            let mut fx = Fixture::new("shadow-package");
            fx.plan.system_volume = hostile.to_string();
            let (work, tree, pkg, kick, exe) = request!(fx);
            let request = with_paths(fx.request(), &work, &tree, &pkg, &kick, &exe);

            let err = media_for(&request).unwrap_err();
            assert!(
                matches!(err, CoreError::SafetyRefused(ref m) if m.contains(PACKAGE_DEVICE)),
                "'{hostile}' must be refused, got {err:?}"
            );
        }
    }

    /// ART's two own volumes must not be one volume. Both are constants, so
    /// this is cheap — and a constant changed to collide with the other would
    /// otherwise show up only as a run whose result file was never written.
    #[test]
    fn arts_two_own_volumes_have_different_names() {
        assert_ne!(WORK_VOLUME, PACKAGE_VOLUME);
        assert!(!claims_package_volume(WORK_VOLUME));
        assert!(!claims_work_volume(PACKAGE_VOLUME));
    }

    /// ART ships no Kickstart, so a run without the user's own is refused
    /// rather than quietly booting AROS.
    #[test]
    fn a_run_without_a_licensed_kickstart_is_refused() {
        let fx = Fixture::new("no-kick");
        let (work, tree, pkg, _kick, exe) = request!(fx);
        let missing = fx.root.join("nothing-here.rom");
        let request = with_paths(fx.request(), &work, &tree, &pkg, &missing, &exe);

        let err = run_with(
            &request,
            &FakeLauncher::new(),
            &TestClock::idle(),
            &NoProgress,
        )
        .unwrap_err();
        assert!(
            matches!(err, CoreError::InvalidInput(ref m) if m.contains("Kickstart")),
            "got {err:?}"
        );
    }

    /// A work volume with no script can only ever end in a deadline, so it is
    /// refused before the emulator is started rather than after twenty minutes.
    #[test]
    fn a_work_volume_with_no_script_is_refused() {
        let fx = Fixture::new("no-script");
        let (work, tree, pkg, kick, exe) = request!(fx);
        std::fs::remove_file(work.join("S").join("Startup-Sequence")).unwrap();
        let request = with_paths(fx.request(), &work, &tree, &pkg, &kick, &exe);

        let launcher = FakeLauncher::new();
        let err = run_with(&request, &launcher, &TestClock::idle(), &NoProgress).unwrap_err();
        assert!(
            matches!(err, CoreError::InvalidInput(ref m) if m.contains("Startup-Sequence")),
            "got {err:?}"
        );
        assert!(
            launcher.log.launched_with.lock().unwrap().is_empty(),
            "nothing should have been launched"
        );
    }

    /// The tree may not be mounted under ART's own device name: the second
    /// mount would shadow the first, and the run would poll for a result file
    /// in a directory nothing writes one into.
    #[test]
    fn a_tree_mounted_under_arts_own_device_name_is_refused() {
        let mut fx = Fixture::new("shadow");
        // Lower case on purpose: AmigaDOS device names are case-insensitive,
        // so a comparison that is not would let this through.
        fx.plan.system_volume = "artwork".to_string();
        let (work, tree, pkg, kick, exe) = request!(fx);
        let request = with_paths(fx.request(), &work, &tree, &pkg, &kick, &exe);

        let err = media_for(&request).unwrap_err();
        assert!(
            matches!(err, CoreError::SafetyRefused(ref m) if m.contains(WORK_DEVICE)),
            "got {err:?}"
        );
    }

    /// An I/O error reading the result file must not orphan the emulator.
    ///
    /// This is the hole eighteen mutations did not find, because no test drove
    /// an error path at all: both `?`s in the loop used to drop the session
    /// without terminating it, and `WinUaeProcess` has no `Drop` — so a
    /// transient read failure left a WinUAE window ART opened running with the
    /// handle to it gone.
    ///
    /// The error is induced by making the result *file* a directory, which is
    /// what a `read` refuses with something other than `NotFound`. Any
    /// non-`NotFound` error would do; this one needs no permissions games and
    /// behaves the same on every host.
    #[test]
    fn an_io_error_reading_the_result_still_ends_the_emulator() {
        let fx = Fixture::new("read-error");
        let (work, tree, pkg, kick, exe) = request!(fx);
        let request = with_paths(fx.request(), &work, &tree, &pkg, &kick, &exe);

        std::fs::create_dir(fx.result_file()).unwrap();

        let launcher = FakeLauncher::new();
        let err = run_with(&request, &launcher, &TestClock::idle(), &NoProgress).unwrap_err();

        assert!(
            matches!(err, CoreError::Io(_)),
            "the error itself must still reach the caller: {err:?}"
        );
        assert_eq!(
            *launcher.log.terminated.lock().unwrap(),
            vec![launcher.pid],
            "an error is an ending too; it must not leave the emulator running"
        );
    }

    /// The same, for the other `?` in the loop: the liveness check failing.
    #[test]
    fn an_io_error_checking_the_emulator_still_ends_it() {
        let fx = Fixture::new("liveness-error");
        let (work, tree, pkg, kick, exe) = request!(fx);
        let request = with_paths(fx.request(), &work, &tree, &pkg, &kick, &exe);

        let launcher = FakeLauncher::new();
        launcher.log.liveness_fails.store(true, Ordering::Relaxed);

        let err = run_with(&request, &launcher, &TestClock::idle(), &NoProgress).unwrap_err();

        assert!(matches!(err, CoreError::Io(_)), "got {err:?}");
        assert_eq!(
            *launcher.log.terminated.lock().unwrap(),
            vec![launcher.pid],
            "an error is an ending too; it must not leave the emulator running"
        );
    }

    /// The trailing-colon form of ART's own volume name is refused too.
    ///
    /// `workvol::build` has always refused `ARTWork:`; the mount planner's own
    /// second copy of that rule did not, so the one name that could produce a
    /// shadowing mount was the one name it let through.
    #[test]
    fn the_trailing_colon_form_of_arts_own_volume_is_refused_as_a_mount() {
        let mut fx = Fixture::new("shadow-colon");
        fx.plan.system_volume = "ARTWork:".to_string();
        let (work, tree, pkg, kick, exe) = request!(fx);
        let request = with_paths(fx.request(), &work, &tree, &pkg, &kick, &exe);

        let err = media_for(&request).unwrap_err();
        assert!(
            matches!(err, CoreError::SafetyRefused(ref m) if m.contains(WORK_DEVICE)),
            "got {err:?}"
        );
    }

    /// The default deadline is the provisional one, and it travels as data.
    ///
    /// It also has to stay clear of what a real package was actually seen to
    /// take. The owner's own `Updater` 45.15 ran for **422 seconds** on
    /// 2026-08-21 and had not finished; a deadline anywhere near that would
    /// report a run that is merely slow as one nobody answered, which is the
    /// wrong sentence and the wrong next step (§89). The number is named here
    /// so that lowering the constant back towards it fails a test rather than
    /// a user's install.
    #[test]
    fn the_deadline_comes_from_data_and_defaults_to_the_provisional_value() {
        const OBSERVED_UNFINISHED_RUN: Duration = Duration::from_secs(422);

        let limits = RunLimits::default();
        assert_eq!(limits.deadline, PROVISIONAL_DEADLINE);
        assert_eq!(limits.poll_interval, POLL_INTERVAL);
        assert!(
            limits.deadline > OBSERVED_UNFINISHED_RUN * 4,
            "a real package was still running at {OBSERVED_UNFINISHED_RUN:?}; a deadline of \
             {:?} leaves too little room above it",
            limits.deadline
        );
    }
}
