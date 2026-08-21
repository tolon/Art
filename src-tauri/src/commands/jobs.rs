//! Job runner — the shell half of background work (spec §54, §55).
//!
//! A job is a long operation running off the UI thread. The registry tracks
//! what is in flight, forwards progress to the frontend as `job-progress`
//! events, and holds the cancel token the user's Stop button flips.
//!
//! Progress is **coalesced**: a scan of 50,000 files would otherwise emit
//! 50,000 events and make the UI slower than the work. Events go out at most
//! every 100 ms, plus one final event whenever a job reaches a terminal state so
//! the UI never sits on a stale bar.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use tauri::{AppHandle, Emitter, State};

use crate::core::error::CoreError;
use crate::core::jobs::{CancelToken, JobId, JobProgress, JobState, ProgressSink};
use crate::error::AppResult;

/// The event name the frontend listens on.
pub const JOB_EVENT: &str = "job-progress";

/// Minimum gap between progress events for one job.
const EVENT_INTERVAL: Duration = Duration::from_millis(100);

/// Everything in flight, plus finished jobs the UI has not collected yet.
#[derive(Default)]
pub struct JobRegistry {
    next_id: AtomicU64,
    jobs: Mutex<HashMap<JobId, JobEntry>>,
}

struct JobEntry {
    progress: JobProgress,
    cancel: CancelToken,
    /// Which lane this job is in, if any — see [`spawn_job_in_lane`].
    lane: Option<&'static str>,
}

impl JobRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a new job and return its id and cancel token.
    fn open(&self, title: &str) -> (JobId, CancelToken) {
        self.open_in_lane(title, None)
    }

    fn open_in_lane(&self, title: &str, lane: Option<&'static str>) -> (JobId, CancelToken) {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed) + 1;
        let cancel = CancelToken::new();
        let entry = JobEntry {
            progress: JobProgress {
                id,
                title: title.to_string(),
                done: 0,
                total: None,
                message: String::new(),
                state: JobState::Running,
            },
            cancel: cancel.clone(),
            lane,
        };
        if let Ok(mut jobs) = self.jobs.lock() {
            jobs.insert(id, entry);
        }
        (id, cancel)
    }

    /// Stop and forget every unfinished job in `lane`, returning what to tell
    /// the UI about each one.
    ///
    /// **ART-195.** A lane holds jobs that answer the same question about the
    /// same thing, so a newer one makes an older one pointless — the OS
    /// Builder's component preview is the case that forced this: every toggle
    /// started another walk of a 468 MB ISO and cancelled nothing, and the
    /// jobs then competed for the same drive.
    ///
    /// Two things happen, and both are needed:
    ///
    /// - the cancel token is flipped, which is what actually stops the disk
    ///   work — the same token, the same `is_cancelled()` check between whole
    ///   units, no second mechanism; and
    /// - the entry is **removed from the registry**, so the job bar shows one
    ///   preview rather than a growing stack. The returned `JobProgress`
    ///   carries [`JobState::Superseded`] for the caller to emit, which is how
    ///   a row already on screen is taken off it. The worker thread's own
    ///   terminal update then finds no entry and emits nothing, so the removed
    ///   job cannot reappear as `Cancelled` a moment later.
    fn supersede(&self, lane: &str) -> Vec<JobProgress> {
        let Ok(mut jobs) = self.jobs.lock() else {
            return Vec::new();
        };
        let doomed: Vec<JobId> = jobs
            .values()
            .filter(|e| e.lane == Some(lane) && !e.progress.state.is_terminal())
            .map(|e| e.progress.id)
            .collect();
        let mut gone = Vec::with_capacity(doomed.len());
        for id in doomed {
            if let Some(entry) = jobs.remove(&id) {
                entry.cancel.cancel();
                let mut progress = entry.progress;
                progress.state = JobState::Superseded;
                gone.push(progress);
            }
        }
        gone
    }

    fn update(
        &self,
        id: JobId,
        done: u64,
        total: Option<u64>,
        message: &str,
    ) -> Option<JobProgress> {
        let mut jobs = self.jobs.lock().ok()?;
        let entry = jobs.get_mut(&id)?;
        entry.progress.done = done;
        entry.progress.total = total;
        entry.progress.message = message.to_string();
        Some(entry.progress.clone())
    }

    fn finish(&self, id: JobId, state: JobState) -> Option<JobProgress> {
        let mut jobs = self.jobs.lock().ok()?;
        let entry = jobs.get_mut(&id)?;
        entry.progress.state = state;
        Some(entry.progress.clone())
    }

    /// Everything the registry knows about, running jobs first.
    pub fn snapshot(&self) -> Vec<JobProgress> {
        let Ok(jobs) = self.jobs.lock() else {
            return Vec::new();
        };
        let mut all: Vec<JobProgress> = jobs.values().map(|e| e.progress.clone()).collect();
        all.sort_by_key(|p| (p.state.is_terminal(), p.id));
        all
    }

    /// Ask a job to stop. Returns false when there is no such running job.
    pub fn cancel(&self, id: JobId) -> bool {
        let Ok(jobs) = self.jobs.lock() else {
            return false;
        };
        match jobs.get(&id) {
            Some(entry) if !entry.progress.state.is_terminal() => {
                entry.cancel.cancel();
                true
            }
            _ => false,
        }
    }

    /// Drop finished jobs so the list does not grow for the life of the app.
    pub fn clear_finished(&self) {
        if let Ok(mut jobs) = self.jobs.lock() {
            jobs.retain(|_, e| !e.progress.state.is_terminal());
        }
    }
}

