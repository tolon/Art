// @vitest-environment jsdom
//
// ART-118: the OS Builder's install screen has never once been seen
// rendering past its five `h2` headings. A headless-Chrome probe (Chrome and
// Edge, headless and headed) reproducibly crashed the renderer
// (`-1073741819`, an access violation) the moment anything past the
// headings was touched — filling the media/ROM/destination fields, ticking
// a component, reading the confirmation or refusals card, running Verify.
// A browser cannot see this screen right now. jsdom can, so this is the
// first automated coverage of `OsInstall.tsx` — not a proxy harness the way
// `FileManagerFilter.test.tsx` and `useRomPairing.test.tsx` are, because the
// whole point here is proving the *real* component mounts, not a stand-in
// that always would have.
//
// Mocked at the same boundary the rest of this test suite mocks at — the
// `@/lib/*` wrappers around `invoke`/`listen`, not `@tauri-apps/api` itself
// (see `useRomPairing.test.tsx`). `@/lib/settings` is mocked too, one layer
// further down than usual: `useRemembered` (which this screen leans on for
// every field and for the component checklist) goes through
// `useSettingsStore`, whose `update()` calls `@/lib/settings`'s
// `saveSettings` — the actual `tauri-plugin-store` IPC boundary — on every
// tick. Left real, that call rejects in jsdom with nothing to catch it
// (`useRemembered`'s setter fires and forgets the promise), which Vitest
// counts as an unhandled rejection and fails the run. `getSettings` is
// mocked for the same reason, though nothing here calls it — this screen
// never calls `load()`, it only ever writes.
//
// What this establishes, one test group per requirement:
//   1. The screen mounts past its headings, with its real controls present.
//   2 & 3. Nothing on screen is a raw i18n key or an unrendered
//      `{{interpolation}}` — in English, and (ART-062, never checked before)
//      in Turkish, whose strings run measurably longer than the English
//      originals.
//   4. Ticking a component in the checklist — the screen's real input —
//      changes the request `osinstallPlan` is asked to plan, and what the
//      plan section shows.
//   5. A refusal renders as an actual sentence, not a blank card.
//
// What this does NOT establish: the access violation itself. jsdom does no
// layout at all, so it cannot reproduce a native renderer crash or measure
// overflow — ART-062's "does a long Turkish string actually fit" is still a
// real-screen job, and a human `pnpm tauri dev` pass over this screen is
// still owed. See the narrowed ART-118 entry in `docs/ISSUES.md`.

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import i18n from "i18next";

import { changeLanguage } from "@/i18n";
import { useSettingsStore } from "@/stores/settingsStore";
import type {
  ComponentDef,
  InstallLayer,
  InstallPlan,
  InstallRelease,
  InstallRequest,
  MediaIdentification,
  MediaRow,
  MediaScanResult,
  OsInstallResult,
  PlanItem,
  PlanResult,
  RefusalReason,
} from "@/lib/osinstall";
import type { RomInfo } from "@/lib/pistorm";

const scanMediaMock = vi.hoisted(() => vi.fn());
const componentsMock = vi.hoisted(() => vi.fn());
const layersForMock = vi.hoisted(() => vi.fn());
const layerForMediaMock = vi.hoisted(() => vi.fn());
const planMock = vi.hoisted(() => vi.fn());
const componentCollisionsMock = vi.hoisted(() => vi.fn());
const applyMock = vi.hoisted(() => vi.fn());
const verifyMock = vi.hoisted(() => vi.fn());
const onResultMock = vi.hoisted(() => vi.fn());
const identifyRomMock = vi.hoisted(() => vi.fn());
const dialogOpenMock = vi.hoisted(() => vi.fn());
const onJobProgressMock = vi.hoisted(() => vi.fn());
const rescanMock = vi.hoisted(() => vi.fn());
const releaseForMediaMock = vi.hoisted(() => vi.fn());
const mediaEvidenceMock = vi.hoisted(() => vi.fn());
const packagesMock = vi.hoisted(() => vi.fn());
const identifyMediaMock = vi.hoisted(() => vi.fn());
const slotsMock = vi.hoisted(() => vi.fn());
const amigaForeverMock = vi.hoisted(() => vi.fn());
const chainMock = vi.hoisted(() => vi.fn());

vi.mock("@/lib/osinstall", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/lib/osinstall")>()),
  osinstallScanMedia: scanMediaMock,
  osinstallComponents: componentsMock,
  layersFor: layersForMock,
  layerForMedia: layerForMediaMock,
  osinstallPlan: planMock,
  osinstallRescanMedia: rescanMock,
  osinstallReleaseForMedia: releaseForMediaMock,
  osinstallMediaEvidence: mediaEvidenceMock,
  osinstallIdentifyMedia: identifyMediaMock,
  osinstallSlots: slotsMock,
  // `AmigaInstallPanel` asks for the chain on mount (round 3, task 2).
  // Mocked at the same boundary as everything else here; the default is a
  // release ART knows no chain for, which is what this screen's own
  // fixtures are.
  osinstallChain: chainMock,
  osinstallPackages: packagesMock,
  osinstallComponentCollisions: componentCollisionsMock,
  osinstallApply: applyMock,
  osinstallVerify: verifyMock,
  onOsInstallResult: onResultMock,
}));

// `PackagePanel` (Task 7) is now mounted inside this screen, and both it and
// this screen's own install job subscribe to `onJobProgress` on mount — real
// `listen()` has no Tauri IPC bridge to reach in jsdom and rejects, which
// Vitest counts as an unhandled rejection at teardown (ART-163's own shape).
// Mocked here for the same reason `osinstallPlan`/`osinstallApply` above are.
vi.mock("@/lib/jobs", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/lib/jobs")>()),
  onJobProgress: onJobProgressMock,
}));

vi.mock("@/lib/pistorm", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/lib/pistorm")>()),
  pistormIdentifyRom: identifyRomMock,
}));

vi.mock("@tauri-apps/plugin-dialog", () => ({
  open: dialogOpenMock,
}));

// The Amiga Forever offer (design § 3.5) asks the host on mount. Mocked at
// the same `@/lib/*` boundary as everything else here; the default is a
// machine that does not have it, so no test gets an offer line it did not
// ask for.
vi.mock("@/lib/api", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/lib/api")>()),
  hostAmigaForeverFolders: amigaForeverMock,
}));

// The one real Tauri IPC boundary `useRemembered` reaches on every tick
// (see the module comment above) — mocked so a checkbox click never tries a
// real `tauri-plugin-store` round trip.
vi.mock("@/lib/settings", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/lib/settings")>()),
  getSettings: vi.fn(),
  saveSettings: vi.fn().mockResolvedValue(undefined),
}));

const { OsInstall, resetIfEmpty } = await import("@/components/osbuilder/OsInstall");
const { refusalPhrase } = await import("@/lib/osinstall");
const { DEFAULT_SETTINGS } = await import("@/lib/settings");

