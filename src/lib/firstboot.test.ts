// The wire between `core/firstboot/report.rs` and this module.
//
// `StepOutcome` and `Ending` are read on the host from a file the Amiga wrote
// (§4.3: endings stay distinct), so a mapper here that collapsed two of them
// onto one key would tell the user a confidently wrong sentence about a card
// that never finished booting. This checks the four kinds of each map to
// four distinct catalogue keys, and — the `amigainstall.test.ts` precedent —
// that the Rust source still tags both enums `kebab-case` so the `kind`
// strings this file assumes really are what serde writes.

import { beforeEach, describe, expect, it, vi } from "vitest";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";

const invokeMock = vi.hoisted(() => vi.fn());
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

import {
  endingPhrase,
  fatMountPhrase,
  firstbootPreview,
  firstbootWrite,
  rebootCommandPhrase,
  rebootReportPhrase,
  rehearsalNextStepPhrase,
  rehearsalOutcomePhrase,
  rehearsalTone,
  reportSourcePhrase,
  stepOutcomePhrase,
  wizardDetail,
  wizardEditorPath,
  wizardLines,
  wizardWindowPhrase,
  WIZARD_STEP_NAME,
  FIRSTBOOT_REHEARSAL_EVENT,
  type Ending,
  type FatMount,
  type FirstBootReport,
  type RehearsalOutcome,
  type ReportSource,
  type StepOutcome,
  type WizardPlan,
} from "@/lib/firstboot";

const REPORT = readFileSync(
  resolve(__dirname, "..", "..", "src-tauri", "src", "core", "firstboot", "report.rs"),
  "utf8"
);

const PLAN = readFileSync(
  resolve(__dirname, "..", "..", "src-tauri", "src", "core", "firstboot", "plan.rs"),
  "utf8"
);

