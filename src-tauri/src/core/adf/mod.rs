//! ADF (Amiga Disk File) engine — read and write Phase 1.
//!
//! Parses AmigaDOS floppy images (OFS and FFS), walks the filesystem, extracts
//! files, validates image integrity, creates new blank/formatted disks, and
//! applies mutations (inject, mkdir, delete, rename).

pub mod bcpl;
pub mod blocks;
pub mod bootblock;
pub mod checksum;
pub mod create;
pub mod extract;
pub mod fs;
pub mod hash;
pub mod mutate;
pub mod validate;

pub use bootblock::{BootBlock, FileSystemType};
pub use create::save_new_adf;
pub use fs::FileEntry;
pub use mutate::{add_file, create_directory, delete_entry, rename_entry};
pub use validate::{HealthStatus, ValidationReport};

use serde::{Deserialize, Serialize};

use crate::core::error::{CoreError, CoreResult};

/// A standard Amiga DD floppy = 80 cyl × 2 heads × 11 sectors × 512 bytes.
pub const DD_TOTAL_BLOCKS: usize = 1760;
/// Total byte size of a DD ADF.
pub const DD_SIZE: usize = DD_TOTAL_BLOCKS * blocks::BLOCK_SIZE;

/// High-level information about an opened ADF (serialised to the frontend).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdfInfo {
    pub volume_name: String,
    pub fs_type: FileSystemType,
    pub international: bool,
    pub dir_cache: bool,
    pub bootable: bool,
    pub checksum_valid: bool,
    pub capacity_bytes: u64,
    pub used_bytes: u64,
    pub free_bytes: u64,
    pub file_count: usize,
    pub directory_count: usize,
    pub root_block: u32,
}

/// An opened ADF image held in memory.
#[derive(Debug, Clone)]
pub struct AdfImage {
    image: Vec<u8>,
    bootblock: BootBlock,
    root_block_num: u32,
}

impl AdfImage {
    /// Open and parse an ADF from a file path.
    pub fn open(path: &std::path::Path) -> CoreResult<Self> {
        let image = std::fs::read(path)?;
        Self::from_bytes(image)
    }

    /// Parse an in-memory byte buffer as an ADF.
    pub fn from_bytes(image: Vec<u8>) -> CoreResult<Self> {
        if image.len() < DD_SIZE {
            return Err(CoreError::Malformed {
                format: "adf".into(),
                detail: format!(
                    "file too small for DD floppy (got {} bytes, expected {})",
                    image.len(),
                    DD_SIZE
                ),
            });
        }

        // Parse bootblock (sectors 0 & 1 = 1024 bytes)
        let bootblock = BootBlock::parse(&image[..1024])?;

        // The root block is derived from the volume's size, not read from the
        // boot block — a boot block has no such field. Verified against
        // ADFlib (`adfVolCalcRootBlk`) and amitools (`calc_root_blk`).
        let total_blocks = (image.len() / blocks::BLOCK_SIZE) as u32;
        let root_block_num = crate::core::volume::VolumeGeometry::root_block_for(total_blocks);

        Ok(Self {
            image,
            bootblock,
            root_block_num,
        })
    }

    /// Return high-level summary info.
    pub fn info(&self) -> CoreResult<AdfInfo> {
        let root = blocks::RootBlock::parse(self.block(self.root_block_num)?)?;
        let stats = fs::walk_and_count(&self.image, self.root_block_num)?;
        let bm_block = self.block(root.bitmap_block)?;
        let bm = blocks::Bitmap::parse(bm_block, DD_TOTAL_BLOCKS)?;

        let capacity_bytes = (DD_TOTAL_BLOCKS * blocks::BLOCK_SIZE) as u64;
        let used_bytes = (bm.used_count() * blocks::BLOCK_SIZE) as u64;
        let free_bytes = (bm.free_count() * blocks::BLOCK_SIZE) as u64;

        Ok(AdfInfo {
            volume_name: root.volume_name,
            fs_type: self.bootblock.fs_type,
            international: self.bootblock.international,
            dir_cache: self.bootblock.dir_cache,
            bootable: self.bootblock.bootable,
            checksum_valid: self.bootblock.checksum_valid,
            capacity_bytes,
            used_bytes,
            free_bytes,
            file_count: stats.file_count,
            directory_count: stats.directory_count,
            root_block: self.root_block_num,
        })
    }

    /// List entries in the root directory.
    pub fn list_root(&self) -> CoreResult<Vec<FileEntry>> {
        fs::list_directory(&self.image, self.root_block_num)
    }

    /// List entries in a specific directory block.
    pub fn list_dir(&self, dir_block: u32) -> CoreResult<Vec<FileEntry>> {
        fs::list_directory(&self.image, dir_block)
    }

