//! ADF block structures: root, directory/file headers, data blocks, bitmap.
//!
//! All offsets follow the official Commodore AmigaDOS filesystem specification
//! and the adflib reference implementation. ADF uses 512-byte blocks throughout.
//! Multi-byte integers are big-endian.

use serde::{Deserialize, Serialize};

use super::bcpl::{read_bcpl_string, AmigaDate};
use crate::core::error::{CoreError, CoreResult};

pub const BLOCK_SIZE: usize = 512;

/// Block primary type field values (offset 0, big-endian i32).
pub mod block_type {
    pub const HEADER: i32 = 2; // T_HEADER: root / file / dir header
    pub const DATA: i32 = 8; // T_DATA: OFS data block
    pub const LIST: i32 = 16; // T_LIST: data-blocks list (file extension)
    pub const FREE: i32 = -1;
}

/// Secondary type values (offset 508, big-endian i32 — LW 127).
pub mod block_subtype {
    pub const ROOT: i32 = 1; // ST_ROOT
    pub const USERDIR: i32 = 2; // ST_USERDIR
    pub const SOFTLINK: i32 = 3; // ST_SOFTLINK
    pub const LINKDIR: i32 = 4; // ST_LINKDIR
    pub const FILE: i32 = -3; // ST_FILE
    pub const LINKFILE: i32 = -4; // ST_LINKFILE
}

// ---------------------------------------------------------------------------
// Generic block readers
// ---------------------------------------------------------------------------

fn read_i32_be(buf: &[u8], offset: usize) -> CoreResult<i32> {
    let b: [u8; 4] = buf
        .get(offset..offset + 4)
        .ok_or_else(|| CoreError::Malformed {
            format: "adf".into(),
            detail: format!("read past end of block at offset {offset}"),
        })?
        .try_into()
        .unwrap();
    Ok(i32::from_be_bytes(b))
}

fn read_u32_be(buf: &[u8], offset: usize) -> CoreResult<u32> {
    Ok(read_i32_be(buf, offset)? as u32)
}

// ---------------------------------------------------------------------------
// Bounds-checked whole-image block access
// ---------------------------------------------------------------------------
//
// Block numbers reach the core straight from the frontend (`dirBlock`,
// `headerBlock`), and corrupt images can point anywhere. Indexing the image
// directly (`img[off..off + BLOCK_SIZE]`) turns a bad number into a panic —
// and because the release profile sets `panic = "abort"`, that panic takes the
// whole application down. Everything that touches a block by number goes
// through these helpers instead, so a bad block number becomes a normal error.

/// Byte offset of `block` within a whole image, checked against the image size.
fn block_offset(img_len: usize, block: u32) -> CoreResult<usize> {
    let offset = (block as usize)
        .checked_mul(BLOCK_SIZE)
        .ok_or_else(|| CoreError::Malformed {
            format: "adf".into(),
            detail: format!("block number {block} overflows the address space"),
        })?;
    if offset + BLOCK_SIZE > img_len {
        return Err(CoreError::Malformed {
            format: "adf".into(),
            detail: format!("block {block} is outside the image ({img_len} bytes)"),
        });
    }
    Ok(offset)
}

/// Borrow one 512-byte block of a whole image.
pub fn block_slice(img: &[u8], block: u32) -> CoreResult<&[u8]> {
    let off = block_offset(img.len(), block)?;
    Ok(&img[off..off + BLOCK_SIZE])
}

/// Mutably borrow one 512-byte block of a whole image.
pub fn block_slice_mut(img: &mut [u8], block: u32) -> CoreResult<&mut [u8]> {
    let off = block_offset(img.len(), block)?;
    Ok(&mut img[off..off + BLOCK_SIZE])
}

/// Read a big-endian u32 at `offset` inside `block`.
pub fn read_u32_at(img: &[u8], block: u32, offset: usize) -> CoreResult<u32> {
    let b = block_slice(img, block)?;
    read_u32_be(b, offset)
}

/// Write a big-endian u32 at `offset` inside `block`.
pub fn write_u32_at(img: &mut [u8], block: u32, offset: usize, value: u32) -> CoreResult<()> {
    let b = block_slice_mut(img, block)?;
    let slot = b
        .get_mut(offset..offset + 4)
        .ok_or_else(|| CoreError::Malformed {
            format: "adf".into(),
            detail: format!("write past end of block {block} at offset {offset}"),
        })?;
    slot.copy_from_slice(&value.to_be_bytes());
    Ok(())
}

