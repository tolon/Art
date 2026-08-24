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

/// The on-disk shape of a hard drive image, as far as ART's WinUAE launcher
/// needs to know it (ART-146).
///
/// This is deliberately its own type rather than reusing [`HdfType`]:
/// `HdfType::Plain` has always meant "not RDB", which `open_hdf` then treats
/// as a bare filesystem image regardless of what is actually there — correct
/// for the images ART itself creates, wrong for a VHD container, whose
/// `conectix` header at offset 0 is not a filesystem signature at all.
/// Widening `HdfType` to cover that would change what every other caller of
/// `open_hdf` sees; a narrow, purpose-built enum for "how should this be
/// mounted" does not. A fourth shape later is a new variant here, not a
/// second bool alongside `write_protect_hardfiles`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HardfileShape {
    /// A bare filesystem image — `DOS\0`..`DOS\7`, `PFS\3`, `PDS\3`, `SFS\0`
    /// at offset 0 — exactly what ART itself creates (`create_hdf` with
    /// `is_rdb: false`). WinUAE is told the geometry explicitly, as it
    /// always has been.
    #[default]
    Bare,
    /// An `RDSK` signature is present within the first 16 blocks: the image
    /// carries its own partition table, device names and geometry. WinUAE
    /// must read that itself — forced geometry over an RDB makes it parse
    /// partition-table bytes as if they were filesystem data.
    Rdb,
    /// Neither of the above. Chief example: a VHD-wrapped image (`conectix`
    /// at offset 0), whose real `RDSK` sits behind a header at a nonzero
    /// block — forcing geometry meant for a bare image reads that header
    /// where AmigaDOS expects a filesystem, which is `ART-146`'s "Not a DOS
    /// disk in unit 0". WinUAE recognises VHD (and other containers) itself,
    /// so the fix here is the same as the RDB case: get out of the way.
    Unknown,
}

/// Known AmigaDOS signatures at offset 0 of a *bare* image — the shapes
/// `create_hdf` writes and `core/detect.rs`'s drop-pipeline classification
/// already checks for the same reason (`DOS` bootblocks plus the three
/// hard-disk filesystem headers ART recognises elsewhere).
fn is_bare_filesystem_signature(head: &[u8]) -> bool {
    if head.len() < 4 {
        return false;
    }
    // DOS\0..DOS\7: OFS/FFS, international, dircache/long-filenames.
    if &head[0..3] == b"DOS" && head[3] <= 0x07 {
        return true;
    }
    matches!(&head[0..4], b"PFS\x03" | b"PDS\x03" | b"SFS\x00")
}

/// How much of an image is read to decide its [`HardfileShape`]: enough for
/// `find_rdb_location` to scan its full 16-block window, which also covers
/// the four signature bytes at offset 0 that `is_bare_filesystem_signature`
/// needs. Far smaller than `HEADER_READ_BYTES` — this only answers "which
/// shape", not "what partitions" — and still never the whole file.
const SHAPE_PROBE_BYTES: usize = 16 * BLOCK_SIZE;

