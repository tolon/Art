//! Free-space allocation across as many bitmap blocks as a volume needs.
//!
//! A DD floppy has one bitmap block covering all 1758 usable blocks, which is
//! why `core/adf` could hardcode block 881 and get away with it. A 100 MB
//! partition has fifty-two of them, listed in the root block's `bm_pages[25]`
//! and, once those run out, in a chain of bitmap **extension** blocks. Code
//! that assumes one bitmap block cannot allocate past block 4062 of any volume
//! (brief §3.1).
//!
//! ## The layout
//!
//! ```text
//! root block  ┌ 312 bm_flag        -1 when the bitmap is valid
//!             │ 316 bm_pages[25]   block numbers of the first 25 bitmap blocks
//!             └ 416 bm_ext         first bitmap extension block, or 0
//!
//! ext block   ┌   0 pages[127]     127 more bitmap block numbers
//!             └ 508 next           the next extension block, or 0
//!
//! bitmap blk  ┌   0 checksum
//!             └   4 bits[127]      1 = free, 0 = used. LSB first.
//! ```
//!
//! ## `bm_flag`
//!
//! AmigaDOS sets `bm_flag` to `-1` when the bitmap is valid and to 0 when it
//! was left dirty by an unclean shutdown. ART **refuses to write** to a volume
//! whose flag is not valid: allocating from a bitmap that does not describe the
//! disk is how two files come to own the same block. Rebuilding it is Recovery
//! Lab work (§29), not something to do silently on the way to copying a file.

use crate::core::adf::blocks::{bit_position, BLOCKS_PER_BITMAP_BLOCK};
use crate::core::adf::checksum::block_checksum;
use crate::core::error::{CoreError, CoreResult};
use crate::core::volume::{read_block_vec, BlockDevice, VolumeGeometry};

/// Offset of `bm_flag` in the root block.
pub const BM_FLAG_OFFSET: usize = 312;
/// Offset of `bm_pages[0]` in the root block.
pub const BM_PAGES_OFFSET: usize = 316;
/// How many bitmap block numbers the root block itself holds.
pub const BM_PAGES_IN_ROOT: usize = 25;
/// Offset of `bm_ext` in the root block.
pub const BM_EXT_OFFSET: usize = 416;

/// The value `bm_flag` holds when the bitmap can be trusted.
pub const BM_FLAG_VALID: u32 = 0xFFFF_FFFF;

/// The most bitmap extension blocks ART will follow.
///
/// At 127 pages each and 4064 blocks per page, ten extension blocks already
/// describe five million blocks — far past the 2 GiB volume ceiling. The limit
/// is here because a corrupt image can point an extension block at itself, and
/// a chain walk without a bound is an infinite loop (the ART-008 lesson).
const MAX_BITMAP_EXTENSIONS: usize = 64;

/// Where all of a volume's bitmap blocks are.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BitmapLayout {
    /// Bitmap block numbers, in order. Block `reserved + i * 4064` onwards is
    /// described by `pages[i]`.
    pub pages: Vec<u32>,
    /// True when `bm_flag` says the bitmap is valid.
    pub valid: bool,
}

impl BitmapLayout {
    /// How many bitmap blocks a volume of this geometry needs.
    pub fn pages_needed(geometry: &VolumeGeometry) -> usize {
        let described = geometry.total_blocks.saturating_sub(geometry.reserved) as usize;
        described.div_ceil(BLOCKS_PER_BITMAP_BLOCK)
    }

