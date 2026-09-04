//! Remembering what a medium held, so the OS Builder does not walk a
//! 468 MB disc again for a preview that changed nothing on it.
//!
//! **ART-188.** `plan()` opens every on component's media through
//! [`scan::open_media`](super::scan::open_media), and opening an optical
//! disc walks its whole directory tree ([`super::source_cd::CdSource`]'s own
//! "walked once, at `open`"). The AmigaOS 3.9 recipe puts three components
//! on one disc, so one preview walked the owner's `AmigaOS39.iso` three
//! times, and every checkbox toggle did it again. The walk is thousands of
//! tiny reads — Amiga files are small and numerous — and the wait is
//! visible.
//!
//! ## Identity is three constant-time reads, never a hash
//!
//! `(path, size, mtime)`, exactly the key
//! `commands::osinstall::preview_cache_key` already uses for archive
//! extraction. The owner's first suggestion was a content hash; hashing
//! 468 MB costs about what the walk costs, so it would trade the problem
//! for itself. A hash answers "are these two files the same"; this cache
//! asks "is this the same file I read last time", which is a different
//! question and a much cheaper one.
//!
//! ## …and three constant-time reads cannot see everything, so there is a rescan
//!
//! **A restored backup can preserve its timestamps.** Several AmigaOS 3.9
//! ISOs circulate and people keep their own copies, so "same path, same
//! size, same mtime, different disc" is a real arrangement rather than a
//! theoretical one. A cache keyed this way would then answer with complete
//! confidence and be wrong — this project's most expensive failure shape,
//! because it does not crash, it tells the user something untrue (§89).
//!
//! So [`ScanCache::forget_all`] exists and the OS Builder puts a button on
//! it. The rescan is not a convenience: it is the escape hatch for the one
//! case the cheap key cannot see, and it is what makes the cheap key safe
//! to trust everywhere else.
//!
//! ## A cache that cannot be trusted is discarded, never partly believed
//!
//! Every path out of [`ScanCache::lookup`] that is not "this file parsed,
//! carried this exact schema, and recorded exactly the identity I just
//! stat-ed" is a **miss** — never a partial answer, never a repair. That is
//! `src/lib/remembered.ts`'s guarded read (`recall`/`recallInto`) applied on
//! the Rust side: a stale, truncated, hand-edited or half-written file
//! costs one walk, and can never cost a wrong install plan. The read is
//! bounded too ([`MAX_CACHE_FILE_BYTES`]) so a cache file that grew wrong
//! cannot be allocated from.
//!
//! ## Where it lives, and how it stops growing
//!
//! Beside the extraction cache, under the caller's chosen directory
//! (`%TEMP%` in the app — the directory is a *parameter*, because
//! `core::osinstall` does not get to decide where a platform keeps scratch
//! files; `core::artwork::cache` takes its directory the same way and for
//! the same reason).
//!
//! One file per medium **path**: the filename hashes the path alone, and the
//! identity that decides staleness lives *inside* the file. That is a
//! deliberate difference from `preview_cache_dir`, which hashes the mtime
//! and length into the directory name and so leaves an orphan behind every
//! time an archive changes, relying on its hourly sweep to catch up. Here a
//! medium that changed **overwrites its own entry**, so repeated use of one
//! disc cannot grow the cache at all. [`ScanCache::sweep`] then reaps
//! whole entries by age — a medium that has gone away, or one the user
//! never opens again — and that is the only growth left.
//!
//! ## What is cached, and what is deliberately not
//!
//! The **listing**: every [`MediaEntry`] the medium's own
//! `walk("")` gave, plus the root entry `entry("")` gave. Not one byte of
//! file content — [`CachedSource::read`] opens the real medium (once,
//! lazily) and reads through it, so the bytes an install writes always come
//! off the medium itself and never out of `%TEMP%`.

use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::core::error::{CoreError, CoreResult};
use crate::core::safety::atomic_write;

use super::amiga_names_equal;
use super::scan::MediaKind;
use super::source::{starts_with_ignoring_case, MediaEntry, MediaSource};

/// Bumped whenever the on-disk shape changes — including a change to
/// [`MediaEntry`]'s own fields, which are serialised into it verbatim. An
/// entry written by a different schema is a miss, never a partial read.
const SCAN_CACHE_SCHEMA: u32 = 1;

/// Every file this module writes starts with this, so [`ScanCache::sweep`]
/// and [`ScanCache::forget_all`] can find them — and only them — inside a
/// `%TEMP%` shared with the extraction cache and with everything else on
/// the machine.
const CACHE_FILE_PREFIX: &str = "art-osinstall-scan-";
const CACHE_FILE_SUFFIX: &str = ".json";

/// How long an untouched entry survives a [`ScanCache::sweep`].
///
/// Long, on purpose: this is not the extraction cache's hour. A user
/// previews the same install disc across days, and the whole point is that
/// they do not wait again. What the age bound is actually for is the entry
/// whose medium has gone away — an unplugged drive, a deleted ISO — which
/// nothing will ever look up again and which would otherwise sit in
/// `%TEMP%` forever.
const CACHE_MAX_AGE: Duration = Duration::from_secs(30 * 24 * 60 * 60);

