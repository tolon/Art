# Collection wave B — artwork from configured sources

**Status:** approved 2026-08-17
**Implements:** wave B of the Collection, deferred by
[the catalogue-persistence design](2026-08-17-catalogue-persistence-design.md) §"B —
enrichment from configured sources".
**Depends on:** wave A (the saved catalogue), G10 wave 1 (the game index).

---

## 1. What this builds, and what the measurement changed

Wave B was specified as "artwork **and chipset** fetched from sources the user
defines in Settings". Measuring the real catalogue before designing changed that
sentence, so this document records the measurement first.

The user's catalogue, 1700 records scanned from `E:\amiga\Amigatolon\WHDload\`:

| field | filled | empty | filled by |
|---|---:|---:|---|
| `title` | 100 % | 0 | the WHDLoad slave, offline |
| `publisher` | 98.8 % | 20 | the slave, offline |
| `year` | 92.5 % | 128 | the slave, offline |
| `kickstart` | 44.6 % | 941 | the slave, offline — no external source knows this |
| `chipset` | **9.6 %** | 1537 | see §1.1 |
| `genre` | **0 %** | 1700 | nothing |
| `rating` | **0 %** | 1700 | nothing |
| `preview` | **0 %** | 1700 | nothing |

The metadata people assume needs fetching — title, publisher, year — is already
there, and it arrived without a network connection because the person who wrote
the slave put it in the slave. **The only field an online source can usefully
fill is the picture.** That is what this document designs.

### 1.1 Chipset cannot be derived offline, and this was tested

An attractive shortcut was proposed and rejected on evidence. `ws_Flags` bit 5
(`ReqAGA`) is read for every slave; the idea was that a *clear* bit means the
title runs on OCS/ECS, which would take `chipset` from 9.6 % to ~99 % with no
network at all.

`chipset_of()` already carried a comment saying this was wrong. The claim was
tested against the catalogue rather than argued:

```
1537  (empty)
  85  aga <- tosec-name      the filename says AGA; the slave's flag does not
  78  aga <- whdload-slave   the flag is set
```

Provenance ranks the slave (3) above the filename (1), so a record reading
`aga <- tosec-name` is one where **the slave was read and left the flag clear
while the title says AGA**. There are 83 such records among the 1681 that have a
slave — `Ace Ball v1.0 Pl AGA.hdf`, `Alfabet Smierci v1.0a AGA Pl.hdf`,
`Akira v1.3 CD32.hdf`, `Arcade Pool v3.0 CD32.hdf` (CD32 *is* AGA hardware).

Writing `OcsEcs` for every clear flag would therefore mislabel at least those
83, and those are only the ones whose filename happens to give them away. AGA
titles that say nothing anywhere would be wrong silently, with no way to detect
it.

A second query confirms the name-based seam is already fully mined: **zero**
records have an empty `chipset` and `AGA` in their filename.

**Decision: `chipset` stays absent when nothing states it.** 9.6 % is the
correct answer, not a gap. ART says "unknown" rather than guessing, which is the
same rule the rest of the record already follows.

### 1.2 Genre, rating and chipset are out of scope, for want of a source

Every candidate that models Amiga chipset or genre was investigated:

| source | access | terms | holds |
|---|---|---|---|
| **libretro-thumbnails** | git tree API, machine-readable | repository licence | images only |
| **whdload.de** | paths built from the package name | — | `.lha` package, `ico/*.png` |
| **Aminet** | already in ART (`core/sources`), INDEX files | — | packages |
| OpenRetro | undocumented "half-public" API | "liberal usage terms", no licence named | chipset, machine config |
| LaunchBox | one 102 MB zip | none published | general metadata |
| Lemon Amiga | **HTTP 403 to every non-browser request, `robots.txt` included** | — | — |
| Hall of Light | HTML pages only | — | — |

Lemon Amiga is excluded because reaching it requires forging a user agent to
defeat an access control the site deliberately operates. Hall of Light is
excluded because ART fetches index files and packages, never HTML (§41.5.3) —
the rule the mirror guarantee rests on.

OpenRetro holds exactly the chipset data wanted and its `/about` page expects
third-party applications, but it publishes no endpoint documentation and names
no licence. The project owner's position, recorded here because it governs
future rounds: an absent licence is **not** a blocker for forty-year-old game
and demo metadata, and every source ART ships is enabled by default. What *is* a
blocker is an absent endpoint — ART cannot build a default source on an API
whose shape is discussed on Discord. OpenRetro therefore returns as its own
round once the endpoint is known; it is not in this one.

`genre` and `rating` follow it out, and this is deliberate rather than
forgotten: with Lemon and Hall of Light closed, the only remaining holder is
LaunchBox, and putting an unshaped dependency into the default path is worse
than leaving a field honestly empty.

---

## 2. The two artwork sources

