# Collection Artwork (wave B) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fill the Collection's empty `preview` field by fetching artwork from two
configured sources, matching strictly, and caching per title in its own directory.

**Architecture:** A new `core/artwork/` module declares what it needs and opens no
connection: it takes `&dyn MirrorClient` (already in `core/sources/mirror.rs`) and
`&dyn ProgressSink`. Sources are code, not data — each ships with ART and exposes
only an on/off switch and an editable mirror base. Enrichment is a job the user
starts; nothing reaches the network when a screen opens.

**Tech Stack:** Rust (`std` + `serde` + `serde_json` only in core), Tauri commands,
React + TypeScript, `react-i18next`.

**Spec:** [docs/superpowers/specs/2026-08-17-collection-artwork-design.md](../specs/2026-08-17-collection-artwork-design.md)

## Global Constraints

- **`core/` opens no connection and imports no Tauri.** `core/artwork` takes
  `&dyn MirrorClient`; the implementation stays in `src-tauri/src/net/`.
- **A lower-level `core/` module must not import a higher-level one.**
  `core/artwork` may read `core/gameindex::record` types; `core/gameindex` must
  **not** import `core/artwork`. Joining a record to its artwork is a
  command-layer job.
- **No function fetches a caller-supplied URL.** Every request is
  `Mirror::url_for(validated_repo_path)`. `validate_fetch_path` rejects spaces,
  `:`, `?`, `#`, `\`, `//`, leading `/` and `.`/`..` segments — do not relax it;
  percent-encode in `core/artwork` instead.
- **Every write goes through `core::safety::atomic_write`.** Derived data, so no
  backup policy — but a half-written PNG must never exist.
- **`safe_join` is the only way an index entry name becomes a destination path.**
- **Cancellation is checked between whole titles, never mid-write.** Return
  `CoreError::Cancelled`.
- **Rate limit: at most 4 requests per second per host, sequential, one
  connection.** A constant in `core/artwork`, never a setting.
- **i18n:** every new key goes in **both** `src/i18n/en.json` and `tr.json` in the
  same commit, or `pnpm test` fails.
- **`src/lib/*` never renders a string.** A helper returns `Phrase { key, params? }`
  and the component calls `t()`.
- **Fixtures are synthetic and generated at runtime in a tempdir.** No network in
  the test suite: sources are tested against a fake `MirrorClient`.
- **New commands go in both `invoke_handler![]` in `lib.rs` and a typed wrapper in
  `src/lib/*.ts`.** Components never call `invoke` directly.
- MSRV 1.93. `cargo clippy --all-targets -- -D warnings` is blocking.

---

## File Structure

| File | Responsibility |
|---|---|
| `src-tauri/src/core/artwork/mod.rs` | `ArtKind`, `ArtRef`, module wiring |
| `src-tauri/src/core/artwork/encode.rs` | percent-encoding for path segments |
| `src-tauri/src/core/artwork/key.rs` | normalisation + the two matching rules |
| `src-tauri/src/core/artwork/cache.rs` | on-disk cache: entries, misses, atomic writes |
| `src-tauri/src/core/artwork/config.rs` | `SourceId`, `ConfiguredSource`, shipped defaults |
| `src-tauri/src/core/artwork/sources/mod.rs` | `ArtSource` trait, `SourceIndex` |
| `src-tauri/src/core/artwork/sources/whdload_de.rs` | exact package-name paths |
| `src-tauri/src/core/artwork/sources/libretro.rs` | two-step git-tree index + image paths |
| `src-tauri/src/core/artwork/enrich.rs` | the run: match, fetch, cache, report |
| `src-tauri/src/commands/artwork.rs` | thin adapters + job spawn |
| `src/lib/artwork.ts` | typed command wrappers |
| `src/pages/Settings.tsx` | the source list (on/off + mirror) |
| `src/pages/CollectionStudio.tsx` | thumbnails + the Enrich action |

---

## Task 1: The vocabulary — `ArtKind`, `ArtRef`, percent-encoding

**Files:**
- Create: `src-tauri/src/core/artwork/mod.rs`
- Create: `src-tauri/src/core/artwork/encode.rs`
- Modify: `src-tauri/src/core/mod.rs` (add `pub mod artwork;`)

**Interfaces:**
- Produces: `ArtKind` (`Boxart`, `Snap`, `Title`, `Logo`, `Icon`) with
  `as_str(&self) -> &'static str`; `ArtRef { kind, source, file }`;
  `encode::path_segment(&str) -> String`.

- [ ] **Step 1: Write the failing test for percent-encoding**

Create `src-tauri/src/core/artwork/encode.rs` with only the test module:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_space_becomes_percent_twenty() {
        assert_eq!(path_segment("1000 Miglia"), "1000%20Miglia");
    }

    /// The real libretro corpus contains apostrophes; they are legal in a URL
    /// path and must survive untouched, or the constructed path 404s.
    #[test]
    fn an_apostrophe_survives() {
        assert_eq!(path_segment("'Allo 'Allo"), "'Allo%20'Allo");
    }

    /// A separator inside a segment would create a directory level that the
    /// caller did not ask for.
    #[test]
    fn a_slash_is_encoded_not_passed_through() {
        assert_eq!(path_segment("a/b"), "a%2Fb");
    }

    /// Everything the validator rejects must be gone by the time it is asked.
    #[test]
    fn the_characters_the_validator_rejects_are_all_encoded() {
        let encoded = path_segment("a b:c?d#e\\f");
        for bad in [' ', ':', '?', '#', '\\'] {
            assert!(!encoded.contains(bad), "{bad:?} survived in {encoded}");
        }
    }

    #[test]
    fn a_percent_is_itself_encoded_so_encoding_is_not_ambiguous() {
        assert_eq!(path_segment("100%"), "100%25");
    }
}
```

- [ ] **Step 2: Run it and watch it fail**

Run: `cd src-tauri && cargo test artwork::encode`
Expected: FAIL — `path_segment` is not defined.

- [ ] **Step 3: Implement `path_segment`**

Put this above the test module in `encode.rs`:

```rust
//! Percent-encoding for one path segment.
//!
//! `core::sources::mirror::validate_fetch_path` rejects spaces, `:`, `?`, `#`,
//! `\` and `//`, because any of them could re-point a request somewhere the
//! caller did not intend. Artwork filenames legitimately contain spaces —
//! `1000 Miglia - 1927-1933 Volume 1.png` — so the *segment* is encoded here
//! rather than the validator being weakened to admit it.

