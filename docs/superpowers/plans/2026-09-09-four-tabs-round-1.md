# Four tabs, round 1 — the lane and the tabs — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** The OS Builder's install lane becomes `hedef` + four numbered tabs (`dosyalar · secim · makine · derle`), the three retired routes redirect, the strip draws `hedef` as a chip, and a bar with the destination and a *Derle'ye git* button sits under every tab — with every panel that works today still reachable.

**Architecture:** Front-end only. `buildSteps.ts` is the one source of the lane; a new `routes.tsx` holds the child routes (including the redirects) so `App.tsx` and the router tests use the same table; `steps.tsx` gains four thin tab components and loses the retired three; `OsBuilder.tsx`'s strip numbers from the second entry and a new `BuildBar.tsx` renders under the outlet. **In this round the tabs mount today's panels unchanged**: `dosyalar` mounts `OsInstall` whole, `secim` mounts `PackagePanel` and `FirstBootPanel` with the readiness banner, `makine` and `derle` say in one sentence where their fields live today. Rounds 2-4 move the content.

**Tech Stack:** React 18, react-router-dom 6 (`Navigate`, `Outlet`), react-i18next, Vitest + jsdom + Testing Library.

**Spec:** `docs/superpowers/specs/2026-09-09-os-builder-four-tabs-design.md` — § 2 (the lane), § 8 row 1 (this round), § 10.4 and § 10.6 (the bar's label, the catalogue grep). Read § 2 whole before Task 1.

**Rulings against the spec's wording, made 2026-09-09 while planning** (the spec's § 8 row 1 says the tabs "render their heading and nothing else"):

- A tab that renders nothing would make the branch's build unusable until round 4 and hide working features (CLAUDE.md: register unready, never hide). So in this round `dosyalar` mounts `OsInstall` as it is today (which still mounts `PackagePanel` and `AmigaInstallPanel` at its foot — that duplication ends in round 3/5), `secim` mounts `PackagePanel` and `FirstBootPanel`, and `makine` / `derle` render a heading plus one sentence that links to `dosyalar`, where those fields are today. Nothing is lost in round 1.
- The bar shows the destination only. Its size line (*1 980 dosya + 4 güncelleme*) needs the plan and arrives in round 4.
- The bar's button reads *Derle'ye git* on tabs 1-3 and is **absent** on `derle` in this round (spec § 10.4: two buttons with one label and different effects is a defect; round 4 gives tab 4 its own *Derle*).
- `readiness()`'s tree-consuming set becomes `["secim"]` — `PackagePanel` and `FirstBootPanel` both read the tree. `makine` and `derle` are always ready in this round.
- `osBuilder.step.kaynak` stays in the catalogues because `AmigaInstallPanel.tsx:1611` and `:1678` render it; the three keys `paketler` / `amiga-kurulum` / `ilk-acilis` are deleted only if `grep -rn "osBuilder.step.<id>" src --include=*.ts --include=*.tsx` finds no renderer outside the catalogues (the executor runs the grep and records the answer). Links inside `PackagePanel.tsx:427` and `AmigaInstallPanel.tsx` to retired segments are left as they are: the redirects cover them and both panels are on their way out.

## Global Constraints

- Turkish path segments, untranslated (`buildSteps.ts:24-27`): `hedef · dosyalar · secim · makine · derle · kart · birimler`. The retired `kaynak · paketler · amiga-kurulum · ilk-acilis` leave `STEP_IDS` and become `<Navigate replace>` redirects: `kaynak → dosyalar`, `paketler → secim`, `amiga-kurulum → secim`, `ilk-acilis → secim`.
- The card lane (`hedef · kart`), the volumes lane (`hedef · birimler`), the `distro` kind and `StepHedef` are **untouched**.
- Every new i18n key lands in `src/i18n/en.json` **and** `src/i18n/tr.json` in the same commit; parity test `pnpm vitest run -t "have identical key sets"`; the literal-keys test (`src/i18n/literal-keys.test.ts`) checks every literal `t("…")`.
- `src/lib` never renders a string: `buildSteps.ts` returns keys.
- No new remembered key, no write on render. The bar reads `osinstall.destination.<release>` through the same key helper and guard `OsInstall.tsx` uses (`rememberedComponentKey("osinstall.destination", release)`, `isTextOrNothing` — read `OsInstall.tsx:615-625` for the exact call and copy it).
- No Rust change: `git diff --stat main -- src-tauri` empty at the end.
- Component tests are `*.test.tsx`; mock at the `@/lib/*` boundary; output pristine (no `act` warnings, no unhandled rejections).
- Commit messages via a file in the scratchpad and `git commit -F`; `git branch --show-current` before every commit; branch `art-four-tabs` (exists, from `art-simplify-a-readout-first`).
- Scratchpad: `C:\Users\ismoz\AppData\Local\Temp\claude\D--Projeler-Amiga\06212805-e1dd-4b6f-b2ae-dd6f17219be6\scratchpad`.
- Mutations by absolute path with `shutil.copyfile`, never `git checkout --`.

---

## File structure

**Create**
- `src/pages/osbuilder/routes.tsx` — `OsBuilderRoutes()` returning the `<Route>` children of `/os-builder` (index, seven live steps, four redirects). One table, used by `App.tsx` and by the router tests.
- `src/pages/osbuilder/BuildBar.tsx` — the bar: destination line + *Derle'ye git*. Reads the session and the remembered destination; renders only when `session.kind === "install"` and the current step is not `derle`.
- `src/pages/osbuilder/BuildBar.test.tsx`.

