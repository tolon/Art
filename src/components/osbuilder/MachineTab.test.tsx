// @vitest-environment jsdom
//
// Tab 3 — the Kickstart and the destination (four-tab design § 3.3).
//
// Six of these cases are **moved** from `OsInstall.test.tsx` by name on
// 2026-09-09, unchanged except for what they render: the three Amiga Forever
// ROM-folder cases and the three ART-241 `describedBy` cases. They were never
// about the install screen — they were about the Kickstart field, and the
// field is here now. The five destination cases are new: the field crossed
// over with two sentences it never had on tab 1 (occupied, and what ART found
// there), and a sentence with no test is a sentence nobody has read.
//
// Mocked at the same boundary `OsInstall.test.tsx` mocks at — the `@/lib/*`
// wrappers around `invoke`, not `@tauri-apps/api` itself. `@/lib/settings` is
// mocked one layer further down for that file's own reason: `useRemembered`
// writes through `useSettingsStore.update()`, whose `saveSettings` is a real
// `tauri-plugin-store` IPC call that rejects unhandled in jsdom.

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import i18n from "i18next";

import { changeLanguage } from "@/i18n";
import { useSettingsStore } from "@/stores/settingsStore";

const identifyRomMock = vi.hoisted(() => vi.fn());
const dialogOpenMock = vi.hoisted(() => vi.fn());
const amigaForeverMock = vi.hoisted(() => vi.fn());
const takenMock = vi.hoisted(() => vi.fn());
const describeTreeMock = vi.hoisted(() => vi.fn());
// Round 4 task 4: the keymap select's options are the **plan's** own items,
// so this tab computes a plan and these three answer for it.
const componentsMock = vi.hoisted(() => vi.fn());
const layersForMock = vi.hoisted(() => vi.fn());
const planMock = vi.hoisted(() => vi.fn());
const componentCollisionsMock = vi.hoisted(() => vi.fn());

// Only the two the destination asks — everything else in `@/lib/osinstall`
// this tab uses (`rememberedComponentKey`) is pure and stays real.
vi.mock("@/lib/osinstall", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/lib/osinstall")>()),
  osinstallDestinationTaken: takenMock,
  osinstallDescribeTree: describeTreeMock,
  osinstallComponents: componentsMock,
  layersFor: layersForMock,
  osinstallPlan: planMock,
  osinstallComponentCollisions: componentCollisionsMock,
}));

vi.mock("@/lib/pistorm", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/lib/pistorm")>()),
  pistormIdentifyRom: identifyRomMock,
}));

vi.mock("@tauri-apps/plugin-dialog", () => ({
  open: dialogOpenMock,
}));

// The Amiga Forever offer (design § 3.5) asks the host on mount. The default
// is a machine that does not have it, so no test gets an offer line it did
// not ask for.
vi.mock("@/lib/api", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/lib/api")>()),
  hostAmigaForeverFolders: amigaForeverMock,
}));

vi.mock("@/lib/settings", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/lib/settings")>()),
  getSettings: vi.fn(),
  saveSettings: vi.fn().mockResolvedValue(undefined),
}));

const { MachineTab } = await import("@/components/osbuilder/MachineTab");
const { DEFAULT_SETTINGS } = await import("@/lib/settings");
type InstallRequest = import("@/lib/osinstall").InstallRequest;
type PlanResult = import("@/lib/osinstall").PlanResult;
type ComponentDef = import("@/lib/osinstall").ComponentDef;

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

// ART-241: the ROM field's Browse button used to sit beside its own
// identified/unreadable paragraph with nothing wiring the two together — a
// screen reader user tabbing to Browse heard only its own name and had to
// go hunting forward in the page for whether the ROM they had already
// chosen was recognised at all.
function describedByIds(el: Element): string[] {
  return (el.getAttribute("aria-describedby") ?? "").split(/\s+/).filter(Boolean);
}

