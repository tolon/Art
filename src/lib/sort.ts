// The two-pane file manager's column sort, factored out so it can be tested
// without rendering `FileManager.tsx` (which calls Tauri commands on mount) —
// the same reason `src/lib/selection.ts` exists on its own. See that file's
// comment for the full rationale.
//
// One rule underneath everything here: folders stay first, in both sort
// directions. `commands/panel.rs`, `core/adf/fs.rs` and
// `core/volume/write/dir.rs::entries_in` already hand back a folders-first,
// case-insensitive-name listing (see their own comments), so this only adds
// a *further* ordering on top of that known baseline — it never has to cope
// with an unsorted pane.

import type { PanelEntry } from "@/lib/panel";

export type SortColumn = "name" | "size" | "date";
export type SortDirection = "asc" | "desc";

export interface SortState {
  column: SortColumn;
  direction: SortDirection;
}

/** What a freshly opened pane starts at, and what navigation resets to. */
export function defaultSortState(): SortState {
  return { column: "name", direction: "asc" };
}

/**
 * Clicking a column header: sort by it if it was not already the active
 * column, or reverse direction if it was.
 */
export function clickColumn(current: SortState, column: SortColumn): SortState {
  if (current.column !== column) return { column, direction: "asc" };
  return { column, direction: current.direction === "asc" ? "desc" : "asc" };
}

/** Case-insensitive name compare — the tie-break every column falls back to. */
function compareName(a: PanelEntry, b: PanelEntry): number {
  return a.name.localeCompare(b.name, undefined, { sensitivity: "base" });
}

/** The chosen column's raw compare, before direction or the folders-first rule. */
function compareColumn(a: PanelEntry, b: PanelEntry, column: SortColumn): number {
  switch (column) {
    case "name":
      return compareName(a, b);
    case "size":
      return a.bytes - b.bytes;
    case "date": {
      // An entry with no date sorts as if it were the oldest possible, in
      // either direction — "unknown" reads more like "long ago" than like
      // "just now", and it keeps the comparator from having to invert its
      // meaning depending on which side of the pair is missing one.
      // `Number.MIN_SAFE_INTEGER` rather than `-Infinity`: two undated
      // entries would otherwise subtract to `-Infinity - -Infinity`, which is
      // `NaN` — not `0` — and a comparator that can return `NaN` is not a
      // total order.
      const aDate = a.date ?? Number.MIN_SAFE_INTEGER;
      const bDate = b.date ?? Number.MIN_SAFE_INTEGER;
      return aDate - bDate;
    }
  }
}

/**
 * Two entries, ordered for display under `sort`.
 *
 * A total order: folders are always first regardless of direction (a
 * commander that scatters directories through the file list is not one —
 * see the file manager's own rules), and within a group, entries that tie on
 * the chosen column fall back to case-insensitive name so the order never
 * flickers between renders — `Array.prototype.sort` is not guaranteed
 * stable-under-reflow across every engine ART targets, and re-sorting an
 * already-sorted list must be a no-op.
 */
export function compareEntries(a: PanelEntry, b: PanelEntry, sort: SortState): number {
  if (a.is_dir !== b.is_dir) return a.is_dir ? -1 : 1;

  const direction = sort.direction === "asc" ? 1 : -1;
  const primary = compareColumn(a, b, sort.column) * direction;
  if (primary !== 0) return primary;

  return compareName(a, b);
}

/** `entries`, sorted under `sort`. Does not mutate its input. */
export function sortEntries(entries: PanelEntry[], sort: SortState): PanelEntry[] {
  return [...entries].sort((a, b) => compareEntries(a, b, sort));
}
