// The seven endings a chain row can produce, and the two things that have to
// be true of every one of them: it is its own ending, and its sentence is in
// **both** catalogues.
//
// A `Phrase` whose key is not a leaf in `en.json` renders the raw dotted key
// on screen; one that is in `en.json` and not in `tr.json` renders English to
// a Turkish user. Neither fails to compile, and `parity.test.ts` compares the
// catalogues to each other rather than to this file — so the key check is
// made here, the way `slots.test.ts` makes it for the material readout.

import { describe, expect, it } from "vitest";

import en from "@/i18n/en.json";
import tr from "@/i18n/tr.json";
import {
  chainLines,
  chainSummaryLine,
  choiceRowState,
  type ChainLineKind,
  type ChoiceRowState,
} from "@/lib/chain";
import type { ChainReport, ChainRow, ChainState } from "@/lib/osinstall";

/** Whether `dotted` names a string leaf in `catalogue`. */
function isLeafKey(catalogue: unknown, dotted: string): boolean {
  let node: unknown = catalogue;
  for (const part of dotted.split(".")) {
    if (typeof node !== "object" || node === null) return false;
    node = (node as Record<string, unknown>)[part];
  }
  return typeof node === "string";
}

function leafText(catalogue: unknown, dotted: string): string {
  let node: unknown = catalogue;
  for (const part of dotted.split(".")) {
    node = (node as Record<string, unknown>)[part];
  }
  if (typeof node !== "string") throw new Error(`${dotted} is not a string leaf`);
  return node;
}

function row(over: Partial<ChainRow> & { state: ChainState }): ChainRow {
  return {
    position: 2,
    packageId: "boingbag-39-1",
    slotId: "package:boingbag-39-1",
    name: "BoingBag 3.9-1",
    sentenceFacts: { file: null, runsOnAmiga: true },
    ...over,
  };
}

/** One row of each of the seven states, in the order the design lists them. */
const EVERY_STATE: ChainRow[] = [
  row({ state: { state: "installed", when: null } }),
  row({
    packageId: "boingbag-39-2",
    name: "BoingBag 3.9-2",
    sentenceFacts: { file: "BoingBag39-2.lha", runsOnAmiga: true },
    state: { state: "ready" },
  }),
  row({
    packageId: "boingbags-39-3-4",
    name: "BoingBags 3&4",
    state: { state: "blocked-by", names: ["BoingBag 3.9-2"] },
  }),
  row({
    packageId: "locale-39",
    name: "Locale 3.9",
    sentenceFacts: { file: null, runsOnAmiga: false },
    state: { state: "missing", expected: ["Locale3_9.lha"] },
  }),
  row({
    packageId: "euro-update",
    name: "Euro-Update",
    sentenceFacts: { file: null, runsOnAmiga: null },
    state: { state: "not-needed", supersededBy: "BoingBags 3&4" },
  }),
  row({
    packageId: "locale-turkish",
    name: "Türkçe catalogs",
    state: {
      state: "refused",
      reason: { because: "ambiguous", candidates: ["D:/a/x.lha", "D:/b/x.lha"] },
    },
  }),
  row({
    packageId: "boingbags-39-3-4",
    name: "BoingBags 3&4",
    state: { state: "not-yet-runnable", reason: "installer-not-measured" },
  }),
  row({
    packageId: "locale-39",
    name: "Locale 3.9",
    sentenceFacts: { file: "Locale3_9.lha", runsOnAmiga: false },
    state: {
      state: "blocked-by-component",
      components: [{ id: "locale-base", labelKey: "osinstall.components.name.os39.locale" }],
    },
  }),
  // The owner's finding of 2026-09-10: a newer update already in the tree
  // writes over this one, so adding it now would put older files over newer.
  row({
    packageId: "locale-39-turkish",
    name: "Türkçe catalogs and fonts (Locale 3.9)",
    sentenceFacts: { file: "Locale3_9.lha", runsOnAmiga: false },
    state: { state: "overtaken-by", names: ["Türkçe catalogs (BoingBag 3.9-2)"] },
  }),
];

