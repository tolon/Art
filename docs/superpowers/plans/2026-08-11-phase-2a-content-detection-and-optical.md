# Phase 2a — Content-First Detection and Optical Images

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** ART opens `.adf`, `.hdf`, `.img`, `.iso`, `.dsk`, `.raw`, `.zip`, `.7z`, `.lha`, `.d64`, `.d71`, `.d81` and `.t64` in the commander, deciding what each one *is* from its contents rather than its filename, and browsing every one of them as a pane. `.tap`, `.prg` and `.crt` are identified but not browsable — a TAP holds a raw tape signal, not a directory.

**The unifying idea:** a floppy image, a hard-disk partition, a CD and an archive are all the same thing to a commander — **a container you can walk into, list, and copy out of**. ART already has that model for ADF and HDF (`PaneState`, where an ADF is simply volume 0). This phase adds five more container kinds behind it — a CD, two archive formats, and Commodore 64 disk and tape archives — rather than five more screens.

**Architecture:** Two halves that must land in this order. First `detect()` stops dispatching on the extension and reads the first blocks instead — that single change makes `.img` and `.dsk` resolve to whatever they actually contain, and turns every later format into one signature rather than one more branch in a growing match. Then an ISO9660 reader, exposed through the same `CopySource`-shaped read path the commander already uses for ADF and HDF volumes, so browsing a CD is the same code as browsing a floppy.

**Tech Stack:** Rust (`core/`), React 18 + TypeScript. Two new crates, both in Task 4 and both MIT/Apache: `zip` (or `flate2`) and `sevenz-rust`. Tasks 1–3 add none — ISO9660 is simple enough that a crate would cost more in audit than it saves.

## Global Constraints

