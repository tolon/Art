import { describe, expect, it } from "vitest";

import {
  copiedPhrase,
  fallbackPhrase,
  foldersToCheck,
  formatCount,
  needsExternalTool,
  pairingLines,
  pairingPhrase,
  pairingStillApplies,
  picksFor,
  plannedToolPhrase,
  preloadBlocker,
  stepPhrase,
  toRequest,
  type PartitionPick,
  type PreloadPlan,
  type PreloadStep,
} from "@/lib/preload";
import type { CardReport } from "@/lib/card";
import type { ParsedPartition } from "@/lib/hdf";

function partition(drive_name: string, dostype_str: string): ParsedPartition {
  return {
    drive_name,
    dostype: 0x50445303,
    dostype_str,
    fs_type: "pfs3directscsi",
    low_cyl: 2,
    high_cyl: 1000,
    cylinder_count: 999,
    size_bytes: 512 * 1024 * 1024,
    bootable: true,
    boot_priority: 0,
    num_buffers: 600,
    block_location: 3,
    next_part_block: 0xffffffff,
    checksum_valid: true,
  };
}

/** Two Amiga disks, the way MultibootOS's card has them. */
const CARD: CardReport = {
  card: {
    path: "E:\\amiga\\ProjeART\\card.img",
    total_bytes: 64 * 1024 * 1024 * 1024,
    mbr: { partitions: [] },
    areas: [
      {
        offset_bytes: 1_178_599_424,
        length_bytes: 32 * 1024 * 1024 * 1024,
        rdb: {
          partitions: [partition("DH0", "PDS\\3"), partition("DH1", "PDS\\3")],
          file_systems: [],
          checksum_valid: true,
        },
      },
      {
        offset_bytes: 34_000_000_000,
        length_bytes: 30 * 1024 * 1024 * 1024,
        rdb: {
          partitions: [partition("DH2", "PDS\\3")],
          file_systems: [],
          checksum_valid: true,
        },
      },
    ],
  },
  file_systems: [],
  unmountable: [],
};

const PLAN: PreloadPlan = {
  image: "E:\\amiga\\ProjeART\\card.img",
  steps: [
    {
      step: "format-partition",
      slot: 2,
      index: 1,
      drive_name: "DH0",
      volume_name: "Work",
    },
  ],
};

/** ART-117 — the one case the native path always refuses (embedding a
 *  driver into an existing card's RDB), so the plan itself already shows
 *  that hst-imager is needed. */
const IMPORT_PLAN: PreloadPlan = {
  image: "E:\\amiga\\ProjeART\\card.img",
  steps: [
    {
      step: "import-filesystem",
      slot: 2,
      driver: "pfs3aio.lha",
      dostype: "PDS3",
      name: "pfs3aio",
    },
    {
      step: "format-partition",
      slot: 2,
      index: 1,
      drive_name: "DH0",
      volume_name: "Work",
    },
  ],
};

/** The card's three partitions, with the first one chosen. */
function chosenFirst(): PartitionPick[] {
  const picks = picksFor(CARD);
  picks[0] = { ...picks[0], chosen: true, volumeName: "Work" };
  return picks;
}

describe("picksFor", () => {
  it("gives one pick per partition, numbered from one within its own disk", () => {
    expect(picksFor(CARD)).toEqual([
      { area: 1, index: 1, driveName: "DH0", chosen: false, volumeName: "DH0", content: null },
      { area: 1, index: 2, driveName: "DH1", chosen: false, volumeName: "DH1", content: null },
      { area: 2, index: 1, driveName: "DH2", chosen: false, volumeName: "DH2", content: null },
    ]);
  });

  it("chooses nothing: formatting is destructive and starts from off", () => {
    expect(picksFor(CARD).some((pick) => pick.chosen)).toBe(false);
  });

  it("defaults the volume name to the drive's own name rather than inventing one", () => {
    expect(picksFor(CARD).map((pick) => pick.volumeName)).toEqual(["DH0", "DH1", "DH2"]);
  });

  it("answers a card with no Amiga disk at all", () => {
    const empty: CardReport = { ...CARD, card: { ...CARD.card, areas: [] } };
    expect(picksFor(empty)).toEqual([]);
  });
});

