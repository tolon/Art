// Typed wrappers for Gotek & FlashFloppy commands.

import { invoke } from "@tauri-apps/api/core";

export type FlashFloppyNavMode = "native" | "quickslot" | "indexed";

export type FlashFloppyDisplay =
  | "oled-128x32"
  | "oled-128x64"
  | "lcd-16x2"
  | "7seg";

export type RotaryMode = "track" | "quickslot" | "buttons" | "half";

export interface FlashFloppyConfig {
  nav_mode: FlashFloppyNavMode;
  display_type: FlashFloppyDisplay;
  oled_font: string;
  rotary: RotaryMode;
  step_volume: number;
  interface: string;
  host: string;
  write_protect: boolean;
  side_select_polarity: string;
}

export interface GotekSlot {
  slot_num: number;
  file_path: string;
  title: string;
}

export interface GotekDriveInfo {
  drive_path: string;
  is_flashfloppy: boolean;
  config: FlashFloppyConfig;
  slots: GotekSlot[];
  adf_files: string[];
}

export async function gotekScan(drivePath: string): Promise<GotekDriveInfo> {
  return invoke<GotekDriveInfo>("gotek_scan", { drivePath });
}

export async function gotekSave(
  drivePath: string,
  config: FlashFloppyConfig,
  slots: GotekSlot[]
): Promise<void> {
  return invoke<void>("gotek_save", {
    drivePath,
    config,
    slots,
  });
}
