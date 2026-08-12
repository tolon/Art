// Typed wrappers for PiStorm & Emu68 cards.
//
// **Every option here is a documented Emu68 token or a Raspberry Pi firmware
// key.** That is the rule, and it is the whole of ART-090's fix: this screen
// once offered a JIT switch (Emu68 *is* a JIT), an MMU switch (it emulates no
// MMU) and a Fast RAM slider (it maps RAM itself), and wrote three tokens Emu68
// has never read.
//
// The tables are not restated here — `pistorm_hardware_matrix` returns them
// from Rust, where they live beside the tests that pin them. A second copy in
// TypeScript is a second copy to be wrong.

import { invoke } from "@tauri-apps/api/core";

// ---------------------------------------------------------------------------
// Hardware
// ---------------------------------------------------------------------------

export type AmigaTarget = "a500" | "a1000" | "a2000" | "a600" | "a1200";

export type PistormVariant = "classic" | "pistorm600" | "pistorm16" | "pistorm32-lite";

export type PiModel =
  | "zero2-w"
  | "pi3-a"
  | "pi3-a-plus"
  | "pi3-b"
  | "pi3-b-plus"
  | "pi4-b"
  | "cm4";

/** How well a Pi is known to work on a board. */
export type PiSupport = "supported" | "reported";

/**
 * The three choices the whole screen derives from.
 *
 * One dropdown was never enough: the kernel build follows the board, the
 * storage device name follows the Pi, and which `cmdline.txt` tokens are even
 * meaningful follows the Amiga.
 */
export interface PistormHardware {
  amiga: AmigaTarget;
  variant: PistormVariant;
  pi: PiModel;
}

export interface PiChoice {
  model: PiModel;
  name: string;
  support: PiSupport;
  /** `brcm-sdhc.device` or `brcm-emmc.device` — the name that reaches a
   *  mountlist, and a wrong one mounts nothing. */
  storage_device: string;
  ram_min_mb: number;
  ram_max_mb: number;
}

export interface VariantChoice {
  variant: PistormVariant;
  name: string;
  kernel_archive: string;
  has_one_slot_option: boolean;
  pi_models: PiChoice[];
}

export interface AmigaChoice {
  amiga: AmigaTarget;
  name: string;
  /** Whether the slow-RAM tokens mean anything on this machine. */
  has_slow_ram: boolean;
  variants: VariantChoice[];
}

/**
 * Something worth telling the user about the combination they picked.
 *
 * An id, not a sentence: `core/` writes no user-facing English, so these are
 * resolved through `pistorm.note.*` and arrive in the user's own language.
 */
export type HardwareNote =
  | "pi-not-guaranteed"
  | "cm4-needs-lite-for-sd-card"
  | "pi-physical-fit"
  | "no-activity-led"
  | "needs-cpu-adapter"
  | "power-supply-quality"
  | "ram-beyond-what-amiga-os-uses";

// ---------------------------------------------------------------------------
// `cmdline.txt` — the Emu68 options
// ---------------------------------------------------------------------------

/** `sd.unit0` / `emmc.unit0` — how the card is exposed to the Amiga. */
export type StorageExposure = "off" | "read-only" | "read-write";

/** One field per documented token. See `core/pistorm/options.rs`. */
export interface Emu68Options {
  storage_unit0: StorageExposure;
  storage_verbose: number | null;
  storage_low_speed: boolean;
  storage_clock_mhz: number | null;
  vbr_move: boolean;
  nofpu: boolean;
  /** `enable_cache` — the JIT *cache* from startup. Not a switch for the JIT;
   *  there is no such switch. */
  enable_cache: boolean;
  limit_2g: boolean;
  z2_ram_size: number | null;
  enable_c0_slow: boolean;
  enable_c8_slow: boolean;
  enable_d0_slow: boolean;
  move_slow_to_chip: boolean;
  copy_rom_kb: number | null;
  checksum_rom: boolean;
  vc4_mem_mb: number | null;
  chip_slowdown: boolean;
  cs_dist: number | null;
  swap_df0_with: number | null;
  one_slot: boolean;
  debug: boolean;
  disassemble: boolean;
  async_log: boolean;
  fast_serial: boolean;
  buptest_kb: number | null;
  bupiter: number | null;
}

