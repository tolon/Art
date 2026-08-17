//! The catalogue on disk (SD-2 · G10 wave A).
//!
//! One file per scanned root, a list of the roots, and the user's own edits kept
//! apart from both. The point of the split is that a refresh rewrites the read
//! layer and **never** the user layer: any other arrangement means every scan
//! destroys what the user corrected by hand.
//!
//! **This module does not know where it writes.** The catalogue directory
//! arrives as a `&Path` from `commands/`, because `core/` is
//! platform-independent and `%APPDATA%` is not — the same rule that gives
//! `CardManifest` a caller-supplied `built_at`.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::core::error::{CoreError, CoreResult};
use crate::core::gameindex::record::{ChipsetRequirement, GameRecord};
use crate::core::hashing::sha256_bytes;
use crate::core::safety::backup::BackupPolicy;
use crate::core::safety::{atomic_write, guarded_write};

/// The catalogue **file format**'s version.
///
/// Distinct from `record::GAMEINDEX_SCHEMA`, which versions a *record*. They move for
/// different reasons: this one when the files change shape, that one when a
/// reader starts producing better facts from the same bytes. Conflating them
/// would make a reader improvement look like a format change and force a
/// migration nobody needs.
pub const CATALOGUE_SCHEMA: u32 = 1;

const ROOTS_FILE: &str = "roots.json";
const OVERRIDES_FILE: &str = "overrides.json";

/// One title as it was read, beside the cheap key that says whether it needs
/// reading again.
///
/// `size` and `mtime_ms` are the whole point: comparing them costs one
/// `metadata()` call, while producing `record` again costs a SHA-256 over the
/// file and a walk of the volume inside it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CachedEntry {
    pub path: String,
    pub size: u64,
    /// Milliseconds since the Unix epoch. Milliseconds rather than seconds
    /// because two writes inside one second are not unusual, and together with
    /// `size` this is what stands between a stale record and a re-read.
    pub mtime_ms: i64,
    pub record: GameRecord,
}

/// Everything ART has read under one root.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CatalogueRoot {
    pub schema: u32,
    pub root: String,
    /// Supplied by the caller: `core` has no clock.
    pub scanned_at: Option<String>,
    /// The value of `record::GAMEINDEX_SCHEMA` when this root was last read. What
    /// makes a reader fix land: an entry read by an older reader is re-read by
    /// the next update even when its path, size and mtime all match.
    pub index_schema: u32,
    pub entries: Vec<CachedEntry>,
}

/// Which roots are catalogued, in the order the user put them in.
///
/// The list and nothing else. A root's own facts — when it was scanned, which
/// reader read it — live in that root's file: two places holding one fact is
/// two places to disagree, and it would mean rewriting this list every time any
/// single root was refreshed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RootsFile {
    pub schema: u32,
    pub roots: Vec<String>,
}

/// One title's hand corrections. Every field absent means "no opinion".
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecordOverride {
    pub title: Option<String>,
    pub year: Option<u16>,
    pub publisher: Option<String>,
    pub genre: Option<String>,
    pub chipset: Option<ChipsetRequirement>,
}

impl RecordOverride {
    /// Whether this override says anything at all. An empty one is deleted
    /// rather than stored, so "I changed my mind" leaves no trace.
    pub fn is_empty(&self) -> bool {
        self.title.is_none()
            && self.year.is_none()
            && self.publisher.is_none()
            && self.genre.is_none()
            && self.chipset.is_none()
    }
}

/// The user layer, keyed by `GameRecord::id`.
///
/// A `BTreeMap` rather than a `HashMap` so the file's key order is stable and
/// two saves of the same edits produce the same bytes.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Overrides {
    pub schema: u32,
    pub edits: BTreeMap<String, RecordOverride>,
}

fn malformed(detail: impl std::fmt::Display) -> CoreError {
    CoreError::Malformed {
        format: "catalogue".into(),
        detail: detail.to_string(),
    }
}

