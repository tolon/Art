//! Hand-built AmigaDOS volumes, for tests only.
//!
//! Stage R is read-only, so ART has no way to *format* a volume at an arbitrary
//! geometry — that is Stage W's job. But the whole point of the stage is
//! reading a filesystem that is **not** floppy-shaped, and a test that only
//! ever sees 1760 blocks proves nothing about the generalisation.
//!
//! So these build the bytes directly from the on-disk layout: a boot block, a
//! root block at the computed position, and entries hashed into its buckets the
//! way AmigaDOS does it. Everything here is synthetic — ART ships no Amiga
//! content, ever.
//!
//! Reference for every offset: `lclevy` ADF FAQ, the same document the block
//! parsers were written against.

use crate::core::adf::blocks::{bit_position, BLOCKS_PER_BITMAP_BLOCK};
use crate::core::adf::checksum::block_checksum;
use crate::core::adf::hash::name_hash;

pub const BLOCK: usize = 512;

/// Block type `T_HEADER`.
const T_HEADER: i32 = 2;
/// Block type `T_DATA`.
const T_DATA: i32 = 8;
/// Secondary type `ST_ROOT`.
const ST_ROOT: i32 = 1;
/// Secondary type `ST_FILE`.
const ST_FILE: i32 = -3;
/// Secondary type `ST_USERDIR`.
const ST_USERDIR: i32 = 2;

const HASH_TABLE_OFFSET: usize = 24;
const HASH_TABLE_SIZE: usize = 72;
/// Boot blocks at the front, which the bitmap does not describe.
const RESERVED: u32 = 2;
/// Bitmap pointers that fit in the root block before the chain is needed.
const MAX_BM_PAGES: usize = 25;
/// Bitmap pointers in one extension block: the whole block bar its `next`.
const PAGES_PER_EXTENSION: usize = (BLOCK / 4) - 1;

/// Build a volume and describe it, which is what the write tests need.
///
/// `make_ffs_volume` predates the writer and returns bytes alone; every write
/// path needs the geometry as well, and deriving it twice is how a test comes
/// to disagree with the image it is testing.
pub fn ffs_volume(
    total_blocks: u32,
    dos_type: crate::core::volume::DosType,
) -> (Vec<u8>, crate::core::volume::VolumeGeometry) {
    let mut image = make_ffs_volume(total_blocks, "Work", &[]);
    // The builder always stamps FFS; honour the caller's flavour so OFS and
    // INTL volumes can be built from the same bytes.
    image[0..4].copy_from_slice(&dos_type.0);

    let geometry =
        crate::core::volume::VolumeGeometry::new(BLOCK, total_blocks, RESERVED, dos_type)
            .expect("a fixture geometry must be valid");
    (image, geometry)
}

/// A file to place in the volume being built.
pub struct FixtureFile {
    pub name: &'static str,
    pub content: &'static [u8],
}

