// Installing AmigaOS from the user's own media (SD-2 · G5).
// Mirrors src-tauri/src/commands/osinstall.rs and
// src-tauri/src/core/osinstall/{plan,apply,verify,scan}.rs.
//
// **`osinstallApply` takes the plan it is given and does not recompute it.**
// The same rule `layoutApply` follows, for the same reason (see
// src/lib/layout.ts's own note): the user's component choices *are* the
// plan, so a screen that previewed one install must not be able to build
// another. `osinstallPlan` and `osinstallScanMedia` are read-only previews —
// §92's PREVIEW — and neither writes anything.
//
// **A bad media folder is a value, not a thrown error.** Both
// `osinstallScanMedia` and `osinstallPlan` answer with an `outcome` tag
// rather than rejecting — `"folder-unreadable"` for the single most likely
// mistake after a bad ROM (a wrong path, or a folder ART cannot read), so
// the screen can translate it (ART-060) instead of showing a raw sentence.
//
// **`verified` is never just `failed === 0`.** A `VerifyReport` carries all
// three counts — `passed`, `failed`, `notChecked` — because "ART did not
// look" is not "ART found nothing wrong" (§89). Show all three.

import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

import type { Phrase } from "@/lib/phrase";

// ---------------------------------------------------------------------------
// Types, mirroring core::osinstall exactly
// ---------------------------------------------------------------------------

/** One install disk `osinstallScanMedia` opened successfully. */
export interface FoundMedia {
  path: string;
  /** Read from **inside** the image — never derived from `path`. */
  volumeName: string;
}

/** What scanning a media folder found, or why it could not be looked at. */
export type MediaScanResult =
  | { outcome: "found"; media: FoundMedia[] }
  | { outcome: "folder-unreadable"; folder: string };

export interface InstallRequest {
  mediaFolder: string;
  /** The paired Kickstart, if supplied. `null` refuses any component whose
   *  condition needs it decided. */
  rom: string | null;
  /** Component ids the user picked. `required` and condition-satisfied ones
   *  are added on top of this, not instead of it. */
  chosen: string[];
  /**
   * Condition-satisfied component ids the user has explicitly turned off
   * (spec requirement 2 — "turning Modules off is a confirmation, not a
   * refusal"). Subtracted **on the Rust side**, inside
   * `core::osinstall::plan::resolve_components_on`, before that
   * component's own media is ever resolved — never client-side.
   *
   * A fix-round correction, not the original shape: the first version of
   * this screen filtered a returned `InstallPlan` in the browser, which
   * left `mediaPaths` (and so `apply()`'s manifest `built_from`) stale, and
   * left an excluded component's own missing-media refusal standing —
   * exactly the "turning Modules off is still a refusal, just a politer
   * one" failure requirement 2 exists to prevent. Only the engine can
   * un-derive a refusal it itself raised; see `plan.rs`'s own doc comment
   * on `InstallRequest::excluded` for the Rust half of this.
   */
  excluded: string[];
  destination: string;
}

/** Whether a rule takes one file or a whole subtree. */
export type RuleKind = "file" | "subtree";

/** Why an install cannot proceed. A value, never a sentence (ART-060) — the
 *  screen translates it. */
export type RefusalReason =
  | { refusal: "media-missing"; component: string; volume_name: string }
  | { refusal: "media-path-missing"; component: string; media: string; path: string }
  | { refusal: "rom-unknown" }
  | { refusal: "destination-collision"; path: string; components: string[] }
  | {
      refusal: "media-ambiguous";
      component: string;
      volume_name: string;
      paths: string[];
    }
  | { refusal: "exclusive-group-conflict"; group: string; components: string[] }
  | {
      refusal: "rule-kind-mismatch";
      component: string;
      from: string;
      expected: RuleKind;
      found: RuleKind;
    };

/** One file or directory `osinstallApply` would place in the distribution
 *  tree. */
export interface PlanItem {
  component: string;
  /** The volume name the bytes came from — not the image's own filename. */
  media: string;
  from: string;
  to: string;
  isDir: boolean;
  bytes: number;
}

