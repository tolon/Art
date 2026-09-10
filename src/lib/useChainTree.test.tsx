// @vitest-environment jsdom
//
// One tree for both tabs (round 3 task 3, fix round 1, Important 2).
//
// The rule is four lines long and it decides which folder every *installed*
// sentence in the wizard is about, so it is asserted here rather than through
// either of the two screens that read it: a rule tested at one of its two
// call sites is a rule the other one can quietly stop obeying.
//
// Mocked at the usual boundary — the `@/lib/osinstall` wrappers around
// `invoke`, never `@tauri-apps/api` — and `@/lib/settings`, because the
// settings store's own writer rejects in jsdom with nothing to catch it.

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, renderHook, waitFor } from "@testing-library/react";

import type { TreeSummary } from "@/lib/osinstall";
import { useSettingsStore } from "@/stores/settingsStore";

const describeTreeMock = vi.hoisted(() => vi.fn());
const destinationTakenMock = vi.hoisted(() => vi.fn());

vi.mock("@/lib/osinstall", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/lib/osinstall")>()),
  osinstallDescribeTree: describeTreeMock,
  osinstallDestinationTaken: destinationTakenMock,
}));

vi.mock("@/lib/settings", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/lib/settings")>()),
  getSettings: vi.fn(),
  saveSettings: vi.fn().mockResolvedValue(undefined),
}));

const { useChainTree } = await import("@/lib/useChainTree");
const { DEFAULT_SETTINGS } = await import("@/lib/settings");

