import { describe, expect, it } from "vitest";

import {
  seedCardImage,
  seedRom,
  seedTreeRoot,
  seededComponents,
  SESSION_KEYS,
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
