//! The one-button card's top: everything that turns a built system tree and a
//! list of named partitions into one card image (design
//! `docs/superpowers/specs/2026-09-11-one-button-card-design.md`).
//!
//! **This module is the top of the card path and nothing below imports it.**
//! It reads sources (`content`), finds the PFS3 driver (`driver`), chooses and
//! writes WHDLoad (`whdload`), proposes and places Kickstarts (`kickstarts`),
//! measures, checks and stages a card (`prepare`), and counts what a built
//! card's partitions hold (`readback`). `core/card` builds the image's shape
//! and `core/preload` fills a volume; neither knows this module exists.
//! `core::independence` holds that (ART-339).

use std::path::Path;

pub mod content;
pub mod driver;
pub mod kickstarts;
pub mod partial;
pub mod prepare;
pub mod readback;
pub mod whdload;

/// A material folder as a refusal names it: said to not exist when it does
/// not, so it never reads as a folder that was searched and held nothing
/// (card round 3, M8).
pub(crate) fn searched_folder(folder: &Path) -> String {
    if folder.is_dir() {
        folder.display().to_string()
    } else {
        format!("{} (does not exist)", folder.display())
    }
}
