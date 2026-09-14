//! Editing an RDB ART did not build, in place (ART-117).
//!
//! **Pure.** Every function takes bytes and returns bytes or a typed refusal;
//! the I/O — reading the range, the backup, the journal — is
//! `core/preload/embed.rs`. So every refusal and every allocation here is a
//! unit test over a byte vector.
//!
//! `core::rdb::parse_rdb` stays lenient on purpose: a card with one bad chain
//! still has partitions worth showing. An editor needs the opposite, and
//! refuses at the first block it cannot account for. Spec:
//! `docs/superpowers/specs/2026-09-14-art-117-rdb-embed-design.md`, decisions
//! 2–5 and 12.

use std::collections::BTreeSet;

use crate::core::error::{PartitionBound, RaiseRefused, RdbEditRefusal};
use crate::core::rdb::{
    build_fshd_block, build_lseg_chain, dos_type_string, verify_rdb_block_checksum, BLOCK_SIZE,
    IDNAME_FSHD, IDNAME_LSEG, IDNAME_PART, IDNAME_RDSK, LSEG_DATA_BYTES, NO_BLOCK,
};

/// What ART reads of an Amiga disk to edit its RDB — the same 8 MiB
/// `core::card` reads (`AREA_WINDOW_BYTES`).
pub const EDIT_WINDOW_BYTES: usize = 8 * 1024 * 1024;

/// RKRM "How A Driver Uses RDB": the RDSK is looked for in blocks 0–15.
const RDB_LOCATION_LIMIT: u32 = 16;
const MAX_PARTS: usize = 64;
const MAX_FSHDS: usize = 32;
const MAX_LSEGS: usize = 4096;

// Longword indices, `hardblocks.h` [1][4].
pub(crate) const NEXT: usize = 4;
pub(crate) const RDSK_FILE_SYS_HEADER_LIST: usize = 8;
pub(crate) const RDSK_RDB_BLOCKS_HI: usize = 33;
pub(crate) const RDSK_HIGH_RDSK_BLOCK: usize = 38;
pub(crate) const FSHD_DOS_TYPE: usize = 8;
pub(crate) const FSHD_VERSION: usize = 9;
pub(crate) const FSHD_SEG_LIST_BLOCKS: usize = 18;

/// Block `n` of `range`, or `None` past its end. The only way this module
/// reaches into bytes read from a file.
pub(crate) fn block_at(range: &[u8], n: u32) -> Option<&[u8]> {
    let start = (n as usize).checked_mul(BLOCK_SIZE)?;
    range.get(start..start.checked_add(BLOCK_SIZE)?)
}

/// Longword `index` of a block; `0` past its end.
pub(crate) fn long(block: &[u8], index: usize) -> u32 {
    let at = index * 4;
    block
        .get(at..at + 4)
        .map(|b| u32::from_be_bytes([b[0], b[1], b[2], b[3]]))
        .unwrap_or(0)
}

/// One block ART will write, whole.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockWrite {
    pub block: u32,
    pub bytes: [u8; BLOCK_SIZE],
}

/// Block `n` of `range` as an owned array, or `None` past its end.
pub(crate) fn block_array(range: &[u8], n: u32) -> Option<[u8; BLOCK_SIZE]> {
    block_at(range, n).and_then(|bytes| bytes.try_into().ok())
}

/// `bytes` with longword `index` set to `value` and the checksum recomputed
/// over the block's own `SummedLongs` (decision 4). Every other byte is kept —
/// a foreign block's name, PatchFlags and tail included.
pub fn with_long(bytes: &[u8; BLOCK_SIZE], index: usize, value: u32) -> [u8; BLOCK_SIZE] {
    debug_assert!(
        index < BLOCK_SIZE / 4 && index != 2,
        "a longword, and not the checksum"
    );
    let mut out = *bytes;
    let at = index * 4;
    out[at..at + 4].copy_from_slice(&value.to_be_bytes());
    let sum = crate::core::rdb::compute_rdb_checksum(&out);
    out[8..12].copy_from_slice(&sum.to_be_bytes());
    out
}

/// The RDSK fields the editor reads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RdskFields {
    pub block: u32,
    pub block_bytes: u32,
    pub bad_block_list: u32,
    pub partition_list: u32,
    pub file_sys_header_list: u32,
    pub drive_init: u32,
    pub cylinders: u32,
    pub sectors: u32,
    pub heads: u32,
    pub rdb_blocks_lo: u32,
    pub rdb_blocks_hi: u32,
    pub lo_cylinder: u32,
    pub cyl_blocks: u32,
    pub high_rdsk_block: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WalkedPart {
    pub block: u32,
    pub low_cyl: u32,
    /// `LowCyl × Surfaces × BlocksPerTrack × SizeBlock×4 / 512`, from the
    /// PART's own envec; `None` when that overflows.
    pub envec_first_block: Option<u64>,
    /// `LowCyl × rdb_Heads × rdb_Sectors`, the RDB's own geometry
    /// (decision 12); `None` when that overflows.
    pub rdb_first_block: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WalkedFshd {
    pub block: u32,
    pub next: u32,
    pub dos_type: u32,
    pub version: u16,
    pub revision: u16,
    pub seg_list: u32,
    pub lseg_blocks: Vec<u32>,
    /// Each LSEG's `(SummedLongs − 5) × 4` data bytes, in order.
    pub payload: Vec<u8>,
}

/// An RDB accounted for block by block.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StrictRdb {
    pub rdsk: RdskFields,
    pub parts: Vec<WalkedPart>,
    /// Chain order.
    pub fshds: Vec<WalkedFshd>,
    pub drive_init: Vec<u32>,
    /// The RDSK, every PART, FSHD and LSEG, and the DriveInit chain.
    pub used: BTreeSet<u32>,
}

impl StrictRdb {
    /// The lowest first block of any partition, from each PART's own envec.
    pub fn first_partition_block(&self) -> Option<u64> {
        self.parts.iter().filter_map(|p| p.envec_first_block).min()
    }

    pub fn highest_used(&self) -> u32 {
        self.used
            .iter()
            .next_back()
            .copied()
            .unwrap_or(self.rdsk.block)
    }
}

fn unaccounted(block: u32, check: impl Into<String>) -> RdbEditRefusal {
    RdbEditRefusal::Unaccounted {
        block,
        check: check.into(),
    }
}

/// Block `n`, checked for its ID, a sane `SummedLongs` and its checksum.
fn checked_block<'a>(
    range: &'a [u8],
    n: u32,
    id: u32,
    name: &str,
    min_longs: u32,
) -> Result<&'a [u8], RdbEditRefusal> {
    let block = block_at(range, n)
        .ok_or_else(|| unaccounted(n, format!("the image ends before block {n}")))?;
    if long(block, 0) != id {
        return Err(unaccounted(
            n,
            format!("block {n} should be {name} and is not"),
        ));
    }
    let longs = long(block, 1);
    if !(min_longs..=128).contains(&longs) {
        return Err(unaccounted(
            n,
            format!("{name} block {n} declares SummedLongs {longs}"),
        ));
    }
    if !verify_rdb_block_checksum(block) {
        return Err(unaccounted(
            n,
            format!("{name} block {n} fails its checksum"),
        ));
    }
    Ok(block)
}

/// A chain pointer: inside the reserved range, and not already in use.
fn claim(
    used: &mut BTreeSet<u32>,
    rdsk: &RdskFields,
    block: u32,
    what: &str,
) -> Result<(), RdbEditRefusal> {
    if !(rdsk.rdb_blocks_lo..=rdsk.rdb_blocks_hi).contains(&block) {
        return Err(unaccounted(
            block,
            format!(
                "{what} points outside RDBBlocksLo {}..RDBBlocksHi {}",
                rdsk.rdb_blocks_lo, rdsk.rdb_blocks_hi
            ),
        ));
    }
    if !used.insert(block) {
        return Err(unaccounted(
            block,
            format!("{what} points at block {block}, which is already part of the RDB"),
        ));
    }
    Ok(())
}

fn walk_lsegs(
    range: &[u8],
    used: &mut BTreeSet<u32>,
    rdsk: &RdskFields,
    first: u32,
    owner: &str,
) -> Result<(Vec<u32>, Vec<u8>), RdbEditRefusal> {
    let mut blocks = Vec::new();
    let mut payload = Vec::new();
    let mut next = first;
    while next != NO_BLOCK {
        if blocks.len() == MAX_LSEGS {
            return Err(unaccounted(
                next,
                format!("{owner} is longer than {MAX_LSEGS} blocks"),
            ));
        }
        claim(used, rdsk, next, owner)?;
        let block = checked_block(range, next, IDNAME_LSEG, "LSEG", 5)?;
        let longs = long(block, 1) as usize;
        let data = block.get(20..longs * 4).ok_or_else(|| {
            unaccounted(next, format!("LSEG block {next} is shorter than it says"))
        })?;
        payload.extend_from_slice(data);
        blocks.push(next);
        next = long(block, NEXT);
    }
    Ok((blocks, payload))
}