describe("chainLines", () => {
  it("gives each of the nine endings its own kind and its own key", () => {
    const lines = chainLines(EVERY_STATE);
    const kinds = lines.map((line) => line.kind);
    expect(new Set(kinds).size).toBe(9);
    expect(kinds).toEqual([
      "installed",
      "ready",
      "blocked",
      "missing",
      "not-needed",
      "refused",
      "not-yet-runnable",
      // Round 3 whole-branch review, M4. Its own ending and not `blocked`,
      // because the next step is somewhere else: every name in `blocked` is
      // another row on this screen, and a component is not.
      "blocked-component",
      "overtaken",
    ] satisfies ChainLineKind[]);

    // Distinct keys, so no two endings can render the same sentence.
    const keys = lines.map((line) => line.phrase.key);
    expect(new Set(keys).size).toBe(9);
  });

  // **The Run button never lands on the CD row** (round 3 whole-branch
  // review, M3). `chain::medium_state` no longer answers `ready`, and this
  // is the other half of the guard: even if it did, "which row is next" is
  // decided here and the answer must be a row the button can fire on —
  // `request` needs a package id, so a disc leaves a dead button and the
  // ready package below it unoffered.
  it("passes the Run button over a ready medium row to the first ready package", () => {
    const lines = chainLines([
      row({ position: 1, packageId: null, slotId: "medium:AmigaOS3.9", name: "AmigaOS3.9", state: { state: "ready" } }),
      row({ packageId: "boingbag-39-1", state: { state: "ready" } }),
      row({ packageId: "boingbag-39-2", state: { state: "ready" } }),
    ]);
    expect(lines.map((line) => line.runnable)).toEqual([false, true, false]);
    expect(lines[0].isMedium).toBe(true);
    expect(lines[1].isMedium).toBe(false);
  });

  // And a chain whose only ready row is the medium offers nothing at all,
  // rather than a button that cannot fire.
  it("offers no Run at all when the only ready row is the medium", () => {
    const lines = chainLines([
      row({ position: 1, packageId: null, slotId: "medium:AmigaOS3.9", name: "AmigaOS3.9", state: { state: "ready" } }),
      row({ packageId: "boingbag-39-1", state: { state: "missing", expected: ["BoingBag39-1.lha"] } }),
    ]);
    expect(lines.some((line) => line.runnable)).toBe(false);
  });

  // The component's name is **not** resolved here: this module does not
  // render, so the key travels and the screen translates it.
  it("carries the blocking components rather than a rendered name", () => {
    const [line] = chainLines([
      row({
        state: {
          state: "blocked-by-component",
          components: [{ id: "locale-base", labelKey: "osinstall.components.name.os39.locale" }],
        },
      }),
    ]);
    expect(line.kind).toBe("blocked-component");
    expect(line.components).toEqual([
      { id: "locale-base", labelKey: "osinstall.components.name.os39.locale" },
    ]);
    expect(line.runnable).toBe(false);
    for (const catalogue of [en, tr]) {
      expect(leafText(catalogue, line.phrase.key)).toContain("{{components}}");
    }
  });

  it("puts every sentence it can produce in both catalogues", () => {
    const rows: ChainRow[] = [
      ...EVERY_STATE,
      // The two fallbacks the ordinary fixtures do not reach.
      row({ state: { state: "ready" }, sentenceFacts: { file: null, runsOnAmiga: true } }),
      row({ state: { state: "missing", expected: [] } }),
      // And the other refusal, which borrows the Packages checklist's own
      // sentence rather than writing a second one.
      row({
        state: { state: "refused", reason: { because: "not-placeable", block: "needs-fixfonts" } },
      }),
      row({
        state: {
          state: "refused",
          reason: { because: "not-placeable", block: "needs-installer-script" },
        },
      }),
      row({
        state: {
          state: "refused",
          reason: { because: "not-placeable", block: "encrypted-payload" },
        },
      }),
    ];
    for (const line of chainLines(rows)) {
      expect(isLeafKey(en, line.phrase.key), `${line.phrase.key} in en`).toBe(true);
      expect(isLeafKey(tr, line.phrase.key), `${line.phrase.key} in tr`).toBe(true);
      if (line.where) {
        expect(isLeafKey(en, line.where.key), `${line.where.key} in en`).toBe(true);
        expect(isLeafKey(tr, line.where.key), `${line.where.key} in tr`).toBe(true);
      }
    }
  });

  it("says a refused-because-unplaceable row in words that name the row", () => {
    // **Round 3 task 2, fix round 1, F3.** This used to assert the
    // *checklist's* key, `osinstall.packages.blocked.needsFixfonts` — one
    // wording for one fact, which was the right instinct and the wrong key.
    // That sentence renders directly under the package's own name on the
    // checklist, so it names no package; in a list of nine rows it left
    // Euro-Update — the state every tree without BoingBags 3&4 is in —
    // beginning "This package replaces the bitmap fonts…" with eight
    // candidates above and below it. The decision is still made once (the
    // same `switch` over `HostPlacementBlock`); only the sentence differs.
    const [line] = chainLines([
      row({
        name: "Euro-Update",
        state: { state: "refused", reason: { because: "not-placeable", block: "needs-fixfonts" } },
      }),
    ]);
    expect(line.phrase.key).toBe("osinstall.chain.refusedNotPlaceable.needsFixfonts");
    expect(line.phrase.params).toEqual({ name: "Euro-Update" });
    // The row names itself, and it still says what the block actually is.
    const said = leafText(en, line.phrase.key);
    expect(said).toMatch(/\{\{name\}\}/);
    expect(said).toMatch(/FixFonts|\.font/);
  });

  it("gives every block its own chain sentence, in both catalogues", () => {
    // The three blocks the recipes ship. A fourth is a compile error in
    // `hostPlacementBlockSuffix`, and this is what stops it arriving with
    // one of its three sentences missing.
    for (const block of ["encrypted-payload", "needs-fixfonts", "needs-installer-script"] as const) {
      const [line] = chainLines([
        row({ state: { state: "refused", reason: { because: "not-placeable", block } } }),
      ]);
      expect(isLeafKey(en, line.phrase.key), `${line.phrase.key} in en`).toBe(true);
      expect(isLeafKey(tr, line.phrase.key), `${line.phrase.key} in tr`).toBe(true);
      // Every one of them names the row: that is the whole of F3.
      expect(leafText(en, line.phrase.key), line.phrase.key).toMatch(/\{\{name\}\}/);
      expect(leafText(tr, line.phrase.key), line.phrase.key).toMatch(/\{\{name\}\}/);
    }
  });

  // **One Run button, on the first ready row.** A button on every ready row
  // would invite somebody to start row 6 before row 4, which is the order
  // this whole screen exists to keep.
  it("marks only the first ready row as the one to run", () => {
    const lines = chainLines([
      row({ packageId: "a", state: { state: "installed", when: null } }),
      row({ packageId: "b", state: { state: "ready" } }),
      row({ packageId: "c", state: { state: "ready" } }),
    ]);
    expect(lines.map((line) => line.runnable)).toEqual([false, true, false]);
  });

  it("never marks a row runnable when nothing is ready", () => {
    const lines = chainLines([
      row({ packageId: "a", state: { state: "blocked-by", names: ["x"] } }),
      row({ packageId: "b", state: { state: "missing", expected: [] } }),
    ]);
    expect(lines.some((line) => line.runnable)).toBe(false);
  });

  // The blocked row names the **rows**, never a slot id: "needs
  // package:boingbag-39-1 first" is ART's own bookkeeping read out at a user.
  it("names what a blocked row waits for by name", () => {
    const [line] = chainLines([
      row({ state: { state: "blocked-by", names: ["BoingBag 3.9-1", "AmigaOS3.9"] } }),
    ]);
    expect(line.phrase.params?.needs).toBe("BoingBag 3.9-1, AmigaOS3.9");
    for (const catalogue of [en, tr]) {
      expect(leafText(catalogue, line.phrase.key)).not.toMatch(/package:/);
    }
  });

  // "Expected ." is a sentence nobody can act on.
  it("drops the file list from a missing row when ART has no name for it", () => {
    const [named, unnamed] = chainLines([
      row({ state: { state: "missing", expected: ["BoingBag39-1.lha"] } }),
      row({ state: { state: "missing", expected: [] } }),
    ]);
    expect(named.phrase.key).toBe("osinstall.chain.missing");
    expect(named.phrase.params?.filenames).toBe("BoingBag39-1.lha");
    expect(unnamed.phrase.key).toBe("osinstall.chain.missingUnnamed");
    for (const catalogue of [en, tr]) {
      expect(leafText(catalogue, unnamed.phrase.key)).not.toMatch(/\{\{filenames\}\}/);
    }
  });

  // A *not needed* row names the package that made it so, and never tells
  // anybody to go and find the file.
  it("says which package makes a row unnecessary, and does not ask for the file", () => {
    const [line] = chainLines([
      row({ state: { state: "not-needed", supersededBy: "BoingBags 3&4" } }),
    ]);
    expect(line.phrase.params?.supersededBy).toBe("BoingBags 3&4");
    for (const catalogue of [en, tr]) {
      const text = leafText(catalogue, line.phrase.key);
      expect(text).not.toMatch(/\{\{filenames\}\}/);
    }
  });

  // Three answers to "where does this happen", and the third is silence.
  it("keeps 'on the Amiga', 'placed from Windows' and 'neither' apart", () => {
    const lines = chainLines([
      row({ sentenceFacts: { file: null, runsOnAmiga: true }, state: { state: "ready" } }),
      row({ sentenceFacts: { file: null, runsOnAmiga: false }, state: { state: "ready" } }),
      row({ sentenceFacts: { file: null, runsOnAmiga: null }, state: { state: "ready" } }),
    ]);
    expect(lines.map((line) => line.where?.key ?? null)).toEqual([
      "osinstall.chain.onAmiga",
      "osinstall.chain.onWindows",
      null,
    ]);
  });

  // The order is the material's, and it arrives sorted from Rust.
  it("renders the rows in the order it is given", () => {
    const lines = chainLines(EVERY_STATE);
    expect(lines.map((line) => line.name)).toEqual(EVERY_STATE.map((r) => r.name));
  });
});

