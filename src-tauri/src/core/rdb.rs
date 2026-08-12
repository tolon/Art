//! RDB (Rigid Disk Block) parser, partition manager, and creator (Phase 3 & Phase 4).
//!
//! Handles Amiga hard disk partitioning specifications (RDSK, PART, FSHD, LSEG)
//! with full 32-bit block checksum validation and multi-filesystem DosType support
//! (PDS3/PFS3, SFS0, DOS3/DOS1).

use serde::{Deserialize, Serialize};

use crate::core::adf::bcpl::{read_bcpl_string, write_bcpl_string};
use crate::core::error::{CoreError, CoreResult};

pub const BLOCK_SIZE: usize = 512;

// Standard Amiga RDB Signatures (Big-Endian ASCII)
pub const IDNAME_RDSK: u32 = 0x5244_534B; // 'RDSK'
pub const IDNAME_PART: u32 = 0x5041_5254; // 'PART'
pub const IDNAME_FSHD: u32 = 0x4653_4844; // 'FSHD'
pub const IDNAME_LSEG: u32 = 0x4C53_4547; // 'LSEG'
pub const IDNAME_BADB: u32 = 0x4241_4442; // 'BADB'

// Amiga Partition Flag Masks
pub const PART_FLAG_BOOTABLE: u32 = 0x0001; // Bit 0: Bootable partition

/// The "no such block" sentinel used throughout the RDB (-1, not 0).
pub const NO_BLOCK: u32 = 0xFFFF_FFFF;

/// Supported Amiga Hard Disk Filesystem Types with descriptive properties.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AmigaHardDiskFs {
    /// PFS3-AIO / DirectSCSI (PDS\3) - Fast, crash-proof, 64-bit DirectSCSI
    Pfs3DirectScsi,
    /// PFS3 Standard (PFS\3)
    Pfs3Standard,
    /// Smart File System (SFS\0) - Journaled, high performance
    Sfs0,
    /// Fast File System Directory Cache (DOS\3) - Classic standard (2.04+)
    FfsDirCache,
    /// Fast File System International (DOS\1) - Maximum Kickstart 1.3+ compatibility
    FfsStandard,
    /// Custom DosType
    Custom(u32),
}

impl AmigaHardDiskFs {
    pub fn to_dostype_u32(self) -> u32 {
        match self {
            Self::Pfs3DirectScsi => 0x5044_5303, // 'PDS\3'
            Self::Pfs3Standard => 0x5046_5303,   // 'PFS\3'
            Self::Sfs0 => 0x5346_5300,           // 'SFS\0'
            Self::FfsDirCache => 0x444F_5303,    // 'DOS\3'
            Self::FfsStandard => 0x444F_5301,    // 'DOS\1'
            Self::Custom(val) => val,
        }
    }

    pub fn from_dostype_u32(val: u32) -> Self {
        match val {
            0x5044_5303 => Self::Pfs3DirectScsi,
            0x5046_5303 => Self::Pfs3Standard,
            0x5346_5300 => Self::Sfs0,
            0x444F_5303 | 0x444F_5305 => Self::FfsDirCache,
            0x444F_5301 => Self::FfsStandard,
            other => Self::Custom(other),
        }
    }

    pub fn display_name(self) -> &'static str {
        match self {
            Self::Pfs3DirectScsi => "PFS3-AIO (DirectSCSI — PDS\\3)",
            Self::Pfs3Standard => "PFS3 (Standard — PFS\\3)",
            Self::Sfs0 => "Smart File System (SFS\\0)",
            Self::FfsDirCache => "Fast File System DC (DOS\\3)",
            Self::FfsStandard => "Fast File System (DOS\\1)",
            Self::Custom(_) => "Custom Filesystem",
        }
    }
}

/// A file system driver to embed in a new RDB — G4's writing half.
///
/// **What it is, exactly:** `data` is the driver *as it exists on an Amiga
/// disk* — a standard AmigaDOS executable, hunks and all — stored into the
/// `LSEG` chain verbatim, 492 bytes at a time. Kickstart `LoadSeg`s it out of
/// those blocks at boot. Nothing here parses or transforms it; a driver ART
/// does not understand is one ART must not silently alter.
///
/// **Where it comes from is not this module's business.** `core/` opens no
/// network and reads no user path of its own: the bytes arrive from the
/// caller, whether that is a file the user picked or a package the Aminet
/// engine fetched and hash-checked. `pfs3aio` is freely distributable but it
/// is not ART's to ship, and ART ships no Amiga content, ever.
#[derive(Debug, Clone)]
pub struct FileSystemSpec {
    pub dos_type: u32,
    /// The two halves of the FSHD's one version longword: `19` and `2` for a
    /// driver that calls itself 19.2.
    pub version: u16,
    pub revision: u16,
    pub data: Vec<u8>,
}

/// How many bytes of driver one `LSEG` block carries.
///
/// A 512-byte block less five longwords of header — id, summed longs,
/// checksum, host id, next.
const LSEG_DATA_BYTES: usize = BLOCK_SIZE - 20;

/// The marker every well-made Amiga binary carries so `Version` can answer.
const VER_MARKER: &[u8] = b"$VER:";

/// How far past the marker to look. The version follows the program name, so
/// a couple of lines is generous; scanning further would start finding the
/// numbers in an unrelated string.
const VER_SCAN_BYTES: usize = 200;

/// The version a driver states about **itself**.
///
/// The FSHD block has to declare a version, and asking the user to type one
/// invites a wrong answer about a file they downloaded. It matters more than
/// a label: AmigaOS compares the version in the RDB against whatever is
/// already loaded and keeps the **higher** one, so a driver that claims 0.0
/// loses to ROM and is never used — the disk mounts with the wrong filesystem
/// or not at all. Reading `$VER: pfs3aio 19.2 (2.10.18)` out of the binary
/// gets it right without asking.
///
/// Returns `None` when the driver says nothing; the caller must then ask
/// rather than guess (spec §89).
pub fn version_from_ver_string(data: &[u8]) -> Option<(u16, u16)> {
    let marker = data
        .windows(VER_MARKER.len())
        .position(|window| window == VER_MARKER)?;
    let start = marker + VER_MARKER.len();
    let end = start.saturating_add(VER_SCAN_BYTES).min(data.len());

    // The first token shaped like `<digits>.<digits>`. The program name comes
    // first and is skipped by that rule even when it contains a dot
    // (`pfs3aio.dev`), because its left half is not all digits.
    for token in data[start..end].split(|b| b.is_ascii_whitespace() || *b == 0) {
        let Some(dot) = token.iter().position(|b| *b == b'.') else {
            continue;
        };
        let major = &token[..dot];
        if major.is_empty() || !major.iter().all(|b| b.is_ascii_digit()) {
            continue;
        }
        // The revision runs until whatever punctuation follows it — `19.2,`
        // and `19.2)` are both real.
        let minor: Vec<u8> = token[dot + 1..]
            .iter()
            .copied()
            .take_while(|b| b.is_ascii_digit())
            .collect();
        if minor.is_empty() {
            continue;
        }

        // A number too big for the field is not this driver's version; keep
        // looking rather than truncating it into a plausible-looking lie.
        let (Ok(major), Ok(minor)) = (
            std::str::from_utf8(major).unwrap_or("x").parse::<u16>(),
            std::str::from_utf8(&minor).unwrap_or("x").parse::<u16>(),
        ) else {
            continue;
        };
        return Some((major, minor));
    }
    None
}

/// Specification for creating or adding a partition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PartitionSpec {
    pub drive_name: String,
    pub fs_type: AmigaHardDiskFs,
    pub size_mb: u32,
    pub bootable: bool,
    pub boot_priority: i8,
    pub num_buffers: u32,
}

/// Parsed Partition Block (`PART`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParsedPartition {
    pub drive_name: String,
    pub dostype: u32,
    pub dostype_str: String,
    pub fs_type: AmigaHardDiskFs,
    pub low_cyl: u32,
    pub high_cyl: u32,
    pub cylinder_count: u32,
    pub size_bytes: u64,
    pub bootable: bool,
    pub boot_priority: i8,
    pub num_buffers: u32,
    pub block_location: u32,
    pub next_part_block: u32,
    pub checksum_valid: bool,

    // ---- the partition's own DosEnvVec, needed to mount it ----
    //
    // Read from the PART block rather than inherited from the RDSK: they are
    // usually the same, but the partition's own values are what AmigaOS uses,
    // and a disk written by an unusual tool is exactly when that matters.
    /// `SizeBlock` in **longwords** — 128 means 512-byte blocks.
    pub size_block: u32,
    /// `Surfaces` (heads) for this partition.
    pub surfaces: u32,
    pub blocks_per_track: u32,
    /// `DosReserved` — boot blocks at the start of the volume. Typically 2.
    pub reserved: u32,
}

impl ParsedPartition {
    /// Block size in bytes. `SizeBlock` counts longwords.
    pub fn block_bytes(&self) -> u64 {
        (self.size_block as u64) * 4
    }