/// Walk an RDB strictly over `range` — area block 0 onwards, at least up to
/// `RDBBlocksHi` (decision 5).
pub fn walk_strict(range: &[u8]) -> Result<StrictRdb, RdbEditRefusal> {
    let found: Vec<u32> = (0..RDB_LOCATION_LIMIT)
        .filter(|&n| block_at(range, n).is_some_and(|b| long(b, 0) == IDNAME_RDSK))
        .collect();
    let rdsk_block = match found.as_slice() {
        [one] => *one,
        [] => {
            return Err(unaccounted(
                0,
                "no block carries the ID RDSK in blocks 0–15",
            ))
        }
        [first, second, ..] => {
            return Err(unaccounted(
                *second,
                format!("blocks {first} and {second} both carry the ID RDSK"),
            ))
        }
    };
    let rdsk = checked_block(range, rdsk_block, IDNAME_RDSK, "RDSK", 1)?;
    let fields = RdskFields {
        block: rdsk_block,
        block_bytes: long(rdsk, 4),
        bad_block_list: long(rdsk, 6),
        partition_list: long(rdsk, 7),
        file_sys_header_list: long(rdsk, RDSK_FILE_SYS_HEADER_LIST),
        drive_init: long(rdsk, 9),
        cylinders: long(rdsk, 16),
        sectors: long(rdsk, 17),
        heads: long(rdsk, 18),
        rdb_blocks_lo: long(rdsk, 32),
        rdb_blocks_hi: long(rdsk, RDSK_RDB_BLOCKS_HI),
        lo_cylinder: long(rdsk, 34),
        cyl_blocks: long(rdsk, 36),
        high_rdsk_block: long(rdsk, RDSK_HIGH_RDSK_BLOCK),
    };

    if fields.block_bytes != BLOCK_SIZE as u32 {
        return Err(unaccounted(
            rdsk_block,
            format!(
                "rdb_BlockBytes is {}; ART edits only 512-byte blocks",
                fields.block_bytes
            ),
        ));
    }
    if fields.bad_block_list != NO_BLOCK {
        return Err(RdbEditRefusal::BadBlocks {
            first: fields.bad_block_list,
        });
    }
    if fields.rdb_blocks_lo > fields.rdb_blocks_hi {
        return Err(unaccounted(
            rdsk_block,
            format!(
                "RDBBlocksLo {} is above RDBBlocksHi {}",
                fields.rdb_blocks_lo, fields.rdb_blocks_hi
            ),
        ));
    }
    if rdsk_block < fields.rdb_blocks_lo {
        return Err(unaccounted(
            rdsk_block,
            format!(
                "the RDSK at block {rdsk_block} lies below RDBBlocksLo {}",
                fields.rdb_blocks_lo
            ),
        ));
    }
    if fields.high_rdsk_block > fields.rdb_blocks_hi {
        return Err(unaccounted(
            rdsk_block,
            format!(
                "HighRDSKBlock {} is above RDBBlocksHi {}",
                fields.high_rdsk_block, fields.rdb_blocks_hi
            ),
        ));
    }
    let window_blocks = (EDIT_WINDOW_BYTES / BLOCK_SIZE) as u64;
    if u64::from(fields.rdb_blocks_hi) + 1 > window_blocks {
        return Err(unaccounted(
            rdsk_block,
            format!(
                "RDBBlocksHi {} reaches past the {window_blocks} blocks ART reads",
                fields.rdb_blocks_hi
            ),
        ));
    }
    if (fields.rdb_blocks_hi as usize + 1) * BLOCK_SIZE > range.len() {
        return Err(unaccounted(
            fields.rdb_blocks_hi,
            format!("the image ends before RDBBlocksHi {}", fields.rdb_blocks_hi),
        ));
    }

    let mut used = BTreeSet::new();
    used.insert(rdsk_block);

    let mut parts = Vec::new();
    let mut next = fields.partition_list;
    while next != NO_BLOCK {
        if parts.len() == MAX_PARTS {
            return Err(unaccounted(
                next,
                format!("the partition list is longer than {MAX_PARTS}"),
            ));
        }
        claim(&mut used, &fields, next, "the partition list")?;
        let block = checked_block(range, next, IDNAME_PART, "PART", 1)?;
        let low_cyl = long(block, 41);
        let envec_first_block = u64::from(low_cyl)
            .checked_mul(u64::from(long(block, 35)))
            .and_then(|v| v.checked_mul(u64::from(long(block, 37))))
            .and_then(|v| v.checked_mul(u64::from(long(block, 33)) * 4))
            .map(|bytes| bytes / BLOCK_SIZE as u64);
        let rdb_first_block = u64::from(low_cyl)
            .checked_mul(u64::from(fields.heads))
            .and_then(|v| v.checked_mul(u64::from(fields.sectors)));
        parts.push(WalkedPart {
            block: next,
            low_cyl,
            envec_first_block,
            rdb_first_block,
        });
        next = long(block, NEXT);
    }

    if let Some(first) = parts.iter().filter_map(|p| p.envec_first_block).min() {
        if u64::from(fields.rdb_blocks_hi) >= first {
            return Err(unaccounted(
                fields.rdb_blocks_hi,
                format!(
                    "RDBBlocksHi {} reaches the first partition, which begins at block {first}",
                    fields.rdb_blocks_hi
                ),
            ));
        }
    }

    let mut fshds = Vec::new();
    let mut next = fields.file_sys_header_list;
    while next != NO_BLOCK {
        if fshds.len() == MAX_FSHDS {
            return Err(unaccounted(
                next,
                format!("the filesystem list is longer than {MAX_FSHDS}"),
            ));
        }
        claim(&mut used, &fields, next, "the filesystem list")?;
        let block = checked_block(range, next, IDNAME_FSHD, "FSHD", 1)?;
        let version = long(block, FSHD_VERSION);
        let seg_list = long(block, FSHD_SEG_LIST_BLOCKS);
        let owner = format!(
            "the {} driver at FSHD {next}",
            dos_type_string(long(block, FSHD_DOS_TYPE))
        );
        let (lseg_blocks, payload) = walk_lsegs(range, &mut used, &fields, seg_list, &owner)?;
        fshds.push(WalkedFshd {
            block: next,
            next: long(block, NEXT),
            dos_type: long(block, FSHD_DOS_TYPE),
            version: (version >> 16) as u16,
            revision: (version & 0xFFFF) as u16,
            seg_list,
            lseg_blocks,
            payload,
        });
        next = long(block, NEXT);
    }

    let drive_init = if fields.drive_init == NO_BLOCK {
        Vec::new()
    } else {
        walk_lsegs(
            range,
            &mut used,
            &fields,
            fields.drive_init,
            "rdb_DriveInit",
        )?
        .0
    };

    Ok(StrictRdb {
        rdsk: fields,
        parts,
        fshds,
        drive_init,
        used,
    })
}

/// Where an edit's blocks go (decisions 2 and 12).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Allocation {
    /// `k`: the FSHD.
    pub fshd_block: u32,
    /// `k + n`: the last LSEG, and the new `HighRDSKBlock`.
    pub last_block: u32,
    /// `n`.
    pub lseg_count: u32,
    /// `RDBBlocksHi` as the edit leaves it.
    pub rdb_blocks_hi: u32,
    /// The value it had, when decision 12 raised it.
    pub raised_from: Option<u32>,
}

pub fn lseg_blocks_for(len: usize) -> u32 {
    len.div_ceil(LSEG_DATA_BYTES) as u32
}

/// Choose `k..=k+n` contiguously above everything live, and raise
/// `RDBBlocksHi` over zero blocks when the reserved range is too small.
pub fn allocate(
    range: &[u8],
    walk: &StrictRdb,
    driver_len: usize,
) -> Result<Allocation, RdbEditRefusal> {
    let rdsk = &walk.rdsk;
    // `walk_strict` keeps every used block and `HighRDSKBlock` ≤ `RDBBlocksHi`
    // < the 8 MiB window, so neither sum below can overflow.
    let start = rdsk.high_rdsk_block.max(walk.highest_used()) + 1;
    let lseg_count = lseg_blocks_for(driver_len);
    let last = start + lseg_count;
    let envec_bound = walk.first_partition_block();
    let below = |end: u32, bound: Option<u64>| bound.is_none_or(|first| u64::from(end) < first);

    if last <= rdsk.rdb_blocks_hi && below(last, envec_bound) {
        return Ok(Allocation {
            fshd_block: start,
            last_block: last,
            lseg_count,
            rdb_blocks_hi: rdsk.rdb_blocks_hi,
            raised_from: None,
        });
    }

    let refuse = |raise: RaiseRefused| RdbEditRefusal::NoRoom {
        needed: lseg_count + 1,
        start,
        free: (rdsk.rdb_blocks_hi + 1).saturating_sub(start),
        rdb_blocks_hi: rdsk.rdb_blocks_hi,
        partition_block: envec_bound,
        raise,
    };

    // Decision 12, in this order: the window, the partitionable area, and
    // only then the bytes — so no block past the window is ever looked for.
    let window_blocks = (EDIT_WINDOW_BYTES / BLOCK_SIZE) as u32;
    if last >= window_blocks {
        return Err(refuse(RaiseRefused::PastWindow { window_blocks }));
    }
    let rdb_geometry_bound = walk.parts.iter().filter_map(|p| p.rdb_first_block).min();
    let lo_cylinder_bound = u64::from(rdsk.lo_cylinder).checked_mul(u64::from(rdsk.cyl_blocks));
    // The lowest bound and which one it is, so the sentence can name it. On a
    // tie the first listed wins: `min_by_key` keeps the first minimum.
    let lowest = [
        (envec_bound, PartitionBound::PartitionEnvec),
        (rdb_geometry_bound, PartitionBound::RdbGeometry),
        (lo_cylinder_bound, PartitionBound::LoCylinder),
    ]
    .into_iter()
    .filter_map(|(limit, bound)| limit.map(|limit| (limit, bound)))
    .min_by_key(|(limit, _)| *limit);
    if let Some((limit, bound)) = lowest {
        if u64::from(last) >= limit {
            return Err(refuse(RaiseRefused::PastPartitionArea { limit, bound }));
        }
    }
    for block in (rdsk.rdb_blocks_hi + 1)..=last {
        match block_at(range, block) {
            Some(bytes) if bytes.iter().all(|b| *b == 0) => {}
            Some(_) => return Err(refuse(RaiseRefused::NonZeroBlock(block))),
            None => {
                return Err(unaccounted(
                    block,
                    format!("the image ends before block {block}"),
                ))
            }
        }
    }

    Ok(Allocation {
        fshd_block: start,
        last_block: last,
        lseg_count,
        rdb_blocks_hi: last.max(rdsk.rdb_blocks_hi),
        raised_from: (last > rdsk.rdb_blocks_hi).then_some(rdsk.rdb_blocks_hi),
    })
}

