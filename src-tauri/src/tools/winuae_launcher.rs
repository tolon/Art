//! `winuae64.exe` as ART's [`EmulatorLauncher`] (ART-274).
//!
//! The implementation of [`crate::core::amigainstall::run::EmulatorLauncher`].
//! It lives here rather than in `core/` for the same reason
//! `tools/hst_imager.rs` does: it launches a real process
//! (CLAUDE.md, "The core independence rule"). Before this module existed,
//! `core/winuae.rs` called `std::process::Command::new` directly — the one
//! process spawn inside `core/`, filed as ART-274 and closed by this move.
//!
//! ## Structured argv, never a shell string
//!
//! `core/security`'s rule, same as every other tool ART drives: the
//! generated `.uae` config's path is passed as a single `-f` argument, never
//! concatenated into a command line.

use std::path::{Path, PathBuf};

use crate::core::amigainstall::run::{EmulatorLauncher, EmulatorSession};
use crate::core::error::{CoreError, CoreResult};

/// A WinUAE process **ART started**, and therefore the only one ART may end.
///
/// It exists because an Amiga-side install has to be able to stop the emulator
/// when its deadline expires, and a bare pid is not enough to do that safely.
/// Terminating by pid alone means asking the operating system to kill a number,
/// and a number can be reused: between the moment ART reads a pid and the
/// moment it acts on it, the process can exit and the pid be handed to
/// something else. Holding the [`std::process::Child`] holds the OS handle
/// too, so [`terminate`](Self::terminate) can only ever reach the process this
/// struct was created from — never a stranger that inherited its number, and
/// never a WinUAE the owner started themselves.
#[derive(Debug)]
pub struct WinUaeProcess {
    child: std::process::Child,
}

impl WinUaeProcess {
    /// The process id, for reporting. Never terminate by this — see the type's
    /// own documentation for why the handle is what does that.
    pub fn pid(&self) -> u32 {
        self.child.id()
    }

    /// Whether the process is still alive.
    ///
    /// A poll rather than a wait: an Amiga-side install must keep reading the
    /// result file while the emulator runs, so it can never afford to block on
    /// the process.
    pub fn is_running(&mut self) -> CoreResult<bool> {
        Ok(self.child.try_wait().map_err(CoreError::Io)?.is_none())
    }

    /// End the process, and reap it.
    ///
    /// Idempotent on purpose: a run terminates the emulator on every ending it
    /// has, including the ones where the emulator has already gone, so "it was
    /// not there" is a success rather than something to report.
    pub fn terminate(&mut self) -> CoreResult<()> {
        if !self.is_running()? {
            return Ok(());
        }
        self.child.kill().map_err(CoreError::Io)?;
        self.child.wait().map_err(CoreError::Io)?;
        Ok(())
    }

    /// Give up ownership: the process keeps running and ART can no longer end
    /// it. This is what a fire-and-forget launch is, stated as a deliberate
    /// act rather than left implicit in a dropped handle.
    pub fn release(self) -> u32 {
        self.child.id()
    }
}

/// Launch WinUAE with a generated configuration, keeping the process.
///
/// The sibling of [`launch_winuae`], which throws the handle away. Anything
/// that has to be able to *stop* the emulator later must use this one.
pub fn launch_winuae_process(
    winuae_path: &Path,
    config_text: &str,
    scratch_root: &Path,
) -> CoreResult<WinUaeProcess> {
    launch_winuae_inner(winuae_path, config_text, scratch_root).map(|child| WinUaeProcess { child })
}

/// Launch WinUAE with a generated configuration.
///
/// `scratch_root` is where the generated `.uae` is written — the caller's
/// answer, never this module's (ART-196). See `crate::scratch`.
pub fn launch_winuae(
    winuae_path: &Path,
    config_text: &str,
    scratch_root: &Path,
) -> CoreResult<u32> {
    Ok(launch_winuae_process(winuae_path, config_text, scratch_root)?.release())
}

