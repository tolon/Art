# Three steps, one updates screen, the readout first — and the commander's columns

**Date:** 2026-09-08
**Status:** design. Built on
[`2026-08-21-os-builder-flow-design.md`](2026-08-21-os-builder-flow-design.md) (the wizard's
shape and the owner's five decisions there),
[`2026-09-08-os-builder-intake-design.md`](2026-09-08-os-builder-intake-design.md) (the one
material list, the slots, the readout) and
[`2026-09-08-os-builder-chain-design.md`](2026-09-08-os-builder-chain-design.md) (the chain as
one list). Those three are history — each describes the tree on the day it was written — and
this document states, in § 1, what the tree actually held when it was measured.

**The trigger.** The owner drove the OS Builder again on 2026-09-08 and said *"işletim sistemi
oluşturma adımları hâlâ çok karışık"* — the OS-creation steps are still far too confusing. Asked
which of three causes it was, the answer was **"1 2 3"**: all three.

1. The packages step and the Amiga-side step are two faces of one thing — the Turkish update
   lives in `paketler`, its prerequisite BoingBag 2 in `amiga-kurulum`.
2. The wizard is too long for a distribution tree; three steps should do — **source → what to
   install → build**.
3. *Which folder holds what* is the readout built the same day, and it is not the first thing a
   person sees.

Plus one Files-screen request from the same evening: the commander's columns (Name, Ext, Size,
Date, Attr) configurable from the header — which are shown, how wide — and remembered.

**Two rulings this design is bound by and does not re-open.** BoingBag 1 and 2 are placed from
Windows (`.superpowers/sdd/2026-09-08-intake/bb-host-brief.md`, the owner's reversal of the
2026-08-19 ruling), and ART never writes a physical card (`docs/owner-checklist.md` § 4).

---

## 1. What was measured before this was designed

Under CLAUDE.md's *research before design* rule. Every number below was counted in the tree on
2026-09-08, on branch `art-091-fixes`, not recalled from the documents above.

### 1.1 The wizard is seven declared steps, and the install lane is five

`src/lib/buildSteps.ts::STEP_IDS` holds **seven**, not the eight the 2026-08-21 design listed:
`hedef · kaynak · paketler · amiga-kurulum · ilk-acilis · kart · birimler`. **`bilesenler` and
`ozet` were never built** — the file says so in its own comment, and `ilk-acilis` was added
afterwards by the first-boot round. `stepsFor` gives each kind its own lane:

| Kind | Steps today |
|---|---|
| `install` | `hedef · kaynak · paketler · amiga-kurulum · ilk-acilis` — five |
| `boot-card` | `hedef · kart` |
| `prepare-volumes` | `hedef · birimler` |
| `distro` | `hedef` alone (every profile is `available: false`, §96) |

So the card lane and the volumes lane are already two steps each and are **not** what the owner
is complaining about. The complaint is the install lane, and the correction is to it alone.

### 1.2 `kaynak` is not one question — it is eight sections

`OsInstall.tsx` is **2719 lines** and renders, in one scrolling column on the `kaynak` route:
the material folder list (line 1763), the readout (1954), components (2140), refusals (2318),
replaces (2360), the plan (2449), the keymap (2507), run (2536) and result (2580). The
2026-08-21 design promised to split it in wave 2 and wave 2 split the *panels* out, not this.
**The step that says "one step asks one question" asks eight.**

### 1.3 The readout is below the folder list, not above it

In `OsInstall.tsx` the folder list renders at 1763 and `<MaterialReadout>` at 1954 — the
answer to *"which folder holds what"* sits under roughly two hundred lines of the question.
`MaterialReadout` also returns `null` for an empty folder list (`MaterialReadout.tsx:266`), so
on a first run the step opens on a picker and nothing else.

### 1.4 The two update screens overlap by construction

| | `paketler` (`PackagePanel.tsx`, 662 lines) | `amiga-kurulum` (`AmigaInstallPanel.tsx`, 2267 lines) |
|---|---|---|
| Rows from | `osinstall_packages` (the catalogue) | `osinstall_chain` → `chain::rows_for` |
| Route | host placement, `useHostPlacement` | the emulator (`compose` → `install`) **and** `useHostPlacement` for host-placed rows |
| Tree picker | owns one (`data-testid="tree-picker"`, line 358) | reads `session.tree.root` |
| Folder | one `packageFolder` | one `packageFolder` for dialogs, plus the whole `materialFolders` list for slots |

Both already import `HostPlacement.tsx` — `useHostPlacement`, `HostPlacementPreview`,
`HostPlacementReport`. **The merge is half-done in the tree already**, and the half that is
done is the half the owner is asking to finish. The chain screen also already carries the
package ticks' meaning: `chain::rows_for` returns **nine rows** for AmigaOS 3.9 (rank 4 is
shared by `locale-39` and `locale-39-turkish`), covering everything `paketler` lists.

### 1.5 The chain is Rust's answer and needs nothing new to be one screen

`core/osinstall/chain.rs` is 2121 lines; `rows_for(release, manifest, slots)` already returns
every row with `position`, `package_id`, `slot_id`, `name`, `state` and `sentence_facts`.
`SentenceFacts::runs_on_amiga` is **three-valued on purpose** — `Some(true)` the Amiga,
`Some(false)` placed from Windows, `None` for neither (the CD; Euro-Update, which ships
`host_placement_block: "needs-fixfonts"`). That field is exactly the per-row route control this
design needs, and it exists.

`rows_for` returns an **empty list** when the release declares no chain (fix round 1, m3), so
AmigaOS 3.2 gets no updates block at all rather than an invented one. The merged screen
inherits that for free.

### 1.6 The commander's columns are one CSS literal and five spans

- `src/pages/FileManager.css:407` — `.tc-row { grid-template-columns: minmax(0, 1fr) 4.3em
  10.7em 9.8em 5.3em 4.8em; }`, shared by the header row and every data row so they line up.
- `src/pages/FileManager.css:553` — `.tc-commander-narrow .tc-row` overrides it to
  `minmax(0, 1fr) 3.3em 8em 4.8em`, and 555-557 hide `.tc-cell-date` and `.tc-cell-attr`.
- `TcHeaderRow` (`FileManager.tsx:4959`) renders **five** spans: Name, Ext, Size, Date, Attr.
  Each data row renders the same five.
- **Six tracks, five spans.** The trailing `4.8em` is the actions slot the CSS comment
  describes ("the per-row copy/delete buttons"), and `.tc-cell-actions` (CSS:498) **is used
  nowhere in `FileManager.tsx`** — grep finds the class in the stylesheet only. So every row
  today ends in 4.8 em of reserved blank, and the narrow rule's fourth track is that same
  reserved blank while Date and Attr are hidden. Measured, recorded, and deliberately **not**
  fixed inside this design (§ 6.4).