// ---------------------------------------------------------------------------
// Root block (Standard 512-byte block at 880)
// ---------------------------------------------------------------------------

/// Standard hash-table size (number of longword buckets).
pub const HASH_TABLE_SIZE: usize = 72;
/// Byte offset of the hash table within the root block (LW 6).
pub const HASH_TABLE_OFFSET: usize = 24;

/// Parsed root block.
#[derive(Debug, Clone)]
pub struct RootBlock {
    pub volume_name: String,
    /// Hash table: 72 longwords, each a block pointer (0 = empty bucket).
    pub hash_table: [u32; HASH_TABLE_SIZE],
    /// Bitmap block pointer (usually 881, from bm_pages[0] at offset 316).
    pub bitmap_block: u32,
    /// Bitmap extension block (LW 104, offset 416; 0 if none).
    pub bitmap_extension: u32,
    pub created: AmigaDate,
}

impl RootBlock {
    /// Parse the root block from its 512-byte slice.
    pub fn parse(block: &[u8]) -> CoreResult<Self> {
        if block.len() < BLOCK_SIZE {
            return Err(CoreError::Malformed {
                format: "adf".into(),
                detail: "root block shorter than 512 bytes".into(),
            });
        }

        let typ = read_i32_be(block, 0)?;
        if typ != block_type::HEADER {
            return Err(CoreError::Malformed {
                format: "adf".into(),
                detail: format!("root block has type {typ}, expected {}", block_type::HEADER),
            });
        }

        let mut hash_table = [0u32; HASH_TABLE_SIZE];
        for (i, slot) in hash_table.iter_mut().enumerate() {
            *slot = read_u32_be(block, HASH_TABLE_OFFSET + i * 4)?;
        }

        // bm_pages array starts at offset 316 (LW 79). bm_pages[0] is the first bitmap block.
        let bitmap_block = read_u32_be(block, 316)?;
        // LW 104. Read from 412 until ART-034 — one longword early, which
        // matters on any volume large enough to need extension blocks.
        let bitmap_extension = read_u32_be(block, 416)?;

        // Disk/Volume name is at offset 432 (LW 108).
        let volume_name = read_bcpl_string(block, 432).unwrap_or_default();

        // Disk creation / alteration date is at offset 420 (LW 105).
        let days = read_u32_be(block, 420).unwrap_or(0);
        let mins = read_u32_be(block, 424).unwrap_or(0);
        let ticks = read_u32_be(block, 428).unwrap_or(0);

        Ok(Self {
            volume_name,
            hash_table,
            bitmap_block,
            bitmap_extension,
            created: AmigaDate { days, mins, ticks },
        })
    }
}

// ---------------------------------------------------------------------------
// File / Directory header block
// ---------------------------------------------------------------------------

/// Whether a header block is a directory or a file (from the sub-type field).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EntryKind {
    File,
    Directory,
}

/// A parsed file or directory header block.
#[derive(Debug, Clone)]
pub struct HeaderBlock {
    pub kind: EntryKind,
    pub name: String,
    /// Self block number (header key at offset 4).
    pub header_key: u32,
    /// Parent directory's block number (offset 500).
    pub parent: u32,
    /// Number of data blocks in this block's list (offset 8).
    pub block_count: u32,
    /// Byte size of the file (directories: 0). LW 81, offset 324.
    pub byte_size: u64,
    /// First data block (OFS chain; offset 16).
    pub first_data_block: u32,
    /// The list of data-block pointers held in *this* header block.
    pub data_blocks: Vec<u32>,
    pub comment: String,
    pub date: AmigaDate,
    /// Next header block in the same hash bucket (sibling chain, offset 496).
    pub next_hash: u32,
    /// Extension block pointer (for files exceeding 72 data blocks, offset 504).
    pub extension: u32,
}

/// Maximum data-block pointers that fit in one header/extension block (72 longwords).
pub const MAX_INLINE_DATA_BLOCKS: usize = 72;

