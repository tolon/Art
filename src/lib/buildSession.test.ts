import { describe, expect, it } from "vitest";

import type { InstallLayer } from "@/lib/osinstall";
import { recallInto } from "@/lib/remembered";
import {
  DEFAULT_FIRSTBOOT,
  FIRSTBOOT_SPEC,
  foldersForPlan,
  isMaterialFolders,
  MATERIAL_SPEC,
  seedCardImage,
  seedRom,
  seedTreeRoot,
  seededComponents,
  seededMaterial,
  LEGACY_KEYS,
  SESSION_KEYS,
  withFolder,
  type FirstBootChoice,
  type MaterialChoice,
} from "./buildSession";

describe("seedTreeRoot", () => {
  it("prefers the session's own key once it exists", () => {
    const bag = {
      [SESSION_KEYS.tree]: { root: "E:\\new", builtHere: false },
      "osinstall.packages.treeRoot": "E:\\old-packages",
      "osinstall.destination": "E:\\old-destination",
    };
    expect(seedTreeRoot(bag)).toBe("E:\\new");
  });

  it("falls back to the tree the packages step was pointing at", () => {
    const bag = {
      "osinstall.packages.treeRoot": "E:\\old-packages",
      "osinstall.destination": "E:\\old-destination",
    };
    expect(seedTreeRoot(bag)).toBe("E:\\old-packages");
  });

  it("falls back to the destination for a user who never picked a tree", () => {
    // ART-197's own user: they watched ART write a tree and were then asked
    // to go and find it. The destination is the answer they could not give.
    const bag = { "osinstall.destination": "E:\\amiga\\dist-3.9" };
    expect(seedTreeRoot(bag)).toBe("E:\\amiga\\dist-3.9");
  });

  it("answers null when there is nothing to seed from", () => {
    expect(seedTreeRoot({})).toBeNull();
  });

  it("rejects a non-string that a hand-edited settings file could hold", () => {
    expect(seedTreeRoot({ "osinstall.destination": 42 })).toBeNull();
  });

  it("survives a bag that is not an object at all", () => {
    expect(seedTreeRoot(null)).toBeNull();
    expect(seedTreeRoot("nonsense")).toBeNull();
  });
});

describe("seededComponents", () => {
  it("reads the unsuffixed key for the release that predates the picker", () => {
    const bag = {
      "osinstall.chosen": ["workbench-base", "extras"],
      "osinstall.excludedConditional": ["modules-a1200"],
    };
    expect(seededComponents(bag, "AmigaOS 3.2")).toEqual({
      chosen: ["workbench-base", "extras"],
      excludedConditional: ["modules-a1200"],
    });
  });

  it("reads the per-release key for every other release", () => {
    // The migration defect this test exists for: a fixed key list reads
    // `osinstall.chosen` and silently drops every 3.9 tick, because
    // `rememberedComponentKey` suffixes every release but 3.2.
    const bag = {
      "osinstall.chosen": ["workbench-base"],
      "osinstall.chosen.AmigaOS 3.9": ["os39-base"],
    };
    expect(seededComponents(bag, "AmigaOS 3.9").chosen).toEqual(["os39-base"]);
  });

  it("keeps two releases apart rather than merging them", () => {
    const bag = {
      "osinstall.chosen": ["workbench-base"],
      "osinstall.chosen.AmigaOS 3.9": ["os39-base"],
    };
    expect(seededComponents(bag, "AmigaOS 3.2").chosen).toEqual(["workbench-base"]);
  });

  it("prefers the session's own per-release key once it exists", () => {
    const bag = {
      [SESSION_KEYS.components("AmigaOS 3.2")]: {
        chosen: ["already-migrated"],
        excludedConditional: [],
      },
      "osinstall.chosen": ["workbench-base"],
    };
    expect(seededComponents(bag, "AmigaOS 3.2").chosen).toEqual(["already-migrated"]);
  });

  it("answers empty lists rather than throwing on a bad value", () => {
    const bag = { "osinstall.chosen": "not a list" };
    expect(seededComponents(bag, "AmigaOS 3.2")).toEqual({
      chosen: [],
      excludedConditional: [],
    });
  });
});

