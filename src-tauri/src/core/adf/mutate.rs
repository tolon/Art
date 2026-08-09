//! ADF filesystem mutation engine (Phase 1).
//!
//! Handles inserting files, creating subdirectories, deleting entries, and
//! renaming entries with full bitmap allocation and AmigaDOS checksum tracking.
//!
//! Every block number that reaches this module may come from the frontend or
//! from a corrupt image, so nothing indexes the image directly — all access
//! goes through the bounds-checked helpers in [`super::blocks`]. A bad number
//! produces an error, never a panic (the release profile aborts on panic, so a
//! panic here would kill the whole application).

use super::bcpl::{write_bcpl_string, AmigaDate};
use super::blocks::{
    block_slice, block_slice_mut, block_subtype, block_type, read_u32_at, write_u32_at,
    HeaderBlock, BLOCK_SIZE, HASH_TABLE_OFFSET, HASH_TABLE_SIZE, MAX_INLINE_DATA_BLOCKS,
    OFS_DATA_BYTES, OFS_DATA_HEADER_SIZE,
};
use super::bootblock::{FileSystemType, DEFAULT_ROOT_BLOCK};
use super::checksum::block_checksum;
use super::create::set_bitmap_bit;
use super::hash::name_hash;
use super::DD_TOTAL_BLOCKS;
use crate::core::error::{CoreError, CoreResult};

/// Byte offset of the `next_hash` pointer inside a header block.
const NEXT_HASH_OFFSET: usize = 496;
/// Byte offset of the parent pointer inside a header block.
const PARENT_OFFSET: usize = 500;
/// Byte offset of the extension pointer inside a header block.
const EXTENSION_OFFSET: usize = 504;
/// Byte offset of the secondary type inside a header block.
const SUBTYPE_OFFSET: usize = 508;
/// Byte offset of the checksum inside a header/extension/root block.
const CHECKSUM_OFFSET: usize = 20;

// ---------------------------------------------------------------------------
// Validation helpers
// ---------------------------------------------------------------------------

/// Resolve a caller-supplied directory block and prove it really is a directory.
///
/// `0` means "the root directory". Anything else must be a block holding a
/// `T_HEADER` with an `ST_ROOT` or `ST_USERDIR` subtype — writing a hash-table
/// entry into a *file* header would silently corrupt the filesystem.
fn resolve_directory(img: &[u8], dir_block: u32) -> CoreResult<u32> {
    let block = if dir_block == 0 {
        DEFAULT_ROOT_BLOCK
    } else {
        dir_block
    };

    let primary = read_u32_at(img, block, 0)? as i32;
    if primary != block_type::HEADER {
        return Err(CoreError::InvalidInput(format!(
            "block {block} is not a directory (block type {primary})"
        )));
    }

    let subtype = read_u32_at(img, block, SUBTYPE_OFFSET)? as i32;
    if subtype != block_subtype::ROOT && subtype != block_subtype::USERDIR {
        return Err(CoreError::InvalidInput(format!(
            "block {block} is not a directory (secondary type {subtype})"
        )));
    }

    Ok(block)
}

/// Reject a name that already exists in `dir_block`.
///
/// AmigaDOS compares names case-insensitively, and two entries with the same
/// name in one directory make the hash chain ambiguous — delete and rename
/// would then act on whichever happened to come first.
fn ensure_name_available(img: &[u8], dir_block: u32, name: &str) -> CoreResult<()> {
    let entries = super::fs::list_directory(img, dir_block)?;
    if entries.iter().any(|e| e.name.eq_ignore_ascii_case(name)) {
        return Err(CoreError::InvalidInput(format!(
            "'{name}' already exists in this directory"
        )));
    }
    Ok(())
}

/// Byte offset of a hash bucket within a directory block.
fn bucket_offset(bucket: usize) -> CoreResult<usize> {
    if bucket >= HASH_TABLE_SIZE {
        return Err(CoreError::Malformed {
            format: "adf".into(),
            detail: format!("hash bucket {bucket} out of range"),
        });
    }
    Ok(HASH_TABLE_OFFSET + bucket * 4)
}

