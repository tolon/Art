// @vitest-environment jsdom
//
// The screen that runs a package's own installer inside an emulator (Task 6
// of the Amiga-side install round). Mocked at the boundary the rest of this
// suite mocks at — the `@/lib/*` wrappers around `invoke`/`listen`, never
// `@tauri-apps/api` itself.
//
// **What these tests are actually for.** This round produced the same defect
// three times and never once as a crash: ART saying a confidently wrong
// sentence. So the assertions below are about *which sentence a person
// reads*, and every one of them is written against what is on screen rather
// than against a catalogue key — a test asserting `outcome.timedOut` would
// pass just as happily if the screen rendered that raw key to the user, which
// is one of the two frontend traps this task was warned about. The other is a
// test that would pass if the component rendered nothing at all; every case
// here therefore asserts on text that must be *present*, and the ones about
// the four endings additionally assert the other three endings' sentences are
// *absent*, which nothing rendering nothing can satisfy.
//
// **No emulator is opened.** `amigaInstallRun` is a mock that answers a job
// id, and the four endings arrive through the mocked result listener — the
// same seam `commands/amigainstall.rs` gave its own tests for exactly this
// reason.

import { useEffect, useRef, useState } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { MemoryRouter } from "react-router-dom";
import i18n from "i18next";

// Side-effecting: gives `useTranslation` a real, synchronously-initialised
// instance, the way `FilesTab.test.tsx` does.
import "@/i18n";
import { useSettingsStore } from "@/stores/settingsStore";
import type {
  AmigaInstallPreview,
  AmigaInstallResult,
  ArchiveClassification,
  RunOutcome,
} from "@/lib/amigainstall";
import { rememberedComponentKey, type ApplyOutcome } from "@/lib/osinstall";
import type {
  ChainReport,
  ChainRow,
  ChainState,
  InstallRelease,
  PackageSummary,
  SlotCandidate,
  SlotReport,
  SlotState,
  TreeSummary,
} from "@/lib/osinstall";
import type { JobProgress } from "@/lib/jobs";

const previewMock = vi.hoisted(() => vi.fn());
const runMock = vi.hoisted(() => vi.fn());
const onResultMock = vi.hoisted(() => vi.fn());
const packagesMock = vi.hoisted(() => vi.fn());
const slotsMock = vi.hoisted(() => vi.fn());
const onJobProgressMock = vi.hoisted(() => vi.fn());
const saveSettingsMock = vi.hoisted(() => vi.fn(async () => {}));
const classifyMock = vi.hoisted(() => vi.fn());
const chainMock = vi.hoisted(() => vi.fn());
const collisionsMock = vi.hoisted(() => vi.fn());
const addPackageMock = vi.hoisted(() => vi.fn());
const onAddPackageResultMock = vi.hoisted(() => vi.fn());
// `useChainTree` asks these two about tab 3's destination (round 5, task 1:
// the panel resolves its own tree now). Mocked at the same boundary as the
// rest — a real call has no Tauri IPC bridge to reach in jsdom.
const describeTreeMock = vi.hoisted(() => vi.fn());
const destinationTakenMock = vi.hoisted(() => vi.fn());

vi.mock("@/lib/amigainstall", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/lib/amigainstall")>()),
  amigaInstallPreview: previewMock,
  amigaInstallRun: runMock,
  onAmigaInstallResult: onResultMock,
  amigainstallClassifyArchive: classifyMock,
}));

vi.mock("@/lib/osinstall", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/lib/osinstall")>()),
  osinstallPackages: packagesMock,
  osinstallSlots: slotsMock,
  // The chain (round 3, task 2) and the host-placed route it runs a row
  // through — the `paketler` step's own two calls, reached from this screen.
  osinstallChain: chainMock,
  osinstallCollisions: collisionsMock,
  osinstallAddPackage: addPackageMock,
  onOsInstallAddPackageResult: onAddPackageResultMock,
  osinstallDescribeTree: describeTreeMock,
  osinstallDestinationTaken: destinationTakenMock,
}));

vi.mock("@/lib/jobs", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/lib/jobs")>()),
  onJobProgress: onJobProgressMock,
}));

// `useRemembered` writes through `useSettingsStore.update()`, which calls the
// real `tauri-plugin-store` IPC on every tick — an unhandled rejection in
// jsdom (see `FilesTab.test.tsx`'s own note).
vi.mock("@/lib/settings", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/lib/settings")>()),
  saveSettings: saveSettingsMock,
  getSettings: vi.fn(async () => ({})),
}));

vi.mock("@tauri-apps/plugin-dialog", () => ({
  open: vi.fn(),
}));

const { AmigaInstallPanel: Panel } = await import("@/components/osbuilder/AmigaInstallPanel");

/**
 * What the panel used to be handed as props, and now reads for itself.
 *
 * **Round 5, task 1.** `AmigaInstallPanel` takes no props: it is mounted in
 * the WinUAE studio as well as nowhere else, and a component that only works
 * where a caller has already resolved its inputs would work on one of its
 * mounts. So it reads `session.release`, `session.material.folders`,
 * `session.packages.folder` and `useChainTree(destination)` itself.
 *
 * Every case below therefore keeps the props it always passed, and the
 * wrapper turns them into the **seeds** the panel reads them back out of.
 * Not one `it(...)` was renamed and not one assertion was rewritten for the
 * move; what changed is where the value enters.
 */
interface PanelSeeds {
  release: InstallRelease;
  treeRoot: string | null;
  packageFolder?: string | null;
  materialFolders?: string[];
  /**
   * **The destination won** — seeded as tab 3's own remembered key rather
   * than as a flag, because that is the only way the panel can learn it: it
   * asks `useChainTree`, which asks `osinstall_describe_tree` about the
   * destination. `describeTreeMock` answers a build by default (see
   * `beforeEach`), so seeding the key is what makes `source` come back
   * `"destination"` — and the cases that do not seed it ask nothing at all,
   * because `useDestinationCheck(null)` makes no round trip.
   */
  treeFromDestination?: boolean;
  /**
   * The destination path itself, when it has to differ from `treeRoot`.
   *
   * **Two distinct paths is what makes the destination-wins assertion a
   * guard** (whole-branch review of round 5, Minor 9). Seeding one path into
   * both the session's tree and tab 3's destination key made the sentence
   * true of either rule, so a panel that read `session.tree.root` alone
   * would have printed the same string. Pass this and the two are told
   * apart. Defaults to `treeRoot`, which is what the cases written before
   * the review already assumed.
   */
  destination?: string;
}

/** Put this build's own values where `useBuildSession` and `useRemembered`
 *  read them, merging rather than replacing: `withChoices()` has usually run
 *  first and owns the `amigaInstall.*` keys. */
function seedSession(seeds: PanelSeeds) {
  const { release, treeRoot, packageFolder = null, materialFolders = [] } = seeds;
  useSettingsStore.setState((state) => ({
    settings: {
      ...state.settings,
      remembered: {
        ...(state.settings.remembered as Record<string, unknown>),
        "buildSession.release": release,
        [`buildSession.material.${release}`]: {
          folders: materialFolders.map((path) => ({ path, layer: null })),
        },
        [`buildSession.packages.${release}`]: { folder: packageFolder, chosen: [] },
        "buildSession.tree": { root: treeRoot, builtHere: false },
        ...(seeds.treeFromDestination
          ? {
              [rememberedComponentKey("osinstall.destination", release)]:
                seeds.destination ?? treeRoot,
            }
          : {}),
      },
    },
  }));
}

/**
 * Every render below goes through a router, because the chain's first row —
 * the CD — is a `<Link>` to the source step, and `react-router`'s `Link`
 * throws outside a `Router`. Wrapping the component here rather than at each
 * of the thirty-odd render sites keeps this file's history readable: not one
 * existing case changed a line to gain a router, and not one changed a line
 * when the props became seeds.
 *
 * **The seeding is split in two on purpose.** The lazy `useState` initialiser
 * runs during this wrapper's *first* render — before `Panel` below it has
 * rendered once, which is what the panel's mount effects need — and nothing
 * else is subscribed to the store at that moment. The effect covers a
 * `rerender` with different props (the two cases that add a material folder
 * mid-test), where a store write during render would be a write into a
 * mounted tree. Neither writes anything the panel itself has not been given.
 */
