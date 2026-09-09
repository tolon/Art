// @vitest-environment jsdom
//
// One question — "what Kickstart is this file" — asked from one place, so
// tab 1 (which plans) and tab 3 (which shows the field) cannot answer it
// differently. The two outcomes stay two: identified, or unreadable.
// A superseded answer is dropped (ART-089's own mechanism).

import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, renderHook, waitFor } from "@testing-library/react";

const identifyMock = vi.hoisted(() => vi.fn());
vi.mock("@/lib/pistorm", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/lib/pistorm")>()),
  pistormIdentifyRom: identifyMock,
}));

const { useRomIdentity } = await import("@/lib/useRomIdentity");

afterEach(() => {
  cleanup();
  identifyMock.mockReset();
});

describe("useRomIdentity", () => {
  it("answers nothing, and asks nothing, without a path", () => {
    const { result } = renderHook(() => useRomIdentity(null));
    expect(result.current).toEqual({ rom: null, unreadable: false });
    expect(identifyMock).not.toHaveBeenCalled();
  });

  it("identifies the file, and clears the answer when the path goes", async () => {
    identifyMock.mockResolvedValue({ name: "Kickstart 3.2 (47.96)" });
    const { result, rerender } = renderHook(({ p }) => useRomIdentity(p), {
      initialProps: { p: "E:\\rom\\kick.rom" as string | null },
    });
    await waitFor(() => expect(result.current.rom?.name).toBe("Kickstart 3.2 (47.96)"));
    expect(result.current.unreadable).toBe(false);
    rerender({ p: null });
    expect(result.current).toEqual({ rom: null, unreadable: false });
  });

  it("says unreadable when the core refuses, never both", async () => {
    identifyMock.mockRejectedValue(new Error("not a Kickstart"));
    const { result } = renderHook(() => useRomIdentity("E:\\rom\\junk.bin"));
    await waitFor(() => expect(result.current.unreadable).toBe(true));
    expect(result.current.rom).toBeNull();
  });

  it("drops a superseded answer", async () => {
    let resolveFirst: (v: { name: string }) => void = () => {};
    identifyMock
      .mockImplementationOnce(() => new Promise((r) => { resolveFirst = r; }))
      .mockResolvedValueOnce({ name: "second" });
    const { result, rerender } = renderHook(({ p }) => useRomIdentity(p), {
      initialProps: { p: "E:\\a.rom" as string | null },
    });
    rerender({ p: "E:\\b.rom" });
    await waitFor(() => expect(result.current.rom?.name).toBe("second"));
    resolveFirst({ name: "first" });
    await new Promise((r) => setTimeout(r, 0));
    expect(result.current.rom?.name).toBe("second");
  });
});
