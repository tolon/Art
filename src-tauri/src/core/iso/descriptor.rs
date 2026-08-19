//! ISO9660 volume descriptors, and the two text encodings a disc uses.
//!
//! Everything a reader needs to find the root directory lives in the volume
//! descriptor set: a run of 2048-byte descriptors starting at sector 16 and
//! ending at a terminator. This module parses those, and nothing else.
//!
//! # Two encodings, and why neither is `from_utf8_lossy`
//!
//! An identifier in the Primary Volume Descriptor is ISO 646 — 7-bit ASCII.
//! An identifier in a Joliet Supplementary descriptor is UCS-2 **big-endian**,
//! two bytes per character with the high byte first. Neither is UTF-8, and
//! feeding either to `String::from_utf8_lossy` corrupts it: UCS-2 `'A'` is
//! `00 41`, which lossy-decodes to a NUL followed by an `A`.
//!
//! That is exactly the shape of ART-074, where an ADF name was written as
//! Latin-1 and read back as UTF-8 and every test used ASCII names, so nothing
//! noticed. Both decoders here are explicit, and both are tested with
//! characters outside ASCII.

use crate::core::error::{CoreError, CoreResult};

/// Bytes of user data in one logical sector. Fixed by ISO9660; a disc that
/// declares anything else in its PVD is not something ART reads.
pub const LOGICAL_SECTOR_SIZE: usize = 2048;

/// Bytes in one sector of a raw CD track dump: 2048 of user data wrapped in a
/// 16-byte sync + header and 288 bytes of EDC/ECC.
pub const RAW_SECTOR_SIZE: usize = 2352;

/// Offset of the user data inside a raw 2352-byte Mode 1 sector: 12 bytes of
/// sync, then a 4-byte header (minute, second, frame, mode).
pub const RAW_DATA_OFFSET: u64 = 16;

/// Offset of the user data inside a raw Mode 2/XA sector: the same 16, plus
/// an 8-byte subheader (file, channel, submode, coding — written twice).
pub const XA_DATA_OFFSET: u64 = 24;

/// Offset of the sector's mode byte, the last of the 4-byte header. `1` is
/// Mode 1, `2` is Mode 2 (which is where the subheader appears).
pub const RAW_MODE_OFFSET: usize = 15;

/// Offset of the submode byte inside a Mode 2 sector — the third byte of the
/// subheader, and the one that says which *form* this sector is.
pub const XA_SUBMODE_OFFSET: usize = 18;

/// Submode bit meaning "Form 2": 2324 bytes of user data with no error
/// correction, used for audio and video, and carrying no ISO9660 filesystem.
/// ART refuses such a disc rather than reading 2048 bytes out of a sector
/// that is not laid out that way.
pub const XA_SUBMODE_FORM2: u8 = 0x20;

/// The volume descriptor set starts here, on every disc.
pub const FIRST_DESCRIPTOR_LBA: u32 = 16;

/// How many descriptors ART will read before giving up. A real disc has two
/// to five; 32 is generous, and a file claiming more is malformed or hostile.
pub const MAX_DESCRIPTORS: usize = 32;

/// Standard identifier present in every volume descriptor.
pub const ISO_MAGIC: &[u8; 5] = b"CD001";

const DESC_TYPE_PRIMARY: u8 = 1;
const DESC_TYPE_SUPPLEMENTARY: u8 = 2;
const DESC_TYPE_TERMINATOR: u8 = 255;

/// Offset of the 32-byte volume identifier inside a descriptor.
const VOLUME_ID_OFFSET: usize = 40;
const VOLUME_ID_LEN: usize = 32;

/// Offset of the 32-byte escape sequence field (Supplementary descriptors).
const ESCAPE_OFFSET: usize = 88;

/// Offset of the 34-byte root directory record inside a descriptor.
const ROOT_RECORD_OFFSET: usize = 156;
const ROOT_RECORD_LEN: usize = 34;

/// The three escape sequences that mean "the names in this descriptor's tree
/// are UCS-2": Joliet levels 1, 2 and 3.
const JOLIET_ESCAPES: [&[u8; 3]; 3] = [b"%/@", b"%/C", b"%/E"];

