// Typed wrappers for forensic hex & sector analysis.

import { invoke } from "@tauri-apps/api/core";

export interface HexLine {
  offset: number;
  offset_hex: string;
  bytes_hex: string;
  ascii: string;
}

export interface SignatureMatch {
  offset: number;
  signature: string;
  description: string;
}

export interface HexChunk {
  file_path: string;
  total_file_size: number;
  offset: number;
  length: number;
  track: number | null;
  sector: number | null;
  block: number | null;
  lines: HexLine[];
  signatures: SignatureMatch[];
}

export async function hexRead(
  path: string,
  offset: number,
  length: number
): Promise<HexChunk> {
  return invoke<HexChunk>("hex_read", {
    path,
    offset,
    length,
  });
}