describe("toRequest", () => {
  it("carries only the partitions that were chosen", () => {
    const request = toRequest("card.img", null, chosenFirst());
    expect(request.partitions).toEqual([
      { area: 1, index: 1, volume_name: "Work", content: null },
    ]);
  });

  it("keeps both numbers, because a partition index means nothing without its disk", () => {
    const picks = picksFor(CARD).map((pick) => ({ ...pick, chosen: true }));
    const request = toRequest("card.img", null, picks);
    expect(request.partitions.map((p) => [p.area, p.index])).toEqual([
      [1, 1],
      [1, 2],
      [2, 1],
    ]);
  });

  it("trims the volume name and passes a content folder through", () => {
    const picks = picksFor(CARD);
    picks[0] = { ...picks[0], chosen: true, volumeName: "  Work  ", content: "E:\\tree" };
    expect(toRequest("card.img", null, picks).partitions[0]).toEqual({
      area: 1,
      index: 1,
      volume_name: "Work",
      content: "E:\\tree",
    });
  });

  it("a driver nobody chose is absent, not an empty path", () => {
    expect(toRequest("card.img", "   ", chosenFirst()).driver).toBeNull();
    expect(toRequest("card.img", "pfs3aio.lha", chosenFirst()).driver).toBe("pfs3aio.lha");
  });
});

describe("preloadBlocker", () => {
  const ready = {
    image: "card.img",
    toolPath: "hst.imager.exe",
    picks: chosenFirst(),
    plan: PLAN,
  };

  it("is clear when a card, a tool, a chosen partition and a plan are all in hand", () => {
    expect(preloadBlocker(ready)).toBeNull();
  });

  it("asks for the card first", () => {
    expect(preloadBlocker({ ...ready, image: null })?.key).toBe("preload.blocked.noCard");
  });

  // ART-120: native is the default and needs no tool for an ordinary
  // preload — `PLAN` here has no `import-filesystem` step.
  it("does not require the tool when the plan does not need it", () => {
    expect(preloadBlocker({ ...ready, toolPath: null })).toBeNull();
    expect(preloadBlocker({ ...ready, toolPath: "  " })).toBeNull();
    expect(preloadBlocker({ ...ready, toolPath: "" })).toBeNull();
  });

  it("asks for the tool only when the plan needs it (ART-117)", () => {
    expect(needsExternalTool(PLAN)).toBe(false);
    expect(needsExternalTool(IMPORT_PLAN)).toBe(true);

    const withImport = { ...ready, plan: IMPORT_PLAN };
    expect(preloadBlocker({ ...withImport, toolPath: null })?.key).toBe("preload.blocked.noTool");
    expect(preloadBlocker({ ...withImport, toolPath: "  " })?.key).toBe(
      "preload.blocked.noTool"
    );
    expect(preloadBlocker(withImport)).toBeNull();
  });

  it("will not run over a card with nothing chosen", () => {
    expect(preloadBlocker({ ...ready, picks: picksFor(CARD) })?.key).toBe(
      "preload.blocked.nothingChosen"
    );
  });

  it("refuses a blank volume name, and says which drive it belongs to", () => {
    const picks = chosenFirst();
    picks[0] = { ...picks[0], volumeName: "   " };
    const blocker = preloadBlocker({ ...ready, picks });
    expect(blocker?.key).toBe("preload.blocked.blankName");
    expect(blocker?.params).toEqual({ drive: "DH0" });
  });

  // The same two rules `core/volume/write/dir.rs::check_name` holds, and for
  // the same reason: a name AmigaDOS cannot store is not a name.
  it("refuses a name carrying a path separator", () => {
    for (const bad of ["Work:", "Games/Old"]) {
      const picks = chosenFirst();
      picks[0] = { ...picks[0], volumeName: bad };
      expect(preloadBlocker({ ...ready, picks })?.key, bad).toBe("preload.blocked.badName");
    }
  });

  it("refuses a name past AmigaDOS's thirty characters, counting characters", () => {
    const picks = chosenFirst();
    picks[0] = { ...picks[0], volumeName: "W".repeat(31) };
    const blocker = preloadBlocker({ ...ready, picks });
    expect(blocker?.key).toBe("preload.blocked.longName");
    expect(blocker?.params).toEqual({ drive: "DH0", max: 30 });

    // Thirty accented characters are thirty characters, not sixty bytes.
    picks[0] = { ...picks[0], volumeName: "ü".repeat(30) };
    expect(preloadBlocker({ ...ready, picks })).toBeNull();
  });

  it("asks for a preview before a format, because §92 puts PREVIEW before APPLY", () => {
    expect(preloadBlocker({ ...ready, plan: null })?.key).toBe("preload.blocked.notPlanned");
  });
});

