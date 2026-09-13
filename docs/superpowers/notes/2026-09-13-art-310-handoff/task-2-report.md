# Task 2 report — the format patch, proved by ART's own tests

## What was implemented

- `src-tauri/src/core/preload/native.rs`, tests module:
  - `formatted_large_pds3_image()` — a 5 100 MiB PFS3 partition on a 5 200 MiB `.hdf`, past
    MAXSMALLDISK, so `format_with_size` selects SUPERINDEX mode.
  - `AnodeChain` and `anode_chain(image)` — reads the rootblock, options, and the index chain
    (`SB`/`IB`/`AB`) straight off the image bytes (not through `libpfs3`'s own reader), plus
    anodes 0–5 of the first anode block.
  - Three tests: `a_small_pfs3_format_reserves_anodes_zero_to_four`,
    `a_large_pfs3_format_writes_the_superblock_level`, `a_large_pfs3_volume_takes_its_own_writes`.
  - All inserted verbatim from the brief at the anchors named (`fn pfs3_free_bytes`,
    `fn copy_in_reports_what_it_moved`), which matched exactly — no NEEDS_CONTEXT needed.
- `src-tauri/vendor/libpfs3/src/format.rs`: patched with `art-310-format-patch.py` (saved at
  `~/Belgeler/Projeler/art-experiments/art-310-format-patch.py`, unmodified from the brief). In
  SUPERINDEX mode a super index block (`SBLKID`, datestamp 1, seqnr 0, `index[0]` = anode index
  block) is now allocated before the anode index block and named by `rext.superindex[0]`; anodes
  0–4 of the anode block are written `(clustersize 0, blocknr 0xFFFFFFFF, next 0)` at every size.
- `src-tauri/vendor/libpfs3/ART-PATCH.md`: `## Changes against 0.1.3` replaced with what changed
  and why, an `## Upstream` note, and `## Diff against 0.1.3` with the generated unified diff.

## TDD evidence

### RED

Command:
```
bash ~/Belgeler/Projeler/art-experiments/art-linux-run.sh bash -c 'cd src-tauri && cargo test --lib -- a_small_pfs3_format_reserves_anodes_zero_to_four a_large_pfs3_format_writes_the_superblock_level a_large_pfs3_volume_takes_its_own_writes'
```
Result: `test result: FAILED. 0 passed; 3 failed; 0 measured; 3307 filtered out; finished in 0.04s`

All three panic messages, verbatim:
```
---- core::preload::native::tests::a_small_pfs3_format_reserves_anodes_zero_to_four stdout ----
thread '...' panicked at src/core/preload/native.rs:1523:13:
assertion `left == right` failed: anode 0 must be reserved the way pfs3aio's AllocAnode leaves it
  left: (0, 0, 0)
 right: (0, 4294967295, 0)

---- core::preload::native::tests::a_large_pfs3_format_writes_the_superblock_level stdout ----
thread '...' panicked at src/core/preload/native.rs:1542:9:
assertion `left == right` failed: superindex[0] must name a super index block, not the anode index block
  left: [[73, 66], [65, 66]]
 right: [[83, 66], [73, 66], [65, 66]]

---- core::preload::native::tests::a_large_pfs3_volume_takes_its_own_writes stdout ----
thread '...' panicked at src/core/preload/native.rs:1569:14:
called `Result::unwrap()` on an `Err` value: Malformed { format: "pfs3", detail: "anode 5 not found" }
```
Each failed for exactly the reason the brief predicted (byte pairs `73,66`/`65,66` = `IB`/`AB`).
No NEEDS_CONTEXT was triggered.

### GREEN

After `art-310-format-patch.py` and `cargo fmt` (see Step 7 below):

Command: `bash ~/Belgeler/Projeler/art-experiments/art-linux-run.sh bash -c 'cd src-tauri && cargo test --lib core::preload::native'`
Result: `test result: ok. 40 passed; 0 failed; 1 ignored; 0 measured; 3269 filtered out; finished in 0.70s`
(Both large-mode tests are inside this 0.70 s total — nowhere near the 60 s stop threshold.)

Command: `bash ~/Belgeler/Projeler/art-experiments/art-linux-run.sh bash -c 'cd src-tauri && cargo test --lib core::card::sizing'`
Result: `test result: ok. 24 passed; 0 failed; 0 measured; 3286 filtered out; finished in 11.17s`
(same counts as Task 1, Step 8.)

## Step 6 — diff -rq

`diff -u` between the pristine 0.1.3 `format.rs` and the vendored copy exited 1 (differences
found, as expected). `diff -rq` between the pristine `src/` and `src-tauri/vendor/libpfs3/src/`
named only `format.rs` as differing (Turkish-locale message: "...format.rs dosyaları birbirinden
farklı", i.e. "the files differ" — that one path only). `ART-PATCH.md` now carries the diff.

## Step 7 — fmt, clippy, sweep

- `cargo fmt --check` initially reported 5 reflow diffs inside the newly pasted test code in
  `native.rs` (long single-line closures/asserts that this rustfmt 1.9.0 wraps under its default
  100-column width). Ran `cargo fmt` (no `rustfmt.toml` in the repo, so default settings) to fix
  them — this only reformatted the added lines (`git diff --stat` showed pure insertions, no
  pre-existing lines touched) — then `cargo fmt --check` exited 0. Re-ran the native test suite
  afterward to confirm the reflow changed nothing behaviorally (still 40 passed/1 ignored).
- `cargo clippy --all-targets -- -D warnings` (through the gate script): failed only at the two
  pre-existing, Linux-only, already-recorded spots from Task 1 (`commands/panel.rs:200`,
  `core/osinstall/mediahash.rs:1966`). Nothing in `vendor/libpfs3` or `native.rs`.
- `python3 scripts/control-byte-sweep.py`: `control-byte sweep: clean - 7 file(s) allow-listed for
  AmigaDOS DosType data and 7 for deliberate alignment; no stray control bytes and no lost line
  continuations anywhere else`.

## Step 9 — mutations

Committed first (`f52bf5e`), then each mutation was applied to `format.rs` (M4 to `Cargo.toml`),
tested with the Step 3 command, and reverted with `git checkout --`.

Two of the four mutations (M2, M3) as literally specified leave the pattern binding `sb_blk` in
`if let Some(sb_blk) = sb_blk { ... }` unused once the one named line changes, and
`libpfs3/src/lib.rs` has `#![deny(warnings)]` — so the crate failed to *compile* rather than
producing the predicted test failure. This is an artifact of Rust's shadowing/unused-variable
lint, incidental to the semantic property each mutation is meant to falsify (M2: the pointer
target; M3: whether the block is written at all) — not a sign the guard or the mutation is wrong.
For those two only, I additionally renamed the pattern to `_sb_blk` on that same line for the
duration of the mutation (reverted along with everything else by the same `git checkout`). M1 and
M4 needed no such adjustment. This deviation is recorded in `plan-run.md` under `## Task 2
mutations` along with the full result table.

| # | Mutation | Failed | Restored |
|---|---|---|---|
| M1 | anode reservation loop starts at 1, not 0 | `a_small_pfs3_format_reserves_anodes_zero_to_four`, `a_large_pfs3_format_writes_the_superblock_level` (predicted); `a_large_pfs3_volume_takes_its_own_writes` passed | yes |
| M2 | `superindex[0]` points at `anidx_blk` again (+ incidental `_sb_blk` rename) | `a_large_pfs3_format_writes_the_superblock_level`, `a_large_pfs3_volume_takes_its_own_writes` (predicted); small test passed | yes |
| M3 | the SB block's `write_reserved_blocks` call deleted (+ incidental `_sb_blk` rename) | `a_large_pfs3_format_writes_the_superblock_level`, `a_large_pfs3_volume_takes_its_own_writes` (predicted); small test passed | yes |
| M4 | `[patch.crates-io]` removed from `Cargo.toml` | all three ART-310 tests + `the_pinned_version_constant_matches_cargo_toml` (predicted) | yes, `git checkout -- src-tauri/Cargo.toml src-tauri/Cargo.lock` |

Every "Must fail" test failed for every mutation — no survivors, no guard needed strengthening.
`git status --short` was empty after the last restore, confirmed again after the final
verification test run (native: 40 passed, 0 failed, 1 ignored).

Full messages, and the mutation-vs-compile-error note, are in
`~/Belgeler/Projeler/art-experiments/2026-09-13-art-310/plan-run.md` under `## Task 2 mutations`.

## Files changed, commit

- `src-tauri/src/core/preload/native.rs`
- `src-tauri/vendor/libpfs3/src/format.rs`
- `src-tauri/vendor/libpfs3/ART-PATCH.md`

Commit: `f52bf5e` — "ART-310 task 2: libpfs3's format writes the SB level and reserves anodes 0-4"
(parent `7ce9136`, branch `art-310-libpfs3-format`, not pushed).

## Self-review

- Completeness: all 9 steps done in order; red before patch, green after, mutations after commit,
  each with the exact expected `test result:` shapes and messages.
- Discipline: `git show --stat f52bf5e` touches exactly the three named files; `git status --short`
  is empty at every checkpoint the brief calls for; `src-tauri/src/commands/osinstall.rs` and
  `src-tauri/src/core/appearance/mod.rs` were never staged or diffed (confirmed with
  `git diff HEAD~1 --` on both, empty).
- Quality: the patch is exactly `art-310-format-patch.py`'s output, unedited by hand; `native.rs`'s
  new code is the brief's code verbatim, reformatted only by `cargo fmt` (pure reflow, confirmed by
  `git diff --stat` showing only insertions).
- Testing: RED reasons matched the brief's predictions exactly; GREEN counts matched
  (`40 passed; 0 failed; 1 ignored` for native, `24 passed` for sizing, both times well under the
  60 s stop threshold); all four "Must fail" mutation rows failed as required.
- Concern for the owner: M2 and M3, applied byte-for-byte as the brief's table states them, do not
  compile under this crate's `#![deny(warnings)]` (unused `sb_blk` binding) — I judged this
  incidental to what each mutation is testing and adjusted only the shadowing name to get a
  meaningful test result, documenting the deviation rather than silently absorbing it. Worth a
  second look if the mutation table is reused verbatim elsewhere (Task 5 uses the same patch
  script, not this mutation table).