describe("seedRom", () => {
  /// Three panels asked for the same Kickstart and each remembered its own
  /// (ART-197's fourth row). The migration has to find whichever of the three
  /// a user's own history filled — losing a ROM they already chose would be
  /// the settings-reset this project forbids outright.
  it("prefers the session's own key once it exists", () => {
    const bag = {
      [SESSION_KEYS.rom]: { path: "E:\\roms\\new.rom" },
      "osinstall.rom": "E:\\roms\\install.rom",
      "cardBuilder.kickstart": "E:\\roms\\card.rom",
      "amigaInstall.kickstart": "E:\\roms\\emulator.rom",
    };
    expect(seedRom(bag)).toBe("E:\\roms\\new.rom");
  });

  it("takes the install step's ROM first, because that is the one the pairing check reads", () => {
    const bag = {
      "osinstall.rom": "E:\\roms\\install.rom",
      "cardBuilder.kickstart": "E:\\roms\\card.rom",
      "amigaInstall.kickstart": "E:\\roms\\emulator.rom",
    };
    expect(seedRom(bag)).toBe("E:\\roms\\install.rom");
  });

  it("then the card step's", () => {
    const bag = {
      "cardBuilder.kickstart": "E:\\roms\\card.rom",
      "amigaInstall.kickstart": "E:\\roms\\emulator.rom",
    };
    expect(seedRom(bag)).toBe("E:\\roms\\card.rom");
  });

  /// **The one that would have been lost.** A user who never used the install
  /// step, and only ever ran a package installer, still has their ROM.
  it("and finally the Amiga-side install step's, which nothing else would find", () => {
    expect(seedRom({ "amigaInstall.kickstart": "E:\\roms\\emulator.rom" })).toBe(
      "E:\\roms\\emulator.rom"
    );
  });

  it("nothing chosen anywhere is null, not an empty string", () => {
    expect(seedRom({})).toBeNull();
  });
});

describe("seedCardImage", () => {
  /// ART-197's remaining duplicate. The card builder remembered where it was
  /// about to *write* an image and the volumes step remembered which image it
  /// was about to *prepare* — two keys for one card, so a user who had just
  /// watched ART lay out a 32 GB image was asked to go and find it.
  it("prefers the session's own key once it exists", () => {
    const bag = {
      [SESSION_KEYS.card]: { image: "E:\amiga\new.img" },
      "preload.image": "E:\amiga\picked.img",
      "cardBuilder.dest": "E:\amiga\written.img",
    };
    expect(seedCardImage(bag)).toBe("E:\amiga\new.img");
  });

  /// **The hand-made pick wins, and the order is the point.** `preload.image`
  /// is a card somebody went and chose; moving a setting is still changing it,
  /// which the remembered-settings rule forbids outright. `seedTreeRoot` takes
  /// the same order for the same reason.
  it("takes the card the user picked over the one ART wrote", () => {
    const bag = {
      "preload.image": "E:\amiga\picked.img",
      "cardBuilder.dest": "E:\amiga\written.img",
    };
    expect(seedCardImage(bag)).toBe("E:\amiga\picked.img");
  });

  /// **The one ART-197 is actually about.** This user never picked a card on
  /// the volumes step, because nothing ever told them they had to — they
  /// watched ART write one and expected the next step to know.
  it("falls back to the image the card builder last wrote", () => {
    expect(seedCardImage({ "cardBuilder.dest": "E:\amiga\written.img" })).toBe(
      "E:\amiga\written.img"
    );
  });

  it("nothing chosen anywhere is null, not an empty string", () => {
    expect(seedCardImage({})).toBeNull();
  });

  /// A hand-edited or stale settings file must fall back to the default
  /// rather than putting a bad value on screen — `recall`'s own rule, applied
  /// to the seed that feeds it.
  it("a session key holding the wrong shape falls through to the legacy keys", () => {
    const bag = {
      [SESSION_KEYS.card]: { image: 42 },
      "cardBuilder.dest": "E:\amiga\written.img",
    };
    expect(seedCardImage(bag)).toBe("E:\amiga\written.img");
  });
});

