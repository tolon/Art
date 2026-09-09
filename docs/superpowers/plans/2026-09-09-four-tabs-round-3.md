# Four tabs, round 3 — tab 2, the one list — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Tab 2 (`secim`) becomes one tick list — the release's parts, the AmigaOS 3.9 updates in the material's order, and one first-boot tick — with the component ticks living in the build session; `PackagePanel` is deleted.

**Architecture:** The plan computation (`layersFor` → `osinstallComponents` → `osinstallPlan` ×1-2 → `osinstallComponentCollisions`, with the sanitize/prune writes and the cancellation) leaves `OsInstall.tsx` for one hook, `useInstallPlan(inputs)` in `src/lib`, so tab 1 (which still shows the plan and runs it until round 4) and tab 2 (which shows the parts) compute it through one code path. The component ticks move from the panel-owned keys `osinstall.chosen.<release>` / `osinstall.excludedConditional.<release>` to `session.components` (ART-290 — the session was seeded from those keys and never written). `ChoiceTab.tsx` renders the three groups; the components JSX moves verbatim; the update rows come from `osinstallChain` through `chainLines`; the first-boot tick is a new session boolean `firstboot.wanted`.

**Tech Stack:** React 18, react-i18next, Vitest + jsdom + Testing Library (`renderHook`).

**Spec:** `docs/superpowers/specs/2026-09-09-os-builder-four-tabs-design.md` § 3.2, § 5, § 7, § 8 row 3. Inventory: `.superpowers/sdd/2026-09-09-four-tabs/inventory-round-3.md` (local-only; every line number below comes from it).

**Rulings against the spec, made while planning on 2026-09-09:**

- **The folded *what this would replace* line counts components only in this round.** § 3.2 merges `osinstall_component_collisions` and `osinstall_collisions` into one count; the package half is one job per ticked row (`useHostPlacement` takes one folder and one job), and sequencing N jobs is the machinery round 4 builds for the run. Round 4 previews each update phase before Run. So the guard *ART-289: the preview gets the user's file* (spec § 7) lands in round 4 with that preview, not here; ART-289 stays open until then.
- **`FirstBootPanel` leaves the wizard in this round** (spec § 4): tab 2 has the tick, and the *write* happens in round 4's run. Between rounds 3 and 4 nothing on the branch writes a first-boot block — a branch state, not a shipped one (no package goes out before round 4).
- **Refusals, the plan, the keymap, Run and the result stay on tab 1** until round 4; the components rows, the confirm-off dialog and the replaces fold move now.
- **`useInstallPlan` is per consumer.** Only one tab is mounted at a time (routes), so one plan is in flight at a time; switching tabs re-plans once. The spec's "one `osinstall_plan` call for two mounted consumers" is not the situation the routes produce.
- **A remembered package id that is not a chain row** (PackagePanel's N4 row) renders as an untickable row *not in this release's list* with an untick control, so a stale id can still be dropped.

## Global Constraints

- **Nothing changes unless the user changes it.** The component ticks migrate by `seededComponents` (one-time read of the legacy keys when `buildSession.components.<release>` is absent); the legacy keys are never written again and never deleted. `firstboot.wanted` absent = ticked; rendering never writes it. `packages.chosen` keeps its ids. No key renamed or deleted.
- **Endings stay distinct.** Every chain row sentence is `chainLines`' (`chain.rs`'s), eight kinds; the components' required / condition / coming-later sentences move verbatim.
- `data-testid`s that exist keep their names: `component-collision-row`; new ones named below.
- Both catalogues in one commit; parity and literal-keys tests; `src/lib` returns keys, never strings.
- No Rust change. Branch `art-four-tabs`; `git branch --show-current` before every commit; commit messages via a scratchpad file; mutations by `shutil.copyfile`.
- Every `it(...)` that leaves `OsInstall.test.tsx` or `PackagePanel.test.tsx` is listed by name in the round report with its fate (moved to / covered by / dropped because).
- **No installer is built or copied in this round.**
- Scratchpad: `C:\Users\ismoz\AppData\Local\Temp\claude\D--Projeler-Amiga\06212805-e1dd-4b6f-b2ae-dd6f17219be6\scratchpad`.

---

## File structure

