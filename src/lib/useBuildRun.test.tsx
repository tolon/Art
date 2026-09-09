// @vitest-environment jsdom
//
// The build run's sequence, driven the way the screen drives it.
//
// Everything below the hook is mocked at the `@/lib/*` boundary — the three
// wrappers it calls (`osinstallApply`, `osinstallAddPackage`,
// `firstbootWrite`) and the three job helpers (`awaitJobResult`, `jobCancel`,
// `onJobProgress`) — because what is under test is *the order of the phases
// and the ending each one earns*, not the IPC beneath them. `importOriginal`
// keeps every other export real, which matters for two of them:
// `isJobCancellation` and `JOB_CANCELLED_MESSAGE` are the real pair, so a
// test that rejects with the real constant proves the real predicate, and
// `subscribeSafely` is the real one the progress subscription goes through.
//
// **The `awaitJobResult` fake calls `start()` itself**, exactly as the real
// one does. That is not decoration: the hook puts `osinstallApply` and
// `osinstallAddPackage` *inside* that callback so the result listener is
// registered before the command is invoked, so a fake that ignored `start`
// would run a sequence in which no command was ever called at all.

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, renderHook, waitFor } from "@testing-library/react";

import { FIRST_BOOT_PHASE_NAME, type Phase, type PhaseReport } from "@/lib/buildRun";
import type { FirstBootWritten } from "@/lib/firstboot";
import { JOB_CANCELLED_MESSAGE, type JobProgress } from "@/lib/jobs";
import {
  OSINSTALL_ADD_PACKAGE_EVENT,
  OSINSTALL_EVENT,
  type ApplyOutcome,
  type InstallPlan,
  type OsInstallResult,
  type RefusalReason,
} from "@/lib/osinstall";

const applyMock = vi.hoisted(() => vi.fn());
const addPackageMock = vi.hoisted(() => vi.fn());
const firstbootWriteMock = vi.hoisted(() => vi.fn());
const awaitJobResultMock = vi.hoisted(() => vi.fn());
const jobCancelMock = vi.hoisted(() => vi.fn());
const onJobProgressMock = vi.hoisted(() => vi.fn());

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

const { useBuildRun } = await import("@/lib/useBuildRun");

// ---------------------------------------------------------------------------
// What the core answers with
// ---------------------------------------------------------------------------

const DEST = "E:\\amiga\\Amigatolon\\sonuclar";
const ARCHIVES = "E:\\amiga\\arsiv";
const BB1 = "E:\\amiga\\arsiv\\BoingBag39-1.lha";

/** The hook never reads a field of the plan — it hands it to `osinstallApply`
 *  exactly as the screen was shown it — so identity is all a test can assert
 *  about it, and all that is worth asserting. */
const PLAN = {} as InstallPlan;

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

const PACKAGE_OUTCOME: ApplyOutcome = { ...TREE_OUTCOME, files: 166, bytes: 2_100_000 };

const TREE_RESULT: OsInstallResult = {
  job_id: 11,
  destination: DEST,
  outcome: TREE_OUTCOME,
  stated_release: { verdict: "confirmed", stated: "39.29" },
};

const PACKAGE_RESULT = { job_id: 12, outcome: PACKAGE_OUTCOME };

const WRITTEN: FirstBootWritten = {
  files: ["S:ART-FirstBoot", "S:ART-FirstBoot-Report"],
  userStartupBackup: null,
  userStartupCreated: true,
};

const FOLDER_MISSING: RefusalReason = {
  refusal: "package-folder-missing",
  packages: ["boingbag-39-1"],
};

// ---------------------------------------------------------------------------
// The phases
// ---------------------------------------------------------------------------

const TREE_PHASE: Phase = { id: "tree", kind: "tree", name: "AmigaOS 3.9" };

const PACKAGE_PHASE: Phase = {
  id: "package:boingbag-39-1",
  kind: "package",
  name: "BoingBag 3.9-1",
  packageId: "boingbag-39-1",
  slotId: "boingbag-39-1-archive",
  file: BB1,
  folder: ARCHIVES,
};

const FIRSTBOOT_PHASE: Phase = {
  id: "firstboot",
  kind: "firstboot",
  name: FIRST_BOOT_PHASE_NAME,
};

function progress(id: number, state: JobProgress["state"], done = 0): JobProgress {
  return { id, title: "Building", done, total: 1980, message: "SYS:C/Assign", state };
}