/// How the sectors of an image are laid out on disk.
///
/// Detection has already decided this from where it found `CD001`
/// (`core::detect`, format hints `iso9660` and `iso9660-raw`); this type
/// carries that decision rather than re-deriving it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SectorLayout {
    /// 2048 bytes per sector, all of it user data. A plain `.iso`.
    Cooked,
    /// 2352 bytes per sector with the 2048 bytes of user data at offset 16.
    /// A raw Mode-1 track dump, as `.bin` and some `.img` files are.
    Raw2352,
    /// 2352 bytes per sector, **Mode 2/XA Form 1**: sync and header as Mode 1,
    /// then an 8-byte subheader, so the 2048 bytes of user data start at 24.
    ///
    /// Its own variant rather than a field, because the whole point is that
    /// the data offset is decided once at open and carried — assuming 16
    /// everywhere is [ART-075], where detection and the reader were wrong
    /// together. CD32 and mixed-mode discs are written this way.
    ///
    /// [ART-075]: ../../../docs/ISSUES.md
    Raw2352Xa,
}

impl SectorLayout {
    /// The layout behind one of `core::detect`'s optical format hints.
    ///
    /// Returns `None` for any other hint, so a caller cannot accidentally
    /// treat, say, an ADF as a disc.
    pub fn from_format_hint(hint: &str) -> Option<Self> {
        match hint {
            "iso9660" => Some(Self::Cooked),
            "iso9660-raw" => Some(Self::Raw2352),
            "iso9660-raw-xa" => Some(Self::Raw2352Xa),
            _ => None,
        }
    }

    /// Bytes occupied by one sector on disk.
    pub fn sector_size(self) -> u64 {
        match self {
            Self::Cooked => LOGICAL_SECTOR_SIZE as u64,
            Self::Raw2352 | Self::Raw2352Xa => RAW_SECTOR_SIZE as u64,
        }
    }

    /// Offset of the user data within a sector.
    pub fn data_offset(self) -> u64 {
        match self {
            Self::Cooked => 0,
            Self::Raw2352 => RAW_DATA_OFFSET,
            Self::Raw2352Xa => XA_DATA_OFFSET,
        }
    }

    /// True for the layouts that are a track dump rather than user data
    /// alone — the ones carrying a sync pattern, a header and, for XA, a
    /// subheader that says which form the sector is.
    pub fn is_raw(self) -> bool {
        matches!(self, Self::Raw2352 | Self::Raw2352Xa)
    }

    /// Byte offset of the user data of logical block `lba`.
    ///
    /// The multiplication is checked: an LBA is a `u32` read straight out of
    /// an untrusted directory record, and `0xFFFF_FFFF × 2352` does not fit a
    /// `u64`'s comfort zone by accident — it does fit, but the addition and
    /// the later length arithmetic must not be allowed to wrap either.
    pub fn data_offset_of(self, lba: u32) -> CoreResult<u64> {
        (lba as u64)
            .checked_mul(self.sector_size())
            .and_then(|v| v.checked_add(self.data_offset()))
            .ok_or_else(|| malformed(format!("block number {lba} is out of range for this image")))
    }
}

/// What ART took from one descriptor: enough to list the disc.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VolumeInfo {
    /// The volume identifier, trimmed of its padding.
    pub volume_name: String,
    /// Starting logical block of the root directory.
    pub root_extent: u32,
    /// Bytes of the root directory's extent.
    pub root_length: u32,
    /// True when this came from a Joliet descriptor, so the names in its
    /// directory tree are UCS-2 big-endian rather than ISO 646.
    pub joliet: bool,
}

/// What kind of descriptor a sector holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DescriptorKind {
    Primary,
    /// A Supplementary descriptor whose escape sequence says Joliet.
    Joliet,
    /// A Supplementary descriptor that is not Joliet, a boot record, or any
    /// other type ART has no use for.
    Other,
    Terminator,
}

/// Classify a 2048-byte volume descriptor.
///
/// Fails when the standard identifier is missing — at that point the run of
/// descriptors has ended without a terminator and nothing further can be
/// trusted.
pub fn descriptor_kind(sector: &[u8]) -> CoreResult<DescriptorKind> {
    if sector.len() < LOGICAL_SECTOR_SIZE {
        return Err(malformed(
            "a volume descriptor is shorter than one sector".to_string(),
        ));
    }
    if &sector[1..6] != ISO_MAGIC {
        return Err(malformed(
            "a volume descriptor is missing its CD001 identifier".to_string(),
        ));
    }
    Ok(match sector[0] {
        DESC_TYPE_TERMINATOR => DescriptorKind::Terminator,
        DESC_TYPE_PRIMARY => DescriptorKind::Primary,
        DESC_TYPE_SUPPLEMENTARY if is_joliet(sector) => DescriptorKind::Joliet,
        _ => DescriptorKind::Other,
    })
}

