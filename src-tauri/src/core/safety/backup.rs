//! Generational backups taken before ART modifies a user's file.
//!
//! Spec §57 / §93: *originals are sacred*. Before any in-place modification the
//! previous contents are copied into a sibling `.art-backup/` directory as
//! `<name>.<stamp>.bak`. Old generations are pruned so the folder cannot grow
//! without bound.
//!
//! Policies differ by file size, not by importance — a 880 KB ADF is cheap to
//! copy on every edit, a multi-gigabyte HDF is not (that is the Snapshot
//! Manager's job, spec §49).

use std::fs;
use std::path::{Path, PathBuf};

use crate::core::error::{CoreError, CoreResult};

/// Directory name used for backups, created next to the file being modified.
pub const BACKUP_DIR: &str = ".art-backup";

/// How many previous generations to retain for a given class of file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BackupPolicy {
    pub enabled: bool,
    /// Number of generations to keep. Older ones are pruned.
    pub keep: usize,
}

impl BackupPolicy {
    /// Floppy-sized disk images (ADF ≈ 880 KB). Cheap to copy on every edit.
    pub const DISK_IMAGE: Self = Self {
        enabled: true,
        keep: 3,
    };

    /// Hand-tuned configuration files (FF.CFG, config.txt, cmdline.txt).
    /// Tiny, irreplaceable, and easy to get wrong — keep more generations.
    pub const CONFIG: Self = Self {
        enabled: true,
        keep: 5,
    };

    /// Multi-gigabyte images. Copying these on every edit would fill the disk,
    /// so automatic backup is off by default and the UI must say so.
    pub const LARGE_IMAGE: Self = Self {
        enabled: false,
        keep: 0,
    };

    /// Explicitly disabled.
    pub const NONE: Self = Self {
        enabled: false,
        keep: 0,
    };
}

/// Monotonic, fixed-width stamp so lexical sort == chronological sort.
fn stamp() -> String {
    let d = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    format!("{:011}-{:09}", d.as_secs(), d.subsec_nanos())
}

/// Copy `path`'s current contents into its `.art-backup/` directory.
///
/// Returns the backup path, or `None` when the policy is disabled or the file
/// does not exist yet (nothing to preserve).
pub fn backup_file(path: &Path, policy: BackupPolicy) -> CoreResult<Option<PathBuf>> {
    if !policy.enabled || !path.is_file() {
        return Ok(None);
    }

    let dir = path.parent().ok_or_else(|| {
        CoreError::InvalidInput(format!("'{}' has no parent directory", path.display()))
    })?;
    let name = path
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .ok_or_else(|| CoreError::InvalidInput("path has no file name".into()))?;

    let backup_dir = dir.join(BACKUP_DIR);
    fs::create_dir_all(&backup_dir)?;

    let target = backup_dir.join(format!("{name}.{}.bak", stamp()));
    fs::copy(path, &target)?;

    prune(&backup_dir, &name, policy.keep)?;

    Ok(Some(target))
}

/// Delete the oldest generations beyond `keep` for one source file name.
fn prune(backup_dir: &Path, name: &str, keep: usize) -> CoreResult<()> {
    let prefix = format!("{name}.");
    let mut generations: Vec<PathBuf> = fs::read_dir(backup_dir)?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| {
            p.file_name()
                .map(|f| {
                    let f = f.to_string_lossy();
                    f.starts_with(&prefix) && f.ends_with(".bak")
                })
                .unwrap_or(false)
        })
        .collect();

    if generations.len() <= keep {
        return Ok(());
    }

    // Fixed-width stamps make the lexical sort chronological.
    generations.sort();
    let excess = generations.len() - keep;
    for old in generations.into_iter().take(excess) {
        let _ = fs::remove_file(old);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(tag: &str) -> PathBuf {
        let s = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("art-backup-{tag}-{s}"));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn copies_current_contents() {
        let dir = scratch("copy");
        let target = dir.join("disk.adf");
        fs::write(&target, b"original bytes").unwrap();

        let backup = backup_file(&target, BackupPolicy::DISK_IMAGE)
            .unwrap()
            .expect("backup should have been created");

        assert_eq!(fs::read(&backup).unwrap(), b"original bytes");
        assert_eq!(fs::read(&target).unwrap(), b"original bytes");
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn disabled_policy_makes_no_backup() {
        let dir = scratch("disabled");
        let target = dir.join("huge.hdf");
        fs::write(&target, b"big").unwrap();

        let backup = backup_file(&target, BackupPolicy::LARGE_IMAGE).unwrap();

        assert!(backup.is_none());
        assert!(!dir.join(BACKUP_DIR).exists());
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn missing_file_is_not_an_error() {
        let dir = scratch("missing");
        let target = dir.join("not-yet-created.adf");

        let backup = backup_file(&target, BackupPolicy::DISK_IMAGE).unwrap();

        assert!(backup.is_none());
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn prunes_beyond_the_keep_limit() {
        let dir = scratch("prune");
        let target = dir.join("disk.adf");
        let policy = BackupPolicy {
            enabled: true,
            keep: 2,
        };

        for generation in 0..5 {
            fs::write(&target, format!("generation {generation}")).unwrap();
            backup_file(&target, policy).unwrap();
        }

        let kept: Vec<_> = fs::read_dir(dir.join(BACKUP_DIR))
            .unwrap()
            .filter_map(|e| e.ok())
            .collect();
        assert_eq!(kept.len(), 2, "should retain exactly `keep` generations");
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn prune_only_touches_the_same_source_name() {
        let dir = scratch("isolate");
        let a = dir.join("a.adf");
        let b = dir.join("b.adf");
        let policy = BackupPolicy {
            enabled: true,
            keep: 1,
        };

        fs::write(&a, b"a1").unwrap();
        backup_file(&a, policy).unwrap();
        fs::write(&b, b"b1").unwrap();
        backup_file(&b, policy).unwrap();
        fs::write(&a, b"a2").unwrap();
        backup_file(&a, policy).unwrap();

        let names: Vec<String> = fs::read_dir(dir.join(BACKUP_DIR))
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().to_string())
            .collect();

        // One generation of a.adf, one of b.adf — pruning a did not touch b.
        assert_eq!(names.len(), 2, "got {names:?}");
        assert!(names.iter().any(|n| n.starts_with("a.adf.")));
        assert!(names.iter().any(|n| n.starts_with("b.adf.")));
        fs::remove_dir_all(&dir).ok();
    }
}
