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
  keymapsIn,
  mediaEvidence,
  mediaIdentityFolderLines,
  mediaIdentityLines,
  mediaIdentitySummary,
  type MediaConfirmation,
  type MediaIdentification,
  type MediaIdentityState,
  type MediaMatch,
  type MediaRow,
  type ReleaseEvidence,
  osinstallBlocker,
  parseOptionalSlot,
  parsePartitionIndex,
  pruneStaleExclusions,
  rememberedComponentKey,
  sanitizeChosen,
  toggleChosen,
  withoutExcluded,
  wrongMediaFolder,
  INSTALL_RELEASES,
  type ComponentDef,
  type InstallPlan,
  type PlanResult,
  type RefusalReason,
} from "@/lib/osinstall";

// ---------------------------------------------------------------------------
// What the screen assumes about a recipe — checked over every shipped one
// ---------------------------------------------------------------------------

const RECIPE_DIR = resolve(__dirname, "..", "..", "src-tauri", "src", "core", "osinstall", "recipes");

interface RecipeComponent {
  id: string;
  media: string;
  label_key?: string;
  required?: boolean;
  condition?: { condition: string; major?: number };
  exclusive_group?: string;
  available?: boolean;
  overrides?: string[];
}

interface Recipe {
  release: string;
  /** Another recipe's `release` this one is layered onto (Task 8). Its own
   *  file carries only its own components — the base's are merged in only
   *  on the Rust side — so a based recipe here does not (yet) carry an id
   *  like `workbench-base` at all. */
  base?: string;
  components: RecipeComponent[];
}

/** Every recipe file ART ships, by the filename `recipe.rs` includes it
 *  under. Listed rather than globbed so a file added without being wired
 *  into `by_release` does not quietly join the suite. */
const RECIPE_FILES = ["amigaos-3.2.json", "amigaos-3.2.2.json", "amigaos-3.9.json"];

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
    labelKey: c.label_key ?? null,
    required: c.required ?? false,
    available: c.available ?? true,
    conditionMajor: c.condition?.condition === "rom-older-than" ? (c.condition.major ?? null) : null,
    requiresRomMajor: c.condition?.condition === "rom-at-least" ? (c.condition.major ?? null) : null,
    exclusiveGroup: c.exclusive_group ?? null,
    overrides: c.overrides ?? [],
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

  it("declares its overrides where the screen can see them (ART-175)", () => {
    // The screen previews exactly the switched-on components whose
    // `overrides` is non-empty, so a component that declares one and does
    // not project it is a component whose replacement nobody is shown — the
    // whole of ART-175.
    //
    // The list below is **read off the shipped recipes**, not assumed: the
    // first version of this test asserted that AmigaOS 3.9's `workbench-39`
    // was the only layering component in shipped data (ART-175's own entry
    // says so) and failed immediately — AmigaOS 3.2 has four of its own,
    // `glowicons` layering over four other components at once. Both halves
    // are present, so neither direction is vacuous.
    const declaring: string[] = [];
    for (const recipe of recipes()) {
      for (const def of catalogueOf(recipe)) {
        const raw = recipe.components.find((c) => c.id === def.id)!;
        expect(def.overrides, `${recipe.release}/${def.id}`).toEqual(raw.overrides ?? []);
        if (def.overrides.length > 0) declaring.push(`${recipe.release}/${def.id}`);
      }
    }
    // **In recipe order, and the order moved** (ART-224, 2026-08-23).
    // `modules-a1200` and `glowicons` were declared *above* `storage`, which
    // they both override — and recipe order is what decides which layer
    // writes last, so both overrides were inert. `glowicons` cost sixteen
    // GlowIcons a user had ticked for. Both now sit after it, which is why
    // this list reads in a different order than it used to; `locale-euro` is
    // ART-159's new one.
    //
    // The four `locale-*` entries arrived on 2026-08-24 with the `Support`
    // drawer the four alphabets that need it carry - Greek, Polish, Russian
    // and Turkish. They declare over `workbench-base` because they write
    // `Prefs/Presets/Font-XX.prefs` into a drawer it owns, and they sit after
    // it in recipe order, which is what makes the declaration mean anything
    // (ART-224: an override declared *above* what it overrides is inert).
    //
    // **AmigaOS 3.2.2's five entries (Task 8) are read off its own file, not
    // the merged tree** — `recipes()` here parses each JSON file raw, so a
    // based recipe's `overrides` naming a *base-layer* id (`extras`,
    // `workbench-base`, `diskdoctor`, …) shows up exactly as declared, in
    // this file's own component order, and lands between the two releases
    // whose file names bracket it in `RECIPE_FILES`.
    expect(declaring).toEqual([
      "AmigaOS 3.2/extras",
      "AmigaOS 3.2/locale-gr",
      "AmigaOS 3.2/locale-pl",
      "AmigaOS 3.2/locale-ru",
      "AmigaOS 3.2/locale-tr",
      "AmigaOS 3.2/classes",
      "AmigaOS 3.2/modules-a1200",
      "AmigaOS 3.2/glowicons",
      "AmigaOS 3.2.2/update-322-system",
      "AmigaOS 3.2.2/update-322-classes",
      "AmigaOS 3.2.2/update-322-diskdoctor",
      "AmigaOS 3.2.2/update-322-modules-a1200",
      "AmigaOS 3.2.2/update-322-modules-a1200-strap",
      "AmigaOS 3.9/workbench-39",
      "AmigaOS 3.9/locale-euro",
    ]);
  });

  it("carries no condition the screen does not know how to explain", () => {
    // ART-119 (#3), widened from 3.2 alone, and widened again by ART-157.
    // Each condition kind has its own field and its own sentence:
    // `rom-older-than` -> `conditionMajor` and `conditionalReason`'s
    // four-branch vocabulary; `rom-at-least` -> `requiresRomMajor` and
    // `reason.romAtLeast`. A third kind that also happened to carry a
    // `major` would otherwise be projected as one of these two and render a
    // sentence that means something else — "below Kickstart V47" over a
    // requirement that is a floor, or the reverse.
    //
    // `resident-older-than` (Task 8) is a fourth kind, and it is *known*
    // without being *explained*: `ComponentSummary::from` maps it to neither
    // field on purpose (a resident's own version is a different number from
    // either Kickstart bound this screen already renders a sentence for),
    // so the two AmigaOS 3.2.2 Modules components carrying it are listed
    // here rather than flagged as an unrecognised kind.
    const KNOWN = ["rom-older-than", "rom-at-least", "resident-older-than"];
    const offenders: string[] = [];
    for (const recipe of recipes()) {
      for (const component of recipe.components) {
        if (component.condition && !KNOWN.includes(component.condition.condition)) {
          offenders.push(`${recipe.release}/${component.id}: "${component.condition.condition}"`);
        }
      }
    }
    expect(offenders).toEqual([]);
  });

  it("projects each condition kind into its own field and never the other", () => {
    // ART-157's real hazard, asserted over the shipped recipes rather than
    // a hand-typed pair: the two numbers read alike and mean opposite
    // things, so a maximum leaking into `requiresRomMajor` would have ART
    // record a Kickstart floor no recipe ever stated, and a minimum leaking
    // into `conditionMajor` would put the switching vocabulary on screen
    // over a fact that switches nothing.
    //
    // All three kinds are present in shipped data — 3.2's `modules-a1200` is
    // `rom-older-than 47`, 3.9's `workbench-base` is `rom-at-least 40`, and
    // AmigaOS 3.2.2's two Modules components are `resident-older-than` —
    // so no branch below is vacuous.
    const seen: string[] = [];
    for (const recipe of recipes()) {
      for (const def of catalogueOf(recipe)) {
        const raw = recipe.components.find((c) => c.id === def.id)!.condition;
        if (!raw) {
          expect(def.conditionMajor, def.id).toBeNull();
          expect(def.requiresRomMajor, def.id).toBeNull();
          continue;
        }
        seen.push(raw.condition);
        if (raw.condition === "rom-older-than") {
          expect(def.conditionMajor, def.id).toBe(raw.major);
          expect(def.requiresRomMajor, def.id).toBeNull();
        } else if (raw.condition === "rom-at-least") {
          expect(def.requiresRomMajor, def.id).toBe(raw.major);
          expect(def.conditionMajor, def.id).toBeNull();
        } else {
          // `resident-older-than` — deliberately neither field (see this
          // test's own doc comment above and `ComponentSummary::from`).
          expect(def.conditionMajor, def.id).toBeNull();
          expect(def.requiresRomMajor, def.id).toBeNull();
        }
      }
    }
    expect([...new Set(seen)].sort()).toEqual([
      "resident-older-than",
      "rom-at-least",
      "rom-older-than",
    ]);
  });

  it("names each component id at most once, so a label can resolve", () => {
    for (const recipe of recipes()) {
      const ids = recipe.components.map((c) => c.id);
      expect(new Set(ids).size, recipe.release).toBe(ids.length);
    }
  });

  it("uses one component id for different media in different releases — which is why the list must be loaded", () => {
    // The concrete reason a hardcoded catalogue was wrong rather than merely
    // redundant: both shipped *unbased* recipes carry `workbench-base`, and
    // it is not the same volume in each, so a label resolved against the
    // wrong recipe names media that has nothing to do with what is being
    // installed.
    //
    // Filtered to `!r.base` (Task 8): AmigaOS 3.2.2's own file inherits
    // `workbench-base` only once merged on the Rust side, so it does not
    // carry the id at all here — which would read as `undefined`, not as a
    // third distinct volume, and is a different fact than this test checks.
    const media = recipes()
      .filter((r) => !r.base)
      .map((r) => r.components.find((c) => c.id === "workbench-base")?.media);
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
    // Found by `release`, not by array position (Task 8): three recipe files
    // now ship, and a positional `[a, b] = recipes().map(...)` destructure
    // would silently pair `threeNine` with AmigaOS 3.2.2's own catalogue
    // instead — which happens to still pass both assertions below (neither
    // `workbench-base` nor `extras` is in 3.2.2's own unmerged file either),
    // so a name lookup is what actually exercises the real 3.9 recipe.
    const all = recipes();
    const threeTwo = catalogueOf(all.find((r) => r.release === "AmigaOS 3.2")!);
    const threeNine = catalogueOf(all.find((r) => r.release === "AmigaOS 3.9")!);
    expect(componentDef(threeTwo, "workbench-base")?.media).not.toBe(
      componentDef(threeNine, "workbench-base")?.media
    );
    expect(componentDef(threeNine, "extras")).toBeUndefined();
  });
});

