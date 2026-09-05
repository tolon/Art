//! Decode a user's own picture and get it ready for [`quantise::quantise`] and,
//! from there, `core::ilbm::encode`.
//!
//! A stock AmigaOS 3.2 tree carries `ilbm.datatype` and nothing that can
//! *produce* one, so before Task 7 can build a wallpaper for the distribution
//! tree, a PNG or JPEG the user already owns has to be turned into plain RGB
//! pixels host-side. This module is that first step: [`decode`] takes bytes
//! and returns [`Rgb`], [`scale_to_fit`] shrinks it to a screen size without
//! ever enlarging, and `quantise::quantise` (next module) turns the result
//! into the palette-plus-indices shape `core::ilbm::encode` already expects.
//!
//! Nothing here touches a filesystem or the network — bytes in, pixels out —
//! so it stays inside `core/`'s platform-independence rule the same way
//! `core::ilbm` and `core::archive`'s decompressors do.
//!
//! # Refuse, never substitute
//!
//! A file this module cannot decode is refused with a message naming what
//! ART accepts (PNG, JPEG). It never falls back to a grey placeholder or a
//! best-effort guess at a pixel format it does not fully trust itself to
//! read correctly — a wrong-but-confident wallpaper is a worse outcome than
//! a refusal the user can act on by choosing a different file.
//!
//! # Constants and choices, each labelled honestly
//!
//! - **[`MAX_SOURCE_BYTES`] (32 MiB)** — arbitrary, not measured. It exists
//!   to refuse an oversized source *before* any decoder allocates a byte for
//!   it, the same "bound the read before it happens" discipline
//!   `core/security` applies to archive entries. 32 MiB is far larger than
//!   any reasonable desktop wallpaper source file while still refusing
//!   comfortably before the `png` crate's own default 64 MiB decode-buffer
//!   limit would otherwise be the only thing standing between a hostile
//!   file and a large allocation.
//! - **PNG dimension check (`u16::MAX` in each axis)** — reasoned, not
//!   arbitrary: [`Rgb::width`]/[`Rgb::height`] are `u16`, so a PNG whose
//!   `IHDR` states a larger dimension (PNG allows up to `u32::MAX`) cannot be
//!   represented and is refused rather than silently truncated. JPEG needs
//!   no equivalent check: its own `SOF` marker stores each dimension in 16
//!   bits, so `jpeg-decoder`'s `ImageInfo::{width,height}` are already `u16`
//!   and can never be a value this module would have to reject.
//! - **Box filter, never enlarging** — [`scale_to_fit`] only ever shrinks:
//!   the scale factor is `min(max_w / src.width, max_h / src.height)`, and
//!   the function returns *before* computing that factor when the source
//!   already fits, so the factor used to reach a resized image is always
//!   `<= 1`. A box filter (area-average per destination pixel) rather than
//!   nearest-neighbour or a weighted kernel, because a Workbench backdrop is
//!   shrunk once and never resampled again — a box filter is the simplest
//!   choice that does not alias when shrinking, and there is no repeated
//!   resampling here for a sharper kernel's ringing to matter.
//! - **Only 8-bit grayscale and RGB JPEGs decode** — reasoned. A JPEG whose
//!   `jpeg-decoder` pixel format is `CMYK32` or `L16` is refused by name
//!   rather than converted, because a JPEG's CMYK channel order is only
//!   correct once Adobe's (frequently inverted) convention is known, and
//!   guessing wrong there is exactly the "confident, wrong" failure this
//!   project pays most for. `RGB24` and `L8` are what a camera, phone, or
//!   ordinary screenshot produces, and `jpeg-decoder` has already applied
//!   the YCbCr-to-RGB conversion by the time either format reaches this
//!   module.

use crate::core::error::{CoreError, CoreResult};

pub mod quantise;

/// A source file over this many bytes is refused before any decoder reads
/// it. See the module doc's "Constants and choices" section for why 32 MiB.
pub const MAX_SOURCE_BYTES: usize = 32 * 1024 * 1024;

