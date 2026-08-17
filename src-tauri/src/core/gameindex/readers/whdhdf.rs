//! A bootable single-game hardfile, read from the inside.
//!
//! 1697 of the files in this user's collection are this shape and nothing else:
//! a bare `DOS\1` FFS volume with no RDB, holding a WHDLoad drawer and an
//! `S/startup-sequence` that runs it. `A Prehistoric Tale v1.1.hdf` is 943 616
//! bytes — 1843 blocks — and carries `Lotus3HD`-style drawer, slave, icon and
//! ReadMe.
//!
//! Two things this module exists to say, and neither can be got from the file's
//! own name:
//!
//! - **The drawer is not the title.** `Lotus3HD` and `Moonstone Install` are
//!   drawer names for games called `Lotus 3` and `Moonstone`. Only the slave
//!   knows, which is why this reader's product is a [`SlaveFacts`] and not a
//!   string. iGame's own scanner makes the same choice, with "use the parent
//!   folder name" offered as the worse alternative.
//! - **`.islave` is not the game's slave.** Both real `.lha` archives here ship
//!   one beside the real one. The extension test comes from
//!   [`crate::core::whdload::has_extension`] rather than being restated, so
//!   there is exactly one place that rule can be got wrong.
//!
//! Nothing is read whole: the volume is walked block by block through
//! `core/volume`'s `BlockDevice`, and only the slave's own bytes are pulled
//! out — bounded, because a header's declared size is a claim from an image ART
//! did not write.

use std::path::Path;

use crate::core::adf::blocks::EntryKind;
use crate::core::adf::bootblock::FileSystemType;
use crate::core::adf::extract::extract_file_on;
use crate::core::adf::fs::{list_directory_on, read_header_on};
use crate::core::error::{CoreError, CoreResult};
use crate::core::gameindex::readers::slave::{read_slave, SlaveFacts};
use crate::core::volume::mount::{mount, scan_image};
use crate::core::whdload::has_extension;

/// How deep below the volume root a slave may sit.
///
/// The same bound `core::whdload::MAX_SLAVE_DEPTH` uses for an unpacked
/// archive, and for the same reason: past this it is not a WHDLoad pack's
/// shape, and a walk of unknown depth on a malformed image does not end.
const MAX_DEPTH: usize = 3;

/// How many directory entries ART will look at before giving up.
///
/// The same ceiling `core::whdload::MAX_ENTRIES` uses for an unpacked archive.
/// It started at 4096 and one real image reached it — `Beneath A Steel Sky
/// v2.2 CD32`, which is a CD32 game and carries the files to prove it.
const MAX_ENTRIES: usize = 20_000;

/// AmigaDOS's filename limit on a normal FFS volume.
///
/// This matters because **the limit truncates the extension too**. A slave
/// called `20000MeilenUnterDemMeerDe.Slave` is 31 characters, so what is
/// actually on the disk is `20000MeilenUnterDemMeerDe.Slav` — thirty, with the
/// final `e` gone. Thirty-two of the 1697 real hardfiles are this case, and a
/// reader matching only on a whole `.slave` extension finds none of them.
const AMIGADOS_NAME_LIMIT: usize = 30;

/// The largest file ART will read while looking for a slave.
///
/// A slave is kilobytes — the two real ones here are 7 KB and 30 KB. A header
/// claiming three gigabytes is a corrupt image, not a slave, and the claim is
/// checked before anything is allocated from it.
const MAX_SLAVE_BYTES: u64 = 2 * 1024 * 1024;

/// A game found inside a hardfile.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HardfileGame {
    /// The drawer the slave sits in. Empty when the slave is at the volume
    /// root, which `core::whdload` also allows.
    pub drawer: String,
    /// The slave's own filename, as AmigaDOS spells it — `.slave` or `.Slave`.
    pub slave_name: String,
    pub slave: SlaveFacts,
    /// `<drawer>.info`, beside the drawer rather than inside it (§82).
    pub icon: Option<String>,
}

