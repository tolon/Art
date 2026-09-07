//! Reading a card's first-boot report back (task 10 / task 10b).
//!
//! `core::firstboot::write` puts the dispatcher into a distribution tree;
//! once that card has actually booted on a real Amiga, the machine leaves
//! its own answer in two places — `S/FirstBoot.log` on the Amiga volume
//! itself, and a copy at `art-firstboot.log` on the FAT boot partition,
//! where Windows can see it without mounting an Amiga filesystem at all
//! (see `core::firstboot`'s own module doc for the tree layout). This module
//! is the read, so the card screen can show what actually happened without
//! opening WinUAE.
//!
//! Spec §9: the Amiga volume's own copy wins where both exist, because the
//! FAT copy is secondary — a fallback for a Windows user who has no way to
//! read PFS3 or FFS. [`read_card_report`] tries the Amiga volume first
//! ([`amiga_report`]) and only falls back to the FAT copy
//! ([`fat_report`]) when nothing on the Amiga side carries the file.
//!
//! The Amiga-volume read reuses the exact per-area device/geometry setup
//! `core::osinstall::verify::verify_volume` uses — see that module's own doc
//! comment for what each family can honestly claim. This module asks less of
//! it than `verify_volume` does: there is no manifest to check against, only
//! a single well-known path (`S/FirstBoot.log`) to look up and, if it is
//! there, read.
//!
//! ## Three endings, not two (fix round 1)
//!
//! "Not booted" and "could not be checked" are different sentences and must
//! stay different — CLAUDE.md's "endings stay distinct". A card whose only
//! Amiga partition is corrupt or unformatted has not told ART "no report
//! yet"; it has told ART nothing at all, and reporting `ReportSource::None`
//! about it would be the same wire shape a genuinely untouched card
//! produces. So [`amiga_report`] treats "no such file here" and "this is not
//! a filesystem ART reads" (`DosFamily::Other`) as silent, ordinary misses —
//! another partition, or the FAT copy, may still answer — but any other
//! failure (a corrupt or unformatted volume, a report too large to read) is
//! carried forward and, if nothing else on the card ever answers, returned
//! as `Err` naming the partition. `read_card_report` and the
//! `card_firstboot_report` command let that `Err` propagate rather than
//! folding it into `ReportSource::None`; Task 11 renders it as a command
//! error under the panel heading.
//!
//! A read never opens the card for writing. `fat_report` wraps the file in
//! [`crate::core::fat32::ReadOnly`] before handing it to
//! [`crate::core::fat32::Region`] — see that type's own doc comment for why.
//! The Amiga-volume side is already read-only end to end: `FileRegion` (FFS)
//! and `libpfs3::volume::Volume::open` (PFS3) both open their underlying file
//! without a write handle at all.

use std::path::Path;

use serde::Serialize;

use crate::core::card::{read_card, AmigaArea, CardImage};
use crate::core::error::{CoreError, CoreResult};
use crate::core::fat32::{read_root_file, ReadOnly, Region};
use crate::core::preload::native::{family_of, from_pfs3, partition_region, DosFamily};
use crate::core::rdb::ParsedPartition;
use crate::core::volume::device::FileRegion;
use crate::core::volume::write::layout::{self, BlockSet};
use crate::core::volume::write::{dir, file, write_refusal};
use crate::core::volume::{BlockDevice, DosType, VolumeGeometry};

use super::report::{parse_bytes, FirstBootReport};
use super::{FAT_REPORT_NAME, REPORT_PATH};

/// A first-boot report is a few hundred bytes. Anything past this on either
/// side of the card is refused rather than read — the same rule
/// `fat32::MAX_ROOT_FILE_BYTES` applies to the FAT copy, and for the same
/// reason: a card is untrusted input, and a length field taken from it is
/// never trusted enough to drive an unbounded allocation.
const MAX_REPORT_BYTES: u64 = 1 << 20;