/** One switched-on component's own contribution to `S:User-Startup`. */
export interface UserStartupContribution {
  component: string;
  lines: string[];
}

/**
 * What planning an install produced: either a full description of what
 * would be written, or every reason it cannot proceed — never both. Any
 * refusal at all empties `items` and `mediaPaths`; check `refusals.length`
 * to tell the two cases apart.
 */
export interface InstallPlan {
  release: string;
  items: PlanItem[];
  refusals: RefusalReason[];
  totalBytes: number;
  /** Every component id switched on — required, chosen, or turned on by its
   *  own condition — regardless of whether its media could be found. */
  componentsOn: string[];
  /** Volume name -> the image it was found in. */
  mediaPaths: Record<string, string>;
  userStartup: UserStartupContribution[];
}

/** What planning found, or why the media folder itself could not be looked
 *  at. */
export type PlanResult =
  | { outcome: "planned"; plan: InstallPlan }
  | { outcome: "folder-unreadable"; folder: string };

/** What `osinstallApply` actually did. */
export interface ApplyOutcome {
  root: string;
  files: number;
  directories: number;
  bytes: number;
}

export const OSINSTALL_EVENT = "osinstall-result";

export interface OsInstallResult {
  job_id: number;
  destination: string;
  outcome: ApplyOutcome;
}

/** Whether one claim about a file was confirmed, contradicted, or never
 *  looked at. `not-checked` is not a soft pass — see the module note. */
export type CheckState = "pass" | "fail" | "not-checked";

export interface FileVerdict {
  path: string;
  state: CheckState;
  /** Why, whenever `state` is anything but a clean pass. Always present when
   *  `state` is `"not-checked"`. */
  detail: string | null;
}

export interface VerifyReport {
  files: FileVerdict[];
  passed: number;
  failed: number;
  notChecked: number;
}

// ---------------------------------------------------------------------------
// The commands
// ---------------------------------------------------------------------------

/**
 * Every install disk found directly inside `mediaFolder` — before any ROM or
 * component has been chosen, so the screen can show what it found the
 * moment a folder is picked. Writes nothing.
 */
export async function osinstallScanMedia(mediaFolder: string): Promise<MediaScanResult> {
  return invoke<MediaScanResult>("osinstall_scan_media", { folder: mediaFolder });
}

/** What installing the chosen components would do — or every reason it
 *  cannot. Writes nothing (§92's PREVIEW). */
export async function osinstallPlan(request: InstallRequest): Promise<PlanResult> {
  return invoke<PlanResult>("osinstall_plan", { request });
}

/**
 * Build the distribution tree. Returns a job id (§54).
 *
 * Takes the plan exactly as the screen was shown it — see the module note
 * above for why this does not recompute the way `preloadRun` does.
 */
export async function osinstallApply(plan: InstallPlan, destination: string): Promise<number> {
  return invoke<number>("osinstall_apply", { request: { plan, destination } });
}

/** Subscribe to finished installs. A cancelled or failed job never sends
 *  one — the job bar is where those are seen. */
export async function onOsInstallResult(
  handler: (result: OsInstallResult) => void
): Promise<UnlistenFn> {
  return listen<OsInstallResult>(OSINSTALL_EVENT, (event) => handler(event.payload));
}

/**
 * Read the volume back and check it against the manifest `osinstallApply`
 * wrote (§92's VERIFY step). `distRoot` is the distribution tree's own
 * root — where `distribution.json` was written — not the manifest file
 * itself.
 */
export async function osinstallVerify(
  image: string,
  slot: number | null,
  index: number,
  distRoot: string
): Promise<VerifyReport> {
  return invoke<VerifyReport>("osinstall_verify", {
    request: { image, slot, index, distRoot },
  });
}

// ---------------------------------------------------------------------------
// What the screen holds, and the rules over it
// ---------------------------------------------------------------------------

/**
 * Why the install cannot run yet, or null when it can.
 *
 * A reason rather than a boolean: a disabled button that does not say why is
 * the defect ART-100 was.
 */
