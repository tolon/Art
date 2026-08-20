// Laying content out into a staging tree (SD-2 · G11).
// Mirrors src-tauri/src/commands/layout.rs and src-tauri/src/core/layout/{mod,policy,apply}.rs.
//
// The staging seam is not a preference — a real PiStorm card is PFS3 and ART
// cannot write PFS3, so writing straight into the volume works only on FFS,
// which is not what a finished card uses. `layoutPlan` turns a pile of
// dropped paths into a proposed tree on the PC; the user edits it — retarget
// a row into another drawer — and only then `layoutApply` copies it in.
//
// **`layoutApply` takes the plan it is given and does not recompute it.**
// That is the opposite of `preloadRun`, which deliberately recomputes so a
// screen cannot preview one card and format another. Here the user's edits
// *are* the plan — retargeting rows in the preview is the whole feature — so
// recomputing would throw away exactly what they came to do. What guards the
// tree instead is the applier: `safe_join` on every destination (user-typed
// text is untrusted like an archive entry name) and a refusal on anything
// already there.

import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

import type { Phrase } from "@/lib/phrase";

// ---------------------------------------------------------------------------
// Types, mirroring core::layout exactly
// ---------------------------------------------------------------------------

/** What happens to an archive holding a WHDLoad pack. */
export type WhdloadPlacement = "unpack" | "as-archive";

/** Which drawer each kind of thing goes in. */
export interface Policy {
  whdload: WhdloadPlacement;
  games: string;
  floppies: string;
  hard_disks: string;
  discs: string;
  unsorted: string;
}

/** What ART can justify saying about one thing on disk. There is no `Demo`
 *  and there will not be one — see core/layout/mod.rs's own note. */
export type ItemKind =
  | { kind: "whdload-archive"; name: string }
  | { kind: "whdload-drawer"; name: string }
  | { kind: "floppy-image" }
  | { kind: "hard-disk-image" }
  | { kind: "optical-image" }
  | { kind: "archive" }
  | { kind: "unknown" }
  | { kind: "rom" }
  | { kind: "commodore8-bit" };

/** How an item reaches the staging tree. */
export type Placement = "copy-file" | "copy-tree" | "unpack-whdload";

export interface LayoutItem {
  source: string;
  kind: ItemKind;
  /** Relative to the staging root, `/` separated. Proposed by the policy; the
   *  user may change it before the plan is applied. */
  destination: string;
  placement: Placement;
  /** What this will occupy once placed. For an unpacked archive this is the
   *  archive's own declared uncompressed total — a claim, and named as one. */
  bytes: number;
}

export type RefusalReason = "belongs-on-boot-partition" | "no-place-on-an-amiga-volume";

export interface Refusal {
  source: string;
  reason: RefusalReason;
}

/** Two or more things wanting one name, or one thing wanting a name the
 *  staging tree already holds. */
export interface Collision {
  destination: string;
  /** Empty of a second entry when the clash is with a file already on disk. */
  sources: string[];
}

/**
 * Things a scan did not put in the plan, named rather than dropped in silence
 * (ART-107). Mirrors `core::layout::scan::Dropped`: `paths` is capped at
 * twenty, and `more` counts whatever did not fit — so the real total is
 * `paths.length + more`, never `paths.length`.
 */
export interface Dropped {
  paths: string[];
  more: number;
}

/** How many things a {@link Dropped} really stands for. */
export function droppedTotal(dropped: Dropped): number {
  return dropped.paths.length + dropped.more;
}

export interface LayoutPlan {
  root: string;
  items: LayoutItem[];
  refused: Refusal[];
  collisions: Collision[];
  bytes: number;
  /** Folders the scan did not look inside — see {@link Dropped}. */
  tooDeep: Dropped;
  /** Sources another source already covered — see {@link Dropped}. */
  duplicates: Dropped;
  /**
   * Destinations that already hold **exactly** what this plan would put
   * there (ART-177) — skipped by Apply, and counted on screen.
   *
   * Deliberately not among `collisions`: a collision is a question for the
   * user, and this is not one. It is what makes re-running a half-finished
   * apply finish it, with no "continue" button and no resume mode.
   */
  alreadyInPlace: string[];
}