/// The [`ProgressSink`] handed to the running operation.
///
/// Holds an `AppHandle` so it can emit, and throttles how often it does.
struct JobSink {
    id: JobId,
    app: AppHandle,
    registry: Arc<JobRegistry>,
    cancel: CancelToken,
    last_emit: Mutex<Instant>,
}

impl ProgressSink for JobSink {
    fn report(&self, done: u64, total: Option<u64>, message: &str) {
        let Some(progress) = self.registry.update(self.id, done, total, message) else {
            return;
        };

        // Only emit if enough time has passed, so a fast loop cannot flood the
        // webview. The registry always holds the current value, so a throttled
        // update is not lost — just not broadcast yet.
        let Ok(mut last) = self.last_emit.lock() else {
            return;
        };
        if last.elapsed() < EVENT_INTERVAL {
            return;
        }
        *last = Instant::now();
        let _ = self.app.emit(JOB_EVENT, &progress);
    }

    fn is_cancelled(&self) -> bool {
        self.cancel.is_cancelled()
    }
}

/// Run `work` on a background thread as a named job.
///
/// Returns the job id immediately; the UI follows the rest through
/// `job-progress` events. The closure receives its own job id — so a result it
/// emits can be tied back to the job — and a sink to report through, and must
/// check `is_cancelled` between units of work.
pub fn spawn_job<F>(app: &AppHandle, registry: Arc<JobRegistry>, title: &str, work: F) -> JobId
where
    F: FnOnce(JobId, &dyn ProgressSink) -> Result<(), CoreError> + Send + 'static,
{
    spawn_in_lane(app, registry, title, None, work)
}

/// [`spawn_job`], but the new job **supersedes** every unfinished job already
/// in `lane` — cancelled, and taken off the job bar (ART-195).
///
/// For work whose answer is only ever wanted once: a live preview that the
/// screen re-asks whenever the selection changes. Without this the OS
/// Builder's component preview reached **2,149 jobs in one session**, each
/// walking the same 468 MB ISO, and the only way the owner could stop the
/// disk work was to close the application.
///
/// A lane is a `&'static str` rather than a caller-built string on purpose:
/// lanes are a fixed, readable set declared beside the commands that use them,
/// and a lane name assembled at run time is a lane two call sites can disagree
/// about by a space.
pub fn spawn_job_in_lane<F>(
    app: &AppHandle,
    registry: Arc<JobRegistry>,
    title: &str,
    lane: &'static str,
    work: F,
) -> JobId
where
    F: FnOnce(JobId, &dyn ProgressSink) -> Result<(), CoreError> + Send + 'static,
{
    spawn_in_lane(app, registry, title, Some(lane), work)
}

