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

use crate::core::archive::extract::ExtractOutcome;
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
    /// Items whose destination already held exactly what this plan would put
    /// there, and which were therefore stepped over (ART-177).
    ///
    /// This is what makes a half-finished run resume: re-running the same
    /// plan places what is missing and skips what is not. It is counted
    /// rather than hidden, so "nothing happened" and "it was already done"
    /// never look the same on screen.
    #[serde(default)]
    pub skipped: usize,
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

        match place(&plan.root, item) {
            Ok(Placed::Copied(bytes)) => {
                outcome.bytes += bytes;
                outcome.placed += 1;
            }
            Ok(Placed::AlreadyThere) => outcome.skipped += 1,
            // Everything before this is durably on disk and stays there —
            // `place` refuses to overwrite, so nothing can be rolled back
            // without deleting files this run legitimately created. What was
            // missing was **saying so** (ART-110): the residue used to come
            // back as a bare error and turn up in the next preview as
            // ordinary collisions, with nothing marking it as the wreckage of
            // a failed run. Cancelling has said this since ART-058; failing
            // now says it too.
            Err(err) if outcome.placed > 0 || outcome.skipped > 0 => {
                return Err(CoreError::PartiallyApplied {
                    placed: outcome.placed as u64,
                    item: item.destination.clone(),
                    reason: err.to_string(),
                })
            }
            Err(err) => return Err(err),
        }
    }

    sink.report(total, Some(total), "done");
    Ok(outcome)
}

/// What placing one item did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Placed {
    Copied(u64),
    /// The destination already held exactly this. Stepped over (ART-177).
    AlreadyThere,
}

/// Copy one item into `root`.
///
/// **Never overwrites** (§93), and that has not changed. What changed is what
/// happens when something is already at the destination: if it is *exactly
/// what this item would place*, it is stepped over rather than refused, which
/// is what lets a re-run of a half-finished plan finish it. Anything else is
/// refused exactly as before — the check is
/// [`presence::presence_of`](crate::core::layout::presence::presence_of), and
/// it answers `Different` whenever it cannot be certain.
fn place(root: &Path, item: &LayoutItem) -> CoreResult<Placed> {
    use crate::core::layout::presence::{presence_of, Presence};

    let target = safe_join(root, &item.destination).map_err(|err| {
        CoreError::SafetyRefused(format!(
            "'{}' does not stay inside the staging folder: {err}",
            item.destination
        ))
    })?;
    if target.exists() {
        // Asked here and not taken from the plan, deliberately: the plan was
        // computed before the confirmation and the disk can have moved on.
        // The screen's count is a preview; this is the decision.
        return match presence_of(root, item) {
            Presence::AlreadyInPlace => Ok(Placed::AlreadyThere),
            // The drawer is exactly right and its `.info` is not there —
            // which a resumed run produces whenever the first one stopped
            // between the two writes. Nothing is in the way, so this is work
            // to finish rather than a clash: the icon is written and the
            // drawer is left alone. Without this a resume produces a tree
            // Workbench cannot see and calls it done (§82, ART-106).
            Presence::IconMissing => Ok(Placed::Copied(unpack_whdload_icon_only(
                &item.source,
                &target,
            )?)),
            _ => Err(CoreError::InvalidInput(format!(
                "'{}' is already there; nothing is overwritten",
                item.destination
            ))),
        };
    }
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let bytes = match item.placement {
        Placement::CopyFile => std::fs::copy(&item.source, &target)?,
        Placement::CopyTree => copy_tree(&item.source, &target, 0)?,
        Placement::UnpackWhdload => unpack_whdload(&item.source, &target, Parts::DrawerAndIcon)?,
    };
    Ok(Placed::Copied(bytes))
}

/// Unpack an archive holding a WHDLoad pack into `target`, which is the
/// drawer's own path — so the icon goes to `target`'s **parent**.
///
/// Everything decompressed goes through `core/archive`'s one gate first, into
/// a scratch directory, and only then is the pack's own shape worked out. The
/// two steps are separate because the gate's question is "is this archive
/// hostile" and `analyse`'s is "where is the game", and neither should be
/// asked in the other's terms.
/// Which halves of a WHDLoad placement [`unpack_whdload`] should write.
///
/// A resume that stopped between the drawer and its icon needs the second
/// without the first — and `place()` will not overwrite, so re-writing the
/// drawer is not an option even when it would be harmless.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Parts {
    DrawerAndIcon,
    IconOnly,
}

