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

- [ ] Step 1: the grid and the height contract, with the page chrome removed
- [ ] Step 2: rows lose their buttons
- [ ] Step 3: `Ctrl+B`, the chevron, and persistence
- [ ] Step 4: gates and commit

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

- [ ] Step 1: tokens and the dark/light chrome
- [ ] Step 2: cursor, selection and focus
- [ ] Step 3: gates and commit

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

- [ ] Step 1: pane header and the source combo
- [ ] Step 2: the bottom dock and F6 = Move
- [ ] Step 3: status bar absorbs the selection line; footer and banner go
- [ ] Step 4: gates and commit

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

- [ ] Step 1: `containerStep` — a pure module for what a row opens into
- [ ] Step 2: enter, leave, and the cursor restored onto the container
- [ ] Step 3: partitions as a level; unsupported interiors labelled
- [ ] Step 4: history, `Backspace`, `Ctrl+PgUp`, `Ctrl+PgDn`
- [ ] Step 5: gates and commit

## Task 5 — The keyboard covers everything (brief §3.2)

He drives TC mouse-free; every mouse action needs a key. Beyond what Task 4
adds: `Space` (mark, and on a directory compute its size), `Ctrl+A` including
directories, `Num +/−/*` (mark by wildcard, invert), **type-to-search**
(letters jump the cursor to the next matching name, Esc clears), `F2`/`Ctrl+R`
refresh, `Alt+F1`/`Alt+F2` source combos, `Ctrl+B` sidebar.

- [ ] Step 1: the pure key-plan module and its tests
- [ ] Step 2: wiring, including directory sizes on `Space`
- [ ] Step 3: gates and commit

## Task 6 — Tabs and session restore (brief §3.3)

`DirTabOptions=824, DirTabLimit=32`, histories full of tab markers: tabs are
how he works, so they are required rather than deferred.

- A tab bar per pane above the path row; `Ctrl+T` duplicates, `Ctrl+W` closes,
  `Ctrl+Tab` cycles, middle-click closes. A tab may live inside a container.
- **Session restore**: tabs, per-tab paths, per-pane sort and filter survive a
  restart (`Savepath=1`, `Savepanels=1`).

- [ ] Step 1: the tab model as a pure module, with tests
- [ ] Step 2: the tab bar and its keys
- [ ] Step 3: persistence and restore
- [ ] Step 4: gates and commit

## Task 7 — The small things his config keeps on (brief §3.4, Part 2 colours)

- Every `[Confirmation]` he has enabled maps onto ART's Safety classes; none
  is dropped.
- Dropdown histories on the path row, command line and New Folder dialog.
- `UseRightButton=1` offered as a Settings toggle, default off.
- **Per-filetype colour rules**: a Settings list of `pattern → colour`,
  TC-shaped, with ART defaults — containers one colour, archives another,
  ROMs a third.

- [ ] Step 1: colour rules, with defaults and a test
- [ ] Step 2: confirmations, histories, the right-button toggle
- [ ] Step 3: gates and commit

## Task 8 — Close the phase

- [ ] `docs/FEATURES.md`, `STATUS.md`, `ISSUES.md`, `CHANGELOG.md`
- [ ] Walk Part 5's twelve acceptance points and record the result of each,
      honestly, including any that did not land

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
