# The OS Builder's flow: one session, eight steps, each its own route

**Date:** 2026-08-21
**Status:** design, approved by the owner in conversation
**Origin:** not a review finding and not a test failure. The owner drove the
built release binary, reached the Amiga-side install panel, and said two
things: *"bu işletim sistemi kurucusunda akış çok karmaşık gereksiz derecede
uzun"* and *"dağıtım ağacı için nereyi seçmeliyim anlamadım ben."* The second
sentence is the more valuable one — it is a person who has read every line of
this project's documentation being unable to answer a field on its own screen.

---

## What was measured before anything was designed

Under `CLAUDE.md`'s "Research before design" rule, this section is what was
counted and read rather than recalled. Every number below came from the files
named beside it.

### The screen's own size

| File | Lines |
|---|---|
| `src/components/osbuilder/OsInstall.tsx` | 1513 |
| `src/components/osbuilder/CardBuilder.tsx` | 873 |
| `src/components/osbuilder/AmigaInstallPanel.tsx` | 759 |
| `src/components/osbuilder/PackagePanel.tsx` | 695 |
| `src/components/osbuilder/VolumePreload.tsx` | 621 |

`/os-builder` already carries a four-way `BuildKind` picker — `distro`,
`boot-card`, `install`, `prepare-volumes` (`src/pages/OsBuilder.tsx`). So a
"what am I doing" step **exists**. What does not exist is any structure below
it: the `install` kind puts **ten `<h2>` sections in one scrolling column** —
eight of them in `OsInstall.tsx` itself (`osinstall` · `components` ·
`refusals` · `replaces` · `plan` · `run` · `result` · `verify`) plus the two
panels it renders below them (`packages`, `amigaInstall`).

### Fields and remembered keys, counted

Across the four builder jobs: **26 labelled fields, 22 distinct remembered
keys.** Both counts were extracted from the sources, not estimated.

### The defect underneath the complaint

The owner's confusion is not a wording problem. It is structural, and it
repeats four times: **ART produces an artefact, and the very next step asks
the user to go and find it.**

| ART produces | Remembered as | The next step asks for | Under | Connected? |
|---|---|---|---|---|
| the distribution tree | `osinstall.destination` | "Dağıtım ağacı" (packages, Amiga-side install) | `osinstall.packages.treeRoot` | **no** |
| the card image | `cardBuilder.dest` | "Kart imajı" (volume preload) | `preload.image` | **no** |
| the distribution tree | `osinstall.destination` | each partition's content folder | picked by hand, per partition | **no** |
| — | — | a Kickstart ROM, in three separate places | `osinstall.rom`, `amigaInstall.kickstart`, `cardBuilder.kickstart` | **no** |

The first row was verified in code rather than inferred:
`setPackagesTreeRoot` is called in exactly two places, `OsInstall.tsx:1362`
and `OsInstall.tsx:1378`, and both are the user's own picking inside a panel.
**Nothing anywhere sets it from `destination`.** So a user who has just
watched ART write 1915 files into a folder is asked, immediately below, to
locate a "distribution tree" — a term that names nothing visible on their
own disk.

This is the same failure class this project already names: the screen does
not crash, does not warn, and leaves the reader not knowing what to do next.

### Prior art, run rather than recalled

