# ART-113 / ART-116 — report

Both fixes land in `core::preload::native` (PFS3/FFS `copy_in`), the layer
that copies an already-built distribution tree onto a real PFS3 or FFS
volume through `libpfs3`/ART's own writer. Neither issue touches
`core/osinstall` — that module builds the distribution tree on NTFS and
never opens `libpfs3` itself; the PFS3 encoding limitation only exists on the
later, separate step (`NativeFormatter::copy_in`) that fills the real volume.
That later step is not yet wired to a Tauri command (no `commands/*.rs`
calls `VolumeFormatter::copy_in` on `NativeFormatter` today — `commands/
preload.rs` still uses `HstImager`), so neither fix crosses the Tauri
boundary as a *new* command; `CopySummary` does cross it already (via
`PreloadOutcome`/`PreloadResult`), so its two new fields are mirrored in
`src/lib/preload.ts`.

## ART-113 — refuse a non-ASCII name by name, on the PFS3 path

**What was built.** `core::preload::native::non_ascii_entries` — a pure
function over the already-flattened `entries: &[CopyEntry]` list `copy_in_pfs3`
builds before it does anything else. It filters on `!leaf_name(&entry.relative).is_ascii()`,
covering **both** files and directories, because a directory's own name goes
through the identical `name.as_bytes()` write path as a file's
(`create_dir_in` vs `write_file_in`) — this is the shape that actually
mattered on the real tree: 24 of the 969 excluded entries in the original
finding were directories.

`copy_in_pfs3` calls it as its very first statement, **before**
`FileRegionMut::open` — before the volume is even opened, not merely before
it is written to. If anything is found, it returns
`CoreError::NonAsciiPfs3Names { paths: Vec<String>, more: usize }` — a new
`CoreError` variant, not a `RefusalReason` (that enum belongs to
`core::osinstall::plan`, which this code path is downstream of and does not
call into; `copy_in`'s signature is fixed to `CoreResult<CopySummary>`, so a
typed `CoreError` variant is the equivalent shape at this layer). `paths` is
bounded to `MAX_NAMED_NON_ASCII = 20`; `more` carries the count of whatever
did not fit. FFS is untouched — the check only runs inside `copy_in_pfs3`,
never `copy_in_ffs`.

**The message.** `CoreError`'s `#[error(...)]` attributes are ordinarily a
single format string, but this one has to branch on whether `more` is
nonzero, so the sentence is built in a free function
(`non_ascii_pfs3_message`) referenced from the attribute. It says: the real
total (named + folded), every named path, "and N more" only when `more > 0`,
which crate and which two encodings disagree, and — the actionable half —
"Use FFS for this volume instead."

**Decisions worth stating:**
- *Paths are the full relative path, not the bare leaf name.* `Locale/Countries/türkçe`
  tells the user where, not just what — "which files?" is always the next
  question a bounded list like this exists to answer.
- *The bound is a constant (`MAX_NAMED_NON_ASCII = 20`), not a magic number
  inline*, so the doc comment and the test can both point at one definition.
- *The old `native.rs:742` "was created and is not listed back" sanity check
  was left in place.* It is superseded for this one cause but still the
  correct safety net for anything else that could trip it — removing it
  would have been scope creep, not a cleanup this task asked for.
- *`code()` and the `every_variant_has_a_distinct_code` test both got the new
  variant.* `CancelledPartway` was missing from that test's array before this
  change (pre-existing gap); it was added alongside the new variant rather
  than left as a growing gap.

**Tests** (`core::preload::native::tests`):
- `a_non_ascii_pfs3_file_name_is_refused_before_anything_is_written` — a
  plain file, `türkçe`. Compares the whole image byte-for-byte before/after,
  not just the error's type, so a version that refused only *after* writing
  something would still be caught.
- `a_non_ascii_pfs3_directory_name_is_refused_even_when_its_contents_are_ascii`
  — the shape that actually mattered: `español/Readme` where the directory
  name is bad and the one file inside it is pure ASCII. Proves the directory
  itself is checked, not merely everything nested under a bad name.
- `more_offending_names_than_the_bound_are_folded_into_a_count` — 25 bad
  names (`MAX_NAMED_NON_ASCII + 5`); asserts `paths.len() == 20` and
  `more == 5` exactly, not merely "some were left out".
- `the_same_non_ascii_name_copies_in_fine_on_ffs` — the identical tree,
  formatted FFS instead, copies in and reports one file. Proves the FFS
  branch is unaffected, not merely untested.
- `core::error::tests::a_non_ascii_pfs3_refusal_names_every_bounded_path_and_the_rest_as_a_count`
  and `a_non_ascii_pfs3_refusal_with_nothing_left_over_says_nothing_left_over`
  — pin the sentence itself: every path present, the true total (not just
  `paths.len()`), "and N more" only when there is one, and "FFS" named as the
  way out.

**Mutation checks (each introduced, confirmed a failure, then reverted):**
1. `non_ascii_entries` filtered `!entry.is_dir && !leaf_name(...).is_ascii()`
   (directories exempted) → failed
   `a_non_ascii_pfs3_directory_name_is_refused_even_when_its_contents_are_ascii`.
