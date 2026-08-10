# ART — gap analysis and roadmap to the full spec

**Date:** 2026-08-09
**Status:** approved
**Scope:** all 96 sections of `AMIGA RETRO TOOLKIT — ART.md`, plus both addenda

---

## What this document is

A survey of what ART actually does today, what the spec asks for, and the
order in which to close the distance — decided by dependency: what unblocks
the most comes first.

It is not a plan. Each slice below gets its own implementation plan when its
turn comes.

## Constraints that shaped it

**Target.** The whole spec. Nothing is out of scope on grounds of ambition;
the AI layer (§45.5) is separated out because it is a different kind of work,
not because it is optional.

**Ordering.** Dependency. Where two slices are independent, the one that
unblocks more comes first.

**Verification.** Every piece of hardware the spec touches is available: a real
Amiga, Gotek, PiStorm, a licensed WinUAE with licensed Amiga Forever ROMs and
Workbench. So nothing has to be marked "buildable but unverifiable", and the
verification ladder gains a top rung:

```
cargo test          ART agrees with itself
amitools oracle     ART agrees with an independent implementation
real Amiga          the disk actually boots
```

The third rung is the only one that settles questions the first two cannot —
whether a disk ART wrote is a disk an Amiga will use. It applies to any slice
that changes what ART writes.

---

## Part 1 — Live defects

These are broken now, not merely missing. Each has a diagnosed root cause.

### D1. ADF Studio cannot open any bootable ADF

`core/adf/bootblock.rs` reads bytes 8..11 of the boot block as a "root block
pointer". An AmigaDOS boot block has no such field: 0..3 is the DOS type,
4..7 the checksum, and **8 onwards is boot code**. ART therefore reads 68000
machine code as a block number. The reported `1963519789` is that code.

The correct value is computed, not read: `total_blocks / 2`. ART already has
this — `VolumeGeometry::root_block_for`, pinned during Stage R against both
ADFlib (`adfVolCalcRootBlk`) and amitools (`BootBlock.calc_root_blk`).

**`core/adf` predates Stage R and never adopted it.** The consequence is
visible: `/files` opens a disk that ADF Studio refuses, because the file
manager runs on `core/volume` and ADF Studio does not. It works only on
non-bootable disks (bytes 8..11 zero, falls back to 880) and on images ART
created itself.

The same omission is why HD ADFs do not work: the old path assumes
`DD_TOTAL_BLOCKS`.

### D2. Disabled buttons look enabled

The string `disabled` appears in no stylesheet. A `disabled` button is
visually identical to an active one — including a primary button, which stays
solid blue. Observed on the WHDLoad screen: after a refusal the "Install to
the disk" button was correctly disabled and looked entirely clickable.

### D3. Content is clipped, with no scrollbar

`.app-shell` is `height: 100vh; overflow: hidden`. `.app-main` is a grid item
with the default `min-height: auto`, so it grows to fit its content instead of
being constrained to the row. `.app-content`'s `overflow: auto` therefore never
engages — there is nothing to scroll, because the parent already grew — and the
excess is clipped by the shell.

`min-height: 0` appears nowhere in the codebase. This is the standard
flex/grid overflow trap.

### D4. Nothing is responsive

Zero `@media` queries in the entire application. The sidebar is a fixed 224px,
quick actions are `repeat(4, 1fr)`, and every page carries a hand-written
`maxWidth` (980 / 1040 / 1100). Narrow windows crush the two-pane manager;
wide windows waste both margins.

### D5. Refusals are presented as system errors

A plan that says "this is not a WHDLoad package" shares its red banner and its
`ART-*` identifier with a genuine runtime failure. The identifier is correct
for a failure (§68) and wrong for an answer. A refusal is the normal outcome
of asking; it belongs where the question was asked, in the calm voice the
plan uses everywhere else.

---

## Part 2 — Gap inventory

### Per-screen missing actions

| Screen | Present | Missing |
|---|---|---|
| Aminet | search, sort, filter, download, install to ADF/HDF, show in Collection, edit mirrors, update view | **move a download to another subfolder** (`sources_relocate` — written and tested, no button) · **version resolution** (`sources_resolve` — written and tested, no UI) · browsing the catalogue's directory tree |
| LHA Browser | open, extract all | extract one entry · **per-entry CRC test** (§14) · jump from the WHDLoad panel into the §82 install · drag out |
| ROM Studio | identify, scan folder | assign a ROM to a profile (`rom.use_in_profile` is registered) · rename/organise · missing-ROM report |
| PiStorm | select SD, save | **editing** the config — it is display-only |
| Gotek | select USB, add disks to slots, save | reorder/remove slots · **bulk workflow** (§38) |
| Collection | scan, grid/table, play | **tags, favourites** (§41 — schema exists, no UI) · **duplicate view** (§43 — hashing works, no screen) · incremental rescan |
| Hard Disk | create HDF | **resize / clone / repack / migrate** (§21–24) · **snapshots** (§49) · partition editing |
| Hex Tools | open, jump to block, show signatures | **search** (bytes / text) · go to offset |
| ADF Studio | open, create, add file, mkdir, rename, delete, extract, validate | **optimisation analysis** (§13) · ADF↔ADZ conversion (§26) |
| Settings | theme, language, WinUAE path, export log | **editor path** (checkout takes an `editor` argument with no setting behind it) · Aminet download folder · backup generation count · file associations (§59) |
| Dashboard | recent files, quick actions | **"what can ART do?"** overview (`list_workflows` is never called) |

