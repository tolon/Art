//! Writing `igame.data` into the user's **own** collection — the explicit half.
//!
//! [`super::igame::write_beside`] is the quiet path: nothing of the user's is
//! touched, because ART built that tree moments ago. This module is the other
//! one. The 893 drawers a real collection scan finds are 893 files somebody
//! else's tools may already have written into, on their own disk, and that
//! takes CLAUDE.md's mandatory pipeline: SOURCE → ANALYZE → VALIDATE →
//! RECOMMEND → PREVIEW → BACKUP → APPLY → VERIFY → REPORT.
//!
//! [`plan`] is ANALYZE/VALIDATE/RECOMMEND: it turns a list of catalogued
//! titles into what can actually be written and what must be refused, before
//! anything is touched — the PREVIEW the command layer shows on screen.
//! [`apply`] is BACKUP/APPLY/VERIFY/REPORT: it does the writing, one drawer at
//! a time, and hands back a verdict **per entry**, never a count. A host
//! filesystem has no journal — nine written and one failed is nine completed
//! operations, exactly the rule `core::hostfs::recycle_many` already follows
//! for the same reason.
//!
//! # Only an unpacked drawer is a route in
//!
//! [`Media::WhdloadDrawer`] is a directory on the host; `write_beside` can put
//! a file in it. Everything else is refused **by name**, with what to do
//! about it — an actionable refusal is the whole point (CLAUDE.md, "the
//! failure that does not crash"):
//!
//! - [`Media::WhdloadArchive`] is a drawer still inside a compressed file
//!   ART has not unpacked; the refusal names the archive and says to unpack
//!   it first.
//! - [`Media::Floppies`], [`Media::Hardfile`] and [`Media::WhdloadHardfile`]
//!   have no folder on the host to write beside at all — writing into a
//!   hardfile's own volume is `igame.rs`'s documented route 1, and it is not
//!   built yet.
//!
//! # `Skipped` is a real ending, not a placeholder
//!
//! A second run over a drawer `apply` already wrote is not a second `Merged`:
//! nothing changed, so nothing was backed up and nothing was rewritten, and
//! saying "merged" would be the exact defect CLAUDE.md names as this
//! project's most expensive — a confident sentence about something that did
//! not happen. [`apply_one`] computes what `write_beside` *would* write and
//! compares it against what is already there before touching anything; when
//! they match, the verdict is [`IGameState::Skipped`] and the file is left
//! alone. `a_second_run_with_nothing_new_is_skipped_not_merged_and_nothing_is_touched`
//! is the test.

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::core::gameindex::igame::{self, IGameData, WriteOutcome};
use crate::core::gameindex::record::{GameRecord, Media};
use crate::core::jobs::ProgressSink;
use crate::core::safety::BackupPolicy;

/// One title `plan` found a real route for.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IGamePlanItem {
    /// The drawer `igame.data` will land beside the slave in.
    pub dir: String,
    /// For the preview screen; not used to decide anything.
    pub title: String,
    pub data: IGameData,
}

/// What `plan` found, before anything is touched.
///
/// Both halves are always present, the same rule [`crate::core::hostfs::HostDeleteOutcome`]
/// follows: a plan that only reported what it *could* do would be silent
/// about the titles it cannot help with.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IGamePlan {
    pub items: Vec<IGamePlanItem>,
    /// One English sentence per title `plan` could not route — naming the
    /// title and, for an archive, naming it and what to do (ART-060: this
    /// stays English the same way a `CoreError` message does; it is not
    /// rendered through i18n).
    pub refusals: Vec<String>,
}

/// What one drawer's write settled on.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", content = "detail", rename_all = "kebab-case")]
pub enum IGameState {
    /// Nothing was there before; a fresh `igame.data` was written.
    Written,
    /// A file was already there and its managed keys were rewritten in place.
    Merged,
    /// A file was already there and it already said exactly what ART would
    /// write — so nothing was touched. English detail (ART-060).
    Skipped(String),
    /// The write did not happen. English detail (ART-060) — the same
    /// `CoreError::to_string()` a log entry would carry.
    Failed(String),
}

