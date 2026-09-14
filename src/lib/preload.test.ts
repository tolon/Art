import { describe, expect, it } from "vitest";

import {
  backupDefaultName,
  copiedPhrase,
  editsRdb,
  embedDetailPhrases,
  embeddedPhrase,
  fallbackPhrase,
  foldersToCheck,
  formatCount,
  pairingLines,
  pairingPhrase,
  pairingStillApplies,
  picksFor,
  planNotePhrase,
  plannedToolPhrase,
  preloadBlocker,
  stepPhrase,
  toRequest,
  versionText,
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

/** ART-117 — an append into the card's RDB, with `RDBBlocksHi` raised. */
const EMBED_STEP: Extract<PreloadStep, { step: "import-filesystem" }> = {
  step: "import-filesystem",
  slot: 2,
  driver: "pfs3aio",
  dostype: "PDS3",
  name: "pfs3aio",
  file_version: { version: 19, revision: 3 },
  blocks: [2, 130],
  rdb_blocks_hi_raised: [1, 130],
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
  notes: [],
  rdb_backup: null,
};

/** A plan that edits the card's RDB, so it needs a backup path. */
const IMPORT_PLAN: PreloadPlan = {
  image: "E:\\amiga\\ProjeART\\card.img",
  steps: [
    EMBED_STEP,
    { step: "format-partition", slot: 2, index: 1, drive_name: "DH0", volume_name: "Work" },
  ],
  notes: [],
  rdb_backup: null,
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
    const request = toRequest("card.img", null, chosenFirst(), null);
    expect(request.partitions).toEqual([
      { area: 1, index: 1, volume_name: "Work", content: null },
    ]);
  });

  it("keeps both numbers, because a partition index means nothing without its disk", () => {
    const picks = picksFor(CARD).map((pick) => ({ ...pick, chosen: true }));
    const request = toRequest("card.img", null, picks, null);
    expect(request.partitions.map((p) => [p.area, p.index])).toEqual([
      [1, 1],
      [1, 2],
      [2, 1],
    ]);
  });

  it("trims the volume name and passes a content folder through", () => {
    const picks = picksFor(CARD);
    picks[0] = { ...picks[0], chosen: true, volumeName: "  Work  ", content: "E:\\tree" };
    expect(toRequest("card.img", null, picks, null).partitions[0]).toEqual({
      area: 1,
      index: 1,
      volume_name: "Work",
      content: "E:\\tree",
    });
  });

  it("a driver nobody chose is absent, not an empty path", () => {
    expect(toRequest("card.img", "   ", chosenFirst(), null).driver).toBeNull();
    expect(toRequest("card.img", "pfs3aio.lha", chosenFirst(), null).driver).toBe("pfs3aio.lha");
  });

  it("carries the RDB backup path, and a blank one is absent", () => {
    expect(toRequest("card.img", null, chosenFirst(), "  E:\\rdb.bin ").rdb_backup).toBe(
      "E:\\rdb.bin"
    );
    expect(toRequest("card.img", null, chosenFirst(), "   ").rdb_backup).toBeNull();
    expect(toRequest("card.img", null, chosenFirst(), null).rdb_backup).toBeNull();
  });
});