### Commander (the two-pane manager)

Multi-select (Shift / Ctrl / Insert) · **adding several `.lha` files to a disk
at once** · **adding an ADF's contents to a disk** (disk to disk) · starting a
WHDLoad install from the panel · sorting by name, size and date · a filename
mask filter · directory comparison · size calculation · history and favourites
· panel synchronisation · Tab to switch panes · dragging a multi-selection.

### Language

Turkish alongside English, both shipped. Twelve of the fourteen screens carry
their English in the code; only Settings, Dashboard and the page headings go
through `t()`.

### Dead code and stale data

`adf_extract_to`, `panel_plan_folder_copy`, `volume_write_bytes` — superseded
by their Stage W equivalents and still registered · `lha_extract_job` —
registered with no TypeScript wrapper · `sourcesGet` · the `ComingLater` page,
which no route reaches · rows in `FEATURES.md` that no longer describe the
code.

### Modules not built

§49 Snapshot Manager · §29 Recovery Lab · §50 Project Manager · §27 image
compare · §30 binary/hunk inspector · §34 Compatibility Engine · §45 Workflow
Wizard · §26 conversion (ADZ/HDZ/DMS) · §57 raw device writes · §45.5 AI layer

### Format and filesystem depth

DMS/ADZ/HDZ decoding · **ISO9660 (AmigaOS, CD32 and CDTV discs)** · dircache
writing · PFS3 · SFS · long-filename FS

ISO is a category ART does not have. `FormatCategory` knows floppy, hard disk,
archive, ROM and directory; an `.iso` dropped on the window is `unknown`, so
the drop pipeline offers nothing. Every AmigaOS install CD, every CD32 and CDTV
title is currently invisible to ART.

**And the wider problem underneath it: ART dispatches on the extension.** An
`.img` is not a format — it is a raw sector dump of *something*. The same name
covers a 901,120-byte floppy dump, a multi-gigabyte hard-disk dump with an RDB
at block 0, and a raw CD image. `.dsk`, `.ima` and `.raw` are the same story.
Deciding from the four letters after the dot is guessing.

The fix is to decide from the **content**, with the extension demoted to a hint
that only breaks ties:

| Evidence | What it is |
|---|---|
| `DOS\0`…`DOS\7` at offset 0 | AmigaDOS floppy — geometry from the length |
| `RDSK` at block 0 | hard disk with a Rigid Disk Block |
| `CD001` at 0x8001 | ISO9660 — offset 0x8001 also proves the 2048-byte sector size |
| `CD001` at 0x9311 | ISO9660 inside a 2352-byte raw/`MODE1/2352` track |
| `PDS\3` / `PFS\3` at block 0 | PFS3 |
| `SFS\0` | SFS |

`Detection` already carries `confidence` and `format_hint` for exactly this;
nothing in the struct has to change. What changes is that `detect()` reads the
first blocks instead of reading the filename — which also settles `.adf` files
that are really HD, and `.hdf` files that are really an unpartitioned single
volume.

The HD floppy path belongs to D1 rather than here: it fails for the same
reason bootable DD images do, and 0.2 settles both.

---

## Part 3 — The slices

Each slice is a complete, usable capability, ordered by dependency. Sizes are
rough: **S** one session, **M** two or three, **L** several, **XL** its own
sub-project.

### Phase 0 — Ground repair

No new features. What exists starts working properly. Every screen built
afterwards inherits these fixes; deferring them means redoing that work.

| Slice | Contents | Size |
|---|---|---|
| **0.1 Shell** | the `min-height: 0` chain so scrolling actually engages · `@media` breakpoints, a flexible sidebar, per-page `maxWidth` removed · `:disabled` and `:focus-visible` styles · **refusal ≠ error**: a shared decision/preview component, with the red banner reserved for genuine failures | S |
| **0.2 `core/adf` onto `core/volume`** | stop reading the root block from boot code, use `root_block_for` → D1 fixed and **HD ADFs read and write** · retire `core/adf/mutate.rs` (superseded by `core/volume/write/`) and the `&[u8]` wrappers in `fs.rs` / `extract.rs` / `validate.rs` whose `*_on` device forms already exist · `create.rs` stays — formatting a blank image has no equivalent yet · the ART-009…013 regression tests move to the surviving path rather than being deleted | M |
| **0.3 Dead code and stale data** | the list above, removed · `FEATURES.md` brought back to the truth | S |
| **0.4 Turkish and i18n** | twelve screens moved onto `t()` · `tr.json` · a language switcher · both languages shipped | M |

### Phase 1 — The commander

After Phase 0 so it is born bilingual and scrolls correctly.

