# Task 3 report: The oracle past MAXSMALLDISK, and the anode question

## What I implemented

### Step 1: Split the write hook (`src-tauri/src/core/preload/native.rs`)

Located the moved range in commit `4b0950b` first: `git show 4b0950b:src-tauri/src/core/preload/native.rs`
lines 2092-2159. Confirmed line 2092 is exactly `// The literal bytes are named once and reused for both
the write and` and line 2159 is exactly `println!("json={entries}");` — the anchors and the line numbers
both matched, so the range needed no adjustment.

Replaced the whole `build_pfs3_volume_for_oracle_when_asked` function (from `#[test]` through its closing
brace, at what were then lines 2251-2349 in the current file — later than 2063-2160 because Task 2 added
two tests and a doc-comment block above this one) with the brief's template verbatim: the original hook
now just calls `write_pfs3_volume_for_oracle(&PathBuf::from(&target), 220 * 1024 * 1024, 200)`; a new
`build_large_pfs3_volume_for_oracle_when_asked` calls it with `5_200 * 1024 * 1024, 5_100`; a new
`pfs3_anode_for_oracle_when_asked` opens the volume with `libpfs3::volume::Volume::open` +
`partition_offset` and prints `anode=<n>` from `vol.lookup(&path)`; and `write_pfs3_volume_for_oracle`
carries the shared body (create_hdf, format_partition, then the moved verbatim block). Inside the moved
block, the one call that took `&image` — `.copy_in(&image, …)` — became `.copy_in(image, …)`, matching the
new `image: &Path` parameter (the other call the note mentions, `format_partition`, already used `image`
in the brief's own template). The doc comment above the old function (Task 11's "ART writes, `hst-imager`
reads" comment, unchanged since it still describes the small volume) was kept in place.

Verified against the vendored crate that `Volume::lookup(&mut self, path: &str) -> Result<Option<DirEntry>>`
exists (`src-tauri/vendor/libpfs3/src/volume.rs:243`) and that `entry.anode` is a real field (used the same
way elsewhere in this file, e.g. line 916).

### Step 3: `scripts/pfs3-oracle-check.py`

- Added `SEP` and `dh0(image)` right after `run_hst`.
- Replaced all three `f"{image.name}\\rdb\\dh0"` occurrences (the `fs dir` listing, `fs copy` extraction in
  `check_art_writes_hst_reads`, and `fs copy` in `check_hst_writes_art_reads`) with `dh0(image)`. Left the
  one occurrence in a doc comment (line ~57) untouched — it is prose, not code.
- `check_art_writes_hst_reads` now takes `hook`, `env_var`, `image_name`; builds `image = work / image_name`
  and calls `run_cargo_test(hook, {env_var: str(image)})`; `extract_dir` is now
  `work / f"{image.stem}-extract"`.
- Added the size-on-disk print line right after `checks.append((True, "ART wrote a PFS3 volume"))`.
- Replaced the function's final `return checks, skipped` with the `hst-imager fs mkdir … HstMade` +
  `pfs3_anode_for_oracle_when_asked` anode check block, exactly as specified.
- `main()` now runs `check_art_writes_hst_reads` twice — once for the small volume (unchanged env/hook
  names, `"art-write.hdf"`), once for the large one (`build_large_pfs3_volume_for_oracle_when_asked`,
  `ART_PFS3_WRITE_OUT_LARGE`, `"art-write-large.hdf"`) — and folds the large section's checks/skips into
  `checks_a`/`skipped_a` before the summary.

`python3 -m py_compile scripts/pfs3-oracle-check.py` succeeded.

## Step 2: hooks build and stay silent without their env

```
bash ~/Belgeler/Projeler/art-experiments/art-linux-run.sh bash -c 'cd src-tauri && cargo test --lib core::preload::native'
```

`test result: ok. 42 passed; 0 failed; 1 ignored; 0 measured; 3269 filtered out; finished in 0.69s`

Matches the brief's expectation exactly (40 → 42 with the two new hooks added, both returning early without
printing anything).

## Step 4: oracle on the fix

Ran the exact command from the brief (scratch dir created first). All three sections passed, ending:

```
ART and hst-imager agree, both directions — names, sizes, bytes, and protection bits.
(2 file(s) with Windows-reserved names skipped for extraction, see above — not a failure.)
```

Key lines:
- `art-write.hdf: 230178816 bytes long, 73728 bytes on disk` — small volume.
- `hst-imager's new directory has an anode pfs3aio does not reserve (anode 15; 0-4 are reserved, ART-310)`
  — small volume, anode 15 (above 4).
