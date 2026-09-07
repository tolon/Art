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
/// Structural failures — the image will not open at all — are a hard `Err`,
/// the same convention `core::osinstall::verify::verify_volume` uses: nothing
/// here could be read, so there is nothing to report. A card that opens fine
/// but simply carries no report yet is not an error — it comes back as
/// [`ReportSource::None`] with an empty, [`super::report::Ending::NotBooted`]
/// report.
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
/// first hit wins. A partition ART cannot even open as the filesystem its own
/// `DosType` claims — unformatted, corrupt, or a family `write_refusal`
/// declines to write and this module equally declines to read — is not this
/// search's business to fail on: another partition, or the FAT copy, may
/// still answer. **Finding the file and it being too large to read is
/// different**: that refusal is real, and letting it be swallowed in favour
/// of a quieter "not booted" further down the fallback chain would be exactly
/// the confident-wrong sentence CLAUDE.md's "the failure that does not crash"
/// warns about, so it is the one error this function lets through.
fn amiga_report(card: &CardImage) -> CoreResult<Option<Vec<u8>>> {
    let path = Path::new(&card.path);
    for area in &card.areas {
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
                Err(err @ CoreError::LimitExceeded { .. }) => return Err(err),
                Err(_) => continue,
            }
        }
    }
    Ok(None)
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
/// takes a write handle (see `core::osinstall::verify`'s own module doc,
/// Decision 1, for what else PFS3 can and cannot honestly claim — content
/// here is a lookup by name and a read by anode chain, the same limited trust
/// that module already places in `libpfs3`).
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
    for segment in REPORT_PATH.split('/') {
        match dir::find_entry(&region, &set, &geometry, current, segment)? {
            Some(entry) => current = entry.block,
            None => return Ok(None),
        }
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

    /// A host folder carrying `S/FirstBoot.log`, ready for `NativeFormatter::copy_in`.
    fn tree_with_report(dir: &std::path::Path, contents: &[u8]) -> std::path::PathBuf {
        let tree = dir.join("tree");
        std::fs::create_dir_all(tree.join("S")).unwrap();
        std::fs::write(tree.join("S/FirstBoot.log"), contents).unwrap();
        tree
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