fn launch_winuae_inner(
    winuae_path: &Path,
    config_text: &str,
    scratch_root: &Path,
) -> CoreResult<std::process::Child> {
    if !winuae_path.is_file() {
        return Err(CoreError::InvalidInput(format!(
            "WinUAE executable not found at '{}'",
            winuae_path.display()
        )));
    }

    // A unique name per launch: a fixed filename means two sessions started
    // close together overwrite each other's configuration — and the stamp
    // alone does not give one. Two launches can share a nanosecond, which is
    // the same defect ART-164 and ART-173 were filed for in test fixtures;
    // this is its production instance, on the path an Amiga-side install run
    // uses repeatedly. The counter is what makes the name unique; the stamp
    // only makes it readable.
    static NEXT_LAUNCH: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let seq = NEXT_LAUNCH.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let temp_config_path = scratch_root.join(format!("art_launch_{stamp}_{seq}.uae"));
    std::fs::write(&temp_config_path, config_text)?;

    // Arguments are passed as a structured argv, never through a shell, so
    // paths containing spaces or shell metacharacters cannot be reinterpreted
    // as commands (spec §56).
    let child = std::process::Command::new(winuae_path)
        .arg("-f")
        .arg(&temp_config_path)
        .spawn()
        .map_err(CoreError::Io)?;

    Ok(child)
}

/// The real [`EmulatorLauncher`]: WinUAE, at the path the user configured.
#[derive(Debug, Clone)]
pub struct WinUaeLauncher {
    executable: PathBuf,
    /// Where the generated `.uae` is written (ART-196). Carried rather than
    /// asked of the platform, for the same reason every other staging site in
    /// ART now carries it: the user's system drive is theirs.
    scratch_root: PathBuf,
}

impl WinUaeLauncher {
    pub fn new(executable: impl Into<PathBuf>, scratch_root: impl Into<PathBuf>) -> Self {
        Self {
            executable: executable.into(),
            scratch_root: scratch_root.into(),
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
            &self.scratch_root,
        )?)))
    }
}

#[cfg(test)]
mod real_boot_hook {
    //! Boot a real distribution tree under WinUAE — the bar this project
    //! sets for a recipe and the one thing its own tests cannot reach.
    //!
    //! A 3.2 tree was proved by booting AmigaOS to a clean Workbench with
    //! the owner's licensed ROM; §89 says a tree that has not booted is not
    //! a tree ART may call working. This hook builds the config ART itself
    //! would write — `generate_uae_config`, not a hand-typed `.uae` — mounts
    //! the tree as a `filesystem2=` directory volume, and launches. What
    //! happens on screen is the finding.
    //!
    //! Gated and `#[ignore]`d: it needs the user's own tree, their own
    //! licensed Kickstart, and an installed WinUAE, none of which exist in
    //! CI. Nothing here is written to the repository.

    use super::*;
    use crate::core::profile::AmigaProfile;
    use crate::core::winuae::{generate_uae_config, DirMount, LaunchMedia};
    use std::path::PathBuf;

    #[test]
    #[ignore = "needs a real tree, a licensed ROM and WinUAE; run explicitly"]
    fn boot_a_distribution_tree_when_asked() {
        let (Ok(tree), Ok(rom)) = (
            std::env::var("ART_BOOT_TREE"),
            std::env::var("ART_BOOT_ROM"),
        ) else {
            return;
        };

        let profile = AmigaProfile::a1200_aga();
        let media = LaunchMedia {
            floppy_paths: Vec::new(),
            hardfile_paths: Vec::new(),
            hardfile_shapes: Vec::new(),
            kickstart_path: Some(rom.clone()),
            use_aros: false,
            write_protect_hardfiles: false,
            directories: vec![DirMount {
                host_path: tree.clone(),
                volume: "DH0".into(),
                // The label AmigaOS itself expects for a system volume. A
                // startup-sequence assigns against `SYS:`, which WinUAE binds
                // to whatever device it booted from, but tools and icons
                // written by the install refer to the label.
                label: "Workbench".into(),
                boot_priority: 0,
                read_only: false,
            }],
            cd_image_path: None,
        };

        let config = generate_uae_config(&profile, &media).expect("ART must be able to write this");
        let out = PathBuf::from(&tree).join("..").join("art-boot-39.uae");
        std::fs::write(&out, &config).unwrap();
        println!("config written to {}", out.display());
        println!("--- config ---\n{config}\n--- end ---");

        if let Ok(winuae) = std::env::var("ART_WINUAE") {
            // ART-281: the launcher's scratch root here is the folder beside
            // the tree this hook already wrote `art-boot-39.uae` into two
            // lines up — **not** a `ScratchDir` and not the platform root.
            // `launch_winuae` `release`s the child: WinUAE outlives this test
            // and opens the generated `.uae` after the function has returned,
            // so a guarded root would be removed between the spawn and that
            // read, and the emulator would fail to start for a reason that
            // has nothing to do with what is being measured. The directory
            // above is the owner's own, proven writable by the `write` on the
            // line before, and it keeps the launcher's copy where the copy
            // this hook prints already is. `ask_a_tree_its_version_when_asked`
            // below does take a guarded root, because it `terminate()`s the
            // process before it returns.
            let uae_root = PathBuf::from(&tree).join("..");
            let pid = launch_winuae(&PathBuf::from(&winuae), &config, &uae_root)
                .expect("WinUAE must start");
            println!("WinUAE started, pid {pid}");
        }
    }
}

