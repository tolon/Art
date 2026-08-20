//! Deep structural checks: does the bitmap describe the files, and is every
//! entry linked into the bucket AmigaDOS will look in?
//!
//! `core/adf/validate.rs` answers the shallow questions — is there a boot
//! block, does its checksum match, is the root block a header block. Those are
//! cheap and they are what the health badge shows. They are also **not enough
//! for the write gate** (ART-050): an operation can leave a volume whose every
//! individual block is well-formed and whose root block is perfect, and which
//! AmigaDOS still cannot read, because
//!
//! - two files own the same block, so writing one destroys the other, or
//! - an entry sits in hash bucket 7 while its name hashes to 41, so it exists
//!   on the disk and no `Dir` will ever list it, or
//! - a block a file occupies is marked **free** in the bitmap, which is the
//!   same crossed-links failure one allocation later.
//!
//! This module walks the volume and says so. It is deliberately here and not
//! in `core/adf`: it needs [`crate::core::volume::write::bitmap::Allocator`]
//! to read a multi-page bitmap, and `core/volume` may depend on `core/adf`
//! but not the other way round (CLAUDE.md's inward-pointing layering rule).
//!
//! ## Findings, not errors
//!
//! Nothing here returns `Err`. A volume whose root block does not parse, whose
//! bitmap chain is broken, whose hash chain loops — all of that is *what the
//! caller asked about*, so it comes back as a finding with a severity. The one
//! thing a caller must never get is "the check itself failed, so we do not
//! know", because the gate above it would then have to decide what to do with
//! that and the safe answer would block every write.
//!
//! ## Bounded, always
//!
//! Every chain walk has a step limit and every visited block is remembered, so
//! a hostile or corrupt image ends the walk with a finding rather than running
//! forever (the ART-008 lesson). The walk is also capped by finding count: a
//! thoroughly wrecked volume produces a readable report, not a million lines.

use std::collections::{HashMap, HashSet};

use crate::core::adf::blocks::{
    block_subtype, block_type, HeaderBlock, RootBlock, HASH_TABLE_SIZE, MAX_INLINE_DATA_BLOCKS,
};
use crate::core::adf::hash::name_hash;
use crate::core::adf::validate::{HealthStatus, ValidationFinding};
use crate::core::volume::write::bitmap::{Allocator, BitmapLayout};
use crate::core::volume::{read_block_vec, BlockDevice, VolumeGeometry};

/// The most entries the walk will visit before it stops and says so.
///
/// A DD floppy holds at most ~1750 headers; the 16 MB whole-file ceiling is
/// 32 768 blocks, so nothing legitimate comes close. The cap exists because a
/// corrupt image can grow the tree without bound.
const MAX_ENTRIES: usize = 200_000;

/// The longest hash-bucket sibling chain the walk will follow.
const MAX_CHAIN: usize = 20_000;

/// The most extension blocks one file may chain.
const MAX_EXTENSIONS: usize = 20_000;

/// The most findings of any single code that are reported individually.
///
/// Past this the report says how many more there were, which is the number
/// that matters once a volume is this broken.
const MAX_PER_CODE: usize = 8;

/// Stable finding codes, so a caller can compare two reports by code rather
/// than by prose. They are compared verbatim by the write gate.
pub mod code {
    /// A block is claimed by two different files.
    pub const CROSSLINKED: &str = "blocks.crosslinked";
    /// A block a file occupies is marked free in the bitmap.
    pub const IN_USE_BUT_FREE: &str = "bitmap.in_use_but_free";
    /// A block nothing references is marked used in the bitmap.
    pub const LEAKED: &str = "bitmap.leaked";
    /// The bitmap could not be read at all.
    pub const BITMAP_UNREADABLE: &str = "bitmap.unreadable";
    /// An entry is linked into a bucket its name does not hash to.
    pub const WRONG_BUCKET: &str = "hashchain.bucket";
    /// A hash chain revisits a block, or does not end.
    pub const CHAIN_LOOP: &str = "hashchain.loop";
    /// An entry's `parent` field names a different directory than the one it
    /// is linked in.
    pub const WRONG_PARENT: &str = "hashchain.parent";
    /// A block reached through a chain does not parse as what the chain says
    /// it is.
    pub const BROKEN_LINK: &str = "structure.link";
    /// The walk hit one of its own limits and is incomplete.
    pub const TRUNCATED: &str = "structure.truncated";
}

