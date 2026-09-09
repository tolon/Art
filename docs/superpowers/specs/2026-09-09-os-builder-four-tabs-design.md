# The OS Builder as four tabs — Amiga files · What to install · Kickstart and destination · Build

**Date:** 2026-09-09
**Status:** design, approved by the owner in chat the same day ("Böyle daha temiz") over the
clickable mock-up <https://claude.ai/code/artifact/82e919d2-27b3-468b-8323-8655231d1b23>.
Supersedes rounds **B** and **C** of
[`2026-09-08-os-builder-simplification-design.md`](2026-09-08-os-builder-simplification-design.md);
round **A** of that design (the readout first, branch `art-simplify-a-readout-first`) is the
base this builds on and round **D** (the commander's columns) is untouched by it.

**The trigger.** The owner drove the 0.9.1 build of the source step on 2026-09-09 and sent three
screenshots. The step scrolled through: the readout, the folder list, a two-line sentence per
ISO in three folders (game CDs included), the Kickstart, the destination, the components, *what
this would replace*, the plan, then the packages checklist with a red refusal, then the update
chain with its own tree picker and its own replace list, then the run button. Their words:
*"normalde 4 ekran görüntüsüne anca sığacak"*, *"bu iş basit olmalı, git gide daha karmaşık
rezalet bir hale döndü"*, *"böyle bir şey hayatta kullanmam"*. Measured against the tree
(§ 1.1): the step renders the whole wizard, and steps 3 and 4 render two of its panels again.

**Two decisions the owner made in chat before this was written, and this design does not
reopen:**

1. **Only the tree lane changes.** The card lane (`hedef · kart`) and the volumes lane
   (`hedef · birimler`) stay exactly as they are; the *what are we building* entry stays.
2. **The Amiga-side emulator run and the first-boot block leave the wizard.** A row that
   cannot be placed from Windows is a closed row with one sentence; first boot is one tick;
   the WinUAE run becomes a section of the WinUAE studio, and the wizard does not know it.

---

## 1. What was measured before this was designed

CLAUDE.md's *research before design* rule. Counted on 2026-09-09 on branch
`art-simplify-a-readout-first` at `db59c78`; the full inventory with line numbers is
`.superpowers/sdd/2026-09-09-four-tabs/inventory.md` (local-only).

### 1.1 One step renders the whole wizard

`OsInstall.tsx` (2 572 lines) renders, top to bottom: `source-columns` (the readout, the folder
column, and inside the folder column the `media-identity` wall), the Amiga Forever ROM offer,
components, the conditional-off confirmations, refusals, *what this would replace*, the plan,
the keymap, run, result — **and then mounts `<PackagePanel>` at line 2540 and
`<AmigaInstallPanel>` at line 2562.** `StepPaketler` and `StepAmigaKurulum` mount those same
two panels again on their own routes. So the five-step install lane shows the packages
checklist twice and the chain screen twice, and step 2 alone is every question the lane asks.

### 1.2 The identity wall is one line per file over every folder, ungated

`OsInstall.tsx:985-1054` runs `osinstall_identify_media` sequentially over every material
folder and `media-identity` (1829) renders one `MediaIdentityLine` per file — five kinds:
confirmed · unconfirmed · not-in-table · unreadable · skipped — with no gate but "at least one
folder" (stated at 1824-1827: deliberately not Power-mode gated). Over the owner's three
folders that is 45 files, 23 of them game and CD32 discs, each with a two-line sentence saying
ART did not touch it. **The pass itself is load-bearing**: it fills the hash cache the readout
reads (`identifiedPass`, fix round 1 F2). The lines are not.

### 1.3 The two update screens overlap, and one of them refuses what the other resolves

`PackagePanel.tsx` (660) and `AmigaInstallPanel.tsx` (2 131) both render a package list over
`osinstall_packages` / `osinstall_chain`, both own a tree picker, both use `useHostPlacement`.
The red refusal in the owner's screenshot — *more than one archive carries 'BoingBag3.9-1'* —
comes from `osinstall_collisions` (`commands/osinstall.rs:2361`), which **does** take
`overrides: Option<Vec<(String, PathBuf)>>` (line 2365), exactly as `osinstall_add_package`
does (2582). `AmigaInstallPanel` passes the slot overrides; `PackagePanel` passes none. ART-288
fixed the add path; the preview path on the packages step was never given the user's choice.
That is [ART-289](../../ISSUES.md), and § 3.2 is what closes it: there is one list and it
always passes the file the readout resolved.

### 1.4 The Rust side already answers every question the four tabs ask

`osinstall_slots` (the readout), `osinstall_components`, `osinstall_packages`,
`osinstall_chain` (`rows_for(release, manifest: Option<&DistributionManifest>, slots)` — a
destination that is not yet a tree is a legal input), `osinstall_plan`,
`osinstall_component_collisions`, `osinstall_collisions`, `osinstall_apply` (job),
`osinstall_add_package` (job), `osinstall_describe_tree`, `osinstall_destination_taken`,
`firstboot_preview`, `firstboot_write`. **No new command is needed and none is added.**

### 1.5 What the session already carries

`buildSession.ts`: `kind`, `release`, `material.folders` (per release), `rom.path` (global,
on purpose — a Kickstart is the machine's), `tree.{root, builtHere}`, `components.{chosen,
excludedConditional}` (per release), `packages.{folder, chosen}` (per release), `card`,
`firstboot.written`. Panel-owned remembered keys the tabs still read: `osinstall.keymap`,
`osinstall.destination` (per release), `osinstall.reuseScan`, `amigaInstall.archive.<pkg>`
(ART-277's per-package overrides). Nothing here is deleted or renamed; § 5 says which tab reads
which.

### 1.6 How Emu68 Hatcher does it (read 2026-09-09, <https://rootrootde.github.io/emu68hatcher/usage/>)

Six tabs, one job each, one button: **Start** (tool check) · **Amiga Files** ("Pick the
Workbench version to install", Add… directories of ROMs and ADFs, "optionally review detection
details") · **Emu68** · **Software** ("Enable/disable optional packages and pick the icon set")
· **Output** · **Partitions** · then **Build image** at the bottom, progress in a dialog, logs to
`buildlog.txt`, Save/Load config bottom-left. A standard build is seven or eight actions.

What ART takes: one question per tab; the detection detail optional and folded; one Build
button that is always in the same place; the log a file, not a screen. What ART keeps that
Hatcher lacks (intake research § 3): four distinct endings, the per-slot readout with *how*
matched, order enforced and named in the refusal, nothing downloaded.

### 1.7 The tests that exist

`OsInstall.test.tsx` 3 932 lines, `AmigaInstallPanel.test.tsx` 2 449, `PackagePanel.test.tsx`
735, `MaterialReadout.test.tsx` 658, `FirstBootPanel.test.tsx` 510, `MaterialFolders.test.tsx`
286, `steps.test.tsx` 275, `buildSteps.test.ts` 153. **Seven thousand lines of behaviour that a
rewrite loses by omission** — § 7 is the rule for moving them.

---

## 2. The lane

```
/os-builder/hedef      Ne yapıyoruz            the entry, not numbered — unchanged
/os-builder/dosyalar   1. Amiga dosyaları      release, folders, the readout
/os-builder/secim      2. Ne kurulacak         one tick list: parts, updates, first boot
/os-builder/makine     3. Kickstart ve hedef   Kickstart, keymap, destination
/os-builder/derle      4. Derle                the summary, one button, the run, the endings
```

`STEP_IDS` becomes `hedef · dosyalar · secim · makine · derle · kart · birimler`. `stepsFor`:
`install` → `hedef, dosyalar, secim, makine, derle`; the other three kinds unchanged. The
strip numbers from the second entry and draws `hedef` as a leading chip with the kind's name
(the 2026-09-08 design § 2.1, unchanged). Paths stay untranslated Turkish (`buildSteps.ts:24`).

**Redirects, never 404s:** `kaynak → dosyalar`, `paketler → secim`, `amiga-kurulum → secim`,
`ilk-acilis → secim`, each a `<Navigate replace>` in `App.tsx`. The retired ids leave
`STEP_IDS`. `builtin.rs::route::OS_BUILDER` is `/os-builder` and is unaffected — re-grep it in
the round, as the 2026-09-08 design § 9.7 asks.

**The Build button is on every tab.** A fixed bar under the four tabs shows the destination,
the one-line size of the work (*1 980 dosya + 4 güncelleme + ilk açılış*) and **Derle**; on
tabs 1-3 pressing it navigates to `derle` and starts nothing — a run starts only from the
button on tab 4, once, knowingly. `readiness()` keeps its three answers; a tab that cannot
proceed says why in that bar rather than in a banner above the content (ART-199's
`wrong-folder` sentence moves there).

---

## 3. The four tabs

### 3.1 Amiga dosyaları

What round A built, with the wall folded. The release select above; the readout
(`MaterialReadout`) first in the DOM and the folder column (`MaterialFolders`) beside it,
exactly as merged in round A; the `material-ask` sentence with no folder.

**The identity wall becomes one collapsed line** at the foot of the readout column:
*Ayrıntılar: klasörlerdeki 45 dosya içeriğine göre ne (22'si önceki taramadan)* — a `<details>`
closed by default, holding the same five-kind lines, the summary, the *reuse last scan*
toggle and *Scan again*. **The identification pass still runs** exactly as today, because the
readout's rank-2 and rank-3 rows depend on the cache it fills (§ 1.2). Folding is a change to
what is drawn, not to what is asked. The count in the summary line is the pass's own
(`identitySummary`), never a fixed width, per CLAUDE.md's progress-bar rule.

**The Amiga Forever ROM offer leaves this tab** for tab 3, where the Kickstart field is.

Not on this tab any more: components, refusals, replaces, plan, keymap, destination, run,
result, and the two panels.

### 3.2 Ne kurulacak — one list

Three groups under one heading, one tick column, one order:

1. **Sistemin parçaları** — the component catalogue (`osinstall_components`) exactly as the
   components section draws it today: required rows ticked and disabled with their sentence,
   optional rows ticked or not, the conditional-off confirmation dialog kept (ART-226's other
   half and the `toggleConditional` state machine move verbatim).
2. **Güncellemeler, sırasıyla** — `chain::rows_for(release, manifest, slots)`'s rows in
   `(position, id)` order, one tick per row, the row's own state sentence from `@/lib/chain`
   (`chainLines`) and nothing composed here. `manifest` is the destination's
   `distribution.json` when `osinstall_describe_tree(destination)` says the destination is an
   ART tree (an update run), `None` when it is empty or absent (a fresh build). A row that is
   `Installed` is ticked and disabled with *bu ağaçta zaten var*. A row that is `NotYetRunnable`
   or `Refused{NotPlaceable}` is **unticked, disabled, and says its one sentence** — the
   precedence rule of the 2026-08 design § 3.2, with the Amiga route removed from the wizard:
   there is no route control, because the wizard only has one route. A `BlockedBy` row can be
   ticked; the run orders it after what it needs and refuses if that is unticked, naming both
   (ART-282's typed refusal, before any raw one).
3. **Amiga'da ilk açılışta** — one tick, *Donanımı tanı, ek diskleri ve datatype'ları kur,
   raporu S:FirstBoot.log'a yaz*, defaulting to the remembered `session.firstboot` choice;
   absent means ticked, because the first-boot block is what makes a PiStorm tree boot its own
   hardware (first-boot design § 3). Its preview sentence (`firstboot_preview`) is the row's
   hint.

**Each update row's file is the slot's** (`osinstall_slots` with the ART-277 overrides from
`amigaInstall.archive.<pkg>`), and it is that file's parent that goes to
`osinstall_collisions` and `osinstall_add_package` as `package_folder`, with the `(slot,
path)` pair as `overrides` — the way `AmigaInstallPanel` already does and `PackagePanel` never
did. That is the whole of ART-289's fix; a guard test in § 7 puts the defect back.

**Below the list, one folded line**: *Yerine geçecek: 41 dosyanın yeni sürümü, 3'ü aynı, 0
eski* — `osinstall_component_collisions` + `osinstall_collisions` merged into one count, the
per-file table inside the fold. It is the *what this would replace* section and the chain's
replace list, which today are two lists on one screen, made one and closed.

`PackageChoice.folder` stops being written, as the 2026-08 design § 3.4 said, and stays read
for the seed. `packages.chosen` is this tab's ticks for the update group;
`components.chosen` / `excludedConditional` for the parts group; `firstboot.written` is
unchanged in meaning (it is a fact about the tree, not a tick) — the tick is a new boolean on
the session, `firstboot.wanted`, default absent = wanted.

### 3.3 Kickstart ve hedef

Three fields and nothing else. **Kickstart** (`session.rom`, the Amiga Forever ROM offer
under it, `pistormIdentifyRom`'s sentence under that — moved verbatim from the source step).
**Klavye düzeni** (`osinstall.keymap`, the select the keymap section draws today, with its *no
default* rule). **Hedef klasör** (`osinstall.destination`, with `osinstall_destination_taken`'s
refusal and `osinstall_describe_tree`'s answer: empty → *fresh build*; an ART tree → *this
tree will be updated: AmigaOS 3.9, built 2026-09-08*; anything else → refused with the
sentence it has today).

### 3.4 Derle

Four summary lines computed from what the other tabs hold, then one button, then the run.

```
1 980 dosya, 18,3 MB    · Workbench 3.5 + 3.9 katmanları, Locale, klavyeler, Türkçe fontlar, Euro
4 güncelleme            · BoingBag 3.9-1 → 3.9-2 → Locale update → Türkçe catalogs, bu sırayla
1 ilk açılış adımı      · S:ART-FirstBoot
Yerine geçecek          · 41 yeni sürüm, 3 aynı, 0 eski                          (listeyi aç)

[ Derle ]   E:\amiga\Amigatolon\sonuclar içine yazar. Hiçbir orijinal dosya silinmez.
```

The first line is `osinstall_plan`'s totals (the plan section's own numbers); the plan's
per-file list is the fold behind *listeyi aç*, not a scrolling box. Refusals from the plan
(`osinstall.refusals.heading` today) render **above the button in the button's place** — a
plan that refuses has no Derle button, it has the refusal and the tab it points at.

**The run is one sequence, and it is the only new behaviour in this design:**

```
apply          osinstall_apply(plan, destination)                       → tree written
add package ×N osinstall_add_package(destination, folderOf(file), [id], [(slot,file)])
               one per ticked, Ready, host-placeable update row, chain order
first boot     firstboot_write(destination)                              if ticked
```

Rules, each one a test in § 7:

- **Stop at the first ending that is not success.** The rows after it say *denenmedi* (not
  attempted), the rows before it keep the ending they earned, and the failed row's sentence is
  the core's (`AddPackageResult`'s typed refusal, the job's error text, or *cancelled*).
  Nothing is retried and nothing is undone: every phase leaves the tree in the state the core's
  own safety rules leave it (`atomic_write`, `guarded_write`), which is the reason a stopped
  run is reportable rather than a disaster.
- **Four endings per phase, never fewer**: succeeded · refused (the sentence, the thing to fix)
  · failed (the error, and *the tree is at …, N files were written before this*) · cancelled
  (between phases only — `is_cancelled()` is the job's, and the sequence checks it between
  jobs, never inside one).
- **One report, one line per phase, in order**, with the phase's own count and time (the
  screenshot's *166 dosya, 44'ü zaten aynıydı* is `AddPackageResult`'s own numbers). The report
  stays on the tab until the next run and is also what the operation log (§ 53) records —
  every phase already logs through `write_result`; this design adds no file. The mock-up's
  *ART-build.log* line is **not built**: the operation log is the log.
- **The hand-off line** when every phase succeeded: *Ağaç hazır: E:\… · Kart yazmak için Kart
  hattına geçin*, the link setting `session.kind = "boot-card"` and navigating to `kart`
  (ART-197's last open row).
- **`session.tree.root` becomes the destination after a successful apply** with
  `builtHere: true`, exactly as today; the update phases read it from there.

Progress: the job's own `job-progress` events, filtered by the current job id (the
`InstallProgress` component moves verbatim); between phases the bar says which phase, and
there is no bar for a phase whose job reports no total.

---

## 4. What leaves the wizard, and where it goes

- **`AmigaInstallPanel`** (the WinUAE run of a package's own installer, four endings, refusals,
  settlement line, the follow-up report) is mounted as a section of the WinUAE studio
  (`src/pages/WinuaeStudio.tsx`), headed *Paket kurucusunu WinUAE'de koştur*, with its own tree
  picker as today. Its component and its 2 449 test lines move **unchanged** in this design;
  its chain block (the list it draws above the run) is the one duplicate this design tolerates,
  and it is named as a follow-up rather than trimmed here, because trimming it is the kind of
  work that loses a behaviour by omission (§ 1.7). The `amigaInstall.*` remembered keys stay
  where they are and are read by both the studio section and tab 2 (the overrides).
- **`FirstBootPanel`** (preview → write → rehearse → report) is replaced in the wizard by the
  tick in § 3.2 and the phase in § 3.4. Its **rehearse** (boot a copy under WinUAE) goes to the
  same WinUAE studio section; its **card report** reading stays in the card lane, where it
  already is (`FirstBootReportPanel` under `components/card`).
- **`PackagePanel`** is deleted. Its behaviours — the ticks, the tree list, the
  host-placement preview and report — are tab 2 and tab 4.
- **`OsInstall.tsx`** is deleted. Everything it rendered has a home above; § 7 lists the tests
  by name.

---

## 5. State: nothing changes unless the user changes it

| Key | Today | After |
|---|---|---|
| `buildSession.material.<release>` | tab 1's folders | same |
| `buildSession.release` | the select | same |
| `buildSession.rom` | the Kickstart | same, read on tab 3 |
| `buildSession.components.<release>` | parts ticks | same, tab 2 group 1 |
| `buildSession.packages.<release>.chosen` | update ticks | same, tab 2 group 2 |
| `buildSession.packages.<release>.folder` | written and read | read only (seed), never written |
| `buildSession.tree` | the tree the panels operate on | the destination once built |
| `buildSession.firstboot.written` | a fact about the tree | same |
| `buildSession.firstboot.wanted` | — | new, `boolean`, absent = ticked; `isBoolean` guard in `FIRSTBOOT_SPEC` |
| `osinstall.destination.<release>`, `osinstall.keymap.<release>`, `osinstall.reuseScan` | the source step | tab 3, tab 3, tab 1's fold |
| `amigaInstall.archive/overlay/medium.<pkg>` | the Amiga-side panel | tab 2's overrides and the WinUAE studio section |
| `amigaInstall.package` | the Amiga-side panel | the WinUAE studio section |

`useRememberedShape`'s `recallInto` rebuilds field by field, so a `buildSession.firstboot`
written by today's ART keeps `written` and gains nothing until the user touches the tick — the
same migration-by-mechanism the 2026-08 design § 3.4 relied on. No key is deleted, none renamed.

---

## 6. Strings

Two catalogues, one commit, and `src/lib` never renders one. New keys, all under
`osBuilder.tab.*` and `osBuilder.build.*`: the four tab labels, the bar's three sentences, the
group headings, the folded lines' summaries, the phase names and the four ending sentences per
phase, the hand-off line. Retired keys (the three step labels, `osinstall.heading`/`intro` and
every key only `OsInstall.tsx` and `PackagePanel.tsx` rendered) are deleted in the same commit
that deletes their last renderer; the parity test counts keys, and a key with no renderer is a
sentence nobody can see. Count them in the round rather than quoting a number here.

The Turkish sentences are written by hand, not translated from the English, and read on screen
by the owner (ART-062's standing gap).

---

## 7. Tests, and the mutation that proves each guard

**The rule for the seven thousand lines**: every `it(...)` in `OsInstall.test.tsx`,
`PackagePanel.test.tsx`, `FirstBootPanel.test.tsx` and `steps.test.tsx` is listed **by name** in
the round's report with one of three fates — *moved to `<file>`*, *covered by `<new test
name>`* (where the behaviour survives but the DOM changed), or *dropped: `<behaviour>`, because
`<reason>`*. A case with no fate is a behaviour lost silently, which is the failure this project
is most afraid of. `AmigaInstallPanel.test.tsx` moves unchanged with its component.

| Guard | Test | The mutation that must fail it |
|---|---|---|
| The install lane is four numbered tabs | `buildSteps.test.ts`: `stepsFor("install")` is literally `["hedef","dosyalar","secim","makine","derle"]`; the other three kinds literal | add a fifth id |
| A retired URL lands on its tab | router test: `/os-builder/paketler` renders the `secim` tab's heading, not the home screen | delete the redirect |
| The wall is folded and still asked | `FilesTab.test.tsx`: `osinstall_identify_media` called once per folder; the `<details>` is closed; its summary carries the pass's count | skip the pass when the fold is closed |
| One list, material order | `ChoiceTab.test.tsx`: nine update rows from a fixture in `(position, id)` order, rank 4 twice | sort by id |
| A closed row says why | same: `NotYetRunnable` and `Refused{NotPlaceable}` rows are unticked, disabled, and carry `chainLines`' sentence | enable the row |
| ART-289: the preview gets the user's file | `ChoiceTab.test.tsx`: with two BB1 candidates and an `amigaInstall.archive.boingbag-39-1` override, `osinstall_collisions` is called with that `(slot, path)` in `overrides` | drop `overrides` from the call |
| The run stops at the first non-success | `useBuildRun.test.tsx`: apply succeeds, package 1 refuses, package 2 is never called, the report has three lines reading succeeded / refused / not attempted | continue past a refusal |
| Four endings per phase | same: refused, failed, cancelled and succeeded each produce their own sentence key; a failed phase names the destination | map failed and refused to one key |
| Cancelled only between phases | same: a cancel during phase 1 lets phase 1's job finish or cancel itself, and phase 2 is not started | check the flag inside the loop only after starting the next job |
| First boot is one tick with a remembered default | `ChoiceTab.test.tsx` + `useBuildSession.test.ts`: absent `wanted` renders ticked; unticking writes `false`; a stored `false` survives reload; **rendering never writes** | write the default on first render (ART-089) |
| `packages.folder` is never written again | `useBuildSession.test.ts` | write it in the new setter |
| The plan's refusal replaces the button | `BuildTab.test.tsx`: a refusing plan renders the refusal and no `build-run` button | render both |
| The hand-off sets the kind | `BuildTab.test.tsx`: the link writes `session.kind = "boot-card"` | navigate without setting it |
| The WinUAE section still has four endings | `AmigaInstallPanel.test.tsx`, moved unchanged | collapse two |
| Both catalogues in parity | the existing test | add a key to `en` only |

**Survivors are disclosed**, classified as a weak guard or a wrong mutation, and each needs the
opposite answer (testing.md).

**What a person drives, on the release build, before this is called done**: from a clean
destination, with `E:\amiga\Amigatolon\os39` and the two other folders: tab 1 reads in one
screen at 100 % and at 200 %; tab 2 shows BoingBag 1 with the owner's chosen copy and no red
box; tick BoingBag 1, 2, Locale update, Türkçe catalogs; tab 3's three fields; tab 4's four
lines, **one press**, five phase lines in order, the hand-off line; then boot the tree under
WinUAE and ask it `version full` — *ask the artefact*. Then `/os-builder/paketler` typed into
the address bar lands on tab 2. Then `python scripts/osbuilder-strip-check.py` with `pnpm dev`
running, both languages, the numbers into the session log. The transcript goes in
`docs/session-log.md`.

---

## 8. The rounds, in order

Each leaves the tree green and `pnpm tauri build` working. Branch `art-four-tabs`, cut from
`art-simplify-a-readout-first` at `db59c78` so round A's `MaterialFolders` is the base; both
land on `main` in one `--no-ff` merge when the owner has driven § 7.

| # | Round | What it contains |
|---|---|---|
| 1 | **The lane and the empty tabs** | `buildSteps`, routes and redirects, the strip with the `hedef` chip, the fixed bar with a Derle that only navigates, four tab components that render their heading and nothing else; the strip check script's expectations. The old routes still work behind the redirects. |
| 2 | **Tab 1 and tab 3** | The readout and folder column mounted on `dosyalar`; the identity wall folded; Kickstart, keymap, destination on `makine` with their sentences moved verbatim. `OsInstall.tsx` loses those sections. |
| 3 | **Tab 2** | The one list: components group, chain group with `ChoiceTab`'s override passing (ART-289 closed with its guard), first-boot tick, the folded replace count. `PackagePanel` deleted. |
| 4 | **Tab 4 and the run** | `useBuildRun`, the summary, the button, the phase report, the hand-off. `OsInstall.tsx` deleted; `FirstBootPanel` leaves the wizard. |
| 5 | **The WinUAE studio section** | `AmigaInstallPanel` and the rehearse mounted there, tests moved unchanged; `amiga-kurulum` redirect confirmed. |
| 6 | **The test ledger and the person** | The by-name fate list for every retired test file; the mutation table executed; § 7's drive; docs (FEATURES rows, STATUS in place, session log, CHANGELOG, ART-289 to Fixed with its test name). |

Rounds 1 → 4 are sequential and share `buildSteps.ts`, `steps.tsx` and the catalogues; 5 can
run in parallel with 3 or 4 in a worktree. Each round files its own `ART-NNN` for anything
found (ART-289 is the highest on 2026-09-09).

---

## 9. What this design does not do

- **It does not change what the engine builds.** `core/osinstall`, `core/firstboot`,
  `core/amigainstall`, every recipe and every command keep their behaviour. No command is
  added, none changes signature.
- **It does not touch the card lane, the volumes lane, or the `hedef` entry** — including the
  `distro` job's UI inside `StepHedef`.
- **It does not add a "run everything" that reports one outcome.** One button starts a
  sequence whose every phase reports its own ending, and the sequence stops at the first one
  that is not success.
- **It does not write a log file.** The operation log is the log.
- **It does not delete the Amiga-side run**, the measured route (169 s and 138 s on the owner's
  material); it moves it out of the wizard.
- **It does not decrypt anything new and does not reopen the BoingBag ruling.** Host placement
  of BoingBag 1 and 2 is the bb-host round's.
- **It does not write a physical card.**
- **It does not touch the commander's columns** (round D of the 2026-09-08 design).
- **It does not trim `AmigaInstallPanel`'s own chain block** — a named follow-up (§ 4).

---

## 10. Known risks

1. **The by-name test ledger is the whole safety net, and it is tedious.** Seven thousand
   lines; the temptation is to summarise. The rule is one line per `it(...)`, and the round's
   reviewer counts them against `grep -c "^\s*it(" <old file>`.
2. **The run's sequencing is where "it failed" sentences are born.** Bounded on purpose
   (§ 3.4): host-placeable rows only, ready rows only, stop at the first non-success, four
   endings per phase, cancel only between phases. If the owner's drive still reads as one
   outcome for several operations, the fallback is a *Sıradakini koş* button per phase — the
   screen is unchanged either way.
3. **`rows_for` with `manifest = None` is a path nobody has driven from the screen**: today
   the chain screen always has a tree. The fresh-build case must be tested with a fixture and
   driven once on the release build (§ 7).
4. **The bar's Derle on tabs 1-3 navigates; tab 4's starts a run.** Two buttons with one label
   and different effects is a defect waiting to happen; the bar's button carries the label
   *Derle'ye git* on tabs 1-3 and *Derle* only on tab 4, and a test pins the difference.
5. **The identity pass keeps running behind a closed fold**, so a slow folder still costs the
   same time it costs today; only the screen is calmer. If the owner asks for it to run only on
   demand, that is a separate decision with a readout cost (§ 1.2).
6. **A redirect is the migration for URLs, and the workflow catalogue points at
   `/os-builder` only** — verified by grep on 2026-09-08, a one-off; re-run in round 1.

---

## 11. Sources

- The owner's three screenshots of 2026-09-09 15:12-15:14 (Windows 11, the 0.9.1 build with
  round A applied) and their words in chat the same afternoon.
- `.superpowers/sdd/2026-09-09-four-tabs/inventory.md` — every line number in § 1, read from
  the tree at `db59c78` on 2026-09-09.
- <https://rootrootde.github.io/emu68hatcher/usage/> and
  <https://github.com/rootrootde/emu68hatcher> — read 2026-09-09; the six-tab flow in § 1.6.
- `docs/superpowers/specs/2026-09-08-os-builder-simplification-design.md` — the three causes,
  the measurements of 2026-09-08, and rounds A and D; this design replaces its B and C.
- `docs/superpowers/specs/2026-09-08-os-builder-intake-research.md` § 3 — the eight projects
  and what each does with a long flow.
- `docs/superpowers/specs/2026-09-07-firstboot-design.md` § 3 — why first boot is on by
  default for a PiStorm tree.
- `docs/superpowers/plans/2026-09-09-readout-first.md` and its report under
  `.superpowers/sdd/2026-09-09-readout-first/` — round A, the base branch.
- `D:\Projeler\Amiga\CLAUDE.md` — the rules this is written against: the failure that does not
  crash (four endings, actionable refusals, the screen may not out-claim the core), nothing
  changes unless the user changes it, a test is not a guard until the defect has been put back,
  and *a work list decays*.
