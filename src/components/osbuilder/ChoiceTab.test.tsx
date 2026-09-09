// @vitest-environment jsdom
//
// Tab 2 — *Ne kurulacak* — the release's own parts, and the folded line
// saying what turning them on would replace (four-tab design § 3.2, round 3
// task 2).
//
// These are the cases that used to prove the same things through
// `OsInstall.tsx`: the component rows are the **loaded** recipe's and not a
// hardcoded list, a tick reaches the request that gets planned, a tick is
// written to the **build session** and never back to the legacy key
// (ART-290), turning off a component the condition switched on is a
// confirmation rather than a plain uncheck, and ART-175's preview of what a
// layering component would replace. They moved with the JSX; the assertions
// are the ones they arrived with.
//
// Mocked at the same boundary the rest of this suite mocks at — the
// `@/lib/*` wrappers around `invoke`, never `@tauri-apps/api` itself — and
// `@/lib/settings` one layer further down, because `useRemembered`'s setter
// fires `saveSettings` on every tick and a real one rejects in jsdom with
// nothing to catch it (ART-163's own shape).

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import i18n from "i18next";

import { changeLanguage } from "@/i18n";
import { chainLines } from "@/lib/chain";
import { useSettingsStore } from "@/stores/settingsStore";
import type {
  ChainReport,
  ChainRow,
  ChainState,
  ComponentDef,
  InstallPlan,
  InstallRelease,
  InstallRequest,
  PlanResult,
  TreeSummary,
} from "@/lib/osinstall";
import type { FirstBootPlan } from "@/lib/firstboot";
import type { RomInfo } from "@/lib/pistorm";

const componentsMock = vi.hoisted(() => vi.fn());
const layersForMock = vi.hoisted(() => vi.fn());
const planMock = vi.hoisted(() => vi.fn());
const componentCollisionsMock = vi.hoisted(() => vi.fn());
const chainMock = vi.hoisted(() => vi.fn());
const slotsMock = vi.hoisted(() => vi.fn());
const identifyRomMock = vi.hoisted(() => vi.fn());
const describeTreeMock = vi.hoisted(() => vi.fn());
const destinationTakenMock = vi.hoisted(() => vi.fn());
const firstbootPreviewMock = vi.hoisted(() => vi.fn());

vi.mock("@/lib/osinstall", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/lib/osinstall")>()),
  osinstallComponents: componentsMock,
  layersFor: layersForMock,
  osinstallPlan: planMock,
  osinstallComponentCollisions: componentCollisionsMock,
  osinstallChain: chainMock,
  osinstallSlots: slotsMock,
  // The destination's own two questions (`useDestinationCheck`): whether it
  // is an ART tree decides what the chain is asked about, and the tick is
  // never asked at all about a folder that is not one.
  osinstallDescribeTree: describeTreeMock,
  osinstallDestinationTaken: destinationTakenMock,
}));

vi.mock("@/lib/firstboot", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/lib/firstboot")>()),
  firstbootPreview: firstbootPreviewMock,
}));

vi.mock("@/lib/pistorm", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/lib/pistorm")>()),
  pistormIdentifyRom: identifyRomMock,
}));

vi.mock("@/lib/settings", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/lib/settings")>()),
  getSettings: vi.fn(),
  saveSettings: vi.fn().mockResolvedValue(undefined),
}));

const { ChoiceTab } = await import("@/components/osbuilder/ChoiceTab");
const { DEFAULT_SETTINGS } = await import("@/lib/settings");

afterEach(async () => {
  cleanup();
  await changeLanguage("en");
});

const ROM: RomInfo = {
  name: "Kickstart 3.2 (47.96)",
  version: "47",
  revision: "96",
  size_bytes: 524288,
  sha256: "a".repeat(64),
  crc32: "12345678",
  is_cloanto: false,
  key_available: false,
  is_aros: false,
  checksum: "valid",
  compatible_models: ["a1200"],
  file_path: "E:\\roms\\kick.rom",
};

/** The two shipped recipes cut down to what this tab reasons about — the
 *  same fixtures `OsInstall.test.tsx` carries, and **different lists** on
 *  purpose: the checklist is the chosen release's own recipe, never a
 *  hardcoded one. */
const COMPONENTS_32: ComponentDef[] = [
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
    id: "install-libs",
    media: "Install3.2",
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
    overrides: [],
  },
  {
    id: "modules-a1200",
    media: "ModulesA1200_3.2",
    labelKey: null,
    required: false,
    available: true,
    conditionMajor: 47,
    requiresRomMajor: null,
    exclusiveGroup: "modules",
    overrides: [],
  },
  {
    id: "keymaps",
    media: "Storage3.2",
    labelKey: null,
    required: false,
    available: true,
    conditionMajor: null,
    requiresRomMajor: null,
    exclusiveGroup: null,
    overrides: [],
  },
];

const COMPONENTS_39: ComponentDef[] = [
  {
    id: "workbench-base",
    media: "AmigaOS3.9",
    labelKey: null,
    required: true,
    available: true,
    conditionMajor: null,
    requiresRomMajor: null,
    exclusiveGroup: null,
    overrides: [],
  },
  // The component ART-175 is about: the one that makes a tree AmigaOS 3.9
  // rather than 3.5, by replacing files `workbench-base` placed.
  {
    id: "workbench-39",
    media: "AmigaOS3.9",
    labelKey: null,
    required: true,
    available: true,
    conditionMajor: null,
    requiresRomMajor: null,
    exclusiveGroup: null,
    overrides: ["workbench-base"],
  },
  {
    id: "keymaps",
    media: "AmigaOS3.9",
    labelKey: null,
    required: false,
    available: true,
    conditionMajor: null,
    requiresRomMajor: null,
    exclusiveGroup: null,
    overrides: [],
  },
];

