# Feature Matrix

What is actually built, where it lives, and what proves it.

Spec §10 and §89 are binding here: **never claim support until it is
implemented and tested.** A row may only be marked ✅ if there is working code
*and* a test. If you are tempted to write "mostly works", it is 🟡.

| | Meaning |
|---|---|
| ✅ | Implemented and covered by tests |
| 🟡 | Partly implemented — the gap is named in the row |
| 🔩 | Stub only: the module exists and returns `NotImplemented` |
| ⏳ | Not started |
| — | Not applicable |

Scheduling lives in [STATUS.md](STATUS.md); defects live in [ISSUES.md](ISSUES.md).

---

## Format support

Mirrors the spec §10 table. "Detect" means ART can identify the format;
the other columns mean it can act on it.

| Format | Detect | Read | Create | Edit | Validate | Convert |
|--------|:------:|:----:|:------:|:----:|:--------:|:-------:|
| **ADF** (DD, OFS/FFS) | ✅ | ✅ | ✅ | ✅ | ✅ | ⏳ |
| **ADF** (HD) | ✅ | ✅ | ⏳ | ✅ | ✅ | ⏳ |
| **ADZ** | 🟡 | ⏳ | ⏳ | — | ⏳ | ⏳ |
| **DMS** | 🟡 | ⏳ | — | — | ⏳ | ⏳ |
| **HDF** | ✅ | ✅ | ✅ | ✅ | 🟡 | ⏳ |
| **HDZ** | 🟡 | ⏳ | ⏳ | — | ⏳ | ⏳ |
| **LHA** | ✅ | ✅ | ⏳ | ⏳ | 🟡 | — |
| **ROM** | ✅ | ✅ | — | — | ✅ | — |

Notes:

- 🟡 **Detect** = extension-only, no signature check. Confidence is reported
  honestly (`core/detect.rs`), so nothing is claimed that was not verified.
- **ADF (HD)** — reads, writes and validates. The DD-only assumption that made
  these fail was ART-037/038; geometry now comes from the image. Edit is
  proven at 3520 blocks through the write gate
  (`an_hd_floppy_is_written_through_the_gate`); create still only produces DD
  images.
- 🟡 **LHA validate** — structure and path safety are enforced; per-entry CRC
  checking is not implemented.
- **ADZ / HDZ / DMS** decompression is Stage 5 work; `xdms-rs` is the candidate
  for DMS (see `ART-kaynak-listesi.md`).

## Filesystems