describe("formatCount", () => {
  it("counts the partitions a plan would erase and nothing else", () => {
    const plan: PreloadPlan = {
      image: "card.img",
      steps: [
        {
          step: "import-filesystem",
          slot: 2,
          driver: "pfs3aio.lha",
          dostype: "PDS3",
          name: "pfs3aio",
        },
        { step: "format-partition", slot: 2, index: 1, drive_name: "DH0", volume_name: "Work" },
        { step: "format-partition", slot: 2, index: 2, drive_name: "DH1", volume_name: "Games" },
        { step: "copy-in", slot: 2, drive_name: "DH0", source: "E:\\tree" },
      ],
    };
    expect(formatCount(plan)).toBe(2);
    expect(formatCount({ image: "card.img", steps: [] })).toBe(0);
  });
});

describe("stepPhrase", () => {
  it("names the drive a format would erase, not just the step", () => {
    const phrase = stepPhrase({
      step: "format-partition",
      slot: 2,
      index: 1,
      drive_name: "DH0",
      volume_name: "Work",
    });
    expect(phrase.key).toBe("preload.plan.step.format");
    expect(phrase.params).toEqual({ drive: "DH0", volume: "Work" });
  });
});

describe("plannedToolPhrase", () => {
  // fix-wave finding 3: the preview must say which writer is expected to
  // run a step *before* the confirmation, not only after the run in the
  // result panel. `import-filesystem` is a static fact (ART-117 always needs
  // the fallback); `copy-in` names the *possibility* of ART-113 rather than a
  // verdict — see this file's own header comment on why that gap cannot be
  // known ahead of time; and a `format-partition` inherits its own
  // partition's copy (ART-122).
  const format = {
    step: "format-partition",
    slot: 2,
    index: 1,
    drive_name: "DH0",
    volume_name: "Work",
  } as const;
  const copy = {
    step: "copy-in",
    slot: 2,
    drive_name: "DH0",
    source: "E:\\tree",
  } as const;
  const planOf = (...steps: PreloadStep[]): PreloadPlan => ({ image: "card.img", steps });

  it("names hst-imager for import-filesystem, unconditionally", () => {
    const step = {
      step: "import-filesystem",
      slot: 2,
      driver: "pfs3aio.lha",
      dostype: "PDS3",
      name: "pfs3aio",
    } as const;
    expect(plannedToolPhrase(step, planOf(step))).toEqual({
      key: "preload.plan.step.tool.hstImager",
    });
  });

  it("names ART's own writer for a format with nothing copied into it", () => {
    expect(plannedToolPhrase(format, planOf(format))).toEqual({
      key: "preload.plan.step.tool.native",
    });
  });

  // ART-122: a volume is formatted and filled by one tool, so a format whose
  // partition is also filled is exactly as conditional as that copy. Saying
  // "ART's own writer does this" against a destructive step that may well run
  // on hst-imager is the untrue label this function exists to prevent.
  it("makes a format conditional when its own partition is filled too", () => {
    expect(plannedToolPhrase(format, planOf(format, copy))).toEqual({
      key: "preload.plan.step.tool.formatConditional",
    });
  });

  it("does not let another partition's copy make a format conditional", () => {
    const elsewhere = { ...copy, drive_name: "DH1" } as const;
    expect(plannedToolPhrase(format, planOf(format, elsewhere))).toEqual({
      key: "preload.plan.step.tool.native",
    });
    const otherSlot = { ...copy, slot: 3 } as const;
    expect(plannedToolPhrase(format, planOf(format, otherSlot))).toEqual({
      key: "preload.plan.step.tool.native",
    });
  });

  it("names ART's own writer for copy-in, with the ASCII caveat", () => {
    expect(plannedToolPhrase(copy, planOf(copy))).toEqual({
      key: "preload.plan.step.tool.nativeConditional",
    });
  });
});

