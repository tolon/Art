/**
 * The one-button card (ART-340): a session, its preparation, its build.
 *
 * Typed wrappers over `src-tauri/src/commands/cardos.rs`. The order is the
 * screen's: `cardOsOpen` → the OS Builder fills `tree` → `cardOsPrepare`
 * (a job; the proposal arrives on `CARD_OS_PREPARE_EVENT`) → the user agrees
 * to Kickstarts → `cardOsBuild` (a job; **every** ending arrives on
 * `CARD_OS_BUILD_EVENT`) — or `cardOsClose` when the user gives up first.
 *
 * Every shape here mirrors the serde shape of its Rust type, field by field
 * (camelCase where the Rust type says so; `CardImagePlan` and
 * `PlannedPartition` carry no rename and arrive snake_case).
 */

import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

import type { HealthReport } from "@/lib/cardBuild";
import type { KickstartOffer, PlaceOutcome } from "@/lib/gameindex";
import type { PartitionSpec } from "@/lib/hdf";
import type { StepReport } from "@/lib/preload";
import type {
  Emu68Line,
  Emu68Options,
  FirmwareConfig,
  PistormHardware,
} from "@/lib/pistorm";

// ---------------------------------------------------------------------------
// Session
// ---------------------------------------------------------------------------

export interface CardOsOpened {
  session: number;
  /** The folder the OS Builder builds System's tree into. */
  tree: string;
}

/** A folder ART could not remove, and why — never claimed gone. */
export interface LeftBehind {
  path: string;
  why: string;
}

export interface CardOsClosed {
  scratchLeft: LeftBehind | null;
}

export async function cardOsOpen(): Promise<CardOsOpened> {
  return invoke<CardOsOpened>("card_os_open");
}

/** Close a session without building; its folder is removed. */
export async function cardOsClose(session: number): Promise<CardOsClosed> {
  return invoke<CardOsClosed>("card_os_close", { session });
}

// ---------------------------------------------------------------------------
// Prepare
// ---------------------------------------------------------------------------

/** One of the user's partitions, as the screen asks for it. */
export interface PartitionInput {
  volumeName: string;
  sources: string[];
  floorBytes?: number;
}

export interface CardOsPrepareRequest {
  session: number;
  cardGb: number;
  image: string;
  partitions: PartitionInput[];
  material: string[];
  pfs3Driver?: string | null;
  /** The card's boot Kickstart; its folder joins the Kickstart collection. */
  kickstart?: string | null;
  hstImagerPath?: string;
}

export interface PlannedPartition {
  drive_name: string;
  volume_name: string;
  bytes: number;
  spec: PartitionSpec;
}

export interface CardImagePlan {
  image_bytes: number;
  area_bytes: number;
  partitions: PlannedPartition[];
}

export type SourceKind =
  | { kind: "folder" }
  | { kind: "archive"; format: string }
  | { kind: "whdload-hardfile" }
  | { kind: "adf" };

export interface MeasuredSource {
  path: string;
  kind: SourceKind;
  files: number;
  bytes: number;
}

export interface MeasuredPartition {
  volumeName: string;
  driveName: string;
  sources: MeasuredSource[];
  writer: { writer: "native" | "hst-imager" };
}

export interface FoundDriver {
  path: string;
  fromArchive: string | null;
  version: number;
  revision: number;
}

export interface MeasuredCard {
  plan: CardImagePlan;
  system: MeasuredPartition;
  partitions: MeasuredPartition[];
  driver: FoundDriver;
  stagingBytes: number;
}

export type WhdloadOrigin =
  | { kind: "loose"; path: string }
  | { kind: "archive"; archive: string; member: string }
  | { kind: "hardfile"; image: string };

export interface WhdloadChoice {
  origin: WhdloadOrigin;
  version: number;
  revision: number;
  name: string;
  passedOver: string[];
}

/** Where a missing `.RTB` can be had: not every one is in `skick346`. */
export type RtbPackage = "aminet-skick346" | "whdload-seven-cities-of-gold";

export type RtbSource =
  | { kind: "loose"; path: string }
  | { kind: "in-archive"; archive: string; member: string }
  | { kind: "missing"; getFrom: RtbPackage };

export interface ProposedKickstart {
  /** The name WHDLoad looks for — the key `agreedKickstarts` passes back. */
  name: string;
  titles: string[];
  titlesMore: number;
  offer: KickstartOffer;
  rtb: RtbSource;
}

