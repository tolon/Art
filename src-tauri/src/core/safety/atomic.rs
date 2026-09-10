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

/// Whether [`atomic_create_new`] made the file, or found one already there.
///
/// Two endings and they stay two: "ART wrote it" and "something of that name
/// was already on the user's disk and ART did not touch it" are different
/// next steps, and neither is a failure. A real failure is still an `Err`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Created {
    Yes,
    AlreadyThere,
}

/// Create `path` holding exactly `bytes` — **refusing an existing file, and
/// never leaving a partial one**.
///
/// [`atomic_write`] is the wrong primitive whenever the operation is
/// `SAFE_CREATE`: it renames over whatever is at the destination, which is
/// precisely what must not happen to a file the user already has. And a plain
/// `File::create_new` + `write_all` is the wrong one too, in the other
/// direction: a write that dies part way leaves a half-written file that the
/// *next* call then refuses to replace, so ART's own debris becomes something
/// the user is told to go and delete.
///
/// So both halves, in this order:
///
/// 1. **Reserve the name exclusively** with `create_new`. That is the
///    SAFE_CREATE guarantee, and it is a single atomic syscall — there is no
///    window between asking whether the file exists and taking the name.
/// 2. Write the bytes to a temporary file **in the same directory**, flush,
///    `sync_all`.
/// 3. `rename` the temporary over the reservation. On one volume that is
///    atomic, so the destination is the empty reservation or the whole file
///    and never a mix.
///
/// Anything failing after step 1 removes **both** the temporary and the
/// reservation, so a failed call leaves the folder exactly as it found it.
pub fn atomic_create_new(path: &Path, bytes: &[u8]) -> CoreResult<Created> {
    match fs::File::create_new(path) {
        Ok(_) => {}
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
            return Ok(Created::AlreadyThere)
        }
        Err(e) => return Err(CoreError::Io(e)),
    }

    let finish = (|| -> CoreResult<()> {
        let tmp = temp_path_for(path)?;
        let written = (|| -> std::io::Result<()> {
            let mut f = fs::File::create_new(&tmp)?;
            f.write_all(bytes)?;
            f.flush()?;
            f.sync_all()?;
            Ok(())
        })();
        if let Err(e) = written {
            let _ = fs::remove_file(&tmp);
            return Err(CoreError::Io(e));
        }
        if let Err(e) = fs::rename(&tmp, path) {
            let _ = fs::remove_file(&tmp);
            return Err(CoreError::Io(e));
        }
        Ok(())
    })();

    match finish {
        Ok(()) => Ok(Created::Yes),
        Err(e) => {
            // The reservation is ART's own, and only ART's: nothing else can
            // have opened this name between step 1 and here, because step 1
            // is what took it. Leaving the empty file would make the next
            // call answer "already there" about ART's own wreckage.
            let _ = fs::remove_file(path);
            Err(e)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(tag: &str) -> (crate::core::ScratchDir, std::path::PathBuf) {
        crate::core::ScratchDir::pair("art-atomic", tag)
    }

    #[test]
    fn writes_a_new_file() {
        let (_guard, dir) = scratch("new");
        let target = dir.join("fresh.adf");

        atomic_write(&target, b"hello amiga").unwrap();

        assert_eq!(fs::read(&target).unwrap(), b"hello amiga");
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn replaces_an_existing_file() {
        let (_guard, dir) = scratch("replace");
        let target = dir.join("existing.adf");
        fs::write(&target, b"old contents").unwrap();

        atomic_write(&target, b"new contents").unwrap();

        assert_eq!(fs::read(&target).unwrap(), b"new contents");
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn leaves_no_temp_files_behind() {
        let (_guard, dir) = scratch("cleanup");
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
    fn create_new_writes_a_file_that_is_not_there() {
        let (_guard, dir) = scratch("create-new");
        let target = dir.join("ART - what goes here.txt");

        assert_eq!(
            atomic_create_new(&target, b"the guide").unwrap(),
            Created::Yes
        );

        assert_eq!(fs::read(&target).unwrap(), b"the guide");
        // Atomic means the temporary went with it.
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

    /// The half `atomic_write` cannot give: the bytes already on the user's
    /// disk are still there afterwards, byte for byte, and the answer says so
    /// rather than being an error.
    #[test]
    fn create_new_never_replaces_what_is_already_there() {
        let (_guard, dir) = scratch("create-new-exists");
        let target = dir.join("ART - what goes here.txt");
        fs::write(&target, b"the owner's own notes").unwrap();

        assert_eq!(
            atomic_create_new(&target, b"the guide").unwrap(),
            Created::AlreadyThere
        );

        assert_eq!(fs::read(&target).unwrap(), b"the owner's own notes");
        fs::remove_dir_all(&dir).ok();
    }

    /// **The reservation is never what the caller is left with.** Step 1
    /// takes the name with a zero-length file; if the rename in step 3 were
    /// dropped, the destination would exist, be empty, and the *next* call
    /// would answer "already there" about ART's own wreckage. So the second
    /// call has to see the first call's real bytes and nothing else.
    #[test]
    fn the_reservation_is_replaced_by_the_real_bytes_not_left_empty() {
        let (_guard, dir) = scratch("create-new-twice");
        let target = dir.join("guide.txt");

        assert_eq!(atomic_create_new(&target, b"one").unwrap(), Created::Yes);
        assert_eq!(fs::metadata(&target).unwrap().len(), 3);

        assert_eq!(
            atomic_create_new(&target, b"two").unwrap(),
            Created::AlreadyThere
        );
        assert_eq!(fs::read(&target).unwrap(), b"one");
        fs::remove_dir_all(&dir).ok();
    }

    /// A destination in a directory that is not there is an error — not
    /// `AlreadyThere`, which would be a claim about a file, and not a silent
    /// success. Nothing is created.
    #[test]
    fn a_destination_in_a_missing_directory_is_an_error_and_creates_nothing() {
        let (_guard, dir) = scratch("create-new-nodir");
        let target = dir.join("not-here").join("guide.txt");

        assert!(atomic_create_new(&target, b"the guide").is_err());
        assert!(!target.exists());
        assert!(!dir.join("not-here").exists());
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
        let (_guard, dir) = scratch("concurrent");
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
        let (_guard, dir) = scratch("preserve");
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
