//! Putting a plan into a tree.
//!
//! Order matters and is the whole design of this file: every file the block
//! points at is written **before** the block, so a write that stops halfway
//! leaves a tree whose `User-Startup` does not name a dispatcher that is not
//! there. The block itself goes through `merge_user_startup` and a
//! `BackupPolicy::CONFIG` guarded write, because `User-Startup` is a file a
//! person edits by hand (§39/§40).
//!
//! No `.uaem` sidecars: every file here has default protection and no
//! comment, and `core/osinstall/apply.rs::settle_sidecar` writes a sidecar
//! only when there is something to say.
//!
//! The wizard and the keymap marker are written or removed before the block too; see `write`.

use std::path::PathBuf;

use serde::Serialize;

use super::plan::FirstBootPlan;
use super::scripts::{fixed_files, WIZARD_FILE};
use super::{user_startup_lines, COMPONENT, FLAG_PATH};
use crate::core::amigaprefs::env;
use crate::core::error::CoreResult;
use crate::core::osinstall::startup::merge_user_startup;
use crate::core::safety::{atomic_write, guarded_remove, guarded_write, BackupPolicy};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Written {
    /// Tree-relative paths, in the order they were written; the block last.
    pub files: Vec<String>,
    /// Tree-relative paths ART removed, in order: its own `90-prefs` when the
    /// wizard was not asked, `ART_Set_Input` when the keymap block is gone.
    /// Only what was there.
    ///
    /// **Where a removal is told** (M3, final review): `commands/firstboot.rs`
    /// records it in the operation log as a `Files removed` detail, and it
    /// crosses the wire in `FirstBootWritten`. No screen renders it — the
    /// build's first-boot row draws one sentence, and that one is about
    /// `S/User-Startup`. This comment used to claim that a removal nobody is
    /// told about is a thing ART did and did not say, which promised a
    /// sentence the product does not print; the log is where it is told.
    pub removed: Vec<String>,
    pub user_startup_backup: Option<PathBuf>,
    pub user_startup_created: bool,
}

/// Write every fixed file, the flag, and finally the `User-Startup` block.
///
/// The block goes last on purpose (see the module doc comment): a failure
/// partway through the fixed files or the flag must never leave a
/// `User-Startup` that dispatches to `S:ART-FirstBoot` before that file — or
/// any step it names — actually exists in the tree.
pub fn write(plan: &FirstBootPlan) -> CoreResult<Written> {
    let tree = &plan.tree;
    let mut files = Vec::new();
    let mut removed = Vec::new();

    for file in fixed_files() {
        let path = tree.join(file.tree_path);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        atomic_write(&path, file.text.as_bytes())?;
        files.push(file.tree_path.to_string());
    }

    // The wizard, or its absence (design §4.4). `S/FirstBoot/` is ART's own
    // drawer and `90-prefs` is compiled-in text re-ticking writes back byte
    // for byte, and the loop above replaces every fixed file with no backup —
    // so the removal goes through `core/safety` with the same policy. Not the
    // Recycle Bin: `core/hostfs.rs` is for a user's own files chosen on the
    // host; `core/osinstall/apply.rs`'s sidecar removal is this precedent.
    let wizard = tree.join(WIZARD_FILE.tree_path);
    if plan.wizard.is_some() {
        atomic_write(&wizard, WIZARD_FILE.text.as_bytes())?;
        files.push(WIZARD_FILE.tree_path.to_string());
    } else if wizard.is_file() {
        guarded_remove(&wizard, BackupPolicy::NONE)?;
        removed.push(WIZARD_FILE.tree_path.to_string());
    }

    let flag = tree.join(FLAG_PATH);
    if let Some(parent) = flag.parent() {
        std::fs::create_dir_all(parent)?;
    }
    atomic_write(&flag, b"TRUE\n")?;
    files.push(FLAG_PATH.to_string());

    // `ART_Set_Input` (design §4.3), re-derived on every write: the keymap
    // line is written only by the build's tree phase, and the first-boot
    // phase always follows it. `plan` read the block with the merge's own
    // pairing; the marker says what that read found.
    let marker = tree.join(env::ART_SET_INPUT_PATH);
    if plan.input_set_by_art {
        // Its own drawer, not the flag's (M2, final review): the two land in
        // the same `Prefs/Env-Archive/` today, so without this the write
        // depended on the flag above having run first — order nothing here
        // declared, and a reorder would have broken it silently.
        if let Some(parent) = marker.parent() {
            std::fs::create_dir_all(parent)?;
        }
        atomic_write(&marker, &env::encode_value(env::MARKER_VALUE)?)?;
        files.push(env::ART_SET_INPUT_PATH.to_string());
    } else if marker.is_file() {
        guarded_remove(&marker, BackupPolicy::NONE)?;
        removed.push(env::ART_SET_INPUT_PATH.to_string());
    }

    let user_startup = tree.join("S").join("User-Startup");
    // A Latin-1 round trip, deliberately: a hand-edited `User-Startup` may
    // carry bytes 0x80-0xFF that are not valid UTF-8, and `merge_user_startup`
    // takes `&str`. Every byte 0-255 is exactly one `char` this way, so
    // nothing outside the block ART touches is altered even when it is not
    // valid text by any modern encoding.
    let existing = match std::fs::read(&user_startup) {
        Ok(bytes) => Some(bytes.iter().map(|&b| b as char).collect::<String>()),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => None,
        Err(err) => return Err(err.into()),
    };
    let merged = merge_user_startup(existing.as_deref(), COMPONENT, &user_startup_lines());
    let merged_bytes: Vec<u8> = merged.chars().map(|c| c as u32 as u8).collect();
    let backup = guarded_write(&user_startup, &merged_bytes, BackupPolicy::CONFIG)?;
    files.push("S/User-Startup".to_string());

    Ok(Written {
        files,
        removed,
        user_startup_backup: backup,
        user_startup_created: existing.is_none(),
    })
}

