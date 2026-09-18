// The card section's mappers (round 4, task 8).
//
// Every assertion here is about *which sentence* a typed answer from the core
// becomes — never about the wording, which lives in the catalogues and is
// checked by `src/i18n/phrase-keys.test.ts`. What this file is for is the
// pairing: a source the core called `not-usable` must not come out as
// "unknown", an overflow must name the partition the core named, and a
// planned Work split must count as one Work row rather than none.

import { describe, expect, it } from "vitest";

import type {
  CardImagePlan,
  CardRefusal,
  ClassifiedSource,
  MeasuredPartition,
  UnusableSource,
} from "@/lib/cardOs";
import {
  driverMissingPhrase,
  driverPhrase,
  isFixedPartition,
  measurableInputs,
  overflowPhrase,
  partitionBytes,
  partitionContentPhrase,
  sizePhrase,
  sourcePhrase,
  totalPhrase,
  volumeNameProblemPhrase,
} from "@/lib/cardOsMeasure";

function planWith(partitions: [string, number][], imageBytes: number): CardImagePlan {
  return {
    image_bytes: imageBytes,
    area_bytes: imageBytes - 1_181_116_006,
    partitions: partitions.map(([volume_name, bytes], index) => ({
      drive_name: `SDH${index}`,
      volume_name,
      bytes,
      spec: {} as CardImagePlan["partitions"][number]["spec"],
    })),
  };
}

describe("which partitions the request carries", () => {
  it("keeps System and Work off the request — the core adds both itself", () => {
    // `measure_card` measures System from the tree and `plan_card_image`
    // appends Work as the rest: sending either back would plan a second
    // volume with the same name.
    const inputs = measurableInputs([
      { name: "System", sources: [] },
      { name: "Games", sources: ["E:\\games"] },
      { name: "Work", sources: [] },
    ]);
    expect(inputs.map((p) => p.volumeName)).toEqual(["Games"]);
    expect(inputs[0].sources).toEqual(["E:\\games"]);
  });

  it("carries an Advanced floor through, and omits it when there is none", () => {
    const inputs = measurableInputs([
      { name: "Games", sources: [], floorBytes: 8_000_000_000 },
      { name: "Stuff", sources: [] },
    ]);
    expect(inputs[0].floorBytes).toBe(8_000_000_000);
    expect(inputs[1].floorBytes).toBeUndefined();
  });

  it("knows the two rows that are always there and cannot be removed", () => {
    expect(isFixedPartition("System")).toBe(true);
    expect(isFixedPartition("Work")).toBe(true);
    // A split Work (`split_work`) is still the Work row, not a user's own.
    expect(isFixedPartition("Work_1")).toBe(true);
    expect(isFixedPartition("Games")).toBe(false);
    expect(isFixedPartition("Workbench")).toBe(false);
  });
});

describe("what a source is, said in one phrase", () => {
  it("says the kind for each kind the core recognises", () => {
    const keys = (
      [
        { kind: "folder" },
        { kind: "archive", format: "lha" },
        { kind: "whdload-hardfile" },
        { kind: "adf" },
      ] as const
    ).map((kind) => sourcePhrase({ path: "E:\\x", kind, why: null }).key);
    // Four kinds, four sentences — no two share a key.
    expect(new Set(keys).size).toBe(4);
    for (const key of keys) expect(key.startsWith("cardSection.sourceKind.")).toBe(true);
  });

  /**
   * **The defect this case exists for.** A source the core refused used to be
   * indistinguishable from one it had not looked at yet — both rendered as
   * "unknown" — so a user was told nothing about the one thing they could
   * act on. Each refusal reason has its own sentence and its own next step.
   */
  it("says the reason for a source that cannot be used, never 'unknown'", () => {
    const reasons: UnusableSource[] = [
      { reason: "missing" },
      { reason: "unreadable", detail: "access denied" },
      { reason: "archive-unreadable", detail: "truncated" },
      { reason: "hardfile-not-whdload", detail: "no slave" },
      { reason: "not-an-amiga-source", formatHint: "exe" },
      { reason: "not-a-folder" },
    ];
    const keys = reasons.map(
      (why) => sourcePhrase({ path: "E:\\x", kind: null, why }).key
    );
    expect(new Set(keys).size).toBe(reasons.length);
    for (const key of keys) expect(key.startsWith("cardSection.unusable.")).toBe(true);
    // The detail the core gave is carried, not dropped.
    expect(sourcePhrase({ path: "E:\\x", kind: null, why: reasons[1] }).params).toMatchObject({
      detail: "access denied",
    });
  });

  it("says a source has not been classified yet rather than guessing", () => {
    expect(sourcePhrase(undefined).key).toBe("cardSection.source.pending");
  });
});

