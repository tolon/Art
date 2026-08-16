//! Finding the volumes inside an image file, and opening one.
//!
//! An `.hdf` is one of three things (§2.4), and the order matters:
//!
//! ```text
//! 1. "RDSK" within the first 16 blocks  → a partitioned disk; enumerate them
//! 2. "DOS\x" at offset 0                → a bare volume; the file IS the disk
//! 3. neither                            → unknown; say so, do not guess
//! ```
//!
//! ## The rule that shapes the output
//!
//! §2.5: **never "cannot see HDF"**. Every partition is listed with its name,
//! size and filesystem — and then either its contents or the exact reason ART
//! cannot read them. A user must be able to learn that their disk is healthy
//! even when ART cannot walk it. So an unsupported filesystem produces a
//! [`VolumeEntry`] with [`unsupported`](VolumeEntry::unsupported) set, not a
//! missing row and not an empty file list.

use std::path::Path;

use serde::Serialize;

use super::device::FileRegion;
use super::journal::{find_journal, journal_path_for};
use super::{DosType, VolumeGeometry, SECTOR_BYTES};
use crate::core::error::{CoreError, CoreResult};
use crate::core::rdb::{find_rdb_location, parse_rdb};

/// How much of an image is read to find its partition table.
///
/// The RDB lives in the first couple of cylinders by definition. Reading a
/// window rather than the file is the ART-021 rule: an HDF can be gigabytes.
const HEADER_WINDOW_BYTES: usize = 1024 * 1024;

/// How ART read the image.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ImageLayout {
    /// A Rigid Disk Block with a partition table.
    Rdb,
    /// No RDB — the file itself is one filesystem.
    BareVolume,
    /// Neither. ART does not guess what it is.
    Unknown,
}

/// One volume ART found in an image.
#[derive(Debug, Clone, Serialize)]
pub struct VolumeEntry {
    /// `DH0`, or the file's own name for a bare volume.
    pub name: String,
    /// `FFS INTL`, `PFS3`, …
    pub filesystem: String,
    pub dos_type: [u8; 4],
    pub byte_offset: u64,
    pub byte_length: u64,
    pub block_size: usize,
    pub reserved: u32,
    pub bootable: bool,
    pub boot_priority: i8,
    /// `None` when ART can open it; otherwise why it can only be listed.
    pub unsupported: Option<String>,
    /// True when the partition claims more space than the file holds — a
    /// truncated image, reported rather than read as garbage (§5).
    pub clamped: bool,
}

impl VolumeEntry {
    pub fn is_mountable(&self) -> bool {
        self.unsupported.is_none()
    }
}

/// Everything ART could work out about an image.
#[derive(Debug, Clone, Serialize)]
pub struct ImageVolumes {
    pub layout: ImageLayout,
    pub volumes: Vec<VolumeEntry>,
    /// An operation that did not finish, found next to this image.
    ///
    /// Reported at scan time because that is when the user is looking at the
    /// image and before anything reads its contents — browsing a volume whose
    /// blocks are half-written would show them a filesystem that never
    /// existed (brief §2).
    pub pending_recovery: Option<PendingRecovery>,
    /// Set when there is something to say beyond the list — an unrecognised
    /// image, or an RDB whose checksum does not add up.
    pub note: Option<String>,
}

/// An unfinished operation, as the UI needs to describe it.
///
/// Deliberately a plain summary rather than the journal itself: deciding is a
/// user action, and the decision travels to the frontend and back before
/// anything is applied.
#[derive(Debug, Clone, Serialize)]
pub struct PendingRecovery {
    /// What the interrupted operation was, in the user's words.
    pub description: String,
    /// How many blocks would be restored.
    pub blocks: usize,
    /// Whether ART is willing to apply it.
    pub can_apply: bool,
    /// Why not, when it is not. Never null when `can_apply` is false.
    pub refusal: Option<String>,
    /// The journal file, so the message can name it.
    pub journal_path: String,
}