// ---------------------------------------------------------------------------
// The harness
// ---------------------------------------------------------------------------

/** Which command ran, in the order it ran — the sequence's own subject. */
let order: string[] = [];

/** The one live `onJobProgress` handler, so a test can deliver an update the
 *  way the backend would. */
let report: ((job: JobProgress) => void) | null = null;

type Start = () => Promise<number>;
type ResultPayload = OsInstallResult | { job_id: number; outcome: ApplyOutcome };

/** `awaitJobResult`'s own contract: call `start` (which invokes the command
 *  and hands the hook the job id), then settle with `extract` over the result
 *  event's payload. */
async function settles(
  event: string,
  start: Start,
  extract: (payload: ResultPayload) => unknown
): Promise<unknown> {
  await start();
  return extract(event === OSINSTALL_EVENT ? TREE_RESULT : PACKAGE_RESULT);
}

/** A job that has started and has not answered yet; the test settles it by
 *  hand. Both halves: rejecting is how a failure and a cancellation reach the
 *  hook, and resolving is how a phase that **succeeds** late does — which is
 *  the only way to tell "the run stopped because the screen went" apart from
 *  "the run stopped because the phase did not succeed". */
function pending(): {
  resolve: (value: unknown) => void;
  reject: (err: unknown) => void;
} {
  const handle = { resolve: (_value: unknown) => {}, reject: (_err: unknown) => {} };
  awaitJobResultMock.mockImplementationOnce(async (_event: string, start: Start) => {
    await start();
    return new Promise((resolve, reject) => {
      handle.resolve = resolve;
      handle.reject = reject;
    });
  });
  return handle;
}

function setup(destination: string | null = DEST) {
  const onTreeWritten = vi.fn();
  const onFirstBootWritten = vi.fn();
  const view = renderHook(() =>
    useBuildRun({ destination, onTreeWritten, onFirstBootWritten })
  );
  return { ...view, onTreeWritten, onFirstBootWritten };
}

function states(reports: PhaseReport[]): string[] {
  return reports.map((r) => r.ending.state);
}

/** Narrowed, with the actual state in the message when it is not a success —
 *  a bare `toBe("succeeded")` says which phase disagreed but not what it did
 *  instead. */
function succeeded(report_: PhaseReport) {
  if (report_.ending.state !== "succeeded") {
    throw new Error(`${report_.phase.id} ended ${report_.ending.state}, not succeeded`);
  }
  return report_.ending;
}

/** One microtask turn inside `act`, for the continuations a settled promise
 *  schedules. */
async function flush() {
  await act(async () => {
    await Promise.resolve();
  });
}

