//! Materialising a staging tree.
//!
//! Three rules, and each has a test that fails without it: the source is never
//! modified, nothing overwrites, and a destination is untrusted text that goes
//! through `safe_join` like an archive entry name — the user types it, and a
//! `../` in a text box is the same hole a `../` in a zip is.
//!
//! Cancellation is checked **between items and never inside one** (§54), so
//! stopping leaves whole files behind and never half of one.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::core::error::{CoreError, CoreResult};
use crate::core::jobs::ProgressSink;
use crate::core::layout::{LayoutItem, LayoutPlan, Placement};
use crate::core::security::path::safe_join;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApplyOutcome {
    pub placed: usize,
    pub bytes: u64,
}

/// Build the staging tree the plan describes.
pub fn apply(plan: &LayoutPlan, sink: &dyn ProgressSink) -> CoreResult<ApplyOutcome> {
    let total = plan.items.len() as u64;
    let mut outcome = ApplyOutcome::default();

    for (done, item) in plan.items.iter().enumerate() {
        if sink.is_cancelled() {
            return Err(CoreError::Cancelled);
        }
        sink.report(done as u64, Some(total), &item.destination);

        outcome.bytes += place(&plan.root, item)?;
        outcome.placed += 1;
    }

    sink.report(total, Some(total), "done");
    Ok(outcome)
}

/// Copy one item into `root`, refusing to overwrite anything already there.
fn place(root: &Path, item: &LayoutItem) -> CoreResult<u64> {
    let target = safe_join(root, &item.destination).map_err(|err| {
        CoreError::SafetyRefused(format!(
            "'{}' does not stay inside the staging folder: {err}",
            item.destination
        ))
    })?;
    if target.exists() {
        return Err(CoreError::InvalidInput(format!(
            "'{}' is already there; nothing is overwritten",
            item.destination
        )));
    }
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent)?;
    }

    match item.placement {
        Placement::CopyFile => Ok(std::fs::copy(&item.source, &target)?),
        Placement::CopyTree => copy_tree(&item.source, &target),
        Placement::UnpackWhdload => Err(CoreError::InvalidInput(
            "unpacking a WHDLoad archive is not implemented yet".into(),
        )),
    }
}

