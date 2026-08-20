//! The original is never the thing being changed (§92, design §2).
//!
//! An Amiga-side install runs a program **ART did not write** — a BoingBag's
//! `Updater`, a package's own `Installer` script — against a mounted
//! directory. ART cannot supervise it file by file, so `core::safety`'s usual
//! answer (back up *this* file, write *this* file, verify *this* file) has
//! nothing to attach itself to: the set of files the installer will touch is
//! not knowable before it runs, and is not reported after.
//!
//! What is available at the granularity the operation actually has is a
//! **whole-tree copy**, and it is cheap: the AmigaOS 3.9 tree is 19 MB and
//! rebuilds from the user's media in about ten seconds. So the install runs
//! against the copy, and the copy replaces the original **only** when the run
//! reported success ([`settle`]).
//!
//! ## What each ending does
//!
//! [`RunOutcome`] has four variants and only one of them promotes anything.
//! The other three are not one thing called "not succeeded": a failure means
//! the installer said no, a timeout means nobody answered a requester, and a
//! closed emulator means the person watching shut the window. All three leave
//! the original **untouched** — and all three leave the copy **in place**,
//! because a user told "it failed" and not told where the evidence went has
//! been given nothing (design §4). [`Settlement::Kept`] carries that path so
//! the report can name it.
//!
//! A cancelled run is not an ending at all — [`super::run::run_with`] returns
//! [`CoreError::Cancelled`] rather than an outcome — and the design is
//! explicit that it *discards* the copy. That is the caller's
//! [`Staged::discard`], and it is the one path where the copy does not
//! survive.
//!
//! ## Why nothing here is a `Drop`
//!
//! A half-made copy is removed on `Drop` — by [`PartialCopy`], armed only
//! while the copy is being made — because a copy interrupted by an error, a
//! cancellation or a panic is not evidence of anything and 19 MB of it beside
//! the user's tree is litter (ART-184).
//!
//! A **finished** [`Staged`] deliberately has no `Drop`. Keeping the copy is
//! the requirement above, so removing it when nobody said to would be losing
//! the one thing a failed run leaves behind. The copy is removed exactly
//! twice: by [`Staged::discard`], and by [`Staged::commit`] when it has been
//! promoted into the original's place.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use super::RunOutcome;
use crate::core::error::{CoreError, CoreResult};
use crate::core::jobs::{NoProgress, ProgressSink};

/// How deep the copy will descend before it refuses.
///
/// The same cap and the same reason as `core/layout`'s own tree copy:
/// unbounded recursion overflows the stack, and the release profile's
/// `panic = "abort"` turns that into a dead application rather than an error.
/// It **refuses** past the cap rather than stopping quietly — a copy that
/// silently omitted everything below a cut could later be promoted over the
/// original, which is the one thing this module exists to prevent.
pub const MAX_TREE_DEPTH: usize = 32;

/// The suffix ART's copy of the tree carries, before the counter.
pub const STAGED_SUFFIX: &str = ".art-staged";

/// The suffix the original carries for the instant it is out of the way.
pub const RETIRED_SUFFIX: &str = ".art-previous";

/// A copy of the tree, made and complete, with the original untouched.
///
/// It is consumed by exactly one of [`commit`](Self::commit),
/// [`discard`](Self::discard) or [`settle`], so "nobody decided" is a shape
/// the type makes visible rather than a state it can be left in.
#[derive(Debug)]
pub struct Staged {
    original: PathBuf,
    copy: PathBuf,
}

/// What a promotion left on disk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Committed {
    /// Where the tree is — the original path, now holding what the installer
    /// produced.
    pub tree: PathBuf,
    /// The retired original, when it could **not** be removed after the swap.
    ///
    /// `None` is the ordinary case. It is `Some` rather than an error because
    /// the promotion did succeed: a directory that a virus scanner or an open
    /// Explorer window held onto for a moment is a thing to *tell the user
    /// about*, not a reason to report a swap that happened as one that did
    /// not (§89). The report names it so the user can delete it.
    pub left_behind: Option<PathBuf>,
}

/// What [`settle`] did, which is one of exactly two things.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Settlement {
    /// The run succeeded and the copy is now the tree.
    Promoted(Committed),
    /// The run did not succeed. **The original was not touched at all** — not
    /// backed up, not rewritten, not read — and the copy is at `copy`, where
    /// the user can look at what the installer did before it stopped.
    Kept { copy: PathBuf },
}

