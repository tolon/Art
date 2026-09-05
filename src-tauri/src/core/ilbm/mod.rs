//! A minimal IFF ILBM encoder: enough to write a single indexed backdrop
//! image, and nothing else. A stock AmigaOS 3.2 tree carries `ilbm.datatype`
//! and nothing that can *produce* an ILBM, so a PNG or JPEG the user supplies
//! must be turned into one host-side before Task 7 can point `WBPattern.prefs`
//! at it.
//!
//! This module takes already-quantised pixels (`Indexed`, built by Task 6)
//! and returns bytes. It touches no filesystem and makes no assumption about
//! where its output goes.
//!
//! # `BMHD`, field by field: measured, reasoned, or arbitrary
//!
//! Every earlier task in this round read an existing file, so an unhandled
//! field could be carried through untouched. This module *writes* a file
//! from scratch, so every `BMHD` byte is a value this code chose — and three
//! consecutive reviews this round found the same defect: a struct designed
//! from the fields ART *needs* rather than the fields the *file* carries,
//! silently zeroing the rest. The question here is which of these 13 fields
//! ART invented.
//!
//! A real Amiga-drawn ILBM was measured for this round —
//! `UhrenskinAMIGA.iff`, 216x213 — and its `BMHD` does **not** match what
//! this encoder writes in every field. Both sets of values are recorded
//! below, with the reason for the difference where one exists.
//!
//! - **`width`, `height`** (`u16`) — taken directly from the caller's
//!   `Indexed`. Not invented; there is nothing to reason about.
//! - **`x`, `y`** (`i16`, both `0`) — reasoned, not measured. These are a
//!   sub-image's offset within a larger composite; a standalone backdrop is
//!   not part of one, so `0, 0` is the conventional value nearly every
//!   encoder writes for a whole-image `BMHD`. `UhrenskinAMIGA.iff`'s own `x`,
//!   `y` were not checked, since they answer a question ("where does this
//!   sit relative to something else") that does not apply to a backdrop.
//! - **`nPlanes`** (`u8`) — computed by [`planes_for`] from the palette this
//!   call was given. Not invented: it is derived from real data (the
//!   caller's own palette size), just not copied from the reference file.
//! - **`masking`** (`u8`, always `0`, `mskNone`) — reasoned, and
//!   deliberately **not** what was measured. The reference file carries `2`
//!   (`mskHasTransparentColor`) — plausible for artwork meant to sit over
//!   something else. A Workbench backdrop tiles behind every icon with
//!   nothing showing through, so `mskNone` is the correct value for this
//!   module's one use, even though it disagrees with the file that was
//!   measured.
//! - **`compression`** (`u8`, always `1`, ByteRun1) — reasoned: this is
//!   ART's own choice of algorithm (`packbits`), asserted rather than
//!   discovered. `0` (uncompressed) is the only other value the format
//!   defines and this encoder never produces it.
//! - **`pad1`** (`u8`, always `0`) — reasoned correction of a measured
//!   defect. The reference file carries `0x80` here. The ILBM `BMHD` layout
//!   reserves this byte with no defined meaning, so `UhrenskinAMIGA.iff`'s
//!   writer left stray data in a byte it never should have touched; `0` is
//!   what the format actually calls for, not what the one sample happened to
//!   contain.
//! - **`transparentColor`** (`u16`, always `0`) — arbitrary, and inert by
//!   construction: it is only meaningful when `masking = mskHasTransparentColor`,
//!   which this encoder never sets. Not measured — irrelevant to the
//!   reference file's own use of masking, and irrelevant here.
//! - **`xAspect`, `yAspect`** (`u8`, both `22`) — measured, and it agrees
//!   with an independent fact: `UhrenskinAMIGA.iff` carries `22:22`, which is
//!   also the long-documented Amiga pixel aspect ratio for a standard
//!   display. One real file and one piece of outside knowledge point the
//!   same way, so this is the one field with two sources rather than a
//!   choice.
//! - **`pageWidth`, `pageHeight`** (`i16`) — reasoned, and deliberately
//!   **not** what was measured. `UhrenskinAMIGA.iff` records `640x480` — the
//!   *screen* it was drawn for, not its own `216x213`. ART has no way to
//!   know what screen mode a user's backdrop is destined for, so guessing a
//!   screen size would be inventing a fact this module cannot know. Writing
//!   the image's own dimensions as its page is the defensible default for an
//!   image that is not known to belong to any particular screen — an image
//!   that is its own page — while knowingly disagreeing with what the one
//!   sample file recorded.