/// Build a volume of `total_blocks` blocks holding `files` in its root.
///
/// FFS only: data blocks are raw, which keeps the fixture readable. The root
/// block lands wherever the volume's own geometry puts it, which is the
/// property under test.
pub fn make_ffs_volume(total_blocks: u32, volume_name: &str, files: &[FixtureFile]) -> Vec<u8> {
    let mut image = vec![0u8; total_blocks as usize * BLOCK];
    let root_block = total_blocks / 2;

    // ---- boot block ----
    image[0..4].copy_from_slice(b"DOS\x01");

    // ---- root block ----
    let root_off = root_block as usize * BLOCK;
    put_i32(&mut image, root_off, T_HEADER);
    put_i32(&mut image, root_off + 508, ST_ROOT);
    // Hash table size in longwords, at LW 3. Real tools read this rather than
    // assuming 72, and a zero here makes a valid volume list as empty.
    put_u32(&mut image, root_off + 12, HASH_TABLE_SIZE as u32);
    put_bcpl(&mut image, root_off + 432, volume_name);
    // bm_flag: -1 means the bitmap is valid. Left at 0 the volume reads as
    // "needs validating", which real tools refuse.
    put_i32(&mut image, root_off + 312, -1);

    // ---- bitmap blocks ----
    //
    // A floppy needs one; anything past ~4000 blocks needs several, which is
    // exactly the case Stage R exists to handle. They sit immediately after the
    // root block, and their numbers go into `bm_pages` at LW 79.
    let described = total_blocks.saturating_sub(RESERVED) as usize;
    let bitmap_count = described.div_ceil(BLOCKS_PER_BITMAP_BLOCK).max(1);
    let bitmap_blocks: Vec<u32> = (0..bitmap_count as u32)
        .map(|i| root_block + 1 + i)
        .collect();

    for (index, block) in bitmap_blocks.iter().enumerate().take(MAX_BM_PAGES) {
        put_u32(&mut image, root_off + 316 + index * 4, *block);
    }

    // ---- bitmap extension chain ----
    //
    // Past 25 bitmap blocks the root block runs out of room and the rest live
    // in a chain of extension blocks, each holding 127 more pointers with the
    // next extension block in its last longword. A ~64 MB partition needs 33
    // bitmap blocks, so this is not an exotic path — it is what any real hard
    // disk takes.
    let overflow = bitmap_blocks.len().saturating_sub(MAX_BM_PAGES);
    let ext_count = overflow.div_ceil(PAGES_PER_EXTENSION);
    let ext_blocks: Vec<u32> = (0..ext_count as u32)
        .map(|i| root_block + 1 + bitmap_count as u32 + i)
        .collect();

    if let Some(first) = ext_blocks.first() {
        put_u32(&mut image, root_off + 416, *first);
    }

    for (index, ext) in ext_blocks.iter().enumerate() {
        let off = *ext as usize * BLOCK;
        let start = MAX_BM_PAGES + index * PAGES_PER_EXTENSION;

        for slot in 0..PAGES_PER_EXTENSION {
            match bitmap_blocks.get(start + slot) {
                Some(page) => put_u32(&mut image, off + slot * 4, *page),
                None => break,
            }
        }
        // The last longword points at the next extension block, or is zero.
        let next = ext_blocks.get(index + 1).copied().unwrap_or(0);
        put_u32(&mut image, off + BLOCK - 4, next);
    }

    // Files go after the bitmap and its extension chain.
    let first_file_block = root_block + 1 + bitmap_count as u32 + ext_count as u32;
    let mut next_free = first_file_block;

    for file in files {
        let header_block = next_free;
        next_free += 1;

        // Data blocks: FFS, so the payload is the whole block.
        let mut data_blocks = Vec::new();
        for chunk in file.content.chunks(BLOCK) {
            let block = next_free;
            next_free += 1;
            let off = block as usize * BLOCK;
            image[off..off + chunk.len()].copy_from_slice(chunk);
            data_blocks.push(block);
        }

        // ---- file header ----
        let off = header_block as usize * BLOCK;
        put_i32(&mut image, off, T_HEADER);
        put_u32(&mut image, off + 4, header_block); // header_key: its own block
        put_u32(&mut image, off + 8, data_blocks.len() as u32); // block count
        if let Some(first) = data_blocks.first() {
            put_u32(&mut image, off + 16, *first);
        }
        // Data-block pointers run backwards from longword 77.
        for (i, block) in data_blocks.iter().enumerate() {
            put_u32(&mut image, off + (77 - i) * 4, *block);
        }
        put_u32(&mut image, off + 324, file.content.len() as u32); // byte size, LW 81
        put_bcpl(&mut image, off + 432, file.name);
        put_u32(&mut image, off + 500, root_block); // parent
        put_i32(&mut image, off + 508, ST_FILE);

        link_into_hash(&mut image, root_off, file.name, header_block);
        checksum_block(&mut image, header_block);
    }

    // ---- fill the bitmap ----
    //
    // Everything the volume actually uses is marked used; the rest is free.
    // A fixture with a bitmap that disagrees with its own contents is worse
    // than no bitmap: it would let a test pass while describing a disk no
    // filesystem would accept.
    for block in &bitmap_blocks {
        let off = *block as usize * BLOCK;
        // All bits free, then clear the ones in use. Longword 0 is the
        // checksum, so the data starts after it.
        for byte in image[off + 4..off + BLOCK].iter_mut() {
            *byte = 0xFF;
        }
    }

    let mut used = vec![root_block];
    used.extend_from_slice(&bitmap_blocks);
    used.extend_from_slice(&ext_blocks);
    used.extend(first_file_block..next_free);

    for block in used {
        mark_used(&mut image, &bitmap_blocks, block, total_blocks);
    }

    for block in &bitmap_blocks {
        checksum_block_at(&mut image, *block, 0);
    }

    // The root block's checksum comes last: linking entries into its hash
    // table changes its contents.
    checksum_block(&mut image, root_block);
    image
}

/// Clear a block's bit, marking it in use.
fn mark_used(image: &mut [u8], bitmap_blocks: &[u32], block: u32, total_blocks: u32) {
    let Some(position) = bit_position(block, RESERVED, total_blocks) else {
        return;
    };
    let Some(bitmap_block) = bitmap_blocks.get(position.bitmap_index) else {
        return;
    };

    let off = *bitmap_block as usize * BLOCK + position.byte_offset;
    let mut lw = u32::from_be_bytes([image[off], image[off + 1], image[off + 2], image[off + 3]]);
    lw &= !position.mask;
    image[off..off + 4].copy_from_slice(&lw.to_be_bytes());
}