/// Copy `tree` beside itself, ready to be installed into.
///
/// The thin wrapper. See [`stage_with`] for everything it does beyond
/// choosing a sink.
pub fn stage(tree: &Path) -> CoreResult<Staged> {
    stage_with(tree, &NoProgress)
}

/// [`stage`], reporting progress and stoppable.
///
/// The check is between whole files — never inside one — so cancelling can
/// leave the copy unfinished but never a half-written file in it. An
/// unfinished copy is then removed entirely, so a cancelled stage leaves
/// nothing behind at all and the original is exactly as it was: nothing here
/// ever opens the original for writing.
///
/// ## The copy is a sibling, and that is load-bearing
///
/// It is made **beside** the tree rather than in `%TEMP%`, because
/// [`Staged::commit`] promotes it with a rename and a rename is only atomic —
/// only a rename at all, on Windows — within one filesystem. A copy on
/// another drive would have to be moved back byte by byte at exactly the
/// moment the user's tree is out of the way, which is the window this module
/// is written to avoid.
pub fn stage_with(tree: &Path, sink: &dyn ProgressSink) -> CoreResult<Staged> {
    if !tree.is_dir() {
        return Err(CoreError::InvalidInput(format!(
            "'{}' is not a directory; an Amiga-side install stages a distribution tree",
            tree.display()
        )));
    }

    let copy = sibling_of(tree, STAGED_SUFFIX)?;

    // SAFE_CREATE: `create_dir`, not `create_dir_all`. The counter makes a
    // collision unlikely; refusing makes it harmless if it happens anyway,
    // because whatever is already at that name is not ART's to overwrite.
    std::fs::create_dir(&copy)?;

    // Armed from here to the end of the copy. Every way out of the walk —
    // an I/O error, a cancellation, a panic — goes through this and leaves
    // nothing behind (ART-184).
    let partial = PartialCopy(Some(copy));

    let mut files = 0u64;
    copy_into(tree, partial.path(), 0, sink, &mut files)?;

    Ok(Staged {
        original: tree.to_path_buf(),
        copy: partial.keep(),
    })
}

/// Promote the copy, or keep it — whichever the run's ending says.
///
/// **Only [`RunOutcome::Succeeded`] promotes.** The other three arms are
/// listed by name rather than swept up by `_`, so a fifth ending would fail
/// to compile here instead of quietly inheriting whatever this one does.
pub fn settle(staged: Staged, outcome: &RunOutcome) -> CoreResult<Settlement> {
    match outcome {
        RunOutcome::Succeeded => staged.commit().map(Settlement::Promoted),
        // Three different things to tell the user, one thing to do with the
        // filesystem: nothing.
        RunOutcome::Failed | RunOutcome::TimedOut { .. } | RunOutcome::EmulatorClosed { .. } => {
            Ok(Settlement::Kept { copy: staged.copy })
        }
    }
}

impl Staged {
    /// The copy — where the install runs, and what a failed run leaves for the
    /// user to look at.
    pub fn copy_path(&self) -> &Path {
        &self.copy
    }

    /// The tree the copy was made from, which nothing here writes to until
    /// [`commit`](Self::commit).
    pub fn original_path(&self) -> &Path {
        &self.original
    }

    /// Put the copy where the original is.
    ///
    /// ## The swap is never a delete-then-move
    ///
    /// Two renames, in this order:
    ///
    /// 1. `original` → `original.art-previous-N`
    /// 2. `copy` → `original`
    ///
    /// and only then is the retired original removed. A delete-then-move
    /// would have an interval in which the user's tree had been destroyed and
    /// the new one was not yet in place; a crash there leaves them with
    /// neither. Here **both trees exist on disk at every instant**: before
    /// step 1 as `original` and `copy`, between the steps as `retired` and
    /// `copy`, after step 2 as `retired` and `original`.
    ///
    /// ## What the remaining window actually is
    ///
    /// It is a window in the **name**, not in the data, and saying so
    /// precisely matters more than claiming there is none. Between step 1 and
    /// step 2 the path `original` does not exist. Something else looking at
    /// that exact path in that interval — the user's file manager, a backup
    /// agent — sees nothing there. Nothing is lost: the tree is at `retired`,
    /// which is a sibling with a name ART chose, and if the process dies in
    /// the interval the user's original is intact under that name and this
    /// module's error says so.
    ///
    /// Both steps are same-directory metadata operations that move no bytes,
    /// so the interval is microseconds rather than the seconds a 19 MB copy
    /// takes — but it is not zero, and no filesystem ART targets offers a way
    /// to make it zero. Windows has no directory equivalent of
    /// `rename(2)`'s replace-in-one-step; `MoveFileEx` with
    /// `MOVEFILE_REPLACE_EXISTING` refuses directories outright, and POSIX
    /// `rename(2)` refuses to replace a non-empty directory too. Two renames
    /// is the best any of them offers.
    ///
    /// If step 2 fails, step 1 is undone and the original goes back — the
    /// error then says the original is unchanged, and if even that fails it
    /// says where the original is instead of leaving the user to find it.
    pub fn commit(self) -> CoreResult<Committed> {
        self.commit_inspecting(&mut |_| {})
    }