/// Encode one path segment, leaving only characters that are unreserved in
/// RFC 3986 plus the sub-delims a filename realistically uses.
///
/// `/` is **not** exempt: this encodes a single segment, and a caller joining
/// segments does so itself.
pub fn path_segment(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    for byte in raw.bytes() {
        let keep = byte.is_ascii_alphanumeric()
            || matches!(byte, b'-' | b'_' | b'.' | b'~' | b'\'' | b'(' | b')' | b'!' | b'*');
        if keep {
            out.push(byte as char);
        } else {
            out.push('%');
            out.push_str(&format!("{byte:02X}"));
        }
    }
    out
}
```

- [ ] **Step 4: Run the tests**

Run: `cd src-tauri && cargo test artwork::encode`
Expected: PASS (5 tests).

- [ ] **Step 5: Write `mod.rs` with its own test**

Create `src-tauri/src/core/artwork/mod.rs`:

```rust
//! Artwork for catalogue titles, fetched from configured sources.
//!
//! This module declares what it needs and opens no connection: every fetch goes
//! through a `&dyn MirrorClient` supplied by the caller, so the whole module is
//! testable with a fake and the test suite touches no network.
//!
//! It reads `core::gameindex::record` types. It must never be imported *by*
//! `core::gameindex` — joining a record to its artwork is a command-layer job.

pub mod encode;

use serde::{Deserialize, Serialize};

/// The kinds of picture the configured sources publish.
///
/// `Boxart`, `Snap`, `Title` and `Logo` are libretro's four directories,
/// measured against the live repository. `Icon` is whdload.de's Amiga icon,
/// which is not box art and is not presented as one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ArtKind {
    Boxart,
    Snap,
    Title,
    Logo,
    Icon,
}

impl ArtKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Boxart => "boxart",
            Self::Snap => "snap",
            Self::Title => "title",
            Self::Logo => "logo",
            Self::Icon => "icon",
        }
    }

    /// Every kind, in the order a screen should prefer them.
    pub const ALL: [ArtKind; 5] = [
        ArtKind::Boxart,
        ArtKind::Title,
        ArtKind::Snap,
        ArtKind::Logo,
        ArtKind::Icon,
    ];
}

/// What a title gained: which picture, from whom, and where it landed.
///
/// `file` is **cache-relative**, never absolute and never a URL, so moving the
/// cache directory does not invalidate what points into it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArtRef {
    pub kind: ArtKind,
    /// The `SourceId` string that provided it.
    pub source: String,
    pub file: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn art_kind_serialises_kebab_case_for_the_frontend() {
        let json = serde_json::to_string(&ArtKind::Boxart).unwrap();
        assert_eq!(json, "\"boxart\"");
    }

    #[test]
    fn every_kind_is_listed_in_all() {
        // A kind added to the enum but not to ALL would silently never be
        // fetched. There is no derive that catches that, so assert the count.
        assert_eq!(ArtKind::ALL.len(), 5);
        for kind in ArtKind::ALL {
            assert!(!kind.as_str().is_empty());
        }
    }
}
```

- [ ] **Step 6: Register the module**

In `src-tauri/src/core/mod.rs`, add `pub mod artwork;` in alphabetical position
(before `pub mod card;`).

- [ ] **Step 7: Verify the whole crate still builds clean**

Run: `cd src-tauri && cargo test artwork:: && cargo clippy --all-targets -- -D warnings`
Expected: PASS, no warnings.

- [ ] **Step 8: Commit**

```bash
git add src-tauri/src/core/artwork/ src-tauri/src/core/mod.rs
git commit -m "feat(artwork): the vocabulary, and encoding that keeps the validator strict"
```

---

## Task 2: Matching — normalisation and exactly two rules

**Files:**
- Create: `src-tauri/src/core/artwork/key.rs`
- Modify: `src-tauri/src/core/artwork/mod.rs` (add `pub mod key;`)

**Interfaces:**
- Consumes: nothing from Task 1.
- Produces: `key::normalise(&str) -> String`;
  `key::lookup<'a>(index: &'a BTreeMap<String, String>, title: &str) -> Option<&'a str>`.

- [ ] **Step 1: Write the failing tests**

Create `src-tauri/src/core/artwork/key.rs` containing only:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn index(entries: &[(&str, &str)]) -> BTreeMap<String, String> {
        entries
            .iter()
            .map(|(name, path)| (normalise(name), (*path).to_string()))
            .collect()
    }

    #[test]
    fn normalisation_folds_case_and_collapses_space() {
        assert_eq!(normalise("  Turrican   II  "), "turrican ii");
    }

    #[test]
    fn normalisation_drops_a_leading_article() {
        assert_eq!(normalise("The Settlers"), "settlers");
        assert_eq!(normalise("A Prehistoric Tale"), "prehistoric tale");
    }

    /// "Theme Park" starts with "The" but the article is "The ", with a space.
    #[test]
    fn a_word_beginning_with_the_is_not_an_article() {
        assert_eq!(normalise("Theme Park"), "theme park");
    }

    #[test]
    fn rule_one_matches_the_whole_title() {
        let idx = index(&[("Turrican II", "Named_Boxarts/Turrican II.png")]);
        assert_eq!(
            lookup(&idx, "Turrican II"),
            Some("Named_Boxarts/Turrican II.png")
        );
    }

    /// The real case this rule exists for: the user's catalogue holds `1869`,
    /// libretro holds `1869 - Erlebte Geschichte Teil I`.
    #[test]
    fn rule_two_matches_the_head_before_a_dash() {
        let idx = index(&[(
            "1869 - Erlebte Geschichte Teil I",
            "Named_Boxarts/1869 - Erlebte Geschichte Teil I.png",
        )]);
        assert_eq!(
            lookup(&idx, "1869"),
            Some("Named_Boxarts/1869 - Erlebte Geschichte Teil I.png")
        );
    }

    /// Two releases of the same title, German and English. Both are legitimate;
    /// the choice must be the same on every run or two scans disagree.
    #[test]
    fn a_head_matching_two_candidates_picks_the_same_one_every_time() {
        let idx = index(&[
            ("1869 - History Experience Part I", "en.png"),
            ("1869 - Erlebte Geschichte Teil I", "de.png"),
        ]);
        let first = lookup(&idx, "1869").unwrap().to_string();
        for _ in 0..8 {
            assert_eq!(lookup(&idx, "1869").unwrap(), first);
        }
        // Sorted order, so the German title wins — stated so a change is visible.
        assert_eq!(first, "de.png");
    }

    /// Rule 1 must win outright. A title that exists whole is never resolved
    /// through the looser rule.
    #[test]
    fn whole_title_beats_head_match() {
        let idx = index(&[
            ("1869", "exact.png"),
            ("1869 - Erlebte Geschichte Teil I", "subtitled.png"),
        ]);
        assert_eq!(lookup(&idx, "1869"), Some("exact.png"));
    }

    /// The strictness the design chose: no edit distance, no token overlap.
    #[test]
    fn a_near_miss_does_not_match() {
        let idx = index(&[("Turrican II", "t2.png")]);
        assert_eq!(lookup(&idx, "Turrican"), None);
        assert_eq!(lookup(&idx, "Turricn II"), None);
    }

    #[test]
    fn an_empty_title_matches_nothing() {
        let idx = index(&[("Turrican II", "t2.png")]);
        assert_eq!(lookup(&idx, "   "), None);
    }
}
```

- [ ] **Step 2: Run and watch it fail**

Run: `cd src-tauri && cargo test artwork::key`
Expected: FAIL — `normalise` and `lookup` are not defined.

- [ ] **Step 3: Implement**

Put above the test module:

```rust
//! Turning a title into a lookup key, and the two rules that use it.
//!
//! Matching is deliberately strict (spec §3). Online artwork is provenance rank
//! 2 — below the WHDLoad slave (3) and the user's own edit (4) — so it fills
//! gaps and never overwrites a stated fact. A silently accepted wrong guess
//! would make that ranking meaningless, so there is no edit distance here and
//! no token overlap: a title either matches by one of two written rules or it
//! has no artwork.