/// A cache file bigger than this is refused unread. The largest listing ART
/// will ever write is [`crate::core::iso::MAX_WALK_ENTRIES`] entries
/// (100 000) — the owner's 468 MB disc holds 8 584 — so this is far above
/// anything legitimate and exists only so a corrupt or hostile file cannot
/// be allocated from (CLAUDE.md: bound reads, never allocate from an
/// unchecked length).
const MAX_CACHE_FILE_BYTES: u64 = 256 * 1024 * 1024;

/// What makes one medium the same medium as last time.
///
/// `mtime_nanos` is `0` when the platform will not give a modification time
/// at all, matching `preview_cache_key`'s own fallback — a medium on such a
/// filesystem is then keyed on path and size alone, which is weaker and is
/// exactly what the rescan is there for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MediaIdentity {
    pub size: u64,
    pub mtime_nanos: u64,
}

/// `(size, mtime)` for `path`, or `None` when it cannot be stat-ed at all.
///
/// `None` is not an error here: a medium that vanished between the scan
/// listing it and this call is an ordinary thing on a filesystem someone is
/// using, and the only correct consequence is a cache miss.
pub fn identity_of(path: &Path) -> Option<MediaIdentity> {
    let metadata = std::fs::metadata(path).ok()?;
    let mtime_nanos = metadata
        .modified()
        .ok()
        .and_then(|m| m.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0);
    Some(MediaIdentity {
        size: metadata.len(),
        mtime_nanos,
    })
}

/// One medium's listing, exactly as it was read off the medium.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct CacheFile {
    schema: u32,
    /// The medium's path as ART saw it. Recorded so a hash collision
    /// between two different paths can never be served as a hit — the
    /// filename is a 64-bit hash and collisions are rare, not impossible.
    path: String,
    size: u64,
    mtime_nanos: u64,
    volume_name: String,
    kind: MediaKind,
    /// What `entry("")` answered — every implementation builds its own
    /// root entry and they do not all build the same one, so it is stored
    /// rather than synthesised here.
    root: MediaEntry,
    /// What `walk("")` answered: the whole medium, the medium's own casing,
    /// root itself excluded (every implementation's rule).
    entries: Vec<MediaEntry>,
}

/// A listing recovered from the cache, ready to be wrapped in a
/// [`CachedSource`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CachedListing {
    pub volume_name: String,
    pub kind: MediaKind,
    pub root: MediaEntry,
    pub entries: Vec<MediaEntry>,
}

/// Where cached listings live — or that there are none.
///
/// [`ScanCache::Off`] is a real member rather than an `Option<ScanCache>` at
/// every call site: turning the cache off must skip **storing** as well as
/// looking up, and a type that can say "off" is what makes forgetting one
/// of the two impossible.
#[derive(Debug, Clone)]
pub enum ScanCache {
    /// The user turned caching off, or the caller has nowhere to put it.
    /// Every `lookup` misses and every `store` does nothing — a cache the
    /// user switched off must not keep quietly writing files it will never
    /// read.
    Off,
    /// Cached in this directory, created on first write.
    In(PathBuf),
}

impl ScanCache {
    pub fn off() -> Self {
        Self::Off
    }

    pub fn in_dir(dir: impl Into<PathBuf>) -> Self {
        Self::In(dir.into())
    }

    pub fn is_on(&self) -> bool {
        matches!(self, Self::In(_))
    }

    fn dir(&self) -> Option<&Path> {
        match self {
            Self::Off => None,
            Self::In(dir) => Some(dir.as_path()),
        }
    }

    /// The one file a given medium path may ever occupy.
    ///
    /// Deterministic, and a hash of the **path only** — see the module doc
    /// for why the identity is inside the file instead of in its name.
    fn file_for(&self, media_path: &Path) -> Option<PathBuf> {
        let dir = self.dir()?;
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        media_path.hash(&mut hasher);
        Some(dir.join(format!(
            "{CACHE_FILE_PREFIX}{:016x}{CACHE_FILE_SUFFIX}",
            hasher.finish()
        )))
    }

    /// The listing recorded for `media_path`, **only** if it was recorded
    /// for exactly the file that is there now.
    ///
    /// Every other outcome — no file, unreadable, oversized, not JSON, a
    /// different schema, a different recorded path, a different size, a
    /// different mtime — is `None`. Nothing here repairs, upgrades or
    /// partly believes a cache file; see the module doc.
    pub fn lookup(&self, media_path: &Path) -> Option<CachedListing> {
        let file = self.file_for(media_path)?;
        let identity = identity_of(media_path)?;

        // Bounded: refuse to read, let alone parse, something far larger
        // than any listing ART writes.
        let metadata = std::fs::metadata(&file).ok()?;
        if !metadata.is_file() || metadata.len() > MAX_CACHE_FILE_BYTES {
            return None;
        }
        let bytes = std::fs::read(&file).ok()?;
        let parsed: CacheFile = serde_json::from_slice(&bytes).ok()?;

        if parsed.schema != SCAN_CACHE_SCHEMA
            || parsed.path != media_path.to_string_lossy()
            || parsed.size != identity.size
            || parsed.mtime_nanos != identity.mtime_nanos
        {
            return None;
        }

        Some(CachedListing {
            volume_name: parsed.volume_name,
            kind: parsed.kind,
            root: parsed.root,
            entries: parsed.entries,
        })
    }

