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

/**
 * Which line of Emu68 releases a card is being built for.
 *
 * A field of its own because the archive names are not stable across the two,
 * and one of them changes meaning: in 1.0.x `Emu68-pistorm.zip` is the classic
 * board's firmware; in 1.1 alpha it is the PiStorm32-lite and PiStorm16
 * firmware, and the classic board's is `Emu68-pistorm-classic.zip` (ART-091).
 */
export type Emu68Line = "stable" | "alpha11";

/** What a board's archive is called in a release line — or why there is no
 *  answer. Two of the three cases are real, which a bare string cannot say. */
export type KernelArchive =
  | { kind: "named"; name: string }
  | { kind: "absent" }
  | { kind: "unstated" };

export interface ArchiveForLine {
  line: Emu68Line;
  archive: KernelArchive;
}

export interface VariantChoice {
  variant: PistormVariant;
  name: string;
  kernel_archives: ArchiveForLine[];
  has_one_slot_option: boolean;
  pi_models: PiChoice[];
}

/** The archive for a board in the chosen line, from the matrix Rust returned. */
export function archiveForLine(
  variant: VariantChoice | undefined,
  line: Emu68Line
): KernelArchive | undefined {
  return variant?.kernel_archives.find((entry) => entry.line === line)?.archive;
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
  | "ram-beyond-what-amiga-os-uses"
  | "needs-prerelease-emu68"
  | "archive-name-differs-by-release"
  | "archive-not-stated-for-this-release";

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
  /** Which Emu68 release line the card is being built for (ART-091). */
  line: Emu68Line;
  options: Emu68Options;
  firmware: FirmwareConfig;
}

/** What `core/rom` makes of a Kickstart-shaped file — mirrors `RomInfo`. */
export interface RomInfo {
  name: string;
  version: string;
  revision: string;
  size_bytes: number;
  sha256: string;
  crc32: string;
  is_cloanto: boolean;
  is_aros: boolean;
  checksum_valid: boolean;
  compatible_models: string[];
  file_path: string;
}

/**
 * A Kickstart-shaped file on the card, identified.
 *
 * `info === null` means the file could not be read at all — **not** "not a
 * known Kickstart", which comes back as a `RomInfo` whose version is
 * `"Custom"`. Unrecognised is a label, never a refusal.
 */
export interface CardRom {
  file_name: string;
  info: RomInfo | null;
}

export interface RomCopyOutcome {
  rom: CardRom;
  /** Where the file it replaced went. ART never deletes a ROM. */
  backup: string | null;
}

/** What the kernel on the card says about itself (F4). */
export interface KernelInfo {
  file_name: string;
  size_bytes: number;
  /** The `$VER:` line Emu68's own build stamped in. `null` is honest. */
  version: string | null;
}

/** Where a named firmware set's text comes from. */
export type ConfigSetSource = "current-config" | "set" | "screen-settings";

export interface ConfigSetPreview {
  file_name: string;
  before: string;
  after: string;
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
  kernel: KernelInfo | null;
  has_config_txt: boolean;
  has_cmdline_txt: boolean;
  kickstart_files: CardRom[];
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
  hardware: PistormHardware,
  line: Emu68Line
): Promise<HardwareNote[]> {
  return invoke<HardwareNote[]>("pistorm_hardware_notes", { hardware, line });
}

/** Identify a ROM anywhere on the machine, before deciding what to do with it. */
export async function pistormIdentifyRom(path: string): Promise<RomInfo> {
  return invoke<RomInfo>("pistorm_identify_rom", { path });
}

/** Whether a ROM suits the chosen machine. `null` when it has no opinion —
 *  and a note either way, never a block. */
export async function pistormRomSuits(
  info: RomInfo,
  amiga: AmigaTarget
): Promise<boolean | null> {
  return invoke<boolean | null>("pistorm_rom_suits", { info, amiga });
}

export async function pistormCopyRom(
  path: string,
  source: string,
  name: string,
  overwrite: boolean
): Promise<RomCopyOutcome> {
  return invoke<RomCopyOutcome>("pistorm_copy_rom", { path, source, name, overwrite });
}

export async function pistormPreviewConfigSet(
  path: string,
  name: string,
  source: ConfigSetSource,
  from: string | null,
  setup: PistormSetup
): Promise<ConfigSetPreview> {
  return invoke<ConfigSetPreview>("pistorm_preview_config_set", {
    path,
    name,
    source,
    from,
    setup,
  });
}

export async function pistormWriteConfigSet(
  path: string,
  name: string,
  source: ConfigSetSource,
  from: string | null,
  setup: PistormSetup
): Promise<string | null> {
  return invoke<string | null>("pistorm_write_config_set", {
    path,
    name,
    source,
    from,
    setup,
  });
}

export async function pistormRenameConfigSet(
  path: string,
  from: string,
  to: string
): Promise<void> {
  return invoke<void>("pistorm_rename_config_set", { path, from, to });
}

export async function pistormPreviewActivateSet(
  path: string,
  name: string
): Promise<ConfigSetPreview> {
  return invoke<ConfigSetPreview>("pistorm_preview_activate_set", { path, name });
}

export async function pistormActivateConfigSet(
  path: string,
  name: string
): Promise<string | null> {
  return invoke<string | null>("pistorm_activate_config_set", { path, name });
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
