import { describe, expect, it } from "vitest";

import {
  colourFor,
  DEFAULT_COLOUR_RULES,
  isUsableRuleList,
  type ColourRule,
} from "@/lib/colourRules";

describe("the shipped defaults", () => {
  it("tell apart the three things ART actually does with a file", () => {
    // Walk into it, unpack it, identify it — which is why these are the three
    // rules and not thirty.
    const container = colourFor("Lotus.adf", false, DEFAULT_COLOUR_RULES);
    const archive = colourFor("Turrican.lha", false, DEFAULT_COLOUR_RULES);
    const rom = colourFor("kick31.rom", false, DEFAULT_COLOUR_RULES);

    expect(container).toBeTruthy();
    expect(archive).toBeTruthy();
    expect(rom).toBeTruthy();
    expect(new Set([container, archive, rom]).size).toBe(3);
  });

  it("cover every container kind the commander can open", () => {
    for (const name of [
      "a.adf",
      "a.adz",
      "a.dms",
      "a.hdf",
      "a.hda",
      "a.iso",
      "a.d64",
      "a.d71",
      "a.d81",
      "a.t64",
    ]) {
      expect(colourFor(name, false, DEFAULT_COLOUR_RULES), name).toBeTruthy();
    }
  });

  it("leave an ordinary file alone", () => {
    // A row no rule claims keeps the colour the built-in classification gave
    // it, so an empty rule list changes nothing at all.
    expect(colourFor("Readme", false, DEFAULT_COLOUR_RULES)).toBeNull();
    expect(colourFor("notes.txt", false, DEFAULT_COLOUR_RULES)).toBeNull();
  });

  it("match whatever the case", () => {
    expect(colourFor("LOTUS.ADF", false, DEFAULT_COLOUR_RULES)).toBeTruthy();
  });
});

describe("colourFor", () => {
  const rules: ColourRule[] = [
    { label: "one adf", patterns: "Lotus*.adf", colour: "#ff0000" },
    { label: "containers", patterns: "*.adf;*.hdf", colour: "#00ff00" },
  ];

  it("takes the first matching rule, so the order in Settings means something", () => {
    // A user picking one `.adf` out of the rest puts that rule above the
    // container rule, exactly as they would in Total Commander.
    expect(colourFor("LotusII.adf", false, rules)).toBe("#ff0000");
    expect(colourFor("Turrican.adf", false, rules)).toBe("#00ff00");
  });

  it("splits a rule's masks on semicolons and trims them", () => {
    const spaced: ColourRule[] = [
      { label: "x", patterns: " *.zip ; *.7z ", colour: "#123456" },
    ];
    expect(colourFor("a.zip", false, spaced)).toBe("#123456");
    expect(colourFor("a.7z", false, spaced)).toBe("#123456");
    expect(colourFor("a.rar", false, spaced)).toBeNull();
  });

  it("never colours a directory", () => {
    // Directories are chrome: `[Name]`, always first, always the pane's own
    // colour. A rule catching one would make the folders-first ordering
    // harder to see, not easier.
    expect(colourFor("Games.adf", true, DEFAULT_COLOUR_RULES)).toBeNull();
  });

  it("skips a rule with no colour or no pattern rather than matching nothing", () => {
    const broken: ColourRule[] = [
      { label: "blank colour", patterns: "*.adf", colour: "  " },
      { label: "blank pattern", patterns: " ; ", colour: "#abcdef" },
      { label: "real", patterns: "*.adf", colour: "#fedcba" },
    ];
    expect(colourFor("a.adf", false, broken)).toBe("#fedcba");
  });

  it("finds nothing in an empty list", () => {
    expect(colourFor("a.adf", false, [])).toBeNull();
  });
});

describe("isUsableRuleList", () => {
  it("accepts the defaults", () => {
    expect(isUsableRuleList(DEFAULT_COLOUR_RULES)).toBe(true);
    expect(isUsableRuleList([])).toBe(true);
  });

  it("rejects anything else, so a broken file falls back to the defaults", () => {
    // "My colours disappeared" reads as a bug; "my colours are the stock ones
    // again" reads as what it is.
    expect(isUsableRuleList(null)).toBe(false);
    expect(isUsableRuleList({})).toBe(false);
    expect(isUsableRuleList([{ patterns: "*.adf" }])).toBe(false);
    expect(isUsableRuleList([{ patterns: 1, colour: "#fff", label: "x" }])).toBe(false);
  });
});