### 2.1 whdload.de — an exact key, no matching at all

Package pages carry a predictable file layout, confirmed against
`https://www.whdload.de/games/Moonstone.html`:

| what | path |
|---|---|
| package | `games/<Name>.lha` |
| icon | `games/ico/<Name>.png` |

`<Name>` is the WHDLoad package name, which ART **already holds** — 1681 of 1700
records were titled from their slave, and the package name is how the drawer and
the slave are both named. There is no matching problem here: the key is exact or
the record has no key.

What the site does *not* give is equally clear. Its metadata (version, memory,
year, publisher) lives on HTML pages, may not be scraped, and is redundant
anyway — the slave already stated it offline. The one thing found only on those
pages is the Hall of Light and Lemon Amiga cross-reference ids, which would make
matching exact everywhere; they are not obtainable within the rules and this
design does without them. A `.readme` at a predictable path does not exist
(verified: HTTP 404).

The icon is an Amiga icon, not box art. Its rendered size must be measured
against real files during implementation; if it is small it belongs in the list
row, not in a cover slot. The design does not claim it is a cover.

### 2.2 libretro-thumbnails — an index, then only the images that matched

Measured on `libretro-thumbnails/Commodore_-_Amiga` (1.76 GB, default branch
`master`):

| kind | files | index JSON |
|---|---:|---:|
| `Named_Boxarts` | 3324 | 0.8 MB |
| `Named_Titles` | 3434 | 0.8 MB |
| `Named_Snaps` | 3475 | 0.8 MB |

The naming is plain title text, not No-Intro region tags:

```
'Allo 'Allo - Cartoon Fun!.png
1000 Miglia - 1927-1933 Volume 1.png
1869 - Erlebte Geschichte Teil I.png
1869 - History Experience Part I.png
```

**The index is fetched, not guessed.** A speculative "construct the URL and read
the 404" strategy cannot work here: `1000 Miglia` cannot be turned into
`1000 Miglia - 1927-1933 Volume 1` by any rule. It is also the impolite design —
1700 requests, most of them misses — where fetching three index files is 2.5 MB
and then downloads only what matched.

The index must be machine-readable, which rules out the browsable HTML directory
listing at `thumbnails.libretro.com`. GitHub's git-tree API serves the same
listing as JSON, and — verified — a **query-free** path returns one directory
complete and untruncated:

```
base:  https://api.github.com/
path:  repos/libretro-thumbnails/Commodore_-_Amiga/git/trees/master:Named_Boxarts
```

This matters because `Mirror::new` rejects a base containing `?` or `#`, and
`url_for` builds `<base><path>` and nothing else. The `?recursive=1` form would
not have been expressible; the `tree-ish:subdirectory` form is.

---

## 3. Matching: strict, and its two rules are written down

The user chose strict matching over a confirmation queue. Provenance already
makes this coherent: online data is rank 2, below the slave (3) and the user's
own edit (4), so it fills gaps and never overwrites a stated fact. A silently
accepted wrong guess would make that ranking meaningless.

Matching normalises both sides, then applies exactly two rules in order:

1. **Whole-title equality** after normalisation.
2. **Head equality** — the part before the first ` - ` — when rule 1 finds
   nothing. This is what connects `1869` to `1869 - Erlebte Geschichte Teil I`.

Normalisation is case folding, trimming, collapsing runs of whitespace, and
dropping a leading article (`The `, `A `, `An `). It does **not** do edit
distance, token overlap, or any other similarity measure. Anything that fails
both rules has no artwork and is left empty for the user to attach by hand once
wave C has a screen for it.

One consequence is accepted rather than hidden: rule 2 can match a title to more
than one candidate — `1869` matches both the German and the English release. The
first in sorted order wins, deterministically, so two runs over the same data
produce the same result.

---

## 4. The cache

Wave A's storage decisions carry over unchanged and are restated because they
are binding:

1. **No image ever goes in the catalogue JSON.** A record points at a cache
   entry; the bytes live beside it as files.
2. **Artwork is cached per *title*, not per record.** The whole Amiga platform is
   about 3000 titles, so the cache has a ceiling that does not grow with the
   collection — this user's 1700 records contain `1869` five times and one image
   serves all five.

To which this round adds:

3. **Misses are cached too.** Without a recorded miss, every run re-asks for the
   same ~1300 titles nobody has a picture of. A miss records which source was
   asked and when, so re-asking is a user action, not a side effect of opening
   the screen.
4. **The cache is its own directory**, a sibling of the catalogue directory, not
   inside it. A user deleting artwork to reclaim 1.6 GB must not lose the index
   that took minutes to build.
5. **Every write goes through `core/safety::atomic_write`.** Derived data, so no
   backup policy — but a half-written PNG must never exist.

---

## 5. Configuration

