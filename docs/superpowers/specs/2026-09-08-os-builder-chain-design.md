# The AmigaOS 3.9 chain as one screen — in the material's own order, one Run

**Date:** 2026-09-08
**Status:** design for round 3 of [`2026-09-08-os-builder-intake-research.md`](2026-09-08-os-builder-intake-research.md) § 7,
building on round 2's slots ([`2026-09-08-os-builder-intake-design.md`](2026-09-08-os-builder-intake-design.md))
**Owner's decisions bound here:** the chain covers the whole material — ISO, BoingBag 1,
BoingBag 2, Locale 3.9, the Turkish locale update, Contribution, Euro-Update, BoingBags 3&4 —
and the Amiga-side step of the wizard *is* this screen, also reachable from the tree picker.

---

## 1. What the material says the chain is

From `material.md` (the owner's archives, read at both levels, corrected once) — each arrow
is enforced by the next package's own `Install` script reading `version.library` off the
target, not by prose:

```
AmigaOS 3.9 CD  →  BoingBag 1  →  BoingBag 2  →  { Locale 3.9 · Turkish locale · Contribution · Euro-Update }  →  BoingBags 3&4
                    (UAE fix only if Updater < 45.15)              (any order; all before BB3&4)
```

- **BB1 and BB2** run `C/Updater` on a ZipCrypto payload; Amiga-side only, and ART's engine
  already does it (ART-193: 169 s and 138 s on the owner's material, the tree answering
  `Workbench 45.3`). BB2 ships a second payload, `XAD-Update`, applied by a second `Updater`
  run only when `xadmaster.library` is older than 10.
- **Locale 3.9, the Turkish update, GenesisPrefs, Euro-Update, BB3&4** are plain files
  installed by an Installer script that runs a bundled helper (`GetLocale`, `FixFonts`):
  Amiga-side, but no encryption. ART places `Locale 3.9`'s files from the host already
  (`locale-39`, `locale-39-turkish` — the recipe reimplements `Install-Locale`'s copies), which
  the research recorded as ART's unusual-but-legitimate host path for a release laid out as
  files.
- **Contribution** is plain files with no script: host-placeable, a `PathRule` package.
- **Euro-Update** is folded into BB3&4 (its readme); on its own only where BB3&4 is not
  installed. **GenesisPrefs** is folded into BB1.
- **BB3&4** (community, v1.59, 2023) requires `OS3.9+BB2` and the locale packages before it.

---

## 2. The screen

> **Corrected in place on 2026-09-08, after the round it designed** (round 3's whole-branch
> review, M2). A spec describes the tree on the day it was written, and this one was right about
> the shape and wrong in four particulars the building measured. The corrections are marked
> **[2026-09-08]** where they belong rather than collected at the end, so a reader of § 2.3 is
> not sent down a road the tree has already left. The reasoning either side of them stands as it
> was written.

`/os-builder/amiga-kurulum` keeps its route and its place in the wizard; its content becomes
the chain, one row per link, in the order above, fed by round 2's `SlotState`s and the tree's
`distribution.json`:

```
AmigaOS 3.9 updates — tree E:\amiga\ProjeART\dist-3.9        ● 3 of 8 applied

1  AmigaOS 3.9 CD            installed 2026-09-07 (built from AmigaOS39.iso, by hash)
2  BoingBag 3.9-1            installed 2026-09-07 · Updater 45.15 · ROM Update promoted
3  BoingBag 3.9-2            installed 2026-09-07 · XAD update: not needed (xadmaster 12.1)
4  Locale 3.9                ready · Locale3_9.lha · placed from Windows            [Run]
5  Turkish locale update     ready · BoingBag39-2-turkce.lha · placed from Windows
6  Contribution              ready · BoingBag39-2-Contribution.lha · placed from Windows
7  Euro-Update               not needed if 8 is installed · Euro-Update.lha
8  BoingBags 3&4             blocked: 4 first · BoingBags3&4.lha · runs on the Amiga
```

> **[2026-09-08] Nine rows, not eight, and rank 4 is shared.** The sketch above gives eight rows
> and eight distinct ranks. The shipped recipes give **nine** — the Turkish material is two
> packages, not one, and the material states no order between two of them. As built, and read
> out of the recipes by `chain::the_chain_is_the_materials_own_order_with_the_cd_first`:
>
> ```
> 1  AmigaOS3.9                     the medium
> 2  boingbag-39-1
> 3  boingbag-39-2
> 4  locale-39                      }  rank 4 twice: the material states no
> 4  locale-39-turkish              }  order between these two
> 5  locale-turkish                 the LocaleUpdate archive, after BoingBag 2
> 6  boingbag-39-2-contribution
> 7  euro-update
> 8  boingbags-39-3-4
> ```
>
> So `chain_position` is a **rank and not a sequence number**, rows sort by `(position, id)`,
> and the summary reads *"N of 9 applied"*.
>
> **[2026-09-08] The CD row's state.** `medium_state` has **two** answers — `Installed` and
> `Missing` — and never `Ready`. § 2.1 below says the CD row is never run here, and a `Ready`
> medium is exactly what let the one Run button land on it (whole-branch review, M3).
>
> **[2026-09-08] A state the table below has no row for:** `blocked-by-component`. A package may
> declare `requires_components`, and a tree built without one is ordinary rather than exotic
> (`locale-base` is `required: false` in the 3.9 recipe). It gets its own sentence, because the
> next step is on the components step and not anywhere in this list (M4).

**One Run button**, on the first row that is *ready*; it runs that row only. A row is:

| State | Meaning | Source of truth |
|---|---|---|
| installed | `distribution.json` records it (`amiga_installed` for a run, `files`/`built_from` for a placement) | the manifest, never a file being present |
| ready | its slot is found, every row it `requires` is installed, its `required_medium` slot is found | slots + manifest |
| blocked: N first | a required row is not installed | `chain.rs` (ART-186), extended from "BB1 before BB2" to the whole chain, read from each recipe's `requires` |
| missing X | its slot is not found; the sentence names the expected file | slots |
| not needed | the material's own rule makes it redundant (Euro-Update under BB3&4; the UAE fix under `Updater` ≥ 45.15) | recipe data: a new `superseded_by` field |
| refused | ART will not run it and says why (no Kickstart, no emulator, an ambiguous slot) | the existing refusal paths, reworded per round 1 |

Every sentence is a `Phrase`; the four endings of a run stay the four the panel already has
(succeeded / refused / timed out / window closed), each with its next step.

### 2.1 What each row does when Run is pressed

- **Amiga-side rows** (BB1, BB2, BB3&4, and — if the owner wants them run by their own
  installers rather than placed — Locale/Turkish/Euro): the existing `compose → install`
  path, unchanged in mechanism: staged copy, three volumes, deadline, result file, promotion
  on success, `finish.rs` post-steps. The preview text the panel shows today stays, folded
  under the row.
- **Host-placed rows** (Locale 3.9, Turkish, Contribution, Euro-Update when chosen): the
  `paketler` step's `add_package` path, with round 1's hash-based clash check. Contribution
  gains a recipe (`boingbag-39-2-contribution.json`: a `subtree` rule into
  `BoingBag3.9-2/Contribution`'s destination as the archive's own icon-drop intends — measure
  where the Amiga-side BB2 put its `Contribution` drawer first, and place beside it).
- **The CD row** is never run here; it links back to `kaynak`.

### 2.2 Recipe data this needs

- `requires` already exists on packages; every chain row gets its correct list (BB3&4:
  `["boingbag-39-2"]`, Turkish update: `["boingbag-39-2"]` per its readme, Locale 3.9: none
  beyond `locale-base`).
- `superseded_by: ["boingbags-39-3-4"]` on `euro-update`; the UAE fix is already expressed by
  `minimum_version` + `overlays`.
- `chain_position: N` so the screen orders rows without a list in code.
- New recipes, each a JSON file and nothing else (the design's own claim, now tested by
  three): `boingbags-39-3-4.json` (Amiga-side, `program: "Installer"` with its own script? —
  **measure first**: BB3&4's `Install` is an Installer script that runs `Files2/C/FixFonts`;
  the engine today runs a *program*, not `Installer <script>`. If `Installer` in non-pretend,
  novice-less mode stops on a requester, the row is *refused: needs a person at the window*
  and says so — not silently timed out), `euro-update.json` (host-placeable: fonts, keymaps,
  countries as `file` rules; `FixFonts` is only a `.font` index rebuild, which ART's own
  `fontfile` code can do or the row says it cannot), `boingbag-39-2-contribution.json`.

> **[2026-09-08] Both "measure first" questions were measured, and both answered against the
> guess above.**
>
> - **BoingBags 3&4** ships as `program: "Install"` — its own top-level script's real name,
>   read from the owner's archive, not `"Installer"` — **and carries `not_yet_runnable`.** The
>   script calls `askoptions` for the languages, `confirm` for the target and
>   `(run "SYS:Tools/EditPad …")` on the startup-sequence, and nobody has measured whether it
>   finishes without a person at the window. So the row is registered and shown **disabled with
>   its own sentence** (§10), and `compose` refuses it too — which is the paragraph above's own
>   remedy, reached by measurement rather than by hope.
> - **Euro-Update is not host-placeable.** It ships `host_placement_block: "needs-fixfonts"`
>   and an untickable row. `FixFonts` rebuilds the `.font` index, and ART parses no
>   `FontContentsHeader` anywhere — the ten descriptors were dumped and compared against ART's
>   own 3.9 tree before the block was written. The refusal names the row and gives two routes.
>   The *"or the row says it cannot"* half of the sentence above is the half that came true.

### 2.3 The three remaining post-Updater fix-ups (ART-227), gated on a measurement

Before any of them becomes a `PostStep` variant, measure on the tree BB2 produced at
`E:\amiga\ProjeART\dist-3.9-bb` (or a fresh run):

1. `Version "Libs/xadmaster.library" FILE` — if < 10, BB2's `XAD-Update` was never applied and
   `finish.rs` gains `run-again-with` (a second `Updater` invocation with another payload,
   gated on a file version); if ≥ 10, the row says *not needed* and nothing is built.
2. `C/Installer` in the tree vs BB2's — if the tree's is older, a `copy-from-package` step
   into `C:` and `Utilities:`.
3. Locale catalogs: whether BB1/BB2's `Updater` already merged the languages the tree has
   (compare `Locale/Catalogs/türkçe` before and after); if not, a `merge-if-present` step.

Each one that a measurement asks for is added with its own test; each one it does not is
written down as *measured, not needed* in ART-227.

> **[2026-09-08] All three were measured, and item 1's remedy is wrong in this text.**
> The measurement itself held: `xadmaster.library` reads 9.0 clean, 9.1 after BoingBag 1 and
> **9.1 still** after BoingBag 2, so `XAD-Update` was never applied. Items 2 and 3 came back
> *measured, not needed* — `C/Installer` is byte-identical to BoingBag 2's own, and 597 catalog
> files in 20 languages were **0 added, 0 removed, 0 changed**, because neither payload carries
> a single `Locale/` entry.
>
> **What `finish.rs` gains is nothing.** That module is host file operations on the staged copy
> — no emulator, no ROM, no licence — and `XAD-Update` is a second ZipCrypto archive only the
> package's own `Updater` can open (ART-166). A `run-again-with` variant there could not place a
> single one of its 39 files.
>
> What shipped instead ([ART-280](../../ISSUES.md), the round's fix round 1) is
> **recipe data plus four lines in the boot script ART already writes** —
> `amiga_installer.follow_ups`, emitted by `workvol::startup_sequence` as
> `If EXISTS <sys>:C/Version` / `Version >NIL: <sys>:Libs/xadmaster.library 10 FILE` / `If Warn`
> / the second invocation, which is HstWB Installer's own shape
> (`Install-Boing-Bag-2` lines 32-36, MIT). **In the same boot**, not a second run, and behind
> no requester: the `#install-xad-update` requester lives in BoingBag 2's own `Install` script,
> which ART does not run. It reports separately in `art-followup.txt` — `ran` / `not-needed` /
> `failed` / `not-checked` — beside the install's ending and never inside it.
>
> **Two ordering traps, and the second was found by running it.** `Version … FILE` sets WARN as
> its *answer*, so the gate must sit below the branch that reads the installer's return code;
> and the host terminates the emulator the instant the result word appears, so the `ok` word
> must be written **last** or the follow-up never executes. Measured: 141.1 s with the block in
> the wrong place, no marker file, a byte-identical tree — then 156.6 s with it right, and
> `xadmaster.library` **9.1 → 10.0 (31.03.2001)**.

### 2.4 The experiment nobody has run: does `Updater` return a code?

One variable: a copy of `BoingBag39-2.lha` with one byte of `AmigaOS-Update` changed (the
member is re-packed into the wrapper; the payload stays encrypted and is merely corrupt).
Counted result: `art-result.txt` after the run, both arms (corrupt vs the control, the real
archive), the control measured too. Decided first: if the corrupt arm writes `ok`, ART's
"succeeded" ending for a BoingBag rests on nothing and the row's success is verified
afterwards by asking the artefact (`Version Libs/workbench.library FILE` before and after, or
`version.library`'s revision — the check the material's own scripts use). If it writes
`failed`, the existing `If Warn` is proven and stays. Either way the finding goes into
`lessons.md` beside the five AmigaDOS rules.

---

## 3. What this round does not do

- No password code (the ruling). No download. No physical card.
- It does not run `Installer` scripts interactively for the user; where a package needs a
  person at the window it says so and the four endings still apply.
- It does not restructure `paketler`: host-placed rows reuse its path; the step itself can
  later collapse into this screen if the owner wants (a question for after the chain has
  been driven end to end).

---

## 4. Verification

- `chain.rs`: order derived from `requires` + `chain_position` over synthetic recipes; every
  state in § 2's table reachable and asserted by its sentence; `superseded_by` yields *not
  needed*; a row with an ambiguous slot is *refused*, not *ready*.
- Recipes: `packages()` loads the three new JSON files; `catalogue-check`-style test that
  every chain row's `requires` names a shipped id.
- Frontend: `ChainScreen.test.tsx` — the eight rows render in order from a fixture; Run sits
  on the first ready row; each state's phrase in both catalogues.
- **Real material, through the screen** (the audit's throughline): the owner runs the chain on
  `E:\amiga\Amigatolon\os39` from a clean 3.9 tree: BB1 → BB2 → Locale → Turkish →
  Contribution → BB3&4; the tree is booted and asked `version full`; the session log records
  each row's time and ending. The measurements of § 2.3 and § 2.4 are recorded in ART-227 and
  ISSUES/lessons respectively.
