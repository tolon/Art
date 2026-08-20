//! Removing a file from the **user's own disk** — the one thing ART could not
//! do until ART-080, and the reason a move off a host folder was a copy.
//!
//! # Why this is a trait
//!
//! Because the answer is a Windows API and `core/` may not call one.
//!
//! The owner's ruling on ART-080 is that a deleted host file goes to the
//! **Windows Recycle Bin**: ART invents no recovery mechanism of its own and
//! uses the one the operating system already has — the one place a user
//! already knows to look. That beats a `.art-backup/` directory beside the
//! file, which nobody discovers and which duplicates a multi-gigabyte ISO in
//! order to move it, and it beats a permanent unlink, which nobody can undo.
//!
//! Sending a file to the Recycle Bin is `IFileOperation` (or `SHFileOperation`
//! before it), and CLAUDE.md's core-independence rule is explicit: if a core
//! module needs something platform-specific, it exposes a **trait** and the
//! implementation lives outside `core/`. That is what
//! [`core::preload::VolumeFormatter`](crate::core::preload::VolumeFormatter)
//! and `tools/hst_imager.rs` already are, and this follows them exactly —
//! trait here, implementation in `tools/recycle_bin.rs`, so the planning above
//! it and every test below it need no shell, no COM apartment and no Windows.
//!
//! # Why the outcome is per entry, and not all-or-nothing
//!
//! `core::volume::write`'s `delete_many` is all-or-nothing on both write
//! strategies (ART-073), and that guarantee is real because a disk image has a
//! journal: a batch that cannot finish rolls back whole. **A host filesystem
//! has none.** Twelve files sent to the Recycle Bin one by one are twelve
//! completed operations, and the thirteenth failing cannot undo them.
//!
//! Claiming all-or-nothing here would be a promise ART cannot keep, and §89
//! says not to. So [`HostDeleteOutcome`] reports every entry by name and by
//! result, and the screen says exactly what went and what did not — which is
//! the honest form of the same duty of care.
//!
//! # What it refuses rather than guesses
//!
//! Every path is checked against the parent directory it was listed from,
//! through [`crate::core::security::safe_join`] — the only route from a name
//! to a path in this codebase. A selection is a list of **names in a
//! directory**, never a list of absolute paths, so nothing the frontend sends
//! can name a file outside the folder the user is looking at, however it was
//! built or tampered with on the way. A **drive root** is refused outright by
//! [`refuse_drive_root`]: `C:\` is where `Windows` and `Program Files` live,
//! and the confirmations a user learns to click through for a game are the
//! same ones here.
//!
//! # What is guarded, and what is not
//!
//! Stated plainly rather than left to be discovered, because this is the one
//! module in ART that removes a user's own file.
//!
//! **Guarded.** `..` in any spelling, an absolute path, a UNC path, a Windows
//! drive prefix, a `RootDir` component — all refused by `safe_join` before the
//! loop starts, and refused for the *whole* pass rather than skipped. A drive
//! root as the parent. A name that is not there. A recycler that returns `Ok`
//! and leaves the file behind.
//!
//! **Not guarded, and known.** `safe_join`'s containment is **lexical**: it
//! reasons about the path as written, not about what the filesystem resolves
//! it to. A symbolic link or an NTFS junction sitting inside the folder, whose
//! target is outside it, passes containment and is then handed to the
//! recycler. What that costs is bounded by what the shell does with a link:
//! `IFileOperation` recycles the **link**, not the directory it points at — so
//! the realistic outcome is a lost shortcut rather than a lost tree. It is
//! bounded, not zero, and it is not checked.
//!
//! `Path::exists()` follows links too, so the pre-check answers about the
//! *target*: a dangling link inside the folder is reported as "not there" and
//! refuses the pass, which is conservative but not the reason one would want.
//!
//! And there is a **check-to-call window**. Every name is resolved and
//! verified before the first recycle, and the filesystem can change in
//! between — the same TOCTOU shape every path in this codebase that pre-checks
//! has, and the reason each entry's result is asked of the filesystem
//! afterwards rather than assumed. Closing it properly means opening each
//! entry by handle and operating on that handle, which `trash` does not
//! expose; recorded here rather than papered over.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::core::error::{CoreError, CoreResult};
use crate::core::jobs::ProgressSink;
use crate::core::security::safe_join;

