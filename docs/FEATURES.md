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

## Which machines

ART is not an A500 tool that tolerates other Amigas. Most of what it does is
machine-independent — a format does not know which machine wrote it — and
where the machine matters it is carried as a **machine profile**, not a code
path. Built-in presets: **A1000, A500, A500+, A600, A2000, A3000, A1200,
A4000, CDTV, CD32** (`core/profile.rs`, read by the WinUAE launcher and the
compatibility check).

Commodore's 8-bit files are in scope as well, as of 2026-08-12, and read-only:
D64, D71, D81 and T64 open in the same commander an ADF does; TAP, PRG and CRT
are identified and described rather than browsed. See the C64 rows below.

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
| **ZIP** | ✅ | ✅ | — | — | 🟡 | — |
| **7z** | ✅ | ✅ | — | — | 🟡 | — |
| **ISO9660 / Joliet** | ✅ | ✅ | — | — | 🟡 | — |
| **ROM** | ✅ | ✅ | — | — | ✅ | — |
| **D64 / D71 / D81** (C64 disk) | ✅ | ✅ | — | — | 🟡 | — |
| **T64** (C64 tape archive) | ✅ | ✅ | — | — | 🟡 | — |
| **TAP / PRG / CRT** (C64) | ✅ | — | — | — | — | — |

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
- **ZIP / 7z / ISO9660 — read-only, and permanently so.** ART reads archives
  and discs; it writes neither, in any direction, and the panes refuse it by
  saying so rather than by doing nothing. All three go through the one
  extraction gate (`core/archive/extract.rs` for archives), so the traversal
  check, the output caps and the "declared size is a claim" check are the same
  code for each — proved by one hostile-archive test every backend is run
  through.
  - 🟡 **validate** = the gate refuses malformed and hostile input, and
    reports what it refused; there is no per-entry checksum verification.
  - ZIP is deflate only; encrypted entries are refused by name. 7z is LZMA.
  - ISO9660 covers Joliet and both raw sector layouts (Mode 1 and Mode 2/XA
    Form 1); Mode 2 Form 2 is refused rather than misread. Rock Ridge and the
    Amiga `AS` entry are **not** read — a Unix-mastered Amiga CD with no
    Joliet descriptor falls back to uppercase 8.3 names.
  - Verified against **7-Zip's independent implementation**
    (`scripts/iso-oracle-check.py`), raw layouts included via
    sector-stripping. Not against a real Amiga CD filesystem; nothing claims
    otherwise.
