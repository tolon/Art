import { describe, expect, it } from "vitest";

import {
  ALL_PROVENANCES,
  isStated,
  provenancePhrase,
  type Provenance,
} from "./gameindex";

describe("provenancePhrase", () => {
  it("names every provenance the core can emit", () => {
    for (const p of ALL_PROVENANCES) {
      expect(provenancePhrase(p).key).toBe(`gameindex.provenance.${p}`);
    }
  });

  /// The list must match `core::gameindex::record::Provenance` exactly. A
  /// variant added there and forgotten here renders as a missing key.
  it("covers the four the core defines and no more", () => {
    expect([...ALL_PROVENANCES].sort()).toEqual([
      "drawer-name",
      "rp9-manifest",
      "tosec-name",
      "whdload-slave",
    ]);
  });
});

describe("isStated", () => {
  /// The whole point of carrying provenance: a manifest and a slave header
  /// *declared* the value, a filename and a drawer name only suggest it. The
  /// screen marks the second pair.
  it("separates a declaration from a guess", () => {
    expect(isStated("rp9-manifest")).toBe(true);
    expect(isStated("whdload-slave")).toBe(true);
    expect(isStated("tosec-name")).toBe(false);
    expect(isStated("drawer-name")).toBe(false);
  });

  it("agrees with itself across the whole set", () => {
    const stated = ALL_PROVENANCES.filter(isStated);
    expect(stated).toHaveLength(2);
  });
});

describe("Provenance type", () => {
  it("accepts only the four strings", () => {
    // A compile-time check written as a runtime one so `pnpm test` carries it:
    // if the union ever widens, this assignment stops matching the list above.
    const sample: Provenance = "whdload-slave";
    expect(ALL_PROVENANCES).toContain(sample);
  });
});
