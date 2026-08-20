//! Getting a title's own media out of a `.rp9`.
//!
//! A `.rp9` package is a zip holding the title's disk images — or, for a
//! hardfile-based title, one whole hard disk image — plus a manifest ART
//! reads separately (`core::artwork::local` reads the same archive for its
//! screenshot, through the same [`core::archive::open`](crate::core::archive::open)
//! gate). WinUAE cannot mount an image that is still inside the zip, so before
//! `core::launch`'s decision becomes a running emulator, the media named in it
//! has to land on disk as an ordinary file.
//!
//! **Why not [`core::archive::extract_selection`](crate::core::archive::extract::extract_selection).**
//! That gate answers "what did the archive hold, and what happened to each
//! entry" — the right shape for extracting an archive's contents. This is a
//! narrower question: "give me exactly this media, or tell me it is missing."
//! A `LaunchPlan`'s `Floppies { images }` is an ordered list WinUAE will mount
//! as `floppy0`, `floppy1`, … — a silently shorter result is a game that boots
//! wrong, not a game that boots without extras — and a `Hardfile { image }`
//! mounted from the zip itself rather than what is inside it is not a
//! shorter result at all, it is the wrong file entirely (the bug this module
//! exists to not repeat).
use std::path::{Path, PathBuf};

use crate::core::archive::open;
use crate::core::error::{CoreError, CoreResult};
use crate::core::safety::atomic_write;
use crate::core::security::path::safe_join;

/// A disk image is a few hundred kilobytes to a few megabytes. Eight is far
/// above any real Amiga floppy format and far below anything that could
/// exhaust memory — the same reasoning `MAX_PREVIEW_BYTES` uses one module
/// over for a screenshot.
pub const MAX_FLOPPY_BYTES: u64 = 8 * 1024 * 1024;

/// A hardfile is a whole installed game, not a floppy image — real ones in
/// this collection run to tens of megabytes. The same ceiling
/// `core::gameindex::scan`'s `MAX_TITLE_BYTES` already treats as "a single
/// catalogued title", so nothing this module extracts can be larger than
/// what the catalogue would have indexed as one title to begin with.
pub const MAX_HARDFILE_BYTES: u64 = 512 * 1024 * 1024;

/// Unpack `ordered`'s entries out of `package` and into `into`, in the order
/// given.
///
/// Every name in `ordered` must be present in the archive and must survive
/// [`safe_join`] against `into` — either failure refuses the whole call with
/// [`CoreError::InvalidInput`] naming the entry, rather than returning a
/// shorter list. A `LaunchKind::Floppies` half-unpacked is not something to
/// hand to WinUAE.
pub fn unpack_floppies(
    package: &Path,
    ordered: &[String],
    into: &Path,
) -> CoreResult<Vec<PathBuf>> {
    unpack_named(package, ordered, into, MAX_FLOPPY_BYTES)
}

/// Unpack the one entry `name` out of `package` and into `into`, returning
/// its path.
///
/// The `Hardfile` counterpart of [`unpack_floppies`]. A `.rp9` whose media is
/// `Media::Hardfile { file }` (`core::gameindex::record`) carries the whole
/// hard disk image as a single named zip entry — `core::gameindex::readers::
/// rp9` reads it out of `<harddrive>`, e.g. `af-application.hdf` — and WinUAE
/// cannot mount it any more than a floppy while it is still inside the
/// archive. Mounting the `.rp9` itself instead of what this function
/// extracts was ART-141.
///
/// **Reuses an already-extracted copy rather than overwriting it.** A
/// hardfile-based title's save lives inside the hardfile itself (WHDLoad and
/// most AGA-era installers alike), and `into` is per-title
/// (`commands/launch.rs::launch_dir_for`), so a second launch finding the
/// same target already there is the *second session of the same title*, not
/// a stale leftover — re-extracting would silently discard whatever the
/// first session saved, which is the launcher failure this wave's own words
/// call out. If the `.rp9` package changes on disk, the stale copy is still
/// preferred; nothing here compares the two.
pub fn unpack_hardfile(package: &Path, name: &str, into: &Path) -> CoreResult<PathBuf> {
    let target =
        safe_join(into, name).map_err(|e| CoreError::InvalidInput(format!("'{name}': {e}")))?;
    if target.is_file() {
        return Ok(target);
    }

    let written = unpack_named(
        package,
        std::slice::from_ref(&name.to_string()),
        into,
        MAX_HARDFILE_BYTES,
    )?;
    written.into_iter().next().ok_or_else(|| {
        CoreError::InvalidInput(format!(
            "'{name}' did not unpack from '{}'",
            package.display()
        ))
    })
}

