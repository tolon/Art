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
  CARD_OS_PREPARE_EVENT,
  cardOsBuild,
  cardOsClose,
  cardOsOpen,
  cardOsPrepare,
  onCardOsBuildResult,
  onCardOsPrepareResult,
  type CardOsBuildRequest,
  type CardOsBuildResult,
  type CardOsPrepareRequest,
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
});

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

  it("listens for a preparation on card-os-prepare-result", async () => {
    listenMock.mockResolvedValueOnce(() => {});
    await onCardOsPrepareResult(() => {});
    expect(CARD_OS_PREPARE_EVENT).toBe("card-os-prepare-result");
    expect(listenMock).toHaveBeenCalledWith("card-os-prepare-result", expect.any(Function));
  });
});