fn spawn_in_lane<F>(
    app: &AppHandle,
    registry: Arc<JobRegistry>,
    title: &str,
    lane: Option<&'static str>,
    work: F,
) -> JobId
where
    F: FnOnce(JobId, &dyn ProgressSink) -> Result<(), CoreError> + Send + 'static,
{
    if let Some(lane) = lane {
        // Before the new job exists, so it can never supersede itself.
        for gone in registry.supersede(lane) {
            let _ = app.emit(JOB_EVENT, &gone);
        }
    }
    let (id, cancel) = registry.open_in_lane(title, lane);

    let sink = JobSink {
        id,
        app: app.clone(),
        registry: Arc::clone(&registry),
        cancel,
        // Start in the past so the first report is emitted immediately.
        last_emit: Mutex::new(Instant::now() - EVENT_INTERVAL),
    };

    let app = app.clone();
    std::thread::spawn(move || {
        let result = work(id, &sink);

        let state = match result {
            Ok(()) => JobState::Finished,
            Err(CoreError::Cancelled) => JobState::Cancelled { files_landed: None },
            // Cancelled, with work already durable on disk (ART-058). Still a
            // cancellation and not a failure — the job bar must not go red for
            // something the user asked for — but the count travels with it so
            // the UI can say what is on the volume.
            Err(CoreError::CancelledPartway { files }) => JobState::Cancelled {
                files_landed: Some(files),
            },
            Err(e) => JobState::Failed {
                error_code: e.code().to_string(),
                message: e.to_string(),
            },
        };

        // A terminal update always goes out, ignoring the throttle: the UI must
        // never be left showing a job as running after it has stopped.
        if let Some(progress) = registry.finish(id, state) {
            let _ = app.emit(JOB_EVENT, &progress);
        }
    });

    id
}

// ---------------------------------------------------------------------------
// Commands
// ---------------------------------------------------------------------------

/// Every job the registry knows about, running ones first.
#[tauri::command]
pub fn job_list(registry: State<'_, Arc<JobRegistry>>) -> Vec<JobProgress> {
    registry.snapshot()
}

/// Ask a running job to stop at its next safe point.
#[tauri::command]
pub fn job_cancel(id: JobId, registry: State<'_, Arc<JobRegistry>>) -> AppResult<bool> {
    Ok(registry.cancel(id))
}

/// Forget jobs that have finished.
#[tauri::command]
pub fn job_clear_finished(registry: State<'_, Arc<JobRegistry>>) {
    registry.clear_finished();
}

#[cfg(test)]
mod tests {
    use super::*;

    /// **ART-195.** The whole point is that the *previous* preview stops, so
    /// this asserts on a job that was demonstrably running first — the
    /// vacuous version of this test is one that supersedes an empty lane and
    /// passes because there was nothing there.
    #[test]
    fn a_new_job_in_a_lane_cancels_the_one_before_it() {
        let registry = JobRegistry::new();
        let (first, first_token) = registry.open_in_lane("Preview 1", Some("preview"));

        // Proof the lane was populated and live before anything superseded
        // it. Without these two the test would pass against a `supersede`
        // that did nothing at all.
        assert!(!first_token.is_cancelled(), "it starts running");
        assert!(
            registry.snapshot().iter().any(|p| p.id == first),
            "and it is in the list"
        );

        let gone = registry.supersede("preview");

        assert_eq!(gone.len(), 1, "exactly the one that was running");
        assert_eq!(gone[0].id, first);
        assert_eq!(gone[0].state, JobState::Superseded);
        assert!(
            first_token.is_cancelled(),
            "the token the worker holds is flipped — this is what actually stops the disk work"
        );
    }

    /// One preview in the list, not a growing stack — the thing the owner
    /// photographed. Four jobs are started in the lane one after another and
    /// the list is measured after each, so a `supersede` that only worked
    /// from empty could not pass.
    #[test]
    fn a_lane_never_holds_more_than_one_unfinished_job() {
        let registry = JobRegistry::new();
        let mut tokens = Vec::new();
        for round in 0..4 {
            for gone in registry.supersede("preview") {
                assert_eq!(gone.state, JobState::Superseded);
            }
            let (_, token) = registry.open_in_lane(&format!("Preview {round}"), Some("preview"));
            tokens.push(token);

            let live = registry
                .snapshot()
                .into_iter()
                .filter(|p| !p.state.is_terminal())
                .count();
            assert_eq!(
                live, 1,
                "after round {round} the lane holds one job, not {live}"
            );
        }
        // Every earlier one was really told to stop; only the last is alive.
        for (at, token) in tokens.iter().enumerate() {
            assert_eq!(
                token.is_cancelled(),
                at < tokens.len() - 1,
                "job {at} cancelled-ness"
            );
        }
    }