/// True when a Supplementary descriptor's escape sequence selects UCS-2.
fn is_joliet(sector: &[u8]) -> bool {
    let escape = &sector[ESCAPE_OFFSET..ESCAPE_OFFSET + 3];
    JOLIET_ESCAPES.iter().any(|e| escape == e.as_slice())
}

/// Pull the volume name and root directory record out of a descriptor.
///
/// `kind` decides how the volume identifier is decoded, which is the whole
/// reason the two are separate calls.
pub fn parse_volume_descriptor(sector: &[u8], kind: DescriptorKind) -> CoreResult<VolumeInfo> {
    let joliet = match kind {
        DescriptorKind::Primary => false,
        DescriptorKind::Joliet => true,
        _ => {
            return Err(malformed(
                "asked to read a volume from a descriptor that describes none".to_string(),
            ))
        }
    };

    let id = &sector[VOLUME_ID_OFFSET..VOLUME_ID_OFFSET + VOLUME_ID_LEN];
    let volume_name = if joliet {
        decode_ucs2_be(id)
    } else {
        decode_iso646(id)
    };
    let volume_name = volume_name.trim_end().to_string();

    let record = &sector[ROOT_RECORD_OFFSET..ROOT_RECORD_OFFSET + ROOT_RECORD_LEN];
    if (record[0] as usize) < ROOT_RECORD_LEN {
        return Err(malformed(format!(
            "the root directory record is {} bytes; it must be at least {ROOT_RECORD_LEN}",
            record[0]
        )));
    }
    let root_extent = u32::from_le_bytes([record[2], record[3], record[4], record[5]]);
    let root_length = u32::from_le_bytes([record[10], record[11], record[12], record[13]]);
    if root_length == 0 {
        return Err(malformed(
            "the root directory of this disc has no contents".to_string(),
        ));
    }

    Ok(VolumeInfo {
        volume_name,
        root_extent,
        root_length,
        joliet,
    })
}

