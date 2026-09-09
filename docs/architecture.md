# Architecture

> This document is the architectural contract for ART. Implementation must be
> consistent with it. See the master specification for the full product vision.

## Overview

ART is a layered application. Each layer depends only on the layer below it,
never upward. The defining rule: **technical complexity belongs in the Rust
core, not in the React UI.**

```
┌──────────────────────────────────────────────┐
│  UI Layer          React + TypeScript        │
│  (presentation, navigation, interaction)     │
└──────────────────┬───────────────────────────┘
                   │ Tauri commands (invoke)
┌──────────────────▼───────────────────────────┐
│  Application Layer  commands/                │
│  (thin adapters, no business logic)          │
└──────────────────┬───────────────────────────┘
                   │
┌──────────────────▼───────────────────────────┐
│  Workflow Engine    core/workflow            │
│  (detection → plan → recommend → execute)    │
└──────────────────┬───────────────────────────┘
                   │
┌──────────────────▼───────────────────────────┐
│  Amiga Core         core/                    │
│  adf · volume · hdf · rdb · lha · archive    │
│  iso · cbm · rom · gameindex · launch        │
│  card · osinstall · preload · amigainstall   │
│  safety · security · jobs · oplog            │
│  detect · hashing · analysis · profile       │
│  (PLATFORM-INDEPENDENT — no tauri, no OS)    │
└──────────────────┬───────────────────────────┘
                   │
┌──────────────────▼───────────────────────────┐
│  Platform Services                           │
│  (Windows-specific: drives, processes, etc.) │
│  No platform/ directory yet — the few such   │
│  needs live in the modules that have them.   │
└──────────────────────────────────────────────┘
```

## Project structure

```
amiga-retro-toolkit/
├── src/                       # Frontend (React + TS)
│   ├── components/             #   reusable UI (layout, DropZone, common)
│   ├── pages/                  #   routed views (Dashboard, Settings, ...)
│   ├── lib/                    #   api, db, settings, dnd, log wrappers
│   ├── stores/                 #   zustand state
│   ├── i18n/                   #   localization
│   ├── styles/                 #   theme + global CSS
│   └── types/                  #   shared TS types mirroring Rust
│
├── src-tauri/                  # Tauri shell + Rust
│   ├── src/
│   │   ├── main.rs             #   entry point (calls lib::run)
│   │   ├── lib.rs              #   Tauri builder, plugins, state, handlers
│   │   ├── error.rs            #   AppError (serializes to frontend)
│   │   ├── commands/           #   #[tauri::command] adapters
│   │   ├── scratch.rs          #   the one staging root everything goes through (ART-196)
│   │   ├── net/                #   the only place in ART that opens a connection
│   │   ├── tools/               #   platform-specific: launches external programs
│   │   │   ├── hst_imager.rs   #     the VolumeFormatter that shells out (outside core/)
│   │   │   ├── recycle_bin.rs  #     the HostRecycler: IFileOperation, also outside core/
│   │   │   └── winuae_launcher.rs #  the EmulatorLauncher: spawns winuae64.exe (ART-274)
│   │   └── core/               #   AMIGA CORE (platform-independent)
│   │       ├── error.rs        #     CoreError
│   │       ├── detect.rs       #     format detection
│   │       ├── hashing.rs      #     SHA256
│   │       ├── safety/         #     DATA safety: atomic writes + backups
│   │       ├── security/       #     INPUT safety: path traversal defence
│   │       ├── jobs/           #     progress reporting + cancellation
│   │       ├── oplog/          #     operation log (what ART did, and to what)
│   │       ├── workflow/       #     Workflow Engine
│   │       │   ├── types.rs    #       Workflow trait, Plan, WorkflowKind
│   │       │   ├── registry.rs #       registry + engine
│   │       │   └── builtin.rs  #       the catalogue of offered actions
│   │       ├── adf/            #     bootblock, blocks, fs, extract, create...
│   │       ├── volume/        #     ART's own OFS/FFS writer: mount, write/, journal
│   │       ├── archive/       #     the one extraction gate (LHA, ZIP, 7z)
│   │       ├── iso/, cbm/     #     ISO9660 discs; Commodore 8-bit images, read-only
│   │       ├── lha/            #     LHA reading, safe_extract, whdload detection
│   │       ├── hdf.rs, rdb.rs  #     hard disk images + partition tables
│   │       ├── mbr.rs, fat32.rs #    SD card partition table + FAT32 boot partition
│   │       ├── card/            #    a card as Vec<AmigaArea>; build.rs builds one
│   │       ├── distro/          #    the registry of real AmigaOS distributions
│   │       ├── osinstall/       #    media -> distribution tree (recipe, plan, apply, verify)
│   │       ├── preload/         #    distribution tree -> a card's Amiga volumes
│   │       │   └── native.rs   #       the VolumeFormatter that launches nothing
│   │       ├── amigainstall/   #    run a package's own installer inside WinUAE
│   │       ├── layout/         #    a pile of files -> a staging tree
│   │       ├── gameindex/      #    the title catalogue (core/collection.rs retired onto it)
│   │       ├── launch/, artwork/, sources/, whdload/
│   │       ├── gotek.rs, pistorm/, winuae.rs, rom/, profile.rs
│   │       ├── hostfs.rs, dirsize.rs, amigaver.rs, analysis.rs, vhd/
│   │       ├── amiganet/       #    seeding an Amiga's network before its first boot
│   │       └── recovery.rs, conversion.rs, binary.rs   # stubs, see FEATURES.md
│   ├── capabilities/           #   Tauri 2 permission model
│   ├── migrations/             #   SQLite migrations
│   └── tauri.conf.json
│
├── docs/                       # this folder
├── deny.toml                   # cargo-deny policy
└── .github/workflows/          # CI
```

