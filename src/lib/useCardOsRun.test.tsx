// @vitest-environment jsdom
//
// The card run's sequence, driven the way the screen drives it (round 4,
// task 10).
//
// Mocked at the `@/lib/*` boundary — `cardOsOpen`/`cardOsPrepare`/
// `cardOsBuild`/`cardOsClose`, the three install wrappers and the job helpers
// — because what is under test is *the session's lifetime, the order of the
// rows, and the ending each one earns*, not the IPC beneath them.
// `importOriginal` keeps the rest real, which matters for `isJobCancellation`
// / `JOB_CANCELLED_MESSAGE` (the real pair) and for `subscribeSafely`.
//
// **The `awaitJobResult` fake calls `start()` itself**, exactly as the real
// one does: the hook puts every `invoke` inside that callback so the result
// listener is registered before the command runs, and a fake that ignored
// `start` would run a sequence in which no command was ever called.

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, renderHook, waitFor } from "@testing-library/react";

import {
  CARD_OS_PREPARE_EVENT,
  type CardOsBuildResult,
  type CardOsPhaseEvent,
  type KickstartProposal,
  type PreparedCard,
} from "@/lib/cardOs";
import type { CardPhaseReport } from "@/lib/cardOsRun";
import type { FirstBootWritten } from "@/lib/firstboot";
import { JOB_CANCELLED_MESSAGE, type JobProgress } from "@/lib/jobs";
import {
  OSINSTALL_ADD_PACKAGE_EVENT,
  OSINSTALL_EVENT,
  type ApplyOutcome,
  type InstallPlan,
  type OsInstallResult,
} from "@/lib/osinstall";

const openMock = vi.hoisted(() => vi.fn());
const closeMock = vi.hoisted(() => vi.fn());
const prepareMock = vi.hoisted(() => vi.fn());
const buildMock = vi.hoisted(() => vi.fn());
const onPhaseMock = vi.hoisted(() => vi.fn());
const applyMock = vi.hoisted(() => vi.fn());
const addPackageMock = vi.hoisted(() => vi.fn());
const firstbootWriteMock = vi.hoisted(() => vi.fn());
const awaitJobResultMock = vi.hoisted(() => vi.fn());
const jobCancelMock = vi.hoisted(() => vi.fn());
const onJobProgressMock = vi.hoisted(() => vi.fn());

vi.mock("@/lib/cardOs", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/lib/cardOs")>()),
  cardOsOpen: openMock,
  cardOsClose: closeMock,
  cardOsPrepare: prepareMock,
  cardOsBuild: buildMock,
  onCardOsPhase: onPhaseMock,
}));

vi.mock("@/lib/osinstall", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/lib/osinstall")>()),
  osinstallApply: applyMock,
  osinstallAddPackage: addPackageMock,
}));

vi.mock("@/lib/firstboot", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/lib/firstboot")>()),
  firstbootWrite: firstbootWriteMock,
}));

vi.mock("@/lib/jobs", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/lib/jobs")>()),
  awaitJobResult: awaitJobResultMock,
  jobCancel: jobCancelMock,
  onJobProgress: onJobProgressMock,
}));

const { useCardOsRun } = await import("@/lib/useCardOsRun");
const { cardBarFraction, cardSequence } = await import("@/lib/cardOsRun");
const { JobRefused } = await import("@/lib/jobs");

// ---------------------------------------------------------------------------
// What the core answers with
// ---------------------------------------------------------------------------

const SESSION = 4;
const TREE = "E:\\amiga\\ProjeART\\build\\tmp\\art-card-4\\tree";
const IMAGE = "E:\\amiga\\kart.img";
const ARCHIVES = "E:\\amiga\\arsiv";
const BB1 = `${ARCHIVES}\\BoingBag39-1.lha`;

/** The hook never reads a field of the plan — it hands it to `osinstallApply`
 *  exactly as the screen was shown it. */
const PLAN = {} as InstallPlan;

const TREE_OUTCOME: ApplyOutcome = {
  root: TREE,
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
  destination: TREE,
  outcome: TREE_OUTCOME,
  stated_release: { verdict: "confirmed", stated: "39.29" },
};

const PACKAGE_RESULT = { job_id: 12, outcome: { ...TREE_OUTCOME, files: 166 } };

const WRITTEN: FirstBootWritten = {
  files: ["S:ART-FirstBoot"],
  userStartupBackup: null,
  userStartupCreated: true,
  removed: [],
};

const PROPOSAL: KickstartProposal = {
  items: [
    {
      name: "kick40068.A1200",
      titles: ["Games/Turrican/Turrican.slave"],
      titlesMore: 0,
      offer: {
        outcome: "supplied",
        wanted: { name: "kick40068.A1200", crc16: 1234, size: 524_288 },
        by: { path: "E:\\roms\\kick40068.A1200", name: "kick40068.A1200", sizeDisagrees: null },
      },
      rtb: { kind: "loose", path: "E:\\roms\\kick40068.A1200.RTB" },
    },
  ],
  unreadableSlaves: [],
  rtbMissing: false,
};

