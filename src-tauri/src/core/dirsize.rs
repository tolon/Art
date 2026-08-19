//! How big a drawer really is (ART-087, brief §3.2).
//!
//! Total Commander's `CountSpace=1`: Space on a **directory** marks it *and*
//! walks it, replacing the `<DIR>` in the Size column with the real total.
//! ART marked and did not count, because there was no primitive to count with
//! — `panel_list_local` lists one level, `scan_collection_directory` looks for
//! Amiga files rather than totalling bytes, and `volume_plan_copy` computes a
//! size only against a destination volume.
//!
//! This is that primitive, on both sides of the file manager's fence: a host
//! folder and a directory inside an Amiga volume. One module rather than two
//! because the *answer* is the same shape whichever side asked, and a caller
//! showing it in one column should not have to reconcile two.
//!
//! ## A total that stopped short says so
//!
//! Both walks are bounded — a symlink cycle plus unbounded recursion
//! overflows the stack, and the release profile sets `panic = "abort"`, so
//! that takes the whole application down rather than reporting an error. When
//! a walk hits the cap, [`DirTotal::partial`] is set and the number becomes a
//! **floor**, not a total. That distinction is the whole reason the field
//! exists: a size column showing `1.2 GB` where the truth is `40 GB` is worse
//! than showing nothing, and this is exactly the silence ART-107 was about on
//! the layout side.
//!
//! ## Cancellable, and checked between whole entries
//!
//! A drawer of forty thousand files must not block the command thread and
//! must be stoppable (§54, §55), so both take a [`ProgressSink`] and check
//! `is_cancelled` between entries. Nothing here writes, so stopping loses
//! only the count.

use std::path::Path;

use serde::Serialize;

use crate::core::error::{CoreError, CoreResult};
use crate::core::jobs::ProgressSink;
use crate::core::volume::BlockDevice;

/// How deep either walk will descend.
///
/// The same cap and the same reason as `core/collection` and
/// `core/layout/scan`.
pub const MAX_TOTAL_DEPTH: usize = 32;

/// What a drawer holds, once counted.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DirTotal {
    /// The sum of every file's size beneath the directory. A **floor** rather
    /// than a total whenever [`partial`](Self::partial) is set.
    pub bytes: u64,
    pub files: u64,
    pub directories: u64,
    /// True when the walk stopped at [`MAX_TOTAL_DEPTH`] somewhere, or could
    /// not read a directory it met. The number is then the least the drawer
    /// holds, and a caller must not print it as if it were the answer.
    pub partial: bool,
}

impl DirTotal {
    fn add_file(&mut self, bytes: u64) {
        // `saturating_add`, not `+`: a size can come from a directory block on
        // an image ART did not write, and a total that wraps is a total that
        // lies. `core::layout::plan` folds its own bytes the same way.
        self.bytes = self.bytes.saturating_add(bytes);
        self.files = self.files.saturating_add(1);
    }
}

/// Count a host folder.
///
/// Symlinks are reported by not being followed — `symlink_metadata` is what
/// keeps a junction pointing back up its own tree from making the walk
/// infinite, the ART-028 rule every other walk in this codebase follows. A
/// link is not counted at all rather than counted as its target: counting the
/// target would double a folder that links to its own subfolder, and counting
/// the link's own size would be a number about the filesystem rather than
/// about the content.
pub fn host_total(dir: &Path, sink: &dyn ProgressSink) -> CoreResult<DirTotal> {
    if !dir.is_dir() {
        return Err(CoreError::InvalidInput(format!(
            "'{}' is not a folder",
            dir.display()
        )));
    }
    let mut total = DirTotal::default();
    host_walk(dir, 0, &mut total, sink)?;
    Ok(total)
}

