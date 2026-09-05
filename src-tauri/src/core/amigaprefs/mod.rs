//! AmigaOS preferences files — the IFF `FORM PREF` container and the chunks
//! ART writes into it.
//!
//! Every byte offset in this module tree was measured from a real file on
//! the owner's own material; see
//! `docs/superpowers/specs/2026-09-05-prefs-and-wallpaper-design.md` §1 for
//! the dumps and the commands that reproduce them. Do not "correct" one of
//! them from memory.

pub mod env;
pub mod iff;
pub mod screenmode;
pub mod wbpattern;

/// Decode ISO-8859-1 bytes to a `String`. Every byte 0..=255 maps directly
/// to the Unicode code point of the same value, so this never fails — an
/// Amiga string is text in the Amiga's own encoding, not UTF-8. Shared by
/// [`wbpattern`] (a backdrop's picture path) and [`env`] (an Env-Archive
/// value) rather than duplicated in each.
pub(crate) fn decode_latin1(bytes: &[u8]) -> String {
    bytes.iter().map(|&b| char::from(b)).collect()
}

/// Encode a `String` back to ISO-8859-1 bytes. Returns the first character
/// that has no ISO-8859-1 byte rather than lossily substituting one — the
/// caller turns that into a `CoreError` with wording for its own chunk.
pub(crate) fn encode_latin1(s: &str) -> Result<Vec<u8>, char> {
    s.chars()
        .map(|c| u8::try_from(c as u32).map_err(|_| c))
        .collect()
}
