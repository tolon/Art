import { describe, expect, it } from "vitest";

import {
  allOptionFields,
  EMU68_OPTION_SPEC,
  FIRMWARE_SPEC,
  HARDWARE_SPEC,
  isOneOfNumbers,
  nullOr,
  visibleOptionGroups,
} from "@/lib/pistormOptions";
import { isFlag } from "@/lib/remembered";
import {
  DEFAULT_EMU68_OPTIONS,
  DEFAULT_FIRMWARE_CONFIG,
  DEFAULT_HARDWARE,
  type AmigaChoice,
  type Emu68Options,
  type PistormHardware,
  type VariantChoice,
} from "@/lib/pistorm";

function amigaRow(hasSlowRam: boolean): AmigaChoice {
  return { amiga: "a500", name: "Amiga 500", has_slow_ram: hasSlowRam, variants: [] };
}

function variantRow(hasOneSlot: boolean): VariantChoice {
  return {
    variant: "classic",
    name: "PiStorm",
    kernel_archive: "Emu68-pistorm.zip",
    has_one_slot_option: hasOneSlot,
    pi_models: [],
  };
}

function fieldKeys(hardware: PistormHardware, slow: boolean, oneSlot: boolean): string[] {
  return visibleOptionGroups(hardware, amigaRow(slow), variantRow(oneSlot))
    .flatMap((group) => group.fields)
    .map((field) => field.key);
}

describe("visibleOptionGroups", () => {
  it("offers the slow-RAM tokens on a machine that has slow RAM", () => {
    const keys = fieldKeys(DEFAULT_HARDWARE, true, false);
    for (const key of [
      "enable_c0_slow",
      "enable_c8_slow",
      "enable_d0_slow",
      "move_slow_to_chip",
    ]) {
      expect(keys).toContain(key);
    }
  });

  it("does not offer them on a machine that has none", () => {
    // Not merely useless there: the Emu68 FAQ's own answer to "my A1200 reports
    // the wrong RAM" is to remove these, so offering them would be offering the
    // user the bug.
    const keys = fieldKeys({ amiga: "a1200", variant: "pistorm32-lite", pi: "cm4" }, false, true);
    for (const key of [
      "enable_c0_slow",
      "enable_c8_slow",
      "enable_d0_slow",
      "move_slow_to_chip",
    ]) {
      expect(keys).not.toContain(key);
    }
  });

  it("keeps the memory options that apply everywhere", () => {
    const keys = fieldKeys({ amiga: "a1200", variant: "pistorm32-lite", pi: "cm4" }, false, true);
    expect(keys).toContain("limit_2g");
    expect(keys).toContain("z2_ram_size");
  });

  it("offers one_slot only on the board that has it", () => {
    expect(fieldKeys(DEFAULT_HARDWARE, true, false)).not.toContain("one_slot");
    expect(
      fieldKeys({ amiga: "a1200", variant: "pistorm32-lite", pi: "cm4" }, false, true)
    ).toContain("one_slot");
  });

  it("is safe when the matrix has not arrived yet", () => {
    // The screen renders before the command answers. An empty inventory is
    // fine; a crash is not.
    const groups = visibleOptionGroups(DEFAULT_HARDWARE, undefined, undefined);
    expect(groups.length).toBeGreaterThan(0);
    expect(groups.flatMap((g) => g.fields).map((f) => f.key)).not.toContain("move_slow_to_chip");
  });
});

describe("the option inventory", () => {
  it("gives every field a real token name", () => {
    // The mechanism that keeps ART-090 from coming back: a control with no
    // token to print beside it is a control with nothing behind it.
    for (const field of allOptionFields()) {
      expect(field.token.length).toBeGreaterThan(0);
      expect(field.token).not.toMatch(/^emu68\.(jit|mmu)$/);
      expect(field.token).not.toBe("buptest.fastram_size");
    }
  });

  it("covers every option the engine has, and invents none", () => {
    const inventory = new Set(allOptionFields().map((field) => field.key));
    const engine = new Set(Object.keys(DEFAULT_EMU68_OPTIONS) as (keyof Emu68Options)[]);
    expect([...engine].filter((key) => !inventory.has(key))).toEqual([]);
    expect([...inventory].filter((key) => !engine.has(key))).toEqual([]);
  });

  it("names the storage tokens after how the Pi exposes its card", () => {
    // `sd.*` on a Pi 3, `emmc.*` on a Pi 4 or CM4 — one placeholder rather than
    // two lists that can disagree.
    const storage = allOptionFields().filter((field) => field.key.startsWith("storage_"));
    expect(storage.length).toBeGreaterThan(0);
    for (const field of storage) {
      expect(field.token).toMatch(/^\{prefix\}\./);
    }
  });
});

