# First boot, phase 3 — the wizard and the reboot

*Written 2026-09-15. **This is history**: it describes the tree, the other
project's source and the owner's own material on the day it was written.
Re-run the measurements rather than re-trusting the claims.*

Phase 3 of round 5 of the Emu68 Hatcher intake. Phases 1–2 (the spike, the
framework and the hardware step) are on `main`; the round's spec is
[2026-09-07-firstboot-design.md](2026-09-07-firstboot-design.md), and this
document **replaces its §5 and the reboot half of its §4.2 item 4**. Where
the two disagree, this one is newer and was measured; §1 says which of the
older spec's premises the measurements overturned.

---

## 0. The decision, in one paragraph

On the first boot of a tree ART built, the Amiga opens only the Preferences
windows ART did not already set: **Locale** always, **Input** unless ART
wrote a keymap, **ScreenMode** unless ART wrote a screen depth. Locale and
Input open in the foreground and the boot waits for each; ScreenMode opens
through `Run` and is saved once the Workbench is up, **with no reboot**. The
decision is made **on the Amiga**, from two marker files the host writes
into `Prefs/Env-Archive/` at the moment it sets the preference, so the order
in which the build's screens ran does not matter. Separately, the step
wrapper now **carries out** a step's reboot request: `C:Wait 3` then
`C:Reboot` when the tree has one, and a logged `reboot unavailable` when it
does not — the preview names the catalogue's Aminet `reboot` package, the
way it already names `fat95`.

## 1. What was measured

Two rounds, both kept in `.superpowers/sdd/2026-09-15-firstboot-phase-3/`
(git-ignored): `research.md` read sources; `experiments.md` ran about 45
WinUAE boots on **copies** of the owner's trees. Every row below is counted.

### 1.1 Material

| Tree (read-only; copies booted) | Release | `C/Reboot` | `WBStartup/` |
|---|---|---|---|
| `E:\amiga\Amigatolon\os39\art5` | AmigaOS 3.2 | present (`reboot 45.1`) | absent |
| `E:\amiga\Amigatolon\os39\art1` | AmigaOS 3.9 | absent | present |
| `E:\amiga\Amigatolon\sonuclar` | 3.9 + BoingBag 1 + 2 | absent | present |

ROMs: 3.2 `kicka1200.rom`; for 3.9 a 3.1 (40.068) A1200 ROM identified with
`scripts/rom-table-check.py --scan`. The older spec's §4.2 correction names
`art205-32`, `art159-boot` and `art205-39c`; **none of them exists any more**,
and STATUS.md's rehearsal command still points at `art205-32`.

Commands present as **files** in `C/` on all three: `Wait`, `Copy`,
`Delete`, `LoadWB`, `IPrefs`. `Ask`, `Run`, `Echo`, `If`, `Skip` are files on
none of them (shell built-ins); `Execute` is not a file on the 3.2 tree. A
presence check therefore covers disk commands only.

### 1.2 Sources read

