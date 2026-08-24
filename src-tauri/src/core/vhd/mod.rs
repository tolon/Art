//! Virtual Hard Disk (VHD) footers — what a file claims to be about itself.
//!
//! Work-list item 7's first half. **The evidence is one of the owner's own
//! files**: `AmiKit.hdf`, 1 200 776 704 bytes, beginning with the eight bytes
//! `conectix`. It is a *dynamic* VHD wearing an `.hdf` extension, and it is
//! what one shipped Amiga distribution hands its users — so this is not a
//! format ART might one day meet.
//!
//! # Why it matters, and it is not that ART would corrupt anything
//!
//! ART would not write garbage into it: the hard-disk studio reads offset 0,
//! finds no `RDSK`, and says so. But *what it says* is **"this hard disk has
//! no partition table"** about a file that has one — a true sentence about
//! the bytes at offset 0 and a wrong sentence about the disk, which is
//! exactly this project's most expensive class of defect. A dynamic VHD does
//! not store its data at offset 0 at all: offset 0 is a **copy of the footer**,
//! then a dynamic disk header, then a block allocation table, then blocks.
//!
//! A **fixed** VHD is different and the difference is load-bearing: it is a
//! raw disk image with a 512-byte footer *appended*. ART's readers work on one
//! unchanged, because everything they look at is before the footer. So the two
//! kinds get two answers rather than one, and only the dynamic kind is kept
//! away from the raw studios.
//!
//! # The format, checked rather than recalled
//!
//! Read on 2026-08-24 from libyal's `libvhdi` format documentation
//! (*Virtual Hard Disk (VHD) image format*), cross-checked against
//! VirtualBox's `src/VBox/Storage/VHD.cpp` and Microsoft's own VHD notes:
//!
//! - The footer is **512 bytes** and every value in it is **big-endian**.
//! - A **fixed** image is `data`, then the footer — at the **end only**.
//! - A **dynamic** image stores a **copy of the footer at offset 0**, then the
//!   dynamic disk header (cookie `cxsparse`), the block allocation table, and
//!   the blocks; the original footer is still at the end.
//! - Footer layout: cookie `[0..8]`, features `[8..12]`, format version
//!   `[12..16]`, data offset `[16..24]`, modification time `[24..28]`, creator
//!   application `[28..32]`, creator version `[32..36]`, creator OS `[36..40]`,
//!   disk size `[40..48]`, data size `[48..56]`, disk geometry `[56..60]`,
//!   **disk type `[60..64]`**, checksum `[64..68]`, identifier `[68..84]`,
//!   saved state `[84]`.
//! - Disk type: **2** fixed, **3** dynamic, **4** differencing.
//! - Data offset is `0xFFFF_FFFF_FFFF_FFFF` for a fixed image and the offset
//!   of the dynamic disk header otherwise.
//!
//! # What this module will not do
//!
//! It does not read a dynamic VHD's contents. Detection says what the file is
//! and the studios stay away from it; a block-allocation-table reader is a
//! separate piece of work, and offering one that does not exist would be the
//! §89 promise this project does not make.
//!
//! **The checksum is computed and reported, never used to reject.** ART has
//! one real VHD to check the arithmetic against and it belongs to the owner;
//! a detection that refused a valid file because this module's arithmetic was
//! wrong would be worse than one that does not check.

pub mod write;

pub use write::DynamicVhd;

use serde::Serialize;

/// The eight bytes every VHD footer begins with.
pub const FOOTER_COOKIE: [u8; 8] = *b"conectix";

/// The eight bytes a dynamic disk header begins with — the second structure
/// in a dynamic image, at the offset the footer's data-offset field gives.
pub const DYNAMIC_HEADER_COOKIE: [u8; 8] = *b"cxsparse";

/// A VHD footer is always exactly this long, whichever kind of image.
pub const FOOTER_LEN: usize = 512;

/// The data-offset value a fixed image carries, meaning "there is nothing
/// after me".
pub const NO_DATA_OFFSET: u64 = u64::MAX;