use std::collections::BTreeMap;

/// Articles dropped from the front of a title, longest first so `An ` is tested
/// before `A `.
const ARTICLES: [&str; 3] = ["the ", "an ", "a "];

/// Fold a title into its lookup key: lower case, single spaces, no leading
/// article.
pub fn normalise(raw: &str) -> String {
    let lowered = raw.to_lowercase();
    let mut folded = String::with_capacity(lowered.len());
    let mut last_was_space = true;
    for ch in lowered.chars() {
        if ch.is_whitespace() {
            if !last_was_space {
                folded.push(' ');
            }
            last_was_space = true;
        } else {
            folded.push(ch);
            last_was_space = false;
        }
    }
    let trimmed = folded.trim_end();

    for article in ARTICLES {
        if let Some(rest) = trimmed.strip_prefix(article) {
            return rest.to_string();
        }
    }
    trimmed.to_string()
}

/// Find `title` in an index keyed by [`normalise`]d name.
///
/// Rule 1 is whole-title equality. Rule 2 — used only when rule 1 finds nothing
/// — is equality against the part before the first ` - `, which is what connects
/// `1869` to `1869 - Erlebte Geschichte Teil I`.
///
/// Rule 2 can match more than one candidate. `BTreeMap` iterates in sorted key
/// order and the first is taken, so the answer is the same on every run.
pub fn lookup<'a>(index: &'a BTreeMap<String, String>, title: &str) -> Option<&'a str> {
    let key = normalise(title);
    if key.is_empty() {
        return None;
    }

    if let Some(hit) = index.get(&key) {
        return Some(hit.as_str());
    }

    let prefix = format!("{key} - ");
    index
        .range(prefix.clone()..)
        .take_while(|(candidate, _)| candidate.starts_with(&prefix))
        .map(|(_, path)| path.as_str())
        .next()
}
```

- [ ] **Step 4: Register and run**

Add `pub mod key;` to `src-tauri/src/core/artwork/mod.rs`.

Run: `cd src-tauri && cargo test artwork::key`
Expected: PASS (9 tests).

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/core/artwork/
git commit -m "feat(artwork): two matching rules, and nothing that guesses"
```

---

## Task 3: The cache — entries, misses, and a re-run that fetches nothing

**Files:**
- Create: `src-tauri/src/core/artwork/cache.rs`
- Modify: `src-tauri/src/core/artwork/mod.rs` (add `pub mod cache;`)

**Interfaces:**
- Consumes: `ArtKind`, `ArtRef` (Task 1); `core::safety::atomic_write`;
  `core::security::path::safe_join`.
- Produces: `cache::Cache` with `open(dir) -> CoreResult<Cache>`,
  `get(&self, title_key, kind) -> Option<&ArtRef>`,
  `is_missing(&self, title_key, kind, source) -> bool`,
  `store(&mut self, title_key, kind, source, ext, bytes) -> CoreResult<ArtRef>`,
  `record_miss(&mut self, title_key, kind, source)`,
  `save(&self) -> CoreResult<()>`.

- [ ] **Step 1: Write the failing tests**

Create `src-tauri/src/core/artwork/cache.rs` containing only:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn tempdir() -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "art-artwork-cache-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn a_stored_image_comes_back_and_its_bytes_are_on_disk() {
        let dir = tempdir();
        let mut cache = Cache::open(&dir).unwrap();
        let art = cache
            .store("turrican ii", ArtKind::Boxart, "libretro", "png", b"PNGDATA")
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
        let dir = tempdir();
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
        let dir = tempdir();
        let mut cache = Cache::open(&dir).unwrap();
        cache.record_miss("moonstone", ArtKind::Icon, "libretro");
        assert!(cache.is_missing("moonstone", ArtKind::Icon, "libretro"));
        assert!(!cache.is_missing("moonstone", ArtKind::Icon, "whdload-de"));
    }

    /// A title is a user-influenced string. It must never escape the cache
    /// directory, whatever it contains.
    #[test]
    fn a_traversing_title_cannot_write_outside_the_cache() {
        let dir = tempdir();
        let mut cache = Cache::open(&dir).unwrap();
        let art = cache
            .store("../../evil", ArtKind::Boxart, "libretro", "png", b"X")
            .unwrap();

        let written = dir.join(&art.file).canonicalize().unwrap();
        assert!(
            written.starts_with(dir.canonicalize().unwrap()),
            "{written:?} escaped the cache directory"
        );
    }

    /// Data safety: a cache that was never saved must not have half-written its
    /// index over a good one.
    #[test]
    fn opening_a_directory_with_a_corrupt_index_starts_empty_rather_than_failing() {
        let dir = tempdir();
        std::fs::write(dir.join(INDEX_FILE), b"{ not json").unwrap();
        let cache = Cache::open(&dir).unwrap();
        assert!(cache.get("anything", ArtKind::Boxart).is_none());
    }
}
```

- [ ] **Step 2: Run and watch it fail**

Run: `cd src-tauri && cargo test artwork::cache`
Expected: FAIL — `Cache` is not defined.

- [ ] **Step 3: Implement**

Put above the test module:

```rust
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

/// The longest a cache filename may get before it is hashed instead.
const MAX_STEM: usize = 96;

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
    format!("{title_key}\u{1f}{}", kind.as_str())
}

fn miss_key(title_key: &str, kind: ArtKind, source: &str) -> String {
    format!("{title_key}\u{1f}{}\u{1f}{source}", kind.as_str())
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

    pub fn get(&self, title_key: &str, kind: ArtKind) -> Option<&ArtRef> {
        self.file.entries.get(&entry_key(title_key, kind))
    }

    pub fn is_missing(&self, title_key: &str, kind: ArtKind, source: &str) -> bool {
        self.file.misses.contains(&miss_key(title_key, kind, source))
    }

    pub fn record_miss(&mut self, title_key: &str, kind: ArtKind, source: &str) {
        self.file.misses.insert(miss_key(title_key, kind, source));
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
        let stem: String = title_key
            .chars()
            .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '-' })
            .take(MAX_STEM)
            .collect();
        let stem = stem.trim_matches('-').to_string();
        let stem = if stem.is_empty() {
            "untitled".to_string()
        } else {
            stem
        };

        let relative = format!("{}/{stem}-{source}.{ext}", kind.as_str());
        let destination = safe_join(&self.dir, &relative)?;
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

    /// Persist the index. Derived data, so atomic but unbacked.
    pub fn save(&self) -> CoreResult<()> {
        let bytes = serde_json::to_vec(&self.file)
            .map_err(|err| CoreError::InvalidInput(format!("cannot serialise the cache: {err}")))?;
        atomic_write(&self.dir.join(INDEX_FILE), &bytes)
    }
}
```

- [ ] **Step 4: Register and run**

Add `pub mod cache;` to `src-tauri/src/core/artwork/mod.rs`.

Run: `cd src-tauri && cargo test artwork::cache`
Expected: PASS (5 tests).

**If `safe_join` has a different signature than `safe_join(base, relative)`,**
read `src-tauri/src/core/security/path.rs` and adapt the call — do not write your
own containment check.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/core/artwork/
git commit -m "feat(artwork): a cache that remembers what nobody has, too"
```

