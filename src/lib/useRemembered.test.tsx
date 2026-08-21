// @vitest-environment jsdom
//
// ART-195. A remembered value whose *identity* changes on every render is a
// dependency array that never settles — and this screen's dependency arrays
// start disk work.
//
// `recall` returns the caller's `fallback` when nothing is stored, and every
// call site spells that fallback inline (`useRemembered(key, isTextList, [])`).
// A fresh `[]` per render meant `useEffect`'s `Object.is` comparison never
// matched, so the OS Builder's plan effect re-ran on every render, each run
// planned — three full walks of a 468 MB ISO — and each new plan set state,
// which rendered again. Preview jobs piled up without bound, their counts
// falling as *different* jobs replaced each other on screen, and the Stop
// button appeared to start a new one because settling the cancelled preview's
// promise was itself a state change.
//
// `recallInto` is worse still: it builds a new object every call, so
// `useRememberedShape` never returned a stable identity even when a value
// *was* stored.
//
// These tests render real hooks and count effect runs. jsdom, hence `.tsx`.

import { renderHook, act } from "@testing-library/react";
import { useEffect, useRef, useState } from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { isTextList, isFlag, isText } from "@/lib/remembered";
import { useRemembered, useRememberedShape } from "@/lib/useRemembered";
import { useSettingsStore } from "@/stores/settingsStore";

vi.mock("@/lib/settings", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/lib/settings")>()),
  getSettings: vi.fn().mockResolvedValue({}),
  saveSettings: vi.fn().mockResolvedValue(undefined),
}));

beforeEach(() => {
  useSettingsStore.setState((s) => ({ ...s, settings: { ...s.settings, remembered: {} } }));
});

/**
 * The exact shape `OsInstall.tsx` has: a remembered list with an inline `[]`
 * fallback, feeding an effect that does expensive work, plus a piece of state
 * the effect itself sets — which is what makes the loop close.
 */
const RUN_CEILING = 25;

function useScreenLikeOsInstall(key: string) {
  const [chosen] = useRemembered<string[]>(key, isTextList, []);
  const [, setPlan] = useState<object | null>(null);
  const runs = useRef(0);

  useEffect(() => {
    runs.current += 1;
    // Every plan answer is a fresh object off the IPC bridge, so setting it
    // always re-renders — exactly as the real screen does. The ceiling is
    // only so that a regression hangs a test for a moment rather than for
    // ever: React puts no limit on an effect that re-triggers itself, which
    // is precisely why this loop ran 2,149 times on the owner's machine
    // instead of throwing.
    if (runs.current < RUN_CEILING) setPlan({ freshEveryTime: true });
  }, [chosen]);

  return runs;
}

describe("a remembered value's identity", () => {
  it("does not change on re-render when nothing is stored, so an effect depending on it settles", () => {
    const { result } = renderHook(() => useScreenLikeOsInstall("art.test.nothing-stored"));

    // One run for the mount. If the fallback identity were fresh per render,
    // the effect's own `setPlan` would re-render, the dependency would compare
    // unequal again, and React would keep going until it gave up — the
    // unbounded loop ART-195 is.
    expect(result.current.current).toBe(1);
  });

  it("does not change on re-render when a value *is* stored", () => {
    useSettingsStore.setState((s) => ({
      ...s,
      settings: { ...s.settings, remembered: { "art.test.stored": ["workbench-base"] } },
    }));
    const { result } = renderHook(() => useScreenLikeOsInstall("art.test.stored"));
    expect(result.current.current).toBe(1);
  });

  it("still changes when the value itself changes, so the effect does run again", () => {
    const { result } = renderHook(() => useScreenLikeOsInstall("art.test.changes"));
    expect(result.current.current).toBe(1);

    act(() => {
      useSettingsStore.setState((s) => ({
        ...s,
        settings: { ...s.settings, remembered: { "art.test.changes": ["workbench-39"] } },
      }));
    });

    // A settled identity must not mean a frozen one: the whole point of the
    // dependency is that a real change re-plans.
    expect(result.current.current).toBe(2);
  });

  it("does not re-run the effect when the persisted read lands holding the default (ART-178)", () => {
    // ART-178's own scenario, exactly: the first render returns the inline
    // fallback, and the asynchronous read then lands with a value that is
    // *structurally equal* to it. The old code handed back a second, different
    // array and every dependent effect ran again with a byte-identical
    // request — for the OS Builder, a second full walk of every switched-on
    // component's disc image.
    const { result } = renderHook(() => useScreenLikeOsInstall("art.test.lands-equal"));
    expect(result.current.current).toBe(1);

    act(() => {
      useSettingsStore.setState((s) => ({
        ...s,
        settings: { ...s.settings, remembered: { "art.test.lands-equal": [] } },
      }));
    });

    expect(result.current.current).toBe(1);
  });

  it("hands back a value equal to what was stored, not merely a stable one", () => {
    useSettingsStore.setState((s) => ({
      ...s,
      settings: { ...s.settings, remembered: { "art.test.value": ["a", "b"] } },
    }));
    const { result } = renderHook(() =>
      useRemembered<string[]>("art.test.value", isTextList, [])
    );
    expect(result.current[0]).toEqual(["a", "b"]);
  });

  it("is per key: two keys do not share one stabilised value", () => {
    useSettingsStore.setState((s) => ({
      ...s,
      settings: {
        ...s.settings,
        remembered: { "art.test.k1": ["one"], "art.test.k2": ["two"] },
      },
    }));
    const { result } = renderHook(() => {
      const [a] = useRemembered<string[]>("art.test.k1", isTextList, []);
      const [b] = useRemembered<string[]>("art.test.k2", isTextList, []);
      return [a, b] as const;
    });
    expect(result.current[0]).toEqual(["one"]);
    expect(result.current[1]).toEqual(["two"]);
  });
});

describe("a remembered shape's identity", () => {
  const SPEC = { on: isFlag, name: isText };
  const FALLBACK = { on: false, name: "" };

  function useShapeScreen() {
    const [shape] = useRememberedShape("art.test.shape", SPEC, FALLBACK);
    const [, setTick] = useState<object | null>(null);
    const runs = useRef(0);
    useEffect(() => {
      runs.current += 1;
      if (runs.current < RUN_CEILING) setTick({ freshEveryTime: true });
    }, [shape]);
    return runs;
  }

  it("does not change on re-render, though `recallInto` rebuilds the object every call", () => {
    const { result } = renderHook(() => useShapeScreen());
    expect(result.current.current).toBe(1);
  });

  it("still changes when a field changes", () => {
    const { result } = renderHook(() => useShapeScreen());
    expect(result.current.current).toBe(1);
    act(() => {
      useSettingsStore.setState((s) => ({
        ...s,
        settings: { ...s.settings, remembered: { "art.test.shape": { on: true, name: "x" } } },
      }));
    });
    expect(result.current.current).toBe(2);
  });
});