/// The first longword of every AmigaDOS executable.
pub const HUNK_HEADER: u32 = 0x0000_03F3;

/// hst-imager's per-driver cap (`RdbFsAddCommand.cs:87-91` [5]).
pub const DRIVER_MAX_BYTES: u64 = 512 * 1024;

/// Whether the chosen file should replace the card's driver (decision 1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VersionVerdict {
    Newer,
    NotNewer,
    FileStatesNone,
}

/// `(version, revision)` compared as a tuple: `19.10` > `19.9`.
pub fn compare_versions(card: (u16, u16), file: Option<(u16, u16)>) -> VersionVerdict {
    match file {
        None => VersionVerdict::FileStatesNone,
        Some(file) if file > card => VersionVerdict::Newer,
        Some(_) => VersionVerdict::NotNewer,
    }
}

/// A driver goes into LSEGs verbatim, so it must be an executable made of
/// whole longwords (decision 6, `NOT-EXECUTABLE`).
pub fn check_driver_bytes(data: &[u8]) -> Result<(), RdbEditRefusal> {
    let refuse = |detail: String| RdbEditRefusal::NotExecutable { detail };
    if data.len() < 4 {
        return Err(refuse(format!(
            "it is {} bytes, too short to be one",
            data.len()
        )));
    }
    if !data.len().is_multiple_of(4) {
        return Err(refuse(format!(
            "its length, {} bytes, is not a whole number of longwords",
            data.len()
        )));
    }
    let first = u32::from_be_bytes([data[0], data[1], data[2], data[3]]);
    if first != HUNK_HEADER {
        return Err(refuse(format!(
            "it starts with {first:#010x}, not HUNK_HEADER {HUNK_HEADER:#010x}"
        )));
    }
    Ok(())
}

/// The inverse of `core::rdb::dos_type_string`: three one-byte characters,
/// then the last byte in decimal.
pub fn dostype_from_label(label: &str) -> Option<u32> {
    let mut chars = label.chars();
    let mut value = 0u32;
    for _ in 0..3 {
        let c = u32::from(chars.next()?);
        if c > 0xFF {
            return None;
        }
        value = (value << 8) | c;
    }
    let last: u8 = chars.as_str().parse().ok()?;
    Some((value << 8) | u32::from(last))
}

/// How far past `$VER:` a name is looked for — the same window
/// `core::rdb::version_from_ver_string` reads.
const VER_NAME_SCAN_BYTES: usize = 200;

/// The program a driver's `$VER:` string names — `pfs3aio` in
/// `$VER: pfs3aio 19.2 (2.10.18)` (spec decision 13). The first token after
/// the marker, split on whitespace and NUL. A token that starts with a digit
/// and holds a dot is a version, so a string that names no program answers
/// `None`, as does a file with no marker.
pub fn program_name_from_ver_string(data: &[u8]) -> Option<String> {
    const MARKER: &[u8] = b"$VER:";
    let at = data
        .windows(MARKER.len())
        .position(|window| window == MARKER)?;
    let start = at + MARKER.len();
    let end = start.saturating_add(VER_NAME_SCAN_BYTES).min(data.len());
    let token = data
        .get(start..end)?
        .split(|b| b.is_ascii_whitespace() || *b == 0)
        .find(|token| !token.is_empty())?;
    let is_version = token.first().is_some_and(u8::is_ascii_digit) && token.contains(&b'.');
    if is_version {
        return None;
    }
    Some(String::from_utf8_lossy(token).into_owned())
}

/// Whether two program names are one program. ASCII case is not identity.
pub fn same_program(card: &str, file: &str) -> bool {
    card.eq_ignore_ascii_case(file)
}

/// Decision 13: a replace needs the same program on both sides. A file that
/// names none is left to decision 1 ("no `$VER:` plans no step").
pub fn check_same_driver(card_payload: &[u8], file: &[u8]) -> Result<(), RdbEditRefusal> {
    let Some(file_name) = program_name_from_ver_string(file) else {
        return Ok(());
    };
    match program_name_from_ver_string(card_payload) {
        Some(card) if same_program(&card, &file_name) => Ok(()),
        card => Err(RdbEditRefusal::DifferentDriver {
            card,
            file: file_name,
        }),
    }
}

/// Decision 3's four stages, each followed by a sync.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Stage {
    /// S1: the LSEGs. Nothing references them.
    Data,
    /// S2: the FSHD. Still unreferenced.
    Header,
    /// S3: the RDSK with `HighRDSKBlock` (and a raised `RDBBlocksHi`).
    HighRdsk,
    /// S4: the one sector that links the driver in — the commit.
    Link,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditKind {
    Append,
    Replace,
}

/// Everything an edit will write, in order, and why.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditPlan {
    pub kind: EditKind,
    pub dos_type: u32,
    pub file_version: (u16, u16),
    /// The replaced FSHD's version; `None` for an append.
    pub card_version: Option<(u16, u16)>,
    pub replaced_fshd: Option<u32>,
    pub allocation: Allocation,
    pub stages: Vec<(Stage, Vec<BlockWrite>)>,
}

impl EditPlan {
    /// Every block the edit writes, sorted, once each — what the journal saves.
    pub fn blocks(&self) -> Vec<u32> {
        let set: BTreeSet<u32> = self
            .stages
            .iter()
            .flat_map(|(_, writes)| writes.iter().map(|w| w.block))
            .collect();
        set.into_iter().collect()
    }
}

/// The RDSK as S3 writes it.
fn raise_rdsk(rdsk: &[u8; BLOCK_SIZE], allocation: &Allocation) -> [u8; BLOCK_SIZE] {
    let raised = with_long(rdsk, RDSK_HIGH_RDSK_BLOCK, allocation.last_block);
    match allocation.raised_from {
        Some(_) => with_long(&raised, RDSK_RDB_BLOCKS_HI, allocation.rdb_blocks_hi),
        None => raised,
    }
}

fn missing(block: u32) -> RdbEditRefusal {
    unaccounted(block, format!("the image ends before block {block}"))
}

/// Append a driver for a DosType this RDB does not carry (decision 1).
pub fn plan_append(
    range: &[u8],
    walk: &StrictRdb,
    dos_type: u32,
    file_version: (u16, u16),
    driver: &[u8],
) -> Result<EditPlan, RdbEditRefusal> {
    check_driver_bytes(driver)?;
    if let Some(present) = walk.fshds.iter().find(|f| f.dos_type == dos_type) {
        return Err(unaccounted(
            present.block,
            format!(
                "this RDB already carries a {} driver",
                dos_type_string(dos_type)
            ),
        ));
    }
    let allocation = allocate(range, walk, driver.len())?;
    let k = allocation.fshd_block;

    let data: Vec<BlockWrite> = build_lseg_chain(driver, k + 1)
        .into_iter()
        .enumerate()
        .map(|(index, bytes)| BlockWrite {
            block: k + 1 + index as u32,
            bytes,
        })
        .collect();
    let header = BlockWrite {
        block: k,
        bytes: build_fshd_block(dos_type, file_version.0, file_version.1, NO_BLOCK, k + 1),
    };
    let rdsk_block = walk.rdsk.block;
    let rdsk = block_array(range, rdsk_block).ok_or_else(|| missing(rdsk_block))?;
    let raised = raise_rdsk(&rdsk, &allocation);
    let link = match walk.fshds.last() {
        None => BlockWrite {
            block: rdsk_block,
            bytes: with_long(&raised, RDSK_FILE_SYS_HEADER_LIST, k),
        },
        Some(last) => BlockWrite {
            block: last.block,
            bytes: with_long(
                &block_array(range, last.block).ok_or_else(|| missing(last.block))?,
                NEXT,
                k,
            ),
        },
    };

    Ok(EditPlan {
        kind: EditKind::Append,
        dos_type,
        file_version,
        card_version: None,
        replaced_fshd: None,
        allocation,
        stages: vec![
            (Stage::Data, data),
            (Stage::Header, vec![header]),
            (
                Stage::HighRdsk,
                vec![BlockWrite {
                    block: rdsk_block,
                    bytes: raised,
                }],
            ),
            (Stage::Link, vec![link]),
        ],
    })
}