**Modify**
- `src/lib/buildSteps.ts` — `STEP_IDS`, `stepsFor`, `readiness`, new `kindLabelKey`.
- `src/lib/buildSteps.test.ts` — the literal lanes, readiness for `secim`, `kindLabelKey`.
- `src/pages/osbuilder/steps.tsx` — `StepDosyalar`, `StepSecim`, `StepMakine`, `StepDerle`; the retired three removed; `Asks`/`WrongFolder` link to `dosyalar`.
- `src/pages/osbuilder/steps.test.tsx` — routes from `routes.tsx`, redirect cases, strip counts.
- `src/pages/OsBuilder.tsx` — the strip: `hedef` chip + numbering from `slice(1)`; `BuildBar` under the outlet; the drop effect navigates to `dosyalar`.
- `src/App.tsx` — `<OsBuilderRoutes/>` replaces the inline children.
- `src/i18n/en.json`, `src/i18n/tr.json` — `osBuilder.step.{dosyalar,secim,makine,derle}`, `osBuilder.tab.{makineNotYet,derleNotYet}`, `osBuilder.bar.{destination,noDestination,goToBuild}`; retired step keys per the ruling above.
- `scripts/osbuilder-strip-check.py` — `#/os-builder/paketler` → `#/os-builder/secim`; the comment "four steps" → "hedef chip + four".
- `docs/STATUS.md`, `docs/session-log.md`, `docs/FEATURES.md`, `CHANGELOG.md` — Task 4.

---

### Task 1: The lane in `buildSteps.ts`

**Files:**
- Modify: `src/lib/buildSteps.ts`
- Modify: `src/lib/buildSteps.test.ts`
- Modify: `src/i18n/en.json`, `src/i18n/tr.json` (the four step labels — added here because `stepLabelKey` must resolve for every id)

**Interfaces:**
- Produces: `STEP_IDS = ["hedef","dosyalar","secim","makine","derle","kart","birimler"] as const`; `type StepId`; `stepsFor(kind)`; `readiness(session, step, treeIsDistribution?)` unchanged signature, tree-consuming set `["secim"]`; `stepLabelKey(step)` unchanged; **new** `kindLabelKey(kind: BuildKind): string` → `osBuilder.what.install | osBuilder.what.bootCard | osBuilder.what.prepareVolumes | osBuilder.what.distro`.

- [ ] **Step 1: Rewrite the failing tests**

In `src/lib/buildSteps.test.ts` replace the `stepsFor` describe's first case and the `readiness` describe's cases that name retired ids, and add `kindLabelKey`:

```ts
describe("stepsFor", () => {
  it("gives the install job hedef and its four numbered tabs, nothing else", () => {
    expect(stepsFor("install")).toEqual(["hedef", "dosyalar", "secim", "makine", "derle"]);
  });

  it("gives the card job the card step and none of the install's", () => {
    expect(stepsFor("boot-card")).toEqual(["hedef", "kart"]);
  });

  it("gives volume preparation its own", () => {
    expect(stepsFor("prepare-volumes")).toEqual(["hedef", "birimler"]);
  });

  it("leaves the unbuilt distro job at the picker", () => {
    expect(stepsFor("distro")).toEqual(["hedef"]);
  });

  it("always begins at the picker, whatever the kind", () => {
    for (const kind of ["distro", "boot-card", "install", "prepare-volumes"] as const) {
      expect(stepsFor(kind)[0]).toBe("hedef");
    }
  });

  it("offers no step that is not a real step, and no retired one", () => {
    for (const kind of ["distro", "boot-card", "install", "prepare-volumes"] as const) {
      for (const step of stepsFor(kind)) {
        expect(STEP_IDS).toContain(step);
      }
    }
    for (const retired of ["kaynak", "paketler", "amiga-kurulum", "ilk-acilis"]) {
      expect(STEP_IDS as readonly string[]).not.toContain(retired);
    }
  });
});

describe("readiness", () => {
  it("says the choice tab with no tree must ask", () => {
    expect(readiness(sessionWith(), "secim")).toBe("asks");
  });

  it("says the choice tab with a tree is ready", () => {
    const s = sessionWith({ tree: { root: "E:\\dist", builtHere: true } });
    expect(readiness(s, "secim")).toBe("ready");
  });

  it("never makes the first step ask — it is where a build begins", () => {
    expect(readiness(sessionWith(), "hedef")).toBe("ready");
  });

  it("treats an empty string as no tree at all", () => {
    const s = sessionWith({ tree: { root: "", builtHere: false } });
    expect(readiness(s, "secim")).toBe("asks");
  });

  it("does not make a step ask for something it does not use", () => {
    // `dosyalar` owns its own inputs; `makine` and `derle` hold nothing yet
    // in this round; `kart` and `birimler` are the other lanes.
    const s = sessionWith();
    for (const step of ["dosyalar", "makine", "derle", "kart", "birimler"] as const) {
      expect(readiness(s, step)).toBe("ready");
    }
  });
});

describe("stepLabelKey", () => {
  it("answers a key for every step, never a sentence", () => {
    for (const step of STEP_IDS) {
      const key = stepLabelKey(step);
      expect(key.startsWith("osBuilder.step.")).toBe(true);
      expect(key).not.toContain(" ");
    }
  });

  it("resolves to a leaf in both catalogues for every step", () => {
    for (const step of STEP_IDS) {
      expect(en.osBuilder.step[step]).toEqual(expect.any(String));
      expect(tr.osBuilder.step[step]).toEqual(expect.any(String));
    }
  });
});

describe("kindLabelKey", () => {
  it("names each kind by its own 'what are we building' label", () => {
    expect(kindLabelKey("install")).toBe("osBuilder.what.install");
    expect(kindLabelKey("boot-card")).toBe("osBuilder.what.bootCard");
    expect(kindLabelKey("prepare-volumes")).toBe("osBuilder.what.prepareVolumes");
    expect(kindLabelKey("distro")).toBe("osBuilder.what.distro");
  });

  it("resolves to a leaf in both catalogues", () => {
    for (const kind of ["distro", "boot-card", "install", "prepare-volumes"] as const) {
      const leaf = kindLabelKey(kind).replace("osBuilder.what.", "");
      expect(en.osBuilder.what[leaf]).toEqual(expect.any(String));
      expect(tr.osBuilder.what[leaf]).toEqual(expect.any(String));
    }
  });
});

describe("readiness, when ART has looked at the folder (ART-199)", () => {
  const withTree = sessionWith({ tree: { root: "E:\\dist", builtHere: false } });

  it("says the folder is the wrong one when ART has looked and it is not a tree", () => {
    expect(readiness(withTree, "secim", false)).toBe("wrong-folder");
  });

  it("is ready once ART has looked and it is a tree", () => {
    expect(readiness(withTree, "secim", true)).toBe("ready");
  });

  it("does not accuse a folder ART has not looked at yet", () => {
    expect(readiness(withTree, "secim", null)).toBe("ready");
  });

  it("still asks first when there is no folder at all", () => {
    expect(readiness(sessionWith(), "secim", false)).toBe("asks");
  });

  it("never accuses a step that does not read a tree", () => {
    expect(readiness(withTree, "kart", false)).toBe("ready");
    expect(readiness(withTree, "dosyalar", false)).toBe("ready");
  });
});
```

