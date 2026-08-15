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
//! subdirectory. This is the same, undemonstrated-by-a-test choice
//! `core::layout::scan` and `core::collection` already make for the same
//! reason: creating a symlink to prove it is skipped needs privileges CI
//! does not have on this project's own Windows runner.
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
//! floppy image — opening one was never going to avoid the read for this
//! case, only for something bigger, and install media never is. Scanning
//! the user's real 36-disk folder therefore reads roughly 36 x 880 KB,
//! about 31 MB, off disk — a few milliseconds of sequential I/O on any
//! storage this runs on, not the multi-second scan a first guess might
//! fear. Said plainly, and checked rather than assumed, because the
//! alternative — trusting that "windowed" means "cheap" without reading
//! `scan_image` — is exactly the kind of thing that turns into a real
//! complaint the first time someone points ART at a folder of images
//! bigger than a floppy.
//!
//! ## Duplicate volume names are data, not a scan failure
//!
//! Two files in one folder can carry the same volume name: a stray backup
//! copy of one disk, or two different revisions of "the same" disk that
//! happen to share a label. Silently keeping the first one found is the
//! wrong call — [`media_for`] is what `apply` (a later task) calls to
//! answer "which file holds volume X" before copying bytes onto the user's
//! system, and a wrong guess there installs the wrong disk under the right
//! name. But failing the *whole folder scan* over one duplicate is too
//! blunt: a user with a stray copy of a Locale disk they will never select
//! could not install anything at all, not even an English-only run that
//! never touches it. So the ambiguity is reported as data, not raised as an
//! error here — [`find_media`] keeps every match, and [`media_for`] returns
//! [`MediaMatch::Ambiguous`] carrying every path that claimed the name. It
//! is the caller's job (`apply`, matching a component against its own
//! `Component::media`) to turn that into
//! [`super::RefusalReason::MediaAmbiguous`] for the one component actually
//! affected — the same shape `MediaMissing` and `MediaPathMissing` already
//! use, a named refusal for the thing that is actually broken, not a
//! blanket failure for the whole folder.

use std::path::{Path, PathBuf};

use crate::core::error::CoreResult;

use super::source::{AdfSource, MediaSource};

/// One piece of install media [`find_media`] opened successfully.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FoundMedia {
    pub path: PathBuf,
    /// The volume name read from **inside** the image — never derived from
    /// `path`.
    pub volume_name: String,
}

/// Every install disk found directly inside `folder`, duplicates included.
///
/// Opens each regular file one directory level deep (no recursion, no
/// symlinks followed) and keeps the ones that open as a single bare
/// AmigaDOS volume; anything else in the folder is skipped, not reported as
/// an error — see the module doc. Two files sharing a volume name are both
/// kept: resolving the ambiguity, or refusing over it, is [`media_for`]'s
/// and its caller's job, not this function's — a folder-wide failure here
/// would refuse an install that never touches the duplicated disk.
pub fn find_media(folder: &Path) -> CoreResult<Vec<FoundMedia>> {
    let mut entries: Vec<PathBuf> = std::fs::read_dir(folder)?
        .map(|entry| entry.map(|e| e.path()))
        .collect::<std::io::Result<_>>()?;
    // Deterministic order: a directory listing's own order is not
    // guaranteed by any filesystem this runs on, and `MediaMatch::Ambiguous`
    // should not depend on it either — sorted by path, so the paths a
    // caller reports for an ambiguous name are stable from run to run.
    entries.sort();

    let mut found: Vec<FoundMedia> = Vec::new();

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
        found.push(FoundMedia { path, volume_name });
    }

    Ok(found)
}

/// What resolving a volume name against a scanned folder found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MediaMatch<'a> {
    /// No file in `found` carries this volume name.
    Missing,
    /// Exactly one file does — the ordinary case.
    Found(&'a FoundMedia),
    /// More than one file claims this volume name, in `find_media`'s own
    /// (sorted-path) order. The caller decides what to do — typically
    /// [`super::RefusalReason::MediaAmbiguous`] for the one component that
    /// actually names this volume.
    Ambiguous(Vec<&'a FoundMedia>),
}

/// Resolve `volume_name` against `found`.
///
/// Returns an enum rather than an `Option` so ambiguity cannot collapse
/// into an arbitrary "first match": a caller that only wanted `Option`'s
/// `Some`/`None` shape would still have to decide what to do when there are
/// two, and matching on [`MediaMatch`] forces that decision to be made
/// explicitly at the call site instead of being made implicitly, once, in
/// here.
pub fn media_for<'a>(found: &'a [FoundMedia], volume_name: &str) -> MediaMatch<'a> {
    let matches: Vec<&FoundMedia> = found
        .iter()
        .filter(|f| f.volume_name == volume_name)
        .collect();
    match matches.len() {
        0 => MediaMatch::Missing,
        1 => MediaMatch::Found(matches[0]),
        _ => MediaMatch::Ambiguous(matches),
    }
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
        assert!(matches!(
            media_for(&found, "Extras3.2"),
            MediaMatch::Found(_)
        ));
        assert!(matches!(
            media_for(&found, "Workbench3.2"),
            MediaMatch::Found(_)
        ));
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
    /// a subdirectory looking for more, nor follow a symlink into one (the
    /// symlink half is documented, not tested here — see the module doc).
    #[test]
    fn the_scan_is_not_recursive() {
        let dir = scratch("scan-not-recursive");
        let nested = dir.join("sub");
        std::fs::create_dir(&nested).unwrap();
        media(&nested, "Workbench3.2", "wb.adf", &[]);

        assert!(find_media(&dir).unwrap().is_empty());
    }

    /// Two files sharing a volume name must both come back from
    /// `find_media` — neither silently dropped — and `media_for` must
    /// report the ambiguity rather than picking one.
    #[test]
    fn two_same_named_files_are_both_reported_and_neither_is_silently_dropped() {
        let dir = scratch("scan-duplicate-names");
        let first = media(&dir, "Workbench3.2", "wb-copy-1.adf", &[]);
        let second = media(&dir, "Workbench3.2", "wb-copy-2.adf", &[]);

        let found = find_media(&dir).unwrap();
        let matching: Vec<&PathBuf> = found
            .iter()
            .filter(|f| f.volume_name == "Workbench3.2")
            .map(|f| &f.path)
            .collect();
        assert_eq!(
            matching,
            vec![&first, &second],
            "both copies must survive the scan"
        );

        match media_for(&found, "Workbench3.2") {
            MediaMatch::Ambiguous(paths) => {
                assert_eq!(
                    paths.len(),
                    2,
                    "both candidates must be reported: {paths:?}"
                );
            }
            other => panic!("expected Ambiguous, got {other:?}"),
        }
    }

    #[test]
    fn media_for_returns_missing_when_no_file_carries_the_name() {
        let dir = scratch("scan-media-for-missing");
        media(&dir, "Workbench3.2", "wb.adf", &[]);

        let found = find_media(&dir).unwrap();
        assert_eq!(media_for(&found, "Extras3.2"), MediaMatch::Missing);
    }
}
