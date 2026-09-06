//! The rendered size — the number a layout actually needs, not the `Gadget`
//! size the classic `DiskObject` header carries.
//!
//! Measured across 798 real icons (spec §1.3): the `Gadget` width/height at
//! offsets 12/14 is **not** the size Workbench actually draws in 613 of
//! 798 — 77 per cent. The extreme case is a NewIcon
//! (`Prefs/Presets/Animated GIFs/Amiga.gif.info`) whose `Gadget` says **3x3**
//! while its `IM1=` tool type says **46x46**, fifteen times larger. A layout
//! built on the `Gadget` size alone overlaps every label; this module is
//! the fix, and that one icon is why the round exists.
//!
//! The rule, in this order:
//!
//! 1. **ColorIcon.** An appended `FORM … ICON` IFF blob's `FACE` chunk gives
//!    width `data[face+8] + 1`, height `data[face+9] + 1`, frameless when
//!    `data[face+10] & 1`. **Trusted only when an `IMAG` chunk is also
//!    present** among the `FORM`'s sibling chunks — MagicWB-era icons (MUI
//!    3.8, MagicMenu) end in a degenerate `FACE` claiming 256x256 with no
//!    image data behind it at all; believing that makes one icon eat a
//!    whole window.
//! 2. **NewIcons.** The first `IM1=` tool type: width `data[im1+5] - 0x21`,
//!    height `data[im1+6] - 0x21` (`im1` is the byte offset of the `IM1=`
//!    marker within that one tool type's own bytes; the byte immediately
//!    after `=` at `im1+4` is not read here — see the module-level note
//!    below).
//! 3. **Otherwise** the `Gadget` width/height at 12/14 — the classic-only
//!    case, and also the floor for the two cases above.
//!
//! In every case the result is `max(gadget, found)` — never smaller than
//! the `Gadget` fields, because those are a real, if sometimes wrong,
//! minimum a caller can always fall back to.
//!
//! **What is not independently re-verified here.** The exact meaning of the
//! `IM1=` byte at `im1 + 4` (the NewIcon encoding reserves it for something
//! other than width/height — likely a palette or transparency indicator)
//! is not decoded; only the two size bytes the brief measured are read.
//! Likewise the ColorIcon `FACE` chunk's `Aspect` and `MaxPalBytes` fields
//! (bytes 11..14 relative to the chunk payload) are not read — this module
//! only needs the width, height and frameless bit to size a layout cell.

use super::{be_u16, layout, malformed, tooltypes, OFF_GADGET_HEIGHT, OFF_GADGET_WIDTH};
use crate::core::error::CoreResult;

/// The size a layout should actually reserve for this icon, plus whether it
/// is drawn with Workbench's usual gadget frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rendered {
    pub width: u16,
    pub height: u16,
    /// `true` unless a ColorIcon's `FACE` chunk says otherwise (bit 0 of its
    /// flags byte). Not measured for the NewIcon or plain-planar cases in
    /// this round — both default to framed.
    pub framed: bool,
}

/// The rendered size, per the module doc's three-case rule.
pub fn rendered_size(bytes: &[u8]) -> CoreResult<Rendered> {
    let parsed = layout(bytes)?;
    let gadget_w = be_u16(bytes, OFF_GADGET_WIDTH)?;
    let gadget_h = be_u16(bytes, OFF_GADGET_HEIGHT)?;

    if let Some((w, h, frameless)) = color_icon_face_size(bytes, parsed.trailing.clone())? {
        return Ok(Rendered {
            width: gadget_w.max(w),
            height: gadget_h.max(h),
            framed: !frameless,
        });
    }

    if let Some((w, h)) = newicon_im1_size(bytes)? {
        return Ok(Rendered {
            width: gadget_w.max(w),
            height: gadget_h.max(h),
            framed: true,
        });
    }

    Ok(Rendered {
        width: gadget_w,
        height: gadget_h,
        framed: true,
    })
}

