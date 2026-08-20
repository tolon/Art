//! The lines that make Y2 one click (wave C).
//!
//! A WHDLoad slave is not a program WinUAE can run: it needs an Amiga that has
//! booted, with WHDLoad installed. ART owns no such system and does not build
//! one for this wave — the user points at their own. What ART owns is *this*:
//! a boot directory of its own, mounted at the highest boot priority, whose
//! startup-sequence assigns from the mounted system and runs the slave.
//!
//! **Nothing here is written to the user's files.** The only path this module
//! writes to is ART's own launch directory. The user's system image is
//! mounted read-only; their game drawer is mounted writable, because WHDLoad
//! keeps save games beside the game and a launcher that discards a saved
//! position is not one.
//!
//! Y1 — mount the system, boot to Workbench, let the user start the game — is
//! always one switch away on the panel, and is what this falls back to. That
//! pairing is the shape `commands/preload.rs::run_with_fallback` already uses:
//! the good path first, a named alternative behind it, never a silent one.
//!
//! ## ART-118 — found by running it, not by reading it
//!
//! The first real run (`1000 Miglia`, 2026-08-18) left the user at an
//! AmigaDOS CLI instead of the game. The script ART had written was never
//! wrong on paper — reading it did not find this — but AmigaOS boots from
//! ART's own directory (mounted at the highest boot priority), and that
//! directory has no `C` drawer. `SYS:` becomes ART's directory, and AmigaDOS
//! auto-assigns `C:` only when a `C` directory actually exists on the boot
//! volume — it does not on ART's, so `C:` is never assigned. `Assign` is
//! itself a command that lives in `C:`, so the script's own first line could
//! not run: AmigaDOS reported the command not found and dropped the user at
//! the CLI. [`startup_sequence`] now invokes that first `Assign` by an
//! explicit path on the mounted system volume instead of relying on `C:`,
//! which is the one line here that is certain: it is the actual blocker, and
//! nothing after it can run without it.
//!
//! The rest of what changed is reasoned rather than confirmed by that run:
//! ART's boot directory stands in for a system volume, so it should present
//! the assigns a real boot would, and it did not. `SYS:` and `S:` join the
//! existing `LIBS:` and `DEVS:` — `S:` in particular because WHDLoad reads
//! its preferences from `S:WHDLoad.prefs`, and with ART's own directory as
//! the boot volume, `S:` would otherwise silently resolve to ART's `S`
//! drawer and ignore the user's WHDLoad settings. Whether `1000 Miglia`
//! actually starts with this fix has not yet been tested against the real
//! AmiKit image — only the generated script is pinned here.

use std::path::{Path, PathBuf};

use crate::core::error::CoreResult;
use crate::core::safety::atomic_write;
use crate::core::security::refuse_shell_metacharacters;

/// Build the startup-sequence text that assigns from `system_volume` and runs
/// `slave` out of `game_volume`.
///
/// This text becomes commands an Amiga executes. All three inputs can come
/// from data ART did not write itself — `slave` out of a file somebody else
/// made, `system_volume` and `game_volume` out of whatever mounted the
/// title's drawer and the user's system — so all three go through
/// [`refuse_shell_metacharacters`] before anything is formatted, rather than
/// being sanitised: a launcher that silently rewrites what it was told to
/// run is not one to trust with a startup-sequence. That guard —
/// [`refuse_shell_metacharacters`], with the sources behind each refused
/// character — lives in [`crate::core::security::amigados`], because ART now
/// generates AmigaDOS scripts in more than one place and a guard kept in two
/// of them is a guard that will diverge.
///
/// The assigns come before the `CD` and the `WHDLoad` line, because AmigaDOS
/// resolves `C:`, `LIBS:` and `DEVS:` for everything that follows — the
/// `WHDLoad` command itself included.
///
/// The very first line is `Assign` invoked by its own path on
/// `system_volume`, not by name — see the module header's ART-118 note.
/// AmigaDOS auto-assigns `C:` only when a `C` directory exists on the boot
/// volume, and ART's boot directory has none, so plain `Assign C: ...` as
/// the first command cannot run: `Assign` itself lives in `C:`, which does
/// not exist yet. Once that one line has run, `C:` exists and every
/// following `Assign` resolves normally by name.
///
/// `SYS:` and `S:` are reasoned, not confirmed by the run that found this —
/// see the module header. `S:` matters specifically because WHDLoad reads
/// `S:WHDLoad.prefs`; without it, `S:` would resolve to ART's own `S`
/// drawer instead of the user's system, silently ignoring their WHDLoad
/// settings.
pub fn startup_sequence(slave: &str, system_volume: &str, game_volume: &str) -> CoreResult<String> {
    refuse_shell_metacharacters("WHDLoad slave name", slave)?;
    refuse_shell_metacharacters("system volume name", system_volume)?;
    refuse_shell_metacharacters("game volume name", game_volume)?;

    Ok(format!(
        "{system_volume}:C/Assign C: {system_volume}:C\n\
         Assign SYS: {system_volume}:\n\
         Assign S: {system_volume}:S\n\
         Assign LIBS: {system_volume}:Libs\n\
         Assign DEVS: {system_volume}:Devs\n\
         CD {game_volume}:\n\
         WHDLoad {slave}\n"
    ))
}