export interface KickstartProposal {
  items: ProposedKickstart[];
  unreadableSlaves: string[];
  /** Some item's `.RTB` was found nowhere; each such item's `getFrom` names its package. */
  rtbMissing: boolean;
}

/** The tree's own `C/WHDLoad`, which the build keeps, beside the best one found elsewhere. */
export interface TreeWhdload {
  path: string;
  /** As it states itself (`WHDLoad 18.9`); null when it states nothing. */
  name: string | null;
  bestElsewhere: string | null;
  newerElsewhere: boolean;
}

export interface PreparedCard {
  measured: MeasuredCard;
  roots: string[][];
  leftBehind: string[];
  whdload: WhdloadChoice | null;
  treeWhdload: TreeWhdload | null;
  kickstarts: KickstartProposal;
}

export const CARD_OS_PREPARE_EVENT = "card-os-prepare-result";

export interface CardOsPrepareResult {
  jobId: number;
  session: number;
  prepared: PreparedCard;
}

/** Prepare the session's card. Returns a job id; a refusal ends the job. */
export async function cardOsPrepare(request: CardOsPrepareRequest): Promise<number> {
  return invoke<number>("card_os_prepare", { request });
}

export async function onCardOsPrepareResult(
  handler: (result: CardOsPrepareResult) => void
): Promise<UnlistenFn> {
  return listen<CardOsPrepareResult>(CARD_OS_PREPARE_EVENT, (event) => handler(event.payload));
}

// ---------------------------------------------------------------------------
// Build
// ---------------------------------------------------------------------------

export interface CardOsBuildRequest {
  session: number;
  /** The Kickstart names the user agreed to. Never paths. */
  agreedKickstarts: string[];
  archive: string;
  kickstart: string | null;
  label: string;
  hardware: PistormHardware;
  line: Emu68Line;
  firmware?: FirmwareConfig;
  options?: Emu68Options;
  builtAt?: string | null;
  // No hstImagerPath: the build uses the hst-imager its preparation asked.
}

export type BuildPhase = "whdload" | "card" | "partitions" | "check";

/**
 * A card refusal's `ART-*` code, its English sentence (for the log), and the
 * typed parameters both catalogues build a sentence from (round 4, task 3 —
 * this reverses round 3's ruling that a refusal's reason was prose). Shared
 * by `CardOsEnding`'s `refused` and `failed` variants.
 */
export interface CardRefusal {
  code: string;
  message: string;
  params: Record<string, string>;
}

/**
 * Four endings, kept apart: a refusal (before anything of the image was
 * written; its next step is the user's) is never a failure, and a stop is
 * never either.
 */
export type CardOsEnding =
  | { ending: "succeeded" }
  | ({ ending: "refused"; phase: BuildPhase } & CardRefusal)
  | ({ ending: "failed"; phase: BuildPhase } & CardRefusal)
  | { ending: "stopped"; phase: BuildPhase };

export type TreeWrite =
  | { outcome: "written"; path: string }
  | { outcome: "already-there"; path: string }
  | { outcome: "kept-existing"; path: string };

export interface WhdloadInstalled {
  binary: TreeWrite;
  prefs: TreeWrite | null;
}

export interface PlacedKickstart {
  name: string;
  image: PlaceOutcome;
  rtb: PlaceOutcome;
}

/** What became of `<image>.partial` — always said. */
export type PartialRemoval =
  | { outcome: "not-created" }
  | { outcome: "removed"; path: string }
  /** This run created it, and it was already gone when the run went to remove it. */
  | { outcome: "already-gone"; path: string }
  | { outcome: "not-removed"; path: string; why: string };

export const CARD_OS_BUILD_EVENT = "card-os-build-result";

export interface CardOsBuildResult {
  jobId: number;
  session: number;
  image: string;
  ending: CardOsEnding;
  whdload: WhdloadInstalled | null;
  /** On any ending but success these went only into the session tree, which the ending removes. */
  kickstarts: PlacedKickstart[];
  steps: StepReport[];
  manifestPath: string | null;
  health: HealthReport | null;
  partial: PartialRemoval;
  scratchLeft: LeftBehind | null;
}

/** Build the prepared card. Returns a job id; every ending arrives on `CARD_OS_BUILD_EVENT`. */
export async function cardOsBuild(request: CardOsBuildRequest): Promise<number> {
  return invoke<number>("card_os_build", { request });
}

export async function onCardOsBuildResult(
  handler: (result: CardOsBuildResult) => void
): Promise<UnlistenFn> {
  return listen<CardOsBuildResult>(CARD_OS_BUILD_EVENT, (event) => handler(event.payload));
}