function componentsFor(release: string): ComponentDef[] {
  return release === "AmigaOS 3.9" ? COMPONENTS_39 : COMPONENTS_32;
}

const ITEM = {
  component: "workbench-base",
  media: "Workbench3.2",
  from: "DF0:C/Format",
  to: "C/Format",
  isDir: false,
  decompress: false,
  bytes: 2 * 1024 * 1024,
  mergeIcon: false,
};

/** What `osinstallPlan` answers — items follow `chosen` for real, so a tick
 *  is visible in what comes back, exactly as it would be against the engine. */
function planResultFor(req: InstallRequest): PlanResult {
  const items = req.chosen.includes("extras") ? [ITEM, { ...ITEM, component: "extras" }] : [ITEM];
  const plan: InstallPlan = {
    release: req.release,
    items,
    refusals: [],
    totalBytes: items.reduce((sum, item) => sum + item.bytes, 0),
    totalFiles: items.length,
    // ROM is V47 here, so `modules-a1200` (conditionMajor 47) is not forced
    // on — the "condition-off" branch, not "rom-needed". For AmigaOS 3.9
    // both `workbench-base` and `workbench-39` are on without being chosen,
    // exactly as the shipped recipe has them.
    componentsOn:
      req.release === "AmigaOS 3.9"
        ? ["workbench-base", "workbench-39", ...req.chosen]
        : ["workbench-base", "install-libs", ...req.chosen],
    mediaPaths: { "Workbench3.2": "E:\\media\\Disk1.adf" },
    packages: [],
    packageMedia: {},
    userStartup: [],
    activations: [],
    mediaStamps: {},
    removals: [],
    layers: [],
  };
  return { outcome: "planned", plan };
}

/** The remembered bag, typed, for the tests that assert on storage rather
 *  than on the screen. */
function rememberedBag(): Record<string, unknown> {
  return useSettingsStore.getState().settings.remembered as Record<string, unknown>;
}

function seedRemembered(overrides: Record<string, unknown>) {
  useSettingsStore.setState({
    loaded: true,
    settings: { ...DEFAULT_SETTINGS, remembered: { ...overrides } },
  });
}

/** Set one remembered key without touching the rest — what "the user was on
 *  another release last time" looks like between two mounts of this tab,
 *  which has no release picker of its own (it is tab 1's). */
function setRemembered(key: string, value: unknown) {
  const state = useSettingsStore.getState();
  useSettingsStore.setState({
    loaded: true,
    settings: {
      ...state.settings,
      remembered: { ...(state.settings.remembered as Record<string, unknown>), [key]: value },
    },
  });
}

/**
 * Every field this tab reads, for both releases — the material folder, the
 * Kickstart and the destination, plus the two **legacy** component keys
 * seeded empty so the ART-290 assertion below has something to prove was
 * left alone.
 */
const FULL_FIELDS: Record<string, unknown> = {
  "osinstall.mediaFolder": "E:\\media",
  "osinstall.destination": "E:\\dist",
  "osinstall.chosen": [],
  "osinstall.excludedConditional": [],
  "osinstall.mediaFolder.AmigaOS 3.9": "E:\\media39",
  "osinstall.destination.AmigaOS 3.9": "E:\\dist39",
  "buildSession.rom": { path: "E:\\roms\\kick.rom" },
};

beforeEach(() => {
  componentsMock
    .mockReset()
    .mockImplementation((release: string) => Promise.resolve(componentsFor(release)));
  layersForMock.mockReset().mockResolvedValue([]);
  planMock
    .mockReset()
    .mockImplementation((req: InstallRequest) => Promise.resolve(planResultFor(req)));
  componentCollisionsMock.mockReset().mockResolvedValue({ reports: [], placed: 0, contested: 0 });
  chainMock.mockReset().mockResolvedValue({
    rows: [],
    summary: { release: "AmigaOS 3.2", total: 0, installed: 0, notNeeded: 0 },
    unreadableFolders: [],
    crowdedFolders: [],
  });
  slotsMock.mockReset().mockResolvedValue({
    states: [],
    summary: {
      release: "AmigaOS 3.2",
      requiredTotal: 0,
      requiredFound: 0,
      optionalTotal: 0,
      optionalFound: 0,
    },
    unreadableFolders: [],
    crowdedFolders: [],
  });
  identifyRomMock.mockReset().mockResolvedValue(ROM);
  // The destination is **not** a tree by default: a fresh build is the
  // ordinary case, and it is the state in which the chain is asked with no
  // manifest and the first-boot row has nothing to say about what is
  // already there.
  describeTreeMock.mockReset().mockResolvedValue({
    isTree: false,
    release: null,
    files: 0,
    components: [],
    amigaInstalled: [],
    problem: "holds no distribution.json",
  } satisfies TreeSummary);
  destinationTakenMock.mockReset().mockResolvedValue(false);
  firstbootPreviewMock.mockReset().mockResolvedValue({
    tree: "E:\\dist39",
    steps: [],
    fatMount: { kind: "available" },
    userStartupExists: false,
    alreadyWritten: false,
    bytesAdded: 4096,
  } satisfies FirstBootPlan);
  useSettingsStore.setState({ loaded: false, settings: DEFAULT_SETTINGS });
});

