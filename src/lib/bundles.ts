// Package bundle commands — the curated catalogue's own screen.
// Mirrors src-tauri/src/core/sources/bundle and src-tauri/src/commands/bundles.rs.
//
// `bundlesList` is read-only and fetches nothing — a screen can call it on
// mount with no consequence. `bundlesDownload` is the one call that touches
// the network, and it only ever runs when the user starts it.

import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

/** The `EntrySource` tag — where one entry's file would come from. */
export type EntryKind =
  | "aminet"
  | "aminet-search"
  | "github-release"
  | "mirror"
  | "user-supplied";

/** A licence or permission condition the screen must state before the tick. */
export interface Permission {
  holder: string;
  note: string;
}

export interface EntrySummary {
  id: string;
  name: string;
  kind: EntryKind;
  permission: Permission | null;
}

export interface BundleSummary {
  id: string;
  order: number;
  entries: EntrySummary[];
}

/**
 * How one entry's download went. **Six** outcomes, not five — a cold-cache
 * download over an occupied library slot is `not-placed`, never
 * `downloaded`: the file at that path was not written by this run and may
 * not even be the same package (see `core/sources/bundle/run.rs`'s own doc
 * comment on `EntryOutcome::NotPlaced`).
 */
export type EntryOutcome =
  | { outcome: "downloaded"; bytes: number; path: string }
  | { outcome: "already-have"; path: string }
  | { outcome: "not-placed"; existing: string }
  | { outcome: "refused"; why: string }
  | { outcome: "failed"; error: string }
  | { outcome: "skipped" };

export interface EntryReport {
  id: string;
  name: string;
  outcome: EntryOutcome;
}

export interface BundleReport {
  entries: EntryReport[];
}

const BUNDLE_DOWNLOAD_EVENT = "bundle-download-result";

export interface BundleDownloadResult {
  job_id: number;
  report: BundleReport;
}

/** Every shipped set, with its entries. Fetches nothing. */
export async function bundlesList(): Promise<BundleSummary[]> {
  return invoke<BundleSummary[]>("bundles_list");
}

/**
 * Download the chosen entries, in the catalogue's own order. Returns a job
 * id; the report arrives on {@link onBundleDownloadResult}. Refused before a
 * job starts when `entryIds` is empty or names something ART does not ship.
 */
export async function bundlesDownload(entryIds: string[]): Promise<number> {
  return invoke<number>("bundles_download", { entryIds });
}

export async function onBundleDownloadResult(
  handler: (result: BundleDownloadResult) => void
): Promise<UnlistenFn> {
  return listen<BundleDownloadResult>(BUNDLE_DOWNLOAD_EVENT, (event) =>
    handler(event.payload)
  );
}
