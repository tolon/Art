# Phase 1a — The Commander Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Turn ART's two-pane manager into a real commander — focus and Tab, multi-select, batch copy and delete, several `.lha` archives onto a disk in one operation, sorting and a filter.

**Architecture:** The Rust core already copies a directory tree recursively, plans a copy before it runs, and stages volume-to-volume through a temp folder. Almost none of this phase needs a new copy engine. `plan_copy` already takes `&[SourceEntry]`, and `copy_into_volume` takes `&dyn CopySource` — so a **multi-root `CopySource`** turns every batch operation into the existing, tested one. The frontend work is larger than the Rust work: selection is singular end to end and "the active pane" is derived from which pane happens to hold a selection, which has to become real focus before any keyboard feature can stand on it.

**Tech Stack:** Rust (Tauri 2 commands, `core/volume/write`), React 18 + TypeScript, `react-i18next`, Vitest.

## Global Constraints

- `src-tauri/src/core/` is platform-independent: `std` + `serde` + `sha2` + `thiserror` + `delharc` only. Never `use tauri`, never call Windows APIs, never touch the network. `commands/` is where Tauri lives.
- **MSRV is 1.77.** `Option::is_none_or` (1.82) and trait-object upcasting (1.86) compile locally and fail CI.
- Release profile is `panic = "abort"`: never index a block directly — reads go through `blocks::block_slice` / `read_u32_at`, writes through `BlockSet`. Chain walks need a step limit. Never allocate from an unchecked length; use `checked_add` / `checked_mul`.
- Every write to a user file goes through `core/safety` (`atomic_write` / `guarded_write`). **A failed operation leaves the original byte-for-byte unchanged**, and there is a test proving it.
- Anything that modifies data follows `SOURCE → ANALYZE → VALIDATE → RECOMMEND → PREVIEW → BACKUP → APPLY → VERIFY → REPORT` (§92). A batch operation must **preview the whole batch** before writing any of it.
- Long operations run through `spawn_job` with a `ProgressSink`. `is_cancelled()` is checked **only between whole units of work**, never mid-write. Return `CoreError::Cancelled` when you stop, and **commit nothing** — a cancelled batch must leave the image as it was.
- Untrusted names from an archive or a disk go through `core/security/path.rs::safe_join()`.
- Commands are registered in **both** `lib.rs`'s `invoke_handler![]` and a typed wrapper in `src/lib/*.ts`. Components never call `invoke` directly.
- Errors reaching the UI are readable sentences carrying a stable `ART-*` id from `CoreError::code()`. A refusal is data, not an error — it renders in the calm panel, never the red banner.
- `usePowerMode()`'s Beginner mode **only hides**. Never disable an operation because of it and never change what ART does.
- Every user-visible string goes in **both** `en.json` and `tr.json` in the same commit. `pnpm test` enforces identical key sets, no empty values, matching interpolation variables, and that every literal `t("…")` key resolves. `src/lib` helpers return `Phrase { key, params? }` — they must never import the i18n singleton.
- Fixtures are synthetic and generated at runtime in a tempdir. **ART ships no copyrighted Amiga content, ever.**
- Never claim support that is not implemented and tested (spec §10, §89).

**Gates — every task ends green on all of these, run from `amiga-retro-toolkit`:**

```
pnpm lint
pnpm test
cd src-tauri && cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test
python scripts/oracle-check.py
```

**Baseline: 683 Rust tests, 18 frontend tests, oracle 48 checks, 814 catalogue keys per language.** **Run `cargo test` twice** before declaring a task done — ART-059 was a race that failed about one run in five.

---

## What already exists — do not rebuild it

A survey of the tree established these. Building any of them again is wasted work:

| Capability | Where | Note |
|---|---|---|
| Recursive folder → volume copy | `core/volume/write/copy.rs::copy_into_volume` | depth-capped at 16, 20 000 entries |
| Cost preview before writing | `core/volume/write/plan.rs::plan_copy` | **already takes `&[SourceEntry]`, not one root** |
| **Volume → volume copy** | `commands/volume_write.rs::volume_copy_between` | ADF↔HDF, gigabyte images, staged through a temp folder |
| Volume → local extract | `volume_copy_out` | |
| Archive → volume install | `commands/sources.rs::install_archive_into_volume` | private to that module; one archive only |
| Background jobs + cancel | `spawn_job`, `ProgressSink` | cancel checked between files |
| One abstraction over ADF and HDF | `PaneState` in `FileManager.tsx` | an ADF is volume 0; below that they are the same |

**Volume-to-volume copy is not a gap.** The gap is that no Rust test exercises the `volume_copy_between` *command*; only the lower-level staging logic in `copy.rs` is covered. Task 8 closes that.

## File Structure

| File | Responsibility |
|---|---|
| `src-tauri/src/core/volume/write/copy.rs` | **add** `HostSelection` — a `CopySource` spanning several roots |
| `src-tauri/src/commands/volume_write.rs` | **add** `volume_plan_copy_many`, `volume_copy_in_many`, `volume_delete_many` |
| `src-tauri/src/commands/archives.rs` | **new** — `archives_plan_install`, `archives_install` for several `.lha` at once |
| `src/lib/volumeWrite.ts` | typed wrappers for the three new volume commands |
| `src/lib/archives.ts` | **new** — typed wrappers for the archive batch |
| `src/pages/FileManager.tsx` | focus, Tab, multi-select, sort, filter |
| `src/components/files/SelectionBar.tsx` | **new** — what is selected and what can be done with it |
| `src/components/files/CopyPlanDialog.tsx` | render a batch plan, not only a single-root one |
| `src/i18n/en.json`, `src/i18n/tr.json` | roughly 25 new keys under `files.*` |

---

## Task 1: Real pane focus, and Tab between panes

**Files:**
- Modify: `src/pages/FileManager.tsx`
- Modify: `src/components/files/FunctionKeys.tsx`
- Modify: `src/i18n/en.json`, `src/i18n/tr.json`

**Interfaces:**
- Consumes: nothing.
- Produces: `const [focused, setFocused] = useState<Side>("left")` replacing the derived `active`, and the invariant that **every F-key action reads `focused`, never `selection`**. Task 2 depends on this.

**Context.** Today `const active: Side = selection.left !== null ? "left" : "right"` (`FileManager.tsx:959`). The active pane is a side effect of which pane holds a selection, and every navigation resets `selection[side]` to `null`. Multi-select makes that untenable — a pane can have many selections or none and still be the one the keyboard is talking to.

- [ ] **Step 1: Replace the derived active pane with tracked focus**

Add `const [focused, setFocused] = useState<Side>("left");` and delete the derived `active`. Set focus in the pane's click handler and whenever a pane is opened or navigated. Every reader of `active` becomes a reader of `focused`.

- [ ] **Step 2: Show which pane has focus**

The focused pane needs a visible border or header treatment — a commander where you cannot see which side the keyboard is talking to is worse than one with no keyboard at all. Use an existing accent token from `src/styles/global.css`; do not introduce a new colour.

- [ ] **Step 3: Bind Tab**

Extend `useFunctionKeys` (or add a sibling hook) so Tab moves focus to the other pane. Follow the existing guards exactly: **ignore the key when focus is in an `input`, `textarea`, `select` or `contenteditable`, and ignore any keydown with a modifier held** (`FunctionKeys.tsx:35-64`). Call `preventDefault()` so Tab does not also move browser focus.

- [ ] **Step 4: Write the test**

There is no frontend test for this page today; this is the first. Add `src/pages/FileManager.focus.test.tsx` using Vitest. If a DOM is needed, add `jsdom` and `@testing-library/react` as dev dependencies and register the environment in a vitest config — remember `tsconfig.test.json` is where test-only types belong.

```tsx
it("moves focus to the other pane on Tab, and not when typing in a filter box", () => {
  // render, assert left has focus, press Tab, assert right has focus,
  // focus an <input>, press Tab, assert the pane focus did not change
});
```

- [ ] **Step 5: Run the gates and commit**