describe("sizes and the total", () => {
  it("scales a size to the unit it is readable in", () => {
    expect(sizePhrase(4_900_000_000)).toEqual({ key: "cardSection.size.gb", params: { value: "4.9" } });
    expect(sizePhrase(1_200_000)).toEqual({ key: "cardSection.size.mb", params: { value: "1.2" } });
    expect(sizePhrase(4_096)).toEqual({ key: "cardSection.size.kb", params: { value: "4.1" } });
    expect(sizePhrase(12)).toEqual({ key: "cardSection.size.bytes", params: { value: 12 } });
  });

  it("finds a partition's planned bytes by name, and sums a split Work", () => {
    const plan = planWith(
      [
        ["System", 838_860_800],
        ["Games", 4_900_000_000],
        ["Work", 50_000_000_000],
        ["Work_1", 5_000_000_000],
      ],
      60_800_000_000
    );
    expect(partitionBytes(plan, "Games")).toBe(4_900_000_000);
    expect(partitionBytes(plan, "Work")).toBe(55_000_000_000);
    expect(partitionBytes(plan, "Stuff")).toBeNull();
    expect(partitionBytes(null, "Games")).toBeNull();
  });

  it("states the total against the card's own bytes", () => {
    const plan = planWith(
      [
        ["System", 800_000_000],
        ["Games", 4_000_000_000],
        ["Work", 50_000_000_000],
      ],
      60_800_000_000
    );
    const phrase = totalPhrase(plan);
    expect(phrase.key).toBe("cardSection.total.line");
    // Both numbers: what the partitions come to, and what the card image is.
    expect(phrase.params).toMatchObject({ used: "54.8", total: "60.8", percent: 90 });
  });

  it("counts what a partition's sources hold", () => {
    const measured: MeasuredPartition = {
      volumeName: "Games",
      driveName: "SDH1",
      sources: [
        { path: "E:\\a", kind: { kind: "folder" }, files: 100, bytes: 1_000 },
        { path: "E:\\b.lha", kind: { kind: "archive", format: "lha" }, files: 12, bytes: 2_000 },
      ],
      writer: { writer: "native" },
    };
    expect(partitionContentPhrase(measured)).toEqual({
      key: "cardSection.content.summary",
      params: { sources: 2, count: 112 },
    });
    expect(partitionContentPhrase(undefined).key).toBe("cardSection.content.none");
  });
});

describe("an overflow names the partition and the bytes", () => {
  const refusal = (params: Record<string, string>): CardRefusal => ({
    code: "ART-CARD-DOES-NOT-FIT",
    message: "the card does not fit",
    params,
  });

  it("names the largest partition asked for when the whole card is short", () => {
    const phrase = overflowPhrase(
      refusal({
        kind: "does-not-fit",
        needed: "61000000000",
        available: "60000000000",
        largest: "Games",
      })
    );
    expect(phrase).not.toBeNull();
    expect(phrase!.key).toBe("cardSection.overflow.doesNotFit");
    expect(phrase!.params).toMatchObject({ partition: "Games" });
    // The bytes, not just "too big".
    expect(String(phrase!.params!.needed)).toContain("61");
    expect(String(phrase!.params!.available)).toContain("60");
  });

  it("has its own sentence when no partition can be named", () => {
    const phrase = overflowPhrase(
      refusal({ kind: "does-not-fit", needed: "61000000000", available: "60000000000" })
    );
    expect(phrase!.key).toBe("cardSection.overflow.doesNotFitNoPartition");
  });

  it("names the partition for every sizing refusal that has one", () => {
    expect(
      overflowPhrase(
        refusal({ kind: "partition-too-large", volumeName: "Games", bytes: "120000000000" })
      )!.params
    ).toMatchObject({ partition: "Games" });
    expect(
      overflowPhrase(
        refusal({
          kind: "partition-content-does-not-fit",
          volumeName: "System",
          neededBlocks: "2000000",
          availableBlocks: "1600000",
        })
      )!.params
    ).toMatchObject({ partition: "System" });
    expect(
      overflowPhrase(refusal({ kind: "card-too-small", cardGb: "16" }))!.params
    ).toMatchObject({ cardGb: "16" });
    expect(
      overflowPhrase(
        refusal({
          kind: "system-additions-do-not-fit",
          neededBlocks: "2000000",
          availableBlocks: "1600000",
          treeBlocks: "1000000",
          kickstarts: "kick34005.A500",
        })
      )!.key
    ).toBe("cardSection.overflow.systemAdditions");
  });

  it("is not the sentence for a refusal that is not about size", () => {
    // `errorPhrase` answers those; a sizing mapper that claimed them would
    // put an overflow sentence over a missing driver.
    expect(
      overflowPhrase({ code: "ART-CARD-SOURCE-UNUSABLE", message: "x", params: {} })
    ).toBeNull();
  });
});