/// Copy `from` to `to` recursively, creating nothing that is already there.
fn copy_tree(from: &Path, to: &Path) -> CoreResult<u64> {
    std::fs::create_dir_all(to)?;
    let mut bytes = 0;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let source = entry.path();
        if std::fs::symlink_metadata(&source)
            .map(|m| m.file_type().is_symlink())
            .unwrap_or(false)
        {
            continue;
        }
        let target: PathBuf = to.join(entry.file_name());
        if source.is_dir() {
            bytes += copy_tree(&source, &target)?;
        } else {
            bytes += std::fs::copy(&source, &target)?;
        }
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::jobs::NoProgress;
    use crate::core::layout::{ItemKind, LayoutItem, LayoutPlan, Placement};

    fn scratch(tag: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("art-layout-apply-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn plan_of(root: &Path, items: Vec<LayoutItem>) -> LayoutPlan {
        LayoutPlan {
            root: root.to_path_buf(),
            bytes: items.iter().map(|i| i.bytes).sum(),
            items,
            refused: Vec::new(),
            collisions: Vec::new(),
        }
    }

    #[test]
    fn a_file_lands_at_its_destination_and_the_source_is_untouched() {
        let dir = scratch("file");
        let root = dir.join("staging");
        let source = dir.join("Disk.adf");
        std::fs::write(&source, b"disk bytes").unwrap();

        let plan = plan_of(
            &root,
            vec![LayoutItem {
                source: source.clone(),
                kind: ItemKind::FloppyImage,
                destination: "Floppies/Disk.adf".into(),
                placement: Placement::CopyFile,
                bytes: 10,
            }],
        );

        let outcome = apply(&plan, &NoProgress).unwrap();

        assert_eq!(outcome.placed, 1);
        assert_eq!(outcome.bytes, 10);
        assert_eq!(
            std::fs::read(root.join("Floppies").join("Disk.adf")).unwrap(),
            b"disk bytes"
        );
        assert_eq!(
            std::fs::read(&source).unwrap(),
            b"disk bytes",
            "the source is never modified"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_drawer_lands_with_its_whole_tree() {
        let dir = scratch("tree");
        let root = dir.join("staging");
        let game = dir.join("Zool");
        std::fs::create_dir_all(game.join("data")).unwrap();
        std::fs::write(game.join("Zool.slave"), b"slave").unwrap();
        std::fs::write(game.join("data").join("level1"), b"level").unwrap();

        let plan = plan_of(
            &root,
            vec![LayoutItem {
                source: game.clone(),
                kind: ItemKind::WhdloadDrawer {
                    name: "Zool".into(),
                },
                destination: "Games/Zool".into(),
                placement: Placement::CopyTree,
                bytes: 10,
            }],
        );

        apply(&plan, &NoProgress).unwrap();

        assert_eq!(
            std::fs::read(root.join("Games").join("Zool").join("Zool.slave")).unwrap(),
            b"slave"
        );
        assert_eq!(
            std::fs::read(root.join("Games").join("Zool").join("data").join("level1")).unwrap(),
            b"level"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// **Nothing overwrites.** The plan reports collisions; if one slips
    /// through — the tree changed between preview and apply — the applier
    /// refuses rather than replacing the user's file.
    #[test]
    fn an_existing_destination_is_refused_and_left_alone() {
        let dir = scratch("exists");
        let root = dir.join("staging");
        std::fs::create_dir_all(root.join("Floppies")).unwrap();
        std::fs::write(root.join("Floppies").join("Disk.adf"), b"already here").unwrap();
        let source = dir.join("Disk.adf");
        std::fs::write(&source, b"new bytes").unwrap();

        let plan = plan_of(
            &root,
            vec![LayoutItem {
                source,
                kind: ItemKind::FloppyImage,
                destination: "Floppies/Disk.adf".into(),
                placement: Placement::CopyFile,
                bytes: 9,
            }],
        );

        let err = apply(&plan, &NoProgress).unwrap_err();
        assert!(err.to_string().contains("already"), "{err}");
        assert_eq!(
            std::fs::read(root.join("Floppies").join("Disk.adf")).unwrap(),
            b"already here",
            "the file that was there is still there"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A destination that climbs out of the staging root is refused. The
    /// destination is user-editable text, so it is untrusted input like an
    /// archive entry name, and goes through the same gate.
    #[test]
    fn a_destination_that_escapes_the_root_is_refused() {
        let dir = scratch("escape");
        let root = dir.join("staging");
        let source = dir.join("Disk.adf");
        std::fs::write(&source, b"x").unwrap();

        for bad in ["../outside/Disk.adf", "C:/Windows/Disk.adf"] {
            let plan = plan_of(
                &root,
                vec![LayoutItem {
                    source: source.clone(),
                    kind: ItemKind::FloppyImage,
                    destination: bad.into(),
                    placement: Placement::CopyFile,
                    bytes: 1,
                }],
            );
            assert!(apply(&plan, &NoProgress).is_err(), "{bad} was allowed");
        }
        assert!(!dir.join("outside").exists());

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A sink that says stop once `after` items have been reported.
    ///
    /// Atomics rather than a `Cell` because `ProgressSink` is `Send + Sync`.
    struct StopAfter {
        seen: std::sync::atomic::AtomicU64,
        after: u64,
    }

    impl ProgressSink for StopAfter {
        fn report(&self, _done: u64, _total: Option<u64>, _label: &str) {
            self.seen.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        }
        fn is_cancelled(&self) -> bool {
            self.seen.load(std::sync::atomic::Ordering::SeqCst) >= self.after
        }
    }

    /// **Cancelling leaves whole files, never half of one** (§54). The check
    /// sits between items, so the first is complete and the second was never
    /// begun.
    #[test]
    fn stopping_leaves_the_finished_item_whole_and_the_next_one_absent() {
        let dir = scratch("cancel");
        let root = dir.join("staging");
        std::fs::write(dir.join("A.adf"), b"first").unwrap();
        std::fs::write(dir.join("B.adf"), b"second").unwrap();

        let plan = plan_of(
            &root,
            vec![
                LayoutItem {
                    source: dir.join("A.adf"),
                    kind: ItemKind::FloppyImage,
                    destination: "Floppies/A.adf".into(),
                    placement: Placement::CopyFile,
                    bytes: 5,
                },
                LayoutItem {
                    source: dir.join("B.adf"),
                    kind: ItemKind::FloppyImage,
                    destination: "Floppies/B.adf".into(),
                    placement: Placement::CopyFile,
                    bytes: 6,
                },
            ],
        );

        // Not cancelled for item 0, cancelled by the time item 1 is reached.
        let sink = StopAfter {
            seen: std::sync::atomic::AtomicU64::new(0),
            after: 1,
        };
        let err = apply(&plan, &sink).unwrap_err();

        assert_eq!(err.code(), "ART-CANCELLED", "{err}");
        assert_eq!(
            std::fs::read(root.join("Floppies").join("A.adf")).unwrap(),
            b"first",
            "the item that was begun is finished"
        );
        assert!(
            !root.join("Floppies").join("B.adf").exists(),
            "the next item was never begun"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }
}