const PNG_MAGIC: [u8; 8] = [0x89, b'P', b'N', b'G', b'\r', b'\n', 0x1a, b'\n'];
const JPEG_MAGIC: [u8; 3] = [0xFF, 0xD8, 0xFF];

fn malformed(format: &str, detail: &str) -> CoreError {
    CoreError::Malformed {
        format: format.to_string(),
        detail: detail.to_string(),
    }
}

/// A fully decoded picture: one `[u8; 3]` per pixel, row-major, no palette.
///
/// [`decode`] produces this from a PNG or JPEG; [`scale_to_fit`] resizes one;
/// `quantise::quantise` turns one into an `Indexed` for `core::ilbm::encode`.
#[derive(Debug, Clone, PartialEq)]
pub struct Rgb {
    pub width: u16,
    pub height: u16,
    pub pixels: Vec<[u8; 3]>,
}

/// Decode a PNG or JPEG from bytes. Refuses anything larger than
/// [`MAX_SOURCE_BYTES`] before either decoder ever sees the bytes, and
/// refuses anything that is neither by name — see the module doc's "Refuse,
/// never substitute" section.
pub fn decode(bytes: &[u8]) -> CoreResult<Rgb> {
    if bytes.len() > MAX_SOURCE_BYTES {
        return Err(malformed(
            "picture",
            &format!(
                "a {} byte source is over ART's {} byte limit for a wallpaper image; refused \
                 before any decoder reads it",
                bytes.len(),
                MAX_SOURCE_BYTES
            ),
        ));
    }
    if bytes.starts_with(&PNG_MAGIC) {
        decode_png(bytes)
    } else if bytes.starts_with(&JPEG_MAGIC) {
        decode_jpeg(bytes)
    } else {
        Err(malformed(
            "picture",
            "ART can turn a PNG or a JPEG into a wallpaper; this file is neither",
        ))
    }
}

fn decode_png(bytes: &[u8]) -> CoreResult<Rgb> {
    let mut decoder = png::Decoder::new(bytes);
    // EXPAND turns a palette image into full RGB(A) and a sub-8-bit
    // grayscale image up to 8 bits; STRIP_16 reduces a 16-bit-per-sample
    // image to 8. Together every PNG this can decode ends up 8 bits per
    // sample in one of four colour types, which the match below handles.
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = decoder
        .read_info()
        .map_err(|err| malformed("PNG", &format!("the PNG header could not be read: {err}")))?;

    let header = reader.info();
    let (width, height) = (header.width, header.height);
    if width > u16::MAX as u32 || height > u16::MAX as u32 {
        return Err(malformed(
            "PNG",
            &format!(
                "a {width}x{height} PNG is wider or taller than ART can hold in a 16-bit image \
                 dimension"
            ),
        ));
    }

    let mut buf = vec![0u8; reader.output_buffer_size()];
    let frame = reader.next_frame(&mut buf).map_err(|err| {
        malformed(
            "PNG",
            &format!("the PNG image data could not be decoded: {err}"),
        )
    })?;

    if frame.bit_depth != png::BitDepth::Eight {
        return Err(malformed(
            "PNG",
            "ART's PNG reader expects 8 bits per sample once its own decode transform has run; \
             this file left a different depth in place",
        ));
    }

    let data = &buf[..frame.buffer_size()];
    let pixels: Vec<[u8; 3]> = match frame.color_type {
        png::ColorType::Rgb => data.as_chunks::<3>().0.to_vec(),
        png::ColorType::Rgba => data
            .as_chunks::<4>()
            .0
            .iter()
            .map(|c| [c[0], c[1], c[2]])
            .collect(),
        png::ColorType::Grayscale => data.iter().map(|&g| [g, g, g]).collect(),
        png::ColorType::GrayscaleAlpha => data
            .as_chunks::<2>()
            .0
            .iter()
            .map(|c| [c[0], c[0], c[0]])
            .collect(),
        png::ColorType::Indexed => {
            // `Transformations::EXPAND` above always turns a palette image
            // into Rgb or Rgba before `next_frame` returns (that is the
            // documented behaviour of the flag), so this arm is not
            // reachable from any PNG this decoder can produce. It still
            // returns a refusal rather than `unreachable!()`: if that
            // library guarantee is ever wrong, ART should say so rather
            // than crash on, or silently misread, a user's file.
            return Err(malformed(
                "PNG",
                "an indexed PNG reached ART's RGB conversion step still carrying palette indices",
            ));
        }
    };

    Ok(Rgb {
        width: width as u16,
        height: height as u16,
        pixels,
    })
}

