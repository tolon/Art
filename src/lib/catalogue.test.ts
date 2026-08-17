import { describe, expect, it } from "vitest";

import { isRefreshMode, NO_OVERRIDE, type RefreshMode } from "./gameindex";

describe("isRefreshMode", () => {
  // The two words Rust will accept, and nothing else. A third string sent from
  // the screen is refused by the command; this stops it being sent.
  it("accepts exactly the two modes the command knows", () => {
    expect(isRefreshMode("update")).toBe(true);
    expect(isRefreshMode("rescan")).toBe(true);
    expect(isRefreshMode("full")).toBe(false);
    expect(isRefreshMode("")).toBe(false);
    expect(isRefreshMode("Update")).toBe(false);
  });

  it("narrows the type", () => {
    const raw: string = "rescan";
    if (isRefreshMode(raw)) {
      const mode: RefreshMode = raw;
      expect(mode).toBe("rescan");
    }
  });
});

describe("NO_OVERRIDE", () => {
  // Sending this is how the screen says "forget my corrections for this
  // title" — the Rust side removes an override with nothing in it rather than
  // storing an empty one, so every field has to be null for that to fire.
  it("says nothing about any field", () => {
    expect(Object.values(NO_OVERRIDE).every((value) => value === null)).toBe(
      true
    );
  });

  it("covers every field the Rust side can store", () => {
    expect(Object.keys(NO_OVERRIDE).sort()).toEqual([
      "chipset",
      "genre",
      "publisher",
      "title",
      "year",
    ]);
  });
});