fn host_walk(
    dir: &Path,
    depth: usize,
    total: &mut DirTotal,
    sink: &dyn ProgressSink,
) -> CoreResult<()> {
    if depth >= MAX_TOTAL_DEPTH {
        total.partial = true;
        return Ok(());
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        // A folder ART is not allowed to open is one it cannot count. That is
        // a partial answer, not a failure of the whole count: refusing the
        // whole drawer because one subfolder is locked would be less useful
        // than a floor that says it is one.
        total.partial = true;
        return Ok(());
    };

    for entry in entries.flatten() {
        // Between whole entries, never mid-read.
        if sink.is_cancelled() {
            return Err(CoreError::Cancelled);
        }
        let path = entry.path();
        let Ok(meta) = std::fs::symlink_metadata(&path) else {
            total.partial = true;
            continue;
        };
        let kind = meta.file_type();
        if kind.is_symlink() {
            continue;
        }
        if kind.is_dir() {
            total.directories = total.directories.saturating_add(1);
            sink.report(total.files, None, &path.to_string_lossy());
            host_walk(&path, depth + 1, total, sink)?;
        } else if kind.is_file() {
            total.add_file(meta.len());
        }
    }
    Ok(())
}

/// Count a directory inside an Amiga volume.
///
/// `dir_block` is the directory's header block — the same number
/// `commands::volume::volume_list` already hands the pane for every row, so
/// the caller needs nothing it does not already have.
///
/// Reads only: this walks the same `list_directory_on` the pane lists with,
/// which is a different code path from anything that writes.
pub fn volume_total(
    device: &dyn BlockDevice,
    dir_block: u32,
    sink: &dyn ProgressSink,
) -> CoreResult<DirTotal> {
    let mut total = DirTotal::default();
    volume_walk(device, dir_block, 0, &mut total, sink)?;
    Ok(total)
}

