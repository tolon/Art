<!--
Provenance: handed over 2026-08-12 as `ART-brief-files-commander-ui.md` at the
repository's parent folder. Copied here and **this copy is the one that is
maintained**, for the same reason the SD gap analysis is: a brief that lives
outside the repository it briefs drifts from it. The plan built from it is
docs/superpowers/plans/2026-08-12-phase-2b-commander-ui.md.
-->

# Files Screen — The Commander, Done Right
## Single comprehensive UI/UX brief. Supersedes the earlier v2/v3 restyle briefs.
## Sources: first human review of the running screen (screenshot, 2026-08-12)
## + the user's own Total Commander `wincmd.ini` (20+ years of daily use, decoded).

The first restyle got the TC *components* right (separate Ext column, `[..]`,
`<DIR>`, bracketed dirs, TC-format status line, command line, F-keys). It
missed the *gestalt*: **Total Commander is not a widget on a page; it IS the
window.** And it missed the behavioural layer the user's own config proves he
relies on: Enter-into-containers, full keyboard coverage, tabs, session
restore, minimal chrome.

Everything below is UI/UX + wiring of existing engine primitives. No new
engine capability except where F6=Move binds an existing command to a new
key. i18n: every new/changed label lands in `en.json` AND `tr.json` in the
same commit; `pnpm test` stays green. Gaps discovered on the way are filed
as `ART-0xx` per ISSUES.md convention.

---

# PART 1 — LAYOUT

## 1.1 The panes fill the screen (the big one)

- The Files route drops the page padding, the "Files" H1 and the explainer
  paragraph entirely. TC needs no intro text.