/// Look for an unfinished operation next to `path`.
///
/// A journal that cannot be parsed is reported as a refusal rather than an
/// error: ART must still be able to show the user what is in their image, and
/// "there is a file here I do not understand" is information, not a failure to
/// open the disk (§89).
fn look_for_recovery(path: &Path) -> Option<PendingRecovery> {
    match find_journal(path) {
        Ok(None) => None,
        Ok(Some(pending)) => {
            let refusal = pending.refusal().unwrap_or_else(|err| Some(err.to_string()));
            Some(PendingRecovery {
                description: pending.header().description.clone(),
                blocks: pending.blocks(),
                can_apply: refusal.is_none(),
                refusal,
                journal_path: pending.path().display().to_string(),
            })
        }
        Err(err) => Some(PendingRecovery {
            description: "an operation ART cannot identify".into(),
            blocks: 0,
            can_apply: false,
            refusal: Some(format!(
                "There is a file named like an ART journal next to this image, but ART                  cannot read it ({err}). Nothing has been touched."
            )),
            journal_path: journal_path_for(path).display().to_string(),
        }),
    }
}

/// Work out what is in an image file, without reading the whole thing.
pub fn scan_image(path: &Path) -> CoreResult<ImageVolumes> {
    let file_len = std::fs::metadata(path)?.len();
    if file_len < SECTOR_BYTES as u64 {
        return Err(CoreError::Malformed {
            format: "hdf".into(),
            detail: format!("{file_len} bytes is smaller than a single block"),
        });
    }

    let window = read_window(path, file_len)?;

    // 1. RDB.
    if find_rdb_location(&window).is_some() {
        return scan_rdb(&window, file_len, path);
    }

    // 2. A bare volume: the filesystem's magic sits at the very start.
    //
    // Not just `DOS`: a bare PFS3 or SFS hardfile is a real thing, and §2.5
    // wants it listed by name with the reason ART cannot read it — which is
    // more useful than calling the whole file unrecognised.
    if window.len() >= 4 && matches!(&window[..3], b"DOS" | b"PFS" | b"PDS" | b"SFS") {
        return Ok(bare_volume(path, &window, file_len));
    }

    // 3. Neither. §2.4 is explicit: report honestly, do not guess.
    Ok(ImageVolumes {
        layout: ImageLayout::Unknown,
        volumes: Vec::new(),
        pending_recovery: look_for_recovery(path),
        note: Some(
            "This file has no partition table and does not start with an AmigaDOS \
             signature, so ART cannot tell what is in it."
                .into(),
        ),
    })
}

fn read_window(path: &Path, file_len: u64) -> CoreResult<Vec<u8>> {
    use std::io::Read;

    let want = HEADER_WINDOW_BYTES.min(file_len as usize);
    let mut buf = vec![0u8; want];
    let mut file = std::fs::File::open(path)?;
    file.read_exact(&mut buf)?;
    Ok(buf)
}

fn scan_rdb(window: &[u8], file_len: u64, path: &Path) -> CoreResult<ImageVolumes> {
    let rdb = parse_rdb(window)?;

    let mut volumes = Vec::new();
    for partition in &rdb.partitions {
        let dos_type = DosType::new(partition.dostype.to_be_bytes());

        // Geometry straight out of a decades-old table: every product is
        // checked, and a partition whose numbers do not multiply is described
        // rather than mounted.
        let (offset, length) = match (partition.byte_offset(), partition.byte_length()) {
            (Some(offset), Some(length)) if length > 0 => (offset, length),
            _ => {
                volumes.push(VolumeEntry {
                    name: partition.drive_name.clone(),
                    filesystem: dos_type.label(),
                    dos_type: dos_type.0,
                    byte_offset: 0,
                    byte_length: 0,
                    block_size: 0,
                    reserved: partition.reserved,
                    bootable: partition.bootable,
                    boot_priority: partition.boot_priority,
                    unsupported: Some(
                        "the partition table describes an impossible size for this partition"
                            .into(),
                    ),
                    clamped: false,
                });
                continue;
            }
        };

        let block_size = partition.block_bytes() as usize;
        let clamped = offset.saturating_add(length) > file_len;

        volumes.push(VolumeEntry {
            name: partition.drive_name.clone(),
            filesystem: dos_type.label(),
            dos_type: dos_type.0,
            byte_offset: offset,
            byte_length: length,
            block_size,
            reserved: partition.reserved,
            bootable: partition.bootable,
            boot_priority: partition.boot_priority,
            unsupported: why_not(dos_type, block_size, offset, file_len),
            clamped,
        });
    }

    let note = (!rdb.checksum_valid).then(|| {
        "The partition table's checksum does not match. The partitions below are \
         what ART could read from it."
            .to_string()
    });

    Ok(ImageVolumes {
        layout: ImageLayout::Rdb,
        volumes,
        pending_recovery: look_for_recovery(path),
        note,
    })
}