use crate::core::error::{CoreError, CoreResult};

pub mod packbits;

fn malformed(detail: &str) -> CoreError {
    CoreError::Malformed {
        format: "ILBM".to_string(),
        detail: detail.to_string(),
    }
}

/// The IFF chunk id byte length of a `BMHD` body — always 20 for this
/// encoder's fixed field set.
const BMHD_LEN: usize = 20;

/// The reference file's own aspect ratio, and also the documented standard
/// Amiga pixel aspect for a normal display — see the module doc's `xAspect`,
/// `yAspect` entry.
const ASPECT: u8 = 22;

/// An already-quantised image, ready to become an ILBM `BODY`.
///
/// `pixels` is one byte per pixel, row-major, each value an index into
/// `palette`. Task 6 builds this from a decoded PNG/JPEG; Task 7 feeds
/// [`encode`]'s output into the distribution tree.
#[derive(Debug, Clone, PartialEq)]
pub struct Indexed {
    pub width: u16,
    pub height: u16,
    pub palette: Vec<[u8; 3]>,
    pub pixels: Vec<u8>,
}

/// How many bit planes are needed to index `colours` distinct values:
/// `max(1, ceil(log2(colours)))`, capped at 8 (a byte can index no more than
/// 256 colours, so an `Indexed` with more than 256 palette entries is refused
/// by [`encode`] before this would ever need to answer for one).
pub fn planes_for(colours: usize) -> u8 {
    let mut planes = 0u8;
    let mut indexable: usize = 1;
    while indexable < colours && planes < 8 {
        indexable <<= 1;
        planes += 1;
    }
    planes.max(1)
}

/// Append one IFF chunk — id, big-endian size, body, and the odd-size pad
/// byte the size field itself does not count. Same rule `amigaprefs::iff`
/// uses for the files it reads; a chunk this encoder writes must satisfy the
/// same reader.
fn write_chunk(out: &mut Vec<u8>, id: &[u8; 4], body: &[u8]) -> CoreResult<()> {
    let size = u32::try_from(body.len())
        .map_err(|_| malformed("a chunk body does not fit an IFF size field"))?;
    out.extend_from_slice(id);
    out.extend_from_slice(&size.to_be_bytes());
    out.extend_from_slice(body);
    if body.len() % 2 == 1 {
        out.push(0);
    }
    Ok(())
}

/// Build one plane's packed row bytes for image row `y`.
///
/// `width` is a `u16` widened to `usize`, so `width.div_ceil(16)` cannot
/// overflow on any platform this builds for; the caller has already
/// validated `pixels.len() == width * height` via `checked_mul`, so
/// `y * width + x` stays in bounds for every `x` this loop visits.
///
/// The row is padded to a whole number of **words** (`div_ceil(16)`, not
/// `div_ceil(8)`) because AmigaDOS's own ILBM readers assume a plane row
/// starts on a word boundary — packing to the nearest byte instead would
/// produce a file this encoder's own tests catch, but a real Amiga datatype
/// would not necessarily reject.
fn plane_row(pixels: &[u8], y: usize, width: usize, plane: u8) -> Vec<u8> {
    let row_bytes = width.div_ceil(16) * 2;
    let mut buf = vec![0u8; row_bytes];
    for x in 0..width {
        let value = pixels[y * width + x];
        if (value >> plane) & 1 == 1 {
            let byte_index = x / 8;
            let bit_index = 7 - (x % 8);
            buf[byte_index] |= 1 << bit_index;
        }
    }
    buf
}