```
pnpm lint && pnpm test
```
then commit `feat: give the panes real focus and bind Tab`.

---

## Task 2: Multi-select

**Files:**
- Modify: `src/pages/FileManager.tsx`
- Create: `src/components/files/SelectionBar.tsx`
- Modify: `src/i18n/en.json`, `src/i18n/tr.json`

**Interfaces:**
- Consumes: `focused` from Task 1.
- Produces: `selection: Record<Side, Set<string>>` — a set of entry **names** per side, matching how the code already identifies an entry (`PanelEntry` has no id). Tasks 4 and 5 consume this. Also `selectedEntries(side): PanelEntry[]`, the accessor every F-key handler uses.

**Context.** `PanelEntry` (`src/lib/panel.ts:10-20`) has `name`, `is_dir`, `bytes`, `path` (local) and `header_block` (volume). Names are unique within a directory on both sides, so a `Set<string>` is a sound key. Selection resets on navigation, as it does today.

- [ ] **Step 1: Change the selection shape**

```ts
const [selection, setSelection] = useState<Record<Side, Set<string>>>({
  left: new Set(),
  right: new Set(),
});
```

Add a helper, and use it everywhere an F-key handler currently does `entries.find((e) => e.name === selection[active])`:

```ts
const selectedEntries = (side: Side): PanelEntry[] =>
  pane(side).entries.filter((e) => selection[side].has(e.name));
```

- [ ] **Step 2: Bind the selection keys and clicks**

- Plain click — select only that entry.
- **Ctrl+click** — toggle that entry, keep the rest.
- **Shift+click** — select the range from the last-clicked entry to this one. Track `anchor: Record<Side, string | null>` for that.
- **Insert** — toggle the entry under the cursor and move down one, the Norton Commander behaviour.
- **Ctrl+A** — select all in the focused pane; again to clear.

Respect the existing guard: no keyboard handling while focus is in an input, and none when an unexpected modifier is held.

- [ ] **Step 3: Show the selection**

Create `SelectionBar.tsx`: how many entries are selected and their total size, or nothing when the selection is empty. Keep the Turkish short — this sits above the function-key bar, which is already the tightest strip in the app.

```tsx
export function SelectionBar({ count, bytes }: { count: number; bytes: number }) {
  const { t } = useTranslation();
  if (count === 0) return null;
  return <div className="selection-bar">{t("files.selection.summary", { count, size: formatBytes(bytes) })}</div>;
}
```

Selected rows need a visible treatment distinct from the cursor row. Use an existing token.

- [ ] **Step 4: Keep every existing action working on exactly one entry**

F3 View, F4 Edit, F6 Rename and F9 Attributes are single-entry operations. With several selected they must **refuse clearly rather than acting on an arbitrary one** — disable the key and say why, using the `*ReasonSelect` keys that already exist in `files.functionKeys.*`. Add a plural reason key where one is missing.

- [ ] **Step 5: Test**

```tsx
it("selects a range with shift-click and toggles one with ctrl-click", ...)
it("refuses rename when more than one entry is selected", ...)
```

- [ ] **Step 6: Gates and commit**

---

## Task 3: A `CopySource` that spans several roots

**Files:**
- Modify: `src-tauri/src/core/volume/write/copy.rs`

**Interfaces:**
- Consumes: the existing `CopySource` trait and `HostFolder`.
- Produces:

```rust
/// Several picked entries — files, folders, or a mix — copied as one operation.
///
/// Each root keeps its own base name at the destination, so picking
/// `Game/` and `Readme.txt` produces `Game/` and `Readme.txt` side by side.
pub struct HostSelection {
    roots: Vec<PathBuf>,
}

impl HostSelection {
    pub fn new(roots: Vec<PathBuf>) -> Self { Self { roots } }
}

impl CopySource for HostSelection { /* … */ }
```

**Context.** This is the whole of the Rust batch work. `copy_into_volume` takes `&dyn CopySource`; `plan_copy` already takes `&[SourceEntry]`. Give the existing engine a source that spans several roots and every batch operation becomes the tested single-root one.

- [ ] **Step 1: Read `HostFolder` first**

