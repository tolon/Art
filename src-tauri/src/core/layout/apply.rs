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
use crate::core::layout::scan::MAX_SCAN_DEPTH;
use crate::core::layout::{LayoutItem, LayoutPlan, Placement};
use crate::core::security::path::safe_join;
use crate::core::whdload::Entry;

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
            // Every item already handled is durably on disk before the next
            // is considered (§54), so a stop partway through has a real count
            // to report — bare `Cancelled` would say nothing landed when it
            // did (ART-058).
            return Err(if outcome.placed > 0 {
                CoreError::CancelledPartway {
                    files: outcome.placed as u64,
                }
            } else {
                CoreError::Cancelled
            });
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
        Placement::CopyTree => copy_tree(&item.source, &target, 0),
        Placement::UnpackWhdload => unpack_whdload(&item.source, &target),
    }
}

/// Unpack an archive holding a WHDLoad pack into `target`, which is the
/// drawer's own path — so the icon goes to `target`'s **parent**.
///
/// Everything decompressed goes through `core/archive`'s one gate first, into
/// a scratch directory, and only then is the pack's own shape worked out. The
/// two steps are separate because the gate's question is "is this archive
/// hostile" and `analyse`'s is "where is the game", and neither should be
/// asked in the other's terms.
fn unpack_whdload(archive: &Path, target: &Path) -> CoreResult<u64> {
    use crate::core::archive::extract::{extract_with_backend, OverwritePolicy};
    use crate::core::archive::open;
    use crate::core::jobs::NoProgress;
    use crate::core::whdload::analyse;

    // Unique per call, not just per destination: `target.with_extension(...)`
    // alone is deterministic, so a process that dies mid-unpack would leave a
    // directory that permanently blocks every later retry to this same
    // destination. The pid plus a counter rules that out — two unpacks in the
    // same process cannot collide, and neither can two processes.
    //
    // This is the same idea as `core::sources::install::Scratch`, deliberately
    // not that type: `Scratch` lives under `std::env::temp_dir()`, which is
    // very likely a different volume from the staging tree, turning every
    // drawer copy that follows into a cross-volume one for a multi-gigabyte
    // game. Staying beside `target` keeps the unpack and the copy on the same
    // disk.
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let scratch = target.with_extension(format!(
        "art-unpack-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    // With a unique name this should be unreachable — kept as the guard it
    // always was, in case something else really is sitting there.
    if scratch.exists() {
        return Err(CoreError::InvalidInput(format!(
            "'{}' is in the way of unpacking",
            scratch.display()
        )));
    }
    std::fs::create_dir_all(&scratch)?;

    let unpacked = (|| -> CoreResult<u64> {
        let mut backend = open(archive)?;
        extract_with_backend(&mut *backend, &scratch, OverwritePolicy::Skip, &NoProgress)?;

        let entries = walk_entries(&scratch, "", 0)?;
        let layout = analyse(&entries)?;

        // The drawer. `layout.root` is empty when the archive's own root is
        // the pack, in which case the scratch directory itself is the
        // drawer — and its icon (found via `find_icon`'s root fallback) sits
        // right there among the files `copy_tree` would otherwise sweep up.
        // Copying it along would land a second copy *inside* the drawer,
        // which is exactly the §82 failure this function exists to prevent.
        let drawer = if layout.root.is_empty() {
            scratch.clone()
        } else {
            safe_join_in_scratch(&scratch, &layout.root)?
        };
        let skip_from_drawer = if layout.root.is_empty() {
            layout
                .icon
                .as_ref()
                .map(|icon| safe_join_in_scratch(&scratch, icon))
                .transpose()?
        } else {
            None
        };
        let mut bytes = copy_tree_excluding(&drawer, target, skip_from_drawer.as_deref(), 0)?;

        // §82: beside the drawer, never inside it.
        if let Some(icon) = &layout.icon {
            let from = safe_join_in_scratch(&scratch, icon)?;
            if let Some(parent) = target.parent() {
                let to = parent.join(layout.icon_name());
                if !to.exists() {
                    bytes += std::fs::copy(&from, &to)?;
                }
            }
        }
        Ok(bytes)
    })();

    let _ = std::fs::remove_dir_all(&scratch);
    unpacked
}

/// `safe_join`, with the entry name that failed folded into the error — the
/// name comes from an archive `unpack_whdload` already extracted through
/// `core/archive`'s gate, but a path built from it is still checked here
/// rather than trusted on the strength of that alone.
fn safe_join_in_scratch(scratch: &Path, name: &str) -> CoreResult<PathBuf> {
    safe_join(scratch, name).map_err(|err| {
        CoreError::SafetyRefused(format!(
            "'{name}' does not stay inside the unpacked archive: {err}"
        ))
    })
}

/// The unpacked tree as the list of names `analyse` reads.
fn walk_entries(base: &Path, relative: &str, depth: usize) -> CoreResult<Vec<Entry>> {
    let mut out = Vec::new();
    if depth >= MAX_SCAN_DEPTH {
        return Ok(out);
    }
    let here = if relative.is_empty() {
        base.to_path_buf()
    } else {
        safe_join_in_scratch(base, relative)?
    };
    for entry in std::fs::read_dir(&here)? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().to_string();
        let child = if relative.is_empty() {
            name
        } else {
            format!("{relative}/{name}")
        };
        if entry.file_type()?.is_dir() {
            out.push(Entry::dir(&child));
            out.extend(walk_entries(base, &child, depth + 1)?);
        } else {
            out.push(Entry::file(&child));
        }
    }
    Ok(out)
}

/// Copy `from` to `to` recursively, creating nothing that is already there.
///
/// Bounded by the same `MAX_SCAN_DEPTH` as `scan.rs`, and for the same
/// reason: unbounded recursion overflows the stack, and with `panic =
/// "abort"` that takes the whole application down. `scan::tree_bytes`
/// answers the same question by returning `0` past the cap, which only makes
/// a displayed number wrong; silently stopping a *copy* would leave a game on
/// the card that looks placed but is missing everything below the cut, so
/// this refuses instead.
fn copy_tree(from: &Path, to: &Path, depth: usize) -> CoreResult<u64> {
    copy_tree_excluding(from, to, None, depth)
}

/// `copy_tree`, but a single path under `from` — the icon file that lives
/// beside a wrapper-less WHDLoad pack's drawer, when the drawer *is* `from`
/// — is left out of the copy rather than swept in with everything else
/// (§82: that file is placed beside the drawer separately, never inside it).
fn copy_tree_excluding(
    from: &Path,
    to: &Path,
    skip: Option<&Path>,
    depth: usize,
) -> CoreResult<u64> {
    if depth >= MAX_SCAN_DEPTH {
        return Err(CoreError::InvalidInput(format!(
            "'{}' is nested deeper than ART will copy (limit {MAX_SCAN_DEPTH})",
            from.display()
        )));
    }
    std::fs::create_dir_all(to)?;
    let mut bytes = 0;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let source = entry.path();
        if Some(source.as_path()) == skip {
            continue;
        }
        if std::fs::symlink_metadata(&source)
            .map(|m| m.file_type().is_symlink())
            .unwrap_or(false)
        {
            continue;
        }
        let target: PathBuf = to.join(entry.file_name());
        if source.is_dir() {
            bytes += copy_tree_excluding(&source, &target, skip, depth + 1)?;
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
                // Deliberately wrong: the plan's estimate. `ApplyOutcome.bytes`
                // is what was actually written, never an echo of this figure —
                // if the applier only forwarded `bytes`, this test would still
                // pass at 999, so it must measure the real copy instead.
                bytes: 999,
            }],
        );

        let outcome = apply(&plan, &NoProgress).unwrap();

        assert_eq!(outcome.placed, 1);
        assert_eq!(
            outcome.bytes, 10,
            "the real size copied, not the plan's estimate"
        );
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

    /// A tree deeper than `MAX_SCAN_DEPTH` is refused, not silently truncated.
    /// `scan::tree_bytes` returning `0` past the cap only makes a displayed
    /// number wrong; a copy that quietly stopped partway would leave a game
    /// on the card that looks placed but is missing everything below the cut
    /// — the exact failure this module exists to prevent.
    #[test]
    fn a_tree_deeper_than_the_scan_limit_is_refused_not_truncated() {
        let dir = scratch("too-deep");
        let root = dir.join("staging");
        let game = dir.join("Deep");
        std::fs::create_dir_all(&game).unwrap();

        let mut deep = game.clone();
        for i in 0..(MAX_SCAN_DEPTH + 2) {
            deep = deep.join(format!("d{i}"));
        }
        std::fs::create_dir_all(&deep).unwrap();
        std::fs::write(deep.join("buried.adf"), b"x").unwrap();

        let plan = plan_of(
            &root,
            vec![LayoutItem {
                source: game.clone(),
                kind: ItemKind::WhdloadDrawer {
                    name: "Deep".into(),
                },
                destination: "Games/Deep".into(),
                placement: Placement::CopyTree,
                bytes: 0,
            }],
        );

        let err = apply(&plan, &NoProgress).unwrap_err();
        assert!(
            err.to_string().contains(&game.display().to_string()),
            "the error should name the offending path: {err}"
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

    fn two_item_plan(dir: &Path, root: &Path) -> LayoutPlan {
        std::fs::write(dir.join("A.adf"), b"first").unwrap();
        std::fs::write(dir.join("B.adf"), b"second").unwrap();

        plan_of(
            root,
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
        )
    }

    /// **Cancelling leaves whole files, never half of one** (§54). The check
    /// sits between items, so the first is complete and the second was never
    /// begun. Every item already handled is durable on disk before the next
    /// is considered, so the count is not an estimate — `CancelledPartway`
    /// carries it (ART-058) rather than the bare `Cancelled` that would say
    /// nothing landed when one file did.
    #[test]
    fn stopping_leaves_the_finished_item_whole_and_reports_how_many_landed() {
        let dir = scratch("cancel-partway");
        let root = dir.join("staging");
        let plan = two_item_plan(&dir, &root);

        // Not cancelled for item 0, cancelled by the time item 1 is reached.
        let sink = StopAfter {
            seen: std::sync::atomic::AtomicU64::new(0),
            after: 1,
        };
        let err = apply(&plan, &sink).unwrap_err();

        assert_eq!(err.code(), "ART-CANCELLED-PARTWAY", "{err}");
        assert!(err.to_string().contains('1'), "{err}");
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

    /// The other half: cancelled before the first item is even considered,
    /// nothing has landed, and the plain `ART-CANCELLED` code is still right
    /// — there is no count to report.
    #[test]
    fn stopping_before_the_first_item_reports_plain_cancelled() {
        let dir = scratch("cancel-before-any");
        let root = dir.join("staging");
        let plan = two_item_plan(&dir, &root);

        // Cancelled from the very first check, before anything is copied.
        let sink = StopAfter {
            seen: std::sync::atomic::AtomicU64::new(0),
            after: 0,
        };
        let err = apply(&plan, &sink).unwrap_err();

        assert_eq!(err.code(), "ART-CANCELLED", "{err}");
        assert!(
            !root.join("Floppies").join("A.adf").exists(),
            "nothing was begun"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    // -----------------------------------------------------------------
    // UnpackWhdload
    // -----------------------------------------------------------------

    /// Asserts nothing under `parent` looks like a leftover unpack scratch
    /// directory. The scratch name is unique per call (pid + a counter), so
    /// this checks by substring rather than an exact path — a leaked one next
    /// to the staging tree is litter that looks like content, and is exactly
    /// as user-facing whatever its generated suffix happens to be.
    fn no_leftover_scratch_directory(parent: &Path) {
        if !parent.exists() {
            return;
        }
        for entry in std::fs::read_dir(parent).unwrap() {
            let name = entry.unwrap().file_name().to_string_lossy().into_owned();
            assert!(
                !name.contains("art-unpack"),
                "'{name}' under {} looks like a leftover unpack scratch directory",
                parent.display()
            );
        }
    }

    /// Build a zip holding `Turrican/Turrican.slave`, `Turrican/data/level1`
    /// and `Turrican.info` beside the drawer — the shape a real WHDLoad
    /// archive has.
    fn whdload_zip(path: &Path) {
        let file = std::fs::File::create(path).unwrap();
        let mut zip = zip::ZipWriter::new(file);
        let options: zip::write::FileOptions<'_, ()> =
            zip::write::FileOptions::default().compression_method(zip::CompressionMethod::Stored);
        for (name, body) in [
            ("Turrican/Turrican.slave", &b"slave"[..]),
            ("Turrican/data/level1", &b"level"[..]),
            ("Turrican.info", &b"icon"[..]),
        ] {
            zip.start_file(name, options).unwrap();
            std::io::Write::write_all(&mut zip, body).unwrap();
        }
        zip.finish().unwrap();
    }

    /// A zip with **no wrapper drawer**: the slave and the icon both sit at
    /// the archive's own root, `Turrican/data/level1` included. This is the
    /// case `layout.root` comes back empty for — the scratch directory
    /// itself is the drawer, and the icon file living right there must not
    /// be swept into the copy along with everything else.
    fn whdload_zip_no_wrapper(path: &Path) {
        let file = std::fs::File::create(path).unwrap();
        let mut zip = zip::ZipWriter::new(file);
        let options: zip::write::FileOptions<'_, ()> =
            zip::write::FileOptions::default().compression_method(zip::CompressionMethod::Stored);
        for (name, body) in [
            ("Turrican.slave", &b"slave"[..]),
            ("data/level1", &b"level"[..]),
            ("Turrican.info", &b"icon"[..]),
        ] {
            zip.start_file(name, options).unwrap();
            std::io::Write::write_all(&mut zip, body).unwrap();
        }
        zip.finish().unwrap();
    }

    /// **§82, in the staging tree.** The drawer lands under `Games/`, and its
    /// icon lands *beside* the drawer — not inside it, which would put the
    /// game on the disk and leave it invisible on Workbench.
    #[test]
    fn a_whdload_archive_unpacks_to_a_drawer_with_its_icon_beside_it() {
        let dir = scratch("unpack");
        let root = dir.join("staging");
        let archive = dir.join("Turrican.zip");
        whdload_zip(&archive);

        let plan = plan_of(
            &root,
            vec![LayoutItem {
                source: archive.clone(),
                kind: ItemKind::WhdloadArchive {
                    name: "Turrican".into(),
                },
                destination: "Games/Turrican".into(),
                placement: Placement::UnpackWhdload,
                bytes: 14,
            }],
        );

        apply(&plan, &NoProgress).unwrap();

        let games = root.join("Games");
        assert_eq!(
            std::fs::read(games.join("Turrican").join("Turrican.slave")).unwrap(),
            b"slave"
        );
        assert_eq!(
            std::fs::read(games.join("Turrican").join("data").join("level1")).unwrap(),
            b"level"
        );
        assert_eq!(
            std::fs::read(games.join("Turrican.info")).unwrap(),
            b"icon",
            "the icon sits beside the drawer, never inside it (§82)"
        );
        assert!(
            !games.join("Turrican").join("Turrican.info").exists(),
            "an icon inside the drawer is a game Workbench cannot see"
        );

        assert!(archive.exists(), "the archive is never consumed");
        no_leftover_scratch_directory(&games);

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// **§82 with no wrapper drawer.** When the archive's own root is the
    /// pack (`layout.root` is empty), the scratch directory *is* the drawer
    /// — so copying it verbatim would sweep the icon file up along with it,
    /// landing a second copy inside the drawer as well as the one placed
    /// beside it. The icon belongs beside the drawer only.
    #[test]
    fn a_wrapperless_whdload_archive_still_keeps_its_icon_outside_the_drawer() {
        let dir = scratch("unpack-no-wrapper");
        let root = dir.join("staging");
        let archive = dir.join("Turrican.zip");
        whdload_zip_no_wrapper(&archive);

        let plan = plan_of(
            &root,
            vec![LayoutItem {
                source: archive.clone(),
                kind: ItemKind::WhdloadArchive {
                    name: "Turrican".into(),
                },
                destination: "Games/Turrican".into(),
                placement: Placement::UnpackWhdload,
                bytes: 14,
            }],
        );

        apply(&plan, &NoProgress).unwrap();

        let games = root.join("Games");
        assert_eq!(
            std::fs::read(games.join("Turrican").join("Turrican.slave")).unwrap(),
            b"slave"
        );
        assert_eq!(
            std::fs::read(games.join("Turrican").join("data").join("level1")).unwrap(),
            b"level"
        );
        assert_eq!(
            std::fs::read(games.join("Turrican.info")).unwrap(),
            b"icon",
            "the icon sits beside the drawer, never inside it (§82)"
        );
        assert!(
            !games.join("Turrican").join("Turrican.info").exists(),
            "an icon inside the drawer is a game Workbench cannot see, and with no \
             wrapper drawer the icon file sits right there in the scratch directory \
             the copy walks — it must be excluded, not swept in"
        );

        assert!(archive.exists(), "the archive is never consumed");
        no_leftover_scratch_directory(&games);

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// An archive holding no pack is an answer about the archive, not a fault
    /// in ART — but it cannot be placed as a drawer, so it is refused by name.
    #[test]
    fn an_archive_with_no_slave_is_refused_rather_than_half_placed() {
        let dir = scratch("nopack");
        let root = dir.join("staging");
        let archive = dir.join("Plain.zip");
        {
            let file = std::fs::File::create(&archive).unwrap();
            let mut zip = zip::ZipWriter::new(file);
            let options: zip::write::FileOptions<'_, ()> = zip::write::FileOptions::default()
                .compression_method(zip::CompressionMethod::Stored);
            zip.start_file("readme.txt", options).unwrap();
            std::io::Write::write_all(&mut zip, b"hello").unwrap();
            zip.finish().unwrap();
        }

        let plan = plan_of(
            &root,
            vec![LayoutItem {
                source: archive,
                kind: ItemKind::WhdloadArchive {
                    name: "Plain".into(),
                },
                destination: "Games/Plain".into(),
                placement: Placement::UnpackWhdload,
                bytes: 5,
            }],
        );

        assert!(apply(&plan, &NoProgress).is_err());
        let games = root.join("Games");
        assert!(!games.join("Plain").exists());
        no_leftover_scratch_directory(&games);

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The gate every archive goes through, exercised on this path too: an
    /// entry naming its way out of the destination is refused, and nothing
    /// lands outside the staging tree.
    #[test]
    fn a_traversing_entry_never_escapes_the_staging_tree() {
        let dir = scratch("hostile");
        let root = dir.join("staging");
        let archive = dir.join("Evil.zip");
        {
            let file = std::fs::File::create(&archive).unwrap();
            let mut zip = zip::ZipWriter::new(file);
            let options: zip::write::FileOptions<'_, ()> = zip::write::FileOptions::default()
                .compression_method(zip::CompressionMethod::Stored);
            for name in ["Evil/Evil.slave", "../../escaped.txt"] {
                zip.start_file(name, options).unwrap();
                std::io::Write::write_all(&mut zip, b"x").unwrap();
            }
            zip.finish().unwrap();
        }

        let plan = plan_of(
            &root,
            vec![LayoutItem {
                source: archive,
                kind: ItemKind::WhdloadArchive {
                    name: "Evil".into(),
                },
                destination: "Games/Evil".into(),
                placement: Placement::UnpackWhdload,
                bytes: 2,
            }],
        );

        // Whether the pack places or the archive is refused, one thing must
        // hold: nothing is written outside the staging tree.
        let _ = apply(&plan, &NoProgress);
        assert!(!dir.join("escaped.txt").exists());
        assert!(!std::env::temp_dir().join("escaped.txt").exists());

        let _ = std::fs::remove_dir_all(&dir);
    }
}
