//! ART's own boot volume — the whole Amiga-side contract, in one script.
//!
//! This volume is not the user's tree. It carries exactly one file, a
//! `Startup-Sequence` ART wrote, and it is mounted at the highest boot
//! priority so AmigaDOS boots *it* rather than the tree
//! ([`crate::core::winuae::DirMount`] documents that convention: a higher
//! `BootPri` boots first). The tree is mounted alongside as data. So the
//! user's own `Startup-Sequence` is never read, never edited, and never
//! appended to.
//!
//! Everything this module promises is testable **without an emulator**: it
//! writes a directory, and the tests assert what is in it.

use std::path::{Path, PathBuf};

use super::{
    claims_package_volume, claims_work_volume, PlannedRun, INVOKED_FILE, MARK_FAILED, MARK_INVOKED,
    MARK_OK, MARK_STARTED, PACKAGE_VOLUME, RESULT_FILE, WORK_VOLUME,
};
use crate::core::error::{CoreError, CoreResult};
use crate::core::safety::atomic_write;
use crate::core::security::refuse_shell_metacharacters;

/// The fail level the generated script sets before it runs anything.
///
/// **Measured, not conventional — ART-188.** See
/// [`startup_sequence`]'s own documentation for the run this came out of: the
/// owner's `Updater` 45.15 returned **900**, the shipped `FailAt 21` aborted
/// the script at that line, and a failure the Amiga had already reported was
/// on its way to being told to the user as a timeout.
///
/// A `LONG` well above anything a return code can carry in practice, so the
/// question is settled by range rather than by a guess about which numbers a
/// program ART did not write will choose.
const FAIL_AT: i64 = 2_000_000_000;

/// Where the Amiga will write its result, seen from the host.
///
/// Task-critical for the poller and trivial here, which is the point: the
/// host and the script must agree on one path, so only one place computes it.
pub fn result_path(work_volume_dir: &Path) -> PathBuf {
    work_volume_dir.join(RESULT_FILE)
}