---

## Task 4: `ArtSource`, and whdload.de as the source with no index

**Files:**
- Create: `src-tauri/src/core/artwork/sources/mod.rs`
- Create: `src-tauri/src/core/artwork/sources/whdload_de.rs`
- Modify: `src-tauri/src/core/artwork/mod.rs` (add `pub mod sources;`)

**Interfaces:**
- Consumes: `ArtKind` (Task 1), `key::normalise` (Task 2), `encode::path_segment` (Task 1).
- Produces:
  ```rust
  pub struct SourceIndex { pub by_kind: BTreeMap<ArtKind, BTreeMap<String, String>> }
  pub trait ArtSource: Send + Sync {
      fn id(&self) -> &'static str;
      fn kinds(&self) -> &'static [ArtKind];
      fn index_paths(&self) -> Vec<(ArtKind, String)>;
      fn absorb_index(&self, kind: ArtKind, bytes: &[u8], into: &mut SourceIndex) -> CoreResult<()>;
      fn locate(&self, index: &SourceIndex, title: &str, kind: ArtKind) -> Option<String>;
  }
  pub struct WhdloadDe;
  ```

- [ ] **Step 1: Write `sources/mod.rs`**

```rust
//! What a source is, and what it is not.
//!
//! A source knows how to find a title's picture *inside its own repository*. It
//! never builds a URL: `Mirror::url_for` does that, once, at the last point
//! before bytes leave the machine. It never opens a connection either — the
//! caller fetches and hands the bytes back.
//!
//! Sources are code rather than data because their index formats differ. The
//! configurable part is per source and small: on/off, and the mirror base
//! (spec §5).

pub mod libretro;
pub mod whdload_de;

use std::collections::BTreeMap;

use crate::core::artwork::ArtKind;
use crate::core::error::CoreResult;

/// What a source parsed out of its index files: normalised title -> repository
/// path, one map per kind.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct SourceIndex {
    pub by_kind: BTreeMap<ArtKind, BTreeMap<String, String>>,
}

pub trait ArtSource: Send + Sync {
    /// Stable identifier, stored in the cache and in settings. Never localised.
    fn id(&self) -> &'static str;

    /// The kinds this source can supply.
    fn kinds(&self) -> &'static [ArtKind];

    /// Index files to fetch before matching can start. Empty when the source
    /// needs none — whdload.de builds its path from the title alone.
    fn index_paths(&self) -> Vec<(ArtKind, String)>;

    /// Parse one fetched index into `into`.
    fn absorb_index(&self, kind: ArtKind, bytes: &[u8], into: &mut SourceIndex) -> CoreResult<()>;

    /// The repository path holding this title's picture, already encoded so it
    /// passes `validate_fetch_path`.
    fn locate(&self, index: &SourceIndex, title: &str, kind: ArtKind) -> Option<String>;
}
```

- [ ] **Step 2: Write the failing test for whdload.de**

Create `src-tauri/src/core/artwork/sources/whdload_de.rs` with only:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_needs_no_index() {
        assert!(WhdloadDe.index_paths().is_empty());
    }

    /// The key is the package name ART already read from the slave, so there is
    /// no matching problem here at all.
    #[test]
    fn the_path_is_built_from_the_title_alone() {
        let empty = SourceIndex::default();
        assert_eq!(
            WhdloadDe.locate(&empty, "Moonstone", ArtKind::Icon),
            Some("games/ico/Moonstone.png".to_string())
        );
    }

    #[test]
    fn a_space_in_the_name_is_encoded_not_passed_through() {
        let empty = SourceIndex::default();
        let path = WhdloadDe
            .locate(&empty, "Cannon Fodder", ArtKind::Icon)
            .unwrap();
        assert_eq!(path, "games/ico/Cannon%20Fodder.png");
        assert!(!path.contains(' '));
    }

    #[test]
    fn it_offers_nothing_but_an_icon() {
        assert_eq!(WhdloadDe.kinds(), &[ArtKind::Icon]);
        let empty = SourceIndex::default();
        assert_eq!(WhdloadDe.locate(&empty, "Moonstone", ArtKind::Boxart), None);
    }

    #[test]
    fn an_empty_title_locates_nothing() {
        let empty = SourceIndex::default();
        assert_eq!(WhdloadDe.locate(&empty, "  ", ArtKind::Icon), None);
    }
}
```

- [ ] **Step 3: Run and watch it fail**

Run: `cd src-tauri && cargo test artwork::sources::whdload_de`
Expected: FAIL — `WhdloadDe` is not defined.

- [ ] **Step 4: Implement**

```rust
//! whdload.de — the source with an exact key.
//!
//! Package pages carry a predictable layout, confirmed against
//! `https://www.whdload.de/games/Moonstone.html`:
//!
//! | what    | path                    |
//! |---------|-------------------------|
//! | package | `games/<Name>.lha`      |
//! | icon    | `games/ico/<Name>.png`  |
//!
//! `<Name>` is the WHDLoad package name, which ART already holds — 1681 of one
//! user's 1700 records were titled from their slave. There is no matching
//! problem here: the key is exact or the record has no key.
//!
//! What the site does not give is equally clear. Its metadata lives on HTML
//! pages, may not be scraped (§41.5.3), and is redundant anyway — the slave
//! stated it offline. The Hall of Light and Lemon Amiga cross-reference ids
//! found only on those pages would make matching exact everywhere; they are not
//! obtainable within the rules and this source does without them.

use crate::core::artwork::encode::path_segment;
use crate::core::artwork::sources::{ArtSource, SourceIndex};
use crate::core::artwork::ArtKind;
use crate::core::error::CoreResult;

const KINDS: [ArtKind; 1] = [ArtKind::Icon];

#[derive(Debug, Default, Clone, Copy)]
pub struct WhdloadDe;