describe("chainSummaryLine", () => {
  const report = (installed: number, notNeeded: number): ChainReport => ({
    rows: [],
    summary: { release: "AmigaOS 3.9", total: 9, installed, notNeeded },
    unreadableFolders: [],
    crowdedFolders: [],
  });

  it("names the redundant rows apart rather than folding them into the fraction", () => {
    const plain = chainSummaryLine(report(3, 0));
    expect(plain.key).toBe("osinstall.chain.setLine");
    for (const catalogue of [en, tr]) {
      expect(leafText(catalogue, plain.key)).not.toMatch(/\{\{notNeeded\}\}/);
    }

    const some = chainSummaryLine(report(3, 1));
    expect(some.key).toBe("osinstall.chain.setLineNotNeeded");
    expect(some.params?.notNeeded).toBe(1);
    for (const catalogue of [en, tr]) {
      expect(leafText(catalogue, some.key)).toMatch(/\{\{notNeeded\}\}/);
    }
  });

  it("puts both set lines in both catalogues", () => {
    for (const line of [chainSummaryLine(report(0, 0)), chainSummaryLine(report(1, 2))]) {
      expect(isLeafKey(en, line.key)).toBe(true);
      expect(isLeafKey(tr, line.key)).toBe(true);
    }
  });
});