    /// Where this partition starts in the image file.
    ///
    /// Cylinders are the unit RDB speaks in: one cylinder is
    /// `surfaces * blocks_per_track` blocks.
    pub fn byte_offset(&self) -> Option<u64> {
        let blocks_per_cylinder =
            (self.surfaces as u64).checked_mul(self.blocks_per_track as u64)?;
        (self.low_cyl as u64)
            .checked_mul(blocks_per_cylinder)?
            .checked_mul(self.block_bytes())
    }

    /// How many bytes it spans.
    pub fn byte_length(&self) -> Option<u64> {
        let blocks_per_cylinder =
            (self.surfaces as u64).checked_mul(self.blocks_per_track as u64)?;
        (self.cylinder_count as u64)
            .checked_mul(blocks_per_cylinder)?
            .checked_mul(self.block_bytes())
    }
}

/// One file system driver embedded in the RDB — an `FSHD` block and the
/// `LSEG` chain hanging off it.
///
/// **Why this matters more than it looks.** PFS3 and SFS are not in Kickstart.
/// A partition whose DosType is `PDS\3` mounts only if the driver for it is
/// *inside the RDB*, loaded from these blocks at boot. A partition table that
/// names a filesystem nothing on the disk provides is one an Amiga silently
/// ignores — which is exactly the image ART's own New HDF wizard produces
/// today (ART-084), and exactly what `hst-imager` refuses to produce at all:
///
/// ```text
/// [ERR] File system with DOS type 'PDS3' not found in Rigid Disk Block
/// ```
///
/// Reading these is the cheaper half of G4 and useful on its own: it is what
/// lets ART say *why* a partition will not mount, instead of guessing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParsedFileSystem {
    pub dos_type: u32,
    pub dos_type_str: String,
    /// `19` and `2` for a `version=19.2` driver — the two halves of one
    /// longword, which is how the FSHD stores them.
    pub version: u16,
    pub revision: u16,
    /// Where the `LSEG` chain starts.
    pub seg_list_block: u32,
    /// How many `LSEG` blocks the chain holds.
    pub segment_blocks: u32,
    /// The driver's size in bytes: the data each `LSEG` actually carries,
    /// which its own `SummedLongs` declares — the last block of a chain is
    /// usually part-full, and counting every block as full would overstate
    /// every driver by up to 492 bytes.
    pub size_bytes: u64,
    pub checksum_valid: bool,
    /// The chain could not be followed to its end — a loop, a pointer past
    /// the end of what was read, or a block that is not an `LSEG`. The entry
    /// is still reported: "there is a driver here and ART could not measure
    /// it" is a different and more useful answer than silence.
    pub truncated: bool,
}

/// Parsed Rigid Disk Block (`RDSK`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParsedRdb {
    pub rdb_block: u32,
    pub cylinders: u32,
    pub sectors: u32,
    pub heads: u32,
    pub block_size: u32,
    pub total_capacity_bytes: u64,
    pub partitions: Vec<ParsedPartition>,
    /// The drivers the RDB carries, in chain order. Empty is the normal and
    /// correct state for a disk that only uses filesystems Kickstart already
    /// has (`DOS\0` … `DOS\7`).
    pub file_systems: Vec<ParsedFileSystem>,
    pub free_cylinders: u32,
    pub checksum_valid: bool,
}

impl ParsedRdb {
    /// Whether this RDB carries a driver for `dos_type`.
    ///
    /// The question ART-084 needs answered before it can stop guessing: a
    /// `PDS\3` partition with no `PDS\3` file system beside it is a partition
    /// an Amiga will not mount, and ART can now say so as a fact rather than
    /// as a general warning.
    ///
    /// Kickstart's own filesystems are not in the RDB and do not need to be,
    /// so this answering `false` is only interesting for a DosType Kickstart
    /// does not know.
    pub fn provides_file_system(&self, dos_type: u32) -> bool {
        self.file_systems.iter().any(|fs| fs.dos_type == dos_type)
    }
}

/// How many longwords an RDB block's checksum covers.
///
/// The block's own `SummedLongs` field (LW 1) declares this — it is **not**
/// fixed at 128. Real Amiga disks write 64 for RDSK and PART, and summing the
/// whole 512-byte block instead would reject valid disks whose later longwords
/// carry vendor strings or padding.
fn summed_longs(block: &[u8]) -> usize {
    let declared = u32::from_be_bytes([block[4], block[5], block[6], block[7]]) as usize;
    // Guard against a malformed value: never read past the block, and treat a
    // nonsensical count as the conventional 64.
    if declared == 0 || declared > BLOCK_SIZE / 4 {
        64
    } else {
        declared
    }
}

/// Longword index of the checksum field (offset 8).
const CHECKSUM_LW: usize = 2;

/// Compute the RDB checksum for a block.
///
/// The sum of the first `SummedLongs` longwords, including the checksum slot,
/// must come to zero; the checksum is therefore the two's complement of the
/// sum of the others.
pub fn compute_rdb_checksum(block: &[u8]) -> u32 {
    let count = summed_longs(block);
    let mut sum: u32 = 0;
    for i in 0..count {
        if i == CHECKSUM_LW {
            continue;
        }
        let off = i * 4;
        let lw = u32::from_be_bytes([block[off], block[off + 1], block[off + 2], block[off + 3]]);
        sum = sum.wrapping_add(lw);
    }
    (!sum).wrapping_add(1)
}

/// Verify if an RDB block's checksum is valid.
pub fn verify_rdb_block_checksum(block: &[u8]) -> bool {
    if block.len() < BLOCK_SIZE {
        return false;
    }
    let count = summed_longs(block);
    let mut sum: u32 = 0;
    for i in 0..count {
        let off = i * 4;
        let lw = u32::from_be_bytes([block[off], block[off + 1], block[off + 2], block[off + 3]]);
        sum = sum.wrapping_add(lw);
    }
    sum == 0
}

/// Scan for `RDSK` signature within the first 16 blocks (8 KB).
pub fn find_rdb_location(image: &[u8]) -> Option<usize> {
    for b in 0..16 {
        let off = b * BLOCK_SIZE;
        if off + BLOCK_SIZE <= image.len() {
            let sig =
                u32::from_be_bytes([image[off], image[off + 1], image[off + 2], image[off + 3]]);
            if sig == IDNAME_RDSK {
                return Some(b);
            }
        }
    }
    None
}

/// A longword from `block` at byte `offset`, or `0` past the end.
fn lw(block: &[u8], offset: usize) -> u32 {
    if offset + 4 > block.len() {
        return 0;
    }
    u32::from_be_bytes([
        block[offset],
        block[offset + 1],
        block[offset + 2],
        block[offset + 3],
    ])
}

/// A DosType as its four printable characters — `PDS\3`, `DOS\1`.
fn dos_type_string(dos_type: u32) -> String {
    format!(
        "{}{}{}{}",
        ((dos_type >> 24) & 0xFF) as u8 as char,
        ((dos_type >> 16) & 0xFF) as u8 as char,
        ((dos_type >> 8) & 0xFF) as u8 as char,
        (dos_type & 0xFF) as u8
    )
}

/// The most `LSEG` blocks one driver's chain may have.
///
/// A generous ceiling on a real driver — `pfs3aio` is 121 blocks — and a hard
/// stop for a malformed chain that points at itself. Reaching it marks the
/// entry `truncated` rather than failing the whole parse: an RDB with one bad
/// filesystem chain still has partitions worth reading.
const MAX_LSEG_BLOCKS: u32 = 4096;

/// The most `FSHD` blocks a chain may have. Real disks carry one or two.
const MAX_FILE_SYSTEMS: usize = 32;

/// Walk the `LSEG` chain from `first`, returning (blocks, data bytes, truncated).
///
/// The data length comes from each block's own `SummedLongs` rather than from
/// a fixed 492: the last block of a chain is nearly always part-full, and
/// counting every block as full overstates a driver by up to half a kilobyte.
/// `pfs3aio` measured this way comes to 59,120 bytes, which is what `rdbtool`
/// independently reports.
fn walk_segment_chain(image: &[u8], first: u32) -> (u32, u64, bool) {
    let mut block = first;
    let mut blocks = 0u32;
    let mut bytes = 0u64;
    let mut seen = std::collections::HashSet::new();

    while block != 0 && block != 0xFFFF_FFFF {
        if blocks >= MAX_LSEG_BLOCKS || !seen.insert(block) {
            return (blocks, bytes, true);
        }

        let off = (block as usize).saturating_mul(BLOCK_SIZE);
        if off + BLOCK_SIZE > image.len() {
            return (blocks, bytes, true);
        }
        let slice = &image[off..off + BLOCK_SIZE];
        if lw(slice, 0) != IDNAME_LSEG {
            return (blocks, bytes, true);
        }

        // Five longwords of header — id, summed longs, checksum, host id,
        // next — and the rest is the driver's own bytes.
        let longs = summed_longs(slice);
        bytes += (longs.saturating_sub(5) as u64) * 4;
        blocks += 1;
        block = lw(slice, 16);
    }

    (blocks, bytes, false)
}

