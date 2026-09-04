//! AmigaDOS block checksums.
//!
//! **There are two different algorithms, and using the wrong one produces a
//! disk that only ART can read** (ART-033):
//!
//! - The **bootblock** sums with end-around carry and stores the bitwise NOT.
//!   That one lives in [`super::bootblock::BootBlock::compute_checksum`].
//! - **Every other block** — root, file and directory headers, OFS data
//!   blocks, bitmap blocks — uses a plain 32-bit wrapping sum of the other
//!   longwords, stored as its two's-complement negation, so that summing the
//!   whole block *including* the checksum gives zero.
//!
//! This module is the second one. ART used to apply the bootblock algorithm
//! everywhere: self-consistent, and rejected by AmigaOS, WinUAE and every
//! other Amiga tool. `core/rdb.rs` had the right algorithm all along, which is
//! why RDB blocks always validated elsewhere while ADFs did not.
//!
//! Cross-checked against `amitools` (`fs/block/Block.py::_calc_chksum`) on
//! 2026-08-09.

/// Compute the AmigaDOS checksum of a 512-byte block.
///
/// `checksum_offset` is the byte offset of the checksum longword within the
/// block (20 for root/header blocks and OFS data blocks, 0 for bitmap blocks).
/// That longword is treated as zero during the sum.
///
/// **Not for bootblocks.** They use a different algorithm; see the module docs.
pub fn block_checksum(block: &[u8], checksum_offset: usize) -> u32 {
    debug_assert!(block.len() >= 512, "ADF blocks are 512 bytes");
    let mut sum: u32 = 0;
    for i in (0..512).step_by(4) {
        // Treat the checksum longword as zero.
        if i == checksum_offset {
            continue;
        }
        let lw = u32::from_be_bytes([block[i], block[i + 1], block[i + 2], block[i + 3]]);
        sum = sum.wrapping_add(lw);
    }
    // Two's complement: stored so the whole block sums to zero.
    (!sum).wrapping_add(1)
}

/// Verify a block's stored checksum. Returns true if the block is consistent.
pub fn verify_block_checksum(block: &[u8], checksum_offset: usize) -> bool {
    // Recompute with the stored longword treated as zero and compare: on a
    // valid block that reproduces the stored value exactly, which is the same
    // statement as "the whole block sums to zero".
    let stored = u32::from_be_bytes([
        block[checksum_offset],
        block[checksum_offset + 1],
        block[checksum_offset + 2],
        block[checksum_offset + 3],
    ]);
    block_checksum(block, checksum_offset) == stored
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a minimal 512-byte block, compute its checksum, embed it, then
    /// verify round-trips to a valid state.
    #[test]
    fn checksum_round_trip_validates() {
        let mut block = vec![0u8; 512];
        // Some arbitrary non-zero content so the checksum isn't trivially ~0.
        for (i, b) in block.iter_mut().enumerate() {
            *b = ((i as u32).wrapping_mul(0x01010101) & 0xFF) as u8;
        }
        let cks_off = 20; // where root, header and OFS data blocks keep it
        let cks = block_checksum(&block, cks_off);
        // Embed it.
        block[cks_off..cks_off + 4].copy_from_slice(&cks.to_be_bytes());
        // Now verify.
        assert!(verify_block_checksum(&block, cks_off));
    }

    #[test]
    fn checksum_detects_corruption() {
        let mut block = vec![0u8; 512];
        for (i, b) in block.iter_mut().enumerate() {
            *b = (i as u8).wrapping_mul(7);
        }
        let cks_off = 20;
        let cks = block_checksum(&block, cks_off);
        block[cks_off..cks_off + 4].copy_from_slice(&cks.to_be_bytes());

        // Corrupt one payload byte → checksum must fail.
        block[100] ^= 0xFF;
        assert!(!verify_block_checksum(&block, cks_off));
    }

    /// An empty block sums to zero, and the negation of zero is zero.
    ///
    /// This test used to assert `0xFFFFFFFF` — the bitwise NOT — which is what
    /// the *bootblock* algorithm produces. It was pinning the bug (ART-033)
    /// rather than the behaviour.
    #[test]
    fn checksum_all_zero_block() {
        let block = vec![0u8; 512];
        assert_eq!(block_checksum(&block, 20), 0);
    }

    /// The property AmigaOS actually relies on: with the checksum in place,
    /// every longword in the block adds up to zero.
    #[test]
    fn a_checksummed_block_sums_to_zero() {
        let mut block = vec![0u8; 512];
        for (i, b) in block.iter_mut().enumerate() {
            *b = (i as u8).wrapping_mul(13).wrapping_add(7);
        }
        let cks = block_checksum(&block, 20);
        block[20..24].copy_from_slice(&cks.to_be_bytes());

        let total = block
            .as_chunks::<4>()
            .0
            .iter()
            .fold(0u32, |acc, w| acc.wrapping_add(u32::from_be_bytes(*w)));
        assert_eq!(total, 0, "a valid block sums to zero");
    }

    /// Cross-checked against amitools' `Block._calc_chksum` on 2026-08-09: a
    /// plain wrapping sum of the other longwords, negated.
    #[test]
    fn the_algorithm_matches_the_reference_implementation() {
        let mut block = vec![0u8; 512];
        block[0..4].copy_from_slice(&2u32.to_be_bytes());
        block[4..8].copy_from_slice(&880u32.to_be_bytes());
        block[508..512].copy_from_slice(&1u32.to_be_bytes());

        let expected = {
            let mut sum: u32 = 0;
            for (i, w) in block.as_chunks::<4>().0.iter().enumerate() {
                if i == 5 {
                    continue;
                }
                sum = sum.wrapping_add(u32::from_be_bytes(*w));
            }
            sum.wrapping_neg()
        };

        assert_eq!(block_checksum(&block, 20), expected);
    }
}