impl HeaderBlock {
    /// Parse a file or directory header block.
    pub fn parse(block: &[u8]) -> CoreResult<Self> {
        if block.len() < BLOCK_SIZE {
            return Err(CoreError::Malformed {
                format: "adf".into(),
                detail: "header block shorter than 512 bytes".into(),
            });
        }

        let typ = read_i32_be(block, 0)?;
        if typ != block_type::HEADER {
            return Err(CoreError::Malformed {
                format: "adf".into(),
                detail: format!(
                    "header block has type {typ}, expected {}",
                    block_type::HEADER
                ),
            });
        }

        // Secondary type is at offset 508 (LW 127).
        let subtype = read_i32_be(block, 508).unwrap_or(0);
        let kind = if subtype == block_subtype::USERDIR
            || subtype == block_subtype::ROOT
            || subtype == block_subtype::LINKDIR
        {
            EntryKind::Directory
        } else {
            EntryKind::File
        };

        let header_key = read_u32_be(block, 4).unwrap_or(0);
        let block_count = read_u32_be(block, 8).unwrap_or(0);
        let first_data_block = read_u32_be(block, 16).unwrap_or(0);

        // In AmigaDOS, data block pointers in File Header are stored in reverse order
        // starting at longword 77 (offset 308) down to longword 6 (offset 24).
        let inline_count = (block_count as usize).min(MAX_INLINE_DATA_BLOCKS);
        let mut data_blocks = Vec::with_capacity(inline_count);
        if kind == EntryKind::File {
            for i in 0..inline_count {
                let off = (77 - i) * 4;
                let ptr = read_u32_be(block, off).unwrap_or(0);
                if ptr != 0 {
                    data_blocks.push(ptr);
                }
            }
        }

        // LW 81. Read from 316 until ART-034: that longword is unused, so
        // every file ART wrote looked like 0 bytes to AmigaOS.
        let byte_size = if kind == EntryKind::File {
            read_u32_be(block, 324).unwrap_or(0) as u64
        } else {
            0
        };

        // LW 82. Offset 320 is the protection bits, not the comment.
        let comment = read_bcpl_string(block, 328).unwrap_or_default();

        // Date is at offset 420 (days), 424 (mins), 428 (ticks).
        let days = read_u32_be(block, 420).unwrap_or(0);
        let mins = read_u32_be(block, 424).unwrap_or(0);
        let ticks = read_u32_be(block, 428).unwrap_or(0);

        // Filename is at offset 432.
        let name = read_bcpl_string(block, 432).unwrap_or_default();

        // Hash chain / next sibling in bucket is at offset 496.
        let next_hash = read_u32_be(block, 496).unwrap_or(0);
        // Parent directory block is at offset 500.
        let parent = read_u32_be(block, 500).unwrap_or(0);
        // Next extension block is at offset 504.
        let extension = read_u32_be(block, 504).unwrap_or(0);

        Ok(Self {
            kind,
            name,
            header_key,
            parent,
            block_count,
            byte_size,
            first_data_block,
            data_blocks,
            comment,
            date: AmigaDate { days, mins, ticks },
            next_hash,
            extension,
        })
    }
}

// ---------------------------------------------------------------------------
// OFS data block
// ---------------------------------------------------------------------------

/// OFS data-block header size.
pub const OFS_DATA_HEADER_SIZE: usize = 24;
/// Usable data bytes per OFS data block.
pub const OFS_DATA_BYTES: usize = BLOCK_SIZE - OFS_DATA_HEADER_SIZE;

/// A parsed OFS data block (the 24-byte header + 488 data bytes).
#[derive(Debug, Clone)]
pub struct DataBlock {
    pub header_key: u32,
    pub sequence: u32,
    pub data_size: u32,
    pub next_data_block: u32,
    pub checksum_valid: bool,
}