/// Read the `FSHD` chain hanging off the RDSK block.
///
/// Every step is bounded the same way the partition walk is — a visited set, a
/// ceiling, and a containment check before each read — because these pointers
/// come out of a file ART did not write, and an RDB that points its filesystem
/// list at itself must cost a loop iteration rather than the application.
fn parse_file_systems(image: &[u8], first: u32) -> Vec<ParsedFileSystem> {
    let mut found = Vec::new();
    let mut block = first;
    let mut seen = std::collections::HashSet::new();

    while block != 0 && block != 0xFFFF_FFFF {
        if found.len() >= MAX_FILE_SYSTEMS || !seen.insert(block) {
            break;
        }

        let off = (block as usize).saturating_mul(BLOCK_SIZE);
        if off + BLOCK_SIZE > image.len() {
            break;
        }
        let slice = &image[off..off + BLOCK_SIZE];
        if lw(slice, 0) != IDNAME_FSHD {
            break;
        }

        // FSHD, in longwords: 0 id · 1 summed longs · 2 checksum · 3 host id ·
        // 4 next · 5 flags · 6-7 reserved · 8 DosType · 9 Version ·
        // 10 PatchFlags · then the DeviceNode, whose `dn_SegListBlock` at
        // LW 18 is where the driver itself begins.
        let dos_type = lw(slice, 32);
        let version_long = lw(slice, 36);
        let seg_list_block = lw(slice, 72);
        let (segment_blocks, size_bytes, truncated) = walk_segment_chain(image, seg_list_block);

        found.push(ParsedFileSystem {
            dos_type,
            dos_type_str: dos_type_string(dos_type),
            version: (version_long >> 16) as u16,
            revision: (version_long & 0xFFFF) as u16,
            seg_list_block,
            segment_blocks,
            size_bytes,
            checksum_valid: verify_rdb_block_checksum(slice),
            truncated,
        });

        block = lw(slice, 16);
    }

    found
}

/// Parse RDB structure, its partition chain and its file system chain.
pub fn parse_rdb(image: &[u8]) -> CoreResult<ParsedRdb> {
    let rdb_block_idx = find_rdb_location(image).ok_or_else(|| CoreError::Malformed {
        format: "rdb".into(),
        detail: "No 'RDSK' signature found within the first 16 blocks".into(),
    })?;

    let rdb_off = rdb_block_idx * BLOCK_SIZE;
    let rdb_slice = &image[rdb_off..rdb_off + BLOCK_SIZE];
    let checksum_valid = verify_rdb_block_checksum(rdb_slice);

    // Geometry offsets:
    // Offset 64 (LW 16): Cylinders
    // Offset 68 (LW 17): Sectors
    // Offset 72 (LW 18): Heads
    let cylinders =
        u32::from_be_bytes([rdb_slice[64], rdb_slice[65], rdb_slice[66], rdb_slice[67]]);
    let sectors = u32::from_be_bytes([rdb_slice[68], rdb_slice[69], rdb_slice[70], rdb_slice[71]]);
    let heads = u32::from_be_bytes([rdb_slice[72], rdb_slice[73], rdb_slice[74], rdb_slice[75]]);
    let block_size = 512u32;

    let total_capacity_bytes = (cylinders as u64)
        .checked_mul(heads as u64)
        .and_then(|v| v.checked_mul(sectors as u64))
        .and_then(|v| v.checked_mul(block_size as u64))
        .unwrap_or(0);

    // The file system list at offset 32 (LW 8), beside the partition list at
    // LW 7. Read before the partitions so a partition can be judged against
    // what the disk actually provides.
    let file_systems = parse_file_systems(image, lw(rdb_slice, 32));

    // First partition pointer at offset 28 (LW 7)
    let mut part_ptr =
        u32::from_be_bytes([rdb_slice[28], rdb_slice[29], rdb_slice[30], rdb_slice[31]]);

    let mut partitions = Vec::new();
    let mut used_cylinders = 0u32;
    let mut visited = std::collections::HashSet::new();

    while part_ptr != 0 && part_ptr != 0xFFFF_FFFF {
        if !visited.insert(part_ptr) || visited.len() > 64 {
            break;
        }

        let p_off = (part_ptr as usize) * BLOCK_SIZE;
        if p_off + BLOCK_SIZE > image.len() {
            break;
        }

        let p_slice = &image[p_off..p_off + BLOCK_SIZE];
        let p_sig = u32::from_be_bytes([p_slice[0], p_slice[1], p_slice[2], p_slice[3]]);
        if p_sig != IDNAME_PART {
            break;
        }

        let p_cks_valid = verify_rdb_block_checksum(p_slice);
        let next_part = u32::from_be_bytes([p_slice[16], p_slice[17], p_slice[18], p_slice[19]]);
        let flags = u32::from_be_bytes([p_slice[20], p_slice[21], p_slice[22], p_slice[23]]);

        // Device name: BCPL string at offset 36
        let drive_name = read_bcpl_string(p_slice, 36).unwrap_or_else(|| "DH?".to_string());

        // Environment vector starts at offset 128 (LW 32)
        // TableSize: offset 128 (LW 32)
        // SizeBlock: offset 132 (LW 33)
        // SecOrg: offset 136 (LW 34)
        // Heads: offset 140 (LW 35)
        // SectorsPerBlock: offset 144 (LW 36)
        // BlocksPerTrack: offset 148 (LW 37)
        // DosReserved: offset 152 (LW 38)
        // PreAlloc: offset 156 (LW 39)
        // Interleave: offset 160 (LW 40)
        // LowCyl: offset 164 (LW 41)
        // HighCyl: offset 168 (LW 42)
        // NumBuffers: offset 172 (LW 43)
        // ...
        // MaxTransfer: offset 180 (LW 45)
        // Mask: offset 184 (LW 46)
        // BootPri: offset 188 (LW 47)
        // DosType: offset 192 (LW 48)
        //
        // These last two were read one longword early until ART-032. The
        // offsets are pinned by `dosenv_offsets_match_the_amiga_layout`.
        let low_cyl = u32::from_be_bytes([p_slice[164], p_slice[165], p_slice[166], p_slice[167]]);
        let high_cyl = u32::from_be_bytes([p_slice[168], p_slice[169], p_slice[170], p_slice[171]]);
        let num_buffers =
            u32::from_be_bytes([p_slice[172], p_slice[173], p_slice[174], p_slice[175]]);
        let size_block =
            u32::from_be_bytes([p_slice[132], p_slice[133], p_slice[134], p_slice[135]]);
        let surfaces = u32::from_be_bytes([p_slice[140], p_slice[141], p_slice[142], p_slice[143]]);
        let blocks_per_track =
            u32::from_be_bytes([p_slice[148], p_slice[149], p_slice[150], p_slice[151]]);
        let dos_reserved =
            u32::from_be_bytes([p_slice[152], p_slice[153], p_slice[154], p_slice[155]]);
        let boot_pri = p_slice[191] as i8;
        let dostype = u32::from_be_bytes([p_slice[192], p_slice[193], p_slice[194], p_slice[195]]);

        let cyl_count = if high_cyl >= low_cyl {
            high_cyl - low_cyl + 1
        } else {
            0
        };
        used_cylinders += cyl_count;

        let cyl_bytes = (heads as u64) * (sectors as u64) * (block_size as u64);
        let part_size = (cyl_count as u64) * cyl_bytes;

        let dostype_str = format!(
            "{}{}{}{}",
            ((dostype >> 24) & 0xFF) as u8 as char,
            ((dostype >> 16) & 0xFF) as u8 as char,
            ((dostype >> 8) & 0xFF) as u8 as char,
            (dostype & 0xFF) as u8
        );

        partitions.push(ParsedPartition {
            drive_name,
            dostype,
            dostype_str,
            fs_type: AmigaHardDiskFs::from_dostype_u32(dostype),
            low_cyl,
            high_cyl,
            cylinder_count: cyl_count,
            size_bytes: part_size,
            bootable: (flags & PART_FLAG_BOOTABLE) != 0,
            boot_priority: boot_pri,
            num_buffers,
            block_location: part_ptr,
            next_part_block: next_part,
            checksum_valid: p_cks_valid,
            size_block,
            surfaces,
            blocks_per_track,
            reserved: dos_reserved,
        });

        part_ptr = next_part;
    }

    let free_cylinders = cylinders.saturating_sub(used_cylinders + 2); // 2 cylinders reserved for RDB

    Ok(ParsedRdb {
        rdb_block: rdb_block_idx as u32,
        cylinders,
        sectors,
        heads,
        block_size,
        total_capacity_bytes,
        partitions,
        file_systems,
        free_cylinders,
        checksum_valid,
    })
}

/// Maximum partitions ART will write into one RDB.
///
/// AmigaOS imposes no hard limit, but every partition costs a block inside the
/// reserved area, and a runaway count would otherwise be used to index past it.
pub const MAX_PARTITIONS: usize = 32;

/// Cylinders reserved at the front of the disk for the RDB itself.
const RESERVED_CYLINDERS: u32 = 2;

/// The RDB area of a new image, plus the size the geometry implies.
///
/// Only the first few blocks of a hard disk image carry structure; the rest is
/// zero. Returning just those blocks lets the caller create the file sparsely
/// instead of materialising the whole image in memory — a 2 GB HDF used to mean
/// a 2 GB allocation (spec §56: never allocate from an unchecked length).
#[derive(Debug, Clone)]
pub struct RdbLayout {
    /// Bytes to write at offset 0 of the new image.
    pub blocks: Vec<u8>,
    /// Total size of the image the geometry describes.
    pub total_size: u64,
}