describe("seededMaterial", () => {
  /// **The order is the migration** — the same rule `seedTreeRoot` and
  /// `seedCardImage` state, one list further out. The four legacy sources are
  /// the four places the OS Builder used to ask for material, and they seed
  /// in the order their fields were drawn in.
  it("takes all four legacy sources, in the order the fields were drawn in", () => {
    const bag = {
      "osinstall.mediaFolder.AmigaOS 3.2.2": "E:\\media\\base",
      "osinstall.extraMediaFolders.AmigaOS 3.2.2": ["E:\\media\\hotfix", "E:\\media\\spare"],
      "osinstall.mediaFolder.update-3.2.2.AmigaOS 3.2.2": "E:\\media\\update",
      [LEGACY_KEYS.packagesSession]: { folder: "E:\\archives", chosen: [] },
    };
    expect(seededMaterial(bag, "AmigaOS 3.2.2").folders).toEqual([
      { path: "E:\\media\\base", layer: null },
      { path: "E:\\media\\hotfix", layer: null },
      { path: "E:\\media\\spare", layer: null },
      { path: "E:\\media\\update", layer: "update-3.2.2" },
      { path: "E:\\archives", layer: null },
    ]);
  });

  /// A layered release's own fields keep their tags, and the tag is what the
  /// plan request is built from — see `foldersForPlan`.
  it("tags a per-layer folder with the layer it was asked for", () => {
    const bag = {
      "osinstall.mediaFolder.base.AmigaOS 3.2.2": "E:\\media\\32",
      "osinstall.mediaFolder.update-3.2.2.AmigaOS 3.2.2": "E:\\media\\322",
    };
    expect(seededMaterial(bag, "AmigaOS 3.2.2").folders).toEqual([
      { path: "E:\\media\\32", layer: "base" },
      { path: "E:\\media\\322", layer: "update-3.2.2" },
    ]);
  });

  /// **The key shape that could go wrong.** AmigaOS 3.2 is the release before
  /// there was a picker, so `rememberedComponentKey` leaves its keys
  /// unsuffixed — and `osinstall.mediaFolder.AmigaOS 3.9` is then the exact
  /// shape of a *layer* key for 3.2. Reading it as one would drag the 3.9
  /// folder into the 3.2 list, which is ART-207 running backwards: the owner
  /// picked 3.2 with 3.9's folder remembered and got sixteen true refusals
  /// that together said something false.
  it("never reads another release's flat folder as a layer of the unsuffixed one", () => {
    const bag = {
      "osinstall.mediaFolder": "E:\\media\\32",
      "osinstall.mediaFolder.AmigaOS 3.9": "E:\\media\\39",
    };
    expect(seededMaterial(bag, "AmigaOS 3.2").folders).toEqual([
      { path: "E:\\media\\32", layer: null },
    ]);
    expect(seededMaterial(bag, "AmigaOS 3.9").folders).toEqual([
      { path: "E:\\media\\39", layer: null },
    ]);
  });

  /// One folder ART would otherwise read twice: `find_media_across` refuses
  /// every disk in it as ambiguous with itself, which is a refusal with no
  /// decision behind it. First spelling kept, first tag kept.
  it("dedupes by folder rather than by spelling, and keeps the first entry", () => {
    const bag = {
      "osinstall.mediaFolder": "E:\\media",
      "osinstall.extraMediaFolders": ["E:/media/", "E:\\MEDIA", "E:\\other"],
      [LEGACY_KEYS.packagesSession]: { folder: "e:\\media", chosen: [] },
    };
    expect(seededMaterial(bag, "AmigaOS 3.2").folders).toEqual([
      { path: "E:\\media", layer: null },
      { path: "E:\\other", layer: null },
    ]);
  });

  /// ART-089's mechanism, and the reason this is a *seed* rather than a
  /// write: `useRememberedShape` reaches for a fallback only while there is
  /// nothing stored. A list the user has touched — including one they have
  /// emptied — is what comes back, and the legacy keys are not consulted.
  it("a key the user touched this run is never overwritten by the migration", () => {
    const bag = {
      [SESSION_KEYS.material("AmigaOS 3.2")]: {
        folders: [{ path: "E:\\chosen-by-hand", layer: null }],
      },
      "osinstall.mediaFolder": "E:\\media",
      [LEGACY_KEYS.packagesSession]: { folder: "E:\\archives", chosen: [] },
    };
    // The seed is what `useRememberedShape` would fall back to; `recallInto`
    // is what it actually reads, and the stored list wins.
    expect(
      recallInto<MaterialChoice>(
        bag,
        SESSION_KEYS.material("AmigaOS 3.2"),
        MATERIAL_SPEC,
        seededMaterial(bag, "AmigaOS 3.2")
      ).folders
    ).toEqual([{ path: "E:\\chosen-by-hand", layer: null }]);

    // Emptied on purpose is emptied, not reseeded from the legacy keys.
    const emptied = { ...bag, [SESSION_KEYS.material("AmigaOS 3.2")]: { folders: [] } };
    expect(
      recallInto<MaterialChoice>(
        emptied,
        SESSION_KEYS.material("AmigaOS 3.2"),
        MATERIAL_SPEC,
        seededMaterial(emptied, "AmigaOS 3.2")
      ).folders
    ).toEqual([]);
  });

  it("answers an empty list rather than throwing on a bad or absent bag", () => {
    expect(seededMaterial({}, "AmigaOS 3.9").folders).toEqual([]);
    expect(seededMaterial(null, "AmigaOS 3.9").folders).toEqual([]);
    expect(seededMaterial({ "osinstall.mediaFolder": 42 }, "AmigaOS 3.2").folders).toEqual([]);
    // A hand-edited list of the wrong shape falls back to the seed, the way
    // every other stored value here does.
    expect(isMaterialFolders([{ path: "E:\\a", layer: null }])).toBe(true);
    expect(isMaterialFolders([{ path: "", layer: null }])).toBe(false);
    expect(isMaterialFolders([{ path: "E:\\a", layer: 7 }])).toBe(false);
    expect(isMaterialFolders("E:\\a")).toBe(false);
  });
});