Read `copy.rs` around `HostFolder::entries` and `walk` before writing anything. `HostSelection` must produce entries in the same shape and obey the same limits — `MAX_COPY_DEPTH` (16) and `MAX_COPY_ENTRIES` (20 000) apply to the **whole selection**, not per root. A selection of 200 folders must not multiply the cap by 200.

- [ ] **Step 2: Write the failing tests**

```rust
#[test]
fn a_selection_of_files_and_folders_copies_each_at_the_top_level() { /* … */ }

#[test]
fn a_selection_obeys_the_entry_cap_across_all_roots_not_per_root() { /* … */ }

#[test]
fn two_roots_with_the_same_base_name_are_refused_rather_than_silently_merged() { /* … */ }
```

That third case matters: `C:\a\Docs` and `D:\b\Docs` both land as `Docs`. Refuse with a `CoreError` naming both paths — a silent merge would interleave two unrelated trees.

- [ ] **Step 3: Run them and watch them fail**

`cargo test host_selection` — expect failures for a type that does not exist.

- [ ] **Step 4: Implement**

- [ ] **Step 5: Run them and watch them pass, then run the whole suite twice**

- [ ] **Step 6: Commit**

---

## Task 4: Batch copy and batch delete through the commands

**Files:**
- Modify: `src-tauri/src/commands/volume_write.rs`
- Modify: `src-tauri/src/lib.rs`, `src/lib/volumeWrite.ts`
- Modify: `src/pages/FileManager.tsx`, `src/components/files/CopyPlanDialog.tsx`

**Interfaces:**
- Consumes: `HostSelection` from Task 3, `selectedEntries` from Task 2.
- Produces: `volume_plan_copy_many(image, volumeIndex, dirBlock, sources: Vec<String>)`, `volume_copy_in_many(…, policy)` returning a job id, and `volume_delete_many(image, volumeIndex, dirBlock, names: Vec<String>)`.

- [ ] **Step 1: The plan command**

`volume_plan_copy_many` builds a `HostSelection` and calls the existing `plan_copy`. It **must not write** — the existing `a_copy_plan_reports_the_cost_without_touching_the_image` test is the pattern to copy.

- [ ] **Step 2: The copy command**

`volume_copy_in_many` goes through `spawn_job` exactly as `volume_copy_in` does, and reuses `run_copy_in_folder_with(.., OnCancel::Abandon, ..)`. **`OnCancel::Abandon` is not optional here**: a cancelled batch commits nothing. Test it by cancelling mid-batch and asserting the image is byte-for-byte unchanged.

- [ ] **Step 3: The delete command**

Batch delete is where §92 bites hardest. It must:
- refuse the whole batch atomically if any entry cannot be deleted, before deleting any of them;
- report what would be deleted, with sizes, so the UI can confirm;
- back the image up once for the batch, not once per entry — `a_batch_copy_backs_the_image_up_once_not_once_per_file` is the existing precedent.

- [ ] **Step 4: The preview dialog**

`CopyPlanDialog` currently describes one root. Extend it to list the batch — how many files, how many folders, total size, blocks needed against blocks free, and the name collisions. Do not remove the single-root rendering; a one-entry batch should read naturally, not as "1 items".

- [ ] **Step 5: Tests**

Rust: batch plan does not touch the image; batch copy lands everything; a cancelled batch leaves the image byte-identical; a batch delete that cannot complete deletes nothing.

- [ ] **Step 6: Gates and commit**

---

## Task 5: Several `.lha` archives onto a disk in one operation

**Files:**
- Create: `src-tauri/src/commands/archives.rs`
- Modify: `src-tauri/src/commands/sources.rs` (make the shared body reachable)
- Modify: `src-tauri/src/lib.rs`
- Create: `src/lib/archives.ts`
- Modify: `src/pages/FileManager.tsx`
- Modify: `src/i18n/en.json`, `src/i18n/tr.json`

