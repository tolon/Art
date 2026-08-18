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

/// Build the startup-sequence text that assigns from `system_volume` and runs
/// `slave` out of `game_volume`.
///
/// This text becomes commands an Amiga executes, and `slave` comes from a
/// file somebody else made — untrusted input, exactly like an archive entry
/// name. A name containing a control character, `"` or `*` (AmigaDOS's
/// escape character) is refused with [`CoreError::InvalidInput`] naming the
/// slave, rather than sanitised: a launcher that silently rewrites what it
/// was told to run is not one to trust with a startup-sequence.
///
/// The assigns come before the `CD` and the `WHDLoad` line, because AmigaDOS
/// resolves `C:`, `LIBS:` and `DEVS:` for everything that follows — the
/// `WHDLoad` command itself included.
pub fn startup_sequence(slave: &str, system_volume: &str, game_volume: &str) -> CoreResult<String> {
    if slave
        .chars()
        .any(|c| c.is_control() || c == '"' || c == '*')
    {
        return Err(CoreError::InvalidInput(format!(
            "'{slave}' is not a valid WHDLoad slave name"
        )));
    }

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

    #[test]
    fn the_startup_sequence_assigns_from_the_system_and_runs_the_slave() {
        let text = startup_sequence("Turrican.slave", "DH0", "DH1").unwrap();

        assert!(text.contains("Assign C: DH0:C"), "{text}");
        assert!(text.contains("Assign LIBS: DH0:Libs"), "{text}");
        assert!(text.contains("Assign DEVS: DH0:Devs"), "{text}");
        assert!(text.contains("CD DH1:"), "{text}");
        assert!(text.contains("WHDLoad Turrican.slave"), "{text}");
    }

    /// A slave's name comes out of a file somebody else made, and this text
    /// becomes commands an Amiga executes.
    #[test]
    fn a_slave_name_that_could_add_a_command_is_refused() {
        assert!(startup_sequence("Turrican.slave\nDelete DH0:#?", "DH0", "DH1").is_err());
        assert!(startup_sequence("Turrican.slave\rFormat", "DH0", "DH1").is_err());
    }

    #[test]
    fn the_boot_directory_is_written_where_art_owns_it() {
        let dir = scratch("boot");
        let written = write_boot_dir(&dir, "Turrican.slave", "DH0", "DH1").unwrap();

        assert!(written.ends_with("Startup-Sequence"));
        assert!(dir.join("S").join("Startup-Sequence").is_file());
        let text = std::fs::read_to_string(dir.join("S").join("Startup-Sequence")).unwrap();
        assert!(text.contains("WHDLoad Turrican.slave"));

        let _ = std::fs::remove_dir_all(&dir);
    }
}
