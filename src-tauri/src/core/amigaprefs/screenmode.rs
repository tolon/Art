//! The `SCRM` chunk inside `ScreenMode.prefs`: the Workbench screen's
//! display mode, dimensions and depth.
//!
//! Measured against a real AmigaOS 3.9 file rather than recalled (design doc
//! §1 — do not "correct" this from memory or from another project's
//! source): the body is 28 bytes — 16 reserved zero bytes, then `DisplayID`
//! (`u32`), `Width`, `Height`, `Depth` and `Control` (each `u16`), all
//! big-endian.
//!
//! **`Depth` is a whole `u16` at offset 24, not a byte at offset 25.** A
//! byte write at offset 25 happens to produce the same result *for values
//! that fit in one byte*, because it lands on that `u16`'s low half — that
//! is how Hatcher writes it, and it is wrong in general: it can never write
//! the high byte, and it never reads or preserves whatever the low byte
//! would otherwise carry from a value greater than 255. ART writes the
//! whole field. Do not "fix" this to match Hatcher's shape.

use crate::core::error::{CoreError, CoreResult};

const BODY_LEN: usize = 28;
const RESERVED_LEN: usize = 16;

/// `Width` or `Height` of `0xFFFF` means "use the mode's own default" rather
/// than naming a literal pixel count.
pub const USE_MODE_DEFAULT: u16 = 0xFFFF;

fn malformed(detail: impl Into<String>) -> CoreError {
    CoreError::Malformed {
        format: "ScreenMode SCRM".to_string(),
        detail: detail.into(),
    }
}

/// One `SCRM` chunk, fully decoded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScreenMode {
    pub display_id: u32,
    pub width: u16,
    pub height: u16,
    pub depth: u16,
    pub control: u16,
}

/// Parse one `SCRM` chunk body. Requires exactly 28 bytes — the chunk carries
/// no variable-length data, so a body of any other length is not a `SCRM`
/// this module understands.
pub fn read_screen_mode(body: &[u8]) -> CoreResult<ScreenMode> {
    if body.len() != BODY_LEN {
        return Err(malformed(format!(
            "SCRM body is {} bytes, expected {BODY_LEN}",
            body.len()
        )));
    }
    let display_id = u32::from_be_bytes([
        body[RESERVED_LEN],
        body[RESERVED_LEN + 1],
        body[RESERVED_LEN + 2],
        body[RESERVED_LEN + 3],
    ]);
    let width = u16::from_be_bytes([body[20], body[21]]);
    let height = u16::from_be_bytes([body[22], body[23]]);
    let depth = u16::from_be_bytes([body[24], body[25]]);
    let control = u16::from_be_bytes([body[26], body[27]]);

    Ok(ScreenMode {
        display_id,
        width,
        height,
        depth,
        control,
    })
}

/// Serialise a `SCRM` chunk body: 16 zero bytes, then the five fields,
/// big-endian.
pub fn write_screen_mode(m: &ScreenMode) -> CoreResult<Vec<u8>> {
    let mut out = Vec::with_capacity(BODY_LEN);
    out.extend_from_slice(&[0u8; RESERVED_LEN]);
    out.extend_from_slice(&m.display_id.to_be_bytes());
    out.extend_from_slice(&m.width.to_be_bytes());
    out.extend_from_slice(&m.height.to_be_bytes());
    out.extend_from_slice(&m.depth.to_be_bytes());
    out.extend_from_slice(&m.control.to_be_bytes());
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The `SCRM` body measured in the AmigaOS 3.9 tree: 28 bytes, reserved
    /// 16, DisplayID 0x00029000, Width and Height 0xFFFF, Depth 4, Control 1.
    fn measured_body() -> Vec<u8> {
        let mut body = vec![0u8; 16];
        body.extend_from_slice(&0x0002_9000u32.to_be_bytes());
        body.extend_from_slice(&0xFFFFu16.to_be_bytes());
        body.extend_from_slice(&0xFFFFu16.to_be_bytes());
        body.extend_from_slice(&4u16.to_be_bytes());
        body.extend_from_slice(&1u16.to_be_bytes());
        assert_eq!(body.len(), 28, "the measured chunk size is 28");
        body
    }

    #[test]
    fn the_measured_body_reads_as_the_release_shipped_it() {
        let m = read_screen_mode(&measured_body()).unwrap();
        assert_eq!(m.display_id, 0x0002_9000);
        assert_eq!(m.width, USE_MODE_DEFAULT);
        assert_eq!(m.height, USE_MODE_DEFAULT);
        assert_eq!(m.depth, 4, "sixteen colours");
        assert_eq!(m.control, 1);
    }

    #[test]
    fn it_round_trips_byte_for_byte() {
        let body = measured_body();
        assert_eq!(
            write_screen_mode(&read_screen_mode(&body).unwrap()).unwrap(),
            body
        );
    }

    #[test]
    fn the_depth_is_a_whole_u16_not_its_low_byte() {
        let mut m = read_screen_mode(&measured_body()).unwrap();
        m.depth = 0x0108;
        let out = write_screen_mode(&m).unwrap();
        assert_eq!(u16::from_be_bytes([out[24], out[25]]), 0x0108);
    }

    #[test]
    fn a_body_that_is_not_twenty_eight_bytes_is_refused() {
        assert!(read_screen_mode(&[0u8; 27]).is_err());
        assert!(read_screen_mode(&[0u8; 29]).is_err());
    }
}
