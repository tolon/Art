//! Block field offsets, and the working set an operation builds before any of
//! it reaches the disk.
//!
//! The offsets are the same ones `core/adf/blocks.rs` reads with, restated here
//! for writing so the two can be compared side by side. Four of them were
//! wrong for months (ART-032 … ART-035) precisely because reading and writing
//! agreed with each other and with nothing else, so every one carries the
//! longword index a reader can check against the lclevy ADF FAQ.

use std::collections::BTreeMap;

use crate::core::adf::bcpl::AmigaDate;
use crate::core::adf::checksum::block_checksum;
use crate::core::error::{CoreError, CoreResult};
use crate::core::volume::{read_block_vec, BlockDevice};

// ---- shared header fields (file header, directory header, root) ----

/// LW 0. `T_HEADER` (2), `T_DATA` (8) or `T_LIST` (16).
pub const TYPE_OFFSET: usize = 0;
/// LW 1. The block's own number, for a header.
pub const HEADER_KEY_OFFSET: usize = 4;
/// LW 2. Data-block pointers held in *this* block.
pub const HIGH_SEQ_OFFSET: usize = 8;
/// LW 3. Bytes used, for an OFS data block. Zero in a header.
pub const DATA_SIZE_OFFSET: usize = 12;
/// LW 4. First data block of a file; `next_data` in an OFS data block.
pub const FIRST_DATA_OFFSET: usize = 16;
/// LW 5. The checksum every block but a bitmap block carries here.
pub const CHECKSUM_OFFSET: usize = 20;
/// LW 6. Start of the hash table (directories) or data-block list (files).
pub const TABLE_OFFSET: usize = 24;
/// LW 80. Protection bits, `HSPARWED`.
pub const PROTECT_OFFSET: usize = 320;
/// LW 81. The file's size in bytes. **Not 316** — see ART-034.
pub const BYTE_SIZE_OFFSET: usize = 324;
/// LW 82. Comment, as a BSTR in an 80-byte field.
pub const COMMENT_OFFSET: usize = 328;
pub const COMMENT_FIELD_LEN: usize = 80;
/// LW 105. Days since 1978-01-01.
pub const DAYS_OFFSET: usize = 420;
/// LW 106. Minutes past midnight.
pub const MINS_OFFSET: usize = 424;
/// LW 107. Ticks past the minute.
pub const TICKS_OFFSET: usize = 428;
/// LW 108. Name, as a BSTR in a 32-byte field.
pub const NAME_OFFSET: usize = 432;
pub const NAME_FIELD_LEN: usize = 32;
/// LW 124. Next entry in the same hash bucket.
pub const NEXT_HASH_OFFSET: usize = 496;
/// LW 125. The directory this entry lives in.
pub const PARENT_OFFSET: usize = 500;
/// LW 126. First file-extension block, or the root's `dircache`.
pub const EXTENSION_OFFSET: usize = 504;
/// LW 127. `ST_ROOT` (1), `ST_USERDIR` (2) or `ST_FILE` (-3).
pub const SUBTYPE_OFFSET: usize = 508;

/// LW 77. Data-block pointers are stored **backwards** from here.
pub const FIRST_TABLE_SLOT_OFFSET: usize = 308;

/// The `HSPARWED` bits AmigaDOS gives a file created by a copy.
///
/// The low four bits are *inverted*: a zero means the permission is granted.
/// `0` therefore reads as `----RWED`, which is what AmigaDOS itself writes and
/// what the brief calls for (§4.1).
pub const DEFAULT_PROTECTION: u32 = 0;

/// The bit that says "this file has not been archived since it changed".
///
/// Not inverted, unlike RWED. Cleared on create, per §4.1.
pub const PROTECT_ARCHIVE: u32 = 1 << 4;

/// Longwords of hash table in a 512-byte directory block.
///
/// `(block_size / 4) - 56`. ART only supports 512-byte blocks, so this is 72,
/// but it is derived rather than pinned so the constant does not become a lie
/// if larger blocks are ever added.
pub fn hash_table_size(block_size: usize) -> usize {
    (block_size / 4).saturating_sub(56)
}

/// Data-block pointers one file header or extension block can hold.
pub fn pointers_per_block(block_size: usize) -> usize {
    hash_table_size(block_size)
}

