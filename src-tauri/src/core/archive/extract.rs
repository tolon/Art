//! The one gate every archive's bytes pass through.
//!
//! Enforces four guarantees (spec §56, §57, §89), for every format, in one
//! place:
//!
//! 1. **Path traversal.** Every entry name goes through `safe_join`. One that
//!    escapes `dest` — `../../Windows/System32/…`, `C:\Users\…`, a name that
//!    is only separators — is reported and **never written**.
//! 2. **Decompression bombs.** Total output is capped at [`MAX_TOTAL_OUTPUT`],
//!    and a single entry at [`MAX_ENTRY_OUTPUT`]. The running total is added
//!    with `checked_add`, because the declared size is a number the archive
//!    chose and `u64::MAX` is a legal choice.
//! 3. **A declared size that is a lie.** The budget is computed from the
//!    claim, but the bytes are measured on arrival, and an entry that hands
//!    back more than its claim is refused rather than written.
//! 4. **No silent overwrites.** An entry landing on an existing file is
//!    skipped unless the caller asked for something else.
//!
//! The original archive is never modified, and nothing is written outside
//! `dest`.
//!
//! This module was `core/lha/safe_extract.rs` and kept its behaviour exactly;
//! what changed is that the format-specific half now lives behind
//! [`ArchiveBackend`](super::ArchiveBackend), so ZIP and 7z inherit these four
//! rather than each growing their own copy.

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::ArchiveBackend;
use crate::core::error::{CoreError, CoreResult};
use crate::core::jobs::ProgressSink;
use crate::core::security::{safe_join, PathTraversalError};

/// Maximum total extracted size per operation (2 GB). Protects against
/// decompression bombs while easily accommodating any historical Amiga archive.
pub const MAX_TOTAL_OUTPUT: u64 = 2 * 1024 * 1024 * 1024;

/// Maximum size of any single entry (256 MB).
///
/// The gate hands a backend a whole entry's bytes at a time, so this is also
/// the largest allocation an archive can provoke. The biggest thing ART
/// legitimately unpacks is a WHDLoad pack — tens of megabytes — so this is
/// generous by an order of magnitude and still bounded, which is the point.
pub const MAX_ENTRY_OUTPUT: u64 = 256 * 1024 * 1024;

/// How many entries ART will look at. A hostile archive can declare millions.
pub const MAX_ENTRIES: usize = 100_000;

/// What to do when an entry's destination already exists on disk.
///
/// Defaults to [`OverwritePolicy::Skip`]: extracting an archive over a folder
/// the user has already worked in must not destroy their files without asking.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OverwritePolicy {
    /// Keep the file that is already there and report the entry as skipped.
    #[default]
    Skip,
    /// Replace the existing file.
    Overwrite,
    /// Keep both, writing the new one as `name (1).ext`.
    Rename,
}

/// Result of extracting a single archive entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtractedEntry {
    pub source_path: String,
    pub destination: String,
    pub bytes: u64,
    pub is_dir: bool,
    pub skipped: bool,
    pub reason: Option<String>,
}

/// Overall outcome of an extraction operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtractOutcome {
    pub total_files: usize,
    pub total_bytes: u64,
    pub extracted: Vec<ExtractedEntry>,
    pub errors: Vec<String>,
    pub aborted: bool,
    pub abort_reason: Option<String>,
    /// Entries left in place because a file already existed there.
    pub skipped_existing: usize,
}

impl ExtractOutcome {
    fn new() -> Self {
        Self {
            total_files: 0,
            total_bytes: 0,
            extracted: Vec::new(),
            errors: Vec::new(),
            aborted: false,
            abort_reason: None,
            skipped_existing: 0,
        }
    }

    fn refuse(&mut self, name: &str, is_dir: bool, reason: String) {
        self.extracted.push(ExtractedEntry {
            source_path: name.to_string(),
            destination: String::new(),
            bytes: 0,
            is_dir,
            skipped: true,
            reason: Some(reason),
        });
    }
}

/// Pick a free path next to `target` (`Game.exe` → `Game (1).exe`).
pub fn next_free_path(target: &Path) -> CoreResult<PathBuf> {
    if !target.exists() {
        return Ok(target.to_path_buf());
    }
    let parent = target.parent().unwrap_or_else(|| Path::new("."));
    let stem = target
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "file".into());
    let ext = target
        .extension()
        .map(|e| format!(".{}", e.to_string_lossy()))
        .unwrap_or_default();

    for n in 1..10_000 {
        let candidate = parent.join(format!("{stem} ({n}){ext}"));
        if !candidate.exists() {
            return Ok(candidate);
        }
    }
    Err(CoreError::InvalidInput(format!(
        "could not find a free name next to '{}'",
        target.display()
    )))
}