/// Replace the driver for a DosType this RDB carries once (decision 1): the
/// new blocks are written above everything live, and one pointer moves from
/// the old FSHD to the new one. The old FSHD and LSEGs stay on disk, unlinked.
pub fn plan_replace(
    range: &[u8],
    walk: &StrictRdb,
    dos_type: u32,
    file_version: (u16, u16),
    driver: &[u8],
) -> Result<EditPlan, RdbEditRefusal> {
    check_driver_bytes(driver)?;
    let label = dos_type_string(dos_type);
    let matching: Vec<usize> = walk
        .fshds
        .iter()
        .enumerate()
        .filter(|(_, fs)| fs.dos_type == dos_type)
        .map(|(index, _)| index)
        .collect();
    let at = match matching.as_slice() {
        [one] => *one,
        [] => {
            return Err(unaccounted(
                walk.rdsk.block,
                format!("this RDB carries no {label} driver to replace"),
            ))
        }
        [_, second, ..] => {
            return Err(unaccounted(
                walk.fshds[*second].block,
                format!("two FSHDs carry {label}, so ART cannot tell which one AmigaOS loads"),
            ))
        }
    };
    let old = &walk.fshds[at];
    let allocation = allocate(range, walk, driver.len())?;
    let k = allocation.fshd_block;

    let data: Vec<BlockWrite> = build_lseg_chain(driver, k + 1)
        .into_iter()
        .enumerate()
        .map(|(index, bytes)| BlockWrite {
            block: k + 1 + index as u32,
            bytes,
        })
        .collect();
    let old_bytes = block_array(range, old.block).ok_or_else(|| missing(old.block))?;
    let version = (u32::from(file_version.0) << 16) | u32::from(file_version.1);
    let header = with_long(
        &with_long(
            &with_long(&old_bytes, NEXT, old.next),
            FSHD_VERSION,
            version,
        ),
        FSHD_SEG_LIST_BLOCKS,
        k + 1,
    );
    let rdsk_block = walk.rdsk.block;
    let rdsk = block_array(range, rdsk_block).ok_or_else(|| missing(rdsk_block))?;
    let raised = raise_rdsk(&rdsk, &allocation);
    let link = if at == 0 {
        BlockWrite {
            block: rdsk_block,
            bytes: with_long(&raised, RDSK_FILE_SYS_HEADER_LIST, k),
        }
    } else {
        let previous = &walk.fshds[at - 1];
        BlockWrite {
            block: previous.block,
            bytes: with_long(
                &block_array(range, previous.block).ok_or_else(|| missing(previous.block))?,
                NEXT,
                k,
            ),
        }
    };

    Ok(EditPlan {
        kind: EditKind::Replace,
        dos_type,
        file_version,
        card_version: Some((old.version, old.revision)),
        replaced_fshd: Some(old.block),
        allocation,
        stages: vec![
            (Stage::Data, data),
            (
                Stage::Header,
                vec![BlockWrite {
                    block: k,
                    bytes: header,
                }],
            ),
            (
                Stage::HighRdsk,
                vec![BlockWrite {
                    block: rdsk_block,
                    bytes: raised,
                }],
            ),
            (Stage::Link, vec![link]),
        ],
    })
}

#[cfg(test)]
pub(crate) mod fixtures {
    //! Card shapes built byte by byte in the test. Nothing here is Amiga
    //! content: the "driver" is a made-up hunk file with a `$VER:` string.

    use std::path::{Path, PathBuf};

    use crate::core::rdb::{
        compute_rdb_checksum, BLOCK_SIZE, IDNAME_FSHD, IDNAME_LSEG, IDNAME_PART, IDNAME_RDSK,
        NO_BLOCK,
    };

    pub const PDS3: u32 = 0x5044_5303;
    pub const DOS3: u32 = 0x444F_5303;
    pub const SFS0: u32 = 0x5346_5300;
    /// The 8 MiB ART reads of an area, in blocks.
    pub const WINDOW_BLOCKS: u32 = 16_384;

    /// `len` bytes (a multiple of 4): `HUNK_HEADER`, `$VER: pfs3aio <version>`
    /// at byte 64, then a byte pattern that holds no second `$VER:`.
    pub fn hunk_driver(len: usize, version: &str) -> Vec<u8> {
        named_driver(len, "pfs3aio", version)
    }

    /// The same, for a driver that calls itself `name` (spec decision 13's
    /// `SmartFilesystem` case).
    pub fn named_driver(len: usize, name: &str, version: &str) -> Vec<u8> {
        assert!(
            len >= 128 && len.is_multiple_of(4),
            "a fixture driver is whole longwords"
        );
        let mut data = vec![0u8; len];
        data[0..4].copy_from_slice(&0x0000_03F3u32.to_be_bytes());
        let ver = format!("$VER: {name} {version} (1.1.26)\0");
        assert!(
            64 + ver.len() <= 128,
            "the $VER: string fits before the byte pattern"
        );
        data[64..64 + ver.len()].copy_from_slice(ver.as_bytes());
        for (index, byte) in data.iter_mut().enumerate().skip(128) {
            *byte = (index % 251) as u8;
        }
        data
    }

    pub fn put(range: &mut [u8], block: u32, index: usize, value: u32) {
        let at = block as usize * BLOCK_SIZE + index * 4;
        range[at..at + 4].copy_from_slice(&value.to_be_bytes());
    }

    pub fn seal(range: &mut [u8], block: u32) {
        let at = block as usize * BLOCK_SIZE;
        let slice = &mut range[at..at + BLOCK_SIZE];
        let sum = compute_rdb_checksum(slice);
        slice[8..12].copy_from_slice(&sum.to_be_bytes());
    }

    pub struct FshdSpec {
        pub dos_type: u32,
        pub version: (u16, u16),
        pub driver: Vec<u8>,
        /// `fhb_FileSysName` at byte 172, as CaffeineOS writes it.
        pub name: Option<&'static str>,
        pub summed_longs: u32,
    }

    pub struct Shape {
        pub cylinders: u32,
        pub heads: u32,
        pub sectors: u32,
        pub rdb_blocks_hi: u32,
        pub lo_cylinder: u32,
        /// (name, LowCyl, HighCyl, DosType)
        pub parts: Vec<(&'static str, u32, u32, u32)>,
        pub fshds: Vec<FshdSpec>,
        /// `None`: the last structured block.
        pub high_rdsk_block: Option<u32>,
        pub total_blocks: u32,
    }

    /// RDSK at 0, PARTs from 1, then each FSHD followed by its LSEGs.
    pub fn build(shape: &Shape) -> Vec<u8> {
        let mut range = vec![0u8; shape.total_blocks as usize * BLOCK_SIZE];
        let mut next_free = 1 + shape.parts.len() as u32;
        let first_fshd = next_free;

        for (index, (name, low, high, dos_type)) in shape.parts.iter().enumerate() {
            let block = 1 + index as u32;
            put(&mut range, block, 0, IDNAME_PART);
            put(&mut range, block, 1, 64);
            put(&mut range, block, 3, 7);
            let next = if index + 1 < shape.parts.len() {
                block + 1
            } else {
                NO_BLOCK
            };
            put(&mut range, block, 4, next);
            put(&mut range, block, 5, u32::from(index == 0));
            let at = block as usize * BLOCK_SIZE + 36;
            range[at] = name.len() as u8;
            range[at + 1..at + 1 + name.len()].copy_from_slice(name.as_bytes());
            put(&mut range, block, 32, 16);
            put(&mut range, block, 33, 128);
            put(&mut range, block, 35, shape.heads);
            put(&mut range, block, 36, 1);
            put(&mut range, block, 37, shape.sectors);
            put(&mut range, block, 38, 2);
            put(&mut range, block, 41, *low);
            put(&mut range, block, 42, *high);
            put(&mut range, block, 43, 600);
            put(&mut range, block, 45, 0x0001_FE00);
            put(&mut range, block, 46, 0x7FFF_FFFE);
            put(&mut range, block, 48, *dos_type);
            seal(&mut range, block);
        }

        for (index, fs) in shape.fshds.iter().enumerate() {
            let fshd = next_free;
            let segments = fs.driver.len().div_ceil(492) as u32;
            let after = fshd + 1 + segments;
            put(&mut range, fshd, 0, IDNAME_FSHD);
            put(&mut range, fshd, 1, fs.summed_longs);
            put(&mut range, fshd, 3, 7);
            let next = if index + 1 < shape.fshds.len() {
                after
            } else {
                NO_BLOCK
            };
            put(&mut range, fshd, 4, next);
            put(&mut range, fshd, 8, fs.dos_type);
            put(
                &mut range,
                fshd,
                9,
                (u32::from(fs.version.0) << 16) | u32::from(fs.version.1),
            );
            put(&mut range, fshd, 10, 0x180);
            put(&mut range, fshd, 18, fshd + 1);
            put(&mut range, fshd, 19, NO_BLOCK);
            if let Some(name) = fs.name {
                let at = fshd as usize * BLOCK_SIZE + 172;
                range[at..at + name.len()].copy_from_slice(name.as_bytes());
            }
            seal(&mut range, fshd);
            for (seg, chunk) in fs.driver.chunks(492).enumerate() {
                let block = fshd + 1 + seg as u32;
                put(&mut range, block, 0, IDNAME_LSEG);
                put(&mut range, block, 1, 5 + chunk.len().div_ceil(4) as u32);
                put(&mut range, block, 3, 7);
                let next = if (seg as u32) + 1 < segments {
                    block + 1
                } else {
                    NO_BLOCK
                };
                put(&mut range, block, 4, next);
                let at = block as usize * BLOCK_SIZE + 20;
                range[at..at + chunk.len()].copy_from_slice(chunk);
                seal(&mut range, block);
            }
            next_free = after;
        }

        put(&mut range, 0, 0, IDNAME_RDSK);
        put(&mut range, 0, 1, 64);
        put(&mut range, 0, 3, 7);
        put(&mut range, 0, 4, 512);
        put(&mut range, 0, 5, 7);
        put(&mut range, 0, 6, NO_BLOCK);
        put(
            &mut range,
            0,
            7,
            if shape.parts.is_empty() { NO_BLOCK } else { 1 },
        );
        put(
            &mut range,
            0,
            8,
            if shape.fshds.is_empty() {
                NO_BLOCK
            } else {
                first_fshd
            },
        );
        put(&mut range, 0, 9, NO_BLOCK);
        for index in 10..16 {
            put(&mut range, 0, index, NO_BLOCK);
        }
        put(&mut range, 0, 16, shape.cylinders);
        put(&mut range, 0, 17, shape.sectors);
        put(&mut range, 0, 18, shape.heads);
        put(&mut range, 0, 32, 0);
        put(&mut range, 0, 33, shape.rdb_blocks_hi);
        put(&mut range, 0, 34, shape.lo_cylinder);
        put(&mut range, 0, 35, shape.cylinders - 1);
        put(&mut range, 0, 36, shape.heads * shape.sectors);
        put(
            &mut range,
            0,
            38,
            shape.high_rdsk_block.unwrap_or(next_free - 1),
        );
        seal(&mut range, 0);
        range
    }