Add at the top of the test file: `import en from "@/i18n/en.json"; import tr from "@/i18n/tr.json";` and `kindLabelKey` to the import from `./buildSteps`. If `osBuilder.what.distro` does not exist in `en.json` (check with `python -c "import json;print('distro' in json.load(open('src/i18n/en.json',encoding='utf-8'))['osBuilder']['what'])"`), find the key `StepHedef` renders for the distro row (`grep -n "what\." src/pages/OsBuilder.tsx`) and use that leaf instead, in both the test and `kindLabelKey`; write the answer in the report.

- [ ] **Step 2: Run, see them fail**

```powershell
pnpm vitest run src/lib/buildSteps.test.ts
```
Expected: failures on the install lane, `secim` readiness, `kindLabelKey` not exported, and the four step labels missing from the catalogues.

- [ ] **Step 3: The lane**

In `src/lib/buildSteps.ts`:

```ts
/**
 * The steps that exist.
 *
 * Turkish path segments, deliberately untranslated (a URL that changed with
 * the language would break every remembered link and every
 * `builtin.rs::route` value). `hedef` is the entry every kind has; the four
 * after it are the install lane's numbered tabs (four-tab design § 2). The
 * retired `kaynak · paketler · amiga-kurulum · ilk-acilis` are redirects in
 * `pages/osbuilder/routes.tsx`, not steps — a step id no strip draws is dead
 * weight the next reader has to eliminate again.
 */
export const STEP_IDS = [
  "hedef",
  "dosyalar",
  "secim",
  "makine",
  "derle",
  "kart",
  "birimler",
] as const;
```

`stepsFor`: the `install` case returns `["hedef", "dosyalar", "secim", "makine", "derle"]`; the other three unchanged. `readiness`: the `case` list becomes `case "secim":` alone, with the comment updated (*"`secim` holds the package ticks and the first-boot tick in this round, both of which read the tree"*). Add:

```ts
/** The i18n key for a kind's own name — the `hedef` chip in the strip. */
export function kindLabelKey(kind: BuildKind): string {
  switch (kind) {
    case "install":
      return "osBuilder.what.install";
    case "boot-card":
      return "osBuilder.what.bootCard";
    case "prepare-volumes":
      return "osBuilder.what.prepareVolumes";
    case "distro":
      return "osBuilder.what.distro";
  }
}
```

Replace the module's second doc paragraph (the one naming `bilesenler`/`ozet`) with: *"Four numbered tabs for a tree, after the owner's 2026-09-09 verdict on the five-step lane (four-tab design § 1.1): `dosyalar`, `secim`, `makine`, `derle`."*

- [ ] **Step 4: The four labels, both catalogues**

`en.json` `osBuilder.step`: add `"dosyalar": "Amiga files"`, `"secim": "What to install"`, `"makine": "Kickstart and destination"`, `"derle": "Build"`. `tr.json`: `"dosyalar": "Amiga dosyaları"`, `"secim": "Ne kurulacak"`, `"makine": "Kickstart ve hedef"`, `"derle": "Derle"`. Edit with the Edit tool. Leave the retired keys in place for now (Task 2 decides them by grep).

- [ ] **Step 5: Green, lint, parity**

```powershell
pnpm vitest run src/lib/buildSteps.test.ts
pnpm vitest run -t "have identical key sets"
pnpm lint
```
Expected: all passed; parity passed; **lint fails** in `steps.tsx`/`OsBuilder.tsx`/`App.tsx`/`steps.test.tsx` wherever a retired id is used as a `StepId` — that is Task 2's work; record the error list and continue. Do not commit a red lint: **Task 1 and Task 2 commit together** (Task 2 Step 8).

---

### Task 2: Routes, redirects and the four tab components

**Files:**
- Create: `src/pages/osbuilder/routes.tsx`
- Modify: `src/pages/osbuilder/steps.tsx`, `src/App.tsx:86-95`, `src/pages/OsBuilder.tsx:93-100`
- Modify: `src/pages/osbuilder/steps.test.tsx`
- Modify: `src/i18n/en.json`, `src/i18n/tr.json` (`osBuilder.tab.*`, retired step keys)

**Interfaces:**
- Consumes: Task 1's `STEP_IDS`, `stepsFor`, `readiness`.
- Produces: `export function OsBuilderRoutes(): JSX.Element` (a fragment of `<Route>`s to spread as children of `<Route path="os-builder" element={<OsBuilder/>}>`); `StepDosyalar`, `StepSecim`, `StepMakine`, `StepDerle` exported from `steps.tsx`; testids `tab-makine`, `tab-derle`.

- [ ] **Step 1: The tests**

In `src/pages/osbuilder/steps.test.tsx`:

- Replace the import of the step components with `const { OsBuilderRoutes } = await import("@/pages/osbuilder/routes");` and the `renderAt` body with:

```tsx
function renderAt(path: string, state?: unknown) {
  return render(
    <MemoryRouter initialEntries={[{ pathname: path, state }]}>
      <Routes>
        <Route path="/os-builder" element={<OsBuilder />}>
          <OsBuilderRoutes />
        </Route>
      </Routes>
    </MemoryRouter>
  );
}
```

