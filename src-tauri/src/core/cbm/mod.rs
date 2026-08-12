//! Commodore 8-bit disk and tape images.
//!
//! A C64 disk image is the same thing to ART's commander that an ADF, a CD and
//! an archive are — a container you walk into, list, and copy out of — which
//! is why it fits behind the model that already exists rather than becoming a
//! second application (spec addendum §10.5).
//!
//! Read-only, all of it. ART reads these; it does not write them.
//!
//! | Module | What it reads |
//! |---|---|
//! | [`geometry`] | where a sector is, per drive — the table everything else stands on |
//! | [`petscii`] | names, which are PETSCII and padded with `0xA0` |
//! | [`d64`] | D64/D71/D81: the header, the directory and a file's sector chain |
//! | [`t64`] | T64 tape archives, whose own headers are not to be trusted |
//!
//! **TAP, PRG and CRT are identified and never browsed**, and that is a
//! property of the formats rather than a gap in the schedule: a TAP holds the
//! tape signal sampled as pulse widths, with no directory and no file table in
//! it at all. Listing one means demodulating the ROM tape format, and most
//! commercial titles shipped their own turbo loader (§10, §89).

pub mod d64;
pub mod geometry;
pub mod petscii;
pub mod t64;

use std::path::Path;

use crate::core::error::CoreResult;

/// Which of these ART can open as a pane, and which it can only name.
///
/// Keyed on `core::detect`'s `format_hint`, so the answer the drop panel gives
/// and the answer the file manager gives cannot drift apart.
pub fn is_browsable(format_hint: &str) -> bool {
    matches!(format_hint, "d64" | "d71" | "d81" | "t64")
}

/// Say what a Commodore file is, for the formats ART deliberately does not
/// browse.
///
/// This is what "identify only" means in practice: not a shrug, an answer.
/// A TAP holds no directory and no file table — it is the tape signal sampled
/// as pulse widths, and listing it would mean demodulating the ROM tape
/// format that most commercial titles replaced with their own loader. ART
/// says what the file is, how big it is, and what it would take to do more.
pub fn identify(path: &Path, format_hint: &str) -> CoreResult<String> {
    let size = std::fs::metadata(path)?.len();

    Ok(match format_hint {
        "tap" => format!(
            "Commodore tape dump (TAP), {size} bytes. A TAP holds the tape signal sampled as \
             pulse widths — there is no directory and no file table inside it, so ART identifies \
             it and stops there rather than pretending to list it."
        ),
        "prg" => {
            // The first two bytes are the address the program loads at, which
            // is the one thing a PRG says about itself.
            let head = crate::core::detect::read_head(path, 2)?;
            let load = if head.len() == 2 {
                format!("${:04X}", u16::from_le_bytes([head[0], head[1]]))
            } else {
                "unknown".to_string()
            };
            format!(
                "Commodore program (PRG), {size} bytes, loading at {load}. A PRG is one program \
                 and no container — there is nothing inside it to browse."
            )
        }
        "crt" => format!(
            "Commodore cartridge image (CRT), {size} bytes. ART identifies cartridges; it does \
             not read their banks."
        ),
        other => format!("Commodore 8-bit file ({other}), {size} bytes."),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_browsable_formats_are_the_ones_with_a_directory() {
        for hint in ["d64", "d71", "d81", "t64"] {
            assert!(is_browsable(hint), "{hint}");
        }
        for hint in ["tap", "prg", "crt", "adf", ""] {
            assert!(!is_browsable(hint), "{hint}");
        }
    }

    /// "Identify only" has to actually say something. A sentence that names
    /// the format, its size and why there is nothing to open is an answer;
    /// "unsupported" is not.
    #[test]
    fn identify_names_the_format_and_says_why_it_is_not_browsable() {
        let dir = std::env::temp_dir().join(format!("art-cbm-id-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();

        let tap = dir.join("game.tap");
        std::fs::write(&tap, b"C64-TAPE-RAW\x00\x00\x00\x00").unwrap();
        let said = identify(&tap, "tap").unwrap();
        assert!(said.contains("16 bytes"), "{said}");
        assert!(said.contains("no directory"), "{said}");

        // A PRG's load address is the one thing it says about itself.
        let prg = dir.join("game.prg");
        std::fs::write(&prg, [0x01, 0x08, 0x00, 0x00]).unwrap();
        let said = identify(&prg, "prg").unwrap();
        assert!(said.contains("$0801"), "{said}");

        let crt = dir.join("game.crt");
        std::fs::write(&crt, b"C64 CARTRIDGE   ").unwrap();
        assert!(identify(&crt, "crt").unwrap().contains("cartridge"));

        std::fs::remove_dir_all(&dir).ok();
    }
}