export function osinstallBlocker(input: {
  mediaFolder: string | null;
  destination: string | null;
  plan: PlanResult | null;
}): Phrase | null {
  if (!input.mediaFolder?.trim()) return { key: "osinstall.blocked.noFolder" };
  if (!input.destination?.trim()) return { key: "osinstall.blocked.noDestination" };
  if (!input.plan) return { key: "osinstall.blocked.notPlanned" };
  if (input.plan.outcome === "folder-unreadable") {
    return { key: "osinstall.blocked.folderUnreadable" };
  }
  if (input.plan.plan.refusals.length > 0) return { key: "osinstall.blocked.refusals" };
  if (input.plan.plan.items.length === 0) return { key: "osinstall.blocked.nothingToInstall" };
  return null;
}

/** Whether this report can honestly be called verified — never `failed ===
 *  0` alone (§89): a file ART never looked at is not a file ART cleared. */
export function isVerified(report: VerifyReport): boolean {
  return report.failed === 0 && report.notChecked === 0;
}

/** The sentence for why one file's install cannot proceed, for the
 *  component to render. */
export function refusalPhrase(reason: RefusalReason): Phrase {
  switch (reason.refusal) {
    case "media-missing":
      return {
        key: "osinstall.refusal.mediaMissing",
        params: { component: reason.component, volume: reason.volume_name },
      };
    case "media-path-missing":
      return {
        key: "osinstall.refusal.mediaPathMissing",
        params: { component: reason.component, media: reason.media, path: reason.path },
      };
    case "rom-unknown":
      return { key: "osinstall.refusal.romUnknown" };
    case "destination-collision":
      return {
        key: "osinstall.refusal.destinationCollision",
        params: { path: reason.path, components: reason.components.join(", ") },
      };
    case "media-ambiguous":
      return {
        key: "osinstall.refusal.mediaAmbiguous",
        params: {
          component: reason.component,
          volume: reason.volume_name,
          paths: reason.paths.join(", "),
        },
      };
    case "exclusive-group-conflict":
      return {
        key: "osinstall.refusal.exclusiveGroupConflict",
        params: { group: reason.group, components: reason.components.join(", ") },
      };
    case "rule-kind-mismatch":
      return {
        key: "osinstall.refusal.ruleKindMismatch",
        params: {
          component: reason.component,
          from: reason.from,
          expected: reason.expected,
          found: reason.found,
        },
      };
  }
}

/** Whether a plan's own refusals include the one that means "the paired
 *  Kickstart could not be identified" — used both for the run blocker (via
 *  `refusalPhrase`) and by the component list's own reasoning below. */
export function hasRomUnknownRefusal(plan: InstallPlan): boolean {
  return plan.refusals.some((r) => r.refusal === "rom-unknown");
}

// ---------------------------------------------------------------------------
// The AmigaOS 3.2 component catalogue
//
// Mirrors `src-tauri/src/core/osinstall/recipes/amigaos-3.2.json`, component
// by component — id, `required`, `condition` and `exclusive_group` all have
// to agree with the shipped recipe, because this list is what the OS
// Install screen turns into checkboxes. `osinstall.test.ts` parses that
// JSON file directly and asserts every field here against it, so a hand
// edit to either side that drifts from the other fails the build in the
// same commit, rather than silently — the fix-round answer to a review
// finding this needed a fifth Tauri command exposing the recipe: a passing
// parity test removes the urgency without adding one.
// ---------------------------------------------------------------------------

export interface ComponentDef {
  id: string;
  /** The volume name inside the image — shown as the row's own label,
   *  unlocalized, the same way the preload screen prints a partition's
   *  `drive_name` untranslated: this is what the Amiga side calls it, not a
   *  sentence ART wrote. */
  media: string;
  required: boolean;
  available: boolean;
  /** `Condition::RomOlderThan { major }`, mirrored — the only condition
   *  shape the recipe carries today. `null` for an unconditional component. */
  conditionMajor: number | null;
  exclusiveGroup: string | null;
}

