// @vitest-environment jsdom
//
// Tab 4 — *Derle* (four-tab design § 3.4): four summary lines, one button,
// the sequenced run's report, and the hand-off to the card lane.
//
// What these cases are about is the **screen**, not the sequence: `buildRun.ts`
// and `useBuildRun.test.tsx` own what the phases are and which ending each one
// earns. Here the questions are the ones only a screen can answer wrongly — a
// button offered over a plan that refuses, a run started without the person
// saying so, a phase row that shows an ending and no next step, a progress bar
// that draws a fixed width over a total nobody knows, and a hand-off that
// navigates without setting the kind it navigates for.
//
// Mocked at the same boundary the rest of this suite mocks at — the `@/lib/*`
// wrappers around `invoke`, never `@tauri-apps/api` — with `@/lib/settings` one
// layer further down for `useRemembered`'s own reason (its setter fires
// `saveSettings`, which rejects unhandled in jsdom).

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { MemoryRouter, Route, Routes, useLocation } from "react-router-dom";
import i18n from "i18next";

import { changeLanguage } from "@/i18n";
import { useSettingsStore } from "@/stores/settingsStore";
import type { FirstBootWritten } from "@/lib/firstboot";
import type { JobProgress } from "@/lib/jobs";
import type {
  ApplyOutcome,
  ChainReport,
  ChainRow,
  ChainState,
  ComponentDef,
  InstallPlan,
  InstallRequest,
  MediaScanResult,
  OsInstallResult,
  PlanResult,
  RefusalReason,
  SlotReport,
  TreeSummary,
} from "@/lib/osinstall";
import type { RomInfo } from "@/lib/pistorm";

const componentsMock = vi.hoisted(() => vi.fn());
const layersForMock = vi.hoisted(() => vi.fn());
const planMock = vi.hoisted(() => vi.fn());
const componentCollisionsMock = vi.hoisted(() => vi.fn());
const collisionsMock = vi.hoisted(() => vi.fn());
const chainMock = vi.hoisted(() => vi.fn());
const slotsMock = vi.hoisted(() => vi.fn());
const describeTreeMock = vi.hoisted(() => vi.fn());
const destinationTakenMock = vi.hoisted(() => vi.fn());
const scanMediaMock = vi.hoisted(() => vi.fn());
const mediaEvidenceMock = vi.hoisted(() => vi.fn());
const releaseForMediaMock = vi.hoisted(() => vi.fn());
const applyMock = vi.hoisted(() => vi.fn());
const addPackageMock = vi.hoisted(() => vi.fn());
const identifyRomMock = vi.hoisted(() => vi.fn());
const firstbootWriteMock = vi.hoisted(() => vi.fn());
const firstbootPreviewMock = vi.hoisted(() => vi.fn());
const awaitJobResultMock = vi.hoisted(() => vi.fn());
const jobCancelMock = vi.hoisted(() => vi.fn());
const onJobProgressMock = vi.hoisted(() => vi.fn());

vi.mock("@/lib/osinstall", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/lib/osinstall")>()),
  osinstallComponents: componentsMock,
  layersFor: layersForMock,
  osinstallPlan: planMock,
  osinstallComponentCollisions: componentCollisionsMock,
  osinstallCollisions: collisionsMock,
  osinstallChain: chainMock,
  osinstallSlots: slotsMock,
  osinstallDescribeTree: describeTreeMock,
  osinstallDestinationTaken: destinationTakenMock,
  osinstallScanMedia: scanMediaMock,
  osinstallMediaEvidence: mediaEvidenceMock,
  osinstallReleaseForMedia: releaseForMediaMock,
  osinstallApply: applyMock,
  osinstallAddPackage: addPackageMock,
}));

vi.mock("@/lib/firstboot", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/lib/firstboot")>()),
  firstbootWrite: firstbootWriteMock,
  firstbootPreview: firstbootPreviewMock,
}));

vi.mock("@/lib/pistorm", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/lib/pistorm")>()),
  pistormIdentifyRom: identifyRomMock,
}));

vi.mock("@/lib/jobs", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/lib/jobs")>()),
  awaitJobResult: awaitJobResultMock,
  jobCancel: jobCancelMock,
  onJobProgress: onJobProgressMock,
}));

