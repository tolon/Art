//! What ART has downloaded, and whether the repository has moved on (§41.5.6).
//!
//! ## Why a record at all
//!
//! Aminet's index has no version column. It has name, directory, size, age in
//! weeks and a one-line description — and the age is *weeks before the index
//! was generated*, not an upload date. So "is there a newer version?" cannot be
//! answered from the catalogue alone: the catalogue only ever describes now.
//!
//! It becomes answerable by remembering what the catalogue said at the moment
//! the file was downloaded, and comparing that against what it says today. Two
//! snapshots of the same entry disagreeing is a re-upload; agreeing is not.
//!
//! ## The age trap
//!
//! Age counts *up* for an unchanged entry: downloaded when the index said 10
//! weeks, re-synced a year later, the same untouched file now reads 62. Only a
//! **decrease** means the package was uploaded again. Getting this backwards
//! would report every package in the library as updated, every time — which is
//! indistinguishable from reporting nothing at all.
//!
//! ## Suggests, never acts
//!
//! Nothing here downloads, replaces or deletes anything. It produces a list for
//! the user to look at (design doc: *"Suggests; never auto-updates"*).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::catalog::{compare_versions, CatalogStore};
use super::index::AGE_CAP_WEEKS;
use super::{PackageMeta, PackageRef};
use crate::core::error::{CoreError, CoreResult};
use crate::core::safety::atomic::atomic_write;

/// The most record bytes ART will read.
///
/// A record is a few hundred bytes and there is one per downloaded package, so
/// even a very large library stays under a megabyte. This is a ceiling on a
/// corrupt or hostile file, not a target.
const MAX_RECORD_BYTES: u64 = 32 * 1024 * 1024;

/// What ART downloaded, and what the catalogue claimed about it at the time.
///
/// Deliberately a snapshot rather than a pointer into the catalogue: the whole
/// point is to still know what yesterday's catalogue said after today's sync
/// has overwritten it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DownloadRecord {
    pub reference: PackageRef,
    /// Where the file was placed on the user's disk. Absolute.
    pub file_path: String,
    /// SHA-256 of the bytes that were downloaded.
    pub sha256: String,
    /// Size the index claimed, and the download matched, at the time.
    pub size_bytes: u64,
    /// Age the index claimed at the time. `None` when the column was missing.
    pub age_weeks: Option<u32>,
    /// Best version claim at the time, if a readme had been read. Usually
    /// `None`: the search screen does not fetch readmes in bulk (§41.5.5).
    pub version: Option<String>,
}

/// Where download records are kept.
///
/// A trait for the same reason [`CatalogStore`] is one: the design doc calls
/// for SQLite eventually, and everything above this line should not care.
pub trait DownloadRecordStore: Send + Sync {
    /// Remember a download, replacing any earlier record of the same package.
    fn record(&self, record: DownloadRecord) -> CoreResult<()>;

    fn all(&self) -> CoreResult<Vec<DownloadRecord>>;

    /// Forget one package. Used when the user removes it from the library.
    fn forget(&self, reference: &PackageRef) -> CoreResult<()>;
}

/// Why ART thinks a downloaded package is out of date.
///
/// Ordered by how much it actually tells you: a version that went up is a
/// statement about the software, a size that changed is a statement about the
/// file, and an age that went backwards is a statement about the upload. All
/// three are shown in the user's words, never as a bare flag.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum NewerReason {
    /// A readme version was known both then and now, and now is higher.
    Version { had: String, now: String },
    /// Same path, different size — the file on Aminet is not the file you got.
    SizeChanged { had: u64, now: u64 },
    /// The age column went backwards, so the entry was uploaded again.
    Reuploaded { had_weeks: u32, now_weeks: u32 },
}

/// Where a downloaded package stands against the current catalogue.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum UpdateState {
    /// The catalogue describes the same file that was downloaded.
    Current,
    /// The catalogue has moved on.
    Newer { reason: NewerReason },
    /// The package is no longer in the catalogue at all.
    ///
    /// Not an error and not a reason to delete anything: Aminet removes
    /// entries, and the copy on the user's disk is still theirs.
    Withdrawn,
    /// ART's record points at a file that is no longer there.
    ///
    /// Reported rather than silently dropped — a user who moved their library
    /// with Explorer should be told why the list looks short, not left to
    /// wonder.
    FileMissing,
}