/// Collects findings, folding repeats of the same code into a count.
#[derive(Default)]
struct Findings {
    out: Vec<ValidationFinding>,
    seen: HashMap<&'static str, usize>,
}

impl Findings {
    fn push(&mut self, severity: HealthStatus, code: &'static str, message: String) {
        let count = self.seen.entry(code).or_insert(0);
        *count += 1;
        if *count <= MAX_PER_CODE {
            self.out.push(ValidationFinding {
                severity,
                code: code.to_string(),
                message,
            });
        }
    }

    fn finish(mut self) -> Vec<ValidationFinding> {
        for (code, count) in self.seen.iter() {
            if *count > MAX_PER_CODE {
                self.out.push(ValidationFinding {
                    severity: HealthStatus::Warning,
                    code: format!("{code}.more"),
                    message: format!(
                        "{} further findings of the same kind were not listed individually.",
                        count - MAX_PER_CODE
                    ),
                });
            }
        }
        self.out
    }
}

/// What the walk found on disk, before it is compared with the bitmap.
struct Occupancy {
    /// Block number → the header block that claims it.
    owner: HashMap<u32, u32>,
}

impl Occupancy {
    fn new() -> Self {
        Self {
            owner: HashMap::new(),
        }
    }

    /// Record `block` as belonging to `owner`. Returns the previous owner when
    /// there was one, which is a cross-link.
    fn claim(&mut self, block: u32, owner: u32) -> Option<u32> {
        match self.owner.entry(block) {
            std::collections::hash_map::Entry::Occupied(existing) => Some(*existing.get()),
            std::collections::hash_map::Entry::Vacant(slot) => {
                slot.insert(owner);
                None
            }
        }
    }
}

/// Walk `device` and report what is structurally wrong with it.
///
/// `geometry` must be the volume's own — the block count and root block this
/// device covers, never a floppy's assumed 1760/880.
///
/// Returns an empty vector for a volume with nothing wrong. Severities follow
/// the same rule as the rest of validation: a `Problem` is something AmigaDOS
/// cannot cope with or that will destroy data on the next write; a `Warning`
/// is something wasteful or untidy that still reads correctly.
pub fn check(device: &dyn BlockDevice, geometry: &VolumeGeometry) -> Vec<ValidationFinding> {
    let mut findings = Findings::default();

    if !geometry.dos_type.is_browsable() {
        // Nothing below applies to a filesystem ART cannot walk. Saying
        // nothing is right: `validate_volume` already reports the filesystem
        // itself, and a second sentence about it would read as a second fault.
        return Vec::new();
    }

    let international = geometry.dos_type.is_international();
    let mut occupancy = Occupancy::new();

    // The system blocks. Reserved blocks and the root block are occupied by
    // definition and are not in the bitmap's remit (`bit_position` returns
    // `None` for them), so they are recorded but never compared.
    let root = geometry.root_block;

    // 1. The bitmap's own blocks are occupied by the bitmap — its pages, and
    //    the extension blocks that name them once the root block's 25 slots
    //    run out. Missing the extension blocks would report every one of them
    //    as leaked space on any volume past ~100 000 blocks, which is a
    //    finding about ART rather than about the disk.
    let layout = BitmapLayout::read(device, geometry);
    match &layout {
        Ok(layout) => {
            let mut system: Vec<u32> = layout.pages.clone();
            system.extend(bitmap_extension_blocks(device, geometry));
            for page in system {
                if let Some(previous) = occupancy.claim(page, root) {
                    findings.push(
                        HealthStatus::Problem,
                        code::CROSSLINKED,
                        format!(
                            "Bitmap block {page} is also claimed by block {previous}; \
                             two structures own the same block."
                        ),
                    );
                }
            }
        }
        Err(err) => findings.push(
            HealthStatus::Warning,
            code::BITMAP_UNREADABLE,
            format!("This volume's free-space map could not be read: {err}"),
        ),
    }

    // 2. Walk the directory tree, claiming every block a file or directory
    //    occupies and checking every link on the way.
    walk_tree(
        device,
        geometry,
        international,
        &mut occupancy,
        &mut findings,
    );

    // 3. Compare what the walk found with what the bitmap says.
    if layout.is_ok() {
        compare_with_bitmap(device, geometry, &occupancy, &mut findings);
    }

    findings.finish()
}

