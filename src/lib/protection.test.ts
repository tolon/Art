import { describe, expect, it } from "vitest";

import { deleteProtectedNames, hasBit, isDeleteProtected } from "@/lib/protection";

describe("hasBit", () => {
  it("reads each bit at its own position", () => {
    // `hsparwed`, most significant first — the order `format_bits` writes.
    expect(hasBit("hsparwed", "h")).toBe(true);
    expect(hasBit("hsparwed", "d")).toBe(true);
    expect(hasBit("----rwed", "h")).toBe(false);
    expect(hasBit("----rwed", "r")).toBe(true);
    expect(hasBit("h-------", "h")).toBe(true);
    expect(hasBit("h-------", "s")).toBe(false);
  });

  it("refuses anything that is not eight characters", () => {
    // A local row's `attrs` are Windows attributes — a different alphabet of a
    // different length — and reading `d` out of them would be reading a bit
    // that is not there.
    expect(hasBit("rahs", "d")).toBe(false);
    expect(hasBit(null, "d")).toBe(false);
    expect(hasBit("", "d")).toBe(false);
  });
});

describe("isDeleteProtected", () => {
  it("is true exactly when the d bit is clear", () => {
    // A letter means the thing is allowed: `----rwed` is an ordinary file,
    // `----rwe-` is one AmigaDOS itself will refuse to delete.
    expect(isDeleteProtected("----rwe-")).toBe(true);
    expect(isDeleteProtected("----rwed")).toBe(false);
    expect(isDeleteProtected("hsparwed")).toBe(false);
    expect(isDeleteProtected("hspa----")).toBe(true);
  });

  it("treats unknown as deletable", () => {
    // The confirmation this feeds is a warning, not a lock: a row whose
    // source does not report attributes must not become undeletable because
    // ART could not tell.
    expect(isDeleteProtected(null)).toBe(false);
    expect(isDeleteProtected("rahs")).toBe(false);
  });
});

describe("deleteProtectedNames", () => {
  it("names only the protected entries, in order", () => {
    const entries = [
      { name: "Readme", attrs: "----rwed" },
      { name: "Slave", attrs: "----rwe-" },
      { name: "Data", attrs: null },
      { name: "System", attrs: "hs--rw-e".slice(0, 8) },
    ];
    expect(deleteProtectedNames(entries)).toEqual(["Slave", "System"]);
  });

  it("finds nothing in an ordinary selection", () => {
    expect(deleteProtectedNames([{ name: "a", attrs: "----rwed" }])).toEqual([]);
  });
});