export const AMIGAOS_32_COMPONENTS: ComponentDef[] = [
  {
    id: "workbench-base",
    media: "Workbench3.2",
    required: true,
    available: true,
    conditionMajor: null,
    exclusiveGroup: null,
  },
  {
    id: "install-libs",
    media: "Install3.2",
    required: true,
    available: true,
    conditionMajor: null,
    exclusiveGroup: null,
  },
  { id: "extras", media: "Extras3.2", required: false, available: true, conditionMajor: null, exclusiveGroup: null },
  { id: "locale-base", media: "Locale", required: false, available: true, conditionMajor: null, exclusiveGroup: null },
  { id: "locale-de", media: "Locale-DE", required: false, available: true, conditionMajor: null, exclusiveGroup: null },
  { id: "locale-dk", media: "Locale-DK", required: false, available: true, conditionMajor: null, exclusiveGroup: null },
  { id: "locale-en", media: "Locale-EN", required: false, available: true, conditionMajor: null, exclusiveGroup: null },
  { id: "locale-es", media: "Locale-ES", required: false, available: true, conditionMajor: null, exclusiveGroup: null },
  { id: "locale-fr", media: "Locale-FR", required: false, available: true, conditionMajor: null, exclusiveGroup: null },
  { id: "locale-gr", media: "Locale-GR", required: false, available: true, conditionMajor: null, exclusiveGroup: null },
  { id: "locale-it", media: "Locale-IT", required: false, available: true, conditionMajor: null, exclusiveGroup: null },
  { id: "locale-nl", media: "Locale-NL", required: false, available: true, conditionMajor: null, exclusiveGroup: null },
  { id: "locale-no", media: "Locale-NO", required: false, available: true, conditionMajor: null, exclusiveGroup: null },
  { id: "locale-pl", media: "Locale-PL", required: false, available: true, conditionMajor: null, exclusiveGroup: null },
  { id: "locale-pt", media: "Locale-PT", required: false, available: true, conditionMajor: null, exclusiveGroup: null },
  { id: "locale-ru", media: "Locale-RU", required: false, available: true, conditionMajor: null, exclusiveGroup: null },
  { id: "locale-se", media: "Locale-SE", required: false, available: true, conditionMajor: null, exclusiveGroup: null },
  { id: "locale-tr", media: "Locale-TR", required: false, available: true, conditionMajor: null, exclusiveGroup: null },
  { id: "locale-uk", media: "Locale-UK", required: false, available: true, conditionMajor: null, exclusiveGroup: null },
  {
    id: "modules-a1200",
    media: "ModulesA1200_3.2",
    required: false,
    available: true,
    conditionMajor: 47,
    exclusiveGroup: "modules",
  },
  {
    id: "update-3.2.1",
    media: "Update3.2.1",
    required: false,
    available: false,
    conditionMajor: null,
    exclusiveGroup: null,
  },
  { id: "fonts", media: "Fonts", required: false, available: true, conditionMajor: null, exclusiveGroup: null },
  { id: "classes", media: "Classes3.2", required: false, available: true, conditionMajor: null, exclusiveGroup: null },
  {
    id: "glowicons",
    media: "GlowIcons3.2",
    required: false,
    available: true,
    conditionMajor: null,
    exclusiveGroup: null,
  },
  {
    id: "backdrops",
    media: "Backdrops3.2",
    required: false,
    available: true,
    conditionMajor: null,
    exclusiveGroup: null,
  },
  { id: "diskdoctor", media: "DiskDoctor", required: false, available: true, conditionMajor: null, exclusiveGroup: null },
  { id: "mmulibs", media: "MMULibs", required: false, available: true, conditionMajor: null, exclusiveGroup: null },
  { id: "hdtools", media: "HDSetup3.2", required: false, available: true, conditionMajor: null, exclusiveGroup: null },
  { id: "storage", media: "Storage3.2", required: false, available: true, conditionMajor: null, exclusiveGroup: null },
];