/// The blocks of the bitmap **extension** chain, which [`BitmapLayout`] reads
/// through but does not report.
///
/// Bounded the same way `BitmapLayout::read` bounds it, and silent on failure:
/// a chain this cannot follow has already been reported by the layout read
/// itself, and a second sentence about it would read as a second fault.
fn bitmap_extension_blocks(device: &dyn BlockDevice, geometry: &VolumeGeometry) -> Vec<u32> {
    const MAX_HOPS: usize = 64;
    let Ok(root) = read_block_vec(device, geometry.root_block) else {
        return Vec::new();
    };
    let mut next = u32::from_be_bytes([root[416], root[417], root[418], root[419]]);
    let mut blocks = Vec::new();
    let mut seen: HashSet<u32> = HashSet::new();

    while next != 0 && blocks.len() < MAX_HOPS {
        if next >= geometry.total_blocks || !seen.insert(next) {
            break;
        }
        blocks.push(next);
        let Ok(raw) = read_block_vec(device, next) else {
            break;
        };
        let last = geometry.block_size - 4;
        next = u32::from_be_bytes([raw[last], raw[last + 1], raw[last + 2], raw[last + 3]]);
    }
    blocks
}

/// Breadth-first over the directory tree, from the root block outwards.
fn walk_tree(
    device: &dyn BlockDevice,
    geometry: &VolumeGeometry,
    international: bool,
    occupancy: &mut Occupancy,
    findings: &mut Findings,
) {
    let mut queue = vec![geometry.root_block];
    let mut visited_dirs: HashSet<u32> = HashSet::new();
    visited_dirs.insert(geometry.root_block);
    let mut entries = 0usize;

    while let Some(dir) = queue.pop() {
        let Some(buckets) = read_hash_table(device, geometry, dir, findings) else {
            continue;
        };

        for (bucket, &head) in buckets.iter().enumerate() {
            let mut next = head;
            let mut steps = 0usize;
            let mut seen: HashSet<u32> = HashSet::new();

            while next != 0 {
                steps += 1;
                if steps > MAX_CHAIN || !seen.insert(next) {
                    findings.push(
                        HealthStatus::Problem,
                        code::CHAIN_LOOP,
                        format!(
                            "The chain of entries in bucket {bucket} of directory block {dir} \
                             revisits block {next}; AmigaDOS would loop reading this directory."
                        ),
                    );
                    break;
                }
                entries += 1;
                if entries > MAX_ENTRIES {
                    findings.push(
                        HealthStatus::Warning,
                        code::TRUNCATED,
                        format!(
                            "Stopped after {MAX_ENTRIES} entries; this report describes only \
                             part of the volume."
                        ),
                    );
                    return;
                }

                let block = next;
                let Some(header) = read_header(device, geometry, block, findings) else {
                    break;
                };
                next = header.next_hash;

                // The bucket an entry is *in* must be the bucket its name
                // hashes *to*. This is the check ART-050 exists for: a
                // mismatch is a file that is on the disk and that no AmigaDOS
                // `Dir`, and no `Open()`, will ever find.
                let expected = name_hash(&header.name, international) as usize;
                if expected != bucket {
                    findings.push(
                        HealthStatus::Problem,
                        code::WRONG_BUCKET,
                        format!(
                            "'{}' (block {block}) is linked in hash bucket {bucket}, but its \
                             name hashes to {expected}; AmigaDOS would not find it.",
                            header.name
                        ),
                    );
                }

                if header.parent != dir {
                    findings.push(
                        HealthStatus::Warning,
                        code::WRONG_PARENT,
                        format!(
                            "'{}' (block {block}) is listed in directory block {dir} but its \
                             parent field names block {}.",
                            header.name, header.parent
                        ),
                    );
                }

                if let Some(previous) = occupancy.claim(block, block) {
                    findings.push(
                        HealthStatus::Problem,
                        code::CROSSLINKED,
                        format!(
                            "Header block {block} ('{}') is also claimed by block {previous}.",
                            header.name
                        ),
                    );
                }

                match header.kind {
                    crate::core::adf::blocks::EntryKind::Directory => {
                        if visited_dirs.insert(block) {
                            queue.push(block);
                        }
                    }
                    crate::core::adf::blocks::EntryKind::File => {
                        claim_file_blocks(device, geometry, &header, block, occupancy, findings);
                    }
                }
            }
        }
    }
}

