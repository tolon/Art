# Phase 2b — The Files Screen, Done Right

> Source brief: `ART-brief-files-commander-ui.md` (2026-08-12), written from
> the first human look at the running screen plus the user's own Total
> Commander `wincmd.ini`. That brief supersedes the earlier v2/v3 restyle
> briefs and is the acceptance authority; this plan is how it gets built.

**Goal:** the Files screen stops being a widget on a page and becomes the
window. Total Commander's *gestalt*, not just its components — plus the
behavioural layer the user's twenty-year-old config proves he relies on:
Enter-into-containers, full keyboard coverage, tabs, session restore, minimal
chrome.

**What this phase is not.** No new engine capability. Every reader, writer and
command it needs already exists and is tested; this is UI, wiring and one
keymap change (F6 = Move, binding a command that is already there). Where a
gap is found, it is filed as `ART-0xx` rather than smuggled in.

## Global constraints

- **i18n or it does not ship.** Every new or changed label lands in
  `en.json` *and* `tr.json` in the same commit. `pnpm test` enforces key
  parity, non-empty values, matching interpolation variables and that every
  literal `t("…")` resolves.
- `src/lib` helpers return `Phrase { key, params? }` and never import the i18n
  singleton. Pure routing/logic goes in `src/lib/*` with its own test, not
  into the 3,100-line component — the same reason `selection.ts`, `sort.ts`
  and `isoPane.ts` exist.
- Colours are **tokens**, never literals in components. The palette is the
  user's own, decoded from `wincmd.ini`.
- `cargo test` stays green and untouched unless a task says otherwise.

**Baseline: 912 Rust tests, 137 frontend tests.**

**Gates — every task ends green on all of these:**

```
pnpm lint
pnpm test
cd src-tauri && cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test
```

---

## Task 1 — The panes fill the window (brief §1.1, §1.5)

**Files:** `src/pages/FileManager.tsx`, `src/pages/FileManager.css`,
`src/components/layout/Layout.tsx`, `src/styles/*.css`, `src/stores/`

- Drop the `Files` H1 and the explainer paragraph. TC needs no intro text.
- The commander becomes a **full-bleed grid**: 50/50 panes of
  **identical height by construction** — `grid` + `minmax(0, 1fr)`, never
  content-sized flex, so the screenshot's shorter right pane is impossible —
  filling from the pane headers to the F-key row and growing with the window.
- An empty listing still paints its pane to the bottom.
- **Remove the per-row `→`/`←` and `X` buttons.** TC puts nothing on rows;
  the freed width goes to Name. The two centre buttons may stay.
- Sidebar becomes collapsible: **`Ctrl+B`** and a chevron, state persisted.

- [x] Step 1: the grid and the height contract, with the page chrome removed
- [x] Step 2: rows lose their buttons
- [x] Step 3: `Ctrl+B`, the chevron, and persistence
- [x] Step 4: gates and commit

## Task 2 — One visual world, in the user's colours (brief §1.2, Part 2)

**Files:** `src/pages/FileManager.css`, `src/styles/theme.css`,
`src/pages/FileManager.tsx`

The current screen is dark panes inside light chrome — half web app, half
terminal. Per **theme**, not per element.

| Element | `wincmd.ini` | Token |
|---|---|---|
| Pane background | 2367260 | `--tc-bg: #1C1F24` |
| Text | 16777215 | `--tc-text: #FFFFFF` |
| Cursor bar | 65535, CursorText 0 | `--tc-cursor: #FFFF00`, `--tc-cursor-text: #000000` |
| Selected names | 3947775 | `--tc-selected: #FF3C3C` |

- `InverseCursor=1`: the cursor is a **solid bar**, which the current build
  already had right. Cursor + selected = yellow bar, red text.
- Headers, path row, column headers, status lines, command line and F-key bar
  share the pane palette one step lighter (TC-style), in dark **and** light.
- The **focused pane** shows through its path row (active accent, inactive
  dimmed); the cursor bar only ever appears in the focused pane.

