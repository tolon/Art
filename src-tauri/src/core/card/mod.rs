//! A PiStorm card, read as what it actually is: an MBR with Amiga disks in it.
//!
//! ART's hard-disk code has always assumed one Amiga disk beginning at byte
//! zero, because that is what an HDF is. A card is not an HDF. It is an
//! MBR-partitioned image with a FAT32 boot partition and **one or more** areas
//! the Amiga sees as separate disks, each starting with its own `RDSK`
//! (ART-095, ART-097).
//!
//! This module is the layer that tells the two apart, and it answers for both:
//! a plain HDF comes back as a card with one area at offset zero, so callers
//! do not branch.
//!
//! **Read-only, and header-only.** A card is sixty to a hundred and twenty
//! gigabytes; nothing here reads more than a window at each area's start
//! (spec §56).

pub mod build;
pub mod health;
pub mod manifest;
pub mod payload;

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::core::error::{CoreError, CoreResult};
use crate::core::mbr::{amiga_bases, parse_mbr, Mbr, SECTOR_BYTES};
use crate::core::rdb::{parse_rdb, ParsedFileSystem, ParsedPartition, ParsedRdb};

/// How much of each area ART reads to parse its RDB.
///
/// The RDB reserves two cylinders, and the largest cylinder seen on a real
/// card is 8192 blocks — 8 MB covers that several times over without being a
/// size anybody notices.
const AREA_WINDOW_BYTES: usize = 8 * 1024 * 1024;

/// One Amiga disk on the card.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AmigaArea {
    /// Where this disk begins in the file. Every block number in its RDB is
    /// relative to here — reading them as file-absolute is the mistake this
    /// module exists to make impossible.
    pub offset_bytes: u64,
    /// How long the MBR says it is. Zero for a plain HDF, where the answer is
    /// "the rest of the file".
    pub length_bytes: u64,
    pub rdb: ParsedRdb,
}

/// What ART found on a card.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CardImage {
    pub path: String,
    pub total_bytes: u64,
    /// `None` for a plain HDF — which is not an error, just a different kind
    /// of file.
    pub mbr: Option<Mbr>,
    pub areas: Vec<AmigaArea>,
}

impl CardImage {
    /// Every filesystem driver on the whole card.
    ///
    /// **The union, and that is the point.** MultibootOS 2.2's second RDB
    /// carries a `DOS\3` driver and no PFS3, while all fifteen of its
    /// partitions are `PDS\3` — and the card works, because a partition may
    /// use any driver the disk carries. Asking one RDB would name fifteen
    /// working partitions as unmountable (ART-097).
    pub fn file_systems(&self) -> Vec<ParsedFileSystem> {
        let mut out: Vec<ParsedFileSystem> = Vec::new();
        for area in &self.areas {
            for fs in &area.rdb.file_systems {
                if !out.iter().any(|seen| seen.dos_type == fs.dos_type) {
                    out.push(fs.clone());
                }
            }
        }
        out
    }

    /// Whether any driver on the card provides this DosType.
    pub fn provides_file_system(&self, dos_type: u32) -> bool {
        self.file_systems().iter().any(|fs| fs.dos_type == dos_type)
    }

    /// Every partition on the card, with the area it belongs to.
    pub fn partitions(&self) -> Vec<(usize, ParsedPartition)> {
        self.areas
            .iter()
            .enumerate()
            .flat_map(|(index, area)| {
                area.rdb
                    .partitions
                    .iter()
                    .cloned()
                    .map(move |part| (index, part))
            })
            .collect()
    }

    /// Partitions naming a filesystem **no** area on the card carries.
    ///
    /// The honest version of the question ART-084 asked: an Amiga ignores such
    /// a partition in silence. DosTypes 0 to 7 are Kickstart's own and need
    /// nothing in the RDB.
    pub fn partitions_missing_driver(&self) -> Vec<(usize, ParsedPartition)> {
        self.partitions()
            .into_iter()
            .filter(|(_, part)| {
                let dos = part.dostype;
                let is_kickstarts = dos & 0xFFFF_FF00 == 0x444F_5300 && (dos & 0xFF) <= 7;
                !is_kickstarts && !self.provides_file_system(dos)
            })
            .collect()
    }
}

