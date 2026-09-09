import { describe, expect, it } from "vitest";

import en from "@/i18n/en.json";
import tr from "@/i18n/tr.json";
import {
  DEFAULT_COMPONENTS,
  DEFAULT_CARD,
  DEFAULT_FIRSTBOOT,
  DEFAULT_MATERIAL,
  DEFAULT_MEDIA,
  DEFAULT_PACKAGES,
  type BuildSession,
} from "./buildSession";
import {
  kindLabelKey,
  readiness,
  stepLabelKey,
  stepPath,
  stepsFor,
  STEP_IDS,
} from "./buildSteps";

function sessionWith(over: Partial<BuildSession> = {}): BuildSession {
  return {
    kind: "install",
    material: DEFAULT_MATERIAL,
    media: DEFAULT_MEDIA,
    rom: { path: null },
    release: "AmigaOS 3.2",
    tree: { root: null, builtHere: false },
    components: DEFAULT_COMPONENTS,
    packages: DEFAULT_PACKAGES,
    card: DEFAULT_CARD,
    firstboot: DEFAULT_FIRSTBOOT,
    ...over,
  };
}

describe("stepsFor", () => {
  it("gives the install job hedef and its four numbered tabs, nothing else", () => {
    expect(stepsFor("install")).toEqual(["hedef", "dosyalar", "secim", "makine", "derle"]);
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

  it("offers no step that is not a real step, and no retired one", () => {
    for (const kind of ["distro", "boot-card", "install", "prepare-volumes"] as const) {
      for (const step of stepsFor(kind)) {
        expect(STEP_IDS).toContain(step);
      }
    }
    for (const retired of ["kaynak", "paketler", "amiga-kurulum", "ilk-acilis"]) {
      expect(STEP_IDS as readonly string[]).not.toContain(retired);
    }
  });
});

describe("readiness", () => {
  it("says the choice tab with no tree must ask", () => {
    expect(readiness(sessionWith(), "secim")).toBe("asks");
  });

  it("says the choice tab with a tree is ready", () => {
    const s = sessionWith({ tree: { root: "E:\\dist", builtHere: true } });
    expect(readiness(s, "secim")).toBe("ready");
  });

  it("never makes the first step ask — it is where a build begins", () => {
    expect(readiness(sessionWith(), "hedef")).toBe("ready");
  });

  it("treats an empty string as no tree at all", () => {
    // A cleared field writes "", and sending "" to the backend as a path is
    // how a refusal ends up naming a folder nobody chose.
    const s = sessionWith({ tree: { root: "", builtHere: false } });
    expect(readiness(s, "secim")).toBe("asks");
  });

  it("does not make a step ask for something it does not use", () => {
    // `dosyalar` owns its own inputs; `makine` and `derle` hold nothing yet
    // in this round; `kart` and `birimler` are the other lanes.
    const s = sessionWith();
    for (const step of ["dosyalar", "makine", "derle", "kart", "birimler"] as const) {
      expect(readiness(s, step)).toBe("ready");
    }
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

  it("resolves to a leaf in both catalogues for every step", () => {
    for (const step of STEP_IDS) {
      expect(en.osBuilder.step[step]).toEqual(expect.any(String));
      expect(tr.osBuilder.step[step]).toEqual(expect.any(String));
    }
  });
});

describe("stepPath", () => {
  it("answers the route the build tab is actually mounted at", () => {
    // The literal, because the bar's button and the route table are two
    // files that have to agree on it: a test that only compared them to each
    // other would pass on both being wrong together.
    expect(stepPath("derle")).toBe("/os-builder/derle");
  });
});

describe("kindLabelKey", () => {
  it("names each kind by its own 'what are we building' label", () => {
    expect(kindLabelKey("install")).toBe("osBuilder.what.install");
    expect(kindLabelKey("boot-card")).toBe("osBuilder.what.bootCard");
    expect(kindLabelKey("prepare-volumes")).toBe("osBuilder.what.prepareVolumes");
    expect(kindLabelKey("distro")).toBe("osBuilder.what.distro");
  });

  it("resolves to a leaf in both catalogues", () => {
    for (const kind of ["distro", "boot-card", "install", "prepare-volumes"] as const) {
      const leaf = kindLabelKey(kind).replace("osBuilder.what.", "");
      expect((en.osBuilder.what as Record<string, string>)[leaf]).toEqual(expect.any(String));
      expect((tr.osBuilder.what as Record<string, string>)[leaf]).toEqual(expect.any(String));
    }
  });
});

describe("readiness, when ART has looked at the folder (ART-199)", () => {
  const withTree = sessionWith({ tree: { root: "E:\\dist", builtHere: false } });

  it("says the folder is the wrong one when ART has looked and it is not a tree", () => {
    // The owner pointed the Amiga-side step at their own AmigaOS folder. The
    // step said ready, and the refusal arrived on the button.
    expect(readiness(withTree, "secim", false)).toBe("wrong-folder");
  });

  it("is ready once ART has looked and it is a tree", () => {
    expect(readiness(withTree, "secim", true)).toBe("ready");
  });

  it("does not accuse a folder ART has not looked at yet", () => {
    // `null` is "not asked". Rendering "wrong folder" while the answer is
    // still in flight would be a confident wrong sentence of its own.
    expect(readiness(withTree, "secim", null)).toBe("ready");
  });

  it("still asks first when there is no folder at all", () => {
    // No folder beats a bad one: "pick one" is the useful sentence, and
    // "that is not a tree" about nothing would be nonsense.
    expect(readiness(sessionWith(), "secim", false)).toBe("asks");
  });

  it("never accuses a step that does not read a tree", () => {
    expect(readiness(withTree, "kart", false)).toBe("ready");
    expect(readiness(withTree, "dosyalar", false)).toBe("ready");
  });
});

/**
 * **The banner judges the tree the tab's list works on** (round 3 fix wave,
 * Critical 1). Tab 2 reads `useChainTree` — the destination when ART found a
 * build in it, `session.tree.root` otherwise — so the readiness this function
 * answers has to be about *that* root. Judging the session's copy while the
 * list works on the destination is one tab giving two answers.
 */
describe("readiness takes the caller's own root when it is given one", () => {
  it("says ready for a chain tree the session does not hold", () => {
    // The exact defect: no session tree at all, the destination pointed at
    // an ART tree. The list answers every row against that tree, and the
    // banner used to say none had been chosen.
    expect(readiness(sessionWith(), "secim", true, "E:\\dist39")).toBe("ready");
  });

  it("asks when the caller states there is no tree, whatever the session holds", () => {
    // The other direction, and the reason `null` is not the same as omitting
    // the argument: a stale `session.tree.root` must not make the banner
    // claim a tree the tab is not working on.
    const withTree = sessionWith({ tree: { root: "E:\\stale", builtHere: false } });
    expect(readiness(withTree, "secim", true, null)).toBe("asks");
  });

  it("still accuses the caller's own root when ART has looked at it", () => {
    expect(readiness(sessionWith(), "secim", false, "E:\\amiga\\os39")).toBe("wrong-folder");
  });

  it("falls back to the session when nothing is passed, as every other caller does", () => {
    const withTree = sessionWith({ tree: { root: "E:\\dist", builtHere: false } });
    expect(readiness(withTree, "secim", true)).toBe("ready");
    expect(readiness(sessionWith(), "secim", true)).toBe("asks");
  });
});
