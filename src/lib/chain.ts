// The sentences one link of the AmigaOS 3.9 update chain is allowed to
// produce (design § 2 of `2026-09-08-os-builder-chain-design.md`).
//
// `core::osinstall::chain` decides *what is true* of each row; this file
// decides *which sentence says it*, and nothing else. No `t()` here —
// `src/lib` never renders (CLAUDE.md) — so every row carries a `Phrase` and
// the component calls `t(phrase.key, phrase.params)`.
//
// **Seven endings, and they stay seven.** A row the tree already records,
// one whose file is in hand and whose turn it is, one waiting for something
// earlier, one whose file is not in the folders, one the material itself
// makes redundant, one ART will not do, and one nobody has measured are
// seven different next steps:
//
//   installed          nothing to do
//   ready              press Run
//   blocked            do the named row first
//   missing            go and get the named file
//   not needed         you already have what is in it
//   refused            here is why ART will not, and what to do instead
//   not yet runnable   ART has never driven this; here is what is unmeasured
//
// Collapsing any two of them produces this project's named defect: a
// confident sentence that sends somebody after a file they already have, or
// tells them to wait for something that has already happened.
//
// **The block sentence is not written twice.** A refused-because-unplaceable
// row reuses `hostPlacementBlockKey` — the same catalogue entry the Packages
// checklist shows under the same package. Two wordings for one fact is how
// the two screens would come to disagree about it.

import { hostPlacementBlockKey, type ChainReport, type ChainRow } from "@/lib/osinstall";
import type { Phrase } from "@/lib/phrase";

/** Which of the seven endings this row is. Kept on the object rather than
 *  derived from the phrase key, so a screen can style *refused* differently
 *  from *missing* without re-deciding which is which. */
export type ChainLineKind =
  | "installed"
  | "ready"
  | "blocked"
  | "missing"
  | "not-needed"
  | "refused"
  | "not-yet-runnable";

/** One rendered row of the chain screen. */
export interface ChainLine {
  /** A stable React key: the package's id, or the slot's for the CD row. */
  id: string;
  /** The rank the material gives this link. Two rows may share one. */
  position: number;
  /** The package's or medium's own name (ART-060). */
  name: string;
  kind: ChainLineKind;
  phrase: Phrase;
  /** The file involved, by name, or `null`. */
  file: string | null;
  /**
   * Where this row happens — *on the Amiga* or *placed from Windows* —
   * or `null` for a row that is neither: the CD, and a package ART can
   * neither place nor run.
   *
   * Its own field rather than part of the sentence, because it is true of
   * the row whatever state the row is in, and the design's own sketch shows
   * it as a separate fact after the state.
   */
  where: Phrase | null;
  /**
   * Whether **the** Run button belongs on this row.
   *
   * The design gives the screen one Run button, on the first row that is
   * ready, and it runs that row only. Decided here rather than in the
   * component so there is one answer to "which row is next" and a test can
   * ask it: a screen that put a button on every ready row would invite
   * somebody to start row 6 before row 4.
   */
  runnable: boolean;
}

/**
 * One line per chain row, in the order the rows arrive — which is the
 * material's own order, already sorted by `(position, id)` in Rust.
 *
 * **Not re-sorted here.** The order *is* information: it is the order the
 * material goes on, and a second sort would be a second answer to a question
 * that already has one.
 */
export function chainLines(rows: ChainRow[]): ChainLine[] {
  let runNext = true;
  return rows.map((row): ChainLine => {
    const name = row.name;
    const base = {
      id: row.packageId ?? row.slotId ?? `row-${row.position}`,
      position: row.position,
      name,
      file: row.facts.file,
      where: wherePhrase(row),
      runnable: false,
    };

    switch (row.state.state) {
      case "installed":
        return {
          ...base,
          kind: "installed" as const,
          // `when` is always null today — `distribution.json` records no
          // date — so the sentence carries none rather than a guess.
          phrase: { key: "osinstall.chain.installed", params: { name } },
        };

      case "ready": {
        // The one Run button, on the first ready row, and only there.
        const runnable = runNext;
        runNext = false;
        return {
          ...base,
          kind: "ready" as const,
          runnable,
          phrase: row.facts.file
            ? { key: "osinstall.chain.ready", params: { name, file: row.facts.file } }
            : { key: "osinstall.chain.readyUnnamed", params: { name } },
        };
      }

      case "blocked-by":
        return {
          ...base,
          kind: "blocked" as const,
          phrase: {
            key: "osinstall.chain.blocked",
            params: { name, needs: row.state.names.join(", ") },
          },
        };

      case "missing":
        return {
          ...base,
          kind: "missing" as const,
          // "Expected ." is a sentence nobody can act on, so a row ART has
          // no recorded file name for gets the shorter one — the same
          // choice `slotLines` makes for the same reason.
          phrase:
            row.state.expected.length > 0
              ? {
                  key: "osinstall.chain.missing",
                  params: { name, filenames: row.state.expected.join(", ") },
                }
              : { key: "osinstall.chain.missingUnnamed", params: { name } },
        };

      case "not-needed":
        return {
          ...base,
          kind: "not-needed" as const,
          phrase: {
            key: "osinstall.chain.notNeeded",
            params: { name, supersededBy: row.state.supersededBy },
          },
        };

      case "refused":
        return {
          ...base,
          kind: "refused" as const,
          phrase:
            row.state.reason.because === "ambiguous"
              ? {
                  key: "osinstall.chain.refusedAmbiguous",
                  params: {
                    name,
                    count: row.state.reason.candidates.length,
                    candidates: row.state.reason.candidates.join(", "),
                  },
                }
              : // The Packages checklist's own sentence for the same block,
                // not a second wording of it.
                { key: hostPlacementBlockKey(row.state.reason.block) },
        };

      case "not-yet-runnable":
        return {
          ...base,
          kind: "not-yet-runnable" as const,
          phrase: {
            key: "osinstall.chain.notYetRunnable",
            params: { name, reason: row.state.reason },
          },
        };
    }
  });
}

/** *runs on the Amiga* / *placed from Windows*, or nothing for a row that is
 *  neither. See {@link ChainLine.where}. */
function wherePhrase(row: ChainRow): Phrase | null {
  if (row.facts.runsOnAmiga === null) return null;
  return { key: row.facts.runsOnAmiga ? "osinstall.chain.onAmiga" : "osinstall.chain.onWindows" };
}

/**
 * The line above the rows — the design's `● 3 of 8 applied`.
 *
 * **A row the material makes redundant is named, never folded into the
 * fraction.** Counting Euro-Update as outstanding under an installed
 * BoingBags 3&4 would send somebody after a file whose contents they already
 * have; counting it as applied would claim ART did something it did not.
 */
export function chainSummaryLine(report: ChainReport): Phrase {
  const { release, total, installed, notNeeded } = report.summary;
  return notNeeded > 0
    ? {
        key: "osinstall.chain.setLineNotNeeded",
        params: { release, installed, total, notNeeded },
      }
    : { key: "osinstall.chain.setLine", params: { release, installed, total } };
}