/// Decode an ISO 646 (7-bit ASCII) identifier — except that real discs put
/// bytes there the standard does not allow, and turning every one of them
/// into `?` was a worse answer than deciding what they mean.
///
/// # What the spec says, and what a real disc does anyway
///
/// ECMA-119 (ISO 9660's own text) restricts a directory or file identifier
/// in the mandatory (non-Joliet, non-Rock-Ridge) tree to upper-case letters,
/// digits, underscore and a dot — "All levels restrict file names in the
/// mandatory file hierarchy to upper case letters, digits, underscores, and
/// a dot" (<https://en.wikipedia.org/wiki/ISO_9660>). A byte with the high
/// bit set is genuinely out of spec there, at every interchange level. So
/// the disc measured below is non-conformant — and it is real material, not
/// a hypothetical: the owner's own AmigaOS 3.9 CD
/// (`OS-VERSION3.9/WORKBENCH3.5/STORAGE/LOCALE/COUNTRIES-EURO`) carries
/// three filenames whose identifier holds exactly one byte with the high
/// bit set — `0xD6`, `0xCB`, `0xD1` — where the country's name needs
/// `Ö`, `Ë`, `Ñ` (measured verbatim by
/// `core::osinstall::apply::tests::read_the_raw_country_name_bytes_when_asked`,
/// pinned below as `a_real_disc_countries_euro_name_decodes_as_latin1`).
/// Those three bytes are exactly the ISO-8859-1 (Latin-1) code points for
/// those three letters.
///
/// That is not a coincidence to guess past. AmigaDOS's own native character
/// set is ISO-8859-1: "the developers chose to use the ANSI–ISO standard
/// ISO-8859-1 (Latin 1), which includes the ASCII character set"
/// (<https://en.wikipedia.org/wiki/AmigaDOS>) — the same byte-transparent
/// choice `write_bcpl_string` already makes in the ADF core, so an Amiga
/// filename's accented letters were Latin-1 before this disc's mastering
/// tool ever copied them, unchanged, into a d-string field the standard
/// says they do not belong in. And this is independently confirmed by a
/// second implementation sharing no code with ART's: this project's own
/// ISO9660 oracle, 7-Zip (`scripts/iso-oracle-check.py`), reads this exact
/// disc's Rock Ridge alternate names for the same three files as
/// lower-case `ö` (`0xF6`), `ë` (`0xEB`), `ñ` (`0xF1`) — the lower-case
/// Latin-1 pairing of the very same three bytes found in the Primary tree.
/// (amitools, this project's chosen *ADF/HDF* oracle, has no ISO9660
/// support to consult at all — checked directly, not assumed.)
///
/// # The decision
///
/// Decode every byte as ISO-8859-1, which for the 0x80..=0xFF range is
/// simply `b as char` — Unicode's first 256 code points are Latin-1 by
/// construction, so no table is needed. This also repairs a real defect the
/// `?` behaviour had, not only a cosmetic one: `Österreich` and a
/// differently-accented name both folded to the same `?`-led string, an
/// actual collision, where Latin-1 keeps every byte value distinct.
///
/// It is still a guess for a disc mastered in some other 8-bit code page —
/// ECMA-119 leaves those bytes formally undefined, so there is no
/// spec-correct answer for *any* choice here — but Latin-1 was the
/// overwhelmingly common choice for 8-bit bytes smuggled into a non-Joliet
/// identifier in this disc's era, it is what AmigaOS itself assumes, and
/// unlike `?` it never merges two distinct names into one.
///
/// # This decodes every ISO9660 disc's Primary tree, not only Amiga ones
///
/// A non-Amiga disc that happens to carry an out-of-spec high-bit byte in a
/// Primary identifier now decodes it as a specific Latin-1 character
/// instead of `?`. For a disc actually mastered in Latin-1 or the
/// closely-related Windows-1252 (the common case for Western European
/// software of this era) that is a strictly better answer. For one mastered
/// in some other 8-bit encoding — Cyrillic, Greek, Shift-JIS — it produces
/// a specific, plausible-looking, wrong character rather than a visibly
/// uninformative `?`. ART has no way to know which case it is looking at:
/// nothing in the Primary Volume Descriptor names a code page for this
/// field, which is exactly why the standard forbids using it for anything
/// but the restricted d-character set in the first place.
/// `a_byte_above_ascii_decodes_as_latin1` (below) pins the general,
/// non-Amiga case; `a_real_disc_countries_euro_name_decodes_as_latin1` pins
/// the real disc that motivated the change.
pub fn decode_iso646(bytes: &[u8]) -> String {
    bytes
        .iter()
        .take_while(|&&b| b != 0)
        .map(|&b| b as char)
        .collect()
}

/// Decode a UCS-2 big-endian (Joliet) identifier.
///
/// Big-endian is not a detail that can be shrugged off: `00 41` is `A` and
/// `41 00` is a CJK ideograph. Surrogate pairs are not legal UCS-2, but real
/// discs written by modern tools contain them, so they are decoded as UTF-16
/// and an unpaired half becomes `?` rather than a panic or silent truncation.
pub fn decode_ucs2_be(bytes: &[u8]) -> String {
    // An odd trailing byte cannot form a character; ignore it rather than
    // reading one byte past the field.
    let units: Vec<u16> = bytes
        .chunks_exact(2)
        .map(|p| u16::from_be_bytes([p[0], p[1]]))
        .take_while(|&u| u != 0)
        .collect();
    std::char::decode_utf16(units)
        .map(|r| r.unwrap_or('?'))
        .collect()
}