- `dockLayout.ts::PANE_NARROW_BELOW_EM = 29.6`, measured with `scripts/zoom-check.py` at
  2575×1407, and its own doc records the residual: **the wide row's fixed columns total 34.9 em
  and have already stopped fitting at 29.6 em.** The breakpoint is late, not early.
- The pane font size is a real setting (`settings.paneFontSize`, 10-28 px) and reaches the DOM
  as an inline custom property, `FileManager.tsx:4070` sets `--tc-font-size`. **That is the
  precedent this design follows for the column widths.**
- There is **no** remembered key for columns today, and no context menu anywhere on the Files
  screen — `onContextMenu` appears once (`FileManager.tsx:4842`), for Norton-style right-button
  marking, and it returns early when that setting is off precisely so "ART puts something there
  later".

### 1.7 What the established projects do with a long flow (research § 3, re-read)

- **HstWB Installer** (MIT) does not ask *"which folder"* four times: one per-file readout —
  name, path found, *how* matched, provenance, red if required and yellow if optional — over a
  set fraction like `'Amiga OS 3.9' (3/4)`, and a `readme.txt` dropped in each expected folder.
  It also enforces order three times over, and refuses a run with the missing file named.
  **What ART takes:** the readout comes first, and it is the screen, not a section of one.
- **Emu68 Hatcher** and **Emu68-Imager** (both MIT) run nothing on the Amiga for the BoingBags
  — they decrypt on the host and copy a curated list. That is now ART's default route too
  (the bb-host brief), which is *why* the packages step and the Amiga-side step collapse: with
  host placement the default, they are one list with one Run and a per-row exception.
- **MultibootOS** puts `put AmigaOS3.2 ADFs here.txt` in the folder itself, `REQUIRED`/
  `OPTIONAL` on line 1. ART already writes that file on request (intake design § 3.7).
- **HstWB's endings are what not to copy** — one sentence, "Installation failed", for four
  different outcomes, and no timeout on the emulator wait. ART's four endings stay four
  through every merge below.

### 1.8 The session already carries what a merged screen needs

`src/lib/buildSession.ts` (682 lines) holds `material.folders` (per release), `tree`,
`components`, `packages { folder, chosen }` (per release), `card`, `firstboot`, `rom`,
`release`. `PackageChoice.folder` is *still stored* for one stated reason — *"it keeps its own
value while `PackagePanel` and `AmigaInstallPanel` still take one folder each"* — which is a
condition § 4 removes.

---

## 2. Three steps for a distribution tree (cause 2)

### 2.1 The lane

```
/os-builder/hedef        what are we building        — the entry, not a numbered step
/os-builder/kaynak       1. Kaynak       Source      — the readout, the folders, the release, the ROM
/os-builder/bilesenler   2. Bileşenler   What to install — components, refusals, what would be replaced
/os-builder/kur          3. Kur          Build       — the plan, one confirm, the run, the updates, the ending
```

**Three numbered steps, and `hedef` is not one of them.** It is where the *kind* is chosen, so
it is above the lane rather than inside it: the strip numbers from the second entry
(`stepsFor(kind).slice(1)`) and draws `hedef` as a leading chip carrying the chosen kind's name.
`stepsFor` keeps `hedef` first, because every kind still has it and the router still needs it.
This is the cheapest honest way to answer *"three steps should do"* without inventing a second
mechanism for "which screen am I on".

**`bilesenler` is the segment the 2026-08-21 design reserved and never built.** It is used here
for exactly what it was reserved for, so no reader of that document is sent somewhere else.

### 2.2 What each step holds, and why the split falls there

**1. `kaynak` — Source.** The readout first (§ 5), the material folder list beside it, the
Amiga Forever offer, the release picker, the Kickstart. One question: *what material do you
have, and what did ART make of it?* Nothing here writes anything.

**2. `bilesenler` — What to install.** The components checklist, and the two blocks that are
consequences of it and of nothing else: the refusals list and the *what this would replace*
list. One question: *which parts of the release go in?*

**3. `kur` — Build.** The plan, the keymap choice, one confirmation, Run, the result — and
then, on the same screen, the **AmigaOS 3.9 updates** list (§ 3) and the first-boot block. One
question: *do it, and tell me what happened.*

**Why the updates are on step 3 and not step 2.** They are not a thing that goes *into* the
tree; they are a thing applied *to* a tree that exists. The material enforces this itself —
each BoingBag's `Install` script reads `version.library` off the target — and ART's own
`chain::rows_for` cannot say a row is `installed` without a `distribution.json` to read it
from. Putting the updates on "what to install" would give a person a checklist whose every row
says *blocked* until they had done something on a later screen. The order the steps are in is
the order the material is in.

**Step 3 does not become the ten-section column again**, because only the phase in hand is
open. Before the tree exists: plan, confirm, Run; the updates list is one collapsed line
(*"6 updates ready — they run once the tree is written"*) and first boot likewise. Once the
tree exists: the plan block collapses to a single summary line and the updates list opens.
That is the 2026-08-21 design's own rule — *"a completed step collapses to a single clickable
summary line"* — applied one level in, to a phase rather than a step.

### 2.3 The summary (`ozet`) does not come back as a route

The 2026-08-21 design's step 8 was never built and is not built here. What it was for — *"a
skipped step stays visible"*, the sentence that says no BoingBag was installed and the tree
stays at 45.1 — becomes the **tail of `kur`**: one block, after the run and the updates, that
states what went in, what was skipped and where the folder is. A ninth route whose only job is
to restate the eighth is a step in a design that is trying to have fewer.