describe("the remembered-value guards", () => {
  it("check every field of every shape", () => {
    // A shape with a missing guard silently accepts anything for that field,
    // which is the one failure mode this whole layer exists to prevent.
    expect(Object.keys(EMU68_OPTION_SPEC).sort()).toEqual(
      Object.keys(DEFAULT_EMU68_OPTIONS).sort()
    );
    expect(Object.keys(FIRMWARE_SPEC).sort()).toEqual(
      Object.keys(DEFAULT_FIRMWARE_CONFIG).sort()
    );
    expect(Object.keys(HARDWARE_SPEC).sort()).toEqual(Object.keys(DEFAULT_HARDWARE).sort());
  });

  it("accept the defaults ART ships with", () => {
    for (const [key, guard] of Object.entries(EMU68_OPTION_SPEC)) {
      const value = DEFAULT_EMU68_OPTIONS[key as keyof Emu68Options];
      expect(guard(value), `${key} = ${String(value)}`).toBe(true);
    }
    for (const [key, guard] of Object.entries(FIRMWARE_SPEC)) {
      expect(guard(DEFAULT_FIRMWARE_CONFIG[key as keyof typeof DEFAULT_FIRMWARE_CONFIG])).toBe(
        true
      );
    }
    for (const [key, guard] of Object.entries(HARDWARE_SPEC)) {
      expect(guard(DEFAULT_HARDWARE[key as keyof PistormHardware])).toBe(true);
    }
  });

  it("refuse a value the engine has no branch for", () => {
    // 3 MB of Zorro II RAM is not a smaller 4: Emu68 takes 0, 1, 2, 4 or 8.
    expect(EMU68_OPTION_SPEC.z2_ram_size(3)).toBe(false);
    expect(EMU68_OPTION_SPEC.z2_ram_size(4)).toBe(true);
    expect(EMU68_OPTION_SPEC.z2_ram_size(null)).toBe(true);

    // `cs_dist` runs 1 to 8.
    expect(EMU68_OPTION_SPEC.cs_dist(0)).toBe(false);
    expect(EMU68_OPTION_SPEC.cs_dist(9)).toBe(false);
    expect(EMU68_OPTION_SPEC.cs_dist(4)).toBe(true);

    // A ROM is one of four sizes.
    expect(EMU68_OPTION_SPEC.copy_rom_kb(768)).toBe(false);
    expect(EMU68_OPTION_SPEC.copy_rom_kb(1024)).toBe(true);
  });

  it("refuse a storage exposure that is not one of the three", () => {
    expect(EMU68_OPTION_SPEC.storage_unit0("rw")).toBe(false);
    expect(EMU68_OPTION_SPEC.storage_unit0("read-write")).toBe(true);
  });

  it("read a display mode back, custom numbers included", () => {
    expect(FIRMWARE_SPEC.display("dmt1080p60")).toBe(true);
    expect(FIRMWARE_SPEC.display({ custom: { group: 2, mode: 16 } })).toBe(true);
    expect(FIRMWARE_SPEC.display({ custom: { group: 2 } })).toBe(false);
    expect(FIRMWARE_SPEC.display("1080p")).toBe(false);
  });

  it("treat a missing overclock as an answer and a broken one as absent", () => {
    expect(FIRMWARE_SPEC.overclock(null)).toBe(true);
    expect(
      FIRMWARE_SPEC.overclock({ arm_freq_mhz: 1400, over_voltage: 4, force_turbo: true })
    ).toBe(true);
    // No frequency is not an overclock, whatever else is set.
    expect(FIRMWARE_SPEC.overclock({ over_voltage: 4, force_turbo: true })).toBe(false);
  });

  it("repair only what is broken", () => {
    // A hardware choice from an older ART falls back on its own without taking
    // the other two with it.
    expect(HARDWARE_SPEC.variant("a1200lite")).toBe(false);
    expect(HARDWARE_SPEC.variant("pistorm32-lite")).toBe(true);
    expect(HARDWARE_SPEC.amiga("a1200")).toBe(true);
  });
});

describe("nullOr and isOneOfNumbers", () => {
  it("nullOr admits nothing, and what it wraps", () => {
    const guard = nullOr(isFlag);
    expect(guard(null)).toBe(true);
    expect(guard(true)).toBe(true);
    expect(guard(undefined)).toBe(false);
    expect(guard(0)).toBe(false);
  });

  it("isOneOfNumbers takes the list and nothing near it", () => {
    const guard = isOneOfNumbers(0, 1, 2, 4, 8);
    expect(guard(8)).toBe(true);
    expect(guard(3)).toBe(false);
    expect(guard("8")).toBe(false);
  });
});