impl HardfileGame {
    /// The best name available: what the slave states, or the drawer's name
    /// when it states none.
    ///
    /// The fallback is deliberately last. `Moonstone Install` is what this
    /// returns only when a slave is too old to carry `ws_name`.
    pub fn best_name(&self) -> &str {
        self.slave
            .name
            .as_deref()
            .filter(|name| !name.is_empty())
            .unwrap_or(&self.drawer)
    }
}

/// One directory waiting to be walked.
struct Pending {
    block: u32,
    /// The directory's own name. Empty for the volume root.
    name: String,
    /// The block of the directory holding this one — where its icon lives.
    parent_block: u32,
    depth: usize,
}

/// A slave found during the walk, with enough context to name its drawer.
struct Candidate {
    name: String,
    header_block: u32,
    byte_size: u64,
    drawer: String,
    /// The directory holding the drawer, which is where `<drawer>.info` sits.
    drawer_parent: u32,
    depth: usize,
}

/// Whether a filename is worth opening to see if it is a slave.
///
/// The exact `.slave` extension first, from
/// [`crate::core::whdload::has_extension`] so `.islave` is excluded in one
/// place only. Then the truncated case: a name at AmigaDOS's thirty-character
/// limit whose extension is a **prefix** of `slave`, which is what
/// `…MeerDe.Slave` becomes on a real disk.
///
/// This is a filter, not a decision. What actually settles it is
/// [`crate::core::gameindex::readers::slave::read_slave`] refusing anything
/// without a `WHDLOADS` structure — the same "a file is what its bytes say"
/// rule `core/detect` was rebuilt around, applied one level down.
fn names_a_slave(name: &str) -> bool {
    if has_extension(name, "slave") {
        return true;
    }

    // A shorter extension is only plausible when the name was cut, so require
    // the name to be *at* the limit. Four characters is the floor: `.sla` and
    // below are common enough elsewhere to be worth opening files for.
    if name.chars().count() != AMIGADOS_NAME_LIMIT {
        return false;
    }
    match name.rsplit_once('.') {
        Some((stem, ext)) if !stem.is_empty() && ext.len() >= 4 => {
            "slave".starts_with(&ext.to_ascii_lowercase())
        }
        _ => false,
    }
}

fn malformed(detail: impl std::fmt::Display) -> CoreError {
    CoreError::Malformed {
        format: "whdload-hardfile".into(),
        detail: detail.to_string(),
    }
}

