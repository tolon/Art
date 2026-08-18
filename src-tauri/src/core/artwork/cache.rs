//! The artwork cache: pictures on disk, keyed by title.
//!
//! Three decisions carried from the design (spec §4) and binding here:
//!
//! 1. **No image ever goes in the catalogue JSON.** A record points at an entry
//!    here; the bytes live beside it as files.
//! 2. **Keyed per *title*, not per record.** The whole Amiga platform is about
//!    3000 titles, so this has a ceiling that does not grow with a collection —
//!    one user's 1700 records contain `1869` five times and one file serves all
//!    five.
//! 3. **Misses are recorded too.** Without them every run re-asks for the same
//!    ~1300 titles nobody has a picture of.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::core::artwork::{ArtKind, ArtRef};
use crate::core::error::{CoreError, CoreResult};
use crate::core::safety::atomic_write;
use crate::core::security::path::safe_join;

/// The index file, a sibling of the picture files it describes.
pub(crate) const INDEX_FILE: &str = "index.json";

/// Bumped when the on-disk shape changes in a way an older ART cannot read.
const CACHE_SCHEMA: u32 = 1;

/// The longest a cache filename's stem may get before it is cut.
const MAX_STEM: usize = 96;

/// Separates the parts of a composite map key. A unit separator cannot appear
/// in a normalised title, a kind or a source id, so no pair of different keys
/// can collide into one.
const KEY_SEPARATOR: char = '\u{1f}';

#[derive(Debug, Serialize, Deserialize)]
struct CacheFile {
    schema: u32,
    /// `"<title key>\u{1f}<kind>"` -> the entry.
    entries: BTreeMap<String, ArtRef>,
    /// `"<title key>\u{1f}<kind>\u{1f}<source>"`, asked for and not found.
    misses: BTreeSet<String>,
}

impl Default for CacheFile {
    fn default() -> Self {
        Self {
            schema: CACHE_SCHEMA,
            entries: BTreeMap::new(),
            misses: BTreeSet::new(),
        }
    }
}

/// The cache, held open across one enrichment run.
#[derive(Debug)]
pub struct Cache {
    dir: PathBuf,
    file: CacheFile,
}

fn entry_key(title_key: &str, kind: ArtKind) -> String {
    format!("{title_key}{KEY_SEPARATOR}{}", kind.as_str())
}

fn miss_key(title_key: &str, kind: ArtKind, source: &str) -> String {
    format!(
        "{title_key}{KEY_SEPARATOR}{}{KEY_SEPARATOR}{source}",
        kind.as_str()
    )
}

impl Cache {
    /// Open a cache directory, creating it when it does not exist.
    ///
    /// A corrupt or unreadable index starts empty rather than failing: it is
    /// derived data, and refusing to open would strand the user with no way to
    /// rebuild it from the UI.
    pub fn open(dir: &Path) -> CoreResult<Self> {
        std::fs::create_dir_all(dir)?;
        let file = std::fs::read(dir.join(INDEX_FILE))
            .ok()
            .and_then(|bytes| serde_json::from_slice::<CacheFile>(&bytes).ok())
            .filter(|parsed| parsed.schema == CACHE_SCHEMA)
            .unwrap_or_default();
        Ok(Self {
            dir: dir.to_path_buf(),
            file,
        })
    }

    /// The directory the cache-relative names in [`ArtRef`] are relative to.
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    pub fn get(&self, title_key: &str, kind: ArtKind) -> Option<&ArtRef> {
        self.file.entries.get(&entry_key(title_key, kind))
    }

    /// The picture a screen should show for this title, or none.
    ///
    /// Walks [`ArtKind::ALL`], which is in preference order, so a title with
    /// both a boxart and an icon shows the boxart.
    pub fn best(&self, title_key: &str) -> Option<&ArtRef> {
        ArtKind::ALL
            .iter()
            .find_map(|kind| self.get(title_key, *kind))
    }

    pub fn is_missing(&self, title_key: &str, kind: ArtKind, source: &str) -> bool {
        self.file
            .misses
            .contains(&miss_key(title_key, kind, source))
    }