impl ArtSource for WhdloadDe {
    fn id(&self) -> &'static str {
        "whdload-de"
    }

    fn kinds(&self) -> &'static [ArtKind] {
        &KINDS
    }

    fn index_paths(&self) -> Vec<(ArtKind, String)> {
        Vec::new()
    }

    fn absorb_index(&self, _kind: ArtKind, _bytes: &[u8], _into: &mut SourceIndex) -> CoreResult<()> {
        Ok(())
    }

    fn locate(&self, _index: &SourceIndex, title: &str, kind: ArtKind) -> Option<String> {
        if kind != ArtKind::Icon {
            return None;
        }
        let trimmed = title.trim();
        if trimmed.is_empty() {
            return None;
        }
        Some(format!("games/ico/{}.png", path_segment(trimmed)))
    }
}
```

- [ ] **Step 5: Register and run**

Add `pub mod sources;` to `src-tauri/src/core/artwork/mod.rs`.

Run: `cd src-tauri && cargo test artwork:: && cargo clippy --all-targets -- -D warnings`
Expected: PASS. (`libretro` is declared in `sources/mod.rs` but does not exist
yet — create `sources/libretro.rs` as an empty file for this step, or move the
`pub mod libretro;` line to Task 5. Prefer the latter.)

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/core/artwork/
git commit -m "feat(artwork): what a source is, and the one that needs no index"
```

---

## Task 5: libretro — the two-step tree index

**Files:**
- Create: `src-tauri/src/core/artwork/sources/libretro.rs`
- Modify: `src-tauri/src/core/artwork/sources/mod.rs` (add `pub mod libretro;`)

**Interfaces:**
- Consumes: `ArtSource`, `SourceIndex` (Task 4); `key::normalise` (Task 2);
  `encode::path_segment` (Task 1).
- Produces: `pub struct Libretro;` plus
  `pub fn subtree_path(sha: &str) -> String` and
  `pub const ROOT_TREE_PATH: &str`.

**Background the implementer needs.** The index is fetched, not guessed: `1000
Miglia` cannot be turned into `1000 Miglia - 1927-1933 Volume 1` by any rule. It
takes **two calls**, and the reason is ART's own validator — `validate_fetch_path`
rejects `?` and `:`, so neither `?recursive=1` nor `trees/master:Named_Boxarts`
is expressible. Verified against the live API:

```
1. repos/libretro-thumbnails/Commodore_-_Amiga/git/trees/master
     -> {"tree":[{"path":"Named_Boxarts","type":"tree","sha":"7a1b0e…"}, …]}
2. repos/libretro-thumbnails/Commodore_-_Amiga/git/trees/7a1b0e…
     -> {"tree":[{"path":"'Allo 'Allo - Cartoon Fun!.png","type":"blob"}, …],
         "truncated":false}
```

Measured: Named_Boxarts 3324, Named_Titles 3434, Named_Snaps 3475, Named_Logos
present. One subtree's JSON is 0.8 MB.

- [ ] **Step 1: Write the failing tests**

Create `src-tauri/src/core/artwork/sources/libretro.rs` with only:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    /// Shaped exactly like the live API's reply, trimmed.
    const ROOT_TREE: &[u8] = br#"{"tree":[
        {"path":".gitignore","type":"blob","sha":"aa4d15"},
        {"path":"Named_Boxarts","type":"tree","sha":"7a1b0e"},
        {"path":"Named_Snaps","type":"tree","sha":"9f65ac"},
        {"path":"Named_Titles","type":"tree","sha":"2d822f"}
    ],"truncated":false}"#;

    const BOXART_TREE: &[u8] = br#"{"tree":[
        {"path":"1869 - Erlebte Geschichte Teil I.png","type":"blob"},
        {"path":"Turrican II.png","type":"blob"},
        {"path":"NotAnImage.txt","type":"blob"}
    ],"truncated":false}"#;

    #[test]
    fn the_root_tree_path_carries_no_colon_and_no_query() {
        assert!(!ROOT_TREE_PATH.contains(':'));
        assert!(!ROOT_TREE_PATH.contains('?'));
        assert!(!ROOT_TREE_PATH.contains(' '));
    }

    #[test]
    fn a_subtree_sha_becomes_a_plain_path() {
        assert_eq!(
            subtree_path("7a1b0e"),
            "repos/libretro-thumbnails/Commodore_-_Amiga/git/trees/7a1b0e"
        );
    }

    /// A sha arrives from a fetched document. It must not be able to become a
    /// path component of its own.
    #[test]
    fn a_hostile_sha_is_refused_rather_than_concatenated() {
        assert_eq!(read_subtree_shas(br#"{"tree":[
            {"path":"Named_Boxarts","type":"tree","sha":"../../etc"}
        ]}"#).unwrap().len(), 0);
    }

    #[test]
    fn only_directories_this_source_knows_are_taken_from_the_root() {
        let shas = read_subtree_shas(ROOT_TREE).unwrap();
        assert_eq!(shas.get(&ArtKind::Boxart).map(String::as_str), Some("7a1b0e"));
        assert_eq!(shas.get(&ArtKind::Snap).map(String::as_str), Some("9f65ac"));
        assert_eq!(shas.get(&ArtKind::Title).map(String::as_str), Some("2d822f"));
        assert_eq!(shas.get(&ArtKind::Logo), None);
    }

    #[test]
    fn a_subtree_becomes_an_index_of_images_only() {
        let mut index = SourceIndex::default();
        Libretro
            .absorb_index(ArtKind::Boxart, BOXART_TREE, &mut index)
            .unwrap();

        let boxarts = index.by_kind.get(&ArtKind::Boxart).unwrap();
        assert_eq!(boxarts.len(), 2, "the .txt must not be indexed");
        assert!(boxarts.contains_key("turrican ii"));
    }

    #[test]
    fn locate_encodes_the_path_it_returns() {
        let mut index = SourceIndex::default();
        Libretro
            .absorb_index(ArtKind::Boxart, BOXART_TREE, &mut index)
            .unwrap();

        let path = Libretro.locate(&index, "1869", ArtKind::Boxart).unwrap();
        assert_eq!(
            path,
            "Named_Boxarts/1869%20-%20Erlebte%20Geschichte%20Teil%20I.png"
        );
        assert!(!path.contains(' '));
    }

    /// A truncated tree is a partial index, and a partial index silently
    /// produces misses that are not misses.
    #[test]
    fn a_truncated_tree_is_an_error_not_a_short_index() {
        let mut index = SourceIndex::default();
        let truncated = br#"{"tree":[{"path":"A.png","type":"blob"}],"truncated":true}"#;
        assert!(Libretro
            .absorb_index(ArtKind::Boxart, truncated, &mut index)
            .is_err());
    }
}
```

- [ ] **Step 2: Run and watch it fail**

Run: `cd src-tauri && cargo test artwork::sources::libretro`
Expected: FAIL — nothing is defined.

- [ ] **Step 3: Implement**

```rust
//! libretro-thumbnails — an index, then only the images that matched.
//!
//! Fetching the index rather than guessing URLs is not an optimisation. A
//! speculative "build the URL and read the 404" strategy cannot work here:
//! `1000 Miglia` does not become `1000 Miglia - 1927-1933 Volume 1` by any rule.
//! It is also the impolite design — 1700 requests, most of them misses — where
//! three index files are 2.5 MB and then only matches are downloaded.
//!
//! Two calls, because `validate_fetch_path` rejects `?` and `:`: neither
//! `?recursive=1` nor the compact `trees/master:Named_Boxarts` form is
//! expressible. The plain root-tree-then-subtree form needs neither.

