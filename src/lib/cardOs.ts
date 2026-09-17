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

export type RtbSource =
  | { kind: "loose"; path: string }
  | { kind: "in-archive"; archive: string; member: string }
  | { kind: "missing" };

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
  /** Some item's `.RTB` was found nowhere: name Aminet `util/boot/skick346`. */
  rtbMissing: boolean;
}

export interface PreparedCard {
  measured: MeasuredCard;
  roots: string[][];
  leftBehind: string[];
  whdload: WhdloadChoice | null;
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
  hstImagerPath?: string;
}

export type BuildPhase = "whdload" | "card" | "partitions" | "check";

/** Three endings, kept apart: a stop is never a failure. */
export type CardOsEnding =
  | { ending: "succeeded" }
  | { ending: "failed"; phase: BuildPhase; code: string; message: string }
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
  | { outcome: "not-removed"; path: string; why: string };

export const CARD_OS_BUILD_EVENT = "card-os-build-result";

export interface CardOsBuildResult {
  jobId: number;
  session: number;
  image: string;
  ending: CardOsEnding;
  whdload: WhdloadInstalled | null;
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