- `art-write-large.hdf: 5452554240 bytes long, 1335296 bytes on disk` — matches the brief's expected
  5 452 554 240 bytes for 10 565 whole cylinders of the 5 200 MiB asked.
- `hst-imager's new directory has an anode pfs3aio does not reserve (anode 15; 0-4 are reserved, ART-310)`
  — large volume, anode 15 (above 4).

Full output copied under `## Task 3 oracle, fix` in
`~/Belgeler/Projeler/art-experiments/2026-09-13-art-310/plan-run.md`. Exit code 0.

## Step 5: defect put back

Deleted `[patch.crates-io]` / `libpfs3 = { path = "vendor/libpfs3" }` from `src-tauri/Cargo.toml`. A
`cargo check --lib` through the gate script picked up crates.io `libpfs3 v0.1.3` on its own (`Adding
libpfs3 v0.1.3`) — no manual `cargo update -p libpfs3 --precise 0.1.3` was needed.

Ran Step 4's command again. Exit code 1. Summary:

- **Small section**: `hst-imager fs mkdir` itself failed with `System.IO.IOException: ERROR_DISK_FULL`
  (its allocator, a port of pfs3aio's, found no free anode on a volume where 0-4 are not reserved). The
  check that shows FAIL is "hst-imager made a directory on the volume ART formatted" — the brief's first
  accepted branch ("hst-imager's `mkdir` either fails, or the directory gets an anode ≤ 4"). The script
  correctly returns before reaching the anode check itself, by design, when `mkdir` fails.
- **Large section did NOT fail the way the brief predicted.** The brief expected it to fail at hst-imager's
  listing with a `NullReferenceException`. Instead it failed one step earlier: ART's own
  `write_pfs3_volume_for_oracle` (running against unpatched 0.1.3) panicked during `copy_in` with
  `Malformed { format: "pfs3", detail: "anode 5 not found" }`, before hst-imager ever touched the volume.
  This is the identical panic message Task 2's mutation table recorded for mutations M2 and M3 on the same
  large-format path (see `plan-run.md`'s "Task 2 mutations" section), so it is a real, reproducible
  consequence of the missing SB level in SUPERINDEX mode — just caught earlier (by ART's writer) than the
  brief anticipated (by hst-imager's reader). Per the decisions already taken, I recorded this exactly and
  did not weaken the script or "fix" anything to force the predicted `NullReferenceException`.

Full output copied under `## Task 3 oracle, 0.1.3` in `plan-run.md`.

Restored with `git checkout -- src-tauri/Cargo.toml src-tauri/Cargo.lock`. `git status --short` afterward
showed only:
```
 M scripts/pfs3-oracle-check.py
 M src-tauri/src/core/preload/native.rs
```
— this task's two files, nothing else. Re-ran `cargo test --lib core::preload::native` through the gate
script after restoring: `test result: ok. 42 passed; 0 failed; 1 ignored` again, confirming the restore was
clean.

## Files changed, commit

- `src-tauri/src/core/preload/native.rs`
- `scripts/pfs3-oracle-check.py`

Commit: `dd10bf8` — "ART-310 task 3: the PFS3 oracle past MAXSMALLDISK, and the anode question", staged by
name (`git add scripts/pfs3-oracle-check.py src-tauri/src/core/preload/native.rs`), message via quoted
heredoc, trailer `Co-Authored-By: Claude Sonnet 5 <noreply@anthropic.com>` per the controller's ruling
(the brief's own heredoc names "Claude Opus 5 (1M context)", which I did not use).

`src-tauri/src/commands/osinstall.rs` and `src-tauri/src/core/appearance/mod.rs` were never staged or
touched (the gate script restores them on its own after each run; confirmed by their absence from every
`git status --short` above).

## Self-review

- **Completeness**: all three steps' deliverables present — `write_pfs3_volume_for_oracle`, the two new
  env-gated hooks, `dh0()`/`SEP`, the size-on-disk print, the anode check block, and `main`'s second
  direction.
- **Discipline**: only `native.rs` and `pfs3-oracle-check.py` are in the commit; `git status --short` is
  clean after the commit; the two Windows-only test files were never staged.
- **Evidence**: both oracle runs' full output are in `plan-run.md` under the two required headings; the
  Step 2 test-result line and Step 4/5 exit codes are quoted above and in the plan-run log.
- **Concern (non-blocking, per instructions)**: Step 5's large-direction failure point differs from the
  brief's prediction (ART's own write panics with "anode 5 not found" rather than hst-imager's listing
  raising a `NullReferenceException`). This is documented in detail above and in `plan-run.md`, is
  corroborated by Task 2's own mutation records (M2/M3 hit the identical panic on the identical code path),
  and was left as observed rather than "fixed" to match the brief, per the decisions already taken.
- No other issues found. `cargo check --lib` and the full `core::preload::native` test module both pass
  cleanly on the restored tree.

## Fix round 1: the anode check's silent failure

**Finding (review):** every other failure branch in `check_art_writes_hst_reads` prints `stdout`/`stderr`
on failure, but the anode-check branch (`asked = run_cargo_test(...)`) never checked `asked.returncode` and
never dumped its output — a compile error or panic in `pfs3_anode_for_oracle_when_asked` (a bad
`ART_PFS3_ANODE_PATH`, a libpfs3 regression) would silently report `anode None` with no diagnostic.

**Change**, in `scripts/pfs3-oracle-check.py`'s `check_art_writes_hst_reads`, right after `anode` is parsed
from `asked.stdout` and before `checks.append(...)`:

```python
    if asked.returncode != 0 or anode is None:
        print(asked.stdout[-3000:])
        print(asked.stderr[-2000:])
```

Same tails the write-hook's own failure branch uses (`asked.stdout[-3000:]`, `asked.stderr[-2000:]`). The
check's pass condition (`anode is not None and anode > 4`) and its message are unchanged. Nothing else in
the file was touched — the deferred minor about reusing the name `made` was explicitly out of scope for
this round.

**Verification**

`python3 -m py_compile scripts/pfs3-oracle-check.py` — compiled with no errors (pyc written to a scratch
`__pycache__`, then deleted so the tree stayed clean; `git status --short` showed only the `.py` file
modified throughout).

Exercised the new branch directly, cheaply, without a full oracle run:

1. Wrote a small PFS3 volume via the existing hook (through the gate script, since `cargo test` needs the
   two Windows-only tests gated to compile on Linux):
   ```
   bash ~/Belgeler/Projeler/art-experiments/art-linux-run.sh bash -c 'cd .../src-tauri && env ART_PFS3_WRITE_OUT=.../fixcheck/art-write.hdf cargo test --lib build_pfs3_volume_for_oracle_when_asked -- --nocapture'
   ```
   → `test result: ok. 1 passed; 0 failed`.

2. Loaded the actual, edited `pfs3-oracle-check.py` as a module (`importlib`) and called its real
   `run_cargo_test` with `pfs3_anode_for_oracle_when_asked` and a deliberately wrong
   `ART_PFS3_ANODE_PATH=NoSuchPath` against that volume, then ran the exact new condition
   (`asked.returncode != 0 or anode is None`) against the result — through the gate script, since this
   still shells out to `cargo test`:
   ```
   bash ~/Belgeler/Projeler/art-experiments/art-linux-run.sh python3 <driver using the module's own run_cargo_test>
   ```
   Output:
   ```
   returncode=101 anode=None
   --- would print stdout tail ---

   running 1 test
   core::preload::native::tests::pfs3_anode_for_oracle_when_asked --- FAILED
   ...
   test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 3311 filtered out; finished in 0.00s

   --- would print stderr tail ---

   thread 'core::preload::native::tests::pfs3_anode_for_oracle_when_asked' (85969) panicked at src/core/preload/native.rs:2296:32:
   NoSuchPath is not on the volume
   note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace
   error: test failed, to rerun pass `--lib`
   ```
   Confirms: `asked.returncode` is `101` (non-zero) and `anode` is `None`, so the new `if` fires and both
   tails print — including the exact panic text (`NoSuchPath is not on the volume`) a bad
   `ART_PFS3_ANODE_PATH` or a real libpfs3 regression would produce. This is the identical condition and
   identical print calls now in the committed file, exercised against the real `run_cargo_test` function
   rather than a re-implementation of it.

Cleaned up afterward: removed the scratch volume (`fixcheck/`) and every `__pycache__` created along the
way; `git status --short` was empty except for the staged/committed file at every check.

**Commit:** `ad52b2b` — "ART-310 task 3 fix: the oracle's anode check prints the hook's output when it
fails", `scripts/pfs3-oracle-check.py` staged by name, message via quoted heredoc, trailer
`Co-Authored-By: Claude Sonnet 5 <noreply@anthropic.com>`.

`git status --short` is empty after the commit.
