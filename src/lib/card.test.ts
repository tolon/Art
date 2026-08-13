// The pure helpers in `@/lib/card`. The reading itself is Rust's
// (`core/card.rs`, tested there against fixtures and against two real cards);
// these are the three questions the screen asks about what came back.
//
// The shapes below are the real ones, from `read_real_card_when_asked` run on
// the user's own cards on 2026-08-13:
//
//   MultibootOS 2.2  128 GB  2 Amiga disks, 17 partitions, 2 drivers
//   CaffeineOS 9317   64 GB  1 Amiga disk,   2 partitions, 1 driver
//
// with the FAT32 boot partition at the front of both and the first Amiga area
// 1 178 599 424 bytes in — which is the number that made ART-095 a bug rather
// than a preference.

import { describe, expect, it } from "vitest";

import {
  bootPartition,
  isCard,
  partitionCount,
  type CardReport,
  type MbrPartition,
} from "./card";
import type { ParsedPartition } from "./hdf";

function slot(index: number, kind: MbrPartition["kind"], sectors: number): MbrPartition {
  return {
    index,
    kind,
    type_byte: kind.kind === "fat32" ? 0x0c : kind.kind === "amiga-rdb" ? 0x76 : 0,
    bootable: index === 0,
    start_lba: 2048 + index * 1000,
    sector_count: sectors,
  };
}

function part(name: string): ParsedPartition {
  return {
    drive_name: name,
    dostype: 0x50445303,
    dostype_str: "PDS\\3",
    fs_type: "pfs3standard",
    low_cyl: 2,
    high_cyl: 201,
    cylinder_count: 200,
    size_bytes: 1024,
    bootable: true,
    boot_priority: 0,
    num_buffers: 600,
    block_location: 1,
    next_part_block: 0,
    checksum_valid: true,
  };
}

function area(offset: number, names: string[]) {
  return {
    offset_bytes: offset,
    length_bytes: 1_000_000,
    rdb: {
      partitions: names.map(part),
      file_systems: [],
      checksum_valid: true,
    },
  };
}

function report(overrides: Partial<CardReport["card"]> = {}): CardReport {
  return {
    card: {
      path: "E:\\card.img",
      total_bytes: 127_999_672_320,
      mbr: {
        partitions: [
          slot(0, { kind: "fat32" }, 409_600),
          slot(1, { kind: "amiga-rdb" }, 96_000_000),
          slot(2, { kind: "amiga-rdb" }, 150_000_000),
        ],
      },
      areas: [area(1_178_599_424, ["SDH0", "SDH1"]), area(50_570_723_328, ["ADH0", "ADH1", "AGS0"])],
      ...overrides,
    },
    file_systems: [],
    unmountable: [],
  } as CardReport;
}

describe("telling a card from a plain hard disk image", () => {
  it("is a card when there is a partition table", () => {
    expect(isCard(report())).toBe(true);
  });

  it("is not a card when there is none — which is what an HDF is", () => {
    // `read_card` answers for both, and an HDF comes back as one area at
    // offset zero with no MBR. The screen branches on this, not on the
    // extension: a card is `.img`, and so is plenty that is not one.
    const hdf = report({ mbr: null, areas: [area(0, ["DH0"])] });
    expect(isCard(hdf)).toBe(false);
  });
});

describe("the FAT32 partition the Pi boots from", () => {
  it("is found by type, not by position", () => {
    const found = bootPartition(report());
    expect(found?.kind.kind).toBe("fat32");
    expect(found?.sector_count).toBe(409_600);
  });

  it("is null on a plain HDF", () => {
    expect(bootPartition(report({ mbr: null }))).toBeNull();
  });

  it("is null on a card that has none — the Pi would have nothing to boot", () => {
    const noBoot = report({
      mbr: { partitions: [slot(0, { kind: "amiga-rdb" }, 96_000_000)] },
    });
    expect(bootPartition(noBoot)).toBeNull();
  });
});

describe("counting partitions", () => {
  it("counts across every Amiga disk, not just the first", () => {
    // The whole ART-097 lesson in one number: a card is a *list* of disks, and
    // an answer that stops at the first one is wrong on both real cards.
    expect(partitionCount(report())).toBe(5);
  });

  it("counts nothing when there is nothing", () => {
    expect(partitionCount(report({ areas: [] }))).toBe(0);
  });
});
