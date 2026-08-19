//! 7z, as a backend for the shared gate.
//!
//! `sevenz-rust2` does the LZMA; ART does what it does with every archive —
//! decides nothing from the file that it can check for itself, and lets
//! [`extract`](super::extract) turn names into paths.
//!
//! **A 7z archive is solid by default**, which shapes this module entirely:
//! its entries share one compressed block, so reading entry *n* on its own
//! decodes everything before it. The reader offers one forward pass over all
//! entries, and [`read_selected`](ArchiveBackend::read_selected) is overridden
//! to use it — pulling entries by index instead would be quadratic on exactly
//! the archives people have. [`read`](ArchiveBackend::read) is still
//! implemented, for a single entry and for the tests, and it costs one pass.
//!
//! Encrypted archives are refused rather than half-read: ART holds no password
//! and asking for one is a feature nobody has asked for yet, so the honest
//! answer is that the archive cannot be opened (§10, §89).

use std::io::Read;
use std::path::{Path, PathBuf};

use sevenz_rust2::{ArchiveReader, Password};

use super::{ArchiveBackend, ArchiveEntry};
use crate::core::error::{CoreError, CoreResult};

pub struct SevenZBackend {
    path: PathBuf,
}

impl std::fmt::Debug for SevenZBackend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SevenZBackend")
            .field("path", &self.path)
            .finish()
    }
}

fn malformed(detail: impl Into<String>) -> CoreError {
    CoreError::Malformed {
        format: "7z".into(),
        detail: detail.into(),
    }
}

impl SevenZBackend {
    pub fn open(path: &Path) -> CoreResult<Self> {
        // Opened once so a file that is not a 7z fails here, where the caller
        // can still choose another backend, rather than halfway through an
        // extraction.
        let _ = Self::reader(path)?;
        Ok(Self {
            path: path.to_path_buf(),
        })
    }

    fn reader(path: &Path) -> CoreResult<ArchiveReader<std::fs::File>> {
        ArchiveReader::open(path, Password::empty())
            .map_err(|e| malformed(format!("failed to read the 7z header: {e}")))
    }
}