export const DEFAULT_EMU68_OPTIONS: Emu68Options = {
  storage_unit0: "read-only",
  storage_verbose: null,
  storage_low_speed: false,
  storage_clock_mhz: null,
  vbr_move: false,
  nofpu: false,
  enable_cache: false,
  limit_2g: false,
  z2_ram_size: null,
  enable_c0_slow: false,
  enable_c8_slow: false,
  enable_d0_slow: false,
  move_slow_to_chip: false,
  copy_rom_kb: null,
  checksum_rom: false,
  vc4_mem_mb: null,
  chip_slowdown: false,
  cs_dist: null,
  swap_df0_with: null,
  one_slot: false,
  debug: false,
  disassemble: false,
  async_log: false,
  fast_serial: false,
  buptest_kb: null,
  bupiter: null,
};

export type Emu68Profile = "performance" | "daily" | "compatibility" | "diagnostics";

// ---------------------------------------------------------------------------
// `config.txt` — the Raspberry Pi firmware
// ---------------------------------------------------------------------------

export type DisplayMode =
  | "auto"
  | "dmt1080p60"
  | "cea1080p50"
  | "cea720p60"
  | { custom: { group: number; mode: number } };

export interface Overclock {
  arm_freq_mhz: number;
  over_voltage: number;
  force_turbo: boolean;
}

export interface FirmwareConfig {
  kickstart_file: string;
  display: DisplayMode;
  /** Null unless the user turned it on themselves. Never part of a profile. */
  overclock: Overclock | null;
  disable_bluetooth: boolean;
}

export const DEFAULT_FIRMWARE_CONFIG: FirmwareConfig = {
  kickstart_file: "kick.rom",
  display: "auto",
  overclock: null,
  disable_bluetooth: false,
};

export interface PistormSetup {
  hardware: PistormHardware;
  options: Emu68Options;
  firmware: FirmwareConfig;
}

export const DEFAULT_HARDWARE: PistormHardware = {
  amiga: "a500",
  variant: "classic",
  pi: "pi3-a",
};

export interface PistormCard {
  path: string;
  is_pistorm_card: boolean;
  has_kernel: boolean;
  has_config_txt: boolean;
  has_cmdline_txt: boolean;
  kickstart_files: string[];
  setup: PistormSetup;
  /** Boot parameters that are none of ART's business, shown read-only so the
   *  user can see for themselves that they survive a save. */
  unmanaged_cmdline: string[];
  /** `config_<name>.txt` sets beside `config.txt` — the MultibootOS pattern. */
  config_sets: string[];
}

export interface PistormPreview {
  cmdline_before: string;
  cmdline_after: string;
  config_before: string;
  config_after: string;
  unmanaged_cmdline: string[];
}

export interface PistormSaveOutcome {
  config_txt_backup: string | null;
  cmdline_txt_backup: string | null;
}

export interface ProfilePreview {
  options: Emu68Options;
  /** What the line will actually say — shown beside the card, because a
   *  profile that claims more than its tokens do is how the last set went
   *  wrong (ART-090). */
  tokens: string[];
}

// ---------------------------------------------------------------------------
// Commands
// ---------------------------------------------------------------------------

export async function pistormHardwareMatrix(): Promise<AmigaChoice[]> {
  return invoke<AmigaChoice[]>("pistorm_hardware_matrix");
}

export async function pistormHardwareNotes(
  hardware: PistormHardware
): Promise<HardwareNote[]> {
  return invoke<HardwareNote[]>("pistorm_hardware_notes", { hardware });
}

export async function pistormScan(
  path: string,
  hardware: PistormHardware
): Promise<PistormCard> {
  return invoke<PistormCard>("pistorm_scan", { path, hardware });
}

export async function pistormPreview(
  path: string,
  setup: PistormSetup
): Promise<PistormPreview> {
  return invoke<PistormPreview>("pistorm_preview", { path, setup });
}

export async function pistormSave(
  path: string,
  setup: PistormSetup
): Promise<PistormSaveOutcome> {
  return invoke<PistormSaveOutcome>("pistorm_save", { path, setup });
}

export async function pistormProfile(
  profile: Emu68Profile,
  hardware: PistormHardware
): Promise<ProfilePreview> {
  return invoke<ProfilePreview>("pistorm_profile", { profile, hardware });
}

export async function pistormTokens(
  options: Emu68Options,
  hardware: PistormHardware
): Promise<string[]> {
  return invoke<string[]>("pistorm_tokens", { options, hardware });
}