use std::collections::BTreeMap;

use serde::Deserialize;

use crate::core::artwork::encode::path_segment;
use crate::core::artwork::key::{lookup, normalise};
use crate::core::artwork::sources::{ArtSource, SourceIndex};
use crate::core::artwork::ArtKind;
use crate::core::error::{CoreError, CoreResult};

pub const ROOT_TREE_PATH: &str =
    "repos/libretro-thumbnails/Commodore_-_Amiga/git/trees/master";

/// The repository directory each kind lives in.
const DIRECTORIES: [(ArtKind, &str); 4] = [
    (ArtKind::Boxart, "Named_Boxarts"),
    (ArtKind::Snap, "Named_Snaps"),
    (ArtKind::Title, "Named_Titles"),
    (ArtKind::Logo, "Named_Logos"),
];

const KINDS: [ArtKind; 4] = [ArtKind::Boxart, ArtKind::Snap, ArtKind::Title, ArtKind::Logo];

/// One subtree's JSON was measured at 0.8 MB; four times that is generous and
/// still bounded.
const MAX_INDEX_BYTES: usize = 16 * 1024 * 1024;

#[derive(Debug, Deserialize)]
struct TreeReply {
    tree: Vec<TreeEntry>,
    #[serde(default)]
    truncated: bool,
}

#[derive(Debug, Deserialize)]
struct TreeEntry {
    path: String,
    #[serde(rename = "type")]
    kind: String,
    #[serde(default)]
    sha: String,
}

/// Build the path for one subtree.
///
/// The sha is not interpolated blindly — see [`read_subtree_shas`], which
/// refuses anything that is not a plain hex id.
pub fn subtree_path(sha: &str) -> String {
    format!("repos/libretro-thumbnails/Commodore_-_Amiga/git/trees/{sha}")
}

fn is_plain_sha(sha: &str) -> bool {
    !sha.is_empty() && sha.len() <= 64 && sha.bytes().all(|b| b.is_ascii_hexdigit())
}

/// Read the root tree and return the sha of each directory this source knows.
///
/// A sha arriving from a fetched document is outside input. One that is not
/// plain hex is dropped rather than concatenated into a path.
pub fn read_subtree_shas(bytes: &[u8]) -> CoreResult<BTreeMap<ArtKind, String>> {
    let reply: TreeReply = serde_json::from_slice(bytes)
        .map_err(|err| CoreError::InvalidInput(format!("libretro root tree: {err}")))?;

    let mut found = BTreeMap::new();
    for entry in reply.tree {
        if entry.kind != "tree" || !is_plain_sha(&entry.sha) {
            continue;
        }
        if let Some((kind, _)) = DIRECTORIES.iter().find(|(_, dir)| *dir == entry.path) {
            found.insert(*kind, entry.sha);
        }
    }
    Ok(found)
}

#[derive(Debug, Default, Clone, Copy)]
pub struct Libretro;

impl ArtSource for Libretro {
    fn id(&self) -> &'static str {
        "libretro"
    }

    fn kinds(&self) -> &'static [ArtKind] {
        &KINDS
    }

    /// The root tree only. The per-kind subtrees are not known until it has been
    /// read, so the run fetches the root first and asks again.
    fn index_paths(&self) -> Vec<(ArtKind, String)> {
        vec![(ArtKind::Boxart, ROOT_TREE_PATH.to_string())]
    }

    fn absorb_index(&self, kind: ArtKind, bytes: &[u8], into: &mut SourceIndex) -> CoreResult<()> {
        if bytes.len() > MAX_INDEX_BYTES {
            return Err(CoreError::InvalidInput(
                "libretro index is larger than the allowed bound".into(),
            ));
        }
        let reply: TreeReply = serde_json::from_slice(bytes)
            .map_err(|err| CoreError::InvalidInput(format!("libretro subtree: {err}")))?;

        // A truncated tree is a partial index, and a partial index turns titles
        // that do have pictures into recorded misses.
        if reply.truncated {
            return Err(CoreError::InvalidInput(
                "libretro returned a truncated tree; the index would be incomplete".into(),
            ));
        }

        let directory = DIRECTORIES
            .iter()
            .find(|(art, _)| *art == kind)
            .map(|(_, dir)| *dir)
            .ok_or_else(|| CoreError::InvalidInput("libretro has no directory for that kind".into()))?;

        let map = into.by_kind.entry(kind).or_default();
        for entry in reply.tree {
            if entry.kind != "blob" {
                continue;
            }
            let Some(stem) = entry.path.strip_suffix(".png") else {
                continue;
            };
            map.insert(normalise(stem), format!("{directory}/{}", entry.path));
        }
        Ok(())
    }

    fn locate(&self, index: &SourceIndex, title: &str, kind: ArtKind) -> Option<String> {
        let map = index.by_kind.get(&kind)?;
        let raw = lookup(map, title)?;
        // The stored value is `<dir>/<filename>`; only the filename is encoded.
        let (dir, file) = raw.rsplit_once('/')?;
        Some(format!("{dir}/{}", path_segment(file)))
    }
}
```

- [ ] **Step 4: Register and run**

Add `pub mod libretro;` to `src-tauri/src/core/artwork/sources/mod.rs`.

Run: `cd src-tauri && cargo test artwork:: && cargo clippy --all-targets -- -D warnings`
Expected: PASS (all artwork tests), no warnings.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/core/artwork/
git commit -m "feat(artwork): libretro's index, fetched in two colon-free calls"
```

---

## Task 6: Configured sources and their shipped defaults

**Files:**
- Create: `src-tauri/src/core/artwork/config.rs`
- Modify: `src-tauri/src/core/artwork/mod.rs` (add `pub mod config;`)

**Interfaces:**
- Consumes: `Mirror` (`core::sources::mirror`), `ArtSource` (Task 4).
- Produces: `ConfiguredSource { id, enabled, mirror_base }`,
  `pub fn shipped_defaults() -> Vec<ConfiguredSource>`,
  `pub fn source_for(id: &str) -> Option<Box<dyn ArtSource>>`,
  `pub fn mirror_for(configured: &ConfiguredSource) -> CoreResult<Mirror>`.