- Add a mock for `StepHedef`'s heavy imports only if the existing mocks do not already cover them (the file mocks `@/lib/osinstall` and `@/lib/settings`; `OsBuilder.tsx` imports `@/lib/distro` and `@/lib/pistorm` — add `vi.mock("@/lib/distro", () => ({ distroProfiles: vi.fn().mockResolvedValue([]), distroCheckCard: vi.fn(), distroRomFamilyMatches: vi.fn(), distroMeasureImage: vi.fn() }))` and `vi.mock("@/lib/pistorm", async (importOriginal) => ({ ...(await importOriginal<typeof import("@/lib/pistorm")>()), pistormIdentifyRom: vi.fn() }))` if rendering `/os-builder/hedef` in the redirect tests otherwise throws; report which).
- Every `renderAt("/os-builder/paketler")` in existing cases becomes `renderAt("/os-builder/secim")`; every `/os-builder/amiga-kurulum` and `/os-builder/ilk-acilis` becomes `/os-builder/secim`; `/os-builder/kaynak` becomes `/os-builder/dosyalar`. The case *asks on the Amiga-side step too, and does not gate it* is renamed *asks on the choice tab too, and does not gate it* and asserts the `packages` mock testid is still rendered.
- The strip case *shows the steps this kind has, and not the others* now expects **`getAllByRole("link").length` to be 5** still (hedef chip is a link + four tabs) and the numbered labels: `screen.getByRole("link", { name: /^1\. Amiga files/ })`, `/^4\. Build/`; and `screen.queryByRole("link", { name: /^1\. What are we building/ })` is **null** (hedef is not numbered).
- Add:

```tsx
describe("the retired routes still resolve (four-tab design § 2)", () => {
  // A URL that stopped resolving is a link in the operation log, a remembered
  // position and a habit that now goes nowhere; the `*` catch-all would send
  // it home, which is the confident-wrong form of "this moved". So the guard
  // asserts the specific tab, never "something rendered".
  beforeEach(() => {
    seed({
      "buildSession.kind": "install",
      "buildSession.tree": { root: "E:\\dist", builtHere: true },
    });
  });

  it("sends kaynak to the files tab", () => {
    renderAt("/os-builder/kaynak");
    expect(screen.getByTestId("install")).toBeTruthy();
    expect(screen.getByRole("link", { name: /^1\. Amiga files/ }).getAttribute("aria-current")).toBe("page");
  });

  it("sends paketler, amiga-kurulum and ilk-acilis to the choice tab", () => {
    for (const old of ["paketler", "amiga-kurulum", "ilk-acilis"]) {
      const view = renderAt(`/os-builder/${old}`);
      expect(screen.getByTestId("packages")).toBeTruthy();
      expect(screen.getByTestId("firstboot")).toBeTruthy();
      expect(screen.getByRole("link", { name: /^2\. What to install/ }).getAttribute("aria-current")).toBe("page");
      view.unmount();
    }
  });
});

describe("the two tabs that hold nothing yet say where their fields are", () => {
  beforeEach(() => seed({ "buildSession.kind": "install" }));

  it("makine names the files tab", () => {
    renderAt("/os-builder/makine");
    const tab = screen.getByTestId("tab-makine");
    expect(tab.textContent).toContain(i18n.t("osBuilder.tab.makineNotYet"));
    expect(within(tab).getByRole("link").getAttribute("href")).toBe("/os-builder/dosyalar");
  });

  it("derle names the files tab", () => {
    renderAt("/os-builder/derle");
    const tab = screen.getByTestId("tab-derle");
    expect(tab.textContent).toContain(i18n.t("osBuilder.tab.derleNotYet"));
    expect(within(tab).getByRole("link").getAttribute("href")).toBe("/os-builder/dosyalar");
  });
});
```

(`aria-current="page"` is what `<NavLink>` sets on the active link — Task 3 switches the strip to `NavLink`; until then these two assertions fail, which is expected and recorded. `i18n` and `within` are already imported or added from `i18next` / `@testing-library/react`.)

- The drop case *reaches the media step* stays: the mock `install` testid renders on `dosyalar`.

- [ ] **Step 2: Run, see the new and the renamed cases fail**

```powershell
pnpm vitest run src/pages/osbuilder/steps.test.tsx
```
Expected: `routes` module missing; then, after Step 3, the strip/`aria-current` cases still failing until Task 3.

- [ ] **Step 3: `routes.tsx`**

```tsx
// The OS Builder's child routes, in one table (four-tab design § 2).
//
// `App.tsx` renders this under `/os-builder`, and the router tests render the
// same function — so a redirect that exists in the application exists in the
// test, and one that is deleted fails a test rather than silently sending a
// remembered URL to the home screen through the `*` catch-all.
//
// **Retired segments redirect, never 404.** `kaynak`, `paketler`,
// `amiga-kurulum` and `ilk-acilis` were the five-step lane's routes until
// 2026-09-09; a link in the operation log, a remembered position and a
// person's habit still name them. A redirect is the honest form of "this
// moved".

import { Navigate, Route } from "react-router-dom";

import {
  StepBirimler,
  StepDerle,
  StepDosyalar,
  StepKart,
  StepMakine,
  StepSecim,
} from "@/pages/osbuilder/steps";
import { StepHedef } from "@/pages/OsBuilder";

export function OsBuilderRoutes() {
  return (
    <>
      <Route index element={<Navigate to="hedef" replace />} />
      <Route path="hedef" element={<StepHedef />} />
      <Route path="dosyalar" element={<StepDosyalar />} />
      <Route path="secim" element={<StepSecim />} />
      <Route path="makine" element={<StepMakine />} />
      <Route path="derle" element={<StepDerle />} />
      <Route path="kart" element={<StepKart />} />
      <Route path="birimler" element={<StepBirimler />} />
      <Route path="kaynak" element={<Navigate to="/os-builder/dosyalar" replace />} />
      <Route path="paketler" element={<Navigate to="/os-builder/secim" replace />} />
      <Route path="amiga-kurulum" element={<Navigate to="/os-builder/secim" replace />} />
      <Route path="ilk-acilis" element={<Navigate to="/os-builder/secim" replace />} />
    </>
  );
}
```