    /// Record `listing` as what `media_path` held, keyed on the identity it
    /// has **now**.
    ///
    /// Best-effort and infallible by design: this is derived data, and a
    /// `%TEMP%` that is full or read-only must cost a walk, never fail the
    /// preview the walk was for. `atomic_write`, never `std::fs::write` —
    /// a half-written listing that still parsed would be a silently short
    /// install plan, and ART's own rule is that nothing is left truncated.
    pub fn store(&self, media_path: &Path, listing: &CachedListing) {
        let Some(file) = self.file_for(media_path) else {
            return;
        };
        let Some(identity) = identity_of(media_path) else {
            return;
        };
        let Some(dir) = self.dir() else {
            return;
        };
        if std::fs::create_dir_all(dir).is_err() {
            return;
        }
        let payload = CacheFile {
            schema: SCAN_CACHE_SCHEMA,
            path: media_path.to_string_lossy().into_owned(),
            size: identity.size,
            mtime_nanos: identity.mtime_nanos,
            volume_name: listing.volume_name.clone(),
            kind: listing.kind,
            root: listing.root.clone(),
            entries: listing.entries.clone(),
        };
        let Ok(bytes) = serde_json::to_vec(&payload) else {
            return;
        };
        let _ = atomic_write(&file, &bytes);
    }

    /// Drop everything this cache remembers, and say how many entries went.
    ///
    /// **This is what the OS Builder's rescan button does**, and it drops
    /// the whole cache rather than one folder's media on purpose. The case
    /// the rescan exists for is a medium whose `(path, size, mtime)` still
    /// match while its contents do not; that medium is, by definition, one
    /// this cache cannot pick out from the others. Working out which files
    /// to keep would mean parsing every entry to read the path back out of
    /// it — more work than the thing being avoided, and one more place for
    /// "the entry that mattered was the one that was kept" to happen. A
    /// listing dropped needlessly costs exactly one walk of a medium the
    /// user is actually looking at.
    ///
    /// Only this module's own files, matched by prefix and suffix, are ever
    /// removed: `%TEMP%` belongs to everybody.
    pub fn forget_all(&self) -> usize {
        let Some(dir) = self.dir() else {
            return 0;
        };
        let Ok(entries) = std::fs::read_dir(dir) else {
            return 0;
        };
        let mut dropped = 0usize;
        for entry in entries.flatten() {
            if !is_cache_file_name(&entry.file_name().to_string_lossy()) {
                continue;
            }
            if std::fs::remove_file(entry.path()).is_ok() {
                dropped += 1;
            }
        }
        dropped
    }

    /// Best-effort: remove entries untouched for longer than
    /// [`CACHE_MAX_AGE`]. Never fails the call it runs inside — an entry
    /// this pass misses is swept next time, and an entry swept out from
    /// under a lookup is a miss, never a wrong answer.
    pub fn sweep(&self) {
        let Some(dir) = self.dir() else {
            return;
        };
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        let now = SystemTime::now();
        for entry in entries.flatten() {
            if !is_cache_file_name(&entry.file_name().to_string_lossy()) {
                continue;
            }
            let Ok(metadata) = entry.metadata() else {
                continue;
            };
            if !metadata.is_file() {
                continue;
            }
            let Ok(modified) = metadata.modified() else {
                continue;
            };
            let Ok(age) = now.duration_since(modified) else {
                continue;
            };
            if age > CACHE_MAX_AGE {
                let _ = std::fs::remove_file(entry.path());
            }
        }
    }
}

fn is_cache_file_name(name: &str) -> bool {
    name.starts_with(CACHE_FILE_PREFIX) && name.ends_with(CACHE_FILE_SUFFIX)
}

/// Read a medium's whole listing out of an open [`MediaSource`], in the
/// shape [`ScanCache::store`] keeps.
///
/// Cheap for both implementations that matter: `CdSource` and `AdfSource`
/// have already read what they need by the time `open` returns, so this is
/// a walk of an in-memory snapshot (2 ms for the owner's 8 584-entry disc),
/// not a second read of the medium.
pub fn listing_of(source: &mut dyn MediaSource, kind: MediaKind) -> CoreResult<CachedListing> {
    let root = source
        .entry("")?
        .ok_or_else(|| CoreError::InvalidInput("this media has no root entry".into()))?;
    let entries = source.walk("")?;
    Ok(CachedListing {
        volume_name: source.volume_name().to_string(),
        kind,
        root,
        entries,
    })
}

