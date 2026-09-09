// @vitest-environment jsdom
//
// Two facts about the destination folder, asked once: is it occupied
// (`osinstall_destination_taken`, which is what `apply` would refuse on) and
// is it a tree ART built (`osinstall_describe_tree`). Both tabs read this;
// neither asks on its own. A path ART cannot examine is not declared taken
// — `apply()` decides, and blocking here would refuse an install the engine
// would have allowed.

import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, renderHook, waitFor } from "@testing-library/react";

const takenMock = vi.hoisted(() => vi.fn());
const describeMock = vi.hoisted(() => vi.fn());
vi.mock("@/lib/osinstall", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/lib/osinstall")>()),
  osinstallDestinationTaken: takenMock,
  osinstallDescribeTree: describeMock,
}));

const { useDestinationCheck } = await import("@/lib/useDestinationCheck");

const TREE = { isTree: true, release: "AmigaOS 3.9", files: 1915, components: ["workbench-base"], amigaInstalled: [], problem: null };
const NOT_TREE = { isTree: false, release: null, files: 0, components: [], amigaInstalled: [], problem: "no distribution.json" };

afterEach(() => {
  cleanup();
  takenMock.mockReset();
  describeMock.mockReset();
});

describe("useDestinationCheck", () => {
  it("asks nothing without a path", () => {
    const { result } = renderHook(() => useDestinationCheck(null));
    expect(result.current).toEqual({ taken: false, tree: null });
    expect(takenMock).not.toHaveBeenCalled();
    expect(describeMock).not.toHaveBeenCalled();
  });

  it("reports an occupied folder and a tree apart", async () => {
    takenMock.mockResolvedValue(true);
    describeMock.mockResolvedValue(TREE);
    const { result } = renderHook(() => useDestinationCheck("E:\\dist"));
    await waitFor(() => expect(result.current.taken).toBe(true));
    await waitFor(() => expect(result.current.tree?.isTree).toBe(true));
    expect(result.current.tree?.files).toBe(1915);
  });

  it("does not declare a folder taken when ART could not look", async () => {
    takenMock.mockRejectedValue(new Error("access is denied"));
    describeMock.mockResolvedValue(NOT_TREE);
    const { result } = renderHook(() => useDestinationCheck("E:\\dist"));
    await waitFor(() => expect(describeMock).toHaveBeenCalled());
    expect(result.current.taken).toBe(false);
  });

  it("keeps a non-tree's answer as not-a-tree, never null, once ART has looked", async () => {
    takenMock.mockResolvedValue(false);
    describeMock.mockResolvedValue(NOT_TREE);
    const { result } = renderHook(() => useDestinationCheck("E:\\empty"));
    await waitFor(() => expect(result.current.tree).not.toBeNull());
    expect(result.current.tree?.isTree).toBe(false);
  });

  it("clears both when the path goes", async () => {
    takenMock.mockResolvedValue(true);
    describeMock.mockResolvedValue(TREE);
    const { result, rerender } = renderHook(({ p }) => useDestinationCheck(p), {
      initialProps: { p: "E:\\dist" as string | null },
    });
    await waitFor(() => expect(result.current.taken).toBe(true));
    rerender({ p: null });
    expect(result.current).toEqual({ taken: false, tree: null });
  });

  it("asks again when the revision moves, so a folder an install just filled reads as taken", async () => {
    takenMock.mockResolvedValueOnce(false).mockResolvedValueOnce(true);
    describeMock.mockResolvedValue(NOT_TREE);
    const { result, rerender } = renderHook(({ rev }) => useDestinationCheck("E:\\dist", rev), {
      initialProps: { rev: null as unknown },
    });
    await waitFor(() => expect(takenMock).toHaveBeenCalledTimes(1));
    expect(result.current.taken).toBe(false);
    rerender({ rev: { finished: true } });
    await waitFor(() => expect(result.current.taken).toBe(true));
  });

  it("drops a superseded generation's answers, and never mixes two", async () => {
    let resolveFirstTree: (v: typeof TREE) => void = () => {};
    takenMock.mockResolvedValue(false);
    describeMock
      .mockImplementationOnce(() => new Promise((r) => { resolveFirstTree = r; }))
      .mockResolvedValueOnce(NOT_TREE);
    const { result, rerender } = renderHook(({ p }) => useDestinationCheck(p), {
      initialProps: { p: "E:\\a" as string | null },
    });
    rerender({ p: "E:\\b" });
    await waitFor(() => expect(result.current.tree?.isTree).toBe(false));
    resolveFirstTree(TREE);
    await new Promise((r) => setTimeout(r, 0));
    expect(result.current.tree?.isTree).toBe(false);
  });

  it("takes the previous answers down the moment the path changes", async () => {
    takenMock.mockResolvedValue(true);
    describeMock.mockResolvedValue(TREE);
    const { result, rerender } = renderHook(({ p }) => useDestinationCheck(p), {
      initialProps: { p: "E:\\a" as string | null },
    });
    await waitFor(() => expect(result.current.tree?.isTree).toBe(true));
    describeMock.mockImplementation(() => new Promise(() => {}));
    takenMock.mockImplementation(() => new Promise(() => {}));
    rerender({ p: "E:\\b" });
    expect(result.current).toEqual({ taken: false, tree: null });
  });
});
