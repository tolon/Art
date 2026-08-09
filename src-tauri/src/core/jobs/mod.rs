//! Background work: progress reporting and cancellation.
//!
//! Spec §54 and §55: long operations run as background jobs and the UI must
//! never freeze. This module is the *core* half of that — the part a
//! platform-independent engine function can use without knowing anything about
//! Tauri, threads or events.
//!
//! A core operation takes a [`ProgressSink`]. It reports what it has done and
//! checks [`ProgressSink::is_cancelled`] at points where stopping is safe. It
//! does not decide where progress goes or how cancellation is triggered; the
//! shell wires that up (`commands/jobs.rs`).
//!
//! Cancellation is **cooperative and safe by construction**: an operation only
//! observes the flag between whole units of work, never mid-write. Combined with
//! `core/safety`, cancelling can leave work unfinished but never a half-written
//! file.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use serde::{Deserialize, Serialize};

/// A running job's identity, unique for the lifetime of the application.
pub type JobId = u64;

/// Where a job has got to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum JobState {
    Running,
    /// Finished normally.
    Finished,
    /// Stopped because the user asked.
    Cancelled,
    /// Stopped because it failed. `error_code` is a `ART-*` identifier (§68).
    Failed {
        error_code: String,
        message: String,
    },
}

impl JobState {
    pub fn is_terminal(&self) -> bool {
        !matches!(self, Self::Running)
    }
}

/// A snapshot of a job, safe to send to the UI.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JobProgress {
    pub id: JobId,
    /// What this job is, in the user's language: "Scanning collection".
    pub title: String,
    /// Units completed so far — files, bytes, blocks; whatever the job counts.
    pub done: u64,
    /// Total units, when the job can know it up front. `None` means indefinite,
    /// and the UI should show an unbounded indicator rather than a fake bar.
    pub total: Option<u64>,
    /// What is happening right now: the current file name, for example.
    pub message: String,
    pub state: JobState,
}

impl JobProgress {
    /// Completion as a fraction, when the total is known.
    pub fn fraction(&self) -> Option<f32> {
        match self.total {
            Some(total) if total > 0 => Some((self.done as f32 / total as f32).clamp(0.0, 1.0)),
            _ => None,
        }
    }
}

/// A flag a long operation checks to see whether it should stop.
///
/// Cloning shares the same flag, so the shell can hold one handle while the
/// worker holds another.
#[derive(Debug, Clone, Default)]
pub struct CancelToken {
    cancelled: Arc<AtomicBool>,
}

impl CancelToken {
    pub fn new() -> Self {
        Self::default()
    }

    /// Ask the operation to stop at its next safe point.
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Relaxed);
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Relaxed)
    }
}

/// Somewhere a long operation reports progress, and asks whether to stop.
///
/// Implementations must be cheap to call: an operation may report once per file
/// in a folder of tens of thousands.
pub trait ProgressSink: Send + Sync {
    /// Report position. `total` may be `None` while the size is still unknown.
    fn report(&self, done: u64, total: Option<u64>, message: &str);

    /// True when the user has asked to stop. Check between units of work.
    fn is_cancelled(&self) -> bool;
}

/// A sink that discards everything and is never cancelled.
///
/// Lets an operation be called directly — from a test, or from a command that
/// is quick enough not to need a job — without threading an `Option` through.
#[derive(Debug, Clone, Copy, Default)]
pub struct NoProgress;

impl ProgressSink for NoProgress {
    fn report(&self, _done: u64, _total: Option<u64>, _message: &str) {}
    fn is_cancelled(&self) -> bool {
        false
    }
}

/// The error a core operation returns when it stopped because it was asked to.
///
/// Distinct from a failure: nothing went wrong, and the UI should say
/// "cancelled" rather than showing an error.
pub fn cancelled_error() -> crate::core::error::CoreError {
    crate::core::error::CoreError::Cancelled
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    /// A sink that remembers what it was told, for asserting in tests.
    #[derive(Default)]
    struct RecordingSink {
        reports: Mutex<Vec<(u64, Option<u64>, String)>>,
        cancel: CancelToken,
    }

    impl ProgressSink for RecordingSink {
        fn report(&self, done: u64, total: Option<u64>, message: &str) {
            self.reports
                .lock()
                .unwrap()
                .push((done, total, message.to_string()));
        }
        fn is_cancelled(&self) -> bool {
            self.cancel.is_cancelled()
        }
    }

    #[test]
    fn cancel_token_is_shared_between_clones() {
        let a = CancelToken::new();
        let b = a.clone();

        assert!(!a.is_cancelled());
        b.cancel();
        assert!(a.is_cancelled(), "cancelling one handle cancels the other");
    }

    #[test]
    fn no_progress_never_cancels() {
        let sink = NoProgress;
        sink.report(1, Some(10), "ignored");
        assert!(!sink.is_cancelled());
    }

    #[test]
    fn fraction_is_none_without_a_total() {
        let mut p = JobProgress {
            id: 1,
            title: "Scanning".into(),
            done: 5,
            total: None,
            message: String::new(),
            state: JobState::Running,
        };
        assert_eq!(p.fraction(), None);

        p.total = Some(10);
        assert_eq!(p.fraction(), Some(0.5));

        // A job that overshoots its estimate must not report over 100%.
        p.done = 20;
        assert_eq!(p.fraction(), Some(1.0));

        // A zero total is indefinite, not a division by zero.
        p.total = Some(0);
        assert_eq!(p.fraction(), None);
    }

    #[test]
    fn a_sink_records_what_it_is_told() {
        let sink = RecordingSink::default();
        sink.report(1, Some(3), "one.adf");
        sink.report(2, Some(3), "two.adf");

        let reports = sink.reports.lock().unwrap();
        assert_eq!(reports.len(), 2);
        assert_eq!(reports[1], (2, Some(3), "two.adf".to_string()));
    }

    #[test]
    fn only_running_is_non_terminal() {
        assert!(!JobState::Running.is_terminal());
        assert!(JobState::Finished.is_terminal());
        assert!(JobState::Cancelled.is_terminal());
        assert!(JobState::Failed {
            error_code: "ART-IO".into(),
            message: "x".into()
        }
        .is_terminal());
    }
}
