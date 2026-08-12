//! HDF (Hard Disk File) engine (Phase 3 & Phase 4).
//!
//! Provides inspection, creation, and partition management for Amiga Hard Disk Files
//! (both RDB partitioned disks and plain/raw single-partition containers).

use serde::{Deserialize, Serialize};
use std::path::Path;

use super::rdb::{
    create_rdb_layout, find_rdb_location, parse_rdb, AmigaHardDiskFs, FileSystemSpec,
    ParsedFileSystem, ParsedPartition, PartitionSpec, BLOCK_SIZE,
};
use crate::core::error::{CoreError, CoreResult};

/// Type of HDF container.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum HdfType {
    /// RDB (Rigid Disk Block) Partitioned Disk (Recommended)
    Rdb,
    /// Plain Raw Single-Filesystem Container (Non-RDB)
    Plain,
}

/// High-level information about an opened HDF image.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HdfInfo {
    pub path: String,
    pub total_bytes: u64,
    pub hdf_type: HdfType,
    pub cylinders: u32,
    pub heads: u32,
    pub sectors: u32,
    pub block_size: u32,
    pub partitions: Vec<ParsedPartition>,
    /// The filesystem drivers the RDB carries (G4's reading half).
    ///
    /// Empty is normal for a disk that only uses what Kickstart already has.
    /// It is **not** normal for one with a `PDS\3` or `SFS\0` partition:
    /// those drivers live in the RDB, and a partition naming one the disk does
    /// not provide is a partition an Amiga silently ignores. This is what lets
    /// ART say which of the two it is looking at.
    pub file_systems: Vec<ParsedFileSystem>,
    pub free_bytes: u64,
    pub rdb_checksum_valid: bool,
}

/// How much of an image ART reads to inspect its structure.
///
/// Everything that describes a hard disk — the RDSK block, its partition chain
/// and filesystem headers — lives in the reserved area at the very front. An
/// HDF can be many gigabytes, so reading the whole file to report its geometry
/// would exhaust memory for no benefit.
const HEADER_READ_BYTES: usize = 1024 * 1024;

/// Open and inspect an HDF image from disk.
pub fn open_hdf(path: &Path) -> CoreResult<HdfInfo> {
    use std::io::Read as _;

    let mut file = std::fs::File::open(path)?;
    let total_bytes = file.metadata()?.len();

    if total_bytes < BLOCK_SIZE as u64 {
        return Err(CoreError::Malformed {
            format: "hdf".into(),
            detail: "File too small to be an Amiga Hard Disk File".into(),
        });
    }

    let to_read = total_bytes.min(HEADER_READ_BYTES as u64) as usize;
    let mut bytes = vec![0u8; to_read];
    file.read_exact(&mut bytes)?;

    // Check if image contains an RDB (Rigid Disk Block)
    if find_rdb_location(&bytes).is_some() {
        let parsed = parse_rdb(&bytes)?;
        let bytes_per_cyl =
            (parsed.heads as u64) * (parsed.sectors as u64) * (parsed.block_size as u64);
        let free_bytes = (parsed.free_cylinders as u64) * bytes_per_cyl;

        return Ok(HdfInfo {
            path: path.to_string_lossy().to_string(),
            total_bytes,
            hdf_type: HdfType::Rdb,
            cylinders: parsed.cylinders,
            heads: parsed.heads,
            sectors: parsed.sectors,
            block_size: parsed.block_size,
            partitions: parsed.partitions,
            file_systems: parsed.file_systems,
            free_bytes,
            rdb_checksum_valid: parsed.checksum_valid,
        });
    }

    // Plain / Raw Single-Partition HDF Container
    let sig_str = if bytes.len() >= 4 {
        String::from_utf8_lossy(&bytes[0..4]).to_string()
    } else {
        "RAW".to_string()
    };

    let fs_type = if sig_str.starts_with("PDS") || sig_str.starts_with("PFS") {
        AmigaHardDiskFs::Pfs3DirectScsi
    } else if sig_str.starts_with("SFS") {
        AmigaHardDiskFs::Sfs0
    } else {
        AmigaHardDiskFs::FfsDirCache
    };

    let synthetic_part = ParsedPartition {
        drive_name: "DH0".into(),
        dostype: fs_type.to_dostype_u32(),
        dostype_str: sig_str,
        fs_type,
        low_cyl: 0,
        high_cyl: 0,
        cylinder_count: 0,
        size_bytes: total_bytes,
        bootable: true,
        boot_priority: 0,
        num_buffers: 100,
        block_location: 0,
        next_part_block: 0,
        checksum_valid: true,
        // A bare hardfile has no RDB, so there is no DosEnvVec to read: the
        // whole file is the volume, in 512-byte blocks, with the usual two
        // boot blocks. Geometry is irrelevant to FFS itself (§5) — it only
        // needs linear blocks — so none is invented here.
        size_block: 128,
        surfaces: 1,
        blocks_per_track: 1,
        reserved: 2,
    };

    Ok(HdfInfo {
        path: path.to_string_lossy().to_string(),
        total_bytes,
        hdf_type: HdfType::Plain,
        cylinders: 0,
        heads: 0,
        sectors: 0,
        block_size: BLOCK_SIZE as u32,
        partitions: vec![synthetic_part],
        // A bare hardfile has no RDB, so there is nowhere for a driver to
        // live — which is a different thing from "none found in the RDB".
        file_systems: Vec::new(),
        free_bytes: 0,
        rdb_checksum_valid: true,
    })
}

