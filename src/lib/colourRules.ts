// Per-filetype colour rules (brief Part 2, last bullet).
//
// The user runs eighteen `ColorFilters` in his own Total Commander, which is
// the strongest possible statement that a listing coloured by *kind* is how he
// reads a directory. ART ships its own defaults for the kinds it actually
// knows about — the containers it can walk into, the archives it can open, the
// ROMs it can identify — and lets the list be edited, TC-shaped: a filename
// mask on the left, a colour on the right, first match wins.
//
// Two things this deliberately is not:
//
// - **Not a colour per extension.** The defaults are three rules, not thirty:
//   what a row *is* to ART is the useful distinction, and a rainbow is harder
//   to read than four colours, not easier.
// - **Not a replacement for the built-in classification.** A row that no rule
//   matches keeps the colour `classifyEntry` (`TcIcon.tsx`) already gave it —
//   text files light, hidden files dimmed. The rules sit *in front* of that,
//   so an empty rule list changes nothing at all.
//
// Pure: masks are matched with the same `@/lib/mask` the filter box uses, so a
// user who has learned one syntax has learned both.

import { matchesMask } from "@/lib/mask";

export interface ColourRule {
  /**
   * One or more filename masks, separated by `;` — Total Commander's own
   * convention, and the reason a rule is not simply one mask: "every
   * container" is a list, and splitting it across five rules would make it
   * five things to reorder rather than one.
   */
  patterns: string;
  /** A CSS colour. Stored as written, so a user may put any valid colour in. */
  colour: string;
  /** What this rule is for, shown in Settings. Free text, the user's own. */
  label: string;
}

/**
 * ART's own defaults, in matching order.
 *
 * Chosen to answer the question a directory of Amiga files actually poses —
 * *which of these can I walk into, which can I unpack, which is a ROM* — since
 * those are exactly the three things ART does with a file and therefore the
 * three worth telling apart at a glance.
 */
export const DEFAULT_COLOUR_RULES: ColourRule[] = [
  {
    label: "Containers",
    patterns: "*.adf;*.adz;*.dms;*.hdf;*.hda;*.iso;*.d64;*.d71;*.d81;*.t64",
    colour: "#7fd7ff",
  },
  { label: "Archives", patterns: "*.lha;*.lzh;*.lzx;*.zip;*.7z", colour: "#c9a0ff" },
  { label: "ROMs", patterns: "*.rom;*.key", colour: "#ffc46b" },
];

/**
 * Darken a `#rrggbb` toward black by `amount` (0–1).
 *
 * Exists because a rule stores **one** colour and ART has **two** themes. The
 * shipped defaults are chosen to sing against the dark pane the user actually
 * runs; the same values on white are pale to the point of unreadable, which is
 * exactly what the first look at the light theme showed.
 *
 * Storing two colours per rule would double the Settings UI for a case most
 * users never see, and darkening in CSS is not available: the colour is applied
 * inline per row, so there is no rule for `color-mix` to reach. Doing the mix
 * here keeps one colour per rule and makes both themes legible.
 *
 * A colour that is not a plain 6-digit hex — a name, `rgb()`, anything a user
 * typed — is returned untouched rather than mangled.
 */
export function darken(colour: string, amount: number): string {
  const match = /^#([0-9a-f]{6})$/i.exec(colour.trim());
  if (!match) return colour;

  const value = parseInt(match[1], 16);
  const scale = Math.max(0, Math.min(1, 1 - amount));
  const channel = (shift: number) =>
    Math.round(((value >> shift) & 0xff) * scale)
      .toString(16)
      .padStart(2, "0");
  return `#${channel(16)}${channel(8)}${channel(0)}`;
}

/**
 * The colour for a name, or `null` when no rule claims it.
 *
 * **First match wins**, which is what makes the order in Settings meaningful:
 * a user who wants one `.adf` picked out from the rest puts that rule above
 * the container rule, exactly as they would in Total Commander.
 *
 * Directories are never coloured by a rule. They are chrome — `[Name]`, always
 * first, always the pane's own text colour — and a rule that caught one would
 * be a rule that made the folders-first ordering harder to see, not easier.
 */
export function colourFor(
  name: string,
  isDir: boolean,
  rules: ColourRule[],
  theme: "dark" | "light" = "dark"
): string | null {
  if (isDir) return null;

  for (const rule of rules) {
    if (rule.colour.trim() === "") continue;
    for (const pattern of rule.patterns.split(";")) {
      const mask = pattern.trim();
      if (mask !== "" && matchesMask(name, mask)) {
        // 45% toward black on a light pane. Enough to bring a colour picked
        // for a near-black background up to a readable contrast on paper,
        // without turning every rule into the same muddy dark grey.
        return theme === "light" ? darken(rule.colour, 0.45) : rule.colour;
      }
    }
  }
  return null;
}

/**
 * Whether a stored rule list is usable.
 *
 * Same reasoning as `isUsableTabSet`: this comes back from a JSON file a user
 * can edit. A malformed list falls back to the defaults rather than colouring
 * nothing, because "my colours disappeared" reads as a bug and "my colours are
 * the stock ones again" reads as what it is.
 */
export function isUsableRuleList(value: unknown): value is ColourRule[] {
  return (
    Array.isArray(value) &&
    value.every(
      (rule) =>
        typeof rule === "object" &&
        rule !== null &&
        typeof (rule as ColourRule).patterns === "string" &&
        typeof (rule as ColourRule).colour === "string" &&
        typeof (rule as ColourRule).label === "string"
    )
  );
}
