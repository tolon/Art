// Pure TS, no DOM: the wrappers over `commands/cardos.rs`. What this proves
// is the wiring a green Rust suite cannot — each wrapper names the command
// `invoke_handler![]` registers and passes its argument under the key the
// command's parameter is called (`request`, `session`), and each subscription
// listens on the event name the Rust constant emits.

import { beforeEach, describe, expect, it, vi } from "vitest";

const invokeMock = vi.hoisted(() => vi.fn());
const listenMock = vi.hoisted(() => vi.fn());

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));
vi.mock("@tauri-apps/api/event", () => ({
  listen: (...args: unknown[]) => listenMock(...args),
}));

import {
  CARD_OS_BUILD_EVENT,
  CARD_OS_MEASURE_EVENT,
  CARD_OS_PHASE_EVENT,
  CARD_OS_PREPARE_EVENT,
  cardOsBuild,
  cardOsCheckVolumeName,
  cardOsClassify,
  cardOsClose,
  cardOsMeasure,
  cardOsOpen,
  cardOsPrepare,
  onCardOsBuildResult,
  onCardOsMeasureResult,
  onCardOsPhase,
  onCardOsPrepareResult,
  type CardOsBuildRequest,
  type CardOsBuildResult,
  type CardOsEnding,
  type CardOsMeasureRequest,
  type CardOsMeasureResult,
  type CardOsPhaseEvent,
  type CardOsPhaseName,
  type CardOsPrepareRequest,
  type ClassifiedSource,
  type PartialRemoval,
  type UnusableSource,
  type VolumeNameVerdict,
} from "@/lib/cardOs";

beforeEach(() => {
  invokeMock.mockReset();
  listenMock.mockReset();
});

const PREPARE: CardOsPrepareRequest = {
  session: 3,
  cardGb: 16,
  image: "E:\\amiga\\ProjeART\\card.img",
  partitions: [{ volumeName: "Games", sources: ["E:\\amiga\\Games"] }],
  material: ["E:\\amiga\\material"],
};

const BUILD: CardOsBuildRequest = {
  session: 3,
  agreedKickstarts: ["kick40068.A1200"],
  archive: "E:\\amiga\\Emu68-pistorm.zip",
  kickstart: null,
  label: "ART CARD",
  hardware: { amiga: "a500", variant: "classic", pi: "pi3-a-plus" },
  line: "stable",
};

describe("the card session wrappers", () => {
  it("opens with card_os_open and no arguments", async () => {
    invokeMock.mockResolvedValueOnce({ session: 3, tree: "E:\\tmp\\card-os-1-1\\tree" });
    const opened = await cardOsOpen();
    expect(invokeMock).toHaveBeenCalledWith("card_os_open");
    expect(opened.session).toBe(3);
  });

  it("closes with card_os_close under the key session", async () => {
    invokeMock.mockResolvedValueOnce({ scratchLeft: null });
    const closed = await cardOsClose(3);
    expect(invokeMock).toHaveBeenCalledWith("card_os_close", { session: 3 });
    expect(closed.scratchLeft).toBeNull();
  });

  it("prepares with card_os_prepare under the key request and returns the job id", async () => {
    invokeMock.mockResolvedValueOnce(41);
    expect(await cardOsPrepare(PREPARE)).toBe(41);
    expect(invokeMock).toHaveBeenCalledWith("card_os_prepare", { request: PREPARE });
  });

  it("builds with card_os_build under the key request and returns the job id", async () => {
    invokeMock.mockResolvedValueOnce(42);
    expect(await cardOsBuild(BUILD)).toBe(42);
    expect(invokeMock).toHaveBeenCalledWith("card_os_build", { request: BUILD });
  });

  // Round 4, task 4, R1: the same inputs `cardOsPrepare` takes, minus the
  // session and the free-space question — no `image` field, since nothing is
  // staged and nothing is written.
  it("measures with card_os_measure under the key request and returns the job id", async () => {
    invokeMock.mockResolvedValueOnce(43);
    expect(await cardOsMeasure(MEASURE)).toBe(43);
    expect(invokeMock).toHaveBeenCalledWith("card_os_measure", { request: MEASURE });
  });

  it("classifies with card_os_classify under the key paths", async () => {
    const answers: ClassifiedSource[] = [
      { path: "E:\\amiga\\Games", kind: { kind: "folder" }, why: null },
    ];
    invokeMock.mockResolvedValueOnce(answers);
    expect(await cardOsClassify(["E:\\amiga\\Games"])).toEqual(answers);
    expect(invokeMock).toHaveBeenCalledWith("card_os_classify", { paths: ["E:\\amiga\\Games"] });
  });

  it("checks a volume name with card_os_check_volume_name under the key name", async () => {
    const verdict: VolumeNameVerdict = { ok: true };
    invokeMock.mockResolvedValueOnce(verdict);
    expect(await cardOsCheckVolumeName("Games")).toEqual(verdict);
    expect(invokeMock).toHaveBeenCalledWith("card_os_check_volume_name", { name: "Games" });
  });
});

const MEASURE: CardOsMeasureRequest = {
  cardGb: 16,
  tree: "E:\\tmp\\card-os-1-1\\tree",
  partitions: [{ volumeName: "Games", sources: ["E:\\amiga\\Games"] }],
  material: ["E:\\amiga\\material"],
};

