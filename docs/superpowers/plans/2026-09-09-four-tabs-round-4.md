# Four tabs, round 4 — tab 4, the run, and the end of `OsInstall.tsx` — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Tab 4 (`derle`) holds four summary lines, one Build button, a sequenced run (tree → ticked updates in chain order → first boot) that stops at the first ending that is not success and reports every phase in its own words, and the hand-off to the card lane; tab 1 keeps only the material; the keymap joins tab 3; `OsInstall.tsx` and `FirstBootPanel`'s wizard mount are deleted; the owner gets a package.

**Architecture:** One hook, `useBuildRun`, owns the sequence: phases are data (`Phase[]`), each run through the existing typed wrappers (`osinstallApply` job, `osinstallAddPackage` typed-refusal-or-job, `firstbootWrite` plain call) with `awaitJobResult` for the jobs, a cancel checked **between** phases (and `jobCancel` inside one), and a per-phase `PhaseReport` with four endings plus *not attempted*. `BuildTab.tsx` renders `useInstallPlan`'s totals, the plan's refusals in the button's place, the button, the run's report and the hand-off. `FilesTab.tsx` is `OsInstall.tsx` minus everything that left (the material card, the fold, the scans and the identity pass), with `AmigaInstallPanel` at its foot until round 5. `MachineTab` gains the keymap select from `useInstallPlan`'s `effectivePlan`. `BuildBar` gains the size line and loses its button on `derle` (tab 4 has the real one).

**Tech Stack:** React 18, react-i18next, Vitest + jsdom + Testing Library (`renderHook`), Tauri events through `@/lib/jobs`.

**Spec:** `docs/superpowers/specs/2026-09-09-os-builder-four-tabs-design.md` § 3.3 (keymap), § 3.4, § 4 (`FirstBootPanel`), § 7, § 8 row 4, § 9.2-9.3, § 10.3. Inventory: `.superpowers/sdd/2026-09-09-four-tabs/inventory-round-4.md` (local-only; line numbers there are from before round-3 Task 3 — find by content).

**Rulings against the spec, made while planning on 2026-09-09:**

