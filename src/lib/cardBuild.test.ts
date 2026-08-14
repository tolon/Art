import { describe, expect, it } from "vitest";

import {
  buildBlocker,
  defaultPartition,
  findingPhrase,
  healthVerdict,
  payloadBytes,
  warningPhrase,
  type CardBuildPlan,
  type CardBuildRequest,
  type HealthItem,
} from "@/lib/cardBuild";
import type { MbrPartition } from "@/lib/card";
import { DEFAULT_EMU68_OPTIONS, DEFAULT_FIRMWARE_CONFIG } from "@/lib/pistorm";

function slot(index: number, start_lba: number, sector_count: number): MbrPartition {
  return {
    index,
    kind: index === 1 ? { kind: "fat32" } : { kind: "amiga-rdb" },
    type_byte: index === 1 ? 0x0c : 0x76,
    bootable: index === 1,
    start_lba,
    sector_count,
  };
}

const REQUEST: CardBuildRequest = {
  archive: "E:\\amiga\\Emu68-pistorm.zip",
  kickstart: null,
  dest: "E:\\amiga\\ProjeART\\card.img",
  total_bytes: 2 * 1024 * 1024 * 1024,
  boot_bytes: 0,
  label: "ART CARD",
  hardware: { amiga: "a500", variant: "classic", pi: "pi3-a-plus" },
  line: "stable",
  firmware: DEFAULT_FIRMWARE_CONFIG,
  options: DEFAULT_EMU68_OPTIONS,
  partitions: [defaultPartition()],
};

const PLAN: CardBuildPlan = {
  layout: {
    total_sectors: 4194304,
    boot: slot(1, 2048, 2299904),
    areas: [slot(2, 2301952, 1892352)],
  },
  boot_files: [
    { name: "Emu68-pistorm.gz", bytes: 1_000_000 },
    { name: "config.txt", bytes: 512 },
  ],
  kernel_file: "Emu68-pistorm.gz",
  kickstart_file: null,
  rom: null,
  warnings: [{ kind: "volumes-unformatted" }],
  dest_exists: false,
};

describe("buildBlocker", () => {
  it("is clear when a plan is in hand and the destination is free", () => {
    expect(buildBlocker(REQUEST, PLAN)).toBeNull();
  });

  it("asks for an archive before anything else", () => {
    expect(buildBlocker({ ...REQUEST, archive: "" }, PLAN)?.key).toBe(
      "cardBuilder.blocked.noArchive"
    );
  });

  it("asks where the image goes", () => {
    expect(buildBlocker({ ...REQUEST, dest: "  " }, PLAN)?.key).toBe(
      "cardBuilder.blocked.noDestination"
    );
  });

  // §92: nothing is written until the user has seen what would be written.
  it("will not build a card nobody has previewed", () => {
    expect(buildBlocker(REQUEST, null)?.key).toBe("cardBuilder.blocked.notPlanned");
  });

  // SAFE_CREATE, said before the button rather than by a failing job.
  it("refuses a destination that is already there", () => {
    expect(buildBlocker(REQUEST, { ...PLAN, dest_exists: true })?.key).toBe(
      "cardBuilder.blocked.destExists"
    );
  });

  it("wants at least one partition on the Amiga disk", () => {
    expect(buildBlocker({ ...REQUEST, partitions: [] }, PLAN)?.key).toBe(
      "cardBuilder.blocked.noPartitions"
    );
  });
});

describe("warningPhrase", () => {
  it("names the machine a recognised ROM does not suit", () => {
    const phrase = warningPhrase({ kind: "rom-wrong-machine", rom: "Kickstart 3.1 (A600)" });
    expect(phrase.key).toBe("cardBuilder.warning.romWrongMachine");
    expect(phrase.params).toEqual({ rom: "Kickstart 3.1 (A600)" });
  });
});

describe("healthVerdict", () => {
  const item = (state: "pass" | "fail" | "not-checked"): HealthItem => ({
    check: { kind: "nothing-overlaps" },
    state,
  });

  it("says it passed when every check answered and answered well", () => {
    expect(healthVerdict({ items: [item("pass")], by_hand: [] }).key).toBe(
      "cardBuilder.health.passed"
    );
  });

  // A tick meaning "ART did not look" must not read like one meaning "ART
  // looked and it is right" (§89).
  it("never says it passed without saying how much went unanswered", () => {
    const verdict = healthVerdict({
      items: [item("pass"), item("not-checked"), item("not-checked")],
      by_hand: [],
    });
    expect(verdict.key).toBe("cardBuilder.health.passedWithGaps");
    expect(verdict.params).toEqual({ unanswered: 2 });
  });

  it("a failure outranks anything unanswered", () => {
    const verdict = healthVerdict({
      items: [item("fail"), item("not-checked")],
      by_hand: [],
    });
    expect(verdict.key).toBe("cardBuilder.health.failed");
    expect(verdict.params).toEqual({ count: 1 });
  });
});

describe("findingPhrase", () => {
  it("names the area whose RDB changed, counting from one", () => {
    const phrase = findingPhrase({ kind: "rdb-changed", area: 0 });
    expect(phrase.key).toBe("cardBuilder.manifest.finding.rdbChanged");
    expect(phrase.params).toEqual({ n: 1 });
  });
});

describe("payloadBytes", () => {
  it("adds up what the boot partition will hold", () => {
    expect(payloadBytes(PLAN.boot_files)).toBe(1_000_512);
  });
});

describe("defaultPartition", () => {
  // FFS because Kickstart mounts it itself: SD-1 embeds no filesystem driver,
  // and a PDS\3 partition with no driver is one an Amiga ignores in silence
  // (ART-084).
  it("is a bootable FFS drive an Amiga can mount without a driver", () => {
    const part = defaultPartition();
    expect(part.fs_type).toBe("ffsstandard");
    expect(part.bootable).toBe(true);
    expect(part.drive_name).toBe("SDH0");
  });
});