- [ ] **Step 1: Write the failing tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    /// The project's decision, recorded as a test so a later change is
    /// deliberate: every source ships enabled.
    #[test]
    fn every_shipped_source_is_enabled_by_default() {
        let defaults = shipped_defaults();
        assert!(!defaults.is_empty());
        assert!(defaults.iter().all(|source| source.enabled));
    }

    #[test]
    fn every_shipped_default_names_a_source_that_exists() {
        for configured in shipped_defaults() {
            assert!(
                source_for(&configured.id).is_some(),
                "no ArtSource for '{}'",
                configured.id
            );
        }
    }

    /// The base is what the user may edit, so it must survive Mirror's
    /// validation as shipped.
    #[test]
    fn every_shipped_base_is_a_valid_mirror() {
        for configured in shipped_defaults() {
            mirror_for(&configured).expect(&configured.id);
        }
    }

    #[test]
    fn a_hostile_mirror_base_is_refused() {
        let bad = ConfiguredSource {
            id: "libretro".into(),
            enabled: true,
            mirror_base: "file:///C:/Windows".into(),
        };
        assert!(mirror_for(&bad).is_err());
    }
}
```

- [ ] **Step 2: Run and watch it fail**

Run: `cd src-tauri && cargo test artwork::config`
Expected: FAIL.

- [ ] **Step 3: Implement**

```rust
//! Which sources ART ships, and the two things a user may change about them.
//!
//! Source *types* are code — their index formats differ and are not expressible
//! as data. What is configurable is small and deliberate (spec §5): **enabled**,
//! which every source ships as, and the **mirror base**, validated by
//! `Mirror::new`.
//!
//! A user may not define a new source from a URL template. That would restore
//! arbitrary-URL fetching and void the guarantee in `core/sources/mirror.rs`
//! that no function anywhere fetches a caller-supplied URL. Adding a source type
//! is a code change, and this project is open source precisely so that stays
//! possible.

use serde::{Deserialize, Serialize};

use crate::core::artwork::sources::{libretro::Libretro, whdload_de::WhdloadDe, ArtSource};
use crate::core::error::CoreResult;
use crate::core::sources::mirror::Mirror;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfiguredSource {
    pub id: String,
    pub enabled: bool,
    pub mirror_base: String,
}

/// What ART ships with. Both enabled: the project's position is that an absent
/// licence is not a blocker for forty-year-old game and demo material, while an
/// absent endpoint is.
pub fn shipped_defaults() -> Vec<ConfiguredSource> {
    vec![
        ConfiguredSource {
            id: "libretro".into(),
            enabled: true,
            mirror_base: "https://api.github.com/".into(),
        },
        ConfiguredSource {
            id: "whdload-de".into(),
            enabled: true,
            mirror_base: "https://www.whdload.de/".into(),
        },
    ]
}

pub fn source_for(id: &str) -> Option<Box<dyn ArtSource>> {
    match id {
        "libretro" => Some(Box::new(Libretro)),
        "whdload-de" => Some(Box::new(WhdloadDe)),
        _ => None,
    }
}

pub fn mirror_for(configured: &ConfiguredSource) -> CoreResult<Mirror> {
    Mirror::new(configured.id.clone(), &configured.mirror_base)
}
```

**Note for the implementer:** libretro's *index* comes from `api.github.com` but
its *images* do not — they are at
`https://raw.githubusercontent.com/libretro-thumbnails/Commodore_-_Amiga/master/`.
Task 7 needs both, so add a second field `image_base: String` to
`ConfiguredSource` with that value for libretro and an empty string for
whdload.de (whose icons are under the same base as everything else), extend
`shipped_defaults`, and add a test that a non-empty `image_base` also validates
as a `Mirror`.

- [ ] **Step 4: Run and commit**

Run: `cd src-tauri && cargo test artwork:: && cargo clippy --all-targets -- -D warnings`

```bash
git add src-tauri/src/core/artwork/
git commit -m "feat(artwork): what ships, and the two things a user may change"
```

---

## Task 7: The run — match, fetch, cache, report

**Files:**
- Create: `src-tauri/src/core/artwork/enrich.rs`
- Modify: `src-tauri/src/core/artwork/mod.rs` (add `pub mod enrich;`)

**Interfaces:**
- Consumes: everything above; `MirrorClient`, `fetch_with_failover` (`core::sources::mirror`);
  `ProgressSink` (`core::jobs`); `Record` (`core::gameindex::record`).
- Produces:
  ```rust
  pub struct EnrichRequest<'a> {
      pub titles: &'a [String],
      pub sources: &'a [ConfiguredSource],
      pub cache_dir: &'a Path,
  }
  pub struct SourceOutcome { pub id: String, pub written: u32, pub matched: u32,
                             pub missed: u32, pub reachable: bool, pub note: Option<String> }
  pub struct EnrichOutcome { pub per_source: Vec<SourceOutcome>, pub cached_before: u32 }
  pub fn enrich(request: EnrichRequest, client: &dyn MirrorClient,
                sink: &dyn ProgressSink) -> CoreResult<EnrichOutcome>;
  ```

**Rules this task must honour, from the spec:**
- At most **4 requests per second per host**, sequential, one connection. A
  `const REQUESTS_PER_SECOND: u32 = 4;` in this file, never a setting.
- `is_cancelled()` is checked **between whole titles**, never mid-write.
- A source that fails is **not** fatal — the run continues and reports
  `reachable: false` with a note.
- A title already in the cache is not fetched; a recorded miss is not re-asked.

- [ ] **Step 1: Write the failing tests, driven by a fake client**

The fake is what keeps the suite off the network:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    #[derive(Default)]
    struct FakeClient {
        /// url -> body
        bodies: BTreeMap<String, Vec<u8>>,
        asked: Mutex<Vec<String>>,
    }

    impl MirrorClient for FakeClient {
        fn fetch(&self, url: &str, _from: u64, out: &mut dyn std::io::Write,
                 _sink: &dyn ProgressSink) -> CoreResult<FetchStats> {
            self.asked.lock().unwrap().push(url.to_string());
            match self.bodies.get(url) {
                Some(body) => { out.write_all(body)?;
                    Ok(FetchStats { written: body.len() as u64, ..Default::default() }) }
                None => Err(CoreError::MirrorUnreachable(format!("404 {url}"))),
            }
        }
    }
