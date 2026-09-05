//! Median-cut colour quantisation: turn a decoded [`super::Rgb`] into the
//! `core::ilbm::Indexed` shape `core::ilbm::encode` already accepts.
//!
//! # The algorithm, and why each rule is there
//!
//! Every pixel starts in one box. Repeatedly: find the box that still holds
//! more than one pixel whose own widest channel range is the largest across
//! every such box, split it at that channel's median, and keep going until
//! there are `colours` boxes or no box can split any further. Average each
//! box's pixels for its palette entry, then map every original pixel to its
//! box.
//!
//! **"No box can split" means a box holds exactly one pixel** — not "every
//! channel range is zero". A box whose pixels are all identical still has
//! more than one pixel to hand out, and splitting it is well-defined even
//! though the "median" is degenerate: sorting by a channel where every value
//! is equal is a stable sort, so the split still divides the box's pixels
//! (by their original position) into two non-empty halves. This matters for
//! a solid-colour source asked for a high colour count: it keeps splitting
//! down to individual pixels rather than stopping at one box, which is
//! exactly the case the deduplication step below exists to clean back up.
//!
//! **Deduplicate identical palette entries after averaging.** A solid-colour
//! image split all the way to single-pixel boxes (previous paragraph)
//! produces many boxes that all average to the same colour; without
//! deduplication a caller asking for 256 colours on a flat-coloured backdrop
//! would get a palette with the same triple repeated many times, which is
//! correct in the sense that the pixels still decode right, but is not what
//! "quantise to N colours" should mean when the image only has one.
//!
//! **`colours` is clamped to `1..=256`, not passed through unchanged.**
//! `Indexed::pixels` is one byte per pixel indexing `Indexed::palette`, so a
//! palette over 256 entries cannot be represented at all — the same limit
//! `core::ilbm::encode` refuses past. Clamping here means this module can
//! never itself build a palette `encode` would go on to refuse, rather than
//! reproducing that check a second time only to fail later at a less
//! informative point.

use std::collections::HashMap;

use super::Rgb;
use crate::core::ilbm::Indexed;

/// Quantise `src` to at most `colours` palette entries (see the module doc
/// for the exact algorithm and why `colours` is clamped rather than trusted).
pub fn quantise(src: &Rgb, colours: usize) -> Indexed {
    let target = colours.clamp(1, 256);
    let pixel_count = src.pixels.len();

    // Boxes hold indices into `src.pixels`, never copies of the colour
    // data, so the final pass below can still say which original pixel
    // position ended up in which box.
    let mut boxes: Vec<Vec<usize>> = if pixel_count == 0 {
        Vec::new()
    } else {
        vec![(0..pixel_count).collect()]
    };

    while boxes.len() < target {
        let mut widest: Option<(usize, usize, u8)> = None; // (box index, channel, range)
        for (i, indices) in boxes.iter().enumerate() {
            if indices.len() < 2 {
                continue; // a single-pixel box cannot split
            }
            let (channel, range) = widest_channel(src, indices);
            let is_wider = match widest {
                None => true,
                Some((_, _, best_range)) => range > best_range,
            };
            if is_wider {
                widest = Some((i, channel, range));
            }
        }

        let Some((index, channel, _)) = widest else {
            break; // every remaining box is a single pixel
        };
        let box_to_split = boxes.remove(index);
        let (left, right) = split_box(src, box_to_split, channel);
        boxes.push(left);
        boxes.push(right);
    }

    let mut palette: Vec<[u8; 3]> = Vec::new();
    let mut palette_index_of: HashMap<[u8; 3], usize> = HashMap::new();
    let mut pixels = vec![0u8; pixel_count];

    for indices in &boxes {
        let avg = average(src, indices);
        let palette_index = *palette_index_of.entry(avg).or_insert_with(|| {
            palette.push(avg);
            palette.len() - 1
        });
        // `target` is clamped to at most 256 above, `boxes.len()` never
        // exceeds `target`, and deduplication only ever shrinks `palette`
        // further, so `palette.len()` never passes 256 and this cast never
        // truncates.
        debug_assert!(
            palette.len() <= 256,
            "quantise must never build more than 256 entries"
        );
        for &idx in indices {
            pixels[idx] = palette_index as u8;
        }
    }

    Indexed {
        width: src.width,
        height: src.height,
        palette,
        pixels,
    }
}