- **Update mode is: the tree phase is absent.** `osinstall_apply` refuses a destination with anything in it (`refuse_unless_free`), and `osinstall_add_package` reads any tree fresh. So when `useDestinationCheck(destination).tree?.isTree` is true, the sequence has no tree phase, the summary's first line says *"Existing tree: {{release}}, {{count}} files — will be updated"*, and Run is enabled when at least one update row is ticked and runnable or first boot is wanted. When the destination is not a tree and `taken`, the plan's blocker stands and there is no button. No engine change.
- **The per-update preview (spec § 3.2's other half, ART-289's guard) is on tab 4, before Run**: one `osinstallCollisions` per ticked runnable update row, sequentially, with the slot's file as the override — the *"Yerine geçecek"* line merges the component preview's counts (tab 2's fold) and these. ART-289 closes here with the guard `asks the preview with the readout's file for each ticked update`.
- **`InstallProgress`'s indeterminate 25 % sliver goes** (CLAUDE.md: a fixed width when the total is unknown is the failure that does not crash). A phase with no total shows *N so far* from `progress.done`, or the phase name alone.
- **Cancel semantics:** *Stop* asks the current job to cancel (`jobCancel`) and marks every later phase *not attempted*; a phase's own `cancelled` ending is the job's; `firstbootWrite` is a plain call and cannot be cancelled mid-way (it is one `atomic_write` set) — Stop pressed during it lands after it.
- **`FirstBootPanel`** is unmounted from the wizard; the file stays for round 5 (rehearse → WinUAE studio). `session.firstboot.written` is set by the first-boot phase, as the panel did.
- **The result card's five `statedRelease` verdicts, the removed/icons lists and the ART-197 `setTree` hand-off move to the tree phase's report** unchanged in meaning.
- **A package goes to the owner at the end of this round** (the owner's instruction: after round 4 only).

## Global Constraints

- Nothing changes unless the user changes it: no new remembered key; the run starts only from tab 4's button; `setTree`/`setFirstBoot` are written by the run's own endings, as today's screen wrote them.
- Endings stay distinct: per phase **succeeded · refused · failed · cancelled · not attempted**, each its own key in both catalogues, never collapsed; the tree phase's `statedRelease` five verdicts kept.
- Never claim what you did not do: a failed run's report says *"the tree is at {{root}}; {{n}} files were written before this"* from the job's own `files_landed`; a refused phase names the refusal (`refusalPhrase`); a not-attempted phase says so.
- The screen may not out-claim the core: every sentence is a `Phrase` from `src/lib` mappers over the core's own answers.
- Every `it(...)` in `OsInstall.test.tsx` and `FirstBootPanel.test.tsx` (the wizard-facing cases) gets a by-name fate in the round report.
- No Rust change. Both catalogues; parity; literal-keys; dead-keys. Branch `art-four-tabs`; commits via a scratchpad file; mutations by `shutil.copyfile`.
- Scratchpad: `C:\Users\ismoz\AppData\Local\Temp\claude\D--Projeler-Amiga\06212805-e1dd-4b6f-b2ae-dd6f17219be6\scratchpad`.

---

## File structure

**Create**
- `src/lib/buildRun.ts` (+ `.test.ts`) — pure: `Phase`, `PhaseReport`, `PhaseEnding`, `sequenceFor(...)`, `phaseOutcomePhrase`, `phaseNextStepPhrase`, `phaseTone`, `runSummaryLines(...)`.
- `src/lib/useBuildRun.ts` (+ `.test.tsx`) — the sequencing hook over the wrappers.
- `src/lib/useUpdatesPreview.ts` (+ `.test.tsx`) — sequential `osinstallCollisions` per update row → one aggregate.
- `src/components/osbuilder/BuildTab.tsx` (+ `.test.tsx`).
- `src/components/osbuilder/FilesTab.tsx` (+ `.test.tsx`, cut from `OsInstall.test.tsx`).

**Modify**
- `src/components/osbuilder/MachineTab.tsx` (+ test) — the keymap select.
- `src/components/osbuilder/ChoiceTab.tsx` — exports `useTickedUpdates()` (the ticked, runnable rows with their slot files) so tab 4 and tab 2 agree on one list.
- `src/pages/osbuilder/BuildBar.tsx` (+ test) — the size line; no button on `derle`.
- `src/pages/osbuilder/steps.tsx` — `StepDosyalar` → `FilesTab`, `StepDerle` → `BuildTab`; `NotYet` deleted.
- `src/components/osbuilder/InstallProgress` → moves into `BuildTab.tsx` without the sliver.
- **Delete** `src/components/osbuilder/OsInstall.tsx`, `OsInstall.test.tsx`.
- Catalogues: `osBuilder.build.*`; retired `osinstall.run.*`/`osinstall.result.*` keys only where the grep finds no renderer (the phase report reuses most of them).
- Docs (Task 6).

---

### Task 1: `buildRun.ts` — phases as data, endings as phrases

**Files:** create `src/lib/buildRun.ts`, `src/lib/buildRun.test.ts`.

**Interfaces — produces:**

```ts
export type PhaseKind = "tree" | "package" | "firstboot";
export interface Phase {
  id: string;                    // "tree" | `package:${packageId}` | "firstboot"
  kind: PhaseKind;
  name: string;                  // the row's name (chain) or the release
  packageId?: string; slotId?: string; file?: string; folder?: string;
}
export type PhaseEnding =
  | { state: "pending" } | { state: "running"; progress: JobProgress | null }
  | { state: "succeeded"; outcome: ApplyOutcome | FirstBootWritten; statedRelease?: StatedRelease; elapsedMs: number }
  | { state: "refused"; refusals: RefusalReason[] }
  | { state: "failed"; message: string; errorCode: string | null; filesLanded: number | null }
  | { state: "cancelled"; filesLanded: number }
  | { state: "not-attempted" };
export interface PhaseReport { phase: Phase; ending: PhaseEnding }
export interface SequenceInputs {
  destination: string; destinationIsTree: boolean; release: InstallRelease;
  updates: { packageId: string; slotId: string; name: string; file: string; }[];   // ticked, runnable, host-placeable, chain order
  firstBootWanted: boolean;
}
export function sequenceFor(inputs: SequenceInputs): Phase[];             // tree only when !destinationIsTree; packages; firstboot when wanted
export function folderOf(path: string): string;                          // moved from AmigaInstallPanel.tsx:443-446
export function phaseOutcomePhrase(report: PhaseReport): Phrase;         // one key per (kind, state), counts from the outcome
export function phaseNextStepPhrase(report: PhaseReport): Phrase | null; // refused → the refusal; failed → "the tree is at …, N files were written"; cancelled → likewise; else null
export function phaseTone(report: PhaseReport): "ok" | "warn" | "err" | "muted";
export function runSummaryLines(args: { plan: InstallPlan | null; destinationIsTree: boolean; treeSummary: TreeSummary | null; updates: SequenceInputs["updates"]; firstBootWanted: boolean; replaces: { fresh: number; unchanged: number; replaced: number } | null }): Phrase[];  // the four lines
```

- [ ] Tests (`buildRun.test.ts`): `sequenceFor` — fresh destination → tree first; existing tree → no tree phase; updates in the given order; firstboot last only when wanted. `phaseOutcomePhrase`/`phaseNextStepPhrase`/`phaseTone` — one case per (kind × state) that exists, every key resolving in both catalogues (import the JSON as `chain.test.ts` does), `failed` naming `filesLanded`, `cancelled` naming it, `not-attempted` its own key. `runSummaryLines` — four lines for a fresh build; the existing-tree first line; zero updates → the second line says none; `replaces` null → the fourth line says *not previewed yet*.
- [ ] Implement; keys `osBuilder.build.phase.{tree,package,firstboot}.{succeeded,refused,failed,cancelled,notAttempted}`, `osBuilder.build.phase.running`, `osBuilder.build.next.{refused,failed,cancelled}`, `osBuilder.build.summary.{tree,treeExisting,updates_one,updates_other,updatesNone,firstboot,firstbootOff,replaces,replacesPending}` in both catalogues (English and Turkish written by hand). Commit.

---

### Task 2: `useBuildRun` — the sequence

**Files:** create `src/lib/useBuildRun.ts`, `src/lib/useBuildRun.test.tsx`.

**Interfaces — produces:**

```ts
export interface BuildRun {
  reports: PhaseReport[];            // one per phase of the last/current run, in order
  running: boolean;
  finished: boolean;                 // every phase reached a terminal ending
  succeeded: boolean;                // every phase succeeded
  start(phases: Phase[], plan: InstallPlan | null): void;
  stop(): void;                      // cancels the current job; later phases become not-attempted
}
export function useBuildRun(args: { destination: string | null; onTreeWritten(root: string): void; onFirstBootWritten(): void }): BuildRun;
```

Behaviour: `start` copies the phases into `reports` all `pending`, then runs them one at a time: `tree` → `awaitJobResult(OSINSTALL_RESULT_EVENT, () => osinstallApply(plan, destination))` with progress into `running`; on success `onTreeWritten(destination)` and the ending carries `outcome` + `stated_release`; `package` → `osinstallAddPackage(destination, phase.folder, [phase.packageId], [[phase.slotId, phase.file]])`: `refused` → the ending `refused` **and the sequence stops**; `started` → await `osinstall-add-package-result` by job id; `firstboot` → `firstbootWrite(destination)` then `onFirstBootWritten()`. Any `failed`/`cancelled` (from `awaitJobResult`'s rejection, `isJobCancellation`) stops the sequence; the remaining phases become `not-attempted`. `stop()` sets a ref and calls `jobCancel(currentJobId)` if one is running. A re-`start` replaces the reports. Elapsed time per phase from `performance.now()`.

- [ ] Tests with mocked `@/lib/osinstall`, `@/lib/firstboot`, `@/lib/jobs` (`awaitJobResult` mocked to resolve/reject per call; `jobCancel` a spy): runs three phases in order and reports succeeded ×3 with `onTreeWritten`/`onFirstBootWritten` called once; **stops at the first non-success** (tree ok, package refused → the third phase `not-attempted`, `firstbootWrite` never called); a failed job carries `filesLanded`; `stop()` during phase 2 → `jobCancel` called with that id, phase 2 `cancelled`, phase 3 `not-attempted`; an existing-tree sequence never calls `osinstallApply`; a refusal is a typed ending, not an error; re-start clears the old report.
- [ ] Implement; commit.

---

### Task 3: `useUpdatesPreview` and `useTickedUpdates` (ART-289)

**Files:** create `src/lib/useUpdatesPreview.ts` (+ test); modify `src/components/osbuilder/ChoiceTab.tsx` to export `useTickedUpdates(): { rows: SequenceInputs["updates"]; unresolved: { packageId: string; name: string }[]; loading: boolean }` — the chain rows that are ticked (`session.packages.chosen`) **and** `choiceRowState(...).enabled` **and** not `runsOnAmiga`, each resolved through `osinstallSlots` (same folders/tree/rom/overrides the chain uses) to `foundPath` (trusting `hash`/`volume-name` only, as `AmigaInstallPanel.tsx:865-871`); a ticked row whose slot has no trusted path is `unresolved` (tab 4 names it and does not run it).

`useUpdatesPreview({ destination, destinationIsTree, updates }) → { previews: { packageId: string; collisions: CollisionPreview | null; error: string | null }[]; totals: { fresh; unchanged; replaced } | null; loading: boolean }` — sequential `osinstallCollisions(destination, folderOf(file), [packageId], [[slotId, file]])` per row, only when `destinationIsTree` (a fresh destination has nothing to collide with); cancellation on input change (ART-089's mechanism); primitive dependency keys (ART-178).

- [ ] Tests: `useTickedUpdates` — a ticked row with a hash-matched slot resolves to its path; a ticked row matched by filename only is `unresolved`; an untick removes it; an Amiga-route row is never in `rows`. `useUpdatesPreview` — calls per row in order **with the overrides** (ART-289's guard, named `asks the preview with the readout's file for each ticked update`); a fresh destination asks nothing; a changed list drops the superseded answers; totals add up.
- [ ] Implement; commit. ISSUES: ART-289 → Fixed with that guard (Task 6 writes it).

---

### Task 4: `BuildTab`, the keymap on tab 3, the bar's size line

**Files:** create `src/components/osbuilder/BuildTab.tsx` (+ test); modify `MachineTab.tsx` (+ test), `BuildBar.tsx` (+ test), `steps.tsx`, `steps.test.tsx`, both catalogues.

`BuildTab` (`data-testid="build-tab"`): `useInstallPlan` with the same inputs as tabs 1-2 (keymap/reuseScan/destination through the same keys; `rescanNonce` 0); `useDestinationCheck(destination)`; `useTickedUpdates()`; `useUpdatesPreview(...)`; `useBuildRun({ destination, onTreeWritten: (root) => setTree({ root, builtHere: true }), onFirstBootWritten: () => setFirstBoot({ written: true }) })`. Renders, in order:
1. the four summary lines (`runSummaryLines`, testids `build-summary-line`), plus `build-unresolved` lines for ticked rows with no trusted file (*"{{name}}: choose its file on the Amiga files tab — it will not be written"*);
2. **either** the plan's refusals (moved from tab 1: `mediaEvidenceLine`, `refusalPhrase` list, `wrongFolder` → the switch-release button) and the blocker sentence, with **no** button — **or** the confirm checkbox (`osinstall.run.confirm`) and the button `build-run` (`osBuilder.build.run`), disabled while `!confirmed || running`; in update mode the blocker's `destinationTaken` is not a blocker (ruling) — compute `blocker` with `destinationTaken: destinationIsTree ? false : taken`;
3. while running: the current phase's name and progress (`fraction` → percent + done/total; no total → `osBuilder.build.progress.soFar {{done}}`; nothing else), and a *Stop* button (`build-stop`);
4. the report: one `build-phase-row` per phase with `phaseTone`, `phaseOutcomePhrase`, `phaseNextStepPhrase`; the tree phase's row also renders the five `statedRelease` verdict sentences, the removed and icons lists (moved from the result card, same keys);
5. when `succeeded`: the hand-off `build-handoff` — `osBuilder.build.handoff {{root}}` and a link that does `setKind("boot-card")` then navigates to `stepPath("kart")`.

`MachineTab` gains the keymap select (`keymap-section`, moved verbatim from `OsInstall.tsx`) fed by `useInstallPlan(...)`'s `effectivePlan` through `keymapsIn` (the "held across re-plans" rule at the old `:1150-1153` kept); `StepMakine`'s keymap note and its key deleted. `BuildBar`: the size line from `runSummaryLines`' first two phrases joined, the button absent on `derle` (already), `osBuilder.bar.goToBuild` unchanged.

- [ ] Tests (`BuildTab.test.tsx`, mocks as `ChoiceTab.test.tsx` + `@/lib/jobs`): the four lines from a planned fixture; **a refusing plan renders the refusal and no `build-run`** (spec § 7); confirm gates the button; pressing Run calls `start` with the sequence in order (tree, the ticked updates, firstboot) — assert the mocked `osinstallApply` is called before any `osinstallAddPackage`; the update-mode sequence has no tree phase and Run is enabled with one ticked update; the report rows carry the endings and *not attempted* after a refusal; the hand-off link sets `session.kind` to `boot-card` (assert the store) and targets `/os-builder/kart`; no fixed-width progress with no total; Turkish renders no raw key. `MachineTab.test.tsx`: the five keymap cases moved by name from `OsInstall.test.tsx`. `BuildBar.test.tsx`: the size line.
- [ ] Implement; `StepDerle` → `<BuildTab/>`; catalogues (`osBuilder.build.{run,stop,confirm?,progress.soFar,handoff,unresolved}`); commit.

---

### Task 5: `FilesTab` — tab 1 is the material; `OsInstall.tsx` deleted

**Files:** create `src/components/osbuilder/FilesTab.tsx` (+ `FilesTab.test.tsx`); delete `OsInstall.tsx`, `OsInstall.test.tsx`; modify `steps.tsx` (`StepDosyalar` → `FilesTab` with the dropped-disc prop), `steps.test.tsx` (mock `FilesTab` as `install`… rename the testid to `files-tab` and the completeness map), `FirstBootPanel` unmounted anywhere in `src/pages` (it already is after round 3; confirm), both catalogues.

`FilesTab` = the material card (release select, `source-columns` with `MaterialReadout` + `MaterialFolders`, the ART-256 lines, the fold with the identity pass and the scans) **and the state that feeds it**: `materialFolders`, `folderScans`, `folderIdentified`/`wrongLayerHintFor`, `layers`/`layerLabel` (from `useInstallPlan` — the tab still needs `plannedFolders.unusedForPlan` and `layers`), `foundVolumeNames`, `mediaIdentity` + the identify effect + `identifiedPass`, `materialOverrides` (slot overrides), `amigaForeverAdf` offer, `reuseScan`/`rescanMedia`/`rescanNonce`, the dropped-disc effect, `packagesTreeRoot` for the readout's `installed` badge. Everything else in `OsInstall.tsx` (plan error badge, refusals, plan card, keymap, run card, result card) has a new home in Tasks 4-5 or is deleted. `AmigaInstallPanel` mounts at the foot of `FilesTab` until round 5 with the same props.

- [ ] `FilesTab.test.tsx` = every `OsInstall.test.tsx` case about the material (the readout-first cases, the fold cases, the Amiga Forever disks offer, ART-241 folder rows, ART-256, the identity-by-hash describe, the layered-release cases, the dropped disc), moved by name; the plan/refusal/keymap/result/run cases moved to `BuildTab.test.tsx`/`MachineTab.test.tsx` in Task 4 or dropped with a reason. The round report lists all of them.
- [ ] Implement by **moving** `OsInstall.tsx` to `FilesTab.tsx` (`git mv`) and deleting the sections and state that left — the diff then shows what was removed, not a rewrite. Commit.

---

### Task 6: Mutations, docs, the package

| # | File | Mutation | Guard |
|---|---|---|---|
| M1 | `useBuildRun.ts` | continue past a refusal | `stops at the first ending that is not success` |
| M2 | `useBuildRun.ts` | `failed` and `refused` → one ending | `a refusal is a typed ending, not an error` |
| M3 | `useBuildRun.ts` | check the stop flag only after starting the next job | `stop during phase 2…` |
| M4 | `buildRun.ts` `sequenceFor` | tree phase even for an existing tree | `existing tree → no tree phase` |
| M5 | `useUpdatesPreview.ts` | drop the overrides | ART-289's guard |
| M6 | `BuildTab.tsx` | render the button beside a refusing plan | `a refusing plan renders the refusal and no build-run` |
| M7 | `BuildTab.tsx` | a 25 % sliver when no total | `no fixed-width progress with no total` |
| M8 | `BuildTab.tsx` | navigate without `setKind` | `the hand-off sets the kind` |
| M9 | `useTickedUpdates` | trust `matchedBy: "filename"` | `a ticked row matched by filename only is unresolved` |

Verification unpiped (lint, test, parity, literal-keys, dead-keys, control-byte, contrast, `git diff --stat main -- src-tauri` empty). The by-name ledgers for `OsInstall.test.tsx` and `FirstBootPanel.test.tsx`'s wizard cases. Docs: STATUS item 0 in place (rounds 1-4 landed; the package; what a person drives — spec § 3.6 and § 7: from a clean destination tick BB1, BB2, Locale update, Türkçe; one press; five phase lines; boot and `version full`); session-log; FEATURES (the OS install screen row rewritten for the four tabs, guards named; the chain-as-one-screen row → tab 2/4; first boot row → the tick and the phase); CHANGELOG; ISSUES ART-289 → Fixed. **Then `pnpm tauri build`** (unpiped) and copy the NSIS installer to `E:\amiga\ProjeART\build\Amiga Retro Toolkit_0.9.1_x64-setup_four-tabs-r4.exe`; the round report names it.

---

## Self-review

**Spec coverage:** § 3.4's four lines, one button, the sequence with its five rules, the hand-off, `session.tree.root` after apply — Tasks 1, 2, 4; § 3.3's keymap — Task 4; § 3.2's per-update preview and ART-289 — Task 3; § 4's `OsInstall`/`FirstBootPanel` — Task 5; § 7's guards for the run — Tasks 2, 4 tests; § 9.2's fallback (*Sıradakini koş*) — not built unless the owner's drive asks; § 10.3's phase collapse — the report replaces the plan block once a run exists (Task 4: the four lines stay, the plan's file list is behind `listeyi aç`).

**Placeholders:** the test lists name every required assertion and the fixture sources; the executors write the bodies; each reviewer checks the named assertions exist.

**Type consistency:** `Phase`/`PhaseReport`/`PhaseEnding`/`SequenceInputs` defined in Task 1 and consumed by Tasks 2-4; `useTickedUpdates` (Task 3) feeds `sequenceFor` and `useUpdatesPreview`; `useBuildRun`'s `start(phases, plan)` called from `BuildTab` (Task 4); `folderOf` moves to `buildRun.ts` and `AmigaInstallPanel` imports it from there.
