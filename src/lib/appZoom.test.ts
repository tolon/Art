// @vitest-environment jsdom
//
// Only `belongsToFileListing` needs a DOM, and it needs a real one: the rule it
// encodes is `closest`, and a hand-rolled stand-in would test the stand-in.

import { describe, expect, it } from "vitest";

import {
  belongsToFileListing,
  clampZoom,
  shellWidth,
  shellWidthClasses,
  SIDEBAR_ICONS_BELOW,
  stepZoom,
  zoomCssValue,
  zoomLabel,
  ZOOM_DEFAULT,
  ZOOM_MAX,
  ZOOM_MIN,
  ZOOM_STEP,
} from "@/lib/appZoom";

describe("clampZoom", () => {
  it("keeps a sensible size as it is", () => {
    expect(clampZoom(140)).toBe(140);
  });

  it("holds both ends", () => {
    expect(clampZoom(10)).toBe(ZOOM_MIN);
    expect(clampZoom(10_000)).toBe(ZOOM_MAX);
  });

  it("goes both ways from 100 %", () => {
    // An accessibility control that can only grow is not an accessibility
    // control: a user on a small laptop wanting more panes on screen is
    // asking the same kind of question.
    expect(ZOOM_MIN).toBeLessThan(ZOOM_DEFAULT);
    expect(ZOOM_MAX).toBeGreaterThan(ZOOM_DEFAULT);
  });

  it("gives a usable size for a number that is not one", () => {
    // What a hand-edited settings file, or a division by zero, produces.
    expect(clampZoom(NaN)).toBe(ZOOM_DEFAULT);
    expect(clampZoom(Infinity)).toBe(ZOOM_DEFAULT);
  });

  it("is always whole", () => {
    expect(clampZoom(112.4)).toBe(112);
  });
});

describe("stepZoom", () => {
  it("moves one step per turn of the wheel, whatever the wheel reports", () => {
    // A mouse reports -120 or -100 or -3 depending on the driver. One turn is
    // one step in every case, or the size runs away under the user's hand.
    expect(stepZoom(100, -120)).toBe(100 - ZOOM_STEP);
    expect(stepZoom(100, -1)).toBe(100 - ZOOM_STEP);
    expect(stepZoom(100, 240)).toBe(100 + ZOOM_STEP);
  });

  it("stops at the ends rather than wrapping", () => {
    expect(stepZoom(ZOOM_MAX, 1)).toBe(ZOOM_MAX);
    expect(stepZoom(ZOOM_MIN, -1)).toBe(ZOOM_MIN);
  });

  it("does nothing for a wheel that did not move", () => {
    expect(stepZoom(120, 0)).toBe(120);
  });

  it("lands back on the step grid", () => {
    // A 103 % that arrived from an older settings file should become 110, not
    // 113 — otherwise the odd three per cent is carried along forever and the
    // user can never get back to a round number.
    expect(stepZoom(103, 1)).toBe(110);
    expect(stepZoom(103, -1)).toBe(90);
  });
});

describe("zoomCssValue", () => {
  it("is the unitless ratio every Chromium accepts", () => {
    expect(zoomCssValue(100)).toBe("1");
    expect(zoomCssValue(150)).toBe("1.5");
  });

  it("never emits a value that would hide the application", () => {
    expect(Number(zoomCssValue(0))).toBeGreaterThan(0);
    expect(Number(zoomCssValue(NaN))).toBe(1);
  });
});

describe("zoomLabel", () => {
  it("reads as a percentage", () => {
    expect(zoomLabel(125)).toBe("125%");
  });
});

describe("belongsToFileListing", () => {
  it("says no when there is no element at all", () => {
    expect(belongsToFileListing(null)).toBe(false);
  });

  it("says yes inside the commander and no outside it", () => {
    // The commander has had its own Ctrl+wheel since phase 2b, meaning the
    // listing's text alone. The nearer gesture wins — the rule a browser
    // applies to a zoomable map inside a zoomable page.
    const outer = document.createElement("div");
    const commander = document.createElement("div");
    commander.className = "tc-commander";
    const row = document.createElement("span");
    commander.appendChild(row);
    outer.appendChild(commander);

    expect(belongsToFileListing(row)).toBe(true);
    expect(belongsToFileListing(outer)).toBe(false);
  });
});

/**
 * ART-101. The sidebar's collapse-to-icons breakpoint was a media query, and a
 * media query is evaluated against the real viewport while everything inside
 * `.app-shell` lays out in zoomed CSS pixels — so the rule the design already
 * agreed on could not fire at exactly the sizes it was written for.
 *
 * The numbers below are the ones measured while closing ART-099, on a real
 * 1258 px window: the sidebar is 224 real px at 100 %, 291 at 130 % and 448 at
 * 200 %, over a third of the glass, while the layout has 629 CSS px to work
 * with — well under the 1000 the design says it wants icons at.
 */
describe("shellWidth", () => {
  it("is the viewport at 100 %", () => {
    expect(shellWidth(1258, 100)).toBe(1258);
  });

  it("shrinks as the application is drawn bigger", () => {
    expect(shellWidth(1258, 200)).toBe(629);
    expect(shellWidth(1258, 130)).toBeCloseTo(967.7, 1);
  });

  it("grows below 100 %, because the setting goes both ways", () => {
    expect(shellWidth(1000, 70)).toBeCloseTo(1428.6, 1);
  });

  it("clamps an impossible zoom rather than dividing by it", () => {
    // A settings file edited by hand, or a 0 that would otherwise be Infinity.
    expect(Number.isFinite(shellWidth(1258, 0))).toBe(true);
    expect(Number.isFinite(shellWidth(1258, Number.NaN))).toBe(true);
  });
});

describe("shellWidthClasses", () => {
  it("leaves a wide shell alone", () => {
    expect(shellWidthClasses(1600, 100)).toBe("");
  });

  it("collapses the sidebar the moment the shell is under the threshold", () => {
    expect(shellWidthClasses(SIDEBAR_ICONS_BELOW + 1, 100)).toBe("");
    expect(shellWidthClasses(SIDEBAR_ICONS_BELOW - 1, 100)).toBe("app-shell-narrow");
  });

  it("fires on the measured window that used to be missed entirely", () => {
    // 1258 px, the window ART-099 was measured in. At 100 % the sidebar's
    // labels are affordable; at 130 % and 200 % they are not, and the old
    // media query said "no" at all three.
    expect(shellWidthClasses(1258, 100)).toBe("");
    expect(shellWidthClasses(1258, 130)).toBe("app-shell-narrow");
    expect(shellWidthClasses(1258, 200)).toContain("app-shell-narrow");
  });

  it("adds the tighter class only when the shell is tighter still", () => {
    // 1900 / 2 = 950: under the sidebar's 1000, over the grid's 760.
    expect(shellWidthClasses(1900, 200)).toBe("app-shell-narrow");
    // 1258 / 2 = 629, under both — which is what a maximised 1258 px window
    // at 200 % really is, and why the old media query missing this mattered.
    expect(shellWidthClasses(1258, 200)).toBe("app-shell-narrow app-shell-tight");
  });
});
