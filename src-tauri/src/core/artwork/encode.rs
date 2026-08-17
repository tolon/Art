//! Percent-encoding for one path segment.
//!
//! `core::sources::mirror::validate_fetch_path` rejects spaces, `:`, `?`, `#`,
//! `\` and `//`, because any of them could re-point a request somewhere the
//! caller did not intend. Artwork filenames legitimately contain spaces —
//! `1000 Miglia - 1927-1933 Volume 1.png` — so the *segment* is encoded here
//! rather than the validator being weakened to admit it.

use std::fmt::Write as _;

/// Encode one path segment, leaving only characters that are unreserved in
/// RFC 3986 plus the sub-delims a filename realistically uses.
///
/// `/` is **not** exempt: this encodes a single segment, and a caller joining
/// segments does so itself.
pub fn path_segment(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    for byte in raw.bytes() {
        let keep = byte.is_ascii_alphanumeric()
            || matches!(
                byte,
                b'-' | b'_' | b'.' | b'~' | b'\'' | b'(' | b')' | b'!' | b'*'
            );
        if keep {
            out.push(byte as char);
        } else {
            // `write!` into a String cannot fail; the result is discarded
            // deliberately rather than unwrapped.
            let _ = write!(out, "%{byte:02X}");
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_space_becomes_percent_twenty() {
        assert_eq!(path_segment("1000 Miglia"), "1000%20Miglia");
    }

    /// The real libretro corpus contains apostrophes; they are legal in a URL
    /// path and must survive untouched, or the constructed path 404s.
    #[test]
    fn an_apostrophe_survives() {
        assert_eq!(path_segment("'Allo 'Allo"), "'Allo%20'Allo");
    }

    /// A separator inside a segment would create a directory level that the
    /// caller did not ask for.
    #[test]
    fn a_slash_is_encoded_not_passed_through() {
        assert_eq!(path_segment("a/b"), "a%2Fb");
    }

    /// Everything the validator rejects must be gone by the time it is asked.
    #[test]
    fn the_characters_the_validator_rejects_are_all_encoded() {
        let encoded = path_segment("a b:c?d#e\\f");
        for bad in [' ', ':', '?', '#', '\\'] {
            assert!(!encoded.contains(bad), "{bad:?} survived in {encoded}");
        }
    }

    #[test]
    fn a_percent_is_itself_encoded_so_encoding_is_not_ambiguous() {
        assert_eq!(path_segment("100%"), "100%25");
    }
}