- [x] Step 1: tokens and the dark/light chrome
- [x] Step 2: cursor, selection and focus
- [x] Step 3: gates and commit

## Task 3 — Chrome: header, dock, F-keys (brief §1.3, §1.4)

His `[Layout]` is `ButtonBar=0, DriveBar1=0, DriveCombo=1, InterfaceFlat=1` —
no button bar, no drive bar, a drive combo.

- Pane header = `[source ▾] [path] [filter]` in one row. The combo lists
  **real, enumerated** drives plus ART's sources (Folder…, ADF…, HDF…, Disc…,
  Archive…, C64…). No hardcoded letters.
- The current button row becomes a Settings toggle, **default off**.
- Command line row directly above the F-key bar, full width, always visible.
  It navigates and filters; it does **not** run shell commands (§56 — out of
  scope, recorded).
- One F-key row docked to the bottom: `F3 View · F4 Edit · F5 Copy ·
  F6 Move · F7 NewFolder · F8 Delete · F9 Attributes`. Never wraps.
- **F6 becomes Move** (Shift+F6 = rename in place). Keymap, labels, i18n.
- The selection line merges into the pane status bar, TC format:
  `880 k / 24,640 k in 1 / 28 file(s)`.
- The permanent collision-policy footer moves into the copy dialog; the red
  "both panes are local" banner becomes a status-bar hint.

- [x] Step 1: pane header and the source combo
- [x] Step 2: the bottom dock and F6 = Move
- [x] Step 3: status bar absorbs the selection line; footer and banner go
- [x] Step 4: gates and commit

**What changed against the plan, and why (2026-08-12):**

- **F2 / Ctrl+R refresh came forward from Task 5.** Hiding the button strip
  left Refresh with no other way to reach it — Up is the `[..]` row, New folder
  is F7, every source is in the combo. Shipping a task's worth of screen with
  no way to re-read a pane was not an option.
- **F6 = Move is narrower than "bind a command that is already there".**
  Three directions have no primitive underneath, and each is refused by name
  rather than half-implemented: moving *out of* a host folder (ART owns no
  host-side delete — **ART-080**), several entries between two images
  (ART-064), and a single *file* between two images (`volume_copy_between`
  addresses a directory — **ART-081**). Volume → host folder, and one folder
  between two images, both work.
- **Move verifies before it deletes.** The destination is re-listed and every
  moved name looked for in it; a copy that reported success and a destination
  that does not hold the file are the same thing as far as the user's data is
  concerned. A collision is refused up front rather than handed to the
  overwrite policy — "leave it alone" would skip the copy and then delete the
  source.
- **`VolumeFooter` became a chrome row** and took the drive row's "free of
  total" with it, so hiding the button strip loses no information.
- **`CheckoutPanel` moved inside the commander**, above the status strip: a
  panel below the F-key row would be a docked strip that is not at the bottom.
- **The error / message / busy lines moved into the dock too**, as one status
  strip with three levels. The brief only names the "both panes are local"
  banner, but all three had the same fault — they pushed the panes down from
  the top of the page.

## Task 4 — Enter opens the container (brief §3.1, the headline)

For twenty years Enter on an ADF has meant *step inside it*. ART has every
reader this needs; the semantics are what is missing.

