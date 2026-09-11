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

// The real on-disk sizes the directory/anode model below is built from
// (round-2 review, CRITICAL 1 — the plan's original "17 fixed bytes" guess
// undercounted against the real writer and is superseded by these).
/// `libpfs3` `ondisk/direntry.rs:36` — `DIR_BLOCK_HEADER_SIZE = 0x14`.
const DIR_BLOCK_HEADER_SIZE: u64 = 20;
/// `libpfs3` `ondisk/mod.rs:160` — `ANODE_BLOCK_HEADER_SIZE`.
const ANODE_BLOCK_HEADER_SIZE: u64 = 16;
/// `libpfs3` `ondisk/mod.rs:112` — one anode on disk: clustersize, blocknr,
/// next, 4 bytes each.
const ANODE_SIZE: u64 = 12;
/// `libpfs3` `ondisk/mod.rs:80-81` — `ANODE_ROOTDIR = 5`, `ANODE_USERFIRST =
/// 6`: the low anode numbers below `ANODE_USERFIRST` are reserved and are
/// skipped by the allocator (`writer.rs:931`), all inside the very first
/// anode block (seqnr 0).
const ANODE_USERFIRST: u64 = 6;

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
    /// Every file rounded up to whole 512-byte blocks (at least one, even
    /// for an empty file — `libpfs3` `writer.rs:148`, `.max(1)`).
    pub data_blocks: u64,
    /// Directory entries, `libpfs3`'s own layout (see `entry_bytes` below).
    pub entry_bytes: u64,
}

fn entry_bytes(name: &str) -> u64 {
    // `libpfs3` `writer.rs:1153-1194` `build_dir_entry`: an 18-byte fixed
    // header, the name, a 1-byte comment-length byte (ART writes no
    // comment), and a 2-byte flags field (4 only once a single file needs
    // `MODE_LARGEFILE`'s `fsizex` extension, i.e. >= 4 GiB — out of scope
    // for ART's content), padded up to even (`writer.rs:1166-1169`). This
    // supersedes the plan's original "17 fixed bytes" guess, which measured
    // 3 bytes an entry short against the real writer (round-2 review,
    // CRITICAL 1). Latin-1 on the Amiga side: one byte a character.
    let raw = 18 + name.chars().count() as u64 + 1 + 2;
    raw + raw % 2
}