/** The destination *is* an ART tree — an update run rather than a fresh
 *  build, which is the state the chain reads a manifest in and the only
 *  state in which the first-boot row can say the tree already carries a
 *  block. */
function destinationIsATree() {
  describeTreeMock.mockResolvedValue({
    isTree: true,
    release: "AmigaOS 3.9",
    files: 4212,
    components: ["workbench-base", "workbench-39"],
    amigaInstalled: [],
    problem: null,
  } satisfies TreeSummary);
}

/** What `osinstallChain` answers, with `rows` in it. */
function chainOf(rows: ChainRow[]): ChainReport {
  return {
    rows,
    summary: {
      release: "AmigaOS 3.9",
      total: rows.length,
      installed: rows.filter((r) => r.state.state === "installed").length,
      notNeeded: rows.filter((r) => r.state.state === "not-needed").length,
    },
    unreadableFolders: [],
    crowdedFolders: [],
  };
}

/** One chain row, defaulted the way `src/lib/chain.test.ts`'s own builder
 *  defaults it — the same fixture shape, so the unit tests and these render
 *  tests cannot describe two different chains. */
function row(over: Partial<ChainRow> & { state: ChainState }): ChainRow {
  return {
    position: 2,
    packageId: "boingbag-39-1",
    slotId: "package:boingbag-39-1",
    name: "BoingBag 3.9-1",
    sentenceFacts: { file: null, runsOnAmiga: false },
    ...over,
  };
}

/**
 * The AmigaOS 3.9 chain as the material actually produces it: nine rows, the
 * disc first, **rank 4 twice** (`locale-39` and `locale-39-turkish` — the
 * material states no order between them, which is why `position` cannot be a
 * React key), and one row of every ending the tick has to answer for.
 */
const NINE_ROWS: ChainRow[] = [
  row({
    position: 1,
    packageId: null,
    slotId: "medium:AmigaOS3.9",
    name: "AmigaOS3.9",
    sentenceFacts: { file: "AmigaOS39.iso", runsOnAmiga: null },
    state: { state: "installed", when: null },
  }),
  row({ position: 2, state: { state: "installed", when: null } }),
  row({
    position: 3,
    packageId: "boingbag-39-2",
    slotId: "package:boingbag-39-2",
    name: "BoingBag 3.9-2",
    sentenceFacts: { file: "BoingBag39-2.lha", runsOnAmiga: false },
    state: { state: "ready" },
  }),
  row({
    position: 4,
    packageId: "locale-39",
    slotId: "package:locale-39",
    name: "Locale 3.9",
    sentenceFacts: { file: "Locale3_9.lha", runsOnAmiga: false },
    state: { state: "ready" },
  }),
  row({
    position: 4,
    packageId: "locale-39-turkish",
    slotId: "package:locale-39-turkish",
    name: "Türkçe catalogs",
    sentenceFacts: { file: null, runsOnAmiga: false },
    state: { state: "missing", expected: ["LocaleTR.lha"] },
  }),
  row({
    position: 5,
    packageId: "boingbags-39-3-4",
    slotId: "package:boingbags-39-3-4",
    name: "BoingBags 3&4",
    sentenceFacts: { file: "BoingBags34.lha", runsOnAmiga: true },
    state: { state: "not-yet-runnable", reason: "installer-not-measured" },
  }),
  row({
    position: 6,
    packageId: "euro-update",
    slotId: "package:euro-update",
    name: "Euro-Update",
    sentenceFacts: { file: null, runsOnAmiga: null },
    state: { state: "not-needed", supersededBy: "BoingBags 3&4" },
  }),
  row({
    position: 7,
    packageId: "fonts-39",
    slotId: "package:fonts-39",
    name: "Fonts 3.9",
    sentenceFacts: { file: "Fonts39.lha", runsOnAmiga: null },
    state: { state: "refused", reason: { because: "not-placeable", block: "needs-fixfonts" } },
  }),
  row({
    position: 8,
    packageId: "extras-39",
    slotId: "package:extras-39",
    name: "Extras 3.9",
    sentenceFacts: { file: "Extras39.lha", runsOnAmiga: false },
    state: {
      state: "blocked-by-component",
      components: [{ id: "locale-base", labelKey: "osinstall.components.name.os39.locale" }],
    },
  }),
];

/** One row of the updates group, by the package id it carries. */
function updateRow(id: string): HTMLElement {
  const found = screen
    .getAllByTestId("choice-update-row")
    .find((el) => el.getAttribute("data-package") === id);
  if (!found) throw new Error(`no update row for ${id}`);
  return found;
}

/** Rendered on AmigaOS 3.9 with `rows` coming back from the chain, waiting
 *  for the rows to be on screen. */
async function renderUpdates(rows: ChainRow[] = NINE_ROWS) {
  chainMock.mockResolvedValue(chainOf(rows));
  const view = await renderChoice("AmigaOS 3.9");
  await screen.findAllByTestId("choice-update-row");
  return view;
}

/** Rendered with every field set, on the given release, with the loaded
 *  catalogue on screen — the checklist arrives on its own round trip, so
 *  this waits for it rather than racing it. */
async function renderChoice(release: InstallRelease = "AmigaOS 3.2") {
  seedRemembered(
    release === "AmigaOS 3.2"
      ? FULL_FIELDS
      : { ...FULL_FIELDS, "buildSession.release": release }
  );
  const view = render(<ChoiceTab />);
  await screen.findAllByTestId("choice-part-row");
  await waitFor(() => expect(planMock).toHaveBeenCalled());
  return view;
}