    /// A superseded job is *removed*, so the worker thread's own terminal
    /// update finds nothing and cannot put a `Cancelled` row back on the bar
    /// a moment after the row was taken off it.
    #[test]
    fn a_superseded_job_cannot_come_back_as_cancelled() {
        let registry = JobRegistry::new();
        let (id, _) = registry.open_in_lane("Preview", Some("preview"));
        assert_eq!(registry.supersede("preview").len(), 1);

        assert!(
            registry
                .finish(id, JobState::Cancelled { files_landed: None })
                .is_none(),
            "the worker's terminal update has nothing to update"
        );
        assert!(
            registry.update(id, 5, None, "still going").is_none(),
            "and neither has its progress sink"
        );
        assert!(registry.snapshot().iter().all(|p| p.id != id));
    }

    /// Lanes are separate: a component preview must not cancel a package
    /// preview, and neither may touch a job that is in no lane at all — an
    /// install writing to a card is not a preview.
    #[test]
    fn superseding_one_lane_leaves_every_other_job_alone() {
        let registry = JobRegistry::new();
        let (_, other_lane) = registry.open_in_lane("Packages", Some("packages"));
        let (_, laneless) = registry.open("Installing AmigaOS");
        let (_, same_lane) = registry.open_in_lane("Components", Some("components"));

        let gone = registry.supersede("components");

        assert_eq!(gone.len(), 1);
        assert!(same_lane.is_cancelled());
        assert!(
            !other_lane.is_cancelled(),
            "a different lane is a different question"
        );
        assert!(!laneless.is_cancelled(), "and an install is not a preview");
    }

    /// A job that already finished is not superseded a second time — there is
    /// nothing to stop, and re-reporting it would take a legitimate
    /// `Finished`/`Failed` row off the bar.
    #[test]
    fn a_finished_job_in_the_lane_is_left_where_it_is() {
        let registry = JobRegistry::new();
        let (done, _) = registry.open_in_lane("Preview", Some("preview"));
        registry.finish(done, JobState::Finished);

        assert!(registry.supersede("preview").is_empty());
        assert!(
            registry.snapshot().iter().any(|p| p.id == done),
            "it stays in the list, as itself"
        );
    }

    #[test]
    fn ids_are_unique_and_start_at_one() {
        let registry = JobRegistry::new();
        let (first, _) = registry.open("A");
        let (second, _) = registry.open("B");

        assert_eq!(first, 1);
        assert_eq!(second, 2);
    }

    #[test]
    fn snapshot_puts_running_jobs_first() {
        let registry = JobRegistry::new();
        let (done_id, _) = registry.open("Finished one");
        let (_running_id, _) = registry.open("Still going");
        registry.finish(done_id, JobState::Finished);

        let snapshot = registry.snapshot();
        assert_eq!(snapshot.len(), 2);
        assert_eq!(snapshot[0].title, "Still going");
    }

    #[test]
    fn cancelling_flips_the_token_the_worker_holds() {
        let registry = JobRegistry::new();
        let (id, token) = registry.open("Scanning");

        assert!(!token.is_cancelled());
        assert!(registry.cancel(id));
        assert!(token.is_cancelled());
    }

    #[test]
    fn a_finished_job_cannot_be_cancelled() {
        let registry = JobRegistry::new();
        let (id, _) = registry.open("Scanning");
        registry.finish(id, JobState::Finished);

        assert!(!registry.cancel(id), "already terminal");
        assert!(!registry.cancel(9999), "unknown job");
    }

    #[test]
    fn clearing_keeps_running_jobs() {
        let registry = JobRegistry::new();
        let (done, _) = registry.open("Done");
        let (_running, _) = registry.open("Running");
        registry.finish(done, JobState::Cancelled { files_landed: None });

        registry.clear_finished();

        let snapshot = registry.snapshot();
        assert_eq!(snapshot.len(), 1);
        assert_eq!(snapshot[0].title, "Running");
    }

    #[test]
    fn updates_are_visible_in_the_snapshot() {
        let registry = JobRegistry::new();
        let (id, _) = registry.open("Hashing");

        registry.update(id, 5, Some(10), "disk.adf");

        let snapshot = registry.snapshot();
        assert_eq!(snapshot[0].done, 5);
        assert_eq!(snapshot[0].total, Some(10));
        assert_eq!(snapshot[0].message, "disk.adf");
        assert_eq!(snapshot[0].fraction(), Some(0.5));
    }
}
