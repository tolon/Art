import { describe, expect, it } from "vitest";

import {
  describeRom,
  isUsableRomName,
  romSuitabilityNote,
  suggestedRomName,
} from "@/lib/pistormRom";
import type { CardRom, RomInfo } from "@/lib/pistorm";

function info(overrides: Partial<RomInfo> = {}): RomInfo {
  return {
    name: "Kickstart 3.1",
    version: "3.1",
    revision: "40.63",
    size_bytes: 524288,
    sha256: "",
    crc32: "",
    is_cloanto: false,
    is_aros: false,
    checksum_valid: true,
    compatible_models: ["A500", "A600", "A2000"],
    file_path: "",
    ...overrides,
  };
}

describe("describeRom", () => {
  it("names the Kickstart, its revision and what it is for", () => {
    const rom: CardRom = { file_name: "kick31.rom", info: info() };
    expect(describeRom(rom)).toEqual({
      key: "pistorm.rom.identified",
      params: {
        name: "kick31.rom",
        rom: "Kickstart 3.1",
        revision: "40.63",
        models: "A500, A600, A2000",
      },
    });
  });

  it("labels a ROM it does not recognise, and only labels it", () => {
    // The file may be byte-swapped, custom, or newer than ART's table. It is
    // still the user's ROM — unrecognised is a label, never a refusal.
    const rom: CardRom = {
      file_name: "mystery.rom",
      info: info({ version: "Custom", name: "Custom / Unknown ROM Image" }),
    };
    expect(describeRom(rom).key).toBe("pistorm.rom.unrecognised");
  });

  it("tells a file it could not read from one it could not recognise", () => {
    // Two different situations with two different remedies, and ART used to
    // have no words for either.
    expect(describeRom({ file_name: "broken.rom", info: null }).key).toBe(
      "pistorm.rom.unreadable"
    );
  });
});

describe("suggestedRomName", () => {
  it("offers the file's own name", () => {
    expect(suggestedRomName("F:\\roms\\kick31.rom")).toBe("kick31.rom");
    expect(suggestedRomName("/home/me/kick31.rom")).toBe("kick31.rom");
    expect(suggestedRomName("kick31.rom")).toBe("kick31.rom");
  });

  it("falls back to something usable rather than to nothing", () => {
    expect(suggestedRomName("")).toBe("kick.rom");
    expect(suggestedRomName("F:\\roms\\")).toBe("kick.rom");
  });
});

describe("isUsableRomName", () => {
  it("takes a plain file name", () => {
    expect(isUsableRomName("kick.rom")).toBe(true);
    expect(isUsableRomName("kick31_A600.rom")).toBe(true);
  });

  it("refuses anything that is a path rather than a name", () => {
    // Rust refuses these too. This is so the dialog can say why *before* the
    // button is pressed rather than after.
    for (const name of ["../kick.rom", "..\\kick.rom", "sub/kick.rom", "C:\\kick.rom", "..", "."]) {
      expect(isUsableRomName(name), name).toBe(false);
    }
  });

  it("refuses characters a FAT32 card cannot carry", () => {
    // The card is read by Windows and by an Amiga; a name with a `?` in it is
    // a file neither of them can open.
    for (const name of ["kick?.rom", "kick*.rom", 'kick".rom', "kick|.rom", "kick .rom"]) {
      expect(isUsableRomName(name), name).toBe(false);
    }
  });

  it("refuses nothing at all, and something absurdly long", () => {
    expect(isUsableRomName("")).toBe(false);
    expect(isUsableRomName("   ")).toBe(false);
    expect(isUsableRomName("k".repeat(100))).toBe(false);
  });
});

describe("romSuitabilityNote", () => {
  it("says nothing when the ROM suits the machine", () => {
    expect(romSuitabilityNote(true, info(), "Amiga 500")).toBeNull();
  });

  it("says nothing about a ROM it does not recognise", () => {
    // Inventing an opinion about an unidentified file is the exact thing this
    // round exists to stop.
    expect(romSuitabilityNote(null, info({ version: "Custom" }), "Amiga 1200")).toBeNull();
  });

  it("notes a mismatch, naming what the ROM is actually for", () => {
    const note = romSuitabilityNote(false, info(), "Amiga 1200");
    expect(note?.key).toBe("pistorm.rom.mayNotSuit");
    expect(note?.params).toMatchObject({ machine: "Amiga 1200", models: "A500, A600, A2000" });
  });
});
