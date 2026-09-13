# Task 5 Report: Prepare the upstream patch for the owner

## Upstream head / branch

- Cloned `https://github.com/metaneutrons/pfs3.git` to `~/Belgeler/Projeler/art-experiments/pfs3-upstream`.
- `main`'s head at clone time: `05f50b0` (`05f50b06c3cd` full, matches the brief's expectation exactly).
- Branch created: `fix/format-superindex-reserved-anodes`.
- New commit after the patch: `6eb44df`.

## TDD evidence

### RED

Command: `cd ~/Belgeler/Projeler/art-experiments/pfs3-upstream && cargo test -p libpfs3 --test format`

```
running 9 tests
test format_and_open ... ok
test format_check_passes ... ok
test format_empty_root ... ok
test format_minimum_size ... ok
test format_rootblock_flags ... ok
test format_reserves_anodes_below_rootdir ... FAILED
test format_then_write_on_minimum_disk ... ok
test format_various_sizes ... ok
test format_superindex_mode_writes_the_sb_level_and_accepts_writes ... FAILED

---- format_reserves_anodes_below_rootdir stdout ----
thread 'format_reserves_anodes_below_rootdir' panicked at crates/libpfs3/tests/format.rs:199:13:
assertion `left == right` failed: 8192 blocks, anode 0
  left: [0, 0, 0]
 right: [0, 4294967295, 0]

---- format_superindex_mode_writes_the_sb_level_and_accepts_writes stdout ----
thread 'format_superindex_mode_writes_the_sb_level_and_accepts_writes' panicked at crates/libpfs3/tests/format.rs:220:5:
assertion `left == right` failed
  left: 18754
 right: 21314

test result: FAILED. 7 passed; 2 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.02s
```

Both failures match the brief exactly (18754 = IBLKID, 21314 = SBLKID = 0x5342).

### GREEN

Patch applied via `python3 ~/Belgeler/Projeler/art-experiments/art-310-format-patch.py crates/libpfs3/src/format.rs`,
then the `SBLKID` import-list edit (assert count == 1, satisfied), then `cargo fmt --all` (also reformatted
the newly-appended test code to rustfmt's line-wrapping — no content change).

Command: `cd ~/Belgeler/Projeler/art-experiments/pfs3-upstream && cargo test -p libpfs3`

```
     Running tests/format.rs (target/debug/deps/format-110225199cfb078c)
running 9 tests
test format_and_open ... ok
test format_check_passes ... ok
test format_empty_root ... ok
test format_minimum_size ... ok
test format_rootblock_flags ... ok
test format_then_write_on_minimum_disk ... ok
test format_various_sizes ... ok
test format_reserves_anodes_below_rootdir ... ok
test format_superindex_mode_writes_the_sb_level_and_accepts_writes ... ok

test result: ok. 9 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.04s
```

All other test files in the crate (`corrupt.rs` 15, `deldir.rs` 4, `fault.rs` 16, `fixtures.rs` 50, `rdb.rs` 5,
`read.rs` 32, `stress.rs` 13, `write.rs` 63, doc-tests 2) also passed, unchanged, 0 failed across the whole
`cargo test -p libpfs3` run.

## Upstream's own checks (Step 5)

| Check | Command | Result |
|---|---|---|
| fmt | `cargo fmt --all -- --check` | exit 0, no diff |
| clippy | `cargo clippy -p libpfs3 --all-targets --all-features -- -D warnings` | exit 0, no warnings |
| test | `cargo test -p libpfs3` | exit 0, all green (see above) |
| deny | `cargo deny check` | exit 0, output: `advisories ok, bans ok, licenses ok, sources ok` |

No `--workspace` run was attempted (would need `libfuse3` headers for `pfs3-fuse`, per the brief); the
line for the owner is unchanged from the brief: `! sudo pacman -S fuse3`, then
`cargo clippy --workspace …` / `cargo nextest run --workspace …`.

## Commit identity (no AI trailer)

Command: `git log --format='%an <%ae>%n%B' -1` in the clone:

```
tolon <turkbug@gmail.com>
fix(libpfs3): write the super index level and reserve anodes 0-4 on format

format_with_size pointed rootblock_ext.superindex[0] straight at the anode
index block in SUPERINDEX mode. Every reader walks superindex -> SB -> IB -> AB
(pfs3aio's GetSuperBlock rejects a block that is not SBLKID; this crate's own
AnodeReader and writer expect the same), so a partition over MAXSMALLDISK
formatted here could not be read back, by this crate ("anode 5 not found") or
by hst-imager.

It also left anodes 0-4 as (0, 0, 0). pfs3aio's format reserves them through
AllocAnode (clustersize 0, blocknr 0xffffffff, next 0); an allocator ported from
pfs3aio treats (0, 0, 0) as free, so hst-imager's first mkdir on such a volume
fails ERROR_DISK_FULL and a later directory is given anode 1.

The super index block is allocated before the anode index block, which is
pfs3aio's order; reserved-area sizing and everything else are unchanged.
```

Author is the clone's configured git identity (`tolon <turkbug@gmail.com>`, the same identity used for ART
commits — no separate identity was configured in the clone, and it was already present globally, so no stop
was needed). No `Co-Authored-By` or other AI trailer present.