Source **types** ship with ART. Each is code — the parsers differ per source and
are not expressible as data — and each exposes exactly two settings:

- **enabled** — every source ships **enabled**;
- **mirror base URL** — editable, validated by `Mirror::new`.

A user may point libretro at a different mirror or turn OpenRetro off. A user may
**not** define a new source from a URL template: that would restore
arbitrary-URL fetching and void the guarantee in `core/sources/mirror.rs` that no
function anywhere fetches a caller-supplied URL. Adding a source *type* is a code
change, and this project is open source precisely so that remains possible.

**Enabled by default does not mean fetched automatically.** Nothing reaches the
network when the Collection screen opens. Enrichment is a job the user starts,
with progress and cancel, for the same reason wave A removed the automatic scan:
a screen that starts work on open and makes the user wait is the behaviour that
was just fixed.

### 5.1 Politeness is a design constraint

Requests go **sequentially, over one connection, no more than four per second
per host**. whdload.de is run by volunteers on a small server; opening dozens of
parallel connections would finish ART's job sooner at their expense. The rate is
a constant in `core/artwork`, not a setting — a user cannot be asked to choose
politely on someone else's behalf. The index-first design already removes the
bulk of the traffic: three index files, then only the images that matched.

---

## 6. Module layout

```
core/artwork/
├── mod.rs          ArtKind { Boxart, Screenshot, Title, Icon }, ArtRef
├── key.rs          normalisation and the two matching rules
├── cache.rs        the on-disk cache: entries, misses, atomic writes
└── sources/
    ├── mod.rs      trait ArtSource
    ├── libretro.rs index parsing + image paths
    └── whdload_de.rs  exact package-name paths
```

`core/artwork` **declares** what it needs and opens no connection itself: it
takes `&dyn MirrorClient` (already defined in `core/sources/mirror.rs`) and
`&dyn ProgressSink`, so the whole module is testable with a fake client and the
test suite touches no network. This is the core-independence rule in its ordinary
form, not an exception to it.

`ArtSource` is deliberately narrow:

```rust
pub trait ArtSource: Send + Sync {
    fn id(&self) -> &'static str;
    fn kinds(&self) -> &'static [ArtKind];
    /// The index this source needs before it can match, if any.
    fn index_paths(&self) -> Vec<String>;
    /// Given a fetched index and a title, the repository path to fetch.
    fn locate(&self, index: &SourceIndex, title: &str, kind: ArtKind) -> Option<String>;
}
```

The two supporting types:

- **`SourceIndex`** — what a source parsed out of its index files: a map from
  normalised title to the repository path holding that title's image, one map per
  `ArtKind`. A source with no index (whdload.de) carries an empty one.
- **`ArtRef`** — what a record gains: which `ArtKind` it is, which source id
  provided it, and the cache-relative filename. Not a URL and not an absolute
  path, so a cache directory that moves does not invalidate the catalogue.

whdload.de returns an empty `index_paths()` and locates from the title alone;
libretro returns three and locates from what it parsed. Neither builds a URL —
`Mirror::url_for` does that, once, at the last point before bytes leave the
machine.

---

## 7. Error handling

- A source that fails is **not** fatal. The job continues with the others and the
  outcome reports, per source: images written, titles matched, titles missed, and
  whether the source was reachable at all.
- `fetch_with_failover` already exists and is used unchanged.
- Every image download is size-gated; an index that exceeds its bound is rejected
  before parsing, in line with "never allocate from an unchecked length field".
- Cancellation is checked between whole titles, never mid-write, so a cancelled
  job leaves work undone but never a truncated file.
- Failures are logged through the operation log with their `ART-*` code, like
  every other operation that touches the user's data.

---

## 8. Testing

- **Normalisation and the two rules** — unit tests using real pairs taken from
  the measured catalogue and the measured libretro index (`1869`,
  `1000 Miglia`, `'Allo 'Allo - Cartoon Fun!`), including a case that must *not*
  match.
- **Sources** — a fake `MirrorClient` returning canned index bytes. No network in
  the suite.
- **Cache** — tempdir fixtures; an entry, a miss, and a re-run that fetches
  nothing because both were recorded.
- **Data safety** — a fetch that fails midway leaves the cache byte-for-byte as
  it was, with a test proving it.
- **Security** — an index entry whose name contains `..`, an absolute path, or a
  drive prefix must never become a file path; `safe_join` is the only way an
  entry name becomes a destination.

---

## 9. What this deliberately does not do

- **Chipset, genre and rating** — §1.1 and §1.2. Not forgotten; unsourceable
  today.
- **OpenRetro** — its own round, once an endpoint is documented.
- **The rich screen** — wave C. This round shows a thumbnail in the existing list
  and nothing more.
- **Attaching artwork by hand** — needs the screen wave C builds.
- **Any write to a user's game files.** This round writes only to its own cache.
