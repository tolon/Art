//! Installing a downloaded package into a floppy image (§41.5.6).
//!
//! This is the last step of "find it, fetch it, use it", and it is deliberately
//! the *thinnest* part of the module. Everything dangerous already exists and
//! is already tested:
//!
//! - unpacking is `core/lha::extract_archive_with` — traversal defence, bomb
//!   caps and overwrite policy included;
//! - writing is `core/volume/write` (the Stage W writer), reached through
//!   `commands::volume_write::run_copy_in_folder`, which journals every entry,
//!   verifies it and commits atomically (§57).
//!
//! So this file only unpacks the archive into a scratch directory. An ADF and
//! a hard disk partition are the same install, two destinations (§41.5.3) —
//! the `commands` layer decides where the unpacked tree goes.
//!
//! ## ADF only, on purpose
//!
//! §41.5.10's Stage B wants "one-click install to HDF" as well. ART cannot do
//! that yet — writing into an HDF means writing into a partition's filesystem,
//! and PFS3/SFS are not implemented while FFS is bound to the floppy layout.
//! The workflow catalogue already carries `install_hdf` as "Coming Later"
//! (§96); pretending otherwise here would be the exact claim §10 and §89
//! forbid.

use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::core::error::{CoreError, CoreResult};
use crate::core::jobs::ProgressSink;
use crate::core::lha::safe_extract::extract_archive_with;
use crate::core::lha::OverwritePolicy;

/// What an install actually did.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct InstallOutcome {
    pub files: usize,
    pub directories: usize,
    pub bytes: u64,
    /// Where inside the image it went; empty for the root.
    pub into: String,
    /// Where the previous version of the image was kept.
    pub backup: Option<String>,
    /// Entries the archive held that ART did not write, with the reason.
    pub skipped: Vec<String>,
}

/// Unpack an archive into a scratch directory, ready to be copied into a
/// volume.
///
/// Both install destinations — an ADF and a hard disk partition — share this
/// one unpack: traversal defence, decompression-bomb limits and all, rather
/// than growing a second one (§41.5.3).
///
/// The caller owns the scratch directory and it removes itself when dropped.
///
/// `scratch_root` is the caller's, never this module's (ART-196): where a
/// platform stages work it will throw away is not a question `core/` gets to
/// answer, the same rule `osinstall::plan_with_cache` states for its own
/// cache directory. The shell hands it `crate::scratch::root()`.
pub fn unpack_for_install(
    archive: &Path,
    scratch_root: &Path,
    sink: &dyn ProgressSink,
) -> CoreResult<(Scratch, Vec<String>)> {
    if sink.is_cancelled() {
        return Err(CoreError::Cancelled);
    }

    let scratch = Scratch::in_dir(scratch_root)?;
    sink.report(0, None, "Unpacking");

    let extracted =
        extract_archive_with(archive, scratch.path(), OverwritePolicy::Overwrite, sink)?;
    if extracted.aborted {
        return Err(CoreError::SafetyRefused(
            extracted
                .abort_reason
                .unwrap_or_else(|| "the archive was refused".into()),
        ));
    }

    let skipped = extracted
        .extracted
        .iter()
        .filter(|entry| entry.skipped && !entry.is_dir)
        .map(|entry| {
            format!(
                "{} ({})",
                entry.source_path,
                entry.reason.clone().unwrap_or_else(|| "skipped".into())
            )
        })
        .collect();

    Ok((scratch, skipped))
}

/// A temporary directory that removes itself.
/// A scratch directory that removes itself.
///
/// `pub(crate)` so the volume install path can unpack into one and hand it to
/// the Stage W copy engine: an unpacked archive is a folder, and copying a
/// folder into a volume is already a tested operation.
#[derive(Debug)]
pub(crate) struct Scratch(PathBuf);

impl Scratch {
    /// A fresh scratch directory under `root`.
    ///
    /// **`root` rather than `std::env::temp_dir()`** — ART-196. Everything
    /// this type stages used to land on the system drive whatever the user
    /// would have preferred; the root is now the shell's answer, and the
    /// shell's answer is the user's.
    pub(crate) fn in_dir(root: &Path) -> CoreResult<Self> {
        // The process id plus a counter is enough: two installs in the same
        // process must not share a directory, and two processes cannot.
        use std::sync::atomic::{AtomicU64, Ordering};
        static NEXT: AtomicU64 = AtomicU64::new(0);

        let path = root.join(format!(
            "art-install-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path)?;
        Ok(Self(path))
    }

    pub(crate) fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::jobs::NoProgress;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "art-install-t-{name}-{}",
            crate::core::test_scratch_id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn archive_with(files: &[(&str, &[u8])]) -> Vec<u8> {
        crate::core::lha::tests::make_lha_with(files)
    }

    /// An archive with nothing in it is rejected by the LHA reader before the
    /// installer ever sees it, which is the right place for it to fail. Both
    /// destinations share this unpack, so it only needs testing once.
    #[test]
    fn an_archive_with_no_files_is_an_honest_error() {
        let dir = scratch("empty");
        let archive = dir.join("pkg.lha");
        std::fs::write(&archive, archive_with(&[])).unwrap();

        let err = unpack_for_install(&archive, &std::env::temp_dir(), &NoProgress).unwrap_err();
        assert_eq!(err.code(), "ART-FORMAT-MALFORMED");

        std::fs::remove_dir_all(&dir).ok();
    }

    /// Unpacking recreates the archive's own tree, nested folders included —
    /// this is the shape both destinations copy from.
    #[test]
    fn unpacking_recreates_the_archive_s_tree() {
        let dir = scratch("unpack");
        let archive = dir.join("pkg.lha");
        std::fs::write(
            &archive,
            archive_with(&[("Docs/readme.txt", b"a"), ("Docs/notes.txt", b"b")]),
        )
        .unwrap();

        let (scratch_dir, skipped) =
            unpack_for_install(&archive, &std::env::temp_dir(), &NoProgress).unwrap();
        assert!(skipped.is_empty());
        assert_eq!(
            std::fs::read(scratch_dir.path().join("Docs/readme.txt")).unwrap(),
            b"a"
        );
        assert_eq!(
            std::fs::read(scratch_dir.path().join("Docs/notes.txt")).unwrap(),
            b"b"
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn cancelling_unpacks_nothing() {
        struct Cancelled;
        impl ProgressSink for Cancelled {
            fn report(&self, _: u64, _: Option<u64>, _: &str) {}
            fn is_cancelled(&self) -> bool {
                true
            }
        }

        let dir = scratch("cancel");
        let archive = dir.join("pkg.lha");
        std::fs::write(&archive, archive_with(&[("hello.txt", b"hi")])).unwrap();

        let err = unpack_for_install(&archive, &std::env::temp_dir(), &Cancelled).unwrap_err();
        assert_eq!(err.code(), "ART-CANCELLED");

        std::fs::remove_dir_all(&dir).ok();
    }
}