/// The 72 hash-table longwords of a root or directory block.
///
/// Both live at the same offset; only the surrounding fields differ, which is
/// why this reads the raw block rather than choosing a parser by subtype.
fn read_hash_table(
    device: &dyn BlockDevice,
    geometry: &VolumeGeometry,
    block: u32,
    findings: &mut Findings,
) -> Option<[u32; HASH_TABLE_SIZE]> {
    if block >= geometry.total_blocks {
        findings.push(
            HealthStatus::Problem,
            code::BROKEN_LINK,
            format!("Directory block {block} is outside this volume."),
        );
        return None;
    }
    let raw = match read_block_vec(device, block) {
        Ok(raw) => raw,
        Err(err) => {
            findings.push(
                HealthStatus::Problem,
                code::BROKEN_LINK,
                format!("Directory block {block} could not be read: {err}"),
            );
            return None;
        }
    };

    // `RootBlock::parse` reads exactly the hash table plus root-only fields
    // and rejects a non-header block, which is what a directory needs too.
    match RootBlock::parse(&raw) {
        Ok(parsed) => Some(parsed.hash_table),
        Err(err) => {
            findings.push(
                HealthStatus::Problem,
                code::BROKEN_LINK,
                format!("Block {block} is linked as a directory but does not parse as one: {err}"),
            );
            None
        }
    }
}

fn read_header(
    device: &dyn BlockDevice,
    geometry: &VolumeGeometry,
    block: u32,
    findings: &mut Findings,
) -> Option<HeaderBlock> {
    if block >= geometry.total_blocks {
        findings.push(
            HealthStatus::Problem,
            code::BROKEN_LINK,
            format!("Entry block {block} is outside this volume."),
        );
        return None;
    }
    let raw = match read_block_vec(device, block) {
        Ok(raw) => raw,
        Err(err) => {
            findings.push(
                HealthStatus::Problem,
                code::BROKEN_LINK,
                format!("Entry block {block} could not be read: {err}"),
            );
            return None;
        }
    };
    match HeaderBlock::parse(&raw) {
        Ok(header) => Some(header),
        Err(err) => {
            findings.push(
                HealthStatus::Problem,
                code::BROKEN_LINK,
                format!("Block {block} is linked as an entry but is not a header block: {err}"),
            );
            None
        }
    }
}

