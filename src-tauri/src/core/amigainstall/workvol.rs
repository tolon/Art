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

use super::{PlannedRun, MARK_FAILED, MARK_OK, MARK_STARTED, RESULT_FILE, WORK_VOLUME};
use crate::core::error::{CoreError, CoreResult};
use crate::core::safety::atomic_write;
use crate::core::security::refuse_shell_metacharacters;

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
/// It is written before the assigns as well as before the installer, and that
/// ordering is not cosmetic: **ART-118 was exactly a line that could not run**
/// — AmigaDOS auto-assigns `C:` only when a `C` drawer exists on the boot
/// volume, ART's own volume has none, and `Assign` itself lives in `C:`, so
/// the script's first `Assign` failed and dropped the user at a CLI. Every
/// `Assign` below is therefore something that might not return, and the rule
/// the research measured — *write the result before anything that might not
/// return* — puts the marker above all of them.
///
/// ## Why `FailAt 21`
///
/// AmigaDOS aborts a script when a command's return code reaches `FailAt`,
/// which defaults to 10. An installer that returns `FAIL` (20) would then end
/// the script *before* the branch below could record anything, and the host
/// would see a file saying only `started` — a failure wearing a hang's
/// clothes. Raising the threshold above 20 keeps the script alive long enough
/// to say which of the two it was, which is the whole reason the branch
/// exists.
///
/// ## Why the run refuses to repeat itself
///
/// Some installers reboot the Amiga when they are done. A reboot re-runs this
/// script, and a second pass would run the installer again over a tree it has
/// already changed. `If EXISTS` on the result file makes the second pass do
/// nothing and leave the first pass's answer alone. When that answer is only
/// `started`, the host times out — which is honest: an installer that rebooted
/// before recording anything has not been *observed* to succeed, and §89 does
/// not allow ART to say it did.
///
/// ## Why the assigns are here at all
///
/// ART's volume booted, so `SYS:` is ART's volume and the tree's commands,
/// libraries and devices are not reachable by name. The first line invokes
/// `Assign` by an explicit path on the tree — the one line here that is
/// certain, because it is ART-118's actual blocker and nothing after it can
/// run without it. The rest of the set is reasoned rather than measured, the
/// same standing as its counterpart in
/// [`crate::core::launch::whdload_boot::startup_sequence`]: `T:` is included
/// because the Amiga `Installer` writes temporary files there. What a real
/// `Updater` actually needs is a thing to measure against the owner's own
/// packages, not to assert here.
pub fn startup_sequence(run: &PlannedRun) -> CoreResult<String> {
    refuse_shell_metacharacters("package id", &run.package_id)?;
    refuse_shell_metacharacters("system volume name", &run.system_volume)?;
    refuse_shell_metacharacters("installer path", &run.program)?;
    for arg in &run.args {
        refuse_shell_metacharacters("installer argument", arg)?;
    }
    if run.program.trim().is_empty() {
        return Err(CoreError::InvalidInput(
            "an Amiga-side install needs a program to run".into(),
        ));
    }

    // Each argument is validated on its own above, so joining them here cannot
    // reintroduce a separator that was refused individually.
    let mut command = run.program.clone();
    for arg in &run.args {
        command.push(' ');
        command.push_str(arg);
    }

    let sys = &run.system_volume;
    let work = WORK_VOLUME;
    let result = RESULT_FILE;
    let package = &run.package_id;

    Ok(format!(
        "; Written by ART to install '{package}'. One run, then a result.\n\
         FailAt 21\n\
         If EXISTS {work}:{result}\n\
         \x20 Echo \"ART: this install already ran. Not repeating it.\"\n\
         Else\n\
         \x20 Echo >{work}:{result} \"{MARK_STARTED}\"\n\
         \x20 {sys}:C/Assign C: {sys}:C\n\
         \x20 Assign SYS: {sys}:\n\
         \x20 Assign S: {sys}:S\n\
         \x20 Assign L: {sys}:L\n\
         \x20 Assign LIBS: {sys}:Libs\n\
         \x20 Assign DEVS: {sys}:Devs\n\
         \x20 Assign FONTS: {sys}:Fonts\n\
         \x20 Assign T: RAM:\n\
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
/// `at` must be a directory ART owns and nothing else has put anything in.
/// The refusal below is not tidiness: `build` writes a `Startup-Sequence`, and
/// a mistyped path that happened to point at the user's distribution tree
/// would overwrite theirs. Refusing anything with content in it means the only
/// directory this can write into is one that was just created for it.
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

    fn scratch(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "art-amigainstall-{tag}-{}",
            crate::core::test_scratch_id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
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

    fn planned(command: &str) -> PlannedRun {
        PlannedRun {
            package_id: "test-pack".to_string(),
            system_volume: "DH0".to_string(),
            program: command.to_string(),
            args: Vec::new(),
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
            "FailAt 21",
            "If EXISTS ARTWork:art-result.txt",
            "  Echo \"ART: this install already ran. Not repeating it.\"",
            "Else",
            "  Echo >ARTWork:art-result.txt \"started\"",
            "  DH0:C/Assign C: DH0:C",
            "  Assign SYS: DH0:",
            "  Assign S: DH0:S",
            "  Assign L: DH0:L",
            "  Assign LIBS: DH0:Libs",
            "  Assign DEVS: DH0:Devs",
            "  Assign FONTS: DH0:Fonts",
            "  Assign T: RAM:",
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

    /// The volume boots ART's script, not the user's system.
    #[test]
    fn the_work_volume_carries_its_own_startup_sequence() {
        let dir = scratch("workvol-startup");
        build(&dir, &planned("C:Updater")).unwrap();
        let ss = std::fs::read_to_string(dir.join("S/Startup-Sequence")).unwrap();
        assert!(ss.contains("Updater"), "got {ss}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The result is written **before** the installer runs, then again after,
    /// so a run that never returns is still distinguishable from one that was
    /// never started. A hang and a crash look identical otherwise.
    #[test]
    fn a_started_run_is_marked_before_the_installer_is_invoked() {
        let dir = scratch("workvol-order");
        build(&dir, &planned("C:Updater")).unwrap();
        let ss = std::fs::read_to_string(dir.join("S/Startup-Sequence")).unwrap();
        let started = ss.find(MARK_STARTED).expect("a started marker");
        let invoke = ss.find("Updater").expect("the installer");
        assert!(started < invoke, "the marker must be written first:\n{ss}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The marker also precedes every `Assign`, because ART-118 was exactly an
    /// `Assign` that could not run — so those lines are among the things that
    /// might not return, not preliminaries that always succeed.
    #[test]
    fn the_started_marker_precedes_the_assigns_as_well() {
        let ss = startup_sequence(&planned("PKG:C/Updater")).unwrap();
        let started = ss.find(MARK_STARTED).expect("a started marker");
        let first_assign = ss.find("Assign").expect("the assigns");
        assert!(started < first_assign, "got:\n{ss}");
    }

    /// The script writes an outcome whether the installer succeeded or not.
    /// Without this a failure and a hang are the same silence.
    #[test]
    fn the_script_records_an_outcome_on_both_paths() {
        let dir = scratch("workvol-both");
        build(&dir, &planned("C:Updater")).unwrap();
        let ss = std::fs::read_to_string(dir.join("S/Startup-Sequence")).unwrap();
        assert!(
            ss.to_lowercase().contains("if warn") || ss.to_lowercase().contains("if fail"),
            "the script must branch on the installer's return code:\n{ss}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Branching is not enough on its own: AmigaDOS aborts a script when a
    /// return code reaches `FailAt`, which defaults to 10, so an installer
    /// returning `FAIL` (20) would end the run before the branch could record
    /// anything — and the host would read a hang where there was a refusal.
    #[test]
    fn a_failing_installer_cannot_abort_the_script_before_it_reports() {
        let ss = startup_sequence(&planned("PKG:C/Updater")).unwrap();
        let failat = ss.find("FailAt 21").expect("the script must raise FailAt");
        let invoke = ss.find("PKG:C/Updater").expect("the installer");
        assert!(
            failat < invoke,
            "FailAt must be raised before the installer runs:\n{ss}"
        );
    }

    /// Some installers reboot when they are done, and a reboot re-runs this
    /// script. A second pass must not install over a tree the first pass
    /// already changed.
    #[test]
    fn a_second_boot_does_not_run_the_installer_again() {
        let ss = startup_sequence(&planned("PKG:C/Updater")).unwrap();
        let guard = ss
            .find(&format!("If EXISTS {WORK_VOLUME}:{RESULT_FILE}"))
            .expect("a guard on the result file");
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
                build(&dir, &planned(hostile)).is_err(),
                "{hostile} should be refused"
            );
        }
        assert!(
            walk_relative(&dir).is_empty(),
            "a refused run writes nothing"
        );
        let _ = std::fs::remove_dir_all(&dir);
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
        build(&dir, &planned("C:Updater")).unwrap();
        let mut found: Vec<String> = walk_relative(&dir);
        found.sort();
        assert_eq!(found, vec!["S/Startup-Sequence".to_string()]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A mistyped destination must not be able to overwrite somebody's real
    /// `Startup-Sequence` — the tree ART is installing *into* has one.
    #[test]
    fn building_into_a_directory_that_already_has_contents_is_refused() {
        let dir = scratch("workvol-occupied");
        std::fs::create_dir_all(dir.join("S")).unwrap();
        std::fs::write(dir.join("S/Startup-Sequence"), b"the user's own\n").unwrap();

        assert!(build(&dir, &planned("C:Updater")).is_err());
        assert_eq!(
            std::fs::read(dir.join("S/Startup-Sequence")).unwrap(),
            b"the user's own\n",
            "and it is left byte-for-byte as it was"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The host and the Amiga must agree on one path for the result.
    #[test]
    fn the_result_path_is_the_file_the_script_writes() {
        let dir = scratch("workvol-result");
        let ss = startup_sequence(&planned("C:Updater")).unwrap();
        assert!(ss.contains(&format!("{WORK_VOLUME}:{RESULT_FILE}")));
        assert_eq!(result_path(&dir), dir.join(RESULT_FILE));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
