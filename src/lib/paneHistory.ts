// Where a pane is, as data — and the per-pane back/forward history built on it
// (brief §3.1's last line, and §3.2's Alt+Left / Alt+Right).
//
// A `PaneLocation` is everything needed to *re-open* a pane, and nothing else:
// no entries, no capability, no warnings, none of the things `PaneState` also
// carries because they were fetched. That split is what makes it useful twice
// — history here, and **session restore in task 6**, which has to write the
// same answer to disk and read it back. Designing it once, as a serialisable
// shape rather than a closure, is why this module exists at all.
//
// The container step is part of a location, not something layered on top of
// it: a pane inside `Lotus.adf` has a *place*, and going back to it means
// going back inside the image, at the same directory, with the same host to
// return to. That is the whole difference between "history" and "a list of
// paths".

import type { IsoTrailEntry, PaneKind } from "@/lib/isoPane";
import type { HostReturn } from "@/lib/containerStep";

/** One step of an ADF/HDF walk — the shape `PaneState.trail` already uses. */
export interface TrailStep {
  name: string;
  block: number | null;
}

export type PaneLocation =
  | { kind: "local"; path: string }
  | {
      kind: "adf";
      path: string;
      dirBlock: number | null;
      trail: TrailStep[];
      host: HostReturn | null;
    }
  | {
      kind: "hdf";
      path: string;
      /** `null` means the partition list itself, which is a level a pane can
       *  genuinely be at — an HDF opens on it. */
      volumeIndex: number | null;
      dirBlock: number | null;
      trail: TrailStep[];
      host: HostReturn | null;
    }
  | {
      kind: "iso";
      path: string;
      extent: number | null;
      length: number | null;
      trail: IsoTrailEntry[];
      host: HostReturn | null;
    }
  | { kind: "archive"; path: string; dir: string; host: HostReturn | null }
  | { kind: "c64"; path: string; host: HostReturn | null };

/** A pane's back/forward list and where in it the pane currently is. */
export interface PaneHistory {
  entries: PaneLocation[];
  /** Index into `entries`. `-1` only while the history is empty. */
  index: number;
}

export function emptyHistory(): PaneHistory {
  return { entries: [], index: -1 };
}

/** The pane kind a location opens. */
export function kindOf(location: PaneLocation): PaneKind {
  return location.kind;
}

/**
 * Whether two locations are the same place.
 *
 * Compared field by field rather than by `JSON.stringify`, which would make
 * two identical places unequal because their keys happened to be built in a
 * different order — and the only thing this function is used for is *not*
 * pushing the same place onto the history twice, so a false "different" is a
 * history full of duplicates and a Back key that does nothing.
 */
export function sameLocation(a: PaneLocation, b: PaneLocation): boolean {
  if (a.kind !== b.kind) return false;
  if (a.path !== b.path) return false;

  switch (a.kind) {
    case "local":
      return true;
    case "adf":
      return a.dirBlock === (b as typeof a).dirBlock;
    case "hdf": {
      const other = b as typeof a;
      return a.volumeIndex === other.volumeIndex && a.dirBlock === other.dirBlock;
    }
    case "iso": {
      const other = b as typeof a;
      return a.extent === other.extent && a.length === other.length;
    }
    case "archive":
      return a.dir === (b as typeof a).dir;
    case "c64":
      return true;
  }
}

/**
 * Record a move to `next`.
 *
 * Browser semantics, because they are the ones everybody already has: moving
 * somewhere new after going Back **discards the forward entries** — keeping
 * them would offer a Forward that leads somewhere the user has since branched
 * away from. Moving to where the pane already is changes nothing at all,
 * which is what keeps a refresh (F2) out of the history.
 */
export function pushLocation(history: PaneHistory, next: PaneLocation): PaneHistory {
  const current = history.entries[history.index];
  if (current && sameLocation(current, next)) return history;

  const kept = history.entries.slice(0, history.index + 1);
  return { entries: [...kept, next], index: kept.length };
}

/** Where Back goes, or `null` at the start of the history. */
export function goBack(history: PaneHistory): { history: PaneHistory; to: PaneLocation } | null {
  if (history.index <= 0) return null;
  const index = history.index - 1;
  return { history: { ...history, index }, to: history.entries[index] };
}

/** Where Forward goes, or `null` at the end of it. */
export function goForward(history: PaneHistory): { history: PaneHistory; to: PaneLocation } | null {
  if (history.index < 0 || history.index >= history.entries.length - 1) return null;
  const index = history.index + 1;
  return { history: { ...history, index }, to: history.entries[index] };
}

/**
 * Where `[..]` goes from a container's root: back out to the host folder, with
 * the cursor on the container file.
 *
 * `null` for a pane that was never entered from a folder — an image opened
 * straight from the source combo has nowhere to go back *out* to, and `[..]`
 * is correctly absent there. That is the one case where leaving a container
 * is not possible, and it is not an error: the pane is where it started.
 */
export function leaveToHost(host: HostReturn | null): { path: string; cursor: string } | null {
  return host ? { path: host.path, cursor: host.name } : null;
}