    /// Read the layout from a volume's root block and extension chain.
    pub fn read<D: BlockDevice + ?Sized>(
        device: &D,
        geometry: &VolumeGeometry,
    ) -> CoreResult<Self> {
        let root = read_block_vec(device, geometry.root_block)?;
        let valid = read_u32(&root, BM_FLAG_OFFSET)? == BM_FLAG_VALID;

        let needed = Self::pages_needed(geometry);
        let mut pages = Vec::with_capacity(needed);

        for index in 0..BM_PAGES_IN_ROOT.min(needed) {
            let page = read_u32(&root, BM_PAGES_OFFSET + index * 4)?;
            pages.push(check_page(page, geometry)?);
        }

        // Only follow the extension chain if the root's own slots ran out. A
        // volume small enough to fit in bm_pages may still have rubbish in
        // bm_ext, and following it would be reading a block for no reason.
        if needed > BM_PAGES_IN_ROOT {
            let mut next = read_u32(&root, BM_EXT_OFFSET)?;
            let mut hops = 0usize;

            while next != 0 && pages.len() < needed {
                hops += 1;
                if hops > MAX_BITMAP_EXTENSIONS {
                    return Err(CoreError::Malformed {
                        format: "volume".into(),
                        detail: "the bitmap extension chain does not end".into(),
                    });
                }
                if next >= geometry.total_blocks {
                    return Err(CoreError::Malformed {
                        format: "volume".into(),
                        detail: format!("bitmap extension block {next} is outside the volume"),
                    });
                }

                let extension = read_block_vec(device, next)?;
                let slots = (geometry.block_size / 4) - 1;
                for index in 0..slots {
                    if pages.len() >= needed {
                        break;
                    }
                    let page = read_u32(&extension, index * 4)?;
                    pages.push(check_page(page, geometry)?);
                }
                next = read_u32(&extension, geometry.block_size - 4)?;
            }
        }

        if pages.len() < needed {
            return Err(CoreError::Malformed {
                format: "volume".into(),
                detail: format!(
                    "this volume needs {needed} bitmap blocks but its root block and \
                     extension chain name only {}",
                    pages.len()
                ),
            });
        }

        Ok(Self { pages, valid })
    }

    /// Why this bitmap cannot be allocated from, or `None` when it can.
    ///
    /// The message is the one the user sees, so it says what ART will not do
    /// and why, not which flag was wrong.
    pub fn refusal(&self) -> Option<String> {
        (!self.valid).then(|| {
            "This volume's free-space map is marked invalid — the disk was not shut down \
             cleanly. ART will not write to it, because allocating from a map that does \
             not describe the disk is how two files come to own the same block. \
             Validate it on an Amiga, or in an emulator, first."
                .to_string()
        })
    }

    /// Which bitmap block holds `block`'s bit, and where in it.
    pub fn locate(&self, block: u32, geometry: &VolumeGeometry) -> Option<(u32, usize, u32)> {
        let position = bit_position(block, geometry.reserved, geometry.total_blocks)?;
        let page = *self.pages.get(position.bitmap_index)?;
        Some((page, position.byte_offset, position.mask))
    }
}

fn check_page(page: u32, geometry: &VolumeGeometry) -> CoreResult<u32> {
    if page == 0 || page >= geometry.total_blocks {
        return Err(CoreError::Malformed {
            format: "volume".into(),
            detail: format!("bitmap block {page} is outside the volume"),
        });
    }
    Ok(page)
}

