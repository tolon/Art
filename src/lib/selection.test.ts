// Covers the multi-select reducer in `selection.ts` directly — plain click,
// Ctrl+click, Shift+click, Insert, Ctrl+A and the reset-on-navigation rule —
// without rendering `FileManager.tsx`. See that file's comment for why: it
// calls Tauri commands on mount and pulls in most of the app's `lib/*`
// surface, so a full render is a rabbit hole for what is otherwise a handful
// of pure `Set<string>` operations.
import { describe, expect, it } from "vitest";

import {
  emptySelectionUpdate,
  entriesIn,
  insertToggle,
  selectOnly,
  selectRange,
  singleSelected,
  toggleOne,
  toggleSelectAll,
} from "./selection";
import type { PanelEntry } from "./panel";

function entry(name: string, overrides: Partial<PanelEntry> = {}): PanelEntry {
  return {
    name,
    is_dir: false,
    bytes: 100,
    path: null,
    header_block: null,
    is_link: false,
    date: null,
    ...overrides,
  };
}

const ENTRIES = ["Alpha", "Beta", "Gamma", "Delta", "Epsilon"].map((name) => entry(name));

describe("selectOnly / toggleOne", () => {
  it("a plain click selects only that entry", () => {
    const update = selectOnly("Beta");
    expect([...update.selected]).toEqual(["Beta"]);
    expect(update.anchor).toBe("Beta");
  });

  it("ctrl+click toggles one entry without losing the rest of the selection", () => {
    const selected = new Set(["Alpha", "Gamma"]);
    const added = toggleOne(selected, "Beta");
    expect([...added.selected].sort()).toEqual(["Alpha", "Beta", "Gamma"]);
    expect(added.anchor).toBe("Beta");

    const removed = toggleOne(added.selected, "Alpha");
    expect([...removed.selected].sort()).toEqual(["Beta", "Gamma"]);

    // The original Set passed in is untouched — callers must not have to
    // guess whether this mutates.
    expect([...selected].sort()).toEqual(["Alpha", "Gamma"]);
  });
});

describe("selectRange (shift+click)", () => {
  it("selects the contiguous range from the anchor to the clicked entry", () => {
    const first = selectOnly("Beta");
    const ranged = selectRange(ENTRIES, first.selected, first.anchor, "Delta");
    expect([...ranged.selected].sort()).toEqual(["Beta", "Delta", "Gamma"]);
    // The anchor stays put, so a second shift+click can extend or shrink
    // the same range rather than starting a new one.
    expect(ranged.anchor).toBe("Beta");
  });

  it("works backwards — clicking above the anchor", () => {
    const first = selectOnly("Delta");
    const ranged = selectRange(ENTRIES, first.selected, first.anchor, "Beta");
    expect([...ranged.selected].sort()).toEqual(["Beta", "Delta", "Gamma"]);
  });

  it("adds to whatever was already selected rather than replacing it", () => {
    const preSelected = new Set(["Alpha"]);
    const ranged = selectRange(ENTRIES, preSelected, "Gamma", "Epsilon");
    expect([...ranged.selected].sort()).toEqual(["Alpha", "Delta", "Epsilon", "Gamma"]);
  });

  it("falls back to a plain single selection when the anchor no longer exists", () => {
    const ranged = selectRange(ENTRIES, new Set(), "Nonexistent", "Gamma");
    expect([...ranged.selected]).toEqual(["Gamma"]);
    expect(ranged.anchor).toBe("Gamma");
  });
});

describe("insertToggle", () => {
  it("toggles the entry at the anchor and moves the anchor down one", () => {
    const step1 = insertToggle(ENTRIES, new Set(), null);
    expect([...step1.selected]).toEqual(["Alpha"]);
    expect(step1.anchor).toBe("Beta");

    const step2 = insertToggle(ENTRIES, step1.selected, step1.anchor);
    expect([...step2.selected].sort()).toEqual(["Alpha", "Beta"]);
    expect(step2.anchor).toBe("Gamma");
  });

  it("un-marks an already-selected entry under the anchor", () => {
    const marked = insertToggle(ENTRIES, new Set(["Alpha"]), "Alpha");
    expect([...marked.selected]).toEqual([]);
    expect(marked.anchor).toBe("Beta");
  });

  it("stops advancing at the last entry", () => {
    const atEnd = insertToggle(ENTRIES, new Set(), "Epsilon");
    expect(atEnd.anchor).toBe("Epsilon");
  });

  it("does nothing on an empty pane", () => {
    const result = insertToggle([], new Set(), null);
    expect(result.selected.size).toBe(0);
    expect(result.anchor).toBeNull();
  });
});

describe("toggleSelectAll (ctrl+a)", () => {
  it("selects every entry, then clears on a second press", () => {
    const all = toggleSelectAll(ENTRIES, new Set());
    expect([...all.selected].sort()).toEqual(ENTRIES.map((e) => e.name).sort());

    const cleared = toggleSelectAll(ENTRIES, all.selected);
    expect(cleared.selected.size).toBe(0);
  });

  it("selects all rather than clearing when the pane is only partly selected", () => {
    const partial = new Set(["Alpha"]);
    const all = toggleSelectAll(ENTRIES, partial);
    expect([...all.selected].sort()).toEqual(ENTRIES.map((e) => e.name).sort());
  });
});

describe("navigation resets the selection", () => {
  it("clears both the selection and the anchor", () => {
    const populated = { selected: new Set(["Alpha", "Beta"]), anchor: "Beta" };
    expect(populated.selected.size).toBeGreaterThan(0);

    const reset = emptySelectionUpdate();
    expect(reset.selected.size).toBe(0);
    expect(reset.anchor).toBeNull();
  });
});

describe("entriesIn / singleSelected", () => {
  it("entriesIn returns the selected entries in pane order", () => {
    const found = entriesIn(ENTRIES, new Set(["Delta", "Alpha"]));
    expect(found.map((e) => e.name)).toEqual(["Alpha", "Delta"]);
  });

  it("singleSelected refuses when more than one entry is selected", () => {
    expect(singleSelected(ENTRIES, new Set(["Alpha", "Beta"]))).toBeNull();
  });

  it("singleSelected refuses when nothing is selected", () => {
    expect(singleSelected(ENTRIES, new Set())).toBeNull();
  });

  it("singleSelected returns the entry when exactly one is selected", () => {
    expect(singleSelected(ENTRIES, new Set(["Gamma"]))?.name).toBe("Gamma");
  });
});
