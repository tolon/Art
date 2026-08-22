//! Package bundle commands — the adapter over `core/sources/bundle`.
//!
//! Two commands only. `bundles_list` is read-only: it parses the shipped
//! catalogue JSON and answers, fetching nothing — a screen can call it on
//! mount with no consequence. `bundles_download` is the one thing here that
//! touches the network, and it never runs on its own: it takes the ids the
//! user ticked, refuses an empty or unknown selection before a job is even
//! started, and then walks `core::sources::bundle::run::download_entries` on
//! a job thread, the same shape `commands/sources.rs::sources_fetch` and
//! `commands/osinstall.rs` already use.
//!
//! `SourcesState` already holds the configured Aminet mirrors, the download
//! cache and the library root — this module reuses all three rather than
//! opening a second copy of any of them.

use std::sync::Arc;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};

use crate::core::error::{CoreError, CoreResult};
use crate::core::jobs::JobId;
use crate::core::oplog::{JsonlOperationLog, OperationOutcome};
use crate::core::sources::bundle::run::{
    download_entries, BundleReport, DownloadContext, EntryOutcome,
};
use crate::core::sources::bundle::{self, BundleEntry, EntrySource, Permission};
use crate::error::AppResult;

use super::jobs::{spawn_job, JobRegistry};
use super::oplog::{user_operation, write_to_path};
use super::sources::SourcesState;

/// The event a finished bundle download arrives on.
pub const BUNDLE_DOWNLOAD_EVENT: &str = "bundle-download-result";

/// One entry, in the shape the checklist screen needs — never the whole
/// [`BundleEntry`], whose `source` carries the repository path or search
/// query the screen has no use for and would only be able to leak (§41.5.7:
/// nothing here is a URL, but a raw Aminet path is still more than the
/// checklist needs to render a row).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EntrySummary {
    pub id: String,
    pub name: String,
    /// The [`EntrySource`] tag — `"aminet"`, `"aminet-search"`,
    /// `"github-release"`, `"mirror"` or `"user-supplied"` — so the screen
    /// can show where a file comes from without seeing the path itself.
    pub kind: &'static str,
    pub permission: Option<Permission>,
    /// Task 7's own addition — `bundles_list` shipped without it, and the
    /// screen needs it to render `tolunnet`/`miamidx` and the two Directory
    /// Opus builds as alternatives (shown, never enforced: downloading both
    /// is a legitimate thing to want). Same field `BundleEntry` already
    /// carries; `camelCase` here matches `ComponentDef::exclusiveGroup` in
    /// `commands/osinstall.rs`, the same shape on the OS Builder's side.
    pub exclusive_group: Option<String>,
}

impl From<&BundleEntry> for EntrySummary {
    fn from(entry: &BundleEntry) -> Self {
        Self {
            id: entry.id.clone(),
            name: entry.name.clone(),
            kind: entry_kind(&entry.source),
            permission: entry.permission.clone(),
            exclusive_group: entry.exclusive_group.clone(),
        }
    }
}