describe("copiedPhrase", () => {
  // ART-125: the tool that does a fallback copy answers in rounded units
  // ("12.2 MB"), so ART has no byte total for that run — and printing the
  // 0 it used to default to told the user a twelve-megabyte copy moved
  // nothing. The clause goes; the counts beside it are exact and stay.
  const counts = { files: 3933, directories: 280, comments_lost: 0, dates_lost: 0 };

  it("prints the byte total when there is one", () => {
    expect(copiedPhrase({ ...counts, bytes: 12_651_178 })).toEqual({
      key: "preload.result.copied",
      params: { files: 3933, directories: 280, bytes: 12_651_178 },
    });
  });

  it("says nothing about bytes when ART has no total", () => {
    expect(copiedPhrase({ ...counts, bytes: null })).toEqual({
      key: "preload.result.copiedNoBytes",
      params: { files: 3933, directories: 280 },
    });
  });

  it("keeps a real zero, which is not the same answer", () => {
    const phrase = copiedPhrase({ ...counts, files: 0, directories: 0, bytes: 0 });
    expect(phrase.key).toBe("preload.result.copied");
    expect(phrase.params).toEqual({ files: 0, directories: 0, bytes: 0 });
  });
});

describe("fallbackPhrase", () => {
  it("names ART-117 with no parameters", () => {
    expect(fallbackPhrase({ reason: "foreign-rdb-embed" })).toEqual({
      key: "preload.fallback.foreignRdbEmbed",
    });
  });

  // ART-122: the format's reason is the pairing, not the copy's own reason —
  // "a name is not ASCII" is not a fact about formatting a partition.
  it("names the drive whose copy pulled a format across, for ART-122", () => {
    expect(fallbackPhrase({ reason: "paired-with-fallback-copy", drive: "DH0" })).toEqual({
      key: "preload.fallback.pairedWithFallbackCopy",
      params: { drive: "DH0" },
    });
  });

  it("counts the bounded names plus the rest, for ART-113", () => {
    const phrase = fallbackPhrase({
      reason: "non-ascii-pfs3-names",
      paths: ["Locale/español", "Locale/français"],
      more: 22,
    });
    expect(phrase.key).toBe("preload.fallback.nonAsciiPfs3Names");
    expect(phrase.params).toEqual({ count: 24, paths: "Locale/español, Locale/français" });
  });
});