/// Build the `Startup-Sequence` that runs one installer and reports what
/// happened.
///
/// ## Why the started marker is written before the installer is invoked
///
/// The host cannot see inside the emulator. All it has is this file, so the
/// file has to distinguish the three things that otherwise look identical
/// from outside: a run that **never began** (the volume did not boot, or the
/// shell could not read the script), a run that **began and hung** (an
/// installer sitting on a requester nobody answered), and a run that
/// **finished**. Writing `started` first splits the first from the second:
/// no file at all means nothing ever ran, a file saying only `started` means
/// the Amiga got this far and then stopped. Those two are fixed by different
/// things — the first by looking at the mount and the config, the second by
/// watching the emulator window and answering the question — so ART must not
/// report them as the same event.
///
/// It is written before every `Assign` invoked *by name* as well as before
/// the installer, and that ordering is not cosmetic: **ART-118 was exactly a
/// line that could not run** — AmigaDOS auto-assigns `C:` only when a `C`
/// drawer exists on the boot volume, ART's own volume has none, and `Assign`
/// itself lives in `C:`, so the script's first `Assign` failed and dropped
/// the user at a CLI. Every command that resolves through `C:` is therefore
/// something that might not return, and the rule the research measured —
/// *write the result before anything that might not return* — puts the marker
/// above all of them.
///
/// ## Why the fully-qualified `Assign` is the very first command
///
/// Exactly one line sits above the marker, and it is the one line that needs
/// nothing assigned in order to run: `{sys}:C/Assign C: {sys}:C`, which names
/// the executable by an explicit path on the mounted tree.
///
/// It is first because the alternative rests on an assumption this project is
/// not entitled to make. `FailAt`, `If`/`Else`/`EndIf` and `Echo` are
/// **Shell-internal** commands rather than files in `C:`, so on a 2.0+ system
/// they would run above the assign perfectly well — and that is sourced, not
/// recalled: all three appear in the internal-command list of *AmigaDOS Inside
/// and Out* (Kerkloh/Tornsdorf/Zoller, 1991,
/// <https://archive.org/stream/1991-kerkloh-tornsdorf-zoller-amigados-inside-and-out/1991-kerkloh-tornsdorf-zoller-amigados-inside-and-out_djvu.txt>),
/// and `FailAt` is described as ROM-resident in a 68k AmigaOS developer thread
/// (<https://www.forums.hollywood-mal.com/viewtopic.php?t=3357>). But that
/// list is the **AmigaDOS 2.0** one: under 1.3 these were disk commands, and
/// ART's audience is real hardware including A500s. Two sources that look like
/// they would settle it do **not** — neither the AmigaOS Documentation Wiki's
/// *AmigaDOS Advanced Features* page
/// (<https://wiki.amigaos.net/wiki/AmigaOS_Manual:_AmigaDOS_Advanced_Features>)
/// nor amigawiki's command list
/// (<https://amigawiki.org/doku.php?id=en%3Asystem%3Ados_commands_large>)
/// marks internal versus external at all, so do not re-walk those two
/// expecting an answer.
///
/// Putting the qualified `Assign` first **removes the need to be right about
/// any of that**: after it, `C:` exists, and every command below resolves
/// whether it is an internal or a file. The one remaining single point of
/// failure is that first line, which is fully qualified and therefore works on
/// 1.3 as well. The same thread carries a related hazard for later tasks in
/// this round: ROM-resident commands invoked through `Execute()` are reported
/// to fail on 3.1.4/3.2/3.9 while working on 3.1 — so nothing here should
/// reach for `Execute`.
///
/// The cost is that a failure of that first line leaves no result file at all,
/// and the host reports "never began". That is the honest report: the run did
/// not begin. Moving the marker above it would not help, because under the
/// hazard being guarded against the `Echo` that writes it is the thing that
/// cannot run.
///
/// ## Why `FailAt` is set as high as it is (ART-188, measured)
///
/// AmigaDOS aborts a script when a command's return code reaches `FailAt`,
/// which defaults to 10. An installer that returns `FAIL` (20) would then end
/// the script *before* the branch below could record anything, and the host
/// would see a file saying only `started` — a failure wearing a hang's
/// clothes. That is design §6's own second hazard: *"whatever writes it has to
/// run even when the installer fails, or a failure and a hang look
/// identical."*
///
/// This shipped as `FailAt 21`, reasoned from the convention that AmigaDOS
/// return codes are `0`, `5` (`WARN`), `10` (`ERROR`) and `20` (`FAIL`).
/// **The convention is not a rule, and the owner's own BoingBag disproved it
/// on the first real run** (2026-08-21, `BoingBag39-1 (1).lha`, `Updater`
/// 45.15, Kickstart 40.68): the Amiga's own screen read
///
/// ```text
///   Cannot open "resource.library", version 44.
///   ARTPkg:BoingBag3.9-1/C/Updater failed returncode 900
/// ```
///
/// 900 is far above 21, so the script aborted at that line, the branch below
/// never ran, and the work volume was left holding exactly `started`. The host
/// then polled for the rest of its deadline and would have reported **timed
/// out** — *"nobody answered a question it asked"* — about an installer that
/// had answered, immediately and clearly. §89 forbids that sentence, and this
/// round exists precisely because three earlier defects produced one like it.
///
/// So the threshold is set above any return code a program can express rather
/// than above the ones convention says it will use. `FailAt`'s argument is a
/// `LONG` (`RCLIM/N`), and the value below is the largest round number well
/// inside it. The measured 900 is the evidence; the value is not a multiple of
/// it, because the next package's number is not ART's to predict either.
///
/// It does not disturb the branch: `If Warn` tests the previous command's
/// return code against `WARN` (5) and is unaffected by the fail level, so a
/// non-zero code still reports `failed` — it now reports it *instead of*
/// killing the script.
///
/// ## Why the run refuses to repeat itself
///
/// Some installers reboot the Amiga when they are done. A reboot re-runs this
/// script, and a second pass would run the installer again over a tree it has
/// already changed. `If EXISTS` on a marker makes the second pass do nothing
/// and leave the first pass's answer alone. When that answer is only
/// `started`, the host times out — which is honest: an installer that rebooted
/// before recording anything has not been *observed* to succeed, and §89 does
/// not allow ART to say it did.
///
/// **Which marker is the whole question, and testing the wrong one was
/// ART-190.** The guard read [`RESULT_FILE`], which is written near the top —
/// so it stopped the second pass of *any* reboot, including one that happened
/// before the installer had run. And such a reboot is routine, not exotic: the
/// `SetPatch` this script now runs (ART-189) resets the machine after loading
/// a tree's ROM update, which is why an AmigaOS 3.9 system appears to boot
/// twice. Measured on 2026-08-21 against the owner's own tree, the second pass
/// printed *"this install already ran"* and stopped, and the installer was
/// never invoked at all.
///
/// So the guard reads [`INVOKED_FILE`], written **below** `SetPatch` and
/// directly above the installer. A reset `SetPatch` caused leaves it absent
/// and the second pass carries on to do the work; a reset the installer caused
/// leaves it present and the second pass stops, which is the case this guard
/// was written for. See [`INVOKED_FILE`]'s own documentation.
///
/// ## Why the assigns are here at all
///
/// ART's volume booted, so `SYS:` is ART's volume and the tree's commands,
/// libraries and devices are not reachable by name. The `C:` assign is
/// ART-118's actual measured blocker and nothing after it can run without it;
/// the rest of the set is reasoned rather than measured, the same standing as
/// its counterpart in
/// [`crate::core::launch::whdload_boot::startup_sequence`], and two members of
/// it are choices rather than transcription:
///
/// What a real `Updater` actually needs was a thing to measure against the
/// owner's own packages rather than assert here. It was measured on
/// 2026-08-21, and the three sections below are what the measurement said.
///
/// ## Why `ENV:` is built the way a real boot builds it (ART-192, measured)
///
/// `ENV:` and `ENVARC:` used to be **deliberately absent**, and the note that
/// stood here said so in as many words: *"a real `Startup-Sequence` builds
/// `ENV:` in `RAM:` and copies `ENVARC:` into it, which is several commands
/// whose failure modes have not been measured here. If an installer turns out
/// to need `ENV:`, the run will say so."*
///
/// **The run said so.** With `LIBS:` fixed below, the owner's own `Updater`
/// 45.15 got past `resource.library` and put a `System Request` on screen:
///
/// ```text
///   Please insert volume ENV in any drive     [ Retry ] [ Cancel ]
/// ```
///
/// Nobody was there to answer it, which is precisely what a requester costs an
/// unattended run — design §6's first hazard, met in the flesh.
///
/// So the four lines a real `Startup-Sequence` uses are here, in its order:
/// `MakeDir RAM:T RAM:Clipboards RAM:ENV RAM:ENV/Sys`, the assigns for `ENV:`,
/// `T:` and `CLIPS:`, and `Copy ENVARC: RAM:ENV ALL` to populate it — the
/// tree's own `Prefs/Env-Archive`, reached through the `ENVARC:` assign
/// AmigaDOS makes itself once `SYS:` points at the tree.
///
/// **The objection the old note raised has been removed rather than
/// overruled.** It argued against `MakeDir` because *"`MakeDir` on a directory
/// that already exists sets a return code, and this script's entire job is to
/// report a return code faithfully"* — true when `FailAt` was 21, since a
/// stray code could end the script. It is no longer: `FailAt` is now above
/// anything a command can return (ART-188), and the only return code this
/// script reads is the installer's own, tested by the `If Warn` directly below
/// the invocation. An intermediate command's code is neither fatal nor
/// reported.
///
/// `T:` moves with it, from `RAM:` to `RAM:T`. It pointed at the root of
/// `RAM:` only to avoid that `MakeDir`; with the `MakeDir` there for `ENV:`
/// anyway, the conventional target costs nothing and is what an Amiga
/// `Installer` writing to `T:` will have been tested against.
///
/// `Copy` is given `ALL QUIET NOREQ`: `ALL` because a real boot copies the
/// whole archive including `Sys/`, and **`NOREQ` because a requester is the
/// one thing an unattended run cannot survive** — a missing `ENVARC:` must
/// make `Copy` return a code, not open a second System Request behind the
/// first.
///
/// One difference from a real boot, stated rather than hidden: these four
/// lines sit **above** `SetPatch` here and below it there, so a `SetPatch`
/// reset repeats them on the second pass. That costs one `MakeDir` on
/// directories that already exist and one re-copy of `ENVARC:` into a `RAM:`
/// the reset emptied anyway — nothing, since `RAM:` does not survive the
/// reset. This is the order the fix was verified in and it was left there
/// rather than tidied afterwards on reasoning alone.
///
/// ## Why `LIBS:` also carries the tree's `Classes` drawer (ART-191, measured)
///
/// With every assign above in place, the owner's own `Updater` 45.15 ended at
/// once with
///
/// ```text
///   Cannot open "resource.library", version 44.
/// ```
///
/// The tree carries that library — `Libs/RESOURCE.LIBRARY`, whose own `$VER:`
/// string reads `resource.library 44.102 (29-Sep-99)`, so the version asked
/// for is the version present, and `LIBS:` really did resolve to the tree's
/// drawer: probed on the running Amiga, `asl.library 45.4`,
/// `amigaguide.library 44.4` and `icon.library 44.543` all opened from it.
/// Only this one would not.
///
/// **The library says why itself.** Its printable strings name five BOOPSI
/// classes it opens —
///
/// ```text
///   gadgets/chooser.gadget      gadgets/clicktab.gadget
///   gadgets/listbrowser.gadget  gadgets/radiobutton.gadget
///   gadgets/speedbar.gadget
/// ```
///
/// — and those live in `SYS:Classes/Gadgets`, which the tree carries and which
/// **nothing had put on `LIBS:`**. A class that will not open makes the
/// library's own initialisation fail, and a library that fails to initialise
/// is `OpenLibrary` returning `NULL`, which is the sentence above.
///
/// The tree's own `S/Startup-Sequence` does it in one line, and this is that
/// line:
///
/// ```text
///   Assign >NIL: LIBS: SYS:Classes ADD
/// ```
///
/// `ADD` rather than a second `Assign`: `LIBS:` becomes both drawers, in that
/// order, which is what a real boot leaves and what every 3.9 library that
/// opens a class expects.
///
/// ## Why the tree's own `SetPatch` runs (ART-189)
///
/// AmigaOS 3.5 and 3.9 are a disk-based operating system over a V40 (or older)
/// Kickstart, and the thing that reconciles the two runs first in the tree's
/// own `S/Startup-Sequence`, ahead of every assign it makes:
///
/// ```text
///   C:SetPatch QUIET
/// ```
///
/// It loads `Devs/AmigaOS ROM Update` (127 956 bytes in the owner's tree) over
/// the ROM. ART's script boots ART's own volume, so nothing had run it, and
/// the installer met a 3.1 ROM under a 3.9 system.
///
/// **Measured, and measured honestly: this was added while diagnosing the
/// `resource.library` failure above and did not fix it** — the class assign
/// did. What it demonstrably does is apply the update: with this line the
/// booted tree answers `Kickstart 40.68, Workbench 45.1`,
/// `workbench.library 45.102`, `version.library 45.1`, and its own
/// copyright banner changes from `1985-1993 Commodore-Amiga` to
/// `1985-2000 Amiga International`. Whether a given package's installer would
/// fail without it has not been measured; what has is that a tree ART runs an
/// installer against is now in the state the tree's own boot puts it in, which
/// is the state its files were built for.
///
/// **This is the tree's own command, run as the tree's own boot runs it.** It
/// is not ART patching anything, and it is not ART touching the user's
/// `Startup-Sequence` (§1) — it is one line the medium ships, invoked by an
/// explicit path on the mounted tree in the same style as the `C:` assign
/// above.
///
/// It is guarded by `If EXISTS` rather than run unconditionally: a tree that
/// carries no `SetPatch` is a tree that does not need one — AmigaOS 3.1 and
/// earlier have no ROM update to load — and a missing-command failure directly
/// above the installer would be a return code this script is supposed to
/// reserve for the installer itself.
///
/// `QUIET` is the release's own wording on that line, not ART's choice.
///
/// ## Why a `CD` may sit between the assigns and the installer
///
/// [`PlannedRun::working_directory`] carries the drawer the installer is run
/// from, and its own documentation carries the reading from the owner's
/// `BoingBag39-1.lha` that makes it necessary: the package's arguments are
/// relative to the package's own drawer, and a shell that booted from ART's
/// volume is not sitting in it. The alternative — rewriting the recipe's
/// arguments into whole paths — would mean ART deciding which of a program's
/// arguments are paths, about a program it did not write.
///
/// [`PlannedRun::working_directory`]: super::PlannedRun::working_directory
pub fn startup_sequence(run: &PlannedRun) -> CoreResult<String> {
    refuse_shell_metacharacters("package id", &run.package_id)?;
    refuse_shell_metacharacters("system volume name", &run.system_volume)?;
    refuse_shell_metacharacters("installer path", &run.program)?;
    for arg in &run.args {
        refuse_shell_metacharacters("installer argument", arg)?;
    }
    if let Some(dir) = &run.working_directory {
        refuse_shell_metacharacters("working directory", dir)?;
        // A blank one would generate a bare `CD`, which in AmigaDOS *prints*
        // the current directory instead of changing it — a line that succeeds
        // and does nothing, leaving the installer to run from wherever the
        // boot left the shell. That is precisely the state naming a directory
        // exists to remove, so an empty name is refused rather than emitted.
        if dir.trim().is_empty() {
            return Err(CoreError::InvalidInput(
                "an Amiga-side install's working directory may not be blank".into(),
            ));
        }
    }
    if run.program.trim().is_empty() {
        return Err(CoreError::InvalidInput(
            "an Amiga-side install needs a program to run".into(),
        ));
    }
    // An empty `system_volume` is not a harmless blank: it generates
    // `:C/Assign C: :C` and `Assign SYS: :`, a script that parses cleanly and
    // assigns nothing — the ART-118 failure shape again, and this time silent.
    if run.system_volume.trim().is_empty() {
        return Err(CoreError::InvalidInput(
            "an Amiga-side install needs the volume its system tree is mounted as".into(),
        ));
    }
    // ART's own volume is not a place the run may reach into. It holds the
    // script currently executing and the result file the host is waiting on,
    // so an installer pointed at it could overwrite either — and the second
    // would make a run report an outcome nothing produced. This is the one
    // confinement the type can enforce on its own; see `PlannedRun::program`
    // for the one it cannot.
    //
    // The rule itself lives in `super::claims_work_volume`, because the mount
    // planner needs the same one and a second copy of it there was weaker
    // than this one.
    for (label, value) in std::iter::once(("installer path", &run.program))
        .chain(run.args.iter().map(|a| ("installer argument", a)))
        .chain(std::iter::once(("system volume name", &run.system_volume)))
        .chain(
            run.working_directory
                .iter()
                .map(|d| ("working directory", d)),
        )
    {
        if claims_work_volume(value) {
            return Err(CoreError::SafetyRefused(format!(
                "'{value}' names ART's own work volume; a {label} may not reach into it"
            )));
        }
    }

    // The **system volume alone** may not be ART's package volume (ART-185).
    // Deliberately not folded into the loop above: the installer path and the
    // working directory are *supposed* to begin `ARTPkg:`, because that is
    // where the package was mounted. What this refuses is the tree taking the
    // same device name, which shadows the package and leaves the run's `CD`
    // pointing at a drawer that is not there — the silent shape again, this
    // time produced by a name rather than by a missing mount.
    if claims_package_volume(&run.system_volume) {
        return Err(CoreError::SafetyRefused(format!(
            "'{}' names '{PACKAGE_VOLUME}', the volume ART mounts the package's own files under; \
             a system volume may not shadow it",
            run.system_volume
        )));
    }

    // Each argument is validated on its own above, so joining them here cannot
    // reintroduce a separator that was refused individually.
    let mut command = run.program.clone();
    for arg in &run.args {
        command.push(' ');
        command.push_str(arg);
    }

    // Below every `Assign`, so `CD` itself resolves through `C:` whether or
    // not it is a Shell-internal command on the system being run — the same
    // reasoning that put the fully-qualified assign first. Directly above the
    // installer, so nothing between the two can move the shell again.
    //
    // A `CD` that fails sets a return code and the script carries on to the
    // installer, which then runs from wherever the shell already was and
    // almost certainly says no — reported as `failed`, which is what it is.
    // Branching on it here would mean a second `If`/`Else` inside the one
    // that already reports the outcome, for an ending the installer's own
    // return code already covers.
    let cd_line = match &run.working_directory {
        Some(dir) => format!("\x20 CD {dir}\n"),
        None => String::new(),
    };

    let sys = &run.system_volume;
    let work = WORK_VOLUME;
    let result = RESULT_FILE;
    let invoked = INVOKED_FILE;
    let package = &run.package_id;

    Ok(format!(
        "; Written by ART to install '{package}'. One run, then a result.\n\
         {sys}:C/Assign C: {sys}:C\n\
         FailAt {FAIL_AT}\n\
         If EXISTS {work}:{invoked}\n\
         \x20 Echo \"ART: the installer already ran on an earlier pass. Not repeating it.\"\n\
         Else\n\
         \x20 Echo >{work}:{result} \"{MARK_STARTED}\"\n\
         \x20 Assign SYS: {sys}:\n\
         \x20 Assign S: {sys}:S\n\
         \x20 Assign L: {sys}:L\n\
         \x20 Assign LIBS: {sys}:Libs\n\
         \x20 Assign LIBS: {sys}:Classes ADD\n\
         \x20 Assign DEVS: {sys}:Devs\n\
         \x20 Assign FONTS: {sys}:Fonts\n\
         \x20 MakeDir RAM:T RAM:Clipboards RAM:ENV RAM:ENV/Sys\n\
         \x20 Assign T: RAM:T\n\
         \x20 Assign CLIPS: RAM:Clipboards\n\
         \x20 Assign ENV: RAM:ENV\n\
         \x20 Copy ENVARC: RAM:ENV ALL QUIET NOREQ\n\
         \x20 If EXISTS {sys}:C/SetPatch\n\
         \x20   {sys}:C/SetPatch QUIET\n\
         \x20 EndIf\n\
         \x20 Echo >{work}:{invoked} \"{MARK_INVOKED}\"\n\
         {cd_line}\
         \x20 {command}\n\
         \x20 If Warn\n\
         \x20   Echo >{work}:{result} \"{MARK_FAILED}\"\n\
         \x20 Else\n\
         \x20   Echo >{work}:{result} \"{MARK_OK}\"\n\
         \x20 EndIf\n\
         EndIf\n"
    ))
}

