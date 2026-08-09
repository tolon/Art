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