/// Extract everything `backend` holds into `dest`, safely.
///
/// Cancellation is checked between entries, never mid-file, so stopping leaves
/// completed files intact and no partial one behind.
pub fn extract_with_backend(
    backend: &mut dyn ArchiveBackend,
    dest: &Path,
    overwrite: OverwritePolicy,
    progress: &dyn ProgressSink,
) -> CoreResult<ExtractOutcome> {
    let format = backend.format();
    let entries = backend.entries()?;

    fs::create_dir_all(dest)?;

    let mut outcome = ExtractOutcome::new();
    let mut total_written: u64 = 0;

    if entries.len() > MAX_ENTRIES {
        outcome.aborted = true;
        outcome.abort_reason = Some(format!(
            "this {format} archive declares {} entries; ART reads at most {MAX_ENTRIES}",
            entries.len()
        ));
        return Ok(outcome);
    }

    for (index, entry) in entries.iter().enumerate() {
        // Between entries nothing is half-written, so this is where stopping is
        // safe. The count is known here, unlike the streaming reader this
        // replaced, so progress carries a total.
        if progress.is_cancelled() {
            return Err(crate::core::jobs::cancelled_error());
        }
        progress.report(index as u64, Some(entries.len() as u64), &entry.name);

        // Bomb guard, on the claim: a hostile archive can declare a size that
        // overflows the running total, so the addition itself is checked.
        let projected = total_written.checked_add(entry.declared_bytes);
        if projected.map_or(true, |p| p > MAX_TOTAL_OUTPUT) {
            outcome.aborted = true;
            outcome.abort_reason = Some(format!(
                "extraction would exceed the {MAX_TOTAL_OUTPUT} byte safety limit"
            ));
            break;
        }
        if entry.declared_bytes > MAX_ENTRY_OUTPUT {
            outcome.errors.push(format!(
                "'{}' declares {} bytes, past the {MAX_ENTRY_OUTPUT} byte per-entry limit",
                entry.name, entry.declared_bytes
            ));
            outcome.refuse(
                &entry.name,
                entry.is_dir,
                format!("larger than the {MAX_ENTRY_OUTPUT} byte per-entry limit"),
            );
            continue;
        }

        // Path traversal defence.
        let target = match safe_join(dest, &entry.name) {
            Ok(p) => p,
            Err(PathTraversalError::Empty) => {
                outcome.refuse(&entry.name, entry.is_dir, "empty entry name".into());
                continue;
            }
            Err(e) => {
                let reason = e.to_string();
                outcome
                    .errors
                    .push(format!("rejected entry '{}': {reason}", entry.name));
                outcome.refuse(&entry.name, entry.is_dir, reason);
                continue;
            }
        };

        if entry.is_dir {
            fs::create_dir_all(&target)?;
            outcome.extracted.push(ExtractedEntry {
                source_path: entry.name.clone(),
                destination: target.to_string_lossy().into_owned(),
                bytes: 0,
                is_dir: true,
                skipped: false,
                reason: None,
            });
            continue;
        }

        // Decide where — or whether — this entry may be written.
        let write_to = if target.exists() {
            match overwrite {
                OverwritePolicy::Skip => {
                    outcome.skipped_existing += 1;
                    outcome.extracted.push(ExtractedEntry {
                        source_path: entry.name.clone(),
                        destination: target.to_string_lossy().into_owned(),
                        bytes: 0,
                        is_dir: false,
                        skipped: true,
                        reason: Some("a file already exists at this path".into()),
                    });
                    continue;
                }
                OverwritePolicy::Overwrite => target.clone(),
                OverwritePolicy::Rename => next_free_path(&target)?,
            }
        } else {
            target.clone()
        };

        // The bytes, bounded twice over: by what is left of the total budget
        // and by the per-entry cap. A backend that comes back with more than
        // it was allowed has already been stopped by its own loop; a backend
        // that returns more than it *declared* is caught here.
        let remaining = MAX_TOTAL_OUTPUT - total_written;
        let limit = remaining.min(MAX_ENTRY_OUTPUT);
        let data = match backend.read(index, limit) {
            Ok(data) => data,
            Err(e) => {
                let reason = format!("reading '{}' from this {format} failed: {e}", entry.name);
                outcome.errors.push(reason.clone());
                outcome.refuse(&entry.name, false, reason);
                continue;
            }
        };

        let produced = data.len() as u64;
        if produced > entry.declared_bytes {
            // The classic bomb: declare four bytes, decompress to gigabytes.
            // Nothing is written, and the archive is called out by name.
            let reason = format!(
                "'{}' declared {} bytes and produced {produced}",
                entry.name, entry.declared_bytes
            );
            outcome.errors.push(reason.clone());
            outcome.refuse(&entry.name, false, reason);
            continue;
        }

        if let Some(parent) = write_to.parent() {
            fs::create_dir_all(parent)?;
        }
        // Written whole or not at all — never a truncated file standing in for
        // the entry (`core/safety`'s rule, and the same reason the streaming
        // version removed its partial output on failure).
        if let Err(e) = crate::core::safety::atomic::atomic_write(&write_to, &data) {
            let reason = format!("writing '{}' failed: {e}", entry.name);
            outcome.errors.push(reason.clone());
            outcome.refuse(&entry.name, false, reason);
            continue;
        }

        total_written += produced;
        outcome.total_bytes += produced;
        outcome.total_files += 1;
        outcome.extracted.push(ExtractedEntry {
            source_path: entry.name.clone(),
            destination: write_to.to_string_lossy().into_owned(),
            bytes: produced,
            is_dir: false,
            skipped: false,
            reason: None,
        });
    }

    Ok(outcome)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::archive::ArchiveEntry;
    use crate::core::jobs::NoProgress;

    fn scratch(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "art-archive-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// A backend that says whatever a test needs it to say.
    ///
    /// The gate's own defences are about what it does with a backend's
    /// *answers*, so the answers have to be controllable — including the
    /// dishonest ones no real archive library will produce on demand.
    struct Liar {
        entries: Vec<ArchiveEntry>,
        payload: Vec<u8>,
    }

    impl ArchiveBackend for Liar {
        fn format(&self) -> &'static str {
            "test"
        }
        fn entries(&mut self) -> CoreResult<Vec<ArchiveEntry>> {
            Ok(self.entries.clone())
        }
        fn read(&mut self, _index: usize, _limit: u64) -> CoreResult<Vec<u8>> {
            Ok(self.payload.clone())
        }
    }

    fn entry(name: &str, declared: u64) -> ArchiveEntry {
        ArchiveEntry {
            name: name.to_string(),
            is_dir: false,
            declared_bytes: declared,
        }
    }

    /// The bomb that hides behind an honest-looking listing: four bytes
    /// declared, a megabyte delivered. Refused, and nothing written.
    #[test]
    fn an_entry_that_produces_more_than_it_declared_is_refused() {
        let dir = scratch("liar");
        let dest = dir.join("out");
        let mut backend = Liar {
            entries: vec![entry("small.txt", 4)],
            payload: vec![b'x'; 1024 * 1024],
        };

        let outcome =
            extract_with_backend(&mut backend, &dest, OverwritePolicy::Skip, &NoProgress).unwrap();

        assert_eq!(outcome.total_files, 0);
        assert!(!dest.join("small.txt").exists(), "nothing may be written");
        assert!(
            outcome.errors[0].contains("declared 4 bytes and produced"),
            "{:?}",
            outcome.errors
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    /// A single entry claiming more than the per-entry cap is refused before
    /// the backend is asked for anything at all.
    #[test]
    fn an_entry_larger_than_the_per_entry_cap_is_refused() {
        let dir = scratch("huge-entry");
        let dest = dir.join("out");
        let mut backend = Liar {
            entries: vec![entry("huge.bin", MAX_ENTRY_OUTPUT + 1), entry("ok.txt", 2)],
            payload: b"hi".to_vec(),
        };

        let outcome =
            extract_with_backend(&mut backend, &dest, OverwritePolicy::Skip, &NoProgress).unwrap();

        assert!(
            !outcome.aborted,
            "one oversized entry is not a dead archive"
        );
        assert_eq!(outcome.total_files, 1, "the honest entry still lands");
        assert_eq!(std::fs::read(dest.join("ok.txt")).unwrap(), b"hi");
        assert!(!dest.join("huge.bin").exists());
        std::fs::remove_dir_all(&dir).ok();
    }

    /// `u64::MAX` is a legal number to write in a header, and it is what
    /// makes an unchecked running total wrap into something that fits.
    #[test]
    fn a_declared_size_that_would_overflow_the_running_total_aborts() {
        let dir = scratch("overflow");
        let dest = dir.join("out");
        let mut backend = Liar {
            entries: vec![entry("boom.bin", u64::MAX)],
            payload: Vec::new(),
        };

        let outcome =
            extract_with_backend(&mut backend, &dest, OverwritePolicy::Skip, &NoProgress).unwrap();

        assert!(outcome.aborted);
        assert!(
            outcome.abort_reason.unwrap().contains("safety limit"),
            "the user is told why"
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    /// More entries than ART will look at is refused as a whole rather than
    /// half-extracted.
    #[test]
    fn an_archive_with_more_entries_than_the_cap_is_refused() {
        let dir = scratch("many");
        let dest = dir.join("out");
        let mut backend = Liar {
            entries: (0..MAX_ENTRIES + 1)
                .map(|i| entry(&format!("f{i}.txt"), 1))
                .collect(),
            payload: b"x".to_vec(),
        };

        let outcome =
            extract_with_backend(&mut backend, &dest, OverwritePolicy::Skip, &NoProgress).unwrap();

        assert!(outcome.aborted);
        assert_eq!(outcome.total_files, 0);
        std::fs::remove_dir_all(&dir).ok();
    }
}
