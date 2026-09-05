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
