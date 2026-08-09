# Design — Software Sources Engine (Aminet)

Implements spec addendum **§41.5**. This document is the design ART will build
against; it describes intent, not state. Live state is in
[STATUS.md](STATUS.md) and [FEATURES.md](FEATURES.md).

> **Aminet is the store. ART is the courier and the customs officer.** (§41.5.11)

---

## 1. The constraint that shapes everything

`core/` may not touch the network and may not take a database dependency
(CLAUDE.md, core independence rule). Aminet is by definition a network feature
with a local catalog. The resolution is the same one `core/oplog` already uses:

**The core owns the logic and declares traits; the shell owns the I/O.**

```
core/sources/            pure: parsing, search, resolve, trust pipeline
    trait MirrorClient   ──► shell/net/http_mirror.rs   (ureq, blocking)
    trait CatalogStore   ──► shell/catalog/jsonl_store.rs (JSONL + in-memory index)
```

Both traits get an in-memory test double, so **the entire engine — including
mirror failover, resume and timeout paths — is testable offline with zero
network** (§41.5.9). CI never opens a socket.

### Catalog storage decision

§41.5.2 says "LOCAL CATALOG (SQLite)". SQLite in core would break the
independence rule, and the frontend's `art.db` would put engine logic in React.
**Decision: `CatalogStore` is a core trait; the v1 implementation is
file-backed** — one JSONL file plus an in-memory index built on load.

*Revised during implementation:* the JSONL store lives **inside** `core/`, not
outside it. The independence rule bars the network, Tauri and a database
dependency — not `std::fs`, which `core/safety` and `core/oplog` both use
already. `core/oplog/jsonl.rs` is the exact precedent: a trait in the parent
module, a file-backed implementation beside it. Only [`MirrorClient`] genuinely
needs a shell implementation, because only it touches the network.

Sizing, **measured against the real catalog on 2026-08-09** rather than
estimated: 85 435 entries, a 22.5 MB JSONL file, and a search that takes about
110 ms. That is slower than the "single-digit milliseconds" this document first
claimed — the scan lowercases each entry's name, path and description on every
query — but it is still inside what a search box can absorb, and it is honest.
If it needs to be faster, caching the lowercased forms is the obvious first
move. A `SqliteCatalogStore` can replace the whole thing later without a single
line changing inside `core/`, which is the point of the trait.

---

## 2. Module map

| Path | Layer | Contents |
|---|---|---|
| `core/sources/mod.rs` | core | Types: `PackageRef`, `PackageMeta`, `Claim`, `ClaimSource`, `apply_readme` |
| `core/sources/index.rs` | core | Aminet `INDEX` parser + `SyncReport` |
| `core/sources/readme.rs` | core | Readme field extraction |
| `core/sources/text.rs` | core | Bounding and control-character stripping for all repository text |
| `core/sources/catalog/mod.rs` | core | `trait CatalogStore`, `SourceQuery`, `search_over`, `resolve`, `compare_versions`, `MemoryCatalogStore` |
| `core/sources/catalog/jsonl.rs` | core | `JsonlCatalogStore` — the on-disk catalog |
| `core/sources/mirror.rs` | core | `Mirror`, URL construction, `trait MirrorClient`, failover |
| `core/sources/cache.rs` | core | Content-addressed cache layout |
| `core/sources/fetch.rs` | core | The trust pipeline (§41.5.3) |
| `core/sources/sync.rs` | core | `ProviderConfig`, `sync_catalog` |
| `net/http_mirror.rs` | shell | `HttpMirrorClient` — `ureq`, `Range:` resume, timeouts |
| `commands/sources.rs` | shell | Thin Tauri adapters |
| `src/lib/sources.ts` | frontend | Typed wrappers (no bare `invoke` in components) |
| `src/pages/AminetStudio.tsx` | frontend | The studio, route `/aminet` |

New Rust dependency: **`ureq`** (blocking, `rustls` TLS, no async runtime).
Chosen over `reqwest` because jobs already run on their own OS threads
(`commands/jobs.rs`), so a blocking client is the natural fit and drags in no
tokio. It lives outside `core/`; the core's `Cargo.toml` surface is unchanged in
spirit — `core/sources` still compiles against `std + serde` alone.

---

## 3. Catalog model (§41.5.2)