/** One row of the parts group, by the component id it carries. */
function partRow(id: string): HTMLElement {
  const row = screen
    .getAllByTestId("choice-part-row")
    .find((el) => el.getAttribute("data-component") === id);
  if (!row) throw new Error(`no part row for ${id}`);
  return row;
}

describe("the parts group", () => {
  it("renders one row per component of the release's catalogue, required rows ticked and disabled", async () => {
    await renderChoice();

    expect(screen.getByTestId("choice-parts")).toBeTruthy();
    expect(screen.getAllByTestId("choice-part-row").length).toBe(COMPONENTS_32.length);

    // A required row is on and cannot be turned off, and says why in its own
    // sentence rather than leaving a disabled box unexplained.
    const base = partRow("workbench-base");
    const baseBox = within(base).getByRole("checkbox") as HTMLInputElement;
    expect(baseBox.checked).toBe(true);
    expect(baseBox.disabled).toBe(true);
    expect(base.textContent).toContain(i18n.t("osinstall.components.required"));

    // An optional row is the user's to decide.
    const extras = partRow("extras");
    const extrasBox = within(extras).getByRole("checkbox") as HTMLInputElement;
    expect(extrasBox.checked).toBe(false);
    expect(extrasBox.disabled).toBe(false);
  });

  it("reaches the request osinstallPlan is asked to plan", async () => {
    // Moved from `OsInstall.test.tsx`'s "ticking a component changes what
    // the screen will do" with the tick — the *plan section's* own half of
    // that case stayed on tab 1, which is where the plan is drawn.
    await renderChoice();

    const checkbox = screen.getByRole("checkbox", { name: "Extras3.2" }) as HTMLInputElement;
    expect(checkbox.checked).toBe(false);

    await userEvent.click(checkbox);
    expect(checkbox.checked).toBe(true);

    // The real input this tab has: the tick has to reach the request that
    // gets planned, not just flip local checkbox state.
    await waitFor(() => {
      const askedForExtras = (planMock.mock.calls as [InstallRequest][]).some(([req]) =>
        req.chosen.includes("extras")
      );
      expect(askedForExtras).toBe(true);
    });
  });

  /**
   * **ART-290.** `buildSession.components.<release>` was added with
   * `seededComponents` to migrate the panel's own two keys, and nothing ever
   * wrote it back: the session carried a one-time copy that went stale the
   * moment anybody ticked a box.
   *
   * Both halves are asserted, because either alone passes for the wrong
   * reason. That the session key gains the id says the write landed
   * somewhere; that `osinstall.chosen` is **still the empty list seeded
   * above** says the legacy key was not written as well.
   */
  it("ticks through the session, never the legacy key (ART-290)", async () => {
    await renderChoice();
    expect(rememberedBag()["buildSession.components.AmigaOS 3.2"]).toBeUndefined();

    await userEvent.click(screen.getByRole("checkbox", { name: "Extras3.2" }));

    await waitFor(() =>
      expect(rememberedBag()["buildSession.components.AmigaOS 3.2"]).toMatchObject({
        chosen: expect.arrayContaining(["extras"]),
      })
    );
    // `rememberedComponentKey` returns the bare key for AmigaOS 3.2 — "the
    // release before there was a picker" — so this is 3.2's own legacy key,
    // seeded empty above and untouched here.
    expect(rememberedBag()["osinstall.chosen"]).toEqual([]);
    expect(rememberedBag()["osinstall.chosen.AmigaOS 3.2"]).toBeUndefined();
  });

  it("asks before turning off a component the condition switched on, and turns it off only on confirm", async () => {
    // `modules-a1200` has to be *forced on by its condition* for excluding
    // it to mean anything — an unforced component is turned off by plain
    // unticking, which changes `chosen` and never populates `excluded`. This
    // is the plan a pre-V47 ROM produces, and the only state in which the
    // screen offers "turn it off anyway".
    planMock.mockImplementation((req: InstallRequest) => {
      const base = planResultFor(req);
      if (base.outcome !== "planned" || req.excluded.includes("modules-a1200")) {
        return Promise.resolve(base);
      }
      return Promise.resolve({
        ...base,
        plan: { ...base.plan, componentsOn: [...base.plan.componentsOn, "modules-a1200"] },
      } satisfies PlanResult);
    });

    await renderChoice();

    const modules = screen.getByRole("checkbox", { name: "ModulesA1200_3.2" }) as HTMLInputElement;
    await waitFor(() => expect(modules.checked).toBe(true));

    // Unticking asks rather than acting.
    await userEvent.click(modules);
    const dialog = await screen.findByTestId("choice-part-confirm-off");
    expect(dialog.textContent).toContain(
      i18n.t("osinstall.components.confirmOff.warning", { major: 47 })
    );
    expect(rememberedBag()["buildSession.components.AmigaOS 3.2"]).toBeUndefined();

    // Cancel leaves it on — a confirmation the user backed out of must not
    // half-apply.
    await userEvent.click(within(dialog).getByRole("button", { name: i18n.t("common.cancel") }));
    await waitFor(() => expect(screen.queryByTestId("choice-part-confirm-off")).toBeNull());
    expect(
      (rememberedBag()["buildSession.components.AmigaOS 3.2"] as { excludedConditional?: string[] })
        ?.excludedConditional ?? []
    ).toEqual([]);

    // Confirm writes the exclusion — to the session, like every other tick.
    await userEvent.click(modules);
    await userEvent.click(
      await screen.findByRole("button", { name: i18n.t("osinstall.components.confirmOff.confirm") })
    );
    await waitFor(() =>
      expect(rememberedBag()["buildSession.components.AmigaOS 3.2"]).toMatchObject({
        excludedConditional: expect.arrayContaining(["modules-a1200"]),
      })
    );
    await waitFor(() =>
      expect(
        (planMock.mock.calls as [InstallRequest][]).some(([req]) =>
          req.excluded.includes("modules-a1200")
        )
      ).toBe(true)
    );
  });

  it("shows the chosen release's own components, not another release's", async () => {
    // The Major finding a whole-branch review called the worst on its list:
    // a hardcoded AmigaOS 3.2 checklist rendered whatever the picker said, so
    // the user was shown one operating system's parts while ART installed
    // another's (§89). The picker is tab 1's now; this tab reads the
    // session, so the release is seeded rather than clicked.
    await renderChoice();
    expect(screen.getByRole("checkbox", { name: "Extras3.2" })).toBeTruthy();
    cleanup();

    await renderChoice("AmigaOS 3.9");
    expect(componentsMock).toHaveBeenCalledWith("AmigaOS 3.9");
    // 3.2's own components are gone, and 3.9's base component is labelled
    // with 3.9's media even though both recipes call it `workbench-base`.
    expect(screen.queryByRole("checkbox", { name: "Extras3.2" })).toBeNull();
    expect(screen.getAllByRole("checkbox", { name: /AmigaOS3\.9/ }).length).toBeGreaterThan(0);
    expect(screen.queryByRole("checkbox", { name: /Workbench3\.2/ })).toBeNull();
  });

  it("keeps each release's own ticks when the release changes and comes back", async () => {
    // "Nothing changes unless the user changes it", applied to a choice made
    // for a release the user then looked away from. The session's component
    // set is keyed per release, so 3.9 — whose recipe holds none of 3.2's
    // ids — cannot sanitize them away.
    await renderChoice();
    await userEvent.click(screen.getByRole("checkbox", { name: "Extras3.2" }));
    await waitFor(() =>
      expect(rememberedBag()["buildSession.components.AmigaOS 3.2"]).toMatchObject({
        chosen: ["extras"],
      })
    );
    cleanup();

    // The same store, one key changed — the user moved the picker on tab 1.
    setRemembered("buildSession.release", "AmigaOS 3.9");
    render(<ChoiceTab />);
    await screen.findAllByTestId("choice-part-row");
    expect(screen.queryByRole("checkbox", { name: "Extras3.2" })).toBeNull();
    cleanup();

    setRemembered("buildSession.release", "AmigaOS 3.2");
    render(<ChoiceTab />);
    const backAgain = (await screen.findByRole("checkbox", {
      name: "Extras3.2",
    })) as HTMLInputElement;
    await waitFor(() => expect(backAgain.checked).toBe(true));
  });
});

