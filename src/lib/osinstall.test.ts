// Pure logic pulled out of `OsInstall.tsx` in a fix round: a Critical defect
// (an excluded component's media staying in `mediaPaths`, so `apply()`'s
// manifest lied about what a tree was built from) shipped inside code no
// test could reach, because it lived only in a component. This file is the
// coverage that would have caught it, plus the parity check that keeps
// `AMIGAOS_32_COMPONENTS` from silently drifting away from the recipe it
// mirrors.

import { describe, expect, it } from "vitest";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";

import {
  AMIGAOS_32_COMPONENTS,
  componentDef,
  componentLabel,
  confirmComponentOff,
  conditionalReason,
  conditionalToggleAction,
  hasRomUnknownRefusal,
  isForcedOnByCondition,
  parseOptionalSlot,
  parsePartitionIndex,
  pruneStaleExclusions,
  sanitizeChosen,
  toggleChosen,
  withoutExcluded,
  type InstallPlan,
} from "@/lib/osinstall";

// ---------------------------------------------------------------------------
// Parity against the Rust recipe — review point 6 (fix round)
// ---------------------------------------------------------------------------

const RECIPE_PATH = resolve(
  __dirname,
  "..",
  "..",
  "src-tauri",
  "src",
  "core",
  "osinstall",
  "recipes",
  "amigaos-3.2.json"
);

interface RecipeComponent {
  id: string;
  media: string;
  required?: boolean;
  condition?: { condition: string; major?: number };
  exclusive_group?: string;
  available?: boolean;
}

interface Recipe {
  release: string;
  components: RecipeComponent[];
}

function recipe(): Recipe {
  return JSON.parse(readFileSync(RECIPE_PATH, "utf8")) as Recipe;
}

describe("AMIGAOS_32_COMPONENTS mirrors the shipped recipe", () => {
  it("has components to check", () => {
    // A recipe that failed to parse, or one emptied by a bad edit, would
    // make every assertion below vacuously true.
    expect(recipe().components.length).toBeGreaterThan(0);
  });

  it("names exactly the same ids, in the same order", () => {
    expect(AMIGAOS_32_COMPONENTS.map((c) => c.id)).toEqual(recipe().components.map((c) => c.id));
  });

  it("agrees on media, required, available, condition and exclusive_group for every id", () => {
    const mismatches: string[] = [];
    for (const rc of recipe().components) {
      const mirrored = componentDef(rc.id);
      if (!mirrored) {
        mismatches.push(`${rc.id}: missing from AMIGAOS_32_COMPONENTS entirely`);
        continue;
      }
      if (mirrored.media !== rc.media) {
        mismatches.push(`${rc.id}: media ${mirrored.media} !== ${rc.media}`);
      }
      const required = rc.required ?? false;
      if (mirrored.required !== required) {
        mismatches.push(`${rc.id}: required ${mirrored.required} !== ${required}`);
      }
      const available = rc.available ?? true;
      if (mirrored.available !== available) {
        mismatches.push(`${rc.id}: available ${mirrored.available} !== ${available}`);
      }
      const conditionMajor = rc.condition?.major ?? null;
      if (mirrored.conditionMajor !== conditionMajor) {
        mismatches.push(`${rc.id}: conditionMajor ${mirrored.conditionMajor} !== ${conditionMajor}`);
      }
      // ART-119 (#3): `ComponentDef.conditionMajor` mirrors one specific
      // variant, `Condition::RomOlderThan { major }` — its own doc comment
      // says so. Matching on `major` alone means a future condition kind
      // that also happens to carry a `major` field would pass this parity
      // check while `conditionalReason`'s callers still assumed
      // rom-older-than and rendered "below Kickstart V47" regardless.
      if (rc.condition && rc.condition.condition !== "rom-older-than") {
        mismatches.push(
          `${rc.id}: condition kind is "${rc.condition.condition}", not "rom-older-than" — ` +
            `ComponentDef.conditionMajor only mirrors rom-older-than and the screen only knows how to explain that one`
        );
      }
      const exclusiveGroup = rc.exclusive_group ?? null;
      if (mirrored.exclusiveGroup !== exclusiveGroup) {
        mismatches.push(`${rc.id}: exclusiveGroup ${mirrored.exclusiveGroup} !== ${exclusiveGroup}`);
      }
    }
    expect(mismatches).toEqual([]);
  });

  it("carries no id the recipe does not also have", () => {
    // The other direction: a component removed from the recipe but left
    // behind here would render a checkbox for something `osinstallPlan`
    // can never actually switch on.
    const recipeIds = new Set(recipe().components.map((c) => c.id));
    const extra = AMIGAOS_32_COMPONENTS.map((c) => c.id).filter((id) => !recipeIds.has(id));
    expect(extra).toEqual([]);
  });
});

