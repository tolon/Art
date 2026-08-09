# Roadmap

ART is built phase by phase. Each phase results in a working application.
**Never proceed with a broken build.** Do not implement future-phase features
until the current phase is stable.

> **This file defines what each phase *contains*. It does not track progress.**
> The ✅/⏳ marks below are the original plan as written at Phase 0 and are no
> longer accurate.
>
> - Current position and stage ordering → [STATUS.md](STATUS.md)
> - Whether a specific feature exists → [FEATURES.md](FEATURES.md)
>
> Work is now scheduled in the stages described in STATUS.md, which cut across
> these phases: data safety and the Workflow Engine were pulled forward because
> later phases — and both spec addenda — depend on them.

Legend: ✅ Done · 🚧 In progress · ⏳ Planned

## Phase 0 — Foundation ✅

- ✅ Tauri 2 + React 19 + TypeScript + Vite shell
- ✅ Platform-independent Rust core (module skeletons)
- ✅ Format detection (ADF/ADZ/DMS/HDF/HDZ/LHA/ROM/directory)
- ✅ SHA256 hashing (streaming)
- ✅ Workflow Engine (trait + registry + plan)
- ✅ Universal Drag & Drop Manager
- ✅ SQLite (settings, recent_files, jobs) + migrations
- ✅ Logging (stdout + log dir)
- ✅ Settings store (theme, UX mode, language, paths)
- ✅ i18n architecture (English; ready for more)
- ✅ Dashboard, Settings, "Coming Later" placeholders
- ✅ CI (Windows x64: lint, fmt, clippy, test, build)
- ✅ cargo-deny policy
- ✅ Documentation set

**Deliverable:** A clean application that builds, runs, tests green, and
installs on Windows.

## Phase 1 — ADF + LHA ⏳

- ⏳ ADF Studio: open, browse, edit, create, validate, extract
- ⏳ LHA Studio: open, browse, extract, create, test, validate
- ⏳ Filesystem browser (OFS/FFS-aware)
- ⏳ WHDLoad detection (confidence levels)
- ⏳ Drag-and-drop workflows wired to real engines

## Phase 2 — WinUAE + Profiles ⏳

- ⏳ WinUAE detection + manual path
- ⏳ Amiga Profile Studio
- ⏳ Compatibility foundation
- ⏳ ADF / HDF launch

## Phase 3 — Collection ⏳

- ⏳ Collection Studio
- ⏳ Folder scanner (adf/adz/dms/lha/hdf/hdz)
- ⏳ SQLite metadata
- ⏳ Search, tags, favorites
- ⏳ Duplicate detection (SHA256)

## Phase 4 — HDF ⏳

- ⏳ HDF Studio: create, open, inspect, browse, edit
- ⏳ RDB inspection + partition management
- ⏳ Filesystem detection (FFS/PFS3/SFS)
- ⏳ Resize, clone, backup, validate, optimize, repack, migration, snapshots

## Phase 5 — Image Lab ⏳

- ⏳ Format identification
- ⏳ Conversion (DMS→ADF, ADF→ADZ, ADF→HDF)
- ⏳ Compression / decompression
- ⏳ Image comparison (diff)
- ⏳ Health analysis + forensics

## Phase 6 — Gotek ⏳

- ⏳ Gotek Studio: USB detection, preparation, ADF management
- ⏳ FlashFloppy Studio: FF.CFG editor
- ⏳ Bulk workflow: drag → validate → dedupe → organize → copy → verify

## Phase 7 — ROM + Binary ⏳

- ⏳ ROM Studio: identification, checksum, SHA256, version, library
- ⏳ Binary Inspector: Hunk, executable, slave, icon
- ⏳ Hex Viewer (read-only)

## Phase 8 — Recovery ⏳

- ⏳ Disk Analyzer (read-only forensics)
- ⏳ Recovery Lab (filesystem consistency, boot block recovery, bitmap repair)
- ⏳ Safe repair only — always preserve originals

## Phase 9 — Launcher ⏳

- ⏳ Amiga Launcher: double-click → identify → profile → WinUAE → launch
- ⏳ Automatic profile selection
- ⏳ Launch history

## Phase 10 — PiStorm ⏳

- ⏳ PiStorm Studio: configuration inspection/edit/validate
- ⏳ Backup / restore
- ⏳ Profile integration
- ⏳ Safe raw-device operations (double confirmation)

---

## Phase completion criteria

Each phase must satisfy:

- Build: PASS
- Tests: PASS
- No critical errors
- No obvious data-loss risk
- UI remains responsive
- Documentation updated
- CHANGELOG updated
