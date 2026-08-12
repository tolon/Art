// How big the commander's parts are: the command-line row, and the listing's
// own text.
//
// **Why the listing's text size is a setting and not a constant.** Most of the
// people this program is for are past fifty and wear reading glasses — the
// user's own words about his own users, and about himself. A file manager is
// a wall of small text by nature, and twelve pixels on a 4K screen is a wall
// they cannot read. This is not a preference like a colour scheme; for a good
// share of the audience it is whether the program is usable at all.
//
// Two numbers that have to agree, which is why they live together. Dragging
// the row taller and leaving the text at eleven pixels gives a large empty box
// with a tiny line floating in it — which is exactly what the first version
// did, and exactly what a user who has just asked for "bigger" does not mean.
// **Making the area bigger means making what is in it bigger.**
//
// Pure, so both rules can be pinned by a test rather than by squinting at a
// screenshot.

/** One line at the default 11px, chrome padding included. */
export const DOCK_MIN_HEIGHT = 26;

/**
 * A third of a small laptop screen.
 *
 * A ceiling because the panes are what this screen is *for*: a dock that can
 * eat them is a dock that eventually will, usually by accident, usually while
 * dragging something else.
 */
export const DOCK_MAX_HEIGHT = 360;

/** Keep a dragged height inside its bounds, and whole. */
export function clampDockHeight(px: number): number {
  return Math.round(Math.max(DOCK_MIN_HEIGHT, Math.min(DOCK_MAX_HEIGHT, px)));
}

/**
 * The text size for a command-line row of this height.
 *
 * Proportional, so the row always looks deliberate rather than padded, with
 * both ends held:
 *
 * - **The floor is the chrome's own 11px.** The command line is not more
 *   important than the pane headers around it, and shrinking below them would
 *   make the one row a user typed into the hardest one to read.
 * - **The ceiling is 30px**, which is about where a single line stops being a
 *   command line and starts being a banner. Past it the extra height is space
 *   around the text — which is what someone dragging to 300px is asking for
 *   anyway.
 */
export function commandLineFontSize(height: number): number {
  // 0.42 rather than a rounder number for one reason worth keeping: it puts
  // the *default* row (26px) at exactly the 11px the chrome around it uses, so
  // the untouched screen looks as it always did and only a deliberate drag
  // changes anything.
  return Math.round(Math.max(11, Math.min(30, height * 0.42)));
}

/** The smallest and largest the listing's text may be, in pixels. */
export const PANE_FONT_MIN = 10;
export const PANE_FONT_MAX = 28;

/** What it is when nobody has said otherwise — Total Commander's own density. */
export const PANE_FONT_DEFAULT = 12;

/** Keep a chosen listing text size inside its bounds, and whole. */
export function clampPaneFontSize(px: number): number {
  return Math.round(Math.max(PANE_FONT_MIN, Math.min(PANE_FONT_MAX, px)));
}

/**
 * The next size up or down from `current`.
 *
 * One pixel a step, because the useful range is small and a user hunting for
 * the size they can read wants to arrive at it, not overshoot it. `step` is
 * `+1` or `-1`; anything else is treated as its sign, so a mouse wheel that
 * reports `-120` still moves one step.
 */
export function stepPaneFontSize(current: number, step: number): number {
  return clampPaneFontSize(current + Math.sign(step));
}