/// Smallest image ART will create. Below this there is no room for a bootblock.
const MIN_PLAIN_BYTES: u64 = 512;

/// Create a new HDF image file on disk.
///
/// The image is created **sparsely**: only the structured blocks at the front
/// are written, and the file is extended to its full length with `set_len`.
/// Materialising the whole image in memory first meant a 2 GB HDF needed 2 GB
/// of RAM (spec §56).
///
/// Creating never replaces an existing file. HDFs are large and irreplaceable,
/// and this is a `SAFE_CREATE` operation — it may only ever write something new
/// (spec §57).
pub fn create_hdf(
    path: &Path,
    total_bytes: u64,
    is_rdb: bool,
    partitions: &[PartitionSpec],
    file_systems: &[FileSystemSpec],
) -> CoreResult<HdfInfo> {
    use std::io::Write as _;

    if path.exists() {
        return Err(CoreError::SafetyRefused(format!(
            "'{}' already exists; creating a hard disk image never replaces one",
            path.display()
        )));
    }

    // Work out the leading blocks and the final size before touching the disk.
    let (leading_blocks, final_size) = if is_rdb {
        let layout = create_rdb_layout(total_bytes, partitions, file_systems)?;
        (layout.blocks, layout.total_size)
    } else {
        if total_bytes < MIN_PLAIN_BYTES {
            return Err(CoreError::InvalidInput(format!(
                "a hard disk image must be at least {MIN_PLAIN_BYTES} bytes"
            )));
        }
        // Standard FFS DOS\3 bootblock signature, nothing else.
        let mut boot = vec![0u8; BLOCK_SIZE];
        boot[0..4].copy_from_slice(b"DOS\x03");
        (boot, total_bytes)
    };

    let file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)?;

    // If anything below fails, remove the half-made file rather than leaving
    // something that looks like a usable image.
    let build = (|| -> std::io::Result<()> {
        let mut file = file;
        file.write_all(&leading_blocks)?;
        file.set_len(final_size)?;
        file.sync_all()
    })();

    if let Err(e) = build {
        let _ = std::fs::remove_file(path);
        return Err(CoreError::Io(e));
    }

    open_hdf(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_and_open_rdb_hdf() {
        let dir = std::env::temp_dir().join(format!(
            "art-hdf-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let hdf_path = dir.join("System.hdf");

        let specs = vec![PartitionSpec {
            drive_name: "DH0".into(),
            fs_type: AmigaHardDiskFs::Pfs3DirectScsi,
            size_mb: 100,
            bootable: true,
            boot_priority: 0,
            num_buffers: 100,
        }];

        let info = create_hdf(&hdf_path, 200 * 1024 * 1024, true, &specs, &[]).unwrap();
        assert_eq!(info.hdf_type, HdfType::Rdb);
        assert_eq!(info.partitions.len(), 1);
        assert_eq!(info.partitions[0].drive_name, "DH0");
        assert!(info.rdb_checksum_valid);

        std::fs::remove_dir_all(&dir).ok();
    }

    fn scratch(tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "art-hdf-{tag}-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// An HDF can hold a user's entire Workbench install. Creating one must
    /// never land on top of an existing file.
    #[test]
    fn create_refuses_to_replace_an_existing_image() {
        let dir = scratch("exists");
        let target = dir.join("Workbench.hdf");
        std::fs::write(&target, b"irreplaceable user data").unwrap();

        let err = create_hdf(&target, 100 * 1024 * 1024, false, &[], &[]).unwrap_err();
        assert!(matches!(err, CoreError::SafetyRefused(_)), "got {err:?}");
        assert_eq!(
            std::fs::read(&target).unwrap(),
            b"irreplaceable user data",
            "the existing file must be untouched"
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    /// The image is created sparsely: a large HDF must not require a matching
    /// allocation in memory. Four gigabytes would previously have been a 4 GB
    /// `Vec` before a single byte reached the disk.
    #[test]
    fn large_images_are_created_sparsely() {
        let dir = scratch("sparse");
        let target = dir.join("Big.hdf");

        let specs = vec![PartitionSpec {
            drive_name: "DH0".into(),
            fs_type: AmigaHardDiskFs::FfsDirCache,
            size_mb: 1024,
            bootable: true,
            boot_priority: 0,
            num_buffers: 100,
        }];

        let info = create_hdf(&target, 4 * 1024 * 1024 * 1024, true, &specs, &[]).unwrap();
        assert!(info.total_bytes >= 4 * 1024 * 1024 * 1024);
        assert_eq!(info.partitions.len(), 1);
        assert!(info.rdb_checksum_valid);

        std::fs::remove_dir_all(&dir).ok();
    }

    /// A tiny size used to index a four-byte bootblock signature into a
    /// shorter buffer and abort the whole application.
    #[test]
    fn absurdly_small_sizes_error_rather_than_panic() {
        let dir = scratch("tiny");

        let err = create_hdf(&dir.join("a.hdf"), 2, false, &[], &[]).unwrap_err();
        assert!(matches!(err, CoreError::InvalidInput(_)), "got {err:?}");

        // The RDB path has its own floor.
        assert!(create_hdf(&dir.join("b.hdf"), 1024, true, &[], &[]).is_err());

        std::fs::remove_dir_all(&dir).ok();
    }
}
