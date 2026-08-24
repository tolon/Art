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
import { driverRequirement } from "@/lib/fsDriver";
import type { AmigaHardDiskFs, FileSystemInput, PartitionSpec } from "@/lib/hdf";
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
  /** Filesystem drivers to embed in the Amiga disk's RDB.
   *
   *  Built by `fileSystemInputsFor` from the chosen filesystem and the
   *  driver the user picked — empty for FFS, which Kickstart mounts itself.
   *  See `CARD_FS_CHOICES` for why this took until 2026-08-23 to exist. */
  file_systems: FileSystemInput[];
  /** The partitions of the card's **first** Amiga disk. */
  partitions: PartitionSpec[];
  /**
   * How much of the card the first Amiga disk takes. Absent, or 0, means
   * *"whatever is left"* - which the planner allows only for the last disk, so
   * a card with `extra_disks` has to state this one.
   */
  first_disk_bytes?: number;
  /** Further Amiga disks, each with its own RDB (SD-3 G16). */
  extra_disks?: AmigaDiskRequest[];
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
  | { kind: "rom-machine-unknown"; rom: string }
  | { kind: "rom-wrong-machine"; rom: string }
  /**
   * An FFS partition larger than this card's Kickstart can address (SD-5 G13).
   *
   * The original FFS does not refuse a partition past its 4 GiB addressing —
   * it writes past it and corrupts the drive. AmigaOS 3.1.4 and 3.2 carry FFS
   * v46, which addresses TD_64 and NSD natively, so this is raised only for an
   * older ROM or none at all.
   *
   * `romMajor` is `null` when no Kickstart was chosen, which is **not** the
   * same as an old one — and the two get different sentences.
   */
  | {
      kind: "partition-beyond-kickstart-ffs";
      driveName: string;
      bytes: number;
      limit: number;
      romMajor: number | null;
    }
  /**
   * Two bootable partitions claim the same priority (SD-3 G16).
   *
   * Higher boots first; what happens on a tie is not documented anywhere, so
   * ART names the pair rather than picking between somebody's systems.
   */
  | { kind: "tied-boot-priority"; priority: number; driveNames: string[] }
  /** The card's Amiga disks boot nothing — legitimate, and said out loud. */
  | { kind: "nothing-bootable" }
  | { kind: "volumes-unformatted" };

/** A ROM as ART identifies it. The fields this screen uses. */
export interface PlannedRom {
  name: string;
  version: string;
  revision: string;
  size_bytes: number;
  sha256: string;
  compatible_models: string[];
  /**
   * The numeric major, when the ROM names one - 40 for a 3.1, 47 for a 3.2.
   *
   * `null` for an AROS replacement, an undecoded Amiga Forever dump, or a file
   * that is not recognisably a Kickstart. It is what decides whether the
   * Kickstart's own FFS can address past 4 GiB, so it reaches the proposal.
   */
  major: number | null;
}

/**
 * Something about a proposed table the table itself does not say.
 *
 * SD-5 G13. See `core/card/propose.rs` - every number in a proposal was
 * measured off the two real cards ART's card model came from.
 */
export type ProposalNote =
  | { note: "split-for-kickstart-ffs"; pieces: number; limit: number; rom_major: number | null }
  | { note: "one-work-volume-because-pfs3" }
  | { note: "tail-unallocated"; bytes: number }
  | { note: "split-because-no-rom-chosen" };

/** A whole card, proposed. */
export interface ProposedTable {
  boot_bytes: number;
  partitions: PartitionSpec[];
  notes: ProposalNote[];
}