- `src-tauri/src/core/` is platform-independent: `std` + `serde` + `sha2` + `thiserror` + `delharc` — and, from Task 4, `zip` and `sevenz-rust2`. Never `use tauri`, never call Windows APIs, never touch the network. **Tasks 1–3 add no dependency.** Task 4 adds two decompressors and must update CLAUDE.md's list and `THIRD_PARTY_LICENSES.md` in the same commit — a crate in `Cargo.toml` and not in the licence file is a licensing defect. Each one parses hostile input inside the process, so it is a real decision, not a convenience.
- ~~**MSRV is 1.77.**~~ **MSRV is 1.93 from 2026-08-12** (Task 4 Step 3, at the user's decision): the maintained `sevenz-rust2` requires it, and the newest release that still built on 1.77 was fourteen minor versions behind — an old LZMA decoder is the wrong thing to point at untrusted files. CI builds on `stable`, so nothing pinned an older compiler. Clippy's suggestions follow the MSRV, so the bump switched on lints that had been suppressed.
- Release profile is `panic = "abort"`. **Every byte in this phase comes from an untrusted file**, so: never index directly, never allocate from a length field a file supplied (`checked_add` / `checked_mul` on running totals, and a hard cap), and bound every walk with a step limit. A malformed ISO must produce a `CoreError`, never a panic and never an infinite loop.
- **Never read a whole image into memory.** An ISO is routinely 700 MB and a DVD image 4.7 GB. Read the descriptors and the directory extents you need, the way `open_hdf` reads a 1 MB window rather than the file.
- Names that come from a disc or an archive go through `core/security/path.rs::safe_join()` before becoming a path on the host. An ISO can contain `../` in a Rock Ridge `NM` entry.
- Errors reaching the UI are readable sentences carrying a stable `ART-*` id from `CoreError::code()`.
- Commands are registered in **both** `lib.rs`'s `invoke_handler![]` and a typed wrapper in `src/lib/*.ts`. Components never call `invoke` directly.
- **Every user-visible string goes in both `en.json` and `tr.json` in the same commit.** `pnpm test` enforces identical key sets, no empty values, matching interpolation variables, and that every literal `t("…")` key resolves. `src/lib` helpers return `Phrase { key, params? }` and never import the i18n singleton.
- Fixtures are **synthetic and generated at runtime in a tempdir**. **ART ships no copyrighted Amiga content, ever** — every test ISO is built byte by byte by the test.
- Never claim support that is not implemented and tested (spec §10, §89).

**Gates — every task ends green on all of these, from `amiga-retro-toolkit`:**

```
pnpm lint
pnpm test
cd src-tauri && cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test
python scripts/oracle-check.py
```

**Baseline at the start of this plan: 731 Rust tests, 107 frontend tests, oracle 48 checks** — Phase 1a's closing numbers (STATUS.md snapshot, 2026-08-11). An earlier draft of this line said 721/76; the gate arithmetic in the task reports is against the real baseline. **Run `cargo test` twice** — ART-059 was a race that failed about one run in five.

**Where this plan stands (2026-08-12):**

| | |
|---|---|
| Tasks 1, 2, 3 | complete — content-first detection, the ISO9660 reader, the disc pane. Task 3's review fixes landed after a mid-session reboot (`80e1d40`) |
| Task 3a | complete (`5be5529`) — the 7-Zip oracle, added by amendment A1 |
| Task 3b | complete (`787fe15`) — Mode 2/XA Form 1, which closed ART-075 |
| Task 4 | complete — the shared gate (`efaaf00`), ZIP (`bf5e577`), 7z + the MSRV bump (`b07ad1c`), the archive pane with its virtual tree (`9c4e578`) |
| Tasks 5, 6 | not started — **Task 5 (D64/D71/D81/T64) is the next work** |

See the Amendments section at the foot of this file for what changed after the
plan was written.

---

## What already exists

| Capability | Where |
|---|---|
| A 4-byte head read, then a signature check, then extension fallback | `core/detect.rs:136-166` — the shape is already right |
| One pane abstraction over ADF and HDF | `PaneState` in `src/pages/FileManager.tsx`; an ADF is volume 0 |
| Recursive copy out of a volume | `core/volume/write/copy.rs`, `volume_copy_out` |
| Bounded reads with a window | `open_hdf` |
| Total Commander presentation | `FileManager.css`, `TcIcon.tsx`, `panelName.ts`, `tcFormat.ts` |

`detect()` currently reads four bytes to spot an LHA header and otherwise matches on the extension (`"adf"`, `"hdf"`, `"rom"`, …). `hdf_by_extension` carries the comment *"HDF detection is extension-only for Phase 0: real RDB/header inspection arrives in Phase 4."* This phase is that arrival.

---

## Task 1: Decide from the content, not the name

**Files:**
- Modify: `src-tauri/src/core/detect.rs`
- Test: same file

**Interfaces:**
- Consumes: `read_head(path, n)`, already in the module.
- Produces: `FormatCategory::OpticalImage`, and a `probe_at(path, offset, len)` helper for signatures that do not live at byte 0. Task 3 consumes both.

**The signature table.** Check in this order; the first match wins, and each carries high confidence because it read the bytes:

| Evidence | Result |
|---|---|
| `DOS` + a byte `0x00`–`0x07` at offset 0 | `FloppyImage` or `HarddiskImage`, by size |
| `RDSK` at offset 0 | `HarddiskImage`, `format_hint: "rdb"` |
| `CD001` at offset **0x8001** | `OpticalImage`, `format_hint: "iso9660"` |
| `CD001` at offset **0x9311** | `OpticalImage`, `format_hint: "iso9660-raw"` — a 2352-byte raw track, Mode 1 |
| `CD001` at offset **0x9319** | `OpticalImage`, `format_hint: "iso9660-raw-xa"` — a 2352-byte raw track, Mode 2/XA Form 1 (**added by amendment A2**; built in Task 3b, not here) |
| `PFS\x03` or `PDS\x03` at offset 0 | `HarddiskImage`, `format_hint: "pfs3"` |
| `SFS\x00` at offset 0 | `HarddiskImage`, `format_hint: "sfs"` |
| `-l` + method (existing) | `Archive` |

The `DOS` case needs a size decision: 901,120 → DD floppy; 1,802,240 → HD floppy; anything larger → a single-volume hard disk image. Use the existing size constants rather than new literals.

**Why 0x9311.** A raw CD track stores 2352 bytes per sector, of which 2048 are data at offset 16 within the sector. Sector 16 therefore begins at `16 × 2352 = 0x9300`, its data at `0x9310`, and the `CD001` at data offset 1 lands at `0x9311`. Finding it there also proves the sector size, which Task 2 needs.

- [x] **Step 1: Write the failing tests**

One per signature, each building a synthetic file at runtime with the bytes at the right offset and nothing else meaningful. Plus these, which are what make it a *content-first* detector rather than a longer extension list:

```rust
#[test]
fn an_img_holding_a_floppy_is_a_floppy_not_an_unknown() { /* .img, DOS\0 at 0, 901_120 bytes */ }

#[test]
fn an_adf_that_is_really_an_iso_is_reported_as_an_iso() { /* .adf name, CD001 at 0x8001 */ }

#[test]
fn a_file_shorter_than_the_signature_offset_is_not_a_panic() { /* 100 bytes, .iso name */ }
```

That last one matters more than it looks: probing at 0x8001 in a 100-byte file is exactly the shape of bug `panic = "abort"` turns into a dead application.

- [x] **Step 2: Run them and watch them fail**

- [x] **Step 3: Implement**

Add `probe_at`, and put the signature checks **before** the extension `match`. Leave the extension match in place as the fallback it now genuinely is — a `.adf` full of zeros is still most likely an ADF, and that is a hint, not a lie. Lower the extension-only confidences to reflect that they are now the weaker evidence.

- [x] **Step 4: Run them and watch them pass, then the whole suite twice**

- [x] **Step 5: Commit**

---

## Task 2: An ISO9660 reader

**Files:**
- Create: `src-tauri/src/core/iso/mod.rs`, `src-tauri/src/core/iso/descriptor.rs`, `src-tauri/src/core/iso/directory.rs`
- Modify: `src-tauri/src/core/mod.rs`

**Interfaces:**
- Consumes: nothing from Task 1 — this is pure format work and can be reviewed on its own.
- Produces:

```rust
pub struct IsoImage { /* holds the path and the sector layout, not the bytes */ }

pub struct IsoEntry {
    pub name: String,
    pub is_dir: bool,
    pub bytes: u64,
    pub extent: u32,      // starting LBA
    pub date: Option<i64>, // unix seconds
}

impl IsoImage {
    pub fn open(path: &Path) -> CoreResult<Self>;
    pub fn volume_name(&self) -> &str;
    pub fn list(&self, extent: u32, length: u32) -> CoreResult<Vec<IsoEntry>>;
    pub fn root(&self) -> (u32, u32);
    pub fn read_file(&self, extent: u32, bytes: u64) -> CoreResult<Vec<u8>>;
}
```

**The format, only as much as this task needs.**

- Sectors are 2048 bytes of data. In a raw image they are 2352 bytes with the data at offset 16 — Task 1 already worked out which, so carry the sector layout rather than re-deriving it.
- **Volume descriptors** start at sector 16 and run until a terminator. Each is 2048 bytes: type byte at 0, `CD001` at 1..6, version at 6. Type 1 is the Primary Volume Descriptor, type 2 a Supplementary (Joliet), type 255 the terminator.
- In the PVD: the volume identifier is 32 bytes at offset 40, and the **root directory record** is 34 bytes at offset 156.
- A **directory record**: length at 0 (zero means "no more records in this sector, skip to the next"), extent LBA as a little-endian u32 at 2, data length as little-endian u32 at 10, a 7-byte recording date at 18, flags at 25 (bit 1 set = directory), identifier length at 32, identifier from 33.
- Identifiers `0x00` and `0x01` are `.` and `..` — skip both. A file identifier normally ends `;1`, a version suffix to strip.
- **Joliet**: a Supplementary descriptor whose escape sequence at offset 88 is `%/@`, `%/C` or `%/E`. Its names are **UCS-2 big-endian**. Prefer it when present, because it is where the real filenames live on any disc that has one.

**Bounds, and why each one:**
- Cap the descriptor scan (32 is generous; a disc with more is malformed).
- Cap directory recursion depth (ISO9660 says 8; allow more but bound it).
- Cap entries per directory and total entries, the way `MAX_COPY_ENTRIES` does.
- `read_file` must refuse a length that would allocate more than a sane ceiling, and must not trust `bytes` past the end of the file.
- A record length of zero must advance to the next sector, **not** loop.

- [x] **Step 1: Write a synthetic ISO builder in the test module**

This is the task's real foundation. A function that assembles a valid minimal ISO in memory — PVD, terminator, a root directory with a couple of entries, and file data — so every test builds its own disc. **No ISO file is ever committed.** Build it for both 2048-byte and 2352-byte sector layouts.

- [x] **Step 2: Write the failing tests**

```rust
#[test]
fn a_minimal_iso_reports_its_volume_name_and_root() { }

#[test]
fn a_directory_lists_its_entries_without_dot_and_dotdot() { }

#[test]
fn a_file_reads_back_byte_for_byte() { }

#[test]
fn a_joliet_disc_prefers_its_unicode_names() { }

#[test]
fn a_raw_2352_byte_image_reads_the_same_as_a_2048_byte_one() { }

#[test]
fn a_record_length_of_zero_moves_to_the_next_sector_rather_than_looping() { }

#[test]
fn a_directory_claiming_a_length_past_the_end_of_the_file_is_an_error() { }

#[test]
fn a_deeply_nested_disc_stops_at_the_depth_limit() { }
```

The last three are the security cases and are **not optional**.

- [x] **Step 3: Run them, watch them fail, implement, watch them pass**

- [x] **Step 4: Whole suite twice, then commit**

---

## Task 3: Open a disc in the commander

**Files:**
- Create: `src-tauri/src/commands/iso.rs`
- Modify: `src-tauri/src/lib.rs`
- Create: `src/lib/iso.ts`
- Modify: `src/pages/FileManager.tsx`

**Interfaces:**
- Consumes: `IsoImage` from Task 2, `FormatCategory::OpticalImage` from Task 1, and the existing `PaneState` shape.
- Produces: `iso_open(path)`, `iso_list(path, extent, length)`, `iso_extract(path, extent, bytes, dest)`.

**The pane.** `PaneState.kind` gains `"iso"`. A disc is **read-only** — the pane must make that plain and every write action must be refused, not merely hidden. `volumeWriteCapability` is the established pattern for a read-only volume; follow it rather than inventing a second one.

**Navigation.** An ISO directory is addressed by `(extent, length)`, not by a block number, so `PaneState`'s `dirBlock`/`trail` need an ISO-shaped equivalent. Keep the two clearly separate rather than overloading `dirBlock` with a meaning it does not have.

- [x] **Step 1: The commands, with tests**

Each command is a thin adapter: deserialize, call core, serialize back. The tests belong to Task 2's core module; these need only the routing cases — a path that is not an ISO is an error, and a bad extent is an error rather than a panic.

- [x] **Step 2: The typed wrappers and the pane**

- [x] **Step 3: F5 out of a disc**

Copying **from** an ISO to a local folder or to an Amiga volume must work, because that is the whole point of opening one — an AmigaOS install CD is useful when its contents reach an HDF. Reuse `copy_into_volume` by giving it a `CopySource` backed by `IsoImage`, the same way `HostSelection` spans several roots. **Do not write a second copy engine.**

- [x] **Step 4: Refuse writing into a disc, visibly**

- [x] **Step 5: Gates and commit**

---

## Task 3a: An independent oracle for the disc reader (amendment A1)

**Files:**
- Create: `scripts/iso-oracle-check.py`
- Modify: `docs/testing.md` (how to run it, and what it does *not* prove)

**Why this task exists.** Task 2's synthetic ISO builder and the reader were
written from the same offsets, so they can agree and both be wrong — the exact
failure mode that let ART-032 … ART-035 ship behind a green suite. The plan's
original third rung was a real AmigaOS CD in front of the reader; that is
**cancelled** (see A1), because assuming the developer or a user reliably owns
licensed AmigaOS media was unrealistic. This replaces it with a rung that
needs no Amiga and no OS driver.

**7-Zip is the oracle.** `7z l` reads ISO9660 and Joliet with its own
implementation, in user space. The script builds the synthetic fixtures to a
temp path and diffs what `IsoImage` reports against what 7-Zip lists.

- [x] **Step 1: Build the fixtures from Rust, read them from Python**

The fixture builder lives in `core::iso::fixture` and is already the one the
tests use. *As built:* no new entry point was needed — the amitools oracle's
existing shape fits exactly. `export_iso_for_oracle_when_asked` writes the
sample discs when `ART_ISO_*_OUT` is set, and a new
`read_iso_for_oracle_when_asked` prints ART's own listing when
`ART_ISO_READ_IN` is. Both are `#[test]`s that do nothing unless their
variable is set, so `core/` stays clean and the script never reimplements the
builder — which would give it a third thing to be wrong in the same way.

- [x] **Step 2: Diff the listing**

Names (including Joliet names), sizes, and directory structure. Then extract
one file with `7z e` and byte-compare it with `IsoImage::read_file`. *As
built:* every file is compared, by SHA-256, not just one.

- [x] **Step 3: Raw layouts get the oracle too**

7-Zip cannot read a 2352-byte track dump, and no host mounts one, so the raw
paths would otherwise rest on ART agreeing with itself — which is what
[ART-075](../../ISSUES.md) records. In the script, strip each raw fixture down
to a plain 2048-byte image first (sync/header, and for XA the subheader, off
every sector — about fifteen lines of Python) and run the same diff against
the stripped image. **The stripping uses the layout's documented offsets, not
ART's code**, or it is not independent. Applies to `iso9660-raw`, and to
`iso9660-raw-xa` once Task 3b lands.

- [x] **Step 4: Fail loudly when `7z` is missing**

An oracle that silently skips is not an oracle. No `7z` on `PATH` → a clear
message and a non-zero exit. It runs **outside `cargo test`**, like the
amitools oracle, and is not part of core CI.

**Only well-formed fixtures ever reach 7-Zip.** The malformed and hostile ISOs
(looping records, lengths past EOF, depth bombs) stay inside `cargo test`
against ART's own reader; handing them to an external tool proves nothing about
ART and risks proving something about 7-Zip.

- [x] **Step 5: Gates and commit**

## Task 3b: Mode 2/XA Form 1 raw sectors — closes ART-075 (amendment A2)

**Files:**
- Modify: `src-tauri/src/core/detect.rs`, `src-tauri/src/core/iso/mod.rs`,
  `scripts/iso-oracle-check.py`, `docs/ISSUES.md`

**The defect.** The raw path assumes Mode 1: 2352-byte sectors with data at
offset 16, so `CD001` at `16 × 2352 + 16 + 1 = 0x9311`. A Mode 2/XA Form 1
disc carries an 8-byte subheader, so its data begins at offset 24 and the
signature lands at `16 × 2352 + 24 + 1 = 0x9319`. Today **both** detection and
the reader would be wrong together on such a disc — CD32 and mixed-mode discs
are exactly where that appears.

- [x] **Step 1: A failing detection test at 0x9319**
- [x] **Step 2: Carry the data offset (16 vs 24) in `SectorLayout`**

The same way 2048 vs 2352 is already carried — one value, decided once at open,
not a branch at every read.

- [x] **Step 3: Mode 2 Form 2 is refused, not misread**

2324-byte user data, no ISO filesystem semantics. If the submode byte says
Form 2, report honestly that it is unsupported (§10, §89).

- [x] **Step 4: Extend the 7-Zip oracle to the XA fixture** (Task 3a Step 3)
- [x] **Step 5: Close ART-075 in `ISSUES.md`, naming the tests**
- [x] **Step 6: Gates and commit**

## Task 4: One security gate, several archive engines (ZIP and 7z)

**Files:**
- Modify: `src-tauri/src/core/lha/safe_extract.rs` → generalise
- Create: `src-tauri/src/core/archive/mod.rs`, `src-tauri/src/core/archive/zip.rs`
- Modify: `src-tauri/Cargo.toml`

**Interfaces:**
- Consumes: `safe_join`, the existing extraction limits.
- Produces: `trait ArchiveReader { fn entries(&self) -> CoreResult<Vec<ArchiveEntry>>; fn read(&self, index: usize) -> CoreResult<Vec<u8>>; }` and `open_archive(path) -> CoreResult<Box<dyn ArchiveReader>>`, dispatching on the signature Task 1 found.

**The architectural point, and the reason this is one task rather than three.**
Everything today goes through `core/lha/safe_extract.rs`, which is where the
traversal, archive-bomb and unusable-name defences live — **one choke point**.
Adding ZIP and 7z as parallel readers beside LHA would mean three copies of
those defences, and the third copy is where the hole is. Generalise the gate
first: `safe_extract` keeps `safe_join`, `MAX_TOTAL_OUTPUT`, the entry cap and
the skipped-entry reporting, and calls a **backend** for the bytes.

Prove it with a test that runs the *same* hostile archive through every
backend — an entry named `../../outside.txt`, an absolute path, and one whose
declared size is a lie — and asserts each is refused identically. A defence
that only one backend has is the defect this task exists to prevent.

**On the dependency list.** `core/` is `std` + `serde` + `sha2` + `thiserror` +
`delharc`. `delharc` is already a decompression crate, so the precedent is set,
but each addition is a real decision: it is code that parses hostile input
inside the process. Add `zip` (or `flate2` directly) for ZIP, and `sevenz-rust`
for 7z — both MIT/Apache. Update CLAUDE.md's core-independence list and
`THIRD_PARTY_LICENSES.md` in the same commit; a dependency that is in
`Cargo.toml` and not in the licence file is a licensing defect.

**RAR is deliberately out of scope**, at the user's decision (2026-08-11).
Worth recording *why*, so nobody adds it later without knowing: RAR is
proprietary, and the UnRAR source licence is **not** an open-source licence —
it forbids using the code to reverse-engineer the compression algorithm.
Bundling an encumbered decoder would sit badly beside this project's existing
care (ADFlib is GPL, so it is read for understanding and never copied; amitools
is an external oracle and never linked). The `ArchiveReader` trait leaves the
door open: if it is ever wanted, the clean route is an **external** `unrar` or
`7z` the user already has — launched with **structured argv, never a shell
string**, since an archive entry name reaching a shell is a command injection —
implemented in `commands/` rather than `core/`, because spawning a process is
platform-specific.

- [x] **Step 1: Generalise the gate, with the shared hostile-archive test**
- [x] **Step 2: ZIP backend, tests built at runtime**
- [x] **Step 3: 7z backend, tests built at runtime**
- [x] **Step 4: Open an archive as a pane, reusing Task 3's container model**
- [x] **Step 5: Gates and commit**

## Task 5: Commodore 64 disk and tape images

**Files:**
- Create: `src-tauri/src/core/cbm/mod.rs`, `src-tauri/src/core/cbm/d64.rs`, `src-tauri/src/core/cbm/t64.rs`
- Modify: `src-tauri/src/core/detect.rs`, `src-tauri/src/commands/`, `src/pages/FileManager.tsx`

**Interfaces:**
- Consumes: the container model from Task 3 and the `ArchiveReader`-shaped read path from Task 4.
- Produces: `D64Image` and `T64Archive`, both exposing the same list-and-read shape as `IsoImage`.

**Scope, and the one thing that must not be over-promised.**

| Format | What it is | This task |
|---|---|---|
| **D64** | 1541 disk image, 256-byte blocks. 35 tracks = 174,848 bytes (175,531 with error bytes); **40 tracks = 196,608 (197,376 with error bytes)** | browsable |
| **D71** | 1571, double-sided, 349,696 bytes (**351,062 with error bytes**) | browsable |
| **D81** | 1581, 3.5″, 819,200 bytes, 80 tracks × 40 sectors | browsable |
| **T64** | tape *archive* — a real header and directory, despite the name | browsable |
| **TAP** | a raw recording of the tape signal as pulse widths | **identify only** |
| **PRG** | one program; the first two bytes are the load address | identify only |
| **CRT** | cartridge image | identify only |

**TAP cannot be browsed, and the reason is not effort.** A TAP file holds no
directory and no file table — it is the analogue tape signal sampled as pulse
lengths. Listing its contents means demodulating the Commodore ROM tape format,
and most commercial titles shipped their own turbo loader, so a standard
decoder finds nothing in them. **Detect it, report what it is and its length,
and stop there.** Do not add a TAP row to `FEATURES.md` that implies more
(§10, §89). A standard-loader decoder is a legitimate future slice; it is not
this one.

**D64 layout, only what the reader needs.**
- No header: the file *is* the sectors, track 1 sector 0 first. Tracks are
  numbered from 1; sectors per track vary by zone — 21 for tracks 1–17, 19 for
  18–24, 18 for 25–30, 17 for 31–35, **and 17 again for 36–40 on a 40-track
  image**. **A track/sector pair converts to a byte offset only through that
  table**, so build it once and test it at every zone boundary — including the
  35/36 one (amendment A3).
- **The size decides the track count**, and only these four are accepted:
  174,848 (35 tracks) · 175,531 (35 + error bytes) · 196,608 (40 tracks) ·
  197,376 (40 + error bytes). 40-track images are SpeedDOS/DolphinDOS-era and
  common in the wild. Any other size is an honest refusal **carrying the size
  in the message** — never a guess at what it might have been. Same for D71
  with error bytes (349,696 / 351,062). The directory is at track 18 either
  way.
- **BAM** at track 18, sector 0. Disk name at offset 0x90 (16 bytes, PETSCII,
  padded with 0xA0), disk ID at 0xA2.
- **Directory** starts at track 18, sector 1. Each sector holds 8 entries of 32
  bytes: file type at offset 2 (low nibble 0=DEL 1=SEQ 2=PRG 3=USR 4=REL, bit 7
  set = closed), first track/sector at 3–4, name at 5–20 (PETSCII, 0xA0
  padded), size in blocks as a little-endian u16 at 30.
- The first two bytes of every sector are the **next track and sector** in the
  chain; a next-track of 0 means this is the last, and then the second byte is
  how many bytes of this sector are used.

**Bounds, and why.** Every one of these comes from an untrusted file:
- A track/sector pair outside the geometry is an error, never an index.
- **Sector chains must have a step limit and a visited set.** A crafted D64 can
  point a sector at itself; without both, the reader loops forever.
- Cap directory entries and total entries.
- `panic = "abort"` — no direct indexing anywhere.

**PETSCII is not ASCII.** Names need a PETSCII→Unicode mapping, and 0xA0 is
padding to strip, not a character. Get the shifted/unshifted case right: in
PETSCII's default upper-case set, 0x41–0x5A are uppercase letters. A name that
comes out as mojibake is a bug even though nothing crashes — pin a few real
names in tests.

- [ ] **Step 1: The track/sector geometry, tested at every zone boundary**
- [ ] **Step 2: BAM and disk name, with PETSCII decoding**
- [ ] **Step 3: The directory walk, with the loop guard proved by a self-referencing fixture**
- [ ] **Step 4: Reading a file's sector chain back byte for byte**
- [ ] **Step 5: T64 — never trust the header, and prove it (amendment A4)**

A T64 has a header, a record count and per-entry load/end addresses, and
real-world files written by buggy tools get all three wrong: `used entries` is
frequently 0 while records exist, `max entries` lies, and **end addresses are
often wrong**, so a declared size is not a size.

- Derive the entry table by **scanning records** — a plausible file-type byte,
  offsets that land inside the file — not by trusting the header counts. If
  `used == 0` and valid records exist, use the records and note the quirk in
  the listing: a warning, not an error.
- Clamp every entry's data range to the actual file length. An end address
  *before* the start address means "compute the length from the container",
  not an error, and never a negative length.
- Fixtures, all of which must list and extract (clamped) without a panic:
  `used = 0` with one real record; end < start; a declared range past EOF.
- [ ] **Step 6: Detection signatures, and identify-only for TAP, PRG and CRT**
- [ ] **Step 7: Open a D64 and a T64 as panes, read-only**
- [ ] **Step 8: Gates and commit**

**A note on scope, recorded rather than argued.** This is a Commodore 64
format in a toolkit named for the Amiga. It fits architecturally — the
container model generalises to it exactly — and it was asked for deliberately.
Worth deciding at some point whether the product's name and `README` should
widen to match; not a reason to delay the work.

## Task 6: Close the phase

**Files:**
- Modify: `docs/FEATURES.md`, `docs/STATUS.md`, `docs/ISSUES.md`, `CHANGELOG.md`, `docs/format-support-matrix.md`

- [ ] **Step 1: Say what is true**

`format-support-matrix.md` currently lists optical images as planned. Move only what a test now covers. **ISO9660 with Joliet, read-only** is the claim; Rock Ridge, the Amiga `AS` System Use entry, `.cue`/`.bin`, `.nrg`, `.mdf` and writing a disc are **not** in this phase and must stay listed as unbuilt (§10, §89).

`FEATURES.md`'s wording adds **"verified against 7-Zip's independent listing
(including raw layouts via sector-stripping)"** — and claims no real Amiga CD
and no host mount, because neither was done (amendment A1).

- [ ] **Step 2: Record what is owed**

Open an `ART-*` for Rock Ridge and the Amiga `AS` entry — without them, an AmigaOS CD's protection bits and file comments are lost on extraction, which matters for a WHDLoad-era disc.

- [ ] ~~**Step 3: Produce a disc for the hardware loop**~~ — **cancelled by
  amendment A1 (user's decision, 2026-08-11).** The original step asked for a
  synthetic ISO in `test/` to be listed by a real Amiga's CD filesystem, on
  the assumption of licensed AmigaOS CDs being to hand. They are not, reliably,
  and the phase must not block on media nobody can promise. The risk that step
  covered is covered instead by Task 3a's 7-Zip oracle.

- [ ] **Step 3 (replacement): Say what the oracle does and does not prove**

`test/README.md` states that the disc reader is verified against **7-Zip's
independent implementation**, not against a real Amiga CD filesystem, and that
a volunteer with a real CD32 or AmigaOS disc is a welcome extra rung — the
community beta is where that arrives. Do not block the phase on it, and do not
let the wording imply it happened.

- [ ] **Step 4: Gates and commit**

---

## Self-Review

**Spec coverage.** The roadmap's slice 3.1 is Task 1 and 3.2 is Tasks 2–4, pulled forward from Phase 3 at the user's request because they are what makes the commander useful on the discs they own. Slice 3.2's full scope — `.cue`/`.bin`, `.nrg`, `.ccd`, `.mdf`, Rock Ridge, the Amiga `AS` entry — is deliberately **not** in this plan; ISO9660 plus Joliet covers AmigaOS install CDs, CD32 and CDTV titles, and the rest is additive once the reader exists.

**Placeholders.** None: every offset, every signature and every bound is named, and the format section carries the byte offsets rather than pointing at a specification.

**Type consistency.** `IsoImage`, `IsoEntry`, `FormatCategory::OpticalImage` and `probe_at` are each introduced in one task's Produces and consumed by name in the next.

**The risk this plan is most likely to be wrong about.** Task 2 asserts byte offsets in a format nobody here can check against a real disc. The synthetic ISO builder in Task 2 Step 1 is written from the same offsets as the reader, so **the reader and its fixtures can agree and both be wrong** — exactly the failure mode that let ART-032 through ART-035 ship behind a green suite. The plan is not sound without a mitigation, and the mitigation is now **Task 3a's 7-Zip oracle** (amendment A1), which needs no Amiga and no OS driver.

*Superseded, kept so the change of mind is legible:* the original two mitigations were to mount the synthetic ISO with the host OS in Task 2, and to get a real AmigaOS CD in front of the reader in Task 4. The first was done once — Task 2 mounted both 2048-byte fixtures with `Mount-DiskImage` and read them through Windows' own CDFS, which is how the Joliet `ë` and the big-endian UCS-2 were confirmed — and is **not** maintained going forward. The second is cancelled.

---

## Amendments

### 2026-08-11 — A1 … A7, from `ART-prompt-phase-2a-amendments.md`

Applied after an external review of this plan and a post-reboot inspection of
the repository. Recorded here so a cancelled step is never mistaken for an
omission.

| # | What changed | Where it lands |
|---|---|---|
| **A1** | The real-Amiga-CD verification is **cancelled**; a mandatory 7-Zip oracle replaces it, raw layouts included via sector-stripping | new Task 3a; Task 6 Step 3 struck through; Self-Review rewritten |
| **A2** | Mode 2/XA Form 1 raw sectors (`CD001` at `0x9319`, data at offset 24) — the fix for ART-075 | new Task 3b; the signature table gained its row |
| **A3** | D64 accepts the 40-track variants (196,608 / 197,376) and D71's error-byte size; any other size is a refusal carrying the size | Task 5 table, layout section, Step 1 |
| **A4** | T64 headers are not trusted: records are scanned, ranges clamped, three quirk fixtures | Task 5 Step 5 |
| **A5** | Baseline corrected to 731 Rust / 107 frontend (was 721 / 76) | the Gates block |
| **A6** | Line endings pinned by a `.gitattributes` in **one standalone commit**, never mixed with feature work | done: `42ab426` |
| **A7** | Recovery from the reboot: finish the interrupted Task 3 review fixes, gates first, its own commit | done: `80e1d40` |

**On A6, for the record:** the amendment described ~134 modified files and
~50k changed lines of CRLF↔LF churn. By the time the working tree was
inspected only the three real files were modified, and `git add --renormalize`
changed nothing — everything stored was already LF. The `.gitattributes` still
landed, because what was missing was the *pin*, and without it the same churn
can return on any checkout.

**On A7, for the record:** the amendment read the uncommitted work as "Task 3
Step 3, F5 out of a disc, in progress". It was not — Task 3 was complete and
committed (`7264e82`); what was half-written were the **fixes from that task's
own code review**, and the tree did not compile (`commands/iso.rs` called
`extract_tree` with the old arity). Finished, tested and committed as
`80e1d40`. The review's findings list itself did not survive the reboot; the
three fixes named in the code were recovered from the work in progress, and
anything else that review said is gone.
