//! Which install disks are in a folder — found by opening each candidate and
//! reading its volume name from inside it, never by trusting its filename
//! (`source`'s own rule, carried one level up: `AdfSource::open` already
//! reads the label out of the root block, not off the path it was given).
//!
//! ## One level, no recursion, no symlinks
//!
//! A real media folder is flat: the user's own 36 ADFs, sitting beside a
//! `readme.txt`, some `.info` files, maybe an HDF that is not install media
//! at all. It is not walked the way `core::layout::scan` walks a WHDLoad
//! collection — a subdirectory here is something else (a backup, an
//! unrelated folder a file manager created), not more install media, and a
//! symlink is never followed, so neither its target's contents nor its
//! target's name get read as if they were a file the user placed in this
//! folder directly. `std::fs::symlink_metadata` — never `metadata`, which
//! would follow the link — is what makes both of those `false` in one check:
//! a symlink is never `is_file()`, whatever it points at, and neither is a
//! subdirectory.
//!
//! ## Skipped, not an error
//!
//! [`AdfSource::open`] already refuses anything that is not a single bare
//! AmigaDOS volume — an RDB-partitioned HDF, a file with no recognisable
//! signature, one truncated below a single block. Every one of those is a
//! normal thing to find beside install media, so `find_media` treats
//! `Err` from `open` as "this candidate is not media", not as a reason to
//! fail the whole scan. Only a directory-listing failure (the folder itself
//! is unreadable) propagates.
//!
//! ## Cost: opening 36 candidates
//!
//! `AdfSource::open` does not read a whole user file just to learn its
//! label — but for a floppy-sized image that guarantee is weaker than it
//! looks. `core::volume::mount::scan_image` reads `min(1 MiB, file_len)`
//! bytes looking for a signature, and every ADF this project writes is
//! under 1 MiB (880 KB, DD), so that "window" *is* the whole file for a
//! floppy image — `open` was never going to avoid the read for this case,
//! only for something bigger, and install media never is. Scanning the
//! user's real 36-disk folder therefore reads roughly 36 x 880 KB, about
//! 31 MB, off disk — a few milliseconds of sequential I/O on any storage
//! this runs on, not the multi-second scan the task's warning was
//! watching for. Said plainly because the alternative (assuming "windowed"
//! meant "cheap" without checking) is exactly the kind of thing that turns
//! into a real complaint the first time someone points ART at a folder of
//! larger images.
//!
//! ## Duplicate volume names are refused, not resolved
//!
//! Two files in one folder can carry the same volume name: a plain copy, or
//! two different revisions of "the same" disk that happen to share a label.
//! `media_for` is what Task 5's `apply` step calls to answer "which file
//! holds volume X" before copying bytes onto the user's system — a wrong
//! answer there installs the wrong disk under the right name, silently.
//! Picking "the first one found" would make that decision depend on
//! directory read order, which is not guaranteed by any filesystem this
//! runs on and is exactly the kind of thing that works on the developer's
//! machine and not the user's. So `find_media` refuses outright the moment
//! a second file claims a name already seen, whether or not the two files
//! are byte-identical: proving they are identical would mean hashing every
//! duplicate pair, for a benefit that does not matter to the caller (it
//! still only gets to keep one path), and does not generalise to the case
//! that actually matters — two *different* revisions sharing a label. A
//! human deciding which file is the real `Workbench3.2` is a five-second
//! question; ART guessing wrong is a corrupted install nobody asked for.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::core::error::{CoreError, CoreResult};

use super::source::{AdfSource, MediaSource};

/// One piece of install media [`find_media`] opened successfully.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FoundMedia {
    pub path: PathBuf,
    /// The volume name read from **inside** the image — never derived from
    /// `path`.
    pub volume_name: String,
}