/** The sentence for a proposal note, for the component to render. */
export function proposalPhrase(note: ProposalNote): Phrase {
  switch (note.note) {
    case "split-for-kickstart-ffs":
      return {
        key: "cardBuilder.propose.note.splitForFfs",
        params: {
          pieces: note.pieces,
          gb: Math.round(note.limit / 1024 ** 3),
          major: note.rom_major ?? 0,
        },
      };
    case "split-because-no-rom-chosen":
      return { key: "cardBuilder.propose.note.splitNoRom" };
    case "one-work-volume-because-pfs3":
      return { key: "cardBuilder.propose.note.onePfs3" };
    case "tail-unallocated":
      return {
        key: "cardBuilder.propose.note.tail",
        params: { gb: Math.round(note.bytes / 1024 ** 3) },
      };
  }
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
/**
 * A volume table proposed for a card of this size.
 *
 * Arithmetic only - it reads nothing and writes nothing - so it is safe to
 * call whenever the size or the filesystem changes.
 */
export async function cardProposeTable(
  cardBytes: number,
  fsType: AmigaHardDiskFs,
  romMajor: number | null
): Promise<ProposedTable> {
  return invoke<ProposedTable>("card_propose_table", {
    cardBytes,
    fsType,
    romMajor,
  });
}

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
 * **PFS3 is back, and the note that promised it is the reason.** This list
 * held only the two Kickstart carries, because SD-1 embedded no driver in the
 * RDB it wrote and a `PDS\3` partition with no driver anywhere on the card is
 * one an Amiga ignores in silence (ART-084) — so offering it would have been
 * offering that failure, and the note said it "comes back with it" once SD-2
 * did the embedding. SD-2 did: `create_rdb_layout` embeds, `rdbtool` reads
 * what it wrote back byte-for-byte, and a real Kickstart mounted the result
 * once ART-126's wrong `PatchFlags` were fixed.
 *
 * **And PFS3 is what everyone else uses.** Both of the real cards ART's own
 * card model was measured from are PFS3 throughout; the Emu68 Imager's
 * `DiskDefaults` names PFS3 for both its partitions; Emu68's own SD tutorial
 * walks the user through PFS3aio; HstWB Installer uses PFS3AIO for most of its
 * images and keeps FFS for "unexpanded Amigas with only chip memory", which a
 * PiStorm machine is the opposite of.
 *
 * **SFS is deliberately still absent** — the owner's own decision on
 * 2026-08-22, recorded in the work list: the Emu68 Imager installs PFS3 and
 * not SFS, and nothing is yet known about the candidate crate's agreement with
 * the real handler.
 *
 * A choice whose driver is missing is offered but not selectable, with the
 * reason on screen: ART ships no `pfs3aio` and never will, the same rule as
 * the Kickstart.
 */
export interface CardFsChoice {
  value: AmigaHardDiskFs;
  label: string;
  /** Why it cannot be picked right now, or null when it can. */
  blocked: Phrase | null;
}

export function cardFsChoices(driverPath: string | null): CardFsChoice[] {
  const needsDriver = (value: AmigaHardDiskFs): Phrase | null =>
    driverRequirement(value).required && !driverPath
      ? { key: "cardBuilder.fs.needsDriver", params: { file: driverRequirement(value).hint } }
      : null;

  return [
    {
      value: "pfs3directscsi",
      label: "PFS3 (PDS\\3)",
      blocked: needsDriver("pfs3directscsi"),
    },
    {
      value: "pfs3standard",
      label: "PFS3 (PFS\\3)",
      blocked: needsDriver("pfs3standard"),
    },
    { value: "ffsstandard", label: "Fast File System (DOS\\1)", blocked: null },
    { value: "ffsdircache", label: "Fast File System DC (DOS\\3)", blocked: null },
  ];
}

/**
 * The second partition, as ART proposes it: **`SDH1`, taking whatever is left**.
 *
 * **Measured, not adopted.** Both of the real PiStorm cards ART's card model
 * came from carry `SDH0` *and* `SDH1` — CaffeineOS's area is exactly this
 * pair, `SDH0` bootable at priority 1 — and the Emu68 Imager's own
 * `DiskDefaults` names the same two, `Workbench` and `Work`. ART offered one
 * partition until 2026-08-23, which is the one shape neither working card has.
 *
 * `size_mb: 0` is the core's "whatever is left", the idiom `AreaSpec.size_bytes`
 * and `boot_bytes` already use. The screen deliberately does **not** work the
 * remainder out itself: that is `bytes_per_cyl` rounding, it lives in
 * `create_rdb_layout`, and a second copy of it is how the two start
 * disagreeing.
 *
 * Not bootable, so its boot priority never decides anything and ART does not
 * invent one. (The Imager writes 99 there; with `bootable` false it changes
 * nothing, and a number that changes nothing is a number to leave out.)
 */
export function defaultSecondPartition(fs: AmigaHardDiskFs): PartitionSpec {
  return {
    drive_name: "SDH1",
    fs_type: fs,
    size_mb: 0,
    bootable: false,
    boot_priority: 0,
  };
}

/**
 * The Amiga disk's system partition, as ART proposes it.
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
    // **1, not 0** — both real cards say 1 for the bootable one, and so does
    // the Imager's table. With a single bootable partition the two behave
    // identically, so this is not a fix; it is matching what the cards that
    // boot actually carry, and leaving room below for a second one.
    boot_priority: 1,
  };
}

/**
 * A further Amiga disk on the same card, with its own RDB.
 *
 * `size_bytes` of 0 means *"whatever is left"*, and only the **last** disk may
 * say it - the planner's own rule, restated nowhere else.
 */
export interface AmigaDiskRequest {
  size_bytes: number;
  partitions: PartitionSpec[];
}

/**
 * The drive names the second AmigaOS gets, and its boot priority.
 *
 * **Priority 0, below the first disk's 1**, which is what
 * [`defaultPartition`]'s own note left room for. That is the whole of what ART
 * decides about multiboot: AmigaOS already has the menu - hold both mouse
 * buttons at power-on and Early Startup lists every bootable partition - so
 * this only says which one starts when nobody holds anything. Equal priorities
 * would be a tie, which is undocumented on the Amiga and which
 * `core::card::multiboot` refuses to resolve rather than guessing.
 */
export const SECOND_SYSTEM_DRIVE = "SDH2";
export const SECOND_SYSTEM_PRIORITY = 0;

/** What a second AmigaOS asks for, or why it cannot be asked for. */
export type SecondSystem =
  | { ok: true; firstDiskBytes: number; extraDisks: AmigaDiskRequest[] }
  | { ok: false; why: Phrase };

/**
 * Split the card between two complete AmigaOS environments (SD-3 G16).
 *
 * The user says how big the **second** one is, because that is the thing they
 * are adding; the first takes what is left. The request is built the other way
 * round - the first disk's size stated, the second passed 0 - because
 * `plan_card` allows "whatever is left" only for the last disk, and stating
 * both would leave the rounding somewhere nobody asked for. So the sector
 * rounding lands on the second disk, which is the one whose size was a round
 * number the user typed rather than a remainder.
 *
 * **One partition, taking the whole disk.** A second environment is a second
 * system, not a second system plus a second work volume: the work volume is
 * shared and lives on the first disk. Offering a split here would be offering
 * a shape with nothing behind it (spec §96), the same reason the first disk
 * has a toggle rather than a partition editor.
 */
export function secondSystem(
  totalBytes: number,
  bootBytes: number,
  secondBytes: number,
  fs: AmigaHardDiskFs,
  firstSystemMb: number
): SecondSystem {
  const forAmiga = totalBytes - bootBytes;
  // What the first disk must still be able to hold: its own system partition.
  // Below that the card has two systems and nowhere to put the first one.
  const firstFloor = firstSystemMb * 1024 * 1024;

  if (secondBytes <= 0) {
    return { ok: false, why: { key: "cardBuilder.second.blocked.noSize" } };
  }
  if (forAmiga - secondBytes < firstFloor) {
    return {
      ok: false,
      why: {
        key: "cardBuilder.second.blocked.tooLarge",
        params: { mb: firstSystemMb },
      },
    };
  }

  return {
    ok: true,
    firstDiskBytes: forAmiga - secondBytes,
    extraDisks: [
      {
        size_bytes: 0,
        partitions: [
          {
            drive_name: SECOND_SYSTEM_DRIVE,
            fs_type: fs,
            size_mb: 0,
            bootable: true,
            boot_priority: SECOND_SYSTEM_PRIORITY,
          },
        ],
      },
    ],
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
    case "rom-machine-unknown":
      return {
        key: "cardBuilder.warning.romMachineUnknown",
        params: { rom: warning.rom },
      };
    case "rom-wrong-machine":
      return { key: "cardBuilder.warning.romWrongMachine", params: { rom: warning.rom } };
    case "partition-beyond-kickstart-ffs":
      // Two sentences, because "your Kickstart is too old" and "you have not
      // chosen one" send somebody to different places.
      return warning.romMajor === null
        ? {
            key: "cardBuilder.warning.beyondKickstartFfsNoRom",
            params: { drive: warning.driveName, gb: Math.round(warning.bytes / 1024 ** 3) },
          }
        : {
            key: "cardBuilder.warning.beyondKickstartFfs",
            params: {
              drive: warning.driveName,
              gb: Math.round(warning.bytes / 1024 ** 3),
              major: warning.romMajor,
            },
          };
    case "tied-boot-priority":
      return {
        key: "cardBuilder.warning.tiedBootPriority",
        params: { priority: warning.priority, drives: warning.driveNames.join(", ") },
      };
    case "nothing-bootable":
      return { key: "cardBuilder.warning.nothingBootable" };
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
  // Before the plan, because it is the one thing here the user can fix
  // without re-planning — and because a card whose partition names a
  // filesystem nothing on it carries is the exact image ART-084 is about.
  const needs = driverRequirement(request.partitions[0].fs_type);
  if (needs.required && request.file_systems.length === 0) {
    return { key: "cardBuilder.blocked.noDriver", params: { file: needs.hint } };
  }
  if (!plan) return { key: "cardBuilder.blocked.notPlanned" };
  if (plan.dest_exists) return { key: "cardBuilder.blocked.destExists" };
  return null;
}

// ---------------------------------------------------------------------------
// What does this file become in *this* card? (SD-1 · G15)
// ---------------------------------------------------------------------------

/** Which board and release line an Emu68 archive's *name* implies. */
export interface ArchiveNameMeans {
  variant: string;
  line: Emu68Line;
}

/** What a dropped file would become on the card being built. */
export type CardRole =
  | { kind: "emu68-archive"; means: ArchiveNameMeans[] }
  | { kind: "kickstart" }
  | { kind: "distro-config"; name: string }
  | { kind: "for-an-amiga-volume"; what: string }
  | { kind: "no-place-on-a-card"; what: string };

export interface CardIntakeItem {
  path: string;
  name: string;
  role: CardRole;
  /** Filled when the role is a Kickstart. */
  rom: PlannedRom | null;
}

/** Ask what each of these files would become on a card. */
export async function cardIntake(paths: string[]): Promise<CardIntakeItem[]> {
  return invoke<CardIntakeItem[]>("card_intake", { paths });
}

/**
 * The sentence for what a dropped file becomes.
 *
 * `for-an-amiga-volume` is the answer SD-1 owes most often and the one worth
 * getting right: the file is fine, the card is not ready for it yet.
 */
export function rolePhrase(role: CardRole): Phrase {
  switch (role.kind) {
    case "emu68-archive":
      return {
        key: "cardBuilder.intake.role.emu68Archive",
        params: { boards: role.means.map((m) => `${m.variant} · ${m.line}`).join(", ") },
      };
    case "kickstart":
      return { key: "cardBuilder.intake.role.kickstart" };
    case "distro-config":
      return { key: "cardBuilder.intake.role.distroConfig", params: { name: role.name } };
    case "for-an-amiga-volume":
      return { key: "cardBuilder.intake.role.forAnAmigaVolume" };
    case "no-place-on-a-card":
      return { key: "cardBuilder.intake.role.noPlace" };
  }
}

/**
 * What a drop changes about the form.
 *
 * Pure, so the rule that an archive fills the archive field and a ROM fills
 * the ROM field is testable without a screen — and so a second dropped
 * archive replacing the first is a decision written down rather than whatever
 * the loop happened to do.
 */
export function intakeFills(items: CardIntakeItem[]): {
  archive?: string;
  kickstart?: string;
} {
  const fills: { archive?: string; kickstart?: string } = {};
  for (const item of items) {
    // The last one wins: dropping two archives means the second is the one
    // just chosen, which is what dropping it says.
    if (item.role.kind === "emu68-archive") fills.archive = item.path;
    if (item.role.kind === "kickstart") fills.kickstart = item.path;
  }
  return fills;
}