const PREPARED = { kickstarts: PROPOSAL } as unknown as PreparedCard;

function buildResult(ending: CardOsBuildResult["ending"]): CardOsBuildResult {
  return {
    jobId: 13,
    session: SESSION,
    image: IMAGE,
    ending,
    whdload: null,
    kickstarts: [],
    steps: [],
    manifestPath: ending.ending === "succeeded" ? `${IMAGE}.art-manifest.json` : null,
    health: null,
    partial:
      ending.ending === "succeeded"
        ? { outcome: "not-created" }
        : { outcome: "removed", path: `${IMAGE}.partial` },
    scratchLeft: null,
  };
}

// ---------------------------------------------------------------------------
// The harness
// ---------------------------------------------------------------------------

let order: string[] = [];
let report: ((job: JobProgress) => void) | null = null;
let phaseSaid: ((event: CardOsPhaseEvent) => void) | null = null;

type Start = () => Promise<number>;

/** What each job's result event carries — settable per test, because the
 *  build's ending is the subject of half of them. */
let buildAnswer: CardOsBuildResult = buildResult({ ending: "succeeded" });

async function settles(
  event: string,
  start: Start,
  extract: (payload: never) => unknown
): Promise<unknown> {
  await start();
  const payload =
    event === OSINSTALL_EVENT
      ? TREE_RESULT
      : event === OSINSTALL_ADD_PACKAGE_EVENT
        ? PACKAGE_RESULT
        : event === CARD_OS_PREPARE_EVENT
          ? { jobId: 12, session: SESSION, prepared: PREPARED }
          : buildAnswer;
  return extract(payload as never);
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

const PHASES = cardSequence({
  release: "AmigaOS 3.9",
  updates: [
    {
      packageId: "boingbag-39-1",
      slotId: "package:boingbag-39-1",
      name: "BoingBag 3.9-1",
      file: BB1,
    },
  ],
  firstBootWanted: true,
  firstBootAskPrefs: true,
  image: IMAGE,
});

const INPUT = {
  phases: PHASES,
  plan: PLAN,
  prepare: {
    cardGb: 64,
    image: IMAGE,
    partitions: [{ volumeName: "Games", sources: [`${ARCHIVES}\\oyunlar`] }],
    material: [ARCHIVES],
  },
  build: {
    archive: "E:\\emu68\\Emu68-pistorm.zip",
    kickstart: "E:\\roms\\kick.rom",
    label: "ART CARD",
    hardware: { amiga: "a500" as const, variant: "pistorm32-lite" as const, pi: "pi4-b" as const },
    line: "stable" as const,
  },
};

function setup() {
  const onSessionTree = vi.fn();
  const view = renderHook(() => useCardOsRun({ onSessionTree }));
  return { ...view, onSessionTree };
}

function states(reports: CardPhaseReport[]): string[] {
  return reports.map((r) => r.ending.state);
}

async function flush() {
  await act(async () => {
    await Promise.resolve();
  });
}

/** Run every row up to the agreement, and stop there. */
async function toAgreement(view: ReturnType<typeof setup>) {
  act(() => view.result.current.start(INPUT));
  await waitFor(() => expect(view.result.current.awaitingAgreement).toBe(true));
}

beforeEach(() => {
  order = [];
  report = null;
  phaseSaid = null;
  buildAnswer = buildResult({ ending: "succeeded" });
  openMock.mockReset().mockImplementation(async () => {
    order.push("open");
    return { session: SESSION, tree: TREE };
  });
  closeMock.mockReset().mockImplementation(async () => {
    order.push("close");
    return { scratchLeft: null };
  });
  prepareMock.mockReset().mockImplementation(async () => {
    order.push("prepare");
    return 12;
  });
  buildMock.mockReset().mockImplementation(async () => {
    order.push("build");
    return 13;
  });
  onPhaseMock.mockReset().mockImplementation(async (handler: (e: CardOsPhaseEvent) => void) => {
    phaseSaid = handler;
    return () => {};
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
});

afterEach(() => {
  cleanup();
});

// ---------------------------------------------------------------------------

describe("the session (Q9, ART-344)", () => {
  it("is not opened by rendering the hook — only by the button", async () => {
    const view = setup();
    await flush();
    expect(openMock).not.toHaveBeenCalled();
    expect(view.result.current.tree).toBeNull();
  });

  it("hands the section the tree while the session lives, and takes it back when it goes", async () => {
    const view = setup();
    await toAgreement(view);
    expect(view.result.current.tree).toBe(TREE);
    expect(view.onSessionTree).toHaveBeenCalledWith(TREE);

    act(() => view.result.current.giveUp());
    await waitFor(() => expect(view.result.current.tree).toBeNull());
    expect(view.onSessionTree).toHaveBeenLastCalledWith(null);
  });

  it("is closed exactly once when the user gives up at the agreement", async () => {
    const view = setup();
    await toAgreement(view);
    act(() => view.result.current.giveUp());
    await waitFor(() => expect(view.result.current.running).toBe(false));
    expect(closeMock.mock.calls).toEqual([[SESSION]]);
    expect(buildMock).not.toHaveBeenCalled();
  });

  it("is closed exactly once when a row before the build fails", async () => {
    const job = pending();
    const view = setup();
    act(() => view.result.current.start(INPUT));
    await flush();
    await act(async () => {
      job.reject(new Error("no media (ART-IO)"));
    });
    await waitFor(() => expect(view.result.current.running).toBe(false));
    expect(closeMock.mock.calls).toEqual([[SESSION]]);
  });

  // **Rust's own `end_build` removes the session after every ending**, so a
  // close from here would be a second answer to a settled question — and an
  // error on screen for a session that is already gone.
  it("is never closed from here once the build has started", async () => {
    const view = setup();
    await toAgreement(view);
    act(() => view.result.current.agree(["kick40068.A1200"]));
    await waitFor(() => expect(view.result.current.finished).toBe(true));
    expect(buildMock).toHaveBeenCalledTimes(1);
    expect(closeMock).not.toHaveBeenCalled();
  });

  it("closes the session once even when the build was refused", async () => {
    buildAnswer = buildResult({
      ending: "refused",
      phase: "card",
      code: "ART-CARD-DOES-NOT-FIT",
      message: "it does not fit",
      params: { kind: "does-not-fit" },
    });
    const view = setup();
    await toAgreement(view);
    act(() => view.result.current.agree([]));
    await waitFor(() => expect(view.result.current.finished).toBe(true));
    expect(closeMock).not.toHaveBeenCalled();
  });
});

describe("the sequence", () => {
  it("opens, builds the tree, adds the update, writes first boot, prepares, waits, then builds", async () => {
    const view = setup();
    await toAgreement(view);
    expect(order).toEqual(["open", "apply", "addPackage", "firstboot", "prepare"]);
    // The build has not been asked for: the agreement is a real pause.
    expect(buildMock).not.toHaveBeenCalled();

    act(() => view.result.current.agree(["kick40068.A1200"]));
    await waitFor(() => expect(view.result.current.finished).toBe(true));
    expect(order).toEqual(["open", "apply", "addPackage", "firstboot", "prepare", "build"]);
    expect(states(view.result.current.reports)).toEqual([
      "succeeded",
      "succeeded",
      "succeeded",
      "succeeded",
      "succeeded",
      "succeeded",
    ]);
    expect(view.result.current.succeeded).toBe(true);
  });

  it("writes the tree into the session's own folder, never into a folder the user chose", async () => {
    const view = setup();
    await toAgreement(view);
    expect(applyMock).toHaveBeenCalledWith(PLAN, TREE);
  });

  it("passes only the names the user agreed to, and the session it opened", async () => {
    const view = setup();
    await toAgreement(view);
    act(() => view.result.current.agree(["kick40068.A1200"]));
    await waitFor(() => expect(buildMock).toHaveBeenCalled());
    expect(buildMock.mock.calls[0][0]).toMatchObject({
      session: SESSION,
      agreedKickstarts: ["kick40068.A1200"],
    });
  });

  it("offers the proposal the preparation answered", async () => {
    const view = setup();
    await toAgreement(view);
    expect(view.result.current.proposal).toBe(PROPOSAL);
  });
});

describe("the four endings, kept apart", () => {
  it("calls a refused build refused, with the refusal it was given", async () => {
    buildAnswer = buildResult({
      ending: "refused",
      phase: "card",
      code: "ART-CARD-DOES-NOT-FIT",
      message: "it does not fit",
      params: { kind: "does-not-fit", needed: "1", available: "0" },
    });
    const view = setup();
    await toAgreement(view);
    act(() => view.result.current.agree([]));
    await waitFor(() => expect(view.result.current.finished).toBe(true));

    const ending = view.result.current.reports[5].ending;
    expect(ending.state).toBe("refused");
    if (ending.state !== "refused") throw new Error("not refused");
    expect(ending.refusal.code).toBe("ART-CARD-DOES-NOT-FIT");
    expect(ending.refusal.params.kind).toBe("does-not-fit");
    expect(view.result.current.succeeded).toBe(false);
  });

  it("calls a failed build failed, and keeps what became of the .partial", async () => {
    buildAnswer = buildResult({
      ending: "failed",
      phase: "partitions",
      code: "ART-IO",
      message: "the disk filled up",
      params: {},
    });
    const view = setup();
    await toAgreement(view);
    act(() => view.result.current.agree([]));
    await waitFor(() => expect(view.result.current.finished).toBe(true));

    expect(view.result.current.reports[5].ending.state).toBe("failed");
    expect(view.result.current.result?.partial).toEqual({
      outcome: "removed",
      path: `${IMAGE}.partial`,
    });
    expect(view.result.current.result?.image).toBe(IMAGE);
  });

  it("says which phase a stopped build stopped in", async () => {
    buildAnswer = buildResult({ ending: "stopped", phase: "partitions" });
    const view = setup();
    await toAgreement(view);
    act(() => view.result.current.agree([]));
    await waitFor(() => expect(view.result.current.finished).toBe(true));

    const ending = view.result.current.reports[5].ending;
    expect(ending.state).toBe("stopped");
    if (ending.state !== "stopped") throw new Error("not stopped");
    expect(ending.phase).toBe("partitions");
  });

  // A job that ends **refused** with no result event: `awaitJobResult` used
  // not to settle at all for that state (Task 1's carry).
  it("calls a refused job refused rather than failed, and settles", async () => {
    const job = pending();
    const view = setup();
    act(() => view.result.current.start(INPUT));
    await flush();
    await act(async () => {
      job.reject(new JobRefused("ART-SAFETY-REFUSED", "that folder is not empty"));
    });
    await waitFor(() => expect(view.result.current.running).toBe(false));

    const ending = view.result.current.reports[0].ending;
    expect(ending.state).toBe("refused");
    if (ending.state !== "refused") throw new Error("not refused");
    expect(ending.refusal.code).toBe("ART-SAFETY-REFUSED");
  });

  it("calls a cancelled job stopped, and never attempts what came after", async () => {
    const job = pending();
    const view = setup();
    act(() => view.result.current.start(INPUT));
    await flush();
    await act(async () => {
      job.reject(new Error(JOB_CANCELLED_MESSAGE));
    });
    await waitFor(() => expect(view.result.current.running).toBe(false));
    expect(states(view.result.current.reports)).toEqual([
      "stopped",
      "not-attempted",
      "not-attempted",
      "not-attempted",
      "not-attempted",
      "not-attempted",
    ]);
  });
});

describe("the count and the phase name, joined", () => {
  it("takes the count from the job and the phase name from the phase event", async () => {
    const job = pending();
    const view = setup();
    act(() => view.result.current.start(INPUT));
    await flush();

    await act(async () => {
      phaseSaid?.({
        jobId: 11,
        session: SESSION,
        phase: "partitions",
        done: 0,
        total: 9216,
        unit: "files",
      });
      report?.({
        id: 11,
        title: { key: "components.jobBar.title.buildCardOs", params: { target: IMAGE } },
        done: 4812,
        total: 9216,
        message: "Games:Turrican",
        state: { state: "running" },
      });
    });

    const row = view.result.current.reports[0];
    expect(row.now).toBe("partitions");
    expect(row.ending).toEqual({ state: "running", done: 4812, total: 9216 });
    expect(cardBarFraction(row.ending)).toBeCloseTo(4812 / 9216, 5);
    job.resolve(TREE_RESULT);
  });

  it("draws no bar for a phase whose job states no total", async () => {
    const job = pending();
    const view = setup();
    act(() => view.result.current.start(INPUT));
    await flush();

    await act(async () => {
      report?.({
        id: 11,
        title: { key: "components.jobBar.title.buildCardOs", params: { target: IMAGE } },
        done: 313,
        total: null,
        message: "",
        state: { state: "running" },
      });
    });

    const row = view.result.current.reports[0];
    expect(row.ending).toEqual({ state: "running", done: 313, total: null });
    expect(cardBarFraction(row.ending)).toBeNull();
    job.resolve(TREE_RESULT);
  });
});

describe("giving up and stopping", () => {
  it("asks the running job to stop, and leaves the ending to the job", async () => {
    const job = pending();
    const view = setup();
    act(() => view.result.current.start(INPUT));
    await flush();
    act(() => view.result.current.stop());
    expect(jobCancelMock).toHaveBeenCalledWith(11);
    await act(async () => {
      job.reject(new Error(JOB_CANCELLED_MESSAGE));
    });
    await waitFor(() => expect(view.result.current.running).toBe(false));
  });

  it("names a session folder that could not be removed rather than claiming it went", async () => {
    closeMock.mockResolvedValue({ scratchLeft: { path: TREE, why: "in use" } });
    const view = setup();
    await toAgreement(view);
    act(() => view.result.current.giveUp());
    await waitFor(() => expect(view.result.current.running).toBe(false));
    expect(view.result.current.scratchLeft).toEqual({ path: TREE, why: "in use" });
  });
});