/// Build the RDB and partition blocks for a new hard disk image.
pub fn create_rdb_layout(
    total_bytes: u64,
    partitions: &[PartitionSpec],
    file_systems: &[FileSystemSpec],
) -> CoreResult<RdbLayout> {
    if total_bytes < 10 * 1024 * 1024 {
        return Err(CoreError::InvalidInput(
            "Hard disk image size must be at least 10 MB".into(),
        ));
    }
    if partitions.len() > MAX_PARTITIONS {
        return Err(CoreError::InvalidInput(format!(
            "too many partitions ({}, maximum {MAX_PARTITIONS})",
            partitions.len()
        )));
    }

    // Disk geometry: Standard LBA geometry (16 heads, 63 sectors/track = 1008 sectors/cyl = 516,096 bytes/cyl)
    let heads = 16u32;
    let sectors = 63u32;
    let cyl_blocks = heads * sectors;
    let bytes_per_cyl = (cyl_blocks as u64) * (BLOCK_SIZE as u64);
    let cylinders = u32::try_from(total_bytes.div_ceil(bytes_per_cyl)).map_err(|_| {
        CoreError::InvalidInput("Hard disk image size is too large to describe in an RDB".into())
    })?;

    if cylinders < 4 {
        return Err(CoreError::InvalidInput(
            "Image size too small for cylinder layout".into(),
        ));
    }

    // Refuse a layout that cannot hold what was asked for, rather than silently
    // shrinking the last partition to fit (spec §89).
    let usable_cylinders = cylinders - RESERVED_CYLINDERS;
    let requested_cylinders: u64 = partitions
        .iter()
        .map(|p| {
            ((p.size_mb as u64) * 1024 * 1024)
                .div_ceil(bytes_per_cyl)
                .max(1)
        })
        .sum();
    if requested_cylinders > usable_cylinders as u64 {
        let requested_mb = requested_cylinders * bytes_per_cyl / (1024 * 1024);
        let available_mb = (usable_cylinders as u64) * bytes_per_cyl / (1024 * 1024);
        return Err(CoreError::InvalidInput(format!(
            "partitions need {requested_mb} MB but only {available_mb} MB is available on a \
             {} MB disk",
            total_bytes / (1024 * 1024)
        )));
    }

    // The RDSK block, one PART block per partition, then one FSHD block per
    // driver followed immediately by that driver's own LSEG chain. Laying each
    // driver out contiguously after its header is not required by the format
    // — every block carries a `next` pointer — but it keeps the arithmetic
    // below readable and puts a driver's blocks where a person reading a hex
    // dump expects them.
    let fs_blocks: Vec<u32> = file_systems
        .iter()
        .map(|fs| 1 + fs.data.len().div_ceil(LSEG_DATA_BYTES) as u32)
        .collect();
    let structured_blocks = 1 + partitions.len() + fs_blocks.iter().sum::<u32>() as usize;

    // Everything above lives in the reserved area at the front of the disk,
    // before the first partition's cylinder. A driver big enough to run past
    // it would be overwritten by the first partition's own data the moment
    // anything was written there — so it is refused, with the numbers, rather
    // than produced and left to fail on the Amiga.
    let reserved_blocks = (RESERVED_CYLINDERS * cyl_blocks) as usize;
    if structured_blocks > reserved_blocks {
        return Err(CoreError::InvalidInput(format!(
            "the partition table and its {} file system driver(s) need {structured_blocks} \
             blocks, but only {reserved_blocks} are reserved at the front of the disk",
            file_systems.len()
        )));
    }

    let mut image = vec![0u8; structured_blocks * BLOCK_SIZE];

    // 1. Initialize RDSK Block at Block 0.
    //
    // Field offsets follow `struct RigidDiskBlock` (hardblocks.h). Longword
    // indices: 4=BlockBytes, 6=BadBlockList, 7=PartitionList, 8=FileSysHeaderList,
    // 9=DriveInit, 16..18=geometry, 32..38=logical drive.
    let last_rdb_block = structured_blocks as u32 - 1;
    {
        let rdb_slice = &mut image[0..BLOCK_SIZE];
        rdb_slice[0..4].copy_from_slice(&IDNAME_RDSK.to_be_bytes()); // 'RDSK'
        rdb_slice[4..8].copy_from_slice(&64u32.to_be_bytes()); // SummedLongs
        rdb_slice[12..16].copy_from_slice(&7u32.to_be_bytes()); // HostID
        rdb_slice[16..20].copy_from_slice(&(BLOCK_SIZE as u32).to_be_bytes()); // BlockBytes

        // These are block pointers. Zero is a *valid* block number, so "none"
        // has to be written as -1 or AmigaOS will follow them into block 0.
        rdb_slice[24..28].copy_from_slice(&NO_BLOCK.to_be_bytes()); // BadBlockList
        let first_part_block = if partitions.is_empty() { NO_BLOCK } else { 1 };
        rdb_slice[28..32].copy_from_slice(&first_part_block.to_be_bytes()); // PartitionList

        // The drivers begin immediately after the partition blocks. `NO_BLOCK`
        // rather than 0 when there are none: block 0 is the RDSK itself, and
        // AmigaOS would follow a zero straight into it.
        let first_fs_block = if file_systems.is_empty() {
            NO_BLOCK
        } else {
            (1 + partitions.len()) as u32
        };
        rdb_slice[32..36].copy_from_slice(&first_fs_block.to_be_bytes()); // FileSysHeaderList
        rdb_slice[36..40].copy_from_slice(&NO_BLOCK.to_be_bytes()); // DriveInit

        // Physical geometry.
        rdb_slice[64..68].copy_from_slice(&cylinders.to_be_bytes()); // Cylinders
        rdb_slice[68..72].copy_from_slice(&sectors.to_be_bytes()); // Sectors
        rdb_slice[72..76].copy_from_slice(&heads.to_be_bytes()); // Heads
        rdb_slice[76..80].copy_from_slice(&1u32.to_be_bytes()); // Interleave
        rdb_slice[80..84].copy_from_slice(&cylinders.to_be_bytes()); // Park

        // Logical drive characteristics. HiCylinder and CylBlocks were never
        // written before, leaving AmigaOS to read a zero-capacity disk.
        rdb_slice[128..132].copy_from_slice(&0u32.to_be_bytes()); // RDBBlocksLo
        rdb_slice[132..136].copy_from_slice(&last_rdb_block.to_be_bytes()); // RDBBlocksHi
        rdb_slice[136..140].copy_from_slice(&RESERVED_CYLINDERS.to_be_bytes()); // LoCylinder
        rdb_slice[140..144].copy_from_slice(&(cylinders - 1).to_be_bytes()); // HiCylinder
        rdb_slice[144..148].copy_from_slice(&cyl_blocks.to_be_bytes()); // CylBlocks
        rdb_slice[152..156].copy_from_slice(&last_rdb_block.to_be_bytes()); // HighRDSKBlock

        // Compute RDSK checksum at offset 8
        let rdb_cks = compute_rdb_checksum(rdb_slice);
        rdb_slice[8..12].copy_from_slice(&rdb_cks.to_be_bytes());
    }

    // 2. Initialize Partition Blocks (PART)
    //
    // Sizes were validated against the disk above, so no partition can overrun
    // the end or be silently truncated, and the chain never points at a block
    // that was not written.
    let mut current_cyl = RESERVED_CYLINDERS;

    for (idx, spec) in partitions.iter().enumerate() {
        let part_block_num = (1 + idx) as u32;
        let next_part_num = if idx + 1 < partitions.len() {
            (2 + idx) as u32
        } else {
            NO_BLOCK
        };

        let req_bytes = (spec.size_mb as u64) * 1024 * 1024;
        let req_cyls = req_bytes.div_ceil(bytes_per_cyl).max(1) as u32;
        let high_cyl = current_cyl + req_cyls - 1;

        let p_off = (part_block_num as usize) * BLOCK_SIZE;
        let p_slice = &mut image[p_off..p_off + BLOCK_SIZE];

        p_slice[0..4].copy_from_slice(&IDNAME_PART.to_be_bytes()); // 'PART'
        p_slice[4..8].copy_from_slice(&64u32.to_be_bytes()); // size in longwords
        p_slice[16..20].copy_from_slice(&next_part_num.to_be_bytes());

        let flags = if spec.bootable { PART_FLAG_BOOTABLE } else { 0 };
        p_slice[20..24].copy_from_slice(&flags.to_be_bytes());

        // Device name: BCPL string at offset 36
        write_bcpl_string(p_slice, 36, &spec.drive_name, 32);

        // Environment Vector (DosEnvec) starting at offset 128 (LW 32)
        p_slice[128..132].copy_from_slice(&17u32.to_be_bytes()); // TableSize = 17
        p_slice[132..136].copy_from_slice(&(BLOCK_SIZE as u32 / 4).to_be_bytes()); // SizeBlock in longwords (128)
        p_slice[140..144].copy_from_slice(&heads.to_be_bytes());
        p_slice[144..148].copy_from_slice(&1u32.to_be_bytes()); // SectorsPerBlock
        p_slice[148..152].copy_from_slice(&sectors.to_be_bytes()); // BlocksPerTrack
        p_slice[152..156].copy_from_slice(&2u32.to_be_bytes()); // Reserved blocks
        p_slice[164..168].copy_from_slice(&current_cyl.to_be_bytes()); // LowCyl
        p_slice[168..172].copy_from_slice(&high_cyl.to_be_bytes()); // HighCyl
        let buffers = if spec.num_buffers > 0 {
            spec.num_buffers
        } else {
            100
        };
        p_slice[172..176].copy_from_slice(&buffers.to_be_bytes());
        // Mask (LW 46) stays zero; BootPri is LW 47 and DosType LW 48.
        p_slice[188..192].copy_from_slice(&(spec.boot_priority as i32).to_be_bytes());

        // DosType at offset 192 (LW 48)
        let dt = spec.fs_type.to_dostype_u32();
        p_slice[192..196].copy_from_slice(&dt.to_be_bytes());

        // Compute PART checksum at offset 8
        let p_cks = compute_rdb_checksum(p_slice);
        p_slice[8..12].copy_from_slice(&p_cks.to_be_bytes());

        current_cyl = high_cyl + 1;
    }

    // 3. File system drivers: one FSHD, then the LSEG chain carrying the
    //    driver's own bytes.
    //
    //    This is the half of G4 that makes a `PDS` partition mountable at
    //    all. Kickstart has no PFS3; it loads one out of these blocks. A
    //    partition table naming a filesystem the disk does not carry is one
    //    an Amiga ignores in silence — which is what ART produced before this
    //    existed (ART-084), and what `hst-imager` refuses to produce.
    let mut next_free = (1 + partitions.len()) as u32;
    for (idx, fs) in file_systems.iter().enumerate() {
        let fshd_block = next_free;
        let segment_count = fs.data.len().div_ceil(LSEG_DATA_BYTES);
        let first_seg_block = fshd_block + 1;
        let next_fshd = if idx + 1 < file_systems.len() {
            fshd_block + fs_blocks[idx]
        } else {
            NO_BLOCK
        };

        {
            let off = fshd_block as usize * BLOCK_SIZE;
            let block = &mut image[off..off + BLOCK_SIZE];
            block[0..4].copy_from_slice(&IDNAME_FSHD.to_be_bytes());
            block[4..8].copy_from_slice(&64u32.to_be_bytes()); // SummedLongs
            block[12..16].copy_from_slice(&7u32.to_be_bytes()); // HostID
            block[16..20].copy_from_slice(&next_fshd.to_be_bytes()); // Next
            block[20..24].copy_from_slice(&0u32.to_be_bytes()); // Flags
            block[32..36].copy_from_slice(&fs.dos_type.to_be_bytes()); // DosType
            let version = ((fs.version as u32) << 16) | (fs.revision as u32);
            block[36..40].copy_from_slice(&version.to_be_bytes()); // Version

            // PatchFlags says which of the DeviceNode fields below AmigaOS
            // should actually take from here. Bit 4 is `dn_SegListBlock` —
            // the only one that matters, and the only one written: patching
            // a stack size or a priority ART has no opinion about would be
            // overriding the user's mountlist for no reason.
            block[40..44].copy_from_slice(&0x0000_0010u32.to_be_bytes()); // PatchFlags

            // DeviceNode. Everything but the seg list is left zero, except
            // GlobalVec, which must be -1: zero means "this filesystem uses a
            // BCPL global vector", and a modern driver does not.
            let seg_list = if segment_count == 0 {
                NO_BLOCK
            } else {
                first_seg_block
            };
            block[72..76].copy_from_slice(&seg_list.to_be_bytes()); // dn_SegListBlock
            block[76..80].copy_from_slice(&NO_BLOCK.to_be_bytes()); // dn_GlobalVec

            let cks = compute_rdb_checksum(block);
            block[8..12].copy_from_slice(&cks.to_be_bytes());
        }

        for (seg_idx, chunk) in fs.data.chunks(LSEG_DATA_BYTES).enumerate() {
            let seg_block = first_seg_block + seg_idx as u32;
            let next_seg = if seg_idx + 1 < segment_count {
                seg_block + 1
            } else {
                NO_BLOCK
            };

            let off = seg_block as usize * BLOCK_SIZE;
            let block = &mut image[off..off + BLOCK_SIZE];
            block[0..4].copy_from_slice(&IDNAME_LSEG.to_be_bytes());

            // **SummedLongs declares how much of this block is real.** The
            // last block of a chain is nearly always part-full, and writing
            // 128 here would tell every reader the driver is up to 492 bytes
            // longer than it is — the same rule the reader relies on, from
            // the other side. Rounded up to a whole longword, because the
            // field counts longwords and a driver whose length is not a
            // multiple of four still has to arrive whole.
            let longs = 5 + chunk.len().div_ceil(4);
            block[4..8].copy_from_slice(&(longs as u32).to_be_bytes());
            block[12..16].copy_from_slice(&7u32.to_be_bytes()); // HostID
            block[16..20].copy_from_slice(&next_seg.to_be_bytes()); // Next
            block[20..20 + chunk.len()].copy_from_slice(chunk);

            let cks = compute_rdb_checksum(block);
            block[8..12].copy_from_slice(&cks.to_be_bytes());
        }

        next_free += fs_blocks[idx];
    }

    Ok(RdbLayout {
        blocks: image,
        total_size: (cylinders as u64) * bytes_per_cyl,
    })
}