| Filesystem | Read | Write | Notes |
|---|:---:|:---:|---|
| **OFS** | ✅ | ✅ | Data blocks carry the 24-byte header |
| **FFS** | ✅ | ✅ | Raw data blocks |
| **FFS INTL** | ✅ | ✅ | Case folding honoured in name hashing ([ART-010](ISSUES.md#fixed)) |
| **Dircache** | 🟡 | ⏳ | Read-only on purpose: a directory is stored twice, and writing one copy corrupts listings on a real Amiga (§3.4) |
| **FFS/OFS on HDF partitions** | ✅ | ✅ | Stage W: multi-block bitmaps, journalled in-place writes |
| **PFS3** | ⏳ | ⏳ | Listed with its name and "not supported yet"; reference: `pfs3aio` |
| **SFS** | ⏳ | ⏳ | Listed with its name and "not supported yet" |

---

## Modules

### Core engine

| Feature | Spec | State | Code | Tests |
|---|---|:---:|---|---|
| Format detection | §4 | ✅ | `core/detect.rs` | ✅ |
| SHA256 hashing (streaming) | §58 | ✅ | `core/hashing.rs` | ✅ |
| Atomic writes | §57 | ✅ | `core/safety/atomic.rs` | ✅ |
| Generational backups | §57, §93 | ✅ | `core/safety/backup.rs` | ✅ |
| Path-traversal defence | §56 | ✅ | `core/security/path.rs` | ✅ |
| Workflow Engine + catalogue | §5, §46, §91 | ✅ | `core/workflow/` | ✅ |
| Job Queue (progress + cancel) | §54, §55 | ✅ | `core/jobs/`, `commands/jobs.rs` | ✅ |
| Operation Log | §53 | ✅ | `core/oplog/` | ✅ |
| Error IDs (`ART-*`) | §68 | ✅ | `core/error.rs` | ✅ |
| Compatibility Engine | §34 | 🔩 | `core/compatibility.rs` | — |
| Generic validation surface | §69 | 🔩 | `core/validation.rs` | — |

### ADF Studio

| Feature | Spec | State | Code |
|---|---|:---:|---|
| Open, browse, volume info | §11 | ✅ | `core/adf/{mod,fs,blocks}.rs` |
| Extract file | §11 | ✅ | `core/adf/extract.rs` |
| Add / delete / rename / mkdir | §11 | ✅ | `commands/adf.rs` → `core/volume/write/` |
| Create blank / formatted / bootable | §11 | ✅ | `core/adf/create.rs`, `core/adf/bootcode.rs`. Until [ART-063](ISSUES.md#fixed) was fixed and verified by booting `test/art-bootable-test.adf`, `bootable` wrote a valid-looking but non-functional boot block (a bare `RTS`) — the boot block now reaches a CLI prompt on a real Kickstart/Workbench (not Workbench itself: `S/Startup-Sequence` and AmigaOS's own commands are content ART cannot supply) |
| Validate (boot block, checksums, bitmap) | §11, §12 | ✅ | `core/adf/validate.rs` |
| Optimisation analysis | §13 | ⏳ | — |
| Drag files in / out of the image | §11, §90 | ✅ | `/files` two-pane manager |

### LHA Studio

| Feature | Spec | State | Code |
|---|---|:---:|---|
| Open, list entries | §14 | ✅ | `core/lha/mod.rs` |
| Safe extraction (traversal + bomb guard) | §14, §56 | ✅ | `core/lha/safe_extract.rs` |
| Overwrite policy | §89 | ✅ | `core/lha/safe_extract.rs` |
| WHDLoad detection with confidence | §15 | ✅ | `core/lha/whdload.rs` |
| WHDLoad pack layout (drawer, slave, icon) | §82 | ✅ | `core/whdload/mod.rs` |
| Per-entry CRC test | §14 | ⏳ | — |
| Create archive (lh5 compression) | §14 | ⏳ | — |
| One-click WHDLoad install to HDF | §14, §82 | ✅ | `core/whdload/`, `commands/whdload.rs`, `/whdload`; cross-checked against amitools |
| Install an archive into an ADF | §14, §41.5.6 | ✅ | `core/sources/install.rs` |

### Hard Disk / RDB

| Feature | Spec | State | Code |
|---|---|:---:|---|
| Open HDF, report geometry | §16 | ✅ | `core/hdf.rs` |
| Create HDF with RDB (sparse) | §16 | ✅ | `core/hdf.rs`, `core/rdb.rs` |
| RDB parse, checksum, partitions | §18, §19 | ✅ | `core/rdb.rs` |
| Partition layout validation | §18, §89 | ✅ | `core/rdb.rs` |
| Browse partition contents | §17 | ✅ | `core/volume/`, `commands/volume.rs` |
| Write into a partition | §17, §57 | ✅ | `core/volume/write/` — OFS/FFS/INTL. Dircache and long-filename volumes stay read-only, with the reason |
| Undo journal for large images | Stage W §2 | ✅ | `core/volume/journal.rs`; recovery offered at mount, proved by a real process-kill test |
| Edit partition contents | §17 | ✅ | `/files`: copy, rename, move, delete, mkdir, attributes |
| Resize, clone, repack, migrate | §21–§24 | ⏳ | — |
| Snapshots | §49 | ⏳ | — |

Audited in Stage 3; see [ART-021 … ART-027](ISSUES.md#fixed) for what was
wrong. Stage R made partition contents reachable: `BlockDevice` +
`VolumeGeometry` let the existing FFS/OFS code read any geometry, so an HDF
partition browses like a floppy. Cross-validated against `amitools` — which is
how [ART-032 … ART-035](ISSUES.md#amiga-format-compatibility-stage-r-oracle)
were found.

Stage W added writing. Two strategies behind one API: an image of 16 MiB or
less is read whole, mutated in memory, validated and written back atomically —
the audited ADF pipeline, unchanged, so every floppy takes exactly the path it
always did. Anything larger is written **in place under an undo journal**,
because backing up two gigabytes before every rename is not a cost anyone
should pay per keystroke.

The write matrix, and why:

| DosType | Write | Why not |
|---|:---:|---|
| `DOS\x00`, `DOS\x01` — OFS/FFS | ✅ | |
| `DOS\x02`, `DOS\x03` — INTL | ✅ | Hashing follows the volume's own flag ([ART-010](ISSUES.md#fixed)) |
| `DOS\x04`, `DOS\x05` — dircache | ⏳ | A directory is stored twice; writing one copy leaves listings that look right in ART and wrong on a real Amiga |
| `DOS\x06`, `DOS\x07` — long filenames | ⏳ | A different directory layout, not a flag; reading is refused too |
| PFS3, SFS | ⏳ | Not mounted at all — listed by name with the reason |
| Bitmap marked invalid | ⏳ | Allocating from a map that may not describe the disk is how two files come to own the same block |

Everything above is cross-checked against `amitools` in both directions —
ART writes and amitools reads, amitools writes and ART reads — on FFS, OFS, a
64 MiB volume with 33 bitmap blocks, and protection bits. Crash recovery is
proved by a test that really kills the process at three points mid-write and
checks the image comes back byte for byte.

### Emulation & hardware

| Feature | Spec | State | Code |
|---|---|:---:|---|
| WinUAE detection | §35 | ✅ | `core/winuae.rs` |
| `.uae` config generation | §35 | ✅ | `core/winuae.rs` |
| Launch session | §35 | ✅ | `core/winuae.rs` |
| Machine profiles (presets) | §33 | 🟡 | `core/profile.rs` — presets only, no user-defined profiles |
| Kickstart ROM identification | §32 | ✅ | `core/rom.rs` |
| Gotek scan + FlashFloppy config | §37, §39 | ✅ | `core/gotek.rs` |
| Gotek bulk workflow | §38 | ⏳ | — |
| PiStorm config inspect/edit | §40 | ✅ | `core/pistorm.rs` |
| Raw device writes | §57 | ⏳ | deliberately absent until double-confirm UI exists |

### Collection & analysis

| Feature | Spec | State | Code |
|---|---|:---:|---|
| Folder scan, TOSEC metadata parse | §41, §42 | ✅ | `core/collection.rs` (runs as a job) |
| Multi-disk grouping | §41 | ✅ | `core/collection.rs` |
| SQLite persistence, tags, favourites | §41 | 🟡 | schema exists; UI does not write to it |
| Duplicate detection (SHA256) | §43 | 🟡 | hashing works; no dedupe view |
| Hex viewer (read-only) | §31 | ✅ | `core/analysis.rs` |
| Signature scanning | §28 | ✅ | `core/analysis.rs` |
| Disk Analyzer / forensics | §28 | 🟡 | hex + signatures only |
| Image compare | §27 | ⏳ | — |
| Binary / Hunk inspector | §30 | 🔩 | `core/binary.rs` |
| Recovery Lab | §29 | 🔩 | `core/recovery.rs` |
| Image conversion | §26 | 🔩 | `core/conversion.rs` |

### Application shell & UX

| Feature | Spec | State | Notes |
|---|---|:---:|---|
| Universal Drag & Drop | §4 | ✅ | one global listener, `lib/dnd.ts` |
| "What can I do?" panel | §46, §91 | ✅ | driven by the engine's plan |
| Dashboard, recent files | §62 | ✅ | |
| Settings (theme, paths, language) | §59 | ✅ | |
| i18n architecture | — | 🟡 | English and Turkish, 863 keys each, chosen in Settings and remembered; `CoreError` and `WhdloadRefusal` sentences from Rust still reach the UI in English regardless of the chosen language ([ART-060](ISSUES.md#open)) |
| Dark / light theme | §61 | ✅ | |
| Beginner / Power User mode | §47, §48 | ✅ | `lib/uxmode.ts`; hides advanced studios, actions and block detail |
| Operation history + export | §53 | ✅ | Settings → Operation Log |
| Workflow Wizard | §45 | ⏳ | — |
| Two-pane file manager | §11, §17 | ✅ | `/files`, Total Commander-styled (row icons, file-type colour, Attr column); F3 view · F4 edit · F5 copy · F6 rename/move · F7 mkdir · F8 delete · F9 attributes. Any volume to any volume for a single entry; see Multi-select below for what a selection can and cannot do |
| Multi-select (Shift/Ctrl-click, Insert, Ctrl+A) | Roadmap 1.1 | ✅ | `src/lib/selection.ts` (pure reducer) + `SelectionBar.tsx`; real pane focus (`focused: Side`) and Tab between panes |
| Batch copy: local ↔ volume | Roadmap 1.1 | ✅ | `volume_plan_copy_many` / `volume_copy_in_many` (host→volume, one job, cancel commits nothing) — `commands/volume_write.rs`. Volume→local multi-select works but is several concurrent per-entry operations, not one atomic job ([ART-065](ISSUES.md#open)); volume→volume multi-select refuses outright, with no primitive to batch on ([ART-064](ISSUES.md#open)) |
| Batch delete | Roadmap 1.1 | ✅ | `volume_delete_many`; a batch that can't fully succeed (missing name, non-empty directory) deletes nothing |
| Several archives installed to a disk at once | Roadmap 1.1 | ✅ | `archives_plan_install` / `archives_install` (`commands/archives.rs`); each archive gets its own drawer, staged into one write so a cancelled batch can't leave two games half-installed. The plan step runs synchronously on the command thread rather than as a job ([ART-066](ISSUES.md#open)), and Stop is unresponsive during one archive's own extraction ([ART-067](ISSUES.md#open)) |
| One listing order + per-pane column sort | Roadmap 1.2 | ✅ | `commands/panel.rs`, `core/adf/fs.rs`, `core/volume/write/dir.rs::entries_in` now share one folders-first, case-insensitive floor; `src/lib/sort.ts` sorts on top of it by name/size/date, click-to-reverse |
| Filename mask filter | Roadmap 1.2 | ✅ | `src/lib/mask.ts` — Total Commander `*`/`?` wildcards, whole-name, case-insensitive; narrows what a pane shows and clears its selection on change. The empty-vs-no-match message is inferred from two entry counts rather than carried as a flag from the matcher ([ART-068](ISSUES.md#open)) |
| Checkout / checkin (F4) | Stage W §6 | ✅ | `core/volume/checkout.rs`; SHA-256 gated, CRLF offered never applied |
| `.uaem` sidecars | Stage W §4.2 | ✅ | `core/volume/write/uaem.rs`, WinUAE's format; round-trip test pins `HSPARWED` |
| `.info` pairing | Stage W §7.1 | ✅ | Rename and delete offer the icon; the copy plan warns when a pair is split |
| Attributes editor | Stage W §7.2 | ✅ | All eight bits with explanations; Power edits, Beginner reads |
| Explorer drag-out | §17, §90 | ✅ | `tauri-plugin-drag`, local files only |
| File associations | §59 | ⏳ | requires explicit consent flow |
| Accessibility (keyboard, focus, contrast) | §64 | 🟡 | not audited |

### Spec addenda

Both are designed — [design-software-sources.md](design-software-sources.md),
[design-ai-layer.md](design-ai-layer.md). Scheduling is in
[STATUS.md](STATUS.md#stage-plan).

| Feature | Spec | State |
|---|---|:---:|
| Aminet catalog sync / search / fetch | §41.5 | 🟡 |
| Aminet install to HDF | §41.5 | ✅ | `sources_install_volume` — same unpack as the ADF path, then a Stage W folder copy |
| Aminet update view | §41.5.6 | ✅ | `core/sources/installed.rs`; compares the recorded download against the catalogue |
| AI read-only assistant | §45.5 Stage A | ⏳ |
| AI plan generation + Plan Cards | §45.5 Stage B | ⏳ |
| AI full scenarios | §45.5 Stage C | ⏳ |

The Aminet rows are ✅ apart from the AI layer. What is built and tested: Built and tested:
`core/sources` — index and readme parsing, catalog store, search, version
resolution, mirror failover, the download trust pipeline and catalog sync (122
tests); `net/http_mirror.rs`, the `ureq`-backed transport (16 tests);
`commands/sources.rs` (6 tests); `src/lib/sources.ts` and Aminet Studio at
`/aminet`. Core tests run against an in-memory mirror and the transport tests
against a localhost socket, so CI never leaves the machine.

The index format, the mirror defaults and the whole pipeline were verified
against live Aminet mirrors on 2026-08-09 — which is how
[ART-030 and ART-031](ISSUES.md#software-sources-aminet-415) were found.

Since then: sorting (six orders) and filters (name-only, age, size range, file
type) in the catalogue; a **user-chosen download folder** with subfolders and
the ability to move a download afterwards; installing a downloaded package into
an ADF; and a two-pane file manager at `/files` with drag & drop.

Also since then: **"Show in Collection"** hands the download folder to the
Collection screen, which accepts a folder through router state the way the
other studios do; the **mirror list is editable** in Power User Mode — reorder,
edit, add, remove, and `sources_reset_mirrors` to go back to the shipped list;
and both the download folder and any custom mirror order are **remembered
across launches** (they lived only in process memory before, so every restart
silently reset them).

Stage A is complete. The **update view** compares each recorded download
against the catalogue as it stands now — Aminet's index has no version column,
so "newer" means the entry changed since the download, and the panel says which
signal fired rather than showing a bare badge. **Install to HDF** unpacks with
the same extractor the ADF path uses and then copies the unpacked folder in
with the Stage W writer: one install path, two destinations.

What remains under §41.5 is the AI layer (§45.5), which is designed
(`docs/design-ai-layer.md`) and not built.

Note on "Show in Collection": ART's Collection indexes a folder, it is not a
database rows are added to. The action therefore indexes the download folder
rather than pretending to file one package away somewhere — §41.5.6's intent,
honestly implemented against the Collection that exists.

---

## Keeping this honest

When you finish a feature:

1. Move its row to ✅ **only** once a test exists.
2. If the UI can reach it, check the workflow catalogue
   (`core/workflow/builtin.rs`) — an action registered `available: false`
   should be flipped to `true` in the same change.
3. Update the format table if the change alters what ART can claim to support.
4. Add a line to the session log in [STATUS.md](STATUS.md).
