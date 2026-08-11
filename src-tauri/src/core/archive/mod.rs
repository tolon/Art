//! Archives, behind one security gate.
//!
//! ART reads more than one archive format, and every one of them is a file
//! somebody else wrote: entry names that try to leave the destination, sizes
//! that are lies, and enough entries to fill a disk. Those defences live in
//! [`extract`] — **once** — and each format supplies only the bytes.
//!
//! That shape is the whole point of this module. Adding ZIP and 7z beside
//! `core/lha` as parallel extractors would have meant three copies of the
//! traversal check, the output cap and the entry limit, and the third copy is
//! where the hole would be. A backend here cannot write a file at all: it
//! answers "what is in this archive" and "give me entry *n*'s bytes, and stop
//! at this many", and the gate does the rest.
//!
//! ```text
//! open(path) ─► Box<dyn ArchiveBackend>
//!                       │  entries() / read(i, limit)
//!                       ▼
//!            extract::extract_with_backend   ← safe_join, the output cap,
//!                       │                      the entry cap, the overwrite
//!                       ▼                      policy, the reporting
//!                  files on disk
//! ```

pub mod extract;
pub mod lha;
pub mod sevenz;
pub mod zip;

use std::path::Path;

use crate::core::error::{CoreError, CoreResult};

/// One entry as the archive describes it — **not** as it will land on disk.
///
/// `name` is raw: whatever bytes the archive carried, decoded as best the
/// backend can. It has not been through `safe_join`, it may be absolute, it
/// may be `../../..`, and it may be empty. Turning it into a path is the
/// gate's job and nobody else's.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArchiveEntry {
    pub name: String,
    pub is_dir: bool,
    /// The size the archive *claims*, which is a claim and not a measurement.
    /// The gate budgets from it and then checks what actually arrives.
    pub declared_bytes: u64,
}

/// What a format has to answer for its bytes to reach the disk.
///
/// Deliberately small, and deliberately unable to write anything.
///
/// `read` takes a `limit` because a declared size is a claim: an archive can
/// say four bytes and decompress to ten gigabytes. A backend that streamed
/// until the data ran out would exhaust memory before the gate ever saw the
/// result, so the bound has to be *inside* the decompression loop. Returning
/// more than `limit` is a backend defect; every backend is tested for it.
pub trait ArchiveBackend {
    /// `"lha"`, `"zip"`, `"7z"` — used in error messages and the operation log.
    fn format(&self) -> &'static str;

    /// Every entry, in the order the archive stores them. The gate reads by
    /// index in this same order, so a sequential backend stays sequential.
    fn entries(&mut self) -> CoreResult<Vec<ArchiveEntry>>;

    /// Entry `index`'s decompressed bytes, stopping with an error rather than
    /// returning more than `limit`.
    fn read(&mut self, index: usize, limit: u64) -> CoreResult<Vec<u8>>;

    /// Deliver the bytes of every entry `wanted` says yes to, in listing
    /// order, each bounded by `limit`.
    ///
    /// The default pulls them one at a time through [`read`](Self::read),
    /// which is what a format with a directory (ZIP) or a rewindable stream
    /// (LHA) wants. **7z overrides it**, and the reason is worth stating: a
    /// 7z archive is solid by default, meaning its entries share one
    /// compressed block, so reading entry *n* on its own decodes everything
    /// before it. Pulling each index in turn is quadratic on exactly the
    /// archives people actually have. Its reader offers one forward pass, and
    /// this is the shape that lets it use it.
    ///
    /// The gate decides *before* calling this what it is willing to write, so
    /// a backend never has to ask what a name means — `wanted[i] == false`
    /// for anything refused or skipped.
    fn read_selected(
        &mut self,
        wanted: &[bool],
        limit: u64,
        sink: &mut dyn FnMut(usize, CoreResult<Vec<u8>>) -> CoreResult<()>,
    ) -> CoreResult<()> {
        for (index, take) in wanted.iter().enumerate() {
            if *take {
                let data = self.read(index, limit);
                sink(index, data)?;
            }
        }
        Ok(())
    }
}