## The core independence rule

`src-tauri/src/core/` compiles with **only `std` + `serde` + `serde_json` + `sha2` + `log`
+ `thiserror` + `delharc` + `zip` + `sevenz-rust2` + `quick-xml` + `fatfs` + `libpfs3`** — the
three decompressors are read-only and sit behind `core/archive`'s single security
gate; `quick-xml` reads exactly one thing, `rp9-manifest.xml` inside an `.rp9`
package, and reads it *through* that gate rather than from a path, so the
manifest's bytes are bounded before the parser sees them; and `fatfs` creates the one filesystem ART writes that is not an Amiga
one: the FAT32 partition a PiStorm card's Raspberry Pi boots from. `libpfs3`
is the PFS3 implementation — the volume format `core/preload` (G3 route
native) writes and reads on a PiStorm card, with SD-2's OS install engine
(G5) as its newest and largest consumer — and the one LGPL-3.0-or-later
dependency inside `core/`: weak copyleft, compatible with ART's own GPL-3.0-or-later, but noted
deliberately against the project's preference for permissive dependencies,
because `core/` is meant to be promotable to a standalone crate. It never
imports `tauri`, never calls Windows APIs, never touches the network.
This is what makes it unit-testable and what leaves the door open to a future
CLI or other shells without rewriting the engine.

Concretely: if a `core/` module needs to do something platform-specific (open
a file dialog, detect a USB drive, launch WinUAE), it exposes a **trait**, and
the implementation lives outside the core.

There are five live instances: `MirrorClient` (`core/sources/mirror.rs` →
`net/http_mirror.rs`, the network), `VolumeFormatter` (below), `HostRecycler`
(`core/hostfs.rs` → `tools/recycle_bin.rs`, which is how a file the user deletes goes to the
Windows Recycle Bin rather than into a recovery mechanism ART invented — ART-080),
`EmulatorLauncher` (`core/amigainstall/run.rs` → `tools/winuae_launcher.rs`, which is how ART
starts and ends the WinUAE process a run or a first-boot rehearsal drives), and
`VolumeSession` (`core/whdload/install.rs` → `commands/whdload.rs`, ART-242): one WHDLoad
pack's disk write — create the drawer, copy its contents, place its icon — run inside a single
opened, backed-up-once, committed-once volume session. Its implementation lives in
`commands/` rather than `tools/`, unlike the other four, because what it wraps
(`commands/volume_write.rs::with_volume`, the session/backup/write-strategy machinery) is
itself still a command-layer helper rather than a `core/` module — promoting `with_volume`
into `core/` is a round of its own, so `CommandVolumeSession` is the thin seam that lets
`core::whdload::install` use it today without `core/` depending on `commands/` directly.

`core/winuae.rs` used to spawn `winuae64.exe` directly from inside `core/`, with no trait
between the decision to open the emulator and the process spawn that carried it out —
[ART-274](ISSUES.md), closed 2026-09-07 by moving the spawn (`WinUaeLauncher`,
`launch_winuae_process`, `WinUaeProcess`) into `tools/winuae_launcher.rs` and leaving only
config generation and install-location detection in `core/winuae.rs`.

`core/preload::VolumeFormatter` (`probe`, `import_filesystem`, `format_partition`, `copy_in`)
is the largest of them: `src-tauri/src/tools/hst_imager.rs` launches `hst.imager.exe` and lives
outside `core/` for exactly this reason, while `core/preload/native.rs` — PFS3 through
`libpfs3`, FFS through `core/volume/write` — launches nothing and lives inside it. Native is the
product's default; `hst-imager` is a fallback the caller in `commands/` chooses per operation,
never a decision `core/` makes for itself.

### The same rule pointing inwards: `core/` modules do not depend upwards

The rule above keeps the platform out of `core/`. It has a second form that is
easy to break because nothing outside `core/` notices: **a lower-level `core/`
module must not import a higher-level one.** `core/rom` is about ROM files;
`core/osinstall` is an engine that happens to record one. When `core/rom`'s
pairing check took `core::osinstall::PairedRom` as its input, `core/rom` could
no longer be read — or extracted — without dragging the whole OS-install engine
along, for the sake of two fields.

The fix is the shape to copy: the comparison declares its **own** record
carrying only what it reads, and the caller in `commands/` maps the other
module's type into it. Translation between two modules' representations is a
command-layer concern, which is where it belongs in this codebase — the same
place `run_with_fallback` decides which formatter runs. `core/rom/pairing.rs`
and `commands/preload.rs::rom_pairing_for` are the worked example.

`commands/*.rs` are thin adapters only: deserialize the arguments, call core,
serialize the result back. Business logic that ends up there is business logic
in the wrong layer.

