import { describe, expect, it } from "vitest";

import {
  extendSearch,
  findByPrefix,
  searchCharacter,
  shortenSearch,
} from "@/lib/quickSearch";

const NAMES = ["Lemmings", "Lotus", "LotusII", "Turrican", "lotus.readme"];

describe("searchCharacter", () => {
  const plain = { ctrlKey: false, altKey: false, metaKey: false };

  it("takes a single printable character", () => {
    expect(searchCharacter({ ...plain, key: "l" })).toBe("l");
    expect(searchCharacter({ ...plain, key: "7" })).toBe("7");
    expect(searchCharacter({ ...plain, key: "ç" })).toBe("ç");
  });

  it("never takes Space — that is the mark key", () => {
    // A search that swallowed Space would take away the shortcut a user
    // reaches for far more often than this one.
    expect(searchCharacter({ ...plain, key: " " })).toBeNull();
  });

  it("ignores named keys and anything held with a modifier", () => {
    for (const key of ["Enter", "F5", "ArrowDown", "Backspace", "Insert"]) {
      expect(searchCharacter({ ...plain, key }), key).toBeNull();
    }
    expect(searchCharacter({ ...plain, key: "a", ctrlKey: true })).toBeNull();
    expect(searchCharacter({ ...plain, key: "a", altKey: true })).toBeNull();
  });
});

describe("findByPrefix", () => {
  it("matches the start of a name, case-insensitively", () => {
    expect(findByPrefix(NAMES, "lot", null)).toBe("Lotus");
    expect(findByPrefix(NAMES, "LOT", null)).toBe("Lotus");
    expect(findByPrefix(NAMES, "tur", null)).toBe("Turrican");
  });

  it("does not match the middle of a name", () => {
    // Aiming, not filtering — that is what the filter box is for.
    expect(findByPrefix(NAMES, "us", null)).toBeNull();
  });

  it("starts at the cursor, so a lengthening prefix refines the same row", () => {
    // `l` → `lo` → `lot` must stay on Lotus rather than skipping to the next
    // match on every keystroke.
    expect(findByPrefix(NAMES, "l", "Lotus")).toBe("Lotus");
    expect(findByPrefix(NAMES, "lo", "Lotus")).toBe("Lotus");
  });

  it("wraps around once, and only once", () => {
    expect(findByPrefix(NAMES, "lem", "Turrican")).toBe("Lemmings");
    expect(findByPrefix(NAMES, "zz", "Turrican")).toBeNull();
  });

  it("finds nothing in an empty pane, or for an empty prefix", () => {
    expect(findByPrefix([], "a", null)).toBeNull();
    expect(findByPrefix(NAMES, "", null)).toBeNull();
  });
});

describe("extendSearch", () => {
  it("builds the prefix up and moves the cursor with it", () => {
    const first = extendSearch(NAMES, "", "l", null);
    expect(first).toEqual({ prefix: "l", match: "Lemmings", accepted: true });

    const second = extendSearch(NAMES, "l", "o", "Lemmings");
    expect(second).toEqual({ prefix: "lo", match: "Lotus", accepted: true });
  });

  it("rejects a character that would match nothing, and keeps the prefix", () => {
    // Otherwise one typo empties the search and the cursor jumps somewhere
    // arbitrary the moment the next letter lands.
    const step = extendSearch(NAMES, "lot", "z", "Lotus");
    expect(step).toEqual({ prefix: "lot", match: null, accepted: false });
  });
});

describe("shortenSearch", () => {
  it("widens the search and re-aims from the top", () => {
    // A shortening prefix is widening; searching onward from the current row
    // would keep it stuck on the narrow answer it already had.
    expect(shortenSearch(NAMES, "lotusi")).toEqual({
      prefix: "lotus",
      match: "Lotus",
      accepted: true,
    });
  });

  it("ends the search when the last character goes", () => {
    expect(shortenSearch(NAMES, "l")).toEqual({ prefix: "", match: null, accepted: true });
  });
});
