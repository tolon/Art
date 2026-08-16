# STATUS.md / FEATURES.md sweep — 2026-08-16

Scope: `docs/STATUS.md` and `docs/FEATURES.md` only, after SD-2 G5 (the OS
install engine, fourteen tasks) merged to `main` and the ART-120/ART-121 fix
wave landed on top of it. `docs/ISSUES.md`, `CHANGELOG.md`, `CLAUDE.md`,
`docs/architecture.md` and `docs/sd-appliance-gap-analysis.md` were being
swept by other agents concurrently and were not touched here.

## Numbers measured myself, not copied

- `cargo test` (`src-tauri`): **1399 passed, 0 failed, 3 ignored** — run
  twice, back to back, no flake (the flaky `core::iso` test named in
  ART-115 did not reproduce either time).
- `pnpm test`: **526 passed across 44 files**, 0 failed.
- `pnpm lint`: clean (both `tsc --noEmit` passes).
- i18n leaf keys, counted with a small Node script rather than trusted:
  **1457 in `en.json`, 1457 in `tr.json`** — parity holds.
- `core::osinstall` + `commands::osinstall` test count, checked against
  FEATURES.md's own claim: `grep -c '#\[test\]'` gives 124 + 21 = 145,
  matching the row exactly.
- CI on GitHub: confirmed green via `gh run view` on run `31939802790`
  (commit `544282c`) — every step, including `pnpm tauri build` and the
  amitools oracle cross-check, succeeded. The two prior pushes on this
  branch (the `sd-1` merge itself, and the ART-121 fix-wave commit) had
  both failed on the same `clippy::question_mark` lint at
  `commands/card.rs:275`, invisible on this machine because CI runs
  `stable` (clippy 1.97.0) against a local toolchain pinned at 0.1.95. I
  waited for the run to finish rather than claiming CI green from a clean
  local `cargo clippy`, which is exactly what would have missed this.

All of the above matched what the previous wave's own commits (`b1751da`,
the ART-121 commit) already claimed in the Snapshot table — nothing had
drifted between commits, so the Snapshot itself needed no numeric
correction, only the "Picking up next session" section did.

## What was stale and corrected

- **`docs/STATUS.md`, "Picking up next session"** — rewritten in full. It
  still described the session that finished *before* G5 merged: `sd-1`
  21 commits ahead of `origin/sd-1` and unpushed (false — `sd-1` merged
  to `main` days ago and `main` is now level with `origin/main`); G5, G9
  and G10 all "owed" (false — G5 landed this wave, only G9/G10 remain);
  a "Four things this project keeps re-learning" list explicitly tied to
  "2026-08-15 alone", i.e. a day that had already passed. Replaced with a
  section describing where SD-2 actually stands (G3/native, G11, G5 done;
  G9/G10 owed and now smaller, since G5 unblocks G9), a new "What is
  blocked on the user, not on code" section naming the three things the
  task specified (driving both new screens in `pnpm tauri dev`, a WinUAE
  boot, a real A500 boot), and an updated "What to pick up" list pointing
  at G9/G10, the un-re-run 4030-file tree, ART-115 (the undiagnosed
  flake), and the six ART-105…110 findings from G11's review.
- **The open-issues paragraph** claimed "twenty entries remain open" and
  "ART-099 and ART-104 closed on 2026-08-15" — the second half was wrong
  even at the time this section was presumably written: ART-104 is still
  open in `ISSUES.md` (only ART-099 closed). Replaced with a pointer to
  `ISSUES.md` for the current list rather than a count that would go
  stale the next time either file is touched, plus the one fact worth
  surfacing without opening it (ART-104 still open, and why).
- **`docs/FEATURES.md`, i18n architecture row** — "1129 keys each",
  left over from Phase 0b (when the true count was 814); the real
  current count is 1457. Corrected.
- Added a session-log row (newest, at the top of the table) recording
  this sweep and the CI-green confirmation, in the project's own voice,
  following the precedent of the 2026-08-14 "Docs swept and the branch
  pushed" entry.
- Added one clause to "Where things are, for a fresh session" noting that
  `E:\amiga\ProjeART\dist-3.2` now holds the real tree G5's Task 14 built,
  since the next likely task (re-running the real-media hook through the
  new fallback path) reads from it directly.

## Left alone, and why

- **`docs/FEATURES.md`'s Aminet test-count claims** ("115 tests" in
  STATUS.md's Stage 5 section, "122 tests" in FEATURES.md's spec-addenda
  section) — checked against the code and neither matches what `grep -c
  '#\[test\]'` actually finds in `core/sources/` today (167, once the
  `catalog/` subdirectory is included). This is real staleness, but it
  predates the G5/fix-wave session entirely and is unrelated to what this
  sweep was scoped to verify (the OS-install/preload/layout work and the
  numbers the task asked me to measure). Left uncorrected rather than
  drawn into a session's docs sweep that wasn't about it; flagging it here
  so it isn't lost.
- **`docs/FEATURES.md`'s Filesystems table, PFS3 row** (`Read ⏳ / Write
  ⏳`, "not supported yet") — technically still accurate: it describes
  `core/volume/write`'s general browse/write capability (the `/files`
  file manager still cannot open a PFS3 partition as a pane), which is a
  different code path from `core/preload::NativeFormatter`'s narrow,
  format-and-copy-only PFS3 writer via `libpfs3` that G3/G5 added. The
  distinction is already spelled out in detail in the SD-2 rows further
  down the same file, so I judged a cross-reference unnecessary rather
  than a gap.
- **The Session log's historical entries** — left untouched throughout,
  including ones with numbers that have since been superseded (e.g. the
  2026-08-10 entry's "814 keys" for `tr.json`, correct for that day).
  Per the project's own convention, a session-log row keeps its original
  wording even after later work supersedes it; only statements about
  *now* were rewritten.
- **`docs/sd-appliance-gap-analysis.md`** — updated by other agents
  during this session (G3/G5/G7/G8/G11/G15 no longer read "owed" there).
  Not touched by me, and I checked my own G9/G10/SD-1-flashing framing in
  STATUS.md does not contradict it.

## Commit

`6a9db43` — "Catch STATUS.md up to a session that had already finished,
and CI to green". `docs/STATUS.md` and `docs/FEATURES.md` only.