/// What happened to one drawer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IGameVerdict {
    pub dir: String,
    pub state: IGameState,
    /// Where the previous `igame.data` went, when one existed and was
    /// changed. `None` for `Written` (nothing to back up) and for `Skipped`
    /// (nothing changed).
    pub backup: Option<String>,
    /// What ART knew about this title but could not put in the file — a
    /// title too long for iGame's line, or a value iGame itself refuses.
    /// English (ART-060), the same as `state`'s own detail string. Empty for
    /// `Failed` (the write never got far enough to know) and whenever
    /// nothing was left out.
    pub omitted: Vec<String>,
}

/// What a whole run did.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IGameOutcome {
    pub verdicts: Vec<IGameVerdict>,
    /// Whether the user stopped it before every planned item was reached.
    /// Without this, nine verdicts out of a twelve-item plan reads as a
    /// plan that only ever had nine items in it.
    pub cancelled: bool,
}

/// What ART knows about one title, in iGame's own vocabulary.
///
/// **`players` is always absent.** `GameRecord` carries no player-count field
/// anywhere in ART's catalogue today (the same gap `commands/whdload.rs`'s
/// `igame_data_for_pack` already documents), so there is nothing here for
/// this to find.
fn igame_data_for(record: &GameRecord) -> IGameData {
    IGameData {
        title: Some(record.title.value.clone()),
        chipset: record
            .chipset
            .as_ref()
            .map(|fact| fact.value.display_name().to_string()),
        genre: record.genre.as_ref().map(|fact| fact.value.clone()),
        year: record.year.as_ref().map(|fact| fact.value),
        players: None,
        exe: None,
    }
}

/// ANALYZE / VALIDATE / RECOMMEND: turn catalogued titles into what can be
/// written and what must be refused, before a single byte moves.
pub fn plan(records: &[GameRecord]) -> IGamePlan {
    let mut items = Vec::new();
    let mut refusals = Vec::new();

    for record in records {
        match &record.media {
            Media::WhdloadDrawer { dir, .. } => {
                items.push(IGamePlanItem {
                    dir: dir.clone(),
                    title: record.title.value.clone(),
                    data: igame_data_for(record),
                });
            }
            Media::WhdloadArchive { file, .. } => {
                // Named and actionable: which archive, and what to do about
                // it. ART's archive readers are read-only behind
                // `core/archive`'s one security gate — there is no writing
                // into one, ever.
                refusals.push(format!(
                    "\"{}\" is inside {file}, which ART has not unpacked — unpack the \
                     archive first; igame.data cannot be written into an archive",
                    record.title.value
                ));
            }
            Media::Floppies { .. } | Media::Hardfile { .. } | Media::WhdloadHardfile { .. } => {
                // Real, but not this route: none of these are a folder on the
                // host `write_beside` can put a file into. `igame.rs`'s own
                // doc calls this route 1 and says plainly it is not built.
                refusals.push(format!(
                    "\"{}\" is not an unpacked WHDLoad drawer on the host — igame.data can \
                     only be written beside a slave in a folder ART can edit",
                    record.title.value
                ));
            }
        }
    }

    IGamePlan { items, refusals }
}

/// One drawer's whole unit of work: read what is there, decide, write.
///
/// Never called with the cancel flag already set — `apply` checks that
/// **between** entries, so once this starts it always finishes.
///
/// **I3.** This used to read and render the file itself just to decide
/// `Skipped` versus proceed, then call [`igame::write_beside`] — which read
/// and rendered the same file again — and finally ignored what that call
/// returned in favour of its own `existed` flag for the `Merged`/`Written`
/// choice. `write_beside` now makes that whole decision from a single read,
/// so this is one call, and every field on the verdict — the ending, the
/// backup path, what was left out — comes from what it actually did rather
/// than being re-derived beside it.
fn apply_one(item: &IGamePlanItem) -> IGameVerdict {
    let dir = Path::new(&item.dir);
    match igame::write_beside(dir, &item.data, BackupPolicy::CONFIG) {
        Ok(written) => {
            let state = match written.outcome {
                WriteOutcome::Written => IGameState::Written,
                WriteOutcome::Merged => IGameState::Merged,
                WriteOutcome::AlreadyCurrent => IGameState::Skipped(
                    "igame.data already says this; nothing was changed".to_string(),
                ),
                // I2: nothing ART knows about this title would survive
                // iGame's own rules — every field was empty, too long, or
                // refused. `write_beside` already refused to write an empty
                // file for it; this is that refusal's own sentence, not
                // "written" or "merged" about a file that says nothing.
                WriteOutcome::NothingFit => IGameState::Skipped(
                    "none of what ART knows about this title would fit iGame's 64-byte line; \
                     nothing was written"
                        .to_string(),
                ),
            };
            IGameVerdict {
                dir: item.dir.clone(),
                state,
                backup: written.backup,
                omitted: igame::notable_omissions(&written.omitted),
            }
        }
        // N-1: a write that fails after a successful backup must still say
        // where the backup went — `failure.backup` carries that forward
        // rather than the `None` this arm used to hard-code regardless.
        Err(failure) => IGameVerdict {
            dir: item.dir.clone(),
            state: IGameState::Failed(failure.error.to_string()),
            backup: failure.backup,
            omitted: Vec::new(),
        },
    }
}

