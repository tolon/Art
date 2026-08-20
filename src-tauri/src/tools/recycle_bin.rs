//! The Windows Recycle Bin as ART's host recycler (ART-080).
//!
//! The implementation of [`HostRecycler`]. It lives here rather than in
//! `core/` for the same reason `hst_imager.rs` beside it does: it calls
//! something platform-specific — `IFileOperation`, through the `trash` crate —
//! and `core/` does not (CLAUDE.md's core-independence rule). `core/hostfs`
//! declares the trait and carries every rule about *which* files may be named
//! and what a partial failure has to say; this file knows only how to hand one
//! path to the shell.
//!
//! ## Why the Recycle Bin, and not a mechanism of ART's own
//!
//! The owner's ruling on ART-080: **ART invents no recovery mechanism of its
//! own and uses the one the operating system already has — the one place a
//! user already knows to look.** The two alternatives both failed on that
//! test. A `core/safety` backup would put `.art-backup/` directories inside
//! the user's own `D:\downloads` and duplicate a multi-gigabyte ISO in order
//! to move it, and nobody discovers it. A permanent unlink costs nothing and
//! cannot be undone by anyone.
//!
//! ## What it refuses rather than guesses
//!
//! Not every path has a Recycle Bin. A network share, and some removable
//! media, have none — Explorer deletes permanently there, after saying so.
//! **ART does not**: `trash` reports the failure and
//! [`core::hostfs::recycle_many`] records it against that entry by name, so
//! the file stays and the user is told which one and why. Silently falling
//! back to a permanent delete would be ART choosing, on the user's behalf, the
//! one option the owner ruled out.

use std::path::Path;

use crate::core::error::{CoreError, CoreResult};
use crate::core::hostfs::{HostRecycler, RecycleTarget};

/// Sends files to the Windows Recycle Bin.
///
/// Stateless: `trash` initialises its own COM apartment per call
/// (`coinit_apartmentthreaded`, the mode the shell's file-operation
/// interfaces expect and the crate's own default), so there is nothing to
/// hold open between deletes and nothing to tear down if a job is cancelled.
#[derive(Debug, Default, Clone, Copy)]
pub struct RecycleBin;

impl HostRecycler for RecycleBin {
    fn target(&self) -> RecycleTarget {
        RecycleTarget::WindowsRecycleBin
    }

    fn recycle(&self, path: &Path) -> CoreResult<()> {
        // `trash::delete` takes the path as given — no name is built here and
        // no string is concatenated. `core::hostfs::recycle_many` has already
        // resolved and `safe_join`-checked it, which is the only route from a
        // name to a path in this codebase.
        trash::delete(path).map_err(|err| {
            // The host's own words, not "it failed": the commonest reasons a
            // file will not go — it is open in something, the volume has no
            // Recycle Bin, the user lacks permission — are all things the user
            // can act on, and only the shell knows which one it was.
            CoreError::InvalidInput(format!(
                "'{}' could not go to the Recycle Bin: {err}",
                path.display()
            ))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::hostfs::recycle_many;
    use crate::core::jobs::NoProgress;
    use std::path::PathBuf;

    fn scratch(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "art-recyclebin-{tag}-{}",
            crate::core::test_scratch_id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// The trait's own answer, which is what reaches the screen and which the
    /// UI translates. Pinned because a `RecycleTarget` that did not match the
    /// implementation would tell the user to look somewhere the file is not.
    #[test]
    fn it_names_the_windows_recycle_bin() {
        assert_eq!(RecycleBin.target(), RecycleTarget::WindowsRecycleBin);
    }

    /// **The real shell, on a real file.**
    ///
    /// `#[ignore]` and not in CI, for one reason worth saying plainly: it puts
    /// something in the machine's actual Recycle Bin. That is harmless — a
    /// five-byte file in a temp directory, named so it is obvious where it
    /// came from — but it is a side effect outside the tempdir every other
    /// test in this project confines itself to, and CLAUDE.md's fixture rule
    /// is about exactly that. The rest of ART-080 is proved against
    /// `core::hostfs`'s own fake, which is why the trait exists.
    ///
    /// Run it deliberately:
    ///
    /// ```text
    /// cargo test recycle_bin -- --ignored --nocapture
    /// ```
    #[test]
    #[ignore = "puts a file in the machine's real Recycle Bin"]
    fn a_real_file_really_goes_to_the_real_bin() {
        let dir = scratch("real");
        let path = dir.join("art-recycle-bin-probe.txt");
        std::fs::write(&path, b"ART").unwrap();

        let outcome = recycle_many(
            &RecycleBin,
            &dir,
            &["art-recycle-bin-probe.txt".to_string()],
            &NoProgress,
        )
        .unwrap();

        assert_eq!(outcome.removed(), 1, "{outcome:?}");
        assert_eq!(outcome.target, Some(RecycleTarget::WindowsRecycleBin));
        assert!(
            !path.exists(),
            "and it is really gone from the folder, not merely reported as gone"
        );
    }
}
