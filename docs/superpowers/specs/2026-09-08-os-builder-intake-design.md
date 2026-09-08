# OS Builder intake — one material folder, named slots, and a readout that says how it knows

**Date:** 2026-09-08
**Status:** design for round 2 of [`2026-09-08-os-builder-intake-research.md`](2026-09-08-os-builder-intake-research.md) § 7
**Owner's decisions bound here:** all three of research § 8 — the BoingBag ruling stands (no
password code), the chain on screen covers the whole material, the Amiga-side step stays a
wizard step and becomes the chain screen (round 3).

---

## 1. What this round fixes, in the owner's words

*"Which archive goes where."* Today the OS Builder asks for the material in four places — the
`kaynak` step's media folder and extra folders, the `paketler` step's package folder, and the
Amiga-side panel's three browse buttons (the package's archive, its second archive, the CD
image) — and each place resolves it separately, by a different rule. A person who has the
files has to know which sentence each field wants. The research (§ 3) found the answer every
established project converged on: **one folder, every artefact resolved once into a named
slot, and a per-artefact readout saying found / how it was identified / where it came from /
not found and whether that matters.**

Round 1 fixed the two defects the owner hit (ART-276, ART-277). This round removes the reason
they could happen.

---

## 2. What was measured before this was designed

- **ART's identification already exists and is good.** `core/osinstall/mediahash.rs` hashes
  every candidate in a folder (MD5, cached by `(path, size, mtime)`), looks the hash up in a
  186-row table, and the `kaynak` step renders one line per file with four distinct endings
  (`confirmed` / `unconfirmed` / `not-in-table` / `unreadable`; `src/lib/osinstall.ts::
  mediaIdentityLines`). `scan::find_media` reads a disk's volume name from its root block;
  `scan::find_packages` reads an archive's single top-level directory. **None of it is
  replaced.** This round joins them.
