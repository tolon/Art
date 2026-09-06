# First boot, phases 1–2 — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** ART writes a one-shot first-boot mechanism into a distribution tree, reads its report back, and can rehearse it under WinUAE — with the silent hardware step as the first real step.

**Architecture:** A new `core/firstboot/` module holds fixed AmigaDOS scripts (compiled in with `include_str!`), a `plan()` that refuses before writing, a `write()` that puts the files into a tree and merges one block into `S:User-Startup` through the existing `merge_user_startup`, and a `report.rs` that parses `S:FirstBoot.log`. The rehearsal lives beside `core/amigainstall` and boots the tree copy itself. Commands are thin; a new OS Builder step and a card-report panel render it.

**Tech Stack:** Rust (std, serde, thiserror), Tauri commands, React + react-i18next + Vitest/jsdom, AmigaDOS scripts.

**Spec:** `docs/superpowers/specs/2026-09-07-firstboot-design.md`. This plan covers spec §11 phases 1 and 2. Phases 3 (wizard) and 4 (packages) get their own plans after phase 2 closes, re-checked against the tree.

## Global Constraints

- `core/` is platform-independent: `std` + `serde` + `serde_json` + `sha2` + `log` + `thiserror` and the listed decompressors only. No `tauri`, no Windows API, no network in `core/firstboot`.
- `core/firstboot` may read `core::osinstall::recipe`/`package` types and call `core::osinstall::startup::merge_user_startup`; `core/osinstall` must never import `core::firstboot`.
- Every write into a user's tree goes through `core::safety` (`atomic_write`, or `guarded_write` with `BackupPolicy::CONFIG` for `S/User-Startup`). Never `std::fs::write` on a user file.
- A file with default protection (`----rwed`) gets **no** `.uaem` sidecar — that is `core/osinstall/apply.rs::settle_sidecar`'s own rule via `sidecar_for` (a sidecar is written only when protection, date or comment is "interesting"). The spec's §4.1 sentence "every file gets a `.uaem` sidecar" is corrected by Task 12.
- Test scratch: `core::ScratchDir::new(prefix, tag)`, never a bare temp path; names are counter-unique per process already.
- Long work runs as a job: `commands/jobs.rs::spawn_job`, `&dyn ProgressSink`, `is_cancelled()` checked between whole units, `CoreError::Cancelled` on stop.
- Every staging directory comes from `crate::scratch::root()`; `core` takes the directory, never chooses it.
- New commands go in **both** `lib.rs`'s `invoke_handler![]` and a typed wrapper under `src/lib/`; components never call `invoke`.
- Every new i18n key lands in `src/i18n/en.json` **and** `tr.json` in the same commit.
- Rust-side sentences stay English (ART-060). AmigaDOS script text is plain ASCII, LF line endings, written with the Write tool (never through a heredoc — CLAUDE.md's control-byte trap).
- Cargo runs: quote the `test result:` line, not the exit code (ART-261). Use `cargo test --lib firstboot` for this module and `cargo test --lib -- --skip artwork` for the suite.
- Commit messages via `git commit -F <file>`, never a double-quoted shell string.
- Branch: `art-firstboot` (already created, spec committed as `ea520c4`). `git branch --show-current` before every commit.

---

## File structure

**Create**
- `src-tauri/src/core/firstboot/mod.rs` — module doc, `FirstBootRequest`, `FirstBootPlan`, `PlannedStep`, `FatMount`, constants, re-exports.
- `src-tauri/src/core/firstboot/scripts/ART-FirstBoot` — the dispatcher.
- `src-tauri/src/core/firstboot/scripts/ART-FirstBoot-Step` — the per-step wrapper the dispatcher calls.
- `src-tauri/src/core/firstboot/scripts/10-hardware`, `20-aux`, `30-datatypes` — fixed steps.
- `src-tauri/src/core/firstboot/scripts/SD0pi3`, `SD0pi4` — mountlists (from emu68hatcher, MIT).
- `src-tauri/src/core/firstboot/scripts.rs` — `include_str!` table + exact-match tests.
- `src-tauri/src/core/firstboot/report.rs` — `FirstBootReport` parser.
- `src-tauri/src/core/firstboot/plan.rs` — `plan()`.
- `src-tauri/src/core/firstboot/write.rs` — `write()`.
- `src-tauri/src/core/amigainstall/rehearse.rs` — boot the tree copy, poll the report.
- `src-tauri/src/commands/firstboot.rs` — `firstboot_preview`, `firstboot_write`, `firstboot_rehearse`, `card_firstboot_report`.
- `src/lib/firstboot.ts` (+ `firstboot.test.ts`) — wrappers, wire types, phrase mappers.
- `src/components/osbuilder/FirstBootPanel.tsx` (+ `.test.tsx`).
- `src/components/card/FirstBootReportPanel.tsx` (+ `.test.tsx`) — the card-side report table.
- `.superpowers/sdd/2026-09-07-firstboot/spike-updater-cd.md` — the spike's table.

**Modify**
- `src-tauri/src/core/mod.rs` — `pub mod firstboot;`
- `src-tauri/src/core/error.rs` — three `CoreError` variants + codes.
- `src-tauri/src/core/amigainstall/mod.rs` — `pub mod rehearse;`; `run.rs` — `pub(crate)` on `end_session`, `read_outcome` stays private (rehearse has its own reader).
- `src-tauri/src/commands/mod.rs`, `src-tauri/src/lib.rs` — module + four commands.
- `src/lib/buildSteps.ts` (+ test) — `ilk-acilis` step.
- `src/lib/buildSession.ts`, `src/lib/useBuildSession.ts` — `firstboot: { written: boolean }` section.
- `src/pages/osbuilder/steps.tsx`, `src/App.tsx` — the route.
- `src/pages/PistormStudio.tsx` or wherever `card_open` is rendered (Task 11 finds it with `grep -rn "cardOpen(" src`) — mount the report panel.
- `src/i18n/en.json`, `src/i18n/tr.json`.
- `THIRD_PARTY_LICENSES.md` — the two mountlists.
- `docs/FEATURES.md`, `docs/ISSUES.md`, `docs/STATUS.md`, `docs/session-log.md`, `CHANGELOG.md`, the teardown note, the spec (Task 12).

---

### Task 1: Spike — what `Updater` reads from the AmigaOS 3.9 CD

**Files:**
- Create: `.superpowers/sdd/2026-09-07-firstboot/spike-updater-cd.md`
- Throwaway edit (reverted before commit): `src-tauri/src/core/amigainstall/run.rs::media_for`

**Interfaces:**
- Produces: a table in the report — which directories/files of the disc the BoingBag 39-1 `Updater` needs to pass its "Checking AmigaOS 3.9 CD-ROM" step, and the byte total. Phase 4's plan reads it. No code survives.

- [ ] **Step 1: Confirm the existing real-material hook still runs with the ISO**

Read `src-tauri/src/commands/amigainstall.rs` lines 2560–2700 (the module doc of the `#[ignore]`d hook names every env var). Run it once as-is so the control arm is measured:

```bash
cd src-tauri
ART_AMIGA_TREE=E:/amiga/ProjeART/art159-boot ART_AMIGA_ROM=<owner's kick40068.A1200> \
ART_WINUAE=<winuae.exe> ART_AMIGA_PACKAGES=<folder with BoingBag39-1.lha> \
ART_AMIGA_PACKAGE_ID=boingbag-39-1 ART_AMIGA_CD=<AmigaOS39.iso> \
cargo test install_a_real_package_when_asked -- --ignored --nocapture 2>&1 | tee ../.superpowers/sdd/2026-09-07-firstboot/spike-arm0-iso.txt
```

Expected: the test's own output reports `Succeeded` (the 2026-08-21 result). If it does not, stop: the control arm has changed and the spike cannot measure anything.

- [ ] **Step 2: Extract the ISO to a directory**

```bash
mkdir -p E:/amiga/ProjeART/art-spike-cd/full
7z x -oE:/amiga/ProjeART/art-spike-cd/full <AmigaOS39.iso>
du -sb E:/amiga/ProjeART/art-spike-cd/full
```

Record the byte total.

- [ ] **Step 3: Throwaway edit — mount a directory as the `AmigaOS3.9` volume**

In `run.rs::media_for`, where `cd_image` becomes a CD mount, add: when `request.cd_image` is a **directory**, push a `DirMount { host_path, volume: "DH3", label: "AmigaOS3.9", boot_priority: -128, read_only: true }` instead. Read `media_for` first; copy the shape of the existing package-volume `DirMount` line. Do not commit this edit.

- [ ] **Step 4: Arm 1 — the whole directory**

Run Step 1's command with `ART_AMIGA_CD=E:/amiga/ProjeART/art-spike-cd/full`. Expected: `Succeeded`. If it fails, an assign-as-volume does not satisfy `Updater` and the spike's answer is "a directory cannot stand in for the disc"; write that and stop at Step 7.

- [ ] **Step 5: Narrow by halves**

Make `E:/amiga/ProjeART/art-spike-cd/sub-N` copies, each with half of the previous arm's top-level entries removed, run each, and record `Succeeded` / not. Then narrow inside the surviving directories the same way, to the file. Stop when removing any remaining entry fails the run. Keep every arm's command line and result.

- [ ] **Step 6: Write the report**

`.superpowers/sdd/2026-09-07-firstboot/spike-updater-cd.md`, with: the control arm's result, a table `arm | what was present | bytes | result`, the final minimal set, and one sentence on what is still not proven (this is 39-1's `Updater` 45.15; 39-2's may differ — run 39-2 against the minimal set as the last arm and record it).

- [ ] **Step 7: Revert the throwaway edit and commit the report only**

```bash
cd src-tauri && git checkout -- src/core/amigainstall/run.rs && git status --short
```

Expected: only the new `.md` and the two arm logs untracked. Commit them with a message file: `docs: spike -- what the BoingBag Updater reads from the 3.9 disc`.

---

### Task 2: The fixed scripts, compiled in and pinned

**Files:**
- Create: `src-tauri/src/core/firstboot/mod.rs`, `scripts.rs`, `scripts/ART-FirstBoot`, `scripts/ART-FirstBoot-Step`, `scripts/10-hardware`, `scripts/20-aux`, `scripts/30-datatypes`, `scripts/SD0pi3`, `scripts/SD0pi4`
- Modify: `src-tauri/src/core/mod.rs` (add `pub mod firstboot;` in alphabetical order after `fat32`)
- Modify: `THIRD_PARTY_LICENSES.md`

**Interfaces:**
- Produces: `pub const DISPATCHER: &str`, `pub const STEP_WRAPPER: &str`, `pub const STEP_10_HARDWARE: &str`, `pub const STEP_20_AUX: &str`, `pub const STEP_30_DATATYPES: &str`, `pub const SD0_PI3: &str`, `pub const SD0_PI4: &str` in `scripts.rs`; `pub struct FixedFile { pub tree_path: &'static str, pub text: &'static str }` and `pub fn fixed_files() -> [FixedFile; 7]`.
- Produces in `mod.rs`: `pub const COMPONENT: &str = "art-firstboot";`, `pub const DISPATCHER_PATH: &str = "S/ART-FirstBoot";`, `pub const STEP_DIR: &str = "S/FirstBoot";`, `pub const REPORT_PATH: &str = "S/FirstBoot.log";`, `pub const FAT_REPORT_NAME: &str = "art-firstboot.log";`, `pub const FLAG_PATH: &str = "Prefs/Env-Archive/ART_FirstBoot";`, `pub const FAIL_AT: i64 = 2_000_000_000;`.

- [ ] **Step 1: Write the module skeleton**

`src-tauri/src/core/firstboot/mod.rs`:

```rust
//! The Amiga finishes what the host cannot start.
//!
//! ART writes a small set of AmigaDOS scripts into a distribution tree. They
//! run **once**, on the machine the card was built for, dispatched from
//! `S:User-Startup` — after the release's own `Startup-Sequence` has run
//! `SetPatch`, `Mount`, `BindDrivers`, `AddDataTypes` and `IPrefs`, which is
//! why the hook is there and not after `BindDrivers` (spec §1.2: both of the
//! owner's real sequences were read; the other project pays for the earlier
//! position with three patches, one of them the ART-193 line).
//!
//! Design: `docs/superpowers/specs/2026-09-07-firstboot-design.md`.
//!
//! ## What is in the tree afterwards
//!
//! ```text
//! S/User-Startup                 ;BEGIN art-firstboot … ;END art-firstboot
//! S/ART-FirstBoot                the dispatcher
//! S/ART-FirstBoot-Step           runs one step and writes its two report lines
//! S/FirstBoot/NN-name            one file per step; deleted when it succeeds
//! S/FirstBoot.log                the report, one line per event
//! Prefs/Env-Archive/ART_FirstBoot  TRUE until the first run begins
//! Storage/DOSDrivers/SD0pi3, SD0pi4   the two mountlists 10-hardware picks from
//! ```
//!
//! Variable and file names carry an `ART_` prefix and no `/` — an AmigaDOS
//! `$name` substitution does not reach into an `ENV:` subdirectory, so the
//! spec's `ENV:ART/…` spelling became `ENV:ART_…` here.
//!
//! ## Layering
//!
//! This module reads `core::osinstall`'s recipe types and calls
//! `core::osinstall::startup::merge_user_startup`. **`core::osinstall` must
//! never import this module** — joining a build to a plan is
//! `commands/firstboot.rs`'s job, the same rule as `core::artwork` and
//! `core::gameindex`.

pub mod plan;
pub mod report;
pub mod scripts;
pub mod write;

/// The `;BEGIN`/`;END` marker id inside `S:User-Startup`.
pub const COMPONENT: &str = "art-firstboot";
/// Tree-relative paths, `/`-separated, as `distribution.json` spells them.
pub const DISPATCHER_PATH: &str = "S/ART-FirstBoot";
pub const STEP_WRAPPER_PATH: &str = "S/ART-FirstBoot-Step";
pub const STEP_DIR: &str = "S/FirstBoot";
pub const REPORT_PATH: &str = "S/FirstBoot.log";
/// The report's name on the FAT boot partition, where Windows can show it.
pub const FAT_REPORT_NAME: &str = "art-firstboot.log";
pub const FLAG_PATH: &str = "Prefs/Env-Archive/ART_FirstBoot";
/// `core::amigainstall::workvol::FAIL_AT`'s reason, verbatim (ART-188).
pub const FAIL_AT: i64 = 2_000_000_000;

/// The four lines merged into `S:User-Startup`. Nothing else ever goes there.
pub fn user_startup_lines() -> Vec<String> {
    vec![
        "IF EXISTS S:ART-FirstBoot".to_string(),
        "  Execute S:ART-FirstBoot".to_string(),
        "ENDIF".to_string(),
    ]
}
```

Add `pub mod firstboot;` to `src-tauri/src/core/mod.rs`. Create empty `plan.rs`, `report.rs`, `write.rs` files containing only `//! (Task N)` so the crate compiles; they are filled by Tasks 3–5.

- [ ] **Step 2: Write the dispatcher script**

`src-tauri/src/core/firstboot/scripts/ART-FirstBoot` (LF endings, ASCII):

```
; ART first boot. Runs every file in S:FirstBoot/ once, in name order, and
; writes one line per event to S:FirstBoot.log. Dispatched from S:User-Startup.
; Written by ART; see docs/superpowers/specs/2026-09-07-firstboot-design.md.
FailAt 2000000000
IF NOT EXISTS S:FirstBoot.log
  Echo >S:FirstBoot.log "art-firstboot 1"
ENDIF

; Which machine. The brcm-* device probes are how Emu68 itself is told apart
; from an emulator; under WinUAE neither device exists.
SetEnv ART_System "UAE"
SetEnv ART_RpiType "none"
Version >NIL: brcm-emmc.device
IF NOT FAIL
  SetEnv ART_System "PiStorm"
  SetEnv ART_RpiType "RPi4"
ELSE
  Version >NIL: brcm-sdhc.device
  IF NOT FAIL
    SetEnv ART_System "PiStorm"
    SetEnv ART_RpiType "RPi3"
  ENDIF
ENDIF

; Which AmigaOS. exec 47 is 3.2; a 3.9 runs on a 3.1 ROM with workbench.library 45 from disk.
SetEnv ART_Kick "3.1"
Version >NIL: exec.library VERSION 47
IF NOT WARN
  SetEnv ART_Kick "3.2"
ELSE
  Version >NIL: workbench.library VERSION 45
  IF NOT WARN
    SetEnv ART_Kick "3.9"
  ENDIF
ENDIF
Echo >>S:FirstBoot.log "system $ART_System $ART_RpiType kick $ART_Kick"

; The banner flag is copied per boot and cleared at once, so a run that stops
; halfway does not present itself as the first boot again.
SetEnv ART_FirstBootBanner "FALSE"
IF EXISTS ENV:ART_FirstBoot
  IF $ART_FirstBoot EQ "TRUE"
    SetEnv ART_FirstBootBanner "TRUE"
    Echo >ENVARC:ART_FirstBoot "FALSE"
    Echo >ENV:ART_FirstBoot "FALSE"
  ENDIF
ENDIF
SetEnv ART_Reboot "FALSE"

; One line per step, generated because AmigaDOS has no loop of its own.
IF EXISTS S:FirstBoot
  List >T:ART-FirstBoot-Run S:FirstBoot PAT=~(#?.info) FILES LFORMAT="Execute S:ART-FirstBoot-Step *"%n*""
  IF EXISTS T:ART-FirstBoot-Run
    Execute T:ART-FirstBoot-Run
    Delete >NIL: T:ART-FirstBoot-Run
  ENDIF
ENDIF

; Ending. The directory decides, not this script's own bookkeeping.
List >NIL: S:FirstBoot PAT=~(#?.info) FILES
IF WARN
  Echo >>S:FirstBoot.log "done all"
  Delete >NIL: S:ART-FirstBoot S:ART-FirstBoot-Step
ELSE
  Echo >>S:FirstBoot.log "done partial"
ENDIF

; The FAT copy is secondary: Windows can show it, the Amiga never waits for it.
Assign >NIL: EXISTS SD0:
IF NOT WARN
  Copy >NIL: S:FirstBoot.log TO SD0:art-firstboot.log
  IF WARN
    Echo >>S:FirstBoot.log "copy-to-fat failed"
  ENDIF
ENDIF
```

Note on `List … FILES` with an empty directory: `List` returns WARN (5) when nothing matches — that is the "empty" test. Pin this behaviour in the rehearsal (Task 8), because it is the one AmigaDOS fact here that a unit test cannot see.

- [ ] **Step 3: Write the step wrapper**

`src-tauri/src/core/firstboot/scripts/ART-FirstBoot-Step`:

```
.KEY name/A
.BRA {
.KET }
; Runs one first-boot step and reports it. A step says no with Quit 20 (or any
; code of 5 and above); a step that decides not to run writes its own
; "skipped <reason>" line first and ends normally.
FailAt 2000000000
Echo >>S:FirstBoot.log "step {name} started"
Execute S:FirstBoot/{name}
IF WARN
  Echo >>S:FirstBoot.log "step {name} refused rc=$RC"
ELSE
  Echo >>S:FirstBoot.log "step {name} ok"
  Delete >NIL: S:FirstBoot/{name}
ENDIF
IF $ART_Reboot EQ "TRUE"
  Echo >>S:FirstBoot.log "reboot requested by {name}"
ENDIF
```

`$RC` is the shell's own return-code variable, set after every command; `IF WARN` tests it against the shell's warn level. A `Quit 20` inside the step is what `Execute` hands back.

- [ ] **Step 4: Write the three fixed steps**

`scripts/10-hardware`:

```
; ART first boot: which Pi, and the FAT boot partition. Needs L:fat95 for the
; mount (Aminet disk/misc/fat95); without it nothing here is attempted.
FailAt 2000000000
IF $ART_System EQ "UAE"
  Echo >>S:FirstBoot.log "step 10-hardware skipped uae"
  Quit 0
ENDIF
IF NOT EXISTS L:fat95
  Echo >>S:FirstBoot.log "step 10-hardware skipped no-fat95"
  Quit 0
ENDIF
IF $ART_RpiType EQ "RPi4"
  Copy >NIL: SYS:Storage/DOSDrivers/SD0pi4 TO DEVS:DOSDrivers/SD0
ELSE
  Copy >NIL: SYS:Storage/DOSDrivers/SD0pi3 TO DEVS:DOSDrivers/SD0
ENDIF
IF NOT EXISTS DEVS:DOSDrivers/SD0
  Echo >>S:FirstBoot.log "step 10-hardware detail sd0-driver-not-copied"
  Quit 20
ENDIF
Delete >NIL: SYS:Storage/DOSDrivers/SD0pi3 QUIET
Delete >NIL: SYS:Storage/DOSDrivers/SD0pi4 QUIET
; The DOSDrivers glob in Startup-Sequence has already run; mount by hand.
Mount >NIL: SD0:
Assign >NIL: EXISTS SD0:
IF WARN
  Echo >>S:FirstBoot.log "step 10-hardware detail sd0-not-mounted"
  Quit 20
ENDIF
Assign >NIL: EMU68BOOT: SD0:
Echo >>S:FirstBoot.log "step 10-hardware detail sd0-mounted $ART_RpiType"
```

`scripts/20-aux`:

```
; ART first boot: AmigaOS 3.9's serial-port DOSDriver is stored as _AUX,
; because AUX is a reserved name on the host that built this tree. A card
; ART wrote already carries it as AUX (core/preload/amiga_names.rs); this
; step is for a tree that reached the Amiga some other way.
FailAt 2000000000
IF $ART_System EQ "UAE"
  Echo >>S:FirstBoot.log "step 20-aux skipped uae"
  Quit 0
ENDIF
IF NOT $ART_Kick EQ "3.9"
  Echo >>S:FirstBoot.log "step 20-aux skipped not-3.9"
  Quit 0
ENDIF
IF NOT EXISTS SYS:Storage/DOSDrivers/_AUX
  Echo >>S:FirstBoot.log "step 20-aux skipped already-aux"
  Quit 0
ENDIF
Rename >NIL: SYS:Storage/DOSDrivers/_AUX SYS:Storage/DOSDrivers/AUX
IF WARN
  Quit 20
ENDIF
IF EXISTS SYS:Storage/DOSDrivers/_AUX.info
  Rename >NIL: SYS:Storage/DOSDrivers/_AUX.info SYS:Storage/DOSDrivers/AUX.info
ENDIF
```

`scripts/30-datatypes`:

```
; ART first boot: apply 68040 patches to the ak* image datatypes when a
; matching -040.pch sits beside each and C:spatch is present. ART ships no
; patches; a user who adds them gets them applied.
FailAt 2000000000
IF NOT EXISTS C:spatch
  Echo >>S:FirstBoot.log "step 30-datatypes skipped no-spatch"
  Quit 0
ENDIF
SetEnv ART_DtPatched "0"
IF EXISTS SYS:Classes/DataTypes/akPNG-040.pch
  IF EXISTS SYS:Classes/DataTypes/akPNG.datatype
    C:spatch >NIL: -oSYS:Classes/DataTypes/akPNG.datatype.new -pSYS:Classes/DataTypes/akPNG-040.pch SYS:Classes/DataTypes/akPNG.datatype
    IF EXISTS SYS:Classes/DataTypes/akPNG.datatype.new
      Delete >NIL: SYS:Classes/DataTypes/akPNG.datatype
      Rename >NIL: SYS:Classes/DataTypes/akPNG.datatype.new SYS:Classes/DataTypes/akPNG.datatype
      SetEnv ART_DtPatched "1"
    ENDIF
  ENDIF
  Delete >NIL: SYS:Classes/DataTypes/akPNG-040.pch
ENDIF
IF EXISTS SYS:Classes/DataTypes/akGIF-040.pch
  IF EXISTS SYS:Classes/DataTypes/akGIF.datatype
    C:spatch >NIL: -oSYS:Classes/DataTypes/akGIF.datatype.new -pSYS:Classes/DataTypes/akGIF-040.pch SYS:Classes/DataTypes/akGIF.datatype
    IF EXISTS SYS:Classes/DataTypes/akGIF.datatype.new
      Delete >NIL: SYS:Classes/DataTypes/akGIF.datatype
      Rename >NIL: SYS:Classes/DataTypes/akGIF.datatype.new SYS:Classes/DataTypes/akGIF.datatype
      SetEnv ART_DtPatched "1"
    ENDIF
  ENDIF
  Delete >NIL: SYS:Classes/DataTypes/akGIF-040.pch
ENDIF
IF EXISTS SYS:Classes/DataTypes/akJFIF-040.pch
  IF EXISTS SYS:Classes/DataTypes/akJFIF.datatype
    C:spatch >NIL: -oSYS:Classes/DataTypes/akJFIF.datatype.new -pSYS:Classes/DataTypes/akJFIF-040.pch SYS:Classes/DataTypes/akJFIF.datatype
    IF EXISTS SYS:Classes/DataTypes/akJFIF.datatype.new
      Delete >NIL: SYS:Classes/DataTypes/akJFIF.datatype
      Rename >NIL: SYS:Classes/DataTypes/akJFIF.datatype.new SYS:Classes/DataTypes/akJFIF.datatype
      SetEnv ART_DtPatched "1"
    ENDIF
  ENDIF
  Delete >NIL: SYS:Classes/DataTypes/akJFIF-040.pch
ENDIF
IF EXISTS SYS:Classes/DataTypes/akTIFF-040.pch
  IF EXISTS SYS:Classes/DataTypes/akTIFF.datatype
    C:spatch >NIL: -oSYS:Classes/DataTypes/akTIFF.datatype.new -pSYS:Classes/DataTypes/akTIFF-040.pch SYS:Classes/DataTypes/akTIFF.datatype
    IF EXISTS SYS:Classes/DataTypes/akTIFF.datatype.new
      Delete >NIL: SYS:Classes/DataTypes/akTIFF.datatype
      Rename >NIL: SYS:Classes/DataTypes/akTIFF.datatype.new SYS:Classes/DataTypes/akTIFF.datatype
      SetEnv ART_DtPatched "1"
    ENDIF
  ENDIF
  Delete >NIL: SYS:Classes/DataTypes/akTIFF-040.pch
ENDIF
IF $ART_DtPatched EQ "0"
  Echo >>S:FirstBoot.log "step 30-datatypes skipped no-patches"
ENDIF
UnSet ART_DtPatched
```

- [ ] **Step 5: Write the two mountlists**

`scripts/SD0pi3` and `scripts/SD0pi4` — copy the text from spec §1.1's table verbatim (`Device = brcm-sdhc.device` / `brcm-emmc.device`, `FileSystem = L:fat95`, `Flags = 0`, `MaxTransfer = 0x1FE00`, `LowCyl = 0`, `HighCyl = 0`, `Surfaces = 1`, `BlocksPerTrack = 1`, `Buffers = 100`, `Stacksize = 4096`, `GlobVec = -1`, `Priority = 5`, `BufMemType = 5`, `UNIT = 0`, `DOSTYPE = 0x46415401`), preceded by one comment line:

```
/* Mountlist for Emu68's FAT boot partition; from emu68hatcher (MIT), rootrootde. */
```

Then add to `THIRD_PARTY_LICENSES.md` an entry beside the existing `media_hashes.json` one (`grep -n emu68hatcher THIRD_PARTY_LICENSES.md` shows where and in what shape): the two mountlists, MIT, `rootrootde/emu68hatcher` commit `3f38b22`.

- [ ] **Step 6: Write the failing exact-match tests**

`src-tauri/src/core/firstboot/scripts.rs`:

```rust
//! The fixed AmigaDOS text, compiled in and pinned.
//!
//! These files never vary per build. They are checked in as files rather
//! than string literals so a reader sees AmigaDOS, not Rust escapes, and
//! they are pinned exact-match below so a one-line edit cannot land without
//! its test changing with it (`core::amigainstall::workvol`'s precedent).

/// One file ART writes into the tree unchanged.
#[derive(Debug, Clone, Copy)]
pub struct FixedFile {
    /// Tree-relative, `/`-separated.
    pub tree_path: &'static str,
    pub text: &'static str,
}

pub const DISPATCHER: &str = include_str!("scripts/ART-FirstBoot");
pub const STEP_WRAPPER: &str = include_str!("scripts/ART-FirstBoot-Step");
pub const STEP_10_HARDWARE: &str = include_str!("scripts/10-hardware");
pub const STEP_20_AUX: &str = include_str!("scripts/20-aux");
pub const STEP_30_DATATYPES: &str = include_str!("scripts/30-datatypes");
pub const SD0_PI3: &str = include_str!("scripts/SD0pi3");
pub const SD0_PI4: &str = include_str!("scripts/SD0pi4");

/// Every fixed file, at the path it takes in the tree.
pub fn fixed_files() -> [FixedFile; 7] {
    [
        FixedFile { tree_path: super::DISPATCHER_PATH, text: DISPATCHER },
        FixedFile { tree_path: super::STEP_WRAPPER_PATH, text: STEP_WRAPPER },
        FixedFile { tree_path: "S/FirstBoot/10-hardware", text: STEP_10_HARDWARE },
        FixedFile { tree_path: "S/FirstBoot/20-aux", text: STEP_20_AUX },
        FixedFile { tree_path: "S/FirstBoot/30-datatypes", text: STEP_30_DATATYPES },
        FixedFile { tree_path: "Storage/DOSDrivers/SD0pi3", text: SD0_PI3 },
        FixedFile { tree_path: "Storage/DOSDrivers/SD0pi4", text: SD0_PI4 },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every script is ASCII with LF endings — an AmigaDOS shell reads a CR
    /// as part of the line, and a stray high byte is the control-byte class.
    #[test]
    fn every_fixed_file_is_ascii_with_lf_endings() {
        for file in fixed_files() {
            assert!(file.text.is_ascii(), "{} is not ASCII", file.tree_path);
            assert!(!file.text.contains('\r'), "{} carries a CR", file.tree_path);
            assert!(file.text.ends_with('\n'), "{} lacks a final newline", file.tree_path);
        }
    }

    #[test]
    fn dispatcher_sets_the_measured_fail_level_first() {
        let first_command = DISPATCHER
            .lines()
            .find(|l| !l.starts_with(';') && !l.trim().is_empty())
            .unwrap();
        assert_eq!(first_command, format!("FailAt {}", super::super::FAIL_AT));
    }

    #[test]
    fn dispatcher_writes_the_format_version_and_both_endings() {
        assert!(DISPATCHER.contains("\"art-firstboot 1\""));
        assert!(DISPATCHER.contains("\"done all\""));
        assert!(DISPATCHER.contains("\"done partial\""));
        assert!(DISPATCHER.contains("\"copy-to-fat failed\""));
    }

    #[test]
    fn dispatcher_clears_the_flag_before_running_any_step() {
        let clear = DISPATCHER.find("Echo >ENVARC:ART_FirstBoot \"FALSE\"").unwrap();
        let run = DISPATCHER.find("Execute T:ART-FirstBoot-Run").unwrap();
        assert!(clear < run);
    }

    #[test]
    fn step_wrapper_reports_started_before_it_executes_and_deletes_only_on_ok() {
        let started = STEP_WRAPPER.find("started").unwrap();
        let execute = STEP_WRAPPER.find("Execute S:FirstBoot/{name}").unwrap();
        let delete = STEP_WRAPPER.find("Delete >NIL: S:FirstBoot/{name}").unwrap();
        let else_branch = STEP_WRAPPER.find("ELSE").unwrap();
        assert!(started < execute);
        assert!(else_branch < delete, "the delete must sit in the ok branch");
        assert!(STEP_WRAPPER.contains("refused rc=$RC"));
    }

    #[test]
    fn hardware_step_skips_under_uae_and_without_fat95_before_touching_anything() {
        let uae = STEP_10_HARDWARE.find("skipped uae").unwrap();
        let fat95 = STEP_10_HARDWARE.find("skipped no-fat95").unwrap();
        let copy = STEP_10_HARDWARE.find("Copy >NIL:").unwrap();
        assert!(uae < fat95 && fat95 < copy);
        assert!(STEP_10_HARDWARE.contains("SD0pi4 TO DEVS:DOSDrivers/SD0"));
        assert!(STEP_10_HARDWARE.contains("SD0pi3 TO DEVS:DOSDrivers/SD0"));
        assert!(STEP_10_HARDWARE.contains("Mount >NIL: SD0:"));
        assert!(STEP_10_HARDWARE.contains("Assign >NIL: EMU68BOOT: SD0:"));
    }

    #[test]
    fn aux_step_only_acts_on_a_3_9_tree_on_real_hardware() {
        assert!(STEP_20_AUX.contains("skipped uae"));
        assert!(STEP_20_AUX.contains("IF NOT $ART_Kick EQ \"3.9\""));
        assert!(STEP_20_AUX.contains("skipped not-3.9"));
        assert!(STEP_20_AUX.contains("skipped already-aux"));
    }

    #[test]
    fn datatypes_step_names_all_four_and_skips_when_nothing_to_patch() {
        for name in ["akPNG", "akGIF", "akJFIF", "akTIFF"] {
            assert!(STEP_30_DATATYPES.contains(&format!("{name}-040.pch")), "{name}");
        }
        assert!(STEP_30_DATATYPES.contains("skipped no-spatch"));
        assert!(STEP_30_DATATYPES.contains("skipped no-patches"));
    }

    #[test]
    fn mountlists_differ_only_in_the_device_and_need_fat95() {
        assert!(SD0_PI3.contains("Device          = brcm-sdhc.device"));
        assert!(SD0_PI4.contains("Device          = brcm-emmc.device"));
        assert_eq!(
            SD0_PI3.replace("brcm-sdhc.device", "X"),
            SD0_PI4.replace("brcm-emmc.device", "X")
        );
        assert!(SD0_PI3.contains("FileSystem      = L:fat95"));
        assert!(SD0_PI3.contains("DOSTYPE         = 0x46415401"));
    }

    /// Pin every byte. A script edit must come with this test's expectation
    /// changing, which is the review point.
    #[test]
    fn every_fixed_file_hash_is_pinned() {
        use sha2::{Digest, Sha256};
        let mut got = Vec::new();
        for file in fixed_files() {
            let hash = Sha256::digest(file.text.as_bytes());
            got.push(format!("{} {:x}", file.tree_path, hash));
        }
        // Fill in from the first run's output, then never change without a review.
        let expected: [&str; 7] = [
            "S/ART-FirstBoot <sha256>",
            "S/ART-FirstBoot-Step <sha256>",
            "S/FirstBoot/10-hardware <sha256>",
            "S/FirstBoot/20-aux <sha256>",
            "S/FirstBoot/30-datatypes <sha256>",
            "Storage/DOSDrivers/SD0pi3 <sha256>",
            "Storage/DOSDrivers/SD0pi4 <sha256>",
        ];
        assert_eq!(got, expected, "a fixed script changed; review the diff and re-pin");
    }
}
```

`<sha256>` is filled in Step 8 from the first run — the plan cannot know it before the files exist. This is the one place a placeholder is permitted, and it is gone before the commit.

- [ ] **Step 7: Run, watch the hash test fail**

```bash
cd src-tauri && cargo test --lib firstboot::scripts 2>&1 | tail -30
```

Expected: every test passes except `every_fixed_file_hash_is_pinned`, which prints the seven `got` lines.

- [ ] **Step 8: Pin the hashes, run again**

Paste the seven printed lines into `expected`. Run again: `test result: ok. 10 passed`.

Mutation check (record the result in the commit message body): change `Quit 20` to `Quit 0` in `10-hardware`, run — `every_fixed_file_hash_is_pinned` must fall. Restore the file **with the Write tool, not `shutil.move`** (CLAUDE.md's mtime trap), run again, green.

- [ ] **Step 9: fmt, clippy, commit**

```bash
cd src-tauri && cargo fmt && cargo clippy --all-targets -- -D warnings 2>&1 | tail -3
cd .. && python scripts/control-byte-sweep.py | tail -1
git add src-tauri/src/core/mod.rs src-tauri/src/core/firstboot THIRD_PARTY_LICENSES.md
git commit -F <message file>   # "Add the first-boot scripts, compiled in and pinned (task 2)"
```

---

### Task 3: The report parser

**Files:**
- Create: `src-tauri/src/core/firstboot/report.rs`

**Interfaces:**
- Produces:
  ```rust
  pub struct FirstBootReport { pub version: Option<u32>, pub system: Option<Detected>, pub steps: Vec<StepReport>, pub ending: Ending, pub fat_copy_failed: bool, pub reboot_requested_by: Option<String>, pub unknown: Vec<String> }
  pub struct Detected { pub system: String, pub rpi: String, pub kick: String }
  pub struct StepReport { pub name: String, pub outcome: StepOutcome, pub details: Vec<String> }
  pub enum StepOutcome { Ok, Skipped { reason: String }, Refused { rc: i64 }, Unfinished }
  pub enum Ending { NotBooted, Unfinished, DoneAll, DonePartial }
  pub fn parse(text: &str) -> FirstBootReport
  pub fn parse_bytes(bytes: &[u8]) -> FirstBootReport   // Latin-1, never lossy UTF-8
  ```
  All `Serialize` with `#[serde(rename_all = "camelCase")]`; the enums `#[serde(tag = "kind", rename_all = "kebab-case")]` **and** struct-variant fields renamed explicitly (`#[serde(rename = "rc")]` is a no-op but write the camelCase ones out — `rename_all` does not cascade into variant fields, the `PackagePanel` trap).

- [ ] **Step 1: Write the failing tests**

Append to `report.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    const FULL: &str = "art-firstboot 1\nsystem PiStorm RPi4 kick 3.2\nstep 10-hardware started\nstep 10-hardware detail sd0-mounted RPi4\nstep 10-hardware ok\nstep 20-aux started\nstep 20-aux skipped not-3.9\nstep 20-aux ok\nstep 50-pkg-boingbag-39-1 started\nstep 50-pkg-boingbag-39-1 refused rc=20\nstep 90-prefs started\nstep 90-prefs ok\nreboot requested by 90-prefs\ndone partial\n";

    #[test]
    fn a_full_report_keeps_every_ending_distinct() {
        let r = parse(FULL);
        assert_eq!(r.version, Some(1));
        let d = r.system.as_ref().unwrap();
        assert_eq!((d.system.as_str(), d.rpi.as_str(), d.kick.as_str()), ("PiStorm", "RPi4", "3.2"));
        assert_eq!(r.steps.len(), 4);
        assert_eq!(r.steps[0].outcome, StepOutcome::Ok);
        assert_eq!(r.steps[0].details, vec!["sd0-mounted RPi4".to_string()]);
        assert_eq!(r.steps[1].outcome, StepOutcome::Skipped { reason: "not-3.9".into() });
        assert_eq!(r.steps[2].outcome, StepOutcome::Refused { rc: 20 });
        assert_eq!(r.steps[3].outcome, StepOutcome::Ok);
        assert_eq!(r.reboot_requested_by.as_deref(), Some("90-prefs"));
        assert_eq!(r.ending, Ending::DonePartial);
        assert!(r.unknown.is_empty());
    }

    #[test]
    fn a_step_that_started_and_never_finished_is_unfinished_and_so_is_the_report() {
        let r = parse("art-firstboot 1\nsystem UAE none kick 3.2\nstep 10-hardware started\n");
        assert_eq!(r.steps[0].outcome, StepOutcome::Unfinished);
        assert_eq!(r.ending, Ending::Unfinished);
    }

    #[test]
    fn an_empty_file_is_not_booted_and_a_missing_done_line_is_unfinished() {
        assert_eq!(parse("").ending, Ending::NotBooted);
        assert_eq!(parse("art-firstboot 1\n").ending, Ending::Unfinished);
        assert_eq!(parse("art-firstboot 1\ndone all\n").ending, Ending::DoneAll);
    }

    #[test]
    fn a_skipped_line_wins_over_the_ok_that_follows_it() {
        let r = parse("art-firstboot 1\nstep 20-aux started\nstep 20-aux skipped uae\nstep 20-aux ok\ndone all\n");
        assert_eq!(r.steps[0].outcome, StepOutcome::Skipped { reason: "uae".into() });
    }

    #[test]
    fn a_line_from_a_newer_art_is_carried_not_dropped() {
        let r = parse("art-firstboot 2\nfrobnicate 3\nstep 10-hardware started\nstep 10-hardware ok\ndone all\n");
        assert_eq!(r.version, Some(2));
        assert_eq!(r.unknown, vec!["frobnicate 3".to_string()]);
        assert_eq!(r.steps[0].outcome, StepOutcome::Ok);
    }

    #[test]
    fn the_fat_copy_failure_is_a_flag_not_a_step() {
        let r = parse("art-firstboot 1\ndone all\ncopy-to-fat failed\n");
        assert!(r.fat_copy_failed);
        assert!(r.steps.is_empty());
        assert_eq!(r.ending, Ending::DoneAll);
    }

    #[test]
    fn crlf_and_latin1_are_read_as_the_amiga_wrote_them() {
        let bytes = b"art-firstboot 1\r\nstep 50-pkg-t\xFCrk started\r\nstep 50-pkg-t\xFCrk ok\r\ndone all\r\n";
        let r = parse_bytes(bytes);
        assert_eq!(r.steps[0].name, "t\u{fc}rk");
        assert_eq!(r.ending, Ending::DoneAll);
    }

    #[test]
    fn a_refused_step_with_a_bad_rc_is_still_refused() {
        let r = parse("art-firstboot 1\nstep x started\nstep x refused rc=banana\ndone partial\n");
        assert_eq!(r.steps[0].outcome, StepOutcome::Refused { rc: -1 });
    }

    #[test]
    fn the_wire_shape_is_what_the_frontend_reads() {
        let r = parse(FULL);
        let json = serde_json::to_value(&r).unwrap();
        assert_eq!(json["ending"], "done-partial");
        assert_eq!(json["steps"][1]["outcome"]["kind"], "skipped");
        assert_eq!(json["steps"][1]["outcome"]["reason"], "not-3.9");
        assert_eq!(json["steps"][2]["outcome"]["rc"], 20);
        assert_eq!(json["rebootRequestedBy"], "90-prefs");
        assert_eq!(json["fatCopyFailed"], false);
    }
}
```

- [ ] **Step 2: Run, expect compile failure** — `cargo test --lib firstboot::report` fails with unresolved names.

- [ ] **Step 3: Implement**

```rust
//! Reading `S:FirstBoot.log` — what the Amiga said happened.
//!
//! One line per event, appended by the dispatcher and the step wrapper
//! (`scripts/`). The parser is lenient on purpose: a line it does not know is
//! **carried** in `unknown`, never dropped, because a card written by a newer
//! ART must still be readable by this one. Endings stay distinct (spec §4.3):
//! no text at all is "not booted", text without `done` is "unfinished", and
//! `done all` / `done partial` are what the directory said.

use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FirstBootReport {
    pub version: Option<u32>,
    pub system: Option<Detected>,
    pub steps: Vec<StepReport>,
    pub ending: Ending,
    pub fat_copy_failed: bool,
    pub reboot_requested_by: Option<String>,
    pub unknown: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Detected {
    pub system: String,
    pub rpi: String,
    pub kick: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StepReport {
    pub name: String,
    pub outcome: StepOutcome,
    pub details: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum StepOutcome {
    Ok,
    Skipped { reason: String },
    Refused { rc: i64 },
    /// `started` with nothing after it: the machine stopped or hung mid-step.
    Unfinished,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Ending {
    /// No text at all — the card has not run the block yet.
    NotBooted,
    /// Text, but no `done` line.
    Unfinished,
    DoneAll,
    DonePartial,
}

/// The Amiga writes Latin-1. Decoding as UTF-8 would turn `türkçe` into
/// replacement characters — ART-168's class, on the way back in.
pub fn parse_bytes(bytes: &[u8]) -> FirstBootReport {
    let text: String = bytes.iter().map(|&b| b as char).collect();
    parse(&text)
}

pub fn parse(text: &str) -> FirstBootReport {
    let mut report = FirstBootReport {
        version: None,
        system: None,
        steps: Vec::new(),
        ending: Ending::NotBooted,
        fat_copy_failed: false,
        reboot_requested_by: None,
        unknown: Vec::new(),
    };
    let mut saw_text = false;
    let mut done: Option<Ending> = None;

    for raw in text.lines() {
        let line = raw.trim_end_matches('\r').trim();
        if line.is_empty() {
            continue;
        }
        saw_text = true;
        let words: Vec<&str> = line.splitn(4, ' ').collect();
        match words.as_slice() {
            ["art-firstboot", v] => report.version = v.parse().ok(),
            ["system", system, rpi, rest] => {
                let kick = rest.strip_prefix("kick ").unwrap_or(rest).to_string();
                report.system = Some(Detected {
                    system: system.to_string(),
                    rpi: rpi.to_string(),
                    kick,
                });
            }
            ["step", name, "started"] => {
                report.steps.push(StepReport {
                    name: name.to_string(),
                    outcome: StepOutcome::Unfinished,
                    details: Vec::new(),
                });
            }
            ["step", name, "ok"] => {
                if let Some(step) = step_named(&mut report, name) {
                    if step.outcome == StepOutcome::Unfinished {
                        step.outcome = StepOutcome::Ok;
                    }
                }
            }
            ["step", name, "skipped", reason] => {
                if let Some(step) = step_named(&mut report, name) {
                    step.outcome = StepOutcome::Skipped {
                        reason: reason.to_string(),
                    };
                }
            }
            ["step", name, "refused", rc] => {
                let rc = rc.strip_prefix("rc=").and_then(|n| n.parse().ok()).unwrap_or(-1);
                if let Some(step) = step_named(&mut report, name) {
                    step.outcome = StepOutcome::Refused { rc };
                }
            }
            ["step", name, "detail", detail] => {
                if let Some(step) = step_named(&mut report, name) {
                    step.details.push(detail.to_string());
                }
            }
            ["reboot", "requested", "by", name] => {
                report.reboot_requested_by = Some(name.to_string());
            }
            ["done", "all"] => done = Some(Ending::DoneAll),
            ["done", "partial"] => done = Some(Ending::DonePartial),
            ["copy-to-fat", "failed"] => report.fat_copy_failed = true,
            _ => report.unknown.push(line.to_string()),
        }
    }

    report.ending = match (saw_text, done) {
        (false, _) => Ending::NotBooted,
        (true, None) => Ending::Unfinished,
        (true, Some(ending)) => ending,
    };
    report
}

/// The most recent step of that name — a step retried on a later boot
/// appears twice, and the later lines belong to the later attempt.
fn step_named<'a>(report: &'a mut FirstBootReport, name: &str) -> Option<&'a mut StepReport> {
    report.steps.iter_mut().rev().find(|s| s.name == name)
}
```

A `step … ok`/`skipped`/`refused` line with no `started` before it (a hand-edited file) lands in `unknown` — that is the `None` branch of `step_named`, and it needs a test: add `fn an_outcome_without_a_start_is_unknown()` asserting `parse("art-firstboot 1\nstep x ok\ndone all\n").unknown == ["step x ok"]`. Implement by pushing to `unknown` in each `None` case.

- [ ] **Step 4: Run** — `cargo test --lib firstboot::report`: `test result: ok. 10 passed`.

- [ ] **Step 5: Mutation** — make `["step", name, "ok"]` set `Ok` unconditionally; `a_skipped_line_wins_over_the_ok_that_follows_it` must fall. Restore.

- [ ] **Step 6: fmt, clippy, commit** — `Parse the first-boot report, every ending distinct (task 3)`.

---

### Task 4: The plan, and its refusals

**Files:**
- Create: `src-tauri/src/core/firstboot/plan.rs`
- Modify: `src-tauri/src/core/error.rs` (variants + `code()` arms)

**Interfaces:**
- Consumes: `scripts::fixed_files()`, `merge_user_startup`.
- Produces:
  ```rust
  pub struct FirstBootRequest { pub tree: PathBuf }
  pub enum FatMount { Available, Unavailable { needs: &'static str } }   // needs = "fat95"
  pub struct PlannedStep { pub name: String, pub tree_path: String, pub fixed: bool }
  pub struct FirstBootPlan { pub tree: PathBuf, pub steps: Vec<PlannedStep>, pub fat_mount: FatMount, pub user_startup_exists: bool, pub already_written: bool, pub bytes_added: u64 }
  pub fn plan(request: &FirstBootRequest) -> CoreResult<FirstBootPlan>
  ```
  `CoreError::FirstBootHookUnreachable { file: PathBuf }` → code `ART-FIRSTBOOT-HOOK-UNREACHABLE`; `CoreError::FirstBootNotATree { tree: PathBuf }` → `ART-FIRSTBOOT-NOT-A-TREE`. (Spec §7's `NeedsDisc` and `NeedsArchiver` are phase 4's; do **not** add them now.)

- [ ] **Step 1: Failing tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::ScratchDir;
    use std::fs;

    /// The smallest thing `plan` accepts: a distribution.json and a
    /// Startup-Sequence that calls User-Startup, the way both real ones do.
    fn tree(tag: &str) -> ScratchDir {
        let d = ScratchDir::new("art-firstboot-plan", tag);
        fs::create_dir_all(d.join("S")).unwrap();
        fs::create_dir_all(d.join("L")).unwrap();
        fs::write(d.join("distribution.json"), b"{}").unwrap();
        fs::write(
            d.join("S/Startup-Sequence"),
            b"C:SetPatch QUIET\nBindDrivers\nIF EXISTS S:User-Startup\n  Execute S:User-Startup\nENDIF\nC:LoadWB\nEndCLI >NIL:\n",
        )
        .unwrap();
        d
    }

    #[test]
    fn a_plain_tree_plans_three_fixed_steps_and_no_fat_mount() {
        let d = tree("plain");
        let p = plan(&FirstBootRequest { tree: d.path().to_path_buf() }).unwrap();
        let names: Vec<&str> = p.steps.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, ["10-hardware", "20-aux", "30-datatypes"]);
        assert_eq!(p.fat_mount, FatMount::Unavailable { needs: "fat95" });
        assert!(!p.user_startup_exists);
        assert!(!p.already_written);
        assert!(p.bytes_added > 0);
    }

    #[test]
    fn fat95_in_l_makes_the_fat_mount_available() {
        let d = tree("fat95");
        fs::write(d.join("L/fat95"), b"x").unwrap();
        let p = plan(&FirstBootRequest { tree: d.path().to_path_buf() }).unwrap();
        assert_eq!(p.fat_mount, FatMount::Available);
    }

    #[test]
    fn a_startup_sequence_that_never_calls_user_startup_is_refused_by_name() {
        let d = tree("nohook");
        fs::write(d.join("S/Startup-Sequence"), b"C:SetPatch QUIET\nC:LoadWB\nEndCLI >NIL:\n").unwrap();
        let err = plan(&FirstBootRequest { tree: d.path().to_path_buf() }).unwrap_err();
        match err {
            CoreError::FirstBootHookUnreachable { file } => {
                assert!(file.ends_with("Startup-Sequence"));
            }
            other => panic!("wrong refusal: {other:?}"),
        }
    }

    #[test]
    fn the_hook_check_is_case_insensitive_like_amigados() {
        let d = tree("case");
        fs::write(d.join("S/Startup-Sequence"), b"if exists s:user-startup\n  execute s:user-startup\nendif\n").unwrap();
        assert!(plan(&FirstBootRequest { tree: d.path().to_path_buf() }).is_ok());
    }

    #[test]
    fn a_folder_without_distribution_json_is_not_a_tree() {
        let d = ScratchDir::new("art-firstboot-plan", "notree");
        let err = plan(&FirstBootRequest { tree: d.path().to_path_buf() }).unwrap_err();
        assert!(matches!(err, CoreError::FirstBootNotATree { .. }));
    }

    #[test]
    fn an_existing_dispatcher_marks_the_plan_as_already_written() {
        let d = tree("again");
        fs::write(d.join("S/ART-FirstBoot"), b"x").unwrap();
        let p = plan(&FirstBootRequest { tree: d.path().to_path_buf() }).unwrap();
        assert!(p.already_written);
    }
}
```

- [ ] **Step 2: Run, expect compile failure.**

- [ ] **Step 3: Implement**

In `error.rs`, beside `EscapedNamesNeedNativeCopy`:

```rust
    /// The tree's own `S/Startup-Sequence` never runs `S:User-Startup`, so the
    /// block ART would merge there could not execute. Writing it and saying
    /// "installed" would be a confident wrong sentence (spec §7).
    #[error(
        "{} never runs S:User-Startup, so a first-boot block placed there would never execute. \
         Both AmigaOS 3.2 and 3.9 ship a sequence that does; restore that line or use the \
         release's own Startup-Sequence.",
        file.display()
    )]
    FirstBootHookUnreachable { file: PathBuf },

    /// No `distribution.json` — this is not a tree ART built.
    #[error(
        "{} is not a distribution tree ART built (no distribution.json), so ART cannot say what a \
         first boot would run against.",
        tree.display()
    )]
    FirstBootNotATree { tree: PathBuf },