beforeEach(() => {
  order = [];
  report = null;
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

describe("the build run's sequence", () => {
  it("runs the tree, the update and first boot in that order, each with its own success", async () => {
    const { result, onTreeWritten, onFirstBootWritten } = setup();

    act(() => {
      result.current.start([TREE_PHASE, PACKAGE_PHASE, FIRSTBOOT_PHASE], PLAN);
    });
    await waitFor(() => expect(result.current.finished).toBe(true));

    expect(order).toEqual(["apply", "addPackage", "firstboot"]);
    expect(states(result.current.reports)).toEqual(["succeeded", "succeeded", "succeeded"]);
    expect(result.current.succeeded).toBe(true);
    expect(result.current.running).toBe(false);

    // Each phase's own answer, on its own row — the tree's release verdict
    // included, which is the one thing only the tree phase carries.
    expect(succeeded(result.current.reports[0]).outcome).toEqual(TREE_OUTCOME);
    expect(succeeded(result.current.reports[0]).statedRelease).toEqual({
      verdict: "confirmed",
      stated: "39.29",
    });
    expect(succeeded(result.current.reports[1]).outcome).toEqual(PACKAGE_OUTCOME);
    expect(succeeded(result.current.reports[2]).outcome).toEqual(WRITTEN);
    expect(succeeded(result.current.reports[0]).elapsedMs).toBeGreaterThanOrEqual(0);

    expect(onTreeWritten).toHaveBeenCalledTimes(1);
    expect(onTreeWritten).toHaveBeenCalledWith(DEST);
    expect(onFirstBootWritten).toHaveBeenCalledTimes(1);

    // The commands, with the arguments the inventory's § 4 pattern settled:
    // the tree root, the folder the archive was actually found in, the one
    // package id, and the slot's own file as the override.
    expect(applyMock).toHaveBeenCalledWith(PLAN, DEST);
    expect(addPackageMock).toHaveBeenCalledWith(
      DEST,
      ARCHIVES,
      ["boingbag-39-1"],
      [["boingbag-39-1-archive", BB1]]
    );
    expect(firstbootWriteMock).toHaveBeenCalledWith(DEST);

    // Each job waited on its own result event, never the other's.
    expect(awaitJobResultMock.mock.calls[0][0]).toBe(OSINSTALL_EVENT);
    expect(awaitJobResultMock.mock.calls[1][0]).toBe(OSINSTALL_ADD_PACKAGE_EVENT);
  });

  it("stops at the first ending that is not a success, and the rest were not attempted", async () => {
    addPackageMock.mockImplementationOnce(async () => {
      order.push("addPackage");
      return { outcome: "refused", refusals: [FOLDER_MISSING] };
    });
    const { result, onFirstBootWritten } = setup();

    act(() => {
      result.current.start([TREE_PHASE, PACKAGE_PHASE, FIRSTBOOT_PHASE], PLAN);
    });
    await waitFor(() => expect(result.current.finished).toBe(true));

    expect(states(result.current.reports)).toEqual([
      "succeeded",
      "refused",
      "not-attempted",
    ]);
    // Nothing was run for the third phase, and nothing claims it was.
    expect(order).toEqual(["apply", "addPackage"]);
    expect(firstbootWriteMock).not.toHaveBeenCalled();
    expect(onFirstBootWritten).not.toHaveBeenCalled();
    expect(result.current.succeeded).toBe(false);
  });

  it("keeps a refusal a typed ending rather than an error", async () => {
    addPackageMock.mockImplementationOnce(async () => ({
      outcome: "refused",
      refusals: [FOLDER_MISSING],
    }));
    const { result } = setup();

    act(() => {
      result.current.start([PACKAGE_PHASE], null);
    });
    await waitFor(() => expect(result.current.finished).toBe(true));

    // The core's own reason, whole — not a message, not a `failed` ending
    // wearing a refusal's words.
    expect(result.current.reports[0].ending).toEqual({
      state: "refused",
      refusals: [FOLDER_MISSING],
    });
    expect(states(result.current.reports)).not.toContain("failed");
  });

  it("carries the failed job's own words, its code and what had landed", async () => {
    const job = pending();
    const { result } = setup();

    act(() => {
      result.current.start([TREE_PHASE, FIRSTBOOT_PHASE], PLAN);
    });
    await waitFor(() => expect(applyMock).toHaveBeenCalled());
    await waitFor(() => expect(report).not.toBeNull());

    // Progress reaches the running phase's own ending, as itself.
    await act(async () => {
      report?.(progress(11, { state: "running" }, 340));
      await Promise.resolve();
    });
    expect(result.current.reports[0].ending).toMatchObject({
      state: "running",
      progress: { id: 11, done: 340, total: 1980 },
    });

    await act(async () => {
      report?.(
        progress(11, { state: "failed", error_code: "ART-042", message: "the disk is full" }, 340)
      );
      // What `awaitJobResult` actually rejects with: the message with the
      // code appended, and nothing else.
      job.reject(new Error("the disk is full (ART-042)"));
      await Promise.resolve();
    });
    await waitFor(() => expect(result.current.finished).toBe(true));

    expect(result.current.reports[0].ending).toEqual({
      state: "failed",
      message: "the disk is full",
      errorCode: "ART-042",
      filesLanded: 340,
    });
    expect(result.current.reports[1].ending).toEqual({ state: "not-attempted" });
    expect(firstbootWriteMock).not.toHaveBeenCalled();
  });

  it("Stop cancels the job that is running and starts no later phase", async () => {
    // `mockImplementationOnce` is a queue: the tree's answer first, then the
    // package's job, which stays unanswered until this test settles it.
    awaitJobResultMock.mockImplementationOnce(settles);
    const job = pending();

    const { result, onFirstBootWritten } = setup();
    act(() => {
      result.current.start([TREE_PHASE, PACKAGE_PHASE, FIRSTBOOT_PHASE], PLAN);
    });
    await waitFor(() => expect(addPackageMock).toHaveBeenCalled());
    await waitFor(() => expect(report).not.toBeNull());

    act(() => {
      result.current.stop();
    });
    await waitFor(() => expect(jobCancelMock).toHaveBeenCalledWith(12));

    await act(async () => {
      report?.(progress(12, { state: "cancelled", files_landed: 44 }, 44));
      job.reject(new Error(JOB_CANCELLED_MESSAGE));
      await Promise.resolve();
    });
    await waitFor(() => expect(result.current.finished).toBe(true));

    expect(states(result.current.reports)).toEqual([
      "succeeded",
      "cancelled",
      "not-attempted",
    ]);
    // The job's own count of what was written and left in place.
    expect(result.current.reports[1].ending).toEqual({ state: "cancelled", filesLanded: 44 });
    expect(firstbootWriteMock).not.toHaveBeenCalled();
    expect(onFirstBootWritten).not.toHaveBeenCalled();
    expect(result.current.running).toBe(false);
  });

  it("never asks for a tree when the sequence has no tree phase", async () => {
    const { result, onTreeWritten, onFirstBootWritten } = setup();

    act(() => {
      result.current.start([PACKAGE_PHASE, FIRSTBOOT_PHASE], null);
    });
    await waitFor(() => expect(result.current.finished).toBe(true));

    expect(applyMock).not.toHaveBeenCalled();
    expect(onTreeWritten).not.toHaveBeenCalled();
    expect(order).toEqual(["addPackage", "firstboot"]);
    expect(states(result.current.reports)).toEqual(["succeeded", "succeeded"]);
    expect(onFirstBootWritten).toHaveBeenCalledTimes(1);
  });

  it("replaces the whole report when it is started again", async () => {
    addPackageMock.mockImplementationOnce(async () => ({
      outcome: "refused",
      refusals: [FOLDER_MISSING],
    }));
    const { result } = setup();

    act(() => {
      result.current.start([PACKAGE_PHASE, FIRSTBOOT_PHASE], null);
    });
    await waitFor(() => expect(result.current.finished).toBe(true));
    expect(states(result.current.reports)).toEqual(["refused", "not-attempted"]);

    act(() => {
      result.current.start([PACKAGE_PHASE], null);
    });
    await waitFor(() => expect(result.current.finished).toBe(true));

    // One row, its own ending — no refusal left over from the run before it.
    expect(result.current.reports).toHaveLength(1);
    expect(states(result.current.reports)).toEqual(["succeeded"]);
    expect(result.current.succeeded).toBe(true);
  });

  it("ignores a second start while a run is going", async () => {
    pending();
    const { result } = setup();

    act(() => {
      result.current.start([TREE_PHASE, PACKAGE_PHASE], PLAN);
    });
    await waitFor(() => expect(applyMock).toHaveBeenCalled());

    act(() => {
      result.current.start([FIRSTBOOT_PHASE], null);
    });
    await flush();

    expect(result.current.reports.map((r) => r.phase.id)).toEqual([
      "tree",
      "package:boingbag-39-1",
    ]);
    expect(firstbootWriteMock).not.toHaveBeenCalled();
    expect(applyMock).toHaveBeenCalledTimes(1);
    expect(result.current.running).toBe(true);
  });

  it("sets nothing and runs nothing further once the screen is gone", async () => {
    const job = pending();
    const { result, unmount, onTreeWritten } = setup();

    act(() => {
      result.current.start([TREE_PHASE, FIRSTBOOT_PHASE], PLAN);
    });
    await waitFor(() => expect(applyMock).toHaveBeenCalled());

    unmount();
    // The phase **succeeds** after the unmount, on purpose: a phase that
    // failed would have stopped the sequence anyway, and then this case would
    // pass for a reason that has nothing to do with the screen being gone.
    await act(async () => {
      job.resolve(TREE_RESULT);
      await Promise.resolve();
    });
    await flush();

    // The session hand-off belongs to a screen that is no longer there, and
    // the phase after it never starts.
    expect(onTreeWritten).not.toHaveBeenCalled();
    expect(firstbootWriteMock).not.toHaveBeenCalled();
  });

  it("runs nothing at all without a destination", async () => {
    const { result } = setup(null);

    act(() => {
      result.current.start([TREE_PHASE, FIRSTBOOT_PHASE], PLAN);
    });
    await flush();

    expect(applyMock).not.toHaveBeenCalled();
    expect(result.current.reports).toEqual([]);
    expect(result.current.running).toBe(false);
    // Nothing ran, so nothing is finished — an empty report is not a finished
    // one, which is the difference between "not started" and "all done".
    expect(result.current.finished).toBe(false);
  });
});