/// Recompute and store a block's AmigaDOS checksum.
fn refresh_checksum(img: &mut [u8], block: u32) -> CoreResult<()> {
    let slice = block_slice_mut(img, block)?;
    let cks = block_checksum(slice, CHECKSUM_OFFSET);
    slice[CHECKSUM_OFFSET..CHECKSUM_OFFSET + 4].copy_from_slice(&cks.to_be_bytes());
    Ok(())
}

/// Walk a hash-bucket chain to find the node whose `next_hash` points at
/// `target`. Returns `None` when the chain ends without reaching it.
///
/// The step limit stops a corrupt image whose chain loops back on itself from
/// hanging the UI forever.
fn find_chain_predecessor(img: &[u8], head: u32, target: u32) -> CoreResult<Option<u32>> {
    let mut current = head;
    let mut steps = 0usize;

    while current != 0 {
        steps += 1;
        if steps > DD_TOTAL_BLOCKS {
            return Err(CoreError::Malformed {
                format: "adf".into(),
                detail: "directory hash chain loops back on itself".into(),
            });
        }

        let next = read_u32_at(img, current, NEXT_HASH_OFFSET)?;
        if next == target {
            return Ok(Some(current));
        }
        current = next;
    }

    Ok(None)
}

/// Unlink `header_block` from its bucket in `parent_block`.
///
/// Errors when the entry is not actually in that chain — silently carrying on
/// would leave the entry linked from two places at once.
fn unlink_from_bucket(
    img: &mut [u8],
    parent_block: u32,
    header_block: u32,
    bucket: usize,
    next_hash: u32,
    entry_name: &str,
) -> CoreResult<()> {
    let bucket_off = bucket_offset(bucket)?;
    let head = read_u32_at(img, parent_block, bucket_off)?;

    if head == header_block {
        write_u32_at(img, parent_block, bucket_off, next_hash)?;
        return Ok(());
    }

    match find_chain_predecessor(img, head, header_block)? {
        Some(prev) => {
            write_u32_at(img, prev, NEXT_HASH_OFFSET, next_hash)?;
            refresh_checksum(img, prev)?;
            Ok(())
        }
        None => Err(CoreError::Malformed {
            format: "adf".into(),
            detail: format!("entry '{entry_name}' not found in parent directory hash chain"),
        }),
    }
}

/// Link `header_block` at the head of its bucket in `parent_block`.
fn link_into_bucket(
    img: &mut [u8],
    parent_block: u32,
    header_block: u32,
    bucket: usize,
) -> CoreResult<()> {
    let bucket_off = bucket_offset(bucket)?;
    let old_head = read_u32_at(img, parent_block, bucket_off)?;

    write_u32_at(img, header_block, NEXT_HASH_OFFSET, old_head)?;
    refresh_checksum(img, header_block)?;

    write_u32_at(img, parent_block, bucket_off, header_block)?;
    refresh_checksum(img, parent_block)?;

    Ok(())
}

// ---------------------------------------------------------------------------
// Block allocation
// ---------------------------------------------------------------------------

/// Whether the bitmap marks `block` as free.
///
/// One place, so the allocator and every capacity check read the bitmap the
/// same way. A floppy's bitmap is a single block at 881.
fn is_block_free(img: &[u8], block: u32) -> CoreResult<bool> {
    let Some(position) = super::blocks::bit_position(
        block,
        super::create::FLOPPY_RESERVED,
        DD_TOTAL_BLOCKS as u32,
    ) else {
        return Ok(false);
    };
    if position.bitmap_index != 0 {
        return Ok(false);
    }
    let lw = read_u32_at(img, 881, position.byte_offset)?;
    Ok(lw & position.mask != 0)
}

/// How many blocks the bitmap still reports as free.
///
/// Deliberately next to [`allocate_blocks`] and walking the bitmap the same
/// way: a capacity check that disagreed with the allocator would be worse than
/// no check at all, because it would promise room that does not exist.
///
/// Used to refuse an install *before* anything is written, so the user is told
/// "this will not fit" instead of watching an operation fail half way.
pub fn free_block_count(img: &[u8]) -> CoreResult<usize> {
    let mut free = 0;
    for block_num in 2..DD_TOTAL_BLOCKS {
        if block_num == 880 || block_num == 881 {
            continue;
        }
        if is_block_free(img, block_num as u32)? {
            free += 1;
        }
    }
    Ok(free)
}