const A_TREE: TreeSummary = {
  isTree: true,
  release: "AmigaOS 3.9",
  files: 4212,
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

/** The session's own tree, as every screen had it before this hook. */
function seedSessionTree(root: string | null) {
  useSettingsStore.setState({
    loaded: true,
    settings: {
      ...DEFAULT_SETTINGS,
      remembered: root ? { "buildSession.tree": { root, builtHere: false } } : {},
    },
  });
}

beforeEach(() => {
  describeTreeMock.mockReset().mockResolvedValue(NOT_A_TREE);
  destinationTakenMock.mockReset().mockResolvedValue(false);
  useSettingsStore.setState({ loaded: false, settings: DEFAULT_SETTINGS });
});

afterEach(cleanup);

describe("useChainTree", () => {
  it("takes the destination when ART has looked at it and found a build", async () => {
    // The update run's own case: the user is building into a folder that
    // already holds a distribution, so *this* is the tree every row's
    // installed/outstanding state is about — not whatever the session was
    // last pointed at.
    describeTreeMock.mockResolvedValue(A_TREE);
    seedSessionTree("E:\\some\\other\\tree");

    const { result } = renderHook(() => useChainTree("E:\\dist39"));

    await waitFor(() => expect(result.current.settled).toBe(true));
    expect(result.current.treeRoot).toBe("E:\\dist39");
  });

  it("takes the session's own tree when the destination is not a build", async () => {
    // A fresh build into an empty folder. The session's tree is what tab 1's
    // readout and the Amiga-side panel have always been handed, and both tabs
    // now say the same thing about it.
    seedSessionTree("E:\\amiga\\os39");

    const { result } = renderHook(() => useChainTree("E:\\empty"));

    await waitFor(() => expect(result.current.settled).toBe(true));
    expect(result.current.treeRoot).toBe("E:\\amiga\\os39");
  });

  it("is not settled while ART is still looking at the destination", async () => {
    // The flicker this field exists to stop: a destination that *is* a build
    // looks like one that is not until the round trip lands, which is long
    // enough to ask the chain about the wrong tree and put *ready* on a row
    // the tree already carries.
    let answer: (summary: TreeSummary) => void = () => {};
    describeTreeMock.mockReturnValue(
      new Promise<TreeSummary>((resolve) => {
        answer = resolve;
      })
    );
    seedSessionTree("E:\\amiga\\os39");

    const { result } = renderHook(() => useChainTree("E:\\dist39"));

    expect(result.current.settled).toBe(false);
    // …and the value it is carrying meanwhile is the *old* answer, never a
    // guess at the new one.
    expect(result.current.treeRoot).toBe("E:\\amiga\\os39");

    answer(A_TREE);
    await waitFor(() => expect(result.current.settled).toBe(true));
    expect(result.current.treeRoot).toBe("E:\\dist39");
  });

  it("is settled from the first render when there is no destination to look at", async () => {
    // Nothing to wait for. A caller that waited anyway would never ask the
    // chain at all for a user who has not chosen a destination yet — and the
    // chain has plenty to say about material with no tree in sight.
    seedSessionTree("E:\\amiga\\os39");

    const { result } = renderHook(() => useChainTree(null));

    expect(result.current.settled).toBe(true);
    expect(result.current.treeRoot).toBe("E:\\amiga\\os39");
    expect(describeTreeMock).not.toHaveBeenCalled();
  });

  it("settles on the session's own tree when the look itself failed", async () => {
    // **A dead IPC call must not silence the tab** (fix round 2). `tree`
    // stays `null` after a rejection — the same `null` it holds before ART
    // has looked — so a `settled` that watched the value would never come
    // true and tab 2 would ask the chain nothing, for ever. Falling back to
    // the tree the session already carries is the honest answer: it is what
    // every screen used before this hook existed.
    describeTreeMock.mockRejectedValue(new Error("no IPC bridge"));
    destinationTakenMock.mockRejectedValue(new Error("no IPC bridge"));
    seedSessionTree("E:\\amiga\\os39");

    const { result } = renderHook(() => useChainTree("E:\\dist39"));

    await waitFor(() => expect(result.current.settled).toBe(true));
    expect(result.current.treeRoot).toBe("E:\\amiga\\os39");
  });

  it("uses the caller's own answer, and asks nothing of its own for it", async () => {
    // `FilesTab` already asks `useDestinationCheck` for the destination
    // refusal. Handing that answer over is what keeps one screen to one round
    // trip per path — two identical `osinstall_describe_tree` calls would be
    // a second reader of the one fact this hook exists to have one of.
    seedSessionTree("E:\\amiga\\os39");

    const { result } = renderHook(() =>
      useChainTree("E:\\dist39", { taken: false, tree: A_TREE, looked: true })
    );

    expect(result.current).toEqual({
      treeRoot: "E:\\dist39",
      settled: true,
      source: "destination",
    });
    expect(describeTreeMock).not.toHaveBeenCalled();
    expect(destinationTakenMock).not.toHaveBeenCalled();
  });

  it("waits on the caller's answer exactly as it waits on its own", async () => {
    // The handed-over check carries `looked` too, so a screen that passes one
    // in mid-flight gets the same gate — never a treeRoot presented as
    // settled because it came from somewhere else.
    seedSessionTree("E:\\amiga\\os39");

    const { result } = renderHook(() =>
      useChainTree("E:\\dist39", { taken: false, tree: null, looked: false })
    );

    expect(result.current.settled).toBe(false);
    expect(result.current.treeRoot).toBe("E:\\amiga\\os39");
  });

  it("answers no tree at all rather than a path, when neither is set", async () => {
    seedSessionTree(null);

    const { result } = renderHook(() => useChainTree(null));

    expect(result.current.settled).toBe(true);
    expect(result.current.treeRoot).toBeNull();
  });
});

/**
 * **`source` says which of the two rules produced the path** (round 3 fix
 * wave, Important 2). `AmigaInstallPanel`'s own tree Browse wrote the
 * session's tree and watched it snap back to the destination; a screen that
 * owns such a field needs to know whether its field is the value on screen,
 * and comparing the two strings would answer "destination" for a session tree
 * that happens to equal it.
 */
describe("useChainTree says where the tree came from", () => {
  it("attributes it to the destination when that is what won", async () => {
    describeTreeMock.mockResolvedValue(A_TREE);
    seedSessionTree("E:\\some\\other\\tree");

    const { result } = renderHook(() => useChainTree("E:\\dist39"));

    await waitFor(() => expect(result.current.settled).toBe(true));
    expect(result.current.source).toBe("destination");
  });

  it("attributes it to the session when the destination is not a build", async () => {
    seedSessionTree("E:\\amiga\\os39");

    const { result } = renderHook(() => useChainTree("E:\\empty"));

    await waitFor(() => expect(result.current.settled).toBe(true));
    expect(result.current.source).toBe("session");
  });

  it("says the session even when its tree is the same folder as the destination", async () => {
    // The reason this is a field and not a string comparison: the rule that
    // produced the value is what the caller needs, and here the destination
    // is *not* a build ART recognises — the panel's own Browse still works.
    seedSessionTree("E:\\dist39");

    const { result } = renderHook(() => useChainTree("E:\\dist39"));

    await waitFor(() => expect(result.current.settled).toBe(true));
    expect(result.current.treeRoot).toBe("E:\\dist39");
    expect(result.current.source).toBe("session");
  });

  it("says neither when there is no tree at all", async () => {
    seedSessionTree(null);

    const { result } = renderHook(() => useChainTree(null));

    expect(result.current.source).toBe("none");
  });
});
