//! `OwnedScratch` — a product scratch guard that names what it could not
//! remove (ART-340 groundwork).
//!
//! **Why this exists next to `ScratchDir`, not instead of it.** `ScratchDir`
//! (`core/mod.rs`) is `#[cfg(test)]` only — it exists so a test's own scratch
//! directory is removed even when the test panics, and it says so to stderr
//! on failure because a test has no user-facing ending to report through.
//! `OwnedScratch` is the product-code counterpart P1 calls for: a folder ART
//! creates under the scratch root for a build session that spans several
//! Tauri commands (`card_os_open` … `card_os_close`), where no single Rust
//! stack frame owns the whole session and `finish()` is called explicitly at
//! the end the caller actually reached — succeeded, refused, or stopped.
//! `Drop` is the same best-effort backstop `ScratchDir` uses for the case
//! nothing called `finish()` (a panic, an early return), but it reports
//! through `log::warn!` rather than stderr, because product code has a log.
//!
//! **Why `finish()` returns `Result<(), LeftBehind>` instead of a `bool` or
//! swallowing the error.** "The screen may not out-claim the core" (CLAUDE.md,
//! "The failure that does not crash"): a caller that only sees `Ok(())` when
//! nothing went wrong, and otherwise sees exactly which folder is still there
//! and why, can report that named folder to the user — rather than either
//! claiming a removal that did not happen, or losing the reason a `bool`
//! would have thrown away.
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use crate::core::error::{CoreError, CoreResult};

/// What removing an [`OwnedScratch`] left behind.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LeftBehind {
    pub path: String,
    pub why: String,
}

/// A folder ART created under the scratch root and must remove on every
/// ending. `finished` tracks whether [`OwnedScratch::finish`] already ran,
/// so [`Drop`] never attempts (and never double-reports) a second removal.
pub struct OwnedScratch {
    path: PathBuf,
    finished: bool,
}

/// Process-wide, so two guards created in the same run never collide even
/// when both land in the same `n`-free millisecond — the same reasoning
/// `core::test_scratch_id` documents for `ScratchDir`, here for product code
/// instead of tests.
static NEXT: AtomicU64 = AtomicU64::new(0);

impl OwnedScratch {
    /// A fresh, empty folder `<root>/<prefix>-<pid>-<n>`. Refuses
    /// ([`CoreError::SafetyRefused`]) naming the path when it already
    /// exists — ART removes only a folder it made in this run, never one
    /// found sitting there (P3's same rule, applied to the session folder).
    pub fn create_in(root: &Path, prefix: &str) -> CoreResult<Self> {
        let n = NEXT.fetch_add(1, Ordering::Relaxed);
        let path = root.join(format!("{prefix}-{}-{n}", std::process::id()));
        if path.exists() {
            return Err(CoreError::SafetyRefused(format!(
                "scratch folder already exists: {}",
                path.display()
            )));
        }
        std::fs::create_dir_all(&path)?;
        Ok(Self {
            path,
            finished: false,
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Remove it now. `Ok(())` when it is gone; `Err(LeftBehind)` naming it
    /// (and why) when it is not — never a claim of removal that did not
    /// happen.
    pub fn finish(mut self) -> Result<(), LeftBehind> {
        self.finished = true;
        match std::fs::remove_dir_all(&self.path) {
            Ok(()) => Ok(()),
            Err(err) => {
                if self.path.exists() {
                    Err(LeftBehind {
                        path: self.path.display().to_string(),
                        why: err.to_string(),
                    })
                } else {
                    Ok(())
                }
            }
        }
    }
}

impl Drop for OwnedScratch {
    /// Best effort when [`finish`](Self::finish) was not called — a panic or
    /// an early return upstream of it. Logged, never propagated: `Drop`
    /// cannot return a `Result`, and this is the backstop, not the primary
    /// path a caller is expected to rely on for the folder's fate.
    fn drop(&mut self) {
        if self.finished {
            return;
        }
        if let Err(err) = std::fs::remove_dir_all(&self.path) {
            if self.path.exists() {
                log::warn!(
                    "OwnedScratch: {} not removed on drop: {err}",
                    self.path.display()
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finish_removes_the_folder_and_everything_in_it() {
        let (_guard, root) = crate::core::ScratchDir::pair("art-owned-scratch", "finish");
        let owned = OwnedScratch::create_in(&root, "card-os").unwrap();
        let inner = owned.path().join("staging").join("a.txt");
        std::fs::create_dir_all(inner.parent().unwrap()).unwrap();
        std::fs::write(&inner, b"x").unwrap();
        let path = owned.path().to_path_buf();
        assert_eq!(owned.finish(), Ok(()));
        assert!(!path.exists());
    }

    /// Deviation from the brief's literal body (disclosed in the task
    /// report): a plain `std::fs::File::create` does **not** block
    /// `remove_dir_all` on this Rust toolchain — Windows `File` opens with
    /// `FILE_SHARE_DELETE` in its default sharing flags, so a bare open
    /// handle no longer keeps a file (or its folder) undeletable. The
    /// established way this codebase actually reproduces an unremovable
    /// file is `share_mode(0)` (no sharing at all) — see
    /// `core/appearance/mod.rs`'s `a_failed_arrange_leaves_every_icon_byte_for_byte_unchanged`
    /// and `scratch.rs`'s `take_sweep_lock` for the same pattern, both
    /// already in this crate.
    #[cfg(windows)]
    #[test]
    fn a_folder_that_cannot_be_removed_is_named_not_claimed_gone() {
        use std::os::windows::fs::OpenOptionsExt;

        let (_guard, root) = crate::core::ScratchDir::pair("art-owned-scratch", "held");
        let owned = OwnedScratch::create_in(&root, "card-os").unwrap();
        let held = owned.path().join("held.bin");
        std::fs::write(&held, b"x").unwrap();
        // No sharing at all: nobody else, including our own `finish()` call
        // below, may open this file while `_lock` is alive.
        let _lock = std::fs::OpenOptions::new()
            .read(true)
            .share_mode(0)
            .open(&held)
            .unwrap();
        let path = owned.path().display().to_string();
        let left = owned.finish().unwrap_err();
        assert_eq!(left.path, path);
        assert!(!left.why.is_empty());
    }

    #[test]
    fn two_guards_in_one_process_never_share_a_folder() {
        let (_guard, root) = crate::core::ScratchDir::pair("art-owned-scratch", "unique");
        let a = OwnedScratch::create_in(&root, "card-os").unwrap();
        let b = OwnedScratch::create_in(&root, "card-os").unwrap();
        assert_ne!(a.path(), b.path());
    }

    #[test]
    fn dropping_without_finish_still_removes_it() {
        let (_guard, root) = crate::core::ScratchDir::pair("art-owned-scratch", "drop");
        let path = OwnedScratch::create_in(&root, "card-os")
            .unwrap()
            .path()
            .to_path_buf();
        assert!(!path.exists());
    }
}
