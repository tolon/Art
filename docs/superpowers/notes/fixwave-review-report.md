# ART-120 fix wave — review, closing report

Commit: see below (staged on `main`). Applies the review findings against
`docs/superpowers/notes/art-120-report.md`'s six commits.

## Findings and what was done

1. **Important — two shipped strings told the user the opposite of what the
   code does.** `preload.scope` and `layout.scope` still claimed ART cannot
   write PFS3 itself; `preload.result.notVerified` implied "not verified"
   only because ART trusts an external tool's word. Read `preload_run`'s own
   job closure to decide which honest framing applies to `notVerified`: it
   runs no readback/verify pass after either writer finishes, so "not
   verified" is true for a native run and a fallback run alike, for the
   *same* reason — not "the tool's word, not ART's". All three rewritten in
   `en.json` and `tr.json`.
2. **Important — `docs/STATUS.md` and `CHANGELOG.md` never recorded
   ART-120.** Both now do: one session-log row on STATUS covering the
   reachability fix and this review round together, and a new dated
   CHANGELOG section. Both state plainly that the real 4030-file `dist-3.2`
   tree was **not** re-run through the new fallback path end to end.
3. **Important — the destructive operation's writer changed and the preview
   never said so.** No Settings toggle was added (the user's decision was
   "native by default, named fallback", not a choice to expose). Instead
   `src/lib/preload.ts::plannedToolPhrase` labels each planned step, from
   the plan alone, before the confirmation checkbox: `import-filesystem` and
   `format-partition` are static facts (ART-117 always needs the fallback; a
   format never does), and `copy-in` names the *possibility* of ART-113
   rather than a verdict it cannot make ahead of time — a fact about that
   step's own content the plan does not scan for, the same line
   `needsExternalTool` already drew. Rendered in `VolumePreload.tsx` beside
   `stepPhrase` in the plan list.
4. **Minor — the result panel's tool label was wrong when every step fell
   back.** `run_with_fallback` set `outcome.tool = native.probe().ok()`
   before the loop ran. Fixed to track which formatter(s) actually did work
   and report accordingly: `native`'s version when every step used it, the
   fallback's when every step did, and `None` for a mixed run (the per-step
   list is what disagrees in that case, not this summary line).
5. **Minor — `preload.fallback.nonAsciiPfs3Names` had no plural forms.**
   Split into `_one`/`_other` in both catalogues. `fallbackPhrase`'s return
   shape did not need to change — i18next resolves the suffixed keys from
   `{{count}}` at `t()`-time, which `resolvesAtRuntime` in both
   `phrase-keys.test.ts` and `literal-keys.test.ts` already accounts for.

**Noted, no action** (as instructed): `core::preload::run` is reached only by
its own tests and the `#[ignore]`d real-card hook — left as is, since it is
still what a caller with exactly one formatter in hand uses, and CLAUDE.md's
core-independence rule wants it untouched by the fallback choice, which lives
in `commands/`. `is_windows_reserved_component` misses trailing-dot reserved
forms (`AUX.`) — irrelevant to AmigaDOS names, which never carry one.

## Files touched

- `src-tauri/src/commands/preload.rs` — `run_with_fallback`'s `outcome.tool`
  computed from what actually ran rather than assumed up front; two new
  tests (`every_step_falling_back_makes_the_summary_follow_the_fallback_tool`,
  plus a mixed-run assertion added to the existing
  `a_non_ascii_source_tree_falls_back_only_for_the_step_that_needs_it`) and
  one assertion added to `native_is_chosen_by_default_over_a_configured_but_
  unreachable_tool`.
- `src/lib/preload.ts` — `plannedToolPhrase`.
- `src/components/osbuilder/VolumePreload.tsx` — the plan list now shows,
  per step, which writer is expected to run it.
- `src/i18n/en.json`, `tr.json` — `preload.scope`, `layout.scope`,
  `preload.result.notVerified` rewritten; new
  `preload.plan.step.tool.{native,nativeConditional,hstImager}`;
  `preload.fallback.nonAsciiPfs3Names` split into `_one`/`_other`.
- `src/i18n/phrase-keys.test.ts` — `plannedToolPhrase` enumerated.
- `src/i18n/literal-keys.test.ts` — dynamic call count 91 → 92, with an
  explanatory entry.
- `src/lib/preload.test.ts` — `plannedToolPhrase` unit tests.
- `docs/ISSUES.md` — new `ART-121`, folding all five findings plus the two
  no-action notes into one entry (the `ART-119` pattern).
- `docs/FEATURES.md` — the "Format Amiga volumes and fill them" and
  "Preparing volumes reaches the UI" rows corrected: they described only
  `hst-imager` and "ART has no PFS3 reader", both stale since ART-120.
- `docs/STATUS.md` — session-log row, Snapshot test counts (1399 Rust / 526
  frontend) and i18n leaf-key count (1457) brought current, "Current stage"
  mentions native-by-default.
- `CHANGELOG.md` — new `### hst-imager is no longer required to prepare a
  card` section under `[Unreleased]`.

## Verification run

- `pnpm lint` — pass.
- `pnpm test` — 44 files, 526 tests, all pass (522 → 526: four new
  `plannedToolPhrase` cases in `preload.test.ts`, one in
  `phrase-keys.test.ts`).
- `cargo fmt --check` — pass. `cargo clippy --all-targets -- -D warnings` —
  clean.
- `cargo test` (src-tauri) — run twice: 1399 passed, 0 failed, 3 ignored,
  both times, no flake (1398 → 1399: one new test).

## Concerns / follow-ups not done here

- The real 4030-file `dist-3.2` tree was **not** re-run through the new
  fallback path end to end — the 969/106 non-ASCII figures in
  `docs/FEATURES.md` remain the prior, separate measurement, and both
  `STATUS.md` and `CHANGELOG.md` say so explicitly rather than implying the
  fallback closes that gap.
- The `pfs3-oracle-check.py` / `fat-oracle-check.py` / `iso-oracle-check.py`
  local oracles were not re-run this round — nothing in this wave touches
  the bytes any of them checks, only strings, a summary label and test
  coverage, so the Rust and frontend suites above are the load-bearing
  verification for this change.