/// Bytes of payload in one data block.
pub fn payload_per_block(block_size: usize, ofs: bool) -> usize {
    if ofs {
        block_size - OFS_DATA_HEADER
    } else {
        block_size
    }
}

/// The header OFS puts on every data block.
pub const OFS_DATA_HEADER: usize = 24;

/// Now, as AmigaDOS counts it.
///
/// The Amiga epoch is 1978-01-01. A clock set before that clamps to the epoch
/// rather than wrapping into a negative day count — a file dated 1969 would
/// display as far in the future on a real Amiga.
pub fn amiga_now() -> AmigaDate {
    let unix = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(crate::core::adf::bcpl::AMIGA_EPOCH_UNIX);
    amiga_from_unix(unix)
}

/// Convert a Unix timestamp to the Amiga triplet, clamping anything before
/// 1978 to the epoch (§4.1).
pub fn amiga_from_unix(unix: i64) -> AmigaDate {
    let since = (unix - crate::core::adf::bcpl::AMIGA_EPOCH_UNIX).max(0);
    AmigaDate {
        days: (since / 86_400) as u32,
        mins: ((since % 86_400) / 60) as u32,
        ticks: (((since % 86_400) % 60) * 50) as u32,
    }
}

/// The blocks one operation is assembling, none of them written yet.
///
/// Everything an operation changes goes in here first. That is what lets the
/// journal be told the complete block set before a single byte reaches the
/// image — an allocator handing out blocks lazily part-way through a write
/// could not provide it, and a journal that learns about a block after it has
/// been overwritten is no journal at all.
#[derive(Debug)]
pub struct BlockSet {
    block_size: usize,
    blocks: BTreeMap<u32, Vec<u8>>,
}

impl BlockSet {
    pub fn new(block_size: usize) -> Self {
        Self {
            block_size,
            blocks: BTreeMap::new(),
        }
    }

    /// A block that starts empty — a freshly allocated one.
    pub fn blank(&mut self, block: u32) -> &mut Vec<u8> {
        self.blocks.entry(block).or_insert_with(|| vec![0u8; 0]);
        let slot = self.blocks.get_mut(&block).expect("just inserted");
        slot.clear();
        slot.resize(self.block_size, 0);
        slot
    }

    /// A block loaded from the volume, so untouched fields survive.
    pub fn edit<D: BlockDevice + ?Sized>(
        &mut self,
        device: &D,
        block: u32,
    ) -> CoreResult<&mut Vec<u8>> {
        if let std::collections::btree_map::Entry::Vacant(slot) = self.blocks.entry(block) {
            slot.insert(read_block_vec(device, block)?);
        }
        Ok(self
            .blocks
            .get_mut(&block)
            .expect("present after the entry above"))
    }

    /// Read a block, from the working set if it is there and the volume if not.
    pub fn view<D: BlockDevice + ?Sized>(&self, device: &D, block: u32) -> CoreResult<Vec<u8>> {
        match self.blocks.get(&block) {
            Some(bytes) => Ok(bytes.clone()),
            None => read_block_vec(device, block),
        }
    }

    /// Put an already-built block in, replacing whatever was there.
    pub fn put(&mut self, block: u32, bytes: Vec<u8>) -> CoreResult<()> {
        if bytes.len() != self.block_size {
            return Err(CoreError::InvalidInput(format!(
                "a block must be {} bytes, got {}",
                self.block_size,
                bytes.len()
            )));
        }
        self.blocks.insert(block, bytes);
        Ok(())
    }

    pub fn contains(&self, block: u32) -> bool {
        self.blocks.contains_key(&block)
    }

    /// Every block this operation will write, in order.
    pub fn touched(&self) -> Vec<u32> {
        self.blocks.keys().copied().collect()
    }

    pub fn iter(&self) -> impl Iterator<Item = (&u32, &Vec<u8>)> {
        self.blocks.iter()
    }

    pub fn iter_mut(&mut self) -> impl Iterator<Item = (&u32, &mut Vec<u8>)> {
        self.blocks.iter_mut()
    }

    pub fn len(&self) -> usize {
        self.blocks.len()
    }

    pub fn is_empty(&self) -> bool {
        self.blocks.is_empty()
    }

