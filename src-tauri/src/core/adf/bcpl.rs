//! BCPL string + Amiga date helpers.
//!
//! These are the lowest-level primitives shared across the ADF engine.

/// Amiga epoch: 1978-01-01 00:00:00 UTC, in Unix seconds.
pub const AMIGA_EPOCH_UNIX: i64 = 252_460_800;

/// Ticks per second in the Amiga date (50 Hz PAL / 60 Hz NTSC; we use PAL).
pub const TICKS_PER_SEC: i64 = 50;

/// Read a BCPL string from `buf` starting at `offset`.
///
/// BCPL format: `[1 byte length N][N bytes chars]` — no NUL terminator.
/// Returns the decoded UTF-8 string (lossy: Amiga filenames are latin-1-ish).
pub fn read_bcpl_string(buf: &[u8], offset: usize) -> Option<String> {
    let len = *buf.get(offset)? as usize;
    // Cap at remaining buffer to stay safe against malformed images.
    let available = buf.len().saturating_sub(offset + 1);
    let len = len.min(available);
    let bytes = &buf[offset + 1..offset + 1 + len];
    Some(String::from_utf8_lossy(bytes).into_owned())
}

/// Write a BCPL string into `buf` at `offset`, returning bytes written.
/// Pads the rest of the field (up to `field_len`) with zeros.
pub fn write_bcpl_string(buf: &mut [u8], offset: usize, s: &str, field_len: usize) {
    let bytes = s.as_bytes();
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
    fn bcpl_string_lossy_on_non_utf8() {
        let buf = [3, 0xFF, 0xFE, 0xFD];
        let s = read_bcpl_string(&buf, 0).unwrap();
        // Each invalid byte becomes U+FFFD (3 UTF-8 bytes), so the String's
        // byte length is 9, not 3. It should still contain 3 replacement chars.
        assert_eq!(s.chars().count(), 3);
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