    pub fn record_miss(&mut self, title_key: &str, kind: ArtKind, source: &str) {
        self.file.misses.insert(miss_key(title_key, kind, source));
    }

    /// Where a picture for this title would live, cache-relative.
    ///
    /// Pure, and separate from [`Cache::store`] because [`Cache::adopt`] needs
    /// to ask the question without answering it: a file already on disk is one
    /// nobody has to fetch again.
    fn relative_for(title_key: &str, kind: ArtKind, source: &str, ext: &str) -> String {
        let stem: String = title_key
            .chars()
            .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '-' })
            .take(MAX_STEM)
            .collect();
        let stem = stem.trim_matches('-');
        let stem = if stem.is_empty() { "untitled" } else { stem };
        format!("{}/{stem}-{source}.{ext}", kind.as_str())
    }

    /// Take a picture that is already on disk into the index, without fetching.
    ///
    /// This is what makes an interrupted run cheap. The index is the *record*
    /// of what was fetched, not the fetching itself, so a run that wrote 790
    /// files and never got to save leaves 790 perfectly good pictures behind.
    /// Without this they would all be downloaded again — which is exactly what
    /// happened the first time this ran against a real collection.
    ///
    /// Returns `None` when there is no such file, which is the ordinary case.
    pub fn adopt(
        &mut self,
        title_key: &str,
        kind: ArtKind,
        source: &str,
        ext: &str,
    ) -> Option<ArtRef> {
        let relative = Self::relative_for(title_key, kind, source, ext);
        // `safe_join` rather than a plain join even to *ask*: the key is
        // user-influenced wherever it is used.
        let destination = safe_join(&self.dir, &relative).ok()?;
        if !destination.is_file() {
            return None;
        }
        let art = ArtRef {
            kind,
            source: source.to_string(),
            file: relative,
        };
        self.file
            .entries
            .insert(entry_key(title_key, kind), art.clone());
        Some(art)
    }

    /// Write one picture and remember it.
    ///
    /// The filename is derived from the title key, which is user-influenced, so
    /// it is reduced to safe characters and then run through `safe_join` — the
    /// only way an outside-supplied name becomes a path anywhere in ART.
    pub fn store(
        &mut self,
        title_key: &str,
        kind: ArtKind,
        source: &str,
        ext: &str,
        bytes: &[u8],
    ) -> CoreResult<ArtRef> {
        let relative = Self::relative_for(title_key, kind, source, ext);
        let destination = safe_join(&self.dir, &relative).map_err(|err| {
            CoreError::InvalidInput(format!("'{relative}' is not a cache path: {err}"))
        })?;
        if let Some(parent) = destination.parent() {
            std::fs::create_dir_all(parent)?;
        }
        atomic_write(&destination, bytes)?;

        let art = ArtRef {
            kind,
            source: source.to_string(),
            file: relative,
        };
        self.file
            .entries
            .insert(entry_key(title_key, kind), art.clone());
        self.file.misses.remove(&miss_key(title_key, kind, source));
        Ok(art)
    }

    /// Forget one picture: drop it from the index and delete its file.
    ///
    /// The mirror of [`Cache::store`], for undoing a hand-attached picture.
    /// A no-op when there is nothing to remove, on either side — an entry
    /// with no file on disk still loses its index row, and a file that is
    /// already gone is not an error.
    pub fn remove(&mut self, title_key: &str, kind: ArtKind) -> CoreResult<()> {
        let Some(art) = self.file.entries.remove(&entry_key(title_key, kind)) else {
            return Ok(());
        };
        let destination = safe_join(&self.dir, &art.file).map_err(|err| {
            CoreError::InvalidInput(format!("'{}' is not a cache path: {err}", art.file))
        })?;
        match std::fs::remove_file(&destination) {
            Ok(()) => Ok(()),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(err) => Err(err.into()),
        }
    }

    /// Persist the index. Derived data, so atomic but unbacked.
    pub fn save(&self) -> CoreResult<()> {
        let bytes = serde_json::to_vec(&self.file)
            .map_err(|err| CoreError::InvalidInput(format!("cannot serialise the cache: {err}")))?;
        atomic_write(&self.dir.join(INDEX_FILE), &bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A directory of this test's own. The name is passed in rather than taken
    /// from the thread, because the harness reuses threads and two tests
    /// sharing a directory fail in a way that looks like a cache bug.
    fn tempdir(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("art-artwork-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn a_stored_image_comes_back_and_its_bytes_are_on_disk() {
        let dir = tempdir("stored");
        let mut cache = Cache::open(&dir).unwrap();
        let art = cache
            .store(
                "turrican ii",
                ArtKind::Boxart,
                "libretro",
                "png",
                b"PNGDATA",
            )
            .unwrap();

        assert_eq!(art.kind, ArtKind::Boxart);
        assert_eq!(art.source, "libretro");
        assert_eq!(cache.get("turrican ii", ArtKind::Boxart), Some(&art));

        let on_disk = std::fs::read(dir.join(&art.file)).unwrap();
        assert_eq!(on_disk, b"PNGDATA");
    }

    /// The point of the cache: a second run asks for nothing it already knows.
    #[test]
    fn a_saved_cache_reloads_with_its_entries_and_misses() {
        let dir = tempdir("reload");
        {
            let mut cache = Cache::open(&dir).unwrap();
            cache
                .store("turrican ii", ArtKind::Boxart, "libretro", "png", b"X")
                .unwrap();
            cache.record_miss("no such game", ArtKind::Boxart, "libretro");
            cache.save().unwrap();
        }

        let reloaded = Cache::open(&dir).unwrap();
        assert!(reloaded.get("turrican ii", ArtKind::Boxart).is_some());
        assert!(reloaded.is_missing("no such game", ArtKind::Boxart, "libretro"));
    }

    /// A miss is recorded against one source. Another source has not been asked
    /// and must still be tried.
    #[test]
    fn a_miss_is_per_source_not_per_title() {
        let dir = tempdir("miss-per-source");
        let mut cache = Cache::open(&dir).unwrap();
        cache.record_miss("moonstone", ArtKind::Icon, "libretro");
        assert!(cache.is_missing("moonstone", ArtKind::Icon, "libretro"));
        assert!(!cache.is_missing("moonstone", ArtKind::Icon, "whdload-de"));
    }

    /// A title is a user-influenced string. It must never escape the cache
    /// directory, whatever it contains.
    #[test]
    fn a_traversing_title_cannot_write_outside_the_cache() {
        let dir = tempdir("traversal");
        let mut cache = Cache::open(&dir).unwrap();
        let art = cache
            .store("../../evil", ArtKind::Boxart, "libretro", "png", b"X")
            .unwrap();

        assert!(
            !art.file.contains(".."),
            "{} kept a relative step",
            art.file
        );
        let written = dir.join(&art.file).canonicalize().unwrap();
        assert!(
            written.starts_with(dir.canonicalize().unwrap()),
            "{written:?} escaped the cache directory"
        );
    }

    /// A title made entirely of characters the filename cannot hold still has
    /// to land somewhere rather than producing an empty name.
    #[test]
    fn a_title_with_nothing_usable_in_it_still_gets_a_file() {
        let dir = tempdir("unusable");
        let mut cache = Cache::open(&dir).unwrap();
        let art = cache
            .store("///", ArtKind::Boxart, "libretro", "png", b"X")
            .unwrap();
        assert!(dir.join(&art.file).exists());
    }

    /// The bug this exists for, in its exact shape: a run wrote 790 pictures
    /// and was interrupted before the index was saved. The files are perfectly
    /// good. Without adoption every one of them is fetched again.
    #[test]
    fn a_picture_already_on_disk_is_adopted_rather_than_refetched() {
        let dir = tempdir("adopt");
        {
            let mut first = Cache::open(&dir).unwrap();
            first
                .store("turrican ii", ArtKind::Boxart, "libretro", "png", b"PNG")
                .unwrap();
            // Deliberately not saved — this is the interrupted run.
        }

        let mut second = Cache::open(&dir).unwrap();
        assert!(
            second.get("turrican ii", ArtKind::Boxart).is_none(),
            "the index really was lost, or this test proves nothing"
        );

        let adopted = second
            .adopt("turrican ii", ArtKind::Boxart, "libretro", "png")
            .expect("the file is on disk and must be adopted");
        assert_eq!(second.get("turrican ii", ArtKind::Boxart), Some(&adopted));
        assert_eq!(
            std::fs::read(dir.join(&adopted.file)).unwrap(),
            b"PNG",
            "adoption must not touch the bytes"
        );
    }

    /// Adoption answers only about files that exist. Everything else is a
    /// fetch waiting to happen, and saying otherwise would put an entry in the
    /// index pointing at nothing.
    #[test]
    fn adopt_finds_nothing_when_there_is_no_file() {
        let dir = tempdir("adopt-empty");
        let mut cache = Cache::open(&dir).unwrap();
        assert!(cache
            .adopt("never fetched", ArtKind::Boxart, "libretro", "png")
            .is_none());
        assert!(cache.get("never fetched", ArtKind::Boxart).is_none());
    }

    /// Derived data. Refusing to open would strand the user with no way to
    /// rebuild it from the UI.
    #[test]
    fn opening_a_directory_with_a_corrupt_index_starts_empty_rather_than_failing() {
        let dir = tempdir("corrupt");
        std::fs::write(dir.join(INDEX_FILE), b"{ not json").unwrap();
        let cache = Cache::open(&dir).unwrap();
        assert!(cache.get("anything", ArtKind::Boxart).is_none());
    }

    /// A title with more than one picture shows the one earliest in
    /// `ArtKind::ALL` — a cover beats an icon, whatever order they arrived in.
    #[test]
    fn best_prefers_the_kind_a_screen_should_show_first() {
        let dir = tempdir("best");
        let mut cache = Cache::open(&dir).unwrap();
        cache
            .store("moonstone", ArtKind::Icon, "whdload-de", "png", b"I")
            .unwrap();
        cache
            .store("moonstone", ArtKind::Boxart, "libretro", "png", b"B")
            .unwrap();

        assert_eq!(cache.best("moonstone").unwrap().kind, ArtKind::Boxart);
        assert!(cache.best("nothing here").is_none());
    }

    /// Two titles that differ only in case are one title, so they must share a
    /// cache entry rather than fetching twice.
    #[test]
    fn the_key_is_taken_as_given_so_the_caller_normalises_once() {
        let dir = tempdir("key-as-given");
        let mut cache = Cache::open(&dir).unwrap();
        cache
            .store("turrican ii", ArtKind::Boxart, "libretro", "png", b"X")
            .unwrap();
        assert!(cache.get("turrican ii", ArtKind::Boxart).is_some());
        assert!(cache.get("Turrican II", ArtKind::Boxart).is_none());
    }

    /// The mirror of `store`: the index entry and the file both go, and a
    /// second removal of the same title is quiet rather than an error.
    #[test]
    fn a_removed_picture_leaves_neither_an_entry_nor_a_file() {
        let dir = tempdir("removed");
        let mut cache = Cache::open(&dir).unwrap();
        let art = cache
            .store("moonstone", ArtKind::Boxart, "manual", "png", b"X")
            .unwrap();
        let file_path = dir.join(&art.file);
        assert!(file_path.exists());

        cache.remove("moonstone", ArtKind::Boxart).unwrap();

        assert!(cache.get("moonstone", ArtKind::Boxart).is_none());
        assert!(!file_path.exists());
        // Nothing left to remove, and that must not be an error.
        cache.remove("moonstone", ArtKind::Boxart).unwrap();
    }

    /// Removing a title nothing was ever stored for is a no-op, not an error.
    #[test]
    fn removing_a_title_with_nothing_stored_is_quiet() {
        let dir = tempdir("remove-nothing");
        let mut cache = Cache::open(&dir).unwrap();
        cache.remove("never stored", ArtKind::Boxart).unwrap();
    }
}
