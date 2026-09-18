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
import { useMemo, useState } from "react";
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
  InstallLayer,
  InstallPlan,
  InstallRequest,
  MediaScanResult,
  OsInstallResult,
  PlanResult,
  RefusalReason,
  SlotReport,
  StatedRelease,
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
const cardOpenMock = vi.hoisted(() => vi.fn());
const cardCloseMock = vi.hoisted(() => vi.fn());
const cardPrepareMock = vi.hoisted(() => vi.fn());
const cardBuildMock = vi.hoisted(() => vi.fn());
const onCardPhaseMock = vi.hoisted(() => vi.fn());

vi.mock("@/lib/cardOs", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/lib/cardOs")>()),
  cardOsOpen: cardOpenMock,
  cardOsClose: cardCloseMock,
  cardOsPrepare: cardPrepareMock,
  cardOsBuild: cardBuildMock,
  onCardOsPhase: onCardPhaseMock,
}));

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
const { RunLockContext } = await import("@/lib/runLock");
const { DEFAULT_SETTINGS } = await import("@/lib/settings");
const { OSINSTALL_EVENT, refusalPhrase } = await import("@/lib/osinstall");
const { CARD_OS_BUILD_EVENT, CARD_OS_PREPARE_EVENT } = await import("@/lib/cardOs");
const { jobStatusLabel } = await import("@/lib/jobs");

// ---------------------------------------------------------------------------
// The card lane's own answers (round 4, task 10)
// ---------------------------------------------------------------------------

const CARD_SESSION = 4;
const CARD_TREE = "E:\\amiga\\ProjeART\\build\\tmp\\art-card-4\\tree";
const CARD_IMAGE = "E:\\amiga\\kart.img";

const PROPOSAL = {
  items: [
    {
      name: "kick40068.A1200",
      titles: ["Games/Turrican/Turrican.slave"],
      titlesMore: 0,
      offer: {
        outcome: "supplied" as const,
        wanted: { name: "kick40068.A1200", crc16: 1234, size: 524_288 },
        by: {
          path: "E:\\roms\\kick40068.A1200",
          name: "kick40068.A1200",
          sizeDisagrees: null,
        },
      },
      rtb: { kind: "loose" as const, path: "E:\\roms\\kick40068.A1200.RTB" },
    },
  ],
  unreadableSlaves: [],
  rtbMissing: false,
};

const PREPARED = { kickstarts: PROPOSAL } as unknown as import("@/lib/cardOs").PreparedCard;

const HEALTH: import("@/lib/cardBuild").HealthReport = {
  items: [
    { check: { kind: "boot-partition-first" }, state: "pass" },
    { check: { kind: "every-partition-can-mount", unmountable: 0 }, state: "pass" },
  ],
  by_hand: [{ kind: "flash-the-card" }],
};

function cardResult(
  ending: import("@/lib/cardOs").CardOsEnding
): import("@/lib/cardOs").CardOsBuildResult {
  return {
    jobId: 13,
    session: CARD_SESSION,
    image: CARD_IMAGE,
    ending,
    whdload: null,
    kickstarts: [],
    steps: [],
    manifestPath:
      ending.ending === "succeeded" ? `${CARD_IMAGE}.art-manifest.json` : null,
    health: ending.ending === "succeeded" ? HEALTH : null,
    partial:
      ending.ending === "succeeded"
        ? { outcome: "not-created" }
        : { outcome: "removed", path: `${CARD_IMAGE}.partial` },
    scratchLeft: null,
  };
}

/** What `card-os-build-result` will carry — the subject of half the card
 *  cases, so it is settable per test. */
let cardAnswer = cardResult({ ending: "succeeded" });

/** What `card-os-prepare-result` will carry. A refusal is `card_os_prepare`'s
 *  own **answer** now, on its own event, exactly as `card_os_measure`'s is —
 *  so it is settable per test too (final review, C2). */
let prepareAnswer: import("@/lib/cardOs").CardOsPrepareResult = {
  jobId: 12,
  session: CARD_SESSION,
  prepared: PREPARED,
  refusal: null,
};

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

