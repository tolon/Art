//! Append-only JSON Lines operation log.
//!
//! One JSON object per line. A crash mid-write costs at most the last line, and
//! reading tolerates a truncated or malformed one rather than refusing to show
//! any history at all — a log that hides everything because of one bad byte is
//! worse than no log.

use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use super::{OperationLog, OperationRecord};
use crate::core::error::{CoreError, CoreResult};

/// Stop the log growing without bound. Older entries are dropped when the file
/// passes this size — history is useful, but not at the cost of the user's disk.
const MAX_LOG_BYTES: u64 = 8 * 1024 * 1024;

/// How many recent entries survive a rotation.
const KEEP_ON_ROTATE: usize = 2000;

/// A JSON Lines operation log on disk.
pub struct JsonlOperationLog {
    path: PathBuf,
    /// Serialises concurrent appends so two commands cannot interleave a line.
    write_lock: Mutex<()>,
}

impl JsonlOperationLog {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            write_lock: Mutex::new(()),
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Read every well-formed record, oldest first. Malformed lines are skipped.
    fn read_all(&self) -> CoreResult<Vec<OperationRecord>> {
        if !self.path.is_file() {
            return Ok(Vec::new());
        }
        let file = fs::File::open(&self.path)?;
        let reader = BufReader::new(file);

        let mut records = Vec::new();
        for line in reader.lines() {
            let Ok(line) = line else { continue };
            if line.trim().is_empty() {
                continue;
            }
            if let Ok(record) = serde_json::from_str::<OperationRecord>(&line) {
                records.push(record);
            }
        }
        Ok(records)
    }

    /// Rewrite the file with only the newest `KEEP_ON_ROTATE` entries.
    fn rotate(&self) -> CoreResult<()> {
        let records = self.read_all()?;
        let start = records.len().saturating_sub(KEEP_ON_ROTATE);

        let mut buffer = String::new();
        for record in &records[start..] {
            match serde_json::to_string(record) {
                Ok(line) => {
                    buffer.push_str(&line);
                    buffer.push('\n');
                }
                Err(_) => continue,
            }
        }

        // Atomically, so a crash during rotation cannot lose the whole history.
        crate::core::safety::atomic_write(&self.path, buffer.as_bytes())
    }
}

impl OperationLog for JsonlOperationLog {
    fn record(&self, record: &OperationRecord) -> CoreResult<()> {
        let line = serde_json::to_string(record).map_err(|e| {
            CoreError::InvalidInput(format!("operation record could not be serialised: {e}"))
        })?;

        let _guard = self
            .write_lock
            .lock()
            .map_err(|_| CoreError::InvalidInput("operation log lock was poisoned".into()))?;

        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }

        let mut file = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)?;
        writeln!(file, "{line}")?;
        file.flush()?;

        // Rotate after the write so the new entry is never the one dropped.
        let too_big = fs::metadata(&self.path).map(|m| m.len() > MAX_LOG_BYTES);
        if matches!(too_big, Ok(true)) {
            drop(file);
            self.rotate()?;
        }

        Ok(())
    }

    fn recent(&self, limit: usize) -> CoreResult<Vec<OperationRecord>> {
        let mut records = self.read_all()?;
        records.reverse();
        records.truncate(limit);
        Ok(records)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::error::CoreError;
    use crate::core::oplog::{OperationOrigin, OperationOutcome};

    fn scratch(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "art-oplog-{tag}-{}",
            crate::core::test_scratch_id()
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn entry(name: &str) -> OperationRecord {
        OperationRecord::new(name, OperationOrigin::UserInterface)
    }

    #[test]
    fn records_survive_a_round_trip() {
        let dir = scratch("roundtrip");
        let log = JsonlOperationLog::new(dir.join("operations.jsonl"));

        log.record(
            &entry("Add file to disk")
                .source("a.txt")
                .destination("d.adf"),
        )
        .unwrap();
        log.record(&entry("Create hard disk").destination("Games.hdf"))
            .unwrap();

        let recent = log.recent(10).unwrap();
        assert_eq!(recent.len(), 2);
        // Newest first.
        assert_eq!(recent[0].operation, "Create hard disk");
        assert_eq!(recent[1].source.as_deref(), Some("a.txt"));

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn creates_the_log_directory_on_first_write() {
        let dir = scratch("mkdir");
        let log = JsonlOperationLog::new(dir.join("nested").join("deeper").join("ops.jsonl"));

        log.record(&entry("First")).unwrap();

        assert_eq!(log.recent(5).unwrap().len(), 1);
        fs::remove_dir_all(&dir).ok();
    }

    /// A half-written line from a crash must not hide the rest of the history.
    #[test]
    fn a_corrupt_line_does_not_hide_the_others() {
        let dir = scratch("corrupt");
        let path = dir.join("operations.jsonl");
        let log = JsonlOperationLog::new(&path);

        log.record(&entry("Good one")).unwrap();
        // Simulate a torn write.
        let mut f = fs::OpenOptions::new().append(true).open(&path).unwrap();
        writeln!(f, "{{\"operation\": \"tru").unwrap();
        drop(f);
        log.record(&entry("Good two")).unwrap();

        let recent = log.recent(10).unwrap();
        assert_eq!(recent.len(), 2, "both intact records should be readable");

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn failures_are_recorded_with_their_error_id() {
        let dir = scratch("failure");
        let log = JsonlOperationLog::new(dir.join("operations.jsonl"));

        let err = CoreError::Malformed {
            format: "adf".into(),
            detail: "bad checksum".into(),
        };
        log.record(&entry("Add file to disk").failed(&err)).unwrap();

        let recent = log.recent(1).unwrap();
        match &recent[0].outcome {
            OperationOutcome::Failure { error_code, .. } => {
                assert_eq!(error_code, "ART-FORMAT-MALFORMED");
            }
            other => panic!("expected a failure, got {other:?}"),
        }

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn reading_an_absent_log_is_not_an_error() {
        let dir = scratch("absent");
        let log = JsonlOperationLog::new(dir.join("never-written.jsonl"));

        assert!(log.recent(10).unwrap().is_empty());
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn rotation_keeps_the_newest_entries() {
        let dir = scratch("rotate");
        let path = dir.join("operations.jsonl");
        let log = JsonlOperationLog::new(&path);

        // Write a couple of entries, then force a rotation directly: driving the
        // 8 MB threshold through the public path would take far too long.
        log.record(&entry("oldest")).unwrap();
        log.record(&entry("newest")).unwrap();
        log.rotate().unwrap();

        let recent = log.recent(10).unwrap();
        assert_eq!(recent.len(), 2);
        assert_eq!(recent[0].operation, "newest");

        fs::remove_dir_all(&dir).ok();
    }
}
