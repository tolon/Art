# Debt round 1 — records cleanup and filing Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make `docs/ISSUES.md` and `docs/STATUS.md` say what is true on `main` at `bc479b6`: close the stale ART-118, and file the five PFS3 differences and the local-time defect that the 2026-09-11 Windows research found but `main` never recorded, each with the owner's 2026-09-14 decision.

**Architecture:** Documentation only. No code changes. Every later debt plan (2 ART-302/300, 3 PFS3, 4 ART-301, 5 local time) moves the entry this plan files, so this plan runs first.

**Tech Stack:** Markdown; `python scripts/control-byte-sweep.py`.

**Spec:** the owner's decisions of 2026-09-14 in this session, and the research collected in `docs/superpowers/notes/2026-09-14-debt-round-research.md` (Task 1 creates it from the scratch notes).

## Global Constraints

- Branch `art-debt-0914` (cut from `main` at `bc479b6`). Run `git branch --show-current` before every commit.
- Commit messages go through a file and `git commit -F` (CLAUDE.md "Before you commit").
- Never write a claim nobody checked: every finding below carries its source (file:line, pfs3aio commit `211f7f0`, or the owner's word) and whether it was run or only read.
- ART ids are stable; the highest existing is ART-312. Re-check with `rg -o "ART-3\d\d" docs/ISSUES.md | sort -u` before filing.
- ISSUES entries follow the house shape: `**ART-NNN** <severity> **<title>** — *found …*` then the file, then the evidence; Open at the top of `## Open`.

---

### Task 1: The research note and the new entries

**Files:**
- Create: `docs/superpowers/notes/2026-09-14-debt-round-research.md`
- Modify: `docs/ISSUES.md` (`## Open`, top)

**Interfaces:**
- Produces: ids ART-313 … ART-318 with exactly these subjects, used by plans 3 and 5:
  - ART-313 🟠 PFS3 directory `parent` fields (D3)
  - ART-314 🟠 PFS3 names longer than 31 bytes on an `fnsize` 32 volume (D14)
  - ART-315 🟡 PFS3 data allocator bound (D8)
  - ART-316 🔵 PFS3 `enable_deldir` never read (D4)
  - ART-317 🟡 every Amiga date ART writes is UTC, the Amiga reads local time (D7)
  - (ART-311 keeps its id; its scope text changes in Step 4)

- [ ] **Step 1: Write the research note**

Copy `D:\Projeler\Amiga\scratch-0913\plan-draft-records.md` into `docs/superpowers/notes/2026-09-14-debt-round-research.md`, retitle it `# Debt round after 0.9.3 — research (2026-09-14)`, and add under the title:

```markdown
*Collected 2026-09-14 by four read-only research agents over this repository and one over pfs3aio master
`211f7f0` (https://github.com/tonioni/pfs3aio) and hst-amiga `main`. Nothing here was run unless it says so.
The owner's decisions of the same day: D7 local time everywhere; ART-311 both modes; D4 implement the deldir;
D14 format `fnsize` 107; separate plans, executed by subagents.*
```

Delete the "Proposed task" section from the copy (this plan replaces it).

- [ ] **Step 2: File ART-313 … ART-317 at the top of `## Open`**

Insert directly after the `## Open` heading and its blank line:

```markdown
**ART-313** 🟠 **PFS3 directories ART writes carry the wrong `parent`: the Amiga's handler cannot find a
file's parent directory** — *found 2026-09-11 by the Windows machine's ART-310 research (D3); verified against
pfs3aio source 2026-09-14; filed 2026-09-14*
`src-tauri/vendor/libpfs3/src/format.rs:321-326` · `src-tauri/vendor/libpfs3/src/writer.rs:1135` · In pfs3aio a
directory block's `parent` is the anode of the directory that *contains* the block's directory, and `0` marks the
root's own blocks: `format.c:548` writes the root with parent 0, `directory.c:1653,1707` gives a subdirectory of
the root parent 5, and a continuation block copies the directory's parent (`directory.c:3176,3204,3392`).
`GetParent` reads the containing block's `parent` and treats 0 as "in root" (`directory.c:645-654`). `libpfs3`
writes the root with parent 5 (`format.rs:321-326`) and gives every continuation block the directory's *own*
anode (`writer.rs:1135`). `libpfs3`'s reader never reads `parent`, which is why ART never saw it.
**How it hurts a user:** on the Amiga, asking for the parent of anything in the root, or of anything in a
directory large enough to need a second block, goes wrong (reading-derived from pfs3aio's source — not run under
a real handler). **Decided 2026-09-14:** fix both in the vendored copy (plan 3).

**ART-314** 🟠 **A PFS3 name longer than 31 bytes lists on the Amiga but cannot be opened by name** — *found
2026-09-11 (D14); verified against pfs3aio source 2026-09-14; filed 2026-09-14*
`src-tauri/vendor/libpfs3/src/format.rs:239` · `src-tauri/vendor/libpfs3/src/writer.rs:1165` ·
`src-tauri/src/core/preload/native.rs:839` · The format writes `fnsize` 32 (pfs3aio's own default,
`format.c:520`); the writer accepts names up to 107 bytes and silently cuts longer ones (`writer.rs:1165`); ART
checks only non-ASCII names. pfs3aio truncates a *search* name to `fnsize - 1` (`directory.c:721-722`) and its
compare needs equal lengths (`assroutines.c:163`), so a 32–107-byte name is listed but never matched
(reading-derived, not run). hst-imager formats `fnsize` 107 (`Pfs3Formatter.cs:297`). **Decided 2026-09-14:**
format `fnsize` 107, and refuse a name longer than 107 bytes by name instead of cutting it (plan 3).

**ART-315** 🟡 **`libpfs3`'s data allocator accepts a block number up to `bitmapstart` past the partition** —
*found 2026-09-11 (D8); verified against pfs3aio source 2026-09-14; filed 2026-09-14*
`src-tauri/vendor/libpfs3/src/writer.rs:776-786` · The bound is `disksize + bitmapstart`; valid data blocks are
`[bitmapstart, disksize)` (`format.rs:120,128,193`), and pfs3aio bounds on the partition's block count
(`allocation.c:344`, `volume.c:637`). ART's own format clears the bitmap's tail bits (`format.rs:268-278`), so
only a volume formatted elsewhere (pfs3aio and hst-imager leave the tail free, `allocation.c:1054-1055`) near full
can reach it, and ART's device refuses the write (`core/volume/device.rs:336-370`) — an error, not corruption.
Not run. **To fix in plan 3.**

**ART-316** 🔵 **`libpfs3`'s `FormatOptions.enable_deldir` is never read** — *found 2026-09-11 (D4); filed
2026-09-14*
`src-tauri/vendor/libpfs3/src/format.rs:25-28,101-111` · The option silently does nothing; ART passes `false`
(`native.rs:189-192`, `sizing.rs:666`). pfs3aio formats a two-block deldir with `MODE_DELDIR | MODE_SUPERDELDIR`
(`format.c:252-255`, `directory.c:4442-4480,4572-4637`); a volume without one is valid (`init.c:642-643`).
**Decided 2026-09-14:** implement the deldir in the vendored format (plan 3); whether ART turns it on for cards
is a separate choice recorded there.

**ART-317** 🟡 **Every Amiga date ART writes is UTC; the Amiga reads it as local time** — *found 2026-09-11
(D7, measured on the Windows run: libpfs3 entries 18:15 beside hst-imager's 21:15 on a UTC+3 machine); scope
widened 2026-09-14; filed 2026-09-14*
`src-tauri/vendor/libpfs3/src/util.rs:163-172` · `src-tauri/src/core/volume/write/layout.rs:104-120` ·
`src-tauri/src/core/adf/bcpl.rs:5-6` · `src-tauri/src/core/preload/native.rs:642-655` ·
`src-tauri/src/core/adf/create.rs:192-204` · `src-tauri/src/core/adf/mutate.rs` ·
`src-tauri/src/core/volume/write/copy.rs:380-388` · AmigaDOS `DateStamp()` is local time with no zone (pfs3aio
stamps with it, `directory.c:3540`, `format.c:393`). ART computes every Amiga date as UTC seconds − 252 460 800:
libpfs3, ART's FFS/OFS writer, the RDB and ADF paths, and the host-mtime fallback. Nothing in ART obtains the
local offset; `core/` may not call a Windows API. **How it hurts a user:** every file, directory and volume ART
writes shows a time off by the machine's UTC offset on the Amiga. **Decided 2026-09-14:** local time everywhere,
with the offset obtained outside `core/` (plan 5, design first).
```

- [ ] **Step 3: Verify the ids**

Run: `rg -n "^\*\*ART-31[3-7]\*\*" docs/ISSUES.md`
Expected: exactly five lines, 313 … 317, all above `## Fixed`.

- [ ] **Step 4: Rescope ART-311**

In ART-311's entry, replace the sentence `Not scheduled;
lifting it changes \`core::card::sizing\` and its tests, which is the owner's to schedule.` (it spans a line
break — match on `Not scheduled;`) with:

```markdown
**Scheduled 2026-09-14 by the owner, both modes:** small mode allocates index blocks on demand up to
`MAXSMALLINDEXNR` and writes the rootblock's index union; SUPERINDEX mode allocates super blocks up to `MAXSUPER`
and writes the rootblock extension; the anode search covers pfs3aio's 16-bit seqnr range and roves as
`curranseqnr` does (pfs3aio `anodes.c:389-465,717-760,844-870`, `update.c:247-269`). `core::card::sizing` then
stops pushing many-file content past MAXSMALLDISK (plan 3).
```

- [ ] **Step 5: Sweep and commit**

Run: `python scripts/control-byte-sweep.py` — Expected: `control-byte sweep: clean …`

```text
docs: file ART-313..317 from the ART-310 research; the round's research note
```
(written to a file, `git commit -F`), staging `docs/ISSUES.md` and the new note only.

---

### Task 2: Close ART-118 as superseded; STATUS's person-owed list

**Files:**
- Modify: `docs/ISSUES.md` (ART-118 → top of `## Fixed`)
- Modify: `docs/STATUS.md:391-397`

- [ ] **Step 1: Move ART-118**

Cut the whole ART-118 entry (from `**ART-118** 🟠` to the line before `**ART-117**`) and paste it at the top of
`## Fixed`. Change `🟠 **` to `🟠 ✅ **` in its first line. Append:

```markdown
**Closed 2026-09-14: superseded, not reproduced.** The screen this entry is about no longer exists: the four-tab
rewrite deleted `src/components/osbuilder/OsInstall.tsx` and its test (`docs/STATUS.md`, the four-tabs item;
no `OsInstall*` remains under `src/components/osbuilder/`). Its successor has jsdom coverage per tab
(`FilesTab.test.tsx`, `ChoiceTab.test.tsx`, `MachineTab.test.tsx`, `BuildTab.test.tsx`) and has been driven by
the owner in packaged builds, which found defects and no crash: ART-297 (2026-09-10, `main-f354f46`), ART-303
(2026-09-10, `main-e2633a6`), ART-304 (2026-09-11, `main-706de9f`); the single-column screen before it on
2026-08-22 (`docs/session-log.md`, "The owner drove the OS Builder"). **Not claimed:** the access violation
(`-1073741819`) was a headless Chrome/Edge renderer crash and was never explained; no headless run was repeated.
The Turkish-layout half stays with ART-062.
```

- [ ] **Step 2: STATUS**

In `docs/STATUS.md`, delete item `2. **[ART-118](ISSUES.md) — the OS Builder's install screen in a real window.**`
and its three continuation lines, and change `Neither needs a design decision or more code first.` to
`It needs no design decision or more code first.` Read the lines around it first; if the list's heading says
"two", make it "one".

- [ ] **Step 3: Verify**

Run: `rg -n "ART-118" docs/STATUS.md` — Expected: no match in the person-owed list.
Run: `rg -n "^\*\*ART-118\*\* 🟠 ✅" docs/ISSUES.md` — Expected: one line, below `## Fixed`.

- [ ] **Step 4: Sweep and commit**

`python scripts/control-byte-sweep.py`, then commit `docs: close ART-118 as superseded by the four-tab OS Builder`.

---

### Task 3: ART-117, ART-250 and ART-301's count

**Files:**
- Modify: `docs/ISSUES.md`

- [ ] **Step 1: ART-117** — no change. Its entry already carries the owner's 2026-08-21 decision ("leave it",
  hst-imager the named fallback, "Revisit only if someone actually meets the case"). Record nothing new.
- [ ] **Step 2: ART-250** — no change; still true (no tree ART builds writes NewIcon tool types).
- [ ] **Step 3: ART-301's count** — replace `34 \`let title = format!(…)\` sites across 18 command
  files.` with:

```markdown
33 `let title = format!(…)` sites in 17 command files and 7 fixed English titles — 40 job titles in 19 files
(counted 2026-09-14; the first count, 34 / 18, included a test's `let title` in `artwork.rs` and missed the fixed
titles).
```

- [ ] **Step 4: Commit** `docs: ART-301's real count`.

---

## Self-review

- Every entry the round's decisions name has a filing step (313–317) or a rescope (311). ✔
- No code step. ✔
- ids re-checked at execution (Global Constraints). ✔