/// A [`MediaSource`] that answers every question about *what is on* the
/// medium from a cached listing, and every question about *bytes* from the
/// medium itself.
///
/// The listing half is a re-implementation of the same three answers
/// `CdSource` and `AdfSource` give, so it is registered as a further
/// implementation in `source_contract.rs` — the file that exists precisely
/// because "two implementations of one trait answering one question two
/// ways" has already happened five times in this module tree. A sixth
/// divergence, introduced by a *cache*, would be the worst of them: it
/// would show up only on the second preview.
///
/// `read` never comes out of the cache. `reopen` is called at most once,
/// lazily, so a plan that only walks (which is every preview) never opens
/// the medium at all, and an `apply` that reads bytes gets them off the
/// real disc.
pub struct CachedSource {
    listing: CachedListing,
    /// How to get at the medium itself, called at most once and only for a
    /// `read`. Not `Send`: `MediaSource` is not a `Send` trait, so
    /// `CachedSource` could not be one whatever this closure promised — the
    /// bound would be a claim with nothing behind it. A job thread builds its
    /// own source inside the thread, as every caller here already does.
    reopen: Box<dyn FnMut() -> CoreResult<Box<dyn MediaSource>>>,
    inner: Option<Box<dyn MediaSource>>,
}

impl std::fmt::Debug for CachedSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CachedSource")
            .field("volume_name", &self.listing.volume_name)
            .field("entries", &self.listing.entries.len())
            .field("opened", &self.inner.is_some())
            .finish()
    }
}

impl CachedSource {
    pub fn new(
        listing: CachedListing,
        reopen: impl FnMut() -> CoreResult<Box<dyn MediaSource>> + 'static,
    ) -> Self {
        Self {
            listing,
            reopen: Box::new(reopen),
            inner: None,
        }
    }

    pub fn kind(&self) -> MediaKind {
        self.listing.kind
    }

    /// `CdSource::normalized`, word for word — a path is compared by its
    /// non-empty `/`-separated segments, so `"C/"`, `"/C"` and `"C"` are one
    /// path.
    fn normalized(path: &str) -> String {
        path.split('/')
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join("/")
    }

    /// Exact match first, folded match second — `CdSource::find_by_path`'s
    /// rule and its reason: AmigaDOS is case-insensitive (ART-012), but a
    /// medium may legitimately hold two names differing only in case and a
    /// query spelling one of them exactly must never be answered with the
    /// other.
    fn find(&self, normalized: &str) -> Option<&MediaEntry> {
        self.listing
            .entries
            .iter()
            .find(|e| e.path == normalized)
            .or_else(|| {
                self.listing
                    .entries
                    .iter()
                    .find(|e| amiga_names_equal(&e.path, normalized))
            })
    }

    fn opened(&mut self) -> CoreResult<&mut Box<dyn MediaSource>> {
        if self.inner.is_none() {
            self.inner = Some((self.reopen)()?);
        }
        Ok(self.inner.as_mut().expect("just assigned when it was None"))
    }
}

impl MediaSource for CachedSource {
    fn volume_name(&self) -> &str {
        &self.listing.volume_name
    }

    fn entry(&mut self, path: &str) -> CoreResult<Option<MediaEntry>> {
        let normalized = Self::normalized(path);
        if normalized.is_empty() {
            return Ok(Some(self.listing.root.clone()));
        }
        Ok(self.find(&normalized).cloned())
    }

    fn walk(&mut self, path: &str) -> CoreResult<Vec<MediaEntry>> {
        let normalized = Self::normalized(path);
        if normalized.is_empty() {
            return Ok(self.listing.entries.clone());
        }
        let Some(found) = self.find(&normalized) else {
            return Ok(Vec::new());
        };
        if !found.is_dir {
            return Err(CoreError::InvalidInput(format!(
                "'{path}' is a file on this media, not a drawer"
            )));
        }
        // The prefix is the medium's own spelling, and the test is folded —
        // both rules are the trait's, argued out in `CdSource::walk` and
        // pinned in `source_contract.rs`.
        let prefix = format!("{}/", found.path);
        Ok(self
            .listing
            .entries
            .iter()
            .filter(|e| starts_with_ignoring_case(&e.path, &prefix))
            .cloned()
            .collect())
    }

