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

/**
 * Below this many **`em` of its own listing text**, one pane is too narrow to
 * carry the full six-column row and `.tc-commander` drops Date and Attr
 * (ART-174).
 *
 * **Why `em` and not pixels, and why not a media query.** `.tc-row`'s columns
 * are `em` — `4.3em 10.7em 9.8em 5.3em 4.8em` beside a flexible Name — because
 * the listing's text size is a setting the user turns up (`PANE_FONT_*`
 * above). So the pixel width of a pane says nothing on its own: 500 px is
 * roomy at 10 px text and impossible at 28 px. And the pane lives inside
 * `.app-shell`, which carries `zoom`, so a `@media (max-width: …)` asks about
 * a viewport the layout is not laid out in — ART-101's defect, and this was
 * the second instance of it. Two independent zooms, one question, and a media
 * query can see neither of them.
 *
 * **Where the number comes from.** `python scripts/zoom-check.py --files`,
 * window 2575×1407. The rule this replaces fired at a 1000 px viewport, so the
 * probe was run at the Application Size that puts the shell at exactly
 * 1000 px (zoom 2.575) and asked the pane how wide it was:
 *
 * ```
 * z=2.575 font=10px pane=356px em=10 pane_in_em=35.6
 * z=2.575 font=12px pane=355px em=12 pane_in_em=29.58
 * z=2.575 font=16px pane=353px em=16 pane_in_em=22.06
 * ```
 *
 * 29.58 em is that measurement at the default 12 px text, and it is the
 * *last narrow* width rather than the first wide one, so the threshold is the
 * next tenth above it. The screen therefore degrades at exactly the point it
 * always did for a user who never touched either zoom — and now also degrades
 * when they turn the *text* up, which is the case the media query could not
 * see at all (at 200 % with 22 px text the pane is 22.36 em and the old rule
 * still called it wide).
 *
 * **A residual this does not fix**, recorded because the measurement exposed
 * it: the wide row's fixed columns total 34.9 em, so at 29.6 em they have
 * already stopped fitting — the agreed breakpoint is late, not early. Moving
 * it is a design change to when the screen degrades, not a change to which
 * viewport the question is asked of, so it is not made here.
 */
export const PANE_NARROW_BELOW_EM = 29.6;

/**
 * How wide `paneWidthPx` is in `em` of `paneFontPx` text — the only width that
 * means anything to a row whose columns are `em`.
 *
 * A non-positive or non-finite font size would make this meaningless rather
 * than merely wrong, so it clamps through `clampPaneFontSize` first.
 */
export function paneWidthInEm(paneWidthPx: number, paneFontPx: number): number {
  return paneWidthPx / clampPaneFontSize(paneFontPx);
}

/**
 * The width-dependent classes `.tc-commander` carries, as one space-joined
 * string (empty while the panes are wide enough).
 *
 * `paneWidthPx` is **measured**, not computed: it is the pane's own
 * `offsetWidth`, which resolves in the zoomed coordinate space the `em`
 * columns are resolved in. A width of 0 means nothing has been measured yet
 * (first render, before the observer fires) and is treated as "not narrow" so
 * the screen never flashes through its degraded layout on the way up.
 */
export function paneWidthClasses(paneWidthPx: number, paneFontPx: number): string {
  if (!(paneWidthPx > 0)) return "";
  return paneWidthInEm(paneWidthPx, paneFontPx) < PANE_NARROW_BELOW_EM
    ? "tc-commander-narrow"
    : "";
}