    /// The owner's card as the research measured it (R §0): 12/256,
    /// `RDBBlocksHi` 6143, `LoCylinder` 2, `SDH0` 2–535 and `SDH1` 536–36194,
    /// a `PDS\3` 19.2 FSHD with `SummedLongs` 128 and a name at byte 172, LSEG
    /// 4–131, `HighRDSKBlock` 131, a stale RDB copy at 2048–2179 with Heads 6,
    /// and non-zero blocks 5120–6031. With `linked` false the driver's blocks
    /// stay where they are but the RDSK no longer lists them — a card that
    /// lacks the DosType and still has `HighRDSKBlock` 131.
    pub fn caffeine_like(linked: bool) -> Vec<u8> {
        let mut range = build(&Shape {
            cylinders: 38_488,
            heads: 12,
            sectors: 256,
            rdb_blocks_hi: 6143,
            lo_cylinder: 2,
            parts: vec![("SDH0", 2, 535, PDS3), ("SDH1", 536, 36_194, PDS3)],
            fshds: vec![FshdSpec {
                dos_type: PDS3,
                version: (19, 2),
                driver: hunk_driver(62_604, "19.2"),
                name: Some("L:pfs3aio040"),
                summed_longs: 128,
            }],
            high_rdsk_block: None,
            total_blocks: WINDOW_BLOCKS,
        });
        if !linked {
            put(&mut range, 0, 8, NO_BLOCK);
            seal(&mut range, 0);
        }
        let (live, stale) = range.split_at_mut(2048 * BLOCK_SIZE);
        stale[..132 * BLOCK_SIZE].copy_from_slice(&live[..132 * BLOCK_SIZE]);
        put(&mut range, 2048, 16, 39_080);
        put(&mut range, 2048, 18, 6);
        put(&mut range, 2048, 33, 3071);
        put(&mut range, 2048, 36, 1536);
        seal(&mut range, 2048);
        for (block, name) in [(2049u32, b"DH0"), (2050, b"DH1")] {
            let at = block as usize * BLOCK_SIZE + 36;
            range[at] = 3;
            range[at + 1..at + 4].copy_from_slice(name);
            range[at + 4] = 0;
            seal(&mut range, block);
        }
        for (offset, byte) in range[5120 * BLOCK_SIZE..6032 * BLOCK_SIZE]
            .iter_mut()
            .enumerate()
        {
            *byte = (offset % 253) as u8 + 1;
        }
        range[5120 * BLOCK_SIZE..5120 * BLOCK_SIZE + 4].copy_from_slice(b"PFS\x01");
        range
    }

    /// An RDB exactly as `create_rdb_layout` writes it: 16/63, one `PDS\3`
    /// partition, and either a 62 604-byte driver (`RDBBlocksHi` 130) or none
    /// (`RDBBlocksHi` 1). Padded to the 8 MiB window with zeros.
    pub fn art_like(with_driver: bool) -> Vec<u8> {
        use crate::core::rdb::{create_rdb_layout, AmigaHardDiskFs, FileSystemSpec, PartitionSpec};
        let drivers = if with_driver {
            vec![FileSystemSpec {
                dos_type: PDS3,
                version: 19,
                revision: 2,
                data: hunk_driver(62_604, "19.2"),
            }]
        } else {
            Vec::new()
        };
        let layout = create_rdb_layout(
            64 * 1024 * 1024,
            &[PartitionSpec {
                drive_name: "DH0".into(),
                fs_type: AmigaHardDiskFs::Pfs3DirectScsi,
                size_mb: 32,
                bootable: true,
                boot_priority: 0,
                num_buffers: 0,
            }],
            &drivers,
        )
        .unwrap();
        let mut range = layout.blocks;
        range.resize(WINDOW_BLOCKS as usize * BLOCK_SIZE, 0);
        range
    }

    /// The owner's geometry with two small drivers: FSHD 3 (LSEG 4–12) of
    /// `first`, FSHD 13 (LSEG 14–22) of `second`; `HighRDSKBlock` 22.
    pub fn two_drivers(first: u32, second: u32) -> Vec<u8> {
        let spec = |dos_type: u32| FshdSpec {
            dos_type,
            version: (19, 2),
            driver: hunk_driver(4096, "19.2"),
            name: Some("L:driver"),
            summed_longs: 128,
        };
        build(&Shape {
            cylinders: 38_488,
            heads: 12,
            sectors: 256,
            rdb_blocks_hi: 6143,
            lo_cylinder: 2,
            parts: vec![("SDH0", 2, 535, PDS3), ("SDH1", 536, 36_194, PDS3)],
            fshds: vec![spec(first), spec(second)],
            high_rdsk_block: None,
            total_blocks: WINDOW_BLOCKS,
        })
    }

    /// A plain image: the RDB at byte 0, zero-extended to `total_bytes`.
    pub fn write_image(dir: &Path, range: &[u8], total_bytes: u64) -> PathBuf {
        let path = dir.join("card.hdf");
        std::fs::write(&path, range).unwrap();
        std::fs::OpenOptions::new()
            .write(true)
            .open(&path)
            .unwrap()
            .set_len(total_bytes)
            .unwrap();
        path
    }

    /// A card: an MBR with a FAT32 slot 1 and a `0x76` slot 2 at LBA 8192
    /// (4 MiB), the RDB range written there. The Amiga area is slot 2.
    pub fn write_card(dir: &Path, range: &[u8]) -> PathBuf {
        use std::io::{Seek, SeekFrom, Write};
        let path = dir.join("card.img");
        let mut sector = [0u8; 512];
        for (slot, kind, lba, count) in
            [(0usize, 0x0Cu8, 2048u32, 6144u32), (1, 0x76, 8192, 32_768)]
        {
            let at = 446 + slot * 16;
            sector[at + 4] = kind;
            sector[at + 8..at + 12].copy_from_slice(&lba.to_le_bytes());
            sector[at + 12..at + 16].copy_from_slice(&count.to_le_bytes());
        }
        sector[510] = 0x55;
        sector[511] = 0xAA;
        let mut file = std::fs::File::create(&path).unwrap();
        file.set_len((8192 + 32_768) * 512).unwrap();
        file.write_all(&sector).unwrap();
        file.seek(SeekFrom::Start(8192 * 512)).unwrap();
        file.write_all(range).unwrap();
        path
    }

    /// The owner's geometry with a non-empty filesystem list: a `DOS\3`
    /// driver at FSHD 3 (LSEG 4–12), no `PDS\3`, `HighRDSKBlock` 12. An append
    /// here links through that FSHD's `Next`, not through the RDSK.
    pub fn dos3_first() -> Vec<u8> {
        build(&Shape {
            cylinders: 38_488,
            heads: 12,
            sectors: 256,
            rdb_blocks_hi: 6143,
            lo_cylinder: 2,
            parts: vec![("SDH0", 2, 535, PDS3), ("SDH1", 536, 36_194, PDS3)],
            fshds: vec![FshdSpec {
                dos_type: DOS3,
                version: (45, 1),
                driver: hunk_driver(4096, "45.1"),
                name: None,
                summed_longs: 64,
            }],
            high_rdsk_block: None,
            total_blocks: WINDOW_BLOCKS,
        })
    }
}

#[cfg(test)]
mod walk_tests {
    use super::fixtures::*;
    use super::*;
    use crate::core::error::RdbEditRefusal;
    use crate::core::rdb::NO_BLOCK;

    #[test]
    fn the_caffeine_shaped_rdb_is_accounted_for_block_by_block() {
        let range = caffeine_like(true);
        let walk = walk_strict(&range).unwrap();
        assert_eq!(walk.rdsk.rdb_blocks_hi, 6143);
        assert_eq!(walk.rdsk.high_rdsk_block, 131);
        assert_eq!(walk.rdsk.heads, 12);
        let firsts: Vec<Option<u64>> = walk.parts.iter().map(|p| p.envec_first_block).collect();
        assert_eq!(firsts, vec![Some(6144), Some(1_646_592)]);
        assert_eq!(walk.first_partition_block(), Some(6144));
        assert_eq!(walk.fshds.len(), 1);
        let fs = &walk.fshds[0];
        assert_eq!(
            (fs.block, fs.dos_type, fs.version, fs.revision),
            (3, PDS3, 19, 2)
        );
        assert_eq!(fs.lseg_blocks, (4..=131).collect::<Vec<u32>>());
        assert_eq!(fs.payload, hunk_driver(62_604, "19.2"));
        assert_eq!(walk.used, (0..=131).collect::<BTreeSet<u32>>());
        assert_eq!(walk.highest_used(), 131);
    }

    /// "Not on a chain does not mean zero" (R §0): the stale copy at 2048 and
    /// the PFS3 blocks from 5120 are inside the range and are not the RDB.
    #[test]
    fn the_stale_copy_and_the_old_pfs3_blocks_are_not_part_of_the_walk() {
        let walk = walk_strict(&caffeine_like(true)).unwrap();
        assert!(!walk.used.contains(&2048));
        assert!(!walk.used.contains(&5120));
    }

