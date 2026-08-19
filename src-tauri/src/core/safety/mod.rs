//! Data-safety primitives: *never destroy what you cannot safely verify*.
//!
//! This is the single gate every write in ART passes through. Distinct from
//! [`crate::core::security`], which defends against hostile *input* (path
//! traversal, malformed headers); this module defends the user's *existing
//! data* against ART itself — half-finished writes, silent overwrites, and
//! modifications with no way back.
//!
//! Spec §57 mandates the pipeline `Original → Backup → Operation → Validation
//! → Commit`. [`guarded_write`] implements the backup-and-commit half; callers
//! supply the validation before handing bytes over.

pub mod atomic;
pub mod backup;

pub use atomic::atomic_write;
pub use backup::{backup_file, BackupPolicy};

use std::path::{Path, PathBuf};

use crate::core::error::CoreResult;

/// Back up the current contents of `path` (per `policy`), then replace it with
/// `bytes` atomically.
///
/// Returns the backup path when one was taken, so the UI can tell the user
/// exactly where the previous version went (spec §92: say what will be backed
/// up).
///
/// The backup happens first: if it fails, the original is never touched.
pub fn guarded_write(
    path: &Path,
    bytes: &[u8],
    policy: BackupPolicy,
) -> CoreResult<Option<PathBuf>> {
    let backup = backup_file(path, policy)?;
    atomic_write(path, bytes)?;
    Ok(backup)
}

/// Delete a file ART itself put there, backing it up first per `policy`.
///
/// **A removal is a write.** This module's whole rule is that the user's
/// existing data never disappears without ART having made a copy first, and a
/// deletion is the most complete form of disappearing there is — so it goes
/// through the same gate, rather than being the one `std::fs` call that
/// bypasses it because it happens not to produce any bytes.
///
/// Returns the backup path when one was taken. A file that is not there is
/// `Ok(None)`, not an error: the caller wanted it gone and it is gone, which
/// is the same rule [`backup_file`] already applies to a file it has nothing
/// to preserve.
pub fn guarded_remove(path: &Path, policy: BackupPolicy) -> CoreResult<Option<PathBuf>> {
    if !path.is_file() {
        return Ok(None);
    }
    let backup = backup_file(path, policy)?;
    std::fs::remove_file(path)?;
    Ok(backup)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn scratch(tag: &str) -> PathBuf {
        let s = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("art-guarded-{tag}-{s}"));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn a_removal_backs_up_before_it_deletes() {
        let dir = scratch("remove");
        let target = dir.join("thing.uaem");
        fs::write(
            &target,
            b"--p-rwed 2026-08-19 14:59:16.00 
",
        )
        .unwrap();

        let backup = guarded_remove(&target, BackupPolicy::CONFIG)
            .unwrap()
            .expect("a removal preserves what it removes");

        assert!(!target.exists(), "the file is gone");
        assert_eq!(
            fs::read(&backup).unwrap(),
            b"--p-rwed 2026-08-19 14:59:16.00 
",
            "and what it said is not"
        );
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn removing_what_is_not_there_is_not_an_error() {
        let dir = scratch("remove-absent");
        assert!(
            guarded_remove(&dir.join("never-existed"), BackupPolicy::CONFIG)
                .unwrap()
                .is_none()
        );
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn backs_up_then_replaces() {
        let dir = scratch("both");
        let target = dir.join("disk.adf");
        fs::write(&target, b"version one").unwrap();

        let backup = guarded_write(&target, b"version two", BackupPolicy::DISK_IMAGE)
            .unwrap()
            .expect("a backup was expected");

        assert_eq!(fs::read(&target).unwrap(), b"version two");
        assert_eq!(fs::read(&backup).unwrap(), b"version one");
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn first_write_has_nothing_to_back_up() {
        let dir = scratch("first");
        let target = dir.join("brand-new.adf");

        let backup = guarded_write(&target, b"fresh", BackupPolicy::DISK_IMAGE).unwrap();

        assert!(backup.is_none());
        assert_eq!(fs::read(&target).unwrap(), b"fresh");
        fs::remove_dir_all(&dir).ok();
    }
}
