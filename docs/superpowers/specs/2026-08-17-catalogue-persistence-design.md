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

## Why JSON and not SQLite: who owns the file

ART has SQLite (`art.db`, `tauri-plugin-sql`) and it is the wrong tool here for
a reason that has nothing to do with performance. **The frontend owns it** —
migrations run on the frontend's first `Database.load` — while the thing that
produces a catalogue is `core/`. Writing 1698 records from TypeScript puts the
work on the wrong side of CLAUDE.md's rule that technical complexity belongs in
the Rust core, and putting a SQL dependency inside `core/` puts a C library into
a list kept deliberately narrow and pure-Rust so the module stays promotable.

The project's own precedent is a JSON manifest written by `core/`:
`distribution.json` (G5), `card.manifest.json` (G7). This follows it.

§4 carries the other half of the answer — the numbers — because the question
deserves both.

## 1. What is stored, in three layers

```
<ART data dir>\catalogue\
  roots.json                        which roots are catalogued, and in what
                                    order — nothing else
  e-amiga-amigatolon-whdload.json   the READ layer — one file per root: its
  e-amiga-titles.json               records each beside a cheap cache key
                                    (path · size · mtime), plus when it was
                                    last scanned and which reader read it
  overrides.json                    the USER layer — hand edits, keyed by
                                    record id
```

**A root's own facts live in that root's file, not in the list.** Two places
holding "when was this scanned" is two places to disagree, and the list would
have to be rewritten every time any single root was refreshed — losing the
per-root isolation that is the whole reason for the split.

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
    pub schema: u32,                  // the catalogue FILE format's version
    pub root: String,
    pub scanned_at: Option<String>,   // supplied by the caller; core has no clock
    pub index_schema: u32,            // GAMEINDEX_SCHEMA when this root was read
    pub entries: Vec<CachedEntry>,
}

pub struct CachedEntry {
    pub path: String,
    pub size: u64,
    pub mtime_ms: i64,                // milliseconds: two writes inside one second are not unusual
    pub record: GameRecord,
}
```

**Two schema numbers, and conflating them would be a mistake.** `schema`
versions the *file format*; `index_schema` records which *reader* produced the
records inside. They move for different reasons — the first when these files
change shape, the second when a reader starts producing better facts from the
same bytes — and treating a reader improvement as a format change would force a
migration nobody needs.

### Two write classes, deliberately not treated alike

| File | What it is | How it is written |
|---|---|---|
| root files | derived — can be rebuilt by rescanning | `core/safety::atomic_write` (a half-written catalogue is a lost one) |
| `overrides.json` | **user data**, not reproducible | `guarded_write` with `BackupPolicy::CONFIG` (5 generations) |

### The same game in two roots

Storage keeps it in both root files; the **screen** merges by `id`. Merging in
storage would make removing a root impossible without rewriting the other.

## 4. Scale, measured

The user asked the right question: *what happens at 10,000 games, and would
SQLite not be more correct?* Measured rather than argued.

**One entry serialises to 824 bytes compact, 1124 pretty**, in the shape
`record.rs` actually produces:

| entries | compact | pretty |
|---:|---:|---:|
| 1698 (today's library) | 1.3 MB | 1.8 MB |
| 10 000 | 7.9 MB | 10.7 MB |
| 50 000 | 39 MB | 54 MB |

**Root files are written compact.** A machine reads them, and pretty-printing
costs 36% for nobody's benefit. `overrides.json` stays pretty — a person may
open it, and it is small.

Where the time actually goes at 10 000, and whether SQLite helps:

| Cost | At 10 000 | Does SQLite help? |
|---|---|---|
| **The scan itself** — hashing, reading inside hardfiles | minutes; the problem this whole round exists to remove | **No.** Identical either way. Persistence is what fixes it, not the format |
| Parsing the catalogue on load | ~8 MB of JSON | Partly — it need not parse what is not shown |
| Writing after a refresh | ~8 MB, atomically | Similar; SQLite writes only changed rows |
| Availability | 10 000 `metadata()` calls | **No.** The same syscalls |
| Crossing to the webview | ~8 MB over IPC | **No**, unless the screen paginates — a screen decision, not a storage one |
| Rendering 10 000 rows | needs virtualisation | **No.** Wave C's problem regardless |

SQLite's one real win is **querying without loading**, and it only pays if the
screen paginates. Against that, in ART specifically: `art.db` is opened by the
*frontend* — migrations run on its first `Database.load` — so a SQLite
catalogue means either TypeScript writing 10 000 rows, which is the wrong side
of "technical complexity belongs in the Rust core", or a SQL dependency inside
`core/`, which is a C library in a dependency list kept deliberately narrow and
pure-Rust so `core/` stays promotable.

**The decision is reversible, which is why measuring beats arguing.**
`store`'s interface — `load`, `refresh_root`, `add_root`, `remove_root`,
`set_override` — says nothing about JSON. A different backing store would touch
`store.rs` and nothing else.

So the plan carries a **10 000-entry load test**, printing its timing. It is
there to catch the one thing that would actually change the answer: an
accidentally quadratic load. If it ever fails, the seam above is where SQLite
goes in.

### Artwork is the big number, and it is not this format's problem

Measured from the sources the user named, `Commodore - Amiga` on
[thumbnails.libretro.com](https://thumbnails.libretro.com/):

| set | files | median | total |
|---|---:|---:|---:|
| `Named_Boxarts` | 2958 | 368 KB | **1217 MB** |
| `Named_Snaps` | 3072 | 11 KB | 96 MB |
| `Named_Titles` | 3045 | 31 KB | 352 MB |
| | | | **1.63 GB** |

([LaunchBox's `Metadata.zip`](https://gamesdb.launchbox-app.com/Metadata.zip)
is 102 MB, refreshed daily, covers every platform in one file, carries no
images, and publishes no terms.)

Three consequences, and they are **constraints A places on B** rather than
things B gets to decide:

1. **No image ever goes in the catalogue JSON.** A record points at a cache
   entry; the bytes live beside it as files. 1.63 GB is 200× the metadata, and
   conflating them would make every load pay for artwork.
2. **Artwork is cached per *title*, not per record.** The whole Amiga platform
   is about 3000 titles, so the cache has a ceiling that does not grow with the
   collection — and one image serves several records: this user's 1697
   hardfiles hold `1869` five times (`1869 AGA`, `1869 AGA De`, `1869 De`,
   `1869 Pl`, `1869`) and `Agony` twice.
3. **The cache's location is a setting.** The default is ART's data directory,
   which is on `C:` — and 1.6 GB written to `C:` without asking is precisely
   what this user objects to. The same instinct that made them ask for
   configurable sources applies to where the bytes land.

## 5. Testing

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

## 6. Out of scope

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