describe("what a layering component would replace (ART-175)", () => {
  // `collide::preview` has been able to answer for a release recipe's own
  // component since ART-170, and nothing asked it. These are the ask, from
  // the screen's side: the request that goes out, and the rows that come
  // back.

  it("asks about the layering components only, never the whole checklist", async () => {
    await renderChoice("AmigaOS 3.9");

    // The preview reads every file the components it is asked about would
    // place, off real install media — asking about all of them would mean
    // reading a whole AmigaOS install to answer a question about a few dozen
    // files. `workbench-base` declares no override and must not be in here.
    await waitFor(() => expect(componentCollisionsMock).toHaveBeenCalled());
    for (const [, components] of componentCollisionsMock.mock.calls as [InstallPlan, string[]][]) {
      expect(components).toEqual(["workbench-39"]);
    }
  });

  it("shows what would be replaced, and what would merely be placed", async () => {
    // Three files placed: one replaced, one already byte-for-byte identical,
    // one landing on nothing. The middle one is what review F4 was about —
    // `collide::preview` drops it, so counting `placed - reports.length` as
    // "new" would report two new files when there is one.
    componentCollisionsMock.mockResolvedValue({
      placed: 3,
      contested: 2,
      reports: [
        {
          path: "Libs/workbench.library",
          collision: { kind: "upgrade", from: "44.5", to: "45.1" },
          declared: true,
        },
      ],
    });

    await renderChoice("AmigaOS 3.9");

    // Folded, and **closed**: the count is the line, the table is behind it.
    const fold = (await screen.findByTestId("choice-replaces-fold")) as HTMLDetailsElement;
    expect(fold.open).toBe(false);
    expect(screen.getByTestId("choice-replaces-summary").textContent).toBe(
      i18n.t("osBuilder.choice.replaces", { count: 1 })
    );

    // The row itself, with the version change read off both files.
    await screen.findByText("Libs/workbench.library");
    expect(within(fold).getAllByTestId("component-collision-row").length).toBe(1);

    // And the sentence that keeps "nothing to report" from reading like
    // "nothing to place": three files placed, one of them over something.
    const summary = i18n.t("osinstall.replaces.summary", {
      components: "AmigaOS3.9",
      placed: 3,
      fresh: 1,
      unchanged: 1,
      replaced: 1,
    });
    expect(fold.textContent).toContain(summary);

    // And explicitly not the miscount: two new files, when one of the two is
    // a file that already exists byte-for-byte.
    expect(document.body.textContent).not.toContain(
      i18n.t("osinstall.replaces.summary", {
        components: "AmigaOS3.9",
        placed: 3,
        fresh: 2,
        unchanged: 0,
        replaced: 1,
      })
    );
  });

  it("counts every file it would replace, not just the first", async () => {
    componentCollisionsMock.mockResolvedValue({
      placed: 4,
      contested: 2,
      reports: [
        {
          path: "Libs/workbench.library",
          collision: { kind: "upgrade", from: "44.5", to: "45.1" },
          declared: true,
        },
        {
          path: "C/Version",
          collision: { kind: "upgrade", from: "44.1", to: "45.1" },
          declared: true,
        },
      ],
    });

    await renderChoice("AmigaOS 3.9");

    const fold = await screen.findByTestId("choice-replaces-fold");
    expect(screen.getByTestId("choice-replaces-summary").textContent).toBe(
      i18n.t("osBuilder.choice.replaces", { count: 2 })
    );
    expect(within(fold).getAllByTestId("component-collision-row").length).toBe(2);
  });

  it("says nothing at all when nothing layering is switched on", async () => {
    // AmigaOS 3.2's fixture has no component declaring an override, so the
    // fold must not appear — and the engine must not be asked either.
    await renderChoice();

    expect(screen.queryByTestId("choice-replaces-fold")).toBeNull();
    expect(componentCollisionsMock).not.toHaveBeenCalled();
  });

  it("draws no fold when the preview found nothing to replace", async () => {
    // Asked, answered, and the answer is "nothing is in the way". A fold
    // over an empty table is a control that opens onto nothing.
    await renderChoice("AmigaOS 3.9");
    await waitFor(() => expect(componentCollisionsMock).toHaveBeenCalled());

    expect(screen.queryByTestId("choice-replaces-fold")).toBeNull();
  });

  it("a preview that failed says so instead of looking like one that found nothing", async () => {
    componentCollisionsMock.mockRejectedValue(new Error("the disc could not be read"));

    await renderChoice("AmigaOS 3.9");

    // **No stray "Error: "** (ART-060). This used to render `String(e)`,
    // which on an `Error` object prepends the word "Error" to a sentence
    // that is already introduced as a failure.
    await screen.findByText(
      i18n.t("osinstall.replaces.failed", { error: "the disc could not be read" })
    );
    expect(document.querySelectorAll('[data-testid="component-collision-row"]').length).toBe(0);
  });
});

