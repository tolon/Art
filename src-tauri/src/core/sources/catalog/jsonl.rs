//! The catalog on disk, as JSON Lines.
//!
//! Same shape and the same reasoning as `core/oplog/jsonl.rs`: one JSON object
//! per line is trivially exportable, survives a crash with at most one damaged
//! line, and needs no schema migration as fields are added — an entry written
//! by an older ART simply lacks the newer fields.
//!
//! §41.5.2 says SQLite. A file plus an in-memory index is the same contract
//! behind the same trait: a linear scan over ninety thousand rows costs single
//! milliseconds, and [`CatalogStore`] exists precisely so a SQLite-backed store
//! can replace this one without anything above it changing. See
//! `docs/design-software-sources.md`.
//!
//! Writes go through `core/safety::atomic_write` like every other write in ART.
//! A catalog is not user data, but a half-written one would make ART claim
//! packages do not exist.

use std::path::{Path, PathBuf};

use super::{CatalogStats, CatalogStore, MemoryCatalogStore, SourceQuery};
use crate::core::error::{CoreError, CoreResult};
use crate::core::safety::atomic::atomic_write;
use crate::core::sources::{PackageMeta, PackageRef};

/// The most catalog bytes ART will read from disk.
///
/// Aminet's catalog lands near 20 MB. This is a ceiling on a corrupted or
/// hostile file, not a target.
const MAX_CATALOG_BYTES: u64 = 256 * 1024 * 1024;

/// A catalog kept in a JSON Lines file, indexed in memory.
#[derive(Debug)]
pub struct JsonlCatalogStore {
    path: PathBuf,
    index: MemoryCatalogStore,
}

/// What loading a catalog file found.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LoadReport {
    pub loaded: usize,
    /// Lines that could not be read. A non-zero count means the file was
    /// damaged; the catalog is still usable, just short.
    pub damaged: usize,
}

impl JsonlCatalogStore {
    /// Open the catalog at `path`, or start empty when there is none.
    ///
    /// A missing catalog is the normal state before the first sync, not an
    /// error — ART works offline with no catalog at all (§60/§94).
    pub fn load(path: impl Into<PathBuf>) -> CoreResult<(Self, LoadReport)> {
        let path = path.into();

        let (entries, report) = if path.exists() {
            read_entries(&path)?
        } else {
            (Vec::new(), LoadReport::default())
        };

        Ok((
            Self {
                path,
                index: MemoryCatalogStore::from_entries(entries),
            },
            report,
        ))
    }

    /// An empty catalog at `path`, without reading whatever is there.
    ///
    /// For the case where the existing file cannot be read at all: ART keeps
    /// the same path and starts empty, so the next sync simply overwrites it.
    /// Falling back to a *different* path would work once and then quietly
    /// leave the broken original in place forever.
    pub fn empty_at(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            index: MemoryCatalogStore::new(),
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Write the whole catalog out atomically.
    fn persist(&self) -> CoreResult<()> {
        let entries = self.index.snapshot();

        let mut buffer = String::new();
        for entry in &entries {
            // A record that cannot be serialised is a bug, not user input;
            // failing the sync beats writing a catalog with a hole in it.
            let line = serde_json::to_string(entry).map_err(|e| {
                CoreError::InvalidInput(format!("could not serialise a catalog entry: {e}"))
            })?;
            buffer.push_str(&line);
            buffer.push('\n');
        }

        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        atomic_write(&self.path, buffer.as_bytes())
    }
}

fn read_entries(path: &Path) -> CoreResult<(Vec<PackageMeta>, LoadReport)> {
    let size = std::fs::metadata(path)?.len();
    if size > MAX_CATALOG_BYTES {
        return Err(CoreError::InvalidInput(format!(
            "catalog file is {size} bytes, which is larger than ART will read"
        )));
    }

    let text = std::fs::read_to_string(path)?;
    let mut entries = Vec::new();
    let mut report = LoadReport::default();

    for line in text.lines() {
        if line.trim().is_empty() {
            continue;
        }
        match serde_json::from_str::<PackageMeta>(line) {
            Ok(entry) => {
                entries.push(entry);
                report.loaded += 1;
            }
            // A line truncated by a crash, or written by a future version with
            // an incompatible shape. Losing one entry beats losing the catalog.
            Err(_) => report.damaged += 1,
        }
    }

    Ok((entries, report))
}

impl CatalogStore for JsonlCatalogStore {
    fn replace_provider(&self, provider: &str, entries: Vec<PackageMeta>) -> CoreResult<()> {
        self.index.replace_provider(provider, entries)?;
        self.persist()
    }