/// Write only the `.info` beside a drawer that is already correct.
fn unpack_whdload_icon_only(archive: &Path, target: &Path) -> CoreResult<u64> {
    unpack_whdload(archive, target, Parts::IconOnly)
}

fn unpack_whdload(archive: &Path, target: &Path, parts: Parts) -> CoreResult<u64> {
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
        let extract_outcome =
            extract_with_backend(&mut *backend, &scratch, OverwritePolicy::Skip, &NoProgress)?;

        // The gate's own verdict, not just its side effects. `aborted` means
        // the running declared total passed `MAX_TOTAL_OUTPUT` partway
        // through — everything after that point was never written, so what
        // landed in `scratch` is a truncated archive, not a truncated game
        // that happens to still work. Placing it would be exactly the
        // silent-truncation failure `apply.rs`'s own module doc rules out.
        if extract_outcome.aborted {
            return Err(CoreError::SafetyRefused(
                extract_outcome
                    .abort_reason
                    .unwrap_or_else(|| "the archive was refused".into()),
            ));
        }

        let entries = walk_entries(&scratch, "", 0)?;
        let layout = analyse(&entries)?;

        // ART is not an Amiga and cannot run an `Install` script. An archive
        // that needs one is a source pack, not an installed game — placing
        // it as a drawer would put raw disk images on the card and call it
        // played, which is worse than saying so up front. `commands/whdload.rs`
        // refuses the same shape for its HDF install path; this is that
        // refusal's other half.
        if layout.needs_installer {
            return Err(CoreError::SafetyRefused(
                "this archive holds an Install script, which means the game has not been \
                 installed yet — ART cannot run an Amiga script. Install it in WinUAE first, \
                 then bring the finished drawer back here."
                    .into(),
            ));
        }

        // An entry the extraction gate refused (a traversal attempt, or one
        // past the per-entry cap) never reached `scratch`, so it is invisible
        // to `walk_entries`/`analyse` above. If it would have landed *inside*
        // the pack, the drawer on disk is missing part of the game — refusing
        // beats placing something that looks installed and is not. A refusal
        // *outside* the pack (a readme past the cap, say) does not block:
        // `layout.outside` already says those files are left behind on
        // purpose, so one that never arrived changes nothing the user sees.
        if let Some(missing) = refused_entry_inside_pack(&extract_outcome, &layout.root) {
            return Err(CoreError::SafetyRefused(format!(
                "'{missing}' could not be extracted safely, so the pack is incomplete and \
                 was not placed"
            )));
        }

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

        // Anything `analyse` marked `outside` the pack is left out of the
        // copy in **both** shapes, from the same field.
        //
        // **This is belt and braces, and no test covers it, because nothing
        // can reach it today** (ART-109). Follow both branches: when there is
        // a wrapper, `layout.outside` holds only paths that are not under
        // `layout.root`, and the copy below walks `drawer` — which *is*
        // `layout.root` — so it never meets them. When there is no wrapper,
        // `is_inside` returns true for every path, so `layout.outside` is
        // empty by construction. Either way the list is a no-op.
        //
        // It stays because the rule it states is the right one and the
        // reachability is `analyse`'s to change, not this function's: the day
        // that function marks a file *inside* the pack as outside it — a
        // second slave's stray sibling, say — this is what stops it riding
        // along, and a copy that relied on the directory boundary alone would
        // not. What must not happen is counting the test below as proof of
        // it; that test documents that such files do not land, which is true
        // for the reasons above and not because of this list.
        let mut skip_from_drawer: Vec<PathBuf> = layout
            .outside
            .iter()
            .map(|name| safe_join_in_scratch(&scratch, name))
            .collect::<CoreResult<_>>()?;
        if let Some(icon) = layout.icon.as_ref().filter(|_| layout.root.is_empty()) {
            skip_from_drawer.push(safe_join_in_scratch(&scratch, icon)?);
        }
        let mut bytes = match parts {
            Parts::DrawerAndIcon => copy_tree_excluding(&drawer, target, &skip_from_drawer, 0)?,
            // The drawer on disk has already been compared byte for byte
            // against this archive (`presence::same_pack`), so there is
            // nothing to write and nothing to check again.
            Parts::IconOnly => 0,
        };

        // §82: beside the drawer, never inside it — and **named after the
        // drawer that landed**, not after the pack name the archive carried.
        //
        // Those were two answers before ART-109: the drawer lands at the
        // destination's leaf and the icon used to land at
        // `layout.icon_name()`, from a second `analyse` over the extracted
        // tree. They agree whenever nobody retargets the row — and the screen
        // exists to let people retarget rows, at which point the drawer would
        // be `Games/TurricanII` and the icon `Games/Turrican.info`: an icon
        // attached to no drawer, which is silent, and which is precisely what
        // §82 exists to prevent. `core::layout::icon_destination` derives the
        // plan's side of the same rule from the same field.
        if let Some(icon) = &layout.icon {
            let from = safe_join_in_scratch(&scratch, icon)?;
            if let (Some(parent), Some(leaf)) = (target.parent(), target.file_name()) {
                let to = parent.join(format!("{}.info", leaf.to_string_lossy()));
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

/// The archive-entry name of an extraction-gate refusal that would have
/// landed inside `root` (the pack's own drawer, `""` meaning the whole
/// archive), or `None` if every refusal was outside it — or there were none.
///
/// Compares against the entry's own name as the gate saw it, which is the
/// same string shape `core/whdload::analyse`'s `is_inside` compares against
/// (`/`-separated, relative to the archive root) — normalised for `\` here
/// since a Windows-built archive can use either.
fn refused_entry_inside_pack(outcome: &ExtractOutcome, root: &str) -> Option<String> {
    outcome
        .extracted
        .iter()
        .filter(|entry| entry.skipped && !entry.is_dir)
        .find(|entry| {
            let normalized = entry.source_path.replace('\\', "/");
            root.is_empty() || normalized == root || normalized.starts_with(&format!("{root}/"))
        })
        .map(|entry| entry.source_path.clone())
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
    copy_tree_excluding(from, to, &[], depth)
}

/// `copy_tree`, but any path under `from` named in `skip` is left out of the
/// copy rather than swept in with everything else — the icon file beside a
/// wrapper-less WHDLoad pack's drawer (§82: placed separately, never inside
/// it), and anything `core::whdload::analyse` marked outside the pack.
fn copy_tree_excluding(from: &Path, to: &Path, skip: &[PathBuf], depth: usize) -> CoreResult<u64> {
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
        if skip.iter().any(|p| p == &source) {
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
        let dir = std::env::temp_dir().join(format!(
            "art-layout-apply-{tag}-{}",
            crate::core::test_scratch_id()
        ));
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
            too_deep: Default::default(),
            duplicates: Default::default(),
            already_in_place: Vec::new(),
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
                writes_icon: false,
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
                writes_icon: false,
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
                writes_icon: false,
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
                writes_icon: false,
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
                    writes_icon: false,
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
                    writes_icon: false,
                },
                LayoutItem {
                    source: dir.join("B.adf"),
                    kind: ItemKind::FloppyImage,
                    destination: "Floppies/B.adf".into(),
                    placement: Placement::CopyFile,
                    bytes: 6,
                    writes_icon: false,
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
                writes_icon: false,
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
                writes_icon: false,
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
                writes_icon: false,
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
                writes_icon: false,
            }],
        );

        // Whether the pack places or the archive is refused, one thing must
        // hold: nothing is written outside the staging tree.
        let _ = apply(&plan, &NoProgress);
        assert!(!dir.join("escaped.txt").exists());
        assert!(!std::env::temp_dir().join("escaped.txt").exists());

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// **A hostile entry inside the drawer refuses the whole pack, not just
    /// that entry.** The traversal attempt is refused by `safe_join` during
    /// extraction — same as the test above — but here the *rest* of the
    /// entry's name still names a path inside the pack's own drawer, so a
    /// version of this code that only looked at what landed in `scratch`
    /// would place a `Turrican` drawer missing whatever that entry was and
    /// call it a success. It must not.
    #[test]
    fn a_refused_entry_inside_the_drawer_blocks_the_whole_pack() {
        let dir = scratch("hostile-inside");
        let root = dir.join("staging");
        let archive = dir.join("Turrican.zip");
        {
            let file = std::fs::File::create(&archive).unwrap();
            let mut zip = zip::ZipWriter::new(file);
            let options: zip::write::FileOptions<'_, ()> = zip::write::FileOptions::default()
                .compression_method(zip::CompressionMethod::Stored);
            for name in ["Turrican/Turrican.slave", "Turrican/../../escaped.txt"] {
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
                    name: "Turrican".into(),
                },
                destination: "Games/Turrican".into(),
                placement: Placement::UnpackWhdload,
                bytes: 2,
                writes_icon: false,
            }],
        );

        let err = apply(&plan, &NoProgress).unwrap_err();
        assert!(err.to_string().contains("incomplete"), "{err}");
        let games = root.join("Games");
        assert!(
            !games.join("Turrican").exists(),
            "a pack missing an entry must not be reported as placed"
        );
        assert!(!dir.join("escaped.txt").exists());
        no_leftover_scratch_directory(&games);

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// **An uninstalled source pack is refused, not placed as a game.**
    /// `commands/whdload.rs` already refuses this shape for its HDF install
    /// path (ART cannot run an Amiga `Install` script); this is the same
    /// refusal on the layout path, so the two do not disagree about the same
    /// archive.
    #[test]
    fn an_archive_needing_an_installer_is_refused_not_placed_as_a_game() {
        let dir = scratch("needs-installer");
        let root = dir.join("staging");
        let archive = dir.join("Game.zip");
        {
            let file = std::fs::File::create(&archive).unwrap();
            let mut zip = zip::ZipWriter::new(file);
            let options: zip::write::FileOptions<'_, ()> = zip::write::FileOptions::default()
                .compression_method(zip::CompressionMethod::Stored);
            for (name, body) in [
                ("Game/Game.slave", &b"slave"[..]),
                ("Game/Install", &b"script"[..]),
            ] {
                zip.start_file(name, options).unwrap();
                std::io::Write::write_all(&mut zip, body).unwrap();
            }
            zip.finish().unwrap();
        }

        let plan = plan_of(
            &root,
            vec![LayoutItem {
                source: archive.clone(),
                kind: ItemKind::WhdloadArchive {
                    name: "Game".into(),
                },
                destination: "Games/Game".into(),
                placement: Placement::UnpackWhdload,
                bytes: 11,
                writes_icon: false,
            }],
        );

        let err = apply(&plan, &NoProgress).unwrap_err();
        assert!(err.to_string().contains("Install"), "{err}");
        let games = root.join("Games");
        assert!(
            !games.join("Game").exists(),
            "a source pack with no game installed must not be placed"
        );
        assert!(archive.exists(), "the archive is never consumed");
        no_leftover_scratch_directory(&games);

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// **A cut-short extraction is refused, never placed as a partial
    /// success.** More entries than `core::archive::extract::MAX_ENTRIES`
    /// aborts the whole extraction (nothing survives phase 1), the same cap
    /// exercised in `core/archive/extract.rs`'s own tests — the discarded
    /// `ExtractOutcome` this test guards against was the bug: before this
    /// fix, `apply.rs` never looked at `aborted` at all.
    #[test]
    fn an_aborted_extraction_is_refused_not_placed_as_a_success() {
        let dir = scratch("aborted");
        let root = dir.join("staging");
        let archive = dir.join("Big.zip");
        {
            let file = std::fs::File::create(&archive).unwrap();
            let mut zip = zip::ZipWriter::new(file);
            let options: zip::write::FileOptions<'_, ()> = zip::write::FileOptions::default()
                .compression_method(zip::CompressionMethod::Stored);
            for i in 0..(crate::core::archive::extract::MAX_ENTRIES + 1) {
                zip.start_file(format!("f{i}"), options).unwrap();
            }
            zip.finish().unwrap();
        }

        let plan = plan_of(
            &root,
            vec![LayoutItem {
                source: archive,
                kind: ItemKind::WhdloadArchive { name: "Big".into() },
                destination: "Games/Big".into(),
                placement: Placement::UnpackWhdload,
                bytes: 0,
                writes_icon: false,
            }],
        );

        let err = apply(&plan, &NoProgress).unwrap_err();
        assert_eq!(err.code(), "ART-SAFETY-REFUSED", "{err}");
        let games = root.join("Games");
        assert!(
            !games.join("Big").exists(),
            "an aborted extraction must not be reported as placed"
        );
        no_leftover_scratch_directory(&games);

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// **The wrapped and wrapper-less shapes drop the same kind of file the
    /// same way.** A file `analyse` marks `outside` the pack never lands
    /// anywhere in the staging tree — not inside the drawer, not beside it.
    ///
    /// **What this does not prove** (ART-109): it does not cover
    /// `skip_from_drawer`'s `outside` entries. It passes with that exclusion
    /// removed, because the wrapped copy walks the drawer directory and these
    /// files are by definition not in it. The exclusion's own reachability is
    /// argued at its call site rather than tested here, and this test is not
    /// evidence for it. What it does pin is real and worth pinning: a readme
    /// beside the pack lands nowhere at all.
    #[test]
    fn a_file_outside_the_pack_is_dropped_rather_than_landing_in_the_drawer() {
        let dir = scratch("outside");
        let root = dir.join("staging");
        let archive = dir.join("Turrican.zip");
        {
            let file = std::fs::File::create(&archive).unwrap();
            let mut zip = zip::ZipWriter::new(file);
            let options: zip::write::FileOptions<'_, ()> = zip::write::FileOptions::default()
                .compression_method(zip::CompressionMethod::Stored);
            for (name, body) in [
                ("Turrican/Turrican.slave", &b"slave"[..]),
                ("Turrican.info", &b"icon"[..]),
                ("Turrican.readme", &b"not part of the pack"[..]),
            ] {
                zip.start_file(name, options).unwrap();
                std::io::Write::write_all(&mut zip, body).unwrap();
            }
            zip.finish().unwrap();
        }

        let plan = plan_of(
            &root,
            vec![LayoutItem {
                source: archive,
                kind: ItemKind::WhdloadArchive {
                    name: "Turrican".into(),
                },
                destination: "Games/Turrican".into(),
                placement: Placement::UnpackWhdload,
                bytes: 25,
                writes_icon: false,
            }],
        );

        apply(&plan, &NoProgress).unwrap();

        let games = root.join("Games");
        assert!(
            !games.join("Turrican").join("Turrican.readme").exists(),
            "not part of the pack, so not inside the drawer"
        );
        assert!(
            !games.join("Turrican.readme").exists(),
            "the pack is what the user asked for — left out, not placed beside it either"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    // ---- ART-109: a real `.lha`, and the icon's one name ----

    /// A `.lha` holding `Turrican/Turrican.slave`, a data file, and
    /// `Turrican.info` beside the drawer — the shape a real WHDLoad pack ships
    /// in.
    ///
    /// Every WHDLoad fixture in this module used to be a ZIP built at runtime,
    /// which is the "fixture more helpful than reality" shape this project has
    /// been bitten by three times: real packs are LHA, and the drawer's name is
    /// derived **twice from two sources** — `plan` reads the archive's entry
    /// names through `archive::open`, `apply` re-runs `analyse` over the
    /// extracted tree. Those two must agree for every backend, and only a
    /// fixture in the real format asks the question of the real backend.
    fn whdload_lha(path: &Path) {
        std::fs::write(
            path,
            crate::core::lha::tests::make_lha_with(&[
                ("Turrican/Turrican.slave", &b"slave"[..]),
                ("Turrican/data/level1", &b"level"[..]),
                ("Turrican.info", &b"icon"[..]),
            ]),
        )
        .unwrap();
    }

    /// F6 of the wave-C1 review. The `.lha` fixture above is stored `-lh0-`
    /// at **level 0**, and Wave A measured 914 level-1 and 2 259 level-2
    /// entries in the owner's own archives — so the format the tests exercise
    /// was the one real packs least often use, which is the "fixture more
    /// helpful than reality" shape again.
    ///
    /// A level-1 header keeps the drawer in an extension header, `0xFF`
    /// separated rather than `/`, which is a different code path in the
    /// reader and therefore a different answer to "what is this pack called".
    /// `plan` reads that name off the entry list and `apply` re-derives it
    /// from the extracted tree; both have to agree here too.
    #[test]
    fn a_level_one_lha_pack_plans_and_applies_under_one_name() {
        use crate::core::layout::{plan, policy::Policy};

        let dir = scratch("lha-level1");
        let root = dir.join("staging");
        let archive = dir.join("Turrican.lha");
        std::fs::write(
            &archive,
            crate::core::lha::tests::make_level1_archive(&[
                (&b"Turrican"[..], &b"Turrican.slave"[..], &b"slave"[..]),
                (&b"Turrican\xFFdata"[..], &b"level1"[..], &b"level"[..]),
                (&b""[..], &b"Turrican.info"[..], &b"icon"[..]),
            ]),
        )
        .unwrap();

        let made = plan(&root, std::slice::from_ref(&archive), &Policy::default()).unwrap();
        assert_eq!(made.items.len(), 1, "{made:?}");
        assert_eq!(made.items[0].destination, "Games/Turrican");
        assert!(made.items[0].writes_icon);

        apply(&made, &NoProgress).unwrap();

        let games = root.join("Games");
        assert_eq!(
            std::fs::read(games.join("Turrican").join("Turrican.slave")).unwrap(),
            b"slave"
        );
        assert!(games.join("Turrican").join("data").join("level1").exists());
        assert_eq!(std::fs::read(games.join("Turrican.info")).unwrap(), b"icon");
        assert!(!games.join("Turrican").join("Turrican.info").exists());

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// ART-109. One `.lha` driven end to end through `plan` → `apply`: the
    /// name the entry list gave and the name the extracted tree gives have to
    /// be the same name, or the icon lands under something that is not the
    /// drawer and §82 fails silently.
    #[test]
    fn an_lha_whdload_pack_plans_and_applies_under_one_name() {
        use crate::core::layout::{plan, policy::Policy};

        let dir = scratch("lha-plan-apply");
        let root = dir.join("staging");
        let archive = dir.join("Turrican.lha");
        whdload_lha(&archive);

        let made = plan(&root, std::slice::from_ref(&archive), &Policy::default()).unwrap();
        assert_eq!(made.items.len(), 1, "{made:?}");
        assert_eq!(made.items[0].placement, Placement::UnpackWhdload);
        assert_eq!(
            made.items[0].destination, "Games/Turrican",
            "the name from the entry list"
        );
        assert!(
            made.items[0].writes_icon,
            "this pack ships an icon, so the plan has to know a second file lands"
        );

        apply(&made, &NoProgress).unwrap();

        let games = root.join("Games");
        assert_eq!(
            std::fs::read(games.join("Turrican").join("Turrican.slave")).unwrap(),
            b"slave",
            "the drawer landed under the name the plan proposed"
        );
        assert!(games.join("Turrican").join("data").join("level1").exists());
        assert_eq!(
            std::fs::read(games.join("Turrican.info")).unwrap(),
            b"icon",
            "§82: the icon beside the drawer, under the drawer's own name"
        );
        assert!(
            !games.join("Turrican").join("Turrican.info").exists(),
            "never inside it"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The divergence ART-109 warned about, made to happen: the screen exists
    /// to let people retarget a row, and the icon has to follow the drawer.
    ///
    /// Before the fix the drawer came from the destination's leaf and the icon
    /// from `PackLayout::icon_name()` — the pack's own name — so retargeting
    /// `Games/Turrican` to `Games/TurricanII` left `Games/Turrican.info`
    /// sitting beside a drawer of a different name, which is an icon Workbench
    /// attaches to nothing. Both now come from the destination.
    #[test]
    fn a_retargeted_whdload_row_takes_its_icon_with_it() {
        use crate::core::layout::{plan, policy::Policy};

        let dir = scratch("lha-retarget");
        let root = dir.join("staging");
        let archive = dir.join("Turrican.lha");
        whdload_lha(&archive);

        let mut made = plan(&root, std::slice::from_ref(&archive), &Policy::default()).unwrap();
        // Exactly what the retarget box on the layout screen does.
        made.items[0].destination = "Games/TurricanII".into();

        apply(&made, &NoProgress).unwrap();

        let games = root.join("Games");
        assert!(games.join("TurricanII").join("Turrican.slave").exists());
        assert!(
            games.join("TurricanII.info").exists(),
            "the icon is named after the drawer that landed"
        );
        assert!(
            !games.join("Turrican.info").exists(),
            "and not after the pack name the archive happened to carry"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// ART-110. A run that fails on its fifth item has already put four on
    /// disk, and `place` refuses to overwrite, so they stay. What was missing
    /// was the error saying so — the residue used to come back as a bare
    /// refusal and turn up in the next preview as ordinary collisions, with
    /// nothing marking it as the wreckage of a failed run.
    ///
    /// Cancelling has reported its count since ART-058; failing now matches.
    #[test]
    fn a_run_that_fails_partway_says_how_much_of_it_landed() {
        let dir = scratch("partial");
        let root = dir.join("staging");
        let good = dir.join("Good.adf");
        let missing = dir.join("Missing.adf");
        std::fs::write(&good, b"disk bytes").unwrap();
        // Not created: `place` reaches `std::fs::copy` and fails on it.

        let plan = plan_of(
            &root,
            vec![
                LayoutItem {
                    source: good,
                    kind: ItemKind::FloppyImage,
                    destination: "Floppies/Good.adf".into(),
                    placement: Placement::CopyFile,
                    bytes: 10,
                    writes_icon: false,
                },
                LayoutItem {
                    source: missing,
                    kind: ItemKind::FloppyImage,
                    destination: "Floppies/Missing.adf".into(),
                    placement: Placement::CopyFile,
                    bytes: 10,
                    writes_icon: false,
                },
            ],
        );

        let err = apply(&plan, &NoProgress).unwrap_err();
        assert_eq!(err.code(), "ART-APPLY-PARTIAL", "{err}");
        match &err {
            CoreError::PartiallyApplied { placed, item, .. } => {
                assert_eq!(*placed, 1);
                assert_eq!(item, "Floppies/Missing.adf");
            }
            other => panic!("{other:?}"),
        }
        assert!(
            root.join("Floppies").join("Good.adf").exists(),
            "the item that landed is still there — which is exactly why the error has to              mention it"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The other side: a run that fails on its **first** item placed nothing,
    /// so it must report the plain reason rather than dressing it up as a
    /// partial apply the user has to go and clean up.
    #[test]
    fn a_run_that_fails_on_its_first_item_reports_the_plain_reason() {
        let dir = scratch("partial-first");
        let root = dir.join("staging");
        let plan = plan_of(
            &root,
            vec![LayoutItem {
                source: dir.join("Missing.adf"),
                kind: ItemKind::FloppyImage,
                destination: "Floppies/Missing.adf".into(),
                placement: Placement::CopyFile,
                bytes: 10,
                writes_icon: false,
            }],
        );

        let err = apply(&plan, &NoProgress).unwrap_err();
        assert_ne!(err.code(), "ART-APPLY-PARTIAL", "{err}");

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// ART-177, the whole point: **re-running a half-finished plan finishes
    /// it.** No "continue" button, no resume mode — the same plan, applied
    /// again, places what is missing and steps over what is not.
    #[test]
    fn re_running_a_half_finished_plan_finishes_it() {
        use crate::core::layout::{plan, policy::Policy};

        let dir = scratch("resume");
        let root = dir.join("staging");
        let first = dir.join("First.adf");
        let second = dir.join("Second.adf");
        // A 901 120-byte file starting `DOS\0` is an ADF, and `core/detect`
        // says so from the bytes. The tail differs between the two so they
        // are genuinely different images, not two names for one.
        let adf = |path: &Path, tag: u8| {
            let mut bytes = vec![0u8; 901_120];
            bytes[0..4].copy_from_slice(b"DOS\0");
            bytes[900_000] = tag;
            std::fs::write(path, bytes).unwrap();
        };
        adf(&first, 1);
        adf(&second, 2);

        let made = plan(&root, &[first.clone(), second.clone()], &Policy::default()).unwrap();
        assert_eq!(made.items.len(), 2);
        assert!(made.already_in_place.is_empty(), "nothing is there yet");

        // Simulate the first run stopping after one item: place it by hand,
        // exactly as `place` would have.
        let floppies = root.join("Floppies");
        std::fs::create_dir_all(&floppies).unwrap();
        std::fs::copy(&first, floppies.join("First.adf")).unwrap();

        // The next preview no longer calls that a collision.
        let again = plan(&root, &[first, second], &Policy::default()).unwrap();
        assert!(
            again.collisions.is_empty(),
            "the wreckage of a stopped run is not a question for the user: {:?}",
            again.collisions
        );
        assert_eq!(
            again.already_in_place,
            vec!["Floppies/First.adf".to_string()]
        );

        // And applying it finishes the job rather than refusing.
        let outcome = apply(&again, &NoProgress).unwrap();
        assert_eq!(outcome.placed, 1, "only the one that was missing");
        assert_eq!(outcome.skipped, 1, "and the one that was already right");
        assert!(floppies.join("Second.adf").exists());

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// G1's third case, end to end: **a resumed apply restores a missing
    /// `.info`.** A run that stopped between the drawer and its icon leaves a
    /// drawer Workbench cannot see; re-running the plan has to finish it, and
    /// before this it reported the drawer as already-in-place and left the
    /// icon missing for ever.
    #[test]
    fn a_resumed_apply_restores_an_icon_the_first_run_never_wrote() {
        use crate::core::layout::{plan, policy::Policy};

        let dir = scratch("resume-icon");
        let root = dir.join("staging");
        let archive = dir.join("Turrican.lha");
        whdload_lha(&archive);

        let made = plan(&root, std::slice::from_ref(&archive), &Policy::default()).unwrap();
        apply(&made, &NoProgress).unwrap();

        let games = root.join("Games");
        let icon = games.join("Turrican.info");
        assert!(icon.exists());

        // The state a run stopped between the two writes leaves behind.
        std::fs::remove_file(&icon).unwrap();

        let again = plan(&root, std::slice::from_ref(&archive), &Policy::default()).unwrap();
        assert!(
            again.collisions.is_empty(),
            "the drawer is ours and the icon is missing — nothing is in the way: {:?}",
            again.collisions
        );
        assert!(
            again.already_in_place.is_empty(),
            "…and it is not finished either, so it must not be counted as settled"
        );

        let outcome = apply(&again, &NoProgress).unwrap();
        assert_eq!(outcome.skipped, 0);
        assert_eq!(outcome.placed, 1, "the icon was written");
        assert_eq!(
            std::fs::read(&icon).unwrap(),
            b"icon",
            "§82: the drawer is unreadable to Workbench without it"
        );

        // The drawer itself was left alone rather than rewritten.
        assert_eq!(
            std::fs::read(games.join("Turrican").join("Turrican.slave")).unwrap(),
            b"slave"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The other side, and the one that stops this being a licence to
    /// overwrite: a destination holding something **else** is still refused.
    #[test]
    fn a_destination_holding_something_else_is_still_refused() {
        let dir = scratch("resume-different");
        let root = dir.join("staging");
        let source = dir.join("Disk.adf");
        std::fs::write(&source, b"the real one").unwrap();
        std::fs::create_dir_all(root.join("Floppies")).unwrap();
        std::fs::write(root.join("Floppies").join("Disk.adf"), b"someone else").unwrap();

        let made = plan_of(
            &root,
            vec![LayoutItem {
                source,
                kind: ItemKind::FloppyImage,
                destination: "Floppies/Disk.adf".into(),
                placement: Placement::CopyFile,
                bytes: 12,
                writes_icon: false,
            }],
        );

        let err = apply(&made, &NoProgress).unwrap_err();
        assert!(err.to_string().contains("already there"), "{err}");
        assert_eq!(
            std::fs::read(root.join("Floppies").join("Disk.adf")).unwrap(),
            b"someone else",
            "nothing is overwritten (§93)"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }
}
