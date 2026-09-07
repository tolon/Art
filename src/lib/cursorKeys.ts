// The commander's cursor movement (ART-275): Up/Down/Home/End/PageUp/PageDown,
// Total Commander's own rules. Pure — no DOM, no React, no selection — so
// `useCursorKeys` (`@/components/files/FunctionKeys.tsx`) only owns the
// keyboard wiring and `FileManager.tsx` only owns what a move *does* to the
// selection (see `markThrough` in `@/lib/selection`).
//
// The rules, all of them Total Commander's:
//
// - **With no cursor** (a pane just opened, or the anchor names an entry the
//   directory no longer has), `down`/`home`/`pageDown` land on the first row
//   and `up`/`end`/`pageUp` land on the last — the direction each key would
//   have travelled from "before the top" or "past the bottom".
// - **Up at the first row stays; down at the last stays.** Neither wraps —
//   a cursor that wrapped around the end of the list would be a surprise,
//   not a shortcut.
// - **PageUp/PageDown move `pageRows` rows, clamped to the ends** exactly the
//   same way a single step is.
// - **An empty list returns `null`** — there is nowhere for a cursor to be.

export type CursorMove = "up" | "down" | "home" | "end" | "pageUp" | "pageDown";

/**
 * Where the cursor lands after `move`, or `null` when the pane has nothing to
 * put it on.
 *
 * `cursor` may name an entry `names` no longer has (a stale anchor, left over
 * from before the directory changed) — treated exactly like "no cursor",
 * rather than indexing to -1 and stepping from there.
 */
export function cursorStep(
  names: string[],
  cursor: string | null,
  move: CursorMove,
  pageRows: number
): string | null {
  if (names.length === 0) return null;

  const last = names.length - 1;
  const rawIndex = cursor === null ? -1 : names.indexOf(cursor);
  const index = rawIndex === -1 ? null : rawIndex;

  switch (move) {
    case "home":
      return names[0];
    case "end":
      return names[last];
    case "down":
      return names[index === null ? 0 : Math.min(index + 1, last)];
    case "up":
      return names[index === null ? last : Math.max(index - 1, 0)];
    case "pageDown":
      return names[index === null ? 0 : Math.min(index + pageRows, last)];
    case "pageUp":
      return names[index === null ? last : Math.max(index - pageRows, 0)];
    default:
      return cursor;
  }
}