afterEach(async () => {
  cleanup();
  // Never let a Turkish test bleed its language into the next test file's
  // first render.
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

/**
 * What `osinstallComponents` answers, per release — the two shipped recipes
 * cut down to what this screen actually reasons about.
 *
 * Two things here are the point rather than the setup. The lists **differ**,
 * and both carry a `workbench-base` naming **different media**: that is the
 * defect the checklist used to have, where a hardcoded AmigaOS 3.2
 * catalogue was rendered whatever the picker said, so 3.9 showed 26
 * components for a one-component recipe and labelled its base component
 * `Workbench3.2`.
 */
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
  // ART-226. The shipped 3.2 recipe has carried a `keymaps` component since
  // 2026-08-24; this fixture was behind it, which is what made the keyboard
  // picker vanish on the second plan - `sanitizeChosen` pruned an id the
  // catalogue did not know.
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
  // rather than 3.5, by replacing files `workbench-base` placed. Its
  // `overrides` is what the screen keys the preview off, and the shipped
  // recipe really declares `["workbench-base", "locale-base"]` for it —
  // pinned by `src/lib/osinstall.test.ts`'s recipe-parity test.
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
  // ART-226. Both shipped recipes carry a `keymaps` component since
  // 2026-08-24, and this fixture was behind the real one until the keyboard
  // picker's own test needed 3.9 to have it too.
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

/**
 * The layers `osinstall_layers` answers for AmigaOS 3.2.2 — Task 10's own
 * subject. Every other shipped release answers empty, which is what makes
 * "an unlayered release renders exactly what it renders today" a real test
 * rather than an assumption (`layersFor` below).
 */
const LAYERS_322: InstallLayer[] = [
  { id: "base", labelKey: "osinstall.layer.base32" },
  { id: "update-3.2.2", labelKey: "osinstall.layer.update322" },
];

function layersForRelease(release: string): InstallLayer[] {
  return release === "AmigaOS 3.2.2" ? LAYERS_322 : [];
}

/** A layer's own field label, resolved the same way `OsInstall.tsx` resolves
 *  it — `t(labelKey)`, exact text, for asserting a wrong-layer hint names the
 *  right field (`layer-field-${id}` is how a test finds the field itself,
 *  since ART-237 moved its accessible name off the wrapping div). */
function layerFieldLabel(layerId: string): string {
  const layer = LAYERS_322.find((l) => l.id === layerId);
  return layer?.labelKey ? i18n.t(layer.labelKey) : layerId;
}

const ITEM_WORKBENCH: PlanItem = {
  component: "workbench-base",
  media: "Workbench3.2",
  from: "DF0:C/Format",
  to: "C/Format",
  isDir: false,
  decompress: false,
  bytes: 2 * 1024 * 1024,
  mergeIcon: false,
};

const ITEM_EXTRAS: PlanItem = {
  component: "extras",
  media: "Extras3.2",
  from: "DF0:Tools/HDToolBox",
  to: "Tools/HDToolBox",
  isDir: false,
  decompress: false,
  bytes: 3 * 1024 * 1024,
  mergeIcon: false,
};

/** The plan `osinstallPlan` would answer for a given request — items follow
 *  `chosen` for real, so ticking "extras" is visible in what comes back,
 *  the same way it would be against the real engine. */
function planResultFor(req: InstallRequest): PlanResult {
  const items = req.chosen.includes("extras") ? [ITEM_WORKBENCH, ITEM_EXTRAS] : [ITEM_WORKBENCH];
  // ART-226: the keyboard picker's options are read off the plan's own items,
  // so a plan that places keymaps is what makes the section appear at all.
  if (req.chosen.includes("keymaps")) {
    items.push(
      { ...ITEM_WORKBENCH, component: "keymaps", to: "Devs/Keymaps/türkçe" },
      { ...ITEM_WORKBENCH, component: "keymaps", to: "Devs/Keymaps/türkçe.info" },
      { ...ITEM_WORKBENCH, component: "keymaps", to: "Devs/Keymaps/usa" }
    );
  }
  return {
    outcome: "planned",
    plan: {
      release: req.release,
      items,
      refusals: [],
      totalBytes: items.reduce((sum, item) => sum + item.bytes, 0),
      totalFiles: items.filter((item) => !item.isDir).length,
      // ROM is V47 here, so "modules-a1200" (conditionMajor: 47) is not
      // forced on — the "condition-off" reasoning branch, not "rom-needed".
      // Release-aware, because ART-175's whole subject is a component that
      // is switched on *by the recipe* and layers over another: for AmigaOS
      // 3.9 both `workbench-base` and `workbench-39` are on without being
      // chosen, exactly as the shipped recipe has them.
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
    },
  };
}

const REFUSAL: RefusalReason = {
  refusal: "media-missing",
  component: "workbench-base",
  volume_name: "Workbench3.2",
};

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

/**
 * Every field set — for **both** releases, which is what "every field" means
 * since ART-207 keyed the media folder and the destination per release the
 * way `chosen` already was. The bare keys are AmigaOS 3.2's (see
 * `rememberedComponentKey`: 3.2 is "the release before there was a picker"),
 * the suffixed ones are 3.9's, and they hold **different paths** on purpose —
 * a test that switches release can then tell which one the screen is using
 * instead of watching one shared value stay put.
 *
 * The ROM is deliberately shared: a Kickstart belongs to the machine being
 * built for, not to the release being installed.
 */
const FULL_FIELDS = {
  "osinstall.mediaFolder": "E:\\media",
  "osinstall.rom": "E:\\roms\\kick.rom",
  "osinstall.destination": "E:\\dist",
  "osinstall.chosen": [],
  "osinstall.excludedConditional": [],
  "osinstall.mediaFolder.AmigaOS 3.9": "E:\\media39",
  "osinstall.destination.AmigaOS 3.9": "E:\\dist39",
};

beforeEach(() => {
  scanMediaMock.mockReset().mockResolvedValue({
    outcome: "found",
    media: [{ path: "E:\\media\\Disk1.adf", volumeName: "Workbench3.2", kind: "floppy" }],
  } satisfies MediaScanResult);
  componentsMock
    .mockReset()
    .mockImplementation((release: string) => Promise.resolve(componentsFor(release)));
  layersForMock
    .mockReset()
    .mockImplementation((release: string) => Promise.resolve(layersForRelease(release)));
  // The honest default: nothing found in a layer's own folder identifies as
  // a *different* layer of the same release, so no test gets a wrong-layer
  // hint it did not ask for. Tests for the hint itself (Task 10 fix round,
  // Finding 1) override this per test.
  layerForMediaMock.mockReset().mockResolvedValue(null);
  planMock.mockReset().mockImplementation((req: InstallRequest) => Promise.resolve(planResultFor(req)));
  // Nothing layering is switched on for AmigaOS 3.2, so this is never called
  // in most tests; an empty preview is the honest default for the ones where
  // it is.
  componentCollisionsMock.mockReset().mockResolvedValue({ reports: [], placed: 0 });
  applyMock.mockReset().mockResolvedValue(1);
  verifyMock.mockReset();
  onResultMock.mockReset().mockResolvedValue(() => {});
  identifyRomMock.mockReset().mockResolvedValue(ROM);
  dialogOpenMock.mockReset().mockResolvedValue(null);
  onJobProgressMock.mockReset().mockResolvedValue(() => {});
  rescanMock.mockReset().mockResolvedValue(1);
  releaseForMediaMock.mockReset().mockResolvedValue(null);
  // ART-253. `wrongMediaFolder`'s claim is checked against the chosen
  // release's own recipe rather than inferred, so this screen asks Rust what
  // the folder holds of it. The default is the ordinary state of the
  // fixtures below — a folder holding this release's own `Workbench3.2` —
  // and the tests about a *wrong* folder override it, which is the whole
  // distinction the command exists to draw.
  mediaEvidenceMock.mockReset().mockResolvedValue({
    release: "AmigaOS 3.2",
    distinguishing: ["Workbench3.2"],
    shared: [],
    missingRequired: ["Install3.2"],
  });
  packagesMock.mockReset().mockResolvedValue([]);
  chainMock.mockReset().mockResolvedValue({
    rows: [],
    summary: { release: "AmigaOS 3.2", total: 0, installed: 0, notNeeded: 0 },
    unreadableFolders: [],
    crowdedFolders: [],
  });
  // The material readout's own round trip. The honest default is an empty
  // release: no slots, nothing found, nothing unreadable — so the readout
  // renders its heading and an all-zero set line and takes nothing away from
  // any other test on this screen.
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
  amigaForeverMock.mockReset().mockResolvedValue({ adf: null, rom: null });
  // The honest default for the content-hash pass: it ran, it read the one
  // disk the media scan above reports, and no row in the table claims it.
  // **A miss is the default on purpose** — every other test in this file
  // therefore renders with the hash lines saying "not in the table", which
  // is the state that must take nothing away from anything else on screen.
  identifyMediaMock.mockReset().mockResolvedValue({
    matches: [
      {
        path: "E:\\media\\Disk1.adf",
        volumeName: "Workbench3.2",
        row: null,
        md5: "0".repeat(32),
        confirmed: null,
      },
    ],
    unreadable: [],
    hashed: 1,
    remembered: 0,
    skipped: [],
  } satisfies MediaIdentification);
  useSettingsStore.setState({ loaded: false, settings: DEFAULT_SETTINGS });
});

/** Media folder, ROM and destination all set — the state the browser probe
 *  could never reach — then waits for the live plan preview to land. */
async function renderFull() {
  seedRemembered(FULL_FIELDS);
  const utils = render(<OsInstall />);
  await waitFor(() => expect(planMock).toHaveBeenCalled());
  await screen.findByText(i18n.t("osinstall.plan.heading"));
  // The checklist is loaded, not hardcoded, so it arrives on its own round
  // trip — wait for it rather than racing it.
  await screen.findByRole("checkbox", { name: "Extras3.2" });
  return utils;
}

/**
 * Render, optionally starting on a chosen release — seeded directly into the
 * build session's own remembered key, the way `FULL_FIELDS` already seeds
 * AmigaOS 3.9's fields, rather than a picker click every caller would
 * otherwise repeat. `"AmigaOS 3.2"` is the session's own default, so it is
 * seeded like every other test that never mentions a release.
 */
function renderOsInstall(options: { release?: InstallRelease } = {}) {
  seedRemembered(
    options.release && options.release !== "AmigaOS 3.2"
      ? { "buildSession.release": options.release }
      : {}
  );
  return render(<OsInstall />);
}

/** Add one folder through the screen's single folder picker (design § 3.1).
 *  There is one Add button now, not one Browse per field, so every test that
 *  used to point a field at a folder goes through here. */
async function addFolder(path: string) {
  dialogOpenMock.mockResolvedValueOnce(path);
  await userEvent.click(await screen.findByTestId("material-add-folder"));
  // The path is on screen more than once once the packages panel below reads
  // the same list, so `findAllByText` — what this waits for is the row
  // having landed, not how many places show it.
  await screen.findAllByText(path);
}

/** Say which part of a layered release a folder in the list holds — the
 *  labelled question the per-layer fields used to ask, attached to the folder
 *  rather than to a field. Found by the select's own accessible name, which
 *  names the folder (ART-240's rule: several rows, several controls, and a
 *  screen reader user must be able to tell them apart). */
async function tagFolder(path: string, layerId: string) {
  const select = await screen.findByRole("combobox", {
    name: i18n.t("osinstall.material.layerAriaLabel", { folder: path }),
  });
  await userEvent.selectOptions(select, layerId);
}

/** Add a folder and tag it with the layer it holds — what pointing a
 *  per-layer field at a folder used to be. */
async function browseLayerFolder(layerId: string, path: string) {
  await addFolder(path);
  await tagFolder(path, layerId);
}

/** Renders on AmigaOS 3.2.2, browses every named layer's own folder in turn,
 *  and answers the request `osinstallPlan` was actually sent. */
async function planWithFolders(folders: Record<string, string>): Promise<InstallRequest> {
  renderOsInstall({ release: "AmigaOS 3.2.2" });
  for (const [layerId, path] of Object.entries(folders)) {
    await browseLayerFolder(layerId, path);
  }
  await waitFor(() => expect(planMock).toHaveBeenCalled());
  return planMock.mock.calls.at(-1)![0] as InstallRequest;
}

describe("OsInstall renders past its headings", () => {
  it("mounts with the real media, checklist and action controls", async () => {
    await renderFull();

    // The five headings a browser probe once confirmed are not the news
    // here — everything below them, which the probe never survived to see,
    // is. The Kickstart and destination rows left this screen for tab 3 on
    // 2026-09-09 (`MachineTab.test.tsx` mounts them there); the checkbox
    // count below is unchanged by that, because both were `Field` rows.
    expect(screen.getByText(i18n.t("osinstall.material.label"))).toBeTruthy();

    // The component checklist is the screen's real input (requirement 4) —
    // one row per component of the release's own loaded recipe. `+ 2` are the
    // tickboxes that are not components: the run card's confirmation and the
    // media section's "reuse the last scan" (ART-194). `PackagePanel`'s
    // confirmation is not among them — it renders only once a package has been
    // ticked, and nothing here ticks one.
    //
    // **`AmigaInstallPanel`'s own is not among them either, and that changed
    // in round 3.** This fixture is AmigaOS 3.2, whose catalogue answers with
    // no runnable package at all, so that panel renders no form — and its
    // emulator confirmation is part of the form. It used to render on its own
    // under the "nothing runnable for this release" sentence, which is
    // ART-212's own complaint one control further on: a confirmation for a
    // run that cannot be configured.
    const checkboxes = screen.getAllByRole("checkbox");
    expect(checkboxes.length).toBe(COMPONENTS_32.length + 2);
    expect(
      screen.getByRole("checkbox", { name: i18n.t("osinstall.media.reuseScan") })
    ).toBeTruthy();

    expect(screen.getByRole("button", { name: i18n.t("osinstall.run.run") })).toBeTruthy();
    // **Verify is deliberately not here** (ART-197 wave 3). It compares a
    // tree against a volume that already exists, so everything it needs is on
    // the volumes step and nothing it needs is on this one. `steps.test.tsx`
    // is where it is now checked for.
    expect(screen.queryByRole("button", { name: i18n.t("osinstall.verify.run") })).toBeNull();
  });
});

// A string that is *only* dot-separated identifier segments, three or more
// of them (two-plus dots) — the shape of an i18next key rendered raw
// (`"osinstall.media.label"`) when the lookup failed. Anchored end to end so
// an ordinary sentence containing a version-numbered name like
// "Workbench3.2" (one dot) never matches: real prose has spaces, and no
// sentence in either catalogue is itself an unbroken run of identifiers and
// dots.
const KEY_SHAPE = /^[a-zA-Z][a-zA-Z0-9]*(\.[a-zA-Z][a-zA-Z0-9]*){2,}$/;

/** Every rendered text node that looks like a raw i18next key, or that
 *  still carries a literal `{{` — i18next's own rendering of a missing
 *  interpolation variable. Walking text nodes individually (rather than
 *  `container.textContent`) is what keeps this from false-positiving on
 *  text that only looks suspicious once two elements' text is concatenated. */
function rawI18nArtifacts(container: HTMLElement): string[] {
  const walker = document.createTreeWalker(container, NodeFilter.SHOW_TEXT);
  const offenders: string[] = [];
  let node: Node | null;
  while ((node = walker.nextNode())) {
    const text = node.textContent?.trim() ?? "";
    if (!text) continue;
    if (KEY_SHAPE.test(text) || text.includes("{{")) offenders.push(text);
  }
  return offenders;
}

describe("nothing on screen is a raw i18n key or an unrendered interpolation", () => {
  it("in English", async () => {
    const { container } = await renderFull();
    expect(rawI18nArtifacts(container)).toEqual([]);
  });

  it("in Turkish — ART-062: no language had ever been checked on a running screen", async () => {
    await changeLanguage("tr");
    const { container } = await renderFull();
    expect(rawI18nArtifacts(container)).toEqual([]);

    // Prove this actually rendered in Turkish rather than silently falling
    // back to English — otherwise the assertion above would pass for the
    // wrong reason.
    expect(screen.getByText("AmigaOS Kur")).toBeTruthy();
  });
});

describe("ticking a component changes what the screen will do", () => {
  it("reaches the request osinstallPlan is asked to plan, and what the plan section shows", async () => {
    await renderFull();

    // ART-205: the tree the plan predicts (one file) and the work it
    // describes (one item), which are the same number only while nothing
    // overrides anything.
    expect(document.body.textContent).toContain("1 file, ");
    expect(document.body.textContent).toContain("from 1 planned item.");

    const checkbox = screen.getByRole("checkbox", { name: "Extras3.2" }) as HTMLInputElement;
    expect(checkbox.checked).toBe(false);

    await userEvent.click(checkbox);
    expect(checkbox.checked).toBe(true);

    // The real input this screen has: the tick has to reach the request
    // that gets planned, not just flip local checkbox state.
    await waitFor(() => {
      const askedForExtras = (planMock.mock.calls as [InstallRequest][]).some(([req]) =>
        req.chosen.includes("extras")
      );
      expect(askedForExtras).toBe(true);
    });

    // ...and what the user sees changes with it — a second plan item shown,
    // not just an API call nobody could see the effect of.
    await waitFor(() => expect(document.body.textContent).toContain("from 2 planned items."));
  });

  /**
   * **ART-290.** `buildSession.components.<release>` was added with
   * `seededComponents` to migrate the panel's own two keys, and nothing ever
   * wrote it back: the session carried a one-time copy that went stale the
   * moment anybody ticked a box, so every screen reading the session saw a
   * different selection from the one on this one.
   *
   * The two halves are both asserted, because either alone passes for the
   * wrong reason. That the session key gains the id says the write landed
   * somewhere; that `osinstall.chosen` is **still the empty list
   * `FULL_FIELDS` seeded** says the legacy key was not written as well —
   * "read once, never written again" is what makes the migration safe to
   * roll back, and a screen writing both would look identical on screen.
   */
  it("writes a tick to the session, never to the legacy key (ART-290)", async () => {
    await renderFull();
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
});

describe("the screen does not plan the same thing twice", () => {
  // ART-119 (#1). This screen keeps two plans on purpose — one asked with
  // nothing excluded, so a conditional component's *true* state is knowable,
  // and one asked with the real exclusions — but with nothing excluded the
  // two requests are byte-identical, so the second call planned the same
  // media over again and threw the answer away. `plan()` opens and walks
  // every switched-on component's disc image, so that is real work on every
  // keystroke in the media, ROM and destination fields alike.

  /** How often each distinct request shape was submitted. */
  function requestCounts(): Map<string, number> {
    const seen = new Map<string, number>();
    for (const [req] of planMock.mock.calls as [InstallRequest][]) {
      const key = JSON.stringify(req);
      seen.set(key, (seen.get(key) ?? 0) + 1);
    }
    return seen;
  }

  it("asks once for a request shape it has already asked for", async () => {
    await renderFull();

    // Every request so far carries no exclusions, which is the case where
    // the base and effective requests are identical.
    expect(
      (planMock.mock.calls as [InstallRequest][]).every(([req]) => req.excluded.length === 0)
    ).toBe(true);

    // Measured, not reasoned about: reverting the dedupe to the old
    // `Promise.all([plan(base), plan(effective)])` makes this same render
    // submit **4** requests, all four byte-identical. It is 2 now. (Two and
    // not one because the effect settles twice — `useRemembered` hands back
    // a fresh array identity when the persisted value lands, which is its
    // own duplication and not this one; halving each pass is what this fix
    // does, and it is the half that was a duplicate *within* a pass.)
    expect(planMock.mock.calls.length).toBe(2);
    expect([...requestCounts().values()]).toEqual([2]);
  });

  it("still asks twice when the two requests genuinely differ", async () => {
    // `modules-a1200` has to be *forced on by its condition* for excluding
    // it to mean anything — an unforced component is turned off by plain
    // unticking, which changes `chosen` and never populates `excluded`. The
    // default fixture's V47 ROM leaves it off, so this test supplies a plan
    // where the engine switched it on without it being chosen: exactly what
    // a pre-V47 ROM produces, and the only state in which the screen offers
    // "turn it off anyway".
    planMock.mockImplementation((req: InstallRequest) => {
      const base = planResultFor(req);
      if (base.outcome !== "planned" || req.excluded.includes("modules-a1200")) return Promise.resolve(base);
      return Promise.resolve({
        ...base,
        plan: { ...base.plan, componentsOn: [...base.plan.componentsOn, "modules-a1200"] },
      } satisfies PlanResult);
    });

    await renderFull();

    const modules = screen.getByRole("checkbox", { name: "ModulesA1200_3.2" }) as HTMLInputElement;
    expect(modules.checked).toBe(true);
    await userEvent.click(modules);
    await userEvent.click(
      await screen.findByRole("button", { name: i18n.t("osinstall.components.confirmOff.confirm") })
    );

    // Both requests are made again, because now they differ. The dedupe must
    // have removed the duplicate, never the second plan — dropping the base
    // plan would make "is this condition satisfied" and "is this excluded"
    // indistinguishable, which is the whole reason there are two.
    await waitFor(() => {
      const calls = planMock.mock.calls as [InstallRequest][];
      expect(calls.some(([req]) => req.excluded.includes("modules-a1200"))).toBe(true);
      expect(
        calls.some(([req]) => req.excluded.length === 0 && req.chosen.length === 0)
      ).toBe(true);
    });
    // Two distinct shapes, not one — the base plan is still being asked for
    // alongside the effective one. `requestCounts` has more than one key
    // exactly when both survived.
    expect(requestCounts().size).toBeGreaterThan(1);
  });
});

describe("reusing the last scan, and asking for a fresh one (ART-194)", () => {
  it("plans with the cache on by default, and remembers the user turning it off", async () => {
    await renderFull();

    // Cached by default: nobody had to switch this on, and the very first
    // request the screen ever sends already says so.
    expect(planMock.mock.calls.length).toBeGreaterThan(0);
    for (const [req] of planMock.mock.calls as [InstallRequest][]) {
      expect(req.scanCache).toBe("reuse");
    }

    const box = screen.getByRole("checkbox", {
      name: i18n.t("osinstall.media.reuseScan"),
    }) as HTMLInputElement;
    expect(box.checked).toBe(true);

    planMock.mockClear();
    await userEvent.click(box);

    // Two things, and the second is the one that would go unnoticed: the plan
    // is re-asked *with the new answer*, and the choice was written to the
    // remembered store so tomorrow's run starts where this one ended.
    await waitFor(() =>
      expect(planMock).toHaveBeenCalledWith(expect.objectContaining({ scanCache: "ignore" }))
    );
    await waitFor(() => {
      const bag = useSettingsStore.getState().settings.remembered as Record<string, unknown>;
      expect(bag["osinstall.reuseScan"]).toBe(false);
    });
  });

  it("asks the backend to forget what it remembered, and re-plans against the discs", async () => {
    await renderFull();
    planMock.mockClear();

    await userEvent.click(screen.getByRole("button", { name: i18n.t("osinstall.media.rescan") }));

    // It reaches the command that actually deletes the listings — a rescan
    // that merely skipped the cache for one call would leave the stale answer
    // sitting there for the next one.
    await waitFor(() => expect(rescanMock).toHaveBeenCalled());
    // …and the screen really re-plans afterwards, rather than showing the
    // answer it already had.
    await waitFor(() => expect(planMock).toHaveBeenCalled());
    // …and says what it did, so the button cannot be mistaken for one that
    // ignored the click.
    await screen.findByText(i18n.t("osinstall.media.rescanned", { count: 1 }));
  });
});

describe("the screen settles instead of re-planning for ever (ART-195)", () => {
  // What the owner saw, and the number that made it undeniable: the release
  // build's `%TEMP%` held preview staging roots numbered up to **2,149** from
  // one session, five of them created inside two seconds. That is not a
  // component being toggled — it is an effect firing per render.
  //
  // The cause is `useRemembered`'s inline `[]` fallback. `recall` hands the
  // caller's own fallback back when nothing is stored, so an unstored key
  // yields a *fresh array identity* every render, and the plan effect lists
  // `chosen` and `excludedConditional` among its dependencies. Each pass
  // planned (three full walks of a 468 MB ISO), set state, and re-rendered.
  //
  // **Why every existing test missed it.** `rememberedComponentKey` returns
  // the bare key for AmigaOS 3.2 — "the release before there was a picker" —
  // and `FULL_FIELDS` seeds exactly those bare keys. So on 3.2 both lists are
  // stored, both identities are stable, and nothing loops. On 3.9 the keys are
  // `osinstall.chosen.AmigaOS 3.9` and `osinstall.excludedConditional.AmigaOS
  // 3.9`, which nothing had ever written. The owner was installing 3.9.
  //
  // So this test switches to 3.9 and counts. It is deliberately about the
  // *release the owner used*, not the one the fixture defaults to.
  it("does not keep re-planning a release whose remembered keys are empty", async () => {
    await renderFull();

    const picker = screen.getByRole("combobox", {
      name: i18n.t("osinstall.release.label"),
    }) as HTMLSelectElement;
    await userEvent.selectOptions(picker, "AmigaOS 3.9");
    await waitFor(() =>
      expect(planMock).toHaveBeenCalledWith(expect.objectContaining({ release: "AmigaOS 3.9" }))
    );

    // Let the screen settle, then measure how much more work it starts while
    // nothing at all is happening. Against the defect this climbs without
    // bound; the assertion is that it climbs by nothing.
    await new Promise((resolve) => setTimeout(resolve, 120));
    const planned = planMock.mock.calls.length;
    const previewed = componentCollisionsMock.mock.calls.length;
    await new Promise((resolve) => setTimeout(resolve, 250));

    expect(planMock.mock.calls.length).toBe(planned);
    expect(componentCollisionsMock.mock.calls.length).toBe(previewed);
  });
});

describe("choosing the release re-plans against it", () => {
  it("plans the release the user chose", async () => {
    await renderFull();

    const picker = screen.getByRole("combobox", {
      name: i18n.t("osinstall.release.label"),
    }) as HTMLSelectElement;
    expect(picker.value).toBe("AmigaOS 3.2");

    await userEvent.selectOptions(picker, "AmigaOS 3.9");
    expect(picker.value).toBe("AmigaOS 3.9");

    await waitFor(() =>
      expect(planMock).toHaveBeenCalledWith(expect.objectContaining({ release: "AmigaOS 3.9" }))
    );
  });

  it("shows the chosen release's own components, not the previous release's", async () => {
    // The Major finding a whole-branch review called the worst on its list:
    // the picker changed the plan and left a hardcoded AmigaOS 3.2 checklist
    // on screen, so the user was shown one operating system's parts while
    // ART installed another's (§89). The old test asserted only that
    // `planMock` was called with the new release — it never looked at the
    // checklist, which is why this survived.
    await renderFull();
    expect(screen.getByRole("checkbox", { name: "Extras3.2" })).toBeTruthy();

    const picker = screen.getByRole("combobox", {
      name: i18n.t("osinstall.release.label"),
    }) as HTMLSelectElement;
    await userEvent.selectOptions(picker, "AmigaOS 3.9");

    // The list is re-read for the release actually chosen...
    await waitFor(() => expect(componentsMock).toHaveBeenCalledWith("AmigaOS 3.9"));
    // ...3.2's own components are gone...
    await waitFor(() => expect(screen.queryByRole("checkbox", { name: "Extras3.2" })).toBeNull());
    // ...and 3.9's base component is labelled with 3.9's media, not 3.2's,
    // even though both recipes call it `workbench-base`.
    // `getAllBy`, not `getBy`: every component in the shipped AmigaOS 3.9
    // recipe carries the media name `AmigaOS3.9`, so more than one row
    // matches — which the fixture only started reflecting when ART-175 added
    // `workbench-39` to it. A `getBy` here was passing because the fixture
    // was thinner than the recipe, not because the screen shows one row.
    expect(screen.getAllByRole("checkbox", { name: /AmigaOS3\.9/ }).length).toBeGreaterThan(0);
    expect(screen.queryByRole("checkbox", { name: /Workbench3\.2/ })).toBeNull();
  });

  it("keeps each release's own ticks when the user switches away and back", async () => {
    // "Nothing changes unless the user changes it", applied to a choice made
    // for a release the user then looked away from. The remembered set is
    // keyed per release (`rememberedComponentKey`), so 3.9 — whose recipe
    // holds none of 3.2's ids — cannot sanitize them away.
    await renderFull();

    const extras = screen.getByRole("checkbox", { name: "Extras3.2" }) as HTMLInputElement;
    await userEvent.click(extras);
    expect(extras.checked).toBe(true);

    const picker = screen.getByRole("combobox", {
      name: i18n.t("osinstall.release.label"),
    }) as HTMLSelectElement;
    await userEvent.selectOptions(picker, "AmigaOS 3.9");
    await waitFor(() => expect(screen.queryByRole("checkbox", { name: "Extras3.2" })).toBeNull());

    await userEvent.selectOptions(picker, "AmigaOS 3.2");
    const backAgain = (await screen.findByRole("checkbox", {
      name: "Extras3.2",
    })) as HTMLInputElement;
    expect(backAgain.checked).toBe(true);
  });
});

describe("a media folder belongs to the release it holds (ART-207)", () => {
  // The owner chose AmigaOS 3.2 with the folder their AmigaOS 3.9 disc was
  // in still remembered, and every one of the 3.2 recipe's sixteen
  // components refused `MediaMissing` — sixteen true sentences that together
  // told them "a lot of programs are missing" about a folder that was simply
  // the wrong one. `chosen` and `excludedConditional` were already keyed per
  // release (`rememberedComponentKey`) for exactly this reason, and the
  // reason given there — "a component id means something only inside the
  // recipe that declares it" — is true word for word of a media folder: a
  // folder holding AmigaOS3.9 means nothing to the 3.2 recipe.
  //
  // The ROM is deliberately NOT part of this: a Kickstart is a property of
  // the machine being built for, not of the release being installed, and the
  // owner's one A1200 ROM is the right answer for both.
  it("shows each release's own media folder, never the other's", async () => {
    await renderFull();
    // `getAllByText`: the folder is on screen more than once since the one
    // material list arrived (design § 3.1) — the list's own row, and the
    // packages panel below, which is a *view onto the same list* rather than
    // a second remembered folder. What this test is about is that the
    // **other release's** folder is nowhere, and that is asserted below.
    expect(screen.getAllByText("E:\\media").length).toBeGreaterThan(0);

    const picker = screen.getByRole("combobox", {
      name: i18n.t("osinstall.release.label"),
    }) as HTMLSelectElement;
    await userEvent.selectOptions(picker, "AmigaOS 3.9");
    await waitFor(() => expect(componentsMock).toHaveBeenCalledWith("AmigaOS 3.9"));

    expect((await screen.findAllByText("E:\\media39")).length).toBeGreaterThan(0);
    expect(screen.queryAllByText("E:\\media")).toHaveLength(0);
  });

  it("plans a release into its own destination, not the other release's", async () => {
    await renderFull();
    // `getAllByText`: the path is on screen more than once — this field, and
    // the build session's own tree root, which is a different thing (ART-197)
    // and is not what this test is about. So the assertion that matters is
    // made against the request `plan()` is actually given, not the DOM.
    expect(screen.getAllByText("E:\\dist").length).toBeGreaterThan(0);

    const picker = screen.getByRole("combobox", {
      name: i18n.t("osinstall.release.label"),
    }) as HTMLSelectElement;
    await userEvent.selectOptions(picker, "AmigaOS 3.9");

    await waitFor(() =>
      expect(planMock).toHaveBeenCalledWith(
        expect.objectContaining({ release: "AmigaOS 3.9", destination: "E:\\dist39" })
      )
    );
    // Not one 3.9 plan may name the folder the user set aside for their 3.2
    // build — `toHaveBeenCalledWith` above would be satisfied by a single
    // correct call among wrong ones.
    const planned39 = planMock.mock.calls
      .map((call) => call[0] as InstallRequest)
      .filter((req) => req.release === "AmigaOS 3.9");
    expect(planned39.length).toBeGreaterThan(0);
    for (const req of planned39) {
      expect(req.destination).not.toBe("E:\\dist");
    }
  });

  it("keeps each release's own media folder when the user switches away and back", async () => {
    await renderFull();

    const picker = screen.getByRole("combobox", {
      name: i18n.t("osinstall.release.label"),
    }) as HTMLSelectElement;
    await userEvent.selectOptions(picker, "AmigaOS 3.9");
    await waitFor(() => expect(componentsMock).toHaveBeenCalledWith("AmigaOS 3.9"));

    // Through the one folder picker (design § 3.1) rather than the flat
    // field's own Browse button, which no longer exists. The 3.9 list is
    // seeded from that release's own remembered folder, so this adds a
    // second row to it.
    await addFolder("E:\\os39");
    // `findAllByText`: since the one material list (design § 3.1) a folder is
    // shown by the list row and by the packages panel below, which reads the
    // same list rather than keeping a folder of its own.
    expect((await screen.findAllByText("E:\\os39")).length).toBeGreaterThan(0);

    await userEvent.selectOptions(picker, "AmigaOS 3.2");
    expect((await screen.findAllByText("E:\\media")).length).toBeGreaterThan(0);

    await userEvent.selectOptions(picker, "AmigaOS 3.9");
    expect((await screen.findAllByText("E:\\os39")).length).toBeGreaterThan(0);
  });
});

describe("a disc dropped on the panel", () => {
  it("takes the folder from a disc dropped on the panel", async () => {
    // A JSX attribute string literal does not process `\\` as a JS escape
    // sequence the way a normal string literal does (unlike the call below,
    // which is an ordinary function argument) — passed as a bare attribute,
    // the brief's literal path would arrive with doubled backslashes. The
    // `{...}` expression form is what makes this an actual JS string.
    render(
      <OsInstall
        droppedMedia={{ path: "E:\\amiga\\Amigatolon\\iso\\AmigaOS39.iso", arrivalKey: "k1" }}
      />
    );
    await waitFor(() =>
      expect(scanMediaMock).toHaveBeenCalledWith("E:\\amiga\\Amigatolon\\iso")
    );
  });

  /// **Appends, never replaces** (fix round 1, F8). The flat field had one
  /// slot, so a drop overwrote whatever was in it; the list has room, and a
  /// person dropping a second disc from a second folder means both folders.
  /// Neither of the two tests around this asserted the previously held folder
  /// survived, which is the whole behaviour change.
  it("adds the dropped folder to the list without taking the held one out", async () => {
    seedRemembered(FULL_FIELDS);
    const { rerender } = render(<OsInstall />);
    await waitFor(() => expect(screen.queryAllByTestId("material-folder")).toHaveLength(1));

    rerender(
      <OsInstall
        droppedMedia={{ path: "E:\\amiga\\Amigatolon\\iso\\AmigaOS39.iso", arrivalKey: "k1" }}
      />
    );

    const rows = await screen.findAllByTestId("material-folder");
    expect(rows.map((row) => row.textContent)).toEqual([
      expect.stringContaining("E:\\media"),
      expect.stringContaining("E:\\amiga\\Amigatolon\\iso"),
    ]);
    // And the folder that was already there is still what the planner reads
    // first \u2014 a drop is an addition, not a re-pick.
    await waitFor(() => expect(planMock).toHaveBeenCalled());
    const sent = planMock.mock.calls.at(-1)![0] as InstallRequest;
    expect(sent.mediaFolder).toBe("E:\\media");
    expect(sent.extraMediaFolders).toEqual(["E:\\amiga\\Amigatolon\\iso"]);
  });

  it("takes effect again when the same disc is dropped a second time", async () => {
    const PATH = "E:\\amiga\\Amigatolon\\iso\\AmigaOS39.iso";
    const FOLDER = "E:\\amiga\\Amigatolon\\iso";

    const { rerender } = render(<OsInstall droppedMedia={{ path: PATH, arrivalKey: "k1" }} />);
    await waitFor(() => expect(scanMediaMock).toHaveBeenCalledWith(FOLDER));
    scanMediaMock.mockClear();

    // The user changes the media folder by hand in between — the same
    // remembered-setter path a Browse click goes through.
    seedRemembered({ "osinstall.mediaFolder": "E:\\somewhere-else" });

    // A second drop of the *same* disc: the path string is identical to the
    // first arrival, so only a change in `arrivalKey` can make this visible.
    // Keying the effect on the path alone (the bug the review caught) would
    // leave the hand-picked folder in place and never re-scan.
    rerender(<OsInstall droppedMedia={{ path: PATH, arrivalKey: "k2" }} />);
    await waitFor(() => expect(scanMediaMock).toHaveBeenCalledWith(FOLDER));
  });
});

describe("a folder that is simply the wrong one (ART-208)", () => {
  // The owner's own screen, reproduced: AmigaOS 3.2 chosen, and the folder
  // still pointing at the one holding their AmigaOS 3.9 disc. Sixteen
  // components, sixteen `MediaMissing` refusals, every one of them true —
  // and what they read off the screen was "a lot of programs are missing".
  const WRONG_FOLDER_REFUSALS: RefusalReason[] = [
    { refusal: "media-missing", component: "workbench-base", volume_name: "Workbench3.2" },
    { refusal: "media-missing", component: "install-libs", volume_name: "Install3.2" },
    { refusal: "media-missing", component: "extras", volume_name: "Extras3.2" },
  ];

  /** A 3.2 build pointed at a folder holding exactly one AmigaOS 3.9 disc. */
  async function renderWrongFolder(releaseHolding: string | null) {
    scanMediaMock.mockReset().mockResolvedValue({
      outcome: "found",
      media: [
        { path: "E:\\media\\AmigaOS39.iso", volumeName: "AmigaOS3.9", kind: "disc" },
      ],
    } satisfies MediaScanResult);
    planMock.mockReset().mockImplementation((req: InstallRequest) =>
      Promise.resolve({
        outcome: "planned",
        plan: {
          release: "AmigaOS 3.2",
          items: [],
          refusals: WRONG_FOLDER_REFUSALS,
          totalBytes: 0,
          totalFiles: 0,
          componentsOn: ["workbench-base", "install-libs", ...req.chosen],
          mediaPaths: {},
          packages: [],
          packageMedia: {},
          userStartup: [],
          activations: [],
          mediaStamps: {},
          removals: [],
          layers: [],
        },
      } satisfies PlanResult)
    );
    releaseForMediaMock.mockReset().mockResolvedValue(releaseHolding);
    // The folder holds one AmigaOS 3.9 disc and nothing 3.2 asks for — the
    // only state in which "none of the disks in this folder are ones this
    // release asks for" is a true sentence (ART-253).
    mediaEvidenceMock.mockReset().mockResolvedValue({
      release: "AmigaOS 3.2",
      distinguishing: [],
      shared: [],
      missingRequired: ["Workbench3.2", "Install3.2"],
    });
    seedRemembered(FULL_FIELDS);
    render(<OsInstall />);
    await waitFor(() => expect(planMock).toHaveBeenCalled());
  }

  it("says it once, instead of once per component", async () => {
    await renderWrongFolder(null);

    expect(
      await screen.findByText(
        i18n.t("osinstall.blocked.wrongFolder", { found: "AmigaOS3.9" })
      )
    ).toBeTruthy();

    // And the three sentences it replaces are gone — computed the way the
    // screen computes them, not typed out here where they could drift.
    for (const refusal of WRONG_FOLDER_REFUSALS) {
      const phrase = refusalPhrase(refusal);
      expect(screen.queryByText(i18n.t(phrase.key, phrase.params))).toBeNull();
    }
  });

  it("names the release the folder belongs to, and switching is one click", async () => {
    await renderWrongFolder("AmigaOS 3.9");

    expect(
      await screen.findByText(
        i18n.t("osinstall.blocked.wrongFolderIsRelease", {
          release: "AmigaOS 3.9",
          found: "AmigaOS3.9",
        })
      )
    ).toBeTruthy();

    const picker = screen.getByRole("combobox", {
      name: i18n.t("osinstall.release.label"),
    }) as HTMLSelectElement;
    expect(picker.value).toBe("AmigaOS 3.2");

    await userEvent.click(
      screen.getByRole("button", {
        name: i18n.t("osinstall.blocked.switchRelease", { release: "AmigaOS 3.9" }),
      })
    );

    // The button does the thing it names — the release actually changes,
    // rather than the screen merely suggesting it.
    await waitFor(() => expect(picker.value).toBe("AmigaOS 3.9"));
  });

  it("keeps the per-disk list when the folder is the right one", async () => {
    // One disk absent from an otherwise right folder is a missing disk, and
    // this screen must still say which. The guard that separates the two
    // cases lives in `wrongMediaFolder`; this is it seen from the screen.
    scanMediaMock.mockReset().mockResolvedValue({
      outcome: "found",
      media: [{ path: "E:\\media\\Disk1.adf", volumeName: "Workbench3.2", kind: "floppy" }],
    } satisfies MediaScanResult);
    planMock.mockReset().mockResolvedValue({
      outcome: "planned",
      plan: {
        release: "AmigaOS 3.2",
        items: [],
        refusals: [WRONG_FOLDER_REFUSALS[0]],
        totalBytes: 0,
        totalFiles: 0,
        componentsOn: ["workbench-base"],
        mediaPaths: {},
        packages: [],
        packageMedia: {},
        userStartup: [],
        activations: [],
        mediaStamps: {},
        removals: [],
        layers: [],
      },
    } satisfies PlanResult);
    releaseForMediaMock.mockReset().mockResolvedValue(null);
    seedRemembered(FULL_FIELDS);
    render(<OsInstall />);

    const phrase = refusalPhrase(WRONG_FOLDER_REFUSALS[0]);
    expect(await screen.findByText(i18n.t(phrase.key, phrase.params))).toBeTruthy();
  });

  // Refusal-evidence round, Task 2. `mediaEvidence` refuses to speak over
  // `wrongMediaFolder`'s own sentence (it calls `wrongMediaFolder` directly
  // rather than re-deriving its conditions) — so the all-or-nothing case
  // must show exactly one message, never both.
  it("leaves the all-or-nothing case to its own message", async () => {
    await renderWrongFolder(null);

    expect(
      await screen.findByText(i18n.t("osinstall.blocked.wrongFolder", { found: "AmigaOS3.9" }))
    ).toBeTruthy();

    // The weaker, unidentified-release evidence sentence this same input
    // would produce for the *partial* case must not also appear here.
    expect(
      screen.queryByText(i18n.t("osinstall.evidence.unidentified", { found: "AmigaOS3.9" }))
    ).toBeNull();
  });
});

describe("the folder's own evidence for a partial build (refusal-evidence round, Task 2)", () => {
  // The folder holds disks this release asks for and one it wants is absent
  // — the ordinary partial case, where the refusals list already names which
  // disk and this line adds what it cannot: what the folder itself looks
  // like.
  //
  // **The fixture used to carry `items: [ITEM_WORKBENCH]` alongside the
  // refusal, and this comment used to say "a component *is* installable
  // (`items` non-empty)".** No plan the core can emit is like that
  // (ART-253): any refusal at all empties `items`
  // (`core/osinstall/plan.rs`). What separates this case from the
  // all-or-nothing one is not the item count — it is that the folder holds
  // media this release asks for, which is what `osinstallMediaEvidence`
  // answers.
  const PARTIAL_REFUSALS: RefusalReason[] = [
    { refusal: "media-missing", component: "extras", volume_name: "Extras3.2" },
  ];

  /** The plan the fixtures below share — refusals, and therefore no items. */
  const PARTIAL_PLAN = {
    release: "AmigaOS 3.2",
    items: [],
    refusals: PARTIAL_REFUSALS,
    totalBytes: 0,
    totalFiles: 0,
    componentsOn: ["workbench-base", "install-libs", "extras"],
    mediaPaths: {},
    packages: [],
    packageMedia: {},
    userStartup: [],
    activations: [],
    mediaStamps: {},
    removals: [],
    layers: [],
  } satisfies InstallPlan;

  async function renderPartialMedia(releaseHolding: string | null) {
    scanMediaMock.mockReset().mockResolvedValue({
      outcome: "found",
      media: [{ path: "E:\\media\\Disk1.adf", volumeName: "Workbench3.2", kind: "floppy" }],
    } satisfies MediaScanResult);
    planMock.mockReset().mockResolvedValue({
      outcome: "planned",
      plan: { ...PARTIAL_PLAN, mediaPaths: { "Workbench3.2": "E:\\media\\Disk1.adf" } },
    } satisfies PlanResult);
    releaseForMediaMock.mockReset().mockResolvedValue(releaseHolding);
    seedRemembered(FULL_FIELDS);
    render(<OsInstall />);
    await waitFor(() => expect(planMock).toHaveBeenCalled());
  }

  it("shows what the folder holds above the refusals when some disks are missing", async () => {
    await renderPartialMedia("AmigaOS 3.2");

    // **The sentence, named** — not `mediaEvidence(…)`'s own answer rendered
    // back at it (the 2026-09-06 review's M4 wave, finding M7). Building the
    // expectation by calling the helper made both sides move together: the
    // key mapping could change to any other state's key and this stayed
    // green, so it guarded "the component renders whatever the helper
    // returns" and not "the right sentence for this state". Its siblings in
    // this file name their key; so does this one now.
    expect(
      await screen.findByText(
        i18n.t("osinstall.evidence.sameRelease", {
          found: "Workbench3.2",
          missing: "Extras3.2",
        })
      )
    ).toBeTruthy();

    // Context, not a replacement: the per-disk refusal this line sits above
    // must still be there.
    const refusalPhraseText = refusalPhrase(PARTIAL_REFUSALS[0]);
    expect(
      screen.getByText(i18n.t(refusalPhraseText.key, refusalPhraseText.params))
    ).toBeTruthy();
  });

  // **The fixture this used to run on was one the core cannot emit**, and
  // ART-257 is what exposed it: a folder holding nothing but `Workbench3.2`,
  // with `osinstall_release_for_media` answering `"AmigaOS 3.9"` and
  // `osinstall_media_evidence` answering that `Workbench3.2` is AmigaOS 3.2's
  // own distinguishing media — two answers about one pile that contradict
  // each other, since a folder holding 3.2's own Workbench disk is never
  // identified as 3.9. It passed only because nothing read the second answer.
  //
  // The real shape of "this is somebody else's folder, but not *entirely*":
  // AmigaOS 3.9's disc beside a plain `Fonts`, while building 3.2. `Fonts`
  // carries no version and every release asks for it, so 3.2's own evidence
  // is `shared: ["Fonts"]` with **nothing** distinguishing — which is what
  // keeps `wrongMediaFolder` quiet (the folder is not entirely foreign) and
  // makes this the sentence to say.
  const OTHER_RELEASE_REFUSALS: RefusalReason[] = [
    { refusal: "media-missing", component: "workbench-base", volume_name: "Workbench3.2" },
    { refusal: "media-missing", component: "install-libs", volume_name: "Install3.2" },
    { refusal: "media-missing", component: "extras", volume_name: "Extras3.2" },
  ];

  it("names the other release when the folder is a different one", async () => {
    scanMediaMock.mockReset().mockResolvedValue({
      outcome: "found",
      media: [
        { path: "E:\\media\\AmigaOS3.9.iso", volumeName: "AmigaOS3.9", kind: "disc" },
        { path: "E:\\media\\Fonts.adf", volumeName: "Fonts", kind: "floppy" },
      ],
    } satisfies MediaScanResult);
    planMock.mockReset().mockResolvedValue({
      outcome: "planned",
      plan: { ...PARTIAL_PLAN, refusals: OTHER_RELEASE_REFUSALS },
    } satisfies PlanResult);
    releaseForMediaMock.mockReset().mockResolvedValue("AmigaOS 3.9");
    mediaEvidenceMock.mockReset().mockResolvedValue({
      release: "AmigaOS 3.2",
      distinguishing: [],
      shared: ["Fonts"],
      missingRequired: ["Workbench3.2", "Install3.2"],
    });
    seedRemembered(FULL_FIELDS);
    render(<OsInstall />);
    await waitFor(() => expect(planMock).toHaveBeenCalled());

    expect(
      await screen.findByText(
        i18n.t("osinstall.evidence.otherRelease", {
          found: "AmigaOS3.9, Fonts",
          release: "AmigaOS 3.9",
          missing: "Workbench3.2, Install3.2, Extras3.2",
        })
      )
    ).toBeTruthy();

    // Still just context — the refusal itself is still named below it.
    const refusalPhraseText = refusalPhrase(OTHER_RELEASE_REFUSALS[0]);
    expect(
      screen.getByText(i18n.t(refusalPhraseText.key, refusalPhraseText.params))
    ).toBeTruthy();
  });

  it("does not claim a release for a folder that identifies none", async () => {
    await renderPartialMedia(null);

    expect(
      await screen.findByText(
        i18n.t("osinstall.evidence.unidentified", { found: "Workbench3.2" })
      )
    ).toBeTruthy();

    // The section says what the folder holds, never a release it cannot
    // actually put a name to.
    const section = screen.getByText(i18n.t("osinstall.refusals.heading")).closest("section");
    expect(section?.textContent).not.toContain("AmigaOS 3.2");
    expect(section?.textContent).not.toContain("AmigaOS 3.9");
  });

  // ART-255. The Ambiguous folder, which nothing in the round constructed:
  // `Workbench3.2` **and** `AmigaOS3.9` in one place. Both disks carry a
  // version of their own and each names a release — ART declines to choose
  // between two, and `release_holding` answers the same `null` an unknown
  // folder gets. The sentence the user reads must therefore be true of every
  // state that reaches it.
  it("does not tell a folder naming two releases that its disks carry no version", async () => {
    scanMediaMock.mockReset().mockResolvedValue({
      outcome: "found",
      media: [
        { path: "E:\\media\\Disk1.adf", volumeName: "Workbench3.2", kind: "floppy" },
        { path: "E:\\media\\AmigaOS39.iso", volumeName: "AmigaOS3.9", kind: "disc" },
      ],
    } satisfies MediaScanResult);
    planMock.mockReset().mockResolvedValue({
      outcome: "planned",
      plan: { ...PARTIAL_PLAN, mediaPaths: { "Workbench3.2": "E:\\media\\Disk1.adf" } },
    } satisfies PlanResult);
    // Ambiguous, collapsed to `null` — the same value Unknown produces, and
    // the reason the sentence cannot state a cause.
    releaseForMediaMock.mockReset().mockResolvedValue(null);
    seedRemembered(FULL_FIELDS);
    render(<OsInstall />);

    const line = await screen.findByText(
      i18n.t("osinstall.evidence.unidentified", { found: "Workbench3.2, AmigaOS3.9" })
    );

    // The point of the whole finding, and the only assertion here a reverted
    // catalogue entry would fail: the sentence explains *no* reason. It used
    // to end "— some disks carry no version of their own", which is false
    // about both of these disks and sends this user to look for a version
    // number that is already there.
    expect(line.textContent).not.toMatch(/carry no version/i);
    expect(line.textContent).not.toMatch(/version of (its|their) own/i);
  });

  // ART-258. The sentence bound *"the right media for this release"* to the
  // whole listing, and the listing is everything the scan found — ART ships
  // recipes for 3.2, 3.2.2 and 3.9 only, so a `Workbench3.1` disk kept beside
  // a 3.2 set contributes nothing to `identify`, the folder still resolves to
  // AmigaOS 3.2, and the line called that disk the right media for a release
  // it has nothing to do with. A claim about each listed disk that ART never
  // made.
  it("does not call every disk in the folder the right media for this release", async () => {
    const STRAY = ["Workbench3.2", "Extras3.2", "Workbench3.1"];
    scanMediaMock.mockReset().mockResolvedValue({
      outcome: "found",
      media: STRAY.map((volumeName) => ({
        path: `E:\\media\\${volumeName}.adf`,
        volumeName,
        kind: "floppy",
      })),
    } satisfies MediaScanResult);
    planMock.mockReset().mockResolvedValue({
      outcome: "planned",
      plan: {
        ...PARTIAL_PLAN,
        refusals: [
          { refusal: "media-missing", component: "install-libs", volume_name: "Install3.2" },
        ],
      },
    } satisfies PlanResult);
    releaseForMediaMock.mockReset().mockResolvedValue("AmigaOS 3.2");
    // `Workbench3.1` is in neither list: it is no release ART installs, so
    // this release's own recipe says nothing about it.
    mediaEvidenceMock.mockReset().mockResolvedValue({
      release: "AmigaOS 3.2",
      distinguishing: ["Workbench3.2", "Extras3.2"],
      shared: [],
      missingRequired: ["Install3.2"],
    });
    seedRemembered(FULL_FIELDS);
    render(<OsInstall />);

    const line = await screen.findByText(
      i18n.t("osinstall.evidence.sameRelease", {
        found: STRAY.join(", "),
        missing: "Install3.2",
      })
    );

    // The listing itself is right — the scan really did read those three
    // names, and hiding one would be a different lie. What must not be there
    // is the claim *about* them, and this is the only assertion here a
    // reverted catalogue entry fails: `i18n.t(key, params)` above resolves
    // from the very file under test, so it moves with any rewording.
    expect(line.textContent).not.toMatch(/the right media/i);
    // The weaker claim ART did check — `identify` named this release off
    // these disks — still has to be said, or the line says nothing at all.
    expect(line.textContent).toMatch(/this release's own media is among them/i);
  });

  // ART-257. **The release with two media folders is the one most likely to
  // arrive part-complete, and it was the one this whole feature never
  // reached.** A layered release sets no flat folder — `mediaFolder` is
  // remembered per release and its field is never drawn — so `mediaScan`
  // stayed `null`, `foundVolumeNames` stayed `[]`, and the evidence line was
  // silent for every AmigaOS 3.2.2 build however full the layer folders were.
  //
  // Both halves are asserted here, because the union alone would have made
  // the line *speak falsely*: `identify` answers `"AmigaOS 3.2"` for the base
  // set (correctly — a based release must not be named off its base's disks
  // alone), and the old branching would have read that as somebody else's
  // media.
  it("says what a layered release's own folders hold, and does not call the base set somebody else's", async () => {
    const BASE_FOLDER = "E:\\base322";
    const BASE_SET = ["Workbench3.2", "Install3.2", "Extras3.2", "Fonts", "Locale"];
    scanMediaMock.mockReset().mockImplementation((folder: string) =>
      Promise.resolve(
        folder === BASE_FOLDER
          ? ({
              outcome: "found",
              media: BASE_SET.map((volumeName) => ({
                path: `${BASE_FOLDER}\\${volumeName}.adf`,
                volumeName,
                kind: "floppy",
              })),
            } satisfies MediaScanResult)
          : ({ outcome: "found", media: [] } satisfies MediaScanResult)
      )
    );
    planMock.mockReset().mockResolvedValue({
      outcome: "planned",
      plan: {
        ...PARTIAL_PLAN,
        release: "AmigaOS 3.2.2",
        refusals: [
          { refusal: "media-missing", component: "update-322-system", volume_name: "Update3.2.2" },
          {
            refusal: "media-missing",
            component: "update-322-classes",
            volume_name: "Classes3.2.2",
          },
        ],
      },
    } satisfies PlanResult);
    // Both answers measured off the shipped recipes rather than chosen —
    // `identify.rs::a_based_releases_own_evidence_claims_the_base_set`.
    releaseForMediaMock.mockReset().mockResolvedValue("AmigaOS 3.2");
    mediaEvidenceMock.mockReset().mockResolvedValue({
      release: "AmigaOS 3.2.2",
      distinguishing: ["Workbench3.2", "Install3.2", "Extras3.2"],
      shared: ["Locale", "Fonts"],
      missingRequired: ["Update3.2.2", "Classes3.2.2"],
    });
    seedRemembered({
      ...FULL_FIELDS,
      "buildSession.release": "AmigaOS 3.2.2",
      "osinstall.destination.AmigaOS 3.2.2": "E:\\dist322",
      // `osinstall.mediaFolder.<layerId>.<release>` — see `layerFolderKey`.
      "osinstall.mediaFolder.base.AmigaOS 3.2.2": BASE_FOLDER,
    });
    render(<OsInstall />);

    // The whole sentence, both parameters: the base disks the layer folder
    // really holds, and the update disks that are really absent.
    expect(
      await screen.findByText(
        i18n.t("osinstall.evidence.sameRelease", {
          found: BASE_SET.join(", "),
          missing: "Update3.2.2, Classes3.2.2",
        })
      )
    ).toBeTruthy();

    // The false sentence the union alone would have produced is not on
    // screen — this release's own base media is not called "AmigaOS 3.2
    // media" (M1: even the corrected wording still overclaims for the
    // wrong disks, so it must not fire here at all).
    expect(
      screen.queryByText(
        i18n.t("osinstall.evidence.otherRelease", {
          found: BASE_SET.join(", "),
          release: "AmigaOS 3.2",
          missing: "Update3.2.2, Classes3.2.2",
        })
      )
    ).toBeNull();

    // And Rust was asked about the layer folder's own disks — a screen
    // cannot be right downstream of a lookup that was asked about nothing.
    await waitFor(() =>
      expect(mediaEvidenceMock).toHaveBeenCalledWith("AmigaOS 3.2.2", BASE_SET)
    );
  });

  // ART-257's other half, and a survivor of the first mutation round rather
  // than something the reading found: `layers` is `[]` both when a release is
  // unlayered *and* before `layersFor` has answered (ART-256 named that trap
  // one layer down). On the render straight after a switch to a layered
  // release, `layers` is still the previous release's answer and `mediaScan`
  // is still the previous release's folder — so a `foundVolumeNames` that did
  // not ask whether `layers` is this release's answer would hand the new
  // release's evidence lookup the **old** folder's disks, and the sentence
  // would be about a folder this build never reads.
  //
  // An invariant, not a wait: the question is one Rust must never be asked,
  // so no timing decides it.
  it("never asks about the previous release's folder while a layered release is loading", async () => {
    scanMediaMock.mockReset().mockImplementation((folder: string) =>
      Promise.resolve(
        folder === "E:\\media"
          ? ({
              outcome: "found",
              media: [{ path: "E:\\media\\Disk1.adf", volumeName: "Workbench3.2", kind: "floppy" }],
            } satisfies MediaScanResult)
          : ({ outcome: "found", media: [] } satisfies MediaScanResult)
      )
    );
    seedRemembered(FULL_FIELDS);
    render(<OsInstall />);

    // Proven, not assumed: the flat folder really is being asked about for
    // AmigaOS 3.2 before the switch, so the absence below is about the
    // switch and not about a screen that never asked anything.
    await waitFor(() =>
      expect(mediaEvidenceMock).toHaveBeenCalledWith("AmigaOS 3.2", ["Workbench3.2"])
    );

    const picker = await screen.findByRole("combobox", {
      name: i18n.t("osinstall.release.label"),
    });
    await userEvent.selectOptions(picker, "AmigaOS 3.2.2");

    // AmigaOS 3.2.2 reads its layer folders, and its own is empty. The one
    // question that must never be asked is the new release against the old
    // release's flat folder.
    await waitFor(() => expect(layersForMock).toHaveBeenCalledWith("AmigaOS 3.2.2"));
    expect(mediaEvidenceMock).not.toHaveBeenCalledWith("AmigaOS 3.2.2", ["Workbench3.2"]);
  });

  // ART-178/ART-195's own shape, one folder set further on. `layerScans` now
  // feeds the memo the two evidence lookups depend on, so an effect that
  // writes a **fresh** empty object on every run — which is what "no layers,
  // clear the scans" used to do — hands `foundVolumeNames` a new identity for
  // no new information and both lookups are asked again for it.
  //
  // Counted, both arms, because "it feels the same" is not a result: with the
  // guard the settled unlayered screen asks each lookup **once**; writing a
  // fresh `{}` instead makes it **twice**. Nothing is on a clock here — the
  // count is read once the sentence those lookups produce is on screen.
  it("asks each media lookup once for a settled folder, not once per render", async () => {
    seedRemembered(FULL_FIELDS);
    planMock.mockReset().mockResolvedValue({
      outcome: "planned",
      plan: {
        ...PARTIAL_PLAN,
        refusals: [
          { refusal: "media-missing", component: "install-libs", volume_name: "Install3.2" },
        ],
      },
    } satisfies PlanResult);
    releaseForMediaMock.mockReset().mockResolvedValue("AmigaOS 3.2");
    render(<OsInstall />);

    await screen.findByText(
      i18n.t("osinstall.evidence.sameRelease", {
        found: "Workbench3.2",
        missing: "Install3.2",
      })
    );

    expect(mediaEvidenceMock.mock.calls).toEqual([["AmigaOS 3.2", ["Workbench3.2"]]]);
    expect(releaseForMediaMock.mock.calls).toEqual([[["Workbench3.2"]]]);
  });
});

// ---------------------------------------------------------------------------
// ART-254 — a release switch must not leave the previous release's evidence
// checking the new release's plan
// ---------------------------------------------------------------------------

describe("switching release does not bring the wrong-folder sentence back (ART-254)", () => {
  /** A plan carrying nothing but media-missing refusals — the state
   *  `wrongMediaFolder` is allowed to speak in at all. */
  function refusedPlanFor(release: string, missing: string): PlanResult {
    return {
      outcome: "planned",
      plan: {
        release,
        items: [],
        refusals: [{ refusal: "media-missing", component: "extras", volume_name: missing }],
        totalBytes: 0,
        totalFiles: 0,
        componentsOn: ["workbench-base"],
        mediaPaths: {},
        packages: [],
        packageMedia: {},
        userStartup: [],
        activations: [],
        mediaStamps: {},
        removals: [],
        layers: [],
      },
    } satisfies PlanResult;
  }

  // The brief's own input, and it is a first-class action on this screen
  // rather than an edge case: the folder holds AmigaOS 3.2 disks, the build
  // is on 3.9, and ART offers a button that switches to the release the
  // folder actually holds. `mediaFacts` is not cleared when its effect
  // re-runs, and the plan is fetched by a different effect — so between the
  // two there is a moment where 3.2's plan is checked against 3.9's evidence,
  // and 3.9's evidence is *correctly* empty for a folder of 3.2 disks. That
  // is exactly the shape ART-253's check reads as "none of these disks are
  // ones this release asks for".
  //
  // The window is not raced here. 3.2's evidence lookup simply never
  // resolves, which is the same state the window is and is deterministic —
  // `CLAUDE.md`: anything timing-dependent gets an invariant, not a wait.
  it("says what the folder really holds while the previous release's evidence is still the only one held", async () => {
    scanMediaMock.mockReset().mockResolvedValue({
      outcome: "found",
      media: [
        { path: "E:\\media\\Disk1.adf", volumeName: "Workbench3.2", kind: "floppy" },
        { path: "E:\\media\\Fonts.adf", volumeName: "Fonts", kind: "floppy" },
        { path: "E:\\media\\Locale.adf", volumeName: "Locale", kind: "floppy" },
      ],
    } satisfies MediaScanResult);
    releaseForMediaMock.mockReset().mockResolvedValue("AmigaOS 3.2");
    planMock.mockReset().mockImplementation((req: InstallRequest) =>
      Promise.resolve(
        req.release === "AmigaOS 3.9"
          ? refusedPlanFor("AmigaOS 3.9", "AmigaOS3.9")
          : refusedPlanFor("AmigaOS 3.2", "Extras3.2")
      )
    );
    mediaEvidenceMock.mockReset().mockImplementation((release: string) =>
      release === "AmigaOS 3.9"
        ? Promise.resolve({
            release: "AmigaOS 3.9",
            distinguishing: [],
            shared: [],
            missingRequired: ["AmigaOS3.9"],
          })
        : // Never resolves: the window, held open.
          new Promise(() => {})
    );
    seedRemembered({ ...FULL_FIELDS, "buildSession.release": "AmigaOS 3.9" });
    render(<OsInstall />);

    const FOUND = "Workbench3.2, Fonts, Locale";

    // The setup is proven rather than assumed: on 3.9 this folder really is
    // the wrong one, the sentence really is true, and it really is on screen.
    expect(
      await screen.findByText(
        i18n.t("osinstall.blocked.wrongFolderIsRelease", {
          release: "AmigaOS 3.2",
          found: FOUND,
        })
      )
    ).toBeTruthy();

    await userEvent.click(
      screen.getByRole("button", {
        name: i18n.t("osinstall.blocked.switchRelease", { release: "AmigaOS 3.2" }),
      })
    );

    // What the screen must say instead — the whole sentence, key and both
    // parameters. "Says nothing" would have been true here for at least three
    // other reasons (no plan yet, no refusals, an empty folder); this state
    // has one cause, and it is that the stale evidence withdrew.
    expect(
      await screen.findByText(
        i18n.t("osinstall.evidence.sameRelease", { found: FOUND, missing: "Extras3.2" })
      )
    ).toBeTruthy();

    // And ART-253's own sentence, in both of its forms, is nowhere on screen.
    expect(
      screen.queryByText(i18n.t("osinstall.blocked.wrongFolder", { found: FOUND }))
    ).toBeNull();
    expect(
      screen.queryByText(
        i18n.t("osinstall.blocked.wrongFolderIsRelease", {
          release: "AmigaOS 3.2",
          found: FOUND,
        })
      )
    ).toBeNull();
  });
});

// ---------------------------------------------------------------------------
// ART-256 — the evidence describes every folder the plan was built from
// ---------------------------------------------------------------------------

describe("the evidence covers the added folders too (ART-256)", () => {
  const EXTRA = "E:\\media\\extras";

  /** `Workbench3.2` in the main folder, `Extras3.2` in an added one — and the
   *  plan, which reads both, still short of `Install3.2`. */
  function scanPerFolder() {
    scanMediaMock.mockReset().mockImplementation((folder: string) =>
      Promise.resolve(
        folder === EXTRA
          ? ({
              outcome: "found",
              media: [
                { path: `${EXTRA}\\Extras.adf`, volumeName: "Extras3.2", kind: "floppy" },
              ],
            } satisfies MediaScanResult)
          : ({
              outcome: "found",
              media: [
                { path: "E:\\media\\Disk1.adf", volumeName: "Workbench3.2", kind: "floppy" },
              ],
            } satisfies MediaScanResult)
      )
    );
  }

  it("names a disk that is only in an added folder", async () => {
    scanPerFolder();
    planMock.mockReset().mockResolvedValue({
      outcome: "planned",
      plan: {
        release: "AmigaOS 3.2",
        items: [],
        refusals: [
          { refusal: "media-missing", component: "install-libs", volume_name: "Install3.2" },
        ],
        totalBytes: 0,
        totalFiles: 0,
        componentsOn: ["workbench-base", "extras"],
        mediaPaths: {},
        packages: [],
        packageMedia: {},
        userStartup: [],
        activations: [],
        mediaStamps: {},
        removals: [],
        layers: [],
      },
    } satisfies PlanResult);
    releaseForMediaMock.mockReset().mockResolvedValue("AmigaOS 3.2");
    mediaEvidenceMock.mockReset().mockResolvedValue({
      release: "AmigaOS 3.2",
      distinguishing: ["Workbench3.2", "Extras3.2"],
      shared: [],
      missingRequired: ["Install3.2"],
    });
    seedRemembered({ ...FULL_FIELDS, "osinstall.extraMediaFolders": [EXTRA] });
    render(<OsInstall />);

    // The whole sentence: `Extras3.2` is named because the plan read it, and
    // an evidence line that stopped at the main folder would say
    // "This folder holds Workbench3.2" about a build that had already found
    // two disks.
    expect(
      await screen.findByText(
        i18n.t("osinstall.evidence.sameRelease", {
          found: "Workbench3.2, Extras3.2",
          missing: "Install3.2",
        })
      )
    ).toBeTruthy();

    // And the question Rust was asked carries both, not the main folder's
    // one — the screen cannot be right about this by accident downstream of a
    // lookup that was asked the smaller question.
    await waitFor(() =>
      expect(mediaEvidenceMock).toHaveBeenCalledWith("AmigaOS 3.2", [
        "Workbench3.2",
        "Extras3.2",
      ])
    );
  });

  // The scope trap the brief names. A layered release passes
  // `extraMediaFolders: []` on the wire and renders no add-folder control at
  // all, so the evidence must not reach into a folder list the request does
  // not carry — the layered fields have their own scans (`layerScans`), and
  // counting a folder twice would be the mistake this fix is preventing in
  // the other direction.
  it("plans a layered release from its tagged folders and names the ones it will not read", async () => {
    scanPerFolder();
    seedRemembered({
      ...FULL_FIELDS,
      "buildSession.release": "AmigaOS 3.2.2",
      "osinstall.extraMediaFolders.AmigaOS 3.2.2": [EXTRA],
      // `osinstall.mediaFolder.<layerId>.<release>` — the per-layer key
      // `seededMaterial` migrates into a tagged entry.
      "osinstall.mediaFolder.base.AmigaOS 3.2.2": "E:\\base322",
    });
    render(<OsInstall />);

    // Waits for the request made **once the recipe's layers have landed**:
    // `layers === []` is one value with two causes, and until `layersFor`
    // answers, a layered release honestly looks unlayered from here
    // (ART-256/ART-257's own `layersKnown` reasoning).
    await waitFor(() => {
      const last = planMock.mock.calls.at(-1)![0] as InstallRequest;
      expect(Object.keys(last.mediaFolders ?? {}).length).toBeGreaterThan(0);
    });
    const request = planMock.mock.calls.at(-1)![0] as InstallRequest;
    // A layered request is the map alone: `plan.rs` ignores the flat fields
    // outright for one.
    expect(request.extraMediaFolders).toEqual([]);
    expect(request.mediaFolder).toBe("");
    expect(request.mediaFolders).toEqual({ base: "E:\\base322" });

    // **The untagged folder is named, not dropped.** It is in the list, ART
    // resolves it in the readout, and the plan cannot carry it — a screen
    // that said nothing would be contradicting the core about what it is
    // going to do.
    const unused = await screen.findByTestId("material-unused");
    expect(unused.textContent).toContain(EXTRA);

    // **And its disks are not part of the release's evidence** — the
    // exclusion this test exists for (ART-256's scope trap), restored as an
    // assertion after the unified list made the old one untrue (round 2
    // whole-branch review, L9).
    //
    // The material list legitimately scans *every* folder now, because the
    // readout resolves against all of them, so "was this folder scanned" is
    // no longer the question. The question is what reaches the **evidence**
    // line, which `foundVolumeNames` filters to `plannedFolderPaths`: a
    // layered release's tagged folders alone. `E:\extra` holds `Extras3.2`
    // and is untagged, so that volume must never be asked about.
    await waitFor(() => expect(mediaEvidenceMock).toHaveBeenCalled());
    for (const call of mediaEvidenceMock.mock.calls) {
      expect(call[1]).not.toContain("Extras3.2");
    }
    // The positive half, so this is an exclusion and not an empty answer: the
    // tagged folder's own disk *is* evidence.
    expect(
      mediaEvidenceMock.mock.calls.some((call) =>
        (call[1] as string[]).includes("Workbench3.2")
      )
    ).toBe(true);
  });

  // M2 (fix wave 5, 2026-09-06 final review) — `foundVolumeNames`'s own doc
  // comment and `docs/FEATURES.md` both claim a disk held by two folders is
  // listed once. Nothing asserted it: deleting the dedup fold left the whole
  // suite green. This is the guard, and it asserts the specific listing
  // rather than a count, because "one name" and "the right name" are
  // different claims.
  it("lists a disk held by two folders once, not twice (M2)", async () => {
    scanMediaMock.mockReset().mockImplementation((folder: string) =>
      Promise.resolve(
        folder === EXTRA
          ? ({
              outcome: "found",
              media: [
                { path: `${EXTRA}\\Backup.adf`, volumeName: "Workbench3.2", kind: "floppy" },
              ],
            } satisfies MediaScanResult)
          : ({
              outcome: "found",
              media: [
                { path: "E:\\media\\Disk1.adf", volumeName: "Workbench3.2", kind: "floppy" },
              ],
            } satisfies MediaScanResult)
      )
    );
    planMock.mockReset().mockResolvedValue({
      outcome: "planned",
      plan: {
        release: "AmigaOS 3.2",
        items: [],
        refusals: [
          { refusal: "media-missing", component: "install-libs", volume_name: "Install3.2" },
        ],
        totalBytes: 0,
        totalFiles: 0,
        componentsOn: ["workbench-base"],
        mediaPaths: {},
        packages: [],
        packageMedia: {},
        userStartup: [],
        activations: [],
        mediaStamps: {},
        removals: [],
        layers: [],
      },
    } satisfies PlanResult);
    releaseForMediaMock.mockReset().mockResolvedValue("AmigaOS 3.2");
    mediaEvidenceMock.mockReset().mockResolvedValue({
      release: "AmigaOS 3.2",
      distinguishing: ["Workbench3.2"],
      shared: [],
      missingRequired: ["Install3.2"],
    });
    seedRemembered({ ...FULL_FIELDS, "osinstall.extraMediaFolders": [EXTRA] });
    render(<OsInstall />);

    // Named once — a sentence that named it twice would be a worse account
    // of the same disk, and `media-ambiguous` already says "two folders, one
    // name" properly, disk by disk, in the refusals list.
    expect(
      await screen.findByText(
        i18n.t("osinstall.evidence.sameRelease", {
          found: "Workbench3.2",
          missing: "Install3.2",
        })
      )
    ).toBeTruthy();

    // And Rust was asked about one name, not the same name twice.
    await waitFor(() =>
      expect(mediaEvidenceMock).toHaveBeenCalledWith("AmigaOS 3.2", ["Workbench3.2"])
    );
  });

  // Two folders can spell one volume differently — AmigaDOS folds case, ART's
  // own scan does not. The doc comment's own claim: folded case-insensitively,
  // first spelling kept. Asserted here rather than assumed, because "kept"
  // and "kept which one" are different claims and only one of them is tested
  // by the test above (identical spelling either way looks the same).
  it("keeps the first spelling when two folders name the same disk differently (M2)", async () => {
    scanMediaMock.mockReset().mockImplementation((folder: string) =>
      Promise.resolve(
        folder === EXTRA
          ? ({
              outcome: "found",
              media: [
                { path: `${EXTRA}\\Backup.adf`, volumeName: "WORKBENCH3.2", kind: "floppy" },
              ],
            } satisfies MediaScanResult)
          : ({
              outcome: "found",
              media: [
                { path: "E:\\media\\Disk1.adf", volumeName: "Workbench3.2", kind: "floppy" },
              ],
            } satisfies MediaScanResult)
      )
    );
    planMock.mockReset().mockResolvedValue({
      outcome: "planned",
      plan: {
        release: "AmigaOS 3.2",
        items: [],
        refusals: [
          { refusal: "media-missing", component: "install-libs", volume_name: "Install3.2" },
        ],
        totalBytes: 0,
        totalFiles: 0,
        componentsOn: ["workbench-base"],
        mediaPaths: {},
        packages: [],
        packageMedia: {},
        userStartup: [],
        activations: [],
        mediaStamps: {},
        removals: [],
        layers: [],
      },
    } satisfies PlanResult);
    releaseForMediaMock.mockReset().mockResolvedValue("AmigaOS 3.2");
    mediaEvidenceMock.mockReset().mockResolvedValue({
      release: "AmigaOS 3.2",
      distinguishing: ["Workbench3.2"],
      shared: [],
      missingRequired: ["Install3.2"],
    });
    seedRemembered({ ...FULL_FIELDS, "osinstall.extraMediaFolders": [EXTRA] });
    render(<OsInstall />);

    // The main folder's own spelling survives — it is scanned before the
    // added folder in `foundVolumeNames`'s own walk order.
    expect(
      await screen.findByText(
        i18n.t("osinstall.evidence.sameRelease", {
          found: "Workbench3.2",
          missing: "Install3.2",
        })
      )
    ).toBeTruthy();
    expect(screen.queryByText(/WORKBENCH3\.2/)).toBeNull();

    await waitFor(() =>
      expect(mediaEvidenceMock).toHaveBeenCalledWith("AmigaOS 3.2", ["Workbench3.2"])
    );
  });
});

describe("the release the user picks is the release the whole screen is on (ART-209, ART-211)", () => {
  // The owner chose AmigaOS 3.2 and the update-packages panel below still
  // offered BoingBags — 3.9's archives, of which 3.2 has none: "3.2 ile 3.9
  // secenekleri GUI'de karismis birbirine girmis."
  //
  // Two defects met here. The packages were never scoped by release at all
  // (ART-209), and the release this screen owned was a *different variable*
  // from the one the build session carried (ART-211) — so even a scoped
  // panel mounted from the OS Builder's own step routes, which read the
  // session, would have been handed the wrong answer.
  //
  // Asserted through what the panel actually asks for, not through what is
  // stored: a test reading the remembered key would pass against a screen
  // that stored the release correctly and still showed the other release's
  // packages.
  it("asks for the chosen release's packages, and asks again when it changes", async () => {
    seedRemembered({
      ...FULL_FIELDS,
      "buildSession.packages": { folder: "E:\\archives", chosen: [] },
    });
    render(<OsInstall />);

    await waitFor(() => expect(packagesMock).toHaveBeenCalled());

    // **Every** call, never "some call" — and that is not pedantry, it is
    // what a mutation caught. `PackagePanel` and `AmigaInstallPanel` both ask
    // this same question, so `toHaveBeenCalledWith(...)` is satisfied by
    // either one of them alone: hardcoding the release inside one panel left
    // the first version of this test green, because the other panel still
    // passed the right one. An assertion that one caller is correct says
    // nothing at all about the other.
    const releasesAsked = () => packagesMock.mock.calls.map((call) => call[1]);
    expect(releasesAsked().length).toBeGreaterThan(0);
    for (const asked of releasesAsked()) expect(asked).toBe("AmigaOS 3.2");
    // The archives folder the user actually chose, not the disks folder the
    // material list happens to start with (fix round 1, F1). What this test
    // is about is unchanged: the release, and that it is *every* caller's.
    expect(packagesMock).toHaveBeenCalledWith("E:\\archives", "AmigaOS 3.2");

    packagesMock.mockClear();
    const picker = screen.getByRole("combobox", {
      name: i18n.t("osinstall.release.label"),
    }) as HTMLSelectElement;
    await userEvent.selectOptions(picker, "AmigaOS 3.9");

    await waitFor(() => expect(packagesMock).toHaveBeenCalled());
    for (const asked of releasesAsked()) expect(asked).toBe("AmigaOS 3.9");
  });

  // **This one deliberately asserts on storage**, which the test above says
  // it will not do — because for ART-211 storage is not an implementation
  // detail, it *is* the channel. The OS Builder's step routes
  // (`pages/osbuilder/steps.tsx`) mount `PackagePanel` and
  // `AmigaInstallPanel` themselves and read `useBuildSession`, so the only
  // thing connecting the picker on this screen to what those steps offer is
  // the session's own remembered key. While this screen owned
  // `osinstall.release` instead, the picker moved one variable and the steps
  // read the other, and nothing in a test of this component alone could see
  // it.
  it("moves the build session's own release, which is what the steps read", async () => {
    seedRemembered(FULL_FIELDS);
    render(<OsInstall />);
    await waitFor(() => expect(planMock).toHaveBeenCalled());

    const picker = screen.getByRole("combobox", {
      name: i18n.t("osinstall.release.label"),
    }) as HTMLSelectElement;
    await userEvent.selectOptions(picker, "AmigaOS 3.9");

    await waitFor(() => {
      const bag = useSettingsStore.getState().settings.remembered as Record<string, unknown>;
      expect(bag["buildSession.release"]).toBe("AmigaOS 3.9");
    });
  });
});

describe("a refusal renders as a sentence, not a blank", () => {
  it("shows the real, translated refusal text", async () => {
    const refusedPlan: InstallPlan = {
      release: "AmigaOS 3.2",
      items: [],
      refusals: [REFUSAL],
      totalBytes: 0,
      totalFiles: 0,
      componentsOn: ["workbench-base", "install-libs"],
      mediaPaths: {},
      packages: [],
      packageMedia: {},
      userStartup: [],
      activations: [],
      mediaStamps: {},
      removals: [],
      layers: [],
    };
    planMock.mockReset().mockResolvedValue({ outcome: "planned", plan: refusedPlan } satisfies PlanResult);

    seedRemembered({
      "osinstall.mediaFolder": "E:\\media",
      "osinstall.destination": "E:\\dist",
    });
    render(<OsInstall />);

    const phrase = refusalPhrase(REFUSAL);
    const expectedSentence = i18n.t(phrase.key, phrase.params);
    // "workbench-base needs Workbench3.2, and no file in the media folder
    // carries that volume name." — a real sentence, computed the same way
    // the screen itself computes it, not a hand-typed copy that could drift.
    const rendered = await screen.findByText(expectedSentence);

    expect(rendered.textContent?.trim().length).toBeGreaterThan(0);
    expect(KEY_SHAPE.test(rendered.textContent ?? "")).toBe(false);
  });

  // **Final whole-branch review, Finding F.** This is the one refusal that
  // tells the user to go and tick the named component themselves — and a
  // raw recipe id ("modules-a1200") is not a checkbox a person can find on
  // screen; the checklist shows it by its label ("ModulesA1200_3.2", this
  // fixture's `COMPONENTS_32` entry, since it declares no `labelKey`).
  it("names the component to tick by its own label, never the raw recipe id", async () => {
    const refusal: RefusalReason = {
      refusal: "resident-table-unreadable",
      component: "modules-a1200",
      resident: "exec",
    };
    const refusedPlan: InstallPlan = {
      release: "AmigaOS 3.2",
      items: [],
      refusals: [refusal],
      totalBytes: 0,
      totalFiles: 0,
      componentsOn: ["workbench-base", "install-libs"],
      mediaPaths: {},
      packages: [],
      packageMedia: {},
      userStartup: [],
      activations: [],
      mediaStamps: {},
      removals: [],
      layers: [],
    };
    planMock
      .mockReset()
      .mockResolvedValue({ outcome: "planned", plan: refusedPlan } satisfies PlanResult);

    seedRemembered({
      "osinstall.mediaFolder": "E:\\media",
      "osinstall.destination": "E:\\dist",
    });
    render(<OsInstall />);

    // Computed the way the screen itself computes it (Finding F's own fix):
    // the raw id resolved to `COMPONENTS_32`'s `modules-a1200` entry's own
    // label, "ModulesA1200_3.2" (it declares no `labelKey`, so `label()`
    // falls back to the component's `media`).
    const phrase = refusalPhrase(refusal);
    const expectedSentence = i18n.t(phrase.key, { ...phrase.params, component: "ModulesA1200_3.2" });
    const rendered = await screen.findByText(expectedSentence);
    expect(rendered.textContent).not.toContain("modules-a1200");
  });
});

describe("the screen says what a layering component would replace (ART-175)", () => {
  // `collide::preview` has been able to answer for a release recipe's own
  // component since ART-170, and nothing asked it. These are the ask, from
  // the screen's side: the request that goes out, and the rows that come
  // back.

  /** Switch to AmigaOS 3.9, whose fixture carries a layering component. */
  async function render39() {
    await renderFull();
    const picker = screen.getByRole("combobox", {
      name: i18n.t("osinstall.release.label"),
    }) as HTMLSelectElement;
    await userEvent.selectOptions(picker, "AmigaOS 3.9");
    // The checklist is loaded, not hardcoded, so the 3.9 catalogue arrives on
    // its own round trip — wait for it rather than racing it.
    await waitFor(() =>
      expect(planMock).toHaveBeenCalledWith(expect.objectContaining({ release: "AmigaOS 3.9" }))
    );
  }

  it("asks about the layering components only, never the whole checklist", async () => {
    await render39();

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

    await render39();

    await screen.findByText(i18n.t("osinstall.replaces.heading"));
    // The row itself, with the version change read off both files.
    await screen.findByText("Libs/workbench.library");
    expect(document.querySelectorAll('[data-testid="component-collision-row"]').length).toBe(1);

    // And the sentence that keeps "nothing to report" from reading like
    // "nothing to place": three files placed, one of them over something.
    const summary = i18n.t("osinstall.replaces.summary", {
      components: "AmigaOS3.9",
      placed: 3,
      fresh: 1,
      unchanged: 1,
      replaced: 1,
    });
    expect(document.body.textContent).toContain(summary);

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

  it("says nothing at all when nothing layering is switched on", async () => {
    // AmigaOS 3.2's fixture has no component declaring an override, so the
    // section must not appear — and the engine must not be asked either.
    await renderFull();
    await waitFor(() => expect(planMock).toHaveBeenCalled());

    expect(screen.queryByText(i18n.t("osinstall.replaces.heading"))).toBeNull();
    expect(componentCollisionsMock).not.toHaveBeenCalled();
  });

  it("a preview that failed says so instead of looking like one that found nothing", async () => {
    componentCollisionsMock.mockRejectedValue(new Error("the disc could not be read"));

    await render39();

    // **No stray "Error: " any more** (ART-060). This used to render
    // `String(e)`, which on an `Error` object prepends the word "Error" to a
    // sentence that is already introduced as a failure — the user read it
    // twice. `errorText` takes the message.
    await screen.findByText(
      i18n.t("osinstall.replaces.failed", { error: "the disc could not be read" })
    );
    expect(document.querySelectorAll('[data-testid="component-collision-row"]').length).toBe(0);
  });
});

describe("a release switch does not leave the other release's answers on screen (ART-210)", () => {
  // The owner, driving the screen: "3.2 kurayım diyorsun, 3.9'un seçenekleri,
  // hataları vb ekranda duruyor asla değişmiyor."
  //
  // Only two effects on this screen depended on `release` — the one that
  // loads the component list and the one that re-plans. Everything *else* is
  // a `useState` nothing invalidated, so a finished install's report, a
  // failed preview, a plan error and a pending confirmation all survived a
  // switch and sat there describing an operating system the user had moved
  // away from. The plan updated underneath them, which made it worse: the
  // screen then showed one release's plan beside another release's result.

  /** Hand back the screen's own `osinstall-result` listener, so a finished
   *  install can be announced the way the backend announces one. */
  function captureAnnounce210(): { current: ((r: OsInstallResult) => void) | null } {
    const held: { current: ((r: OsInstallResult) => void) | null } = { current: null };
    onResultMock.mockImplementation((fn: (r: OsInstallResult) => void) => {
      held.current = fn;
      return Promise.resolve(() => {});
    });
    return held;
  }

  const FINISHED_39: OsInstallResult = {
    job_id: 1,
    destination: "E:\\amiga\\dist-3.9",
    outcome: {
      root: "E:\\amiga\\dist-3.9",
      files: 1915,
      directories: 75,
      bytes: 1024,
      removed: [],
      icons: [],
      iconMergeFailures: 0,
    extraMembers: [],
    postPlace: [],
    },
    stated_release: { verdict: "unstated" },
  };

  it("takes a finished install's report down when the release changes", async () => {
    const announce = captureAnnounce210();
    seedRemembered(FULL_FIELDS);
    render(<OsInstall />);
    await waitFor(() => expect(announce.current).not.toBeNull());

    act(() => announce.current!(FINISHED_39));
    expect(await screen.findByText(i18n.t("osinstall.result.heading"))).toBeTruthy();

    const picker = screen.getByRole("combobox", {
      name: i18n.t("osinstall.release.label"),
    }) as HTMLSelectElement;
    await userEvent.selectOptions(picker, "AmigaOS 3.9");

    await waitFor(() =>
      expect(screen.queryByText(i18n.t("osinstall.result.heading"))).toBeNull()
    );
  });

  it("takes an answer a control gave down when the release changes", async () => {
    // A second card, and a different mechanism on purpose: the report above
    // is set by an event from the backend, this one by a button on the
    // screen. One test that clears one piece of state proves one piece of
    // state.
    //
    // "Scan again" is the right second case because **nothing else can
    // clear it**. A plan error would be wiped by the re-plan a release
    // switch triggers anyway, so a test built on one would pass against the
    // defect exactly as happily as against the fix.
    seedRemembered(FULL_FIELDS);
    render(<OsInstall />);
    await waitFor(() => expect(planMock).toHaveBeenCalled());

    await userEvent.click(
      screen.getByRole("button", { name: i18n.t("osinstall.media.rescan") })
    );
    const rescanned = i18n.t("osinstall.media.rescanned", { count: 1 });
    expect(await screen.findByText(rescanned)).toBeTruthy();

    const picker = screen.getByRole("combobox", {
      name: i18n.t("osinstall.release.label"),
    }) as HTMLSelectElement;
    await userEvent.selectOptions(picker, "AmigaOS 3.9");

    await waitFor(() => expect(screen.queryByText(rescanned)).toBeNull());
  });
});

describe("the tree it builds is the tree the next steps get (ART-197)", () => {
  /** Hand back the screen's own `osinstall-result` listener, so a finished
   *  install can be announced the way the backend announces one. */
  function captureAnnounce(): { current: ((r: OsInstallResult) => void) | null } {
    const held: { current: ((r: OsInstallResult) => void) | null } = { current: null };
    onResultMock.mockImplementation((fn: (r: OsInstallResult) => void) => {
      held.current = fn;
      return Promise.resolve(() => {});
    });
    return held;
  }

  const FINISHED: OsInstallResult = {
    job_id: 1,
    destination: "E:\\amiga\\dist-3.9",
    outcome: {
      root: "E:\\amiga\\dist-3.9",
      files: 1915,
      directories: 75,
      bytes: 1024,
      removed: [],
      icons: [],
      iconMergeFailures: 0,
    extraMembers: [],
    postPlace: [],
    },
    stated_release: { verdict: "unstated" },
  };

  it("hands a finished install's destination to the session", async () => {
    // The defect: `OsInstall` remembered where it *wrote* under
    // `osinstall.destination`, and the panels beneath it read a different
    // key, `osinstall.packages.treeRoot`. Nothing joined them, so a user who
    // had just watched ART write 1915 files was asked to go and find them.
    const announce = captureAnnounce();
    seedRemembered(FULL_FIELDS);
    render(<OsInstall />);
    await waitFor(() => expect(announce.current).not.toBeNull());

    act(() => announce.current!(FINISHED));

    await waitFor(() => {
      const bag = useSettingsStore.getState().settings.remembered as Record<string, unknown>;
      expect(bag["buildSession.tree"]).toEqual({
        root: "E:\\amiga\\dist-3.9",
        builtHere: true,
      });
    });
  });

  it("says on screen which folder the next steps will act on", async () => {
    // A carry nobody can see is the same failure as a carry that does not
    // happen: the screen must not change what the next step points at
    // without saying so. This project's most expensive defects are the ones
    // that leave the reader not knowing what happened.
    const announce = captureAnnounce();
    seedRemembered(FULL_FIELDS);
    render(<OsInstall />);
    await waitFor(() => expect(announce.current).not.toBeNull());

    act(() => announce.current!(FINISHED));

    await screen.findByText(
      i18n.t("osinstall.result.carried", { root: "E:\\amiga\\dist-3.9" })
    );
    // Not the raw key, and not an unrendered interpolation.
    expect(screen.queryByText(/osinstall\.result\.carried/)).toBeNull();
    expect(screen.queryByText(/\{\{/)).toBeNull();
  });

  it("carries a tree the user picked by hand, without a build", async () => {
    // The migration's own case, at the screen: a user upgrading into this
    // build finds the packages panel pointing where they last pointed it.
    seedRemembered({
      ...FULL_FIELDS,
      "osinstall.packages.treeRoot": "E:\\amiga\\picked-by-hand",
    });
    render(<OsInstall />);

    // `PackagePanel` and `AmigaInstallPanel` each render the tree root through
    // `Field`, as plain text beside their Browse button - so the path appears
    // **twice**, once per panel, and that is the point: both are reading the
    // one session value. `findAllByText`, because `findByText` refuses a
    // multiple match.
    //
    // It was briefly three, while `VerifyAgainstCard` still lived on this
    // screen. Wave 3 moved that section to the volumes step, where the card it
    // compares against actually is, and it reads the same session value there
    // - which is what makes the move cost nothing.
    const shown = await screen.findAllByText("E:\\amiga\\picked-by-hand");
    expect(shown.length).toBe(2);
  });
});

// ---------------------------------------------------------------------------
// Task 9: what the tree's own release marker says, on its own line
// ---------------------------------------------------------------------------
//
// Three distinct sentences, never folded into one — CLAUDE.md's answer to
// the round that shipped AmigaOS 3.5 labelled 3.9. Each test below sends a
// different `stated_release` verdict and asserts the one sentence it names,
// never the other two: collapsing "confirmed" and "mismatch" into a shared
// wording would still pass a test that only checked *a* sentence appeared.

describe("Task 9: the tree's own release marker gets its own line", () => {
  function captureAnnounce(): { current: ((r: OsInstallResult) => void) | null } {
    const held: { current: ((r: OsInstallResult) => void) | null } = { current: null };
    onResultMock.mockImplementation((fn: (r: OsInstallResult) => void) => {
      held.current = fn;
      return Promise.resolve(() => {});
    });
    return held;
  }

  const BASE_OUTCOME = {
    root: "E:\\amiga\\dist",
    files: 10,
    directories: 2,
    bytes: 1024,
    removed: [],
    icons: [],
    iconMergeFailures: 0,
    extraMembers: [],
    postPlace: [],
  };

  it("reports a confirmed marker by its own text", async () => {
    const announce = captureAnnounce();
    seedRemembered(FULL_FIELDS);
    render(<OsInstall />);
    await waitFor(() => expect(announce.current).not.toBeNull());

    act(() =>
      announce.current!({
        job_id: 1,
        destination: "E:\\amiga\\dist",
        outcome: BASE_OUTCOME,
        stated_release: { verdict: "confirmed", stated: "Release 3.2.2" },
      })
    );

    await screen.findByText(
      i18n.t("osinstall.result.statedRelease.confirmed", { stated: "Release 3.2.2" })
    );
    expect(
      screen.queryByText(i18n.t("osinstall.result.statedRelease.unstated"))
    ).toBeNull();
  });

  // **The mutation table's third row.** Naming both sides is the point of
  // this sentence — the frontend key this pins is exactly what collapsing
  // the three sentences into one would break.
  it("names both sides of a mismatch", async () => {
    const announce = captureAnnounce();
    seedRemembered(FULL_FIELDS);
    render(<OsInstall />);
    await waitFor(() => expect(announce.current).not.toBeNull());

    act(() =>
      announce.current!({
        job_id: 1,
        destination: "E:\\amiga\\dist",
        outcome: BASE_OUTCOME,
        stated_release: {
          verdict: "mismatch",
          expected: "Release 3.2.2",
          stated: "Release 3.2",
        },
      })
    );

    await screen.findByText(
      i18n.t("osinstall.result.statedRelease.mismatch", {
        expected: "Release 3.2.2",
        stated: "Release 3.2",
      })
    );
    expect(screen.queryByText(/\{\{/)).toBeNull();
  });

  it("says a tree with no marker states none, not a guess", async () => {
    const announce = captureAnnounce();
    seedRemembered(FULL_FIELDS);
    render(<OsInstall />);
    await waitFor(() => expect(announce.current).not.toBeNull());

    act(() =>
      announce.current!({
        job_id: 1,
        destination: "E:\\amiga\\dist",
        outcome: BASE_OUTCOME,
        stated_release: { verdict: "unstated" },
      })
    );

    await screen.findByText(i18n.t("osinstall.result.statedRelease.unstated"));
  });

  // **Fix round 1, Finding 1.** An unreadable marker must never render the
  // same sentence as "states none" — the two are different facts with
  // different next steps, and folding them is the exact defect this task
  // exists to catch.
  it("says the marker could not be read, never the same sentence as unstated", async () => {
    const announce = captureAnnounce();
    seedRemembered(FULL_FIELDS);
    render(<OsInstall />);
    await waitFor(() => expect(announce.current).not.toBeNull());

    act(() =>
      announce.current!({
        job_id: 1,
        destination: "E:\\amiga\\dist",
        outcome: BASE_OUTCOME,
        stated_release: {
          verdict: "unreadable",
          detail: "malformed release marker: too many bytes",
        },
      })
    );

    await screen.findByText(
      i18n.t("osinstall.result.statedRelease.unreadable", {
        detail: "malformed release marker: too many bytes",
      })
    );
    expect(
      screen.queryByText(i18n.t("osinstall.result.statedRelease.unstated"))
    ).toBeNull();
  });

  // **Final whole-branch review, Finding E.** A differing marker for a
  // release ART has never measured (AmigaOS 3.9 today) must render its own
  // sentence, and never the "mismatch" wording — that would tell the user
  // their correct tree is wrong for a formula nobody has checked.
  it("reports a differing marker for an unmeasured release plainly, never as a mismatch", async () => {
    const announce = captureAnnounce();
    seedRemembered(FULL_FIELDS);
    render(<OsInstall />);
    await waitFor(() => expect(announce.current).not.toBeNull());

    act(() =>
      announce.current!({
        job_id: 1,
        destination: "E:\\amiga\\dist",
        outcome: BASE_OUTCOME,
        stated_release: { verdict: "expected-unknown", stated: "Release 3.5" },
      })
    );

    await screen.findByText(
      i18n.t("osinstall.result.statedRelease.expectedUnknown", { stated: "Release 3.5" })
    );
    expect(
      screen.queryByText(
        i18n.t("osinstall.result.statedRelease.mismatch", {
          expected: "Release 3.9",
          stated: "Release 3.5",
        })
      )
    ).toBeNull();
  });
});

// ---------------------------------------------------------------------------
// Work-list item 8: media in more than one folder
// ---------------------------------------------------------------------------
//
// AmigaOS 3.2.2.1 is the user's own 3.2 ADFs plus the update disks plus the
// hotfix disk, and Hyperion ships the last two as `ADFs/Update/` and
// `ADFs/Hotfix/` inside a single download. Until this existed, whichever
// folder they named, every component from the other one came back
// `MediaMissing` -- so the install could not be expressed at all.

describe("the one material folder list (design § 3.1)", () => {
  it("sends every folder in the list to the planner, not only the first", async () => {
    await renderFull();
    planMock.mockClear();

    await addFolder("E:\\media\\Update");

    await waitFor(() => expect(planMock).toHaveBeenCalled());
    const sent = planMock.mock.calls.at(-1)![0] as InstallRequest;
    expect(sent.mediaFolder).toBe("E:\\media");
    expect(sent.extraMediaFolders).toEqual(["E:\\media\\Update"]);
  });

  it("shows one row per folder, so the user can see what ART will read", async () => {
    await renderFull();
    expect(screen.getAllByTestId("material-folder")).toHaveLength(1);

    await addFolder("E:\\media\\Hotfix");

    const rows = await screen.findAllByTestId("material-folder");
    expect(rows).toHaveLength(2);
    expect(rows[1].textContent).toContain("E:\\media\\Hotfix");
  });

  it("takes one back out again", async () => {
    await renderFull();
    await addFolder("E:\\media\\Update");

    planMock.mockClear();
    await userEvent.click(
      screen.getByRole("button", {
        name: i18n.t("osinstall.media.removeFolderAriaLabel", { folder: "E:\\media\\Update" }),
      })
    );

    await waitFor(() => expect(screen.queryAllByTestId("material-folder")).toHaveLength(1));
    await waitFor(() => expect(planMock).toHaveBeenCalled());
    const sent = planMock.mock.calls.at(-1)![0] as InstallRequest;
    expect(sent.extraMediaFolders).toEqual([]);
  });

  // ART-240 (found by the media-step accessibility sweep filed alongside
  // ART-237): with more than one row, every "Remove" button used to carry
  // the identical accessible name "Remove" — a screen reader user tabbing
  // through them could not tell which row any one of them belonged to.
  it("names which folder each Remove button removes, once there is more than one", async () => {
    await renderFull();
    await addFolder("E:\\media\\Update");
    await addFolder("E:\\media\\Hotfix");
    await waitFor(() => expect(screen.queryAllByTestId("material-folder")).toHaveLength(3));

    const removeUpdate = screen.getByRole("button", { name: /remove.*update/i });
    const removeHotfix = screen.getByRole("button", { name: /remove.*hotfix/i });
    expect(removeUpdate).not.toBe(removeHotfix);

    planMock.mockClear();
    await userEvent.click(removeUpdate);

    await waitFor(() => expect(planMock).toHaveBeenCalled());
    const sent = planMock.mock.calls.at(-1)![0] as InstallRequest;
    // Removing the row named "Update" left "Hotfix" behind — proof the
    // accessible name actually picked out the right row, not just any one
    // with visible text "Remove".
    expect(sent.extraMediaFolders).toEqual(["E:\\media\\Hotfix"]);
  });

  it("does not add the same folder twice, in any spelling", async () => {
    // The core reads a folder named twice exactly once; a screen that showed
    // it twice would be contradicting the core about what it is going to do,
    // and `find_media_across` would refuse every disk in it as ambiguous with
    // itself.
    await renderFull();
    await addFolder("E:\\media\\Update");
    expect(screen.queryAllByTestId("material-folder")).toHaveLength(2);

    dialogOpenMock.mockResolvedValueOnce("E:\\media\\Update");
    await userEvent.click(screen.getByTestId("material-add-folder"));
    expect(screen.queryAllByTestId("material-folder")).toHaveLength(2);

    // Case and separators are not two folders on Windows (`canonicalFolder`).
    dialogOpenMock.mockResolvedValueOnce("e:/MEDIA/");
    await userEvent.click(screen.getByTestId("material-add-folder"));
    expect(screen.queryAllByTestId("material-folder")).toHaveLength(2);
  });

  /// Per release, like everything else keyed by one (ART-207): a 3.2.2.1
  /// install's update folder means nothing to a 3.9 one.
  it("remembers the list per release", async () => {
    seedRemembered({
      ...FULL_FIELDS,
      "osinstall.extraMediaFolders": ["E:\\media\\Update"],
    });
    render(<OsInstall />);
    await screen.findByText(i18n.t("osinstall.plan.heading"));
    // The 3.2 list migrates to two folders: its flat one and its extra.
    expect(screen.queryAllByTestId("material-folder")).toHaveLength(2);

    await userEvent.selectOptions(
      screen.getByRole("combobox", { name: i18n.t("osinstall.release.label") }),
      "AmigaOS 3.9"
    );
    // 3.9's own list is its own flat folder alone.
    await waitFor(() => expect(screen.queryAllByTestId("material-folder")).toHaveLength(1));
    expect(screen.getAllByTestId("material-folder")[0].textContent).toContain("E:\\media39");
  });

  /// **The legacy keys are read once and never written again.** A migration
  /// that wrote back would make the old keys a second live store of the same
  /// folders, and a user who rolled back to an earlier ART would find their
  /// list edits half-applied there.
  it("edits the list without ever writing a legacy key", async () => {
    await renderFull();
    const before = rememberedBag();
    expect(before["osinstall.mediaFolder"]).toBe("E:\\media");

    await addFolder("E:\\media\\Update");
    await userEvent.click(
      screen.getByRole("button", {
        name: i18n.t("osinstall.media.removeFolderAriaLabel", { folder: "E:\\media" }),
      })
    );
    await waitFor(() => expect(screen.queryAllByTestId("material-folder")).toHaveLength(1));

    const after = rememberedBag();
    // The list is the session's own key, and it holds the edit.
    expect(after["buildSession.material.AmigaOS 3.2"]).toEqual({
      folders: [{ path: "E:\\media\\Update", layer: null }],
    });
    // Every legacy key is exactly as it was.
    expect(after["osinstall.mediaFolder"]).toBe("E:\\media");
    expect(after["osinstall.extraMediaFolders"]).toBeUndefined();
    // Neither the global migration source nor this release's own key was
    // written: the archives folder here was only ever the list's.
    expect(after["buildSession.packages"]).toBeUndefined();
    expect(after["buildSession.packages.AmigaOS 3.2"]).toBeUndefined();
  });
});

// ---------------------------------------------------------------------------
// Amiga Forever, offered — design § 3.5
// ---------------------------------------------------------------------------
//
// `AMIGAFOREVERDATA` is set by Amiga Forever itself, so ART can find the
// disks without asking. What it may not do is *use* them: adding a folder
// nobody chose is the settings-change-without-a-user the remembered-settings
// rule forbids outright.

describe("archives in one folder, disks in another (fix round 1, F1)", () => {
  /// **The configuration an upgrade must not lose.** A user with
  /// `osinstall.mediaFolder = E:\disks` and `buildSession.packages.folder =
  /// E:\archives` has both in the one material list after the migration, in
  /// that order — but the two package panels take *one* folder each, and
  /// handing them the list's head means `osinstallPackages` finds no archives
  /// and their already-chosen packages sit above a catalogue that cannot see
  /// them.
  it("keeps the archives folder for the package panels after the upgrade", async () => {
    seedRemembered({
      "osinstall.mediaFolder": "E:\\disks",
      "osinstall.rom": "E:\\roms\\kick.rom",
      "osinstall.destination": "E:\\dist",
      "buildSession.packages": { folder: "E:\\archives", chosen: [] },
    });
    render(<OsInstall />);

    await waitFor(() => expect(packagesMock).toHaveBeenCalled());
    // **Every** call, never "some call": `PackagePanel` and
    // `AmigaInstallPanel` both ask, and one of them being right says nothing
    // about the other.
    for (const call of packagesMock.mock.calls) expect(call[0]).toBe("E:\\archives");

    // And both folders really are in the one list, which is what makes the
    // readout and the planner see the archives at all.
    const rows = await screen.findAllByTestId("material-folder");
    expect(rows.map((row) => row.textContent)).toEqual([
      expect.stringContaining("E:\\disks"),
      expect.stringContaining("E:\\archives"),
    ]);
  });

  /// **The half the report missed.** `PackagePanel`'s own picker calls
  /// `onPackageFolderChange` → `setPackages({ folder })`. While that only
  /// appended to the material list, the derived value stayed the list's
  /// *first* entry: the user browsed to a folder, the field went on showing
  /// another, and nothing they could do on that step fixed it. "Nothing
  /// changes unless the user changes it" running backwards.
  it("follows the packages panel's own Browse", async () => {
    await renderFull();
    await waitFor(() => expect(packagesMock).toHaveBeenCalled());
    packagesMock.mockClear();

    dialogOpenMock.mockResolvedValueOnce("E:\\archives");
    await userEvent.click(
      within(await screen.findByTestId("package-folder-field")).getByRole("button")
    );

    await waitFor(() => expect(packagesMock).toHaveBeenCalledWith("E:\\archives", "AmigaOS 3.2"));
    // The pick is stored as the packages step's own value, **under this
    // release's own key** (round 2 review, M3): the archives folder is a
    // folder for a build, and a build is per release.
    expect(rememberedBag()["buildSession.packages.AmigaOS 3.2"]).toMatchObject({
      folder: "E:\\archives",
    });
    // … and the folder is in the one list too, so the readout and the
    // planner see it.
    const rows = await screen.findAllByTestId("material-folder");
    expect(rows.map((row) => row.textContent)).toEqual([
      expect.stringContaining("E:\\media"),
      expect.stringContaining("E:\\archives"),
    ]);
  });

  /// A user who never kept a separate archives folder gets the one list's
  /// answer for free — which is the whole point of deriving at all.
  it("derives the list's first folder when nothing was ever stored", async () => {
    seedRemembered({
      "osinstall.mediaFolder": "E:\\media",
      "osinstall.rom": "E:\\roms\\kick.rom",
      "osinstall.destination": "E:\\dist",
    });
    render(<OsInstall />);
    await waitFor(() => expect(packagesMock).toHaveBeenCalled());
    for (const call of packagesMock.mock.calls) expect(call[0]).toBe("E:\\media");
  });
});

describe("the readout re-asks once the identification pass has landed (fix round 1, F2)", () => {
  /// `osinstall_slots` hashes nothing: it reads the scan cache
  /// `osinstall_identify_media` fills. Nothing in the readout's dependency
  /// list changed when that job finished, so on a first visit with a cold
  /// cache the readout said "nobody has read its bytes yet — identify this
  /// folder by content" directly above a section reporting that it had just
  /// hashed them. Two halves of one screen disagreeing about the same files.
  ///
  /// Asserted on the *wiring* — that the readout is asked a second time
  /// once the pass settles — because `MaterialReadout.test.tsx` owns the
  /// other half (that the sentence really changes when it is).
  it("asks a second time when the pass settles, and not before", async () => {
    let settleIdentify: (value: MediaIdentification) => void = () => {};
    identifyMediaMock.mockReset().mockReturnValue(
      new Promise<MediaIdentification>((resolve) => {
        settleIdentify = resolve;
      })
    );

    seedRemembered(FULL_FIELDS);
    render(<OsInstall />);

    // The readout has asked once, off a cold cache, while the pass runs.
    await waitFor(() => expect(slotsMock).toHaveBeenCalled());
    const beforeSettle = slotsMock.mock.calls.length;
    expect(await screen.findByTestId("media-identity")).toBeTruthy();
    expect(slotsMock.mock.calls.length).toBe(beforeSettle);

    settleIdentify({
      matches: [
        {
          path: "E:\\media\\Disk1.adf",
          volumeName: "Workbench3.2",
          row: null,
          md5: "0".repeat(32),
          confirmed: null,
        },
      ],
      unreadable: [],
      hashed: 1,
      remembered: 0,
      skipped: [],
    });

    // Now the cache holds something it did not, so the readout asks again.
    await waitFor(() => expect(slotsMock.mock.calls.length).toBeGreaterThan(beforeSettle));
    // The same folders, not a different question — this is a re-ask, not a
    // second, different readout.
    expect(slotsMock.mock.calls.at(-1)![1]).toEqual(slotsMock.mock.calls[0][1]);
  });
});

describe("ART-241: a row's controls are described by the row's own paragraphs", () => {
  /// The fix landed on the `Field`s these rows replaced, and the one-list
  /// round removed both `describedBy` call sites with them (fix round 1, F4).
  /// `docs/ISSUES.md` records ART-241 as Fixed naming exactly these two
  /// paragraphs; the entry stays Fixed only while it is true.
  it("names the unreadable paragraph on the row that could not be read", async () => {
    const BAD = "E:\\gone";
    scanMediaMock.mockReset().mockImplementation((folder: string) =>
      Promise.resolve(
        folder === BAD
          ? ({ outcome: "folder-unreadable", folder: BAD } satisfies MediaScanResult)
          : ({
              outcome: "found",
              media: [{ path: "E:\\media\\Disk1.adf", volumeName: "Workbench3.2", kind: "floppy" }],
            } satisfies MediaScanResult)
      )
    );
    await renderFull();
    await addFolder(BAD);

    const rows = await screen.findAllByTestId("material-folder");
    expect(rows).toHaveLength(2);
    await waitFor(() => expect(screen.queryByTestId("material-folder-unreadable-1")).toBeTruthy());

    expect(
      within(rows[1])
        .getByRole("button", { name: /^remove /i })
        .getAttribute("aria-describedby")
    ).toBe("material-folder-unreadable-1");
    // The readable row names nothing — a description that is always there
    // is not a description.
    expect(
      within(rows[0])
        .getByRole("button", { name: /^remove /i })
        .getAttribute("aria-describedby")
    ).toBeNull();
  });
});

describe("Amiga Forever, offered and never added", () => {
  const AF = "E:\\amiga\\Shared\\adf";
  // The ROM half of this offer moved to `MachineTab.test.tsx` on 2026-09-09
  // with the Kickstart field it answers — three cases, by name.

  /** An empty material list plus a host that has Amiga Forever. */
  function renderWithOffer() {
    amigaForeverMock.mockResolvedValue({ adf: AF, rom: null });
    seedRemembered({});
    return render(<OsInstall />);
  }

  it("offers the folder, and adds nothing at all until the click", async () => {
    renderWithOffer();
    const offer = await screen.findByTestId("amiga-forever-offer");
    expect(offer.textContent).toContain(AF);

    // **Nothing is in the session.** Asserted against the store rather than
    // the screen: a readout that happened not to render the row would pass a
    // DOM-only check while the folder had already been written.
    expect(
      rememberedBag()["buildSession.material.AmigaOS 3.2"]
    ).toBeUndefined();
    expect(screen.queryAllByTestId("material-folder")).toHaveLength(0);
  });

  it("adds it on the click, and stops offering once the list is not empty", async () => {
    renderWithOffer();
    await screen.findByTestId("amiga-forever-offer");

    await userEvent.click(
      screen.getByRole("button", { name: i18n.t("osinstall.material.amigaForeverAdd") })
    );

    const rows = await screen.findAllByTestId("material-folder");
    expect(rows).toHaveLength(1);
    expect(rows[0].textContent).toContain(AF);
    expect(screen.queryByTestId("amiga-forever-offer")).toBeNull();
  });

  it("goes away when dismissed, without adding anything and without remembering the refusal", async () => {
    renderWithOffer();
    await screen.findByTestId("amiga-forever-offer");

    await userEvent.click(
      screen.getByRole("button", { name: i18n.t("osinstall.material.amigaForeverDismiss") })
    );

    await waitFor(() => expect(screen.queryByTestId("amiga-forever-offer")).toBeNull());
    expect(screen.queryAllByTestId("material-folder")).toHaveLength(0);
    // **A suggestion is not a setting.** Nothing about the dismissal is
    // written anywhere: remembering "they said no once" would be storing a
    // choice about every future build from one click.
    const bag = rememberedBag();
    expect(Object.keys(bag).filter((key) => key.includes("amigaForever"))).toEqual([]);
  });

  it("says nothing at all on a machine that does not have it", async () => {
    seedRemembered({});
    render(<OsInstall />);
    await screen.findByTestId("material-folders");
    await waitFor(() => expect(amigaForeverMock).toHaveBeenCalled());
    expect(screen.queryByTestId("amiga-forever-offer")).toBeNull();
  });

  it("says nothing when the user has already pointed ART somewhere", async () => {
    amigaForeverMock.mockResolvedValue({ adf: AF, rom: null });
    await renderFull();
    await waitFor(() => expect(amigaForeverMock).toHaveBeenCalled());
    expect(screen.queryByTestId("amiga-forever-offer")).toBeNull();
  });
});

describe("the readout first (simplification design § 4)", () => {
  /// **The answer before the question.** The owner's third cause on
  /// 2026-09-08: *which folder holds what* was built and sat two hundred
  /// lines under the folder list. First in the DOM is first for a screen
  /// reader and first at a narrow width, so the DOM order is the guard, not
  /// a pixel position jsdom cannot measure.
  it("puts the readout before the folder list in document order", async () => {
    await renderFull();
    const readout = await screen.findByTestId("material-readout");
    const folders = screen.getByTestId("material-folders");
    expect(readout.compareDocumentPosition(folders) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    // Both inside the one row, so a narrow window stacks them rather than
    // scattering them.
    const row = screen.getByTestId("source-columns");
    expect(row.contains(readout)).toBe(true);
    expect(row.contains(folders)).toBe(true);
  });

  /// A question nobody has asked is not an answer of "nothing found". With
  /// no folder the readout renders nothing (its own rule), and what stands
  /// in its place is the next action — never a set line, which would be a
  /// claim about the user's disk.
  it("asks for a folder rather than reporting on none", async () => {
    seedRemembered({});
    render(<OsInstall />);
    const ask = await screen.findByTestId("material-ask");
    expect(ask.textContent).toBe(i18n.t("osinstall.material.askFolders"));
    expect(screen.queryByTestId("material-set-line")).toBeNull();
    expect(screen.queryByTestId("material-readout")).toBeNull();
    // The two sentences do not share a place: the list's own "no folder
    // chosen" line stays in the folder column.
    expect(screen.getByTestId("source-secondary").textContent).toContain(
      i18n.t("osinstall.media.none")
    );
    expect(screen.getByTestId("source-primary").textContent).not.toContain(
      i18n.t("osinstall.media.none")
    );
  });

  it("drops the ask the moment a folder is in the list", async () => {
    await renderFull();
    await screen.findByTestId("material-readout");
    expect(screen.queryByTestId("material-ask")).toBeNull();
  });

  it("renders the ask in Turkish as a sentence, not a key", async () => {
    await changeLanguage("tr");
    seedRemembered({});
    render(<OsInstall />);
    const ask = await screen.findByTestId("material-ask");
    expect(ask.textContent).toBe(i18n.t("osinstall.material.askFolders"));
    expect(ask.textContent).not.toContain("osinstall.");
    expect(ask.textContent).not.toContain("{{");
  });
});

// ---------------------------------------------------------------------------
// ART-226's other half: choosing the keyboard, having placed it
// ---------------------------------------------------------------------------
//
// The owner built a Turkish tree, watched its menus render ç ü ş Ğ, and then
// could not type them: every keymap was placed and none was selected. The
// picker's options are read off the plan's own items, so a layout it offers
// cannot then be refused for not being there.

describe("choosing the keyboard the system boots with", () => {
  it("says nothing when the install places no keymaps", async () => {
    await renderFull();
    expect(screen.queryByTestId("keymap-section")).toBeNull();
  });

  it("offers what the plan will really place, and not the icons", async () => {
    seedRemembered({ ...FULL_FIELDS, "osinstall.chosen": ["keymaps"] });
    render(<OsInstall />);
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
    seedRemembered({ ...FULL_FIELDS, "osinstall.chosen": ["keymaps"] });
    render(<OsInstall />);
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
    seedRemembered({ ...FULL_FIELDS, "osinstall.chosen": ["keymaps"] });
    render(<OsInstall />);
    await screen.findByTestId("keymap-section");

    const sent = planMock.mock.calls.at(-1)![0] as InstallRequest;
    expect(sent.keymap).toBeNull();
  });

  /// Per release, like the media folder (ART-207): a layout is a name in
  /// *that* release's `Devs/Keymaps`.
  it("remembers the choice per release", async () => {
    seedRemembered({
      ...FULL_FIELDS,
      "osinstall.chosen": ["keymaps"],
      "osinstall.keymap": "türkçe",
      "osinstall.chosen.AmigaOS 3.9": ["keymaps"],
      "osinstall.keymap.AmigaOS 3.9": "",
    });
    render(<OsInstall />);
    await screen.findByTestId("keymap-section");
    expect(
      (screen.getByRole("combobox", { name: /keyboard layout/i }) as HTMLSelectElement).value
    ).toBe("türkçe");

    await userEvent.selectOptions(
      screen.getByRole("combobox", { name: i18n.t("osinstall.release.label") }),
      "AmigaOS 3.9"
    );
    await waitFor(() =>
      expect(
        (screen.getByRole("combobox", { name: /keyboard layout/i }) as HTMLSelectElement).value
      ).toBe("")
    );
  });
});

// ART-207's own rule taken one level finer: instead of one media folder plus
// a bag of extra ones, a layered recipe (AmigaOS 3.2.2's own `base` and
// `update-3.2.2`) is asked one **labelled** question per layer it declares.
// `layersFor` carries that shape from the recipe (Task 8's own `label_key`s)
// to the screen; an unlayered release answers empty, and the screen must
// render exactly what it always has for one.
describe("saying which part of a layered release a folder holds (Task 10)", () => {
  it("offers every layer the recipe declares, in the recipe's own order", async () => {
    renderOsInstall({ release: "AmigaOS 3.2.2" });
    await addFolder("E:\\media\\3.2");

    const select = (await screen.findByRole("combobox", {
      name: i18n.t("osinstall.material.layerAriaLabel", { folder: "E:\\media\\3.2" }),
    })) as HTMLSelectElement;
    // Declaration order, not merely presence — a screen offering the
    // recipe's own layers in reverse would still pass a contains check. The
    // first option is "scan it for everything", which is what an untagged
    // folder means.
    expect([...select.querySelectorAll("option")].map((o) => o.textContent)).toEqual([
      i18n.t("osinstall.material.layerAny"),
      layerFieldLabel("base"),
      layerFieldLabel("update-3.2.2"),
    ]);
  });

  it("sends one folder per layer", async () => {
    const sent = await planWithFolders({
      base: "E:\\media\\3.2",
      "update-3.2.2": "E:\\media\\Update3.2.2",
    });
    expect(sent.mediaFolders).toEqual({
      base: "E:\\media\\3.2",
      "update-3.2.2": "E:\\media\\Update3.2.2",
    });
  });

  it("offers no layer at all for an unlayered release", async () => {
    renderOsInstall({ release: "AmigaOS 3.2" });
    await addFolder("E:\\media");
    // A release whose recipe declares no layers has no part to ask about, so
    // the row is a path and a Remove button and nothing else.
    expect(screen.queryAllByRole("combobox", { name: /E:..media/ })).toHaveLength(0);
  });

  it("remembers each folder's own layer across a remount", async () => {
    // ART's standing rule: nothing changes unless the user changes it — and
    // per folder, since a tag that moved would hand the update part the base
    // disks the first time somebody switched releases.
    const { unmount } = renderOsInstall({ release: "AmigaOS 3.2.2" });
    await browseLayerFolder("base", "E:\\a");
    await browseLayerFolder("update-3.2.2", "E:\\b");

    // Remount **without re-seeding**: `renderOsInstall`'s own
    // `seedRemembered` replaces the whole remembered bag, which would defeat
    // the point of this test by wiping the very writes it is asking about. A
    // real remount reads back whatever `settings.json` actually holds.
    unmount();
    render(<OsInstall />);

    await screen.findByText("E:\\a");
    expect(rememberedBag()["buildSession.material.AmigaOS 3.2.2"]).toEqual({
      folders: [
        { path: "E:\\a", layer: "base" },
        { path: "E:\\b", layer: "update-3.2.2" },
      ],
    });
  });

  // Fix round 1, Finding 1: the mistake this screen invites most — the
  // update disks tagged as the base part, or the reverse — is still
  // "AmigaOS 3.2.2 media" at the release level, so `wrongMediaFolder` and
  // `osinstallReleaseForMedia` cannot see it. `layerForMedia` is the
  // per-layer question that can.
  /// **Two rows, one tag** (fix round 1, F5). `unusedForPlan` exists because
  /// this state is reachable, and keying the hint by the layer id gave two
  /// DOM elements one `id` — an accessibility fault in its own right — with
  /// the second row rendering a sentence computed from the *first* row's
  /// scan. A hint is a claim about the folder in front of it.
  it("computes each row's hint from that row's own folder, under its own id", async () => {
    const WRONG = "E:\\media\\Update3.2.2";
    const RIGHT = "E:\\media\\3.2";
    scanMediaMock.mockReset().mockImplementation((folder: string) =>
      Promise.resolve(
        folder === WRONG
          ? ({
              outcome: "found",
              media: [{ path: "E:\\x", volumeName: "Update3.2.2", kind: "floppy" }],
            } satisfies MediaScanResult)
          : ({
              outcome: "found",
              media: [{ path: "E:\\y", volumeName: "Workbench3.2", kind: "floppy" }],
            } satisfies MediaScanResult)
      )
    );
    layerForMediaMock
      .mockReset()
      .mockImplementation((_release: string, names: string[]) =>
        Promise.resolve(names.includes("Update3.2.2") ? "update-3.2.2" : "base")
      );

    renderOsInstall({ release: "AmigaOS 3.2.2" });
    // Both rows tagged `base`: the first holds the base disks, the second
    // holds the update disks. Only the second is wrong.
    await browseLayerFolder("base", RIGHT);
    await browseLayerFolder("base", WRONG);

    // Exactly one hint, and it is the second row's.
    await waitFor(() => expect(screen.queryAllByTestId(/^layer-wrong-hint-/)).toHaveLength(1));
    const hint = screen.getByTestId("layer-wrong-hint-1");
    expect(hint.textContent).toContain("Update3.2.2");
    expect(hint.textContent).toContain(layerFieldLabel("update-3.2.2"));

    // ART-241: the row's own controls name it, so a screen reader user hears
    // the warning with the control rather than hunting forward for it.
    const rows = screen.getAllByTestId("material-folder");
    expect(
      within(rows[1])
        .getByRole("button", { name: /^remove /i })
        .getAttribute("aria-describedby")
    ).toBe("layer-wrong-hint-1");
    // And the row that is right names nothing.
    expect(
      within(rows[0])
        .getByRole("button", { name: /^remove /i })
        .getAttribute("aria-describedby")
    ).toBeNull();
  });

  it("warns a row when its folder holds a different part's own disks", async () => {
    const UPDATE_FOLDER = "E:\\media\\Update3.2.2";
    scanMediaMock.mockReset().mockImplementation((folder: string) =>
      Promise.resolve(
        folder === UPDATE_FOLDER
          ? ({
              outcome: "found",
              media: [{ path: "E:\\x", volumeName: "Update3.2.2", kind: "floppy" }],
            } satisfies MediaScanResult)
          : ({ outcome: "found", media: [] } satisfies MediaScanResult)
      )
    );
    layerForMediaMock
      .mockReset()
      .mockImplementation((_release: string, names: string[]) =>
        Promise.resolve(names.includes("Update3.2.2") ? "update-3.2.2" : null)
      );

    renderOsInstall({ release: "AmigaOS 3.2.2" });
    // The mistake itself: the update disks, tagged as the base part.
    await browseLayerFolder("base", UPDATE_FOLDER);

    const hint = await screen.findByTestId("layer-wrong-hint-0");
    expect(hint.textContent).toContain("Update3.2.2");
    // Named by what the media actually is, not a bare id.
    expect(hint.textContent).toContain(layerFieldLabel("update-3.2.2"));
    // Exactly one hint on screen — no other row claims anything.
    expect(screen.queryAllByTestId(/^layer-wrong-hint-/)).toHaveLength(1);
  });

  it("carries no hint once every row holds what its tag says", async () => {
    // The default `beforeEach` wiring already answers this way; asserted
    // explicitly so a change to that default cannot silently start every
    // other test in this file with a hint nobody wrote.
    renderOsInstall({ release: "AmigaOS 3.2.2" });
    await browseLayerFolder("base", "E:\\media\\3.2");
    await browseLayerFolder("update-3.2.2", "E:\\media\\Update3.2.2");

    expect(screen.queryAllByTestId(/^layer-wrong-hint-/)).toHaveLength(0);
  });

  /**
   * **One folder in the list is one folder to identify** (final-review.md M5,
   * fix wave 2). `identifyFoldersKey` used to be built from every layer's own
   * remembered folder with no de-duplication, so a user who pointed both
   * `base` and `update-3.2.2` at the same disks hashed that folder twice and
   * rendered every line twice under the same `key={line.path}`. The list
   * cannot hold one folder twice at all now (`withFolder`), and the
   * de-duplication stays because a hand-edited settings file need not obey
   * it.
   *
   * Asserted as an exact count on both sides, never by presence alone: "one
   * line" must not be satisfiable by the line simply being absent, and one
   * call must not be satisfiable by the mock never having been asked at all.
   */
  it("identifies a folder once, however many parts name it (M5)", async () => {
    const SHARED = "E:\\media\\Shared";
    renderOsInstall({ release: "AmigaOS 3.2.2" });
    await addFolder(SHARED);
    // A second attempt at the same folder is not a second folder.
    dialogOpenMock.mockResolvedValueOnce(SHARED);
    await userEvent.click(screen.getByTestId("material-add-folder"));
    expect(screen.queryAllByTestId("material-folder")).toHaveLength(1);

    await waitFor(() => expect(identifyMediaMock).toHaveBeenCalledWith(SHARED));
    expect(identifyMediaMock.mock.calls.filter((call) => call[0] === SHARED)).toHaveLength(1);

    // And exactly one line for the one file the (mocked) pass reports in
    // that folder -- two would be the duplicate-key defect.
    const lines = await screen.findAllByTestId("media-identity-not-in-table");
    expect(lines).toHaveLength(1);
  });
});

// ---------------------------------------------------------------------------
// What the screen says about a file's *contents* — design §4.3
// ---------------------------------------------------------------------------
//
// The round's whole risk surface. Everything before this either matched or
// did not; these are the sentences a person reads and acts on, and this
// project's most expensive defects are the confident wrong sentence rather
// than the crash.
describe("identifying install media by content hash (design §4.3)", () => {
  /** One `MediaMatch` on the wire, as the Rust side would send it. */
  function match(over: Partial<MediaIdentification["matches"][number]> = {}) {
    return {
      path: "E:\\media\\Disk1.adf",
      volumeName: "Workbench3.2",
      row: null,
      md5: "0".repeat(32),
      confirmed: null,
      ...over,
    };
  }
  const ROW: MediaRow = {
    md5: "5edf0b7a10409ef992ea351565ef8b6c",
    version: "3.2",
    // Hatcher's own identifier, which is *not* what the disk calls itself.
    volume: "Workbench3_2",
    name: "Workbench 3.2",
    source: "Hyperion (3.2 base)",
    sequence: 1,
    kind: "floppy",
    artefact: null,
    filenames: [],
    tableOrigin: "adopted",
  };

  function answersWith(identification: MediaIdentification) {
    identifyMediaMock.mockReset().mockResolvedValue(identification);
  }

  /**
   * **A miss takes nothing away.** §4.3's "additive, never subtractive", and
   * the reason it is the first test here: any modification, re-imaging or
   * different revision breaks the hash, so a miss is the *ordinary* state
   * for somebody's own disks and must never be allowed to weaken, contradict
   * or remove what the disk's own volume name already established.
   *
   * The two claims are asserted together on purpose. Asserting only that the
   * "not in the table" line appears would pass while the found line had
   * silently gone; asserting only the found line would pass while the hash
   * pass had never run at all.
   */
  it("leaves what the disks call themselves standing when nothing matches the table", async () => {
    answersWith({ matches: [match()], unreadable: [], hashed: 1, remembered: 0, skipped: [] });
    await renderFull();

    // The name-based result, unchanged and unqualified: read off the disk's
    // own root block, from a different source, and the hash pass has no
    // business touching it.
    expect(
      await screen.findByText(
        i18n.t("osinstall.media.found", { count: 1, names: "Workbench3.2" })
      )
    ).toBeTruthy();

    // And the miss really is on screen, saying what it is a fact about.
    const miss = await screen.findByTestId("media-identity-not-in-table");
    expect(miss.textContent).toContain("Disk1.adf");
    expect(miss.textContent).toContain("not in Emu68 Hatcher's install-media table");
    expect(miss.textContent).toContain("fact about the table, not about the disk");
    // Never an accusation, and never a claim about the disk's identity.
    expect(miss.textContent).not.toMatch(/not genuine|fake|counterfeit|invalid/i);
  });

  /**
   * **A confirmed row and an unconfirmed one are two sentences.** 35 of the
   * table's 186 rows have been hashed off a real disk; 151 have not. Both
   * arms in one test, because the guard is the *difference*: a screen that
   * printed the confirmed sentence for everything would pass a
   * confirmed-only test, and one that printed the unconfirmed sentence for
   * everything would pass an unconfirmed-only test.
   */
  it("does not present a checked row and an unchecked one with the same confidence", async () => {
    answersWith({
      matches: [
        match({
          path: "E:\\media\\Workbench3.2.adf",
          row: ROW,
          md5: ROW.md5,
          confirmed: {
            checked: "2026-09-06",
            against: "the ART author's own AmigaOS 3.2 install set, 35 ADFs",
          },
        }),
        match({
          path: "E:\\media\\Someone-elses.adf",
          volumeName: "Workbench3.1",
          row: { ...ROW, md5: "a".repeat(32), name: "Workbench 3.1", version: "3.1" },
          md5: "a".repeat(32),
          confirmed: null,
        }),
      ],
      unreadable: [],
      hashed: 2,
      remembered: 0,
      skipped: [],
    });
    await renderFull();

    const confirmed = await screen.findByTestId("media-identity-confirmed");
    const unconfirmed = await screen.findByTestId("media-identity-unconfirmed");

    // The confirmed one cites what checked it and when — a badge that cannot
    // say what confirmed it is not a citation.
    expect(confirmed.textContent).toContain("checked against a real disk here");
    expect(confirmed.textContent).toContain("the ART author's own AmigaOS 3.2 install set");
    expect(confirmed.textContent).toContain("2026-09-06");

    // The unconfirmed one says plainly that nobody has, and how much of the
    // table that is true of.
    expect(unconfirmed.textContent).toContain("Nobody has checked that row against a real disk");
    expect(unconfirmed.textContent).toContain("151 of the table's 186 rows");
    expect(unconfirmed.textContent).not.toContain("checked against a real disk here");

    // Two different sentences, not one sentence twice.
    expect(confirmed.textContent).not.toEqual(unconfirmed.textContent);
  });

  /**
   * **Attribution is inside the sentence, not a footnote.** ART's claim is
   * about the *row*, not about the disk: "this file matches the row Emu68
   * Hatcher's table calls X". That stays true even if the row is wrong.
   */
  it("names whose table the claim comes from, in the sentence itself", async () => {
    answersWith({
      matches: [match({ row: ROW, md5: ROW.md5 })],
      unreadable: [],
      hashed: 1,
      remembered: 0,
      skipped: [],
    });
    await renderFull();

    const line = await screen.findByTestId("media-identity-unconfirmed");
    expect(line.textContent).toContain("Emu68 Hatcher's install-media table");
    expect(line.textContent).toContain("Workbench 3.2");
    // The row's own fields, as the table states them — never re-derived.
    expect(line.textContent).toContain("Hyperion (3.2 base)");
    // And never the row's `volume` as if it were the disk's own name: those
    // are two namespaces, measured 0 of 12 matching.
    expect(line.textContent).not.toContain("Workbench3_2");
  });

  /**
   * **"Could not be read" is not "not in the table".** The first has a next
   * step (fix the file); the second has none. `unreadable` exists on the
   * wire so these cannot collapse, and this is the screen half of that.
   */
  it("keeps a file it could not read apart from a file no row claims", async () => {
    answersWith({
      matches: [match()],
      unreadable: ["E:\\media\\locked.adf"],
      hashed: 1,
      remembered: 0,
      skipped: [],
    });
    await renderFull();

    const unreadable = await screen.findByTestId("media-identity-unreadable");
    expect(unreadable.textContent).toContain("locked.adf");
    expect(unreadable.textContent).toContain("could not read this file's bytes");
    expect(unreadable.textContent).toContain("the table was never asked");

    const miss = await screen.findByTestId("media-identity-not-in-table");
    expect(miss.textContent).toContain("Disk1.adf");
    expect(miss.textContent).not.toContain("locked.adf");
  });

  /**
   * **The screen says where its answer came from.** The hash is remembered
   * against `(path, size, mtime)`, so a restored backup that keeps its
   * timestamps is answered out of the previous file's hash — with complete
   * confidence, and wrong. A stale listing is a stale list; a stale hash is a
   * wrong *name* for a disk. So the counts are on screen and the escape
   * hatch is named.
   */
  it("says how much it read now and how much it remembered, and names the way out", async () => {
    answersWith({ matches: [match()], unreadable: [], hashed: 0, remembered: 1, skipped: [] });
    await renderFull();

    const summary = await screen.findByTestId("media-identity-summary");
    expect(summary.textContent).toContain("Read now: 0");
    expect(summary.textContent).toContain("Answered from an earlier pass: 1");
    expect(summary.textContent).toContain("Scan again");
    // The button that sentence points at is really there, under that name.
    expect(screen.getByRole("button", { name: i18n.t("osinstall.media.rescan") })).toBeTruthy();
  });

  /**
   * **A pass that could not run is its own ending.** Falling back to an
   * empty result would read as "ART looked and found nothing", which is the
   * §89 collapse — and it would be the fourth ending wearing the third's
   * sentence.
   */
  it("says the pass failed rather than showing an empty result", async () => {
    identifyMediaMock.mockReset().mockRejectedValue(new Error("no"));
    await renderFull();

    // Read off the fold's closed line, not the `media-identity-summary`
    // paragraph inside it: after fix wave 2 (#4) the paragraph renders only
    // for the identified state, and the three other endings carry their
    // sentence on the `<summary>` — one place, so the two cannot disagree.
    const summary = (await screen.findByTestId("media-identity-fold")).querySelector("summary")!;
    expect(summary.textContent).toBe(
      i18n.t("osinstall.mediaId.failed", { folder: "E:\\media", identified: 0, total: 1 })
    );
    expect(summary.textContent).toContain("could not read E:\\media");
    expect(screen.queryByTestId("media-identity-not-in-table")).toBeNull();
    // And the name-based line is untouched by the failure, the same way a
    // miss leaves it alone.
    expect(
      screen.getByText(i18n.t("osinstall.media.found", { count: 1, names: "Workbench3.2" }))
    ).toBeTruthy();
  });

  /**
   * **Pressing Stop is not ART failing** (fix wave 1, M1). `awaitJobResult`
   * rejects with `Error("cancelled")` when the user stops the job, and that
   * used to arrive in the same `catch` as a real failure — so a user who
   * stopped the pass themselves was told ART "could not identify these files
   * by content", which is both untrue and the wrong next step.
   *
   * Asserted on the two sentences together, because the failure mode is one
   * of them wearing the other's words: a test that only checked the cancelled
   * sentence appears would pass if both states rendered it.
   */
  it("does not report a pass the user stopped as a failure", async () => {
    identifyMediaMock.mockReset().mockRejectedValue(new Error("cancelled"));
    await renderFull();

    // The fold's closed line — see the failed case above for why.
    const summary = (await screen.findByTestId("media-identity-fold")).querySelector("summary")!;
    expect(summary.textContent).toBe(
      i18n.t("osinstall.mediaId.cancelled", { identified: 0, total: 1 })
    );
    expect(summary.textContent).toContain("You stopped this pass");
    expect(summary.textContent).toContain(i18n.t("osinstall.media.rescan"));
    // The failure sentence, and the accusation inside it, are absent.
    expect(summary.textContent).not.toContain("could not read");
    expect(summary.textContent).not.toBe(
      i18n.t("osinstall.mediaId.failed", { folder: "E:\\media", identified: 0, total: 1 })
    );
    // The folder is reported as stopped, by name — not as unreadable.
    const folder = await screen.findByTestId("media-identity-folder-stopped");
    expect(folder.textContent).toContain("E:\\media");
    expect(screen.queryByTestId("media-identity-folder-unreadable")).toBeNull();
  });

  /**
   * **One bad folder must not discard the folders already identified** (fix
   * wave 1, M2). The pass merges across folders in a loop; a rejection on the
   * second used to throw away what the first had found and say the whole pass
   * failed — ART denying work it had done.
   *
   * `core/hostfs.rs`'s rule is the shape: per entry, by name and by result.
   */
  it("keeps the folders it identified when a later folder cannot be read", async () => {
    const EXTRA = "E:\\media\\Update";
    identifyMediaMock
      .mockReset()
      .mockImplementation((folder: string) =>
        folder === EXTRA
          ? Promise.reject(new Error("boom"))
          : Promise.resolve({
              matches: [
                {
                  path: "E:\\media\\Disk1.adf",
                  volumeName: "Workbench3.2",
                  row: ROW,
                  md5: ROW.md5,
                  confirmed: null,
                },
              ],
              unreadable: [],
              hashed: 1,
              remembered: 0,
              skipped: [],
            } satisfies MediaIdentification)
      );
    seedRemembered({ ...FULL_FIELDS, "osinstall.extraMediaFolders": [EXTRA] });
    render(<OsInstall />);
    await screen.findByText(i18n.t("osinstall.plan.heading"));

    // What the first folder produced is still on screen, in full.
    const kept = await screen.findByTestId("media-identity-unconfirmed");
    expect(kept.textContent).toContain("Disk1.adf");
    expect(kept.textContent).toContain("Workbench 3.2");

    // And the report says which folder failed and how many finished — a
    // count with no name would leave the user to guess which of their two
    // folders ART never got through.
    const summary = (await screen.findByTestId("media-identity-fold")).querySelector("summary")!;
    expect(summary.textContent).toBe(
      i18n.t("osinstall.mediaId.failed", { folder: EXTRA, identified: 1, total: 2 })
    );

    const ok = await screen.findByTestId("media-identity-folder-identified");
    expect(ok.textContent).toContain("E:\\media");
    const bad = await screen.findByTestId("media-identity-folder-unreadable");
    expect(bad.textContent).toContain(EXTRA);
    expect(bad.textContent).not.toBe(ok.textContent);
  });

  /** Every folder the plan reads is identified, not only the first — and one
   *  at a time, because the job lane supersedes a second start and would
   *  leave the first promise unsettled. */
  it("identifies every folder the request carries", async () => {
    await renderFull();
    identifyMediaMock.mockClear();

    dialogOpenMock.mockResolvedValue("E:\\media\\Update");
    await userEvent.click(screen.getByRole("button", { name: /add another folder/i }));

    await waitFor(() =>
      expect(identifyMediaMock.mock.calls.map((c) => c[0])).toEqual([
        "E:\\media",
        "E:\\media\\Update",
      ])
    );
  });

  /** "Scan again" drops the remembered hashes as well as the remembered
   *  listings, so what is on screen is about nothing until the pass runs
   *  again. */
  it("re-identifies after the remembered answers are dropped", async () => {
    await renderFull();
    identifyMediaMock.mockClear();

    await userEvent.click(screen.getByRole("button", { name: i18n.t("osinstall.media.rescan") }));

    await waitFor(() => expect(identifyMediaMock).toHaveBeenCalledWith("E:\\media"));
  });
});

describe("the identity wall is folded (four-tab design § 3.1)", () => {
  it("draws one closed line whose count is the pass's own, and keeps the lines inside", async () => {
    await renderFull();
    const fold = await screen.findByTestId("media-identity-fold");
    expect((fold as HTMLDetailsElement).open).toBe(false);
    const lines = within(fold).queryAllByTestId(
      /^media-identity-(confirmed|unconfirmed|not-in-table|unreadable|skipped)$/
    );
    expect(fold.textContent).toBeTruthy();
    expect(fold.querySelector("summary")!.textContent).toBe(
      i18n.t("osinstall.mediaId.foldSummary", { count: lines.length })
    );
    expect(within(fold).getByTestId("media-identity")).toBeTruthy();
  });

  it("keeps the reuse toggle and Scan again inside the fold, still working", async () => {
    await renderFull();
    const fold = await screen.findByTestId("media-identity-fold");
    const toggle = within(fold).getByRole("checkbox", { name: i18n.t("osinstall.media.reuseScan") });
    expect(toggle).toBeTruthy();
    await userEvent.click(within(fold).getByRole("button", { name: i18n.t("osinstall.media.rescan") }));
    await waitFor(() => expect(rescanMock).toHaveBeenCalled());
  });

  /**
   * **Running is not "0 files".** The count in the closed line is the number
   * of *lines the pass has produced*, and while the pass is still reading
   * there are none — so a summary built from that count reads "what 0 files
   * in these folders are", which is a confident, wrong sentence over folders
   * ART is at that moment hashing (§89: the failure that does not crash).
   * The pass's own running sentence is the one the block already shows
   * inside, so the closed line says that instead until there is something to
   * count.
   *
   * The assertion is the sentence, not merely "does not contain 0": both are
   * asserted, because "no zero" alone would pass on a blank summary and the
   * equality alone would pass if the running phrase itself ever gained a
   * count.
   */
  it("says the pass is running, not 'what 0 files are', while it is still reading", async () => {
    identifyMediaMock.mockReset().mockImplementation(() => new Promise(() => {}));
    await renderFull();
    const fold = await screen.findByTestId("media-identity-fold");
    const summary = fold.querySelector("summary")!.textContent!;
    expect(summary).not.toContain("0");
    expect(summary).toBe(i18n.t("osinstall.mediaId.identifying"));
    // Once, not twice (fix wave 2, #4): the paragraph inside the fold
    // rendered the identical phrase, so the closed line and the first line
    // inside it were the same sentence.
    expect(within(fold).queryByTestId("media-identity-summary")).toBeNull();
  });

  /**
   * **A pass that died on its first folder is not a pass that found nothing**
   * (fix wave 2, #1). The count branch used to catch `failed` as well as
   * `identified`, so a pass that never produced a line read "what 0 files in
   * these folders are" — the §89 collapse, and the ending wearing another
   * ending's sentence.
   *
   * Both halves are asserted: equal to the failed phrase *and* not equal to
   * the count line, because equality alone would still pass on the day the
   * two sentences coincide, and inequality alone would pass on a blank.
   */
  it("says the pass failed on the closed line, not 'what 0 files are'", async () => {
    identifyMediaMock.mockReset().mockRejectedValue(new Error("no"));
    await renderFull();
    const fold = await screen.findByTestId("media-identity-fold");
    const summary = fold.querySelector("summary")!.textContent!;
    expect(summary).toBe(
      i18n.t("osinstall.mediaId.failed", { folder: "E:\\media", identified: 0, total: 1 })
    );
    expect(summary).not.toBe(i18n.t("osinstall.mediaId.foldSummary", { count: 0 }));
    // And said once (fix wave 2, #4): the paragraph inside the fold used to
    // render the same phrase, so opening the fold showed the screen
    // answering one question twice.
    expect(within(fold).queryByTestId("media-identity-summary")).toBeNull();
  });

  /** **And a pass the user stopped at N of M reads as stopped**, not as a
   *  finished one counting the lines it happened to have (fix wave 2, #1).
   *  Asserted against the failed phrase too: the cancelled line must not
   *  arrive wearing the failure's accusation. */
  it("says the pass was stopped on the closed line, not the finished count", async () => {
    identifyMediaMock.mockReset().mockRejectedValue(new Error("cancelled"));
    await renderFull();
    const fold = await screen.findByTestId("media-identity-fold");
    const summary = fold.querySelector("summary")!.textContent!;
    expect(summary).toBe(i18n.t("osinstall.mediaId.cancelled", { identified: 0, total: 1 }));
    expect(summary).not.toBe(i18n.t("osinstall.mediaId.foldSummary", { count: 0 }));
    expect(summary).not.toContain("could not read");
    expect(within(fold).queryByTestId("media-identity-summary")).toBeNull();
  });

  it("still runs the identification pass while the fold is closed", async () => {
    await renderFull();
    await screen.findByTestId("media-identity-fold");
    await waitFor(() => expect(identifyMediaMock).toHaveBeenCalled());
  });

  it("draws no fold before any folder is chosen", () => {
    seedRemembered({});
    render(<OsInstall />);
    expect(screen.queryByTestId("media-identity-fold")).toBeNull();
  });
});

// ART-260: `layerScans`, `layerIdentified` and `extraScans` all clear to
// `{}` through this one guard rather than their own inline `{}` literal, so
// a no-op reset (already empty, resetting to empty again -- what happens on
// every settled render while a release has no layers) keeps the *same*
// object instead of manufacturing a fresh identity for nothing. Nothing
// downstream depends on that identity yet (harmless today, per the entry),
// so this tests the guard directly rather than an effect that does not
// exist -- the same shape `foundVolumeNames`'s own identity guard would need
// the day something does read it.
describe("resetIfEmpty (ART-260)", () => {
  it("keeps the same object identity across a no-op reset", () => {
    const empty: Record<string, string | null> = {};
    expect(resetIfEmpty(empty)).toBe(empty);
  });

  it("still clears a non-empty record to a genuinely new empty object", () => {
    const populated: Record<string, string | null> = { "layer-a": "Workbench3.2" };
    const reset = resetIfEmpty(populated);
    expect(reset).toEqual({});
    expect(reset).not.toBe(populated);
  });
});
