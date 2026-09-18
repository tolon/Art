// The Kickstart agreement step's own sentences (round 4, task 9).
//
// `src/lib` never renders a string: every function here returns a `Phrase`
// — a catalogue key plus its parameters — and `KickstartAgreement.tsx` calls
// `t()` on it. Nothing here decides *whether* an image is available: that
// answer already arrived as `KickstartProposal` from `card_os_prepare`
// (`core::cardos::kickstarts::propose_kickstarts`). This module only decides
// which sentence says what the core already found, and — the one piece of
// logic that belongs here rather than in the component — which rows the
// owner's 2026-08-21 rule allows to be ticked at all.
//
// **The RTB gate is load-bearing.** WHDLoad needs both the decoded image and
// its `.RTB` beside it in `Devs/Kickstarts/`; placing one without the other
// is a Kickstart WHDLoad still refuses to use. `canAgree` is the one place
// that rule lives on this screen, and `place_agreed`
// (`core/cardos/kickstarts.rs::check_agreed`) enforces the same rule again
// on the Rust side before a byte is written — belt and braces, not "the
// screen decides".

import type { KickstartOffer } from "@/lib/gameindex";
import type { ProposedKickstart, RtbPackage, RtbSource } from "@/lib/cardOs";
import type { Phrase } from "@/lib/phrase";

/**
 * Where a missing `.RTB` can be had, named plainly — the same kind of value
 * as `driverPhrase`'s `from`/`at` in `cardOsMeasure.ts`: a real archive name
 * inserted into a translated sentence, not a sentence of its own. Matches
 * `core::cardos::kickstarts::check_agreed`'s own English names for the two
 * packages, so the refusal a build might still raise and the offer this
 * screen showed name the same thing the same way.
 */
export function rtbPackageLabel(pkg: RtbPackage): string {
  switch (pkg) {
    case "aminet-skick346":
      return "Aminet util/boot/skick346";
    case "whdload-seven-cities-of-gold":
      return "whdload.de's 7CitiesOfGold.lha";
  }
}

/**
 * What ART found for this name: the file it has, or — for the three
 * outcomes that are not `supplied` — why not and, where there is one, its
 * own next step. Four endings and they stay four sentences (CLAUDE.md's
 * rule): `encrypted` in particular is a file the user *has*, so it must
 * never read like `not-here`.
 */
export function offerPhrase(offer: KickstartOffer): Phrase {
  switch (offer.outcome) {
    case "supplied":
      return { key: "cardKickstart.offer.supplied", params: { rom: offer.by.name } };
    case "encrypted":
      return {
        key: "cardKickstart.offer.encrypted",
        params: { count: offer.candidates.length },
      };
    case "not-here":
      return { key: "cardKickstart.offer.notHere" };
    case "unmatchable":
      return { key: "cardKickstart.offer.unmatchable" };
  }
}

/**
 * Where this name's `.RTB` would come from, or — when nothing ART looked at
 * carried one — which package to add and where it comes from
 * (`RtbPackage`).
 */
export function rtbPhrase(rtb: RtbSource): Phrase {
  switch (rtb.kind) {
    case "loose":
      return { key: "cardKickstart.rtb.loose", params: { path: rtb.path } };
    case "in-archive":
      return { key: "cardKickstart.rtb.inArchive", params: { archive: rtb.archive } };
    case "missing":
      return {
        key: "cardKickstart.rtb.missing",
        params: { package: rtbPackageLabel(rtb.getFrom) },
      };
  }
}

/**
 * The titles that named this Kickstart, said in one sentence — the proposal
 * lists at most 20 and counts the rest (`titlesMore`), never drops them
 * silently.
 */
export function titlesPhrase(item: ProposedKickstart): Phrase {
  const list = item.titles.join(", ");
  return item.titlesMore > 0
    ? { key: "cardKickstart.titles.withMore", params: { list, count: item.titlesMore } }
    : { key: "cardKickstart.titles.list", params: { list } };
}

/**
 * Whether this row's checkbox can be ticked at all.
 *
 * A row whose offer is not `supplied` — ART has no file to place — or whose
 * `.RTB` is `missing` — ART has an image but nothing to place beside it —
 * cannot be ticked. Nothing is ticked by ART either way (the owner's rule,
 * 2026-08-21): this only decides which rows the *user* is allowed to tick.
 */
export function canAgree(item: ProposedKickstart): boolean {
  return item.offer.outcome === "supplied" && item.rtb.kind !== "missing";
}

/**
 * The step's own summary: how many will be placed if the build ran now, and
 * what happens to the rest. Ticking nothing is allowed — `agreedCount === 0`
 * is not an error state, and the sentence says so rather than staying
 * silent about the titles that asked for one.
 */
export function summaryPhrase(agreedCount: number, totalItems: number): Phrase {
  if (totalItems === 0) return { key: "cardKickstart.summary.none" };
  return {
    key: "cardKickstart.summary.line",
    params: { agreed: agreedCount, total: totalItems },
  };
}
