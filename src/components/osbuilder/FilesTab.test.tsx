// @vitest-environment jsdom
//
// ART-118: the OS Builder's install screen has never once been seen
// rendering past its five `h2` headings. A headless-Chrome probe (Chrome and
// Edge, headless and headed) reproducibly crashed the renderer
// (`-1073741819`, an access violation) the moment anything past the
// headings was touched — filling the media/ROM/destination fields, ticking
// a component, reading the confirmation or refusals card, running Verify.
// A browser cannot see this screen right now. jsdom can, so this is the
// first automated coverage of what is now `FilesTab.tsx` — not a proxy
// harness the way `FileManagerFilter.test.tsx` and `useRomPairing.test.tsx`
// are, because the whole point here is proving the *real* component mounts,
// not a stand-in that always would have.
//
// **This file is `OsInstall.test.tsx`, renamed with its component on
// 2026-09-09 by round 4 task 5 of the four-tab rewrite** (`git log --follow`
// shows the rename). Every case about a section that found another tab went
// out with that section; the list of them, by name and with the fate of
// each, is the banner above the first surviving describe.
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
//   1. The tab mounts past its heading, with its real controls present.
//   2 & 3. Nothing on screen is a raw i18n key or an unrendered
//      `{{interpolation}}` — in English, and (ART-062, never checked before)
//      in Turkish, whose strings run measurably longer than the English
//      originals.
//   4. The material: the folder list reaches the request `osinstallPlan` is
//      asked to plan, the readout comes first, the identity pass runs behind
//      its fold, and a dropped disc is appended rather than substituted.
//
// What this does NOT establish: the access violation itself. jsdom does no
// layout at all, so it cannot reproduce a native renderer crash or measure
// overflow — ART-062's "does a long Turkish string actually fit" is still a
// real-screen job, and a human `pnpm tauri dev` pass over this screen is
// still owed. See the narrowed ART-118 entry in `docs/ISSUES.md`.

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import i18n from "i18next";

import { changeLanguage } from "@/i18n";
import { useSettingsStore } from "@/stores/settingsStore";
import type {
  ComponentDef,
  InstallLayer,
  InstallRelease,
  InstallRequest,
  MediaIdentification,
  MediaRow,
  MediaScanResult,
  PlanItem,
  PlanResult,
} from "@/lib/osinstall";
import type { RomInfo } from "@/lib/pistorm";

const scanMediaMock = vi.hoisted(() => vi.fn());
const componentsMock = vi.hoisted(() => vi.fn());
const layersForMock = vi.hoisted(() => vi.fn());
const layerForMediaMock = vi.hoisted(() => vi.fn());
const planMock = vi.hoisted(() => vi.fn());
const componentCollisionsMock = vi.hoisted(() => vi.fn());
const describeTreeMock = vi.hoisted(() => vi.fn());
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
  // `useChainTree` asks this about the destination (round 3 task 3, fix
  // round 1). Mocked at the same boundary as the rest, and defaulting to
  // *not a build* — which is what the unmocked, rejecting call already meant
  // for every case in this file.
  osinstallDescribeTree: describeTreeMock,
}));

// `AmigaInstallPanel` is mounted inside this screen, and both it and this
// screen's own install job subscribe to `onJobProgress` on mount — real
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

const { FilesTab, resetIfEmpty } = await import("@/components/osbuilder/FilesTab");
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

/** A layer's own field label, resolved the same way `FilesTab.tsx` resolves
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
/** The folder `FULL_FIELDS` points AmigaOS 3.2 at, and one a test adds
 *  beside it. Named rather than spelled out at each site: a Windows path
 *  written through a shell heredoc loses its backslashes and nothing
 *  fails (CLAUDE.md), so the two this file adds by hand live here. */
const MAIN_FOLDER = "E:\\media";
const UPDATE_FOLDER = "E:\\media\\Update";

/**
 * A **different** disk in the added folder from the one in the main folder.
 *
 * The default fixture answers `Workbench3.2` for every folder, and
 * `foundVolumeNames` folds duplicates — so a found line reading "1 install
 * disk found: Workbench3.2" is what a screen that read both folders and a
 * screen that read only the first both produce. Naming two disks is what
 * makes "every folder in the list" a claim a test can fail.
 */
/** The folder column's found line, exactly — count and names together, so
 *  "one name" and "the right name" stay different claims. */
function foundLine(names: string[]): string {
  return i18n.t("osinstall.media.found", {
    count: names.length,
    names: names.join(", "),
  });
}

function scanTwoFolders(extra: string, extraVolume: string) {
  scanMediaMock.mockImplementation((folder: string) =>
    Promise.resolve(
      (folder === extra
        ? {
            outcome: "found",
            media: [{ path: `${extra}\\Disk.adf`, volumeName: extraVolume, kind: "floppy" }],
          }
        : {
            outcome: "found",
            media: [
              { path: `${MAIN_FOLDER}\\Disk1.adf`, volumeName: "Workbench3.2", kind: "floppy" },
            ],
          }) satisfies MediaScanResult
    )
  );
  // **The identify pass needs a distinct file per folder too.** It merges its
  // matches across folders and the per-file lines are keyed by path, so the
  // default fixture — one `Disk1.adf` whatever folder is asked about —
  // renders two children under one React key the moment a second folder is in
  // the list. A duplicate key is not this file's subject, but React warning
  // about one is output nobody asked for.
  identifyMediaMock.mockImplementation((folder: string) =>
    Promise.resolve({
      matches: [
        {
          path: `${folder}\\Disk.adf`,
          volumeName: folder === extra ? extraVolume : "Workbench3.2",
          row: null,
          md5: "0".repeat(32),
          confirmed: null,
        },
      ],
      unreadable: [],
      hashed: 1,
      remembered: 0,
      skipped: [],
    } satisfies MediaIdentification)
  );
}

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
  describeTreeMock.mockReset().mockResolvedValue({
    isTree: false,
    release: null,
    files: 0,
    components: [],
    amigaInstalled: [],
    problem: "holds no distribution.json",
  });
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
 *  could never reach — then waits for the plan to have been asked for and the
 *  material columns to be on screen.
 *
 *  **There is no plan heading to wait for any more** (round 4 task 5): the
 *  plan is still computed here, because the folder column is drawn from its
 *  layers and its `unusedForPlan`, but the section that drew it is
 *  `BuildTab`'s. So the wait is on the request having gone out and the
 *  columns having rendered. */
