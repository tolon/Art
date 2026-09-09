# Four tabs, round 5 — the WinUAE studio section, the last spec rows, the lock on the shell — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** The Amiga-side emulator run and the first-boot rehearsal leave the wizard for a section of the WinUAE studio; `packages.folder` is never written again; the run lock covers the whole shell; the deferred minors close; the owner gets the round-5 package.

**Architecture:** `AmigaInstallPanel` stops taking props and reads what it needs from the build session and the same `useChainTree(destination)` rule the four tabs use, so it says the same thing about the tree wherever it is mounted; `WinuaeStudio` mounts it as a full-width card after its launcher grid, with a `FirstBootRehearse` component cut out of `FirstBootPanel` beside it; `FirstBootPanel` is deleted. `useBuildSession` stops writing `packages.folder` (the panel adds an archive folder to the material list instead — spec § 3.4). `RunLockContext`'s provider moves to `Layout.tsx`; the sidebar and the dashboard's drop cards honour it.

**Tech Stack:** React 18, react-router-dom 7, react-i18next, Vitest + jsdom.

**Spec:** `docs/superpowers/specs/2026-09-09-os-builder-four-tabs-design.md` § 4, § 5 (`packages.folder`), § 8 row 5, § 9. Inventory: `.superpowers/sdd/2026-09-09-four-tabs/inventory-round-5.md`.

**Rulings, made while planning on 2026-09-10:**

- **The panel resolves its own inputs** (`session.release`, `session.material.folders`, `session.packages.folder`, `useChainTree(destination)` where `destination` is the tab-3 remembered key) — one rule for the tree everywhere; its Browse field still writes `session.tree.root` and hides when the destination wins.
- **`FirstBootPanel` is deleted**: the write is tab 4's phase, the preview's `alreadyWritten` is tab 2's hint; the rehearsal becomes `FirstBootRehearse({ treeRoot })` in the studio. Its write/preview cases are dropped with the reason; the rehearsal cases move by name.
- **The lock's provider lives in the shell**; the sidebar's links become disabled spans with the sentence; the dashboard's "open in the OS Builder" action is disabled with the same sentence while a run is going. Browser back/forward is not blocked (nothing in this app blocks it today).
- **A package goes to the owner at the end** (`…_four-tabs-r5.exe`), and the branch is then ready for the owner's decision on the merge.

## Global Constraints

- Nothing changes unless the user changes it: no new remembered key; `packages.folder` read once as a seed, never written; nothing written on render.
- Endings stay distinct: the panel's four endings and the rehearsal's four move untouched.
- Every `it(...)` that leaves `AmigaInstallPanel.test.tsx`, `FirstBootPanel.test.tsx` or `FilesTab.test.tsx` gets a by-name fate in the round report.
- Both catalogues; parity; literal-keys; dead-keys; no Rust; branch `art-four-tabs`; commits via a scratchpad file; mutations by `shutil.copyfile`.
- Scratchpad: `C:\Users\ismoz\AppData\Local\Temp\claude\D--Projeler-Amiga\06212805-e1dd-4b6f-b2ae-dd6f17219be6\scratchpad`.

---

### Task 1: The panel reads the session; the studio mounts it; `FirstBootRehearse`