function AmigaInstallPanel(props: PanelSeeds) {
  useState(() => {
    seedSession(props);
    return null;
  });
  const seeded = useRef(true);
  const key = JSON.stringify(props);
  useEffect(() => {
    if (seeded.current) {
      seeded.current = false;
      return;
    }
    seedSession(props);
    // The props themselves, flattened: a fresh object every render is a fresh
    // identity, and this effect would then re-seed on every one.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [key]);

  return (
    <MemoryRouter>
      <Panel />
    </MemoryRouter>
  );
}

/** The two shipped BoingBags plus the one package that has no Amiga-side
 *  installer — the real catalogue's own shape, so the "only what a recipe
 *  declares is offered" assertion has something that must *not* appear. */
const PACKAGES: PackageSummary[] = [
  {
    id: "boingbag-39-1",
    name: "BoingBag 3.9-1",
    requires: [],
    requiresComponents: [],
    available: true,
    hostPlacementBlock: "encrypted-payload",
    amigaInstallable: true,
    refusedNames: [],
    notYetRunnable: null,
  },
  {
    id: "boingbag-39-2",
    name: "BoingBag 3.9-2",
    requires: ["boingbag-39-1"],
    requiresComponents: [],
    available: true,
    hostPlacementBlock: "encrypted-payload",
    amigaInstallable: true,
    refusedNames: [],
    notYetRunnable: null,
  },
  {
    id: "locale-turkish",
    name: "Türkçe catalogs (BoingBag 3.9-2)",
    requires: [],
    requiresComponents: ["locale-base"],
    available: true,
    hostPlacementBlock: null,
    amigaInstallable: false,
    refusedNames: [],
    notYetRunnable: null,
  },
];

function preview(over: Partial<AmigaInstallPreview> = {}): AmigaInstallPreview {
  return {
    packageId: "boingbag-39-1",
    packageName: "BoingBag 3.9-1",
    tree: "D:/amiga/os39",
    systemVolume: "DH0",
    workingDirectory: "ARTPkg:BoingBag3.9-1",
    program: "ARTPkg:BoingBag3.9-1/C/Updater",
    args: ["AmigaOS-Update", "DH0:"],
    workVolume: "ARTWork",
    packageVolume: "ARTPkg",
    packageArchive: "D:/pkg/BoingBag39-1.lha",
    packageArchivePresent: true,
    packageDir: "BoingBag3.9-1",
    resultFile: "art-result.txt",
    deadlineSeconds: 1800,
    kickstart: "D:/roms/kick31.rom",
    kickstartPresent: true,
    emulator: "C:/Program Files/WinUAE/winuae64.exe",
    profileId: "a1200-aga",
    profileName: "Amiga 1200 (AGA)",
    ...over,
  };
}

/** The choices `AmigaInstallPanel` remembers, put in place directly rather
 *  than driven through four file dialogs: the dialogs are the plugin's, not
 *  this screen's, and what these tests are about is what the screen *says*
 *  once a complete request exists. */
function withChoices() {
  useSettingsStore.setState((state) => ({
    settings: {
      ...state.settings,
      winuaePath: "C:/Program Files/WinUAE/winuae64.exe",
      // `Settings.remembered` is `unknown` by design — `@/lib/remembered`'s
      // guards are what give it a shape — so it is replaced whole here
      // rather than spread.
      remembered: {
        "amigaInstall.package": "boingbag-39-1",
        // Scoped per package (ART-277) — see `amigaInstallArchiveKey`.
        "amigaInstall.archive.boingbag-39-1": "D:/pkg/BoingBag39-1.lha",
        "amigaInstall.kickstart": "D:/roms/kick31.rom",
      },
    },
  }));
}

// ---------------------------------------------------------------------------
// The slots (round 2, § 3.4)
//
// `osinstall_slots` is one answer about the whole material, and these fixtures
// state it the way Rust does. Written out rather than derived from the real
// recipes on purpose: what is under test is *which sentence the panel puts on
// screen for a given answer*, so the answer has to be stated, not computed by
// the same code the screen reads.
// ---------------------------------------------------------------------------

function slot(over: Partial<SlotState["slot"]> = {}): SlotState["slot"] {
  return {
    id: "package:boingbag-39-1",
    kind: "package",
    name: "BoingBag 3.9-1",
    identity: "BoingBag3.9-1",
    artefact: "boingbag-39-1",
    required: false,
    filenames: ["BoingBag39-1.lha"],
    provenance: "the owner's copy, 2026-09-08",
    position: 1,
    requires: [],
    supersededBy: [],
    expectsDirectories: [],
    ...over,
  };
}

function slotState(
  over: Partial<Omit<SlotState, "slot">> & { slot?: Partial<SlotState["slot"]> } = {}
): SlotState {
  const { slot: slotOver, ...rest } = over;
  return {
    slot: slot(slotOver),
    found: null,
    candidates: [],
    installed: { state: "no" },
    chosenMissing: null,
    blockedBy: [],
    incomplete: null,
    ...rest,
  };
}

/** A find. `hash` is the strongest rank and the one the design's own readout
 *  sketch shows for the owner's real material. */
function foundByHash(path: string): SlotState["found"] {
  return { path, matchedBy: "hash", row: null, confirmed: null, bytesRead: { state: "read-no-row" } };
}

/** One candidate nobody has hashed — the ordinary state before the identify
 *  pass has run over a folder. */
const candidate = (path: string): SlotCandidate => ({ path, bytesRead: { state: "not-read" } });

function slotReport(states: SlotState[]): SlotReport {
  return {
    states,
    summary: {
      release: "AmigaOS 3.9",
      requiredTotal: 1,
      requiredFound: 0,
      optionalTotal: states.length,
      optionalFound: states.filter((state) => state.found !== null).length,
    },
    unreadableFolders: [],
    crowdedFolders: [],
  };
}

function emptyReport(): SlotReport {
  return slotReport([]);
}

/** The one live `onAmigaInstallResult` handler, so a test can deliver an
 *  ending the way the backend would. */
let deliver: ((result: AmigaInstallResult) => void) | null = null;

/** The one live `onJobProgress` handler, so a test can deliver a terminal
 *  job event — the channel the Major of fix round 1 lives on. */
let report: ((progress: JobProgress) => void) | null = null;

/** The one live `onOsInstallAddPackageResult` handler — how a host-placed
 *  row's own ending arrives. */
let place: ((result: { job_id: number; outcome: ApplyOutcome }) => void) | null = null;

// ---------------------------------------------------------------------------
// The chain (round 3 § 2)
//
// Stated the way Rust states it, never computed: what is under test is which
// sentence the screen puts on screen for a given answer, so the answer is a
// fixture and not something `chain::rows_for` worked out on the way past.
// ---------------------------------------------------------------------------

function chainRow(
  position: number,
  name: string,
  state: ChainState,
  over: Partial<ChainRow> = {}
): ChainRow {
  return {
    position,
    packageId: null,
    slotId: null,
    name,
    state,
    sentenceFacts: { file: null, runsOnAmiga: null },
    ...over,
  };
}

function chainReport(rows: ChainRow[], installed = 0): ChainReport {
  return {
    rows,
    summary: {
      release: "AmigaOS 3.9",
      total: rows.length,
      installed,
      notNeeded: rows.filter((row) => row.state.state === "not-needed").length,
    },
    unreadableFolders: [],
    crowdedFolders: [],
  };
}

/**
 * The owner's own material, as `chain::rows_for` answers for it: the disc,
 * two BoingBags, the two locale packages the material gives one rank to,
 * Contribution, Euro-Update and the community BoingBags 3&4.
 *
 * **Ranks 4 and 4 are two rows**, and that is the material's own word — it
 * states no order between Locale 3.9 and the Turkish slice. A screen that
 * renumbered them 1..8 would be inventing an order.
 */
function THE_CHAIN(): ChainRow[] {
  return [
    chainRow(1, "AmigaOS3.9", { state: "installed", when: null }, {
      slotId: "medium:AmigaOS3.9",
    }),
    chainRow(2, "BoingBag 3.9-1", { state: "installed", when: null }, {
      packageId: "boingbag-39-1",
      slotId: "package:boingbag-39-1",
      sentenceFacts: { file: "BoingBag39-1.lha", runsOnAmiga: true },
    }),
    chainRow(3, "BoingBag 3.9-2", { state: "ready" }, {
      packageId: "boingbag-39-2",
      slotId: "package:boingbag-39-2",
      sentenceFacts: { file: "BoingBag39-2.lha", runsOnAmiga: true },
    }),
    chainRow(4, "AmigaOS 3.9 Locale update", { state: "ready" }, {
      packageId: "locale-39",
      slotId: "package:locale-39",
      sentenceFacts: { file: "Locale3_9.lha", runsOnAmiga: false },
    }),
    chainRow(4, "Türkçe catalogs (BoingBag 3.9-2)", {
      state: "blocked-by",
      names: ["BoingBag 3.9-2"],
    }, {
      packageId: "locale-turkish",
      slotId: "package:locale-turkish",
      sentenceFacts: { file: "BoingBag39-2-turkce.lha", runsOnAmiga: false },
    }),
    chainRow(6, "BoingBag 3.9-2 Contribution", {
      state: "missing",
      expected: ["BoingBag39-2-Contribution.lha"],
    }, {
      packageId: "boingbag-39-2-contribution",
      slotId: "package:boingbag-39-2-contribution",
      sentenceFacts: { file: null, runsOnAmiga: false },
    }),
    chainRow(7, "Euro-Update", {
      state: "not-needed",
      supersededBy: "BoingBags 3&4 for AmigaOS 3.9",
    }, {
      packageId: "euro-update",
      slotId: "package:euro-update",
      sentenceFacts: { file: "Euro-Update.lha", runsOnAmiga: null },
    }),
    chainRow(8, "BoingBags 3&4 for AmigaOS 3.9", {
      state: "not-yet-runnable",
      reason: "installer-not-measured",
    }, {
      packageId: "boingbags-39-3-4",
      slotId: "package:boingbags-39-3-4",
      sentenceFacts: { file: "BoingBags3&4.lha", runsOnAmiga: true },
    }),
  ];
}

beforeEach(() => {
  vi.clearAllMocks();
  deliver = null;
  report = null;
  useSettingsStore.setState((state) => ({
    settings: { ...state.settings, uxMode: "beginner", winuaePath: null, remembered: {} },
  }));
  packagesMock.mockResolvedValue(PACKAGES);
  // Nothing resolved, which is the state of a panel nobody has given a
  // material folder: every field falls back to the browse row it always had.
  // The block at the end of this file is where slots actually answer.
  slotsMock.mockResolvedValue(emptyReport());
  previewMock.mockResolvedValue(preview());
  runMock.mockResolvedValue(7);
  // The ordinary answer: whatever was picked is the package's own archive.
  // One constant, because there is one field to ask about since 2026-09-09.
  // Individual tests override this to exercise ART-277's own blockers.
  classifyMock.mockImplementation(async () => ({
    kind: "the-package",
    topLevel: [],
    expectedMedia: null,
    sharedBy: [],
  }));
  onJobProgressMock.mockImplementation(async (handler: (p: JobProgress) => void) => {
    report = handler;
    return () => {};
  });
  onResultMock.mockImplementation(async (handler: (r: AmigaInstallResult) => void) => {
    deliver = handler;
    return () => {};
  });
  // **No chain by default**, which is a real answer and not an absence: a
  // release ART knows no update chain for answers with no rows, and the
  // panel then shows the package list it had before the chain existed. Every
  // case written before round 3 therefore reads exactly as it did.
  chainMock.mockResolvedValue(chainReport([]));
  // **A build, and that is not the default answer it looks like.** Only the
  // cases that seed a destination reach this at all — with no destination
  // `useDestinationCheck` makes no round trip — so answering *is a tree*
  // here is what makes `treeFromDestination` mean "the user seeded one",
  // rather than a second thing every case would have to opt out of.
  describeTreeMock.mockResolvedValue({
    isTree: true,
    release: "AmigaOS 3.9",
    files: 1915,
    components: ["workbench-base"],
    amigaInstalled: [],
    problem: null,
  });
  destinationTakenMock.mockResolvedValue(false);
  collisionsMock.mockResolvedValue([]);
  addPackageMock.mockResolvedValue({ outcome: "started", job_id: 11 });
  onAddPackageResultMock.mockImplementation(
    async (handler: (r: { job_id: number; outcome: ApplyOutcome }) => void) => {
      place = handler;
      return () => {};
    }
  );
});

afterEach(() => {
  cleanup();
});

/** Render, wait for the preview, tick the confirmation and press Run. */
async function runToConfirmation() {
  withChoices();
  render(<AmigaInstallPanel release="AmigaOS 3.9" treeRoot="D:/amiga/os39" packageFolder="D:/pkg" />);
  await screen.findByTestId("amiga-install-preview");
  const user = userEvent.setup();
  await user.click(screen.getByRole("checkbox"));
  await user.click(screen.getByRole("button", { name: i18n.t("osinstall.amigaInstall.run") }));
  await waitFor(() => expect(runMock).toHaveBeenCalled());
}

describe("a package chosen for another release does not survive the switch (ART-212)", () => {
  // The owner, on the OS Builder's fourth step with AmigaOS 3.2 chosen: the
  // package list correctly said ART carries no runnable package for 3.2
  // (ART-209 working), and directly underneath it the panel still showed
  // BoingBag39-1.lha, BoingBag39-2.lha, AmigaOS39.iso and a full "what will
  // run" card reading `ARTPkg:BoingBag3.9-1/C/Updater ...`.
  //
  // The preview was computed from the remembered `amigaInstall.package` and
  // never looked at the catalogue at all, so an id the chosen release does
  // not offer still drove a run plan. A screen offering to run an installer
  // for an operating system that is not there is the same class as the
  // sixteen refusals of ART-208: every line true of *something*, and the
  // whole false.
  it("previews nothing for a package the chosen release does not carry", async () => {
    withChoices();
    // AmigaOS 3.2 has no update package at all — what ART-209 makes
    // `osinstallPackages` answer for it.
    packagesMock.mockResolvedValue([]);

    render(
      <AmigaInstallPanel
        release="AmigaOS 3.2"
        treeRoot="D:/amiga/dist-3.2"
        packageFolder="D:/pkg"
      />
    );

    await waitFor(() => expect(packagesMock).toHaveBeenCalledWith("D:/pkg", "AmigaOS 3.2"));
    // Never previewed, not merely un-rendered: a run plan for a package this
    // release does not have is work ART should not even ask for.
    expect(previewMock).not.toHaveBeenCalled();
    expect(screen.queryByTestId("amiga-install-preview")).toBeNull();
  });

  it("shows no run form at all when the release carries nothing runnable", async () => {
    // The other half of what the owner saw. The sentence "ART carries no
    // package with an installer for this release" was already on screen — and
    // under it sat four filled-in fields naming the previous release's
    // archives and disc. A form for a run that cannot be configured is the
    // screen offering what it has just said it does not have.
    withChoices();
    packagesMock.mockResolvedValue([]);
    render(
      <AmigaInstallPanel
        release="AmigaOS 3.2"
        treeRoot="D:/amiga/dist-3.2"
        packageFolder="D:/pkg"
      />
    );

    expect(
      await screen.findByText(i18n.t("osinstall.amigaInstall.package.none"))
    ).toBeTruthy();
    await waitFor(() =>
      expect(screen.queryByText(i18n.t("osinstall.amigaInstall.archive.label"))).toBeNull()
    );
    expect(screen.queryByText(i18n.t("osinstall.amigaInstall.medium.label"))).toBeNull();
  });

  it("still previews the package the release does carry", async () => {
    // The other arm. A filter that previewed nothing at all would satisfy
    // the test above and break the panel.
    withChoices();
    render(
      <AmigaInstallPanel
        release="AmigaOS 3.9"
        treeRoot="D:/amiga/os39"
        packageFolder="D:/pkg"
      />
    );
    expect(await screen.findByTestId("amiga-install-preview")).toBeTruthy();
  });
});

describe("before anything opens", () => {
  // "The emulator is a window on the owner's desktop. The confirmation says
  // so *before* it opens." An earlier round opened one repeatedly without
  // warning and that was a real annoyance.
  it("says an emulator window will open, before the run and in beginner mode", async () => {
    render(<AmigaInstallPanel release="AmigaOS 3.9" treeRoot={null} packageFolder={null} />);
    const warning = screen.getByTestId("emulator-window-warning");
    expect(warning.textContent).toBe(i18n.t("osinstall.amigaInstall.emulatorWindow"));
    expect(warning.textContent).toMatch(/emulator window/i);
    expect(runMock).not.toHaveBeenCalled();
    expect(previewMock).not.toHaveBeenCalled();
  });

  it("says the tree is copied and only replaced on success", () => {
    render(<AmigaInstallPanel release="AmigaOS 3.9" treeRoot={null} packageFolder={null} />);
    expect(screen.getByText(i18n.t("osinstall.amigaInstall.copyNote"))).toBeTruthy();
  });

  // The prerequisite chain is legible *before* the run, not only inside a
  // refusal — "refused" without the order is not a next step.
  it("names the order the packages go on in", () => {
    render(<AmigaInstallPanel release="AmigaOS 3.9" treeRoot={null} packageFolder={null} />);
    const note = screen.getByText(i18n.t("osinstall.amigaInstall.chainNote"));
    expect(note.textContent).toMatch(/BoingBag 3\.9-1.*BoingBag 3\.9-2/s);
  });

  it("offers only the packages whose own recipe declares an Amiga-side installer", async () => {
    render(<AmigaInstallPanel release="AmigaOS 3.9" treeRoot="D:/amiga/os39" packageFolder="D:/pkg" />);
    await waitFor(() => expect(screen.getAllByTestId("amiga-package-row")).toHaveLength(2));
    const rows = screen.getAllByTestId("amiga-package-row").map((row) => row.textContent ?? "");
    expect(rows[0]).toContain("BoingBag 3.9-1");
    expect(rows[1]).toContain("BoingBag 3.9-2");
    // `locale-turkish` has no `amiga_installer`; offering it would be a pick
    // `compose` refuses by name a moment later.
    expect(screen.queryByText(/Türkçe catalogs/)).toBeNull();
    // And BoingBag 2's own prerequisite is on its row, from the catalogue's
    // data rather than from a sentence written here.
    expect(
      screen.getByText(i18n.t("osinstall.packages.requiresPackages", { list: "BoingBag 3.9-1" }))
    ).toBeTruthy();
  });

  // **Registered unready, never hidden** (§10/§89, round 3). BoingBags 3&4
  // declares an installer so the row exists at all, and declares
  // `notYetRunnable` because nobody has driven its Installer script
  // unattended. Both halves are asserted: the row is there *and* it cannot
  // be picked, and the sentence says what has not been measured.
  it("shows a package nobody has run yet, disabled, with the reason on its row", async () => {
    packagesMock.mockResolvedValue([
      ...PACKAGES,
      {
        id: "boingbags-39-3-4",
        name: "BoingBags 3&4 for AmigaOS 3.9",
        requires: ["boingbag-39-2"],
        requiresComponents: [],
        available: true,
        hostPlacementBlock: "needs-installer-script",
        amigaInstallable: true,
        refusedNames: [],
        notYetRunnable: "installer-not-measured",
      },
    ] satisfies PackageSummary[]);

    render(<AmigaInstallPanel release="AmigaOS 3.9" treeRoot="D:/amiga/os39" packageFolder="D:/pkg" />);
    const rows = await screen.findAllByTestId("amiga-package-row");
    expect(rows).toHaveLength(3);
    expect(rows[2].textContent).toContain("BoingBags 3&4");

    const radios = screen.getAllByRole("radio");
    expect(radios[0].hasAttribute("disabled")).toBe(false);
    expect(radios[2].hasAttribute("disabled")).toBe(true);

    // The whole sentence comes from the catalogue (fix round 1, m6) — no
    // English clause is interpolated into a translated frame, so this is a
    // real translated sentence in both languages rather than half of one.
    const sentence = screen.getByTestId("amiga-package-not-yet-runnable").textContent;
    expect(sentence).toBe(
      i18n.t("osinstall.amigaInstall.package.notYetRunnable.installerNotMeasured")
    );
    expect(sentence).not.toContain("installer-not-measured");
  });

  it("will not let the run be confirmed until a preview exists", () => {
    render(<AmigaInstallPanel release="AmigaOS 3.9" treeRoot={null} packageFolder={null} />);
    expect(screen.getByRole("checkbox").hasAttribute("disabled")).toBe(true);
    expect(
      screen.getByRole("button", { name: i18n.t("osinstall.amigaInstall.run") }).hasAttribute("disabled")
    ).toBe(true);
  });
});

describe("a refusal says which reason applies", () => {
  // ART-060: the sentence is Rust's and is English. Replacing it with one
  // translated "it was refused" would lose the half that matters — *which*
  // reason, and what to do about it.
  it("shows the prerequisite refusal verbatim, naming what is missing and in what order", async () => {
    const said =
      "'BoingBag 3.9-2' has to go on after BoingBag 3.9-1, and 'D:/amiga/os39' does not have it " +
      "yet — install BoingBag 3.9-1 first, in that order.";
    previewMock.mockRejectedValue(said);
    withChoices();
    render(<AmigaInstallPanel release="AmigaOS 3.9" treeRoot="D:/amiga/os39" packageFolder="D:/pkg" />);

    const refusal = await screen.findByTestId("amiga-install-refusal");
    expect(refusal.textContent).toContain("install BoingBag 3.9-1 first, in that order");
    // And the half ART *can* say in the user's own language: nothing was
    // copied. A refusal happens before the tree is touched at all.
    expect(refusal.textContent).toContain(i18n.t("osinstall.amigaInstall.refused.nothingCopied"));
    // Nothing offers a run for something that cannot happen.
    expect(screen.queryByTestId("amiga-install-preview")).toBeNull();
    expect(screen.getByRole("checkbox").hasAttribute("disabled")).toBe(true);
  });

// ART-202. The owner pressed the run button seven times against an
  // unchanged request, and their operation log recorded every one of them:
  // the refusal rendered 209 lines of JSX above the button, so on a maximised
  // window pressing it changed nothing they could see.
  //
  // The first fix rendered it in **both** places, and the owner read that as
  // two separate errors — *"aynı uyarı tek ekranda 2 tane"*. They were right:
  // a refusal means the preview did not succeed, so there is no preview card,
  // so the panel is short and the two boxes land within a screen of each
  // other. One box, at the control, is what the rule actually asks for.
  it("says why beside the button, and says it once", async () => {
    previewMock.mockRejectedValue(
      "'D:/pkg/BoingBag39-1-UAE.lha' is this package's update archive, not the package itself"
    );
    withChoices();
    render(<AmigaInstallPanel release="AmigaOS 3.9" treeRoot="D:/amiga/os39" packageFolder="D:/pkg" />);

    const boxes = await screen.findAllByTestId("amiga-install-refusal");
    expect(boxes.length).toBe(1);
    expect(boxes[0].textContent).toContain("update archive");

    // At the control — its immediate neighbour, not merely somewhere else on
    // the same screen. "Follows the button" would pass with the two a
    // thousand pixels apart, which is the defect.
    const button = screen.getByRole("button", {
      name: i18n.t("osinstall.amigaInstall.run"),
    });
    expect(boxes[0].nextElementSibling?.contains(button)).toBe(true);
  });
});

describe("what a previewed run still lacks", () => {
  it("names each missing thing and refuses the confirmation until they are there", async () => {
    previewMock.mockResolvedValue(
      preview({ kickstartPresent: false, emulator: null, packageArchivePresent: false })
    );
    withChoices();
    render(<AmigaInstallPanel release="AmigaOS 3.9" treeRoot="D:/amiga/os39" packageFolder="D:/pkg" />);

    const blockers = await screen.findByTestId("amiga-install-blockers");
    expect(blockers.textContent).toContain("D:/roms/kick31.rom");
    expect(blockers.textContent).toContain(i18n.t("osinstall.amigaInstall.blocker.noEmulator"));
    expect(screen.getByRole("checkbox").hasAttribute("disabled")).toBe(true);
  });
});

describe("the four endings stay four sentences on screen", () => {
  /** Every ending's own sentence, as the user reads it. */
  const SAID: Record<string, string> = {
    succeeded: i18n.t("osinstall.amigaInstall.outcome.succeeded"),
    failed: i18n.t("osinstall.amigaInstall.outcome.failed"),
    "timed-out": i18n.t("osinstall.amigaInstall.outcome.timedOut", { seconds: 1800 }),
    "emulator-closed": i18n.t("osinstall.amigaInstall.outcome.emulatorClosed", { seconds: 12 }),
  };

  /** And the next step each one must carry. A defect that swaps two of
   *  these keeps four distinct sentences and gives half the readers the
   *  wrong instruction, which is the failure mode the four endings exist
   *  to prevent. */
  const NEXT: Record<string, string> = {
    succeeded: i18n.t("osinstall.amigaInstall.next.succeeded"),
    failed: i18n.t("osinstall.amigaInstall.next.failed"),
    "timed-out": i18n.t("osinstall.amigaInstall.next.timedOut"),
    "emulator-closed": i18n.t("osinstall.amigaInstall.next.emulatorClosed"),
  };

  const ENDINGS: RunOutcome[] = [
    { kind: "succeeded" },
    { kind: "failed" },
    { kind: "timed-out", waited: { secs: 1800, nanos: 0 } },
    { kind: "emulator-closed", waited: { secs: 12, nanos: 0 } },
  ];

  for (const ending of ENDINGS) {
    it(`says its own sentence for '${ending.kind}' and none of the other three`, async () => {
      await runToConfirmation();
      const settlement =
        ending.kind === "succeeded"
          ? ({ kind: "promoted", tree: "D:/amiga/os39", leftBehind: null } as const)
          : ({ kind: "kept", copy: "D:/amiga/os39.art-run", original: "D:/amiga/os39" } as const);
      deliver!({ job_id: 7, outcome: ending, settlement });

      const outcome = await screen.findByTestId("amiga-install-outcome");
      expect(outcome.textContent).toBe(SAID[ending.kind]);
      // The whole point: no other ending's sentence is on screen. A screen
      // rendering nothing at all fails the line above, and a screen
      // collapsing two endings fails this one.
      const report = screen.getByTestId("amiga-install-report").textContent ?? "";
      for (const [kind, sentence] of Object.entries(SAID)) {
        if (kind === ending.kind) continue;
        expect(report).not.toContain(sentence);
      }
      // And *this ending's own* next step — "watch the window next time" is
      // the wrong advice for a window the owner shut themselves. Asserted
      // against the expected sentence rather than merely against being
      // different from the outcome, which nothing plausible could break:
      // swapping two endings' next steps round is a permutation, so it keeps
      // them four and distinct and would sail past a distinctness check.
      const next = screen.getByTestId("amiga-install-next").textContent ?? "";
      expect(next).toBe(NEXT[ending.kind]);
    });
  }

  it("gives the four endings four different next steps", async () => {
    const steps = new Set<string>();
    for (const ending of ENDINGS) {
      await runToConfirmation();
      deliver!({
        job_id: 7,
        outcome: ending,
        settlement: { kind: "kept", copy: "D:/c", original: "D:/o" },
      });
      steps.add((await screen.findByTestId("amiga-install-next")).textContent ?? "");
      cleanup();
    }
    expect(steps.size).toBe(4);
  });
});

describe("where the copy is", () => {
  // "A user told 'it failed' and not told where the evidence went has been
  // given nothing."
  it("names the copy and says the original was untouched, for every ending that is not success", async () => {
    for (const ending of [
      { kind: "failed" } as const,
      { kind: "timed-out", waited: { secs: 1800, nanos: 0 } } as const,
      { kind: "emulator-closed", waited: { secs: 12, nanos: 0 } } as const,
    ]) {
      await runToConfirmation();
      deliver!({
        job_id: 7,
        outcome: ending,
        settlement: { kind: "kept", copy: "D:/amiga/os39.art-run", original: "D:/amiga/os39" },
      });
      const settlement = await screen.findByTestId("amiga-install-settlement");
      expect(settlement.textContent, ending.kind).toContain("D:/amiga/os39.art-run");
      expect(settlement.textContent, ending.kind).toContain("D:/amiga/os39");
      cleanup();
    }
  });

  it("names a retired tree it could not delete, rather than staying silent about it", async () => {
    await runToConfirmation();
    deliver!({
      job_id: 7,
      outcome: { kind: "succeeded" },
      settlement: { kind: "promoted", tree: "D:/amiga/os39", leftBehind: "D:/amiga/os39.art-old" },
    });
    const settlement = await screen.findByTestId("amiga-install-settlement");
    expect(settlement.textContent).toContain("D:/amiga/os39.art-old");
  });
});

describe("beginner mode hides and never disables", () => {
  it("keeps the warning, the copy note and the run available, and hides only the machinery", async () => {
    withChoices();
    render(<AmigaInstallPanel release="AmigaOS 3.9" treeRoot="D:/amiga/os39" packageFolder="D:/pkg" />);
    await screen.findByTestId("amiga-install-preview");

    // Hidden in beginner mode: the AmigaDOS command line and the volumes.
    expect(screen.queryByTestId("amiga-install-detail")).toBeNull();
    // Never hidden, and never disabled: the announcement, and the run itself.
    expect(screen.getByTestId("emulator-window-warning")).toBeTruthy();
    expect(
      screen.getByRole("button", { name: i18n.t("osinstall.amigaInstall.run") }).hasAttribute("disabled")
    ).toBe(true); // …until confirmed, which is a confirmation and not the mode
    const user = userEvent.setup();
    await user.click(screen.getByRole("checkbox"));
    expect(
      screen.getByRole("button", { name: i18n.t("osinstall.amigaInstall.run") }).hasAttribute("disabled")
    ).toBe(false);
  });

  it("shows the command line and the three volumes in power mode", async () => {
    withChoices();
    useSettingsStore.setState((state) => ({ settings: { ...state.settings, uxMode: "power" } }));
    render(<AmigaInstallPanel release="AmigaOS 3.9" treeRoot="D:/amiga/os39" packageFolder="D:/pkg" />);

    const detail = await screen.findByTestId("amiga-install-detail");
    expect(detail.textContent).toContain("ARTPkg:BoingBag3.9-1/C/Updater AmigaOS-Update DH0:");
    expect(detail.textContent).toContain("ARTWork");
  });
});

describe("the run itself", () => {
  it("sends the tree, the package and the archive exactly as chosen", async () => {
    useSettingsStore.setState((state) => ({
      settings: {
        ...state.settings,
        winuaePath: "C:/WinUAE/winuae64.exe",
        remembered: {
          "amigaInstall.package": "boingbag-39-1",
          "amigaInstall.archive.boingbag-39-1": "D:/pkg/BoingBag39-1.lha",
          "amigaInstall.kickstart": "D:/roms/kick31.rom",
        },
      },
    }));
    render(<AmigaInstallPanel release="AmigaOS 3.9" treeRoot="D:/amiga/os39" packageFolder="D:/pkg" />);
    await screen.findByTestId("amiga-install-preview");
    const user = userEvent.setup();
    await user.click(screen.getByRole("checkbox"));
    await user.click(screen.getByRole("button", { name: i18n.t("osinstall.amigaInstall.run") }));

    await waitFor(() => expect(runMock).toHaveBeenCalled());
    expect(runMock.mock.calls[0][0]).toEqual({
      tree: "D:/amiga/os39",
      packageId: "boingbag-39-1",
      packageArchive: "D:/pkg/BoingBag39-1.lha",
      kickstart: "D:/roms/kick31.rom",
    });
    expect(runMock.mock.calls[0][1]).toBe("C:/WinUAE/winuae64.exe");
  });

  it("shows a refusal raised at the run instead of a job that could only go red", async () => {
    runMock.mockRejectedValue("WinUAE was not found in a standard install location");
    await runToConfirmation();
    const refusal = await screen.findByTestId("amiga-install-refusal");
    expect(refusal.textContent).toContain("WinUAE was not found");
  });
});

// ---------------------------------------------------------------------------
// Fix round 1
// ---------------------------------------------------------------------------

describe("a run that goes wrong mid-flight still says where the copy is", () => {
  /** What `commands/amigainstall.rs::perform` really reports on that path,
   *  word for word, immediately before it returns the error. */
  const REPORTED =
    "'D:/amiga/os39' was not touched; the copy ART installed into is at 'D:/amiga/os39.art-run'";

  // The Major of this task's review, and the round's signature defect coming
  // in through a door nobody had checked: this is the **one** path where a
  // copy really is orphaned, and it was the one path that said nothing about
  // it. Every other ending was handled.
  it("renders ART's own last word beside the error, naming the copy and the untouched tree", async () => {
    await runToConfirmation();
    report!({
      id: 7,
      title: "Installing BoingBag 3.9-1 on the Amiga",
      done: 0,
      total: null,
      message: REPORTED,
      state: { state: "failed", error_code: "ART-014", message: "the mount went away" },
    });

    const badge = await screen.findByTestId("amiga-install-job-error");
    // The error itself is still there…
    expect(badge.textContent).toContain("the mount went away");
    expect(badge.textContent).toContain("ART-014");
    // …and so is where the evidence went. Both paths, not just the error.
    expect(badge.textContent).toContain("D:/amiga/os39.art-run");
    expect(badge.textContent).toContain("was not touched");
    expect(screen.getByTestId("amiga-install-last-reported").textContent).toBe(REPORTED);
  });

  it("says nothing extra when the run reported nothing at the end", async () => {
    await runToConfirmation();
    report!({
      id: 7,
      title: "Installing BoingBag 3.9-1 on the Amiga",
      done: 0,
      total: null,
      message: "   ",
      state: { state: "failed", error_code: "ART-014", message: "the mount went away" },
    });

    const badge = await screen.findByTestId("amiga-install-job-error");
    expect(badge.textContent).toContain("the mount went away");
    expect(screen.queryByTestId("amiga-install-last-reported")).toBeNull();
  });

  it("ignores a job that is not this panel's", async () => {
    await runToConfirmation();
    // The flush is the test. Written without it, this asserted before React
    // had rendered anything the handler queued, so it passed with the job-id
    // guard deleted — a test that could not fail, caught by mutating the
    // guard rather than by reading the test (mutation MJ4, round 1).
    await act(async () => {
      report!({
        id: 99,
        title: "Something else entirely",
        done: 0,
        total: null,
        message: REPORTED,
        state: { state: "failed", error_code: "ART-014", message: "the mount went away" },
      });
      await Promise.resolve();
    });
    expect(screen.queryByTestId("amiga-install-job-error")).toBeNull();
    expect(screen.queryByTestId("amiga-install-last-reported")).toBeNull();
  });
});

describe("a cancelled run does not claim the copy was cleaned up when it was not", () => {
  // The same defect as the Major, on the cancelled channel: `perform` reports
  // when a cancelled run's copy could **not** be removed, and the screen used
  // to answer that with a flat "the copy has been discarded".
  it("shows what ART said about the copy it could not remove", async () => {
    const said = "The cancelled run's copy could not be removed: Access is denied. (os error 5)";
    await runToConfirmation();
    report!({
      id: 7,
      title: "Installing BoingBag 3.9-1 on the Amiga",
      done: 0,
      total: null,
      message: said,
      state: { state: "cancelled", files_landed: null },
    });

    const badge = await screen.findByTestId("amiga-install-cancelled");
    // The one thing that is true either way, in the user's own language.
    expect(badge.textContent).toContain(i18n.t("osinstall.amigaInstall.cancelled"));
    // And the thing only ART knows, verbatim.
    expect(badge.textContent).toContain("could not be removed");
    // The sentence that used to be here must not be: a copy still on disk
    // described as discarded is the wrong sentence, not a rounding error.
    expect(i18n.t("osinstall.amigaInstall.cancelled")).not.toMatch(/discard/i);
    expect(badge.textContent).not.toMatch(/has been discarded/i);
    // A cancellation is not a failure and must not go red.
    expect(screen.queryByTestId("amiga-install-job-error")).toBeNull();
  });
});

describe("while it is running", () => {
  // The same question as the Major, asked of the running half of the job
  // channel: an install takes minutes, and the phase ART reports is the only
  // sign of life ART itself controls.
  it("shows the phase ART reports, not only a percentage", async () => {
    await runToConfirmation();
    await act(async () => {
      report!({
        id: 7,
        title: "Installing BoingBag 3.9-1 on the Amiga",
        done: 3,
        total: 10,
        message: "Unpacking BoingBag39-1.lha",
        state: { state: "running" },
      });
      await Promise.resolve();
    });
    expect(screen.getByTestId("amiga-install-phase").textContent).toBe(
      "Unpacking BoingBag39-1.lha"
    );
    // …and the run is still running: a progress event is not an ending.
    expect(screen.queryByTestId("amiga-install-report")).toBeNull();
    expect(screen.queryByTestId("amiga-install-job-error")).toBeNull();
  });
});

describe("one Kickstart for the build (ART-197's fourth row)", () => {
  /// **Seeded through a key this panel never owned.** `osinstall.rom` belongs
  /// to the install step; before wave 2 this panel read
  /// `amigaInstall.kickstart` and would have shown nothing. Reverting it to
  /// its own key fails here, which is the only thing that proves the panel
  /// actually reads the session rather than merely compiling against it.
  it("shows the Kickstart chosen on another step", async () => {
    useSettingsStore.setState((state) => ({
      settings: {
        ...state.settings,
        winuaePath: "C:/Program Files/WinUAE/winuae64.exe",
        remembered: {
          "amigaInstall.package": "boingbag-39-1",
          "amigaInstall.archive.boingbag-39-1": "D:/pkg/BoingBag39-1.lha",
          // Deliberately *not* "amigaInstall.kickstart".
          "osinstall.rom": "D:/roms/from-the-install-step.rom",
        },
      },
    }));

    render(
      <AmigaInstallPanel release="AmigaOS 3.9" treeRoot="D:/amiga/os39" packageFolder="D:/pkg" />
    );

    expect(
      await screen.findByText("D:/roms/from-the-install-step.rom")
    ).toBeTruthy();
  });
});

// ---------------------------------------------------------------------------
// ART-277: the owner ran BoingBag 1, then supplied BoingBag 2's own archive
// while BoingBag 1 was still selected, and read a refusal quoting an
// internal overlay path that never said either package's name.
// ---------------------------------------------------------------------------

describe("ART-277: switching the selected package does not carry its archives", () => {
  it("switching the selected package reads that package's own remembered archive", async () => {
    useSettingsStore.setState((state) => ({
      settings: {
        ...state.settings,
        winuaePath: "C:/Program Files/WinUAE/winuae64.exe",
        remembered: {
          "amigaInstall.package": "boingbag-39-1",
          "amigaInstall.archive.boingbag-39-1": "D:/pkg/BoingBag39-1.lha",
          "amigaInstall.kickstart": "D:/roms/kick31.rom",
        },
      },
    }));
    render(<AmigaInstallPanel release="AmigaOS 3.9" treeRoot="D:/amiga/os39" packageFolder="D:/pkg" />);

    // BoingBag 1's own archive is showing, under its own key.
    expect(await screen.findByText("D:/pkg/BoingBag39-1.lha")).toBeTruthy();

    const user = userEvent.setup();
    const rows = await screen.findAllByTestId("amiga-package-row");
    const boingbag2Radio = rows[1].querySelector("input[type=radio]") as HTMLInputElement;
    await user.click(boingbag2Radio);

    // BoingBag 2's own archive field is empty — never BoingBag 1's path,
    // which is the owner's own defect (ART-277's first cause).
    await waitFor(() =>
      expect(screen.queryByText("D:/pkg/BoingBag39-1.lha")).toBeNull()
    );
    expect(
      screen.getByText(i18n.t("osinstall.amigaInstall.archive.none"))
    ).toBeTruthy();

    // And BoingBag 1's own choice is still remembered, under its own key —
    // nothing was cleared, a different key is read (CLAUDE.md: nothing
    // changes unless the user changes it).
    expect(useSettingsStore.getState().settings.remembered).toMatchObject({
      "amigaInstall.archive.boingbag-39-1": "D:/pkg/BoingBag39-1.lha",
    });
  });

  it("a wrong-package archive disables Run and the confirm checkbox, and names which package it belongs to in the blockers list", async () => {
    // ART-277 review, Medium 2: the reason renders where the button is —
    // the `blockers` box, directly above the checkbox — not in a second box
    // beside the field, which is the same "aynı uyarı tek ekranda 2 tane"
    // mistake ART-202 already named on this exact screen.
    classifyMock.mockImplementation(async (path: string) =>
      path === "D:/pkg/BoingBag39-2.lha"
        ? {
            kind: "another-package:boingbag-39-2",
            topLevel: ["BoingBag3.9-2", "BoingBag3.9-2.info"],
            expectedMedia: null,
                }
        : { kind: "the-package", topLevel: [], expectedMedia: null, sharedBy: [] }
    );
    useSettingsStore.setState((state) => ({
      settings: {
        ...state.settings,
        winuaePath: "C:/Program Files/WinUAE/winuae64.exe",
        remembered: {
          "amigaInstall.package": "boingbag-39-1",
          // BoingBag 2's own archive, sitting under BoingBag 1's own key —
          // exactly the owner's mistake, reproduced directly rather than
          // driven through the file picker.
          "amigaInstall.archive.boingbag-39-1": "D:/pkg/BoingBag39-2.lha",
          "amigaInstall.kickstart": "D:/roms/kick31.rom",
        },
      },
    }));
    render(<AmigaInstallPanel release="AmigaOS 3.9" treeRoot="D:/amiga/os39" packageFolder="D:/pkg" />);

    const blockers = await screen.findByTestId("amiga-install-blockers");
    expect(blockers.textContent).toContain("BoingBag 3.9-2");
    expect(blockers.textContent).toContain("BoingBag 3.9-1");

    // The checkbox itself is disabled — not merely left untouched at "run
    // stays disabled" while a person could still tick "I understand" over a
    // request that could never succeed.
    expect(screen.getByRole("checkbox").hasAttribute("disabled")).toBe(true);
    expect(
      screen.getByRole("button", { name: i18n.t("osinstall.amigaInstall.run") }).hasAttribute(
        "disabled"
      )
    ).toBe(true);
    expect(runMock).not.toHaveBeenCalled();
  });

  it("says only that this step does not run an other-artefact archive, never which step does", async () => {
    classifyMock.mockImplementation(async () => ({
      kind: "other-artefact:LocaleUpdate",
      topLevel: ["LocaleUpdate", "LocaleUpdate.info"],
      expectedMedia: "BoingBag3.9-1",
      sharedBy: [],
    }));
    withChoices();
    render(<AmigaInstallPanel release="AmigaOS 3.9" treeRoot="D:/amiga/os39" packageFolder="D:/pkg" />);

    const blockers = await screen.findByTestId("amiga-install-blockers");
    expect(blockers.textContent).toContain("LocaleUpdate");
    expect(blockers.textContent).not.toMatch(/Packages step/i);
    expect(blockers.textContent).not.toMatch(/Windows/i);
    expect(screen.getByRole("checkbox").hasAttribute("disabled")).toBe(true);
  });

  // ART-277 round 1 whole-branch review, M1: the classification must not
  // outlive the file it was asked about.
  it("clears the previous verdict immediately, before the new archive's own answer lands", async () => {
    let resolvePending: (value: ArchiveClassification) => void = () => {};
    const pending = new Promise<ArchiveClassification>((resolve) => {
      resolvePending = resolve;
    });
    classifyMock.mockImplementation(async (path: string) => {
      if (path === "D:/pkg/Locale3.9.lha") {
        return {
          kind: "other-artefact:Locale3.9",
          topLevel: ["Locale3.9", "Locale3.9.info"],
          expectedMedia: "BoingBag3.9-1",
              sharedBy: [],
        };
      }
      if (path === "D:/pkg/pending.lha") {
        // Never resolves within this test — if the stale verdict were not
        // cleared immediately, it would be the only thing on screen for
        // the whole time this promise is outstanding.
        return pending;
      }
      return {
        kind: "the-package",
        topLevel: [],
        expectedMedia: null,
          sharedBy: [],
      };
    });
    useSettingsStore.setState((state) => ({
      settings: {
        ...state.settings,
        winuaePath: "C:/Program Files/WinUAE/winuae64.exe",
        remembered: {
          "amigaInstall.package": "boingbag-39-1",
          "amigaInstall.archive.boingbag-39-1": "D:/pkg/Locale3.9.lha",
          "amigaInstall.kickstart": "D:/roms/kick31.rom",
        },
      },
    }));
    render(<AmigaInstallPanel release="AmigaOS 3.9" treeRoot="D:/amiga/os39" packageFolder="D:/pkg" />);

    // The first (stale-to-be) verdict is up.
    let blockers = await screen.findByTestId("amiga-install-blockers");
    expect(blockers.textContent).toContain("Locale3.9");

    // The archive changes to a file whose classify call this test holds
    // pending forever.
    await act(async () => {
      useSettingsStore.setState((state) => ({
        settings: {
          ...state.settings,
          remembered: {
            "amigaInstall.package": "boingbag-39-1",
            "amigaInstall.archive.boingbag-39-1": "D:/pkg/pending.lha",
            "amigaInstall.kickstart": "D:/roms/kick31.rom",
          },
        },
      }));
      await Promise.resolve();
    });

    // The stale "Locale3.9" verdict must be gone immediately — not held
    // over until the pending promise resolves, which in this test never
    // happens at all.
    await waitFor(() => {
      const box = screen.queryByTestId("amiga-install-blockers");
      if (box) {
        expect(box.textContent).not.toContain("Locale3.9");
      }
    });

    // Resolving the new answer renders the new verdict in its place.
    resolvePending({
      kind: "other-artefact:Locale3.9",
      topLevel: [],
      expectedMedia: "BoingBag3.9-1",
      sharedBy: [],
    });
    blockers = await screen.findByTestId("amiga-install-blockers");
    expect(blockers.textContent).toContain("Locale3.9");
  });

});

describe("the fields are filled from the slots (design § 3.4)", () => {
  // The three browse buttons were three questions a person had to answer from
  // memory: which of the forty files in their downloads folder is "the
  // package's own archive", which is "its update archive", which is "the disc
  // the installer checks". `core::osinstall::slots` answers all three from
  // the material folders, and these tests are about what the panel then says
  // — which is the whole content, exactly as everywhere else on this screen.

  /** The bag as it stands, to prove what a click did (or did not) persist. */
  const bag = () =>
    (useSettingsStore.getState().settings.remembered ?? {}) as Record<string, unknown>;

  function withPackageChosen(remembered: Record<string, unknown> = {}) {
    useSettingsStore.setState((state) => ({
      settings: {
        ...state.settings,
        winuaePath: "C:/Program Files/WinUAE/winuae64.exe",
        remembered: {
          "amigaInstall.package": "boingbag-39-1",
          "amigaInstall.kickstart": "D:/roms/kick31.rom",
          ...remembered,
        },
      },
    }));
  }

  function renderPanel(folders: string[] = ["E:/material"]) {
    return render(
      <AmigaInstallPanel
        release="AmigaOS 3.9"
        treeRoot="D:/amiga/os39"
        packageFolder="D:/pkg"
        materialFolders={folders}
      />
    );
  }

  /** The Browse button of one field, by the accessible name `Field` gives it
   *  — the one way to tell "this field is still asking" from "this field is
   *  filled", since both render under the same test id. */
  const browseFor = (label: string) =>
    screen.queryByRole("button", { name: `${i18n.t("common.browse")} ${label}` });

  it("shows a found archive as ART's own sentence about it, and stops asking", async () => {
    withPackageChosen();
    slotsMock.mockResolvedValue(
      slotReport([slotState({ found: foundByHash("E:/material/BoingBag39-1.lha") })])
    );
    renderPanel();

    // The readout's own sentence for that row, verbatim — the panel and the
    // `kaynak` step must not say two different things about one file.
    expect(
      await screen.findByText(
        i18n.t("osinstall.slots.foundByHash", {
          file: "BoingBag39-1.lha",
          name: "BoingBag 3.9-1",
        })
      )
    ).toBeTruthy();
    // The question is gone; the way to change ART's answer remains.
    expect(browseFor(i18n.t("osinstall.amigaInstall.archive.label"))).toBeNull();
    expect(screen.getByTestId("amiga-slot-archive-choose-another")).toBeTruthy();
    // And it is the file the run gets.
    await waitFor(() =>
      expect(previewMock).toHaveBeenCalledWith(
        expect.objectContaining({ packageArchive: "E:/material/BoingBag39-1.lha" }),
        expect.anything()
      )
    );
  });

  it("asks for an archive it could not find by the names it expects, and says so where the button is", async () => {
    withPackageChosen();
    slotsMock.mockResolvedValue(slotReport([slotState()]));
    renderPanel();

    // The hint is no longer a sentence about what the field is *for* — the
    // one thing a person reading the label already knows. It is what ART
    // actually expects, from the recipes' own data.
    expect(
      await screen.findByText(
        i18n.t("osinstall.amigaInstall.archive.hint", {
          filenames: "BoingBag39-1.lha",
          provenance: "the owner's copy, 2026-09-08",
        })
      )
    ).toBeTruthy();
    expect(browseFor(i18n.t("osinstall.amigaInstall.archive.label"))).toBeTruthy();

    // ART-202's rule: the reason Run is dead is rendered where Run is.
    const blockers = await screen.findByTestId("amiga-install-blockers");
    expect(blockers.textContent).toContain(
      i18n.t("osinstall.amigaInstall.blocker.slotMissing", {
        name: "BoingBag 3.9-1",
        filenames: "BoingBag39-1.lha",
      })
    );
    expect(
      screen.getByRole("button", { name: i18n.t("osinstall.amigaInstall.run") }).hasAttribute("disabled")
    ).toBe(true);
    expect(previewMock).not.toHaveBeenCalled();
  });

  it("keeps the file the user chose over the one ART found, and a re-scan does not take it back", async () => {
    // CLAUDE.md, "nothing changes unless the user changes it": a found
    // artefact pre-fills a field, a chosen one overrides, and a re-scan never
    // replaces a choice. The override is the remembered per-package key
    // ART-277 already gave this screen.
    withPackageChosen({ "amigaInstall.archive.boingbag-39-1": "D:/pkg/my-own-copy.lha" });
    slotsMock.mockResolvedValue(
      slotReport([slotState({ found: foundByHash("E:/material/BoingBag39-1.lha") })])
    );
    const view = renderPanel();

    expect(
      await screen.findByText(
        i18n.t("osinstall.slots.chosen", { file: "my-own-copy.lha", name: "BoingBag 3.9-1" })
      )
    ).toBeTruthy();
    await waitFor(() =>
      expect(previewMock).toHaveBeenCalledWith(
        expect.objectContaining({ packageArchive: "D:/pkg/my-own-copy.lha" }),
        expect.anything()
      )
    );

    // The re-scan: another folder added, and ART now resolves the slot to a
    // different file. Nothing the user did.
    slotsMock.mockResolvedValue(
      slotReport([slotState({ found: foundByHash("F:/more/BoingBag39-1.lha") })])
    );
    view.rerender(
      <AmigaInstallPanel
        release="AmigaOS 3.9"
        treeRoot="D:/amiga/os39"
        packageFolder="D:/pkg"
        materialFolders={["E:/material", "F:/more"]}
      />
    );
    await waitFor(() => expect(slotsMock).toHaveBeenCalledTimes(2));

    expect(
      screen.getByText(
        i18n.t("osinstall.slots.chosen", { file: "my-own-copy.lha", name: "BoingBag 3.9-1" })
      )
    ).toBeTruthy();
    expect(
      screen.queryByText(
        i18n.t("osinstall.slots.foundByHash", {
          file: "BoingBag39-1.lha",
          name: "BoingBag 3.9-1",
        })
      )
    ).toBeNull();
    for (const call of previewMock.mock.calls) {
      expect(call[0].packageArchive).toEqual("D:/pkg/my-own-copy.lha");
    }
  });

  it("gives the field back to ART when the user asks for the one ART found", async () => {
    withPackageChosen({ "amigaInstall.archive.boingbag-39-1": "D:/pkg/my-own-copy.lha" });
    slotsMock.mockResolvedValue(
      slotReport([slotState({ found: foundByHash("E:/material/BoingBag39-1.lha") })])
    );
    renderPanel();

    await screen.findByTestId("amiga-slot-archive-use-found");
    await userEvent.setup().click(screen.getByTestId("amiga-slot-archive-use-found"));

    // `forget`, not "store null": a stored `null` is itself a decision, and
    // the found file could then never fill the field again.
    await waitFor(() =>
      expect("amigaInstall.archive.boingbag-39-1" in bag()).toBe(false)
    );
    expect(
      await screen.findByText(
        i18n.t("osinstall.slots.foundByHash", {
          file: "BoingBag39-1.lha",
          name: "BoingBag 3.9-1",
        })
      )
    ).toBeTruthy();
  });

  /// **Fix round 1, m5.** A slot found by hash whose file the user then
  /// deletes: the read-only line above still says *"identified by its
  /// bytes"* (nothing re-scanned), and the blocker under it used to say *"ART
  /// checked the file you chose"* about a file the user never chose.
  it("says ART's own archive has gone, not that a file the user chose has", async () => {
    withPackageChosen();
    slotsMock.mockResolvedValue(
      slotReport([slotState({ found: foundByHash("E:/material/BoingBag39-1.lha") })])
    );
    previewMock.mockResolvedValue(
      preview({
        packageArchive: "E:/material/BoingBag39-1.lha",
        packageArchivePresent: false,
      })
    );
    renderPanel();

    const blockers = await screen.findByTestId("amiga-install-blockers");
    expect(blockers.textContent).toContain(
      i18n.t("osinstall.amigaInstall.blocker.archiveMissingFound", {
        count: 1,
        path: "E:/material/BoingBag39-1.lha",
      })
    );
    // And the sentence about a choice nobody made is gone.
    expect(blockers.textContent).not.toContain(
      i18n.t("osinstall.amigaInstall.blocker.archiveMissing", { count: 1 })
    );
  });

  it("keeps 'the file you chose' for a file the user really did choose", async () => {
    // The other arm. A blanket rename would satisfy the test above and lie
    // here instead.
    withPackageChosen({ "amigaInstall.archive.boingbag-39-1": "D:/pkg/my-own-copy.lha" });
    slotsMock.mockResolvedValue(
      slotReport([slotState({ found: foundByHash("E:/material/BoingBag39-1.lha") })])
    );
    previewMock.mockResolvedValue(
      preview({ packageArchive: "D:/pkg/my-own-copy.lha", packageArchivePresent: false })
    );
    renderPanel();

    const blockers = await screen.findByTestId("amiga-install-blockers");
    expect(blockers.textContent).toContain(
      i18n.t("osinstall.amigaInstall.blocker.archiveMissing", { count: 1 })
    );
  });

  /// **Fix round 1, m6.** `osinstall_slots` refuses a chosen tree that
  /// carries no `distribution.json`, so a panel with no slot answer is not
  /// only the exotic case — and the fields used to lose their hint entirely,
  /// taking the explanation with it. "ART has no note" is what is true.
  it("still says what it expects when no slot answer arrived at all", async () => {
    withPackageChosen();
    slotsMock.mockRejectedValue(new Error("that tree carries no distribution.json"));
    renderPanel();

    const hint = i18n.t("osinstall.amigaInstall.archive.hint", {
      filenames: i18n.t("osinstall.slots.filenamesUnknown"),
      provenance: i18n.t("osinstall.slots.provenanceUnknown"),
    });
    expect(await screen.findByText(hint)).toBeTruthy();
    // The browse row is still there — a panel usable with a hand-picked
    // archive is the state ART-212 ruled must keep working.
    expect(browseFor(i18n.t("osinstall.amigaInstall.archive.label"))).toBeTruthy();
  });

  /// **Fix round 1, L9.** With every material folder layer-tagged (AmigaOS
  /// 3.2.2, each folder said to hold a part of the release) `packages.folder`
  /// derives to `null`, and the panel printed "choose the update packages
  /// folder above" and offered no radio — while its own slots had just found
  /// everything in those same folders.
  it("reads the catalogue from the material list when no archives folder is set", async () => {
    withPackageChosen();
    render(
      <AmigaInstallPanel
        release="AmigaOS 3.9"
        treeRoot="D:/amiga/os39"
        packageFolder={null}
        materialFolders={["E:/tagged-base", "E:/tagged-update"]}
      />
    );

    await waitFor(() =>
      expect(packagesMock).toHaveBeenCalledWith("E:/tagged-base", "AmigaOS 3.9")
    );
    // And the radio is really offered, which is the half the user sees.
    expect(await screen.findAllByTestId("amiga-package-row")).toHaveLength(2);
    expect(
      screen.queryByText(i18n.t("osinstall.amigaInstall.package.needsFolder"))
    ).toBeNull();
  });

  it("lists every candidate of an ambiguous slot, and picking one becomes the choice", async () => {
    // ART does not choose between two of somebody's files (design § 4). The
    // paths are the whole of the information — the commonest shape here is
    // two copies under the *same* name — and each carries its own evidence.
    withPackageChosen();
    slotsMock.mockResolvedValue(
      slotReport([
        slotState({
          candidates: [
            candidate("E:/material/BoingBag39-1.lha"),
            candidate("F:/more/BoingBag39-1.lha"),
          ],
        }),
      ])
    );
    renderPanel(["E:/material", "F:/more"]);

    const rows = await screen.findAllByTestId("amiga-slot-archive-candidate");
    expect(rows).toHaveLength(2);
    expect(rows[0].textContent).toContain("E:/material/BoingBag39-1.lha");
    expect(rows[1].textContent).toContain("F:/more/BoingBag39-1.lha");
    // Each one's own sentence, not one "these might be it" over both.
    expect(rows[1].textContent).toContain(
      i18n.t("osinstall.slots.guessedByFilenameUnread", {
        file: "BoingBag39-1.lha",
        name: "BoingBag 3.9-1",
        other: "",
      })
    );

    await userEvent.setup().click(rows[1].querySelector("input") as HTMLInputElement);

    await waitFor(() =>
      expect(bag()["amigaInstall.archive.boingbag-39-1"]).toBe("F:/more/BoingBag39-1.lha")
    );
    expect(
      await screen.findByText(
        i18n.t("osinstall.slots.chosen", {
          file: "BoingBag39-1.lha",
          name: "BoingBag 3.9-1",
        })
      )
    ).toBeTruthy();
  });

  /// **Round 5, task 2 (spec § 5, spec § 3.4).** This Browse is the panel's
  /// one dialog that reaches a folder full of the user's archives, and until
  /// this round the folder it reached went nowhere: the file became the
  /// per-package override and the folder was forgotten. `packages.folder` —
  /// the key the retired `PackagePanel`'s own Browse used to write — is read
  /// only now, so the folder goes where every other folder in this build
  /// goes and where the slots are actually resolved from: the material list.
  it("adds the folder of an archive chosen by hand to the material list", async () => {
    const { open } = await import("@tauri-apps/plugin-dialog");
    vi.mocked(open).mockResolvedValue("F:/downloads/BoingBag39-1.lha");
    withPackageChosen();
    slotsMock.mockResolvedValue(slotReport([slotState()]));
    renderPanel();

    await screen.findByTestId("amiga-slot-archive");
    await userEvent
      .setup()
      .click(browseFor(i18n.t("osinstall.amigaInstall.archive.label")) as HTMLElement);

    await waitFor(() =>
      expect(bag()["buildSession.material.AmigaOS 3.9"]).toEqual({
        folders: [
          { path: "E:/material", layer: null },
          { path: "F:/downloads", layer: null },
        ],
      })
    );
    // **And not into `packages`**, which is what spec § 5 turns into a rule:
    // the seeded object is exactly as this test seeded it, folder included.
    expect(bag()["buildSession.packages.AmigaOS 3.9"]).toEqual({
      folder: "D:/pkg",
      chosen: [],
    });
    // The file itself is still the choice it always was.
    expect(bag()["amigaInstall.archive.boingbag-39-1"]).toBe("F:/downloads/BoingBag39-1.lha");
  });

  /// The other half of that move, and the reason the panel keeps a
  /// `useState` for it: `addMaterialFolder` **appends**, so the folder the
  /// person just pointed at is the list's last entry, while the catalogue
  /// reads `packages.folder ?? materialFolders[0]` — the folder they picked
  /// would be the one folder the catalogue does not ask about.
  it("asks the catalogue about the folder just browsed to, not the list's first", async () => {
    const { open } = await import("@tauri-apps/plugin-dialog");
    vi.mocked(open).mockResolvedValue("F:/downloads/BoingBag39-1.lha");
    withPackageChosen();
    slotsMock.mockResolvedValue(slotReport([slotState()]));
    renderPanel();

    // The control: before the click it is the seeded archives folder.
    await waitFor(() => expect(packagesMock).toHaveBeenCalledWith("D:/pkg", "AmigaOS 3.9"));
    await screen.findByTestId("amiga-slot-archive");
    await userEvent
      .setup()
      .click(browseFor(i18n.t("osinstall.amigaInstall.archive.label")) as HTMLElement);

    await waitFor(() =>
      expect(packagesMock).toHaveBeenLastCalledWith("F:/downloads", "AmigaOS 3.9")
    );
  });
});

// ---------------------------------------------------------------------------
// The chain is the list (round 3, task 2)
//
// Every assertion below is about *which sentence a person reads* and *which
// row the one Run button is about* — the two things this screen can get
// confidently wrong. A test that only counted rows would pass just as
// happily if every row said the same thing.
// ---------------------------------------------------------------------------

describe("the chain", () => {
  /**
   * The catalogue as `osinstall_packages` really answers for AmigaOS 3.9:
   * every package of the release, host-placeable ones included, with
   * `amigaInstallable` marking the two that are not.
   *
   * The three-package fixture above is this screen's *old* question — which
   * packages can be run on the Amiga. The chain asks about all of them, and
   * a catalogue missing a chain row's package would have ART-212's
   * sanitiser drop the user's own selection the moment they made it.
   */
  const CHAIN_PACKAGES: PackageSummary[] = [
    ...PACKAGES,
    ...["locale-39", "boingbag-39-2-contribution", "euro-update", "boingbags-39-3-4"].map(
      (id) => ({
        id,
        name: id,
        requires: [],
        requiresComponents: [],
        available: true,
        hostPlacementBlock: null,
        amigaInstallable: id === "boingbags-39-3-4",
        refusedNames: [],
        notYetRunnable: null,
      })
    ),
  ];

  /**
   * The archive one row needs, remembered under **that row's own key**
   * (ART-277). Deliberately not `withChoices`, which also pre-selects
   * BoingBag 3.9-1: half of what is under test here is what the screen does
   * when the user has selected *nothing*.
   */
  function withArchiveFor(id: string, file: string) {
    useSettingsStore.setState((state) => ({
      settings: {
        ...state.settings,
        winuaePath: "C:/Program Files/WinUAE/winuae64.exe",
        remembered: {
          ...(state.settings.remembered as Record<string, unknown>),
          [`amigaInstall.archive.${id}`]: file,
          "amigaInstall.kickstart": "D:/roms/kick31.rom",
        },
      },
    }));
  }

  beforeEach(() => {
    packagesMock.mockResolvedValue(CHAIN_PACKAGES);
  });

  function renderChain(rows: ChainRow[] = THE_CHAIN(), installed = 2) {
    chainMock.mockResolvedValue(chainReport(rows, installed));
    return render(
      <AmigaInstallPanel
        release="AmigaOS 3.9"
        treeRoot="D:/amiga/os39"
        packageFolder="D:/pkg"
        materialFolders={["E:/material"]}
      />
    );
  }

  /** The radio of one row, by position in the list. */
  const radioAt = (index: number) =>
    screen.getAllByTestId("amiga-chain-row")[index].querySelector("input") as HTMLInputElement;

  /** The radio of one row, by the row's own name. */
  function radioFor(name: string): HTMLInputElement {
    const row = screen
      .getAllByTestId("amiga-chain-row")
      .find((element) => (element.textContent ?? "").includes(name));
    if (!row) throw new Error(`no chain row named ${name}`);
    return row.querySelector("input") as HTMLInputElement;
  }

  /** The one Run button, whichever of its three labels it is wearing — the
   *  Amiga-side one, the packages one, or (fix round 1, F8) the neutral one
   *  a row that opens no emulator and places no file gets. */
  const runButton = () =>
    screen.getByRole("button", {
      name: new RegExp(
        [
          i18n.t("osinstall.amigaInstall.run"),
          i18n.t("osinstall.packages.apply.run"),
          i18n.t("osinstall.chain.runRow"),
        ]
          .map((label) => label.replace(/[.*+?^${}()|[\]\\]/g, "\\$&"))
          .join("|")
      ),
    });

  it("renders one row per link, in the material's own order and in the state's own words", async () => {
    renderChain();
    const rows = await screen.findAllByTestId("amiga-chain-row");
    expect(rows).toHaveLength(8);

    // The order is the material's, and the two rank-4 rows are two rows
    // sharing one number — never renumbered 1..8, which would invent an
    // order the material does not state.
    const text = rows.map((row) => row.textContent ?? "");
    expect(text.map((line) => line.trim().slice(0, 1))).toEqual([
      "1",
      "2",
      "3",
      "4",
      "4",
      "6",
      "7",
      "8",
    ]);

    // Each state's own sentence, and seven different ones.
    expect(text[0]).toContain(i18n.t("osinstall.chain.installed", { name: "AmigaOS3.9" }));
    expect(text[2]).toContain(
      i18n.t("osinstall.chain.ready", { name: "BoingBag 3.9-2", file: "BoingBag39-2.lha" })
    );
    expect(text[4]).toContain(
      i18n.t("osinstall.chain.blocked", {
        name: "Türkçe catalogs (BoingBag 3.9-2)",
        needs: "BoingBag 3.9-2",
      })
    );
    expect(text[5]).toContain(
      i18n.t("osinstall.chain.missing", {
        name: "BoingBag 3.9-2 Contribution",
        filenames: "BoingBag39-2-Contribution.lha",
      })
    );
    expect(text[6]).toContain(
      i18n.t("osinstall.chain.notNeeded", {
        name: "Euro-Update",
        supersededBy: "BoingBags 3&4 for AmigaOS 3.9",
      })
    );
    expect(text[7]).toContain(
      i18n.t("osinstall.chain.notYetRunnable.installerNotMeasured", {
        name: "BoingBags 3&4 for AmigaOS 3.9",
      })
    );
    // Where each row happens, beside its state and not folded into it.
    expect(text[2]).toContain(i18n.t("osinstall.chain.onAmiga"));
    expect(text[3]).toContain(i18n.t("osinstall.chain.onWindows"));

    // The set line above them, and the tree it is about — a fraction with no
    // tree named is true of nothing in particular.
    expect(screen.getByTestId("amiga-chain").textContent).toContain(
      i18n.t("osinstall.chain.setLine", {
        release: "AmigaOS 3.9",
        installed: 2,
        total: 8,
      })
    );
    expect(screen.getByTestId("amiga-chain-tree").textContent).toBe(
      i18n.t("osinstall.chain.tree", { root: "D:/amiga/os39" })
    );

    // The chain *is* the list: the package radio it replaced is gone.
    expect(screen.queryAllByTestId("amiga-package-row")).toHaveLength(0);
  });

  it("renders the same eight rows in Turkish", async () => {
    await act(async () => {
      await i18n.changeLanguage("tr");
    });
    try {
      renderChain();
      const rows = await screen.findAllByTestId("amiga-chain-row");
      expect(rows).toHaveLength(8);
      expect(rows[4].textContent).toContain(
        i18n.t("osinstall.chain.blocked", {
          name: "Türkçe catalogs (BoingBag 3.9-2)",
          needs: "BoingBag 3.9-2",
        })
      );
      // …and it is really Turkish, not the English catalogue answering.
      expect(rows[4].textContent).not.toContain("has to go on first");
    } finally {
      await act(async () => {
        await i18n.changeLanguage("en");
      });
    }
  });

  it("selects the package when a row is selected", async () => {
    // BoingBag 3.9-1 is **installed** — deliberately not the row the button
    // already points at. Selecting it has to make the whole panel about it
    // (its own archive is the one ART asks about) *and* leave Run refusing
    // it in the row's own words.
    withArchiveFor("boingbag-39-1", "D:/pkg/BoingBag39-1.lha");
    renderChain();
    await screen.findAllByTestId("amiga-chain-row");
    classifyMock.mockClear();

    await userEvent.setup().click(radioFor("BoingBag 3.9-1"));

    await waitFor(() =>
      expect(classifyMock).toHaveBeenCalledWith(
        "D:/pkg/BoingBag39-1.lha",
        "boingbag-39-1",
        "AmigaOS 3.9"
      )
    );
    expect(radioFor("BoingBag 3.9-1").checked).toBe(true);
    expect(screen.getByTestId("amiga-chain-cannot-run").textContent).toBe(
      i18n.t("osinstall.chain.installed", { name: "BoingBag 3.9-1" })
    );
    // A row already in the tree is never composed: no emulator run plan is
    // asked for on the strength of a selection alone.
    expect(previewMock).not.toHaveBeenCalled();
  });

  it("targets the first ready row when nothing is selected, and says which", async () => {
    withArchiveFor("boingbag-39-2", "D:/pkg/BoingBag39-2.lha");
    renderChain();
    await screen.findAllByTestId("amiga-chain-row");

    // Named beside the button: a Run button that does not say which row it
    // will run is the confident action this screen exists to prevent.
    expect(screen.getByTestId("amiga-chain-next").textContent).toBe(
      i18n.t("osinstall.chain.next", { name: "BoingBag 3.9-2" })
    );
    // …and it is the row the request is actually about.
    await waitFor(() =>
      expect(previewMock.mock.calls.at(-1)?.[0]).toMatchObject({
        packageId: "boingbag-39-2",
      })
    );

    await screen.findByTestId("amiga-install-preview");
    const user = userEvent.setup();
    await user.click(screen.getByLabelText(i18n.t("osinstall.amigaInstall.confirm")));
    await user.click(runButton());
    await waitFor(() => expect(runMock).toHaveBeenCalled());
    expect(runMock.mock.calls[0][0]).toMatchObject({ packageId: "boingbag-39-2" });
  });

  it("runs a host-placed row through the packages path, not through an emulator", async () => {
    slotsMock.mockResolvedValue(
      slotReport([
        slotState({
          slot: { id: "package:locale-39", name: "AmigaOS 3.9 Locale update" },
          // Found in a material folder that is *not* the archives folder —
          // which folder `add_package` is given has to come from the find.
          found: foundByHash("E:/material/locale/Locale3_9.lha"),
        }),
      ])
    );
    renderChain();
    await screen.findAllByTestId("amiga-chain-row");
    const user = userEvent.setup();
    await user.click(radioFor("AmigaOS 3.9 Locale update"));

    await waitFor(() =>
      // The fourth argument is the user's own per-slot choices (ART-288):
      // this path resolves a package's archive the way the chain row does,
      // and an empty list here is "the user has chosen nothing by hand",
      // not "ignore what they chose".
      expect(collisionsMock).toHaveBeenCalledWith(
        "D:/amiga/os39",
        "E:/material/locale",
        ["locale-39"],
        []
      )
    );
    // No emulator anywhere near this row: no window warning, no emulator
    // confirmation, and no `compose`.
    expect(screen.queryByTestId("emulator-window-warning")).toBeNull();
    expect(screen.queryByText(i18n.t("osinstall.amigaInstall.confirm"))).toBeNull();
    expect(runMock).not.toHaveBeenCalled();

    await user.click(screen.getByLabelText(i18n.t("osinstall.packages.confirm", { count: 1 })));
    await user.click(runButton());
    await waitFor(() =>
      expect(addPackageMock).toHaveBeenCalledWith(
        "D:/amiga/os39",
        "E:/material/locale",
        ["locale-39"],
        []
      )
    );

    // Its ending is the `paketler` step's own, and it names the row.
    await act(async () => {
      place!({
        job_id: 11,
        outcome: {
          root: "D:/amiga/os39",
          files: 12,
          directories: 3,
          bytes: 4096,
          removed: [],
          icons: [],
          iconMergeFailures: 0,
          extraMembers: [],
          postPlace: [],
        },
      });
      await Promise.resolve();
    });
    expect((await screen.findByTestId("host-placement-outcome")).textContent).toContain(
      i18n.t("osinstall.packages.apply.done")
    );
    expect(screen.getByTestId("amiga-placement-report-row").textContent).toBe(
      i18n.t("osinstall.chain.reportRow", { name: "AmigaOS 3.9 Locale update" })
    );
  });

  /**
   * **ART-287.** `docs/assets/chain.png`, the 11:2x build: the preview
   * finished and **refused** — the owner's folder holds two BoingBag 3.9-1
   * builds — and the heading above the red box still read *"Checking what
   * this would replace…"*. The screen said it was working directly over a
   * finished failure. Checking, done and refused are three states and a
   * person does three different things about them, so the heading has three
   * answers now.
   *
   * Both halves: the refusal's own heading appears, and the checking sentence
   * is gone. Asserting only the first would pass for a screen showing both at
   * once, which is exactly the frame this is about.
   */
  it("stops saying it is checking once the preview has refused", async () => {
    collisionsMock.mockRejectedValue(
      "invalid input: more than one archive carries 'BoingBag3.9-1', the media " +
        "'boingbag-39-1' needs: E:/os39/BoingBag39-1 (1).lha, E:/os39/BoingBag39-1.lha " +
        "(ART-INPUT-INVALID)"
    );
    renderChain();
    await screen.findAllByTestId("amiga-chain-row");
    await userEvent.setup().click(radioFor("AmigaOS 3.9 Locale update"));

    // The refusal itself, verbatim from Rust (ART-060).
    const box = await screen.findByTestId("host-placement-preview-error");
    expect(box.textContent).toContain("more than one archive carries");

    const block = screen.getByTestId("amiga-chain-placement");
    expect(block.textContent).toContain(i18n.t("osinstall.packages.preview.refused"));
    expect(block.textContent).not.toContain(i18n.t("osinstall.packages.preview.loading"));
  });

  it("asks the chain again after a run, and offers the next ready row", async () => {
    withArchiveFor("boingbag-39-2", "D:/pkg/BoingBag39-2.lha");
    renderChain();
    await screen.findAllByTestId("amiga-chain-row");
    const asked = chainMock.mock.calls.length;

    // BoingBag 3.9-2 has gone on: the row is installed and the next ready
    // one is the Locale update.
    const after = THE_CHAIN();
    after[2] = { ...after[2], state: { state: "installed", when: null } };
    chainMock.mockResolvedValue(chainReport(after, 3));

    await screen.findByTestId("amiga-install-preview");
    const user = userEvent.setup();
    await user.click(screen.getByLabelText(i18n.t("osinstall.amigaInstall.confirm")));
    await user.click(runButton());
    await waitFor(() => expect(runMock).toHaveBeenCalled());
    await act(async () => {
      deliver!({
        job_id: 7,
        outcome: { kind: "succeeded" },
        settlement: { kind: "promoted", tree: "D:/amiga/os39", leftBehind: null },
      });
      await Promise.resolve();
    });

    await waitFor(() => expect(chainMock.mock.calls.length).toBeGreaterThan(asked));
    // The row moved, and the button moved with it.
    await waitFor(() =>
      expect(screen.getByTestId("amiga-chain-next").textContent).toBe(
        i18n.t("osinstall.chain.next", { name: "AmigaOS 3.9 Locale update" })
      )
    );
    // …and the report of the run that just finished is still on screen, and
    // still says which row it was about. A report wiped by the button moving
    // on is the screen saying nothing about work it did.
    expect(screen.getByTestId("amiga-install-report-row").textContent).toBe(
      i18n.t("osinstall.chain.reportRow", { name: "BoingBag 3.9-2" })
    );
  });

  /**
   * **ART-286, both halves.** `docs/assets/chain.png`, the 10:21 build: the
   * panel opened on a remembered selection sitting on an *already-installed*
   * host-placed row and showed *"Checking what this would replace…"* — polled
   * 4 min 40 s, nothing clicked, nothing in flight. `useHostPlacement`'s own
   * `enabled` gate had correctly declined to call `osinstall_collisions` for a
   * non-ready row; the panel's `hostSideRow` render gate had not, so the block
   * rendered a progress sentence for a request nobody made and
   * `placement.collisions` stayed `null` for ever.
   *
   * Two arms, because the fix is two changes and each needs its own:
   *
   * 1. the **restored** selection is no longer the Run target when it cannot
   *    run, so the block is about the first ready row (or about nothing);
   * 2. and a row the user **clicks** is still the target — round 3's rule —
   *    so the render gate itself has to carry `targetReady`. This is the arm
   *    that fails on the old `hostSideRow`.
   */
  function chainWithInstalledLocale(): ChainRow[] {
    const rows = THE_CHAIN();
    // `locale-39` is the host-placed row of this fixture (`runsOnAmiga:
    // false`); make it already in the tree, which is the screenshot's state.
    rows[3] = { ...rows[3], state: { state: "installed", when: null } };
    return rows;
  }

  function withRememberedRow(id: string) {
    useSettingsStore.setState((state) => ({
      settings: {
        ...state.settings,
        remembered: {
          ...(state.settings.remembered as Record<string, unknown>),
          "amigaInstall.package": id,
        },
      },
    }));
  }

  it("never leaves 'Checking what this would replace' on a row nothing was asked about", async () => {
    withRememberedRow("locale-39");
    renderChain(chainWithInstalledLocale());
    await screen.findAllByTestId("amiga-chain-row");

    // Arm 1 — the remembered row is installed, so it is not the target.
    expect(screen.queryByTestId("amiga-chain-placement")).toBeNull();
    expect(
      screen.queryByText(i18n.t("osinstall.packages.preview.loading"))
    ).toBeNull();
    expect(
      screen.queryByLabelText(i18n.t("osinstall.packages.confirm", { count: 1 }))
    ).toBeNull();

    // Arm 2 — a host-placed row the user **clicks**, which round 3 allows:
    // selecting is reading, and a clicked row *is* the target whatever state
    // it is in. Türkçe catalogs is blocked and host-placed, so `enabled` is
    // false and nothing is asked — the render gate has to agree.
    //
    // A different row from arm 1's on purpose: clicking the row that is
    // already selected fires no change event, so it would prove nothing.
    await userEvent.setup().click(radioFor("Türkçe catalogs"));
    await waitFor(() => expect(radioFor("Türkçe catalogs").checked).toBe(true));
    expect(screen.queryByTestId("amiga-chain-placement")).toBeNull();
    expect(
      screen.queryByText(i18n.t("osinstall.packages.preview.loading"))
    ).toBeNull();
    // The fetch gate and the render gate now say the same thing: nothing was
    // requested, so nothing claims to be waiting for an answer.
    expect(collisionsMock).not.toHaveBeenCalled();
    // …and the row's own sentence is still what says why, beside the button.
    expect(screen.getByTestId("amiga-chain-cannot-run").textContent).toBe(
      i18n.t("osinstall.chain.blocked", {
        name: "Türkçe catalogs (BoingBag 3.9-2)",
        needs: "BoingBag 3.9-2",
      })
    );
  });

  it("never captions the button with an action on a package already in the tree", async () => {
    // **ART-286's other half.** With the remembered row targeted, the one Run
    // button read *"Add the chosen packages"* directly over *"…already in this
    // tree"* — a caption naming work on a package that needs none. A restored
    // selection is a default, not a decision: the target falls back to the
    // first ready row, exactly as no selection at all does.
    withRememberedRow("locale-39");
    renderChain(chainWithInstalledLocale());
    await screen.findAllByTestId("amiga-chain-row");

    expect(runButton().textContent).not.toBe(i18n.t("osinstall.packages.apply.run"));
    // And it says which row it *is* about, which is round 3's own rule for a
    // button whose row is not the one selected on screen.
    expect(screen.getByTestId("amiga-chain-next").textContent).toBe(
      i18n.t("osinstall.chain.next", { name: "BoingBag 3.9-2" })
    );
    // The remembered row keeps its selection and its own sentence — nothing
    // changes unless the user changes it; only the *button* moved.
    expect(radioFor("AmigaOS 3.9 Locale update").checked).toBe(true);
    expect(screen.getByTestId("amiga-chain-cannot-run").textContent).toBe(
      i18n.t("osinstall.chain.installed", { name: "AmigaOS 3.9 Locale update" })
    );
  });

  it("refuses to run a blocked row, in the row's own words", async () => {
    // **The mutation this case is written against.** Make Run ignore
    // `BlockedBy` — drop `targetReady` from `canRun` — and this fails: the
    // Turkish catalogue pack would be offered on a tree BoingBag 3.9-2 has
    // never touched, which is the one thing ART-186 exists to prevent.
    renderChain();
    await screen.findAllByTestId("amiga-chain-row");
    await userEvent.setup().click(radioFor("Türkçe catalogs"));

    expect(runButton().hasAttribute("disabled")).toBe(true);
    expect(screen.getByTestId("amiga-chain-cannot-run").textContent).toBe(
      i18n.t("osinstall.chain.blocked", {
        name: "Türkçe catalogs (BoingBag 3.9-2)",
        needs: "BoingBag 3.9-2",
      })
    );
    // Nothing was asked of either route about a row that cannot run.
    expect(collisionsMock).not.toHaveBeenCalled();
    expect(runMock).not.toHaveBeenCalled();
  });

  it("refuses to run a row nobody has measured, and says what is unmeasured", async () => {
    renderChain();
    await screen.findAllByTestId("amiga-chain-row");
    // By position, not by name: Euro-Update's own sentence names BoingBags
    // 3&4 as the package that supersedes it, so a name match would find the
    // wrong row — and finding the wrong row is what this file is about.
    await userEvent.setup().click(radioAt(7));

    expect(runButton().hasAttribute("disabled")).toBe(true);
    expect(screen.getByTestId("amiga-chain-cannot-run").textContent).toBe(
      i18n.t("osinstall.chain.notYetRunnable.installerNotMeasured", {
        name: "BoingBags 3&4 for AmigaOS 3.9",
      })
    );
    // §10: listed rather than hidden — the row is there to be read.
    expect(radioAt(7).checked).toBe(true);
  });

  it("sends the CD row to the top of this tab, where a disc is chosen, and never runs it", async () => {
    renderChain();
    await screen.findAllByTestId("amiga-chain-row");

    const link = screen.getByTestId("amiga-chain-cd-link");
    expect(link.getAttribute("href")).toBe("/os-builder/dosyalar");
    expect(link.textContent).toBe(i18n.t("osinstall.chain.cdLink"));

    await userEvent.setup().click(radioFor("AmigaOS3.9"));
    expect(runButton().hasAttribute("disabled")).toBe(true);
  });

  // **The CD row's own sentence** (round 3 whole-branch review, M3). A tree
  // whose manifest names no such volume answers `missing`, and "not in the
  // folders you named" would be false with the ISO sitting right there — the
  // row says what is actually true and the link beside it is the action.
  it("tells a tree that was not built from the disc so, rather than blaming the folders", async () => {
    const rows = THE_CHAIN();
    rows[0] = { ...rows[0], state: { state: "missing", expected: ["AmigaOS39.iso"] } };
    renderChain(rows, 1);
    const row = (await screen.findAllByTestId("amiga-chain-row"))[0];

    expect(row.textContent).toContain(
      i18n.t("osinstall.chain.mediumNotBuiltFrom", { name: "AmigaOS3.9" })
    );
    expect(row.textContent).not.toContain(i18n.t("osinstall.chain.missingUnnamed", { name: "AmigaOS3.9" }));
    // And the link is still there, because it is the row's only action.
    expect(screen.getByTestId("amiga-chain-cd-link").getAttribute("href")).toBe(
      "/os-builder/dosyalar"
    );
  });

  // **A row blocked on a component names the component the way the
  // components step names it** (M4). `@/lib/chain` carries the key and never
  // renders; the screen translates it here.
  it("names the component a row is waiting on, translated, not by its id", async () => {
    const rows = THE_CHAIN();
    rows[3] = {
      ...rows[3],
      state: {
        state: "blocked-by-component",
        components: [{ id: "locale-base", labelKey: "osinstall.components.name.os39.locale" }],
      },
    };
    renderChain(rows, 2);
    const row = (await screen.findAllByTestId("amiga-chain-row"))[3];

    expect(row.textContent).toContain(
      i18n.t("osinstall.chain.blockedComponent", {
        name: "AmigaOS 3.9 Locale update",
        components: i18n.t("osinstall.components.name.os39.locale"),
      })
    );
    // Not the id, and not the row-blocking sentence — this is a different
    // next step and must not read like "do row 4 first".
    expect(row.textContent).not.toContain("locale-base");
  });

  it("says which tree the count is about, and says so when there is none", async () => {
    renderChain();
    await screen.findAllByTestId("amiga-chain-row");
    expect(screen.getByTestId("amiga-chain-tree").textContent).toBe(
      i18n.t("osinstall.chain.tree", { root: "D:/amiga/os39" })
    );

    cleanup();
    chainMock.mockResolvedValue(chainReport(THE_CHAIN(), 0));
    render(
      <AmigaInstallPanel
        release="AmigaOS 3.9"
        treeRoot={null}
        packageFolder="D:/pkg"
        materialFolders={["E:/material"]}
      />
    );
    await screen.findAllByTestId("amiga-chain-row");
    expect(screen.getByTestId("amiga-chain-tree").textContent).toBe(
      i18n.t("osinstall.chain.treeNone")
    );
  });

  // -------------------------------------------------------------------------
  // Fix round 1 — the sentences the first pass left the button and two rows
  // without.
  // -------------------------------------------------------------------------

  it("says why nothing can run, and what the first row still needs", async () => {
    // **F1.** No row is ready and the user has selected none, which is the
    // state of a fresh tree with only the CD in it. Both of the branches
    // beside the button are false there, and what was left was a dead
    // button with nothing next to it — ART-202's own defect, on this exact
    // screen. The mutation this case is written against is putting that
    // silence back.
    const rows = THE_CHAIN().map((row) =>
      row.state.state === "ready"
        ? { ...row, state: { state: "missing" as const, expected: [`${row.name}.lha`] } }
        : row
    );
    renderChain(rows, 2);
    await screen.findAllByTestId("amiga-chain-row");

    const said = screen.getByTestId("amiga-chain-none-ready").textContent ?? "";
    expect(said).toContain(i18n.t("osinstall.chain.noneReady"));
    // …and it carries the first outstanding row's own words, so it says what
    // is actually needed rather than only that something is.
    expect(said).toContain(
      i18n.t("osinstall.chain.missing", {
        name: "BoingBag 3.9-2",
        filenames: "BoingBag 3.9-2.lha",
      })
    );
    // The other ending is not on screen: "nothing can run yet" and
    // "everything is done" are two sentences and two next steps.
    expect(said).not.toContain(i18n.t("osinstall.chain.allApplied"));
    expect(runButton().hasAttribute("disabled")).toBe(true);
  });

  it("says the chain is finished when every row is accounted for", async () => {
    // **F1, the other cause.** Installed *or* not needed — a chain whose one
    // remaining row is `not needed` is finished, and `installed === total`
    // would call that "nothing can run yet" about a tree that has
    // everything.
    const rows = THE_CHAIN().map((row) =>
      row.state.state === "not-needed" ? row : { ...row, state: { state: "installed" as const, when: null } }
    );
    renderChain(rows, rows.length - 1);
    await screen.findAllByTestId("amiga-chain-row");

    const said = screen.getByTestId("amiga-chain-none-ready").textContent ?? "";
    expect(said).toBe(i18n.t("osinstall.chain.allApplied"));
    expect(said).not.toContain(i18n.t("osinstall.chain.noneReady"));
    expect(runButton().hasAttribute("disabled")).toBe(true);
  });

  it("names the package on a row ART will not place from Windows", async () => {
    // **F3.** Euro-Update is `refused` for `needs-fixfonts` on every tree
    // that does not already carry BoingBags 3&4 — and ART cannot put those
    // there, so this is the ordinary state of that row rather than a corner
    // of one. It used to render the Packages checklist's own sentence, which
    // begins "This package…" because on that screen it sits under the
    // package's name. In a list of eight rows it named nothing.
    const rows = THE_CHAIN().map((row) =>
      row.packageId === "euro-update"
        ? {
            ...row,
            state: {
              state: "refused" as const,
              reason: { because: "not-placeable" as const, block: "needs-fixfonts" as const },
            },
          }
        : row
    );
    renderChain(rows);
    const list = await screen.findAllByTestId("amiga-chain-row");
    expect(list[6].textContent).toContain(
      i18n.t("osinstall.chain.refusedNotPlaceable.needsFixfonts", { name: "Euro-Update" })
    );
    // The checklist's own nameless wording is not what is on screen.
    expect(list[6].textContent).not.toContain(
      i18n.t("osinstall.packages.blocked.needsFixfonts")
    );

    // …and selecting it puts that same named sentence beside the dead button.
    await userEvent.setup().click(radioAt(6));
    expect(runButton().hasAttribute("disabled")).toBe(true);
    expect(screen.getByTestId("amiga-chain-cannot-run").textContent).toBe(
      i18n.t("osinstall.chain.refusedNotPlaceable.needsFixfonts", { name: "Euro-Update" })
    );
  });

  it("offers the next ready row after running a row the user picked", async () => {
    // **F4.** `selectRow` writes the selection, so after a run of an
    // *explicitly* selected row the selection stayed on the row that had
    // just become installed and no next row was named. Only a run this
    // screen made may move it — a selection the user makes has to survive
    // every other refresh, which the case below asserts.
    withArchiveFor("boingbag-39-2", "D:/pkg/BoingBag39-2.lha");
    renderChain();
    await screen.findAllByTestId("amiga-chain-row");
    const user = userEvent.setup();
    await user.click(radioFor("BoingBag 3.9-2"));

    const after = THE_CHAIN();
    after[2] = { ...after[2], state: { state: "installed", when: null } };
    chainMock.mockResolvedValue(chainReport(after, 3));

    await screen.findByTestId("amiga-install-preview");
    await user.click(screen.getByLabelText(i18n.t("osinstall.amigaInstall.confirm")));
    await user.click(runButton());
    await waitFor(() => expect(runMock).toHaveBeenCalled());
    await act(async () => {
      deliver!({
        job_id: 7,
        outcome: { kind: "succeeded" },
        settlement: { kind: "promoted", tree: "D:/amiga/os39", leftBehind: null },
      });
      await Promise.resolve();
    });

    await waitFor(() =>
      expect(screen.getByTestId("amiga-chain-next").textContent).toBe(
        i18n.t("osinstall.chain.next", { name: "AmigaOS 3.9 Locale update" })
      )
    );
    // The report of the run that just finished survives the move and still
    // names its own row.
    expect(screen.getByTestId("amiga-install-report-row").textContent).toBe(
      i18n.t("osinstall.chain.reportRow", { name: "BoingBag 3.9-2" })
    );
  });

  it("keeps a selection the user made when the chain re-answers on its own", async () => {
    // The other arm of F4, and the one that keeps it honest: a refresh that
    // is not the result of a run this screen made must not move a selection
    // the user made. Without it, "clear the selection when the row becomes
    // installed" would drop a selection every time the material re-resolved.
    chainMock.mockResolvedValue(chainReport(THE_CHAIN(), 2));
    const view = render(
      <AmigaInstallPanel
        release="AmigaOS 3.9"
        treeRoot="D:/amiga/os39"
        packageFolder="D:/pkg"
        materialFolders={["E:/material"]}
      />
    );
    await screen.findAllByTestId("amiga-chain-row");
    await userEvent.setup().click(radioFor("BoingBag 3.9-1"));
    expect(radioFor("BoingBag 3.9-1").checked).toBe(true);
    const asked = chainMock.mock.calls.length;

    // A **real** re-ask, driven the way the screen's own effect drives one —
    // the material list changed — with the row in the same installed state
    // it was already in. Nothing here was a run this screen made.
    view.rerender(
      <AmigaInstallPanel
        release="AmigaOS 3.9"
        treeRoot="D:/amiga/os39"
        packageFolder="D:/pkg"
        materialFolders={["E:/material", "F:/more"]}
      />
    );
    await waitFor(() => expect(chainMock.mock.calls.length).toBeGreaterThan(asked));
    expect(radioFor("BoingBag 3.9-1").checked).toBe(true);
    expect(screen.queryByTestId("amiga-chain-next")).toBeNull();
  });

  it("keeps a selection made after a run that did not succeed", async () => {
    // **Fix round 2, N1.** `pendingAdvance` was armed at the *start* of a
    // run and cleared only when it actually fired — so a run that refused,
    // failed, timed out or was cancelled left it armed with no row having
    // become installed, and the next time the user selected an
    // already-installed row **to read it**, F4's effect fired on that row
    // and silently dropped the selection. "Nothing changes unless the user
    // changes it", broken by the code written to respect it: the second
    // click asked to read one row, not to move on from anything.
    withArchiveFor("boingbag-39-2", "D:/pkg/BoingBag39-2.lha");
    renderChain();
    await screen.findAllByTestId("amiga-chain-row");
    await screen.findByTestId("amiga-install-preview");
    const user = userEvent.setup();
    await user.click(screen.getByLabelText(i18n.t("osinstall.amigaInstall.confirm")));
    await user.click(runButton());
    await waitFor(() => expect(runMock).toHaveBeenCalled());

    // The run ends badly. Nothing was promoted, so no row becomes
    // installed and the chain comes back exactly as it was.
    await act(async () => {
      deliver!({
        job_id: 7,
        outcome: { kind: "failed" },
        settlement: { kind: "kept", copy: "D:/amiga/os39.art-run", original: "D:/amiga/os39" },
      });
      await Promise.resolve();
    });
    expect(await screen.findByTestId("amiga-install-report")).toBeTruthy();

    // Now the user selects an unrelated row that is already installed, to
    // read its facts. It must stay selected.
    await user.click(radioFor("BoingBag 3.9-1"));
    await waitFor(() => expect(radioFor("BoingBag 3.9-1").checked).toBe(true));
    expect(radioFor("BoingBag 3.9-1").checked).toBe(true);
    // …and the button is about that row, not about one ART moved to on its
    // own: "Next: X" is what a cleared selection would have rendered.
    expect(screen.queryByTestId("amiga-chain-next")).toBeNull();
    expect(screen.getByTestId("amiga-chain-cannot-run").textContent).toBe(
      i18n.t("osinstall.chain.installed", { name: "BoingBag 3.9-1" })
    );
  });

  it("says ART has nowhere to look when no material folder has been named", async () => {
    // **F6.** "…is not in the folders you named" is strictly true of an
    // empty set and useless — a fact about a set the user has not
    // populated, read seven times over by somebody who reached this step
    // before the source step.
    chainMock.mockResolvedValue(chainReport(THE_CHAIN(), 2));
    render(
      <AmigaInstallPanel
        release="AmigaOS 3.9"
        treeRoot="D:/amiga/os39"
        packageFolder="D:/pkg"
        materialFolders={[]}
      />
    );
    const rows = await screen.findAllByTestId("amiga-chain-row");

    // Once, above the rows, naming where the folders are added.
    const banner = screen.getByTestId("amiga-chain-no-folders");
    expect(banner.textContent).toContain(i18n.t("osinstall.chain.noFolders"));
    expect(banner.querySelector("a")?.getAttribute("href")).toBe("/os-builder/dosyalar");
    expect(banner.querySelector("a")?.textContent).toBe(i18n.t("osBuilder.step.dosyalar"));

    // And the missing row says the short form of it rather than the sentence
    // about folders nobody named.
    expect(rows[5].textContent).toContain(
      i18n.t("osinstall.chain.missingNoFolders", { name: "BoingBag 3.9-2 Contribution" })
    );
    expect(rows[5].textContent).not.toContain("folders you named");
  });

  it("names a folder it could not read, and one it stopped counting in", async () => {
    // **F5.** Both fields have been on the wire since task 1 and nothing on
    // this screen rendered them — so a remembered folder on a drive nobody
    // plugged in left every row below saying "not in the folders you named"
    // about a folder ART could not open.
    chainMock.mockResolvedValue({
      ...chainReport(THE_CHAIN(), 2),
      unreadableFolders: ["Z:/gone"],
      crowdedFolders: [["E:/material", 200]] as [string, number][],
    });
    render(
      <AmigaInstallPanel
        release="AmigaOS 3.9"
        treeRoot="D:/amiga/os39"
        packageFolder="D:/pkg"
        materialFolders={["E:/material", "Z:/gone"]}
      />
    );
    await screen.findAllByTestId("amiga-chain-row");

    // The readout's own sentences, so the two screens cannot word it
    // differently.
    expect(screen.getByTestId("amiga-chain-unreadable-folder").textContent).toBe(
      i18n.t("osinstall.slots.unreadableFolder", { folder: "Z:/gone" })
    );
    expect(screen.getByTestId("amiga-chain-crowded-folder").textContent).toBe(
      i18n.t("osinstall.slots.crowdedFolder", { folder: "E:/material", bound: 200 })
    );
  });

  it("says nothing about folders when every one of them was read", async () => {
    // The control: an ordinary answer renders neither line. "0 folders could
    // not be read" is a sentence about nothing.
    renderChain();
    await screen.findAllByTestId("amiga-chain-row");
    expect(screen.queryByTestId("amiga-chain-unreadable-folder")).toBeNull();
    expect(screen.queryByTestId("amiga-chain-crowded-folder")).toBeNull();
    expect(screen.queryByTestId("amiga-chain-no-folders")).toBeNull();
  });

  it("falls back to the package list when ART knows no chain for this release", async () => {
    // A release with no chain answers with no rows, which is a fact and not
    // a failure — and a screen with no list at all would be one nobody can
    // select a package from.
    chainMock.mockResolvedValue(chainReport([]));
    render(
      <AmigaInstallPanel
        release="AmigaOS 3.9"
        treeRoot="D:/amiga/os39"
        packageFolder="D:/pkg"
        materialFolders={["E:/material"]}
      />
    );
    // Three, because this describe's catalogue is the real one: the two
    // BoingBags and BoingBags 3&4, which is listed and disabled (§10).
    expect(await screen.findAllByTestId("amiga-package-row")).toHaveLength(3);
    expect(screen.queryByTestId("amiga-chain")).toBeNull();
  });
});

/**
 * **The tree field is not offered when the caller's tree is the destination**
 * (round 3 fix wave, Important 2).
 *
 * `FilesTab` hands this panel `useChainTree`'s tree. When that hook picked
 * the *destination*, this panel's own Browse wrote `session.tree.root` — a
 * value the hook goes on overruling — so the path in the field snapped back
 * to the destination on the next render. A control that appears to work and
 * changes nothing is the confident wrong sentence in its purest form; the
 * panel says where the tree came from instead, and where to change it.
 */
describe("the distribution tree field, when the destination is what won", () => {
  it("draws one sentence with the path instead of a Browse row", async () => {
    // **Two distinct paths** (whole-branch review, Minor 9): the session
    // carries one tree, tab 3's destination is another, and the sentence
    // must print the destination. Seeded to one path — as this case was
    // until the review — a panel reading `session.tree.root` alone would
    // print the same string and pass.
    render(
      <AmigaInstallPanel
        release="AmigaOS 3.9"
        treeRoot="D:/amiga/session-tree"
        destination="D:/amiga/os39"
        packageFolder={null}
        treeFromDestination
      />
    );

    // **`find`, not `get`** (round 5, task 1): the panel asks
    // `osinstall_describe_tree` about the destination itself now, so *the
    // destination is a build* is a round trip rather than a prop. Until it
    // lands the panel draws the checking line the case below asserts — not
    // the Browse row it drew until the whole-branch review's Important 2.
    const said = await screen.findByTestId("amiga-tree-from-destination");
    expect(screen.queryByTestId("amiga-tree-root-field")).toBeNull();
    expect(said.textContent).toBe(
      i18n.t("osinstall.amigaInstall.treeRoot.fromDestination", { path: "D:/amiga/os39" })
    );
    // The sentence carries the path itself, and names the tab that owns it —
    // a user told "you cannot change it here" and not told where would have
    // been given nothing.
    expect(said.textContent).toContain("D:/amiga/os39");
    // And it is the *destination*, not the session's own tree: the two are
    // different paths in this case on purpose.
    expect(said.textContent).not.toContain("session-tree");
    expect(said.textContent).toContain(i18n.t("osBuilder.step.makine"));
  });

  it("still offers the Browse row when the tree is the session's own", () => {
    // The other arm, and the one that makes the first mean something: with
    // the prop absent this panel is what it always was.
    render(
      <AmigaInstallPanel release="AmigaOS 3.9" treeRoot="D:/amiga/os39" packageFolder={null} />
    );

    const field = screen.getByTestId("amiga-tree-root-field");
    expect(within(field).getByRole("button", { name: i18n.t("common.browse") })).toBeTruthy();
    expect(field.textContent).toContain(i18n.t("osinstall.packages.treeRoot.label"));
    expect(screen.queryByTestId("amiga-tree-from-destination")).toBeNull();
  });

  // **Added in round 5, task 1, because the write moved.** Until this round
  // the Browse row called an `onTreeRootChange` prop and `FilesTab` did the
  // writing; neither file guarded the wiring, and a mutation confirmed it —
  // dropping the call entirely left the whole suite green. The panel writes
  // `session.tree` itself now, so the case is written where the write is.
  //
  // Asserted on the store rather than on the field, because a value the
  // *session* does not carry is a Browse that appears to work and changes
  // nothing — this panel's own founding defect, one control along.
  it("writes the folder Browse returns into the build session's own tree", async () => {
    const { open } = await import("@tauri-apps/plugin-dialog");
    vi.mocked(open).mockResolvedValue("D:/amiga/picked-by-hand");
    render(
      <AmigaInstallPanel release="AmigaOS 3.9" treeRoot="D:/amiga/os39" packageFolder={null} />
    );

    const field = screen.getByTestId("amiga-tree-root-field");
    await userEvent
      .setup()
      .click(within(field).getByRole("button", { name: i18n.t("common.browse") }));

    await waitFor(() => {
      const bag = useSettingsStore.getState().settings.remembered as Record<string, unknown>;
      expect(bag["buildSession.tree"]).toMatchObject({
        root: "D:/amiga/picked-by-hand",
        builtHere: false,
      });
    });
  });
});

/**
 * **Neither branch, and no question, until the destination check has
 * answered** (whole-branch review of round 5, Important 2).
 *
 * `useChainTree`'s `settled` exists for one defect and its own comment names
 * it: `isTree` arrives by round trip, so for the first render or two a
 * destination that *is* a build looks like one that is not. The panel
 * ignored the flag, so it
 *
 *   - drew the *Distribution tree* Browse row for the session's tree and
 *     replaced it with the destination sentence a moment later — a control
 *     the user may have reached for, gone by the time they got there; and
 *   - asked `osinstall_chain` and `osinstall_slots` about the session's tree
 *     first and the destination second, so a row could read *ready* and then
 *     *installed* about one file, which is the exact contradiction the hook
 *     was written to end.
 *
 * `ChoiceTab` gates its ask on `treeSettled` and `StepSecim` draws nothing
 * until `settled`; these three cases are the panel saying the same thing.
 * The two arms below are the control: whichever way the check answers, the
 * panel commits to one branch and asks once.
 */
describe("the tree, while ART is still looking at the destination", () => {
  /** A `describe_tree` held open, plus the resolver for it. */
  function heldTree(): (summary: TreeSummary) => void {
    let answer: (summary: TreeSummary) => void = () => {};
    describeTreeMock.mockImplementation(
      () =>
        new Promise<TreeSummary>((resolve) => {
          answer = resolve;
        })
    );
    return (summary: TreeSummary) => act(() => answer(summary));
  }

  const A_TREE: TreeSummary = {
    isTree: true,
    release: "AmigaOS 3.9",
    files: 1915,
    components: ["workbench-base"],
    amigaInstalled: [],
    problem: null,
  };
  const NOT_A_TREE: TreeSummary = {
    isTree: false,
    release: null,
    files: 0,
    components: [],
    amigaInstalled: [],
    problem: "holds no distribution.json",
  };

  /** The destination and the session's tree are different paths on purpose:
   *  the chain's third argument then says which of the two was asked about. */
  function renderPending() {
    render(
      <AmigaInstallPanel
        release="AmigaOS 3.9"
        treeRoot="D:/amiga/session-tree"
        destination="D:/amiga/os39"
        packageFolder={null}
        treeFromDestination
      />
    );
  }

  it("draws one line about the wait, offers no field and asks no question", async () => {
    heldTree();
    renderPending();

    const line = await screen.findByTestId("amiga-tree-checking");
    expect(line.textContent).toBe(i18n.t("osinstall.chain.checkingDestination"));
    // Neither branch of the ternary: not the Browse row for a tree that may
    // be about to lose, and not a sentence about a destination ART has not
    // finished looking at.
    expect(screen.queryByTestId("amiga-tree-root-field")).toBeNull();
    expect(screen.queryByTestId("amiga-tree-from-destination")).toBeNull();
    // And nothing was asked about either tree. Asserted as "not called at
    // all" rather than "not called with the session tree", because a call
    // with `null` would be the same defect wearing a different argument.
    expect(chainMock).not.toHaveBeenCalled();
    expect(slotsMock).not.toHaveBeenCalled();
  });

  it("commits to the destination sentence and asks once, when the look finds a build", async () => {
    const answer = heldTree();
    renderPending();
    await screen.findByTestId("amiga-tree-checking");

    answer(A_TREE);

    const said = await screen.findByTestId("amiga-tree-from-destination");
    expect(said.textContent).toContain("D:/amiga/os39");
    expect(screen.queryByTestId("amiga-tree-checking")).toBeNull();
    expect(screen.queryByTestId("amiga-tree-root-field")).toBeNull();
    // **Once, and about the destination.** Two calls is the `ready →
    // installed` flip; one call naming `D:/amiga/session-tree` is the wrong
    // tree asked once.
    await waitFor(() => expect(chainMock).toHaveBeenCalledTimes(1));
    expect(chainMock.mock.calls[0][2]).toBe("D:/amiga/os39");
    await waitFor(() => expect(slotsMock).toHaveBeenCalledTimes(1));
    expect(slotsMock.mock.calls[0][2]).toBe("D:/amiga/os39");
  });

  it("offers the Browse row and asks about the session's tree, when the look finds no build", async () => {
    const answer = heldTree();
    renderPending();
    await screen.findByTestId("amiga-tree-checking");

    answer(NOT_A_TREE);

    const field = await screen.findByTestId("amiga-tree-root-field");
    expect(within(field).getByRole("button", { name: i18n.t("common.browse") })).toBeTruthy();
    expect(screen.queryByTestId("amiga-tree-checking")).toBeNull();
    expect(screen.queryByTestId("amiga-tree-from-destination")).toBeNull();
    await waitFor(() => expect(chainMock).toHaveBeenCalledTimes(1));
    expect(chainMock.mock.calls[0][2]).toBe("D:/amiga/session-tree");
    await waitFor(() => expect(slotsMock).toHaveBeenCalledTimes(1));
    expect(slotsMock.mock.calls[0][2]).toBe("D:/amiga/session-tree");
  });
});