impl ArchiveBackend for SevenZBackend {
    fn format(&self) -> &'static str {
        "7z"
    }

    fn entries(&mut self) -> CoreResult<Vec<ArchiveEntry>> {
        let reader = Self::reader(&self.path)?;
        Ok(reader
            .archive()
            .files
            .iter()
            .map(|file| ArchiveEntry {
                // Raw, as stored. 7z uses `/` between components; a hostile
                // archive uses whatever it likes, and neither is this
                // module's business.
                name: file.name.clone(),
                is_dir: file.is_directory,
                declared_bytes: file.size,
            })
            .collect())
    }

    fn read(&mut self, index: usize, limit: u64) -> CoreResult<Vec<u8>> {
        let mut wanted = Vec::new();
        let mut found: Option<CoreResult<Vec<u8>>> = None;
        {
            let reader = Self::reader(&self.path)?;
            let count = reader.archive().files.len();
            if index >= count {
                return Err(malformed(format!(
                    "this archive has no entry {index}; it holds {count}"
                )));
            }
            wanted.resize(count, false);
            wanted[index] = true;
        }

        self.read_selected(&wanted, limit, &mut |_, data| {
            found = Some(data);
            Ok(())
        })?;

        found.unwrap_or_else(|| Err(malformed(format!("entry {index} carries no data stream"))))
    }

    /// One forward pass, which is the only shape a solid archive should be
    /// read in.
    fn read_selected(
        &mut self,
        wanted: &[bool],
        limit: u64,
        sink: &mut dyn FnMut(usize, CoreResult<Vec<u8>>) -> CoreResult<()>,
    ) -> CoreResult<()> {
        if !wanted.iter().any(|w| *w) {
            return Ok(());
        }

        let mut reader = Self::reader(&self.path)?;

        // **Not a counter.** The reader walks its compressed *blocks* first —
        // every entry that carries data, in block order — and the streamless
        // ones (directories, empty files) after them. That is neither the
        // order of `archive().files` nor the same set, so counting yields as
        // though it were puts one entry's bytes into another entry's file the
        // moment an archive holds a directory. Every archive a real tool
        // writes holds directories (ART-079).
        //
        // Matching on the stored name instead is stable under both. Duplicate
        // names keep their order, which is the best a duplicate can be given.
        let mut pending: std::collections::HashMap<String, std::collections::VecDeque<usize>> =
            std::collections::HashMap::new();
        for (index, file) in reader.archive().files.iter().enumerate() {
            if wanted.get(index).copied().unwrap_or(false) {
                pending
                    .entry(file.name.clone())
                    .or_default()
                    .push_back(index);
            }
        }

        // The callback cannot return a `CoreError` through the crate's own
        // signature, so a refusal from the sink is carried out here and
        // re-raised once the pass has ended.
        let mut escaped: Option<CoreError> = None;

        // The closure's error type is the crate's, and nothing here ever
        // produces one — every failure ART cares about is carried to the sink
        // as a `CoreResult` or out through `escaped` — so it has to be
        // spelled out rather than inferred.
        let result = reader.for_each_entries(&mut |entry: &sevenz_rust2::ArchiveEntry,
                                                   stream: &mut dyn Read|
         -> Result<bool, sevenz_rust2::Error> {
            let Some(at) = pending
                .get_mut(entry.name.as_str())
                .and_then(|queue| queue.pop_front())
            else {
                // Not one of the entries the gate asked for — but its bytes
                // still have to be **drained**, not skipped.
                //
                // A 7z block is one compressed stream holding several files
                // end to end, so a file's data only starts where the previous
                // one's ended. Returning without consuming this entry leaves
                // the block reader short, and the *next* wanted file is then
                // decoded from the wrong place: right length, wrong contents,
                // no error. A partial selection is the normal case — the gate
                // skips entries that already exist and refuses hostile names —
                // so this is not an edge (ART-079).
                std::io::copy(
                    &mut stream.take(limit.saturating_add(1)),
                    &mut std::io::sink(),
                )?;
                return Ok(true);
            };

            // `take(limit + 1)`: one byte past what is allowed is how "at the
            // limit" and "past it" are told apart without ever holding the
            // "past it". A declared size is a claim; this is the claim being
            // checked rather than trusted.
            let mut out = Vec::new();
            let read = stream
                .take(limit.saturating_add(1))
                .read_to_end(&mut out)
                .map_err(|e| format!("decompressing entry {at} failed: {e}"));

            let data = match read {
                Err(reason) => Err(malformed(reason)),
                Ok(_) if out.len() as u64 > limit => Err(malformed(format!(
                    "entry {at} decompresses to more than the {limit} bytes it was allowed"
                ))),
                Ok(_) => Ok(out),
            };

            match sink(at, data) {
                Ok(()) => Ok(true),
                Err(e) => {
                    escaped = Some(e);
                    Ok(false)
                }
            }
        });

        if let Some(e) = escaped {
            return Err(e);
        }
        result.map_err(|e| malformed(format!("reading this 7z failed: {e}")))?;
        Ok(())
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use sevenz_rust2::ArchiveWriter;

    /// A 7z holding the given entries, built at runtime — ART ships no
    /// fixtures, and a 7z cannot be hand-assembled the way a level-0 LHA
    /// header can.
    /// A name ending in `/` becomes a **directory entry**, which matters more
    /// than it looks: a directory carries no data stream, and the reader's
    /// `for_each_entries` walks only the entries that do. A fixture set with
    /// no directories in it cannot catch an index that drifts because of one.
    pub fn make_7z_with(files: &[(&str, &[u8])]) -> Vec<u8> {
        let mut writer = ArchiveWriter::new(std::io::Cursor::new(Vec::new()))
            .expect("the fixture writer must start");
        for (name, data) in files {
            if let Some(folder) = name.strip_suffix('/') {
                writer
                    .push_archive_entry::<std::io::Cursor<Vec<u8>>>(
                        sevenz_rust2::ArchiveEntry::new_directory(folder),
                        None,
                    )
                    .expect("the fixture writer must accept a directory");
                continue;
            }
            writer
                .push_archive_entry(
                    sevenz_rust2::ArchiveEntry::new_file(name),
                    Some(std::io::Cursor::new(data.to_vec())),
                )
                .expect("the fixture writer must accept the name under test");
        }
        writer
            .finish()
            .expect("the fixture writer must finish")
            .into_inner()
    }

    fn scratch(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "art-7z-{tag}-{}-{}",
            crate::core::test_scratch_id(),
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
        let archive = dir.join("test.7z");
        std::fs::write(
            &archive,
            make_7z_with(&[("readme.txt", b"hello"), ("data/file.bin", b"\x00\x01\x02")]),
        )
        .unwrap();

        let mut backend = SevenZBackend::open(&archive).unwrap();
        let entries = backend.entries().unwrap();
        assert_eq!(entries.len(), 2, "{entries:?}");
        assert_eq!(entries[0].name, "readme.txt");
        assert_eq!(entries[0].declared_bytes, 5);
        assert_eq!(backend.read(0, 1024).unwrap(), b"hello");
        assert_eq!(backend.read(1, 1024).unwrap(), b"\x00\x01\x02");
        // And again, because a solid archive's reader is a one-way stream and
        // a second read has to start a new pass rather than come back empty.
        assert_eq!(backend.read(0, 1024).unwrap(), b"hello");

        std::fs::remove_dir_all(&dir).ok();
    }

    /// The selective pass is what the gate actually uses: every wanted entry
    /// arrives, in listing order, and nothing else does.
    #[test]
    fn a_selective_pass_delivers_exactly_what_was_asked_for() {
        let dir = scratch("selective");
        let archive = dir.join("test.7z");
        std::fs::write(
            &archive,
            make_7z_with(&[("a.txt", b"aaa"), ("b.txt", b"bbb"), ("c.txt", b"ccc")]),
        )
        .unwrap();

        let mut backend = SevenZBackend::open(&archive).unwrap();
        let mut seen: Vec<(usize, Vec<u8>)> = Vec::new();
        backend
            .read_selected(&[true, false, true], 1024, &mut |index, data| {
                seen.push((index, data.unwrap()));
                Ok(())
            })
            .unwrap();

        assert_eq!(seen.len(), 2, "{seen:?}");
        assert_eq!(seen[0], (0, b"aaa".to_vec()));
        assert_eq!(seen[1], (2, b"ccc".to_vec()));

        std::fs::remove_dir_all(&dir).ok();
    }

    /// ART-079. The reader walks its *blocks* — every entry that carries data
    /// — and then the empty ones, which is neither the order nor the set of
    /// `archive().files`. Counting yields as though they matched that list
    /// puts one entry's bytes in another entry's file the moment the archive
    /// holds a directory, and every archive a real tool writes holds
    /// directories.
    ///
    /// It survived because the fixtures here were all-files: the bug needs a
    /// streamless entry to exist at all. Found by pointing ART at a 7z the
    /// 7-Zip application wrote.
    #[test]
    fn an_archive_with_a_directory_still_reads_each_file_as_itself() {
        let dir = scratch("dir-drift");
        let archive = dir.join("mixed.7z");
        std::fs::write(
            &archive,
            make_7z_with(&[
                ("Tools/", b"" as &[u8]),
                ("ReadMe.txt", b"the readme"),
                ("Tools/Notes.txt", b"the notes"),
            ]),
        )
        .unwrap();

        let mut backend = SevenZBackend::open(&archive).unwrap();
        let entries = backend.entries().unwrap();

        // Whatever order the archive stores them in, each index must read the
        // entry *at that index* — checked by name, not by position.
        for (index, entry) in entries.iter().enumerate() {
            if entry.is_dir {
                continue;
            }
            let expected: &[u8] = match entry.name.as_str() {
                "ReadMe.txt" => b"the readme",
                "Tools/Notes.txt" => b"the notes",
                other => panic!("unexpected entry {other}"),
            };
            assert_eq!(
                backend.read(index, 1024).unwrap(),
                expected,
                "entry {index} ({}) read somebody else's bytes",
                entry.name
            );
        }

        std::fs::remove_dir_all(&dir).ok();
    }

    /// The other half of ART-079: a *partial* selection, which is the normal
    /// case — the gate skips entries that already exist on disk and refuses
    /// hostile names, so most extractions want some entries and not others.
    ///
    /// **What this test cannot prove**, said plainly: the fixture writer here
    /// produces non-solid archives, one block per file, and the defect only
    /// bites in a solid block where a skipped file's bytes have to be drained
    /// for the next one to start in the right place. The proof for that is
    /// `read_foreign_archive_for_oracle_when_asked` pointed at an archive the
    /// 7-Zip application wrote (see `test/README.md`), which is where the
    /// defect was found. This test holds the shape so the plumbing cannot rot.
    #[test]
    fn a_partial_selection_delivers_only_what_was_asked_for_and_gets_it_right() {
        let dir = scratch("partial");
        let archive = dir.join("some.7z");
        std::fs::write(
            &archive,
            make_7z_with(&[
                ("Tools/", b"" as &[u8]),
                ("first.txt", b"the first file"),
                ("second.txt", b"the second file"),
                ("third.txt", b"the third file"),
            ]),
        )
        .unwrap();

        let mut backend = SevenZBackend::open(&archive).unwrap();
        let entries = backend.entries().unwrap();
        let wanted: Vec<bool> = entries
            .iter()
            .map(|e| e.name == "third.txt" || e.name == "first.txt")
            .collect();

        let mut delivered: Vec<(String, Vec<u8>)> = Vec::new();
        backend
            .read_selected(&wanted, 1024, &mut |index, data| {
                delivered.push((entries[index].name.clone(), data.unwrap()));
                Ok(())
            })
            .unwrap();

        delivered.sort_by(|a, b| a.0.cmp(&b.0));
        assert_eq!(delivered.len(), 2, "{delivered:?}");
        assert_eq!(
            delivered[0],
            ("first.txt".into(), b"the first file".to_vec())
        );
        assert_eq!(
            delivered[1],
            ("third.txt".into(), b"the third file".to_vec())
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_file_that_is_not_a_7z_fails_at_open() {
        let dir = scratch("not-7z");
        let bogus = dir.join("plain.7z");
        std::fs::write(&bogus, vec![0u8; 512]).unwrap();

        assert!(SevenZBackend::open(&bogus).is_err());
        std::fs::remove_dir_all(&dir).ok();
    }
}