    #[test]
    fn an_art_built_rdb_walks_too() {
        let walk = walk_strict(&art_like(true)).unwrap();
        assert_eq!(walk.rdsk.rdb_blocks_hi, 130);
        assert_eq!(walk.rdsk.high_rdsk_block, 130);
        assert_eq!(walk.first_partition_block(), Some(2016));
        assert_eq!(walk.fshds[0].lseg_blocks, (3..=130).collect::<Vec<u32>>());
    }

    /// `rdb_DriveInit` is a `LoadSegBlock` list (`hardblocks.h` [1][4]); its
    /// blocks are in use.
    #[test]
    fn a_drive_init_chain_is_in_the_used_set() {
        let mut range = caffeine_like(true);
        put(&mut range, 132, 0, crate::core::rdb::IDNAME_LSEG);
        put(&mut range, 132, 1, 6);
        put(&mut range, 132, 3, 7);
        put(&mut range, 132, 4, NO_BLOCK);
        put(&mut range, 132, 5, 0x4E75_4E75);
        seal(&mut range, 132);
        put(&mut range, 0, 9, 132);
        seal(&mut range, 0);
        let walk = walk_strict(&range).unwrap();
        assert_eq!(walk.drive_init, vec![132]);
        assert!(walk.used.contains(&132));
        assert_eq!(walk.highest_used(), 132);
    }

    /// One row per strict check (decision 5): the variant, the block it names
    /// and the words of its sentence.
    #[test]
    fn each_strict_check_refuses_with_its_own_block_and_sentence() {
        type Break = fn(&mut Vec<u8>);
        let rows: Vec<(&str, Break, u32, &str)> = vec![
            (
                "no RDSK",
                |r| r.fill(0),
                0,
                "no block carries the ID RDSK in blocks 0–15",
            ),
            (
                "two RDSK",
                |r| {
                    let (a, b) = r.split_at_mut(5 * 512);
                    b[..512].copy_from_slice(&a[..512]);
                },
                5,
                "blocks 0 and 5 both carry the ID RDSK",
            ),
            // Byte 200 is inside the RDSK's 64 summed longwords; a byte past 256 would not be.
            (
                "RDSK checksum",
                |r| r[200] ^= 1,
                0,
                "RDSK block 0 fails its checksum",
            ),
            (
                "block bytes",
                |r| {
                    put(r, 0, 4, 1024);
                    seal(r, 0);
                },
                0,
                "rdb_BlockBytes is 1024",
            ),
            (
                "lo above hi",
                |r| {
                    put(r, 0, 32, 7000);
                    seal(r, 0);
                },
                0,
                "RDBBlocksLo 7000 is above RDBBlocksHi 6143",
            ),
            (
                "RDSK below lo",
                |r| {
                    put(r, 0, 32, 1);
                    seal(r, 0);
                },
                0,
                "lies below RDBBlocksLo 1",
            ),
            (
                "high above hi",
                |r| {
                    put(r, 0, 38, 6144);
                    seal(r, 0);
                },
                0,
                "HighRDSKBlock 6144 is above RDBBlocksHi 6143",
            ),
            (
                "hi past window",
                |r| {
                    put(r, 0, 33, 16_384);
                    seal(r, 0);
                },
                0,
                "reaches past the 16384 blocks ART reads",
            ),
            (
                "hi reaches partition",
                |r| {
                    put(r, 0, 33, 6144);
                    seal(r, 0);
                },
                6144,
                "reaches the first partition, which begins at block 6144",
            ),
            (
                "PART id",
                |r| {
                    put(r, 1, 0, 0);
                    seal(r, 1);
                },
                1,
                "block 1 should be PART",
            ),
            (
                "PART summed longs",
                |r| {
                    put(r, 2, 1, 129);
                    seal(r, 2);
                },
                2,
                "PART block 2 declares SummedLongs 129",
            ),
            (
                "FSHD summed longs",
                |r| {
                    put(r, 3, 1, 0);
                },
                3,
                "FSHD block 3 declares SummedLongs 0",
            ),
            (
                "LSEG summed longs",
                |r| {
                    put(r, 50, 1, 4);
                    seal(r, 50);
                },
                50,
                "LSEG block 50 declares SummedLongs 4",
            ),
            (
                "LSEG checksum",
                |r| r[50 * 512 + 100] ^= 1,
                50,
                "LSEG block 50 fails its checksum",
            ),
            (
                "pointer outside",
                |r| {
                    put(r, 2, 4, 7000);
                    seal(r, 2);
                },
                7000,
                "points outside RDBBlocksLo 0..RDBBlocksHi 6143",
            ),
            (
                "repeat",
                |r| {
                    put(r, 131, 4, 4);
                    seal(r, 131);
                },
                4,
                "block 4, which is already part of the RDB",
            ),
            (
                "drive init not LSEG",
                |r| {
                    put(r, 0, 9, 132);
                    seal(r, 0);
                },
                132,
                "block 132 should be LSEG",
            ),
        ];
        for (name, damage, block, needle) in rows {
            let mut range = caffeine_like(true);
            damage(&mut range);
            match walk_strict(&range) {
                Err(RdbEditRefusal::Unaccounted {
                    block: named,
                    ref check,
                }) => {
                    assert_eq!(named, block, "{name}: {check}");
                    let sentence = walk_strict(&range).unwrap_err().to_string();
                    assert!(sentence.contains(needle), "{name}: {sentence}");
                    assert!(
                        sentence.contains("Nothing was written."),
                        "{name}: {sentence}"
                    );
                }
                other => panic!("{name}: expected Unaccounted, got {other:?}"),
            }
        }
    }

    #[test]
    fn a_bad_block_list_is_its_own_refusal() {
        let mut range = caffeine_like(true);
        put(&mut range, 0, 6, 7);
        seal(&mut range, 0);
        let err = walk_strict(&range).unwrap_err();
        assert_eq!(err, RdbEditRefusal::BadBlocks { first: 7 });
        assert_eq!(err.code(), "ART-RDB-EDIT-BAD-BLOCKS");
        assert!(
            err.to_string().contains("bad-block list (from block 7)"),
            "{err}"
        );
        assert!(err.to_string().contains("hst-imager can"), "{err}");
    }

    #[test]
    fn a_range_shorter_than_rdb_blocks_hi_is_refused_not_indexed() {
        let range = caffeine_like(true);
        let err = walk_strict(&range[..4096 * 512]).unwrap_err();
        assert!(
            err.to_string()
                .contains("the image ends before RDBBlocksHi 6143"),
            "{err}"
        );
    }
}

#[cfg(test)]
mod block_tests {
    use super::fixtures::*;
    use super::*;
    use crate::core::rdb::verify_rdb_block_checksum;

    /// Decision 3: a replaced FSHD keeps every byte but the longs ART sets —
    /// including a name at byte 172 inside a 128-longword checksum.
    #[test]
    fn with_long_changes_one_longword_and_reseals_over_the_blocks_own_summed_longs() {
        let range = caffeine_like(true);
        let before = block_array(&range, 3).unwrap();
        let after = with_long(&before, FSHD_VERSION, (19 << 16) | 3);
        assert!(verify_rdb_block_checksum(&after));
        assert_eq!(long(&after, FSHD_VERSION), (19 << 16) | 3);
        for (at, (old, new)) in before.iter().zip(after.iter()).enumerate() {
            if !(8..12).contains(&at) && !(36..40).contains(&at) {
                assert_eq!(old, new, "byte {at}");
            }
        }
        assert_eq!(&after[172..184], b"L:pfs3aio040");
    }

    #[test]
    fn block_array_answers_none_past_the_end_rather_than_panicking() {
        assert!(block_array(&[0u8; 1024], 1).is_some());
        assert!(block_array(&[0u8; 1024], 2).is_none());
        assert!(block_array(&[0u8; 1023], 1).is_none());
        assert!(block_array(&[], u32::MAX).is_none());
    }
}

#[cfg(test)]
mod alloc_tests {
    use super::fixtures::*;
    use super::*;
    use crate::core::error::{PartitionBound, RaiseRefused, RdbEditRefusal};

    /// 1006 LSEGs from block 2 end at 1008; 1005 end at 1007.
    const REACHES_1008: usize = 1006 * 492;
    const STOPS_AT_1007: usize = 1005 * 492;

    fn allocate_on(range: &[u8], len: usize) -> Result<Allocation, RdbEditRefusal> {
        allocate(range, &walk_strict(range).unwrap(), len)
    }

    /// Decision 2 on the owner's shape: 132–260 whether the old driver is
    /// linked or only still sitting there — `HighRDSKBlock` 131 is respected.
    #[test]
    fn on_the_caffeine_shape_the_driver_takes_132_to_260_and_nothing_is_raised() {
        for linked in [true, false] {
            let got = allocate_on(&caffeine_like(linked), 62_604).unwrap();
            assert_eq!(
                got,
                Allocation {
                    fshd_block: 132,
                    last_block: 260,
                    lseg_count: 128,
                    rdb_blocks_hi: 6143,
                    raised_from: None,
                },
                "linked {linked}"
            );
        }
    }

    /// Decision 12: the cards ART builds have no room until the raise.
    #[test]
    fn an_art_built_card_gets_room_by_raising_rdb_blocks_hi_to_exactly_the_last_block() {
        assert_eq!(
            allocate_on(&art_like(true), 62_604).unwrap(),
            Allocation {
                fshd_block: 131,
                last_block: 259,
                lseg_count: 128,
                rdb_blocks_hi: 259,
                raised_from: Some(130),
            }
        );
        assert_eq!(
            allocate_on(&art_like(false), 62_604).unwrap(),
            Allocation {
                fshd_block: 2,
                last_block: 130,
                lseg_count: 128,
                rdb_blocks_hi: 130,
                raised_from: Some(1),
            }
        );
    }