export interface LayoutRequest {
  root: string;
  paths: string[];
  policy: Policy;
}

export interface ApplyOutcome {
  placed: number;
  bytes: number;
  /** Items stepped over because the destination already held exactly them. */
  skipped: number;
}

export const LAYOUT_EVENT = "layout-result";

export interface LayoutResult {
  job_id: number;
  root: string;
  outcome: ApplyOutcome;
}

// ---------------------------------------------------------------------------
// The commands
// ---------------------------------------------------------------------------

export const LAYOUT_PLAN_EVENT = "layout-plan-result";

export interface LayoutPlanResult {
  job_id: number;
  plan: LayoutPlan;
}

/**
 * What laying these out would do. Writes nothing (§92's PREVIEW). Returns a
 * **job id**; the plan arrives on {@link onLayoutPlanResult}.
 *
 * A job because planning is no longer cheap. Since ART-177 the preview
 * compares destination **content**, so a plan over a staging tree that
 * already holds its output — the resume case, which is what the feature is
 * for — reads every one of those files in full. Measured on the owner's own
 * collection (1 697 WHDLoad HDFs, 3.74 GB): 797 ms for a first plan and
 * **138 898 ms** for a resume. That is not something to do on the command
 * thread (§54), and `archivesPlanInstall` took the same route for the same
 * reason (ART-066).
 *
 * A cancelled or failed job never sends the event; `onJobProgress` is where
 * the screen learns that.
 */
export async function layoutPlan(request: LayoutRequest): Promise<number> {
  return invoke<number>("layout_plan", { request });
}

/** Subscribe to finished plans. */
export async function onLayoutPlanResult(
  handler: (result: LayoutPlanResult) => void
): Promise<UnlistenFn> {
  return listen<LayoutPlanResult>(LAYOUT_PLAN_EVENT, (event) => handler(event.payload));
}

/**
 * Ask the engine which of `plan`'s **current** destinations collide, on disk
 * or with each other. Not a replan — no walking, no classifying, no policy —
 * just `core::layout::collisions_in` re-run over the plan exactly as it
 * stands, which is the one question `retarget` cannot answer on its own: it
 * only knows the plan in front of it, and whether a new destination already
 * exists on disk is a fact only the engine has looked at.
 */
export interface RecheckResult {
  collisions: Collision[];
  /** Destinations already holding exactly what the plan would put there. */
  already_in_place: string[];
}

export async function layoutRecheck(plan: LayoutPlan): Promise<RecheckResult> {
  return invoke<RecheckResult>("layout_recheck", { plan });
}

/**
 * Build the staging tree. Returns a job id (§54).
 *
 * Takes the plan as the user edited it — see the module note above for why
 * this does not recompute the way `preloadRun` does.
 */
export async function layoutApply(plan: LayoutPlan): Promise<number> {
  return invoke<number>("layout_apply", { plan });
}

/** Subscribe to finished layouts. A cancelled or failed job never sends one —
 *  the job bar is where those are seen. */
export async function onLayoutResult(
  handler: (result: LayoutResult) => void
): Promise<UnlistenFn> {
  return listen<LayoutResult>(LAYOUT_EVENT, (event) => handler(event.payload));
}

// ---------------------------------------------------------------------------
// What the screen holds, and the rules over it
// ---------------------------------------------------------------------------

