//! Getting the floppies out of a `.rp9`.
//!
//! A `.rp9` package is a zip holding the title's disk images plus a manifest
//! ART reads separately (`core::artwork::local` reads the same archive for its
//! screenshot, through the same [`core::archive::open`](crate::core::archive::open)
//! gate). WinUAE cannot mount an image that is still inside the zip, so before
//! `core::launch`'s decision becomes a running emulator, the disks named in it
//! have to land on disk as ordinary files.
//!
//! **Why not [`core::archive::extract_selection`](crate::core::archive::extract::extract_selection).**
//! That gate answers "what did the archive hold, and what happened to each
//! entry" — the right shape for extracting an archive's contents. This is a
//! narrower question: "give me exactly these disks, in exactly this order, or
//! tell me which one is missing." A `LaunchPlan`'s `Floppies { images }` is an
//! ordered list WinUAE will mount as `floppy0`, `floppy1`, … — a silently
//! shorter result is a game that boots wrong, not a game that boots without
//! extras.
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
    let mut archive = open(package)?;
    let entries = archive.entries()?;

    // Resolve every wanted name to an index before writing anything, so a
    // package missing disk 3 of 4 fails before disks 1 and 2 land on disk.
    let mut indices = Vec::with_capacity(ordered.len());
    for name in ordered {
        let index = entries
            .iter()
            .position(|entry| {
                !entry.is_dir && entry.name.replace('\\', "/") == name.replace('\\', "/")
            })
            .ok_or_else(|| {
                CoreError::InvalidInput(format!("'{name}' is not in '{}'", package.display()))
            })?;
        indices.push(index);
    }

    let mut written = Vec::with_capacity(ordered.len());
    for (name, index) in ordered.iter().zip(indices) {
        let target =
            safe_join(into, name).map_err(|e| CoreError::InvalidInput(format!("'{name}': {e}")))?;

        let bytes = archive.read(index, MAX_FLOPPY_BYTES)?;

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
        let dir = std::env::temp_dir().join(format!("art-launch-{tag}-{}", std::process::id()));
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

        let err = unpack_floppies(&pkg, &["../../evil.adf".into()], &dir.join("out")).unwrap_err();

        assert!(matches!(err, CoreError::InvalidInput(_)), "{err:?}");
        assert!(!dir.join("evil.adf").exists());

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
}