impl DataBlock {
    /// Parse the 24-byte header of an OFS data block.
    pub fn parse(block: &[u8]) -> CoreResult<Self> {
        if block.len() < BLOCK_SIZE {
            return Err(CoreError::Malformed {
                format: "adf".into(),
                detail: "data block shorter than 512 bytes".into(),
            });
        }
        let typ = read_i32_be(block, 0)?;
        if typ != block_type::DATA {
            return Err(CoreError::Malformed {
                format: "adf".into(),
                detail: format!("data block has type {typ}, expected {}", block_type::DATA),
            });
        }
        // OFS data checksum lives at offset 20.
        let checksum_valid = {
            use super::checksum::verify_block_checksum;
            verify_block_checksum(block, 20)
        };
        Ok(Self {
            header_key: read_u32_be(block, 4).unwrap_or(0),
            sequence: read_u32_be(block, 8).unwrap_or(0),
            data_size: read_u32_be(block, 12).unwrap_or(0),
            next_data_block: read_u32_be(block, 16).unwrap_or(0),
            checksum_valid,
        })
    }

    /// The data payload of an OFS data block (bytes 24..512).
    pub fn payload(block: &[u8], data_size: u32) -> &[u8] {
        let end = (OFS_DATA_HEADER_SIZE + data_size as usize).min(BLOCK_SIZE);
        &block[OFS_DATA_HEADER_SIZE..end]
    }
}

// ---------------------------------------------------------------------------
// Bitmap
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// Where a block's bit lives in the bitmap
// ---------------------------------------------------------------------------

/// Longwords of bitmap data in one bitmap block. The first longword is the
/// block's checksum, so 127 of the 128 carry bits.
pub const BITMAP_LONGWORDS: usize = 127;

/// Blocks described by a single bitmap block.
pub const BLOCKS_PER_BITMAP_BLOCK: usize = BITMAP_LONGWORDS * 32;

/// The position of a block's bit inside the bitmap.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BitPosition {
    /// Which bitmap block, counting from the first.
    pub bitmap_index: usize,
    /// Byte offset of the longword within that bitmap block.
    pub byte_offset: usize,
    /// The bit, as a mask.
    pub mask: u32,
}

/// Work out where block `block` is recorded, given the volume's reserved count.
///
/// Two details here are easy to get backwards and impossible to notice from
/// inside ART, because a reader and writer that share the mistake agree with
/// each other perfectly (ART-035):
///
/// - **The bitmap starts at the first non-reserved block.** Block `reserved`
///   is bit 0, not block 0.
/// - **Bits run from the least significant end** of each longword:
///   `1 << (n % 32)`, not `1 << (31 - n % 32)`.
///
/// Both confirmed against ADFlib (`adf_bitm.c`, `bitMask[i] == 1 << i` and
/// `sectOfMap = nSect - 2`) and amitools (`ADFSBitmap.get_bit`) on 2026-08-09.
///
/// Returns `None` for a block the bitmap does not describe — the reserved
/// blocks at the front, or one past the end.
pub fn bit_position(block: u32, reserved: u32, total_blocks: u32) -> Option<BitPosition> {
    if block < reserved || block >= total_blocks {
        return None;
    }
    let offset = (block - reserved) as usize;

    Some(BitPosition {
        bitmap_index: offset / BLOCKS_PER_BITMAP_BLOCK,
        // Longword 0 of a bitmap block is its checksum, so the data starts at
        // byte 4.
        byte_offset: 4 + ((offset / 32) % BITMAP_LONGWORDS) * 4,
        mask: 1u32 << (offset % 32),
    })
}

/// A parsed bitmap describing which blocks are free (1) vs used (0).
#[derive(Debug, Clone)]
pub struct Bitmap {
    /// One bit per block. Index 0 = block 0.
    bits: Vec<bool>,
}

impl Bitmap {
    /// Parse the bitmap from a bitmap block (typically block 881).
    ///
    /// `total_blocks` is the number of blocks in the volume. The bitmap only
    /// describes blocks from `reserved` onward, so the first `reserved` entries
    /// are reported as used — which is what they are.
    pub fn parse(block: &[u8], total_blocks: usize) -> CoreResult<Self> {
        Self::parse_with_reserved(block, total_blocks, 2)
    }

