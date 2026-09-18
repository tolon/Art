// The card run's data model and its sentences (round 4, task 10).
//
// What is asked here is what only this module can get wrong: an ending that
// renders through another ending's key, a bar drawn over a total nobody
// stated, a next step that names nothing, and a (kind, ending) pair whose key
// somebody forgot. The sequence itself and the session are
// `useCardOsRun.test.tsx`'s.

import { describe, expect, it } from "vitest";

import en from "@/i18n/en.json";
import type { CardOsPhaseName, CardRefusal, PartialRemoval } from "@/lib/cardOs";
import {
  cardBarFraction,
  cardCountPhrase,
  cardNextStepPhrase,
  cardPartialPhrase,
  cardPhasePhrase,
  cardPhaseTone,
  cardScratchLeftPhrase,
  cardSequence,
  cardSubPhasePhrase,
  installRefusal,
  INSTALL_REFUSED_CODE,
  type CardPhaseEnding,
  type CardPhaseKind,
} from "@/lib/cardOsRun";

/** Whether `dotted` names a string leaf in the English catalogue — the same
 *  check `phrase-keys.test.ts` makes, repeated here because a key this module
 *  invents and nobody translates renders as the dotted string itself. */
function isLeafKey(dotted: string): boolean {
  const parts = dotted.split(".");
  let node: unknown = en;
  for (const part of parts) {
    if (typeof node !== "object" || node === null) return false;
    node = (node as Record<string, unknown>)[part];
  }
  return typeof node === "string";
}

/** i18next resolves a plural key through `_one`/`_other`, so a leaf may be
 *  absent under its own name and still be translatable. */
function resolves(dotted: string): boolean {
  return isLeafKey(dotted) || isLeafKey(`${dotted}_one`) || isLeafKey(`${dotted}_other`);
}

const REFUSAL: CardRefusal = {
  code: "ART-CARD-SOURCE-UNUSABLE",
  message: "that source cannot be used",
  params: { path: "E:\\oyunlar\\kirik.lha" },
};

const EVERY_KIND: CardPhaseKind[] = [
  "tree",
  "package",
  "firstboot",
  "prepare",
  "agree",
  "build",
];

const EVERY_ENDING: CardPhaseEnding[] = [
  { state: "pending" },
  { state: "running", done: 0, total: null },
  { state: "succeeded" },
  { state: "refused", refusal: REFUSAL },
  { state: "failed", refusal: REFUSAL },
  { state: "stopped", phase: "partitions" },
  { state: "not-attempted" },
];

describe("the card run's phase sentences", () => {
  it("has a key that resolves for every (kind, ending) pair", () => {
    for (const kind of EVERY_KIND) {
      for (const ending of EVERY_ENDING) {
        const phrase = cardPhasePhrase(kind, ending);
        expect(resolves(phrase.key), `${kind}/${ending.state} → ${phrase.key}`).toBe(true);
      }
    }
  });

  // **The four endings are four keys, per kind** — the defect this guards is
  // a refusal rendered through the failed branch, which tells somebody whose
  // next step is one download that something broke.
  it("gives a refusal, a failure and a stop three different keys in the same kind", () => {
    const refused = cardPhasePhrase("build", { state: "refused", refusal: REFUSAL }).key;
    const failed = cardPhasePhrase("build", { state: "failed", refusal: REFUSAL }).key;
    const stopped = cardPhasePhrase("build", { state: "stopped", phase: "card" }).key;
    expect(new Set([refused, failed, stopped]).size).toBe(3);
  });

  it("gives two kinds two different keys for the same ending", () => {
    const tree = cardPhasePhrase("tree", { state: "succeeded" }).key;
    const build = cardPhasePhrase("build", { state: "succeeded" }).key;
    expect(tree).not.toBe(build);
  });

  it("colours a success, a refusal, a failure and a stop apart", () => {
    expect(cardPhaseTone({ state: "succeeded" })).toBe("ok");
    expect(cardPhaseTone({ state: "refused", refusal: REFUSAL })).toBe("warn");
    expect(cardPhaseTone({ state: "failed", refusal: REFUSAL })).toBe("err");
    expect(cardPhaseTone({ state: "stopped", phase: null })).toBe("warn");
    expect(cardPhaseTone({ state: "not-attempted" })).toBe("muted");
  });
});

describe("what to do about it", () => {
  it("names the image for a failure and for a stop, and nothing for a success", () => {
    const image = "E:\\amiga\\kart.img";
    const failed = cardNextStepPhrase({ state: "failed", refusal: REFUSAL }, image);
    expect(failed).not.toBeNull();
    expect(resolves(failed!.key)).toBe(true);
    expect(failed!.params?.image).toBe(image);

    const stopped = cardNextStepPhrase({ state: "stopped", phase: "partitions" }, image);
    expect(stopped).not.toBeNull();
    expect(stopped!.params?.image).toBe(image);

    expect(cardNextStepPhrase({ state: "succeeded" }, image)).toBeNull();
    expect(cardNextStepPhrase({ state: "pending" }, image)).toBeNull();
    expect(cardNextStepPhrase({ state: "not-attempted" }, image)).toBeNull();
  });

  it("gives a refusal its own next step, which is not the failure's", () => {
    const refused = cardNextStepPhrase({ state: "refused", refusal: REFUSAL }, null);
    const failed = cardNextStepPhrase({ state: "failed", refusal: REFUSAL }, null);
    expect(refused).not.toBeNull();
    expect(failed).not.toBeNull();
    expect(refused!.key).not.toBe(failed!.key);
    expect(resolves(refused!.key)).toBe(true);
  });
});

