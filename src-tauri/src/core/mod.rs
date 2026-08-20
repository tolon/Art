//! Amiga Core Engine
//!
//! Platform-independent Rust modules for Amiga file-format handling.
//! This crate MUST NOT depend on `tauri` — it is pure Rust (`std` + `serde`)
//! so it stays unit-testable and reusable by a future CLI or other shells.
//!
//! See `docs/architecture.md` for the layered design.

pub mod adf;
pub mod amigaver;
pub mod analysis;
pub mod archive;
pub mod artwork;
pub mod binary;
pub mod card;
pub mod cbm;
pub mod compatibility;
pub mod conversion;
pub mod detect;
pub mod dirsize;
pub mod distro;
pub mod error;
pub mod fat32;
pub mod gameindex;
pub mod gotek;
pub mod hashing;
pub mod hdf;
pub mod hostfs;
pub mod iso;
pub mod jobs;
pub mod launch;
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

/// A component that makes a test's scratch directory name unique — process
/// id plus a counter that never repeats within the process.
///
/// **Why a counter and not a timestamp** (ART-164, ART-115, ART-173). Cargo
/// runs tests in parallel threads of **one** process, so the pid is shared by
/// every one of them, and `SystemTime::now()` does not advance between two
/// calls that land in the same clock tick — coarse on Windows. Two tests then
/// build the same directory name, and whichever writes second hands the other
/// its fixture. That has been diagnosed three times in this codebase now:
/// `core::iso` (5 failures in 40 runs, four *different* tests losing the
/// race), `core::cbm` (4 in 40), and `net`'s own test server before them.
///
/// It is a `String` rather than a number so a helper that already formats
/// `"{tag}-{}"` with `std::process::id()` can swap one call for another and
/// gain the counter without its format string changing at all — which is how
/// the sweep across every scratch helper in the crate stayed mechanical and
/// reviewable.
///
/// Test-only: nothing ART ships names a file this way.
#[cfg(test)]
pub fn test_scratch_id() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);
    format!(
        "{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    )
}