/// Claim every data block and extension block one file occupies.
///
/// Reads the header's own data-block list and then follows the extension
/// chain, which is the shape both OFS and FFS share — an FFS data block has no
/// header of its own to walk, so the pointer lists are the only truth.
fn claim_file_blocks(
    device: &dyn BlockDevice,
    geometry: &VolumeGeometry,
    header: &HeaderBlock,
    owner: u32,
    occupancy: &mut Occupancy,
    findings: &mut Findings,
) {
    let claim = |block: u32, occupancy: &mut Occupancy, findings: &mut Findings| {
        if block == 0 || block >= geometry.total_blocks {
            findings.push(
                HealthStatus::Problem,
                code::BROKEN_LINK,
                format!(
                    "'{}' (block {owner}) points at block {block}, which is outside this volume.",
                    header.name
                ),
            );
            return;
        }
        if let Some(previous) = occupancy.claim(block, owner) {
            findings.push(
                HealthStatus::Problem,
                code::CROSSLINKED,
                format!(
                    "Block {block} belongs to '{}' (block {owner}) and to block {previous}; \
                     writing to one would destroy the other.",
                    header.name
                ),
            );
        }
    };

    for &data in &header.data_blocks {
        claim(data, occupancy, findings);
    }

    let mut next = header.extension;
    let mut steps = 0usize;
    let mut seen: HashSet<u32> = HashSet::new();
    while next != 0 {
        steps += 1;
        if steps > MAX_EXTENSIONS || !seen.insert(next) {
            findings.push(
                HealthStatus::Problem,
                code::CHAIN_LOOP,
                format!(
                    "The extension chain of '{}' (block {owner}) revisits block {next}.",
                    header.name
                ),
            );
            return;
        }
        claim(next, occupancy, findings);
        if next >= geometry.total_blocks {
            return;
        }
        let raw = match read_block_vec(device, next) {
            Ok(raw) => raw,
            Err(_) => return,
        };
        // An extension block is a T_LIST block carrying the same reversed
        // pointer array a header does, at the same offsets.
        let typ = i32::from_be_bytes([raw[0], raw[1], raw[2], raw[3]]);
        if typ != block_type::LIST {
            findings.push(
                HealthStatus::Problem,
                code::BROKEN_LINK,
                format!(
                    "Block {next} is '{}''s extension block but has type {typ}, not \
                     {} (T_LIST).",
                    header.name,
                    block_type::LIST
                ),
            );
            return;
        }
        let count = u32::from_be_bytes([raw[8], raw[9], raw[10], raw[11]]) as usize;
        for index in 0..count.min(MAX_INLINE_DATA_BLOCKS) {
            let offset = (77 - index) * 4;
            let pointer = u32::from_be_bytes([
                raw[offset],
                raw[offset + 1],
                raw[offset + 2],
                raw[offset + 3],
            ]);
            if pointer != 0 {
                claim(pointer, occupancy, findings);
            }
        }
        next = u32::from_be_bytes([raw[504], raw[505], raw[506], raw[507]]);
    }
}

/// The two directions of bitmap disagreement, which are not equally serious.
///
/// A block a file occupies that the bitmap calls **free** is a `Problem`: the
/// next allocation hands it out and the file loses its contents. A block that
/// nothing references and the bitmap calls **used** is a `Warning`: it wastes
/// space and reads perfectly, and refusing every write to a volume that leaked
/// a block years ago would lock the user out of their own disk (§89).
fn compare_with_bitmap(
    device: &dyn BlockDevice,
    geometry: &VolumeGeometry,
    occupancy: &Occupancy,
    findings: &mut Findings,
) {
    let allocator = match Allocator::load(device, geometry) {
        Ok(allocator) => allocator,
        Err(err) => {
            findings.push(
                HealthStatus::Warning,
                code::BITMAP_UNREADABLE,
                format!("This volume's free-space map could not be loaded: {err}"),
            );
            return;
        }
    };

    // On a `DOS\4`/`DOS\5` volume every directory also owns a chain of
    // directory-cache blocks, and the walk above does not follow them because
    // nothing in ART reads them. Reporting each of those as leaked space would
    // be a statement about ART's coverage dressed up as a statement about the
    // user's disk, so the leak direction is not asked on those volumes. The
    // *other* direction still is: a block a file occupies and the map calls
    // free is wrong on any flavour.
    let report_leaks = !geometry.dos_type.has_dircache();

    for block in geometry.reserved..geometry.total_blocks {
        if block == geometry.root_block {
            continue;
        }
        let occupied = occupancy.owner.contains_key(&block);
        let free = allocator.is_free(block);
        if !occupied && !free && !report_leaks {
            continue;
        }
        match (occupied, free) {
            (true, true) => findings.push(
                HealthStatus::Problem,
                code::IN_USE_BUT_FREE,
                format!(
                    "Block {block} is part of a file but the free-space map calls it free; \
                     the next file written here would be given it."
                ),
            ),
            (false, false) => findings.push(
                HealthStatus::Warning,
                code::LEAKED,
                format!(
                    "Block {block} is marked used but no file references it; \
                     the space is unusable until the map is rebuilt."
                ),
            ),
            _ => {}
        }
    }
}

