//! LHA, as a backend for the shared gate.
//!
//! The decompression is `delharc`'s, the header interpretation is ART's
//! (`core::lha::entry_path` — level 2 and 3 headers carry the name somewhere
//! else entirely, which is ART-031), and every defence is the gate's.
//!
//! `delharc` reads an archive as a stream: one header, its bytes, then the
//! next. There is no index to seek to. So this keeps a cursor and walks
//! forward, reopening the file when asked for an entry it has already gone
//! past. The gate reads in ascending order, so in practice it never reopens —
//! but a backend that silently returned the *wrong* entry for an out-of-order
//! read would be a defect nobody would notice until a file landed with
//! somebody else's contents in it.

use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};

use delharc::LhaDecodeReader;

use super::{ArchiveBackend, ArchiveEntry};
use crate::core::error::{CoreError, CoreResult};

pub struct LhaBackend {
    path: PathBuf,
    cursor: Option<Cursor>,
}

/// Where the open stream is, and whether the entry it sits on still has its
/// bytes.
///
/// `consumed` is the half that is easy to leave out and impossible to notice
/// afterwards: a decode reader that has already yielded an entry's bytes
/// returns *nothing* if asked again, so a second read of the same index would
/// hand back an empty file rather than the file. It has a test.
struct Cursor {
    reader: LhaDecodeReader<File>,
    at: usize,
    consumed: bool,
}

impl std::fmt::Debug for LhaBackend {
    // `LhaDecodeReader` is not `Debug`, and the path is the only interesting
    // part anyway.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LhaBackend")
            .field("path", &self.path)
            .finish()
    }
}

fn malformed(detail: impl Into<String>) -> CoreError {
    CoreError::Malformed {
        format: "lha".into(),
        detail: detail.into(),
    }
}

impl LhaBackend {
    pub fn open(path: &Path) -> CoreResult<Self> {
        // Opened once here so a file that is not an LHA at all fails at
        // `open`, where the caller can still choose another backend, rather
        // than halfway through an extraction.
        let _ = Self::reader(path)?;
        Ok(Self {
            path: path.to_path_buf(),
            cursor: None,
        })
    }

    fn reader(path: &Path) -> CoreResult<LhaDecodeReader<File>> {
        let file = File::open(path)?;
        LhaDecodeReader::new(file).map_err(|e| malformed(format!("failed to read LHA header: {e}")))
    }

    /// Position the stream at `index`, rewinding by reopening when the cursor
    /// is already past it.
    fn seek_to(&mut self, index: usize) -> CoreResult<&mut LhaDecodeReader<File>> {
        let rewind = match &self.cursor {
            // Past it, or sitting on it with its bytes already handed out.
            Some(cursor) => cursor.at > index || (cursor.at == index && cursor.consumed),
            None => true,
        };
        if rewind {
            self.cursor = Some(Cursor {
                reader: Self::reader(&self.path)?,
                at: 0,
                consumed: false,
            });
        }

        let cursor = self.cursor.as_mut().expect("just set");
        while cursor.at < index {
            let has_more = cursor
                .reader
                .next_file()
                .map_err(|e| malformed(format!("failed to seek past entry {}: {e}", cursor.at)))?;
            if !has_more {
                return Err(malformed(format!(
                    "this archive has no entry {index}; it ended at {}",
                    cursor.at
                )));
            }
            cursor.at += 1;
            cursor.consumed = false;
        }
        cursor.consumed = true;
        Ok(&mut cursor.reader)
    }
}

impl ArchiveBackend for LhaBackend {
    fn format(&self) -> &'static str {
        "lha"
    }

    fn entries(&mut self) -> CoreResult<Vec<ArchiveEntry>> {
        let mut reader = Self::reader(&self.path)?;
        let mut entries = Vec::new();
        loop {
            let header = reader.header();
            let method = String::from_utf8_lossy(&header.compression).to_string();
            entries.push(ArchiveEntry {
                name: crate::core::lha::entry_path(header),
                is_dir: method == "-lhd-",
                declared_bytes: header.original_size,
            });

            let has_more = reader
                .next_file()
                .map_err(|e| malformed(format!("failed to seek past entry: {e}")))?;
            if !has_more {
                break;
            }
        }
        // Leave the cursor unset: `entries` walked a stream of its own, and a
        // cursor claiming a position this reader no longer holds is exactly
        // the bug the reopen logic exists to avoid.
        self.cursor = None;
        Ok(entries)
    }

    fn read(&mut self, index: usize, limit: u64) -> CoreResult<Vec<u8>> {
        let reader = self.seek_to(index)?;

        // Bounded inside the loop, not after it: a `-lh5-` entry that declares
        // four bytes can decompress forever, and a reader that only checked
        // the total afterwards would be out of memory before it got there.
        let mut out = Vec::new();
        let mut buf = [0u8; 8192];
        loop {
            let n = reader
                .read(&mut buf)
                .map_err(|e| malformed(format!("decompressing entry {index} failed: {e}")))?;
            if n == 0 {
                break;
            }
            if out.len() as u64 + n as u64 > limit {
                return Err(malformed(format!(
                    "entry {index} decompresses to more than the {limit} bytes it was allowed"
                )));
            }
            out.extend_from_slice(&buf[..n]);
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::lha::tests::make_minimal_lha;

    fn scratch(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "art-lha-backend-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn it_lists_and_reads_an_archive() {
        let dir = scratch("list");
        let archive = dir.join("test.lha");
        std::fs::write(&archive, make_minimal_lha()).unwrap();

        let mut backend = LhaBackend::open(&archive).unwrap();
        let entries = backend.entries().unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "hi.txt");
        assert_eq!(entries[0].declared_bytes, 2);
        assert_eq!(backend.read(0, 1024).unwrap(), b"hi");

        std::fs::remove_dir_all(&dir).ok();
    }

    /// Reading the same entry twice must give the same bytes twice. The
    /// stream cannot go backwards, so this is the reopen path, and getting it
    /// wrong would put one entry's contents in another entry's file.
    #[test]
    fn reading_an_entry_again_rewinds_rather_than_returning_the_next_one() {
        let dir = scratch("rewind");
        let archive = dir.join("test.lha");
        std::fs::write(&archive, make_minimal_lha()).unwrap();

        let mut backend = LhaBackend::open(&archive).unwrap();
        backend.entries().unwrap();
        assert_eq!(backend.read(0, 1024).unwrap(), b"hi");
        assert_eq!(backend.read(0, 1024).unwrap(), b"hi", "read twice");

        std::fs::remove_dir_all(&dir).ok();
    }

    /// The limit the gate passes is honoured, and honoured as an error rather
    /// than a truncated file.
    #[test]
    fn a_read_past_its_limit_is_an_error_not_a_short_file() {
        let dir = scratch("limit");
        let archive = dir.join("test.lha");
        std::fs::write(&archive, make_minimal_lha()).unwrap();

        let mut backend = LhaBackend::open(&archive).unwrap();
        backend.entries().unwrap();
        let err = backend.read(0, 1).unwrap_err();
        assert_eq!(err.code(), "ART-FORMAT-MALFORMED", "{err}");

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_file_that_is_not_an_lha_fails_at_open() {
        let dir = scratch("not-lha");
        let bogus = dir.join("plain.lha");
        std::fs::write(&bogus, vec![0u8; 512]).unwrap();

        assert!(LhaBackend::open(&bogus).is_err());
        std::fs::remove_dir_all(&dir).ok();
    }
}
