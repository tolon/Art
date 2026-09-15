// First boot — the Amiga finishes what the host cannot start.
// Mirrors src-tauri/src/commands/firstboot.rs and src-tauri/src/core/firstboot/.
//
// `firstbootPreview` writes nothing (§92 PREVIEW). `firstbootWrite` puts the
// files into the tree and merges one block into S:User-Startup.
//
// Endings stay distinct (spec §4.3): a card with no report has *not booted*,
// never "failed".

import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

import { mebibytes, waitedSeconds, type WireDuration } from "@/lib/amigainstall";
import type { Phrase } from "@/lib/phrase";

export type FatMount = { kind: "available" } | { kind: "unavailable"; needs: string };

/** Whether the step wrapper can carry out a reboot request (phase 3 design §2
 *  decision 4). Not a refusal — the `FatMount` shape. */
export type RebootCommand = { kind: "available" } | { kind: "unavailable"; needs: string };

/** A Preferences window the wizard may open, in `90-prefs`'s order. */
export type WizardWindow = "locale" | "input" | "screen-mode";
export type WindowState = "ask" | "set-by-art" | "missing";

export interface WizardRow {
  window: WizardWindow;
  state: WindowState;
}

export interface WizardPlan {
  rows: WizardRow[];
}

/** A window the boot waits in. ScreenMode runs detached and never is. */
export type ForegroundWindow = "locale" | "input";

/** `core::firstboot::WIZARD_STEP` — `firstboot.test.ts` holds it to the Rust. */
export const WIZARD_STEP_NAME = "90-prefs";

export interface PlannedStep {
  name: string;
  treePath: string;
  fixed: boolean;
}

export interface FirstBootPlan {
  tree: string;
  steps: PlannedStep[];
  fatMount: FatMount;
  userStartupExists: boolean;
  alreadyWritten: boolean;
  bytesAdded: number;
  reboot: RebootCommand;
  wizard: WizardPlan | null;
  inputSetByArt: boolean;
}

export interface FirstBootWritten {
  files: string[];
  userStartupBackup: string | null;
  userStartupCreated: boolean;
  removed: string[];
}

export type StepOutcome =
  | { kind: "ok" }
  | { kind: "skipped"; reason: string }
  | { kind: "refused"; rc: number }
  | { kind: "unfinished" };

export type Ending = "not-booted" | "unfinished" | "done-all" | "done-partial";

export interface Detected {
  system: string;
  rpi: string;
  kick: string;
}

export interface StepReport {
  name: string;
  outcome: StepOutcome;
  details: string[];
}

export interface FirstBootReport {
  version: number | null;
  system: Detected | null;
  steps: StepReport[];
  ending: Ending;
  fatCopyFailed: boolean;
  rebootRequestedBy: string | null;
  rebootUnavailable: boolean;
  restartedAfterRequest: boolean;
  waitingIn: ForegroundWindow | null;
  unknown: string[];
}

export async function firstbootPreview(tree: string, askPrefs: boolean): Promise<FirstBootPlan> {
  return invoke<FirstBootPlan>("firstboot_preview", { tree, askPrefs });
}

export async function firstbootWrite(tree: string, askPrefs: boolean): Promise<FirstBootWritten> {
  return invoke<FirstBootWritten>("firstboot_write", { tree, askPrefs });
}

// ---------------------------------------------------------------------------
// Reading a card's first-boot report back (§9)
// ---------------------------------------------------------------------------
//
// Where the report actually came from. `"amiga-volume"` is the copy spec §9
// says wins where both exist; `"fat"` is the fallback for a Windows user who
// cannot otherwise read PFS3 or FFS; `"none"` means the card has not booted
// the first-boot block yet — not an error.
export type ReportSource = "fat" | "amiga-volume" | "none";

export interface CardFirstBootReport {
  source: ReportSource;
  report: FirstBootReport;
}

/** Read a card's first-boot report — no card operation, no boot, just files
 *  already on the image. Never opens the card for writing. */
