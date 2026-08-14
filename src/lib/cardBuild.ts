// Building a PiStorm card image (SD-1 · G2).
// Mirrors src-tauri/src/commands/card.rs and src-tauri/src/core/card/.
//
// **ART builds the image; it never touches the card.** The user flashes the
// `.img` with whichever imager they already have — that decision (2026-08-12)
// deleted raw `\\.\PhysicalDriveN` access and with it ART's largest safety
// surface, and §56 keeps the raw-device guard in the spec as the reason.
//
// What comes out is a card an Amiga boots into a partition table it can see,
// with volumes it will offer to format. Putting a system on them is SD-2's
// work, and `CardBuildWarning` says so on the screen rather than letting it be
// discovered on the Amiga (§10, §89).

import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

import type { CardReport, MbrPartition } from "@/lib/card";
import type { AmigaHardDiskFs, PartitionSpec } from "@/lib/hdf";
import type { Phrase } from "@/lib/phrase";
import type {
  Emu68Line,
  Emu68Options,
  FirmwareConfig,
  PistormHardware,
} from "@/lib/pistorm";

/** A card's shape, before any of it exists. */
export interface CardLayout {
  total_sectors: number;
  /** The FAT32 partition the Pi firmware boots from. Always first. */
  boot: MbrPartition;
  /** One to three Amiga disks, each of which gets its own RDB. */
  areas: MbrPartition[];
}

/** What a screen asks for when it wants a card. */
export interface CardBuildRequest {
  /** The user's own Emu68 release archive. ART never downloads it (§2). */
  archive: string;
  /** Their Kickstart. Null builds a card that will not boot — allowed, and
   *  warned about rather than substituted for. */
  kickstart: string | null;
  /** Where the image goes. SAFE_CREATE: an existing file is refused. */
  dest: string;
  total_bytes: number;
  /** 0 for the 1.10 GiB measured off both real cards. */
  boot_bytes: number;
  label: string;
  hardware: PistormHardware;
  /** Which release line the archive came from. It decides what the archive's
   *  *name* means, and ART cannot tell from the bytes (ART-091). */
  line: Emu68Line;
  firmware: FirmwareConfig;
  options: Emu68Options;
  /** When the build happened, for the manifest (G7). The screen's own clock —
   *  `core` has none, and the caller already knows the user's locale. Absent
   *  when planning: a plan writes nothing and has no date. */
  built_at?: string;
  /** The partitions of the card's one Amiga disk. */
  partitions: PartitionSpec[];
}

/** A file on its way to the boot partition — its name and its size. */
export interface PlacedFile {
  name: string;
  bytes: number;
}

/** Something true about this card the user would otherwise find out from an
 *  Amiga that does not come up. Typed, so the words are the interface's and in
 *  the user's language (ART-060). */
export type CardBuildWarning =
  | { kind: "no-kickstart" }
  | { kind: "rom-unrecognised" }
  | { kind: "rom-wrong-machine"; rom: string }
  | { kind: "volumes-unformatted" };

/** A ROM as ART identifies it. The fields this screen uses. */
export interface PlannedRom {
  name: string;
  version: string;
  revision: string;
  size_bytes: number;
  sha256: string;
  compatible_models: string[];
}

/** What building the request would produce. Writes nothing (§92's PREVIEW). */
export interface CardBuildPlan {
  layout: CardLayout;
  boot_files: PlacedFile[];
  /** The file `config.txt` points the firmware at — the release's own answer,
   *  not ART's (ART-103). */
  kernel_file: string;
  kickstart_file: string | null;
  rom: PlannedRom | null;
  warnings: CardBuildWarning[];
  /** SAFE_CREATE would refuse. Here so the screen can say it before the button
   *  rather than after it. */
  dest_exists: boolean;
}

/** What a finished build produced. */
export interface CardBuildResult {
  job_id: number;
  dest: string;
  layout: CardLayout;
  /** Where the build manifest was written (G7): beside the image. */
  manifest_path: string;
  /** The card read back out of the file that was just written, through the
   *  same reader the Hard Disk studio uses for somebody else's card. */
  verified: CardReport;
}

/** The event a finished build arrives on. */
export const CARD_BUILD_EVENT = "card-build-result";

// ---------------------------------------------------------------------------
// The build manifest (SD-1 · G7)
// ---------------------------------------------------------------------------

/** Something on the card that does not match its manifest. */
export type ManifestFinding =
  | { kind: "schema-too-new"; found: number; understood: number }
  | { kind: "size-changed"; expected: number; found: number }
  | { kind: "partition-table-changed" }
  | { kind: "area-count-changed"; expected: number; found: number }
  | { kind: "area-moved"; area: number; expected: number; found: number }
  | { kind: "area-resized"; area: number; expected: number; found: number }
  | { kind: "rdb-changed"; area: number }
  | { kind: "partition-count-changed"; area: number; expected: number; found: number }
  | { kind: "partition-changed"; area: number; name: string };

// ---------------------------------------------------------------------------
// Is this image a card that will boot? (SD-1 · G8)
// ---------------------------------------------------------------------------