#[cfg(test)]
mod real_version_hook {
    //! **Ask a booted tree what it is, and read the answer on the host.**
    //!
    //! The bar the Amiga-side install round sets for itself is that a
    //! BoingBag'd tree *boots and shows its update* — and the method that
    //! found this project's biggest defect applies: **ask the running system,
    //! do not infer.** This project once shipped an AmigaOS **3.5** tree under
    //! the name 3.9 because it booted cleanly and a copyright line was read as
    //! proof. A directory name, a file size and a copyright line are each
    //! consistent with several answers; `Version FULL` is not.
    //!
    //! It does not interrupt the tree's own `Startup-Sequence` to reach a
    //! shell — a healthy tree resists interruption by design, which is itself
    //! a thing this project learned by measuring. Instead it uses the same
    //! mechanism [`crate::core::amigainstall`] uses for a run: ART's own work
    //! volume, mounted at the highest boot priority, carrying one script ART
    //! wrote. **The tree is mounted as data and is never written to.**
    //!
    //! The script asks four questions and redirects the answers to ART's own
    //! volume, where the host reads them:
    //!
    //! - `Version FULL` — the running system's Kickstart and Workbench.
    //! - `Version version.library FULL` — the library `Version` reports the
    //!   Workbench number *from*, so the two can be compared rather than one
    //!   being taken on trust.
    //! - `Version workbench.library FULL`.
    //! - `Version resource.library FULL` — because a real `Updater` failed
    //!   against a real tree with `Cannot open "resource.library", version
    //!   44.` (2026-08-21), and the difference between *the library is not
    //!   there* and *the library is there and will not open* is the difference
    //!   between two entirely different fixes.
    //!
    //! Gated and `#[ignore]`d, like every hook of this shape: it needs the
    //! user's own tree, their own licensed Kickstart and an installed WinUAE,
    //! none of which exist in CI.
    //!
    //! ```text
    //!   ART_BOOT_TREE=E:\amiga\ProjeART\bb-run\p2 ^
    //!   ART_BOOT_ROM="E:\...\Kickstart v3.1 rev 40.68 (1993)(Commodore)(A1200).rom" ^
    //!   ART_WINUAE="C:\Program Files\WinUAE\winuae64.exe" ^
    //!   cargo test ask_a_tree_its_version_when_asked -- --ignored --nocapture
    //! ```

    use super::*;
    use crate::core::amigainstall::WORK_VOLUME;
    use crate::core::profile::AmigaProfile;
    use crate::core::winuae::{generate_uae_config, DirMount, LaunchMedia};
    use std::path::PathBuf;
    use std::time::{Duration, Instant};

    /// What the Amiga writes and the host reads.
    const ANSWER: &str = "art-version.txt";
    /// Written last, so the host never reads a half-written answer.
    const DONE: &str = "art-version-done.txt";