```rust
/// Identity of a package in a repository. Stable across syncs.
pub struct PackageRef {
    /// Repository-relative path, e.g. "util/libs/AmiSSL-5.5.lha".
    pub path: String,
    /// Provider id, e.g. "aminet". Reserved for OS4Depot etc.
    pub provider: String,
}

pub struct PackageMeta {
    pub reference: PackageRef,
    pub name: String,          // file name, "AmiSSL-5.5.lha"
    pub directory: String,     // "util/libs"
    pub size_bytes: u64,       // as claimed by the index, never an allocation
    pub age_weeks: Option<u32>,// Aminet's age column; saturates at 999
    pub short: String,         // one-line description from the index
    /// Fields recovered from the .readme, each with its own confidence.
    pub version: Option<Claim<String>>,
    pub requires: Vec<Claim<String>>,
    pub author: Option<Claim<String>>,
    pub distribution: Option<Claim<String>>,
}

/// A fact ART believes with a stated confidence. Never present free text as
/// certain (§14, §34, and the existing `Confidence` enum in core/workflow).
pub struct Claim<T> {
    pub value: T,
    pub confidence: Confidence, // HIGH | MEDIUM | LOW | UNKNOWN
    /// An enum, not free text, so the mapping stays exhaustive and testable.
    pub source: ClaimSource,    // IndexColumn | ReadmeField | Filename
}
```

Confidence assignment, fixed and testable:

| Source | Confidence |
|---|---|
| Index column (path, size, directory, short) | `High` |
| Readme `Version:` field, single clean value | `Medium` |
| Readme field present but ambiguous (multiple versions, prose) | `Low` |
| Version guessed from the filename only | `Low` |
| Field absent | no `Claim` at all — never `Unknown` as a stand-in for "not looked" |

§41.5.2's rule is explicit: when the newest catalog entry and the readme
`Version:` disagree, **show both**. `Resolution` therefore carries both and the
UI renders both; `resolve()` never silently picks.

```rust
pub struct Resolution {
    pub best: PackageMeta,
    /// Populated when catalog order and readme version disagree.
    pub disagreement: Option<VersionDisagreement>,
    pub alternatives: Vec<PackageMeta>,
}
```

### Index parser (`index.rs`)

**Verified against live mirrors on 2026-08-09.** The format is not what this
document originally assumed, and the correction is worth recording: the index
has a `|`-prefixed header and **five columns — name, directory, size, age,
description**. There is no column that already contains a path; the repository
path is the directory and the name joined.

```text
|
| Aminet index, created on 9-Aug-2026
|
A2KDeck.lha                    biz/dbase  671K 999 DataBase For AMWAY Distributors
AB.lha                         biz/dbase   31K 999 Nice address book program
```

The parser **never slices at absolute offsets** — it reads four
whitespace-delimited tokens and then the description, so column drift between
mirrors costs nothing. A line that yields no usable name or directory is
**skipped and counted**, never guessed, and the `|` header does not count as
damage.

Evidence: 3 026 of 3 026 data lines in a 256 KB sample of
`ftp.fau.de/aminet/INDEX` parse with zero skips.

Two honesty details fall out of the real data:

- The age column is **weeks-ago at index-generation time, not an upload date**,
  and it **saturates at 999**. `PackageMeta::age_is_capped()` exists so the UI
  says "or older" rather than inventing a precise nineteen years.
- A fourth column that is not a number is treated as the start of the
  description rather than a defect. Losing a sort key beats losing the package.

### Verified mirror defaults

| Mirror | Base URL |
|---|---|
| Aminet | `https://aminet.net/` |
| Aminet (Sweden) | `https://se.aminet.net/` |
| FAU Erlangen (Germany) | `https://ftp.fau.de/aminet/` |

All three served a byte-identical `INDEX` of 7 229 355 bytes, answered
`Accept-Ranges: bytes`, and returned `206` to a range request — so resume works
on every shipped default. `main.aminet.net` is a 301 to `aminet.net`.

Deliberately **not** shipped: `de`, `us3`, `au` and `sg` failed TLS on the same
day, and `nl` did not answer on 443. The list is configuration, so a user can
add them; ART just does not ship a default it could not reach.

`INDEX.gz` exists and is a quarter of the size, but ART has no decompressor for
it. Shipping that path without one would be exactly the untested claim §89
forbids.

`SyncReport { parsed, skipped, first_skipped_examples }` is returned and shown
in Power User Mode, so a mirror that changes format surfaces as a visible number
rather than a silently short catalog.

Hardening rules that apply here as everywhere (`core/security`):

