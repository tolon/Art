# A saved catalogue — the Collection stops re-reading 3.74 GB (SD-2 · G10 wave A)

**Date:** 2026-08-17
**Status:** approved (2026-08-17)
**Scope:** a new `core/gameindex/store.rs`, one new command, the Collection
screen's load path, and two new `Provenance` variants
**Follows:** [2026-08-17-g10-launcher-metadata-design.md](2026-08-17-g10-launcher-metadata-design.md)
(wave 1, the index itself)

---

## What this document is

The design for the first of three rounds the user asked for after wave 1
landed. It is not a plan; the implementation plan follows.

The three rounds, in the dependency order the user confirmed:

- **A — the catalogue becomes a saved thing** (this document): persistence and
  more than one folder, together, because the storage shape has to know about
  several roots from the start or it gets migrated twice.
- **B — enrichment from configured sources**: artwork and chipset fetched from
  sources the user defines in Settings. Its own round.
- **C — the richer screen**: LaunchBox-shaped. Its own round.

## The problem, measured

Wave 1's Collection re-scans on every visit: 1699 files, 3.74 GB of SHA-256.
That is minutes. The user's words: *"10 dk beklemek istemiyorum — o datayı ayrı
bir klasör içinde indexlesin sistem."*

**Caching was in wave 1's design round and did not make it into wave 1's plan.**
That is an omission, not a decision, and this document is where it is repaid.

A second thing was found while writing this: `FEATURES.md` claimed a SQLite
schema for the catalogue existed and only lacked wiring. It does not.
`001_initial.sql` creates `settings`, `recent_files` and `jobs`, and it is the
only migration. Corrected in the same session.

## Why not SQLite

ART has SQLite (`art.db`, `tauri-plugin-sql`) and it is the wrong tool here.
**The frontend owns it** — migrations run on the frontend's first
`Database.load` — while the thing that produces a catalogue is `core/`. Writing
1698 records from TypeScript puts the work on the wrong side of CLAUDE.md's
rule that technical complexity belongs in the Rust core.

The project's own precedent is a JSON manifest written by `core/`:
`distribution.json` (G5), `card.manifest.json` (G7). This follows it.

## 1. What is stored, in three layers

```
<ART data dir>\catalogue\
  roots.json                        which roots are catalogued, in order, each
                                    with its last-scan time and the schema it
                                    was scanned with
  e-amiga-amigatolon-whdload.json   the READ layer — one file per root: its
  e-amiga-titles.json               records, each beside a cheap cache key
                                    (path · size · mtime)
  overrides.json                    the USER layer — hand edits, keyed by
                                    record id
```

One file per root is what makes several folders fall out for free: adding a
root writes a file, removing one deletes a file, and refreshing one leaves the
others untouched. A single combined file would rewrite everything to refresh
anything, and two roots scanned at once would overwrite each other.

### The layers are ranked, and `is_stated()` is no longer enough

`Fact<T>` already carries the source beside the value. What this design adds is
an **order** over those sources:

```
user edit  >  declared (slave · rp9)  >  fetched from a configured source (B)  >  filename · drawer name
```

Concretely, in `core/gameindex/record.rs`:

- `Provenance` gains `UserEdit`. B will add its own variant for a third-party
  database; it ranks **below** a declaration, because a stranger's guess does
  not outrank what the packager wrote down.
- `Provenance::is_stated() -> bool` becomes insufficient with four tiers. It
  stays, because the screen's `~guessed` badge is exactly a two-way question,
  but **which fact wins** is decided by a new `rank()`. A bool cannot order
  four things, and the place that silently ordered them would be the place the
  order got wrong.

### A rescan rewrites the read layer and never touches overrides

Not a convenience. Any other arrangement means every scan destroys the user's
own work, which is the heaviest possible breach of *nothing changes unless the
user changes it*.

### Availability is derived, never stored

The catalogue records what was **read**. Whether a file is there *now* is a
question only the disk can answer, and answering it costs one `metadata()` per
entry — under a second for 1699. Storing it would let the catalogue say
"Ready" about a game on an unplugged drive.

This is a deliberate exception to "no work unless asked" (§2 below), taken with
the user's agreement and for a §89-shaped reason: a stale *available* is a
missing answer rendered as a pass.