describe("the live count", () => {
  // CLAUDE.md's bar rule: a fixed width over an unknown total looks like
  // progress and carries none.
  it("says N so far and refuses a bar when the job stated no total", () => {
    const ending: CardPhaseEnding = { state: "running", done: 4812, total: null };
    const phrase = cardCountPhrase(ending);
    expect(phrase).not.toBeNull();
    expect(resolves(phrase!.key)).toBe(true);
    expect(phrase!.params?.done).toBe(4812);
    expect(cardBarFraction(ending)).toBeNull();
  });

  it("says N of M and allows a bar when it did", () => {
    const ending: CardPhaseEnding = { state: "running", done: 4812, total: 9216 };
    const phrase = cardCountPhrase(ending);
    expect(phrase!.params?.total).toBe(9216);
    expect(cardBarFraction(ending)).toBeCloseTo(4812 / 9216, 5);
  });

  it("refuses a bar for a total of zero and for a row that is not running", () => {
    expect(cardBarFraction({ state: "running", done: 0, total: 0 })).toBeNull();
    expect(cardBarFraction({ state: "succeeded" })).toBeNull();
    expect(cardCountPhrase({ state: "succeeded" })).toBeNull();
  });
});

describe("the facts every ending carries", () => {
  it("says what became of the .partial, four ways", () => {
    const partials: PartialRemoval[] = [
      { outcome: "not-created" },
      { outcome: "removed", path: "E:\\amiga\\kart.img.partial" },
      { outcome: "already-gone", path: "E:\\amiga\\kart.img.partial" },
      { outcome: "not-removed", path: "E:\\amiga\\kart.img.partial", why: "in use" },
    ];
    const keys = partials.map((partial) => cardPartialPhrase(partial).key);
    for (const key of keys) expect(resolves(key), key).toBe(true);
    expect(new Set(keys).size).toBe(4);
  });

  it("names a session folder that could not be removed, and says nothing when it went", () => {
    const left = cardScratchLeftPhrase({ path: "E:\\tmp\\card-1", why: "in use" });
    expect(left).not.toBeNull();
    expect(resolves(left!.key)).toBe(true);
    expect(left!.params?.path).toBe("E:\\tmp\\card-1");
    expect(cardScratchLeftPhrase(null)).toBeNull();
  });

  it("names every sub-phase the core can announce", () => {
    const names: CardOsPhaseName[] = ["prepare", "whdload", "card", "partitions", "check"];
    for (const name of names) {
      expect(resolves(cardSubPhasePhrase(name).key), name).toBe(true);
    }
    expect(new Set(names.map((n) => cardSubPhasePhrase(n).key)).size).toBe(5);
  });

  it("carries an install refusal under its own code, with the reasons beside it", () => {
    const refusal = installRefusal([{ refusal: "rom-unknown" }]);
    expect(refusal.code).toBe(INSTALL_REFUSED_CODE);
    expect(refusal.params.count).toBe("1");
  });
});

describe("the sequence", () => {
  const inputs = {
    release: "AmigaOS 3.9",
    updates: [
      {
        packageId: "boingbag-39-1",
        slotId: "package:boingbag-39-1",
        name: "BoingBag 3.9-1",
        file: "E:\\arsiv\\BoingBag39-1.lha",
      },
    ],
    firstBootWanted: true,
    firstBootAskPrefs: false,
    image: "E:\\amiga\\kart.img",
  };

  it("runs the tree, the updates, first boot, prepare, the agreement and the build, in that order", () => {
    const phases = cardSequence(inputs);
    expect(phases.map((phase) => phase.kind)).toEqual([
      "tree",
      "package",
      "firstboot",
      "prepare",
      "agree",
      "build",
    ]);
    // The card's tree is always built: the session folder is ART's own and is
    // always empty, so there is no update mode here to skip it.
    expect(phases[0].name).toBe("AmigaOS 3.9");
    expect(phases[1].packageId).toBe("boingbag-39-1");
    expect(phases[1].folder).toBe("E:\\arsiv");
    expect(phases[5].name).toBe("E:\\amiga\\kart.img");
  });

  it("leaves first boot out when it is not wanted, and keeps the card's own three", () => {
    const phases = cardSequence({ ...inputs, firstBootWanted: false, updates: [] });
    expect(phases.map((phase) => phase.kind)).toEqual(["tree", "prepare", "agree", "build"]);
  });

  it("gives every row an id of its own", () => {
    const phases = cardSequence(inputs);
    expect(new Set(phases.map((phase) => phase.id)).size).toBe(phases.length);
  });
});
