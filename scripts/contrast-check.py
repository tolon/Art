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


# ---------------------------------------------------------------------------
# The islands: colours ART hard-codes outside the token system
# ---------------------------------------------------------------------------
#
# Three screens draw something that is deliberately **not** theme-coloured: a
# hex dump, a simulated Gotek display and a partition map. Each is a picture of
# a thing rather than ART's chrome, which is a fair reason to opt out of the
# tokens — and it also meant nothing measured them. The pass above reads
# `theme.css` and cannot see a colour written into a component.
#
# So they are declared here, each foreground with the ground it actually lands
# on. Two rules make the declaration honest:
#
# 1. **Every hex literal in these files must appear below.** A colour added or
#    changed in the source and not declared here fails the check, with the
#    value printed. Otherwise this table would quietly drift out of date and
#    read as coverage it no longer has.
# 2. **A foreground is paired with the one ground it appears on.** Pairing
#    every colour with every background is how this check first reported two
#    failures that did not exist: `#ff3b30` is the seven-segment readout on
#    `#0d1117`, and never appears on the LCD's green at all.
#
# Not scanned, with the reason: `colourRules.ts` and `Settings.tsx` carry the
# File Manager's *user-editable* colours, over a ground that is also the user's
# — the same reason the `--tc-*` palette is excluded above; `TcIcon.tsx` is a
# stroke inside an icon; `ContentLayout.tsx`'s is a fallback for a token that
# always exists; and `AdfBrowser.tsx`'s one colour is inside `.hex-view`, so it
# is measured there.

ROOT = Path(__file__).resolve().parent.parent

ISLANDS = [
    {
        "file": "src/styles/global.css",
        "what": ".hex-view",
        "ground": "#0d1117",
        "text": ["#c9d1d9", "#79c0ff", "#e6edf3", "#7ee787"],
        "decorative": [],
    },
    {
        "file": "src/pages/HexTools.tsx",
        "what": "the hex dump",
        "ground": "#0d1117",
        "text": ["#c9d1d9", "#8b949e", "#79c0ff", "#7ee787"],
        "decorative": ["#21262d"],
    },
    {
        "file": "src/pages/GotekStudio.tsx",
        "what": "the simulated Gotek display (7-segment and OLED)",
        "ground": "#0d1117",
        # Held to 4.5 even though the seven-segment readout is 48px bold and
        # WCAG would allow 3.0 for it. The people this program is for are over
        # fifty, which is the reason the whole item exists.
        "text": ["#ff3b30", "#58a6ff", "#8b949e"],
        "decorative": ["#30363d", "#21262d"],
    },
    {
        "file": "src/pages/GotekStudio.tsx",
        "what": "the simulated Gotek display (16x2 LCD)",
        "ground": "#1e3a1e",
        "text": ["#7ee787"],
        "decorative": [],
    },
]

# The partition map is its own shape: five fills, each carrying a label, and a
# two-ring selection indicator that has to be visible on any of them.
PARTITION_MAP = {
    "file": "src/pages/HardDiskStudio.tsx",
    "map_background": "#161b22",
    "fills": ["#388bfd", "#3fb950", "#d29922", "#a371f7", "#f85149"],
    "label": "#000000",
    # Inner and outer ring. White alone measured 2.52:1 on the amber fill.
    "rings": ["#000000", "#ffffff"],
    # And the shape, required verbatim in the source.
    #
    # The tables above are a copy of the file's values, which the undeclared-
    # colour rule keeps honest for the *set* of colours but not for how they
    # are arranged. Reverting this to a single white ring changes neither set:
    # black stays in the file as the label colour, white stays as the ring, and
    # the measurement would go on reporting a black ring that is no longer
    # drawn. Caught by mutation, 2026-08-24, and fixed by asking the source for
    # the arrangement rather than only for the colours.
    "ring_source": "inset 0 0 0 2px #000000, inset 0 0 0 4px #ffffff",
}


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


