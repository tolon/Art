// @vitest-environment jsdom
//
// The two silences the final review called blockers, pinned.
//
// The screen takes a content folder **per partition** and the plan emits a
// `copy-in` for each, but the pairing asked about the first filled one only.
// So a `dist-3.2b` tree needing Kickstart V47 on DH1 went onto a V40 card
// with nothing rendered above the destructive confirmation, because DH0's own
// folder happened to be paired — or, worse, because DH0's folder was a
// staging tree with no `distribution.json` and the screen said so instead, a
// sentence about the one folder that was not at risk.
//
// And a rejecting command was indistinguishable from "checked, and fine":
// both cleared the verdict to nothing.

import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, renderHook, waitFor } from "@testing-library/react";

import type { Pairing, PartitionPick } from "@/lib/preload";

const romPairing = vi.hoisted(() => vi.fn());

vi.mock("@/lib/preload", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/lib/preload")>()),
  preloadRomPairing: romPairing,
}));

const { useRomPairing } = await import("@/lib/useRomPairing");
const { pairingLines } = await import("@/lib/preload");

afterEach(() => {
  cleanup();
  romPairing.mockReset();
});

function pick(driveName: string, content: string | null, chosen = true): PartitionPick {
  return { area: 1, index: 1, driveName, chosen, volumeName: driveName, content };
}

const PAIRED: Pairing = { verdict: "paired" };
const UNSUITABLE: Pairing = { verdict: "unsuitable", needs: 47, found: 40, rom: "kick.rom" };
const NO_TREE: Pairing = { verdict: "not-checked", why: "tree-records-no-rom" };

describe("useRomPairing", () => {
  it("asks about every chosen folder, not just the first", async () => {
    romPairing.mockImplementation(async (_image: string, content: string) =>
      content.endsWith("v47-tree") ? UNSUITABLE : PAIRED
    );

    const { result } = renderHook(() =>
      useRomPairing("E:\\card.img", [
        pick("DH0", "E:\\staging"),
        pick("DH1", "E:\\v47-tree"),
      ])
    );

    await waitFor(() => expect(result.current.results).toHaveLength(2));
    expect(romPairing).toHaveBeenCalledTimes(2);
    expect(result.current.results.map((r) => r.driveName)).toEqual(["DH0", "DH1"]);
  });

  it("keeps a warning that a paired folder used to swallow, named by drive", async () => {
    romPairing.mockImplementation(async (_image: string, content: string) =>
      content.endsWith("v47-tree") ? UNSUITABLE : PAIRED
    );

    const { result } = renderHook(() =>
      useRomPairing("E:\\card.img", [
        pick("DH0", "E:\\v40-tree"),
        pick("DH1", "E:\\v47-tree"),
      ])
    );

    await waitFor(() => expect(result.current.results).toHaveLength(2));
    const lines = pairingLines(result.current.results);
    expect(lines).toHaveLength(1);
    expect(lines[0].driveName).toBe("DH1");
    expect(lines[0].phrase.key).toBe("preload.pairing.unsuitable47");
  });

  it("never asks about a partition that is not chosen, or has no folder", async () => {
    romPairing.mockResolvedValue(NO_TREE);

    const { result } = renderHook(() =>
      useRomPairing("E:\\card.img", [
        pick("DH0", null),
        pick("DH1", "E:\\tree"),
        pick("DH2", "E:\\other", false),
      ])
    );

    await waitFor(() => expect(result.current.results).toHaveLength(1));
    expect(result.current.results[0].driveName).toBe("DH1");
    expect(romPairing).toHaveBeenCalledTimes(1);
  });

  it("says the check failed rather than falling silent", async () => {
    romPairing.mockRejectedValue(new Error("the command itself failed"));

    const { result } = renderHook(() =>
      useRomPairing("E:\\card.img", [pick("DH0", "E:\\tree")])
    );

    await waitFor(() => expect(result.current.checking).toBe(false));
    expect(result.current.results).toEqual([
      { driveName: "DH0", pairing: { verdict: "not-checked", why: "check-failed" } },
    ]);
    expect(pairingLines(result.current.results)[0].phrase.key).toBe(
      "preload.pairing.notChecked.failed"
    );
  });

  it("is checking while the answer is in flight, so silence is never ambiguous", async () => {
    let answer: (value: Pairing) => void = () => {};
    romPairing.mockReturnValue(
      new Promise<Pairing>((resolve) => {
        answer = resolve;
      })
    );

    const { result } = renderHook(() =>
      useRomPairing("E:\\card.img", [pick("DH0", "E:\\tree")])
    );

    await waitFor(() => expect(result.current.checking).toBe(true));
    answer(PAIRED);
    await waitFor(() => expect(result.current.checking).toBe(false));
    expect(pairingLines(result.current.results)).toEqual([]);
  });

  it("is not checking when there is nothing to ask about", async () => {
    const { result } = renderHook(() => useRomPairing("E:\\card.img", [pick("DH0", null)]));

    await waitFor(() => expect(result.current.results).toEqual([]));
    expect(result.current.checking).toBe(false);
    expect(romPairing).not.toHaveBeenCalled();
  });

  it("forgets a verdict the moment the folders it described change", async () => {
    romPairing.mockResolvedValue(UNSUITABLE);

    const { result, rerender } = renderHook(
      ({ picks }: { picks: PartitionPick[] }) => useRomPairing("E:\\card.img", picks),
      { initialProps: { picks: [pick("DH0", "E:\\tree")] } }
    );
    await waitFor(() => expect(result.current.results).toHaveLength(1));

    romPairing.mockReturnValue(new Promise<Pairing>(() => {}));
    rerender({ picks: [pick("DH0", "E:\\a-different-tree")] });
    expect(result.current.results).toEqual([]);
  });
});
