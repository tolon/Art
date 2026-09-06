// The wire between `core/firstboot/report.rs` and this module.
//
// `StepOutcome` and `Ending` are read on the host from a file the Amiga wrote
// (§4.3: endings stay distinct), so a mapper here that collapsed two of them
// onto one key would tell the user a confidently wrong sentence about a card
// that never finished booting. This checks the four kinds of each map to
// four distinct catalogue keys, and — the `amigainstall.test.ts` precedent —
// that the Rust source still tags both enums `kebab-case` so the `kind`
// strings this file assumes really are what serde writes.

import { describe, expect, it } from "vitest";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";

import {
  endingPhrase,
  fatMountPhrase,
  stepOutcomePhrase,
  type Ending,
  type FatMount,
  type StepOutcome,
} from "@/lib/firstboot";

const REPORT = readFileSync(
  resolve(__dirname, "..", "..", "src-tauri", "src", "core", "firstboot", "report.rs"),
  "utf8"
);

describe("report.rs still tags both enums kebab-case", () => {
  it("StepOutcome", () => {
    expect(REPORT).toMatch(/#\[serde\(tag = "kind", rename_all = "kebab-case"\)\]\s*\r?\n\s*pub enum StepOutcome/);
  });

  it("Ending", () => {
    expect(REPORT).toMatch(/#\[serde\(rename_all = "kebab-case"\)\]\s*\r?\n\s*pub enum Ending/);
  });
});

describe("stepOutcomePhrase", () => {
  const outcomes: StepOutcome[] = [
    { kind: "ok" },
    { kind: "skipped", reason: "not-3.9" },
    { kind: "refused", rc: 20 },
    { kind: "unfinished" },
  ];

  it("gives every kind its own key", () => {
    const keys = outcomes.map((o) => stepOutcomePhrase(o).key);
    expect(new Set(keys).size).toBe(outcomes.length);
  });

  it("carries the reason and the code through", () => {
    expect(stepOutcomePhrase({ kind: "skipped", reason: "not-3.9" }).params).toEqual({
      reason: "not-3.9",
    });
    expect(stepOutcomePhrase({ kind: "refused", rc: 20 }).params).toEqual({ rc: 20 });
  });
});

describe("endingPhrase", () => {
  const endings: Ending[] = ["not-booted", "unfinished", "done-all", "done-partial"];

  it("gives every value its own key", () => {
    const keys = endings.map((e) => endingPhrase(e).key);
    expect(new Set(keys).size).toBe(endings.length);
  });
});

describe("fatMountPhrase", () => {
  it("says the mount is available with no params", () => {
    const phrase = fatMountPhrase({ kind: "available" });
    expect(phrase.key).toBe("firstboot.fat.available");
    expect(phrase.params).toBeUndefined();
  });

  it("names what is missing when it is not", () => {
    const fat: FatMount = { kind: "unavailable", needs: "fat95" };
    const phrase = fatMountPhrase(fat);
    expect(phrase.key).toBe("firstboot.fat.unavailable");
    expect(phrase.params).toEqual({ needs: "fat95" });
  });
});