fn bare_volume(path: &Path, window: &[u8], file_len: u64) -> ImageVolumes {
    let mut magic = [0u8; 4];
    magic.copy_from_slice(&window[..4]);
    let dos_type = DosType::new(magic);

    let name = path
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "DH0".into());

    ImageVolumes {
        layout: ImageLayout::BareVolume,
        volumes: vec![VolumeEntry {
            name,
            filesystem: dos_type.label(),
            dos_type: dos_type.0,
            byte_offset: 0,
            byte_length: file_len,
            block_size: SECTOR_BYTES,
            // No RDB means no DosEnvVec; two boot blocks is what AmigaDOS uses.
            reserved: 2,
            bootable: true,
            boot_priority: 0,
            unsupported: why_not(dos_type, SECTOR_BYTES, 0, file_len),
            clamped: false,
        }],
        pending_recovery: look_for_recovery(path),
        note: None,
    }
}

/// Why ART cannot open this volume, in the user's language.
fn why_not(dos_type: DosType, block_size: usize, offset: u64, file_len: u64) -> Option<String> {
    if let Some(reason) = dos_type.unsupported_reason() {
        return Some(reason);
    }
    // Only 512-byte blocks are supported. The hash table size and the
    // trailing header fields are both block-size dependent, and ART has
    // neither implemented nor tested the larger layout — claiming it would be
    // exactly the unverified support §10 and §89 forbid.
    if block_size != SECTOR_BYTES {
        return Some(format!(
            "{block_size}-byte blocks — ART only reads 512-byte blocks so far"
        ));
    }
    if offset >= file_len {
        return Some("this partition starts past the end of the file".into());
    }
    None
}