/// Write a block's checksum into longword 5, the way AmigaDOS does.
///
/// Without it a volume is structurally complete and still rejected by real
/// tools — which is the difference between a fixture that proves something and
/// one that only satisfies ART.
pub fn checksum_block(image: &mut [u8], block: u32) {
    checksum_block_at(image, block, 20);
}

/// Checksum a block whose checksum longword is somewhere other than 20 —
/// bitmap blocks keep theirs at the very start.
pub fn checksum_block_at(image: &mut [u8], block: u32, offset: usize) {
    let off = block as usize * BLOCK;
    let slice = &mut image[off..off + BLOCK];
    let checksum = block_checksum(slice, offset);
    slice[offset..offset + 4].copy_from_slice(&checksum.to_be_bytes());
}

/// Add a subdirectory to a volume built by [`make_ffs_volume`], returning its
/// block. Files can then be linked into it with [`add_file_to_dir`].
pub fn add_directory(image: &mut [u8], root_block: u32, name: &str, at_block: u32) -> u32 {
    let off = at_block as usize * BLOCK;
    put_i32(image, off, T_HEADER);
    put_u32(image, off + 4, at_block);
    put_bcpl(image, off + 432, name);
    put_u32(image, off + 500, root_block);
    put_i32(image, off + 508, ST_USERDIR);

    link_into_hash(image, root_block as usize * BLOCK, name, at_block);
    checksum_block(image, at_block);
    checksum_block(image, root_block);
    at_block
}

/// Place a file inside a directory built by [`add_directory`].
pub fn add_file_to_dir(
    image: &mut [u8],
    dir_block: u32,
    name: &str,
    content: &[u8],
    header_block: u32,
    first_data_block: u32,
) {
    let mut data_blocks = Vec::new();
    for (block, chunk) in (first_data_block..).zip(content.chunks(BLOCK)) {
        let off = block as usize * BLOCK;
        image[off..off + chunk.len()].copy_from_slice(chunk);
        data_blocks.push(block);
    }

    let off = header_block as usize * BLOCK;
    put_i32(image, off, T_HEADER);
    put_u32(image, off + 4, header_block);
    put_u32(image, off + 8, data_blocks.len() as u32);
    for (i, b) in data_blocks.iter().enumerate() {
        put_u32(image, off + (77 - i) * 4, *b);
    }
    put_u32(image, off + 324, content.len() as u32);
    put_bcpl(image, off + 432, name);
    put_u32(image, off + 500, dir_block);
    put_i32(image, off + 508, ST_FILE);

    link_into_hash(image, dir_block as usize * BLOCK, name, header_block);
    checksum_block(image, header_block);
    checksum_block(image, dir_block);
}

/// Build an OFS data block, header and all, for OFS fixtures.
pub fn put_ofs_data_block(
    image: &mut [u8],
    block: u32,
    header_key: u32,
    sequence: u32,
    payload: &[u8],
    next: u32,
) {
    let off = block as usize * BLOCK;
    put_i32(image, off, T_DATA);
    put_u32(image, off + 4, header_key);
    put_u32(image, off + 8, sequence);
    put_u32(image, off + 12, payload.len() as u32);
    put_u32(image, off + 16, next);
    image[off + 24..off + 24 + payload.len()].copy_from_slice(payload);
}

/// Hook an entry into a directory's hash table, pushing onto the bucket chain.
///
/// The chain order is what AmigaDOS produces when entries are added: the newest
/// is the bucket head and points at the previous one.
fn link_into_hash(image: &mut [u8], dir_off: usize, name: &str, entry_block: u32) {
    // International mode off: these fixtures use plain `DOS\1`.
    let bucket = (name_hash(name, false) as usize) % HASH_TABLE_SIZE;
    let slot = dir_off + HASH_TABLE_OFFSET + bucket * 4;

    let existing = u32::from_be_bytes([
        image[slot],
        image[slot + 1],
        image[slot + 2],
        image[slot + 3],
    ]);
    if existing != 0 {
        // next_hash of the new entry points at whatever was there.
        put_u32(image, entry_block as usize * BLOCK + 496, existing);
    }
    put_u32(image, slot, entry_block);
}

fn put_u32(image: &mut [u8], offset: usize, value: u32) {
    image[offset..offset + 4].copy_from_slice(&value.to_be_bytes());
}

fn put_i32(image: &mut [u8], offset: usize, value: i32) {
    image[offset..offset + 4].copy_from_slice(&value.to_be_bytes());
}

/// Write a BCPL string: one length byte, then the characters.
fn put_bcpl(image: &mut [u8], offset: usize, text: &str) {
    let bytes = text.as_bytes();
    let len = bytes.len().min(30);
    image[offset] = len as u8;
    image[offset + 1..offset + 1 + len].copy_from_slice(&bytes[..len]);
}
