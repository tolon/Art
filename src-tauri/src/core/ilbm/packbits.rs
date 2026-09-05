//! ByteRun1, the compression an ILBM `BMHD` selects with `compression = 1`.
//!
//! A control byte `n` in `0..=127` means "the next `n + 1` bytes are
//! literal"; `n` in `129..=255` means "repeat the next byte `257 - n`
//! times"; `128` is a no-operation that this encoder never emits, because
//! some historical decoders treat it as end-of-data.
//!
//! ILBM compresses **each plane row independently** — a run never crosses a
//! row boundary, which is why this takes a row rather than the whole body.

use crate::core::error::{CoreError, CoreResult};

fn malformed(detail: &str) -> CoreError {
    CoreError::Malformed {
        format: "ILBM ByteRun1".to_string(),
        detail: detail.to_string(),
    }
}

/// The length, capped at 128, of the run of identical bytes starting at
/// `row[at]`. Never looks past `row`'s own bounds.
fn run_length_at(row: &[u8], at: usize) -> usize {
    let value = row[at];
    let mut len = 1usize;
    while at + len < row.len() && row[at + len] == value && len < 128 {
        len += 1;
    }
    len
}

/// Pack one plane row with ByteRun1.
///
/// Two-state scan: a run of 3 or more identical bytes is emitted as a
/// replicate (capped at 128 bytes per control byte, so a longer run is split
/// across several); everything else accumulates into a literal buffer that is
/// flushed at 128 bytes or as soon as a qualifying run begins.
pub fn pack_row(row: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut i = 0usize;
    let n = row.len();

    while i < n {
        let run_len = run_length_at(row, i);
        if run_len >= 3 {
            // control = 257 - run_len, always in 129..=254 — never 0x80.
            out.push((257 - run_len) as u8);
            out.push(row[i]);
            i += run_len;
            continue;
        }

        // Accumulate a literal run until a qualifying replicate run begins,
        // the row ends, or the 128-byte literal cap is reached.
        let lit_start = i;
        let mut lit_len = 0usize;
        while i < n && lit_len < 128 {
            if run_length_at(row, i) >= 3 {
                break;
            }
            i += 1;
            lit_len += 1;
        }
        // control = lit_len - 1, always in 0..=127 — never 0x80.
        out.push((lit_len - 1) as u8);
        out.extend_from_slice(&row[lit_start..lit_start + lit_len]);
    }

    out
}

/// The inverse of [`pack_row`]. Used by this module's own round-trip tests
/// and, per Task 5's brief, by Task 11's oracle script's Rust half — kept
/// `pub` rather than test-only for that reason.
///
/// `expect` is the caller's own bound on how many bytes the unpacked row
/// should hold; a mismatch is refused rather than silently truncated or
/// padded, since a wrong length here means the packer or its input was wrong.
pub fn unpack_row(packed: &[u8], expect: usize) -> CoreResult<Vec<u8>> {
    let mut out = Vec::with_capacity(expect.min(packed.len().saturating_mul(128)));
    let mut i = 0usize;

    while i < packed.len() {
        let control = packed[i] as i8;
        i += 1;

        if control >= 0 {
            let len = control as usize + 1;
            let end = i
                .checked_add(len)
                .ok_or_else(|| malformed("packbits literal run overflows"))?;
            let slice = packed
                .get(i..end)
                .ok_or_else(|| malformed("packbits literal run runs past the end of the row"))?;
            out.extend_from_slice(slice);
            i = end;
        } else if control != -128 {
            let len = (1 - control as i32) as usize;
            let value = *packed
                .get(i)
                .ok_or_else(|| malformed("packbits replicate run is missing its byte"))?;
            i += 1;
            let new_len = out
                .len()
                .checked_add(len)
                .ok_or_else(|| malformed("packbits replicate run overflows the output"))?;
            out.resize(new_len, value);
        }
        // control == -128 (control byte 0x80) is the documented no-op.
    }

    if out.len() != expect {
        return Err(malformed(&format!(
            "unpacked row is {} byte(s), expected {expect}",
            out.len()
        )));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_run_becomes_one_control_byte_and_one_value() {
        // 10 identical bytes: control = 257 - 10 = 247 = 0xF7, then the byte.
        assert_eq!(pack_row(&[0xAA; 10]), vec![0xF7, 0xAA]);
    }

    #[test]
    fn a_literal_run_is_prefixed_with_its_length_minus_one() {
        assert_eq!(pack_row(&[1, 2, 3]), vec![2, 1, 2, 3]);
    }

    #[test]
    fn a_run_longer_than_128_is_split() {
        let packed = pack_row(&[7u8; 200]);
        assert_eq!(unpack_row(&packed, 200).unwrap(), vec![7u8; 200]);
        assert!(packed.len() < 200, "compression must actually compress");
    }

    #[test]
    fn the_control_byte_0x80_is_never_emitted() {
        // 0x80 means "no operation" and some decoders treat it as a stop.
        for len in 1..300usize {
            let row: Vec<u8> = (0..len).map(|i| (i % 7) as u8).collect();
            assert!(!pack_row(&row).contains(&0x80), "len {len}");
        }
    }

    #[test]
    fn every_row_shape_round_trips() {
        let cases: Vec<Vec<u8>> = vec![
            vec![],
            vec![1],
            vec![5, 5],
            vec![1, 1, 1, 2, 3, 4, 4, 4, 4, 4],
            (0..255u8).collect(),
            vec![0u8; 1000],
        ];
        for row in cases {
            let packed = pack_row(&row);
            assert_eq!(unpack_row(&packed, row.len()).unwrap(), row);
        }
    }
}