/// The shared body of [`unpack_floppies`] and [`unpack_hardfile`]: resolve
/// every wanted name to an archive index *and* a validated destination
/// before writing anything (so a package missing one of several wanted
/// entries, or naming one that escapes `into`, fails before any of the
/// others land on disk), then read each one bounded by `max_bytes` and write
/// it through [`atomic_write`].
fn unpack_named(
    package: &Path,
    ordered: &[String],
    into: &Path,
    max_bytes: u64,
) -> CoreResult<Vec<PathBuf>> {
    let mut archive = open(package)?;
    let entries = archive.entries()?;

    let mut resolved = Vec::with_capacity(ordered.len());
    for name in ordered {
        let index = entries
            .iter()
            .position(|entry| {
                !entry.is_dir && entry.name.replace('\\', "/") == name.replace('\\', "/")
            })
            .ok_or_else(|| {
                CoreError::InvalidInput(format!("'{name}' is not in '{}'", package.display()))
            })?;
        let target =
            safe_join(into, name).map_err(|e| CoreError::InvalidInput(format!("'{name}': {e}")))?;
        resolved.push((index, target));
    }

    let mut written = Vec::with_capacity(ordered.len());
    for (index, target) in resolved {
        let bytes = archive.read(index, max_bytes)?;

        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)?;
        }
        atomic_write(&target, &bytes)?;
        written.push(target);
    }

    Ok(written)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A `.rp9` is a zip. These entries are the disks (or, in the traversal
    /// test, an entry pretending to be one).
    fn package(dir: &Path, name: &str, entries: &[(&str, &[u8])]) -> PathBuf {
        use std::io::Write;
        let path = dir.join(name);
        let file = std::fs::File::create(&path).unwrap();
        let mut zip = zip::ZipWriter::new(file);
        for (entry, bytes) in entries {
            zip.start_file(*entry, zip::write::SimpleFileOptions::default())
                .unwrap();
            zip.write_all(bytes).unwrap();
        }
        zip.finish().unwrap();
        path
    }

    fn scratch(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "art-launch-{tag}-{}",
            crate::core::test_scratch_id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn the_disks_come_out_in_the_order_the_manifest_gave() {
        let dir = scratch("unpack");
        let pkg = package(
            &dir,
            "Dune2.rp9",
            &[("b.adf", b"SECOND"), ("a.adf", b"FIRST")],
        );

        let written =
            unpack_floppies(&pkg, &["a.adf".into(), "b.adf".into()], &dir.join("out")).unwrap();

        assert_eq!(written.len(), 2);
        assert_eq!(std::fs::read(&written[0]).unwrap(), b"FIRST");
        assert_eq!(std::fs::read(&written[1]).unwrap(), b"SECOND");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_entry_that_escapes_the_destination_is_refused() {
        let dir = scratch("unpack-traversal");
        let pkg = package(&dir, "Evil.rp9", &[("../../evil.adf", b"NOPE")]);

        // **The destination is two levels down on purpose (ART-144 #5).**
        // This test used to unpack into `dir/out` and then assert
        // `!dir.join("evil.adf").exists()` — but `../../` from `dir/out`
        // resolves to `dir`'s *parent*, so that assertion looked in a place
        // an unguarded join would never have written to. It could not fail,
        // whatever the code did; the test bit only because `.unwrap_err()`
        // panicked first. Two levels down means the escape lands at
        // `escaped` below, inside this test's own scratch directory, which
        // is both checkable and private to this run.
        let out = dir.join("out").join("disks");
        let escaped = dir.join("evil.adf");

        let result = unpack_floppies(&pkg, &["../../evil.adf".into()], &out);

        // **The filesystem is asserted before the error is**, and that order
        // is the rest of ART-144 #5. `unwrap_err()` on the first line would
        // panic the moment the guard came out, so every assertion after it
        // was unreachable — the test could only ever fail for one reason,
        // and it was not the one it claims to check. Asking the disk first
        // means removing `safe_join` fails *this* line, naming the file it
        // wrote and where.
        assert!(
            !escaped.exists(),
            "an unguarded join writes exactly here: {}",
            escaped.display()
        );
        // And nothing landed at the legitimate destination either — the
        // refusal happens before any write, not after a partial one.
        assert!(!out.join("evil.adf").exists());

        let err = result.unwrap_err();
        assert!(matches!(err, CoreError::InvalidInput(_)), "{err:?}");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_disk_the_package_does_not_carry_is_an_error_not_a_gap() {
        let dir = scratch("unpack-missing");
        let pkg = package(&dir, "Half.rp9", &[("a.adf", b"FIRST")]);

        assert!(
            unpack_floppies(&pkg, &["a.adf".into(), "b.adf".into()], &dir.join("out")).is_err()
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// ART-141. A `.rp9`'s hardfile is one named zip entry, the same shape a
    /// floppy set's entries are — the bug was mounting the zip itself.
    #[test]
    fn the_hardfile_comes_out_from_under_its_entry_name() {
        let dir = scratch("unpack-hardfile");
        let pkg = package(
            &dir,
            "Enzo.rp9",
            &[("af-application.hdf", b"NOT-A-ZIP-ANYMORE")],
        );

        let written = unpack_hardfile(&pkg, "af-application.hdf", &dir.join("out")).unwrap();

        assert_eq!(std::fs::read(&written).unwrap(), b"NOT-A-ZIP-ANYMORE");
        assert_ne!(
            written, pkg,
            "the extracted image, not the package it came from"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_hardfile_the_package_does_not_carry_is_an_error() {
        let dir = scratch("unpack-hardfile-missing");
        let pkg = package(&dir, "Empty.rp9", &[("readme.txt", b"nothing here")]);

        let err = unpack_hardfile(&pkg, "af-application.hdf", &dir.join("out")).unwrap_err();
        assert!(matches!(err, CoreError::InvalidInput(_)), "{err:?}");

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A hardfile entry may be well past the 8 MB floppy ceiling —
    /// `unpack_floppies` would refuse this; `unpack_hardfile` must not.
    /// A second launch of the same title must not discard whatever the first
    /// session saved inside the hardfile — the failure this wave's own words
    /// call out ("a launcher that silently discards a saved position is not
    /// a launcher").
    #[test]
    fn a_second_extraction_reuses_the_copy_already_there_instead_of_overwriting_it() {
        let dir = scratch("unpack-hardfile-reuse");
        let pkg = package(&dir, "Enzo.rp9", &[("af-application.hdf", b"PRISTINE")]);
        let out = dir.join("out");

        let first = unpack_hardfile(&pkg, "af-application.hdf", &out).unwrap();
        assert_eq!(std::fs::read(&first).unwrap(), b"PRISTINE");

        // Stand in for a WHDLoad save written during the first session.
        std::fs::write(&first, b"PRISTINE-PLUS-A-SAVED-GAME").unwrap();

        let second = unpack_hardfile(&pkg, "af-application.hdf", &out).unwrap();
        assert_eq!(second, first);
        assert_eq!(
            std::fs::read(&second).unwrap(),
            b"PRISTINE-PLUS-A-SAVED-GAME",
            "the second launch must not re-extract over a saved game"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_hardfile_larger_than_a_floppy_ceiling_still_unpacks() {
        let dir = scratch("unpack-hardfile-large");
        let big = vec![0x42u8; (MAX_FLOPPY_BYTES + 1) as usize];
        let pkg = package(&dir, "Big.rp9", &[("game.hdf", &big)]);

        let written = unpack_hardfile(&pkg, "game.hdf", &dir.join("out")).unwrap();
        assert_eq!(std::fs::read(&written).unwrap().len(), big.len());

        let _ = std::fs::remove_dir_all(&dir);
    }
}