- Emu68 Hatcher at `3f38b22` (the teardown note's commit):
  `Hatcher-FirstBoot:51-57` reboots mid-run with `DiskChange`, `Wait 3`,
  `C:Reboot`; `:112-118` reboots at the end with `C:Reboot` (UAE) or
  `C:EMU68INFO HARDRESET` (PiStorm). `reboot.yaml` makes Aminet
  `util/boot/reboot` mandatory on every release. `FirstBootWB:16-17`: the
  new mode "can only apply once no window is left open on the workbench
  screen" — *no window open*, not *a reboot*.
- Emu68-tools `095b88b`: `HARDRESET` is in the binary-only Emu68Info; the
  same author's `Emu68Reset.c:116-138` kills ExecBase and arms the Pi's
  watchdog, which reloads Emu68 and re-reads `config.txt`. A plain
  `ColdReboot()` does not.
- WinUAE `5db5720`: a guest reset is `cpureset()` inside the running
  `m68k_go` loop (`newcpu.cpp:8038`, `:6726-6733`); directory mounts are
  re-mounted from the config (`filesys.cpp:7416-7465`).
- HstWB `5dfdf48`: waits 10 s "to allow file system to write changes" and
  quits UAE rather than rebooting.
- Aminet `util/boot/reboot.lha`: Reboot 1.01, public domain by its own
  documentation, 4 508 B, calls `ColdReboot()`. Its MD5 matches Hatcher's.

### 1.3 The four experiments

| # | Question | Result (counted) |
|---|---|---|
| 1 | Can 3.9 reboot without `C:Reboot`? | Aminet Reboot 1.01 on a 3.1 ROM: **11/11**. `Utilities/Installer` 44.10 `(reboot)`: **0/2** — it stops on its *Set Installation Mode* page, and its template has no argument that skips it. No command (control): 1 boot. 3.2's own `C:Reboot`: works. |
| 1b | Does a write just before the reboot survive? | The log line reached the host **every time**. Without a wait the FFS volume needed validation **8/8** (3.2's "waits for writes" `C:Reboot` did not prevent it, 4/4); with `Wait 3`, **0/6**. The validator's *failure* traced to an amitools-formatted image; an image formatted by AmigaOS's own `Format` rebooted cleanly 6/6. |
| 2 | Does the WinUAE session survive a guest reboot? | Same PID and a live directory mount after the reboot: **18/18**. |
| 3 | Do the Preferences editors block when run from a script? | Foreground `SYS:Prefs/Input` and `Locale` on 3.2 and 3.9: block, `LoadWB` never reached (**4/4**). Under `Run <NIL: >NIL:`: do not block (**4/4**). |
| 4 | Does a ScreenMode change need a reboot? | **No**, on both releases. `ScreenMode FROM … USE` with the boot console open: an IPrefs *System Request* for ~26 s, then the screen resets within ~4.7 s of the console closing, nobody clicking. From a background script after the console closed: applied within one sampler tick (≤ 2.7 s), no requester. Control: unchanged 2/2. An ARexx sampler read the screen from `IntuitionBase` and was checked against screenshots first. |

**Not measured:** the interactive editor's own Save (only `FROM … USE`); a
user's own window holding the screen; PFS3 or a real card's partition under
a reboot; Emu68 `HARDRESET`; plain 3.9 (`art1`) — `sonuclar` booted.

### 1.4 What the measurements overturned

1. **The older §5.1 sent ScreenMode through `WBStartup` *and* `ART_Reboot`.**
   Experiment 4 needs neither. Phase 3 as designed then has no step that
   requests a reboot; the owner chose to build the reboot anyway (§2).
2. **The older §5.3 decided "answered" from a build-session facade**
   (`written: { keymap, screenmode }`). That field was never built, and it
   cannot work: the Appearance panel lives on `/os-builder/birimler` and is
   applied with its own button **after** the build run, whose last phase is
   the first-boot write — so the fact arrives after the file that needed it.
   A tree cannot answer it either: a release ships its own `ScreenMode.prefs`
   and ART only rewrites the depth inside it.
3. **`WBStartup` cannot be the route:** the 3.2 tree has no `WBStartup/`
   drawer and no `RexxMast`.
4. **"Call by name" is not safe.** On the 3.2 copy a bare `Format` answered
   *Unknown command* from `User-Startup`, although `Path` names `SYS:System`
   (the likely cause, not verified: `Path` names drawers the tree lacks).

## 2. Decisions taken with the owner, 2026-09-15

| # | Question | Decision |
|---|---|---|
| 1 | Scope | The wizard **and** the reboot. |
| 2 | Measure first? | All four experiments before the design (§1.3). |
| 3 | Build the reboot although no phase-3 step requests one? | Yes, now. |
| 4 | Where 3.9's reboot command comes from | As `fat95`: the tree's own `C/Reboot` is used; when absent, first boot is still written, the preview names the catalogue package, and the Amiga logs `reboot unavailable`. The download is the user's. |
| 5 | The screen's control for the wizard | A second tick, *"Ask the remaining preferences on the Amiga at first boot"*, default on, remembered, rendering writes nothing. |
| 6 | How "answered" is decided | Markers in the tree written by whoever set the preference; the Amiga decides (§4). |
| 7 | Who writes the Input marker | The first-boot write, from the tree's `;BEGIN keymap-selection` block — `osinstall` is not touched (§4.3). |

## 3. The Amiga side

### 3.1 Files

```
S/FirstBoot/90-prefs                 the wizard                        (fixed; only when asked)
Prefs/Env-Archive/ART_Set_Input      "TRUE" — ART wrote a keymap        (first-boot write)
Prefs/Env-Archive/ART_Set_ScreenMode "TRUE" — ART wrote a screen depth  (Appearance)
S/ART-FirstBoot-Step                 the wrapper, now carrying out a reboot (fixed, changed)
```

`90-prefs` is **fixed text**, compiled in with `include_str!` and pinned
exact-match like every other script in `core/firstboot/scripts/`. Nothing in
it varies per build; the markers carry the per-build facts. That replaces
the older spec's "generated" `90-prefs`.

A marker's content is `TRUE` with no trailing newline, as
`core/amigaprefs/env.rs` writes every value; the Amiga only ever asks
`IF EXISTS`, so the content is for a person reading the drawer.

### 3.2 `90-prefs`

In this order, every command by full path, every variable read `${name}`,
no `Quit` (the three measured rules of the dispatcher's own header):

1. If `${ART_FirstBootBanner}` is `TRUE`: one `Echo` naming what was
   detected (`${ART_System} ${ART_RpiType}`, Kickstart `${ART_Kick}`).
2. **Locale**: if `SYS:Prefs/Locale` exists, log
   `step 90-prefs detail opened Locale`, `Echo` one sentence (*choose your
   language and country, then Save*), run it in the foreground — the boot
   waits. Otherwise log `step 90-prefs detail missing Locale`.
3. **Input**: if `ENVARC:ART_Set_Input` exists, log
   `step 90-prefs detail not-asked Input`. Otherwise as Locale, in the
   foreground, or `missing Input`.
4. **ScreenMode**: if `ENVARC:ART_Set_ScreenMode` exists, log
   `not-asked ScreenMode`. Otherwise, if the editor exists, log
   `opened ScreenMode` and `Run <NIL: >NIL: SYS:Prefs/ScreenMode` — it does
   not block (experiment 3), the boot goes on to `LoadWB`, and the user saves
   on a Workbench with no console (experiment 4, arm B). A Save made before
   the console closed shows IPrefs' requester, which clears itself when the
   console closes (arm A). Otherwise `missing ScreenMode`.

The step ends normally and the wrapper logs `ok` and deletes it. **A machine
switched off inside a foreground window** leaves `step 90-prefs started`
with no ending and the file in place: the report says *unfinished* and the
next boot asks again. That is the existing behaviour for any step, kept.

`ENVARC:` is read directly rather than `ENV:`: it is correct whether or not
the release copied `ENVARC:` to `ENV:` before `User-Startup` (the 3.9
`Startup-Sequence` copies it at line 15; the 3.2 one links `RAM:ENV` to
`ENVARC:` at line 21 — read, context not checked).

### 3.3 The reboot, in `ART-FirstBoot-Step`

After the step's own `ok`/`refused` line and its deletion, when
`${ART_Reboot}` is `TRUE`:

```
SetEnv ART_Reboot "FALSE"
Echo >>S:FirstBoot.log "reboot requested by [name]"
IF EXISTS C:Reboot
  C:Wait 3
  C:Reboot
ELSE
  Echo >>S:FirstBoot.log "reboot unavailable"
ENDIF
```

- The variable is cleared **before** the reboot: on the 3.2 tree `ENV:` may
  be `ENVARC:` itself (§3.2), so a `TRUE` left behind could survive into the
  next boot. The dispatcher's own `SetEnv ART_Reboot "FALSE"` at start stays
  as the second guard.
- `Wait 3` is experiment 1b's number, measured on FFS. PFS3 and a real
  card are not measured; the plan does not claim them.
- **Plain `Reboot` on PiStorm too.** `HARDRESET` exists to reload Emu68 after
  `config.txt` changed; no first-boot step touches the FAT partition. A step
  that one day does is where `HARDRESET` belongs, with its own measurement.
- The remaining steps run on the next boot through the same block, which is
  what the older spec already promised; the log simply grows across boots.
- Without a reboot command the run continues to `done`, and the report says
  a restart was asked for and could not be made (§6).

### 3.4 What the plan now requires of `C/`

`NEEDED_COMMANDS` gains `Wait` (a file on all three trees). `Reboot` is not
required; it is an availability (§4.4). No shell built-in is ever looked for.

## 4. The host side

### 4.1 Names, in one lower-level place

`core/amigaprefs/env.rs` gains the two marker names and their tree paths.
`core/appearance` and `core/firstboot` both read them from there, so
neither imports the other (the inward rule).

### 4.2 `ART_Set_ScreenMode`: written by Appearance

When `apply_appearance_with` writes a screen depth, it writes the marker in
the **same** committed list, through `guarded_write` with
`BackupPolicy::CONFIG` like the prefs file beside it, and reports it in
`AppearanceOutcome.written`. No depth written, no marker. The marker is never
removed: a depth ART wrote stays written.

### 4.3 `ART_Set_Input`: written by the first-boot write

`core/firstboot::write` asks the tree's `S/User-Startup` whether it carries
a **well-formed** `keymap-selection` block — through `core/osinstall::startup`'s
own block pairing, never a substring search (that module's doc explains the
stray-`;BEGIN` case). Today that pairing is private: the module's only public
function is `merge_user_startup` (`startup.rs:173`), which the first-boot write
already imports. The module gains one small public query built on the same
pairing, so the two can never disagree about what a block is. Block present: write the marker. Block absent and
marker present: remove the marker. The keymap line is written only by the
build's tree phase, and the build run always follows that phase with the
first-boot phase when first boot is wanted, so the marker is re-derived
every time it can have changed. A tree ART built before this round has the
block and no marker, and is corrected by the same read.

### 4.4 Plan, preview and write

- `FirstBootRequest` gains `ask_prefs: bool`.
- `FirstBootPlan` gains:
  - `reboot: RebootCommand` — `Available`, or `Unavailable { needs: "reboot" }`
    (the catalogue id in `core/sources/bundle/catalogue/acilis.json`), the
    same `#[serde(tag = "kind")]` shape as `FatMount`;
  - `wizard: Option<WizardPlan>` — `None` when not asked; otherwise one row
    per window, each `Ask`, `SetByArt` or `Missing`, computed from the
    markers and the editors present **at preview time**.
- `steps` lists `90-prefs` when asked; `bytes_added` counts it.
- `write` writes `90-prefs` when asked and **removes ART's own `90-prefs`**
  when not, so unticking and building again does not leave the wizard behind.
  `write` twice is still byte-identical.
- No new refusal and no new `ART-*` code: a missing `C/Wait` is the existing
  `FirstBootNeedsCommand`.

## 5. The screen

- **Choice tab**, under the first-boot tick: the second tick over
  `session.firstboot.askPrefs` — optional in `FirstBootChoice`, **absent
  meaning ticked**, rendering writes nothing, a stored `false` survives: the
  `wanted` rule, guarded the same way. With the first-boot tick off it stays
  visible and enabled and says it has no effect (Beginner mode hides, never
  disables).
- Beneath it, from `firstboot_preview`: which windows will open, and in one
  line why the others will not (*ART set the keymap*, *ART set the screen
  mode*, *this release has no …*). When `reboot` is unavailable, one line
  naming the catalogue's Reboot package — beside the tick, not after the
  build, the reason `fatMountPhrase` sits there. Beginner mode hides file
  names and AmigaDOS lines.
- **The preview is honest about order:** Appearance applied after the build
  writes its marker then, and the Amiga decides on the day. The line under
  the tick reads the tree as it is when shown and says so.
- `useBuildRun`'s `firstboot` arm passes the tick to `firstbootWrite`.
- Every string in both catalogues, in the same commit.

## 6. The report

- `reboot unavailable` becomes a parsed line (`reboot_unavailable: bool`);
  today it would fall into `unknown`.
- The screen keeps the endings apart: *a step asked for a restart and the
  Amiga restarted*, *a step asked for a restart and this Amiga has no
  command for it — restart it yourself to finish*, and no restart at all.
- A log spanning boots — a second `system` line, a step retried — is already
  read correctly by `parse_bytes` (latest step of a name, latest `done`);
  it gets a test rather than a change.
- `90-prefs`'s `detail` lines are shown on the card's report panel and in the
  rehearsal as *opened*, *not asked — ART set it*, *this release has no …*.

## 7. The rehearsal

- **A reboot is a mid-run event, not an ending** (experiment 2, 18/18). The
  poll carries on; `finished()` keeps ending only on a `done` line. The
  phase-2 comment calling *"a `partial` with no refusal a reboot request"*
  is removed: with the reboot carried out, no `done` line precedes it.
- **A rehearsal with the wizard is interactive.** The foreground windows wait
  for a person (experiment 3). While polling, the progress line names the
  window the Amiga is waiting in, from the latest `opened` detail with no
  `ok` after it: *"the Amiga is waiting for you in the Locale window — answer
  it in the WinUAE window"*. A rehearsal that reaches its deadline there says
  which window was waiting, a different sentence from a machine that stopped
  writing.
- The four endings and their copies are unchanged.

## 8. Proof

### 8.1 Tests, by layer

- `core/firstboot/scripts`: `90-prefs` and the changed wrapper pinned
  exact-match; mutations — drop the `C:Wait 3`, drop the `IF EXISTS
  C:Reboot` guard, move `SetEnv ART_Reboot "FALSE"` after the reboot, run
  ScreenMode in the foreground, drop a `not-asked` branch — each seen red.
- `core/firstboot::plan`: the wizard rows for markers present/absent, an
  editor missing, `ask_prefs` false; `reboot` available/unavailable; a tree
  without `C/Wait` refused **by that command's name**.
- `core/firstboot::write`: twice byte-identical; the Input marker written
  with a well-formed block, removed without one, **not** written for a stray
  `;BEGIN keymap-selection` with no closer; `90-prefs` removed when unticked.
- `core/appearance`: a depth writes the marker in the same outcome; no depth
  writes none.
- `core/firstboot::report`: `reboot unavailable`; a two-boot log.
- `core/amigainstall::rehearse`: a fake launcher whose log contains a reboot
  then `done all` ends `Finished`; one that stops at `opened Locale` times out
  with that window named.
- Frontend: the second tick in jsdom (absent = ticked, rendering writes
  nothing, untick survives a reload); the windows line and the reboot line;
  `useBuildRun` passes the tick.
- Sweeps: control-byte (the scripts are ASCII with LF), scratch-root,
  scratch-guard, contrast.

### 8.2 Real material, `#[ignore]`d and gated

On copies of the owner's trees, through the existing rehearsal:

| Proof | Tree | Expected |
|---|---|---|
| A test-only step that requests one reboot | 3.2 (`art5`) | two `system` lines, `reboot requested by`, `done all`, `Finished` |
| The same | 3.9 (`sonuclar`) | `reboot requested by`, `reboot unavailable`, `done all` |
| The wizard, with a short deadline | 3.2 and 3.9 | `step 90-prefs detail opened Locale`, then the deadline with Locale named |

The test-only step lives in the test, never in `scripts/`.

### 8.3 The phase's closing measurement — a person

A rehearsal the owner answers by hand, on 3.2 and on 3.9: the windows that
open are exactly the preview's rows, the report says so, and a ScreenMode
saved from the editor applies without a reboot. The interactive Save path is
what §1.3 did not measure; until this is done the FEATURES row stays 🟡 and
says why.

## 9. Out of scope

`HARDRESET` and any step that changes the FAT partition; ART shipping or
fetching the Reboot binary itself; a Time/Font/Overscan window; PFS3 and a
real card under a reboot (phase 5's real card); phase 4's packages.

**Found on the way, not this round's:** amitools `xdftool`-formatted FFS
images fail the Kickstart validator after an unclean reboot (8/8), while
AmigaOS's own `Format` does not (0/6). ART's own `core/volume` formatter was
not tried. The plan's documents task records it for the owner to rule on.

## 10. Documents this round touches

This spec; a plan in `docs/superpowers/plans/`; dated corrections in place
in the 2026-09-07 spec (§4.2 item 4's reboot and tree names, §5.1, §5.3);
STATUS.md's rehearsal command (the stale `art205-32` path); FEATURES.md's
first-boot row; ISSUES.md for anything found; CHANGELOG; CLAUDE.md's command
list if a new gated test command is added. Task reports under
`.superpowers/sdd/`.

## 11. Sources

- `.superpowers/sdd/2026-09-15-firstboot-phase-3/research.md` and
  `experiments.md` (git-ignored), with the experiment scripts, configs and
  results under `E:\amiga\ProjeART\build\tmp\fb3-exp\`.
- `D:\tmp\hatcher-research\`: Emu68 Hatcher `3f38b22`, Emu68-tools
  `095b88b`, WinUAE `5db5720` (four files), HstWB `5dfdf48`, Aminet
  `util/boot/reboot.lha`.
- The owner's trees listed in §1.1.
