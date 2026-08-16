// Formatting a card's Amiga volumes from Windows, and filling them
// (SD-2 · G3, route E). Mirrors src-tauri/src/commands/preload.rs and
// src-tauri/src/core/preload/mod.rs.
//
// **The tool is not ART.** `hst-imager` does the formatting — a proven MIT
// implementation both existing PiStorm imagers stand on — and ART does not
// ship it, so where it lives is a setting (`hstImagerPath`) beside the WinUAE
// path rather than an assumption. `preload_probe` asks it what it is and says
// when it is not the version ART's command set was written against; that is a
// remark, never a refusal.
//
// **What ART cannot check afterwards.** There is no PFS3 reader here, so once
// a volume is formatted and filled ART can confirm the partition table, the
// embedded driver and the geometry — and not one file inside the volume. The
// result panel says so; a tick meaning "ART did not look" is the claim §89
// forbids.

import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

import type { CardReport } from "@/lib/card";
import type { Phrase } from "@/lib/phrase";

/** What the formatter reports itself to be. */
export interface ToolVersion {
  raw: string;
}

export interface FormatterReport {
  version: ToolVersion;
  /** False when it is not the version ART was written against. Not a refusal. */
  is_tested_version: boolean;
  tested_version: string;
}

/** How much a copy moved. */
export interface CopySummary {
  files: number;
  directories: number;
  bytes: number;
}

/**
 * One partition to prepare.
 *
 * **Two numbers, and both are needed.** A card is a list of Amiga disks and
 * each carries its own RDB, so "partition 1" means nothing until you say which
 * disk (ART-095). Both count from one, the way the disk itself numbers them.
 */
export interface PreloadPartition {
  area: number;
  index: number;
  volume_name: string;
  /** A folder on the PC whose tree goes in. Null formats and stops. */
  content: string | null;
}

export interface PreloadRequest {
  image: string;
  /** A filesystem driver to embed first, when the card carries none for the
   *  DosType its partitions name. */
  driver: string | null;
  partitions: PreloadPartition[];
}

/** One thing the run would do, in order. */
export type PreloadStep =
  | {
      step: "import-filesystem";
      /** Which MBR slot holds the Amiga disk; null for a plain image whose RDB
       *  is at offset zero. */
      slot: number | null;
      driver: string;
      dostype: string;
      name: string;
    }
  | {
      step: "format-partition";
      slot: number | null;
      index: number;
      drive_name: string;
      volume_name: string;
    }
  | { step: "copy-in"; slot: number | null; drive_name: string; source: string };

export interface PreloadPlan {
  image: string;
  steps: PreloadStep[];
}

export interface PreloadOutcome {
  formatted: string[];
  copied: CopySummary;
  tool: ToolVersion | null;
}

export const PRELOAD_EVENT = "preload-result";

export interface PreloadResult {
  job_id: number;
  image: string;
  outcome: PreloadOutcome;
}

// ---------------------------------------------------------------------------
// The commands
// ---------------------------------------------------------------------------

/** Ask the configured tool what it is. Runs it with `--version` and nothing else. */
export async function preloadProbe(toolPath: string): Promise<FormatterReport> {
  return invoke<FormatterReport>("preload_probe", { toolPath });
}

/** What a preload would do. Writes nothing (§92's PREVIEW). */
export async function preloadPlan(
  request: PreloadRequest,
  toolPath: string
): Promise<PreloadPlan> {
  return invoke<PreloadPlan>("preload_plan", {
    command: { ...request, tool_path: toolPath },
  });
}

/**
 * Format the partitions and copy the content in. Returns a job id (§54).
 *
 * The engine recomputes the plan rather than taking the one the screen showed,
 * so a screen that previewed one thing cannot run another.
 */
export async function preloadRun(
  request: PreloadRequest,
  toolPath: string
): Promise<number> {
  return invoke<number>("preload_run", {
    command: { ...request, tool_path: toolPath },
  });
}

/** Subscribe to finished preloads. A cancelled or failed job never sends one —
 *  the job bar is where those are seen. */
export async function onPreloadResult(
  handler: (result: PreloadResult) => void
): Promise<UnlistenFn> {
  return listen<PreloadResult>(PRELOAD_EVENT, (event) => handler(event.payload));
}