/// Sending things on the user's own disk to wherever the host's own
/// undo mechanism keeps them.
///
/// One method, and deliberately one: ART puts things in the Recycle Bin and
/// never reads it back, so there is no `list` and no `restore` here. Adding
/// them would mean ART owning a second view of a thing the operating system
/// already shows the user better than ART could.
pub trait HostRecycler {
    /// The name of the place things go, for the sentence the screen shows.
    ///
    /// A value the implementation supplies rather than a string `core/`
    /// invents, because `core/` does not know it is on Windows — and because
    /// "where did my file go" has to be answerable in the user's own language
    /// (ART-060 is why this is a [`RecycleTarget`] and not an English
    /// sentence).
    fn target(&self) -> RecycleTarget;

    /// Send one existing file or directory there.
    ///
    /// `path` has already been resolved and checked by [`recycle_many`]; an
    /// implementation must not re-derive it from a name. Returning `Ok(())`
    /// means the host has taken it — not that ART has verified it is gone,
    /// which is [`recycle_many`]'s job and which it does by asking the
    /// filesystem.
    fn recycle(&self, path: &Path) -> CoreResult<()>;
}

/// Where a deleted host file goes, as a value the UI translates.
///
/// An enum rather than a string so a future non-Windows shell cannot make the
/// screen say "Recycle Bin" on a machine that has none — and so the sentence
/// itself lives in both catalogues rather than in Rust (ART-060).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RecycleTarget {
    /// Windows' own Recycle Bin.
    WindowsRecycleBin,
}

impl RecycleTarget {
    /// The name for the **operation log**, which is English whatever language
    /// the screen is in (ART-060, the same rule every `CoreError` sentence
    /// follows).
    ///
    /// Deliberately not the screen's sentence: the UI translates
    /// [`RecycleTarget`] itself through its own catalogue. This exists so the
    /// log records where a file went by asking the recycler rather than by
    /// repeating a literal — a second recycler would otherwise log the first
    /// one's destination.
    pub fn log_label(self) -> &'static str {
        match self {
            Self::WindowsRecycleBin => "Recycle Bin",
        }
    }
}

/// What happened to one named entry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HostDeleteRow {
    /// The name as the pane listed it — not the resolved path. The user
    /// recognises the name; the path is where it happened to live.
    pub name: String,
    /// Whether it reached the recycler **and is really gone from the folder**.
    pub removed: bool,
    /// Why not, in the reader's own words, when it did not. `None` on
    /// success. English, like every `CoreError` sentence (ART-060), and shown
    /// after the translated one rather than instead of it.
    pub problem: Option<String>,
}

/// What one whole pass did.
///
/// Both halves are always present and always named, because "eleven of twelve"
/// is not an answer a user can act on — the twelfth's name is.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HostDeleteOutcome {
    pub rows: Vec<HostDeleteRow>,
    /// Where the removed ones went, for the sentence the screen shows.
    /// `None` when nothing was removed — naming a destination for a delete
    /// that did not happen would be the same class of invention §89 forbids.
    pub target: Option<RecycleTarget>,
    /// How many names the pass was **asked** for, which is not `rows.len()`
    /// when it stopped early.
    pub asked: usize,
    /// Whether the user stopped it partway.
    ///
    /// **Without this the outcome reads as a success it did not have.** A
    /// twelve-name request cancelled after three has three rows, all
    /// `removed: true`, and every count derived from `rows` alone then says
    /// "three items went to the Recycle Bin" — true of the three, and silent
    /// about the nine that did not go and were never attempted. The log said
    /// `verified(true)` on exactly that. The number that was *asked for* and
    /// the fact that it stopped are both part of what happened.
    pub cancelled: bool,
}

impl HostDeleteOutcome {
    pub fn removed(&self) -> usize {
        self.rows.iter().filter(|row| row.removed).count()
    }