#[cfg(test)]
mod dosenv_layout {
    use super::*;

    /// ART-032. The DosEnvVec field order, pinned against the layout amitools
    /// reads and writes (`fs/block/rdb/PartitionBlock.py`, verified 2026-08-09):
    ///
    /// | longword | field | byte offset |
    /// |---|---|---|
    /// | 45 | MaxTransfer | 180 |
    /// | 46 | Mask | 184 |
    /// | 47 | BootPri | 188 |
    /// | 48 | DosType | 192 |
    ///
    /// ART used to write BootPri at 184 and DosType at 188 — one longword early
    /// — and read them back from the same wrong places. Every ART test passed
    /// because both halves agreed with each other; `rdbtool` reading an
    /// ART-made image is what showed the disk said DosType 0 to the rest of the
    /// world. This test is the reason that cannot come back.
    #[test]
    fn dosenv_offsets_match_the_amiga_layout() {
        let layout = create_rdb_layout(
            32 * 1024 * 1024,
            &[PartitionSpec {
                drive_name: "DH0".into(),
                fs_type: AmigaHardDiskFs::FfsStandard,
                size_mb: 10,
                bootable: true,
                boot_priority: 3,
                num_buffers: 100,
            }],
            &[],
        )
        .unwrap();

        // The PART block follows the RDSK block.
        let part = &layout.blocks[BLOCK_SIZE..BLOCK_SIZE * 2];
        assert_eq!(
            u32::from_be_bytes([part[0], part[1], part[2], part[3]]),
            IDNAME_PART
        );

        let long_at = |offset: usize| {
            u32::from_be_bytes([
                part[offset],
                part[offset + 1],
                part[offset + 2],
                part[offset + 3],
            ])
        };

        // Mask is left alone; BootPri and DosType sit where AmigaOS looks.
        assert_eq!(long_at(184), 0, "longword 46 is Mask, not BootPri");
        assert_eq!(long_at(188), 3, "BootPri belongs at longword 47");
        assert_eq!(
            long_at(192),
            AmigaHardDiskFs::FfsStandard.to_dostype_u32(),
            "DosType belongs at longword 48"
        );

        // TableSize must cover DosType, or an Amiga would ignore it.
        assert!(long_at(128) >= 17, "TableSize must include DosType");
    }

