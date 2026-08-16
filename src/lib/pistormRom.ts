// How a Kickstart on a PiStorm card is described, and what may be done with it.
//
// The name was never an answer (F1). A card's `kick.rom` can be a 1.3 image, a
// 3.1 image, a byte-swapped one, or a text file somebody renamed — and ART used
// to write whichever name it found into `initramfs` and say nothing at all.
// `core/rom` identifies by checksum; this turns that verdict into the sentence
// the screen shows.
//
// Pure, and separate from the screen, because the rule that matters is a rule
// about *honesty*: unrecognised is a label, never a refusal, and this is where
// that can be pinned by a test.

import type { Phrase } from "@/lib/phrase";
import type { CardRom, RomInfo } from "@/lib/pistorm";

/**
 * What to say about one ROM on the card.
 *
 * Three answers, and the middle one is the one that matters: a ROM ART does not
 * recognise is still a ROM the user may want, so it is labelled and left
 * usable.
 */
export function describeRom(rom: CardRom): Phrase {
  if (!rom.info) {
    return { key: "pistorm.rom.unreadable", params: { name: rom.file_name } };
  }
  if (rom.info.version === "Custom") {
    return { key: "pistorm.rom.unrecognised", params: { name: rom.file_name } };
  }
  return {
    key: "pistorm.rom.identified",
    params: {
      name: rom.file_name,
      rom: rom.info.name,
      revision: rom.info.revision,
      models: rom.info.compatible_models.join(", "),
    },
  };
}

/**
 * The name to offer when copying a ROM onto the card.
 *
 * Its own name, because that is what the user recognises — and because a card
 * may sensibly carry several. The path separators are Windows' and Unix's
 * alike: the dialog returns whichever the platform uses.
 */
export function suggestedRomName(sourcePath: string): string {
  const cut = Math.max(sourcePath.lastIndexOf("\\"), sourcePath.lastIndexOf("/"));
  const name = cut >= 0 ? sourcePath.slice(cut + 1) : sourcePath;
  return name.trim() || "kick.rom";
}

/**
 * Whether a name is one the card can take.
 *
 * A plain file name: no folders, no traversal. Rust refuses these too — this is
 * so the dialog can say why before the user presses the button, rather than
 * after.
 */
export function isUsableRomName(name: string): boolean {
  const trimmed = name.trim();
  if (trimmed.length === 0 || trimmed.length > 64) return false;
  if (trimmed.includes("/") || trimmed.includes("\\")) return false;
  if (trimmed === "." || trimmed === "..") return false;
  // Windows forbids these outright, and a FAT32 card is read by Windows.
  // Whitespace is refused too — not because a card cannot hold it, but
  // because this name is written into a `config.txt` directive, where a
  // space ends the value.
  const forbidden = ['<', '>', ':', '"', '|', '?', '*'];
  if (forbidden.some((character) => trimmed.includes(character))) return false;
  return !/\s/.test(name);
}

/**
 * What to say about a ROM's suitability for the chosen machine.
 *
 * `null` in, nothing out: an unrecognised ROM has no opinion attached, and
 * inventing one would be the very thing this round is fixing. Even a mismatch
 * is a note — people boot 1.3 on an A1200 on purpose.
 */
export function romSuitabilityNote(
  suits: boolean | null,
  info: RomInfo | null,
  machine: string
): Phrase | null {
  if (suits === null || !info) return null;
  if (suits) return null;
  return {
    key: "pistorm.rom.mayNotSuit",
    params: { rom: info.name, machine, models: info.compatible_models.join(", ") },
  };
}
