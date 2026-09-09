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
import { useSettingsStore } from "@/stores/settingsStore";
import type {
  ComponentDef,
  InstallPlan,
  InstallRelease,
  InstallRequest,
  PlanResult,
} from "@/lib/osinstall";
import type { RomInfo } from "@/lib/pistorm";

const componentsMock = vi.hoisted(() => vi.fn());
const layersForMock = vi.hoisted(() => vi.fn());
const planMock = vi.hoisted(() => vi.fn());
const componentCollisionsMock = vi.hoisted(() => vi.fn());
const chainMock = vi.hoisted(() => vi.fn());
const slotsMock = vi.hoisted(() => vi.fn());
const identifyRomMock = vi.hoisted(() => vi.fn());

vi.mock("@/lib/osinstall", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/lib/osinstall")>()),
  osinstallComponents: componentsMock,
  layersFor: layersForMock,
  osinstallPlan: planMock,
  osinstallComponentCollisions: componentCollisionsMock,
  // Not read by this tab in round 3 — the updates group is task 3 — but
  // mocked at the same boundary as everything else so adding it cannot
  // start a real round trip in jsdom.
  osinstallChain: chainMock,
  osinstallSlots: slotsMock,
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
  useSettingsStore.setState({ loaded: false, settings: DEFAULT_SETTINGS });
});

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
