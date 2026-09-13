# Task 1 report: vendor `libpfs3` 0.1.3 unchanged as `0.1.3+art.1`

## What was implemented

1. **Linux gate script** — `/home/tolon/Belgeler/Projeler/art-experiments/art-linux-run.sh`, exactly as the
   brief specifies: gates the two Windows-only tests with `#[cfg(windows)]` for the run, restores both
   files afterward, sets `TMP`/`TMPDIR` to a scratch directory outside the repo, refuses to run if either
   gated file already has uncommitted changes.
2. **Vendored `libpfs3` 0.1.3` as `0.1.3+art.1`** at `src-tauri/vendor/libpfs3/`:
   - `src/**` and `README.md` copied verbatim from the verified `.crate` tarball (byte-for-byte identical
     to 0.1.3 — see diff below).
   - `Cargo.toml` built from `Cargo.toml.orig`: version bumped to `0.1.3+art.1`, `[dev-dependencies]`
     (the `sevenz-rust` 0.6 dev-dependency) dropped.
   - `LICENSE` fetched from upstream commit `33e9ff6ba8462cc4e434dfb6e2783d91b7dd5b14` (verified via
     `.cargo_vcs_info.json` inside the crate), `COPYING.LESSER` fetched from gnu.org.
   - `ART-PATCH.md` written with provenance, what was/wasn't carried, and "no changes in this revision."
   - No `tests/` directory carried.
3. **`src-tauri/Cargo.toml`**: replaced the comment above `libpfs3 = "=0.1.3"` to describe the vendored
   copy and the patch; added a `[patch.crates-io]` section (with its own ART-310 comment) immediately
   before `[dev-dependencies]`, pointing `libpfs3` at `vendor/libpfs3`. The `=0.1.3` pin itself is
   unchanged.
4. **`src-tauri/Cargo.lock`**: `libpfs3` entry now `version = "0.1.3+art.1"` with no `source =` / no
   `checksum =` line (path dependency). Getting Cargo to actually apply the patch required an explicit
   `cargo update -p libpfs3 --precise 0.1.3+art.1` after adding `[patch.crates-io]` — see "Self-review"
   below for why, and confirmation this is expected/safe.
5. **`src-tauri/src/core/preload/native.rs`**:
   - `LIBPFS3_VERSION` doc comment and value updated to `"0.1.3+art.1"`, describing the vendored copy.
   - `the_pinned_version_constant_matches_cargo_toml` rewritten to check three things: the `=0.1.3` pin
     still exists, `[patch.crates-io]` + the vendored-path line exist, and the vendored manifest's
     `version =` matches `LIBPFS3_VERSION`.
   - `probe_names_libpfs3` rewritten to assert the exact string
     `"libpfs3 0.1.3+art.1 (native, no external tool)"`.

## TDD evidence

**RED** — command:
```
bash ~/Belgeler/Projeler/art-experiments/art-linux-run.sh bash -c 'cd src-tauri && cargo test --lib core::preload::native'
```
Compile error (after Step 2's test rewrite, before vendoring):
```
error: couldn't read `src/core/preload/../../../vendor/libpfs3/Cargo.toml`: No such file or directory (os error 2)
    --> src/core/preload/native.rs:1765:24
```
Matches the brief's expected error exactly.

**GREEN** — same command, after Steps 4–6 (vendoring + Cargo.toml patch + native.rs constant):
```
test result: ok. 37 passed; 0 failed; 1 ignored; 0 measured; 3269 filtered out; finished in 0.67s
```

Step 1 (before any of the above edits) was also run and confirmed green first, to prove the gate script
itself works on the base commit:
```
test result: ok. 37 passed; 0 failed; 1 ignored; 0 measured; 3269 filtered out; finished in 0.65s
```
`git status --short` after Step 1's run showed nothing (neither gated file touched).

## Step 7 — Cargo.lock

```
$ grep -A2 'name = "libpfs3"' src-tauri/Cargo.lock
name = "libpfs3"
version = "0.1.3+art.1"
dependencies = [
```
No `source =` or `checksum =` line. (See "Self-review" for the `cargo update -p libpfs3 --precise
0.1.3+art.1` step needed to get here.)

## Step 8 outputs

- **Sizing suite** (`cargo test --lib core::card::sizing`, via the gate script):
  `test result: ok. 24 passed; 0 failed; 0 ignored; 0 measured; 3283 filtered out; finished in 10.99s`.
  Verified identical pass/ignore counts on the base commit (`git stash -u`, same command, same
  `24 passed; 0 failed; 0 ignored`, `finished in 11.01s`) before popping the stash back.
- **`cargo fmt --check`** (in `src-tauri/`): exit 0.
- **`git status --short src-tauri/vendor`**: `?? src-tauri/vendor/` only — `cargo fmt` did not rewrite
  the vendored source.
- **`cargo clippy --all-targets -- -D warnings`** (via the gate script): **fails**, with two errors:
  - `src/commands/panel.rs:200` — `clippy::vec_init_then_push`, inside a `#[cfg(not(windows))]` block
    (Linux-only code path; ART builds Windows-only).
  - `src/core/osinstall/mediahash.rs:1966` — `clippy::let_unit_value` (`let _guard = deny_reads(locked);`
    in a test).
  Neither is under `vendor/libpfs3` or in `core/preload/native.rs`. I confirmed both are pre-existing and
  unrelated to this task by stashing all of task 1's changes (`git stash -u`) and re-running the identical
  clippy command on the base commit `ff4c8fe`: the same two errors, same paths, appeared unchanged. Per
  the brief's own rule ("An error elsewhere that is Linux-only... is recorded for the owner's Windows
  clippy run, not fixed here"), these are recorded, not fixed. Note: because these two errors halt
  compilation of the `amiga-retro-toolkit` crate before clippy can lint every module, clippy's own
  cleanliness of `native.rs` beyond what compiled during `cargo test`/`cargo build` is not independently
  confirmed by this run — `vendor/libpfs3` itself did compile and check cleanly as its own crate
  (`Checking libpfs3 v0.1.3+art.1 (.../vendor/libpfs3)` with no errors reported against it).
- **`cargo deny check`** (in `src-tauri/`): `advisories ok, bans ok, licenses ok, sources ok`.

## `diff -r` — vendored `src/` vs. pristine 0.1.3

```
$ diff -r /home/tolon/Belgeler/Projeler/art-experiments/libpfs3-0.1.3-pristine/libpfs3-0.1.3/src \
          /home/tolon/Belgeler/Projeler/Art/src-tauri/vendor/libpfs3/src
(no output — identical)
$ diff .../libpfs3-0.1.3/README.md .../vendor/libpfs3/README.md
(no output — identical)
```

## Files changed / commit

Commit `7ce9136` — "ART-310 task 1: vendor libpfs3 0.1.3 unchanged as 0.1.3+art.1":
- `src-tauri/Cargo.lock` (modified)
- `src-tauri/Cargo.toml` (modified)
- `src-tauri/src/core/preload/native.rs` (modified)
- `src-tauri/vendor/libpfs3/{ART-PATCH.md,COPYING.LESSER,Cargo.toml,LICENSE,README.md,src/**}` (new, 20
  files)

`git status --short` before the commit listed no gated file (`osinstall.rs`, `appearance/mod.rs`); after
the commit, working tree is clean.

Note on attribution: the plan's own Step 9 heredoc text names "Claude Opus 5 (1M context)" as the
co-author, but this session's system reminder explicitly says it replaces any such earlier attribution
guidance and directs `Co-Authored-By: Claude Sonnet 5 <noreply@anthropic.com>` for commits made in this
session. I used the latter, per that reminder's explicit precedence rule. Flagging this in case the
owner wants it otherwise.

## Self-review

**Completeness.** All 9 steps done. Step 7's expected Cargo.lock line was **not** produced automatically
by the mere presence of `[patch.crates-io]` plus the compile/test runs in Steps 3 and 7 as written —
Cargo initially recorded the patch as `[[patch.unused]]` and kept resolving the original crates-io
`libpfs3 0.1.3` (with `source =`/`checksum =`) as the active dependency, because the existing Cargo.lock
already had a resolved entry and cargo does not automatically re-resolve to a newly-added patch on a
plain `cargo test`/`cargo build`. I ran `cargo update -p libpfs3 --precise 0.1.3+art.1` (via the gate
script) to force the swap, after which the lock file matches the brief's expectation exactly and all
tests still pass. This is a deviation from the literal step sequence (the brief's Steps 3/6/7 don't
mention `cargo update`), but it's the standard, minimal way to make Cargo pick up a newly-added
`[patch.crates-io]` entry, it doesn't touch anything outside `Cargo.lock`'s `libpfs3` block, and the
resulting lock diff is exactly the two-line change (version bumped, source/checksum lines dropped) the
brief describes. Flagging it explicitly since it wasn't spelled out in the steps.

**Quality.** Vendored manifest, `ART-PATCH.md`, `Cargo.toml` comment and `native.rs` doc comments/tests
match the brief's exact text. No unrelated formatting changes (`cargo fmt --check` passed, and
`git status --short src-tauri/vendor` shows only the new, untouched directory).

**Discipline.** Only the files the brief names were touched. `src-tauri/vendor/libpfs3/src` and
`README.md` are confirmed byte-for-byte identical to the 0.1.3 release via `diff -r`. No `tests/`
directory vendored. No new top-level dependency added (byteorder/thiserror were already libpfs3's own
deps). Gated files (`osinstall.rs`, `appearance/mod.rs`) were never staged or committed — script restores
them after every run, and `git status --short` was checked clean of them both before Step 1 and before
the final commit.

**Testing.** RED shown (Step 3's compile error, matching the brief's expected message verbatim) before
GREEN (Step 7's `37 passed; 0 failed; 1 ignored`). Sizing suite counts cross-checked against the base
commit via `git stash -u` to rule out a regression from this change.

**Concerns to flag to the owner:**
1. The `cargo update -p libpfs3 --precise 0.1.3+art.1` step needed to actually activate the patch (see
   above) — not mentioned in the brief's steps, but necessary and minimal.
2. Two pre-existing clippy failures unrelated to this task (`commands/panel.rs:200`,
   `core/osinstall/mediahash.rs:1966`), both Linux-only code paths, confirmed present on base commit
   `ff4c8fe` before any of this task's changes. Recorded per the brief's own instruction, not fixed.
3. Used `Co-Authored-By: Claude Sonnet 5 <noreply@anthropic.com>` instead of the plan text's literal
   "Claude Opus 5 (1M context)", per this session's attribution system reminder.