/// The file name a root's catalogue is kept under.
///
/// A readable slug **plus eight hex characters of the path's hash**. The slug
/// alone is not enough: `E:\a\b` and `E:\a-b` produce the same one, and one
/// folder's catalogue silently overwriting another's is the kind of data loss
/// nobody notices until both are wrong. Same shape as `record::derive_id`.
pub fn root_file_name(root: &Path) -> String {
    let text = root.to_string_lossy();
    let mut slug = String::new();
    let mut pending = false;
    for ch in text.chars() {
        if ch.is_ascii_alphanumeric() {
            if pending && !slug.is_empty() {
                slug.push('-');
            }
            pending = false;
            slug.push(ch.to_ascii_lowercase());
        } else {
            pending = true;
        }
    }
    if slug.is_empty() {
        slug.push_str("root");
    }
    let short = &sha256_bytes(text.as_bytes())[..8];
    format!("{slug}-{short}.json")
}

fn read_json<T: for<'de> Deserialize<'de>>(path: &Path) -> CoreResult<Option<T>> {
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(err) => return Err(err.into()),
    };
    serde_json::from_slice(&bytes)
        .map(Some)
        .map_err(|err| malformed(format!("'{}' is not readable: {err}", path.display())))
}

/// Serialise for a machine to read back.
///
/// **Compact, not pretty.** One entry is 824 bytes compact and 1124 pretty —
/// 36% more for nobody's benefit, since nothing but ART reads a root file. At
/// 10 000 entries that is 8 MB against 11.
fn write_json<T: Serialize>(path: &Path, value: &T) -> CoreResult<Vec<u8>> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    serde_json::to_vec(value)
        .map_err(|err| malformed(format!("cannot serialise the catalogue: {err}")))
}

/// Serialise for a person to read.
///
/// Only `overrides.json` uses this: it is the one file here somebody might open
/// to see what they corrected, and it is small enough that the indentation costs
/// nothing worth counting.
fn write_json_pretty<T: Serialize>(path: &Path, value: &T) -> CoreResult<Vec<u8>> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    serde_json::to_vec_pretty(value)
        .map_err(|err| malformed(format!("cannot serialise the catalogue: {err}")))
}

fn refuse_if_newer(found: u32, what: &str) -> CoreResult<()> {
    if found > CATALOGUE_SCHEMA {
        return Err(malformed(format!(
            "this {what} was written by a newer ART (schema {found}, this build reads {CATALOGUE_SCHEMA})"
        )));
    }
    Ok(())
}

pub fn read_roots(dir: &Path) -> CoreResult<RootsFile> {
    match read_json::<RootsFile>(&dir.join(ROOTS_FILE))? {
        Some(file) => {
            refuse_if_newer(file.schema, "catalogue root list")?;
            Ok(file)
        }
        None => Ok(RootsFile {
            schema: CATALOGUE_SCHEMA,
            roots: Vec::new(),
        }),
    }
}

pub fn write_roots(dir: &Path, roots: &RootsFile) -> CoreResult<()> {
    let path = dir.join(ROOTS_FILE);
    let bytes = write_json(&path, roots)?;
    atomic_write(&path, &bytes)
}

pub fn read_root(dir: &Path, root: &Path) -> CoreResult<Option<CatalogueRoot>> {
    match read_json::<CatalogueRoot>(&dir.join(root_file_name(root)))? {
        Some(value) => {
            refuse_if_newer(value.schema, "catalogue file")?;
            Ok(Some(value))
        }
        None => Ok(None),
    }
}

pub fn write_root(dir: &Path, value: &CatalogueRoot) -> CoreResult<()> {
    let path = dir.join(root_file_name(Path::new(&value.root)));
    let bytes = write_json(&path, value)?;
    atomic_write(&path, &bytes)
}

