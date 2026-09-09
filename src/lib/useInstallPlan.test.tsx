// @vitest-environment jsdom
//
// The plan, on its own, away from the screen that used to hold it.
//
// `useInstallPlan` is the whole of what `OsInstall.tsx` used to compute
// inline: the release's layers, its component catalogue, one or two
// `osinstall_plan` round trips, and the collision preview for whichever
// layering components the plan switched on. Two tabs read it now (round 3 of
// the four-tab rewrite), so the rules it carries — ART-119's one call when
// nothing is excluded, ART-178's stable dependencies, ART-089's refusal to
// write a tick set against a catalogue that has not arrived — are tested
// here, once, rather than through a 3900-line component test.
//
// Mocked at the same boundary the rest of this suite mocks at: the
// `@/lib/osinstall` wrappers around `invoke`, never `@tauri-apps/api`
// itself. `sanitizeChosen` / `pruneStaleExclusions` / `foldersForPlan` stay
// **real** — they are the logic under test's own reasoning, and stubbing
// them would leave the sanitize and prune assertions below proving nothing.

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, renderHook, waitFor } from "@testing-library/react";

// The hook calls `useTranslation()` for `errorText`; importing the app's own
// i18n initialises the singleton synchronously (see `src/i18n/index.ts`).
import "@/i18n";
import type { MaterialChoice } from "@/lib/buildSession";
import type { ComponentDef, InstallPlan, InstallRequest, PlanResult } from "@/lib/osinstall";

const layersForMock = vi.hoisted(() => vi.fn());
const componentsMock = vi.hoisted(() => vi.fn());
const planMock = vi.hoisted(() => vi.fn());
const collisionsMock = vi.hoisted(() => vi.fn());

vi.mock("@/lib/osinstall", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/lib/osinstall")>()),
  layersFor: layersForMock,
  osinstallComponents: componentsMock,
  osinstallPlan: planMock,
  osinstallComponentCollisions: collisionsMock,
}));

const { useInstallPlan } = await import("@/lib/useInstallPlan");

// --- fixtures --------------------------------------------------------------

/** One folder, untagged — every shipped release but AmigaOS 3.2.2 is
 *  unlayered, so this is the ordinary material list. A module constant, not
 *  an inline literal: `foldersForPlan` is memoized on this identity, and a
 *  fresh object per render is ART-178's own defect. */
const MATERIAL: MaterialChoice = { folders: [{ path: "E:\\media", layer: null }] };
const NO_MATERIAL: MaterialChoice = { folders: [] };

/** `workbench-base` required and plain; `extras` optional and **layering** —
 *  its `overrides` is what makes the collision preview a question at all. */
const CATALOGUE: ComponentDef[] = [
  {
    id: "workbench-base",
    media: "Workbench3.2",
    labelKey: null,
    required: true,
    available: true,
    conditionMajor: null,
    requiresRomMajor: null,
    exclusiveGroup: null,
    overrides: [],
  },
  {
    id: "extras",
    media: "Extras3.2",
    labelKey: null,
    required: false,
    available: true,
    conditionMajor: null,
    requiresRomMajor: null,
    exclusiveGroup: null,
    overrides: ["workbench-base"],
  },
];

/** A planned answer, every required field of `InstallPlan` filled. `mark` is
 *  what tells one fixture from another in an assertion — `totalFiles` is a
 *  number the hook never touches, so it can carry an identity without
 *  changing any behaviour under test. */
function planned(componentsOn: string[], mark: number): PlanResult {
  const plan: InstallPlan = {
    release: "AmigaOS 3.2",
    items: [],
    refusals: [],
    totalBytes: 0,
    totalFiles: mark,
    componentsOn,
    activations: [],
    mediaStamps: {},
    mediaPaths: {},
    packages: [],
    packageMedia: {},
    userStartup: [],
    removals: [],
    layers: [],
  };
  return { outcome: "planned", plan };
}

const PLAN_BASE = planned(["workbench-base"], 1);
const PLAN_LAYERING = planned(["workbench-base", "extras"], 2);

/** A promise this test resolves by hand — what "a plan still in flight"
 *  means, without a timer to race. */
function deferred<T>(): { promise: Promise<T>; resolve: (value: T) => void } {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((res) => {
    resolve = res;
  });
  return { promise, resolve };
}

interface Props {
  material: MaterialChoice;
  keymap: string;
  chosen: string[];
  excludedConditional: string[];
}

