//! External programs ART drives, and the only place it drives them from.
//!
//! `core/` declares what it needs as a trait and never launches anything
//! (CLAUDE.md); this is where those traits are implemented. Same shape as
//! `net/`, which is the only place a connection is opened.
//!
//! One rule holds for everything here: **structured argv, never a shell
//! string** (`core/security`). A path the user picked is an argument, not text
//! concatenated into a command line.

pub mod hst_imager;
