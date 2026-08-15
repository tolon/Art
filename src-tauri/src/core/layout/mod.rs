//! What goes where (SD-2 · G11).
//!
//! Between the classifier and the card: a pile of files in, a staging tree on
//! the PC out, which the preload screen then copies onto a volume. The staging
//! seam is not a preference — a real PiStorm card is PFS3 and ART cannot write
//! PFS3, so writing straight into the volume works only on FFS, which is not
//! what a finished card uses.

pub mod apply;
pub mod policy;
pub mod scan;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::core::detect::{detect, FormatCategory};
use crate::core::error::CoreResult;
use crate::core::layout::policy::{drawer_for, Policy, WhdloadPlacement};
use crate::core::layout::scan::{gather, Found};

/// What ART can justify saying about one thing on disk.
///
/// **There is no `Demo` and there will not be one.** `Detection` carries a
/// category, a format hint and a confidence, and nothing derivable from the
/// bytes separates a demo from a game. The preview is editable instead; §14
/// and §34 say an uncertain classification is offered, never acted on as fact.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum ItemKind {
    /// An archive holding a WHDLoad pack. `name` is what the drawer will be
    /// called, from `core/whdload::analyse`.
    WhdloadArchive {
        name: String,
    },
    /// A folder that *is* a drawer — it directly holds a `.slave`.
    WhdloadDrawer {
        name: String,
    },
    FloppyImage,
    HardDiskImage,
    OpticalImage,
    /// An archive that is not a WHDLoad pack. Could be anything, so it goes to
    /// `Unsorted/` and the user moves it.
    Archive,
    Unknown,
    /// Belongs on the FAT32 boot partition. Refused here.
    Rom,
    /// No business on an Amiga volume at all. Refused here.
    Commodore8Bit,
}