/** Which file on the boot partition a check is about. */
export type BootFileRole = "config" | "cmdline" | "kernel" | "kickstart";

/** One thing the health check looked at, or could not. */
export type HealthCheck =
  | { kind: "boot-partition-first" }
  | { kind: "boot-partition-aligned"; lba: number }
  | { kind: "amiga-area-count"; count: number }
  | { kind: "areas-aligned" }
  | { kind: "nothing-overlaps" }
  | { kind: "everything-inside-the-image" }
  | { kind: "area-has-rdb"; area: number }
  | { kind: "area-rdb-checksum"; area: number }
  | { kind: "every-partition-can-mount"; unmountable: number }
  | { kind: "manifest-agrees"; findings: ManifestFinding[] }
  | { kind: "boot-file"; role: BootFileRole; name: string };

/** `not-checked` is never a pass. It is what the report exists to say. */
export type CheckState = "pass" | "fail" | "not-checked";

export interface HealthItem {
  check: HealthCheck;
  state: CheckState;
}

/** Something only the person at the machine can do or confirm. */
export type ManualStep =
  | { kind: "flash-the-card" }
  | { kind: "hdmi-before-power" }
  | { kind: "pi-model-matches"; pi: string }
  | { kind: "volumes-need-formatting"; count: number };

export interface HealthReport {
  items: HealthItem[];
  by_hand: ManualStep[];
}

/**
 * Check a built image — the last gate before the file is handed over.
 *
 * The manifest comparison is a section of this, not a separate call: somebody
 * asking "is this card right" is asking one question.
 */
export async function cardCheckImage(
  image: string,
  manifest?: string,
  pi?: string
): Promise<HealthReport> {
  return invoke<HealthReport>("card_check_image", { image, manifest, pi });
}

/** How many checks came back wrong. `not-checked` is not among them. */
export function healthFailures(report: HealthReport): number {
  return report.items.filter((item) => item.state === "fail").length;
}

/** How many questions ART could not answer at all. */
export function healthUnanswered(report: HealthReport): number {
  return report.items.filter((item) => item.state === "not-checked").length;
}

/**
 * The one-line answer — and it never says "fine" without saying how much went
 * unanswered. A tick that means "ART did not look" reading like one that means
 * "ART looked and it is right" is the claim §89 forbids.
 */
export function healthVerdict(report: HealthReport): Phrase {
  const failed = healthFailures(report);
  if (failed > 0) return { key: "cardBuilder.health.failed", params: { count: failed } };
  const unanswered = healthUnanswered(report);
  if (unanswered > 0) {
    return { key: "cardBuilder.health.passedWithGaps", params: { unanswered } };
  }
  return { key: "cardBuilder.health.passed" };
}

/** The sentence for one check. Areas and disks are numbered from one. */
export function healthCheckPhrase(check: HealthCheck): Phrase {
  switch (check.kind) {
    case "boot-partition-first":
      return { key: "cardBuilder.health.check.bootFirst" };
    case "boot-partition-aligned":
      return { key: "cardBuilder.health.check.bootAligned", params: { lba: check.lba } };
    case "amiga-area-count":
      return { key: "cardBuilder.health.check.areaCount", params: { count: check.count } };
    case "areas-aligned":
      return { key: "cardBuilder.health.check.areasAligned" };
    case "nothing-overlaps":
      return { key: "cardBuilder.health.check.noOverlap" };
    case "everything-inside-the-image":
      return { key: "cardBuilder.health.check.insideImage" };
    case "area-has-rdb":
      return { key: "cardBuilder.health.check.hasRdb", params: { n: check.area + 1 } };
    case "area-rdb-checksum":
      return { key: "cardBuilder.health.check.rdbChecksum", params: { n: check.area + 1 } };
    case "every-partition-can-mount":
      return {
        key: "cardBuilder.health.check.canMount",
        params: { unmountable: check.unmountable },
      };
    case "manifest-agrees":
      return { key: "cardBuilder.health.check.manifest" };
    case "boot-file":
      return {
        key: `cardBuilder.health.check.bootFile.${check.role}`,
        params: { name: check.name },
      };
  }
}

/** The sentence for a step only the user can take. */
export function manualStepPhrase(step: ManualStep): Phrase {
  switch (step.kind) {
    case "flash-the-card":
      return { key: "cardBuilder.health.byHand.flash" };
    case "hdmi-before-power":
      return { key: "cardBuilder.health.byHand.hdmi" };
    case "pi-model-matches":
      return { key: "cardBuilder.health.byHand.pi", params: { pi: step.pi } };
    case "volumes-need-formatting":
      return { key: "cardBuilder.health.byHand.format", params: { count: step.count } };
  }
}