/// Build ART's own boot volume into `at`.
///
/// `at` must be absent or **empty**. The refusal below is not tidiness:
/// `build` writes a `Startup-Sequence`, and a mistyped path that happened to
/// point at the user's distribution tree would overwrite theirs. Refusing
/// anything with content in it means the only directory this can write into
/// holds nothing anyone would miss.
///
/// It is deliberately *empty* rather than *freshly created*: ART cannot tell
/// the two apart without inventing a marker file, and a marker file would
/// break `the_work_volume_contains_only_what_art_wrote`'s promise that this
/// volume holds one file and nothing else. An empty directory the caller
/// prepared is as safe as one `build` made itself — the property that matters
/// is that nothing is destroyed, and emptiness is exactly that property.
pub fn build(at: &Path, run: &PlannedRun) -> CoreResult<()> {
    // Validate before touching the filesystem: whether a hostile string is
    // refused must not depend on what happens to be on disk.
    let text = startup_sequence(run)?;

    if at.exists() {
        let mut entries = std::fs::read_dir(at)?;
        if entries.next().is_some() {
            return Err(CoreError::SafetyRefused(format!(
                "'{}' already has contents; ART's work volume is built into an empty directory",
                at.display()
            )));
        }
    }

    let s_dir = at.join("S");
    std::fs::create_dir_all(&s_dir)?;
    atomic_write(&s_dir.join("Startup-Sequence"), text.as_bytes())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A scratch directory that removes itself, including when the test
    /// panics (ART-184).
    ///
    /// It used to hand back a bare `PathBuf` with a trailing
    /// `remove_dir_all` at each call site — the shape ART-184 was filed
    /// about, in a module written after it was filed, and Task 4's review
    /// measured it as the whole of the +18 directories one
    /// `cargo test amigainstall` left in `%TEMP%`. A trailing statement is
    /// exactly what a failing test skips.
    fn scratch(tag: &str) -> crate::core::ScratchDir {
        crate::core::ScratchDir::new("art-amigainstall", tag)
    }

    /// Every path under `root`, relative and `/`-separated, so an assertion
    /// reads the same on any host.
    fn walk_relative(root: &Path) -> Vec<String> {
        fn walk(dir: &Path, prefix: &str, out: &mut Vec<String>) {
            for entry in std::fs::read_dir(dir).unwrap() {
                let entry = entry.unwrap();
                let name = entry.file_name().to_string_lossy().to_string();
                let rel = if prefix.is_empty() {
                    name
                } else {
                    format!("{prefix}/{name}")
                };
                if entry.file_type().unwrap().is_dir() {
                    walk(&entry.path(), &rel, out);
                } else {
                    out.push(rel);
                }
            }
        }
        let mut out = Vec::new();
        walk(root, "", &mut out);
        out
    }

    /// Every whole line strictly above the one carrying the started marker,
    /// trimmed. Whole lines, because the marker sits mid-line and a byte
    /// offset would leave half of its own `Echo` looking like a command that
    /// ran before it.
    fn lines_above_the_marker(script: &str) -> Vec<&str> {
        let at = script.find(MARK_STARTED).expect("a started marker");
        let line_start = script[..at].rfind('\n').map(|i| i + 1).unwrap_or(0);
        script[..line_start].lines().map(str::trim).collect()
    }

    fn planned(command: &str) -> PlannedRun {
        PlannedRun {
            package_id: "test-pack".to_string(),
            system_volume: "DH0".to_string(),
            program: command.to_string(),
            args: Vec::new(),
            working_directory: None,
        }
    }

    /// Pins the complete text, not a substring of it.
    ///
    /// Every other test here asserts one relationship — an ordering, a
    /// refusal, a branch — and a substring check cannot notice a line that
    /// went missing between two it did look at, or a carriage return that
    /// would reach an AmigaDOS shell as part of a filename. This is the same
    /// reasoning `whdload_boot`'s own exact-match test states, and it is what
    /// makes a change to this script a deliberate act rather than a
    /// side-effect. The trailing empty element is the final newline: a script
    /// whose last line has no terminator is one the Shell may not run.
    #[test]
    fn the_whole_script_is_what_it_is() {
        let mut run = planned("PKG:C/Updater");
        run.package_id = "boingbag-39-1".to_string();

        let expected = [
            "; Written by ART to install 'boingbag-39-1'. One run, then a result.",
            "DH0:C/Assign C: DH0:C",
            "FailAt 2000000000",
            "If EXISTS ARTWork:art-invoked.txt",
            "  Echo \"ART: the installer already ran on an earlier pass. Not repeating it.\"",
            "Else",
            "  Echo >ARTWork:art-result.txt \"started\"",
            "  Assign SYS: DH0:",
            "  Assign S: DH0:S",
            "  Assign L: DH0:L",
            "  Assign LIBS: DH0:Libs",
            "  Assign LIBS: DH0:Classes ADD",
            "  Assign DEVS: DH0:Devs",
            "  Assign FONTS: DH0:Fonts",
            "  MakeDir RAM:T RAM:Clipboards RAM:ENV RAM:ENV/Sys",
            "  Assign T: RAM:T",
            "  Assign CLIPS: RAM:Clipboards",
            "  Assign ENV: RAM:ENV",
            "  Copy ENVARC: RAM:ENV ALL QUIET NOREQ",
            "  If EXISTS DH0:C/SetPatch",
            "    DH0:C/SetPatch QUIET",
            "  EndIf",
            "  Echo >ARTWork:art-invoked.txt \"invoked\"",
            "  PKG:C/Updater",
            "  If Warn",
            "    Echo >ARTWork:art-result.txt \"failed\"",
            "  Else",
            "    Echo >ARTWork:art-result.txt \"ok\"",
            "  EndIf",
            "EndIf",
            "",
        ]
        .join(
            "
",
        );

        assert_eq!(startup_sequence(&run).unwrap(), expected);
    }

    /// The installer runs **from the package's own drawer**, and that line
    /// sits directly above the invocation with every `Assign` above it.
    ///
    /// A package's arguments are relative to that drawer — the owner's
    /// `BoingBag39-1.lha` runs `C/Updater AmigaOS-Update "<target>"` from
    /// there — and ART passes them through as declared rather than deciding
    /// which of a program's arguments are paths. So where the shell is
    /// standing is the whole of what makes `AmigaOS-Update` resolvable, and
    /// a `CD` that drifted above an `Assign` would resolve through a `C:`
    /// that does not exist yet.
    #[test]
    fn the_installer_runs_from_the_directory_it_was_given() {
        let mut run = planned("DH0:BoingBag3.9-1/C/Updater");
        run.args = vec!["AmigaOS-Update".to_string(), "DH0:".to_string()];
        run.working_directory = Some("DH0:BoingBag3.9-1".to_string());

        let ss = startup_sequence(&run).unwrap();
        let lines: Vec<&str> = ss.lines().map(str::trim).collect();

        let cd = lines
            .iter()
            .position(|l| *l == "CD DH0:BoingBag3.9-1")
            .unwrap_or_else(|| {
                panic!(
                    "no CD line in:
{ss}"
                )
            });
        let command = lines
            .iter()
            .position(|l| l.starts_with("DH0:BoingBag3.9-1/C/Updater"))
            .expect("the installer");

        assert_eq!(
            cd + 1,
            command,
            "the CD must sit directly above the installer:
{ss}"
        );
        assert!(
            lines[..cd].iter().any(|l| l.starts_with("Assign SYS:")),
            "and below the assigns, so it resolves through C::
{ss}"
        );
        assert_eq!(
            lines[command], "DH0:BoingBag3.9-1/C/Updater AmigaOS-Update DH0:",
            "the arguments reach the Amiga exactly as they were composed"
        );
    }

    /// Without one, nothing is emitted at all — no bare `CD`, which in
    /// AmigaDOS prints the current directory instead of changing it and would
    /// read as a line that did its job.
    #[test]
    fn no_working_directory_emits_no_cd_at_all() {
        let ss = startup_sequence(&planned("DH0:C/Updater")).unwrap();
        assert!(
            !ss.lines().any(|l| l.trim().starts_with("CD")),
            "got:
{ss}"
        );
    }

    /// The working directory is a value ART interpolates into a command line,
    /// so it is refused on exactly the same terms as the program and its
    /// arguments: no metacharacter, nothing naming ART's own volume, and
    /// nothing blank.
    #[test]
    fn a_hostile_or_blank_working_directory_is_refused() {
        for hostile in [
            "DH0:Pkg ; Delete SYS:#?",
            "DH0:Pkg
Delete SYS:#?",
            "DH0:P\"kg",
            "ARTWork:",
            "ARTWork",
            "",
            "   ",
        ] {
            let mut run = planned("DH0:C/Updater");
            run.working_directory = Some(hostile.to_string());
            assert!(
                startup_sequence(&run).is_err(),
                "'{hostile}' must be refused as a working directory"
            );
        }
    }

    /// The volume boots ART's script, not the user's system.
    #[test]
    fn the_work_volume_carries_its_own_startup_sequence() {
        let dir = scratch("workvol-startup");
        build(dir.path(), &planned("C:Updater")).unwrap();
        let ss = std::fs::read_to_string(dir.join("S/Startup-Sequence")).unwrap();
        assert!(ss.contains("Updater"), "got {ss}");
    }

    /// The result is written **before** the installer runs, then again after,
    /// so a run that never returns is still distinguishable from one that was
    /// never started. A hang and a crash look identical otherwise.
    #[test]
    fn a_started_run_is_marked_before_the_installer_is_invoked() {
        let dir = scratch("workvol-order");
        build(dir.path(), &planned("C:Updater")).unwrap();
        let ss = std::fs::read_to_string(dir.join("S/Startup-Sequence")).unwrap();
        let started = ss.find(MARK_STARTED).expect("a started marker");
        let invoke = ss.find("Updater").expect("the installer");
        assert!(started < invoke, "the marker must be written first:\n{ss}");
    }

    /// Exactly one command runs above the started marker, and it is the one
    /// that needs nothing assigned in order to run.
    ///
    /// Every other line resolves through `C:`, so every other line is
    /// something that might not return — ART-118 was exactly an `Assign` that
    /// could not run. Putting the fully-qualified `Assign` first means the
    /// script never depends on which commands a given Kickstart keeps in ROM;
    /// a line moved across that boundary breaks this test, which is the point
    /// of it.
    #[test]
    fn only_the_fully_qualified_assign_runs_above_the_started_marker() {
        let ss = startup_sequence(&planned("PKG:C/Updater")).unwrap();

        let above: Vec<&str> = lines_above_the_marker(&ss)
            .into_iter()
            .filter(|l| !l.is_empty() && !l.starts_with(';'))
            // `Else` and the re-run arm's `Echo` belong to the guard the
            // marker lives inside, not to the sequence that precedes it.
            .filter(|l| *l != "Else" && !l.starts_with("Echo \"ART:"))
            .collect();

        assert_eq!(
            above,
            vec![
                "DH0:C/Assign C: DH0:C",
                "FailAt 2000000000",
                "If EXISTS ARTWork:art-invoked.txt",
            ],
            "got:\n{ss}"
        );
        assert!(
            above[0].starts_with("DH0:C/"),
            "the first command must name its executable by path, not through C:"
        );
    }

    /// Every `Assign` invoked *by name* is below the marker, because each of
    /// them resolves through the `C:` the first line created.
    #[test]
    fn the_started_marker_precedes_every_assign_invoked_by_name() {
        let ss = startup_sequence(&planned("PKG:C/Updater")).unwrap();
        for line in lines_above_the_marker(&ss) {
            assert!(
                !line.starts_with("Assign "),
                "'{line}' is an Assign by name above the marker:\n{ss}"
            );
        }
        assert!(
            ss.contains("Assign SYS:"),
            "the assigns must still happen:\n{ss}"
        );
    }

    /// The script writes an outcome whether the installer succeeded or not.
    /// Without this a failure and a hang are the same silence.
    #[test]
    fn the_script_records_an_outcome_on_both_paths() {
        let dir = scratch("workvol-both");
        build(dir.path(), &planned("C:Updater")).unwrap();
        let ss = std::fs::read_to_string(dir.join("S/Startup-Sequence")).unwrap();
        assert!(
            ss.to_lowercase().contains("if warn") || ss.to_lowercase().contains("if fail"),
            "the script must branch on the installer's return code:\n{ss}"
        );
    }

    /// Branching is not enough on its own: AmigaDOS aborts a script when a
    /// return code reaches `FailAt`, which defaults to 10, so an installer
    /// returning `FAIL` (20) would end the run before the branch could record
    /// anything — and the host would read a hang where there was a refusal.
    ///
    /// **And 21 was not enough either — ART-188.** The owner's own
    /// `BoingBag39-1 (1).lha` `Updater` 45.15 returned **900** on 2026-08-21,
    /// which the shipped `FailAt 21` let abort the script; the work volume was
    /// left holding `started` and the run was heading for a *timed out* report
    /// about an installer that had already answered. So this asserts the level
    /// against the number that was actually observed rather than against the
    /// convention that missed it, and it names the number so a future edit
    /// that lowers the level back into range fails here rather than on the
    /// owner's desktop.
    const MEASURED_REAL_RETURN_CODE: i64 = 900;

    #[test]
    fn a_failing_installer_cannot_abort_the_script_before_it_reports() {
        let ss = startup_sequence(&planned("PKG:C/Updater")).unwrap();

        // Read the level out of the generated script rather than off the
        // constant: what protects the run is the number the Amiga's shell
        // sees, and a `FailAt` line that stopped being formatted from
        // `FAIL_AT` would still satisfy a comparison between two constants.
        let line = ss
            .lines()
            .find(|l| l.trim_start().starts_with("FailAt "))
            .expect("the script must raise FailAt");
        let level: i64 = line
            .trim()
            .trim_start_matches("FailAt ")
            .parse()
            .expect("FailAt takes a number");

        let failat = ss.find(line).expect("the line is in the script");
        let invoke = ss.find("PKG:C/Updater").expect("the installer");
        assert!(
            failat < invoke,
            "FailAt must be raised before the installer runs:\n{ss}"
        );
        assert!(
            level > MEASURED_REAL_RETURN_CODE,
            "a real Updater returned {MEASURED_REAL_RETURN_CODE}; a fail level of {level} \
             would abort the script before it could report that"
        );
    }

    /// The ROM update the tree ships is loaded before the installer runs, and
    /// only when the tree carries it.
    ///
    /// **ART-189, measured.** With every assign in place the owner's own
    /// `Updater` still ended at once with `Cannot open "resource.library",
    /// version 44.` — a library the tree carries at version 44.102. What was
    /// missing is the line the tree's own `Startup-Sequence` runs eighth,
    /// ahead of all of its assigns: `C:SetPatch QUIET`, which loads
    /// `Devs/AmigaOS ROM Update` over a V40 Kickstart. Below every assign, so
    /// `DEVS:` resolves; above the installer, so the libraries it opens are
    /// the updated ones.
    #[test]
    fn the_trees_own_rom_update_is_loaded_before_the_installer() {
        let ss = startup_sequence(&planned("PKG:C/Updater")).unwrap();

        let guard = ss
            .find("If EXISTS DH0:C/SetPatch")
            .expect("SetPatch must be guarded: a tree without one does not need one");
        let setpatch = ss
            .find("DH0:C/SetPatch QUIET")
            .expect("the tree's own SetPatch, by an explicit path on the tree");
        let devs = ss.find("Assign DEVS:").expect("the DEVS: assign");
        let invoke = ss.find("PKG:C/Updater").expect("the installer");

        assert!(
            devs < guard && guard < setpatch,
            "SetPatch reads DEVS:AmigaOS ROM Update, so DEVS: must be assigned first:\n{ss}"
        );
        assert!(
            setpatch < invoke,
            "the ROM update must be in place before the installer opens a library:\n{ss}"
        );
    }

    /// Some installers reboot when they are done, and a reboot re-runs this
    /// script. A second pass must not install over a tree the first pass
    /// already changed.
    #[test]
    fn a_second_boot_does_not_run_the_installer_again() {
        let ss = startup_sequence(&planned("PKG:C/Updater")).unwrap();
        let guard = ss
            .find(&format!("If EXISTS {WORK_VOLUME}:{INVOKED_FILE}"))
            .expect("a guard on the invoked marker");
        let else_at = guard + ss[guard..].find("\nElse\n").expect("an Else arm");
        let invoke = ss.find("PKG:C/Updater").expect("the installer");
        assert!(
            guard < else_at && else_at < invoke,
            "the installer belongs in the arm the guard did not take:\n{ss}"
        );

        // The arm a rebooted Amiga takes runs nothing and, just as important,
        // rewrites nothing: overwriting the first pass's answer would erase
        // the very outcome the host is waiting to read.
        let repeat_arm = &ss[guard..else_at];
        assert!(
            !repeat_arm.contains("Updater"),
            "a repeat boot must not run the installer again:\n{ss}"
        );
        assert!(
            !repeat_arm.contains(&format!("Echo >{WORK_VOLUME}:{RESULT_FILE}")),
            "a repeat boot must not rewrite the result file:\n{ss}"
        );
    }

    /// A reboot **before** the installer must leave the second pass free to do
    /// the work — ART-190.
    ///
    /// The guard above stops a second pass, and the guard above is right for
    /// the reboot it was written for: one the installer caused. It was wrong
    /// for the other one, which this script now causes itself. `SetPatch`
    /// loads a tree's `Devs/AmigaOS ROM Update` and resets the machine — the
    /// reason an AmigaOS 3.9 system appears to boot twice — and with the guard
    /// reading a marker written *above* that line, the second pass printed
    /// "already ran" and stopped before the installer had ever been invoked.
    /// Measured on the owner's own tree, 2026-08-21.
    ///
    /// So the marker the guard reads has to be written **below** `SetPatch`.
    /// This asserts the ordering that makes the two reboots distinguishable,
    /// which is the whole of the fix.
    #[test]
    fn a_reboot_before_the_installer_lets_the_second_pass_do_the_work() {
        let ss = startup_sequence(&planned("PKG:C/Updater")).unwrap();

        let guard = ss
            .find(&format!("If EXISTS {WORK_VOLUME}:{INVOKED_FILE}"))
            .expect("the re-run guard");
        let setpatch = ss.find("DH0:C/SetPatch QUIET").expect("SetPatch");
        let marker = ss
            .find(&format!("Echo >{WORK_VOLUME}:{INVOKED_FILE}"))
            .expect("the invoked marker");
        let invoke = ss.find("PKG:C/Updater").expect("the installer");

        assert!(
            setpatch < marker && marker < invoke,
            "the marker the guard reads must be written below SetPatch and above the \
             installer, or a SetPatch reset skips the install entirely:\n{ss}"
        );
        assert_ne!(
            INVOKED_FILE, RESULT_FILE,
            "the guard's marker and the host's result file are different questions"
        );
        assert!(
            guard < setpatch,
            "the guard is still the outermost thing, so a second pass reaches it first:\n{ss}"
        );

        // And the started marker stays where it was: the host's three-way
        // reading of `art-result.txt` (absent / started / an outcome) is not
        // what changed.
        let started = ss
            .find(&format!(
                "Echo >{WORK_VOLUME}:{RESULT_FILE} \"{MARK_STARTED}\""
            ))
            .expect("the started marker");
        assert!(started < setpatch, "started is written first:\n{ss}");
    }

    /// Nothing ART generates is assembled from a string ART did not author.
    /// A package name is shipped data; an archive's contents are not.
    #[test]
    fn a_command_with_amigados_metacharacters_is_refused() {
        let dir = scratch("workvol-meta");
        for hostile in [
            "C:Updater ; Delete SYS:#?",
            "C:Updater\nDelete SYS:#?",
            "C:Up\"dater",
            "C:Updater >DH0:C/Assign",
            "C:Updater `Format DH0:`",
        ] {
            assert!(
                build(dir.path(), &planned(hostile)).is_err(),
                "{hostile} should be refused"
            );
        }
        assert!(
            walk_relative(dir.path()).is_empty(),
            "a refused run writes nothing"
        );
    }

    /// The installer's path is not the only value that reaches the script —
    /// the arguments and the volume name are interpolated the same way.
    #[test]
    fn a_hostile_argument_or_volume_name_is_refused_too() {
        let mut run = planned("PKG:C/Updater");
        run.args = vec!["QUIET".to_string(), "; Delete SYS:#?".to_string()];
        assert!(
            startup_sequence(&run).is_err(),
            "arguments are interpolated"
        );

        let mut run = planned("PKG:C/Updater");
        run.system_volume = "DH0\nFormat".to_string();
        assert!(startup_sequence(&run).is_err(), "so is the volume name");

        let mut run = planned("PKG:C/Updater");
        run.package_id = "pack\"; Delete".to_string();
        assert!(startup_sequence(&run).is_err(), "and so is the package id");
    }

    /// Arguments stay separate strings up to the last possible moment, and
    /// each is checked on its own — so a legitimate multi-argument installer
    /// still runs.
    #[test]
    fn ordinary_arguments_reach_the_command_line() {
        let mut run = planned("PKG:Installer");
        run.args = vec!["SCRIPT".to_string(), "PKG:Install".to_string()];
        let ss = startup_sequence(&run).unwrap();
        assert!(
            ss.contains("PKG:Installer SCRIPT PKG:Install"),
            "got:\n{ss}"
        );
    }

    /// An empty `system_volume` is not a blank that formats away: it produces
    /// `:C/Assign C: :C` and `Assign SYS: :`, a script that parses cleanly and
    /// assigns nothing. That is ART-118's failure again, and silent this time.
    #[test]
    fn a_run_with_no_system_volume_is_refused() {
        let mut run = planned("PKG:C/Updater");
        run.system_volume = "  ".to_string();
        assert!(startup_sequence(&run).is_err());
    }

    /// ART's own volume holds the script that is running and the result file
    /// the host is waiting on. An installer pointed at it could overwrite
    /// either — and overwriting the result would make a run report an outcome
    /// that nothing produced.
    #[test]
    fn nothing_in_the_run_may_name_arts_own_volume() {
        let mut run = planned(&format!("{WORK_VOLUME}:S/Startup-Sequence"));
        assert!(startup_sequence(&run).is_err(), "not as the program");

        // AmigaDOS volume names are case-insensitive, so the guard must be.
        run = planned("artwork:art-result.txt");
        assert!(startup_sequence(&run).is_err(), "nor in another case");

        run = planned("PKG:C/Updater");
        run.args = vec![format!("{WORK_VOLUME}:{RESULT_FILE}")];
        assert!(startup_sequence(&run).is_err(), "nor as an argument");

        run = planned("PKG:C/Updater");
        run.system_volume = WORK_VOLUME.to_string();
        assert!(
            startup_sequence(&run).is_err(),
            "nor as the volume the tree is mounted as"
        );
    }

    /// The **system volume alone** may not name ART's package volume — and
    /// the installer path and the working directory must still be free to,
    /// because that is where the package was mounted (ART-185).
    ///
    /// Both halves are asserted here on purpose. Folding the package volume
    /// into the loop above would refuse every real run: `ARTPkg:C/Updater` is
    /// the correct value, not a hostile one. Leaving the check out altogether
    /// lets a tree take the package's device name, shadow it, and turn the
    /// script's `CD` into ART-185's silent failure by another route.
    #[test]
    fn only_the_system_volume_is_refused_for_naming_the_package_volume() {
        let mut run = planned(&format!("{PACKAGE_VOLUME}:BoingBag3.9-1/C/Updater"));
        run.working_directory = Some(format!("{PACKAGE_VOLUME}:BoingBag3.9-1"));
        run.args = vec![format!("{PACKAGE_VOLUME}:BoingBag3.9-1/AmigaOS-Update")];
        startup_sequence(&run).expect("the package's own volume is where the installer is");

        for hostile in [PACKAGE_VOLUME, "artpkg", "ARTPkg:"] {
            let mut run = planned(&format!("{PACKAGE_VOLUME}:C/Updater"));
            run.system_volume = hostile.to_string();
            let err = startup_sequence(&run).unwrap_err();
            assert!(
                matches!(err, CoreError::SafetyRefused(ref m) if m.contains(PACKAGE_VOLUME)),
                "'{hostile}' must be refused by name: {err:?}"
            );
        }

        // And the guard is a claim on the volume, not a prefix match.
        let mut run = planned(&format!("{PACKAGE_VOLUME}:C/Updater"));
        run.system_volume = "ARTPkgStore".to_string();
        startup_sequence(&run).unwrap();
    }

    /// The guard refuses a *claim on the volume*, not any name that begins
    /// with the same letters — refusing `ARTWorkbench:` would be a guard that
    /// grew past what it was for.
    #[test]
    fn a_volume_whose_name_merely_starts_the_same_is_not_refused() {
        startup_sequence(&planned("ARTWorkbench:C/Updater")).unwrap();
        startup_sequence(&planned("ARTWork-2:C/Updater")).unwrap();
    }

    /// A non-ASCII name must not be able to kill the application.
    ///
    /// The work-volume comparison looks at a fixed number of leading bytes,
    /// and slicing a `&str` there would panic if the boundary fell inside a
    /// multi-byte character. `panic = "abort"` in the release profile makes
    /// that fatal, and non-ASCII AmigaDOS names are a thing this project
    /// already meets (ART-113).
    #[test]
    fn a_non_ascii_name_is_compared_without_panicking() {
        // The comparison looks at `WORK_VOLUME.len()` == 7 leading bytes, and
        // this name puts a two-byte character across bytes 6 and 7 — so byte 7
        // is a continuation byte and not a character boundary. Slicing the
        // `&str` there panics; comparing the bytes does not.
        assert_eq!(
            "Amigatürk".as_bytes()[7],
            0xbc,
            "the fixture must straddle the boundary, or it tests nothing"
        );
        startup_sequence(&planned("Amigatürk:C/Updater")).unwrap();

        startup_sequence(&planned("türkçe:C/Updater")).unwrap();
        let mut run = planned("PKG:C/Updater");
        run.system_volume = "Amigatürk".to_string();
        startup_sequence(&run).unwrap();
    }

    /// An empty program would produce a script that reports an outcome for an
    /// installer that never ran.
    #[test]
    fn a_run_with_nothing_to_run_is_refused() {
        assert!(startup_sequence(&planned("   ")).is_err());
    }

    /// The volume must not be a place the installer can escape from.
    #[test]
    fn the_work_volume_contains_only_what_art_wrote() {
        let dir = scratch("workvol-contents");
        build(dir.path(), &planned("C:Updater")).unwrap();
        let mut found: Vec<String> = walk_relative(dir.path());
        found.sort();
        assert_eq!(found, vec!["S/Startup-Sequence".to_string()]);
    }

    /// A mistyped destination must not be able to overwrite somebody's real
    /// `Startup-Sequence` — the tree ART is installing *into* has one.
    #[test]
    fn building_into_a_directory_that_already_has_contents_is_refused() {
        let dir = scratch("workvol-occupied");
        std::fs::create_dir_all(dir.join("S")).unwrap();
        std::fs::write(dir.join("S/Startup-Sequence"), b"the user's own\n").unwrap();

        assert!(build(dir.path(), &planned("C:Updater")).is_err());
        assert_eq!(
            std::fs::read(dir.join("S/Startup-Sequence")).unwrap(),
            b"the user's own\n",
            "and it is left byte-for-byte as it was"
        );
    }

    /// The host and the Amiga must agree on one file name.
    ///
    /// Not `result_path(d) == d.join(RESULT_FILE)`, which only restates the
    /// implementation and cannot fail. This reads the name back out of the
    /// script's own redirections and compares it with the name the host will
    /// poll — so a script that redirected somewhere else, or a `result_path`
    /// that appended something, breaks it.
    ///
    /// **Every redirection is accounted for, not merely most of them.** The
    /// script writes to two names now — the host's result file three times
    /// (`started`, `failed`, `ok`) and the re-run guard's marker once
    /// (ART-190) — and both must land on ART's own volume. A test that only
    /// looked at the ones it expected would not notice a fourth appearing
    /// somewhere else, which on this volume is the difference between ART
    /// reading its own answer and reading nothing.
    #[test]
    fn the_host_polls_the_name_the_script_redirects_to() {
        let ss = startup_sequence(&planned("C:Updater")).unwrap();

        let redirected: Vec<&str> = ss
            .split('>')
            .skip(1)
            .map(|rest| rest.split_whitespace().next().unwrap_or(""))
            .collect();

        let polled = result_path(Path::new("host-side"));
        let polled = format!(
            "{WORK_VOLUME}:{}",
            polled.file_name().unwrap().to_string_lossy()
        );
        let marker = format!("{WORK_VOLUME}:{INVOKED_FILE}");

        assert_eq!(
            redirected.iter().filter(|t| **t == polled).count(),
            3,
            "started, failed and ok:\n{ss}"
        );
        assert_eq!(
            redirected.iter().filter(|t| **t == marker).count(),
            1,
            "the invoked marker, once:\n{ss}"
        );
        assert_eq!(
            redirected.len(),
            4,
            "every redirection must be one of those two:\n{ss}"
        );
    }
}
