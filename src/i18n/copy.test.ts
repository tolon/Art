// Sentences whose *wording* is the defect, kept honest by a test.
//
// The parity test next door proves both catalogues carry the same keys. It
// cannot prove a sentence is true: ART-198 passed parity for months because
// both catalogues were equally wrong — the Turkish carried the English's
// contradiction faithfully, which is what a good translation of a bad
// sentence does.
//
// These assertions name the specific contradiction each string used to hold,
// so putting the old sentence back fails the run. That is the whole point:
// a test written for a copy defect is worth nothing unless the defect,
// restored, fails it.

import { describe, expect, it } from "vitest";

import en from "./en.json";
import tr from "./tr.json";

describe("osinstall.packages.intro (ART-198)", () => {
  it("does not offer an unofficial pack as an example of an official update", () => {
    const english = en.osinstall.packages.intro.toLowerCase();
    // The defect was both words in one sentence: "an official update — … an
    // unofficial pack …", where the em-dash pair reads as an appositive. So
    // the sentence offered an unofficial pack as an example of an official
    // one. Either word alone is fine; the pair is the bug.
    const promisesOfficial = english.includes("official update");
    const offersUnofficial = english.includes("unofficial");
    expect(promisesOfficial && offersUnofficial).toBe(false);
  });

  it("does not carry the same contradiction in Turkish", () => {
    const turkish = tr.osinstall.packages.intro.toLowerCase();
    const promisesOfficial = turkish.includes("resmi bir güncelleme");
    const offersUnofficial = turkish.includes("resmi olmayan");
    expect(promisesOfficial && offersUnofficial).toBe(false);
  });

  it("names a BoingBag without explaining what one is", () => {
    // The owner's ruling: the name is known across the Amiga community —
    // "BoingBag'ı bütün Amiga camiası bilir, onu çevirmene gerek yok." It is
    // used, not glossed, so the sentence must not introduce it with a
    // "like …" example list.
    expect(en.osinstall.packages.intro).toContain("BoingBag");
    expect(tr.osinstall.packages.intro).toContain("BoingBag");
    expect(en.osinstall.packages.intro.toLowerCase()).not.toContain("like the turkish");
    expect(tr.osinstall.packages.intro.toLowerCase()).not.toContain("türkçe katalog paketi gibi");
  });
});