    /// Parse a bitmap block for a volume with an explicit reserved count.
    pub fn parse_with_reserved(
        block: &[u8],
        total_blocks: usize,
        reserved: u32,
    ) -> CoreResult<Self> {
        let mut bits = vec![false; total_blocks];

        for (block_num, slot) in bits.iter_mut().enumerate() {
            let Some(position) = bit_position(block_num as u32, reserved, total_blocks as u32)
            else {
                // Reserved blocks are not in the bitmap and are never free.
                continue;
            };
            // One bitmap block only; larger volumes chain, which browsing does
            // not need (Stage R §2.3).
            if position.bitmap_index != 0 || position.byte_offset + 4 > block.len() {
                continue;
            }
            let lw = u32::from_be_bytes([
                block[position.byte_offset],
                block[position.byte_offset + 1],
                block[position.byte_offset + 2],
                block[position.byte_offset + 3],
            ]);
            *slot = lw & position.mask != 0;
        }

        Ok(Self { bits })
    }

    /// True if `block` is marked free in the bitmap.
    pub fn is_free(&self, block: usize) -> bool {
        self.bits.get(block).copied().unwrap_or(false)
    }

    /// Count of free blocks.
    pub fn free_count(&self) -> usize {
        self.bits.iter().filter(|&&b| b).count()
    }