    fn get(&self, reference: &PackageRef) -> CoreResult<Option<PackageMeta>> {
        self.index.get(reference)
    }

    fn in_directory(&self, provider: &str, directory: &str) -> CoreResult<Vec<PackageMeta>> {
        self.index.in_directory(provider, directory)
    }

    fn search(&self, query: &SourceQuery) -> CoreResult<Vec<PackageMeta>> {
        self.index.search(query)
    }

    fn stats(&self) -> CoreResult<CatalogStats> {
        self.index.stats()
    }
}

#[cfg(test)]
mod tests {
    use super::super::tests::sample_catalog;
    use super::*;
    use crate::core::sources::PROVIDER_AMINET;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "art-catalog-{name}-{}",
            crate::core::test_scratch_id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// The normal state before a first sync. Not an error, not an empty file.
    #[test]
    fn a_missing_catalog_loads_as_an_empty_one() {
        let dir = temp_dir("missing");
        let (store, report) = JsonlCatalogStore::load(dir.join("catalog.jsonl")).unwrap();

        assert_eq!(report, LoadReport::default());
        assert_eq!(store.stats().unwrap().total, 0);
        assert!(!store.path().exists(), "loading must not create the file");

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_synced_catalog_survives_a_restart() {
        let dir = temp_dir("roundtrip");
        let path = dir.join("catalog.jsonl");

        {
            let (store, _) = JsonlCatalogStore::load(&path).unwrap();
            store
                .replace_provider(PROVIDER_AMINET, sample_catalog())
                .unwrap();
        }

        let (reopened, report) = JsonlCatalogStore::load(&path).unwrap();
        assert_eq!(report.loaded, 5);
        assert_eq!(report.damaged, 0);

        // §41.5.2: search works offline after one sync.
        let hits = reopened.search(&SourceQuery::text("amissl")).unwrap();
        assert_eq!(hits.len(), 2);
        assert_eq!(hits[0].name, "AmiSSL-5.5.lha");

        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// A catalog is not user data, but a half-written one would make ART claim
    /// packages do not exist. One damaged line must not cost the rest.
    #[test]
    fn a_damaged_line_costs_one_entry_not_the_catalog() {
        let dir = temp_dir("damaged");
        let path = dir.join("catalog.jsonl");

        {
            let (store, _) = JsonlCatalogStore::load(&path).unwrap();
            store
                .replace_provider(PROVIDER_AMINET, sample_catalog())
                .unwrap();
        }

        // Simulate a crash mid-write: truncate the final line.
        let text = std::fs::read_to_string(&path).unwrap();
        let cut = text.len() - 20;
        std::fs::write(&path, &text[..cut]).unwrap();

        let (reopened, report) = JsonlCatalogStore::load(&path).unwrap();
        assert_eq!(report.damaged, 1);
        assert_eq!(report.loaded, 4);
        assert_eq!(reopened.stats().unwrap().total, 4);

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn an_oversized_catalog_file_is_refused_rather_than_read() {
        let dir = temp_dir("oversized");
        let path = dir.join("catalog.jsonl");

        let file = std::fs::File::create(&path).unwrap();
        file.set_len(MAX_CATALOG_BYTES + 1).unwrap();
        drop(file);

        let err = JsonlCatalogStore::load(&path).unwrap_err();
        assert_eq!(err.code(), "ART-INPUT-INVALID");

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_second_sync_replaces_rather_than_appends() {
        let dir = temp_dir("resync");
        let path = dir.join("catalog.jsonl");

        let (store, _) = JsonlCatalogStore::load(&path).unwrap();
        store
            .replace_provider(PROVIDER_AMINET, sample_catalog())
            .unwrap();
        store
            .replace_provider(PROVIDER_AMINET, sample_catalog())
            .unwrap();

        let (reopened, report) = JsonlCatalogStore::load(&path).unwrap();
        assert_eq!(report.loaded, 5, "a re-sync must not double the catalog");
        assert_eq!(reopened.stats().unwrap().total, 5);

        std::fs::remove_dir_all(&dir).unwrap();
    }
}