**React Router 6 note:** `<Routes>` only accepts `<Route>` elements as children, and a component that *returns* a fragment of `<Route>`s is not walked by `createRoutesFromChildren`. If rendering shows the children as unmatched (every path falls through), switch to the data form: export `const OS_BUILDER_CHILDREN: RouteObject[] = [...]` and render them in both places with `useRoutes` or by mapping to `<Route>` elements inline: `{OS_BUILDER_CHILDREN.map((r) => <Route key={r.path ?? "index"} {...r} />)}`. Try the fragment first; if it fails, use the mapped array (the `element` values are the same). Report which form landed.

In `App.tsx` replace the seven inline children (lines 87-94) with `<OsBuilderRoutes />` (or the map) and import it; remove the now-unused step imports from `App.tsx`. In `OsBuilder.tsx:93-100` the drop effect navigates to `stepPath("dosyalar")`.

- [ ] **Step 4: The tab components in `steps.tsx`**

Delete `StepKaynak`, `StepPaketler`, `StepAmigaKurulum`, `StepIlkAcilis`. `Asks` and `WrongFolder` link to `/os-builder/dosyalar` with `t("osBuilder.step.dosyalar")`. Add:

```tsx
/** Tab 1 — Amiga files. In round 1 this is the whole of today's source step. */
export function StepDosyalar() {
  const location = useLocation();
  const dropped = (location.state as { path?: string } | null)?.path ?? null;
  return (
    <OsInstall droppedMedia={dropped ? { path: dropped, arrivalKey: location.key } : null} />
  );
}

/**
 * Tab 2 — what to install. In round 1 it holds the two panels round 3 turns
 * into ticks: the package checklist and the first-boot block. Both read the
 * tree, so one readiness banner covers both.
 */
export function StepSecim() {
  const { session, setTree, setPackages } = useBuildSession();
  const isTree = useTreeCheck(session.tree.root);
  const state = readiness(session, "secim", isTree);
  return (
    <>
      {state === "asks" && <Asks />}
      {state === "wrong-folder" && <WrongFolder />}
      <PackagePanel
        treeRoot={session.tree.root}
        onTreeRootChange={(root) => setTree({ root, builtHere: false })}
        packageFolder={session.packages.folder}
        onPackageFolderChange={(folder) => setPackages({ folder })}
        chosen={session.packages.chosen}
        onChosenChange={(chosen) => setPackages({ chosen })}
        release={session.release}
      />
      <FirstBootPanel
        treeRoot={session.tree.root}
        onTreeRootChange={(root) => setTree({ root, builtHere: false })}
      />
    </>
  );
}

/** A tab whose fields still live on the files tab — says so, links there. */
function NotYet({ id, sentenceKey }: { id: "makine" | "derle"; sentenceKey: string }) {
  const { t } = useTranslation();
  return (
    <section className="card" data-testid={`tab-${id}`} style={{ marginBottom: 16 }}>
      <h2 style={{ fontSize: 16, marginTop: 0 }}>{t(`osBuilder.step.${id}`)}</h2>
      <p className="muted" style={{ fontSize: 12, margin: 0 }}>
        {t(sentenceKey)} <Link to="/os-builder/dosyalar">{t("osBuilder.step.dosyalar")}</Link>
      </p>
    </section>
  );
}

/** Tab 3 — Kickstart and destination. Round 2 moves the three fields here. */
export function StepMakine() {
  return <NotYet id="makine" sentenceKey="osBuilder.tab.makineNotYet" />;
}

/** Tab 4 — build. Round 4 moves the plan, the button and the run here. */
export function StepDerle() {
  return <NotYet id="derle" sentenceKey="osBuilder.tab.derleNotYet" />;
}
```

Remove the `AmigaInstallPanel` import from `steps.tsx` (it is no longer mounted here; it still renders inside `OsInstall`). Update the file's header comment: the four tabs, round 1's ruling, the retired three.

Catalogues: `osBuilder.tab.makineNotYet` — en *"The Kickstart, the keyboard layout and the destination folder are on the files tab for now:"*, tr *"Kickstart, klavye düzeni ve hedef klasör şimdilik Amiga dosyaları sekmesinde:"*; `osBuilder.tab.derleNotYet` — en *"The plan and the Build button are on the files tab for now:"*, tr *"Plan ve Derle düğmesi şimdilik Amiga dosyaları sekmesinde:"*.

Retired step keys: run `grep -rn "osBuilder.step.paketler\|osBuilder.step.amiga-kurulum\|osBuilder.step.ilk-acilis\|osBuilder.step.kaynak" src --include=*.ts --include=*.tsx`. Delete from **both** catalogues every key with no hit outside `src/i18n/`; keep `kaynak` (AmigaInstallPanel renders it) and any other with a hit. Write the grep output in the report.

- [ ] **Step 5: Run the router tests**

```powershell
pnpm vitest run src/pages/osbuilder/steps.test.tsx
```
Expected: everything passes except the cases that assert the strip's numbering and `aria-current` (Task 3). Name them in the report.

- [ ] **Step 6: Type-check**

```powershell
pnpm lint
```
Expected: clean twice — every retired id is gone from `App.tsx`, `steps.tsx`, `OsBuilder.tsx` and the tests. (`OsBuilder.tsx` still numbers from `slice(0)`; that is Task 3 and compiles.)

- [ ] **Step 7: Parity and the literal-keys test**

```powershell
pnpm vitest run -t "have identical key sets"
pnpm vitest run src/i18n/literal-keys.test.ts
```
Expected: both pass.

- [ ] **Step 8: Commit Tasks 1 and 2 together**

`<scratchpad>\msg-r1-t2.txt`:

```
Four tabs, round 1: the lane, the routes and the redirects

buildSteps: hedef · dosyalar · secim · makine · derle for a tree, the three
other kinds untouched; readiness reads the tree on secim only; kindLabelKey
for the hedef chip. routes.tsx is the one table App.tsx and the router tests
render, with kaynak → dosyalar and paketler / amiga-kurulum / ilk-acilis →
secim as redirects. In this round dosyalar mounts OsInstall whole, secim
mounts PackagePanel and FirstBootPanel, makine and derle say where their
fields are today. Strip numbering and aria-current land in the next commit.
```