    /// Extract the payload of a file given its header block number.
    pub fn extract(&self, header_block: u32) -> CoreResult<Vec<u8>> {
        let hdr = blocks::HeaderBlock::parse(self.block(header_block)?)?;
        extract::extract_file(&self.image, &hdr, self.bootblock.fs_type)
    }

    /// Validate image health against the AmigaDOS spec.
    pub fn validate(&self) -> CoreResult<ValidationReport> {
        validate::validate_image(&self.image)
    }

    /// Access a single 512-byte block slice.
    pub fn block(&self, block_num: u32) -> CoreResult<&[u8]> {
        let offset = block_num as usize * blocks::BLOCK_SIZE;
        self.image
            .get(offset..offset + blocks::BLOCK_SIZE)
            .ok_or_else(|| CoreError::Malformed {
                format: "adf".into(),
                detail: format!("block {block_num} out of range"),
            })
    }

    pub fn bootblock(&self) -> &BootBlock {
        &self.bootblock
    }

    pub fn root_block(&self) -> u32 {
        self.root_block_num
    }

    pub fn bytes(&self) -> &[u8] {
        &self.image
    }

    pub fn bytes_mut(&mut self) -> &mut [u8] {
        &mut self.image
    }
}

/// The result of a successful on-disk mutation.
///
/// Carries the backup location so the UI can tell the user where the previous
/// version went (spec §92: state what will be backed up).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MutationOutcome {
    pub info: AdfInfo,
    /// Absolute path of the backup taken before writing, when one was made.
    pub backup_path: Option<String>,
}

/// Safely perform a transactional mutation on an on-disk ADF file.
///
/// Implements the spec §57 pipeline:
/// `read → parse → mutate → validate → backup → atomic commit`.
///
/// The original file is not touched until the mutated image has been re-parsed
/// and validated. If validation fails the error propagates and the on-disk
/// image is left exactly as it was.
pub fn mutate_disk_file<F>(path: &std::path::Path, mutate_fn: F) -> CoreResult<MutationOutcome>
where
    F: FnOnce(&mut Vec<u8>, FileSystemType) -> CoreResult<()>,
{
    let mut bytes = std::fs::read(path)?;

    // Parse the bootblock directly rather than building a throwaway AdfImage,
    // so the buffer is never cloned (an ADF is ~880 KB, an HDF far larger).
    if bytes.len() < DD_SIZE {
        return Err(CoreError::Malformed {
            format: "adf".into(),
            detail: format!(
                "file too small for DD floppy (got {} bytes, expected {})",
                bytes.len(),
                DD_SIZE
            ),
        });
    }
    let fs_type = BootBlock::parse(&bytes[..1024])?.fs_type;

    mutate_fn(&mut bytes, fs_type)?;

    // Re-parse and validate the mutated bytes BEFORE they reach the disk.
    let updated_adf = AdfImage::from_bytes(bytes)?;
    let report = updated_adf.validate()?;
    if report.status == HealthStatus::Problem {
        return Err(CoreError::Malformed {
            format: "adf".into(),
            detail: "modification resulted in invalid filesystem structure; \
                     the original image was not modified"
                .into(),
        });
    }

    // Back up the previous contents, then swap the new image in atomically.
    let backup = crate::core::safety::guarded_write(
        path,
        updated_adf.bytes(),
        crate::core::safety::BackupPolicy::DISK_IMAGE,
    )?;

    Ok(MutationOutcome {
        info: updated_adf.info()?,
        backup_path: backup.map(|p| p.to_string_lossy().into_owned()),
    })
}

#[cfg(test)]
mod mod_tests {
    use super::*;
    use create::create_blank_adf;

    fn make_blank_ffs_image() -> Vec<u8> {
        create_blank_adf("BlankDisk", FileSystemType::Ffs, false).unwrap()
    }

    #[test]
    fn open_blank_ffs_adf() {
        let bytes = make_blank_ffs_image();
        let adf = AdfImage::from_bytes(bytes).unwrap();
        let info = adf.info().unwrap();
        assert_eq!(info.volume_name, "BlankDisk");
        assert_eq!(info.fs_type, FileSystemType::Ffs);
        assert!(!info.bootable);
        assert!(info.checksum_valid);
    }

    #[test]
    fn list_root_of_blank_is_empty() {
        let bytes = make_blank_ffs_image();
        let adf = AdfImage::from_bytes(bytes).unwrap();
        let entries = adf.list_root().unwrap();
        assert!(entries.is_empty());
    }

    #[test]
    fn validate_blank_is_healthy() {
        let bytes = make_blank_ffs_image();
        let adf = AdfImage::from_bytes(bytes).unwrap();
        let rep = adf.validate().unwrap();
        assert_eq!(rep.status, HealthStatus::Healthy);
    }

