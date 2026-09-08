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
// one that is simply absent, one the user chose by hand, one whose chosen file
// has gone, and one ART has measured as unnecessary are eight different
// states with eight different next steps. They never collapse into "not
// found" or into "found".
//
// **And "ART did not look" is not "ART looked and found nothing", which is
// not "ART looked and these bytes are something else"** (fix round 1's F1,
// then the re-review's F13). `osinstallSlots` hashes nothing — it reads the
// scan cache the identify job fills — so a folder nobody has identified yet
// produces rank-2 and rank-3 rows with *nothing at all known about the bytes*.
// Every such row picks between **three** sentences on `bytesRead`: the lookup
// has not happened and here is what to do about it; the lookup happened and
// matched nothing; the lookup happened and the bytes are a *different*
// catalogued artefact — a relabelled disk, about which "in no table ART has"
// is simply false. The first version of this file had one sentence for all
// three, and it was the one that asserted a fact about a table nobody had
// asked.
//
// The `installed` badge is separate from all of them on purpose: it comes
// from the tree's own `distribution.json` and says nothing about whether the
// file is still in a folder — and the folder says nothing about whether it
// was ever installed.

import {
  fileName,
  type BytesRead,
  type Installed,
  type SetSummary,
  type SlotState,
} from "@/lib/osinstall";
import type { Phrase } from "@/lib/phrase";

/**
 * Which of the eight endings this row is.
 *
 * Kept on the object rather than derived from the phrase key, so a screen can
 * style a guess differently from a find without re-deciding which is which.
 * `found-by-name` and `guessed-by-filename` each carry three possible
 * sentences — see the file header — because the *ending* is the same and only
 * the reason ART cannot say more differs.
 */
export type SlotLineKind =
  | "found-by-hash"
  | "found-by-name"
  | "chosen"
  | "chosen-missing"
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
  /**
   * Every candidate's **full path**, for an ambiguous row.
   *
   * Paths and not file names (fix round 1, F3). The ambiguity this row exists
   * for is one artefact in two folders, and its commonest shape is two copies
   * under the *same* name: rendering names gave the user
   * "BoingBag39-1.lha, BoingBag39-1.lha" and nothing to pick between. The
   * folder is the whole of the information here.
   */
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
 * That is two data values placed side by side, not a translated sentence. A
 * release that states no floor (`rom-older-than` only) leaves `identity`
 * empty and the row simply says "Kickstart".
 */
export function displayName(state: SlotState): string {
  const { slot } = state;
  if (slot.kind === "rom" && slot.identity) return `${slot.name} ${slot.identity}`;
  return slot.name;
}

/**
 * The sentence a row about a **matched-by-its-own-name** file gets, chosen by
 * what is actually known about its bytes (F1, F13).
 *
 * Three keys, never two: the middle one is the only one entitled to say the
 * bytes are in no table ART has. `read-row` at this rank means a row claims
 * these bytes for a *different* artefact — rank 1 filters by the slot's own
 * artefact and returns before rank 2 is reached — so the sentence names what
 * the table says they are instead.
 */
function foundByNameKey(bytes: BytesRead): string {
  switch (bytes.state) {
    case "not-read":
      return "osinstall.slots.foundByNameUnread";
    case "read-no-row":
      return "osinstall.slots.foundByName";
    case "read-row":
      return "osinstall.slots.foundByNameOtherArtefact";
  }
}

/** The same three-way choice for the *guess* rank. */
function guessedByFilenameKey(bytes: BytesRead): string {
  switch (bytes.state) {
    case "not-read":
      return "osinstall.slots.guessedByFilenameUnread";
    case "read-no-row":
      return "osinstall.slots.guessedByFilename";
    case "read-row":
      return "osinstall.slots.guessedByFilenameOtherArtefact";
  }
}

/** What the table calls the artefact these bytes actually are, or `""` when
 *  the row is not the one being rendered. A parameter the two
 *  `…OtherArtefact` sentences interpolate and the other four ignore. */
