//! The one-button card's top: everything that turns a built system tree and a
//! list of named partitions into one card image (design
//! `docs/superpowers/specs/2026-09-11-one-button-card-design.md`).
//!
//! **This module is the top of the card path and nothing below imports it.**
//! It reads sources (`content`), finds the PFS3 driver (`driver`), chooses and
//! writes WHDLoad (`whdload`), proposes and places Kickstarts (`kickstarts`),
//! and measures, checks and stages a card (`prepare`). `core/card` builds the
//! image's shape and `core/preload` fills a volume; neither knows this module
//! exists. `core::independence` holds that (ART-339).

pub mod content;
pub mod partial;
