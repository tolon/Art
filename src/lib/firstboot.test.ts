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
  rehearsalNextStepPhrase,
  rehearsalOutcomePhrase,
  rehearsalTone,
  stepOutcomePhrase,
  FIRSTBOOT_REHEARSAL_EVENT,
  type Ending,
  type FatMount,
  type FirstBootReport,
  type RehearsalOutcome,
  type StepOutcome,
} from "@/lib/firstboot";

const REPORT = readFileSync(
  resolve(__dirname, "..", "..", "src-tauri", "src", "core", "firstboot", "report.rs"),
  "utf8"
);

const REHEARSE = readFileSync(
  resolve(__dirname, "..", "..", "src-tauri", "src", "core", "amigainstall", "rehearse.rs"),
  "utf8"
);

const COMMAND = readFileSync(
  resolve(__dirname, "..", "..", "src-tauri", "src", "commands", "firstboot.rs"),
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

// ---------------------------------------------------------------------------
// Rehearsing under WinUAE
// ---------------------------------------------------------------------------

const EMPTY_REPORT: FirstBootReport = {
  version: 1,
  system: null,
  steps: [],
  ending: "unfinished",
  fatCopyFailed: false,
  rebootRequestedBy: null,
  unknown: [],
};

const REHEARSAL_ENDINGS: RehearsalOutcome[] = [
  { kind: "finished", report: EMPTY_REPORT },
  { kind: "step-refused", report: EMPTY_REPORT },
  { kind: "timed-out", waited: { secs: 90, nanos: 0 }, report: EMPTY_REPORT },
  { kind: "emulator-closed", waited: { secs: 12, nanos: 0 }, report: EMPTY_REPORT },
];

/** `EmulatorClosed` → `emulator-closed`, which is what `rename_all` does. */
function kebab(variant: string): string {
  return variant.replace(/([a-z0-9])([A-Z])/g, "$1-$2").toLowerCase();
}

describe("rehearse.rs still tags RehearsalOutcome kebab-case, and the union knows every variant", () => {
  it("tags it", () => {
    expect(REHEARSE).toMatch(
      /#\[serde\(tag = "kind", rename_all = "kebab-case"\)\]\s*\r?\n\s*pub enum RehearsalOutcome/
    );
  });

  it("declares every variant the Rust has", () => {
    const start = REHEARSE.indexOf("pub enum RehearsalOutcome {");
    const body = REHEARSE.slice(start, REHEARSE.indexOf("\n}", start));
    const rust = body
      .split("\n")
      .map((line) => line.trim())
      .filter((line) => /^[A-Z][A-Za-z0-9]*\s*(\{|,|$)/.test(line))
      .map((line) => kebab(line.replace(/[^A-Za-z0-9].*$/, "")));
    expect(rust).toEqual(["finished", "step-refused", "timed-out", "emulator-closed"]);
    expect([...REHEARSAL_ENDINGS.map((o) => o.kind)].sort()).toEqual([...rust].sort());
  });

  it("listens on the event the command emits", () => {
    expect(COMMAND).toContain(`pub const REHEARSAL_EVENT: &str = "${FIRSTBOOT_REHEARSAL_EVENT}";`);
  });
});

describe("the four rehearsal endings stay four sentences", () => {
  // "Nobody answered — watch the window next time" is the wrong advice for a
  // window the owner shut themselves, and "every step ran" said about a step
  // that refused is the confident wrong sentence this project pays most for.
  it("gives every ending its own key, and its own next step", () => {
    const said = REHEARSAL_ENDINGS.map((o) => rehearsalOutcomePhrase(o).key);
    expect(new Set(said).size).toBe(REHEARSAL_ENDINGS.length);
    const next = REHEARSAL_ENDINGS.map((o) => rehearsalNextStepPhrase(o).key);
    expect(new Set(next).size).toBe(REHEARSAL_ENDINGS.length);
    expect(said.some((key, i) => key === next[i])).toBe(false);
  });

  it("says how long a timeout or a closed window waited", () => {
    expect(rehearsalOutcomePhrase(REHEARSAL_ENDINGS[2]).params).toEqual({ seconds: 90 });
    expect(rehearsalOutcomePhrase(REHEARSAL_ENDINGS[3]).params).toEqual({ seconds: 12 });
    expect(rehearsalOutcomePhrase(REHEARSAL_ENDINGS[0]).params).toBeUndefined();
  });

  it("colours a refusal as an error and an unanswered run as a warning", () => {
    expect(rehearsalTone(REHEARSAL_ENDINGS[0])).toBe("ok");
    expect(rehearsalTone(REHEARSAL_ENDINGS[1])).toBe("err");
    expect(rehearsalTone(REHEARSAL_ENDINGS[2])).toBe("warn");
    expect(rehearsalTone(REHEARSAL_ENDINGS[3])).toBe("warn");
  });
});