**Files:** modify `src/components/osbuilder/AmigaInstallPanel.tsx` (+ test), `src/pages/WinuaeStudio.tsx`, `src/components/osbuilder/FilesTab.tsx` (+ test); create `src/components/osbuilder/FirstBootRehearse.tsx` (+ test, from `FirstBootPanel.test.tsx`'s rehearsal describes); delete `FirstBootPanel.tsx` + test; catalogues.

- `AmigaInstallPanelProps` goes: inside, `const { session, setRom, setTree } = useBuildSession()`; `release = session.release`; `materialFolders = session.material.folders.map(f => f.path)`; `packageFolder = session.packages.folder`; `destination` through `useRemembered(rememberedComponentKey("osinstall.destination", release), isTextOrNothing, null)`; `{ treeRoot, source } = useChainTree(destination)`; `treeFromDestination = source === "destination"`; `onTreeRootChange` → `setTree({ root, builtHere: false })`. The test's wrapper renders the panel with **seeds** instead of props (the same values); every case keeps its name and assertions — the report lists any assertion changed.
- `WinuaeStudio.tsx`: after the grid (line ~387), `<section className="card" data-testid="winuae-install-section">` with `<h2>{t("winuae.installHeading")}</h2>` (en "Run a package's own installer" · tr "Paket kurucusunu WinUAE'de koştur"), one intro line `winuae.installIntro` (en "For updates whose installer must run on the Amiga. The tree and the archives come from the OS Builder's choices." · tr "Kurucusu Amiga'da koşması gereken güncellemeler için. Ağaç ve arşivler İşletim Sistemi Kurucusu'ndaki seçimlerden gelir."), then `<AmigaInstallPanel/>` and `<FirstBootRehearse treeRoot={treeRoot}/>` (the studio computes `treeRoot` the same way — or the rehearse component resolves it itself like the panel; prefer self-resolving, one rule). A `WinuaeStudio.test.tsx` is created with the mocks the panel tests use: the section renders both components (mocked) — two cases.
- `FilesTab.tsx`: the `AmigaInstallPanel` mount and its comment go; `packagesFolder`/`packagesTreeSource` plumbing kept only if the readout still needs it (`packagesTreeRoot` — yes; `packagesFolder` — check).
- `FirstBootRehearse.tsx`: lines 89-103, 196-244, 366-448 of `FirstBootPanel.tsx` as a component with `treeRoot: string | null` (self-resolving `kickstart` from `session.rom.path` and `winuaePath` from the settings store, as today). Tests: the three rehearsal describes by name; the four write/preview describes dropped with the reason (write = tab 4's phase, guard `runs three phases…`; `alreadyWritten` = tab 2's `says the tree already carries a block when it does`).
- Tests green; parity; literal-keys; dead-keys (retired `firstboot.panel.*` write keys deleted where no renderer remains); lint; suite; commit.

### Task 2: `packages.folder` is never written again (spec § 5)

**Files:** `src/lib/useBuildSession.ts` (+ test), `src/components/osbuilder/AmigaInstallPanel.tsx` (+ test), `src/lib/buildSession.ts` (a `@deprecated` note on `PackageChoice.folder`).

- `setPackages` no longer accepts or writes `folder`: `setPackages(change: { chosen?: string[] })`; `setMaterial`'s clear-on-removal write goes; the derived read (`stored ?? derived`) stays, and the seed (`seedPackagesFolder`) stays read-once. `PackageChoice.folder` marked `@deprecated — read only; never written since round 5`.
- The panel's archive-folder Browse (whatever called `setPackages({ folder })`) now calls `addMaterialFolder(folder)` — the folder is in the material list, which is where the slots resolve from (spec § 3.4's rule).
- Guard: `useBuildSession.test.tsx` — `packages.folder is never written again`: after `setPackages({ chosen: [...] })`, `addMaterialFolder(...)`, and `setMaterial([...])` the stored `buildSession.packages.<release>` has no `folder` key (or the seeded value, unchanged); `AmigaInstallPanel.test.tsx` — the archive-folder Browse adds to `buildSession.material.<release>` and never to `packages.folder`.

### Task 3: The lock on the shell

**Files:** `src/pages/osbuilder/runLock.tsx`, `src/pages/OsBuilder.tsx`, `src/components/layout/Layout.tsx`, `src/components/layout/Sidebar.tsx` (+ test if one exists, else create `Sidebar.test.tsx`), `src/pages/Dashboard.tsx` (+ test), catalogues.

