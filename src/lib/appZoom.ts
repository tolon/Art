// How big the whole application is drawn, as a percentage.
//
// **An accessibility setting, not a preference.** The user's own words about
// his own users, and about himself: most of the people this program is for are
// past fifty and wear reading glasses. `@/lib/dockLayout` already answered
// that for the file listing, but a listing is not the application — the Aminet
// search box, the studio cards, the icons, the labels on every button are all
// twelve-and-thirteen-pixel text on a 4K screen, and none of them could be
// made bigger. This is the same answer given once, for every screen.
//
// **Why a zoom rather than a font size.** The ask was fonts *and* search
// fields *and* icons — everything. ART's screens size their text in pixels
// inline, so a root `font-size` would move some of it and none of the rest,
// which is the worst of the two outcomes: a half-scaled screen looks broken in
// a way a small one does not. CSS `zoom` scales the whole box tree — text,
// padding, borders, inputs and emoji alike — and unlike `transform: scale` it
// participates in layout, so nothing overflows or leaves a gap. WebView2 is
// Chromium, where `zoom` is supported and standardised.
//
// Pure, so the rules can be pinned by a test rather than by squinting at a
// screenshot.

/**
 * The smallest and largest the application may be drawn.
 *
 * The floor is below 100 % on purpose: a user on a small laptop with a lot of
 * panes may want *more* on screen, and refusing that would make this a
 * one-directional accessibility control, which is not what an accessibility
 * control is. The ceiling is where a maximised ART on a 1080p screen still
 * fits its own two-pane layout without the panes becoming unusable.
 */
export const ZOOM_MIN = 70;
export const ZOOM_MAX = 250;

/** What it is when nobody has said otherwise: the size ART was designed at. */
export const ZOOM_DEFAULT = 100;

/**
 * One step of the wheel, or of Ctrl+plus.
 *
 * Ten per cent, because the useful range is wide and a user hunting for the
 * size they can read wants to get there in a few turns. Below about five per
 * cent a step is not visibly different from the last one, which reads as the
 * control being broken.
 */
export const ZOOM_STEP = 10;

/** Keep a zoom inside its bounds, and whole. */
export function clampZoom(percent: number): number {
  if (!Number.isFinite(percent)) return ZOOM_DEFAULT;
  return Math.round(Math.max(ZOOM_MIN, Math.min(ZOOM_MAX, percent)));
}

/**
 * The next size up or down from `current`.
 *
 * `step` is a direction, not an amount: `+1` or `-1`, so a mouse wheel that
 * reports `-120` still moves exactly one step. Rounded onto the step grid
 * first, so a zoom that arrived from a settings file at 103 % does not carry
 * that 3 % along forever.
 */
export function stepZoom(current: number, step: number): number {
  const direction = Math.sign(step);
  if (direction === 0) return clampZoom(current);
  const onGrid = Math.round(clampZoom(current) / ZOOM_STEP) * ZOOM_STEP;
  return clampZoom(onGrid + direction * ZOOM_STEP);
}

/**
 * The CSS value to hand `zoom`.
 *
 * A unitless ratio rather than a percentage string: `zoom: 1.2` is the form
 * every Chromium version has accepted, while `zoom: 120%` is newer.
 */
export function zoomCssValue(percent: number): string {
  return String(clampZoom(percent) / 100);
}

/** How the size is written where a person reads it. */
export function zoomLabel(percent: number): string {
  return `${clampZoom(percent)}%`;
}

/**
 * Whether this wheel event belongs to the file listing rather than to the
 * application.
 *
 * The commander has had its own Ctrl+wheel since phase 2b, and it means
 * something narrower: the size of the *listing's* text, independent of the
 * chrome around it. Both gestures are right, and the nearer one wins — the
 * same rule a browser applies to a zoomable map inside a zoomable page.
 *
 * Takes an `Element | null` rather than the event so it can be tested without
 * a DOM event; `closest` is all it needs.
 */
export function belongsToFileListing(target: Element | null): boolean {
  return target?.closest?.(".tc-commander") != null;
}

/**
 * The width, in the CSS pixels the shell actually lays out in, of a viewport
 * `viewportWidth` real pixels wide at `zoomPercent` (ART-101).
 *
 * `zoom` on `.app-shell` means every length inside it is divided by the zoom
 * ratio: a 1258 px window at 200 % gives the layout 629 px to work with. A
 * media query cannot see this — media queries are evaluated against the real
 * viewport, so `@media (max-width: 1000px)` asks a question about a window the
 * layout does not live in, and the answer is "no" exactly when the layout is
 * most cramped.
 */
export function shellWidth(viewportWidth: number, zoomPercent: number): number {
  return viewportWidth / (clampZoom(zoomPercent) / 100);
}

/**
 * Below this the sidebar's labels cost more than they give, and it collapses
 * to icons.
 *
 * The number is the one the design already agreed on in `layout.css`; only the
 * width it is compared against has changed.
 */
export const SIDEBAR_ICONS_BELOW = 1000;

/** Below this the quick-action grid drops to two columns. */
export const QUICK_ACTIONS_STACK_BELOW = 760;

/**
 * The width-dependent classes `.app-shell` carries, as one space-joined
 * string (empty when the shell is wide enough for neither).
 *
 * Classes rather than media queries because of the above. Named for what they
 * mean rather than for a pixel count, so the thresholds can move without
 * every rule that reads them having to.
 */
export function shellWidthClasses(viewportWidth: number, zoomPercent: number): string {
  const width = shellWidth(viewportWidth, zoomPercent);
  const classes: string[] = [];
  if (width < SIDEBAR_ICONS_BELOW) classes.push("app-shell-narrow");
  if (width < QUICK_ACTIONS_STACK_BELOW) classes.push("app-shell-tight");
  return classes.join(" ");
}
