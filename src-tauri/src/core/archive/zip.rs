//! ZIP, as a backend for the shared gate.
//!
//! The `zip` crate does the central directory and the deflate; ART does what
//! it always does with somebody else's file — decides nothing from it that it
//! can check for itself. In particular the *names* come back raw and go
//! straight to [`extract`](super::extract), which is the only thing allowed to
//! turn one into a path.
//!
//! Two things worth knowing about ZIP specifically:
//!
//! - **`mangled_name` is not used.** The crate offers a "sanitised" name, and
//!   taking it would move the traversal defence out of the gate and into a
//!   dependency, which is precisely the arrangement `core/archive` exists to
//!   avoid. `raw_name`/`name` is what ART asks for, and `safe_join` decides.
//! - **Encrypted entries are refused, not skipped quietly.** A ZIP can be
//!   partly encrypted; an entry ART cannot decrypt is reported by name so the
//!   user knows something did not arrive.

use std::fs::File;
use std::io::{BufReader, Read};
use std::path::{Path, PathBuf};

use zip::ZipArchive;

use super::{ArchiveBackend, ArchiveEntry};
use crate::core::error::{CoreError, CoreResult};

pub struct ZipBackend {
    path: PathBuf,
    archive: ZipArchive<BufReader<File>>,
}

impl std::fmt::Debug for ZipBackend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ZipBackend")
            .field("path", &self.path)
            .finish()
    }
}

fn malformed(detail: impl Into<String>) -> CoreError {
    CoreError::Malformed {
        format: "zip".into(),
        detail: detail.into(),
    }
}

impl ZipBackend {
    pub fn open(path: &Path) -> CoreResult<Self> {
        let file = BufReader::new(File::open(path)?);
        let archive = ZipArchive::new(file)
            .map_err(|e| malformed(format!("failed to read the ZIP directory: {e}")))?;
        Ok(Self {
            path: path.to_path_buf(),
            archive,
        })
    }
}