    #[test]
    fn rejects_too_small_image() {
        let small = vec![0u8; 1000];
        let err = AdfImage::from_bytes(small).unwrap_err();
        assert!(matches!(err, CoreError::Malformed { .. }));
    }

    #[test]
    fn rejects_non_dos_signature() {
        let mut bytes = make_blank_ffs_image();
        bytes[0..4].copy_from_slice(b"NDOS");
        let err = AdfImage::from_bytes(bytes).unwrap_err();
        assert!(matches!(
            err,
            CoreError::UnsupportedFormat(..) | CoreError::Malformed { .. }
        ));
    }

    /// A bootable disk carries 68000 boot code from byte 8 onwards. ART used
    /// to read those bytes as a root-block pointer, which is why every
    /// bootable ADF failed to open with a nonsense block type.
    #[test]
    fn a_bootable_image_opens_because_the_root_block_is_computed() {
        use crate::core::adf::create::create_blank_adf;

        let mut image = create_blank_adf("Boot", FileSystemType::Ffs, false).unwrap();
        // Real boot code, not zeros: `bra.s` plus arbitrary following bytes.
        image[8..12].copy_from_slice(&[0x60, 0x0E, 0x75, 0x0B]);

        let opened = AdfImage::from_bytes(image).expect("a bootable ADF must open");
        assert_eq!(
            opened.info().unwrap().root_block,
            880,
            "a DD image's root block is 1760/2, whatever the boot code says"
        );
    }

    /// The same omission is why HD images never worked: the old path assumed
    /// the DD block count as well as the DD root.
    #[test]
    fn an_hd_image_finds_its_root_at_1760() {
        let mut image = vec![0u8; 3520 * blocks::BLOCK_SIZE];
        image[0..4].copy_from_slice(b"DOS\x01");

        let root = 1760 * blocks::BLOCK_SIZE;
        // A minimal root block: type T_HEADER, subtype ST_ROOT, hash size.
        image[root..root + 4].copy_from_slice(&2i32.to_be_bytes());
        image[root + 12..root + 16].copy_from_slice(&72u32.to_be_bytes());
        image[root + 508..root + 512].copy_from_slice(&1i32.to_be_bytes());

        let opened = AdfImage::from_bytes(image).expect("an HD ADF must open");
        assert_eq!(opened.info().unwrap().root_block, 1760);
    }

    // --- mutate_disk_file: the spec §57 data-safety pipeline ---

    fn scratch_disk(tag: &str) -> (std::path::PathBuf, std::path::PathBuf) {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("art-mutate-{tag}-{stamp}"));
        std::fs::create_dir_all(&dir).unwrap();
        let disk = dir.join("disk.adf");
        std::fs::write(&disk, make_blank_ffs_image()).unwrap();
        (dir, disk)
    }

    #[test]
    fn mutation_backs_up_the_previous_version() {
        let (dir, disk) = scratch_disk("backup");
        let before = std::fs::read(&disk).unwrap();

        let outcome = mutate_disk_file(&disk, |img, fs_type| {
            crate::core::adf::mutate::add_file(img, 880, "Note.txt", b"hi", fs_type)?;
            Ok(())
        })
        .unwrap();

        let backup = outcome
            .backup_path
            .expect("a backup should have been taken");
        // The backup holds the pre-modification image, byte for byte.
        assert_eq!(std::fs::read(&backup).unwrap(), before);
        // And the live file really did change.
        assert_ne!(std::fs::read(&disk).unwrap(), before);
        assert_eq!(outcome.info.file_count, 1);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn failed_mutation_leaves_the_original_untouched() {
        let (dir, disk) = scratch_disk("failed");
        let before = std::fs::read(&disk).unwrap();

        let result = mutate_disk_file(&disk, |_img, _fs| {
            Err(CoreError::InvalidInput("deliberate failure".into()))
        });

        assert!(result.is_err());
        assert_eq!(
            std::fs::read(&disk).unwrap(),
            before,
            "a failed mutation must not modify the on-disk image"
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn mutation_that_corrupts_the_image_is_not_committed() {
        let (dir, disk) = scratch_disk("corrupt");
        let before = std::fs::read(&disk).unwrap();

        // Wipe the root block: the mutation "succeeds" but the resulting image
        // is structurally invalid, so validation must reject the commit.
        let result = mutate_disk_file(&disk, |img, _fs| {
            let root = 880 * blocks::BLOCK_SIZE;
            img[root..root + blocks::BLOCK_SIZE].fill(0);
            Ok(())
        });

        assert!(result.is_err(), "corrupting mutation should be rejected");
        assert_eq!(
            std::fs::read(&disk).unwrap(),
            before,
            "the original must survive a rejected mutation"
        );
        std::fs::remove_dir_all(&dir).ok();
    }
}