**Interfaces:**
- Consumes: `install_archive_into_volume` (currently private in `sources.rs`), `plan_copy_in_folder`, `run_copy_in_folder_with`.
- Produces: `archives_plan_install(archives: Vec<String>, image, volumeIndex, dirBlock)` → a plan describing every archive, and `archives_install(…, policy)` → a job id.

**Context.** Nothing today installs more than one archive per operation — `sources_install_adf`, `sources_install_volume` and `whdload_install` each take a single `archive: String`. `install_archive_into_volume` is close to generic but is a private `fn` in `sources.rs`. Make it `pub(crate)` and move it somewhere both callers can reach, or wrap the same primitives; say which you chose and why.

- [ ] **Step 1: Decide where each archive's contents land, and show it**

An Amiga user adding five game archives to a disk expects five drawers, not five archives' contents merged into one directory. The rule:

- if the archive's contents have exactly one top-level directory, use that name;
- otherwise create a drawer named after the archive's file stem.

**The plan must show the resulting drawer name for every archive before anything is written** (§92). That is what resolves the ambiguity — the user sees the five names and can cancel.

- [ ] **Step 2: The plan command**

`archives_plan_install` unpacks each archive to a scratch directory, works out its drawer name, and runs the existing `plan_copy` over the union. It returns per-archive rows plus a total, and a refusal — as **data**, not an error — when the batch does not fit, naming blocks needed against blocks free.

Clean up every scratch directory even when the plan is refused. `the_staging_folder_removes_itself` is the existing precedent.

- [ ] **Step 3: The install command**

`spawn_job`, `ProgressSink` reporting per archive, `OnCancel::Abandon`. Cancelling after two of five archives must leave the image as it was — not two games installed.

- [ ] **Step 4: The UI**

Selecting several `.lha` files in a local pane and pressing F5 into a volume pane runs this path instead of a plain file copy. The plan dialog lists the drawers. Nothing else about F5 changes.

- [ ] **Step 5: Tests**

Rust, with synthetic archives built at runtime:
- three archives install into three drawers, each with its contents;
- an archive with one top-level directory uses that name; one without uses the stem;
- a batch that does not fit is refused before anything is written, image byte-identical;
- cancelling after the first archive leaves the image byte-identical;
- two archives whose drawer names collide are reported, not silently merged.

- [ ] **Step 6: Gates and commit**

---

## Task 6: One sort rule, and column headers that change it

**Files:**
- Modify: `src-tauri/src/core/volume/write/dir.rs`, `src-tauri/src/core/adf/fs.rs`, `src-tauri/src/commands/panel.rs`
- Modify: `src/pages/FileManager.tsx`
- Modify: `src/i18n/en.json`, `src/i18n/tr.json`

**Interfaces:**
- Consumes: nothing.
- Produces: a stable listing order from every source, and client-side sort state `{ column: "name" | "size" | "date", direction: "asc" | "desc" }` per pane.

**Context, and the thing to get right.** Today there are **three different orders**: local folders are sorted folders-first then case-insensitive name (`commands/panel.rs:107-113`); ADF directories are sorted by name only, not folders-first (`core/adf/fs.rs:90-91`); and HDF partition listings are **not sorted at all** (`core/volume/write/dir.rs:148-184` walks hash buckets in bucket order). Adding a client-side sort on top of three different server behaviours produces a control that behaves differently pane to pane.

**Fix the floor first:** make every source return folders-first then case-insensitive name, so an unsorted pane is never possible. Then add the client-side sort on top of a known baseline.

Note why the ADF comment says what it says: Amiga directories genuinely have no intrinsic order, which is the reason to impose one, not to avoid it. Sorting the listing does not touch the on-disk hash chains.

- [ ] **Step 1: Test the floor, in Rust**

A test per source asserting folders-first then case-insensitive name, including the HDF path that has no order today.

- [ ] **Step 2: Make them pass**

- [ ] **Step 3: Client-side sort**

Clicking a column header sorts by it; clicking again reverses. Folders stay first in both directions — a commander that scatters directories through the file list is not one. Sort state is per pane and resets on navigation.

- [ ] **Step 4: Frontend test**

- [ ] **Step 5: Gates and commit**

---