describe("sanitizeChosen", () => {
  it("drops unknown and unavailable ids, keeps real available ones", () => {
    // `backdrops` used to belong in this list; it became available when the
    // running system named its own wallpaper path (ART-127). The unavailable
    // example used to be the shipped `update-3.2.1` placeholder — removed in
    // Task 8, since AmigaOS 3.2.2 is now its own recipe rather than a
    // not-yet-built component of 3.2 — so this constructs its own §96
    // "Coming Later" fixture instead of depending on any one shipped
    // component staying unbuilt forever.
    const catalogue: ComponentDef[] = [
      ...catalogueOf(recipes()[0]),
      {
        id: "not-yet-built",
        media: "SomeFutureDisk",
        labelKey: null,
        required: false,
        available: false,
        conditionMajor: null,
        requiresRomMajor: null,
        exclusiveGroup: null,
        overrides: [],
      },
    ];
    expect(
      sanitizeChosen(catalogue, [
        "workbench-base",
        "extras",
        "not-a-real-id",
        "not-yet-built",
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
    totalFiles: 0,
    componentsOn,
    mediaPaths: {},
    packages: [],
    packageMedia: {},
    userStartup: [],
    activations: [],
    mediaStamps: {},
    removals: [],
    layers: [],
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

// ---------------------------------------------------------------------------
// The blocker — and the folder that is simply the wrong one (ART-208)
// ---------------------------------------------------------------------------

describe("osinstallBlocker", () => {
  const MEDIA_MISSING = (component: string, volume: string): RefusalReason => ({
    refusal: "media-missing",
    component,
    volume_name: volume,
  });

  /**
   * A `ReleaseEvidence` fixture. Written out rather than derived, because
   * deriving it in TypeScript would be a second copy of the matcher
   * `core::osinstall::identify` owns — the ART-249 shape this round is
   * removing, not adding. The Rust side has its own tests that the evidence
   * a real recipe produces is this shape.
   */
  const evidenceOf = (
    distinguishing: string[],
    shared: string[] = [],
    missingRequired: string[] = []
  ): ReleaseEvidence => ({
    release: "AmigaOS 3.2",
    distinguishing,
    shared,
    missingRequired,
  });

  /** No media this release asks for is in the folder — the only state that
   *  makes `wrongMediaFolder`'s claim a true one. */
  const NOTHING_OF_THIS_RELEASE = evidenceOf([], [], ["Workbench3.2", "Install3.2"]);

  function planned(input: {
    refusals?: RefusalReason[];
    items?: InstallPlan["items"];
  }): PlanResult {
    // ART-253's guard, at the place fixtures are built. `InstallPlan`'s own
    // invariant (`core/osinstall/plan.rs`): a plan is either a full
    // description of what would be written or every reason it cannot
    // proceed, never both. The one production construction site empties
    // `items` whenever there is any refusal — so a fixture carrying both
    // exercises a state the core cannot emit, and a screen tested only
    // against it is tested against nothing. That is how the false sentence
    // survived a whole suite.
    if ((input.refusals?.length ?? 0) > 0 && (input.items?.length ?? 0) > 0) {
      throw new Error(
        "InstallPlan invariant: a plan with refusals has no items (core/osinstall/plan.rs)"
      );
    }
    return {
      outcome: "planned",
      plan: {
        release: "AmigaOS 3.2",
        items: input.items ?? [],
        refusals: input.refusals ?? [],
        totalBytes: 0,
        totalFiles: 0,
        componentsOn: [],
        mediaPaths: {},
        packages: [],
        packageMedia: {},
        userStartup: [],
        activations: [],
        mediaStamps: {},
        removals: [],
        layers: [],
      },
    };
  }

  const READY = {
    mediaFolder: "E:\\media",
    destination: "E:\\dist",
    destinationTaken: false,
    found: ["Workbench3.2"],
    releaseHolding: null,
    mediaFacts: evidenceOf(["Workbench3.2"]),
  };

  it("says nothing is wrong when a plan has items and no refusals", () => {
    expect(
      osinstallBlocker({
        ...READY,
        plan: planned({
          items: [
            {
              component: "workbench-base",
              media: "Workbench3.2",
              from: "DF0:C/Format",
              to: "C/Format",
              isDir: false,
              decompress: false,
              bytes: 10,
              mergeIcon: false,
            },
          ],
        }),
      })
    ).toBeNull();
  });

  // The owner's own screen, reduced: sixteen components, sixteen
  // `MediaMissing` refusals, nothing installable — because the folder held
  // their AmigaOS 3.9 disc and the release chosen was 3.2. Sixteen true
  // sentences that together read as "a lot of programs are missing".
  it("names the folder rather than the disks when nothing in it is wanted", () => {
    const blocker = osinstallBlocker({
      ...READY,
      found: ["AmigaOS3.9"],
      mediaFacts: NOTHING_OF_THIS_RELEASE,
      plan: planned({
        refusals: [
          MEDIA_MISSING("workbench-base", "Workbench3.2"),
          MEDIA_MISSING("locale-tr", "Locale-TR"),
          MEDIA_MISSING("storage", "Storage3.2"),
        ],
      }),
    });
    expect(blocker?.key).toBe("osinstall.blocked.wrongFolder");
    expect(blocker?.params?.found).toBe("AmigaOS3.9");
  });

  it("names the release the folder does belong to, when ART can tell", () => {
    const blocker = osinstallBlocker({
      ...READY,
      found: ["AmigaOS3.9"],
      releaseHolding: "AmigaOS 3.9",
      mediaFacts: NOTHING_OF_THIS_RELEASE,
      plan: planned({ refusals: [MEDIA_MISSING("workbench-base", "Workbench3.2")] }),
    });
    expect(blocker?.key).toBe("osinstall.blocked.wrongFolderIsRelease");
    expect(blocker?.params?.release).toBe("AmigaOS 3.9");
    expect(blocker?.params?.found).toBe("AmigaOS3.9");
  });

  // The distinction the whole entry rests on: one absent disk in an
  // otherwise right folder is a missing disk, and telling that user "none of
  // these disks are what this release wants" would be false about a folder
  // holding fifteen disks it does want.
  //
  // **The fixture used to carry one placed item beside the refusal**, and no
  // plan the core can emit does (ART-253): any refusal at all empties
  // `items`. So the condition that actually withdrew the sentence in
  // production was never the one this test thought it was exercising. What
  // withdraws it now is the evidence — the folder holds `Workbench3.2`,
  // which this release does ask for.
  it("keeps the per-disk refusal when the folder is the right one and a disk is missing", () => {
    const blocker = osinstallBlocker({
      ...READY,
      found: ["Workbench3.2", "Locale-TR"],
      mediaFacts: evidenceOf(["Workbench3.2", "Locale-TR"], [], ["Install3.2"]),
      plan: planned({ refusals: [MEDIA_MISSING("storage", "Storage3.2")] }),
    });
    expect(blocker?.key).toBe("osinstall.blocked.refusals");
  });

  // The review's own folder, at the blocker: `Workbench3.2`, `Fonts` and
  // `Locale` present, `Extras3.2` absent. On `main` this rendered
  //
  //   "None of the disks in this folder are ones this release asks for. It
  //    holds: Workbench3.2, Fonts, Locale."
  //
  // — false about `Workbench3.2`, and it sends a user away from the right
  // folder. `Fonts` and `Locale` are `shared`, never `distinguishing`, and
  // they still count here: they are disks this recipe asks for whatever else
  // also asks for them.
  it("does not call the folder wrong when it holds this release's own disks and one is absent", () => {
    const blocker = osinstallBlocker({
      ...READY,
      found: ["Workbench3.2", "Fonts", "Locale"],
      plan: planned({ refusals: [MEDIA_MISSING("extras", "Extras3.2")] }),
      mediaFacts: evidenceOf(["Workbench3.2"], ["Locale", "Fonts"], ["Install3.2"]),
    });
    expect(blocker?.key).toBe("osinstall.blocked.refusals");
    expect(blocker?.key).not.toBe("osinstall.blocked.wrongFolder");
  });

  // Same folder, one difference: ART can name what it is holding. The
  // all-or-nothing sentence must still withdraw — naming the release does
  // not make "none of these disks are asked for" true.
  it("does not call the folder wrong even when a release can be named for it", () => {
    const blocker = osinstallBlocker({
      ...READY,
      found: ["Workbench3.2", "Fonts", "Locale"],
      releaseHolding: "AmigaOS 3.2",
      plan: planned({ refusals: [MEDIA_MISSING("extras", "Extras3.2")] }),
      mediaFacts: evidenceOf(["Workbench3.2"], ["Locale", "Fonts"], ["Install3.2"]),
    });
    expect(blocker?.key).toBe("osinstall.blocked.refusals");
  });

  // ART-253 asked whether `osinstall.blocked.wrongFolder` — the bare listing,
  // no release named — still has a reachable state once the claim is checked
  // properly. It does, and this is it: a folder of disks that are nobody's
  // install media. `release_holding` answers `null` for them (they match no
  // recipe), the chosen release's evidence is empty (same reason), and the
  // sentence is then simply true. The string stays in both catalogues.
  it("still names a folder holding disks that are no release's install media", () => {
    const blocker = osinstallBlocker({
      ...READY,
      found: ["Lemmings", "MyBackup"],
      releaseHolding: null,
      mediaFacts: NOTHING_OF_THIS_RELEASE,
      plan: planned({ refusals: [MEDIA_MISSING("workbench-base", "Workbench3.2")] }),
    });
    expect(blocker?.key).toBe("osinstall.blocked.wrongFolder");
    expect(blocker?.params?.found).toBe("Lemmings, MyBackup");
  });

  // The claim is specific, so it is never made unchecked. Until the lookup
  // lands there is nothing to check it against, and the per-disk list below
  // is true either way.
  it("says nothing about the folder while the evidence has not arrived", () => {
    const blocker = osinstallBlocker({
      ...READY,
      found: ["AmigaOS3.9"],
      mediaFacts: null,
      plan: planned({ refusals: [MEDIA_MISSING("workbench-base", "Workbench3.2")] }),
    });
    expect(blocker?.key).toBe("osinstall.blocked.refusals");
  });

  // The fixture guard itself. A future edit that hands `planned` both a
  // refusal and an item is building a plan the core cannot emit, and this is
  // what stops it becoming a test that proves nothing.
  it("refuses to build a plan carrying both refusals and items", () => {
    expect(() =>
      planned({
        refusals: [MEDIA_MISSING("storage", "Storage3.2")],
        items: [
          {
            component: "workbench-base",
            media: "Workbench3.2",
            from: "DF0:C/Format",
            to: "C/Format",
            isDir: false,
            decompress: false,
            bytes: 10,
            mergeIcon: false,
          },
        ],
      })
    ).toThrow(/invariant/);
  });

  // The sentence claims something specific — "none of the disks in this
  // folder are ones this release asks for" — so it has to be *checked*, not
  // inferred from an empty plan. A folder holding a disk the recipe named
  // gets the per-disk list, whatever else went wrong, because the claim
  // would be false about that disk.
  it("does not claim the folder is wrong when it holds a disk the recipe named", () => {
    const blocker = osinstallBlocker({
      ...READY,
      found: ["Workbench3.2"],
      mediaFacts: evidenceOf(["Workbench3.2"], [], ["Install3.2"]),
      plan: planned({ refusals: [MEDIA_MISSING("workbench-base", "Workbench3.2")] }),
    });
    expect(blocker?.key).toBe("osinstall.blocked.refusals");
  });

  // A refusal that is not about media at all — an unreadable ROM, a
  // collision, an exclusive group — must never be papered over with a
  // sentence about folders. It is a different problem with a different fix.
  it("keeps the per-refusal list when something other than media is refused", () => {
    const blocker = osinstallBlocker({
      ...READY,
      found: ["AmigaOS3.9"],
      mediaFacts: NOTHING_OF_THIS_RELEASE,
      plan: planned({
        refusals: [MEDIA_MISSING("workbench-base", "Workbench3.2"), { refusal: "rom-unknown" }],
      }),
    });
    expect(blocker?.key).toBe("osinstall.blocked.refusals");
  });

  // "This folder holds no install media at all" is `osinstall.media.empty`'s
  // sentence, said the moment the folder was picked. Restating it as "none of
  // these are the right disks" would be a claim about disks that are not
  // there.
  it("keeps the per-refusal list when the folder holds no media at all", () => {
    const blocker = osinstallBlocker({
      ...READY,
      found: [],
      mediaFacts: NOTHING_OF_THIS_RELEASE,
      plan: planned({ refusals: [MEDIA_MISSING("workbench-base", "Workbench3.2")] }),
    });
    expect(blocker?.key).toBe("osinstall.blocked.refusals");
  });
});

// ---------------------------------------------------------------------------
// mediaEvidence — the partial-media case wrongMediaFolder refuses to speak,
// and the round this file's own module comment does not yet mention: the
// refusals list already names which component wants which disk, this adds
// what the folder itself looks like.
// ---------------------------------------------------------------------------

describe("mediaEvidence", () => {
  const RELEASE = "AmigaOS 3.2";

  /**
   * A `ReleaseEvidence` fixture — the same shape and the same reasoning as
   * the one in `osinstallBlocker`'s describe above: written out rather than
   * derived, because deriving it here would be a second copy of the matcher
   * `core::osinstall::identify` owns.
   */
  const evidenceOf = (
    distinguishing: string[],
    shared: string[] = [],
    missingRequired: string[] = []
  ): ReleaseEvidence => ({
    release: RELEASE,
    distinguishing,
    shared,
    missingRequired,
  });

  /**
   * ART-253. **`items` is derived, not taken.**
   *
   * It used to be a parameter, and every test in this describe passed it 12,
   * 3, 2 or 40 alongside refusals — a plan the core cannot emit.
   * `InstallPlan`'s invariant (`core/osinstall/plan.rs`) is that a plan is
   * either a full description of what would be written or every reason it
   * cannot proceed, never both, and the one production construction site
   * empties `items` for any refusal at all. So every assertion in here was
   * exercising an impossible state, and `wrongMediaFolder` was being
   * withdrawn by a condition that can never fire in production.
   *
   * Derived means the impossible plan is not merely rejected, it is
   * unspellable.
   */
  function planWith(missing: string[]): InstallPlan {
    const items =
      missing.length > 0
        ? 0
        : // No refusals: a planned tree really does carry items, and the two
          // silent-state tests below need one that does.
          40;
    return {
      release: RELEASE,
      items: Array.from({ length: items }, (_, i) => ({
        component: `component-${i}`,
        media: "Workbench3.2",
        from: `DF0:C/Item${i}`,
        to: `C/Item${i}`,
        isDir: false,
        decompress: false,
        bytes: 10,
        mergeIcon: false,
      })),
      refusals: missing.map((volume_name) => ({
        refusal: "media-missing",
        component: `component-${volume_name}`,
        volume_name,
      })),
      totalBytes: 0,
      totalFiles: 0,
      componentsOn: [],
      mediaPaths: {},
      packages: [],
      packageMedia: {},
      userStartup: [],
      activations: [],
      mediaStamps: {},
      removals: [],
      layers: [],
    };
  }

  it("says nothing when nothing is missing", () => {
    expect(
      mediaEvidence({
        plan: planWith([]),
        found: ["Workbench3.2"],
        releaseHolding: RELEASE,
        release: RELEASE,
        evidence: evidenceOf(["Workbench3.2"]),
      })
    ).toBeNull();
  });

  it("says nothing when wrongMediaFolder owns the case", () => {
    // Its conditions: folder non-empty, at least one refusal, every refusal
    // media-missing, and — checked against the recipe rather than inferred
    // (ART-253) — none of this release's own media in the folder. A 3.1
    // Workbench disk is none of 3.2's. The two helpers must never both
    // produce a sentence.
    const plan = planWith(["Workbench3.2", "Extras3.2"]);
    const found = ["Workbench3.1"];
    const evidence = evidenceOf([], [], ["Workbench3.2", "Install3.2"]);
    expect(wrongMediaFolder(plan, found, evidence)).not.toBeNull();
    expect(
      mediaEvidence({ plan, found, releaseHolding: "AmigaOS 3.1", release: RELEASE, evidence })
    ).toBeNull();
  });

  it("names what the folder holds and which disks are absent", () => {
    const phrase = mediaEvidence({
      plan: planWith(["Extras3.2", "Classes3.2"]),
      found: ["Workbench3.2", "Fonts", "Locale", "Install3.2"],
      releaseHolding: RELEASE,
      release: RELEASE,
      evidence: evidenceOf(["Workbench3.2", "Install3.2"], ["Locale", "Fonts"]),
    });
    expect(phrase?.key).toBe("osinstall.evidence.sameRelease");
    expect(phrase?.params?.found).toBe("Workbench3.2, Fonts, Locale, Install3.2");
    expect(phrase?.params?.missing).toBe("Extras3.2, Classes3.2");
  });

  it("says which release the folder is when it is a different one", () => {
    const phrase = mediaEvidence({
      plan: planWith(["Extras3.2"]),
      found: ["Workbench3.1", "Fonts", "Locale"],
      releaseHolding: "AmigaOS 3.1",
      release: RELEASE,
      // `Fonts` and `Locale` are unsuffixed across 3.1, 3.1.4 and 3.2, so
      // 3.2's own recipe does ask for them. That is what keeps
      // `wrongMediaFolder` quiet here and lets this sentence be the one said
      // — on the old fixture it was the impossible `items: 3` doing that job.
      evidence: evidenceOf([], ["Locale", "Fonts"], ["Workbench3.2", "Install3.2"]),
    });
    expect(phrase?.key).toBe("osinstall.evidence.otherRelease");
    expect(phrase?.params?.release).toBe("AmigaOS 3.1");
    // and it still names the missing disk, because that is what the user acts on
    expect(phrase?.params?.missing).toBe("Extras3.2");
    // and it names what the folder holds — "this is 3.1 media" is an
    // assertion; naming the disks found is evidence the user can check.
    expect(phrase?.params?.found).toBe("Workbench3.1, Fonts, Locale");
  });

  it("does not name a release it cannot identify", () => {
    // `Fonts` and `Locale` are unversioned across 3.1, 3.1.4 and 3.2, so a
    // folder holding only those identifies nothing. Saying "this looks like
    // 3.2" here would be a confident wrong sentence.
    const phrase = mediaEvidence({
      plan: planWith(["Workbench3.2"]),
      found: ["Fonts", "Locale"],
      releaseHolding: null,
      release: RELEASE,
      evidence: evidenceOf([], ["Locale", "Fonts"], ["Workbench3.2", "Install3.2"]),
    });
    expect(phrase?.key).toBe("osinstall.evidence.unidentified");
    expect(phrase?.params?.found).toBe("Fonts, Locale");
    expect(JSON.stringify(phrase?.params)).not.toContain(RELEASE);
  });

  it("says nothing at all when no folder has been chosen", () => {
    // `osinstall.blocked.noFolder` already owns this; a second sentence here
    // would be two answers to one question.
    expect(
      mediaEvidence({
        plan: planWith(["Workbench3.2"]),
        found: [],
        releaseHolding: null,
        release: RELEASE,
        evidence: evidenceOf([], [], ["Workbench3.2", "Install3.2"]),
      })
    ).toBeNull();
  });

  // ART-253, the folder the review measured: `Workbench3.2`, `Fonts` and
  // `Locale` in hand, `Extras3.2` absent. It is the ordinary partial case
  // and it was never covered — every fixture in this describe reached it
  // through an impossible `items` count instead. On `main` the screen said
  //
  //   "None of the disks in this folder are ones this release asks for. It
  //    holds: Workbench3.2, Fonts, Locale."
  //
  // The whole sentence is asserted here, key and both parameters, not merely
  // that something was said: "something was said" was true of the false
  // sentence too.
  it("names the folder's own disks and the absent one, for the review's own folder", () => {
    const plan = planWith(["Extras3.2"]);
    const found = ["Workbench3.2", "Fonts", "Locale"];
    const evidence = evidenceOf(["Workbench3.2"], ["Locale", "Fonts"], ["Install3.2"]);

    // First: the false sentence is not available at all here.
    expect(wrongMediaFolder(plan, found, evidence)).toBeNull();

    const phrase = mediaEvidence({
      plan,
      found,
      releaseHolding: RELEASE,
      release: RELEASE,
      evidence,
    });
    expect(phrase).toEqual({
      key: "osinstall.evidence.sameRelease",
      params: { found: "Workbench3.2, Fonts, Locale", missing: "Extras3.2" },
    });
  });

  it("returns a different key for every state", () => {
    // The round's central guard. Every other test checks one state in
    // isolation and would stay green if two of them were merged into one
    // sentence — which is precisely the collapse this project names as its
    // most expensive failure. This is the only test that can see it.
    const keys = [
      mediaEvidence({
        plan: planWith(["Extras3.2"]),
        found: ["Workbench3.2", "Fonts"],
        releaseHolding: RELEASE,
        release: RELEASE,
        evidence: evidenceOf(["Workbench3.2"], ["Fonts"]),
      }),
      mediaEvidence({
        plan: planWith(["Extras3.2"]),
        found: ["Workbench3.1", "Fonts"],
        releaseHolding: "AmigaOS 3.1",
        release: RELEASE,
        evidence: evidenceOf([], ["Fonts"], ["Workbench3.2", "Install3.2"]),
      }),
      mediaEvidence({
        plan: planWith(["Workbench3.2"]),
        found: ["Fonts", "Locale"],
        releaseHolding: null,
        release: RELEASE,
        evidence: evidenceOf([], ["Locale", "Fonts"], ["Workbench3.2", "Install3.2"]),
      }),
    ].map((phrase) => phrase?.key);

    expect(keys.every((key) => typeof key === "string")).toBe(true);
    expect(new Set(keys).size).toBe(keys.length);
  });

  // -------------------------------------------------------------------------
  // ART-254 — evidence about another release brings ART-253's sentence back
  // -------------------------------------------------------------------------
  //
  // The plan and the evidence are fetched by two uncoordinated effects on the
  // screen, so a release switch leaves a window where the plan is the new
  // release's and the evidence is still the old release's. Old evidence for a
  // folder of *this* release's disks is legitimately empty — the folder holds
  // none of AmigaOS 3.1's media — which is precisely the shape ART-253's
  // check reads as "none of the disks in this folder are ones this release
  // asks for".

  /** AmigaOS 3.1's honest answer about a folder full of 3.2 disks: it holds
   *  nothing of 3.1. True, and about a question nobody on screen is asking
   *  any more. */
  const STALE_31_EVIDENCE: ReleaseEvidence = {
    release: "AmigaOS 3.1",
    distinguishing: [],
    shared: [],
    missingRequired: ["Workbench3.1", "Install3.1"],
  };

  it("wrongMediaFolder withdraws when the evidence answers for a different release than the plan", () => {
    const plan = planWith(["Extras3.2"]);
    const found = ["Workbench3.2", "Fonts", "Locale"];

    expect(wrongMediaFolder(plan, found, STALE_31_EVIDENCE)).toBeNull();

    // The control, and the reason the assertion above has exactly one cause.
    // The identical emptiness, relabelled with this plan's own release, does
    // produce the sentence — so what withdrew it is the release check and
    // not `found`, the refusals, or the emptiness itself.
    expect(wrongMediaFolder(plan, found, { ...STALE_31_EVIDENCE, release: plan.release })).toBe(
      "Workbench3.2, Fonts, Locale"
    );
  });

  it("says the partial-media sentence, not the false one, while the evidence is a release behind", () => {
    const plan = planWith(["Extras3.2"]);
    const found = ["Workbench3.2", "Fonts", "Locale"];

    // Not merely "something else was said": the whole phrase, key and both
    // parameters. "Says nothing" would have been true here for several
    // reasons; this state has one.
    expect(
      mediaEvidence({
        plan,
        found,
        releaseHolding: RELEASE,
        release: RELEASE,
        evidence: STALE_31_EVIDENCE,
      })
    ).toEqual({
      key: "osinstall.evidence.sameRelease",
      params: { found: "Workbench3.2, Fonts, Locale", missing: "Extras3.2" },
    });

    // Control again: the same empty evidence, about the release being built,
    // is the one state where the all-or-nothing sentence is true — and there
    // this correctly says nothing, because the two never both speak.
    expect(
      mediaEvidence({
        plan,
        found,
        releaseHolding: RELEASE,
        release: RELEASE,
        evidence: { ...STALE_31_EVIDENCE, release: RELEASE },
      })
    ).toBeNull();
  });

  // The other half of the same window, one render earlier: **both** the plan
  // and the evidence are still the previous release's, because the two
  // effects that fetch them are uncoordinated and neither is cleared when the
  // release changes. They agree with each other, so `wrongMediaFolder`'s own
  // check cannot see it — and it would then say "none of these disks are ones
  // this release asks for" about the release the picker has already left.
  //
  // Withdrawn whole rather than partly: `missing` is read off the plan, so
  // the alternative is a sentence naming AmigaOS 3.9's absent disks under
  // AmigaOS 3.2's name. Two stale artefacts do not make one current sentence.
  it("says nothing at all while the plan itself is still the previous release's", () => {
    const stalePlan: InstallPlan = { ...planWith(["AmigaOS3.9"]), release: "AmigaOS 3.9" };
    const found = ["Workbench3.2", "Fonts", "Locale"];
    // Evidence held at `null` in **both** arms below, so the only difference
    // between them is the plan's own release and the `null` has one cause.
    // It is also the honest state here: on a release switch the evidence
    // lookup is in flight while the previous plan is still on screen.
    const args = { found, releaseHolding: RELEASE, release: RELEASE, evidence: null };

    expect(mediaEvidence({ ...args, plan: stalePlan })).toBeNull();

    // The control. Identical in every respect except that the plan is the
    // release being built — and then there is a sentence, so what silenced
    // the call above was the plan's release and not the folder or the refusal.
    expect(mediaEvidence({ ...args, plan: { ...stalePlan, release: RELEASE } })).toEqual({
      key: "osinstall.evidence.sameRelease",
      params: { found: "Workbench3.2, Fonts, Locale", missing: "AmigaOS3.9" },
    });
  });

  // -------------------------------------------------------------------------
  // ART-255 — the Ambiguous folder, which no test in the round constructed
  // -------------------------------------------------------------------------
  //
  // `Workbench3.1` and `AmigaOS3.9` in one folder, building AmigaOS 3.2 —
  // plus `Fonts` and `Locale`, which AmigaOS 3.2's own recipe also asks for.
  // The two versioned disks name AmigaOS 3.1 and AmigaOS 3.9, and
  // `release_holding` declines to choose between them, so it answers `null`
  // — the same `null` an unknown folder produces. Neither versioned disk is
  // *this* release's own (AmigaOS 3.2's `distinguishing` for this pile is
  // empty), so the sentence must be one that is true of both Ambiguous and
  // Unknown, which is why it no longer states a reason. `Fonts`/`Locale` are
  // in the pile so `wrongMediaFolder` — which owns the all-or-nothing case
  // and would otherwise fire here, since it is silent whenever this
  // release's own evidence is entirely empty — correctly withdraws, leaving
  // this the state under test.
  //
  // **Not `Workbench3.2` and `AmigaOS3.9`, as this fixture originally read**
  // (ART-259). That pairing is genuinely ambiguous too, but `Workbench3.2` is
  // AmigaOS 3.2's own distinguishing disk — the fixture was accidentally
  // exercising `holdsThisReleasesOwnMedia`, not the "neither disk is this
  // release's own" state ART-255 names. Once ART-257's check is asked before
  // this one (as it must be, ART-259), that fixture correctly answers
  // `sameRelease`: the folder really does hold a genuine AmigaOS 3.2 disk,
  // and saying so is not a guess, whatever else the folder also holds. This
  // fixture keeps ART-255's own state isolated from that one.
  it("says the unidentified sentence for a folder naming two other releases, not just an unknown one", () => {
    const phrase = mediaEvidence({
      plan: planWith(["Extras3.2"]),
      found: ["Workbench3.1", "AmigaOS3.9", "Fonts", "Locale"],
      // Ambiguous, not Unknown — and indistinguishable from here.
      releaseHolding: null,
      release: RELEASE,
      // Neither versioned disk is AmigaOS 3.2's own: no distinguishing.
      // `Fonts`/`Locale` are shared, not distinguishing, and never settle
      // identification — but they keep `wrongMediaFolder` from also firing.
      evidence: evidenceOf([], ["Fonts", "Locale"], ["Workbench3.2", "Extras3.2"]),
    });

    expect(phrase).toEqual({
      key: "osinstall.evidence.unidentified",
      params: { found: "Workbench3.1, AmigaOS3.9, Fonts, Locale" },
    });
  });

  // The pairing ART-255's own fixture used before ART-259: genuinely
  // Ambiguous (two releases named), but one of the two disks really is this
  // release's own distinguishing media. `sameRelease` is the honest answer
  // here, not `unidentified` — the claim it makes ("this release's own media
  // is among what the folder holds") stays true whatever else is in the pile.
  it("calls an ambiguous folder this release's own once one of its disks genuinely is (ART-259)", () => {
    const phrase = mediaEvidence({
      plan: planWith(["Extras3.2"]),
      found: ["Workbench3.2", "AmigaOS3.9"],
      releaseHolding: null,
      release: RELEASE,
      evidence: evidenceOf(["Workbench3.2"], [], ["Extras3.2"]),
    });

    expect(phrase).toEqual({
      key: "osinstall.evidence.sameRelease",
      params: { found: "Workbench3.2, AmigaOS3.9", missing: "Extras3.2" },
    });
  });

  // -------------------------------------------------------------------------
  // ART-257 — a based release's inherited media is its own media
  // -------------------------------------------------------------------------
  //
  // The layered release is the one most likely to arrive part-complete, and
  // it is the one where `identify` and the release being built legitimately
  // disagree. Every fixture below is the answer the shipped recipes actually
  // give, pinned on the Rust side by
  // `identify.rs::a_based_releases_own_evidence_claims_the_base_set`:
  // `release_holding` of the AmigaOS 3.2 base set is `"AmigaOS 3.2"`, while
  // `evidence_for("AmigaOS 3.2.2", …)` of the same names claims those disks
  // as 3.2.2's own `distinguishing` media and names the update disks absent.

  const BASED = "AmigaOS 3.2.2";
  /** The base set on the shelf, nothing of the update yet. */
  const BASE_SET = ["Workbench3.2", "Install3.2", "Extras3.2", "Fonts", "Locale"];
  const BASED_PLAN: InstallPlan = {
    ...planWith(["Update3.2.2", "Classes3.2.2"]),
    release: BASED,
  };
  const BASED_EVIDENCE: ReleaseEvidence = {
    release: BASED,
    distinguishing: ["Workbench3.2", "Install3.2", "Extras3.2"],
    shared: ["Locale", "Fonts"],
    missingRequired: ["Update3.2.2", "Classes3.2.2"],
  };

  it("calls the inherited base set this release's own media, not the base release's", () => {
    expect(
      mediaEvidence({
        plan: BASED_PLAN,
        found: BASE_SET,
        // `identify`'s own answer, and it is right: a based release must not
        // be named off its base's disks alone.
        releaseHolding: RELEASE,
        release: BASED,
        evidence: BASED_EVIDENCE,
      })
    ).toEqual({
      key: "osinstall.evidence.sameRelease",
      params: {
        found: "Workbench3.2, Install3.2, Extras3.2, Fonts, Locale",
        missing: "Update3.2.2, Classes3.2.2",
      },
    });

    // The control, one field apart. With none of this release's own
    // distinguishing media in the pile — the folder really is somebody
    // else's — the other sentence is still the one said, so what produced
    // the sentence above was the evidence and not the release names, the
    // refusals or the folder.
    expect(
      mediaEvidence({
        plan: BASED_PLAN,
        found: BASE_SET,
        releaseHolding: RELEASE,
        release: BASED,
        evidence: { ...BASED_EVIDENCE, distinguishing: [] },
      })
    ).toEqual({
      key: "osinstall.evidence.otherRelease",
      params: {
        found: "Workbench3.2, Install3.2, Extras3.2, Fonts, Locale",
        release: RELEASE,
        missing: "Update3.2.2, Classes3.2.2",
      },
    });
  });

  it("will not call a folder somebody else's media without this release's own evidence", () => {
    // `otherRelease` no longer claims the folder holds none of this
    // release's own media (M1), but it still must not fire while stale
    // evidence (ART-254's window, and it is wider here: a switch to a based
    // release lands with the base release's evidence still held) could yet
    // turn out to hold this release's own distinguishing media once it
    // resolves — which would make `sameRelease` the right ending instead.
    // The per-disk refusals below say what is missing either way.
    const stale: ReleaseEvidence = { ...BASED_EVIDENCE, release: RELEASE };
    const args = {
      plan: BASED_PLAN,
      found: BASE_SET,
      releaseHolding: RELEASE,
      release: BASED,
    };

    expect(mediaEvidence({ ...args, evidence: stale })).toBeNull();
    // In flight is the same answer for the same reason.
    expect(mediaEvidence({ ...args, evidence: null })).toBeNull();

    // The control: the identical evidence, labelled with the release it is
    // actually about, does produce a sentence — so the silence above has one
    // cause and it is the release label, not the folder or the refusals.
    expect(mediaEvidence({ ...args, evidence: BASED_EVIDENCE })?.key).toBe(
      "osinstall.evidence.sameRelease"
    );
  });

  // -------------------------------------------------------------------------
  // ART-259 — ART-257's own check ran after the `releaseHolding === null`
  // return, so it was unreachable for the one folder it exists to catch.
  // -------------------------------------------------------------------------

  /**
   * The update set on the shelf, none of the base yet. Unlike `BASE_SET`
   * above — where `identify` still names the base release outright — a
   * based release with **none** of its base present has a non-empty
   * `missing_required` and is dropped from the named candidates
   * (`identify.rs`'s own base-subsumption pass), and the update disk names
   * are nobody else's. So `release_holding` of this exact pile answers
   * `Unknown`, not `"AmigaOS 3.2"` and not `"AmigaOS 3.2.2"` — this is the
   * folder the previous fix wave's own reorder bug left unreachable.
   */
  const UPDATE_SET = ["Update3.2.2", "Classes3.2.2"];
  const UPDATE_ONLY_PLAN: InstallPlan = {
    ...planWith(["Workbench3.2", "Install3.2", "Extras3.2"]),
    release: BASED,
  };
  const UPDATE_ONLY_EVIDENCE: ReleaseEvidence = {
    release: BASED,
    distinguishing: ["Update3.2.2", "Classes3.2.2"],
    shared: [],
    missingRequired: ["Workbench3.2", "Install3.2", "Extras3.2"],
  };

  it("calls the update-only folder this release's own media even though identify names no release at all", () => {
    expect(
      mediaEvidence({
        plan: UPDATE_ONLY_PLAN,
        found: UPDATE_SET,
        // `identify`'s own answer for this exact pile: Unknown.
        releaseHolding: null,
        release: BASED,
        evidence: UPDATE_ONLY_EVIDENCE,
      })
    ).toEqual({
      key: "osinstall.evidence.sameRelease",
      params: {
        found: "Update3.2.2, Classes3.2.2",
        missing: "Workbench3.2, Install3.2, Extras3.2",
      },
    });
  });

  it("still says unidentified for a folder holding neither this release's base nor its update disks", () => {
    // `unidentified` must keep every state it is genuinely right for: a pile
    // with nothing of this release's own `distinguishing` media in it is
    // still exactly that state, `releaseHolding === null` and nothing else,
    // whatever the reorder above changed.
    expect(
      mediaEvidence({
        plan: UPDATE_ONLY_PLAN,
        found: ["Fonts", "Locale"],
        releaseHolding: null,
        release: BASED,
        evidence: {
          release: BASED,
          distinguishing: [],
          shared: ["Fonts", "Locale"],
          missingRequired: ["Workbench3.2", "Install3.2", "Extras3.2", "Update3.2.2", "Classes3.2.2"],
        },
      })
    ).toEqual({ key: "osinstall.evidence.unidentified", params: { found: "Fonts, Locale" } });
  });
});

// ---------------------------------------------------------------------------
// ART-226's other half: which keyboards a plan would really place
// ---------------------------------------------------------------------------

describe("keymapsIn", () => {
  const planWith = (to: string[]): InstallPlan =>
    ({
      items: to.map((path) => ({
        component: "keymaps",
        media: "Shelf",
        from: path,
        to: path,
        isDir: false,
      })),
    }) as unknown as InstallPlan;

  it("reads the layouts off the plan's own items", () => {
    expect(
      keymapsIn(
        planWith(["Devs/Keymaps/türkçe", "Devs/Keymaps/usa", "C/Assign", "Libs/x.library"])
      )
    ).toEqual(["türkçe", "usa"]);
  });

  /// **The icon is not a layout.** `Devs/Keymaps` carries a `.info` beside
  /// every keymap, and offering `türkçe.info` in the picker would write a
  /// `SetKeyboard türkçe.info` line that prints an error at every boot.
  it("does not offer the icons as keyboards", () => {
    expect(keymapsIn(planWith(["Devs/Keymaps/tr", "Devs/Keymaps/tr.info"]))).toEqual(["tr"]);
  });

  /// **A directory inside the drawer is not a layout either**, and that is
  /// the case `isDir` actually guards: it splits into three parts exactly as a
  /// keymap does, so nothing about the path says it is not one. Found by
  /// mutation — the first version of these tests only had the drawer itself,
  /// which the three-part rule already excludes, so removing `isDir` broke
  /// nothing.
  it("does not offer a directory that sits inside the drawer", () => {
    const plan = {
      items: [
        {
          component: "keymaps",
          media: "S",
          from: "Keymaps/extra",
          to: "Devs/Keymaps/extra",
          isDir: true,
        },
        {
          component: "keymaps",
          media: "S",
          from: "Keymaps/tr",
          to: "Devs/Keymaps/tr",
          isDir: false,
        },
      ],
    } as unknown as InstallPlan;
    expect(keymapsIn(plan)).toEqual(["tr"]);
  });

  /// A directory entry for the drawer itself is not a layout either.
  it("does not offer the drawer", () => {
    const plan = {
      items: [
        { component: "keymaps", media: "S", from: "Keymaps", to: "Devs/Keymaps", isDir: true },
        { component: "keymaps", media: "S", from: "Keymaps/d", to: "Devs/Keymaps/d", isDir: false },
      ],
    } as unknown as InstallPlan;
    expect(keymapsIn(plan)).toEqual(["d"]);
  });

  /// Anything deeper than one level is not a keymap AmigaOS would load.
  it("ignores anything below the drawer", () => {
    expect(keymapsIn(planWith(["Devs/Keymaps/extra/deep"]))).toEqual([]);
  });

  it("says nothing when there is no plan yet", () => {
    expect(keymapsIn(null)).toEqual([]);
  });

  it("is sorted, so the list does not move between two plans", () => {
    expect(keymapsIn(planWith(["Devs/Keymaps/usa", "Devs/Keymaps/d", "Devs/Keymaps/i"]))).toEqual([
      "d",
      "i",
      "usa",
    ]);
  });
});

// ---------------------------------------------------------------------------
// The five endings a content-hash result is allowed to produce (design §4.3)
// ---------------------------------------------------------------------------

describe("what a content-hash result is allowed to say", () => {
  const ROW: MediaRow = {
    md5: "5edf0b7a10409ef992ea351565ef8b6c",
    version: "3.2",
    // Hatcher's own identifier for the disk, which is measurably *not* the
    // disk's own AmigaDOS volume name (0 of 12 matched, 2026-09-06).
    volume: "Workbench3_2",
    name: "Workbench 3.2",
    source: "Hyperion (3.2 base)",
    sequence: 1,
    // An adopted row: Hatcher's data states none of these three, so they
    // default the same way `mediahash.rs`'s own adopted rows do.
    kind: "floppy",
    artefact: null,
    filenames: [],
    tableOrigin: "adopted",
  };
  /** An own-table row — the AmigaOS 3.9 CD-ROM, design's §3.6 — used to
   *  prove `mediaIdentityLines` names the right table. */
  const OWN_ROW: MediaRow = {
    md5: "e32a107e68edfc9b28a2fe075e32e5f6",
    version: "3.9",
    volume: "AmigaOS3.9",
    name: "AmigaOS 3.9 CD-ROM",
    source: "HstWB Installer amiga-os-entries.csv (MIT)",
    sequence: null,
    kind: "disc",
    artefact: "amigaos-39-cd",
    filenames: ["AmigaOS39.iso", "amigaos3.9.iso"],
    tableOrigin: "own",
  };
  const CHECK: MediaConfirmation = {
    checked: "2026-09-06",
    against: "the ART author's own AmigaOS 3.2 install set, 35 ADFs",
  };
  function match(over: Partial<MediaMatch> = {}): MediaMatch {
    return {
      path: "E:\\media\\Disk1.adf",
      volumeName: "Workbench3.2",
      row: null,
      md5: "0".repeat(32),
      confirmed: null,
      ...over,
    };
  }
  function identified(over: Partial<MediaIdentification> = {}): MediaIdentityState {
    return {
      kind: "identified",
      identification: { matches: [], unreadable: [], hashed: 0, remembered: 0, ...over },
    };
  }

  /**
   * **Four files, four endings, four different keys.** Asserted as a set
   * rather than one at a time: the failure this guards against is two of
   * them collapsing into one sentence, and a per-ending test would still
   * pass while two endings shared a key.
   */
  it("gives a matched-and-checked, a matched-unchecked, a miss and an unreadable file four different sentences", () => {
    const lines = mediaIdentityLines(
      identified({
        matches: [
          match({ path: "a.adf", row: ROW, md5: ROW.md5, confirmed: CHECK }),
          match({ path: "b.adf", row: ROW, md5: ROW.md5 }),
          match({ path: "c.adf" }),
        ],
        unreadable: ["d.adf"],
      })
    );
    expect(lines.map((l) => l.kind)).toEqual([
      "confirmed",
      "unconfirmed",
      "not-in-table",
      "unreadable",
    ]);
    const keys = lines.map((l) => l.phrase.key);
    expect(new Set(keys).size).toBe(4);
    expect(keys).toEqual([
      "osinstall.mediaId.confirmed",
      "osinstall.mediaId.unconfirmed",
      "osinstall.mediaId.notInTable",
      "osinstall.mediaId.unreadable",
    ]);
  });

  /**
   * **A confirmation is cited, not badged.** Both fields have to reach the
   * sentence, or "confirmed" is an assertion with nothing behind it.
   */
  it("carries what confirmed the row and when, into the sentence", () => {
    const [line] = mediaIdentityLines(
      identified({ matches: [match({ row: ROW, md5: ROW.md5, confirmed: CHECK })] })
    );
    expect(line.phrase.params).toMatchObject({
      name: "Workbench 3.2",
      version: "3.2",
      source: "Hyperion (3.2 base)",
      checked: "2026-09-06",
      against: CHECK.against,
    });
  });

  /**
   * **Which table answered gets its own sentence** (design's §3.6). The
   * existing "confirmed"/"unconfirmed" keys name Emu68 Hatcher's table by
   * name and have no slot for a different one, so a match against ART's own
   * table must take a different key — never the adopted one, which would
   * misattribute the claim, and never a silently-shared key that could not
   * tell the two apart. Both confirmation states are asserted, the same way
   * the adopted pair is above: a mapper that always picked the "own" key
   * would pass an own-only test just as easily as one that never did.
   */
  it("names ART's own table when a match came from it, confirmed and unconfirmed both", () => {
    const [unconfirmed] = mediaIdentityLines(
      identified({ matches: [match({ row: OWN_ROW, md5: OWN_ROW.md5 })] })
    );
    expect(unconfirmed.phrase.key).toBe("osinstall.mediaId.unconfirmedOwn");
    expect(unconfirmed.phrase.params).toMatchObject({
      name: "AmigaOS 3.9 CD-ROM",
      version: "3.9",
      source: "HstWB Installer amiga-os-entries.csv (MIT)",
    });

    const [confirmed] = mediaIdentityLines(
      identified({ matches: [match({ row: OWN_ROW, md5: OWN_ROW.md5, confirmed: CHECK })] })
    );
    expect(confirmed.phrase.key).toBe("osinstall.mediaId.confirmedOwn");

    // And the adopted row still takes the adopted keys — the two tables'
    // sentences must not have merged into one another.
    const [adopted] = mediaIdentityLines(
      identified({ matches: [match({ row: ROW, md5: ROW.md5 })] })
    );
    expect(adopted.phrase.key).toBe("osinstall.mediaId.unconfirmed");
  });

  /**
   * **Additive, never subtractive — at the level where it is structural.**
   * The component test proves the volume-name line survives a miss on
   * screen; this proves the stronger property that makes it survive: no
   * sentence built here reads `volumeName` at all, for *any* ending. The two
   * facts come from two sources and are rendered from two places, so there
   * is no code path along which a hash result could weaken a name result.
   */
  it("never puts the disk's own name into a sentence about the table", () => {
    const lines = mediaIdentityLines(
      identified({
        matches: [
          match({ path: "a.adf", row: ROW, md5: ROW.md5, confirmed: CHECK }),
          match({ path: "b.adf", row: ROW, md5: ROW.md5 }),
          match({ path: "c.adf" }),
        ],
        unreadable: ["d.adf"],
      })
    );
    for (const line of lines) {
      const values = Object.values(line.phrase.params ?? {}).map(String);
      expect(values).not.toContain("Workbench3.2");
      // And the row's `volume` is not smuggled in as a stand-in for it
      // either — that field is Hatcher's internal identifier and belongs in
      // no sentence at all.
      expect(values).not.toContain("Workbench3_2");
    }
  });

  /** A miss names the file and nothing else: there is no row to quote, and
   *  inventing a claim about the disk is exactly what §4.3 forbids. */
  it("says only which file when no row claims it", () => {
    const [line] = mediaIdentityLines(identified({ matches: [match({ path: "E:\\m\\odd.adf" })] }));
    expect(line.kind).toBe("not-in-table");
    expect(line.phrase.params).toEqual({ file: "odd.adf" });
  });

  /** Sorted by path, so two folders' files interleave in one readable list
   *  rather than in arrival order — and an unreadable file sits among them
   *  rather than in a footnote a reader can miss. */
  it("lists every file in one path-ordered list, unreadable ones included", () => {
    const lines = mediaIdentityLines(
      identified({
        matches: [match({ path: "E:\\m\\c.adf" }), match({ path: "E:\\m\\a.adf" })],
        unreadable: ["E:\\m\\b.adf"],
      })
    );
    expect(lines.map((l) => l.file)).toEqual(["a.adf", "b.adf", "c.adf"]);
  });

  /**
   * **"Not hashed yet", "running", "could not run" and a real result are
   * four states with four next steps.** Collapsing any pair — most
   * temptingly a failure into an empty result — is the §89 defect.
   */
  it("keeps not-asked, running, failed, stopped and done apart", () => {
    expect(mediaIdentitySummary({ kind: "not-asked" })).toEqual({
      key: "osinstall.mediaId.notHashedYet",
    });
    expect(mediaIdentitySummary({ kind: "identifying" })).toEqual({
      key: "osinstall.mediaId.identifying",
    });
    expect(mediaIdentitySummary(stalled("failed"))?.key).toBe("osinstall.mediaId.failed");
    expect(mediaIdentitySummary(stalled("cancelled"))?.key).toBe("osinstall.mediaId.cancelled");
    expect(mediaIdentitySummary(identified({ hashed: 2, remembered: 1 }))).toEqual({
      key: "osinstall.mediaId.provenance",
      params: { hashed: 2, remembered: 1 },
    });
    // The point of the two above is that they are *different*: a "cancelled"
    // that is only a second message on the `failed` state is one edit away
    // from collapsing back.
    expect(mediaIdentitySummary(stalled("failed"))?.key).not.toBe(
      mediaIdentitySummary(stalled("cancelled"))?.key
    );
  });

  // -- Fix wave 1, M1 and M2. Pressing Stop is not a failure, and a pass that
  // died on folder 3 still did folders 1 and 2.

  /** A pass over three folders that ended on the second, in whichever way.
   *  Deliberately built with one folder of each outcome, so an assertion
   *  about "the folder that failed" cannot be satisfied by the only folder
   *  there is. */
  function stalled(kind: "failed" | "cancelled"): MediaIdentityState {
    return {
      kind,
      identification: {
        matches: [match({ path: "E:\\one\\a.adf", row: ROW, md5: ROW.md5, confirmed: CHECK })],
        unreadable: [],
        hashed: 1,
        remembered: 0,
      },
      folders: [
        { folder: "E:\\one", result: "identified" },
        { folder: "E:\\two", result: kind === "failed" ? "unreadable" : "stopped" },
        { folder: "E:\\three", result: "not-reached" },
      ],
    };
  }

  /**
   * **The user pressing Stop is not ART failing**, and the two sentences send
   * a user to different places: one says scan again, the other says something
   * is wrong with the media. Asserted on the *sentence*, not on "the panel
   * shows something", which is reachable from four other states.
   */
  it("does not tell a user who pressed Stop that ART could not identify their media", () => {
    const phrase = mediaIdentitySummary(stalled("cancelled"));
    expect(phrase?.key).toBe("osinstall.mediaId.cancelled");
    // And it does not smuggle the failure sentence in under another key.
    expect(phrase?.key).not.toContain("failed");
  });

  /**
   * **A refusal names what it could not read.** "ART could not read a folder"
   * with no folder in it is a refusal the user cannot act on, and with three
   * folders configured it is not even a hint.
   */
  it("names the folder a failed pass died on, and counts the ones it finished", () => {
    expect(mediaIdentitySummary(stalled("failed"))?.params).toEqual({
      folder: "E:\\two",
      identified: 1,
      total: 3,
    });
    // The stopped case has no unreadable folder to name, so it counts only.
    expect(mediaIdentitySummary(stalled("cancelled"))?.params).toEqual({
      identified: 1,
      total: 3,
    });
  });

  /**
   * **Never claim what you did not do, read the other way round.** ART hashed
   * the first folder's disk; a second folder it could not open takes nothing
   * away from that, and a screen showing an empty list would be denying work
   * ART had finished.
   */
  it("keeps the files an interrupted pass did identify", () => {
    for (const kind of ["failed", "cancelled"] as const) {
      const lines = mediaIdentityLines(stalled(kind));
      expect(lines.map((l) => l.file), kind).toEqual(["a.adf"]);
      expect(lines[0].kind, kind).toBe("confirmed");
    }
  });

  /**
   * **Per entry, by name and by result** — `core/hostfs.rs`'s rule. Four
   * results, four keys, and the folder's own name in each: a report that
   * said "2 of 3 folders" without saying *which* would leave the user to
   * guess which of their folders was never opened.
   */
  it("reports every folder of an interrupted pass by name and by result", () => {
    const lines = mediaIdentityFolderLines(stalled("failed"));
    expect(lines.map((l) => l.folder)).toEqual(["E:\\one", "E:\\two", "E:\\three"]);
    expect(lines.map((l) => l.result)).toEqual(["identified", "unreadable", "not-reached"]);
    expect(new Set(lines.map((l) => l.phrase.key)).size).toBe(3);
    for (const line of lines) {
      expect(line.phrase.params).toEqual({ folder: line.folder });
    }
    // The fourth result is the cancelled pass's in-flight folder, and it has
    // a key of its own — "you stopped me here" is not "I could not read it".
    const stoppedLine = mediaIdentityFolderLines(stalled("cancelled"))[1];
    expect(stoppedLine.result).toBe("stopped");
    expect(stoppedLine.phrase.key).toBe("osinstall.mediaId.folderStopped");
    expect(stoppedLine.phrase.key).not.toBe(lines[1].phrase.key);
  });

  /** Nothing per-folder to say about a pass that has not run, is running, or
   *  covered every folder — for the last, the per-file list *is* the report,
   *  and a second list would be the screen answering twice. */
  it("says nothing per folder about a pass that ran to the end", () => {
    expect(mediaIdentityFolderLines({ kind: "not-asked" })).toEqual([]);
    expect(mediaIdentityFolderLines({ kind: "identifying" })).toEqual([]);
    expect(mediaIdentityFolderLines(identified({ hashed: 1 }))).toEqual([]);
  });

  /** Where the answer came from is on screen, because a remembered hash can
   *  be stale: the identity is `(path, size, mtime)`, and a restored backup
   *  keeps all three. A wrong *name* for a disk is worse than a stale
   *  listing, so the counts are stated rather than hidden. */
  it("states how many answers were read now and how many were remembered", () => {
    expect(mediaIdentitySummary(identified({ hashed: 0, remembered: 35 }))?.params).toEqual({
      hashed: 0,
      remembered: 35,
    });
  });

  /** A folder with nothing hashable in it says nothing here —
   *  `osinstall.media.empty` already owns that sentence, and two lines
   *  counting the same zero would be the screen answering one question
   *  twice. */
  it("says nothing about a pass that had no files to look at", () => {
    expect(mediaIdentitySummary(identified())).toBeNull();
    expect(mediaIdentityLines(identified())).toEqual([]);
  });

  /**
   * **The same `null` is also reached a second way** (final-review.md M6):
   * every candidate came back unreadable, so `hashed + remembered` is zero
   * even though the folder plainly had files in it. `mediaIdentitySummary`'s
   * own doc comment used to name only the empty-folder cause; this is the
   * other one, and it must not produce the same silence as "nothing here" —
   * the per-file `unreadable` lines are the report for this case, which is
   * why the summary staying silent is correct, not merely untested.
   */
  it("also says nothing about the pass itself when every candidate was unreadable — but the per-file lines still report it", () => {
    const state = identified({ unreadable: ["a.adf", "b.adf"] });
    expect(mediaIdentitySummary(state)).toBeNull();
    const lines = mediaIdentityLines(state);
    expect(lines).toHaveLength(2);
    expect(lines.every((line) => line.kind === "unreadable")).toBe(true);
  });

  /** Nothing to show before an answer exists — including while one is in
   *  flight, when a stale list from the previous folder would be the worst
   *  of the options. */
  it("shows no per-file line until there is a result", () => {
    expect(mediaIdentityLines({ kind: "not-asked" })).toEqual([]);
    expect(mediaIdentityLines({ kind: "identifying" })).toEqual([]);
    // A pass that failed before it identified anything is empty too — but it
    // is empty because nothing was found, not because the state discards it
    // (`keeps the files an interrupted pass did identify` is that half).
    expect(
      mediaIdentityLines({
        kind: "failed",
        identification: { matches: [], unreadable: [], hashed: 0, remembered: 0 },
        folders: [{ folder: "E:\\one", result: "unreadable" }],
      })
    ).toEqual([]);
  });

  /** Both separators, because the folder is the user's and a future CLI
   *  shell's fixtures are POSIX. */
  it("names a file by its own name on either kind of path", () => {
    expect(
      mediaIdentityLines(identified({ matches: [match({ path: "/mnt/media/Fonts.adf" })] }))[0].file
    ).toBe("Fonts.adf");
  });
});