describe("the card session events", () => {
  it("listens for a build's every ending on card-os-build-result", async () => {
    let deliver: ((event: { payload: unknown }) => void) | undefined;
    listenMock.mockImplementationOnce(async (_name: string, cb: typeof deliver) => {
      deliver = cb;
      return () => {};
    });
    const seen: CardOsBuildResult[] = [];

    await onCardOsBuildResult((result) => seen.push(result));

    expect(CARD_OS_BUILD_EVENT).toBe("card-os-build-result");
    expect(listenMock).toHaveBeenCalledWith("card-os-build-result", expect.any(Function));
    const stopped = { ending: { ending: "stopped", phase: "partitions" } };
    deliver?.({ payload: stopped });
    expect(seen).toEqual([stopped]);
  });

  // Card round 3, I3: a refusal is its own ending, beside succeeded, failed
  // and stopped — the screen must be able to tell "choose another name" from
  // "build again". The literal is typed against the Rust mirror, so a shape
  // the type does not carry fails `pnpm lint` (tsconfig.test.json).
  it("delivers a refusal as its own ending, with the partial not created", async () => {
    let deliver: ((event: { payload: unknown }) => void) | undefined;
    listenMock.mockImplementationOnce(async (_name: string, cb: typeof deliver) => {
      deliver = cb;
      return () => {};
    });
    const seen: CardOsBuildResult[] = [];
    await onCardOsBuildResult((result) => seen.push(result));

    const ending: CardOsEnding = {
      ending: "refused",
      phase: "card",
      code: "ART-SAFETY-REFUSED",
      message: "'E:\\cards\\card.img' already exists",
      params: { path: "E:\\cards\\card.img" },
    };
    const partial: PartialRemoval = { outcome: "not-created" };
    const gone: PartialRemoval = { outcome: "already-gone", path: "E:\\cards\\card.img.partial" };
    const endings: CardOsEnding["ending"][] = ["succeeded", "refused", "failed", "stopped"];
    deliver?.({ payload: { ending, partial } });

    expect(seen).toEqual([{ ending, partial }]);
    expect(new Set(endings).size).toBe(4);
    expect(gone.outcome).toBe("already-gone");
  });

  it("listens for a preparation on card-os-prepare-result", async () => {
    listenMock.mockResolvedValueOnce(() => {});
    await onCardOsPrepareResult(() => {});
    expect(CARD_OS_PREPARE_EVENT).toBe("card-os-prepare-result");
    expect(listenMock).toHaveBeenCalledWith("card-os-prepare-result", expect.any(Function));
  });

  // Round 4, Task 2: the build says which phase it is in and how far — a
  // typed event, not a freeform message, so the screen can draw a count.
  it("listens for a phase-and-count on card-os-phase", async () => {
    let deliver: ((event: { payload: unknown }) => void) | undefined;
    listenMock.mockImplementationOnce(async (_name: string, cb: typeof deliver) => {
      deliver = cb;
      return () => {};
    });
    const seen: CardOsPhaseEvent[] = [];

    await onCardOsPhase((event) => seen.push(event));

    expect(CARD_OS_PHASE_EVENT).toBe("card-os-phase");
    expect(listenMock).toHaveBeenCalledWith("card-os-phase", expect.any(Function));

    // `total: null` is the bar rule: a phase that cannot know its total is
    // drawn as a count, never a bar of a guessed width.
    const started: CardOsPhaseEvent = {
      jobId: 7,
      session: 3,
      phase: "whdload",
      done: 0,
      total: null,
      unit: "files",
    };
    const advanced: CardOsPhaseEvent = {
      jobId: 7,
      session: 3,
      phase: "partitions",
      done: 4812,
      total: 9216,
      unit: "files",
    };
    deliver?.({ payload: started });
    deliver?.({ payload: advanced });

    expect(seen).toEqual([started, advanced]);

    // The literal is typed against the Rust mirror: a `phase` the type does
    // not carry (or a five-phase set that drops "prepare") fails `pnpm lint`.
    const phases: CardOsPhaseName[] = ["whdload", "card", "partitions", "check", "prepare"];
    expect(new Set(phases).size).toBe(5);
  });

  // Round 4, task 4: a measurement answers a plan or a typed refusal on the
  // same event — never a bare string, and never both at once.
  it("listens for a measurement's plan or refusal on card-os-measure-result", async () => {
    let deliver: ((event: { payload: unknown }) => void) | undefined;
    listenMock.mockImplementationOnce(async (_name: string, cb: typeof deliver) => {
      deliver = cb;
      return () => {};
    });
    const seen: CardOsMeasureResult[] = [];

    await onCardOsMeasureResult((result) => seen.push(result));

    expect(CARD_OS_MEASURE_EVENT).toBe("card-os-measure-result");
    expect(listenMock).toHaveBeenCalledWith("card-os-measure-result", expect.any(Function));

    const refused: CardOsMeasureResult = {
      jobId: 9,
      measured: null,
      refusal: {
        code: "ART-CARD-DOES-NOT-FIT",
        message: "a 1 GB card is too small to hold anything",
        params: { cardGb: "1" },
      },
    };
    deliver?.({ payload: refused });

    expect(seen).toEqual([refused]);
  });
});

// Round 4, task 4, Q6: the same typed reasons `card_os_classify` answers a
// dropped path with, mirroring `core::error::UnusableSource`.
describe("classify's typed reasons", () => {
  it("names every reason the literal must cover", () => {
    const reasons: UnusableSource[] = [
      { reason: "missing" },
      { reason: "unreadable", detail: "x" },
      { reason: "archive-unreadable", detail: "x" },
      { reason: "hardfile-not-whdload", detail: "x" },
      { reason: "not-an-amiga-source", formatHint: "unknown" },
      { reason: "not-a-folder" },
    ];
    expect(new Set(reasons.map((r) => r.reason)).size).toBe(6);
  });
});