## Task 7: The filter box

**Files:**
- Modify: `src/pages/FileManager.tsx`
- Modify: `src/i18n/en.json`, `src/i18n/tr.json`

**Interfaces:**
- Consumes: the sort state from Task 6.
- Produces: `filter: Record<Side, string>`.

- [ ] **Step 1: Add a per-pane filter input**

A plain substring match, case-insensitive, plus `*` and `?` wildcards, which is what a commander user expects from a mask. Filtering is display-only — **it must never change what an action operates on**. A selected entry that the filter then hides must not be silently copied. Decide the rule, state it in the report, and test it: either clear the selection when the filter changes, or keep hidden selections and show their count. Prefer clearing; a hidden selection is a surprise.

- [ ] **Step 2: The empty state**

A filter matching nothing says so, rather than showing a blank pane that reads as a broken listing.

- [ ] **Step 3: Keyboard**

The filter input must not swallow the F-keys, and the F-key handler must keep ignoring keystrokes while the input has focus. Escape clears the filter and returns focus to the pane.

- [ ] **Step 4: Test**

```tsx
it("hides non-matching entries without changing what F5 would copy", ...)
```

- [ ] **Step 5: Gates and commit**

---

## Task 8: Close the phase

**Files:**
- Modify: `src-tauri/src/commands/volume_write.rs` (the missing test)
- Modify: `docs/FEATURES.md`, `docs/STATUS.md`, `docs/ISSUES.md`, `CHANGELOG.md`

**Interfaces:**
- Consumes: everything above.

- [ ] **Step 1: Cover `volume_copy_between` at the command level**

The survey found that volume-to-volume copy ships and is wired into F5, but **no Rust test exercises the command** — only the lower-level staging logic in `copy.rs`. Add an end-to-end test: two synthetic images, copy a tree from one into the other through `volume_copy_between`, verify the contents and that the source is unchanged. This is the oldest untested write path in the panel.

- [ ] **Step 2: Documents**

- `docs/FEATURES.md` — flip only rows a test now covers.
- `docs/STATUS.md` — snapshot numbers and a session-log line.
- `docs/ISSUES.md` — record anything found and not fixed.
- `CHANGELOG.md` — a user-visible entry.

State plainly that **nothing has been checked on a running screen** (ART-062 is still open) and that nothing has been verified on real hardware.

- [ ] **Step 3: Gates and commit**

---

## Self-Review

**Spec coverage.** The roadmap's slice 1.1 asks for Shift/Ctrl/Insert selection, several `.lha` files onto a disk at once, an ADF's contents onto a disk, and batch copy and delete as one job — Tasks 2, 5 and 4 cover the first, second and fourth. **The third already ships** (`volume_copy_between`), so Task 8 covers it with the test it never had instead of rebuilding it. Slice 1.2 asks for sorting, a mask filter, Tab between panes, history and favourites, panel synchronisation, size calculation and directory comparison — Tasks 1, 6 and 7 cover Tab, sorting and the filter. **History, favourites, panel synchronisation, size calculation and directory comparison are deliberately not in this plan**; they are independent conveniences, and this plan is already eight tasks. They belong in a Phase 1b.

**Placeholders.** None: every task names its files, its commands and its expected result, and the two design decisions that could have been left vague — where an archive's contents land, and what a filter does to a selection — are decided in the text with the reasoning.

**Type consistency.** `Side`, `PaneState` and `PanelEntry` are the existing types and are used with those names throughout. `focused` is introduced in Task 1's Produces and consumed by name in Task 2. `selection: Record<Side, Set<string>>` and `selectedEntries(side)` are introduced in Task 2's Produces and consumed by name in Tasks 4 and 5. `HostSelection` is introduced in Task 3's Produces and consumed by name in Task 4.

**The risk this plan is most likely to be wrong about.** Task 1 says focus replaces a derived value, and Task 2 says selection becomes a set — both touch nearly every handler in a 1600-line component that has **no frontend test coverage at all** today. That is why Task 1 writes the first test rather than deferring tests to the end, and why the two are separate tasks with a review between them.