    /// [`commit`](Self::commit), calling `at_the_gap` with the retired
    /// original's path at the one instant the tree's own path is empty.
    ///
    /// The seam exists so the paragraph above is **provable** rather than
    /// asserted: `the_swap_never_has_a_moment_with_no_tree` runs inside the
    /// gap and reads the retired tree byte for byte. A design constraint
    /// nobody can observe is a design comment, and this project has spent
    /// enough on tests that pass against the defect they were written for.
    pub fn commit_inspecting(self, at_the_gap: &mut dyn FnMut(&Path)) -> CoreResult<Committed> {
        let retired = sibling_of(&self.original, RETIRED_SUFFIX)?;

        // Step 1. Nothing has been destroyed: the tree has a different name.
        std::fs::rename(&self.original, &retired)?;

        at_the_gap(&retired);

        // Step 2.
        if let Err(err) = std::fs::rename(&self.copy, &self.original) {
            return Err(match std::fs::rename(&retired, &self.original) {
                Ok(()) => CoreError::SafetyRefused(format!(
                    "the staged tree could not be moved into place ({err}); \
                     '{}' is back exactly as it was and the copy is still at '{}'",
                    self.original.display(),
                    self.copy.display()
                )),
                Err(back) => CoreError::SafetyRefused(format!(
                    "the staged tree could not be moved into place ({err}) and the \
                     original could not be moved back ({back}); nothing was destroyed \
                     — the original tree is at '{}' and the copy at '{}'",
                    retired.display(),
                    self.copy.display()
                )),
            });
        }

        // The tree is in place. From here nothing that goes wrong can cost the
        // user anything, so a retirement that will not delete is reported
        // rather than raised.
        let left_behind = match std::fs::remove_dir_all(&retired) {
            Ok(()) => None,
            Err(_) => Some(retired),
        };

        Ok(Committed {
            tree: self.original,
            left_behind,
        })
    }

    /// Throw the copy away and leave the original alone — the cancellation
    /// path (design §4).
    ///
    /// A copy that is already gone is `Ok(())`: the caller wanted it gone and
    /// it is gone. The same rule `core::safety::guarded_remove` applies to a
    /// file it has nothing to preserve.
    pub fn discard(self) -> CoreResult<()> {
        match std::fs::remove_dir_all(&self.copy) {
            Ok(()) => Ok(()),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(err) => Err(err.into()),
        }
    }
}

/// A copy that is not finished yet, and is therefore worth nothing.
///
/// Removes itself on `Drop` — including on a panic, which is when leaking
/// hurts most (ART-184) — unless [`keep`](Self::keep) claims it.
#[derive(Debug)]
struct PartialCopy(Option<PathBuf>);

impl PartialCopy {
    fn path(&self) -> &Path {
        self.0.as_deref().expect("armed until keep() is called")
    }

    /// The copy is complete. Take the path and disarm.
    fn keep(mut self) -> PathBuf {
        self.0.take().expect("keep() is called once")
    }
}

impl Drop for PartialCopy {
    fn drop(&mut self) {
        if let Some(path) = &self.0 {
            let _ = std::fs::remove_dir_all(path);
        }
    }
}

