// Pure logic pulled out of `OsInstall.tsx` in a fix round: a Critical defect
// (an excluded component's media staying in `mediaPaths`, so `apply()`'s
// manifest lied about what a tree was built from) shipped inside code no
// test could reach, because it lived only in a component. This file is the
// coverage that would have caught it.
//
// The parity suite that used to sit here guarded `AMIGAOS_32_COMPONENTS`, a
// hand-written copy of the AmigaOS 3.2 recipe, against the recipe it
// mirrored. That constant is gone — the checklist is now a projection of
// whichever release's recipe the user chose (`osinstallComponents`), so
// there is no second copy left to drift. What survives from it is the part a
// projection cannot guarantee: the assumptions the *screen* makes about what
// a recipe may contain, asserted over **every** shipped recipe rather than
// only over 3.2, since 3.9 arriving is exactly what turned a hand-mirror
// from redundant into wrong.

import { describe, expect, it } from "vitest";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";

import {
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
  rememberedComponentKey,
  sanitizeChosen,
  toggleChosen,
  withoutExcluded,
  INSTALL_RELEASES,
  type ComponentDef,
  type InstallPlan,
} from "@/lib/osinstall";

// ---------------------------------------------------------------------------
// What the screen assumes about a recipe — checked over every shipped one
// ---------------------------------------------------------------------------

const RECIPE_DIR = resolve(__dirname, "..", "..", "src-tauri", "src", "core", "osinstall", "recipes");

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

/** Every recipe file ART ships, by the filename `recipe.rs` includes it
 *  under. Listed rather than globbed so a file added without being wired
 *  into `by_release` does not quietly join the suite. */
const RECIPE_FILES = ["amigaos-3.2.json", "amigaos-3.9.json"];

function recipes(): Recipe[] {
  return RECIPE_FILES.map(
    (name) => JSON.parse(readFileSync(resolve(RECIPE_DIR, name), "utf8")) as Recipe
  );
}

/** A recipe's components in the shape the command projects them into — the
 *  same mapping `ComponentSummary::from` performs on the Rust side, so the
 *  helpers below are exercised against real recipe data rather than a
 *  hand-typed catalogue that could be wrong in the same direction. */
function catalogueOf(recipe: Recipe): ComponentDef[] {
  return recipe.components.map((c) => ({
    id: c.id,
    media: c.media,
    required: c.required ?? false,
    available: c.available ?? true,
    conditionMajor: c.condition?.major ?? null,
    exclusiveGroup: c.exclusive_group ?? null,
  }));
}

describe("every shipped recipe", () => {
  it("parses and has components to check", () => {
    // A recipe that failed to parse, or one emptied by a bad edit, would
    // make every assertion below vacuously true.
    for (const recipe of recipes()) {
      expect(recipe.components.length, recipe.release).toBeGreaterThan(0);
    }
  });

  it("is reachable from the release picker, and the picker offers nothing else", () => {
    // Review finding 11's boundary, now actually crossed by a test:
    // `INSTALL_RELEASES` is what the picker lists, and a release listed but
    // unshipped (or shipped but unlisted) is a release the user either
    // cannot install or cannot reach.
    expect([...INSTALL_RELEASES].sort()).toEqual(recipes().map((r) => r.release).sort());
  });

  it("carries no condition the screen does not know how to explain", () => {
    // ART-119 (#3), widened from 3.2 alone. `ComponentDef.conditionMajor`
    // flattens one specific variant, `Condition::RomOlderThan { major }`.
    // A future condition kind that also happened to carry a `major` would
    // otherwise render as "below Kickstart V47" regardless of what it
    // actually meant, and `conditionalReason`'s whole four-branch
    // vocabulary is written in those terms.
    const offenders: string[] = [];
    for (const recipe of recipes()) {
      for (const component of recipe.components) {
        if (component.condition && component.condition.condition !== "rom-older-than") {
          offenders.push(`${recipe.release}/${component.id}: "${component.condition.condition}"`);
        }
      }
    }
    expect(offenders).toEqual([]);
  });

  it("names each component id at most once, so a label can resolve", () => {
    for (const recipe of recipes()) {
      const ids = recipe.components.map((c) => c.id);
      expect(new Set(ids).size, recipe.release).toBe(ids.length);
    }
  });

  it("uses one component id for different media in different releases — which is why the list must be loaded", () => {
    // The concrete reason a hardcoded catalogue was wrong rather than merely
    // redundant: both shipped recipes carry `workbench-base`, and it is not
    // the same volume in each, so a label resolved against the wrong recipe
    // names media that has nothing to do with what is being installed.
    const media = recipes().map((r) => r.components.find((c) => c.id === "workbench-base")?.media);
    expect(media.every((m) => m !== undefined)).toBe(true);
    expect(new Set(media).size).toBe(media.length);
  });
});

describe("componentLabel", () => {
  it("is the media name for an id the loaded release holds, the id itself otherwise", () => {
    const catalogue = catalogueOf(recipes()[0]);
    expect(componentLabel(catalogue, "workbench-base")).toBe("Workbench3.2");
    // Never a fabricated volume name for something this release does not
    // hold — including an id that belongs to the *other* release.
    expect(componentLabel(catalogue, "not-a-real-component")).toBe("not-a-real-component");
    expect(componentLabel([], "workbench-base")).toBe("workbench-base");
  });
});