fn decode_jpeg(bytes: &[u8]) -> CoreResult<Rgb> {
    let mut decoder = jpeg_decoder::Decoder::new(bytes);
    let raw = decoder.decode().map_err(|err| {
        malformed(
            "JPEG",
            &format!("the JPEG image data could not be decoded: {err}"),
        )
    })?;
    // `jpeg_decoder::Decoder::info()` returns `None` only before `decode()`
    // has returned `Ok` (see its own doc comment); that already happened on
    // the line above, so this is an established library invariant rather
    // than a state this module has to guard against.
    let info = decoder
        .info()
        .expect("jpeg-decoder records frame info once decode() has returned Ok");

    let pixels: Vec<[u8; 3]> = match info.pixel_format {
        jpeg_decoder::PixelFormat::RGB24 => raw.as_chunks::<3>().0.to_vec(),
        jpeg_decoder::PixelFormat::L8 => raw.iter().map(|&g| [g, g, g]).collect(),
        jpeg_decoder::PixelFormat::L16 | jpeg_decoder::PixelFormat::CMYK32 => {
            return Err(malformed(
                "JPEG",
                "ART converts 8-bit grayscale and RGB JPEGs; this file uses a 16-bit-per-sample \
                 or CMYK pixel format that a wrong colour conversion could silently misread, so \
                 it is refused rather than guessed at",
            ));
        }
    };

    Ok(Rgb {
        width: info.width,
        height: info.height,
        pixels,
    })
}

/// Shrink `src` to fit within `max_w` x `max_h`, preserving aspect ratio, by
/// area-averaging (a box filter). Never enlarges: if `src` already fits, it
/// is returned unchanged. See the module doc for why a box filter.
pub fn scale_to_fit(src: &Rgb, max_w: u16, max_h: u16) -> Rgb {
    if src.width <= max_w && src.height <= max_h {
        return src.clone();
    }

    let scale_w = f64::from(max_w) / f64::from(src.width);
    let scale_h = f64::from(max_h) / f64::from(src.height);
    let scale = scale_w.min(scale_h);
    // Reached only when at least one of `src.width > max_w`,
    // `src.height > max_h` holds (the early return above covers the only
    // other case), so the factor for the dimension that exceeds its bound is
    // already < 1 and `min` can never pick something >= 1 here.
    debug_assert!(
        scale <= 1.0,
        "scale_to_fit must never compute an enlarging factor"
    );

    let new_w = ((f64::from(src.width) * scale).round() as u16).max(1);
    let new_h = ((f64::from(src.height) * scale).round() as u16).max(1);
    box_filter_scale(src, new_w, new_h)
}

fn box_filter_scale(src: &Rgb, new_w: u16, new_h: u16) -> Rgb {
    let src_w = u32::from(src.width);
    let src_h = u32::from(src.height);
    let new_w_u32 = u32::from(new_w);
    let new_h_u32 = u32::from(new_h);

    let mut pixels = Vec::with_capacity(new_w as usize * new_h as usize);
    for dy in 0..new_h_u32 {
        let y0 = dy * src_h / new_h_u32;
        let y1 = ((dy + 1) * src_h / new_h_u32).clamp(y0 + 1, src_h);
        for dx in 0..new_w_u32 {
            let x0 = dx * src_w / new_w_u32;
            let x1 = ((dx + 1) * src_w / new_w_u32).clamp(x0 + 1, src_w);

            let mut sum = [0u64; 3];
            let mut count = 0u64;
            for y in y0..y1 {
                for x in x0..x1 {
                    let p = src.pixels[(y * src_w + x) as usize];
                    sum[0] += u64::from(p[0]);
                    sum[1] += u64::from(p[1]);
                    sum[2] += u64::from(p[2]);
                    count += 1;
                }
            }
            // `x1 > x0` and `y1 > y0` are enforced by the `clamp` calls
            // above, so `count` is always at least 1 here.
            debug_assert!(
                count > 0,
                "a destination pixel's source box must not be empty"
            );
            pixels.push([
                (sum[0] / count) as u8,
                (sum[1] / count) as u8,
                (sum[2] / count) as u8,
            ]);
        }
    }

    Rgb {
        width: new_w,
        height: new_h,
        pixels,
    }
}

