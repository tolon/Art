// What the Files screen saves between runs, and how it is read back
// (brief §3.3 — his `Savepath=1`, `Savepanels=1`).
//
// Split out of `FileManager.tsx` for one reason: the round trip is the part of
// session restore that **cannot be true by construction**, and until this
// module existed it was also the part nothing tested. The tab model has its
// own tests (`paneTabs.test.ts`); the shape that actually reaches the settings
// store had none, which is exactly the gap the phase's acceptance walk had to
// record as unverified.
//
// The rule the whole module is built around: **what comes back off disk is
// data, not a promise.** `settings.json` is a file a user can edit, an older
// ART may have written, and a half-finished write may have truncated. So
// `readSession` validates rather than casts, and returns `null` for anything
// it cannot vouch for — a commander that opens to a blank screen because its
// settings file was odd is worse than one that forgot your tabs.

import { isUsableTabSet, type TabSet } from "@/lib/paneTabs";

export type PaneSide = "left" | "right";

export interface PaneSession {
  left: TabSet;
  right: TabSet;
  /** Which pane had the keyboard. */
  focused: PaneSide;
  /** The command line's own history, newest first. */
  commandHistory: string[];
}

/** How many command-line entries are kept — a history is useful only while it
 *  is short enough to scan. */
export const COMMAND_HISTORY_LIMIT = 20;

/** Build the object that goes into the settings store. */
export function toSession(
  left: TabSet,
  right: TabSet,
  focused: PaneSide,
  commandHistory: string[]
): PaneSession {
  return { left, right, focused, commandHistory: commandHistory.slice(0, COMMAND_HISTORY_LIMIT) };
}

/**
 * Read a session back, or `null` when there is nothing usable in it.
 *
 * The two halves are validated to different standards on purpose:
 *
 * - **The tabs are load-bearing.** Either both panes come back or the whole
 *   session is refused, because a restore that produced one restored pane and
 *   one empty one is a state the screen has no way to explain.
 * - **The command history is a convenience.** A malformed one is dropped on
 *   its own and the rest of the session still opens; losing a dropdown is not
 *   worth losing your tabs over.
 */
export function readSession(value: unknown): PaneSession | null {
  if (typeof value !== "object" || value === null) return null;

  const record = value as Record<string, unknown>;
  if (!isUsableTabSet(record.left) || !isUsableTabSet(record.right)) return null;

  const focused: PaneSide = record.focused === "right" ? "right" : "left";

  const history = record.commandHistory;
  const commandHistory =
    Array.isArray(history) && history.every((entry) => typeof entry === "string")
      ? (history as string[]).slice(0, COMMAND_HISTORY_LIMIT)
      : [];

  return { left: record.left, right: record.right, focused, commandHistory };
}

/**
 * The largest tab id already in use, so freshly minted ones cannot collide
 * with a tab that came back from disk.
 *
 * Ids are `tab-<n>` and mean nothing beyond being React keys, but a duplicate
 * key is a rendering bug that looks like a state bug, and the counter that
 * mints them starts at 1 on every launch.
 */
export function highestTabNumber(session: PaneSession): number {
  let highest = 0;
  for (const set of [session.left, session.right]) {
    for (const tab of set.tabs) {
      const numeric = Number(tab.id.replace(/^tab-/, ""));
      if (Number.isFinite(numeric)) highest = Math.max(highest, numeric);
    }
  }
  return highest;
}
