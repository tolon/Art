// Covers `cursorStep` directly — every rule ART-275's fix states, both ends
// of the list, an empty pane, and `pageRows` both smaller and larger than the
// list. No DOM, no React: see `cursorKeys.ts`'s own header for why.
import { describe, expect, it } from "vitest";

import { cursorStep } from "./cursorKeys";

const NAMES = ["Alpha", "Beta", "Gamma", "Delta", "Epsilon"];

describe("cursorStep — no cursor yet", () => {
  it("down lands on the first name", () => {
    expect(cursorStep(NAMES, null, "down", 2)).toBe("Alpha");
  });

  it("home lands on the first name", () => {
    expect(cursorStep(NAMES, null, "home", 2)).toBe("Alpha");
  });

  it("pageDown lands on the first name", () => {
    expect(cursorStep(NAMES, null, "pageDown", 2)).toBe("Alpha");
  });

  it("up lands on the last name", () => {
    expect(cursorStep(NAMES, null, "up", 2)).toBe("Epsilon");
  });

  it("end lands on the last name", () => {
    expect(cursorStep(NAMES, null, "end", 2)).toBe("Epsilon");
  });

  it("pageUp lands on the last name", () => {
    expect(cursorStep(NAMES, null, "pageUp", 2)).toBe("Epsilon");
  });

  it("treats a stale cursor (no longer in the list) the same as no cursor", () => {
    expect(cursorStep(NAMES, "Ghost", "down", 2)).toBe("Alpha");
    expect(cursorStep(NAMES, "Ghost", "up", 2)).toBe("Epsilon");
  });
});

describe("cursorStep — up/down, one row at a time", () => {
  it("moves down one row", () => {
    expect(cursorStep(NAMES, "Beta", "down", 2)).toBe("Gamma");
  });

  it("moves up one row", () => {
    expect(cursorStep(NAMES, "Gamma", "up", 2)).toBe("Beta");
  });

  it("up at the first row stays — no wrap", () => {
    expect(cursorStep(NAMES, "Alpha", "up", 2)).toBe("Alpha");
  });

  it("down at the last row stays — no wrap", () => {
    expect(cursorStep(NAMES, "Epsilon", "down", 2)).toBe("Epsilon");
  });
});

describe("cursorStep — Home/End", () => {
  it("home always lands on the first name, wherever the cursor is", () => {
    expect(cursorStep(NAMES, "Delta", "home", 2)).toBe("Alpha");
  });

  it("end always lands on the last name, wherever the cursor is", () => {
    expect(cursorStep(NAMES, "Beta", "end", 2)).toBe("Epsilon");
  });
});

describe("cursorStep — PageUp/PageDown", () => {
  it("pageDown moves pageRows rows down", () => {
    expect(cursorStep(NAMES, "Alpha", "pageDown", 2)).toBe("Gamma");
  });

  it("pageUp moves pageRows rows up", () => {
    expect(cursorStep(NAMES, "Epsilon", "pageUp", 2)).toBe("Gamma");
  });

  it("pageDown clamps to the last row when pageRows overshoots the list", () => {
    expect(cursorStep(NAMES, "Beta", "pageDown", 50)).toBe("Epsilon");
  });

  it("pageUp clamps to the first row when pageRows overshoots the list", () => {
    expect(cursorStep(NAMES, "Delta", "pageUp", 50)).toBe("Alpha");
  });

  it("a pageRows of 1 behaves exactly like a single up/down step", () => {
    expect(cursorStep(NAMES, "Beta", "pageDown", 1)).toBe("Gamma");
    expect(cursorStep(NAMES, "Beta", "pageUp", 1)).toBe("Alpha");
  });

  it("pageDown at the last row stays, pageUp at the first row stays", () => {
    expect(cursorStep(NAMES, "Epsilon", "pageDown", 2)).toBe("Epsilon");
    expect(cursorStep(NAMES, "Alpha", "pageUp", 2)).toBe("Alpha");
  });
});

describe("cursorStep — empty pane", () => {
  it("returns null for every move, cursor or no cursor", () => {
    expect(cursorStep([], null, "down", 2)).toBeNull();
    expect(cursorStep([], "Anything", "home", 2)).toBeNull();
    expect(cursorStep([], null, "pageUp", 2)).toBeNull();
  });
});