vi.mock("@/lib/settings", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/lib/settings")>()),
  getSettings: vi.fn(),
  saveSettings: vi.fn().mockResolvedValue(undefined),
}));

const { BuildTab } = await import("@/components/osbuilder/BuildTab");
const { DEFAULT_SETTINGS } = await import("@/lib/settings");
const { OSINSTALL_EVENT } = await import("@/lib/osinstall");

// ---------------------------------------------------------------------------
// What the core answers with
// ---------------------------------------------------------------------------

const DEST = "E:\\amiga\\Amigatolon\\sonuclar";
const MEDIA = "E:\\amiga\\os39";
const ARCHIVES = "E:\\amiga\\arsiv";
const BB1 = `${ARCHIVES}\\BoingBag39-1.lha`;
const BB2 = `${ARCHIVES}\\BoingBag39-2.lha`;

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

/** AmigaOS 3.9's two layering parts, cut down: `workbench-39` overrides
 *  `workbench-base`, which is what makes `layeringOn` non-empty and the
 *  component preview a real answer rather than an absent one. */
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
  {
    id: "workbench-39",
    media: "AmigaOS3.9",
    labelKey: "osinstall.components.name.os39.overlay",
    required: true,
    available: true,
    conditionMajor: null,
    requiresRomMajor: null,
    exclusiveGroup: null,
    overrides: ["workbench-base"],
  },
];

const ITEM = {
  component: "workbench-base",
  media: "AmigaOS3.9",
  from: "AmigaOS3.9:C/Format",
  to: "C/Format",
  isDir: false,
  decompress: false,
  bytes: 2 * 1024 * 1024,
  mergeIcon: false,
};

let refusals: RefusalReason[] = [];