describe("preloadBlocker", () => {
  const ready = {
    image: "card.img",
    rdbBackup: null as string | null,
    picks: chosenFirst(),
    plan: PLAN,
  };

  it("is clear when a card, a chosen partition and a plan are in hand", () => {
    expect(preloadBlocker(ready)).toBeNull();
  });

  it("asks for the card first", () => {
    expect(preloadBlocker({ ...ready, image: null })?.key).toBe("preload.blocked.noCard");
  });

  // ART-117, decision 11: an RDB edit needs a backup path; nothing else does.
  it("asks for a backup path only when the plan edits the RDB", () => {
    expect(editsRdb(PLAN)).toBe(false);
    expect(editsRdb(IMPORT_PLAN)).toBe(true);
    expect(preloadBlocker({ ...ready, plan: IMPORT_PLAN })?.key).toBe("preload.blocked.noBackup");
    expect(preloadBlocker({ ...ready, plan: IMPORT_PLAN, rdbBackup: "  " })?.key).toBe(
      "preload.blocked.noBackup"
    );
    expect(preloadBlocker({ ...ready, plan: IMPORT_PLAN, rdbBackup: "E:\\rdb.bin" })).toBeNull();
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
        EMBED_STEP,
        { step: "format-partition", slot: 2, index: 1, drive_name: "DH0", volume_name: "Work" },
        { step: "format-partition", slot: 2, index: 2, drive_name: "DH1", volume_name: "Games" },
        { step: "copy-in", slot: 2, drive_name: "DH0", source: "E:\\tree" },
      ],
      notes: [],
      rdb_backup: null,
    };
    expect(formatCount(plan)).toBe(2);
    expect(formatCount({ image: "card.img", steps: [], notes: [], rdb_backup: null })).toBe(0);
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

  it("names the driver's version for an embed", () => {
    expect(stepPhrase(EMBED_STEP)).toEqual({
      key: "preload.plan.step.import",
      params: { name: "pfs3aio", dostype: "PDS3", version: "19.3" },
    });
  });

  // Decision 11: "PDS\3 19.2 on the card → 19.3 from the file".
  it("names both versions for a replace", () => {
    expect(
      stepPhrase({
        step: "replace-filesystem",
        slot: 2,
        driver: "pfs3aio",
        dostype: "PDS3",
        name: "pfs3aio",
        card_version: { version: 19, revision: 2 },
        file_version: { version: 19, revision: 10 },
        blocks: [132, 260],
        rdb_blocks_hi_raised: null,
      })
    ).toEqual({
      key: "preload.plan.step.replace",
      params: { name: "pfs3aio", dostype: "PDS3", card: "19.2", file: "19.10" },
    });
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
  const planOf = (...steps: PreloadStep[]): PreloadPlan => ({
    image: "card.img",
    steps,
    notes: [],
    rdb_backup: null,
  });

  it("names ART's own RDB editor for both edit steps", () => {
    expect(plannedToolPhrase(EMBED_STEP, planOf(EMBED_STEP))).toEqual({
      key: "preload.plan.step.tool.nativeEmbed",
    });
    const replace: PreloadStep = {
      ...EMBED_STEP,
      step: "replace-filesystem",
      card_version: { version: 19, revision: 2 },
    };
    expect(plannedToolPhrase(replace, planOf(replace))).toEqual({
      key: "preload.plan.step.tool.nativeEmbed",
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

describe("embedDetailPhrases", () => {
  it("names the blocks and says a backup is still to be chosen", () => {
    expect(embedDetailPhrases(EMBED_STEP, IMPORT_PLAN)).toEqual([
      { key: "preload.plan.step.blocksRaised", params: { first: 2, last: 130, from: 1, to: 130 } },
      { key: "preload.plan.step.backupMissing" },
    ]);
  });

  it("names the backup once there is one, and plain blocks when nothing is raised", () => {
    const step: PreloadStep = { ...EMBED_STEP, rdb_blocks_hi_raised: null };
    expect(embedDetailPhrases(step, { ...IMPORT_PLAN, rdb_backup: "E:\\rdb.bin" })).toEqual([
      { key: "preload.plan.step.blocks", params: { first: 2, last: 130 } },
      { key: "preload.plan.step.backup", params: { backup: "E:\\rdb.bin" } },
    ]);
  });

  it("says nothing for a step that is not an RDB edit", () => {
    expect(embedDetailPhrases(PLAN.steps[0], PLAN)).toEqual([]);
  });
});

describe("planNotePhrase", () => {
  const card = { version: 19, revision: 2 };
  it("says the card's driver stays, with both versions", () => {
    expect(
      planNotePhrase({ note: "driver-kept", dostype: "PDS3", card_version: card, file_version: card })
    ).toEqual({ key: "preload.plan.note.kept", params: { dostype: "PDS3", card: "19.2", file: "19.2" } });
    expect(
      planNotePhrase({ note: "driver-kept", dostype: "PDS3", card_version: card, file_version: null })
    ).toEqual({ key: "preload.plan.note.keptNoVersion", params: { dostype: "PDS3", card: "19.2" } });
  });

  it("carries a refusal's own sentence and code", () => {
    expect(
      planNotePhrase({
        note: "replace-refused",
        dostype: "PDS3",
        card_version: card,
        code: "ART-RDB-EDIT-NO-ROOM",
        detail: "the driver needs 129 blocks",
      })
    ).toEqual({
      key: "preload.plan.note.replaceRefused",
      params: { dostype: "PDS3", card: "19.2", detail: "the driver needs 129 blocks", code: "ART-RDB-EDIT-NO-ROOM" },
    });
    // Spec decision 13: both program names, or only the file's.
    expect(
      planNotePhrase({
        note: "different-driver",
        dostype: "SFS0",
        card_version: { version: 1, revision: 293 },
        card_name: "SmartFilesystem",
        file_name: "pfs3aio",
      })
    ).toEqual({
      key: "preload.plan.note.differentDriver",
      params: { dostype: "SFS0", card: "1.293", cardName: "SmartFilesystem", fileName: "pfs3aio" },
    });
    expect(
      planNotePhrase({ note: "different-driver", dostype: "PDS3", card_version: card, card_name: null, file_name: "pfs3aio" })
    ).toEqual({
      key: "preload.plan.note.differentDriverUnknown",
      params: { dostype: "PDS3", card: "19.2", fileName: "pfs3aio" },
    });
    // Final review I1: a file whose $VER: names no program, against a named
    // and an unnamed card driver.
    expect(
      planNotePhrase({
        note: "different-driver",
        dostype: "SFS0",
        card_version: { version: 1, revision: 293 },
        card_name: "SmartFilesystem",
        file_name: null,
      })
    ).toEqual({
      key: "preload.plan.note.differentDriverFileUnnamed",
      params: { dostype: "SFS0", card: "1.293", cardName: "SmartFilesystem" },
    });
    expect(
      planNotePhrase({ note: "different-driver", dostype: "PDS3", card_version: card, card_name: null, file_name: null })
    ).toEqual({
      key: "preload.plan.note.differentDriverBothUnnamed",
      params: { dostype: "PDS3", card: "19.2" },
    });
    expect(planNotePhrase({ note: "second-edit-skipped", dostype: "DOS3" })).toEqual({
      key: "preload.plan.note.secondEdit",
      params: { dostype: "DOS3" },
    });
  });
});

describe("embeddedPhrase", () => {
  const report = {
    slot: 2,
    dostype: "PDS3",
    card_version: null,
    file_version: { version: 19, revision: 3 },
    first_block: 132,
    last_block: 260,
    rdb_blocks_hi_raised: null,
    backup: "E:\\rdb.bin",
  };
  it("names what went where and where the backup is", () => {
    expect(embeddedPhrase(report)).toEqual({
      key: "preload.result.embedded",
      params: { dostype: "PDS3", file: "19.3", first: 132, last: 260, backup: "E:\\rdb.bin" },
    });
    expect(embeddedPhrase({ ...report, card_version: { version: 19, revision: 2 } })).toEqual({
      key: "preload.result.replaced",
      params: { dostype: "PDS3", file: "19.3", first: 132, last: 260, backup: "E:\\rdb.bin", card: "19.2" },
    });
  });

  // Final review I3: a run that stopped after the edit says so, with the
  // same parts — the backup included.
  it("says the change stays when the run stopped after it", () => {
    expect(embeddedPhrase(report, true)).toEqual({
      key: "preload.result.stoppedAfterEmbedded",
      params: { dostype: "PDS3", file: "19.3", first: 132, last: 260, backup: "E:\\rdb.bin" },
    });
    expect(embeddedPhrase({ ...report, card_version: { version: 19, revision: 2 } }, true)).toEqual({
      key: "preload.result.stoppedAfterReplaced",
      params: { dostype: "PDS3", file: "19.3", first: 132, last: 260, backup: "E:\\rdb.bin", card: "19.2" },
    });
  });
});

describe("backupDefaultName", () => {
  it("is the card's stem with -rdb-backup.bin, on either separator", () => {
    expect(backupDefaultName("E:\\amiga\\CaffeineOS_Storm_9317.img")).toBe("CaffeineOS_Storm_9317-rdb-backup.bin");
    expect(backupDefaultName("/cards/work.hdf")).toBe("work-rdb-backup.bin");
    expect(backupDefaultName("card")).toBe("card-rdb-backup.bin");
    expect(backupDefaultName("")).toBe("card-rdb-backup.bin");
  });

  it("prints a version the way the FSHD states it", () => {
    expect(versionText({ version: 19, revision: 10 })).toBe("19.10");
  });
});
