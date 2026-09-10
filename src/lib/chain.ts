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
// **The block is decided once and said twice.** A refused-because-unplaceable
// row goes through `hostPlacementBlockChainKey` — the same `switch` over the
// same `HostPlacementBlock` the Packages checklist uses, one key family
// further on. Two *decisions* about one fact is how two screens come to
// disagree; two *sentences* is what a row in a list of nine needs and a row
// sitting under its own package's name must not have (fix round 1, F3).

import {
  hostPlacementBlockChainKey,
  notYetRunnableChainKey,
  type BlockedComponent,
  type ChainReport,
  type ChainRow,
} from "@/lib/osinstall";
import type { Phrase } from "@/lib/phrase";

/** Which of the seven endings this row is. Kept on the object rather than
 *  derived from the phrase key, so a screen can style *refused* differently
 *  from *missing* without re-deciding which is which. */
export type ChainLineKind =
  | "installed"
  | "ready"
  | "blocked"
  | "blocked-component"
  | "missing"
  | "not-needed"
  | "overtaken"
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
  /**
   * The components this row is waiting on, for `blocked-component` and empty
   * for every other kind (M4).
   *
   * The **names are not resolved here**: each carries an i18n key, and this
   * module never renders (`CLAUDE.md`'s "src/lib never renders one"). The
   * screen translates each key and joins them, exactly as it does for every
   * other catalogue phrase.
   */
  components: BlockedComponent[];
  /**
   * Whether this row is the medium — the CD, not a package.
   *
   * On the object rather than derived from `packageId === null` at each use
   * site, for the reason `kind` is: the screen already asks it twice (the
   * `kaynak` link, and the sentence a `missing` medium row gets) and two
   * hand-written derivations of one fact is how they come to disagree.
   */
  isMedium: boolean;
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
      file: row.sentenceFacts.file,
      where: wherePhrase(row),
      runnable: false,
      components: [],
      isMedium: row.packageId === null,
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
        // **The one Run button, on the first ready row that is a package**
        // (round 3 whole-branch review, M3). The medium row can no longer
        // read `ready` at all — `chain::medium_state` has two answers now —
        // but the guard stays here as well, because this is where "which row
        // is next" is decided and the answer must never be a row the Run
        // button cannot fire on: `request` needs a package id, so a disc
        // would leave *"Next: AmigaOS3.9"* over a dead button while the
        // BoingBag below it, which really is ready, was never offered. The
        // CD row's own action is the `kaynak` link beside it.
        //
        // `runNext` is only spent when it is actually taken, so a medium row
        // passes the button on rather than swallowing it.
        const runnable = runNext && row.packageId !== null;
        if (runnable) {
          runNext = false;
        }
        return {
          ...base,
          kind: "ready" as const,
          runnable,
          phrase: row.sentenceFacts.file
            ? { key: "osinstall.chain.ready", params: { name, file: row.sentenceFacts.file } }
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

      case "blocked-by-component":
        return {
          ...base,
          kind: "blocked-component" as const,
          components: row.state.components,
          // The sentence needs the component's *translated* name, and this
          // module does not translate — so the phrase is completed by the
          // screen, which fills `components` in from the field above. The
          // key is chosen here, because which sentence this row gets is a
          // decision and not a rendering: "the tree was not built with X" is
          // a different next step from "do row 4 first", and folding it into
          // `chain.blocked` would send somebody looking for a row that is
          // not in the list.
          phrase: {
            key: "osinstall.chain.blockedComponent",
            params: { name, components: "" },
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

      // The owner's finding of 2026-09-10: not *not needed* — the row carries
      // files nothing else does — and not *ready*, because the newer update
      // already in the tree would have its files written over.
      case "overtaken-by":
        return {
          ...base,
          kind: "overtaken" as const,
          phrase: {
            key: "osinstall.chain.overtaken",
            params: { name, names: row.state.names.join(", ") },
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
              : // **The block's own sentence, in this row's own words**
                // (round 3 task 2, fix round 1, F3). It used to be the
                // Packages checklist's key — one wording for one fact,
                // which was the right instinct and the wrong key: that
                // sentence renders *under the package's own name* on the
                // checklist and so names no package. In a list of nine
                // rows it left the one row the owner's tree will actually
                // show — Euro-Update, refused for `needs-fixfonts` on
                // every tree without BoingBags 3&4 — beginning "This
                // package replaces the bitmap fonts…" with eight
                // candidates above and below it. One block, two keys, the
                // same `switch`: `notYetRunnableChainKey`'s shape exactly.
                {
                  key: hostPlacementBlockChainKey(row.state.reason.block),
                  params: { name },
                },
        };

      case "not-yet-runnable":
        return {
          ...base,
          kind: "not-yet-runnable" as const,
          // One key per reason, so the whole sentence is translated rather
          // than a translated frame around an English clause (fix round 1,
          // m6) — the same shape `hostPlacementBlockKey` already has.
          phrase: { key: notYetRunnableChainKey(row.state.reason), params: { name } },
        };
    }
  });
}