- **C64 formats — read-only, like every other container ART opens.** D64,
  D71 and D81 (35- and 40-track, with or without error bytes), and T64 tape
  archives, open as panes and copy out to a folder. Writing one is not
  implemented and not planned.
  - 🟡 **validate** = malformed images are refused rather than misread — an
    unknown size with the size in the message, a track or sector outside the
    disk, a sector chain that loops. There is no BAM-versus-directory
    consistency check.
  - **A T64's header is not trusted**: `used = 0` with real records lists the
    records, and an end address the file cannot support is clamped to what is
    actually there. The pane says when it had to do either.
  - `.tap`, `.prg` and `.crt` are **identify-only by design, not by
    schedule** — `c64.identify` reports what the file is, how big it is and
    why there is nothing inside to open. A TAP is the tape signal sampled as
    pulse widths: no directory, no file table.
  - Names are PETSCII with `0xA0` padding stripped from the end only. The
    graphics set renders as `·` rather than guessed-at Unicode look-alikes.
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
| Create blank / formatted / bootable | §11 | ✅ | `core/adf/create.rs`, `core/adf/bootcode.rs`. Until [ART-063](ISSUES.md#fixed) was fixed and verified by booting `test/art-bootable-test.adf`, `bootable` wrote a valid-looking but non-functional boot block (a bare `RTS`) — the boot block now reaches a CLI prompt on a real Kickstart/Workbench (not Workbench itself: `S/Startup-Sequence` and AmigaOS's own commands are content ART cannot supply). **Verified on real hardware 2026-08-12**: a real A500/A500+ with Kickstart 3.9, booting the image off a Gotek |
| Validate (boot block, checksums, bitmap) | §11, §12 | ✅ | `core/adf/validate.rs` |
| Optimisation analysis | §13 | ⏳ | — |
| Drag files in / out of the image | §11, §90 | ✅ | `/files` two-pane manager |

### Commodore 8-bit reader

| Feature | Spec | State | Code |
|---|---|:---:|---|
| Sector geometry (D64 35/40 track, D71, D81) | §10.5 | ✅ | `core/cbm/geometry.rs` — every zone boundary pinned |
| PETSCII names | §10.5 | ✅ | `core/cbm/petscii.rs` — `0xA0` stripped from the end only |
| Directory and file sector chains | §10.5 | ✅ | `core/cbm/d64.rs` — step limit *and* visited set, both proved by self-referencing fixtures |
| T64 tape archives | §10.5 | ✅ | `core/cbm/t64.rs` — records over header, ranges clamped |
| Identify-only formats (TAP, PRG, CRT) | §10.5 | ✅ | `core/cbm/mod.rs::identify`, offered as `c64.identify` |
| Open as a pane, copy files out | §10.5 | ✅ | `commands/cbm.rs`, `/files` |
| Writing any Commodore image | — | — | Not implemented, not planned |

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
| Read a PiStorm card (MBR + several RDBs) | SD-2a | ✅ | `core/mbr.rs`, `core/card/mod.rs` — verified against CaffeineOS 9317 and MultibootOS 2.2. A plain HDF reads as one area at offset zero, so callers do not branch |
| Plan a card's shape and write its partition table | SD-1 G2 | ✅ | `core/mbr.rs::plan_card` + `write_mbr` — FAT32 first at LBA 2048, one to three `0x76` areas, 4 MiB aligned. Defaults measured off both real cards; asked for MultibootOS's shape it produces MultibootOS's layout to the sector. **An Amiga disk at byte zero is not expressible**, which is how SD-0's unit-0 rule is enforced |
| Format the FAT32 boot partition | SD-1 G2 | ✅ | `core/fat32.rs` — FAT32 forced (a small partition would silently be FAT16 and not boot), 4 KiB clusters as on both real cards, files placed in the root. Every write is bounded to the partition by `Region`, so a formatter cannot reach the Amiga's first RDB. Cross-checked against **7-Zip** (`scripts/fat-oracle-check.py`): filesystem type, geometry, label, names and every file's bytes |
| Place the Emu68 payload on FAT32 | SD-1 G2 | ✅ | `core/card/payload.rs::emu68_payload` — unpacks the user's archive through the one security gate, refuses one that is not for their board or release line (and names `Emu68-raspi.zip` for what it is), merges the Pi's own `config.txt` instead of regenerating it, writes `cmdline.txt`, and places the Kickstart under the name the config points at. Driven end to end with the real release |
| Build a card image end to end | SD-1 G2 | ✅ | `core/card/build.rs::build_card` — sparse image, partition table, formatted boot partition, and an RDB at the start of every Amiga area at its own offset. Reads itself back with the reader that opens real cards; refuses to build over an existing file. The volumes inside are **not** formatted: that is SD-2's PFS3 work, and nothing claims otherwise |
| Building a card reaches the UI | SD-1 G2 | ✅ | `commands/card.rs::card_plan_build` + `card_build` on the Job Queue, and the OS Builder's **boot-only card**. One request type and one spec mapping for both, so a screen cannot show one card and write another; the plan answers `SAFE_CREATE` before the button rather than by a job that fails, and the result is the card read back by ART's own reader. Warnings are typed, not sentences, so they reach the user in their own language |
| Build manifest, and a card checked against it | SD-1 G7 | ✅ | `core/card/manifest.rs` — written beside the image as `<card>.manifest.json`, and **read off the finished card** rather than remembered from the build, so it records what is there. Carries the archive's and the ROM's SHA-256, the partition table's, every boot file's, and a 256 KiB window at each RDB. `card_verify_manifest` checks the table, the areas and the RDBs; the boot partition's files it reports as **not checked**, because ART writes FAT32 and cannot read one — `scripts/fat-oracle-check.py <card.img>` answers those with 7-Zip |
| Image health check — the last gate before the file is handed over | SD-1 G8 | ✅ | `core/card/health.rs` + `card_check_image`. Fourteen checks on a card ART built: the boot partition is first (SD-0's unit-0 rule) and at sector 2048, the areas are 4 MiB aligned, nothing overlaps or runs past the end, every RDB reads and checksums, no partition names a filesystem the card lacks (ART-084 as a gate), the manifest still agrees, and the four files the firmware needs are there. **Three states, kept apart**: pass, fail, and `not-checked` — the last never rendered as a tick. Plus the steps only the user can take (§50): flash it, HDMI before power, check the Pi, the volumes will ask to be formatted |
| A build as a drop target | SD-1 G15 | ✅ | `core/card/intake.rs` + `card_intake`. The one drop pipeline asked the *other* question: not "what can I do with this file" but "what does it become in this card". An Emu68 archive and a Kickstart fill their fields; a `config_<name>.txt` is recognised and declared not-yet-used (G16); **everything Amiga is told it needs a volume this card has not got** — the answer SD-1 owes most often, said rather than swallowed. The archive is matched **by name**, deliberately: its bytes say "zip", and which board and line it is for lives only in the name (ART-091), so the answer carries every reading rather than picking one |
| Format Amiga volumes and fill them | SD-2 G3/G5 route E and native | ✅ | `core/preload/` — the plan, the `VolumeFormatter` trait, and **two** implementations: `native::NativeFormatter` (`libpfs3` for PFS3, ART's own writer for FFS, launches nothing) and `tools/hst_imager.rs`. `commands/preload.rs::run_with_fallback` tries native first for every step and reaches `hst-imager` only for the two known capability gaps ([ART-113](ISSUES.md)'s non-ASCII PFS3 names, [ART-117](ISSUES.md)'s foreign-RDB embed) — reachable from the product as of [ART-120](ISSUES.md), fixed 2026-08-16, having shipped built-but-unreachable since G5. Run against the real tool on a card ART built (RDB embedded, `DH0` formatted as `Work`, a tree copied in, listed back by the tool as 1 directory, 2 files, 20 B) and checked against it as an independent oracle both directions (`scripts/pfs3-oracle-check.py`). Refuses before anything runs when a partition names a filesystem the card lacks (ART-084 again). **Neither writer's output is read back and checked within this operation** — see the OS-install rows below for where ART's own PFS3 reader (the same `libpfs3`) is used, on a different screen |
| Preparing volumes reaches the UI | SD-2 G3/G5 route E and native | ✅ | `src/lib/preload.ts` + the OS Builder's **prepare a card's Amiga volumes**, its third kind. §92 in full: read the card → choose what gets erased → preview → confirm the count → run as a job → report. **Nothing is chosen to start with and the choices are not remembered**, unlike every path on the screen — a screen that came back armed to erase two partitions would be ART's own remembering rule turned into a hazard. The volume-name rules are `check_name`'s, checked before the format rather than during it. `hst.imager`'s path is a setting beside `winuaePath`, no longer required for an ordinary run ([ART-120](ISSUES.md)) — only when the plan already shows the one step that always needs it, or a step's own content turns out to. The preview names which writer is expected to run each step, before the confirmation; the result panel names which one actually did, per step, never silently, and says plainly that no writer's output is read back and checked here (§89) |
| Does the card's Kickstart suit the OS about to go on it | SD-2 G9 | ✅ | `core/rom/pairing.rs::compare` + `preload_rom_pairing` + the volume-preparation screen. Reads two already-recorded facts rather than any ROM bytes — the tree's own planning ROM (`core::osinstall::PairedRom`, from G5's `distribution.json`) and the card's manifest (G7's `SourceFacts::kickstart_stated_major`, recorded at build time because ART writes FAT32 and has no reader for it) — and compares only what the tree's own recipe required, never "is this the same ROM": a different Kickstart is ordinary, an older one than the recipe named is not. Four verdicts (`Paired`, `Suitable`, `Unsuitable`, `NotChecked`), the last **never** rendered as a pass (§89) — a folder that is not a distribution tree, or a card ART did not build, is left alone rather than guessed at. Shown before the confirmation checkbox, beside the writer-choice line G3/G5 already print. **Run against the pairing that actually failed under WinUAE with a licensed ROM on 2026-08-16 (real hardware untouched)**: the real AmigaOS 3.2 tree requiring Kickstart 47 against a card built with the real Kickstart 40 comes back `Unsuitable { needs: 47, found: Some(40), rom: "kick.rom" }`; the same recipe's V40 build, which carries its own compatibility modules, comes back `Paired` against the identical card, since it is the same ROM file the card and the tree's own plan both hashed and against a **second** card built from the user's real Kickstart 47 — a ROM it was never built for — that same V40 build comes back `Suitable { rom: "kick.rom" }`, which is the only evidence on real material that the check reads the tree's own capability rather than comparing version numbers, since 40 is not ≥ 47 (`commands::preload::tests::the_real_trees_against_a_real_card_when_asked`, `#[ignore]`d, env-gated — both cards it builds to prove this are deleted at the end of the run). The verdict is asked and rendered **per content folder**, named by its drive, and the requirement is evaluated before identity is ever consulted ([ART-129](ISSUES.md)) |
| What goes where — turn a pile of files into a staging tree | SD-2 G11 | ✅ | `core/layout/` (`scan.rs`, `policy.rs`, `mod.rs`, `apply.rs`) + `commands/layout.rs` + its own **layout** screen (`/layout`, `src/pages/ContentLayout.tsx`) — a peer of the OS Builder in the sidebar, not a screen inside it; the staging folder it produces is pointed at **by hand** from the preload screen. Walks a drop, stopping at a WHDLoad drawer rather than descending into it, and proposes a destination per item: `Games`, `Floppies`, `HardDisks`, `CDs`, `Unsorted`. A ROM and a Commodore 8-bit disk are **refused with a reason** rather than placed — a ROM is the FAT32 boot partition's business, a C64 disk has none on an Amiga volume. Unpacking a WHDLoad archive goes through the one archive gate and places the drawer's `.info` **beside** the drawer, never inside it (§82). Never overwrites an existing destination, never touches the source (tested byte for byte), `safe_join` on every destination since it is user-typed text, cancellable between items with `CancelledPartway` reporting how many landed. **ART cannot tell a demo from a game** — nothing in a `Detection` says so — so the policy proposes only what it can justify and the preview table is editable, not a cleverer rule engine. Driven in headless Chrome against the real bundle, with `invoke` mocked — **not yet driven in `pnpm tauri dev` against real material, and no staging tree it built has been carried onto a card** |
| Filesystem drivers as the card's, not the area's | SD-2a | ✅ | `CardImage::partitions_missing_driver` — MultibootOS's second RDB carries no PFS3 while all fifteen of its partitions are PFS3, and the card works |
| Card reading reaches the UI | SD-2a | ✅ | `commands/card.rs::card_open` + the Hard Disk studio's card view. It asks the card reader for **every** file and branches on whether a partition table was found, so an HDF is unchanged and a card no longer has to be recognised by its extension. Read-only, and the screen says so |

| Read embedded filesystem drivers (FSHD + LSEG) | §18, SD G4 | ✅ | `core/rdb.rs::parse_file_systems`; the studio names partitions nothing will mount (`@/lib/rdbDrivers`) |
| Embed a filesystem driver into a new RDB | §16, SD G4 | ✅ | `core/rdb.rs::create_rdb_layout`; wizard step 4 (`@/lib/fsDriver`). Version read from the driver's `$VER:`. **Verified by a Kickstart, not only by tools** — which is the whole story of [ART-126](ISSUES.md): `rdbtool` extracted the driver back byte-for-byte and `hst-imager` listed it, and AmigaOS still ignored it, because `PatchFlags` named `StackSize` (`0x10`) where it had to name `SegListBlock`+`GlobalVec` (`0x180`, the value both of the user's real booting cards write). Until 2026-08-16 no disk ART made had ever been mounted by an Amiga; one now boots to a Workbench under WinUAE with a licensed ROM |
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
| Machine profiles (presets) | §33 | 🟡 | `core/profile.rs` — the classic line, end to end: A1000, A500, A500+, A600, A2000, A3000, A1200, A4000, CDTV, CD32. Presets only; user-defined profiles are not built yet. A preset pins a Kickstart hash only where there is one to pin — an A1000 loads its Kickstart from floppy, and an A2000 or A3000 may run 1.3, 2.04 or 3.1, so those pin none rather than a guess |
| Kickstart ROM identification | §32 | ✅ | `core/rom.rs` |
| Gotek scan + FlashFloppy config | §37, §39 | ✅ | `core/gotek.rs` |
| Gotek bulk workflow | §38 | ⏳ | — |
| PiStorm hardware matrix (Amiga × board × Pi) | §40 | ✅ | `core/pistorm/hardware.rs` — kernel archive, storage device name, token gating and per-combination notes all derive from it |
| PiStorm `cmdline.txt` options | §40 | ✅ | `core/pistorm/options.rs` — one field per documented Emu68 token, merged never regenerated. Four profiles, each the tokens it writes (ART-090) |
| PiStorm `config.txt` firmware | §40 | ✅ | `core/pistorm/firmware.rs` — kernel, `initramfs`, display presets, opt-in overclock. Merged; the user's own lines survive |
| PiStorm Kickstart through ROM Manager | §32, §40 | ✅ | every ROM on the card identified by checksum; pick one from anywhere and copy it on, named and confirmed. Unrecognised is a label, never a refusal |
| PiStorm kernel version detection | §40 | ✅ | `firmware.rs::version_from_kernel` reads the `$VER:` string Emu68's own build compiles in; "unknown" when it says nothing |
| PiStorm kernel update from GitHub | §40 | ⏳ | F4's second half — not built; the screen offers nothing rather than a button that does nothing |
| PiStorm card verified on real hardware | §40 | ⏳ | no card built by this screen has been booted |
| PiStorm WiFi / network pre-seeding | §40, SD G14 | ⏳ | Amiga-side (`wifipi.device`, `DEVS:NetInterfaces`), so it belongs to volume building — declared on screen rather than offered |
| Multiboot `config_<name>.txt` sets | SD G16 | ✅ | list, create, duplicate, rename, activate, delete — each through preview → backup → write. A deleted set is kept; the one the card is running cannot be deleted |
| Distro profile registry | SD G13 | ✅ | `core/distro/registry.json` — data, not code; CaffeineOS, CoffinOS, AmiKit and two ART Baseline entries. No profile can name a token Emu68 lacks |
| OS Builder: licence, ROM family and card checks | SD G13 | ✅ | `/os-builder` — what each distribution requires of you, and whether the ROM and card you have suit it |
| OS Builder: prepare the card | SD-2a | ⏳ | blocked on inspecting a real distribution's layout by hand (research §8.2). Every profile is `available: false` and the screen says so |
| OS install: build a distribution tree from the user's own AmigaOS 3.2 media | SD-2 G5 | ✅ | `core/osinstall/{scan,plan,apply}.rs` — install ADFs found by the volume name recorded *inside* them, never by filename; which components switch on (required, chosen, or a satisfied `Condition` such as pre-V47 `modules-a1200`) resolved once and shared by the preview and the build; file-level destination collisions across two disks refused by name; the tree written with a `.uaem` sidecar per file and a `distribution.json` manifest (path, component, medium, SHA-256, protection, per file). 124 `core::osinstall` unit tests plus 21 `commands::osinstall` unit tests, 145 in all (synthetic fixtures — ART ships no Amiga content, so none of them can guard a *recipe-data* mistake that only real media exposes — except two: `core::osinstall::apply`'s `inspect_real_media_when_asked` and `run_the_real_engine_against_the_users_own_media_when_asked` are `#[ignore]`d hooks that run against the user's own copyrighted media, not a fixture). Run for real against the user's own 36-ADF 3.2 set and the paired Kickstart, and its own assertions now pin the numbers: 26 components on (`modules-a1200` among them, switched on by its own condition against the real V40 ROM, never chosen), exactly 4030 files / 330 directories / 11.9 MB written as plan items — the tree that produced holds 3933 files (the manifest included) and 280 directories, because an override writes one destination twice and a shared directory is counted twice ([ART-124](ISSUES.md)) — zero refusals — after two recipe defects the real run found were fixed ([ART-111](ISSUES.md), [ART-112](ISSUES.md)); the fix itself has no repo-side regression test, since proving it needs the user's own copyrighted media, which the no-content rule forbids — the real-media test hook is what stands in its place |
| OS install: put the tree onto a PFS3/FFS volume | SD-2 G5 | 🟡 | `core/preload::NativeFormatter::copy_in` (`format_partition` then `copy_in`, the same two calls G5 always makes), reached through `commands/preload.rs::run_with_fallback` — native first, `hst-imager` only for the two known gaps ([ART-120](ISSUES.md)). **Re-measured 2026-08-16 against the real tree, through the path that now ships** — which replaces the earlier 3061-of-4030 / 969-excluded figures, a one-off manual pass against code that has since changed. The tree at `E:\amiga\ProjeART\dist-3.2` holds **3933 files / 280 directories / 12.2 MB** (`apply()` announced 4030/330; that number counts plan items, not entries — [ART-124](ISSUES.md)). Carried through the fallback, **all of it lands**: `hst-imager fs dir -r` counts *280 directories, 3933 files, 12.2 MB* on the volume, `Locale/Countries` holding `españa.country`, `österreich.country` and `türkiye.country` by name — so the non-ASCII quarter that [ART-113](ISSUES.md) excludes from the native writer is not lost, it is copied by the fallback. **And it boots**: the tree built for a V47 A1200 ROM (4047 files / 328 directories, `backdrops` included) boots AmigaOS 3.2 to a clean Workbench under WinUAE with the user's own licensed ROM — after [ART-126](ISSUES.md) (the RDB's `PatchFlags`) and [ART-127](ISSUES.md) (`icon.library`, `workbench.library`, and the wallpaper path the running system named itself). It lands in **one run, unattended**, which it did not at first: `hst-imager`'s first write into a volume `NativeFormatter` had just formatted died `ERROR_DISK_FULL` ([ART-122](ISSUES.md) — the two implementations size the PFS3 reserved area differently, and ART's number is `pfs3aio`'s own, so the fix was to stop mixing them). A partition is now formatted and filled by **one** tool, chosen before the destructive step. Reproducible, unlike the measurement it replaces: `commands::preload::tests::carry_the_real_dist_tree_through_the_fallback_path_when_asked` (`#[ignore]`d, env-gated). Which tool ran which step, and why, is reported per step (`StepReport`), never silently; when the fallback is needed and no `hst.imager.exe` is configured, the refusal names both the missing tool and the reason. The osinstall→preload handoff is still two separate manual steps, not one flow |
| OS install: verify a volume against its manifest | SD-2 G5 | ✅ | `core/osinstall/verify.rs` / `osinstall_verify` — presence, size and protection per file (content too, on PFS3), `NotChecked` kept apart from `Pass`/`Fail` (§89: "ART did not look" is never "ART found nothing wrong"), an unreadable filesystem family answering `NotChecked` for the whole run rather than failing it |
| OS install screen (`/os-builder`, Install kind) | SD-2 G5 | 🟡 | `src/components/osbuilder/OsInstall.tsx` / `src/lib/osinstall.ts` — 26 pure-logic unit tests, both i18n catalogues complete, the wire pinned in both directions (`commands/osinstall.rs`'s `wire_shapes` tests). Confirmed rendering in a headless browser: the route, the screen's own headings, five `h2`s resolved with no raw key and no `{{…}}`. **Unconfirmed**: the component checklist and its reasons, the confirmation panel, the file list, the refusals card, the Verify states, and Turkish in the screen's tighter controls — deeper interaction crashed the renderer reproducibly ([ART-118](ISSUES.md)) |
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
| Settings (theme, paths, language) | §59 | ✅ | Every path has a Browse button beside the box (ART-086); the Aminet download folder is one of them |
| Every choice remembered | user rule | ✅ | `src/lib/remembered.ts` — a persisted value is read back **through a guard**, so a stale or hand-edited settings file falls back to the default rather than putting a bad value on screen. `settingsStore` refuses to let a late-landing read overwrite a key the user has already touched this run (the other half of ART-089) |
| Application Size (Ctrl +/-/0) | §64 | ✅ | `src/lib/appZoom.ts` — 70–250 % in tens, set from Settings or the keyboard, remembered. Most of the people using ART are over fifty; this is a first-class setting, not an accessibility afterthought. The right-edge complaint ([ART-099](ISSUES.md#open)) **did not reproduce when measured**: seven screens at 100/130/200 % in a window the size of the one it was reported on, nothing overflowing (`scripts/zoom-check.py`). What was real is fixed — the content area can be scrolled sideways now instead of clipping in silence |
| About: author, licence, source | §67 | ✅ | Settings → About — the logo, name and version, author (tolon), the GitHub address, GPL-3.0-or-later with the warranty notice and where the licence text lives. No email anywhere, deliberately |
| i18n architecture | — | 🟡 | English and Turkish, 1469 keys each, chosen in Settings and remembered; `CoreError` and `WhdloadRefusal` sentences from Rust still reach the UI in English regardless of the chosen language ([ART-060](ISSUES.md#open)) |
| Dark / light theme | §61 | ✅ | |
| Beginner / Power User mode | §47, §48 | ✅ | `lib/uxmode.ts`; hides advanced studios, actions and block detail |
| Operation history + export | §53 | ✅ | Settings → Operation Log |
| Workflow Wizard | §45 | ⏳ | — |
| Two-pane file manager | §11, §17 | ✅ | `/files`, Total Commander-styled (row icons, file-type colour, Attr column); F3 view · F4 edit · F5 copy · **F6 move** (Shift+F6 rename) · F7 mkdir · F8 delete · F9 attributes, plus F2/Ctrl+R refresh. Any volume to any volume for a single entry; see Multi-select below for what a selection can and cannot do |
| Pane header: source combo + path + filter | Brief §1.3 | ✅ | `src/lib/paneSources.ts` (pure, 10 tests) — enumerated mounts from `panel_local_roots` plus the six picker sources, no hardcoded letters; the button strip behind it is `showSourceButtons` in Settings, default off |
| Command line (navigate + filter) | Brief §1.4 | ✅ | `src/lib/commandLine.ts` (pure, 8 tests) — a full path, `cd ..`, or a `*`/`?` mask. Running programs is deliberately out of scope (§56) and refused by name, never silently ignored |
| Move (F6) | Brief §1.4, §92 | 🟡 | `src/lib/movePlan.ts` (pure, 16 tests) + `moveSelection` in `FileManager.tsx`: copy → **re-list the destination and look for every moved name** → delete, so a stopped move can only leave a duplicate. A collision is refused, not resolved by the overwrite policy. Volume→folder and one folder between two images work; out of a host folder ([ART-080](ISSUES.md#open)), several entries between images ([ART-064](ISSUES.md#open)) and a single file between images ([ART-081](ISSUES.md#open)) are each refused by name |
| Enter opens a container in the same pane | Brief §3.1 | ✅ | `src/lib/containerStep.ts` (10 tests) + `src/lib/paneHistory.ts` (15 tests). What a row opens into comes from `analyze_paths`, never the extension; `PaneState.host` carries the way back out, so `[..]` at a container's root returns to the host folder **with the cursor on the container file**. Verified on the running screen with a real ADF. Backspace / Ctrl+PgUp go up, Ctrl+PgDn enters, Alt+Left / Alt+Right are per-pane history with container steps as places |
| Full keyboard coverage | Brief §3.2 | 🟡 | Space marks in place, Insert marks and advances, Ctrl+A includes directories, the numpad marks by mask and inverts, type-to-search moves the cursor and only the cursor (`src/lib/quickSearch.ts`, 15 tests), F2/Ctrl+R refresh, Alt+F1/Alt+F2 open the source combos, Ctrl+B the sidebar. **Space does not compute a directory's size** ([ART-087](ISSUES.md#open)) — no primitive exists to count with |
| Tabs per pane, and session restore | Brief §3.3 | 🟡 | `src/lib/paneTabs.ts` (19 tests): a tab is a `PaneLocation` plus its sort order and mask, so it can live inside an image. Ctrl+T duplicates, Ctrl+W closes (never the last), Ctrl+Tab cycles, middle-click closes. Persisted to the settings store and validated on the way back in (`src/lib/paneSession.ts`, 9 tests, including the JSON round trip a restart really performs). **The write half is verified on the running application** — two tabs made in the pane appear in `settings.json` as two tabs. **The read-back half has still not been seen**: nothing has closed and reopened ART since |
| Per-filetype colour rules | Brief Part 2 | ✅ | `src/lib/colourRules.ts` (11 tests): mask → colour, first match wins, several masks per rule separated by `;`. Three ART defaults — containers, archives, ROMs. Sits *in front of* the built-in classification, so an empty list changes nothing, and a malformed stored list falls back to the defaults |
| Multi-select (Shift/Ctrl-click, Insert, Ctrl+A) | Roadmap 1.1 | ✅ | `src/lib/selection.ts` (pure reducer); real pane focus (`focused: Side`) and Tab between panes. The count and its size live in each pane's own Total Commander status line |
| Batch copy: local ↔ volume | Roadmap 1.1 | ✅ | `volume_plan_copy_many` / `volume_copy_in_many` (host→volume, one job, cancel commits nothing) — `commands/volume_write.rs`. Volume→local multi-select works but is several concurrent per-entry operations, not one atomic job ([ART-065](ISSUES.md#open)); volume→volume multi-select refuses outright, with no primitive to batch on ([ART-064](ISSUES.md#open)) |
| Batch delete | Roadmap 1.1 | ✅ | `volume_delete_many`; a batch that can't fully succeed (missing name, non-empty directory) deletes nothing |
| Several archives installed to a disk at once | Roadmap 1.1 | ✅ | `archives_plan_install` / `archives_install` (`commands/archives.rs`); each archive gets its own drawer, staged into one write so a cancelled batch can't leave two games half-installed. Planning runs as a job with its own progress and Stop ([ART-066](ISSUES.md)), and Stop is answered *inside* one archive's extraction rather than only between two ([ART-067](ISSUES.md)) |
| One listing order + per-pane column sort | Roadmap 1.2 | ✅ | `commands/panel.rs`, `core/adf/fs.rs`, `core/volume/write/dir.rs::entries_in` now share one folders-first, case-insensitive floor; `src/lib/sort.ts` sorts on top of it by name/size/date, click-to-reverse |
| Filename mask filter | Roadmap 1.2 | ✅ | `src/lib/mask.ts` — Total Commander `*`/`?` wildcards, whole-name, case-insensitive; narrows what a pane shows and clears its selection on change. "this folder is empty" and "your mask matches nothing" are told apart by the matcher itself rather than inferred from two counts ([ART-068](ISSUES.md)) |
| Checkout / checkin (F4) | Stage W §6 | ✅ | `core/volume/checkout.rs`; SHA-256 gated, CRLF offered never applied |
| `.uaem` sidecars | Stage W §4.2 | ✅ | `core/volume/write/uaem.rs`, WinUAE's format; round-trip test pins `HSPARWED` |
| `.info` pairing | Stage W §7.1 | ✅ | Rename, delete and move all offer the icon; the copy plan warns when a pair is split |
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

The Aminet rows are ✅ apart from the AI layer. What is built and tested:
`core/sources` — index and readme parsing, catalog store, search, version
resolution, mirror failover, the download trust pipeline and catalog sync (167
tests); `net/http_mirror.rs`, the `ureq`-backed transport (16 tests);
`commands/sources.rs` (12 tests); `src/lib/sources.ts` and Aminet Studio at
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