function planFor(req: InstallRequest): InstallPlan {
  return {
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
}

function planResultFor(req: InstallRequest): PlanResult {
  return { outcome: "planned", plan: planFor(req) };
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

/** What the finished tree's own release marker says. Mutable, because the
 *  five verdicts are five sentences and each of them is a case. */
let statedRelease: StatedRelease = { verdict: "confirmed", stated: "39.29" };

function treeResult(): OsInstallResult {
  return {
    job_id: 11,
    destination: DEST,
    outcome: TREE_OUTCOME,
    stated_release: statedRelease,
  };
}

const PACKAGE_OUTCOME: ApplyOutcome = { ...TREE_OUTCOME, files: 166, bytes: 2_100_000 };

const WRITTEN: FirstBootWritten = {
  files: ["S:ART-FirstBoot", "S:ART-FirstBoot-Report"],
  userStartupBackup: null,
  userStartupCreated: true,
  removed: [],
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
/** The one live `onCardOsPhase` handler, likewise. */
let cardPhaseSaid: ((event: import("@/lib/cardOs").CardOsPhaseEvent) => void) | null = null;

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
    event === OSINSTALL_EVENT
      ? treeResult()
      : event === CARD_OS_PREPARE_EVENT
        ? prepareAnswer
        : event === CARD_OS_BUILD_EVENT
          ? cardAnswer
          : { job_id: 12, outcome: PACKAGE_OUTCOME }
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
  statedRelease = { verdict: "confirmed", stated: "39.29" };
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
    reboot: { kind: "available" },
    wizard: null,
    inputSetByArt: false,
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
  cardAnswer = cardResult({ ending: "succeeded" });
  prepareAnswer = { jobId: 12, session: CARD_SESSION, prepared: PREPARED, refusal: null };
  cardOpenMock.mockReset().mockImplementation(async () => {
    order.push("cardOpen");
    return { session: CARD_SESSION, tree: CARD_TREE };
  });
  cardCloseMock.mockReset().mockImplementation(async () => {
    order.push("cardClose");
    return { scratchLeft: null };
  });
  cardPrepareMock.mockReset().mockImplementation(async () => {
    order.push("cardPrepare");
    return 12;
  });
  cardBuildMock.mockReset().mockImplementation(async () => {
    order.push("cardBuild");
    return 13;
  });
  cardPhaseSaid = null;
  onCardPhaseMock
    .mockReset()
    .mockImplementation(async (handler: (e: import("@/lib/cardOs").CardOsPhaseEvent) => void) => {
      cardPhaseSaid = handler;
      return () => {};
    });
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

  // **ART-303, the owner's ruling of 2026-09-10.** This test used to end by
  // pinning the run's own line to BoingBag 3.9-2 alone — the rest run without
  // the unresolved row. On the owner's material that run could not succeed:
  // BoingBag 3.9-2 requires 3.9-1, and the core refused it. A ticked update
  // ART cannot resolve now withholds Build, by name, until a file is chosen.
  it("names a ticked update whose file ART would not trust, and does not run it", async () => {
    slotsMock.mockResolvedValue(
      slotsOf({ "package:boingbag-39-1": null, "package:boingbag-39-2": BB2 })
    );
    bothTicked();
    renderTab();

    const named = await screen.findByTestId("build-unresolved");
    // **The whole sentence** (round 4 whole-branch review, C1). It used to
    // say *"choose its file on the Amiga files tab"* about a tab that chose
    // nothing; it now names the control that answers — one per candidate on
    // the readout's own row — and says what happens until they do.
    expect(named.textContent).toBe(
      i18n.t("osBuilder.build.unresolved", { name: "BoingBag 3.9-1" })
    );
    expect(named.textContent).toContain("BoingBag 3.9-1");
    // …and nothing runs until a file is chosen: the button's place says why.
    await waitFor(() =>
      expect(screen.getByTestId("build-blocker").textContent).toBe(
        i18n.t("osBuilder.build.blocked.unresolved", { names: "BoingBag 3.9-1" })
      )
    );
    expect(screen.queryByTestId("build-run")).toBeNull();
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

  // **Moved from `OsInstall.test.tsx` by name** in round 4 task 5's fix round
  // — the twenty-fifth case, which that task's own fate list missed. Final
  // whole-branch review, Finding F: this is the one refusal that tells the
  // user to go and tick the named component *themselves*, and a raw recipe id
  // ("workbench-39") is not a checkbox a person can find on screen. Every
  // other refusal names a component purely for identification; `refusalPhrase`
  // is pure `src/lib` and has no catalogue to resolve one with, so the two
  // places that draw a refusal resolve it through `label()` before rendering.
  //
  // **Both of those places, in one case**, because there are two of them now
  // and they cannot be on screen together: the refusals card stands *instead
  // of* the Build button, and a phase row only exists once a run the plan
  // allowed has started. So the case renders twice, with a `cleanup()`
  // between — the arm that was `OsInstall.test.tsx`'s, and the arm task 4's
  // copy added.
  it("names the component to tick by its own label, never the raw recipe id", async () => {
    const refusal: RefusalReason = {
      refusal: "resident-table-unreadable",
      component: "workbench-39",
      resident: "exec",
    };
    // Computed the way the screen computes it: the raw id resolved through
    // the loaded catalogue to `COMPONENTS_39`'s own `labelKey`, which is a
    // real sentence in both catalogues ("Workbench 3.9 overlay").
    const phrase = refusalPhrase(refusal);
    const expected = i18n.t(phrase.key, {
      ...phrase.params,
      component: i18n.t("osinstall.components.name.os39.overlay"),
    });

    // Arm 1 — the plan's own refusal, in the Build button's place.
    refusals = [refusal];
    seed(FIELDS);
    renderTab();

    const card = await screen.findByTestId("build-refusals");
    expect(within(card).getByText(expected)).toBeTruthy();
    expect(card.textContent).not.toContain("workbench-39");

    // Arm 2 — the same refusal earned by a phase of a run, listed under that
    // phase's own advice. The plan has to allow the run for a phase to exist
    // at all, so the refusal comes back from `osinstall_add_package` instead.
    cleanup();
    refusals = [];
    addPackageMock.mockImplementation(async () => {
      order.push("addPackage");
      return { outcome: "refused", refusals: [refusal] };
    });
    bothTicked();
    renderTab();
    await screen.findByTestId("build-run");
    await userEvent.click(screen.getByTestId("build-confirm"));
    await userEvent.click(screen.getByTestId("build-run"));

    const listed = await screen.findByTestId("build-phase-refusal");
    expect(listed.textContent).toBe(expected);
    expect(listed.textContent).not.toContain("workbench-39");
  });

  it("will not run until the person says they have read the summary", async () => {
    bothTicked();
    renderTab();

    const button = (await screen.findByTestId("build-run")) as HTMLButtonElement;
    expect(button.disabled).toBe(true);
    await userEvent.click(screen.getByTestId("build-confirm"));
    await waitFor(() => expect((screen.getByTestId("build-run") as HTMLButtonElement).disabled).toBe(false));
  });

  // Round 4 task 5: the plan-error badge moved here from the install screen,
  // because the summary above is the plan now. Without it a plan that had
  // been asked for and had **failed** showed only the blocker's "Preview it
  // first" — the right shape of sentence for the wrong reason, and Rust's own
  // words nowhere on screen.
  it("says what the planner refused to answer, not 'preview it first'", async () => {
    planMock.mockReset().mockRejectedValue(new Error("could not open E:\\amiga\\os39\\Disk1.adf"));
    seed(FIELDS);
    renderTab();

    const badge = await screen.findByTestId("build-plan-error");
    expect(badge.textContent).toContain("could not open E:\\amiga\\os39\\Disk1.adf");
    // The blocker still stands — the button must not be offered over a plan
    // that does not exist — but it is no longer the only thing said.
    expect(screen.queryByTestId("build-run")).toBeNull();
  });

  it("draws no plan-error badge when the plan answers", async () => {
    // The control, measured rather than assumed: a badge that is always there
    // proves nothing about the failure above.
    bothTicked();
    renderTab();

    await screen.findByTestId("build-run");
    expect(screen.queryByTestId("build-plan-error")).toBeNull();
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

  // **The owner's finding of 2026-09-10: the tab froze the PC.** In update
  // mode the run has no tree phase and reads no plan — `useBuildRun` needs
  // one only for `osinstall_apply`, and the first summary line comes from
  // the tree — yet the tab planned anyway, and on the owner's material one
  // plan read 3.5 GB of discs. Update mode asks for no plan at all.
  it("asks for no plan when the destination is already a tree", async () => {
    describeTreeMock.mockResolvedValue(IS_A_TREE);
    bothTicked();
    renderTab();

    await screen.findByTestId("build-run");
    expect(summaryLines()[0]).toBe(
      i18n.t("osBuilder.build.summary.treeExisting", { release: "AmigaOS 3.9", count: 4212 })
    );
    expect(planMock).not.toHaveBeenCalled();
  });

  // The control: a fresh tree still plans, once the destination has been
  // looked at — the plan is the tree phase's own input.
  it("still plans a fresh tree once the destination has been looked at", async () => {
    bothTicked();
    renderTab();

    await screen.findByTestId("build-run");
    await waitFor(() => expect(planMock).toHaveBeenCalled());
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
        title: { key: "components.jobBar.title.installRelease", params: { release: "AmigaOS 3.2", target: "Tree" } },
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

    // **And the phase is named once, not twice** (round 4 whole-branch
    // review, M1). A copy of *"Running: AmigaOS 3.9"* stood beside the Build
    // button as well as in the phase's own row — ART-202's "aynı uyarı tek
    // ekranda 2 tane", and the copy beside the button was a lookup over the
    // reports rather than the report's own ending, so the two could drift.
    expect(
      screen.getAllByText(i18n.t("osBuilder.build.phase.running", { name: "AmigaOS 3.9" }))
    ).toHaveLength(1);

    // …and a total that *is* known gets the percentage and the bar.
    act(() => {
      report!({
        id: 11,
        title: { key: "components.jobBar.title.installRelease", params: { release: "AmigaOS 3.2", target: "Tree" } },
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

    act(() => job.resolve(treeResult()));
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

// ---------------------------------------------------------------------------
// What the fourth line may claim, and when the tab may claim anything at all
// ---------------------------------------------------------------------------
//
// Round 4 task 4, fix round 1. Three separate ways this line was a confident
// wrong sentence: it counted tree-phase work in update mode, where no tree
// phase runs; it said *not previewed yet* about a preview that had already
// failed; and the whole first line picked one of "a fresh build" and "this
// tree will be updated" before ART had looked at the folder.

describe("the fourth summary line", () => {
  it("states only what the updates would replace when the tree already exists", async () => {
    describeTreeMock.mockResolvedValue(IS_A_TREE);
    destinationTakenMock.mockResolvedValue(true);
    // One collision per ticked update — the only number ART holds for an
    // update row (`osinstall_collisions` answers a list and nothing else).
    collisionsMock.mockResolvedValue([{ path: "C/Format", collision: { collision: "newer" } }]);
    bothTicked();
    renderTab();

    await waitFor(() =>
      expect(summaryLines()[3]).toBe(
        i18n.t("osBuilder.build.summary.replacesUpdates", { replaced: 2 })
      )
    );
    // …and the component preview is not merely unshown, it is never asked
    // for: it partitions what the release's own parts would place, and in
    // update mode they are not placed at all.
    expect(componentCollisionsMock).not.toHaveBeenCalled();
  });

  it("says the preview failed, and does not promise one that is not coming", async () => {
    componentCollisionsMock.mockRejectedValue(new Error("the disc changed since the scan"));
    bothTicked();
    renderTab();

    await waitFor(() =>
      expect(summaryLines()[3]).toContain("the disc changed since the scan")
    );
    expect(summaryLines()[3]).not.toBe(i18n.t("osBuilder.build.summary.replacesPending"));
  });
});

describe("before ART has looked at the destination", () => {
  it("says it is checking rather than calling the folder fresh, and offers no button", async () => {
    // The window held open rather than raced — CLAUDE.md: anything
    // timing-dependent gets an invariant, not a wait.
    describeTreeMock.mockImplementation(() => new Promise(() => {}));
    bothTicked();
    renderTab();

    await waitFor(() =>
      expect(summaryLines()[0]).toBe(i18n.t("osBuilder.build.summary.checking"))
    );
    // Which sequence this build runs is exactly what has not been decided
    // yet, so there is nothing to press.
    expect(screen.queryByTestId("build-run")).toBeNull();
    expect(screen.getByTestId("build-blocker").textContent).toBe(
      i18n.t("osBuilder.build.summary.checking")
    );
  });
});

// **The owner's finding of 2026-09-10.** After a run the tab asks again which
// updates are ticked, and until the answer lands the list is empty. The
// summary read that as *"no update ticked"* and *"replaces 0 files"* under a
// report of the run that had just refused one, and the Build button stood
// over a sequence built from the empty list: the owner's second press wrote
// the first-boot files alone (`operations.jsonl`, 21:22:29, 21 s after the
// refusal). Held open rather than raced.
describe("while ART is still finding out which updates are ticked", () => {
  it("says it is looking rather than 'none ticked', and offers no button", async () => {
    slotsMock.mockImplementation(() => new Promise(() => {}));
    describeTreeMock.mockResolvedValue(IS_A_TREE);
    bothTicked();
    renderTab();

    await waitFor(() =>
      expect(summaryLines()[1]).toBe(i18n.t("osBuilder.build.summary.updatesChecking"))
    );
    expect(summaryLines()[3]).toBe(i18n.t("osBuilder.build.summary.replacesPending"));
    expect(screen.queryByTestId("build-run")).toBeNull();
    expect(screen.getByTestId("build-blocker").textContent).toBe(
      i18n.t("osBuilder.build.summary.updatesChecking")
    );
  });
});

describe("stopping a run, and what a finished run changes", () => {
  it("asks the running job to stop", async () => {
    const job = pending();
    seed({ ...FIELDS, "buildSession.firstboot": { written: false, wanted: false } });
    renderTab();
    await screen.findByTestId("build-run");
    await userEvent.click(screen.getByTestId("build-confirm"));
    await userEvent.click(screen.getByTestId("build-run"));

    const stop = await screen.findByTestId("build-stop");
    expect(jobCancelMock).not.toHaveBeenCalled();
    await userEvent.click(stop);
    // The job the run is actually waiting on — asserted by id, because
    // "cancel was called" is true of cancelling the wrong job too.
    expect(jobCancelMock).toHaveBeenCalledWith(11);

    act(() => job.reject(new Error("cancelled")));
    await flush();
  });

  it("asks about the destination again once a run has finished (I2)", async () => {
    seed({ ...FIELDS, "buildSession.firstboot": { written: false, wanted: false } });
    renderTab();
    await screen.findByTestId("build-run");
    await waitFor(() => expect(destinationTakenMock).toHaveBeenCalled());
    const before = destinationTakenMock.mock.calls.length;

    await userEvent.click(screen.getByTestId("build-confirm"));
    await userEvent.click(screen.getByTestId("build-run"));
    await screen.findByTestId("build-handoff");

    // The folder is not what it was: the run just filled it. A gate still
    // offering a fresh tree into it would be offering a run `apply` refuses.
    await waitFor(() =>
      expect(destinationTakenMock.mock.calls.length).toBeGreaterThan(before)
    );
    expect(describeTreeMock.mock.calls.length).toBeGreaterThan(1);
  });

  /**
   * **The button stops offering the run that already happened** (round 5,
   * task 4). It read *Build* both before and after a successful run, over a
   * report saying the tree is written and a hand-off offering the card lane —
   * a second press is a real thing to want (a tick changed, an archive
   * replaced), but *Build* on a screen that has just built says nothing about
   * which of the two it is. *Build again* does.
   *
   * The state it reads is `run.succeeded`, the same flag the hand-off badge
   * reads, so the two cannot disagree about whether a run finished.
   */
  it("offers Build again once a run has succeeded, not Build", async () => {
    seed({ ...FIELDS, "buildSession.firstboot": { written: false, wanted: false } });
    renderTab();
    await screen.findByTestId("build-run");
    // The control, measured rather than assumed: before the run it is Build.
    expect(screen.getByTestId("build-run").textContent).toBe(i18n.t("osBuilder.build.run"));

    await userEvent.click(screen.getByTestId("build-confirm"));
    await userEvent.click(screen.getByTestId("build-run"));
    await screen.findByTestId("build-handoff");

    await waitFor(() =>
      expect(screen.getByTestId("build-run").textContent).toBe(
        i18n.t("osBuilder.build.runAgain")
      )
    );
    expect(screen.getByTestId("build-run").textContent).not.toBe(i18n.t("osBuilder.build.run"));
  });
});

/**
 * **What the first-boot phase did to `S/User-Startup`** (round 5, task 4,
 * carried from task 1's review). Two of the ten cases dropped with
 * `FirstBootPanel` were its post-write sentences about that file; the write
 * is this phase now, and `phaseDetailPhrase` (`buildRun.ts`) is the mapper.
 * These two cases are that pair, arriving by their meaning rather than by
 * their old names — the assertion is that the row on **this** screen says it.
 */
describe("the first-boot phase says what happened to S/User-Startup", () => {
  /** Run to a finish with first boot on, and hand back its phase row. */
  async function firstBootRow(written: FirstBootWritten): Promise<HTMLElement> {
    firstbootWriteMock.mockReset().mockImplementation(async () => {
      order.push("firstboot");
      return written;
    });
    bothTicked();
    renderTab();
    await screen.findByTestId("build-run");
    await userEvent.click(screen.getByTestId("build-confirm"));
    await userEvent.click(screen.getByTestId("build-run"));
    await waitFor(() => expect(screen.getAllByTestId("build-phase-row")).toHaveLength(4));
    const rows = screen.getAllByTestId("build-phase-row");
    await waitFor(() =>
      expect(rows[3].textContent).toContain(
        i18n.t("osBuilder.build.phase.firstboot.succeeded", { files: written.files.length })
      )
    );
    return rows[3];
  }

  it("names the file the previous S/User-Startup was backed up to", async () => {
    const BACKUP = "E:\\amiga\\Amigatolon\\sonuclar\\S\\User-Startup.art-bak";
    const row = await firstBootRow({
      files: ["S:ART-FirstBoot"],
      userStartupBackup: BACKUP,
      userStartupCreated: false,
      removed: [],
    });
    // The path itself, not just "it was backed up": a user told their file
    // was replaced and not told where the old one went has been given
    // nothing (CLAUDE.md, "never claim what you did not do").
    expect(row.textContent).toContain(i18n.t("osBuilder.build.phase.firstboot.backup", { path: BACKUP }));
    expect(row.textContent).toContain(BACKUP);
    expect(row.textContent).not.toContain(i18n.t("osBuilder.build.phase.firstboot.created"));
  });

  it("says ART created S/User-Startup when there was none to back up", async () => {
    const row = await firstBootRow({
      files: ["S:ART-FirstBoot"],
      userStartupBackup: null,
      userStartupCreated: true,
      removed: [],
    });
    expect(row.textContent).toContain(i18n.t("osBuilder.build.phase.firstboot.created"));
  });

  it("adds neither sentence when the write touched no existing file and made none", async () => {
    // The control: a row that always carried one of the two would pass both
    // cases above and be wrong here.
    const row = await firstBootRow({
      files: ["S:ART-FirstBoot"],
      userStartupBackup: null,
      userStartupCreated: false,
      removed: [],
    });
    expect(row.textContent).not.toContain(i18n.t("osBuilder.build.phase.firstboot.created"));
    expect(within(row).queryByTestId("build-phase-detail")).toBeNull();
  });
});

// ---------------------------------------------------------------------------
// The tree's own release marker — five verdicts, five sentences (Task 9)
// ---------------------------------------------------------------------------
//
// **Four of these are moved from `OsInstall.test.tsx` by name** (round 4 task
// 5 deleted them with the result card; task 4's fix round 1 brings them here,
// where the JSX went). CLAUDE.md's answer to the round that shipped AmigaOS
// 3.5 labelled 3.9: ask the artefact, never assert. The fifth,
// `reports a confirmed marker by its own text`, is already covered by
// `carries the tree phase's own release verdict and the time it took`.
//
// What changed is only where the verdict arrives from: it is the tree
// phase's `succeeded` ending now, not a result card fed by an event.

describe("the tree phase's release marker", () => {
  /** A tree-only run, to its report row. */
  async function treeRow(): Promise<HTMLElement> {
    seed({ ...FIELDS, "buildSession.firstboot": { written: false, wanted: false } });
    renderTab();
    await screen.findByTestId("build-run");
    await userEvent.click(screen.getByTestId("build-confirm"));
    await userEvent.click(screen.getByTestId("build-run"));
    const rows = await screen.findAllByTestId("build-phase-row");
    return rows[0];
  }

  // **The mutation table's third row.** Naming both sides is the point of
  // this sentence — the frontend key this pins is exactly what collapsing
  // the three sentences into one would break.
  it("names both sides of a mismatch", async () => {
    statedRelease = { verdict: "mismatch", expected: "Release 3.2.2", stated: "Release 3.2" };
    const row_ = await treeRow();

    await waitFor(() =>
      expect(within(row_).getByTestId("build-stated-release").textContent).toBe(
        i18n.t("osinstall.result.statedRelease.mismatch", {
          expected: "Release 3.2.2",
          stated: "Release 3.2",
        })
      )
    );
    expect(screen.queryByText(/\{\{/)).toBeNull();
  });

  it("says a tree with no marker states none, not a guess", async () => {
    statedRelease = { verdict: "unstated" };
    const row_ = await treeRow();
    await waitFor(() =>
      expect(within(row_).getByTestId("build-stated-release").textContent).toBe(
        i18n.t("osinstall.result.statedRelease.unstated")
      )
    );
  });

  // **Fix round 1, Finding 1** (of the round that wrote it). An unreadable
  // marker must never render the same sentence as "states none" — the two
  // are different facts with different next steps, and folding them is the
  // exact defect the case exists to catch.
  it("says the marker could not be read, never the same sentence as unstated", async () => {
    statedRelease = {
      verdict: "unreadable",
      detail: "malformed release marker: too many bytes",
    };
    const row_ = await treeRow();
    await waitFor(() =>
      expect(within(row_).getByTestId("build-stated-release").textContent).toBe(
        i18n.t("osinstall.result.statedRelease.unreadable", {
          detail: "malformed release marker: too many bytes",
        })
      )
    );
    expect(screen.queryByText(i18n.t("osinstall.result.statedRelease.unstated"))).toBeNull();
  });

  // **Final whole-branch review, Finding E.** A differing marker for a
  // release ART has never measured must render its own sentence, and never
  // the "mismatch" wording — that would tell the user their correct tree is
  // wrong for a formula nobody has checked.
  it("reports a differing marker for an unmeasured release plainly, never as a mismatch", async () => {
    statedRelease = { verdict: "expected-unknown", stated: "Release 3.5" };
    const row_ = await treeRow();
    await waitFor(() =>
      expect(within(row_).getByTestId("build-stated-release").textContent).toBe(
        i18n.t("osinstall.result.statedRelease.expectedUnknown", { stated: "Release 3.5" })
      )
    );
    expect(
      screen.queryByText(
        i18n.t("osinstall.result.statedRelease.mismatch", {
          expected: "AmigaOS 3.9",
          stated: "Release 3.5",
        })
      )
    ).toBeNull();
  });
});

// ---------------------------------------------------------------------------
// What the folder actually holds — the evidence line above the refusals
// ---------------------------------------------------------------------------
//
// **Six cases moved from `OsInstall.test.tsx` by name** (round 4 task 5
// deleted them with the refusals card; task 4's fix round 1 brings them here,
// where `useMediaEvidence` and the card went). ART-208's own subject: sixteen
// true "this disk is missing" sentences add up to a false impression, and the
// line above them is the context that stops it.
//
// The release these run on is AmigaOS 3.2 — the release before there was a
// picker, so its remembered keys are the unsuffixed ones — because that is
// the release the originals measured and the volume names in their
// assertions are its own.

const MEDIA_32 = "E:\\media";

/** The refusal list a partly-filled folder produces. */
const MISSING_EXTRAS: RefusalReason[] = [
  { refusal: "media-missing", component: "extras", volume_name: "Extras3.2" },
];

/** Everything tab 4 reads, for AmigaOS 3.2 (bare keys — see
 *  `rememberedComponentKey`). */
const FIELDS_32: Record<string, unknown> = {
  "buildSession.kind": "install",
  "buildSession.release": "AmigaOS 3.2",
  "buildSession.rom": { path: "E:\\roms\\kick.rom" },
  "buildSession.material.AmigaOS 3.2": { folders: [{ path: MEDIA_32, layer: null }] },
  "osinstall.destination": DEST,
};

const LAYERS_322: InstallLayer[] = [
  { id: "base", labelKey: "osinstall.layer.base32" },
  { id: "update-3.2.2", labelKey: "osinstall.layer.update322" },
];

/** The evidence line, waited for and read out of the refusals card. */
async function evidenceLine(): Promise<HTMLElement> {
  const card = await screen.findByTestId("build-refusals");
  return card;
}

describe("what the folder holds, above the refusals", () => {
  async function renderPartialMedia(releaseHolding: string | null) {
    scanMediaMock.mockReset().mockResolvedValue({
      outcome: "found",
      media: [{ path: `${MEDIA_32}\\Disk1.adf`, volumeName: "Workbench3.2", kind: "floppy" }],
    } satisfies MediaScanResult);
    releaseForMediaMock.mockReset().mockResolvedValue(releaseHolding);
    mediaEvidenceMock.mockReset().mockResolvedValue({
      release: "AmigaOS 3.2",
      distinguishing: ["Workbench3.2"],
      shared: [],
      missingRequired: ["Extras3.2"],
    });
    refusals = MISSING_EXTRAS;
    seed(FIELDS_32);
    renderTab();
    await waitFor(() => expect(planMock).toHaveBeenCalled());
  }

  it("shows what the folder holds above the refusals when some disks are missing", async () => {
    await renderPartialMedia("AmigaOS 3.2");

    // **The sentence, named** — not `mediaEvidence(…)`'s own answer rendered
    // back at it. Building the expectation by calling the helper made both
    // sides move together: the key mapping could change to any other state's
    // key and this stayed green.
    const card = await evidenceLine();
    await waitFor(() =>
      expect(card.textContent).toContain(
        i18n.t("osinstall.evidence.sameRelease", {
          found: "Workbench3.2",
          missing: "Extras3.2",
        })
      )
    );

    // Context, not a replacement: the per-disk refusal this line sits above
    // must still be there.
    expect(card.textContent).toContain(
      i18n.t("osinstall.refusal.mediaMissing", { component: "extras", volume: "Extras3.2" })
    );
  });

  // **The fixture the original ran on was one the core cannot emit**, and
  // ART-257 is what exposed it. The real shape of "this is somebody else's
  // folder, but not *entirely*": AmigaOS 3.9's disc beside a plain `Fonts`,
  // while building 3.2. `Fonts` carries no version and every release asks for
  // it, so 3.2's own evidence is `shared: ["Fonts"]` with **nothing**
  // distinguishing — which is what keeps `wrongMediaFolder` quiet and makes
  // this the sentence to say.
  it("names the other release when the folder is a different one", async () => {
    const OTHER_RELEASE_REFUSALS: RefusalReason[] = [
      { refusal: "media-missing", component: "workbench-base", volume_name: "Workbench3.2" },
      { refusal: "media-missing", component: "install-libs", volume_name: "Install3.2" },
      { refusal: "media-missing", component: "extras", volume_name: "Extras3.2" },
    ];
    scanMediaMock.mockReset().mockResolvedValue({
      outcome: "found",
      media: [
        { path: `${MEDIA_32}\\AmigaOS3.9.iso`, volumeName: "AmigaOS3.9", kind: "disc" },
        { path: `${MEDIA_32}\\Fonts.adf`, volumeName: "Fonts", kind: "floppy" },
      ],
    } satisfies MediaScanResult);
    releaseForMediaMock.mockReset().mockResolvedValue("AmigaOS 3.9");
    mediaEvidenceMock.mockReset().mockResolvedValue({
      release: "AmigaOS 3.2",
      distinguishing: [],
      shared: ["Fonts"],
      missingRequired: ["Workbench3.2", "Install3.2"],
    });
    refusals = OTHER_RELEASE_REFUSALS;
    seed(FIELDS_32);
    renderTab();

    const card = await evidenceLine();
    await waitFor(() =>
      expect(card.textContent).toContain(
        i18n.t("osinstall.evidence.otherRelease", {
          found: "AmigaOS3.9, Fonts",
          release: "AmigaOS 3.9",
          missing: "Workbench3.2, Install3.2, Extras3.2",
        })
      )
    );
    // Still just context — the refusal itself is still named below it.
    expect(card.textContent).toContain(
      i18n.t("osinstall.refusal.mediaMissing", {
        component: "workbench-base",
        volume: "Workbench3.2",
      })
    );
  });

  // ART-257. **The release with two media folders is the one most likely to
  // arrive part-complete, and it was the one this whole feature never
  // reached.** Both halves are asserted, because the union alone would have
  // made the line *speak falsely*: `identify` answers `"AmigaOS 3.2"` for the
  // base set (correctly — a based release must not be named off its base's
  // disks alone), and the old branching would have read that as somebody
  // else's media.
  it("says what a layered release's own folders hold, and does not call the base set somebody else's", async () => {
    const BASE_FOLDER = "E:\\base322";
    const BASE_SET = ["Workbench3.2", "Install3.2", "Extras3.2", "Fonts", "Locale"];
    layersForMock.mockImplementation((release: string) =>
      Promise.resolve(release === "AmigaOS 3.2.2" ? LAYERS_322 : [])
    );
    scanMediaMock.mockReset().mockImplementation((folder: string) =>
      Promise.resolve(
        folder === BASE_FOLDER
          ? ({
              outcome: "found",
              media: BASE_SET.map((volumeName) => ({
                path: `${BASE_FOLDER}\\${volumeName}.adf`,
                volumeName,
                kind: "floppy" as const,
              })),
            } satisfies MediaScanResult)
          : ({ outcome: "found", media: [] } satisfies MediaScanResult)
      )
    );
    refusals = [
      { refusal: "media-missing", component: "update-322-system", volume_name: "Update3.2.2" },
      { refusal: "media-missing", component: "update-322-classes", volume_name: "Classes3.2.2" },
    ];
    // Both answers measured off the shipped recipes rather than chosen —
    // `identify.rs::a_based_releases_own_evidence_claims_the_base_set`.
    releaseForMediaMock.mockReset().mockResolvedValue("AmigaOS 3.2");
    mediaEvidenceMock.mockReset().mockResolvedValue({
      release: "AmigaOS 3.2.2",
      distinguishing: ["Workbench3.2", "Install3.2", "Extras3.2"],
      shared: ["Locale", "Fonts"],
      missingRequired: ["Update3.2.2", "Classes3.2.2"],
    });
    seed({
      "buildSession.kind": "install",
      "buildSession.release": "AmigaOS 3.2.2",
      "buildSession.rom": { path: "E:\\roms\\kick.rom" },
      "buildSession.material.AmigaOS 3.2.2": {
        folders: [{ path: BASE_FOLDER, layer: "base" }],
      },
      "osinstall.destination.AmigaOS 3.2.2": DEST,
    });
    renderTab();

    // The whole sentence, both parameters: the base disks the layer folder
    // really holds, and the update disks that are really absent.
    const card = await evidenceLine();
    await waitFor(() =>
      expect(card.textContent).toContain(
        i18n.t("osinstall.evidence.sameRelease", {
          found: BASE_SET.join(", "),
          missing: "Update3.2.2, Classes3.2.2",
        })
      )
    );

    // The false sentence the union alone would have produced is not on
    // screen — this release's own base media is not called "AmigaOS 3.2
    // media".
    expect(card.textContent).not.toContain(
      i18n.t("osinstall.evidence.otherRelease", {
        found: BASE_SET.join(", "),
        release: "AmigaOS 3.2",
        missing: "Update3.2.2, Classes3.2.2",
      })
    );

    // And Rust was asked about the layer folder's own disks — a screen
    // cannot be right downstream of a lookup that was asked about nothing.
    await waitFor(() =>
      expect(mediaEvidenceMock).toHaveBeenCalledWith("AmigaOS 3.2.2", BASE_SET)
    );
  });

  // ART-257's other half, and a survivor of a mutation round rather than
  // something the reading found: `layers` is `[]` both when a release is
  // unlayered *and* before `layersFor` has answered (ART-256 named that trap
  // one layer down). On the render straight after a switch to a layered
  // release, `layers` is still the previous release's answer — so a
  // `foundVolumeNames` that did not ask whether `layers` is *this* release's
  // answer would hand the new release's evidence lookup the **old** folder's
  // disks, and the sentence would be about a folder this build never reads.
  //
  // An invariant, not a wait: the question is one Rust must never be asked,
  // so no timing decides it. The release changes on tab 1; this tab reads it
  // out of the session, which is what the store write below is.
  it("never asks about the previous release's folder while a layered release is loading", async () => {
    layersForMock.mockImplementation((release: string) =>
      Promise.resolve(release === "AmigaOS 3.2.2" ? LAYERS_322 : [])
    );
    scanMediaMock.mockReset().mockImplementation((folder: string) =>
      Promise.resolve(
        folder === MEDIA_32
          ? ({
              outcome: "found",
              media: [
                { path: `${MEDIA_32}\\Disk1.adf`, volumeName: "Workbench3.2", kind: "floppy" },
              ],
            } satisfies MediaScanResult)
          : ({ outcome: "found", media: [] } satisfies MediaScanResult)
      )
    );
    releaseForMediaMock.mockReset().mockResolvedValue("AmigaOS 3.2");
    refusals = MISSING_EXTRAS;
    seed({
      ...FIELDS_32,
      "buildSession.material.AmigaOS 3.2.2": { folders: [{ path: "E:\\base322", layer: "base" }] },
      "osinstall.destination.AmigaOS 3.2.2": DEST,
    });
    renderTab();

    // Proven, not assumed: the flat folder really is being asked about for
    // AmigaOS 3.2 before the switch, so the absence below is about the
    // switch and not about a screen that never asked anything.
    await waitFor(() =>
      expect(mediaEvidenceMock).toHaveBeenCalledWith("AmigaOS 3.2", ["Workbench3.2"])
    );

    act(() => {
      const state = useSettingsStore.getState();
      useSettingsStore.setState({
        loaded: true,
        settings: {
          ...state.settings,
          remembered: {
            ...(state.settings.remembered as Record<string, unknown>),
            "buildSession.release": "AmigaOS 3.2.2",
          },
        },
      });
    });

    // AmigaOS 3.2.2 reads its layer folders, and its own is empty. The one
    // question that must never be asked is the new release against the old
    // release's flat folder.
    await waitFor(() => expect(layersForMock).toHaveBeenCalledWith("AmigaOS 3.2.2"));
    expect(mediaEvidenceMock).not.toHaveBeenCalledWith("AmigaOS 3.2.2", ["Workbench3.2"]);
  });

  // **ART-256's rule, and the one F9 caught nothing without.** A layered
  // release plans from its *tagged* folders alone: an untagged folder in the
  // list is `unusedForPlan`, the request never reads it, and a line counting
  // its disks would be the screen out-claiming the core about a folder this
  // build does not open. The mutation this exists for is measuring the
  // evidence over the whole material list instead — which every other case
  // here survives, because they each have one folder and the two answers
  // agree.
  it("says what the folders the request actually reads hold, and not what the others do", async () => {
    const BASE_FOLDER = "E:\\base322";
    const UNTAGGED = "E:\\somebody-elses";
    const BASE_SET = ["Workbench3.2", "Install3.2"];
    layersForMock.mockImplementation((release: string) =>
      Promise.resolve(release === "AmigaOS 3.2.2" ? LAYERS_322 : [])
    );
    scanMediaMock.mockReset().mockImplementation((folder: string) =>
      Promise.resolve(
        folder === BASE_FOLDER
          ? ({
              outcome: "found",
              media: BASE_SET.map((volumeName) => ({
                path: `${BASE_FOLDER}\\${volumeName}.adf`,
                volumeName,
                kind: "floppy" as const,
              })),
            } satisfies MediaScanResult)
          : ({
              outcome: "found",
              media: [
                { path: `${UNTAGGED}\\AmigaOS3.9.iso`, volumeName: "AmigaOS3.9", kind: "disc" },
              ],
            } satisfies MediaScanResult)
      )
    );
    refusals = [
      { refusal: "media-missing", component: "update-322-system", volume_name: "Update3.2.2" },
    ];
    releaseForMediaMock.mockReset().mockResolvedValue("AmigaOS 3.2.2");
    mediaEvidenceMock.mockReset().mockResolvedValue({
      release: "AmigaOS 3.2.2",
      distinguishing: BASE_SET,
      shared: [],
      missingRequired: ["Update3.2.2"],
    });
    seed({
      "buildSession.kind": "install",
      "buildSession.release": "AmigaOS 3.2.2",
      "buildSession.rom": { path: "E:\\roms\\kick.rom" },
      "buildSession.material.AmigaOS 3.2.2": {
        folders: [
          { path: BASE_FOLDER, layer: "base" },
          { path: UNTAGGED, layer: null },
        ],
      },
      "osinstall.destination.AmigaOS 3.2.2": DEST,
    });
    renderTab();

    const card = await evidenceLine();
    await waitFor(() =>
      expect(card.textContent).toContain(
        i18n.t("osinstall.evidence.sameRelease", {
          found: BASE_SET.join(", "),
          missing: "Update3.2.2",
        })
      )
    );
    // The disc in the untagged folder is not in the sentence, and Rust was
    // never asked about it either.
    expect(card.textContent).not.toContain("AmigaOS3.9");
    expect(mediaEvidenceMock.mock.calls).toEqual([["AmigaOS 3.2.2", BASE_SET]]);
    expect(releaseForMediaMock.mock.calls).toEqual([[BASE_SET]]);
  });

  // ART-178/ART-195's own shape, one folder set further on. The scans feed
  // the memo the two evidence lookups depend on, so an effect that writes a
  // **fresh** empty object on every run hands `found` a new identity for no
  // new information and both lookups are asked again for it.
  //
  // Counted, both arms, because "it feels the same" is not a result. Nothing
  // is on a clock here — the count is read once the sentence those lookups
  // produce is on screen.
  it("asks each media lookup once for a settled folder, not once per render", async () => {
    await renderPartialMedia("AmigaOS 3.2");
    const card = await evidenceLine();
    await waitFor(() =>
      expect(card.textContent).toContain(
        i18n.t("osinstall.evidence.sameRelease", {
          found: "Workbench3.2",
          missing: "Extras3.2",
        })
      )
    );

    expect(mediaEvidenceMock.mock.calls).toEqual([["AmigaOS 3.2", ["Workbench3.2"]]]);
    expect(releaseForMediaMock.mock.calls).toEqual([[["Workbench3.2"]]]);
  });
});

// ---------------------------------------------------------------------------
// ART-254 — a release switch must not leave the previous release's evidence
// checking the new release's plan
// ---------------------------------------------------------------------------
//
// **Moved from `OsInstall.test.tsx` by name.** The switch-release button is
// tab 4's now, and it is a first-class action rather than an edge case: the
// folder holds AmigaOS 3.2 disks, the build is on 3.9, and ART offers one
// click. `mediaFacts` is not cleared when its effect re-runs, and the plan is
// fetched by a different effect — so between the two there is a moment where
// 3.2's plan is checked against 3.9's evidence, and 3.9's evidence is
// *correctly* empty for a folder of 3.2 disks, which is exactly the shape
// ART-253's check reads as "none of these disks are ones this release asks
// for".
//
// The window is not raced: 3.2's evidence lookup simply never resolves, which
// is the same state the window is and is deterministic.

describe("switching release does not bring the wrong-folder sentence back (ART-254)", () => {
  it("says what the folder really holds while the previous release's evidence is still the only one held", async () => {
    const FOUND = "Workbench3.2, Fonts, Locale";
    scanMediaMock.mockReset().mockResolvedValue({
      outcome: "found",
      media: [
        { path: `${MEDIA_32}\\Disk1.adf`, volumeName: "Workbench3.2", kind: "floppy" },
        { path: `${MEDIA_32}\\Fonts.adf`, volumeName: "Fonts", kind: "floppy" },
        { path: `${MEDIA_32}\\Locale.adf`, volumeName: "Locale", kind: "floppy" },
      ],
    } satisfies MediaScanResult);
    releaseForMediaMock.mockReset().mockResolvedValue("AmigaOS 3.2");
    planMock.mockReset().mockImplementation((req: InstallRequest) =>
      Promise.resolve({
        outcome: "planned",
        plan: {
          ...planFor(req),
          refusals: [
            req.release === "AmigaOS 3.9"
              ? { refusal: "media-missing", component: "extras", volume_name: "AmigaOS3.9" }
              : { refusal: "media-missing", component: "extras", volume_name: "Extras3.2" },
          ] as RefusalReason[],
        },
      } satisfies PlanResult)
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
    seed({
      "buildSession.kind": "install",
      "buildSession.release": "AmigaOS 3.9",
      "buildSession.rom": { path: "E:\\roms\\kick.rom" },
      "buildSession.material.AmigaOS 3.9": { folders: [{ path: MEDIA_32, layer: null }] },
      "buildSession.material.AmigaOS 3.2": { folders: [{ path: MEDIA_32, layer: null }] },
      "osinstall.destination.AmigaOS 3.9": DEST,
      "osinstall.destination": DEST,
    });
    renderTab();

    // The setup is proven rather than assumed: on 3.9 this folder really is
    // the wrong one, the sentence really is true, and it really is on screen.
    await waitFor(() =>
      expect(screen.getByTestId("build-blocker").textContent).toBe(
        i18n.t("osinstall.blocked.wrongFolderIsRelease", {
          release: "AmigaOS 3.2",
          found: FOUND,
        })
      )
    );

    await userEvent.click(screen.getByTestId("build-switch-release"));

    // What the screen must say instead — the whole sentence, key and both
    // parameters. "Says nothing" would have been true here for at least three
    // other reasons (no plan yet, no refusals, an empty folder); this state
    // has one cause, and it is that the stale evidence withdrew.
    const card = await evidenceLine();
    await waitFor(() =>
      expect(card.textContent).toContain(
        i18n.t("osinstall.evidence.sameRelease", { found: FOUND, missing: "Extras3.2" })
      )
    );

    // And ART-253's own sentence, in both of its forms, is nowhere on screen.
    const tab = screen.getByTestId("build-tab");
    expect(tab.textContent).not.toContain(
      i18n.t("osinstall.blocked.wrongFolder", { found: FOUND })
    );
    expect(tab.textContent).not.toContain(
      i18n.t("osinstall.blocked.wrongFolderIsRelease", {
        release: "AmigaOS 3.2",
        found: FOUND,
      })
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

// ---------------------------------------------------------------------------
// What a finished run changes, and what a run in flight forbids
// ---------------------------------------------------------------------------

describe("after a run, the tab re-asks the questions a run answers (I1)", () => {
  it("asks the chain again once a run has finished", async () => {
    // **The defect.** `useChoiceInputs`' effects are keyed on `treeRoot`, the
    // folders, the release and the ROM. In update mode a finished run changes
    // none of them — same tree, same folders — so the chain was never
    // re-asked: the rows the run had just added stayed ticked-enabled, the
    // second summary line went on saying "2 updates", and Build was offered
    // again for work that was already done.
    describeTreeMock.mockResolvedValue(IS_A_TREE);
    destinationTakenMock.mockResolvedValue(true);
    bothTicked();
    renderTab();

    await screen.findByTestId("build-run");
    await waitFor(() => expect(chainMock).toHaveBeenCalled());
    const before = chainMock.mock.calls.length;

    await userEvent.click(screen.getByTestId("build-confirm"));
    await userEvent.click(screen.getByTestId("build-run"));
    await screen.findByTestId("build-handoff");

    await waitFor(() => expect(chainMock.mock.calls.length).toBeGreaterThan(before));
    // The slot report is asked with it, not left behind: a new chain paired
    // with the previous pass's slots is one answer about two moments.
    expect(slotsMock.mock.calls.length).toBeGreaterThan(1);
  });
});

describe("the lane's strip while a build runs (I2)", () => {
  /**
   * The tab inside a real run lock, with the lock's own state on screen.
   *
   * `mounted` is a control the *test* owns, so the tab can be taken away
   * while the lock is still on — which is the one thing a `setRunning(false)`
   * on unmount is for, and a plain `view.unmount()` cannot show, because it
   * takes the probe away with it.
   */
  function renderLocked() {
    function Harness() {
      const [running, setRunning] = useState(false);
      const [mounted, setMounted] = useState(true);
      const value = useMemo(() => ({ running, setRunning }), [running]);
      return (
        <RunLockContext.Provider value={value}>
          {mounted && <BuildTab />}
          <div data-testid="lock">{running ? "locked" : "free"}</div>
          <button data-testid="drop-tab" onClick={() => setMounted(false)}>
            leave
          </button>
        </RunLockContext.Provider>
      );
    }
    return render(
      <MemoryRouter initialEntries={["/os-builder/derle"]}>
        <Routes>
          <Route path="/os-builder/*" element={<Harness />} />
        </Routes>
      </MemoryRouter>
    );
  }

  it("holds the lock while the run is in flight and gives it back when it ends", async () => {
    // Leaving tab 4 unmounts the sequencer, so the rest of the phases and the
    // ART-197 hand-off simply stop. The lock is what makes the strip refuse
    // that navigation and say why; a lock that is never set is a strip that
    // never refuses, and a lock never cleared is a lane nobody can leave.
    const job = pending();
    seed({ ...FIELDS, "buildSession.firstboot": { written: false, wanted: false } });
    renderLocked();
    await screen.findByTestId("build-run");
    expect(screen.getByTestId("lock").textContent).toBe("free");

    await userEvent.click(screen.getByTestId("build-confirm"));
    await userEvent.click(screen.getByTestId("build-run"));
    await waitFor(() => expect(screen.getByTestId("lock").textContent).toBe("locked"));

    act(() => job.resolve(treeResult()));
    await waitFor(() => expect(screen.getByTestId("lock").textContent).toBe("free"));
  });

  it("gives the lock back if the tab goes away with the run still on", async () => {
    // The lane must not be lockable for ever. Nothing in the application
    // should reach this — the strip is what stops a person leaving — but a
    // lock whose only release is a run that no longer exists would be a
    // wizard nobody can navigate, with no control anywhere that lifts it.
    const job = pending();
    seed({ ...FIELDS, "buildSession.firstboot": { written: false, wanted: false } });
    renderLocked();
    await screen.findByTestId("build-run");
    await userEvent.click(screen.getByTestId("build-confirm"));
    await userEvent.click(screen.getByTestId("build-run"));
    await waitFor(() => expect(screen.getByTestId("lock").textContent).toBe("locked"));

    await userEvent.click(screen.getByTestId("drop-tab"));
    await waitFor(() => expect(screen.getByTestId("lock").textContent).toBe("free"));

    act(() => job.reject(new Error("gone")));
    await flush();
  });
});

describe("while the plan is still being computed (I5)", () => {
  it("says the plan is being computed, not 'preview it first'", async () => {
    // Tab 4 has no Preview control and never had one — it plans on its own —
    // so `osinstall.blocked.notPlanned` told the person to press something
    // that is not on the screen, for a state that clears itself in a second.
    planMock.mockReset().mockImplementation(() => new Promise<PlanResult>(() => {}));
    seed(FIELDS);
    renderTab();

    await waitFor(() =>
      expect(screen.getByTestId("build-blocker").textContent).toBe(
        i18n.t("osinstall.blocked.planning")
      )
    );
    // Both endings stay distinct: this is not the sentence a plan that has
    // been asked for and refused to answer gets.
    expect(screen.getByTestId("build-blocker").textContent).not.toBe(
      i18n.t("osinstall.blocked.notPlanned")
    );
    expect(screen.queryByTestId("build-run")).toBeNull();
  });

  it("still says 'preview it first' when there is no folder to plan from", async () => {
    // The control, measured rather than assumed. `notPlanned` is not dead:
    // with nothing to plan from, no plan is coming, and "still being
    // computed" would be a promise nothing is going to keep. (`noFolder`
    // outranks it, so this is the shape that reaches it: a folder in the
    // list that the request does not read.)
    const { osinstallBlocker } = await import("@/lib/osinstall");
    expect(
      osinstallBlocker({
        mediaFolder: MEDIA,
        destination: DEST,
        destinationTaken: false,
        plan: null,
        found: [],
        releaseHolding: null,
        mediaFacts: null,
      })?.key
    ).toBe("osinstall.blocked.notPlanned");
  });
});

// ---------------------------------------------------------------------------
// Card mode (round 4, task 10): the same tab, the card's own run
// ---------------------------------------------------------------------------

/** The Machine tab's card destination, its image and its partitions — the
 *  keys `useBuildSession` reads them through, per release (Q2). */
function cardMode(over: Record<string, unknown> = {}) {
  seed({
    ...FIELDS,
    "osinstall.destinationKind.AmigaOS 3.9": "card-image",
    // One ticked update, so the card's sequence really carries the OS
    // Builder's own three phases into the session tree.
    "buildSession.packages.AmigaOS 3.9": { folder: null, chosen: ["boingbag-39-1"] },
    "osinstall.cardTarget.AmigaOS 3.9": {
      sizeGb: 64,
      image: CARD_IMAGE,
      emu68Archive: "E:\\emu68\\Emu68-pistorm.zip",
      pfs3Driver: null,
      partitions: [
        { name: "System", sources: [] },
        { name: "Games", sources: [`${ARCHIVES}\\oyunlar`] },
        { name: "Work", sources: [] },
      ],
    },
    ...over,
  });
}

/** Press the card's Build button, which is the only thing that opens a
 *  session. */
async function pressCardBuild() {
  await screen.findByTestId("card-run");
  await userEvent.click(screen.getByTestId("build-confirm"));
  await userEvent.click(screen.getByTestId("card-run"));
}

/** Through the agreement: tick the one proposed Kickstart and continue. */
async function agreeAndContinue() {
  await screen.findByTestId("kickstart-agreement");
  await userEvent.click(screen.getByTestId("kickstart-check-kick40068.A1200"));
  await userEvent.click(screen.getByTestId("card-agree-continue"));
}

function cardRows(): string[] {
  return screen.getAllByTestId("card-phase-row").map((el) => el.textContent ?? "");
}

describe("card mode — the card's own run", () => {
  it("says a card is being built rather than a folder, in the first summary line", async () => {
    cardMode();
    renderTab();
    await screen.findByTestId("card-run");
    await waitFor(() => expect(summaryLines()[0]).toContain(CARD_IMAGE));
    expect(summaryLines()[0]).not.toBe(
      i18n.t("osBuilder.build.summary.tree", { files: 1980, bytes: "", components: "" })
    );
  });

  // Q9 / ART-344: a session opened by looking at a tab is a scratch folder
  // nobody asked for.
  it("opens no session until the button is pressed", async () => {
    cardMode();
    renderTab();
    await screen.findByTestId("card-run");
    await flush();
    expect(cardOpenMock).not.toHaveBeenCalled();
  });

  it("will not build without an image, and says where to choose one", async () => {
    cardMode({
      "osinstall.cardTarget.AmigaOS 3.9": {
        sizeGb: 64,
        image: null,
        emu68Archive: null,
        pfs3Driver: null,
        partitions: [{ name: "System", sources: [] }],
      },
    });
    renderTab();
    const blocker = await screen.findByTestId("build-blocker");
    expect(blocker.textContent).toBe(i18n.t("cardRun.blocked.noImage"));
    expect(screen.queryByTestId("card-run")).toBeNull();
  });

  // NEW (controller's ruling, Task 10's own concern): a card target the
  // build cannot possibly finish is refused in the Build button's place,
  // before a session is even opened — not discovered after the tree, the
  // updates and the prepare have already run.
  it("will not build without an Emu68 archive, and says where to choose one", async () => {
    cardMode({
      "osinstall.cardTarget.AmigaOS 3.9": {
        sizeGb: 64,
        image: CARD_IMAGE,
        emu68Archive: null,
        pfs3Driver: null,
        partitions: [{ name: "System", sources: [] }],
      },
    });
    renderTab();
    const blocker = await screen.findByTestId("build-blocker");
    expect(blocker.textContent).toBe(i18n.t("cardRun.blocked.noArchive"));
    expect(screen.queryByTestId("card-run")).toBeNull();
    expect(cardOpenMock).not.toHaveBeenCalled();
  });

  it("will not build without a card size, and says to choose one", async () => {
    cardMode({
      "osinstall.cardTarget.AmigaOS 3.9": {
        sizeGb: 0,
        image: CARD_IMAGE,
        emu68Archive: "E:\\emu68\\Emu68-pistorm.zip",
        pfs3Driver: null,
        partitions: [{ name: "System", sources: [] }],
      },
    });
    renderTab();
    const blocker = await screen.findByTestId("build-blocker");
    expect(blocker.textContent).toBe(i18n.t("cardRun.blocked.noSize"));
    expect(screen.queryByTestId("card-run")).toBeNull();
  });

  it("will not build a partition the user added but never put anything on, and names it", async () => {
    cardMode({
      "osinstall.cardTarget.AmigaOS 3.9": {
        sizeGb: 64,
        image: CARD_IMAGE,
        emu68Archive: "E:\\emu68\\Emu68-pistorm.zip",
        pfs3Driver: null,
        partitions: [
          { name: "System", sources: [] },
          { name: "Games", sources: [] },
          { name: "Work", sources: [] },
        ],
      },
    });
    renderTab();
    const blocker = await screen.findByTestId("build-blocker");
    expect(blocker.textContent).toBe(i18n.t("cardRun.blocked.emptyPartition", { partition: "Games" }));
    expect(screen.queryByTestId("card-run")).toBeNull();
  });

  it("builds a plain System-and-Work card with no extra partition, unblocked", async () => {
    cardMode({
      "osinstall.cardTarget.AmigaOS 3.9": {
        sizeGb: 64,
        image: CARD_IMAGE,
        emu68Archive: "E:\\emu68\\Emu68-pistorm.zip",
        pfs3Driver: null,
        partitions: [
          { name: "System", sources: [] },
          { name: "Work", sources: [] },
        ],
      },
    });
    renderTab();
    await screen.findByTestId("card-run");
    expect(screen.queryByTestId("build-blocker")).toBeNull();
  });

  it("runs the whole card, shows the manifest and the health checklist", async () => {
    cardMode();
    renderTab();
    await pressCardBuild();
    await agreeAndContinue();

    await waitFor(() => expect(screen.getByTestId("card-manifest")).toBeTruthy());
    expect(screen.getByTestId("card-manifest").textContent).toContain(
      `${CARD_IMAGE}.art-manifest.json`
    );
    // The lifted `HealthPanel`, drawn by the run rather than by a second copy.
    expect(screen.getByTestId("card-health")).toBeTruthy();
    expect(screen.getByTestId("card-health").textContent).toContain(
      i18n.t("cardBuilder.health.check.bootFirst")
    );
    expect(order).toEqual([
      "cardOpen",
      "apply",
      "addPackage",
      "firstboot",
      "cardPrepare",
      "cardBuild",
    ]);
    // Rust's own `end_build` removed the session; a close from here would be
    // a second answer to a settled question.
    expect(cardCloseMock).not.toHaveBeenCalled();
  });

  /**
   * **A card run leaves the folder lane's own remembered tree exactly as it
   * found it** (round 4 final review, C3).
   *
   * The run used to put `card_os_open`'s scratch path into `session.tree` —
   * a *persisted* key five screens read as the user's own distribution root —
   * and then set it to `null` in the `finally` that runs on every path,
   * success included, clearing `firstboot.written` with it. A user who had
   * built a folder distribution and later built a card found the Appearance
   * and Network panels, the step banner and `VerifyAgainstCard` all saying
   * there was no tree, having changed nothing themselves. The run's tree now
   * lives in a store of its own (`@/lib/cardRunTree`), which is not persisted
   * and which no folder screen reads.
   */
  it("leaves session.tree and firstboot.written untouched across a whole card run", async () => {
    cardMode({
      "buildSession.tree": { root: "E:\\Amiga\\dist39", builtHere: true },
      "buildSession.firstboot": { written: true, askPrefs: true },
    });
    renderTab();
    await pressCardBuild();
    await agreeAndContinue();
    await waitFor(() => expect(screen.getByTestId("card-manifest")).toBeTruthy());

    const bag = useSettingsStore.getState().settings.remembered as Record<string, unknown>;
    expect(bag["buildSession.tree"]).toEqual({ root: "E:\\Amiga\\dist39", builtHere: true });
    expect((bag["buildSession.firstboot"] as { written: boolean }).written).toBe(true);
  });

  // **The agreement is a real pause** (Q8, the owner's rule of 2026-08-21).
  it("does not build until the user has acted on the Kickstart agreement", async () => {
    cardMode();
    renderTab();
    await pressCardBuild();
    await screen.findByTestId("kickstart-agreement");
    await flush();
    expect(cardBuildMock).not.toHaveBeenCalled();

    await userEvent.click(screen.getByTestId("card-agree-continue"));
    await waitFor(() => expect(cardBuildMock).toHaveBeenCalled());
  });

  it("closes the session exactly once when the user gives the run up", async () => {
    cardMode();
    renderTab();
    await pressCardBuild();
    await screen.findByTestId("kickstart-agreement");
    await userEvent.click(screen.getByTestId("card-give-up"));
    await waitFor(() => expect(cardCloseMock).toHaveBeenCalled());
    expect(cardCloseMock.mock.calls).toEqual([[CARD_SESSION]]);
    expect(cardBuildMock).not.toHaveBeenCalled();
  });

  it("renders a refusal's own Turkish sentence and its next step, and never the failure's", async () => {
    cardAnswer = cardResult({
      ending: "refused",
      phase: "card",
      code: "ART-CARD-SOURCE-UNUSABLE",
      message: "that source cannot be used",
      params: { source: `${ARCHIVES}\\kirik.lha`, partition: "Games", reason: "missing" },
    });
    await changeLanguage("tr");
    cardMode();
    renderTab();
    await pressCardBuild();
    await agreeAndContinue();

    const refusal = await screen.findByTestId("card-refusal-sentence");
    expect(refusal.textContent).toBe(
      i18n.t("errors.cardSourceUnusable.missing", {
        id: "ART-CARD-SOURCE-UNUSABLE",
        source: `${ARCHIVES}\\kirik.lha`,
        partition: "Games",
      })
    );
    const next = screen.getAllByTestId("card-phase-next").at(-1);
    expect(next?.textContent).toBe(i18n.t("cardRun.next.refused"));
    expect(next?.textContent).not.toBe(i18n.t("cardRun.next.failed", { image: CARD_IMAGE }));
    // The build row says refused, not failed.
    expect(cardRows().at(-1)).toContain(
      i18n.t("cardRun.phase.build.refused", { code: "ART-CARD-SOURCE-UNUSABLE" })
    );
    // And so does the job bar's own row (round 4, Task 1).
    const bar = jobStatusLabel({
      id: 13,
      title: { key: "components.jobBar.title.buildCardOs", params: { target: CARD_IMAGE } },
      done: 0,
      total: null,
      message: "",
      state: { state: "refused", code: "ART-CARD-SOURCE-UNUSABLE", message: "x" },
    });
    expect(i18n.t(bar.key, bar.params)).toBe(i18n.t("components.jobBar.status.refused"));
    expect(i18n.t(bar.key, bar.params)).not.toBe(
      i18n.t("components.jobBar.status.failed", { code: "ART-CARD-SOURCE-UNUSABLE" })
    );
  });

  /**
   * **A refusal raised while preparing reaches the screen as a refusal**
   * (round 4 final review, C2).
   *
   * Every card refusal but the three the build itself raises comes out of
   * `card_os_prepare` — `CardSourceUnusable`, `CardDoesNotFit`,
   * `Pfs3DriverNotFound`, `CardNamesNeedHstImager`, `NotEnoughSpace`,
   * `CardPartitionTooManyEntries`, `WhdloadNotFound`. The command used plain
   * `spawn_job` and returned `Err`, so the job ended **Failed**, the frontend
   * got one formatted English string with the code in parentheses (which
   * `parseError` does not read), and the row said *"Preparing the card
   * failed"* in red with the failure's next step. Owner decision 4 (card
   * refusals are Turkish), decision 5 (a refusal is never called failed) and
   * "endings stay distinct" were all broken for the commonest refusal in the
   * flow. The existing case above proved the same sentence through the
   * *build*'s result event — a path this error cannot take.
   */
  it("says a prepare refusal in Turkish, on a refused row, and never as a failure", async () => {
    prepareAnswer = {
      jobId: 12,
      session: CARD_SESSION,
      prepared: null,
      refusal: {
        code: "ART-CARD-SOURCE-UNUSABLE",
        message: `'${ARCHIVES}\\oyunlar' in Games cannot be used: it does not exist.`,
        params: { partition: "Games", source: `${ARCHIVES}\\oyunlar`, reason: "missing" },
      },
    };
    await changeLanguage("tr");
    cardMode();
    renderTab();
    await pressCardBuild();

    const refusal = await screen.findByTestId("card-refusal-sentence");
    expect(refusal.textContent).toBe(
      i18n.t("errors.cardSourceUnusable.missing", {
        id: "ART-CARD-SOURCE-UNUSABLE",
        source: `${ARCHIVES}\\oyunlar`,
        partition: "Games",
      })
    );
    // The prepare row itself: refused, with its code — not failed.
    const prepareRow = cardRows().find((row) =>
      row.includes(i18n.t("cardRun.phase.prepare.refused", { code: "ART-CARD-SOURCE-UNUSABLE" }))
    );
    expect(prepareRow).toBeTruthy();
    expect(cardRows().join("\n")).not.toContain(
      i18n.t("cardRun.phase.prepare.failed", { code: "ART-CARD-SOURCE-UNUSABLE" })
    );
    // The refusal's next step, not the failure's "where the evidence is".
    const next = screen.getAllByTestId("card-phase-next").at(-1);
    expect(next?.textContent).toBe(i18n.t("cardRun.next.refused"));
    // Nothing was built, and the session was closed exactly once.
    expect(cardBuildMock).not.toHaveBeenCalled();
    await waitFor(() => expect(cardCloseMock.mock.calls).toEqual([[CARD_SESSION]]));
  });

  /**
   * **A fallback is news** (round 4 final review, minor). The manual builder
   * has said which tool wrote which partition, and why ART's own writer could
   * not, since ART-120; the one-button run dropped `result.steps` entirely, so
   * a card half-written by hst-imager looked exactly like one ART wrote
   * itself. Nothing was out-claimed — and the user was not told.
   */
  it("names the partition hst-imager wrote and why ART's own writer could not", async () => {
    cardAnswer = {
      ...cardResult({ ending: "succeeded" }),
      steps: [
        {
          step: {
            step: "format-partition",
            slot: null,
            index: 0,
            drive_name: "SDH0",
            volume_name: "System",
          },
          tool: "native",
          fallback_reason: null,
        },
        {
          step: {
            step: "format-partition",
            slot: null,
            index: 1,
            drive_name: "SDH1",
            volume_name: "Games",
          },
          tool: "hst-imager 1.2.3",
          fallback_reason: { reason: "non-ascii-pfs3-names", paths: ["türkçe"], more: 0 },
        },
      ] satisfies import("@/lib/preload").StepReport[],
    };
    cardMode();
    renderTab();
    await pressCardBuild();
    await agreeAndContinue();

    const said = await screen.findByTestId("card-fallbacks");
    // One line: the step that went the ordinary way is not news.
    expect(said.querySelectorAll("li").length).toBe(1);
    expect(said.textContent).toContain("hst-imager");
    expect(said.textContent).toContain("Games");
  });

  /**
   * **No Stop while the run waits at the agreement** (the review's triage of
   * Task 10's deferred minor): there is no job to cancel, so Stop did what
   * Give up does and then printed the give-up sentence.
   */
  it("offers only Give up at the agreement, never Stop", async () => {
    cardMode();
    renderTab();
    await pressCardBuild();
    await screen.findByTestId("kickstart-agreement");
    expect(screen.queryByTestId("card-stop")).toBeNull();
    expect(screen.getByTestId("card-give-up")).toBeTruthy();
  });

  it("says where the evidence is and what became of the .partial when a build fails", async () => {
    cardAnswer = cardResult({
      ending: "failed",
      phase: "partitions",
      code: "ART-IO",
      message: "the disk filled up",
      params: {},
    });
    cardMode();
    renderTab();
    await pressCardBuild();
    await agreeAndContinue();

    const next = await screen.findByTestId("card-phase-next");
    expect(next.textContent).toBe(i18n.t("cardRun.next.failed", { image: CARD_IMAGE }));
    expect(screen.getByTestId("card-partial").textContent).toBe(
      i18n.t("cardRun.partial.removed", { path: `${CARD_IMAGE}.partial` })
    );
  });

  // R2 (card round 3's residual re-review, "New breakage"): the ending says
  // neither name could be removed when the build finished; the report beside
  // it, once a later retry actually removed `.partial`, must say both facts
  // in order rather than flatly contradicting the sentence above it.
  it("says both, in order, when the .partial was reported unremovable and then removed on a later try", async () => {
    cardAnswer = cardResult({
      ending: "failed",
      phase: "check",
      code: "ART-CARD-FINISH-LEFT-BOTH-NAMES",
      message:
        "ART could not finish naming its card: neither the .partial name nor the new name could be removed.",
      params: { image: CARD_IMAGE, partial: `${CARD_IMAGE}.partial`, why: "held by a scanner" },
    });
    cardMode();
    renderTab();
    await pressCardBuild();
    await agreeAndContinue();

    await screen.findByTestId("card-phase-next");
    const partial = screen.getByTestId("card-partial");
    expect(partial.textContent).toBe(
      i18n.t("cardRun.partial.removedAfterBothNamesLeft", { path: `${CARD_IMAGE}.partial` })
    );
    expect(partial.textContent).not.toBe(
      i18n.t("cardRun.partial.removed", { path: `${CARD_IMAGE}.partial` })
    );
  });

  it("says which phase a stopped build stopped in", async () => {
    cardAnswer = cardResult({ ending: "stopped", phase: "partitions" });
    cardMode();
    renderTab();
    await pressCardBuild();
    await agreeAndContinue();

    await waitFor(() =>
      expect(cardRows().at(-1)).toContain(i18n.t("cardRun.phase.build.stopped"))
    );
    expect(screen.getByTestId("card-stopped-phase").textContent).toBe(
      i18n.t("cardRun.sub.partitions")
    );
  });

  // CLAUDE.md's bar rule: a fixed width over a total nobody stated looks like
  // progress and carries none.
  it("advances the count from the job and draws no bar when the total is unknown", async () => {
    const job = pending();
    cardMode();
    renderTab();
    await pressCardBuild();
    await flush();

    await act(async () => {
      cardPhaseSaid?.({
        jobId: 11,
        session: CARD_SESSION,
        phase: "partitions",
        done: 0,
        total: null,
        unit: "files",
      });
      report?.({
        id: 11,
        title: { key: "components.jobBar.title.buildCardOs", params: { target: CARD_IMAGE } },
        done: 4812,
        total: null,
        message: "",
        state: { state: "running" },
      });
    });

    expect(screen.getByTestId("card-phase-count").textContent).toBe(
      i18n.t("cardRun.count.soFar", { done: 4812 })
    );
    expect(screen.queryByTestId("card-phase-bar")).toBeNull();
    expect(screen.getByTestId("card-phase-now").textContent).toBe(
      i18n.t("cardRun.sub.partitions")
    );

    await act(async () => {
      report?.({
        id: 11,
        title: { key: "components.jobBar.title.buildCardOs", params: { target: CARD_IMAGE } },
        done: 4812,
        total: 9216,
        message: "",
        state: { state: "running" },
      });
    });
    expect(screen.getByTestId("card-phase-count").textContent).toBe(
      i18n.t("cardRun.count.of", { done: 4812, total: 9216 })
    );
    expect(screen.getByTestId("card-phase-bar")).toBeTruthy();
    job.resolve(treeResult());
  });
});
