//! LHA extraction — the format's entry point into the shared gate.
//!
//! The guarantees this module used to implement (path traversal, bombs,
//! no silent overwrites, §56/§57/§89) now live in
//! [`core::archive::extract`](crate::core::archive::extract), so ZIP and 7z
//! obey exactly the same ones rather than each growing a copy. This is the
//! LHA-shaped door to them, kept because half the codebase calls
//! `extract_archive` by name and because the tests below are worth keeping
//! pointed at a real archive rather than at a test double.
//!
//! Originals are untouched.

use std::path::Path;

use crate::core::archive::lha::LhaBackend;
use crate::core::error::CoreResult;
use crate::core::jobs::{NoProgress, ProgressSink};

pub use crate::core::archive::extract::{next_free_path, ExtractOutcome, OverwritePolicy};

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
    let mut backend = LhaBackend::open(archive_path)?;
    crate::core::archive::extract::extract_with_backend(&mut backend, dest, overwrite, progress)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::lha::tests::make_minimal_lha;
    use std::path::PathBuf;

    fn scratch(tag: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("art-lha-{tag}-{}", crate::core::test_scratch_id()));
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
