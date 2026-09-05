//! The `PTRN` chunk inside `WBPattern.prefs`: one backdrop assignment
//! (`Root`, a `Drawer`, or the `Screen`).
//!
//! Measured against real files rather than recalled (design doc §1.1): the
//! release's own root `PTRN` in ART's built AmigaOS 3.2 tree is 16 reserved
//! zero bytes, then `Which`, `Flags`, `Revision`, `Depth`, `DataLength`, then
//! the data — 24 bytes of header before anything variable-length begins.
//!
//! Two rules here are measured and are the opposite of a plausible guess:
//!
//! - A picture's path is **not** NUL-terminated. `DataLength` is the exact
//!   string length; the trailing zero visible in a hex dump of an odd-sized
//!   chunk is the IFF pad byte from [`super::iff`], not part of the data.
//! - A pattern's `DataLength` is **not** `Depth * PATTERN_BYTES_PER_PLANE`.
//!   That holds for the `Christmas` preset (`Depth=3`, 96 bytes) and fails
//!   for the AmigaOS 3.2/3.9 release's own screen chunk (`Depth=0`, a
//!   256-byte blank buffer). Pattern bytes are carried opaquely.

use crate::core::error::{CoreError, CoreResult};

const RESERVED_LEN: usize = 16;
const HEADER_LEN: usize = 24;
/// `PAT_WIDTH` x `PAT_HEIGHT` = 16x16 bits = 32 bytes, per plane. Documents
/// the shape of a *populated* pattern (the `Christmas` preset's `3 * 32 =
/// 96`) but is deliberately not enforced — see the module doc above.
#[allow(dead_code)]
const PATTERN_BYTES_PER_PLANE: usize = 32;

const WBPF_PATTERN: u16 = 0x0001;
const WBPF_NOREMAP: u16 = 0x0010;
const DITHER_MASK: u16 = 0x0300;
const PRECISION_MASK: u16 = 0x0C00;
const PLACEMENT_MASK: u16 = 0x3000;
/// Every bit this module gives a name to. A bit outside this mask is not
/// unused — it is carried in `Backdrop::other_flags` and written back
/// verbatim, because ART must not depend on the release never setting one
/// (Task 7 edits the release's own `WBPattern.prefs` in place).
const KNOWN_FLAG_MASK: u16 =
    WBPF_PATTERN | WBPF_NOREMAP | DITHER_MASK | PRECISION_MASK | PLACEMENT_MASK;

/// The flags the release's own root backdrop carries:
/// `PLACEMENT_SCALE | PRECISION_IMAGE | DITHER_GOOD`. ART's default for a
/// picture is the release's choice, not one invented here.
pub const DEFAULT_PICTURE_FLAGS: u16 = 0x2A00;

fn malformed(detail: impl Into<String>) -> CoreError {
    CoreError::Malformed {
        format: "WBPattern PTRN".to_string(),
        detail: detail.into(),
    }
}

/// Which slot this backdrop assignment applies to. Serialises as the
/// `Which` field's own values: `Root` = 0, `Drawer` = 1, `Screen` = 2.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Which {
    Root,
    Drawer,
    Screen,
}

impl Which {
    fn to_bits(self) -> u16 {
        match self {
            Which::Root => 0,
            Which::Drawer => 1,
            Which::Screen => 2,
        }
    }

    fn from_bits(bits: u16) -> CoreResult<Self> {
        match bits {
            0 => Ok(Which::Root),
            1 => Ok(Which::Drawer),
            2 => Ok(Which::Screen),
            other => Err(malformed(format!("unrecognised Which value {other}"))),
        }
    }
}

/// `WBPF_PLACEMENT`, bits 12-13 of `Flags`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Placement {
    Tile,
    Center,
    Scale,
    ScaleGood,
}

impl Placement {
    fn to_bits(self) -> u16 {
        match self {
            Placement::Tile => 0,
            Placement::Center => 1,
            Placement::Scale => 2,
            Placement::ScaleGood => 3,
        }
    }

    fn from_bits(bits: u16) -> Self {
        match bits {
            0 => Placement::Tile,
            1 => Placement::Center,
            2 => Placement::Scale,
            _ => Placement::ScaleGood,
        }
    }
}

/// `WBPF_PRECISION`, bits 10-11 of `Flags`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Precision {
    Default,
    Icon,
    Image,
    Exact,
}