/// Where the report [`read_card_report`] returned actually came from.
///
/// Serialises kebab-case, matching every other tagged enum in
/// `core::firstboot` ([`super::report::Ending`], [`super::report::StepOutcome`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ReportSource {
    /// The Amiga volume's own `S/FirstBoot.log` — the one spec §9 says wins,
    /// and the one [`read_card_report`] tries first.
    AmigaVolume,
    /// The FAT boot partition's `art-firstboot.log` — a fallback for a
    /// Windows user who cannot otherwise read the Amiga volume.
    Fat,
    /// Neither carried a report — the card has not booted the first-boot
    /// block yet. Not an error: [`FirstBootReport::ending`] says so as
    /// [`super::report::Ending::NotBooted`].
    None,
}

/// What [`read_card_report`] found: where it came from, and the report
/// itself, parsed exactly as [`super::report::parse`] would parse the file
/// on either side.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CardFirstBootReport {
    pub source: ReportSource,
    pub report: FirstBootReport,
}

/// Read a card's first-boot report back.
///
/// The Amiga volume's own copy wins where both exist (spec §9); the FAT copy
/// is tried only when nothing on the Amiga side carries the file.
///
/// Three endings, kept distinct (fix round 1's own section in the module
/// doc): the image will not even open (a hard `Err`, nothing here could be
/// read at all); a partition ART could read carried no file but nothing else
/// on the card went wrong either (`ReportSource::None`,
/// [`super::report::Ending::NotBooted`] — not an error, the ordinary shape
/// of a card that has not booted the block yet); or a partition ART should
/// have been able to read (PFS3, FFS/OFS) came back corrupt, unformatted, or
/// carrying a report too large to read, and nothing else on the card ever
/// answered — also an `Err`, but a different one: "could not be checked",
/// never quietly folded into "not booted".
pub fn read_card_report(path: &Path) -> CoreResult<CardFirstBootReport> {
    let card = read_card(path)?;

    if let Some(bytes) = amiga_report(&card)? {
        return Ok(CardFirstBootReport {
            source: ReportSource::AmigaVolume,
            report: parse_bytes(&bytes),
        });
    }

    if let Some(bytes) = fat_report(&card, path)? {
        return Ok(CardFirstBootReport {
            source: ReportSource::Fat,
            report: parse_bytes(&bytes),
        });
    }

    Ok(CardFirstBootReport {
        source: ReportSource::None,
        report: parse_bytes(b""),
    })
}

/// The FAT boot partition's copy, or `None` when there is no FAT32 partition
/// on this card or it carries no report yet.
fn fat_report(card: &CardImage, path: &Path) -> CoreResult<Option<Vec<u8>>> {
    let Some(boot) = card.mbr.as_ref().and_then(|mbr| mbr.boot_partition()) else {
        return Ok(None);
    };

    // Read-only end to end: this is a look at the user's own card, not an
    // operation on it.
    let file = std::fs::File::open(path)?;
    let mut region = Region::new(ReadOnly::new(file), boot.start_bytes(), boot.length_bytes());
    read_root_file(&mut region, FAT_REPORT_NAME)
}

// ---------------------------------------------------------------------------
// The Amiga volume's own copy (task 10b)
// ---------------------------------------------------------------------------

