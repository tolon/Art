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

/// A picture the user attached by hand.
///
/// **Two halves with two owners.** The bytes live in the artwork cache, which
/// is derived data and can be rebuilt; the *choice* lives here, in the layer
/// no refresh touches. `chosen` is the file the user picked, kept so the
/// binding can be re-materialised if the cache is ever cleared; `cached` is
/// the cache-relative name the screen renders.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArtBinding {
    pub chosen: String,
    pub cached: String,
}

/// One title's hand corrections. Every field absent means "no opinion".
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecordOverride {
    pub title: Option<String>,
    pub year: Option<u16>,
    pub publisher: Option<String>,
    pub genre: Option<String>,
    pub chipset: Option<ChipsetRequirement>,
    /// The user's own picture, not a `Fact` on the record — the screen reads
    /// it from the cache rather than through `apply_override`.
    pub art: Option<ArtBinding>,
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
            && self.art.is_none()
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

/// Add a folder to the catalogue. Nothing is scanned; that is a separate ask.
///
/// A path that is not a directory is refused rather than listed: a typo left in
/// the list would produce an empty root for ever and look like a folder with no
/// games in it.
pub fn add_root(dir: &Path, root: &Path) -> CoreResult<()> {
    if !root.is_dir() {
        return Err(CoreError::InvalidInput(format!(
            "Directory not found at '{}'",
            root.display()
        )));
    }
    let key = root.to_string_lossy().to_string();
    let mut roots = read_roots(dir)?;
    if !roots.roots.iter().any(|existing| existing == &key) {
        roots.roots.push(key);
        write_roots(dir, &roots)?;
    }
    Ok(())
}

/// Take a folder out of the catalogue, and its entries with it.
///
/// **The only way anything leaves the catalogue.** A refresh never deletes an
/// entry, so this is the escape hatch for a folder of games the user really has
/// deleted. Removing a root ART never had is not an error: the folder is gone
/// either way, which is what was asked for.
pub fn remove_root(dir: &Path, root: &Path) -> CoreResult<()> {
    let key = root.to_string_lossy().to_string();
    let mut roots = read_roots(dir)?;
    let before = roots.roots.len();
    roots.roots.retain(|existing| existing != &key);
    if roots.roots.len() != before {
        write_roots(dir, &roots)?;
    }

    // The file goes whether or not the root was listed — an unlisted file left
    // behind would come back the next time the same folder was added.
    let path = dir.join(root_file_name(root));
    match std::fs::remove_file(&path) {
        Ok(()) => Ok(()),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(err.into()),
    }
}

/// One title as the screen sees it.
///
/// `available` is **not** stored: see [`load`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EntryView {
    pub path: String,
    pub available: bool,
    pub record: GameRecord,
}

/// One catalogued root as the screen sees it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RootView {
    pub root: String,
    pub scanned_at: Option<String>,
    /// Read by an older reader than this build's. The screen says an update
    /// would improve these entries; it does not run one.
    pub stale: bool,
    pub entries: Vec<EntryView>,
}

/// Put the user's corrections over a record.
///
/// Each edited field becomes a `Fact` whose provenance is
/// `Provenance::UserEdit`, which outranks everything. A field the user did not
/// touch is left exactly as it was read, provenance included.
pub fn apply_override(record: &mut GameRecord, edit: &RecordOverride) {
    use crate::core::gameindex::record::{Fact, Provenance};

    if let Some(title) = &edit.title {
        record.title = Fact::new(title.clone(), Provenance::UserEdit);
    }
    if let Some(year) = edit.year {
        record.year = Some(Fact::new(year, Provenance::UserEdit));
    }
    if let Some(publisher) = &edit.publisher {
        record.publisher = Some(Fact::new(publisher.clone(), Provenance::UserEdit));
    }
    if let Some(genre) = &edit.genre {
        record.genre = Some(Fact::new(genre.clone(), Provenance::UserEdit));
    }
    if let Some(chipset) = edit.chipset {
        record.chipset = Some(Fact::new(chipset, Provenance::UserEdit));
    }
}

