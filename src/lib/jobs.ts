// Background jobs (spec §54, §55). Mirrors src-tauri/src/core/jobs
// and src-tauri/src/commands/jobs.rs.

import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

import type { Phrase } from "@/lib/phrase";

export type JobState =
  | { state: "running" }
  | { state: "finished" }
  | { state: "cancelled" }
  | { state: "failed"; error_code: string; message: string };

export interface JobProgress {
  id: number;
  /** What the job is, in the user's language. */
  title: string;
  done: number;
  /** Null while the size is unknown — show an indeterminate indicator, not a fake bar. */
  total: number | null;
  /** What is happening right now, e.g. the current file. */
  message: string;
  state: JobState;
}

export const JOB_EVENT = "job-progress";

export async function jobList(): Promise<JobProgress[]> {
  return invoke<JobProgress[]>("job_list");
}

/** Ask a job to stop at its next safe point. False when it already ended. */
export async function jobCancel(id: number): Promise<boolean> {
  return invoke<boolean>("job_cancel", { id });
}

export async function jobClearFinished(): Promise<void> {
  return invoke<void>("job_clear_finished");
}

/** Subscribe to progress updates. Returns an unlisten function. */
export async function onJobProgress(
  handler: (job: JobProgress) => void
): Promise<UnlistenFn> {
  return listen<JobProgress>(JOB_EVENT, (event) => handler(event.payload));
}

export function isRunning(job: JobProgress): boolean {
  return job.state.state === "running";
}

/** Completion as 0–1, or null when the total is unknown. */
export function fraction(job: JobProgress): number | null {
  if (job.total === null || job.total <= 0) return null;
  return Math.min(1, Math.max(0, job.done / job.total));
}

/** A short status word for the UI. */
export function jobStatusLabel(job: JobProgress): Phrase {
  switch (job.state.state) {
    case "running":
      return { key: "components.jobBar.status.running" };
    case "finished":
      return { key: "components.jobBar.status.done" };
    case "cancelled":
      return { key: "components.jobBar.status.cancelled" };
    case "failed":
      return { key: "components.jobBar.status.failed", params: { code: job.state.error_code } };
  }
}
