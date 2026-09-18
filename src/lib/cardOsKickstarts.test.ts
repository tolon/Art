// The Kickstart agreement step's own sentences (round 4, task 9).
//
// Pure TS, no DOM: `KickstartAgreement.tsx` calls these and renders what
// they return with `t()`. What this file proves is that a row's shown text
// tracks the proposal's own shape — which offers and `.RTB` sources can be
// ticked, and which cannot and say why — without booting a translator
// (`phrase-keys.test.ts` is the one that checks every key actually resolves
// in the catalogue).

import { describe, expect, it } from "vitest";

import type { KickstartOffer } from "@/lib/gameindex";
import type { ProposedKickstart, RtbSource } from "@/lib/cardOs";
import {
  canAgree,
  offerPhrase,
  rtbPackageLabel,
  rtbPhrase,
  summaryPhrase,
  titlesPhrase,
} from "@/lib/cardOsKickstarts";

const SUPPLIED: KickstartOffer = {
  outcome: "supplied",
  wanted: { name: "kick40068.A1200", crc16: 1234, size: 524_288 },
  by: { path: "E:\\roms\\kick40068.A1200", name: "kick40068.A1200", sizeDisagrees: null },
};
const ENCRYPTED: KickstartOffer = {
  outcome: "encrypted",
  wanted: { name: "kick40068.A1200", crc16: 1234, size: 524_288 },
  candidates: ["E:\\afe\\rom\\kick40068.A1200"],
};
const NOT_HERE: KickstartOffer = {
  outcome: "not-here",
  wanted: { name: "kick40068.A1200", crc16: 1234, size: 524_288 },
};
const UNMATCHABLE: KickstartOffer = {
  outcome: "unmatchable",
  wanted: { name: "kick13.A500", crc16: null, size: null },
};

const RTB_LOOSE: RtbSource = { kind: "loose", path: "E:\\material\\kick40068.A1200.RTB" };
const RTB_ARCHIVE: RtbSource = {
  kind: "in-archive",
  archive: "E:\\material\\skick346.lha",
  member: "kick40068.A1200.RTB",
};
const RTB_MISSING_SKICK: RtbSource = { kind: "missing", getFrom: "aminet-skick346" };
const RTB_MISSING_7COG: RtbSource = { kind: "missing", getFrom: "whdload-seven-cities-of-gold" };

function item(over: Partial<ProposedKickstart> = {}): ProposedKickstart {
  return {
    name: "kick40068.A1200",
    titles: ["Games/Turrican/Turrican.slave"],
    titlesMore: 0,
    offer: SUPPLIED,
    rtb: RTB_LOOSE,
    ...over,
  };
}

describe("canAgree", () => {
  it("is true for a supplied offer with a loose or in-archive RTB", () => {
    expect(canAgree(item({ offer: SUPPLIED, rtb: RTB_LOOSE }))).toBe(true);
    expect(canAgree(item({ offer: SUPPLIED, rtb: RTB_ARCHIVE }))).toBe(true);
  });

  it("is false when the offer is not supplied, whatever the RTB says", () => {
    expect(canAgree(item({ offer: ENCRYPTED, rtb: RTB_LOOSE }))).toBe(false);
    expect(canAgree(item({ offer: NOT_HERE, rtb: RTB_LOOSE }))).toBe(false);
    expect(canAgree(item({ offer: UNMATCHABLE, rtb: RTB_LOOSE }))).toBe(false);
  });

  it("is false when the RTB is missing, even for a supplied offer", () => {
    expect(canAgree(item({ offer: SUPPLIED, rtb: RTB_MISSING_SKICK }))).toBe(false);
    expect(canAgree(item({ offer: SUPPLIED, rtb: RTB_MISSING_7COG }))).toBe(false);
  });
});

describe("offerPhrase", () => {
  it("names the file for a supplied offer", () => {
    const phrase = offerPhrase(SUPPLIED);
    expect(phrase.key).toBe("cardKickstart.offer.supplied");
    expect(phrase.params).toMatchObject({ rom: "kick40068.A1200" });
  });

  it("counts the candidates for an encrypted offer", () => {
    const phrase = offerPhrase(ENCRYPTED);
    expect(phrase.key).toBe("cardKickstart.offer.encrypted");
    expect(phrase.params).toMatchObject({ count: 1 });
  });

  it("has its own key for not-here and unmatchable", () => {
    expect(offerPhrase(NOT_HERE).key).toBe("cardKickstart.offer.notHere");
    expect(offerPhrase(UNMATCHABLE).key).toBe("cardKickstart.offer.unmatchable");
  });
});

describe("rtbPhrase", () => {
  it("names where a found RTB is", () => {
    expect(rtbPhrase(RTB_LOOSE).key).toBe("cardKickstart.rtb.loose");
    expect(rtbPhrase(RTB_LOOSE).params).toMatchObject({ path: RTB_LOOSE.path });
    expect(rtbPhrase(RTB_ARCHIVE).key).toBe("cardKickstart.rtb.inArchive");
    expect(rtbPhrase(RTB_ARCHIVE).params).toMatchObject({ archive: RTB_ARCHIVE.archive });
  });

  it("names the package a missing RTB can be had from", () => {
    const skick = rtbPhrase(RTB_MISSING_SKICK);
    expect(skick.key).toBe("cardKickstart.rtb.missing");
    expect(skick.params?.package).toBe(rtbPackageLabel("aminet-skick346"));

    const seven = rtbPhrase(RTB_MISSING_7COG);
    expect(seven.params?.package).toBe(rtbPackageLabel("whdload-seven-cities-of-gold"));
  });
});

describe("rtbPackageLabel", () => {
  it("names both packages ART's own catalogue carries", () => {
    expect(rtbPackageLabel("aminet-skick346")).toMatch(/skick346/);
    expect(rtbPackageLabel("whdload-seven-cities-of-gold")).toMatch(/7CitiesOfGold/);
  });
});

describe("titlesPhrase", () => {
  it("lists the titles when there are no more than shown", () => {
    const phrase = titlesPhrase(item({ titles: ["A/A.slave", "B/B.slave"], titlesMore: 0 }));
    expect(phrase.key).toBe("cardKickstart.titles.list");
    expect(phrase.params).toMatchObject({ list: "A/A.slave, B/B.slave" });
  });

  it("says how many more when the proposal truncated the list", () => {
    const phrase = titlesPhrase(item({ titles: ["A/A.slave"], titlesMore: 5 }));
    expect(phrase.key).toBe("cardKickstart.titles.withMore");
    expect(phrase.params).toMatchObject({ list: "A/A.slave", count: 5 });
  });
});

describe("summaryPhrase", () => {
  it("says nothing needs one when the proposal is empty", () => {
    expect(summaryPhrase(0, 0).key).toBe("cardKickstart.summary.none");
  });

  it("says how many of the total will be placed, agreed or not", () => {
    expect(summaryPhrase(0, 3).key).toBe("cardKickstart.summary.line");
    expect(summaryPhrase(0, 3).params).toMatchObject({ agreed: 0, total: 3 });
    expect(summaryPhrase(2, 3).params).toMatchObject({ agreed: 2, total: 3 });
    expect(summaryPhrase(3, 3).params).toMatchObject({ agreed: 3, total: 3 });
  });
});