- The size column is a *claim*, never an allocation size. Nothing is
  pre-allocated from it; `checked_add` guards running totals.
- Line length is bounded; a single 400 MB line does not become a `String`.
- Package paths go through `safe_join()` before they touch any local path, and
  a path that fails is dropped from the catalog entirely, not sanitised.

### Readme parser (`readme.rs`)

Aminet readmes carry `Short:`, `Author:`, `Uploader:`, `Type:`, `Version:`,
`Requires:`, `Distribution:`. They are hand-written and creative. Rules:

- Field extraction is line-anchored (`^Field:`), case-insensitive, and stops at
  the first blank line following a continuation.
- Values are bounded (256 bytes) and stripped of control characters.
- Anything unparsed stays available as the raw readme for the viewer — ART
  displays the readme verbatim (§41.5.5: never strip the licence).

---

## 4. Trust pipeline (§41.5.3)

Downloads are untrusted input (§56) and get the standard treatment:

```
FETCH (mirror, resumable)  →  SIZE CHECK  →  SHA256  →  ARCHIVE VALIDATION
                                                              ↓
                        INSTALL ← existing workflows ← CACHE (content-addressed)
```

Concretely:

```rust
pub fn fetch_package(
    meta: &PackageMeta,
    mirrors: &[Mirror],
    client: &dyn MirrorClient,
    cache: &CacheLayout,
    sink: &dyn ProgressSink,
) -> CoreResult<FetchOutcome>;
```

- **Resume** — a partial download lives at `<cache>/partial/<sha-of-path>.part`.
  On retry the client sends `Range: bytes=<len>-`. A mirror that ignores Range
  and replies `200` restarts from zero rather than appending; detecting that is
  a test case.
- **Size check** — a body larger than the catalog size + tolerance is aborted
  mid-stream. A body *smaller* than claimed is a failure, not a truncation.
  Either way the partial file is removed, never promoted.
- **Hash** — SHA256 over the completed file (`core/hashing`, which already
  streams rather than reading whole files).
- **Archive validation** — the completed file goes through the existing LHA
  engine: `open_archive()` for structure, and the `safe_extract` traversal
  checks. **No new archive code.** A package that fails validation never enters
  the cache.
- **Cache promotion** — only after all of the above: `atomic_write`-style
  rename into `<cache>/objects/<aa>/<sha256>.lha`. Content-addressed, so the
  same hash is never fetched twice and a corrupted entry is self-evident.
- **Install** — reuses the existing extract/copy workflows unchanged. This
  module knows how to *find* and *fetch*; it does not know how to install.
- **Failure leaves images untouched** (§57). The cache lives outside every
  image by construction.

Mirror failover: mirrors are tried in configured order; a failure records
`(mirror, reason)` and moves on. All mirrors failing is one error listing every
attempt, not a bare "download failed". A *bad path* propagates immediately
instead — no other mirror will do better with it — and a cancellation stops
rather than trying harder.

### A bad sync must not cost a good catalog

Added during implementation, and the most valuable rule in the module. The
dangerous failure is not a network error — that is obvious and recoverable. It
is a mirror answering **successfully** with an error page or an index in a
format ART cannot read: the parse yields three entries, the sync "succeeds",
and ninety thousand packages quietly disappear from the user's catalog.

So `sync_catalog` only replaces the catalog when `SyncReport::looks_complete()`
holds (no truncation, and fewer than one line in twenty skipped). Otherwise the
existing catalog stays and the report says what arrived. §57's rule — never
destroy what is there on the strength of something that failed — applies to the
catalog as much as to an image. The exception is a *first* sync, where there is
nothing to lose: a poor catalog beats no catalog, and the report still says so.

### Safety classification

| Operation | `Safety` | Notes |
|---|---|---|
| `sources_sync` | `Safe` | writes only the catalog file, atomically |
| `sources_search` / `sources_resolve` | `ReadOnly` | pure catalog queries |
| `sources_fetch` | `Safe` | writes only into the cache; refuses to overwrite a differing object at the same hash path |
| Install to HDF/ADF | existing class | unchanged; goes through its studio's pipeline |

Nothing here is `RequiresBackup` or `Destructive`, which is why the whole module
can be reached from the drop panel without violating the `run_workflow`
read-only rule — installs still route through their studio.

---

## 5. What this module does not promise (§41.5.4)

Encoded in the design, not just the docs:

- **No dependency resolution.** A readme `Requires: MUI` becomes a
  `Claim<String>` rendered as a warning with a one-click "also install MUI?"
  suggestion. There is no recursive install path in the code at all — the
  suggestion opens a normal search, so the honest label and the implementation
  cannot drift apart.
- **No uninstall.** The Operation Log records what was copied where; that is the
  whole story for v1.
- **No ART-side "stable" semantics.** The channel label is catalog data,
  displayed with its provenance.

---

## 6. UI — Aminet Studio (§41.5.6)

Route `/aminet`, page `src/pages/AminetStudio.tsx`, reached from the sidebar.

*Revised during implementation:* no workflow-catalogue entry. The catalogue
answers "I dropped this file, what can I do with it?", and in Stage A nothing
you can drop leads to Aminet. Registering a `route::AMINET` that no workflow
points at would be dead code with no test behind it. Stage B's update view is
the first thing that plausibly needs one.

- Search + category browse (`util/`, `game/`, `demo/`, `comm/`, …)
- Readme viewer, verbatim
- Per-package actions follow §46: primary **Install to \<target\>**, secondary
  **Download / Inspect / Add to Collection**
- Download/install queue on the existing Job Queue — no second queue. Sync,
  fetch and install are `spawn_job` operations reporting through `ProgressSink`,
  so they appear in the global `JobBar` and are cancellable from anywhere.
  Cancellation is checked between whole units (per file, per package), never
  mid-write.
- **Update view** — "3 packages in your Collection have newer versions on
  Aminet", computed by comparing Collection hashes/versions against the local
  catalog. Suggests; never auto-updates.
- **Beginner Mode hides** mirror selection, cache path and raw catalog queries.
  It hides only — sync and download still work exactly the same.

Every mutating step writes an `OperationRecord`, with `details` carrying the
package path, the SHA-256 and the mirror used. A sync that was *not* applied is
logged as a verified-false success — it worked, and it deliberately changed
nothing — so the log can still explain later why the catalogue is old.

---

## 7. Offline behaviour (§60/§94)

- Search, browse, resolve and readme viewing work **from the local catalog with
  no network**. One sync at a friend's house, browse at home.
- With no catalog at all, the studio shows "no catalog yet — sync to get one"
  and every other ART feature is unaffected. Nothing in ART waits on this
  module, and no core feature calls into it.

---

## 8. Testing (§41.5.9)

All offline, all synthetic — ART ships no copyrighted Amiga content.

| Area | Tests |
|---|---|
| Index parser | column drift, CRLF, truncated final line, malformed lines counted not guessed, huge size field, non-UTF8 bytes |
| Readme parser | missing fields, duplicated `Version:`, continuation lines, 10 MB readme, control characters |
| Confidence | each source maps to its documented level; disagreement surfaces both values |
| Mirror | `MockMirror` serving fixtures; failover order, timeout, resume via Range, mirror that ignores Range, mirror that returns a short body, mirror that returns a long body |
| Trust pipeline | wrong size → no cache entry; bad archive → no cache entry; cancel mid-fetch → no partial promoted, no image touched |
| Injection (feeds §45.5.7) | package names containing `../`, `C:\`, `:`, NUL, ANSI escapes, and readmes containing "IGNORE PREVIOUS INSTRUCTIONS" |
| End-to-end | search → fetch → validate → install to HDF → verify, entirely against `MockMirror` |

---

## 9. Staging

**Stage A — build now.** Catalog sync, search, browse, readme viewer, download
to cache, add to Collection. Everything above except installs.

~~Open item: the real Aminet mirror list and index path have not been
confirmed.~~ **Done 2026-08-09** — see the verified format and mirror table
above. `sync::aminet_defaults()` carries the result, and a test asserts the
defaults compose the URLs that were actually reached.

**Stage B — after Stage A lands.** One-click Install to HDF, update view.
Registered `available: false` until then, so they render as "Coming Later"
(§96) rather than vanishing.

**Stage C.** AI plan integration — the `sources_*` tools of §41.5.7, defined in
[design-ai-layer.md](design-ai-layer.md).

## 10. Prior art (§41.5.8)

Read before implementing the parsers; licence-check before reusing anything.
ART's engine is its own implementation.

- [amiget (emartisoft)](https://github.com/emartisoft/amiget)
- [amiget (johey)](https://github.com/johey/amiget)
- [pyadt](https://github.com/ali1234/pyadt) — index parsing reference

`docs/licenses.md` gains a section describing the Aminet arrangement: packages
carry their own licences, ART displays them before install and installs the
readme alongside the package by default.