impl ContentMeasure {
    pub fn add_file(&mut self, name: &str, bytes: u64) {
        self.files += 1;
        self.data_blocks += bytes.div_ceil(PFS3_BLOCK).max(1);
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

/// Directory blocks `content` needs beyond the one guaranteed to root and to
/// every directory, packed the way `add_dir_entry` really packs them:
/// sequentially, a block taking one more entry only while `pos + entry_len <
/// resblocksize` (`writer.rs:1115` — strictly less, so a block is never
/// packed to its exact size; the `-1` below is that byte).
/// `ContentMeasure` does not keep directories apart, so this sums
/// `entry_bytes` across the whole tree and packs it as one pool — which is
/// provably still safe: for directories 1..n with entry-byte totals `e_i`
/// and a shared per-block capacity `C`, `sum(ceil(e_i/C)) <= n +
/// ceil(sum(e_i)/C)` always (`ceil(x/C) < x/C + 1` for every `i`, summed).
/// Real per-directory packing can only need *more* blocks than the pooled
/// estimate, never fewer.
fn dir_extra_blocks(total_blocks: u64, content: &ContentMeasure) -> u64 {
    let capacity = u64::from(pfs3_reserved_block_bytes(total_blocks)) - DIR_BLOCK_HEADER_SIZE - 1;
    content.entry_bytes.div_ceil(capacity)
}

/// Anodes `content` needs: one a file (`create_anode_chain`), one a
/// directory (its own first dir block, `create_dir_in`, `writer.rs:167-169`),
/// and one more for every directory block beyond a directory's guaranteed
/// first (`extend_anode_chain`, `writer.rs:1126-1136` and `1196-1199`) —
/// exactly `dir_extra_blocks` above. The plan's original
/// `files + directories + 16` never charged an anode for directory overflow
/// blocks at all, which is why it undercounted for large directories
/// (round-2 review, CRITICAL 1).
fn anode_count_needed(total_blocks: u64, content: &ContentMeasure) -> u64 {
    content.files + content.directories + dir_extra_blocks(total_blocks, content)
}

/// Reserved blocks `content` needs beyond the format's own (bitmap, bitmap
/// index, the format's own first anode block and its index block, root) —
/// directory blocks and anode blocks, each always exactly one reserved
/// block, whatever the reserved block size (`create_dir_in`,
/// `alloc_anode_block`).
fn reserved_needed(total_blocks: u64, content: &ContentMeasure) -> u64 {
    let dir_blocks = (content.directories + 1) + dir_extra_blocks(total_blocks, content);
    let resblocksize = u64::from(pfs3_reserved_block_bytes(total_blocks));
    // `rootblock.rs:158-161` `anodes_per_block` — 84 at the 1024-byte
    // reserved block size every size in ART's normal-mode range uses; the
    // plan's original `80` over-counted this term on its own (safe by
    // itself), but that safety margin was too small to cover the missing
    // `dir_extra_blocks` anode charge above (round-2 review, CRITICAL 1).
    let anodes_per_block = (resblocksize - ANODE_BLOCK_HEADER_SIZE) / ANODE_SIZE;
    let anode_count = anode_count_needed(total_blocks, content);
    // The format's own first anode block (seqnr 0, holding ANODE_ROOTDIR) is
    // already inside `pfs3_num_reserved`'s count; it offers
    // `anodes_per_block - ANODE_USERFIRST` slots to new content before a
    // second anode block (a fresh reserved block) is needed.
    let first_block_room = anodes_per_block.saturating_sub(ANODE_USERFIRST);
    let anode_blocks = anode_count
        .saturating_sub(first_block_room)
        .div_ceil(anodes_per_block);
    dir_blocks + anode_blocks
}

/// The most anodes `libpfs3`'s writer can ever serve below `MAXSMALLDISK`
/// (small mode), independent of the partition's size (round-2 review,
/// IMPORTANT 2). Format only ever pre-allocates ONE anode index block
/// (`format.rs` "Write anode index block", registered as the single entry
/// `rootblock.indexblocks[0]`), and small mode's own allocator refuses a
/// second one outright rather than allocating one on demand the way large
/// (SUPERINDEX) mode does: `alloc_anode_block`'s small-mode branch
/// (`writer.rs:1014-1024`) returns `Err("no index block slot available")`
/// the instant `indexblocks[idx_nr]` is unset, where the large-mode branch
/// just above it (`writer.rs:991-1006`) allocates a fresh index block and
/// registers it. One index block holds `index_per_block` anode-block
/// pointers (`(resblocksize/4)-3`, `rootblock.rs:153-155` / `format.rs:113`
/// — 253 at the 1024-byte reserved block size), each anode block holding
/// `anodes_per_block` anodes (84 at that size) minus the `ANODE_USERFIRST`
/// (6) reserved low anode numbers, all inside the very first one. Measured
/// against the real writer: 25 000 small files failed after 20 382 at both
/// a 17 MB and a 21.7 MB small-mode partition — consistent with this cap
/// (253 × 84 - 6 = 21 246) once directory overhead is subtracted.
fn pfs3_small_mode_anode_cap(total_blocks: u64) -> u64 {
    let resblocksize = u64::from(pfs3_reserved_block_bytes(total_blocks));
    let index_per_block = (resblocksize / 4).saturating_sub(3);
    let anodes_per_block = (resblocksize - ANODE_BLOCK_HEADER_SIZE) / ANODE_SIZE;
    (index_per_block * anodes_per_block).saturating_sub(ANODE_USERFIRST)
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
    if content.data_blocks > usable || reserved_needed(total_blocks, content) > reserved_free {
        return false;
    }
    // Below MAXSMALLDISK, the writer's own anode-space ceiling
    // (`pfs3_small_mode_anode_cap`) does not grow with `total_blocks` — a
    // size that otherwise fits must still be refused once content needs
    // more anodes than that, so `pfs3_fit_bytes`'s bisection sizes up past
    // MAXSMALLDISK instead of settling on a small-mode size the real writer
    // would refuse.
    if total_blocks <= MAXSMALLDISK
        && anode_count_needed(total_blocks, content) > pfs3_small_mode_anode_cap(total_blocks)
    {
        return false;
    }
    true
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

/// One of the user's own partitions, requested between System and Work.
///
/// `content: None` is reserved for the planner's own System (first) and Work
/// (last) entries — a caller's requested partition always carries content.
pub struct RequestedPartition {
    pub volume_name: String,
    pub content: Option<ContentMeasure>,
    /// The Advanced override, in bytes — `0` for none.
    pub floor_bytes: u64,
}

/// Why [`plan_card_image`] could not build a plan.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(
    tag = "refusal",
    rename_all = "kebab-case",
    rename_all_fields = "camelCase"
)]
pub enum SizingRefusal {
    DoesNotFit { needed: u64, available: u64 },
    PartitionTooLarge { volume_name: String, bytes: u64 },
    CardTooSmall { card_gb: u32 },
}

/// One partition of the plan, ready for `core::rdb::create_rdb_layout`.
#[derive(Debug, Clone, serde::Serialize)]
pub struct PlannedPartition {
    pub drive_name: String,
    pub volume_name: String,
    pub bytes: u64,
    pub spec: crate::core::rdb::PartitionSpec,
}

/// A whole card image, laid out: System, the user's partitions, Work.
#[derive(Debug, Clone, serde::Serialize)]
pub struct CardImagePlan {
    pub image_bytes: u64,
    pub area_bytes: u64,
    pub partitions: Vec<PlannedPartition>,
}

use super::propose::{MEASURED_BOOT_BYTES, MEASURED_BUFFERS, MEASURED_SYSTEM_MB};
use crate::core::rdb::{AmigaHardDiskFs, PartitionSpec};

/// Two cylinders at the front of the area hold the RDB (`core::rdb`).
const RDB_RESERVED_BYTES: u64 = 2 * BYTES_PER_CYLINDER;

fn spec(drive: &str, bytes: u64, rest: bool, bootable: bool) -> PartitionSpec {
    PartitionSpec {
        drive_name: drive.to_string(),
        fs_type: AmigaHardDiskFs::Pfs3DirectScsi,
        // `core::rdb` rounds MB up to cylinders; floor here so the writer's
        // round-up lands on (never past) the cylinders planned.
        size_mb: if rest {
            0
        } else {
            u32::try_from(bytes / (1024 * 1024)).unwrap_or(u32::MAX)
        },
        bootable,
        boot_priority: 0,
        num_buffers: MEASURED_BUFFERS,
    }
}

/// Plan a whole card image: System first, `content` in order, then Work —
/// split at PFS3's largest partition if what is left does not fit one.
pub fn plan_card_image(
    card_gb: u32,
    content: &[RequestedPartition],
) -> Result<CardImagePlan, SizingRefusal> {
    let image_bytes = image_bytes_for_label(card_gb);
    let area_bytes = image_bytes
        .checked_sub(MEASURED_BOOT_BYTES)
        .ok_or(SizingRefusal::CardTooSmall { card_gb })?;
    let usable = (area_bytes / BYTES_PER_CYLINDER) * BYTES_PER_CYLINDER - RDB_RESERVED_BYTES;

    let system = (u64::from(MEASURED_SYSTEM_MB) * 1024 * 1024).div_ceil(BYTES_PER_CYLINDER)
        * BYTES_PER_CYLINDER;
    let mut sized = vec![("System".to_string(), system)];
    for part in content {
        let from_content = match &part.content {
            Some(m) => {
                content_partition_bytes(m).ok_or_else(|| SizingRefusal::PartitionTooLarge {
                    volume_name: part.volume_name.clone(),
                    bytes: m.data_blocks * PFS3_BLOCK,
                })?
            }
            None => PFS3_MIN_BYTES,
        };
        let bytes = from_content
            .max(part.floor_bytes)
            .div_ceil(BYTES_PER_CYLINDER)
            * BYTES_PER_CYLINDER;
        sized.push((part.volume_name.clone(), bytes));
    }

    let needed: u64 = sized.iter().map(|(_, b)| b).sum::<u64>() + PFS3_MIN_BYTES;
    if needed > usable {
        return Err(SizingRefusal::DoesNotFit {
            needed,
            available: usable,
        });
    }

    // Work: the rest, split every `PFS3_MAX_BYTES` like both imagers.
    let mut rest = usable - (needed - PFS3_MIN_BYTES);
    let mut works = Vec::new();
    while rest > PFS3_MAX_BYTES {
        let piece = (PFS3_MAX_BYTES / BYTES_PER_CYLINDER) * BYTES_PER_CYLINDER;
        works.push(piece);
        rest -= piece;
    }
    works.push(rest);

    let count = sized.len() + works.len();
    let mut partitions = Vec::with_capacity(count);
    for (i, (name, bytes)) in sized.into_iter().enumerate() {
        let drive = format!("SDH{i}");
        partitions.push(PlannedPartition {
            spec: spec(&drive, bytes, false, i == 0),
            drive_name: drive,
            volume_name: name,
            bytes,
        });
    }
    for (j, bytes) in works.into_iter().enumerate() {
        let i = partitions.len();
        let drive = format!("SDH{i}");
        let name = if j == 0 {
            "Work".to_string()
        } else {
            format!("Work_{j}")
        };
        partitions.push(PlannedPartition {
            spec: spec(&drive, bytes, i + 1 == count, false),
            drive_name: drive,
            volume_name: name,
            bytes,
        });
    }
    Ok(CardImagePlan {
        image_bytes,
        area_bytes,
        partitions,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::rdb::create_rdb_layout;

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

    /// Like `profile`, but with `name_len`-character names (`MODE_LONGFN`
    /// allows up to 107, `writer.rs:1162`) — round-2 review's large-directory
    /// / long-name profiles need names longer than `profile`'s fixed 12
    /// characters to exercise `entry_bytes`'s real per-entry cost.
    fn profile_named(
        dirs: usize,
        per_dir: usize,
        name_len: usize,
        size_of: impl Fn(usize) -> usize,
    ) -> (Vec<String>, Vec<(String, usize)>, ContentMeasure) {
        let width = name_len.saturating_sub(1);
        let mut measure = ContentMeasure::default();
        let mut dir_names = Vec::new();
        let mut files = Vec::new();
        for d in 0..dirs {
            let dir = format!("D{d:0width$}");
            measure.add_directory(&dir);
            for f in 0..per_dir {
                let idx = d * per_dir + f;
                let name = format!("F{idx:0width$}");
                let size = size_of(idx);
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
    ///
    /// "Not wasteful" is **minimality, measured on the real writer**, not a
    /// byte bound over the fit's free blocks. PFS3's own reserved area is a
    /// step function of `total_blocks` — `pfs3_num_reserved` jumps 736 -> 1408
    /// at exactly 32 768 blocks, a threshold inside pfs3aio's own doubling
    /// series in `calc_num_reserved` — so the smallest whole-cylinder size
    /// that fits can step past such a boundary and legitimately carry large
    /// data-block slack (measured: 8924 blocks, ~4.4 MB of a ~16.25 MB
    /// partition, for 20 000 small files at cylinder 33 vs. cylinder 32 just
    /// short of the step). No byte bound over that slack is both safe and
    /// tight, because the slack a step produces is not proportional to the
    /// content. Minimality asks the question a byte bound cannot: does the
    /// estimate need every cylinder it has — does one cylinder less fail on
    /// the real writer, either by refusing the write outright or by breaching
    /// the always-free twentieth?
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
        // Still the calibration record, even though it no longer gates a bound.
        let slack = free - data / 20;
        println!("{label}: {bytes} bytes, {free} free blocks, {slack} blocks past the reserve");

        let cyl_blocks = BYTES_PER_CYLINDER / PFS3_BLOCK;
        let min_blocks = PFS3_MIN_BYTES / PFS3_BLOCK;
        let smaller_total = total_blocks.saturating_sub(cyl_blocks);
        if smaller_total < min_blocks {
            println!(
                "{label}: {bytes} bytes is already PFS3_MIN_BYTES, the estimate's floor — no smaller size to check minimality against"
            );
            return;
        }
        match fill(smaller_total, dirs, files) {
            Err(libpfs3::error::Error::DiskFull(msg)) => println!(
                "{label}: one cylinder smaller ({smaller_total} blocks) the writer refused it: {msg}"
            ),
            Err(other) => panic!(
                "{label}: one cylinder smaller ({smaller_total} blocks) failed with an \
                 unexpected error (not DiskFull): {other}"
            ),
            Ok((smaller_free, smaller_data)) => {
                assert!(
                    smaller_free < smaller_data / 20,
                    "{label}: one cylinder smaller ({smaller_total} blocks) still held everything \
                     with {smaller_free} free of {smaller_data} (>= the always-free twentieth) — \
                     the estimate is not minimal"
                );
                println!(
                    "{label}: one cylinder smaller ({smaller_total} blocks) the always-free \
                     twentieth was breached: {smaller_free} free of {smaller_data}"
                );
            }
        }
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
    ///
    /// 10 000 files (`profile(100, 100, ..)`) never crosses out of
    /// `PFS3_MIN_BYTES` — its fit stays at the 21-cylinder floor, so
    /// `assert_estimate_holds`'s one-cylinder-smaller check has nothing below
    /// the floor to try and is skipped, proving nothing about reserved-area
    /// exhaustion. Every count from 10 000 through 13 600 fits the same
    /// 21-cylinder floor; 13 700 is the smallest count whose *estimate*
    /// crosses PFS3's reserved-area step (`pfs3_num_reserved` 736 -> 1408 at
    /// 32 768 blocks — cylinders 22 through 32 all fail `reserved_needed <=
    /// reserved_free`, cylinder 33 is the next that fits: 33 264 blocks,
    /// 17 031 168 bytes).
    ///
    /// **Corrected (round-2 review, IMPORTANT 3):** the round-1 report
    /// mischaracterized this. `assert_estimate_holds` only checks *one*
    /// cylinder smaller than the estimate, and at cylinder 32 that one probe
    /// found a near miss (`reserved_needed` a few blocks over what cylinder
    /// 32 had free) — but a probe one cylinder down is a *local* check, not
    /// a search for the true minimum, and reading "near miss one cylinder
    /// down" as "close to minimal" does not follow from it. Measured
    /// directly: 13 700 files hold at PFS3_MIN_BYTES itself (cylinder 21,
    /// 10 838 016 bytes) — free 5 306 of 19 006 data blocks, nowhere near
    /// the always-free twentieth (950). The 33-cylinder estimate was 57 %
    /// over the size that actually held this content, not "a few blocks
    /// conservative". 14 000 (`profile(140, 100, ..)`) is the smallest round
    /// count past the reserved-area step whose one-cylinder-smaller probe is
    /// itself decisive: verified unchanged after the round-2 model fix (its
    /// estimate is still 33 264 blocks, and one cylinder smaller still runs
    /// the writer out of reserved blocks) — see the calibration lines this
    /// test prints.
    #[test]
    fn many_small_files_do_not_run_out_of_reserved_blocks() {
        let (dirs, files, m) = profile(140, 100, |_| 200);
        assert_estimate_holds("14 000 small files", &dirs, &files, &m);
    }

    /// CRITICAL 1 (round-2 review): ten drawers holding 1 600 files each,
    /// with 100-character names (`MODE_LONGFN` allows up to 107,
    /// `writer.rs:1162`) — the shape that broke the old directory/anode
    /// model. Measured directly against the old model before this fix: at
    /// its 10.8 MB estimate (cylinder 21) *and* its 13.9 MB content
    /// partition (cylinder 27), the real writer ran out of reserved blocks.
    #[test]
    fn large_directories_with_long_names_fit_their_estimate() {
        let (dirs, files, m) = profile_named(10, 1_600, 100, |_| 200);
        assert_estimate_holds("10 large drawers, 100-char names", &dirs, &files, &m);
    }

    /// IMPORTANT 2 (round-2 review): below `MAXSMALLDISK`, `libpfs3`'s
    /// writer can never allocate a second anode index block ("no index
    /// block slot available", `writer.rs:1014-1024`) — small mode's entire
    /// anode space is one index block's worth, `pfs3_small_mode_anode_cap`
    /// blocks regardless of the partition's size (measured against the real
    /// writer: 25 000 files failed after 20 382 at both a 17 MB and a
    /// 21.7 MB small-mode partition). `pfs3_fits` must refuse every
    /// small-mode size once content needs more anodes than that, so
    /// `pfs3_fit_bytes`'s bisection sizes up past `MAXSMALLDISK`.
    ///
    /// This only proves the size crosses the mode boundary, not that the
    /// content can be written there: filling this content at the returned
    /// estimate is **not** attempted here. A direct experiment against
    /// `libpfs3` 0.1.3's SUPERINDEX (large) mode — formatting a volume well
    /// past `MAXSMALLDISK` and calling `create_dir` on it once — fails
    /// immediately with `"anode 5 not found"` (`ANODE_ROOTDIR = 5`) at every
    /// size tried, including far past `MAXSMALLDISK`. Traced to a mismatch
    /// between `format.rs`'s "Write anode index block" step (which registers
    /// the *index* block directly as `superindex[0]`, a 2-level structure)
    /// and `resolve_anode_block`'s large-mode path (`anode.rs:110-125`,
    /// which expects a 3-level `superindex -> index -> anode` structure and
    /// so misreads the index block's own first entry as if it were a
    /// further index pointer) — a `libpfs3` 0.1.3 defect, not an estimate
    /// question, reported to the round rather than worked around here. Filling
    /// AGS scale (~140 000 files) — and SUPERINDEX mode generally — is
    /// round 5's concern per the review's own ruling; this test proves only
    /// what it can honestly prove today.
    #[test]
    fn many_files_cross_into_superindex_mode() {
        let (_dirs, _files, m) = profile(250, 100, |_| 200); // 25 000 files
        let bytes = pfs3_fit_bytes(&m).expect("within PFS3's range");
        let total_blocks = bytes / PFS3_BLOCK;
        assert!(
            total_blocks > MAXSMALLDISK,
            "25 000 files: estimate {bytes} bytes ({total_blocks} blocks) is still small-mode \
             (MAXSMALLDISK={MAXSMALLDISK} blocks) — the anode cap should have pushed this past it"
        );
        println!(
            "25 000 files: {bytes} bytes ({total_blocks} blocks, MAXSMALLDISK={MAXSMALLDISK} \
             blocks) — estimate crosses into SUPERINDEX mode, not filled (see doc comment)"
        );
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

    fn games(mb: u64) -> RequestedPartition {
        let mut m = ContentMeasure::default();
        m.add_file("Game", mb * 1024 * 1024);
        RequestedPartition {
            volume_name: "Games".into(),
            content: Some(m),
            floor_bytes: 0,
        }
    }

    #[test]
    fn a_card_is_system_then_the_users_partitions_then_work() {
        let plan = plan_card_image(64, &[games(4000)]).unwrap();
        let names: Vec<&str> = plan
            .partitions
            .iter()
            .map(|p| p.volume_name.as_str())
            .collect();
        assert_eq!(names, ["System", "Games", "Work"]);
        let drives: Vec<&str> = plan
            .partitions
            .iter()
            .map(|p| p.drive_name.as_str())
            .collect();
        assert_eq!(drives, ["SDH0", "SDH1", "SDH2"]);
        assert!(plan.partitions[0].spec.bootable && !plan.partitions[1].spec.bootable);
        assert_eq!(
            plan.partitions[0].bytes,
            (u64::from(MEASURED_SYSTEM_MB) * 1024 * 1024).div_ceil(BYTES_PER_CYLINDER)
                * BYTES_PER_CYLINDER
        );
        assert_eq!(plan.image_bytes, image_bytes_for_label(64));
    }

    /// The plan is only a plan if the RDB writer builds it: every partition
    /// lands at its size or larger, and the last ends inside the area.
    #[test]
    fn the_plan_is_one_the_rdb_writer_builds_inside_its_area() {
        let plan = plan_card_image(32, &[games(4000), games(900)]).unwrap();
        let specs: Vec<PartitionSpec> = plan.partitions.iter().map(|p| p.spec.clone()).collect();
        let layout = create_rdb_layout(plan.area_bytes, &specs, &[]).unwrap();
        assert!(layout.total_size <= plan.area_bytes);
        assert!(MEASURED_BOOT_BYTES + plan.area_bytes <= plan.image_bytes);
    }

    #[test]
    fn content_that_does_not_fit_is_refused_with_both_numbers() {
        let err = plan_card_image(16, &[games(20_000)]).unwrap_err();
        let SizingRefusal::DoesNotFit { needed, available } = err else {
            panic!("{err:?}")
        };
        assert!(needed > available);
    }

    #[test]
    fn work_past_pfs3s_largest_partition_is_split() {
        let plan = plan_card_image(128, &[]).unwrap();
        let names: Vec<&str> = plan
            .partitions
            .iter()
            .map(|p| p.volume_name.as_str())
            .collect();
        assert_eq!(names, ["System", "Work", "Work_1"]);
        assert!(plan.partitions.iter().all(|p| p.bytes <= PFS3_MAX_BYTES));
    }

    #[test]
    fn an_override_is_a_floor_never_a_way_past_the_card() {
        let mut g = games(100);
        g.floor_bytes = 5 * 1024 * 1024 * 1024;
        let plan = plan_card_image(16, &[g]).unwrap();
        assert!(plan.partitions[1].bytes >= 5 * 1024 * 1024 * 1024);
        let mut huge = games(100);
        huge.floor_bytes = 40 * 1000 * 1000 * 1000;
        assert!(matches!(
            plan_card_image(16, &[huge]),
            Err(SizingRefusal::DoesNotFit { .. })
        ));
    }
}