/// Walk the sub-chunks of an appended `FORM … ICON` ColorIcon blob (`data`
/// is everything from just after the `ICON` form-type tag to the end of
/// the buffer) and report the byte offset of the first `FACE` chunk's own
/// `FACE` tag (so the caller can read its payload at `face + 8`), and
/// whether an `IMAG` chunk is present anywhere among the siblings.
///
/// Every chunk's declared size is checked against the buffer's real length
/// before its payload is touched — a chunk claiming to run past the file is
/// refused, never read past. Sizes are padded to an even length, matching
/// IFF's own word-alignment rule.
fn scan_color_icon_chunks(data: &[u8]) -> CoreResult<(Option<usize>, bool)> {
    let mut pos = 0usize;
    let mut face_offset = None;
    let mut has_imag = false;

    while pos.checked_add(8).is_some_and(|end| end <= data.len()) {
        let id = &data[pos..pos + 4];
        let size = u32::from_be_bytes([data[pos + 4], data[pos + 5], data[pos + 6], data[pos + 7]])
            as usize;

        let payload_start = pos
            .checked_add(8)
            .ok_or_else(|| malformed("IFF chunk offset overflow"))?;
        let payload_end = payload_start
            .checked_add(size)
            .ok_or_else(|| malformed("IFF chunk size overflow"))?;
        if payload_end > data.len() {
            return Err(malformed(
                "truncated icon: an appended IFF chunk runs past the file",
            ));
        }

        if id == b"FACE" {
            if size < 3 {
                return Err(malformed("FACE chunk shorter than its required fields"));
            }
            if face_offset.is_none() {
                face_offset = Some(pos);
            }
        }
        if id == b"IMAG" {
            has_imag = true;
        }

        let padded = if size % 2 == 1 { size + 1 } else { size };
        pos = payload_start
            .checked_add(padded)
            .ok_or_else(|| malformed("IFF chunk padding overflow"))?;
    }

    Ok((face_offset, has_imag))
}

/// The ColorIcon `FACE` size, or `None` when there is no appended `FORM …
/// ICON` blob, or its `FACE` chunk has no `IMAG` chunk backing it (the
/// degenerate MagicWB case this module refuses to trust).
fn color_icon_face_size(
    bytes: &[u8],
    trailing: std::ops::Range<usize>,
) -> CoreResult<Option<(u16, u16, bool)>> {
    let region = bytes
        .get(trailing)
        .ok_or_else(|| malformed("trailing region runs past the file"))?;

    if region.len() < 12 || &region[0..4] != b"FORM" || &region[8..12] != b"ICON" {
        return Ok(None);
    }

    let (face_offset, has_imag) = scan_color_icon_chunks(&region[12..])?;
    let Some(face_rel) = face_offset else {
        return Ok(None);
    };
    if !has_imag {
        return Ok(None);
    }

    // `scan_color_icon_chunks` already confirmed at least 3 payload bytes
    // exist behind this FACE chunk's own header (id + size = 8 bytes), so
    // `face + 8 .. face + 11` is proven in bounds.
    let face = 12 + face_rel;
    let width = region[face + 8] as u16 + 1;
    let height = region[face + 9] as u16 + 1;
    let frameless = region[face + 10] & 1 != 0;
    Ok(Some((width, height, frameless)))
}

/// The first `IM1=` tool type's encoded width/height, or `None` when no
/// tool type carries one.
fn newicon_im1_size(bytes: &[u8]) -> CoreResult<Option<(u16, u16)>> {
    for tt in tooltypes(bytes)? {
        let tt_bytes = tt.as_bytes();
        let Some(im1) = find_subsequence(tt_bytes, b"IM1=") else {
            continue;
        };
        let width_at = im1
            .checked_add(5)
            .ok_or_else(|| malformed("IM1 offset overflow"))?;
        let height_at = im1
            .checked_add(6)
            .ok_or_else(|| malformed("IM1 offset overflow"))?;
        let (Some(&w_byte), Some(&h_byte)) = (tt_bytes.get(width_at), tt_bytes.get(height_at))
        else {
            return Err(malformed(
                "truncated icon: an IM1= tool type is shorter than its encoded size",
            ));
        };
        let width = w_byte
            .checked_sub(0x21)
            .ok_or_else(|| malformed("IM1 width byte below its encoding base (0x21)"))?
            as u16;
        let height = h_byte
            .checked_sub(0x21)
            .ok_or_else(|| malformed("IM1 height byte below its encoding base (0x21)"))?
            as u16;
        return Ok(Some((width, height)));
    }
    Ok(None)
}