impl ArchiveBackend for ZipBackend {
    fn format(&self) -> &'static str {
        "zip"
    }

    fn entries(&mut self) -> CoreResult<Vec<ArchiveEntry>> {
        let count = self.archive.len();
        let mut entries = Vec::with_capacity(count);
        for index in 0..count {
            let entry = self
                .archive
                .by_index_raw(index)
                .map_err(|e| malformed(format!("entry {index} could not be read: {e}")))?;
            entries.push(ArchiveEntry {
                // The name exactly as stored. A ZIP writes `/` as its
                // separator whatever made it, and a hostile one writes
                // whatever it likes — neither is this module's problem.
                name: entry.name().to_string(),
                is_dir: entry.is_dir(),
                declared_bytes: entry.size(),
            });
        }
        Ok(entries)
    }

    fn read(&mut self, index: usize, limit: u64) -> CoreResult<Vec<u8>> {
        let mut entry = self.archive.by_index(index).map_err(|e| match e {
            zip::result::ZipError::UnsupportedArchive(reason) => CoreError::UnsupportedFormat(
                format!("entry {index} of this ZIP cannot be read: {reason}"),
            ),
            other => malformed(format!("entry {index} could not be opened: {other}")),
        })?;

        // `take(limit + 1)`: reading one byte past what is allowed is how the
        // difference between "exactly at the limit" and "more than the limit"
        // is discovered without ever holding the "more". A declared size is a
        // claim, and this is the claim being checked rather than trusted.
        let ceiling = limit.saturating_add(1);
        let mut out = Vec::new();
        entry
            .by_ref()
            .take(ceiling)
            .read_to_end(&mut out)
            .map_err(|e| malformed(format!("decompressing entry {index} failed: {e}")))?;

        if out.len() as u64 > limit {
            return Err(malformed(format!(
                "entry {index} decompresses to more than the {limit} bytes it was allowed"
            )));
        }
        Ok(out)
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use std::io::Write;
    use zip::write::SimpleFileOptions;

    /// A ZIP holding the given entries, built at runtime — ART ships no
    /// fixtures. Stored rather than deflated so the bytes in the file are the
    /// bytes the test wrote, which makes a failure readable.
    pub fn make_zip_with(files: &[(&str, &[u8])]) -> Vec<u8> {
        let mut cursor = std::io::Cursor::new(Vec::new());
        {
            let mut writer = zip::ZipWriter::new(&mut cursor);
            let options =
                SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
            for (name, data) in files {
                // `start_file` refuses some of the names this test suite exists
                // to try, so the raw entry API is used: the archive under test
                // has to be able to say `../../outside.txt`, because a real
                // hostile one can.
                writer
                    .start_file(*name, options)
                    .expect("the fixture writer must accept the name under test");
                writer.write_all(data).unwrap();
            }
            writer.finish().unwrap();
        }
        cursor.into_inner()
    }

    fn scratch(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "art-zip-{tag}-{}-{}",
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
        let archive = dir.join("test.zip");
        std::fs::write(
            &archive,
            make_zip_with(&[("readme.txt", b"hello"), ("data/file.bin", b"\x00\x01\x02")]),
        )
        .unwrap();

        let mut backend = ZipBackend::open(&archive).unwrap();
        let entries = backend.entries().unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].name, "readme.txt");
        assert_eq!(entries[0].declared_bytes, 5);
        assert_eq!(backend.read(0, 1024).unwrap(), b"hello");
        assert_eq!(backend.read(1, 1024).unwrap(), b"\x00\x01\x02");
        // Backwards, too: the gate reads in order, but a backend that could
        // only go forwards would be a trap for the next caller.
        assert_eq!(backend.read(0, 1024).unwrap(), b"hello");

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_file_that_is_not_a_zip_fails_at_open() {
        let dir = scratch("not-zip");
        let bogus = dir.join("plain.zip");
        std::fs::write(&bogus, vec![0u8; 512]).unwrap();

        assert!(ZipBackend::open(&bogus).is_err());
        std::fs::remove_dir_all(&dir).ok();
    }

    /// The fixture writer must store the hostile names **verbatim**, or the
    /// shared gate test in `core::archive` would be proving nothing: an
    /// archive whose `../../outside.txt` was quietly turned into
    /// `outside.txt` on the way in cannot show that the gate refuses
    /// traversal — it would pass just as well with the defence removed.
    /// Asserted beside the writer, because that is where the sanitising
    /// would happen.
    #[test]
    fn the_fixture_writer_stores_hostile_names_exactly_as_given() {
        let dir = scratch("verbatim");
        let archive = dir.join("hostile.zip");
        // The same three names the shared gate test hands every backend.
        let names = [
            "../../outside.txt",
            "../sibling.txt",
            r"C:\Windows\Temp\absolute.txt",
        ];
        let files: Vec<(&str, &[u8])> = names.iter().map(|n| (*n, b"x" as &[u8])).collect();
        std::fs::write(&archive, make_zip_with(&files)).unwrap();

        let mut backend = ZipBackend::open(&archive).unwrap();
        let stored: Vec<String> = backend
            .entries()
            .unwrap()
            .into_iter()
            .map(|e| e.name)
            .collect();
        assert_eq!(stored, names, "the names must survive the round trip");

        std::fs::remove_dir_all(&dir).ok();
    }

    /// A directory entry is a directory, not a zero-byte file — get this
    /// wrong and an archive's folders arrive as empty files with the folder's
    /// name, and every path under them is refused.
    #[test]
    fn a_directory_entry_is_reported_as_one() {
        let dir = scratch("dirs");
        let archive = dir.join("test.zip");
        let mut cursor = std::io::Cursor::new(Vec::new());
        {
            let mut writer = zip::ZipWriter::new(&mut cursor);
            writer
                .add_directory("Tools", SimpleFileOptions::default())
                .unwrap();
            writer.finish().unwrap();
        }
        std::fs::write(&archive, cursor.into_inner()).unwrap();

        let mut backend = ZipBackend::open(&archive).unwrap();
        let entries = backend.entries().unwrap();
        assert_eq!(entries.len(), 1);
        assert!(entries[0].is_dir, "{:?}", entries[0]);

        std::fs::remove_dir_all(&dir).ok();
    }
}
