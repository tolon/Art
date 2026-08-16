# ART-120 — closing report

Commit: see below (staged on `main`).

## Decision taken

Native by default, `hst-imager` a named fallback — **chosen per step, not
per run**. `commands/preload.rs::run_with_fallback` tries `NativeFormatter`
first for every `PreloadStep` and only reaches the configured `HstImager` for
the two known capability gaps, matched on typed `CoreError` variants:
`NonAsciiPfs3Names` (ART-113) and a new `ForeignRdbEmbedNotSupported`
(ART-117, replacing the generic `NotImplemented` `import_filesystem` used to
return, so the fallback logic can tell "known gap" apart from every other
unbuilt corner of the engine). Both are safe to retry because both refuse
before writing anything. Per-step, not per-run, because `import-filesystem`
always needs the tool while `format-partition`/`copy-in` almost never do —
a run-level choice would force everything onto `hst-imager` over one
accented folder name, or waste native after one driver import.

Never silent: `StepReport { step, tool, fallback_reason }` travels on
`PreloadResult.steps`, is logged (`"Fallback"` oplog detail), and is rendered
per step in `VolumePreload.tsx` via `fallbackPhrase`. A missing tool refuses
clearly (`missing_tool_error`) before that step's formatter call, naming the
step and the reason. `core/` stayed free of the choice — `VolumeFormatter`
and `core::preload::run` are untouched; only `CoreError` gained the one
variant.

## Files touched

- `src-tauri/src/core/error.rs` — `CoreError::ForeignRdbEmbedNotSupported`.
- `src-tauri/src/core/preload/native.rs` — `import_filesystem` returns the
  new variant; oracle fixture (`build_pfs3_volume_for_oracle_when_asked`)
  gained a real `DOSDrivers/AUX` entry (ART-114's untested skip path).
- `src-tauri/src/core/preload/mod.rs` — `step_label` made `pub(crate)`.
- `src-tauri/src/commands/preload.rs` — `run_with_fallback`, `FallbackReason`,
  `StepReport`, `missing_tool_error`, `fallback_summary`; `preload_run`
  rewired; new tests (see below).
- `src/lib/preload.ts` — `FallbackReason`, `StepReport`, `fallbackPhrase`,
  `needsExternalTool`; `preloadBlocker` no longer requires a tool unless the
  plan shows `import-filesystem`.
- `src/components/osbuilder/VolumePreload.tsx` — Preview/Run no longer
  gated on the tool by default; result panel lists each step's tool and any
  fallback reason.
- `src/i18n/en.json`, `tr.json` — reworded `preload.blocked.noTool`; new
  `preload.fallback.*`, `preload.result.steps.*`.
- `src/i18n/phrase-keys.test.ts`, `src/i18n/literal-keys.test.ts`,
  `src/lib/preload.test.ts` — updated/added tests.
- `docs/ISSUES.md` — ART-120 moved Open → Fixed.
- `docs/FEATURES.md` — OS-install/preload row corrected.

## Mutation-checked properties

- **Native chosen by default**: `commands::preload::tests::
  native_is_chosen_by_default_over_a_configured_but_unreachable_tool` points
  the fallback at an `HstImager` whose exe does not exist; any code path
  using it even once for a step native can do fails with an I/O error.
  Frontend twin: `src/lib/preload.test.ts`'s `"does not require the tool
  when the plan does not need it"` and `phrase-keys.test.ts`'s explicit
  `toolPath: null` assertion.
- **Fallback fires only for the step that needs it**: `commands::preload::
  tests::a_non_ascii_source_tree_falls_back_only_for_the_step_that_needs_it`
  — a two-step plan (ASCII format, non-ASCII PFS3 copy) asserts the first
  step stays `"native"` while only the second falls back.
- **Missing tool refuses, does not half-run**: `commands::preload::tests::
  a_missing_fallback_tool_refuses_before_the_rest_of_the_plan_runs` (asserts
  format/copy never ran) plus the negative control `a_real_failure_is_not_
  treated_as_a_reason_to_fall_back` (an `Io` error is never retried).
- `core::preload::native::tests::import_filesystem_refuses_rather_than_guess`
  now asserts the specific `ForeignRdbEmbedNotSupported` variant, not just
  the error code.
- Wire shapes pinned: `commands::preload::tests::wire_shapes` (`StepReport`,
  `FallbackReason`, `PreloadResult.steps`).

## Oracle coverage gap closed

`scripts/pfs3-oracle-check.py`'s ART-114 skip-path had never run end to end.
`build_pfs3_volume_for_oracle_when_asked` now writes a real `DOSDrivers/AUX`
file (the genuine AmigaOS DOSDriver name, same as `Storage3.2.adf`/
`GlowIcons3.2.adf` carry) into the fixture tree; the live oracle run below
confirms it reports as skipped-with-a-reason, not a failure.

## Verification run

- `pnpm lint` — pass.
- `pnpm test` — 44 files, 522 tests, all pass.
- `cargo fmt --check` — pass. `cargo clippy --all-targets -- -D warnings` —
  clean.
- `cargo test` (src-tauri) — run twice: 1398 passed, 0 failed, 3 ignored,
  both times (no flake).
- `python scripts/pfs3-oracle-check.py` against
  `E:\amiga\Amigatolon\hstimager\hst.imager.exe` (exit 0): every name/size/
  protection/hash check passed both directions ("ART and hst-imager agree,
  both directions"); `DOSDrivers/AUX` reported as `1 file(s) skipped …
  Windows/MS-DOS reserved device name(s) … not a failure`.

## Concerns / follow-ups not done here

- The real 4030-file `dist-3.2` tree was **not** re-run through the new
  fallback path end to end — the 969/106 non-ASCII figures in
  `docs/FEATURES.md` remain a prior, separate measurement. `FEATURES.md`
  says this explicitly rather than implying the fallback closes that gap.
- `VolumePreload.tsx`'s new result-panel rendering has no component-level
  test (ART-118 notes this screen crashes headless Chrome past its
  headings, pre-existing and unrelated); coverage is at the `src/lib`
  logic layer (`preloadBlocker`, `fallbackPhrase`) and the Rust
  orchestration layer, not the JSX itself.
- `core::iso`'s known flake (ART-115) is unrelated and was not encountered.