function otherArtefactName(bytes: BytesRead): string {
  return bytes.state === "read-row" ? bytes.name : "";
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
  // A slot id is ART's own bookkeeping. "needs package:boingbag-39-1,
  // medium:AmigaOS3.9 first" is the most-rendered sentence in the readout
  // before a tree is chosen, and it was rendering ids at the user (fix round
  // 1, F7). Every state is in hand, so the name is one lookup away.
  const names = new Map(states.map((state) => [state.slot.id, displayName(state)]));

  return [...states]
    .sort((a, b) => a.slot.position - b.slot.position)
    .map((state): SlotLine => {
      const name = displayName(state);
      const line = {
        id: state.slot.id,
        name,
        required: state.slot.required,
        candidates: state.candidates.map((candidate) => candidate.path),
        installed: installedPhrase(state.installed),
        blocked:
          state.blockedBy.length > 0
            ? {
                key: "osinstall.slots.blocked",
                params: {
                  needs: state.blockedBy.map((id) => names.get(id) ?? id).join(", "),
                },
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

      // A file the user named that is not on disk. Not *chosen*, which would
      // describe a file that is not there, and not *not found*, which would
      // say nothing about the choice they already made.
      if (state.chosenMissing) {
        return {
          ...line,
          kind: "chosen-missing" as const,
          file: fileName(state.chosenMissing),
          phrase: {
            key: "osinstall.slots.chosenMissing",
            params: { name, path: state.chosenMissing },
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
                key: foundByNameKey(state.found.bytesRead),
                params: {
                  file,
                  name,
                  identity: state.slot.identity,
                  other: otherArtefactName(state.found.bytesRead),
                },
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
                key: guessedByFilenameKey(state.found.bytesRead),
                params: { file, name, other: otherArtefactName(state.found.bytesRead) },
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
        const [candidate] = state.candidates;
        const file = fileName(candidate.path);
        return {
          ...line,
          kind: "guessed-by-filename" as const,
          file,
          phrase: {
            key: guessedByFilenameKey(candidate.bytesRead),
            params: { file, name, other: otherArtefactName(candidate.bytesRead) },
          },
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

/** One candidate of an ambiguous slot, with the sentence that candidate alone
 *  is entitled to. */
export interface CandidateLine {
  /** The candidate's **full path** — the whole of the information, since the
   *  commonest ambiguity here is two copies under the same file name (fix
   *  round 1, F3). Also the value a screen offering a choice sets. */
  path: string;
  phrase: Phrase;
}

/**
 * One line per candidate of an ambiguous slot.
 *
 * `slotLines` says *that* a slot is ambiguous and lists the paths; this says
 * what is known about each candidate on its own, so a screen offering the
 * choice can put the evidence beside each option rather than asking the user
 * to pick between two paths and nothing else. The three sentences are the
 * *guess* rank's own — which is exactly what a candidate is: a file that
 * might be this artefact, and about which ART is either silent, or says the
 * table does not know its bytes, or says its bytes are some other catalogued
 * artefact entirely. Reusing those keys rather than writing a fourth set is
 * deliberate: the situation is the same one, and two catalogues saying it
 * twice is how they come to say it differently.
 */
export function candidateLines(state: SlotState): CandidateLine[] {
  const name = displayName(state);
  return state.candidates.map((candidate) => ({
    path: candidate.path,
    phrase: {
      key: guessedByFilenameKey(candidate.bytesRead),
      params: {
        file: fileName(candidate.path),
        name,
        other: otherArtefactName(candidate.bytesRead),
      },
    },
  }));
}

/**
 * The set line above the rows — *"AmigaOS 3.9 · 5 of 6 found · 1 required
 * missing"* — plus whether it may be shown as ready.
 *
 * **Required and optional are counted apart in `SetSummary`, and the totals
 * here keep them apart too**: a set missing only optional files is ready to
 * build, and one fraction cannot say that. `ready` is `false` the moment a
 * required slot is missing, which is the only thing the colour is allowed to
 * mean.
 *
 * **The not-needed count is on the line for a reason.** A slot ART has
 * measured as unnecessary leaves *both* totals (`slots::summarize`), so the
 * denominator legitimately shrinks as ART learns more — "5 of 6" becomes
 * "5 of 5" between two calls, and with nothing else on the line that reads as
 * material vanishing. Saying "1 not needed" beside the counts is what makes
 * the smaller denominator an answer rather than a disappearance. It is said
 * only when there is one: "0 not needed" is noise about nothing.
 */
export function setLine(
  summary: SetSummary,
  states: SlotState[]
): { phrase: Phrase; ready: boolean } {
  const found = summary.requiredFound + summary.optionalFound;
  const total = summary.requiredTotal + summary.optionalTotal;
  const missingRequired = summary.requiredTotal - summary.requiredFound;
  const notNeeded = states.filter((state) => state.notNeeded !== null).length;
  const params = {
    release: summary.release,
    found,
    total,
    missingRequired,
    notNeeded,
  };
  return {
    ready: missingRequired === 0,
    phrase: {
      key: notNeeded > 0 ? "osinstall.slots.setLineNotNeeded" : "osinstall.slots.setLine",
      params,
    },
  };
}

/**
 * One line per material folder ART could not read at all.
 *
 * **Its own sentence, and not a row of the readout** (fix round 1, F2, whose
 * field this finally renders). A remembered path on a drive nobody plugged in
 * is the ordinary case here, and every slot resolved against the *other*
 * folders is still true — so this says what was not counted rather than
 * casting doubt over the rows. Empty is the normal answer and renders
 * nothing: "0 folders could not be read" is a sentence about nothing.
 */
export function unreadableFolderLines(
  folders: string[]
): { folder: string; phrase: Phrase }[] {
  return folders.map((folder) => ({
    folder,
    phrase: { key: "osinstall.slots.unreadableFolder", params: { folder } },
  }));
}

/**
 * What the readout says while it is working.
 *
 * **A count, never a bar** (CLAUDE.md: a progress bar showing a fixed width
 * with no total looks like progress and carries none). `osinstall_slots` is
 * one round trip over the whole folder list and reports no progress of its
 * own, so the honest statement is how many folders it was given — not a
 * fraction ART cannot fill in, and not a moving sliver that means nothing.
 */
export function readoutRunningLine(folders: number): Phrase {
  return { key: "osinstall.slots.reading", params: { count: folders } };
}