/// The first index at which `needle` occurs in `haystack`, or `None`.
fn find_subsequence(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || haystack.len() < needle.len() {
        return None;
    }
    haystack.windows(needle.len()).position(|w| w == needle)
}

/// Fixture building shared by this module's own tests **and, for
/// [`synthetic_colour_icon`], by `core::appearance`'s own tests** — see that
/// module's `a_frameless_icon_gets_a_narrower_footprint_than_a_framed_one`,
/// which needs a genuinely frameless ColorIcon (backed by an `IMAG` chunk,
/// the only shape [`rendered_size`] trusts) to prove `Rendered::framed`
/// reaches `icongrid::Cell::framed` rather than being defaulted. Reusing
/// this builder rather than a third hand-rolled copy is why it and its
/// containing module are `pub(crate)` rather than `pub(super)` — extends
/// [`super::tests_support`] rather than duplicating its `DiskObject` byte
/// layout — see that module's doc comment for why a second hand-rolled copy
/// would be a second place for the offsets to drift.
#[cfg(test)]
pub(crate) mod tests_support {
    use super::super::tests_support::synthetic_icon;
    use super::super::{OFF_GADGET_HEIGHT, OFF_GADGET_WIDTH};

    /// An icon carrying an appended `FORM … ICON` ColorIcon blob whose
    /// `FACE` chunk claims `face_w`x`face_h`, with an `IMAG` chunk following
    /// it only when `with_imag` is true. `frameless` sets the `FACE`
    /// chunk's flags bit 0 — [`rendered_size`]'s own doc: frameless when
    /// that bit is set, framed otherwise — and only has any effect when
    /// `with_imag` is also true, since a `FACE` with no backing `IMAG` is
    /// not trusted at all (the degenerate MagicWB case).
    pub(crate) fn synthetic_colour_icon(
        gadget_w: u16,
        gadget_h: u16,
        face_w: u16,
        face_h: u16,
        with_imag: bool,
        frameless: bool,
    ) -> Vec<u8> {
        let face_payload: [u8; 6] = [
            (face_w.saturating_sub(1)) as u8,
            (face_h.saturating_sub(1)) as u8,
            u8::from(frameless), // Flags: bit 0 set means frameless
            0,                   // Aspect: not read
            0,
            0, // MaxPalBytes: not read
        ];

        let mut chunks = Vec::new();
        chunks.extend_from_slice(b"FACE");
        chunks.extend_from_slice(&(face_payload.len() as u32).to_be_bytes());
        chunks.extend_from_slice(&face_payload);
        // face_payload.len() is 6, already even - no pad byte needed.

        if with_imag {
            let imag_payload = [0u8; 4]; // contents unread; only presence matters
            chunks.extend_from_slice(b"IMAG");
            chunks.extend_from_slice(&(imag_payload.len() as u32).to_be_bytes());
            chunks.extend_from_slice(&imag_payload);
        }

        let mut form = Vec::new();
        form.extend_from_slice(b"FORM");
        let form_size = (4 + chunks.len()) as u32; // "ICON" tag + all sub-chunks
        form.extend_from_slice(&form_size.to_be_bytes());
        form.extend_from_slice(b"ICON");
        form.extend_from_slice(&chunks);

        let mut buf = synthetic_icon(&[], 4096, &form);
        buf[OFF_GADGET_WIDTH..OFF_GADGET_WIDTH + 2].copy_from_slice(&gadget_w.to_be_bytes());
        buf[OFF_GADGET_HEIGHT..OFF_GADGET_HEIGHT + 2].copy_from_slice(&gadget_h.to_be_bytes());
        buf
    }

