import { describe, expect, it } from "vitest";

import {
  DEFAULT_COMPONENTS,
  DEFAULT_CARD,
  DEFAULT_MEDIA,
  DEFAULT_PACKAGES,
  type BuildSession,
} from "./buildSession";
import { readiness, stepLabelKey, stepsFor, STEP_IDS } from "./buildSteps";

function sessionWith(over: Partial<BuildSession> = {}): BuildSession {
  return {
    kind: "install",
    media: DEFAULT_MEDIA,
    rom: { path: null },
    release: "AmigaOS 3.2",
    tree: { root: null, builtHere: false },
    components: DEFAULT_COMPONENTS,
    packages: DEFAULT_PACKAGES,
    card: DEFAULT_CARD,
    ...over,
  };
}

describe("stepsFor", () => {
  it("gives the install job its own steps and not the card's", () => {
    expect(stepsFor("install")).toEqual(["hedef", "kaynak", "paketler", "amiga-kurulum"]);
  });

  it("gives the card job the card step and none of the install's", () => {
    expect(stepsFor("boot-card")).toEqual(["hedef", "kart"]);
  });

  it("gives volume preparation its own", () => {
    expect(stepsFor("prepare-volumes")).toEqual(["hedef", "birimler"]);
  });

  it("leaves the unbuilt distro job at the picker", () => {
    expect(stepsFor("distro")).toEqual(["hedef"]);
  });

  it("always begins at the picker, whatever the kind", () => {
    for (const kind of ["distro", "boot-card", "install", "prepare-volumes"] as const) {
      expect(stepsFor(kind)[0]).toBe("hedef");
    }
  });

  it("offers no step that is not a real step", () => {
    for (const kind of ["distro", "boot-card", "install", "prepare-volumes"] as const) {
      for (const step of stepsFor(kind)) {
        expect(STEP_IDS).toContain(step);
      }
    }
  });
});

describe("readiness", () => {
  it("says a packages step with no tree must ask", () => {
    expect(readiness(sessionWith(), "paketler")).toBe("asks");
  });

  it("says a packages step with a tree is ready", () => {
    const s = sessionWith({ tree: { root: "E:\\dist", builtHere: true } });
    expect(readiness(s, "paketler")).toBe("ready");
  });

  it("says the Amiga-side install must ask without a tree, and is ready with one", () => {
    expect(readiness(sessionWith(), "amiga-kurulum")).toBe("asks");
    const s = sessionWith({ tree: { root: "E:\\dist", builtHere: false } });
    expect(readiness(s, "amiga-kurulum")).toBe("ready");
  });

  it("never makes the first step ask — it is where a build begins", () => {
    expect(readiness(sessionWith(), "hedef")).toBe("ready");
  });

  it("treats an empty string as no tree at all", () => {
    // A cleared field writes "", and sending "" to the backend as a path is
    // how a refusal ends up naming a folder nobody chose.
    const s = sessionWith({ tree: { root: "", builtHere: false } });
    expect(readiness(s, "paketler")).toBe("asks");
  });

  it("does not make a step ask for something it does not use", () => {
    // `kaynak`, `kart` and `birimler` own their own inputs and ask for them
    // inline, exactly as they do today. A tree they never read must not gate
    // them.
    const s = sessionWith();
    expect(readiness(s, "kaynak")).toBe("ready");
    expect(readiness(s, "kart")).toBe("ready");
    expect(readiness(s, "birimler")).toBe("ready");
  });
});

describe("stepLabelKey", () => {
  it("answers a key for every step, never a sentence", () => {
    for (const step of STEP_IDS) {
      const key = stepLabelKey(step);
      expect(key.startsWith("osBuilder.step.")).toBe(true);
      expect(key).not.toContain(" ");
    }
  });
});

describe("readiness, when ART has looked at the folder (ART-199)", () => {
  const withTree = sessionWith({ tree: { root: "E:\\dist", builtHere: false } });

  it("says the folder is the wrong one when ART has looked and it is not a tree", () => {
    // The owner pointed the Amiga-side step at their own AmigaOS folder. The
    // step said ready, and the refusal arrived on the button.
    expect(readiness(withTree, "paketler", false)).toBe("wrong-folder");
    expect(readiness(withTree, "amiga-kurulum", false)).toBe("wrong-folder");
  });

  it("is ready once ART has looked and it is a tree", () => {
    expect(readiness(withTree, "paketler", true)).toBe("ready");
  });

  it("does not accuse a folder ART has not looked at yet", () => {
    // `null` is "not asked". Rendering "wrong folder" while the answer is
    // still in flight would be a confident wrong sentence of its own.
    expect(readiness(withTree, "paketler", null)).toBe("ready");
  });

  it("still asks first when there is no folder at all", () => {
    // No folder beats a bad one: "pick one" is the useful sentence, and
    // "that is not a tree" about nothing would be nonsense.
    expect(readiness(sessionWith(), "paketler", false)).toBe("asks");
  });

  it("never accuses a step that does not read a tree", () => {
    expect(readiness(withTree, "kart", false)).toBe("ready");
    expect(readiness(withTree, "kaynak", false)).toBe("ready");
  });
});
