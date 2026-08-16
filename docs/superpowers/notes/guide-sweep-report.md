# Guide sweep — G5 / preload native / libpfs3, 2026-08-16

Scope: `CLAUDE.md` (outside the repo, untracked), `docs/architecture.md`,
`docs/sd-appliance-gap-analysis.md`, `docs/roadmap.md`. Read
`docs/superpowers/specs/2026-08-15-os-install-design.md`,
`src-tauri/src/core/osinstall/mod.rs`, `src-tauri/src/core/preload/mod.rs`,
`src-tauri/src/core/preload/native.rs`, `src-tauri/src/commands/preload.rs`
and the current `docs/STATUS.md`/`docs/ISSUES.md` entries before writing.

## What was corrected

**CLAUDE.md** (edited, not committed — it lives one level above the repo):

- The core-dependency sentence already named `libpfs3` in its `+`-chain (a
  prior session had fixed that), but the clause explaining its licence
  pointed at "the preference below" — a dangling reference; nothing below it
  in this file states a preference. Reworded to be self-contained
  (`deny.toml`).
- Added `scripts/pfs3-oracle-check.py` to the Commands block, alongside the
  other three local-only oracle scripts it belongs next to.
- Added a paragraph noting `src-tauri/Cargo.lock` is tracked, not ignored,
  and why that started mattering on 2026-08-16 (`libpfs3`, the first
  copyleft dependency inside `core/`) — the task's instruction that the
  Commands section "should not imply otherwise" had nothing to contradict
  today, but nothing stated it either.
- Added two new architecture subsections after "A card is a list of disks,
  not a disk": **"Installing an OS: a component is a set of paths, not a
  disk"** (the `ModulesA1200_3.2.adf` measurement — 14 commands in `C/`, 13
  are older `Workbench3.2` copies — and the distribution-tree/`.uaem`/
  `distribution.json` shape) and **"Two ways to write a PiStorm volume:
  native by default, a named fallback"** (`VolumeFormatter`'s two
  implementations, ART-113/ART-117 as the only two fallback triggers, never
  silent).

**docs/architecture.md** (committed):

- The core-dependency sentence attributed `libpfs3` solely to "SD-2 G5's OS
  install" — narrower than reality; it is `core/preload` (G3 route native)
  that owns the writer, with G5 as its largest consumer. Corrected.
- The project-structure tree was missing `tools/`, `core/mbr.rs`,
  `core/fat32.rs`, `core/card/`, `core/distro/`, `core/osinstall/` and
  `core/preload/` entirely — stale beyond just this task's scope (it never
  caught up with SD-1's card work either), but all seven exist and are
  central to what a new session needs to find. Added.
- Added `core/preload::VolumeFormatter` as the concrete, current illustration
  of the core-independence rule's generic trait pattern, next to the existing
  abstract sentence.

**docs/sd-appliance-gap-analysis.md** (committed) — the largest correction:

- **G5** was still marked 🟧 "owed" describing an unbuilt gap. Rewrote to
  ✅-adjacent status (kept 🟧 since it is not fully closed) with a dated
  status block: what is built and proven (the engine, run against the user's
  real 3.2 media — 4030 files / 330 directories to a tree, 3061 onto a native
  PFS3 volume verified byte-identical by `hst-imager`), and three precise
  remaining items — no Amiga has booted anything G5 built, the OS Builder
  screen is unverified past its headings (ART-118), and 3.9/3.2.1/3.2.2 have
  no recipe — plus a note that G9/G10 are separate gaps, not sub-items of G5.
- **G3** was still marked 🟥 blocker, recommending "Route D" (WinUAE-assisted)
  as v1 and describing PFS3 write as something "ART does not have in any
  form." Both false since 2026-08-15: Route E (`hst-imager`) shipped first,
  Route D was never built (proved unnecessary), and native Route B
  (`libpfs3`) landed as the *default* writer. Rewrote the status header, the
  "Recommendation" paragraph (now "what actually happened"), and a stray
  present-tense claim in the body that PFS3 write "ART does not have in any
  form."
- Fixed the "Positioning" paragraph's "(c) longer term, native AmigaOS
  filesystem engines (G3 Route B)" — no longer "longer term"; landed.
- Fixed the phasing table: SD-2's line still said "G3 Route D
  (WinUAE-assisted PFS3 format+fill)"; SD-4's line still said "G3 Route B —
  native PFS3 write, own brief" as a *future* flagship phase, when B shipped
  in SD-2 instead. Rewrote both lines with dates and an honest note that
  SD-4's slot is now unclaimed pending the boot rungs.

**docs/roadmap.md** (committed):

- It pointed to STATUS.md for "current position" but never stated the
  relationship STATUS.md itself declares ("this supersedes the phase
  numbering in `roadmap.md` for scheduling purposes"). Added one paragraph
  making that explicit and non-competing: the Phase 0–7 spec still defines
  what a phase *contains*; STATUS.md's Stage plan (SD-0…SD-5) defines
  current *order*.

## Found stale, left alone, and why

- **G9's and G10's own section markers (🟧) and G11's (🟨)** were not
  touched. STATUS.md already says G11 "landed, engine and screen," which
  would make its 🟨 stale too — but G11 is unrelated to this session's
  design doc (layout policy, not OS install/preload), and rewriting its
  section risked scope creep beyond the four files' brief. Left for a
  dedicated pass.
- **G0's "one blocker-grade unknown… verify the result with ART's readers
  and WinUAE"** (line ~116) is dated 2026-08-12 framing of what G0 handed
  forward. The reader half is now answered (`pfs3-oracle-check.py`); WinUAE
  is not. Left as historical record of that moment rather than rewritten,
  since G3/G5's own new status blocks already state the current WinUAE gap
  precisely.
- **`docs/licenses.md`** was not touched — out of the four assigned files,
  and a prior session (commit `fb57598`) already corrected it for the same
  `libpfs3` licence-inventory rot this sweep was watching for.

## Verification

Every cited path was checked with `ls`/`grep` against the actual tree:
`core/osinstall/`, `core/osinstall/recipes/amigaos-3.2.json`,
`core/preload/native.rs`, `core/preload/pfs3dev.rs`, `core/card/`,
`core/distro/`, `core/mbr.rs`, `core/fat32.rs`,
`src-tauri/src/tools/hst_imager.rs`, `scripts/pfs3-oracle-check.py`, and the
issue IDs cited (ART-113 ✅, ART-117 open, ART-118 open, ART-120 ✅) against
`docs/ISSUES.md`'s current entries.

No commands were run beyond `git`/`grep`/`ls` — Markdown only, per the task.