/// Open an archive as whatever it actually is.
///
/// The format comes from [`core::detect`](crate::core::detect), which decides
/// it from the file's own bytes: a `.lha` that is really a ZIP is a ZIP. This
/// deliberately does **not** carry its own copy of the signatures — two
/// modules matching the same magic separately is two things to keep in step,
/// and the one that drifts is the one nobody is looking at.
pub fn open(path: &Path) -> CoreResult<Box<dyn ArchiveBackend>> {
    let detection = crate::core::detect::detect(path)?;

    match detection.format_hint.as_str() {
        "lha" => Ok(Box::new(lha::LhaBackend::open(path)?)),
        "zip" => Ok(Box::new(zip::ZipBackend::open(path)?)),
        "7z" => Ok(Box::new(sevenz::SevenZBackend::open(path)?)),
        _ => Err(CoreError::UnsupportedFormat(format!(
            "'{}' is not an archive ART can open",
            path.display()
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::archive::extract::{extract_with_backend, OverwritePolicy};
    use crate::core::jobs::NoProgress;
    use std::path::PathBuf;

    fn scratch(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "art-archive-gate-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// The same hostile archive, in every format ART reads.
    ///
    /// Three ways out of the destination folder and one harmless file to prove
    /// the archive was really read rather than rejected wholesale. A defence
    /// that only one backend has is the defect this module exists to prevent,
    /// so this list is built once and every backend is run through it —
    /// **each new format adds itself to `backends()` below, and inherits
    /// these assertions rather than writing its own.**
    fn hostile_entries() -> Vec<(&'static str, &'static [u8])> {
        vec![
            ("../../outside.txt", b"escaped" as &[u8]),
            ("../sibling.txt", b"one level up"),
            (r"C:\Windows\Temp\absolute.txt", b"absolute"),
            ("ok.txt", b"harmless"),
        ]
    }

    /// Every format, and how to build one holding the given entries.
    ///
    /// One entry today; ZIP and 7z join it in the same list, which is the
    /// point of the list existing before there is more than one thing in it.
    #[allow(clippy::type_complexity)]
    fn backends() -> Vec<(&'static str, &'static str, fn(&[(&str, &[u8])]) -> Vec<u8>)> {
        vec![
            (
                "lha",
                "hostile.lha",
                crate::core::lha::tests::make_lha_with as fn(&[(&str, &[u8])]) -> Vec<u8>,
            ),
            (
                "zip",
                "hostile.zip",
                zip::tests::make_zip_with as fn(&[(&str, &[u8])]) -> Vec<u8>,
            ),
            (
                "7z",
                "hostile.7z",
                sevenz::tests::make_7z_with as fn(&[(&str, &[u8])]) -> Vec<u8>,
            ),
        ]
    }

    #[test]
    fn every_backend_refuses_the_same_hostile_entries() {
        for (format, filename, build) in backends() {
            let dir = scratch(format);
            let archive = dir.join(filename);
            std::fs::write(&archive, build(&hostile_entries())).unwrap();
            let dest = dir.join("out");

            // Through `open`, so the dispatch is exercised too: the format is
            // decided from the file's own bytes.
            let mut backend = open(&archive)
                .unwrap_or_else(|e| panic!("{format}: open should have recognised it: {e}"));
            assert_eq!(backend.format(), format);

            let outcome =
                extract_with_backend(&mut *backend, &dest, OverwritePolicy::Skip, &NoProgress)
                    .unwrap();

            assert_eq!(
                outcome.total_files, 1,
                "{format}: only the harmless entry may land: {:?}",
                outcome.extracted
            );
            assert_eq!(std::fs::read(dest.join("ok.txt")).unwrap(), b"harmless");

            // Nothing anywhere but inside `dest`.
            assert!(!dir.join("outside.txt").exists(), "{format}: escaped");
            assert!(!dir.join("sibling.txt").exists(), "{format}: escaped");
            assert!(
                !PathBuf::from(r"C:\Windows\Temp\absolute.txt").exists(),
                "{format}: an absolute name was honoured"
            );

            let refused: Vec<&str> = outcome
                .extracted
                .iter()
                .filter(|e| e.skipped)
                .map(|e| e.source_path.as_str())
                .collect();
            assert_eq!(
                refused.len(),
                3,
                "{format}: every hostile entry is reported, not silently dropped: {refused:?}"
            );
            assert!(
                outcome.errors.len() >= 3,
                "{format}: and the user is told: {:?}",
                outcome.errors
            );

            std::fs::remove_dir_all(&dir).ok();
        }
    }

    /// A backend must stop at the limit the gate gives it, in its own
    /// decompression loop. Checking the size afterwards is too late: the
    /// memory is already gone, which is what a decompression bomb is for.
    #[test]
    fn every_backend_stops_at_the_limit_it_is_given() {
        for (format, filename, build) in backends() {
            let dir = scratch(&format!("{format}-limit"));
            let archive = dir.join(filename);
            std::fs::write(&archive, build(&[("big.bin", &vec![b'x'; 4096])])).unwrap();

            let mut backend = open(&archive).unwrap();
            backend.entries().unwrap();
            assert!(
                backend.read(0, 16).is_err(),
                "{format}: a read past its limit must fail rather than return"
            );
            assert_eq!(
                backend.read(0, 8192).unwrap().len(),
                4096,
                "{format}: and the same entry still reads whole when allowed"
            );

            std::fs::remove_dir_all(&dir).ok();
        }
    }
}