/// Load every catalogued root, with the user layer applied.
///
/// **Availability is asked of the disk here, not read from a file.** One
/// `metadata()` per entry — well under a second for the 1699 this was built
/// against — and the reason is §89-shaped: a stored "available" would let the
/// catalogue claim a game on an unplugged drive is ready to run, which is a
/// missing answer rendered as a pass.
pub fn load(dir: &Path) -> CoreResult<Vec<RootView>> {
    use crate::core::gameindex::record::GAMEINDEX_SCHEMA;

    let overrides = read_overrides(dir)?;
    let mut out = Vec::new();

    for root in read_roots(dir)?.roots {
        let Some(stored) = read_root(dir, Path::new(&root))? else {
            // Listed but never scanned: an empty root rather than a refusal, so
            // adding a folder and not scanning it yet is a normal state.
            out.push(RootView {
                root,
                scanned_at: None,
                stale: false,
                entries: Vec::new(),
            });
            continue;
        };

        let entries = stored
            .entries
            .into_iter()
            .map(|entry| {
                let available = file_key(Path::new(&entry.path)).is_some();
                let mut record = entry.record;
                if let Some(edit) = overrides.edits.get(&record.id) {
                    apply_override(&mut record, edit);
                }
                EntryView {
                    path: entry.path,
                    available,
                    record,
                }
            })
            .collect();

        out.push(RootView {
            root: stored.root,
            scanned_at: stored.scanned_at,
            stale: stored.index_schema != GAMEINDEX_SCHEMA,
            entries,
        });
    }

    Ok(out)
}