/// The same tag [`EntrySource`]'s own `#[serde(rename_all = "kebab-case")]`
/// would produce, spelled out by hand rather than serialised through it —
/// `EntrySummary` never carries the `EntrySource` itself (see its own doc
/// comment), so there is nothing to serialise the tag *of*.
fn entry_kind(source: &EntrySource) -> &'static str {
    match source {
        EntrySource::Aminet { .. } => "aminet",
        EntrySource::AminetSearch { .. } => "aminet-search",
        EntrySource::GithubRelease { .. } => "github-release",
        EntrySource::Mirror { .. } => "mirror",
        EntrySource::UserSupplied { .. } => "user-supplied",
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct BundleSummary {
    pub id: String,
    pub order: u32,
    pub entries: Vec<EntrySummary>,
}

/// Every shipped set, with its entries. Read-only: parses shipped JSON,
/// opens no media and fetches nothing — nothing here may download on load or
/// on listing.
#[tauri::command]
pub fn bundles_list() -> AppResult<Vec<BundleSummary>> {
    Ok(bundle::bundles()?
        .into_iter()
        .map(|b| BundleSummary {
            id: b.id,
            order: b.order,
            entries: b.entries.iter().map(EntrySummary::from).collect(),
        })
        .collect())
}

/// Resolve every chosen id against the shipped catalogue.
///
/// Refuses an empty selection — a job that downloads nothing and reports
/// success is a mistake worth naming, not a no-op — and refuses any id ART
/// does not ship, by name, rather than silently dropping it: a caller
/// sending a stale or typo'd id must learn which one, not just that
/// something in its list was wrong.
fn entry_ids_or_refuse(ids: &[String]) -> CoreResult<Vec<BundleEntry>> {
    if ids.is_empty() {
        return Err(CoreError::InvalidInput("no packages were chosen".into()));
    }
    let catalogue = bundle::entries()?;
    ids.iter()
        .map(|id| {
            catalogue
                .iter()
                .find(|entry| &entry.id == id)
                .cloned()
                .ok_or_else(|| {
                    CoreError::InvalidInput(format!("'{id}' is not a package ART ships"))
                })
        })
        .collect()
}

/// What a finished bundle download delivers to the UI.
#[derive(Debug, Clone, Serialize)]
pub struct BundleDownloadResult {
    pub job_id: JobId,
    pub report: BundleReport,
}

/// Download the chosen entries, in the catalogue's own download order.
///
/// Returns a job id; the report arrives on [`BUNDLE_DOWNLOAD_EVENT`]. Nothing
/// is fetched until this is called — `bundles_list` never triggers it, and
/// this command itself refuses before starting a job at all when the
/// selection is empty or names something ART does not ship.
///
/// Downloads land in the user's existing download folder (no per-set
/// subfolder), the same destination `sources_fetch` places into — one
/// library, whichever door a file came through.
#[tauri::command]
pub fn bundles_download(
    app: AppHandle,
    state: State<'_, SourcesState>,
    oplog: State<'_, JsonlOperationLog>,
    registry: State<'_, Arc<JobRegistry>>,
    entry_ids: Vec<String>,
) -> AppResult<JobId> {
    let entries = entry_ids_or_refuse(&entry_ids)?;

    let aminet = state.provider().mirrors;
    let library = state.library();
    let log_path = oplog.path().to_path_buf();
    let registry = Arc::clone(&registry);
    let emit_app = app.clone();
    let title = format!(
        "Downloading {} package{}",
        entries.len(),
        if entries.len() == 1 { "" } else { "s" }
    );

    let id = spawn_job(&app, registry, &title, move |job_id, progress| {
        // The state is managed by Tauri and outlives every job, so the
        // worker reaches it through the handle rather than capturing a
        // borrow across the thread boundary — the same shape
        // `sources_fetch`'s own job body uses.
        let state = emit_app.state::<SourcesState>();
        let ctx = DownloadContext {
            aminet: &aminet,
            // ART ships no registry of named, non-Aminet mirrors yet — every
            // `EntrySource::Mirror` entry is refused by name until one
            // exists (`resolve.rs`'s own behaviour), rather than this
            // command inventing a second source of mirror configuration.
            configured: &[],
            client: state.client(),
            cache: state.cache(),
            library: &library,
            subfolder: "",
        };

        let (report, outcome) = run_and_report_cancellation(&entries, &ctx, progress);

        let record = user_operation("Download package set");
        record_bundle_download(&log_path, record, &entries, &report);

        // The report is emitted whether or not the run was cancelled — a
        // cancelled run still names what happened to every entry reached
        // before the cancel, and a user told only "cancelled" with no detail
        // has been told nothing (CLAUDE.md: "a user told 'it failed' without
        // being told where the evidence went has been given nothing").
        let _ = emit_app.emit(
            BUNDLE_DOWNLOAD_EVENT,
            BundleDownloadResult { job_id, report },
        );

        // Said only *after* the event above, so the job's own terminal state
        // (§54, §55) matches what the screen already shows.
        outcome
    });

    Ok(id)
}

/// Run the download, and decide the job's own terminal state from what
/// actually happened — factored out so it is testable without a live Tauri
/// `AppHandle`, the same shape `install_archive_into_volume`
/// (`commands/sources.rs`) gives its own job body.
///
/// `Ok(())` becomes `JobState::Finished`
/// (`commands/jobs.rs::spawn_in_lane`); `Err(CoreError::Cancelled)` becomes
/// `JobState::Cancelled`. Reporting `Ok(())` over a run the user cancelled
/// would leave the job bar saying "finished" while the report right below it
/// says "skipped when you cancelled" — the screen out-claiming the core,
/// exactly the defect class CLAUDE.md names under "The failure that does not
/// crash". The sibling guard is `commands/sources.rs:1304`
/// ("the job must end Cancelled, not Completed"), which returns this same
/// variant; this follows it rather than inventing a second shape.
fn run_and_report_cancellation(
    entries: &[BundleEntry],
    ctx: &DownloadContext<'_>,
    sink: &dyn crate::core::jobs::ProgressSink,
) -> (BundleReport, CoreResult<()>) {
    let report = download_entries(entries, ctx, sink);
    // `download_entries` never returns an `Err` of its own — a cancelled run
    // marks the entries it never reached `Skipped` instead, so that is what
    // is checked here.
    let cancelled = report
        .entries
        .iter()
        .any(|entry| matches!(entry.outcome, EntryOutcome::Skipped));
    let outcome = if cancelled {
        Err(CoreError::Cancelled)
    } else {
        Ok(())
    };
    (report, outcome)
}

/// Record what a bundle download actually did — best-effort, and it must
/// never fail the download it describes (the same rule every other job body
/// in this codebase follows for its own oplog write).
///
/// `download_entries` never returns an `Err`: every entry gets its own typed
/// outcome instead (`EntryOutcome`), so there is no failure branch here to
/// mirror `record_fetch`'s — only counts to tally per kind.
fn record_bundle_download(
    log_path: &std::path::Path,
    record: crate::core::oplog::OperationRecord,
    entries: &[BundleEntry],
    report: &BundleReport,
) {
    let mut downloaded = 0usize;
    let mut already_have = 0usize;
    let mut not_placed = 0usize;
    let mut refused = 0usize;
    let mut failed = 0usize;
    let mut skipped = 0usize;
    for entry in &report.entries {
        match &entry.outcome {
            EntryOutcome::Downloaded { .. } => downloaded += 1,
            EntryOutcome::AlreadyHave { .. } => already_have += 1,
            EntryOutcome::NotPlaced { .. } => not_placed += 1,
            EntryOutcome::Refused { .. } => refused += 1,
            EntryOutcome::Failed { .. } => failed += 1,
            EntryOutcome::Skipped => skipped += 1,
        }
    }

    let record = record
        .detail("Chosen", entries.len().to_string())
        .detail("Downloaded", downloaded.to_string())
        .detail("Already had", already_have.to_string())
        .detail("Not placed (slot occupied)", not_placed.to_string())
        .detail("Refused", refused.to_string())
        .detail("Failed", failed.to_string())
        .detail("Cancelled/skipped", skipped.to_string())
        // Verified when at least one entry actually landed in the library —
        // freshly downloaded or already there. A run that only refused or
        // failed did not verify anything, and the log must not say it did.
        .outcome(OperationOutcome::verified(
            downloaded > 0 || already_have > 0,
        ));
    write_to_path(log_path, &record);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_shipped_set_is_listed_with_its_entries_and_their_kinds() {
        let sets = bundles_list().unwrap();
        assert_eq!(sets.len(), 14);
        let arsiv = sets.iter().find(|s| s.id == "arsiv").unwrap();
        assert_eq!(arsiv.entries.len(), 6);
        assert!(arsiv.entries.iter().all(|e| e.kind == "aminet"));
    }

    /// Task 7 (the screen): `tolunnet` and `miamidx` must reach the screen
    /// still carrying `"tcp"`, so the panel can say they are alternatives
    /// rather than silently offering two ticks with no connection between
    /// them.
    #[test]
    fn the_two_tcp_stacks_are_listed_carrying_the_same_exclusive_group() {
        let sets = bundles_list().unwrap();
        let group = |id: &str| {
            sets.iter()
                .flat_map(|s| &s.entries)
                .find(|e| e.id == id)
                .unwrap_or_else(|| panic!("'{id}' is shipped"))
                .exclusive_group
                .clone()
        };
        assert_eq!(group("tolunnet"), Some("tcp".to_string()));
        assert_eq!(group("miamidx"), Some("tcp".to_string()));
        // An ordinary entry outside any group must not gain one by accident.
        assert_eq!(group("lha"), None);
    }

    #[test]
    fn a_permission_entry_is_listed_with_its_condition_so_the_screen_can_say_it_first() {
        let sets = bundles_list().unwrap();
        let picasso = sets
            .iter()
            .flat_map(|s| &s.entries)
            .find(|e| e.id == "picasso96")
            .expect("Picasso96 is shipped");
        assert!(picasso.permission.is_some());
    }

    #[test]
    fn downloading_nothing_is_refused_rather_than_run_as_an_empty_job() {
        // An empty selection is a mistake worth naming, not a job that does
        // nothing and reports success.
        let err = entry_ids_or_refuse(&[]).expect_err("an empty selection is refused");
        assert!(format!("{err}").contains("no packages"), "got: {err}");
    }

    #[test]
    fn an_id_art_does_not_ship_is_refused_by_name() {
        let err = entry_ids_or_refuse(&["not-a-real-package".to_string()])
            .expect_err("an id ART does not ship must be refused");
        assert!(
            format!("{err}").contains("not-a-real-package"),
            "got: {err}"
        );
    }

    #[test]
    fn known_ids_resolve_to_their_entries_in_the_order_the_caller_gave() {
        // Not the catalogue's own order — the caller's, since the screen may
        // let the user tick boxes in any order and the job should not
        // silently reshuffle the selection it was handed.
        let resolved = entry_ids_or_refuse(&["lzx".to_string(), "lha".to_string()])
            .expect("both ids are shipped");
        let ids: Vec<&str> = resolved.iter().map(|e| e.id.as_str()).collect();
        assert_eq!(ids, vec!["lzx", "lha"]);
    }

    #[test]
    fn one_unknown_id_among_known_ones_refuses_the_whole_selection_by_its_name() {
        // A partial download of a selection the user did not make would be
        // its own kind of confident-wrong sentence — refuse before anything
        // starts, and say which id was the problem.
        let err = entry_ids_or_refuse(&["lha".to_string(), "does-not-exist".to_string()])
            .expect_err("one unknown id must refuse the whole call");
        assert!(format!("{err}").contains("does-not-exist"), "got: {err}");
    }

    /// All **six** `EntryOutcome` kinds must be tallied — five would be wrong
    /// after the review that added `NotPlaced` (see the module doc). Reads
    /// the record back through `JsonlOperationLog` rather than the raw file
    /// text, the same way `oplog_recent` itself would.
    #[test]
    fn every_one_of_the_six_outcome_kinds_is_tallied_and_a_placement_counts_as_verified() {
        use crate::core::oplog::{JsonlOperationLog, OperationLog as _};
        use crate::core::sources::bundle::run::{EntryOutcome, EntryReport};
        use crate::core::ScratchDir;

        // `ScratchDir` removes itself on `Drop`, including on the panicking
        // path — a trailing `remove_dir_all` (the shape this replaces) is
        // exactly what a red suite skips, which is the ART-184 pattern
        // CLAUDE.md forbids by name.
        let scratch = ScratchDir::new("art-bundles-oplog", "tally");
        let log_path = scratch.join("operations.jsonl");

        let report = BundleReport {
            entries: vec![
                EntryReport {
                    id: "a".into(),
                    name: "A".into(),
                    outcome: EntryOutcome::Downloaded {
                        bytes: 10,
                        path: "p1".into(),
                    },
                },
                EntryReport {
                    id: "b".into(),
                    name: "B".into(),
                    outcome: EntryOutcome::AlreadyHave { path: "p2".into() },
                },
                EntryReport {
                    id: "c".into(),
                    name: "C".into(),
                    outcome: EntryOutcome::NotPlaced {
                        existing: "p3".into(),
                    },
                },
                EntryReport {
                    id: "d".into(),
                    name: "D".into(),
                    outcome: EntryOutcome::Refused { why: "no".into() },
                },
                EntryReport {
                    id: "e".into(),
                    name: "E".into(),
                    outcome: EntryOutcome::Failed {
                        error: "boom".into(),
                    },
                },
                EntryReport {
                    id: "f".into(),
                    name: "F".into(),
                    outcome: EntryOutcome::Skipped,
                },
            ],
        };

        let entries = entry_ids_or_refuse(&["lha".to_string()]).unwrap();
        record_bundle_download(
            &log_path,
            user_operation("Download package set"),
            &entries,
            &report,
        );

        let log = JsonlOperationLog::new(log_path.clone());
        let recorded = log.recent(1).unwrap();
        let record = recorded.first().expect("one record was written");

        let count = |key: &str| -> &str {
            record
                .details
                .iter()
                .find(|(k, _)| k == key)
                .map(|(_, v)| v.as_str())
                .unwrap_or_else(|| panic!("no '{key}' detail was recorded"))
        };
        assert_eq!(count("Downloaded"), "1");
        assert_eq!(count("Already had"), "1");
        assert_eq!(count("Not placed (slot occupied)"), "1");
        assert_eq!(count("Refused"), "1");
        assert_eq!(count("Failed"), "1");
        assert_eq!(count("Cancelled/skipped"), "1");
        // One real placement (Downloaded) plus one AlreadyHave among six
        // outcomes: the run genuinely put something in the library, so it
        // must read as verified rather than as an undifferentiated failure.
        assert!(record.outcome.is_success());
    }

    /// Finding 4 of the final review: `bundles_download`'s job body used to
    /// return `Ok(())` unconditionally, so a run the user cancelled ended
    /// `JobState::Finished` — the job bar saying "finished" over a report
    /// that says "skipped when you cancelled". `run_and_report_cancellation`
    /// is the factored-out decision, tested here the way
    /// `install_archive_into_volume` is tested directly in
    /// `commands/sources.rs` rather than through a live Tauri job.
    #[test]
    fn a_cancelled_run_reports_cancelled_not_finished_but_still_carries_the_report() {
        use crate::core::jobs::ProgressSink;
        use crate::core::sources::bundle::{BundleEntry, EntrySource};
        use crate::core::sources::mirror::tests::MockMirror;
        use crate::core::sources::mirror::Mirror;
        use crate::core::ScratchDir;
        use std::sync::atomic::{AtomicBool, Ordering};

        const BASE: &str = "https://mirror.invalid/";

        /// Cancels once the second entry's name is reported — deterministic,
        /// the same shape `core::sources::bundle::run`'s own `CancelOn` test
        /// sink uses.
        struct CancelOn {
            name: &'static str,
            hit: AtomicBool,
        }
        impl ProgressSink for CancelOn {
            fn report(&self, _done: u64, _total: Option<u64>, message: &str) {
                if message == self.name {
                    self.hit.store(true, Ordering::SeqCst);
                }
            }
            fn is_cancelled(&self) -> bool {
                self.hit.load(Ordering::SeqCst)
            }
        }

        let scratch = ScratchDir::new("art-bundles-cancel", "job");
        let client = MockMirror::new()
            .with_file(&format!("{BASE}util/arc/lha_68k"), b"lha bytes")
            .with_file(&format!("{BASE}util/arc/lzx121r1"), b"lzx bytes");
        let mirrors = vec![Mirror::new("Test", BASE).unwrap()];
        let cache = crate::core::sources::cache::CacheLayout::new(scratch.join("cache"));
        let library = crate::core::sources::library::Library::new(scratch.join("library"));
        let ctx = DownloadContext {
            aminet: &mirrors,
            configured: &[],
            client: &client,
            cache: &cache,
            library: &library,
            subfolder: "",
        };
        let entries = vec![
            BundleEntry {
                id: "lha".into(),
                name: "lha".into(),
                source: EntrySource::Aminet {
                    path: "util/arc/lha_68k".into(),
                },
                order: 1,
                exclusive_group: None,
                requires: Vec::new(),
                permission: None,
            },
            BundleEntry {
                id: "lzx".into(),
                name: "lzx".into(),
                source: EntrySource::Aminet {
                    path: "util/arc/lzx121r1".into(),
                },
                order: 2,
                exclusive_group: None,
                requires: Vec::new(),
                permission: None,
            },
        ];

        let sink = CancelOn {
            name: "lzx",
            hit: AtomicBool::new(false),
        };
        let (report, outcome) = run_and_report_cancellation(&entries, &ctx, &sink);

        assert!(matches!(
            report.entries[0].outcome,
            EntryOutcome::Downloaded { .. }
        ));
        assert!(matches!(report.entries[1].outcome, EntryOutcome::Skipped));

        let err = outcome.expect_err("a cancelled run must not report success");
        assert_eq!(
            err.code(),
            "ART-CANCELLED",
            "the job must end Cancelled, not Finished: {err}"
        );
    }

    /// The counterpart: nothing cancelled, so the run must still end `Ok`.
    /// Without this, a mutation that always returns `Err(Cancelled)` would
    /// pass the test above and go unnoticed.
    #[test]
    fn an_uncancelled_run_still_reports_ok() {
        use crate::core::sources::bundle::{BundleEntry, EntrySource};
        use crate::core::sources::mirror::tests::MockMirror;
        use crate::core::sources::mirror::Mirror;
        use crate::core::ScratchDir;

        const BASE: &str = "https://mirror.invalid/";

        let scratch = ScratchDir::new("art-bundles-cancel", "no-cancel");
        let client = MockMirror::new().with_file(&format!("{BASE}util/arc/lha_68k"), b"lha bytes");
        let mirrors = vec![Mirror::new("Test", BASE).unwrap()];
        let cache = crate::core::sources::cache::CacheLayout::new(scratch.join("cache"));
        let library = crate::core::sources::library::Library::new(scratch.join("library"));
        let ctx = DownloadContext {
            aminet: &mirrors,
            configured: &[],
            client: &client,
            cache: &cache,
            library: &library,
            subfolder: "",
        };
        let entries = vec![BundleEntry {
            id: "lha".into(),
            name: "lha".into(),
            source: EntrySource::Aminet {
                path: "util/arc/lha_68k".into(),
            },
            order: 1,
            exclusive_group: None,
            requires: Vec::new(),
            permission: None,
        }];

        let (report, outcome) =
            run_and_report_cancellation(&entries, &ctx, &crate::core::jobs::NoProgress);
        assert!(matches!(
            report.entries[0].outcome,
            EntryOutcome::Downloaded { .. }
        ));
        assert!(outcome.is_ok());
    }
}