- **The adopted table already carries seven AmigaOS 3.9 rows — corrected 2026-09-08, in
  place.** This paragraph first said the table had none; a `grep` for `BoingBag` and `iso`
  found nothing because Hatcher names them `AmigaOS3_9`, `AmigaOS3_9BB1`, `AmigaOS3_9BB2`,
  `AmigaOS3_9BB34` under source `Haage and Partners (3.9)`: four ISO masterings (including the
  owner's `3cb96e77…`), BoingBag 1 (the pre-fix `71353d4a…`), BoingBag 2 and BoingBags 3&4.
  Round 2's first task found it by a test — `no_md5_is_shared_between_the_adopted_and_own
  _tables` — when the eleven rows below were first all written into ART's own table. So ART's
  own table ships **eight** rows (the 45.15 BoingBag 1, the UAE fix, Contribution, the Turkish
  update, Euro-Update, Locale 3.9, GenesisPrefs, and HstWB's other ISO hash `e32a107e…`), not
  twelve; the four the adopted table already had stay there. `candidates_in` already hashes
  `adf`, `iso` and `lha` (`MEDIA_EXTENSIONS`, `mediahash.rs:430`). The lesson is the one the
  research already recorded once tonight: a search that finds nothing is a claim about the
  search.
- **HstWB's table (MIT) and the owner's files agree where they overlap**, measured 2026-09-08:

  | Artefact | Owner's file | MD5 | In HstWB's CSV |
  |---|---|---|---|
  | AmigaOS 3.9 CD | `AmigaOS39.iso` (490 856 448 B) | `3cb96e77d922a4f8eb696e525a240448` | yes (its second accepted hash; the first, `e32a107e…`, is another mastering) |
  | BoingBag 1, pre-fix `Updater` 45.13 | `BoingBag39-1.lha` (5 254 174 B) | `71353d4aeb9af1f129545618d013a8c8` | yes |
  | BoingBag 1, fixed `Updater` 45.15 | `BoingBag39-1 (1).lha` (5 254 220 B) | `ef67ce2f786044dae1bce8fafb439d5e` | **no** |
  | BoingBag 1 UAE fix | `BoingBag39-1-UAE.lha` | `e00a91cd6800d6a34821a156c44b1c2f` | no |
  | BoingBag 2 | `BoingBag39-2.lha` (2 053 444 B) | `fd45d24bb408203883a4c9a56e968e28` | yes |
  | Contribution | `BoingBag39-2-Contribution.lha` | `a7dcf701e9b9c77cb5ad51f679fa41e2` | no |
  | Turkish locale update | `BoingBag39-2-turkce.lha` | `c7f97fc909d3a135c48ecefb9cc948d2` | no |
  | BoingBags 3&4 v1.59 | `BoingBags3&4.lha` | `4e48f80573554385a6fbc274d8ae328b` | no |
  | Euro-Update | `Euro-Update.lha` | `1c1a21e1f5c95dacd4dfa0c1fa6e244e` | no |
  | Locale 3.9 | `Locale3_9.lha` | `ed08b553fa3f150d2262ab657dc5484b` | no |
  | GenesisPrefs | `GenesisPrefs.lha` | `8d5a778c19b0214fa3e7318f44e68c5c` | no |

  Three sources agree on the four rows Hatcher also carries; the rest are the owner's own files and are recorded
  as such (`source: "the owner's copy, 2026-09-08"`), unconfirmed by a second source — the
  honest label `mediahash.rs` already has for 151 of its 186 rows.
- **A medium may have several accepted hashes.** HstWB accepts two for the 3.9 ISO; ten of its
  logical media accept more than one. ART's table is one row per hash already (a row *is* a
  hash), so "several hashes, one artefact" is a matter of several rows sharing an `artefact`
  id — no schema change beyond that field.
- **Amiga Forever is on this machine and findable without asking**: `AMIGAFOREVERDATA=E:\amiga\`,
  and `E:\amiga\Shared\{adf,rom,hdf,dir,nvram}` exist (`Shared\adf` holds `amiga-os-*.adf`,
  `Shared\rom` the ROMs). The registry key is under `HKLM\SOFTWARE\WOW6432Node\Cloanto\Amiga
  Forever`, not the 64-bit hive.
- **The archive's own `Updater` version decides whether the UAE fix is needed**, not the user:
  `BoingBag39-1.lha` carries `Updater` 25 588 B dated 2001-04-03 (CRC `0000B9D4`),
  `BoingBag39-1 (1).lha` and the UAE archive both carry 25 732 B, 2001-04-17 (CRC `00009C29`),
  and the UAE readme's condition is "downloaded before 20-4-2001". `boingbag-39-1.json`
  already declares `minimum_version: "45.15"` and the panel already shows `overlay.needed`;
  what changes is that the fix becomes a slot ART fills by itself when the folder has it.

---

## 3. The design

### 3.1 One material folder list, for the whole build

The build session (`src/lib/buildSession.ts`) gains **one** `material: { folders: string[] }`
that replaces three things a user is asked for today: `media.folder`, the extra media folders
(`osinstall.extraMediaFolders.<release>`), and `packages.folder`. Migration is the session's
existing pattern (`LEGACY_KEYS`, "the order is the migration"): the first non-empty of the
three old keys seeds the list, in that order, deduplicated; the old keys are read once and never
written again. **Nothing the user chose is lost, and nothing is applied that they did not
choose** — a suggested folder (§ 3.5) is shown as a suggestion until confirmed.

Layers (`amigaos-3.2.2.json`'s update folders) keep their meaning: a folder in the list may be
tagged with the layer it holds, as today; an untagged folder is scanned for everything.

### 3.2 Slots — what a release needs, resolved once

A **slot** is one artefact a release's build can use. The list of slots is **data derived from
the recipes**, never a list written in code:

| Slot kind | Derived from | Example |
|---|---|---|
| medium | every `MediaLayer` volume the release's components read from | `AmigaOS3.9:` (the CD), `Workbench3.2:` … |
| package | every shipped package whose `releases` includes this release | `boingbag-39-1`, `locale-39-turkish` |
| overlay | every `amiga_installer.overlays[].from` of those packages | BoingBag 1's UAE fix |
| rom | the release's ROM condition | a Kickstart ≥ V40 |

Each slot carries: `id`, `kind`, `required` (a medium the required components read from, or a
package another chosen package `requires`) or `optional`, the **expected filenames** as the
established projects and the owner's files name them (data on the row: `filenames:
["BoingBag39-1.lha"]`, several allowed), the `provenance` text ("Amiga, Inc. and Haage &
Partner; amiga.com, 2001"), and the **order position** in the chain (§ 3.6 of the research;
BB1 = 2, BB2 = 3, Locale = 4 …), so the readout can be sorted the way the material installs.

**Resolution** (`core/osinstall/slots.rs`, new, pure): given the folders' scan results —
`find_media` (volume names), `find_packages` (top-level directories), `mediahash` (hashes, now
over `.iso` and `.lha` too) — and the tree's `distribution.json` when a tree is chosen, produce
one `SlotState` per slot:

```
SlotState {
  slot: SlotId,
  found: Option<{ path, matched_by: Hash | VolumeName | TopLevelDirectory | Filename,
                  row: Option<MediaRow>, confirmed: Option<Confirmation> }>,
  candidates: Vec<path>,              // >1 = ambiguous, and the readout says so
  installed: Option<InstalledRecord>, // from distribution.json: date, hash, by which run
  blocked_by: Option<SlotId>,         // order: BB2 waits for BB1
}
```

`matched_by` is ranked the way HstWB ranks it and for the same reason: a hash is a fact about
the bytes, a volume name or top-level directory is what the medium says about itself, a
filename is a guess. **A filename match is shown as a guess** (yellow), never as a find, and it
is never enough to run anything — it exists so the readout can say *"a file called
`BoingBag39-2.lha` is here but its bytes are not what ART knows; ART opened it and it calls
itself `BoingBag3.9-2`"*, which is two facts, not one.

`MediaMatch::Ambiguous` keeps its rule: two candidates for one slot is a state the readout
shows and the user resolves by picking, never an arbitrary winner.

### 3.3 The readout

One component, `MaterialReadout.tsx`, rendered on the `kaynak` step and on the Amiga-side
panel (round 3 makes it the chain screen's spine). One row per slot, in chain order:

```
✔  AmigaOS 3.9 CD          AmigaOS39.iso        by hash · Amiga, Inc. and H&P · installed 2026-09-07
✔  BoingBag 3.9-1          BoingBag39-1 (1).lha by hash · Updater 45.15, no fix needed · installed
—  BoingBag 3.9-1 UAE fix  not needed           (this copy already carries Updater 45.15)
✔  BoingBag 3.9-2          BoingBag39-2.lha     by hash · ready
!  Locale 3.9              Locale3_9.lha        by its own name, not in the table · ready
✖  BoingBags 3&4           not found            optional · expected BoingBags3&4.lha · needs BB2 first
```

and above it the set line HstWB shows: **`AmigaOS 3.9 · 5 of 6 found · 1 optional missing`**,
green when every required slot is found, red naming how many required are missing. Required
and optional are counted separately, so a set missing only optional files is green.

Every row's sentence is a `Phrase` from `src/lib` (two catalogues), and the four endings stay
distinct per row: found-by-hash, found-by-its-own-name, guessed-by-filename, not found; plus
`installed` as a state from the manifest, never inferred from a file being present.

### 3.4 The Amiga-side panel reads slots

The panel's three browse buttons go. The package's archive, its overlay and the CD image are
**pre-filled from the slots**; a field is shown only when its slot is `not found` or
`ambiguous`, and then it says what it wants in the slot's own words (*"expected
`BoingBag39-1-UAE.lha`; only needed because your BoingBag 1 carries `Updater` 45.13"*). The
per-package remembered keys from round 1 stay as the override: a file the user picked by hand
wins over the slot, and the readout says *chosen by you* for it.

### 3.5 Amiga Forever, offered

When the material list is empty, ART **offers** `%AMIGAFOREVERDATA%\Shared\adf` and
`Shared\rom` as one suggestion line with an *Add* button, if the variable is set and the
folders exist (the registry key is a second signal; the environment variable alone suffices).
Never added silently: `remembered.ts`'s rule.

### 3.6 The table grows, and its check script with it

- **A second table file, ART's own.** `mediahash.rs` keeps the adopted table exactly as
  adopted (its own rule: a reader must be able to tell whose claim a row is). So the eleven
  rows of § 2 go into `media_hashes_own.json`, each with `source` ("HstWB Installer
  `amiga-os-entries.csv` (MIT)" for the three it also carries, "the owner's copy,
  2026-09-08" for the rest), `artefact` (the slot id) and `kind` (`disc` | `archive`);
  `rows()` serves both files, and a row's provenance sentence names its file. The adopted
  rows gain nothing; `artefact` for them is derived from `volume` + `version` by the
  existing name mapping, not written into Hatcher's data.
- `candidates_in` already hashes `.iso` and `.lha`; add `lzh`, `zip`, `7z` (what
  `find_packages` opens) so a re-packed archive is at least reported. An ISO is 490 MB:
  hashed once, cached against `(path, size, mtime)` like everything else, in the background
  job that already exists.
- `scripts/media-table-check.py DIR` learns the new kinds and reports, for a directory of
  real material, which rows matched — the re-runnable check the table's own design demands.
- **Structural check beside the hash** for the CD (HstWB's six directories): a slot for a
  disc that matched by hash *and* lacks `OS-Version3.9`, `Emergency-Boot`, `Contribution` … is
  reported as *matched but incomplete* — a different sentence from *not found*.

### 3.7 The drop-folder text, on request

A button on the readout, **"Write a guide into this folder"**, writes one text file per
folder — `ART - what goes here.txt` in the UI language — listing every slot: REQUIRED /
OPTIONAL, the expected filenames, the provenance and where to obtain it, the order, and what
happens without it. Generated from the slot list, so it cannot drift from what the code
accepts. **Only on request** — ART does not write into a user's folder because they pointed
at it. The SD lane (round 3 or later) puts the same text onto the FAT32 partition.

---

## 4. What this round does not do

- It does not run anything. Resolution is read-only; the run is the Amiga-side panel's and
  round 3 turns that panel into the chain screen.
- It does not fetch. No download of a table, no download of material; `homepage`/provenance
  text is where the *user* goes.
- It does not retire `find_media`, `find_packages` or the layer mechanism; it composes them.
- It does not touch the password ruling.
- It does not decide for the user between two candidates.

---

## 5. Verification

- `slots.rs`: pure tests — every slot kind derived from a synthetic recipe; each `matched_by`
  rank chosen over the next when both apply; ambiguity preserved; `installed` read only from a
  manifest; `blocked_by` follows `requires`; a filename match never yields `found` alone.
- `mediahash`: `.iso`/`.lha` hashed; the eleven rows looked up by their own MD5 (the table's
  `rows()` test pattern); `media-table-check.py` run over `E:\amiga\Amigatolon\os39\` by the
  owner and its line quoted in STATUS.
- Session migration: the three legacy keys seed `material.folders` in order, once; a key the
  user touched this run is never overwritten (ART-089's test pattern).
- `MaterialReadout.test.tsx`: the six row endings render their own sentences in both
  languages; the set line counts required and optional apart; ambiguous shows both candidates.
- Amiga-side panel: fields hidden when slots are found; shown with the slot's sentence when
  not; a hand-picked file overrides and is labelled.
- **A person drives it**: the owner points the readout at `E:\amiga\Amigatolon\os39` and reads
  the screen; the transcript goes in the session log.

---

## 6. Known risks

- Hashing a 490 MB ISO the first time costs seconds; the job already reports progress and the
  cache makes it once. If the owner's folder holds several ISOs, say so in the readout while
  it runs ("hashing 2 of 3") rather than a bar with no total.
- `find_packages` opens every archive in a folder; a folder of 200 `.lha` files (an Aminet
  mirror) is not a material folder, and the readout should say *"this folder holds 200
  archives; ART looked at them all"* rather than hang silently. Bound the count and name the
  bound.
- The recipes' `filenames` data is a convention, not a fact about the user's disk; it is only
  ever used for the *guess* rank and for the guide text.
