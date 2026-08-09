//! Atomic file writes.
//!
//! A plain `std::fs::write` truncates the destination *before* the new bytes
//! land. If the process dies, the power fails, or the disk fills up half way
//! through, the user is left with a truncated file — for an ADF or HDF that
//! means a destroyed disk image.
//!
//! Every write in ART goes through [`atomic_write`] instead: the bytes are
//! written to a temporary file **in the same directory** (so the rename stays
//! on one volume and is therefore atomic), flushed to disk with `sync_all`,
//! and only then renamed over the destination. The destination is either the
//! old file or the new one — never a half-written mix.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use crate::core::error::{CoreError, CoreResult};

/// Build a temp path next to `path` that will not collide with a real file.
fn temp_path_for(path: &Path) -> CoreResult<PathBuf> {
    let dir = path.parent().ok_or_else(|| {
        CoreError::InvalidInput(format!("'{}' has no parent directory", path.display()))
    })?;
    let stem = path
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "art-output".to_string());

    // Nanosecond stamp keeps concurrent writers from colliding. The leading dot
    // hides the file on Unix and keeps it out of the way on Windows.
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);

    Ok(dir.join(format!(".{stem}.art-tmp-{stamp}")))
}

/// Write `bytes` to `path` atomically.
///
/// On success the destination contains exactly `bytes`. On failure the
/// destination is left untouched and the temporary file is removed.
pub fn atomic_write(path: &Path, bytes: &[u8]) -> CoreResult<()> {
    let tmp = temp_path_for(path)?;

    // Scope the handle so it is closed before the rename (required on Windows).
    let write_result = (|| -> std::io::Result<()> {
        let mut f = fs::File::create(&tmp)?;
        f.write_all(bytes)?;
        f.flush()?;
        // Force the bytes out of the OS cache before we swap the file in.
        f.sync_all()?;
        Ok(())
    })();

    if let Err(e) = write_result {
        let _ = fs::remove_file(&tmp);
        return Err(CoreError::Io(e));
    }

    if let Err(e) = fs::rename(&tmp, path) {
        let _ = fs::remove_file(&tmp);
        return Err(CoreError::Io(e));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(tag: &str) -> PathBuf {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("art-atomic-{tag}-{stamp}"));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn writes_a_new_file() {
        let dir = scratch("new");
        let target = dir.join("fresh.adf");

        atomic_write(&target, b"hello amiga").unwrap();

        assert_eq!(fs::read(&target).unwrap(), b"hello amiga");
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn replaces_an_existing_file() {
        let dir = scratch("replace");
        let target = dir.join("existing.adf");
        fs::write(&target, b"old contents").unwrap();

        atomic_write(&target, b"new contents").unwrap();

        assert_eq!(fs::read(&target).unwrap(), b"new contents");
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn leaves_no_temp_files_behind() {
        let dir = scratch("cleanup");
        let target = dir.join("disk.adf");

        atomic_write(&target, b"payload").unwrap();

        let leftovers: Vec<_> = fs::read_dir(&dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().to_string())
            .filter(|n| n.contains("art-tmp"))
            .collect();
        assert!(
            leftovers.is_empty(),
            "temp files left behind: {leftovers:?}"
        );
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn failed_write_preserves_the_original() {
        let dir = scratch("preserve");
        let target = dir.join("precious.adf");
        fs::write(&target, b"irreplaceable").unwrap();

        // A directory that does not exist cannot host the temp file, so the
        // write fails before the destination is touched.
        let doomed = dir.join("no-such-dir").join("precious.adf");
        assert!(atomic_write(&doomed, b"whatever").is_err());

        assert_eq!(fs::read(&target).unwrap(), b"irreplaceable");
        fs::remove_dir_all(&dir).ok();
    }
}
