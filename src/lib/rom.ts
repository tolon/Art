// Typed wrappers for Kickstart ROM commands.

import { invoke } from "@tauri-apps/api/core";

import type { Phrase } from "@/lib/phrase";

/** The three answers `core/rom` gives about a ROM's stored checksum. */
export type RomChecksum = "valid" | "invalid" | "not-checked";

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
  /** What ART can honestly say about this file's integrity (ART-138).
   *  `"not-checked"` is not a failure: an accelerator's ROM, or a licensed
   *  dump with no `rom.key` beside it, carries no Kickstart checksum for ART
   *  to verify — and calling that `CRC ERR` claimed damage ART cannot see. */
  checksum: RomChecksum;
  compatible_models: string[];
  file_path: string;
}

export async function romIdentify(path: string): Promise<RomInfo> {
  return invoke<RomInfo>("rom_identify", { path });
}

export async function romScanDir(dirPath: string): Promise<RomInfo[]> {
  return invoke<RomInfo[]>("rom_scan_dir", { dirPath });
}

/** The badge a ROM's checksum earns: a tone class and the sentence for it. */
export interface ChecksumBadge {
  tone: string;
  phrase: Phrase;
}

/**
 * What ART may claim about a ROM file's integrity — the whole of ART-138 in
 * one function.
 *
 * `"not-checked"` is **not** a fault. An accelerator's boot ROM keeps no
 * Kickstart checksum, and an encrypted Amiga Forever dump with no `rom.key`
 * beside it cannot be read at all. Both used to render as `CRC ERR`, which
 * says a file is damaged — a claim ART has no basis for, and which it made
 * about 46 of the 76 files in this project's own ROM folder.
 */
export function checksumBadge(
  rom: Pick<RomInfo, "checksum" | "is_cloanto" | "key_available">,
): ChecksumBadge {
  switch (rom.checksum) {
    case "valid":
      return { tone: "badge-ok", phrase: { key: "common.ok" } };
    case "invalid":
      return { tone: "badge-warn", phrase: { key: "rom.badges.crcErr" } };
    default:
      return rom.is_cloanto && !rom.key_available
        ? { tone: "badge-muted", phrase: { key: "rom.badges.encrypted" } }
        : { tone: "badge-muted", phrase: { key: "rom.badges.notKickstart" } };
  }
}
