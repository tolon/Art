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
│  adf · hdf · lha · rdb · rom · collection    │
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
│   │       ├── adf/            #     bootblock, blocks, fs, extract, mutate...
│   │       ├── lha/            #     archive, safe_extract, whdload
│   │       ├── hdf.rs, rdb.rs  #     hard disk images + partition tables
│   │       ├── gotek.rs, pistorm.rs, winuae.rs, rom.rs, profile.rs
│   │       ├── collection.rs, analysis.rs
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

`src-tauri/src/core/` compiles with **only `std` + `serde` + `sha2` + `thiserror`
+ `delharc` + `zip` + `sevenz-rust2` + `fatfs` + `libpfs3`** — the three
decompressors are read-only and sit behind `core/archive`'s single security
gate, and `fatfs` creates the one filesystem ART writes that is not an Amiga
one: the FAT32 partition a PiStorm card's Raspberry Pi boots from. `libpfs3`
is the PFS3 implementation — the volume format SD-2 G5's OS install writes
and reads on a PiStorm card — and the one LGPL-3.0-or-later dependency inside
`core/`: weak copyleft, compatible with ART's own GPL-3.0-or-later, but noted
deliberately against the project's preference for permissive dependencies,
because `core/` is meant to be promotable to a standalone crate. It never
imports `tauri`, never calls Windows APIs, never touches the network.
This is what makes it unit-testable and what leaves the door open to a future
CLI or other shells without rewriting the engine.

Concretely: if a `core/` module needs to do something platform-specific (open
a file dialog, detect a USB drive, launch WinUAE), it exposes a **trait**, and
the implementation lives outside the core.

## Data flow: the DROP pipeline

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

The execute half of the pipeline is in place for operations that modify data:
`core/safety` performs `BACKUP → APPLY` atomically and the volume writer's
`commit_whole_file` (`commands/volume_write.rs`) validates the whole image
before committing. Copying into a volume also has the **preview**
step §92 asks for — `volume_plan_copy` reports the cost, the unstorable names
and the collisions before anything is written. Delete and overwrite still tell
the user what happened rather than what is about to.

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
`lib.rs::build_engine()` simply registers everything it declares. The engine
turns a detection into an ordered candidate list and a set of recommendations.

`WorkflowInfo::kind` splits actions in two:

- `Navigate { route }` — the UI opens that route with the object loaded. Most
  actions are this, and `run()` deliberately refuses: opening a studio is not
  engine work, so routing knowledge stays out of React.
- `Execute` — the engine performs the work and returns a `WorkflowOutcome`.

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
units of work**. Together with `core/safety` that means stopping can leave work
unfinished, but never a half-written file.

## Operation log

Every operation that changes user data records what happened (§53): the action,
source and destination, where the backup went, whether verification passed, and
on failure the error ID (§68). `core/oplog` defines the record and an
`OperationLog` trait; the JSON Lines implementation writes beside the
application log.

Recording is best-effort by design — a failure to log must never turn a
successful write into a reported failure.

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
- **SQLite**: relational data (`recent_files`, `jobs`, future `collection`).
- **JSON store**: key/value preferences (`theme`, `uxMode`, `winuaePath`).
- **Zustand**: live UI state mirroring the persisted stores.

## Why not a Cargo workspace?

ART keeps the core inside the same crate as the shell (`src-tauri`) as a
`core/` module tree. This is simpler to bootstrap and still enforces the
independence rule (the core simply doesn't import `tauri`). If the core grows
large or a CLI is added, promoting `core/` to its own crate in a workspace is
a mechanical refactor — the public API won't change.
