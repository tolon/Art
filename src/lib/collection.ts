// Typed wrappers for Amiga Collection Organizer commands.

import { invoke } from "@tauri-apps/api/core";

export type MediaKind = "adf" | "lhawhdload" | "hdf";
export type ChipsetRequirement = "ocsecs" | "aga";

export interface CollectionItem {
  id: string;
  title: string;
  clean_title: string;
  year: number | null;
  publisher: string | null;
  chipset: ChipsetRequirement;
  media_kind: MediaKind;
  primary_path: string;
  disks: string[];
  disk_count: usize;
}

export type usize = number;

/**
 * Start scanning a folder. Returns a job id straight away — the scan runs in
 * the background (spec §54) and the titles arrive in a `collection-scan-result`
 * event. Watch progress with the helpers in `@/lib/jobs`.
 */
export async function collectionScan(dirPath: string): Promise<number> {
  return invoke<number>("collection_scan", { dirPath });
}

/** Payload of the `collection-scan-result` event. */
export interface ScanResult {
  job_id: number;
  dir_path: string;
  items: CollectionItem[];
}

export const SCAN_RESULT_EVENT = "collection-scan-result";

/** Subscribe to finished scans. Returns an unlisten function. */
export async function onScanResult(
  handler: (result: ScanResult) => void
): Promise<() => void> {
  const { listen } = await import("@tauri-apps/api/event");
  return listen<ScanResult>(SCAN_RESULT_EVENT, (e) => handler(e.payload));
}
