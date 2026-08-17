# Catalogue persistence (SD-2 · G10 wave A) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** The Collection stops re-reading 3.74 GB on every visit: the catalogue
is saved per folder, refreshed only when asked, and the user's own edits survive
every refresh.

**Architecture:** One JSON file per scanned root plus a roots list plus a
separate overrides file, written by `core/gameindex/store.rs` into a directory
the command layer resolves (`core/` cannot know where an app's data lives). A
refresh reuses any cached entry whose path, size and mtime match and whose
record schema is current; the read layer is rewritten, the user layer never is.

**Tech Stack:** Rust (`core/gameindex`, `core/safety`, `core/jobs`),
TypeScript + React (`src/lib/gameindex.ts`, `src/pages/CollectionStudio.tsx`),
i18next (`src/i18n/en.json`, `tr.json`).

**Spec:** [../specs/2026-08-17-catalogue-persistence-design.md](../specs/2026-08-17-catalogue-persistence-design.md)

## Global Constraints

- `core/` is platform-independent: `std` + `serde` + the crates CLAUDE.md
  lists. **No `use tauri`, no network, no launching programs.** In particular
  `core/gameindex/store.rs` **must not resolve the app data directory** — every
  function takes the catalogue directory as a `&Path` and the command layer
  passes it in. The same rule that gives `CardManifest` a caller-supplied
  `built_at` ("core has no clock").
- **A lower-level `core/` module must not import a higher-level one.**
  `core/gameindex` may call `core::safety`, `core::jobs`, `core::hashing`,
  `core::error`. Nothing in those may call back.
- Every string the user reads is a `Phrase { key, params? }` from `src/lib`,
  rendered by the component through `t()`. Rust returns typed values, never
  English sentences (ART-060).
- A key added to `src/i18n/en.json` is added to `tr.json` **in the same
  commit**; `pnpm test` fails the build otherwise.
- **Two write classes, never treated alike.** Root files and `roots.json` are
  derived and use `core::safety::atomic_write`. `overrides.json` is *user data*
  and uses `guarded_write` with `BackupPolicy::CONFIG` (5 generations).
- **Nothing runs automatically.** Opening the Collection screen starts no scan.
  Update and Rescan are explicit, and both are jobs (§54/§55) with a Stop.
- **No refresh ever deletes an entry whose file has gone.** Removing a whole
  root is the only way entries leave the catalogue.
- The user's collection folders stay **read-only to ART**: nothing is created,
  renamed or written under a scanned root.
- New commands go in **both** `invoke_handler![]` in `lib.rs` and a typed
  wrapper in `src/lib/*.ts`. Components never call `invoke` directly.
- Run `cargo test` more than once before calling a task done (ART-059).

## File Structure

| File | Responsibility |
|---|---|
| `src-tauri/src/core/gameindex/record.rs` (modify) | `Provenance` gains `UserEdit` and `rank()`; `is_stated()` stays for the screen's badge |
| `src-tauri/src/core/gameindex/store.rs` (create) | the catalogue: its file shapes, load, refresh, roots, overrides. No I/O paths of its own |
| `src-tauri/src/core/gameindex/mod.rs` (modify) | register `store` |
| `src-tauri/src/commands/gameindex.rs` (modify) | resolves the catalogue directory and the clock; six thin commands |
| `src-tauri/src/lib.rs` (modify) | register them |
| `src/lib/gameindex.ts` (modify) | typed mirrors + wrappers for the six |
| `src/pages/CollectionStudio.tsx` (modify) | loads the saved catalogue, no auto-scan, Update/Rescan, folder list, availability and staleness markers |
| `src/i18n/en.json`, `tr.json` (modify) | the new keys, both files, same commit |

---

## Task 1: `Provenance` gets an order, and a user tier

Four tiers cannot be ordered by a bool. `is_stated()` stays — the screen's
`~guessed` badge really is a two-way question — but *which fact wins* moves to
a rank.

**Files:**
- Modify: `src-tauri/src/core/gameindex/record.rs`

**Interfaces:**
- Consumes: nothing
- Produces: `Provenance::UserEdit`, `Provenance::rank(self) -> u8`

- [ ] **Step 1: Write the failing test**

Add to `record.rs`'s `mod tests`:

```rust
    /// The order the whole catalogue turns on, pinned as a list rather than
    /// asserted pairwise — a pairwise test passes while two variants are
    /// accidentally equal.
    #[test]
    fn the_sources_are_ranked_from_user_edit_down_to_a_drawer_name() {
        let order = [
            Provenance::UserEdit,
            Provenance::Rp9Manifest,
            Provenance::TosecName,
            Provenance::DrawerName,
        ];
        for pair in order.windows(2) {
            assert!(
                pair[0].rank() >= pair[1].rank(),
                "{:?} must not rank below {:?}",
                pair[0],
                pair[1]
            );
        }
        // A user edit outranks everything, and the two declarations tie: an
        // .rp9 manifest and a slave header are both the packager writing it
        // down, and nothing here is entitled to prefer one.
        assert!(Provenance::UserEdit.rank() > Provenance::Rp9Manifest.rank());
        assert_eq!(
            Provenance::Rp9Manifest.rank(),
            Provenance::WhdloadSlave.rank()
        );
        assert!(Provenance::WhdloadSlave.rank() > Provenance::TosecName.rank());
        assert!(Provenance::TosecName.rank() > Provenance::DrawerName.rank());
    }

    /// `is_stated` and `rank` must not drift apart: anything that states a fact
    /// outranks anything that merely suggests one.
    #[test]
    fn every_stated_source_outranks_every_guessed_one() {
        let all = [
            Provenance::Rp9Manifest,
            Provenance::WhdloadSlave,
            Provenance::TosecName,
            Provenance::DrawerName,
            Provenance::UserEdit,
        ];
        for stated in all.iter().filter(|p| p.is_stated()) {
            for guessed in all.iter().filter(|p| !p.is_stated() && **p != Provenance::UserEdit) {
                assert!(
                    stated.rank() > guessed.rank(),
                    "{stated:?} vs {guessed:?}"
                );
            }
        }
    }

    /// A user edit is not a *declaration by the packager*, so the badge does not
    /// call it one — but it still wins. The two questions are different and the
    /// two methods answer them differently on purpose.
    #[test]
    fn a_user_edit_is_not_stated_but_still_wins() {
        assert!(!Provenance::UserEdit.is_stated());
        assert_eq!(
            Provenance::UserEdit.rank(),
            *[
                Provenance::Rp9Manifest,
                Provenance::WhdloadSlave,
                Provenance::TosecName,
                Provenance::DrawerName,
                Provenance::UserEdit,
            ]
            .iter()
            .map(|p| p.rank())
            .max()
            .as_ref()
            .unwrap()
        );
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cd src-tauri && cargo test gameindex::record::tests::the_sources_are_ranked`
Expected: FAIL — `no variant named UserEdit`, `no method named rank`.

- [ ] **Step 3: Add the variant and the rank**

In `record.rs`, add to `Provenance` after `DrawerName`:

```rust
    /// The user typed it. Outranks everything, including a declaration —
    /// *nothing changes unless the user changes it* cuts both ways, and a
    /// correction they made must survive every later refresh.
    UserEdit,
```

and to its `impl`:

```rust
    /// Which fact wins when two sources answer the same question.
    ///
    /// **A bool cannot order four tiers.** [`is_stated`](Self::is_stated) stays
    /// because the screen's badge asks a two-way question — did anybody declare
    /// this, or was it guessed — but the precedence the catalogue turns on is
    /// this:
    ///
    /// ```text
    /// 4  the user typed it
    /// 3  the packager declared it   (.rp9 manifest · WHDLoad slave header)
    /// 2  reserved for a third-party database (wave B)
    /// 1  a TOSEC-shaped filename
    /// 0  the name of the drawer a slave happens to sit in
    /// ```
    ///
    /// **2 is deliberately empty.** Wave B fetches facts from a source the user
    /// configures, and a stranger's guess does not outrank what the packager
    /// wrote down — leaving the gap here means B adds a variant rather than
    /// renumbering the tiers and re-reading every comparison that depends on
    /// them.
    pub fn rank(self) -> u8 {
        match self {
            Self::UserEdit => 4,
            Self::Rp9Manifest | Self::WhdloadSlave => 3,
            Self::TosecName => 1,
            Self::DrawerName => 0,
        }
    }
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cd src-tauri && cargo test gameindex::record`
Expected: PASS, 8 tests.

- [ ] **Step 5: Mutation-check the order**

Change `Self::TosecName => 1` to `=> 3` and re-run.
Expected: `the_sources_are_ranked_from_user_edit_down_to_a_drawer_name` and
`every_stated_source_outranks_every_guessed_one` both FAIL. Restore and confirm
they pass. An ordering test that passes with the order broken is not testing
the order.

- [ ] **Step 6: Commit**

```bash
cd src-tauri && cargo fmt && cargo clippy --all-targets -- -D warnings && cargo test
git add src-tauri/src/core/gameindex/record.rs
git commit -m "feat(gameindex): rank the sources, and give the user the top tier"
```

---

## Task 2: The catalogue's files, written and read back

**Files:**
- Create: `src-tauri/src/core/gameindex/store.rs`
- Modify: `src-tauri/src/core/gameindex/mod.rs`

**Interfaces:**
- Consumes: `record::{GameRecord, ChipsetRequirement, GAMEINDEX_SCHEMA}`,
  `core::safety::{atomic_write, guarded_write}`,
  `core::safety::backup::BackupPolicy`, `core::hashing::sha256_bytes`
- Produces:
  - `pub const CATALOGUE_SCHEMA: u32`
  - `pub struct CachedEntry { pub path: String, pub size: u64, pub mtime_ms: i64, pub record: GameRecord }`
  - `pub struct CatalogueRoot { pub schema: u32, pub root: String, pub scanned_at: Option<String>, pub index_schema: u32, pub entries: Vec<CachedEntry> }`
  - `pub struct RootsFile { pub schema: u32, pub roots: Vec<String> }`
  - `pub struct RecordOverride { pub title: Option<String>, pub year: Option<u16>, pub publisher: Option<String>, pub genre: Option<String>, pub chipset: Option<ChipsetRequirement> }`
  - `pub struct Overrides { pub schema: u32, pub edits: BTreeMap<String, RecordOverride> }`
  - `pub fn root_file_name(root: &Path) -> String`
  - `pub fn read_roots(dir: &Path) -> CoreResult<RootsFile>`
  - `pub fn write_roots(dir: &Path, roots: &RootsFile) -> CoreResult<()>`
  - `pub fn read_root(dir: &Path, root: &Path) -> CoreResult<Option<CatalogueRoot>>`
  - `pub fn write_root(dir: &Path, value: &CatalogueRoot) -> CoreResult<()>`
  - `pub fn read_overrides(dir: &Path) -> CoreResult<Overrides>`
  - `pub fn write_overrides(dir: &Path, value: &Overrides) -> CoreResult<Option<PathBuf>>`

- [ ] **Step 1: Write the failing test**

Create `src-tauri/src/core/gameindex/store.rs` with only this test module:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::gameindex::record::{
        Fact, Media, Provenance, SourceRef, GAMEINDEX_SCHEMA,
    };

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

    /// A root ART has never scanned is `None`, not an error. An empty
    /// catalogue is the normal first-run state.
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

        assert_eq!(read_roots(&dir).unwrap().roots, vec![r"E:\amiga".to_string()]);
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
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cd src-tauri && cargo test gameindex::store`
Expected: FAIL — the module is not registered and none of the types exist.

- [ ] **Step 3: Register the module**

In `src-tauri/src/core/gameindex/mod.rs`, add `pub mod store;` keeping the list
alphabetical:

```rust
pub mod readers;
pub mod record;
pub mod scan;
pub mod store;
```

- [ ] **Step 4: Write the shapes and the I/O**

At the top of `store.rs`, above the test module:

```rust
//! The catalogue on disk (SD-2 · G10 wave A).
//!
//! One file per scanned root, a list of the roots, and the user's own edits
//! kept apart from both. The point of the split is that a refresh rewrites the
//! read layer and **never** the user layer: any other arrangement means every
//! scan destroys what the user corrected by hand.
//!
//! **This module does not know where it writes.** The catalogue directory
//! arrives as a `&Path` from `commands/`, because `core/` is
//! platform-independent and `%APPDATA%` is not — the same rule that gives
//! `CardManifest` a caller-supplied `built_at`.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::core::error::{CoreError, CoreResult};
use crate::core::gameindex::record::{ChipsetRequirement, GameRecord, GAMEINDEX_SCHEMA};
use crate::core::hashing::sha256_bytes;
use crate::core::safety::backup::BackupPolicy;
use crate::core::safety::{atomic_write, guarded_write};

/// The catalogue **file format**'s version.
///
/// Distinct from [`GAMEINDEX_SCHEMA`], which versions a *record*. They move for
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
    /// The value of [`GAMEINDEX_SCHEMA`] when this root was last read. What
    /// makes a reader fix land: an entry read by an older reader is re-read by
    /// the next update even when its path, size and mtime all match.
    pub index_schema: u32,
    pub entries: Vec<CachedEntry>,
}

/// Which roots are catalogued, in the order the user put them in.
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
/// to see what they corrected, and it is small enough that the indentation
/// costs nothing worth counting.
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
```

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cd src-tauri && cargo test gameindex::store`
Expected: PASS, 7 tests.

- [ ] **Step 6: Commit**

```bash
cd src-tauri && cargo fmt && cargo clippy --all-targets -- -D warnings && cargo test
git add src-tauri/src/core/gameindex/
git commit -m "feat(gameindex): the catalogue's files, and the two write classes kept apart"
```

---

## Task 3: Update and Rescan

The task the whole design exists for. Update reuses a cached entry when its
path, size and mtime match **and** its record was read by the current reader.

**Files:**
- Modify: `src-tauri/src/core/gameindex/store.rs`
- Modify: `src-tauri/src/core/gameindex/scan.rs` (expose `read_one` and the
  walk to the store)

**Interfaces:**
- Consumes: `scan::{collect_indexable, read_one}`, `core::jobs::ProgressSink`
- Produces:
  - `pub enum Refresh { Update, Rescan }`
  - `pub fn refresh_root(dir: &Path, root: &Path, mode: Refresh, scanned_at: Option<String>, progress: &dyn ProgressSink) -> CoreResult<CatalogueRoot>`
  - `pub fn file_key(path: &Path) -> Option<(u64, i64)>`

- [ ] **Step 1: Make the walk and the reader reachable**

In `scan.rs`, change the two private items the store needs:

```rust
/// Every indexable file under `dir`, sorted, depth-limited, symlinks skipped.
///
/// `pub(crate)` because `store::refresh_root` walks the same way — two walks
/// with two depth limits is two things to keep in step.
pub(crate) fn collect_indexable(dir: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    collect(dir, &mut files, 0);
    files.retain(|path| {
        path.extension()
            .map(|e| e.to_string_lossy().to_ascii_lowercase())
            .is_some_and(|e| matches!(e.as_str(), "rp9" | "hdf" | "img" | "adf" | "adz"))
    });
    files.sort();
    files
}
```

and make `read_one` `pub(crate)` with its doc comment unchanged. Then rewrite
`scan_titles_with`'s body to use `collect_indexable` so there is exactly one
walk:

```rust
    progress.report(0, None, "Looking for titles…");
    let files = collect_indexable(dir);
```

(The extension filter moves out of `read_one`'s head into the walk; `read_one`
keeps its own `match` on the extension because it still needs to know *which*
reader to use.)

- [ ] **Step 2: Run the existing suite to confirm the move changed nothing**

Run: `cd src-tauri && cargo test gameindex`
Expected: PASS, unchanged counts. A refactor that changes behaviour is not a
refactor.

- [ ] **Step 3: Write the failing tests**

Add to `store.rs`'s `mod tests`:

```rust
    use crate::core::jobs::NoProgress;

    /// Build a real, readable `.adf` under `dir` and return its path.
    fn a_real_file(dir: &Path, name: &str) -> PathBuf {
        let path = dir.join(name);
        std::fs::write(&path, format!("pretend this is {name}")).unwrap();
        path
    }

    /// **The cache really skips the read.**
    ///
    /// A cached entry is planted whose `record` says `SENTINEL` — a title the
    /// file's own name could never produce — with the file's real size and
    /// mtime. If Update returns `SENTINEL`, the file was not opened. An
    /// implementation that reads anyway cannot pass this, and no clock or
    /// mtime trickery is needed to prove it.
    #[test]
    fn an_unchanged_file_is_not_read_again() {
        let dir = scratch("cache-hit");
        let root = dir.join("library");
        std::fs::create_dir_all(&root).unwrap();
        let file = a_real_file(&root, "Zool (1992)(Gremlin).adf");
        let (size, mtime_ms) = file_key(&file).unwrap();

        write_root(
            &dir,
            &CatalogueRoot {
                schema: CATALOGUE_SCHEMA,
                root: root.to_string_lossy().into(),
                scanned_at: Some("2026-08-17T12:00:00Z".into()),
                index_schema: GAMEINDEX_SCHEMA,
                entries: vec![CachedEntry {
                    path: file.to_string_lossy().into(),
                    size,
                    mtime_ms,
                    record: a_record("SENTINEL"),
                }],
            },
        )
        .unwrap();

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

        write_root(
            &dir,
            &CatalogueRoot {
                schema: CATALOGUE_SCHEMA,
                root: root.to_string_lossy().into(),
                scanned_at: None,
                index_schema: GAMEINDEX_SCHEMA - 1,
                entries: vec![CachedEntry {
                    path: file.to_string_lossy().into(),
                    size,
                    mtime_ms,
                    record: stale,
                }],
            },
        )
        .unwrap();

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

        write_root(
            &dir,
            &CatalogueRoot {
                schema: CATALOGUE_SCHEMA,
                root: root.to_string_lossy().into(),
                scanned_at: None,
                index_schema: GAMEINDEX_SCHEMA,
                entries: vec![CachedEntry {
                    path: file.to_string_lossy().into(),
                    size: 1,
                    mtime_ms,
                    record: a_record("SENTINEL"),
                }],
            },
        )
        .unwrap();

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

        write_root(
            &dir,
            &CatalogueRoot {
                schema: CATALOGUE_SCHEMA,
                root: root.to_string_lossy().into(),
                scanned_at: None,
                index_schema: GAMEINDEX_SCHEMA,
                entries: vec![CachedEntry {
                    path: file.to_string_lossy().into(),
                    size,
                    mtime_ms,
                    record: a_record("SENTINEL"),
                }],
            },
        )
        .unwrap();

        let after = refresh_root(&dir, &root, Refresh::Rescan, None, &NoProgress).unwrap();
        assert_eq!(after.entries[0].record.title.value, "Zool");

        std::fs::remove_dir_all(&dir).ok();
    }

    /// **A file that has gone keeps its entry** — through an Update *and*
    /// through a Rescan. A catalogue is a library, not a mirror of the disk,
    /// and an unplugged drive must not delete it.
    #[test]
    fn an_entry_whose_file_has_gone_is_kept_by_both_modes() {
        for mode in [Refresh::Update, Refresh::Rescan] {
            let dir = scratch("missing");
            let root = dir.join("library");
            std::fs::create_dir_all(&root).unwrap();

            write_root(
                &dir,
                &CatalogueRoot {
                    schema: CATALOGUE_SCHEMA,
                    root: root.to_string_lossy().into(),
                    scanned_at: None,
                    index_schema: GAMEINDEX_SCHEMA,
                    entries: vec![CachedEntry {
                        path: root.join("Gone.adf").to_string_lossy().into(),
                        size: 100,
                        mtime_ms: 1,
                        record: a_record("Gone Game"),
                    }],
                },
            )
            .unwrap();

            let after = refresh_root(&dir, &root, mode, None, &NoProgress).unwrap();
            assert_eq!(after.entries.len(), 1, "{mode:?} dropped a missing entry");
            assert_eq!(after.entries[0].record.title.value, "Gone Game");

            std::fs::remove_dir_all(&dir).ok();
        }
    }

    /// A refresh reports what it will actually read, not the file count. Three
    /// changed files out of 1699 is "3", which is both the honest number and
    /// the reassuring one.
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

        write_root(
            &dir,
            &CatalogueRoot {
                schema: CATALOGUE_SCHEMA,
                root: root.to_string_lossy().into(),
                scanned_at: None,
                index_schema: GAMEINDEX_SCHEMA,
                entries: vec![CachedEntry {
                    path: cached.to_string_lossy().into(),
                    size,
                    mtime_ms,
                    record: a_record("SENTINEL"),
                }],
            },
        )
        .unwrap();

        let sink = Recorder::default();
        refresh_root(&dir, &root, Refresh::Update, None, &sink).unwrap();

        let totals = sink.totals.lock().unwrap();
        assert!(
            totals.iter().any(|t| *t == Some(1)),
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
```

- [ ] **Step 4: Run the tests to verify they fail**

Run: `cd src-tauri && cargo test gameindex::store`
Expected: FAIL — `Refresh`, `refresh_root` and `file_key` do not exist.

- [ ] **Step 5: Write the refresh**

Add to `store.rs`:

```rust
/// Which entries a refresh is willing to trust.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refresh {
    /// Trust a cached entry whose file looks unchanged and whose record was
    /// read by the current reader.
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

    // Decide what needs reading *before* reading anything, so the total the
    // user sees is the work that is actually left.
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

    let mut entries: Vec<CachedEntry> =
        reuse.into_iter().chain(fresh).chain(missing).collect();
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
```

- [ ] **Step 6: Run the tests to verify they pass**

Run: `cd src-tauri && cargo test gameindex::store`
Expected: PASS, 14 tests.

- [ ] **Step 7: Mutation-check the cache**

Delete `&& entry.record.schema == GAMEINDEX_SCHEMA` from the `filter` and
re-run. Expected: `a_record_from_an_older_reader_is_read_again` FAILS. Restore
it. Then delete the whole `.filter(...)` and re-run: expected
`a_changed_file_is_read_again` FAILS. Restore. A cache test that passes with
the cache condition removed is testing nothing.

- [ ] **Step 8: Commit**

```bash
cd src-tauri && cargo fmt && cargo clippy --all-targets -- -D warnings && cargo test && cargo test
git add src-tauri/src/core/gameindex/
git commit -m "feat(gameindex): update reuses what it can, rescan trusts nothing present"
```

---

## Task 4: The user layer, applied on load

**Files:**
- Modify: `src-tauri/src/core/gameindex/store.rs`

**Interfaces:**
- Consumes: `record::{Fact, Provenance}`
- Produces:
  - `pub struct EntryView { pub path: String, pub available: bool, pub record: GameRecord }`
  - `pub struct RootView { pub root: String, pub scanned_at: Option<String>, pub stale: bool, pub entries: Vec<EntryView> }`
  - `pub fn load(dir: &Path) -> CoreResult<Vec<RootView>>`
  - `pub fn apply_override(record: &mut GameRecord, edit: &RecordOverride)`
  - `pub fn set_override(dir: &Path, id: &str, edit: RecordOverride) -> CoreResult<Option<PathBuf>>`

- [ ] **Step 1: Write the failing tests**

Add to `store.rs`'s `mod tests`:

```rust
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

    /// **Overrides survive a rescan.** The whole reason the layers are apart:
    /// a refresh rewrites the read layer, and the user's correction is still
    /// there afterwards.
    #[test]
    fn overrides_survive_a_rescan() {
        let dir = scratch("survive");
        let root = dir.join("library");
        std::fs::create_dir_all(&root).unwrap();
        a_real_file(&root, "Zool (1992)(Gremlin).adf");

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
    /// was read; whether the file is there *now* is asked of the disk each
    /// time, so a catalogue cannot claim "ready" about a game on an unplugged
    /// drive.
    #[test]
    fn availability_comes_from_the_disk_not_the_file() {
        let dir = scratch("availability");
        let root = dir.join("library");
        std::fs::create_dir_all(&root).unwrap();
        let file = a_real_file(&root, "Zool (1992)(Gremlin).adf");

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
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cd src-tauri && cargo test gameindex::store`
Expected: FAIL — `apply_override`, `set_override`, `load`, `EntryView` and
`RootView` do not exist.

- [ ] **Step 3: Write the user layer**

Add to `store.rs`:

```rust
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
/// [`Provenance::UserEdit`], which outranks everything. A field the user did
/// not touch is left exactly as it was read, provenance included.
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
    let overrides = read_overrides(dir)?;
    let mut out = Vec::new();

    for root in read_roots(dir)?.roots {
        let Some(stored) = read_root(dir, Path::new(&root))? else {
            // Listed but never scanned: an empty root rather than a refusal,
            // so adding a folder and not scanning it yet is a normal state.
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
/// An override with nothing in it is **removed** rather than stored, so
/// changing one's mind leaves no trace. Returns where the previous overrides
/// were backed up, which the command surfaces.
pub fn set_override(
    dir: &Path,
    id: &str,
    edit: RecordOverride,
) -> CoreResult<Option<PathBuf>> {
    let mut overrides = read_overrides(dir)?;
    if edit.is_empty() {
        overrides.edits.remove(id);
    } else {
        overrides.edits.insert(id.to_string(), edit);
    }
    write_overrides(dir, &overrides)
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cd src-tauri && cargo test gameindex::store`
Expected: PASS, 20 tests.

- [ ] **Step 4b: The ten-thousand-entry measurement**

The user asked whether this shape survives 10 000 games, and whether SQLite
would be the better answer. The spec's §4 argues it from measured sizes; this is
the test that keeps the argument honest as the code changes.

Add to `mod tests`:

```rust
    /// **Ten thousand entries, loaded.**
    ///
    /// The question this answers is not "is JSON fast" — it is "did anything
    /// here become quadratic". A load that walks the overrides map per entry,
    /// or re-reads a file per entry, turns 10 000 into minutes; the ceiling
    /// below is loose enough never to flake on a slow machine and tight enough
    /// to catch that.
    ///
    /// The timing is **printed**, so the real number is in the log whether or
    /// not the assert is near it:
    ///
    /// ```text
    /// cargo test ten_thousand -- --nocapture
    /// ```
    ///
    /// Measured shapes it is checked against (spec §4): one entry is 824 bytes
    /// compact, so 10 000 is about 8 MB.
    #[test]
    fn ten_thousand_entries_load_without_going_quadratic() {
        let dir = scratch("ten-thousand");
        let root = dir.join("library");
        std::fs::create_dir_all(&root).unwrap();

        // Entries only; the files themselves are not created. That is the point
        // — `load` must not open a title to list it, and every one of these
        // will come back marked unavailable.
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

        let bytes = std::fs::metadata(dir.join(root_file_name(&root))).unwrap().len();
        println!(
            "10 000 entries: {bytes} bytes on disk, wrote in {wrote:?}, loaded in {read:?}"
        );

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
```

Run: `cd src-tauri && cargo test ten_thousand -- --nocapture`
Expected: PASS, and **write the printed numbers into the task's commit message**
— they are the measurement the spec's §4 promises, and a number nobody recorded
is a number nobody can compare against later.

- [ ] **Step 5: Mutation-check that the layers really are apart**

In `refresh_root`, make the written `CatalogueRoot` also clear the overrides
file (`write_overrides(dir, &Overrides::default())?`) and re-run. Expected:
`overrides_survive_a_rescan` FAILS. Remove the line again. The test must fail
when a refresh touches the user layer, or it is not guarding anything.

- [ ] **Step 6: Commit**

```bash
cd src-tauri && cargo fmt && cargo clippy --all-targets -- -D warnings && cargo test && cargo test
git add src-tauri/src/core/gameindex/
git commit -m "feat(gameindex): the user layer wins, and survives every refresh"
```

---

## Task 5: Adding and removing roots

**Files:**
- Modify: `src-tauri/src/core/gameindex/store.rs`

**Interfaces:**
- Produces:
  - `pub fn add_root(dir: &Path, root: &Path) -> CoreResult<()>`
  - `pub fn remove_root(dir: &Path, root: &Path) -> CoreResult<()>`

- [ ] **Step 1: Write the failing tests**

```rust
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
            vec![b.to_string_lossy().to_string(), a.to_string_lossy().to_string()],
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
    /// does not sit in the list forever producing nothing.
    #[test]
    fn a_root_that_is_not_a_directory_is_refused() {
        let dir = scratch("bad-root");
        let file = a_real_file(&dir, "not-a-folder.adf");
        assert!(add_root(&dir, &file).is_err());
        std::fs::remove_dir_all(&dir).ok();
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cd src-tauri && cargo test gameindex::store::tests::roots`
Expected: FAIL — `add_root` and `remove_root` do not exist.

- [ ] **Step 3: Write them**

```rust
/// Add a folder to the catalogue. Nothing is scanned; that is a separate ask.
///
/// A path that is not a directory is refused rather than listed: a typo left
/// in the list would produce an empty root for ever and look like a folder
/// with no games in it.
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
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cd src-tauri && cargo test gameindex::store`
Expected: PASS, 24 tests.

- [ ] **Step 5: Commit**

```bash
cd src-tauri && cargo fmt && cargo clippy --all-targets -- -D warnings && cargo test && cargo test
git add src-tauri/src/core/gameindex/
git commit -m "feat(gameindex): roots are added, removed, and take their entries with them"
```

---

## Task 6: The commands, and the typed wrappers

**Files:**
- Modify: `src-tauri/src/commands/gameindex.rs`
- Modify: `src-tauri/src/lib.rs`
- Modify: `src/lib/gameindex.ts`

**Interfaces:**
- Consumes: `core::gameindex::store::*`
- Produces, in `src/lib/gameindex.ts`:
  - `catalogueLoad(): Promise<RootView[]>`
  - `catalogueAddRoot(root: string): Promise<void>`
  - `catalogueRemoveRoot(root: string): Promise<void>`
  - `catalogueRefresh(root: string, mode: "update" | "rescan"): Promise<number>`
  - `catalogueSetOverride(id: string, edit: RecordOverride): Promise<string | null>`
  - types `RootView`, `EntryView`, `RecordOverride`

- [ ] **Step 1: Resolve the catalogue directory in one place**

Add to `commands/gameindex.rs`:

```rust
/// Where the catalogue lives.
///
/// **Resolved here and nowhere else.** `core/gameindex/store` takes the
/// directory as an argument because `core/` is platform-independent and
/// `%APPDATA%` is not. The temp-directory fallback is the same one `lib.rs`
/// uses for the software catalog, and for the same reason: a catalogue ART
/// cannot place is recoverable with one rescan, refusing to start is not.
fn catalogue_dir(app: &AppHandle) -> PathBuf {
    app.path()
        .app_data_dir()
        .unwrap_or_else(|_| std::env::temp_dir())
        .join("catalogue")
}

/// An ISO-8601 timestamp for `scanned_at`. `core` has no clock, so the command
/// layer supplies one — the same split `CardManifest::built_at` uses.
fn now_iso() -> Option<String> {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now().duration_since(UNIX_EPOCH).ok()?.as_secs();
    Some(format!("{secs}"))
}
```

Add `use std::path::PathBuf;` and `use tauri::Manager;` to the imports if they
are not already there.

- [ ] **Step 2: Write the five commands**

```rust
#[tauri::command]
pub fn catalogue_load(app: AppHandle) -> AppResult<Vec<store::RootView>> {
    Ok(store::load(&catalogue_dir(&app))?)
}

#[tauri::command]
pub fn catalogue_add_root(root: String, app: AppHandle) -> AppResult<()> {
    Ok(store::add_root(&catalogue_dir(&app), Path::new(&root))?)
}

#[tauri::command]
pub fn catalogue_remove_root(root: String, app: AppHandle) -> AppResult<()> {
    Ok(store::remove_root(&catalogue_dir(&app), Path::new(&root))?)
}

/// Refresh one root on a job. `mode` is `"update"` or `"rescan"`; anything else
/// is refused rather than guessed at.
#[tauri::command]
pub fn catalogue_refresh(
    root: String,
    mode: String,
    app: AppHandle,
    registry: State<'_, Arc<JobRegistry>>,
) -> AppResult<JobId> {
    let refresh = match mode.as_str() {
        "update" => store::Refresh::Update,
        "rescan" => store::Refresh::Rescan,
        other => {
            return Err(crate::error::AppError::from(
                crate::core::error::CoreError::InvalidInput(format!(
                    "'{other}' is not a refresh mode"
                )),
            ))
        }
    };

    let dir = catalogue_dir(&app);
    let root_path = PathBuf::from(&root);
    let registry = Arc::clone(&registry);
    let emit_app = app.clone();
    let stamped = now_iso();

    let id = spawn_job(
        &app,
        registry,
        "Refreshing the catalogue",
        move |job_id, progress| {
            store::refresh_root(&dir, &root_path, refresh, stamped, progress)?;
            // The screen reloads the whole catalogue rather than patching one
            // root: the user layer and availability both apply across roots,
            // and one reload is cheaper than keeping two views in step.
            let _ = emit_app.emit(REFRESHED_EVENT, RefreshedRoot { job_id, root });
            Ok(())
        },
    );

    Ok(id)
}

#[tauri::command]
pub fn catalogue_set_override(
    id: String,
    edit: store::RecordOverride,
    app: AppHandle,
) -> AppResult<Option<String>> {
    let backup = store::set_override(&catalogue_dir(&app), &id, edit)?;
    Ok(backup.map(|path| path.to_string_lossy().to_string()))
}
```

and the event type beside `IndexResult`:

```rust
/// Emitted when a root has been refreshed. The screen reloads on it.
pub const REFRESHED_EVENT: &str = "catalogue-refreshed";

#[derive(Debug, Clone, Serialize)]
pub struct RefreshedRoot {
    pub job_id: JobId,
    pub root: String,
}
```

- [ ] **Step 3: Register them**

In `lib.rs`'s `invoke_handler![]`, beside `commands::gameindex::gameindex_scan`:

```rust
            commands::gameindex::catalogue_load,
            commands::gameindex::catalogue_add_root,
            commands::gameindex::catalogue_remove_root,
            commands::gameindex::catalogue_refresh,
            commands::gameindex::catalogue_set_override,
```

- [ ] **Step 4: Compile and check**

Run: `cd src-tauri && cargo clippy --all-targets -- -D warnings`
Expected: clean. `store::RecordOverride` must derive `Deserialize` for the
command argument — it already does from Task 2.

- [ ] **Step 5: Write the frontend test first**

Create `src/lib/catalogue.test.ts`:

```ts
import { describe, expect, it } from "vitest";

import { isRefreshMode, type RefreshMode } from "./gameindex";

describe("isRefreshMode", () => {
  /// The two words Rust will accept, and nothing else. A third string sent
  /// from the screen is refused by the command; this stops it being sent.
  it("accepts exactly the two modes the command knows", () => {
    expect(isRefreshMode("update")).toBe(true);
    expect(isRefreshMode("rescan")).toBe(true);
    expect(isRefreshMode("full")).toBe(false);
    expect(isRefreshMode("")).toBe(false);
  });

  it("narrows the type", () => {
    const raw: string = "rescan";
    if (isRefreshMode(raw)) {
      const mode: RefreshMode = raw;
      expect(mode).toBe("rescan");
    }
  });
});
```

Run: `pnpm vitest run src/lib/catalogue.test.ts`
Expected: FAIL — `isRefreshMode` is not exported.

- [ ] **Step 6: Write the wrappers**

Add to `src/lib/gameindex.ts`:

```ts
/** How much a refresh is willing to trust. Mirrors `store::Refresh`. */
export type RefreshMode = "update" | "rescan";

export function isRefreshMode(value: string): value is RefreshMode {
  return value === "update" || value === "rescan";
}

/** One title's hand corrections. Absent means "no opinion". */
export interface RecordOverride {
  title: string | null;
  year: number | null;
  publisher: string | null;
  genre: string | null;
  chipset: ChipsetRequirement | null;
}

/** An empty override, which `catalogueSetOverride` treats as "forget my edits". */
export const NO_OVERRIDE: RecordOverride = {
  title: null,
  year: null,
  publisher: null,
  genre: null,
  chipset: null,
};

export interface EntryView {
  path: string;
  available: boolean;
  record: GameRecord;
}

export interface RootView {
  root: string;
  scanned_at: string | null;
  /** Read by an older reader; an update would improve these entries. */
  stale: boolean;
  entries: EntryView[];
}

/** Load the saved catalogue. Starts no scan. */
export async function catalogueLoad(): Promise<RootView[]> {
  return invoke<RootView[]>("catalogue_load");
}

export async function catalogueAddRoot(root: string): Promise<void> {
  return invoke<void>("catalogue_add_root", { root });
}

export async function catalogueRemoveRoot(root: string): Promise<void> {
  return invoke<void>("catalogue_remove_root", { root });
}

/** Refresh one root. Returns a job id; watch it through `@/lib/jobs`. */
export async function catalogueRefresh(
  root: string,
  mode: RefreshMode
): Promise<number> {
  return invoke<number>("catalogue_refresh", { root, mode });
}

/** Record or clear one title's corrections. Returns where the backup went. */
export async function catalogueSetOverride(
  id: string,
  edit: RecordOverride
): Promise<string | null> {
  return invoke<string | null>("catalogue_set_override", { id, edit });
}

export const REFRESHED_EVENT = "catalogue-refreshed";

export interface RefreshedRoot {
  job_id: number;
  root: string;
}

/** Subscribe to finished refreshes. Returns an unlisten function. */
export async function onCatalogueRefreshed(
  handler: (result: RefreshedRoot) => void
): Promise<() => void> {
  const { listen } = await import("@tauri-apps/api/event");
  return listen<RefreshedRoot>(REFRESHED_EVENT, (e) => handler(e.payload));
}
```

- [ ] **Step 7: Run the frontend tests**

Run: `pnpm vitest run src/lib/catalogue.test.ts && pnpm lint`
Expected: PASS, lint clean.

- [ ] **Step 8: Commit**

```bash
pnpm lint && pnpm test
cd src-tauri && cargo test && cargo clippy --all-targets -- -D warnings
git add src-tauri/src/commands/ src-tauri/src/lib.rs src/lib/gameindex.ts src/lib/catalogue.test.ts
git commit -m "feat(catalogue): five commands, and the directory resolved in one place"
```

---

## Task 7: The screen loads the saved catalogue

**Files:**
- Modify: `src/pages/CollectionStudio.tsx`
- Modify: `src/i18n/en.json`, `src/i18n/tr.json`

- [ ] **Step 1: Add the i18n keys to both catalogues**

Add under `gameindex` in **both** files, same edit. English:

```json
    "catalogue": {
      "folders": "Folders",
      "addFolder": "Add folder…",
      "removeFolder": "Remove",
      "removeConfirm": "Remove {{root}} from the catalogue? Its titles go with it; the files themselves are untouched.",
      "update": "Update",
      "updateHint": "Reads only what has changed since the last scan.",
      "rescan": "Rescan",
      "rescanHint": "Reads every file again. Slow; use it when you do not trust the catalogue.",
      "never": "never scanned",
      "scannedAt": "last scanned {{when}}",
      "stale": "These titles were read by an older version of ART. An update would improve them.",
      "unavailable": "not where the catalogue left it",
      "unavailableHint": "The file is not at {{path}} right now. The entry is kept; plug the drive back in or use Update.",
      "empty": "No folders in the catalogue yet. Add one to begin.",
      "emptyRoot": "Nothing scanned in this folder yet — press Update."
    }
```

Turkish:

```json
    "catalogue": {
      "folders": "Klasörler",
      "addFolder": "Klasör ekle…",
      "removeFolder": "Kaldır",
      "removeConfirm": "{{root}} katalogdan kaldırılsın mı? Başlıkları da gider; dosyalara dokunulmaz.",
      "update": "Güncelle",
      "updateHint": "Yalnızca son taramadan beri değişenleri okur.",
      "rescan": "Yeniden tara",
      "rescanHint": "Her dosyayı baştan okur. Yavaştır; kataloğa güvenmediğinizde kullanın.",
      "never": "hiç taranmadı",
      "scannedAt": "son tarama {{when}}",
      "stale": "Bu başlıklar ART'ın daha eski bir sürümüyle okundu. Güncelleme onları iyileştirir.",
      "unavailable": "kataloğun bıraktığı yerde değil",
      "unavailableHint": "Dosya şu an {{path}} konumunda değil. Kayıt korunuyor; sürücüyü geri takın ya da Güncelle'yi kullanın.",
      "empty": "Katalogda henüz klasör yok. Başlamak için bir tane ekleyin.",
      "emptyRoot": "Bu klasörde henüz hiçbir şey taranmadı — Güncelle'ye basın."
    }
```

Run: `pnpm test`
Expected: the parity test passes — equal key sets, no empty values, matching
interpolation variables.

- [ ] **Step 2: Replace the screen's load path**

In `CollectionStudio.tsx`:

- Drop `gameindexScan`, `onIndexResult` and the two auto-scan effects
  (`openedFor` / `arrivedWith`) entirely. **Nothing runs on open** — that is
  the whole point, and the auto-scan effect is also what ART-132's double scan
  came from.
- On mount, call `catalogueLoad()` once and flatten every root's entries into
  the existing `Shown[]`, adding `available` and the owning root.
- Subscribe to `onCatalogueRefreshed` and re-`catalogueLoad()` when it fires.
- A folder arriving through router state (`location.state.path`) calls
  `catalogueAddRoot` and then `catalogueRefresh(root, "update")` — one
  explicit action, not a silent scan.

The `Shown` interface gains:

```ts
  root: string;
  available: boolean;
```

and `flatten` takes the root and the `EntryView`:

```ts
function flatten(root: string, entry: EntryView): Shown {
  const r = entry.record;
  return {
    id: r.id,
    path: entry.path,
    root,
    available: entry.available,
    title: r.title.value,
    titleFrom: r.title.from,
    publisher: r.publisher?.value ?? null,
    publisherFrom: r.publisher?.from ?? null,
    year: r.year?.value ?? null,
    yearFrom: r.year?.from ?? null,
    chipset: r.chipset?.value ?? null,
    chipsetFrom: r.chipset?.from ?? null,
    media: mediaKind(r.media),
    diskCount: r.media.kind === "floppies" ? r.media.ordered.length : 1,
    kickstart: r.kickstart?.value.image ?? null,
  };
}
```

Titles from several roots are merged by `id`, keeping the first — the spec's
"storage is per root, the merge happens on screen".

- [ ] **Step 3: Add the folders panel**

A section above the filters listing each root with, per row: the path, either
`catalogue.scannedAt` or `catalogue.never`, an **Update** and a **Rescan**
button, and **Remove**. Above the list, **Add folder…** opens the directory
dialog and calls `catalogueAddRoot`.

A root whose `stale` is true shows `catalogue.stale` beside it. **Say it, do
not act on it** — the user chose that nothing runs unasked.

Remove asks first, through the existing confirmation pattern, using
`catalogue.removeConfirm`. It is the only action that deletes anything, and it
names what goes.

- [ ] **Step 4: Mark unavailable titles**

A row or card whose `available` is false renders `catalogue.unavailable`,
`title={t("gameindex.catalogue.unavailableHint", { path })}`, and its launch
buttons disabled — **disabled, not hidden**: a game whose drive is unplugged is
still in the library, and hiding it would look like ART lost it.

- [ ] **Step 5: Drive it in `pnpm tauri dev`**

Not optional, and not substitutable by a browser smoke test (ART-118, and
ART-132 was found this way):

1. Open the Collection. **Nothing should scan.** If the catalogue is empty it
   says so; if it has content it appears at once.
2. Add `E:\amiga\Amigatolon\WHDload`, press **Update**. One job. Watch the
   total: on a first run it is the file count, and **on the second Update it
   should be 0 or close to it** — that is the whole feature.
3. Add `E:\amiga\Titles` as a second folder. Both appear; the titles merge into
   one list.
4. Rename a `.hdf` outside ART, press Update: one file read, and the old entry
   stays marked unavailable.
5. Remove a folder: confirm the prompt names it, and its titles go.
6. Check Ctrl +/− still scales the folders panel.

**If something is wrong, report it before fixing it.**

- [ ] **Step 6: Commit**

```bash
pnpm lint && pnpm test
git add src/pages/CollectionStudio.tsx src/i18n/
git commit -m "feat(catalogue): the Collection opens saved, and scans only when asked"
```

---

## Task 8: Docs

- [ ] **Step 1: `docs/FEATURES.md`**

Flip the persistence row — and only as far as a test carries it:

```markdown
| Catalogue persistence, multiple folders | §41 | ✅ | `core/gameindex/store.rs` — one JSON per root, refreshed only when asked; a cached entry is reused when path, size and mtime match and the record's schema is current |
| Hand-edited titles | §41 | 🟡 | the override layer and "the user always wins" are built and tested; the editing UI belongs to wave C |
```

- [ ] **Step 2: `docs/STATUS.md`**

A session-log line, and refresh the snapshot's test counts and i18n key count
with the numbers `cargo test`, `pnpm test` and a leaf count actually printed.
Under "what to pick up next", strike A from the user's list of five and leave
B and C as they are.

- [ ] **Step 3: `docs/ISSUES.md`**

Nothing to close unless the screen run in Task 7 found something. If it did,
file it with what the run showed.

- [ ] **Step 4: `CHANGELOG.md`**

Under `[Unreleased]`, a user-facing entry: the Collection opens instantly and
remembers its folders; Update reads only what changed; a title whose file has
moved is kept and marked rather than dropped; hand corrections survive a
rescan.

- [ ] **Step 5: Full verification, twice**

```bash
pnpm lint && pnpm test
cd src-tauri && cargo fmt --check && cargo clippy --all-targets -- -D warnings
cargo test && cargo test
cargo deny check
python ../scripts/oracle-check.py
```

- [ ] **Step 6: Commit**

```bash
git add -A
git commit -m "docs: a saved catalogue, and what it does not do yet"
```

---

## Wave A done when

- `cargo test` passes twice, clippy clean at `-D warnings`, `pnpm lint` and
  `pnpm test` clean, `cargo deny check` clean.
- **A second Update of an unchanged folder reads zero files**, watched on a real
  screen against `E:\amiga\Amigatolon\WHDload`.
- Two folders are catalogued at once and their titles appear in one list.
- A hand-written override survives a Rescan — proved by a test and seen once on
  screen.
- Removing a folder is the only thing that deleted anything.
- No claim in `FEATURES.md` is ahead of a test.