/// Write the boot directory ART owns for a one-click WHDLoad launch.
///
/// Creates `S/` under `into` and writes `S/Startup-Sequence` through
/// [`atomic_write`] — the only path this module ever writes to, and never the
/// user's system image or game drawer. Returns the path written.
pub fn write_boot_dir(
    into: &Path,
    slave: &str,
    system_volume: &str,
    game_volume: &str,
) -> CoreResult<PathBuf> {
    let text = startup_sequence(slave, system_volume, game_volume)?;

    let s_dir = into.join("S");
    std::fs::create_dir_all(&s_dir)?;

    let target = s_dir.join("Startup-Sequence");
    atomic_write(&target, text.as_bytes())?;

    Ok(target)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "art-launch-{tag}-{}",
            crate::core::test_scratch_id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// Pins the complete text, not a substring of it — a substring check
    /// would not notice the assigns landing *after* the `CD` and `WHDLoad`
    /// lines, which the module doc comment says must never happen because
    /// those lines depend on the assigns already being in effect. An exact
    /// match also catches a stray `\r` that a substring check would miss.
    #[test]
    fn the_startup_sequence_assigns_from_the_system_and_runs_the_slave() {
        let text = startup_sequence("Turrican.slave", "DH0", "DH1").unwrap();

        assert_eq!(
            text,
            "DH0:C/Assign C: DH0:C\n\
             Assign SYS: DH0:\n\
             Assign S: DH0:S\n\
             Assign LIBS: DH0:Libs\n\
             Assign DEVS: DH0:Devs\n\
             CD DH1:\n\
             WHDLoad Turrican.slave\n"
        );
    }

    /// A slave's name comes out of a file somebody else made, and this text
    /// becomes commands an Amiga executes.
    #[test]
    fn a_slave_name_that_could_add_a_command_is_refused() {
        assert!(startup_sequence("Turrican.slave\nDelete DH0:#?", "DH0", "DH1").is_err());
        assert!(startup_sequence("Turrican.slave\rFormat", "DH0", "DH1").is_err());
    }

    /// `;` separates commands on an AmigaShell line — a name carrying one
    /// would run whatever follows it as a second command.
    #[test]
    fn a_slave_name_with_a_command_separator_is_refused() {
        assert!(startup_sequence("Turrican.slave;Delete DH0:#?", "DH0", "DH1").is_err());
    }

    /// `>` redirects a command's output. `WHDLoad`'s own argument line can
    /// carry a redirection that overwrites an arbitrary file on the game
    /// volume — mounted writable on purpose, so WHDLoad can keep saves.
    #[test]
    fn a_slave_name_with_an_output_redirection_is_refused() {
        assert!(startup_sequence("Turrican.slave >DH1:C/something", "DH0", "DH1").is_err());
    }

    /// `<` redirects a command's input — the read side of the same hazard.
    #[test]
    fn a_slave_name_with_an_input_redirection_is_refused() {
        assert!(startup_sequence("Turrican.slave <DH1:secret", "DH0", "DH1").is_err());
    }

    /// Refused on a confirmed mechanism: the AmigaOS Manual's "AmigaDOS
    /// Using Scripts" chapter documents backtick command substitution as a
    /// real Shell feature, at the same command-line-parser level as
    /// `;`/`>`/`<`. See `refuse_shell_metacharacters`'s doc comment.
    #[test]
    fn a_slave_name_with_a_backtick_is_refused() {
        assert!(startup_sequence("Turrican.slave `Format DH0:`", "DH0", "DH1").is_err());
    }

    /// Refused on the same confirmed mechanism as the backtick, for `$name`
    /// expansion.
    #[test]
    fn a_slave_name_with_a_dollar_sign_is_refused() {
        assert!(startup_sequence("Turrican.slave $evil", "DH0", "DH1").is_err());
    }

    /// The slave name is not the only value that lands in the script —
    /// `system_volume` and `game_volume` are interpolated exactly the same
    /// way, and ART is no longer the only caller that supplies them.
    #[test]
    fn a_volume_name_with_a_shell_metacharacter_is_refused() {
        assert!(startup_sequence("Turrican.slave", "DH0;Format", "DH1").is_err());
        assert!(startup_sequence("Turrican.slave", "DH0", "DH1>evil").is_err());
    }

    #[test]
    fn the_boot_directory_is_written_where_art_owns_it() {
        let dir = scratch("boot");
        let written = write_boot_dir(&dir, "Turrican.slave", "DH0", "DH1").unwrap();

        assert!(written.ends_with("Startup-Sequence"));
        assert!(dir.join("S").join("Startup-Sequence").is_file());
        let text = std::fs::read_to_string(dir.join("S").join("Startup-Sequence")).unwrap();
        assert_eq!(
            text,
            "DH0:C/Assign C: DH0:C\n\
             Assign SYS: DH0:\n\
             Assign S: DH0:S\n\
             Assign LIBS: DH0:Libs\n\
             Assign DEVS: DH0:Devs\n\
             CD DH1:\n\
             WHDLoad Turrican.slave\n"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }
}