    /// The script, which is entirely ART's own text.
    ///
    /// `SetPatch` is here for the same reason it is in a run's script: a tree
    /// carrying `Devs/AmigaOS ROM Update` is a tree whose disk libraries
    /// expect it, and `SetPatch` **resets the machine** after loading it — so
    /// the guard below is on a marker written *after* that line, or the second
    /// pass would answer nothing.
    ///
    /// **The environment it builds deliberately mirrors
    /// [`crate::core::amigainstall::workvol::startup_sequence`]'s** — the same
    /// assigns, the same `LIBS: … Classes ADD`, the same `ENV:`, the same
    /// `SetPatch`. An instrument that set up a *different* machine from the one
    /// the product sets up would answer questions about a machine nobody runs,
    /// and on 2026-08-21 a diagnostic run from this hook did exactly that: it
    /// had no `ENV:` while the product script did, so what it measured was the
    /// requester ART-192 had already fixed, not the question being asked. Keep
    /// the two in step — `AddDataTypes` (ART-193) is here for that reason and
    /// no other, since a `Version` needs no datatypes at all.
    fn probe_script(volume: &str, extra: &str) -> String {
        format!(
            "; Written by ART to ask a tree what it is. It writes nothing to the tree.\n\
             {volume}:C/Assign C: {volume}:C\n\
             FailAt 2000000000\n\
             If EXISTS {WORK_VOLUME}:{DONE}\n\
             \x20 Echo \"ART: already asked.\"\n\
             Else\n\
             \x20 Assign SYS: {volume}:\n\
             \x20 Assign S: {volume}:S\n\
             \x20 Assign L: {volume}:L\n\
             \x20 Assign LIBS: {volume}:Libs\n\
             \x20 Assign LIBS: {volume}:Classes ADD\n\
             \x20 Assign DEVS: {volume}:Devs\n\
             \x20 Assign FONTS: {volume}:Fonts\n\
             \x20 MakeDir RAM:T RAM:Clipboards RAM:ENV RAM:ENV/Sys\n\
             \x20 Assign T: RAM:T\n\
             \x20 Assign CLIPS: RAM:Clipboards\n\
             \x20 Assign ENV: RAM:ENV\n\
             \x20 Copy ENVARC: RAM:ENV ALL QUIET NOREQ\n\
             \x20 If EXISTS {volume}:C/SetPatch\n\
             \x20   {volume}:C/SetPatch QUIET\n\
             \x20 EndIf\n\
             \x20 If EXISTS {volume}:C/AddDataTypes\n\
             \x20   {volume}:C/AddDataTypes REFRESH QUIET\n\
             \x20 EndIf\n\
             \x20 Version >{WORK_VOLUME}:{ANSWER} FULL\n\
             \x20 Version >>{WORK_VOLUME}:{ANSWER} version.library FULL\n\
             \x20 Version >>{WORK_VOLUME}:{ANSWER} workbench.library FULL\n\
             \x20 Version >>{WORK_VOLUME}:{ANSWER} resource.library FULL\n\
             \x20 Version >>{WORK_VOLUME}:{ANSWER} FILE {volume}:Libs/resource.library FULL\n\
             {extra}\
             \x20 Echo >{WORK_VOLUME}:{DONE} \"done\"\n\
             EndIf\n"
        )
    }

    /// Extra AmigaDOS lines for one investigation, from `ART_BOOT_PROBE`,
    /// `;`-separated and indented into the script's own arm.
    ///
    /// **The instrument stays general.** Today's question is why a real
    /// `Updater` could not open `resource.library`; tomorrow's will be a
    /// different one, and a hook that could only ask today's gets rewritten
    /// every time — which is how a diagnostic stops being re-runnable, and the
    /// next reader ends up re-trusting a report instead of repeating it.
    ///
    /// This is the **one** place in ART where a generated AmigaDOS line is not
    /// ART's own text, and it is `#[cfg(test)]`, `#[ignore]`d, and gated on an
    /// environment variable that exists only on the machine of the person
    /// typing it. Nothing the product ships can reach it. The product's rule —
    /// that a script ART generates is assembled only from strings ART authored,
    /// enforced by [`crate::core::security::refuse_shell_metacharacters`] — is
    /// unchanged, and is exactly why this can live nowhere but a hook.
    fn extra_probes() -> String {
        std::env::var("ART_BOOT_PROBE")
            .unwrap_or_default()
            .split(';')
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .map(|line| format!("  {line}\n"))
            .collect()
    }