describe("a volume name the core refuses", () => {
  it("says nothing for a name the core accepted", () => {
    expect(volumeNameProblemPhrase({ ok: true })).toBeNull();
  });

  it("says which rule was broken, with the core's own limit", () => {
    expect(volumeNameProblemPhrase({ ok: false, why: "empty", maxBytes: 30 })!.key).toBe(
      "cardSection.nameProblem.empty"
    );
    const long = volumeNameProblemPhrase({ ok: false, why: "too-long", maxBytes: 30 })!;
    expect(long.key).toBe("cardSection.nameProblem.tooLong");
    expect(long.params).toMatchObject({ max: 30 });
    expect(
      volumeNameProblemPhrase({ ok: false, why: "reserved-character", maxBytes: 30 })!.key
    ).toBe("cardSection.nameProblem.reservedCharacter");
  });
});

describe("the PFS3 driver line", () => {
  it("says which driver, and where it came from — an archive or a loose file", () => {
    const fromArchive = driverPhrase({
      path: "E:\\stage\\pfs3aio",
      fromArchive: "E:\\paketler\\pfs3aio.lha",
      version: 19,
      revision: 2,
    });
    expect(fromArchive.key).toBe("cardSection.driver.fromArchive");
    expect(fromArchive.params).toMatchObject({
      version: "19.2",
      from: "E:\\paketler\\pfs3aio.lha",
    });

    const loose = driverPhrase({
      path: "E:\\paketler\\pfs3aio",
      fromArchive: null,
      version: 19,
      revision: 2,
    });
    // A different sentence, because "from pfs3aio.lha" about a loose file is
    // a claim about where it came from that nobody made.
    expect(loose.key).toBe("cardSection.driver.loose");
    expect(loose.params).toMatchObject({ version: "19.2", at: "E:\\paketler\\pfs3aio" });
  });

  /** A refusal a user can fix with one download must not read like one they
   *  cannot fix: it names what to add **and** everywhere ART already looked. */
  it("says what to add and where ART looked when no driver was found", () => {
    const phrase = driverMissingPhrase({
      code: "ART-PFS3-DRIVER-NOT-FOUND",
      message: "No PFS3 driver was found.",
      params: { searched: "E:\\media, E:\\paketler", unreadable: "" },
    });
    expect(phrase!.key).toBe("cardSection.driver.missing");
    expect(phrase!.params).toMatchObject({ searched: "E:\\media, E:\\paketler" });
  });

  it("names the archives it could not read, when there were any", () => {
    const phrase = driverMissingPhrase({
      code: "ART-PFS3-DRIVER-NOT-FOUND",
      message: "No PFS3 driver was found.",
      params: { searched: "E:\\media", unreadable: "E:\\media\\broken.lha" },
    });
    expect(phrase!.key).toBe("cardSection.driver.missingUnreadable");
    expect(phrase!.params).toMatchObject({ unreadable: "E:\\media\\broken.lha" });
  });

  it("is not the sentence for any other refusal", () => {
    expect(
      driverMissingPhrase({ code: "ART-CARD-DOES-NOT-FIT", message: "x", params: {} })
    ).toBeNull();
  });
});

describe("a classified source's own path is kept", () => {
  it("carries the path into the phrase so a row can name the file", () => {
    const source: ClassifiedSource = {
      path: "E:\\games\\Turrican.lha",
      kind: { kind: "archive", format: "lha" },
      why: null,
    };
    expect(sourcePhrase(source).params).toMatchObject({ format: "lha" });
  });
});