2. `more` computed as `offending.len().saturating_sub(MAX_NAMED_NON_ASCII - 1)`
   (off-by-one in the bound) → failed
   `more_offending_names_than_the_bound_are_folded_into_a_count` (`more` came
   back `6` instead of `5`).
3. The same pre-flight check added to the top of `copy_in_ffs` (a routing
   bug that ran it on both branches) → failed
   `the_same_non_ascii_name_copies_in_fine_on_ffs`.

## ART-116 — count a dropped comment/date instead of hiding them

**What was built.** `CopySummary` (`core::preload::mod.rs`) gained two
fields: `comments_lost: u64` and `dates_lost: u64`, both `#[serde(default)]`
so a summary serialized before this change still deserializes. `copy_in_pfs3`'s
existing sidecar-application block — the one that already reads a `.uaem`
and applies its protection bits through `update_dir_entry_protection` — now
also checks, from the same already-parsed `Sidecar`:
- `!parsed.comment.is_empty()` → `summary.comments_lost += 1`
- `parsed.date != AmigaDate::default()` → `summary.dates_lost += 1`

This mirrors the exact "is this actually worth mentioning" rule
`core/volume/write/copy.rs::sidecar_for` already uses when deciding whether
a sidecar is worth *writing* at all — a sidecar that exists only to carry
non-default protection bits does not count as losing a comment or a date it
never had. `copy_in_ffs` is untouched; it never increments either field,
because `FileMeta` genuinely carries both through to `add_file`/`set_attributes`.

This is explicitly not a refusal — `CopySummary` just carries more
information for whatever caller wants to report it. `src/lib/preload.ts`'s
`CopySummary` interface got the same two fields (already crossing the wire
via `PreloadResult`/`PRELOAD_EVENT`), so a future screen has them available;
no UI surface was built to display them, since the issue asked only that the
information exist, not for a new confirmation dialog.

**Struct-literal fallout.** Two existing `CopySummary { .. }` literals
(`core::preload::mod.rs`'s `Recorder::copy_in` test double, and
`tools::hst_imager.rs`'s `a_listings_summary_is_read_back` test) did not use
`..Default::default()` and would not compile with two new required fields;
both now do.

**Tests** (`core::preload::native::tests`):
- `copy_in_pfs3_counts_a_dropped_comment_and_a_dropped_date` — one sidecar
  carrying both a real comment and a real date; asserts both counters in one
  test, from the one sidecar, so neither field can be "accidentally" correct
  while the other is wrong.
- `copy_in_pfs3_does_not_count_a_sidecar_with_no_comment_or_date_to_lose` —
  the negative control: a sidecar with only protection bits set (empty
  comment, the Amiga epoch as the date). Without this, unconditionally
  incrementing both counters on any sidecar would still pass every other
  test.
- `copy_in_ffs_never_counts_anything_lost` — the identical
  comment+date-carrying sidecar, on FFS. Asserts both counters stay `0`,
  because FFS actually keeps both fields.

**Mutation checks (each introduced, confirmed a failure, then reverted):**
1. `summary.comments_lost += 1; summary.dates_lost += 1;` made
   unconditional (dropped the emptiness/default gate) in `copy_in_pfs3` →
   failed `copy_in_pfs3_does_not_count_a_sidecar_with_no_comment_or_date_to_lose`.
2. The same two unconditional increments added to `copy_in_ffs`'s file
   branch (the sidecar-is-present case) → failed
   `copy_in_ffs_never_counts_anything_lost`.

## Verification evidence

- `pnpm lint` — clean (`tsc --noEmit` twice).
- `pnpm test` — 44 files / 518 tests passed.
- `cd src-tauri && cargo fmt --check` — clean.
- `cd src-tauri && cargo clippy --all-targets -- -D warnings` — clean, twice
  (once before the mutation-check round, once after reverting it).
- `cd src-tauri && cargo test` — **1391 passed, 0 failed, 3 ignored**, run
  twice back to back. `core::iso`'s flake (ART-115, not this task's) was not
  seen in either run.

## Concurrent session note

A second session was active in this repository throughout (visible via
`git log`/`git status`: commits for ART-115's reproduction attempt landed
mid-session, and `scripts/pfs3-oracle-check.py`, `src/components/osbuilder/OsInstall.tsx`
and `src/lib/osinstall.test.ts` were left modified but uncommitted by that
session, unrelated to this one). Per this project's own standing rule, no
`git add -A` was used; only the five files this task actually touched were
staged for commit. `cargo` file-lock contention from the other session's own
builds was observed once (`Blocking waiting for file lock…`) and simply
retried — no code-level collision, since the two sessions touched disjoint
files.

## Status

Both ART-113 and ART-116 are fully closed — moved from `## Open` to
`## Fixed` in `docs/ISSUES.md` with the test names above. Nothing is left
partially done: the refusal is pre-flight (nothing is ever written for a
plan `copy_in_pfs3` would refuse), and the metadata-loss counters are wired
all the way to the one place `CopySummary` already crosses the Tauri
boundary.
