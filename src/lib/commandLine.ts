// What typing into the commander's command line does (brief §1.4).
//
// Total Commander's command line runs programs. ART's does not, and that is a
// decision rather than an omission: §56 keeps ART away from executing whatever
// a user types, and a box that looks like a shell prompt and quietly is not
// would be worse than no box at all. So it does the two things it *can* do
// honestly — **it navigates and it filters** — and says so plainly for
// anything else, rather than swallowing the keystroke.
//
// Pure, like every other decision module in `src/lib`: no Tauri, no i18n
// singleton. A refusal carries a `Phrase` for the caller to render.

import type { Phrase } from "@/lib/phrase";

export type CommandLineAction =
  /** Nothing typed. */
  | { kind: "none" }
  /** Set the focused pane's filename mask (`@/lib/mask`). */
  | { kind: "filter"; mask: string }
  /** Up one level — `cd ..`, or a bare `..`. */
  | { kind: "up" }
  /** Open an absolute host path in the focused pane. */
  | { kind: "open"; path: string }
  /** Understood, and deliberately not done. */
  | { kind: "refused"; reason: Phrase };

/**
 * Whether `text` is an absolute path this can hand to `panel_list_local`.
 *
 * Deliberately generous about *shape* and silent about *existence*: a path
 * that does not exist is the pane's error to report, with the real reason from
 * the filesystem, not something to guess at here. What it will not accept is a
 * relative path — `Games` — because resolving one needs to know what the pane
 * is currently showing, and a pane can be showing the inside of an ADF, where
 * "join these two strings" is not what "up one, then Games" means.
 */
function isAbsolutePath(text: string): boolean {
  // `C:\…` or `C:/…`, a UNC share, or a Unix root.
  return /^[A-Za-z]:[\\/]/.test(text) || text.startsWith("\\\\") || text.startsWith("/");
}

/** `*` and `?` are `@/lib/mask`'s wildcards; either one makes this a filter. */
function looksLikeMask(text: string): boolean {
  return text.includes("*") || text.includes("?");
}

/**
 * Read one line of input as an action.
 *
 * The order is what makes it predictable: an explicit `cd` wins over
 * everything (so `cd *backup*` is a navigation attempt, not a filter), then
 * `..`, then a wildcard, then an absolute path. Anything left over is a
 * refusal that names what this line is for — never a silent no-op, which
 * would read as ART having crashed.
 */
export function parseCommandLine(input: string): CommandLineAction {
  const text = input.trim();
  if (text === "") return { kind: "none" };

  // `cd..` with no space is as common as `cd ..` in twenty years of muscle
  // memory, so both are the same thing here.
  const cd = /^cd\s*(.*)$/i.exec(text);
  if (cd) {
    const argument = cd[1].trim();
    if (argument === "" || argument === "..") return { kind: "up" };
    if (isAbsolutePath(argument)) return { kind: "open", path: argument };
    return { kind: "refused", reason: { key: "files.commandLine.refuseRelative" } };
  }

  if (text === "..") return { kind: "up" };
  if (looksLikeMask(text)) return { kind: "filter", mask: text };
  if (isAbsolutePath(text)) return { kind: "open", path: text };

  return { kind: "refused", reason: { key: "files.commandLine.refuseNotAShell" } };
}