/** The sentence for one finding. Areas are numbered from one on screen. */
export function findingPhrase(finding: ManifestFinding): Phrase {
  switch (finding.kind) {
    case "schema-too-new":
      return {
        key: "cardBuilder.manifest.finding.schemaTooNew",
        params: { found: finding.found, understood: finding.understood },
      };
    case "size-changed":
      return {
        key: "cardBuilder.manifest.finding.sizeChanged",
        params: { expected: finding.expected, found: finding.found },
      };
    case "partition-table-changed":
      return { key: "cardBuilder.manifest.finding.partitionTableChanged" };
    case "area-count-changed":
      return {
        key: "cardBuilder.manifest.finding.areaCountChanged",
        params: { expected: finding.expected, found: finding.found },
      };
    case "area-moved":
      return {
        key: "cardBuilder.manifest.finding.areaMoved",
        params: { n: finding.area + 1, expected: finding.expected, found: finding.found },
      };
    case "area-resized":
      return {
        key: "cardBuilder.manifest.finding.areaResized",
        params: { n: finding.area + 1, expected: finding.expected, found: finding.found },
      };
    case "rdb-changed":
      return {
        key: "cardBuilder.manifest.finding.rdbChanged",
        params: { n: finding.area + 1 },
      };
    case "partition-count-changed":
      return {
        key: "cardBuilder.manifest.finding.partitionCountChanged",
        params: { n: finding.area + 1, expected: finding.expected, found: finding.found },
      };
    case "partition-changed":
      return {
        key: "cardBuilder.manifest.finding.partitionChanged",
        params: { n: finding.area + 1, name: finding.name },
      };
  }
}

/** What building this card would do. Writes nothing. */
export async function cardPlanBuild(request: CardBuildRequest): Promise<CardBuildPlan> {
  return invoke<CardBuildPlan>("card_plan_build", { request });
}

/** Build the card. Returns a job id; the result arrives on `CARD_BUILD_EVENT`. */
export async function cardBuild(request: CardBuildRequest): Promise<number> {
  return invoke<number>("card_build", { request });
}

/**
 * Subscribe to finished builds. Returns an unlisten function.
 *
 * A cancelled or failed job never sends one — the job bar is where those are
 * seen, the same shape every other job-backed result in ART uses.
 */
export async function onCardBuildResult(
  handler: (result: CardBuildResult) => void
): Promise<UnlistenFn> {
  return listen<CardBuildResult>(CARD_BUILD_EVENT, (event) => handler(event.payload));
}

/**
 * The filesystems this builder can give a partition an Amiga will **mount**.
 *
 * Only the two Kickstart carries. SD-1 embeds no filesystem driver in the RDB
 * it writes, and a `PDS\3` or `SFS\0` partition with no driver anywhere on the
 * card is one an Amiga ignores in silence — ART-084, the failure the Hard Disk
 * studio already labels. Offering them here would be offering that failure;
 * embedding the driver is SD-2's work and they come back with it (§89).
 */
export const CARD_FS_CHOICES: { value: AmigaHardDiskFs; label: string }[] = [
  { value: "ffsstandard", label: "Fast File System (DOS\\1)" },
  { value: "ffsdircache", label: "Fast File System DC (DOS\\3)" },
];

/**
 * The Amiga disk's one partition, as ART proposes it.
 *
 * **FFS, because Kickstart mounts FFS itself.** SD-1 embeds no filesystem
 * driver in the RDB it writes, and a `PDS\3` partition with no driver anywhere
 * on the card is one an Amiga ignores in silence — ART-084, and the reason the
 * default here is not the one a finished PiStorm card would use.
 *
 * `num_buffers` is deliberately absent: the core has a measured default of 600
 * and a screen that never asked has no business stating one (ART-096).
 */
export function defaultPartition(): PartitionSpec {
  return {
    drive_name: "SDH0",
    fs_type: "ffsstandard",
    size_mb: 512,
    bootable: true,
    boot_priority: 0,
  };
}

/** How much the boot partition will hold. */
export function payloadBytes(files: PlacedFile[]): number {
  return files.reduce((total, file) => total + file.bytes, 0);
}

/** The sentence for a warning, for the component to render. */
export function warningPhrase(warning: CardBuildWarning): Phrase {
  switch (warning.kind) {
    case "no-kickstart":
      return { key: "cardBuilder.warning.noKickstart" };
    case "rom-unrecognised":
      return { key: "cardBuilder.warning.romUnrecognised" };
    case "rom-wrong-machine":
      return { key: "cardBuilder.warning.romWrongMachine", params: { rom: warning.rom } };
    case "volumes-unformatted":
      return { key: "cardBuilder.warning.volumesUnformatted" };
  }
}

/**
 * Why the card cannot be built yet, or null when it can.
 *
 * A reason rather than a boolean: a disabled button that does not say why is
 * the defect ART-100 was, and §92 puts PREVIEW before APPLY — so "nobody has
 * looked at this yet" is one of the answers.
 */
export function buildBlocker(
  request: CardBuildRequest,
  plan: CardBuildPlan | null
): Phrase | null {
  if (!request.archive.trim()) return { key: "cardBuilder.blocked.noArchive" };
  if (!request.dest.trim()) return { key: "cardBuilder.blocked.noDestination" };
  if (request.partitions.length === 0) return { key: "cardBuilder.blocked.noPartitions" };
  if (!plan) return { key: "cardBuilder.blocked.notPlanned" };
  if (plan.dest_exists) return { key: "cardBuilder.blocked.destExists" };
  return null;
}
