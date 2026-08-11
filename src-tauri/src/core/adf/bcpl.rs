//! BCPL string + Amiga date helpers.
//!
//! These are the lowest-level primitives shared across the ADF engine.

/// Amiga epoch: 1978-01-01 00:00:00 UTC, in Unix seconds.
pub const AMIGA_EPOCH_UNIX: i64 = 252_460_800;

/// Ticks per second in the Amiga date (50 Hz PAL / 60 Hz NTSC; we use PAL).
pub const TICKS_PER_SEC: i64 = 50;

// AmigaDOS stores strings as Latin-1 — **one byte per character**. Rust
// strings are UTF-8, where anything above `~` takes two bytes. The two are not
// interchangeable, and treating them as if they were is what ART-074 was:
// `dir.rs::put_name` wrote a name correctly as Latin-1 and `read_bcpl_string`
// read it back with `from_utf8_lossy`, so `Grüße` — bytes `47 72 FC DF 65` —
// came back as `Gr??e`. `FC` and `DF` are not valid UTF-8 lead bytes.
//
// Latin-1 is exactly the first 256 Unicode code points, so both directions are
// a plain cast. There is no lookup table and no lossiness within range.

/// Read a BCPL string from `buf` starting at `offset`.
///
/// BCPL format: `[1 byte length N][N bytes chars]` — no NUL terminator.
/// Decoded as Latin-1, which is what AmigaDOS wrote.
pub fn read_bcpl_string(buf: &[u8], offset: usize) -> Option<String> {
    let len = *buf.get(offset)? as usize;
    // Cap at remaining buffer to stay safe against malformed images.
    let available = buf.len().saturating_sub(offset + 1);
    let len = len.min(available);
    let bytes = &buf[offset + 1..offset + 1 + len];
    // Latin-1 → Unicode is the identity on code points 0..=255.
    Some(bytes.iter().map(|&b| b as char).collect())
}

/// Write a BCPL string into `buf` at `offset`, encoded as Latin-1.
///
/// Pads the rest of the field (up to `field_len`) with zeros.
///
/// A character above `U+00FF` has no Latin-1 byte and becomes `?`. Callers on
/// the file-name path have already been through `check_name`, which refuses
/// those outright; this only bites a volume or drive name, where a visible `?`
/// is a better answer than a byte pair an Amiga would render as two characters
/// of mojibake.
pub fn write_bcpl_string(buf: &mut [u8], offset: usize, s: &str, field_len: usize) {
    // Encode first, then truncate: truncating UTF-8 by byte count could split
    // a character in half, which is how a name loses its last letter.
    let bytes: Vec<u8> = s
        .chars()
        .map(|c| if (c as u32) <= 0xFF { c as u8 } else { b'?' })
        .collect();
    let len = bytes.len().min(field_len - 1);
    // Clear the field first.
    for i in 0..field_len {
        buf[offset + i] = 0;
    }
    buf[offset] = len as u8;
    buf[offset + 1..offset + 1 + len].copy_from_slice(&bytes[..len]);
}

/// An Amiga timestamp (days + mins + ticks since/before midnight).
///
/// `Default` is the Amiga epoch, 1978-01-01 00:00:00 — the same value
/// [`AmigaDate::zero`] returns, and the right answer for a file whose date
/// nothing is known about.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct AmigaDate {
    /// Days since 1978-01-01.
    pub days: u32,
    /// Minutes past midnight (0..1439).
    pub mins: u32,
    /// Ticks past the minute (1/50 s; 0..2999).
    pub ticks: u32,
}

impl AmigaDate {
    pub fn to_unix(self) -> i64 {
        AMIGA_EPOCH_UNIX
            + (self.days as i64) * 86_400
            + (self.mins as i64) * 60
            + (self.ticks as i64) / TICKS_PER_SEC
    }