export async function cardFirstBootReport(path: string): Promise<CardFirstBootReport> {
  return invoke<CardFirstBootReport>("card_firstboot_report", { path });
}

/** `beginner: true` drops the AmigaDOS-facing detail spec §9 says a beginner
 *  should not see — here, the `rc=` return code inside a refusal's own
 *  sentence (I4, final review: `FirstBootReportPanel` already hides `details`
 *  and `unknown` in beginner mode; this closes the third leak). */
export function stepOutcomePhrase(outcome: StepOutcome, opts?: { beginner?: boolean }): Phrase {
  switch (outcome.kind) {
    case "ok":
      return { key: "firstboot.step.ok" };
    case "skipped":
      return { key: "firstboot.step.skipped", params: { reason: outcome.reason } };
    case "refused":
      return opts?.beginner
        ? { key: "firstboot.step.refusedPlain" }
        : { key: "firstboot.step.refused", params: { rc: outcome.rc } };
    case "unfinished":
      return { key: "firstboot.step.unfinished" };
  }
}

export function endingPhrase(ending: Ending): Phrase {
  switch (ending) {
    case "not-booted":
      return { key: "firstboot.ending.notBooted" };
    case "unfinished":
      return { key: "firstboot.ending.unfinished" };
    case "done-all":
      return { key: "firstboot.ending.doneAll" };
    case "done-partial":
      return { key: "firstboot.ending.donePartial" };
  }
}

export function fatMountPhrase(fat: FatMount): Phrase {
  return fat.kind === "available"
    ? { key: "firstboot.fat.available" }
    : { key: "firstboot.fat.unavailable", params: { needs: fat.needs } };
}

/** The line beside the tick when the tree has no reboot command; an
 *  available command has nothing to say. */
export function rebootCommandPhrase(reboot: RebootCommand): Phrase | null {
  return reboot.kind === "available"
    ? null
    : { key: "firstboot.reboot.unavailable", params: { needs: reboot.needs } };
}

/** A window's label — what it asks, never its file name (Beginner mode). */
export function wizardWindowPhrase(window: WizardWindow): Phrase {
  switch (window) {
    case "locale":
      return { key: "firstboot.wizard.window.locale" };
    case "input":
      return { key: "firstboot.wizard.window.input" };
    case "screen-mode":
      return { key: "firstboot.wizard.window.screenMode" };
  }
}

/** The editor `90-prefs` runs — for Power mode only. An AmigaDOS path, never translated. */
export function wizardEditorPath(window: WizardWindow): string {
  switch (window) {
    case "locale":
      return "SYS:Prefs/Locale";
    case "input":
      return "SYS:Prefs/Input";
    case "screen-mode":
      return "SYS:Prefs/ScreenMode";
  }
}

export interface WizardLines {
  ask: WizardWindow[];
  setByArt: WizardWindow[];
  missing: WizardWindow[];
}

/** The rows, grouped by what the Amiga will do with each. */
export function wizardLines(wizard: WizardPlan): WizardLines {
  const pick = (state: WindowState) =>
    wizard.rows.filter((row) => row.state === state).map((row) => row.window);
  return { ask: pick("ask"), setByArt: pick("set-by-art"), missing: pick("missing") };
}

export interface WizardDetail {
  phrase: Phrase;
  window: WizardWindow;
}

function windowNamed(name: string | undefined): WizardWindow | null {
  switch (name) {
    case "Locale":
      return "locale";
    case "Input":
      return "input";
    case "ScreenMode":
      return "screen-mode";
    default:
      return null;
  }
}

/** One of `90-prefs`'s own detail lines, read as a sentence: `opened Locale`,
 *  `not-asked Input`, `missing ScreenMode`. `null` for any other detail, which
 *  stays a raw line only Power mode shows. The component fills `window` with
 *  the translated {@link wizardWindowPhrase}. */