```powershell
git branch --show-current
git add src/lib/buildSteps.ts src/lib/buildSteps.test.ts src/pages/osbuilder/routes.tsx src/pages/osbuilder/steps.tsx src/pages/osbuilder/steps.test.tsx src/pages/OsBuilder.tsx src/App.tsx src/i18n/en.json src/i18n/tr.json
git commit -F "<scratchpad>\msg-r1-t2.txt"
```

The two strip cases in `steps.test.tsx` are red at this commit; the message says so and Task 3 closes them within the same round. Do not `pnpm test` the whole suite here — do it in Task 3.

---

### Task 3: The strip with the `hedef` chip, and the bar

**Files:**
- Modify: `src/pages/OsBuilder.tsx:105-137` (the `<nav>`), plus `<BuildBar/>` after `<Outlet/>`
- Create: `src/pages/osbuilder/BuildBar.tsx`, `src/pages/osbuilder/BuildBar.test.tsx`
- Modify: `src/i18n/en.json`, `src/i18n/tr.json` (`osBuilder.bar.*`)
- Modify: `scripts/osbuilder-strip-check.py`

**Interfaces:**
- Consumes: `stepsFor`, `stepLabelKey`, `kindLabelKey` from Task 1; `useBuildSession`; `rememberedComponentKey`/`useRemembered`/`isTextOrNothing` as `OsInstall.tsx` uses them for `osinstall.destination`.
- Produces: `BuildBar()` with testids `build-bar`, `build-bar-destination`, `build-bar-go`.

- [ ] **Step 1: The bar's tests**

`src/pages/osbuilder/BuildBar.test.tsx`:

```tsx
// @vitest-environment jsdom
//
// The bar under every tab of the install lane (four-tab design § 2): the
// destination, and one button that only navigates in this round. Two rules
// it must keep: it never writes a setting (it reads the same remembered
// destination the files tab writes), and its button is absent on the build
// tab, where round 4 puts the real Derle — one label, one effect.

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { MemoryRouter, Route, Routes, useLocation } from "react-router-dom";
import i18n from "i18next";

import "@/i18n";

vi.mock("@/lib/settings", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/lib/settings")>()),
  saveSettings: vi.fn().mockResolvedValue(undefined),
  getSettings: vi.fn(),
}));

const { useSettingsStore } = await import("@/stores/settingsStore");
const { DEFAULT_SETTINGS } = await import("@/lib/settings");
const { BuildBar } = await import("@/pages/osbuilder/BuildBar");

function seed(remembered: Record<string, unknown>) {
  useSettingsStore.setState({ loaded: true, settings: { ...DEFAULT_SETTINGS, remembered } });
}

function WhereAmI() {
  const location = useLocation();
  return <div data-testid="where">{location.pathname}</div>;
}

function renderAt(path: string) {
  return render(
    <MemoryRouter initialEntries={[path]}>
      <Routes>
        <Route
          path="/os-builder/*"
          element={
            <>
              <BuildBar />
              <WhereAmI />
            </>
          }
        />
      </Routes>
    </MemoryRouter>
  );
}

afterEach(() => {
  cleanup();
  useSettingsStore.setState({ loaded: false, settings: DEFAULT_SETTINGS });
});

describe("the build bar", () => {
  beforeEach(() => seed({ "buildSession.kind": "install", "buildSession.release": "AmigaOS 3.9" }));

  it("shows the remembered destination for the release, and says so when there is none", () => {
    seed({
      "buildSession.kind": "install",
      "buildSession.release": "AmigaOS 3.9",
      "osinstall.destination.AmigaOS 3.9": "E:\\amiga\\Amigatolon\\sonuclar",
    });
    renderAt("/os-builder/dosyalar");
    expect(screen.getByTestId("build-bar-destination").textContent).toContain("E:\\amiga\\Amigatolon\\sonuclar");

    cleanup();
    seed({ "buildSession.kind": "install", "buildSession.release": "AmigaOS 3.9" });
    renderAt("/os-builder/dosyalar");
    expect(screen.getByTestId("build-bar-destination").textContent).toBe(i18n.t("osBuilder.bar.noDestination"));
  });

  it("goes to the build tab and starts nothing", async () => {
    renderAt("/os-builder/secim");
    await userEvent.setup().click(screen.getByTestId("build-bar-go"));
    expect(screen.getByTestId("where").textContent).toBe("/os-builder/derle");
  });

  it("has no button on the build tab itself — one label, one effect", () => {
    renderAt("/os-builder/derle");
    expect(screen.getByTestId("build-bar")).toBeTruthy();
    expect(screen.queryByTestId("build-bar-go")).toBeNull();
  });

  it("is not drawn for the other lanes", () => {
    seed({ "buildSession.kind": "boot-card" });
    renderAt("/os-builder/kart");
    expect(screen.queryByTestId("build-bar")).toBeNull();
  });

  it("never writes a setting by rendering", () => {
    renderAt("/os-builder/dosyalar");
    const bag = useSettingsStore.getState().settings.remembered as Record<string, unknown>;
    expect(Object.keys(bag).filter((k) => k.startsWith("osinstall.destination"))).toEqual([]);
  });
});
```

If `useSettingsStore.getState().settings.remembered` is not the store's shape, read `steps.test.tsx`'s `seed` and `rememberedBag` helpers (`OsInstall.test.tsx:395-405`) and use the same accessors.

- [ ] **Step 2: Run, see them fail on the missing module**

```powershell
pnpm vitest run src/pages/osbuilder/BuildBar.test.tsx
```

- [ ] **Step 3: `BuildBar.tsx`**