## Error types

Two levels, deliberately separate. `core::CoreError` (`thiserror`, no Tauri) is
wrapped by `error::AppError`, which serializes to its `Display` string, so the
frontend always receives a readable sentence rather than a discriminant.

**Never surface a raw OS code to the UI.** Technical detail goes to the log; the
user gets a sentence and an `ART-*` identifier. The identifier registry is
`CoreError::code()` — see
[security-model.md § Error reporting](security-model.md#error-reporting).

## Where the network lives

`src-tauri/src/net/` is the transport, and **nothing else in ART may open a
connection**. It is blocking `ureq`, pinned `=3.2.1`; downloads already run on
job threads, so an async runtime would buy nothing.

`gzip` is deliberately off. Transparent decompression would make the bytes ART
writes differ from the bytes the server counted, breaking both the resume offset
and the size gate.

What ART is *allowed* to ask for is not decided here: the policy — a request is
always constructed from a configured `Mirror` plus a validated repository path,
never from a caller-supplied URL — lives in `core/sources/mirror.rs` and is
written down in
[security-model.md § Where ART may fetch from](security-model.md#where-art-may-fetch-from).

## Data flow: the DROP pipeline

Drag and drop is architectural, not a convenience. There is exactly **one**
global webview listener, registered in `components/layout/Layout.tsx` via
`lib/dnd.ts`. Per-module drop systems are not allowed: a second listener means
two answers to "what did the user just drop", and only one of them reaches the
panel.

```
USER drops a file
      │
      ▼
Frontend onDragDropEvent  ──paths──▶  invoke('analyze_paths')
                                            │
                                            ▼
                               commands::dragdrop::analyze_paths
                                            │
                                            ▼
                               WorkflowEngine::plan(path)
                                            │
                              ┌─────────────┴─────────────┐
                              ▼                           ▼
                       detect::detect(path)    registry.candidates_for(detection)
                              │                           │
                       Detection {category,         Vec<Arc<dyn Workflow>>
                                   format_hint,            │
                                   confidence,             ▼
                                   size}           sort by priority
                                            │
                                            ▼
                                     Plan { detection,
                                            recommendations,
                                            candidates }
                                            │
              ◀── serialize (JSON) ────────┘
      │
      ▼
Frontend renders "What can I do?" panel
```

`Detection { category, format_hint, confidence, size, is_dir }` is what crosses
the boundary. `FormatCategory` serializes as kebab-case strings —
`floppy-image`, `harddisk-image`, `archive`, `rom`, `directory`, `unknown` — so
the TypeScript side matches on those literals and not on a numeric tag.

The execute half of the pipeline is in place for operations that modify data:
`core/safety` performs `BACKUP → APPLY` atomically and the volume writer's
`commit_whole_file` (`commands/volume_write.rs`) validates the whole image
before committing. Copying into a volume also has the **preview**
step §92 asks for — `volume_plan_copy` reports the cost, the unstorable names
and the collisions before anything is written.

**Delete has since gained the naming half of that step, but not the costed
half** (re-read 2026-08-21). `src/lib/deletePlan.ts` decides what one F8
removes *as data*, and `FileManager.tsx` asks about the entry and about the
paired `.info` it found — both by name — before a single block is journalled;
the whole batch then commits or rolls back as one (ART-081), so the icon is no
longer a second operation with a second chance to half-finish. What it still
does not have is `volume_plan_copy`'s other half: a panel stating the cost and
the consequences of the whole act before the first question is asked.
Overwrite is the same shape — the copy plan reports collisions for a
directory, and the per-name policy dialog asks when one is actually hit, but a
single-file copy carries its answer in ahead of the fact rather than being
shown one.

## Writing into a volume: two strategies, one API

> *A 2 GB image is not a big floppy. It needs a journal, not a bigger buffer.*

The ADF pipeline — read whole, mutate in memory, validate, back up, replace
atomically — is correct for 880 KB and unworkable for two gigabytes: a
whole-file backup per rename is minutes of I/O, and reading the image into
memory is the ART-021 mistake again. So `core/volume` picks a strategy from the
image's size, and callers never see which ran.

```
                     WriteStrategy::for_image(bytes)
                                  │
             ≤ 16 MiB ────────────┴──────────── > 16 MiB
                 │                                  │
           WholeFile                          BlockJournal
                 │                                  │
    std::fs::read → VecDevice            FileRegionMut over the partition
                 │                                  │
    VolumeWriter mutates in memory       VolumeWriter mutates in place
                 │                                  │
    guarded_write: backup + atomic       journal saved & fsynced first
                 │                                  │
                 └────────── same WriteOutcome ─────┘
```

Every mutation, whichever strategy, runs the same six steps:

```
1. load the allocator     the whole free-space map, one read per bitmap block
2. plan                   decide every block the operation will touch
3. journal those blocks   old contents saved and fsynced — nothing written yet
4. write                  through the journal, which refuses any other block
5. validate               re-read from the device and check what landed
6. commit, or roll back   never leave a half-written volume
```

Step 2 finishing before step 3 begins is the part that matters. The journal has
to know the **complete** block set up front, and an allocator that hands out
blocks lazily mid-write cannot provide it — which is why operations assemble a
`BlockSet` in memory first and hand it to one `commit` function.

`Journalled::write_block` refuses a block that was not named to
`Journalled::begin`. That single check is the whole safety property: a block
ART cannot undo is a block ART will not touch.

`commands/volume_write.rs::with_volume()` is the intended shape on the
`WholeFile` side: `read -> mutate -> validate -> backup -> commit`. Validation
happens on the in-memory result **before** anything reaches disk; the backup and
the atomic replace are `core/safety`'s job, described in
[security-model.md § Two safety modules](security-model.md#two-safety-modules-different-threats).

### One writer for OFS and FFS, two crates for everything else

`core/volume/` is ART's **own** filesystem writer and the only one for OFS and
FFS. It goes through a `BlockDevice` rather than a raw image buffer, so DD
floppies, HD floppies and hard-disk partitions are the same code path with
different geometry.

Two other filesystems are written, both through a crate and neither through
`core/volume`:

- **PFS3** via `libpfs3` (`core/preload/native.rs`) — the volume format a real
  PiStorm card carries.
- **FAT32** via `fatfs` (`core/fat32.rs`) — the PiStorm card's boot partition,
  and the one filesystem ART creates that is not an Amiga one.

**The root block is always computed** — `VolumeGeometry::root_block_for(total_blocks)`
— and never read from the boot block. A real AmigaDOS boot block has 68000 code
where a reader looking for a pointer would find one.

### Journal recovery is a mount-time step

`panic = "abort"` means an out-of-range index kills the process outright,
possibly between two block writes. So the journal outlives the process: it sits
next to the image as `<image>.artjournal`, and `scan_image` looks for it. A
journal found there means an operation died part-way, and **every write to that
image is refused** until the user decides — writing over a half-written volume
would leave the journal describing blocks that no longer hold what it recorded,
which is the one state nothing can recover from.

The journal identifies its image by **path and size**, not by modification
time. The recorded mtime is from *before* the operation, and a crash mid-write
is precisely the case where the file has changed since; gating on it would
reject every journal worth replaying. Size is the strong invariant instead —
these writes are in place and never resize the file.

## Workflow Engine design

Every operation in ART is a `Workflow`:

```rust
pub trait Workflow: Send + Sync {
    fn info(&self) -> &WorkflowInfo;          // id, name, safety, priority, ...
    fn can_handle(&self, d: &Detection) -> bool;  // routing
    fn run(&self, input: &Path, d: &Detection) -> CoreResult<WorkflowOutcome>;
}
```

The catalogue of workflows lives in one place, `core/workflow/builtin.rs`;
`lib.rs::build_engine()` simply calls `register_all` and registers everything it
declares. The engine turns a detection into an ordered candidate list and a set
of recommendations.

**Add a new action to the catalogue, not to `build_engine`.** The tests that
guard routing — `workflows_do_not_cross_formats`,
`every_recognised_format_has_a_recommendation`,
`every_workflow_route_is_a_real_app_route` — all read from the catalogue, so an
action registered anywhere else is an unguarded one.

`WorkflowInfo::kind` splits actions in two:

- `Navigate { route }` — the UI opens that route with the object's path in
  router state. Most actions are this, and `run()` deliberately refuses (the
  trait default): opening a studio is not engine work, so routing knowledge
  stays out of React. Routes in `builtin.rs::route` must match the
  `<Route path=...>` values in `src/App.tsx`, and a test enforces it.
- `Execute` — the engine performs the work and returns a `WorkflowOutcome`.
  Reached through the `run_workflow` command, **which refuses anything that is
  not `Safety::ReadOnly`**: a data-changing action must go through its studio's
  preview/backup/verify flow (spec §92), never straight off the drop panel.

Actions that are planned but not implemented are still registered, with
`available: false`, so they surface as "Coming Later" instead of silently
missing (spec §96). See [FEATURES.md](FEATURES.md) for which are which.

## Background work

Long operations (§54, §55) never run on the UI thread. The split follows the
core independence rule:

- `core/jobs` defines a `ProgressSink` an engine function reports through, and a
  cancel flag it checks. It knows nothing about threads or events.
- `commands/jobs.rs` runs the work on a background thread, throttles progress
  into `job-progress` events, and owns the registry the UI queries.

Cancellation is cooperative: an operation only observes the flag **between whole
units of work**, never mid-write. Together with `core/safety` that means
stopping can leave work unfinished, but never a half-written file. Return
`CoreError::Cancelled` when you stop; the runner turns it into a `Cancelled` job
state rather than an error.

A core function that can be long takes `&dyn ProgressSink` and keeps a thin
wrapper passing `NoProgress`, so callers who do not need a job are unaffected —
`scan_titles` / `scan_titles_with` (`core/gameindex/scan.rs`) is the shape to
copy.

## Operation log

Every operation that changes user data records what happened (§53): the action,
source and destination, where the backup went, whether verification passed, and
on failure the error ID (§68). `core/oplog` defines the record and an
`OperationLog` trait; the JSON Lines implementation writes beside the
application log.

A command that changes user data takes `oplog: State<'_, JsonlOperationLog>` and
goes through `write_result` in `commands/oplog.rs`; `commands/adf.rs` is the
worked example.

Recording is best-effort by design — a failure to log must never turn a
successful write into a reported failure. Failures are recorded too, with the
error's `ART-*` id from `CoreError::code()`. Those ids are user-facing: treat
them as stable.

## Safety classification

Every `WorkflowInfo` carries a `Safety` tag that drives the confirmation UI:

| Level | Meaning |
|-------|---------|
| `ReadOnly` | No writes anywhere. |
| `Safe` | Writes only to new/derivative files; originals untouched. |
| `RequiresBackup` | Modifies the original after an automatic backup. |
| `Destructive` | Requires explicit, double confirmation. |
| `Experimental` | Unproven; clearly flagged. |

See [security-model.md](security-model.md) for the full policy.

## State management

- **Tauri `State`**: long-lived engine objects (`WorkflowEngine`).
- **SQLite** (`sqlite:art.db`, `tauri-plugin-sql`): relational data
  (`settings`, `recent_files`, `jobs`). Migrations live in
  `src-tauri/migrations/`, are declared in `lib.rs`, and run lazily on the
  frontend's first `Database.load`. **Never edit a released migration** — add a
  new file. The title catalogue is **not** here — `core/gameindex/store.rs`
  keeps it as one JSON file per scanned root, because it is rebuilt from a
  folder scan rather than queried relationally.
- **JSON store** (`tauri-plugin-store`, `settings.json`): key/value preferences
  (`theme`, `uxMode`, `language`, paths).
- **Zustand** (`src/stores/`): live UI state mirroring the persisted stores.

A new command goes in **both** `invoke_handler![]` in `lib.rs` and a typed
wrapper in `src/lib/*.ts`; the frontend never calls `invoke` directly from a
component. New plugin permissions go in `src-tauri/capabilities/default.json`.

The frontend uses `HashRouter` (routes in `App.tsx`) and the `@/*` alias, which
is declared in **both** `tsconfig.json` and `vite.config.ts` — keep the two in
sync.

## Where scratch goes, and where a deleted file goes

Two rules about the host's own disks, both of them the user's ruling rather than
a convenience.

**Every staging site goes through one root** (`src-tauri/src/scratch.rs`,
ART-196; `scripts/scratch-root-sweep.py` is blocking in CI). Preview
extractions, install staging, unpacked packages, the emulator's launch
configuration: none of them may call `std::env::temp_dir()` for themselves.

`core/` never chooses where to stage. A core function that needs somewhere to
work **takes the directory**, and the command layer hands it the one this module
resolved. The default is the platform temp dir, so nothing changes for a user
who never opens the setting; but **a chosen root that is not usable is a
refusal, never a fallback** (`AppError::ScratchUnavailable`), because silently
staging on `C:` after the user said "not `C:`" is the confident-and-wrong class
of defect this project pays most for. Repointing the root moves and deletes
nothing.

**A file removed from the user's own disk goes to the Windows Recycle Bin**
(`core/hostfs.rs` -> `tools/recycle_bin.rs`, ART-080). ART invents no recovery
mechanism of its own and uses the one place the user already knows to look.

And unlike `core::volume::write::delete_many`, which is all-or-nothing because a
disk image has a journal, **a host filesystem has none**: twelve files recycled
one by one are twelve completed operations that a thirteenth failure cannot
undo. So the outcome is reported **per entry**, by name and by result, and no
screen claims otherwise.

## Bounds checking in the ADF core

Block numbers arrive from the frontend (`dirBlock`, `headerBlock`) and from
corrupt images. The release profile sets `panic = "abort"`, so an out-of-range
index kills the entire application. **Never index the image directly.** Which
helper depends on which side you are on:

- **Reading a whole image** (`core/adf`, validation) — `blocks::block_slice` and
  `blocks::read_u32_at`. They compute the offset with `checked_mul`, verify
  containment, and turn a bad number into a `CoreError`.
- **Writing** (`core/volume/write`) — `BlockSet` in `layout.rs`. It stages
  blocks in a `BTreeMap` rather than slicing a buffer, so there is nothing to
  index out of range. The old whole-image mutators (`block_slice_mut`,
  `write_u32_at`) were deleted along with `core/adf/mutate.rs`; do not
  reintroduce that shape.

Chain walks — hash buckets, file extension blocks — need a step limit. A
malformed image can loop forever.

## A card is a list of disks, not a disk

Learnt from two real PiStorm cards, not from a document
([sd2-card-layout.md](sd2-card-layout.md)). An SD card written for Emu68 is an
**MBR** with a FAT32 primary and one to three `0x76` primaries, and the m68k
side sees each `0x76` area as a **separate hard drive** — so each carries its
**own RDB, at a byte offset inside the card**, never at offset 0.

- `core/mbr.rs` reads the four primary entries and nothing else; no MBR at all
  means one area at offset 0, which is what a plain HDF is. It **writes** one
  too (`plan_card` / `write_mbr`), and the defaults there are measured off the
  two real cards rather than chosen — including that an Amiga disk at byte zero
  is not expressible, which is how SD-0's unit-0 rule is enforced.
- `core/card/mod.rs` models the card as `Vec<AmigaArea>`. Its two methods exist
  because of ART-097: `file_systems()` unions the drivers across **all** areas,
  and `partitions_missing_driver()` asks against that union. A card whose second
  RDB carries the FFS driver and whose first carries PFS3 boots fine; asking one
  RDB in isolation reported fifteen working partitions as broken.
- RDB block numbers inside an area are relative to the **area base**. Both
  halves of that are built: writing *into* a volume at an offset was ART-043,
  and laying an RDB *at* one is `core/card/build.rs`.

**Never read a whole card**: `read_card` takes an 8 MB window per area.

Building one (`core/card/build.rs`, `core/card/payload.rs`, `core/fat32.rs`) has
three rules of its own:

- **Nothing may reach past its partition.** `fat32::Region` maps offset zero to
  the boot partition's first byte and *refuses* a write past its last, because
  the Amiga's first RDB begins where that partition ends.
- **Say what the release says.** The Emu68 archive's own `config.txt` names the
  kernel it boots — `Emu68-pistorm.gz`, not the `Emu68.img` ART used to write
  over it (ART-103) — and the archive's name means different boards in the two
  release lines (ART-091). Neither is ART's to guess; both are checked against
  the files actually being placed.
- **A card is verified by something that is not ART.**
  `scripts/fat-oracle-check.py` reads the boot partition with 7-Zip, which is
  how `fatfs`'s two directory defects were found (ART-102).

## Installing an OS: a component is a set of paths

`core/osinstall/` (`recipe.rs`, `source.rs`, `scan.rs`, `plan.rs`, `apply.rs`,
`startup.rs`, `verify.rs`) turns the user's own AmigaOS install media into a
populated system volume, without running the Amiga Installer.

It does not do this disk by disk. `ModulesA1200_3.2.adf` holds fourteen commands
in `C/`, and **thirteen are older copies of commands `Workbench3.2` already
carries** — copying the whole disk onto `SYS:` would downgrade thirteen commands
in order to install one new one (`LoadModule`).

So a **component** is a named set of `PathRule`s (`from` on the media, `to` on
the tree, `File` or `Subtree`), not "copy this disk". Recipes are **data** —
`core/osinstall/recipes/amigaos-3.2.json` — so a future release (3.9,
CaffeineOS) adds a JSON file, not a code path. No two components may claim the
same destination without one declaring an `overrides` relationship, and a test
enforces that over the shipped recipe.

The engine's product is a **distribution tree**, not a volume: a host folder that
is the finished system volume file for file, an Amiga-metadata `.uaem` sidecar
beside each one, and a `distribution.json` at the root recording which component
and which media every file came from. That record is what makes a later
component add or remove possible, and what lets the whole engine run in a
tempdir with no volume, no driver and no external binary at all. Putting the
tree onto an actual card is `core/preload`'s job.

### Which distributions ART knows about is data

`core/distro/`'s registry is a JSON file compiled in with `include_str!` —
reviewable in a diff and unable to grow a code path of its own.

**ART never downloads a distro image**: no URL list, no fetch button. The
`homepage` field is where the *user* goes, and ART then accepts the local file
they came back with. That is a legal line, not a preference.

### What a catalogued title is called is sourced

`core/gameindex` keeps the distinction in the type rather than in a comment: an
`.rp9` manifest and a WHDLoad slave's header **state** a title; a filename and a
drawer name only **suggest** one (`Lotus3HD` and `Moonstone Install` are drawers
for games called *Lotus 3* and *Moonstone*). The screen then marks a suggestion
as guessed rather than presenting it as a fact.

Same rule as "ask the artefact", one layer down. `core/artwork` reads
`core::gameindex::record` types and **must never be imported by
`core/gameindex`**: joining a title to its picture is a command-layer job, the
same shape as `core/rom/pairing.rs` above.

## Two ways to write a PiStorm volume

`core/preload::VolumeFormatter` (`probe`, `import_filesystem`,
`format_partition`, `copy_in`) has two implementations: `tools/hst_imager.rs`,
which launches `hst.imager.exe` and therefore lives outside `core/`, and
`core/preload/native.rs`, which launches nothing — PFS3 through `libpfs3`, FFS
through ART's own `core/volume/write`.

**Native is the default and `hst-imager` is a named fallback, chosen per
operation, never per run.** `commands/preload.rs::run_with_fallback` tries the
native path first for every step of a plan and only reaches a configured
`hst-imager` for two known, typed capability gaps:

- non-ASCII AmigaDOS names on a PFS3 volume, which `libpfs3` 0.1.3 cannot
  round-trip (`CoreError::NonAsciiPfs3Names`, ART-113);
- embedding a filesystem driver into a *foreign* card's existing RDB in place,
  which ART's own RDB writer cannot do without risking silently shifting every
  partition after the first (`CoreError::ForeignRdbEmbedNotSupported`, ART-117).

Both are refused **before a single byte is written**, which is what makes
retrying on the other tool safe. The fallback is never silent: every step's
result carries which tool ran it and, when it was not the default, why — logged
and shown on the confirmation screen before the destructive step runs, not only
afterwards in the result panel.

## AmigaDOS compatibility

`hash::name_hash(name, international)` must match `adfGetHashValue` exactly,
**including the `& 0x7ff` mask applied after every character**. Dropping it
produces images ART can read back but that AmigaDOS and WinUAE cannot. Reference
values are pinned in tests. The `international` flag comes from the volume's
bootblock, never assumed.

### AmigaDOS scripts ART writes

`core/firstboot/scripts/` is AmigaDOS text ART puts on a user's system volume and
the Amiga executes on its first boot. Every rule below was **measured under
WinUAE on 2026-09-07**, after seven tasks of green tests had passed against
scripts that stopped at their first step on a real shell
([ART-272](ISSUES.md)/[ART-273](ISSUES.md)):

- **Read a variable as `${name}`, never `$name`.** `$ART_System` did not expand
  and `${ART_System}` did; a bare name with an underscore is the case that
  fails. `every_variable_read_is_braced` scans every script for a `$` not
  followed by `{`.
- **Never `Quit`.** A `Quit` inside an `Execute`d script ends every script above
  it — the wrapper, the generated run list and the dispatcher all went with the
  first step's `Quit 0`. A step leaves through `Skip end` / `Lab end` and refuses
  by `Set ART_StepRc 20`, which the wrapper resets before the step and reads
  after. `no_script_ever_quits` guards it.
- **`List` does not sort.** It answered `30, 20, 10`; the run list goes through
  `C:Sort` before it executes, and `plan` refuses a tree missing any of the
  eight disk commands the scripts run (`NEEDED_COMMANDS`).
- **A script cannot delete itself while it runs.** The finished state is "the
  step directory is gone", never "the dispatcher is gone".
- A `.KEY` script's brackets are `[]`, because `${...}` inside a `{}`-bracketed
  script is taken for a key. Scripts are ASCII with LF endings and pinned by
  SHA-256 (`every_fixed_file_hash_is_pinned`): an edit lands only with its pin.

The guards read the text; the proof is the gated real boot
(`rehearse_the_real_tree_when_asked`, see
[testing.md § Real material](testing.md#real-material-and-the-ignored-hooks)).
Emu68 Hatcher's own scripts, which run on real hardware, follow the same rules.

**A sixth rule, about the other script ART writes.**
`core/amigainstall/workvol.rs` generates a one-shot `Startup-Sequence` that runs
a package's own installer and then reports through `If Warn`. **That branch is
unexercised, not proven** — measured 2026-09-08 with a controlled experiment on
the owner's own BoingBags, one variable, each arm twice, the control measured:
a payload with **one byte** changed does not make the `Updater` set WARN, it
makes it **hang** (`TimedOut` at 1 800.7 s, twice, stopped on the payload entry
the byte falls in), and the real archive applied to a **wrong target** returns
**`ok`, twice**. So the word the Amiga writes says the program returned without
WARN and nothing more. **And the artefact to ask afterwards is not the one the
package updates**: `Libs/version.library` reads `45.3` byte-identically on a
correctly chained tree and on one that skipped BoingBag 1 and is missing 57
files — the package writes that string either way — while
`Libs/xadmaster.library` separates them, 9.1 against 9.0. **What protects the
user here is the refusal before the run**
(`core/osinstall/chain.rs::refuse_unless_installable`, 17.6 ms, nothing copied),
which is a *bookkeeping* guard: it reads a manifest only a successful ART run
writes. Numbers and both arms: ART-227.

**The script carried a second, version-gated invocation for one day** — a
package's `follow_ups`, HstWB's own shape for BoingBag 3.9-2's `XAD-Update`
(ART-280) — and the rule it bought is worth keeping even though the mechanism
went with the two BoingBags' emulator route on 2026-09-09: **`Version … FILE`
sets WARN as its answer, not as an error**, so a gate emitted *above* the
`If Warn` that turns the installer's own return code into the result word would
overwrite the installer's verdict with the gate's, and every successful run on a
tree with an old `xadmaster` would be reported as *the installer said no*. The
second payload is placed from the host now (`boingbag-39-2.json`'s
`extra_members`, behind the same gate read from HstWB's own script), so the
generated script is back to one invocation and one result word.

## Config files are user data

**Never regenerate a user's config file from scratch.** `FF.CFG` (FlashFloppy),
`config.txt` and `cmdline.txt` (Raspberry Pi) are hand-tuned and carry dozens of
settings ART knows nothing about.

Each generator takes an `existing: Option<&str>` and **edits in place**: managed
keys are rewritten, and everything else — comments, ordering, unknown keys —
passes through verbatim. Regenerating `cmdline.txt` drops `root=` and the Pi
stops booting. Spec §39 and §40 both mandate this.

When adding a managed key, add it to the module's `managed_*` list rather than
writing it directly.

## The UI layer's own rules

### Beginner and Power User mode

`usePowerMode()` (`src/lib/uxmode.ts`) decides what the UI shows. Beginner mode
hides the raw-data studios, the Advanced action group and block-level numbers
(§47, §48).

It **only hides**. Never disable an operation based on the mode, and never
change what ART does.

### Nothing changes unless the user changes it

The user's rule, and it holds for **every** feature, not just Settings: no
choice ART offers may reset itself between runs. A studio's last folder, a pane's
sort order, a filter, a window's Application Size — if the user set it, it comes
back.

- `src/lib/remembered.ts` is the one way to do it. `recall`/`recallInto` read a
  persisted value **through a guard** (`isOneOf`, `isWholeNumberBetween`,
  `nullOr`), so a hand-edited or stale settings file falls back to the default
  instead of putting a bad value on screen.
- The read is asynchronous and the user is not. `settingsStore` tracks which
  keys the user has touched this run and refuses to let a late-landing read
  overwrite them — ART-089 was that bug from the other side, and a setting that
  changes without the user changing it is the one outcome this rule forbids.
- **Application Size** (`src/lib/appZoom.ts`, Ctrl +/-/0) exists because most of
  the people using this are over fifty. It is a first-class setting, not an
  accessibility afterthought; new screens inherit it from the shell and must not
  fight it with fixed pixel heights.
- **A found artefact pre-fills a field; a chosen one overrides it; a re-scan
  never replaces a choice.** The OS Builder resolves the build's material once
  (`core/osinstall/slots.rs`) and fills the Amiga-side panel's fields from the
  answer — but a path the user picked by hand wins over it, is labelled as
  theirs, and outlives every later pass. Taking the choice back is its own
  control (*Use the one ART found*), and it `forget`s the remembered key rather
  than storing a `null`: a stored "nothing" is itself a decision, and it would
  switch the pre-fill off for good.

### The OS Builder is a wizard, and one value carries the build

`/os-builder` is a shell — a progress strip over an `<Outlet/>` — with the steps
as **real sub-routes** (`hedef`, `kaynak`, `paketler`, `amiga-kurulum`, `kart`,
`birimler`), so back/forward and a jump to a step work at the router level.
`src/lib/buildSteps.ts` says which steps a build kind has and whether one can
act; it is pure and returns i18n **keys**.

**The build's own values live in one place** (`src/lib/buildSession.ts` +
`useBuildSession.ts`), and the reason is ART-197: the folder ART *wrote* and the
folder the next step *operated on* were two remembered keys joined by nothing,
so a user who had just watched ART write 1915 files was asked to go and find
them.

It is a **typed facade over `settingsStore`**, not a second store. A parallel
store would have to re-answer ART-089 (a load landing after the user acted) and
ART-178/ART-195 (a fresh identity per render driving effects into a loop), plus
the guards. Legacy keys are read once, never written again and never deleted, so
a rollback still finds them.

**What the facade owns — narrowed 2026-09-05 to match the tree.** A value the
build *produces*, or one that has to survive from the step that writes it to a
later step that reads it, goes in the facade: that pair is what ART-197 was. A
value a single step **asks for and consumes in the same render** does not — the
media folders are the standing example, and the layered-release round added the
per-layer ones the same way, after checking that ART-197's failure cannot reach
them. Three media fields now persist outside the facade.

When you are unsure which kind a value is, one question decides it: **can the
value ART wrote and the value the next step operates on drift apart?** If yes it
belongs in the facade, whatever it looks like.

**The `amiga-kurulum` step is the update chain** (round 3 of the intake work):
one row per link of the release's own order, from `core::osinstall::chain`
through `osinstallChain` and `src/lib/chain.ts`, with a single Run button that
runs the row the user selected or, when they selected none, the first *ready*
row — and names it. Which of two routes a row takes is the **row's** answer,
`SentenceFacts::runs_on_amiga`: an Amiga-side row goes through `compose →
install`, a host-placed one through the `paketler` step's own
`osinstall_collisions` → `osinstall_add_package`, shared as
`components/osbuilder/HostPlacement.tsx` rather than copied.

**A row's state comes from the manifest and the slots, never from a file being
present.** `distribution.json` is what makes a row *installed*, and the resolved
slot is what makes it *ready* or *missing*; a screen that inferred either from a
file on disk would tell somebody a package was applied because its archive was
still in the folder.

### Strings: two catalogues, and `src/lib` never renders one

`react-i18next`, English **and Turkish** (`src/i18n/en.json`, `tr.json`). Add or
change a key in **both files in the same commit** — `pnpm test` fails the build
if the key sets differ, a value is empty, or an interpolation variable present in
one is missing from the other. The live key count is in
[STATUS.md](STATUS.md); count them rather than quoting a number from prose.

`src/lib/*` is pure TypeScript with no i18next singleton, so a helper that builds
a message returns a `Phrase { key, params? }` and the *component* calls
`t(phrase.key, phrase.params)`.

The three helpers that cannot finish their own sentence return `PartialPhrase<K>`
(`src/lib/phrase.ts`), whose `params` type is poisoned so that passing it
straight to `t()` fails to compile — i18next renders a missing variable as a
literal `{{now}}` on screen, and that is the bug the type exists to catch.
Nothing in the build catches a `Phrase` pointing at a key nobody added either,
so `src/i18n/phrase-keys.test.ts` enumerates every variant of every such mapper
and asserts it resolves to a real leaf.

Rust-side strings (`CoreError` messages, `WhdloadRefusal.reason` /
`.suggestion`) are not in this system yet and stay English whatever the chosen
language — ART-060.

## Why not a Cargo workspace?

ART keeps the core inside the same crate as the shell (`src-tauri`) as a
`core/` module tree. This is simpler to bootstrap and still enforces the
independence rule (the core simply doesn't import `tauri`). If the core grows
large or a CLI is added, promoting `core/` to its own crate in a workspace is
a mechanical refactor — the public API won't change.