export function wizardDetail(detail: string): WizardDetail | null {
  const words = detail.split(" ");
  const window = windowNamed(words[1]);
  if (words.length !== 2 || window === null) return null;
  switch (words[0]) {
    case "opened":
      return { phrase: { key: "firstboot.report.wizard.opened" }, window };
    case "not-asked":
      return { phrase: { key: "firstboot.report.wizard.notAsked" }, window };
    case "missing":
      return { phrase: { key: "firstboot.report.wizard.missing" }, window };
    default:
      return null;
  }
}

/** What the report says about a restart. Four states, three sentences and
 *  silence: a request is not a restart until a later boot shows one. */
export function rebootReportPhrase(report: FirstBootReport): Phrase | null {
  const step = report.rebootRequestedBy;
  if (step === null) return null;
  if (report.rebootUnavailable) return { key: "firstboot.report.reboot.unavailable", params: { step } };
  if (report.restartedAfterRequest) return { key: "firstboot.report.reboot.restarted", params: { step } };
  return { key: "firstboot.report.reboot.pending", params: { step } };
}

/** Which copy of the report was actually read (spec §9: the Amiga volume's
 *  own `S/FirstBoot.log` wins over the FAT partition's `art-firstboot.log`
 *  where both exist). `null` for `"none"` — a card with neither copy has not
 *  booted, and the not-booted sentence already says so; there is nothing
 *  about a source to add to it. */
export function reportSourcePhrase(source: ReportSource): Phrase | null {
  switch (source) {
    case "fat":
      return { key: "firstboot.report.source.fat" };
    case "amiga-volume":
      return { key: "firstboot.report.source.amigaVolume" };
    case "none":
      return null;
  }
}

/** The tone a row takes — colour is never the only signal, the key wording is. */
export function stepOutcomeTone(outcome: StepOutcome): "ok" | "muted" | "warn" | "err" {
  switch (outcome.kind) {
    case "ok":
      return "ok";
    case "skipped":
      return "muted";
    case "refused":
      return "warn";
    case "unfinished":
      return "err";
  }
}

// ---------------------------------------------------------------------------
// Rehearsing the first boot under WinUAE
// ---------------------------------------------------------------------------
//
// A rehearsal boots a **copy** of the tree, never the tree. It proves the
// mechanism — the dispatcher runs, each step reports, the block removes
// itself — and it can never prove the Pi3/Pi4 hardware branch, because under
// an emulator `10-hardware` writes `skipped uae` and copies nothing. That
// half closes on a real card only.
//
// Five endings, five sentences, exactly like `amigainstall`'s: "the window
// was closed" and "nobody answered in time" call for opposite next steps, so
// collapsing them into "it did not work" is the defect, not a shortcut.

/** How a rehearsal ended. Mirrors `core::amigainstall::rehearse::RehearsalOutcome`,
 *  whose `kind` tags are kebab-case. Every variant carries the report,
 *  including the two that did not finish: a run that got partway still told
 *  the host something. */
export type RehearsalOutcome =
  | { kind: "finished"; report: FirstBootReport }
  | { kind: "step-refused"; report: FirstBootReport }
  | { kind: "timed-out"; waited: WireDuration; report: FirstBootReport }
  | { kind: "emulator-closed"; waited: WireDuration; report: FirstBootReport }
  /** ART-294: the copy grew by `written` bytes, past the `ceiling` bytes a
   *  run may add to it, while the first boot was still writing. */
  | {
      kind: "wrote-without-stopping";
      waited: WireDuration;
      written: number;
      ceiling: number;
      report: FirstBootReport;
    };

/** A finished rehearsal's own answer. `job_id` is snake_case to match every
 *  other job result in ART. */
export interface RehearsalResult {
  job_id: number;
  outcome: RehearsalOutcome;
  /** The copy that booted: where it still is when `discarded` is false. */
  copy: string;
  /** True only when the copy really was removed. A failed removal reports
   *  false and keeps the path, because no screen may claim a discard that did
   *  not happen. */
  discarded: boolean;
}

