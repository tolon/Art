// Typed wrappers for PiStorm & Emu68 hardware commands.

import { invoke } from "@tauri-apps/api/core";

export type PistormBoard = "a500a2000" | "a1200lite" | "a600";

export type PistormProfileMode = "workstation" | "balancedwhdload" | "classiccompat";

export type RtgResolution =
  | "res1080p"
  | "res720p"
  | "res1024x768"
  | "res800x600"
  | "disabled";

export interface PistormConfig {
  board: PistormBoard;
  profile_mode: PistormProfileMode;
  fast_ram_mb: number;
  rtg_resolution: RtgResolution;
  enable_jit: boolean;
  enable_mmu: boolean;
  enable_wifi: boolean;
  enable_sd_storage: boolean;
  custom_kickstart_path: string | null;
}

export interface PistormDriveInfo {
  drive_path: string;
  is_pistorm_sd: boolean;
  has_emu68_img: boolean;
  has_config_txt: boolean;
  has_kickstart: boolean;
  kickstart_name: string | null;
  detected_config: PistormConfig;
}

export async function pistormScan(drivePath: string): Promise<PistormDriveInfo> {
  return invoke<PistormDriveInfo>("pistorm_scan", { drivePath });
}

export async function pistormSave(
  drivePath: string,
  config: PistormConfig
): Promise<void> {
  return invoke<void>("pistorm_save", {
    drivePath,
    config,
  });
}