/**
 * Move the chosen rows into `drawer`, keeping each one's own leaf name.
 *
 * The collisions this leaves on the plan are **only the ones inside it** —
 * two rows now wanting the same name. Whether either new destination already
 * exists on disk is a fact only the engine has looked at, and `retarget`
 * cannot answer it from the plan alone: the caller must follow this with
 * `layoutRecheck` and fold its answer back in before trusting the plan's
 * `collisions` again. `ContentLayout.tsx` does exactly that after every
 * retarget, which is why there is no "stale" flag on the screen — the
 * question `retarget` cannot answer here gets asked, not deferred.
 */
export function retarget(plan: LayoutPlan, indices: number[], drawer: string): LayoutPlan {
  if (indices.length === 0) return plan;
  const chosen = new Set(indices);
  const items = plan.items.map((item, index) => {
    if (!chosen.has(index)) return item;
    const leaf = item.destination.split("/").pop() ?? item.destination;
    return { ...item, destination: `${drawer}/${leaf}` };
  });
  // `alreadyInPlace` is carried over untouched: it is a fact about the disk,
  // and `retarget` has not looked at the disk. The `layoutRecheck` that
  // follows every retarget replaces both it and `collisions` with answers the
  // engine computed together — see `ContentLayout.tsx`.
  return { ...plan, items, collisions: collisionsIn(items) };
}

/**
 * Destinations two rows want, **within this plan only**.
 *
 * A destination the staging tree already holds on disk is not decided here —
 * only the engine has looked at the disk, so an on-disk collision from the
 * last `layoutPlan` (or `layoutRecheck`) does **not** survive a `retarget`
 * that never touched it. That is exactly why `retarget` cannot be the last
 * word on `plan.collisions`: see this function's callers.
 */
function collisionsIn(items: LayoutItem[]): Collision[] {
  const by = new Map<string, string[]>();
  for (const item of items) {
    by.set(item.destination, [...(by.get(item.destination) ?? []), item.source]);
  }
  return [...by.entries()]
    .filter(([, sources]) => sources.length > 1)
    .map(([destination, sources]) => ({ destination, sources }));
}

/**
 * Why the layout cannot be applied yet, or null when it can.
 *
 * A reason rather than a boolean: a disabled button that does not say why is
 * the defect ART-100 was.
 */
export function layoutBlocker(input: {
  root: string | null;
  paths: string[];
  plan: LayoutPlan | null;
}): Phrase | null {
  if (!input.root?.trim()) return { key: "layout.blocked.noRoot" };
  if (input.paths.length === 0) return { key: "layout.blocked.nothingToPlace" };
  if (!input.plan) return { key: "layout.blocked.notPlanned" };
  if (input.plan.items.length === 0) return { key: "layout.blocked.nothingToPlace" };
  if (input.plan.collisions.length > 0) return { key: "layout.blocked.collisions" };
  return null;
}

/** The sentence for why one item was refused, for the component to render. */
export function refusalPhrase(reason: RefusalReason): Phrase {
  switch (reason) {
    case "belongs-on-boot-partition":
      return { key: "layout.refusal.belongsOnBootPartition" };
    case "no-place-on-an-amiga-volume":
      return { key: "layout.refusal.noPlaceOnAnAmigaVolume" };
  }
}

/** The sentence naming what one item is, for the component to render. */
export function kindPhrase(kind: ItemKind): Phrase {
  switch (kind.kind) {
    case "whdload-archive":
      return { key: "layout.kind.whdloadArchive", params: { name: kind.name } };
    case "whdload-drawer":
      return { key: "layout.kind.whdloadDrawer", params: { name: kind.name } };
    case "floppy-image":
      return { key: "layout.kind.floppyImage" };
    case "hard-disk-image":
      return { key: "layout.kind.hardDiskImage" };
    case "optical-image":
      return { key: "layout.kind.opticalImage" };
    case "archive":
      return { key: "layout.kind.archive" };
    case "unknown":
      return { key: "layout.kind.unknown" };
    case "rom":
      return { key: "layout.kind.rom" };
    case "commodore8-bit":
      return { key: "layout.kind.commodore8Bit" };
  }
}