**Create**
- `src/lib/useInstallPlan.ts` (+ `.test.tsx`) — the plan hook.
- `src/components/osbuilder/ChoiceTab.tsx` (+ `.test.tsx`) — tab 2: `ComponentsGroup`, `UpdatesGroup`, `FirstBootTick`, the replaces fold.

**Modify**
- `src/lib/buildSession.ts`, `src/lib/useBuildSession.ts` — `FirstBootChoice.wanted?: boolean` (absent = wanted), `FIRSTBOOT_SPEC`, `setFirstBoot` unchanged in shape.
- `src/components/osbuilder/OsInstall.tsx` — plan state → the hook; ticks → `session.components`; the components section, the confirm-off dialog, the replaces section and the `<PackagePanel>` mount removed.
- `src/components/osbuilder/OsInstall.test.tsx` — cases leave by name; seeds of `osinstall.chosen.<release>` still work through the migration (the test asserts that).
- `src/pages/osbuilder/steps.tsx` — `StepSecim` mounts `ChoiceTab`; `PackagePanel` and `FirstBootPanel` imports go.
- `src/pages/osbuilder/steps.test.tsx` — the `packages`/`firstboot` mock testids → `choice-tab`.
- **Delete** `src/components/osbuilder/PackagePanel.tsx`, `PackagePanel.test.tsx`.
- `src/i18n/en.json`, `src/i18n/tr.json` — `osBuilder.choice.*` keys; retired `osinstall.packages.*` keys only `PackagePanel` rendered (grep-decided).
- Docs (Task 5).

---

### Task 1: `useInstallPlan`, and the ticks move to the session (ART-290)

**Files:**
- Create: `src/lib/useInstallPlan.ts`, `src/lib/useInstallPlan.test.tsx`
- Modify: `src/components/osbuilder/OsInstall.tsx` (`:330-372` layers, `:640-662` the two remembered keys, `:740-744` catalogue state, `:824-851` plan/preview state, `:878-895` catalogue effect, `:1104-1246` the plan effect, `:1326-1360` `layeringOn` + collisions effect)
- Test: `src/components/osbuilder/OsInstall.test.tsx`

**Interfaces — produces:**

```ts
// src/lib/useInstallPlan.ts
export interface InstallPlanInputs {
  release: InstallRelease;
  material: MaterialChoice;                 // session.material
  keymap: string;                           // "" = the ROM's usa
  rom: string | null;
  destination: string | null;
  reuseScan: boolean;
  rescanNonce: number;
  components: ComponentChoice;              // session.components — { chosen, excludedConditional }
  setComponents: (change: Partial<ComponentChoice>) => void;
}
export interface InstallPlanState {
  layers: InstallLayer[];
  plannedFolders: ReturnType<typeof foldersForPlan>;
  layerFoldersKey: string;
  catalogue: ComponentDef[] | null;         // null until loaded for this release
  componentsError: boolean;
  basePlanResult: PlanResult | null;
  effectivePlanResult: PlanResult | null;
  basePlan: InstallPlan | null;
  effectivePlan: InstallPlan | null;
  planError: string | null;
  layeringOn: string[];
  componentPreview: ComponentPreview | null;
  componentPreviewError: string | null;
  /** Bumps on every plan answer (success or failure). A consumer that holds a
   *  confirmation about the plan on screen resets it on this. */
  planVersion: number;
}
export function useInstallPlan(inputs: InstallPlanInputs): InstallPlanState;
```

- [ ] **Step 1: The hook's tests** — `src/lib/useInstallPlan.test.tsx`, mocking `@/lib/osinstall`'s `layersFor`, `osinstallComponents`, `osinstallPlan`, `osinstallComponentCollisions` (hoisted `vi.fn()`s, `...importOriginal`), with a `material` of one folder and a catalogue of two components (`workbench-base` required, `extras` optional with `overrides: ["workbench-base"]`) and a `PlanResult` fixture `{ outcome: "planned", plan: { componentsOn: [...], items: [], refusals: [] } }` shaped after `src/lib/osinstall.ts:388-470` (read the types; fill every required field):

