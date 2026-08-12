// Tabs, per pane (brief §3.3).
//
// `DirTabOptions=824, DirTabLimit=32` and histories full of tab markers: tabs
// are how the user works, which is why the brief lists them as required rather
// than deferred.
//
// A tab is **a place plus how you were looking at it** — the `PaneLocation`
// from `@/lib/paneHistory` (so a tab can live inside `Lotus.adf`, and does not
// need a second notion of "where"), plus the sort order and the filename mask,
// which are per-pane state today and become per-*tab* state the moment there
// is more than one tab. Session restore then falls out: the same object,
// written to the settings store and read back.
//
// Pure — no ids are minted here. The caller supplies one, because a module
// that reached for `Date.now()` or a counter of its own would be untestable in
// exactly the way this one needs to be testable.

import type { PaneLocation } from "@/lib/paneHistory";
import type { SortState } from "@/lib/sort";

export interface PaneTab {
  /** Stable for the life of the tab; React's key, and nothing else. */
  id: string;
  location: PaneLocation;
  sort: SortState;
  /** The pane's filename mask (`@/lib/mask`). */
  filter: string;
}

export interface TabSet {
  tabs: PaneTab[];
  /** Index into `tabs`. Always valid: a `TabSet` never has zero tabs. */
  active: number;
}

/** How many tabs one pane may hold — the user's own `DirTabLimit=32`. */
export const TAB_LIMIT = 32;

export function singleTabSet(tab: PaneTab): TabSet {
  return { tabs: [tab], active: 0 };
}

export function activeTab(set: TabSet): PaneTab {
  return set.tabs[set.active];
}

/**
 * Ctrl+T — duplicate the active tab and switch to the copy.
 *
 * Duplicating rather than opening a blank one is Total Commander's behaviour
 * and the more useful default: a new tab is nearly always wanted *near* where
 * you are — the same folder, to go two different ways from it.
 *
 * At the limit the set is returned unchanged. Refusing quietly is the right
 * failure here: thirty-two tabs is not a number anyone reaches by accident,
 * and an error dialog for it would be noise.
 */
export function duplicateTab(set: TabSet, id: string): TabSet {
  if (set.tabs.length >= TAB_LIMIT) return set;
  const copy: PaneTab = { ...activeTab(set), id };
  const tabs = [...set.tabs.slice(0, set.active + 1), copy, ...set.tabs.slice(set.active + 1)];
  return { tabs, active: set.active + 1 };
}

/**
 * Ctrl+W, or a middle-click — close a tab.
 *
 * **The last tab never closes.** A pane with no tabs is not a state this
 * model has, and giving it one would mean every reader of `activeTab` needing
 * a null check for a situation the UI should simply not allow.
 *
 * Closing the active tab lands on its neighbour — the one to the left, unless
 * it was the first, which is where the eye expects to go next.
 */
export function closeTab(set: TabSet, index: number): TabSet {
  if (set.tabs.length <= 1) return set;
  if (index < 0 || index >= set.tabs.length) return set;

  const tabs = set.tabs.filter((_, i) => i !== index);
  const active =
    index < set.active
      ? set.active - 1
      : index === set.active
        ? Math.max(0, index - 1)
        : set.active;
  return { tabs, active };
}

/** Ctrl+Tab — the next tab, wrapping. */
export function nextTab(set: TabSet): TabSet {
  if (set.tabs.length <= 1) return set;
  return { ...set, active: (set.active + 1) % set.tabs.length };
}

/** Click — go to a tab by index; out of range is ignored, not clamped. */
export function selectTab(set: TabSet, index: number): TabSet {
  if (index < 0 || index >= set.tabs.length || index === set.active) return set;
  return { ...set, active: index };
}

/**
 * Write the pane's current state back into the active tab.
 *
 * Called whenever the pane moves, sorts or filters, so the tab is always what
 * the pane actually shows — and therefore so is what gets persisted. Returns
 * the same object when nothing changed, so it can be called from an effect
 * without looping.
 */
export function updateActiveTab(set: TabSet, patch: Partial<Omit<PaneTab, "id">>): TabSet {
  const current = activeTab(set);
  const next: PaneTab = { ...current, ...patch };
  if (
    next.location === current.location &&
    next.sort === current.sort &&
    next.filter === current.filter
  ) {
    return set;
  }
  const tabs = set.tabs.map((tab, i) => (i === set.active ? next : tab));
  return { ...set, tabs };
}

/**
 * The title a tab shows: the last meaningful step of where it is.
 *
 * A container names the image — `Lotus.adf` — because that is what a tab
 * inside one *is* to the user, whatever directory it happens to be sitting in.
 * A folder names its last segment. Neither shows a full path: a tab bar is a
 * row of short labels or it is not a tab bar.
 */
export function tabTitle(tab: PaneTab): string {
  const path = tab.location.kind === "local" ? tab.location.path : tab.location.path;
  const segments = path.split(/[\\/]/).filter((part) => part !== "");
  return segments[segments.length - 1] ?? path;
}

/**
 * Whether a restored set is usable.
 *
 * Session restore reads whatever is in the settings store, which is a file a
 * user can edit and an older version of ART may have written. Anything that
 * would leave `activeTab` returning `undefined` is rejected outright and the
 * pane starts fresh — a commander that opens to a blank screen because its
 * settings file was half-written is worse than one that forgot your tabs.
 */
export function isUsableTabSet(value: unknown): value is TabSet {
  if (typeof value !== "object" || value === null) return false;
  const set = value as Partial<TabSet>;
  if (!Array.isArray(set.tabs) || set.tabs.length === 0) return false;
  if (typeof set.active !== "number" || set.active < 0 || set.active >= set.tabs.length) {
    return false;
  }
  return set.tabs.every(
    (tab) =>
      typeof tab === "object" &&
      tab !== null &&
      typeof tab.id === "string" &&
      typeof tab.filter === "string" &&
      typeof tab.location === "object" &&
      tab.location !== null &&
      typeof (tab.location as PaneLocation).path === "string"
  );
}
