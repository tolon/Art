# Identifying install media by what it *is*, not what it is called

*Written 2026-09-06. **This is history**: it describes the tree, and one
outside table, on the day it was written. Re-run the measurements below rather
than re-trusting them — every number here came from a command, and the commands
are named.*

Round 4 of six taking what is worth taking from
[`rootrootde/emu68hatcher`](https://github.com/rootrootde/emu68hatcher) (MIT).
The teardown ranks this **first** of its suggested order.

Today ART answers *"which disk is this?"* by opening the image and reading the
volume name out of the root block — never by trusting the filename, which
`core/osinstall/scan.rs`'s own module doc is careful to say. That is already
better than most tools. It is still not enough, and the gap has a name.

---

## 1. The gap, and what it has already cost

**`DiskDoctor`.** Hyperion never version-named it. Across the owner's AmigaOS
3.2 and 3.2.1 folders, **61 disks carry 60 distinct volume names** — that one
collision is the whole reason the layer mechanism exists in the shape it does,
and the reason a user must keep media in separate labelled folders so ART can
tell two disks apart. A volume name cannot answer "which `DiskDoctor` is this?"
because both disks answer `DiskDoctor`.

A content hash answers it without asking the user anything.

---

## 2. What was measured before this was designed

`CLAUDE.md`'s first rule is to verify from outside and to run the thing you are
about to depend on. Both were done, and the second **changed this document's
central decision**.

### 2.1 The owner's own disks against Hatcher's table: 35 of 35

35 ADFs at `E:\amiga\Amigatolon\paketler\3.2\AmigaOs 3.2\ADF`, MD5'd locally and
looked up in Hatcher's `install_media_hashes.yaml` (186 rows, fetched from the
repository). **Every one matched**, to the right `friendly_name` and the right
`source` (`Hyperion (3.2 base)`). Zero misses.

*(Note for the next reader: the teardown records the table at
`data/reference/install_media_hashes.yaml`. That is its path inside the Python
package; from the repository root it is
`src/main/python/emu68hatcher/data/reference/install_media_hashes.yaml`. The
recorded path 404s on `raw.githubusercontent.com`.)*

### 2.2 The table, measured rather than described

186 rows, **186 distinct hashes, no duplicates**.

| version | source | rows |
|---|---|---|
| 3.1 | Commodore | 25 |
| 3.1 | Escom | 20 |
| 3.1 | Cloanto | 12 |
| 3.2 | Hyperion (3.2 base) | 35 |
| 3.2.2 | Hyperion (3.2.2 Update) | 28 |
| 3.2.2 | Hyperion (3.2.2.1 Hotfix Pack) | 28 |
| 3.2.2.1 | Hyperion (3.2.2.1 Hotfix Pack) | 1 |
| 3.2.3 | Hyperion (3.2.3) | 30 |
| 3.9 | Haage and Partners | 7 |

Three properties fall out of it, and the design is shaped by all three:

- **The mapping is many-to-one.** One logical 3.1 disk has many dumps:
  `Workbench3_1` has **11** hashes, `Install3_1` 11, `Locale3_1` 10,
  `Storage3_1` 10, `Extras3_1` 9, `Fonts3_1` 6. A lookup keyed *hash → disk* is
  correct; anything assuming *disk → hash* is wrong.
- **The hash separates what the version string cannot.** Version `3.2.2` carries
  56 rows — 28 from the Update and 28 from the Hotfix Pack, **all with different
  hashes**. So the hash says which *pack* a disk came from where the version
  tag cannot. That is the `DiskDoctor` distinction, already solved in the data.
- **The source's own tagging is internally inconsistent.** The Hotfix Pack
  contributes 29 rows, of which 28 are tagged `version: 3.2.2` and one
  `version: 3.2.2.1`. §4.3 says what ART does about that.

### 2.3 What is *not* established

**TOSEC.** It publishes Amiga `.dat` catalogues with CRC32/MD5/SHA1 — 81,493
disks in the 2019-05-10 Amiga ADF release — but whether it catalogues AmigaOS
**install** media (as opposed to games, coverdisks and demos) was not settled by
research and **was not measured**. After §2.1 it is also off the critical path:
the owner's material is Hyperion-era and Hatcher's table covers it completely.
TOSEC stays a candidate second oracle for the 3.1 era, to be measured if and
when that matters. **Nothing in this round depends on it.**

---

## 3. The decision this round reverses, and why it is allowed to

`core/osinstall/identify.rs` carries a recorded refusal: a previous round
*"dropped a 186-row hash table that ART could not verify. So there is no table
here."* The teardown agreed and concluded **"take the idea; generate the
table."**

**The premise was that ART could not verify it. §2.1 verified 35 of its 186 rows
in one command.** So the refusal's reason does not hold, and its conclusion does
not follow.

Generating ART's own table instead would cover only what the owner happens to
own — 35 rows of 186 — and would throw away 151 rows about material this project
exists to support. Carrying the table and **saying which rows this machine has
confirmed** is strictly better than either alternative, and it is the same shape
ART already ships for Kickstarts: `core/rom/remus.rs` is a generated table that
`scripts/rom-table-check.py` re-verifies against amitools' Remus database on
every CI run (ART-104).

The refusal in `identify.rs` is updated in place, stating what changed and what
measured it — not deleted, so the next reader meets the reasoning rather than a
silent reversal.

**Licence:** Hatcher is MIT. The table ships with attribution in
`THIRD_PARTY_LICENSES.md`, added in the same commit that adds the data.

---

## 4. What this round builds

### 4.1 MD5, which `hashing.rs` already promises and does not have

