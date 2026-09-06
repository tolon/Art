# First boot — the Amiga finishes what the host cannot start

*Written 2026-09-07. **This is history**: it describes the tree, the other
project's source and the owner's own material on the day it was written.
Re-run the commands below rather than re-trusting the claims.*

**Round 5 of the Emu68 Hatcher intake.** Rounds 1–4 (prefs and wallpaper,
drawer icons, refusal evidence, media identification by hash) are merged to
`main`. The intake note is
[2026-09-05-emu68hatcher-teardown.md](../notes/2026-09-05-emu68hatcher-teardown.md);
it ranks this round second of five and was re-checked against the tree before
this was written — §1.3 below records where it had gone stale.

---

## 0. The decision, in one paragraph

ART writes a small set of AmigaDOS scripts into the distribution tree. They
run **once**, on the machine the card was built for, dispatched from
`S:User-Startup`, and they do three families of work: silently pick the
hardware-dependent files for the Pi the card actually booted on; open only
those Preferences windows nobody has answered yet; and run a package's own
installer or unpack an archive **on the Amiga**, where names, passwords and
hardware are native. Every step writes one line to a report the host reads
back from the card later — or, in a WinUAE rehearsal, live. The owner chose
the full combination (hardware + wizard + packages) on 2026-09-06, and chose
the hook, the report, the wizard rule and the package scope in four
questions recorded in §3.

## 1. What was measured

Nothing below rests on recalled knowledge. Each claim names what was read
or run.

### 1.1 The other project's mechanism, read from its source

`rootrootde/emu68hatcher` at commit `3f38b22` (2026-08-28), MIT. Files fetched
with `gh api repos/rootrootde/emu68hatcher/contents/<path>?ref=3f38b22…` on
2026-09-06 and read in full:

| File | What it is |
|---|---|
| `…/local_packages/System/S/Startup-Sequence_FirstBoot` (60 lines) | the block injected into `Startup-Sequence` after `BindDrivers`: detects PiStorm vs UAE by `C:Version brcm-emmc.device` / `brcm-sdhc.device`, sets `System`, `RpiType`, `KickstartVersion`, mounts `SD0:` and assigns `EMU68BOOT:`, then `Execute S:Hatcher-FirstBoot` |
| `…/S/Hatcher-FirstBoot` (121 lines) | the wizard: runs every file in `S:FirstBoot/` via a `List … LFORMAT` generated script, deletes each after it runs, writes `S:FirstBoot.done`, clears `ENVARC:FIRSTTIMEBOOT` early, reboots on `$REBOOT`, opens `Prefs/Locale` and `Prefs/Input`, self-deletes, reboots |
| `…/FirstBoot/Pi4vsPi3_Pistorm` (48) | copies `Storage/DOSDrivers/SD0pi3` or `SD0pi4` to `DEVS:DOSDrivers/SD0` by `$RpiType`, renames the matching `HDToolBoxPi3.info`/`Pi4.info` to `HDToolbox.info`, mounts `SD0:` explicitly |
| `…/FirstBoot/AUXRename` (4) | 3.9 only: `Storage/DOSDrivers/_AUX` → `AUX` |
| `…/FirstBoot/PatchAkDatatypes` (51) | `C:spatch` over four `ak*.datatype` files when a `-040.pch` sits beside each |
| `…/FirstBoot/Ibrowse` (12) | `spatch` over IBrowse codecs, when installed |
| `…/WBStartup/FirstBootWB` (22) | second boot: opens `Prefs/ScreenMode` detached, deletes itself |
| `…/Devs/DosDrivers/SD0pi3`, `SD0pi4` | mountlists: `Device = brcm-sdhc.device` / `brcm-emmc.device`, **`FileSystem = L:fat95`**, `DOSTYPE = 0x46415401` |
| `…/data/packages/firstboot.yaml` | the package that places them: `mandatory: true`, `from: System/FirstBoot/*` |
| `builder/staging/scripts/injector.py` (365) | marker-block injection into `Startup-Sequence`; its comment at line 226: *"InjectAfter BindDrivers stacks LIFO … UAEGFX, FirstBoot, REXXMAST, RTC"* |
| `builder/pipeline/configure_scripts.py` (417) | the stage that drives the injections |
| `tests/test_firstboot.py` (6) | one test: the sequence fragment must not contain `>ENV:` |

