//! Writing a file's blocks: data, extension chain, header.
//!
//! ## OFS and FFS are not a flag on the same layout
//!
//! FFS puts the payload in the whole block. OFS gives every data block a
//! 24-byte header — block type, the file header it belongs to, its sequence
//! number, how many bytes are used, the next data block, and a checksum — and
//! 488 bytes of payload. A file written as if it were FFS onto an OFS volume
//! is not slightly wrong; it is unreadable, and 24 bytes of every block are
//! interpreted as data.
//!
//! ## The pointer list runs backwards
//!
//! A header holds up to 72 data-block pointers (at a 512-byte block), stored
//! from longword 77 **downwards**. Past 72, the file grows an extension chain,
//! each block holding another 72. Writing the list forwards produces a file
//! ART reads back perfectly and AmigaDOS reads in the wrong order — the exact
//! shape of ART-032 … ART-035, which is why the oracle check exists.

use crate::core::error::{CoreError, CoreResult};
use crate::core::volume::{BlockDevice, VolumeGeometry};

use super::dir::{block_type, subtype};
use super::layout::{
    amiga_now, payload_per_block, pointer_slot, pointers_per_block, set_date, set_i32, set_u32,
    BlockSet, BYTE_SIZE_OFFSET, CHECKSUM_OFFSET, DATA_SIZE_OFFSET, DEFAULT_PROTECTION,
    EXTENSION_OFFSET, FIRST_DATA_OFFSET, HEADER_KEY_OFFSET, HIGH_SEQ_OFFSET, OFS_DATA_HEADER,
    PARENT_OFFSET, PROTECT_OFFSET, SUBTYPE_OFFSET, TYPE_OFFSET,
};

/// The largest file ART will write into a volume in one operation.
///
/// A 2 GiB volume ceiling makes anything past this impossible anyway; the
/// limit is here so a caller passing a nonsense length is refused before any
/// allocation arithmetic runs on it.
pub const MAX_FILE_BYTES: u64 = 2 * 1024 * 1024 * 1024;

/// How many blocks of each kind a file of `bytes` needs.
///
/// Computed before anything is allocated, so a copy that will not fit is
/// refused with real numbers instead of failing part-way (§3.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FileBudget {
    pub data_blocks: usize,
    pub extension_blocks: usize,
    /// One, always: the file header.
    pub header_blocks: usize,
}

impl FileBudget {
    pub fn total(&self) -> usize {
        self.data_blocks + self.extension_blocks + self.header_blocks
    }
}

/// Work out what a file of `bytes` will cost on this volume.
pub fn budget_for(bytes: u64, geometry: &VolumeGeometry) -> CoreResult<FileBudget> {
    if bytes > MAX_FILE_BYTES {
        return Err(CoreError::InvalidInput(format!(
            "{bytes} bytes is larger than any AmigaDOS volume ART writes"
        )));
    }

    let ofs = !geometry.dos_type.is_ffs();
    let payload = payload_per_block(geometry.block_size, ofs) as u64;
    let per_header = pointers_per_block(geometry.block_size);

    // A zero-byte file is real and has no data blocks at all — just a header.
    let data_blocks = bytes.div_ceil(payload) as usize;

    // The header holds the first `per_header` pointers; every extension block
    // holds another `per_header`.
    let extension_blocks = data_blocks
        .saturating_sub(per_header)
        .div_ceil(per_header.max(1));

    Ok(FileBudget {
        data_blocks,
        extension_blocks,
        header_blocks: 1,
    })
}

/// What a file's blocks were, once written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WrittenFile {
    pub header_block: u32,
    pub data_blocks: Vec<u32>,
    pub extension_blocks: Vec<u32>,
}

impl WrittenFile {
    /// Every block the file occupies, header included.
    pub fn all_blocks(&self) -> Vec<u32> {
        let mut all = vec![self.header_block];
        all.extend_from_slice(&self.data_blocks);
        all.extend_from_slice(&self.extension_blocks);
        all
    }
}

