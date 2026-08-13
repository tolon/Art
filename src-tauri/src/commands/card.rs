//! Reading a PiStorm SD card — the screen half of ART-095/ART-097.
//!
//! `core/card.rs` has been able to read a real card since 2026-08-13; nothing
//! could show one. This is the adapter that lets the Hard Disk studio ask.
//!
//! The two derived answers travel with the card rather than being left for the
//! UI to work out, and that is deliberate: **both of them are questions the UI
//! would get wrong.** `file_systems` is the union across every area, and
//! `unmountable` is asked against that union — MultibootOS 2.2 carries PFS3 in
//! its first RDB and not its second, so an interface that asked one area in
//! isolation would report fifteen working partitions as broken (ART-097). The
//! rule lives in `core/`, where a test pins it.

use std::path::PathBuf;

use serde::Serialize;

use crate::core::card::{read_card, CardImage};
use crate::core::rdb::ParsedFileSystem;
use crate::error::AppResult;

/// A partition naming a filesystem **no** area on the card carries.
///
/// An Amiga ignores such a partition in silence, which is the failure ART-084
/// was about — and the only way a user finds out is that a drive they expected
/// is not there.
#[derive(Debug, Clone, Serialize)]
pub struct UnmountablePartition {
    /// Which Amiga disk on the card it belongs to.
    pub area: usize,
    pub drive_name: String,
    /// `PDS\3`, `SFS\0` — the four characters as they read.
    pub dostype_str: String,
}

/// What ART found on a card, ready for a screen.
#[derive(Debug, Clone, Serialize)]
pub struct CardReport {
    pub card: CardImage,
    /// Every driver on the **whole card**, deduplicated by DosType.
    pub file_systems: Vec<ParsedFileSystem>,
    pub unmountable: Vec<UnmountablePartition>,
}

/// Read a card — or a plain HDF, which comes back as one area at offset zero.
///
/// Header-only: a window at each area's start, never the sixty to a hundred
/// and twenty gigabytes in between (§56).
#[tauri::command]
pub fn card_open(path: String) -> AppResult<CardReport> {
    let card = read_card(&PathBuf::from(path.trim()))?;

    let file_systems = card.file_systems();
    let unmountable = card
        .partitions_missing_driver()
        .into_iter()
        .map(|(area, part)| UnmountablePartition {
            area,
            drive_name: part.drive_name,
            dostype_str: part.dostype_str,
        })
        .collect();

    Ok(CardReport {
        card,
        file_systems,
        unmountable,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A plain HDF is not a card, and must not be reported as a broken one:
    /// one area at offset zero, no MBR, and nothing unmountable.
    ///
    /// The adapter is thin enough that this is really a test of the contract
    /// the screen depends on — that it can call one command for both kinds of
    /// file and branch on `mbr` rather than on a guess.
    #[test]
    fn a_plain_hdf_comes_back_as_one_area_at_offset_zero() {
        use crate::core::rdb::{AmigaHardDiskFs, PartitionSpec};

        let dir = std::env::temp_dir().join(format!("art-card-cmd-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("plain.hdf");

        crate::core::hdf::create_hdf(
            &path,
            32 * 1024 * 1024,
            true,
            &[PartitionSpec {
                drive_name: "DH0".into(),
                fs_type: AmigaHardDiskFs::FfsStandard,
                size_mb: 10,
                bootable: true,
                boot_priority: 0,
                num_buffers: 0,
            }],
            &[],
        )
        .unwrap();

        let report = card_open(path.display().to_string()).unwrap();

        assert!(
            report.card.mbr.is_none(),
            "an HDF carries no partition table"
        );
        assert_eq!(report.card.areas.len(), 1);
        assert_eq!(report.card.areas[0].offset_bytes, 0);
        assert_eq!(report.card.areas[0].rdb.partitions.len(), 1);
        assert!(
            report.unmountable.is_empty(),
            "FFS is Kickstart's own and needs no driver in the RDB"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A partition naming a filesystem nothing carries is named, with the area
    /// it is on — the answer ART-084 wanted and ART-097 corrected.
    #[test]
    fn a_partition_with_no_driver_anywhere_on_the_card_is_named() {
        use crate::core::rdb::{AmigaHardDiskFs, PartitionSpec};

        let dir = std::env::temp_dir().join(format!("art-card-cmd-pfs-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("pfs.hdf");

        // PFS3 is not in Kickstart: without a driver in the RDB, an Amiga
        // ignores this partition in silence.
        crate::core::hdf::create_hdf(
            &path,
            32 * 1024 * 1024,
            true,
            &[PartitionSpec {
                drive_name: "DH0".into(),
                fs_type: AmigaHardDiskFs::Pfs3Standard,
                size_mb: 10,
                bootable: true,
                boot_priority: 0,
                num_buffers: 0,
            }],
            &[],
        )
        .unwrap();

        let report = card_open(path.display().to_string()).unwrap();

        assert_eq!(report.file_systems.len(), 0, "no driver was embedded");
        assert_eq!(report.unmountable.len(), 1);
        assert_eq!(report.unmountable[0].area, 0);
        assert_eq!(report.unmountable[0].drive_name, "DH0");

        let _ = std::fs::remove_dir_all(&dir);
    }
}
