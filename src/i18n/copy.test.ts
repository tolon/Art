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
describe("the sentences that send a user to another tab name it as the tab is labelled", () => {
  const KEYS = [
    "osBuilder.bar.noDestination",
    "osinstall.blocked.noDestination",
    "osinstall.blocked.destinationExists",
    "osinstall.refusal.romUnknown",
    "osinstall.slots.romNotChosen",
    "osinstall.slots.romNotChosenNoFloor",
    // Round 3's fix wave adds tab 2's own two banners. They used to say
    // "pick one below", where `PackagePanel`'s tree picker was — deleted in
    // round 3, so the sentences pointed at nothing on the screen. The tree a
    // build works on is chosen once, on tab 3, and both now say so.
    "osBuilder.step.asksTree",
    "osBuilder.step.notATree",
    // The whole-branch review of round 5 (Minor 1). `AmigaInstallPanel`'s
    // one sentence for the case where the destination won names the tab that
    // owns the destination — the panel draws no field then, so the name is
    // the whole of what the user is given to act on, and it was unpinned.
    "osinstall.amigaInstall.treeRoot.fromDestination",
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

  /**
   * And tab 2's two banners no longer send the reader *below*, where nothing
   * is. Asserted as the absence of the old word plus the presence of the new
   * instruction, because a sentence that merely dropped "below" would leave a
   * user told there is no tree and not told where one comes from.
   */
  it("does not send the reader below the banner for a picker that is gone", () => {
    expect(en.osBuilder.step.asksTree.toLowerCase()).not.toContain("below");
    expect(en.osBuilder.step.notATree.toLowerCase()).not.toContain("below");
    expect(tr.osBuilder.step.asksTree.toLowerCase()).not.toContain("aşağıdan");
    expect(tr.osBuilder.step.notATree.toLowerCase()).not.toContain("aşağıdan");
    expect(en.osBuilder.step.asksTree).toContain("Choose the destination on the");
    expect(en.osBuilder.step.notATree).toContain("Choose another on the");
    expect(tr.osBuilder.step.asksTree).toContain("sekmesinde seçin");
    expect(tr.osBuilder.step.notATree).toContain("sekmesinde başka bir klasör seçin");
  });

  /**
   * **And the one sentence that sends a user to tab 1** (four tabs round 5,
   * task 4). `osinstall.blocked.noFolder` is the Build button's blocker when
   * no install media folder has been chosen; it said *"Choose the media
   * folder first"* — first meaning *before this*, with nothing saying where
   * the folder is chosen. Since round 2 that is the **Amiga files** tab,
   * three tabs away from the button carrying the refusal, so a refusal that
   * named no tab was a refusal the user could not act on (CLAUDE.md, "a
   * refusal must be actionable").
   *
   * Asserted against `osBuilder.step.dosyalar` for the same reason as the six
   * above: the failure guarded is drift between the label on the tab and the
   * name the sentence uses, and a literal here would drift with it.
   */
  it("uses the Amiga files tab label in the blocker that sends a user there", () => {
    expect(en.osBuilder.step.dosyalar).toBeTruthy();
    expect(tr.osBuilder.step.dosyalar).toBeTruthy();
    expect(en.osinstall.blocked.noFolder).toContain(en.osBuilder.step.dosyalar);
    expect(tr.osinstall.blocked.noFolder).toContain(tr.osBuilder.step.dosyalar);
  });

  /**
   * **And the badge that says ART has nowhere to look** (whole-branch review
   * of round 5, Important 1). `osinstall.chain.noFolders` said *"add the
   * folders your archives are in at the top of this tab"*, written when
   * `AmigaInstallPanel` sat at the foot of the **Amiga files** tab, above
   * which the folder list really was. Since round 5 the panel's only mount is
   * the WinUAE studio, which has no folder list on it anywhere — so the
   * badge sent the reader to the top of the screen they were already on while
   * the `Link` a space later sent them to another tab. Two contradictory
   * instructions one sentence apart.
   *
   * Pinned against `osBuilder.step.dosyalar` for the same drift reason as the
   * blocker above: the sentence and the link must name one tab, and the tab's
   * label is where that name comes from.
   */
  it("names the Amiga files tab in the badge whose link goes there", () => {
    expect(en.osinstall.chain.noFolders).toContain(en.osBuilder.step.dosyalar);
    expect(tr.osinstall.chain.noFolders).toContain(tr.osBuilder.step.dosyalar);
    // And no longer points at the top of a screen that has no folder list:
    // the presence check above would pass on a sentence that said both.
    expect(en.osinstall.chain.noFolders.toLowerCase()).not.toContain("top of this tab");
    expect(tr.osinstall.chain.noFolders.toLowerCase()).not.toContain("bu sekmenin üstünde");
  });

  /**
   * **Two more sentences that name a tab or a field label** (whole-branch
   * review of round 5, Minor 1). Neither was pinned, and both were written
   * in round 5 — the wave that renamed nothing is exactly the wave after
   * which a rename would go unnoticed.
   *
   * `firstboot.rehearse.needsTree` sits under the studio's disabled
   * *Rehearse* button and names two places a tree can come from: the field
   * above it (`osinstall.packages.treeRoot.label`) and the tab that owns the
   * destination (`osBuilder.step.makine`). `osBuilder.build.navigationLockedLane`
   * is the shell's own sentence while a build runs, and it names where the
   * Stop button is: the OS Builder (`nav.osBuilder`), Build tab
   * (`osBuilder.step.derle`).
   */
  it("names the tree field and the Kickstart tab in the rehearsal's own refusal", () => {
    expect(en.osinstall.packages.treeRoot.label).toBeTruthy();
    expect(tr.osinstall.packages.treeRoot.label).toBeTruthy();
    expect(en.firstboot.rehearse.needsTree).toContain(en.osinstall.packages.treeRoot.label);
    expect(en.firstboot.rehearse.needsTree).toContain(en.osBuilder.step.makine);
    expect(tr.firstboot.rehearse.needsTree).toContain(tr.osinstall.packages.treeRoot.label);
    expect(tr.firstboot.rehearse.needsTree).toContain(tr.osBuilder.step.makine);
  });

  it("names the OS Builder and its Build tab in the shell's locked sentence", () => {
    expect(en.nav.osBuilder).toBeTruthy();
    expect(tr.nav.osBuilder).toBeTruthy();
    expect(en.osBuilder.step.derle).toBeTruthy();
    expect(tr.osBuilder.step.derle).toBeTruthy();
    expect(en.osBuilder.build.navigationLockedLane).toContain(en.nav.osBuilder);
    expect(en.osBuilder.build.navigationLockedLane).toContain(en.osBuilder.step.derle);
    expect(tr.osBuilder.build.navigationLockedLane).toContain(tr.nav.osBuilder);
    expect(tr.osBuilder.build.navigationLockedLane).toContain(tr.osBuilder.step.derle);
  });

  /**
   * **And the sentence after a refused run names the button as it reads**
   * (whole-branch review of round 5, Minor 2). It said *"press Build
   * again"*, but `run.succeeded` is `false` after a refusal, so
   * `BuildTab` draws `osBuilder.build.run` — *Build* — and never
   * `osBuilder.build.runAgain`. "Again" told the reader to look for a label
   * that is not on the screen.
   */
  it("names the Build button as it reads after a refusal, not as it reads after a success", () => {
    expect(en.osBuilder.build.next.refused).toContain(en.osBuilder.build.run);
    expect(tr.osBuilder.build.next.refused).toContain(tr.osBuilder.build.run);
    expect(en.osBuilder.build.next.refused).not.toContain(en.osBuilder.build.runAgain);
    expect(tr.osBuilder.build.next.refused).not.toContain(tr.osBuilder.build.runAgain);
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