`core/hashing.rs`'s module doc says *"MD5 is available only for compatibility
with historical databases — never as a security primitive."* **There is no MD5
in ART**: no function, no `md5`/`md-5` in `Cargo.toml`, and the only occurrence
of the string in `src-tauri/src/` is a historical remark in
`core/gameindex/record.rs`. The doc has been describing a capability the tree
does not have.

This round makes the sentence true. MD5 is the right hash here precisely because
it is *not* a security primitive: it is the key of a historical database, which
is the one use the doc already reserved for it. SHA256 stays ART's canonical
integrity hash and is not replaced.

### 4.2 The table as data, and a check script beside it

- The 186 rows become a compiled-in table under `core/osinstall/`, reviewable in
  a diff, unable to grow a code path of its own — the same shape
  `core/distro/`'s registry and the recipes already use.
- `scripts/media-table-check.py` takes a directory of the owner's real media,
  hashes it, and reports **verified / unverified / conflicting** per row. It is
  the sibling of `rom-table-check.py`. Like the other real-material scripts it
  is **not in CI**, because it needs media ART must never ship.
- A `#[ignore]`d Rust test takes the same directory through the same lookup, so
  the check exists on both sides — the pattern `round_trip_every_icon_in_a_folder_when_asked`
  already established.

### 4.3 What ART says about a match, and what it refuses to say

This is the part that matters, because this project's named failure class is a
confident wrong sentence.

- **A match is strong evidence and is stated with its source.** MD5 collision on
  a real ADF does not happen by accident. But the *claim* ART makes is about the
  row, not about the disk: **"this file matches the row Emu68 Hatcher's table
  calls `Workbench 3.2` (Hyperion 3.2 base)"**. That sentence is true whether or
  not the row is right, because it reports what was checked and where the claim
  came from. `CLAUDE.md`: *cite what you checked.*
- **A verified row says so.** Where the owner's own media has confirmed a row,
  ART may say so and name what confirmed it. Where it has not, ART says the row
  is unconfirmed. Two different sentences, never collapsed.
- **A non-match says nothing about identity.** Any modification, any different
  revision, any re-imaging breaks the hash. ART must never turn a miss into
  *"this is not X"*, and must never let a miss weaken what the volume name
  already established. Hash identification is **additive** to the existing
  name-based path, never a replacement for it.
- **ART does not re-derive a version from the row.** §2.2's third property is the
  reason: the source's `version` and `source` fields disagree about the Hotfix
  Pack. ART reports both fields as the row states them and lets the user see the
  disagreement, rather than picking one and presenting the result as its own
  finding.

### 4.4 An index of the owner's own material — *"ne nerede duruyor"*

The owner's stated purpose is not only a verdict per file: it is to know **what
is where**. So the round keeps an index — file, size, identity, hash, and what
the table made of it — and that index, not a re-hash, is what later questions
are answered from.

- It is built on `core/osinstall/scan_cache.rs`, which already keys on
  `MediaIdentity` (`path`, `size`, `mtime_nanos`, ART-188) and already has
  `in_dir` / `lookup` / `store` / `forget_all` / `sweep`. **Not a second
  mechanism** — this codebase has a recorded defect (ART-249) where two copies
  of one job shared an identical wrong constant and every round-trip test passed.
- **Hashing is not free and must not be repeated.** `scan_cache`'s own doc
  records that hashing 468 MB costs about what the walk costs — which is why a
  content hash was refused for *change detection*, and that refusal still
  stands. Change detection stays `(path, size, mtime)`. The hash is computed once
  per identity and cached against it, never per render and never per plan.
- `core/` does not choose where the index lives. The command layer hands it a
  directory, the way `crate::scratch::root()` already governs every staging site
  (ART-196, and its sweep is blocking in CI).

---

## 5. What this round does not do

- **It does not retire the layer mechanism.** Layers still say which release a
  component belongs to and in what order updates apply. What the hash retires is
  the part where the *user* must keep media in separate labelled folders so ART
  can tell two same-named disks apart.
- **It does not fetch.** No download of media, no download of the table at
  runtime. The table is compiled in; the check script is run by a person.
  `CLAUDE.md`'s line stands and this round does not touch it.
- **It does not replace name-based identification.** §4.3.
- **It does not ship TOSEC**, or depend on it — §2.3.
- **It does not use MD5 for anything but table lookup.** SHA256 remains the
  integrity hash for verification, duplicates and snapshots.

---

## 6. Verification

- Unit tests over the lookup: a hash in the table, a hash that is not, a disk
  with several dumps (`Workbench3_1`'s 11), and the two rows that disagree about
  the Hotfix Pack's version.
- **A test that the table's own invariants hold**: 186 rows, 186 distinct
  hashes, every row carrying the fields the lookup reads. A table is data, and
  data drifts.
- The real-material script and its `#[ignore]`d twin (§4.2), with the owner's
  own 35 disks as the standing measurement — **35 of 35 today**, so a regression
  is visible as a number rather than as a feeling.
- **Mutation, per `CLAUDE.md`**: every guard gets its defect put back and is
  watched to fail, with the actual failure message quoted. The trap to expect
  here is asserting a state with more than one cause — "no identification" is
  reachable by a missing row, an unreadable file, and an index miss, and those
  are three different sentences.

## 7. Known risks

- **The table is somebody else's and can change under ART.** That is what the
  check script is for, and why ART reports the row's own fields rather than
  conclusions drawn from them.
- **151 of 186 rows are unconfirmed on this machine.** ART must say so rather
  than presenting all rows with equal confidence — §4.3.
- **A re-imaged or patched disk is a legitimate miss.** The user has a good disk
  and no row. The screen must make that read as "not in the table", never as
  "not genuine".