    fn read(&mut self, path: &str) -> CoreResult<Vec<u8>> {
        // Never from the cache: the cache holds a listing, not bytes.
        let owned = path.to_string();
        self.opened()?.read(&owned)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::osinstall::fixtures;
    use crate::core::osinstall::scan::{find_media, identify, open_media, open_media_cached};
    use crate::core::ScratchDir;

    /// A one-file ADF plus somewhere to keep a cache, and the `FoundMedia`
    /// for it.
    fn media_and_cache(tag: &str) -> (ScratchDir, PathBuf, ScanCache) {
        let dir = ScratchDir::new("art-scan-cache", tag);
        let image = fixtures::media(
            dir.path(),
            "Workbench3.2",
            "wb.adf",
            &[("C/LoadModule", b"cmd", 0x20)],
        );
        let cache_dir = dir.join("cache");
        std::fs::create_dir_all(&cache_dir).unwrap();
        (dir, image, ScanCache::in_dir(cache_dir))
    }

    fn listing_for(path: &Path) -> CachedListing {
        let found = identify(path).expect("the fixture is media");
        let mut source = open_media(&found).unwrap();
        listing_of(source.as_mut(), found.kind).unwrap()
    }

    /// The plain round trip: what went in comes back out.
    #[test]
    fn a_stored_listing_comes_back_unchanged() {
        let (_dir, image, cache) = media_and_cache("round-trip");
        let listing = listing_for(&image);
        cache.store(&image, &listing);
        assert_eq!(cache.lookup(&image).unwrap(), listing);
    }

    /// **The staleness guard, and the fixture is built so it cannot pass
    /// against a key that ignores `mtime`.** The second image is written
    /// with *different content at exactly the same length* — a different
    /// file with an identical size, which is precisely the shape a key
    /// comparing size alone would wave through.
    #[test]
    fn a_medium_of_the_same_size_with_a_later_mtime_is_a_miss() {
        let (dir, image, cache) = media_and_cache("mtime");
        let listing = listing_for(&image);
        cache.store(&image, &listing);
        assert!(cache.lookup(&image).is_some(), "stored, so it must hit");

        // Same byte length, different bytes, and a modification time the
        // filesystem is guaranteed to record as later.
        let replacement = fixtures::media(
            dir.path(),
            "Workbench3.2",
            "wb2.adf",
            &[("C/Version", b"ver", 0x20)],
        );
        let before = std::fs::metadata(&image).unwrap().len();
        let bytes = std::fs::read(&replacement).unwrap();
        assert_eq!(
            bytes.len() as u64,
            before,
            "both fixtures are DD floppies, so the size cannot be what distinguishes them"
        );
        let stamp = std::fs::metadata(&image)
            .unwrap()
            .modified()
            .unwrap()
            .checked_add(Duration::from_secs(60))
            .unwrap();
        std::fs::write(&image, &bytes).unwrap();
        filetime_set(&image, stamp);

        assert_eq!(
            identity_of(&image).unwrap().size,
            before,
            "the size is unchanged — only the mtime and the contents moved"
        );
        assert!(
            cache.lookup(&image).is_none(),
            "a different disc at the same path and the same size must miss"
        );
    }

    /// The size half of the key, on its own.
    #[test]
    fn a_medium_that_changed_size_is_a_miss() {
        let (_dir, image, cache) = media_and_cache("size");
        cache.store(&image, &listing_for(&image));
        let mut bytes = std::fs::read(&image).unwrap();
        let stamp = std::fs::metadata(&image).unwrap().modified().unwrap();
        bytes.extend_from_slice(&[0u8; 512]);
        std::fs::write(&image, &bytes).unwrap();
        // Put the mtime *back*, so size is the only thing that moved and a
        // key that only compared mtime would wrongly hit.
        filetime_set(&image, stamp);
        assert!(cache.lookup(&image).is_none());
    }

    /// A cache file that is not what this ART writes is a miss — never a
    /// partial read, never a repair.
    #[test]
    fn an_unusable_cache_file_is_a_miss_not_an_answer() {
        let (_dir, image, cache) = media_and_cache("guarded");
        let listing = listing_for(&image);
        let file = cache.file_for(&image).unwrap();

        let good = std::fs::read(&file).map(|_| ()).err().is_some();
        assert!(good, "nothing is stored yet");

        cache.store(&image, &listing);
        let stored = std::fs::read(&file).unwrap();

        // 1. Not JSON at all.
        std::fs::write(&file, b"{ this is not json").unwrap();
        assert!(cache.lookup(&image).is_none(), "unparseable");

        // 2. Truncated half-way — parses as nothing, must not parse as
        //    "some of the entries".
        std::fs::write(&file, &stored[..stored.len() / 2]).unwrap();
        assert!(cache.lookup(&image).is_none(), "truncated");

        // 3. A schema this ART does not write.
        let mut value: serde_json::Value = serde_json::from_slice(&stored).unwrap();
        value["schema"] = serde_json::json!(SCAN_CACHE_SCHEMA + 1);
        std::fs::write(&file, serde_json::to_vec(&value).unwrap()).unwrap();
        assert!(cache.lookup(&image).is_none(), "another schema");

        // 4. A file recorded for a *different* medium that happens to land
        //    in this slot — a hash collision, served as a hit, would be an
        //    install plan built from the wrong disc.
        let mut value: serde_json::Value = serde_json::from_slice(&stored).unwrap();
        value["path"] = serde_json::json!("Z:\\somewhere\\else.iso");
        std::fs::write(&file, serde_json::to_vec(&value).unwrap()).unwrap();
        assert!(cache.lookup(&image).is_none(), "a different recorded path");

        // 5. Sound again, to prove the four above failed for their own
        //    reasons and not because the fixture stopped working.
        std::fs::write(&file, &stored).unwrap();
        assert!(cache.lookup(&image).is_some(), "the original still hits");
    }

    /// An entry bigger than any listing ART writes is refused unread.
    #[test]
    fn an_oversized_cache_file_is_refused_without_being_read() {
        let (_dir, image, cache) = media_and_cache("oversized");
        cache.store(&image, &listing_for(&image));
        let file = cache.file_for(&image).unwrap();
        // Sparse where the filesystem allows it; the point is the length
        // field, which is what `lookup` checks before reading.
        let handle = std::fs::OpenOptions::new().write(true).open(&file).unwrap();
        handle.set_len(MAX_CACHE_FILE_BYTES + 1).unwrap();
        drop(handle);
        assert!(cache.lookup(&image).is_none());
    }

    /// The rescan, and it has to actually remove the file — a rescan that
    /// left the entry in place would serve the stale answer again on the
    /// very next preview, which is the failure it exists to prevent.
    #[test]
    fn forgetting_removes_the_entry_from_disk_and_the_next_lookup_misses() {
        let (_dir, image, cache) = media_and_cache("forget");
        cache.store(&image, &listing_for(&image));
        let file = cache.file_for(&image).unwrap();
        assert!(file.exists(), "stored");
        assert!(cache.lookup(&image).is_some(), "and hits");

        assert_eq!(cache.forget_all(), 1, "one entry dropped");
        assert!(
            !file.exists(),
            "the entry is gone from disk, not just from a map"
        );
        assert!(cache.lookup(&image).is_none(), "and the next lookup misses");
    }

    /// `forget_all` sweeps this module's files and nothing else — `%TEMP%`
    /// is shared with the extraction cache and with the rest of the machine.
    #[test]
    fn forgetting_touches_only_this_modules_own_files() {
        let (dir, image, cache) = media_and_cache("forget-scope");
        cache.store(&image, &listing_for(&image));
        let ScanCache::In(cache_dir) = &cache else {
            unreachable!()
        };
        let bystander = cache_dir.join("art-osinstall-collisions-deadbeef");
        std::fs::write(&bystander, b"someone else's").unwrap();
        let unrelated = cache_dir.join("notes.txt");
        std::fs::write(&unrelated, b"the user's").unwrap();

        assert_eq!(cache.forget_all(), 1);
        assert!(
            bystander.exists(),
            "the extraction cache is not ours to delete"
        );
        assert!(unrelated.exists(), "and neither is anything else");
        let _ = dir;
    }

    /// Off means off in **both** directions: nothing is read and nothing is
    /// written. A cache the user switched off that kept filling `%TEMP%`
    /// would be a control that silently ignores them.
    #[test]
    fn a_cache_that_is_off_neither_reads_nor_writes() {
        let (dir, image, _cache) = media_and_cache("off");
        let off = ScanCache::off();
        let listing = listing_for(&image);
        off.store(&image, &listing);
        assert!(off.lookup(&image).is_none());
        assert!(!off.is_on());
        // Nothing appeared anywhere.
        let strays: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .flatten()
            .filter(|e| is_cache_file_name(&e.file_name().to_string_lossy()))
            .collect();
        assert!(strays.is_empty(), "an off cache wrote {strays:?}");
    }

    /// A medium that has gone away can never produce a hit, whatever is
    /// still sitting in the cache directory for it.
    #[test]
    fn a_medium_that_vanished_is_a_miss_not_a_stale_hit() {
        let (_dir, image, cache) = media_and_cache("vanished");
        cache.store(&image, &listing_for(&image));
        assert!(cache.lookup(&image).is_some());
        std::fs::remove_file(&image).unwrap();
        assert!(
            cache.file_for(&image).unwrap().exists(),
            "the entry is still there — this is about `lookup`, not about tidying"
        );
        assert!(cache.lookup(&image).is_none());
    }

    /// The sweep reaps by age and leaves a fresh entry alone.
    #[test]
    fn the_sweep_reaps_only_what_is_older_than_the_bound() {
        let (_dir, image, cache) = media_and_cache("sweep");
        cache.store(&image, &listing_for(&image));
        let fresh = cache.file_for(&image).unwrap();

        let ScanCache::In(cache_dir) = &cache else {
            unreachable!()
        };
        let old = cache_dir.join(format!(
            "{CACHE_FILE_PREFIX}0000000000000000{CACHE_FILE_SUFFIX}"
        ));
        std::fs::write(&old, b"{}").unwrap();
        filetime_set(
            &old,
            SystemTime::now() - CACHE_MAX_AGE - Duration::from_secs(60),
        );

        cache.sweep();
        assert!(fresh.exists(), "a fresh entry survives the sweep");
        assert!(!old.exists(), "an entry past the age bound does not");
    }

    /// **The cached source answers exactly what the medium answers.** Asked
    /// of every path the medium holds, plus its root, plus a missing path,
    /// plus each path spelled in the other case — so a divergence cannot
    /// hide in the one path the test forgot to ask about.
    #[test]
    fn a_cached_source_answers_what_the_real_source_answers() {
        let (_dir, image, _cache) = media_and_cache("agrees");
        let found = identify(&image).unwrap();
        let mut real = open_media(&found).unwrap();
        let listing = listing_of(real.as_mut(), found.kind).unwrap();
        let path = image.clone();
        let mut cached = CachedSource::new(listing.clone(), move || {
            open_media(&identify(&path).expect("still media"))
        });

        let mut paths: Vec<String> = vec![String::new()];
        paths.extend(listing.entries.iter().map(|e| e.path.clone()));
        paths.push("Libs/Nothing".into());
        for p in &paths {
            paths.iter().for_each(|_| ());
            assert_eq!(
                real.entry(p).unwrap(),
                cached.entry(p).unwrap(),
                "entry({p:?})"
            );
            assert_eq!(
                real.entry(&p.to_uppercase()).unwrap(),
                cached.entry(&p.to_uppercase()).unwrap(),
                "entry({:?}) — the medium's own casing must come back from both",
                p.to_uppercase()
            );
            let (r, c) = (real.walk(p), cached.walk(p));
            match (r, c) {
                (Ok(a), Ok(b)) => assert_eq!(a, b, "walk({p:?})"),
                (Err(a), Err(b)) => assert_eq!(a.to_string(), b.to_string(), "walk({p:?})"),
                (a, b) => panic!("walk({p:?}) disagreed: real={a:?} cached={b:?}"),
            }
        }

        // And the bytes come off the medium, not out of the listing.
        assert_eq!(cached.read("C/LoadModule").unwrap(), b"cmd");
    }

    /// A cached source does not open the medium to answer a walk — which is
    /// the entire point, and would go unnoticed if it stopped being true.
    #[test]
    fn walking_a_cached_source_never_opens_the_medium() {
        let (_dir, image, _cache) = media_and_cache("no-open");
        let found = identify(&image).unwrap();
        let listing = {
            let mut real = open_media(&found).unwrap();
            listing_of(real.as_mut(), found.kind).unwrap()
        };
        // The medium is deleted, so opening it is impossible. Every listing
        // question must still be answered.
        std::fs::remove_file(&image).unwrap();
        let mut cached = CachedSource::new(listing, || {
            Err(CoreError::InvalidInput("must not be opened".into()))
        });
        assert!(cached.entry("").unwrap().is_some());
        assert!(cached.entry("C").unwrap().is_some());
        assert!(!cached.walk("").unwrap().is_empty());
        assert_eq!(cached.walk("C").unwrap().len(), 1);
        // …and reading bytes *does* reach for it.
        assert!(cached.read("C/LoadModule").is_err());
    }

    /// `find_media` still finds the fixture, so the cache is keyed on
    /// something the scan actually produces rather than a path invented
    /// here.
    #[test]
    fn the_cache_is_keyed_on_a_path_the_scan_itself_reports() {
        let (dir, image, cache) = media_and_cache("scan-keyed");
        let found = find_media(dir.path()).unwrap();
        let scanned = found
            .iter()
            .find(|f| f.path == image)
            .expect("find_media reports the fixture");
        cache.store(&scanned.path, &listing_for(&scanned.path));
        assert_eq!(
            cache.lookup(&image).unwrap().volume_name,
            "Workbench3.2",
            "the scan's own path and this one are the same key"
        );
    }

    /// Replace a medium's contents with bytes that are **not** media, while
    /// keeping its path, its length and its modification time exactly as they
    /// were.
    ///
    /// This is the arrangement ART-194 says is real and the cheap key cannot
    /// see: a restored backup keeps its timestamps, and several AmigaOS 3.9
    /// ISOs are in circulation. It is also the only fixture that can prove a
    /// cache hit *is* a hit — the medium can no longer be read at all, so an
    /// answer can only have come out of the cache.
    fn swap_contents_keeping_identity(path: &Path) {
        let metadata = std::fs::metadata(path).unwrap();
        let stamp = metadata.modified().unwrap();
        let rubbish = vec![0x5au8; metadata.len() as usize];
        std::fs::write(path, &rubbish).unwrap();
        assert_eq!(std::fs::metadata(path).unwrap().len(), metadata.len());
        filetime_set(path, stamp);
        assert!(
            identify(path).is_none(),
            "the fixture must be genuinely unreadable now, or the test proves nothing"
        );
    }

    /// **The cache is consulted, and this cannot pass if it is not.**
    ///
    /// The trap named in ART-194's own round: a cache-hit test that would pass
    /// even if the cache were never looked at, because the real read would
    /// have produced the same answer. So the medium is made unreadable between
    /// the two calls, with its `(path, size, mtime)` untouched. `open_media`
    /// on it now fails outright — asserted, so the fixture cannot rot into a
    /// readable one — and the second `open_media_cached` still answers the
    /// whole listing.
    #[test]
    fn a_second_open_is_answered_from_the_cache_and_not_from_the_medium() {
        let (_dir, image, cache) = media_and_cache("consulted");
        let found = identify(&image).expect("the fixture is media");

        let listing = {
            let mut first = open_media_cached(&found, &cache).unwrap();
            listing_of(first.as_mut(), found.kind).unwrap()
        };
        assert!(!listing.entries.is_empty());

        swap_contents_keeping_identity(&image);
        assert!(
            open_media(&found).is_err(),
            "the medium itself can no longer answer anything"
        );

        let mut second = open_media_cached(&found, &cache).unwrap();
        assert_eq!(second.walk("").unwrap(), listing.entries);
        assert_eq!(second.volume_name(), listing.volume_name);
    }

    /// **The rescan actually rescans**, and this cannot pass because the cache
    /// happened to be empty: the assertion above it proves the very same
    /// arrangement was being served *from* the cache a line earlier.
    ///
    /// After `forget_all`, the only place an answer could come from is the
    /// medium — which is unreadable — so the correct behaviour is to fail
    /// rather than to keep answering. That failure is the escape hatch
    /// working: a user who suspects the disc is not the disc gets ART to look
    /// again instead of being told something untrue with confidence.
    #[test]
    fn a_rescan_goes_back_to_the_medium_instead_of_the_remembered_listing() {
        let (_dir, image, cache) = media_and_cache("rescan");
        let found = identify(&image).expect("the fixture is media");
        {
            let mut first = open_media_cached(&found, &cache).unwrap();
            assert!(!first.walk("").unwrap().is_empty());
        }

        swap_contents_keeping_identity(&image);
        assert!(
            open_media_cached(&found, &cache).is_ok(),
            "the cache is serving this — which is exactly the stale answer the rescan exists for"
        );

        assert_eq!(cache.forget_all(), 1, "one remembered listing dropped");

        assert!(
            open_media_cached(&found, &cache).is_err(),
            "after a rescan the answer must come off the medium, whatever the medium now says"
        );
    }

    /// A cache that is switched off never serves the stale answer either —
    /// the same fixture, and the same medium, read every time.
    #[test]
    fn a_cache_that_is_off_always_reads_the_medium() {
        let (_dir, image, _cache) = media_and_cache("off-reads");
        let off = ScanCache::off();
        let found = identify(&image).expect("the fixture is media");
        assert!(open_media_cached(&found, &off).is_ok());
        swap_contents_keeping_identity(&image);
        assert!(open_media_cached(&found, &off).is_err());
    }

    /// **The measurement ART-194 and ART-195 both asked for**, against the
    /// owner's own 468 MB `AmigaOS39.iso` rather than a synthetic disc.
    ///
    /// ```text
    /// cd src-tauri && ART_OS39_ISO="E:\amiga\Amigatolon\os39\AmigaOS39.iso" \
    ///   cargo test --release --lib scan_cache::tests::measure -- --ignored --nocapture
    /// ```
    ///
    /// Release, because that is what the owner runs; the numbers in
    /// `docs/ISSUES.md` come from this test and nothing else.
    ///
    /// Three things are timed, in the order they were fixed:
    /// 1. one cold plan;
    /// 2. **four concurrent** cold plans — what the screen was actually doing
    ///    before ART-195, since every toggle started another and cancelled
    ///    nothing;
    /// 3. a warm plan, the same work with ART-194's cache.
    #[test]
    #[ignore = "reads the owner's real 468 MB disc; run explicitly, see the doc comment"]
    fn measure_a_real_disc_cold_concurrent_and_warm() {
        let Ok(iso) = std::env::var("ART_OS39_ISO") else {
            return;
        };
        let iso = PathBuf::from(iso);
        let folder = iso
            .parent()
            .expect("the ISO sits in a folder")
            .to_path_buf();
        let recipe = crate::core::osinstall::recipe::by_release("AmigaOS 3.9").unwrap();
        let request = crate::core::osinstall::plan::InstallRequest {
            media_folder: folder,
            extra_media_folders: Vec::new(),
            media_folders: std::collections::BTreeMap::new(),
            keymap: None,
            rom: None,
            chosen: Vec::new(),
            excluded: Vec::new(),
            packages: Vec::new(),
            package_folder: None,
            destination: std::env::temp_dir().join("art-measure-dest"),
            release: "AmigaOS 3.9".into(),
            scan_cache: Default::default(),
        };

        let dir = ScratchDir::new("art-scan-cache", "measure");
        let cache = ScanCache::in_dir(dir.join("cache"));
        use crate::core::osinstall::plan::plan_with_cache;

        // 1. one cold plan.
        cache.forget_all();
        let t = std::time::Instant::now();
        let cold = plan_with_cache(&request, &recipe, &ScanCache::off()).unwrap();
        let one_cold = t.elapsed();
        println!(
            "[1] one cold plan: {one_cold:?} ({} items)",
            cold.items.len()
        );

        // 2. four concurrent cold plans — ART-195's shape, measured as wall
        //    time to the last one finishing, which is what the user waits.
        let t = std::time::Instant::now();
        std::thread::scope(|scope| {
            for _ in 0..4 {
                let request = &request;
                let recipe = &recipe;
                scope.spawn(move || {
                    plan_with_cache(request, recipe, &ScanCache::off()).unwrap();
                });
            }
        });
        let four_cold = t.elapsed();
        println!("[2] four concurrent cold plans: {four_cold:?}");

        // 3. warm: prime once, then measure the next one.
        cache.forget_all();
        let t = std::time::Instant::now();
        plan_with_cache(&request, &recipe, &cache).unwrap();
        println!(
            "[3a] cold plan that also fills the cache: {:?}",
            t.elapsed()
        );
        let t = std::time::Instant::now();
        let warm = plan_with_cache(&request, &recipe, &cache).unwrap();
        let one_warm = t.elapsed();
        println!("[3b] warm plan: {one_warm:?} ({} items)", warm.items.len());

        // The measurement is only worth anything if both plans say the same
        // thing about the disc.
        assert_eq!(cold.items.len(), warm.items.len());
        assert_eq!(cold.total_bytes, warm.total_bytes);
        assert_eq!(cold.refusals, warm.refusals);

        println!(
            "[summary] four-concurrent/one-cold = {:.2}x   cold/warm = {:.1}x",
            four_cold.as_secs_f64() / one_cold.as_secs_f64(),
            one_cold.as_secs_f64() / one_warm.as_secs_f64()
        );
    }

    /// Set a file's modification time without pulling in a crate for it.
    fn filetime_set(path: &Path, when: SystemTime) {
        let file = std::fs::OpenOptions::new().write(true).open(path).unwrap();
        file.set_modified(when).unwrap();
        file.sync_all().unwrap();
    }
}