    #[test]
    fn a_raise_never_crosses_a_block_that_is_not_empty() {
        let mut range = art_like(false);
        range[500 * 512 + 7] = 1;
        assert!(
            allocate_on(&range, 62_604).is_ok(),
            "control: 2–130 stays below block 500"
        );
        let err = allocate_on(&range, 600 * 492).unwrap_err();
        assert!(
            matches!(
                err,
                RdbEditRefusal::NoRoom {
                    raise: RaiseRefused::NonZeroBlock(500),
                    ..
                }
            ),
            "{err:?}"
        );
        assert_eq!(err.code(), "ART-RDB-EDIT-NO-ROOM");
        let sentence = err.to_string();
        assert!(
            sentence.contains("needs 601 blocks from block 2"),
            "{sentence}"
        );
        assert!(
            sentence.contains("only 0 are free below RDBBlocksHi 1"),
            "{sentence}"
        );
        assert!(
            sentence.contains("block 500 above it is not empty"),
            "{sentence}"
        );
        assert!(
            sentence.contains("hst-imager rewrites the whole RDB"),
            "{sentence}"
        );
    }

    fn refused_at_1008(range: &[u8]) -> RdbEditRefusal {
        assert!(
            allocate_on(range, STOPS_AT_1007).is_ok(),
            "control: 2–1007 fits"
        );
        allocate_on(range, REACHES_1008).unwrap_err()
    }

    #[test]
    fn the_raise_stops_at_the_first_partition_by_its_own_envec() {
        assert!(
            allocate_on(&art_like(false), REACHES_1008).is_ok(),
            "control: 2016 is the bound"
        );
        let mut range = art_like(false);
        put(&mut range, 1, 35, 8); // PART Surfaces: 2 × 8 × 63 = 1008
        seal(&mut range, 1);
        let err = refused_at_1008(&range);
        assert!(
            matches!(
                err,
                RdbEditRefusal::NoRoom {
                    raise: RaiseRefused::PastPartitionArea {
                        limit: 1008,
                        bound: PartitionBound::PartitionEnvec
                    },
                    partition_block: Some(1008),
                    ..
                }
            ),
            "{err:?}"
        );
        assert!(
            err.to_string()
                .contains("the first partition begins at block 1008 by its own DosEnvec"),
            "{err}"
        );
    }

    #[test]
    fn the_raise_stops_at_the_first_partition_by_the_rdbs_own_geometry() {
        let mut range = art_like(false);
        put(&mut range, 0, 18, 8); // rdb_Heads: 2 × 8 × 63 = 1008
        seal(&mut range, 0);
        let err = refused_at_1008(&range);
        assert!(
            matches!(
                err,
                RdbEditRefusal::NoRoom {
                    raise: RaiseRefused::PastPartitionArea {
                        limit: 1008,
                        bound: PartitionBound::RdbGeometry
                    },
                    partition_block: Some(2016),
                    ..
                }
            ),
            "{err:?}"
        );
        assert!(
            err.to_string().contains(
                "the first partition begins at block 1008 by the RDB's own heads and sectors"
            ),
            "{err}"
        );
    }

    #[test]
    fn the_raise_stops_where_lo_cylinder_says_the_partitionable_area_begins() {
        let mut range = art_like(false);
        put(&mut range, 0, 36, 504); // rdb_CylBlocks: 2 × 504 = 1008
        seal(&mut range, 0);
        let err = refused_at_1008(&range);
        assert!(
            matches!(
                err,
                RdbEditRefusal::NoRoom {
                    raise: RaiseRefused::PastPartitionArea {
                        limit: 1008,
                        bound: PartitionBound::LoCylinder
                    },
                    ..
                }
            ),
            "{err:?}"
        );
        assert!(
            err.to_string()
                .contains("rdb_LoCylinder puts the partitionable area at block 1008"),
            "{err}"
        );
    }

    #[test]
    fn the_raise_stops_at_the_window_art_reads() {
        let range = build(&Shape {
            cylinders: 1000,
            heads: 255,
            sectors: 63,
            rdb_blocks_hi: 16_000,
            lo_cylinder: 2,
            parts: vec![("DH0", 2, 999, PDS3)],
            fshds: Vec::new(),
            high_rdsk_block: Some(16_000),
            total_blocks: WINDOW_BLOCKS,
        });
        let err = allocate_on(&range, 400 * 492).unwrap_err();
        assert!(
            matches!(
                err,
                RdbEditRefusal::NoRoom {
                    raise: RaiseRefused::PastWindow {
                        window_blocks: 16_384
                    },
                    start: 16_001,
                    ..
                }
            ),
            "{err:?}"
        );
        assert!(
            err.to_string()
                .contains("ART reads only the first 16384 blocks"),
            "{err}"
        );
    }
}

#[cfg(test)]
mod driver_tests {
    use super::fixtures::*;
    use super::*;
    use crate::core::error::RdbEditRefusal;
    use crate::core::rdb::{dos_type_string, version_from_ver_string};

    /// Decision 1: the tuple, so 19.10 is newer than 19.9; strictly greater
    /// replaces; equal, older and silent do not.
    #[test]
    fn a_file_replaces_the_card_only_when_its_version_is_strictly_greater() {
        assert_eq!(
            compare_versions((19, 9), Some((19, 10))),
            VersionVerdict::Newer
        );
        assert_eq!(
            compare_versions((19, 2), Some((19, 3))),
            VersionVerdict::Newer
        );
        assert_eq!(
            compare_versions((19, 99), Some((20, 0))),
            VersionVerdict::Newer
        );
        assert_eq!(
            compare_versions((19, 2), Some((19, 2))),
            VersionVerdict::NotNewer
        );
        assert_eq!(
            compare_versions((19, 3), Some((19, 2))),
            VersionVerdict::NotNewer
        );
        assert_eq!(
            compare_versions((19, 2), None),
            VersionVerdict::FileStatesNone
        );
    }

    /// The number compared is the number written: `version_from_ver_string`'s
    /// `u16` halves, the FSHD's own field.
    #[test]
    fn the_fixture_drivers_version_is_read_the_way_the_fshd_stores_it() {
        assert_eq!(
            version_from_ver_string(&hunk_driver(1024, "19.10")),
            Some((19, 10))
        );
        let card = walk_strict(&caffeine_like(true)).unwrap().fshds[0].clone();
        assert_eq!(
            compare_versions(
                (card.version, card.revision),
                version_from_ver_string(&hunk_driver(1024, "19.10"))
            ),
            VersionVerdict::Newer
        );
    }

    #[test]
    fn a_hunk_file_of_whole_longwords_is_accepted() {
        assert_eq!(check_driver_bytes(&hunk_driver(62_604, "19.3")), Ok(()));
    }

    #[test]
    fn an_lha_archive_is_not_an_executable_and_the_sentence_says_to_unpack_it() {
        let mut lha = vec![0u8; 64];
        lha[..7].copy_from_slice(b"\x2a\x00-lh5-");
        let err = check_driver_bytes(&lha).unwrap_err();
        assert!(
            matches!(err, RdbEditRefusal::NotExecutable { .. }),
            "{err:?}"
        );
        assert_eq!(err.code(), "ART-RDB-EDIT-NOT-EXECUTABLE");
        let sentence = err.to_string();
        assert!(
            sentence.contains("it starts with 0x2a002d6c, not HUNK_HEADER 0x000003f3"),
            "{sentence}"
        );
        assert!(
            sentence.contains("An .lha archive has to be unpacked first"),
            "{sentence}"
        );
    }

    #[test]
    fn a_length_that_is_not_whole_longwords_is_refused() {
        let mut driver = hunk_driver(62_604, "19.3");
        driver.pop();
        let err = check_driver_bytes(&driver).unwrap_err();
        assert!(
            err.to_string()
                .contains("62603 bytes, is not a whole number of longwords"),
            "{err}"
        );
        let err = check_driver_bytes(&[0, 0]).unwrap_err();
        assert!(
            err.to_string()
                .contains("it is 2 bytes, too short to be one"),
            "{err}"
        );
    }

    #[test]
    fn a_dostype_label_round_trips() {
        for dos_type in [PDS3, DOS3, SFS0, 0x444F_530A, 0x5046_5303] {
            assert_eq!(
                dostype_from_label(&dos_type_string(dos_type)),
                Some(dos_type),
                "{dos_type:#x}"
            );
        }
        assert_eq!(dostype_from_label("PD"), None);
        assert_eq!(dostype_from_label("PDSx"), None);
        assert_eq!(dostype_from_label("PDS256"), None);
    }

    /// Decision 13: the first token after `$VER:` is the program; a token
    /// shaped like a version is not a name.
    #[test]
    fn the_program_name_is_the_first_token_after_ver() {
        assert_eq!(
            program_name_from_ver_string(&hunk_driver(1024, "19.3")).as_deref(),
            Some("pfs3aio")
        );
        assert_eq!(
            program_name_from_ver_string(b"$VER: SmartFilesystem 1.293 (18.1.17)").as_deref(),
            Some("SmartFilesystem")
        );
        assert_eq!(
            program_name_from_ver_string(b"$VER:\0\0pfs3aio.device 4.1").as_deref(),
            Some("pfs3aio.device")
        );
        assert_eq!(program_name_from_ver_string(b"$VER: 19.2 (1.1.26)"), None);
        assert_eq!(program_name_from_ver_string(b"$VER:    "), None);
        assert_eq!(program_name_from_ver_string(&[0u8; 512]), None);
    }