describe("pairingPhrase", () => {
  it("says nothing when the card carries the very ROM the tree was built for", () => {
    expect(pairingPhrase({ verdict: "paired" })).toBeNull();
  });

  it("names the ROM when it is a different but sufficient one", () => {
    expect(pairingPhrase({ verdict: "suitable", rom: "kick.rom" })).toEqual({
      key: "preload.pairing.suitable",
      params: { rom: "kick.rom" },
    });
  });

  it("gives both versions when the card's ROM is too old", () => {
    expect(
      pairingPhrase({ verdict: "unsuitable", needs: 47, found: 40, rom: "kick.rom" })
    ).toEqual({
      key: "preload.pairing.unsuitable47",
      params: { needs: 47, found: 40, rom: "kick.rom" },
    });
  });

  // The quoted AmigaOS message names V47 in its own words. The threshold is
  // the recipe's, not ART's, so the moment a recipe names another one the
  // quote stops being true — and the sentence that carries no quote is the
  // one that gets used.
  it("only quotes the observed V47 message when 47 is what is needed", () => {
    expect(
      pairingPhrase({ verdict: "unsuitable", needs: 45, found: 40, rom: "kick.rom" })
    ).toEqual({
      key: "preload.pairing.unsuitable",
      params: { needs: 45, found: 40, rom: "kick.rom" },
    });
  });

  it("has a sentence for a ROM that states no version at all", () => {
    expect(
      pairingPhrase({ verdict: "unsuitable", needs: 47, found: null, rom: "kick.rom" })
    ).toEqual({
      key: "preload.pairing.unsuitableUnknown",
      params: { needs: 47, rom: "kick.rom" },
    });
  });

  it("says which side did not answer, and never passes", () => {
    expect(pairingPhrase({ verdict: "not-checked", why: "tree-records-no-rom" })).toEqual({
      key: "preload.pairing.notChecked.tree",
    });
    expect(pairingPhrase({ verdict: "not-checked", why: "card-records-no-rom" })).toEqual({
      key: "preload.pairing.notChecked.card",
    });
  });

  // The fourth silence: the command itself rejecting used to be rendered
  // exactly like "checked, and the ROM is the one you built for".
  it("has a sentence for the check failing outright", () => {
    expect(pairingPhrase({ verdict: "not-checked", why: "check-failed" })).toEqual({
      key: "preload.pairing.notChecked.failed",
    });
  });
});

describe("foldersToCheck", () => {
  const pick = (driveName: string, chosen: boolean, content: string | null) => ({
    area: 1,
    index: 1,
    driveName,
    chosen,
    volumeName: driveName,
    content,
  });

  it("takes every chosen partition that has a folder, not the first", () => {
    expect(
      foldersToCheck([
        pick("DH0", true, "E:\\staging"),
        pick("DH1", true, "E:\\dist-3.2b"),
        pick("DH2", true, null),
        pick("DH3", false, "E:\\not-chosen"),
      ])
    ).toEqual([
      { driveName: "DH0", content: "E:\\staging" },
      { driveName: "DH1", content: "E:\\dist-3.2b" },
    ]);
  });
});

describe("pairingLines", () => {
  it("drops the folders with nothing to say and keeps the rest named", () => {
    expect(
      pairingLines([
        { driveName: "DH0", pairing: { verdict: "paired" } },
        {
          driveName: "DH1",
          pairing: { verdict: "unsuitable", needs: 47, found: 40, rom: "kick.rom" },
        },
      ])
    ).toEqual([
      {
        driveName: "DH1",
        verdict: "unsuitable",
        phrase: {
          key: "preload.pairing.unsuitable47",
          params: { needs: 47, found: 40, rom: "kick.rom" },
        },
      },
    ]);
  });

  it("says nothing at all when every folder is paired", () => {
    expect(
      pairingLines([
        { driveName: "DH0", pairing: { verdict: "paired" } },
        { driveName: "DH1", pairing: { verdict: "paired" } },
      ])
    ).toEqual([]);
  });
});

describe("pairingStillApplies", () => {
  // The invalidation check the pairing effect makes *before* issuing a new
  // fetch — this is the decision that was missing, letting a verdict fetched
  // for one card/folder sit on screen beside a plan for a different one
  // until the new fetch happened to resolve.
  it("holds when the verdict was fetched for the request now on screen", () => {
    expect(pairingStillApplies("fp-a", "fp-a")).toBe(true);
  });

  it("does not hold once the request has moved on", () => {
    expect(pairingStillApplies("fp-a", "fp-b")).toBe(false);
  });

  it("never holds when nothing has been fetched yet", () => {
    expect(pairingStillApplies(null, "fp-a")).toBe(false);
  });
});