**Not measured**: the application was not run and no Hatcher card was built.
Everything above is read off source.

### 1.2 Where the hook goes — decided by two real `Startup-Sequence` files

The owner's own trees, built by ART from their own media:

```bash
grep -n -i "SetPatch\|BindDrivers\|Mount \|IPrefs\|AddDataTypes\|RexxMast\|User-Startup\|LoadWB" \
  E:/amiga/ProjeART/art205-32/S/Startup-sequence      # AmigaOS 3.2
grep -n -i "…same…" E:/amiga/ProjeART/art159-boot/S/Startup-Sequence   # AmigaOS 3.9
```

Both answer the same order:

```
SetPatch → ENV: → BindDrivers → Mount DEVS:DOSDrivers/~(#?.info) → Monitors
→ AddDataTypes REFRESH QUIET → IPrefs → (3.9: RexxMast) → Execute S:User-Startup → LoadWB → EndCLI
```

Hatcher's block sits immediately after `BindDrivers`: **before** the
DOSDrivers mount glob, before `AddDataTypes`, before `IPrefs`, before
`RexxMast`. Its own source pays for that position three times:
`Hatcher-FirstBoot` runs `C:AddDataTypes REFRESH QUIET` itself for 3.9 (the
very line ART-193 cost three sittings to find), `injector.py` removes the
release's `RexxMast` line and re-adds it after `BindDrivers` *"so FirstBoot
scripts can use ARexx"*, and it suppresses the duplicate-`SD0` mount error on
second boots. Nothing in this round's three families needs to run before
`IPrefs` — Hatcher itself applies ScreenMode by rebooting.

So the hook is `S:User-Startup`, and this is an engineering choice, not a
rules choice: at that point the release's own sequence has already done the
five things `core/amigainstall` had to reconstruct by hand (ART-189..193),
and ART's `merge_user_startup` (`core/osinstall/startup.rs`) already edits
that file in place with the `;BEGIN <id>` … `;END <id>` convention real
installers use. Cost: a DOSDriver copied into `DEVS:` during first boot has
missed the mount glob and is mounted explicitly in the same step —
Hatcher's `Pi4vsPi3_Pistorm` does the same.

### 1.3 The intake note was stale on the one claim that mattered

The teardown says this round *"unlocks ART-166"*. Checked against the tree:
`core/amigainstall/` exists, and FEATURES.md's row *"run a package's own
installer on the Amiga"* is ✅ with a measurement — both of the owner's
BoingBags installed under WinUAE on 2026-08-21 and the tree answered
`Workbench 45.3`. ART-166 stays open as a **host-placement** entry by the
owner's decision, but the Amiga-side path it points to was built on
`amiga-side` weeks before the teardown was written. This round is therefore
**not** the BoingBag round. Its value is:

1. **Things only the real machine can decide.** Which Pi the card booted on
   (`brcm-emmc.device` vs `brcm-sdhc.device`), and so which `SD0` DOSDriver
   and which `HDToolBox` icon. ART's card already boots on both boards through
   `config.txt` conditional sections (ART-204); the Amiga side has no
   counterpart — neither of the owner's trees carries any `SD0` driver
   (`ls E:/amiga/ProjeART/{art205-32,art159-boot}/Storage/DOSDrivers`: `CD0
   PC0 PC1 RAD _AUX` and `CD0 PC0 PC1 RAD ZIC ZIP _AUX`).
2. **Things the host cannot do and an emulator does not help with.** Non-ASCII
   AmigaDOS names (ART-168, the `türkçe` drawer) unpacked by the Amiga's own
   tools; binary patches; anything that reads the real hardware.
3. **A user without WinUAE.** `core/amigainstall` requires it; a first-boot
   step runs on the machine the card is going into.

### 1.4 Two facts that changed the design while it was being written

