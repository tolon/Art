// Typed wrappers for the game index (SD-2 · G10).
//
// The mirror of `core::gameindex::record`. Every optional field is a `Fact`
// rather than a bare value, because the same field arrives from two tiers and
// they disagree: a slave's `ReqAGA` bit against a filename containing the
// letters "AGA" is a real case in this collection, and which one won has to
// stay visible on screen.

import { invoke } from "@tauri-apps/api/core";

import type { Phrase } from "./phrase";

/** Where a fact came from. Mirrors `core::gameindex::record::Provenance`. */
export type Provenance =
  | "rp9-manifest"
  | "whdload-slave"
  | "tosec-name"
  | "drawer-name";

/**
 * Every provenance the core can emit, in the order the record documents them:
 * the two that **state** first, the two that **suggest** after.
 */
export const ALL_PROVENANCES: readonly Provenance[] = [
  "rp9-manifest",
  "whdload-slave",
  "tosec-name",
  "drawer-name",
] as const;

/**
 * Whether this source declared the value rather than implying it.
 *
 * The same split as `Provenance::is_stated` in Rust. Kept in both places
 * deliberately: the screen needs the answer without a round trip, and the two
 * lists are four words long each.
 */
export function isStated(from: Provenance): boolean {
  return from === "rp9-manifest" || from === "whdload-slave";
}

/** A value and the source that gave it. */
export interface Fact<T> {
  value: T;
  from: Provenance;
}

export type TitleKind =
  | "game"
  | "demo"
  | "system"
  | "gallery"
  | "video"
  | { other: string };

export type ChipsetRequirement = "ocsecs" | "aga";

export interface KickstartNeed {
  image: string | null;
  size: number | null;
  crc16: number | null;
  rom_version: number | null;
}

export type Media =
  | { kind: "floppies"; ordered: string[] }
  | { kind: "hardfile"; file: string }
  | { kind: "whdload-drawer"; slave: string };

export interface SourceRef {
  name: string;
  sha256: string;
  bytes: number;
}

export interface GameRecord {
  schema: number;
  id: string;
  title: Fact<string>;
  kind: Fact<TitleKind> | null;
  year: Fact<number> | null;
  publisher: Fact<string> | null;
  genre: Fact<string> | null;
  rating: Fact<number> | null;
  chipset: Fact<ChipsetRequirement> | null;
  kickstart: Fact<KickstartNeed> | null;
  media: Media;
  preview: string | null;
  source: SourceRef;
}

/** One catalogued title and where its file sits. */
export interface CatalogueEntry {
  path: string;
  record: GameRecord;
}

/** The name of a source, for the marker beside a guessed value. */
export function provenancePhrase(from: Provenance): Phrase {
  return { key: `gameindex.provenance.${from}` };
}

/**
 * Start indexing a folder. Returns a job id straight away — indexing reads
 * inside every hardfile and hashes every file (§54/§55), and the catalogue
 * arrives in a `gameindex-result` event.
 */
export async function gameindexScan(dirPath: string): Promise<number> {
  return invoke<number>("gameindex_scan", { dirPath });
}

/** Payload of the `gameindex-result` event. */
export interface IndexResult {
  job_id: number;
  dir_path: string;
  entries: CatalogueEntry[];
}

export const INDEX_RESULT_EVENT = "gameindex-result";

/** Subscribe to finished indexes. Returns an unlisten function. */
export async function onIndexResult(
  handler: (result: IndexResult) => void
): Promise<() => void> {
  const { listen } = await import("@tauri-apps/api/event");
  return listen<IndexResult>(INDEX_RESULT_EVENT, (e) => handler(e.payload));
}

/** What kind of media a record describes, for a filter or a badge. */
export function mediaKind(media: Media): "floppies" | "hardfile" | "whdload" {
  switch (media.kind) {
    case "floppies":
      return "floppies";
    case "hardfile":
      return "hardfile";
    case "whdload-drawer":
      return "whdload";
  }
}