/**
 * *runs on the Amiga* / *placed from Windows*, or nothing for a row that is
 * neither. See {@link ChainLine.where}.
 *
 * **The precedence, decided in Rust and only rendered here** — `runsOnAmiga`
 * comes from `core::osinstall::chain`, where the three-way `match` and the
 * reasoning live. It is repeated in one sentence because this is the file a
 * reader reaches for when the row says the wrong thing:
 *
 *   - **no `hostPlacementBlock`** → `false`, ART places the files from
 *     Windows, and `AmigaInstallPanel`'s Run goes through
 *     `useHostPlacement`. Both BoingBags are here since 2026-09-08, and
 *     neither declares an `amigaInstaller` any more (2026-09-09): the
 *     emulator route for them was removed, not merely bypassed.
 *   - **a block and an `amigaInstaller`** → `true`, an emulator run through
 *     `compose` (BoingBags 3&4).
 *   - **a block and no installer** → `null`, neither route (Euro-Update).
 *
 * The block decides; the installer only breaks the tie. Reading the
 * installer first — which is what this used to do — would have gone on
 * offering a ~140 s emulator run, needing a ROM and a licence, for work the
 * host now does in seconds.
 */
function wherePhrase(row: ChainRow): Phrase | null {
  const where = row.sentenceFacts.runsOnAmiga;
  if (where === null) return null;
  return { key: where ? "osinstall.chain.onAmiga" : "osinstall.chain.onWindows" };
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

/**
 * How one chain row is drawn on the choice tab (four-tab design § 3.2).
 *
 * **Three tick states, and the third is the only one the user owns.**
 * `on` is a row the tree already carries; `off` is a row that cannot go on
 * this build whatever the user wants; `user` is a row whose box means what a
 * box means. The reason travels with the two decided states so a screen can
 * say *why* a box is dead — and it never carries the sentence, which is
 * `line.phrase`'s job and only ever `chain.rs`'s decision.
 *
 * **Why a function and not four `&&`s in the JSX.** The 2026-08 design's own
 * precedence rule — installed, then unmeasured, then unplaceable, then the
 * Amiga route, then the rest — is exactly the kind of ordered decision this
 * project has twice found spread across a component as independent guards,
 * where a fifth state falls through every one of them and renders a tick
 * nobody decided. Here a fifth `ChainLineKind` reaches the last line and
 * comes back `user`, which is a decision, and the arms above it are ordered
 * once, in one place, with a test each.
 */
export type ChoiceRowState =
  | { tick: "on"; enabled: false; reason: "installed" }
  | {
      tick: "off";
      enabled: false;
      reason:
        | "not-yet-runnable"
        | "not-placeable"
        | "runs-on-amiga"
        | "missing"
        | "not-needed"
        | "overtaken";
    }
  | { tick: "user"; enabled: true };

/**
 * {@link ChoiceRowState} for one row.
 *
 * Both halves are needed and neither is derivable from the other: `line`
 * carries the ending, `row` carries the two facts the ending does not —
 * which refusal it is (`RefusedBecause`), and where the row would run.
 */
export function choiceRowState(line: ChainLine, row: ChainRow): ChoiceRowState {
  // **Installed first, above everything.** It is a fact about the tree, read
  // out of `distribution.json`, and every arm below it is a fact about the
  // route or the material. A row the tree carries is ticked whatever else is
  // true of it — reading the route first would untick a BoingBag that is
  // already in, which is the screen out-claiming the core in the direction
  // that loses work.
  if (line.kind === "installed") {
    return { tick: "on", enabled: false, reason: "installed" };
  }

  // Above `runs-on-amiga`, which is also true of these rows: "nobody has
  // measured this installer" is the more specific fact and the one with a
  // next step in it.
  if (line.kind === "not-yet-runnable") {
    return { tick: "off", enabled: false, reason: "not-yet-runnable" };
  }

  // A refusal has two answers and they are not the same offer. *Unplaceable*
  // is ART saying it will not do this at all; *ambiguous* is a question the
  // user answers on tab 1, so that row stays theirs.
  //
  // **And tab 1 really does ask it now** (round 4 whole-branch review, C1).
  // This sentence was written before there was any such control: the material
  // readout listed an ambiguous row's candidates and offered nothing to pick
  // one with, so a ticked-enabled row here reached `runnablePath`, matched
  // nothing it trusts, and became tab 4's *unresolved* warning pointing at a
  // tab that chose nothing — three screens agreeing on a next step that did
  // not exist. `MaterialReadout` renders one *Use this* button per candidate
  // and `FilesTab` writes the override, which is what makes leaving the row
  // enabled the right answer rather than a dead tick.
  if (
    line.kind === "refused" &&
    row.state.state === "refused" &&
    row.state.reason.because === "not-placeable"
  ) {
    return { tick: "off", enabled: false, reason: "not-placeable" };
  }

  // **The wizard has one route** (spec § 3.2): there is no route control on
  // this tab, so a row whose only route is an emulator run cannot be offered
  // here — including a `ready` one, which is precisely the state that would
  // otherwise draw a tick the wizard cannot honour. `null` — neither route —
  // is *not* this arm: such a row is already refused above.
  if (row.sentenceFacts.runsOnAmiga === true) {
    return { tick: "off", enabled: false, reason: "runs-on-amiga" };
  }

  if (line.kind === "missing") {
    return { tick: "off", enabled: false, reason: "missing" };
  }
  if (line.kind === "not-needed") {
    return { tick: "off", enabled: false, reason: "not-needed" };
  }
  // Adding it would put older files over newer ones, and `add_package`
  // refuses exactly that — so the box is not offered (2026-09-10).
  if (line.kind === "overtaken") {
    return { tick: "off", enabled: false, reason: "overtaken" };
  }

  // `ready`, `blocked` and `blocked-component` — and the ambiguous refusal.
  // A blocked row is **later**, not impossible: the run orders it after what
  // it needs and refuses by name if that is unticked (ART-282's typed
  // refusal). Drawing it disabled would say the row cannot happen at all.
  return { tick: "user", enabled: true };
}