// ---------------------------------------------------------------------------
// What the screen holds, and the rules over it
// ---------------------------------------------------------------------------

/**
 * One row of the screen: a partition on the card, and what the user has said
 * about it.
 *
 * `driveName` is carried alongside the two numbers so a refusal can name the
 * drive rather than "partition 1 of disk 1", which is not what the user is
 * looking at.
 */
export interface PartitionPick {
  area: number;
  index: number;
  driveName: string;
  chosen: boolean;
  volumeName: string;
  content: string | null;
}

/**
 * Every partition on the card, as a row with nothing chosen.
 *
 * The volume name starts as the drive's own name — a fact read off the card
 * rather than a `Work` this screen would be inventing. Formatting is
 * `Destructive`, so nothing is chosen: the user picks what gets erased.
 */
export function picksFor(report: CardReport): PartitionPick[] {
  return report.card.areas.flatMap((area, areaIndex) =>
    area.rdb.partitions.map((partition, partitionIndex) => ({
      area: areaIndex + 1,
      index: partitionIndex + 1,
      driveName: partition.drive_name,
      chosen: false,
      volumeName: partition.drive_name,
      content: null,
    }))
  );
}

/** The request the commands take. Only the chosen rows reach it. */
export function toRequest(
  image: string,
  driver: string | null,
  picks: PartitionPick[]
): PreloadRequest {
  return {
    image,
    driver: driver?.trim() ? driver.trim() : null,
    partitions: picks
      .filter((pick) => pick.chosen)
      .map((pick) => ({
        area: pick.area,
        index: pick.index,
        volume_name: pick.volumeName.trim(),
        content: pick.content,
      })),
  };
}

/** AmigaDOS names stop at thirty characters — `core/volume/write/dir.rs`'s
 *  `MAX_NAME_LEN`, restated here so a refusal can say the number. */
export const MAX_VOLUME_NAME = 30;

/**
 * Why the preload cannot run yet, or null when it can.
 *
 * A reason rather than a boolean: a disabled button that does not say why is
 * the defect ART-100 was. The volume-name rules are the two
 * `core/volume/write/dir.rs::check_name` already holds — a name AmigaDOS
 * cannot store is not a name, and finding that out after the format has begun
 * is finding it out too late.
 */
export function preloadBlocker(input: {
  image: string | null;
  toolPath: string | null;
  picks: PartitionPick[];
  plan: PreloadPlan | null;
}): Phrase | null {
  if (!input.image?.trim()) return { key: "preload.blocked.noCard" };
  if (!input.toolPath?.trim()) return { key: "preload.blocked.noTool" };

  const chosen = input.picks.filter((pick) => pick.chosen);
  if (chosen.length === 0) return { key: "preload.blocked.nothingChosen" };

  for (const pick of chosen) {
    const name = pick.volumeName.trim();
    if (!name) return { key: "preload.blocked.blankName", params: { drive: pick.driveName } };
    if (name.includes(":") || name.includes("/")) {
      return { key: "preload.blocked.badName", params: { drive: pick.driveName } };
    }
    // Characters, not bytes: thirty accented characters are thirty characters.
    if ([...name].length > MAX_VOLUME_NAME) {
      return {
        key: "preload.blocked.longName",
        params: { drive: pick.driveName, max: MAX_VOLUME_NAME },
      };
    }
  }

  if (!input.plan) return { key: "preload.blocked.notPlanned" };
  return null;
}

/** How many partitions this plan would erase. */
export function formatCount(plan: PreloadPlan): number {
  return plan.steps.filter((step) => step.step === "format-partition").length;
}

/** The sentence for one planned step, for the component to render. */
export function stepPhrase(step: PreloadStep): Phrase {
  switch (step.step) {
    case "import-filesystem":
      return {
        key: "preload.plan.step.import",
        params: { name: step.name, dostype: step.dostype },
      };
    case "format-partition":
      return {
        key: "preload.plan.step.format",
        params: { drive: step.drive_name, volume: step.volume_name },
      };
    case "copy-in":
      return {
        key: "preload.plan.step.copy",
        params: { drive: step.drive_name, source: step.source },
      };
  }
}
