//! How big a card image and its partitions are — the one-button card
//! design's § 4 (`docs/superpowers/specs/2026-09-11-one-button-card-design.md`).
//!
//! Every number here is measured or read from the code of a project that
//! works; the sources are in `docs/superpowers/notes/2026-09-11-card-layout-research.md`.

/// The share of a card's printed capacity an image may use, per mille.
///
/// **95 %, emu68hatcher's rule** — *"95% of decimal GB for SD card safety"*
/// (`rootrootde/emu68hatcher`, MIT, `config/partition_helpers.py:34-36`).
/// Cards carrying one label differ by up to ~2 %: two real "64 GB" cards
/// expose 62 534 975 488 and 63 864 569 856 bytes, and an image has to fit the
/// smaller. MultibootOS's last partition ends at 121.60 × 10⁹ on a 128 GB card
/// — exactly 95 %.
pub const CARD_MARGIN_PER_MILLE: u64 = 950;

/// The image size for a card sold as `card_gb` gigabytes — **decimal**, the
/// way every card maker counts (ART-308: this used to be `card_gb × 2³⁰`, and
/// ART's "64" was 4.7 × 10⁹ bytes past any 64 GB card).
pub fn image_bytes_for_label(card_gb: u32) -> u64 {
    u64::from(card_gb) * 1_000_000 * CARD_MARGIN_PER_MILLE
}

/// One PFS3 block on a card: the partition's own block size.
pub const PFS3_BLOCK: u64 = 512;
/// The smallest PFS3 partition worth making (Emu68-Imager's own floor,
/// `Get-MinimumPartitionSizes.ps1`).
pub const PFS3_MIN_BYTES: u64 = 10 * 1024 * 1024;
/// The largest partition PFS3 supports in its normal mode — pfs3aio
/// `blocks.h:106-120`, *"normaldisk = 213.021.952 blocks of 512 byte"*. Past it
/// the format is experimental; both established imagers cap at 101 GiB.
pub const PFS3_MAX_BLOCKS: u64 = 213_021_952;
pub const PFS3_MAX_BYTES: u64 = PFS3_MAX_BLOCKS * PFS3_BLOCK;

// Ported from `libpfs3` 0.1.3 `format.rs` (itself pfs3aio's `format.c`), and
// held to it by `the_ported_reserved_area_matches_libpfs3s_own_format`.
const MAXSMALLBITMAPINDEX: u64 = 4;
const MAXBITMAPINDEX: u64 = 103;
const MAXSMALLDISK: u64 = (MAXSMALLBITMAPINDEX + 1) * 253 * 253 * 32;
const MAXNUMRESERVED: u32 = 4096 + 255 * 1024 * 8;

/// The size of one reserved block, from the partition's size.
pub fn pfs3_reserved_block_bytes(total_blocks: u64) -> u32 {
    let mut size = 1024;
    if total_blocks > MAXSMALLDISK {
        if total_blocks > (MAXBITMAPINDEX + 1) * 253 * 253 * 32 {
            size = 2048;
        }
        if total_blocks > (MAXBITMAPINDEX + 1) * 509 * 509 * 32 {
            size = 4096;
        }
    }
    size
}

/// How many reserved blocks a format sets aside — `calc_num_reserved`.
pub fn pfs3_num_reserved(total_blocks: u64) -> u32 {
    let resblocksize = pfs3_reserved_block_bytes(total_blocks);
    let mut taken: u32 = 32;
    let mut i: u64 = 2048;
    while i > 0 && i / 2 < total_blocks {
        let m: u32 = if i >= 512 * 2048 { 10 } else { 14 };
        taken += taken * m / 16;
        i = i.checked_shl(1).unwrap_or(0);
    }
    taken /= resblocksize / 1024;
    taken = taken.saturating_sub(1).min(MAXNUMRESERVED);
    taken = (taken + 31) & !0x1F;
    taken.max(32)
}