/// Allocate `count` free blocks from the ADF bitmap.
pub fn allocate_blocks(img: &mut [u8], count: usize) -> CoreResult<Vec<u32>> {
    if count == 0 {
        return Ok(Vec::new());
    }

    let mut allocated = Vec::with_capacity(count);

    // Scan the 1760 blocks (skip 0, 1, 880, 881)
    for block_num in 2..DD_TOTAL_BLOCKS {
        if block_num == 880 || block_num == 881 {
            continue;
        }
        if is_block_free(img, block_num as u32)? {
            allocated.push(block_num as u32);
            if allocated.len() == count {
                break;
            }
        }
    }

    if allocated.len() < count {
        return Err(CoreError::Malformed {
            format: "adf".into(),
            detail: format!(
                "insufficient disk space (needed {} blocks, found {})",
                count,
                allocated.len()
            ),
        });
    }

    // Mark all allocated blocks as used in bitmap
    {
        let bm_slice = block_slice_mut(img, 881)?;
        for &b in &allocated {
            set_bitmap_bit(bm_slice, b as usize, false);
        }
        // Recompute bitmap checksum at offset 0
        let bm_cks = block_checksum(bm_slice, 0);
        bm_slice[0..4].copy_from_slice(&bm_cks.to_be_bytes());
    }

    // Clear the allocated blocks
    for &b in &allocated {
        block_slice_mut(img, b)?.fill(0);
    }

    Ok(allocated)
}

/// Free a list of blocks back to the ADF bitmap.
pub fn free_blocks(img: &mut [u8], blocks: &[u32]) -> CoreResult<()> {
    if blocks.is_empty() {
        return Ok(());
    }
    let bm_slice = block_slice_mut(img, 881)?;

    for &b in blocks {
        let bn = b as usize;
        if bn < DD_TOTAL_BLOCKS && bn != 0 && bn != 1 && bn != 880 && bn != 881 {
            set_bitmap_bit(bm_slice, bn, true);
        }
    }
    // Recompute bitmap checksum at offset 0
    let bm_cks = block_checksum(bm_slice, 0);
    bm_slice[0..4].copy_from_slice(&bm_cks.to_be_bytes());
    Ok(())
}

// ---------------------------------------------------------------------------
// Mutations
// ---------------------------------------------------------------------------