describe("componentDef", () => {
  it("resolves against the list it is given, not a module constant", () => {
    const [threeTwo, threeNine] = recipes().map(catalogueOf);
    expect(componentDef(threeTwo, "workbench-base")?.media).not.toBe(
      componentDef(threeNine, "workbench-base")?.media
    );
    expect(componentDef(threeNine, "extras")).toBeUndefined();
  });
});

describe("sanitizeChosen", () => {
  it("drops unknown and unavailable ids, keeps real available ones", () => {
    // `backdrops` used to belong in this list; it became available when the
    // running system named its own wallpaper path (ART-127), so the
    // unavailable example is `update-3.2.1`, which is still registered and
    // still not implemented.
    const catalogue = catalogueOf(recipes()[0]);
    expect(
      sanitizeChosen(catalogue, [
        "workbench-base",
        "extras",
        "not-a-real-id",
        "update-3.2.1",
        "backdrops",
      ])
    ).toEqual(["workbench-base", "extras", "backdrops"]);
  });

  it("drops everything against an empty catalogue — which is why the screen must not call it before one loads", () => {
    // Stated as a test rather than only as a comment: this is the ART-089
    // shape. `OsInstall.tsx` holds `null` for "not loaded yet" and passes
    // the remembered ids through untouched until a real list arrives.
    expect(sanitizeChosen([], ["workbench-base", "extras"])).toEqual([]);
  });
});

describe("rememberedComponentKey", () => {
  it("keeps the unsuffixed key for the release that existed before the picker did", () => {
    // Anyone upgrading into the release picker finds the selection they last
    // made still ticked, rather than an empty list under a key nothing ever
    // wrote.
    expect(rememberedComponentKey("osinstall.chosen", "AmigaOS 3.2")).toBe("osinstall.chosen");
  });

  it("gives every other release its own key, so switching does not destroy the other's choices", () => {
    expect(rememberedComponentKey("osinstall.chosen", "AmigaOS 3.9")).toBe(
      "osinstall.chosen.AmigaOS 3.9"
    );
    expect(rememberedComponentKey("osinstall.excludedConditional", "AmigaOS 3.9")).toBe(
      "osinstall.excludedConditional.AmigaOS 3.9"
    );
    // Two releases never share a key — otherwise one release's ids would be
    // sanitized out of the other's remembered set on every switch.
    const keys = INSTALL_RELEASES.map((r) => rememberedComponentKey("osinstall.chosen", r));
    expect(new Set(keys).size).toBe(keys.length);
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
    packages: [],
    packageMedia: {},
    userStartup: [],
  };
}

/** The AmigaOS 3.2 catalogue, as the command would project it — every test
 *  below reasons about `modules-a1200`, which only that recipe declares. */
const CATALOGUE = catalogueOf(recipes()[0]);

describe("isForcedOnByCondition", () => {
  it("is true only for a non-required component on by the plan but not chosen", () => {
    const plan = planWith(["workbench-base", "modules-a1200"]);
    expect(isForcedOnByCondition(CATALOGUE, plan, [], "modules-a1200")).toBe(true);
    // Required: never "forced by condition", even though it is in componentsOn.
    expect(isForcedOnByCondition(CATALOGUE, plan, [], "workbench-base")).toBe(false);
    // Explicitly chosen: the user's own choice, not the condition's.
    expect(isForcedOnByCondition(CATALOGUE, plan, ["modules-a1200"], "modules-a1200")).toBe(false);
  });

  it("is false when there is no plan, or the id is unknown, or the id is not on", () => {
    expect(isForcedOnByCondition(CATALOGUE, null, [], "modules-a1200")).toBe(false);
    expect(isForcedOnByCondition(CATALOGUE, planWith([]), [], "not-a-real-id")).toBe(false);
    expect(isForcedOnByCondition(CATALOGUE, planWith([]), [], "modules-a1200")).toBe(false);
  });

  it("is false against a release whose recipe does not hold the id at all", () => {
    // The 3.9 recipe has no `modules-a1200`. A plan that somehow named it
    // must not make a row light up in a release that cannot install it —
    // the "unknown id" branch, reached the way the release picker reaches it.
    const threeNine = catalogueOf(recipes()[1]);
    const plan = planWith(["workbench-base", "modules-a1200"]);
    expect(isForcedOnByCondition(threeNine, plan, [], "modules-a1200")).toBe(false);
  });
});

describe("pruneStaleExclusions", () => {
  it("keeps an exclusion only while the component is still forced on", () => {
    const stillOn = planWith(["workbench-base", "modules-a1200"]);
    expect(pruneStaleExclusions(CATALOGUE, stillOn, [], ["modules-a1200"])).toEqual([
      "modules-a1200",
    ]);

    const noLongerOn = planWith(["workbench-base"]);
    expect(pruneStaleExclusions(CATALOGUE, noLongerOn, [], ["modules-a1200"])).toEqual([]);
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
    expect(toggleChosen(CATALOGUE, ["extras"], "modules-a1200")).toEqual([
      "extras",
      "modules-a1200",
    ]);
  });

  it("removes an id already present", () => {
    expect(toggleChosen(CATALOGUE, ["extras", "fonts"], "extras")).toEqual(["fonts"]);
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
