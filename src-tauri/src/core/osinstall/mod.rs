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
//! ## The one genuine file collision: `C/LoadModule`
//!
//! `Storage3.2` and `ModulesA1200_3.2` both carry a file at `C/LoadModule` —
//! the only place two components in the shipped recipe actually write the
//! same file (everything else two or more components touch is a `Subtree`
//! merge point, like `Devs/` or `Locale/Languages`, not a collision — see
//! the doc comment on `recipe::tests::no_two_components_claim_one_destination_without_declaring_it`).
//! The two copies are measured **byte-identical**: SHA-256
//! `35acfea734816965d271352f59c3643963f69c7e4b2469e3473c5f5a8a60fc14` for
//! both. So the direction can't break anything either way; the recipe has
//! `modules-a1200` declare the override, because the Modules disk is the one
//! that ships `LoadModule` *beside the modules it exists to load* — the
//! command belongs with that disk's own purpose, not with the general
//! toolkit disk that happens to also carry a copy.
//!
//! `recipe`, `source`, `scan` and `plan` (the ROM condition, and now
//! [`plan::plan`] itself — components, media and ROM resolved into an
//! [`plan::InstallPlan`] or a collected list of [`RefusalReason`]s) exist so
//! far — the module tree this doc comment describes (`apply`, `startup`,
//! `verify`) lands one task at a time, each adding its own `pub mod` line,
//! so the crate compiles at the end of every task rather than only at the
//! end of the feature.

pub mod plan;
pub mod recipe;
pub mod scan;
pub mod source;

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
    /// More than one file in the media folder carries this component's
    /// volume name. `scan::find_media`/`scan::media_for` report every
    /// match rather than guessing at one, so a folder with a stray backup
    /// copy of a disk nobody selected still installs everything else — this
    /// only fires for the component that actually names the ambiguous
    /// volume, at plan time, never as a whole-folder scan failure. `paths`
    /// carries every file that claimed the name, because the user's next
    /// question is always "which two files?".
    ///
    /// `String`, not `PathBuf` — every other path-carrying field in this
    /// enum already is (`MediaPathMissing::path`), and `RefusalReason`
    /// derives `Serialize`: `serde`'s `PathBuf` implementation errors on a
    /// non-UTF-8 path, which would mean a refusal built to explain a
    /// problem could itself fail to cross the command boundary on Windows.
    /// Rendered with `.display().to_string()`, the same conversion
    /// `MediaPathMissing` already uses.
    MediaAmbiguous {
        component: String,
        volume_name: String,
        paths: Vec<String>,
    },
}