describe("componentLabel", () => {
  it("is the media name for a known id, the id itself otherwise", () => {
    expect(componentLabel("workbench-base")).toBe("Workbench3.2");
    expect(componentLabel("not-a-real-component")).toBe("not-a-real-component");
  });
});

describe("sanitizeChosen", () => {
  it("drops unknown and unavailable ids, keeps real available ones", () => {
    expect(
      sanitizeChosen(["workbench-base", "extras", "not-a-real-id", "update-3.2.1", "backdrops"])
    ).toEqual(["workbench-base", "extras"]);
  });
});

// ---------------------------------------------------------------------------
// isForcedOnByCondition / pruneStaleExclusions
// ---------------------------------------------------------------------------

function planWith(componentsOn: string[]): InstallPlan {
  return {
    release: "AmigaOS 3.2",
    items: [],
    refusals: [],
    totalBytes: 0,
    componentsOn,
    mediaPaths: {},
    userStartup: [],
  };
}

describe("isForcedOnByCondition", () => {
  it("is true only for a non-required component on by the plan but not chosen", () => {
    const plan = planWith(["workbench-base", "modules-a1200"]);
    expect(isForcedOnByCondition(plan, [], "modules-a1200")).toBe(true);
    // Required: never "forced by condition", even though it is in componentsOn.
    expect(isForcedOnByCondition(plan, [], "workbench-base")).toBe(false);
    // Explicitly chosen: the user's own choice, not the condition's.
    expect(isForcedOnByCondition(plan, ["modules-a1200"], "modules-a1200")).toBe(false);
  });

  it("is false when there is no plan, or the id is unknown, or the id is not on", () => {
    expect(isForcedOnByCondition(null, [], "modules-a1200")).toBe(false);
    expect(isForcedOnByCondition(planWith([]), [], "not-a-real-id")).toBe(false);
    expect(isForcedOnByCondition(planWith([]), [], "modules-a1200")).toBe(false);
  });
});

describe("pruneStaleExclusions", () => {
  it("keeps an exclusion only while the component is still forced on", () => {
    const stillOn = planWith(["workbench-base", "modules-a1200"]);
    expect(pruneStaleExclusions(stillOn, [], ["modules-a1200"])).toEqual(["modules-a1200"]);

    const noLongerOn = planWith(["workbench-base"]);
    expect(pruneStaleExclusions(noLongerOn, [], ["modules-a1200"])).toEqual([]);
  });
});

// ---------------------------------------------------------------------------
// conditionalReason — every branch, including the disagreement case a
// review found rendering no reason at all
// ---------------------------------------------------------------------------

describe("conditionalReason", () => {
  it("falls back to rom-needed whenever the plan could not decide the ROM", () => {
    expect(conditionalReason(47, false, false, true, "Kickstart 3.1")).toEqual({ kind: "rom-needed" });
    // The disagreement case a review found: romUnknown false but rom itself
    // null (the frontend's own ROM read failed independently of the plan's).
    expect(conditionalReason(47, true, false, false, null)).toEqual({ kind: "rom-needed" });
  });

  it("is condition-overridden when excluded and still forced on", () => {
    expect(conditionalReason(47, true, true, false, "Kickstart 3.1 (40.068)")).toEqual({
      kind: "condition-overridden",
      major: 47,
    });
  });

  it("is condition-on when forced on and not excluded", () => {
    expect(conditionalReason(47, true, false, false, "Kickstart 3.1 (40.068)")).toEqual({
      kind: "condition-on",
      rom: "Kickstart 3.1 (40.068)",
      major: 47,
    });
  });

  it("is condition-off when not forced on and not excluded", () => {
    expect(conditionalReason(47, false, false, false, "Kickstart 3.2.2 (47.111)")).toEqual({
      kind: "condition-off",
      rom: "Kickstart 3.2.2 (47.111)",
      major: 47,
    });
  });

  it("an exclusion that is not actually forcing anything on reads as condition-off, not overridden", () => {
    // A stale exclusion (pruned elsewhere) must not itself invent a reason
    // to alarm the user — `excluded && forcedOn` is the only overridden case.
    expect(conditionalReason(47, false, true, false, "Kickstart 3.2.2 (47.111)")).toEqual({
      kind: "condition-off",
      rom: "Kickstart 3.2.2 (47.111)",
      major: 47,
    });
  });
});

