//! What goes where (SD-2 · G11).
//!
//! Between the classifier and the card: a pile of files in, a staging tree on
//! the PC out, which the preload screen then copies onto a volume. The staging
//! seam is not a preference — a real PiStorm card is PFS3 and ART cannot write
//! PFS3, so writing straight into the volume works only on FFS, which is not
//! what a finished card uses.

pub mod apply;
pub mod policy;
pub mod presence;
pub mod scan;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::core::detect::{detect, FormatCategory};
use crate::core::error::{CoreError, CoreResult};
use crate::core::layout::policy::{drawer_for, Policy, WhdloadPlacement};
use crate::core::layout::scan::{gather, Found};
use crate::core::security::path::safe_join;

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
    ///
    /// Serialises as `"commodore8-bit"` — serde's `kebab-case` splits before
    /// `Bit` but not before the digit, so `Commodore8Bit` becomes
    /// `commodore8-bit` rather than `commodore-8-bit`. This is deliberately
    /// **not** the same string `FormatCategory::Commodore8Bit` uses
    /// (`"commodore-8bit"`, from an explicit `#[serde(rename = …)]` in
    /// `core/detect.rs`): the two enums are independent wire formats, each
    /// stable in its own right, and neither is required to spell the word
    /// the way the other does.
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
    /// True when applying this item also writes an icon **beside** the
    /// drawer, at [`icon_destination`] (§82).
    ///
    /// A flag rather than a second path, deliberately: the user retargets
    /// `destination` on screen, and a stored icon path would go stale the
    /// moment they did. Derived from the archive's entry list at plan time —
    /// a WHDLoad pack that ships without an icon writes none — and
    /// `#[serde(default)]` because the screen sends the plan back for
    /// `layout_recheck` and `layout_apply`.
    #[serde(default)]
    pub writes_icon: bool,
}

/// Where this item's icon lands, or `None` when it writes none.
///
/// **One answer, from one source.** §82 requires the icon to be named after
/// the drawer that actually lands, so it is derived from `destination` here
/// and from the *target path* in `apply::unpack_whdload` — never from the
/// pack name the archive happened to carry. Those were two answers once
/// (ART-109), and they diverge the moment a user retargets a row: the drawer
/// would land as `Games/TurricanII` and the icon as `Games/Turrican.info`,
/// which is an icon Workbench does not attach to any drawer — the exact
/// silent failure §82 exists to prevent.
pub fn icon_destination(item: &LayoutItem) -> Option<String> {
    item.writes_icon
        .then(|| format!("{}.info", icon_stem(&item.destination)))
}

/// The destination's leaf, the way `safe_join` and the filesystem will see it.
///
/// Built from the **same** normalisation on both sides of the rule (ART-176's
/// divergence, found a second time by the wave-C1 review as F7): the plan side
/// used the raw `destination` string and the apply side used
/// `target.file_name()`. Those agree until somebody types `Games/Turrican/`
/// into the retarget box — then the plan says `Games/Turrican/.info` and the
/// applier says `Turrican.info`, and the icon is once again named something
/// the drawer is not. Trailing separators go, and the leaf is what is left.
pub fn icon_stem(destination: &str) -> &str {
    destination.trim_end_matches(['/', '\\'])
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

// `rename_all` is a no-op for every field that existed before `too_deep`
// (all one word) and makes the two ART-107 fields read as `tooDeep` and
// `duplicates` on the wire, which is the casing the frontend uses throughout.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LayoutPlan {
    pub root: PathBuf,
    pub items: Vec<LayoutItem>,
    pub refused: Vec<Refusal>,
    pub collisions: Vec<Collision>,
    pub bytes: u64,
    /// Folders the scan did not look inside because they sit at
    /// `scan::MAX_SCAN_DEPTH` — ART-107. Anything under them is missing from
    /// `items`, and the screen says so rather than showing a plan that is
    /// quietly short. `#[serde(default)]` because `InstallPlan`-style
    /// round-tripping applies here too: `layout_apply` takes back the plan the
    /// screen was shown.
    #[serde(default)]
    pub too_deep: crate::core::layout::scan::Dropped,
    /// Sources dropped because another source in the same scan already covers
    /// them — ART-107. Dropping a folder and then a file inside it used to put
    /// that file in the plan twice and make it collide with itself.
    #[serde(default)]
    pub duplicates: crate::core::layout::scan::Dropped,
    /// Destinations that already hold **exactly** what this plan would put
    /// there — ART-177. Skipped by `apply`, counted by the screen, and
    /// deliberately *not* listed among `collisions`: a collision is a
    /// question for the user, and this is not one.
    ///
    /// It is what makes a half-finished apply resume by itself: re-running
    /// the same plan places what is missing and steps over what is not.
    #[serde(default)]
    pub already_in_place: Vec<String>,
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
        FormatCategory::Archive => match whdload_pack(&found.path) {
            Some((name, _)) => ItemKind::WhdloadArchive { name },
            None => ItemKind::Archive,
        },
        FormatCategory::Directory | FormatCategory::Unknown => ItemKind::Unknown,
    })
}