**Emu68-Imager** (<https://mja65.github.io/Emu68-Imager/>) prepares an SD card
with Emu68 and a pre-configured AmigaOS 3.1/3.2/3.2.2.1/3.2.3/3.9. Its own
Quick Start page states a **nine-step Simple Mode**, and three of its choices
are directly instructive here:

1. It has an explicit **Simple Mode**, chosen at launch.
2. Every path field carries a **`Check` button** — *"Click to set Kickstart
   Path"* then validate with `Check`, and the same for the ADF path. The tool
   tells you whether what you picked is right **at the moment you pick it**.
3. Its fields are named for **things on the user's disk** — "Kickstart Path",
   "ADF Path" — never for the tool's own internal artefacts.

ART's "Dağıtım ağacı" fails (3) and has nothing answering (2). That is the
whole of the owner's confusion, and it was diagnosable only by comparing
against a tool that solved it differently.

The other established builders (HstWB Installer, AmiKit, AmigaSYS, ClassicWB)
are recorded in `docs/STATUS.md` already and are not re-derived here; they
differ from ART on a separate axis — running the install inside an emulator —
which this design does not touch.

---

## Decisions the owner made

Recorded because they bound the design, and because a later reader should be
able to tell what was chosen from what was derived.

1. **All four complaints are real** — order and length; fields that do not say
   what they want; the same thing asked repeatedly; sections that do not
   belong on this screen.
2. **Both a wizard and separate screens**, not one or the other.
3. **The tree picker is a list *and* a folder picker**, with the picked folder
   validated at once.
4. **The Amiga-side install is an optional step inside the wizard**, offered
   at the moment the tree is built.
5. **ART never writes a physical card. The output is an image file.** This
   restates an existing decision (`docs/owner-checklist.md` § 4: *"Kartı yaz
   … ART yazmıyor — bilinçli karar"*) and the design must not quietly widen
   it.

---

## The design

### 1. Shape: one route tree, steps as real sub-routes

```
/os-builder                 → the wizard, resuming at the furthest step reached
/os-builder/hedef           1. What are we building        (today's BuildKind picker)
/os-builder/kaynak          2. Install media + Kickstart
/os-builder/bilesenler      3. Components                  (+ refusals, + replaces)
/os-builder/paketler        4. Update packages             [optional]
/os-builder/amiga-kurulum   5. Run the package's own installer on the Amiga [optional]
/os-builder/kart            6. The card image              (Emu68 + ROM + partitions)
/os-builder/birimler        7. Prepare volumes, copy the tree in
/os-builder/ozet            8. Verify and finish
```

Sub-routes rather than internal step state, so browser back/forward and
"jump to a step" work at the router level. `App.tsx`'s route table gains the
children; `builtin.rs::route` values that point at `/os-builder` keep working
because the parent route still renders.

Three rules govern every step:

- **One step asks one question.** Ten sections become eight steps, and exactly
  one is on screen at a time.
- **A step opens standalone.** Navigating straight to `/os-builder/paketler`
  works: if the session already holds a tree, the step shows a one-line
  summary of it (*"AmigaOS 3.9, 1915 files, E:\…\t2"*); if it does not, the
  step asks. This *is* the "separate screens" half of decision 2 — not a
  second mode, just what a step does when its precondition is or is not
  already answered.
- **A skipped step stays visible.** Steps 4 and 5 are optional; skipping is
  stated on the summary (*"No BoingBag was installed — the tree stays at
  45.1"*) rather than passing silently. This is the four-endings rule applied
  to a step rather than to a run.

The left navigation keeps one entry. A progress strip sits above the step;
a completed step collapses to a single clickable summary line.

### 2. The session

`src/stores/buildSession.ts`, one Zustand store:

```ts
BuildSession {
  kind
  media        { folder, reuseScan }
  rom          { path }              // ONE Kickstart; every step that needs one reads this
  release
  tree         { root, builtHere }   // destination and packages.treeRoot, merged
  components   { chosen, excludedConditional }
  packages     { folder, chosen }
  amigaInstall { packageId, archive, overlayArchive, medium }
  card         { archive, fsType, line, partitions }
  output       { imagePath }         // always a file, never a physical card
}
```

The one load-bearing behaviour: **step 3 writes `tree.root` itself the moment
it finishes building** (`builtHere: true`), and steps 4, 5 and 7 read it. All
four disconnections in the table above close because there is one variable,
not because anyone remembered to wire them.

**Migration is mandatory, and is not negotiable.** The 22 existing remembered
keys are read once and seeded into the session, which then persists under its
own keys. `CLAUDE.md`'s rule — *nothing changes unless the user changes it* —
covers a setting being **moved** as much as a setting being reset: without
migration, every user's saved paths silently empty on first run of the new
build, which is exactly the outcome the rule forbids. Reads keep
`sameRemembered`'s identity guard (ART-178/ART-195), so a late-landing recall
cannot overwrite a value the user has touched this run.

### 3. Validation at the moment of picking

One reusable component, used in three places — the tree, the card image, the
ROM:

```
┌ AmigaOS folders ART built ─────────────────────────┐
│ ● AmigaOS 3.9 · 1915 files · no BoingBag · 21.08   │
│ ○ AmigaOS 3.2 · 4030 files · V47 · 16.08           │
│ ○ Choose another folder…                           │
└────────────────────────────────────────────────────┘
```

- **The list is built from what ART already knows**: the operation log's own
  `Build an AmigaOS distribution tree` destinations plus remembered paths.
  Each candidate is turned into a row by reading its `distribution.json`.
- **A hand-picked folder is validated immediately** — Emu68-Imager's `Check`,
  without a button to press: *"AmigaOS 3.9 tree, 1915 files"*, or *"no
  `distribution.json` here"*. Never a refusal arriving minutes later.
- Rust side: one new command over `core::osinstall`, `describe_tree(path) ->
  TreeSummary`. The other two artefacts already have theirs — `rom_identify`
  and the card reader.

A tree the list does not know is still answerable, because the folder picker
never goes away. That is decision 3 in full.

### 4. What moves, and the wording that goes with it

`OsInstall.tsx` splits into step components; `PackagePanel` becomes step 4,
`AmigaInstallPanel` step 5, `CardBuilder` step 6, `VolumePreload` step 7, and
the "verify against a card" section leaves `OsInstall` for step 8.

Fields are renamed for what the user has on disk. "Dağıtım ağacı" becomes
**"the AmigaOS folder ART built"** (`osinstall.tree.label`), in both
catalogues.

One string is corrected in both catalogues because it contradicts itself:

> EN: *"Add an **official** update — a BoingBag, or an **unofficial** pack
> like the Turkish catalogs — onto a distribution tree…"*
> TR: *"…**resmi bir güncelleme** — bir BoingBag, ya da Türkçe katalog paketi
> gibi **resmi olmayan** bir paket — ekle."*

The em-dash pair reads as an appositive to "an official update", so the
sentence offers an unofficial pack as an example of an official one. It also
explains what a BoingBag is; the owner's ruling is that it needs no
explaining — the name is known across the Amiga community. Both halves are
fixed together.

### 5. Testing

The mutation rule applies: every guard that matters is put back and seen to
fail. This round's own traps, named in advance so a green suite cannot be
mistaken for a guarded one:

1. **Break the carry** — stop step 3 writing `tree.root`, and a test must
   fail. This is the defect being fixed, so it is the one mutation that
   matters most.
2. **Remove the migration** — the previously remembered paths must be seen to
   disappear.
3. **Silence a skipped step** — the summary must fail when it stops saying
   that an optional step was skipped.
4. **Open a step with no precondition** — it must *ask*, and a mutation that
   makes it throw or render empty must fail.

"Which step is satisfiable" is pure TypeScript in `src/lib/`, tested without a
DOM. Sentences built there return `Phrase`, per the two-catalogue rule.
Rust is untouched apart from `describe_tree`.

---

## Scope

Three waves, each leaving the tree green and the application usable:

1. **Session, migration, routes — and the two filed defects.** The store, the
   22-key migration, the sub-route table, the progress strip. No panel is
   rewritten yet; each is mounted at its step and reads the session for the
   values it used to own. **[ART-197](../../ISSUES.md) closes here**, because
   the session is what closes it: `destination` and `packages.treeRoot`
   become one `tree.root`, so the carry holds by structure rather than by
   wiring. **[ART-198](../../ISSUES.md) closes here too** — the
   `osinstall.packages.intro` sentence is corrected in both catalogues in one
   commit, dropping the gloss on "BoingBag" as well as the
   official/unofficial contradiction. It rides in this wave rather than
   wave 3 at the owner's request: it is a one-line correction in two files
   and there is no reason it should wait behind a restructuring.
2. **The steps and the artefact picker.** `OsInstall.tsx` is split, the
   picker is built, `describe_tree` lands, the duplicated fields are deleted
   rather than merely bypassed.
3. **Summary, verify, wording.** Step 8, the skipped-step sentences, and the
   field renames (`osinstall.tree.label` and its neighbours).

## What this design does not do

- It does not write a physical card, and no wave may add that.
- It does not change what the engine builds. `core/osinstall`, `core/card`,
  `core/preload` and `core/amigainstall` keep their behaviour; this is a
  front-end restructuring plus one read-only Rust command.
- It does not touch the Amiga-side install's own four endings, its refusals,
  or its staging and promotion. Step 5 hosts that panel; it does not redesign
  it.
- It does not resurrect user-defined machine profiles or any other unbuilt
  claim; a step shows what exists.

## Sources

- `src/components/osbuilder/*.tsx`, `src/pages/OsBuilder.tsx`,
  `src/i18n/{en,tr}.json` — read on 2026-08-21 for every count in this
  document.
- Emu68-Imager Quick Start and Included Components pages,
  <https://mja65.github.io/Emu68-Imager/>, fetched 2026-08-21 — the nine-step
  Simple Mode, the `Check` buttons, and the disk-facing field names.
- `docs/owner-checklist.md` § 4 — ART does not write cards.
- The owner's own session on the release build, 2026-08-21, including the
  `SAFETY-REFUSED` refusal that was read on screen.