    /// Count of used blocks.
    pub fn used_count(&self) -> usize {
        self.bits.len() - self.free_count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// ART-035. The bitmap describes blocks from `reserved` onward, and bits
    /// run from the least significant end of each longword. This test used to
    /// assert the opposite on both counts — block 0 "free", bit 0 at the MSB —
    /// which is the shape ART had invented for itself.
    #[test]
    fn bitmap_marks_used_and_free() {
        let mut block = vec![0u8; BLOCK_SIZE];
        // Bit 0 (least significant) and bit 31 (most significant) set.
        block[4..8].copy_from_slice(&0x8000_0001u32.to_be_bytes());
        let bm = Bitmap::parse(&block, 1760).unwrap();

        // The two boot blocks are not in the bitmap and are never free.
        assert!(!bm.is_free(0));
        assert!(!bm.is_free(1));

        // Bit 0 is the first block the bitmap describes: block 2.
        assert!(bm.is_free(2));
        assert!(!bm.is_free(3));
        // Bit 31 is 31 blocks further on.
        assert!(bm.is_free(33));
        assert!(!bm.is_free(34));
    }

    /// Pinned against ADFlib (`adf_bitm.c`) and amitools (`ADFSBitmap`).
    #[test]
    fn bit_positions_match_the_reference_implementations() {
        // The first described block sits at the bottom bit of the first
        // longword, which starts after the bitmap block's own checksum.
        let first = bit_position(2, 2, 1760).unwrap();
        assert_eq!(first.bitmap_index, 0);
        assert_eq!(first.byte_offset, 4);
        assert_eq!(first.mask, 1);

        // 32 blocks later, the next longword.
        let next_long = bit_position(34, 2, 1760).unwrap();
        assert_eq!(next_long.byte_offset, 8);
        assert_eq!(next_long.mask, 1);

        // The top bit of the first longword.
        assert_eq!(bit_position(33, 2, 1760).unwrap().mask, 1 << 31);

        // Reserved blocks and anything past the end are not described.
        assert!(bit_position(0, 2, 1760).is_none());
        assert!(bit_position(1, 2, 1760).is_none());
        assert!(bit_position(1760, 2, 1760).is_none());
    }

    /// A volume bigger than one bitmap block chains into further ones.
    #[test]
    fn a_large_volume_spills_into_more_bitmap_blocks() {
        let per_block = BLOCKS_PER_BITMAP_BLOCK as u32;

        let last_of_first = bit_position(2 + per_block - 1, 2, 100_000).unwrap();
        assert_eq!(last_of_first.bitmap_index, 0);

        let first_of_second = bit_position(2 + per_block, 2, 100_000).unwrap();
        assert_eq!(first_of_second.bitmap_index, 1);
        assert_eq!(first_of_second.byte_offset, 4, "each block starts over");
        assert_eq!(first_of_second.mask, 1);
    }

    #[test]
    fn bitmap_counts() {
        let mut block = vec![0u8; BLOCK_SIZE];
        block[4..8].copy_from_slice(&0xFFFF_FFFFu32.to_be_bytes());
        let bm = Bitmap::parse(&block, 1760).unwrap();
        assert_eq!(bm.free_count(), 32);
        assert_eq!(bm.used_count(), 1760 - 32);
    }

    #[test]
    fn root_block_parses_minimal() {
        let mut block = vec![0u8; BLOCK_SIZE];
        // Type = HEADER (2).
        block[0..4].copy_from_slice(&2i32.to_be_bytes());
        // Volume name "DISK" at offset 432.
        block[432] = 4;
        block[433..437].copy_from_slice(b"DISK");
        let root = RootBlock::parse(&block).unwrap();
        assert_eq!(root.volume_name, "DISK");
        assert_eq!(root.hash_table, [0u32; HASH_TABLE_SIZE]);
    }

    #[test]
    fn root_block_rejects_wrong_type() {
        let mut block = vec![0u8; BLOCK_SIZE];
        block[0..4].copy_from_slice(&8i32.to_be_bytes()); // DATA, not HEADER
        let err = RootBlock::parse(&block).unwrap_err();
        assert!(matches!(err, CoreError::Malformed { .. }));
    }

    #[test]
    fn header_block_parses_file() {
        let mut block = vec![0u8; BLOCK_SIZE];
        // Type = HEADER (2).
        block[0..4].copy_from_slice(&2i32.to_be_bytes());
        // Header key = 1000.
        block[4..8].copy_from_slice(&1000u32.to_be_bytes());
        // Block count = 1.
        block[8..12].copy_from_slice(&1u32.to_be_bytes());
        // data_blocks[0] at offset 308 (LW 77) = 1001.
        block[308..312].copy_from_slice(&1001u32.to_be_bytes());
        // byte_size at LW 81 (offset 324).
        block[324..328].copy_from_slice(&500u32.to_be_bytes());
        // Name "GAME" at offset 432.
        block[432] = 4;
        block[433..437].copy_from_slice(b"GAME");
        // subtype at 508: -3 (ST_FILE).
        block[508..512].copy_from_slice(&(-3i32).to_be_bytes());
        let hdr = HeaderBlock::parse(&block).unwrap();
        assert_eq!(hdr.kind, EntryKind::File);
        assert_eq!(hdr.name, "GAME");
        assert_eq!(hdr.header_key, 1000);
        assert_eq!(hdr.byte_size, 500);
        assert_eq!(hdr.data_blocks, vec![1001]);
    }

    #[test]
    fn header_block_parses_directory() {
        let mut block = vec![0u8; BLOCK_SIZE];
        block[0..4].copy_from_slice(&2i32.to_be_bytes());
        // subtype at 508: 2 (ST_USERDIR) -> Directory.
        block[508..512].copy_from_slice(&2i32.to_be_bytes());
        let hdr = HeaderBlock::parse(&block).unwrap();
        assert_eq!(hdr.kind, EntryKind::Directory);
        assert_eq!(hdr.byte_size, 0);
    }

    #[test]
    fn block_access_rejects_out_of_range_numbers() {
        let img = vec![0u8; BLOCK_SIZE * 4];

        assert!(block_slice(&img, 3).is_ok());
        assert!(block_slice(&img, 4).is_err(), "one past the end");
        assert!(block_slice(&img, 99_999).is_err());
        assert!(block_slice(&img, u32::MAX).is_err(), "must not overflow");
    }

    #[test]
    fn block_word_access_is_bounds_checked() {
        let mut img = vec![0u8; BLOCK_SIZE * 2];

        write_u32_at(&mut img, 1, 16, 0xDEAD_BEEF).unwrap();
        assert_eq!(read_u32_at(&img, 1, 16).unwrap(), 0xDEAD_BEEF);

        // Straddling the end of a block is a read past its end.
        assert!(read_u32_at(&img, 1, BLOCK_SIZE - 2).is_err());
        assert!(write_u32_at(&mut img, 1, BLOCK_SIZE - 2, 1).is_err());
        // ...and so is any access to a block that isn't there.
        assert!(read_u32_at(&img, 7, 0).is_err());
    }

    #[test]
    fn data_block_parses_ofs_header() {
        let mut block = vec![0u8; BLOCK_SIZE];
        // Type = DATA (8).
        block[0..4].copy_from_slice(&8i32.to_be_bytes());
        // data_size at offset 12 = 488.
        block[12..16].copy_from_slice(&488u32.to_be_bytes());
        let db = DataBlock::parse(&block).unwrap();
        assert_eq!(db.data_size, 488);
    }
}