#[cfg(test)]
mod tests {
    use crate::core::amigaprefs::env;
    use crate::core::firstboot::plan::FirstBootPlan;
    use crate::core::firstboot::plan::{plan, FirstBootRequest};
    use crate::core::ScratchDir;
    use std::fs;

    fn tree(tag: &str) -> ScratchDir {
        let d = ScratchDir::new("art-firstboot-write", tag);
        fs::create_dir_all(d.join("S")).unwrap();
        fs::write(d.join("distribution.json"), b"{}").unwrap();
        fs::write(
            d.join("S/Startup-Sequence"),
            b"IF EXISTS S:User-Startup\n  Execute S:User-Startup\nENDIF\n",
        )
        .unwrap();
        fs::create_dir_all(d.join("C")).unwrap();
        for command in crate::core::firstboot::plan::NEEDED_COMMANDS {
            fs::write(d.join("C").join(command), b"\x00\x00\x03\xf3").unwrap();
        }
        d
    }

    fn snapshot(root: &std::path::Path) -> std::collections::BTreeMap<String, Vec<u8>> {
        fn walk(
            dir: &std::path::Path,
            root: &std::path::Path,
            out: &mut std::collections::BTreeMap<String, Vec<u8>>,
        ) {
            for entry in fs::read_dir(dir).unwrap() {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    walk(&path, root, out);
                } else {
                    let rel = path
                        .strip_prefix(root)
                        .unwrap()
                        .to_string_lossy()
                        .replace('\\', "/");
                    out.insert(rel, fs::read(&path).unwrap());
                }
            }
        }
        let mut out = std::collections::BTreeMap::new();
        walk(root, root, &mut out);
        out
    }

    #[test]
    fn writes_every_fixed_file_the_flag_and_the_block() {
        let d = tree("all");
        let p = plan(&FirstBootRequest {
            tree: d.path().to_path_buf(),
            ask_prefs: false,
        })
        .unwrap();
        let w = super::write(&p).unwrap();
        for f in crate::core::firstboot::scripts::fixed_files() {
            assert_eq!(
                fs::read_to_string(d.join(f.tree_path)).unwrap(),
                f.text,
                "{}",
                f.tree_path
            );
            assert!(
                !d.join(format!("{}.uaem", f.tree_path)).exists(),
                "no sidecar for default protection"
            );
        }
        assert_eq!(
            fs::read_to_string(d.join("Prefs/Env-Archive/ART_FirstBoot")).unwrap(),
            "TRUE\n"
        );
        let us = fs::read_to_string(d.join("S/User-Startup")).unwrap();
        assert_eq!(us, ";BEGIN art-firstboot\nIF EXISTS S:ART-FirstBoot\n  Execute S:ART-FirstBoot\nENDIF\n;END art-firstboot\n");
        assert!(w.user_startup_created);
        assert!(w.user_startup_backup.is_none());
        assert_eq!(w.files.len(), 9);
    }

    #[test]
    fn writing_twice_is_byte_identical() {
        let d = tree("twice");
        let p = plan(&FirstBootRequest {
            tree: d.path().to_path_buf(),
            ask_prefs: false,
        })
        .unwrap();
        super::write(&p).unwrap();
        let first = snapshot(d.path());
        let p2 = plan(&FirstBootRequest {
            tree: d.path().to_path_buf(),
            ask_prefs: false,
        })
        .unwrap();
        assert!(p2.already_written);
        super::write(&p2).unwrap();
        let second = snapshot(d.path());
        // A backup generation of User-Startup is the one permitted difference.
        let strip = |m: std::collections::BTreeMap<String, Vec<u8>>| {
            m.into_iter()
                .filter(|(k, _)| !k.contains("User-Startup."))
                .collect::<Vec<_>>()
        };
        assert_eq!(strip(first), strip(second));
    }

    #[test]
    fn the_users_own_user_startup_lines_survive_byte_for_byte() {
        let d = tree("keep");
        let theirs = "; my own line\r\nAssign FONTS: Work:Fonts ADD\r\n;BEGIN other-tool\r\nRun Other\r\n;END other-tool\r\n";
        fs::write(d.join("S/User-Startup"), theirs).unwrap();
        let p = plan(&FirstBootRequest {
            tree: d.path().to_path_buf(),
            ask_prefs: false,
        })
        .unwrap();
        let w = super::write(&p).unwrap();
        let after = fs::read_to_string(d.join("S/User-Startup")).unwrap();
        assert!(
            after.starts_with(theirs),
            "everything before the block is untouched"
        );
        assert!(after.ends_with(";END art-firstboot\n"));
        assert!(!w.user_startup_created);
        let backup = w
            .user_startup_backup
            .expect("an existing User-Startup is backed up first");
        assert_eq!(fs::read_to_string(backup).unwrap(), theirs);
    }

    #[test]
    fn a_failed_write_leaves_the_tree_without_a_half_written_block() {
        let d = tree("fail");
        // Make S/FirstBoot a *file* so the step directory cannot be created.
        fs::write(d.join("S/FirstBoot"), b"in the way").unwrap();
        let p = plan(&FirstBootRequest {
            tree: d.path().to_path_buf(),
            ask_prefs: false,
        })
        .unwrap();
        assert!(super::write(&p).is_err());
        assert!(!d.join("S/User-Startup").exists(), "the block is the last thing written, so nothing points at a dispatcher that is not there");
    }

    #[test]
    fn high_bytes_in_user_startup_round_trip() {
        let d = tree("highbytes");
        let mut theirs: Vec<u8> = b"; caf".to_vec();
        theirs.push(0xE9);
        theirs.extend_from_slice(b"\n");
        fs::write(d.join("S/User-Startup"), &theirs).unwrap();
        let p = plan(&FirstBootRequest {
            tree: d.path().to_path_buf(),
            ask_prefs: false,
        })
        .unwrap();
        super::write(&p).unwrap();
        let after = fs::read(d.join("S/User-Startup")).unwrap();
        assert_eq!(after[5], 0xE9, "the high byte must survive the round trip");
    }

    fn planned(d: &ScratchDir, ask_prefs: bool) -> FirstBootPlan {
        plan(&FirstBootRequest {
            tree: d.path().to_path_buf(),
            ask_prefs,
        })
        .unwrap()
    }

    #[test]
    fn asking_writes_the_wizard_exactly_and_before_the_block() {
        let d = tree("wizard");
        let w = super::write(&planned(&d, true)).unwrap();
        assert_eq!(
            fs::read_to_string(d.join("S/FirstBoot/90-prefs")).unwrap(),
            crate::core::firstboot::scripts::STEP_90_PREFS
        );
        let wizard = w
            .files
            .iter()
            .position(|f| f == "S/FirstBoot/90-prefs")
            .unwrap();
        let block = w.files.iter().position(|f| f == "S/User-Startup").unwrap();
        assert!(wizard < block, "the block is the last thing written");
        assert!(w.removed.is_empty());
    }

    /// Design §4.4: unticking and building again must not leave the wizard.
    #[test]
    fn unticking_and_writing_again_removes_arts_own_wizard_and_nothing_else() {
        let d = tree("untick");
        super::write(&planned(&d, true)).unwrap();
        let w = super::write(&planned(&d, false)).unwrap();
        assert!(!d.join("S/FirstBoot/90-prefs").exists());
        assert_eq!(w.removed, vec!["S/FirstBoot/90-prefs".to_string()]);
        assert!(d.join("S/FirstBoot/10-hardware").is_file());
        assert!(
            !d.join("S/FirstBoot/.art-backup").exists(),
            "no backup drawer inside the step directory the card carries"
        );
    }

    #[test]
    fn a_tree_that_never_had_a_wizard_reports_no_removal() {
        let d = tree("never");
        assert!(super::write(&planned(&d, false))
            .unwrap()
            .removed
            .is_empty());
    }

    #[test]
    fn a_keymap_block_writes_the_input_marker() {
        let d = tree("marker");
        fs::write(
            d.join("S/User-Startup"),
            b";BEGIN keymap-selection\nSetKeyboard usa\n;END keymap-selection\n",
        )
        .unwrap();
        let w = super::write(&planned(&d, false)).unwrap();
        assert_eq!(fs::read(d.join(env::ART_SET_INPUT_PATH)).unwrap(), b"TRUE");
        assert!(w.files.contains(&env::ART_SET_INPUT_PATH.to_string()));
    }

    /// **M2 (final review).** The marker's write makes its own drawer. The
    /// flag written a few lines above it lands in the same
    /// `Prefs/Env-Archive/`, so before the guard this write worked only
    /// because that one had run first — a dependency on statement order that
    /// nothing in the file declared. This tree has no `Prefs/Env-Archive`
    /// until the write makes one.
    #[test]
    fn the_input_marker_makes_its_own_drawer() {
        let d = tree("marker-drawer");
        fs::write(
            d.join("S/User-Startup"),
            b";BEGIN keymap-selection\nSetKeyboard usa\n;END keymap-selection\n",
        )
        .unwrap();
        assert!(
            !d.join("Prefs/Env-Archive").exists(),
            "nothing has made the drawer yet"
        );
        let w = super::write(&planned(&d, false)).unwrap();
        assert_eq!(fs::read(d.join(env::ART_SET_INPUT_PATH)).unwrap(), b"TRUE");
        assert!(w.files.contains(&env::ART_SET_INPUT_PATH.to_string()));
    }

    #[test]
    fn a_stray_keymap_opener_writes_no_input_marker() {
        let d = tree("stray");
        fs::write(
            d.join("S/User-Startup"),
            b";BEGIN keymap-selection\nSetKeyboard usa\n",
        )
        .unwrap();
        super::write(&planned(&d, false)).unwrap();
        assert!(!d.join(env::ART_SET_INPUT_PATH).exists());
    }

    #[test]
    fn a_tree_whose_keymap_block_is_gone_loses_the_marker() {
        let d = tree("lost");
        fs::create_dir_all(d.join("Prefs/Env-Archive")).unwrap();
        fs::write(d.join(env::ART_SET_INPUT_PATH), b"TRUE").unwrap();
        let w = super::write(&planned(&d, false)).unwrap();
        assert!(!d.join(env::ART_SET_INPUT_PATH).exists());
        assert_eq!(w.removed, vec![env::ART_SET_INPUT_PATH.to_string()]);
    }

    #[test]
    fn writing_twice_with_the_wizard_and_a_keymap_is_byte_identical() {
        let d = tree("twice-wizard");
        fs::write(
            d.join("S/User-Startup"),
            b";BEGIN keymap-selection\nSetKeyboard usa\n;END keymap-selection\n",
        )
        .unwrap();
        super::write(&planned(&d, true)).unwrap();
        let first = snapshot(d.path());
        super::write(&planned(&d, true)).unwrap();
        let second = snapshot(d.path());
        let strip = |m: std::collections::BTreeMap<String, Vec<u8>>| {
            m.into_iter()
                .filter(|(k, _)| !k.contains("User-Startup."))
                .collect::<Vec<_>>()
        };
        assert_eq!(strip(first), strip(second));
    }

    #[test]
    fn a_removal_crosses_the_wire_under_its_own_name() {
        let w = super::Written {
            files: vec![],
            removed: vec!["S/FirstBoot/90-prefs".into()],
            user_startup_backup: None,
            user_startup_created: false,
        };
        assert_eq!(
            serde_json::to_value(&w).unwrap()["removed"][0],
            "S/FirstBoot/90-prefs"
        );
    }
}