async function renderFull() {
  seedRemembered(FULL_FIELDS);
  const utils = render(<FilesTab />);
  // **Nothing here waits on a plan** (round 4 task 5's fix round): tab 1 does
  // not ask for one. What it does ask is the recipe's layers — which decide
  // whether a folder row offers a layer tag at all, and `layers === []` has
  // two causes, so every case below that reads the folder column needs the
  // settled one — and a scan of each folder in the list.
  await waitFor(() => expect(layersForMock).toHaveBeenCalled());
  await screen.findByTestId("source-columns");
  await waitFor(() => expect(scanMediaMock).toHaveBeenCalled());
  return utils;
}

/**
 * Render, optionally starting on a chosen release — seeded directly into the
 * build session's own remembered key, the way `FULL_FIELDS` already seeds
 * AmigaOS 3.9's fields, rather than a picker click every caller would
 * otherwise repeat. `"AmigaOS 3.2"` is the session's own default, so it is
 * seeded like every other test that never mentions a release.
 */
function renderFilesTab(options: { release?: InstallRelease } = {}) {
  seedRemembered(
    options.release && options.release !== "AmigaOS 3.2"
      ? { "buildSession.release": options.release }
      : {}
  );
  return render(<FilesTab />);
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

/** Renders on AmigaOS 3.2.2 and browses every named layer's own folder in
 *  turn, then waits for the recipe's layers to have landed — which is what
 *  decides whether the tags mean anything at all. */
async function withLayerFolders(folders: Record<string, string>): Promise<void> {
  renderFilesTab({ release: "AmigaOS 3.2.2" });
  for (const [layerId, path] of Object.entries(folders)) {
    await browseLayerFolder(layerId, path);
  }
  await waitFor(() => expect(layersForMock).toHaveBeenCalledWith("AmigaOS 3.2.2"));
}

// ---------------------------------------------------------------------------
// What left this file with the sections it belonged to (round 4 task 5)
// ---------------------------------------------------------------------------
//
// **Twenty-four** `it(...)`s were removed on 2026-09-09, with the plan-error
// badge, the refusals card, the plan card, the run card and the result card.
// Every one of them is below, by name, with where it went. (The keymap five
// left a commit earlier, in task 4, and are named in `MachineTab.test.tsx`.)
//
// The count is stated because this list has already been wrong once: the
// first version of it named twenty-three, and `names the component to tick by
// its own label, never the raw recipe id` — the one refusal that tells a
// person to go and tick a component themselves — went out unnamed and
// unheld. A fates list that does not say how many it is accounting for cannot
// be checked against the diff that produced it.
//
// A second correction, made in place rather than left to decay: the first
// version of this list called ten of these guards lost. **Task 4's own fix
// round moved all ten**, by their original names, into `BuildTab.test.tsx` —
// so they are under "moved" below, not under "dropped".
//
// **Moved to `MachineTab.test.tsx` in task 4** (that file names all five):
//   says nothing when the install places no keymaps · offers what the plan
//   will really place, and not the icons · sends the chosen layout to the
//   planner · sends null when nothing is chosen, leaving it to Kickstart ·
//   remembers the choice per release.
//
// **Covered by `BuildTab.test.tsx`** — the sections these were about are that
// tab's now, and a case there asserts the same sentence:
//   - `says it once, instead of once per component` (ART-208) and
//     `names the release the folder belongs to, and switching is one click`
//     → `offers the switch-release button when the folder holds another
//     release's media`, which asserts the switch button, the suppressed
//     `build-refusals` card and no Build button.
//   - `keeps the per-disk list when the folder is the right one` →
//     `renders the plan's refusal and no Build button (design § 7)`.
//   - `shows the real, translated refusal text` → the same case, which reads
//     `osinstall.refusal.mediaMissing` out of `build-refusals`.
//   - `names the component to tick by its own label, never the raw recipe id`
//     → the case of the same name, moved whole in round 4 task 5's fix round.
//     It covers **both** places a refusal is drawn on tab 4 now — the
//     refusals card in the Build button's place, and a refused phase's own
//     list under its advice — which is one more than this file ever had.
//   - `hands a finished install's destination to the session` (ART-197) →
//     `hands the finished tree to the session, once, and never on render
//     (ART-197)`, which additionally proves nothing is written on render.
//   - `reports a confirmed marker by its own text` (Task 9) →
//     `carries the tree phase's own release verdict and the time it took`.
//
// **Covered by `ChoiceTab.test.tsx`**:
//   - `shows one release's own item count, and a ticked component's second
//     item` → `reaches the request osinstallPlan is asked to plan`. What a
//     tick changes is tab 2's; what the plan then totals is tab 4's first
//     summary line.
//
// **Covered by `src/lib/osinstall.test.ts`** — these asserted a pure mapper's
// answer through the screen, and that mapper has its own named case:
//   - `leaves the all-or-nothing case to its own message` →
//     `mediaEvidence … says nothing when wrongMediaFolder owns the case`.
//   - `does not claim a release for a folder that identifies none` ·
//     `does not tell a folder naming two releases that its disks carry no
//     version` (ART-255) · `does not call every disk in the folder the right
//     media for this release` (ART-258) → `mediaEvidence`'s own describe,
//     which names each sentence.
//
// **Dropped, with the reason**:
//   - `says on screen which folder the next steps will act on` — the result
//     card's own carried-tree sentence, whose key is deleted with it; tab 4
//     states the hand-off in the tree phase's report instead, and `sets the
//     kind the card lane needs and points at it` is the case for it.
//   - `takes a finished install's report down when the release changes`
//     (ART-210) — the report is not on this tab. Its sibling,
//     `takes an answer a control gave down when the release changes`, stays
//     and is the case that proves the rule.
//
// **Moved to `BuildTab.test.tsx` by task 4's fix round**, each under its own
// original name — these were reported as lost when task 5 landed, and were
// not left that way:
//   - `names both sides of a mismatch` · `says a tree with no marker states
//     none, not a guess` · `says the marker could not be read, never the same
//     sentence as unstated` · `reports a differing marker for an unmeasured
//     release plainly, never as a mismatch` (Task 9) — the four `statedRelease`
//     verdicts beside the confirmed one, in that file's
//     `the tree phase's release marker` describe.
//   - `shows what the folder holds above the refusals when some disks are
//     missing` · `names the other release when the folder is a different
//     one` · `says what a layered release's own folders hold, and does not
//     call the base set somebody else's` (ART-257) · `never asks about the
//     previous release's folder while a layered release is loading`
//     (ART-257) · `asks each media lookup once for a settled folder, not once
//     per render` — the evidence line above the refusals and the effects
//     behind it, in `what the folder holds, above the refusals`.
//   - `says what the folder really holds while the previous release's
//     evidence is still the only one held` (ART-254) — its own describe there.
//
// ---------------------------------------------------------------------------
// What round 4 task 5's **fix round** changed about the cases that stayed
// ---------------------------------------------------------------------------
//
// Tab 1 stopped calling `useInstallPlan` (it read three fields of the answer
// and paid for a walk of every ADF in the list to get them). Every case that
// asserted through `osinstallPlan` therefore had to say what it means without
// one; none of them left the file, and each carries the reasoning at its own
// site rather than here. In short: what the *request* carries is
// `foldersForPlan`'s and `useInstallPlan`'s and is asserted against those
// (`buildSession.test.ts`, `useInstallPlan.test.tsx`, `ChoiceTab.test.tsx`);
// what this tab still owes is that every folder in the list is read and
// named, which the folder column's own found line states. The one case that
// left in that pass, `asks once for a request shape it has already asked for`
// (ART-119), is named where it stood.

describe("FilesTab renders past its heading", () => {
  it("mounts with the material, and with nothing that belongs to another tab", async () => {
    await renderFull();

    // The five headings a browser probe once confirmed are not the news
    // here — everything below them, which the probe never survived to see,
    // is. There is one heading now: the Kickstart and destination rows went
    // to tab 3 on 2026-09-09 (`MachineTab.test.tsx` mounts them there), and
    // the plan, the refusals, the run and the result went to tab 4 in round 4
    // task 5 (`BuildTab.test.tsx`).
    expect(screen.getByText(i18n.t("osinstall.material.label"))).toBeTruthy();

    // **One tickbox, and it is not a component** (four-tab design § 3.2 and
    // § 3.1): the fold's "reuse the last scan" (ART-194). The component
    // checklist moved to tab 2 and the run confirmation to tab 4; asserted as
    // a number rather than left unsaid, because a control rendered on *two*
    // tabs is the state these moves must not leave behind.
    //
    // **`AmigaInstallPanel`'s own is not among them either, and that changed
    // in round 3.** This fixture is AmigaOS 3.2, whose catalogue answers with
    // no runnable package at all, so that panel renders no form — and its
    // emulator confirmation is part of the form. It used to render on its own
    // under the "nothing runnable for this release" sentence, which is
    // ART-212's own complaint one control further on: a confirmation for a
    // run that cannot be configured.
    const checkboxes = screen.getAllByRole("checkbox");
    expect(checkboxes.length).toBe(1);
    expect(screen.queryByTestId("choice-part-row")).toBeNull();
    expect(
      screen.getByRole("checkbox", { name: i18n.t("osinstall.media.reuseScan") })
    ).toBeTruthy();

    // **No Build button, no refusals card, no run report** — named one by
    // one rather than as "nothing else", because each is a section that used
    // to be here and each has its own tab now. A section drawn on two tabs at
    // once is the duplicate this whole rewrite exists to remove (ART-202).
    //
    // Found by tab 4's own testids rather than by its sentences, because the
    // plan, run and result headings were **deleted from both catalogues** in
    // the same commit that deleted their last renderer (round 4 task 5), and
    // `i18n.t` of a key nobody holds is the key itself — an assertion that
    // would pass for the wrong reason. `osinstall.refusals.heading` is still
    // a key, so that one is named as a sentence.
    expect(screen.queryByTestId("build-run")).toBeNull();
    expect(screen.queryByTestId("build-summary-line")).toBeNull();
    expect(screen.queryByTestId("build-phase-row")).toBeNull();
    expect(screen.queryByTestId("build-plan-error")).toBeNull();
    expect(screen.queryByText(i18n.t("osinstall.refusals.heading"))).toBeNull();
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

describe("tab 1 asks for layers, not a plan (round 4 task 5's fix round)", () => {
  // **The cost this replaces.** This screen called `useInstallPlan` and read
  // three fields of the answer — `layers`, `layersKnown`, `plannedFolders` —
  // which between them cost one `osinstall_layers`, a read of a shipped JSON
  // recipe. The rest of that hook costs `osinstall_components` and then
  // `osinstall_plan`, and `plan()` opens and walks every switched-on
  // component's disc image. Nothing on this tab read one byte of it. That is
  // ART-119's own cost, on the tab a person opens first and stays on longest
  // while they are still adding folders — each added folder starting the walk
  // again.
  //
  // The case that stood here, `asks once for a request shape it has already
  // asked for`, is **covered by `useInstallPlan.test.tsx`**'s `plans once
  // when nothing is excluded, twice when something is`, which measures the
  // dedupe at the hook rather than through a screen that no longer calls it.

  it("asks for no plan and no component catalogue, however many folders it holds", async () => {
    await renderFull();
    await addFolder(UPDATE_FOLDER);

    // Waited on a *later* answer than the ones counted, so this is not a
    // snapshot taken before anything could have been asked: the second
    // folder's own scan has landed by here.
    await waitFor(() => expect(scanMediaMock).toHaveBeenCalledWith(UPDATE_FOLDER));
    // Counted zeroes, named one by one so a failure says which command came
    // back.
    expect({ osinstallPlan: planMock.mock.calls.length }).toEqual({ osinstallPlan: 0 });
    expect({ osinstallComponents: componentsMock.mock.calls.length }).toEqual({
      osinstallComponents: 0,
    });
    expect({ osinstallComponentCollisions: componentCollisionsMock.mock.calls.length }).toEqual({
      osinstallComponentCollisions: 0,
    });
    // …and the one it *does* ask, so this is not a screen that asks nothing
    // at all: the recipe's own layers, once per release.
    expect(layersForMock).toHaveBeenCalledWith("AmigaOS 3.2");
  });
});

describe("reusing the last scan, and asking for a fresh one (ART-194)", () => {
  // **The toggle is on this tab; what reads it is not.** Tab 1 stopped
  // planning in round 4 task 5's fix round, so what this control does is
  // write the one remembered key the planning tabs pass as `scanCache` —
  // which `useInstallPlan.test.tsx` and `ChoiceTab.test.tsx` see from the
  // other side. The half of the old case that asserted `scanCache: "reuse"`
  // on this screen's own request went with the request.
  it("is on by default, and remembers the user turning it off", async () => {
    await renderFull();

    const box = screen.getByRole("checkbox", {
      name: i18n.t("osinstall.media.reuseScan"),
    }) as HTMLInputElement;
    // Cached by default: nobody had to switch this on.
    expect(box.checked).toBe(true);

    await userEvent.click(box);

    // The choice was written to the remembered store, so tomorrow's run
    // starts where this one ended — and so the tab that plans reads it.
    await waitFor(() => {
      const bag = useSettingsStore.getState().settings.remembered as Record<string, unknown>;
      expect(bag["osinstall.reuseScan"]).toBe(false);
    });
    expect(
      (
        screen.getByRole("checkbox", {
          name: i18n.t("osinstall.media.reuseScan"),
        }) as HTMLInputElement
      ).checked
    ).toBe(false);
  });

  it("asks the backend to forget what it remembered, and re-scans the discs", async () => {
    await renderFull();
    scanMediaMock.mockClear();

    await userEvent.click(screen.getByRole("button", { name: i18n.t("osinstall.media.rescan") }));

    // It reaches the command that actually deletes the listings — a rescan
    // that merely skipped the cache for one call would leave the stale answer
    // sitting there for the next one.
    await waitFor(() => expect(rescanMock).toHaveBeenCalled());
    // …and the screen really re-reads the folders afterwards, rather than
    // leaving the listing it already had on screen. (It used to re-plan too;
    // tab 1 does not plan, and `forget_all` has deleted the listings, so the
    // tabs that do cannot be served a stale one either.)
    await waitFor(() => expect(scanMediaMock).toHaveBeenCalledWith(MAIN_FOLDER));
    // …and says what it did, so the button cannot be mistaken for one that
    // ignored the click.
    await screen.findByText(i18n.t("osinstall.media.rescanned", { count: 1 }));
  });

  // Round 4 task 5. The failure used to be reported through the plan's own
  // error badge at the top of this screen; that badge is `BuildTab`'s now, so
  // a rescan that threw would have said **nothing at all** on the tab whose
  // button was pressed — a control the user cannot tell apart from one that
  // ignored them, which is the exact defect the count above exists against.
  it("says why Scan again could not forget anything, where the button is", async () => {
    await renderFull();
    rescanMock.mockRejectedValueOnce(new Error("the store is locked by another process"));

    await userEvent.click(screen.getByRole("button", { name: i18n.t("osinstall.media.rescan") }));

    const badge = await screen.findByTestId("rescan-error");
    // Rust's own sentence, not a sentence of this screen's about it.
    expect(badge.textContent).toContain("the store is locked by another process");
    // And it does not also claim to have dropped anything.
    expect(screen.queryByText(i18n.t("osinstall.media.rescanned", { count: 1 }))).toBeNull();
    expect(screen.queryByText(i18n.t("osinstall.media.rescannedNone"))).toBeNull();
  });

  it("draws no failure badge for a rescan that worked", async () => {
    // The control, measured rather than assumed: a badge that is always there
    // proves nothing about the failure above.
    await renderFull();
    await userEvent.click(screen.getByRole("button", { name: i18n.t("osinstall.media.rescan") }));

    await screen.findByText(i18n.t("osinstall.media.rescanned", { count: 1 }));
    expect(screen.queryByTestId("rescan-error")).toBeNull();
  });
});

describe("the screen settles instead of re-asking for ever (ART-195)", () => {
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
  //
  // **What is counted changed in round 4 task 5's fix round, and the rule did
  // not.** The plan is not this tab's any more, and `useInstallPlan.test.tsx`
  // counts it where it lives. What tab 1 still starts per render if a
  // dependency is rebuilt per render is the *folder scan* and the *identity
  // pass* — both of which walk real media, both of which key on a list read
  // off the remembered bag, and both of which are exactly the shape ART-195
  // was. So the same measurement is made against them.
  it("does not keep re-asking about a release whose remembered keys are empty", async () => {
    await renderFull();

    const picker = screen.getByRole("combobox", {
      name: i18n.t("osinstall.release.label"),
    }) as HTMLSelectElement;
    await userEvent.selectOptions(picker, "AmigaOS 3.9");
    await waitFor(() => expect(layersForMock).toHaveBeenCalledWith("AmigaOS 3.9"));

    // Let the screen settle, then measure how much more work it starts while
    // nothing at all is happening. Against the defect this climbs without
    // bound; the assertion is that it climbs by nothing.
    await new Promise((resolve) => setTimeout(resolve, 120));
    const scanned = scanMediaMock.mock.calls.length;
    const identified = identifyMediaMock.mock.calls.length;
    const asked = layersForMock.mock.calls.length;
    await new Promise((resolve) => setTimeout(resolve, 250));

    expect(scanMediaMock.mock.calls.length).toBe(scanned);
    expect(identifyMediaMock.mock.calls.length).toBe(identified);
    expect(layersForMock.mock.calls.length).toBe(asked);
  });
});

describe("choosing the release re-asks against it", () => {
  // What tab 1 asks *of* a release is its recipe's layers — which decide
  // whether a folder row is offered a layer tag at all. That the release also
  // reaches the planner is `ChoiceTab.test.tsx`'s and
  // `useInstallPlan.test.tsx`'s, on the tabs that plan.
  it("asks the release the user chose about its own layers", async () => {
    await renderFull();

    const picker = screen.getByRole("combobox", {
      name: i18n.t("osinstall.release.label"),
    }) as HTMLSelectElement;
    expect(picker.value).toBe("AmigaOS 3.2");

    await userEvent.selectOptions(picker, "AmigaOS 3.9");
    expect(picker.value).toBe("AmigaOS 3.9");

    await waitFor(() => expect(layersForMock).toHaveBeenCalledWith("AmigaOS 3.9"));
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
    await waitFor(() => expect(layersForMock).toHaveBeenCalledWith("AmigaOS 3.9"));

    expect((await screen.findAllByText("E:\\media39")).length).toBeGreaterThan(0);
    expect(screen.queryAllByText("E:\\media")).toHaveLength(0);
  });

  // **The destination reached the planner from here until round 4 task 5's
  // fix round**, and that half of this case went with the plan — it is
  // `useInstallPlan.test.tsx`'s and `MachineTab.test.tsx`'s now, on the tabs
  // that read the field and the tabs that plan. What is left here is the half
  // this tab can still get wrong: the panel at its foot is handed a tree, and
  // that tree is looked up **per release**, so a release switch must not
  // leave ART examining the folder the other release was going to be built
  // into.
  it("looks at each release's own destination, not the other release's", async () => {
    await renderFull();
    await waitFor(() => expect(describeTreeMock).toHaveBeenCalledWith("E:\\dist"));

    const picker = screen.getByRole("combobox", {
      name: i18n.t("osinstall.release.label"),
    }) as HTMLSelectElement;
    await userEvent.selectOptions(picker, "AmigaOS 3.9");

    await waitFor(() => expect(describeTreeMock).toHaveBeenCalledWith("E:\\dist39"));
    // And it does not go on asking about the 3.2 folder afterwards —
    // `toHaveBeenCalledWith` alone would be satisfied by one right call among
    // wrong ones.
    const after = describeTreeMock.mock.calls.length;
    describeTreeMock.mockClear();
    expect(after).toBeGreaterThan(0);
    await new Promise((resolve) => setTimeout(resolve, 120));
    for (const call of describeTreeMock.mock.calls) {
      expect(call[0]).not.toBe("E:\\dist");
    }
  });

  it("keeps each release's own media folder when the user switches away and back", async () => {
    await renderFull();

    const picker = screen.getByRole("combobox", {
      name: i18n.t("osinstall.release.label"),
    }) as HTMLSelectElement;
    await userEvent.selectOptions(picker, "AmigaOS 3.9");
    await waitFor(() => expect(layersForMock).toHaveBeenCalledWith("AmigaOS 3.9"));

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
      <FilesTab
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
    const { rerender } = render(<FilesTab />);
    await waitFor(() => expect(screen.queryAllByTestId("material-folder")).toHaveLength(1));

    rerender(
      <FilesTab
        droppedMedia={{ path: "E:\\amiga\\Amigatolon\\iso\\AmigaOS39.iso", arrivalKey: "k1" }}
      />
    );

    const rows = await screen.findAllByTestId("material-folder");
    expect(rows.map((row) => row.textContent)).toEqual([
      expect.stringContaining("E:\\media"),
      expect.stringContaining("E:\\amiga\\Amigatolon\\iso"),
    ]);
    // And the folder that was already there is still read — a drop is an
    // addition, not a re-pick. (What the *planner* is then handed as its
    // first folder is `foldersForPlan`'s, unit-tested in
    // `buildSession.test.ts`, and reaches Rust from the tabs that plan.)
    await waitFor(() => expect(scanMediaMock).toHaveBeenCalledWith("E:\\media"));
    await waitFor(() =>
      expect(scanMediaMock).toHaveBeenCalledWith("E:\\amiga\\Amigatolon\\iso")
    );
  });

  it("takes effect again when the same disc is dropped a second time", async () => {
    const PATH = "E:\\amiga\\Amigatolon\\iso\\AmigaOS39.iso";
    const FOLDER = "E:\\amiga\\Amigatolon\\iso";

    const { rerender } = render(<FilesTab droppedMedia={{ path: PATH, arrivalKey: "k1" }} />);
    await waitFor(() => expect(scanMediaMock).toHaveBeenCalledWith(FOLDER));
    scanMediaMock.mockClear();

    // The user changes the media folder by hand in between — the same
    // remembered-setter path a Browse click goes through.
    seedRemembered({ "osinstall.mediaFolder": "E:\\somewhere-else" });

    // A second drop of the *same* disc: the path string is identical to the
    // first arrival, so only a change in `arrivalKey` can make this visible.
    // Keying the effect on the path alone (the bug the review caught) would
    // leave the hand-picked folder in place and never re-scan.
    rerender(<FilesTab droppedMedia={{ path: PATH, arrivalKey: "k2" }} />);
    await waitFor(() => expect(scanMediaMock).toHaveBeenCalledWith(FOLDER));
  });
});

// ---------------------------------------------------------------------------
// ART-256 — the found line describes every folder the plan was built from
// ---------------------------------------------------------------------------
//
// **Asserted through the folder column's own found line since round 4 task
// 5.** ART-256 is a claim about `foundVolumeNames` — every folder the request
// carries, duplicates folded, first spelling kept — and these cases used to
// read it out of the evidence sentence above the refusals, because that was
// the longest sentence it reached. The refusals and their evidence are
// `BuildTab`'s now; `foundVolumeNames` is still this tab's and still feeds
// `MaterialFolders`'s `osinstall.media.found`, which counts and names the
// very same list. Same claim, same value, read where it is still drawn.

describe("the found line covers the added folders too (ART-256)", () => {
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
    render(<FilesTab />);

    // The whole sentence: `Extras3.2` is named because the plan reads it, and
    // a line that stopped at the main folder would say "1 install disk found:
    // Workbench3.2" about a build that had already found two.
    expect(
      await screen.findByText(foundLine(["Workbench3.2", "Extras3.2"]))
    ).toBeTruthy();

    // And the added folder really was scanned — the screen cannot be right
    // about this by accident downstream of a scan that never happened.
    await waitFor(() => expect(scanMediaMock).toHaveBeenCalledWith(EXTRA));
  });

  // The scope trap the brief names. A layered release passes
  // `extraMediaFolders: []` on the wire and renders no add-folder control at
  // all, so the evidence must not reach into a folder list the request does
  // not carry — the layered fields have their own scans (`layerScans`), and
  // counting a folder twice would be the mistake this fix is preventing in
  // the other direction.
  it("reads a layered release's tagged folders and names the ones it will not", async () => {
    scanPerFolder();
    seedRemembered({
      ...FULL_FIELDS,
      "buildSession.release": "AmigaOS 3.2.2",
      "osinstall.extraMediaFolders.AmigaOS 3.2.2": [EXTRA],
      // `osinstall.mediaFolder.<layerId>.<release>` — the per-layer key
      // `seededMaterial` migrates into a tagged entry.
      "osinstall.mediaFolder.base.AmigaOS 3.2.2": "E:\\base322",
    });
    render(<FilesTab />);

    // Waits until **the recipe's layers have landed**: `layers === []` is one
    // value with two causes, and until `layersFor` answers, a layered release
    // honestly looks unlayered from here (ART-256/ART-257's own `layersKnown`
    // reasoning).
    //
    // The *shape* of the request a layered release sends — the map alone,
    // `mediaFolder: ""`, `extraMediaFolders: []`, because `plan.rs` ignores
    // the flat fields for one — is `foldersForPlan`'s, and is asserted
    // against it directly in `buildSession.test.ts`. Tab 1 stopped planning
    // in round 4 task 5's fix round; what it still owes is that the tags
    // reach that function, which is what the two assertions below measure.
    await waitFor(() => expect(layersForMock).toHaveBeenCalledWith("AmigaOS 3.2.2"));

    // **The untagged folder is named, not dropped.** It is in the list, ART
    // resolves it in the readout, and the plan cannot carry it — a screen
    // that said nothing would be contradicting the core about what it is
    // going to do.
    const unused = await screen.findByTestId("material-unused");
    expect(unused.textContent).toContain(EXTRA);

    // **And its disks are not counted as found** — the exclusion this test
    // exists for (ART-256's scope trap), restored as an assertion after the
    // unified list made the old one untrue (round 2 whole-branch review, L9).
    //
    // The material list legitimately scans *every* folder now, because the
    // readout resolves against all of them, so "was this folder scanned" is
    // no longer the question. The question is what reaches the **found**
    // line, which `foundVolumeNames` filters to `plannedFolderPaths`: a
    // layered release's tagged folders alone. `E:\extra` holds `Extras3.2`
    // and is untagged, so that volume must never be named.
    //
    // Both arms, so this is an exclusion and not an empty answer: the tagged
    // folder's own disk *is* named.
    expect(await screen.findByText(foundLine(["Workbench3.2"]))).toBeTruthy();
    expect(screen.queryByText(/Extras3\.2/)).toBeNull();
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
    render(<FilesTab />);

    // Named once, and counted once — a line that named it twice would be a
    // worse account of the same disk, and `media-ambiguous` already says
    // "two folders, one name" properly, disk by disk, in the refusals list
    // tab 4 draws.
    expect(await screen.findByText(foundLine(["Workbench3.2"]))).toBeTruthy();
    // Proven the second folder was actually read, so this is a fold and not
    // a folder that never contributed.
    await waitFor(() => expect(scanMediaMock).toHaveBeenCalledWith(EXTRA));
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
    render(<FilesTab />);

    // The main folder's own spelling survives — it is scanned before the
    // added folder in `foundVolumeNames`'s own walk order.
    expect(await screen.findByText(foundLine(["Workbench3.2"]))).toBeTruthy();
    expect(screen.queryByText(/WORKBENCH3\.2/)).toBeNull();
    await waitFor(() => expect(scanMediaMock).toHaveBeenCalledWith(EXTRA));
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
    render(<FilesTab />);

    await waitFor(() => expect(packagesMock).toHaveBeenCalled());

    // **Every** call, never "some call" — and that is not pedantry, it is
    // what a mutation caught. Two panels used to ask this same question, so
    // `toHaveBeenCalledWith(...)` was satisfied by either one of them alone:
    // hardcoding the release inside one panel left the first version of this
    // test green, because the other panel still passed the right one. Only
    // `AmigaInstallPanel` asks it on this screen now, and the assertion stays
    // in its stronger form because a second caller is one round away — tab 2
    // asks `osinstall_chain` the same way. An assertion that one caller is
    // correct says nothing at all about the other.
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
  // (`pages/osbuilder/steps.tsx`) mount `ChoiceTab` and the panels themselves
  // and read `useBuildSession`, so the only thing connecting the picker on
  // this screen to what those steps offer is
  // the session's own remembered key. While this screen owned
  // `osinstall.release` instead, the picker moved one variable and the steps
  // read the other, and nothing in a test of this component alone could see
  // it.
  it("moves the build session's own release, which is what the steps read", async () => {
    seedRemembered(FULL_FIELDS);
    render(<FilesTab />);
    await waitFor(() => expect(layersForMock).toHaveBeenCalled());

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
  //
  // **One answer is left on this tab to clear** (round 4 task 5): the install
  // report, the plan error and the run confirmation all went to tab 4 with
  // the sections that set them, and the sibling case about the report went
  // with them. What survives is the case that always carried the rule.

  it("takes an answer a control gave down when the release changes", async () => {
    // The mechanism is a button on the screen rather than an event from the
    // backend. One test that clears one piece of state proves one piece of
    // state.
    //
    // "Scan again" is the right case because **nothing else can clear it**.
    // A plan error would be wiped by the re-plan a release switch triggers
    // anyway, so a test built on one would pass against the defect exactly
    // as happily as against the fix.
    seedRemembered(FULL_FIELDS);
    render(<FilesTab />);
    await waitFor(() => expect(layersForMock).toHaveBeenCalled());

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

describe("the tree the next steps get (ART-197)", () => {
  // **This tab does not build one any more** (round 4 task 5). The two cases
  // about a finished install — the hand-off to the session and the sentence
  // that says so — went with the run; `BuildTab.test.tsx` holds the first as
  // `hands the finished tree to the session, once, and never on render
  // (ART-197)`. What is left here is the other half of ART-197, which is
  // still this tab's: the panel at its foot reads the *one* session value,
  // whether the user picked the tree by hand or ART found a build in the
  // destination.
  it("carries a tree the user picked by hand, without a build", async () => {
    // The migration's own case, at the screen: a user upgrading into this
    // build finds the packages panel pointing where they last pointed it.
    seedRemembered({
      ...FULL_FIELDS,
      "osinstall.packages.treeRoot": "E:\\amiga\\picked-by-hand",
    });
    render(<FilesTab />);

    // `AmigaInstallPanel` renders the tree root through `Field`, as plain
    // text beside its Browse button, and reads it from the session rather
    // than from a key of its own — which is the whole of the migration.
    //
    // **Once, and it has been three and two before.** `VerifyAgainstCard`
    // moved to the volumes step in wave 3 and `PackagePanel` is deleted in
    // round 3 task 3; each move cost nothing precisely because every panel
    // reads the one session value. The count is asserted rather than "at
    // least one" so a panel that started reading a key of its own again
    // would be caught here, which is the defect this case is named for.
    const shown = await screen.findAllByText("E:\\amiga\\picked-by-hand");
    expect(shown.length).toBe(1);
  });

  it("hands the panels the destination once ART finds a build in it", async () => {
    // **One tree for both tabs** (fix round 1, Important 2). Tab 2 asks the
    // chain about the destination when `osinstall_describe_tree` calls it a
    // build; this screen used `session.tree.root` regardless, so the two
    // lanes could give two `installed` answers about one file. Both read
    // `useChainTree` now, and this is the case that says so from the screen's
    // side: the session's own tree is *not* what the panel is handed.
    describeTreeMock.mockResolvedValue({
      isTree: true,
      release: "AmigaOS 3.2",
      files: 1915,
      components: ["workbench-base"],
      amigaInstalled: [],
      problem: null,
    });
    seedRemembered({
      ...FULL_FIELDS,
      "buildSession.tree": { root: "E:\\amiga\\somewhere-else", builtHere: false },
    });
    render(<FilesTab />);

    // **And it says so rather than offering a field that cannot change it**
    // (round 3 fix wave, Important 2): the panel's own Browse wrote
    // `session.tree.root`, which `useChainTree` overrules while the
    // destination wins, so the path snapped straight back. The sentence
    // carries the path and names the tab that owns it.
    const said = await screen.findByTestId("amiga-tree-from-destination");
    expect(said.textContent).toContain("E:\\dist");
    expect(screen.queryByTestId("amiga-tree-root-field")).toBeNull();
    await waitFor(() =>
      expect(screen.queryByText("E:\\amiga\\somewhere-else")).toBeNull()
    );

    // **Asked once for the one path** (fix round 2). This screen already asks
    // `useDestinationCheck` for its occupied-folder refusal and hands that
    // answer to `useChainTree`; a hook asking again would be a second reader
    // of the one fact it exists to have one of.
    expect(describeTreeMock.mock.calls.filter((call) => call[0] === "E:\\dist")).toHaveLength(1);
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
  // **Read through the found line since round 4 task 5's fix round.** This
  // asserted the request `osinstall_plan` was handed — `mediaFolder` plus
  // `extraMediaFolders` — and tab 1 does not plan. The claim is the same one
  // either way: every folder in the list is material, not just the first.
  // What draws it here is `foundVolumeNames`, which walks exactly the folders
  // `foldersForPlan` says the request carries; what hands those folders to
  // Rust is `useInstallPlan`, asserted in `useInstallPlan.test.tsx` and from
  // the screen in `ChoiceTab.test.tsx`.
  it("reads every folder in the list, not only the first", async () => {
    scanTwoFolders(UPDATE_FOLDER, "Extras3.2");
    await renderFull();
    await addFolder(UPDATE_FOLDER);

    await waitFor(() => expect(scanMediaMock).toHaveBeenCalledWith(UPDATE_FOLDER));
    expect(scanMediaMock).toHaveBeenCalledWith(MAIN_FOLDER);
    // Both folders' disks in one sentence, in list order — the added folder's
    // `Extras3.2` beside the first folder's `Workbench3.2`.
    expect(
      await screen.findByText(
        i18n.t("osinstall.media.found", { count: 2, names: "Workbench3.2, Extras3.2" })
      )
    ).toBeTruthy();
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
    scanTwoFolders(UPDATE_FOLDER, "Extras3.2");
    await renderFull();
    await addFolder(UPDATE_FOLDER);
    await screen.findByText(
      i18n.t("osinstall.media.found", { count: 2, names: "Workbench3.2, Extras3.2" })
    );

    await userEvent.click(
      screen.getByRole("button", {
        name: i18n.t("osinstall.media.removeFolderAriaLabel", { folder: UPDATE_FOLDER }),
      })
    );

    await waitFor(() => expect(screen.queryAllByTestId("material-folder")).toHaveLength(1));
    // And the removed folder's disks stop being material — the row going is
    // not the whole claim, since the list is what everything downstream is
    // computed from. (It used to be read off the plan request; tab 1 does not
    // plan.)
    await waitFor(() =>
      expect(
        screen.getByText(i18n.t("osinstall.media.found", { count: 1, names: "Workbench3.2" }))
      ).toBeTruthy()
    );
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

    await userEvent.click(removeUpdate);

    // Removing the row named "Update" left "Hotfix" behind — proof the
    // accessible name actually picked out the right row, not just any one
    // with visible text "Remove". Read off the rows themselves since round 4
    // task 5's fix round; it used to be read off the plan request, and tab 1
    // does not plan.
    await waitFor(() => expect(screen.queryAllByTestId("material-folder")).toHaveLength(2));
    const left = screen.getAllByTestId("material-folder").map((row) => row.textContent ?? "");
    expect(left.some((row) => row.includes("Hotfix"))).toBe(true);
    expect(left.some((row) => row.includes("Update"))).toBe(false);
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
    render(<FilesTab />);
    // The plan left this tab in round 4 task 5's fix round, so the wait is on
    // the one question it does ask about a release.
    await waitFor(() => expect(layersForMock).toHaveBeenCalledWith("AmigaOS 3.2"));
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
    render(<FilesTab />);

    await waitFor(() => expect(packagesMock).toHaveBeenCalled());
    // **Every** call, never "some call": two panels used to ask, and one of
    // them being right said nothing about the other. Kept in that form now
    // that one asks, because the weaker assertion is what let the defect
    // through the first time.
    for (const call of packagesMock.mock.calls) expect(call[0]).toBe("E:\\archives");

    // And both folders really are in the one list, which is what makes the
    // readout and the planner see the archives at all.
    const rows = await screen.findAllByTestId("material-folder");
    expect(rows.map((row) => row.textContent)).toEqual([
      expect.stringContaining("E:\\disks"),
      expect.stringContaining("E:\\archives"),
    ]);
  });

  /// **`it("follows the packages panel's own Browse")` was here, and its
  /// control is gone** (round 3 task 3). The case drove `PackagePanel`'s
  /// package-folder field, which is deleted with the panel; the rule it
  /// guarded is not this screen's and did not go with it. `setPackages({
  /// folder })` writing **both** the stored value and the material list —
  /// the half a report once missed, where Browse appended to the list while
  /// the field went on showing the list's head — is asserted at the hook, in
  /// `useBuildSession.test.tsx` ("drops the stored archives folder when the
  /// list stops holding it", which clicks *choose archives* and reads both
  /// back). `AmigaInstallPanel` still takes `packageFolder` from the session
  /// and has no picker of its own.

  /// A user who never kept a separate archives folder gets the one list's
  /// answer for free — which is the whole point of deriving at all.
  it("derives the list's first folder when nothing was ever stored", async () => {
    seedRemembered({
      "osinstall.mediaFolder": "E:\\media",
      "osinstall.rom": "E:\\roms\\kick.rom",
      "osinstall.destination": "E:\\dist",
    });
    render(<FilesTab />);
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
    render(<FilesTab />);

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
    return render(<FilesTab />);
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
    render(<FilesTab />);
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
    render(<FilesTab />);
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
    render(<FilesTab />);
    const ask = await screen.findByTestId("material-ask");
    expect(ask.textContent).toBe(i18n.t("osinstall.material.askFolders"));
    expect(ask.textContent).not.toContain("osinstall.");
    expect(ask.textContent).not.toContain("{{");
  });
});

// **The five keyboard cases moved to `MachineTab.test.tsx` by name** (round
// 4 task 4, four-tab design § 3.3): "says nothing when the install places no
// keymaps", "offers what the plan will really place, and not the icons",
// "sends the chosen layout to the planner", "sends null when nothing is
// chosen, leaving it to Kickstart" and "remembers the choice per release".
// The select is on tab 3 now, beside the Kickstart; this screen still plans
// with the value and no longer draws it.

// ART-207's own rule taken one level finer: instead of one media folder plus
// a bag of extra ones, a layered recipe (AmigaOS 3.2.2's own `base` and
// `update-3.2.2`) is asked one **labelled** question per layer it declares.
// `layersFor` carries that shape from the recipe (Task 8's own `label_key`s)
// to the screen; an unlayered release answers empty, and the screen must
// render exactly what it always has for one.
describe("saying which part of a layered release a folder holds (Task 10)", () => {
  it("offers every layer the recipe declares, in the recipe's own order", async () => {
    renderFilesTab({ release: "AmigaOS 3.2.2" });
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

  // **What the map itself looks like is `foldersForPlan`'s**, asserted
  // against that function in `buildSession.test.ts` — this used to read it
  // out of the request `osinstall_plan` was handed, and tab 1 does not plan
  // since round 4 task 5's fix round. What a screen can still get wrong is
  // upstream of it: a tag that never reaches the list. So both tagged folders
  // must be material and neither may be listed as one the request will skip.
  it("makes one folder per layer material, and calls neither of them unused", async () => {
    const BASE = "E:\\media\\3.2";
    const UPDATE = "E:\\media\\Update3.2.2";
    scanTwoFolders(UPDATE, "Update3.2.2");
    await withLayerFolders({ base: BASE, "update-3.2.2": UPDATE });

    // Both tagged folders' own disks, in layer order — `foundVolumeNames`
    // walks exactly the folders `foldersForPlan` says the request carries, so
    // a tag that failed to land would drop its folder out of this sentence.
    expect(
      await screen.findByText(foundLine(["Workbench3.2", "Update3.2.2"]))
    ).toBeTruthy();
    // And neither is called a folder the request will not read.
    expect(screen.queryByTestId("material-unused")).toBeNull();
  });

  it("offers no layer at all for an unlayered release", async () => {
    renderFilesTab({ release: "AmigaOS 3.2" });
    await addFolder("E:\\media");
    // A release whose recipe declares no layers has no part to ask about, so
    // the row is a path and a Remove button and nothing else.
    expect(screen.queryAllByRole("combobox", { name: /E:..media/ })).toHaveLength(0);
  });

  it("remembers each folder's own layer across a remount", async () => {
    // ART's standing rule: nothing changes unless the user changes it — and
    // per folder, since a tag that moved would hand the update part the base
    // disks the first time somebody switched releases.
    const { unmount } = renderFilesTab({ release: "AmigaOS 3.2.2" });
    await browseLayerFolder("base", "E:\\a");
    await browseLayerFolder("update-3.2.2", "E:\\b");

    // Remount **without re-seeding**: `renderFilesTab`'s own
    // `seedRemembered` replaces the whole remembered bag, which would defeat
    // the point of this test by wiping the very writes it is asking about. A
    // real remount reads back whatever `settings.json` actually holds.
    unmount();
    render(<FilesTab />);

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

    renderFilesTab({ release: "AmigaOS 3.2.2" });
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

    renderFilesTab({ release: "AmigaOS 3.2.2" });
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
    renderFilesTab({ release: "AmigaOS 3.2.2" });
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
    renderFilesTab({ release: "AmigaOS 3.2.2" });
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
    render(<FilesTab />);
    await waitFor(() => expect(layersForMock).toHaveBeenCalled());

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
    render(<FilesTab />);
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