impl Precision {
    fn to_bits(self) -> u16 {
        match self {
            Precision::Default => 0,
            Precision::Icon => 1,
            Precision::Image => 2,
            Precision::Exact => 3,
        }
    }

    fn from_bits(bits: u16) -> Self {
        match bits {
            0 => Precision::Default,
            1 => Precision::Icon,
            2 => Precision::Image,
            _ => Precision::Exact,
        }
    }
}

/// `WBPF_DITHER`, bits 8-9 of `Flags`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dither {
    Default,
    Bad,
    Good,
    Best,
}

impl Dither {
    fn to_bits(self) -> u16 {
        match self {
            Dither::Default => 0,
            Dither::Bad => 1,
            Dither::Good => 2,
            Dither::Best => 3,
        }
    }

    fn from_bits(bits: u16) -> Self {
        match bits {
            0 => Dither::Default,
            1 => Dither::Bad,
            2 => Dither::Good,
            _ => Dither::Best,
        }
    }
}

/// What this backdrop shows: a picture file (`WBPF_PATTERN` clear) or a
/// dithered bitmap pattern (`WBPF_PATTERN` set). A `Pattern`'s bytes are
/// carried opaquely — see the module doc.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Content {
    Picture(String),
    Pattern { depth: u8, planes: Vec<u8> },
}

/// One `PTRN` chunk, fully decoded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Backdrop {
    pub which: Which,
    pub placement: Placement,
    pub precision: Precision,
    pub dither: Dither,
    pub no_remap: bool,
    /// Flag bits outside every mask this module understands, carried through
    /// verbatim. ART changes only what the user set; a bit it cannot name is
    /// still the release's byte.
    pub other_flags: u16,
    /// `wbp_Revision`, carried rather than assumed zero.
    pub revision: i8,
    pub content: Content,
}

/// Encode a path back to ISO-8859-1 bytes. Refuses a character outside the
/// encoding rather than lossily substituting one. Decoding uses
/// [`super::decode_latin1`] directly — it never fails, so there is nothing
/// for a local wrapper to add.
fn encode_latin1(s: &str) -> CoreResult<Vec<u8>> {
    super::encode_latin1(s)
        .map_err(|c| malformed(format!("path character '{c}' is outside ISO-8859-1")))
}

/// Parse one `PTRN` chunk body.
pub fn read_backdrop(body: &[u8]) -> CoreResult<Backdrop> {
    if body.len() < HEADER_LEN {
        return Err(malformed("shorter than the PTRN header"));
    }
    let which = Which::from_bits(u16::from_be_bytes([body[16], body[17]]))?;
    let flags = u16::from_be_bytes([body[18], body[19]]);
    let revision = body[20] as i8;
    let depth = body[21];
    let data_length = u16::from_be_bytes([body[22], body[23]]) as usize;
    let remaining = body.len() - HEADER_LEN;
    if data_length != remaining {
        return Err(malformed(format!(
            "DataLength {data_length} does not match the {remaining} bytes that follow"
        )));
    }
    let data = &body[HEADER_LEN..];

    let content = if flags & WBPF_PATTERN != 0 {
        Content::Pattern {
            depth,
            planes: data.to_vec(),
        }
    } else {
        Content::Picture(super::decode_latin1(data))
    };

    Ok(Backdrop {
        which,
        placement: Placement::from_bits((flags & PLACEMENT_MASK) >> 12),
        precision: Precision::from_bits((flags & PRECISION_MASK) >> 10),
        dither: Dither::from_bits((flags & DITHER_MASK) >> 8),
        no_remap: flags & WBPF_NOREMAP != 0,
        other_flags: flags & !KNOWN_FLAG_MASK,
        revision,
        content,
    })
}

