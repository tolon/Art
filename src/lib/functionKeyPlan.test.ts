// Covers `planFunctionKeys` directly, without rendering `FileManager.tsx` —
// same reasoning as `selection.test.ts`, whose header comment explains why a
// full render is a rabbit hole here.
//
// This is the regression net finding 4 of the phase-1a whole-branch review
// asked for: with two entries selected, F3/F4/F6/F9 must refuse rather than
// act on an arbitrary one of the two; with exactly one, each must be enabled
// and target that entry. Before this file existed, nothing would have failed
// if one of those `run` closures in `FileManager.tsx` had been changed to
// pick `selectedEntries(focused)[0]` instead of the selection-aware target —
// acting on an arbitrary entry when several are selected, which renames or
// deletes the wrong file.
import { describe, expect, it } from "vitest";

import { planFunctionKeys } from "./functionKeyPlan";
import type { PanelEntry } from "./panel";

function entry(name: string, overrides: Partial<PanelEntry> = {}): PanelEntry {
  return {
    name,
    is_dir: false,
    bytes: 100,
    path: null,
    header_block: 42,
    is_link: false,
    date: null,
    attrs: null,
    ...overrides,
  };
}

const ENTRIES = [entry("Alpha"), entry("Beta"), entry("Gamma")];

/** The permissive baseline: in a writable volume, nothing busy. Individual
 * tests narrow one flag at a time to prove each key actually reads it. */
function permissive(selected: Set<string>) {
  return planFunctionKeys({
    entries: ENTRIES,
    selected,
    inVolume: true,
    canWrite: true,
    busy: false,
  });
}

describe("planFunctionKeys — the single-entry keys refuse a multi-selection", () => {
  it("with two entries selected, F3/F4/F6/F9 are all refused and target nothing", () => {
    const plan = permissive(new Set(["Alpha", "Beta"]));

    expect(plan.multipleSelected).toBe(true);
    expect(plan.single).toBeNull();

    expect(plan.f3).toEqual({ enabled: false, target: null });
    expect(plan.f4).toEqual({ enabled: false, target: null });
    expect(plan.f6).toEqual({ enabled: false, target: null });
    expect(plan.f9).toEqual({ enabled: false, target: null });

    // F5/F8 act on the whole selection — the opposite rule — so this
    // shape must not have accidentally disabled them too.
    expect(plan.hasSelection).toBe(true);
  });

  it("with three entries selected, the refusal holds the same way", () => {
    const plan = permissive(new Set(["Alpha", "Beta", "Gamma"]));
    expect(plan.f3.enabled).toBe(false);
    expect(plan.f4.enabled).toBe(false);
    expect(plan.f6.enabled).toBe(false);
    expect(plan.f9.enabled).toBe(false);
  });

  it("with nothing selected, the same four are refused and hasSelection is false", () => {
    const plan = permissive(new Set());
    expect(plan.multipleSelected).toBe(false);
    expect(plan.single).toBeNull();
    expect(plan.hasSelection).toBe(false);
    expect(plan.f3.enabled).toBe(false);
    expect(plan.f4.enabled).toBe(false);
    expect(plan.f6.enabled).toBe(false);
    expect(plan.f9.enabled).toBe(false);
  });
});

describe("planFunctionKeys — exactly one entry selected", () => {
  it("enables all four and every one targets that entry, not an arbitrary one", () => {
    const plan = permissive(new Set(["Beta"]));

    expect(plan.multipleSelected).toBe(false);
    expect(plan.single?.name).toBe("Beta");

    expect(plan.f3).toEqual({ enabled: true, target: plan.single });
    expect(plan.f4).toEqual({ enabled: true, target: plan.single });
    expect(plan.f6).toEqual({ enabled: true, target: plan.single });
    expect(plan.f9).toEqual({ enabled: true, target: plan.single });
  });

  it("F3 and F4 refuse a directory even as the sole selection; F6 and F9 do not care", () => {
    const entries = [entry("Alpha"), entry("Tools", { is_dir: true })];
    const plan = planFunctionKeys({
      entries,
      selected: new Set(["Tools"]),
      inVolume: true,
      canWrite: true,
      busy: false,
    });

    expect(plan.f3.enabled).toBe(false);
    expect(plan.f4.enabled).toBe(false);
    expect(plan.f6).toEqual({ enabled: true, target: plan.single });
    expect(plan.f9).toEqual({ enabled: true, target: plan.single });
  });

  it("F3 and F9 refuse outside a volume, even with one entry selected", () => {
    const plan = planFunctionKeys({
      entries: ENTRIES,
      selected: new Set(["Beta"]),
      inVolume: false,
      canWrite: true,
      busy: false,
    });
    expect(plan.f3.enabled).toBe(false);
    expect(plan.f9.enabled).toBe(false);
    // F4/F6 do not depend on inVolume, only on write capability.
    expect(plan.f4.enabled).toBe(true);
    expect(plan.f6.enabled).toBe(true);
  });

  it("F4 and F6 refuse on a read-only volume, even with one entry selected", () => {
    const plan = planFunctionKeys({
      entries: ENTRIES,
      selected: new Set(["Beta"]),
      inVolume: true,
      canWrite: false,
      busy: false,
    });
    expect(plan.f4.enabled).toBe(false);
    expect(plan.f6.enabled).toBe(false);
    // F3/F9 do not depend on write capability.
    expect(plan.f3.enabled).toBe(true);
    expect(plan.f9.enabled).toBe(true);
  });

  it("F4 and F6 wait for a busy operation to finish; F3/F9 do not", () => {
    const plan = planFunctionKeys({
      entries: ENTRIES,
      selected: new Set(["Beta"]),
      inVolume: true,
      canWrite: true,
      busy: true,
    });
    expect(plan.f4.enabled).toBe(false);
    expect(plan.f6.enabled).toBe(false);
    expect(plan.f3.enabled).toBe(true);
    expect(plan.f9.enabled).toBe(true);
  });
});