/// Which of the three kinds of VHD a footer says it belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum VhdKind {
    /// Raw disk image with the footer appended. **ART's readers work on one
    /// unchanged** — everything they look at is before the footer.
    Fixed,
    /// Header, block allocation table and blocks. Offset 0 is a copy of the
    /// footer, *not* the disk's first sector.
    Dynamic,
    /// Like dynamic, and its blocks are deltas against a parent image.
    Differencing,
    /// A disk-type value the specification does not name. Reported as itself
    /// rather than folded into one of the three: a file claiming type 9 is
    /// not a fixed image, and guessing it is would be the thing this module
    /// exists to stop.
    Unrecognised(u32),
}

impl VhdKind {
    /// Whether the disk's own first sector really is at file offset 0.
    ///
    /// The one question the rest of ART needs answered. `false` for anything
    /// unrecognised — a kind nobody has read the specification for is not a
    /// kind whose layout can be assumed.
    pub fn data_starts_at_offset_zero(self) -> bool {
        matches!(self, Self::Fixed)
    }

    fn from_field(value: u32) -> Self {
        match value {
            2 => Self::Fixed,
            3 => Self::Dynamic,
            4 => Self::Differencing,
            other => Self::Unrecognised(other),
        }
    }
}

/// What a VHD footer says.
///
/// Only the fields something in ART reads. The rest of the 512 bytes —
/// timestamps, the creator application, the geometry, the identifier — is
/// left unparsed rather than carried around unread.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VhdFooter {
    pub kind: VhdKind,
    /// Where the dynamic disk header begins, or [`NO_DATA_OFFSET`] for a
    /// fixed image.
    pub data_offset: u64,
    /// The size of the disk the image represents — which for a dynamic image
    /// is **not** the size of the file.
    pub disk_size: u64,
    /// The footer's own format version, `0x0001_0000` for every VHD in the
    /// wild. Carried so a future version can be reported rather than assumed
    /// compatible.
    pub format_version: u32,
    /// Whether the stored checksum matches the one computed over these bytes.
    ///
    /// **Reported, never used to reject** — see the module doc.
    pub checksum_matches: bool,
}

/// The checksum a footer should carry: the one's complement of the sum of
/// every byte in it, with the checksum field itself taken as zero.
///
/// Takes the whole 512 bytes and skips `[64..68]` internally rather than
/// asking the caller to zero them, because a caller who forgot would get a
/// plausible wrong number instead of an error.
pub fn checksum(footer: &[u8]) -> u32 {
    let mut sum: u32 = 0;
    for (i, byte) in footer.iter().enumerate().take(FOOTER_LEN) {
        if (64..68).contains(&i) {
            continue;
        }
        sum = sum.wrapping_add(u32::from(*byte));
    }
    !sum
}