/// Add a file into `parent_dir_block` (pass 880 or 0 for the root directory).
pub fn add_file(
    img: &mut [u8],
    parent_dir_block: u32,
    filename: &str,
    data: &[u8],
    fs_type: FileSystemType,
) -> CoreResult<u32> {
    let clean_name = sanitize_amiga_filename(filename)?;
    let parent_block = resolve_directory(img, parent_dir_block)?;
    ensure_name_available(img, parent_block, &clean_name)?;

    // Calculate block requirements
    let payload_per_block = if fs_type.has_data_header() {
        OFS_DATA_BYTES
    } else {
        BLOCK_SIZE
    };

    let data_blocks_count = data.len().div_ceil(payload_per_block);

    let ext_blocks_count = if data_blocks_count > MAX_INLINE_DATA_BLOCKS {
        (data_blocks_count - MAX_INLINE_DATA_BLOCKS).div_ceil(MAX_INLINE_DATA_BLOCKS)
    } else {
        0
    };

    let total_needed = 1 + data_blocks_count + ext_blocks_count;
    let allocated = allocate_blocks(img, total_needed)?;

    let header_block_num = allocated[0];
    let data_block_nums = &allocated[1..1 + data_blocks_count];
    let ext_block_nums = &allocated[1 + data_blocks_count..];

    // 1. Write Data Blocks
    let mut data_offset = 0;
    for (seq, &block_num) in data_block_nums.iter().enumerate() {
        let chunk_len = (data.len() - data_offset).min(payload_per_block);
        let chunk = &data[data_offset..data_offset + chunk_len];

        if fs_type.has_data_header() {
            // OFS Data Block Header (24 bytes)
            let next_blk = if seq + 1 < data_block_nums.len() {
                data_block_nums[seq + 1]
            } else {
                0
            };
            let blk_slice = block_slice_mut(img, block_num)?;
            blk_slice[0..4].copy_from_slice(&block_type::DATA.to_be_bytes()); // T_DATA = 8
            blk_slice[4..8].copy_from_slice(&header_block_num.to_be_bytes());
            blk_slice[8..12].copy_from_slice(&((seq + 1) as u32).to_be_bytes());
            blk_slice[12..16].copy_from_slice(&(chunk_len as u32).to_be_bytes());
            blk_slice[16..20].copy_from_slice(&next_blk.to_be_bytes());

            // Payload
            blk_slice[OFS_DATA_HEADER_SIZE..OFS_DATA_HEADER_SIZE + chunk_len]
                .copy_from_slice(chunk);

            // OFS Checksum at offset 20
            let cks = block_checksum(blk_slice, CHECKSUM_OFFSET);
            blk_slice[CHECKSUM_OFFSET..CHECKSUM_OFFSET + 4].copy_from_slice(&cks.to_be_bytes());
        } else {
            // FFS Raw Payload
            block_slice_mut(img, block_num)?[..chunk_len].copy_from_slice(chunk);
        }

        data_offset += chunk_len;
    }

    // 2. Write Extension Blocks (if > 72 data blocks)
    let inline_count = data_blocks_count.min(MAX_INLINE_DATA_BLOCKS);
    let mut ext_ptr = 0u32;
    if !ext_block_nums.is_empty() {
        ext_ptr = ext_block_nums[0];
        let mut remaining_data_blocks = &data_block_nums[MAX_INLINE_DATA_BLOCKS..];

        for (ext_idx, &ext_block) in ext_block_nums.iter().enumerate() {
            let next_ext = if ext_idx + 1 < ext_block_nums.len() {
                ext_block_nums[ext_idx + 1]
            } else {
                0
            };
            let count_in_this_ext = remaining_data_blocks.len().min(MAX_INLINE_DATA_BLOCKS);
            let ext_slice = block_slice_mut(img, ext_block)?;

            ext_slice[0..4].copy_from_slice(&block_type::LIST.to_be_bytes()); // T_LIST = 16
            ext_slice[4..8].copy_from_slice(&ext_block.to_be_bytes());
            ext_slice[8..12].copy_from_slice(&(count_in_this_ext as u32).to_be_bytes());

            // Write reverse pointers from LW 77 (offset 308) down
            for (i, &data_block) in remaining_data_blocks
                .iter()
                .take(count_in_this_ext)
                .enumerate()
            {
                let off = (77 - i) * 4;
                ext_slice[off..off + 4].copy_from_slice(&data_block.to_be_bytes());
            }

            ext_slice[PARENT_OFFSET..PARENT_OFFSET + 4]
                .copy_from_slice(&header_block_num.to_be_bytes());
            ext_slice[EXTENSION_OFFSET..EXTENSION_OFFSET + 4]
                .copy_from_slice(&next_ext.to_be_bytes());
            ext_slice[SUBTYPE_OFFSET..SUBTYPE_OFFSET + 4]
                .copy_from_slice(&block_subtype::FILE.to_be_bytes());

            let cks = block_checksum(ext_slice, CHECKSUM_OFFSET);
            ext_slice[CHECKSUM_OFFSET..CHECKSUM_OFFSET + 4].copy_from_slice(&cks.to_be_bytes());

            remaining_data_blocks = &remaining_data_blocks[count_in_this_ext..];
        }
    }

    // 3. Write File Header Block
    {
        let hdr_slice = block_slice_mut(img, header_block_num)?;

        hdr_slice[0..4].copy_from_slice(&block_type::HEADER.to_be_bytes()); // T_HEADER = 2
        hdr_slice[4..8].copy_from_slice(&header_block_num.to_be_bytes());
        hdr_slice[8..12].copy_from_slice(&(inline_count as u32).to_be_bytes());
        let first_blk = data_block_nums.first().copied().unwrap_or(0);
        hdr_slice[16..20].copy_from_slice(&first_blk.to_be_bytes());

        // Data blocks in reverse order starting at LW 77 (offset 308)
        for (i, &data_block) in data_block_nums.iter().take(inline_count).enumerate() {
            let off = (77 - i) * 4;
            hdr_slice[off..off + 4].copy_from_slice(&data_block.to_be_bytes());
        }

        // byte_size at LW 81 (offset 324), not 316 — see ART-034.
        hdr_slice[324..328].copy_from_slice(&(data.len() as u32).to_be_bytes());

        let now = get_current_amiga_date();
        hdr_slice[420..424].copy_from_slice(&now.days.to_be_bytes());
        hdr_slice[424..428].copy_from_slice(&now.mins.to_be_bytes());
        hdr_slice[428..432].copy_from_slice(&now.ticks.to_be_bytes());

        write_bcpl_string(hdr_slice, 432, &clean_name, 32);

        hdr_slice[PARENT_OFFSET..PARENT_OFFSET + 4].copy_from_slice(&parent_block.to_be_bytes());
        hdr_slice[EXTENSION_OFFSET..EXTENSION_OFFSET + 4].copy_from_slice(&ext_ptr.to_be_bytes());
        hdr_slice[SUBTYPE_OFFSET..SUBTYPE_OFFSET + 4]
            .copy_from_slice(&block_subtype::FILE.to_be_bytes()); // ST_FILE = -3
    }

    // 4. Link into parent directory's hash table
    let bucket = name_hash_for(img, &clean_name)? as usize;
    link_into_bucket(img, parent_block, header_block_num, bucket)?;

    Ok(header_block_num)
}