/// Lay a file's blocks out in the working set.
///
/// `blocks` must be exactly what [`budget_for`] asked for, in the order
/// header · data · extensions — the caller allocates, so that the whole
/// allocation succeeds or fails before the journal opens.
///
/// Nothing is linked into a directory here; that is [`super::dir::link_into`],
/// kept separate so a caller can build several files and link them all under
/// one journal.
#[allow(clippy::too_many_arguments)]
pub fn write_file_blocks(
    set: &mut BlockSet,
    geometry: &VolumeGeometry,
    blocks: &[u32],
    parent: u32,
    name: &str,
    data: &[u8],
    protection: u32,
    date: Option<crate::core::adf::bcpl::AmigaDate>,
) -> CoreResult<WrittenFile> {
    let checked = super::dir::check_name(name)?;
    let budget = budget_for(data.len() as u64, geometry)?;

    if blocks.len() != budget.total() {
        return Err(CoreError::InvalidInput(format!(
            "this file needs {} blocks and {} were allocated",
            budget.total(),
            blocks.len()
        )));
    }

    let header_block = blocks[0];
    let data_blocks = &blocks[1..1 + budget.data_blocks];
    let extension_blocks = &blocks[1 + budget.data_blocks..];

    let ofs = !geometry.dos_type.is_ffs();
    let payload = payload_per_block(geometry.block_size, ofs);
    let per_header = pointers_per_block(geometry.block_size);

    // ---- data blocks ----
    let mut written = 0usize;
    for (index, &block) in data_blocks.iter().enumerate() {
        let take = (data.len() - written).min(payload);
        let chunk = &data[written..written + take];

        let bytes = set.blank(block);
        if ofs {
            set_i32(bytes, TYPE_OFFSET, block_type::DATA)?;
            // Which file this block belongs to — an OFS volume can be walked
            // from any data block back to its header.
            set_u32(bytes, HEADER_KEY_OFFSET, header_block)?;
            // Sequence numbers are 1-based.
            set_u32(bytes, HIGH_SEQ_OFFSET, (index + 1) as u32)?;
            set_u32(bytes, DATA_SIZE_OFFSET, take as u32)?;
            let next = data_blocks.get(index + 1).copied().unwrap_or(0);
            set_u32(bytes, FIRST_DATA_OFFSET, next)?;
            bytes[OFS_DATA_HEADER..OFS_DATA_HEADER + take].copy_from_slice(chunk);
        } else {
            bytes[..take].copy_from_slice(chunk);
        }

        if ofs {
            set.checksum(block, CHECKSUM_OFFSET)?;
        }
        written += take;
    }

    // ---- extension blocks ----
    let inline = data_blocks.len().min(per_header);
    let mut remaining = &data_blocks[inline..];

    for (index, &block) in extension_blocks.iter().enumerate() {
        let here = remaining.len().min(per_header);
        let next = extension_blocks.get(index + 1).copied().unwrap_or(0);

        let bytes = set.blank(block);
        set_i32(bytes, TYPE_OFFSET, block_type::LIST)?;
        set_u32(bytes, HEADER_KEY_OFFSET, block)?;
        set_u32(bytes, HIGH_SEQ_OFFSET, here as u32)?;
        for (slot, &data_block) in remaining.iter().take(here).enumerate() {
            set_u32(bytes, pointer_slot(slot, geometry.block_size)?, data_block)?;
        }
        set_u32(bytes, PARENT_OFFSET, header_block)?;
        set_u32(bytes, EXTENSION_OFFSET, next)?;
        set_i32(bytes, SUBTYPE_OFFSET, subtype::FILE)?;
        set.checksum(block, CHECKSUM_OFFSET)?;

        remaining = &remaining[here..];
    }

    // ---- header ----
    {
        let bytes = set.blank(header_block);
        set_i32(bytes, TYPE_OFFSET, block_type::HEADER)?;
        set_u32(bytes, HEADER_KEY_OFFSET, header_block)?;
        set_u32(bytes, HIGH_SEQ_OFFSET, inline as u32)?;
        set_u32(
            bytes,
            FIRST_DATA_OFFSET,
            data_blocks.first().copied().unwrap_or(0),
        )?;
        for (slot, &data_block) in data_blocks.iter().take(inline).enumerate() {
            set_u32(bytes, pointer_slot(slot, geometry.block_size)?, data_block)?;
        }
        set_u32(bytes, PROTECT_OFFSET, protection)?;
        set_u32(bytes, BYTE_SIZE_OFFSET, data.len() as u32)?;
        set_date(bytes, date.unwrap_or_else(amiga_now))?;
        set_u32(bytes, PARENT_OFFSET, parent)?;
        set_u32(
            bytes,
            EXTENSION_OFFSET,
            extension_blocks.first().copied().unwrap_or(0),
        )?;
        set_i32(bytes, SUBTYPE_OFFSET, subtype::FILE)?;
    }
    // The name goes in through the directory module, which owns the Latin-1
    // encoding, and the checksum follows it.
    super::dir::write_header_name(set, header_block, &checked)?;

    Ok(WrittenFile {
        header_block,
        data_blocks: data_blocks.to_vec(),
        extension_blocks: extension_blocks.to_vec(),
    })
}

