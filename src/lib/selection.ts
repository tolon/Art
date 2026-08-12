// The two-pane file manager's multi-select reducer, factored out so it can be
// tested without rendering `FileManager.tsx` (which calls Tauri commands on
// mount and pulls in most of the app's `lib/*` surface).
//
// A pane's selection is `Set<string>` of entry *names* — `PanelEntry` has no
// id, and names are unique within a directory on both sides (see
// `src/lib/panel.ts`). Alongside it sits an `anchor`: the name of the entry
// the keyboard/mouse last landed on, used both to extend a Shift+click range
// and as Norton Commander's "cursor" for Insert. It is deliberately a
// separate value from the selection itself — a row can be the anchor without
// being selected, and `FileManager.tsx` uses that gap to give the anchor row
// its own highlight, distinct from a selected one.
//
// Every function here is pure: entries in, `SelectionUpdate` out. Nothing
// touches React state, so a caller (a click handler, a keyboard hook) only
// has to thread the two pieces of state through.

import { matchesMask } from "@/lib/mask";
import type { PanelEntry } from "@/lib/panel";

export interface SelectionUpdate {
  selected: Set<string>;
  anchor: string | null;
}

/** No selection, no anchor — the state every pane starts in, and returns to
 * on navigation (§ "reset on navigation" in the file manager's own rules):
 * a Set that survived a directory change would let an action reach an entry
 * the user has since left behind. */
export function emptySelectionUpdate(): SelectionUpdate {
  return { selected: new Set(), anchor: null };
}

/** Plain click — select only this entry, and anchor future Shift+clicks here. */
export function selectOnly(name: string): SelectionUpdate {
  return { selected: new Set([name]), anchor: name };
}

/** Ctrl/Cmd+click — toggle this entry, keep the rest, move the anchor here. */
export function toggleOne(selected: Set<string>, name: string): SelectionUpdate {
  const next = new Set(selected);
  if (next.has(name)) next.delete(name);
  else next.add(name);
  return { selected: next, anchor: name };
}

/**
 * Shift+click — select the contiguous run between the anchor and `name`,
 * inclusive, adding it to whatever was already selected. The anchor itself
 * does not move: a second Shift+click extends or shrinks the same range
 * rather than starting a new one from the last click.
 *
 * When the anchor names an entry no longer in `entries` (the directory
 * changed under it, or there was never one) this falls back to a plain
 * single-entry selection rather than guessing a range.
 */
export function selectRange(
  entries: PanelEntry[],
  selected: Set<string>,
  anchor: string | null,
  name: string
): SelectionUpdate {
  const names = entries.map((e) => e.name);
  const anchorName = anchor ?? name;
  const from = names.indexOf(anchorName);
  const to = names.indexOf(name);
  if (from === -1 || to === -1) return selectOnly(name);

  const [lo, hi] = from <= to ? [from, to] : [to, from];
  const next = new Set(selected);
  for (let i = lo; i <= hi; i++) next.add(names[i]);
  return { selected: next, anchor: anchorName };
}

/**
 * Insert — Norton Commander's mark key: toggle the entry the anchor sits on
 * and move the anchor down one, so repeated presses mark a run without a
 * mouse. With no prior anchor (nothing clicked yet this pane) it starts at
 * the first entry; with a stale anchor (the directory changed under it) it
 * falls back the same way. Does nothing on an empty pane.
 */
export function insertToggle(
  entries: PanelEntry[],
  selected: Set<string>,
  anchor: string | null
): SelectionUpdate {
  if (entries.length === 0) return { selected, anchor };

  const names = entries.map((e) => e.name);
  const index = anchor !== null ? Math.max(names.indexOf(anchor), 0) : 0;
  const name = names[index];

  const next = new Set(selected);
  if (next.has(name)) next.delete(name);
  else next.add(name);

  const nextIndex = Math.min(index + 1, names.length - 1);
  return { selected: next, anchor: names[nextIndex] };
}

/** Ctrl+A — select every entry in the pane; press again to clear it. */
export function toggleSelectAll(entries: PanelEntry[], selected: Set<string>): SelectionUpdate {
  const allSelected = entries.length > 0 && entries.every((e) => selected.has(e.name));
  if (allSelected) return { selected: new Set(), anchor: null };
  return { selected: new Set(entries.map((e) => e.name)), anchor: null };
}

/**
 * Num + / Num − — mark, or unmark, everything matching a filename mask.
 *
 * The mask is the same `*`/`?` matcher the filter box uses (`@/lib/mask`), so
 * a user who has learned one has learned both. It **adds to** the selection
 * rather than replacing it: Total Commander's Num+ is how a selection gets
 * built up out of several patterns, and a version that started over each time
 * would make "*.adf then *.hdf" impossible.
 */
export function markByMask(
  entries: PanelEntry[],
  selected: Set<string>,
  mask: string,
  mark: boolean
): SelectionUpdate {
  const next = new Set(selected);
  for (const entry of entries) {
    if (!matchesMask(entry.name, mask)) continue;
    if (mark) next.add(entry.name);
    else next.delete(entry.name);
  }
  return { selected: next, anchor: null };
}

/**
 * Num * — invert the selection.
 *
 * Over the entries the pane is *showing*, which is what makes it compose with
 * the filter box: mask the pane to `*.lha`, invert, and you have every `.lha`
 * that was not already marked — not every file on the disk.
 */
export function invertSelection(entries: PanelEntry[], selected: Set<string>): SelectionUpdate {
  const next = new Set<string>();
  for (const entry of entries) {
    if (!selected.has(entry.name)) next.add(entry.name);
  }
  return { selected: next, anchor: null };
}

/**
 * Space — mark the entry under the cursor, and stay where you are.
 *
 * The difference from `insertToggle` is the whole point of having both: Insert
 * marks and steps down, which is how a run of files gets marked; Space marks
 * without moving, which is how one file gets marked without losing your place.
 * Total Commander has both for that reason.
 */
export function spaceToggle(selected: Set<string>, cursor: string | null): SelectionUpdate {
  if (!cursor) return { selected, anchor: cursor };
  const next = new Set(selected);
  if (next.has(cursor)) next.delete(cursor);
  else next.add(cursor);
  return { selected: next, anchor: cursor };
}

/** The entries a selection actually names, in pane order. */
export function entriesIn(entries: PanelEntry[], names: Set<string>): PanelEntry[] {
  return entries.filter((e) => names.has(e.name));
}

/**
 * The one entry a single-entry action (View, Edit, Rename, Attributes...)
 * may act on — `null` whenever the selection holds anything but exactly one
 * entry, so a caller can refuse cleanly rather than picking an arbitrary one
 * out of several.
 */
export function singleSelected(entries: PanelEntry[], selected: Set<string>): PanelEntry | null {
  if (selected.size !== 1) return null;
  const [name] = selected;
  return entries.find((e) => e.name === name) ?? null;
}
