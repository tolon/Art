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

/// Bytes a cylinder holds in ART's fixed RDB geometry (`core::rdb`).
pub const BYTES_PER_CYLINDER: u64 = 16 * 63 * 512;
/// A content partition's room to grow, per mille of its fit — MultibootOS's
/// game partitions are 76-80 % full (research note § 5).
pub const HEADROOM_PER_MILLE: u64 = 1250;

/// What host content will cost on PFS3, counted the way PFS3 spends it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ContentMeasure {
    pub files: u64,
    pub directories: u64,
    /// Every file rounded up to whole 512-byte blocks.
    pub data_blocks: u64,
    /// Directory entries: 17 fixed bytes, the name, a comment-length byte,
    /// padded to even (pfs3aio `blocks.h:327-340`).
    pub entry_bytes: u64,
}

fn entry_bytes(name: &str) -> u64 {
    // Latin-1 on the Amiga side: one byte a character.
    let raw = 17 + name.chars().count() as u64 + 1;
    raw + raw % 2
}

impl ContentMeasure {
    pub fn add_file(&mut self, name: &str, bytes: u64) {
        self.files += 1;
        self.data_blocks += bytes.div_ceil(PFS3_BLOCK);
        self.entry_bytes += entry_bytes(name);
    }
    pub fn add_directory(&mut self, name: &str) {
        self.directories += 1;
        self.entry_bytes += entry_bytes(name);
    }
    pub fn merge(&mut self, other: &ContentMeasure) {
        self.files += other.files;
        self.directories += other.directories;
        self.data_blocks += other.data_blocks;
        self.entry_bytes += other.entry_bytes;
    }
}

/// Reserved blocks the content needs beyond the format's own: directory
/// blocks (at least one per directory, entries packed into 1 KB blocks with a
/// 20-byte header), anode blocks (12 bytes an anode, one per file and
/// directory), counted in 1 KB units and rounded to the reserved block size.
fn reserved_needed(total_blocks: u64, content: &ContentMeasure) -> u64 {
    let dir_blocks = (content.directories + 1) + content.entry_bytes.div_ceil(1024 - 20);
    let anode_blocks = (content.files + content.directories + 16).div_ceil(80) + 1;
    // The format's own use of the reserved area (bitmap, bitmap index, anode
    // index, root) is already inside `pfs3_num_reserved`'s count, and what it
    // leaves free is what this is checked against.
    let ones_k = dir_blocks + anode_blocks;
    let per = u64::from(pfs3_reserved_block_bytes(total_blocks)) / 1024;
    ones_k.div_ceil(per)
}

/// Whether `content` fits a fresh PFS3 partition of `total_blocks`, keeping
/// the always-free twentieth pfs3aio holds back on the Amiga.
pub fn pfs3_fits(total_blocks: u64, content: &ContentMeasure) -> bool {
    let data = pfs3_data_blocks(total_blocks);
    let usable = data - data / 20;
    // The format itself spends part of the reserved area; the bitmap alone is
    // data/(253×32) blocks. Leave that and a margin of 8 before counting ours.
    let format_own = data.div_ceil(253 * 32) + 8;
    let reserved_free = u64::from(pfs3_num_reserved(total_blocks)).saturating_sub(format_own);
    content.data_blocks <= usable && reserved_needed(total_blocks, content) <= reserved_free
}

/// The smallest whole-cylinder PFS3 partition `content` fits — `None` past
/// PFS3's normal-mode maximum.
pub fn pfs3_fit_bytes(content: &ContentMeasure) -> Option<u64> {
    let blocks_per_cyl = BYTES_PER_CYLINDER / PFS3_BLOCK;
    let min_cyl = PFS3_MIN_BYTES.div_ceil(BYTES_PER_CYLINDER);
    let max_cyl = PFS3_MAX_BYTES / BYTES_PER_CYLINDER;
    if !pfs3_fits(max_cyl * blocks_per_cyl, content) {
        return None;
    }
    // Fitting only grows with size, so the first size that fits is found by
    // bisection over whole cylinders.
    let (mut lo, mut hi) = (min_cyl, max_cyl);
    while lo < hi {
        let mid = lo + (hi - lo) / 2;
        if pfs3_fits(mid * blocks_per_cyl, content) {
            hi = mid;
        } else {
            lo = mid + 1;
        }
    }
    Some(lo * BYTES_PER_CYLINDER)
}