fn read_u32(block: &[u8], offset: usize) -> CoreResult<u32> {
    let end = offset.checked_add(4).ok_or_else(|| CoreError::Malformed {
        format: "volume".into(),
        detail: "a block offset overflowed".into(),
    })?;
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

fn write_u32(block: &mut [u8], offset: usize, value: u32) -> CoreResult<()> {
    let end = offset.checked_add(4).ok_or_else(|| CoreError::Malformed {
        format: "volume".into(),
        detail: "a block offset overflowed".into(),
    })?;
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

/// A volume's free-space map, loaded into memory for one operation.
///
/// Held as a plain bit vector rather than as the raw blocks: an allocation of
/// three hundred blocks touching four bitmap blocks becomes four block writes
/// at the end instead of three hundred read-modify-writes, and — more
/// importantly — the whole allocation is decided before *any* of it is
/// journalled, so the set of blocks the operation will touch is known up front.
#[derive(Debug, Clone)]
pub struct Allocator {
    layout: BitmapLayout,
    geometry: VolumeGeometry,
    /// One entry per block in the volume. `true` = free.
    free: Vec<bool>,
}

impl Allocator {
    /// Load the whole map. Refuses a volume whose `bm_flag` is not valid.
    pub fn load<D: BlockDevice + ?Sized>(
        device: &D,
        geometry: &VolumeGeometry,
    ) -> CoreResult<Self> {
        let layout = BitmapLayout::read(device, geometry)?;
        if let Some(reason) = layout.refusal() {
            return Err(CoreError::InvalidInput(reason));
        }

        let mut free = vec![false; geometry.total_blocks as usize];
        // One read per bitmap block, not one per volume block.
        let mut pages = Vec::with_capacity(layout.pages.len());
        for &page in &layout.pages {
            pages.push(read_block_vec(device, page)?);
        }

        for (block, slot) in free.iter_mut().enumerate() {
            let Some(position) =
                bit_position(block as u32, geometry.reserved, geometry.total_blocks)
            else {
                // Reserved blocks are not described by the bitmap and are
                // never free.
                continue;
            };
            let Some(page) = pages.get(position.bitmap_index) else {
                continue;
            };
            *slot = read_u32(page, position.byte_offset)? & position.mask != 0;
        }

        Ok(Self {
            layout,
            geometry: *geometry,
            free,
        })
    }

    /// Blocks the map reports as free.
    pub fn free_count(&self) -> usize {
        self.free.iter().filter(|free| **free).count()
    }

    pub fn is_free(&self, block: u32) -> bool {
        self.free.get(block as usize).copied().unwrap_or(false)
    }

    /// Reserve `count` blocks, or fail without reserving any.
    ///
    /// All-or-nothing on purpose: a partial allocation would leave blocks
    /// marked used that no file references, which is a leak the user has no
    /// way to see. The caller learns it will not fit *before* the journal is
    /// opened.
    pub fn allocate(&mut self, count: usize) -> CoreResult<Vec<u32>> {
        if count == 0 {
            return Ok(Vec::new());
        }

        let mut taken = Vec::with_capacity(count);
        for block in self.geometry.reserved..self.geometry.total_blocks {
            if self.free[block as usize] {
                taken.push(block);
                if taken.len() == count {
                    break;
                }
            }
        }

        if taken.len() < count {
            return Err(CoreError::InvalidInput(format!(
                "this needs {count} blocks and only {} are free",
                taken.len()
            )));
        }

        for &block in &taken {
            self.free[block as usize] = false;
        }
        Ok(taken)
    }

    /// Give blocks back.
    ///
    /// A block outside the volume, or one the bitmap does not describe, is
    /// ignored rather than treated as an error: freeing is what happens on the
    /// failure path, and it must not fail in turn.
    pub fn free_blocks(&mut self, blocks: &[u32]) {
        for &block in blocks {
            if block < self.geometry.reserved {
                continue;
            }
            if let Some(slot) = self.free.get_mut(block as usize) {
                *slot = true;
            }
        }
    }

    /// The bitmap blocks that would have to be rewritten to persist the
    /// current state, given the blocks whose bits changed.
    ///
    /// Called *before* the journal opens, so the journal knows every block the
    /// operation will touch.
    pub fn pages_covering(&self, blocks: &[u32]) -> Vec<u32> {
        let mut pages: Vec<u32> = blocks
            .iter()
            .filter_map(|&block| {
                self.layout
                    .locate(block, &self.geometry)
                    .map(|(page, _, _)| page)
            })
            .collect();
        pages.sort_unstable();
        pages.dedup();
        pages
    }

    /// Every bitmap block of this volume.
    pub fn all_pages(&self) -> &[u32] {
        &self.layout.pages
    }

    /// Rebuild the bitmap blocks affected by `blocks` and hand them back as
    /// (block number, contents) ready to be written through the journal.
    ///
    /// Reads the current contents so untouched longwords — and anything a
    /// future filesystem revision puts in the same block — pass through
    /// unchanged rather than being regenerated from ART's model.
    pub fn rebuild_pages<D: BlockDevice + ?Sized>(
        &self,
        device: &D,
        blocks: &[u32],
    ) -> CoreResult<Vec<(u32, Vec<u8>)>> {
        let mut out = Vec::new();

        for page in self.pages_covering(blocks) {
            let mut bytes = read_block_vec(device, page)?;

            for &block in blocks {
                let Some((owner, offset, mask)) = self.layout.locate(block, &self.geometry) else {
                    continue;
                };
                if owner != page {
                    continue;
                }
                let mut word = read_u32(&bytes, offset)?;
                if self.free[block as usize] {
                    word |= mask;
                } else {
                    word &= !mask;
                }
                write_u32(&mut bytes, offset, word)?;
            }

            // Longword 0 is the checksum over the rest of the block.
            write_u32(&mut bytes, 0, 0)?;
            let checksum = block_checksum(&bytes, 0);
            write_u32(&mut bytes, 0, checksum)?;

            out.push((page, bytes));
        }

        Ok(out)
    }
}

// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::volume::device::VecDevice;
    use crate::core::volume::fixture::ffs_volume;
    use crate::core::volume::DosType;

    fn floppy() -> (VecDevice, VolumeGeometry) {
        let (bytes, geometry) = ffs_volume(1760, DosType::new(*b"DOS\x01"));
        (VecDevice::new(bytes, 512).unwrap(), geometry)
    }

    /// The number the floppy code hardcoded, now derived.
    #[test]
    fn a_floppy_needs_exactly_one_bitmap_block() {
        let geometry = VolumeGeometry::floppy_dd(DosType::new(*b"DOS\x01"));
        assert_eq!(BitmapLayout::pages_needed(&geometry), 1);
    }

    /// The reason this module exists. One bitmap block describes 4064 blocks;
    /// a volume of any real size needs many, and the root block only has room
    /// for 25 of them.
    #[test]
    fn a_larger_volume_needs_more_than_one_bitmap_block() {
        let dos = DosType::new(*b"DOS\x01");
        // Exactly 4064 usable blocks still fits in one page.
        let geometry = VolumeGeometry::new(512, 4064 + 2, 2, dos).unwrap();
        assert_eq!(BitmapLayout::pages_needed(&geometry), 1);

        // One more block needs a second.
        let geometry = VolumeGeometry::new(512, 4064 + 3, 2, dos).unwrap();
        assert_eq!(BitmapLayout::pages_needed(&geometry), 2);

        // A 64 MiB partition: past what bm_pages[25] can hold on its own.
        let geometry = VolumeGeometry::new(512, 131_072, 2, dos).unwrap();
        assert_eq!(BitmapLayout::pages_needed(&geometry), 33);
        assert!(
            BitmapLayout::pages_needed(&geometry) > BM_PAGES_IN_ROOT,
            "this size must exercise the extension chain"
        );
    }

    #[test]
    fn a_floppys_bitmap_reads_back_with_its_one_page() {
        let (device, geometry) = floppy();
        let layout = BitmapLayout::read(&device, &geometry).unwrap();
        assert_eq!(layout.pages.len(), 1);
        assert_eq!(layout.pages[0], 881);
        assert!(layout.valid);
        assert!(layout.refusal().is_none());
    }

    #[test]
    fn allocation_takes_free_blocks_and_marks_them_used() {
        let (device, geometry) = floppy();
        let mut allocator = Allocator::load(&device, &geometry).unwrap();

        let before = allocator.free_count();
        let taken = allocator.allocate(3).unwrap();

        assert_eq!(taken.len(), 3);
        assert_eq!(allocator.free_count(), before - 3);
        for block in &taken {
            assert!(!allocator.is_free(*block));
            assert!(*block >= geometry.reserved && *block < geometry.total_blocks);
        }
    }

    /// A partial allocation would leak blocks the user cannot see. The caller
    /// has to learn it will not fit before anything is journalled.
    #[test]
    fn an_allocation_that_does_not_fit_takes_nothing() {
        let (device, geometry) = floppy();
        let mut allocator = Allocator::load(&device, &geometry).unwrap();
        let before = allocator.free_count();

        let err = allocator.allocate(before + 1).unwrap_err();
        assert!(err.to_string().contains("are free"), "{err}");
        assert_eq!(
            allocator.free_count(),
            before,
            "a refused allocation must reserve nothing"
        );
    }

    #[test]
    fn freeing_puts_blocks_back() {
        let (device, geometry) = floppy();
        let mut allocator = Allocator::load(&device, &geometry).unwrap();
        let before = allocator.free_count();

        let taken = allocator.allocate(5).unwrap();
        allocator.free_blocks(&taken);

        assert_eq!(allocator.free_count(), before);
        for block in &taken {
            assert!(allocator.is_free(*block));
        }
    }

    /// Never free a reserved block: the boot block is not the bitmap's to give
    /// away, and marking it free would let a file be written over it.
    #[test]
    fn freeing_cannot_hand_out_the_boot_blocks() {
        let (device, geometry) = floppy();
        let mut allocator = Allocator::load(&device, &geometry).unwrap();

        allocator.free_blocks(&[0, 1, u32::MAX]);
        assert!(!allocator.is_free(0));
        assert!(!allocator.is_free(1));
    }

    #[test]
    fn rebuilt_pages_round_trip_through_a_reload() {
        let (mut device, geometry) = floppy();
        let mut allocator = Allocator::load(&device, &geometry).unwrap();
        let taken = allocator.allocate(4).unwrap();

        let pages = allocator.rebuild_pages(&device, &taken).unwrap();
        assert_eq!(pages.len(), 1, "a floppy's allocation touches one page");

        use crate::core::volume::BlockDeviceMut;
        for (block, bytes) in &pages {
            device.write_block(*block, bytes).unwrap();
        }

        let reloaded = Allocator::load(&device, &geometry).unwrap();
        for block in &taken {
            assert!(
                !reloaded.is_free(*block),
                "block {block} should still be used after a reload"
            );
        }
        assert_eq!(reloaded.free_count(), allocator.free_count());
    }

    /// §3.1: a volume whose bitmap is marked invalid is read-only. Allocating
    /// from a map that does not describe the disk is how two files come to own
    /// the same block.
    #[test]
    fn an_invalid_bitmap_flag_refuses_writes() {
        let (mut device, geometry) = floppy();

        use crate::core::volume::BlockDeviceMut;
        let mut root = read_block_vec(&device, geometry.root_block).unwrap();
        write_u32(&mut root, BM_FLAG_OFFSET, 0).unwrap();
        device.write_block(geometry.root_block, &root).unwrap();

        let layout = BitmapLayout::read(&device, &geometry).unwrap();
        assert!(!layout.valid);
        assert!(layout.refusal().unwrap().contains("will not write"));

        let err = Allocator::load(&device, &geometry).unwrap_err();
        assert!(err.to_string().contains("not shut down cleanly"), "{err}");
    }

    /// A corrupt image can point a bitmap page anywhere. `panic = "abort"`
    /// means an unchecked one would take the application down.
    #[test]
    fn a_bitmap_page_outside_the_volume_is_refused() {
        let (mut device, geometry) = floppy();

        use crate::core::volume::BlockDeviceMut;
        let mut root = read_block_vec(&device, geometry.root_block).unwrap();
        write_u32(&mut root, BM_PAGES_OFFSET, 99_999).unwrap();
        device.write_block(geometry.root_block, &root).unwrap();

        let err = BitmapLayout::read(&device, &geometry).unwrap_err();
        assert!(err.to_string().contains("outside the volume"), "{err}");
    }

    #[test]
    fn a_bitmap_page_of_zero_is_refused() {
        let (mut device, geometry) = floppy();

        use crate::core::volume::BlockDeviceMut;
        let mut root = read_block_vec(&device, geometry.root_block).unwrap();
        write_u32(&mut root, BM_PAGES_OFFSET, 0).unwrap();
        device.write_block(geometry.root_block, &root).unwrap();

        assert!(BitmapLayout::read(&device, &geometry).is_err());
    }

    #[test]
    fn pages_covering_reports_each_bitmap_block_once() {
        let (device, geometry) = floppy();
        let allocator = Allocator::load(&device, &geometry).unwrap();

        // Every block of a floppy lives in the same single bitmap page.
        let pages = allocator.pages_covering(&[10, 200, 1500]);
        assert_eq!(pages, vec![881]);
    }
}