/// `<parent>/<name><suffix>-<pid>-<counter>`.
///
/// A counter, never a bare timestamp: two stages a second apart would collide
/// on a one-second stamp, and two in the same millisecond on a finer one.
/// Same shape as `core::sources::install::Scratch` and `test_scratch_id`.
///
/// The name is built as an `OsString` rather than through
/// `to_string_lossy`, because a tree whose folder name is not valid Unicode
/// would otherwise be given a *different* name — and this path is the one the
/// original is renamed to during the swap.
fn sibling_of(tree: &Path, suffix: &str) -> CoreResult<PathBuf> {
    static NEXT: AtomicU64 = AtomicU64::new(0);

    let parent = tree.parent().ok_or_else(|| {
        CoreError::InvalidInput(format!(
            "'{}' has no parent directory, so ART cannot stage a copy beside it",
            tree.display()
        ))
    })?;
    let leaf = tree.file_name().ok_or_else(|| {
        CoreError::InvalidInput(format!(
            "'{}' does not name a directory ART can stage",
            tree.display()
        ))
    })?;

    let mut name = OsString::from(leaf);
    name.push(format!(
        "{suffix}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    Ok(parent.join(name))
}

/// Copy everything under `from` into `to`, which already exists.
///
/// ## Why `safe_join` is not the tool here
///
/// `core::security::safe_join` is the only route from an **untrusted name** —
/// an archive entry, a manifest string — to a path, and it works on `&str`.
/// These names come from `read_dir` on ART's own tree: they are single path
/// components by construction, since the OS returns them one component at a
/// time, and they are `OsString`s that may not be valid Unicode. Putting them
/// through `safe_join` would mean `to_string_lossy` first, which would give a
/// copied file a *different name* from the original — silent corruption of
/// exactly the kind this module exists to prevent, in the name of a check
/// that has nothing left to check.
fn copy_into(
    from: &Path,
    to: &Path,
    depth: usize,
    sink: &dyn ProgressSink,
    files: &mut u64,
) -> CoreResult<()> {
    if depth >= MAX_TREE_DEPTH {
        return Err(CoreError::InvalidInput(format!(
            "'{}' is nested deeper than ART will copy (limit {MAX_TREE_DEPTH})",
            from.display()
        )));
    }

    for entry in std::fs::read_dir(from)? {
        let entry = entry?;

        // Between whole files, never inside one. Nothing is open for writing
        // at this point, so stopping here can only leave the copy short — and
        // the copy is about to be removed entirely.
        if sink.is_cancelled() {
            return Err(CoreError::Cancelled);
        }

        let source = entry.path();
        let target = to.join(entry.file_name());
        let kind = EntryKind::from(&entry.file_type()?);

        match action_for(kind, &source)? {
            CopyAction::Descend => {
                std::fs::create_dir(&target)?;
                copy_into(&source, &target, depth + 1, sink, files)?;
            }
            CopyAction::CopyFile => {
                let expected = entry.metadata()?.len();
                let written = std::fs::copy(&source, &target)?;
                if written != expected {
                    return Err(CoreError::SafetyRefused(format!(
                        "'{}' copied short ({written} of {expected} bytes); the staged \
                         tree is incomplete and will not be used",
                        source.display()
                    )));
                }
                *files += 1;
                sink.report(*files, None, &entry.file_name().to_string_lossy());
            }
        }
    }
    Ok(())
}

/// What a directory entry is, reduced to the two questions the copy asks.
///
/// A pair of booleans rather than the `FileType` itself, so the policy below
/// is a **pure function that a test can drive**. That is not tidiness: a
/// symlink cannot be created on Windows without a privilege this project's
/// machine and its CI runner do not have, so a test that builds one and
/// asserts the refusal *silently does not run* — measured, not assumed, by
/// putting the defect back and watching nothing fail. A guard whose only test
/// skips itself is a guard with no test.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct EntryKind {
    is_link: bool,
    is_dir: bool,
}

impl From<&std::fs::FileType> for EntryKind {
    fn from(kind: &std::fs::FileType) -> Self {
        Self {
            is_link: kind.is_symlink(),
            is_dir: kind.is_dir(),
        }
    }
}

/// What the copy does with one entry — and there is deliberately **no
/// `Skip`**.
///
/// A skipped entry would leave the copy missing something the original had,
/// and the copy can later be promoted *over* the original, so a silent skip
/// here is a way to lose the user's data one release later. Anything that is
/// not a plain file or a plain directory is refused, which stops the run
/// before a single byte of the user's tree is at risk.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CopyAction {
    Descend,
    CopyFile,
}

