// First boot — the Amiga finishes what the host cannot start.
// Mirrors src-tauri/src/commands/firstboot.rs and src-tauri/src/core/firstboot/.
//
// `firstbootPreview` writes nothing (§92 PREVIEW). `firstbootWrite` puts the
// files into the tree and merges one block into S:User-Startup.
//
// Endings stay distinct (spec §4.3): a card with no report has *not booted*,
// never "failed".

import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

import { waitedSeconds, type WireDuration } from "@/lib/amigainstall";
import type { Phrase } from "@/lib/phrase";

export type FatMount = { kind: "available" } | { kind: "unavailable"; needs: string };

export interface PlannedStep {
  name: string;
  treePath: string;
  fixed: boolean;
}

export interface FirstBootPlan {
  tree: string;
  steps: PlannedStep[];
  fatMount: FatMount;
  userStartupExists: boolean;
  alreadyWritten: boolean;
  bytesAdded: number;
}

export interface FirstBootWritten {
  files: string[];
  userStartupBackup: string | null;
  userStartupCreated: boolean;
}

export type StepOutcome =
  | { kind: "ok" }
  | { kind: "skipped"; reason: string }
  | { kind: "refused"; rc: number }
  | { kind: "unfinished" };

export type Ending = "not-booted" | "unfinished" | "done-all" | "done-partial";

export interface Detected {
  system: string;
  rpi: string;
  kick: string;
}

export interface StepReport {
  name: string;
  outcome: StepOutcome;
  details: string[];
}

export interface FirstBootReport {
  version: number | null;
  system: Detected | null;
  steps: StepReport[];
  ending: Ending;
  fatCopyFailed: boolean;
  rebootRequestedBy: string | null;
  unknown: string[];
}

export async function firstbootPreview(tree: string): Promise<FirstBootPlan> {
  return invoke<FirstBootPlan>("firstboot_preview", { tree });
}

export async function firstbootWrite(tree: string): Promise<FirstBootWritten> {
  return invoke<FirstBootWritten>("firstboot_write", { tree });
}

// ---------------------------------------------------------------------------
// Reading a card's first-boot report back (§9)
// ---------------------------------------------------------------------------
//
// Where the report actually came from. `"amiga-volume"` is the copy spec §9
// says wins where both exist; `"fat"` is the fallback for a Windows user who
// cannot otherwise read PFS3 or FFS; `"none"` means the card has not booted
// the first-boot block yet — not an error.
export type ReportSource = "fat" | "amiga-volume" | "none";

export interface CardFirstBootReport {
  source: ReportSource;
  report: FirstBootReport;
}

/** Read a card's first-boot report — no card operation, no boot, just files
 *  already on the image. Never opens the card for writing. */
export async function cardFirstBootReport(path: string): Promise<CardFirstBootReport> {
  return invoke<CardFirstBootReport>("card_firstboot_report", { path });
}

export function stepOutcomePhrase(outcome: StepOutcome): Phrase {
  switch (outcome.kind) {
    case "ok":
      return { key: "firstboot.step.ok" };
    case "skipped":
      return { key: "firstboot.step.skipped", params: { reason: outcome.reason } };
    case "refused":
      return { key: "firstboot.step.refused", params: { rc: outcome.rc } };
    case "unfinished":
      return { key: "firstboot.step.unfinished" };
  }
}

export function endingPhrase(ending: Ending): Phrase {
  switch (ending) {
    case "not-booted":
      return { key: "firstboot.ending.notBooted" };
    case "unfinished":
      return { key: "firstboot.ending.unfinished" };
    case "done-all":
      return { key: "firstboot.ending.doneAll" };
    case "done-partial":
      return { key: "firstboot.ending.donePartial" };
  }
}

export function fatMountPhrase(fat: FatMount): Phrase {
  return fat.kind === "available"
    ? { key: "firstboot.fat.available" }
    : { key: "firstboot.fat.unavailable", params: { needs: fat.needs } };
}

/** Which copy of the report was actually read (spec §9: the Amiga volume's
 *  own `S/FirstBoot.log` wins over the FAT partition's `art-firstboot.log`
 *  where both exist). `null` for `"none"` — a card with neither copy has not
 *  booted, and the not-booted sentence already says so; there is nothing
 *  about a source to add to it. */
export function reportSourcePhrase(source: ReportSource): Phrase | null {
  switch (source) {
    case "fat":
      return { key: "firstboot.report.source.fat" };
    case "amiga-volume":
      return { key: "firstboot.report.source.amigaVolume" };
    case "none":
      return null;
  }
}

/** The tone a row takes — colour is never the only signal, the key wording is. */
export function stepOutcomeTone(outcome: StepOutcome): "ok" | "muted" | "warn" | "err" {
  switch (outcome.kind) {
    case "ok":
      return "ok";
    case "skipped":
      return "muted";
    case "refused":
      return "warn";
    case "unfinished":
      return "err";
  }
}