/// Serialise a `PTRN` chunk body.
pub fn write_backdrop(b: &Backdrop) -> CoreResult<Vec<u8>> {
    let mut flags = (b.placement.to_bits() << 12)
        | (b.precision.to_bits() << 10)
        | (b.dither.to_bits() << 8)
        | b.other_flags;
    if b.no_remap {
        flags |= WBPF_NOREMAP;
    }

    let (depth, data): (u8, Vec<u8>) = match &b.content {
        Content::Picture(path) => (0, encode_latin1(path)?),
        Content::Pattern { depth, planes } => {
            flags |= WBPF_PATTERN;
            (*depth, planes.clone())
        }
    };
    let data_length = u16::try_from(data.len())
        .map_err(|_| malformed("backdrop data is longer than a PTRN chunk can hold"))?;

    let mut out = Vec::with_capacity(HEADER_LEN + data.len());
    out.extend_from_slice(&[0u8; RESERVED_LEN]);
    out.extend_from_slice(&b.which.to_bits().to_be_bytes());
    out.extend_from_slice(&flags.to_be_bytes());
    out.push(b.revision as u8);
    out.push(depth);
    out.extend_from_slice(&data_length.to_be_bytes());
    out.extend_from_slice(&data);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The root `PTRN` of the `WBPattern.prefs` in ART's own built AmigaOS
    /// 3.2 tree: 16 reserved zero bytes, Which=0, Flags=0x2A00, Revision=0,
    /// Depth=0, DataLength=43, then the path — **with no terminator**. The
    /// trailing `00` in a hex dump of that file is the IFF pad byte for an
    /// odd chunk size (67), not part of the data.
    fn measured_picture_body() -> Vec<u8> {
        let path = b"Sys:Prefs/Presets/Backdrops/default_pal.iff";
        assert_eq!(path.len(), 43, "the measured DataLength is 43");
        let mut body = vec![0u8; 16];
        body.extend_from_slice(&0u16.to_be_bytes()); // Which = root
        body.extend_from_slice(&0x2A00u16.to_be_bytes()); // Flags
        body.push(0); // Revision
        body.push(0); // Depth
        body.extend_from_slice(&(path.len() as u16).to_be_bytes());
        body.extend_from_slice(path);
        body
    }

    /// The `Christmas` preset's screen `PTRN`: WBPF_PATTERN set, Depth=3,
    /// DataLength=0x60=96, which is 16x16 bits times three planes.
    fn measured_pattern_body() -> Vec<u8> {
        let mut body = vec![0u8; 16];
        body.extend_from_slice(&2u16.to_be_bytes()); // Which = screen
        body.extend_from_slice(&0x0001u16.to_be_bytes()); // WBPF_PATTERN
        body.push(0);
        body.push(3); // Depth
        body.extend_from_slice(&96u16.to_be_bytes());
        body.extend(std::iter::repeat_n(0xA5u8, 96));
        body
    }

    /// The **release's own** screen `PTRN`, from the same 3.2 file as
    /// `measured_picture_body`: WBPF_PATTERN set but Depth=0 and a
    /// **256-byte** blank buffer. `DataLength` is therefore *not*
    /// `Depth * 32`, and a guard that assumed it would refuse a file
    /// AmigaOS itself ships.
    fn measured_blank_pattern_body() -> Vec<u8> {
        let mut body = vec![0u8; 16];
        body.extend_from_slice(&2u16.to_be_bytes()); // Which = screen
        body.extend_from_slice(&0x0001u16.to_be_bytes()); // WBPF_PATTERN
        body.push(0);
        body.push(0); // Depth = 0
        body.extend_from_slice(&256u16.to_be_bytes());
        body.extend(std::iter::repeat_n(0u8, 256));
        body
    }

    #[test]
    fn the_measured_picture_backdrop_reads_as_a_path() {
        let b = read_backdrop(&measured_picture_body()).unwrap();
        assert_eq!(b.which, Which::Root);
        assert_eq!(b.placement, Placement::Scale);
        assert_eq!(b.precision, Precision::Image);
        assert_eq!(b.dither, Dither::Good);
        assert!(!b.no_remap);
        assert_eq!(
            b.content,
            Content::Picture("Sys:Prefs/Presets/Backdrops/default_pal.iff".to_string())
        );
    }

    #[test]
    fn the_measured_pattern_backdrop_reads_as_planes() {
        let b = read_backdrop(&measured_pattern_body()).unwrap();
        assert_eq!(b.which, Which::Screen);
        match b.content {
            Content::Pattern { depth, ref planes } => {
                assert_eq!(depth, 3);
                assert_eq!(planes.len(), 96, "16x16 bits times three planes");
            }
            _ => panic!(
                "WBPF_PATTERN set must read as a pattern, got {:?}",
                b.content
            ),
        }
    }

    #[test]
    fn every_measured_body_round_trips_byte_for_byte() {
        for body in [
            measured_picture_body(),
            measured_pattern_body(),
            measured_blank_pattern_body(),
        ] {
            let parsed = read_backdrop(&body).unwrap();
            assert_eq!(write_backdrop(&parsed).unwrap(), body);
        }
    }

    #[test]
    fn a_release_pattern_chunk_with_depth_zero_and_256_bytes_is_accepted() {
        // DataLength is not Depth * 32. The release's own screen chunk is
        // Depth=0 with 256 bytes; refusing it would refuse AmigaOS itself.
        let b = read_backdrop(&measured_blank_pattern_body()).unwrap();
        match b.content {
            Content::Pattern { depth, ref planes } => {
                assert_eq!(depth, 0);
                assert_eq!(planes.len(), 256);
            }
            _ => panic!(
                "WBPF_PATTERN set must read as a pattern, got {:?}",
                b.content
            ),
        }
    }

    #[test]
    fn a_written_picture_backdrop_carries_the_sixteen_reserved_bytes() {
        let out = write_backdrop(&Backdrop {
            which: Which::Root,
            placement: Placement::Scale,
            precision: Precision::Image,
            dither: Dither::Good,
            no_remap: false,
            other_flags: 0,
            revision: 0,
            content: Content::Picture("Sys:X/y.iff".to_string()),
        })
        .unwrap();
        assert_eq!(
            &out[0..16],
            &[0u8; 16],
            "wbp_Reserved is 16 bytes, not zero"
        );
        assert_eq!(u16::from_be_bytes([out[18], out[19]]), 0x2A00);
        assert_eq!(
            out.len(),
            24 + "Sys:X/y.iff".len(),
            "no terminator: DataLength is the exact string length"
        );
        assert_eq!(
            u16::from_be_bytes([out[22], out[23]]) as usize,
            "Sys:X/y.iff".len()
        );
    }

    #[test]
    fn the_data_length_must_agree_with_what_follows() {
        let mut body = measured_picture_body();
        body[22..24].copy_from_slice(&9999u16.to_be_bytes());
        assert!(read_backdrop(&body).is_err());
    }

    #[test]
    fn a_body_shorter_than_the_header_is_refused() {
        assert!(read_backdrop(&[0u8; 23]).is_err());
    }

    #[test]
    fn a_picture_path_is_not_nul_terminated() {
        // Measured across four real PTRN chunks: DataLength is exactly the
        // string length. Appending a NUL would make every ART path one byte
        // longer than the one AmigaOS wrote.
        let mut body = vec![0u8; 16];
        body.extend_from_slice(&0u16.to_be_bytes());
        body.extend_from_slice(&0x2A00u16.to_be_bytes());
        body.push(0);
        body.push(0);
        body.extend_from_slice(&4u16.to_be_bytes());
        body.extend_from_slice(b"abcd");
        let b = read_backdrop(&body).unwrap();
        assert_eq!(b.content, Content::Picture("abcd".to_string()));
        assert_eq!(write_backdrop(&b).unwrap(), body);
    }

    #[test]
    fn a_trailing_nul_inside_the_data_is_kept_not_stripped() {
        // A path is taken verbatim for DataLength bytes. Stripping a NUL
        // would silently shorten a name and break the round trip.
        let mut body = vec![0u8; 16];
        body.extend_from_slice(&0u16.to_be_bytes());
        body.extend_from_slice(&0x2A00u16.to_be_bytes());
        body.push(0);
        body.push(0);
        body.extend_from_slice(&5u16.to_be_bytes());
        body.extend_from_slice(b"abcd\0");
        let b = read_backdrop(&body).unwrap();
        assert_eq!(b.content, Content::Picture("abcd\u{0}".to_string()));
        assert_eq!(write_backdrop(&b).unwrap(), body);
    }

    #[test]
    fn a_flag_bit_this_module_does_not_understand_survives_a_round_trip() {
        let mut body = measured_picture_body();
        // 0x0040 is in no mask this module defines.
        body[18..20].copy_from_slice(&(0x2A00u16 | 0x0040).to_be_bytes());
        let b = read_backdrop(&body).unwrap();
        assert_eq!(
            b.other_flags, 0x0040,
            "an unknown bit must be carried, not dropped"
        );
        assert_eq!(b.placement, Placement::Scale, "the known bits still decode");
        assert_eq!(write_backdrop(&b).unwrap(), body);
    }

    #[test]
    fn a_nonzero_revision_survives_a_round_trip() {
        let mut body = measured_picture_body();
        body[20] = 7;
        let b = read_backdrop(&body).unwrap();
        assert_eq!(b.revision, 7);
        assert_eq!(write_backdrop(&b).unwrap(), body);
    }
}