/// Encode an indexed image as `FORM`...`ILBM` with a `ByteRun1`-compressed
/// `BODY`. See the module doc for exactly which `BMHD` fields are measured,
/// reasoned, or arbitrary.
pub fn encode(image: &Indexed) -> CoreResult<Vec<u8>> {
    let width = image.width as usize;
    let height = image.height as usize;
    let pixel_count = width
        .checked_mul(height)
        .ok_or_else(|| malformed("width times height overflows"))?;
    if image.pixels.len() != pixel_count {
        return Err(malformed(
            "pixel buffer length does not equal width times height",
        ));
    }
    if image.palette.is_empty() {
        return Err(malformed("palette must not be empty"));
    }
    if image.palette.len() > 256 {
        return Err(malformed("palette holds more than 256 colours"));
    }
    for &value in &image.pixels {
        if value as usize >= image.palette.len() {
            return Err(malformed("a pixel index is outside the palette"));
        }
    }

    let planes = planes_for(image.palette.len());
    // planes_for(n) always returns enough planes to index n colours for
    // every n <= 256 (the bound already enforced above), so
    // `1 << planes >= palette.len()` can never fail at runtime — this is an
    // invariant on the record, not a reachable guard.
    debug_assert!(
        image.palette.len() <= 1usize << planes,
        "planes_for must return enough planes to index the palette"
    );

    if image.width > i16::MAX as u16 || image.height > i16::MAX as u16 {
        return Err(malformed(&format!(
            "an ILBM page dimension is a signed 16-bit field, so {}x{} cannot be written",
            image.width, image.height
        )));
    }

    let width_u16 = image.width;
    let height_u16 = image.height;

    let mut bmhd = Vec::with_capacity(BMHD_LEN);
    bmhd.extend_from_slice(&width_u16.to_be_bytes());
    bmhd.extend_from_slice(&height_u16.to_be_bytes());
    bmhd.extend_from_slice(&0i16.to_be_bytes()); // x
    bmhd.extend_from_slice(&0i16.to_be_bytes()); // y
    bmhd.push(planes);
    bmhd.push(0); // masking = mskNone
    bmhd.push(1); // compression = ByteRun1
    bmhd.push(0); // pad1
    bmhd.extend_from_slice(&0u16.to_be_bytes()); // transparentColor
    bmhd.push(ASPECT); // xAspect
    bmhd.push(ASPECT); // yAspect
    bmhd.extend_from_slice(&(width_u16 as i16).to_be_bytes()); // pageWidth
    bmhd.extend_from_slice(&(height_u16 as i16).to_be_bytes()); // pageHeight
    debug_assert_eq!(bmhd.len(), BMHD_LEN);

    let mut cmap = Vec::with_capacity(image.palette.len() * 3);
    for colour in &image.palette {
        cmap.extend_from_slice(colour);
    }

    let mut body = Vec::new();
    for y in 0..height {
        for plane in 0..planes {
            let row = plane_row(&image.pixels, y, width, plane);
            body.extend_from_slice(&packbits::pack_row(&row));
        }
    }

    let mut content = Vec::new();
    content.extend_from_slice(b"ILBM");
    write_chunk(&mut content, b"BMHD", &bmhd)?;
    write_chunk(&mut content, b"CMAP", &cmap)?;
    write_chunk(&mut content, b"BODY", &body)?;

    let form_size = u32::try_from(content.len())
        .map_err(|_| malformed("the FORM does not fit an IFF size field"))?;
    let mut out = Vec::with_capacity(content.len() + 8);
    out.extend_from_slice(b"FORM");
    out.extend_from_slice(&form_size.to_be_bytes());
    out.extend_from_slice(&content);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::ilbm::packbits::unpack_row;

    fn two_by_two_black_and_white() -> Indexed {
        Indexed {
            width: 2,
            height: 2,
            palette: vec![[0, 0, 0], [255, 255, 255]],
            pixels: vec![0, 1, 1, 0],
        }
    }

    #[test]
    fn a_two_colour_image_uses_one_plane_not_eight() {
        assert_eq!(planes_for(2), 1);
        assert_eq!(planes_for(3), 2);
        assert_eq!(planes_for(16), 4);
        assert_eq!(planes_for(256), 8);
    }

    #[test]
    fn the_form_type_is_ilbm() {
        let out = encode(&two_by_two_black_and_white()).unwrap();
        assert_eq!(&out[0..4], b"FORM");
        assert_eq!(&out[8..12], b"ILBM");
    }

    #[test]
    fn the_bmhd_carries_the_measured_field_order() {
        let out = encode(&two_by_two_black_and_white()).unwrap();
        assert_eq!(&out[12..16], b"BMHD");
        assert_eq!(u32::from_be_bytes([out[16], out[17], out[18], out[19]]), 20);
        assert_eq!(u16::from_be_bytes([out[20], out[21]]), 2, "width");
        assert_eq!(u16::from_be_bytes([out[22], out[23]]), 2, "height");
        assert_eq!(out[28], 1, "nPlanes for two colours");
        assert_eq!(out[29], 0, "masking is mskNone for a backdrop");
        assert_eq!(out[30], 1, "compression is ByteRun1");
        assert_eq!(out[31], 0, "pad1");
    }

    #[test]
    fn the_cmap_holds_three_bytes_for_every_colour_the_planes_can_index() {
        let out = encode(&two_by_two_black_and_white()).unwrap();
        let at = out.windows(4).position(|w| w == b"CMAP").unwrap();
        let size = u32::from_be_bytes([out[at + 4], out[at + 5], out[at + 6], out[at + 7]]);
        assert_eq!(size, 2 * 3, "one plane indexes two colours");
    }

    #[test]
    fn a_palette_larger_than_256_entries_is_refused() {
        let mut img = two_by_two_black_and_white();
        img.palette = vec![[0, 0, 0]; 300];
        let err = encode(&img).unwrap_err();
        assert!(
            format!("{err}").contains("256"),
            "the refusal must name the limit, got: {err}"
        );
    }

    #[test]
    fn a_dimension_too_large_for_a_signed_page_field_is_refused_not_wrapped() {
        let img = Indexed {
            width: 40_000,
            height: 1,
            palette: vec![[0, 0, 0], [255, 255, 255]],
            pixels: vec![1; 40_000],
        };
        assert!(
            encode(&img).is_err(),
            "40000 does not fit an i16 page field"
        );
    }

    #[test]
    fn an_odd_length_chunk_is_padded_and_the_size_field_is_not() {
        // three colours -> CMAP is 9 bytes, which is odd.
        let img = Indexed {
            width: 2,
            height: 2,
            palette: vec![[0, 0, 0], [255, 255, 255], [255, 0, 0]],
            pixels: vec![0, 1, 2, 0],
        };
        let out = encode(&img).unwrap();
        let at = out.windows(4).position(|w| w == b"CMAP").unwrap();
        let size = u32::from_be_bytes([out[at + 4], out[at + 5], out[at + 6], out[at + 7]]);
        assert_eq!(size, 9, "the size field states the real length");
        assert_eq!(out[at + 8 + 9], 0, "an odd body is followed by a pad byte");
        assert_eq!(out.len() % 2, 0, "the file itself stays word-aligned");
    }

    #[test]
    fn a_pixel_index_outside_the_palette_is_refused() {
        let mut img = two_by_two_black_and_white();
        img.pixels = vec![0, 1, 1, 9];
        assert!(encode(&img).is_err());
    }

    #[test]
    fn pixels_must_be_width_times_height() {
        let mut img = two_by_two_black_and_white();
        img.pixels = vec![0, 1, 1];
        assert!(encode(&img).is_err());
    }

    #[test]
    fn a_row_is_padded_to_a_whole_number_of_words() {
        // 17 pixels wide needs ((17 + 15) / 16) * 2 = 4 bytes per plane row.
        let img = Indexed {
            width: 17,
            height: 1,
            palette: vec![[0, 0, 0], [255, 255, 255]],
            pixels: vec![1; 17],
        };
        let out = encode(&img).unwrap();
        let at = out.windows(4).position(|w| w == b"BODY").unwrap();
        let size =
            u32::from_be_bytes([out[at + 4], out[at + 5], out[at + 6], out[at + 7]]) as usize;
        let unpacked = unpack_row(&out[at + 8..at + 8 + size], 4).unwrap();
        assert_eq!(unpacked.len(), 4);
        assert_eq!(
            unpacked,
            vec![0xFF, 0xFF, 0x80, 0x00],
            "17 set bits, then padding"
        );
    }

    #[test]
    fn a_run_never_crosses_a_row_boundary() {
        // 16 pixels wide, two rows, one plane, every pixel set: each plane row is
        // [0xFF, 0xFF]. Packed per row that is two independent two-byte rows;
        // packed as one buffer the four identical bytes collapse into a single
        // run, which is a different and wrong BODY - a decoder unpacking row by
        // row would desync on the second row and every row after it.
        let img = Indexed {
            width: 16,
            height: 2,
            palette: vec![[0, 0, 0], [255, 255, 255]],
            pixels: vec![1; 32],
        };
        let out = encode(&img).unwrap();
        let at = out.windows(4).position(|w| w == b"BODY").unwrap();
        let size =
            u32::from_be_bytes([out[at + 4], out[at + 5], out[at + 6], out[at + 7]]) as usize;
        let body = &out[at + 8..at + 8 + size];

        let one_row = [0xFFu8, 0xFF];
        let mut per_row = packbits::pack_row(&one_row);
        per_row.extend(packbits::pack_row(&one_row));
        assert_eq!(
            body,
            per_row.as_slice(),
            "each plane row is packed on its own"
        );

        let whole = packbits::pack_row(&[0xFF, 0xFF, 0xFF, 0xFF]);
        assert_ne!(
            body,
            whole.as_slice(),
            "packing the rows together is the defect this test exists for"
        );
    }
}
