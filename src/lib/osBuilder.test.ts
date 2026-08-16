import { describe, expect, it } from "vitest";

import {
  canBuild,
  imageProblem,
  licenceSentence,
  minCardBytes,
  whatYouSupply,
} from "@/lib/osBuilder";
import type { DistroProfile, SuppliedImage } from "@/lib/distro";

function profile(overrides: Partial<DistroProfile> = {}): DistroProfile {
  return {
    id: "caffeineos",
    name: "CaffeineOS",
    homepage: "https://caffeineos.neocities.org/",
    licence_model: "free-grey",
    acquisition: "user-supplies-image",
    image_format: "raw-img",
    min_card_gb: 32,
    base_os: "os39",
    rom_requirement: { family: "3.1", drop_name: "kick.rom" },
    default_cmdline_tokens: ["enable_cache"],
    multiboot: { config_set_name: "caffeineos" },
    packages: [],
    post_install_notes: [],
    available: false,
    ...overrides,
  };
}

function image(overrides: Partial<SuppliedImage> = {}): SuppliedImage {
  return {
    path: "F:\\downloads\\caffeineos.img",
    size_bytes: 17 * 1024 * 1024 * 1024,
    is_file: true,
    ...overrides,
  };
}

describe("licenceSentence", () => {
  it("leads with the licence, not with the features", () => {
    // Both free distributions ship "if you paid for this, ask for your money
    // back", because a reseller economy grew around them. ART's answer is to
    // be the thing that builds the card, so this is said first.
    expect(licenceSentence(profile()).key).toBe("distro.licence.freeGrey");
    expect(licenceSentence(profile({ licence_model: "user-licensed" })).key).toBe(
      "distro.licence.userLicensed"
    );
    expect(licenceSentence(profile({ licence_model: "art-baseline" })).key).toBe(
      "distro.licence.artBaseline"
    );
  });

  it("names the distribution, so the sentence is about a thing", () => {
    expect(licenceSentence(profile()).params).toEqual({ name: "CaffeineOS" });
  });
});

describe("whatYouSupply", () => {
  it("asks for the image when the user downloads it themselves", () => {
    const keys = whatYouSupply(profile()).map((phrase) => phrase.key);
    expect(keys).toContain("distro.supply.image");
    expect(keys).not.toContain("distro.supply.media");
  });

  it("asks for the OS media when ART builds it", () => {
    const keys = whatYouSupply(
      profile({ acquisition: "art-builds", base_os: "os32" })
    ).map((phrase) => phrase.key);
    expect(keys).toContain("distro.supply.media");
    expect(keys).not.toContain("distro.supply.image");
  });

  it("names the ROM family, because the wrong one is a card that does not boot", () => {
    const rom = whatYouSupply(profile()).find((p) => p.key === "distro.supply.rom");
    expect(rom?.params).toEqual({ family: "3.1" });
  });

  it("says how big a card is needed", () => {
    const card = whatYouSupply(profile()).find((p) => p.key === "distro.supply.card");
    expect(card?.params).toEqual({ gb: 32 });
  });

  it("says nothing about a ROM for a profile that declares none", () => {
    const keys = whatYouSupply(
      profile({ rom_requirement: null, base_os: "none-declared" })
    ).map((p) => p.key);
    expect(keys).not.toContain("distro.supply.rom");
  });
});

describe("canBuild", () => {
  it("is false for every profile as things stand", () => {
    // Not a placeholder: the adaptation checklist is blocked on inspecting a
    // real distribution's layout by hand. The screen says so rather than
    // offering a button that would do nothing.
    expect(canBuild(profile())).toBe(false);
  });

  it("follows the registry rather than guessing", () => {
    expect(canBuild(profile({ available: true }))).toBe(true);
  });
});

describe("imageProblem", () => {
  it("says nothing before the user has picked anything", () => {
    expect(imageProblem(profile(), null)).toBeNull();
  });

  it("is happy with a real image of a plausible size", () => {
    expect(imageProblem(profile(), image())).toBeNull();
  });

  it("names a folder picked by mistake", () => {
    const problem = imageProblem(profile(), image({ is_file: false }));
    expect(problem?.key).toBe("distro.image.notAFile");
  });

  it("names an empty file", () => {
    expect(imageProblem(profile(), image({ size_bytes: 0 }))?.key).toBe(
      "distro.image.empty"
    );
  });

  it("catches a raw image far too small to be one", () => {
    // A truncated download, or the wrong file entirely — worth saying before a
    // write that would fail two thirds of the way through.
    const problem = imageProblem(profile(), image({ size_bytes: 4 * 1024 * 1024 }));
    expect(problem?.key).toBe("distro.image.tooSmallForRaw");
  });

  it("does not apply the raw-image size rule to a recipe", () => {
    // An ART Baseline profile has no image to point at, so a small file here
    // is somebody's OS media and none of this check's business.
    expect(
      imageProblem(
        profile({ image_format: "build-recipe" }),
        image({ size_bytes: 4 * 1024 * 1024 })
      )
    ).toBeNull();
  });
});

describe("minCardBytes", () => {
  it("turns the declared card size into bytes", () => {
    expect(minCardBytes(profile())).toBe(32 * 1024 * 1024 * 1024);
  });
});