/// The Amiga volume's own `S/FirstBoot.log` — task 10b, and the copy spec §9
/// says wins.
///
/// Tries every partition on every area, in the order `read_card` reported
/// them, for the two families this module can read at all (PFS3, FFS/OFS);
/// **first hit wins, and nothing short-circuits the search except a hit** —
/// not even a failure on an earlier partition, because another partition
/// further along may still answer (fix round 1's second test: a corrupt
/// first partition must not hide a good report on the second).
///
/// Two outcomes are not failures and are never carried forward: `Ok(None)`
/// from a partition simply not carrying the file (`pfs3_report`/`ffs_report`
/// already return that for "no such file", a directory where the file
/// should be, or a shape `write_refusal` declines), and `DosFamily::Other` —
/// a filesystem this module has no reader for at all. Everything else `Err`
/// is a genuine problem — corrupt, unformatted, a report too large to read —
/// and is logged and remembered; if the whole search ends with no hit, the
/// **first** such failure is what `amiga_report` returns, naming the
/// partition it came from. See the module doc's "Three endings, not two".
fn amiga_report(card: &CardImage) -> CoreResult<Option<Vec<u8>>> {
    let path = Path::new(&card.path);
    let mut failure: Option<(usize, String, CoreError)> = None;

    for (area_index, area) in card.areas.iter().enumerate() {
        for part in &area.rdb.partitions {
            let dos = DosType::new(part.dostype.to_be_bytes());
            let found = match family_of(dos) {
                DosFamily::Pfs3 => pfs3_report(path, area, part),
                DosFamily::Ffs => ffs_report(path, area, part, dos),
                DosFamily::Other => Ok(None),
            };
            match found {
                Ok(Some(bytes)) => return Ok(Some(bytes)),
                Ok(None) => continue,
                Err(err) => {
                    log::warn!(
                        "first-boot report: area {area_index} partition '{}' ({}) could not \
                         be read: {err}",
                        part.drive_name,
                        dos.label(),
                    );
                    if failure.is_none() {
                        failure = Some((area_index, part.drive_name.clone(), err));
                    }
                }
            }
        }
    }

    match failure {
        None => Ok(None),
        // A report that was actually *found*, and is simply too large, is
        // already a complete, actionable sentence — return it as itself
        // rather than folding it into the generic "could not be checked"
        // wording below, which would bury the one detail (the size) a user
        // could act on.
        Some((_, _, err @ CoreError::LimitExceeded { .. })) => Err(err),
        Some((area_index, name, err)) => Err(CoreError::Malformed {
            format: "card".into(),
            detail: format!(
                "the Amiga volume's own first-boot report could not be checked: area \
                 {area_index} partition '{name}': {err}"
            ),
        }),
    }
}

fn too_large(size: u64) -> CoreError {
    CoreError::LimitExceeded {
        subject: "an Amiga volume's first-boot report".into(),
        detail: format!(
            "'{REPORT_PATH}' is {size} bytes, more than ART reads as a report \
             ({MAX_REPORT_BYTES} bytes)"
        ),
    }
}

/// `S/FirstBoot.log` off a PFS3 partition. Read-only: `Volume::open` never
/// takes a write handle.
///
/// **The same weak witness `core::osinstall::verify`'s own module doc names
/// in Decision 2, and disclosed here for the same reason it is disclosed
/// there.** `lookup` (presence) and `entry.file_size()` (size) are a
/// directory entry PFS3's own on-disk structure carries, worth trusting the
/// way `verify_pfs3_one` trusts them. **The bytes this function returns are
/// not the same kind of proof.** `read_file_data` walks an anode chain with
/// `libpfs3` — the very library `core::preload::native::copy_in_pfs3` used
/// to *write* that chain on every fixture in this module's own test suite,
/// and on a real card built by `core::preload`. A bug the writer and this
/// reader share (ART-079's shape) would agree with itself and pass every
/// test here; `verify.rs`'s Decision 2 declines to re-hash PFS3 content for
/// exactly this reason and this function inherits the same limit. **What
/// this closes and what it does not**: a real card is written by a real
/// Amiga running the dispatcher, not by `NativeFormatter`, so a fixture built
/// through `core::preload::native` proves this code path runs and returns
/// *something* — it does not independently prove the bytes it returns are
/// the bytes a real Amiga wrote. That only closes when a real card, actually
/// booted, is read back through this path.
fn pfs3_report(
    path: &Path,
    area: &AmigaArea,
    part: &ParsedPartition,
) -> CoreResult<Option<Vec<u8>>> {
    let (offset, _length, _block_size) = partition_region(area, part)?;
    let mut vol = libpfs3::volume::Volume::open(path, offset).map_err(from_pfs3)?;
    let Some(entry) = vol.lookup(REPORT_PATH).map_err(from_pfs3)? else {
        return Ok(None);
    };
    if entry.is_dir() {
        return Ok(None);
    }
    let size = entry.file_size();
    if size > MAX_REPORT_BYTES {
        return Err(too_large(size));
    }
    let bytes = vol.read_file_data(entry.anode, size).map_err(from_pfs3)?;
    Ok(Some(bytes))
}