The tail also carries the **hand-off line**, which is the last instance of the ART-197 defect
still open across lanes: *"Your tree is at `E:\…\dist-3.9`. To write a card for it, choose
**a boot card**"* — a link that sets `session.kind` and navigates to `/os-builder/kart`. The
card and volumes lanes already read `session.tree.root` (`VerifyAgainstCard`, the volumes
step's own carry), so nothing new has to be wired for the sentence to be true.

### 2.4 The other three kinds — and the card lane is not touched

| Kind | Lane after this design | Change |
|---|---|---|
| `install` | `hedef` + `kaynak · bilesenler · kur` | five steps become three |
| `boot-card` | `hedef` + `kart` | **none** |
| `prepare-volumes` | `hedef` + `birimler` | **none** |
| `distro` | `hedef` alone | **none** |

`CardBuilder.tsx` (1212 lines), `VolumePreload.tsx` (635), `AppearancePanel`, `NetworkPanel`
and `VerifyAgainstCard` keep their routes, their props and their tests. `ilk-acilis` is the one
route that leaves the lane: `FirstBootPanel` becomes the tail block of `kur` and the route
redirects. **Nothing in `core/card`, `core/preload` or `core/firstboot` changes at all** — this
is a front-end restructuring, exactly as the 2026-08-21 design was.

### 2.5 Routes, redirects, and what must not break

`App.tsx`'s child table gains `bilesenler` and `kur`, and **keeps every retired segment as a
redirect** rather than deleting it:

```
paketler        → /os-builder/kur      (Navigate replace)
amiga-kurulum   → /os-builder/kur
ilk-acilis      → /os-builder/kur
```

Three reasons, each a rule this project already holds: a URL that stopped resolving is a link
in the operation log, in a remembered position and in a user's own habit that now goes nowhere;
`builtin.rs::route::OS_BUILDER` is `/os-builder` and the parent route still renders it, so the
workflow catalogue is unaffected either way; and a redirect is the honest form of *"this moved"*
where a 404 is the confident-wrong form.

`STEP_IDS` becomes `hedef · kaynak · bilesenler · kur · kart · birimler` — six. The retired
three are **not** in `STEP_IDS`; a redirect is a route, not a step, and a step id that no strip
ever draws is dead weight the next reader has to eliminate again.

`readiness()` keeps its shape and its `wrong-folder` answer (ART-199); its tree-consuming set
becomes `bilesenler` and `kur`. `stepLabelKey` is unchanged; `osBuilder.step.bilesenler` and
`osBuilder.step.kur` join both catalogues in one commit, and the three retired keys are
**deleted in the same commit** because nothing renders them any more and an i18n parity test
counts keys.

`scripts/osbuilder-strip-check.py` measures the strip in a real browser, per language and per
kind, and asserts the link count against `stepsFor(kind)`. It is updated in the same round and
**re-run by a person**, because the Turkish labels run measurably longer than the English and
jsdom does no layout: `Bileşenler` and the folded `hedef` chip are new widths nobody has
measured.

### 2.6 What moves in code

| File | What happens |
|---|---|
| `src/lib/buildSteps.ts` | `STEP_IDS`, `stepsFor`, `readiness`'s step set |
| `src/pages/OsBuilder.tsx` | the strip numbers from `slice(1)`; `hedef` renders as the leading chip |
| `src/App.tsx` | two new child routes, three `Navigate` redirects |
| `src/pages/osbuilder/steps.tsx` | `StepBilesenler`, `StepKur`; `StepPaketler`/`StepAmigaKurulum`/`StepIlkAcilis` removed |
| `src/components/osbuilder/OsInstall.tsx` (2719) | **split three ways** — `SourceStep.tsx`, `ComponentsStep.tsx`, `BuildStep.tsx`, sharing the plan state through the session and one new `useInstallPlan` hook |
| `src/components/osbuilder/FirstBootPanel.tsx` | unchanged component, mounted by `BuildStep` |
| `src/i18n/{en,tr}.json` | two keys added, three removed, in one commit |
| `scripts/osbuilder-strip-check.py` | the expected link counts |

The split of `OsInstall.tsx` is the expensive part and the reason this is its own round. The
plan (`osinstall_plan`) is computed from the material list, the release and the components, and
is read by both `bilesenler` (refusals, replaces) and `kur` (the plan block, the run). It moves
into **one hook, `useInstallPlan(session)`**, so the two steps cannot compute two plans and
disagree — the same reasoning that made `buildSession` a facade rather than two variables
(ART-197). The hook keeps `MaterialReadout`'s cancellation and its primitive-dependency rule
(ART-178/ART-195, measured at 2,149 preview jobs in one session); a fresh array in that
dependency list is how this round would reproduce that defect.

### 2.7 Tests, and the mutation that proves each guard

| Guard | Test | The mutation that must fail it |
|---|---|---|
| The install lane is three numbered steps | `buildSteps.test.ts`: `stepsFor("install")` is `["hedef","kaynak","bilesenler","kur"]` | add `paketler` back to the lane |
| The card lane is untouched | the same test asserts `boot-card`, `prepare-volumes`, `distro` literally | add any step to `boot-card` |
| A retired URL still resolves | `App.test.tsx` (or a router test): rendering `/os-builder/paketler` lands on `kur` | delete the redirect — the test must fail, not render nothing |
| `hedef` is not numbered | `OsBuilder.test.tsx`: the strip's first numbered link reads `1. …` and is `kaynak` | number from `slice(0)` |
| The two steps read one plan | `useInstallPlan.test.tsx`: one `osinstall_plan` call for two mounted consumers | give each step its own effect |
| Step 3 does not open its updates block before a tree exists | `BuildStep.test.tsx`: with `tree.root = null` the updates list renders its collapsed line and no Run | render the list expanded |
| The i18n catalogues stay in parity | the existing parity test | add `osBuilder.step.kur` to `en` only |

**Survivors are disclosed.** The one this round is most likely to produce is the redirect test:
a router test that asserts *"something rendered"* rather than *"the build step rendered"* passes
with the redirect deleted, because the `*` catch-all sends everything to `/`. Assert the
specific step, per *never assert a state that has more than one cause*.

### 2.8 What a person must drive

The owner, on the release build: choose **install**, walk `kaynak → bilesenler → kur` with the
3.9 material at `E:\amiga\Amigatolon\os39`, build a tree, and read the strip in **both**
languages. Then open `/os-builder/paketler` by hand in the address bar and confirm it lands on
the build step rather than on the home screen. Then `python scripts/osbuilder-strip-check.py`
with `pnpm dev` running, and the numbers quoted in the session log.

---

## 3. One screen: **AmigaOS 3.9 updates** (cause 1)

### 3.1 The screen

One block on the build step, headed *AmigaOS 3.9 updates*, whose rows are
`chain::rows_for(release, manifest, slots)` — the nine rows measured in § 1.5, sorted by
`(position, id)`, rank 4 shared. It is the chain screen `AmigaInstallPanel` already renders,
with the `paketler` step's two remaining jobs folded into each row: **the tick** (is this
update part of this build) and **the route** (from Windows, or on the Amiga).

```
AmigaOS 3.9 updates — tree E:\amiga\ProjeART\dist-3.9            ● 3 of 9 applied

 ☑ 1  AmigaOS 3.9 CD          installed · built from AmigaOS39.iso, by hash        → Source
 ☑ 2  BoingBag 3.9-1          installed · placed from Windows
 ☑ 3  BoingBag 3.9-2          ready · BoingBag39-2.lha, by hash   [Windows|Amiga]
 ☑ 4  Locale 3.9              ready · Locale3_9.lha, by hash      [Windows]
 ☑ 4  Turkish locale update   ready · BoingBag39-2-turkce.lha     [Windows]
 ☐ 6  Contribution            ready · …-Contribution.lha          [Windows]
 ☐ 7  Euro-Update             cannot be placed from Windows: needs FixFonts
 ☐ 8  BoingBags 3&4           blocked: 3 first · runs on the Amiga [Amiga]

                                                    [ Run the ticked updates ]
```

- **One list.** The Turkish update and its prerequisite BoingBag 2 are two rows of one list,
  which is the whole of cause 1.