describe("foldersForPlan", () => {
  const LAYERS: InstallLayer[] = [
    { id: "base", labelKey: "osinstall.layer.base32" },
    { id: "update-3.2.2", labelKey: "osinstall.layer.update322" },
  ];

  /// The unlayered request `plan()` reads: one folder plus the rest. Every
  /// folder in the list reaches it, which is what the flat field plus the
  /// added-folder bag did before.
  it("an unlayered release sends the first folder and every other as an extra", () => {
    const built = foldersForPlan(
      {
        folders: [
          { path: "E:\\media", layer: null },
          { path: "E:\\update", layer: null },
          { path: "E:\\archives", layer: null },
        ],
      },
      []
    );
    expect(built.mediaFolder).toBe("E:\\media");
    expect(built.extraMediaFolders).toEqual(["E:\\update", "E:\\archives"]);
    expect(built.mediaFolders).toEqual({});
    expect(built.unusedForPlan).toEqual([]);
  });

  it("an empty list sends an empty folder rather than undefined", () => {
    expect(foldersForPlan({ folders: [] }, [])).toEqual({
      mediaFolder: "",
      extraMediaFolders: [],
      mediaFolders: {},
      unusedForPlan: [],
    });
  });

  /// A layered recipe reads `media_folders` alone — `plan.rs` ignores the
  /// flat fields outright for one — so the request must carry the map and
  /// nothing else, exactly as the per-layer fields sent it.
  it("a layered release sends one folder per declared layer and no flat folder", () => {
    const built = foldersForPlan(
      {
        folders: [
          { path: "E:\\media\\32", layer: "base" },
          { path: "E:\\media\\322", layer: "update-3.2.2" },
        ],
      },
      LAYERS
    );
    expect(built.mediaFolder).toBe("");
    expect(built.extraMediaFolders).toEqual([]);
    expect(built.mediaFolders).toEqual({
      base: "E:\\media\\32",
      "update-3.2.2": "E:\\media\\322",
    });
    expect(built.unusedForPlan).toEqual([]);
  });

  /// **The folders a layered request cannot carry are named, not dropped.**
  /// A map holds one folder per layer, so an untagged folder, a second
  /// folder under a tag already taken, and a tag this release does not
  /// declare are all folders the readout resolves and the planner will not
  /// read. A screen that dropped them silently would be contradicting the
  /// core about what it is going to do.
  it("names every folder a layered request cannot carry", () => {
    const built = foldersForPlan(
      {
        folders: [
          { path: "E:\\media\\32", layer: "base" },
          { path: "E:\\media\\also-base", layer: "base" },
          { path: "E:\\media\\loose", layer: null },
          { path: "E:\\media\\gone", layer: "a-layer-that-was-dropped" },
        ],
      },
      LAYERS
    );
    expect(built.mediaFolders).toEqual({ base: "E:\\media\\32" });
    expect(built.unusedForPlan).toEqual([
      "E:\\media\\also-base",
      "E:\\media\\loose",
      "E:\\media\\gone",
    ]);
  });

  /// A tag can only be set while a release declares layers, so one on an
  /// unlayered release is a hand-edited file or a recipe that dropped a
  /// layer. The folder is still the user's; using it is what they asked for,
  /// and dropping it would be losing a choice they made.
  it("an unlayered release reads a tagged folder anyway rather than losing it", () => {
    const built = foldersForPlan(
      { folders: [{ path: "E:\\media\\32", layer: "base" }] },
      []
    );
    expect(built.mediaFolder).toBe("E:\\media\\32");
    expect(built.unusedForPlan).toEqual([]);
  });
});

