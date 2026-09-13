# ART-310 — where the work stopped, and how to pick it up on Windows (2026-09-13)

Why this exists: the owner is moving from the CachyOS Linux machine this work ran on to the Windows
machine. Three things the next session needs would not travel with the branch:

- the subagent-driven-development workspace (`.superpowers/sdd/…`), which is git-ignored;
- the run log, which lives outside the repository on the Linux machine;
- the prepared upstream branch, in a local clone on the Linux machine.

This directory carries copies of all three. Paths inside the copies refer to the Linux machine
(`~/Belgeler/Projeler/art-experiments/…`).

## Where it stands

- **Branch:** `art-310-libpfs3-format`, pushed to `origin` on the owner's word. **Not merged.**
- **Documents:**
  - research `docs/superpowers/notes/2026-09-11-libpfs3-format-fix-research.md`
  - design `docs/superpowers/specs/2026-09-13-art-310-libpfs3-format-fix-design.md` (approved by the owner)
  - plan `docs/superpowers/plans/2026-09-13-art-310-libpfs3-format-fix.md`
- **Plan tasks 1, 2, 3 and 5 are done**, each with an implementer and a task review:

| Plan task | Commit(s) | What |
|---|---|---|
| 1 | `7ce9136` | `libpfs3` 0.1.3 vendored byte-for-byte in `src-tauri/vendor/libpfs3` as `0.1.3+art.1`; `[patch.crates-io]`; `probe()` and the pin test name it |
| 2 | `f52bf5e` | `format.rs` writes the super index (SB) level in SUPERINDEX mode and reserves anodes 0–4; three tests read the image's own blocks, seen red on 0.1.3; mutations M1–M4 all killed |
| 3 | `dd10bf8`, `ad52b2b` | `scripts/pfs3-oracle-check.py`: a direction past MAXSMALLDISK and an anode check at both sizes; a fix round made a failing anode hook print its output |
| 5 | `bfdedd6` | `ART-PATCH.md` records the upstream branch prepared for the owner |
| **4** | — | **not started:** closing ART-310, filing ART-311, the licence and status documents |

**Evidence so far:**

- **Linux module runs**, through the gate script (`art-linux-run.sh`, which holds back ART's two Windows-only
  tests for the run):
  - `core::preload::native` — 42 passed; 0 failed; 1 ignored
  - `core::card::sizing` — 24 passed; 0 failed
- **Red lines, mutations and both oracle runs:** [`plan-run.md`](plan-run.md). The oracle ran with hst-imager
  1.6.616 linux-x64 and pfs3aio 3.1.
  - **On the fix:** every check ok. hst-imager's new directory got anode 15 at both sizes. The large image
    is 5 452 554 240 bytes.
  - **With `[patch]` removed:** the small direction failed `ERROR_DISK_FULL`; the large direction failed in
    ART's own write hook with `anode 5 not found`.
- **CI run 34756185385 on `bfdedd6`** (Windows x64, push): success, every step. `cargo test`:
  `test result: ok. 3257 passed; 0 failed; 58 ignored; 0 measured; 0 filtered out; finished in 301.03s`.
  That is 3252 + 5, the count the plan predicted.
- **Upstream (`metaneutrons/pfs3`):** local branch `fix/format-superindex-reserved-anodes`, commit `6eb44df`
  on `main` `05f50b0`, **not pushed**. The portable copy is [`upstream/`](upstream/): the `git format-patch`
  file and `PR-BODY.md`. Its two tests were seen red on upstream `main`; `cargo test -p libpfs3`, fmt,
  clippy and deny were clean. The commit carries no AI attribution trailer, per upstream's
  `CONTRIBUTING.md`.

## What is left

1. **Plan task 4, step 1 — the owner's Windows runs** (PowerShell, from the repository):

   ```powershell
   git fetch origin
   git switch art-310-libpfs3-format
   cd src-tauri
   $env:TMP='E:\amiga\ProjeART\build\tmp'; $env:TEMP=$env:TMP
   cargo fmt --check
   cargo clippy --all-targets -- -D warnings
   cargo test --lib
   cargo test --lib
   cargo deny check
   ```

   Expected: fmt and clippy clean, both runs `3257 passed; 0 failed; 58 ignored`, deny ok. On Windows, run
   `cargo test --lib` directly; the Linux gate script is not needed there.