/// Create a new subdirectory inside `parent_dir_block`.
pub fn create_directory(img: &mut [u8], parent_dir_block: u32, dirname: &str) -> CoreResult<u32> {
    let clean_name = sanitize_amiga_filename(dirname)?;
    let parent_block = resolve_directory(img, parent_dir_block)?;
    ensure_name_available(img, parent_block, &clean_name)?;

    let allocated = allocate_blocks(img, 1)?;
    let dir_block_num = allocated[0];

    {
        let dir_slice = block_slice_mut(img, dir_block_num)?;

        dir_slice[0..4].copy_from_slice(&block_type::HEADER.to_be_bytes()); // T_HEADER = 2
        dir_slice[4..8].copy_from_slice(&dir_block_num.to_be_bytes());
        dir_slice[12..16].copy_from_slice(&(HASH_TABLE_SIZE as u32).to_be_bytes());

        let now = get_current_amiga_date();
        dir_slice[420..424].copy_from_slice(&now.days.to_be_bytes());
        dir_slice[424..428].copy_from_slice(&now.mins.to_be_bytes());
        dir_slice[428..432].copy_from_slice(&now.ticks.to_be_bytes());

        write_bcpl_string(dir_slice, 432, &clean_name, 32);

        dir_slice[PARENT_OFFSET..PARENT_OFFSET + 4].copy_from_slice(&parent_block.to_be_bytes());
        dir_slice[SUBTYPE_OFFSET..SUBTYPE_OFFSET + 4]
            .copy_from_slice(&block_subtype::USERDIR.to_be_bytes()); // ST_USERDIR = 2
    }

    let bucket = name_hash_for(img, &clean_name)? as usize;
    link_into_bucket(img, parent_block, dir_block_num, bucket)?;

    Ok(dir_block_num)
}

/// Delete an entry (file or empty directory) from the disk.
pub fn delete_entry(img: &mut [u8], parent_dir_block: u32, header_block: u32) -> CoreResult<()> {
    let parent_block = resolve_directory(img, parent_dir_block)?;
    let hdr = HeaderBlock::parse(block_slice(img, header_block)?)?;

    // If directory, ensure it is empty
    if hdr.kind == super::blocks::EntryKind::Directory {
        let entries = super::fs::list_directory(img, header_block)?;
        if !entries.is_empty() {
            return Err(CoreError::InvalidInput("directory is not empty".into()));
        }
    }

    // 1. Unlink from parent directory's hash bucket
    let bucket = name_hash_for(img, &hdr.name)? as usize;
    unlink_from_bucket(
        img,
        parent_block,
        header_block,
        bucket,
        hdr.next_hash,
        &hdr.name,
    )?;
    refresh_checksum(img, parent_block)?;

    // 2. Collect all blocks to free
    let mut blocks_to_free = vec![header_block];
    blocks_to_free.extend_from_slice(&hdr.data_blocks);

    let mut ext = hdr.extension;
    let mut steps = 0usize;
    while ext != 0 {
        steps += 1;
        if steps > DD_TOTAL_BLOCKS {
            return Err(CoreError::Malformed {
                format: "adf".into(),
                detail: "file extension chain loops back on itself".into(),
            });
        }

        blocks_to_free.push(ext);
        let count = read_u32_at(img, ext, 8)? as usize;
        for i in 0..count.min(MAX_INLINE_DATA_BLOCKS) {
            let ptr = read_u32_at(img, ext, (77 - i) * 4)?;
            if ptr != 0 {
                blocks_to_free.push(ptr);
            }
        }
        ext = read_u32_at(img, ext, EXTENSION_OFFSET)?;
    }

    // 3. Free blocks in bitmap
    free_blocks(img, &blocks_to_free)?;

    // 4. Clear freed header block
    block_slice_mut(img, header_block)?.fill(0);

    Ok(())
}

