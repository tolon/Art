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
  osinstallBlocker,
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
    expect(declaring).toEqual([
      "AmigaOS 3.2/extras",
      "AmigaOS 3.2/locale-gr",
      "AmigaOS 3.2/locale-pl",
      "AmigaOS 3.2/locale-ru",
      "AmigaOS 3.2/locale-tr",
      "AmigaOS 3.2/classes",
      "AmigaOS 3.2/modules-a1200",
      "AmigaOS 3.2/glowicons",
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
    const KNOWN = ["rom-older-than", "rom-at-least"];
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
    // Both kinds are present in shipped data — 3.2's `modules-a1200` is
    // `rom-older-than 47`, 3.9's `workbench-base` is `rom-at-least 40` —
    // so neither half of this is vacuous.
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
        } else {
          expect(def.requiresRomMajor, def.id).toBe(raw.major);
          expect(def.conditionMajor, def.id).toBeNull();
        }
      }
    }
    expect([...new Set(seen)].sort()).toEqual(["rom-at-least", "rom-older-than"]);
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
    totalFiles: 0,
    componentsOn,
    mediaPaths: {},
    packages: [],
    packageMedia: {},
    userStartup: [],
    activations: [],
    mediaStamps: {},
    removals: [],
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

  function planned(input: {
    refusals?: RefusalReason[];
    items?: InstallPlan["items"];
  }): PlanResult {
    return {
      outcome: "planned",
      plan: {
        release: "3.2",
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
      },
    };
  }

  const READY = {
    mediaFolder: "E:\\media",
    destination: "E:\\dist",
    destinationTaken: false,
    found: ["Workbench3.2"],
    releaseHolding: null,
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
  it("keeps the per-disk refusal when the folder is the right one and a disk is missing", () => {
    const blocker = osinstallBlocker({
      ...READY,
      found: ["Workbench3.2", "Locale-TR"],
      plan: planned({
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
      }),
    });
    expect(blocker?.key).toBe("osinstall.blocked.refusals");
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
      plan: planned({ refusals: [MEDIA_MISSING("workbench-base", "Workbench3.2")] }),
    });
    expect(blocker?.key).toBe("osinstall.blocked.refusals");
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