const setComponents = vi.fn();

function renderPlan(overrides: Partial<Props> = {}) {
  const initialProps: Props = {
    material: MATERIAL,
    keymap: "",
    chosen: [],
    excludedConditional: [],
    ...overrides,
  };
  return renderHook(
    (props: Props) =>
      useInstallPlan({
        release: "AmigaOS 3.2",
        material: props.material,
        keymap: props.keymap,
        rom: "E:\\roms\\kick.rom",
        destination: "E:\\dist",
        reuseScan: true,
        rescanNonce: 0,
        components: {
          chosen: props.chosen,
          excludedConditional: props.excludedConditional,
        },
        setComponents,
      }),
    { initialProps }
  );
}

beforeEach(() => {
  layersForMock.mockReset().mockResolvedValue([]);
  componentsMock.mockReset().mockResolvedValue(CATALOGUE);
  planMock.mockReset().mockResolvedValue(PLAN_BASE);
  collisionsMock.mockReset().mockResolvedValue({ reports: [], placed: 0, contested: 0 });
  setComponents.mockReset();
});

afterEach(() => {
  cleanup();
});

/** One microtask flush inside `act`, so a promise that settled after the
 *  last assertion has been handled before the next one reads state. */
async function flush() {
  await act(async () => {
    await Promise.resolve();
    await Promise.resolve();
  });
}

/**
 * Rendered, with the mount's own round trips finished.
 *
 * The layers and the catalogue arrive separately and **both are plan
 * dependencies**, so a mount plans more than once by design. Every test that
 * counts work therefore counts from here, after the last of them has landed
 * — otherwise it is measuring the mount rather than the change it made.
 */
async function settled(overrides: Partial<Props> = {}) {
  const view = renderPlan(overrides);
  await waitFor(() => expect(view.result.current.catalogue).not.toBeNull());
  await waitFor(() => expect(view.result.current.effectivePlanResult).not.toBeNull());
  await flush();
  return view;
}

