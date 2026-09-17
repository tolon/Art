// Converts a Tauri drop's physical screen position into a CSS point, and
// hit-tests that point against the one attribute a card row carries.
//
// This is the second half of the one global drop listener (`dnd.ts`,
// `Layout.tsx`): the listener still owns the only subscription to Tauri's
// drag/drop events, and a row never subscribes to anything — it only carries
// `data-card-row="<index>"`, and `cardRowAt` is the one place that reads it.
//
// The conversion is measured, not assumed
// (`.superpowers/sdd/2026-09-17-card-round-4/experiment-drop-coordinates.md`):
// **physical ÷ devicePixelRatio, and nothing else** — in particular, no
// second division by `.app-shell`'s CSS `zoom` (`--app-zoom`,
// `layout.css`). `getBoundingClientRect()`/`elementFromPoint()` already
// report the zoomed layout box in real viewport pixels, so dividing by zoom
// a second time undershoots the farther a drop lands from the window's
// top-left corner — the same shape of mistake as ART-101, repeated in a new
// place. The experiment measured 15/15 hits (3 zoom levels × 5 rows) for
// physical ÷ devicePixelRatio alone, against 7/15 for physical ÷
// (devicePixelRatio × zoom).

/** A point in CSS pixels — what `getBoundingClientRect()` and
 * `document.elementFromPoint()` both work in. */
export interface CssPoint {
  x: number;
  y: number;
}

/**
 * The measured conversion (see module doc above): `physical ÷
 * devicePixelRatio`, and nothing else.
 */
export function cssPointOf(
  position: { x: number; y: number },
  dpr: number = window.devicePixelRatio
): CssPoint {
  return { x: position.x / dpr, y: position.y / dpr };
}

/**
 * The `data-card-row` index under a CSS point, or `null` when the point is
 * outside every row. Rows never subscribe to the drop listener themselves;
 * this is the one hit test that reads their attribute.
 */
export function cardRowAt(point: CssPoint): number | null {
  const element = document.elementFromPoint(point.x, point.y);
  const row = element?.closest("[data-card-row]");
  if (!row) return null;
  const index = Number(row.getAttribute("data-card-row"));
  return Number.isInteger(index) ? index : null;
}