/// The findings of `after` that were not already true of `before`.
///
/// The write gate's question is not "is this volume perfect" but "did **this
/// operation** break it". A volume a user has carried since 1993 may well have
/// a leaked block or an entry in the wrong bucket already; refusing to write
/// to it on that ground would take their disk away from them rather than
/// protect it. So the gate compares, and only a finding the operation
/// *introduced* refuses it.
///
/// Compared by `(code, message)` rather than by code alone, deliberately: a
/// volume that already had one cross-link and now has two must still refuse.
pub fn newly_broken(before: &[ValidationFinding], after: &[ValidationFinding]) -> Vec<String> {
    let existing: HashSet<(&str, &str)> = before
        .iter()
        .map(|finding| (finding.code.as_str(), finding.message.as_str()))
        .collect();

    after
        .iter()
        .filter(|finding| finding.severity == HealthStatus::Problem)
        .filter(|finding| !existing.contains(&(finding.code.as_str(), finding.message.as_str())))
        .map(|finding| format!("{} ({})", finding.message, finding.code))
        .collect()
}

/// Whether a header block's subtype says "directory", for callers that have
/// the raw block rather than a parsed one.
#[allow(dead_code)]
pub fn is_directory_subtype(subtype: i32) -> bool {
    subtype == block_subtype::ROOT
        || subtype == block_subtype::USERDIR
        || subtype == block_subtype::LINKDIR
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::adf::blocks::bit_position;
    use crate::core::volume::device::SliceDevice;
    use crate::core::volume::fixture::{
        checksum_block, checksum_block_at, make_ffs_volume, FixtureFile, BLOCK,
    };
    use crate::core::volume::DosType;

    const TOTAL: u32 = 1760;
    const ROOT: u32 = TOTAL / 2;
    const RESERVED: u32 = 2;

    fn geometry() -> VolumeGeometry {
        VolumeGeometry::new(BLOCK, TOTAL, RESERVED, DosType(*b"DOS\x01")).unwrap()
    }

    fn two_file_volume() -> Vec<u8> {
        make_ffs_volume(
            TOTAL,
            "Work",
            &[
                FixtureFile {
                    name: "Hello",
                    content: b"one block of text",
                },
                FixtureFile {
                    name: "Second",
                    content: b"another block",
                },
            ],
        )
    }

    fn check_bytes(image: &[u8]) -> Vec<ValidationFinding> {
        let device = SliceDevice::new(image, BLOCK).unwrap();
        check(&device, &geometry())
    }

    fn codes(findings: &[ValidationFinding]) -> Vec<String> {
        findings.iter().map(|f| f.code.clone()).collect()
    }

    fn put_u32(image: &mut [u8], offset: usize, value: u32) {
        image[offset..offset + 4].copy_from_slice(&value.to_be_bytes());
    }

    fn get_u32(image: &[u8], offset: usize) -> u32 {
        u32::from_be_bytes(image[offset..offset + 4].try_into().unwrap())
    }

    /// The first bitmap block of a fixture volume: the block after the root.
    fn bitmap_block() -> u32 {
        ROOT + 1
    }

    /// Flip `block`'s bit to "free" in the fixture's single bitmap block, and
    /// re-checksum it so the map still parses.
    fn mark_free(image: &mut [u8], block: u32) {
        let position = bit_position(block, RESERVED, TOTAL).expect("in the bitmap's range");
        assert_eq!(position.bitmap_index, 0, "one bitmap block on a DD floppy");
        let off = bitmap_block() as usize * BLOCK + position.byte_offset;
        let lw = get_u32(image, off) | position.mask;
        put_u32(image, off, lw);
        checksum_block_at(image, bitmap_block(), 0);
    }

    /// Clear `block`'s bit, marking it used by nothing.
    fn mark_used(image: &mut [u8], block: u32) {
        let position = bit_position(block, RESERVED, TOTAL).expect("in the bitmap's range");
        let off = bitmap_block() as usize * BLOCK + position.byte_offset;
        let lw = get_u32(image, off) & !position.mask;
        put_u32(image, off, lw);
        checksum_block_at(image, bitmap_block(), 0);
    }

    /// Which of the root's 72 buckets holds `block`, and the bucket's offset.
    fn bucket_of(image: &[u8], block: u32) -> usize {
        let root_off = ROOT as usize * BLOCK;
        (0..HASH_TABLE_SIZE)
            .find(|index| get_u32(image, root_off + 24 + index * 4) == block)
            .expect("the entry must be linked in some bucket")
    }

    fn header_block_of(image: &[u8], name: &str) -> u32 {
        let root_off = ROOT as usize * BLOCK;
        let bucket = name_hash(name, false) as usize;
        let block = get_u32(image, root_off + 24 + bucket * 4);
        assert_ne!(block, 0, "'{name}' must be linked in bucket {bucket}");
        block
    }

    // -- the baseline ------------------------------------------------------
    //
    // Every corruption test below is only worth anything because this one
    // passes: the fixture is a volume with nothing wrong with it, so a finding
    // in any of them came from the corruption and not from the fixture.

    #[test]
    fn a_clean_volume_has_nothing_to_report() {
        let image = two_file_volume();
        let findings = check_bytes(&image);
        assert!(findings.is_empty(), "{findings:?}");
    }

    #[test]
    fn a_clean_volume_with_a_multi_page_bitmap_has_nothing_to_report() {
        // 200 000 blocks needs 50 bitmap pages: 25 in the root block and the
        // rest in an extension block. Those extension blocks are marked used
        // and referenced by nothing the tree walk sees, so a check that did
        // not know about them would call every one of them a leak.
        let total = 200_000u32;
        let image = make_ffs_volume(total, "Big", &[]);
        let geometry = VolumeGeometry::new(BLOCK, total, RESERVED, DosType(*b"DOS\x01")).unwrap();
        let device = SliceDevice::new(&image, BLOCK).unwrap();
        let findings = check(&device, &geometry);
        assert!(findings.is_empty(), "{findings:?}");
    }

    // -- bitmap consistency ------------------------------------------------

    #[test]
    fn a_block_a_file_occupies_that_the_map_calls_free_is_a_problem() {
        let mut image = two_file_volume();
        let header = header_block_of(&image, "Hello");
        let data = get_u32(&image, header as usize * BLOCK + 16);
        assert_ne!(data, 0);

        mark_free(&mut image, data);

        let findings = check_bytes(&image);
        let finding = findings
            .iter()
            .find(|f| f.code == code::IN_USE_BUT_FREE)
            .unwrap_or_else(|| panic!("expected {}: {findings:?}", code::IN_USE_BUT_FREE));
        assert_eq!(finding.severity, HealthStatus::Problem);
        assert!(finding.message.contains(&data.to_string()));
    }

    #[test]
    fn a_block_nothing_references_that_the_map_calls_used_is_only_a_warning() {
        let mut image = two_file_volume();
        // A block well past everything the fixture placed.
        mark_used(&mut image, TOTAL - 3);

        let findings = check_bytes(&image);
        let finding = findings
            .iter()
            .find(|f| f.code == code::LEAKED)
            .unwrap_or_else(|| panic!("expected {}: {findings:?}", code::LEAKED));
        assert_eq!(
            finding.severity,
            HealthStatus::Warning,
            "a leaked block reads perfectly; refusing every write to a volume that \
             leaked one would take the user's disk away from them"
        );
        assert!(
            newly_broken(&[], &findings).is_empty(),
            "a warning must never reach the write gate's refusal list"
        );
    }

    // -- hash-chain integrity ----------------------------------------------

    #[test]
    fn an_entry_linked_in_the_wrong_bucket_is_a_problem() {
        let mut image = two_file_volume();
        let header = header_block_of(&image, "Hello");
        let root_off = ROOT as usize * BLOCK;
        let right = bucket_of(&image, header);
        let wrong = (right + 1) % HASH_TABLE_SIZE;
        assert_eq!(get_u32(&image, root_off + 24 + wrong * 4), 0, "free bucket");

        put_u32(&mut image, root_off + 24 + right * 4, 0);
        put_u32(&mut image, root_off + 24 + wrong * 4, header);
        checksum_block(&mut image, ROOT);

        let findings = check_bytes(&image);
        let finding = findings
            .iter()
            .find(|f| f.code == code::WRONG_BUCKET)
            .unwrap_or_else(|| panic!("expected {}: {findings:?}", code::WRONG_BUCKET));
        assert_eq!(finding.severity, HealthStatus::Problem);
        assert!(finding.message.contains("Hello"), "{}", finding.message);
    }

    #[test]
    fn a_hash_chain_that_points_at_itself_ends_with_a_finding_rather_than_a_hang() {
        let mut image = two_file_volume();
        let header = header_block_of(&image, "Hello");
        // next_hash at offset 496, pointing back at this very block.
        put_u32(&mut image, header as usize * BLOCK + 496, header);
        checksum_block(&mut image, header);

        let findings = check_bytes(&image);
        assert!(
            codes(&findings).iter().any(|c| c == code::CHAIN_LOOP),
            "{findings:?}"
        );
    }

    // -- cross-links -------------------------------------------------------

    #[test]
    fn two_files_owning_the_same_block_is_a_problem() {
        let mut image = two_file_volume();
        let first = header_block_of(&image, "Hello");
        let second = header_block_of(&image, "Second");
        let shared = get_u32(&image, first as usize * BLOCK + 16);
        assert_ne!(shared, 0);

        // Point the second file's only data pointer at the first file's block,
        // and free the block it used to own so the bitmap still agrees about
        // everything except the thing under test.
        let owned = get_u32(&image, second as usize * BLOCK + 16);
        put_u32(&mut image, second as usize * BLOCK + 16, shared);
        put_u32(&mut image, second as usize * BLOCK + 77 * 4, shared);
        checksum_block(&mut image, second);
        mark_free(&mut image, owned);

        let findings = check_bytes(&image);
        let finding = findings
            .iter()
            .find(|f| f.code == code::CROSSLINKED)
            .unwrap_or_else(|| panic!("expected {}: {findings:?}", code::CROSSLINKED));
        assert_eq!(finding.severity, HealthStatus::Problem);
        assert!(finding.message.contains(&shared.to_string()));
    }

    // -- what the gate does with the findings -------------------------------

    #[test]
    fn newly_broken_reports_only_what_the_operation_introduced() {
        let mut image = two_file_volume();
        let header = header_block_of(&image, "Hello");
        let root_off = ROOT as usize * BLOCK;
        let right = bucket_of(&image, header);
        let wrong = (right + 1) % HASH_TABLE_SIZE;
        put_u32(&mut image, root_off + 24 + right * 4, 0);
        put_u32(&mut image, root_off + 24 + wrong * 4, header);
        checksum_block(&mut image, ROOT);

        let before = check_bytes(&image);
        assert!(!before.is_empty());

        // The very same volume, unchanged: an operation that introduced
        // nothing must be allowed through even though the volume is broken.
        assert!(
            newly_broken(&before, &before).is_empty(),
            "a volume that was already like this must still be writable"
        );

        // Now break it further, the way a bad operation would.
        let mut worse = image.clone();
        let second = header_block_of(&worse, "Second");
        let shared = get_u32(&worse, header as usize * BLOCK + 16);
        put_u32(&mut worse, second as usize * BLOCK + 16, shared);
        put_u32(&mut worse, second as usize * BLOCK + 77 * 4, shared);
        checksum_block(&mut worse, second);

        let after = check_bytes(&worse);
        let new = newly_broken(&before, &after);
        assert!(
            new.iter().any(|line| line.contains(code::CROSSLINKED)),
            "the cross-link this operation introduced must refuse it: {new:?}"
        );
    }

    #[test]
    fn a_filesystem_art_cannot_walk_produces_no_findings_rather_than_false_ones() {
        let image = two_file_volume();
        // PFS3: not an AmigaDOS volume at all. Walking it with AmigaDOS block
        // parsers would produce nonsense findings about a healthy disk.
        let geometry = VolumeGeometry::new(BLOCK, TOTAL, RESERVED, DosType(*b"PFS\x03")).unwrap();
        let device = SliceDevice::new(&image, BLOCK).unwrap();
        assert!(check(&device, &geometry).is_empty());
    }

    #[test]
    fn repeats_of_one_finding_are_summarised_rather_than_listed_forever() {
        let mut image = two_file_volume();
        for block in (TOTAL - 40)..(TOTAL - 5) {
            mark_used(&mut image, block);
        }
        let findings = check_bytes(&image);
        let listed = findings.iter().filter(|f| f.code == code::LEAKED).count();
        assert_eq!(listed, MAX_PER_CODE);
        assert!(
            findings
                .iter()
                .any(|f| f.code == format!("{}.more", code::LEAKED)),
            "{findings:?}"
        );
    }
}