/// A content partition's size: its fit, a quarter again, whole cylinders.
pub fn content_partition_bytes(content: &ContentMeasure) -> Option<u64> {
    let fit = pfs3_fit_bytes(content)?;
    let wanted = (fit * HEADROOM_PER_MILLE / 1000).max(PFS3_MIN_BYTES);
    let bytes = wanted.div_ceil(BYTES_PER_CYLINDER) * BYTES_PER_CYLINDER;
    (bytes <= PFS3_MAX_BYTES).then_some(bytes)
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

    /// Write `files` into a PFS3 partition of `total_blocks` in memory with
    /// `libpfs3`'s own writer, and return the free blocks left and the data
    /// blocks the format made — or the writer's own error.
    fn fill(
        total_blocks: u64,
        dirs: &[String],
        files: &[(String, usize)],
    ) -> Result<(u64, u64), libpfs3::error::Error> {
        let (dev, result) = format_in_memory(total_blocks);
        let vol = libpfs3::volume::Volume::from_device(Box::new(dev))?;
        let mut writer = libpfs3::writer::Writer::open(vol)?;
        for dir in dirs {
            writer.create_dir(dir)?;
        }
        for (path, size) in files {
            writer.write_file(path, &vec![0x5A; *size])?;
        }
        let vol = writer.into_volume();
        Ok((u64::from(vol.free_blocks()), result.data_blocks))
    }

    /// A content profile: `dirs` directories, `per_dir` files each, sized by
    /// `size_of(i)`, names 12 characters — measured into a `ContentMeasure`
    /// exactly as round 2's walker will measure host files.
    fn profile(
        dirs: usize,
        per_dir: usize,
        size_of: impl Fn(usize) -> usize,
    ) -> (Vec<String>, Vec<(String, usize)>, ContentMeasure) {
        let mut measure = ContentMeasure::default();
        let mut dir_names = Vec::new();
        let mut files = Vec::new();
        for d in 0..dirs {
            let dir = format!("Drawer{d:06}");
            measure.add_directory(&dir);
            for f in 0..per_dir {
                let name = format!("File{:08}", d * per_dir + f);
                let size = size_of(d * per_dir + f);
                measure.add_file(&name, size as u64);
                files.push((format!("{dir}/{name}"), size));
            }
            dir_names.push(dir);
        }
        (dir_names, files, measure)
    }

    /// Fill a partition of exactly the estimated size and require that it
    /// holds everything **and** keeps the twentieth pfs3aio holds back — the
    /// Amiga's handler refuses new files below it (`allocation.c:158`), and
    /// `libpfs3`'s writer does not (so the check is ours to make).
    fn assert_estimate_holds(
        label: &str,
        dirs: &[String],
        files: &[(String, usize)],
        m: &ContentMeasure,
    ) {
        let bytes = pfs3_fit_bytes(m).expect("within PFS3's range");
        let total_blocks = bytes / PFS3_BLOCK;
        let (free, data) = fill(total_blocks, dirs, files).unwrap_or_else(|e| {
            panic!("{label}: the estimated {bytes} bytes did not hold it: {e}")
        });
        assert!(
            free >= data / 20,
            "{label}: {free} free of {data}, under the always-free twentieth"
        );
        // Not wasteful: at most a cylinder past the always-free line, plus
        // what the estimate's own rounding of directory space allows.
        let slack = free - data / 20;
        println!("{label}: {bytes} bytes, {free} free blocks, {slack} blocks past the reserve");
        assert!(
            slack * PFS3_BLOCK <= BYTES_PER_CYLINDER + (m.entry_bytes + 1024 * (m.directories + 1)),
            "{label}: {slack} blocks of slack is more than the estimate should leave"
        );
    }

    /// The owner's 3.9 tree: 4 599 files, median 521 bytes, half of them 512
    /// or less (research note § 5). Synthetic, same shape.
    #[test]
    fn a_tree_shaped_like_the_owners_fits_its_estimate() {
        let (dirs, files, m) = profile(277, 17, |i| match i % 4 {
            0 | 1 => 400,
            2 => 2_000,
            _ => 18_000,
        });
        assert_estimate_holds("3.9-shaped tree", &dirs, &files, &m);
    }

    /// AGS1 carries 140 602 files at 23 KB average; directory space comes out
    /// of PFS3's reserved area, so many small files are what could run it out.
    #[test]
    fn many_small_files_do_not_run_out_of_reserved_blocks() {
        let (dirs, files, m) = profile(200, 100, |_| 200);
        assert_estimate_holds("20 000 small files", &dirs, &files, &m);
    }

    #[test]
    fn a_few_large_files_fit_their_estimate() {
        let (dirs, files, m) = profile(2, 10, |_| 4 * 1024 * 1024);
        assert_estimate_holds("20 large files", &dirs, &files, &m);
    }

    #[test]
    fn a_content_partition_is_a_quarter_larger_than_its_fit_and_whole_cylinders() {
        let (_, _, m) = profile(277, 17, |_| 2_000);
        let fit = pfs3_fit_bytes(&m).unwrap();
        let part = content_partition_bytes(&m).unwrap();
        assert_eq!(part % BYTES_PER_CYLINDER, 0);
        assert!(part >= fit * HEADROOM_PER_MILLE / 1000);
        assert!(part >= PFS3_MIN_BYTES);
        assert!(part < fit * HEADROOM_PER_MILLE / 1000 + BYTES_PER_CYLINDER);
    }

    #[test]
    fn content_past_pfs3s_largest_partition_has_no_size() {
        let mut m = ContentMeasure::default();
        m.add_file("Huge", PFS3_MAX_BYTES);
        assert_eq!(pfs3_fit_bytes(&m), None);
        assert_eq!(content_partition_bytes(&m), None);
    }
}