- Enter / double-click on a recognised container → **the same pane navigates
  into it**, pane kind switching underneath, breadcrumb showing the container
  step (`E:\amiga\Games\Lotus.adf\`).
- First row `[..]`; Enter on it at a container root leaves to the host folder
  **with the cursor back on the container file**. `Backspace` and `Ctrl+PgUp`
  go up; `Ctrl+PgDn` force-enters.
- A multi-partition HDF lists its partitions as a level.
- An interior ART cannot read (PFS3, TAP) shows the honest label — never an
  error toast.
- Per-pane history treats container levels as steps (`Alt+Left/Right`).
- Studios stay reachable from the context menu. The commander walks; the
  studios operate.

- [x] Step 1: `containerStep` — a pure module for what a row opens into
- [x] Step 2: enter, leave, and the cursor restored onto the container
- [x] Step 3: partitions as a level; unsupported interiors labelled
- [x] Step 4: history, `Backspace`, `Ctrl+PgUp`, `Ctrl+PgDn`
- [x] Step 5: gates and commit

## Task 5 — The keyboard covers everything (brief §3.2)

He drives TC mouse-free; every mouse action needs a key. Beyond what Task 4
adds: `Space` (mark, and on a directory compute its size), `Ctrl+A` including
directories, `Num +/−/*` (mark by wildcard, invert), **type-to-search**
(letters jump the cursor to the next matching name, Esc clears), `F2`/`Ctrl+R`
refresh, `Alt+F1`/`Alt+F2` source combos, `Ctrl+B` sidebar.

- [x] Step 1: the pure key-plan module and its tests
- [x] Step 2: wiring — **except directory sizes on `Space`**, which needs a
      primitive that does not exist and is filed as ART-087
- [x] Step 3: gates and commit

## Task 6 — Tabs and session restore (brief §3.3)

`DirTabOptions=824, DirTabLimit=32`, histories full of tab markers: tabs are
how he works, so they are required rather than deferred.

- A tab bar per pane above the path row; `Ctrl+T` duplicates, `Ctrl+W` closes,
  `Ctrl+Tab` cycles, middle-click closes. A tab may live inside a container.
- **Session restore**: tabs, per-tab paths, per-pane sort and filter survive a
  restart (`Savepath=1`, `Savepanels=1`).

- [x] Step 1: the tab model as a pure module, with tests
- [x] Step 2: the tab bar and its keys
- [x] Step 3: persistence and restore
- [x] Step 4: gates and commit

## Task 7 — The small things his config keeps on (brief §3.4, Part 2 colours)

- Every `[Confirmation]` he has enabled maps onto ART's Safety classes; none
  is dropped.
- Dropdown histories on the path row, command line and New Folder dialog.
- `UseRightButton=1` offered as a Settings toggle, default off.
- **Per-filetype colour rules**: a Settings list of `pattern → colour`,
  TC-shaped, with ART defaults — containers one colour, archives another,
  ROMs a third.

- [x] Step 1: colour rules, with defaults and a test
- [x] Step 2: confirmations, histories, the right-button toggle —
      the audit found ART-088 (the writer ignores the `d` bit)
- [x] Step 3: gates and commit

## Task 8 — Close the phase

- [x] `docs/FEATURES.md`, `STATUS.md`, `ISSUES.md`, `CHANGELOG.md`
- [x] Walk Part 5's twelve acceptance points and record the result of each,
      honestly, including any that did not land — see below

---

## Acceptance (brief Part 5, verbatim in intent)

The next screenshot is judged against these, so they are repeated here rather
than referenced:

1. Resized window → panes always equal, always filling; no dead space.
2. Dark theme → zero light chrome inside Files; light theme → zero dark blocks.
3. Solid yellow cursor bar with black text; selected names red; the cursor
   only in the focused pane; side by side with his TC it reads as a sibling.
4. No buttons inside rows.
5. One F-key row docked at the bottom, command line directly above, F6 says
   Move (tr: **Taşı**).
6. No permanent collision footer, no intro paragraph, no alert banner.
7. Default pane header is combo + path + filter; the button bar only via
   Settings.
8. Enter on `Workbench.adf` lists its contents in the same pane; Backspace
   returns; the breadcrumb shows the container step; Enter on a
   multi-partition HDF shows DH0:/DH1:.
9. A full session — navigate, enter an ADF, mark five files, F5 to the other
   pane, exit — with zero mouse touches.
10. `Ctrl+B` hides the sidebar; the panes widen.
11. Two tabs on the right pane survive a restart with their paths and sort
    orders.
12. `pnpm test` green including en/tr parity for every new label; `cargo test`
    green.

## Out of scope, recorded so it is not relitigated

- Lister/content/filesystem plugins (WLX/WDX/WFX). F3 covers text and hex;
  format-aware preview is later work.
- Running shell commands from the command line (`cm_ExecuteDOS`) — §56.
- RAR — the licence decision already recorded in the Phase 2a plan.

---

# The acceptance walk (2026-08-12)

Part 5's twelve points, each with what actually happened. **Six were verified
on the running screen, four on tests alone, and two did not get looked at** —
which is written down rather than rounded up, because rounding it up is what
[ART-062](../../ISSUES.md#open) has cost this project twice.

| # | Point | Result |
|---|---|---|
| 1 | Resized window → panes equal, filling, no dead space | ✅ **Seen.** And it is the point that earned its keep: the first look found [ART-082](../../ISSUES.md#open) — the panes filled the window and their *listings* did not, capped at 420 px behind 178 green tests. Fixed, re-checked on screen |
| 2 | Dark theme → no light chrome; light theme → no dark blocks | 🟡 **Half seen.** Dark is verified on screen and is coherent. **The light theme was never opened.** Its tokens exist and derive from the dark ones by role; nobody has looked |
| 3 | Yellow cursor bar, black text; selected names red; cursor only in the focused pane | 🟡 **Mostly seen.** The solid yellow bar with black text is in the screenshots, and it appears in one pane only. **Red selected names were not exercised on screen.** "Reads as a sibling of his TC" is the user's judgement, not one ART can award itself |
| 4 | No buttons inside rows | ✅ **Seen.** Rows carry an icon, a name and columns; nothing clickable |
| 5 | One F-key row docked at the bottom, command line directly above, F6 says Move (tr: **Taşı**) | ✅ **Seen**, in both languages |
| 6 | No permanent collision footer, no intro paragraph, no alert banner | ✅ **Seen.** All three are gone; what is left is one status strip inside the dock |
| 7 | Default pane header is combo + path + filter; the button bar only via Settings | ✅ **Seen** |
| 8 | Enter on an ADF lists it in the same pane; Backspace returns; the breadcrumb shows the container step; a multi-partition HDF shows its partitions | 🟡 **Mostly seen.** Enter on `art-bootable-test.adf` opened the disk in the pane — breadcrumb `…	estrt-bootable-test.adf`, volume row `Work FFS 877 k of 880 k free` — and Backspace came back out **with the cursor on the ADF it had entered**. The **multi-partition HDF** case was not tried: no such image was to hand. The code path is the one the partition list has always used |
| 9 | A full session — navigate, enter an ADF, mark five, F5, exit — with zero mouse touches | 🟡 **Not walked end to end.** Every key in it exists and is unit-tested, and the navigate/enter/leave half was driven by keyboard on screen. The marking and copying half was not |
| 10 | `Ctrl+B` hides the sidebar; the panes widen | 🟡 **Not re-checked.** Landed in task 1 and unchanged since; not exercised this session |
| 11 | Two tabs on the right pane survive a restart with their paths and sort orders | ❌ **Not verified.** This is the one that matters most on this list, because it is the only claim here that cannot be true "by construction": it needs the application closed and reopened. The model has 19 tests and the store round-trip has none |
| 12 | `pnpm test` green including en/tr parity for every new label; `cargo test` green | ✅ **279 frontend, 912 Rust**, plus `tsc`, `cargo fmt --check` and `clippy -D warnings` |

**The honest summary:** the phase's headline — Enter opens the container, in
the same pane — is verified on real screen with a real disk. Its second
headline, session restore, is not verified at all. Those two facts belong next
to each other rather than averaged into "done".

## What was filed rather than built

- **ART-087** — `Space` marks but does not compute a directory's size
  (`CountSpace=1`). No primitive exists to count with; it needs a
  depth-limited walk per side of the fence, as jobs, plus a "counting…" state
  in the Size column.
- **ART-088** — the volume writer deletes a delete-protected entry without
  noticing the `d` bit. The file manager now asks; the writer still does not
  refuse, and `volume_put_file` does not check `w` either.
- Dropdown histories landed on the **command line** only. The path row is a
  label rather than an input, and New Folder is a `window.prompt`, which
  cannot carry one — both need a real dialog first.
