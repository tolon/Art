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
pub struct EntrySummary {
    pub id: String,
    pub name: String,
    /// The [`EntrySource`] tag — `"aminet"`, `"aminet-search"`,
    /// `"github-release"`, `"mirror"` or `"user-supplied"` — so the screen
    /// can show where a file comes from without seeing the path itself.
    pub kind: &'static str,
    pub permission: Option<Permission>,
}

impl From<&BundleEntry> for EntrySummary {
    fn from(entry: &BundleEntry) -> Self {
        Self {
            id: entry.id.clone(),
            name: entry.name.clone(),
            kind: entry_kind(&entry.source),
            permission: entry.permission.clone(),
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

        let report = download_entries(&entries, &ctx, progress);

        let record = user_operation("Download package set");
        record_bundle_download(&log_path, record, &entries, &report);

        let _ = emit_app.emit(
            BUNDLE_DOWNLOAD_EVENT,
            BundleDownloadResult { job_id, report },
        );
        Ok(())
    });

    Ok(id)
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

        let dir = std::env::temp_dir().join(format!(
            "art-bundles-oplog-{}",
            crate::core::test_scratch_id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let log_path = dir.join("operations.jsonl");

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

        let _ = std::fs::remove_dir_all(&dir);
    }
}