/// The drawer name an archive would unpack to and whether it carries an icon,
/// if it holds a WHDLoad pack at all.
///
/// Reads the archive's **entry list only** — no decompression — so asking the
/// question of four hundred files costs four hundred directory reads rather
/// than four hundred unpacks. `analyse` is the right function here and the
/// wrong one for a folder: this is precisely the shape it was written for, one
/// drawer beside its own `.info`.
///
/// The icon half is what ART-106 needed: `apply` writes `<parent>/<name>.info`
/// beside the drawer, and a plan that did not know that could report *no*
/// collisions for a staging tree that already holds exactly that file.
fn whdload_pack(path: &Path) -> Option<(String, bool)> {
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
        .map(|layout| (layout.name, layout.icon.is_some()))
}

fn name_of(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// What laying these paths out under `root` would do. Writes nothing.
///
/// The thin wrapper, for callers with no job to report through — the same
/// shape `scan_collection_directory` / `_with` uses.
pub fn plan(root: &Path, paths: &[PathBuf], policy: &Policy) -> CoreResult<LayoutPlan> {
    plan_with(root, paths, policy, &crate::core::jobs::NoProgress)
}

/// [`plan`], reporting progress and stopping when asked.
///
/// **This is a long operation and it is measured** (§54). On the owner's own
/// collection — 1 697 WHDLoad HDFs, 3.74 GB, at `E:\amiga\Amigatolon\WHDload`:
///
/// | | items | time |
/// |---|---|---|
/// | first plan, nothing at the destination | 1 702 | 797 ms warm, 2 804 ms cold-cache |
/// | plan over a staging tree that already holds it — the **resume** case | 1 702 | **138 898 ms** |
///
/// 81.6 ms an item on the resume, because `presence_of` compares **content**
/// (ART-177's G1): every destination that already exists is read in full and
/// matched against its source, which for that collection is 3.74 GB read
/// twice. That is the right comparison — a cheaper one is how a wrong file
/// gets skipped — and it is emphatically not something to run on the command
/// thread. `commands::layout::layout_plan` is a job because of this number.
///
/// Re-run it with `core::layout::tests::layout_plan_timing_over_a_real_collection`
/// rather than trusting the table above.
///
/// Cancellation is checked **between whole items**, never inside a comparison.
pub fn plan_with(
    root: &Path,
    paths: &[PathBuf],
    policy: &Policy,
    sink: &dyn crate::core::jobs::ProgressSink,
) -> CoreResult<LayoutPlan> {
    let mut items = Vec::new();
    let mut refused = Vec::new();

    sink.report(0, None, "looking at what was dropped");
    let scanned = gather(paths)?;
    let total = scanned.found.len() as u64;

    for (done, found) in scanned.found.into_iter().enumerate() {
        if sink.is_cancelled() {
            return Err(CoreError::Cancelled);
        }
        sink.report(done as u64, Some(total), &found.path.to_string_lossy());
        let kind = classify(&found)?;
        let drawer = match drawer_for(&kind, policy) {
            Ok(drawer) => drawer,
            Err(reason) => {
                refused.push(Refusal {
                    source: found.path.clone(),
                    reason,
                });
                continue;
            }
        };

        let (placement, leaf, bytes, writes_icon) = match (&kind, policy.whdload) {
            (ItemKind::WhdloadArchive { name }, WhdloadPlacement::Unpack) => (
                Placement::UnpackWhdload,
                name.clone(),
                declared_bytes(&found.path).unwrap_or(found.bytes),
                // Asked of the archive, not assumed: plenty of packs ship
                // without an icon, and claiming a collision for a file that
                // will never be written is as wrong as missing one that will.
                whdload_pack(&found.path).is_some_and(|(_, icon)| icon),
            ),
            (ItemKind::WhdloadDrawer { name }, _) => {
                (Placement::CopyTree, name.clone(), found.bytes, false)
            }
            _ => (
                Placement::CopyFile,
                name_of(&found.path),
                found.bytes,
                false,
            ),
        };

        items.push(LayoutItem {
            source: found.path,
            kind,
            destination: format!("{drawer}/{leaf}"),
            placement,
            bytes,
            writes_icon,
        });
    }

    let (collisions, already_in_place) = settled_in_with(root, &items, sink)?;
    // Folded with a checked running total, never a plain `.sum()`, for the
    // same reason `declared_bytes` below is: an item's `bytes` can come from
    // an archive's own declared size, which is an adversarial claim.
    let bytes = items
        .iter()
        .fold(0u64, |total, item| total.saturating_add(item.bytes));

    Ok(LayoutPlan {
        root: root.to_path_buf(),
        items,
        refused,
        collisions,
        bytes,
        too_deep: scanned.too_deep,
        duplicates: scanned.duplicates,
        already_in_place,
    })
}

/// What an archive says it decompresses to. A claim, used only to show the
/// user a number; the gate measures what actually arrives.
///
/// Folded with a checked running total, never a plain `.sum()` — a declared
/// size is an adversarial claim (`core/archive/extract.rs` treats the same
/// field the same way). `saturating_add` rather than `checked_add` + `?`
/// because this total only ever reaches the screen: overflow saturates rather
/// than erroring, since a colossal declared total is still worth showing as
/// "colossal".
fn declared_bytes(path: &Path) -> Option<u64> {
    let mut backend = crate::core::archive::open(path).ok()?;
    let total = backend.entries().ok()?.iter().fold(0u64, |total, entry| {
        total.saturating_add(entry.declared_bytes)
    });
    Some(total)
}

/// Every destination two items want, and every one the tree already holds.
///
/// `pub` so `commands/layout.rs::layout_recheck` can re-ask this exact
/// question after the user retargets a row on screen, without re-walking or
/// re-classifying anything — see that command's own doc comment.
///
/// **An item's icon is a destination too** (ART-106). Applying an
/// `UnpackWhdload` item writes `<parent>/<name>.info` beside the drawer as
/// well as the drawer itself, and walking `destination` alone reported *no*
/// collisions for a staging tree that already held `Games/Turrican.info` —
/// after which `apply` silently no-ops the icon (`if !to.exists()`) and
/// places a drawer Workbench cannot see, which is the exact failure §82
/// exists to prevent, reached from the other side.
pub fn collisions_in(root: &Path, items: &[LayoutItem]) -> Vec<Collision> {
    settled_in(root, items).0
}

/// [`collisions_in`], and the destinations that are already exactly right.
///
/// One walk answers both, because they are the same question asked of the
/// same paths: what is at this destination, and is it this item's own output
/// from a run that did not finish (ART-177)? A destination that already holds
/// exactly what this item would place is **not** a collision — reporting it
/// as one is what made a half-finished apply a dead end.
pub fn settled_in(root: &Path, items: &[LayoutItem]) -> (Vec<Collision>, Vec<String>) {
    settled_in_with(root, items, &crate::core::jobs::NoProgress).expect("NoProgress never cancels")
}

/// [`settled_in`], reporting progress and stopping when asked.
///
/// This is where a plan spends its time on a resume: every destination that
/// already exists is compared byte for byte. See [`plan_with`] for the
/// measurement. Cancellation is checked between whole items — never inside a
/// comparison, which would mean a half-read answer.
pub fn settled_in_with(
    root: &Path,
    items: &[LayoutItem],
    sink: &dyn crate::core::jobs::ProgressSink,
) -> CoreResult<(Vec<Collision>, Vec<String>)> {
    use crate::core::layout::presence::{presence_of, Presence};

    let total = items.len() as u64;
    let mut already: Vec<String> = Vec::new();
    let mut by_destination: BTreeMap<String, Vec<PathBuf>> = BTreeMap::new();
    for (done, item) in items.iter().enumerate() {
        if sink.is_cancelled() {
            return Err(CoreError::Cancelled);
        }
        sink.report(done as u64, Some(total), &item.destination);
        match presence_of(root, item) {
            Presence::AlreadyInPlace => {
                already.push(item.destination.clone());
                // Its icon rides with it: `apply` writes that beside the
                // drawer in the same step, so a drawer that is already right
                // and an icon that is already there are one settled item, not
                // one settled and one clashing.
                continue;
            }
            // The drawer is right and the `.info` is missing (ART-106, and
            // the resume case G1 was filed about). There is nothing in the
            // way, so it is neither settled nor a clash — it is work still to
            // do, and it counts as such on screen. Registering the drawer as
            // a collision here would block Apply on the one run that is
            // supposed to finish the job.
            Presence::IconMissing => continue,
            Presence::Absent | Presence::Different => {}
        }
        by_destination
            .entry(item.destination.clone())
            .or_default()
            .push(item.source.clone());
        if let Some(icon) = icon_destination(item) {
            by_destination
                .entry(icon)
                .or_default()
                .push(item.source.clone());
        }
    }

    let collisions = by_destination
        .into_iter()
        .filter_map(|(destination, sources)| {
            // Untrusted like an archive entry name: the destination can be
            // typed by hand (the retarget drawer box), so this goes through
            // the same gate `apply.rs` uses rather than a bare `root.join`.
            // A destination `safe_join` refuses is not reported as a
            // collision here — `apply()` will refuse it on its own, with a
            // clearer reason than "this name is taken".
            let on_disk = safe_join(root, &destination)
                .map(|p| p.exists())
                .unwrap_or(false);
            if sources.len() > 1 || on_disk {
                Some(Collision {
                    destination,
                    sources,
                })
            } else {
                None
            }
        })
        .collect();

    Ok((collisions, already))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "art-layout-{tag}-{}",
            crate::core::test_scratch_id()
        ));
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

    /// A zip holding `Turrican/Turrican.slave` and `Turrican.info` beside the
    /// drawer — the shape a real WHDLoad archive has.
    fn whdload_zip(path: &Path) {
        let file = std::fs::File::create(path).unwrap();
        let mut zip = zip::ZipWriter::new(file);
        let options: zip::write::FileOptions<'_, ()> =
            zip::write::FileOptions::default().compression_method(zip::CompressionMethod::Stored);
        for (name, body) in [
            ("Turrican/Turrican.slave", &b"slave"[..]),
            ("Turrican/data/level1", &b"level"[..]),
            ("Turrican.info", &b"icon"[..]),
        ] {
            zip.start_file(name, options).unwrap();
            std::io::Write::write_all(&mut zip, body).unwrap();
        }
        zip.finish().unwrap();
    }

    /// A zip with no `.slave` anywhere in it — a plain archive, not a WHDLoad
    /// pack.
    fn plain_zip(path: &Path) {
        let file = std::fs::File::create(path).unwrap();
        let mut zip = zip::ZipWriter::new(file);
        let options: zip::write::FileOptions<'_, ()> =
            zip::write::FileOptions::default().compression_method(zip::CompressionMethod::Stored);
        zip.start_file("readme.txt", options).unwrap();
        std::io::Write::write_all(&mut zip, b"just a readme").unwrap();
        zip.finish().unwrap();
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

    /// The default policy unpacks a WHDLoad archive: the destination and the
    /// kind's name both come from the **pack**, from `whdload::analyse`
    /// reading the archive's entry list, not from the archive's own filename.
    #[test]
    fn a_whdload_archive_is_unpacked_under_games_by_default() {
        let dir = scratch("whdload-unpack");
        let root = dir.join("staging");
        let archive = dir.join("Turrican.zip");
        whdload_zip(&archive);

        let made = plan(&root, std::slice::from_ref(&archive), &Policy::default()).unwrap();

        assert_eq!(made.items.len(), 1, "{:?}", made.items);
        assert_eq!(
            made.items[0].kind,
            ItemKind::WhdloadArchive {
                name: "Turrican".into()
            }
        );
        assert_eq!(made.items[0].destination, "Games/Turrican");
        assert_eq!(made.items[0].placement, Placement::UnpackWhdload);
        assert!(made.refused.is_empty());

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// With `AsArchive`, the same file is copied in as itself: the placement
    /// is `CopyFile` and the destination keeps the archive's **own**
    /// filename, not the pack's name.
    #[test]
    fn as_archive_policy_copies_the_whdload_zip_in_whole() {
        let dir = scratch("whdload-as-archive");
        let root = dir.join("staging");
        let archive = dir.join("Turrican.zip");
        whdload_zip(&archive);
        let policy = Policy {
            whdload: WhdloadPlacement::AsArchive,
            ..Policy::default()
        };

        let made = plan(&root, std::slice::from_ref(&archive), &policy).unwrap();

        assert_eq!(made.items.len(), 1, "{:?}", made.items);
        assert_eq!(
            made.items[0].kind,
            ItemKind::WhdloadArchive {
                name: "Turrican".into()
            }
        );
        assert_eq!(made.items[0].destination, "Games/Turrican.zip");
        assert_eq!(made.items[0].placement, Placement::CopyFile);

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A zip with no `.slave` in it is a plain archive, not a WHDLoad pack —
    /// the branch that fires when `whdload_name` comes back `None`.
    #[test]
    fn a_zip_with_no_slave_is_a_plain_archive_into_unsorted() {
        let dir = scratch("plain-archive");
        let root = dir.join("staging");
        let archive = dir.join("stuff.zip");
        plain_zip(&archive);

        let made = plan(&root, std::slice::from_ref(&archive), &Policy::default()).unwrap();

        assert_eq!(made.items.len(), 1, "{:?}", made.items);
        assert_eq!(made.items[0].kind, ItemKind::Archive);
        assert_eq!(made.items[0].destination, "Unsorted/stuff.zip");
        assert_eq!(made.items[0].placement, Placement::CopyFile);

        let _ = std::fs::remove_dir_all(&dir);
    }

    // ---- ART-106: an item's icon is a destination too ----

    /// The staging tree already holds `Games/Turrican.info`, and the plan
    /// would unpack a pack that writes exactly that file beside its drawer.
    ///
    /// Before ART-106 the preview reported **no collisions** here: it walked
    /// `item.destination` only, `Games/Turrican` was free, and the apply then
    /// silently no-opped the icon (`if !to.exists()`) and placed a drawer
    /// Workbench cannot see — the exact failure §82 exists to prevent,
    /// reached from the other side.
    #[test]
    fn an_icon_already_in_the_staging_tree_is_a_collision() {
        let dir = scratch("icon-collision");
        let root = dir.join("staging");
        std::fs::create_dir_all(root.join("Games")).unwrap();
        std::fs::write(root.join("Games").join("Turrican.info"), b"an older icon").unwrap();

        let archive = dir.join("Turrican.zip");
        whdload_zip(&archive);

        let made = plan(&root, std::slice::from_ref(&archive), &Policy::default()).unwrap();

        assert_eq!(made.items.len(), 1);
        assert_eq!(made.items[0].destination, "Games/Turrican");
        assert!(
            !root.join("Games").join("Turrican").exists(),
            "the drawer itself is free — the clash is the icon's alone, which is what \
             makes this test discriminate"
        );

        let names: Vec<&str> = made
            .collisions
            .iter()
            .map(|c| c.destination.as_str())
            .collect();
        assert!(
            names.contains(&"Games/Turrican.info"),
            "the icon's destination has to be reported: {names:?}"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A pack that ships **no** icon writes none, so nothing about
    /// `Games/Whatever.info` is a collision. A check that assumed every
    /// unpacked item writes an icon would report a clash for a file that will
    /// never be written, which is as wrong as missing one that will.
    #[test]
    fn a_pack_with_no_icon_claims_no_icon_destination() {
        let dir = scratch("no-icon");
        let root = dir.join("staging");
        std::fs::create_dir_all(root.join("Games")).unwrap();
        std::fs::write(root.join("Games").join("Turrican.info"), b"unrelated").unwrap();

        // The same pack, minus its `.info`.
        let archive = dir.join("Turrican.zip");
        {
            let file = std::fs::File::create(&archive).unwrap();
            let mut zip = zip::ZipWriter::new(file);
            let options: zip::write::FileOptions<'_, ()> = zip::write::FileOptions::default()
                .compression_method(zip::CompressionMethod::Stored);
            zip.start_file("Turrican/Turrican.slave", options).unwrap();
            std::io::Write::write_all(&mut zip, b"slave").unwrap();
            zip.finish().unwrap();
        }

        let made = plan(&root, std::slice::from_ref(&archive), &Policy::default()).unwrap();

        assert!(!made.items[0].writes_icon);
        assert_eq!(icon_destination(&made.items[0]), None);
        assert!(
            made.collisions.is_empty(),
            "nothing here clashes: {:?}",
            made.collisions
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// F7 of the wave-C1 review: the two halves of the icon rule have to
    /// **normalise the same way**, not merely read the same field.
    ///
    /// The retarget box is free text, and `Games/Turrican/` is what a person
    /// types. `apply` builds the icon from the *target path*, which
    /// `safe_join` has already normalised, so it says `Turrican.info`; the
    /// plan side split the raw string and said `Games/Turrican/.info`. Same
    /// field, two answers — ART-176's divergence in a second place.
    #[test]
    fn a_destination_with_a_trailing_separator_names_the_icon_the_same_way() {
        let dir = scratch("icon-trailing");
        let root = dir.join("staging");
        let archive = dir.join("Turrican.zip");
        whdload_zip(&archive);

        let mut made = plan(&root, std::slice::from_ref(&archive), &Policy::default()).unwrap();
        for typed in ["Games/Turrican/", "Games/Turrican//", "Games\\Turrican\\"] {
            made.items[0].destination = typed.into();
            let expected = if typed.contains('\\') {
                "Games\\Turrican.info"
            } else {
                "Games/Turrican.info"
            };
            assert_eq!(
                icon_destination(&made.items[0]),
                Some(expected.to_string()),
                "typed as {typed:?} — a trailing separator must not become part \
                 of the icon's name"
            );
        }

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The icon moves with the row. `layout_recheck` re-asks `collisions_in`
    /// after a retarget, and an icon path stored at plan time would answer
    /// about where the row *used* to point.
    #[test]
    fn a_retargeted_row_moves_its_icon_destination_with_it() {
        let dir = scratch("icon-retarget");
        let root = dir.join("staging");
        let archive = dir.join("Turrican.zip");
        whdload_zip(&archive);

        let mut made = plan(&root, std::slice::from_ref(&archive), &Policy::default()).unwrap();
        assert_eq!(
            icon_destination(&made.items[0]),
            Some("Games/Turrican.info".to_string())
        );

        made.items[0].destination = "Games/TurricanII".into();
        assert_eq!(
            icon_destination(&made.items[0]),
            Some("Games/TurricanII.info".to_string()),
            "derived from the destination, so a retarget carries it"
        );

        // And the clash follows: put the *new* icon name on disk.
        std::fs::create_dir_all(root.join("Games")).unwrap();
        std::fs::write(root.join("Games").join("TurricanII.info"), b"in the way").unwrap();
        let found = collisions_in(&root, &made.items);
        let names: Vec<&str> = found.iter().map(|c| c.destination.as_str()).collect();
        assert_eq!(names, vec!["Games/TurricanII.info"], "{found:?}");

        let _ = std::fs::remove_dir_all(&dir);
    }

    // ---- The measurement, checked in so the number can be re-run ----------

    /// A plan stops between whole items when it is asked to, and never
    /// inside a comparison — the rule `core/jobs` states, and the reason a
    /// two-minute preview is safe to cancel rather than merely slow.
    #[test]
    fn a_plan_stops_when_asked_and_between_whole_items() {
        struct StopAtOnce;
        impl crate::core::jobs::ProgressSink for StopAtOnce {
            fn report(&self, _done: u64, _total: Option<u64>, _message: &str) {}
            fn is_cancelled(&self) -> bool {
                true
            }
        }

        let dir = scratch("cancel");
        let root = dir.join("staging");
        adf(&dir.join("Workbench.adf"));

        let err = plan_with(
            &root,
            &[dir.join("Workbench.adf")],
            &Policy::default(),
            &StopAtOnce,
        )
        .unwrap_err();
        assert_eq!(err.code(), "ART-CANCELLED", "{err}");
        assert!(!root.exists(), "planning writes nothing, cancelled or not");

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Time `plan()` over a real collection, cold and on a resume.
    ///
    /// `#[ignore]`d because it needs the owner's own material, which ART never
    /// ships. Run it as:
    ///
    /// ```text
    /// ART_LAYOUT_BENCH="E:\amiga\Amigatolon\WHDload" \
    /// ART_LAYOUT_BENCH_ROOT="E:\amiga\ProjeART\layout-bench" \
    ///   cargo test --lib layout_plan_timing -- --ignored --nocapture
    /// ```
    ///
    /// It exists because `presence_of` compares **content** (G1), so the
    /// second plan — the one a resume runs — reads every byte of every
    /// destination that already exists. That is the right comparison and it is
    /// not free, and "if it proves too slow" is how a freeze ships. The number
    /// this prints is what `layout_plan`'s own doc comment quotes.
    #[test]
    #[ignore = "needs the owner's own collection; set ART_LAYOUT_BENCH"]
    fn layout_plan_timing_over_a_real_collection() {
        use std::time::Instant;

        let Ok(source) = std::env::var("ART_LAYOUT_BENCH") else {
            eprintln!("set ART_LAYOUT_BENCH to a directory of real material");
            return;
        };
        let root = PathBuf::from(
            std::env::var("ART_LAYOUT_BENCH_ROOT")
                .expect("set ART_LAYOUT_BENCH_ROOT to a scratch staging folder"),
        );
        let _ = std::fs::remove_dir_all(&root);

        let paths = vec![PathBuf::from(&source)];

        let started = Instant::now();
        let cold = plan(&root, &paths, &Policy::default()).unwrap();
        let cold_ms = started.elapsed().as_millis();
        eprintln!(
            "cold plan: {} items, {:.2} GB, {} refused, {} ms",
            cold.items.len(),
            cold.bytes as f64 / 1_073_741_824.0,
            cold.refused.len(),
            cold_ms
        );

        // A real collection contains real refusals — a pack that still holds
        // its `Install` script, say — so a partial apply is the ordinary
        // outcome here and not a failure of the measurement. What matters is
        // that most destinations now exist.
        let started = Instant::now();
        let placed = match crate::core::layout::apply::apply(&cold, &crate::core::jobs::NoProgress)
        {
            Ok(outcome) => outcome.placed,
            Err(crate::core::error::CoreError::PartiallyApplied {
                placed,
                item,
                reason,
            }) => {
                eprintln!("apply stopped at '{item}': {reason}");
                placed as usize
            }
            Err(err) => panic!("{err}"),
        };
        eprintln!(
            "apply: {placed} placed, {} ms",
            started.elapsed().as_millis()
        );

        // The measurement that matters: every destination now exists, so
        // every item is compared byte for byte.
        let started = Instant::now();
        let warm = plan(&root, &paths, &Policy::default()).unwrap();
        let warm_ms = started.elapsed().as_millis();
        eprintln!(
            "resume plan: {} items, {} already in place, {} collisions, {} ms",
            warm.items.len(),
            warm.already_in_place.len(),
            warm.collisions.len(),
            warm_ms
        );
        eprintln!(
            "per item: cold {:.2} ms, resume {:.2} ms",
            cold_ms as f64 / cold.items.len().max(1) as f64,
            warm_ms as f64 / warm.items.len().max(1) as f64
        );

        // `>=`, not `==`: a real folder can already hold a destination this
        // plan wants (`paketler` holds two sources whose `Unsorted/` names
        // collide), and that one is recognised as already-in-place without
        // this apply having placed it. What must hold is that nothing the
        // apply *did* place comes back unrecognised.
        assert!(
            warm.already_in_place.len() >= placed,
            "everything the apply placed must be recognised on the re-plan: {} < {placed}",
            warm.already_in_place.len()
        );

        let _ = std::fs::remove_dir_all(&root);
    }
}
