//! `Prefs/Env-Archive/<name>` variables: plain-text preference values, one
//! file per name, that AmigaDOS `SetEnv`/`GetEnv` reads and writes directly —
//! not an IFF container at all.
//!
//! A value is ISO-8859-1 text with bare LF line endings and no trailing
//! newline appended. Measured, not assumed: an Amiga writes its own text in
//! its own encoding, and a `\r\n` written by a host editor is not what the
//! release itself puts on disk. [`encode_value`]/[`decode_value`] share their
//! byte mapping with [`super::wbpattern`]'s picture-path encoding via
//! [`super::encode_latin1`]/[`super::decode_latin1`] — same encoding, two
//! call sites, one implementation.
//!
//! [`SHELL_DEFAULTS`] is Hatcher's list of five measured shell defaults,
//! **minus its sixth** (`Workbench` = `Workbench:`). That one is omitted on
//! purpose: a release sets its own `Workbench` value, and ART does not
//! second-guess what the release already wrote.

use crate::core::error::{CoreError, CoreResult};

fn malformed(detail: impl Into<String>) -> CoreError {
    CoreError::Malformed {
        format: "Env-Archive value".to_string(),
        detail: detail.into(),
    }
}

/// The five measured shell defaults ART writes into `Prefs/Env-Archive/`.
/// See the module doc for why `Workbench` is not the sixth entry here.
pub const SHELL_DEFAULTS: [(&str, &str); 5] = [
    ("Sys/def_shell", "CON:0/50//150/Shell/CLOSE"),
    ("Sys/def_editor", "C:Ed"),
    ("Sys/def_cli", "NewShell"),
    ("Sys/def_width", "640"),
    ("Sys/def_height", "256"),
];

/// Encode a value as ISO-8859-1 bytes with bare LF line endings. `\r\n` is
/// normalised to `\n` first; no newline is appended to a value that has
/// none. A character with no ISO-8859-1 byte is refused, not silently
/// replaced — writing a wrong byte to the Amiga's own file and saying
/// nothing is this project's most expensive failure class.
pub fn encode_value(value: &str) -> CoreResult<Vec<u8>> {
    let normalized = value.replace("\r\n", "\n");
    super::encode_latin1(&normalized)
        .map_err(|c| malformed(format!("'{c}' has no ISO-8859-1 byte")))
}

/// Decode ISO-8859-1 bytes back to a `String`. Never fails — every byte
/// 0..=255 is a valid ISO-8859-1 character.
pub fn decode_value(bytes: &[u8]) -> String {
    super::decode_latin1(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_value_is_iso_8859_1_not_utf_8() {
        // U+00FC LATIN SMALL LETTER U WITH DIAERESIS is one byte in
        // ISO-8859-1 and two in UTF-8. An Amiga reads the file as bytes.
        assert_eq!(encode_value("\u{00FC}").unwrap(), vec![0xFC]);
    }

    #[test]
    fn a_character_outside_iso_8859_1_is_refused_not_mangled() {
        // U+011F LATIN SMALL LETTER G WITH BREVE has no ISO-8859-1 byte.
        // Silently writing "?" would put a wrong value on an Amiga and say
        // nothing, which is this project's most expensive failure class.
        let err = encode_value("\u{011F}").unwrap_err();
        assert!(
            format!("{err}").contains("ISO-8859-1"),
            "the refusal must say why, got: {err}"
        );
    }

    #[test]
    fn line_endings_are_bare_lf() {
        assert_eq!(encode_value("a\r\nb").unwrap(), b"a\nb".to_vec());
        assert_eq!(encode_value("a\nb").unwrap(), b"a\nb".to_vec());
    }

    #[test]
    fn no_newline_is_appended_to_a_single_line_value() {
        assert_eq!(encode_value("Workbench:").unwrap(), b"Workbench:".to_vec());
    }

    #[test]
    fn the_shell_defaults_are_the_five_measured_names() {
        let names: Vec<&str> = SHELL_DEFAULTS.iter().map(|(n, _)| *n).collect();
        assert_eq!(
            names,
            vec![
                "Sys/def_shell",
                "Sys/def_editor",
                "Sys/def_cli",
                "Sys/def_width",
                "Sys/def_height"
            ]
        );
    }

    #[test]
    fn a_value_round_trips() {
        for v in ["Workbench:", "C:Ed", "640", "CON:0/50//150/Shell/CLOSE"] {
            assert_eq!(decode_value(&encode_value(v).unwrap()), v);
        }
    }
}
