//! Safe archive extraction with path-traversal, bomb and overwrite defence.
//!
//! Enforces three guarantees (spec §56, §57, §89):
//! 1. **Path-traversal defence**: every archive entry is checked against
//!    `safe_join`. Entries that escape `dest` (e.g. `../../Windows/...`) are
//!    rejected with an error and **never written**.
//! 2. **Zip-bomb defence**: extraction caps total output at 2 GB and refuses
//!    entries whose declared size would overflow the running total.
//! 3. **No silent overwrites**: an entry that would replace an existing file is
//!    skipped by default. The caller has to ask for `Overwrite` explicitly.
//!
//! Originals are untouched.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::core::error::{CoreError, CoreResult};
use crate::core::jobs::{NoProgress, ProgressSink};
use crate::core::security::{safe_join, PathTraversalError};

/// Maximum total extracted size per operation (2 GB). Protects against
/// decompression bombs while easily accommodating any historical Amiga archive.
pub const MAX_TOTAL_OUTPUT: u64 = 2 * 1024 * 1024 * 1024;

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

/// Pick a free path next to `target` (`Game.exe` → `Game (1).exe`).
pub(crate) fn next_free_path(target: &Path) -> CoreResult<PathBuf> {
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

/// Extract an entire LHA archive to `dest` safely.
pub fn extract_archive(
    archive_path: &Path,
    dest: &Path,
    overwrite: OverwritePolicy,
) -> CoreResult<ExtractOutcome> {
    extract_archive_with(archive_path, dest, overwrite, &NoProgress)
}

/// Extract, reporting progress and honouring cancellation.
///
/// Cancellation is checked between entries, never mid-file, so stopping leaves
/// completed files intact and no partial one behind.
pub fn extract_archive_with(
    archive_path: &Path,
    dest: &Path,
    overwrite: OverwritePolicy,
    progress: &dyn ProgressSink,
) -> CoreResult<ExtractOutcome> {
    let file = fs::File::open(archive_path)?;
    let mut reader = delharc::LhaDecodeReader::new(file).map_err(|e| CoreError::Malformed {
        format: "lha".into(),
        detail: format!("failed to read LHA header: {e}"),
    })?;

    fs::create_dir_all(dest)?;

    let mut outcome = ExtractOutcome {
        total_files: 0,
        total_bytes: 0,
        extracted: Vec::new(),
        errors: Vec::new(),
        aborted: false,
        abort_reason: None,
        skipped_existing: 0,
    };
    let mut total_written: u64 = 0;

    loop {
        let header = reader.header().clone();
        let method = String::from_utf8_lossy(&header.compression).to_string();
        let is_dir = method == "-lhd-";
        let entry_name = super::entry_path(&header);

        // Between entries nothing is half-written, so this is where stopping is
        // safe. An archive's entry count is not known up front, so progress is
        // reported as a count without a total.
        if progress.is_cancelled() {
            return Err(crate::core::jobs::cancelled_error());
        }
        progress.report(outcome.total_files as u64, None, &entry_name);

        // Bomb guard: a hostile archive can declare a size that overflows the
        // running total, so the addition itself has to be checked.
        let projected = total_written.checked_add(header.original_size);
        if projected.map_or(true, |p| p > MAX_TOTAL_OUTPUT) {
            outcome.aborted = true;
            outcome.abort_reason = Some(format!(
                "extraction would exceed the {MAX_TOTAL_OUTPUT} byte safety limit"
            ));
            break;
        }

        // Path traversal defence.
        let target = match safe_join(dest, &entry_name) {
            Ok(p) => Some(p),
            Err(PathTraversalError::Empty) => {
                outcome.extracted.push(ExtractedEntry {
                    source_path: entry_name.clone(),
                    destination: String::new(),
                    bytes: 0,
                    is_dir,
                    skipped: true,
                    reason: Some("empty entry name".into()),
                });
                None
            }
            Err(e) => {
                let reason = e.to_string();
                outcome
                    .errors
                    .push(format!("rejected entry '{entry_name}': {reason}"));
                outcome.extracted.push(ExtractedEntry {
                    source_path: entry_name.clone(),
                    destination: String::new(),
                    bytes: 0,
                    is_dir,
                    skipped: true,
                    reason: Some(reason),
                });
                None
            }
        };

        if let Some(target) = target {
            if is_dir {
                fs::create_dir_all(&target)?;
                outcome.extracted.push(ExtractedEntry {
                    source_path: entry_name.clone(),
                    destination: target.to_string_lossy().into_owned(),
                    bytes: 0,
                    is_dir: true,
                    skipped: false,
                    reason: None,
                });
            } else {
                // Decide where — or whether — this entry may be written.
                let resolved = if target.exists() {
                    match overwrite {
                        OverwritePolicy::Skip => None,
                        OverwritePolicy::Overwrite => Some(target.clone()),
                        OverwritePolicy::Rename => Some(next_free_path(&target)?),
                    }
                } else {
                    Some(target.clone())
                };

                match resolved {
                    None => {
                        outcome.skipped_existing += 1;
                        outcome.extracted.push(ExtractedEntry {
                            source_path: entry_name.clone(),
                            destination: target.to_string_lossy().into_owned(),
                            bytes: 0,
                            is_dir: false,
                            skipped: true,
                            reason: Some("a file already exists at this path".into()),
                        });
                    }
                    Some(write_to) => {
                        if let Some(parent) = write_to.parent() {
                            fs::create_dir_all(parent)?;
                        }
                        let mut out = fs::File::create(&write_to)?;
                        let mut written: u64 = 0;
                        let mut buf = [0u8; 8192];
                        let mut failed: Option<String> = None;
                        use std::io::Read as _;
                        loop {
                            let n = match reader.read(&mut buf) {
                                Ok(0) => break,
                                Ok(n) => n,
                                Err(e) => {
                                    failed = Some(format!("decompress '{entry_name}' failed: {e}"));
                                    break;
                                }
                            };
                            total_written += n as u64;
                            written += n as u64;
                            if total_written > MAX_TOTAL_OUTPUT {
                                outcome.aborted = true;
                                outcome.abort_reason = Some(format!(
                                    "extraction exceeded the {MAX_TOTAL_OUTPUT} byte safety limit while writing '{entry_name}'"
                                ));
                                break;
                            }
                            if let Err(e) = out.write_all(&buf[..n]) {
                                failed = Some(format!("writing '{entry_name}' failed: {e}"));
                                break;
                            }
                        }
                        out.flush().ok();
                        drop(out);

                        if failed.is_some() || outcome.aborted {
                            // Never leave a truncated file behind claiming to be
                            // the extracted entry.
                            let _ = fs::remove_file(&write_to);
                            if let Some(reason) = failed {
                                outcome.errors.push(reason.clone());
                                outcome.extracted.push(ExtractedEntry {
                                    source_path: entry_name.clone(),
                                    destination: String::new(),
                                    bytes: 0,
                                    is_dir: false,
                                    skipped: true,
                                    reason: Some(reason),
                                });
                            }
                        } else {
                            outcome.total_bytes += written;
                            outcome.total_files += 1;
                            outcome.extracted.push(ExtractedEntry {
                                source_path: entry_name.clone(),
                                destination: write_to.to_string_lossy().into_owned(),
                                bytes: written,
                                is_dir: false,
                                skipped: false,
                                reason: None,
                            });
                        }
                    }
                }
            }
        }

        if outcome.aborted {
            break;
        }

        let has_more = match reader.next_file() {
            Ok(b) => b,
            Err(e) => {
                outcome
                    .errors
                    .push(format!("reading next entry failed: {e}"));
                break;
            }
        };
        if !has_more {
            break;
        }
    }

    Ok(outcome)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::lha::tests::make_minimal_lha;

    fn scratch(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "art-lha-{tag}-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn make_traversal_lha() -> Vec<u8> {
        let filename = b"../evil.txt";
        let content = b"pwned";
        let csize: u32 = content.len() as u32;
        let usize_: u32 = content.len() as u32;
        let header_len: u8 = (22 + filename.len()) as u8;
        let mut buf = Vec::new();
        buf.push(header_len);
        buf.push(0);
        buf.extend_from_slice(b"-lh0-");
        buf.extend_from_slice(&csize.to_le_bytes());
        buf.extend_from_slice(&usize_.to_le_bytes());
        buf.extend_from_slice(&0u16.to_le_bytes());
        buf.extend_from_slice(&0u16.to_le_bytes());
        buf.push(0x20);
        buf.push(0x00);
        buf.push(filename.len() as u8);
        buf.extend_from_slice(filename);
        buf.extend_from_slice(&0u16.to_le_bytes());
        let cks: u8 = buf[2..2 + header_len as usize]
            .iter()
            .fold(0u8, |a, &b| a.wrapping_add(b));
        buf[1] = cks;
        buf.extend_from_slice(content);
        buf.push(0x00);
        buf
    }

    #[test]
    fn extract_minimal_lha() {
        let dir = scratch("extract");
        let archive = dir.join("test.lha");
        std::fs::write(&archive, make_minimal_lha()).unwrap();
        let dest = dir.join("out");

        let outcome = extract_archive(&archive, &dest, OverwritePolicy::default()).unwrap();
        assert!(!outcome.aborted);
        assert_eq!(outcome.extracted.len(), 1);
        assert_eq!(outcome.extracted[0].bytes, 2);
        assert!(dest.join("hi.txt").exists());
        assert_eq!(std::fs::read(dest.join("hi.txt")).unwrap(), b"hi");

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn traversal_entry_is_rejected_not_extracted() {
        let dir = scratch("trav");
        let archive = dir.join("bad.lha");
        std::fs::write(&archive, make_traversal_lha()).unwrap();
        let dest = dir.join("out");

        let outcome = extract_archive(&archive, &dest, OverwritePolicy::default()).unwrap();
        assert!(!outcome.aborted);
        assert!(!outcome.errors.is_empty());
        assert!(!dir.join("evil.txt").exists());
        assert!(outcome.extracted[0].skipped);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn existing_files_are_skipped_by_default() {
        let dir = scratch("skip");
        let archive = dir.join("test.lha");
        std::fs::write(&archive, make_minimal_lha()).unwrap();
        let dest = dir.join("out");
        std::fs::create_dir_all(&dest).unwrap();
        std::fs::write(dest.join("hi.txt"), b"PRECIOUS USER DATA").unwrap();

        let outcome = extract_archive(&archive, &dest, OverwritePolicy::Skip).unwrap();

        assert_eq!(outcome.skipped_existing, 1);
        assert_eq!(outcome.total_files, 0);
        assert!(outcome.extracted[0].skipped);
        assert_eq!(
            std::fs::read(dest.join("hi.txt")).unwrap(),
            b"PRECIOUS USER DATA",
            "the user's file must survive an extraction over it"
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn overwrite_policy_replaces_when_asked() {
        let dir = scratch("over");
        let archive = dir.join("test.lha");
        std::fs::write(&archive, make_minimal_lha()).unwrap();
        let dest = dir.join("out");
        std::fs::create_dir_all(&dest).unwrap();
        std::fs::write(dest.join("hi.txt"), b"old").unwrap();

        let outcome = extract_archive(&archive, &dest, OverwritePolicy::Overwrite).unwrap();

        assert_eq!(outcome.skipped_existing, 0);
        assert_eq!(outcome.total_files, 1);
        assert_eq!(std::fs::read(dest.join("hi.txt")).unwrap(), b"hi");

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn rename_policy_keeps_both_copies() {
        let dir = scratch("rename");
        let archive = dir.join("test.lha");
        std::fs::write(&archive, make_minimal_lha()).unwrap();
        let dest = dir.join("out");
        std::fs::create_dir_all(&dest).unwrap();
        std::fs::write(dest.join("hi.txt"), b"original").unwrap();

        let outcome = extract_archive(&archive, &dest, OverwritePolicy::Rename).unwrap();

        assert_eq!(outcome.total_files, 1);
        assert_eq!(std::fs::read(dest.join("hi.txt")).unwrap(), b"original");
        assert_eq!(std::fs::read(dest.join("hi (1).txt")).unwrap(), b"hi");

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn next_free_path_walks_past_taken_names() {
        let dir = scratch("free");
        let base = dir.join("Game.exe");
        assert_eq!(next_free_path(&base).unwrap(), base);

        std::fs::write(&base, b"x").unwrap();
        assert_eq!(next_free_path(&base).unwrap(), dir.join("Game (1).exe"));

        std::fs::write(dir.join("Game (1).exe"), b"x").unwrap();
        assert_eq!(next_free_path(&base).unwrap(), dir.join("Game (2).exe"));

        std::fs::remove_dir_all(&dir).ok();
    }
}