/// Read a card, or a plain HDF, and report the Amiga disks in it.
pub fn read_card(path: &Path) -> CoreResult<CardImage> {
    use std::io::{Read as _, Seek as _, SeekFrom};

    let mut file = std::fs::File::open(path)?;
    let total_bytes = file.metadata()?.len();

    if total_bytes < SECTOR_BYTES {
        return Err(CoreError::Malformed {
            format: "card".into(),
            detail: "File too small to hold a partition table".into(),
        });
    }

    let mut first = vec![0u8; SECTOR_BYTES as usize];
    file.read_exact(&mut first)?;

    let mbr = parse_mbr(&first);
    let bases = amiga_bases(&first);

    let lengths: Vec<u64> = match &mbr {
        Some(mbr) => mbr.amiga_areas().iter().map(|p| p.length_bytes()).collect(),
        // A plain HDF is one disk and it is the whole file.
        None => vec![total_bytes],
    };

    let mut areas = Vec::new();
    for (index, base) in bases.iter().enumerate() {
        if *base >= total_bytes {
            // A table describing an area past the end of the file. Reported by
            // its absence rather than by a crash: the rest of the card may be
            // perfectly readable.
            continue;
        }

        let window = AREA_WINDOW_BYTES.min((total_bytes - base) as usize);
        let mut bytes = vec![0u8; window];
        file.seek(SeekFrom::Start(*base))?;
        // Not `read_exact`: a truncated download is a card whose last area is
        // short, and reading what is there beats refusing the whole file.
        let read = read_as_much_as_possible(&mut file, &mut bytes)?;
        bytes.truncate(read);

        let Ok(rdb) = parse_rdb(&bytes) else {
            continue;
        };

        areas.push(AmigaArea {
            offset_bytes: *base,
            length_bytes: lengths.get(index).copied().unwrap_or(0),
            rdb,
        });
    }

    Ok(CardImage {
        path: path.to_string_lossy().into_owned(),
        total_bytes,
        mbr,
        areas,
    })
}