- **Mounting the FAT boot partition on the Amiga needs `L:fat95`.** Both of
  Hatcher's `SD0` mountlists name it. It is a third-party handler
  (Aminet `disk/misc/fat95`, freeware), and ART's bundle catalogue already
  lists it (`core/sources/bundle/catalogue/dosya-sistemi.json`, id `fat95`).
  ART never fetches on its own — the owner's policy is that the user starts
  every download — so the `SD0`/`EMU68BOOT:` half of the hardware step is
  **conditional on `L:fat95` being in the tree**, the preview says so and
  names the package, and without it the step logs `skipped no-fat95` rather
  than failing. The report's FAT copy (§4.2) is skipped with it.
- **Neither release ships `C:LhA`.** `ls E:/amiga/ProjeART/art205-32/C | grep
  -i lha` finds nothing; the 3.9 tree carries `xadUnFile` and friends
  (xadmaster), the 3.2 tree carries no archiver at all. So the `unpack`
  action uses `C:xadUnFile` when present, else `C:LhA` when present, else
  the plan refuses and names Aminet's `util/arc/lha.run`.

### 1.5 What ART writes host-side today, so the wizard knows what not to ask

`grep -rln -i keymap src-tauri/src/commands` → `commands/osinstall.rs` only:
the keymap is a field of the install request (ART-226). The screen depth and
wallpaper come from `AppearancePanel.tsx` (round 1). **Locale is written by
nothing.** The releases themselves differ in what they ship —
`art159-boot/Prefs/Env-Archive/Sys` has `locale.prefs` and no
`ScreenMode.prefs`; `art205-32`'s has `ScreenMode.prefs` and no `locale.prefs`
— which is why §5.3 derives "already answered" from what **ART wrote**, never
from which files exist.

---

## 2. Scope

**In**, as one spec and one four-phase plan (§9):

- A. the framework and the silent hardware step;
- B. the first-boot wizard, asking only what nobody answered;
- C. package actions on the Amiga: `run` and `unpack`, declared in the
  package recipe as data;
- the report, read back from the card and live in a WinUAE rehearsal;
- an OS Builder step and the card-reading screen's report panel.