    /// A zero/empty date (used when a file has no date set).
    pub fn zero() -> Self {
        Self {
            days: 0,
            mins: 0,
            ticks: 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bcpl_string_round_trip() {
        let mut buf = vec![0u8; 40];
        write_bcpl_string(&mut buf, 0, "Workbench", 40);
        assert_eq!(buf[0], 9); // length byte
        assert_eq!(read_bcpl_string(&buf, 0), Some("Workbench".to_string()));
    }

    #[test]
    fn bcpl_string_truncates_to_field() {
        let mut buf = vec![0u8; 6]; // field_len 6 → max 5 chars
        write_bcpl_string(&mut buf, 0, "HelloWorld", 6);
        assert_eq!(buf[0], 5);
        assert_eq!(read_bcpl_string(&buf, 0), Some("Hello".to_string()));
    }

    #[test]
    fn bcpl_string_empty() {
        let mut buf = vec![0u8; 10];
        write_bcpl_string(&mut buf, 0, "", 10);
        assert_eq!(buf[0], 0);
        assert_eq!(read_bcpl_string(&buf, 0), Some(String::new()));
    }

    #[test]
    fn every_byte_is_a_latin1_character_not_an_encoding_error() {
        let buf = [3, 0xFF, 0xFE, 0xFD];
        let s = read_bcpl_string(&buf, 0).unwrap();
        // These are legal Latin-1, so they are `ÿþý` — not three replacement
        // characters, which is what this asserted while the reader used
        // `from_utf8_lossy` (ART-074). The old assertion counted characters
        // only, so it passed either way and said nothing.
        assert_eq!(s, "ÿþý");
    }

    #[test]
    fn amiga_date_to_unix() {
        // 1978-01-01 00:00:00 → exactly the Amiga epoch.
        let d = AmigaDate::zero();
        assert_eq!(d.to_unix(), AMIGA_EPOCH_UNIX);

        // One day later.
        let d = AmigaDate {
            days: 1,
            mins: 0,
            ticks: 0,
        };
        assert_eq!(d.to_unix(), AMIGA_EPOCH_UNIX + 86_400);

        // 1978-01-02 00:01:00 → +1 day +1 min.
        let d = AmigaDate {
            days: 1,
            mins: 1,
            ticks: 0,
        };
        assert_eq!(d.to_unix(), AMIGA_EPOCH_UNIX + 86_400 + 60);
    }

    #[test]
    fn bcpl_read_out_of_bounds_safe() {
        let buf = [5u8, b'a']; // claims 5 chars but only 1 available
        let s = read_bcpl_string(&buf, 0).unwrap();
        assert_eq!(s, "a"); // clamped to available
    }
}

#[cfg(test)]
mod bcpl_string_tests {
    use super::*;

    /// ART-074. `dir.rs::put_name` wrote Latin-1 while `read_bcpl_string` read
    /// UTF-8, so an accented name came back with replacement characters. Every
    /// test in the suite used ASCII names, so nothing caught it — the same
    /// shape as ART-032..035, except here the writer and the reader were in
    /// the same module.
    #[test]
    fn an_accented_name_survives_a_round_trip() {
        let mut buf = [0u8; 64];
        write_bcpl_string(&mut buf, 0, "Grüße", 32);
        // Latin-1: one byte per character, not two for the accented ones.
        assert_eq!(buf[0], 5, "length is characters, not UTF-8 bytes");
        assert_eq!(&buf[1..6], &[0x47, 0x72, 0xFC, 0xDF, 0x65]);
        assert_eq!(read_bcpl_string(&buf, 0).unwrap(), "Grüße");
    }

    #[test]
    fn a_name_written_as_latin1_elsewhere_reads_back_intact() {
        // What `core/volume/write/dir.rs::put_name` lays down directly.
        let mut buf = [0u8; 64];
        buf[0] = 5;
        buf[1..6].copy_from_slice(&[0x47, 0x72, 0xFC, 0xDF, 0x65]);
        assert_eq!(read_bcpl_string(&buf, 0).unwrap(), "Grüße");
    }

    #[test]
    fn ascii_is_unchanged() {
        let mut buf = [0u8; 64];
        write_bcpl_string(&mut buf, 0, "Startup-Sequence", 32);
        assert_eq!(read_bcpl_string(&buf, 0).unwrap(), "Startup-Sequence");
    }

    #[test]
    fn a_character_with_no_latin1_byte_becomes_a_question_mark() {
        let mut buf = [0u8; 64];
        write_bcpl_string(&mut buf, 0, "Ω", 32);
        assert_eq!(read_bcpl_string(&buf, 0).unwrap(), "?");
    }

    #[test]
    fn truncation_counts_characters_and_never_splits_one() {
        let mut buf = [0u8; 16];
        // Field of 8 holds 7 characters. In UTF-8 these would be 12 bytes and
        // the old code would have cut one in half.
        write_bcpl_string(&mut buf, 0, "üüüüüüüü", 8);
        let out = read_bcpl_string(&buf, 0).unwrap();
        assert_eq!(out.chars().count(), 7);
        assert!(
            out.chars().all(|c| c == 'ü'),
            "a character was split: {out:?}"
        );
    }

    #[test]
    fn a_length_past_the_buffer_is_clamped_not_a_panic() {
        let buf = [200u8, b'a', b'b'];
        assert_eq!(read_bcpl_string(&buf, 0).unwrap(), "ab");
    }
}