const AF_ROM = "E:\\amiga\\Shared\\rom";
const NOT_TREE = {
  isTree: false,
  release: null,
  files: 0,
  components: [],
  amigaInstalled: [],
  problem: "no distribution.json",
};
const TREE = {
  isTree: true,
  release: "AmigaOS 3.9",
  files: 1915,
  components: ["workbench-base"],
  amigaInstalled: [],
  problem: null,
};

/** Two components, cut down to what the keymap select needs: the base and
 *  the one that places `Devs/Keymaps`. */
const COMPONENTS: ComponentDef[] = [
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

/** ART-226: the picker's options are read off the plan's own items, so a
 *  plan that places keymaps is what makes the section appear at all. */
function planResultFor(req: InstallRequest): PlanResult {
  const items = [ITEM];
  if (req.chosen.includes("keymaps")) {
    items.push(
      { ...ITEM, component: "keymaps", to: "Devs/Keymaps/türkçe" },
      { ...ITEM, component: "keymaps", to: "Devs/Keymaps/türkçe.info" },
      { ...ITEM, component: "keymaps", to: "Devs/Keymaps/usa" }
    );
  }
  return {
    outcome: "planned",
    plan: {
      release: req.release,
      items,
      refusals: [],
      totalBytes: items.reduce((sum, item) => sum + item.bytes, 0),
      totalFiles: items.length,
      componentsOn: ["workbench-base", ...req.chosen],
      mediaPaths: { "Workbench3.2": "E:\\media\\Disk1.adf" },
      packages: [],
      packageMedia: {},
      userStartup: [],
      activations: [],
      mediaStamps: {},
      removals: [],
      layers: [],
    },
  };
}

/** What the plan needs to be asked at all — a material folder and a
 *  destination, per release. */
const PLAN_FIELDS: Record<string, unknown> = {
  "buildSession.release": "AmigaOS 3.9",
  "buildSession.material.AmigaOS 3.9": { folders: [{ path: "E:\\media", layer: null }] },
  "buildSession.material.AmigaOS 3.2": { folders: [{ path: "E:\\media", layer: null }] },
  "osinstall.destination.AmigaOS 3.9": "E:\\dist39",
  "osinstall.chosen.AmigaOS 3.9": ["keymaps"],
  "osinstall.chosen.AmigaOS 3.2": ["keymaps"],
  "buildSession.components.AmigaOS 3.9": { chosen: ["keymaps"], excludedConditional: [] },
  "buildSession.components.AmigaOS 3.2": { chosen: ["keymaps"], excludedConditional: [] },
};

beforeEach(() => {
  amigaForeverMock.mockReset().mockResolvedValue({ adf: null, rom: null });
  identifyRomMock.mockReset().mockResolvedValue({ name: "Kickstart 3.2 (47.96)" });
  dialogOpenMock.mockReset();
  takenMock.mockReset().mockResolvedValue(false);
  describeTreeMock.mockReset().mockResolvedValue(NOT_TREE);
  componentsMock.mockReset().mockResolvedValue(COMPONENTS);
  layersForMock.mockReset().mockResolvedValue([]);
  planMock
    .mockReset()
    .mockImplementation((req: InstallRequest) => Promise.resolve(planResultFor(req)));
  componentCollisionsMock.mockReset().mockResolvedValue({ reports: [], placed: 0, contested: 0 });
});

afterEach(async () => {
  cleanup();
  useSettingsStore.setState({ loaded: false, settings: DEFAULT_SETTINGS });
  // Never let a Turkish test bleed its language into the next test file's
  // first render.
  await changeLanguage("en");
});

// Moved from OsInstall.test.tsx by name on 2026-09-09 (four-tab design § 3.3).
describe("Amiga Forever's ROM folder, offered and never chosen", () => {
  it("offers the ROM folder for the Kickstart field, on its own dismissal", async () => {
    amigaForeverMock.mockResolvedValue({ adf: "E:\\amiga\\Shared\\adf", rom: AF_ROM });
    seedRemembered({ "buildSession.release": "AmigaOS 3.9" });
    render(<MachineTab />);
    const offer = await screen.findByTestId("amiga-forever-rom-offer");
    expect(offer.textContent).toContain(AF_ROM);
    // Nothing is chosen by the line existing.
    expect(rememberedBag()["buildSession.rom"]).toBeUndefined();
    await userEvent.click(
      within(offer).getByRole("button", { name: i18n.t("osinstall.material.amigaForeverDismiss") })
    );
    await waitFor(() => expect(screen.queryByTestId("amiga-forever-rom-offer")).toBeNull());
  });

  it("opens the Kickstart picker on that folder, and the user still picks the file", async () => {
    amigaForeverMock.mockResolvedValue({ adf: null, rom: AF_ROM });
    seedRemembered({ "buildSession.release": "AmigaOS 3.9" });
    render(<MachineTab />);
    const offer = await screen.findByTestId("amiga-forever-rom-offer");
    dialogOpenMock.mockResolvedValueOnce("E:\\amiga\\Shared\\rom\\amiga-os-310-a1200.rom");
    await userEvent.click(
      within(offer).getByRole("button", { name: i18n.t("osinstall.material.amigaForeverAdd") })
    );
    // The picker was opened **on** the folder, not handed a file.
    await waitFor(() => expect(dialogOpenMock).toHaveBeenCalled());
    expect(dialogOpenMock.mock.calls.at(-1)![0]).toMatchObject({ defaultPath: AF_ROM });
    await waitFor(() =>
      expect(rememberedBag()["buildSession.rom"]).toMatchObject({
        path: "E:\\amiga\\Shared\\rom\\amiga-os-310-a1200.rom",
      })
    );
    expect(screen.queryByTestId("amiga-forever-rom-offer")).toBeNull();
  });

  it("says nothing about ROMs when a Kickstart is already chosen", async () => {
    amigaForeverMock.mockResolvedValue({ adf: null, rom: AF_ROM });
    seedRemembered({
      "buildSession.release": "AmigaOS 3.9",
      "buildSession.rom": { path: "E:\\rom\\kick.rom" },
    });
    render(<MachineTab />);
    await waitFor(() => expect(amigaForeverMock).toHaveBeenCalled());
    expect(screen.queryByTestId("amiga-forever-rom-offer")).toBeNull();
  });
});

// Moved from OsInstall.test.tsx by name.
describe("ART-241: the ROM field's Browse button is described by its own outcome paragraph", () => {
  it("names the identified paragraph once the ROM resolves", async () => {
    seedRemembered({
      "buildSession.release": "AmigaOS 3.9",
      "buildSession.rom": { path: "E:\\rom\\kick.rom" },
    });
    render(<MachineTab />);
    const field = screen.getByTestId("osinstall-rom-field");
    const button = within(field).getByRole("button", { name: i18n.t("common.browse") });
    const identified = await waitFor(() => {
      const el = document.getElementById("osinstall-rom-identified");
      if (!el) throw new Error("not rendered yet");
      return el;
    });
    expect(identified.textContent).toContain("Kickstart 3.2 (47.96)");
    // Also carries the hint's own id (the ROM's `Field` sets a `hint` too) —
    // the point is that the outcome paragraph's id is *among* them, not that
    // it is the only one.
    expect(describedByIds(button)).toContain("osinstall-rom-identified");
    expect(describedByIds(button)).not.toContain("osinstall-rom-unreadable");
  });

  it("names the unreadable paragraph when the ROM fails to identify, not the identified one", async () => {
    identifyRomMock.mockReset().mockRejectedValue(new Error("not a Kickstart"));
    seedRemembered({
      "buildSession.release": "AmigaOS 3.9",
      "buildSession.rom": { path: "E:\\rom\\junk.bin" },
    });
    render(<MachineTab />);
    const field = screen.getByTestId("osinstall-rom-field");
    const button = within(field).getByRole("button", { name: i18n.t("common.browse") });
    await screen.findByText(i18n.t("osinstall.rom.unreadable"));
    expect(describedByIds(button)).toContain("osinstall-rom-unreadable");
    expect(describedByIds(button)).not.toContain("osinstall-rom-identified");
    expect(screen.queryByText(/Kickstart 3\.2 \(47\.96\)/)).toBeNull();
  });

  it("names neither outcome paragraph before any ROM has been chosen", () => {
    seedRemembered({ "buildSession.release": "AmigaOS 3.9" });
    render(<MachineTab />);
    const field = screen.getByTestId("osinstall-rom-field");
    const button = within(field).getByRole("button", { name: i18n.t("common.browse") });
    expect(describedByIds(button)).not.toContain("osinstall-rom-identified");
    expect(describedByIds(button)).not.toContain("osinstall-rom-unreadable");
  });
});

describe("the destination (four-tab design § 3.3)", () => {
  it("shows the remembered folder for the release, and writes nothing by rendering", () => {
    seedRemembered({
      "buildSession.release": "AmigaOS 3.9",
      "osinstall.destination.AmigaOS 3.9": "E:\\out\\dist",
    });
    render(<MachineTab />);
    expect(screen.getByTestId("osinstall-destination-field").textContent).toContain("E:\\out\\dist");
    // **Nothing changes unless the user changes it.** Asserted against the
    // bag rather than the screen: a render that wrote the value straight back
    // under a second key would look identical on screen.
    const keys = Object.keys(rememberedBag());
    expect(keys.filter((k) => k.startsWith("osinstall.destination"))).toEqual([
      "osinstall.destination.AmigaOS 3.9",
    ]);
  });

  it("says an empty folder gets a fresh tree", async () => {
    seedRemembered({
      "buildSession.release": "AmigaOS 3.9",
      "osinstall.destination.AmigaOS 3.9": "E:\\out\\empty",
    });
    render(<MachineTab />);
    expect((await screen.findByTestId("osinstall-destination-fresh")).textContent).toBe(
      i18n.t("osinstall.destination.fresh")
    );
    expect(screen.queryByTestId("osinstall-destination-taken")).toBeNull();
    expect(screen.queryByTestId("osinstall-destination-tree")).toBeNull();
  });

  it("refuses an occupied folder in its own sentence, and names a tree ART built apart from it", async () => {
    takenMock.mockResolvedValue(true);
    describeTreeMock.mockResolvedValue(TREE);
    seedRemembered({
      "buildSession.release": "AmigaOS 3.9",
      "osinstall.destination.AmigaOS 3.9": "E:\\out\\dist",
    });
    render(<MachineTab />);
    expect((await screen.findByTestId("osinstall-destination-taken")).textContent).toBe(
      i18n.t("osinstall.destination.taken")
    );
    expect((await screen.findByTestId("osinstall-destination-tree")).textContent).toBe(
      i18n.t("osinstall.destination.tree", { release: "AmigaOS 3.9", count: 1915 })
    );
    expect(screen.queryByTestId("osinstall-destination-fresh")).toBeNull();
  });

  /**
   * **An occupied folder that is nobody's tree is still occupied.** The case
   * above pairs `taken` with a tree ART built, and `!tree.isTree` alone is
   * enough to keep the fresh line off the screen there — so it never
   * exercised the `!taken` half of that guard (found by mutation M3,
   * 2026-09-09). The ordinary occupied folder is somebody's own files: not a
   * tree, and refused. Told "this folder gets a fresh tree" beside "ART
   * refuses this folder", the screen would be contradicting the core it just
   * asked, which is the §89 defect and not a cosmetic one — the fresh line
   * is the sentence that says the run will go ahead.
   */
  it("does not promise a fresh tree in the folder it has just refused", async () => {
    takenMock.mockResolvedValue(true);
    describeTreeMock.mockResolvedValue(NOT_TREE);
    seedRemembered({
      "buildSession.release": "AmigaOS 3.9",
      "osinstall.destination.AmigaOS 3.9": "E:\\out\\mine",
    });
    render(<MachineTab />);
    expect((await screen.findByTestId("osinstall-destination-taken")).textContent).toBe(
      i18n.t("osinstall.destination.taken")
    );
    expect(screen.queryByTestId("osinstall-destination-fresh")).toBeNull();
    expect(screen.queryByTestId("osinstall-destination-tree")).toBeNull();
  });

  it("picks a folder through the dialog and remembers it for this release only", async () => {
    seedRemembered({ "buildSession.release": "AmigaOS 3.9" });
    render(<MachineTab />);
    dialogOpenMock.mockResolvedValueOnce("E:\\out\\new");
    const field = screen.getByTestId("osinstall-destination-field");
    await userEvent.click(within(field).getByRole("button", { name: i18n.t("common.browse") }));
    await waitFor(() =>
      expect(rememberedBag()["osinstall.destination.AmigaOS 3.9"]).toBe("E:\\out\\new")
    );
    expect(rememberedBag()["osinstall.destination.AmigaOS 3.2"]).toBeUndefined();
  });

  it("renders no raw key in Turkish", async () => {
    await changeLanguage("tr");
    seedRemembered({
      "buildSession.release": "AmigaOS 3.9",
      "osinstall.destination.AmigaOS 3.9": "E:\\out\\empty",
    });
    render(<MachineTab />);
    const tab = screen.getByTestId("machine-tab");
    await screen.findByTestId("osinstall-destination-fresh");
    expect(tab.textContent).not.toMatch(/osinstall\.|osBuilder\./);
    expect(tab.textContent).not.toContain("{{");
  });
});

// ---------------------------------------------------------------------------
// ART-226's other half: choosing the keyboard, having placed it
// ---------------------------------------------------------------------------
//
// **Moved from `OsInstall.test.tsx` by name** on 2026-09-09 (four-tab design
// § 3.3, round 4 task 4). The five cases arrive with the assertions they had;
// what changed is what they render and how the release is switched — this tab
// has no release picker of its own, so the fifth case moves the session's own
// key between two mounts, which is what switching a release actually does to
// this tab.
//
// The owner built a Turkish tree, watched its menus render ç ü ş Ğ, and then
// could not type them: every keymap was placed and none was selected. The
// picker's options are read off the plan's own items, so a layout it offers
// cannot then be refused for not being there.

describe("choosing the keyboard the system boots with", () => {
  it("says nothing when the install places no keymaps", async () => {
    seedRemembered({
      ...PLAN_FIELDS,
      "buildSession.components.AmigaOS 3.9": { chosen: [], excludedConditional: [] },
    });
    render(<MachineTab />);
    await screen.findByTestId("osinstall-destination-fresh");
    // The plan has landed and placed no keymap, so there is nothing to pick.
    await waitFor(() => expect(planMock).toHaveBeenCalled());
    expect(screen.queryByTestId("keymap-section")).toBeNull();
  });

  it("offers what the plan will really place, and not the icons", async () => {
    seedRemembered(PLAN_FIELDS);
    render(<MachineTab />);
    await screen.findByTestId("keymap-section");

    const picker = screen.getByRole("combobox", { name: /keyboard layout/i });
    const options = [...picker.querySelectorAll("option")].map((o) => o.getAttribute("value"));
    // The empty one is "leave it to Kickstart".
    expect(options).toContain("");
    expect(options).toContain("türkçe");
    expect(options).toContain("usa");
    expect(options).not.toContain("türkçe.info");
  });

  it("sends the chosen layout to the planner", async () => {
    seedRemembered(PLAN_FIELDS);
    render(<MachineTab />);
    await screen.findByTestId("keymap-section");

    await userEvent.selectOptions(
      screen.getByRole("combobox", { name: /keyboard layout/i }),
      "türkçe"
    );

    // Does the control itself move?
    expect(
      (screen.getByRole("combobox", { name: /keyboard layout/i }) as HTMLSelectElement).value
    ).toBe("türkçe");
    // The last plan the screen asked for is the one it is showing, and it has
    // to carry the choice - otherwise the picker moves and nothing follows it.
    await waitFor(() => {
      const sent = planMock.mock.calls.at(-1)![0] as InstallRequest;
      expect(sent.keymap).toBe("türkçe");
    });
  });

  /// **No default.** Every ART tree until now booted on the ROM's `usa`, and
  /// choosing somebody's keyboard for them is not ART's to do.
  it("sends null when nothing is chosen, leaving it to Kickstart", async () => {
    seedRemembered(PLAN_FIELDS);
    render(<MachineTab />);
    await screen.findByTestId("keymap-section");

    const sent = planMock.mock.calls.at(-1)![0] as InstallRequest;
    expect(sent.keymap).toBeNull();
  });

  /// **A control that disappears under somebody's hand is the same defect as
  /// one that answers wrongly.** `effectivePlan` goes `null` when a plan
  /// answer fails, and reading the option list straight off it took the whole
  /// section away — including on the user's own choice of keyboard, which is
  /// what re-plans here. The list is held across a re-plan and only a plan
  /// that *arrives* supersedes it.
  it("keeps the list on screen when the next plan does not arrive", async () => {
    seedRemembered(PLAN_FIELDS);
    render(<MachineTab />);
    await screen.findByTestId("keymap-section");

    // The next plan answer is a failure — which is the state `effectivePlan`
    // is null in — and choosing a layout is what asks for it.
    planMock.mockRejectedValue(new Error("the disc changed"));
    await userEvent.selectOptions(
      screen.getByRole("combobox", { name: /keyboard layout/i }),
      "türkçe"
    );
    await waitFor(() => expect(planMock.mock.calls.length).toBeGreaterThan(1));

    // Still there, still offering what the last real plan placed.
    await waitFor(() => expect(screen.getByTestId("keymap-section")).toBeTruthy());
    const picker = screen.getByRole("combobox", { name: /keyboard layout/i });
    expect([...picker.querySelectorAll("option")].map((o) => o.getAttribute("value"))).toContain(
      "usa"
    );
  });

  /// Per release, like the media folder (ART-207): a layout is a name in
  /// *that* release's `Devs/Keymaps`.
  it("remembers the choice per release", async () => {
    seedRemembered({
      ...PLAN_FIELDS,
      // AmigaOS 3.2 keeps the unsuffixed key it had before there was a
      // picker (`rememberedComponentKey`), so these two really are the two
      // releases' own slots.
      "osinstall.keymap.AmigaOS 3.9": "türkçe",
      "osinstall.keymap": "",
    });
    const view = render(<MachineTab />);
    await screen.findByTestId("keymap-section");
    expect(
      (screen.getByRole("combobox", { name: /keyboard layout/i }) as HTMLSelectElement).value
    ).toBe("türkçe");

    // The release changes on tab 1; this tab reads it out of the session.
    // Remounted rather than re-seeded, so the assertion is about the stored
    // per-release keys and not about a fresh bag.
    view.unmount();
    const state = useSettingsStore.getState();
    useSettingsStore.setState({
      loaded: true,
      settings: {
        ...state.settings,
        remembered: {
          ...(state.settings.remembered as Record<string, unknown>),
          "buildSession.release": "AmigaOS 3.2",
        },
      },
    });
    render(<MachineTab />);
    await screen.findByTestId("keymap-section");
    await waitFor(() =>
      expect(
        (screen.getByRole("combobox", { name: /keyboard layout/i }) as HTMLSelectElement).value
      ).toBe("")
    );
  });
});