describe("the updates group", () => {
  // Group 2 of the one list (four-tab design § 3.2): `chain::rows_for`'s rows
  // in the material's own order, one tick each, and **not one sentence
  // composed here** — every word under a row is `@/lib/chain`'s, which is
  // `core::osinstall::chain`'s.
  //
  // These are the cases `PackagePanel.test.tsx` used to make about a flat
  // catalogue that never joined the chain at all (the two screens could say
  // different things about one file, ART-289's own shape). The tick is the
  // same tick — `session.packages.chosen` — and the list is now the chain's.

  it("draws the chain's rows in the material's order, rank 4 twice, with each row's own sentence", async () => {
    await renderUpdates();

    const rows = screen.getAllByTestId("choice-update-row");
    expect(rows.length).toBe(NINE_ROWS.length);
    // The order is the material's, arriving already sorted by `(position,
    // id)` in Rust and **not re-sorted here**: the order is information.
    // Both rank-4 rows are drawn, which is why `position` cannot be the key.
    expect(rows.map((el) => el.getAttribute("data-package"))).toEqual([
      "medium:AmigaOS3.9",
      "boingbag-39-1",
      "boingbag-39-2",
      "locale-39",
      "locale-39-turkish",
      "boingbags-39-3-4",
      "euro-update",
      "fonts-39",
      "extras-39",
    ]);

    // Every row carries its own ending's sentence — nine rows, and no two of
    // the eight endings collapsed into "not done".
    for (const line of chainLines(NINE_ROWS)) {
      const el = updateRow(line.id);
      const said =
        line.kind === "blocked-component"
          ? i18n.t(line.phrase.key, {
              ...line.phrase.params,
              components: i18n.t("osinstall.components.name.os39.locale"),
            })
          : i18n.t(line.phrase.key, line.phrase.params);
      expect(el.textContent, line.id).toContain(said);
    }

    // …and no raw key, and no interpolation left unfilled — the two ways a
    // sentence reaches the screen looking like a bug report.
    const group = screen.getByTestId("choice-updates");
    expect(group.textContent).not.toMatch(/osinstall\.chain\./);
    expect(group.textContent).not.toContain("{{");
  });

  it("ticks an installed row and disables it; leaves a not-yet-runnable row unticked and disabled with its sentence", async () => {
    await renderUpdates();

    // Installed: on, and not the user's to turn off — ART cannot un-install
    // it, and a tickable box would offer exactly that.
    const installed = within(updateRow("boingbag-39-1")).getByRole("checkbox") as HTMLInputElement;
    expect(installed.checked).toBe(true);
    expect(installed.disabled).toBe(true);

    // Not yet runnable: off, disabled, and **saying why** rather than a dead
    // box with nothing beside it.
    const unmeasured = within(updateRow("boingbags-39-3-4")).getByRole(
      "checkbox"
    ) as HTMLInputElement;
    expect(unmeasured.checked).toBe(false);
    expect(unmeasured.disabled).toBe(true);
    expect(updateRow("boingbags-39-3-4").textContent).toContain(
      i18n.t("osinstall.chain.notYetRunnable.installerNotMeasured", { name: "BoingBags 3&4" })
    );

    // And the three the run would refuse for a reason of its own are off and
    // disabled too, each with its own sentence — never one shared "cannot".
    for (const [id, key, name] of [
      ["fonts-39", "osinstall.chain.refusedNotPlaceable.needsFixfonts", "Fonts 3.9"],
      ["euro-update", "osinstall.chain.notNeeded", "Euro-Update"],
    ] as const) {
      const box = within(updateRow(id)).getByRole("checkbox") as HTMLInputElement;
      expect(box.checked, id).toBe(false);
      expect(box.disabled, id).toBe(true);
      expect(updateRow(id).textContent, id).toContain(
        i18n.t(key, { name, supersededBy: "BoingBags 3&4" })
      );
    }
    const gone = within(updateRow("locale-39-turkish")).getByRole("checkbox") as HTMLInputElement;
    expect(gone.checked).toBe(false);
    expect(gone.disabled).toBe(true);
    expect(updateRow("locale-39-turkish").textContent).toContain(
      i18n.t("osinstall.chain.missing", { name: "Türkçe catalogs", filenames: "LocaleTR.lha" })
    );
  });

  it("never offers a row that runs on the Amiga — one route in the wizard", async () => {
    // A `ready` row whose only route is the emulator. It is the state that
    // would otherwise draw a tick this wizard cannot honour: there is no
    // route control on this tab, and pressing Build would have to either run
    // an emulator nobody asked for or silently skip the row.
    await renderUpdates([
      row({
        position: 3,
        packageId: "boingbags-39-3-4",
        slotId: "package:boingbags-39-3-4",
        name: "BoingBags 3&4",
        sentenceFacts: { file: "BoingBags34.lha", runsOnAmiga: true },
        state: { state: "ready" },
      }),
    ]);

    const box = within(updateRow("boingbags-39-3-4")).getByRole("checkbox") as HTMLInputElement;
    expect(box.checked).toBe(false);
    expect(box.disabled).toBe(true);
    // The row still says where it would happen, which is the fact that makes
    // the dead box make sense.
    expect(updateRow("boingbags-39-3-4").textContent).toContain(i18n.t("osinstall.chain.onAmiga"));
  });

  it("ticks through session.packages.chosen and reads a remembered tick back", async () => {
    await renderUpdates();
    expect(rememberedBag()["buildSession.packages.AmigaOS 3.9"]).toBeUndefined();

    const ready = within(updateRow("boingbag-39-2")).getByRole("checkbox") as HTMLInputElement;
    expect(ready.disabled).toBe(false);
    await userEvent.click(ready);

    // The session's own key, and the components' key untouched by it: two
    // groups on one tab writing one key would be ART-290 again.
    await waitFor(() =>
      expect(rememberedBag()["buildSession.packages.AmigaOS 3.9"]).toMatchObject({
        chosen: ["boingbag-39-2"],
      })
    );
    expect(rememberedBag()["buildSession.components.AmigaOS 3.9"]).toBeUndefined();
    cleanup();

    // Remembered, and read back on the next mount rather than reset.
    chainMock.mockResolvedValue(chainOf(NINE_ROWS));
    render(<ChoiceTab />);
    await screen.findAllByTestId("choice-update-row");
    const again = within(updateRow("boingbag-39-2")).getByRole("checkbox") as HTMLInputElement;
    expect(again.checked).toBe(true);

    // And unticking takes it out again — a remembered pick that cannot be
    // dropped is `PackagePanel`'s own F3.
    await userEvent.click(again);
    await waitFor(() =>
      expect(rememberedBag()["buildSession.packages.AmigaOS 3.9"]).toMatchObject({ chosen: [] })
    );
  });

  it("shows a remembered id that is not a row, untickable, and lets it be unticked", async () => {
    // `PackagePanel`'s N4, kept: an id from an older ART, or a package since
    // removed from the release, used to render no row at all — invisible,
    // and so impossible to clear, since there was no box to click.
    seedRemembered({
      ...FULL_FIELDS,
      "buildSession.release": "AmigaOS 3.9",
      "buildSession.packages.AmigaOS 3.9": { folder: null, chosen: ["a-package-nobody-ships"] },
    });
    chainMock.mockResolvedValue(chainOf(NINE_ROWS));
    render(<ChoiceTab />);
    await screen.findAllByTestId("choice-update-row");

    const stale = await screen.findByTestId("choice-update-unknown");
    expect(stale.textContent).toContain(
      i18n.t("osBuilder.choice.notInList", { id: "a-package-nobody-ships" })
    );

    await userEvent.click(
      within(stale).getByRole("button", { name: i18n.t("osBuilder.choice.untick") })
    );
    await waitFor(() =>
      expect(rememberedBag()["buildSession.packages.AmigaOS 3.9"]).toMatchObject({ chosen: [] })
    );
    expect(screen.queryByTestId("choice-update-unknown")).toBeNull();
  });

  it("draws no updates group for a release without a chain", async () => {
    // AmigaOS 3.2 has none (spec § 1.5). An empty heading over an empty list
    // would say a release has updates and none of them apply.
    await renderChoice();
    await waitFor(() => expect(chainMock).toHaveBeenCalled());

    expect(screen.queryByTestId("choice-updates")).toBeNull();
    expect(screen.queryAllByTestId("choice-update-row")).toHaveLength(0);
  });

  it("asks the chain with the readout's overrides, so both say the same about one file", async () => {
    // **ART-284/ART-289.** The user names the file for a slot on tab 1
    // (`amigaInstall.archive.<pkg>`); `slotOverrides` is what turns those
    // keys into the `(slot, path)` pairs `osinstall_chain` takes. Without
    // them this list called the owner's own BoingBag 1 build ambiguous while
    // the readout one tab over said *the file you chose* — one resolver, two
    // callers, one of them not handed the decision.
    seedRemembered({
      ...FULL_FIELDS,
      "buildSession.release": "AmigaOS 3.9",
      "amigaInstall.archive.boingbag-39-1": "E:\\archives\\BoingBag39-1.lha",
      "amigaInstall.archive.locale-39": "E:\\archives\\Locale3_9.lha",
    });
    chainMock.mockResolvedValue(chainOf(NINE_ROWS));
    destinationIsATree();
    render(<ChoiceTab />);
    await screen.findAllByTestId("choice-update-row");

    await waitFor(() =>
      expect(chainMock).toHaveBeenCalledWith(
        "AmigaOS 3.9",
        expect.arrayContaining(["E:\\media39"]),
        // The destination is an ART tree, so the chain is asked *against it*
        // — which is what makes an installed row installed.
        "E:\\dist39",
        "E:\\roms\\kick.rom",
        [
          ["package:boingbag-39-1", "E:\\archives\\BoingBag39-1.lha"],
          ["package:locale-39", "E:\\archives\\Locale3_9.lha"],
        ]
      )
    );
  });

  it("asks the chain about no tree when the destination is not one", async () => {
    // A fresh build. Handing a folder that is not a tree to the chain would
    // be refused — every *installed* state comes from `distribution.json`
    // alone — and the whole list would go missing rather than reading as the
    // fresh build it is.
    await renderUpdates();
    await waitFor(() => expect(chainMock).toHaveBeenCalled());
    for (const call of chainMock.mock.calls as [string, string[], string | null][]) {
      expect(call[2]).toBeNull();
    }
  });
});