describe("useInstallPlan", () => {
  it("plans nothing without a folder", async () => {
    const { result } = renderPlan({ material: NO_MATERIAL });
    // The catalogue is still fetched — it is the release's own recipe, not
    // an answer about the material — so waiting on it proves the effects ran
    // rather than that nothing has happened yet.
    await waitFor(() => expect(result.current.catalogue).toEqual(CATALOGUE));
    expect(planMock).not.toHaveBeenCalled();
    expect(result.current.basePlanResult).toBeNull();
    expect(result.current.effectivePlanResult).toBeNull();
    expect(result.current.basePlan).toBeNull();
    expect(result.current.effectivePlan).toBeNull();
  });

  it("plans once when nothing is excluded, twice when something is", async () => {
    // ART-119 (#1): with an empty `excluded` the two requests are identical,
    // so the second call planned the same media twice and threw one answer
    // away — real work, on every keystroke.
    //
    // **Counted per answer, from a settled hook.** The catalogue and the
    // layers arrive on their own round trips and both are plan dependencies,
    // so the mount legitimately plans more than once; what this measures is
    // how many requests **one** change costs.
    const { result, rerender } = await settled();
    planMock.mockClear();
    rerender({ material: MATERIAL, keymap: "türkçe", chosen: [], excludedConditional: [] });
    await waitFor(() => expect(planMock).toHaveBeenCalledTimes(1));
    await flush();
    expect(planMock).toHaveBeenCalledTimes(1);
    expect((planMock.mock.calls[0][0] as InstallRequest).excluded).toEqual([]);
    // The one answer is given to both, rather than one of them being null.
    expect(result.current.basePlanResult).toBe(result.current.effectivePlanResult);

    planMock.mockClear();
    rerender({
      material: MATERIAL,
      keymap: "türkçe",
      chosen: [],
      excludedConditional: ["extras"],
    });
    await waitFor(() => expect(planMock).toHaveBeenCalledTimes(2));
    const excludedIn = (planMock.mock.calls as [InstallRequest][]).map(([req]) => req.excluded);
    expect(excludedIn).toContainEqual([]);
    expect(excludedIn).toContainEqual(["extras"]);
  });

  it("sanitizes a stale tick through the session setter, never by dropping it silently", async () => {
    // A remembered id this release's recipe does not hold actually clears,
    // rather than being filtered again on every read — and it clears in the
    // **session**, which is ART-290: the panel keys were seeded from and
    // never written back to.
    const { result } = renderPlan({ chosen: ["gone"] });
    await waitFor(() => expect(result.current.catalogue).toEqual(CATALOGUE));
    await waitFor(() => expect(setComponents).toHaveBeenCalledWith({ chosen: [] }));
    // What was actually planned is the sanitized list, not the stale one.
    await waitFor(() => expect(planMock).toHaveBeenCalled());
    expect((planMock.mock.calls.at(-1)![0] as InstallRequest).chosen).toEqual([]);
  });

  it("prunes a stale exclusion once the base plan has landed", async () => {
    // `extras` is excluded but nothing forces it on — its condition does not
    // hold — so keeping the id would silently reapply an override the user
    // never confirmed for *this* pairing.
    const { result } = renderPlan({ excludedConditional: ["extras"] });
    await waitFor(() => expect(result.current.basePlan).not.toBeNull());
    await waitFor(() =>
      expect(setComponents).toHaveBeenCalledWith({ excludedConditional: [] })
    );
  });

  it("does not write a tick set while the catalogue has not arrived (ART-089)", async () => {
    // Against `null` the remembered ids pass through untouched and nothing is
    // written back: dropping every id because a fetch has not landed yet is a
    // setting changing without the user changing it.
    componentsMock.mockReset().mockReturnValue(deferred<ComponentDef[]>().promise);
    const { result } = renderPlan({ chosen: ["gone"] });
    await waitFor(() => expect(result.current.effectivePlanResult).not.toBeNull());
    await flush();
    expect(result.current.catalogue).toBeNull();
    expect(setComponents).not.toHaveBeenCalled();
    // ...and the stale id was still sent, rather than being dropped for the
    // request while being kept on disk — one answer, not two.
    expect((planMock.mock.calls.at(-1)![0] as InstallRequest).chosen).toEqual(["gone"]);
  });

  it("drops a superseded plan", async () => {
    const first = deferred<PlanResult>();
    planMock.mockReset().mockReturnValueOnce(first.promise).mockResolvedValue(PLAN_LAYERING);
    const { result, rerender } = renderPlan();
    await waitFor(() => expect(planMock).toHaveBeenCalledTimes(1));

    rerender({ material: MATERIAL, keymap: "türkçe", chosen: [], excludedConditional: [] });
    await waitFor(() => expect(result.current.effectivePlan?.totalFiles).toBe(2));

    // The first request lands late. It describes a keymap the user has moved
    // away from, so it may not reach the screen.
    first.resolve(PLAN_BASE);
    await flush();
    expect(result.current.effectivePlan?.totalFiles).toBe(2);
  });

  it("asks for component collisions only for layering components that are on", async () => {
    const { result, rerender } = renderPlan();
    await waitFor(() => expect(result.current.effectivePlan).not.toBeNull());
    await flush();
    // `workbench-base` declares no `overrides`, so there is nothing in
    // anybody's way and nothing to ask about.
    expect(result.current.layeringOn).toEqual([]);
    expect(collisionsMock).not.toHaveBeenCalled();
    expect(result.current.componentPreview).toBeNull();

    planMock.mockResolvedValue(PLAN_LAYERING);
    rerender({ material: MATERIAL, keymap: "türkçe", chosen: [], excludedConditional: [] });
    await waitFor(() => expect(result.current.layeringOn).toEqual(["extras"]));
    // The plan the preview is asked about is the one on screen, not a
    // separately-shaped copy of it.
    await waitFor(() =>
      expect(collisionsMock).toHaveBeenCalledWith(result.current.effectivePlan, ["extras"])
    );
    await waitFor(() => expect(result.current.componentPreview).not.toBeNull());
  });

  it("bumps planVersion on every answer, success or failure", async () => {
    const { result, rerender } = await settled();
    // A success is an answer, and it counted.
    const afterSuccess = result.current.planVersion;
    expect(afterSuccess).toBeGreaterThan(0);

    planMock.mockReset().mockRejectedValue(new Error("the disc could not be read"));
    rerender({ material: MATERIAL, keymap: "türkçe", chosen: [], excludedConditional: [] });
    await waitFor(() => expect(result.current.planVersion).toBe(afterSuccess + 1));
    // A failure is an answer, and it says so — a confirmation held about the
    // plan that was on screen is stale either way.
    expect(result.current.planError).toContain("the disc could not be read");
    expect(result.current.effectivePlanResult).toBeNull();
  });
});