    #[test]
    #[ignore = "opens WinUAE against the owner's own tree and ROM; run explicitly"]
    fn ask_a_tree_its_version_when_asked() {
        let (_root_guard, root) = crate::core::ScratchDir::pair("art-winuae-root", "ask-version");
        let (Ok(tree), Ok(rom), Ok(winuae)) = (
            std::env::var("ART_BOOT_TREE"),
            std::env::var("ART_BOOT_ROM"),
            std::env::var("ART_WINUAE"),
        ) else {
            return;
        };

        let work = crate::core::ScratchDir::new("art-version", "probe");
        std::fs::create_dir_all(work.join("S")).unwrap();
        let script = probe_script("DH0", &extra_probes());
        println!("--- the script ---\n{script}--- end ---");
        std::fs::write(work.join("S/Startup-Sequence"), &script).unwrap();

        let mut directories = vec![
            DirMount {
                host_path: tree.clone(),
                volume: "DH0".into(),
                label: "Workbench".into(),
                boot_priority: 0,
                read_only: false,
            },
            DirMount {
                host_path: work.path().to_string_lossy().to_string(),
                volume: "DH9".into(),
                label: WORK_VOLUME.into(),
                boot_priority: 10,
                read_only: false,
            },
        ];
        // An unpacked package, when the question being asked is about one —
        // `ART_BOOT_PKG`, mounted as `DH8:` and never the boot device. The
        // instrument needs it to be able to ask *why* a real installer
        // refused, which is a question about the installer and the tree
        // together and cannot be asked of either alone.
        if let Ok(package) = std::env::var("ART_BOOT_PKG") {
            directories.push(DirMount {
                host_path: package,
                volume: "DH8".into(),
                label: "ARTPkg".into(),
                boot_priority: -1,
                read_only: false,
            });
        }

        // A disc in the emulated CD drive, when the question being asked is
        // about one — `ART_BOOT_CD`. ART-193's whole diagnosis is that a
        // package's installer verifies a medium ART did not mount, and
        // "does the Amiga see it, and under what name" is a question about a
        // running machine that no host-side reading can answer.
        let media = LaunchMedia {
            kickstart_path: Some(rom),
            directories,
            cd_image_path: std::env::var("ART_BOOT_CD").ok(),
            ..LaunchMedia::default()
        };

        let config = generate_uae_config(&AmigaProfile::a1200_aga(), &media).unwrap();
        let mut process = launch_winuae_process(&PathBuf::from(&winuae), &config, &root).unwrap();
        println!("WinUAE pid {}", process.pid());

        let started = Instant::now();
        let done = work.join(DONE);
        // Three minutes answers a `Version`; a probe that runs a real
        // installer needs longer, and re-editing the constant per question is
        // how an instrument stops being re-runnable. `ART_BOOT_DEADLINE` is
        // in seconds.
        let deadline = Duration::from_secs(
            std::env::var("ART_BOOT_DEADLINE")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(180),
        );
        while started.elapsed() < deadline && !done.is_file() {
            std::thread::sleep(Duration::from_millis(500));
            if !process.is_running().unwrap_or(false) {
                break;
            }
        }
        let waited = started.elapsed();
        let _ = process.terminate();

        // Read as **bytes**, not as a `String`. The Amiga writes Latin-1, and
        // a single high-bit byte anywhere in the answer — a `List` of a
        // drawer with an accented name, a `Status` line — made
        // `read_to_string` fail and threw away an entire measured run
        // (2026-08-21). An instrument that discards its own answer over an
        // encoding detail is worse than one that shows a replacement
        // character.
        match std::fs::read(work.join(ANSWER)) {
            Ok(bytes) => println!(
                "--- the tree's own answer, after {waited:.1?} ---\n{}",
                String::from_utf8_lossy(&bytes)
            ),
            Err(err) => println!("no answer after {waited:.1?}: {err}"),
        }
    }
}