- The two panes become a **full-bleed grid**: 50/50 width, **identical
  height by construction** (`grid` + `minmax(0,1fr)`, never content-sized
  flex — the screenshot's shorter right pane must be impossible), stretching
  from the pane headers to the command-line row, growing with the window.
- An empty listing still paints the pane background to the bottom.
- The app sidebar stays but becomes **collapsible: `Ctrl+B` + a chevron**,
  state persisted. Collapsed = hidden (panes take the width). This was an
  explicit user request.

## 1.2 One visual world

Panes are currently dark inside light chrome — half web-app, half terminal.
Per THEME, not per element: in dark theme the pane background, pane headers,
path row, column headers, status lines, command line and F-key bar all share
the dark palette (headers/status one step lighter than rows, TC-style).
Light theme: same, inverted. The theme toggle keeps working; Files is
monochrome-coherent in both.

## 1.3 Pane header: minimal chrome, TC-faithful

The user's `[Layout]` says `ButtonBar=0, DriveBar1=0, DriveCombo=1,
InterfaceFlat=1` — he runs TC with NO button bar and NO drive bar, just a
drive combo. Default accordingly:

- Per-pane header = `[source ▾ combo] [path] [filter box]` in one row.
  The combo lists real, present drives AND the ART sources (Folder…, ADF…,
  HDF…, Disc…, Archive…, C64…). No hardcoded Y:\/Z:\ — enumerate mounts.
- The current full button row becomes an optional Settings toggle
  ("Show drive/source buttons"), default OFF.
- Flat style throughout: no bevels, quiet 1px borders.

## 1.4 Bottom dock

- Command line row directly ABOVE the F-key bar, full width, always visible:
  `[path prompt] [input]`. It navigates and filters; it does NOT execute
  shell commands (§56 — deliberate, record as out of scope).
- F-key bar: ONE row, full width, docked to the window bottom, equal flat
  buttons. `F3 View · F4 Edit · F5 Copy · F6 Move · F7 NewFolder ·
  F8 Delete · F9 Attributes`. Never wraps — shrink padding, and at very
  narrow widths drop to keycap-only labels.
- **F6 is Move, not Rename** (TC semantics; Shift-F6 = rename-in-place).
  Update keymap, labels, i18n keys.
- The separate "1 item selected (880 KB)" line merges into the pane status
  bar (TC format: `880 k / 24,640 k in 1 / 28 file(s)`), which already
  half-exists.
- The permanent collision-policy footer ("When a name is already taken…")
  moves into the copy/confirm dialog where TC asks it; default from
  Settings. The footer disappears.
- The red banner ("Both panes are local folders — use Explorer for that.")
  becomes a one-line status-bar hint, not an alert bar pushing panes down.

## 1.5 Rows, columns, typography

- Remove the per-row `→`/`←` button column entirely — TC puts nothing on
  rows; copying is F5/drag/the two centre buttons (those may stay).
  The freed width goes to Name.
- Row density tightens toward TC: ≈1.5em line height, minimal vertical
  padding. Narrow UI font (system-ui ~13px), NOT monospace, with
  `font-variant-numeric: tabular-nums` on Size/Date so columns stop
  shimmering.
- Column defaults: Name flexible, Ext 4ch (own column, left-aligned —
  `Aligned extension=1` ✓ already right), Size 12ch right-aligned,
  Date 16ch with 4-digit year (`ShowCentury=1` ✓), Attr 6ch.
- Dirs sort by name among themselves whatever the file sort column is
  (`SortDirsByName=1`) — keep under every sort mode.

---

# PART 2 — COLOURS: the "tolon" theme

Decoded from the user's `[Colors]` (COLORREF = BGR). This CORRECTS the
earlier "thin outline cursor" idea — his TC uses `InverseCursor=1`, i.e. a
solid bar, and the current build's yellow bar was actually right:

| Element | wincmd.ini | CSS |
|---|---|---|
| Pane background | 2367260 | `#1C1F24` |
| Text | 16777215 | `#FFFFFF` |
| **Cursor bar** | 65535 + CursorText 0 | **solid `#FFFF00`, black text** |
| **Selected names** | 3947775 | **red `#FF3C3C`** |
| Cursor + selected | | yellow bar, red text |

- Make these themable tokens (`--tc-bg`, `--tc-cursor`, `--tc-cursor-text`,
  `--tc-selected`) with the above as the default dark ("tolon") theme; the
  light theme derives sanely (dark cursor bar, red marks).
- The focused PANE shows via its path row (active: accent; inactive:
  dimmed) — cursor bar only ever visible in the focused pane.
- **Per-filetype colour rules** (his TC runs 18 ColorFilters): ship a
  Settings list of `pattern → colour` rules, TC-shaped, with ART defaults —
  containers (adf/adz/dms/hdf/iso/d64/d71/d81/t64) one colour, archives
  (lha/zip/7z/lzx) another, ROMs a third, everything else default.

---

# PART 3 — BEHAVIOUR

## 3.1 Enter opens the container, in the same pane (the headline)

His TC maps `adf/dms/hdf/adz/iso/7z/…` to packer plugins — for 20 years
Enter on an ADF has meant "step inside it". ART has every reader this needs
(ADF, HDF partitions, ISO, LHA, ZIP, 7z; D64/T64 when Task 5 lands). Wire
the semantics:

- **Enter / double-click on a recognised container** → the pane navigates
  INTO it (pane kind switches); the path shows a breadcrumb like
  `E:\amiga\Games\Lotus.adf\`.
- First row `[..]`; Enter on it at container root exits to the host folder
  with the cursor restored onto the container file. **Backspace** = up one
  level (dir or container boundary alike). **Ctrl+PgUp** = up,
  **Ctrl+PgDn** = force-enter even if an association exists (TC keys).
- Multi-partition HDF: Enter lists partitions (DH0:, DH1:…) as a level;
  Enter again enters one.
- Unsupported interior (PFS3, TAP…): entering shows the honest label view
  ("DH1: 2.0 GB, PFS3 — not supported yet"), never an error toast.
- Per-pane history treats container levels as steps (Alt+Left/Right).
- Inside Files, this replaces "open in Studio" as the container default;
  Studios stay reachable via context menu ("Open in ADF Studio"). The
  commander is for walking; studios are for surgery.

## 3.2 Full keyboard coverage ("klavye örtüsü uyumu")

His config: `AltSearch=1`, custom shortcuts, `MarkDirectories=1`,
`CountSpace=1` — he drives TC mouse-free. Every mouse action gets a key:

| Key | Action |
|---|---|
| Tab | switch pane focus |
| Enter | open (dir/container: enter; file: default action) |
| Backspace / Ctrl+PgUp | up one level |
| Ctrl+PgDn | force-enter container |
| Insert | toggle mark + advance (exists) |
| **Space** | toggle mark; on a DIRECTORY also compute and show its size in the Size column (`CountSpace=1`) |
| Ctrl+A | mark all incl. dirs (`MarkDirectories=1`) |
| Num + / Num − | mark / unmark by wildcard dialog |
| Num * | invert marks |
| **Typing letters/digits** | quick search: cursor jumps to next name starting with the typed prefix (`AltSearch=1` letters-only); Esc clears |
| F2 or Ctrl+R | refresh |
| F3…F9 | as §1.4 (F6=Move, Shift-F6=rename) |
| Alt+F1 / Alt+F2 | left / right source-combo dropdown |
| Alt+Left / Alt+Right | pane history back / forward |
| Ctrl+B | toggle sidebar |
| Ctrl+T / Ctrl+W / Ctrl+Tab | tab: new / close / next |

## 3.3 Tabs per pane

His `DirTabOptions=824, DirTabLimit=32` and histories full of tab markers:
tabs are how he works. Required, not deferred:

- Tab bar per pane above the path row. Ctrl+T duplicates current into a new
  tab, Ctrl+W closes, Ctrl+Tab cycles, middle-click closes.
- A tab may live inside a container (title shows `Lotus.adf`).
- **Session restore**: tabs, per-tab paths, per-pane sort order and filter
  survive restart (`Savepath=1`, `Savepanels=1`, per-pane `sortorder` —
  two decades of his muscle memory expect the app to reopen where he left
  it).

## 3.4 Small behaviours his config expects

- `[Confirmation]` all enabled: deleting non-empty dirs, overwrite,
  overwrite read-only each confirm — map onto ART's Safety classes; never
  drop a confirmation his config keeps on.
- Dropdown histories on: path row, command line, New Folder dialog (his
  MkDirHistory repeats `multiboot`, `Amigatolon`, `kickstart`…).
- `UseRightButton=1` (NC right-click marking): offer as a Settings toggle,
  default OFF (right-click = context menu by default).

---

# PART 4 — OUT OF SCOPE (recorded so it isn't relitigated)

- Lister/content/filesystem plugin zoos (WLX/WDX/WFX): ART's F3 covers
  text/hex; format-aware preview is future work.
- Executing shell commands from the command line (`cm_ExecuteDOS`): not
  added, §56.
- RAR: stays out (licence decision, already recorded).

---

# PART 5 — ACCEPTANCE (the next screenshot is judged against these)

1. Window resized → panes always equal, always filling; no dead field
   below or beside them.
2. Dark theme → zero light chrome inside Files; light theme → zero dark
   blocks.
3. Cursor bar solid yellow with black text; selected names red; cursor
   only in the focused pane; a screenshot beside his TC at the same folder
   reads as siblings.
4. No buttons inside rows.
5. One F-key row docked at the bottom, command line directly above, F6
   says Move (tr: Taşı).
6. No permanent collision footer, no intro paragraph, no alert banner.
7. Default pane header is combo+path+filter; the button bar only appears
   via Settings.
8. Enter on `Workbench.adf` lists its contents in the same pane;
   Backspace returns; breadcrumb shows the container step; Enter on a
   multi-partition HDF shows DH0:/DH1:.
9. A full session — navigate, enter an ADF, mark five files with
   Insert/Space, F5-copy to the other pane, exit — completes with zero
   mouse touches.
10. Ctrl+B hides the sidebar; panes widen.
11. Two tabs on the right pane survive an app restart with their paths and
    sort orders.
12. `pnpm test` green (en/tr key parity incl. every new label);
    `cargo test` untouched or green.

# END OF BRIEF
