//! ADF (Amiga Disk File) engine — reading, validating and formatting.
//!
//! Parses AmigaDOS floppy images (OFS and FFS), walks the filesystem, extracts
//! files, validates image integrity and creates new blank/formatted disks.
//!
//! **Mutation does not live here.** It used to: `core/adf/mutate.rs` was a
//! second AmigaDOS writer that worked on a whole image in memory and hardcoded
//! a DD floppy's geometry — block 880 as the root, 881 as the only bitmap
//! block, 1760 as the size — which is right for every ADF and wrong for every
//! hard disk. [`crate::core::volume::write`] is that same set of operations
//! with the geometry as a parameter, working through a block device, and it is
//! now the only filesystem writer in ART. Two writers meant two ideas of the
//! same disk; one of them had to go.

pub mod bcpl;
pub mod blocks;
pub mod bootblock;
pub mod checksum;
pub mod create;
pub mod extract;
pub mod fs;
pub mod hash;
pub mod validate;

pub use bootblock::{BootBlock, FileSystemType};
pub use create::save_new_adf;
pub use fs::FileEntry;
pub use validate::{HealthStatus, ValidationReport};

use serde::{Deserialize, Serialize};

use crate::core::error::{CoreError, CoreResult};

/// A standard Amiga DD floppy = 80 cyl × 2 heads × 11 sectors × 512 bytes.
pub const DD_TOTAL_BLOCKS: usize = 1760;
/// Total byte size of a DD ADF.
pub const DD_SIZE: usize = DD_TOTAL_BLOCKS * blocks::BLOCK_SIZE;

/// The number of whole blocks an image holds, derived from its own length.
///
/// One place, deliberately: DD and HD ADFs differ only in block count, and
/// every consumer of that count (root-block placement, bitmap size, reported
/// capacity) must agree on it or they silently disagree about the same disk.
pub fn total_blocks_of(image: &[u8]) -> usize {
    image.len() / blocks::BLOCK_SIZE
}

/// Where the root block of an image of this size lives.
///
/// One place, deliberately. ART used to read this from the boot block, where
/// no such field exists (ART-037); the value is computed, and it is computed
/// here so two call sites cannot drift apart the way the reader and the
/// writer once did.
pub fn root_block_of(image: &[u8]) -> u32 {
    let total_blocks = total_blocks_of(image) as u32;
    crate::core::volume::VolumeGeometry::root_block_for(total_blocks)
}

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
        let root_block_num = root_block_of(&image);

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

        // The image's own size, not a floppy-shaped assumption. An HD ADF has
        // twice the blocks and its bitmap describes twice as many.
        let total_blocks = total_blocks_of(&self.image);
        let bm = blocks::Bitmap::parse(bm_block, total_blocks)?;
        let capacity_bytes = (total_blocks * blocks::BLOCK_SIZE) as u64;
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
/// version went (spec §92: state what will be backed up). The mutation itself
/// is performed by [`crate::core::volume::write::VolumeWriter`]; this type is
/// what `commands/adf.rs` hands the frontend afterwards.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MutationOutcome {
    pub info: AdfInfo,
    /// Absolute path of the backup taken before writing, when one was made.
    pub backup_path: Option<String>,
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

    #[test]
    fn an_hd_image_reports_its_real_capacity() {
        let mut image = vec![0u8; 3520 * blocks::BLOCK_SIZE];
        image[0..4].copy_from_slice(b"DOS\x01");

        let root = 1760 * blocks::BLOCK_SIZE;
        image[root..root + 4].copy_from_slice(&2i32.to_be_bytes());
        image[root + 12..root + 16].copy_from_slice(&72u32.to_be_bytes());
        // bm_pages[0] → block 1761, and bm_flag valid.
        image[root + 312..root + 316].copy_from_slice(&(-1i32).to_be_bytes());
        image[root + 316..root + 320].copy_from_slice(&1761u32.to_be_bytes());
        image[root + 508..root + 512].copy_from_slice(&1i32.to_be_bytes());

        let info = AdfImage::from_bytes(image).unwrap().info().unwrap();
        assert_eq!(
            info.capacity_bytes,
            3520 * blocks::BLOCK_SIZE as u64,
            "an HD image is 1.76 MB, not 880 KB"
        );
    }

    // The spec §57 data-safety pipeline used to be tested here, against
    // `mutate_disk_file`. That function is gone and the pipeline now lives in
    // `commands/volume_write.rs::with_volume`, so its tests live beside it:
    //
    // - the backup holds the previous image byte for byte
    //   → `commands::volume_write::tests::
    //      a_write_backs_up_the_previous_version_byte_for_byte`
    // - a failure after the mutation never reaches the disk
    //   → `commands::adf::tests::a_failure_after_the_mutation_never_reaches_the_disk`
    //   and `commands::volume_write::tests::
    //        a_refused_operation_leaves_the_image_byte_for_byte_unchanged`
    // - a write that would not validate is rolled back whole
    //   → `core::volume::write::tests::
    //      a_failed_validation_rolls_the_whole_operation_back`

    /// Open an image some other tool wrote and print what ART made of it.
    ///
    /// `scripts/oracle-check.py` has `xdftool` build a *bootable* floppy — the
    /// case ART used to fail on, because a bootable disk has 68000 code where
    /// ART looked for a block number.
    #[test]
    fn open_foreign_adf_for_oracle_when_asked() {
        let Ok(source) = std::env::var("ART_ADF_READ_IN") else {
            return;
        };
        let image = AdfImage::open(std::path::Path::new(&source)).unwrap();
        let info = image.info().unwrap();
        println!("volume={}", info.volume_name);
        println!("root={}", info.root_block);
        println!("capacity={}", info.capacity_bytes);
    }
}
