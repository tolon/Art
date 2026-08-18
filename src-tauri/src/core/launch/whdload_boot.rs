//! The five lines that make Y2 one click (wave C).
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

use std::path::{Path, PathBuf};

use crate::core::error::{CoreError, CoreResult};
use crate::core::safety::atomic_write;

/// Refuse a value that would change what the generated AmigaShell command
/// line does, rather than merely what it names.
///
/// Every one of `slave`, `system_volume` and `game_volume` is interpolated
/// straight into the script `startup_sequence` builds, so any of the three
/// can carry an attack — not only the slave name. The set refused:
///
/// - a control character (`\n` or `\r` starts a new script line — a name
///   ending `...slave\nDelete DH0:#?` adds a command of its own);
/// - `"` (opens a quoted string, changing where the current one ends);
/// - `*` — AmigaDOS's escape character, which cancels the special meaning of
///   whatever follows it;
/// - `;` — separates multiple commands on one AmigaShell line;
/// - `>` and `<` — redirect a command's output or input. `>` is how
///   `Turrican.slave >DH1:C/something` turns `WHDLoad`'s own command line
///   into a redirection that overwrites an arbitrary file on the game
///   volume, which is mounted **writable on purpose** so WHDLoad can keep
///   save games there. `>>` (append) is already refused because it contains
///   `>`.
///
/// Considered and not added: AmigaDOS's pattern-matching wildcards (`#?`,
/// `%`, `(a|b)`) are interpreted per-command by whichever program chooses to
/// treat its own argument as a pattern — unlike a Unix shell, AmigaDOS does
/// not expand them while parsing the command line, so they cannot change
/// *which* command runs or *where its output goes*, only how one already-
/// chosen command might later read its own argument. Backtick command
/// substitution and `$VAR` environment expansion do not exist in the stock
/// AmigaDOS command-line parser, so there is nothing there to refuse either.
fn refuse_shell_metacharacters(label: &str, value: &str) -> CoreResult<()> {
    if value
        .chars()
        .any(|c| c.is_control() || matches!(c, '"' | '*' | ';' | '>' | '<'))
    {
        return Err(CoreError::InvalidInput(format!(
            "'{value}' is not a valid {label}"
        )));
    }
    Ok(())
}

/// Build the startup-sequence text that assigns from `system_volume` and runs
/// `slave` out of `game_volume`.
///
/// This text becomes commands an Amiga executes. All three inputs can come
/// from data ART did not write itself — `slave` out of a file somebody else
/// made, `system_volume` and `game_volume` out of whatever mounted the
/// title's drawer and the user's system — so all three go through
/// [`refuse_shell_metacharacters`] before anything is formatted, rather than
/// being sanitised: a launcher that silently rewrites what it was told to
/// run is not one to trust with a startup-sequence.
///
/// The assigns come before the `CD` and the `WHDLoad` line, because AmigaDOS
/// resolves `C:`, `LIBS:` and `DEVS:` for everything that follows — the
/// `WHDLoad` command itself included.
pub fn startup_sequence(slave: &str, system_volume: &str, game_volume: &str) -> CoreResult<String> {
    refuse_shell_metacharacters("WHDLoad slave name", slave)?;
    refuse_shell_metacharacters("system volume name", system_volume)?;
    refuse_shell_metacharacters("game volume name", game_volume)?;

    Ok(format!(
        "Assign C: {system_volume}:C\n\
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
        let dir = std::env::temp_dir().join(format!("art-launch-{tag}-{}", std::process::id()));
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
            "Assign C: DH0:C\n\
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
            "Assign C: DH0:C\n\
             Assign LIBS: DH0:Libs\n\
             Assign DEVS: DH0:Devs\n\
             CD DH1:\n\
             WHDLoad Turrican.slave\n"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }
}