export interface RehearseRequest {
  tree: string;
  kickstart: string;
  profile?: string | null;
}

/** The event a finished rehearsal's own answer arrives on. */
export const FIRSTBOOT_REHEARSAL_EVENT = "firstboot-rehearsal-result";

/**
 * Boot a copy of the tree under WinUAE and read the Amiga's own report.
 * Returns a job id (§54) — progress on the ordinary `job-progress` event, the
 * answer on [`FIRSTBOOT_REHEARSAL_EVENT`].
 *
 * A tree with no first boot written into it, an unknown machine id and a
 * missing emulator all reject synchronously, before any job starts.
 */
export async function firstbootRehearse(
  request: RehearseRequest,
  winuaePath?: string | null
): Promise<number> {
  return invoke<number>("firstboot_rehearse", {
    request,
    winuaePath: winuaePath ?? null,
  });
}

/** Subscribe to finished rehearsals. A cancelled or failed job never sends
 *  one — the job bar is where those are seen. */
export async function onFirstBootRehearsalResult(
  handler: (result: RehearsalResult) => void
): Promise<UnlistenFn> {
  return listen<RehearsalResult>(FIRSTBOOT_REHEARSAL_EVENT, (event) => handler(event.payload));
}

/** What happened, in one sentence — a different key for every ending. */
export function rehearsalOutcomePhrase(outcome: RehearsalOutcome): Phrase {
  switch (outcome.kind) {
    case "finished":
      return { key: "firstboot.rehearsal.outcome.finished" };
    case "step-refused":
      return { key: "firstboot.rehearsal.outcome.stepRefused" };
    case "timed-out": {
      const params = { seconds: waitedSeconds(outcome.waited) };
      switch (outcome.report.waitingIn) {
        case "locale":
          return { key: "firstboot.rehearsal.outcome.timedOutInLocale", params };
        case "input":
          return { key: "firstboot.rehearsal.outcome.timedOutInInput", params };
        case null:
          return { key: "firstboot.rehearsal.outcome.timedOut", params };
      }
    }
    case "emulator-closed":
      return {
        key: "firstboot.rehearsal.outcome.emulatorClosed",
        params: { seconds: waitedSeconds(outcome.waited) },
      };
    case "wrote-without-stopping":
      return {
        key: "firstboot.rehearsal.outcome.wroteWithoutStopping",
        params: {
          seconds: waitedSeconds(outcome.waited),
          written: mebibytes(outcome.written),
          ceiling: mebibytes(outcome.ceiling),
        },
      };
  }
}

/** What to do about it — again one per ending, because the next step is what
 *  actually differs between them. */
export function rehearsalNextStepPhrase(outcome: RehearsalOutcome): Phrase {
  switch (outcome.kind) {
    case "finished":
      return { key: "firstboot.rehearsal.next.finished" };
    case "step-refused":
      return { key: "firstboot.rehearsal.next.stepRefused" };
    case "timed-out":
      switch (outcome.report.waitingIn) {
        case "locale":
          return { key: "firstboot.rehearsal.next.timedOutInLocale" };
        case "input":
          return { key: "firstboot.rehearsal.next.timedOutInInput" };
        case null:
          return { key: "firstboot.rehearsal.next.timedOut" };
      }
    case "emulator-closed":
      return { key: "firstboot.rehearsal.next.emulatorClosed" };
    case "wrote-without-stopping":
      return { key: "firstboot.rehearsal.next.wroteWithoutStopping" };
  }
}

/** How the rehearsal's report is coloured. Never the only signal — each
 *  ending already says which it is in words. */
export function rehearsalTone(outcome: RehearsalOutcome): "ok" | "warn" | "err" {
  switch (outcome.kind) {
    case "finished":
      return "ok";
    case "step-refused":
      return "err";
    case "timed-out":
    case "emulator-closed":
      return "warn";
    // A refusal's colour, as an install's runaway: something kept writing.
    case "wrote-without-stopping":
      return "err";
  }
}