// ---------------------------------------------------------------------------
// Rehearsing the first boot under WinUAE
// ---------------------------------------------------------------------------
//
// A rehearsal boots a **copy** of the tree, never the tree. It proves the
// mechanism — the dispatcher runs, each step reports, the block removes
// itself — and it can never prove the Pi3/Pi4 hardware branch, because under
// an emulator `10-hardware` writes `skipped uae` and copies nothing. That
// half closes on a real card only.
//
// Four endings, four sentences, exactly like `amigainstall`'s: "the window
// was closed" and "nobody answered in time" call for opposite next steps, so
// collapsing them into "it did not work" is the defect, not a shortcut.

/** How a rehearsal ended. Mirrors `core::amigainstall::rehearse::RehearsalOutcome`,
 *  whose `kind` tags are kebab-case. Every variant carries the report,
 *  including the two that did not finish: a run that got partway still told
 *  the host something. */
export type RehearsalOutcome =
  | { kind: "finished"; report: FirstBootReport }
  | { kind: "step-refused"; report: FirstBootReport }
  | { kind: "timed-out"; waited: WireDuration; report: FirstBootReport }
  | { kind: "emulator-closed"; waited: WireDuration; report: FirstBootReport };

/** A finished rehearsal's own answer. `job_id` is snake_case to match every
 *  other job result in ART. */
export interface RehearsalResult {
  job_id: number;
  outcome: RehearsalOutcome;
  /** The copy that booted: where it still is when `discarded` is false. */
  copy: string;
  /** True only when the copy really was removed. A failed removal reports
   *  false and keeps the path, because no screen may claim a discard that did
   *  not happen. */
  discarded: boolean;
}

export interface RehearseRequest {
  tree: string;
  kickstart: string;
  profile?: string | null;
}

/** The event a finished rehearsal's own answer arrives on. */
export const FIRSTBOOT_REHEARSAL_EVENT = "firstboot-rehearsal-result";

/**
 * Boot a copy of the tree under WinUAE and read the Amiga's own report.
 * Returns a job id (§54) — progress on the ordinary `job-progress` event, the
 * answer on [`FIRSTBOOT_REHEARSAL_EVENT`].
 *
 * A tree with no first boot written into it, an unknown machine id and a
 * missing emulator all reject synchronously, before any job starts.
 */
export async function firstbootRehearse(
  request: RehearseRequest,
  winuaePath?: string | null
): Promise<number> {
  return invoke<number>("firstboot_rehearse", {
    request,
    winuaePath: winuaePath ?? null,
  });
}

/** Subscribe to finished rehearsals. A cancelled or failed job never sends
 *  one — the job bar is where those are seen. */
export async function onFirstBootRehearsalResult(
  handler: (result: RehearsalResult) => void
): Promise<UnlistenFn> {
  return listen<RehearsalResult>(FIRSTBOOT_REHEARSAL_EVENT, (event) => handler(event.payload));
}

/** What happened, in one sentence — a different key for every ending. */
export function rehearsalOutcomePhrase(outcome: RehearsalOutcome): Phrase {
  switch (outcome.kind) {
    case "finished":
      return { key: "firstboot.rehearsal.outcome.finished" };
    case "step-refused":
      return { key: "firstboot.rehearsal.outcome.stepRefused" };
    case "timed-out":
      return {
        key: "firstboot.rehearsal.outcome.timedOut",
        params: { seconds: waitedSeconds(outcome.waited) },
      };
    case "emulator-closed":
      return {
        key: "firstboot.rehearsal.outcome.emulatorClosed",
        params: { seconds: waitedSeconds(outcome.waited) },
      };
  }
}

/** What to do about it — again one per ending, because the next step is what
 *  actually differs between them. */
export function rehearsalNextStepPhrase(outcome: RehearsalOutcome): Phrase {
  switch (outcome.kind) {
    case "finished":
      return { key: "firstboot.rehearsal.next.finished" };
    case "step-refused":
      return { key: "firstboot.rehearsal.next.stepRefused" };
    case "timed-out":
      return { key: "firstboot.rehearsal.next.timedOut" };
    case "emulator-closed":
      return { key: "firstboot.rehearsal.next.emulatorClosed" };
  }
}

/** How the rehearsal's report is coloured. Never the only signal — each
 *  ending already says which it is in words. */
export function rehearsalTone(outcome: RehearsalOutcome): "ok" | "warn" | "err" {
  switch (outcome.kind) {
    case "finished":
      return "ok";
    case "step-refused":
      return "err";
    case "timed-out":
    case "emulator-closed":
      return "warn";
  }
}
