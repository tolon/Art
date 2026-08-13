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
}

impl JobRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a new job and return its id and cancel token.
    fn open(&self, title: &str) -> (JobId, CancelToken) {
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
        };
        if let Ok(mut jobs) = self.jobs.lock() {
            jobs.insert(id, entry);
        }
        (id, cancel)
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
    let (id, cancel) = registry.open(title);

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