const MOD = readFileSync(
  resolve(__dirname, "..", "..", "src-tauri", "src", "core", "firstboot", "mod.rs"),
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

const CARDREAD = readFileSync(
  resolve(__dirname, "..", "..", "src-tauri", "src", "core", "firstboot", "cardread.rs"),
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

describe("cardread.rs still tags ReportSource kebab-case", () => {
  it("ReportSource", () => {
    expect(CARDREAD).toMatch(/#\[serde\(rename_all = "kebab-case"\)\]\s*\r?\n\s*pub enum ReportSource/);
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

describe("reportSourcePhrase", () => {
  it("gives the two real sources their own key, and none to a card with neither copy", () => {
    const sources: ReportSource[] = ["fat", "amiga-volume", "none"];
    const phrases = sources.map((s) => reportSourcePhrase(s));
    expect(phrases[0]?.key).toBe("firstboot.report.source.fat");
    expect(phrases[1]?.key).toBe("firstboot.report.source.amigaVolume");
    expect(phrases[2]).toBeNull();
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
  rebootUnavailable: false,
  restartedAfterRequest: false,
  waitingIn: null,
  unknown: [],
};

const REHEARSAL_ENDINGS: RehearsalOutcome[] = [
  { kind: "finished", report: EMPTY_REPORT },
  { kind: "step-refused", report: EMPTY_REPORT },
  { kind: "timed-out", waited: { secs: 90, nanos: 0 }, report: EMPTY_REPORT },
  { kind: "emulator-closed", waited: { secs: 12, nanos: 0 }, report: EMPTY_REPORT },
  {
    kind: "wrote-without-stopping",
    waited: { secs: 40, nanos: 0 },
    written: 170_328_064,
    ceiling: 67_108_864,
    report: EMPTY_REPORT,
  },
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
    expect(rust).toEqual([
      "finished",
      "step-refused",
      "timed-out",
      "emulator-closed",
      "wrote-without-stopping",
    ]);
    expect([...REHEARSAL_ENDINGS.map((o) => o.kind)].sort()).toEqual([...rust].sort());
  });

  it("listens on the event the command emits", () => {
    expect(COMMAND).toContain(`pub const REHEARSAL_EVENT: &str = "${FIRSTBOOT_REHEARSAL_EVENT}";`);
  });
});

describe("the five rehearsal endings stay five sentences", () => {
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

  // ART-294: the runaway is the one ending with a measurement, and it
  // carries it in whole mebibytes, the same unit an install's runaway uses.
  it("says how much a runaway first boot wrote, against what it was allowed", () => {
    const runaway = REHEARSAL_ENDINGS[4];
    expect(rehearsalOutcomePhrase(runaway).params).toEqual({ seconds: 40, written: 162, ceiling: 64 });
    expect(rehearsalTone(runaway)).toBe("err");
    // Not the timeout's advice: a first boot that kept writing was not
    // waiting on anybody.
    expect(rehearsalNextStepPhrase(runaway).key).not.toBe(
      rehearsalNextStepPhrase(REHEARSAL_ENDINGS[2]).key
    );
  });

  it("colours a refusal as an error and an unanswered run as a warning", () => {
    expect(rehearsalTone(REHEARSAL_ENDINGS[0])).toBe("ok");
    expect(rehearsalTone(REHEARSAL_ENDINGS[1])).toBe("err");
    expect(rehearsalTone(REHEARSAL_ENDINGS[2])).toBe("warn");
    expect(rehearsalTone(REHEARSAL_ENDINGS[3])).toBe("warn");
  });
});

describe("the tick crosses the wire under one name, both ways", () => {
  beforeEach(() => invokeMock.mockReset().mockResolvedValue({}));

  it("firstbootPreview sends askPrefs", async () => {
    await firstbootPreview("E:\\dist", false);
    expect(invokeMock).toHaveBeenCalledWith("firstboot_preview", { tree: "E:\\dist", askPrefs: false });
  });

  it("firstbootWrite sends askPrefs", async () => {
    await firstbootWrite("E:\\dist", true);
    expect(invokeMock).toHaveBeenCalledWith("firstboot_write", { tree: "E:\\dist", askPrefs: true });
  });

  it("and the Rust commands take ask_prefs, which Tauri calls askPrefs", () => {
    expect(COMMAND).toMatch(/pub fn firstboot_preview\(\s*tree: String,\s*ask_prefs: bool,?\s*\)/);
    expect(COMMAND).toMatch(/pub fn firstboot_write\(\s*tree: String,\s*ask_prefs: bool,/);
  });

  it("the new enums are tagged the way this file reads them", () => {
    expect(PLAN).toMatch(/#\[serde\(tag = "kind", rename_all = "kebab-case"\)\]\s*\r?\n\s*pub enum RebootCommand/);
    expect(PLAN).toMatch(/#\[serde\(rename_all = "kebab-case"\)\]\s*\r?\n\s*pub enum WindowState/);
    expect(MOD).toMatch(/#\[serde\(rename_all = "kebab-case"\)\]\s*\r?\n\s*pub enum WizardWindow/);
    expect(MOD).toMatch(/#\[serde\(rename_all = "kebab-case"\)\]\s*\r?\n\s*pub enum ForegroundWindow/);
    expect(MOD).toContain(`pub const WIZARD_STEP: &str = "${WIZARD_STEP_NAME}";`);
  });
});

describe("the reboot and the wizard, as phrases", () => {
  it("says nothing about an available reboot and names the package otherwise", () => {
    expect(rebootCommandPhrase({ kind: "available" })).toBeNull();
    expect(rebootCommandPhrase({ kind: "unavailable", needs: "reboot" })).toEqual({
      key: "firstboot.reboot.unavailable",
      params: { needs: "reboot" },
    });
  });

  it("groups the rows by what the Amiga will do", () => {
    const wizard: WizardPlan = {
      rows: [
        { window: "locale", state: "ask" },
        { window: "input", state: "set-by-art" },
        { window: "screen-mode", state: "missing" },
      ],
    };
    expect(wizardLines(wizard)).toEqual({ ask: ["locale"], setByArt: ["input"], missing: ["screen-mode"] });
  });

  it("gives each window its own label and its own editor", () => {
    const windows = ["locale", "input", "screen-mode"] as const;
    expect(new Set(windows.map((w) => wizardWindowPhrase(w).key)).size).toBe(3);
    expect(windows.map(wizardEditorPath)).toEqual(["SYS:Prefs/Locale", "SYS:Prefs/Input", "SYS:Prefs/ScreenMode"]);
  });

  it("reads 90-prefs's three detail words and nothing else", () => {
    expect(wizardDetail("opened Locale")).toEqual({ phrase: { key: "firstboot.report.wizard.opened" }, window: "locale" });
    expect(wizardDetail("not-asked Input")).toEqual({ phrase: { key: "firstboot.report.wizard.notAsked" }, window: "input" });
    expect(wizardDetail("missing ScreenMode")).toEqual({ phrase: { key: "firstboot.report.wizard.missing" }, window: "screen-mode" });
    expect(wizardDetail("sd0-mounted RPi4")).toBeNull();
    expect(wizardDetail("opened constructor")).toBeNull();
    expect(wizardDetail("opened Locale twice")).toBeNull();
  });

  it("keeps four restart states apart: none, restarted, no command, not back yet", () => {
    const asked = { ...EMPTY_REPORT, rebootRequestedBy: "15-probe" };
    expect(rebootReportPhrase(EMPTY_REPORT)).toBeNull();
    expect(rebootReportPhrase({ ...asked, restartedAfterRequest: true })?.key).toBe("firstboot.report.reboot.restarted");
    expect(rebootReportPhrase({ ...asked, rebootUnavailable: true })?.key).toBe("firstboot.report.reboot.unavailable");
    expect(rebootReportPhrase(asked)).toEqual({ key: "firstboot.report.reboot.pending", params: { step: "15-probe" } });
  });

  it("names the window a timed-out rehearsal was waiting in, with its own next step", () => {
    const waited = { secs: 120, nanos: 0 };
    const plain: RehearsalOutcome = { kind: "timed-out", waited, report: EMPTY_REPORT };
    const locale: RehearsalOutcome = { kind: "timed-out", waited, report: { ...EMPTY_REPORT, waitingIn: "locale" } };
    const input: RehearsalOutcome = { kind: "timed-out", waited, report: { ...EMPTY_REPORT, waitingIn: "input" } };
    const said = [plain, locale, input].map((o) => rehearsalOutcomePhrase(o).key);
    const next = [plain, locale, input].map((o) => rehearsalNextStepPhrase(o).key);
    expect(new Set(said).size).toBe(3);
    expect(new Set(next).size).toBe(3);
    expect(rehearsalOutcomePhrase(locale)).toEqual({
      key: "firstboot.rehearsal.outcome.timedOutInLocale",
      params: { seconds: 120 },
    });
  });
});
