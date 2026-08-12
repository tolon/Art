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