/// BACKUP / APPLY / VERIFY / REPORT: write every planned item, one verdict
/// each.
///
/// **Cancellation is checked between entries, never mid-write** — each
/// drawer is a whole unit of work, which is what keeps a stopped run safe:
/// it can leave work unfinished, never a half-written `igame.data`.
///
/// Never fails outright: a plan's own refusals were already settled by
/// [`plan`], and every remaining item gets its own verdict, however it goes.
pub fn apply(plan: &IGamePlan, progress: &dyn ProgressSink) -> IGameOutcome {
    let total = plan.items.len() as u64;
    let mut verdicts = Vec::with_capacity(plan.items.len());
    let mut cancelled = false;

    for (done, item) in plan.items.iter().enumerate() {
        if progress.is_cancelled() {
            cancelled = true;
            break;
        }
        progress.report(done as u64, Some(total), &item.dir);
        verdicts.push(apply_one(item));
    }

    IGameOutcome {
        verdicts,
        cancelled,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::gameindex::readers::drawer::read_drawer;
    use crate::core::gameindex::readers::slave::tests_support::build_slave;
    use crate::core::gameindex::record::{
        derive_id, Fact, Provenance, SourceRef, GAMEINDEX_SCHEMA,
    };
    use crate::core::jobs::NoProgress;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicBool, Ordering};

    fn scratch(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "art-igamewrite-{tag}-{}",
            crate::core::test_scratch_id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// A minimal real drawer: one slave, no icon, nothing ambiguous.
    fn synthetic_drawer(root: &Path, name: &'static str, slave_file: &str) -> PathBuf {
        let dir = root.join(name);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(slave_file), build_slave(name, "1992 Someone", 16)).unwrap();
        dir
    }

    /// A drawer whose `igame.data` path is occupied by a directory, so any
    /// write to it fails: `atomic_write` renames a temp file *onto* `path`,
    /// and a rename onto an existing directory fails on every platform ART
    /// ships for. Chosen over a permissions trick specifically because it is
    /// portable rather than relying on a Windows ACL or a Unix mode bit that
    /// behaves differently across CI and a developer's own machine.
    fn unwritable_drawer(root: &Path, name: &'static str, slave_file: &str) -> PathBuf {
        let dir = synthetic_drawer(root, name, slave_file);
        std::fs::create_dir(dir.join(igame::FILE_NAME)).unwrap();
        dir
    }

    fn drawer_record(dir: &Path) -> GameRecord {
        read_drawer(dir)
            .unwrap()
            .expect("a synthetic drawer with a slave is a title")
    }

    /// A title `readers::lhadrawer` would have produced, built directly:
    /// there is no shared fixture for a real `.lha`, and this test needs
    /// nothing more than the `Media::WhdloadArchive` shape and a title.
    fn archived_record(file: &str, inner: &str, slave: &str) -> GameRecord {
        let title = inner.rsplit('/').next().unwrap_or(inner).to_string();
        let sha256 = "0".repeat(64);
        GameRecord {
            schema: GAMEINDEX_SCHEMA,
            id: derive_id(&title, &sha256),
            title: Fact::new(title, Provenance::DrawerName),
            kind: None,
            year: None,
            publisher: None,
            genre: None,
            rating: None,
            chipset: None,
            kickstart: None,
            media: Media::WhdloadArchive {
                file: file.to_string(),
                inner: inner.to_string(),
                slave: slave.to_string(),
            },
            preview: None,
            source: SourceRef {
                name: file.to_string(),
                sha256,
                bytes: 0,
            },
        }
    }

    // -- the brief's own tests, verbatim in substance ----------------------

    #[test]
    fn an_archived_title_is_refused_and_the_refusal_names_the_archive() {
        let record = archived_record("WHDLoadDemos100.lha", "Demos/T/Tag", "Tag.Slave");
        let plan = plan(&[record]);
        let refusal = plan
            .refusals
            .first()
            .expect("an archive cannot be written into");
        assert!(
            refusal.contains("WHDLoadDemos100.lha") && refusal.to_lowercase().contains("unpack"),
            "the refusal names the archive and says what to do, got: {refusal}"
        );
        assert!(
            plan.items.is_empty(),
            "nothing may be planned against an archive"
        );
    }

    #[test]
    fn every_drawer_gets_its_own_verdict() {
        let root = scratch("many");
        let a = synthetic_drawer(&root, "One", "One.slave");
        let b = synthetic_drawer(&root, "Two", "Two.slave");
        let outcome = apply(&plan(&[drawer_record(&a), drawer_record(&b)]), &NoProgress);
        assert_eq!(
            outcome.verdicts.len(),
            2,
            "two drawers is two results, never one number"
        );
        assert!(outcome
            .verdicts
            .iter()
            .all(|v| matches!(v.state, IGameState::Written)));
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn a_backup_is_taken_before_an_existing_file_is_changed() {
        let root = scratch("backup");
        let dir = synthetic_drawer(&root, "Kept", "Kept.slave");
        std::fs::write(dir.join(igame::FILE_NAME), "favourite=yes\n").unwrap();
        let outcome = apply(&plan(&[drawer_record(&dir)]), &NoProgress);
        let verdict = &outcome.verdicts[0];
        assert!(
            matches!(verdict.state, IGameState::Merged),
            "{:?}",
            verdict.state
        );
        assert!(
            verdict.backup.is_some(),
            "the user is told where their previous version went"
        );
        std::fs::remove_dir_all(&root).ok();
    }

    /// **N-1.** A `Failed` verdict must still say where the backup went when
    /// one was taken before the failure — the code did this before
    /// `write_beside` was collapsed into one writer, and losing it in that
    /// collapse is exactly the "given nothing" shape CLAUDE.md warns about.
    /// A read-only `igame.data` is readable (so the merge and the backup
    /// both succeed) but not renameable-over, which is what makes
    /// `atomic_write`'s own final step fail.
    #[test]
    fn a_failed_write_after_a_successful_backup_still_reports_it() {
        let root = scratch("failed-after-backup");
        let dir = synthetic_drawer(&root, "Kept", "Kept.slave");
        let igame_path = dir.join(igame::FILE_NAME);
        std::fs::write(&igame_path, "title=Old\n").unwrap();
        let mut perms = std::fs::metadata(&igame_path).unwrap().permissions();
        perms.set_readonly(true);
        std::fs::set_permissions(&igame_path, perms).unwrap();

        let outcome = apply(&plan(&[drawer_record(&dir)]), &NoProgress);
        let verdict = &outcome.verdicts[0];
        assert!(
            matches!(verdict.state, IGameState::Failed(_)),
            "{:?}",
            verdict.state
        );
        assert!(
            verdict.backup.is_some(),
            "a backup was taken before the write failed; it must still be reported: {verdict:?}"
        );

        // Test cleanup on Windows only (CI is Windows x64) —
        // `set_readonly(false)` there only clears the DOS read-only
        // attribute this test itself set, not Unix's `S_IWOTH`.
        let mut perms = std::fs::metadata(&igame_path).unwrap().permissions();
        #[allow(clippy::permissions_set_readonly_false)]
        perms.set_readonly(false);
        std::fs::set_permissions(&igame_path, perms).unwrap();
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn one_failure_does_not_stop_the_rest() {
        // A host filesystem has no journal: nine written and one failed is
        // nine completed operations, and the report says so per entry.
        let root = scratch("partial");
        let ok = synthetic_drawer(&root, "Fine", "Fine.slave");
        let bad = unwritable_drawer(&root, "Locked", "Locked.slave");
        let outcome = apply(
            &plan(&[drawer_record(&ok), drawer_record(&bad)]),
            &NoProgress,
        );
        assert_eq!(outcome.verdicts.len(), 2);
        assert!(outcome
            .verdicts
            .iter()
            .any(|v| matches!(v.state, IGameState::Written)));
        assert!(outcome
            .verdicts
            .iter()
            .any(|v| matches!(v.state, IGameState::Failed(_))));
        std::fs::remove_dir_all(&root).ok();
    }

    /// The order-sensitive half of the mutation above. With the failure
    /// *last*, "process everything" and "stop as soon as one entry fails"
    /// look identical — there is nothing left to stop before. Putting the
    /// failure **first** is what actually distinguishes them: an abort-on-
    /// failure mutant never reaches the entry that comes after it.
    #[test]
    fn a_failure_before_a_later_success_does_not_stop_that_one_either() {
        let root = scratch("partial-reversed");
        let bad = unwritable_drawer(&root, "Locked", "Locked.slave");
        let ok = synthetic_drawer(&root, "Fine", "Fine.slave");
        let outcome = apply(
            &plan(&[drawer_record(&bad), drawer_record(&ok)]),
            &NoProgress,
        );
        assert_eq!(
            outcome.verdicts.len(),
            2,
            "the entry after the failure must still be reached"
        );
        assert!(outcome
            .verdicts
            .iter()
            .any(|v| matches!(v.state, IGameState::Written)));
        assert!(outcome
            .verdicts
            .iter()
            .any(|v| matches!(v.state, IGameState::Failed(_))));
        std::fs::remove_dir_all(&root).ok();
    }

    // -- this task's own ruling on `Skipped` --------------------------------

    /// The legitimate producer of `Skipped`: a second run over a drawer that
    /// already says exactly what ART would write. Calling that "merged" would
    /// claim an edit that never happened — the "failure that does not crash"
    /// CLAUDE.md warns about, arriving here as a false "changed" rather than
    /// a false "worked".
    #[test]
    fn a_second_run_with_nothing_new_is_skipped_not_merged_and_nothing_is_touched() {
        let root = scratch("idempotent");
        let dir = synthetic_drawer(&root, "Twice", "Twice.slave");
        let record = drawer_record(&dir);

        let first = apply(&plan(std::slice::from_ref(&record)), &NoProgress);
        assert!(matches!(first.verdicts[0].state, IGameState::Written));
        let written = std::fs::read_to_string(dir.join(igame::FILE_NAME)).unwrap();

        let second = apply(&plan(&[record]), &NoProgress);
        assert!(
            matches!(second.verdicts[0].state, IGameState::Skipped(_)),
            "nothing changed, so this is not a second Written or a Merged: {:?}",
            second.verdicts[0].state
        );
        assert!(
            second.verdicts[0].backup.is_none(),
            "nothing changed, so nothing was backed up"
        );
        assert_eq!(
            std::fs::read_to_string(dir.join(igame::FILE_NAME)).unwrap(),
            written,
            "the file itself is untouched"
        );
        std::fs::remove_dir_all(&root).ok();
    }

    /// The fixture actually distinguishes the two states: an unrelated first
    /// change is still `Written`/`Merged` and only the exact repeat is
    /// `Skipped` — otherwise this fixture could not tell "nothing to do" from
    /// "always skips".
    #[test]
    fn a_real_change_after_a_skip_is_still_applied() {
        let root = scratch("skip-then-change");
        let dir = synthetic_drawer(&root, "Changes", "Changes.slave");
        let mut record = drawer_record(&dir);
        apply(&plan(std::slice::from_ref(&record)), &NoProgress);
        let repeat = apply(&plan(std::slice::from_ref(&record)), &NoProgress);
        assert!(matches!(repeat.verdicts[0].state, IGameState::Skipped(_)));

        record.genre = Some(Fact::new("Puzzle".to_string(), Provenance::UserEdit));
        let changed = apply(&plan(&[record]), &NoProgress);
        assert!(
            matches!(changed.verdicts[0].state, IGameState::Merged),
            "a real change is still applied: {:?}",
            changed.verdicts[0].state
        );
        std::fs::remove_dir_all(&root).ok();
    }

    /// **I2's own fix, from this module's side.** A title too long to fit and
    /// nothing else known produces `WriteOutcome::NothingFit` inside
    /// `write_beside`; this module must map that to `Skipped`, name what did
    /// not fit, and never let `igame.data` appear on disk with nothing in it.
    #[test]
    fn a_title_that_does_not_fit_is_skipped_and_writes_no_file() {
        let root = scratch("nothing-fits");
        let dir = synthetic_drawer(&root, "Long", "Long.slave");
        let mut record = drawer_record(&dir);
        record.title = Fact::new("x".repeat(200), Provenance::UserEdit);
        // `synthetic_drawer`'s fixture slave states a copyright ("1992
        // Someone"), which `read_drawer` turns into a year fact — leaving it
        // in place would let `year=1992` fit and the file would still be
        // written, just missing its title. Nulled here so nothing at all
        // survives, which is the case this test is actually about.
        record.year = None;

        let outcome = apply(&plan(&[record]), &NoProgress);
        let verdict = &outcome.verdicts[0];
        assert!(
            matches!(verdict.state, IGameState::Skipped(_)),
            "nothing survived to write; this must not read as Written: {:?}",
            verdict.state
        );
        assert!(
            !verdict.omitted.is_empty(),
            "the title that did not fit must be named, not just silently dropped"
        );
        assert!(
            !dir.join(igame::FILE_NAME).exists(),
            "an empty igame.data is worse than none: it reads as a written file"
        );
        std::fs::remove_dir_all(&root).ok();
    }

    // -- other media kinds are refused too, not silently dropped -----------

    #[test]
    fn a_title_with_no_host_folder_is_refused_by_name() {
        let record = GameRecord {
            schema: GAMEINDEX_SCHEMA,
            id: derive_id("Some Hardfile Game", "abc"),
            title: Fact::new("Some Hardfile Game".to_string(), Provenance::WhdloadSlave),
            kind: None,
            year: None,
            publisher: None,
            genre: None,
            rating: None,
            chipset: None,
            kickstart: None,
            media: Media::WhdloadHardfile {
                file: "Game.hdf".to_string(),
                slave: "Game.slave".to_string(),
            },
            preview: None,
            source: SourceRef {
                name: "Game.hdf".to_string(),
                sha256: "abc".to_string(),
                bytes: 0,
            },
        };
        let plan = plan(&[record]);
        assert!(plan.items.is_empty());
        assert_eq!(plan.refusals.len(), 1);
        assert!(plan.refusals[0].contains("Some Hardfile Game"));
    }

    // -- cancellation is between entries, never mid-write -------------------

    struct CancelAfterOne {
        seen: AtomicBool,
    }

    impl ProgressSink for CancelAfterOne {
        fn report(&self, _done: u64, _total: Option<u64>, _message: &str) {}
        fn is_cancelled(&self) -> bool {
            // False the first time it is asked, true from then on — so the
            // first entry runs to completion and the second is never
            // started, exactly the "between whole entries" rule.
            self.seen.swap(true, Ordering::SeqCst)
        }
    }

    #[test]
    fn stopping_leaves_the_untouched_entries_untouched_and_says_so() {
        let root = scratch("cancel");
        let a = synthetic_drawer(&root, "First", "First.slave");
        let b = synthetic_drawer(&root, "Second", "Second.slave");
        let sink = CancelAfterOne {
            seen: AtomicBool::new(false),
        };
        let outcome = apply(&plan(&[drawer_record(&a), drawer_record(&b)]), &sink);
        assert!(outcome.cancelled);
        assert_eq!(
            outcome.verdicts.len(),
            1,
            "one entry ran before the stop was seen"
        );
        assert!(
            !b.join(igame::FILE_NAME).exists(),
            "the second drawer was never touched"
        );
        std::fs::remove_dir_all(&root).ok();
    }
}
