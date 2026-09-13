# SDD ledger — plan: docs/superpowers/plans/2026-09-13-art-310-libpfs3-format-fix.md

Spec: docs/superpowers/specs/2026-09-13-art-310-libpfs3-format-fix-design.md (read; binding).
Branch: art-310-libpfs3-format, plan commit ff4c8fe (BASE for Task 1).
Workspace verified: feature branch in the main checkout (not main/master); the plan's gate script
hard-codes this checkout's path, so no separate worktree.

## Pre-flight scan

| Rows | Produces / consumes | Found |
|---|---|---|
| T1 ↔ T2 | T1 produces `LIBPFS3_VERSION`, the vendored crate, `art-linux-run.sh`, the pristine 0.1.3 copy; T2 patches `vendor/libpfs3/src/format.rs`, rewrites `ART-PATCH.md`'s "Changes" section, diffs against the pristine copy | consistent: T1's `ART-PATCH.md` ends with `## Changes against 0.1.3`, which T2 Step 6 replaces to end of file |
| T1 ↔ T3 | both edit `native.rs` tests module; T3 moves the oracle hook body from `4b0950b` by `git show`, so T1/T2's line shifts do not matter | consistent |
| T2 ↔ T3 | T3 consumes `partition_offset` (pre-existing), not T2's helpers; test counts 37 → 40 (T2) → 42 (T3) | consistent |
| T2 ↔ T5 | T5 consumes `art-310-format-patch.py` (T2 Step 4) and rewrites `ART-PATCH.md`'s `## Upstream` body (written by T2 Step 6) | consistent |
| T2/T3 ↔ T4 | T4 consumes `plan-run.md` (red messages, mutations, oracle output, times) | consistent |
| T1/T2 ↔ T4 | T4 edits `native.rs` module doc (T1/T2 touched tests and const only) and licence docs | consistent |
| T4 ↔ T5 | no shared file; T4 Step 5's ART-310 text names upstream as "prepared", which T5 does | order-sensitive only (see Ruling 1) |
| T1 self | red = compile error (vendored manifest missing) then green 37; Step 8 clippy may fail Linux-only outside touched files — the plan says record, not fix | consistent |
| T2 self | test code uses `.iter().take(5).enumerate()` (clippy-safe); mutations after the Step 8 commit so `git checkout --` restores the patch | consistent |
| T3 self | hook split keeps the small hook's env/args; expected large image length 5 452 554 240 | consistent |
| T4 self | Step 1 needs the branch on the owner's Windows machine — a push | a side effect outside this worktree (see Ruling 1) |
| T5 self | clone outside the repo; commit without AI trailer; not pushed; `cargo deny check` upstream may report advisories unrelated to the change | noted; recorded as found if it happens |

## Rulings