/// Rename an entry on the disk.
pub fn rename_entry(
    img: &mut [u8],
    parent_dir_block: u32,
    header_block: u32,
    new_name: &str,
) -> CoreResult<()> {
    let clean_new_name = sanitize_amiga_filename(new_name)?;
    let parent_block = resolve_directory(img, parent_dir_block)?;

    let hdr = HeaderBlock::parse(block_slice(img, header_block)?)?;
    let old_name = hdr.name.clone();

    if old_name == clean_new_name {
        return Ok(());
    }
    // A pure case change keeps the same hash bucket, but any other collision
    // would create two entries with the same name.
    if !old_name.eq_ignore_ascii_case(&clean_new_name) {
        ensure_name_available(img, parent_block, &clean_new_name)?;
    }

    // 1. Unlink from the old bucket. If the entry is not in that chain the
    //    image is inconsistent — stop rather than linking it in a second time.
    let old_bucket = name_hash_for(img, &old_name)? as usize;
    unlink_from_bucket(
        img,
        parent_block,
        header_block,
        old_bucket,
        hdr.next_hash,
        &old_name,
    )?;

    // 2. Write the new name, then link into its bucket.
    {
        let hdr_slice = block_slice_mut(img, header_block)?;
        write_bcpl_string(hdr_slice, 432, &clean_new_name, 32);
    }

    let new_bucket = name_hash_for(img, &clean_new_name)? as usize;
    link_into_bucket(img, parent_block, header_block, new_bucket)?;

    Ok(())
}

// ---------------------------------------------------------------------------
// Small helpers
// ---------------------------------------------------------------------------

/// Hash a name using the disk's own case-folding rules.
///
/// International disks fold accented characters differently, so the flag has to
/// come from the image's bootblock rather than being assumed.
fn name_hash_for(img: &[u8], name: &str) -> CoreResult<u32> {
    let international = super::bootblock::BootBlock::parse(img.get(..1024).ok_or_else(|| {
        CoreError::Malformed {
            format: "adf".into(),
            detail: "image too small to contain a bootblock".into(),
        }
    })?)
    .map(|bb| bb.international)
    .unwrap_or(false);

    Ok(name_hash(name, international))
}

fn sanitize_amiga_filename(name: &str) -> CoreResult<String> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err(CoreError::InvalidInput("name cannot be empty".into()));
    }
    if trimmed.len() > 30 {
        return Err(CoreError::InvalidInput(
            "AmigaDOS filenames must not exceed 30 characters".into(),
        ));
    }
    if trimmed.contains('/') || trimmed.contains(':') {
        return Err(CoreError::InvalidInput(
            "AmigaDOS filenames cannot contain '/' or ':'".into(),
        ));
    }
    Ok(trimmed.to_string())
}