// ---------------------------------------------------------------------------
// Shared test fixtures (plan doc "Shared test fixtures" section).
//
// Grown one task at a time rather than landing whole here: Task 1 added
// `scratch`, `media`, `workbench`, `CancelAfter`, `digest_of_folder` and
// `fake_rom`. Task 5 adds `entries_for` and `planned_with`, now that
// `InstallRequest` and `plan()` exist to build them against. `rdb_image` and
// `partition_offset` reference `core/card/build.rs`, which is still a later
// task, so they are not written yet.
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

    /// Every file and directory under `root`, in no particular order —
    /// `digest_of_folder` does its own sorting, over the *relative* keys it
    /// actually hashes. Does not swallow an unreadable directory: a scratch
    /// tree this code just created should always be readable, and silently
    /// skipping one would make "unchanged" pass over data that was never
    /// examined.
    fn walk_paths(root: &Path) -> Vec<PathBuf> {
        let mut found = Vec::new();
        let mut stack = vec![root.to_path_buf()];
        while let Some(dir) = stack.pop() {
            let read = std::fs::read_dir(&dir)
                .unwrap_or_else(|e| panic!("reading {}: {e}", dir.display()));
            for entry in read {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    stack.push(path.clone());
                }
                found.push(path);
            }
        }
        found
    }

    /// `path`, relative to `root`, as a `/`-joined key — never the absolute
    /// path, and never the platform's own separator. Two identical trees
    /// rooted at different scratch directories must hash the same, which an
    /// absolute-path key would break by construction, and a bare
    /// `Path::to_string_lossy` would break on the one platform this project
    /// ships for, where the separator is `\`.
    fn relative_key(root: &Path, path: &Path) -> String {
        path.strip_prefix(root)
            .unwrap()
            .components()
            .map(|c| c.as_os_str().to_string_lossy().into_owned())
            .collect::<Vec<_>>()
            .join("/")
    }

    /// One hash over a whole folder, so "unchanged" is a single assertion,
    /// and so that the same tree copied to a different scratch directory
    /// still compares equal — the whole point of hashing a *copy* against
    /// its *original* (Task 6's proof). Sorted by the relative key it
    /// hashes, so it does not depend on directory read order.
    pub fn digest_of_folder(root: &Path) -> String {
        use sha2::{Digest, Sha256};
        let mut entries: Vec<(String, PathBuf)> = walk_paths(root)
            .into_iter()
            .map(|path| (relative_key(root, &path), path))
            .collect();
        entries.sort_by(|a, b| a.0.cmp(&b.0));

        let mut hasher = Sha256::new();
        for (key, path) in &entries {
            hasher.update(key.as_bytes());
            if path.is_file() {
                hasher.update(std::fs::read(path).unwrap());
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

    /// Every entry a component's own rules need present so `plan()` finds
    /// nothing missing: one placeholder file inside the drawer each
    /// `Subtree` rule names, and the literal file each `File` rule names.
    /// Built from the recipe actually passed in, not hand-copied from the
    /// JSON — so a rule added to `amigaos-3.2.json` later is automatically
    /// covered here too, and a test that wants *broken* media (Task 5's
    /// `plan_where_extras_has_no_l`) starts from this and removes the one
    /// entry it means to break, rather than drifting from what the shipped
    /// recipe actually asks for.
    pub fn entries_for(recipe: &super::Recipe, volume: &str) -> Vec<(String, Vec<u8>, u32)> {
        let mut entries = Vec::new();
        for component in recipe.components.iter().filter(|c| c.media == volume) {
            for rule in &component.rules {
                match rule.kind {
                    super::RuleKind::File => entries.push((rule.from.clone(), b"data".to_vec(), 0)),
                    super::RuleKind::Subtree if !rule.from.is_empty() => {
                        entries.push((format!("{}/placeholder", rule.from), b"data".to_vec(), 0));
                    }
                    // `from: ""` means the media's own root (`fonts`,
                    // `backdrops`) — `AdfSource::entry("")` always resolves
                    // to the root itself, so no placeholder is needed to
                    // make that rule satisfiable.
                    super::RuleKind::Subtree => {}
                }
            }
        }
        entries
    }

    /// A media folder plus a plan over it, so a test states only what it
    /// varies. `present` lists the volume names to create, each built with
    /// exactly the content its own component(s) in the shipped recipe need
    /// (via [`entries_for`]) — a test naming `"Workbench3.2"` gets media
    /// that satisfies every one of `workbench-base`'s rules, so nothing
    /// trips `MediaPathMissing` by accident. A fresh scratch directory every
    /// call (an atomic counter, not the caller's own tag) — this is called
    /// from many different tests, several of which run in parallel threads
    /// of the same process, and a shared tag would let two calls race over
    /// the same directory.
    pub fn planned_with(
        chosen: &[&str],
        present: &[&str],
        rom_major: Option<u16>,
    ) -> (crate::core::osinstall::plan::InstallPlan, PathBuf) {
        static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let n = COUNTER.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let dir = scratch(&format!("planned-with-{n}"));
        let folder = dir.join("media");
        std::fs::create_dir(&folder).unwrap();

        let recipe = crate::core::osinstall::recipe::amigaos_32().unwrap();
        for volume in present {
            let owned = entries_for(&recipe, volume);
            let refs: Vec<(&str, &[u8], u32)> = owned
                .iter()
                .map(|(path, bytes, protection)| (path.as_str(), bytes.as_slice(), *protection))
                .collect();
            media(&folder, volume, &format!("{volume}.adf"), &refs);
        }

        let rom = rom_major.map(|major| fake_rom(&dir, major));
        let request = crate::core::osinstall::plan::InstallRequest {
            media_folder: folder,
            rom,
            chosen: chosen.iter().map(|s| s.to_string()).collect(),
            destination: dir.join("dist"),
        };

        let plan = crate::core::osinstall::plan::plan(&request, &recipe).unwrap();
        (plan, dir)
    }

    /// Tasks 2 through 10 build their evidence on these helpers, so the
    /// helpers get their own coverage rather than trusting a one-off
    /// exercise that was run once by hand and then deleted. `entries_for`
    /// and `planned_with` are exercised indirectly, by every `plan_tests`
    /// test that calls them — a dedicated test here would only restate the
    /// shipped recipe's own shape.
    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn scratch_starts_empty_even_on_a_second_call_with_the_same_tag() {
            let dir = scratch("fixture-scratch");
            assert!(dir.is_dir());
            assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 0);

            std::fs::write(dir.join("leftover"), b"from a previous run").unwrap();

            let dir_again = scratch("fixture-scratch");
            assert_eq!(dir, dir_again, "the tag plus pid must be a stable path");
            assert_eq!(
                std::fs::read_dir(&dir_again).unwrap().count(),
                0,
                "scratch must clear whatever a previous run left behind"
            );
        }

        /// The protection byte is the one thing a test cannot see just by
        /// opening the file with ordinary I/O — it lives in the AmigaDOS
        /// header block, so this proves the volume writer really stored what
        /// `media()` was asked to store, not just that the bytes exist.
        #[test]
        fn media_writes_the_protection_bits_it_was_asked_for() {
            let dir = scratch("media-protection");
            let image = media(
                &dir,
                "Test",
                "t.adf",
                &[
                    ("C/LoadModule", b"cmd", 0x20),
                    ("S/Startup-sequence", b"; test\n", 0x42),
                ],
            );

            let parsed =
                crate::core::adf::AdfImage::from_bytes(std::fs::read(&image).unwrap()).unwrap();
            let root = parsed.list_root().unwrap();

            let c_dir = root.iter().find(|e| e.name == "C").unwrap();
            let load_module = parsed
                .list_dir(c_dir.header_block)
                .unwrap()
                .into_iter()
                .find(|e| e.name == "LoadModule")
                .unwrap();
            assert_eq!(load_module.attrs, "--p-rwed");

            let s_dir = root.iter().find(|e| e.name == "S").unwrap();
            let startup = parsed
                .list_dir(s_dir.header_block)
                .unwrap()
                .into_iter()
                .find(|e| e.name == "Startup-sequence")
                .unwrap();
            assert_eq!(startup.attrs, "-s--rw-d");
        }

        #[test]
        fn workbench_carries_the_two_files_every_test_in_this_plan_leans_on() {
            let dir = scratch("workbench-fixture");
            let image = workbench(&dir);

            let parsed =
                crate::core::adf::AdfImage::from_bytes(std::fs::read(&image).unwrap()).unwrap();
            let root = parsed.list_root().unwrap();
            let c_dir = root.iter().find(|e| e.name == "C").unwrap();
            let s_dir = root.iter().find(|e| e.name == "S").unwrap();

            assert!(parsed
                .list_dir(c_dir.header_block)
                .unwrap()
                .iter()
                .any(|e| e.name == "LoadModule"));
            assert!(parsed
                .list_dir(s_dir.header_block)
                .unwrap()
                .iter()
                .any(|e| e.name == "Startup-sequence"));
        }

        #[test]
        fn fake_rom_states_the_major_it_was_asked_for() {
            let dir = scratch("fake-rom");
            let rom = fake_rom(&dir, 45);
            let bytes = std::fs::read(&rom).unwrap();
            assert_eq!(crate::core::rom::stated_version(&bytes), Some((45, 68)));
        }

        #[test]
        fn cancel_after_flips_on_the_nth_report_and_not_before() {
            let sink = CancelAfter::new(3);
            assert!(!sink.is_cancelled());
            sink.report(0, None, "one");
            sink.report(0, None, "two");
            assert!(
                !sink.is_cancelled(),
                "the third report is the one that trips it"
            );
            sink.report(0, None, "three");
            assert!(sink.is_cancelled());
        }

        /// The property Task 6 actually leans on: two copies of the same
        /// tree, rooted at two different scratch directories, must digest
        /// identically — and a single changed byte must not.
        #[test]
        fn digest_of_folder_does_not_depend_on_where_the_tree_is_rooted() {
            let dir = scratch("digest-portable");
            let left = dir.join("left");
            let right = dir.join("right");
            std::fs::create_dir_all(left.join("sub")).unwrap();
            std::fs::create_dir_all(right.join("sub")).unwrap();
            std::fs::write(left.join("sub").join("a.txt"), b"hello").unwrap();
            std::fs::write(right.join("sub").join("a.txt"), b"hello").unwrap();

            assert_eq!(
                digest_of_folder(&left),
                digest_of_folder(&right),
                "identical trees under different roots must digest the same"
            );

            std::fs::write(right.join("sub").join("a.txt"), b"hello!").unwrap();
            assert_ne!(
                digest_of_folder(&left),
                digest_of_folder(&right),
                "a single changed byte must change the digest"
            );
        }
    }
}
