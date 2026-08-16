// Typed wrappers for Kickstart ROM commands.

import { invoke } from "@tauri-apps/api/core";

export interface RomInfo {
  name: string;
  version: string;
  revision: string;
  size_bytes: number;
  sha256: string;
  crc32: string;
  is_cloanto: boolean;
  /** Only meaningful with `is_cloanto`: whether the `rom.key` that decodes
   *  it was found beside it. False means ART could read nothing of the
   *  image itself — the name says so and no machine is claimed. */
  key_available: boolean;
  is_aros: boolean;
  checksum_valid: boolean;
  compatible_models: string[];
  file_path: string;
}

export async function romIdentify(path: string): Promise<RomInfo> {
  return invoke<RomInfo>("rom_identify", { path });
}

export async function romScanDir(dirPath: string): Promise<RomInfo[]> {
  return invoke<RomInfo[]>("rom_scan_dir", { dirPath });
}