fn read_as_much_as_possible(file: &mut std::fs::File, buffer: &mut [u8]) -> CoreResult<usize> {
    use std::io::Read as _;

    let mut filled = 0;
    while filled < buffer.len() {
        match file.read(&mut buffer[filled..])? {
            0 => break,
            n => filled += n,
        }
    }
    Ok(filled)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::rdb::{create_rdb_layout, AmigaHardDiskFs, PartitionSpec};

    fn scratch(tag: &str) -> std::path::PathBuf {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("art-card-{tag}-{stamp}"));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn partition(name: &str, fs: AmigaHardDiskFs) -> PartitionSpec {
        PartitionSpec {
            drive_name: name.into(),
            fs_type: fs,
            size_mb: 12,
            bootable: true,
            boot_priority: 0,
            num_buffers: 600,
        }
    }

    /// A card built the way both real ones are: MBR, a FAT32 partition, then
    /// one or two Amiga areas each holding a real RDB.
    fn card(tag: &str, areas: &[(u64, Vec<u8>)]) -> std::path::PathBuf {
        let dir = scratch(tag);
        let path = dir.join("card.img");

        let end = areas
            .iter()
            .map(|(base, blocks)| base + blocks.len() as u64)
            .max()
            .unwrap_or(SECTOR_BYTES);

        let mut image = vec![0u8; end as usize];
        // MBR: FAT32 at LBA 2048, then a 0x76 area per RDB.
        image[510] = 0x55;
        image[511] = 0xAA;
        let mut slot = 0usize;
        let write_entry = |image: &mut Vec<u8>, slot: usize, ptype: u8, lba: u32, count: u32| {
            let at = 446 + slot * 16;
            image[at + 4] = ptype;
            image[at + 8..at + 12].copy_from_slice(&lba.to_le_bytes());
            image[at + 12..at + 16].copy_from_slice(&count.to_le_bytes());
        };
        write_entry(&mut image, slot, 0x0C, 2048, 2048);
        slot += 1;
        for (base, blocks) in areas {
            write_entry(
                &mut image,
                slot,
                0x76,
                (base / SECTOR_BYTES) as u32,
                (blocks.len() as u64 / SECTOR_BYTES) as u32,
            );
            slot += 1;
            image[*base as usize..*base as usize + blocks.len()].copy_from_slice(blocks);
        }

        std::fs::write(&path, &image).unwrap();
        path
    }

    #[test]
    fn a_card_is_read_at_the_offset_its_partition_table_gives() {
        // The whole of ART-095: the RDB is not at byte 0, and ART used to look
        // for it only there.
        let layout = create_rdb_layout(
            64 * 1024 * 1024,
            &[partition("SDH0", AmigaHardDiskFs::FfsStandard)],
            &[],
        )
        .unwrap();
        let base = 1_048_576 + 1024 * 1024; // past the FAT32 partition
        let path = card("one-area", &[(base, layout.blocks)]);

        let found = read_card(&path).unwrap();
        assert!(found.mbr.is_some(), "a card has a partition table");
        assert_eq!(found.areas.len(), 1);
        assert_eq!(found.areas[0].offset_bytes, base);
        assert_eq!(found.areas[0].rdb.partitions.len(), 1);
        assert_eq!(found.areas[0].rdb.partitions[0].drive_name, "SDH0");

        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn a_plain_hdf_still_reads_as_one_disk_at_zero() {
        // The other half of the same function: callers must not have to know
        // which kind of file they were handed.
        let layout = create_rdb_layout(
            64 * 1024 * 1024,
            &[partition("DH0", AmigaHardDiskFs::FfsStandard)],
            &[],
        )
        .unwrap();
        let dir = scratch("plain");
        let path = dir.join("plain.hdf");
        std::fs::write(&path, &layout.blocks).unwrap();

        let found = read_card(&path).unwrap();
        assert!(found.mbr.is_none(), "an HDF has no partition table");
        assert_eq!(found.areas.len(), 1);
        assert_eq!(found.areas[0].offset_bytes, 0);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_card_with_two_amiga_areas_reads_both() {
        // MultibootOS 2.2's shape.
        let first = create_rdb_layout(
            64 * 1024 * 1024,
            &[partition("SDH0", AmigaHardDiskFs::FfsStandard)],
            &[],
        )
        .unwrap();
        let second = create_rdb_layout(
            64 * 1024 * 1024,
            &[partition("ADH0", AmigaHardDiskFs::FfsStandard)],
            &[],
        )
        .unwrap();

        let base_a = 1_048_576;
        let base_b = base_a + 4 * 1024 * 1024;
        let path = card(
            "two-areas",
            &[(base_a, first.blocks), (base_b, second.blocks)],
        );

        let found = read_card(&path).unwrap();
        assert_eq!(found.areas.len(), 2);
        assert_eq!(found.areas[0].rdb.partitions[0].drive_name, "SDH0");
        assert_eq!(found.areas[1].rdb.partitions[0].drive_name, "ADH0");
        assert_eq!(found.partitions().len(), 2);

        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    /// ART-097, and the reason it is not a cosmetic bug: MultibootOS's second
    /// RDB carries no PFS3 while every partition in it is PFS3, and the card
    /// works — because the drivers are the card's, not the area's.
    #[test]
    fn a_driver_in_one_area_covers_a_partition_in_another() {
        use crate::core::rdb::FileSystemSpec;

        let with_driver = create_rdb_layout(
            64 * 1024 * 1024,
            &[],
            &[FileSystemSpec {
                dos_type: 0x5044_5303,
                version: 19,
                revision: 2,
                data: vec![7u8; 600],
            }],
        )
        .unwrap();
        let without_driver = create_rdb_layout(
            64 * 1024 * 1024,
            &[partition("AGS0", AmigaHardDiskFs::Pfs3DirectScsi)],
            &[],
        )
        .unwrap();

        let base_a = 1_048_576;
        let base_b = base_a + 4 * 1024 * 1024;
        let path = card(
            "union",
            &[
                (base_a, with_driver.blocks),
                (base_b, without_driver.blocks),
            ],
        );

        let found = read_card(&path).unwrap();
        assert!(
            found.provides_file_system(0x5044_5303),
            "the card carries PFS3, whichever area it is in"
        );
        assert!(
            found.partitions_missing_driver().is_empty(),
            "asking one area at a time would name a working partition as broken"
        );

        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn a_partition_with_no_driver_anywhere_is_still_reported() {
        // The guard against over-correcting: the union must not turn ART-084's
        // real finding into silence.
        let layout = create_rdb_layout(
            64 * 1024 * 1024,
            &[partition("SDH0", AmigaHardDiskFs::Pfs3DirectScsi)],
            &[],
        )
        .unwrap();
        let path = card("missing", &[(1_048_576, layout.blocks)]);

        let found = read_card(&path).unwrap();
        let missing = found.partitions_missing_driver();
        assert_eq!(missing.len(), 1);
        assert_eq!(missing[0].1.drive_name, "SDH0");

        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn an_ffs_partition_needs_nothing_in_the_rdb() {
        // `DOS\0`…`DOS\7` are Kickstart's own.
        let layout = create_rdb_layout(
            64 * 1024 * 1024,
            &[partition("DH0", AmigaHardDiskFs::FfsDirCache)],
            &[],
        )
        .unwrap();
        let path = card("ffs", &[(1_048_576, layout.blocks)]);

        let found = read_card(&path).unwrap();
        assert!(found.partitions_missing_driver().is_empty());

        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn an_area_the_table_puts_past_the_end_of_the_file_is_skipped() {
        // A truncated download. The rest of the card may be perfectly
        // readable, so this is an absence rather than a refusal.
        let layout = create_rdb_layout(
            64 * 1024 * 1024,
            &[partition("SDH0", AmigaHardDiskFs::FfsStandard)],
            &[],
        )
        .unwrap();
        let base = 1_048_576;
        let path = card("truncated", &[(base, layout.blocks)]);

        // Claim a second area far past the end.
        let mut image = std::fs::read(&path).unwrap();
        let at = 446 + 2 * 16;
        image[at + 4] = 0x76;
        image[at + 8..at + 12].copy_from_slice(&40_000_000u32.to_le_bytes());
        image[at + 12..at + 16].copy_from_slice(&1000u32.to_le_bytes());
        std::fs::write(&path, &image).unwrap();

        let found = read_card(&path).unwrap();
        assert_eq!(found.areas.len(), 1, "the readable area still reads");

        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn a_file_too_small_to_hold_a_table_is_refused_by_name() {
        let dir = scratch("tiny");
        let path = dir.join("tiny.img");
        std::fs::write(&path, b"nope").unwrap();
        assert!(read_card(&path).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Read a real card and print what is on it, for the oracle script and for
    /// a human checking ART against a card they can see.
    ///
    /// ```text
    /// ART_CARD_IN=E:\...\CaffeineOS_Storm_9317.img cargo test read_real_card_when_asked -- --nocapture
    /// ```
    #[test]
    fn read_real_card_when_asked() {
        let Ok(path) = std::env::var("ART_CARD_IN") else {
            return;
        };

        let found = read_card(std::path::Path::new(&path)).unwrap();
        println!("card={} bytes={}", found.path, found.total_bytes);
        println!("areas={}", found.areas.len());
        for area in &found.areas {
            println!(
                "  area at {} cyl={} heads={} sectors={} partitions={} drivers={}",
                area.offset_bytes,
                area.rdb.cylinders,
                area.rdb.heads,
                area.rdb.sectors,
                area.rdb.partitions.len(),
                area.rdb.file_systems.len()
            );
            for part in &area.rdb.partitions {
                println!(
                    "    {} {} lo={} hi={} buffers={}",
                    part.drive_name,
                    part.dostype_str,
                    part.low_cyl,
                    part.high_cyl,
                    part.num_buffers
                );
            }
        }
        println!("drivers-on-card={}", found.file_systems().len());
        println!(
            "partitions-missing-driver={}",
            found.partitions_missing_driver().len()
        );
    }
}