**Out**, deliberately: ART applying `spatch`/`PTCH` patches itself (Hatcher's
decoder is noted in the teardown; the Amiga's own `spatch` runs it here);
tooltype-patching `HDToolBox.info` / `Videocore.info`; the Emu68
`dtoverlay` tuning line; anything that runs on a third boot; the IBrowse
step (ART ships no IBrowse).

---

## 3. Decisions taken with the owner, 2026-09-06

| # | Question | Decision |
|---|---|---|
| 1 | Scope | A + B + C in one round; the plan is phased and each phase closes on its own measurement (§9). |
| 2 | Where the hook goes | `S:User-Startup`, `;BEGIN art-firstboot`/`;END art-firstboot`, because it is better (§1.2), not because it is the rule. |
| 3 | How ART learns the outcome | A per-step report file on the card, written to the Amiga volume and copied to the FAT partition when it is mounted (§4). |
| 4 | What the wizard asks | Only preferences ART did not write during this build; one checkbox, default on (§5.3). |
| 5 | Which package work | Both `run` (the package's own installer) and `unpack` (an archive opened by the Amiga), one mechanism (§6). |

The owner's phrasing on question 2 — *"if Hatcher's approach is better, use
it; we are engineers, not lawyers"* — is why §1.2 argues from the two real
files rather than from `startup.rs`'s doc comment.

---

## 4. The Amiga side

### 4.1 Files written into the tree

```
S/User-Startup                    ;BEGIN art-firstboot … ;END art-firstboot   (merged in place)
S/ART-FirstBoot                   the dispatcher                               (fixed, embedded)
S/FirstBoot/10-hardware           detect Pi3/Pi4/UAE; SD0 DOSDriver; EMU68BOOT: (fixed)
S/FirstBoot/20-aux                3.9 only: Storage/DOSDrivers/_AUX → AUX     (fixed)
S/FirstBoot/30-datatypes          spatch ak*.datatype when a .pch is present  (fixed)
S/FirstBoot/50-pkg-<id>           one per selected package: run or unpack     (generated)
S/FirstBoot/90-prefs              the wizard: only the windows to ask         (generated)
Prefs/Env-Archive/ART/FirstBoot   "TRUE"                                      (generated)
Storage/DOSDrivers/SD0pi3, SD0pi4 the two mountlists; 10-hardware picks one   (fixed)
ART/FirstBoot/Packages/<id>/      the package's unpacked wrapper, or the archive itself
ART/FirstBoot/Volumes/<name>/     what a `needs_volume` package must find (§6.2)
WBStartup/ART-FirstBootWB(.info)  second boot, ScreenMode only, self-deleting (fixed; only when asked)
```

"Fixed" files are AmigaDOS text checked into `core/firstboot/scripts/` and
compiled in with `include_str!`; they never vary per build and are pinned by
exact-match tests. "Generated" files vary only in a step list and a few
`SetEnv` lines. Every file gets a `.uaem` sidecar (`----rwed`) like every
other file in the tree; `Execute` does not need the script bit.

The `User-Startup` block is four lines and nothing else:

```
;BEGIN art-firstboot
IF EXISTS S:ART-FirstBoot
  Execute S:ART-FirstBoot
ENDIF
;END art-firstboot
```

After the last step has run the dispatcher deletes itself, the block becomes
a dead `IF EXISTS`, and the next `apply` or `firstboot_write` on that tree
removes it through `merge_user_startup`. Nothing on the Amiga edits
`User-Startup`.

### 4.2 The dispatcher

`FailAt 2000000000` first — `core/amigainstall`'s measured value (ART-188:
`Updater` 45.15 returns 900 and the stock `FailAt 21` aborted the script).

1. Detect, and record the detection as the report's second line:
   `C:Version brcm-emmc.device` → `PiStorm RPi4`; else `brcm-sdhc.device` →
   `PiStorm RPi3`; else `UAE`. `KickstartVersion` from `$KICKVER` (47 → 3.2),
   else `Version Workbench.Library 45` (3.9), else `exec.library 40` (3.1).
   Hatcher's probes, verbatim in method; ART's own variable names
   (`ART_System`, `ART_RpiType`, `ART_Kick`) so nothing collides with a
   user's or Hatcher's.
2. If `ENV:ART/FirstBoot` is not `TRUE` and `S:FirstBoot/` is empty: delete
   `S:ART-FirstBoot` and stop. (A tree whose flag was cleared by a partial
   run still finishes its remaining steps; the flag only gates the wizard's
   banner and the "first" in first boot.)
3. Copy the flag into a per-boot variable first (`SetEnv ART_FirstBootBanner
   $ART/FirstBoot`, Hatcher's `HATCHERFIRSTBOOT` move), then write
   `ENVARC:ART/FirstBoot` = `FALSE` **now**, before any step — the same
   reason Hatcher gives: a partial run must not repeat as if it were the
   first, and the wizard still needs to know, this boot, that it was.
4. For each file in `S:FirstBoot/`, sorted by name (`List … LFORMAT` into
   `T:`, the way Hatcher does it, because AmigaDOS has no `for`): write
   `step <name> started` to the report; `Execute` it; on `WARN` or worse
   write `step <name> refused rc=<n>` and **leave the file**; otherwise write
   `step <name> ok` and delete it. A step that decided not to run writes its
   own `skipped <reason>` line before returning 0 and is deleted like any
   success. If `ENV:ART/Reboot` is `TRUE` after a step: clear it, write
   `reboot requested by <name>`, `Wait 3`, `Reboot` (UAE) or `EMU68INFO
   HARDRESET` (PiStorm, the command Hatcher uses); the remaining steps run
   on the next boot through the same block.
5. `done all` if `S:FirstBoot/` is empty, else `done partial`. Delete
   `S:ART-FirstBoot` only on `done all`.
6. If `SD0:` is mounted, copy `S:FirstBoot.log` to `SD0:art-firstboot.log`;
   on failure append `copy-to-fat failed` to the Amiga-side file. The Amiga
   never waits for the host.

### 4.3 The report

`S:FirstBoot.log`, appended by the dispatcher, one line per event:

```
art-firstboot 1
system PiStorm RPi4 kick 3.2
step 10-hardware started
step 10-hardware ok
step 20-aux started
step 20-aux skipped not-3.9
step 50-pkg-boingbag-39-1 started
step 50-pkg-boingbag-39-1 refused rc=20
step 90-prefs started
step 90-prefs ok
done partial
```

The first line is a format version so a later ART can read an older card.
Endings stay distinct and the screen may not collapse them:

| Evidence | What happened | The screen says |
|---|---|---|
| `ok` | ran, deleted | done |
| `skipped <reason>` | the step's own condition did not hold | skipped, with the reason |
| `refused rc=N` | the program the step ran returned an error; the step stays | refused; it will be retried on the next boot |
| `started` with no `ok`/`skipped`/`refused` after it | began and the machine stopped or hung | unfinished — watch the screen next boot |
| `done all` / `done partial` | the directory is empty / is not | |
| no file | the card has not booted, or `User-Startup` never ran | **"not booted yet"** — never "failed" |
| a line the parser does not know | an ART newer than this one wrote it | carried as `unknown`, shown verbatim |

### 4.4 The fixed steps

- **`10-hardware`** — under UAE writes `skipped uae` and returns. On PiStorm:
  copy `Storage/DOSDrivers/SD0pi4` or `SD0pi3` to `DEVS:DOSDrivers/SD0` by
  `$ART_RpiType`, delete both variants from `Storage/`; if `L:fat95` is
  absent write `skipped no-fat95` first and do none of it. Then `Mount SD0:`
  explicitly (the glob has passed) and `Assign EMU68BOOT: SD0:`. `HDToolBox`
  icon selection is **not** in this step: ART places no `HDToolBoxPi3/Pi4.info`
  variants today (§2, out of scope).
- **`20-aux`** — `IF $ART_Kick EQ 3.9`: rename `_AUX`/`_AUX.info` to
  `AUX`/`AUX.info` under `Storage/DOSDrivers`, else `skipped not-3.9`. This
  is the reserved-name case `core/preload/amiga_names.rs` already documents;
  the Amiga's own `Rename` is the one tool that can produce that name.
- **`30-datatypes`** — for each of `akPNG akGIF akJFIF akTIFF`: if
  `SYS:Classes/DataTypes/<name>-040.pch` exists and `C:spatch` exists, patch
  to `.new`, replace, delete the patch. With no `.pch` at all: `skipped
  no-patches`. ART ships no patches; the step exists because the mechanism
  should be proven on a step that is a no-op on every tree ART builds today,
  and because a user who adds akDatatypes gets the patch for free.

---

## 5. The wizard (family B)

### 5.1 `90-prefs`, generated

A banner under `IF $ART_FirstBootBanner EQ TRUE` — *"ART first boot —
detected: PiStorm on RPi4, Kickstart 3.2"* — then, for each window the plan
listed, a sentence saying which window opens next and that Save closes it,
`C:Ask "Press RETURN to continue"`, then the program: `SYS:Prefs/Locale`,
`SYS:Prefs/Input`. `ScreenMode` is not opened here: a mode change with a
console open on the Workbench screen cannot apply (Hatcher's `FirstBootWB`
comment says why), so when ScreenMode is to be asked the step copies
`ART-FirstBootWB` into `WBStartup/` and sets `ENV:ART/Reboot`.

### 5.2 `ART-FirstBootWB`, fixed

Second boot, from `WBStartup`: one sentence, `Ask`, `Run <NIL: >NIL:
SYS:Prefs/ScreenMode` detached, delete itself and its `.info`, `EndCLI`. Its
outcome is not in the report — it runs after the dispatcher has finished and
the Workbench is up — and the report says so with `screenmode deferred to
WBStartup` on the line before `done`.

### 5.3 "Only what nobody answered"

`AskPrefs { locale: bool, input: bool, screenmode: bool }` is computed on
the host and passed in. A field is `true` when the user ticked the checkbox
**and** ART did not write that preference during this build:

| Window | ART writes it when | Source of "written" |
|---|---|---|
| Input | a keymap was chosen in the install request (ART-226) | `commands/osinstall.rs`'s result |
| ScreenMode | the Appearance panel wrote a depth (round 1) | `AppearancePanel`'s write result |
| Locale | never | always asked when the box is ticked |

"Written" is a value the build **produces** that a later step **consumes**,
so it lives in the build-session facade (`buildSession.ts`,
`written: { keymap, screenmode }`), set from the two write commands'
results — CLAUDE.md's one-question test for the facade, answered yes. It is
not derived from which `.prefs` files exist in the tree (§1.5: the releases
ship different sets).

---

## 6. Package actions (family C)

### 6.1 Declared in the recipe, as data

```json
"firstboot": { "kind": "run",    "command": "…", "needs_volume": "AmigaOS3.9" }
"firstboot": { "kind": "unpack", "to": "SYS:" }
```

`run.command` is the same command line the recipe's `amiga_installable`
already carries for `core/amigainstall`; the recipe holds one command, not
two. A recipe without `firstboot` offers no first-boot action and the
checklist says so. The three shipped recipes gain: `boingbag-39-1` and
`boingbag-39-2` a `run` with `needs_volume: "AmigaOS3.9"`;
`locale-turkish` an `unpack` to `SYS:`.

### 6.2 `50-pkg-<id>`, generated per package

- `run`: `IF EXISTS SYS:ART/FirstBoot/Volumes/<vol>` → `Assign <vol>:
  SYS:ART/FirstBoot/Volumes/<vol>` (an assign satisfies a volume request);
  `CD SYS:ART/FirstBoot/Packages/<id>`; the command; `IF WARN` → return the
  code; on success delete the package directory. Under UAE the step runs
  exactly the same — that is what the rehearsal is for.
- `unpack`: pick `C:xadUnFile` else `C:LhA`, extract the archive into `to`,
  and on success delete the archive. The plan has already refused when
  neither tool is in the tree, so the step does not re-check. **The exact
  argument syntax of both tools is not asserted here**: phase 4 reads each
  tool's own usage text on the tree (`xadUnFile ?`, `LhA` with no arguments)
  and pins what it read in the generated step's test.

### 6.3 What a `needs_volume` package finds — **a spike, not a design decision**

`Updater` verifies the AmigaOS 3.9 CD and, without it, stops on `Please
insert volume AmigaOS3.9` (measured and screenshotted on 2026-08-21). A real
A1200 with a PiStorm has no CD. ART will copy **something** from the user's
ISO into `ART/FirstBoot/Volumes/AmigaOS3.9/` and assign it — but what
`Updater` reads is unknown, and this spec does not guess. Phase 1 of the
plan runs `Updater` under WinUAE through `core/amigainstall` with the ISO
replaced by a directory assign, narrowing the directory until the check
fails, and writes the answer as a table. "All 470 MB" is a valid answer; in
that case the BoingBag `run` step is offered only when the card has the
room, and the preview says how much.

---

## 7. The Rust core: `core/firstboot/`

```
core/firstboot/
  mod.rs        FirstBootRequest, FirstBootPlan, AskPrefs, the module doc
  scripts/      dispatcher + fixed steps + SD0 mountlists + FirstBootWB, include_str!
  plan.rs       Request → Plan, every refusal decided before a byte is written
  write.rs      Plan → tree: atomic_write + .uaem, User-Startup via merge_user_startup
  packages.rs   recipe firstboot action → 50-pkg-<id> text + files to carry
  report.rs     S:FirstBoot.log → FirstBootReport
```

**`FirstBootRequest`** is what the command layer knows and the core does not
ask for: tree root, release, `AskPrefs`, the selected packages with their
archive paths, the 3.9 ISO path (optional). The core decides nothing about
the user; it builds.

**`plan()` refuses first**, typed, and names what fixes it:

| Refusal | When | Names |
|---|---|---|
| `FirstBootHookUnreachable` | `S/Startup-Sequence` has no `User-Startup` call | the file |
| `FirstBootNeedsDisc { package, volume }` | a `run` with `needs_volume` and no ISO given | the volume |
| `FirstBootNeedsArchiver { package }` | an `unpack` and neither `C/xadUnFile` nor `C/LhA` in the tree | Aminet `util/arc/lha.run` |

`L:fat95` absent is **not** a refusal: the plan carries
`fat_mount: Unavailable("fat95")`, the preview shows it with the Aminet id,
and the step skips. A missing handler should not block a build that never
needed the FAT partition.

The plan also carries `bytes_added` (package wrappers, archives, the volume
directory) for the card step's free-space arithmetic; that is information,
not a refusal.

**`write()`** is idempotent on an untouched tree — writing twice yields the
same bytes — and is the only writer. `User-Startup` goes through
`merge_user_startup(existing, "art-firstboot", …)`; every other file through
`atomic_write`; the command layer wraps `User-Startup` in `guarded_write`
with `BackupPolicy::CONFIG` because that file is one a user hand-edits.

**Layering.** `core/firstboot` reads `core::osinstall::recipe` types and
`core::osinstall::startup::merge_user_startup`; `core/osinstall` never
imports `core::firstboot`. Joining a build session to a plan is
`commands/firstboot.rs`'s job — the `core/artwork` ↔ `core/gameindex` rule.
Nothing in `core/firstboot` names WinUAE; the rehearsal lives beside
`core/amigainstall` and takes the tree copy's directory.

---

## 8. The rehearsal under WinUAE

Built on `core/amigainstall`'s pieces (`stage.rs` for the copy, `run.rs`
for launch/poll/terminate, `core/winuae::DirMount`) with one difference:
**the tree copy is the boot device** at the highest `bootpri`, not ART's own
work volume, so the chain that runs is the one a real machine runs —
release `Startup-Sequence` → `User-Startup` → block → dispatcher. The host
polls `<copy>/S/FirstBoot.log`; a write into a `filesystem2=rw` directory
mount is visible on the host while the emulator runs (measured 2026-08-20).
`done` terminates the emulator; the deadline is amigainstall's 30 minutes,
with its reason.

Four endings, four sentences, four next steps — `AmigaInstallPanel`'s
contract reused:

| Ending | Evidence | The copy |
|---|---|---|
| finished | `done all` | deleted |
| a step refused | a `refused` line | **kept**, path shown |
| deadline | no `done`, window still open | kept |
| the owner closed the window | pid gone, no `done` | kept |

**What it proves and does not.** Under UAE `10-hardware` writes `skipped
uae` and copies nothing; the rehearsal proves the mechanism, the wizard and
both package actions, and **does not prove the Pi3/Pi4 branch**. That branch
closes only on a real card (§10). The rehearsal is `ReadOnly` towards the
user's tree and promotes nothing — installing under WinUAE for real is
`core/amigainstall`'s existing job, not this one.

---

## 9. The screen

A new OS Builder step, `/os-builder/ilk-acilis`, between `amiga-kurulum` and
`kart`; `buildSteps.ts` lists it for the Install kind only (a card-only build
has no tree). In §92 order:

1. **Preview**, read-only, `firstboot_preview`: the steps that will be
   written, one row each; every package with its action; the windows the
   wizard will open and, in one line, why the others will not. Refusals are
   rendered **above the button**, in their own words — Rust's English
   verbatim (ART-060) plus a translated line saying what was *not* done —
   and each names its fix, so a refusal one download away never looks like a
   dead end. The `fat95` line is shown here with its Aminet id.
2. **One checkbox**: *"Ask the remaining preferences on the Amiga at first
   boot"*, default on, `useRemembered`.
3. **Write to tree**, `Safe`: `firstboot_write`; the result card lists what
   was written, where the `User-Startup` backup went, and by how many bytes
   the card will grow. `firstboot.written` enters the build-session facade —
   the card step reads "this tree carries a first boot", and that pair is
   exactly ART-197's shape.
4. **Rehearse in WinUAE**, a job with progress and Stop, four endings as four
   sentences.

**The card-reading screen** gains a report panel: when the card carries
`art-firstboot.log` (FAT partition first — Windows itself can show it —
then the Amiga volume's `S/FirstBoot.log`; where both exist **the Amiga
volume's wins**, because the FAT copy is secondary), it renders §4.3's table;
when neither exists it says *"this card has not been booted on an Amiga
yet"* and nothing stronger.

Beginner mode hides the AmigaDOS lines, file names and `rc=` numbers, and
disables nothing. Every string in both catalogues.

---

## 10. Proof

### 10.1 Tests, by layer

- `core/firstboot`: each fixed script pinned exact-match (mutation: change one
  line, watch it fall); `plan` asserts **which** refusal, never `is_err()`;
  `write` twice is byte-identical; a `User-Startup` with the owner's own
  lines and another component's block keeps every byte outside
  `art-firstboot`; `report` has one test per ending in §4.3 and one each for
  an empty, truncated and unknown-line file; `core::ScratchDir`, counter-named.
- `commands/firstboot`: the wire pinned in both directions (the `#[serde
  (rename_all)]` struct-variant trap `PackagePanel` met).
- Frontend: `buildSteps` pure tests; the panel in jsdom with the four
  rehearsal endings asserted as **four distinct sentences**; the checkbox
  remembered; i18n parity by the existing suite.
- Sweeps: control-byte (the scripts are plain text and no Windows path passes
  through a heredoc), scratch-root, contrast.

### 10.2 Real material, `#[ignore]`d and gated

| Proof | Gate | Closes |
|---|---|---|
| Rehearsal on the owner's 3.2 and 3.9 trees with their Kickstart | `ART_FIRSTBOOT_TREE`, `ART_FIRSTBOOT_ROM` | mechanism, wizard, `unpack` |
| BoingBag `run` under rehearsal with the ISO subset from §6.3 | + `ART_FIRSTBOOT_ISO` | `run`, and the spike's answer |
| A real card in a real A1200, both Pi families if both exist | by hand: insert the card afterwards, read `art-firstboot.log` | `10-hardware`'s Pi branch |

The third row is done by a person and the report names the Amiga, the Pi
and the log. Until it is done the hardware step's row stays 🟡 in
FEATURES.md and says why.

---

## 11. Phases, each closed by its own measurement

1. **Spike** — what `Updater` reads from the CD (§6.3). Output: a table, in
   this round's `sdd/` folder. No code kept.
2. **Framework + hardware (A)** — `core/firstboot` minus `90-prefs` and
   `50-pkg-*`, the report, the rehearsal, the OS Builder step, the card
   panel. Closes when the rehearsal on the owner's 3.2 tree reports `done
   all` with `10-hardware skipped uae`, `20-aux skipped not-3.9`,
   `30-datatypes skipped no-patches`.
3. **Wizard (B)** — `90-prefs`, `ART-FirstBootWB`, `AskPrefs` and the
   facade's `written`. Closes when a rehearsal opens exactly the windows the
   preview listed and the report says so.
4. **Packages (C)** — `unpack` first (the Turkish pack; the booted tree lists
   a `türkçe` drawer where the host holds `t<U+FFFD>rk<U+FFFD>e` — ART-168's
   real answer), then `run` per the spike. Closes on the rehearsal's report
   and, for `run`, on the tree answering `Workbench 45.3`.
5. **Real card** — §10.2 row three.

A phase that does not close leaves its rows 🟡 with the reason written; the
round is not reported complete over it.

---

## 12. Documents this round touches

- This spec; a plan in `docs/superpowers/plans/`; task reports and the spike
  table under `.superpowers/sdd/2026-09-07-firstboot/`.
- ISSUES.md: ART-166 gains a note that the Amiga-side path now has two
  vehicles (WinUAE and first boot) and stays open as the host-placement
  entry it is; ART-168 gains the `unpack` link; any defect found gets its
  `ART-NNN`.
- The teardown note gets a dated correction under its existing correction
  section: "unlocks ART-166" was stale when written, because
  `core/amigainstall` already existed.
- FEATURES.md, STATUS.md, CHANGELOG.md, session-log.md when the round lands.

---

## 13. Sources

- `rootrootde/emu68hatcher` @ `3f38b22`, files in §1.1, fetched 2026-09-06.
- `E:/amiga/ProjeART/art205-32/S/Startup-sequence`,
  `E:/amiga/ProjeART/art159-boot/S/Startup-Sequence` — the owner's own trees.
- `src-tauri/src/core/amigainstall/{mod,workvol}.rs` — the result-file
  contract and the `FailAt` value; `core/osinstall/startup.rs` —
  `merge_user_startup`; `core/amiganet/seed.rs` — the precedent for "a
  module, not a component" (§7); `core/pistorm/hardware.rs` — the
  `brcm-*.device` rule; `core/sources/bundle/catalogue/dosya-sistemi.json`
  — `fat95`; `src/lib/buildSession.ts` — the facade and its keys.
- docs/ISSUES.md ART-166, ART-168, ART-188..193; docs/FEATURES.md rows 296–299.