/// One line of the update view.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PackageUpdate {
    pub reference: PackageRef,
    pub name: String,
    pub file_path: String,
    pub state: UpdateState,
    /// The catalogue's current entry, when there still is one.
    pub current: Option<PackageMeta>,
}

impl PackageUpdate {
    /// Whether this row is something the user might want to act on.
    pub fn is_actionable(&self) -> bool {
        matches!(self.state, UpdateState::Newer { .. })
    }
}

/// Compare every recorded download against the catalogue as it stands now.
///
/// Pure: no network, no clock, no filesystem beyond asking whether each
/// recorded file is still present. Runs entirely off the local catalogue, so it
/// works with nothing connected — it just answers about the last sync (§60).
pub fn check_updates(
    records: &dyn DownloadRecordStore,
    catalog: &dyn CatalogStore,
    exists: &dyn Fn(&Path) -> bool,
) -> CoreResult<Vec<PackageUpdate>> {
    let mut out = Vec::new();

    for record in records.all()? {
        let name = file_name_of(&record.reference.path);
        let current = catalog.get(&record.reference)?;

        let state = if !exists(Path::new(&record.file_path)) {
            UpdateState::FileMissing
        } else {
            match current {
                None => UpdateState::Withdrawn,
                Some(ref meta) => match compare_against(&record, meta) {
                    Some(reason) => UpdateState::Newer { reason },
                    None => UpdateState::Current,
                },
            }
        };

        out.push(PackageUpdate {
            reference: record.reference.clone(),
            name,
            file_path: record.file_path.clone(),
            state,
            current,
        });
    }

    // Actionable rows first, then by name, so the answer to "what should I
    // look at?" is the top of the list rather than a scan of the whole thing.
    out.sort_by(|a, b| {
        b.is_actionable()
            .cmp(&a.is_actionable())
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });

    Ok(out)
}

/// The single comparison, in strength order. `None` means "nothing says this
/// changed" — which is not the same as proving it did not.
fn compare_against(record: &DownloadRecord, current: &PackageMeta) -> Option<NewerReason> {
    // 1. A version known on both sides. The strongest statement available, and
    //    the rarest: readmes are fetched one at a time, on request.
    if let (Some(had), Some(now)) = (
        record.version.as_deref(),
        current.version.as_ref().map(|claim| claim.value.as_str()),
    ) {
        if compare_versions(now, had) == std::cmp::Ordering::Greater {
            return Some(NewerReason::Version {
                had: had.to_string(),
                now: now.to_string(),
            });
        }
    }

    // 2. Same path, different size. The index size is machine-generated, so a
    //    change means the file itself changed.
    if current.size_bytes != record.size_bytes {
        return Some(NewerReason::SizeChanged {
            had: record.size_bytes,
            now: current.size_bytes,
        });
    }

    // 3. Age going backwards. An untouched entry's age only ever grows, so a
    //    decrease is a re-upload. A capped age is not evidence of anything —
    //    everything old enough reports the same number.
    if let (Some(had), Some(now)) = (record.age_weeks, current.age_weeks) {
        if now < had && had != AGE_CAP_WEEKS && now != AGE_CAP_WEEKS {
            return Some(NewerReason::Reuploaded {
                had_weeks: had,
                now_weeks: now,
            });
        }
    }

    None
}

fn file_name_of(path: &str) -> String {
    match path.rfind('/') {
        Some(slash) => path[slash + 1..].to_string(),
        None => path.to_string(),
    }
}

// ---------------------------------------------------------------------------
// The JSON Lines store
// ---------------------------------------------------------------------------

/// Download records in a JSON Lines file.
///
/// Same shape and the same reasoning as the catalog and the operation log: one
/// object per line, survives a crash with at most one damaged line, and gains
/// fields without a migration. Rewritten whole through
/// [`atomic_write`](crate::core::safety::atomic::atomic_write) — a truncated
/// record file would make ART forget downloads it made.
#[derive(Debug)]
pub struct JsonlDownloadRecords {
    path: PathBuf,
    /// Keyed by `provider\0path` so one package has exactly one record and a
    /// re-download replaces rather than appends.
    entries: std::sync::Mutex<BTreeMap<String, DownloadRecord>>,
}