export function componentDef(id: string): ComponentDef | undefined {
  return AMIGAOS_32_COMPONENTS.find((c) => c.id === id);
}

export function componentLabel(id: string): string {
  return componentDef(id)?.media ?? id;
}

/** `chosen`, with anything that is not a real, available component id
 *  dropped — a stale remembered id (an old ART's component that was
 *  renamed or removed) or an unavailable one (Coming Later) can never
 *  reach `InstallRequest.chosen`, and calling this on every plan lets a
 *  stale entry actually clear from what gets remembered, not just be
 *  filtered again on the next read. */
export function sanitizeChosen(chosen: string[]): string[] {
  return chosen.filter((id) => componentDef(id)?.available === true);
}

// ---------------------------------------------------------------------------
// Conditional components — reasoning and the toggle state machine
//
// Pulled out of the screen component so it can be unit-tested directly —
// the fix-round answer to a review finding that a browser pass was the only
// way to exercise `withExclusions`/`isForcedOnByCondition`/the toggle
// handlers, and that a Critical defect (a stale `mediaPaths`) had shipped
// inside exactly that untested logic. `withExclusions` itself is gone: the
// engine now performs the exclusion (`InstallRequest.excluded`), so there is
// nothing left to filter on this side — only *reasoning about* the plan the
// engine already excluded correctly.
// ---------------------------------------------------------------------------

/**
 * Whether `id` is switched on **only** because its own `Condition` is
 * satisfied — never because it is `required` or because the caller put it
 * in `chosen`. Rust's `resolve_components_on` computes `is_on` as
 * `required || chosen.contains(id)`, then ORs in the condition — it can
 * only ever add `true`, never remove one — so if `plan.componentsOn`
 * carries `id` and neither of the first two is true, the condition is the
 * only thing left that could have done it.
 *
 * Always call this with a plan requested with an **empty** `excluded` list
 * (a "base" plan) — a plan requested with `id` itself excluded will never
 * carry `id` in `componentsOn` at all (the engine skips it entirely), which
 * would make this function report `false` for a component that is, in
 * truth, still condition-satisfied and merely turned off. The screen keeps
 * two plans for exactly this reason: one reflecting the real exclusions,
 * for the file list and for `osinstallApply`; one with none, purely to
 * reason about what *would* be on.
 */
export function isForcedOnByCondition(basePlan: InstallPlan | null, chosen: string[], id: string): boolean {
  const def = componentDef(id);
  if (!basePlan || !def || def.required) return false;
  return basePlan.componentsOn.includes(id) && !chosen.includes(id);
}

/**
 * Every excluded id whose component is still, in truth, condition-satisfied
 * — the ones worth keeping. An excluded id whose condition no longer holds
 * (the paired ROM changed) is pruned: leaving it in the remembered set would
 * silently reapply the override the moment a pre-V47 ROM came back, without
 * the user ever confirming *that* pairing, which is "nothing changes unless
 * the user changes it" read backwards.
 */
export function pruneStaleExclusions(
  basePlan: InstallPlan,
  chosen: string[],
  excluded: string[]
): string[] {
  return excluded.filter((id) => isForcedOnByCondition(basePlan, chosen, id));
}

/** Why a conditional component's row is ticked or not — always exactly one
 *  of these four, never none: a branch chain that can fall through to
 *  nothing is the defect a review found here once already (a tick with no
 *  explanation, when the frontend's own ROM read and the plan's own ROM
 *  read disagreed). `rom-needed` is the catch-all: whenever the plan itself
 *  could not decide the condition (no ROM, or one it could not identify),
 *  every other branch is unreachable by construction below. */
export type ConditionalReason =
  | { kind: "rom-needed" }
  | { kind: "condition-on"; rom: string; major: number }
  | { kind: "condition-off"; rom: string; major: number }
  | { kind: "condition-overridden"; major: number };

