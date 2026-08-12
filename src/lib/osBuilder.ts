// What the OS Builder can say about a profile before anything is built.
//
// The screen's job in this slice is to be **honest about what the user is
// getting into**: which licence model this distribution has, what they have to
// supply themselves, whether the ROM they picked belongs with it, and whether
// their card is big enough. ART writes no card yet — the adaptation checklist
// is blocked on inspecting a real distribution's layout by hand, which the
// research parks deliberately rather than guesses at
// (`ART-research-distro-profiles.md` §8.2).
//
// Pure, because these are the rules worth pinning: the licence sentence a
// profile shows is not decoration, and "you may not build this yet" must not
// quietly become "you may".

import type { Phrase } from "@/lib/phrase";
import type { DistroProfile, SuppliedImage } from "@/lib/distro";

/**
 * The one sentence a profile card leads with.
 *
 * Both free distributions ship the same line — *if you paid for this, ask for
 * your money back* — because a card-reseller economy grew around them. ART's
 * whole answer to that is being the thing that builds the card yourself, so the
 * licence model is the first thing said and not a footnote.
 */
export function licenceSentence(profile: DistroProfile): Phrase {
  switch (profile.licence_model) {
    case "free-grey":
      return { key: "distro.licence.freeGrey", params: { name: profile.name } };
    case "user-licensed":
      return { key: "distro.licence.userLicensed", params: { name: profile.name } };
    case "art-baseline":
      return { key: "distro.licence.artBaseline" };
  }
}

/** What the user has to bring, in the order they will need it. */
export function whatYouSupply(profile: DistroProfile): Phrase[] {
  const items: Phrase[] = [];

  if (profile.acquisition === "user-supplies-image") {
    items.push({ key: "distro.supply.image", params: { name: profile.name } });
  } else {
    items.push({ key: "distro.supply.media", params: { base: baseOsName(profile) } });
  }

  if (profile.rom_requirement) {
    items.push({
      key: "distro.supply.rom",
      params: { family: profile.rom_requirement.family },
    });
  }

  items.push({ key: "distro.supply.card", params: { gb: profile.min_card_gb } });
  return items;
}

function baseOsName(profile: DistroProfile): string {
  switch (profile.base_os) {
    case "os32":
      return "AmigaOS 3.2";
    case "os39":
      return "AmigaOS 3.9";
    case "none-declared":
      return "";
  }
}

/**
 * Whether ART should offer to do anything with this profile yet.
 *
 * `available: false` is not a placeholder — it is the honest state of every
 * entry in the registry today, and the screen says so rather than presenting a
 * button that would do nothing (§96).
 */
export function canBuild(profile: DistroProfile): boolean {
  return profile.available;
}

/**
 * What is wrong with the file the user pointed at, if anything.
 *
 * Cheap checks only, and deliberately: this is before the user has chosen their
 * hardware, and hashing seventeen gigabytes to tell them they picked a folder
 * would be a long wait for the obvious.
 */
export function imageProblem(
  profile: DistroProfile,
  image: SuppliedImage | null
): Phrase | null {
  if (!image) return null;
  if (!image.is_file) {
    return { key: "distro.image.notAFile", params: { path: image.path } };
  }
  if (image.size_bytes === 0) {
    return { key: "distro.image.empty", params: { path: image.path } };
  }
  // A raw image smaller than the card the distribution asks for is either the
  // wrong file or a truncated download — both worth saying before a write that
  // would fail two thirds of the way through.
  if (profile.image_format === "raw-img") {
    const gb = image.size_bytes / (1024 * 1024 * 1024);
    if (gb < 1) {
      return {
        key: "distro.image.tooSmallForRaw",
        params: { name: profile.name, gb: Math.round(gb * 100) / 100 },
      };
    }
  }
  return null;
}

/** The card size a profile needs, in bytes — for the size check. */
export function minCardBytes(profile: DistroProfile): number {
  return profile.min_card_gb * 1024 * 1024 * 1024;
}
