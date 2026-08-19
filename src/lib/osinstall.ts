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

import { awaitJobResult } from "@/lib/jobs";
import { formatBytes } from "@/lib/panel";
import type { Phrase } from "@/lib/phrase";

// ---------------------------------------------------------------------------
// Types, mirroring core::osinstall exactly
// ---------------------------------------------------------------------------

/** Which reader opened this medium. Mirrors `core::osinstall::scan::MediaKind`. */
export type MediaKind = "floppy" | "disc";

/** One install disk or disc `osinstallScanMedia` opened successfully. */
export interface FoundMedia {
  path: string;
  /** Read from **inside** the image — never derived from `path`. */
  volumeName: string;
  kind: MediaKind;
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
  /** Which shipped recipe to plan from — a `Recipe.release` string. */
  release: string;
}

/**
 * The releases ART ships a recipe for, in the order the picker lists them.
 *
 * Hand-maintained against `core::osinstall::recipe::releases()`. The Rust
 * side refuses a name it does not know rather than defaulting, so a stale
 * entry here surfaces as a refusal on screen and never as the wrong
 * operating system being written.
 */
export const INSTALL_RELEASES = ["AmigaOS 3.2", "AmigaOS 3.9"] as const;

export type InstallRelease = (typeof INSTALL_RELEASES)[number];

/**
 * Whether a persisted value is still one of the releases ART ships.
 *
 * Takes `unknown`, not `string` — this is what `useRemembered` needs to call
 * it as a `Guard<InstallRelease>` against a raw settings-file value, which
 * may not even be a string (a hand-edited or stale settings file).
 */