impl JsonlDownloadRecords {
    /// Open the records at `path`, or start empty when there are none.
    ///
    /// A damaged line is skipped, not fatal: losing the memory of one download
    /// costs a row in the update view, while refusing to start costs the whole
    /// feature.
    pub fn load(path: impl Into<PathBuf>) -> CoreResult<Self> {
        let path = path.into();
        let mut entries = BTreeMap::new();

        if path.exists() {
            let size = std::fs::metadata(&path).map_err(CoreError::from)?.len();
            if size > MAX_RECORD_BYTES {
                return Err(CoreError::InvalidInput(format!(
                    "the download record file is {size} bytes, which is larger than ART will read"
                )));
            }

            let text = std::fs::read_to_string(&path).map_err(CoreError::from)?;
            for line in text.lines() {
                let line = line.trim();
                if line.is_empty() {
                    continue;
                }
                if let Ok(record) = serde_json::from_str::<DownloadRecord>(line) {
                    entries.insert(key_of(&record.reference), record);
                }
            }
        }

        Ok(Self {
            path,
            entries: std::sync::Mutex::new(entries),
        })
    }

    /// An empty set of records at `path`, without reading whatever is there.
    ///
    /// For the startup path that has already decided the file on disk is
    /// unusable: ART carries on with no memory of past downloads rather than
    /// refusing to open.
    pub fn empty_at(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            entries: std::sync::Mutex::new(BTreeMap::new()),
        }
    }

    fn flush(&self, entries: &BTreeMap<String, DownloadRecord>) -> CoreResult<()> {
        let mut text = String::new();
        for record in entries.values() {
            text.push_str(&serde_json::to_string(record).map_err(|e| {
                CoreError::InvalidInput(format!("a download record could not be written: {e}"))
            })?);
            text.push('\n');
        }
        atomic_write(&self.path, text.as_bytes())
    }
}

fn key_of(reference: &PackageRef) -> String {
    format!("{}\0{}", reference.provider, reference.path)
}

impl DownloadRecordStore for JsonlDownloadRecords {
    fn record(&self, record: DownloadRecord) -> CoreResult<()> {
        let mut held = self
            .entries
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        held.insert(key_of(&record.reference), record);
        self.flush(&held)
    }

    fn all(&self) -> CoreResult<Vec<DownloadRecord>> {
        let held = self
            .entries
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        Ok(held.values().cloned().collect())
    }

    fn forget(&self, reference: &PackageRef) -> CoreResult<()> {
        let mut held = self
            .entries
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        held.remove(&key_of(reference));
        self.flush(&held)
    }
}

// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::sources::catalog::MemoryCatalogStore;
    use crate::core::sources::{Claim, PROVIDER_AMINET};

    /// An in-memory store, so the comparison logic is tested without a disk.
    #[derive(Default)]
    struct MemoryRecords(std::sync::Mutex<Vec<DownloadRecord>>);

    impl DownloadRecordStore for MemoryRecords {
        fn record(&self, record: DownloadRecord) -> CoreResult<()> {
            let mut held = self.0.lock().unwrap();
            held.retain(|existing| existing.reference != record.reference);
            held.push(record);
            Ok(())
        }
        fn all(&self) -> CoreResult<Vec<DownloadRecord>> {
            Ok(self.0.lock().unwrap().clone())
        }
        fn forget(&self, reference: &PackageRef) -> CoreResult<()> {
            self.0.lock().unwrap().retain(|e| &e.reference != reference);
            Ok(())
        }
    }

    fn reference(path: &str) -> PackageRef {
        PackageRef::new(PROVIDER_AMINET, path)
    }

    fn meta(path: &str, size: u64, age: Option<u32>) -> PackageMeta {
        let (directory, name) = match path.rfind('/') {
            Some(slash) => (path[..slash].to_string(), path[slash + 1..].to_string()),
            None => (String::new(), path.to_string()),
        };
        PackageMeta {
            reference: reference(path),
            name,
            directory,
            size_bytes: size,
            age_weeks: age,
            short: "A package".into(),
            version: None,
            requires: Vec::new(),
            author: None,
            distribution: None,
        }
    }

    fn record(path: &str, size: u64, age: Option<u32>) -> DownloadRecord {
        DownloadRecord {
            reference: reference(path),
            file_path: format!("D:/amiga/{}", file_name_of(path)),
            sha256: "0".repeat(64),
            size_bytes: size,
            age_weeks: age,
            version: None,
        }
    }

    fn always_present(_: &Path) -> bool {
        true
    }

    fn check(records: &MemoryRecords, catalog: &MemoryCatalogStore) -> Vec<PackageUpdate> {
        check_updates(records, catalog, &always_present).unwrap()
    }

    #[test]
    fn an_unchanged_entry_is_current() {
        let records = MemoryRecords::default();
        records
            .record(record("util/libs/AmiSSL.lha", 1000, Some(10)))
            .unwrap();
        let catalog =
            MemoryCatalogStore::from_entries(vec![meta("util/libs/AmiSSL.lha", 1000, Some(10))]);

        let rows = check(&records, &catalog);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].state, UpdateState::Current);
        assert!(!rows[0].is_actionable());
    }

    /// The trap this module exists to avoid. Aminet's age is "weeks before the
    /// index was made", so an untouched entry reads *older* at every later
    /// sync. Treating that as an update would flag the entire library forever.
    #[test]
    fn an_entry_growing_older_is_not_an_update() {
        let records = MemoryRecords::default();
        records
            .record(record("util/libs/AmiSSL.lha", 1000, Some(10)))
            .unwrap();
        // A year later, same file, same size: age has climbed to 62.
        let catalog =
            MemoryCatalogStore::from_entries(vec![meta("util/libs/AmiSSL.lha", 1000, Some(62))]);

        assert_eq!(check(&records, &catalog)[0].state, UpdateState::Current);
    }

    #[test]
    fn an_age_going_backwards_is_a_reupload() {
        let records = MemoryRecords::default();
        records
            .record(record("util/libs/AmiSSL.lha", 1000, Some(40)))
            .unwrap();
        let catalog =
            MemoryCatalogStore::from_entries(vec![meta("util/libs/AmiSSL.lha", 1000, Some(2))]);

        let rows = check(&records, &catalog);
        assert_eq!(
            rows[0].state,
            UpdateState::Newer {
                reason: NewerReason::Reuploaded {
                    had_weeks: 40,
                    now_weeks: 2
                }
            }
        );
    }

    /// Aminet saturates the age column, so everything old enough reports the
    /// same number. A capped value compared against a real one is noise, and
    /// would announce an update for packages nobody has touched since 1996.
    #[test]
    fn a_capped_age_is_never_evidence_of_a_reupload() {
        let records = MemoryRecords::default();
        records
            .record(record("misc/old/Ancient.lha", 500, Some(AGE_CAP_WEEKS)))
            .unwrap();
        let catalog = MemoryCatalogStore::from_entries(vec![meta(
            "misc/old/Ancient.lha",
            500,
            Some(AGE_CAP_WEEKS - 1),
        )]);

        assert_eq!(check(&records, &catalog)[0].state, UpdateState::Current);
    }

    #[test]
    fn a_size_change_is_an_update_in_either_direction() {
        for now in [900u64, 1100] {
            let records = MemoryRecords::default();
            records
                .record(record("util/libs/AmiSSL.lha", 1000, Some(10)))
                .unwrap();
            let catalog =
                MemoryCatalogStore::from_entries(vec![meta("util/libs/AmiSSL.lha", now, Some(10))]);

            assert_eq!(
                check(&records, &catalog)[0].state,
                UpdateState::Newer {
                    reason: NewerReason::SizeChanged { had: 1000, now }
                },
                "a size of {now} against 1000 should read as changed"
            );
        }
    }

    #[test]
    fn a_higher_readme_version_outranks_the_other_signals() {
        let records = MemoryRecords::default();
        let mut had = record("util/libs/AmiSSL.lha", 1000, Some(10));
        had.version = Some("4.9".into());
        records.record(had).unwrap();

        // Size changed too, but the version is the better statement, so it is
        // the one reported.
        let mut current = meta("util/libs/AmiSSL.lha", 2000, Some(10));
        current.version = Some(Claim::from_readme("5.10".into()));
        let catalog = MemoryCatalogStore::from_entries(vec![current]);

        assert_eq!(
            check(&records, &catalog)[0].state,
            UpdateState::Newer {
                reason: NewerReason::Version {
                    had: "4.9".into(),
                    // 5.10 beats 4.9, and beats 5.9 too — segment-wise, not
                    // as a decimal.
                    now: "5.10".into()
                }
            }
        );
    }

    #[test]
    fn a_lower_version_with_an_unchanged_file_is_not_an_update() {
        let records = MemoryRecords::default();
        let mut had = record("util/libs/AmiSSL.lha", 1000, Some(10));
        had.version = Some("5.10".into());
        records.record(had).unwrap();

        let mut current = meta("util/libs/AmiSSL.lha", 1000, Some(10));
        current.version = Some(Claim::from_readme("5.9".into()));
        let catalog = MemoryCatalogStore::from_entries(vec![current]);

        assert_eq!(check(&records, &catalog)[0].state, UpdateState::Current);
    }

    #[test]
    fn a_package_no_longer_in_the_catalogue_is_withdrawn_not_deleted() {
        let records = MemoryRecords::default();
        records
            .record(record("util/libs/Gone.lha", 1000, Some(10)))
            .unwrap();
        let catalog = MemoryCatalogStore::from_entries(Vec::new());

        let rows = check(&records, &catalog);
        assert_eq!(rows[0].state, UpdateState::Withdrawn);
        // Withdrawn is information, not a call to action: there is nothing
        // newer to fetch.
        assert!(!rows[0].is_actionable());
        assert_eq!(rows[0].file_path, "D:/amiga/Gone.lha");
    }

    #[test]
    fn a_file_the_user_moved_away_is_reported_rather_than_dropped() {
        let records = MemoryRecords::default();
        records
            .record(record("util/libs/AmiSSL.lha", 1000, Some(10)))
            .unwrap();
        let catalog =
            MemoryCatalogStore::from_entries(vec![meta("util/libs/AmiSSL.lha", 2000, Some(1))]);

        let rows = check_updates(&records, &catalog, &|_| false).unwrap();
        // Missing beats "newer": telling someone to update a file that is not
        // there would send them looking for the wrong problem.
        assert_eq!(rows[0].state, UpdateState::FileMissing);
    }

    #[test]
    fn updates_are_listed_before_everything_else() {
        let records = MemoryRecords::default();
        records
            .record(record("util/libs/Zzz.lha", 1000, Some(10)))
            .unwrap();
        records
            .record(record("util/libs/Aaa.lha", 1000, Some(10)))
            .unwrap();
        let catalog = MemoryCatalogStore::from_entries(vec![
            meta("util/libs/Zzz.lha", 4000, Some(10)),
            meta("util/libs/Aaa.lha", 1000, Some(10)),
        ]);

        let rows = check(&records, &catalog);
        assert_eq!(rows[0].name, "Zzz.lha", "the actionable row must be first");
        assert_eq!(rows[1].name, "Aaa.lha");
    }

    #[test]
    fn recording_the_same_package_twice_keeps_one_row() {
        let dir = std::env::temp_dir().join("art-records-replace");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("downloads.jsonl");

        let store = JsonlDownloadRecords::load(&file).unwrap();
        store
            .record(record("util/libs/AmiSSL.lha", 1000, Some(40)))
            .unwrap();
        store
            .record(record("util/libs/AmiSSL.lha", 2000, Some(2)))
            .unwrap();

        let reopened = JsonlDownloadRecords::load(&file).unwrap();
        let all = reopened.all().unwrap();
        assert_eq!(all.len(), 1, "a re-download must replace, not append");
        assert_eq!(all[0].size_bytes, 2000);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_damaged_line_costs_one_record_not_the_file() {
        let dir = std::env::temp_dir().join("art-records-damaged");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("downloads.jsonl");

        let store = JsonlDownloadRecords::load(&file).unwrap();
        store
            .record(record("util/libs/One.lha", 1, Some(1)))
            .unwrap();
        store
            .record(record("util/libs/Two.lha", 2, Some(2)))
            .unwrap();

        // Corrupt the first line the way a half-flushed write would.
        let text = std::fs::read_to_string(&file).unwrap();
        let mut lines: Vec<&str> = text.lines().collect();
        lines[0] = "{\"reference\":{\"provi";
        std::fs::write(&file, lines.join("\n")).unwrap();

        let reopened = JsonlDownloadRecords::load(&file).unwrap();
        assert_eq!(reopened.all().unwrap().len(), 1);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn forgetting_a_package_removes_it_from_the_view() {
        let records = MemoryRecords::default();
        records
            .record(record("util/libs/AmiSSL.lha", 1000, Some(10)))
            .unwrap();
        records.forget(&reference("util/libs/AmiSSL.lha")).unwrap();

        let catalog = MemoryCatalogStore::from_entries(Vec::new());
        assert!(check(&records, &catalog).is_empty());
    }
}