describe("withFolder", () => {
  it("appends, and re-adding a folder neither duplicates it nor re-tags it", () => {
    const held = [{ path: "E:\\media", layer: "base" }];
    expect(withFolder(held, { path: "E:\\other", layer: null })).toEqual([
      { path: "E:\\media", layer: "base" },
      { path: "E:\\other", layer: null },
    ]);
    // Same folder, different spelling, different tag — the held entry wins,
    // and it is the same array back, so nothing downstream re-renders.
    expect(withFolder(held, { path: "e:/media/", layer: null })).toBe(held);
  });
});

describe("FIRSTBOOT_SPEC", () => {
  // **Absent means ticked** (four-tab design § 3.2, § 5). The tick is a new
  // field on a shape people already have stored, and the migration is
  // `recallInto`'s own mechanism rather than a written default: it starts
  // from the fallback and overwrites a field only where the *stored* object
  // holds a value the guard accepts.
  //
  // Both halves matter and either alone passes for the wrong reason. That
  // `wanted` is `undefined` after recalling `{ written: true }` says nothing
  // was invented for a user who has never seen the tick; that a stored
  // `false` comes back `false` says the guard is not throwing the user's own
  // choice away — a guard that rejected `false` would silently re-tick a box
  // they turned off, which is the remembered-settings rule broken in the
  // direction nobody notices.
  it("leaves wanted absent for a firstboot written by an ART that had no tick", () => {
    const recalled = recallInto<FirstBootChoice>(
      { [SESSION_KEYS.firstboot]: { written: true } },
      SESSION_KEYS.firstboot,
      FIRSTBOOT_SPEC,
      DEFAULT_FIRSTBOOT
    );
    expect(recalled.written).toBe(true);
    expect(recalled.wanted).toBeUndefined();
    expect("wanted" in recalled).toBe(false);
  });

  it("keeps a tick the user turned off", () => {
    expect(
      recallInto<FirstBootChoice>(
        { [SESSION_KEYS.firstboot]: { written: false, wanted: false } },
        SESSION_KEYS.firstboot,
        FIRSTBOOT_SPEC,
        DEFAULT_FIRSTBOOT
      ).wanted
    ).toBe(false);
    // And a tick they turned back on.
    expect(
      recallInto<FirstBootChoice>(
        { [SESSION_KEYS.firstboot]: { written: false, wanted: true } },
        SESSION_KEYS.firstboot,
        FIRSTBOOT_SPEC,
        DEFAULT_FIRSTBOOT
      ).wanted
    ).toBe(true);
  });

  it("ships no wanted in the default, so rendering the tick stores nothing", () => {
    // The default is what `useRememberedShape` hands back when nothing is
    // stored, and what its setter merges a change into. A `wanted: true`
    // here would be written to `settings.json` the first time anything else
    // on the session was set — a value the user never chose, indistinguishable
    // from one they did.
    expect("wanted" in DEFAULT_FIRSTBOOT).toBe(false);
  });
});