/// Open one of the volumes [`scan_image`] found.
///
/// Returns the device to read blocks from and the geometry to read them with.
pub fn mount(path: &Path, entry: &VolumeEntry) -> CoreResult<(FileRegion, VolumeGeometry)> {
    if let Some(reason) = &entry.unsupported {
        return Err(CoreError::UnsupportedFormat(format!(
            "'{}' cannot be opened: {reason}",
            entry.name
        )));
    }

    let device = FileRegion::open(path, entry.byte_offset, entry.byte_length, entry.block_size)?;

    // The geometry comes from what the device *actually* covers, not from what
    // the table claimed: a clamped partition must not be walked as if the
    // missing blocks were there.
    let geometry = VolumeGeometry::new(
        entry.block_size,
        <FileRegion as super::BlockDevice>::total_blocks(&device),
        entry.reserved,
        DosType::new(entry.dos_type),
    )?;

    Ok((device, geometry))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::adf::bootblock::FileSystemType;
    use crate::core::adf::create::create_blank_adf;
    use crate::core::volume::BlockDevice;

    fn scratch(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("art-mount-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    // ---- detection order (§2.4) ----

    /// The WHDLoad-world case: no RDB, the file *is* the filesystem.
    #[test]
    fn a_bare_hardfile_mounts_as_one_volume() {
        let dir = scratch("bare");
        let path = dir.join("Games.hdf");
        // A blank ADF is a bare DOS volume; the layout is identical, only the
        // size differs, which is exactly the point of the abstraction.
        std::fs::write(
            &path,
            create_blank_adf("Games", FileSystemType::Ffs, false).unwrap(),
        )
        .unwrap();

        let found = scan_image(&path).unwrap();

        assert_eq!(found.layout, ImageLayout::BareVolume);
        assert_eq!(found.volumes.len(), 1);
        assert_eq!(found.volumes[0].name, "Games");
        assert_eq!(found.volumes[0].filesystem, "FFS");
        assert!(found.volumes[0].is_mountable());
        assert_eq!(found.volumes[0].byte_offset, 0);

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_bare_volume_can_be_opened_and_read() {
        let dir = scratch("bareopen");
        let path = dir.join("Games.hdf");
        std::fs::write(
            &path,
            create_blank_adf("Games", FileSystemType::Ffs, false).unwrap(),
        )
        .unwrap();

        let found = scan_image(&path).unwrap();
        let (device, geometry) = mount(&path, &found.volumes[0]).unwrap();

        assert_eq!(device.total_blocks(), 1760);
        assert_eq!(geometry.root_block, 880);

        // The root block really is a header block.
        let root = crate::core::volume::read_block_vec(&device, geometry.root_block).unwrap();
        assert_eq!(i32::from_be_bytes([root[0], root[1], root[2], root[3]]), 2);

        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// §2.4 case 3. The old plain-HDF path guessed "FFS + dircache" for any
    /// file it did not recognise; guessing is what this replaces.
    #[test]
    fn a_file_that_is_neither_is_reported_as_unknown_not_guessed() {
        let dir = scratch("unknown");
        let path = dir.join("mystery.hdf");
        std::fs::write(&path, vec![0x42u8; 4096]).unwrap();

        let found = scan_image(&path).unwrap();

        assert_eq!(found.layout, ImageLayout::Unknown);
        assert!(found.volumes.is_empty());
        assert!(found.note.unwrap().contains("cannot tell"));

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_file_too_small_to_hold_a_block_is_refused() {
        let dir = scratch("tiny");
        let path = dir.join("tiny.hdf");
        std::fs::write(&path, vec![0u8; 10]).unwrap();

        assert!(scan_image(&path).is_err());

        std::fs::remove_dir_all(&dir).unwrap();
    }

    // ---- the honesty matrix (§2.5) ----

    #[test]
    fn an_unsupported_filesystem_is_listed_with_its_reason() {
        let dir = scratch("pfs");
        let path = dir.join("pfs.hdf");
        let mut image = vec![0u8; 4096];
        image[..4].copy_from_slice(b"PFS\x03");
        std::fs::write(&path, image).unwrap();

        let found = scan_image(&path).unwrap();

        // Listed, not hidden — the user learns the disk is fine.
        assert_eq!(found.layout, ImageLayout::BareVolume);
        assert_eq!(found.volumes[0].filesystem, "PFS3");
        assert!(!found.volumes[0].is_mountable());
        assert!(found.volumes[0]
            .unsupported
            .as_ref()
            .unwrap()
            .contains("PFS3"));

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn opening_an_unsupported_volume_says_why_rather_than_failing_obscurely() {
        let dir = scratch("pfsopen");
        let path = dir.join("pfs.hdf");
        let mut image = vec![0u8; 4096];
        image[..4].copy_from_slice(b"SFS\x00");
        std::fs::write(&path, image).unwrap();

        let found = scan_image(&path).unwrap();
        let err = mount(&path, &found.volumes[0]).unwrap_err();

        assert_eq!(err.code(), "ART-FORMAT-UNSUPPORTED");
        assert!(err.to_string().contains("SFS"), "{err}");

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_long_filename_volume_is_listed_but_refused() {
        let dir = scratch("lnfs");
        let path = dir.join("lnfs.hdf");
        let mut image = vec![0u8; 4096];
        image[..4].copy_from_slice(b"DOS\x07");
        std::fs::write(&path, image).unwrap();

        let found = scan_image(&path).unwrap();
        assert_eq!(found.volumes[0].filesystem, "FFS long filenames");
        assert!(found.volumes[0]
            .unsupported
            .as_ref()
            .unwrap()
            .contains("long-filename"));

        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// Dircache volumes are readable — the cache blocks are simply ignored
    /// while browsing. Only writing them would be a problem, and that is
    /// Stage W's decision, not this one's.
    #[test]
    fn a_dircache_volume_is_browsable() {
        let dir = scratch("dircache");
        let path = dir.join("dc.hdf");
        let mut image = vec![0u8; 4096];
        image[..4].copy_from_slice(b"DOS\x05");
        std::fs::write(&path, image).unwrap();

        let found = scan_image(&path).unwrap();
        assert!(found.volumes[0].is_mountable());

        std::fs::remove_dir_all(&dir).unwrap();
    }

    // ---- hostile geometry (§4.5) ----

    #[test]
    fn a_volume_that_claims_more_than_the_file_holds_is_clamped_and_flagged() {
        let dir = scratch("clamped");
        let path = dir.join("short.hdf");
        let mut image = create_blank_adf("Short", FileSystemType::Ffs, false).unwrap();
        image.truncate(900 * 512); // half a floppy
        std::fs::write(&path, image).unwrap();

        let found = scan_image(&path).unwrap();
        let (device, geometry) = mount(&path, &found.volumes[0]).unwrap();

        // The geometry follows what is really there, not what was claimed.
        assert_eq!(device.total_blocks(), 900);
        assert_eq!(geometry.total_blocks, 900);
        assert_eq!(geometry.root_block, 450);

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn mounting_never_panics_on_absurd_entries() {
        let dir = scratch("absurd");
        let path = dir.join("d.hdf");
        std::fs::write(&path, vec![0u8; 4096]).unwrap();

        let hostile = VolumeEntry {
            name: "DH9".into(),
            filesystem: "FFS".into(),
            dos_type: *b"DOS\x01",
            byte_offset: u64::MAX - 10,
            byte_length: u64::MAX,
            block_size: 512,
            reserved: 2,
            bootable: false,
            boot_priority: 0,
            unsupported: None,
            clamped: false,
        };

        assert!(mount(&path, &hostile).is_err());

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_block_size_other_than_512_is_refused_with_a_reason() {
        let reason = why_not(DosType::new(*b"DOS\x01"), 1024, 0, 100_000).unwrap();
        assert!(reason.contains("512-byte blocks"), "{reason}");

        assert!(why_not(DosType::new(*b"DOS\x01"), 512, 0, 100_000).is_none());
    }
}

#[cfg(test)]
mod rdb_tests {
    use super::*;
    use crate::core::adf::fs::list_directory_on;
    use crate::core::rdb::{AmigaHardDiskFs, PartitionSpec};
    use crate::core::volume::fixture::{make_ffs_volume, FixtureFile};
    use crate::core::volume::BlockDevice;
    use std::io::{Seek, SeekFrom, Write};

    /// The geometry `create_rdb_layout` writes: 16 heads x 63 sectors.
    const BLOCKS_PER_CYLINDER: u64 = 16 * 63;
    const BYTES_PER_CYLINDER: u64 = BLOCKS_PER_CYLINDER * 512;
    /// Cylinders the RDB itself reserves at the front.
    const RESERVED_CYLINDERS: u64 = 2;

    fn scratch(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("art-rdb-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// Write an ART-made image to `dest`, for the amitools oracle (§4.3).
    pub fn export_for_oracle(dest: &std::path::Path) {
        rdb_image_with(
            dest,
            &[FixtureFile {
                name: "Readme",
                content: b"hello from ART",
            }],
        );
    }

    /// Build an RDB image with one FFS partition holding `files`.
    fn rdb_image_with(path: &std::path::Path, files: &[FixtureFile]) -> (u64, u32) {
        crate::core::hdf::create_hdf(
            path,
            32 * 1024 * 1024,
            true,
            &[PartitionSpec {
                drive_name: "DH0".into(),
                fs_type: AmigaHardDiskFs::FfsStandard,
                size_mb: 10,
                bootable: true,
                boot_priority: 0,
                num_buffers: 100,
            }],
            &[],
        )
        .unwrap();

        // Where the partition must start, computed independently of the code
        // under test: the RDB reserves the first two cylinders.
        let offset = RESERVED_CYLINDERS * BYTES_PER_CYLINDER;

        // Find out how long it is, then format a volume of exactly that size.
        let found = scan_image(path).unwrap();
        let blocks = (found.volumes[0].byte_length / 512) as u32;

        let volume = make_ffs_volume(blocks, "Work", files);
        let mut file = std::fs::OpenOptions::new().write(true).open(path).unwrap();
        file.seek(SeekFrom::Start(offset)).unwrap();
        file.write_all(&volume).unwrap();
        file.sync_all().unwrap();

        (offset, blocks)
    }

    /// The whole point of Stage R: a filesystem that is not floppy-shaped,
    /// living at a non-zero offset inside a bigger file.
    #[test]
    fn a_partition_is_found_at_the_offset_the_geometry_implies() {
        let dir = scratch("offset");
        let path = dir.join("disk.hdf");
        let (expected_offset, blocks) = rdb_image_with(&path, &[]);

        let found = scan_image(&path).unwrap();

        assert_eq!(found.layout, ImageLayout::Rdb);
        assert_eq!(found.volumes.len(), 1);
        assert_eq!(found.volumes[0].name, "DH0");
        assert_eq!(
            found.volumes[0].byte_offset, expected_offset,
            "the partition must start after the reserved cylinders"
        );
        assert_eq!(found.volumes[0].block_size, 512);
        assert!(found.volumes[0].is_mountable());
        assert!(blocks > 1760, "the partition must not be floppy-sized");

        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// The root block of a partition is *not* 880 — which is exactly what the
    /// old floppy-shaped code could never have read.
    #[test]
    fn a_partition_mounts_with_its_own_geometry() {
        let dir = scratch("geometry");
        let path = dir.join("disk.hdf");
        let (_, blocks) = rdb_image_with(&path, &[]);

        let found = scan_image(&path).unwrap();
        let (device, geometry) = mount(&path, &found.volumes[0]).unwrap();

        assert_eq!(geometry.total_blocks, blocks);
        assert_eq!(geometry.root_block, blocks / 2);
        assert_ne!(geometry.root_block, 880, "this is not a floppy");
        assert_eq!(BlockDevice::total_blocks(&device), blocks);

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn files_inside_a_partition_are_listed() {
        let dir = scratch("list");
        let path = dir.join("disk.hdf");
        rdb_image_with(
            &path,
            &[
                FixtureFile {
                    name: "Readme",
                    content: b"hello from the partition",
                },
                FixtureFile {
                    name: "Startup-Sequence",
                    content: b"echo hi",
                },
            ],
        );

        let found = scan_image(&path).unwrap();
        let (device, geometry) = mount(&path, &found.volumes[0]).unwrap();

        let entries = list_directory_on(&device, geometry.root_block).unwrap();
        let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();

        assert_eq!(names, vec!["Readme", "Startup-Sequence"]);
        assert_eq!(entries[0].byte_size, 24);

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_file_inside_a_partition_extracts_byte_for_byte() {
        let dir = scratch("extract");
        let path = dir.join("disk.hdf");
        let payload: &[u8] = b"The quick brown fox jumps over the lazy dog. 0123456789";
        rdb_image_with(
            &path,
            &[FixtureFile {
                name: "Fox.txt",
                content: payload,
            }],
        );

        let found = scan_image(&path).unwrap();
        let (device, geometry) = mount(&path, &found.volumes[0]).unwrap();

        let entries = list_directory_on(&device, geometry.root_block).unwrap();
        let header =
            crate::core::adf::fs::read_header_on(&device, entries[0].header_block).unwrap();
        let bytes = crate::core::adf::extract::extract_file_on(
            &device,
            &header,
            crate::core::adf::bootblock::FileSystemType::Ffs,
        )
        .unwrap();

        assert_eq!(bytes, payload);

        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// §2.3: a volume past ~4000 blocks needs more than one bitmap block, and
    /// the root block has to point at all of them. A floppy never exercises
    /// this, which is why it went unnoticed for so long.
    #[test]
    fn a_partition_large_enough_needs_several_bitmap_blocks() {
        let dir = scratch("bitmapchain");
        let path = dir.join("disk.hdf");
        let (_, blocks) = rdb_image_with(&path, &[]);

        let found = scan_image(&path).unwrap();
        let (device, geometry) = mount(&path, &found.volumes[0]).unwrap();

        let root = crate::core::volume::read_block_vec(&device, geometry.root_block).unwrap();
        let parsed = crate::core::adf::blocks::RootBlock::parse(&root).unwrap();

        let described = blocks - 2;
        let expected =
            described.div_ceil(crate::core::adf::blocks::BLOCKS_PER_BITMAP_BLOCK as u32) as usize;
        assert!(expected > 1, "the fixture must be past one bitmap block");

        // bm_pages[0] is the first; the rest follow it contiguously here.
        assert!(parsed.bitmap_block > geometry.root_block);
        assert!(
            parsed.bitmap_block + expected as u32 - 1 < geometry.total_blocks,
            "every bitmap block must be inside the volume"
        );

        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// A volume validator that only knew about floppies would call a healthy
    /// partition damaged.
    #[test]
    fn a_healthy_partition_validates_clean() {
        let dir = scratch("validate");
        let path = dir.join("disk.hdf");
        rdb_image_with(&path, &[]);

        let found = scan_image(&path).unwrap();
        let (device, geometry) = mount(&path, &found.volumes[0]).unwrap();

        let report = crate::core::adf::validate::validate_volume(&device, &geometry).unwrap();
        assert_eq!(
            report.status,
            crate::core::adf::validate::HealthStatus::Healthy,
            "{:?}",
            report.findings
        );

        std::fs::remove_dir_all(&dir).unwrap();
    }
}

#[cfg(test)]
mod oracle_export {
    /// Write an RDB image for the amitools oracle (brief §4.2/§4.3).
    ///
    /// Not an assertion about ART: a hook so `rdbtool` can be pointed at an
    /// image ART wrote. Set `ART_ORACLE_OUT` to a path and run the suite.
    #[test]
    fn export_rdb_for_oracle_when_asked() {
        let Ok(dest) = std::env::var("ART_ORACLE_OUT") else {
            return;
        };
        super::rdb_tests::export_for_oracle(std::path::Path::new(&dest));
    }
}
