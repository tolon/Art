// Typed wrappers for the LHA Studio commands.

import { invoke } from "@tauri-apps/api/core";

export interface LhaEntry {
  path: string;
  is_dir: boolean;
  compressed_size: number;
  uncompressed_size: number;
  method: string;
  last_modified: number;
}

export interface LhaInfo {
  entry_count: number;
  total_uncompressed: number;
  total_compressed: number;
  entries: LhaEntry[];
}

export interface ExtractedEntry {
  source_path: string;
  destination: string;
  bytes: number;
  is_dir: boolean;
  skipped: boolean;
  reason: string | null;
}

export interface ExtractOutcome {
  extracted: ExtractedEntry[];
  total_bytes: number;
  errors: string[];
  aborted: boolean;
  abort_reason: string | null;
}

export type Confidence = "HIGH" | "MEDIUM" | "LOW" | "UNKNOWN";

export interface WhdloadResult {
  confidence: Confidence;
  slave: string | null;
  executable: string | null;
  has_data_dir: boolean;
  has_icon: boolean;
  notes: string;
}

export async function lhaOpen(path: string): Promise<LhaInfo> {
  return invoke<LhaInfo>("lha_open", { path });
}

export async function lhaExtract(path: string, dest: string): Promise<ExtractOutcome> {
  return invoke<ExtractOutcome>("lha_extract", { path, dest });
}

export async function lhaDetectWhdload(path: string): Promise<WhdloadResult> {
  return invoke<WhdloadResult>("lha_detect_whdload", { path });
}