def expand(value: str) -> str:
    """`#abc` and `#abc` alike, as six digits."""
    digits = value.lstrip("#")
    if len(digits) == 3:
        digits = "".join(c * 2 for c in digits)
    return "#" + digits.lower()


def hexes_in(path: Path) -> set[str]:
    """Every colour literal in a file, six digits, lowercase."""
    text = path.read_text(encoding="utf-8")
    return {
        expand(m) for m in re.findall(r"#[0-9a-fA-F]{6}\b|#[0-9a-fA-F]{3}\b", text)
    }


def check_islands() -> tuple[list[tuple[str, float, float]], list[str]]:
    """Measure the hard-coded pairs, and report anything undeclared."""
    results: list[tuple[str, float, float]] = []
    problems: list[str] = []
    declared: dict[str, set[str]] = {}

    def declare(file: str, *values: str) -> None:
        declared.setdefault(file, set()).update(expand(v) for v in values)

    for island in ISLANDS:
        ground = island["ground"]
        declare(island["file"], ground, *island["text"], *island["decorative"])
        for ink in island["text"]:
            results.append(
                (
                    f"{ink} on {ground} ({island['what']})",
                    contrast(rgb(expand(ink)), rgb(expand(ground))),
                    4.5,
                )
            )

    fills = PARTITION_MAP["fills"]
    label = PARTITION_MAP["label"]
    inner, outer = PARTITION_MAP["rings"]
    declare(
        PARTITION_MAP["file"],
        PARTITION_MAP["map_background"],
        label,
        *fills,
        *PARTITION_MAP["rings"],
    )
    for fill in fills:
        results.append(
            (
                f"{label} label on {fill} partition",
                contrast(rgb(expand(label)), rgb(expand(fill))),
                4.5,
            )
        )
        # WCAG 1.4.11: an indicator that is not text needs 3:1 against what is
        # next to it. The inner ring is surrounded by the fill.
        results.append(
            (
                f"selection ring {inner} against {fill} partition",
                contrast(rgb(expand(inner)), rgb(expand(fill))),
                3.0,
            )
        )
    results.append(
        (
            f"selection ring {outer} against {inner}",
            contrast(rgb(expand(outer)), rgb(expand(inner))),
            3.0,
        )
    )

    # The arrangement, not just the colours. See PARTITION_MAP["ring_source"].
    map_source = (ROOT / PARTITION_MAP["file"]).read_text(encoding="utf-8")
    if PARTITION_MAP["ring_source"] not in map_source:
        problems.append(
            f"{PARTITION_MAP['file']} no longer draws the two-ring selection "
            f"indicator ({PARTITION_MAP['ring_source']}) - a single ring is "
            "below 3:1 on the green and amber fills"
        )

    # Rule 1: nothing in these files may be undeclared.
    for file, known in declared.items():
        found = hexes_in(ROOT / file)
        for colour in sorted(found - known):
            problems.append(
                f"{file} uses {colour}, which is not declared in ISLANDS - "
                "say what it sits on, or give it a reason"
            )
        for colour in sorted(known - found):
            problems.append(
                f"{file} no longer uses {colour}, which ISLANDS still declares"
            )

    return results, problems


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

    # The islands are theme-independent by construction — that is what makes
    # them islands — so they are measured once rather than per theme.
    island_results, problems = check_islands()
    if not args.quiet:
        print("\n=== hard-coded, outside the token system")
    for description, measured, required in island_results:
        failed = measured < required
        failures += failed
        if failed or not args.quiet:
            mark = "FAIL" if failed else "ok  "
            print(f"  {mark} {measured:5.2f} (needs {required:.1f})  {description}")
    total += len(island_results)

    for problem in problems:
        print(f"  FAIL  {problem}")

    if failures or problems:
        if failures:
            print(f"\n{failures} of {total} pairs are below their threshold.")
        if problems:
            print(f"{len(problems)} problem(s) in the hard-coded files.")
        return 1
    print(f"\nAll {total} pairs clear their threshold, both themes.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
