// @vitest-environment jsdom
//
// Which media layers a release declares — one `osinstall_layers` round trip
// and the two-causes rule it exists to settle.
//
// This effect was `useInstallPlan`'s until round 4 task 5's fix round, and it
// is here because `FilesTab` needs it and needs nothing else of that hook:
// the folder column offers a layer tag per row, and the plan behind it walks
// every ADF in the material list for an answer tab 1 never reads. Both
// callers go through this one implementation, so `layersKnown` cannot mean
// two things.
//
// Mocked at the `@/lib/*` wrapper around `invoke`, the boundary this suite
// mocks at everywhere.

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, renderHook, waitFor } from "@testing-library/react";

import type { InstallLayer } from "@/lib/osinstall";

const layersForMock = vi.hoisted(() => vi.fn());

vi.mock("@/lib/osinstall", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/lib/osinstall")>()),
  layersFor: layersForMock,
}));

const { useLayers } = await import("@/lib/useLayers");

/** AmigaOS 3.2.2's own two, which is the only shipped recipe that declares
 *  any — every other release answers `[]`, and that is what makes the second
 *  case below a real distinction rather than a hypothetical one. */
const LAYERS_322: InstallLayer[] = [
  { id: "base", labelKey: "osinstall.layer.base32" },
  { id: "update-3.2.2", labelKey: "osinstall.layer.update322" },
];

beforeEach(() => {
  layersForMock.mockReset().mockResolvedValue([]);
});

afterEach(() => cleanup());

describe("useLayers", () => {
  it("answers the release's own layers, asked once", async () => {
    layersForMock.mockResolvedValue(LAYERS_322);
    const { result } = renderHook(() => useLayers("AmigaOS 3.2.2"));

    await waitFor(() => expect(result.current.layersKnown).toBe(true));
    // In the recipe's own order — a hook handing them back reversed would
    // still pass a contains check, and the folder row's select draws them in
    // this order.
    expect(result.current.layers.map((l) => l.id)).toEqual(["base", "update-3.2.2"]);
    expect(layersForMock.mock.calls).toEqual([["AmigaOS 3.2.2"]]);
  });

  it("says the answer is not this release's until it lands (ART-256)", async () => {
    // `layers === []` is one value with two causes — "this release is
    // unlayered" and "nobody has asked yet" — and a caller scoping itself
    // with `layers.length > 0` reads a layered release as unlayered for as
    // long as the round trip takes. That is how a folder a layered release
    // never sends came to be scanned.
    //
    // Held open deliberately, so the state under test is the one before the
    // answer rather than a race: an unresolved promise, then resolved by hand.
    let answer: (ls: InstallLayer[]) => void = () => {};
    layersForMock.mockReturnValue(
      new Promise<InstallLayer[]>((resolve) => {
        answer = resolve;
      })
    );
    const { result } = renderHook(() => useLayers("AmigaOS 3.2.2"));

    // Both halves, because `[]` on its own is the very ambiguity this field
    // exists to remove.
    expect(result.current.layers).toEqual([]);
    expect(result.current.layersKnown).toBe(false);

    answer(LAYERS_322);
    await waitFor(() => expect(result.current.layersKnown).toBe(true));
    expect(result.current.layers).toEqual(LAYERS_322);
  });

  it("drops a superseded release's answer rather than letting it land late", async () => {
    // The switch a user makes while a lookup is in flight. If the first
    // release's answer were allowed to land afterwards, a screen would offer
    // AmigaOS 3.2.2's two layer tags on a release that declares none — a
    // setting the user did not choose, arriving after they chose otherwise
    // (ART-089's own mechanism).
    let answerFirst: (ls: InstallLayer[]) => void = () => {};
    layersForMock.mockReturnValueOnce(
      new Promise<InstallLayer[]>((resolve) => {
        answerFirst = resolve;
      })
    );
    const { result, rerender } = renderHook(({ release }: { release: string }) => useLayers(release as never), {
      initialProps: { release: "AmigaOS 3.2.2" },
    });

    layersForMock.mockResolvedValue([]);
    rerender({ release: "AmigaOS 3.9" });
    await waitFor(() => expect(result.current.layersKnown).toBe(true));

    // The first release's answer arrives now, and must change nothing.
    answerFirst(LAYERS_322);
    await new Promise((resolve) => setTimeout(resolve, 20));
    expect(result.current.layers).toEqual([]);
    expect(result.current.layersKnown).toBe(true);
  });

  it("treats a release with no shipped recipe as unlayered, not as unanswered", async () => {
    // A caller waiting for an answer that will never come waits for ever, so
    // a rejection is `[]` **for this release** rather than a state left
    // pending — the distinction is `layersKnown`, which must go true.
    layersForMock.mockRejectedValue(new Error("no recipe for that release"));
    const { result } = renderHook(() => useLayers("AmigaOS 3.9"));

    await waitFor(() => expect(result.current.layersKnown).toBe(true));
    expect(result.current.layers).toEqual([]);
  });
});