/// Every block a file already on the volume occupies.
///
/// Needed before deleting or overwriting one: the blocks have to go back to
/// the bitmap, and the journal has to know about every one of them.
///
/// Walks are bounded. A corrupt header can claim more data blocks than the
/// volume holds, or point its extension chain at itself.
pub fn blocks_of<D: BlockDevice + ?Sized>(
    device: &D,
    set: &BlockSet,
    geometry: &VolumeGeometry,
    header_block: u32,
) -> CoreResult<WrittenFile> {
    use super::layout::get_u32;

    let per_header = pointers_per_block(geometry.block_size);
    let mut data_blocks = Vec::new();
    let mut extension_blocks = Vec::new();

    let header = set.view(device, header_block)?;
    collect_pointers(&header, geometry, &mut data_blocks)?;

    let mut next = get_u32(&header, EXTENSION_OFFSET)?;
    let mut hops = 0usize;
    // One extension block per `per_header` data blocks; a volume cannot hold
    // more than one extension block per block it has.
    let max_hops = (geometry.total_blocks as usize / per_header.max(1)) + 2;

    while next != 0 {
        hops += 1;
        if hops > max_hops {
            return Err(CoreError::Malformed {
                format: "volume".into(),
                detail: format!("the extension chain of block {header_block} does not end"),
            });
        }
        if next >= geometry.total_blocks {
            return Err(CoreError::Malformed {
                format: "volume".into(),
                detail: format!("extension block {next} is outside the volume"),
            });
        }

        extension_blocks.push(next);
        let extension = set.view(device, next)?;
        collect_pointers(&extension, geometry, &mut data_blocks)?;
        next = get_u32(&extension, EXTENSION_OFFSET)?;
    }

    Ok(WrittenFile {
        header_block,
        data_blocks,
        extension_blocks,
    })
}

/// Read the data-block pointers out of a header or extension block.
fn collect_pointers(block: &[u8], geometry: &VolumeGeometry, out: &mut Vec<u32>) -> CoreResult<()> {
    use super::layout::get_u32;

    let per_header = pointers_per_block(geometry.block_size);
    let claimed = get_u32(block, HIGH_SEQ_OFFSET)? as usize;

    // `high_seq` comes from the image and is not trusted: a corrupt value
    // would read pointers from outside the table.
    let count = claimed.min(per_header);

    for slot in 0..count {
        let pointer = get_u32(block, pointer_slot(slot, geometry.block_size)?)?;
        if pointer == 0 {
            continue;
        }
        if pointer >= geometry.total_blocks {
            return Err(CoreError::Malformed {
                format: "volume".into(),
                detail: format!("data block {pointer} is outside the volume"),
            });
        }
        out.push(pointer);
    }
    Ok(())
}

/// Read a file's contents back out of a volume.
///
/// Used to verify a copy against its source (§5), so it deliberately does not
/// share code with the writer: a reader built from the writer's own idea of
/// the layout would agree with it however wrong both were.
pub fn read_file<D: BlockDevice + ?Sized>(
    device: &D,
    set: &BlockSet,
    geometry: &VolumeGeometry,
    header_block: u32,
) -> CoreResult<Vec<u8>> {
    use super::layout::get_u32;

    let header = set.view(device, header_block)?;
    let size = get_u32(&header, BYTE_SIZE_OFFSET)? as usize;

    let file = blocks_of(device, set, geometry, header_block)?;
    let ofs = !geometry.dos_type.is_ffs();
    let payload = payload_per_block(geometry.block_size, ofs);

    let mut out = Vec::with_capacity(size);
    for &block in &file.data_blocks {
        if out.len() >= size {
            break;
        }
        let bytes = set.view(device, block)?;
        let take = if ofs {
            // An OFS data block says how much of it is used; a corrupt value
            // must not read past the block.
            (get_u32(&bytes, DATA_SIZE_OFFSET)? as usize).min(payload)
        } else {
            payload
        }
        .min(size - out.len());

        let start = if ofs { OFS_DATA_HEADER } else { 0 };
        out.extend_from_slice(&bytes[start..start + take]);
    }

    if out.len() != size {
        return Err(CoreError::Malformed {
            format: "volume".into(),
            detail: format!(
                "this file says it is {size} bytes but its blocks hold {}",
                out.len()
            ),
        });
    }
    Ok(out)
}