/// Every install disk found directly inside `folder`.
///
/// Opens each regular file one directory level deep (no recursion, no
/// symlinks followed) and keeps the ones that open as a single bare
/// AmigaDOS volume; anything else in the folder is skipped, not reported as
/// an error — see the module doc. Refuses the whole scan, rather than
/// picking one, the moment two files carry the same volume name.
pub fn find_media(folder: &Path) -> CoreResult<Vec<FoundMedia>> {
    let mut entries: Vec<PathBuf> = std::fs::read_dir(folder)?
        .map(|entry| entry.map(|e| e.path()))
        .collect::<std::io::Result<_>>()?;
    // Deterministic order: a directory listing's own order is not guaranteed
    // by any filesystem this runs on, and the duplicate-name refusal below
    // should not depend on it either — sorted by path, so which file is
    // named "existing" in that error is stable from run to run.
    entries.sort();

    let mut found: Vec<FoundMedia> = Vec::new();
    let mut by_name: HashMap<String, PathBuf> = HashMap::new();

    for path in entries {
        // `symlink_metadata`, not `metadata`: a symlink must never be
        // followed, whether it points at a file, a directory, or nothing at
        // all. Its own file type is never `is_file()`, so this one check
        // skips both a symlink and a subdirectory (no recursion) together.
        let metadata = std::fs::symlink_metadata(&path)?;
        if !metadata.is_file() {
            continue;
        }

        let Ok(source) = AdfSource::open(&path) else {
            // Not an Amiga volume at all, or a layout `open` refuses by
            // name (RDB, unrecognised signature, too small) — a normal
            // thing to find in a real media folder, not a scan failure.
            continue;
        };
        let volume_name = source.volume_name().to_string();

        if let Some(existing) = by_name.get(&volume_name) {
            return Err(CoreError::InvalidInput(format!(
                "'{}' and '{}' in '{}' both carry the volume name '{volume_name}' — \
                 ART cannot tell which one a component naming '{volume_name}' should \
                 read from",
                existing.display(),
                path.display(),
                folder.display(),
            )));
        }
        by_name.insert(volume_name.clone(), path.clone());
        found.push(FoundMedia { path, volume_name });
    }

    Ok(found)
}

/// The file in `found` that carries `volume_name`, or `None` when none does.
///
/// `find_media` already refuses a folder where two files claim the same
/// name, so the first (and only) match is the only one there can be.
pub fn media_for<'a>(found: &'a [FoundMedia], volume_name: &str) -> Option<&'a FoundMedia> {
    found.iter().find(|f| f.volume_name == volume_name)
}

// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::osinstall::fixtures::{media, scratch};

    #[test]
    fn media_is_found_by_its_volume_name_not_its_filename() {
        let dir = scratch("scan-by-volume-name");
        media(&dir, "Workbench3.2", "wb.adf", &[]);
        // The point of the task: a disk that reached this folder under a
        // name that says nothing about what is on it still has to resolve.
        media(&dir, "Extras3.2", "totally-unrelated-name.bin", &[]);

        let found = find_media(&dir).unwrap();
        assert!(media_for(&found, "Extras3.2").is_some());
        assert!(media_for(&found, "Workbench3.2").is_some());
    }

    #[test]
    fn a_file_that_is_not_an_amiga_image_is_skipped_not_an_error() {
        let dir = scratch("scan-skip-non-amiga");
        std::fs::write(dir.join("readme.txt"), b"hello").unwrap();
        media(&dir, "Workbench3.2", "wb.adf", &[]);

        let found = find_media(&dir).unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].volume_name, "Workbench3.2");
    }

    /// The user's own 3.2 folder holds 36 ADFs; a scan must not descend into
    /// a subdirectory looking for more, nor follow a symlink into one.
    #[test]
    fn the_scan_is_not_recursive() {
        let dir = scratch("scan-not-recursive");
        let nested = dir.join("sub");
        std::fs::create_dir(&nested).unwrap();
        media(&nested, "Workbench3.2", "wb.adf", &[]);

        assert!(find_media(&dir).unwrap().is_empty());
    }

    #[test]
    fn duplicate_volume_names_are_refused_not_silently_resolved() {
        let dir = scratch("scan-duplicate-names");
        media(&dir, "Workbench3.2", "wb-copy-1.adf", &[]);
        media(&dir, "Workbench3.2", "wb-copy-2.adf", &[]);

        let err = find_media(&dir).unwrap_err();
        assert!(
            err.to_string().contains("Workbench3.2"),
            "expected the shared volume name in the refusal: {err}"
        );
    }

    #[test]
    fn media_for_returns_none_when_no_file_carries_the_name() {
        let dir = scratch("scan-media-for-missing");
        media(&dir, "Workbench3.2", "wb.adf", &[]);

        let found = find_media(&dir).unwrap();
        assert!(media_for(&found, "Extras3.2").is_none());
    }
}