pub fn read_overrides(dir: &Path) -> CoreResult<Overrides> {
    match read_json::<Overrides>(&dir.join(OVERRIDES_FILE))? {
        Some(value) => {
            refuse_if_newer(value.schema, "overrides file")?;
            Ok(value)
        }
        None => Ok(Overrides {
            schema: CATALOGUE_SCHEMA,
            edits: BTreeMap::new(),
        }),
    }
}

/// Write the user layer, keeping the previous version.
///
/// `guarded_write` with `BackupPolicy::CONFIG`, not `atomic_write`: a root file
/// can be rebuilt by rescanning and this cannot. Returns where the backup went,
/// which the command surfaces the way every mutating command in ART does.
pub fn write_overrides(dir: &Path, value: &Overrides) -> CoreResult<Option<PathBuf>> {
    let path = dir.join(OVERRIDES_FILE);
    let bytes = write_json_pretty(&path, value)?;
    guarded_write(&path, &bytes, BackupPolicy::CONFIG)
}

/// Which entries a refresh is willing to trust.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refresh {
    /// Trust a cached entry whose file looks unchanged and whose record was read
    /// by the current reader.
    Update,
    /// Trust nothing on disk that is still there. **Not** "start from zero":
    /// entries whose files have gone are still kept.
    Rescan,
}

/// The cheap identity of a file: its size and its modification time.
///
/// `None` when the file is not there — which is not an error, and is how a
/// refresh recognises an entry whose file has gone.
pub fn file_key(path: &Path) -> Option<(u64, i64)> {
    let meta = std::fs::metadata(path).ok()?;
    let mtime = meta
        .modified()
        .ok()?
        .duration_since(std::time::UNIX_EPOCH)
        .ok()?
        .as_millis();
    Some((meta.len(), mtime as i64))
}

