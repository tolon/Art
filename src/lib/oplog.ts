// Operation Log — the record of everything ART did to the user's data.
// Spec §53. Mirrors src-tauri/src/core/oplog.

import { invoke } from "@tauri-apps/api/core";

export type OperationOrigin =
  | { origin: "user_interface" }
  | { origin: "workflow"; workflow_id: string }
  | { origin: "ai_plan"; plan_id: string };

export type OperationOutcome =
  | { result: "success"; verification: boolean | null }
  | { result: "failure"; error_code: string; message: string };

export interface OperationRecord {
  /** Seconds since the Unix epoch. */
  timestamp: number;
  operation: string;
  source: string | null;
  destination: string | null;
  /** Where the previous version was preserved, when a backup was taken. */
  backup: string | null;
  /** Extra facts: `["Files", "247"]`. */
  details: [string, string][];
  outcome: OperationOutcome;
  origin: OperationOrigin["origin"];
  workflow_id?: string;
  plan_id?: string;
}

export async function oplogRecent(limit = 100): Promise<OperationRecord[]> {
  return invoke<OperationRecord[]>("oplog_recent", { limit });
}

/** The history as readable text, ready to paste into a bug report. */
export async function oplogExport(limit = 1000): Promise<string> {
  return invoke<string>("oplog_export", { limit });
}

/** Write the readable history to a file the user picked. */
export async function oplogExportTo(path: string, limit = 1000): Promise<void> {
  return invoke<void>("oplog_export_to", { path, limit });
}

export async function oplogPath(): Promise<string> {
  return invoke<string>("oplog_path");
}

/** True when the operation finished without an error. */
export function succeeded(record: OperationRecord): boolean {
  return record.outcome.result === "success";
}

/** A short status word for the UI: PASS / FAILED / Done / Failed. */
export function statusLabel(record: OperationRecord): string {
  if (record.outcome.result === "failure") return "Failed";
  if (record.outcome.verification === true) return "Verified";
  if (record.outcome.verification === false) return "Verification failed";
  return "Done";
}
