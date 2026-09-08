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
import { chainLines, chainSummaryLine, type ChainLineKind } from "@/lib/chain";
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
    facts: { file: null, runsOnAmiga: true },
    ...over,
  };
}

/** One row of each of the seven states, in the order the design lists them. */
const EVERY_STATE: ChainRow[] = [
  row({ state: { state: "installed", when: null } }),
  row({
    packageId: "boingbag-39-2",
    name: "BoingBag 3.9-2",
    facts: { file: "BoingBag39-2.lha", runsOnAmiga: true },
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
    facts: { file: null, runsOnAmiga: false },
    state: { state: "missing", expected: ["Locale3_9.lha"] },
  }),
  row({
    packageId: "euro-update",
    name: "Euro-Update",
    facts: { file: null, runsOnAmiga: null },
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
    state: { state: "not-yet-runnable", reason: "nobody has run it" },
  }),
];

describe("chainLines", () => {
  it("gives each of the seven endings its own kind and its own key", () => {
    const lines = chainLines(EVERY_STATE);
    const kinds = lines.map((line) => line.kind);
    expect(new Set(kinds).size).toBe(7);
    expect(kinds).toEqual([
      "installed",
      "ready",
      "blocked",
      "missing",
      "not-needed",
      "refused",
      "not-yet-runnable",
    ] satisfies ChainLineKind[]);

    // Distinct keys, so no two endings can render the same sentence.
    const keys = lines.map((line) => line.phrase.key);
    expect(new Set(keys).size).toBe(7);
  });

  it("puts every sentence it can produce in both catalogues", () => {
    const rows: ChainRow[] = [
      ...EVERY_STATE,
      // The two fallbacks the ordinary fixtures do not reach.
      row({ state: { state: "ready" }, facts: { file: null, runsOnAmiga: true } }),
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

  it("says a refused-because-unplaceable row with the Packages checklist's own words", () => {
    const [line] = chainLines([
      row({
        state: { state: "refused", reason: { because: "not-placeable", block: "needs-fixfonts" } },
      }),
    ]);
    // Not a second wording of one fact: the key is the checklist's.
    expect(line.phrase.key).toBe("osinstall.packages.blocked.needsFixfonts");
    expect(leafText(en, line.phrase.key)).toMatch(/FixFonts/);
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
      row({ facts: { file: null, runsOnAmiga: true }, state: { state: "ready" } }),
      row({ facts: { file: null, runsOnAmiga: false }, state: { state: "ready" } }),
      row({ facts: { file: null, runsOnAmiga: null }, state: { state: "ready" } }),
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