/// `S/FirstBoot.log` off an FFS/OFS partition — the same `FileRegion` +
/// `VolumeGeometry` + `BlockSet` setup `core::osinstall::verify::verify_ffs_files`
/// uses, and the same `dir::find_entry`/`file::read_file` free functions,
/// never `VolumeWriter` (which wants a write handle for no reason a read
/// ever has).
fn ffs_report(
    path: &Path,
    area: &AmigaArea,
    part: &ParsedPartition,
    dos: DosType,
) -> CoreResult<Option<Vec<u8>>> {
    let (offset, length, block_size) = partition_region(area, part)?;
    let region = FileRegion::open(path, offset, length, block_size)?;
    let total_blocks = region.total_blocks();
    let geometry = VolumeGeometry::new(block_size, total_blocks, part.reserved, dos)?;
    if write_refusal(&geometry).is_some() {
        return Ok(None);
    }

    let set = BlockSet::new(geometry.block_size);
    let mut current = geometry.root_block;
    let mut is_dir = true; // the root itself is a directory
    for segment in REPORT_PATH.split('/') {
        match dir::find_entry(&region, &set, &geometry, current, segment)? {
            Some(entry) => {
                current = entry.block;
                is_dir = entry.is_dir;
            }
            None => return Ok(None),
        }
    }
    // A directory named like the report is not the report — the same check
    // `pfs3_report` makes on `entry.is_dir()`.
    if is_dir {
        return Ok(None);
    }

    // Peeked before `file::read_file` reads the whole thing, the same reason
    // `fat32::read_root_file` checks a length before allocating for it: a
    // report is a few hundred bytes, and a length field off the card is never
    // trusted enough on its own to drive an allocation.
    let header = set.view(&region, current)?;
    let size = layout::get_u32(&header, layout::BYTE_SIZE_OFFSET)? as u64;
    if size > MAX_REPORT_BYTES {
        return Err(too_large(size));
    }

    let bytes = file::read_file(&region, &set, &geometry, current)?;
    Ok(Some(bytes))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::fat32::{create_boot_partition, DEFAULT_LABEL};
    use crate::core::mbr::{plan_card, write_mbr};
    use crate::core::ScratchDir;
    use std::io::Write;

    /// A card with only the FAT32 boot partition actually laid out — there is
    /// no RDB written on it, which `read_card` tolerates (it simply finds no
    /// Amiga area), and that is exactly the shape this test wants: a card
    /// with nothing on the Amiga side to confuse the FAT-only read task 10
    /// implements.
    fn card_with_fat_report(dir: &std::path::Path, contents: Option<&[u8]>) -> std::path::PathBuf {
        let total = 400 * 1024 * 1024u64;
        let boot_bytes = 300 * 1024 * 1024u64;
        let layout = plan_card(total, boot_bytes, &[0]).unwrap();
        let mbr_sector = write_mbr(&layout);
        let boot = layout.boot;

        let path = dir.join("card.img");
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .truncate(true)
            .read(true)
            .write(true)
            .open(&path)
            .unwrap();
        file.set_len(total).unwrap();
        file.write_all(&mbr_sector).unwrap();

        create_boot_partition(
            &mut file,
            boot.start_bytes(),
            boot.length_bytes(),
            DEFAULT_LABEL,
            &[],
        )
        .unwrap();

        if let Some(bytes) = contents {
            let mut region = Region::new(&mut file, boot.start_bytes(), boot.length_bytes());
            let fs = fatfs::FileSystem::new(&mut region, fatfs::FsOptions::new()).unwrap();
            let mut report = fs.root_dir().create_file(FAT_REPORT_NAME).unwrap();
            report.write_all(bytes).unwrap();
            report.flush().unwrap();
        }

        path
    }

    #[test]
    fn a_cards_fat_copy_reads_back_as_the_fat_source() {
        let dir = ScratchDir::new("art-firstboot-cardread", "fat-present");
        let path = card_with_fat_report(dir.path(), Some(b"art-firstboot 1\ndone all\n"));

        let found = read_card_report(&path).unwrap();
        assert_eq!(found.source, ReportSource::Fat);
        assert_eq!(found.report.ending, super::super::report::Ending::DoneAll);
    }

    #[test]
    fn a_card_with_no_report_yet_is_not_booted_and_not_an_error() {
        let dir = ScratchDir::new("art-firstboot-cardread", "fat-absent");
        let path = card_with_fat_report(dir.path(), None);

        let found = read_card_report(&path).unwrap();
        assert_eq!(found.source, ReportSource::None);
        assert_eq!(found.report.ending, super::super::report::Ending::NotBooted);
    }

    // ---- the Amiga volume's own copy (task 10b) ----

    use crate::core::jobs::NoProgress;
    use crate::core::preload::{native::NativeFormatter, VolumeFormatter};
    use crate::core::rdb::{create_rdb_layout, AmigaHardDiskFs, PartitionSpec};
    use std::io::{Seek, SeekFrom};

    /// A plain HDF (no MBR, one Amiga disk at offset zero) with one
    /// partition of `fs`, sized `mb` megabytes. `RDB_HEADROOM_MB` past `mb`
    /// for the RDB's own reserved cylinders — the same headroom
    /// `core::osinstall::verify`'s own fixtures use, and for the same reason:
    /// `create_rdb_layout` refuses anything under 10 MB whole regardless of
    /// one partition's own size.
    const RDB_HEADROOM_MB: u64 = 2;

    fn card_with_partition(
        dir: &std::path::Path,
        fs: AmigaHardDiskFs,
        mb: u32,
    ) -> std::path::PathBuf {
        let path = dir.join("card.hdf");
        crate::core::hdf::create_hdf(
            &path,
            (mb as u64 + RDB_HEADROOM_MB) * 1024 * 1024,
            true,
            &[PartitionSpec {
                drive_name: "DH0".into(),
                fs_type: fs,
                size_mb: mb,
                bootable: true,
                boot_priority: 0,
                num_buffers: 0,
            }],
            &[],
        )
        .unwrap();
        path
    }

    /// Two FFS partitions in one RDB — `DH0` and `DH1` — so a test can corrupt
    /// one and still copy a real report into the other.
    fn card_with_two_partitions(dir: &std::path::Path, mb: u32) -> std::path::PathBuf {
        let path = dir.join("card.hdf");
        let spec = |name: &str| PartitionSpec {
            drive_name: name.into(),
            fs_type: AmigaHardDiskFs::FfsStandard,
            size_mb: mb,
            bootable: true,
            boot_priority: 0,
            num_buffers: 0,
        };
        crate::core::hdf::create_hdf(
            &path,
            (mb as u64 * 2 + RDB_HEADROOM_MB) * 1024 * 1024,
            true,
            &[spec("DH0"), spec("DH1")],
            &[],
        )
        .unwrap();
        path
    }

    /// A host folder carrying `S/FirstBoot.log`, ready for `NativeFormatter::copy_in`.
    fn tree_with_report(dir: &std::path::Path, contents: &[u8]) -> std::path::PathBuf {
        let tree = dir.join("tree");
        std::fs::create_dir_all(tree.join("S")).unwrap();
        std::fs::write(tree.join("S/FirstBoot.log"), contents).unwrap();
        tree
    }

    /// Overwrite one partition's root block with garbage, directly — a real
    /// corruption, not a mocked error. `read_card` and `partition_region` are
    /// used to find the exact bytes to hit, the same way production code
    /// would locate them; nothing here goes through ART's own writer.
    fn corrupt_root_block(image: &std::path::Path, area_index: usize, part_index: usize) {
        let card = read_card(image).unwrap();
        let area = &card.areas[area_index];
        let part = &area.rdb.partitions[part_index];
        let (offset, length, block_size) = partition_region(area, part).unwrap();
        let total_blocks = (length / block_size as u64) as u32;
        let root_block = VolumeGeometry::root_block_for(total_blocks);
        let at = offset + root_block as u64 * block_size as u64;

        let mut file = std::fs::OpenOptions::new().write(true).open(image).unwrap();
        file.seek(SeekFrom::Start(at)).unwrap();
        file.write_all(&vec![0xFFu8; block_size]).unwrap();
    }

    /// A report larger than `MAX_REPORT_BYTES` on the Amiga side is a real
    /// refusal, not something swallowed in favour of a quieter "not booted"
    /// down the fallback chain — see `amiga_report`'s own doc comment.
    #[test]
    fn an_oversized_amiga_volume_report_is_refused_not_silently_skipped() {
        let dir = ScratchDir::new("art-firstboot-cardread", "ffs-oversized");
        let image = card_with_partition(dir.path(), AmigaHardDiskFs::FfsStandard, 8);
        NativeFormatter
            .format_partition(&image, None, 1, "Work", &NoProgress)
            .unwrap();
        let huge = vec![7u8; (MAX_REPORT_BYTES + 1) as usize];
        let tree = tree_with_report(dir.path(), &huge);
        NativeFormatter
            .copy_in(&image, None, "DH0", &tree, &NoProgress)
            .unwrap();

        let err = read_card_report(&image).unwrap_err();
        assert_eq!(err.code(), "ART-LIMIT-EXCEEDED", "{err}");
    }

    #[test]
    fn an_ffs_volumes_own_report_reads_back_as_the_amiga_volume_source() {
        let dir = ScratchDir::new("art-firstboot-cardread", "ffs-report");
        let image = card_with_partition(dir.path(), AmigaHardDiskFs::FfsStandard, 8);
        NativeFormatter
            .format_partition(&image, None, 1, "Work", &NoProgress)
            .unwrap();
        let tree = tree_with_report(dir.path(), b"art-firstboot 1\ndone all\n");
        NativeFormatter
            .copy_in(&image, None, "DH0", &tree, &NoProgress)
            .unwrap();

        let found = read_card_report(&image).unwrap();
        assert_eq!(found.source, ReportSource::AmigaVolume);
        assert_eq!(found.report.ending, super::super::report::Ending::DoneAll);
    }

    #[test]
    fn a_pfs3_volumes_own_report_reads_back_as_the_amiga_volume_source() {
        let dir = ScratchDir::new("art-firstboot-cardread", "pfs3-report");
        let image = card_with_partition(dir.path(), AmigaHardDiskFs::Pfs3DirectScsi, 8);
        NativeFormatter
            .format_partition(&image, None, 1, "Work", &NoProgress)
            .unwrap();
        let tree = tree_with_report(dir.path(), b"art-firstboot 1\ndone all\n");
        NativeFormatter
            .copy_in(&image, None, "DH0", &tree, &NoProgress)
            .unwrap();

        let found = read_card_report(&image).unwrap();
        assert_eq!(found.source, ReportSource::AmigaVolume);
        assert_eq!(found.report.ending, super::super::report::Ending::DoneAll);
    }

    #[test]
    fn a_formatted_amiga_volume_with_neither_copy_is_not_booted() {
        let dir = ScratchDir::new("art-firstboot-cardread", "neither-copy");
        let image = card_with_partition(dir.path(), AmigaHardDiskFs::FfsStandard, 8);
        NativeFormatter
            .format_partition(&image, None, 1, "Work", &NoProgress)
            .unwrap();
        // Nothing copied in: an empty, freshly formatted volume, and no FAT
        // partition at all (this is a plain HDF).

        let found = read_card_report(&image).unwrap();
        assert_eq!(found.source, ReportSource::None);
        assert_eq!(found.report.ending, super::super::report::Ending::NotBooted);
    }

    // ---- fix round 1: "not booted" and "could not be checked" are not the
    // same ending ----

    /// A card whose only Amiga partition is corrupt has told ART nothing at
    /// all — the same wire shape as a genuinely untouched card would be a
    /// confident-wrong sentence. This must be `Err`, never `ReportSource::None`.
    #[test]
    fn a_card_whose_only_amiga_partition_is_corrupt_is_could_not_be_checked() {
        let dir = ScratchDir::new("art-firstboot-cardread", "ffs-corrupt-only");
        let image = card_with_partition(dir.path(), AmigaHardDiskFs::FfsStandard, 8);
        NativeFormatter
            .format_partition(&image, None, 1, "Work", &NoProgress)
            .unwrap();
        corrupt_root_block(&image, 0, 0);

        let err = read_card_report(&image).unwrap_err();
        let text = err.to_string();
        assert!(
            text.contains("DH0"),
            "the refusal must name the partition: {text}"
        );
    }

    /// The other half: a failure on one partition must not stop the search.
    /// `DH0` is corrupt; `DH1` carries the real report, and it still wins.
    #[test]
    fn a_corrupt_first_partition_does_not_hide_a_good_report_on_the_second() {
        let dir = ScratchDir::new("art-firstboot-cardread", "ffs-corrupt-first");
        let image = card_with_two_partitions(dir.path(), 8);
        NativeFormatter
            .format_partition(&image, None, 1, "Work", &NoProgress)
            .unwrap();
        NativeFormatter
            .format_partition(&image, None, 2, "Work", &NoProgress)
            .unwrap();
        corrupt_root_block(&image, 0, 0);

        let tree = tree_with_report(dir.path(), b"art-firstboot 1\ndone all\n");
        NativeFormatter
            .copy_in(&image, None, "DH1", &tree, &NoProgress)
            .unwrap();

        let found = read_card_report(&image).unwrap();
        assert_eq!(found.source, ReportSource::AmigaVolume);
        assert_eq!(found.report.ending, super::super::report::Ending::DoneAll);
    }

    /// A card whose only partitions are a family this module has no reader
    /// for at all (`DosFamily::Other`) is not a failure — it is the same
    /// "nothing to try" `DosFamily::Other` always has been. With no FAT copy
    /// either, this is an ordinary "not booted", not an error.
    #[test]
    fn a_card_with_only_unreadable_filesystem_families_and_no_fat_copy_is_not_booted() {
        let dir = ScratchDir::new("art-firstboot-cardread", "other-family-only");
        let image = card_with_partition(dir.path(), AmigaHardDiskFs::Sfs0, 8);
        // Deliberately not formatted or copied into: `DosFamily::Other` is
        // skipped before this module ever tries to open one.

        let found = read_card_report(&image).unwrap();
        assert_eq!(found.source, ReportSource::None);
        assert_eq!(found.report.ending, super::super::report::Ending::NotBooted);
    }

    /// **Fix round 1, finding 3.** `S/FirstBoot.log` existing as a directory
    /// is not the report — the same check `pfs3_report` already made for
    /// PFS3, now made for FFS too.
    #[test]
    fn an_ffs_directory_named_like_the_report_is_not_read_as_one() {
        let dir = ScratchDir::new("art-firstboot-cardread", "ffs-report-is-dir");
        let image = card_with_partition(dir.path(), AmigaHardDiskFs::FfsStandard, 8);
        NativeFormatter
            .format_partition(&image, None, 1, "Work", &NoProgress)
            .unwrap();

        let tree = dir.path().join("tree");
        std::fs::create_dir_all(tree.join("S/FirstBoot.log")).unwrap(); // a directory
        NativeFormatter
            .copy_in(&image, None, "DH0", &tree, &NoProgress)
            .unwrap();

        let found = read_card_report(&image).unwrap();
        assert_eq!(found.source, ReportSource::None);
        assert_eq!(found.report.ending, super::super::report::Ending::NotBooted);
    }

    /// **The central claim of task 10b, spec §9.** A card carrying both
    /// copies — a stale one on FAT, the real one on the Amiga's own FFS
    /// partition — must answer with the Amiga volume's, never the FAT
    /// fallback.
    #[test]
    fn the_amiga_volumes_own_copy_wins_over_the_fat_copy_when_both_exist() {
        let dir = ScratchDir::new("art-firstboot-cardread", "amiga-wins");
        let total = 400 * 1024 * 1024u64;
        let boot_bytes = 300 * 1024 * 1024u64;
        let area_bytes = 12 * 1024 * 1024u64;
        let layout = plan_card(total, boot_bytes, &[area_bytes]).unwrap();
        let mbr_sector = write_mbr(&layout);
        let boot = layout.boot;
        let area = layout.areas[0];

        let path = dir.join("card.img");
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .truncate(true)
            .read(true)
            .write(true)
            .open(&path)
            .unwrap();
        file.set_len(total).unwrap();
        file.write_all(&mbr_sector).unwrap();

        // FAT side: a stale report, to prove it loses.
        create_boot_partition(
            &mut file,
            boot.start_bytes(),
            boot.length_bytes(),
            DEFAULT_LABEL,
            &[],
        )
        .unwrap();
        {
            let mut region = Region::new(&mut file, boot.start_bytes(), boot.length_bytes());
            let fs = fatfs::FileSystem::new(&mut region, fatfs::FsOptions::new()).unwrap();
            let mut report = fs.root_dir().create_file(FAT_REPORT_NAME).unwrap();
            report
                .write_all(b"art-firstboot 1\ndone partial\n")
                .unwrap();
            report.flush().unwrap();
        }

        // Amiga side: a real RDB with one FFS partition, at the area's own
        // offset — the shape `core::card`'s own tests build a card with.
        let rdb = create_rdb_layout(
            area_bytes,
            &[PartitionSpec {
                drive_name: "DH0".into(),
                fs_type: AmigaHardDiskFs::FfsStandard,
                size_mb: 8,
                bootable: true,
                boot_priority: 0,
                num_buffers: 0,
            }],
            &[],
        )
        .unwrap();
        file.seek(SeekFrom::Start(area.start_bytes())).unwrap();
        file.write_all(&rdb.blocks).unwrap();
        drop(file);

        NativeFormatter
            .format_partition(&path, Some(area.slot_number()), 1, "Work", &NoProgress)
            .unwrap();
        let tree = tree_with_report(dir.path(), b"art-firstboot 1\ndone all\n");
        NativeFormatter
            .copy_in(&path, Some(area.slot_number()), "DH0", &tree, &NoProgress)
            .unwrap();

        let found = read_card_report(&path).unwrap();
        assert_eq!(
            found.source,
            ReportSource::AmigaVolume,
            "the Amiga volume's own copy must win, per spec §9"
        );
        assert_eq!(found.report.ending, super::super::report::Ending::DoneAll);
    }

    /// The wire shape `src/lib/firstboot.ts` reads: three kebab-case strings,
    /// none of them `null`/renamed.
    #[test]
    fn the_source_crosses_the_wire_as_one_of_three_kebab_strings() {
        for (source, wire) in [
            (ReportSource::Fat, "fat"),
            (ReportSource::AmigaVolume, "amiga-volume"),
            (ReportSource::None, "none"),
        ] {
            let found = CardFirstBootReport {
                source,
                report: parse_bytes(b""),
            };
            let json = serde_json::to_value(&found).unwrap();
            assert_eq!(json["source"], wire, "{source:?}");
        }
    }
}