/**
 * Decide a conditional component's reason, from primitives rather than from
 * `InstallPlan`/`RomInfo` directly — easier to test exhaustively (every
 * combination of three booleans and an optional string, sixteen cases) and
 * it cannot itself misread a plan that was requested with the wrong
 * `excluded` list, because it never sees one.
 *
 * `romUnknown` should come from the **base** plan's own refusals
 * (`hasRomUnknownRefusal`), not the effective one — the effective plan can
 * legitimately have no `RomUnknown` refusal at all once the one conditional
 * component that needed the ROM decided has been excluded (that is the
 * point of requirement 2: no ROM, no Modules, nothing left to decide), and
 * that must not be read as "the ROM is fine".
 */
export function conditionalReason(
  major: number,
  forcedOn: boolean,
  excluded: boolean,
  romUnknown: boolean,
  rom: string | null
): ConditionalReason {
  if (romUnknown || rom === null) return { kind: "rom-needed" };
  if (excluded && forcedOn) return { kind: "condition-overridden", major };
  if (forcedOn) return { kind: "condition-on", rom, major };
  return { kind: "condition-off", rom, major };
}

/** What checking or unchecking a conditional component's box should do,
 *  decided from state alone so the screen's `onChange` handler is a plain
 *  dispatch over the result. */
export type ConditionalToggleAction = "undo-exclusion" | "confirm-off" | "toggle-chosen";

export function conditionalToggleAction(excluded: boolean, forcedOn: boolean): ConditionalToggleAction {
  if (excluded) return "undo-exclusion";
  if (forcedOn) return "confirm-off";
  return "toggle-chosen";
}

/** Ordinary opt-in/opt-out through `chosen` for a non-conditional
 *  component (and the opt-in half of a conditional one's own toggle, when
 *  its condition does not currently hold) — clears any other member of the
 *  same `exclusiveGroup` on the way in. */
export function toggleChosen(chosen: string[], id: string): string[] {
  if (chosen.includes(id)) return chosen.filter((c) => c !== id);
  const group = componentDef(id)?.exclusiveGroup ?? null;
  const withoutGroup = group ? chosen.filter((c) => componentDef(c)?.exclusiveGroup !== group) : chosen;
  return [...withoutGroup, id];
}

/** Confirming "turn this off anyway": excluded and chosen are kept mutually
 *  exclusive by construction, so confirming an exclusion also drops any
 *  stray `chosen` entry for the same id. */
export function confirmComponentOff(
  chosen: string[],
  excluded: string[],
  id: string
): { chosen: string[]; excluded: string[] } {
  return {
    chosen: chosen.filter((x) => x !== id),
    excluded: excluded.includes(id) ? excluded : [...excluded, id],
  };
}

export function withoutExcluded(excluded: string[], id: string): string[] {
  return excluded.filter((x) => x !== id);
}

// ---------------------------------------------------------------------------
// The Verify section's own small parsers — pulled out for the same reason:
// Minor findings both traced back to ad hoc `Number.parseInt` at the two
// call sites disagreeing with each other instead of sharing one answer.
// ---------------------------------------------------------------------------

/** A PiStorm area number, or `null` for "a plain HDF" — `""` is legal and
 *  means the latter; anything else has to be a non-negative integer, never
 *  `NaN` silently reaching the wire as `null` (`JSON.stringify(NaN)` is
 *  `"null"`, which is indistinguishable from a deliberate "no slot"). */
export function parseOptionalSlot(text: string): { ok: true; value: number | null } | { ok: false } {
  const trimmed = text.trim();
  if (trimmed === "") return { ok: true, value: null };
  if (!/^\d+$/.test(trimmed)) return { ok: false };
  return { ok: true, value: Number.parseInt(trimmed, 10) };
}

/** A partition index, 1-based — `null` for anything that is not a whole
 *  number `>= 1`. The one function both the Verify button's `disabled` and
 *  `runVerify`'s own guard call, so the two cannot silently disagree about
 *  what counts as ready the way an enabled-but-inert button once did. */
export function parsePartitionIndex(text: string): number | null {
  const trimmed = text.trim();
  if (!/^\d+$/.test(trimmed)) return null;
  const n = Number.parseInt(trimmed, 10);
  return n >= 1 ? n : null;
}