    /// **The case the ruling exists for.** SmartFilesystem 1.293 is "older"
    /// than pfs3aio 19.3 by the version tuple alone, so a replace would put a
    /// PFS3 driver under an `SFS\0` header. The names refuse it.
    #[test]
    fn an_sfs_driver_on_the_card_is_not_replaced_by_pfs3aio() {
        let card = named_driver(4096, "SmartFilesystem", "1.293");
        let file = hunk_driver(62_604, "19.3");
        assert_eq!(
            compare_versions((1, 293), Some((19, 3))),
            VersionVerdict::Newer,
            "the versions alone would replace it"
        );
        let err = check_same_driver(&card, &file).unwrap_err();
        assert_eq!(
            err,
            RdbEditRefusal::DifferentDriver {
                card: Some("SmartFilesystem".into()),
                file: "pfs3aio".into()
            }
        );
        assert_eq!(err.code(), "ART-RDB-EDIT-DIFFERENT-DRIVER");
        let sentence = err.to_string();
        assert!(
            sentence.contains("calls itself 'SmartFilesystem'"),
            "{sentence}"
        );
        assert!(sentence.contains("calls itself 'pfs3aio'"), "{sentence}");
        assert!(sentence.contains("hst-imager can replace it"), "{sentence}");
    }

    #[test]
    fn a_card_driver_that_does_not_say_what_it_is_is_not_replaced() {
        let mut card = hunk_driver(4096, "19.2");
        card[64..69].copy_from_slice(b"$XXX:");
        let err = check_same_driver(&card, &hunk_driver(62_604, "19.3")).unwrap_err();
        assert_eq!(
            err,
            RdbEditRefusal::DifferentDriver {
                card: None,
                file: "pfs3aio".into()
            }
        );
        assert!(err.to_string().contains("does not say what it is"), "{err}");
    }

    /// Case is not identity, and a silent file is decision 1's "no step",
    /// not this refusal.
    #[test]
    fn the_same_program_in_another_case_replaces_and_a_silent_file_is_not_this_refusal() {
        let card = named_driver(4096, "PFS3AIO", "19.2");
        assert_eq!(
            check_same_driver(&card, &hunk_driver(62_604, "19.3")),
            Ok(())
        );
        let mut silent = hunk_driver(62_604, "19.3");
        silent[64..69].copy_from_slice(b"$XXX:");
        assert_eq!(check_same_driver(&card, &silent), Ok(()));
    }
}

#[cfg(test)]
mod append_plan_tests {
    use super::fixtures::*;
    use super::*;

    fn stage(plan: &EditPlan, name: Stage) -> &[BlockWrite] {
        &plan.stages.iter().find(|(s, _)| *s == name).unwrap().1
    }

    /// Decision 3's order, and each stage's blocks, on an empty list.
    #[test]
    fn an_append_is_data_then_header_then_high_rdsk_then_one_link_sector() {
        let range = caffeine_like(false);
        let driver = hunk_driver(62_604, "19.3");
        let plan = plan_append(
            &range,
            &walk_strict(&range).unwrap(),
            PDS3,
            (19, 3),
            &driver,
        )
        .unwrap();
        let order: Vec<Stage> = plan.stages.iter().map(|(s, _)| *s).collect();
        assert_eq!(
            order,
            vec![Stage::Data, Stage::Header, Stage::HighRdsk, Stage::Link]
        );

        let data: Vec<u32> = stage(&plan, Stage::Data).iter().map(|w| w.block).collect();
        assert_eq!(data, (133..=260).collect::<Vec<u32>>());
        let header = &stage(&plan, Stage::Header)[0];
        assert_eq!(header.block, 132);
        assert_eq!(long(&header.bytes, NEXT), NO_BLOCK);
        assert_eq!(long(&header.bytes, FSHD_SEG_LIST_BLOCKS), 133);
        assert_eq!(long(&header.bytes, FSHD_VERSION), (19 << 16) | 3);

        let high = &stage(&plan, Stage::HighRdsk)[0];
        assert_eq!(high.block, 0);
        assert_eq!(long(&high.bytes, RDSK_HIGH_RDSK_BLOCK), 260);
        assert_eq!(
            long(&high.bytes, RDSK_FILE_SYS_HEADER_LIST),
            NO_BLOCK,
            "S3 does not link"
        );
        assert_eq!(long(&high.bytes, RDSK_RDB_BLOCKS_HI), 6143);

        let link = &stage(&plan, Stage::Link)[0];
        assert_eq!(link.block, 0);
        assert_eq!(long(&link.bytes, RDSK_FILE_SYS_HEADER_LIST), 132);
        assert_eq!(
            long(&link.bytes, RDSK_HIGH_RDSK_BLOCK),
            260,
            "the link keeps S3's raise"
        );
        assert_eq!(
            plan.blocks(),
            std::iter::once(0).chain(132..=260).collect::<Vec<u32>>()
        );
    }

    #[test]
    fn a_non_empty_list_is_linked_through_its_last_fshd() {
        let range = dos3_first();
        let driver = hunk_driver(62_604, "19.3");
        let plan = plan_append(
            &range,
            &walk_strict(&range).unwrap(),
            PDS3,
            (19, 3),
            &driver,
        )
        .unwrap();
        let link = &stage(&plan, Stage::Link)[0];
        assert_eq!(link.block, 3);
        assert_eq!(long(&link.bytes, NEXT), 13);
        assert!(crate::core::rdb::verify_rdb_block_checksum(&link.bytes));
    }

    /// Decision 12: the raise rides in S3, with `HighRDSKBlock`.
    #[test]
    fn a_raise_is_written_in_the_high_rdsk_stage() {
        let range = art_like(false);
        let driver = hunk_driver(62_604, "19.3");
        let plan = plan_append(
            &range,
            &walk_strict(&range).unwrap(),
            PDS3,
            (19, 3),
            &driver,
        )
        .unwrap();
        let high = &stage(&plan, Stage::HighRdsk)[0];
        assert_eq!(long(&high.bytes, RDSK_RDB_BLOCKS_HI), 130);
        assert_eq!(long(&high.bytes, RDSK_HIGH_RDSK_BLOCK), 130);
    }
}

#[cfg(test)]
mod replace_plan_tests {
    use super::fixtures::*;
    use super::*;
    use crate::core::error::RdbEditRefusal;

    fn replace(range: &[u8]) -> EditPlan {
        plan_replace(
            range,
            &walk_strict(range).unwrap(),
            PDS3,
            (19, 3),
            &hunk_driver(62_604, "19.3"),
        )
        .unwrap()
    }

    fn only(plan: &EditPlan, name: Stage) -> BlockWrite {
        plan.stages.iter().find(|(s, _)| *s == name).unwrap().1[0].clone()
    }

    /// Decision 3: the new FSHD is the old one's 512 bytes with only `Next`,
    /// `Version`, `SegListBlocks` and the checksum rewritten.
    #[test]
    fn a_replace_keeps_the_old_fshd_bytes_but_version_seg_list_and_checksum() {
        let range = caffeine_like(true);
        let plan = replace(&range);
        assert_eq!(
            (plan.kind, plan.card_version, plan.replaced_fshd),
            (EditKind::Replace, Some((19, 2)), Some(3))
        );
        let header = only(&plan, Stage::Header);
        assert_eq!(header.block, 132);
        let old = block_array(&range, 3).unwrap();
        for (at, (was, now)) in old.iter().zip(header.bytes.iter()).enumerate() {
            let rewritten =
                (8..12).contains(&at) || (36..40).contains(&at) || (72..76).contains(&at);
            if !rewritten {
                assert_eq!(was, now, "byte {at}");
            }
        }
        assert_eq!(long(&header.bytes, FSHD_SEG_LIST_BLOCKS), 133);
        assert_eq!(
            long(&header.bytes, 1),
            128,
            "SummedLongs as the card had it"
        );
        let link = only(&plan, Stage::Link);
        assert_eq!(
            (link.block, long(&link.bytes, RDSK_FILE_SYS_HEADER_LIST)),
            (0, 132)
        );
        assert!(
            plan.blocks()
                .iter()
                .all(|b| *b == 0 || (132..=260).contains(b)),
            "the old driver's blocks are not written"
        );
    }

    #[test]
    fn a_replace_after_another_driver_swaps_its_predecessors_next() {
        let range = two_drivers(DOS3, PDS3);
        let plan = replace(&range);
        let link = only(&plan, Stage::Link);
        assert_eq!((link.block, long(&link.bytes, NEXT)), (3, 23));
        assert_eq!(long(&only(&plan, Stage::Header).bytes, NEXT), NO_BLOCK);
    }

    #[test]
    fn a_replace_before_another_driver_carries_its_next() {
        let range = two_drivers(PDS3, SFS0);
        let plan = replace(&range);
        assert_eq!(long(&only(&plan, Stage::Header).bytes, NEXT), 13);
        let link = only(&plan, Stage::Link);
        assert_eq!(
            (link.block, long(&link.bytes, RDSK_FILE_SYS_HEADER_LIST)),
            (0, 23)
        );
    }

    #[test]
    fn two_fshds_of_the_target_dostype_are_refused() {
        let range = two_drivers(PDS3, PDS3);
        let err = plan_replace(
            &range,
            &walk_strict(&range).unwrap(),
            PDS3,
            (19, 3),
            &hunk_driver(62_604, "19.3"),
        )
        .unwrap_err();
        assert!(
            matches!(err, RdbEditRefusal::Unaccounted { block: 13, .. }),
            "{err:?}"
        );
        assert!(err.to_string().contains("two FSHDs carry PDS3"), "{err}");
    }
}