    pub fn failed(&self) -> usize {
        self.rows.len() - self.removed()
    }

    /// Names that were asked for and never attempted, because the pass
    /// stopped first. Zero unless [`cancelled`](Self::cancelled).
    pub fn untouched(&self) -> usize {
        self.asked.saturating_sub(self.rows.len())
    }

    /// Whether every name asked for was removed. The one condition that
    /// deserves an unqualified success, in the log or on screen.
    pub fn complete(&self) -> bool {
        !self.cancelled && self.failed() == 0 && self.rows.len() == self.asked
    }

    /// The names that did **not** go, in the order they were asked for.
    pub fn failed_names(&self) -> Vec<&str> {
        self.rows
            .iter()
            .filter(|row| !row.removed)
            .map(|row| row.name.as_str())
            .collect()
    }
}

/// Whether `path` is a **drive root** rather than a folder inside one.
///
/// `C:\`, `C:`, `\\server\share`, `/` — anything whose components are a
/// prefix and a root and nothing else. A folder one level down is not.
///
/// **This lives here and not in the screen** (ART-080 review, F5). It was
/// first written only in `movePlan.ts`, which made it a rule the command
/// could be reached around: `panel_delete_many` is a Tauri command like any
/// other, and a refusal that exists only in TypeScript is a refusal that
/// exists only when the TypeScript runs. This is the first operation in ART
/// that removes a file from the user's own disk, so its refusals belong where
/// nothing can route around them. The frontend keeps its own copy as an
/// *early answer* — it greys the key and explains itself before the user
/// clicks — never as the guarantee.
pub fn is_drive_root(path: &Path) -> bool {
    use std::path::Component;
    path.components()
        .all(|component| matches!(component, Component::Prefix(_) | Component::RootDir))
}

/// Refuse a drive root as the folder to delete from.
///
/// Separate from [`is_drive_root`] so the sentence is written once and every
/// caller refuses with the same words.
pub fn refuse_drive_root(parent: &Path) -> CoreResult<()> {
    if is_drive_root(parent) {
        return Err(CoreError::SafetyRefused(format!(
            "'{}' is a drive root, not a folder — ART will not delete from one",
            parent.display()
        )));
    }
    Ok(())
}

/// Resolve `names` inside `parent`, and send each to `recycler`.
///
/// **Every name goes through `safe_join`, and nothing else does.** The caller
/// supplies a directory and a list of names, never paths: that is what makes
/// it impossible for a selection — however it was assembled, and whatever a
/// hostile or merely stale frontend sent — to name a file outside the folder
/// the user is looking at. A name that escapes refuses **the whole pass**,
/// before one file is touched, rather than being skipped: a selection ART
/// cannot account for is not a selection to act on partially.
///
/// A name that is not there also refuses the whole pass, for the same reason
/// [`crate::core::volume::write`]'s own batch pre-check exists — the common
/// mistakes are reported before anything irreversible starts.
///
/// After that, each entry is its own unit: the host has no journal, so a
/// failure partway leaves the earlier ones gone and says so. Cancellation is
/// checked **between whole entries**, never during one.
///
/// Verified by asking the filesystem, not by trusting the recycler's word: a
/// `recycle` that returns `Ok` and leaves the file sitting there is the same
/// thing as a failure as far as the user's data is concerned, and the move
/// that calls this has already deleted nothing on the strength of a copy's
/// own word for exactly that reason.
pub fn recycle_many(
    recycler: &dyn HostRecycler,
    parent: &Path,
    names: &[String],
    sink: &dyn ProgressSink,
) -> CoreResult<HostDeleteOutcome> {
    // ---- refuse, resolve and pre-check, before anything is touched ----
    //
    // The drive root goes first: containment is *relative to `parent`*, so a
    // `parent` nobody should be deleting from makes every subsequent check
    // answer the right question about the wrong place.
    refuse_drive_root(parent)?;

    let mut resolved: Vec<(String, PathBuf)> = Vec::with_capacity(names.len());
    for name in names {
        let path = safe_join(parent, name).map_err(|err| {
            CoreError::SafetyRefused(format!(
                "'{name}' does not stay inside '{}': {err}",
                parent.display()
            ))
        })?;
        if !path.exists() {
            return Err(CoreError::InvalidInput(format!(
                "'{name}' is not in '{}' any more",
                parent.display()
            )));
        }
        resolved.push((name.clone(), path));
    }

    let total = resolved.len() as u64;
    let mut outcome = HostDeleteOutcome {
        asked: resolved.len(),
        ..HostDeleteOutcome::default()
    };

    for (done, (name, path)) in resolved.into_iter().enumerate() {
        // Between whole entries, never during one.
        if sink.is_cancelled() {
            // Not an error: what has gone has gone, and the caller needs to
            // be told which. A `Cancelled` here would throw away the only
            // record of it.
            //
            // **And it is marked as cancelled**, because an outcome that only
            // counts its own rows cannot tell "three of three" from "three of
            // twelve, stopped" — and the second one reported as the first is
            // a success ART did not have (review F1).
            outcome.cancelled = true;
            outcome.target = recycled_target(recycler, &outcome);
            return Ok(outcome);
        }
        sink.report(done as u64, Some(total), &name);

        let row = match recycler.recycle(&path) {
            Ok(()) if path.exists() => HostDeleteRow {
                name,
                removed: false,
                // The one failure mode a recycler can have without saying so.
                problem: Some("it is still there afterwards".to_string()),
            },
            Ok(()) => HostDeleteRow {
                name,
                removed: true,
                problem: None,
            },
            Err(err) => HostDeleteRow {
                name,
                removed: false,
                problem: Some(err.to_string()),
            },
        };
        outcome.rows.push(row);
    }

    sink.report(total, Some(total), "");
    outcome.target = recycled_target(recycler, &outcome);
    Ok(outcome)
}