/// How an item reaches the staging tree.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Placement {
    CopyFile,
    CopyTree,
    UnpackWhdload,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LayoutItem {
    pub source: PathBuf,
    pub kind: ItemKind,
    /// Relative to the staging root, `/` separated. Proposed by the policy;
    /// the user may change it before the plan is applied.
    pub destination: String,
    pub placement: Placement,
    /// What this will occupy once placed. For an unpacked archive this is the
    /// archive's own declared uncompressed total — a claim, and named as one.
    pub bytes: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RefusalReason {
    /// A Kickstart: the Pi firmware reads it off FAT32, not off a volume.
    BelongsOnBootPartition,
    /// A 1541 disk, a tape, a cartridge.
    NoPlaceOnAnAmigaVolume,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Refusal {
    pub source: PathBuf,
    pub reason: RefusalReason,
}

/// Two or more things wanting one name, or one thing wanting a name the
/// staging tree already holds.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Collision {
    pub destination: String,
    /// Empty of a second entry when the clash is with a file already on disk.
    pub sources: Vec<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LayoutPlan {
    pub root: PathBuf,
    pub items: Vec<LayoutItem>,
    pub refused: Vec<Refusal>,
    pub collisions: Vec<Collision>,
    pub bytes: u64,
}

/// What one found thing is.
///
/// A directory reaching here is always a WHDLoad drawer — `scan::gather` walks
/// through every other kind rather than returning it.
pub fn classify(found: &Found) -> CoreResult<ItemKind> {
    if found.is_dir {
        return Ok(ItemKind::WhdloadDrawer {
            name: name_of(&found.path),
        });
    }

    let detection = detect(&found.path)?;
    Ok(match detection.category {
        FormatCategory::FloppyImage => ItemKind::FloppyImage,
        FormatCategory::HardDiskImage => ItemKind::HardDiskImage,
        FormatCategory::OpticalImage => ItemKind::OpticalImage,
        FormatCategory::Rom => ItemKind::Rom,
        FormatCategory::Commodore8Bit => ItemKind::Commodore8Bit,
        FormatCategory::Archive => match whdload_name(&found.path) {
            Some(name) => ItemKind::WhdloadArchive { name },
            None => ItemKind::Archive,
        },
        FormatCategory::Directory | FormatCategory::Unknown => ItemKind::Unknown,
    })
}

/// The drawer name an archive would unpack to, if it holds a WHDLoad pack.
///
/// Reads the archive's **entry list only** — no decompression — so asking the
/// question of four hundred files costs four hundred directory reads rather
/// than four hundred unpacks. `analyse` is the right function here and the
/// wrong one for a folder: this is precisely the shape it was written for, one
/// drawer beside its own `.info`.
fn whdload_name(path: &Path) -> Option<String> {
    let mut backend = crate::core::archive::open(path).ok()?;
    let entries = backend.entries().ok()?;
    let listed: Vec<crate::core::whdload::Entry> = entries
        .iter()
        .map(|entry| crate::core::whdload::Entry {
            relative: entry.name.clone(),
            is_dir: entry.is_dir,
        })
        .collect();
    crate::core::whdload::analyse(&listed)
        .ok()
        .map(|layout| layout.name)
}

fn name_of(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// What laying these paths out under `root` would do. Writes nothing.
pub fn plan(root: &Path, paths: &[PathBuf], policy: &Policy) -> CoreResult<LayoutPlan> {
    let mut items = Vec::new();
    let mut refused = Vec::new();

    for found in gather(paths)? {
        let kind = classify(&found)?;
        let Some(drawer) = drawer_for(&kind, policy) else {
            refused.push(Refusal {
                source: found.path.clone(),
                reason: match kind {
                    ItemKind::Rom => RefusalReason::BelongsOnBootPartition,
                    _ => RefusalReason::NoPlaceOnAnAmigaVolume,
                },
            });
            continue;
        };

        let (placement, leaf, bytes) = match (&kind, policy.whdload) {
            (ItemKind::WhdloadArchive { name }, WhdloadPlacement::Unpack) => (
                Placement::UnpackWhdload,
                name.clone(),
                declared_bytes(&found.path).unwrap_or(found.bytes),
            ),
            (ItemKind::WhdloadDrawer { name }, _) => {
                (Placement::CopyTree, name.clone(), found.bytes)
            }
            _ => (Placement::CopyFile, name_of(&found.path), found.bytes),
        };

        items.push(LayoutItem {
            source: found.path,
            kind,
            destination: format!("{drawer}/{leaf}"),
            placement,
            bytes,
        });
    }

    let collisions = collisions_in(root, &items);
    let bytes = items.iter().map(|item| item.bytes).sum();

    Ok(LayoutPlan {
        root: root.to_path_buf(),
        items,
        refused,
        collisions,
        bytes,
    })
}

/// What an archive says it decompresses to. A claim, used only to show the
/// user a number; the gate measures what actually arrives.
fn declared_bytes(path: &Path) -> Option<u64> {
    let mut backend = crate::core::archive::open(path).ok()?;
    Some(
        backend
            .entries()
            .ok()?
            .iter()
            .map(|entry| entry.declared_bytes)
            .sum(),
    )
}

/// Every destination two items want, and every one the tree already holds.
fn collisions_in(root: &Path, items: &[LayoutItem]) -> Vec<Collision> {
    let mut by_destination: BTreeMap<&str, Vec<PathBuf>> = BTreeMap::new();
    for item in items {
        by_destination
            .entry(item.destination.as_str())
            .or_default()
            .push(item.source.clone());
    }

    by_destination
        .into_iter()
        .filter_map(|(destination, sources)| {
            let on_disk = root.join(destination.replace('/', std::path::MAIN_SEPARATOR_STR));
            if sources.len() > 1 || on_disk.exists() {
                Some(Collision {
                    destination: destination.to_string(),
                    sources,
                })
            } else {
                None
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("art-layout-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// A 901 120-byte file whose first bytes say `DOS\0` is an ADF, and
    /// `core/detect` says so from the bytes rather than the name.
    fn adf(path: &Path) {
        let mut bytes = vec![0u8; 901_120];
        bytes[0..4].copy_from_slice(b"DOS\0");
        std::fs::write(path, bytes).unwrap();
    }

    #[test]
    fn a_floppy_image_is_proposed_for_the_floppies_drawer() {
        let dir = scratch("adf");
        let root = dir.join("staging");
        adf(&dir.join("Workbench.adf"));

        let made = plan(&root, &[dir.join("Workbench.adf")], &Policy::default()).unwrap();

        assert_eq!(made.items.len(), 1, "{made:?}");
        assert_eq!(made.items[0].kind, ItemKind::FloppyImage);
        assert_eq!(made.items[0].destination, "Floppies/Workbench.adf");
        assert_eq!(made.items[0].placement, Placement::CopyFile);
        assert!(made.refused.is_empty());

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A folder that is a drawer keeps its own name under `Games/` and is
    /// copied as a tree.
    #[test]
    fn a_whdload_drawer_is_proposed_whole_under_games() {
        let dir = scratch("drawer");
        let root = dir.join("staging");
        let game = dir.join("TurricanII");
        std::fs::create_dir_all(&game).unwrap();
        std::fs::write(game.join("TurricanII.slave"), b"slave").unwrap();

        let made = plan(&root, std::slice::from_ref(&game), &Policy::default()).unwrap();

        assert_eq!(
            made.items[0].kind,
            ItemKind::WhdloadDrawer {
                name: "TurricanII".into()
            }
        );
        assert_eq!(made.items[0].destination, "Games/TurricanII");
        assert_eq!(made.items[0].placement, Placement::CopyTree);

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// **A ROM is refused, not placed.** It belongs on the FAT32 partition,
    /// and saying so beats a Kickstart quietly landing in `Unsorted/`.
    ///
    /// The 512 KB size alone is what makes this a ROM — `core/detect::rom_by_size`
    /// is reached by the `.rom` extension and judges only the size, never a
    /// signature (see `detect.rs`'s own `rom_known_size_has_higher_confidence`,
    /// which writes 512 KB of zeros to `kick.rom`). The magic bytes below are
    /// harmless flavour, not what triggers detection.
    #[test]
    fn a_rom_is_refused_with_a_reason_and_never_reaches_items() {
        let dir = scratch("rom");
        let root = dir.join("staging");
        let rom = dir.join("kick.rom");
        // 512 KB — the size a real Kickstart ROM is, and the only thing
        // `rom_by_size` actually checks.
        let mut bytes = vec![0u8; 512 * 1024];
        bytes[0..4].copy_from_slice(&[0x11, 0x11, 0x4E, 0xF9]);
        std::fs::write(&rom, bytes).unwrap();

        let made = plan(&root, std::slice::from_ref(&rom), &Policy::default()).unwrap();

        assert!(made.items.is_empty(), "{:?}", made.items);
        assert_eq!(made.refused.len(), 1);
        assert_eq!(made.refused[0].source, rom);
        assert_eq!(
            made.refused[0].reason,
            RefusalReason::BelongsOnBootPartition
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Two sources proposing the same destination are a collision, reported
    /// before the button rather than discovered during the copy.
    #[test]
    fn two_things_wanting_one_name_are_a_collision() {
        let dir = scratch("collide");
        let root = dir.join("staging");
        std::fs::create_dir_all(dir.join("one")).unwrap();
        std::fs::create_dir_all(dir.join("two")).unwrap();
        adf(&dir.join("one").join("Disk.adf"));
        adf(&dir.join("two").join("Disk.adf"));

        let made = plan(
            &root,
            &[dir.join("one"), dir.join("two")],
            &Policy::default(),
        )
        .unwrap();

        assert_eq!(made.collisions.len(), 1, "{:?}", made.collisions);
        assert_eq!(made.collisions[0].destination, "Floppies/Disk.adf");
        assert_eq!(made.collisions[0].sources.len(), 2);

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A destination that already exists on disk is a collision too — the
    /// applier never overwrites, so the plan has to say it first.
    #[test]
    fn a_name_already_in_the_staging_tree_is_a_collision() {
        let dir = scratch("exists");
        let root = dir.join("staging");
        std::fs::create_dir_all(root.join("Floppies")).unwrap();
        std::fs::write(root.join("Floppies").join("Disk.adf"), b"already here").unwrap();
        adf(&dir.join("Disk.adf"));

        let made = plan(&root, &[dir.join("Disk.adf")], &Policy::default()).unwrap();

        assert_eq!(made.collisions.len(), 1, "{:?}", made.collisions);

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The plan totals what the staging tree will need, so the number is on
    /// screen before the button.
    #[test]
    fn the_plan_totals_the_bytes_the_tree_will_need() {
        let dir = scratch("bytes");
        let root = dir.join("staging");
        adf(&dir.join("A.adf"));
        adf(&dir.join("B.adf"));

        let made = plan(&root, std::slice::from_ref(&dir), &Policy::default()).unwrap();

        assert_eq!(made.bytes, 901_120 * 2);

        let _ = std::fs::remove_dir_all(&dir);
    }
}
