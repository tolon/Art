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

use super::{ArchiveBackend, ArchiveEntry};
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
///
/// `Default` is an outcome that did nothing — the starting point for any
/// extraction that reports through this shape, including the Commodore one in
/// `commands::cbm`, which walks flat media rather than an archive but reports
/// the same counts to the same job event.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
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
        Self::default()
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

/// One thing to write: which entry it is, and what to call it under `dest`.
///
/// The name is separate from the entry because the two callers mean different
/// things by it. Extracting a whole archive writes each entry under the name
/// the archive gave it; copying one folder out of a pane writes the subtree
/// *relative to that folder*, so `Tools/Sub/Deep.txt` lands as `Sub/Deep.txt`
/// under a `Tools` the caller picked. Neither name is trusted: both go through
/// `safe_join` below like everything else.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Wanted {
    pub index: usize,
    pub name: String,
}

/// Extract everything `backend` holds into `dest`, safely.
///
/// Each entry keeps the name the archive gave it. For part of an archive, or
/// for names arranged some other way, see [`extract_selection`].
pub fn extract_with_backend(
    backend: &mut dyn ArchiveBackend,
    dest: &Path,
    overwrite: OverwritePolicy,
    progress: &dyn ProgressSink,
) -> CoreResult<ExtractOutcome> {
    let entries = backend.entries()?;
    let selection: Vec<Wanted> = entries
        .iter()
        .enumerate()
        .map(|(index, entry)| Wanted {
            index,
            name: entry.name.clone(),
        })
        .collect();
    extract_selection(backend, &entries, &selection, dest, overwrite, progress)
}

