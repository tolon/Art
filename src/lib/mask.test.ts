// Covers the filename mask in `mask.ts` directly — see that file's comment,
// and `sort.test.ts`'s / `selection.test.ts`'s, for why a pure `src/lib`
// function gets its own test file rather than being exercised only through
// `FileManager.tsx` (which calls Tauri commands on mount).
import { describe, expect, it } from "vitest";

import { entriesIn } from "./selection";
import { filterEntries, matchesMask } from "./mask";
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

describe("matchesMask — an empty mask", () => {
  it("matches everything", () => {
    expect(matchesMask("readme.txt", "")).toBe(true);
    expect(matchesMask("Anything.Else", "   ")).toBe(true);
  });
});

describe("matchesMask — * matches any run of characters", () => {
  it("a mask that is only * matches every name, with or without an extension", () => {
    expect(matchesMask("readme.txt", "*")).toBe(true);
    expect(matchesMask("Makefile", "*")).toBe(true);
    expect(matchesMask("", "*")).toBe(true);
  });

  it("*.txt matches by extension only", () => {
    expect(matchesMask("readme.txt", "*.txt")).toBe(true);
    expect(matchesMask("README.TXT", "*.txt")).toBe(true);
    expect(matchesMask("readme.doc", "*.txt")).toBe(false);
  });

  it("a*.txt matches on both ends", () => {
    expect(matchesMask("art.txt", "a*.txt")).toBe(true);
    expect(matchesMask("brutus.txt", "a*.txt")).toBe(false);
  });
});

describe("matchesMask — ? matches exactly one character", () => {
  it("requires exactly one character in that position, not zero and not several", () => {
    expect(matchesMask("abc", "a?c")).toBe(true);
    expect(matchesMask("ac", "a?c")).toBe(false);
    expect(matchesMask("abbc", "a?c")).toBe(false);
  });
});

describe("matchesMask — case-insensitive", () => {
  it("matches regardless of case on either side", () => {
    expect(matchesMask("Workbench.adf", "workbench.adf")).toBe(true);
    expect(matchesMask("workbench.adf", "WORKBENCH.ADF")).toBe(true);
  });
});

describe("matchesMask — regex metacharacters are literal, not special", () => {
  it("a mask containing + . ( ) matches only the literal name, never as a pattern", () => {
    expect(matchesMask("a+b.txt", "a+b.txt")).toBe(true);
    // If '+' were a live regex quantifier ("one or more of the preceding
    // character"), this mask would also match "aaaab.txt" — it must not.
    expect(matchesMask("aaaab.txt", "a+b.txt")).toBe(false);
    expect(matchesMask("(1).txt", "(1).txt")).toBe(true);
  });

  it("does not catastrophically backtrack on a classic ReDoS-shaped mask", () => {
    const mask = "(a+)+$";
    const hostileName = "a".repeat(40) + "!";
    const started = Date.now();
    expect(matchesMask(hostileName, mask)).toBe(false);
    expect(Date.now() - started).toBeLessThan(100);
  });
});

describe("matchesMask — no match at all", () => {
  it("returns false rather than falling back to something looser", () => {
    expect(matchesMask("readme.txt", "zzz")).toBe(false);
  });
});

describe("matchesMask — a name with no extension", () => {
  it("*.txt does not match an extensionless name; a bare mask can still match it", () => {
    expect(matchesMask("Makefile", "*.txt")).toBe(false);
    expect(matchesMask("Makefile", "Makefile")).toBe(true);
    expect(matchesMask("Makefile", "Make*")).toBe(true);
  });
});

describe("filterEntries", () => {
  const entries = [entry("readme.txt"), entry("photo.jpg"), entry("notes.txt"), entry("Tools", { is_dir: true })];

  it("hides non-matching entries and keeps the rest, in their original order", () => {
    const filtered = filterEntries(entries, "*.txt");
    expect(filtered.map((e) => e.name)).toEqual(["readme.txt", "notes.txt"]);
  });

  it("an empty mask returns every entry unchanged", () => {
    expect(filterEntries(entries, "")).toEqual(entries);
  });

  it("a mask matching nothing returns an empty list", () => {
    expect(filterEntries(entries, "*.zip")).toEqual([]);
  });
});

// The safety property the plan calls out by name: filtering must never
// change what an action like F5 operates on. `FileManager.tsx`'s
// `selectedEntries(side)` is exactly `entriesIn(paneEntries(side),
// selection[side])` (see `selection.ts`) — and `paneEntries` is the
// filtered-then-sorted list. So even a selection that still names an
// entry the filter just hid can never make it into what F5 copies: the
// entry is not in `paneEntries` for `entriesIn` to find. This is the
// belt; `FileManager.tsx` clearing the selection on every filter change
// (see its `setPaneFilter`) is the suspenders — it keeps the on-screen
// "N selected" count from lying about a selection the user can no longer
// see.
describe("filterEntries + entriesIn — hides non-matching entries without changing what F5 would copy", () => {
  it("a hidden entry drops out of what a selection resolves to, even if the raw Set still names it", () => {
    const entries = [entry("readme.txt"), entry("photo.jpg"), entry("notes.txt")];
    const selection = new Set(entries.map((e) => e.name)); // all three selected

    const visible = filterEntries(entries, "*.txt");
    const wouldCopy = entriesIn(visible, selection);

    expect(wouldCopy.map((e) => e.name)).toEqual(["readme.txt", "notes.txt"]);
    expect(wouldCopy.some((e) => e.name === "photo.jpg")).toBe(false);
  });

  it("filtering down to nothing means F5 would copy nothing, however large the stale selection is", () => {
    const entries = [entry("a.txt"), entry("b.txt"), entry("c.txt")];
    const selection = new Set(entries.map((e) => e.name));

    const visible = filterEntries(entries, "*.zip");
    expect(entriesIn(visible, selection)).toEqual([]);
  });
});