2. **Plan task 4, steps 2–7** as written, taking the values from `plan-run.md`, the CI run and step 1. Two
   rulings change what the ART-310 entry must say:
   - **Ruling 7:** M2/M3 needed `sb_blk` bound as `_sb_blk` for those runs.
   - **Ruling 9:** the large direction on 0.1.3 failed at ART's own write, not at hst-imager's listing.
3. **Resume subagent-driven development.**
   - Copy [`sdd-ledger.md`](sdd-ledger.md) to
     `.superpowers/sdd/2026-09-13-art-310-libpfs3-format-fix/progress.md`. The skill skips tasks with a
     `Task N: complete` line and resumes at task 4.
   - Cut task 4's brief from the plan by line range, **1073–1328** (Ruling 3; `scripts/task-brief` misreads
     this plan's nested fences).
   - After task 4: the final whole-branch review, then finishing the branch. **Merging is the owner's word.**
4. **Owed by a person, not blocking task 4:**
   - mount a patched volume past MAXSMALLDISK under real pfs3aio in WinUAE, `dir` it and make a drawer;
   - then open the upstream pull request from the owner's own account. Use `git am` on a fresh clone of
     `metaneutrons/pfs3` `main` with the patch in `upstream/`, and `PR-BODY.md` as its text, without AI
     trailers.

## Rulings made on the owner's behalf during execution

Each was recorded with what it costs if wrong; the owner reads these and reworks any that are wrong.

1. **Order T1, T2, T3, T5, then T4.** T4's first step needs a push and the owner's Windows machine.
   *Cost:* none in code.
2. **Models.** Implementers and reviewers on Sonnet; the final whole-branch review on the most capable
   model. *Cost:* spend only.
3. **Briefs 2–5 cut from the plan by line range.** `scripts/task-brief` ran task 2 into tasks 3–5 because of
   a nested fence. *Cost:* a boundary line; each brief was checked to hold exactly one task heading.
4. **`cargo update -p libpfs3 --precise 0.1.3+art.1` accepted in task 1.** Cargo did not refresh the lock on
   its own. *Cost:* none; the lock entry is what the plan expected.
5. **Linux-only clippy errors not fixed.** They are at `commands/panel.rs:200` and
   `core/osinstall/mediahash.rs:1966`, both older than this branch. *Cost:* a Windows clippy failure found
   late; CI's clippy passed on `bfdedd6`.
6. **Task 1's commit trailer is `Claude Sonnet 5`, not the plan's Opus line.** The Sonnet subagent wrote
   it. *Cost:* cosmetic.
7. **M2/M3 were run with `_sb_blk`.** As written they only failed to compile under 0.1.3's
   `#![deny(warnings)]`. *Cost:* the evidence is one rename away from the plan's literal text.
8. **Sonnet trailers kept on subagent commits (`f52bf5e`).** No amend. *Cost:* cosmetic; the owner may
   rebase the trailers before merge.
9. **Task 3's 0.1.3 large-direction failure recorded as measured** (ART's write hook), not as predicted
   (hst's listing). *Cost:* the hst-side crash on a 0.1.3 large volume is shown by the research note, not
   by the oracle.
10. **The oracle's anode check prints the hook's output when it fails.** A plan-mandated diagnostics gap,
    fixed in one round. *Cost:* a few lines beyond the plan's text.
11. **Ruling 8 extended to `bfdedd6`.** *Cost:* cosmetic.

## Deferred minors, for the final review to triage

- The lock-refresh step (`cargo update -p libpfs3 --precise 0.1.3+art.1`) is recorded only in task 1's
  report, not in `ART-PATCH.md`.
- The subagent commits' trailers read Sonnet (Rulings 6, 8, 11).
- In `scripts/pfs3-oracle-check.py`, the name `made` holds the write-hook result and later the hst `mkdir`
  result.

## Files here

| File | What |
|---|---|
| `sdd-ledger.md` | the SDD ledger as it stood at this handoff: pre-flight scan, rulings, per-task progress |
| `plan-run.md` | the plan's run log: task 2's red lines, green times and mutation table; task 3's oracle output on the fix and on 0.1.3 |
| `task-1-report.md`, `task-2-report.md`, `task-3-report.md`, `task-5-report.md` | each implementer's full report, with TDD evidence and fix rounds |
| `upstream/0001-fix-libpfs3-write-the-super-index-level-and-reserve-.patch` | the upstream commit `6eb44df`, `git format-patch` |
| `upstream/PR-BODY.md` | the prepared pull-request text |