/// The destination to report — only when something actually went there.
fn recycled_target(
    recycler: &dyn HostRecycler,
    outcome: &HostDeleteOutcome,
) -> Option<RecycleTarget> {
    (outcome.removed() > 0).then(|| recycler.target())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::jobs::NoProgress;
    use std::cell::RefCell;

    /// A recycler that records what it was asked for and really removes it —
    /// no COM, no Windows, which is the whole reason `HostRecycler` is a
    /// trait.
    struct FakeBin {
        seen: RefCell<Vec<PathBuf>>,
        /// A name to fail on, as the host would.
        fail_on: Option<String>,
        /// A name to claim success on and leave behind — the failure mode
        /// `recycle_many` catches by asking the filesystem rather than
        /// trusting the answer.
        lie_about: Option<String>,
    }

    impl FakeBin {
        fn new() -> Self {
            Self {
                seen: RefCell::new(Vec::new()),
                fail_on: None,
                lie_about: None,
            }
        }
    }

    impl HostRecycler for FakeBin {
        fn target(&self) -> RecycleTarget {
            RecycleTarget::WindowsRecycleBin
        }
        fn recycle(&self, path: &Path) -> CoreResult<()> {
            self.seen.borrow_mut().push(path.to_path_buf());
            let name = path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            if self.fail_on.as_deref() == Some(name.as_str()) {
                return Err(CoreError::InvalidInput("the file is in use".into()));
            }
            if self.lie_about.as_deref() == Some(name.as_str()) {
                return Ok(());
            }
            std::fs::remove_file(path)?;
            Ok(())
        }
    }

    fn scratch(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "art-hostfs-{tag}-{}",
            crate::core::test_scratch_id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn file(dir: &Path, name: &str) -> PathBuf {
        let path = dir.join(name);
        std::fs::write(&path, b"BYTES").unwrap();
        path
    }

    #[test]
    fn every_named_entry_goes_and_the_destination_is_reported() {
        let dir = scratch("all-go");
        file(&dir, "a.adf");
        file(&dir, "b.adf");

        let bin = FakeBin::new();
        let outcome = recycle_many(
            &bin,
            &dir,
            &["a.adf".to_string(), "b.adf".to_string()],
            &NoProgress,
        )
        .unwrap();

        assert_eq!(outcome.removed(), 2);
        assert_eq!(outcome.failed(), 0);
        assert_eq!(outcome.target, Some(RecycleTarget::WindowsRecycleBin));
        assert!(!dir.join("a.adf").exists());
        assert!(!dir.join("b.adf").exists());
    }

    /// The honest half of ART-080: a host filesystem has no journal, so this
    /// is not all-or-nothing — and the report names what did not go, because
    /// "one of three failed" is not something a user can act on.
    #[test]
    fn a_partial_failure_says_exactly_what_went_and_what_did_not() {
        let dir = scratch("partial");
        file(&dir, "a.adf");
        file(&dir, "locked.adf");
        file(&dir, "c.adf");

        let bin = FakeBin {
            fail_on: Some("locked.adf".to_string()),
            ..FakeBin::new()
        };
        let outcome = recycle_many(
            &bin,
            &dir,
            &[
                "a.adf".to_string(),
                "locked.adf".to_string(),
                "c.adf".to_string(),
            ],
            &NoProgress,
        )
        .unwrap();

        assert_eq!(outcome.removed(), 2);
        assert_eq!(outcome.failed_names(), vec!["locked.adf"]);
        // And the reason travels with it, in the host's own words.
        let row = outcome
            .rows
            .iter()
            .find(|row| row.name == "locked.adf")
            .unwrap();
        assert!(
            row.problem.as_deref().unwrap().contains("in use"),
            "{row:?}"
        );

        // One failure does not stop the others — the whole point of not
        // pretending this is a transaction.
        assert!(!dir.join("a.adf").exists());
        assert!(dir.join("locked.adf").is_file());
        assert!(!dir.join("c.adf").exists());
    }

    /// A recycler that says `Ok` and leaves the file there is a failure, and
    /// it is caught by asking the filesystem rather than by trusting the
    /// answer — the same rule F6's own VERIFY step follows before it deletes
    /// anything.
    #[test]
    fn a_recycler_that_claims_success_and_leaves_the_file_is_not_believed() {
        let dir = scratch("liar");
        file(&dir, "a.adf");

        let bin = FakeBin {
            lie_about: Some("a.adf".to_string()),
            ..FakeBin::new()
        };
        let outcome = recycle_many(&bin, &dir, &["a.adf".to_string()], &NoProgress).unwrap();

        assert_eq!(outcome.removed(), 0);
        assert_eq!(outcome.failed_names(), vec!["a.adf"]);
        assert_eq!(
            outcome.target, None,
            "nothing went anywhere, so nowhere is named"
        );
        assert!(dir.join("a.adf").is_file());
    }

    /// `safe_join`, and the reason the command takes a directory plus names
    /// rather than paths. Refused **before anything is touched**: a selection
    /// ART cannot account for is not one to act on partially.
    #[test]
    fn a_name_that_escapes_the_folder_refuses_the_whole_pass() {
        let dir = scratch("escape");
        let inside = file(&dir, "a.adf");
        let outside = dir.parent().unwrap().join("art-hostfs-escape-target.adf");
        std::fs::write(&outside, b"NOT YOURS").unwrap();

        let bin = FakeBin::new();
        let result = recycle_many(
            &bin,
            &dir,
            &[
                "a.adf".to_string(),
                "../art-hostfs-escape-target.adf".to_string(),
            ],
            &NoProgress,
        );

        // **The filesystem is asserted before the error is** — ART-144 #5's
        // own lesson, applied to the test that was written knowing it.
        // `unwrap_err()` on the line above would panic the moment the guard
        // came out, and every assertion after it would be unreachable: the
        // test could only ever fail for one reason, and it would not be this
        // one. Asked this way round, removing `safe_join` fails *here*,
        // naming the file it recycled.
        assert!(
            outside.is_file(),
            "an unguarded join recycles exactly this: {}",
            outside.display()
        );
        // ...and so is the legitimate one, because the pass refused whole.
        assert!(inside.is_file());
        assert!(bin.seen.borrow().is_empty(), "nothing was even attempted");
        assert!(
            matches!(result, Err(CoreError::SafetyRefused(_))),
            "{result:?}"
        );

        let _ = std::fs::remove_file(&outside);
    }

    /// An absolute path is a name that escapes, and is refused by the same
    /// gate — `safe_join` rejects absolute paths and Windows prefixes
    /// outright.
    #[test]
    fn an_absolute_path_is_refused_the_same_way() {
        let dir = scratch("absolute");
        file(&dir, "a.adf");

        let bin = FakeBin::new();
        let result = recycle_many(
            &bin,
            &dir,
            &[r"C:\Windows\System32\kernel32.dll".to_string()],
            &NoProgress,
        );

        // Nothing was even attempted, asserted first for the reason the test
        // above gives. Without the guard this pass reaches the real shell and
        // asks it to recycle `kernel32.dll` — which it refuses with
        // `os error 5`, but only because Windows says no, not because ART did.
        assert!(bin.seen.borrow().is_empty());
        assert!(
            matches!(result, Err(CoreError::SafetyRefused(_))),
            "{result:?}"
        );
    }

    /// A name that is not there refuses the whole pass rather than being
    /// skipped — the same pre-check `delete_many` runs against a read-only
    /// mount before its writer session opens, and for the same reason: the
    /// common mistakes are reported before anything irreversible starts.
    #[test]
    fn a_name_that_is_not_there_refuses_before_anything_is_touched() {
        let dir = scratch("missing");
        file(&dir, "a.adf");

        let bin = FakeBin::new();
        let result = recycle_many(
            &bin,
            &dir,
            &["a.adf".to_string(), "gone.adf".to_string()],
            &NoProgress,
        );

        assert!(dir.join("a.adf").is_file(), "the pass refused whole");
        assert!(bin.seen.borrow().is_empty());
        assert!(
            matches!(result, Err(CoreError::InvalidInput(_))),
            "{result:?}"
        );
    }

    /// Cancelling stops between entries and **keeps the record** of what had
    /// already gone. A `Cancelled` error here would throw away the only
    /// account of an irreversible act.
    #[test]
    fn cancelling_reports_what_had_already_gone() {
        use std::sync::atomic::{AtomicBool, Ordering};

        struct AfterFirst {
            cancelled: AtomicBool,
        }
        impl ProgressSink for AfterFirst {
            fn report(&self, _done: u64, _total: Option<u64>, _label: &str) {
                self.cancelled.store(true, Ordering::SeqCst);
            }
            fn is_cancelled(&self) -> bool {
                self.cancelled.load(Ordering::SeqCst)
            }
        }

        let dir = scratch("cancel");
        file(&dir, "a.adf");
        file(&dir, "b.adf");

        let bin = FakeBin::new();
        let outcome = recycle_many(
            &bin,
            &dir,
            &["a.adf".to_string(), "b.adf".to_string()],
            &AfterFirst {
                cancelled: AtomicBool::new(false),
            },
        )
        .unwrap();

        assert_eq!(outcome.rows.len(), 1, "{outcome:?}");
        assert_eq!(outcome.removed(), 1);
        assert!(!dir.join("a.adf").exists());
        assert!(
            dir.join("b.adf").is_file(),
            "the second was never attempted"
        );
        // **And it says it was cut short** (review F1). Without these three
        // the outcome is indistinguishable from a complete one-file delete,
        // and the log recorded exactly that: `verified(true)`, "1 item went to
        // the Recycle Bin", silent about the one that did not.
        assert!(outcome.cancelled, "a stopped pass has to say so");
        assert_eq!(outcome.asked, 2, "what was asked for, not what was reached");
        assert_eq!(outcome.untouched(), 1);
        assert!(
            !outcome.complete(),
            "a cancelled pass is never a complete one, however many rows succeeded"
        );
    }

    /// The other side of the same coin: a pass that really did everything is
    /// `complete()`, so the honest report above cannot be bought by making
    /// every pass look partial.
    #[test]
    fn a_pass_that_removed_everything_is_complete() {
        let dir = scratch("complete");
        file(&dir, "a.adf");
        file(&dir, "b.adf");

        let bin = FakeBin::new();
        let outcome = recycle_many(
            &bin,
            &dir,
            &["a.adf".to_string(), "b.adf".to_string()],
            &NoProgress,
        )
        .unwrap();

        assert!(outcome.complete());
        assert!(!outcome.cancelled);
        assert_eq!(outcome.asked, 2);
        assert_eq!(outcome.untouched(), 0);
    }

    /// A pass that reached every name and lost one is **not** complete either
    /// — `complete()` is about the whole request, not about having tried.
    #[test]
    fn a_pass_that_reached_everything_and_failed_one_is_not_complete() {
        let dir = scratch("incomplete");
        file(&dir, "a.adf");
        file(&dir, "locked.adf");

        let bin = FakeBin {
            fail_on: Some("locked.adf".to_string()),
            ..FakeBin::new()
        };
        let outcome = recycle_many(
            &bin,
            &dir,
            &["a.adf".to_string(), "locked.adf".to_string()],
            &NoProgress,
        )
        .unwrap();

        assert!(!outcome.complete());
        assert!(!outcome.cancelled, "it was not stopped — it failed");
        assert_eq!(outcome.untouched(), 0, "every name was reached");
    }

    // ---- review F5: the drive-root refusal, in Rust ----

    /// A drive root is refused **here**, not only in the screen.
    ///
    /// The refusal was first written only in `movePlan.ts`, which made it a
    /// rule the command could be reached around — `panel_delete_many` is a
    /// Tauri command like any other, and `recycle_many` is what it calls.
    /// This is the test that would have caught that.
    #[test]
    fn a_drive_root_is_refused_before_anything_is_resolved() {
        let bin = FakeBin::new();
        // Every spelling of "a root" this codebase can meet: a drive with
        // and without its trailing separator, a **UNC share root** (which is
        // a root even though it has two names in it), and the POSIX one that
        // `core/` must still answer correctly because `core/` is not allowed
        // to know it is on Windows.
        for root in [r"C:\", "C:", r"\\server\share", "/"] {
            let result = recycle_many(
                &bin,
                Path::new(root),
                &["anything.txt".to_string()],
                &NoProgress,
            );
            assert!(bin.seen.borrow().is_empty(), "{root}");
            assert!(
                matches!(result, Err(CoreError::SafetyRefused(_))),
                "{root}: {result:?}"
            );
        }
    }

    /// ...and a folder inside a root is not a root, or the refusal would ban
    /// the only case this feature exists for.
    #[test]
    fn a_folder_inside_a_root_is_not_a_root() {
        assert!(is_drive_root(Path::new(r"C:\")));
        assert!(is_drive_root(Path::new("/")));
        assert!(!is_drive_root(Path::new(r"C:\downloads")));
        assert!(!is_drive_root(Path::new(r"C:\downloads\amiga")));
        assert!(!is_drive_root(Path::new("/home/user")));
        // A relative path names no root at all.
        assert!(!is_drive_root(Path::new("downloads")));
    }

    /// The refusal is checked **before** containment, because containment is
    /// relative to the parent: a `parent` nobody should delete from makes
    /// every later check answer the right question about the wrong place.
    #[test]
    fn the_root_refusal_comes_before_the_containment_check() {
        let bin = FakeBin::new();
        // A name that would *also* fail `safe_join`. If containment ran
        // first, the error would be about the name; it is about the folder.
        let result = recycle_many(
            &bin,
            Path::new(r"C:\"),
            &["../escape.txt".to_string()],
            &NoProgress,
        );
        match result {
            Err(CoreError::SafetyRefused(message)) => {
                assert!(
                    message.contains("drive root"),
                    "the folder is refused first, not the name: {message}"
                );
            }
            other => panic!("{other:?}"),
        }
    }

    /// Nothing removed names no destination — a delete that did not happen
    /// must not be reported as having gone somewhere.
    #[test]
    fn nothing_removed_names_nowhere() {
        let dir = scratch("nowhere");
        let bin = FakeBin::new();
        let outcome = recycle_many(&bin, &dir, &[], &NoProgress).unwrap();
        assert_eq!(outcome.rows, vec![]);
        assert_eq!(outcome.target, None);
    }
}