/// Read a footer out of exactly the bytes that should hold one.
///
/// `None` when `bytes` is too short or does not begin with
/// [`FOOTER_COOKIE`] — both are "this is not a VHD footer", which is an
/// answer rather than an error, the same way [`super::osinstall::chain`]'s
/// `describe_tree` answers rather than failing.
pub fn parse_footer(bytes: &[u8]) -> Option<VhdFooter> {
    if bytes.len() < FOOTER_LEN || bytes[0..8] != FOOTER_COOKIE {
        return None;
    }
    let be32 = |at: usize| u32::from_be_bytes(bytes[at..at + 4].try_into().expect("4 bytes"));
    let be64 = |at: usize| u64::from_be_bytes(bytes[at..at + 8].try_into().expect("8 bytes"));

    Some(VhdFooter {
        kind: VhdKind::from_field(be32(60)),
        data_offset: be64(16),
        disk_size: be64(40),
        format_version: be32(12),
        checksum_matches: be32(64) == checksum(bytes),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A footer built the way the specification describes one, so the tests
    /// below are about this module's reading rather than about a fixture
    /// somebody typed out.
    fn footer(kind: u32, data_offset: u64, disk_size: u64) -> Vec<u8> {
        let mut bytes = vec![0u8; FOOTER_LEN];
        bytes[0..8].copy_from_slice(&FOOTER_COOKIE);
        bytes[8..12].copy_from_slice(&0x0000_0002u32.to_be_bytes()); // features
        bytes[12..16].copy_from_slice(&0x0001_0000u32.to_be_bytes()); // version
        bytes[16..24].copy_from_slice(&data_offset.to_be_bytes());
        bytes[40..48].copy_from_slice(&disk_size.to_be_bytes());
        bytes[48..56].copy_from_slice(&disk_size.to_be_bytes());
        bytes[60..64].copy_from_slice(&kind.to_be_bytes());
        let sum = checksum(&bytes);
        bytes[64..68].copy_from_slice(&sum.to_be_bytes());
        bytes
    }

    /// **The real file this module was written for.** The owner's
    /// `AmiKit.hdf` — 1 200 776 704 bytes beginning `conectix`, a dynamic VHD
    /// shipped under an `.hdf` name by a real Amiga distribution.
    ///
    /// Permanent and `#[ignore]`d, like `archive::compress`'s 7-Zip hook and
    /// the OS-install hooks: ART ships no copyrighted Amiga content, so the
    /// synthetic fixtures above are what CI runs and this is what says the
    /// synthetic ones describe the world. **Read-only.**
    ///
    /// It is also the only way to find out whether [`checksum`] is right.
    /// Nothing rejects a footer over it, precisely because the arithmetic has
    /// never met a file somebody else wrote — so this prints the answer
    /// rather than asserting it, and the day it runs is the day that note in
    /// the module doc can be revisited.
    #[test]
    #[ignore = "needs a real VHD; set ART_VHD_IN to one (e.g. the owner's AmiKit.hdf)"]
    fn a_real_vhd_reads_the_way_the_synthetic_ones_do() {
        let Ok(path) = std::env::var("ART_VHD_IN") else {
            return;
        };
        let path = std::path::PathBuf::from(path);
        let size = std::fs::metadata(&path).expect("the file").len();
        println!("{} is {size} bytes", path.display());

        let head = read_window(&path, 0);
        let tail = read_window(&path, size - FOOTER_LEN as u64);

        let at_zero = parse_footer(&head);
        let at_end = parse_footer(&tail);
        println!("  footer at offset 0: {at_zero:?}");
        println!("  footer at the end:  {at_end:?}");

        let footer = at_zero
            .clone()
            .or_else(|| at_end.clone())
            .expect("ART_VHD_IN must name a VHD");

        // The one thing worth asserting: whichever copy was read, the two
        // agree about what the file is. A dynamic image keeps the same footer
        // in both places, and a disagreement would mean this module is
        // reading one of them wrongly.
        if let (Some(head), Some(end)) = (at_zero, at_end) {
            assert_eq!(head.kind, end.kind, "the two copies must agree");
            assert_eq!(head.disk_size, end.disk_size);
        }
        println!(
            "  kind={:?} disk_size={} checksum_matches={}",
            footer.kind, footer.disk_size, footer.checksum_matches
        );
        assert!(
            footer.checksum_matches,
            "the stored checksum disagrees with `checksum` here, which means the arithmetic is wrong rather than the file"
        );
    }

    fn read_window(path: &std::path::Path, offset: u64) -> Vec<u8> {
        use std::io::{Read, Seek, SeekFrom};
        let mut file = std::fs::File::open(path).expect("open");
        file.seek(SeekFrom::Start(offset)).expect("seek");
        let mut buf = vec![0u8; FOOTER_LEN];
        let read = file.read(&mut buf).expect("read");
        buf.truncate(read);
        buf
    }

    #[test]
    fn a_dynamic_footer_says_dynamic_and_where_its_header_is() {
        let read = parse_footer(&footer(3, 512, 4 * 1024 * 1024 * 1024)).expect("a VHD footer");
        assert_eq!(read.kind, VhdKind::Dynamic);
        assert_eq!(read.data_offset, 512);
        assert_eq!(read.disk_size, 4 * 1024 * 1024 * 1024);
        assert!(read.checksum_matches);
    }

    #[test]
    fn a_fixed_footer_says_fixed_and_that_nothing_follows_it() {
        let read = parse_footer(&footer(2, NO_DATA_OFFSET, 1024)).expect("a VHD footer");
        assert_eq!(read.kind, VhdKind::Fixed);
        assert_eq!(read.data_offset, NO_DATA_OFFSET);
    }

    /// **The question the rest of ART asks**, and the reason the two kinds are
    /// not one answer: a fixed image's first sector is the disk's first
    /// sector, and a dynamic image's is a copy of its own footer.
    #[test]
    fn only_a_fixed_image_has_its_data_at_offset_zero() {
        assert!(VhdKind::Fixed.data_starts_at_offset_zero());
        assert!(!VhdKind::Dynamic.data_starts_at_offset_zero());
        assert!(!VhdKind::Differencing.data_starts_at_offset_zero());
        assert!(!VhdKind::Unrecognised(9).data_starts_at_offset_zero());
    }

    /// A type nobody has read the specification for is reported as itself.
    /// Folding it into `Fixed` would be ART guessing a layout, which is the
    /// thing this module exists to stop.
    #[test]
    fn an_unnamed_disk_type_is_reported_rather_than_guessed() {
        let read = parse_footer(&footer(9, 512, 1024)).expect("still a VHD footer");
        assert_eq!(read.kind, VhdKind::Unrecognised(9));
    }

    #[test]
    fn something_that_is_not_a_footer_is_not_read_as_one() {
        assert!(parse_footer(&[0u8; FOOTER_LEN]).is_none());
        assert!(parse_footer(b"RDSK").is_none(), "an Amiga RDB is not a VHD");
        assert!(parse_footer(&[]).is_none());
    }

    /// Truncation is not a partial answer. A 511-byte tail cannot hold a
    /// footer, and reading one out of it would mean reading past the end of
    /// what the file has.
    #[test]
    fn a_short_footer_is_refused_rather_than_padded() {
        let mut short = footer(3, 512, 1024);
        short.truncate(FOOTER_LEN - 1);
        assert!(parse_footer(&short).is_none());
    }

    /// A corrupted footer still identifies itself — it is reported with
    /// `checksum_matches: false` rather than refused, which is the module
    /// doc's own decision: ART has one real VHD to check the arithmetic
    /// against and it belongs to the owner.
    #[test]
    fn a_bad_checksum_is_reported_and_not_a_refusal() {
        let mut bytes = footer(3, 512, 1024);
        bytes[64..68].copy_from_slice(&0u32.to_be_bytes());
        let read = parse_footer(&bytes).expect("still identifiable");
        assert_eq!(read.kind, VhdKind::Dynamic);
        assert!(!read.checksum_matches);
    }

    /// The checksum skips its own four bytes. Without that it would depend on
    /// what it is about to write, which cannot be satisfied.
    #[test]
    fn the_checksum_ignores_the_field_it_lands_in() {
        let mut bytes = footer(3, 512, 1024);
        let before = checksum(&bytes);
        bytes[64..68].copy_from_slice(&0xDEAD_BEEFu32.to_be_bytes());
        assert_eq!(checksum(&bytes), before);
    }

    /// Every field is big-endian, and getting that backwards is the kind of
    /// mistake that reads as a plausible number rather than as an error: a
    /// dynamic image (3) read little-endian is 50 331 648.
    #[test]
    fn the_fields_are_big_endian() {
        let bytes = footer(3, 512, 1024);
        assert_eq!(bytes[60..64], [0, 0, 0, 3], "disk type, big-endian");
        assert_eq!(parse_footer(&bytes).unwrap().kind, VhdKind::Dynamic);
    }
}