| Slice | Contents | Size |
|---|---|---|
| **1.1 Multi-select and batch work** | Shift / Ctrl / Insert selection · **several `.lha` files onto a disk at once** · **an ADF's contents onto a disk** · batch copy and delete as one job | M |
| **1.2 Panel strength** | sorting · mask filter · Tab between panes · history and favourites · panel synchronisation · size calculation · directory comparison | M |
| **1.3 WHDLoad from the panel** | a selected `.lha` goes straight into the §82 install without leaving the panel | S |
| **1.4 The missing buttons** | every row from the per-screen table above | L |

### Phase 2 — Completing data safety

| Slice | Contents | Size |
|---|---|---|
| **2.1 §92 preview everywhere** | delete, overwrite and format explain themselves beforehand, not afterwards | M |
| **2.2 §49 Snapshot Manager** | *Recovery Lab, Project Manager and resize/clone all stand on this* | M |
| **2.3 §57 raw device writes** | double confirmation, verified on real hardware | M |

### Phase 3 — Format breadth

| Slice | Contents | Size |
|---|---|---|
| **3.1 Content-first detection** | `detect()` reads the first blocks instead of the filename · `.img` / `.dsk` / `.ima` / `.raw` resolve to whatever they actually contain · the extension becomes a tie-breaker · an `.adf` that is really HD, and an `.hdf` that is really one unpartitioned volume, both stop being special cases · *everything below depends on this* | S |
| **3.2 Optical images** | a new `optical-image` category · ISO9660 with Joliet and Rock Ridge, including the Amiga `AS` System Use entry that carries protection bits and file comments · `.cue`+`.bin`, `.nrg`, `.ccd`/`.img`/`.sub`, `.mdf`/`.mds`, both 2048- and 2352-byte sectors · a read-only browser and extraction, on the same two-pane manager · AmigaOS install CDs, CD32 and CDTV titles readable, and installable to an HDF through the §82 path | M |
| **3.3 §26 DMS / ADZ / HDZ decoding** | *every studio gains these formats at once* | L |
| **3.4 Filesystem depth** | dircache writing, PFS3, SFS, long-filename FS | XL |

Optical images are read-only in this phase. Writing a bootable Amiga CD is a
separate problem — El Torito plus the CD filesystem the target machine
expects — and it does not belong in the slice that makes the discs readable.

### Phase 4 — Disk operations

| Slice | Contents | Size |
|---|---|---|
| **4.1 §21–24 resize / clone / repack / migrate** | requires 2.2 | L |
| **4.2 §29 Recovery Lab** | requires 2.2 and the §69 validation surface | L |

### Phase 5 — Analysis

| Slice | Contents | Size |
|---|---|---|
| **5.1** | §27 image compare · §30 hunk inspector · §28 Disk Analyzer depth | M |

### Phase 6 — Judgement

| Slice | Contents | Size |
|---|---|---|
| **6.1 §34 Compatibility Engine** | prerequisite for the Wizard | L |
| **6.2 §45 Workflow Wizard** | requires 6.1 | L |

### Phase 7 — Consolidation

| Slice | Contents | Size |
|---|---|---|
| **7.1** | §50 Project Manager (requires 2.2) · §38 Gotek bulk · §14 LHA creation · §13 optimisation analysis · §64 accessibility audit | L |

### Separate project

**§45.5 AI layer.** Three stages of its own — a read-only assistant, then plan
generation with Plan Cards, then multi-step scenarios. It needs its own spec,
its own security model and its own test strategy. It follows the roadmap
rather than forming a phase of it.

---

## Why this order

- **0.1 and 0.2 are underneath everything.** One is the ground every screen
  stands on; the other is a core that is currently wrong.
- **0.4 precedes Phase 1** so the commander is born translated rather than
  retrofitted.
- **2.2 Snapshot unblocks three things** — Recovery Lab, Project Manager and
  the resize/clone family — which is the largest fan-out left.
- **3.1 detection comes before every other format work.** Optical images and
  the compressed formats both add cases to the same decision; making that
  decision read the content first means each new format is one signature
  rather than one more extension in a growing list.
- **3.3 conversion unblocks a format everywhere at once**, in every studio,
  rather than one screen at a time.
- **6.1 Compatibility is the Wizard's prerequisite**, and the Wizard is in
  turn what the AI layer has to describe.

## Risks

- **3.4 is XL and may be its own sub-project.** Three separate filesystems,
  each with its own undocumented format. It sits here by dependency; its real
  size may warrant splitting it out.
- **6.1 Compatibility Engine is a rules base, not an algorithm.** Its value
  depends on the quality of the knowledge in it, which is research rather than
  engineering, and it is easy to underestimate.
- **0.2 deletes a working code path.** `core/adf/mutate.rs` is audited
  (ART-009 … ART-013) and its tests are the evidence for behaviour the new
  path must keep. Those tests move rather than disappear.

## Notes

This repository is not under git (no `.git`), so this document is written but
not committed. `CONTRIBUTING.md` still refers to branches and PRs; that is one
of the stale claims 0.3 should settle.
