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
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    temp_path_at(path, stamp)
}

/// The body of [`temp_path_for`] with the clock passed in.
///
/// The clock is a parameter for one reason: the property that matters is
/// "unique **even when the clock does not advance**", and a test cannot
/// arrange that against a real `SystemTime::now()`. Two calls a nanosecond
/// apart is luck; two calls with the same stamp is an argument.
fn temp_path_at(path: &Path, stamp: u128) -> CoreResult<PathBuf> {
    let dir = path.parent().ok_or_else(|| {
        CoreError::InvalidInput(format!("'{}' has no parent directory", path.display()))
    })?;
    let stem = path
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "art-output".to_string());

    // A stamp alone does NOT keep concurrent writers from colliding — that is
    // what the comment here used to claim, and it was wrong (ART-181). Two
    // threads can read the same nanosecond, and the open below used to be a
    // truncating `create`, so both would have written into one file and both
    // renamed it over the destination. On the one path every user file in ART
    // is written through, that is a corrupted file, not a flaky test.
    //
    // The counter is what makes the name unique within the process; the
    // exclusive open in `atomic_write` is what makes it unique against
    // anything else. The stamp only makes it readable.
    static NEXT_TEMP: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let seq = NEXT_TEMP.fetch_add(1, std::sync::atomic::Ordering::Relaxed);

    Ok(dir.join(format!(".{stem}.art-tmp-{stamp}-{seq}")))
}

/// Write `bytes` to `path` atomically.
///
/// On success the destination contains exactly `bytes`. On failure the
/// destination is left untouched and the temporary file is removed.
pub fn atomic_write(path: &Path, bytes: &[u8]) -> CoreResult<()> {
    let tmp = temp_path_for(path)?;

    // Scope the handle so it is closed before the rename (required on Windows).
    let write_result = (|| -> std::io::Result<()> {
        // `create_new` and not `create`: an exclusive open turns a name that
        // is somehow still taken into an error instead of silently truncating
        // whatever is there. See the counter above.
        let mut f = fs::File::create_new(&tmp)?;
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
        let stamp = crate::core::test_scratch_id();
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
    fn two_temp_paths_for_one_destination_are_never_equal() {
        // ART-181, and **this is the guard**. The temp name used to be a bare
        // nanosecond stamp opened with a truncating `create`: two threads that
        // read the same nanosecond wrote into ONE file and both renamed it
        // over the destination, on the path every user file in ART is written
        // through.
        //
        // It is asserted here and not in the threaded test below because this
        // one fails against the defect *every* time. Collisions are what the
        // bug produces; distinct names are what the fix guarantees, and only
        // the second is a property a test can hold to.
        // The clock is held still on purpose. Against the defect this fails
        // every run; with a real clock it passed five runs out of five, which
        // is how the first two attempts at this test were caught being no
        // test at all.
        let target = Path::new("C:/nowhere/disk.adf");
        let frozen = 1_700_000_000_000_000_000u128;
        let mut seen = std::collections::HashSet::new();
        for _ in 0..64 {
            assert!(
                seen.insert(temp_path_at(target, frozen).unwrap()),
                "two temp names for one destination collided when the clock did not advance",
            );
        }
    }

    #[test]
    fn concurrent_writers_leave_one_whole_payload() {
        // A stressor, **not** the guard — it passed five runs out of five
        // against the ART-181 defect, because two threads landing inside one
        // nanosecond is luck rather than something a test can arrange. It is
        // kept because it exercises the real `atomic_write` under real
        // threads, and the deterministic guard above is what actually holds
        // the property.
        //
        // Each writer's payload is a distinct byte repeated, so a mix is
        // detectable: a correct result is entirely one byte.
        let dir = scratch("concurrent");
        let target = dir.join("contested.adf");
        const WRITERS: usize = 16;
        const LEN: usize = 64 * 1024;

        std::thread::scope(|s| {
            for i in 0..WRITERS {
                let target = target.clone();
                s.spawn(move || {
                    let payload = vec![b'a' + i as u8; LEN];
                    atomic_write(&target, &payload).unwrap();
                });
            }
        });

        let got = fs::read(&target).unwrap();
        assert_eq!(got.len(), LEN, "destination is not one whole payload");
        let first = got[0];
        assert!(
            got.iter().all(|&b| b == first),
            "destination holds a mix of two writers' bytes",
        );
        assert!((b'a'..b'a' + WRITERS as u8).contains(&first));

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