/// Decide how WinUAE should be told to mount a hard drive image (ART-146).
///
/// Reuses `find_rdb_location` (`core/rdb.rs`) rather than re-implementing
/// RDB detection — the whole point is one place that knows an image's shape,
/// not a second detector that can quietly disagree with the first.
pub fn detect_hardfile_shape(path: &Path) -> CoreResult<HardfileShape> {
    use std::io::Read as _;

    let mut file = std::fs::File::open(path)?;
    let total_bytes = file.metadata()?.len();
    let to_read = total_bytes.min(SHAPE_PROBE_BYTES as u64) as usize;
    let mut bytes = vec![0u8; to_read];
    file.read_exact(&mut bytes)?;

    if find_rdb_location(&bytes).is_some() {
        return Ok(HardfileShape::Rdb);
    }
    if is_bare_filesystem_signature(&bytes) {
        return Ok(HardfileShape::Bare);
    }
    Ok(HardfileShape::Unknown)
}

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
        // A bare volume has no RDB, so no driver geometry was ever
        // stated for it. Zero here means "unstated", not "forbidden".
        max_transfer: 0,
        mask: 0,
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
    use super::super::rdb::IDNAME_RDSK;
    use super::*;

    #[test]
    fn create_and_open_rdb_hdf() {
        let dir = std::env::temp_dir().join(format!("art-hdf-{}", crate::core::test_scratch_id()));
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
        let dir =
            std::env::temp_dir().join(format!("art-hdf-{tag}-{}", crate::core::test_scratch_id()));
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

    /// ART-146: a bare filesystem image — what `create_hdf(is_rdb: false)`
    /// writes and what ART's forced-geometry `hardfile2=` line has always
    /// been correct for.
    #[test]
    fn detect_shape_of_a_bare_dos_image() {
        let dir = scratch("shape-bare");
        let target = dir.join("Bare.hdf");
        let mut image = vec![0u8; BLOCK_SIZE * 4];
        image[0..4].copy_from_slice(b"DOS\x03");
        std::fs::write(&target, &image).unwrap();

        assert_eq!(detect_hardfile_shape(&target).unwrap(), HardfileShape::Bare);

        std::fs::remove_dir_all(&dir).ok();
    }

    /// A PFS3-DirectSCSI bare image carries the same shape — `DOS` is not
    /// the only bare signature ART already knows.
    #[test]
    fn detect_shape_of_a_bare_pds3_image() {
        let dir = scratch("shape-pds3");
        let target = dir.join("Bare.hdf");
        let mut image = vec![0u8; BLOCK_SIZE * 4];
        image[0..4].copy_from_slice(b"PDS\x03");
        std::fs::write(&target, &image).unwrap();

        assert_eq!(detect_hardfile_shape(&target).unwrap(), HardfileShape::Bare);

        std::fs::remove_dir_all(&dir).ok();
    }

    /// The real defect: an `RDSK` block, wherever within the first 16
    /// blocks it falls, must be read as an RDB image rather than a bare one.
    #[test]
    fn detect_shape_of_an_rdb_image() {
        let dir = scratch("shape-rdb");
        let target = dir.join("Rdb.hdf");
        let mut image = vec![0u8; BLOCK_SIZE * 16];
        image[0..4].copy_from_slice(&IDNAME_RDSK.to_be_bytes());
        std::fs::write(&target, &image).unwrap();

        assert_eq!(detect_hardfile_shape(&target).unwrap(), HardfileShape::Rdb);

        std::fs::remove_dir_all(&dir).ok();
    }

    /// The exact evidence from `AmiKit.hdf`: a VHD container's `conectix`
    /// signature at offset 0 is not a filesystem signature ART knows, and
    /// must not be forced through the bare-image geometry path — its real
    /// `RDSK` sits at block 67, past this function's 16-block window, which
    /// is the point: forcing geometry over unrecognised bytes is exactly
    /// what produced "Not a DOS disk in unit 0".
    #[test]
    fn detect_shape_of_a_vhd_image_is_unknown() {
        let dir = scratch("shape-vhd");
        let target = dir.join("AmiKit.hdf");
        let mut image = vec![0u8; BLOCK_SIZE * 16];
        image[0..8].copy_from_slice(b"conectix");
        std::fs::write(&target, &image).unwrap();

        assert_eq!(
            detect_hardfile_shape(&target).unwrap(),
            HardfileShape::Unknown
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    /// [ART-146](../../../docs/ISSUES.md#fixed) against the image it was found
    /// on, rather than against a fixture built from its first eight bytes.
    ///
    /// The defect was found by **reading** `AmiKit.hdf`'s bytes and the fix
    /// was tested on a synthetic `conectix` header — which is the right unit
    /// test and is not the same claim. A real VHD is 1.2 GB of header, block
    /// allocation table and blocks, and what this asks is whether the shape
    /// ART decides from the real file still produces the corrected
    /// `hardfile2=` line: empty device name, zeroed geometry, no second
    /// forced-geometry line beside it.
    ///
    /// Read-only, and deliberately so: it opens somebody's live AmiKit
    /// installation.
    ///
    /// ```text
    /// set ART_REAL_HARDFILE=<the image>
    /// cargo test the_real_vhd_gets_no_forced_geometry -- --ignored --nocapture
    /// ```
    ///
    /// **What it still does not prove.** ART-146's own entry says the branch
    /// has never been mounted in the real emulator, and this does not change
    /// that: it measures the configuration ART writes, not what WinUAE does
    /// with it. That half needs the owner and a running emulator.
    #[test]
    #[ignore = "reads a real multi-gigabyte VHD from disk; run explicitly with ART_REAL_HARDFILE"]
    fn the_real_vhd_gets_no_forced_geometry_when_asked() {
        let Ok(path) = std::env::var("ART_REAL_HARDFILE") else {
            eprintln!("set ART_REAL_HARDFILE to a real hard-disk image");
            return;
        };
        let path = std::path::PathBuf::from(path);
        assert!(path.is_file(), "{} is not a file", path.display());

        // Two independent readers, asked about the same file. `core/vhd` looks
        // for the footer; `detect_hardfile_shape` looks for signatures it
        // knows and gives up otherwise. They agree or something is wrong with
        // one of them.
        let head = {
            use std::io::Read as _;
            let mut file = std::fs::File::open(&path).unwrap();
            let mut buffer = vec![0u8; crate::core::vhd::FOOTER_LEN];
            file.read_exact(&mut buffer).unwrap();
            buffer
        };
        let footer = crate::core::vhd::parse_footer(&head);
        let shape = detect_hardfile_shape(&path).unwrap();
        println!("  {} -> {shape:?}, footer {footer:?}", path.display());

        match footer {
            Some(footer) => {
                assert_eq!(
                    shape,
                    HardfileShape::Unknown,
                    "a VHD ({:?}) must not be mounted as a bare image",
                    footer.kind
                );
            }
            None => assert_ne!(
                shape,
                HardfileShape::Unknown,
                "not a VHD, so it should have been recognised as bare or RDB"
            ),
        }

        // And the line ART would actually write for it.
        let profile = crate::core::profile::AmigaProfile::a1200_aga();
        let media = crate::core::winuae::LaunchMedia {
            hardfile_paths: vec![path.to_string_lossy().into_owned()],
            hardfile_shapes: vec![shape],
            use_aros: true,
            ..Default::default()
        };
        let uae = crate::core::winuae::generate_uae_config(&profile, &media).unwrap();
        let line = uae
            .lines()
            .find(|l| l.starts_with("hardfile2="))
            .expect("one hardfile line");
        println!("  {line}");

        assert_eq!(
            uae.matches("hardfile2=").count(),
            1,
            "a forced-geometry line beside it is the defect ART-146 was"
        );
        if shape != HardfileShape::Bare {
            assert!(
                line.contains(&format!("rw,:{},0,0,0,0,0,,uae", path.display())),
                "{line}"
            );
        }
    }
}