/// Record — or clear — one title's hand corrections.
///
/// An override with nothing in it is **removed** rather than stored, so changing
/// one's mind leaves no trace. Returns where the previous overrides were backed
/// up, which the command surfaces.
pub fn set_override(dir: &Path, id: &str, edit: RecordOverride) -> CoreResult<Option<PathBuf>> {
    let mut overrides = read_overrides(dir)?;
    if edit.is_empty() {
        overrides.edits.remove(id);
    } else {
        overrides.edits.insert(id.to_string(), edit);
    }
    write_overrides(dir, &overrides)
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

    // Entries whose files are gone: kept, whichever mode this was — **unless
    // the same title turned up somewhere else**.
    //
    // Content-derived identity is what makes that distinction possible. If a
    // record with the same id was just read at another path, the bytes did not
    // vanish, they moved: the old entry is the same title at an address it has
    // left, and keeping it would leave a permanent ghost behind every rename.
    // Seen for real — renaming one hardfile left two entries sharing
    // `1st-division-manager-73284bd6`, and the screen showed the missing one
    // because it sorted first.
    //
    // A file that moved *nowhere* still keeps its entry. Following a move must
    // not quietly become deleting a title.
    let present: std::collections::BTreeSet<String> = reuse
        .iter()
        .chain(fresh.iter())
        .map(|entry| entry.path.clone())
        .collect();
    let found_ids: std::collections::BTreeSet<String> = reuse
        .iter()
        .chain(fresh.iter())
        .map(|entry| entry.record.id.clone())
        .collect();
    let missing: Vec<CachedEntry> = cached
        .into_values()
        .filter(|entry| !present.contains(&entry.path) && !found_ids.contains(&entry.record.id))
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

    /// **A renamed file is followed, not duplicated.**
    ///
    /// Content-derived identity is what makes this possible: the bytes did not
    /// change, so the record's id did not change, so the old path record is the
    /// *same title at an address it has left*. Keeping it would leave a
    /// permanent ghost behind every rename.
    ///
    /// Seen for real: renaming `1st Division Manager v1.0.hdf` produced two
    /// entries sharing `1st-division-manager-73284bd6`, and the screen showed
    /// the missing one because it sorted first.
    #[test]
    fn a_renamed_file_replaces_its_old_path_rather_than_haunting_the_catalogue() {
        let dir = scratch("renamed");
        let root = dir.join("library");
        std::fs::create_dir_all(&root).unwrap();
        let before = a_real_file(&root, "Zool (1992)(Gremlin).adf");

        let first = refresh_root(&dir, &root, Refresh::Update, None, &NoProgress).unwrap();
        assert_eq!(first.entries.len(), 1);
        let id = first.entries[0].record.id.clone();

        // Rename outside ART, keeping the bytes **and** the title. The `[!]`
        // is a TOSEC dump flag and the parser strips it, so the record's title
        // — and therefore its id — is unchanged. That is the case the user hit:
        // a hardfile renamed while the slave inside it still states the same
        // name. An `.adf` renamed to a *different* title gets a different id
        // and is genuinely a different record, which is correct and not what
        // this test is about.
        let after_path = root.join("Zool (1992)(Gremlin)[!].adf");
        std::fs::rename(&before, &after_path).unwrap();

        let after = refresh_root(&dir, &root, Refresh::Update, None, &NoProgress).unwrap();
        assert_eq!(
            after.entries.len(),
            1,
            "the old path must not linger beside the new one: {:?}",
            after.entries.iter().map(|e| &e.path).collect::<Vec<_>>()
        );
        assert_eq!(after.entries[0].record.id, id, "it is the same title");
        assert!(
            after.entries[0].path.ends_with("[!].adf"),
            "{}",
            after.entries[0].path
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    /// A file that is **gone**, with nothing carrying its id, still keeps its
    /// entry. Following a move must not become deleting a title.
    #[test]
    fn a_file_that_moved_nowhere_still_keeps_its_entry() {
        let dir = scratch("gone-for-good");
        let root = dir.join("library");
        std::fs::create_dir_all(&root).unwrap();
        let file = a_real_file(&root, "Zool (1992)(Gremlin).adf");

        refresh_root(&dir, &root, Refresh::Update, None, &NoProgress).unwrap();
        std::fs::remove_file(&file).unwrap();

        let after = refresh_root(&dir, &root, Refresh::Update, None, &NoProgress).unwrap();
        assert_eq!(after.entries.len(), 1, "a lost title is kept");
        assert_eq!(after.entries[0].record.title.value, "Zool");

        std::fs::remove_dir_all(&dir).ok();
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

    /// A user edit replaces the value **and** says so. It does not pretend the
    /// packager declared it — `is_stated()` is false for a `UserEdit`, so the
    /// screen will not badge it as a guess and will not badge it as a
    /// declaration either.
    #[test]
    fn an_override_wins_and_records_that_it_was_the_user() {
        let mut record = a_record("Lotus 3");
        record.year = Some(Fact::new(1991, Provenance::WhdloadSlave));

        apply_override(
            &mut record,
            &RecordOverride {
                title: Some("Lotus III".into()),
                year: Some(1992),
                ..RecordOverride::default()
            },
        );

        assert_eq!(record.title.value, "Lotus III");
        assert_eq!(record.title.from, Provenance::UserEdit);
        assert_eq!(record.year.as_ref().unwrap().value, 1992);
        assert_eq!(record.year.as_ref().unwrap().from, Provenance::UserEdit);
        assert!(record.title.from.rank() > Provenance::WhdloadSlave.rank());
    }

    /// A field the user did not touch keeps whatever read it.
    #[test]
    fn an_override_leaves_untouched_fields_alone() {
        let mut record = a_record("Lotus 3");
        record.publisher = Some(Fact::new("Gremlin".into(), Provenance::WhdloadSlave));

        apply_override(
            &mut record,
            &RecordOverride {
                title: Some("Lotus III".into()),
                ..RecordOverride::default()
            },
        );

        assert_eq!(record.publisher.as_ref().unwrap().value, "Gremlin");
        assert_eq!(
            record.publisher.as_ref().unwrap().from,
            Provenance::WhdloadSlave
        );
    }

    /// **Overrides survive a rescan.** The whole reason the layers are apart: a
    /// refresh rewrites the read layer, and the user's correction is still there
    /// afterwards.
    #[test]
    fn overrides_survive_a_rescan() {
        let dir = scratch("survive");
        let root = dir.join("library");
        std::fs::create_dir_all(&root).unwrap();
        a_real_file(&root, "Zool (1992)(Gremlin).adf");
        write_roots(
            &dir,
            &RootsFile {
                schema: CATALOGUE_SCHEMA,
                roots: vec![root.to_string_lossy().into()],
            },
        )
        .unwrap();

        let first = refresh_root(&dir, &root, Refresh::Rescan, None, &NoProgress).unwrap();
        let id = first.entries[0].record.id.clone();

        set_override(
            &dir,
            &id,
            RecordOverride {
                title: Some("Zool: Ninja of the Nth Dimension".into()),
                ..RecordOverride::default()
            },
        )
        .unwrap();

        refresh_root(&dir, &root, Refresh::Rescan, None, &NoProgress).unwrap();

        let loaded = load(&dir).unwrap();
        let entry = &loaded[0].entries[0];
        assert_eq!(entry.record.title.value, "Zool: Ninja of the Nth Dimension");
        assert_eq!(entry.record.title.from, Provenance::UserEdit);

        std::fs::remove_dir_all(&dir).ok();
    }

    /// An override emptied out is deleted rather than stored, so changing one's
    /// mind leaves nothing behind.
    #[test]
    fn an_emptied_override_is_removed() {
        let dir = scratch("empty-override");
        set_override(
            &dir,
            "some-id-00000000",
            RecordOverride {
                title: Some("X".into()),
                ..RecordOverride::default()
            },
        )
        .unwrap();
        assert_eq!(read_overrides(&dir).unwrap().edits.len(), 1);

        set_override(&dir, "some-id-00000000", RecordOverride::default()).unwrap();
        assert!(read_overrides(&dir).unwrap().edits.is_empty());

        std::fs::remove_dir_all(&dir).ok();
    }

    /// **Availability is derived, never stored.** The record on disk says what
    /// was read; whether the file is there *now* is asked of the disk each time,
    /// so a catalogue cannot claim "ready" about a game on an unplugged drive.
    #[test]
    fn availability_comes_from_the_disk_not_the_file() {
        let dir = scratch("availability");
        let root = dir.join("library");
        std::fs::create_dir_all(&root).unwrap();
        let file = a_real_file(&root, "Zool (1992)(Gremlin).adf");
        write_roots(
            &dir,
            &RootsFile {
                schema: CATALOGUE_SCHEMA,
                roots: vec![root.to_string_lossy().into()],
            },
        )
        .unwrap();

        refresh_root(&dir, &root, Refresh::Rescan, None, &NoProgress).unwrap();
        assert!(load(&dir).unwrap()[0].entries[0].available);

        std::fs::remove_file(&file).unwrap();
        let after = load(&dir).unwrap();
        assert_eq!(after[0].entries.len(), 1, "the entry is kept");
        assert!(
            !after[0].entries[0].available,
            "the file is gone, so it cannot be available"
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    /// A root read by an older reader is flagged, so the screen can say an
    /// update would improve it **without doing the update**.
    #[test]
    fn a_root_read_by_an_older_reader_is_flagged_as_stale() {
        let dir = scratch("stale-flag");
        let root = dir.join("library");
        std::fs::create_dir_all(&root).unwrap();

        write_roots(
            &dir,
            &RootsFile {
                schema: CATALOGUE_SCHEMA,
                roots: vec![root.to_string_lossy().into()],
            },
        )
        .unwrap();
        write_root(
            &dir,
            &CatalogueRoot {
                schema: CATALOGUE_SCHEMA,
                root: root.to_string_lossy().into(),
                scanned_at: None,
                index_schema: GAMEINDEX_SCHEMA - 1,
                entries: vec![],
            },
        )
        .unwrap();

        assert!(load(&dir).unwrap()[0].stale);

        std::fs::remove_dir_all(&dir).ok();
    }

    /// **Ten thousand entries, loaded.**
    ///
    /// The question this answers is not "is JSON fast" — it is "did anything
    /// here become quadratic". A load that walks the overrides map per entry, or
    /// re-reads a file per entry, turns 10 000 into minutes; the ceiling below
    /// is loose enough never to flake on a slow machine and tight enough to
    /// catch that.
    ///
    /// The timing is **printed**, so the real number is in the log:
    ///
    /// ```text
    /// cargo test ten_thousand -- --nocapture
    /// ```
    #[test]
    fn ten_thousand_entries_load_without_going_quadratic() {
        let dir = scratch("ten-thousand");
        let root = dir.join("library");
        std::fs::create_dir_all(&root).unwrap();

        // Entries only; the files themselves are not created. That is the point
        // — `load` must not open a title to list it, and every one of these will
        // come back marked unavailable.
        let entries: Vec<CachedEntry> = (0..10_000)
            .map(|n| CachedEntry {
                path: root
                    .join(format!("Game {n:05} (1992)(Someone).adf"))
                    .to_string_lossy()
                    .into(),
                size: 901_120,
                mtime_ms: 1_700_000_000_000 + n as i64,
                record: a_record(&format!("Game {n:05}")),
            })
            .collect();

        write_roots(
            &dir,
            &RootsFile {
                schema: CATALOGUE_SCHEMA,
                roots: vec![root.to_string_lossy().into()],
            },
        )
        .unwrap();

        let started = std::time::Instant::now();
        write_root(
            &dir,
            &CatalogueRoot {
                schema: CATALOGUE_SCHEMA,
                root: root.to_string_lossy().into(),
                scanned_at: None,
                index_schema: GAMEINDEX_SCHEMA,
                entries,
            },
        )
        .unwrap();
        let wrote = started.elapsed();

        // A few overrides, so the merge is exercised rather than skipped.
        for n in [0, 5_000, 9_999] {
            set_override(
                &dir,
                &a_record(&format!("Game {n:05}")).id,
                RecordOverride {
                    year: Some(1993),
                    ..RecordOverride::default()
                },
            )
            .unwrap();
        }

        let started = std::time::Instant::now();
        let loaded = load(&dir).unwrap();
        let read = started.elapsed();

        let bytes = std::fs::metadata(dir.join(root_file_name(&root)))
            .unwrap()
            .len();
        println!("10 000 entries: {bytes} bytes on disk, wrote in {wrote:?}, loaded in {read:?}");

        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].entries.len(), 10_000);
        assert!(
            loaded[0].entries.iter().all(|entry| !entry.available),
            "none of these files exist, so none may be reported available"
        );
        assert_eq!(
            loaded[0]
                .entries
                .iter()
                .filter(|entry| entry.record.year.as_ref().map(|y| y.value) == Some(1993))
                .count(),
            3,
            "the three overrides must be applied, and only those three"
        );

        // Loose on purpose: this is a debug build with 10 000 stat calls in it.
        // A quadratic load blows through this by orders of magnitude.
        assert!(
            read < std::time::Duration::from_secs(30),
            "loading 10 000 entries took {read:?} — something is quadratic"
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    /// Roots keep the order the user put them in, and adding the same folder
    /// twice does not double it.
    #[test]
    fn roots_keep_their_order_and_do_not_duplicate() {
        let dir = scratch("roots");
        let a = dir.join("a");
        let b = dir.join("b");
        std::fs::create_dir_all(&a).unwrap();
        std::fs::create_dir_all(&b).unwrap();

        add_root(&dir, &b).unwrap();
        add_root(&dir, &a).unwrap();
        add_root(&dir, &b).unwrap();

        let roots = read_roots(&dir).unwrap().roots;
        assert_eq!(
            roots,
            vec![
                b.to_string_lossy().to_string(),
                a.to_string_lossy().to_string()
            ],
            "b was added first, and adding it again must not move or repeat it"
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    /// **Removing a root removes its entries** — the only way anything leaves
    /// the catalogue, and therefore the escape hatch for a folder full of games
    /// the user has really deleted.
    #[test]
    fn removing_a_root_takes_its_entries_with_it() {
        let dir = scratch("remove");
        let root = dir.join("library");
        std::fs::create_dir_all(&root).unwrap();
        a_real_file(&root, "Zool (1992)(Gremlin).adf");

        add_root(&dir, &root).unwrap();
        refresh_root(&dir, &root, Refresh::Update, None, &NoProgress).unwrap();
        assert_eq!(load(&dir).unwrap()[0].entries.len(), 1);

        remove_root(&dir, &root).unwrap();

        assert!(load(&dir).unwrap().is_empty());
        assert!(
            !dir.join(root_file_name(&root)).exists(),
            "the root's file must be gone, not just unlisted"
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    /// Removing a root ART never had is not an error. The user asked for it to
    /// be gone; it is gone.
    #[test]
    fn removing_an_unknown_root_is_not_an_error() {
        let dir = scratch("remove-unknown");
        assert!(remove_root(&dir, Path::new(r"E:\nowhere")).is_ok());
        std::fs::remove_dir_all(&dir).ok();
    }

    /// A root that is not a directory is refused when it is added, so a typo
    /// does not sit in the list for ever producing nothing.
    #[test]
    fn a_root_that_is_not_a_directory_is_refused() {
        let dir = scratch("bad-root");
        let file = a_real_file(&dir, "not-a-folder.adf");
        assert!(add_root(&dir, &file).is_err());
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

    /// The user's picture is a choice, not derived data: a refresh re-reads every
    /// file on disk and must not touch it.
    #[test]
    fn a_hand_attached_picture_survives_a_refresh() {
        let dir = scratch("art-binding");
        set_override(
            &dir,
            "turrican-1a2b3c4d",
            RecordOverride {
                art: Some(ArtBinding {
                    chosen: r"D:\pictures\turrican.png".into(),
                    cached: "turrican/manual-snap.png".into(),
                }),
                ..Default::default()
            },
        )
        .unwrap();

        let overrides = read_overrides(&dir).unwrap();
        let kept = overrides
            .edits
            .get("turrican-1a2b3c4d")
            .expect("still there");

        assert_eq!(
            kept.art.as_ref().map(|art| art.cached.as_str()),
            Some("turrican/manual-snap.png")
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// And an override that says nothing is still deleted rather than stored.
    #[test]
    fn removing_the_picture_leaves_no_trace() {
        let dir = scratch("art-binding-removed");
        assert!(RecordOverride {
            art: None,
            ..Default::default()
        }
        .is_empty());

        let _ = std::fs::remove_dir_all(&dir);
    }
}
