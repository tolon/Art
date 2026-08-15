//! Installing AmigaOS from the user's own media (SD-2 · G5).
//!
//! ## A component is a set of paths, not a disk
//!
//! Measured, not assumed: `ModulesA1200_3.2.adf` holds 14 commands in `C/`,
//! and **thirteen are boot-floppy copies of commands `Workbench3.2` already
//! carries**. Exactly one, `LoadModule`, is new. Copying that disk onto `SYS:`
//! downgrades thirteen commands. `HDSetup3.2` (22), `DiskDoctor` (39) and
//! `Storage3.2` (9) have the same shape.
//!
//! So the unit of choice is a named set of [`PathRule`]s, and the recipe says
//! which paths on which media are actually wanted.
//!
//! Only `recipe` exists so far — the module tree this doc comment describes
//! (`apply`, `plan`, `scan`, `source`, `startup`, `verify`) lands one task at
//! a time, each adding its own `pub mod` line, so the crate compiles at the
//! end of every task rather than only at the end of the feature.

pub mod recipe;

use serde::{Deserialize, Serialize};

/// Whether a rule takes one file or a whole subtree.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RuleKind {
    File,
    Subtree,
}

/// One path taken out of a component's media.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PathRule {
    /// Where it lives on the media, `/`-separated: `LIBS/Modules`.
    pub from: String,
    /// Where it goes in the tree, `/`-separated: `Libs/Modules`.
    pub to: String,
    pub kind: RuleKind,
}

