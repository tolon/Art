// One-click WHDLoad install (spec §82).
// Mirrors src-tauri/src/commands/whdload.rs.
//
//     Game.lha → DROP → WHDLoad detected → Install to HDF → Backup → Apply → Verify
//
// `whdloadPlan` writes nothing and answers every question the user needs before
// deciding — what ART detected and how sure it is, where the game will land,
// what it costs, and why ART would refuse. Only then `whdloadInstall`.

import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

import type { Confidence } from "@/lib/sources";
import type { CopyPlan } from "@/lib/volumeWrite";

/** What ART found in the archive, and on what grounds (§14, §34). */
export interface WhdloadVerdict {
  confidence: Confidence;
  slave: string | null;
  executable: string | null;
  has_data_dir: boolean;
  has_icon: boolean;
  notes: string;
}

/** Where the game is inside the archive. */
export interface PackLayout {
  /** The drawer that is the pack; empty when the archive root is. */
  root: string;
  /** What the drawer will be called on the Amiga. */
  name: string;
  slave: string;
  /**
   * The pack's icon, which sits *beside* the drawer. Without it the game is
   * invisible on Workbench — which looks exactly like a failed install.
   */
  icon: string | null;
  /** Files that are not part of the game, usually a readme. Left behind. */
  outside: string[];
  /** The archive holds an Install script, so the game is not installed yet. */
  needs_installer: boolean;
}

export interface WhdloadPlan {
  verdict: WhdloadVerdict;
  layout: PackLayout;
  /** Where it lands, as an Amiga path: `Games:Turrican`. */
  drawer: string;
  volume_name: string;
  cost: CopyPlan;
  name_taken: boolean;
  /** Why ART will not install this. Never null when it refuses. */
  refusal: string | null;
}

export interface WhdloadOutcome {
  drawer: string;
  files: number;
  directories: number;
  bytes: number;
  /** Files read back out of the volume and checked. */
  verified: number;
  icon_installed: boolean;
  skipped: string[];
  backup: string | null;
}

export type WhdloadResult = {
  kind: "installed";
  job_id: number;
  outcome: WhdloadOutcome;
};

export const WHDLOAD_EVENT = "whdload-result";

/** What installing this archive would do. Writes nothing. */
export async function whdloadPlan(
  archive: string,
  image: string,
  volumeIndex: number,
  dirBlock: number | null = null
): Promise<WhdloadPlan> {
  return invoke<WhdloadPlan>("whdload_plan", {
    archive,
    image,
    volumeIndex,
    dirBlock,
  });
}

/** Install it. Returns a job id (§54). */
export async function whdloadInstall(
  archive: string,
  image: string,
  volumeIndex: number,
  dirBlock: number | null = null
): Promise<number> {
  return invoke<number>("whdload_install", {
    archive,
    image,
    volumeIndex,
    dirBlock,
  });
}

export function onWhdloadResult(
  handler: (result: WhdloadResult) => void
): Promise<() => void> {
  return listen<WhdloadResult>(WHDLOAD_EVENT, (event) => handler(event.payload));
}

/** How to describe the detection in one line. */
export function describeVerdict(verdict: WhdloadVerdict): string {
  switch (verdict.confidence) {
    case "HIGH":
      return "This is a WHDLoad package";
    case "MEDIUM":
      return "This looks like a WHDLoad package, but not conclusively";
    case "LOW":
      return "Only a weak WHDLoad signal — this may be an ordinary archive";
    case "UNKNOWN":
      return "No WHDLoad markers found";
  }
}

/** What the install produced, in one line. */
export function describeOutcome(outcome: WhdloadOutcome): string {
  const files = `${outcome.files} file${outcome.files === 1 ? "" : "s"}`;
  const verified =
    outcome.verified === outcome.files
      ? "all verified"
      : `${outcome.verified} of them verified`;
  const icon = outcome.icon_installed
    ? ""
    : " — without an icon, so it will not show up on Workbench";

  return `Installed to ${outcome.drawer}: ${files}, ${verified}${icon}`;
}