```

Write these tests (fill the `bodies` map with the same JSON shapes as Task 5):

1. `a_matched_title_is_written_to_the_cache` — one title, one boxart, assert the
   file exists and `EnrichOutcome.per_source[0].written == 1`.
2. `an_unmatched_title_is_recorded_as_a_miss_not_retried` — run twice with the
   same cache dir; assert the second run asks the client for **zero** image URLs.
3. `a_cached_title_is_not_fetched_again` — same shape, for a hit.
4. `an_unreachable_source_does_not_stop_the_other` — make libretro's root tree
   404; assert whdload.de still wrote its icon and libretro reports
   `reachable: false`.
5. `cancellation_between_titles_returns_cancelled_and_leaves_a_whole_cache` —
   a sink cancelled after the first title; assert `Err(CoreError::Cancelled)`
   and that the first title's file is complete and readable.
6. `a_disabled_source_is_never_asked` — assert the fake was asked no URL
   containing that source's base.

- [ ] **Step 2: Run and watch them fail**

Run: `cd src-tauri && cargo test artwork::enrich`

- [ ] **Step 3: Implement `enrich`**

Shape, in order:

1. `Cache::open(request.cache_dir)`.
2. For each **enabled** source: build its `Mirror` from `mirror_base`; fetch
   `index_paths()`; for libretro, read the root tree with `read_subtree_shas`
   and then fetch each `subtree_path(sha)` and `absorb_index` it. Any failure
   here marks the source `reachable: false` and moves to the next source.
3. For each title, for each of that source's `kinds()`: skip when `cache.get`
   already has it or `cache.is_missing` already recorded it; otherwise
   `locate()`, and when it returns `None`, `record_miss` and continue.
4. Fetch the located path with `fetch_with_failover` into a `Vec<u8>`, then
   `cache.store(...)`. A 404 is `record_miss`, not an error.
5. Sleep to hold the rate: track the last request instant per host and wait when
   fewer than `1000 / REQUESTS_PER_SECOND` ms have passed.
6. `sink.is_cancelled()` between titles → `cache.save()` then
   `Err(CoreError::Cancelled)`.
7. `cache.save()` at the end; return the per-source counts.

Report progress through `sink` as `(done, total)` over titles × enabled sources.

- [ ] **Step 4: Run everything twice**

Run: `cd src-tauri && cargo test && cargo test`
Expected: PASS both times. (The suite has had a race before — ART-059 — so a
single green run is not the bar.)

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/core/artwork/
git commit -m "feat(artwork): the run, and what it refuses to ask twice"
```

---

## Task 8: Commands, the job, and the artwork directory

**Files:**
- Create: `src-tauri/src/commands/artwork.rs`
- Modify: `src-tauri/src/commands/mod.rs`, `src-tauri/src/lib.rs`

**Interfaces:**
- Produces four commands: `artwork_sources_load`, `artwork_sources_save`,
  `artwork_enrich` (spawns a job), `artwork_dir`.

- [ ] **Step 1: Resolve the directory beside the catalogue**

`commands/gameindex.rs` already has:

```rust
fn catalogue_dir(app: &AppHandle) -> PathBuf {
    app.path().app_data_dir().unwrap_or_else(|_| std::env::temp_dir()).join("catalogue")
}
```

Write the sibling — **not** a child (spec §4.4: deleting 1.6 GB of pictures must
not lose the index that took minutes to build):

```rust
fn artwork_dir(app: &AppHandle) -> PathBuf {
    app.path().app_data_dir().unwrap_or_else(|_| std::env::temp_dir()).join("artwork")
}
```

- [ ] **Step 2: Write the commands as thin adapters**

`artwork_sources_load` returns the saved list or `config::shipped_defaults()`
when nothing is saved. `artwork_sources_save` validates each entry through
`config::mirror_for` **before** persisting, so a bad base is refused at the door
rather than at fetch time. `artwork_enrich` collects the titles from the loaded
catalogue and calls `spawn_job` with a closure over `core::artwork::enrich`,
exactly as `gameindex_scan` does — copy its shape.

Persist the source list through the existing settings store, and note the
project rule: a setting the user changed must never change back on its own.

- [ ] **Step 3: Register**

Add all four to `invoke_handler![]` in `lib.rs`. Add any new plugin permission to
`src-tauri/capabilities/default.json`.

- [ ] **Step 4: Verify and commit**

Run: `cd src-tauri && cargo test && cargo clippy --all-targets -- -D warnings`

```bash
git add src-tauri/src/commands/ src-tauri/src/lib.rs src-tauri/capabilities/
git commit -m "feat(artwork): four commands, and a directory that is a sibling"
```

---

## Task 9: The screen — a thumbnail, a button, and a settings panel

**Files:**
- Create: `src/lib/artwork.ts`
- Modify: `src/pages/CollectionStudio.tsx`, `src/pages/Settings.tsx`

- [ ] **Step 1: The typed wrappers**

`src/lib/artwork.ts` mirrors the four commands. Components never call `invoke`
directly. Any message this file builds is a `Phrase`, not a rendered string.

- [ ] **Step 2: Settings — the source list**

One row per source: name, an on/off switch, and an editable mirror base. Save on
change through `artwork_sources_save`; a rejected base shows its error and leaves
the previous value in place. Read through `recall`/`recallInto` with a guard so a
hand-edited settings file falls back to the shipped default rather than putting a
bad value on screen.

- [ ] **Step 3: Collection — the action and the thumbnail**

An **Enrich artwork** button starts the job with progress and cancel, using the
same job UI the scan already uses. **Nothing fetches when the screen opens** —
that is the behaviour wave A removed and it must not come back.

Show the thumbnail in the existing list row, preferring `ArtKind::ALL` order.
The rich screen is wave C; this task adds a picture to the row and nothing more.

- [ ] **Step 4: Verify and commit**

Run: `pnpm lint && pnpm test`

```bash
git add src/
git commit -m "feat(artwork): the sources are settings, and the row has a picture"
```

---

## Task 10: Strings, and the record of what landed

**Files:**
- Modify: `src/i18n/en.json`, `src/i18n/tr.json`, `docs/STATUS.md`,
  `docs/FEATURES.md`, `docs/ISSUES.md`, `CHANGELOG.md`

- [ ] **Step 1: Both catalogues, same commit**

Every key added in Task 9 goes in **both** files. `pnpm test` fails the build if
the key sets differ, a value is empty, or an interpolation variable present in
one is missing from the other.

- [ ] **Step 2: The four documents**

- `docs/STATUS.md` — a session log line, and the snapshot if the counts moved.
- `docs/FEATURES.md` — flip the artwork row **only** because tests exist.
- `docs/ISSUES.md` — an `ART-NNN` for any defect found on the way, with the test
  name that now covers it.
- `CHANGELOG.md` — the user-visible change.

State plainly what is **not** done: chipset, genre and rating are unsourceable
today (spec §1.1, §1.2) and the rich screen is wave C. Do not let a picture in
the list row read as a finished Collection.

- [ ] **Step 3: Full verification, then commit**

```bash
pnpm lint && pnpm test
cd src-tauri && cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test && cargo test
```

```bash
git add src/i18n/ docs/ CHANGELOG.md
git commit -m "docs: artwork lands, and the three fields that still cannot"
```

---

## Self-review notes

**Spec coverage.** §1.1/§1.2 (chipset, genre, rating out of scope) → Task 10
step 2 records it; no task implements them, deliberately. §2.1 whdload.de →
Task 4. §2.2 libretro → Task 5. §3 matching → Task 2. §4 cache → Task 3. §5
configuration → Tasks 6, 8, 9. §5.1 politeness → Task 7. §6 module layout →
Tasks 1–7. §7 error handling → Task 7 tests 4–5. §8 testing → every task. §9
non-goals → Task 10.

**Known gap, stated rather than hidden.** Task 6's note adds `image_base` to
`ConfiguredSource` after its tests are written; Task 7 depends on that field. An
implementer working Task 6 must carry out the note, not only the numbered steps.

**Unmeasured value.** The spec records `Named_Logos` as "present, unmeasured".
Nothing depends on its size; Task 5 indexes it like the others.
