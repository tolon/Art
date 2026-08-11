// Pane-kind routing for the disc pane, factored out of `FileManager.tsx` so
// it can be tested without rendering the page — the same reason
// `@/lib/selection` and `@/lib/sort` exist (see their own header comments).
//
// Two things a disc pane needs that `FileManager.tsx`'s existing "local" /
// "adf" / "hdf" three-way switch does not:
//
//   1. Which copy pipeline a source/target pane pair needs — `copyDirection`
//      — because a disc adds a fourth kind and, uniquely, a *forbidden*
//      target rather than just another source.
//   2. How the disc's own breadcrumb trail moves — `enterIsoTrail` /
//      `leaveIsoTrail` — because an ISO directory is addressed by
//      `(extent, length)`, not a block number, so it cannot reuse the
//      ADF/HDF trail's `{ name, block }` shape (see `PaneState` in
//      `FileManager.tsx` for why that pair is kept separate rather than
//      overloading `dirBlock`/`trail`).

import type { Phrase } from "@/lib/phrase";

export type PaneKind = "local" | "adf" | "hdf" | "iso";

/** ADF and HDF panes share every write/copy command, indexed by
 * `volumeIndex` — a disc and a local folder do not. */
export function isVolumeKind(kind: PaneKind): boolean {
  return kind === "adf" || kind === "hdf";
}

/** The sentence a disc pane shows for every write attempt — F5 in, F6, F7,
 * F8 all refuse with this one reason. Exported so `FileManager.tsx`'s
 * capability text and `copyDirection`'s refusal use the same key rather than
 * two copies of the same string drifting apart. */
export const ISO_WRITE_REFUSAL: Phrase = { key: "files.writeRefusal.iso" };

export type CopyDirection =
  | { kind: "local-to-local" }
  | { kind: "local-to-volume" }
  | { kind: "volume-to-local" }
  | { kind: "volume-to-volume" }
  | { kind: "iso-to-local" }
  | { kind: "iso-to-volume" }
  /** Refused before anything runs — the pane shows `reason` instead. */
  | { kind: "refused"; reason: Phrase };

/**
 * Which copy pipeline moves an entry from a `source`-kind pane to a
 * `target`-kind one — the routing F5 and a drag-and-drop both need before
 * picking a command to call.
 *
 * A disc is read-only (Task 3 brief): every direction that would write
 * *into* one is `"refused"`, whichever kind the source is — a local folder,
 * a volume, or another disc. That is also why `target === "iso"` is checked
 * first: it must win over every other combination, including `iso` on both
 * sides, rather than falling through to a copy pipeline that happens to
 * match the source instead.
 */
export function copyDirection(source: PaneKind, target: PaneKind): CopyDirection {
  if (target === "iso") {
    return { kind: "refused", reason: ISO_WRITE_REFUSAL };
  }
  if (source === "iso") {
    return { kind: target === "local" ? "iso-to-local" : "iso-to-volume" };
  }
  if (source === "local" && target === "local") {
    return { kind: "local-to-local" };
  }
  if (source === "local") {
    return { kind: "local-to-volume" };
  }
  if (target === "local") {
    return { kind: "volume-to-local" };
  }
  return { kind: "volume-to-volume" };
}

/** One breadcrumb in a disc pane's trail: the directory to return to on
 * "up", tagged with the name of the directory entered to leave it — the
 * same shape and the same reason the ADF/HDF trail captures `state.dirBlock`
 * at the moment of entry (see `FileManager.tsx`'s `openAdf`/`openVolume`
 * callers), just carrying an `(extent, length)` pair instead of a block. */
export interface IsoTrailEntry {
  name: string;
  extent: number;
  length: number;
}

/** Navigate into a subdirectory: push the directory being left onto the
 * trail, named after the one just entered. */
export function enterIsoTrail(
  trail: IsoTrailEntry[],
  enteredName: string,
  fromExtent: number,
  fromLength: number
): IsoTrailEntry[] {
  return [...trail, { name: enteredName, extent: fromExtent, length: fromLength }];
}

/** Navigate up one level: pop the last breadcrumb and read where it points.
 * `null` when the trail is already empty — already at the root, nothing to
 * go up to. */
export function leaveIsoTrail(
  trail: IsoTrailEntry[]
): { extent: number; length: number; trail: IsoTrailEntry[] } | null {
  if (trail.length === 0) return null;
  const rest = trail.slice(0, -1);
  const previous = trail[trail.length - 1];
  return { extent: previous.extent, length: previous.length, trail: rest };
}
