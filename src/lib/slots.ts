// The sentences a resolved slot is allowed to produce (design §3.3).
//
// `core::osinstall::slots` decides *what is true* about each artefact; this
// file decides *which sentence says it*, and nothing else. No `t()` here —
// `src/lib` never renders (CLAUDE.md) — so every row carries a `Phrase` and
// the component calls `t(phrase.key, phrase.params)`.
//
// **The endings stay distinct, and that is the whole point of the file.**
// This project's most expensive defects are not crashes; they are one
// confident sentence covering several different situations. A slot ART
// identified by its bytes, one that merely calls itself the right thing, one
// that only has the right *file name*, two files ART will not choose between,
// one that is simply absent, one the user chose by hand and one ART has
// measured as unnecessary are seven different states with seven different
// next steps. They never collapse into "not found" or into "found".
//
// The `installed` badge is separate from all of them on purpose: it comes
// from the tree's own `distribution.json` and says nothing about whether the
// file is still in a folder — and the folder says nothing about whether it
// was ever installed.

import { fileName, type Installed, type SlotState } from "@/lib/osinstall";
import type { Phrase } from "@/lib/phrase";

/**
 * Which of the seven endings this row is.
 *
 * Kept on the object rather than derived from the phrase key, so a screen can
 * style a guess differently from a find without re-deciding which is which.
 */
export type SlotLineKind =
  | "found-by-hash"
  | "found-by-name"
  | "chosen"
  | "guessed-by-filename"
  | "ambiguous"
  | "not-found"
  | "not-needed";

/** One row of the material readout. */
export interface SlotLine {
  /** The slot's own id — a stable React key, and what `blockedBy` names. */
  id: string;
  kind: SlotLineKind;
  /** What to call the artefact. The recipe's own name (ART-060), except a
   *  Kickstart, which the recipe names only by the major it needs. */
  name: string;
  required: boolean;
  /** The file's own name, when one file is involved. `null` otherwise. */
  file: string | null;
  /** Every candidate's file name, for an ambiguous row. */
  candidates: string[];
  phrase: Phrase;
  /** The installed badge, from the manifest alone. `null` when the manifest
   *  says nothing — never rendered as "not installed", which is a claim about
   *  a tree that may not even have been chosen. */
  installed: Phrase | null;
  /** What this row waits for, when something is in the way. */
  blocked: Phrase | null;
}

/**
 * How the recipe names this artefact.
 *
 * A ROM is the one slot whose `name` is generic — the recipe states a
 * Kickstart *floor*, not a Kickstart — so the major it asks for is joined on.
 * That is two data values placed side by side, not a translated sentence.
 */
function displayName(state: SlotState): string {
  const { slot } = state;
  if (slot.kind === "rom" && slot.identity) return `${slot.name} ${slot.identity}`;
  return slot.name;
}

function installedPhrase(installed: Installed): Phrase | null {
  switch (installed.state) {
    case "ran":
      // The command line the tree recorded, verbatim — an Amiga installer is
      // a program ART did not write and cannot account for per file, so what
      // is honestly known is that it ran and said it worked.
      return { key: "osinstall.slots.installedRan", params: { command: installed.command } };
    case "placed":
      return { key: "osinstall.slots.installedPlaced" };
    case "no":
      return null;
  }
}

/**
 * One line per slot, in chain order (media, then the packages in the order
 * they install with each overlay right after its package, then the ROM).
 *
 * Sorted by the slot's own `position` rather than by the order the backend
 * happened to answer in: the order *is* information here — it is the order
 * the material goes on — and a readout that re-sorted it alphabetically would
 * be throwing that away.
 */
export function slotLines(states: SlotState[]): SlotLine[] {
  return [...states]
    .sort((a, b) => a.slot.position - b.slot.position)
    .map((state): SlotLine => {
      const name = displayName(state);
      const line = {
        id: state.slot.id,
        name,
        required: state.slot.required,
        candidates: state.candidates.map(fileName),
        installed: installedPhrase(state.installed),
        blocked:
          state.blockedBy.length > 0
            ? {
                key: "osinstall.slots.blocked",
                params: { needs: state.blockedBy.join(", ") },
              }
            : null,
      };

      // Measured as unnecessary, and that outranks everything below: telling
      // somebody a file is missing when ART has just proved nobody has to
      // obtain it is the confident-wrong sentence this project is most
      // expensive at.
      if (state.notNeeded) {
        return {
          ...line,
          kind: "not-needed" as const,
          file: null,
          phrase: {
            key: "osinstall.slots.notNeeded",
            params: { name, carries: state.notNeeded },
          },
        };
      }

      if (state.found) {
        const file = fileName(state.found.path);
        switch (state.found.matchedBy) {
          case "hash":
            return {
              ...line,
              kind: "found-by-hash" as const,
              file,
              phrase: { key: "osinstall.slots.foundByHash", params: { file, name } },
            };
          case "volume-name":
          case "top-level-directory":
            return {
              ...line,
              kind: "found-by-name" as const,
              file,
              phrase: {
                key: "osinstall.slots.foundByName",
                params: { file, name, identity: state.slot.identity },
              },
            };
          case "chosen":
            return {
              ...line,
              kind: "chosen" as const,
              file,
              phrase: { key: "osinstall.slots.chosen", params: { file, name } },
            };
          case "filename":
            // Unreachable through the resolver, which never lets a name fill
            // `found` — kept because a `matchedBy` the switch did not handle
            // would otherwise fall out of the readout entirely, and a row
            // that silently is not there is worse than a cautious one.
            return {
              ...line,
              kind: "guessed-by-filename" as const,
              file,
              phrase: {
                key: "osinstall.slots.guessedByFilename",
                params: { file, name },
              },
            };
        }
      }

      if (state.candidates.length > 1) {
        return {
          ...line,
          kind: "ambiguous" as const,
          file: null,
          phrase: {
            key: "osinstall.slots.ambiguous",
            params: {
              name,
              count: state.candidates.length,
              candidates: line.candidates.join(", "),
            },
          },
        };
      }

      if (state.candidates.length === 1) {
        const file = fileName(state.candidates[0]);
        return {
          ...line,
          kind: "guessed-by-filename" as const,
          file,
          phrase: { key: "osinstall.slots.guessedByFilename", params: { file, name } },
        };
      }

      // Not found. The expected names are said only when ART actually has
      // some — "expected " with nothing after it is the sentence a user
      // cannot act on.
      return {
        ...line,
        kind: "not-found" as const,
        file: null,
        phrase:
          state.slot.filenames.length > 0
            ? {
                key: "osinstall.slots.notFound",
                params: { name, filenames: state.slot.filenames.join(", ") },
              }
            : { key: "osinstall.slots.notFoundUnnamed", params: { name } },
      };
    });
}