## 2. Update and Rescan — two explicit actions, nothing automatic

The user's choice, and consistent with the rule that ART starts no work by
itself. Opening the screen shows the catalogue as it stands. Both actions below
are jobs (§54/§55) with a Stop.

**Update**, per root:

```
1. walk the root                readdir, depth-limited, no hashing
2. for each indexable file:
     is there a cached entry with the same path + size + mtime,
     whose record's schema is current?
        yes -> reuse it verbatim; the file is never opened
        no  -> read it through read_one
3. entries whose path no longer appears -> kept
4. write the root file atomically
```

**Rescan** is the same walk with the cache ignored: every file present is
re-read and re-hashed. It is **not** "start from zero" — entries for files that
are absent are still kept, because that is what the user chose.

**Progress reports what will actually be read**, not the file count. Three
changed files show "3", not "1699 of 1699". That is both the honest number and
the reassuring one.

### Consequence, stated rather than discovered later

**There is no way to remove a single entry.** Over years, records for games
long deleted accumulate. The escape hatch is coarse: removing a *root* deletes
its file and its entries with it. Entry-level cleanup was offered and not
chosen; it can be added when it becomes annoying.

### Schema invalidation is what makes reader fixes land

`roots.json` records the schema each root was scanned with — the value of
`record::GAMEINDEX_SCHEMA` at the time. After a fix like
[ART-131](../../ISSUES.md), where a reader starts producing better facts from
the same bytes, that constant is bumped, and the screen can say
*these entries were read by an older reader; an update would improve them* —
**saying it without doing it**. An entry whose cached schema is stale is
re-read by the next Update even if its path, size and mtime all match.

## 3. Where it lives

`core/gameindex/store.rs`.

**`core/` cannot know where the app's data directory is** — it is platform
specific, and `core/` is not. So every function here takes the catalogue
directory as a `&Path`, and `commands/gameindex.rs` resolves it from Tauri.
The same discipline as `CardManifest`'s `built_at: Option<String>` ("core has
no clock") and `core/preload`'s `VolumeFormatter` trait.

Shape:

```rust
pub struct CatalogueRoot {
    pub root: String,
    pub scanned_at: Option<String>,   // supplied by the caller; core has no clock
    pub schema: u32,
    pub entries: Vec<CachedEntry>,
}

pub struct CachedEntry {
    pub path: String,
    pub size: u64,
    pub mtime: i64,
    pub record: GameRecord,
}
```

### Two write classes, deliberately not treated alike

| File | What it is | How it is written |
|---|---|---|
| root files | derived — can be rebuilt by rescanning | `core/safety::atomic_write` (a half-written catalogue is a lost one) |
| `overrides.json` | **user data**, not reproducible | `guarded_write` with `BackupPolicy::CONFIG` (5 generations) |

### The same game in two roots

Storage keeps it in both root files; the **screen** merges by `id`. Merging in
storage would make removing a root impossible without rewriting the other.

## 4. Testing

Tempdir throughout, as always. Two tests carry the design:

- **The cache really skips the read.** Change a file's *contents* while keeping
  its size and mtime. If the record comes back unchanged, the file was not
  opened. An implementation that still reads cannot pass this.
- **A schema bump forces a re-read.** Lower a cached record's schema by one and
  assert Update reads that entry again even though path, size and mtime all
  match.

And: Rescan ignores the cache · a missing file keeps its entry · **overrides
survive a rescan** · removing a root removes its entries · a root file written
and loaded round-trips · a catalogue directory that does not exist yet is
created rather than refused · a corrupt root file is refused with a reason
rather than silently starting empty.

## 5. Out of scope

- **The editing UI.** A carries the override layer and the "user always wins"
  rule; the interface for editing a title belongs to C.
- **Anything fetched.** A designs the *tier* — where a third-party fact ranks
  against a declaration and a filename — because ranking four sources once is
  cheaper than ranking three and then re-ranking. Where those facts are stored,
  and whether they go beside the read layer or in a file of their own, is B's
  decision and this document does not make it.
- **Entry-level cleanup**, per §2.
- **Artwork.** The `.rp9` previews wave 1 already reads into `record.preview`
  stay unrendered until C.