    /// Recompute the checksum of a block in the set.
    ///
    /// Always the last thing done to a block. A checksum computed before a
    /// later field is written is the ART-033 failure again: an image ART reads
    /// happily and AmigaDOS rejects.
    pub fn checksum(&mut self, block: u32, offset: usize) -> CoreResult<()> {
        let Some(bytes) = self.blocks.get_mut(&block) else {
            return Err(CoreError::InvalidInput(format!(
                "block {block} is not in this operation"
            )));
        };
        if offset + 4 > bytes.len() {
            return Err(CoreError::InvalidInput(format!(
                "checksum offset {offset} is outside a {}-byte block",
                bytes.len()
            )));
        }
        bytes[offset..offset + 4].fill(0);
        let sum = block_checksum(bytes, offset);
        bytes[offset..offset + 4].copy_from_slice(&sum.to_be_bytes());
        Ok(())
    }
}

/// Read a big-endian longword, refusing an offset outside the block.
pub fn get_u32(block: &[u8], offset: usize) -> CoreResult<u32> {
    let end = offset.checked_add(4).ok_or_else(overflow)?;
    if end > block.len() {
        return Err(CoreError::Malformed {
            format: "volume".into(),
            detail: format!(
                "offset {offset} is past the end of a {}-byte block",
                block.len()
            ),
        });
    }
    Ok(u32::from_be_bytes(
        block[offset..end].try_into().expect("checked"),
    ))
}

/// Write a big-endian longword, refusing an offset outside the block.
pub fn set_u32(block: &mut [u8], offset: usize, value: u32) -> CoreResult<()> {
    let end = offset.checked_add(4).ok_or_else(overflow)?;
    if end > block.len() {
        return Err(CoreError::Malformed {
            format: "volume".into(),
            detail: format!(
                "offset {offset} is past the end of a {}-byte block",
                block.len()
            ),
        });
    }
    block[offset..end].copy_from_slice(&value.to_be_bytes());
    Ok(())
}

pub fn set_i32(block: &mut [u8], offset: usize, value: i32) -> CoreResult<()> {
    set_u32(block, offset, value as u32)
}

pub fn get_i32(block: &[u8], offset: usize) -> CoreResult<i32> {
    Ok(get_u32(block, offset)? as i32)
}

/// Write the date triplet into a header block.
pub fn set_date(block: &mut [u8], date: AmigaDate) -> CoreResult<()> {
    set_u32(block, DAYS_OFFSET, date.days)?;
    set_u32(block, MINS_OFFSET, date.mins)?;
    set_u32(block, TICKS_OFFSET, date.ticks)
}

/// Where a data-block pointer lives, counting backwards from longword 77.
///
/// AmigaDOS stores the list in reverse: index 0 at LW 77, index 1 at LW 76 and
/// so on. Writing it forwards produces a file that reads back with its blocks
/// in the wrong order — and ART's own reader would agree with it, which is
/// exactly the class of bug the amitools oracle exists to catch.
pub fn pointer_slot(index: usize, block_size: usize) -> CoreResult<usize> {
    let capacity = pointers_per_block(block_size);
    if index >= capacity {
        return Err(CoreError::InvalidInput(format!(
            "pointer {index} does not fit in a block that holds {capacity}"
        )));
    }
    Ok(FIRST_TABLE_SLOT_OFFSET - index * 4)
}

fn overflow() -> CoreError {
    CoreError::Malformed {
        format: "volume".into(),
        detail: "a block offset overflowed".into(),
    }
}

// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::volume::device::VecDevice;

    fn device() -> VecDevice {
        let mut bytes = vec![0u8; 8 * 512];
        for block in 0..8 {
            bytes[block * 512] = block as u8;
        }
        VecDevice::new(bytes, 512).unwrap()
    }

    /// The value the floppy code hardcodes as 72, now derived from the block.
    #[test]
    fn a_512_byte_block_holds_72_pointers() {
        assert_eq!(hash_table_size(512), 72);
        assert_eq!(pointers_per_block(512), 72);
    }

    #[test]
    fn ofs_gives_up_24_bytes_of_every_block_and_ffs_gives_up_none() {
        assert_eq!(payload_per_block(512, true), 488);
        assert_eq!(payload_per_block(512, false), 512);
    }

    /// Backwards from LW 77. Forwards would produce a file whose blocks come
    /// back in the wrong order — and ART's reader would agree with it.
    #[test]
    fn data_pointers_are_stored_backwards_from_longword_77() {
        assert_eq!(pointer_slot(0, 512).unwrap(), 308);
        assert_eq!(pointer_slot(1, 512).unwrap(), 304);
        assert_eq!(pointer_slot(71, 512).unwrap(), 24);
        assert!(
            pointer_slot(72, 512).is_err(),
            "the 73rd needs an extension block"
        );
    }

    #[test]
    fn the_last_pointer_slot_meets_the_start_of_the_table() {
        assert_eq!(pointer_slot(71, 512).unwrap(), TABLE_OFFSET);
    }

    #[test]
    fn an_edited_block_keeps_the_bytes_it_was_not_asked_about() {
        let device = device();
        let mut set = BlockSet::new(512);

        let block = set.edit(&device, 3).unwrap();
        set_u32(block, 100, 0xDEAD_BEEF).unwrap();

        let after = set.view(&device, 3).unwrap();
        assert_eq!(get_u32(&after, 100).unwrap(), 0xDEAD_BEEF);
        assert_eq!(after[0], 3, "the byte that was there before is still there");
    }

    #[test]
    fn a_blank_block_starts_empty_even_if_the_volume_had_bytes_there() {
        let device = device();
        let mut set = BlockSet::new(512);
        set.blank(3);

        let after = set.view(&device, 3).unwrap();
        assert!(after.iter().all(|b| *b == 0));
        assert_eq!(after.len(), 512);
    }

    #[test]
    fn a_block_not_in_the_set_reads_from_the_volume() {
        let device = device();
        let set = BlockSet::new(512);
        assert_eq!(set.view(&device, 5).unwrap()[0], 5);
    }

    #[test]
    fn touched_lists_every_block_the_operation_will_write() {
        let device = device();
        let mut set = BlockSet::new(512);
        set.blank(7);
        set.edit(&device, 2).unwrap();
        set.blank(4);
        assert_eq!(set.touched(), vec![2, 4, 7]);
    }

    #[test]
    fn a_checksum_is_computed_over_the_block_with_its_own_field_zeroed() {
        let device = device();
        let mut set = BlockSet::new(512);
        let block = set.edit(&device, 1).unwrap();
        set_u32(block, 200, 0x1234_5678).unwrap();
        set.checksum(1, CHECKSUM_OFFSET).unwrap();

        let bytes = set.view(&device, 1).unwrap();
        // Verifying the way a reader does: zero the field and recompute.
        let mut check = bytes.clone();
        check[CHECKSUM_OFFSET..CHECKSUM_OFFSET + 4].fill(0);
        assert_eq!(
            get_u32(&bytes, CHECKSUM_OFFSET).unwrap(),
            block_checksum(&check, CHECKSUM_OFFSET)
        );
    }

    #[test]
    fn an_offset_past_the_end_of_a_block_is_an_error_not_a_panic() {
        let mut bytes = vec![0u8; 512];
        assert!(get_u32(&bytes, 512).is_err());
        assert!(get_u32(&bytes, usize::MAX).is_err());
        assert!(set_u32(&mut bytes, 509, 1).is_err());
        assert!(set_u32(&mut bytes, 508, 1).is_ok());
    }

    /// A clock set before 1978 must clamp, not wrap: a negative day count
    /// displays as a date far in the future on a real Amiga.
    #[test]
    fn a_date_before_the_amiga_epoch_clamps_to_it() {
        let date = amiga_from_unix(0); // 1970
        assert_eq!(date.days, 0);
        assert_eq!(date.mins, 0);
        assert_eq!(date.ticks, 0);
    }

    #[test]
    fn a_date_after_the_epoch_converts_and_converts_back() {
        let unix = crate::core::adf::bcpl::AMIGA_EPOCH_UNIX + 86_400 * 100 + 3600 + 42;
        let date = amiga_from_unix(unix);
        assert_eq!(date.days, 100);
        assert_eq!(date.mins, 60);
        assert_eq!(date.ticks, 42 * 50);
        assert_eq!(date.to_unix(), unix);
    }

    /// `----RWED`: the low four bits are inverted, so zero grants everything.
    /// A copy that came out `----` would leave files nobody could read.
    #[test]
    fn the_default_protection_grants_read_write_execute_and_delete() {
        assert_eq!(DEFAULT_PROTECTION & 0x0F, 0);
        assert_eq!(DEFAULT_PROTECTION & PROTECT_ARCHIVE, 0);
    }
}