/// When a component applies without the user being asked.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "condition", rename_all = "kebab-case")]
pub enum Condition {
    /// On when the paired Kickstart's own stated major is below this.
    ///
    /// The ROM's **own header** answers it (`core::rom::stated_version`), not a
    /// checksum table — which is what keeps ART-104 from repeating: the
    /// user's licensed A1200 dump is not in `KNOWN_ROMS`, and a condition
    /// resting on that table would misfire on a ROM that is right.
    RomOlderThan { major: u16 },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Component {
    pub id: String,
    /// The **volume name inside the image**, not a filename: `Workbench3.2`.
    pub media: String,
    pub rules: Vec<PathRule>,
    #[serde(default)]
    pub required: bool,
    #[serde(default)]
    pub condition: Option<Condition>,
    /// Component ids this one may legitimately write over.
    #[serde(default)]
    pub overrides: Vec<String>,
    /// Lines for `S:User-Startup`, written inside this component's own block.
    #[serde(default)]
    pub user_startup: Vec<String>,
    /// Only one of a group may be chosen — `"modules"` for the Modules disks.
    #[serde(default)]
    pub exclusive_group: Option<String>,
    /// Registered but not built (CLAUDE.md, §96): shown as Coming Later.
    #[serde(default = "yes")]
    pub available: bool,
}

fn yes() -> bool {
    true
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Recipe {
    /// `"AmigaOS 3.2"`.
    pub release: String,
    pub components: Vec<Component>,
}

impl Recipe {
    pub fn component(&self, id: &str) -> Option<&Component> {
        self.components.iter().find(|c| c.id == id)
    }
}

/// Why an install cannot proceed. A value, never a sentence — the UI
/// translates it (ART-060).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "refusal", rename_all = "kebab-case")]
pub enum RefusalReason {
    /// No image in the folder carries this volume name.
    MediaMissing {
        component: String,
        volume_name: String,
    },
    /// The media is here and the path the recipe expects is not — so the
    /// recipe is wrong about *this* media, probably a different revision.
    /// Skipping it silently would give a system missing a library.
    MediaPathMissing {
        component: String,
        media: String,
        path: String,
    },
    /// The ROM was not identified, so a `Condition` cannot be decided.
    RomUnknown,
    /// Two components claim one destination and neither declared an override.
    DestinationCollision {
        path: String,
        components: Vec<String>,
    },
}

// ---------------------------------------------------------------------------
// Shared test fixtures (plan doc "Shared test fixtures" section).
//
// Grown one task at a time rather than landing whole here: this task adds
// `scratch`, `media`, `workbench`, `CancelAfter`, `digest_of_folder` and
// `fake_rom`. `planned_with`, `rdb_image` and `partition_offset` reference
// types (`InstallRequest`, `plan()`, `core/card/build.rs`) that later tasks
// create, so they are not written yet.
// ---------------------------------------------------------------------------

#[cfg(test)]
pub(crate) mod fixtures {
    use std::path::{Path, PathBuf};

    use crate::core::adf::create::create_blank_adf;
    use crate::core::adf::FileSystemType;
    use crate::core::jobs::ProgressSink;
    use crate::core::volume::device::FileRegionMut;
    use crate::core::volume::write::{FileMeta, VolumeWriter};
    use crate::core::volume::{DosType, VolumeGeometry};

    /// A fresh, empty directory for one test, named after `tag` and the
    /// process id so parallel test runs never collide. The repository's own
    /// convention (`core/archive/extract.rs::scratch`,
    /// `core/layout/apply.rs::scratch`) — deliberately not `tempfile`, which
    /// is not a dependency of this project.
    pub fn scratch(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("art-osinstall-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// Split a `/`-separated path into its directory segments and file name.
    fn split_path(path: &str) -> (Vec<&str>, &str) {
        let mut segments: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
        let file_name = segments.pop().expect("entry path must not be empty");
        (segments, file_name)
    }

    /// Write `entries` into the blank volume at `path`, creating whatever
    /// directories each entry's path needs one level at a time — the volume
    /// writer has no `mkdir -p`.
    fn write_entries(path: &Path, entries: &[(&str, &[u8], u32)]) {
        let geometry = VolumeGeometry::floppy_dd(DosType::new(*b"DOS\x01"));
        let mut device =
            FileRegionMut::open(path, 0, geometry.total_bytes(), geometry.block_size).unwrap();
        let mut writer = VolumeWriter::open(&mut device, geometry, path, 0).unwrap();

        for (entry_path, bytes, protection) in entries {
            let (dirs, file_name) = split_path(entry_path);
            let mut parent = 0u32;
            for dir_name in dirs {
                parent = match writer.find(parent, dir_name).unwrap() {
                    Some(existing) => existing.block,
                    None => writer.make_dir(parent, dir_name).unwrap().block.unwrap(),
                };
            }
            writer
                .add_file(
                    parent,
                    file_name,
                    bytes,
                    FileMeta {
                        protection: Some(*protection),
                        date: None,
                    },
                )
                .unwrap();
        }
    }

    /// A synthetic install disk. **ART ships no Amiga content** — this builds
    /// one, now, in a tempdir.
    ///
    /// `entries` is `(path, bytes, protection)`. Protection is `HSPARWED` with
    /// `RWED` inverted, so `0x20` is `--p-rwed` and `0x42` is `-s--rw-d`.
    pub fn media(
        dir: &Path,
        volume: &str,
        filename: &str,
        entries: &[(&str, &[u8], u32)],
    ) -> PathBuf {
        let path = dir.join(filename);
        std::fs::write(
            &path,
            create_blank_adf(volume, FileSystemType::Ffs, false).unwrap(),
        )
        .unwrap();
        write_entries(&path, entries);
        path
    }

    /// `Workbench3.2` with the two files every test in this plan leans on.
    pub fn workbench(dir: &Path) -> PathBuf {
        media(
            dir,
            "Workbench3.2",
            "wb.adf",
            &[
                ("C/LoadModule", b"cmd", 0x20),            // --p-rwed
                ("S/Startup-sequence", b"; test\n", 0x42), // -s--rw-d
            ],
        )
    }

    /// Stops the job after `n` units, so a cancel path can be tested without
    /// timing.
    pub struct CancelAfter {
        limit: u64,
        seen: std::sync::atomic::AtomicU64,
    }

    impl CancelAfter {
        pub fn new(limit: u64) -> Self {
            Self {
                limit,
                seen: std::sync::atomic::AtomicU64::new(0),
            }
        }
    }

    impl ProgressSink for CancelAfter {
        fn report(&self, _done: u64, _total: Option<u64>, _message: &str) {
            self.seen.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        }
        fn is_cancelled(&self) -> bool {
            self.seen.load(std::sync::atomic::Ordering::SeqCst) >= self.limit
        }
    }

    /// Every path under `root`, in a stable order.
    fn walkdir_sorted(root: &Path) -> Vec<PathBuf> {
        let mut found = Vec::new();
        let mut stack = vec![root.to_path_buf()];
        while let Some(dir) = stack.pop() {
            let Ok(read) = std::fs::read_dir(&dir) else {
                continue;
            };
            for entry in read.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    stack.push(path.clone());
                }
                found.push(path);
            }
        }
        found
    }

    /// One hash over a whole folder, so "unchanged" is a single assertion.
    /// Sorted, so it does not depend on directory order.
    pub fn digest_of_folder(root: &Path) -> String {
        use sha2::{Digest, Sha256};
        let mut names: Vec<PathBuf> = walkdir_sorted(root);
        names.sort();
        let mut hasher = Sha256::new();
        for path in names {
            hasher.update(path.to_string_lossy().as_bytes());
            if path.is_file() {
                hasher.update(std::fs::read(&path).unwrap());
            }
        }
        format!("{:x}", hasher.finalize())
    }

    /// A ROM file that states `major` in its own header — which is what
    /// `core::rom::stated_version` reads, and what the Modules condition asks.
    /// Deliberately *not* a real dump: ART ships none, and the condition does
    /// not consult `KNOWN_ROMS` anyway (ART-104).
    pub fn fake_rom(dir: &Path, major: u16) -> PathBuf {
        let path = dir.join(format!("kick-{major}.rom"));
        let mut bytes = vec![0u8; 512 * 1024];
        bytes[12..14].copy_from_slice(&major.to_be_bytes());
        bytes[14..16].copy_from_slice(&68u16.to_be_bytes());
        std::fs::write(&path, &bytes).unwrap();
        path
    }
}