/// Read the game inside a bootable WHDLoad hardfile.
pub fn read_whdload_hardfile(path: &Path) -> CoreResult<HardfileGame> {
    let scanned = scan_image(path)?;
    let mountable: Vec<_> = scanned
        .volumes
        .iter()
        .filter(|volume| volume.is_mountable())
        .collect();

    let entry = match mountable.as_slice() {
        [only] => *only,
        [] => return Err(malformed("no mountable volume in this image")),
        // A card is a list of disks; a single-game hardfile is one volume. If
        // there are several this is something else, and guessing which one
        // holds the game is not a decision to make silently.
        many => {
            return Err(malformed(format!(
                "{} volumes here, so this is not a single-game hardfile",
                many.len()
            )))
        }
    };

    let (device, geometry) = mount(path, entry)?;
    let fs_type = if geometry.dos_type.is_ffs() {
        FileSystemType::Ffs
    } else {
        FileSystemType::Ofs
    };

    let mut queue = vec![Pending {
        block: geometry.root_block,
        name: String::new(),
        parent_block: geometry.root_block,
        depth: 0,
    }];
    let mut candidates: Vec<Candidate> = Vec::new();
    let mut seen = 0usize;

    while let Some(dir) = queue.pop() {
        for found in list_directory_on(&device, dir.block)? {
            seen += 1;
            if seen > MAX_ENTRIES {
                return Err(malformed(
                    "this image holds more entries than ART will walk",
                ));
            }

            match found.kind {
                EntryKind::Directory if dir.depth < MAX_DEPTH => queue.push(Pending {
                    block: found.header_block,
                    name: found.name,
                    parent_block: dir.block,
                    depth: dir.depth + 1,
                }),
                EntryKind::Directory => {}
                EntryKind::File if names_a_slave(&found.name) => {
                    candidates.push(Candidate {
                        name: found.name,
                        header_block: found.header_block,
                        byte_size: found.byte_size,
                        drawer: dir.name.clone(),
                        drawer_parent: dir.parent_block,
                        depth: dir.depth,
                    });
                }
                EntryKind::File => {}
            }
        }
    }

    // The shallowest wins, then the shortest name — the tie-break
    // `core::whdload::pick_slave` already uses, so a pack shipping
    // `Game.slave` and `GameNTSC.slave` resolves the same way in both readers.
    candidates.sort_by(|a, b| {
        a.depth
            .cmp(&b.depth)
            .then_with(|| a.name.len().cmp(&b.name.len()))
            .then_with(|| a.name.cmp(&b.name))
    });
    if candidates.is_empty() {
        return Err(malformed("no .slave anywhere in this image"));
    }

    // The name got a file onto the shortlist; its **bytes** decide. Trying each
    // in turn means a truncated name that is not really a slave costs one read
    // rather than the whole image, and a drawer holding both a slave and
    // something else called `.slav` resolves without a second naming rule.
    let mut refusal = None;
    let mut winner = None;
    for candidate in &candidates {
        if candidate.byte_size > MAX_SLAVE_BYTES {
            refusal.get_or_insert_with(|| {
                malformed(format!(
                    "'{}' claims {} bytes, too large for a slave",
                    candidate.name, candidate.byte_size
                ))
            });
            continue;
        }

        let header = read_header_on(&device, candidate.header_block)?;
        let bytes = extract_file_on(&device, &header, fs_type)?;
        match read_slave(&bytes) {
            Ok(slave) => {
                winner = Some((candidate, slave));
                break;
            }
            Err(err) => {
                refusal.get_or_insert(err);
            }
        }
    }

    let (winner, slave) = match winner {
        Some(found) => found,
        None => return Err(refusal.unwrap_or_else(|| malformed("no readable slave in this image"))),
    };

    // §82: the icon sits *beside* the drawer, not inside it. At the volume root
    // there is no drawer, and the slave's own stem is the name — which is what
    // `core::whdload::analyse` does with a wrapper-less archive.
    let drawer = if winner.drawer.is_empty() {
        stem_of(&winner.name).to_string()
    } else {
        winner.drawer.clone()
    };
    let wanted_icon = format!("{drawer}.info");
    let icon = list_directory_on(&device, winner.drawer_parent)?
        .into_iter()
        .find(|entry| {
            entry.kind == EntryKind::File && entry.name.eq_ignore_ascii_case(&wanted_icon)
        })
        .map(|entry| entry.name);

    Ok(HardfileGame {
        drawer,
        slave_name: winner.name.clone(),
        slave,
        icon,
    })
}

