// @vitest-environment jsdom
//
// Task 6 of card round 4: the drop position travels through the one global
// listener, converted as measured, so a partition row can be hit.
//
// The conversion is settled by `.superpowers/sdd/2026-09-17-card-round-4/
// experiment-drop-coordinates.md`: physical ÷ devicePixelRatio, and NOT a
// second division by `.app-shell`'s `zoom` (`--app-zoom`). That candidate
// scored 15/15 hits across three zoom levels; dividing by zoom as well
// (candidate c in the experiment) scored 7/15 — ART-101's mistake in a new
// place. The first `describe` below guards exactly that regression.

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { cardRowAt, cssPointOf } from "./dropTarget";

describe("cssPointOf: physical ÷ devicePixelRatio, and nothing else", () => {
  beforeEach(() => {
    // A non-1 shell zoom present in the DOM the whole time this suite runs —
    // if the implementation ever reads it and divides by it too (the
    // ART-101-shaped mistake the experiment measured at 7/15 hits), these
    // cases go red.
    document.documentElement.style.setProperty("--app-zoom", "2");
  });

  afterEach(() => {
    document.documentElement.style.removeProperty("--app-zoom");
  });

  it.each([
    [1, { x: 300, y: 450 }, { x: 300, y: 450 }],
    [1.5, { x: 300, y: 450 }, { x: 200, y: 300 }],
    [2, { x: 300, y: 450 }, { x: 150, y: 225 }],
  ])("divides a physical point by devicePixelRatio %s", (dpr, physical, expected) => {
    expect(cssPointOf(physical, dpr)).toEqual(expected);
  });

  it("defaults the ratio to window.devicePixelRatio", () => {
    const original = window.devicePixelRatio;
    Object.defineProperty(window, "devicePixelRatio", { value: 1.5, configurable: true });
    try {
      expect(cssPointOf({ x: 300, y: 450 })).toEqual({ x: 200, y: 300 });
    } finally {
      Object.defineProperty(window, "devicePixelRatio", {
        value: original,
        configurable: true,
      });
    }
  });
});

describe("cardRowAt: which data-card-row sits under a CSS point", () => {
  let container: HTMLDivElement;

  beforeEach(() => {
    container = document.createElement("div");
    container.innerHTML = `
      <div data-card-row="0"><span class="label">Row 0</span></div>
      <div data-card-row="1"><span class="label">Row 1</span></div>
      <div data-card-row="2"><span class="label">Row 2</span></div>
    `;
    document.body.appendChild(container);
  });

  afterEach(() => {
    container.remove();
    delete (document as { elementFromPoint?: unknown }).elementFromPoint;
    vi.restoreAllMocks();
  });

  it("answers the row index when the point lands on a row's own child", () => {
    // jsdom does not implement elementFromPoint at all (no layout engine),
    // so it is stubbed directly and only the hit test itself is under test
    // here.
    const label = container.querySelectorAll(".label")[1] as HTMLElement;
    const stub = vi.fn().mockReturnValue(label);
    document.elementFromPoint = stub;

    expect(cardRowAt({ x: 42, y: 17 })).toBe(1);
    expect(stub).toHaveBeenCalledWith(42, 17);
  });

  it("answers null when the point is outside every row", () => {
    document.elementFromPoint = vi.fn().mockReturnValue(document.body);

    expect(cardRowAt({ x: 999, y: 999 })).toBeNull();
  });

  it("answers null when elementFromPoint finds nothing", () => {
    document.elementFromPoint = vi.fn().mockReturnValue(null);

    expect(cardRowAt({ x: 0, y: 0 })).toBeNull();
  });
});