    /// The round trip has to agree with the layout, not merely with itself.
    #[test]
    fn a_partition_reads_back_the_filesystem_it_was_written_with() {
        let layout = create_rdb_layout(
            32 * 1024 * 1024,
            &[PartitionSpec {
                drive_name: "DH0".into(),
                fs_type: AmigaHardDiskFs::FfsStandard,
                size_mb: 10,
                bootable: true,
                boot_priority: -2,
                num_buffers: 100,
            }],
            &[],
        )
        .unwrap();

        let parsed = parse_rdb(&layout.blocks).unwrap();
        let partition = &parsed.partitions[0];

        assert_eq!(
            partition.dostype,
            AmigaHardDiskFs::FfsStandard.to_dostype_u32()
        );
        assert_eq!(partition.fs_type, AmigaHardDiskFs::FfsStandard);
        assert_eq!(partition.boot_priority, -2, "a negative priority survives");
        assert_eq!(partition.size_block, 128, "SizeBlock counts longwords");
        assert_eq!(partition.reserved, 2);
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_driver_states_its_own_version() {
        // The real shape, from `pfs3aio`.
        let mut data = vec![0u8; 64];
        data.extend_from_slice(b"$VER: pfs3aio 19.2 (2.10.18)\0");
        data.extend_from_slice(&[0u8; 32]);
        assert_eq!(version_from_ver_string(&data), Some((19, 2)));
    }

    #[test]
    fn a_dot_in_the_program_name_is_not_mistaken_for_a_version() {
        let data = b"$VER: pfs3aio.dev 1.4 (1.1.99)".to_vec();
        assert_eq!(version_from_ver_string(&data), Some((1, 4)));
    }

    #[test]
    fn a_driver_that_says_nothing_says_nothing() {
        // `None`, not `(0, 0)`: 0.0 loses to whatever AmigaOS already has
        // loaded, so guessing it would produce a disk that quietly does not
        // use the driver it carries. The caller has to ask instead.
        assert_eq!(version_from_ver_string(&[0u8; 4096]), None);
        assert_eq!(version_from_ver_string(b"$VER: pfs3aio"), None);
        assert_eq!(version_from_ver_string(b"$VER: pfs3aio v.x"), None);
    }

    #[test]
    fn a_version_too_big_for_the_field_is_passed_over_not_truncated() {
        // 70000 does not fit a u16. Taking it modulo 65536 would give 4464 —
        // a number the driver never claimed.
        let data = b"$VER: thing 70000.1 real 3.5".to_vec();
        assert_eq!(version_from_ver_string(&data), Some((3, 5)));
    }

    #[test]
    fn the_search_does_not_run_off_the_end_of_a_short_file() {
        // Every length from nothing to past the marker, so a bound that is
        // wrong by one shows up as a panic rather than as a wrong answer.
        let full = b"$VER: x 1.2";
        for len in 0..=full.len() {
            let _ = version_from_ver_string(&full[..len]);
        }
    }

    /// Round-trip: a driver written into an RDB comes back out measured
    /// exactly, through ART's own reader.
    ///
    /// The two halves of G4 checking each other is worth something but not
    /// everything — a writer and a reader that share a mistake agree with each
    /// other and with nothing else, which is precisely how ART-032…035 and
    /// ART-079 shipped. `write_rdb_with_driver_for_oracle_when_asked` below is
    /// the half that answers to somebody outside.
    #[test]
    fn a_written_driver_reads_back_with_its_version_and_exact_size() {
        // 1000 bytes: two full LSEG blocks and one holding 16 — a part-full
        // last block, which is the case a fixed 492 would get wrong.
        let driver: Vec<u8> = (0..1000u32).map(|i| (i % 251) as u8).collect();
        let layout = create_rdb_layout(
            64 * 1024 * 1024,
            &[PartitionSpec {
                drive_name: "DH0".into(),
                fs_type: AmigaHardDiskFs::Pfs3DirectScsi,
                size_mb: 32,
                bootable: true,
                boot_priority: 0,
                num_buffers: 30,
            }],
            &[FileSystemSpec {
                dos_type: 0x5044_5303,
                version: 19,
                revision: 2,
                data: driver.clone(),
            }],
        )
        .unwrap();

        let parsed = parse_rdb(&layout.blocks).unwrap();
        assert_eq!(parsed.file_systems.len(), 1);
        let fs = &parsed.file_systems[0];
        assert_eq!(fs.dos_type_str, "PDS3");
        assert_eq!((fs.version, fs.revision), (19, 2));
        assert_eq!(fs.segment_blocks, 3);
        assert_eq!(fs.size_bytes, driver.len() as u64);
        assert!(!fs.truncated);
        assert!(fs.checksum_valid);
        // The whole point of writing it: the partition's DosType is now
        // provided by the disk it sits on.
        assert!(parsed.provides_file_system(0x5044_5303));
    }

    #[test]
    fn the_drivers_bytes_survive_the_trip_verbatim() {
        // A driver ART does not understand must not be silently altered — so
        // this checks the bytes themselves, not just the length.
        let driver: Vec<u8> = (0..2000u32).map(|i| (i * 7 % 256) as u8).collect();
        let layout = create_rdb_layout(
            64 * 1024 * 1024,
            &[],
            &[FileSystemSpec {
                dos_type: 0x5044_5303,
                version: 19,
                revision: 2,
                data: driver.clone(),
            }],
        )
        .unwrap();

        // Walk the chain by hand and reassemble, exactly as Kickstart would.
        let parsed = parse_rdb(&layout.blocks).unwrap();
        let mut block = parsed.file_systems[0].seg_list_block;
        let mut rebuilt = Vec::new();
        while block != 0 && block != NO_BLOCK {
            let off = block as usize * BLOCK_SIZE;
            let slice = &layout.blocks[off..off + BLOCK_SIZE];
            let longs = summed_longs(slice);
            let bytes = (longs - 5) * 4;
            rebuilt.extend_from_slice(&slice[20..20 + bytes]);
            block = u32::from_be_bytes([slice[16], slice[17], slice[18], slice[19]]);
        }
        // The last block is padded up to a whole longword; the driver itself
        // is the prefix.
        assert!(rebuilt.len() >= driver.len());
        assert_eq!(&rebuilt[..driver.len()], &driver[..]);
    }

    #[test]
    fn a_disk_with_no_drivers_says_so_rather_than_pointing_at_block_zero() {
        // `NO_BLOCK`, not 0: block 0 is the RDSK itself, and AmigaOS would
        // follow a zero straight into it.
        let layout = create_rdb_layout(64 * 1024 * 1024, &[], &[]).unwrap();
        let list = u32::from_be_bytes([
            layout.blocks[32],
            layout.blocks[33],
            layout.blocks[34],
            layout.blocks[35],
        ]);
        assert_eq!(list, NO_BLOCK);
        assert!(parse_rdb(&layout.blocks).unwrap().file_systems.is_empty());
    }

    #[test]
    fn a_driver_too_big_for_the_reserved_area_is_refused_with_the_numbers() {
        // It would be overwritten by the first partition's own data the
        // moment anything was written there. Refused before the image exists,
        // rather than produced and left to fail on the Amiga.
        let huge = vec![0u8; 3 * 1024 * 1024];
        let err = create_rdb_layout(
            64 * 1024 * 1024,
            &[],
            &[FileSystemSpec {
                dos_type: 0x5044_5303,
                version: 1,
                revision: 0,
                data: huge,
            }],
        )
        .unwrap_err();
        let message = err.to_string();
        assert!(message.contains("blocks"), "{message}");
    }

    #[test]
    fn two_drivers_are_chained_and_both_come_back() {
        let layout = create_rdb_layout(
            64 * 1024 * 1024,
            &[],
            &[
                FileSystemSpec {
                    dos_type: 0x5044_5303,
                    version: 19,
                    revision: 2,
                    data: vec![1u8; 600],
                },
                FileSystemSpec {
                    dos_type: 0x5346_5300,
                    version: 1,
                    revision: 84,
                    data: vec![2u8; 100],
                },
            ],
        )
        .unwrap();

        let parsed = parse_rdb(&layout.blocks).unwrap();
        let seen: Vec<(&str, u16, u16)> = parsed
            .file_systems
            .iter()
            .map(|fs| (fs.dos_type_str.as_str(), fs.version, fs.revision))
            .collect();
        assert_eq!(seen, vec![("PDS3", 19, 2), ("SFS0", 1, 84)]);
        assert_eq!(parsed.file_systems[0].segment_blocks, 2);
        assert_eq!(parsed.file_systems[1].segment_blocks, 1);
    }

    /// Write an RDB with a **real** driver and leave it for `rdbtool` to judge.
    ///
    /// The other direction of the oracle: `read_foreign_rdb_*` proves ART can
    /// read what `hst-imager` wrote; this produces something for an
    /// implementation outside ART to read back. Both halves are needed, and
    /// this project has the scars to say why (ART-032…035, ART-075, ART-079).
    ///
    /// ```text
    /// ART_FS_DRIVER_IN=... ART_RDB_WRITE_OUT=... cargo test write_rdb_with_driver_for_oracle_when_asked -- --nocapture
    /// ```
    #[test]
    fn write_rdb_with_driver_for_oracle_when_asked() {
        let (Ok(driver_path), Ok(out)) = (
            std::env::var("ART_FS_DRIVER_IN"),
            std::env::var("ART_RDB_WRITE_OUT"),
        ) else {
            return;
        };

        let data = std::fs::read(&driver_path).unwrap();

        // Read the version out of the driver, the way the command does. The
        // synthetic `$VER:` tests above pin the parser against strings this
        // file wrote; this one runs it over a binary nobody here made.
        let (version, revision) = version_from_ver_string(&data)
            .expect("the driver states no version — pass one explicitly");
        println!(
            "driver={} bytes={} version={version}.{revision}",
            driver_path,
            data.len()
        );

        let layout = create_rdb_layout(
            64 * 1024 * 1024,
            &[PartitionSpec {
                drive_name: "DH0".into(),
                fs_type: AmigaHardDiskFs::Pfs3DirectScsi,
                size_mb: 50,
                bootable: true,
                boot_priority: 0,
                num_buffers: 30,
            }],
            &[FileSystemSpec {
                dos_type: 0x5044_5303,
                version,
                revision,
                data,
            }],
        )
        .unwrap();

        // Sparse, the way `create_hdf` does it: the structured blocks, then
        // the file extended to its full length.
        let mut file = std::fs::File::create(&out).unwrap();
        std::io::Write::write_all(&mut file, &layout.blocks).unwrap();
        file.set_len(layout.total_size).unwrap();
        println!("wrote={out} total={} bytes", layout.total_size);
    }

    // ---- FSHD / LSEG reading (G4's reading half) ------------------------
    //
    // Synthetic and built here at runtime, per the project's rule: ART ships
    // no copyrighted Amiga content, so the fixtures are the smallest blocks
    // that carry the shape being tested. The real thing — an RDB `hst-imager`
    // built, with `pfs3aio` in it — is checked by
    // `read_foreign_rdb_for_oracle_when_asked` above, against `rdbtool`.

    /// A one-megabyte image with an `RDSK` at block 0 and nothing else.
    fn blank_image_with_rdsk(fs_list: u32, part_list: u32) -> Vec<u8> {
        let mut image = vec![0u8; 64 * BLOCK_SIZE];
        image[0..4].copy_from_slice(&IDNAME_RDSK.to_be_bytes());
        image[4..8].copy_from_slice(&64u32.to_be_bytes()); // SummedLongs
        image[28..32].copy_from_slice(&part_list.to_be_bytes());
        image[32..36].copy_from_slice(&fs_list.to_be_bytes());
        image[64..68].copy_from_slice(&10u32.to_be_bytes()); // cylinders
        image[68..72].copy_from_slice(&32u32.to_be_bytes()); // sectors
        image[72..76].copy_from_slice(&2u32.to_be_bytes()); // heads
        image
    }

    /// Write an `FSHD` at `block`, pointing at `seg_list` and `next`.
    fn put_fshd(
        image: &mut [u8],
        block: u32,
        dos_type: u32,
        version: u32,
        seg_list: u32,
        next: u32,
    ) {
        let off = block as usize * BLOCK_SIZE;
        image[off..off + 4].copy_from_slice(&IDNAME_FSHD.to_be_bytes());
        image[off + 4..off + 8].copy_from_slice(&64u32.to_be_bytes());
        image[off + 16..off + 20].copy_from_slice(&next.to_be_bytes());
        image[off + 32..off + 36].copy_from_slice(&dos_type.to_be_bytes());
        image[off + 36..off + 40].copy_from_slice(&version.to_be_bytes());
        image[off + 72..off + 76].copy_from_slice(&seg_list.to_be_bytes());
    }

    /// Write an `LSEG` at `block` declaring `longs` summed longwords.
    fn put_lseg(image: &mut [u8], block: u32, longs: u32, next: u32) {
        let off = block as usize * BLOCK_SIZE;
        image[off..off + 4].copy_from_slice(&IDNAME_LSEG.to_be_bytes());
        image[off + 4..off + 8].copy_from_slice(&longs.to_be_bytes());
        image[off + 16..off + 20].copy_from_slice(&next.to_be_bytes());
    }

    #[test]
    fn an_rdb_with_no_file_systems_reports_none() {
        // The normal, correct state for a disk that only uses filesystems
        // Kickstart already has. Empty must not read as "something went
        // wrong".
        let image = blank_image_with_rdsk(0, 0);
        let parsed = parse_rdb(&image).unwrap();
        assert!(parsed.file_systems.is_empty());
        assert!(!parsed.provides_file_system(0x5044_5303));
    }

    #[test]
    fn a_file_system_is_read_with_its_version_and_measured_size() {
        let mut image = blank_image_with_rdsk(1, 0);
        put_fshd(&mut image, 1, 0x5044_5303, 0x0013_0002, 2, 0);
        // Two full blocks and one part-full: 492 + 492 + 80.
        put_lseg(&mut image, 2, 128, 3);
        put_lseg(&mut image, 3, 128, 4);
        put_lseg(&mut image, 4, 25, 0);

        let parsed = parse_rdb(&image).unwrap();
        assert_eq!(parsed.file_systems.len(), 1);
        let fs = &parsed.file_systems[0];
        assert_eq!(fs.dos_type_str, "PDS3");
        assert_eq!((fs.version, fs.revision), (19, 2));
        assert_eq!(fs.segment_blocks, 3);
        // **From each block's own SummedLongs, not from a fixed 492.** Three
        // blocks counted as full would be 1476; the real answer is 1064, and
        // getting this wrong overstates every driver by up to half a
        // kilobyte. `pfs3aio` measured this way comes to 59,120 bytes, which
        // is exactly what `rdbtool` independently reports.
        assert_eq!(fs.size_bytes, 492 + 492 + 80);
        assert!(!fs.truncated);
        assert!(parsed.provides_file_system(0x5044_5303));
    }

    #[test]
    fn several_file_systems_are_read_in_chain_order() {
        let mut image = blank_image_with_rdsk(1, 0);
        put_fshd(&mut image, 1, 0x5044_5303, 0x0013_0002, 3, 2);
        put_fshd(&mut image, 2, 0x5346_5300, 0x0001_0000, 4, 0);
        put_lseg(&mut image, 3, 25, 0);
        put_lseg(&mut image, 4, 25, 0);

        let parsed = parse_rdb(&image).unwrap();
        let types: Vec<&str> = parsed
            .file_systems
            .iter()
            .map(|fs| fs.dos_type_str.as_str())
            .collect();
        assert_eq!(types, vec!["PDS3", "SFS0"]);
    }

    #[test]
    fn a_segment_chain_that_loops_is_marked_rather_than_followed_forever() {
        // These pointers come out of a file ART did not write. A chain that
        // points at itself must cost a loop iteration, not the application.
        let mut image = blank_image_with_rdsk(1, 0);
        put_fshd(&mut image, 1, 0x5044_5303, 0x0013_0002, 2, 0);
        put_lseg(&mut image, 2, 128, 3);
        put_lseg(&mut image, 3, 128, 2); // back to 2

        let parsed = parse_rdb(&image).unwrap();
        let fs = &parsed.file_systems[0];
        assert!(fs.truncated);
        // What it did manage to walk is still reported: "there is a driver
        // here and ART could not measure it" beats silence.
        assert_eq!(fs.segment_blocks, 2);
        assert!(parsed.provides_file_system(0x5044_5303));
    }

    #[test]
    fn a_segment_pointer_past_the_end_is_refused_not_indexed() {
        let mut image = blank_image_with_rdsk(1, 0);
        put_fshd(&mut image, 1, 0x5044_5303, 0x0013_0002, 9_000_000, 0);

        let parsed = parse_rdb(&image).unwrap();
        assert!(parsed.file_systems[0].truncated);
        assert_eq!(parsed.file_systems[0].segment_blocks, 0);
    }

    #[test]
    fn a_file_system_chain_that_loops_stops() {
        let mut image = blank_image_with_rdsk(1, 0);
        put_fshd(&mut image, 1, 0x5044_5303, 0x0013_0002, 0, 2);
        put_fshd(&mut image, 2, 0x4453_0003, 0x0001_0000, 0, 1); // back to 1

        let parsed = parse_rdb(&image).unwrap();
        assert_eq!(parsed.file_systems.len(), 2);
    }

    #[test]
    fn a_block_that_is_not_an_fshd_ends_the_list() {
        // A pointer into the middle of a partition, say. Stop; do not
        // interpret whatever happens to be there as a driver.
        let image = blank_image_with_rdsk(5, 0);
        let parsed = parse_rdb(&image).unwrap();
        assert!(parsed.file_systems.is_empty());
    }

    /// Read an RDB **ART did not write**, and print what it found.
    ///
    /// The third time this shape of hook has been needed, and the reason is
    /// always the same: ART's reader and ART's writer can agree with each
    /// other and with nothing else (ART-032…035, ART-075, ART-079). An RDB
    /// built by `hst-imager` — which is what both existing PiStorm imagers
    /// stand on — is the one ART has to be able to read before SD-1 can write
    /// one.
    ///
    /// It prints the **file system entries** as well as the partitions,
    /// because that is the part ART does not write yet and the part the whole
    /// PiStorm build depends on (G4, and ART-084): a PDS3 partition with no
    /// FSHD/LSEG behind it is one an Amiga cannot mount, and `hst-imager`
    /// refuses to create one at all.
    ///
    /// ```text
    /// ART_RDB_READ_IN=F:\art-sd0\sd0-test.img cargo test read_foreign_rdb_for_oracle_when_asked -- --nocapture
    /// ```
    #[test]
    fn read_foreign_rdb_for_oracle_when_asked() {
        let Ok(source) = std::env::var("ART_RDB_READ_IN") else {
            return;
        };
        // The same window `open_hdf` reads, so this exercises the path the
        // application actually takes rather than a convenient shortcut.
        let bytes = std::fs::read(&source).unwrap();
        let window = &bytes[..bytes.len().min(1024 * 1024)];

        let at = find_rdb_location(window);
        println!("rdb_at={at:?}");

        let parsed = parse_rdb(window).unwrap();
        println!("checksum_valid={}", parsed.checksum_valid);
        println!(
            "geometry cyls={} heads={} sectors={} block_size={}",
            parsed.cylinders, parsed.heads, parsed.sectors, parsed.block_size
        );
        // The half rdbtool reports and ART used to be blind to.
        println!("file_systems={}", parsed.file_systems.len());
        for fs in &parsed.file_systems {
            println!(
                "  {} ({:#010x}) version={}.{} size={} seg_list_blk={:#x} blocks={} checksum_ok={} truncated={}",
                fs.dos_type_str,
                fs.dos_type,
                fs.version,
                fs.revision,
                fs.size_bytes,
                fs.seg_list_block,
                fs.segment_blocks,
                fs.checksum_valid,
                fs.truncated,
            );
        }
        println!("partitions={}", parsed.partitions.len());
        for part in &parsed.partitions {
            println!(
                "  {} dostype={} ({:#010x}) fs={:?} bootable={} cyls={}..{} bytes={}",
                part.drive_name,
                part.dostype_str,
                part.dostype,
                part.fs_type,
                part.bootable,
                part.low_cyl,
                part.high_cyl,
                part.size_bytes,
            );
            println!(
                "    driver present in RDB: {}",
                parsed.provides_file_system(part.dostype)
            );
        }
    }

    use super::*;

    #[test]
    fn create_and_parse_rdb_image_with_pfs3_and_ffs() {
        let partitions = vec![
            PartitionSpec {
                drive_name: "DH0".into(),
                fs_type: AmigaHardDiskFs::Pfs3DirectScsi,
                size_mb: 50,
                bootable: true,
                boot_priority: 5,
                num_buffers: 150,
            },
            PartitionSpec {
                drive_name: "DH1".into(),
                fs_type: AmigaHardDiskFs::FfsDirCache,
                size_mb: 100,
                bootable: false,
                boot_priority: 0,
                num_buffers: 100,
            },
        ];

        let layout = create_rdb_layout(200 * 1024 * 1024, &partitions, &[]).unwrap();
        assert!(layout.total_size >= 200 * 1024 * 1024);
        // Only the RDSK block plus one PART block per partition are materialised.
        assert_eq!(layout.blocks.len(), 3 * BLOCK_SIZE);

        let parsed = parse_rdb(&layout.blocks).unwrap();
        assert!(parsed.checksum_valid);
        assert_eq!(parsed.partitions.len(), 2);

        // Verify DH0
        assert_eq!(parsed.partitions[0].drive_name, "DH0");
        assert_eq!(
            parsed.partitions[0].fs_type,
            AmigaHardDiskFs::Pfs3DirectScsi
        );
        assert!(parsed.partitions[0].bootable);
        assert_eq!(parsed.partitions[0].boot_priority, 5);
        assert!(parsed.partitions[0].checksum_valid);

        // Verify DH1
        assert_eq!(parsed.partitions[1].drive_name, "DH1");
        assert_eq!(parsed.partitions[1].fs_type, AmigaHardDiskFs::FfsDirCache);
        assert!(!parsed.partitions[1].bootable);
        assert!(parsed.partitions[1].checksum_valid);
    }

    fn lw(block: &[u8], index: usize) -> u32 {
        let off = index * 4;
        u32::from_be_bytes([block[off], block[off + 1], block[off + 2], block[off + 3]])
    }

    /// The RDSK block's logical-drive fields decide what capacity AmigaOS sees.
    /// HiCylinder and CylBlocks were previously never written, so a disk ART
    /// created reported zero usable geometry.
    #[test]
    fn rdsk_logical_drive_fields_are_written() {
        let specs = vec![PartitionSpec {
            drive_name: "DH0".into(),
            fs_type: AmigaHardDiskFs::FfsDirCache,
            size_mb: 20,
            bootable: true,
            boot_priority: 0,
            num_buffers: 100,
        }];
        let layout = create_rdb_layout(100 * 1024 * 1024, &specs, &[]).unwrap();
        let rdsk = &layout.blocks[0..BLOCK_SIZE];

        let cylinders = lw(rdsk, 16);
        assert!(cylinders >= 4);

        assert_eq!(lw(rdsk, 32), 0, "RDBBlocksLo");
        assert_eq!(lw(rdsk, 33), 1, "RDBBlocksHi = last written RDB block");
        assert_eq!(lw(rdsk, 34), RESERVED_CYLINDERS, "LoCylinder");
        assert_eq!(lw(rdsk, 35), cylinders - 1, "HiCylinder");
        assert_eq!(lw(rdsk, 36), 16 * 63, "CylBlocks = heads × sectors");
    }

    /// Zero is a valid block number, so "no such list" must be written as -1.
    /// Leaving these at zero sends AmigaOS looking into block 0.
    #[test]
    fn rdsk_absent_lists_use_the_no_block_sentinel() {
        let layout = create_rdb_layout(100 * 1024 * 1024, &[], &[]).unwrap();
        let rdsk = &layout.blocks[0..BLOCK_SIZE];

        assert_eq!(lw(rdsk, 4), BLOCK_SIZE as u32, "BlockBytes must be 512");
        assert_eq!(lw(rdsk, 6), NO_BLOCK, "BadBlockList");
        assert_eq!(lw(rdsk, 7), NO_BLOCK, "PartitionList when there are none");
        assert_eq!(lw(rdsk, 8), NO_BLOCK, "FileSysHeaderList");
        assert_eq!(lw(rdsk, 9), NO_BLOCK, "DriveInit");
    }

    /// Partitions that do not fit used to be silently clipped to the end of the
    /// disk, so the user got far less space than they asked for — and the
    /// partition chain could point at a block that was never written.
    #[test]
    fn oversized_partitions_are_refused_not_truncated() {
        let specs = vec![
            PartitionSpec {
                drive_name: "DH0".into(),
                fs_type: AmigaHardDiskFs::FfsDirCache,
                size_mb: 500,
                bootable: true,
                boot_priority: 0,
                num_buffers: 100,
            },
            PartitionSpec {
                drive_name: "DH1".into(),
                fs_type: AmigaHardDiskFs::FfsDirCache,
                size_mb: 500,
                bootable: false,
                boot_priority: 0,
                num_buffers: 100,
            },
        ];

        // 100 MB of disk cannot hold 1000 MB of partitions.
        let err = create_rdb_layout(100 * 1024 * 1024, &specs, &[]).unwrap_err();
        assert!(matches!(err, CoreError::InvalidInput(_)), "got {err:?}");
    }

    #[test]
    fn partitions_do_not_overlap_and_stay_inside_the_disk() {
        let specs: Vec<PartitionSpec> = (0..4)
            .map(|i| PartitionSpec {
                drive_name: format!("DH{i}"),
                fs_type: AmigaHardDiskFs::FfsDirCache,
                size_mb: 30,
                bootable: i == 0,
                boot_priority: 0,
                num_buffers: 100,
            })
            .collect();

        let layout = create_rdb_layout(500 * 1024 * 1024, &specs, &[]).unwrap();
        let parsed = parse_rdb(&layout.blocks).unwrap();
        assert_eq!(parsed.partitions.len(), 4);

        for pair in parsed.partitions.windows(2) {
            assert!(
                pair[0].high_cyl < pair[1].low_cyl,
                "partitions {} and {} overlap",
                pair[0].drive_name,
                pair[1].drive_name
            );
        }
        let last = parsed.partitions.last().unwrap();
        assert!(
            last.high_cyl < parsed.cylinders,
            "last partition overruns the disk"
        );
    }

    #[test]
    fn too_many_partitions_are_refused() {
        let specs: Vec<PartitionSpec> = (0..MAX_PARTITIONS + 1)
            .map(|i| PartitionSpec {
                drive_name: format!("DH{i}"),
                fs_type: AmigaHardDiskFs::FfsDirCache,
                size_mb: 1,
                bootable: false,
                boot_priority: 0,
                num_buffers: 100,
            })
            .collect();

        assert!(create_rdb_layout(4096 * 1024 * 1024u64, &specs, &[]).is_err());
    }

    /// The checksum covers `SummedLongs` longwords, not the whole block.
    /// Summing all 128 would reject real Amiga disks whose later longwords
    /// carry vendor strings.
    #[test]
    fn checksum_honours_summed_longs() {
        let mut block = vec![0u8; BLOCK_SIZE];
        block[0..4].copy_from_slice(&IDNAME_RDSK.to_be_bytes());
        block[4..8].copy_from_slice(&64u32.to_be_bytes()); // SummedLongs = 64
        block[64..68].copy_from_slice(&100u32.to_be_bytes());

        let cks = compute_rdb_checksum(&block);
        block[8..12].copy_from_slice(&cks.to_be_bytes());
        assert!(verify_rdb_block_checksum(&block));

        // Data beyond the summed region must not affect validity — this is
        // where a real disk keeps its vendor and product strings.
        block[300..304].copy_from_slice(&0xDEAD_BEEFu32.to_be_bytes());
        assert!(
            verify_rdb_block_checksum(&block),
            "longwords past SummedLongs must be excluded"
        );
    }

    #[test]
    fn checksum_survives_a_nonsense_summed_longs_field() {
        let mut block = vec![0u8; BLOCK_SIZE];
        block[0..4].copy_from_slice(&IDNAME_RDSK.to_be_bytes());
        // A hostile value that would otherwise index far past the block.
        block[4..8].copy_from_slice(&0xFFFF_FFFFu32.to_be_bytes());

        // Must fall back rather than panic.
        let _ = compute_rdb_checksum(&block);
        let _ = verify_rdb_block_checksum(&block);
    }
}
