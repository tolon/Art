import { describe, expect, it } from "vitest";

import { GIB, gibNumber, mibNumber, size } from "./size";

/**
 * ART-105 folded three copies of this arithmetic into one, so this is the
 * first test any of them has had. The numbers below are what the three
 * screens printed before the fold, not what the new function happens to
 * return — a fold that quietly changed a rounding would be a regression on
 * every OS Builder screen at once.
 */
describe("size", () => {
  it("switches to GB at exactly one gibibyte", () => {
    expect(size(GIB - 1)).toBe("1024 MB");
    expect(size(GIB)).toBe("1 GB");
  });

  it("prints gigabytes to two decimal places", () => {
    // A 32 GB card, and one that is not a round number of gibibytes.
    expect(size(32 * GIB)).toBe("32 GB");
    expect(size(Math.round(1.5 * GIB))).toBe("1.5 GB");
    expect(size(Math.round(1.234 * GIB))).toBe("1.23 GB");
  });

  it("prints megabytes to one decimal place", () => {
    expect(size(880 * 1024)).toBe("0.9 MB"); // an ADF
    expect(size(0)).toBe("0 MB");
  });

  it("hands the bare numbers over without a unit", () => {
    // `CardBuilder` interpolates these into a translated sentence, so a unit
    // baked into the number would be an untranslatable string.
    expect(gibNumber(2 * GIB)).toBe("2");
    expect(mibNumber(100 * 1024 * 1024)).toBe("100");
    expect(gibNumber(GIB)).not.toContain("GB");
  });

  it("never renders a negative zero", () => {
    // `-0` stringifies as "0" in JS, but a rounding that produced it would
    // print "-0 MB" if anything ever concatenated it differently.
    expect(size(0)).not.toContain("-");
    expect(mibNumber(0)).toBe("0");
  });
});