// ---------------------------------------------------------------------------
// Phase and count
// ---------------------------------------------------------------------------

/** A build's four phases, plus the preparation's own. */
export type CardOsPhaseName = BuildPhase | "prepare";

/** What a phase's count is measured in. */
export type PhaseUnit = "files" | "steps";

export const CARD_OS_PHASE_EVENT = "card-os-phase";

/**
 * How far a build (or a preparation) has got in one of its phases, so the
 * screen can say "Bölümler: 4 812 / 9 216 dosya" instead of nothing.
 * Emitted when a phase starts (`done: 0`) and as it advances.
 *
 * `total: null` when the phase cannot know its total — draw a count then,
 * never a bar (a bar of a guessed width carries no information).
 */
export interface CardOsPhaseEvent {
  jobId: number;
  session: number;
  phase: CardOsPhaseName;
  done: number;
  total: number | null;
  unit: PhaseUnit;
}

export async function onCardOsPhase(
  handler: (event: CardOsPhaseEvent) => void
): Promise<UnlistenFn> {
  return listen<CardOsPhaseEvent>(CARD_OS_PHASE_EVENT, (event) => handler(event.payload));
}

// ---------------------------------------------------------------------------
// Measure (round 4, task 4): sizes and the sizing refusal only, without
// staging anything or opening a session (R1) — the free-space question stays
// in `cardOsPrepare`.
// ---------------------------------------------------------------------------

export interface CardOsMeasureRequest {
  cardGb: number;
  /** The tree, already built (by the OS Builder, into a session opened separately). */
  tree: string;
  partitions: PartitionInput[];
  material: string[];
  pfs3Driver?: string | null;
  hstImagerPath?: string;
}

export const CARD_OS_MEASURE_EVENT = "card-os-measure-result";

export interface CardOsMeasureResult {
  jobId: number;
  measured: MeasuredCard | null;
  /** Anything `measure_card` refused — never a failed job: a preview's own
   *  refusal is its answer. */
  refusal: CardRefusal | null;
}

/** Measure a whole card without staging anything. Returns a job id; the
 *  answer arrives on `CARD_OS_MEASURE_EVENT`. */
export async function cardOsMeasure(request: CardOsMeasureRequest): Promise<number> {
  return invoke<number>("card_os_measure", { request });
}

export async function onCardOsMeasureResult(
  handler: (result: CardOsMeasureResult) => void
): Promise<UnlistenFn> {
  return listen<CardOsMeasureResult>(CARD_OS_MEASURE_EVENT, (event) => handler(event.payload));
}

// ---------------------------------------------------------------------------
// Classify (round 4, task 4): what a dropped path is as a card source, or why
// it cannot be one — the same call the drop pipeline makes (Q6).
// ---------------------------------------------------------------------------

/** Why a source cannot be used, each with its own next step — mirrors
 *  `core::error::UnusableSource`. */
export type UnusableSource =
  | { reason: "missing" }
  | { reason: "unreadable"; detail: string }
  | { reason: "archive-unreadable"; detail: string }
  | { reason: "hardfile-not-whdload"; detail: string }
  | { reason: "not-an-amiga-source"; formatHint: string }
  | { reason: "not-a-folder" };

export interface ClassifiedSource {
  path: string;
  kind: SourceKind | null;
  why: UnusableSource | null;
}

/** Classify every path as a card source. Read-only and synchronous: nothing
 *  is written, and every path is answered whether or not an earlier one
 *  could not be used. */
export async function cardOsClassify(paths: string[]): Promise<ClassifiedSource[]> {
  return invoke<ClassifiedSource[]>("card_os_classify", { paths });
}

// ---------------------------------------------------------------------------
// Check a volume name (round 4, task 4): the core's own AmigaDOS name rule
// (`core::volume::write::dir::check_name`), replacing `preload.ts`'s two
// restated copies (Q7).
// ---------------------------------------------------------------------------

export type VolumeNameVerdict =
  | { ok: true }
  | {
      ok: false;
      why: "empty" | "too-long" | "reserved-character";
      maxBytes: number;
    };

/** Check a volume name against AmigaDOS's own rule. Synchronous; never
 *  re-derived on the screen. */
export async function cardOsCheckVolumeName(name: string): Promise<VolumeNameVerdict> {
  return invoke<VolumeNameVerdict>("card_os_check_volume_name", { name });
}
