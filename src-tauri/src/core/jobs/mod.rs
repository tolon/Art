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

use std::borrow::Cow;
use std::collections::BTreeMap;
use std::fmt::Display;
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
    ///
    /// `files_landed` is how many files were already written and left in place
    /// when it stopped, or `None` when nothing was — which is the usual case
    /// and the only one the whole-file write strategy can produce. It is a
    /// number rather than a sentence because the sentence belongs to the UI's
    /// catalogue, in the user's language (ART-058, §68).
    Cancelled {
        files_landed: Option<u64>,
    },
    /// Stopped because it failed. `error_code` is a `ART-*` identifier (§68).
    Failed {
        error_code: String,
        message: String,
    },
    /// Cancelled by ART itself, because a newer job in the same lane made
    /// this one's answer worthless before it could give it (ART-195).
    ///
    /// **Not a second cancellation mechanism** — a superseded job is stopped
    /// through exactly the same [`CancelToken`] the user's Stop button flips,
    /// and stops at exactly the same `is_cancelled()` check. What is
    /// different is only who asked and therefore who needs telling: the user
    /// asked for the *newer* preview, so the older one going away is not news
    /// and the job bar drops it instead of stacking a row nobody wants. A
    /// `Cancelled` row here would have replaced four stacked running previews
    /// with four stacked cancelled ones.
    Superseded,
}

impl JobState {
    pub fn is_terminal(&self) -> bool {
        !matches!(self, Self::Running)
    }
}

/// What a job is, as a catalogue key and the values its sentence needs —
/// never a sentence (ART-301).
///
/// The job bar used to show an English sentence the command layer composed
/// with `format!`, so a Turkish screen said *"Adding 1 package(s) to …"*. The
/// sentence belongs to the UI's catalogue, in the user's language (§68) — the
/// reasoning [`JobState::Cancelled`] already follows for its count.
///
/// Serialises to exactly the TypeScript `Phrase` shape, `{ "key": …,
/// "params": { … } }` with `params` left out when empty, so the job bar
/// renders it with `t(title.key, title.params)`.
///
/// **A key is a literal, by construction.** [`JobTitle::new`] takes a
/// `&'static str` and the field is private. `src/i18n/job-title-keys.test.ts`
/// reads every `JobTitle::new("…")` in the Rust tree and fails on a key the
/// catalogue list does not hold, a listed key no Rust site names, or a value
/// the sentence does not use.
///
/// Values are what the user gave or what ART found — a path, a package name,
/// a release — and are never translated. A count goes in through
/// [`JobTitle::count`], because i18next chooses `_one` / `_other` only from a
/// value named `count`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JobTitle {
    /// A `Cow` only so the type can derive `Deserialize`; every key ART
    /// builds is borrowed from a literal.
    key: Cow<'static, str>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    params: BTreeMap<String, JobParam>,
}

/// One value a job title interpolates.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum JobParam {
    /// A JSON number. Listed first so a number reads back as a count.
    Count(u64),
    Text(String),
}

impl JobTitle {
    pub fn new(key: &'static str) -> Self {
        Self {
            key: Cow::Borrowed(key),
            params: BTreeMap::new(),
        }
    }

    /// A value the sentence names as `{{name}}`, rendered through `Display` —
    /// pass a path as `&path.display()`.
    pub fn text(mut self, name: &'static str, value: &dyn Display) -> Self {
        self.params
            .insert(name.to_string(), JobParam::Text(value.to_string()));
        self
    }

    /// The `{{count}}` that chooses the sentence's plural form.
    pub fn count(mut self, n: usize) -> Self {
        self.params
            .insert("count".to_string(), JobParam::Count(n as u64));
        self
    }

    pub fn key(&self) -> &str {
        &self.key
    }

    pub fn params(&self) -> &BTreeMap<String, JobParam> {
        &self.params
    }
}

/// A snapshot of a job, safe to send to the UI.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JobProgress {
    pub id: JobId,
    /// What this job is: a catalogue key and its values, rendered in the
    /// user's language by the job bar (ART-301).
    pub title: JobTitle,
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
            title: JobTitle::new("components.jobBar.title.indexTitles"),
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
        assert!(JobState::Cancelled { files_landed: None }.is_terminal());
        assert!(JobState::Failed {
            error_code: "ART-IO".into(),
            message: "x".into()
        }
        .is_terminal());
    }

    /// ART-301. The job bar renders a title with `t(title.key, title.params)`,
    /// so the wire shape is the TypeScript `Phrase` shape, exactly.
    #[test]
    fn a_job_title_serializes_to_the_phrase_shape() {
        let title = JobTitle::new("components.jobBar.title.copyOutOf").text("source", &"DH0.hdf");
        assert_eq!(
            serde_json::to_value(&title).unwrap(),
            serde_json::json!({
                "key": "components.jobBar.title.copyOutOf",
                "params": { "source": "DH0.hdf" }
            })
        );
    }

    /// A count travels as a JSON number named `count` — the shape
    /// `JobTitle.params` declares on the TypeScript side, and the name
    /// i18next reads to choose `_one` or `_other`.
    #[test]
    fn a_count_is_sent_as_a_number_named_count() {
        let title = JobTitle::new("components.jobBar.title.downloadPackages").count(1);
        assert_eq!(
            serde_json::to_value(&title).unwrap(),
            serde_json::json!({
                "key": "components.jobBar.title.downloadPackages",
                "params": { "count": 1 }
            })
        );
    }

    /// Nothing to interpolate sends no `params` at all — `Phrase.params?`.
    #[test]
    fn a_title_without_values_sends_no_params_field() {
        let title = JobTitle::new("components.jobBar.title.syncAminet");
        assert_eq!(
            serde_json::to_value(&title).unwrap(),
            serde_json::json!({ "key": "components.jobBar.title.syncAminet" })
        );
    }

    /// The event the job bar receives carries the phrase, not a sentence.
    #[test]
    fn a_job_progress_sends_its_title_as_a_phrase_not_a_sentence() {
        let progress = JobProgress {
            id: 7,
            title: JobTitle::new("components.jobBar.title.addPackages")
                .count(2)
                .text("target", &"E:/tree"),
            done: 0,
            total: None,
            message: String::new(),
            state: JobState::Running,
        };
        let sent = serde_json::to_value(&progress).unwrap();
        assert_eq!(
            sent["title"],
            serde_json::json!({
                "key": "components.jobBar.title.addPackages",
                "params": { "count": 2, "target": "E:/tree" }
            })
        );
    }

    /// `JobProgress` derives `Deserialize`, so the title must read back as
    /// itself — a count as a count, a text as a text.
    #[test]
    fn a_title_reads_back_as_itself() {
        let title = JobTitle::new("components.jobBar.title.previewPackages")
            .count(3)
            .text("target", &"Work.hdf");
        let back: JobTitle = serde_json::from_value(serde_json::to_value(&title).unwrap()).unwrap();
        assert_eq!(back, title);
    }
}
