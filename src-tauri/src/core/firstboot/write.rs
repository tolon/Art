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

use std::path::PathBuf;

use serde::Serialize;

use super::plan::FirstBootPlan;
use super::scripts::fixed_files;
use super::{user_startup_lines, COMPONENT, FLAG_PATH};
use crate::core::error::CoreResult;
use crate::core::osinstall::startup::merge_user_startup;
use crate::core::safety::{atomic_write, guarded_write, BackupPolicy};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Written {
    /// Tree-relative paths, in the order they were written; the block last.
    pub files: Vec<String>,
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

    for file in fixed_files() {
        let path = tree.join(file.tree_path);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        atomic_write(&path, file.text.as_bytes())?;
        files.push(file.tree_path.to_string());
    }

    let flag = tree.join(FLAG_PATH);
    if let Some(parent) = flag.parent() {
        std::fs::create_dir_all(parent)?;
    }
    atomic_write(&flag, b"TRUE\n")?;
    files.push(FLAG_PATH.to_string());

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
        user_startup_backup: backup,
        user_startup_created: existing.is_none(),
    })
}

#[cfg(test)]
mod tests {
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
        })
        .unwrap();
        super::write(&p).unwrap();
        let first = snapshot(d.path());
        let p2 = plan(&FirstBootRequest {
            tree: d.path().to_path_buf(),
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
        })
        .unwrap();
        super::write(&p).unwrap();
        let after = fs::read(d.join("S/User-Startup")).unwrap();
        assert_eq!(after[5], 0xE9, "the high byte must survive the round trip");
    }
}