/// The blocks a fresh format leaves for file data.
pub fn pfs3_data_blocks(total_blocks: u64) -> u64 {
    let rescluster = u64::from(pfs3_reserved_block_bytes(total_blocks)) / PFS3_BLOCK;
    let reserved_area = rescluster * u64::from(pfs3_num_reserved(total_blocks)) + 2;
    total_blocks.saturating_sub(reserved_area)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The smallest real card measured for each label (fdisk output in the
    /// threads the research note cites). An image for that label must fit it.
    const SMALLEST_MEASURED: [(u32, u64); 3] = [
        (32, 31_914_983_424),
        (64, 62_534_975_488),
        (128, 127_865_454_592),
    ];

    #[test]
    fn a_card_image_is_ninety_five_percent_of_the_decimal_label() {
        assert_eq!(image_bytes_for_label(16), 15_200_000_000);
        assert_eq!(image_bytes_for_label(32), 30_400_000_000);
        assert_eq!(image_bytes_for_label(64), 60_800_000_000);
        assert_eq!(image_bytes_for_label(128), 121_600_000_000);
    }

    /// **ART-308.** The image for a label fits the smallest card of that label
    /// measured anywhere — and is no longer 64 GiB for "64".
    #[test]
    fn a_card_image_fits_the_smallest_real_card_of_its_label() {
        for (label, smallest) in SMALLEST_MEASURED {
            assert!(
                image_bytes_for_label(label) <= smallest,
                "{label} GB: {} > {smallest}",
                image_bytes_for_label(label)
            );
        }
        assert!(image_bytes_for_label(64) < 64 * 1024 * 1024 * 1024);
    }

    use std::collections::HashMap;
    use std::sync::Mutex;

    /// A block device that keeps only the blocks written to it — so a
    /// 100 GiB PFS3 format costs the reserved area in memory, not 100 GiB of
    /// disk. `libpfs3`'s own trait, `libpfs3::io::BlockDevice`.
    #[derive(Default)]
    struct MemDevice {
        blocks: Mutex<HashMap<u64, Vec<u8>>>,
    }

    impl libpfs3::io::BlockDevice for MemDevice {
        fn read_block(&self, block: u64, buf: &mut [u8]) -> libpfs3::error::Result<()> {
            match self.blocks.lock().unwrap().get(&block) {
                Some(data) => buf.copy_from_slice(data),
                None => buf.fill(0),
            }
            Ok(())
        }
        fn read_blocks(
            &self,
            block: u64,
            count: u32,
            buf: &mut [u8],
        ) -> libpfs3::error::Result<()> {
            for i in 0..count as usize {
                self.read_block(block + i as u64, &mut buf[i * 512..(i + 1) * 512])?;
            }
            Ok(())
        }
        fn block_size(&self) -> u32 {
            512
        }
        fn write_block(&self, block: u64, data: &[u8]) -> libpfs3::error::Result<()> {
            self.blocks
                .lock()
                .unwrap()
                .insert(block, data[..512].to_vec());
            Ok(())
        }
        fn write_blocks(&self, block: u64, count: u32, data: &[u8]) -> libpfs3::error::Result<()> {
            for i in 0..count as usize {
                self.write_block(block + i as u64, &data[i * 512..(i + 1) * 512])?;
            }
            Ok(())
        }
        fn flush(&self) -> libpfs3::error::Result<()> {
            Ok(())
        }
    }

    fn format_in_memory(total_blocks: u64) -> (MemDevice, libpfs3::format::FormatResult) {
        let dev = MemDevice::default();
        let result = libpfs3::format::format_with_size(
            &dev,
            total_blocks,
            &libpfs3::format::FormatOptions {
                volume_name: "Test".into(),
                enable_deldir: false,
            },
        )
        .unwrap();
        (dev, result)
    }

    /// The port is `libpfs3`'s own arithmetic or it is nothing: every size
    /// from the smallest PFS3 partition to the largest normal-mode one,
    /// formatted for real, and compared to the block.
    #[test]
    fn the_ported_reserved_area_matches_libpfs3s_own_format() {
        for total_blocks in [
            20_480u64,       // 10 MiB, the minimum
            204_800,         // 100 MiB
            2_097_152,       // 1 GiB
            8_388_608,       // 4 GiB
            10_485_760,      // 5 GiB, around the SUPERINDEX switch
            33_554_432,      // 16 GiB
            125_829_120,     // 60 GiB
            PFS3_MAX_BLOCKS, // 101.58 GiB
        ] {
            let (_dev, result) = format_in_memory(total_blocks);
            assert_eq!(
                pfs3_num_reserved(total_blocks),
                result.num_reserved,
                "{total_blocks} blocks"
            );
            assert_eq!(
                pfs3_reserved_block_bytes(total_blocks),
                result.reserved_blksize,
                "{total_blocks} blocks"
            );
            assert_eq!(
                pfs3_data_blocks(total_blocks),
                result.data_blocks,
                "{total_blocks} blocks"
            );
        }
    }
}