function planResultFor(req: InstallRequest): PlanResult {
  const plan: InstallPlan = {
    release: req.release,
    items: [ITEM],
    refusals,
    totalBytes: 18_300_000,
    totalFiles: 1980,
    componentsOn: ["workbench-base", "workbench-39"],
    mediaPaths: { "AmigaOS3.9": `${MEDIA}\\AmigaOS39.iso` },
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

const NOT_A_TREE: TreeSummary = {
  isTree: false,
  release: null,
  files: 0,
  components: [],
  amigaInstalled: [],
  problem: "holds no distribution.json",
};

const IS_A_TREE: TreeSummary = {
  isTree: true,
  release: "AmigaOS 3.9",
  files: 4212,
  components: ["workbench-base", "workbench-39"],
  amigaInstalled: [],
  problem: null,
};

const TREE_OUTCOME: ApplyOutcome = {
  root: DEST,
  files: 1980,
  directories: 214,
  bytes: 18_300_000,
  removed: [],
  icons: [],
  iconMergeFailures: 0,
  extraMembers: [],
  postPlace: [],
};

const TREE_RESULT: OsInstallResult = {
  job_id: 11,
  destination: DEST,
  outcome: TREE_OUTCOME,
  stated_release: { verdict: "confirmed", stated: "39.29" },
};

const PACKAGE_OUTCOME: ApplyOutcome = { ...TREE_OUTCOME, files: 166, bytes: 2_100_000 };

const WRITTEN: FirstBootWritten = {
  files: ["S:ART-FirstBoot", "S:ART-FirstBoot-Report"],
  userStartupBackup: null,
  userStartupCreated: true,
};

// ---------------------------------------------------------------------------
// The chain, and the slots that resolve its files
// ---------------------------------------------------------------------------

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

const TWO_UPDATES: ChainRow[] = [
  row({ position: 2, state: { state: "ready" } }),
  row({
    position: 3,
    packageId: "boingbag-39-2",
    slotId: "package:boingbag-39-2",
    name: "BoingBag 3.9-2",
    sentenceFacts: { file: "BoingBag39-2.lha", runsOnAmiga: false },
    state: { state: "ready" },
  }),
];

function chainOf(rows: ChainRow[]): ChainReport {
  return {
    rows,
    summary: {
      release: "AmigaOS 3.9",
      total: rows.length,
      installed: 0,
      notNeeded: 0,
    },
    unreadableFolders: [],
    crowdedFolders: [],
  };
}

/** A slot report that resolves both BoingBags by hash — a find, not a guess,
 *  which is what `useTickedUpdates` will hand the run. The state's shape is
 *  `ChoiceTab.test.tsx`'s own builder, so the two files cannot describe two
 *  different slot reports. */
function slotsOf(found: Record<string, string | null>): SlotReport {
  return {
    states: Object.entries(found).map(([id, path]) => ({
      slot: {
        id,
        kind: "package" as const,
        name: id,
        identity: id,
        artefact: null,
        required: false,
        filenames: [],
        provenance: null,
        position: 2,
        requires: [],
        supersededBy: [],
        expectsDirectories: [],
      },
      found: path
        ? {
            path,
            matchedBy: "hash" as const,
            row: null,
            confirmed: null,
            bytesRead: { state: "read-no-row" as const },
          }
        : null,
      candidates: [],
      installed: { state: "no" as const },
      chosenMissing: null,
      blockedBy: [],
      incomplete: null,
    })),
    summary: {
      release: "AmigaOS 3.9",
      requiredTotal: 0,
      requiredFound: 0,
      optionalTotal: 2,
      optionalFound: 2,
    },
    unreadableFolders: [],
    crowdedFolders: [],
  };
}

// ---------------------------------------------------------------------------
// The harness
// ---------------------------------------------------------------------------

function seed(remembered: Record<string, unknown>) {
  useSettingsStore.setState({
    loaded: true,
    settings: { ...DEFAULT_SETTINGS, remembered: { ...remembered } },
  });
}

/** Everything tab 4 reads: the release, the material folders, the Kickstart
 *  and the per-release destination. */
const FIELDS: Record<string, unknown> = {
  "buildSession.kind": "install",
  "buildSession.release": "AmigaOS 3.9",
  "buildSession.rom": { path: "E:\\roms\\kick.rom" },
  "buildSession.material.AmigaOS 3.9": {
    folders: [
      { path: MEDIA, layer: null },
      { path: ARCHIVES, layer: null },
    ],
  },
  "osinstall.destination.AmigaOS 3.9": DEST,
};

function WhereAmI() {
  const location = useLocation();
  return <div data-testid="where">{location.pathname}</div>;
}

function renderTab() {
  return render(
    <MemoryRouter initialEntries={["/os-builder/derle"]}>
      <Routes>
        <Route
          path="/os-builder/*"
          element={
            <>
              <BuildTab />
              <WhereAmI />
            </>
          }
        />
      </Routes>
    </MemoryRouter>
  );
}

/** The summary lines, as sentences. */
function summaryLines(): string[] {
  return screen.getAllByTestId("build-summary-line").map((el) => el.textContent ?? "");
}

/** Which command ran, in the order it ran. */
let order: string[] = [];
/** The one live `onJobProgress` handler, so a test can deliver an update the
 *  way the backend would. */
let report: ((job: JobProgress) => void) | null = null;

type Start = () => Promise<number>;

/** `awaitJobResult`'s own contract: call `start` — which invokes the command
 *  and hands the hook the job id — then settle over the result payload. */
async function settles(
  event: string,
  start: Start,
  extract: (payload: unknown) => unknown
): Promise<unknown> {
  await start();
  return extract(
    event === OSINSTALL_EVENT ? TREE_RESULT : { job_id: 12, outcome: PACKAGE_OUTCOME }
  );
}

/** A job that has started and has not answered yet; the test settles it. */
function pending(): { resolve: (value: unknown) => void; reject: (err: unknown) => void } {
  const handle = { resolve: (_v: unknown) => {}, reject: (_e: unknown) => {} };
  awaitJobResultMock.mockImplementationOnce(async (_event: string, start: Start) => {
    await start();
    return new Promise((resolve, reject) => {
      handle.resolve = resolve;
      handle.reject = reject;
    });
  });
  return handle;
}

async function flush() {
  await act(async () => {
    await Promise.resolve();
  });
}

beforeEach(() => {
  order = [];
  report = null;
  refusals = [];
  componentsMock.mockReset().mockResolvedValue(COMPONENTS_39);
  layersForMock.mockReset().mockResolvedValue([]);
  planMock
    .mockReset()
    .mockImplementation((req: InstallRequest) => Promise.resolve(planResultFor(req)));
  componentCollisionsMock
    .mockReset()
    .mockResolvedValue({ reports: [{ path: "C/Format", collision: { collision: "newer" } }], placed: 44, contested: 4 });
  collisionsMock.mockReset().mockResolvedValue([]);
  chainMock.mockReset().mockResolvedValue(chainOf(TWO_UPDATES));
  slotsMock
    .mockReset()
    .mockResolvedValue(
      slotsOf({ "package:boingbag-39-1": BB1, "package:boingbag-39-2": BB2 })
    );
  describeTreeMock.mockReset().mockResolvedValue(NOT_A_TREE);
  destinationTakenMock.mockReset().mockResolvedValue(false);
  scanMediaMock.mockReset().mockResolvedValue({
    outcome: "found",
    media: [{ path: `${MEDIA}\\AmigaOS39.iso`, volumeName: "AmigaOS3.9", kind: "disc" }],
  } satisfies MediaScanResult);
  mediaEvidenceMock.mockReset().mockResolvedValue({
    release: "AmigaOS 3.9",
    distinguishing: ["AmigaOS3.9"],
    shared: [],
  });
  releaseForMediaMock.mockReset().mockResolvedValue("AmigaOS 3.9");
  identifyRomMock.mockReset().mockResolvedValue(ROM);
  firstbootPreviewMock.mockReset().mockResolvedValue({
    tree: DEST,
    steps: [],
    fatMount: { kind: "available" },
    userStartupExists: false,
    alreadyWritten: false,
    bytesAdded: 4096,
  });
  applyMock.mockReset().mockImplementation(async () => {
    order.push("apply");
    return 11;
  });
  addPackageMock.mockReset().mockImplementation(async () => {
    order.push("addPackage");
    return { outcome: "started", job_id: 12 };
  });
  firstbootWriteMock.mockReset().mockImplementation(async () => {
    order.push("firstboot");
    return WRITTEN;
  });
  jobCancelMock.mockReset().mockResolvedValue(true);
  onJobProgressMock.mockReset().mockImplementation(async (handler: (job: JobProgress) => void) => {
    report = handler;
    return () => {};
  });
  awaitJobResultMock.mockReset().mockImplementation(settles);
  useSettingsStore.setState({ loaded: false, settings: DEFAULT_SETTINGS });
});

afterEach(async () => {
  cleanup();
  useSettingsStore.setState({ loaded: false, settings: DEFAULT_SETTINGS });
  await changeLanguage("en");
});

/** Both BoingBags ticked, on a fresh destination. The tick list holds the
 *  chain **line** ids, which are the package ids — the same key and the same
 *  shape tab 2 writes. */
function bothTicked() {
  seed({
    ...FIELDS,
    "buildSession.packages.AmigaOS 3.9": {
      folder: null,
      chosen: ["boingbag-39-1", "boingbag-39-2"],
    },
  });
}

// ---------------------------------------------------------------------------

describe("tab 4's four summary lines", () => {
  it("says what will be written, what will be updated, first boot, and what it would replace", async () => {
    bothTicked();
    renderTab();
    await screen.findByTestId("build-run");
    await waitFor(() => expect(summaryLines()[1]).toContain("BoingBag 3.9-2"));

    const lines = summaryLines();
    expect(lines).toHaveLength(4);
    // The tree: the plan's own totals, and the components by the **label**
    // the catalogue gives them — not the raw recipe ids `src/lib` can only
    // hand over (task 1's concern 3).
    expect(lines[0]).toContain("1980");
    expect(lines[0]).toContain(i18n.t("osinstall.components.name.os39.overlay"));
    expect(lines[0]).not.toContain("workbench-39");
    // The updates, in the chain's order, named.
    expect(lines[1]).toContain("BoingBag 3.9-1");
    expect(lines[1]).toContain("BoingBag 3.9-2");
    expect(lines[1].indexOf("BoingBag 3.9-1")).toBeLessThan(lines[1].indexOf("BoingBag 3.9-2"));
    expect(lines[2]).toBe(i18n.t("osBuilder.build.summary.firstboot"));
    // What it would replace: the component preview's own three counts, with
    // the two that describe only the release's parts said to be about them.
    await waitFor(() =>
      expect(summaryLines()[3]).toBe(
        i18n.t("osBuilder.build.summary.replaces", { replaced: 1, fresh: 40, unchanged: 3 })
      )
    );
  });

  it("names a ticked update whose file ART would not trust, and does not run it", async () => {
    slotsMock.mockResolvedValue(
      slotsOf({ "package:boingbag-39-1": null, "package:boingbag-39-2": BB2 })
    );
    bothTicked();
    renderTab();

    const named = await screen.findByTestId("build-unresolved");
    expect(named.textContent).toContain("BoingBag 3.9-1");
    // …and the run's own line names only the one that can run.
    await waitFor(() => expect(summaryLines()[1]).toContain("BoingBag 3.9-2"));
    expect(summaryLines()[1]).not.toContain("BoingBag 3.9-1");
  });
});

describe("the button, and what stands in its place", () => {
  it("renders the plan's refusal and no Build button (design § 7)", async () => {
    refusals = [
      { refusal: "media-missing", component: "workbench-base", volume_name: "AmigaOS3.9" },
    ];
    seed(FIELDS);
    renderTab();

    const card = await screen.findByTestId("build-refusals");
    expect(card.textContent).toContain(
      i18n.t("osinstall.refusal.mediaMissing", {
        component: "workbench-base",
        volume: "AmigaOS3.9",
      })
    );
    expect(screen.queryByTestId("build-run")).toBeNull();
    expect(screen.getByTestId("build-blocker").textContent).toBe(
      i18n.t("osinstall.blocked.refusals")
    );
  });

  it("offers the switch-release button when the folder holds another release's media", async () => {
    // Every disk this release asks for is missing, and the folder identifies
    // as a release ART ships — ART-208's one wrong folder, one sentence.
    refusals = [
      { refusal: "media-missing", component: "workbench-base", volume_name: "AmigaOS3.9" },
    ];
    scanMediaMock.mockResolvedValue({
      outcome: "found",
      media: [{ path: `${MEDIA}\\Workbench.adf`, volumeName: "Workbench3.2", kind: "floppy" }],
    } satisfies MediaScanResult);
    releaseForMediaMock.mockResolvedValue("AmigaOS 3.2");
    mediaEvidenceMock.mockResolvedValue({
      release: "AmigaOS 3.9",
      distinguishing: [],
      shared: [],
    });
    seed(FIELDS);
    renderTab();

    const button = await screen.findByTestId("build-switch-release");
    expect(button.textContent).toBe(
      i18n.t("osinstall.blocked.switchRelease", { release: "AmigaOS 3.2" })
    );
    // ART-202: the per-disk list is suppressed for this case, so the sentence
    // is said once.
    expect(screen.queryByTestId("build-refusals")).toBeNull();
    expect(screen.queryByTestId("build-run")).toBeNull();
  });

  it("will not run until the person says they have read the summary", async () => {
    bothTicked();
    renderTab();

    const button = (await screen.findByTestId("build-run")) as HTMLButtonElement;
    expect(button.disabled).toBe(true);
    await userEvent.click(screen.getByTestId("build-confirm"));
    await waitFor(() => expect((screen.getByTestId("build-run") as HTMLButtonElement).disabled).toBe(false));
  });

  it("says what is missing when nothing at all is ticked on an existing tree", async () => {
    describeTreeMock.mockResolvedValue(IS_A_TREE);
    destinationTakenMock.mockResolvedValue(true);
    seed({ ...FIELDS, "buildSession.firstboot": { written: false, wanted: false } });
    renderTab();

    await waitFor(() =>
      expect(screen.getByTestId("build-blocker").textContent).toBe(
        i18n.t("osBuilder.build.blocked.nothingToRun")
      )
    );
    expect(screen.queryByTestId("build-run")).toBeNull();
  });
});

describe("pressing Build", () => {
  it("runs the tree, then the ticked updates in the chain's order, then first boot", async () => {
    bothTicked();
    renderTab();
    await screen.findByTestId("build-run");
    await userEvent.click(screen.getByTestId("build-confirm"));
    await userEvent.click(await screen.findByTestId("build-run"));

    await waitFor(() => expect(screen.getAllByTestId("build-phase-row")).toHaveLength(4));
    // The tree first, and every package after it — asserted as an order,
    // because "both were called" is true of the wrong order too.
    expect(order).toEqual(["apply", "addPackage", "addPackage", "firstboot"]);
    expect(addPackageMock.mock.calls[0][2]).toEqual(["boingbag-39-1"]);
    expect(addPackageMock.mock.calls[1][2]).toEqual(["boingbag-39-2"]);
    // ART-289: each row is applied with the file the readout resolved.
    expect(addPackageMock.mock.calls[0][1]).toBe(ARCHIVES);
    expect(addPackageMock.mock.calls[0][3]).toEqual([["package:boingbag-39-1", BB1]]);
  });

  it("runs no tree on a destination that is already a tree, and Run is offered anyway", async () => {
    describeTreeMock.mockResolvedValue(IS_A_TREE);
    // `apply` would refuse this folder — it is full — and the plan refuses
    // too, because the install media is no longer in the folders. **Neither
    // is a reason to withhold this run**: update mode has no tree phase, so
    // what runs needs the BoingBag's own archive and nothing else. Both
    // blockers are in the fixture on purpose; with the plan's blocker gating
    // update mode, this case fails.
    destinationTakenMock.mockResolvedValue(true);
    refusals = [
      { refusal: "media-missing", component: "workbench-base", volume_name: "AmigaOS3.9" },
    ];
    bothTicked();
    renderTab();

    await screen.findByTestId("build-run");
    expect(screen.queryByTestId("build-blocker")).toBeNull();
    // …and the refusal is not shown either: it is about a tree this run is
    // not writing, and a red box over a run that is going to work fine is
    // the screen out-claiming the core in the other direction.
    expect(screen.queryByTestId("build-refusals")).toBeNull();
    await userEvent.click(screen.getByTestId("build-confirm"));
    await userEvent.click(screen.getByTestId("build-run"));

    await waitFor(() => expect(order.length).toBeGreaterThan(0));
    expect(order).not.toContain("apply");
    expect(applyMock).not.toHaveBeenCalled();
    // The first line says the tree will be updated rather than written.
    expect(summaryLines()[0]).toBe(
      i18n.t("osBuilder.build.summary.treeExisting", { release: "AmigaOS 3.9", count: 4212 })
    );
  });

  it("carries the tree phase's own release verdict and the time it took", async () => {
    seed({ ...FIELDS, "buildSession.firstboot": { written: false, wanted: false } });
    renderTab();
    await screen.findByTestId("build-run");
    await userEvent.click(screen.getByTestId("build-confirm"));
    await userEvent.click(screen.getByTestId("build-run"));

    const rows = await screen.findAllByTestId("build-phase-row");
    await waitFor(() =>
      expect(rows[0].textContent).toContain(
        i18n.t("osinstall.result.statedRelease.confirmed", { stated: "39.29" })
      )
    );
    // The design's own "count and time" — a phase that took four seconds says so.
    expect(within(rows[0]).getByTestId("build-phase-elapsed").textContent).toMatch(/\d/);
  });

  it("stops at a refusal, lists it, and says the rest was not attempted", async () => {
    const REFUSED: RefusalReason = {
      refusal: "package-folder-missing",
      packages: ["boingbag-39-1"],
    };
    addPackageMock.mockImplementation(async () => {
      order.push("addPackage");
      return { outcome: "refused", refusals: [REFUSED] };
    });
    bothTicked();
    renderTab();
    await screen.findByTestId("build-run");
    await userEvent.click(screen.getByTestId("build-confirm"));
    await userEvent.click(screen.getByTestId("build-run"));

    const rows = await screen.findAllByTestId("build-phase-row");
    await waitFor(() =>
      expect(rows[3].textContent).toBe(
        i18n.t("osBuilder.build.phase.firstboot.notAttempted")
      )
    );
    // The refusal itself, not only the advice: a refused phase that says
    // "fix what the refusal names" with no refusal on screen names nothing.
    expect(within(rows[1]).getByTestId("build-phase-refusal").textContent).toBe(
      i18n.t("osinstall.refusal.packageFolderMissing", { packages: "boingbag-39-1" })
    );
    expect(rows[1].textContent).toContain(i18n.t("osBuilder.build.next.refused"));
    // Only one package was ever asked for.
    expect(order).toEqual(["apply", "addPackage"]);
  });

  it("says how far a job with no total has got, and draws no bar over a total nobody knows", async () => {
    const job = pending();
    seed({ ...FIELDS, "buildSession.firstboot": { written: false, wanted: false } });
    renderTab();
    await screen.findByTestId("build-run");
    await userEvent.click(screen.getByTestId("build-confirm"));
    await userEvent.click(screen.getByTestId("build-run"));
    await waitFor(() => expect(report).not.toBeNull());

    act(() => {
      report!({
        id: 11,
        title: "Building",
        done: 412,
        total: null,
        message: "",
        state: { state: "running" },
      });
    });

    await waitFor(() =>
      expect(screen.getByTestId("build-progress").textContent).toBe(
        i18n.t("osBuilder.build.progress.soFar", { done: 412 })
      )
    );
    expect(screen.queryByTestId("build-progress-bar")).toBeNull();

    // …and a total that *is* known gets the percentage and the bar.
    act(() => {
      report!({
        id: 11,
        title: "Building",
        done: 990,
        total: 1980,
        message: "",
        state: { state: "running" },
      });
    });
    await waitFor(() =>
      expect(screen.getByTestId("build-progress").textContent).toBe(
        i18n.t("osinstall.run.progress.percent", { percent: 50, done: 990, total: 1980 })
      )
    );
    expect(screen.getByTestId("build-progress-bar")).toBeTruthy();

    act(() => job.resolve(TREE_RESULT));
    await flush();
  });
});

describe("the hand-off", () => {
  it("sets the kind the card lane needs and points at it", async () => {
    seed({ ...FIELDS, "buildSession.firstboot": { written: false, wanted: false } });
    renderTab();
    await screen.findByTestId("build-run");
    await userEvent.click(screen.getByTestId("build-confirm"));
    await userEvent.click(screen.getByTestId("build-run"));

    const handoff = await screen.findByTestId("build-handoff");
    expect(handoff.textContent).toContain(DEST);
    const link = within(handoff).getByRole("link");
    expect(link.getAttribute("href")).toBe("/os-builder/kart");

    await userEvent.click(link);
    // The store, not the screen: a lane entered without its kind renders the
    // wrong steps and nothing says why.
    await waitFor(() =>
      expect(
        (useSettingsStore.getState().settings.remembered as Record<string, unknown>)[
          "buildSession.kind"
        ]
      ).toBe("boot-card")
    );
    expect(screen.getByTestId("where").textContent).toBe("/os-builder/kart");
  });

  it("hands the finished tree to the session, once, and never on render (ART-197)", async () => {
    seed({ ...FIELDS, "buildSession.firstboot": { written: false, wanted: false } });
    renderTab();
    await screen.findByTestId("build-run");
    // Nothing written by merely looking at the tab.
    expect(
      (useSettingsStore.getState().settings.remembered as Record<string, unknown>)[
        "buildSession.tree"
      ]
    ).toBeUndefined();

    await userEvent.click(screen.getByTestId("build-confirm"));
    await userEvent.click(screen.getByTestId("build-run"));
    await waitFor(() =>
      expect(
        (useSettingsStore.getState().settings.remembered as Record<string, unknown>)[
          "buildSession.tree"
        ]
      ).toEqual({ root: DEST, builtHere: true })
    );
  });
});

describe("in Turkish", () => {
  it("renders sentences, not keys", async () => {
    await changeLanguage("tr");
    bothTicked();
    renderTab();
    const tab = await screen.findByTestId("build-tab");
    await screen.findByTestId("build-run");
    // Waited on the **content**, not the count: four lines are drawn before
    // the chain has answered, and a language check over a half-answered
    // screen proves nothing about the sentences that were not there yet.
    await waitFor(() => expect(summaryLines()[1]).toContain("BoingBag 3.9-2"));
    await waitFor(() =>
      expect(summaryLines()[3]).not.toBe(i18n.t("osBuilder.build.summary.replacesPending"))
    );
    expect(tab.textContent).not.toContain("osBuilder.");
    expect(tab.textContent).not.toContain("osinstall.");
    expect(tab.textContent).not.toContain("{{");
  });
});