```tsx
// The bar under the install lane's tabs (four-tab design § 2).
//
// One place that answers "where is this going" on every tab, and one button
// that takes the person to the build tab. **It only navigates**: a run
// starts from tab 4's own button, once, knowingly (§ 10.4 — a button that
// navigates on three tabs and runs on the fourth, under one label, is a
// defect waiting to happen; so this button is absent on `derle`).
//
// It reads the remembered destination through the same key and guard the
// files tab writes it with, and writes nothing: `remembered.ts`'s rule.

import { useTranslation } from "react-i18next";
import { useLocation, useNavigate } from "react-router-dom";

import { isTextOrNothing, rememberedComponentKey, useRemembered } from "@/lib/remembered";
import { useBuildSession } from "@/lib/useBuildSession";

export function BuildBar() {
  const { t } = useTranslation();
  const { session } = useBuildSession();
  const location = useLocation();
  const navigate = useNavigate();
  const [destination] = useRemembered<string | null>(
    rememberedComponentKey("osinstall.destination", session.release),
    isTextOrNothing,
    null
  );

  if (session.kind !== "install") return null;
  const onBuildTab = location.pathname === "/os-builder/derle";

  return (
    <div
      data-testid="build-bar"
      style={{
        position: "sticky",
        bottom: 0,
        display: "flex",
        gap: 12,
        alignItems: "center",
        flexWrap: "wrap",
        padding: "8px 12px",
        marginTop: 16,
        borderTop: "1px solid var(--border)",
        background: "var(--bg)",
        fontSize: 12,
      }}
    >
      <span className="muted" data-testid="build-bar-destination" style={{ flex: 1, minWidth: "12em", wordBreak: "break-all" }}>
        {destination ? t("osBuilder.bar.destination", { path: destination }) : t("osBuilder.bar.noDestination")}
      </span>
      {!onBuildTab && (
        <button className="btn" data-testid="build-bar-go" onClick={() => navigate("/os-builder/derle")}>
          {t("osBuilder.bar.goToBuild")}
        </button>
      )}
    </div>
  );
}
```

Match the import names to what `OsInstall.tsx:615-625` actually imports for the destination (the helper may be named differently — copy that call). Catalogues: `osBuilder.bar.destination` — en *"Tree: {{path}}"*, tr *"Ağaç: {{path}}"*; `osBuilder.bar.noDestination` — en *"No destination folder chosen yet."*, tr *"Hedef klasör henüz seçilmedi."*; `osBuilder.bar.goToBuild` — en *"Go to Build"*, tr *"Derle'ye git"*.

- [ ] **Step 4: The strip**

In `OsBuilder.tsx` replace the `<nav>` body:

```tsx
      <nav
        aria-label={t("nav.osBuilder")}
        style={{ display: "flex", gap: 8, flexWrap: "wrap", alignItems: "center", marginBottom: 16, paddingBottom: 12, borderBottom: "1px solid var(--border)" }}
      >
        {/* `hedef` is the entry, not a numbered step (four-tab design § 2):
            the chip names the kind and links back to the picker. */}
        <NavLink
          to={stepPath("hedef")}
          className="btn"
          data-testid="strip-hedef"
          style={({ isActive }) => ({
            fontSize: 12,
            textDecoration: "none",
            border: isActive ? "1px solid var(--accent)" : "1px solid var(--border)",
            background: isActive ? "var(--bg-hover)" : "var(--bg)",
          })}
        >
          {t(kindLabelKey(session.kind))}
        </NavLink>
        {steps.slice(1).map((step, at) => (
          <NavLink
            key={step}
            to={stepPath(step)}
            className="btn"
            style={({ isActive }) => ({
              fontSize: 12,
              textDecoration: "none",
              border: isActive ? "1px solid var(--accent)" : "1px solid var(--border)",
              background: isActive ? "var(--bg-hover)" : "var(--bg)",
            })}
          >
            {at + 1}. {t(stepLabelKey(step))}
          </NavLink>
        ))}
      </nav>

      <Outlet />
      <BuildBar />
```

Imports: `NavLink` replaces `Link` from `react-router-dom` (keep `Link` if `StepHedef` still uses it), `kindLabelKey` from `@/lib/buildSteps`, `BuildBar` from `@/pages/osbuilder/BuildBar`. `NavLink` sets `aria-current="page"` on the active link, which is what Task 2's tests assert. `useLocation` may become unused in `OsBuilder()` if it was only for `here` — the drop effect still uses it; keep it.

- [ ] **Step 5: The strip check script**

In `scripts/osbuilder-strip-check.py`: `location.hash = "#/os-builder/paketler";` → `"#/os-builder/secim"`, the tag `install@paketler` → `install@secim`; the comment *"Install is the widest: four steps."* → *"Install is the widest: the hedef chip plus four numbered tabs."*; the zoom comment's *"(four steps)"* → *"(the chip plus four)"*. Nothing else.

- [ ] **Step 6: Everything green**

```powershell
pnpm vitest run src/pages/osbuilder/BuildBar.test.tsx src/pages/osbuilder/steps.test.tsx src/lib/buildSteps.test.ts
pnpm vitest run -t "have identical key sets"
pnpm lint
pnpm test
```
Expected: the three files all passed (the two strip cases from Task 2 now green); parity passed; lint clean twice; `pnpm test` all passed — quote the `Test Files` / `Tests` lines.

- [ ] **Step 7: Commit**

`<scratchpad>\msg-r1-t3.txt`:

```
Four tabs, round 1: the hedef chip, the numbered strip, and the build bar

The strip draws hedef as a chip carrying the kind's name and numbers the
four tabs from 1 (NavLink, so aria-current says which). BuildBar sits under
the outlet on the install lane: the remembered destination for the release
and a Go-to-Build button that only navigates, absent on the build tab
itself. The strip check script measures install@secim.
```

```powershell
git branch --show-current
git add src/pages/OsBuilder.tsx src/pages/osbuilder/BuildBar.tsx src/pages/osbuilder/BuildBar.test.tsx src/i18n/en.json src/i18n/tr.json scripts/osbuilder-strip-check.py
git commit -F "<scratchpad>\msg-r1-t3.txt"
```

---

### Task 4: Mutations, sweeps, docs