export function isInstallRelease(value: unknown): value is InstallRelease {
  return typeof value === "string" && (INSTALL_RELEASES as readonly string[]).includes(value);
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
    }
  // ---- Packages (Task 7) — the six variants `plan()` could already raise
  // but nothing on screen could reach until packages became selectable.
  // `rename_all = "kebab-case"` on `RefusalReason` renames the `refusal` tag
  // only, never a struct variant's own field names (see
  // `commands/osinstall.rs`'s `refusal_reason_tag_and_field_spellings_for_every_variant`,
  // which pins exactly this) — so every field below stays spelled as Rust
  // wrote it, same as the seven above.
  | { refusal: "package-unknown"; package: string }
  | { refusal: "package-folder-missing"; packages: string[] }
  | { refusal: "package-requirement-missing"; package: string; requires: string }
  | { refusal: "package-component-missing"; package: string; component: string }
  | { refusal: "package-archive-missing"; package: string; media: string }
  | {
      refusal: "package-archive-ambiguous";
      package: string;
      media: string;
      paths: string[];
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
  /** The chosen package ids, in the order the engine applies them — which
   *  is dependency order, not the order the boxes were ticked in.
   *
   *  Carried even when the plan refuses, the same way `componentsOn` is, so
   *  a screen can tell "no packages asked for" from "these were asked for
   *  and refused". When a refusal stopped the plan before the order could be
   *  worked out, this is the request's own list as given — the Rust side
   *  states no order for a run that will not happen. */
  packages: string[];
  /** Package media name -> the archive it was found in, and the member
   *  inside it that holds the payload. The package half of `mediaPaths`;
   *  kept apart because the two are found by different scans of different
   *  folders. */
  packageMedia: Record<string, { path: string; member: string | null }>;
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
// Update packages — landing an official (or unofficial) package on a
// distribution tree that already exists (Task 7). `Collision`/`CollisionReport`
// mirror `core::osinstall::collide` exactly, including the `kind` tag —
// `Collision::SameVersion`/`::Unversioned` carry an explicit
// `#[serde(rename)]` on their byte counts (`collide.rs`'s own doc comment
// explains why `rename_all = "kebab-case"` alone would not have camelCased
// them), so `fromBytes`/`toBytes` here really is what crosses the wire.
//
// ## Fix round 1 (Task 7's own review)
//
// **F2 — refusals are data, not debug text.** `osinstallAddPackage` used to
// return a bare job id; a refused selection reached the screen as Rust's
// own `{:?}` output. It now returns [`AddPackageResult`], and a `Refused`
// answer carries the same six-variant `RefusalReason` the install screen's
// own refusals already render through [`refusalPhrase`].
//
// **F4 — the preview runs off the command thread.** `osinstall_collisions`
// now starts a job and answers with its id; [`osinstallCollisions`] hides
// that behind its own unchanged `Promise<CollisionReport[]>` shape by
// awaiting [`OSINSTALL_COLLISIONS_EVENT`] through [`awaitJobResult`] — no
// caller of this function had to change.
// ---------------------------------------------------------------------------

/** What landing one incoming file over an existing one would actually do —
 *  mirrors `core::osinstall::collide::Collision`. `Identical` never reaches
 *  this type: the core excludes it before a `CollisionReport` is ever built
 *  (see `preview`'s own doc comment), so there is nothing to render it as. */
export type Collision =
  | { kind: "upgrade"; from: string; to: string }
  | { kind: "downgrade"; from: string; to: string }
  | { kind: "same-version"; version: string; fromBytes: number; toBytes: number }
  | { kind: "unversioned"; fromBytes: number; toBytes: number };

/** One planned item that would land on something already in the tree —
 *  mirrors `core::osinstall::collide::CollisionReport`. */
export interface CollisionReport {
  /** Where in the tree, `/`-separated. */
  path: string;
  collision: Collision;
  /** Whether the package's own recipe declared it may write over what is
   *  there (its `overrides`). An `undeclared` row is not necessarily wrong —
   *  it just was not promised, and the panel says so beside the row rather
   *  than silently treating every collision alike. */
  declared: boolean;
}

/** One package ART ships a recipe for, in the shape the checklist needs.
 *  Mirrors `commands::osinstall::PackageSummary`. */
export interface PackageSummary {
  id: string;
  /** Shown on screen, unlocalized — a package's own name (ART-060). */
  name: string;
  /** Other **packages** this one needs applied first (`BoingBag 3.9-2`
   *  needs `BoingBag 3.9-1`) — dependency order, not the order ticked. */
  requires: string[];
  /** Recipe **components** this package needs switched on on the tree it
   *  lands on (`locale-turkish` needs `locale-base`, ART-162) — a different
   *  relationship from `requires`: this names a component, never a package. */
  requiresComponents: string[];
  /** Whether an archive carrying this package's own top-level directory
   *  name was actually found in the package folder. `false` is not "ART
   *  cannot install this" — every shipped package always can — it is "the
   *  file is not here yet", and a checkbox for a package whose file is
   *  absent is a promise ART cannot keep. */
  available: boolean;
}

/** Every package ART ships a recipe for, paired with whether its own archive
 *  was actually found in `packageFolder` — never assumed from the shipped
 *  list alone. Read-only; an unreadable folder answers the same way an empty
 *  one would (every package `available: false`) rather than refusing, so the
 *  checklist itself always renders. */
export async function osinstallPackages(packageFolder: string): Promise<PackageSummary[]> {
  return invoke<PackageSummary[]>("osinstall_packages", { packageFolder });
}

/** The event `osinstall_collisions`'s own background job answers on. */
export const OSINSTALL_COLLISIONS_EVENT = "osinstall-collisions-result";

interface OsInstallCollisionsResult {
  job_id: number;
  reports: CollisionReport[];
}

/**
 * What landing the chosen packages on `treeRoot` would actually do to the
 * files already there (§3's PREVIEW) — writes nothing. `packages` empty
 * answers `[]` **without calling into Tauri at all** (F5 of Task 7's own fix
 * round — an empty selection, or no package folder chosen yet, is guarded
 * here rather than asked of the backend), matching the panel's own rule
 * that the preview only appears once at least one package is chosen.
 *
 * Runs as a background job on the Rust side (F4 — a real BoingBag extracts
 * on the order of two hundred files, which is too long for the command
 * thread); this wrapper hides that behind the same
 * `Promise<CollisionReport[]>` shape it always had, by starting the job and
 * awaiting its own result event.
 */
export async function osinstallCollisions(
  treeRoot: string,
  packageFolder: string,
  packages: string[]
): Promise<CollisionReport[]> {
  if (!treeRoot || !packageFolder || packages.length === 0) return [];
  const jobId = await invoke<number>("osinstall_collisions", {
    treeRoot,
    packageFolder,
    packages,
  });
  return awaitJobResult<OsInstallCollisionsResult, CollisionReport[]>(
    jobId,
    OSINSTALL_COLLISIONS_EVENT,
    (payload) => payload.reports
  );
}

/** `osinstall_add_package`'s own answer — either a job started, or every
 *  typed reason it could not (F2). Mirrors `commands::osinstall::AddPackageResult`
 *  exactly, including the `outcome` tag. */
export type AddPackageResult =
  | { outcome: "started"; job_id: number }
  | { outcome: "refused"; refusals: RefusalReason[] };

/**
 * Add every chosen package to a distribution tree that already exists.
 * One job for the whole set, never one per package and never one per file,
 * matching the panel's single confirmation for the whole set. A refused
 * selection never reaches the job system at all — see [`AddPackageResult`].
 * Progress for a started job is the ordinary `job-progress` event, filtered
 * by its id, the same way `osinstallApply`'s own install job is tracked;
 * its outcome (files/directories/bytes actually written) arrives on
 * [`OSINSTALL_ADD_PACKAGE_EVENT`].
 */
export async function osinstallAddPackage(
  treeRoot: string,
  packageFolder: string,
  packages: string[]
): Promise<AddPackageResult> {
  return invoke<AddPackageResult>("osinstall_add_package", {
    treeRoot,
    packageFolder,
    packages,
  });
}

/** The event a finished (or failed) package-add job's own outcome arrives
 *  on (F10 — the counts used to reach nowhere but the oplog). */
export const OSINSTALL_ADD_PACKAGE_EVENT = "osinstall-add-package-result";

export interface OsInstallAddPackageResult {
  job_id: number;
  outcome: ApplyOutcome;
}

export async function onOsInstallAddPackageResult(
  handler: (result: OsInstallAddPackageResult) => void
): Promise<UnlistenFn> {
  return listen<OsInstallAddPackageResult>(OSINSTALL_ADD_PACKAGE_EVENT, (event) =>
    handler(event.payload)
  );
}

/**
 * `reports`, split into the five classes the preview groups by — downgrades
 * first ("a downgrade is what the user most needs to see": spec §3's whole
 * reason for existing is `ModulesA1200_3.2.adf`'s thirteen stale commands),
 * then upgrades, then same-version, then unversioned. `same-version` is
 * never folded into `unversioned`, here or anywhere else — the two mean
 * different things (both sides agree on a version vs. neither side says
 * anything at all). A group with nothing in it is omitted, not shown empty
 * (F1 of Task 7's own fix round — this replaces a flat, unlabelled sort
 * that gave a downgrade the same key, and so the same look, as an upgrade).
 */
export interface CollisionGroup {
  kind: Collision["kind"];
  reports: CollisionReport[];
}

const COLLISION_GROUP_ORDER: Collision["kind"][] = [
  "downgrade",
  "upgrade",
  "same-version",
  "unversioned",
];

export function groupCollisionsForPreview(reports: CollisionReport[]): CollisionGroup[] {
  return COLLISION_GROUP_ORDER.map((kind) => ({
    kind,
    reports: reports.filter((r) => r.collision.kind === kind),
  })).filter((group) => group.reports.length > 0);
}

/** The heading label for one group — its own translation key, not a value
 *  interpolated into a shared one, so "Downgrades" and "Upgrades" can read
 *  differently rather than only differing by colour (F1: colour alone is
 *  not marking). */
export function collisionGroupHeadingKey(kind: Collision["kind"]): string {
  switch (kind) {
    case "downgrade":
      return "osinstall.packages.preview.group.downgrade";
    case "upgrade":
      return "osinstall.packages.preview.group.upgrade";
    case "same-version":
      return "osinstall.packages.preview.group.sameVersion";
    case "unversioned":
      return "osinstall.packages.preview.group.unversioned";
  }
}

/** How many of each class `reports` holds — the preview heading's own
 *  counts, shown before the list so its shape ("3 downgrades, 41 upgrades,
 *  13 unversioned") is legible before the list itself is read. */
export interface CollisionCounts {
  downgrades: number;
  upgrades: number;
  sameVersion: number;
  unversioned: number;
}

export function collisionCounts(reports: CollisionReport[]): CollisionCounts {
  const counts: CollisionCounts = { downgrades: 0, upgrades: 0, sameVersion: 0, unversioned: 0 };
  for (const report of reports) {
    switch (report.collision.kind) {
      case "downgrade":
        counts.downgrades++;
        break;
      case "upgrade":
        counts.upgrades++;
        break;
      case "same-version":
        counts.sameVersion++;
        break;
      case "unversioned":
        counts.unversioned++;
        break;
    }
  }
  return counts;
}

/** The sentence for one row's own collision — what it would replace and
 *  with what, or the sizes alone when neither side names a version.
 *
 *  **F1** — `upgrade` and `downgrade` now read different keys (they used to
 *  share one, `collision.versioned`, which is why a downgrade rendered
 *  exactly like an upgrade with nothing but colour to tell them apart).
 *  **F12** — byte counts go through `formatBytes` (`@/lib/panel.ts`, the
 *  same helper the rest of the app already uses) rather than crossing raw:
 *  spec §3 shows sizes the way a person reads them, in KB/MB, not as an
 *  unbroken run of digits. */
export function collisionPhrase(collision: Collision): Phrase {
  switch (collision.kind) {
    case "upgrade":
      return {
        key: "osinstall.packages.collision.upgrade",
        params: { from: collision.from, to: collision.to },
      };
    case "downgrade":
      return {
        key: "osinstall.packages.collision.downgrade",
        params: { from: collision.from, to: collision.to },
      };
    case "same-version":
      return {
        key: "osinstall.packages.collision.sameVersion",
        params: { version: collision.version },
      };
    case "unversioned":
      return {
        key: "osinstall.packages.collision.unversioned",
        params: {
          fromBytes: formatBytes(collision.fromBytes),
          toBytes: formatBytes(collision.toBytes),
        },
      };
  }
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
/**
 * Whether something already sits where the tree would go.
 *
 * Read-only. `apply()` refuses an existing destination and always has; this
 * exists so the screen can say so *before* the button rather than after.
 */
export async function osinstallDestinationTaken(destination: string): Promise<boolean> {
  return invoke<boolean>("osinstall_destination_taken", { destination });
}

export function osinstallBlocker(input: {
  mediaFolder: string | null;
  destination: string | null;
  destinationTaken: boolean;
  plan: PlanResult | null;
}): Phrase | null {
  if (!input.mediaFolder?.trim()) return { key: "osinstall.blocked.noFolder" };
  if (!input.destination?.trim()) return { key: "osinstall.blocked.noDestination" };
  // Before the plan, because it is true regardless of what the plan says and
  // is the one blocker the user can fix without understanding any of the rest.
  if (input.destinationTaken) {
    return { key: "osinstall.blocked.destinationExists", params: { path: input.destination } };
  }
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
    case "package-unknown":
      return { key: "osinstall.refusal.packageUnknown", params: { package: reason.package } };
    case "package-folder-missing":
      return {
        key: "osinstall.refusal.packageFolderMissing",
        params: { packages: reason.packages.join(", ") },
      };
    case "package-requirement-missing":
      return {
        key: "osinstall.refusal.packageRequirementMissing",
        params: { package: reason.package, requires: reason.requires },
      };
    case "package-component-missing":
      return {
        key: "osinstall.refusal.packageComponentMissing",
        params: { package: reason.package, component: reason.component },
      };
    case "package-archive-missing":
      return {
        key: "osinstall.refusal.packageArchiveMissing",
        params: { package: reason.package, media: reason.media },
      };
    case "package-archive-ambiguous":
      return {
        key: "osinstall.refusal.packageArchiveAmbiguous",
        params: { package: reason.package, media: reason.media, paths: reason.paths.join(", ") },
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
// The component catalogue — the chosen release's own recipe, loaded
//
// **Not a hand-written mirror any more.** This module used to carry
// `AMIGAOS_32_COMPONENTS`, a literal copy of
// `src-tauri/src/core/osinstall/recipes/amigaos-3.2.json`, and the OS Install
// screen rendered it whatever release the picker had selected. With
// "AmigaOS 3.9" chosen the user saw 26 components for a recipe that holds
// one, and 3.9's own `workbench-base` was labelled `Workbench3.2` — both
// recipes use that component id, so a label resolved against the wrong
// recipe named a floppy volume that has nothing to do with the disc being
// installed from. Nothing was ever written wrongly (the engine is the
// authority and ignores an id its recipe does not hold), but showing one
// operating system's parts while installing another's is §89 on the screen
// itself.
//
// A parity test cannot fix that, only a second copy of the same mistake:
// the list has to *be* the recipe. `osinstallComponents` asks the Rust side,
// which projects the chosen release's own `Recipe` and refuses a release it
// ships no recipe for. Everything below therefore takes the loaded list as
// its first argument rather than reaching for a module constant — a
// component id means nothing without knowing which release is being
// installed.
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
  /** `Condition::RomOlderThan { major }`, flattened by the command — `null`
   *  for an unconditional component. The Rust side's `match` is exhaustive,
   *  so a second `Condition` variant is a compile error there rather than a
   *  silent `null` here. */
  conditionMajor: number | null;
  exclusiveGroup: string | null;
}

/**
 * Which components `release`'s own shipped recipe holds, in recipe order.
 *
 * Read-only: parses shipped JSON, opens no media, writes nothing. An unknown
 * release rejects rather than answering with a default catalogue.
 */
export async function osinstallComponents(release: string): Promise<ComponentDef[]> {
  return invoke<ComponentDef[]>("osinstall_components", { release });
}

/**
 * Where one release's component picks are remembered.
 *
 * **Per release, not shared.** A component id means something only inside
 * the recipe that declares it, and the two shipped recipes both use
 * `workbench-base` for different media. Sharing one remembered set would
 * mean either carrying ids the current release cannot install, or dropping
 * ids the *other* release legitimately holds the moment the user switched —
 * and a choice destroyed by switching away and back is exactly the thing
 * this project's remembered-settings rule forbids.
 *
 * `AmigaOS 3.2` keeps the unsuffixed key it has always used. It is the only
 * release that existed before there was a picker, so anyone upgrading into
 * this version finds the selection they last made still ticked, rather than
 * an empty list under a key nothing ever wrote.
 */
const RELEASE_BEFORE_THE_PICKER = "AmigaOS 3.2";

export function rememberedComponentKey(base: string, release: string): string {
  return release === RELEASE_BEFORE_THE_PICKER ? base : `${base}.${release}`;
}

export function componentDef(components: ComponentDef[], id: string): ComponentDef | undefined {
  return components.find((c) => c.id === id);
}

/** A component's own label — the volume name the recipe names, or the bare
 *  id when the loaded release does not hold it (a plan item from a release
 *  whose list has not arrived yet, never a fabricated volume name). */
export function componentLabel(components: ComponentDef[], id: string): string {
  return componentDef(components, id)?.media ?? id;
}

/** `chosen`, with anything that is not a real, available component id of
 *  `components` dropped — a stale remembered id (an old ART's component that
 *  was renamed or removed) or an unavailable one (Coming Later) can never
 *  reach `InstallRequest.chosen`.
 *
 *  **Only call this with a list that has actually loaded.** Against an empty
 *  `components` it drops everything, and persisting that would be a setting
 *  changing without the user changing it (ART-089's shape). The screen holds
 *  `null` for "not loaded yet" and does not sanitize until it is a list. */
export function sanitizeChosen(components: ComponentDef[], chosen: string[]): string[] {
  return chosen.filter((id) => componentDef(components, id)?.available === true);
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
export function isForcedOnByCondition(
  components: ComponentDef[],
  basePlan: InstallPlan | null,
  chosen: string[],
  id: string
): boolean {
  const def = componentDef(components, id);
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
  components: ComponentDef[],
  basePlan: InstallPlan,
  chosen: string[],
  excluded: string[]
): string[] {
  return excluded.filter((id) => isForcedOnByCondition(components, basePlan, chosen, id));
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
export function toggleChosen(components: ComponentDef[], chosen: string[], id: string): string[] {
  if (chosen.includes(id)) return chosen.filter((c) => c !== id);
  const group = componentDef(components, id)?.exclusiveGroup ?? null;
  const withoutGroup = group
    ? chosen.filter((c) => componentDef(components, c)?.exclusiveGroup !== group)
    : chosen;
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
