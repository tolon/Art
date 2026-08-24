import { describe, expect, it } from "vitest";

import {
  buildBlocker,
  secondSystem,
  cardFsChoices,
  defaultPartition,
  defaultSecondPartition,
  findingPhrase,
  healthVerdict,
  intakeFills,
  payloadBytes,
  warningPhrase,
  type CardBuildPlan,
  type CardBuildRequest,
  type CardIntakeItem,
  type HealthItem,
} from "@/lib/cardBuild";
import { fileSystemInputsFor } from "@/lib/fsDriver";
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
  file_systems: [],
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

describe("intakeFills", () => {
  const item = (name: string, role: CardIntakeItem["role"]): CardIntakeItem => ({
    path: `E:\\drop\\${name}`,
    name,
    role,
    rom: null,
  });

  it("an Emu68 archive fills the archive field and a ROM fills the ROM field", () => {
    const fills = intakeFills([
      item("Emu68-pistorm.zip", { kind: "emu68-archive", means: [] }),
      item("kick.rom", { kind: "kickstart" }),
    ]);
    expect(fills.archive).toBe("E:\\drop\\Emu68-pistorm.zip");
    expect(fills.kickstart).toBe("E:\\drop\\kick.rom");
  });

  // Dropping a second archive says "this one now". The rule is written down
  // rather than left to whatever the loop happened to do.
  it("the last archive dropped is the one chosen", () => {
    const fills = intakeFills([
      item("Emu68-pistorm.zip", { kind: "emu68-archive", means: [] }),
      item("Emu68-pistorm-classic.zip", { kind: "emu68-archive", means: [] }),
    ]);
    expect(fills.archive).toBe("E:\\drop\\Emu68-pistorm-classic.zip");
  });

  // The answer SD-1 owes most often: the file is fine, the card is not ready.
  it("Amiga content changes nothing on the form", () => {
    const fills = intakeFills([
      item("Turrican.lha", { kind: "for-an-amiga-volume", what: "archive" }),
      item("elite.d64", { kind: "no-place-on-a-card", what: "commodore-8bit" }),
    ]);
    expect(fills).toEqual({});
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

describe("PFS3 on the card builder (ART-084's own expiry condition)", () => {
  it("offers PFS3, and does not offer SFS", () => {
    const values = cardFsChoices("E:\\pfs3aio").map((choice) => choice.value);
    expect(values).toContain("pfs3directscsi");
    expect(values).toContain("pfs3standard");
    expect(values).toContain("ffsstandard");
    // The owner's own decision on 2026-08-22: the Emu68 Imager installs PFS3
    // and not SFS, and nothing is known yet about the candidate crate's
    // agreement with the real handler. `SFS — not supported yet` stays.
    expect(values).not.toContain("sfs0");
  });

  /// **Shown, and not selectable.** ART ships no `pfs3aio` and never will, so
  /// without one PFS3 cannot work — but hiding it would leave "why can I not
  /// have PFS3" unanswerable, which is worse than saying so.
  it("cannot pick PFS3 with no driver, and says which file is wanted", () => {
    const withNothing = cardFsChoices(null);
    const pfs3 = withNothing.find((choice) => choice.value === "pfs3directscsi");
    expect(pfs3?.blocked).toEqual({
      key: "cardBuilder.fs.needsDriver",
      params: { file: "pfs3aio" },
    });
    // FFS is in Kickstart and needs nothing, so it is never blocked.
    expect(withNothing.find((c) => c.value === "ffsstandard")?.blocked).toBeNull();
  });

  it("a driver unblocks it", () => {
    const withDriver = cardFsChoices("E:\\amiga\\pfs3aio");
    expect(withDriver.every((choice) => choice.blocked === null)).toBe(true);
  });

  /// The image ART-084 is actually about: a partition naming a filesystem
  /// nothing on the card carries. Refused **before** the build rather than
  /// produced and then explained.
  it("refuses to build a PFS3 card with no driver, and names the file", () => {
    const request = {
      ...{ ...REQUEST, partitions: [{ ...defaultPartition(), fs_type: "pfs3directscsi" as const }] },
      file_systems: [],
    };
    expect(buildBlocker(request, PLAN)).toEqual({
      key: "cardBuilder.blocked.noDriver",
      params: { file: "pfs3aio" },
    });
  });

  it("builds once the driver is there", () => {
    const request = {
      ...{ ...REQUEST, partitions: [{ ...defaultPartition(), fs_type: "pfs3directscsi" as const }] },
      file_systems: [{ path: "E:\\pfs3aio", dos_type: "PDS3" }],
    };
    expect(buildBlocker(request, PLAN)).toBeNull();
  });

  /// An FFS card must not have become harder to build. Kickstart carries FFS,
  /// so nothing is required and nothing is embedded.
  it("an FFS card still needs no driver at all", () => {
    const request = { ...{ ...REQUEST, partitions: [{ ...defaultPartition(), fs_type: "ffsstandard" as const }] }, file_systems: [] };
    expect(buildBlocker(request, PLAN)).toBeNull();
    expect(fileSystemInputsFor("ffsstandard", "E:\\pfs3aio")).toEqual([]);
  });
});

describe("the card's two partitions", () => {
  /// Both real PiStorm cards carry `SDH0` and `SDH1`; ART offered one, which
  /// is the one shape neither working card has.
  it("proposes SDH0 for the system and SDH1 for the rest", () => {
    expect(defaultPartition().drive_name).toBe("SDH0");
    expect(defaultSecondPartition("ffsstandard").drive_name).toBe("SDH1");
  });

  /// `0` is the core's "whatever is left". The screen must not work the
  /// remainder out itself — that is `bytes_per_cyl` rounding, and a second
  /// copy of it is how the two start disagreeing.
  it("asks the core for the rest rather than computing it", () => {
    expect(defaultSecondPartition("ffsstandard").size_mb).toBe(0);
  });

  /// Only one of them boots, and it is the system one. Two bootable
  /// partitions on a fresh card is a choice nobody made.
  it("only the system partition is bootable", () => {
    expect(defaultPartition().bootable).toBe(true);
    expect(defaultSecondPartition("ffsstandard").bootable).toBe(false);
  });

  /// Measured: both cards say priority 1 for the bootable one, and so does
  /// the Imager's table. With one bootable partition it changes nothing —
  /// it is matching what the cards that boot actually carry.
  it("the bootable partition uses the priority the real cards use", () => {
    expect(defaultPartition().boot_priority).toBe(1);
  });

  /// The second partition is the same filesystem as the first, because it is
  /// the same card and the same driver — a PFS3 system partition beside an
  /// FFS work one would need two drivers to mount one disk.
  it("the second partition follows the filesystem the first uses", () => {
    expect(defaultSecondPartition("pfs3directscsi").fs_type).toBe("pfs3directscsi");
    expect(defaultSecondPartition("ffsdircache").fs_type).toBe("ffsdircache");
  });
});

describe("secondSystem", () => {
  const GIB = 1024 ** 3;
  const CARD = 64 * GIB;
  const BOOT = 1 * GIB;

  it("gives the second disk the size asked for and the first the rest", () => {
    const split = secondSystem(CARD, BOOT, 8 * GIB, "ffsstandard", 512);
    expect(split.ok).toBe(true);
    if (!split.ok) return;

    // The FIRST disk's size is what goes in the request, because the planner
    // allows "whatever is left" only for the last disk.
    expect(split.firstDiskBytes).toBe(CARD - BOOT - 8 * GIB);
    expect(split.extraDisks).toHaveLength(1);
    expect(split.extraDisks[0].size_bytes).toBe(0);
  });

  it("the two disks and the boot partition account for the whole card", () => {
    const split = secondSystem(CARD, BOOT, 8 * GIB, "ffsstandard", 512);
    if (!split.ok) throw new Error("expected a split");
    // The second takes the rest, so what is left after boot and the first is
    // exactly what it gets - no gap, and nothing counted twice.
    expect(BOOT + split.firstDiskBytes + 8 * GIB).toBe(CARD);
  });

  it("the second system boots below the first, so there is no tie", () => {
    const split = secondSystem(CARD, BOOT, 8 * GIB, "ffsstandard", 512);
    if (!split.ok) throw new Error("expected a split");
    const [partition] = split.extraDisks[0].partitions;

    expect(partition.bootable).toBe(true);
    // The number, not the constant: comparing a constant with itself is a test
    // that passes whatever the value becomes.
    expect(partition.boot_priority).toBe(0);
    expect(defaultPartition().boot_priority).toBe(1);
    expect(partition.boot_priority).toBeLessThan(defaultPartition().boot_priority);
  });

  it("its one partition takes the whole disk and does not reuse a drive name", () => {
    const split = secondSystem(CARD, BOOT, 8 * GIB, "pfs3directscsi", 512);
    if (!split.ok) throw new Error("expected a split");
    const [partition] = split.extraDisks[0].partitions;

    expect(partition.size_mb).toBe(0);
    expect(partition.fs_type).toBe("pfs3directscsi");
    // A name the first disk already uses would be two volumes answering to
    // one name on the same card.
    expect(partition.drive_name).not.toBe(defaultPartition().drive_name);
    expect(partition.drive_name).not.toBe(defaultSecondPartition("ffsstandard").drive_name);
  });

  it("refuses a second system with no size", () => {
    const split = secondSystem(CARD, BOOT, 0, "ffsstandard", 512);
    expect(split.ok).toBe(false);
    if (split.ok) return;
    expect(split.why.key).toBe("cardBuilder.second.blocked.noSize");
  });

  it("refuses one that leaves the first system no room for its own partition", () => {
    // 63 GB of a 64 GB card, with 1 GB already gone to the boot partition:
    // the first Amiga disk would be 0 bytes.
    const split = secondSystem(CARD, BOOT, 63 * GIB, "ffsstandard", 512);
    expect(split.ok).toBe(false);
    if (split.ok) return;
    expect(split.why.key).toBe("cardBuilder.second.blocked.tooLarge");
    expect(split.why.params).toEqual({ mb: 512 });
  });

  it("the floor follows the first system's own partition size", () => {
    // A card with exactly enough for a 512 MB first system is allowed; the
    // same card with a 4096 MB first system is not. The floor is the size the
    // user asked for, not a constant.
    const card = 10 * GIB;
    const second = card - BOOT - 512 * 1024 * 1024;
    expect(secondSystem(card, BOOT, second, "ffsstandard", 512).ok).toBe(true);
    expect(secondSystem(card, BOOT, second, "ffsstandard", 4096).ok).toBe(false);
  });
});