/// The per-entry policy. A link is refused, whether it points at a file or a
/// directory — on Windows a directory symlink and a junction both report as
/// links *and* as directories, so the link question is asked first.
///
/// ART's own distribution trees (`core::osinstall`) contain no links at all,
/// so this is a refusal about a tree ART did not build.
fn action_for(kind: EntryKind, source: &Path) -> CoreResult<CopyAction> {
    if kind.is_link {
        return Err(CoreError::SafetyRefused(format!(
            "'{}' is a link; ART copies a distribution tree file by file and \
             will not promote a copy that silently lost one",
            source.display()
        )));
    }
    Ok(if kind.is_dir {
        CopyAction::Descend
    } else {
        CopyAction::CopyFile
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    use std::fs;
    use std::time::Duration;

    /// A directory that removes itself on `Drop` — **not** at the end of the
    /// happy path, because a panicking test never reaches the happy path and
    /// a red suite is exactly when leaking hurts most (ART-184).
    struct Scratch(PathBuf);

    impl Scratch {
        fn new(tag: &str) -> Self {
            let dir = std::env::temp_dir().join(format!(
                "art-amigainstall-stage-{tag}-{}",
                crate::core::test_scratch_id()
            ));
            let _ = fs::remove_dir_all(&dir);
            fs::create_dir_all(&dir).unwrap();
            Self(dir)
        }

        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    /// A distribution tree the shape `core::osinstall` produces: files, an
    /// Amiga-metadata `.uaem` sidecar beside each one, and a
    /// `distribution.json` at the root.
    fn distribution_tree(at: &Path) -> PathBuf {
        let tree = at.join("Workbench3.9");
        fs::create_dir_all(tree.join("C")).unwrap();
        fs::create_dir_all(tree.join("Libs")).unwrap();
        fs::create_dir_all(tree.join("Storage/DOSDrivers")).unwrap();

        fs::write(
            tree.join("distribution.json"),
            b"{\"components\":[\"base\"]}",
        )
        .unwrap();
        fs::write(tree.join("C/LoadModule"), b"\0\0\x03\xf3 LoadModule").unwrap();
        fs::write(
            tree.join("C/LoadModule.uaem"),
            b"----rwed 2026-08-20 11:02:03.00 \n",
        )
        .unwrap();
        fs::write(tree.join("Libs/workbench.library"), vec![0xa5; 4096]).unwrap();
        fs::write(
            tree.join("Libs/workbench.library.uaem"),
            b"----rwed 2026-08-20 11:02:04.00 \n",
        )
        .unwrap();
        fs::write(tree.join("Storage/DOSDrivers/CD0"), b"mountlist").unwrap();
        tree
    }

    /// Every file under `root`, relative and `/`-separated, with its bytes —
    /// so "byte for byte" means the sidecars and the manifest too, not just
    /// the payload files.
    fn fingerprint(root: &Path) -> BTreeMap<String, Vec<u8>> {
        fn walk(dir: &Path, prefix: &str, out: &mut BTreeMap<String, Vec<u8>>) {
            for entry in fs::read_dir(dir).unwrap() {
                let entry = entry.unwrap();
                let name = entry.file_name().to_string_lossy().to_string();
                let rel = if prefix.is_empty() {
                    name
                } else {
                    format!("{prefix}/{name}")
                };
                if entry.file_type().unwrap().is_dir() {
                    // An empty drawer is part of the tree too.
                    out.insert(format!("{rel}/"), Vec::new());
                    walk(&entry.path(), &rel, out);
                } else {
                    out.insert(rel, fs::read(entry.path()).unwrap());
                }
            }
        }
        let mut out = BTreeMap::new();
        walk(root, "", &mut out);
        out
    }

    /// What an installer running inside the emulator would have done to the
    /// copy: new files, changed files, changed sidecars.
    fn as_if_the_installer_ran(copy: &Path) {
        fs::write(copy.join("C/LoadModule"), b"\0\0\x03\xf3 LoadModule 45.6").unwrap();
        fs::write(
            copy.join("C/LoadModule.uaem"),
            b"----rwed 2026-08-21 09:00:00.00 \n",
        )
        .unwrap();
        fs::write(copy.join("Libs/boingbag.library"), b"new from the update").unwrap();
    }

    /// Names of `root`'s children that are ART staging siblings.
    fn staging_siblings(root: &Path) -> Vec<String> {
        let mut names: Vec<String> = fs::read_dir(root)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().to_string())
            .filter(|n| n.contains(STAGED_SUFFIX) || n.contains(RETIRED_SUFFIX))
            .collect();
        names.sort();
        names
    }

    /// A sink that cancels once it has seen `after` files go by.
    struct CancelAfter(u64, std::sync::atomic::AtomicU64);

    impl ProgressSink for CancelAfter {
        fn report(&self, _done: u64, _total: Option<u64>, _message: &str) {
            self.1.fetch_add(1, Ordering::Relaxed);
        }
        fn is_cancelled(&self) -> bool {
            self.1.load(Ordering::Relaxed) >= self.0
        }
    }

    #[test]
    fn a_copy_is_the_tree_and_the_tree_is_untouched() {
        let scratch = Scratch::new("copy");
        let tree = distribution_tree(scratch.path());
        let before = fingerprint(&tree);

        let staged = stage(&tree).unwrap();

        assert_eq!(
            fingerprint(staged.copy_path()),
            before,
            "the copy is the tree, sidecars and manifest included"
        );
        assert_eq!(fingerprint(&tree), before, "and the tree is still the tree");
        assert_ne!(staged.copy_path(), tree, "beside it, not over it");
        assert_eq!(
            staged.copy_path().parent(),
            tree.parent(),
            "a sibling, so the promotion is a rename and not a second copy"
        );

        staged.discard().unwrap();
    }

    /// The whole point. A run that fails must leave the original byte-for-byte
    /// as it was — including its `.uaem` sidecars and its manifest.
    ///
    /// The copy is **changed first**, the way a real installer would change
    /// it. Without that this test would pass against the defect it is written
    /// for: a `settle` that promoted unconditionally would put an identical
    /// copy in the original's place and the fingerprints would still match.
    #[test]
    fn a_failed_run_leaves_the_original_byte_for_byte() {
        let scratch = Scratch::new("failed");
        let tree = distribution_tree(scratch.path());
        let before = fingerprint(&tree);

        let staged = stage(&tree).unwrap();
        as_if_the_installer_ran(staged.copy_path());
        let copy_path = staged.copy_path().to_path_buf();

        let settled = settle(staged, &RunOutcome::Failed).unwrap();

        assert_eq!(
            fingerprint(&tree),
            before,
            "a failed run changes nothing about the original"
        );
        assert!(
            !before.contains_key("Libs/boingbag.library"),
            "the fixture only proves that if the installer's file is not in it"
        );
        match settled {
            Settlement::Kept { copy } => {
                assert_eq!(copy, copy_path, "and the report says where the copy is");
                assert!(
                    copy.join("Libs/boingbag.library").is_file(),
                    "which is still there to look at"
                );
            }
            other => panic!("a failed run must not promote: {other:?}"),
        }
    }

    /// The same for the two endings that are not failures. They differ in what
    /// the user is told, never in what happens to the tree.
    #[test]
    fn a_timeout_and_a_closed_emulator_leave_the_original_too() {
        for outcome in [
            RunOutcome::TimedOut {
                waited: Duration::from_secs(1200),
            },
            RunOutcome::EmulatorClosed {
                waited: Duration::from_secs(31),
            },
        ] {
            let scratch = Scratch::new("not-success");
            let tree = distribution_tree(scratch.path());
            let before = fingerprint(&tree);

            let staged = stage(&tree).unwrap();
            as_if_the_installer_ran(staged.copy_path());

            let settled = settle(staged, &outcome).unwrap();

            assert_eq!(fingerprint(&tree), before, "unchanged after {outcome:?}");
            assert!(
                matches!(settled, Settlement::Kept { .. }),
                "{outcome:?} must not promote, it got {settled:?}"
            );
        }
    }

    #[test]
    fn a_successful_run_replaces_the_original_with_the_copy() {
        let scratch = Scratch::new("ok");
        let tree = distribution_tree(scratch.path());
        let before = fingerprint(&tree);

        let staged = stage(&tree).unwrap();
        as_if_the_installer_ran(staged.copy_path());
        let installed = fingerprint(staged.copy_path());
        assert_ne!(installed, before, "the installer changed something");

        let settled = settle(staged, &RunOutcome::Succeeded).unwrap();

        assert_eq!(
            fingerprint(&tree),
            installed,
            "the tree is now what the installer produced"
        );
        match settled {
            Settlement::Promoted(committed) => {
                assert_eq!(committed.tree, tree, "at the path it always had");
                assert_eq!(committed.left_behind, None);
            }
            other => panic!("a success promotes: {other:?}"),
        }
        assert!(
            staging_siblings(scratch.path()).is_empty(),
            "and nothing of ART's is left beside it: {:?}",
            staging_siblings(scratch.path())
        );
    }

    /// A discarded copy leaves nothing behind.
    #[test]
    fn a_discarded_stage_removes_its_copy() {
        let scratch = Scratch::new("discard");
        let tree = distribution_tree(scratch.path());
        let before = fingerprint(&tree);

        let staged = stage(&tree).unwrap();
        as_if_the_installer_ran(staged.copy_path());
        let copy = staged.copy_path().to_path_buf();

        staged.discard().unwrap();

        assert!(!copy.exists(), "the copy is gone");
        assert_eq!(fingerprint(&tree), before, "the original never moved");
        assert!(staging_siblings(scratch.path()).is_empty());
    }

    /// The swap must not be a delete-then-move: a crash between the two would
    /// leave the user with neither.
    ///
    /// Read inside the gap — the one instant the tree's own path is empty —
    /// through the seam `commit_inspecting` exists for. Against a
    /// delete-then-move the retired tree would not exist at all, and against
    /// any implementation that copied instead of renaming, its contents would
    /// be incomplete at this moment.
    #[test]
    fn the_swap_never_has_a_moment_with_no_tree() {
        let scratch = Scratch::new("gap");
        let tree = distribution_tree(scratch.path());
        let before = fingerprint(&tree);

        let staged = stage(&tree).unwrap();
        as_if_the_installer_ran(staged.copy_path());
        let copy = staged.copy_path().to_path_buf();

        let mut seen = 0u32;
        let committed = staged
            .commit_inspecting(&mut |retired| {
                seen += 1;
                assert!(
                    retired.is_dir(),
                    "the original must still exist while its own path is empty"
                );
                assert_eq!(
                    fingerprint(retired),
                    before,
                    "and it must be all of it, byte for byte"
                );
                assert!(copy.is_dir(), "with the copy also still on disk");
            })
            .unwrap();

        assert_eq!(seen, 1, "one gap, entered once");
        assert_eq!(committed.tree, tree);
        assert!(tree.is_dir(), "and the tree is back at its own path");
        assert!(staging_siblings(scratch.path()).is_empty());
    }

    /// If the tree cannot be put back at its own name, the error says where
    /// the original is. Nothing is destroyed, and the user is not left to
    /// guess which of two ART-named siblings holds their system.
    #[test]
    fn a_swap_that_cannot_finish_says_where_the_original_is() {
        let scratch = Scratch::new("stuck");
        let tree = distribution_tree(scratch.path());
        let before = fingerprint(&tree);

        let staged = stage(&tree).unwrap();
        let copy = staged.copy_path().to_path_buf();
        let mut retired_path = PathBuf::new();

        // Occupy the tree's own name with a non-empty directory while it is
        // free: neither `rename` can then land on it, on Windows or on POSIX.
        let err = staged
            .commit_inspecting(&mut |retired| {
                retired_path = retired.to_path_buf();
                fs::create_dir(&tree).unwrap();
                fs::write(tree.join("in the way"), b"squatter").unwrap();
            })
            .expect_err("the swap could not finish");

        let message = err.to_string();
        assert!(
            message.contains(&retired_path.display().to_string()),
            "the error must name the retired original: {message}"
        );
        assert!(
            message.contains(&copy.display().to_string()),
            "and the copy: {message}"
        );
        assert_eq!(
            fingerprint(&retired_path),
            before,
            "the original is intact, under the name the error gave"
        );
    }

    #[test]
    fn a_cancelled_stage_leaves_nothing_behind() {
        let scratch = Scratch::new("cancel");
        let tree = distribution_tree(scratch.path());
        let before = fingerprint(&tree);

        let sink = CancelAfter(2, std::sync::atomic::AtomicU64::new(0));
        let err = stage_with(&tree, &sink).expect_err("cancelled");

        assert!(matches!(err, CoreError::Cancelled), "{err:?}");
        assert_eq!(fingerprint(&tree), before, "the original is untouched");
        assert!(
            staging_siblings(scratch.path()).is_empty(),
            "and the half-made copy was removed: {:?}",
            staging_siblings(scratch.path())
        );
    }

    #[test]
    fn staging_something_that_is_not_a_directory_is_refused() {
        let scratch = Scratch::new("notdir");
        let file = scratch.path().join("Workbench3.9.adf");
        fs::write(&file, b"not a tree").unwrap();

        assert!(matches!(
            stage(&file).unwrap_err(),
            CoreError::InvalidInput(_)
        ));
        assert!(staging_siblings(scratch.path()).is_empty());
    }

    #[test]
    fn a_tree_nested_deeper_than_the_cap_is_refused_not_truncated() {
        let scratch = Scratch::new("deep");
        let tree = scratch.path().join("Deep");
        let mut here = tree.clone();
        for _ in 0..=MAX_TREE_DEPTH {
            here = here.join("d");
        }
        fs::create_dir_all(&here).unwrap();
        fs::write(here.join("bottom"), b"still mine").unwrap();

        let err = stage(&tree).expect_err("too deep to copy");
        assert!(matches!(err, CoreError::InvalidInput(_)), "{err:?}");
        assert!(
            staging_siblings(scratch.path()).is_empty(),
            "and the partial copy went with it"
        );
    }

    #[test]
    fn two_stages_of_one_tree_do_not_share_a_directory() {
        let scratch = Scratch::new("counter");
        let tree = distribution_tree(scratch.path());

        let first = stage(&tree).unwrap();
        let second = stage(&tree).unwrap();

        assert_ne!(first.copy_path(), second.copy_path());
        first.discard().unwrap();
        second.discard().unwrap();
    }

    /// A link is refused rather than skipped: a copy silently missing one file
    /// can still be promoted over the original.
    ///
    /// Driven through the pure policy rather than through a real link,
    /// **because a real one cannot be created here**. `New-Item -ItemType
    /// SymbolicLink` on this machine answers *"Administrator privilege
    /// required"*, so the end-to-end version of this test returned early and
    /// asserted nothing — it passed with the guard removed. This one does not.
    #[test]
    fn a_link_is_refused_and_never_skipped() {
        let file_link = EntryKind {
            is_link: true,
            is_dir: false,
        };
        let dir_link = EntryKind {
            is_link: true,
            is_dir: true, // a Windows directory symlink, and a junction, report both
        };

        for kind in [file_link, dir_link] {
            let err = action_for(kind, Path::new("Libs/linked")).expect_err("{kind:?} is refused");
            assert!(matches!(err, CoreError::SafetyRefused(_)), "{err:?}");
            assert!(
                err.to_string().contains("Libs/linked"),
                "and it says which entry: {err}"
            );
        }

        assert_eq!(
            action_for(
                EntryKind {
                    is_link: false,
                    is_dir: true
                },
                Path::new("Libs")
            )
            .unwrap(),
            CopyAction::Descend
        );
        assert_eq!(
            action_for(
                EntryKind {
                    is_link: false,
                    is_dir: false
                },
                Path::new("Libs/workbench.library")
            )
            .unwrap(),
            CopyAction::CopyFile
        );
    }

    /// The half the pure policy cannot see: that a real link reaches it as
    /// one. Everything else about the copy walk is exercised by the tests
    /// above; this only asks whether `EntryKind::from` reads a `FileType`
    /// correctly, and it can only ask where the host lets a link be made.
    ///
    /// It **skips** where it cannot, and says so rather than reporting a pass
    /// it did not earn.
    #[test]
    fn a_real_link_reaches_the_policy_as_one_where_one_can_be_made() {
        let scratch = Scratch::new("link");
        let tree = distribution_tree(scratch.path());
        let target = tree.join("C/LoadModule");
        let link = tree.join("C/LoadModule-link");

        #[cfg(windows)]
        let made = std::os::windows::fs::symlink_file(&target, &link).is_ok();
        #[cfg(not(windows))]
        let made = std::os::unix::fs::symlink(&target, &link).is_ok();

        if !made {
            eprintln!(
                "skipped: this host will not create a symlink without a privilege it \
                 does not have; the refusal itself is covered by \
                 a_link_is_refused_and_never_skipped"
            );
            return;
        }

        let err = stage(&tree).expect_err("a link is refused");
        assert!(matches!(err, CoreError::SafetyRefused(_)), "{err:?}");
        assert!(staging_siblings(scratch.path()).is_empty());
    }
}
