// How the PiStorm screen presents the Emu68 option inventory, and how the
// remembered copies of it are checked on the way back in.
//
// Split out of the screen for the usual reason: the rules here are the ones
// worth a test. **Which options are even visible** is one of them — the
// slow-RAM tokens are A500-family concepts, and the official Emu68 FAQ's answer
// to "my A1200 reports the wrong RAM" is to remove them, so showing them on an
// A1200 would be offering the user the bug. Rust drops them on the way out
// (`gated_for`); this stops them being offered in the first place.
//
// Every field carries its real token name, and the screen prints it beside the
// control. That is the mechanism that keeps ART-090 from happening again: a
// control with no token to show is a control with nothing behind it.

import {
  isFlag,
  isOneOf,
  isText,
  isWholeNumberBetween,
  type Guard,
} from "@/lib/remembered";
import type {
  AmigaChoice,
  AmigaTarget,
  DisplayMode,
  Emu68Options,
  FirmwareConfig,
  Overclock,
  PiModel,
  PistormHardware,
  PistormVariant,
  StorageExposure,
  VariantChoice,
} from "@/lib/pistorm";

/** "Either nothing, or this" — the shape of every optional token value. */
export function nullOr<T>(guard: Guard<T>): Guard<T | null> {
  return (value: unknown): value is T | null => value === null || guard(value);
}

export const HARDWARE_SPEC: { [K in keyof PistormHardware]: Guard<PistormHardware[K]> } = {
  amiga: isOneOf<AmigaTarget>("a500", "a1000", "a2000", "a600", "a1200"),
  variant: isOneOf<PistormVariant>("classic", "pistorm600", "pistorm16", "pistorm32-lite"),
  pi: isOneOf<PiModel>(
    "zero2-w",
    "pi3-a",
    "pi3-a-plus",
    "pi3-b",
    "pi3-b-plus",
    "pi4-b",
    "cm4"
  ),
};

/**
 * The bounds are the documented ones, not round numbers.
 *
 * `z2_ram_size` takes 0, 1, 2, 4 or 8; `cs_dist` runs 1 to 8; `copy_rom` is a
 * ROM size in KB. A value outside them came from a hand-edited settings file,
 * and writing it onto a card would hand a real machine something Emu68 has no
 * branch for.
 */
export const EMU68_OPTION_SPEC: { [K in keyof Emu68Options]: Guard<Emu68Options[K]> } = {
  storage_unit0: isOneOf<StorageExposure>("off", "read-only", "read-write"),
  storage_verbose: nullOr(isWholeNumberBetween(0, 2)),
  storage_low_speed: isFlag,
  storage_clock_mhz: nullOr(isWholeNumberBetween(1, 200)),
  vbr_move: isFlag,
  nofpu: isFlag,
  enable_cache: isFlag,
  limit_2g: isFlag,
  z2_ram_size: nullOr(isOneOfNumbers(0, 1, 2, 4, 8)),
  enable_c0_slow: isFlag,
  enable_c8_slow: isFlag,
  enable_d0_slow: isFlag,
  move_slow_to_chip: isFlag,
  copy_rom_kb: nullOr(isOneOfNumbers(256, 512, 1024, 2048)),
  checksum_rom: isFlag,
  vc4_mem_mb: nullOr(isWholeNumberBetween(1, 512)),
  chip_slowdown: isFlag,
  cs_dist: nullOr(isWholeNumberBetween(1, 8)),
  swap_df0_with: nullOr(isWholeNumberBetween(1, 3)),
  one_slot: isFlag,
  debug: isFlag,
  disassemble: isFlag,
  async_log: isFlag,
  fast_serial: isFlag,
  buptest_kb: nullOr(isWholeNumberBetween(1, 1024 * 1024)),
  bupiter: nullOr(isWholeNumberBetween(1, 1000)),
};

/** A number from a fixed set — the shape `z2_ram_size` and `copy_rom` take. */
export function isOneOfNumbers(...allowed: readonly number[]): Guard<number> {
  return (value: unknown): value is number =>
    typeof value === "number" && allowed.includes(value);
}

function isDisplayMode(value: unknown): value is DisplayMode {
  if (typeof value === "string") {
    return ["auto", "dmt1080p60", "cea1080p50", "cea720p60"].includes(value);
  }
  if (typeof value !== "object" || value === null) return false;
  const custom = (value as { custom?: unknown }).custom;
  if (typeof custom !== "object" || custom === null) return false;
  const { group, mode } = custom as { group?: unknown; mode?: unknown };
  return isWholeNumberBetween(0, 3)(group) && isWholeNumberBetween(0, 255)(mode);
}

function isOverclock(value: unknown): value is Overclock | null {
  if (value === null) return true;
  if (typeof value !== "object") return false;
  const { arm_freq_mhz, over_voltage, force_turbo } = value as Record<string, unknown>;
  return (
    isWholeNumberBetween(100, 4000)(arm_freq_mhz) &&
    isWholeNumberBetween(-16, 8)(over_voltage) &&
    isFlag(force_turbo)
  );
}

export const FIRMWARE_SPEC: { [K in keyof FirmwareConfig]: Guard<FirmwareConfig[K]> } = {
  kickstart_file: isText,
  display: isDisplayMode,
  overclock: isOverclock,
  disable_bluetooth: isFlag,
};