/// Extract the entries `selection` names, under the names it gives them.
///
/// Two phases, and the split is not cosmetic:
///
/// 1. **Decide.** Every wanted entry is judged on its name and its claim alone
///    — traversal, the caps, the overwrite policy — producing a target path
///    for the ones that may be written and a reported refusal for the ones
///    that may not. Nothing is decompressed to reach any of those answers.
/// 2. **Read what survived**, through [`ArchiveBackend::read_selected`], and
///    write it.
///
/// The alternative — decide and read one entry at a time — reads perfectly
/// well for ZIP and LHA and is quadratic for 7z, whose entries share one
/// compressed block, so reading entry *n* decodes everything before it. This
/// shape lets a solid archive be read in one pass without moving a single
/// decision out of this file.
///
/// Cancellation is checked between entries, never mid-file, so stopping leaves
/// completed files intact and no partial one behind.
pub fn extract_selection(
    backend: &mut dyn ArchiveBackend,
    entries: &[ArchiveEntry],
    selection: &[Wanted],
    dest: &Path,
    overwrite: OverwritePolicy,
    progress: &dyn ProgressSink,
) -> CoreResult<ExtractOutcome> {
    let format = backend.format();

    fs::create_dir_all(dest)?;

    let mut outcome = ExtractOutcome::new();

    if entries.len() > MAX_ENTRIES {
        outcome.aborted = true;
        outcome.abort_reason = Some(format!(
            "this {format} archive declares {} entries; ART reads at most {MAX_ENTRIES}",
            entries.len()
        ));
        return Ok(outcome);
    }

    // ---- phase 1: decide, on names and claims only ------------------------

    let mut wanted = vec![false; entries.len()];
    let mut targets: Vec<Option<PathBuf>> = vec![None; entries.len()];
    let mut budget: u64 = 0;

    for item in selection {
        if progress.is_cancelled() {
            return Err(crate::core::jobs::cancelled_error());
        }
        let Some(entry) = entries.get(item.index) else {
            // A selection pointing outside the archive is a bug in the caller,
            // not something to write blindly.
            outcome.errors.push(format!(
                "'{}' refers to entry {} of an archive that holds {}",
                item.name,
                item.index,
                entries.len()
            ));
            continue;
        };

        // Bomb guard, on the claim: a hostile archive can declare a size that
        // overflows the running total, so the addition itself is checked.
        let projected = budget.checked_add(entry.declared_bytes);
        if projected.is_none_or(|p| p > MAX_TOTAL_OUTPUT) {
            outcome.aborted = true;
            outcome.abort_reason = Some(format!(
                "extraction would exceed the {MAX_TOTAL_OUTPUT} byte safety limit"
            ));
            break;
        }
        if entry.declared_bytes > MAX_ENTRY_OUTPUT {
            outcome.errors.push(format!(
                "'{}' declares {} bytes, past the {MAX_ENTRY_OUTPUT} byte per-entry limit",
                item.name, entry.declared_bytes
            ));
            outcome.refuse(
                &item.name,
                entry.is_dir,
                format!("larger than the {MAX_ENTRY_OUTPUT} byte per-entry limit"),
            );
            continue;
        }

        // Path traversal defence.
        let target = match safe_join(dest, &item.name) {
            Ok(p) => p,
            Err(PathTraversalError::Empty) => {
                outcome.refuse(&item.name, entry.is_dir, "empty entry name".into());
                continue;
            }
            Err(e) => {
                let reason = e.to_string();
                outcome
                    .errors
                    .push(format!("rejected entry '{}': {reason}", item.name));
                outcome.refuse(&item.name, entry.is_dir, reason);
                continue;
            }
        };

        if entry.is_dir {
            fs::create_dir_all(&target)?;
            outcome.extracted.push(ExtractedEntry {
                source_path: item.name.clone(),
                destination: target.to_string_lossy().into_owned(),
                bytes: 0,
                is_dir: true,
                skipped: false,
                reason: None,
            });
            continue;
        }

        // Where — or whether — this entry may be written.
        let write_to = if target.exists() {
            match overwrite {
                OverwritePolicy::Skip => {
                    outcome.skipped_existing += 1;
                    outcome.extracted.push(ExtractedEntry {
                        source_path: item.name.clone(),
                        destination: target.to_string_lossy().into_owned(),
                        bytes: 0,
                        is_dir: false,
                        skipped: true,
                        reason: Some("a file already exists at this path".into()),
                    });
                    continue;
                }
                OverwritePolicy::Overwrite => target,
                OverwritePolicy::Rename => next_free_path(&target)?,
            }
        } else {
            target
        };

        budget += entry.declared_bytes;
        wanted[item.index] = true;
        targets[item.index] = Some(write_to);
    }

    // ---- phase 2: read what survived, and write it ------------------------

    // Every surviving claim fits inside the total budget together, so the
    // per-entry allowance is simply the per-entry cap. A backend returning
    // more than that has been stopped inside its own decompression loop; one
    // returning more than it *declared* is caught below.
    let mut written: Vec<(usize, u64, PathBuf)> = Vec::new();
    let mut failures: Vec<(usize, String)> = Vec::new();

    backend.read_selected(&wanted, MAX_ENTRY_OUTPUT, &mut |index, data| {
        let entry = &entries[index];
        if progress.is_cancelled() {
            return Err(crate::core::jobs::cancelled_error());
        }
        progress.report(index as u64, Some(entries.len() as u64), &entry.name);

        let data = match data {
            Ok(data) => data,
            Err(e) => {
                failures.push((
                    index,
                    format!("reading '{}' from this {format} failed: {e}", entry.name),
                ));
                return Ok(());
            }
        };

        let produced = data.len() as u64;
        if produced > entry.declared_bytes {
            // The classic bomb: declare four bytes, decompress to gigabytes.
            // Nothing is written, and the archive is called out by name.
            failures.push((
                index,
                format!(
                    "'{}' declared {} bytes and produced {produced}",
                    entry.name, entry.declared_bytes
                ),
            ));
            return Ok(());
        }

        let write_to = targets[index]
            .clone()
            .expect("phase 1 gives every wanted entry a target");
        if let Some(parent) = write_to.parent() {
            fs::create_dir_all(parent)?;
        }
        // Written whole or not at all — never a truncated file standing in for
        // the entry (`core/safety`'s rule).
        match crate::core::safety::atomic::atomic_write(&write_to, &data) {
            Ok(()) => written.push((index, produced, write_to)),
            Err(e) => failures.push((index, format!("writing '{}' failed: {e}", entry.name))),
        }
        Ok(())
    })?;

    for (index, bytes, path) in written {
        outcome.total_bytes += bytes;
        outcome.total_files += 1;
        outcome.extracted.push(ExtractedEntry {
            source_path: entries[index].name.clone(),
            destination: path.to_string_lossy().into_owned(),
            bytes,
            is_dir: false,
            skipped: false,
            reason: None,
        });
    }
    for (index, reason) in failures {
        outcome.errors.push(reason.clone());
        outcome.refuse(&entries[index].name, false, reason);
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
