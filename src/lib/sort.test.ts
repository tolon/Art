// Covers the panel column sort in `sort.ts` directly — see that file's
// comment, and `selection.test.ts`'s, for why: `FileManager.tsx` calls Tauri
// commands on mount and is not something to render for a handful of pure
// comparator checks.
import { describe, expect, it } from "vitest";

import { clickColumn, compareEntries, defaultSortState, sortEntries } from "./sort";
import type { PanelEntry } from "./panel";

function entry(name: string, overrides: Partial<PanelEntry> = {}): PanelEntry {
  return {
    name,
    is_dir: false,
    bytes: 100,
    path: null,
    header_block: null,
    iso_extent: null,
    is_link: false,
    date: null,
    attrs: null,
    ...overrides,
  };
}

describe("defaultSortState", () => {
  it("starts on name, ascending — what a freshly opened pane and a navigation reset both use", () => {
    expect(defaultSortState()).toEqual({ column: "name", direction: "asc" });
  });
});

describe("clickColumn", () => {
  it("switches to a new column ascending", () => {
    const state = clickColumn({ column: "name", direction: "desc" }, "size");
    expect(state).toEqual({ column: "size", direction: "asc" });
  });

  it("reverses direction on the same column", () => {
    const first = clickColumn({ column: "size", direction: "asc" }, "size");
    expect(first).toEqual({ column: "size", direction: "desc" });

    const second = clickColumn(first, "size");
    expect(second).toEqual({ column: "size", direction: "asc" });
  });
});

describe("compareEntries / sortEntries — folders first, in both directions", () => {
  const mixed = [
    entry("zebra.txt"),
    entry("Tools", { is_dir: true }),
    entry("Apple.txt"),
    entry("assets", { is_dir: true }),
  ];

  it("folders come before files ascending", () => {
    const sorted = sortEntries(mixed, { column: "name", direction: "asc" });
    expect(sorted.map((e) => e.name)).toEqual(["assets", "Tools", "Apple.txt", "zebra.txt"]);
  });

  it("folders still come before files descending — a commander does not scatter them", () => {
    const sorted = sortEntries(mixed, { column: "name", direction: "desc" });
    expect(sorted.map((e) => e.name)).toEqual(["Tools", "assets", "zebra.txt", "Apple.txt"]);
  });

  it("folders stay first even when sorting by size or date", () => {
    const bySize = sortEntries(mixed, { column: "size", direction: "desc" });
    expect(bySize.slice(0, 2).every((e) => e.is_dir)).toBe(true);

    const byDate = sortEntries(mixed, { column: "date", direction: "asc" });
    expect(byDate.slice(0, 2).every((e) => e.is_dir)).toBe(true);
  });
});

describe("compareEntries — name column is case-insensitive", () => {
  it("sorts 'assets' and 'Tools' as letters, not by ASCII case", () => {
    const files = [entry("Zebra.txt"), entry("apple.txt"), entry("Banana.txt")];
    const sorted = sortEntries(files, { column: "name", direction: "asc" });
    expect(sorted.map((e) => e.name)).toEqual(["apple.txt", "Banana.txt", "Zebra.txt"]);
  });
});

describe("compareEntries — size column", () => {
  it("orders smallest to largest ascending, largest to smallest descending", () => {
    const files = [entry("big", { bytes: 3000 }), entry("small", { bytes: 10 }), entry("mid", { bytes: 500 })];

    const asc = sortEntries(files, { column: "size", direction: "asc" });
    expect(asc.map((e) => e.name)).toEqual(["small", "mid", "big"]);

    const desc = sortEntries(files, { column: "size", direction: "desc" });
    expect(desc.map((e) => e.name)).toEqual(["big", "mid", "small"]);
  });
});

describe("compareEntries — date column", () => {
  it("orders oldest to newest ascending", () => {
    const files = [
      entry("new", { date: 3000 }),
      entry("old", { date: 100 }),
      entry("mid", { date: 500 }),
    ];
    const sorted = sortEntries(files, { column: "date", direction: "asc" });
    expect(sorted.map((e) => e.name)).toEqual(["old", "mid", "new"]);
  });

  it("treats a missing date as the oldest possible, in either direction", () => {
    const files = [entry("dated", { date: 100 }), entry("undated", { date: null })];

    const asc = sortEntries(files, { column: "date", direction: "asc" });
    expect(asc.map((e) => e.name)).toEqual(["undated", "dated"]);

    const desc = sortEntries(files, { column: "date", direction: "desc" });
    expect(desc.map((e) => e.name)).toEqual(["dated", "undated"]);
  });
});

describe("compareEntries — total order: ties fall back to name", () => {
  it("same size, in either direction, resolves by case-insensitive name so the order cannot flicker", () => {
    const files = [entry("Charlie", { bytes: 50 }), entry("alpha", { bytes: 50 }), entry("Bravo", { bytes: 50 })];

    const asc = sortEntries(files, { column: "size", direction: "asc" });
    expect(asc.map((e) => e.name)).toEqual(["alpha", "Bravo", "Charlie"]);

    const desc = sortEntries(files, { column: "size", direction: "desc" });
    expect(desc.map((e) => e.name)).toEqual(["alpha", "Bravo", "Charlie"]);
  });

  it("re-sorting an already-sorted list is a no-op", () => {
    const files = [entry("Apple.txt"), entry("Banana.txt"), entry("cherry.txt")];
    const once = sortEntries(files, { column: "name", direction: "asc" });
    const twice = sortEntries(once, { column: "name", direction: "asc" });
    expect(twice.map((e) => e.name)).toEqual(once.map((e) => e.name));
  });
});

describe("compareEntries — a stray call never returns something inconsistent", () => {
  it("comparing an entry with itself is always 0", () => {
    const e = entry("Solo");
    expect(compareEntries(e, e, { column: "name", direction: "asc" })).toBe(0);
    expect(compareEntries(e, e, { column: "size", direction: "desc" })).toBe(0);
    expect(compareEntries(e, e, { column: "date", direction: "asc" })).toBe(0);
  });
});