fn get_current_amiga_date() -> AmigaDate {
    let now_unix = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(super::bcpl::AMIGA_EPOCH_UNIX);

    let secs_since_amiga = (now_unix - super::bcpl::AMIGA_EPOCH_UNIX).max(0);
    let days = (secs_since_amiga / 86_400) as u32;
    let rem_secs = secs_since_amiga % 86_400;
    let mins = (rem_secs / 60) as u32;
    let ticks = ((rem_secs % 60) * 50) as u32;

    AmigaDate { days, mins, ticks }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::adf::create::create_blank_adf;
    use crate::core::adf::AdfImage;

    #[test]
    fn add_and_extract_file_ffs() {
        let mut img = create_blank_adf("TestFFS", FileSystemType::Ffs, false).unwrap();
        let payload = b"Hello Amiga World from ART!";
        let hdr_block =
            add_file(&mut img, 880, "Greeting.txt", payload, FileSystemType::Ffs).unwrap();
        assert_ne!(hdr_block, 0);

        let adf = AdfImage::from_bytes(img).unwrap();
        let entries = adf.list_root().unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "Greeting.txt");
        assert_eq!(entries[0].byte_size, payload.len() as u64);

        let extracted = adf.extract(hdr_block).unwrap();
        assert_eq!(extracted, payload);
    }

    #[test]
    fn add_and_extract_file_ofs() {
        let mut img = create_blank_adf("TestOFS", FileSystemType::Ofs, false).unwrap();
        let payload = vec![0x42u8; 1000]; // spans 3 OFS data blocks (488 + 488 + 24)
        let hdr_block = add_file(
            &mut img,
            880,
            "LargeData.bin",
            &payload,
            FileSystemType::Ofs,
        )
        .unwrap();

        let adf = AdfImage::from_bytes(img).unwrap();
        let extracted = adf.extract(hdr_block).unwrap();
        assert_eq!(extracted.len(), 1000);
        assert_eq!(extracted, payload);
    }

    #[test]
    fn create_directory_and_nest_file() {
        let mut img = create_blank_adf("DirDisk", FileSystemType::Ffs, false).unwrap();
        let dir_block = create_directory(&mut img, 880, "S").unwrap();
        assert_ne!(dir_block, 0);

        let startup_data = b"Echo \"Amiga Retro Toolkit\"";
        let startup_hdr = add_file(
            &mut img,
            dir_block,
            "Startup-Sequence",
            startup_data,
            FileSystemType::Ffs,
        )
        .unwrap();

        let adf = AdfImage::from_bytes(img).unwrap();
        let root_entries = adf.list_root().unwrap();
        assert_eq!(root_entries.len(), 1);
        assert_eq!(root_entries[0].name, "S");
        assert_eq!(
            root_entries[0].kind,
            super::super::blocks::EntryKind::Directory
        );

        let s_entries = adf.list_dir(dir_block).unwrap();
        assert_eq!(s_entries.len(), 1);
        assert_eq!(s_entries[0].name, "Startup-Sequence");

        let extracted = adf.extract(startup_hdr).unwrap();
        assert_eq!(extracted, startup_data);
    }

    #[test]
    fn delete_file_frees_space() {
        let mut img = create_blank_adf("DelDisk", FileSystemType::Ffs, false).unwrap();
        let payload = vec![0xAAu8; 2048]; // 4 data blocks + 1 header = 5 blocks
        let hdr = add_file(&mut img, 880, "Temp.bin", &payload, FileSystemType::Ffs).unwrap();

        let adf = AdfImage::from_bytes(img.clone()).unwrap();
        assert_eq!(adf.list_root().unwrap().len(), 1);

        delete_entry(&mut img, 880, hdr).unwrap();

        let adf_after = AdfImage::from_bytes(img).unwrap();
        assert_eq!(adf_after.list_root().unwrap().len(), 0);
        // Used space should return to standard blank 4 blocks (2048 bytes)
        assert_eq!(adf_after.info().unwrap().used_bytes, 4 * 512);
    }

    #[test]
    fn rename_file_preserves_content() {
        let mut img = create_blank_adf("RenDisk", FileSystemType::Ffs, false).unwrap();
        let payload = b"Original Content";
        let hdr = add_file(&mut img, 880, "OldName.txt", payload, FileSystemType::Ffs).unwrap();

        rename_entry(&mut img, 880, hdr, "NewName.txt").unwrap();

        let adf = AdfImage::from_bytes(img).unwrap();
        let entries = adf.list_root().unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "NewName.txt");

        let extracted = adf.extract(hdr).unwrap();
        assert_eq!(extracted, payload);
    }

    // --- Robustness: bad input must produce errors, never panics ---

    #[test]
    fn out_of_range_directory_block_is_an_error() {
        let mut img = create_blank_adf("Guard", FileSystemType::Ffs, false).unwrap();

        // A block number far past the end of an 880 KB image.
        let err = add_file(&mut img, 99_999, "X.txt", b"x", FileSystemType::Ffs).unwrap_err();
        assert!(matches!(err, CoreError::Malformed { .. }));

        // And the extreme value that would overflow an offset calculation.
        assert!(create_directory(&mut img, u32::MAX, "Y").is_err());
    }

    #[test]
    fn a_file_cannot_be_used_as_a_directory() {
        let mut img = create_blank_adf("Guard2", FileSystemType::Ffs, false).unwrap();
        let file_hdr =
            add_file(&mut img, 880, "NotADir.txt", b"data", FileSystemType::Ffs).unwrap();

        // Pointing an insert at a *file* header used to corrupt its contents.
        let err = add_file(&mut img, file_hdr, "Inner.txt", b"x", FileSystemType::Ffs).unwrap_err();
        assert!(matches!(err, CoreError::InvalidInput(_)), "got {err:?}");
    }

    #[test]
    fn duplicate_names_are_rejected() {
        let mut img = create_blank_adf("Dupes", FileSystemType::Ffs, false).unwrap();
        add_file(&mut img, 880, "Same.txt", b"first", FileSystemType::Ffs).unwrap();

        let err = add_file(&mut img, 880, "Same.txt", b"second", FileSystemType::Ffs).unwrap_err();
        assert!(matches!(err, CoreError::InvalidInput(_)));

        // AmigaDOS compares names case-insensitively, so this collides too.
        let err = add_file(&mut img, 880, "SAME.TXT", b"third", FileSystemType::Ffs).unwrap_err();
        assert!(matches!(err, CoreError::InvalidInput(_)));

        // A directory may not shadow an existing file name either.
        assert!(create_directory(&mut img, 880, "Same.txt").is_err());
    }

    #[test]
    fn rename_onto_an_existing_name_is_rejected() {
        let mut img = create_blank_adf("RenDupe", FileSystemType::Ffs, false).unwrap();
        add_file(&mut img, 880, "Taken.txt", b"a", FileSystemType::Ffs).unwrap();
        let second = add_file(&mut img, 880, "Free.txt", b"b", FileSystemType::Ffs).unwrap();

        let err = rename_entry(&mut img, 880, second, "Taken.txt").unwrap_err();
        assert!(matches!(err, CoreError::InvalidInput(_)));

        // The failed rename left the directory exactly as it was.
        let adf = AdfImage::from_bytes(img).unwrap();
        let names: Vec<_> = adf
            .list_root()
            .unwrap()
            .into_iter()
            .map(|e| e.name)
            .collect();
        assert_eq!(names.len(), 2);
        assert!(names.contains(&"Taken.txt".to_string()));
        assert!(names.contains(&"Free.txt".to_string()));
    }

    #[test]
    fn rename_can_change_only_case() {
        let mut img = create_blank_adf("CaseDisk", FileSystemType::Ffs, false).unwrap();
        let hdr = add_file(&mut img, 880, "readme.txt", b"x", FileSystemType::Ffs).unwrap();

        rename_entry(&mut img, 880, hdr, "README.TXT").unwrap();

        let adf = AdfImage::from_bytes(img).unwrap();
        let entries = adf.list_root().unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "README.TXT");
    }

    #[test]
    fn rename_of_an_unlinked_entry_reports_an_error() {
        let mut img = create_blank_adf("Broken", FileSystemType::Ffs, false).unwrap();
        let hdr = add_file(&mut img, 880, "Ghost.txt", b"x", FileSystemType::Ffs).unwrap();

        // Simulate a corrupt image: clear every hash bucket so the entry is no
        // longer reachable from its parent. The rename must refuse rather than
        // linking the orphan in a second time.
        for bucket in 0..HASH_TABLE_SIZE {
            write_u32_at(&mut img, 880, HASH_TABLE_OFFSET + bucket * 4, 0).unwrap();
        }

        let err = rename_entry(&mut img, 880, hdr, "Renamed.txt").unwrap_err();
        assert!(matches!(err, CoreError::Malformed { .. }), "got {err:?}");
    }

    #[test]
    fn deleting_a_non_empty_directory_is_refused() {
        let mut img = create_blank_adf("NonEmpty", FileSystemType::Ffs, false).unwrap();
        let dir = create_directory(&mut img, 880, "Tools").unwrap();
        add_file(&mut img, dir, "Inside.txt", b"x", FileSystemType::Ffs).unwrap();

        let err = delete_entry(&mut img, 880, dir).unwrap_err();
        assert!(matches!(err, CoreError::InvalidInput(_)));
    }
}
