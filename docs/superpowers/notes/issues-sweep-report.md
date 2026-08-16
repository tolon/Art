# ISSUES.md / CHANGELOG.md sweep — report

Scope: `docs/ISSUES.md` and `CHANGELOG.md` only. Did not touch `docs/STATUS.md`,
`docs/FEATURES.md`, `CLAUDE.md`, `docs/architecture.md` or
`docs/sd-appliance-gap-analysis.md` — `docs/architecture.md` was modified by a
concurrent session during this pass (visible in `git diff --stat`) and was left
alone.

## 1. Open section audit — all six specifically named entries checked against code

- **ART-113 / ART-116** — closed correctly. `CoreError::NonAsciiPfs3Names` and
  the `comments_lost`/`dates_lost` fields on `CopySummary` both exist exactly
  as the Fixed entries describe (`src-tauri/src/core/error.rs:86`,
  `src-tauri/src/core/preload/mod.rs`). Entries already in Fixed, worded
  correctly — no action needed.
- **ART-114** — closed correctly. `scripts/pfs3-oracle-check.py` names Windows
  reserved device basenames as their own `skipped` category, matching the
  entry. Already in Fixed.
- **ART-119** — verified line by line against the live code. `#3`
  (`src/lib/osinstall.test.ts:105`) asserts `rc.condition.condition ===
  "rom-older-than"` exactly as claimed; `#4`
  (`src/components/osbuilder/OsInstall.tsx:525`) has the
  `!def.required && def.available` gate restored. `#1`, `#2`, `#5` remain
  untouched in the code, matching "left open." The entry accurately says
  which two of five are closed and which three remain — no correction needed.
- **ART-120** — reads as one coherent closed entry, not two stitched halves.
  The "decision taken" / "never silent" / "core stayed free of the choice"
  narrative plus the mutation-checked properties all read as a single
  filed-and-fixed story. `commands/preload.rs::run_with_fallback` exists and
  matches the description.
- **ART-121** — filed and closed at the end of the wave, folding the review
  findings into one entry the same way ART-119 folds its five. Reads
  correctly.
- **ART-115** — still open and undiagnosed, and nothing about it has gone
  stale: `git log --oneline -- src-tauri/src/core/iso/mod.rs` shows no commits
  since ART-075 (before the flake was even filed), so the "next person should
  save the panic message" guidance is still the right advice.

No entry marked Fixed turned out not to be, and no entry marked Open turned
out to already be resolved. The concurrent fix-wave sessions had already left
`docs/ISSUES.md` in the state this audit was asked to verify — this pass is a
confirmation, not a correction, for all six.

## 2. Rot in older Fixed entries

Found one real instance: **ART-034** and **ART-035** (both 🔴, Stage R oracle
findings) still named `core/adf/mutate.rs` as a location with no note that the
file is gone. Every sibling entry that names the same deleted file — ART-007,
ART-008, ART-011, ART-012, ART-013, ART-047, ART-048 — already carries a
`(Location updated: ... deleted when task 10 retired it — fixed, not
reopened.)` annotation; ART-034/035 were the two stragglers. Added the same
annotation, wording unchanged otherwise (`docs/ISSUES.md:2494`, `:2508`).

Checked and found *not* stale: `hst-imager`-as-requirement claims across the
Fixed section (ART-084's RDB driver history, ART-095, etc.) are all correctly
in past tense describing what was true when found; the one place a *current*
claim about PFS3/hst-imager exists is ART-121's own description of what it
corrected (the two scope-badge strings), which is accurate to today. No
`core/adf/mutate.rs` references remain outside history sections; no other
deleted-file or reversed-rule references found in a full pass over the file's
`ART-NNN` bodies.

## 3. Navigability

- **Severity/checkmark inconsistency, fixed.** Nine of the newest Fixed
  entries (ART-099, ART-111–ART-114, ART-116, ART-120, ART-121) used `✅` in
  place of a severity emoji, breaking the document's own stated `Format:
  ART-NNN · severity · ...` line and making them invisible to a `grep 🟠` or
  `grep 🔴` sweep for defect class. Restored a severity marker to each,
  judged from the entry's own content (🟠 for ART-112/113/120 — real
  functionality broken or unreachable in a way users would hit; 🟡 for
  ART-111/121 — real but narrower gaps; 🔵 for ART-099/114/116 — hygiene,
  a false alarm, or an external tool's own defect). Kept `✅` alongside
  severity rather than dropping it — it carries real information (found and
  closed in the same pass) the severity alone doesn't — and added one line to
  the severity key explaining what `✅` means, since the key previously
  didn't mention it at all.
- **Severity key otherwise still accurate** — the four-row table and its
  wording match how entries actually use it.
- **No duplicate ART-NNN IDs** — checked every heading across the file.
- **Did not split Fixed into its own file.** Considered it — the Fixed
  section is ~2260 of the file's 2838 lines against ~540 for Open — but Open
  already sits first, right after a two-line severity key, so a session
  scanning "what's broken now" hits it in the first screen without wading
  through history; splitting wouldn't shorten that path further. Against it:
  9 internal `(#fixed)`/`(#open)` anchor links and roughly 70 references to
  `ISSUES.md` from other docs would all need auditing for cross-file
  correctness, for a file that isn't actually hard to navigate today (its own
  `## Open` / `## Fixed` headers are exactly what `grep -n "^## "` needs).
  Judged not worth the churn; noted here in case a future session disagrees
  once the Fixed section grows further.

## 4. CHANGELOG.md

Read in full. The `[Unreleased]` section's two most recent entries — "hst-imager
is no longer required to prepare a card" and "Install AmigaOS 3.2 from your own
media" — both match what actually shipped (cross-checked against
`commands/preload.rs`, `core/error.rs`, and the ISSUES.md entries above) and
already state plainly what was *not* re-verified (the 969/106 non-ASCII figures
not re-run through the new fallback path). No correction needed there — this
was clearly done carefully in the fix-wave's own review pass (see
`docs/superpowers/notes/fixwave-review-report.md` point 2).

Structure is consistent throughout: 43 `###` dated sections, `#### Added` /
`Changed` / `Fixed` / `Known` sub-headers used the same way from
2026-08-08 through 2026-08-16, newest first. No stale file/function references
found (the one `mutate.rs` mention, in the 2026-08-09 "Data-safety &
correctness hardening" section, is describing what was true that day — the
same historical-past-tense pattern as ISSUES.md's older Fixed entries — left
as is). No edits made to this file.

## Verification

- `pnpm test` — 44 files, 526 tests, all pass (run after the edits above).
- Every `ART-NNN` cross-reference added or touched (`ART-047`, `ART-048`)
  resolves to a real entry — checked by grep.
- `git diff --stat` shows only `docs/ISSUES.md` changed by this session
  (`docs/architecture.md` also shows as modified — that is the concurrent
  architecture-doc session, not this one, and was left untouched and unstaged
  by this commit).

## Could not resolve / left for a future session

Nothing from the required checklist. The one open item worth flagging: the
severity I assigned to the nine `✅` entries is a judgment call (the document
itself doesn't define how to grade an entry that bundles several findings of
different severities, e.g. ART-121 or ART-119) — reasonable people could pick
a notch differently. If a future session disagrees with a specific one, the
fix is a one-character edit, not a design question.
