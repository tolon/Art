// A PiStorm SD card, read as what it is (§ ART-095, ART-097).
// Mirrors src-tauri/src/commands/card.rs and src-tauri/src/core/card.rs.
//
// **A card is a list of disks, not a disk.** It is an MBR with a FAT32 boot
// partition and one to three `0x76` areas, and the m68k side sees each of
// those as a *separate hard drive* — so each carries its own RDB, at a byte
// offset inside the card rather than at zero. Learnt from two real cards, not
// from a document; the layout is written up in docs/sd2-card-layout.md.
//
// A plain HDF comes back through the same call as one area at offset zero,
// which is what lets the Hard Disk studio ask once and branch on `mbr` rather
// than guessing at the kind of file it has.

import { invoke } from "@tauri-apps/api/core";

import type { ParsedFileSystem, ParsedPartition } from "@/lib/hdf";

/** What ART recognises a primary partition as. Anything else carries its byte. */
export type PartitionKind =
  | { kind: "fat32" }
  | { kind: "amiga-rdb" }
  | { kind: "other"; code: number };

export interface MbrPartition {
  /** Which of the four slots — a card's own documentation says "partition 1",
   *  and a listing that renumbers them disagrees with the user's notes. */
  index: number;
  kind: PartitionKind;
  type_byte: number;
  bootable: boolean;
  start_lba: number;
  sector_count: number;
}

export interface Mbr {
  partitions: MbrPartition[];
}

/** One Amiga disk on the card. */
export interface AmigaArea {
  /** Where this disk begins in the file. Every block number in its RDB is
   *  relative to here. */
  offset_bytes: number;
  /** What the table says it is; 0 for a plain HDF, where the answer is
   *  "the rest of the file". */
  length_bytes: number;
  rdb: {
    partitions: ParsedPartition[];
    file_systems: ParsedFileSystem[];
    checksum_valid: boolean;
  };
}

export interface CardImage {
  path: string;
  total_bytes: number;
  /** Null for a plain HDF — not an error, just a different kind of file. */
  mbr: Mbr | null;
  areas: AmigaArea[];
}

/** A partition naming a filesystem no area on the card carries. */
export interface UnmountablePartition {
  area: number;
  drive_name: string;
  dostype_str: string;
}

export interface CardReport {
  card: CardImage;
  /**
   * Every driver on the **whole card**, deduplicated.
   *
   * The union, and asked in Rust rather than here: MultibootOS 2.2 carries
   * PFS3 in its first RDB and not its second, so asking one area in isolation
   * reports fifteen working partitions as broken (ART-097).
   */
  file_systems: ParsedFileSystem[];
  unmountable: UnmountablePartition[];
}

/** Read a card, or a plain HDF. Header-only — never the whole card. */
export async function cardOpen(path: string): Promise<CardReport> {
  return invoke<CardReport>("card_open", { path });
}

/** Whether this file is a card rather than a plain hard disk image. */
export function isCard(report: CardReport): boolean {
  return report.card.mbr !== null;
}

/** The FAT32 partition the Pi firmware boots from, if the card has one. */
export function bootPartition(report: CardReport): MbrPartition | null {
  return report.card.mbr?.partitions.find((p) => p.kind.kind === "fat32") ?? null;
}

/** How many partitions the whole card holds, across every area. */
export function partitionCount(report: CardReport): number {
  return report.card.areas.reduce((total, area) => total + area.rdb.partitions.length, 0);
}
