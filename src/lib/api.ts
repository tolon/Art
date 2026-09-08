// Typed wrappers around Tauri `invoke`.
// All backend calls go through here so error handling is centralised.

import { invoke } from "@tauri-apps/api/core";
import type {
  AppInfo,
  DroppedAnalysis,
  Plan,
  WorkflowOutcome,
} from "@/types";

export async function ping(): Promise<string> {
  return invoke<string>("ping");
}

export async function getAppInfo(): Promise<AppInfo> {
  return invoke<AppInfo>("app_info");
}

export async function listWorkflows(): Promise<string[]> {
  return invoke<string[]>("list_workflows");
}

export async function planPath(path: string): Promise<Plan> {
  return invoke<Plan>("plan_path", { path });
}

export async function analyzePaths(paths: string[]): Promise<DroppedAnalysis[]> {
  return invoke<DroppedAnalysis[]>("analyze_paths", { paths });
}

/**
 * Run an `execute`-kind workflow the engine offered for this object.
 *
 * The engine refuses anything that is not read-only — actions that change data
 * are performed in their own studio, where they can be previewed and backed up.
 */
export async function runWorkflow(
  path: string,
  workflowId: string
): Promise<WorkflowOutcome> {
  return invoke<WorkflowOutcome>("run_workflow", { path, workflowId });
}

/**
 * Where Cloanto's Amiga Forever keeps its shared material on this machine.
 * Mirrors `commands::system::AmigaForeverFolders`.
 *
 * Both fields are `null` on a machine that does not have it, which is the
 * ordinary answer and never an error.
 */
export interface AmigaForeverFolders {
  /** `%AMIGAFOREVERDATA%\Shared\adf`, when that folder is really there. */
  adf: string | null;
  /** `%AMIGAFOREVERDATA%\Shared\rom`, likewise. */
  rom: string | null;
}

/**
 * Ask the host where Amiga Forever's shared folders are (design § 3.5).
 *
 * Read-only in the strongest sense: it reads one environment variable and
 * asks whether two folders exist. It opens nothing and lists nothing, and
 * **nothing is added anywhere by calling it** — the OS Builder shows the
 * answer as one suggestion line with an Add button, and the click is the
 * user acting. `remembered.ts`'s rule: nothing changes unless the user
 * changes it.
 */
export async function hostAmigaForeverFolders(): Promise<AmigaForeverFolders> {
  return invoke<AmigaForeverFolders>("host_amiga_forever_folders");
}