```

and in `code()`: `Self::FirstBootHookUnreachable { .. } => "ART-FIRSTBOOT-HOOK-UNREACHABLE"`, `Self::FirstBootNotATree { .. } => "ART-FIRSTBOOT-NOT-A-TREE"`. Check `error.rs` for an existing test that enumerates codes for uniqueness (`grep -n "codes_are_unique\|every_code" core/error.rs`) and extend it if it lists variants by hand.

`plan.rs`:

```rust
//! Deciding what first boot will run — every refusal here, before a byte.

use std::path::{Path, PathBuf};

use serde::Serialize;

use super::scripts::fixed_files;
use crate::core::error::{CoreError, CoreResult};

#[derive(Debug, Clone)]
pub struct FirstBootRequest {
    pub tree: PathBuf,
}

/// Whether `10-hardware` can mount the FAT boot partition.
///
/// Not a refusal: a build that never needed `EMU68BOOT:` must not be blocked
/// by a handler it does not have. The step skips, and the preview names the
/// package that would change that.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum FatMount {
    Available,
    Unavailable { needs: &'static str },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlannedStep {
    /// `10-hardware` — the file name in `S:FirstBoot/`, which is also the
    /// name the report uses.
    pub name: String,
    pub tree_path: String,
    /// `true` for text compiled into ART; generated steps arrive in phase 3.
    pub fixed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FirstBootPlan {
    pub tree: PathBuf,
    pub steps: Vec<PlannedStep>,
    pub fat_mount: FatMount,
    /// `S/User-Startup` is there already — the merge will edit it, and the
    /// command layer backs it up first.
    pub user_startup_exists: bool,
    /// `S/ART-FirstBoot` is already in the tree: writing again is a re-install.
    pub already_written: bool,
    /// What the fixed files add to the tree, for the card step's arithmetic.
    pub bytes_added: u64,
}

pub fn plan(request: &FirstBootRequest) -> CoreResult<FirstBootPlan> {
    let tree = &request.tree;
    if !tree.join("distribution.json").is_file() {
        return Err(CoreError::FirstBootNotATree { tree: tree.clone() });
    }
    let sequence = tree.join("S").join("Startup-Sequence");
    if !calls_user_startup(&sequence)? {
        return Err(CoreError::FirstBootHookUnreachable { file: sequence });
    }

    let fat_mount = if tree.join("L").join("fat95").is_file() {
        FatMount::Available
    } else {
        FatMount::Unavailable { needs: "fat95" }
    };

    let mut steps = Vec::new();
    let mut bytes_added = 0u64;
    for file in fixed_files() {
        bytes_added += file.text.len() as u64;
        if let Some(name) = file.tree_path.strip_prefix("S/FirstBoot/") {
            steps.push(PlannedStep {
                name: name.to_string(),
                tree_path: file.tree_path.to_string(),
                fixed: true,
            });
        }
    }
    bytes_added += b"TRUE\n".len() as u64;

    Ok(FirstBootPlan {
        tree: tree.clone(),
        steps,
        fat_mount,
        user_startup_exists: tree.join("S").join("User-Startup").is_file(),
        already_written: tree.join(super::DISPATCHER_PATH).is_file(),
        bytes_added,
    })
}

/// AmigaDOS is case-insensitive and so is this check. A missing file is a
/// tree with no sequence at all, which is the same refusal.
fn calls_user_startup(sequence: &Path) -> CoreResult<bool> {
    let bytes = match std::fs::read(sequence) {
        Ok(bytes) => bytes,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(err) => return Err(err.into()),
    };
    let text = bytes.to_ascii_lowercase();
    Ok(text
        .windows(b"execute s:user-startup".len())
        .any(|w| w == b"execute s:user-startup"))
}
```

`tree.join(super::DISPATCHER_PATH)` with a `/`-separated constant works on Windows because `Path::join` accepts it; keep the constant `/`-separated so `distribution.json` and the report agree.

- [ ] **Step 4: Run** — `test result: ok. 6 passed`.

- [ ] **Step 5: Mutation** — make `calls_user_startup` return `Ok(true)` unconditionally; the refusal test must fall with "wrong refusal" or an `unwrap_err` panic. Restore.

- [ ] **Step 6: fmt, clippy, commit** — `Plan a first boot and refuse before writing (task 4)`.

---

### Task 5: Writing the plan into the tree

**Files:**
- Create: `src-tauri/src/core/firstboot/write.rs`

**Interfaces:**
- Consumes: `FirstBootPlan`, `scripts::fixed_files()`, `merge_user_startup`, `core::safety::{atomic_write, guarded_write, BackupPolicy}`.
- Produces:
  ```rust
  pub struct Written { pub files: Vec<String>, pub user_startup_backup: Option<PathBuf>, pub user_startup_created: bool }
  pub fn write(plan: &FirstBootPlan) -> CoreResult<Written>
  ```

- [ ] **Step 1: Failing tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::firstboot::plan::{plan, FirstBootRequest};
    use crate::core::ScratchDir;
    use std::fs;

    fn tree(tag: &str) -> ScratchDir {
        let d = ScratchDir::new("art-firstboot-write", tag);
        fs::create_dir_all(d.join("S")).unwrap();
        fs::write(d.join("distribution.json"), b"{}").unwrap();
        fs::write(d.join("S/Startup-Sequence"), b"IF EXISTS S:User-Startup\n  Execute S:User-Startup\nENDIF\n").unwrap();
        d
    }

    fn snapshot(root: &std::path::Path) -> std::collections::BTreeMap<String, Vec<u8>> {
        fn walk(dir: &std::path::Path, root: &std::path::Path, out: &mut std::collections::BTreeMap<String, Vec<u8>>) {
            for entry in fs::read_dir(dir).unwrap() {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    walk(&path, root, out);
                } else {
                    let rel = path.strip_prefix(root).unwrap().to_string_lossy().replace('\\', "/");
                    out.insert(rel, fs::read(&path).unwrap());
                }
            }
        }
        let mut out = std::collections::BTreeMap::new();
        walk(root, root, &mut out);
        out
    }

    #[test]
    fn writes_every_fixed_file_the_flag_and_the_block() {
        let d = tree("all");
        let p = plan(&FirstBootRequest { tree: d.path().to_path_buf() }).unwrap();
        let w = write(&p).unwrap();
        for f in crate::core::firstboot::scripts::fixed_files() {
            assert_eq!(fs::read_to_string(d.join(f.tree_path)).unwrap(), f.text, "{}", f.tree_path);
            assert!(!d.join(format!("{}.uaem", f.tree_path)).exists(), "no sidecar for default protection");
        }
        assert_eq!(fs::read_to_string(d.join("Prefs/Env-Archive/ART_FirstBoot")).unwrap(), "TRUE\n");
        let us = fs::read_to_string(d.join("S/User-Startup")).unwrap();
        assert_eq!(us, ";BEGIN art-firstboot\nIF EXISTS S:ART-FirstBoot\n  Execute S:ART-FirstBoot\nENDIF\n;END art-firstboot\n");
        assert!(w.user_startup_created);
        assert!(w.user_startup_backup.is_none());
        assert_eq!(w.files.len(), 9);
    }

    #[test]
    fn writing_twice_is_byte_identical() {
        let d = tree("twice");
        let p = plan(&FirstBootRequest { tree: d.path().to_path_buf() }).unwrap();
        write(&p).unwrap();
        let first = snapshot(d.path());
        let p2 = plan(&FirstBootRequest { tree: d.path().to_path_buf() }).unwrap();
        assert!(p2.already_written);
        write(&p2).unwrap();
        let second = snapshot(d.path());
        // A backup generation of User-Startup is the one permitted difference.
        let strip = |m: std::collections::BTreeMap<String, Vec<u8>>| {
            m.into_iter().filter(|(k, _)| !k.contains("User-Startup.")).collect::<Vec<_>>()
        };
        assert_eq!(strip(first), strip(second));
    }

    #[test]
    fn the_users_own_user_startup_lines_survive_byte_for_byte() {
        let d = tree("keep");
        let theirs = "; my own line\r\nAssign FONTS: Work:Fonts ADD\r\n;BEGIN other-tool\r\nRun Other\r\n;END other-tool\r\n";
        fs::write(d.join("S/User-Startup"), theirs).unwrap();
        let p = plan(&FirstBootRequest { tree: d.path().to_path_buf() }).unwrap();
        let w = write(&p).unwrap();
        let after = fs::read_to_string(d.join("S/User-Startup")).unwrap();
        assert!(after.starts_with(theirs), "everything before the block is untouched");
        assert!(after.ends_with(";END art-firstboot\n"));
        assert!(!w.user_startup_created);
        let backup = w.user_startup_backup.expect("an existing User-Startup is backed up first");
        assert_eq!(fs::read_to_string(backup).unwrap(), theirs);
    }

    #[test]
    fn a_failed_write_leaves_the_tree_without_a_half_written_block() {
        let d = tree("fail");
        // Make S/FirstBoot a *file* so the step directory cannot be created.
        fs::write(d.join("S/FirstBoot"), b"in the way").unwrap();
        let p = plan(&FirstBootRequest { tree: d.path().to_path_buf() }).unwrap();
        assert!(write(&p).is_err());
        assert!(!d.join("S/User-Startup").exists(), "the block is the last thing written, so nothing points at a dispatcher that is not there");
    }
}
```

- [ ] **Step 2: Run, expect compile failure.**

- [ ] **Step 3: Implement**

```rust
//! Putting a plan into a tree.
//!
//! Order matters and is the whole design of this file: every file the block
//! points at is written **before** the block, so a write that stops halfway
//! leaves a tree whose `User-Startup` does not name a dispatcher that is not
//! there. The block itself goes through `merge_user_startup` and a
//! `BackupPolicy::CONFIG` guarded write, because `User-Startup` is a file a
//! person edits by hand (§39/§40).
//!
//! No `.uaem` sidecars: every file here has default protection and no
//! comment, and `core/osinstall/apply.rs::settle_sidecar` writes a sidecar
//! only when there is something to say.

use std::path::PathBuf;

use serde::Serialize;

use super::plan::FirstBootPlan;
use super::scripts::fixed_files;
use super::{user_startup_lines, COMPONENT, FLAG_PATH};
use crate::core::error::CoreResult;
use crate::core::osinstall::startup::merge_user_startup;
use crate::core::safety::{atomic_write, guarded_write, BackupPolicy};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Written {
    /// Tree-relative paths, in the order they were written; the block last.
    pub files: Vec<String>,
    pub user_startup_backup: Option<PathBuf>,
    pub user_startup_created: bool,
}

pub fn write(plan: &FirstBootPlan) -> CoreResult<Written> {
    let tree = &plan.tree;
    let mut files = Vec::new();

    for file in fixed_files() {
        let path = tree.join(file.tree_path);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        atomic_write(&path, file.text.as_bytes())?;
        files.push(file.tree_path.to_string());
    }

    let flag = tree.join(FLAG_PATH);
    if let Some(parent) = flag.parent() {
        std::fs::create_dir_all(parent)?;
    }
    atomic_write(&flag, b"TRUE\n")?;
    files.push(FLAG_PATH.to_string());

    let user_startup = tree.join("S").join("User-Startup");
    let existing = match std::fs::read(&user_startup) {
        Ok(bytes) => Some(bytes.iter().map(|&b| b as char).collect::<String>()),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => None,
        Err(err) => return Err(err.into()),
    };
    let merged = merge_user_startup(existing.as_deref(), COMPONENT, &user_startup_lines());
    let merged_bytes: Vec<u8> = merged.chars().map(|c| c as u32 as u8).collect();
    let backup = guarded_write(&user_startup, &merged_bytes, BackupPolicy::CONFIG)?;
    files.push("S/User-Startup".to_string());

    Ok(Written {
        files,
        user_startup_backup: backup,
        user_startup_created: existing.is_none(),
    })
}
```

Read `core/safety/mod.rs::guarded_write`'s exact return type first (it returns the backup path in some shape — match it; if it returns `GuardedOutcome` or similar, map to `Option<PathBuf>`). The Latin-1 round-trip (`b as char` / `c as u32 as u8`) is deliberate: a user's `User-Startup` may carry high bytes and must come back byte for byte; `merge_user_startup` takes `&str`, and every byte 0–255 is one `char` this way. Add a test `high_bytes_in_user_startup_round_trip` with a line `; caf\xE9` and assert the byte is still `0xE9` afterwards.

- [ ] **Step 4: Run** — `test result: ok. 5 passed`.

- [ ] **Step 5: Mutation** — move the `User-Startup` merge to the top of `write`; `a_failed_write_leaves_the_tree_without_a_half_written_block` must fall. Restore.

- [ ] **Step 6: fmt, clippy, commit** — `Write the first-boot files into a tree, the block last (task 5)`.

---

### Task 6: Commands `firstboot_preview` and `firstboot_write`, and the TS wrapper

**Files:**
- Create: `src-tauri/src/commands/firstboot.rs`, `src/lib/firstboot.ts`, `src/lib/firstboot.test.ts`
- Modify: `src-tauri/src/commands/mod.rs`, `src-tauri/src/lib.rs`

**Interfaces:**
- Produces (Rust): `firstboot_preview(tree: String) -> AppResult<FirstBootPlan>`, `firstboot_write(tree: String, oplog) -> AppResult<Written>`.
- Produces (TS): `firstbootPreview(tree): Promise<FirstBootPlan>`, `firstbootWrite(tree): Promise<FirstBootWritten>`, types `FirstBootPlan`, `PlannedStep`, `FatMount`, `FirstBootWritten`, `FirstBootReport`, `StepReport`, `StepOutcome`, `Ending`; mappers `stepOutcomePhrase(outcome): Phrase`, `endingPhrase(ending): Phrase`, `fatMountPhrase(fat): Phrase`.

- [ ] **Step 1: Rust command**

```rust
//! First boot: thin adapters over `core::firstboot`.

use std::path::PathBuf;

use tauri::State;

use super::oplog::{user_operation, write_result};
use crate::core::firstboot::plan::{plan, FirstBootPlan, FirstBootRequest};
use crate::core::firstboot::write::{write, Written};
use crate::core::oplog::JsonlOperationLog;
use crate::error::{AppError, AppResult};

/// §92 PREVIEW: what a first boot would run. Writes nothing.
#[tauri::command]
pub fn firstboot_preview(tree: String) -> AppResult<FirstBootPlan> {
    Ok(plan(&FirstBootRequest {
        tree: PathBuf::from(tree.trim()),
    })?)
}

/// §92 APPLY: put the files into the tree. `Safe` — nothing of the user's is
/// overwritten except `S/User-Startup`, which is merged and backed up first.
#[tauri::command]
pub fn firstboot_write(tree: String, oplog: State<'_, JsonlOperationLog>) -> AppResult<Written> {
    let request = FirstBootRequest {
        tree: PathBuf::from(tree.trim()),
    };
    let result = plan(&request)
        .and_then(|p| write(&p))
        .map_err(AppError::from);
    write_result(
        &oplog,
        user_operation("Write the first-boot files into a tree").destination(&tree),
        &result,
        |record, done: &Written| {
            let record = record.detail("Files written", done.files.join(", "));
            match &done.user_startup_backup {
                Some(backup) => record.detail("User-Startup backup", backup.display().to_string()),
                None => record,
            }
        },
    );
    result
}
```

Check `OperationRecord::destination` and `.detail` exist with those signatures (`grep -n "pub fn destination\|pub fn detail" src-tauri/src/core/oplog/*.rs`). Register: `pub mod firstboot;` in `commands/mod.rs`; `commands::firstboot::firstboot_preview, commands::firstboot::firstboot_write,` in `lib.rs`'s `invoke_handler![]` next to the `amiga_install_*` lines.

- [ ] **Step 2: Wire-shape test on the Rust side**

In `commands/firstboot.rs`:

```rust
#[cfg(test)]
mod tests {
    use crate::core::firstboot::plan::{FatMount, FirstBootPlan, PlannedStep};

    #[test]
    fn the_plan_crosses_the_wire_in_camel_case_with_kebab_tags() {
        let p = FirstBootPlan {
            tree: "x".into(),
            steps: vec![PlannedStep { name: "10-hardware".into(), tree_path: "S/FirstBoot/10-hardware".into(), fixed: true }],
            fat_mount: FatMount::Unavailable { needs: "fat95" },
            user_startup_exists: false,
            already_written: false,
            bytes_added: 1,
        };
        let json = serde_json::to_value(&p).unwrap();
        assert_eq!(json["fatMount"]["kind"], "unavailable");
        assert_eq!(json["fatMount"]["needs"], "fat95");
        assert_eq!(json["userStartupExists"], false);
        assert_eq!(json["alreadyWritten"], false);
        assert_eq!(json["bytesAdded"], 1);
        assert_eq!(json["steps"][0]["treePath"], "S/FirstBoot/10-hardware");
    }
}
```

Run `cargo test --lib commands::firstboot` → `1 passed`.

- [ ] **Step 3: TS wrapper and mappers**

`src/lib/firstboot.ts`:

```ts
// First boot — the Amiga finishes what the host cannot start.
// Mirrors src-tauri/src/commands/firstboot.rs and src-tauri/src/core/firstboot/.
//
// `firstbootPreview` writes nothing (§92 PREVIEW). `firstbootWrite` puts the
// files into the tree and merges one block into S:User-Startup.
//
// Endings stay distinct (spec §4.3): a card with no report has *not booted*,
// never "failed".

import { invoke } from "@tauri-apps/api/core";

import type { Phrase } from "@/lib/phrase";

export type FatMount = { kind: "available" } | { kind: "unavailable"; needs: string };

export interface PlannedStep {
  name: string;
  treePath: string;
  fixed: boolean;
}

export interface FirstBootPlan {
  tree: string;
  steps: PlannedStep[];
  fatMount: FatMount;
  userStartupExists: boolean;
  alreadyWritten: boolean;
  bytesAdded: number;
}

export interface FirstBootWritten {
  files: string[];
  userStartupBackup: string | null;
  userStartupCreated: boolean;
}

export type StepOutcome =
  | { kind: "ok" }
  | { kind: "skipped"; reason: string }
  | { kind: "refused"; rc: number }
  | { kind: "unfinished" };

export type Ending = "not-booted" | "unfinished" | "done-all" | "done-partial";

export interface Detected {
  system: string;
  rpi: string;
  kick: string;
}

export interface StepReport {
  name: string;
  outcome: StepOutcome;
  details: string[];
}

export interface FirstBootReport {
  version: number | null;
  system: Detected | null;
  steps: StepReport[];
  ending: Ending;
  fatCopyFailed: boolean;
  rebootRequestedBy: string | null;
  unknown: string[];
}

export async function firstbootPreview(tree: string): Promise<FirstBootPlan> {
  return invoke<FirstBootPlan>("firstboot_preview", { tree });
}

export async function firstbootWrite(tree: string): Promise<FirstBootWritten> {
  return invoke<FirstBootWritten>("firstboot_write", { tree });
}

export function stepOutcomePhrase(outcome: StepOutcome): Phrase {
  switch (outcome.kind) {
    case "ok":
      return { key: "firstboot.step.ok" };
    case "skipped":
      return { key: "firstboot.step.skipped", params: { reason: outcome.reason } };
    case "refused":
      return { key: "firstboot.step.refused", params: { rc: outcome.rc } };
    case "unfinished":
      return { key: "firstboot.step.unfinished" };
  }
}

export function endingPhrase(ending: Ending): Phrase {
  switch (ending) {
    case "not-booted":
      return { key: "firstboot.ending.notBooted" };
    case "unfinished":
      return { key: "firstboot.ending.unfinished" };
    case "done-all":
      return { key: "firstboot.ending.doneAll" };
    case "done-partial":
      return { key: "firstboot.ending.donePartial" };
  }
}

export function fatMountPhrase(fat: FatMount): Phrase {
  return fat.kind === "available"
    ? { key: "firstboot.fat.available" }
    : { key: "firstboot.fat.unavailable", params: { needs: fat.needs } };
}

/** The tone a row takes — colour is never the only signal, the key wording is. */
export function stepOutcomeTone(outcome: StepOutcome): "ok" | "muted" | "warn" | "err" {
  switch (outcome.kind) {
    case "ok":
      return "ok";
    case "skipped":
      return "muted";
    case "refused":
      return "warn";
    case "unfinished":
      return "err";
  }
}
```

`src/lib/firstboot.test.ts` — assert the four `StepOutcome` kinds and the four `Ending` values each map to a distinct key, and that the kebab tags match the Rust source (read `src-tauri/src/core/firstboot/report.rs` as text and assert it contains `rename_all = "kebab-case"` on both enums — the `amigainstall.test.ts` precedent, copy its file-reading shape). Add every `firstboot.*` key used here to `src/i18n/phrase-keys.test.ts`'s enumeration (read that file: it lists mappers and their variants).

- [ ] **Step 4: i18n keys, both catalogues**

Add to `en.json` under a new top-level `firstboot`:

```json
"firstboot": {
  "step": {
    "ok": "Done",
    "skipped": "Skipped — {{reason}}",
    "refused": "Refused (code {{rc}}); it will be tried again on the next boot",
    "unfinished": "Started and never finished — the Amiga stopped or hung on this step"
  },
  "ending": {
    "notBooted": "This card has not been booted on an Amiga yet.",
    "unfinished": "The first boot began and did not reach its end.",
    "doneAll": "Every first-boot step has run.",
    "donePartial": "The first boot ran; some steps are still waiting for the next boot."
  },
  "fat": {
    "available": "The FAT boot partition will be mounted as EMU68BOOT: (L:fat95 is in the tree).",
    "unavailable": "The FAT boot partition will not be mounted: the tree has no L:{{needs}}. Aminet disk/misc/{{needs}} supplies it; the step skips without it."
  }
}
```

and the Turkish twin in `tr.json` (translate; keep `{{reason}}`, `{{rc}}`, `{{needs}}`). Run `pnpm test -- src/i18n` and `pnpm lint`; both must pass.

- [ ] **Step 5: Commit** — `Expose the first-boot plan and write to the screen (task 6)`.

---

### Task 7: The rehearsal core — boot the tree copy, poll the report

**Files:**
- Create: `src-tauri/src/core/amigainstall/rehearse.rs`
- Modify: `src-tauri/src/core/amigainstall/mod.rs` (`pub mod rehearse;`), `run.rs` (`pub(crate) fn end_session`, `pub(crate) fn deadline_secs(limits: &RunLimits) -> Option<u64>` extracted from `deadline_units`)

**Interfaces:**
- Consumes: `run::{Clock, EmulatorLauncher, EmulatorSession, RunLimits, RealClock, WinUaeLauncher}`, `core::winuae::{DirMount, LaunchMedia, generate_uae_config}`, `firstboot::report::{parse_bytes, Ending, FirstBootReport}`, `firstboot::REPORT_PATH`.
- Produces:
  ```rust
  pub struct RehearseRequest<'a> { pub tree_copy: &'a Path, pub scratch_root: &'a Path, pub profile: &'a AmigaProfile, pub kickstart_path: &'a Path, pub winuae_path: &'a Path, pub limits: RunLimits }
  pub enum RehearsalOutcome { Finished { report: FirstBootReport }, StepRefused { report: FirstBootReport }, TimedOut { waited: Duration, report: FirstBootReport }, EmulatorClosed { waited: Duration, report: FirstBootReport } }
  pub fn media_for(request: &RehearseRequest) -> CoreResult<LaunchMedia>
  pub fn rehearse(request: &RehearseRequest, sink: &dyn ProgressSink) -> CoreResult<RehearsalOutcome>
  pub fn rehearse_with(request, launcher: &dyn EmulatorLauncher, clock: &dyn Clock, sink) -> CoreResult<RehearsalOutcome>
  ```

- [ ] **Step 1: Read the pieces you are reusing**

Read `run.rs` in full once: `media_for` (how a `DirMount` is pushed into `LaunchMedia.dir_mounts` or whatever the field is named — copy that exact field), `run_with`, `poll_until_ending`, `end_session`, and the test module's fake launcher/clock (`grep -n "struct Fake\|impl EmulatorLauncher for\|impl Clock for" run.rs`). The rehearsal's tests use the **same fakes**; make them `pub(crate)` inside `run.rs`'s test module if they are private (`#[cfg(test)] pub(crate) mod fakes`).

- [ ] **Step 2: Failing tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::amigainstall::run::tests::fakes::{FakeLauncher, FakeClock}; // adjust to the real path
    use crate::core::firstboot::report::Ending;
    use crate::core::jobs::NoProgress;
    use crate::core::profile::AmigaProfile;
    use crate::core::ScratchDir;
    use std::fs;

    fn copy_with_report(tag: &str, report: &str) -> ScratchDir {
        let d = ScratchDir::new("art-firstboot-rehearse", tag);
        fs::create_dir_all(d.join("S")).unwrap();
        fs::write(d.join("S/FirstBoot.log"), report).unwrap();
        d
    }

    fn request<'a>(copy: &'a std::path::Path, scratch: &'a std::path::Path, profile: &'a AmigaProfile, rom: &'a std::path::Path) -> RehearseRequest<'a> {
        RehearseRequest {
            tree_copy: copy,
            scratch_root: scratch,
            profile,
            kickstart_path: rom,
            winuae_path: rom, // unused by rehearse_with
            limits: RunLimits { deadline: Duration::from_secs(10), poll_interval: Duration::from_millis(1) },
        }
    }

    #[test]
    fn the_tree_copy_is_the_only_mount_and_it_boots() {
        let d = ScratchDir::new("art-firstboot-rehearse", "media");
        let profile = AmigaProfile::default_for_tests(); // use whatever run.rs's tests use
        let rom = d.join("kick.rom");
        fs::write(&rom, vec![0u8; 16]).unwrap();
        let req = request(d.path(), d.path(), &profile, &rom);
        let media = media_for(&req).unwrap();
        assert_eq!(media.dir_mounts.len(), 1);
        assert_eq!(media.dir_mounts[0].boot_priority, 127);
        assert!(!media.dir_mounts[0].read_only, "the steps delete themselves and write the report");
    }

    #[test]
    fn done_all_ends_as_finished_and_done_partial_with_a_refusal_as_step_refused() {
        let profile = AmigaProfile::default_for_tests();
        for (text, expect_finished) in [
            ("art-firstboot 1\nstep 10-hardware started\nstep 10-hardware skipped uae\nstep 10-hardware ok\ndone all\n", true),
            ("art-firstboot 1\nstep 10-hardware started\nstep 10-hardware refused rc=20\ndone partial\n", false),
        ] {
            let d = copy_with_report(if expect_finished { "fin" } else { "ref" }, text);
            let rom = d.join("kick.rom");
            fs::write(&rom, vec![0u8; 16]).unwrap();
            let req = request(d.path(), d.path(), &profile, &rom);
            let outcome = rehearse_with(&req, &FakeLauncher::running_forever(), &FakeClock::new(), &NoProgress).unwrap();
            match (outcome, expect_finished) {
                (RehearsalOutcome::Finished { report }, true) => assert_eq!(report.ending, Ending::DoneAll),
                (RehearsalOutcome::StepRefused { report }, false) => assert_eq!(report.ending, Ending::DonePartial),
                (other, _) => panic!("wrong ending: {other:?}"),
            }
        }
    }

    #[test]
    fn a_closed_window_before_done_is_emulator_closed_and_carries_what_was_written() {
        let profile = AmigaProfile::default_for_tests();
        let d = copy_with_report("closed", "art-firstboot 1\nstep 10-hardware started\n");
        let rom = d.join("kick.rom");
        fs::write(&rom, vec![0u8; 16]).unwrap();
        let req = request(d.path(), d.path(), &profile, &rom);
        let outcome = rehearse_with(&req, &FakeLauncher::exits_immediately(), &FakeClock::new(), &NoProgress).unwrap();
        match outcome {
            RehearsalOutcome::EmulatorClosed { report, .. } => assert_eq!(report.ending, Ending::Unfinished),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn the_deadline_is_a_timeout_not_a_failure() {
        let profile = AmigaProfile::default_for_tests();
        let d = copy_with_report("late", "art-firstboot 1\n");
        let rom = d.join("kick.rom");
        fs::write(&rom, vec![0u8; 16]).unwrap();
        let mut req = request(d.path(), d.path(), &profile, &rom);
        req.limits.deadline = Duration::from_secs(1);
        let clock = FakeClock::advancing_by(Duration::from_secs(1));
        let outcome = rehearse_with(&req, &FakeLauncher::running_forever(), &clock, &NoProgress).unwrap();
        assert!(matches!(outcome, RehearsalOutcome::TimedOut { .. }));
    }

    #[test]
    fn stop_before_launch_is_cancelled_and_launches_nothing() {
        use crate::core::jobs::CancelledSink; // whatever run.rs's tests use for a pre-cancelled sink
        let profile = AmigaProfile::default_for_tests();
        let d = copy_with_report("stop", "");
        let rom = d.join("kick.rom");
        fs::write(&rom, vec![0u8; 16]).unwrap();
        let req = request(d.path(), d.path(), &profile, &rom);
        let launcher = FakeLauncher::running_forever();
        let err = rehearse_with(&req, &launcher, &FakeClock::new(), &CancelledSink).unwrap_err();
        assert!(matches!(err, CoreError::Cancelled));
        assert_eq!(launcher.launches(), 0);
    }
}
```

Names like `default_for_tests`, `running_forever`, `exits_immediately`, `advancing_by`, `launches()`, `CancelledSink` are **the fakes `run.rs`'s own tests already have under some name** — use those names; if one does not exist (a clock that advances by a fixed step, say), add it to the shared fakes rather than inventing a second fake here.

- [ ] **Step 3: Implement**

```rust
//! Rehearsing a first boot under WinUAE.
//!
//! One difference from [`super::run`]: **the tree copy is the boot device.**
//! ART's own work volume is not mounted, so the chain that runs is the one a
//! real machine runs — the release's `Startup-Sequence` → `S:User-Startup` →
//! the block → the dispatcher. The host polls `S/FirstBoot.log` inside the
//! copy; a write into a `filesystem2=rw` directory mount is visible on the
//! host while the emulator runs (measured 2026-08-20).
//!
//! What it proves and does not: under UAE `10-hardware` writes `skipped uae`
//! and copies nothing, so the rehearsal proves the mechanism and never the
//! Pi3/Pi4 branch. That closes only on a real card.

use std::path::Path;
use std::time::Duration;

use serde::Serialize;

use super::run::{end_session, Clock, EmulatorLauncher, EmulatorSession, RealClock, RunLimits, WinUaeLauncher};
use crate::core::error::{CoreError, CoreResult};
use crate::core::firstboot::report::{parse_bytes, Ending, FirstBootReport, StepOutcome};
use crate::core::firstboot::REPORT_PATH;
use crate::core::jobs::ProgressSink;
use crate::core::profile::AmigaProfile;
use crate::core::winuae::{generate_uae_config, DirMount, LaunchMedia};

/// The WinUAE device name and Amiga label for the booted copy. The label is
/// what `SYS:` resolves to; the release's own sequence uses `SYS:` throughout,
/// so the label's exact text does not matter to it.
const BOOT_DEVICE: &str = "DH0";
const BOOT_LABEL: &str = "ARTSys";

#[derive(Debug, Clone, Copy)]
pub struct RehearseRequest<'a> {
    pub tree_copy: &'a Path,
    pub scratch_root: &'a Path,
    pub profile: &'a AmigaProfile,
    pub kickstart_path: &'a Path,
    pub winuae_path: &'a Path,
    pub limits: RunLimits,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum RehearsalOutcome {
    Finished { report: FirstBootReport },
    StepRefused { report: FirstBootReport },
    TimedOut { waited: Duration, report: FirstBootReport },
    EmulatorClosed { waited: Duration, report: FirstBootReport },
}

pub fn media_for(request: &RehearseRequest) -> CoreResult<LaunchMedia> {
    // Build exactly as run::media_for builds its LaunchMedia — same struct
    // literal, kickstart from request.kickstart_path — with one mount:
    let mount = DirMount {
        host_path: request.tree_copy.display().to_string(),
        volume: BOOT_DEVICE.to_string(),
        label: BOOT_LABEL.to_string(),
        boot_priority: 127,
        read_only: false,
    };
    // … construct LaunchMedia with dir_mounts: vec![mount] (field name as in run.rs)
    todo!("copy run::media_for's LaunchMedia literal here — this line is the only placeholder in the plan and exists because the struct has more fields than this plan should restate")
}

pub fn rehearse(request: &RehearseRequest, sink: &dyn ProgressSink) -> CoreResult<RehearsalOutcome> {
    let launcher = WinUaeLauncher::new(request.winuae_path, request.scratch_root);
    rehearse_with(request, &launcher, &RealClock::new(), sink)
}

pub fn rehearse_with(
    request: &RehearseRequest,
    launcher: &dyn EmulatorLauncher,
    clock: &dyn Clock,
    sink: &dyn ProgressSink,
) -> CoreResult<RehearsalOutcome> {
    let media = media_for(request)?;
    let config = generate_uae_config(request.profile, &media)?;
    let report_file = request.tree_copy.join(REPORT_PATH);

    if sink.is_cancelled() {
        return Err(CoreError::Cancelled);
    }
    sink.report(0, deadline_secs(&request.limits), "Starting the emulator");
    let mut session = launcher.launch(&config)?;
    let ending = poll(request, session.as_mut(), clock, sink, &report_file);
    end_session(session.as_mut(), sink);
    ending
}

fn poll(
    request: &RehearseRequest,
    session: &mut dyn EmulatorSession,
    clock: &dyn Clock,
    sink: &dyn ProgressSink,
    report_file: &Path,
) -> CoreResult<RehearsalOutcome> {
    loop {
        let report = read_report(report_file)?;
        if let Some(done) = finished(&report) {
            return Ok(done);
        }
        if sink.is_cancelled() {
            return Err(CoreError::Cancelled);
        }
        if !session.is_running()? {
            let report = read_report(report_file)?;
            if let Some(done) = finished(&report) {
                return Ok(done);
            }
            return Ok(RehearsalOutcome::EmulatorClosed { waited: clock.elapsed(), report });
        }
        let waited = clock.elapsed();
        if waited >= request.limits.deadline {
            return Ok(RehearsalOutcome::TimedOut { waited, report });
        }
        let ran = report.steps.len();
        sink.report(waited.as_secs(), deadline_secs(&request.limits), &format!("Waiting for the Amiga — {ran} steps reported so far"));
        clock.sleep(request.limits.poll_interval);
    }
}

fn read_report(path: &Path) -> CoreResult<FirstBootReport> {
    match std::fs::read(path) {
        Ok(bytes) => Ok(parse_bytes(&bytes)),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(parse_bytes(b"")),
        Err(err) => Err(err.into()),
    }
}

/// `done` is the only ending the Amiga writes; which of the two it is comes
/// from whether any step refused, not from the `all`/`partial` word alone — a
/// `partial` with no refusal is a reboot request, phase 3's case.
fn finished(report: &FirstBootReport) -> Option<RehearsalOutcome> {
    match report.ending {
        Ending::DoneAll | Ending::DonePartial => {
            let refused = report.steps.iter().any(|s| matches!(s.outcome, StepOutcome::Refused { .. }));
            Some(if refused {
                RehearsalOutcome::StepRefused { report: report.clone() }
            } else {
                RehearsalOutcome::Finished { report: report.clone() }
            })
        }
        Ending::NotBooted | Ending::Unfinished => None,
    }
}

fn deadline_secs(limits: &RunLimits) -> Option<u64> {
    let secs = limits.deadline.as_secs();
    (secs > 0).then_some(secs)
}
```

Replace the `todo!` by copying `run::media_for`'s literal before running anything. The progress text "N steps reported so far" is the spec's own rule about unknown totals (say "N so far", never a fixed width).

- [ ] **Step 4: Run** — `cargo test --lib amigainstall::rehearse`: `test result: ok. 5 passed`. Then `cargo test --lib amigainstall` to be sure the shared fakes did not disturb the existing 121.

- [ ] **Step 5: Mutation** — make `finished` ignore refusals (always `Finished`); the second test must fall. Restore.

- [ ] **Step 6: fmt, clippy, commit** — `Rehearse a first boot under WinUAE, the tree copy as the boot device (task 7)`.

---

### Task 8: The `firstboot_rehearse` job and its real-material hook

**Files:**
- Modify: `src-tauri/src/commands/firstboot.rs`, `src-tauri/src/lib.rs`, `src/lib/firstboot.ts`, `src/lib/firstboot.test.ts`

**Interfaces:**
- Produces (Rust): `firstboot_rehearse(request: RehearseRequestWire, winuae_path: Option<String>, app, registry, oplog) -> AppResult<JobId>`; result event `"firstboot-rehearsal-result"` with payload `{ job_id, outcome: RehearsalOutcome, copy: PathBuf, discarded: bool }`.
- Produces (TS): `firstbootRehearse(request, winuaePath): Promise<number>`, `onFirstBootRehearsalResult(cb)`, `FIRSTBOOT_REHEARSAL_EVENT`, `rehearsalOutcomePhrase(outcome): Phrase`, `rehearsalNextStepPhrase(outcome): Phrase`, `rehearsalTone(outcome)`.

- [ ] **Step 1: The command**

Model on `amiga_install_run` (read lines 1088–1180 of `commands/amigainstall.rs`): resolve WinUAE with `detect_winuae`, the profile with `profile_for` (make it `pub(crate)` in `amigainstall.rs` rather than copying it), the scratch root with `crate::scratch::root()`. Inside the job: `stage_with(tree, sink)` to make the copy, `rehearse(&RehearseRequest { tree_copy: staged.copy_path(), … }, sink)`, then — **never promote** — on `Finished` call `staged.discard()` and report `discarded: true`; on any other outcome keep the copy and report its path. Emit the result event with `app.emit(...)` the way `amiga_install_run` does; write the oplog record with the outcome's kind and the copy's path (`user_operation("Rehearse a first boot under WinUAE")`, `Safety::ReadOnly` towards the tree — the record's `destination` is the copy, not the tree).

```rust
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RehearseRequestWire {
    pub tree: String,
    pub kickstart: String,
    pub profile: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RehearsalResult {
    pub job_id: JobId,
    pub outcome: RehearsalOutcome,
    pub copy: PathBuf,
    pub discarded: bool,
}
pub const REHEARSAL_EVENT: &str = "firstboot-rehearsal-result";
```

Refuse before spawning when the tree has no `S/ART-FirstBoot` (`CoreError::InvalidInput("the tree carries no first boot yet — write it first")`) — a rehearsal of nothing would report `done all` about nothing, a confident wrong sentence.

- [ ] **Step 2: Wire test** — a Rust test serialising `RehearsalResult` with a `TimedOut` outcome and asserting `json["outcome"]["kind"] == "timed-out"`, `json["outcome"]["waited"]["secs"]` present, `json["discarded"] == false`.

- [ ] **Step 3: TS side**

Add to `firstboot.ts`:

```ts
export type RehearsalOutcome =
  | { kind: "finished"; report: FirstBootReport }
  | { kind: "step-refused"; report: FirstBootReport }
  | { kind: "timed-out"; waited: WireDuration; report: FirstBootReport }
  | { kind: "emulator-closed"; waited: WireDuration; report: FirstBootReport };

export interface RehearsalResult {
  job_id: number;
  outcome: RehearsalOutcome;
  copy: string;
  discarded: boolean;
}

export interface RehearseRequest {
  tree: string;
  kickstart: string;
  profile?: string | null;
}

export const FIRSTBOOT_REHEARSAL_EVENT = "firstboot-rehearsal-result";

export async function firstbootRehearse(request: RehearseRequest, winuaePath: string | null): Promise<number> {
  return invoke<number>("firstboot_rehearse", { request, winuaePath });
}

export async function onFirstBootRehearsalResult(cb: (r: RehearsalResult) => void): Promise<UnlistenFn> {
  return listen<RehearsalResult>(FIRSTBOOT_REHEARSAL_EVENT, (e) => cb(e.payload));
}

export function rehearsalOutcomePhrase(o: RehearsalOutcome): Phrase {
  switch (o.kind) {
    case "finished": return { key: "firstboot.rehearsal.outcome.finished" };
    case "step-refused": return { key: "firstboot.rehearsal.outcome.stepRefused" };
    case "timed-out": return { key: "firstboot.rehearsal.outcome.timedOut", params: { seconds: waitedSeconds(o.waited) } };
    case "emulator-closed": return { key: "firstboot.rehearsal.outcome.emulatorClosed", params: { seconds: waitedSeconds(o.waited) } };
  }
}

export function rehearsalNextStepPhrase(o: RehearsalOutcome): Phrase {
  switch (o.kind) {
    case "finished": return { key: "firstboot.rehearsal.next.finished" };
    case "step-refused": return { key: "firstboot.rehearsal.next.stepRefused" };
    case "timed-out": return { key: "firstboot.rehearsal.next.timedOut" };
    case "emulator-closed": return { key: "firstboot.rehearsal.next.emulatorClosed" };
  }
}
```

Import `WireDuration` and `waitedSeconds` from `@/lib/amigainstall` rather than redefining them. Tests: the four outcomes map to four distinct outcome keys **and** four distinct next-step keys; the kebab tags match `rehearse.rs`'s source. Register the keys in `phrase-keys.test.ts`.

- [ ] **Step 4: i18n** — eight keys under `firstboot.rehearsal.{outcome,next}.*` in both catalogues, in the same voice as `osinstall.amigaInstall.outcome/next`: `finished` ("Every step ran under WinUAE and the copy has been discarded" / next: "Write the tree to a card; the hardware step runs only on the real machine"), `stepRefused` ("A step refused; the copy is kept so you can read its own log" / next: "Open S/FirstBoot.log in the copy — the refused step's own lines say why"), `timedOut`, `emulatorClosed` (mirror the amigainstall sentences).

- [ ] **Step 5: The gated hook**

In `commands/firstboot.rs`'s test module, `#[ignore = "opens WinUAE against the owner's own tree and ROM; run explicitly"] fn rehearse_the_real_tree_when_asked()`, reading `ART_FIRSTBOOT_TREE`, `ART_FIRSTBOOT_ROM`, `ART_WINUAE` (and `ART_FIRSTBOOT_PROFILE`, default `a1200`), skipping with a printed sentence when any is unset. It copies the tree with `stage_with`, calls `plan` + `write` on the **copy**, then `rehearse`, prints the whole report, and asserts: `RehearsalOutcome::Finished`, and the report's three steps are `Skipped { reason: "uae" }`, `Skipped { reason: "uae" }`, `Skipped { reason: "no-spatch" }` (or `no-patches` if the tree has `C:spatch`). Print the copy's path when it fails, and never delete it then.

Run it once against `E:/amiga/ProjeART/art205-32` with the owner's ROM and paste the `test result:` line and the report text into the commit message body. **This is phase 2's closing measurement** (spec §11 item 2). If it does not pass, the phase does not close; record what the report said.

- [ ] **Step 6: Commit** — `Rehearse the first boot as a job, and prove it on the owner's 3.2 tree (task 8)`.

---

### Task 9: The OS Builder step

**Files:**
- Modify: `src/lib/buildSteps.ts`, `src/lib/buildSteps.test.ts`, `src/lib/buildSession.ts`, `src/lib/useBuildSession.ts`, `src/pages/osbuilder/steps.tsx`, `src/App.tsx`, `src/i18n/en.json`, `src/i18n/tr.json`
- Create: `src/components/osbuilder/FirstBootPanel.tsx`, `FirstBootPanel.test.tsx`

**Interfaces:**
- Consumes: `firstbootPreview`, `firstbootWrite`, `firstbootRehearse`, `onFirstBootRehearsalResult`, the mappers; `useBuildSession().session.tree.root`, `session.rom.path`; `awaitJobResult`, `jobCancel`, `isJobCancellation` from `@/lib/jobs`; `usePowerMode`.
- Produces: `StepId` gains `"ilk-acilis"`; `stepsFor("install")` returns `["hedef", "kaynak", "paketler", "amiga-kurulum", "ilk-acilis"]`; `readiness(session, "ilk-acilis", isTree)` behaves like `amiga-kurulum`; facade gains `firstboot: { written: boolean }` under key `buildSession.firstboot` with `setFirstBoot(change)`.

- [ ] **Step 1: `buildSteps` failing test**

In `buildSteps.test.ts` add: `stepsFor("install")` ends with `"ilk-acilis"`; `readiness(session, "ilk-acilis")` is `"asks"` with no tree and `"wrong-folder"` when `treeIsDistribution === false`; `stepLabelKey("ilk-acilis") === "osBuilder.step.ilk-acilis"`. Run `pnpm vitest run src/lib/buildSteps.test.ts` → fails on the type.

- [ ] **Step 2: Implement `buildSteps`** — add `"ilk-acilis"` to `STEP_IDS` after `"amiga-kurulum"`, to the `install` list, and to the `readiness` match arm with `paketler`/`amiga-kurulum`. Add `"ilk-acilis": "First boot on the Amiga"` under `osBuilder.step` in both catalogues (Turkish: "Amiga'da ilk açılış"). Test green.

- [ ] **Step 3: The facade section**

`buildSession.ts`: `export interface FirstBootChoice { written: boolean }`, `SESSION_KEYS.firstboot = "buildSession.firstboot"`, `FIRSTBOOT_SPEC = { written: isFlag }`, `DEFAULT_FIRSTBOOT = { written: false }`, and a `firstboot: FirstBootChoice` field on `BuildSession`. `useBuildSession.ts`: `setFirstBoot: (change: Partial<FirstBootChoice>) => void`, implemented exactly like `setCard`/`setPackages` (read those two first). **`written` resets to `false` whenever `setTree` changes `root`** — the value belongs to one tree, and carrying it to another is ART-197 the other way round; add that line to `setTree` and a test in `useBuildSession.test.ts` (find the existing tests for `setTree`).

- [ ] **Step 4: The panel — failing jsdom tests first**

`FirstBootPanel.test.tsx`, mocking `@/lib/firstboot` (the way `AmigaInstallPanel.test.tsx` mocks `@/lib/amigainstall` — copy its `vi.mock` block):

1. renders three step rows from a preview with `10-hardware`, `20-aux`, `30-datatypes` by name;
2. renders the `fat.unavailable` sentence containing `fat95` when `fatMount.kind === "unavailable"`, and the `available` one otherwise;
3. renders `firstboot.preview.alreadyWritten` when `alreadyWritten` is true, and the write button's label changes to `firstboot.write.again`;
4. pressing Write calls `firstbootWrite(tree)` once and renders `userStartupBackup` when present, `userStartupCreated` sentence when created;
5. **the four rehearsal endings are four distinct sentences on screen**: for each mocked `RehearsalResult` outcome, the rendered text contains `t(rehearsalOutcomePhrase(o).key)` and not any of the other three;
6. a rehearsal result whose `discarded` is false renders the copy path;
7. Stop calls `jobCancel` and renders `firstboot.rehearsal.cancelled`, not an error (use `isJobCancellation`).
8. with `usePowerMode` mocked to beginner, the AmigaDOS `treePath` column is absent and the write button is still enabled.

- [ ] **Step 5: Implement the panel**

Structure, top to bottom, §92: heading + intro (what first boot is, one paragraph); the tree field (reuse the `Field` component and the same `onTreeRootChange` pattern as `AmigaInstallPanel`); preview card (rows: step name, `treePath` in power mode only; the FAT line; `bytesAdded` as "the card grows by N KB"); refusals from `firstbootPreview`'s rejection rendered as Rust's own sentence plus `firstboot.refusal.nothingWritten`; the Write button (`Safe`: no confirmation dialog, but the result card names files and the backup); Rehearse section: Kickstart field seeded from `session.rom.path`, WinUAE path from the winuae settings the install panel already reads (`grep -n winuaePath AmigaInstallPanel.tsx`), a note that the emulator window will open, Run/Stop, progress via `awaitJobResult(FIRSTBOOT_REHEARSAL_EVENT, () => firstbootRehearse(...), (p) => p)`, then outcome sentence, next-step sentence, the report table (reuse `FirstBootReportPanel` from Task 11 — create it in this task as a plain presentational component taking `report: FirstBootReport` and move nothing later), and the copy path when kept. On a successful write call `setFirstBoot({ written: true })`.

- [ ] **Step 6: Route** — `StepIlkAcilis` in `steps.tsx` shaped like `StepAmigaKurulum` (`readiness(session, "ilk-acilis", isTree)`, `Asks`/`WrongFolder`), `<Route path="ilk-acilis" element={<StepIlkAcilis />} />` in `App.tsx`. Check `src-tauri/src/core/workflow/builtin.rs::route` has no entry to add (first boot is not a drop-panel action) — the `every_workflow_route_is_a_real_app_route` test only checks routes it lists.

- [ ] **Step 7: i18n** — every key used: `firstboot.panel.{heading,intro,tree,preview,steps,bytes,alreadyWritten,write,writeAgain,written,backup,created,nothingWritten,rehearse,rehearseIntro,kickstart,winuae,window,run,stop,running,cancelled,copyKept}` in both catalogues. `pnpm test -- src/i18n` green.

- [ ] **Step 8: Run everything** — `pnpm lint` (both passes), `pnpm vitest run src/components/osbuilder/FirstBootPanel.test.tsx src/lib/firstboot.test.ts src/lib/buildSteps.test.ts src/lib/useBuildSession.test.ts`, then `pnpm test`. Quote the counts.

- [ ] **Step 9: Commit** — `The first-boot step in the OS Builder: preview, write, rehearse (task 9)`.

---

### Task 10: Reading the report back from a card — FAT first

**Files:**
- Modify: `src-tauri/src/core/fat32.rs` (a reader), `src-tauri/src/commands/firstboot.rs`, `src-tauri/src/lib.rs`, `src/lib/firstboot.ts`

**Interfaces:**
- Produces (Rust): `fat32::read_root_file<T: Read + Write + Seek>(region: &mut Region<T>, name: &str) -> CoreResult<Option<Vec<u8>>>`; `card_firstboot_report(path: String) -> AppResult<CardFirstBootReport>` where `CardFirstBootReport { source: ReportSource, report: FirstBootReport }`, `ReportSource::{Fat, AmigaVolume, None}`.
- Produces (TS): `cardFirstBootReport(path): Promise<CardFirstBootReport>`.

- [ ] **Step 1: Failing test for the FAT reader**

In `fat32.rs`'s test module, next to the existing tests that call `create_boot_partition` and then `fatfs::FileSystem::new` (lines ~560 and ~646 — read them): create a boot partition in a scratch file, write `art-firstboot.log` into its root with `fatfs` directly (`fs.root_dir().create_file(...)`), then assert `read_root_file(&mut region, "art-firstboot.log")` returns those bytes, `read_root_file(&mut region, "absent.log")` returns `None`, and a name longer than 8.3 with a long-name entry still reads (fatfs writes LFNs by default).

- [ ] **Step 2: Implement**

```rust
/// One file from the boot partition's root, or `None` when it is not there.
///
/// Bounded: a report is a few hundred bytes, so anything past
/// `MAX_ROOT_FILE_BYTES` is refused rather than read — a card is untrusted
/// input like any other.
pub const MAX_ROOT_FILE_BYTES: u64 = 1 << 20;

pub fn read_root_file<T: Read + Write + Seek>(
    region: &mut Region<T>,
    name: &str,
) -> CoreResult<Option<Vec<u8>>> {
    let fs = fatfs::FileSystem::new(region, fatfs::FsOptions::new()).map_err(|err| {
        CoreError::Malformed { format: "FAT32".into(), detail: format!("the boot partition cannot be opened: {err}") }
    })?;
    let root = fs.root_dir();
    let entry = match root.iter().flatten().find(|e| e.file_name().eq_ignore_ascii_case(name)) {
        Some(entry) => entry,
        None => return Ok(None),
    };
    if entry.len() > MAX_ROOT_FILE_BYTES {
        return Err(CoreError::LimitExceeded { /* fill the variant's fields as error.rs defines them */ });
    }
    let mut file = entry.to_file();
    let mut bytes = Vec::with_capacity(entry.len() as usize);
    file.read_to_end(&mut bytes)?;
    Ok(Some(bytes))
}
```

Check how `fatfs`'s error type converts — the existing code at line 180 shows the shape. Check `CoreError::LimitExceeded`'s fields in `error.rs` and fill them.

- [ ] **Step 3: The command**

`card_firstboot_report(path)`: `read_card(&path)` for the MBR; find the FAT32 primary (`mbr.rs` — read how `plan_card`/`read` name the FAT entry's type and offset; `core/card/mod.rs` `CardImage.mbr` carries it); open the image read-only **but** `Region::new` needs `Read + Write + Seek` — open with `OpenOptions::new().read(true).write(true)` only if the fatfs bound requires it; if `fatfs::FileSystem::new` accepts `Read + Seek` only, open read-only. Prefer read-only; the plan does not know which bound `Region` was declared with, so read `fat32.rs` line 39 first. Call `read_root_file(&mut region, FAT_REPORT_NAME)`. If `Some`, return `{ source: Fat, report: parse_bytes(&bytes) }`; else return `{ source: None, report: parse_bytes(b"") }` (ending `NotBooted`). **The Amiga-volume read is Task 10b below and lands as its own commit**; until then the command's doc says so, and the result type already carries `AmigaVolume` so the wire does not change.

Register in `lib.rs`; wrapper in `firstboot.ts`; wire test on the Rust side for `source` serialising as `"fat"` / `"none"`.

- [ ] **Step 4: Commit** — `Read a card's first-boot report from its FAT partition (task 10)`.

#### Task 10b: The Amiga-volume copy of the report

The spec (§9) says the Amiga volume's copy wins where both exist, because the FAT copy is secondary. Reading a file out of a card's **PFS3** partition needs `libpfs3::volume::Volume::read_file` and a path walk (`core/osinstall/verify.rs` line ~143 records that it exists and `core/preload/native.rs` line ~838 shows `Volume::from_device`); out of an **FFS** partition it is `core::volume::write::file::read_file` after `dir::find_entry` from the root (see `verify.rs` line ~447). Both already run in `verify.rs`'s volume check — **read `verify.rs`'s `verify_volume` first and reuse the exact device/geometry setup it uses per area**, then:

- [ ] add `fn amiga_report(card: &CardImage) -> CoreResult<Option<Vec<u8>>>` in `commands/firstboot.rs` (or a small `core/firstboot/cardread.rs` if the device setup is more than thirty lines): for each area, for each partition whose DosType is PFS3 or FFS/OFS, try `S/FirstBoot.log` at the root; first hit wins;
- [ ] tests: an FFS fixture built with `core::volume::write` carrying `S/FirstBoot.log` reads back; a PFS3 fixture formatted through `core::preload::native` carrying the file reads back (both fixtures are synthetic, in `ScratchDir`); a card with neither returns `None`;
- [ ] the command prefers `amiga_report` over the FAT copy and reports `source: AmigaVolume`;
- [ ] commit — `Prefer the Amiga volume's own first-boot report over the FAT copy (task 10b)`.

If the PFS3 read turns out to need libpfs3 API that is not there, **stop and record it as an ART-NNN** in ISSUES.md with what was tried, ship FFS + FAT, and say so in the FEATURES row — do not fake `AmigaVolume`.

---

### Task 11: The report panel on the card screen

**Files:**
- Create: `src/components/card/FirstBootReportPanel.tsx` (if not already created in Task 9 — Task 9 creates it as a presentational component; this task mounts it), `FirstBootReportPanel.test.tsx`
- Modify: the page that calls `cardOpen(` (find with `grep -rn "cardOpen(" src --include=*.tsx`), `src/i18n/*.json`

- [ ] **Step 1: Failing tests** — `FirstBootReportPanel` given a `FirstBootReport`: renders `system` line ("PiStorm · RPi4 · Kickstart 3.2"), one row per step with the outcome sentence, `details` joined under the row in power mode only, the ending sentence, a `fatCopyFailed` note when set, `unknown` lines verbatim under a "lines this ART does not know" heading; given `ending: "not-booted"` renders **only** the not-booted sentence and no table; given `source: "fat"` vs `"amiga-volume"` renders which copy was read.

- [ ] **Step 2: Implement** — a table (`<table>` with `overflow-x: auto` wrapper), tones from `stepOutcomeTone` **plus** the wording (never colour alone), beginner mode hides `details`, `rc`, `unknown`.

- [ ] **Step 3: Mount** — on the card page, after `cardOpen` resolves, call `cardFirstBootReport(path)` and render the panel below the partition table; a failure of the report read renders its error under the panel heading and does not disturb the partition table.

- [ ] **Step 4: i18n** — `firstboot.report.{heading,system,source.fat,source.amigaVolume,fatCopyFailed,unknown,details}` both catalogues. `pnpm test`, `pnpm lint`.

- [ ] **Step 5: Commit** — `Show a card's first-boot report where the card is read (task 11)`.

---

### Task 12: Documents, sweeps, and the phase-2 close

**Files:**
- Modify: `docs/FEATURES.md`, `docs/ISSUES.md`, `docs/STATUS.md`, `docs/session-log.md`, `CHANGELOG.md`, `docs/superpowers/notes/2026-09-05-emu68hatcher-teardown.md`, `docs/superpowers/specs/2026-09-07-firstboot-design.md`

- [ ] **Step 1: Spec corrections, dated, in place** — §4.1: sidecars only when protection/date/comment is non-default (none in phase 2); `ENV:ART/…` → `ENV:ART_…`; §4.2 step 4: phase 2's dispatcher records a reboot request and does not itself reboot (the command is phase 3's, after checking which `Reboot` the trees carry: `ls E:/amiga/ProjeART/{art205-32,art159-boot}/C | grep -i reboot`, record the answer in the correction).

- [ ] **Step 2: FEATURES.md** — one row, `First boot on the Amiga (framework + hardware step)`, spec ref `Hatcher intake round 5`, 🟡, with what is measured (Task 8's rehearsal on the 3.2 tree, the report text) and what is not (the Pi3/Pi4 branch: needs a real card; phases 3 and 4 not built). Add a row for the card-side report read with its source coverage (FAT / FFS / PFS3 as actually achieved in Task 10b).

- [ ] **Step 3: ISSUES.md** — ART-166 gains a paragraph: the Amiga-side path now has two vehicles (WinUAE via `core/amigainstall`, and the first-boot `run` action planned for phase 4), and the entry stays open as the host-placement entry it is. ART-168 gains one line pointing at phase 4's `unpack`. Any defect found during Tasks 1–11 gets its own `ART-NNN`.

- [ ] **Step 4: The teardown note** — under its existing "Correction, 2026-09-06" section add a dated paragraph: *"unlocks ART-166" was stale when written — `core/amigainstall` had installed both BoingBags under WinUAE on 2026-08-21; round 5's value is the real machine, non-ASCII names and users without WinUAE (spec §1.3).*

- [ ] **Step 5: STATUS.md** — the snapshot numbers (from real `test result:` lines and `pnpm test`'s summary), and the "Start here" block **updated in place**: phase 2 of round 5 merged or not, the spike's answer in one line, phase 3 next, `main` still unpushed unless the owner pushed.

- [ ] **Step 6: session-log.md row, CHANGELOG.md entry** (user-visible: the OS Builder step, the card report panel).

- [ ] **Step 7: The full bar, quoted**

```bash
pnpm lint && pnpm test 2>&1 | tail -5
cd src-tauri && cargo fmt --check && cargo clippy --all-targets -- -D warnings 2>&1 | tail -2
cargo test --lib -- --skip artwork 2>&1 | grep "test result:"
cd .. && python scripts/control-byte-sweep.py | tail -1 && python scripts/scratch-root-sweep.py | tail -1 && python scripts/contrast-check.py --quiet | tail -1 && python scripts/scratch-counter-sweep.py | tail -3
```

Run the Rust suite twice (ART-059). Quote every summary line in the round's report at `.superpowers/sdd/2026-09-07-firstboot/phase-2-report.md`, with the mutation table (Tasks 2, 3, 4, 5, 7: what was mutated, what fell, any survivor and which of the two kinds it was).

- [ ] **Step 8: Commit** — `docs: land first boot phases 1-2 in the documents`. Then stop: merging `art-firstboot` into `main` and pushing are the owner's; say so in the final message and do not merge.

---

## Self-review

**Spec coverage (phases 1–2 only):** §4.1 files → Tasks 2, 5. §4.2 dispatcher → Task 2 (reboot deferred, Task 12 records it). §4.3 report → Task 3. §4.4 steps → Task 2. §6.3 spike → Task 1. §7 core shape → Tasks 2–5 (`NeedsDisc`/`NeedsArchiver` are phase 4). §8 rehearsal → Tasks 7–8. §9 screen → Task 9 (no checkbox: phase 3), card panel → Tasks 10, 11. §10.1 tests → each task. §10.2 row 1 → Task 8 Step 5; rows 2–3 → later phases and the owner. §12 documents → Task 12. §5 wizard and §6 packages → **not in this plan**, by design.

**Placeholders:** Task 2's `<sha256>` (filled in Step 8 before commit) and Task 7's `todo!` for the `LaunchMedia` literal (replaced before the first run) are the only two, both named as such. Task 9's i18n key list gives every key by name.

**Type consistency:** `FirstBootPlan.fat_mount: FatMount` (Task 4) is what Task 6's wire test and Task 9's panel read as `fatMount.kind`; `Written` (Task 5) is `FirstBootWritten` on the TS side (Task 6); `RehearsalOutcome` (Task 7) carries `report: FirstBootReport` (Task 3) in every variant, and Task 8's TS type mirrors that; `REPORT_PATH = "S/FirstBoot.log"` is used by Tasks 2 (script text), 7 (poll path) and 10b (volume read); `FAT_REPORT_NAME = "art-firstboot.log"` by Tasks 2 and 10. `plan()` is called by Tasks 5, 6 and 8 with the same one-field `FirstBootRequest`.
