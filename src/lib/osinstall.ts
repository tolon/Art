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
