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

export interface LayoutPlan {
  root: string;
  items: LayoutItem[];
  refused: Refusal[];
  collisions: Collision[];
  bytes: number;
}

export interface LayoutRequest {
  root: string;
  paths: string[];
  policy: Policy;
}

export interface ApplyOutcome {
  placed: number;
  bytes: number;
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

/** What laying these out would do. Writes nothing (§92's PREVIEW). */
export async function layoutPlan(request: LayoutRequest): Promise<LayoutPlan> {
  return invoke<LayoutPlan>("layout_plan", { request });
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

/** Move the chosen rows into `drawer`, keeping each one's own leaf name. */
export function retarget(plan: LayoutPlan, indices: number[], drawer: string): LayoutPlan {
  if (indices.length === 0) return plan;
  const chosen = new Set(indices);
  const items = plan.items.map((item, index) => {
    if (!chosen.has(index)) return item;
    const leaf = item.destination.split("/").pop() ?? item.destination;
    return { ...item, destination: `${drawer}/${leaf}` };
  });
  return { ...plan, items, collisions: collisionsIn(items) };
}

/**
 * Destinations two rows want.
 *
 * **Only the ones inside this plan.** A destination the staging tree already
 * holds is a fact about the disk, and only the engine has looked at the disk —
 * so those survive from the last `layoutPlan` and are recomputed when the
 * user previews again.
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