- **One Run.** It walks the ticked rows that are `ready` **and host-placed**, in chain order,
  one at a time, and reports each row's own ending as it lands. It **stops at the first row
  that does not succeed** and says which row and why. Nothing is collapsed: a walk that placed
  three rows and was refused on the fourth says exactly that, and the three that landed are
  named. A walk that placed nothing says *nothing was placed* rather than *it failed*.
- **A row that runs on the Amiga is never in the walk.** It opens a window on the person's
  desktop; that is a thing somebody presses a button for, once, knowingly. Such a row carries
  its own `Run on the Amiga` button, and the screen says before it opens that a window will
  (the sentence `AmigaInstallPanel` already has).
- **Every state and every sentence is `@/lib/chain`'s, which is `chain.rs`'s.** Nothing here
  composes a sentence about a row, and `installed` comes from `distribution.json` and never
  from a file being present. The chain design's eight states, and its
  `blocked-by-component` ninth, all still render.

### 3.2 Host placement is the default; the Amiga is a per-row choice

Precedence, one rule, stated once in `src/lib/chain.ts` and tested there:

1. A row whose package sets `host_placement_block` is **not** host-placeable — the route
   control shows one option and the block's own sentence (Euro-Update: `needs-fixfonts`).
2. A row whose package declares `amiga_installer` and carries `not_yet_runnable` is Amiga-only
   and **disabled** with its own sentence (BoingBags 3&4, measured 2026-09-08: its `Install`
   calls `askoptions`, `confirm` and `(run "SYS:Tools/EditPad …")`, and nobody has measured
   whether it finishes without a person at the window).