/// The channel (0=R, 1=G, 2=B) with the largest `max - min` across `indices`,
/// and that range.
fn widest_channel(src: &Rgb, indices: &[usize]) -> (usize, u8) {
    let mut min = [u8::MAX; 3];
    let mut max = [u8::MIN; 3];
    for &idx in indices {
        let p = src.pixels[idx];
        for c in 0..3 {
            min[c] = min[c].min(p[c]);
            max[c] = max[c].max(p[c]);
        }
    }
    let ranges = [max[0] - min[0], max[1] - min[1], max[2] - min[2]];
    let mut best_channel = 0;
    for (c, &range) in ranges.iter().enumerate().skip(1) {
        if range > ranges[best_channel] {
            best_channel = c;
        }
    }
    (best_channel, ranges[best_channel])
}

/// Split `indices` into two non-empty halves by sorting on `channel` and
/// cutting at the middle. The caller only ever passes a box of at least two
/// pixels (see `quantise`'s `indices.len() < 2` guard above), so both halves
/// are guaranteed non-empty: `mid = len / 2` satisfies `1 <= mid < len`
/// whenever `len >= 2`.
fn split_box(src: &Rgb, mut indices: Vec<usize>, channel: usize) -> (Vec<usize>, Vec<usize>) {
    indices.sort_by_key(|&idx| src.pixels[idx][channel]);
    let mid = indices.len() / 2;
    let right = indices.split_off(mid);
    (indices, right)
}

/// The rounded-to-nearest average colour of the pixels at `indices`.
/// `indices` is never empty here: `quantise` only ever calls this on a box
/// it actually built, and every box holds at least the one pixel it started
/// with.
fn average(src: &Rgb, indices: &[usize]) -> [u8; 3] {
    let mut sum = [0u64; 3];
    for &idx in indices {
        let p = src.pixels[idx];
        sum[0] += u64::from(p[0]);
        sum[1] += u64::from(p[1]);
        sum[2] += u64::from(p[2]);
    }
    let n = indices.len() as u64;
    [
        ((sum[0] + n / 2) / n) as u8,
        ((sum[1] + n / 2) / n) as u8,
        ((sum[2] + n / 2) / n) as u8,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::picture::tests_support::solid;

    #[test]
    fn a_single_colour_image_quantises_to_one_entry() {
        let out = quantise(&solid(4, 4, [10, 20, 30]), 256);
        assert_eq!(out.palette, vec![[10, 20, 30]]);
        assert_eq!(out.pixels, vec![0u8; 16]);
    }

    #[test]
    fn quantising_never_returns_more_colours_than_asked_for() {
        let mut src = solid(16, 16, [0, 0, 0]);
        for (i, p) in src.pixels.iter_mut().enumerate() {
            *p = [(i * 7) as u8, (i * 13) as u8, (i * 3) as u8];
        }
        for want in [2usize, 4, 16, 256] {
            let out = quantise(&src, want);
            assert!(
                out.palette.len() <= want,
                "asked {want}, got {}",
                out.palette.len()
            );
            assert!(!out.palette.is_empty());
            assert!(out.pixels.iter().all(|&i| (i as usize) < out.palette.len()));
        }
    }

    #[test]
    fn quantising_preserves_the_pixel_count_and_the_dimensions() {
        let src = solid(7, 5, [1, 2, 3]);
        let out = quantise(&src, 16);
        assert_eq!((out.width, out.height), (7, 5));
        assert_eq!(out.pixels.len(), 35);
    }
}