/// Read a root again, reusing what can be reused.
///
/// Never deletes an entry whose file has gone: a catalogue is a library, not a
/// mirror of the disk, and an unplugged drive must not empty it.
pub fn refresh_root(
    dir: &Path,
    root: &Path,
    mode: Refresh,
    scanned_at: Option<String>,
    progress: &dyn crate::core::jobs::ProgressSink,
) -> CoreResult<CatalogueRoot> {
    use crate::core::gameindex::record::GAMEINDEX_SCHEMA;
    use crate::core::gameindex::scan::{collect_indexable, read_one};

    if !root.is_dir() {
        return Err(CoreError::InvalidInput(format!(
            "Directory not found at '{}'",
            root.display()
        )));
    }

    let cached: BTreeMap<String, CachedEntry> = read_root(dir, root)?
        .map(|value| {
            value
                .entries
                .into_iter()
                .map(|entry| (entry.path.clone(), entry))
                .collect()
        })
        .unwrap_or_default();

    progress.report(0, None, "Looking for titles…");
    let files = collect_indexable(root);

    // Decide what needs reading *before* reading anything, so the total the user
    // sees is the work that is actually left.
    let mut reuse: Vec<CachedEntry> = Vec::new();
    let mut to_read: Vec<(PathBuf, u64, i64)> = Vec::new();
    for path in &files {
        let Some((size, mtime_ms)) = file_key(path) else {
            continue;
        };
        let key = path.to_string_lossy().to_string();
        let hit = (mode == Refresh::Update)
            .then(|| cached.get(&key))
            .flatten()
            .filter(|entry| {
                entry.size == size
                    && entry.mtime_ms == mtime_ms
                    && entry.record.schema == GAMEINDEX_SCHEMA
            });
        match hit {
            Some(entry) => reuse.push(entry.clone()),
            None => to_read.push((path.clone(), size, mtime_ms)),
        }
    }

    let total = to_read.len() as u64;
    let mut fresh: Vec<CachedEntry> = Vec::new();
    for (index, (path, size, mtime_ms)) in to_read.into_iter().enumerate() {
        // Between whole files is the only safe place to stop, and nothing has
        // been written yet in any case.
        if progress.is_cancelled() {
            return Err(crate::core::jobs::cancelled_error());
        }
        let short = path
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
            .unwrap_or_default();
        progress.report(index as u64 + 1, Some(total), &short);

        match read_one(&path) {
            Ok(Some(record)) => fresh.push(CachedEntry {
                path: path.to_string_lossy().into(),
                size,
                mtime_ms,
                record,
            }),
            Ok(None) => {}
            Err(err) if matches!(err, CoreError::Cancelled) => return Err(err),
            Err(err) => log::debug!("catalogue: skipping {}: {err}", path.display()),
        }
    }

    // Entries whose files are gone: kept, whichever mode this was.
    let present: std::collections::BTreeSet<String> = reuse
        .iter()
        .chain(fresh.iter())
        .map(|entry| entry.path.clone())
        .collect();
    let missing: Vec<CachedEntry> = cached
        .into_values()
        .filter(|entry| !present.contains(&entry.path))
        .collect();

    let mut entries: Vec<CachedEntry> = reuse.into_iter().chain(fresh).chain(missing).collect();
    entries.sort_by(|a, b| a.path.cmp(&b.path));

    let value = CatalogueRoot {
        schema: CATALOGUE_SCHEMA,
        root: root.to_string_lossy().into(),
        scanned_at,
        index_schema: GAMEINDEX_SCHEMA,
        entries,
    };
    write_root(dir, &value)?;
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::gameindex::record::{Fact, Media, Provenance, SourceRef, GAMEINDEX_SCHEMA};

    fn scratch(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "art-catalogue-{tag}-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn a_record(title: &str) -> GameRecord {
        GameRecord {
            schema: GAMEINDEX_SCHEMA,
            id: format!("{}-00000000", title.to_lowercase().replace(' ', "-")),
            title: Fact::new(title.to_string(), Provenance::WhdloadSlave),
            kind: None,
            year: None,
            publisher: None,
            genre: None,
            rating: None,
            chipset: None,
            kickstart: None,
            media: Media::WhdloadDrawer {
                slave: format!("{title}.slave"),
            },
            preview: None,
            source: SourceRef {
                name: format!("{title}.hdf"),
                sha256: "0".repeat(64),
                bytes: 943_616,
            },
        }
    }

    use crate::core::jobs::NoProgress;

    /// Build a real, readable `.adf` under `dir` and return its path.
    fn a_real_file(dir: &Path, name: &str) -> PathBuf {
        let path = dir.join(name);
        std::fs::write(&path, format!("pretend this is {name}")).unwrap();
        path
    }

    /// A root with one planted entry, so the cache tests all start the same way.
    fn plant(dir: &Path, root: &Path, entry: CachedEntry, index_schema: u32) {
        write_root(
            dir,
            &CatalogueRoot {
                schema: CATALOGUE_SCHEMA,
                root: root.to_string_lossy().into(),
                scanned_at: None,
                index_schema,
                entries: vec![entry],
            },
        )
        .unwrap();
    }

    /// **The cache really skips the read.**
    ///
    /// A cached entry is planted whose `record` says `SENTINEL` — a title the
    /// file's own name could never produce — with the file's real size and
    /// mtime. If Update returns `SENTINEL`, the file was not opened. An
    /// implementation that reads anyway cannot pass this, and no clock or mtime
    /// trickery is needed to prove it.
    #[test]
    fn an_unchanged_file_is_not_read_again() {
        let dir = scratch("cache-hit");
        let root = dir.join("library");
        std::fs::create_dir_all(&root).unwrap();
        let file = a_real_file(&root, "Zool (1992)(Gremlin).adf");
        let (size, mtime_ms) = file_key(&file).unwrap();

        plant(
            &dir,
            &root,
            CachedEntry {
                path: file.to_string_lossy().into(),
                size,
                mtime_ms,
                record: a_record("SENTINEL"),
            },
            GAMEINDEX_SCHEMA,
        );

        let after = refresh_root(&dir, &root, Refresh::Update, None, &NoProgress).unwrap();
        assert_eq!(after.entries.len(), 1);
        assert_eq!(
            after.entries[0].record.title.value, "SENTINEL",
            "the file was re-read when the cache should have answered"
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    /// A record read by an **older reader** is re-read even when path, size and
    /// mtime all match. This is what makes a fix like ART-131 land without the
    /// user knowing to ask for it.
    #[test]
    fn a_record_from_an_older_reader_is_read_again() {
        let dir = scratch("stale-schema");
        let root = dir.join("library");
        std::fs::create_dir_all(&root).unwrap();
        let file = a_real_file(&root, "Zool (1992)(Gremlin).adf");
        let (size, mtime_ms) = file_key(&file).unwrap();

        let mut stale = a_record("SENTINEL");
        stale.schema = GAMEINDEX_SCHEMA - 1;
        plant(
            &dir,
            &root,
            CachedEntry {
                path: file.to_string_lossy().into(),
                size,
                mtime_ms,
                record: stale,
            },
            GAMEINDEX_SCHEMA - 1,
        );

        let after = refresh_root(&dir, &root, Refresh::Update, None, &NoProgress).unwrap();
        assert_eq!(after.entries[0].record.title.value, "Zool");
        assert_eq!(after.index_schema, GAMEINDEX_SCHEMA);

        std::fs::remove_dir_all(&dir).ok();
    }

    /// A file whose size changed is read again.
    #[test]
    fn a_changed_file_is_read_again() {
        let dir = scratch("changed");
        let root = dir.join("library");
        std::fs::create_dir_all(&root).unwrap();
        let file = a_real_file(&root, "Zool (1992)(Gremlin).adf");
        let (_, mtime_ms) = file_key(&file).unwrap();

        plant(
            &dir,
            &root,
            CachedEntry {
                path: file.to_string_lossy().into(),
                size: 1,
                mtime_ms,
                record: a_record("SENTINEL"),
            },
            GAMEINDEX_SCHEMA,
        );

        let after = refresh_root(&dir, &root, Refresh::Update, None, &NoProgress).unwrap();
        assert_eq!(after.entries[0].record.title.value, "Zool");

        std::fs::remove_dir_all(&dir).ok();
    }

    /// **Rescan ignores the cache.** Same planted sentinel, and this time it
    /// must be gone.
    #[test]
    fn a_rescan_reads_everything_present() {
        let dir = scratch("rescan");
        let root = dir.join("library");
        std::fs::create_dir_all(&root).unwrap();
        let file = a_real_file(&root, "Zool (1992)(Gremlin).adf");
        let (size, mtime_ms) = file_key(&file).unwrap();

        plant(
            &dir,
            &root,
            CachedEntry {
                path: file.to_string_lossy().into(),
                size,
                mtime_ms,
                record: a_record("SENTINEL"),
            },
            GAMEINDEX_SCHEMA,
        );

        let after = refresh_root(&dir, &root, Refresh::Rescan, None, &NoProgress).unwrap();
        assert_eq!(after.entries[0].record.title.value, "Zool");

        std::fs::remove_dir_all(&dir).ok();
    }

    /// **A file that has gone keeps its entry** — through an Update *and*
    /// through a Rescan. A catalogue is a library, not a mirror of the disk, and
    /// an unplugged drive must not delete it.
    #[test]
    fn an_entry_whose_file_has_gone_is_kept_by_both_modes() {
        for mode in [Refresh::Update, Refresh::Rescan] {
            let dir = scratch("missing");
            let root = dir.join("library");
            std::fs::create_dir_all(&root).unwrap();

            plant(
                &dir,
                &root,
                CachedEntry {
                    path: root.join("Gone.adf").to_string_lossy().into(),
                    size: 100,
                    mtime_ms: 1,
                    record: a_record("Gone Game"),
                },
                GAMEINDEX_SCHEMA,
            );

            let after = refresh_root(&dir, &root, mode, None, &NoProgress).unwrap();
            assert_eq!(after.entries.len(), 1, "{mode:?} dropped a missing entry");
            assert_eq!(after.entries[0].record.title.value, "Gone Game");

            std::fs::remove_dir_all(&dir).ok();
        }
    }

    /// A refresh reports what it will actually read, not the file count. Three
    /// changed files out of 1699 is "3", which is both the honest number and the
    /// reassuring one.
    #[test]
    fn progress_counts_the_files_that_need_reading() {
        use std::sync::Mutex;

        #[derive(Default)]
        struct Recorder {
            totals: Mutex<Vec<Option<u64>>>,
        }
        impl crate::core::jobs::ProgressSink for Recorder {
            fn report(&self, _done: u64, total: Option<u64>, _message: &str) {
                self.totals.lock().unwrap().push(total);
            }
            fn is_cancelled(&self) -> bool {
                false
            }
        }

        let dir = scratch("progress");
        let root = dir.join("library");
        std::fs::create_dir_all(&root).unwrap();
        let cached = a_real_file(&root, "Cached (1992)(Someone).adf");
        a_real_file(&root, "Fresh (1993)(Someone).adf");
        let (size, mtime_ms) = file_key(&cached).unwrap();

        plant(
            &dir,
            &root,
            CachedEntry {
                path: cached.to_string_lossy().into(),
                size,
                mtime_ms,
                record: a_record("SENTINEL"),
            },
            GAMEINDEX_SCHEMA,
        );

        let sink = Recorder::default();
        refresh_root(&dir, &root, Refresh::Update, None, &sink).unwrap();

        let totals = sink.totals.lock().unwrap();
        assert!(
            totals.contains(&Some(1)),
            "one file needed reading, so the total must be 1: {totals:?}"
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    /// Cancelling between files is a cancellation, never a failure, and the
    /// catalogue on disk is left as it was.
    #[test]
    fn a_cancelled_refresh_leaves_the_catalogue_alone() {
        use crate::core::jobs::CancelToken;

        struct CancelAtOnce(CancelToken);
        impl crate::core::jobs::ProgressSink for CancelAtOnce {
            fn report(&self, _done: u64, _total: Option<u64>, _message: &str) {
                self.0.cancel();
            }
            fn is_cancelled(&self) -> bool {
                self.0.is_cancelled()
            }
        }

        let dir = scratch("cancel");
        let root = dir.join("library");
        std::fs::create_dir_all(&root).unwrap();
        a_real_file(&root, "One (1992)(Someone).adf");
        a_real_file(&root, "Two (1992)(Someone).adf");

        let err = refresh_root(
            &dir,
            &root,
            Refresh::Update,
            None,
            &CancelAtOnce(CancelToken::default()),
        )
        .unwrap_err();
        assert!(matches!(err, CoreError::Cancelled), "{err}");
        assert!(
            read_root(&dir, &root).unwrap().is_none(),
            "a cancelled refresh must not have written a partial catalogue"
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    /// A root file written and read back is the same value.
    #[test]
    fn a_root_file_round_trips() {
        let dir = scratch("round-trip");
        let value = CatalogueRoot {
            schema: CATALOGUE_SCHEMA,
            root: r"E:\amiga\Amigatolon\WHDload".into(),
            scanned_at: Some("2026-08-17T12:00:00Z".into()),
            index_schema: GAMEINDEX_SCHEMA,
            entries: vec![CachedEntry {
                path: r"E:\amiga\Amigatolon\WHDload\Lotus3.hdf".into(),
                size: 943_616,
                mtime_ms: 1_700_000_000_000,
                record: a_record("Lotus 3"),
            }],
        };

        write_root(&dir, &value).unwrap();
        let back = read_root(&dir, Path::new(&value.root)).unwrap().unwrap();
        assert_eq!(back, value);

        std::fs::remove_dir_all(&dir).ok();
    }

    /// A root ART has never scanned is `None`, not an error. An empty catalogue
    /// is the normal first-run state.
    #[test]
    fn an_unscanned_root_reads_as_nothing() {
        let dir = scratch("absent");
        assert!(read_root(&dir, Path::new(r"E:\nowhere")).unwrap().is_none());
        assert_eq!(read_roots(&dir).unwrap().roots, Vec::<String>::new());
        std::fs::remove_dir_all(&dir).ok();
    }

    /// A catalogue directory that does not exist yet is created, not refused.
    #[test]
    fn the_catalogue_directory_is_created_on_first_write() {
        let parent = scratch("mkdir");
        let dir = parent.join("does").join("not").join("exist");

        write_roots(
            &dir,
            &RootsFile {
                schema: CATALOGUE_SCHEMA,
                roots: vec![r"E:\amiga".into()],
            },
        )
        .unwrap();

        assert_eq!(
            read_roots(&dir).unwrap().roots,
            vec![r"E:\amiga".to_string()]
        );
        std::fs::remove_dir_all(&parent).ok();
    }

    /// **Two roots must not share a file.** `E:\a\b` and `E:\a-b` slug
    /// identically; the hash is what keeps them apart, and without it one
    /// folder's catalogue would silently overwrite another's.
    #[test]
    fn two_roots_that_slug_alike_get_different_files() {
        let a = root_file_name(Path::new(r"E:\a\b"));
        let b = root_file_name(Path::new(r"E:\a-b"));
        assert_ne!(a, b, "both were {a}");
        assert!(a.starts_with("e-a-b-"), "{a}");
        assert!(a.ends_with(".json"), "{a}");
    }

    /// A file written by a **newer** ART is refused rather than half-read: the
    /// fields this build does not know about are exactly the ones a later one
    /// used to describe something it cannot check. `CardManifest`'s rule.
    #[test]
    fn a_root_file_from_a_newer_art_is_refused() {
        let dir = scratch("newer");
        let root = Path::new(r"E:\amiga");
        let json = format!(
            r#"{{"schema":{},"root":"E:\\amiga","scanned_at":null,"index_schema":1,"entries":[]}}"#,
            CATALOGUE_SCHEMA + 1
        );
        std::fs::write(dir.join(root_file_name(root)), json).unwrap();

        let err = read_root(&dir, root).unwrap_err();
        assert!(err.to_string().contains("newer"), "{err}");

        std::fs::remove_dir_all(&dir).ok();
    }

    /// A corrupt file is refused with a reason rather than silently starting
    /// empty — losing a catalogue quietly is worse than saying so.
    #[test]
    fn a_corrupt_root_file_is_refused_with_a_reason() {
        let dir = scratch("corrupt");
        let root = Path::new(r"E:\amiga");
        std::fs::write(dir.join(root_file_name(root)), b"{ not json at all").unwrap();

        assert!(read_root(&dir, root).is_err());
        std::fs::remove_dir_all(&dir).ok();
    }

    /// Overrides are user data: writing them backs up the previous version and
    /// returns where it went, the way every guarded write in ART does.
    #[test]
    fn writing_overrides_keeps_the_previous_version() {
        let dir = scratch("overrides");

        let first = Overrides {
            schema: CATALOGUE_SCHEMA,
            edits: [(
                "lotus-3-abcd1234".to_string(),
                RecordOverride {
                    title: Some("Lotus III".into()),
                    ..RecordOverride::default()
                },
            )]
            .into_iter()
            .collect(),
        };
        assert!(
            write_overrides(&dir, &first).unwrap().is_none(),
            "nothing to back up the first time"
        );

        let mut second = first.clone();
        second.edits.get_mut("lotus-3-abcd1234").unwrap().year = Some(1992);
        let backup = write_overrides(&dir, &second).unwrap();
        assert!(backup.is_some(), "the previous overrides must be kept");

        let back = read_overrides(&dir).unwrap();
        assert_eq!(back.edits["lotus-3-abcd1234"].year, Some(1992));
        assert_eq!(
            back.edits["lotus-3-abcd1234"].title.as_deref(),
            Some("Lotus III")
        );

        std::fs::remove_dir_all(&dir).ok();
    }
}
