//! System / app-info commands: health check and metadata.

use std::path::PathBuf;

use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct AppInfo {
    pub name: &'static str,
    pub version: &'static str,
    pub platform: &'static str,
}

/// Health-check command the frontend calls on startup to confirm the Rust
/// backend is alive.
#[tauri::command]
pub fn ping() -> String {
    "pong".to_string()
}

#[tauri::command]
pub fn app_info() -> AppInfo {
    AppInfo {
        name: "Amiga Retro Toolkit",
        version: env!("CARGO_PKG_VERSION"),
        platform: std::env::consts::OS,
    }
}

// ---------------------------------------------------------------------------
// Amiga Forever, found on this host (design § 3.5)
// ---------------------------------------------------------------------------

/// Where Cloanto's Amiga Forever keeps the shared material, when it is
/// installed here.
///
/// Both fields are `None` on a machine that does not have it, which is the
/// ordinary answer and never an error: nothing on the screen depends on it,
/// and a suggestion that cannot be made is simply not made.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AmigaForeverFolders {
    /// `%AMIGAFOREVERDATA%\Shared\adf`, when that folder is really there.
    pub adf: Option<String>,
    /// `%AMIGAFOREVERDATA%\Shared\rom`, likewise.
    pub rom: Option<String>,
}

/// The Amiga Forever folders on this machine, or `None`s.
///
/// **This is a `commands/` matter and cannot be a `core/` one.** It reads an
/// environment variable and asks the filesystem whether two folders exist —
/// both host questions, and `core::independence` is the rule that keeps them
/// out of the platform-independent half.
///
/// **The environment variable alone, never the registry.** Measured on the
/// owner's machine, 2026-09-08: `AMIGAFOREVERDATA=E:\amiga\`, with
/// `E:\amiga\Shared\adf` holding `amiga-os-*.adf` and `E:\amiga\Shared\rom`
/// holding the ROMs. The registry key exists too — under
/// `HKLM\SOFTWARE\WOW6432Node\Cloanto\Amiga Forever`, not the 64-bit hive —
/// and is deliberately not read: it is a second signal for the same fact, it
/// needs a Windows API this command does not otherwise want, and the variable
/// is what Amiga Forever itself sets for exactly this purpose.
///
/// **Each folder is checked on its own.** An installation with ADFs and no
/// ROMs answers with one path and one `None`, rather than nothing — the same
/// per-entry rule `core/hostfs.rs` states for anything the host answers
/// unevenly.
///
/// Read-only in the strongest sense: it opens nothing, lists nothing and
/// writes nothing. What the *user* does with the answer is a click.
#[tauri::command]
pub fn host_amiga_forever_folders() -> AmigaForeverFolders {
    let Ok(root) = std::env::var("AMIGAFOREVERDATA") else {
        return AmigaForeverFolders {
            adf: None,
            rom: None,
        };
    };
    let shared = PathBuf::from(root).join("Shared");
    let named = |name: &str| {
        let path = shared.join(name);
        // `is_dir()`, not `exists()`: a *file* called `adf` is not a folder to
        // scan, and offering it would produce a refusal one screen later.
        path.is_dir().then(|| path.to_string_lossy().into_owned())
    };
    AmigaForeverFolders {
        adf: named("adf"),
        rom: named("rom"),
    }
}

#[cfg(test)]
mod amiga_forever_tests {
    use super::*;

    /// The wire keys the offer line reads. `serde(rename_all)` on one struct
    /// is what made `VerifyReport::notChecked` render `undefined` for a whole
    /// round, and this struct is two fields the screen indexes by name.
    #[test]
    fn the_amiga_forever_answer_serializes_with_the_keys_the_offer_reads() {
        let value = serde_json::to_value(AmigaForeverFolders {
            adf: Some("E:\\amiga\\Shared\\adf".to_string()),
            rom: None,
        })
        .unwrap();
        assert_eq!(value["adf"], "E:\\amiga\\Shared\\adf");
        assert!(value["rom"].is_null());
        assert_eq!(
            value.as_object().unwrap().keys().collect::<Vec<_>>(),
            vec!["adf", "rom"]
        );
    }

    /// The three answers the offer can get, in **one** test on purpose.
    ///
    /// `AMIGAFOREVERDATA` is process-wide, and Rust runs tests in parallel:
    /// two tests each setting and restoring it would pass alone and race each
    /// other in a full run, which is precisely the timing-dependent shape
    /// CLAUDE.md says to make deterministic rather than to hope about. One
    /// test owns the variable, in sequence, and puts back whatever the
    /// developer's own machine had.
    ///
    /// `unsafe` because the environment is process-wide; nothing else in this
    /// crate reads or writes this variable.
    #[test]
    fn the_variable_and_the_folders_are_both_checked_and_neither_is_an_error() {
        let held = std::env::var("AMIGAFOREVERDATA").ok();

        // 1. No variable at all: two `None`s, never a failure. A machine
        //    without Amiga Forever simply never sees the offer line.
        unsafe { std::env::remove_var("AMIGAFOREVERDATA") };
        let missing = host_amiga_forever_folders();

        // 2. A variable naming a folder that holds no `Shared` material:
        //    still `None`. The check is that the folders are *there*, not
        //    that a path could be spelled — offering one that does not
        //    exist puts a refusal one screen later in the user's way.
        let empty = crate::core::ScratchDir::new("art-system", "amiga-forever-empty");
        unsafe { std::env::set_var("AMIGAFOREVERDATA", empty.path()) };
        let bare = host_amiga_forever_folders();

        // 3. The same shape *with* the folders answers both — so the
        //    arm above is about the folders rather than about the path being
        //    odd, and each folder is checked on its own.
        let full = crate::core::ScratchDir::new("art-system", "amiga-forever-full");
        std::fs::create_dir_all(full.join("Shared").join("adf")).unwrap();
        std::fs::create_dir_all(full.join("Shared").join("rom")).unwrap();
        unsafe { std::env::set_var("AMIGAFOREVERDATA", full.path()) };
        let found = host_amiga_forever_folders();

        // 4. ADFs but no ROMs answers one path and one `None`, rather than
        //    nothing — `core/hostfs.rs`'s per-entry rule, one layer out.
        let half = crate::core::ScratchDir::new("art-system", "amiga-forever-half");
        std::fs::create_dir_all(half.join("Shared").join("adf")).unwrap();
        unsafe { std::env::set_var("AMIGAFOREVERDATA", half.path()) };
        let partial = host_amiga_forever_folders();

        match held {
            Some(value) => unsafe { std::env::set_var("AMIGAFOREVERDATA", value) },
            None => unsafe { std::env::remove_var("AMIGAFOREVERDATA") },
        }

        assert_eq!(missing.adf, None, "no variable is not a guess");
        assert_eq!(missing.rom, None);
        assert_eq!(bare.adf, None, "a folder that is not there is not offered");
        assert_eq!(bare.rom, None);
        assert!(found.adf.unwrap().ends_with("adf"));
        assert!(found.rom.unwrap().ends_with("rom"));
        assert!(partial.adf.is_some(), "the ADFs are there and are offered");
        assert_eq!(partial.rom, None, "the ROMs are not, and are not");
    }
}