- The provider moves to `Layout.tsx` (around `<Sidebar/> + <TopBar/> + <Outlet/>`); `OsBuilder.tsx` consumes only. `Sidebar`: while `running`, every `NavLink` renders as `<span aria-disabled="true">` with the same look, and one line `osBuilder.build.navigationLocked` under the list. `Dashboard`: the drop-result card's "open" button is `disabled` with `title`/a line `osBuilder.build.navigationLocked` while running. Tests: `Sidebar.test.tsx` — links vs spans by the context; `Dashboard`'s existing test file (if any) or a new one — the button disabled under the lock; `steps.test.tsx`'s lock cases keep passing with the provider now outside `OsBuilder` (their `renderAt` wraps in the provider explicitly).

### Task 4: The deferred minors

**Files:** `src/i18n/*.json`, `src/components/osbuilder/BuildTab.tsx` (+ test), `docs/FEATURES.md`, new `src/components/osbuilder/HostPlacement.test.tsx`.

- `osinstall.blocked.noFolder` → en "Choose the media folder on the Amiga files tab first." · tr "Önce Amiga dosyaları sekmesinde ortam klasörünü seçin." (pin in `copy.test.ts`'s tab-name describe with `osBuilder.step.dosyalar`).
- `BuildTab`'s button label after `run.succeeded`: `osBuilder.build.runAgain` (en "Build again" · tr "Yeniden derle"); case.
- `docs/FEATURES.md:308`'s head: "in the WinUAE studio (`/winuae`, since round 5 of the four-tab rewrite), over …" — and the two "until round 5 moves it" clauses in that row become past tense.
- `HostPlacement.test.tsx`: `HostPlacementPreview` renders one `collision-row` per report entry grouped by class, and `host-placement-refusals` for a refusal — two cases against a fixture from `AmigaInstallPanel.test.tsx`'s collisions mocks.

### Task 5: Mutations, docs, the package

| # | Mutation | Guard |
|---|---|---|
| M1 | the panel reads `treeRoot` from `session.tree.root` alone | the panel's `the distribution tree field, when the destination is what won` cases |
| M2 | `setPackages` writes `folder` again | `packages.folder is never written again` |
| M3 | the sidebar ignores the lock | `Sidebar.test.tsx`'s lock case |
| M4 | the dashboard's open button ignores the lock | its case |
| M5 | `runAgain` label never shown | its case |
| M6 | `FirstBootRehearse` collapses two endings | the moved `four rehearsal endings` case |

Verification unpiped; the by-name fate lists (`AmigaInstallPanel.test.tsx` — none should leave; `FirstBootPanel.test.tsx` — all 21; `FilesTab.test.tsx` — the panel mount cases); the report `.superpowers/sdd/2026-09-09-four-tabs/round-5-report.md`; docs — STATUS item 0 (rounds 1-5 landed; the package; the merge is the owner's call; what a person drives — the studio section on a real tree, the lock from the sidebar, the dashboard drop during a run), session-log, FEATURES (the chain row, first boot row, the WinUAE studio row), CHANGELOG (Changed: the Amiga-side installer run moved to the WinUAE studio; Fixed: the archive folder no longer writes a second key). **Then `pnpm tauri build`** (unpiped) and copy the NSIS installer to `E:\amiga\ProjeART\build\Amiga Retro Toolkit_0.9.1_x64-setup_four-tabs-r5.exe`.

---

## Self-review

**Spec coverage:** § 4's four bullets — Tasks 1 (panel, rehearse, `FirstBootPanel`), 2 (`packages.folder`); § 5's table — Task 2 (`folder` read only); § 8 row 5 — Task 1; § 9's "does not trim `AmigaInstallPanel`'s chain block" — kept (the panel moves whole); the round-4 review's I2 sibling doors (sidebar, dashboard drop) — Task 3; the deferred minors — Task 4.

**Placeholders:** the line ranges for `FirstBootRehearse` come from the inventory and the executor cuts them; test names given for every guard.

**Type consistency:** `AmigaInstallPanel` takes no props after Task 1 (Task 2 edits its Browse handler; Task 5's M1 mutates its tree source); `FirstBootRehearse({ treeRoot })`; `RunLockContext` unchanged in shape, moved provider; `setPackages({ chosen })` after Task 2 — grep every caller (`ChoiceTab.tsx`) and adjust.
