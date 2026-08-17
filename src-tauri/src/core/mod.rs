//! Amiga Core Engine
//!
//! Platform-independent Rust modules for Amiga file-format handling.
//! This crate MUST NOT depend on `tauri` — it is pure Rust (`std` + `serde`)
//! so it stays unit-testable and reusable by a future CLI or other shells.
//!
//! See `docs/architecture.md` for the layered design.

pub mod adf;
pub mod analysis;
pub mod archive;
pub mod binary;
pub mod card;
pub mod cbm;
pub mod collection;
pub mod compatibility;
pub mod conversion;
pub mod detect;
pub mod distro;
pub mod error;
pub mod fat32;
pub mod gameindex;
pub mod gotek;
pub mod hashing;
pub mod hdf;
pub mod iso;
pub mod jobs;
pub mod layout;
pub mod lha;
pub mod mbr;
pub mod oplog;
pub mod osinstall;
pub mod pistorm;
pub mod preload;
pub mod profile;
pub mod rdb;
pub mod recovery;
pub mod rom;
pub mod safety;
pub mod security;
pub mod sources;
pub mod validation;
pub mod volume;
pub mod whdload;
pub mod winuae;
pub mod workflow;

#[allow(unused_imports)]
pub use error::{CoreError, CoreResult};