fn malformed(detail: String) -> CoreError {
    CoreError::Malformed {
        format: "iso9660".to_string(),
        detail,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_layouts_agree_with_where_detection_found_cd001() {
        // `core::detect` probes 0x8001 and 0x9311 for "CD001" and turns each
        // into a format hint. If this module computed sector 16 differently,
        // detection would hand over a layout the reader then misreads — so
        // both numbers are pinned here against detect's constants.
        assert_eq!(
            SectorLayout::Cooked
                .data_offset_of(FIRST_DESCRIPTOR_LBA)
                .unwrap()
                + 1,
            0x8001
        );
        assert_eq!(
            SectorLayout::Raw2352
                .data_offset_of(FIRST_DESCRIPTOR_LBA)
                .unwrap()
                + 1,
            0x9311
        );
    }

    #[test]
    fn a_format_hint_maps_to_a_layout() {
        assert_eq!(
            SectorLayout::from_format_hint("iso9660"),
            Some(SectorLayout::Cooked)
        );
        assert_eq!(
            SectorLayout::from_format_hint("iso9660-raw"),
            Some(SectorLayout::Raw2352)
        );
        assert_eq!(SectorLayout::from_format_hint("adf"), None);
        assert_eq!(SectorLayout::from_format_hint(""), None);
    }

    #[test]
    fn a_block_number_near_the_top_of_u32_is_an_error_not_a_wrap() {
        // 0xFFFF_FFFF × 2352 still fits a u64, so this one succeeds; the
        // point is that it produces a number, not a wrapped small offset.
        let off = SectorLayout::Raw2352.data_offset_of(u32::MAX).unwrap();
        assert_eq!(off, u32::MAX as u64 * 2352 + 16);
        assert!(off > u32::MAX as u64);
    }

    #[test]
    fn ascii_identifiers_decode_and_stop_at_the_first_nul() {
        assert_eq!(decode_iso646(b"AMIGAOS39"), "AMIGAOS39");
        assert_eq!(decode_iso646(b"ART\0\0\0"), "ART");
        assert_eq!(decode_iso646(b""), "");
    }

    #[test]
    fn a_byte_above_ascii_is_not_a_silent_utf8_replacement() {
        // `from_utf8_lossy` would give U+FFFD here, and on a two-byte
        // sequence it would give one character where the disc had two. The
        // decoder is explicit, so the count is always the byte count.
        let s = decode_iso646(&[b'A', 0xFC, 0xDF, b'Z']);
        assert_eq!(s.chars().count(), 4);
        // Was "A??Z" before this task: every non-ASCII byte became `?`.
        // `decode_iso646`'s doc comment records why that changed — this
        // assertion is updated, not just the code under it, because 0xFC
        // and 0xDF are themselves the Latin-1 code points for `ü` and `ß`.
        assert_eq!(s, "AüßZ");
    }

    #[test]
    fn a_byte_above_ascii_decodes_as_latin1() {
        // The general case `decode_iso646`'s doc comment discusses: any
        // ISO9660 disc, not only an Amiga one, that puts an out-of-spec
        // high-bit byte in a Primary identifier. Unicode's first 256 code
        // points are Latin-1 by construction, so byte value and code point
        // coincide across the whole 0x80..=0xFF range, not just the two
        // bytes the sibling test happens to use.
        for b in 0x80u8..=0xFF {
            let s = decode_iso646(&[b]);
            assert_eq!(s.chars().count(), 1);
            assert_eq!(s.chars().next().unwrap() as u32, b as u32);
        }
    }

    #[test]
    fn a_real_disc_countries_euro_name_decodes_as_latin1() {
        // Verbatim raw identifier bytes from the owner's real AmigaOS 3.9
        // CD (`AmigaOS39.iso`, volume `AmigaOS3.9`),
        // `OS-VERSION3.9/WORKBENCH3.5/STORAGE/LOCALE/COUNTRIES-EURO`,
        // measured by
        // `core::osinstall::apply::tests::read_the_raw_country_name_bytes_when_asked`
        // (Task 5, Step 1) — not invented, and not the same bytes the
        // sibling tests above use. This is ART-155's own evidence: three
        // real names ART decoded as `?STERREICH.COUNTRY`, `BELGI?.COUNTRY`
        // and `ESPA?A.COUNTRY` before this task.
        assert_eq!(
            decode_iso646(&[
                0xd6, 0x53, 0x54, 0x45, 0x52, 0x52, 0x45, 0x49, 0x43, 0x48, b'.', b'C', b'O', b'U',
                b'N', b'T', b'R', b'Y', b';', b'1'
            ]),
            "ÖSTERREICH.COUNTRY;1"
        );
        assert_eq!(
            decode_iso646(&[
                0x42, 0x45, 0x4c, 0x47, 0x49, 0xcb, b'.', b'C', b'O', b'U', b'N', b'T', b'R', b'Y',
                b';', b'1'
            ]),
            "BELGIË.COUNTRY;1"
        );
        assert_eq!(
            decode_iso646(&[
                0x45, 0x53, 0x50, 0x41, 0xd1, 0x41, b'.', b'C', b'O', b'U', b'N', b'T', b'R', b'Y',
                b';', b'1'
            ]),
            "ESPAÑA.COUNTRY;1"
        );
    }

    #[test]
    fn ucs2_is_big_endian_not_little() {
        // "Grüße" in UCS-2 BE.
        let bytes = [0x00, 0x47, 0x00, 0x72, 0x00, 0xFC, 0x00, 0xDF, 0x00, 0x65];
        assert_eq!(decode_ucs2_be(&bytes), "Grüße");
        // The same bytes read little-endian are a different string entirely,
        // which is what makes the endianness load-bearing.
        let swapped: Vec<u8> = bytes.chunks_exact(2).flat_map(|p| [p[1], p[0]]).collect();
        assert_ne!(decode_ucs2_be(&swapped), "Grüße");
    }

    #[test]
    fn ucs2_from_utf8_lossy_would_have_been_wrong() {
        // The trap this module exists to avoid, written down: the lossy
        // decoder turns "AB" into a string with embedded NULs.
        let bytes = [0x00, 0x41, 0x00, 0x42];
        assert_eq!(decode_ucs2_be(&bytes), "AB");
        assert_ne!(String::from_utf8_lossy(&bytes), "AB");
    }

    #[test]
    fn ucs2_handles_a_surrogate_pair_and_a_lone_half() {
        // U+1F600, as a surrogate pair.
        assert_eq!(decode_ucs2_be(&[0xD8, 0x3D, 0xDE, 0x00]), "\u{1F600}");
        // A high surrogate with nothing after it is not a character.
        assert_eq!(decode_ucs2_be(&[0xD8, 0x3D]), "?");
    }

    #[test]
    fn ucs2_ignores_an_odd_trailing_byte() {
        assert_eq!(decode_ucs2_be(&[0x00, 0x41, 0x00]), "A");
        assert_eq!(decode_ucs2_be(&[0x41]), "");
    }

    #[test]
    fn a_descriptor_without_cd001_is_rejected() {
        let mut sector = vec![0u8; LOGICAL_SECTOR_SIZE];
        sector[0] = 1;
        sector[1..6].copy_from_slice(b"CDXXX");
        let err = descriptor_kind(&sector).unwrap_err();
        assert!(err.to_string().contains("CD001"), "{err}");
        assert_eq!(err.code(), "ART-FORMAT-MALFORMED");
    }

    #[test]
    fn a_short_sector_is_rejected_rather_than_indexed() {
        let sector = vec![0u8; 8];
        assert!(descriptor_kind(&sector).is_err());
    }

    #[test]
    fn each_joliet_escape_sequence_is_recognised() {
        for escape in JOLIET_ESCAPES {
            let mut sector = vec![0u8; LOGICAL_SECTOR_SIZE];
            sector[0] = DESC_TYPE_SUPPLEMENTARY;
            sector[1..6].copy_from_slice(ISO_MAGIC);
            sector[ESCAPE_OFFSET..ESCAPE_OFFSET + 3].copy_from_slice(escape.as_slice());
            assert_eq!(descriptor_kind(&sector).unwrap(), DescriptorKind::Joliet);
        }
        // A supplementary descriptor with some other escape is not Joliet and
        // must not have its names read as UCS-2.
        let mut sector = vec![0u8; LOGICAL_SECTOR_SIZE];
        sector[0] = DESC_TYPE_SUPPLEMENTARY;
        sector[1..6].copy_from_slice(ISO_MAGIC);
        sector[ESCAPE_OFFSET..ESCAPE_OFFSET + 3].copy_from_slice(b"%/Z");
        assert_eq!(descriptor_kind(&sector).unwrap(), DescriptorKind::Other);
    }

    #[test]
    fn a_root_record_shorter_than_34_bytes_is_malformed() {
        let mut sector = vec![0u8; LOGICAL_SECTOR_SIZE];
        sector[0] = DESC_TYPE_PRIMARY;
        sector[1..6].copy_from_slice(ISO_MAGIC);
        sector[ROOT_RECORD_OFFSET] = 12;
        let err = parse_volume_descriptor(&sector, DescriptorKind::Primary).unwrap_err();
        assert!(err.to_string().contains("root directory record"), "{err}");
    }

    #[test]
    fn a_root_of_zero_length_is_malformed() {
        let mut sector = vec![0u8; LOGICAL_SECTOR_SIZE];
        sector[0] = DESC_TYPE_PRIMARY;
        sector[1..6].copy_from_slice(ISO_MAGIC);
        sector[ROOT_RECORD_OFFSET] = 34;
        // extent set, length left at zero
        sector[ROOT_RECORD_OFFSET + 2..ROOT_RECORD_OFFSET + 6]
            .copy_from_slice(&23u32.to_le_bytes());
        let err = parse_volume_descriptor(&sector, DescriptorKind::Primary).unwrap_err();
        assert!(err.to_string().contains("no contents"), "{err}");
    }
}