fn volume_walk(
    device: &dyn BlockDevice,
    dir_block: u32,
    depth: usize,
    total: &mut DirTotal,
    sink: &dyn ProgressSink,
) -> CoreResult<()> {
    if depth >= MAX_TOTAL_DEPTH {
        total.partial = true;
        return Ok(());
    }

    // A malformed image can point a directory at itself or at a block that is
    // not a directory at all. `list_directory_on` bounds its own chain walks
    // and returns an error rather than looping, and an error on one drawer is
    // the same "partial, not failed" answer a locked host folder gets: the
    // rest of the tree is still worth counting, and the flag says the number
    // is a floor.
    let Ok(entries) = crate::core::adf::fs::list_directory_on(device, dir_block) else {
        total.partial = true;
        return Ok(());
    };

    for entry in entries {
        if sink.is_cancelled() {
            return Err(CoreError::Cancelled);
        }
        if entry.kind == crate::core::adf::blocks::EntryKind::Directory {
            total.directories = total.directories.saturating_add(1);
            sink.report(total.files, None, &entry.name);
            volume_walk(device, entry.header_block, depth + 1, total, sink)?;
        } else {
            total.add_file(entry.byte_size);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::jobs::NoProgress;
    use std::path::PathBuf;

    fn scratch(tag: &str) -> PathBuf {
        use std::sync::atomic::{AtomicU64, Ordering};
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let dir = std::env::temp_dir().join(format!(
            "art-dirsize-{tag}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn a_folder_totals_every_file_beneath_it() {
        let dir = scratch("host");
        std::fs::create_dir_all(dir.join("sub").join("deeper")).unwrap();
        std::fs::write(dir.join("a"), vec![0u8; 10]).unwrap();
        std::fs::write(dir.join("sub").join("b"), vec![0u8; 20]).unwrap();
        std::fs::write(dir.join("sub").join("deeper").join("c"), vec![0u8; 30]).unwrap();

        let total = host_total(&dir, &NoProgress).unwrap();
        assert_eq!(total.bytes, 60);
        assert_eq!(total.files, 3);
        assert_eq!(total.directories, 2);
        assert!(!total.partial, "nothing was skipped");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_empty_folder_is_zero_and_not_partial() {
        let dir = scratch("empty");
        let total = host_total(&dir, &NoProgress).unwrap();
        assert_eq!(total, DirTotal::default());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A total that stopped short must say so, or the Size column prints a
    /// number that is quietly far too small — the same silence ART-107 was.
    #[test]
    fn a_tree_deeper_than_the_cap_reports_a_floor_not_a_total() {
        let root = scratch("deep");
        let mut deep = root.clone();
        for i in 0..(MAX_TOTAL_DEPTH * 2) {
            deep = deep.join(format!("d{i}"));
        }
        std::fs::create_dir_all(&deep).unwrap();
        std::fs::write(deep.join("buried"), vec![0u8; 1000]).unwrap();
        std::fs::write(root.join("shallow"), vec![0u8; 7]).unwrap();

        let total = host_total(&root, &NoProgress).unwrap();
        assert!(total.partial, "the cap was hit and must be admitted");
        assert_eq!(
            total.bytes, 7,
            "the buried 1000 bytes are past the cap, so the number is a floor"
        );

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_file_is_not_a_folder() {
        let dir = scratch("notdir");
        let file = dir.join("one");
        std::fs::write(&file, b"x").unwrap();
        assert!(matches!(
            host_total(&file, &NoProgress),
            Err(CoreError::InvalidInput(_))
        ));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Cancelling is a `Cancelled`, not a half-answer dressed up as a total.
    #[test]
    fn cancelling_stops_rather_than_returning_a_short_number() {
        struct StopAtOnce;
        impl ProgressSink for StopAtOnce {
            fn report(&self, _: u64, _: Option<u64>, _: &str) {}
            fn is_cancelled(&self) -> bool {
                true
            }
        }

        let dir = scratch("cancel");
        std::fs::write(dir.join("a"), vec![0u8; 10]).unwrap();

        assert!(matches!(
            host_total(&dir, &StopAtOnce),
            Err(CoreError::Cancelled)
        ));

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The volume side, over a volume ART builds here — no shipped Amiga
    /// content, and a real FFS tree rather than a stand-in for one.
    #[test]
    fn a_volume_directory_totals_its_whole_tree() {
        use crate::core::volume::device::FileRegionMut;
        use crate::core::volume::fixture::ffs_volume;
        use crate::core::volume::write::{FileMeta, VolumeWriter};
        use crate::core::volume::DosType;

        let dir = scratch("volume");
        let path = dir.join("disk.adf");
        let (bytes, geometry) = ffs_volume(1760, DosType::new(*b"DOS"));
        std::fs::write(&path, &bytes).unwrap();

        let root = geometry.root_block;
        let sub = {
            let mut device =
                FileRegionMut::open(&path, 0, geometry.total_bytes(), geometry.block_size).unwrap();
            let mut writer = VolumeWriter::open(&mut device, geometry, &path, 0).unwrap();
            writer
                .add_file(root, "top", &[0u8; 100], FileMeta::default())
                .unwrap();
            let made = writer.make_dir(root, "Games").unwrap();
            let sub = made.block.expect("a new drawer has a header block");
            writer
                .add_file(sub, "inner", &[0u8; 250], FileMeta::default())
                .unwrap();
            sub
        };

        let device =
            FileRegionMut::open(&path, 0, geometry.total_bytes(), geometry.block_size).unwrap();

        let total = volume_total(&device, root, &NoProgress).unwrap();
        assert_eq!(total.bytes, 350, "both files, at every depth");
        assert_eq!(total.files, 2);
        assert_eq!(total.directories, 1);
        assert!(!total.partial);

        // And a subdirectory counts only itself — the number the Size column
        // replaces `<DIR>` with is about that row, not about the volume.
        let inner = volume_total(&device, sub, &NoProgress).unwrap();
        assert_eq!(inner.bytes, 250);
        assert_eq!(inner.files, 1);
        assert_eq!(inner.directories, 0);

        drop(device);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A block that is not a directory is a floor of zero, not a panic and not
    /// an error that loses the rest of a tree — a corrupt image must not take
    /// the Size column down with it.
    #[test]
    fn a_block_that_is_not_a_directory_is_partial_rather_than_fatal() {
        use crate::core::volume::device::FileRegionMut;
        use crate::core::volume::fixture::ffs_volume;
        use crate::core::volume::DosType;

        let dir = scratch("volume-bad");
        let path = dir.join("disk.adf");
        let (bytes, geometry) = ffs_volume(1760, DosType::new(*b"DOS"));
        std::fs::write(&path, &bytes).unwrap();

        let device =
            FileRegionMut::open(&path, 0, geometry.total_bytes(), geometry.block_size).unwrap();
        // Block 0 is the boot block, which is not a directory header.
        let total = volume_total(&device, 0, &NoProgress).unwrap();
        assert!(total.partial);
        assert_eq!(total.bytes, 0);

        drop(device);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
