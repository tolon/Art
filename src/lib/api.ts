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