describe("choiceRowState", () => {
  // How the choice tab (four-tab design § 3.2) draws one chain row: which of
  // three tick states it is in, and — when it is not the user's to decide —
  // which single reason it is not.
  //
  // **The tick never carries the sentence.** Every one of these rows already
  // says what it is in `line.phrase`; this decides only whether the box is
  // on, off, or the user's, so a row can never be drawn tickable while its
  // own sentence says it cannot run.
  //
  // One case per `ChainLineKind`, plus the two refusals apart (they are the
  // one kind with two answers) and the `runsOnAmiga` arm, which cuts across
  // the kinds rather than being one of them.

  /** The state of the one line `chainLines` makes from `state`. */
  function stateOf(over: Partial<ChainRow> & { state: ChainState }): ChoiceRowState {
    const built = row(over);
    return choiceRowState(chainLines([built])[0], built);
  }

  it("ticks an installed row and takes it out of the user's hands", () => {
    // On, because it *is* on: the tree records it. Disabled, because
    // unticking it would offer to un-install something ART cannot un-install.
    expect(stateOf({ state: { state: "installed", when: null } })).toEqual({
      tick: "on",
      enabled: false,
      reason: "installed",
    });
  });

  it("leaves a ready row to the user", () => {
    expect(
      stateOf({ state: { state: "ready" }, sentenceFacts: { file: "BB1.lha", runsOnAmiga: false } })
    ).toEqual({ tick: "user", enabled: true });
  });

  it("leaves a blocked row tickable — the run orders it and refuses by name", () => {
    // Spec § 3.2: *a `BlockedBy` row can be ticked; the run orders it after
    // what it needs and refuses if that is unticked, naming both.* Drawing
    // it disabled would make the list say the row is impossible when it is
    // merely later.
    expect(
      stateOf({
        state: { state: "blocked-by", names: ["BoingBag 3.9-1"] },
        sentenceFacts: { file: "BB2.lha", runsOnAmiga: false },
      })
    ).toEqual({ tick: "user", enabled: true });
  });

  it("leaves a row blocked on a component tickable, for the same reason", () => {
    expect(
      stateOf({
        state: {
          state: "blocked-by-component",
          components: [{ id: "locale-base", labelKey: "osinstall.components.name.os39.locale" }],
        },
        sentenceFacts: { file: "Locale3_9.lha", runsOnAmiga: false },
      })
    ).toEqual({ tick: "user", enabled: true });
  });

  it("leaves a row refused as ambiguous tickable, so the refusal is the run's", () => {
    // Two candidate files is a question the user answers on tab 1 by naming
    // one. The row stays in the list, ticked or not as they left it, and the
    // run refuses with the row's own sentence rather than the list quietly
    // dropping it.
    expect(
      stateOf({
        state: {
          state: "refused",
          reason: { because: "ambiguous", candidates: ["D:/a/x.lha", "D:/b/x.lha"] },
        },
        sentenceFacts: { file: null, runsOnAmiga: false },
      })
    ).toEqual({ tick: "user", enabled: true });
  });

  it("refuses the tick for a row ART cannot place at all", () => {
    expect(
      stateOf({
        state: { state: "refused", reason: { because: "not-placeable", block: "needs-fixfonts" } },
        sentenceFacts: { file: "Euro.lha", runsOnAmiga: null },
      })
    ).toEqual({ tick: "off", enabled: false, reason: "not-placeable" });
  });

  it("refuses the tick for a row nobody has measured", () => {
    expect(
      stateOf({
        state: { state: "not-yet-runnable", reason: "installer-not-measured" },
        sentenceFacts: { file: "BB34.lha", runsOnAmiga: true },
      })
    ).toEqual({ tick: "off", enabled: false, reason: "not-yet-runnable" });
    // And **not** `runs-on-amiga`, which is also true of it: the more
    // specific reason is the one that says what is actually unknown.
  });

  it("never offers a row that runs on the Amiga — the wizard has one route", () => {
    // The precedence rule of the 2026-08 design § 3.2 with the Amiga route
    // removed: there is no route control on this tab, so a row whose only
    // route is the emulator is off and disabled whatever else it is —
    // including `ready`, which is the state that would otherwise draw a tick
    // the wizard cannot honour.
    expect(
      stateOf({
        state: { state: "ready" },
        sentenceFacts: { file: "BB34.lha", runsOnAmiga: true },
      })
    ).toEqual({ tick: "off", enabled: false, reason: "runs-on-amiga" });
  });

  it("refuses the tick for a row whose file is not here", () => {
    expect(
      stateOf({
        state: { state: "missing", expected: ["Locale3_9.lha"] },
        sentenceFacts: { file: null, runsOnAmiga: false },
      })
    ).toEqual({ tick: "off", enabled: false, reason: "missing" });
  });

  it("refuses the tick for a row a newer update already in the tree writes over", () => {
    expect(
      stateOf({
        state: { state: "overtaken-by", names: ["Türkçe catalogs (BoingBag 3.9-2)"] },
        sentenceFacts: { file: "Locale3_9.lha", runsOnAmiga: false },
      })
    ).toEqual({ tick: "off", enabled: false, reason: "overtaken" });
  });

  it("refuses the tick for a row the material itself makes redundant", () => {
    expect(
      stateOf({
        state: { state: "not-needed", supersededBy: "BoingBags 3&4" },
        sentenceFacts: { file: null, runsOnAmiga: null },
      })
    ).toEqual({ tick: "off", enabled: false, reason: "not-needed" });
  });

  it("keeps an installed row ticked even when it runs on the Amiga", () => {
    // The order of the two arms, pinned: *installed* is a fact about the
    // tree and *runs on the Amiga* is a fact about the route. Reading the
    // route first would untick a row the tree already carries — the screen
    // out-claiming the core in the direction that loses work.
    expect(
      stateOf({
        state: { state: "installed", when: null },
        sentenceFacts: { file: null, runsOnAmiga: true },
      })
    ).toEqual({ tick: "on", enabled: false, reason: "installed" });
  });
});
