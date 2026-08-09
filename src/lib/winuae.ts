// Typed wrappers for WinUAE & Profile commands.

import { invoke } from "@tauri-apps/api/core";

export type CpuModel =
  | "m68000"
  | "m68010"
  | "m68020"
  | "m68ec020"
  | "m68030"
  | "m68040"
  | "m68060";

export type ChipsetModel = "ocs" | "ecs" | "aga";

export interface MemoryConfig {
  chip_kb: number;
  slow_kb: number;
  fast_mb: number;
  z3_fast_mb: number;
}

export interface FloppyConfig {
  drive_count: number;
  speed_percent: number;
}

export interface DisplayConfig {
  width: number;
  height: number;
  fullscreen: boolean;
  scanlines: boolean;
}

export interface AmigaProfile {
  id: string;
  name: string;
  description: string;
  cpu: CpuModel;
  cpu_speed_mhz: number;
  chipset: ChipsetModel;
  memory: MemoryConfig;
  floppy: FloppyConfig;
  display: DisplayConfig;
  kickstart_version: string;
  preferred_rom_sha256?: string | null;
  custom_rom_path?: string | null;
  is_builtin: boolean;
}

export interface WinUaeInstallation {
  found: boolean;
  executable_path: string | null;
  version: string | null;
  is_64bit: boolean;
}

export interface LaunchMedia {
  floppy_paths: string[];
  hardfile_paths: string[];
  kickstart_path: string | null;
  use_aros: boolean;
}

export async function winuaeDetect(customPath?: string): Promise<WinUaeInstallation> {
  return invoke<WinUaeInstallation>("winuae_detect", { customPath: customPath ?? null });
}

export async function winuaeListProfiles(): Promise<AmigaProfile[]> {
  return invoke<AmigaProfile[]>("winuae_list_profiles");
}

export async function winuaeLaunch(
  profile: AmigaProfile,
  media: LaunchMedia,
  winuaePath?: string
): Promise<number> {
  return invoke<number>("winuae_launch", {
    profile,
    media,
    winuaePath: winuaePath ?? null,
  });
}
