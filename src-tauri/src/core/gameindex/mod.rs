//! What a game *is*, said by something that knows (SD-2 · G10).
//!
//! Four sources answer that question, and they are not equal. An `.rp9`
//! manifest and a WHDLoad slave's own header **state** a title; a filename and
//! a drawer name **suggest** one. `Lotus3HD` and `Moonstone Install` are drawer
//! names for games actually called `Lotus 3` and `Moonstone`, which is why the
//! distinction lives in the type rather than in a comment.
//!
//! `store` is where a catalogue is kept between runs, so the 3.74 GB the
//! readers walk is walked once rather than on every visit.
//!
//! Callers name types by their full path (`core::gameindex::record::Fact`)
//! rather than through a re-export here. That is what the rest of `core/` does
//! — `core::card::manifest::SourceFacts`, `core::adf::fs::FileEntry` — and a
//! convenience `pub use` that nothing imports is an error in this crate, not a
//! warning.

pub mod cleanup;
pub mod igame;
pub mod igamewrite;
pub mod readers;
pub mod record;
pub mod scan;
pub mod store;