// ---------------------------------------------------------------------------
// The inventory, as the screen groups it
// ---------------------------------------------------------------------------

export interface OptionField {
  key: keyof Emu68Options;
  /**
   * The token as Emu68 spells it. `{prefix}` stands for `sd` or `emmc`,
   * because the storage tokens are named after how the Pi exposes its card.
   */
  token: string;
  kind: "flag" | "number" | "choice";
  choices?: string[];
}

export interface OptionGroup {
  id: string;
  fields: OptionField[];
}

const STORAGE: OptionGroup = {
  id: "storage",
  fields: [
    {
      key: "storage_unit0",
      token: "{prefix}.unit0",
      kind: "choice",
      choices: ["off", "read-only", "read-write"],
    },
    { key: "storage_verbose", token: "{prefix}.verbose", kind: "number" },
    { key: "storage_low_speed", token: "{prefix}.low_speed", kind: "flag" },
    { key: "storage_clock_mhz", token: "{prefix}.clock", kind: "number" },
  ],
};

const CPU: OptionGroup = {
  id: "cpu",
  fields: [
    { key: "vbr_move", token: "vbr_move", kind: "flag" },
    { key: "nofpu", token: "nofpu", kind: "flag" },
    { key: "enable_cache", token: "enable_cache", kind: "flag" },
  ],
};

const MEMORY_COMMON: OptionField[] = [
  { key: "limit_2g", token: "limit_2g", kind: "flag" },
  { key: "z2_ram_size", token: "z2_ram_size", kind: "number" },
];

const MEMORY_SLOW: OptionField[] = [
  { key: "enable_c0_slow", token: "enable_c0_slow", kind: "flag" },
  { key: "enable_c8_slow", token: "enable_c8_slow", kind: "flag" },
  { key: "enable_d0_slow", token: "enable_d0_slow", kind: "flag" },
  { key: "move_slow_to_chip", token: "move_slow_to_chip", kind: "flag" },
];

const ROM: OptionGroup = {
  id: "rom",
  fields: [
    { key: "copy_rom_kb", token: "copy_rom", kind: "number" },
    { key: "checksum_rom", token: "checksum_rom", kind: "flag" },
  ],
};

const RTG: OptionGroup = {
  id: "rtg",
  fields: [{ key: "vc4_mem_mb", token: "vc4.mem", kind: "number" }],
};

const TIMING: OptionGroup = {
  id: "timing",
  fields: [
    { key: "chip_slowdown", token: "chip_slowdown", kind: "flag" },
    { key: "cs_dist", token: "cs_dist", kind: "number" },
  ],
};

const FLOPPY: OptionGroup = {
  id: "floppy",
  fields: [{ key: "swap_df0_with", token: "swap_df0_with_dfN", kind: "number" }],
};

const DIAGNOSTICS: OptionGroup = {
  id: "diagnostics",
  fields: [
    { key: "debug", token: "debug", kind: "flag" },
    { key: "disassemble", token: "disassemble", kind: "flag" },
    { key: "async_log", token: "async_log", kind: "flag" },
    { key: "fast_serial", token: "fast_serial", kind: "flag" },
    { key: "buptest_kb", token: "buptest", kind: "number" },
    { key: "bupiter", token: "bupiter", kind: "number" },
  ],
};

/**
 * The groups worth showing for this hardware.
 *
 * Two things are hidden rather than shown-and-ignored, and both for the same
 * reason — an option that does nothing here is not information, it is a wrong
 * answer waiting to be given:
 *
 * - the slow-RAM tokens on anything but an A500-family machine;
 * - `one_slot`, which exists on the PiStorm32-lite alone.
 *
 * `amiga` and `variant` are passed as the matrix rows Rust returned rather than
 * re-derived here, so there is one table and it is the tested one.
 */
export function visibleOptionGroups(
  hardware: PistormHardware,
  amiga: AmigaChoice | undefined,
  variant: VariantChoice | undefined
): OptionGroup[] {
  const hasSlowRam = amiga?.has_slow_ram ?? false;
  const hasOneSlot = variant?.has_one_slot_option ?? false;

  const memory: OptionGroup = {
    id: "memory",
    fields: hasSlowRam ? [...MEMORY_COMMON, ...MEMORY_SLOW] : MEMORY_COMMON,
  };

  const groups: OptionGroup[] = [STORAGE, CPU, memory, ROM, RTG, TIMING, FLOPPY];

  if (hasOneSlot) {
    groups.push({
      id: "board",
      fields: [{ key: "one_slot", token: "one_slot", kind: "flag" }],
    });
  }

  groups.push(DIAGNOSTICS);
  void hardware;
  return groups;
}

/** Every option field ART knows, for the tests that check nothing is orphaned. */
export function allOptionFields(): OptionField[] {
  return [
    ...STORAGE.fields,
    ...CPU.fields,
    ...MEMORY_COMMON,
    ...MEMORY_SLOW,
    ...ROM.fields,
    ...RTG.fields,
    ...TIMING.fields,
    ...FLOPPY.fields,
    { key: "one_slot", token: "one_slot", kind: "flag" },
    ...DIAGNOSTICS.fields,
  ];
}