- Ruling 1: execute in the order T1, T2, T3, T5, then T4 — T4 Step 1 (the owner's Windows suite runs) needs a push, which is the owner's word; T5 does not depend on T4 and T4's text describes T5's result — costs if wrong: T4's docs are written after T5 instead of before, no code difference; the stop for the push stays.
- Ruling 3: task briefs 2-5 are cut from the plan by line range (Task 2 388-786, Task 3 789-1071, Task 4 1073-1328, Task 5 1330 to before "## Self-review") instead of `scripts/task-brief` — the script's extraction of Task 2 ran into Tasks 3-5 (1 199 lines) because Task 2 nests a ```diff fence inside a ````markdown block; Task 1's script brief (284 lines = plan 104-387) was right — costs if wrong: a brief missing a line at a boundary; each was checked to hold exactly one "### Task" heading.
- Ruling 2: models — implementers on `sonnet` (every task mixes transcription with shell steps, a vendoring checksum, TDD red/green, mutations or an external clone, so turn count matters more than token price); task reviewers `sonnet`; scoped re-reviews `sonnet`; final whole-branch review `fable` — costs if wrong: more spend than a cheaper tier, no correctness cost.

## Progress

Task 1: dispatched (BASE ff4c8fe, implementer sonnet, agent a6bc3c10b61436f19, brief task-1-brief.md, report task-1-report.md)
Task 1: implementer DONE_WITH_CONCERNS, commit 7ce9136
- Ruling 4: `cargo update -p libpfs3 --precise 0.1.3+art.1` accepted as part of Task 1 Step 7 — the plan assumed a build would rewrite Cargo.lock; it did not — costs if wrong: none beyond a lockfile step the plan did not name; the lock entry is what the plan expected.
- Ruling 5: clippy on Linux fails at commands/panel.rs:200 and core/osinstall/mediahash.rs:1966, both present on base ff4c8fe (implementer checked) and outside the task's files — recorded for the owner's Windows clippy run (Task 4 Step 1), not fixed — costs if wrong: a Windows clippy failure found late.
Task 1: reviewer dispatched (sonnet, agent a27e010d3488acb67, package review-ff4c8fe..7ce9136.diff)
Task 1: review — Spec ✅, quality Approved, 0 Critical/Important; ⚠️ clippy-on-Linux claim resolved by controller: tree clean, no stash left; panel.rs:199 is a `#[cfg(not(windows))]` path and mediahash.rs:1966 a test's read-deny fixture, both last changed in 9ac3f00, before this branch — pre-existing, Linux-only (Ruling 5 stands)
Task 1: minor (deferred): the `[patch]` lock-refresh step (`cargo update -p libpfs3 --precise 0.1.3+art.1`) is recorded only in task-1-report.md, not in ART-PATCH.md or a comment
Task 1: minor (deferred): Task 1's commit trailer says Claude Sonnet 5, not the plan's Opus line (Ruling 6)
Task 1: complete (commits ff4c8fe..7ce9136, review clean)
Task 2: dispatched (BASE 7ce9136, implementer sonnet, agent ac1c2fa6927cdee00, brief task-2-brief.md, report task-2-report.md)
Task 2: implementer DONE_WITH_CONCERNS, commit f52bf5e — native 40/0/1, sizing 24/0; 3 red reasons as the brief listed; M1-M4 all killed
Task 2: reviewer dispatched (sonnet, package review-7ce9136..f52bf5e.diff); tree clean, no stash
Task 2: review — Spec ❌ only for the commit trailer (Important, plan-mandated); quality Approved; ⚠️ pass counts not re-run — resolved: the implementer's report and plan-run.md are the test evidence by process
- Ruling 8: the plan-mandated Opus trailer vs f52bf5e's Sonnet trailer — keep Sonnet on commits a subagent writes (the controller told it to name the model it is, Ruling 6); the plan text assumed the controller would commit; no amend, no fix round — costs if wrong: trailers on 7ce9136 and f52bf5e (and later subagent commits) read Sonnet; cosmetic, fixable by the owner with a rebase before merge.
Task 2: complete (commits 7ce9136..f52bf5e, review clean after Ruling 8)
Task 3: dispatched (BASE f52bf5e, implementer sonnet, agent a213ecf2b81f1ef0e, brief task-3-brief.md, report task-3-report.md)
Task 3: implementer DONE_WITH_CONCERNS, commit dd10bf8 — native 42/0/1; oracle on the fix all ok, hst's new directory anode 15 at both sizes, large image 5 452 554 240 bytes
Task 3: reviewer dispatched (sonnet, package review-f52bf5e..dd10bf8.diff); tree clean, no stash
Task 3: review — Spec ✅ (verbatim move checked against 4b0950b); quality Approved; 1 Important (plan-mandated): the anode check never looks at the hook's returncode or prints its output, so a failing hook reads as "anode None" with no diagnostic (scripts/pfs3-oracle-check.py:429-448)
Task 3: minor (deferred): `made` reused for the write-hook result and the hst mkdir result (scripts/pfs3-oracle-check.py:319,429), brief-verbatim
Task 3: fix round 1/5 dispatched (FIX_BASE dd10bf8, resumed implementer a213ecf2b81f1ef0e, finding: anode-hook failure has no diagnostic)
Task 3: fix round 1 implementer DONE, commit ad52b2b (py_compile clean; bad ART_PFS3_ANODE_PATH → returncode 101, both tails printed with the panic text); scoped re-review dispatched (sonnet, package review-dd10bf8..ad52b2b.diff); tree clean
Task 3: fix round 1/5 (1 addressed, 0 open — anode-hook diagnostics; commits dd10bf8..ad52b2b)
Task 3: complete (commits f52bf5e..ad52b2b, review clean)
Task 5: dispatched (BASE ad52b2b, implementer sonnet, agent ab19d52ee68162ece, brief task-5-brief.md, report task-5-report.md)
Task 5: implementer DONE — upstream clone commit 6eb44df (no AI trailer, author tolon), ART commit bfdedd6; upstream red as briefed, green 9/9 tests/format.rs, cargo test -p libpfs3 green, fmt/clippy/deny clean; nothing pushed
Owner's decision (2026-09-13, asked during Task 5's review): push `art-310-libpfs3-format` to origin (no merge) after Task 5's review; the owner runs Task 4 Step 1 on Windows and pastes the output.
Task 5: review — Spec ✅ (upstream patch byte-identical to ART's in all five regions; no push, no PR; upstream commit has no AI trailer; PR-BODY.md untracked and as briefed); 1 Important (plan-mandated): bfdedd6's trailer is Sonnet, not the plan's Opus line; ⚠️ upstream CONTRIBUTING's rules not re-read by the reviewer — resolved: the controller read metaneutrons/pfs3 CONTRIBUTING.md at 05f50b06c3cd this session (Conventional Commits; "No AI attribution trailers, in commits or in the pull request body")
- Ruling 11: Ruling 8 extended to bfdedd6 — same reason, same cost (cosmetic, owner may rebase the trailers before merge); no fix round.
Pushed (owner's word): origin/art-310-libpfs3-format = bfdedd6, upstream tracking set, no merge. Task 4 waits on the owner's Windows output (Step 1).
CI run 34756185385 on bfdedd6 (Windows x64, push trigger): conclusion success — every step green (fmt, clippy -D warnings, cargo test, amitools oracle, rom table, control-byte/scratch sweeps, contrast, cargo deny, tauri build). Independent Windows evidence; the owner's two local runs (TMP on E:) are still what Task 4 Step 1 records. CI's cargo test: `test result: ok. 3257 passed; 0 failed; 58 ignored; 0 measured; 0 filtered out; finished in 301.03s` — the plan's predicted count exactly (3252 + 5); 12:06:27Z → 12:28:12Z.
Task 5: complete (commits ad52b2b..bfdedd6 in ART; 6eb44df in the local upstream clone, unpushed; review clean after Ruling 11)
Task 5: reviewer dispatched (sonnet; packages review-ad52b2b..bfdedd6.diff + upstream-6eb44df.diff); ART tree clean; clone has only untracked PR-BODY.md, 6eb44df's parent is upstream main 05f50b0 — Ruling 1 order: T5 before T4
- Ruling 10: fix the plan-mandated diagnostics gap — when the anode hook's returncode is non-zero or no `anode=` line is found, print its stdout/stderr tails like every other failure branch in the function; the check's pass condition (`anode is not None and anode > 4`) stays — the spec wants an oracle whose failures say why; the gap never produced a false pass — costs if wrong: a few lines beyond the plan's text.
- Ruling 9: with [patch] removed, the large direction failed inside ART's own write hook ("anode 5 not found") before hst-imager listed anything, not at hst's listing (NullReferenceException) as the plan predicted — accepted as the measured result: it is the research's own finding (0.1.3 cannot write its own large volume) and still a failing oracle; Task 4's ART-310 entry records what happened, not the prediction — costs if wrong: the hst-side crash on a 0.1.3 large volume is not re-shown by the oracle (the research note's experiment still shows it).
- Ruling 7: M2/M3 as written leave `sb_blk` unused, which 0.1.3's `#![deny(warnings)]` turns into a compile error; the implementer bound it as `_sb_blk` for those two mutation runs only, so they fail as tests — accepted: the mutation's meaning (pointer back at the IB / SB never written) is unchanged and a compile error proves nothing about the guard — costs if wrong: M2/M3's evidence is one rename away from the plan's literal text; Task 4's ART-310 entry must say so.
- Ruling 6: Task 1's commit carries `Co-Authored-By: Claude Sonnet 5` instead of the plan's Opus line — the Sonnet subagent wrote it, so the trailer is accurate; not rewritten — costs if wrong: an inconsistent trailer across the branch, cosmetic.
