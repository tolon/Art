import { describe, expect, it } from "vitest";

import {
  clampDockHeight,
  clampPaneFontSize,
  commandLineFontSize,
  DOCK_MAX_HEIGHT,
  DOCK_MIN_HEIGHT,
  PANE_FONT_DEFAULT,
  PANE_FONT_MAX,
  PANE_FONT_MIN,
  PANE_NARROW_BELOW_EM,
  paneWidthClasses,
  paneWidthInEm,
  stepPaneFontSize,
} from "@/lib/dockLayout";

describe("clampDockHeight", () => {
  it("keeps a dragged height inside its bounds", () => {
    expect(clampDockHeight(120)).toBe(120);
    expect(clampDockHeight(-500)).toBe(DOCK_MIN_HEIGHT);
    expect(clampDockHeight(10_000)).toBe(DOCK_MAX_HEIGHT);
  });

  it("never returns a fraction of a pixel", () => {
    expect(clampDockHeight(120.6)).toBe(121);
  });

  it("has a floor, because a row dragged to nothing cannot be found again", () => {
    expect(clampDockHeight(0)).toBe(DOCK_MIN_HEIGHT);
    expect(DOCK_MIN_HEIGHT).toBeGreaterThan(0);
  });
});

describe("commandLineFontSize", () => {
  it("grows the text with the row", () => {
    // The whole point: making the area bigger means making what is in it
    // bigger. A tall box with an eleven-pixel line floating in it is what the
    // first version did, and is not what "bigger" means.
    expect(commandLineFontSize(60)).toBeGreaterThan(commandLineFontSize(26));
    expect(commandLineFontSize(120)).toBeGreaterThan(commandLineFontSize(60));
  });

  it("never shrinks below the chrome around it", () => {
    // The command line is not less important than the pane headers; making the
    // one row a user types into the hardest to read would be backwards.
    expect(commandLineFontSize(DOCK_MIN_HEIGHT)).toBe(11);
    expect(commandLineFontSize(0)).toBe(11);
  });

  it("stops growing where a line stops being a command line", () => {
    expect(commandLineFontSize(DOCK_MAX_HEIGHT)).toBe(30);
    expect(commandLineFontSize(10_000)).toBe(30);
  });

  it("is a whole number of pixels at every height", () => {
    for (let height = DOCK_MIN_HEIGHT; height <= DOCK_MAX_HEIGHT; height += 7) {
      expect(Number.isInteger(commandLineFontSize(height)), String(height)).toBe(true);
    }
  });
});

describe("the listing's text size", () => {
  it("stays inside bounds a body can actually read", () => {
    expect(clampPaneFontSize(16)).toBe(16);
    expect(clampPaneFontSize(0)).toBe(PANE_FONT_MIN);
    expect(clampPaneFontSize(999)).toBe(PANE_FONT_MAX);
    expect(clampPaneFontSize(16.4)).toBe(16);
  });

  it("goes high enough to matter to somebody wearing reading glasses", () => {
    // The point of the whole setting: most of this program's users are past
    // fifty, and twelve pixels on a 4K screen is a wall of text they cannot
    // read. A ceiling that only reached 14 would be a gesture, not a fix.
    expect(PANE_FONT_MAX).toBeGreaterThanOrEqual(24);
    expect(PANE_FONT_MAX / PANE_FONT_DEFAULT).toBeGreaterThan(2);
  });

  it("steps one pixel at a time, whatever the wheel reports", () => {
    // A user hunting for the size they can read wants to arrive at it, not
    // overshoot it — and a wheel event says -120, not -1.
    expect(stepPaneFontSize(12, 1)).toBe(13);
    expect(stepPaneFontSize(12, -1)).toBe(11);
    expect(stepPaneFontSize(12, -120)).toBe(11);
    expect(stepPaneFontSize(12, 240)).toBe(13);
  });

  it("does not run off either end", () => {
    expect(stepPaneFontSize(PANE_FONT_MIN, -1)).toBe(PANE_FONT_MIN);
    expect(stepPaneFontSize(PANE_FONT_MAX, 1)).toBe(PANE_FONT_MAX);
  });
});

describe("how narrow is a pane, really", () => {
  // Every figure below is from `python scripts/zoom-check.py --files` at the
  // 2575x1407 window that script already uses. Nothing here is invented; the
  // probe printed `pane_in_em` and these are those rows.

  it("measures a pane in em of its own text, not in pixels", () => {
    // The same 355 px pane is roomy at 10 px text and cramped at 16 px, which
    // is the whole reason a pixel breakpoint could not answer this.
    expect(paneWidthInEm(356, 10)).toBeCloseTo(35.6, 1);
    expect(paneWidthInEm(355, 12)).toBeCloseTo(29.58, 1);
    expect(paneWidthInEm(353, 16)).toBeCloseTo(22.06, 1);
  });

  it("degrades at exactly the point the media query it replaces did", () => {
    // Measured: shell width 1000 px (zoom 2.575 of a 2575 px window) puts one
    // pane at 355 px with the default 12 px listing text. That was the old
    // `@media (max-width: 1000px)`'s firing point, and it is still the firing
    // point for a user who has touched neither zoom.
    expect(paneWidthClasses(355, 12)).toBe("tc-commander-narrow");
    // One notch wider and it is not narrow. 356 px at 12 px text is 29.67 em,
    // which is above the threshold: the boundary is where it was measured,
    // not a round number chosen near it.
    expect(paneWidthClasses(356, 12)).toBe("");
  });

  it("leaves a wide window alone at every text size the wheel can reach", () => {
    // z=1, window 2575: pane 1132..1124 px across 10..28 px text. None of
    // these is narrow, and none of them was under the old rule either.
    expect(paneWidthClasses(1132, 10)).toBe("");
    expect(paneWidthClasses(1131, 12)).toBe("");
    expect(paneWidthClasses(1124, 28)).toBe("");
  });

  it("sees the case the media query was blind to: the text grew, not the window", () => {
    // z=2, window 2575, so the real viewport is 2575 px and the old
    // `max-width: 1000px` said "wide" at every text size. Measured panes:
    // 497 px at 10 px text (49.7 em, genuinely wide) and 492 px at 22 px text
    // (22.36 em, not remotely). Same window, same pixels, opposite answers.
    expect(paneWidthClasses(497, 10)).toBe("");
    expect(paneWidthClasses(492, 22)).toBe("tc-commander-narrow");
    expect(paneWidthClasses(489, 28)).toBe("tc-commander-narrow");
  });

  it("says nothing before anything has been measured", () => {
    // The observer has not fired on the first render. Reporting "narrow" for
    // a width of zero would flash the degraded layout on every mount.
    expect(paneWidthClasses(0, 12)).toBe("");
    expect(paneWidthClasses(-1, 12)).toBe("");
  });

  it("does not divide by a font size the settings file could not produce", () => {
    // A hand-edited `settings.json` can carry 0 here; `clampPaneFontSize`
    // is what keeps this from returning Infinity and calling every pane wide.
    expect(Number.isFinite(paneWidthInEm(355, 0))).toBe(true);
    expect(paneWidthInEm(355, 0)).toBe(355 / PANE_FONT_MIN);
    expect(PANE_NARROW_BELOW_EM).toBeGreaterThan(0);
  });
});