describe("the first-boot tick", () => {
  it("is ticked when nothing is remembered, and rendering writes nothing", async () => {
    // **Absent means ticked** (design § 3.2): the first-boot block is what
    // makes a PiStorm tree boot its own hardware. And the default is not
    // *written*: a screen that stores its own default the moment it is drawn
    // makes "the user chose this" and "ART chose this" the same value.
    await renderChoice("AmigaOS 3.9");

    const box = within(screen.getByTestId("choice-firstboot")).getByRole(
      "checkbox"
    ) as HTMLInputElement;
    expect(box.checked).toBe(true);
    expect(screen.getByTestId("choice-firstboot").textContent).toContain(
      i18n.t("osBuilder.choice.firstbootRow")
    );
    expect(rememberedBag()["buildSession.firstboot"]).toBeUndefined();
  });

  it("unticking writes wanted=false and it survives a reload", async () => {
    await renderChoice("AmigaOS 3.9");
    await userEvent.click(within(screen.getByTestId("choice-firstboot")).getByRole("checkbox"));

    await waitFor(() =>
      expect(rememberedBag()["buildSession.firstboot"]).toMatchObject({ wanted: false })
    );
    cleanup();

    render(<ChoiceTab />);
    await screen.findAllByTestId("choice-part-row");
    const again = within(screen.getByTestId("choice-firstboot")).getByRole(
      "checkbox"
    ) as HTMLInputElement;
    expect(again.checked).toBe(false);
  });

  it("says the tree already carries a block when it does", async () => {
    // A fact about the folder, not about the tick: `written` and `wanted`
    // are different questions, and the row says the first one where it is
    // true so a second write is a decision rather than a surprise.
    destinationIsATree();
    firstbootPreviewMock.mockResolvedValue({
      tree: "E:\\dist39",
      steps: [],
      fatMount: { kind: "available" },
      userStartupExists: true,
      alreadyWritten: true,
      bytesAdded: 4096,
    } satisfies FirstBootPlan);

    await renderChoice("AmigaOS 3.9");

    const said = await screen.findByTestId("choice-firstboot-already-written");
    expect(said.textContent).toBe(i18n.t("firstboot.panel.alreadyWritten"));
  });

  it("asks nothing about a destination that is not an ART tree", async () => {
    // `firstboot_preview` reads a tree. Asking it about a folder that is not
    // one produces a refusal ART would then have to explain on a row whose
    // whole content is a tick.
    await renderChoice("AmigaOS 3.9");
    await waitFor(() => expect(chainMock).toHaveBeenCalled());

    expect(firstbootPreviewMock).not.toHaveBeenCalled();
    expect(screen.queryByTestId("choice-firstboot-already-written")).toBeNull();
  });
});

describe("in Turkish", () => {
  it("renders no raw key and no unfilled interpolation anywhere on the tab", async () => {
    // ART-062's standing gap, asked of the whole tab at once: a key present
    // in `en.json` and missing from `tr.json` renders the dotted key, and a
    // parameter the Turkish sentence does not name renders `{{…}}`. Neither
    // fails to compile and neither is visible to `parity.test.ts`, which
    // compares the catalogues to each other rather than to a screen.
    await changeLanguage("tr");
    destinationIsATree();
    await renderUpdates();

    const tab = screen.getByTestId("choice-tab");
    expect(tab.textContent).not.toMatch(/osinstall\.|osBuilder\./);
    expect(tab.textContent).not.toContain("{{");
  });
});