#[cfg(test)]
pub(crate) mod tests_support {
    use super::Rgb;

    /// Build a real PNG (via the `png` crate's own encoder) so the decode
    /// test below is an independent statement of what the bytes mean, not
    /// a copy of `decode_png`'s own assumptions.
    pub fn synthetic_png(width: u16, height: u16, colour: [u8; 3]) -> Vec<u8> {
        let mut bytes = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut bytes, width as u32, height as u32);
            encoder.set_color(png::ColorType::Rgb);
            encoder.set_depth(png::BitDepth::Eight);
            let mut writer = encoder.write_header().expect("write PNG header");
            let mut data = Vec::with_capacity(width as usize * height as usize * 3);
            for _ in 0..(width as usize * height as usize) {
                data.extend_from_slice(&colour);
            }
            writer
                .write_image_data(&data)
                .expect("write PNG image data");
        }
        bytes
    }

    /// A minimal `Rgb` of one solid colour, for tests that do not need a
    /// real decoded file — `quantise` and `scale_to_fit` take an `Rgb`
    /// directly and do not care how it was produced.
    pub fn solid(width: u16, height: u16, colour: [u8; 3]) -> Rgb {
        Rgb {
            width,
            height,
            pixels: vec![colour; width as usize * height as usize],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::tests_support::solid;
    use super::*;

    #[test]
    fn a_file_that_is_neither_png_nor_jpeg_is_refused_by_name() {
        let err = decode(b"GIF89a....").unwrap_err();
        let text = format!("{err}");
        assert!(
            text.contains("PNG"),
            "the refusal must name what ART accepts: {text}"
        );
        assert!(
            text.contains("JPEG"),
            "the refusal must name what ART accepts: {text}"
        );
    }

    #[test]
    fn an_oversized_source_is_refused_before_it_is_decoded() {
        let bytes = vec![0x89u8; MAX_SOURCE_BYTES + 1];
        let err = decode(&bytes).unwrap_err();
        let text = format!("{err}");
        // The bytes above do not carry a PNG or JPEG magic number (only the
        // first byte happens to match PNG's), so if the size cap were
        // dropped this would instead be refused by `decode`'s "neither PNG
        // nor JPEG" branch — a real `Err`, but for the wrong reason. Assert
        // the size limit itself appears in the message so that a dropped
        // cap is caught by an `Err` for the wrong reason, not silently
        // accepted as still-an-`Err`.
        assert!(
            text.contains(&MAX_SOURCE_BYTES.to_string()),
            "the refusal must name the size limit ART enforces before decoding, got: {text}"
        );
    }

    #[test]
    fn a_decoded_png_has_the_dimensions_the_png_declares() {
        let png = tests_support::synthetic_png(3, 2, [200, 100, 50]);
        let rgb = decode(&png).unwrap();
        assert_eq!((rgb.width, rgb.height), (3, 2));
        assert_eq!(rgb.pixels[0], [200, 100, 50]);
        assert_eq!(rgb.pixels.len(), 6);
    }

    #[test]
    fn scaling_keeps_the_aspect_ratio_and_never_enlarges() {
        let src = solid(800, 400, [0, 0, 0]);
        let out = scale_to_fit(&src, 640, 512);
        assert_eq!((out.width, out.height), (640, 320));

        let small = solid(100, 50, [0, 0, 0]);
        let same = scale_to_fit(&small, 640, 512);
        assert_eq!(
            (same.width, same.height),
            (100, 50),
            "a small picture is left alone"
        );
    }
}