/// The default protection bits for a newly written file (§4.1).
pub fn default_protection() -> u32 {
    DEFAULT_PROTECTION
}

// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::volume::device::VecDevice;
    use crate::core::volume::fixture::ffs_volume;
    use crate::core::volume::DosType;

    fn volume(dos: [u8; 4]) -> (VecDevice, VolumeGeometry) {
        let (bytes, geometry) = ffs_volume(1760, DosType::new(dos));
        (VecDevice::new(bytes, 512).unwrap(), geometry)
    }

    fn ffs() -> (VecDevice, VolumeGeometry) {
        volume(*b"DOS\x01")
    }

    fn ofs() -> (VecDevice, VolumeGeometry) {
        volume(*b"DOS\x00")
    }

    /// The 488-vs-512 difference, which decides how many blocks a copy needs.
    #[test]
    fn ofs_needs_more_blocks_for_the_same_file_than_ffs() {
        let (_, ffs) = ffs();
        let (_, ofs) = ofs();

        // Exactly one FFS block; OFS needs two, because 24 bytes of each are
        // its own header.
        assert_eq!(budget_for(512, &ffs).unwrap().data_blocks, 1);
        assert_eq!(budget_for(512, &ofs).unwrap().data_blocks, 2);

        assert_eq!(budget_for(488, &ofs).unwrap().data_blocks, 1);
        assert_eq!(budget_for(489, &ofs).unwrap().data_blocks, 2);
    }

    #[test]
    fn an_empty_file_costs_one_block_and_no_data() {
        let (_, geometry) = ffs();
        let budget = budget_for(0, &geometry).unwrap();
        assert_eq!(budget.data_blocks, 0);
        assert_eq!(budget.extension_blocks, 0);
        assert_eq!(budget.total(), 1, "the header, and nothing else");
    }

    /// The header holds 72 pointers; the 73rd data block is what forces an
    /// extension block into existence.
    #[test]
    fn the_seventy_third_data_block_needs_an_extension_block() {
        let (_, geometry) = ffs();

        let exactly_full = budget_for(72 * 512, &geometry).unwrap();
        assert_eq!(exactly_full.data_blocks, 72);
        assert_eq!(exactly_full.extension_blocks, 0);

        let one_more = budget_for(72 * 512 + 1, &geometry).unwrap();
        assert_eq!(one_more.data_blocks, 73);
        assert_eq!(one_more.extension_blocks, 1);

        let two_chains = budget_for(145 * 512, &geometry).unwrap();
        assert_eq!(two_chains.data_blocks, 145);
        assert_eq!(two_chains.extension_blocks, 2);
    }

    /// The whole point: write it, read it back through a separate path, get
    /// the same bytes.
    #[test]
    fn an_ffs_file_reads_back_byte_for_byte() {
        let (device, geometry) = ffs();
        let mut set = BlockSet::new(512);

        let data: Vec<u8> = (0..2000u32).map(|i| (i % 251) as u8).collect();
        let budget = budget_for(data.len() as u64, &geometry).unwrap();
        let blocks: Vec<u32> = (900..900 + budget.total() as u32).collect();

        let written = write_file_blocks(
            &mut set,
            &geometry,
            &blocks,
            geometry.root_block,
            "Data.bin",
            &data,
            default_protection(),
            None,
        )
        .unwrap();

        assert_eq!(
            read_file(&device, &set, &geometry, written.header_block).unwrap(),
            data
        );
    }

    #[test]
    fn an_ofs_file_reads_back_byte_for_byte() {
        let (device, geometry) = ofs();
        let mut set = BlockSet::new(512);

        let data: Vec<u8> = (0..2000u32).map(|i| (i % 251) as u8).collect();
        let budget = budget_for(data.len() as u64, &geometry).unwrap();
        let blocks: Vec<u32> = (900..900 + budget.total() as u32).collect();

        let written = write_file_blocks(
            &mut set,
            &geometry,
            &blocks,
            geometry.root_block,
            "Data.bin",
            &data,
            default_protection(),
            None,
        )
        .unwrap();

        assert_eq!(
            read_file(&device, &set, &geometry, written.header_block).unwrap(),
            data
        );
    }

    /// OFS data blocks carry a header ART has to fill in: which file, which
    /// position, how much is used, what comes next. A missing sequence number
    /// makes AmigaDOS read the file in the wrong order.
    #[test]
    fn ofs_data_blocks_carry_their_own_bookkeeping() {
        use super::super::layout::get_u32;

        let (device, geometry) = ofs();
        let mut set = BlockSet::new(512);

        let data = vec![0xAAu8; 1000]; // three OFS blocks
        let budget = budget_for(data.len() as u64, &geometry).unwrap();
        assert_eq!(budget.data_blocks, 3);
        let blocks: Vec<u32> = (900..900 + budget.total() as u32).collect();

        let written = write_file_blocks(
            &mut set,
            &geometry,
            &blocks,
            geometry.root_block,
            "Ofs.bin",
            &data,
            default_protection(),
            None,
        )
        .unwrap();

        for (index, &block) in written.data_blocks.iter().enumerate() {
            let bytes = set.view(&device, block).unwrap();
            assert_eq!(get_u32(&bytes, TYPE_OFFSET).unwrap(), 8, "T_DATA");
            assert_eq!(
                get_u32(&bytes, HEADER_KEY_OFFSET).unwrap(),
                written.header_block,
                "every data block names its file"
            );
            assert_eq!(
                get_u32(&bytes, HIGH_SEQ_OFFSET).unwrap(),
                (index + 1) as u32,
                "sequence numbers are 1-based"
            );

            let expected_next = written.data_blocks.get(index + 1).copied().unwrap_or(0);
            assert_eq!(get_u32(&bytes, FIRST_DATA_OFFSET).unwrap(), expected_next);

            let used = get_u32(&bytes, DATA_SIZE_OFFSET).unwrap() as usize;
            assert!(used <= 488);
        }

        // The last block holds the remainder, not a full 488.
        let last = set
            .view(&device, *written.data_blocks.last().unwrap())
            .unwrap();
        assert_eq!(get_u32(&last, DATA_SIZE_OFFSET).unwrap(), 1000 - 2 * 488);
    }

    /// A file long enough to need an extension chain, read back through the
    /// chain walk rather than the writer's own list.
    #[test]
    fn a_file_across_an_extension_boundary_reads_back_whole() {
        let (device, geometry) = ffs();
        let mut set = BlockSet::new(512);

        // 100 blocks: 72 in the header, 28 in one extension block.
        let data: Vec<u8> = (0..100 * 512u32).map(|i| (i % 253) as u8).collect();
        let budget = budget_for(data.len() as u64, &geometry).unwrap();
        assert_eq!(budget.extension_blocks, 1);

        let blocks: Vec<u32> = (200..200 + budget.total() as u32).collect();
        let written = write_file_blocks(
            &mut set,
            &geometry,
            &blocks,
            geometry.root_block,
            "Big.bin",
            &data,
            default_protection(),
            None,
        )
        .unwrap();

        let found = blocks_of(&device, &set, &geometry, written.header_block).unwrap();
        assert_eq!(found.data_blocks, written.data_blocks);
        assert_eq!(found.extension_blocks, written.extension_blocks);
        assert_eq!(
            read_file(&device, &set, &geometry, written.header_block).unwrap(),
            data
        );
    }

    #[test]
    fn a_file_across_two_extension_blocks_reads_back_whole() {
        let (device, geometry) = ffs();
        let mut set = BlockSet::new(512);

        let data: Vec<u8> = (0..150 * 512u32).map(|i| (i % 199) as u8).collect();
        let budget = budget_for(data.len() as u64, &geometry).unwrap();
        assert_eq!(budget.extension_blocks, 2);

        let blocks: Vec<u32> = (200..200 + budget.total() as u32).collect();
        let written = write_file_blocks(
            &mut set,
            &geometry,
            &blocks,
            geometry.root_block,
            "Bigger.bin",
            &data,
            default_protection(),
            None,
        )
        .unwrap();

        assert_eq!(
            read_file(&device, &set, &geometry, written.header_block).unwrap(),
            data
        );
        assert_eq!(
            blocks_of(&device, &set, &geometry, written.header_block)
                .unwrap()
                .data_blocks
                .len(),
            150
        );
    }

    #[test]
    fn an_empty_file_writes_and_reads_as_empty() {
        let (device, geometry) = ffs();
        let mut set = BlockSet::new(512);

        let written = write_file_blocks(
            &mut set,
            &geometry,
            &[900],
            geometry.root_block,
            "Empty",
            &[],
            default_protection(),
            None,
        )
        .unwrap();

        assert!(written.data_blocks.is_empty());
        assert!(read_file(&device, &set, &geometry, written.header_block)
            .unwrap()
            .is_empty());
    }

    #[test]
    fn the_wrong_number_of_blocks_is_refused_before_anything_is_written() {
        let (_, geometry) = ffs();
        let mut set = BlockSet::new(512);

        let err = write_file_blocks(
            &mut set,
            &geometry,
            &[900, 901],
            geometry.root_block,
            "Empty",
            &[],
            default_protection(),
            None,
        )
        .unwrap_err();
        assert!(
            err.to_string().contains("blocks and 2 were allocated"),
            "{err}"
        );
        assert!(set.is_empty(), "nothing should have been written");
    }

    /// `high_seq` comes from the image. A corrupt one must not make the reader
    /// walk off the end of the pointer table.
    #[test]
    fn a_header_claiming_more_pointers_than_fit_is_clamped() {
        use super::super::layout::set_u32 as put;

        let (device, geometry) = ffs();
        let mut set = BlockSet::new(512);

        let data = vec![1u8; 1024];
        let budget = budget_for(data.len() as u64, &geometry).unwrap();
        let blocks: Vec<u32> = (900..900 + budget.total() as u32).collect();
        let written = write_file_blocks(
            &mut set,
            &geometry,
            &blocks,
            geometry.root_block,
            "Data.bin",
            &data,
            default_protection(),
            None,
        )
        .unwrap();

        let header = set.edit(&device, written.header_block).unwrap();
        put(header, HIGH_SEQ_OFFSET, 100_000).unwrap();

        // Clamped to the 72 the table actually holds, so this returns rather
        // than reading outside the block.
        let found = blocks_of(&device, &set, &geometry, written.header_block).unwrap();
        assert!(found.data_blocks.len() <= 72);
    }

    #[test]
    fn a_data_pointer_outside_the_volume_is_an_error() {
        use super::super::layout::set_u32 as put;

        let (device, geometry) = ffs();
        let mut set = BlockSet::new(512);

        let data = vec![1u8; 1024];
        let budget = budget_for(data.len() as u64, &geometry).unwrap();
        let blocks: Vec<u32> = (900..900 + budget.total() as u32).collect();
        let written = write_file_blocks(
            &mut set,
            &geometry,
            &blocks,
            geometry.root_block,
            "Data.bin",
            &data,
            default_protection(),
            None,
        )
        .unwrap();

        let header = set.edit(&device, written.header_block).unwrap();
        put(header, pointer_slot(0, 512).unwrap(), 99_999).unwrap();

        let err = blocks_of(&device, &set, &geometry, written.header_block).unwrap_err();
        assert!(err.to_string().contains("outside the volume"), "{err}");
    }

    #[test]
    fn an_extension_chain_that_loops_is_an_error_not_a_hang() {
        use super::super::layout::set_u32 as put;

        let (device, geometry) = ffs();
        let mut set = BlockSet::new(512);

        let data = vec![1u8; 100 * 512];
        let budget = budget_for(data.len() as u64, &geometry).unwrap();
        let blocks: Vec<u32> = (200..200 + budget.total() as u32).collect();
        let written = write_file_blocks(
            &mut set,
            &geometry,
            &blocks,
            geometry.root_block,
            "Loop.bin",
            &data,
            default_protection(),
            None,
        )
        .unwrap();

        // Point the extension block at itself.
        let extension = written.extension_blocks[0];
        let bytes = set.edit(&device, extension).unwrap();
        put(bytes, EXTENSION_OFFSET, extension).unwrap();

        let err = blocks_of(&device, &set, &geometry, written.header_block).unwrap_err();
        assert!(err.to_string().contains("does not end"), "{err}");
    }

    #[test]
    fn all_blocks_lists_the_header_the_data_and_the_extensions() {
        let (_, geometry) = ffs();
        let mut set = BlockSet::new(512);

        let data = vec![7u8; 100 * 512];
        let budget = budget_for(data.len() as u64, &geometry).unwrap();
        let blocks: Vec<u32> = (200..200 + budget.total() as u32).collect();
        let written = write_file_blocks(
            &mut set,
            &geometry,
            &blocks,
            geometry.root_block,
            "Big.bin",
            &data,
            default_protection(),
            None,
        )
        .unwrap();

        let mut all = written.all_blocks();
        all.sort_unstable();
        let mut expected = blocks.clone();
        expected.sort_unstable();
        assert_eq!(all, expected, "every allocated block must be accounted for");
    }
}