3. A row that is **both** placeable and runnable — BoingBag 1 and 2, after the bb-host round
   removed their block — defaults to **Windows** and offers the Amiga as a choice. The
   Amiga-side run is kept, not retired: it is the route that was measured working (169 s and
   138 s on the owner's material, the tree answering `Workbench 45.3`), and a host placement
   that ever disagrees with it needs somewhere to go.
4. A row that is neither is `NotPlaceable` and says which block applies.

The choice is per **row**, per **release**, and remembered (§ 3.4). It is never guessed and
never changed by ART: a row the user set to Amiga stays on Amiga even after the host route
becomes available for it, which is `remembered.ts`'s rule about a setting ART moved.

### 3.3 What happens to `PackagePanel` and `AmigaInstallPanel`

**One component, and the two run-routes become two hooks.** Not a 2900-line merge: the tree
already proved the pattern when `HostPlacement.tsx` was cut out of `PackagePanel` for the chain
screen to reuse — *"two implementations of 'what would this replace' is two answers to one
question, and the one that drifts is the one nobody is looking at."* The Amiga route gets the
same treatment.

| New file | What goes in it | Cut from |
|---|---|---|
| `src/components/osbuilder/UpdatesChain.tsx` | **the screen**: rows, ticks, the route control, one Run, the walk, the per-row report | `AmigaInstallPanel.tsx` (the chain block, 1660-1930) + `PackagePanel.tsx` (the ticks, the catalogue join) |
| `src/components/osbuilder/AmigaRun.tsx` | `useAmigaRun` + `AmigaRunPreview` + `AmigaRunReport` — preview → confirm → job → report, the four endings, the refusals, the settlement line, the follow-up report | `AmigaInstallPanel.tsx` (verbatim; behaviour unchanged) |
| `src/components/osbuilder/SlotField.tsx` | the one field a person acts on when a slot is not found or ambiguous, with its override keys | `AmigaInstallPanel.tsx:259` |
| `src/components/osbuilder/TreePicker.tsx` | the list of trees `osinstall_trees_in` found, plus Browse, plus `describe_tree` validation | `PackagePanel.tsx:358` |
| `src/components/osbuilder/HostPlacement.tsx` | **unchanged** | — |

`PackagePanel.tsx` and `AmigaInstallPanel.tsx` are then **deleted**, not left as thin wrappers:
a wrapper nobody renders is the next reader's puzzle.

**The tests move with the behaviour, and the move is where coverage is lost if anyone is
careless.** `AmigaInstallPanel.test.tsx` is 2669 lines and `PackagePanel.test.tsx` 735; between
them they hold the four endings, every refusal sentence, ART-277's per-package archive scoping,
ART-240's accessible names, the stale-remembered-id row (N4), the blocked-row rules (M3) and
the refused-entry names (m6). Each case is re-homed to `AmigaRun.test.tsx`,
`UpdatesChain.test.tsx` or `SlotField.test.tsx` **by name**, and the round's report lists the
before and after counts per file. A case that has no new home is a behaviour being dropped and
must be named as one.

### 3.4 The session fields that move, and the remembered keys

**`packages.chosen` stays and gains its true meaning**: the rows ticked for this build, per
release, under `buildSession.packages.<release>`. Nothing migrates; it already holds package
ids and the chain rows *are* packages.

**`packages.folder` stops being a question.** Its stated reason for existing was that *"the two
panels take one folder each"*; with one screen, each row's archive is the file the **slot**
resolved, and `osinstall_add_package(treeRoot, folder, ids)` gets that file's own parent
(`folderOf`, which `AmigaInstallPanel.tsx:446` already has). So:

- the field is **no longer written**, and no longer offered as a control;
- `PackageChoice.folder` stays on the type and in `PACKAGE_SPEC`, marked `@deprecated`, and is
  still **read** — the same shape `MediaChoice.folder` already has;
- `seedPackagesFolder`'s three-spelling walk is untouched, so a rollback to an earlier ART
  still finds the folder where that ART looks for it;
- the folder is still in `material.folders` (that is how it got there — `seededMaterial`'s
  fourth source), so nothing the user pointed at stops being scanned.

**One new field: the route.** `PackageChoice` gains

```ts
/** Which route the user chose for a row, by package id. Absent = ART's default,
 *  which is host placement wherever the package allows it. */
route: Record<string, "host" | "amiga">;
```

with its own guard `isRouteMap` in `buildSession.ts` beside `isMaterialFolders`, added to
`PACKAGE_SPEC`. It is per release because it lives inside `buildSession.packages.<release>`,
which is per release for the reason `components` is (ART-209: a package id means something only
inside the recipe that declares it). **Nothing migrates into it** — no key today records a
route — and its absence is ART's default, which is what makes "nothing changes unless the user
changes it" true for a user who never opens the control.

`useRememberedShape` rebuilds field by field (`recallInto`), so a settings file written by
today's ART — `{ folder, chosen }` — keeps both and gains an empty `route`. That is the whole
migration, and it is the mechanism, not a step somebody has to remember.

**The per-package override keys are untouched.** `amigaInstall.archive.<packageId>`,
`amigaInstall.overlay.<packageId>` and `amigaInstall.medium.<packageId>` (ART-277's scoping)
stay exactly where they are, keep their guards, and are still read by `SlotField` and passed to
`osinstall_slots` as `overrides` — which is what makes the readout and the row say the same
thing about one file (round 2, M5). `amigaInstall.kickstart` was already folded into
`session.rom` by `seedRom` and is still read once there.

**Nothing the user chose is lost.** The audit for the round's report, one line per key:

| Key | Today | After |
|---|---|---|
| `buildSession.packages.<release>.chosen` | the packages step's ticks | the updates rows' ticks — same ids |
| `buildSession.packages.<release>.folder` | written and read | read only, never written, never deleted |
| `buildSession.packages.<release>.route` | — | new; absent means ART's default |
| `amigaInstall.archive/overlay/medium.<packageId>` | overrides on the Amiga-side panel | overrides on the one screen |
| `osinstall.packages.folder`, `buildSession.packages` (global) | legacy seeds | still read once by `seedPackagesFolder` |
| `buildSession.tree`, `.rom`, `.material.<release>` | as they are | as they are |

### 3.5 Tests, and the mutation that proves each guard

| Guard | Test | The mutation that must fail it |
|---|---|---|
| The chain is one list, in the material's order | `UpdatesChain.test.tsx`: nine rows from a fixture, in `(position, id)` order, rank 4 twice | sort by id alone |
| Host placement is the default | `chain.test.ts`: a row that is placeable **and** runnable resolves to `host` with no stored route | flip the default to `amiga` |
| A stored route wins over the default | same file, with `route: { "boingbag-39-2": "amiga" }` | read the default unconditionally |
| A blocked row offers no host route | `chain.test.ts` over Euro-Update's `needs-fixfonts` | ignore `host_placement_block` |
| `not_yet_runnable` stays disabled | `UpdatesChain.test.tsx` over BoingBags 3&4 | enable a row whose package carries the flag |
| The walk stops at the first non-success | `UpdatesChain.test.tsx`: three rows, the second refuses — the third is **not** attempted and the report names all three states | continue past a refusal |
| The walk never runs an Amiga row | same file: a ticked Amiga-route row is skipped by Run and keeps its own button | include `runs_on_amiga === true` rows in the walk |
| The four endings stay four | `AmigaRun.test.tsx`, moved verbatim | collapse *timed out* and *window closed* into one sentence |
| A run that did not succeed says where the copy is | `AmigaRun.test.tsx`, moved | drop the settlement line |
| `packages.folder` is never written again | `useBuildSession.test.ts`: after ticking a row and running, the stored section has the same `folder` it had | write `folder` in the new setter |
| A route the user set survives a reload | `useBuildSession.test.ts` | write the default into the store on first render (ART-089's own defect) |

### 3.6 What a person must drive

From a **clean 3.9 tree**, on the release build: tick BoingBag 1, BoingBag 2, Locale 3.9, the
Turkish update and Contribution, leave everything on the default route, press Run **once**, and
read the five endings. Then set BoingBag 2 to *Amiga* on a second tree and run that row alone,
to confirm the route the emulator takes still works and still reports its four endings. Then
boot the first tree and ask it `version full` — *ask the artefact*; a directory name and a
`distribution.json` are each consistent with several answers. Times and endings into the
session log; the host-placed tree compared against the Updater's own snapshots, which is the
bb-host brief's oracle and not re-derived here.

---

## 4. The readout first (cause 3)

### 4.1 What changes on screen

`kaynak` opens on the answer, not on the question. Two columns at a comfortable width, one
column below a narrow one:

```
┌ What ART found ───────────────────────────┐ ┌ Where it looked ──────────────┐
│ AmigaOS 3.9 · everything required is here │ │ E:\amiga\Amigatolon\os39   [×]│
│ ✔ AmigaOS 3.9 CD    AmigaOS39.iso  by hash│ │ E:\amiga\Shared\rom        [×]│
│ ✔ BoingBag 3.9-1    …(1).lha       by hash│ │ [ Add a folder… ]             │
│ ! Locale 3.9        Locale3_9.lha  by name│ │ [ Write a guide into … ]      │
│ ✖ BoingBags 3&4     not found · optional  │ └───────────────────────────────┘
└───────────────────────────────────────────┘
```

The readout is the primary column and comes **first in the DOM**, so it is also first for a
screen reader and first at a narrow width. The folder list, the layer tags, the Amiga Forever
offer and the *unused for this plan* line move to the secondary column beside it.

**With no folders at all, the primary column still speaks.** `MaterialReadout` returns `null`
for an empty list (`MaterialReadout.tsx:266`) and keeps that behaviour — it is right, because a
set line about a release the user has pointed nothing at would be a claim about their disk. The
*step* renders, in the readout's place, the sentence that names the next action and the Amiga
Forever offer beside it. A question nobody has asked is not an answer of "nothing found", and
the two must not share a sentence.

**The guide button block stays with the folders**, because it is an action on a folder. The set
line, the rows, the unreadable-folder lines and the crowded-folder lines stay with the readout.

### 4.2 What moves in code

| File | What happens |
|---|---|
| `src/components/osbuilder/SourceStep.tsx` (from `OsInstall.tsx`) | the two-column layout; the readout mounted **above** the folder list in source order |
| `src/components/osbuilder/MaterialReadout.tsx` | the guide block moves out to the folder column; **nothing else changes** — no new props, no new states, the eleven row endings untouched |
| `src/components/osbuilder/MaterialFolders.tsx` (new, from `OsInstall.tsx:1763-1900`) | the list, the tags, Add, remove, the Amiga Forever offer, the guide buttons |

Inline styles, as the rest of the builder is — this design adds no stylesheet to the OS Builder
and touches no existing one.

### 4.3 Tests, mutations, and what a person drives

| Guard | Test | The mutation that must fail it |
|---|---|---|
| The readout comes first | `SourceStep.test.tsx`: `material-readout` precedes `material-folders` in document order (`compareDocumentPosition`) | swap the two blocks |
| An empty list asks rather than reporting | `SourceStep.test.tsx`: no `material-set-line`, and the ask line is present | render a set line with zero folders |
| The guide buttons still work where they now live | the moved cases from `MaterialReadout.test.tsx` | drop the `create_new` semantics — *already there* must not read as an error |

A person: open `kaynak` on a **fresh profile** (no remembered folders) and confirm the first
thing on the screen is a sentence about what to do, not an empty box; then with the owner's
three folders, and read the top of the screen without scrolling at 100 % and at 200 %
application size.

---

## 5. The commander's columns

### 5.1 What changes on screen

- **Right-click the header row** → a small menu in the chrome colours: a checkbox per optional
  column (Ext · Size · Date · Attr), and **Reset columns**. Name has no checkbox: a listing
  with no names is not a listing.
- **Drag a header border** → the column to the *left* of the grip resizes, live, with a 2 em
  floor. Name has no grip of its own (§ 5.4).
- Nothing else moves. Sorting by clicking a header, the `↓Date` arrow before the label, the
  cursor row, the colours, the density — all exactly as they are.

### 5.2 Shared by both panes, not per pane — and why

**One column set for the whole commander.** Four reasons, in the order they decided it:

1. **The markup says so already.** `.tc-row` is one class carrying the grid template, used by
   both panes' header rows and every data row (`FileManager.css:407`, § 1.6). Per-pane means
   two templates and a per-pane custom property on a class shared by four things.
2. **Total Commander's own columns are a global view setting**, not a per-panel one. The
   owner's standing rule for this screen is that it looks and behaves like the reference.
3. **Two panes are a comparison.** Panes side by side with different columns have stopped
   comparing, and the person most likely to end up there is the one who set it by accident.
4. **Per pane raises "per tab?", which nobody asked.** The Files screen has tabs per pane
   (`@/lib/paneSession`); a per-pane answer would immediately owe a per-tab answer, and a tab
   set is a structure with its own rules — `remembered.ts` says so in its own header.

The cost is real and accepted: someone who wants Date on the left and not on the right cannot
have it. If that is ever asked for, the shape below takes a second key without changing the
model — `files.columns.right` beside `files.columns` — and nothing in the pure layer has to
move.

### 5.3 Widths in `em`, never pixels

`dockLayout.ts` already argues this, measured, for the narrow rule: the listing's text size is a
setting the user turns up (10-28 px), and the pane lives inside `.app-shell`, which carries
`zoom` — *"500 px is roomy at 10 px text and impossible at 28 px"*, and a media query can see
neither zoom. A remembered pixel width would break in exactly the two ways ART-101 and ART-174
already broke.

So: a drag measures pixels, and the value **stored** is
`px / clampPaneFontSize(paneFontPx)`, rounded to one decimal — the same conversion
`paneWidthInEm` already performs, from the same module, so there is one implementation of
"how wide is that in em".

### 5.4 The grid template becomes one custom property

`FileManager.css:407` becomes:

```css
grid-template-columns: var(--tc-columns, minmax(0, 1fr) 4.3em 10.7em 9.8em 5.3em 4.8em);
```

**The default literal is unchanged, character for character.** With nothing remembered, no
`--tc-columns` is set and the screen renders byte-identically to today — which is what *"the
Total Commander look is kept, `FileManager.css` untouched in look"* means in practice. The
commander sets `--tc-columns` inline when a set is stored, exactly the way it already sets
`--tc-font-size` at `FileManager.tsx:4070`.

**The trailing 4.8 em stays.** § 1.6 measured that the sixth track is the actions slot and that
`.tc-cell-actions` is used nowhere — every row ends in reserved blank. It is **not** offered in
the menu, **not** resizable, and **not** removed here: removing it changes the look of every
row, which is the one thing this feature may not do. It is filed as its own issue with the grep
that found it, to be decided on its own.

**Hidden columns are not rendered**, or the grid gains empty tracks. `TcHeaderRow` and the row
`<li>` render their cells from the visible column list rather than from five literal spans.
That is a change to `FileManager.tsx` and to no stylesheet rule.

### 5.5 Narrow width — `dockLayout.ts`'s rules, applied to the user's set

The existing rule stands: below `PANE_NARROW_BELOW_EM = 29.6` the commander carries
`tc-commander-narrow`, and Date and Attr go. What changes is that the rule now has to answer
about a set the user may have altered:

```ts
// src/lib/columns.ts — pure, no DOM, no i18next
export function visibleColumns(
  chosen: ColumnLayout, paneWidthPx: number, paneFontPx: number
): ColumnId[]
export function gridTemplate(ids: ColumnId[], chosen: ColumnLayout): string
```

Three rules, each with a reason:

1. **Narrow hides, it never shrinks.** A user's chosen width is a decision; silently halving it
   because a window moved is ART changing a setting the user did not change. Hiding is
   temporary and reversible by widening the window; a rewritten width is not.
2. **It hides the same two columns it hides today** — Attr, then Date — and no more. The
   threshold is already late (34.9 em of fixed columns at a 29.6 em breakpoint, measured), and
   moving it is a change to *when* the screen degrades, which `dockLayout.ts` explicitly
   declined to make and this design declines with it.
3. **A narrow layout is never written back.** The remembered set is what the user chose, full
   stop. This is ART-089's rule and it gets its own mutation below, because it is the one
   defect here that would be invisible: the user's Date column would quietly disappear for good
   after one narrow session.

`PANE_NARROW_BELOW_EM` and `paneWidthInEm` stay in `dockLayout.ts` and are **imported** by
`columns.ts`. One threshold, one owner; a second copy is how the header row and the data rows
would come to disagree about when to degrade.

### 5.6 The remembered keys

One key, one object, through `useRememberedShape` — **not** `useRemembered` with a whole-object
guard, because `recallInto` rebuilds field by field and a bad `widths` map must cost only the
widths, not the user's shown/hidden choice:

```ts
// src/lib/columns.ts
export const COLUMN_IDS = ["name", "ext", "size", "date", "attr"] as const;

export interface ColumnLayout {
  shown: ColumnId[];                    // isColumnIdList  — order is the row's order
  widthsEm: Record<ColumnId, number>;   // isColumnWidths  — each 2 ≤ w ≤ 40
}

export const COLUMN_SPEC = { shown: isColumnIdList, widthsEm: isColumnWidths };
export const DEFAULT_COLUMNS: ColumnLayout = {
  shown: ["name", "ext", "size", "date", "attr"],
  widthsEm: { ext: 4.3, size: 10.7, date: 9.8, attr: 5.3 },   // FileManager.css:407, unchanged
};
```

Key: **`files.columns`**. There is nothing to migrate — no key exists today (§ 1.6) — and the
key's *absence* is the default, which is what makes the untouched screen identical to today's.
`isColumnWidths` bounds each value the way `isWholeNumberBetween` bounds a font size and for
the same stated reason: *"a remembered font size of -1 or 1e9 is not a preference, it is a
broken screen."* `name` carries no width because it is `minmax(0, 1fr)`; a width for it in the
file is dropped by the guard rather than honoured.

**Reset** is `forget("files.columns")` — the third element `useRemembered` grew for exactly this
distinction — and not "write the defaults". A stored copy of the defaults is itself a decision,
and it would survive a future change to what the defaults are.

### 5.7 The gesture, precisely

- A 5 px `.tc-header-grip` at the right edge of each resizable header cell, `cursor:
  col-resize`, drawn in the existing chrome border colour so it reads as the column divider it
  already looks like.
- `pointerdown` → `setPointerCapture`; `pointermove` → a *local* px delta applied to the live
  template; `pointerup` → one write through `useRememberedShape`. **One write per drag**, not
  one per move: a `settings.json` write per pointer event is the ART-178 shape of defect
  wearing a different hat.
- Floor 2 em per column. A column at zero width is indistinguishable on screen from a hidden
  one, and two states that look the same are one state as far as the user is concerned.
- **Name has no grip.** It is `minmax(0, 1fr)` and takes what is left; "resizing" it means
  fixing it, which changes what the pane does when the window changes size. The grip on Ext's
  right edge resizes Ext, and so on rightwards — each grip resizes the column to its left.
- Keyboard: the header row is focusable, `Shift+F10` and the Menu key open the same menu, and
  the menu's checkboxes are `role="menuitemcheckbox"`. `Escape` closes; a click outside closes;
  focus returns to the header. ART-240's rule holds — each item's accessible name is the
  column's own, never a repeated "Show".

### 5.8 What moves in code

| File | What happens |
|---|---|
| `src/lib/columns.ts` (new) | the model, the guards, `visibleColumns`, `gridTemplate`, `DEFAULT_COLUMNS` — pure, tested without a DOM |
| `src/lib/dockLayout.ts` | nothing moves out; `PANE_NARROW_BELOW_EM` and `paneWidthInEm` gain one importer and a note naming it |
| `src/components/files/ColumnMenu.tsx` (new) | the header context menu |
| `src/components/files/ColumnGrip.tsx` (new) | the drag handle |
| `src/pages/FileManager.tsx` | `TcHeaderRow` (4959) and the data row render from the visible list; `--tc-columns` set inline beside `--tc-font-size` (4070); one `useRememberedShape` |
| `src/pages/FileManager.css` | **one line**, 407, wrapped in `var(--tc-columns, …)` with the literal kept; plus the grip's own rule. `553-557` unchanged |
| `src/i18n/{en,tr}.json` | the menu's labels, both catalogues, one commit |

### 5.9 Tests, and the mutation that proves each guard

| Guard | Test | The mutation that must fail it |
|---|---|---|
| The untouched screen is today's screen | `columns.test.ts`: `gridTemplate(DEFAULT_COLUMNS)` equals the literal at `FileManager.css:407`, **read from the file** | change one default width |
| Hiding a column removes its track | `columns.test.ts`: `shown` without `date` yields a template with one fewer track | leave the track and hide the cell |
| A width is stored in `em`, from the pane's own font | `columns.test.ts` over the px→em conversion at 10, 12 and 28 px | store the px |
| Narrow hides Date and Attr | `columns.test.ts` at 25 em | hide by index rather than by id |
| **Narrow never writes back** | `FileManager.test.tsx`: shrink the pane below the threshold, then read `settings.remembered["files.columns"]` — unchanged | persist `visibleColumns`' answer |
| Reset forgets rather than stores | `FileManager.test.tsx`: after Reset the key is **absent** | write `DEFAULT_COLUMNS` |
| A hand-edited settings file cannot break the screen | `columns.test.ts`: `widthsEm: { date: 1e9 }` and `shown: ["grd"]` both fall back | drop the bounds from `isColumnWidths` |
| A column cannot be dragged to nothing | `columns.test.ts` floor at 2 em | remove the floor |
| Name is not hideable | `ColumnMenu.test.tsx`: no checkbox for Name | offer one |
| The menu is reachable without a mouse | `ColumnMenu.test.tsx`: `Shift+F10` opens it, `Escape` closes it, focus returns | drop the key handler |

`FileManager.css:407` is read **by the test**, not copied into it — *a test that reads a table
instead of the file is a copy, and copies drift*. That is the guard that keeps "the look is
unchanged" a fact rather than an intention.

### 5.10 What a person must drive

The owner, on the release build: hide Ext, widen Date, restart ART and confirm both survived;
turn the listing text up to 28 px and confirm the widened Date is still proportionally the same
width (this is the `em` decision being checked, and it is the one a pixel value would fail);
drag the splitter until the pane is narrow and confirm Date and Attr disappear **and come
back**; press Reset and confirm the screen is byte-for-byte the screen they started with.
Then `python scripts/zoom-check.py --files` and the numbers in the session log, because jsdom
does no layout and this feature is layout.

---

## 6. The rounds, in order, with a rough size

Each leaves the tree green, `pnpm tauri build` working and the application usable. The order is
chosen so that the **riskiest merge is verified on routes that already work**, before any route
moves.

| # | Round | Size | Why here |
|---|---|---|---|
| **A** | **The readout first** (§ 4) | ~half a round. One component split out of `OsInstall.tsx`, one layout, three tests | Cheapest, most visible, and it is the answer to cause 3. It needs nothing from the others and gives the owner something to drive the same day |
| **B** | **One updates screen** (§ 3), landing on the **existing** `/os-builder/amiga-kurulum`, with `paketler` redirecting to it | ~2 rounds. `useAmigaRun` extraction, `UpdatesChain`, `SlotField`, `TreePicker`, two panels deleted, ~3400 lines of tests re-homed, one session field | The merge is the expensive, defect-prone part. Doing it while the routes are still the routes people know means one variable changes at a time — and if the walk or the route precedence is wrong, it is wrong on a screen the owner already knows how to drive |
| **C** | **Three steps** (§ 2) | ~1.5 rounds. `OsInstall.tsx` split three ways, `useInstallPlan`, `buildSteps`, routes and redirects, the strip, the strip check script | Now the updates screen is one block that moves as a block. Splitting `OsInstall` is mechanical once nothing else is in flight |
| **D** | **The columns** (§ 5) | ~1 round. One pure module, two small components, one CSS line, `TcHeaderRow` | Independent of A-C: no shared file. Runs in its own `git worktree` in parallel with any of them |

Rounds A → B → C are sequential and share `OsInstall.tsx`, `steps.tsx` and the i18n
catalogues; **do not run two of them in one tree.** D shares nothing with them and should be a
worktree, per CLAUDE.md's rule about the branch a subagent moved.

Each round files its own `ART-NNN` for anything found (ART-282 was the highest on 2026-09-08 —
check `docs/ISSUES.md` before claiming a number), flips its `docs/FEATURES.md` rows **only with
a test name**, and updates `docs/STATUS.md`'s "Picking up next session" block **in place**.

---

## 7. What this design does not do

- **It does not change what the engine builds.** `core/osinstall`, `core/amigainstall`,
  `core/card`, `core/preload`, `core/firstboot` and every recipe keep their behaviour. The one
  Rust-adjacent thing is § 3.2's route precedence, and even that is a rule stated in
  `src/lib/chain.ts` over a field `chain.rs` already returns.
- **It does not touch `core/osinstall` or `docs/assets`** — both are being edited concurrently
  by other work, and this document was written read-only over them.
- **It does not decrypt anything, and it does not re-open the BoingBag ruling** either way.
  Host placement of BoingBag 1 and 2 is the bb-host round's, already decided and already
  briefed; this design consumes it.
- **It does not write a physical card**, and no round here may add that.
- **It does not collapse an ending.** Not the Amiga run's four, not host placement's three, not
  the chain's nine row states, not the readout's eleven. Every merge in § 3 is a merge of
  *screens*, never of *sentences*.
- **It does not add a summary route**, does not resurrect `ozet`, and does not add a "run
  everything" button that would report one outcome for several operations.
- **It does not change what the commander looks like.** The default column set renders the
  literal that is in `FileManager.css` today, and a test reads that literal out of the file.
- **It does not offer per-pane columns**, per-tab columns, column reordering by drag, or a
  sixth column. Those are separate features and each would need its own reasoning; § 5.2 says
  what the shape would cost if per-pane is ever asked for.
- **It does not move the narrow breakpoint**, whose lateness `dockLayout.ts` measured and
  recorded.

---

## 8. Verification, collected

Per round, and quoted in its report rather than summarised:

- `pnpm lint` (both passes — test-file type errors show only in the second),
  `pnpm test`, the i18n parity test by name, `cargo fmt --check`,
  `cargo clippy --all-targets -- -D warnings`, `cargo test --lib` **twice** (ART-059),
  `python scripts/control-byte-sweep.py` **unpiped**.
- Round C additionally: `python scripts/osbuilder-strip-check.py` with `pnpm dev` running, both
  languages, all four kinds, the `over` and `tallest` numbers quoted.
- Round D additionally: `python scripts/zoom-check.py --files`, and the
  `FileManager.css:407`-reading test named in the report.
- **Every mutation table in §§ 2.7, 3.5, 4.3 and 5.9 is executed**, the survivors disclosed, and
  each survivor classified as *a weak guard* or *a wrong mutation* — they need opposite answers.
  Mutate by absolute path with a `shutil.copyfile` restore, never `git checkout --`, never
  `shutil.move`.
- **A person drives each round** (§§ 2.8, 3.6, 4.3, 5.10) and the transcript goes in
  `docs/session-log.md`. Nothing in this design is finished on a green suite alone: every
  defect it exists to fix was invisible to one.

---

## 9. Known risks

1. **Round B is a 3400-line test migration, and coverage is lost by omission, not by error.**
   `AmigaInstallPanel.test.tsx` (2669) and `PackagePanel.test.tsx` (735) hold behaviours nobody
   will miss until a user does. Mitigation: the round's report lists every case by name with its
   new home, and any case without one is named as a behaviour being dropped and re-decided in
   the open.
2. **The walk in § 3.1 is the one new sequencing behaviour in this design**, and sequencing is
   where "it failed" sentences are born. It is bounded deliberately: host-placed rows only,
   ready rows only, stop at the first non-success, one report per row. If it still reads as one
   outcome for several operations when the owner drives it, drop it to today's *Run the next
   ready row* — the screen is unchanged either way and only the button's label moves.
3. **Step 3 could become the old scrolling column.** Plan + run + result + updates + first boot
   + summary is six blocks. The phase collapse (§ 2.2) is the whole mitigation, and it is
   exactly the mechanism the 2026-08-21 design specified and never built — so it has no
   precedent in this tree and must be built and driven, not assumed.
4. **`--tc-columns` cascades to both panes and to the header rows of both.** That is the point
   (§ 5.2), but it means a bad template string breaks four things at once. `gridTemplate` is
   pure and tested against the file's own literal, which is the cheapest place to catch it.
5. **A grip is 5 px inside a `zoom`ed shell.** At a small application size it may be hard to
   hit, and at a large one it may overlap the sort click target — a header that resizes when
   the user meant to sort is worse than no resizing. Measure the hit areas with
   `zoom-check.py --files` at three zooms before the round is called done, and widen the grip
   rather than narrowing the sort button if they collide.
6. **`packages.folder` becoming read-only depends on every row having a resolved file.** A row
   whose slot is ambiguous or missing has no folder to derive, and `add_package` needs one.
   That case already has a screen — `SlotField`, where the user picks — and the row is not
   `ready` until they have. If a route is found where a row is `ready` and yet has no path, the
   field goes back to being written and this design is wrong about it; say so rather than
   passing `""`, which is a refusal naming a folder the user never chose.
7. **The redirects are the migration for URLs, and there is no test that a user's habit still
   works** beyond § 2.7's router case. Anyone who has bookmarked `/os-builder/paketler` in the
   Windows sense (a workflow's `route` value) is covered by `builtin.rs` pointing only at
   `/os-builder`; that was verified by grep, once, on 2026-09-08, and a one-off verification
   answers only for the day it happened. Re-run the grep in round C.

---

## 10. Sources

- Read on 2026-09-08 for every count in § 1, on branch `art-091-fixes`:
  `src/pages/osbuilder/steps.tsx`, `src/lib/{buildSteps,buildSession,useBuildSession,remembered,dockLayout}.ts`,
  `src/pages/OsBuilder.tsx`, `src/App.tsx`,
  `src/components/osbuilder/{OsInstall,PackagePanel,AmigaInstallPanel,MaterialReadout,HostPlacement}.tsx`,
  `src/pages/FileManager.tsx`, `src/pages/FileManager.css`,
  `src-tauri/src/core/osinstall/chain.rs`, `scripts/osbuilder-strip-check.py`.
- `docs/superpowers/specs/2026-08-21-os-builder-flow-design.md` — the wizard's shape, the
  owner's five decisions, and the eight steps two of which were never built.
- `docs/superpowers/specs/2026-09-08-os-builder-intake-design.md` — the one material list, the
  slots, the readout, and the corrections marked in place after the round that built it.
- `docs/superpowers/specs/2026-09-08-os-builder-chain-design.md` — the chain as one list, the
  nine rows, the shared rank, the states, and the two "measure first" questions that were
  measured and answered against the guess.
- `docs/superpowers/specs/2026-09-08-os-builder-intake-research.md` § 3 — HstWB Installer,
  Emu68 Hatcher, Emu68-Imager, emu68-bootstrap, AmiKit, MultibootOS, CaffeineOS, ClassicWB:
  how each identifies media, says which file goes where, enforces order, and reports an ending.
- `.superpowers/sdd/2026-09-08-intake/bb-host-brief.md` — the owner's 2026-09-08 reversal, and
  the measurement behind it.
- `docs/FEATURES.md` rows for the OS Builder — the one material folder list, the per-artefact
  readout, the tree list, the chain screen (🟡), the packages panel (🟡), the install screen
  (🟡), first boot (🟡).
- `D:\Projeler\Amiga\CLAUDE.md` — read whole; the rules this design is written against are the
  sub-routes-and-one-typed-facade rule, the four rules of *the failure that does not crash*,
  *research before design*, *nothing changes unless the user changes it*, and *a test is not a
  guard until the defect has been put back*.
- The owner's own words, 2026-09-08: *"işletim sistemi oluşturma adımları hâlâ çok karışık"*,
  and *"1 2 3"*.