    /// An icon carrying a single `IM1=` NewIcon tool type encoding
    /// `im1_w`x`im1_h`, with a `Gadget` size of `gadget_w`x`gadget_h`.
    pub(super) fn synthetic_newicon(
        gadget_w: u16,
        gadget_h: u16,
        im1_w: u16,
        im1_h: u16,
    ) -> Vec<u8> {
        let mut im1 = vec![b'I', b'M', b'1', b'=', b'0'];
        im1.push((im1_w as u8).wrapping_add(0x21));
        im1.push((im1_h as u8).wrapping_add(0x21));
        let tt = String::from_utf8(im1).expect("encoded IM1 bytes are ASCII, hence valid UTF-8");

        let mut buf = synthetic_icon(&[&tt], 4096, &[]);
        buf[OFF_GADGET_WIDTH..OFF_GADGET_WIDTH + 2].copy_from_slice(&gadget_w.to_be_bytes());
        buf[OFF_GADGET_HEIGHT..OFF_GADGET_HEIGHT + 2].copy_from_slice(&gadget_h.to_be_bytes());
        buf
    }
}

#[cfg(test)]
mod tests {
    use super::super::tests_support::{synthetic_icon, synthetic_icon_sized};
    use super::tests_support::{synthetic_colour_icon, synthetic_newicon};
    use super::*;

    #[test]
    fn a_colour_icon_uses_its_face_chunk_not_its_gadget_size() {
        // Measured: art1/Devs/DataTypes.info says 44x44 in the Gadget and
        // 46x46 in FACE - which is why a layout cannot use the Gadget
        // fields.
        let icon = synthetic_colour_icon(44, 44, 46, 46, true, false);
        let r = rendered_size(&icon).unwrap();
        assert_eq!((r.width, r.height), (46, 46));
    }

    #[test]
    fn a_degenerate_face_with_no_imag_is_not_trusted() {
        // MagicWB-era icons claim 256x256 in FACE with no image behind it.
        // Believing that makes one icon eat a whole window.
        let icon = synthetic_colour_icon(32, 32, 256, 256, false, false);
        let r = rendered_size(&icon).unwrap();
        assert_eq!(
            (r.width, r.height),
            (32, 32),
            "fall back to the planar size"
        );
    }

    #[test]
    fn a_newicon_uses_its_im1_tooltype_not_its_stub_gadget() {
        // Measured: art1/Prefs/Presets/Animated GIFs/Amiga.gif.info has a
        // 3x3 Gadget and a 46x46 IM1= - fifteen times larger. This single
        // case is why the round exists.
        let icon = synthetic_newicon(3, 3, 46, 46);
        let r = rendered_size(&icon).unwrap();
        assert_eq!((r.width, r.height), (46, 46));
    }

    #[test]
    fn a_plain_planar_icon_uses_its_gadget_size() {
        // Assert the EXACT size the fixture was built with. "greater than
        // zero" would pass for any wrong answer, including a hard-coded
        // one.
        let icon = synthetic_icon_sized(35, 18);
        let r = rendered_size(&icon).unwrap();
        assert_eq!((r.width, r.height), (35, 18));
    }

    #[test]
    fn the_result_is_never_smaller_than_the_gadget() {
        let icon = synthetic_colour_icon(64, 64, 16, 16, true, false);
        let r = rendered_size(&icon).unwrap();
        assert_eq!((r.width, r.height), (64, 64));
    }

    #[test]
    fn a_truncated_face_or_im1_is_refused_not_read_past() {
        // A FACE chunk cut short mid-payload (no IMAG sibling, so it is the
        // last thing in the buffer) must be refused by the chunk walk's own
        // bounds check, not read past.
        let whole = synthetic_colour_icon(32, 32, 46, 46, false, false);
        let cut = &whole[..whole.len() - 2];
        assert!(
            rendered_size(cut).is_err(),
            "a FACE chunk cut short must be refused"
        );

        // An IM1= tool type present but shorter than its two encoded size
        // bytes must be refused the same way.
        let short_im1 = synthetic_icon(&["IM1=0"], 4096, &[]);
        assert!(
            rendered_size(&short_im1).is_err(),
            "an IM1= tool type shorter than its encoded size must be refused"
        );
    }
}
