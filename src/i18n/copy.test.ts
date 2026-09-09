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

// **`osinstall.packages.intro` (ART-198) is gone, and its three cases with
// it** (four tabs round 3, task 3). The sentence was `PackagePanel`'s lead
// paragraph; that panel is deleted, the key had no renderer left, and
// `dead-keys.test.ts` is what says so. A copy test over a key nothing can put
// on screen guards nothing — it would go on passing about a sentence no user
// can read, which is the same shape as the defect this file exists for.
//
// The ruling it recorded is not lost and is not this file's to keep alive:
// *an official update is not introduced by an unofficial example*, and
// *BoingBag is used, never glossed* ("BoingBag'ı bütün Amiga camiası bilir").
// Tab 2 names a package by the name the manifest states (ART-060) and adds no
// lead paragraph of its own, so there is nothing here to assert against yet.
// A screen that writes one writes its guard with it.

/**
 * **A refusal that names a tab must name the tab the user will see** (four
 * tabs, rounds 2 and its fix wave). The Kickstart and destination fields
 * moved off tab 1, and six sentences send the user after them. Each says
 * *the Kickstart and destination tab* — and "Kickstart and destination" is
 * `osBuilder.step.makine`, the label printed on the tab itself.
 *
 * The assertion is against that key rather than a literal, because the
 * failure this guards is **drift**: renaming the tab in one catalogue and
 * leaving the six sentences pointing at a name nothing on screen carries.
 * A literal in this file would be a copy of the label, and copies drift the
 * same way. Read the substring out of the catalogue, and the two cannot part.
 *
 * Both catalogues, because a Turkish user following a sentence that says
 * "Kickstart and destination" over a tab labelled "Kickstart ve hedef" has
 * been given a name that is not on their screen.
 */
describe("the sentences that send a user to tab 3 name it as the tab is labelled", () => {
  const KEYS = [
    "osBuilder.bar.noDestination",
    "osinstall.blocked.noDestination",
    "osinstall.blocked.destinationExists",
    "osinstall.refusal.romUnknown",
    "osinstall.slots.romNotChosen",
    "osinstall.slots.romNotChosenNoFloor",
  ] as const;

  const read = (catalogue: unknown, key: string): string =>
    key.split(".").reduce<unknown>((node, part) => (node as Record<string, unknown>)[part], catalogue) as string;

  it("uses the English tab label, character for character", () => {
    // The label first: a test that only checked the six sentences would pass
    // on the day the label itself went missing.
    expect(en.osBuilder.step.makine).toBeTruthy();
    for (const key of KEYS) {
      expect(read(en, key)).toContain(en.osBuilder.step.makine);
    }
  });

  it("uses the Turkish tab label, character for character", () => {
    expect(tr.osBuilder.step.makine).toBeTruthy();
    for (const key of KEYS) {
      expect(read(tr, key)).toContain(tr.osBuilder.step.makine);
    }
  });

  /**
   * And the two refusals the fix wave reworded actually say where to go —
   * asserted apart from the label check above, which a sentence could pass
   * by mentioning the tab without telling the user to do anything there.
   */
  it("tells the user to act on that tab, in the two refusals that used not to", () => {
    expect(en.osinstall.refusal.romUnknown).toContain("Choose another on the");
    expect(tr.osinstall.refusal.romUnknown).toContain("sekmesinde başka bir Kickstart seçin");
    expect(en.osinstall.blocked.destinationExists).toContain("move the old one aside — on the");
    expect(tr.osinstall.blocked.destinationExists).toContain("eskisini kenara alın — ");
  });
});

/**
 * **A sentence may only claim what the boolean behind it proves** (fix wave
 * 2, #2). `useDestinationCheck` folds three core answers into `taken`, and
 * `describe_tree` says `isTree: false` for a path that is not there — so the
 * old wording, "Empty folder: a new tree will be written here", described a
 * remembered path whose drive letter had moved as an empty folder, and the
 * refusal called a *file* a folder with something in it. Neither is a state
 * the hook can distinguish; both were the screen out-claiming the core.
 */
describe("osinstall.destination.fresh / .taken say what the check proves", () => {
  it("does not call an unexamined path an empty folder", () => {
    expect(en.osinstall.destination.fresh.toLowerCase()).not.toContain("empty folder");
    expect(tr.osinstall.destination.fresh.toLowerCase()).not.toContain("boş klasör");
  });

  it("does not call a path that may be a file a folder with something in it", () => {
    expect(en.osinstall.destination.taken.toLowerCase()).not.toContain("this folder already has");
    expect(en.osinstall.destination.taken.toLowerCase()).not.toContain("empty this one");
    expect(tr.osinstall.destination.taken.toLowerCase()).not.toContain("bu klasörde zaten");
    expect(tr.osinstall.destination.taken.toLowerCase()).not.toContain("bunu boşaltın");
  });

  it("still says what ART will and will not do, so neither is merely blank", () => {
    expect(en.osinstall.destination.fresh).toContain("ART can write here");
    expect(en.osinstall.destination.taken).toContain("ART will not write here");
    expect(tr.osinstall.destination.fresh).toContain("ART buraya yazabilir");
    expect(tr.osinstall.destination.taken).toContain("ART buraya yazmaz");
  });
});
