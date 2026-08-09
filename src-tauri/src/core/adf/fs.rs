//! Filesystem walker — reads the directory tree from an ADF image.
//!
//! Uses the root block's hash table plus per-entry sibling chains (offset 496)
//! to enumerate every entry. Returns a flat list of file/directory entries for
//! a given directory block (default: the root).

use serde::{Deserialize, Serialize};

use super::blocks::{EntryKind, HeaderBlock};
use crate::core::error::{CoreError, CoreResult};
use crate::core::volume::device::SliceDevice;
use crate::core::volume::{read_block_vec, BlockDevice};

/// A single filesystem entry (file or directory).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileEntry {
    pub name: String,
    pub kind: EntryKind,
    /// Byte size (0 for directories).
    pub byte_size: u64,
    pub comment: String,
    /// Unix timestamp (seconds since 1970), derived from the Amiga date.
    pub unix_date: i64,
    /// Block number of this entry's header.
    pub header_block: u32,
    /// Block number of the parent directory (root = the root block).
    pub parent: u32,
}

/// Statistics from walking a disk filesystem tree.
#[derive(Debug, Default, Clone)]
pub struct FsStats {
    pub file_count: usize,
    pub directory_count: usize,
}

/// Maximum depth of sibling-chain traversal (defends against malformed loops).
const MAX_SIBLING_VISITS: usize = 4096;

// ---------------------------------------------------------------------------
// Volume-based walking — the real implementation
// ---------------------------------------------------------------------------
//
// These take a `BlockDevice` and so work on any volume: a floppy image in
// memory, or a partition two gigabytes into an HDF read on demand. Nothing
// below this line knows how big the disk is or where it came from.
//
// The `&[u8]` functions further down are the floppy special case, kept so the
// existing ADF callers and their tests are untouched.

/// Walk a directory's contents on any volume.
pub fn list_directory_on(device: &dyn BlockDevice, dir_block: u32) -> CoreResult<Vec<FileEntry>> {
    // Reading the header first is what validates `dir_block`: a number straight
    // from the frontend, or from a corrupt image, must not reach the hash-table
    // read as an offset.
    let _dir_hdr = read_header_on(device, dir_block)?;
    let hash_table = read_directory_hash_table_on(device, dir_block)?;

    let mut entries = Vec::new();
    for &bucket_head in hash_table.iter() {
        if bucket_head == 0 {
            continue;
        }
        // Walk the sibling chain for this bucket.
        let mut current = bucket_head;
        let mut visits = 0usize;
        while current != 0 {
            visits += 1;
            if visits > MAX_SIBLING_VISITS {
                return Err(CoreError::Malformed {
                    format: "adf".into(),
                    detail: "sibling chain too long (possible loop)".into(),
                });
            }
            let hdr = read_header_on(device, current)?;
            let name = hdr.name.clone();
            entries.push(FileEntry {
                name,
                kind: hdr.kind,
                byte_size: hdr.byte_size,
                comment: hdr.comment.clone(),
                unix_date: hdr.date.to_unix(),
                header_block: hdr.header_key,
                parent: hdr.parent,
            });
            current = hdr.next_hash;
        }
    }

    // Amiga directories are not sorted; present a stable order to the UI.
    entries.sort_by_key(|a| a.name.to_lowercase());
    Ok(entries)
}

/// Walk the whole tree on any volume, counting what is there.
pub fn walk_and_count_on(device: &dyn BlockDevice, root_block: u32) -> CoreResult<FsStats> {
    let mut stats = FsStats::default();
    let mut stack = vec![root_block];
    let mut visited = std::collections::HashSet::new();

    while let Some(dir_block) = stack.pop() {
        if !visited.insert(dir_block) {
            continue;
        }
        let entries = list_directory_on(device, dir_block)?;
        for entry in entries {
            match entry.kind {
                EntryKind::Directory => {
                    stats.directory_count += 1;
                    stack.push(entry.header_block);
                }
                EntryKind::File => {
                    stats.file_count += 1;
                }
            }
        }
    }

    Ok(stats)
}

/// Read and parse a header block from any volume.
pub fn read_header_on(device: &dyn BlockDevice, block: u32) -> CoreResult<HeaderBlock> {
    let bytes = read_block_vec(device, block)?;
    HeaderBlock::parse(&bytes)
}

/// Read a directory's hash table.
///
/// Every offset here is bounds-checked by the device, which is the point of the
/// abstraction: the previous version indexed the image directly and would have
/// panicked — fatally, under `panic = "abort"` — on a block number that got
/// past its caller.
fn read_directory_hash_table_on(
    device: &dyn BlockDevice,
    dir_block: u32,
) -> CoreResult<[u32; super::blocks::HASH_TABLE_SIZE]> {
    let block = read_block_vec(device, dir_block)?;

    let mut table = [0u32; super::blocks::HASH_TABLE_SIZE];
    for (i, slot) in table.iter_mut().enumerate() {
        let o = super::blocks::HASH_TABLE_OFFSET + i * 4;
        let word = block.get(o..o + 4).ok_or_else(|| CoreError::Malformed {
            format: "adf".into(),
            detail: format!("hash table runs past the end of block {dir_block}"),
        })?;
        *slot = u32::from_be_bytes([word[0], word[1], word[2], word[3]]);
    }
    Ok(table)
}

// ---------------------------------------------------------------------------
// The floppy special case
// ---------------------------------------------------------------------------

/// Walk a directory's contents given the directory's block number.
///
/// Pass the root block number (usually 880) to list the top level.
/// `image` is the full ADF byte slice; blocks are `BLOCK_SIZE` bytes each.
pub fn list_directory(image: &[u8], dir_block: u32) -> CoreResult<Vec<FileEntry>> {
    list_directory_on(&SliceDevice::floppy(image), dir_block)
}

/// Walk the full directory tree from root and count files and directories.
pub fn walk_and_count(image: &[u8], root_block: u32) -> CoreResult<FsStats> {
    walk_and_count_on(&SliceDevice::floppy(image), root_block)
}

/// Read a header block at the given block number.
pub fn read_header(image: &[u8], block: u32) -> CoreResult<HeaderBlock> {
    read_header_on(&SliceDevice::floppy(image), block)
}

/// Convenience: list the root directory.
pub fn list_root(image: &[u8], root_block: u32) -> CoreResult<Vec<FileEntry>> {
    list_directory(image, root_block)
}