```tsx
describe("useInstallPlan", () => {
  it("plans nothing without a folder", () => { /* material.folders = [] → planMock not called, results null */ });
  it("plans once when nothing is excluded, twice when something is", async () => {
    /* excludedConditional: [] → planMock called once; rerender with ["extras"] → called twice more (base + effective) */
  });
  it("sanitizes a stale tick through the session setter, never by dropping it silently", async () => {
    /* components.chosen = ["gone"] with the catalogue lacking it → setComponents called with { chosen: [] } */
  });
  it("prunes a stale exclusion once the base plan has landed", async () => { /* pruneStaleExclusions path → setComponents({ excludedConditional: […] }) */ });
  it("does not write a tick set while the catalogue has not arrived (ART-089)", async () => {
    /* componentsMock never resolves; chosen = ["gone"] → setComponents never called */
  });
  it("drops a superseded plan", async () => { /* first plan pending, rerender with a new keymap, second resolves → state is the second; resolve the first late → unchanged */ });
  it("asks for component collisions only for layering components that are on", async () => { /* plan.componentsOn includes extras → collisionsMock called with (plan, ["extras"]); without → not called and preview null */ });
  it("bumps planVersion on every answer, success or failure", async () => { /* resolve → 1; rerender with a rejecting plan → 2 */ });
});
```

Write each body in full against the fixture; the assertions above are the required ones.

- [ ] **Step 2: Run, fail on the missing module.**

- [ ] **Step 3: The hook** — the bodies are the effects named in the Files list, moved verbatim with these substitutions: `chosen` → `components.chosen`, `excludedConditional` → `components.excludedConditional`, `setChosen(x)` → `setComponents({ chosen: x })`, `setExcludedConditional(x)` → `setComponents({ excludedConditional: x })`, `session.material` → `material`, `romPath` → `rom`; `setConfirmed(false); setPendingExclusion(null);` in the plan's `.then` become `setPlanVersion((v) => v + 1)` (also in the `.catch`); `errorText(t, e)` keeps working through the hook's own `useTranslation()`. `layers`/`layersRelease`, the catalogue state (`components` → rename to `catalogueState` inside the hook), `plannedFolders = useMemo(() => foldersForPlan(material, layers), …)`, `layerFoldersKey`, `layeringOn` all live in the hook. The dependency lists stay as they are, translated. Doc comment at the top: why one hook (two tabs, one plan), ART-290, and the ART-119/ART-178/ART-089 rules the effects carry — copy those paragraphs from `OsInstall.tsx` rather than paraphrasing.

- [ ] **Step 4: `OsInstall.tsx` on the hook** — replace the moved state and effects with:

```tsx
  const { session, setComponents, /* …existing… */ } = useBuildSession();
  const plan = useInstallPlan({
    release,
    material: session.material,
    keymap,
    rom: romPath,
    destination,
    reuseScan,
    rescanNonce,
    components: session.components,
    setComponents,
  });
  const { layers, plannedFolders, layerFoldersKey, catalogue, componentsError, basePlanResult, effectivePlanResult, basePlan, effectivePlan, planError, layeringOn, componentPreview, componentPreviewError } = plan;
  const chosen = session.components.chosen;
  const excludedConditional = session.components.excludedConditional;
  const setChosen = (next: string[]) => setComponents({ chosen: next });
  const setExcludedConditional = (next: string[]) => setComponents({ excludedConditional: next });
  useEffect(() => {
    setConfirmed(false);
    setPendingExclusion(null);
  }, [plan.planVersion]);
```

Delete the two `useRemembered` calls for `osinstall.chosen` / `osinstall.excludedConditional` and their comments; keep `rememberedComponentKey("osinstall.keymap"…)`, `…destination…`, `…reuseScan` as they are. Remove now-unused imports.

- [ ] **Step 5: The regression net** — `OsInstall.test.tsx` cases that seed `osinstall.chosen.<release>` keep passing through `seededComponents` (the session key is absent in a fresh store, so the seed reads the legacy key). Cases that **assert a write** to `osinstall.chosen…` / `osinstall.excludedConditional…` change to assert `buildSession.components.<release>` — each named in the report with the assertion changed. Add one case:

```tsx
  it("writes a tick to the session, never to the legacy key (ART-290)", async () => {
    await renderFull();
    await userEvent.click(screen.getByRole("checkbox", { name: "Extras3.2" }));
    await waitFor(() => expect(rememberedBag()["buildSession.components.AmigaOS 3.2"]).toMatchObject({ chosen: expect.arrayContaining(["extras"]) }));
    expect(rememberedBag()["osinstall.chosen.AmigaOS 3.2"]).toBeUndefined();
  });
```

(Match the component id and label to `FULL_FIELDS`' fixtures in that file — read them.)

```powershell
pnpm vitest run src/lib/useInstallPlan.test.tsx src/components/osbuilder/OsInstall.test.tsx
pnpm lint
pnpm test
```
Expected: hook 8 passed; `OsInstall.test.tsx` 104 (103 + 1) with every changed assertion named; lint clean; suite green.

- [ ] **Step 6: Commit** — *"useInstallPlan: the plan out of OsInstall.tsx; component ticks in the session (ART-290)"*.

---

### Task 2: `ChoiceTab` — the parts, and the replaces fold

**Files:**
- Create: `src/components/osbuilder/ChoiceTab.tsx`, `src/components/osbuilder/ChoiceTab.test.tsx`
- Modify: `src/components/osbuilder/OsInstall.tsx` (the components section `:1846-2000` and the replaces section `:2060-2151` removed; `toggleConditional`/`confirmExclusion`/`pendingExclusion` go with them; the refusals section stays), `src/components/osbuilder/OsInstall.test.tsx` (the cases *ticking a component changes what the screen will do* and *the screen says what a layering component would replace (ART-175)* move to `ChoiceTab.test.tsx` by name, adapted to render `<ChoiceTab/>` with the same seeds and mocks), `src/pages/osbuilder/steps.tsx` (`StepSecim` renders `<ChoiceTab/>` **above** `PackagePanel` and `FirstBootPanel` for this task only — Task 3 removes those two), `src/pages/osbuilder/steps.test.tsx` (mock `ChoiceTab` as `choice-tab`; the completeness test's `secim` expectation → `choice-tab`), both catalogues.

**Interfaces — produces:** `ChoiceTab()` reading `useBuildSession()` and `useInstallPlan` with the same inputs `OsInstall` passes (the keymap and reuseScan from the same remembered keys, `rescanNonce` **0** — tab 2 has no Scan-again; note it); testids `choice-tab`, `choice-parts`, `choice-part-row` (one per component, with `data-component={def.id}`), `choice-part-confirm-off`, `choice-replaces-fold`, `choice-replaces-summary`.

- [ ] **Step 1: Tests** — `ChoiceTab.test.tsx` with the mock block copied from `OsInstall.test.tsx` (settings, `@/lib/osinstall`'s `layersFor`, `osinstallComponents`, `osinstallPlan`, `osinstallComponentCollisions`, `osinstallChain` (resolve `{ rows: [], summary … }` for this task), `osinstallSlots`) and the `seedRemembered`/`rememberedBag` helpers:

```tsx
describe("the parts group", () => {
  it("renders one row per component of the release's catalogue, required rows ticked and disabled", async () => { /* rows count = catalogue length; required → checked + disabled + required badge text */ });
  it("ticks through the session, never the legacy key (ART-290)", async () => { /* click optional row → buildSession.components.<release>.chosen contains id; osinstall.chosen.<release> undefined */ });
  it("asks before turning off a component the condition switched on, and turns it off only on confirm", async () => { /* moved: the conditional-off dialog — choice-part-confirm-off appears; Cancel keeps it; Confirm writes excludedConditional */ });
  it("says what a layering component would replace, folded and counted (ART-175)", async () => {
    /* componentPreview with 2 reports → choice-replaces-fold closed; summary text = t("osBuilder.choice.replaces", { count }); component-collision-row ×2 inside */
  });
  it("draws no replaces fold when nothing would be replaced", async () => { /* preview null → no fold */ });
});
```

Full bodies against the fixtures; the two moved cases keep their original assertions.

- [ ] **Step 2: Red.** - [ ] **Step 3: The component** — `ChoiceTab` = `<section className="card" data-testid="choice-tab">` with `<h2>{t("osBuilder.step.secim")}</h2>`, the lead `osBuilder.choice.lead`, then `<ComponentsGroup>` (heading `osBuilder.choice.parts`; the row JSX from `OsInstall.tsx:1911-1996` verbatim with `data-testid="choice-part-row" data-component={def.id}` added on the outer div, the confirm-off dialog with `data-testid="choice-part-confirm-off"`), then the fold:

```tsx
      {componentPreview && componentPreview.reports.length > 0 && (
        <details data-testid="choice-replaces-fold" style={{ margin: "12px 0 0" }}>
          <summary data-testid="choice-replaces-summary" className="muted" style={{ fontSize: 12, cursor: "pointer" }}>
            {t("osBuilder.choice.replaces", { count: componentPreview.reports.length })}
          </summary>
          {/* the replaces section's groups from OsInstall.tsx:2091-2151, verbatim */}
        </details>
      )}
      {componentPreviewError && <p className="badge badge-err" …>{t("osinstall.replaces.error", { detail: componentPreviewError })}</p>}
```

(If the existing replaces section renders its error through a different key, keep that key.) Catalogue keys: `osBuilder.choice.lead` — en "One list, in the material's own order. Ticked rows are written; a row that cannot be written says why and stays closed." / tr "Tek liste, malzemenin kendi sırasıyla. İşaretli satırlar yazılır; yazılamayan satır nedenini söyler ve kapalı durur."; `osBuilder.choice.parts` — en "The release's parts" / tr "Sistemin parçaları"; `osBuilder.choice.replaces_one`/`_other` — en "Would replace {{count}} file already in the tree (open to see which)" / "…{{count}} files…" ; tr "Ağaçtaki {{count}} dosyanın yerine geçer (hangileri, açın)".

- [ ] **Step 4: `OsInstall.tsx` loses the two sections**; `StepSecim` mounts `<ChoiceTab/>` first; `steps.test.tsx` mocks it. - [ ] **Step 5: Green, parity, literal-keys, lint, suite; commit** — *"ChoiceTab: the release's parts and the replaces fold on tab 2"*.

---

### Task 3: The updates group, the first-boot tick, `PackagePanel` deleted

**Files:**
- Modify: `src/lib/buildSession.ts` (`FirstBootChoice.wanted?: boolean`, `FIRSTBOOT_SPEC.wanted = isFlag` — read how optional fields are guarded in `recallInto` and follow it), `src/lib/buildSession.test.ts` (one case: a stored `{ written: true }` recalls with `wanted` absent; a stored `{ wanted: false }` survives).
- Modify: `src/components/osbuilder/ChoiceTab.tsx` (+ test), `src/pages/osbuilder/steps.tsx` (`StepSecim` = `<ChoiceTab/>` alone; `Asks`/`WrongFolder` banner logic stays for the tree readiness), `src/components/osbuilder/OsInstall.tsx` (`<PackagePanel>` mount at the foot removed; `AmigaInstallPanel` stays until round 5).
- Delete: `src/components/osbuilder/PackagePanel.tsx`, `src/components/osbuilder/PackagePanel.test.tsx`.
- Catalogues: `osBuilder.choice.updates`, `osBuilder.choice.firstboot`, `osBuilder.choice.firstbootRow`, `osBuilder.choice.firstbootHint`, `osBuilder.choice.notInList`, `osBuilder.choice.untick`; retired `osinstall.packages.*` keys deleted where the grep finds no renderer after `PackagePanel` is gone.

**The updates group** — `osinstallChain(release, folders, treeRoot, rom, overrides)` with `folders = session.material.folders.map(f => f.path)`, `treeRoot = destination when useDestinationCheck(destination).tree?.isTree else null`, `rom = session.rom.path`, `overrides = slotOverrides(settings.remembered)` (the way `AmigaInstallPanel.tsx:999-1007` builds its `overridesKey`); rows = `chainLines(report.rows)`; the group renders only when `rows.length > 0` (AmigaOS 3.2 gets none, spec § 1.5). Per row (`data-testid="choice-update-row" data-package={line.id}`): a checkbox whose state and enablement follow one pure function in `src/lib/chain.ts`:

```ts
/** How a chain row is drawn on the choice tab (four-tab design § 3.2). */
export type ChoiceRowState =
  | { tick: "on"; enabled: false; reason: "installed" }
  | { tick: "off"; enabled: false; reason: "not-yet-runnable" | "not-placeable" | "runs-on-amiga" | "missing" | "not-needed" }
  | { tick: "user"; enabled: true };
export function choiceRowState(line: ChainLine, row: ChainRow): ChoiceRowState;
```

with: `installed` → on/disabled; `not-yet-runnable` → off/disabled; `refused` with a `NotPlaceable` block → off/disabled `not-placeable`; `row.sentenceFacts.runsOnAmiga === true` → off/disabled `runs-on-amiga`; `missing` → off/disabled; `not-needed` → off/disabled; `ready` / `blocked` / `blocked-component` / `refused` (ambiguous) → user (tickable; the run refuses later with the row's own sentence). Tested in `chain.test.ts`, one case per arm. The label is `line.name`; the sentence under it `t(line.phrase.key, line.phrase.params)`; `line.where` if present. Ticks read/write `session.packages.chosen` (`setPackages({ chosen })`). A `packages.chosen` id with no row renders `osBuilder.choice.notInList` (en "{{id}} is not in this release's list" / tr "{{id}} bu sürümün listesinde yok") with an *untick* button (`osBuilder.choice.untick` — en "Untick" / tr "İşareti kaldır") — PackagePanel's N4 behaviour, kept.

**The first-boot tick** — `<label data-testid="choice-firstboot">` checkbox `checked={session.firstboot.wanted ?? true}` `onChange={(e) => setFirstBoot({ wanted: e.target.checked })}`; label `osBuilder.choice.firstbootRow` (en "At first boot on the Amiga: detect the hardware, mount the extra disks and datatypes, write the report to S:FirstBoot.log" / tr "Amiga'da ilk açılışta: donanımı tanı, ek diskleri ve datatype'ları kur, raporu S:FirstBoot.log'a yaz"); hint `osBuilder.choice.firstbootHint` (en "Runs once, then removes itself. You read the report on the card lane." / tr "Bir kez koşar, sonra kendini siler. Raporu kart hattında okursunuz."). When the destination is an ART tree, `firstbootPreview(destination)`'s `alreadyWritten` adds the existing `firstboot-already-written` sentence under the row.

- [ ] **Step 1: Tests** — `chain.test.ts`: `choiceRowState` one case per arm (nine). `ChoiceTab.test.tsx`:

```tsx
describe("the updates group", () => {
  it("draws the chain's rows in the material's order, rank 4 twice, with each row's own sentence", …);   // nine rows from a fixture shaped after chain.test.ts's fixtures
  it("ticks an installed row and disables it; leaves a not-yet-runnable row unticked and disabled with its sentence", …);
  it("never offers a row that runs on the Amiga — one route in the wizard", …);
  it("ticks through session.packages.chosen and reads a remembered tick back", …);
  it("shows a remembered id that is not a row, untickable, and lets it be unticked", …);
  it("draws no updates group for a release without a chain", …);                                        // rows: []
  it("asks the chain with the readout's overrides, so both say the same about one file", …);           // chainMock called with the (slot, path) pairs from amigaInstall.archive.* seeds
});
describe("the first-boot tick", () => {
  it("is ticked when nothing is remembered, and rendering writes nothing", …);
  it("unticking writes wanted=false and it survives a reload", …);
  it("says the tree already carries a block when it does", …);                                          // firstbootPreview mock alreadyWritten: true with a tree destination
});
```

Full bodies; the fixtures for rows come from `src/lib/chain.test.ts` (read it and reuse its row builders).

- [ ] **Step 2: Red. Step 3: Build** (`choiceRowState`, the two groups, the session field). - [ ] **Step 4: Delete `PackagePanel.tsx` and its test**; `OsInstall.tsx` drops the mount and the `packagesFolder`/`packagesChosen` plumbing that only fed it (keep what `AmigaInstallPanel` still takes); `StepSecim` = `<ChoiceTab/>` with the readiness banner above it; `steps.test.tsx`'s `packages`/`firstboot` mocks removed. Retired keys: `grep -rn "osinstall.packages\." src --include=*.ts --include=*.tsx` — delete from both catalogues every key with no renderer; list them. - [ ] **Step 5: The PackagePanel test ledger** — every `it(...)` in the deleted file, by name, with one fate: *covered by `<ChoiceTab test>`* (N4 → the not-in-list case; M3 → the not-placeable arm), *moves to round 4* (F1 downgrade marked, F2 refused add, F10 outcome, m6 refused entry name — the run's report), *dropped because* (the artefact picker and the package-folder field — the destination on tab 3 and the slots replaced them; the updates link — the lane has no separate updates screen). Written into the round report in Task 5; the list is prepared here. - [ ] **Step 6: Green, parity, literal-keys, lint, suite; commit** — *"ChoiceTab: the updates in the material's order and the first-boot tick; PackagePanel deleted"*.

---

### Task 4: `OsInstall.tsx` header and the source step's remaining sections

**Files:** `src/components/osbuilder/OsInstall.tsx` header comment (it still describes ten sections and the panels), `docs/FEATURES.md` rows for the packages panel (`/os-builder/paketler`) and the chain screen — the packages row flips to *retired 2026-09-09: its ticks are tab 2 (`ChoiceTab`, guards …)*.

- [ ] Rewrite the header comment: what tab 1 holds now (release, readout, folders, the fold, refusals, plan, keymap, run, result, and `AmigaInstallPanel` at the foot until round 5) and what left (tabs 2 and 3). No code change; commit with the FEATURES rows — *"OsInstall.tsx says what it holds now; FEATURES rows for the retired packages panel"*.

---

### Task 5: Mutations, docs, the report

| # | File | Mutation | Guard |
|---|---|---|---|
| M1 | `useInstallPlan.ts` | sanitize against a `null` catalogue (`catalogue ?? []`) | `does not write a tick set while the catalogue has not arrived` |
| M2 | `useInstallPlan.ts` | always two plan calls | `plans once when nothing is excluded…` |
| M3 | `chain.ts` `choiceRowState` | `runsOnAmiga === true` → user | `never offers a row that runs on the Amiga` |
| M4 | `ChoiceTab.tsx` | write `firstboot.wanted` on mount | `is ticked when nothing is remembered, and rendering writes nothing` |
| M5 | `ChoiceTab.tsx` | ticks write `osinstall.chosen.<release>` | `ticks through the session, never the legacy key` |
| M6 | `ChoiceTab.tsx` | drop `overrides` from the chain call | `asks the chain with the readout's overrides` |
| M7 | `ChoiceTab.tsx` | `<details open>` on the replaces fold | `says what a layering component would replace, folded and counted` |

Verification unpiped (lint, test, parity, sweeps, `git diff --stat main -- src-tauri` empty). Report `.superpowers/sdd/2026-09-09-four-tabs/round-3-report.md`: rulings, both by-name fate lists (OsInstall cases, PackagePanel cases), mutations with tails, quoted verification, owed by a person (listed, not requested). Docs: STATUS item 0 in place (round 3 landed; ART-290 fixed; next round 4 and then the package); session-log row; FEATURES rows; CHANGELOG line (*"OS Builder: what to install is one list — the release's parts, the AmigaOS 3.9 updates in order, first boot — with the parts' ticks now kept with the rest of the build's choices"*); ISSUES: ART-290 → Fixed with the guard `ticks through the session, never the legacy key`. **No build.**

---

## Self-review

**Spec coverage:** § 3.2 three groups — Tasks 2-3; the per-row rules — `choiceRowState` (Task 3) minus the route control (ruled out by the one-route decision); the file per row for `add_package` — round 4 (ruled); the replaces fold — Task 2 (components only, ruled); `firstboot.wanted` — Task 3; § 5 table — the legacy keys read once, never written (Task 1), `packages.folder` untouched; § 7 guards — `ChoiceTab.test.tsx` and `chain.test.ts` above, ART-289's guard deferred with its preview (ruled); § 8 row 3 — all.

**Placeholders:** the test skeletons name the required assertions and point at the fixture files to build from; each implementer writes the bodies — the reviewer checks every named assertion exists.

**Type consistency:** `InstallPlanInputs`/`InstallPlanState` used identically in `OsInstall` (Task 1) and `ChoiceTab` (Task 2); `ComponentChoice` from `buildSession.ts:107-110`; `choiceRowState(line: ChainLine, row: ChainRow)` defined in Task 3 and mutated in Task 5 M3; `firstboot.wanted` optional boolean in `buildSession.ts` and read with `?? true` in `ChoiceTab`.
