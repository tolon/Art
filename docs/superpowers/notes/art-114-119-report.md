# ART-114 / ART-119 — closing report

Commit: `113ccd7` on `main` (working tree was on `main`, not `sd-1`, when this
session started — see "Environment note" below).

## ART-114 — fixed

`scripts/pfs3-oracle-check.py`'s `check_art_writes_hst_reads` now returns
`(checks, skipped)`. A path whose basename (or any path segment) collides
with a Windows/MS-DOS reserved device name (`CON`, `PRN`, `AUX`, `NUL`,
`COM1`-`COM9`, `LPT1`-`LPT9`, matched case-insensitively before the first
`.`) is recorded in `skipped`, printed in its own named section by `main()`
with the reason, and counts toward neither the pass list nor the failure
list — exit status is unaffected. Anything else missing from extraction
still fails exactly as before.

Verified:
- `python scripts/pfs3-oracle-check.py` run for real against
  `E:\amiga\Amigatolon\hstimager\hst.imager.exe` — printed "ART and
  hst-imager agree, both directions" (full output captured in the session).
  The synthetic Rust fixture (`build_pfs3_volume_for_oracle_when_asked`,
  `core/preload/native.rs`) carries no reserved-name entry, so this run did
  not exercise the new skip path end to end.
- The new predicate was checked directly by hand (module loaded and called
  outside `main()`): `AUX`, `AUX.info`, `aux`, `com3.txt`,
  `Storage/DOSDrivers/AUX` → `True`; `COM10`, `AUX2` → `False`.
- I did not add a reserved-name entry to the Rust-side fixture that would
  exercise the skip path inside the real oracle run — that fixture lives in
  `core/preload/native.rs`, which another session was actively editing
  throughout this pass. Said plainly rather than assumed to work.

## ART-119 — partially closed (2 of 5)

- **#3 (the one specifically asked for) — fixed.** `src/lib/osinstall.test.ts`
  now asserts `rc.condition.condition === "rom-older-than"` whenever a
  recipe component carries a `condition`, not just `major`. A future
  condition variant with a `major` field would now fail this test instead of
  passing it silently.
- **#4 — fixed.** `src/components/osbuilder/OsInstall.tsx`: the
  `!def.required && def.available` gate is restored on the single `reason =
  …` computation. No new render test — none exists for this screen
  (ART-118: it crashes headless Chrome past its headings), and the gate is
  unreachable against today's shipped recipe, same as before.
- **#1, #2 — left open, judged not one-line fixes.** #1 (the two-plan
  double-request) and #2 (four independent JSX guards vs. an exhaustive
  `switch`) are both deliberate, working shapes; fixing either changes more
  than one line to reason about safely. Reasoning recorded in ISSUES.md.
- **#5 — left open, blocked by collision.** Lives in
  `src-tauri/src/core/osinstall/plan.rs`, which another session was actively
  working in. Not touched.

`docs/ISSUES.md` updated: ART-114 moved to Fixed; ART-119 kept in Open with
per-item status (closed/open/blocked) recorded inline. ART-119 was **not**
closed wholesale.

## Environment note

The working tree was on `main` (2 commits ahead of `origin/main`), not
`sd-1` as the task described, with pre-existing uncommitted changes in
`src-tauri/src/core/error.rs`, `core/preload/native.rs`, `core/preload/mod.rs`,
`src-tauri/src/tools/hst_imager.rs` and `src/lib/preload.ts` — evidently the
concurrent agent's in-progress work on ART-113 (non-ASCII PFS3 names) in
exactly the files I was told to stay out of. I did not touch, stage, or
commit any of those files. I stayed on `main` rather than switching branches,
since switching with another agent's uncommitted work present risked
disrupting it.

## Verification run

- `pnpm lint` — pass.
- `pnpm test` — 44 files, 518 tests, all pass (includes `osinstall.test.ts`,
  26 tests).
- `cargo check` (src-tauri) — compiles clean; made zero Rust changes myself.
- `cargo fmt --check` — **fails**, but only on the concurrent agent's
  unstaged, unformatted WIP in `error.rs` and `native.rs`. Not run/fixed —
  not my files.
- `cargo clippy --all-targets -- -D warnings` — clean (no warnings) against
  the current tree including the concurrent WIP.
- `cargo test` (src-tauri) — 1390 passed, 1 failed:
  `core::preload::native::tests::the_same_non_ascii_name_copies_in_fine_on_ffs`
  panics with `FFS has no such encoding mismatch to refuse:
  NonAsciiPfs3Names { paths: ["türkçe"], more: 0 }`. This is inside the
  concurrent agent's in-progress `native.rs`/`error.rs` work (ART-113), not
  something I introduced or touched — reported here rather than silently
  ignored.
