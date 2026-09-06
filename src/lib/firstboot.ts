// First boot — the Amiga finishes what the host cannot start.
// Mirrors src-tauri/src/commands/firstboot.rs and src-tauri/src/core/firstboot/.
//
// `firstbootPreview` writes nothing (§92 PREVIEW). `firstbootWrite` puts the
// files into the tree and merges one block into S:User-Startup.
//
// Endings stay distinct (spec §4.3): a card with no report has *not booted*,
// never "failed".

import { invoke } from "@tauri-apps/api/core";

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