describe("conditionalToggleAction", () => {
  it("covers every combination of excluded and forcedOn", () => {
    expect(conditionalToggleAction(true, true)).toBe("undo-exclusion");
    expect(conditionalToggleAction(true, false)).toBe("undo-exclusion");
    expect(conditionalToggleAction(false, true)).toBe("confirm-off");
    expect(conditionalToggleAction(false, false)).toBe("toggle-chosen");
  });
});

// ---------------------------------------------------------------------------
// The plain toggle/exclusion helpers
// ---------------------------------------------------------------------------

describe("toggleChosen", () => {
  it("adds an id, and clears any other member of the same exclusive group", () => {
    // `modules-a1200` is the only member of the "modules" group today, so
    // this proves the mechanism rather than a real conflict — the same
    // note Task 1's own review left on `exclusive_group` itself.
    expect(toggleChosen(["extras"], "modules-a1200")).toEqual(["extras", "modules-a1200"]);
  });

  it("removes an id already present", () => {
    expect(toggleChosen(["extras", "fonts"], "extras")).toEqual(["fonts"]);
  });
});

describe("confirmComponentOff", () => {
  it("adds to excluded and drops any stray chosen entry for the same id", () => {
    expect(confirmComponentOff(["modules-a1200", "extras"], [], "modules-a1200")).toEqual({
      chosen: ["extras"],
      excluded: ["modules-a1200"],
    });
  });

  it("does not duplicate an id already excluded", () => {
    expect(confirmComponentOff([], ["modules-a1200"], "modules-a1200")).toEqual({
      chosen: [],
      excluded: ["modules-a1200"],
    });
  });
});

describe("withoutExcluded", () => {
  it("removes the named id and leaves the rest", () => {
    expect(withoutExcluded(["a", "modules-a1200", "b"], "modules-a1200")).toEqual(["a", "b"]);
  });
});

describe("hasRomUnknownRefusal", () => {
  it("is true only when a rom-unknown refusal is present", () => {
    const withIt: InstallPlan = { ...planWith([]), refusals: [{ refusal: "rom-unknown" }] };
    const withoutIt: InstallPlan = {
      ...planWith([]),
      refusals: [{ refusal: "media-missing", component: "extras", volume_name: "Extras3.2" }],
    };
    expect(hasRomUnknownRefusal(withIt)).toBe(true);
    expect(hasRomUnknownRefusal(withoutIt)).toBe(false);
  });
});

// ---------------------------------------------------------------------------
// The Verify section's parsers — Minor findings, both about a mismatch
// between what the button would accept and what running actually accepted
// ---------------------------------------------------------------------------

describe("parseOptionalSlot", () => {
  it("empty text is a valid null (a plain HDF)", () => {
    expect(parseOptionalSlot("")).toEqual({ ok: true, value: null });
    expect(parseOptionalSlot("   ")).toEqual({ ok: true, value: null });
  });

  it("a whole number is valid", () => {
    expect(parseOptionalSlot("2")).toEqual({ ok: true, value: 2 });
    expect(parseOptionalSlot("0")).toEqual({ ok: true, value: 0 });
  });

  it("anything else is not ok — never a silent NaN reaching the wire as null", () => {
    expect(parseOptionalSlot("abc")).toEqual({ ok: false });
    expect(parseOptionalSlot("-1")).toEqual({ ok: false });
    expect(parseOptionalSlot("1.5")).toEqual({ ok: false });
  });
});

describe("parsePartitionIndex", () => {
  it("accepts whole numbers >= 1", () => {
    expect(parsePartitionIndex("1")).toBe(1);
    expect(parsePartitionIndex("12")).toBe(12);
  });

  it("rejects 0, negative, non-numeric and empty text — the same check everywhere it is used", () => {
    expect(parsePartitionIndex("0")).toBeNull();
    expect(parsePartitionIndex("-1")).toBeNull();
    expect(parsePartitionIndex("abc")).toBeNull();
    expect(parsePartitionIndex("")).toBeNull();
  });
});