fn stem_of(name: &str) -> &str {
    match name.rfind('.') {
        Some(dot) if dot > 0 => &name[..dot],
        _ => name,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::gameindex::readers::slave::tests_support::build_slave;
    use crate::core::volume::device::FileRegionMut;
    use crate::core::volume::fixture::make_ffs_volume;
    use crate::core::volume::write::{FileMeta, VolumeWriter};
    use crate::core::volume::{DosType, VolumeGeometry};
    use std::path::PathBuf;

    /// 1843 blocks is what `A Prehistoric Tale v1.1.hdf` actually is, so the
    /// fixture is the real collection's size rather than a round one.
    const TOTAL_BLOCKS: u32 = 1843;

    fn scratch(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "art-whdhdf-{tag}-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn empty_volume(dir: &Path, name: &str) -> PathBuf {
        let image = dir.join(format!("{name}.hdf"));
        std::fs::write(&image, make_ffs_volume(TOTAL_BLOCKS, "Games", &[])).unwrap();
        image
    }

    fn geometry() -> VolumeGeometry {
        VolumeGeometry::new(512, TOTAL_BLOCKS, 2, DosType::new(*b"DOS\x01")).unwrap()
    }

    /// Build a hardfile holding `<drawer>/<stem>.slave` with `<drawer>.info`
    /// beside the drawer — the shape every one of the 1697 real images has.
    ///
    /// The volume comes from `core/volume/fixture` and the contents go in
    /// through `core/volume/write`, so the fixture is written by ART's own
    /// filesystem writer. A fixture built by a second implementation would only
    /// prove that the two agree with each other.
    fn build_hardfile(
        dir: &Path,
        drawer: &str,
        slave_stem: &str,
        slave: &[u8],
        extra: Option<(&str, &[u8])>,
    ) -> PathBuf {
        let image = empty_volume(dir, drawer);

        // `FileRegionMut`, not `FileRegion`: `VolumeWriter::open` takes a
        // `&mut dyn BlockDeviceMut`, and the mutable region refuses a length
        // running past the file rather than clamping it — the right way round
        // when something is about to be written.
        let mut device = FileRegionMut::open(&image, 0, TOTAL_BLOCKS as u64 * 512, 512).unwrap();
        let mut writer = VolumeWriter::open(&mut device, geometry(), &image, 0).unwrap();
        let root = writer.geometry().root_block;

        writer.make_dir(root, drawer).unwrap();
        // `DirEntry`'s field is `block` — the header block of the entry.
        let drawer_block = writer.find(root, drawer).unwrap().unwrap().block;

        writer
            .add_file(
                drawer_block,
                &format!("{slave_stem}.slave"),
                slave,
                FileMeta::default(),
            )
            .unwrap();
        if let Some((name, bytes)) = extra {
            writer
                .add_file(drawer_block, name, bytes, FileMeta::default())
                .unwrap();
        }
        writer
            .add_file(
                root,
                &format!("{drawer}.info"),
                b"icon",
                FileMeta::default(),
            )
            .unwrap();

        drop(writer);
        image
    }

    /// The reader finds the drawer, the slave, and what the slave says — not
    /// what the file is called. `Lotus3HD` is a drawer name; `Lotus 3` is the
    /// game, and only the slave knows that.
    #[test]
    fn a_bootable_hardfile_names_the_game_from_its_slave() {
        let dir = scratch("read");
        let slave = build_slave("Lotus 3", "1992 Gremlin", 16);
        let image = build_hardfile(&dir, "Lotus3HD", "Lotus3", &slave, None);

        let game = read_whdload_hardfile(&image).unwrap();
        assert_eq!(game.drawer, "Lotus3HD");
        assert_eq!(game.slave_name, "Lotus3.slave");
        assert_eq!(game.slave.name.as_deref(), Some("Lotus 3"));
        assert_eq!(game.icon.as_deref(), Some("Lotus3HD.info"));
        assert_eq!(game.best_name(), "Lotus 3", "the drawer is not the title");

        std::fs::remove_dir_all(&dir).ok();
    }

    /// An image with no slave anywhere is refused, not reported as a game with
    /// an empty name.
    #[test]
    fn a_hardfile_with_no_slave_is_refused() {
        let dir = scratch("empty");
        let image = empty_volume(&dir, "nothing");

        let err = read_whdload_hardfile(&image).unwrap_err();
        assert!(err.to_string().contains("no .slave"), "{err}");

        std::fs::remove_dir_all(&dir).ok();
    }

    /// `.islave` is an installer's slave, not the game's. `Lotus3.lha` on this
    /// machine ships `Lotus3.islave` beside `Lotus3.slave`, so this is the real
    /// collection's shape and not a hypothetical.
    #[test]
    fn an_installer_slave_is_not_mistaken_for_the_game() {
        let dir = scratch("islave");
        let slave = build_slave("Lotus 3", "1992 Gremlin", 16);
        let image = build_hardfile(
            &dir,
            "Lotus3HD",
            "Lotus3",
            &slave,
            Some(("Lotus3.islave", b"not the game")),
        );

        assert_eq!(
            read_whdload_hardfile(&image).unwrap().slave_name,
            "Lotus3.slave"
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    /// A slave whose name AmigaDOS truncated is still found.
    ///
    /// `20000MeilenUnterDemMeerDe.Slave` is thirty-one characters, so what is
    /// on the real disk is `20000MeilenUnterDemMeerDe.Slav` — the limit ate the
    /// extension's last letter. Thirty-two of the 1697 real hardfiles are this
    /// case, and a reader matching only a whole `.slave` extension found none
    /// of them.
    #[test]
    fn a_name_truncated_by_amigados_is_still_a_slave() {
        let dir = scratch("truncated");
        let slave = build_slave("20000 Leagues Under The Sea", "1988 Coktel Vision", 16);

        // The name below is exactly thirty characters, as AmigaDOS left it.
        let stem = "20000MeilenUnterDemMeerDe";
        let truncated = format!("{stem}.Slav");
        assert_eq!(
            truncated.chars().count(),
            30,
            "the fixture must be at the limit"
        );

        let image = empty_volume(&dir, "Meer");
        let mut device = FileRegionMut::open(&image, 0, TOTAL_BLOCKS as u64 * 512, 512).unwrap();
        let mut writer = VolumeWriter::open(&mut device, geometry(), &image, 0).unwrap();
        let root = writer.geometry().root_block;
        writer.make_dir(root, stem).unwrap();
        let drawer_block = writer.find(root, stem).unwrap().unwrap().block;
        writer
            .add_file(drawer_block, &truncated, &slave, FileMeta::default())
            .unwrap();
        drop(writer);

        let game = read_whdload_hardfile(&image).unwrap();
        assert_eq!(game.slave_name, truncated);
        assert_eq!(game.best_name(), "20000 Leagues Under The Sea");

        std::fs::remove_dir_all(&dir).ok();
    }

    /// A thirty-character name whose extension merely looks slave-ish does not
    /// become a game.
    ///
    /// The name gets a file onto the shortlist; the bytes decide. This is the
    /// half that keeps the truncation rule from turning any long filename into
    /// a slave.
    #[test]
    fn a_truncated_name_without_a_slave_inside_is_not_a_game() {
        let dir = scratch("pretender");
        let stem = "SomethingWithAVeryLongName";
        let pretender = format!("{stem}.Sla");
        let image = empty_volume(&dir, "Pretend");

        let mut device = FileRegionMut::open(&image, 0, TOTAL_BLOCKS as u64 * 512, 512).unwrap();
        let mut writer = VolumeWriter::open(&mut device, geometry(), &image, 0).unwrap();
        let root = writer.geometry().root_block;
        writer
            .add_file(root, &pretender, b"not a slave at all", FileMeta::default())
            .unwrap();
        drop(writer);

        assert!(read_whdload_hardfile(&image).is_err());

        std::fs::remove_dir_all(&dir).ok();
    }

    /// A slave whose header claims an absurd size is refused before it is read.
    /// An image is untrusted input and `byte_size` in a header is a claim.
    #[test]
    fn a_slave_claiming_an_absurd_size_is_refused() {
        let dir = scratch("huge");
        let slave = build_slave("Fine", "1992 Someone", 16);
        let image = build_hardfile(&dir, "Big", "Big", &slave, None);

        inflate_declared_size(&image, "Big.slave", 3 * 1024 * 1024 * 1024);

        let err = read_whdload_hardfile(&image).unwrap_err();
        assert!(err.to_string().contains("too large"), "{err}");

        std::fs::remove_dir_all(&dir).ok();
    }

    /// Rewrite a file header's declared `byte_size`, re-checksumming the block
    /// so the test proves the size check rather than the checksum check.
    fn inflate_declared_size(image: &Path, file_name: &str, claimed: u64) {
        use crate::core::volume::fixture::checksum_block;

        let mut bytes = std::fs::read(image).unwrap();
        let geometry = geometry();

        // Find the header block by walking for the name, the same way the
        // reader does — a hard-coded block number would rot the moment the
        // fixture changed.
        let device = FileRegionMut::open(image, 0, TOTAL_BLOCKS as u64 * 512, 512).unwrap();
        let mut queue = vec![geometry.root_block];
        let mut target = None;
        while let Some(block) = queue.pop() {
            for entry in list_directory_on(&device, block).unwrap() {
                match entry.kind {
                    EntryKind::Directory => queue.push(entry.header_block),
                    EntryKind::File if entry.name == file_name => target = Some(entry.header_block),
                    EntryKind::File => {}
                }
            }
        }
        drop(device);

        let block = target.expect("the fixture should hold that file");
        // `byte_size` is longword 81 of the header block (offset 324).
        let at = block as usize * 512 + 324;
        bytes[at..at + 4].copy_from_slice(&(claimed as u32).to_be_bytes());
        checksum_block(&mut bytes, block);
        std::fs::write(image, bytes).unwrap();
    }

    /// Real hardfiles, read through the product's own path. `#[ignore]`d and
    /// env-gated: ART ships no copyrighted content.
    ///
    /// ```text
    /// cd src-tauri && ART_WHDHDF_DIR="E:\amiga\Amigatolon\WHDload" \
    ///   cargo test read_the_real_hardfiles_when_asked -- --nocapture --ignored
    /// ```
    ///
    /// **The number that matters is the refusal count.** Anything above a
    /// handful of 1697 means this reader's assumptions about the collection are
    /// wrong, and wave 2 needs to know before it starts placing anything.
    #[test]
    #[ignore]
    fn read_the_real_hardfiles_when_asked() {
        let Ok(dir) = std::env::var("ART_WHDHDF_DIR") else {
            eprintln!("ART_WHDHDF_DIR unset — skipping");
            return;
        };

        let found = walk_for_hardfiles(Path::new(&dir));
        let mut read = 0usize;
        let mut named_by_slave = 0usize;
        let mut with_icon = 0usize;
        let mut reasons: std::collections::BTreeMap<String, usize> = Default::default();

        for entry in &found {
            match read_whdload_hardfile(entry) {
                Ok(game) => {
                    read += 1;
                    if game.slave.name.is_some() {
                        named_by_slave += 1;
                    }
                    if game.icon.is_some() {
                        with_icon += 1;
                    }
                    if read <= 10 {
                        println!(
                            "{:<44} -> {:?} (drawer {:?}, v{})",
                            entry.file_name().unwrap_or_default().to_string_lossy(),
                            game.best_name(),
                            game.drawer,
                            game.slave.version
                        );
                    }
                }
                Err(err) => {
                    let reason = err.to_string();
                    *reasons.entry(reason.clone()).or_default() += 1;
                    println!(
                        "REFUSED {:<52} {reason}",
                        entry.file_name().unwrap_or_default().to_string_lossy()
                    );
                }
            }
        }

        let refused: usize = reasons.values().sum();
        println!(
            "\n{} hardfiles, {read} read, {refused} refused",
            found.len()
        );
        println!(
            "{named_by_slave} named by their slave, {with_icon} with an icon beside the drawer"
        );
        for (reason, count) in &reasons {
            println!("    x{count}  {reason}");
        }
        assert!(!found.is_empty(), "no .hdf files found under {dir}");
    }

    fn walk_for_hardfiles(dir: &Path) -> Vec<PathBuf> {
        let mut out = Vec::new();
        let Ok(entries) = std::fs::read_dir(dir) else {
            return out;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                out.extend(walk_for_hardfiles(&path));
            } else if path
                .extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| e.eq_ignore_ascii_case("hdf"))
            {
                out.push(path);
            }
        }
        out.sort();
        out
    }
}
