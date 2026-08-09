//! Extract a file's bytes from an ADF image.
//!
//! Handles both filesystem types:
//! - **OFS**: each data block has a 24-byte header; usable payload is 488 bytes.
//! - **FFS**: data blocks are raw 512-byte payloads.
//!
//! A file's block list lives inline in the header block (up to 72 entries)
//! and may chain through T_LIST extension blocks for larger files.

use super::blocks::{
    block_type, HeaderBlock, BLOCK_SIZE, MAX_INLINE_DATA_BLOCKS, OFS_DATA_BYTES,
    OFS_DATA_HEADER_SIZE,
};
use crate::core::error::{CoreError, CoreResult};
use crate::core::volume::device::SliceDevice;
use crate::core::volume::{read_block_vec, BlockDevice};

use super::bootblock::FileSystemType;

/// How long an extension chain may run before ART calls it a loop.
const MAX_EXTENSION_BLOCKS: usize = 4096;

/// Read a file's bytes from any volume.
///
/// OFS and FFS differ only in the **data block** layout — OFS gives each one a
/// 24-byte header — so this is the one place the filesystem flavour matters
/// while browsing.
pub fn extract_file_on(
    device: &dyn BlockDevice,
    header: &HeaderBlock,
    fs_type: FileSystemType,
) -> CoreResult<Vec<u8>> {
    if header.kind != super::blocks::EntryKind::File {
        return Err(CoreError::InvalidInput(format!(
            "'{}' is not a file",
            header.name
        )));
    }

    // The declared size is a claim from the image; it caps the allocation only
    // after being clamped, so a corrupt header cannot ask for a gigabyte.
    let mut out = Vec::with_capacity(header.byte_size.min(1024 * 1024) as usize);
    let mut remaining = header.byte_size as usize;

    // Collect the full ordered list of data-block pointers (inline + extensions).
    let block_ptrs = collect_data_blocks_on(device, header)?;

    for ptr in block_ptrs {
        if remaining == 0 {
            break;
        }
        let block = read_block_vec(device, ptr)?;

        let payload: &[u8] = if fs_type.has_data_header() {
            // OFS: validate type, then slice bytes 24..
            let typ = i32::from_be_bytes([block[0], block[1], block[2], block[3]]);
            if typ != block_type::DATA {
                return Err(CoreError::Malformed {
                    format: "adf".into(),
                    detail: format!("expected OFS data block, got type {typ} at block {ptr}"),
                });
            }
            let data_size =
                u32::from_be_bytes([block[12], block[13], block[14], block[15]]) as usize;
            let data_size = data_size.min(OFS_DATA_BYTES).min(remaining);
            &block[OFS_DATA_HEADER_SIZE..OFS_DATA_HEADER_SIZE + data_size]
        } else {
            // FFS: raw block.
            let take = remaining.min(BLOCK_SIZE);
            &block[..take]
        };

        out.extend_from_slice(payload);
        remaining = remaining.saturating_sub(payload.len());
    }

    // Truncate / pad to the declared byte size.
    out.truncate(header.byte_size as usize);
    Ok(out)
}

/// Read the full byte content of a file given its header block.
///
/// `image` is the full ADF slice; `fs_type` selects OFS vs FFS decoding.
pub fn extract_file(
    image: &[u8],
    header: &HeaderBlock,
    fs_type: FileSystemType,
) -> CoreResult<Vec<u8>> {
    extract_file_on(&SliceDevice::floppy(image), header, fs_type)
}

/// Collect every data-block pointer for a file, following extension chains.
fn collect_data_blocks_on(device: &dyn BlockDevice, header: &HeaderBlock) -> CoreResult<Vec<u32>> {
    // Not `with_capacity(block_count)`: the count comes from the image, and
    // reserving from an unchecked number is how a malformed header turns into
    // an allocation the size of the address space.
    let mut all = Vec::new();
    // Inline list is already in sequential order (parsed from high to low indices).
    all.extend_from_slice(&header.data_blocks);

    let mut ext = header.extension;
    let mut guard = 0usize;
    while ext != 0 {
        guard += 1;
        if guard > MAX_EXTENSION_BLOCKS {
            return Err(CoreError::Malformed {
                format: "adf".into(),
                detail: "extension chain too long (possible loop)".into(),
            });
        }
        let block = read_block_vec(device, ext)?;

        // T_LIST blocks store data-block pointers starting at LW 77 (offset 308) down to LW 6 (offset 24).
        for i in 0..MAX_INLINE_DATA_BLOCKS {
            let o = (77 - i) * 4;
            let ptr = u32::from_be_bytes([block[o], block[o + 1], block[o + 2], block[o + 3]]);
            if ptr == 0 {
                break;
            }
            all.push(ptr);
        }
        // Next extension block is at offset 504 (LW 126).
        ext = u32::from_be_bytes([block[504], block[505], block[506], block[507]]);
    }
    Ok(all)
}