Files staged for the upstream commit (verified with `git status --short` before commit): only
`crates/libpfs3/src/format.rs` and `crates/libpfs3/tests/format.rs`.

## PR-BODY.md

Path: `~/Belgeler/Projeler/art-experiments/pfs3-upstream/PR-BODY.md`, written verbatim from the brief.
Confirmed untracked (`git status --short` shows `?? PR-BODY.md`) and not committed.

Nothing was pushed; `git remote -v` in the clone still shows only the default `origin` pointing at
`metaneutrons/pfs3` (fetch/push), unchanged from the clone — no push, fork, or `gh pr create` was run.

## ART commit

SHA: `bfdedd668b2dd553084128f2c0eadde7cea52c6e` (short `bfdedd6`), on branch `art-310-libpfs3-format`.

New `## Upstream` section body in `src-tauri/vendor/libpfs3/ART-PATCH.md`:

```markdown
Prepared, not offered. Branch `fix/format-superindex-reserved-anodes` (`6eb44df`, on `main` at `05f50b0`) in
a local clone of `metaneutrons/pfs3`: the same change to `crates/libpfs3/src/format.rs`, two tests in
`tests/format.rs` (seen red on `main`, green on the change), upstream's fmt, clippy, test and deny checks
clean, and a Conventional Commit with no AI attribution, as upstream's `CONTRIBUTING.md` requires. The
owner opens the pull request after a patched volume has been mounted under real pfs3aio.
```

(`<sha>` = `6eb44df` from `git -C ~/Belgeler/Projeler/art-experiments/pfs3-upstream rev-parse --short HEAD`;
`<base>` = `05f50b0`, Step 1's head.)

The ART commit's trailer names the model actually doing this task, per the task brief's "Decisions already
taken": `Co-Authored-By: Claude Sonnet 5 <noreply@anthropic.com>` (not the brief's literal placeholder
`Claude Opus 5 (1M context)`, since I am Sonnet 5, per the explicit ruling that the trailer names the model
you actually are).

Only `src-tauri/vendor/libpfs3/ART-PATCH.md` was staged and committed in ART (verified via `git status
--short` before `git add`).

## Self-review

- **Completeness:** All 7 steps executed in order: clone+branch, write tests, RED, patch+SBLKID
  import+fmt, upstream checks (fmt/clippy/test/deny), commit+PR-BODY.md, ART record+commit.
- **Discipline:** Upstream commit touches only `crates/libpfs3/src/format.rs` and
  `crates/libpfs3/tests/format.rs` (confirmed via `git status --short` pre-add, and via the diff shown
  above matching ART-PATCH.md's own recorded diff against 0.1.3 byte-for-byte in format.rs). ART commit
  touches only `ART-PATCH.md`. `PR-BODY.md` stays untracked. Neither the upstream nor the ART repo was
  pushed anywhere; no PR was opened; no fork was created.
- **Evidence:** RED shown (both exact failure messages, matching the brief's expected numbers). GREEN
  shown (`test result: ok. 9 passed`, all other test files unaffected). All four upstream checks' exact
  commands and results recorded above.
- **Anchors held:** every constant/type/method the brief's test code references (`SBLKID`, `MODE_SUPERINDEX`,
  `ABLKID`, `IBLKID`, `ANODE_ROOTDIR`, `ANODE_SIZE`, `ANODE_BLOCK_HEADER_SIZE`, `RootblockExt.superindex`,
  `Rootblock.indexblocks`, `Rootblock.options`, `Volume.rootblock_ext`, `Volume.open_rw`,
  `Writer::open`/`create_dir`/`write_file`, `FormatOptions::default()`) was verified present with matching
  signatures before use — no anchor drift from the brief's assumptions, so no NEEDS_CONTEXT was warranted.
- **No issues or concerns found.** `git config --global user.name/user.email` was already `tolon` /
  `turkbug@gmail.com` before starting, so the "stop if no identity configured" branch of the Decisions
  section never triggered. `cargo deny` needed no advisories/bans exceptions — output was fully clean, so
  the "record and continue" branch for unrelated deny failures also never triggered.
