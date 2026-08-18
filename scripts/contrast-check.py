#!/usr/bin/env python3
"""Measure every colour pair ART renders, in both themes, against WCAG.

ART-140 is why this exists. The light theme had been "widened" once by eye and
still put 2.20:1 text inside its own success badge — the level at which a
person with ordinary middle-aged eyesight reads nothing at all. Looking at a
screen and deciding it seems fine is exactly what produced that number, and it
is not a measurement.

So this reads `src/styles/theme.css` — the file itself, not a copy of its
values — works out every combination the stylesheet actually renders, and
prints the ratio. It fails when one drops below its threshold.

    python scripts/contrast-check.py            # table plus a verdict
    python scripts/contrast-check.py --quiet     # verdict only

## What it checks, and against which threshold

* **Body text on every surface** — 4.5:1, WCAG AA for text under 18.66px
  bold / 24px regular. ART's screens are 11-13px, so AA is the floor and not
  the target.
* **A status colour as text on a tint of itself** — the badge case, and the one
  that was broken. `.badge-ok` is `color-mix(in srgb, var(--ok) 22%,
  transparent)` behind `var(--ok-text)`, so the ground is the tint composited
  over whichever surface the badge sits on.
* **White on the primary button** — `--accent-fg` on `--accent`, 4.5:1.
* **A boundary that has to be seen to be used** — `--border-strong`, which
  form controls and the drop zone draw with, at 3:1 (WCAG 1.4.11 non-text).

## What it does not check

`--border` (panel edges) is decorative: a card is also separated from the page
by its fill, so the line is not carrying the information on its own. The
File Manager's `--tc-*` palette is Total Commander's, taken from the user's own
config and theme-aware in its own right — it is measured nowhere here because
changing it would be changing his colours, not ART's.

Nothing here needs a browser or a dev server, which is why it can run in CI
where `zoom-check.py` cannot.
"""

from __future__ import annotations

import argparse
import re
import sys
from pathlib import Path

THEME = Path(__file__).resolve().parent.parent / "src" / "styles" / "theme.css"

# The four surfaces a foreground can land on.
SURFACES = ["--bg", "--bg-elevated", "--bg-panel", "--bg-hover"]

# Foregrounds that carry words, and the ratio each must clear.
TEXT_ON_SURFACES = {
    "--text": 4.5,
    "--text-muted": 4.5,
    "--text-faint": 4.5,
    "--accent-text": 4.5,
    "--ok-text": 4.5,
    "--warn-text": 4.5,
    "--err-text": 4.5,
}

# A badge: `color-mix(in srgb, <mark> 22%, transparent)` behind <text>.
BADGES = [("--ok", "--ok-text"), ("--warn", "--warn-text"), ("--err", "--err-text")]
BADGE_TINT = 0.22

# `.btn-primary` and `.btn-warn`: a fill with a label on it.
FILLS = [("--accent-fg", "--accent", 4.5)]

# Boundaries that identify a control (WCAG 1.4.11).
BOUNDARIES = [("--border-strong", 3.0)]


def parse_theme(text: str) -> dict[str, dict[str, str]]:
    """The token table for each theme, read out of the stylesheet."""
    blocks = {}
    for selector, key in ((":root,", "dark"), (".theme-light {", "light")):
        at = text.index(selector)
        end = text.index("}", at)
        blocks[key] = dict(
            re.findall(r"(--[a-z-]+):\s*(#[0-9a-fA-F]{6})", text[at:end])
        )
    return blocks


def rgb(value: str) -> tuple[float, float, float]:
    return tuple(int(value[i : i + 2], 16) / 255 for i in (1, 3, 5))


def relative_luminance(colour: tuple[float, float, float]) -> float:
    def channel(c: float) -> float:
        return c / 12.92 if c <= 0.04045 else ((c + 0.055) / 1.055) ** 2.4

    r, g, b = (channel(c) for c in colour)
    return 0.2126 * r + 0.7152 * g + 0.0722 * b


def contrast(a: tuple[float, ...], b: tuple[float, ...]) -> float:
    la, lb = relative_luminance(a), relative_luminance(b)
    lighter, darker = max(la, lb), min(la, lb)
    return (lighter + 0.05) / (darker + 0.05)


def over(fg: tuple[float, ...], bg: tuple[float, ...], alpha: float) -> tuple[float, ...]:
    """`color-mix(in srgb, fg <alpha>%, transparent)` composited onto bg."""
    return tuple(fg[i] * alpha + bg[i] * (1 - alpha) for i in range(3))


def check(theme: str, tokens: dict[str, str]) -> list[tuple[str, float, float]]:
    """Every pair, as (description, measured, required)."""
    results: list[tuple[str, float, float]] = []

    for token, required in TEXT_ON_SURFACES.items():
        for surface in SURFACES:
            results.append(
                (
                    f"{token} on {surface}",
                    contrast(rgb(tokens[token]), rgb(tokens[surface])),
                    required,
                )
            )

    for mark, ink in BADGES:
        for surface in SURFACES:
            tint = over(rgb(tokens[mark]), rgb(tokens[surface]), BADGE_TINT)
            results.append(
                (
                    f"{ink} on {mark} badge over {surface}",
                    contrast(rgb(tokens[ink]), tint),
                    4.5,
                )
            )

    for label, fill, required in FILLS:
        results.append(
            (f"{label} on {fill}", contrast(rgb(tokens[label]), rgb(tokens[fill])), required)
        )

    for token, required in BOUNDARIES:
        for surface in SURFACES:
            results.append(
                (
                    f"{token} against {surface}",
                    contrast(rgb(tokens[token]), rgb(tokens[surface])),
                    required,
                )
            )

    return results


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--quiet", action="store_true", help="print the verdict only")
    args = parser.parse_args()

    themes = parse_theme(THEME.read_text(encoding="utf-8"))
    failures = 0
    for theme in ("dark", "light"):
        results = check(theme, themes[theme])
        if not args.quiet:
            print(f"\n=== {theme} theme")
        for description, measured, required in results:
            failed = measured < required
            failures += failed
            if failed or not args.quiet:
                mark = "FAIL" if failed else "ok  "
                print(f"  {mark} {measured:5.2f} (needs {required:.1f})  {description}")

    total = sum(len(check(t, themes[t])) for t in ("dark", "light"))
    if failures:
        print(f"\n{failures} of {total} pairs are below their threshold.")
        return 1
    print(f"\nAll {total} pairs clear their threshold, both themes.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