**Files:**
- Create: `.superpowers/sdd/2026-09-09-four-tabs/round-1-report.md`
- Modify: `docs/STATUS.md` ("Start here", in place), `docs/session-log.md` (one row), `docs/FEATURES.md` (the OS Builder wizard row), `CHANGELOG.md` (`[Unreleased]`)

- [ ] **Step 1: Mutations, each restored with `shutil.copyfile`**

Write `<scratchpad>\mutate-r1.py` with the Write tool (same shape as round A's `mutate.py`: back up by absolute path, replace exactly one occurrence, run one test by name, restore). Four mutations:

| # | File | Mutation | Test that must fail |
|---|---|---|---|
| M1 | `src/lib/buildSteps.ts` | add `"paketler"` back to the install lane's array | `gives the install job hedef and its four numbered tabs, nothing else` |
| M2 | `src/pages/osbuilder/routes.tsx` | delete the `paketler` redirect `<Route>` | `sends paketler, amiga-kurulum and ilk-acilis to the choice tab` |
| M3 | `src/pages/OsBuilder.tsx` | `steps.slice(1)` → `steps` (hedef numbered) | `shows the steps this kind has, and not the others` (the `^1\. What are we building` assertion) |
| M4 | `src/pages/osbuilder/BuildBar.tsx` | drop the `!onBuildTab &&` guard | `has no button on the build tab itself` |

Run it; `git status --short` afterwards must show no modified source file. Record each run's tail.

- [ ] **Step 2: Verification, unpiped**

```powershell
pnpm lint
pnpm test
pnpm vitest run -t "have identical key sets"
python scripts/control-byte-sweep.py
python scripts/contrast-check.py --quiet
git diff --stat main -- src-tauri
```
Expected: clean; the last prints nothing.

- [ ] **Step 3: The report** — `.superpowers/sdd/2026-09-09-four-tabs/round-1-report.md`: the rulings (from this plan's header), the retired-key grep output, which route form landed (fragment or mapped array), the mutation table with tails, the quoted verification lines, and *owed by a person*: `python scripts/osbuilder-strip-check.py` with `pnpm dev` running (both languages; the chip's width in Turkish is new), and typing `/os-builder/paketler` into the address bar on a dev build.

- [ ] **Step 4: Docs**

- `docs/STATUS.md` "Start here" item 0: after the round-A sentence add: *"**Four-tab rewrite, round 1 landed on `art-four-tabs`** (plan `docs/superpowers/plans/2026-09-09-four-tabs-round-1.md`, report `.superpowers/sdd/2026-09-09-four-tabs/round-1-report.md`): the lane is `hedef` + `dosyalar · secim · makine · derle`, the retired routes redirect, the strip has the `hedef` chip, the bar has the destination and *Derle'ye git*. In this round the tabs mount today's panels; rounds 2-4 move the content (spec § 8). Owed by a person: the strip check in both languages."* — edited in place, no new block.
- `docs/session-log.md`: one row: *"Four-tab rewrite, round 1 (spec § 8): the lane, `routes.tsx` with four redirects, the `hedef` chip, `BuildBar`. Tabs mount today's panels; nothing moves yet. Four mutations, four killed. | Vitest <files> / <tests>"* with the measured numbers.
- `docs/FEATURES.md`: find the row for the OS Builder's wizard/steps (`grep -n "sub-route\|progress strip\|stepsFor" docs/FEATURES.md`) and append: *"**2026-09-09: four tabs.** The install lane is `hedef` + `dosyalar · secim · makine · derle` (four-tab design § 2); `kaynak`, `paketler`, `amiga-kurulum`, `ilk-acilis` redirect (guard: `sends paketler, amiga-kurulum and ilk-acilis to the choice tab`). Round 1 moves no content; the row stays 🟡 until round 4."* If the row is ✅, flip it to 🟡 with that sentence, because the lane is mid-rewrite.
- `CHANGELOG.md` `[Unreleased]` → `### Changed`: *"- OS Builder: the install lane is four tabs (Amiga files · What to install · Kickstart and destination · Build); the old step URLs redirect. This release moves no content between them yet."*

- [ ] **Step 5: Commit**

```
Docs for four tabs, round 1

Four mutations, four killed. STATUS's Start here edited in place; the
FEATURES wizard row marked mid-rewrite until round 4.
```

```powershell
git branch --show-current
git add docs/STATUS.md docs/session-log.md docs/FEATURES.md CHANGELOG.md
git commit -F "<scratchpad>\msg-r1-t4.txt"
```

---

## Self-review

**Spec coverage (§ 2 and § 8 row 1):** `STEP_IDS` and `stepsFor` — Task 1; redirects as `<Navigate replace>` — Task 2; the strip numbering from the second entry with the `hedef` chip — Task 3; the bar with the destination and the *Derle'ye git* label absent on `derle` (§ 10.4) — Task 3; `readiness` keeping its three answers — Task 1; the strip check script's expectations — Task 3; `builtin.rs::route::OS_BUILDER` re-grep (§ 10.6) — add to Task 4 Step 3: `grep -rn "os-builder" src-tauri/src/core/workflow/builtin.rs` and quote it. The spec's "heading and nothing else" is corrected by the rulings at the top.

**Placeholders:** none; the two open questions (the `distro` kind label key; the React Router fragment vs. array form) each name the check and both outcomes.

**Type consistency:** `kindLabelKey(kind: BuildKind): string` in Task 1 and Task 3; `OsBuilderRoutes()` in Task 2 and in Task 2's tests; testids `tab-makine`, `tab-derle`, `build-bar`, `build-bar-destination`, `build-bar-go`, `strip-hedef` used consistently; `StepDosyalar`/`StepSecim`/`StepMakine`/`StepDerle` exported from `steps.tsx` and imported in `routes.tsx`; `StepHedef` stays in `OsBuilder.tsx` and is imported by `routes.tsx` (check for an import cycle `OsBuilder.tsx → BuildBar.tsx` / `routes.tsx → OsBuilder.tsx`: `routes.tsx` imports `OsBuilder.tsx` for `StepHedef`, and `App.tsx` imports both — no cycle, since `OsBuilder.tsx` does not import `routes.tsx`).
