// The per-pane filename filter (task 7): a Total Commander-style mask —
// `*` matches any run of characters, `?` matches exactly one — not a plain
// substring box, which is what a commander user expects from the `*.*` that
// sits in the reference's path row. Pure, like `src/lib/selection.ts` and
// `src/lib/sort.ts`: entries and a mask string in, a filtered list out, so
// it can be tested without rendering `FileManager.tsx` and carries no React,
// DOM or i18n import of its own.
//
// Filtering is display-only — `FileManager.tsx`'s own rule, stated in its
// filter-change handler: it changes what a pane *shows*, never what a
// selection actually names. This module only owns the matching itself.

import type { PanelEntry } from "@/lib/panel";

/**
 * Every regex metacharacter escaped to its literal meaning, `*` and `?`
 * included.
 *
 * Building the pattern by concatenating the mask straight into a `RegExp`
 * would let a mask like `a+b.txt` mean "one or more a, then b.txt" instead
 * of the literal filename, and a mask like `(a+)+$` is a catastrophic-
 * backtracking hazard. Escaping first — turning `*` into `\*` and `?` into
 * `\?` along with every other special character — and translating only
 * those two wildcards afterwards keeps every other character a plain
 * literal. The translated pattern never nests a quantifier (each wildcard
 * becomes a single, unnested `.*` or `.`), so it cannot backtrack
 * pathologically either.
 */
function escapeRegExp(text: string): string {
  return text.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}

/** `mask`, compiled to a case-insensitive, whole-name regular expression. */
function maskToRegex(mask: string): RegExp {
  const pattern = escapeRegExp(mask).replace(/\\\*/g, ".*").replace(/\\\?/g, ".");
  return new RegExp(`^${pattern}$`, "i");
}

/**
 * Whether `name` — the whole filename, extension included, even though the
 * pane splits the two into separate columns — matches `mask`.
 *
 * Case-insensitive, and an empty (or all-whitespace) mask matches
 * everything: that is what lets a cleared filter box show the whole
 * directory again rather than an empty pattern that matches nothing.
 */
export function matchesMask(name: string, mask: string): boolean {
  if (mask.trim() === "") return true;
  return maskToRegex(mask).test(name);
}

/**
 * `entries`, narrowed to the ones `mask` matches. Does not mutate its input,
 * and does not sort — `FileManager.tsx` runs this before `sortEntries` so
 * the visible list is filtered first and sorted second, through the one
 * comparator `@/lib/sort.ts` already owns rather than a second one grown
 * here.
 */
export function filterEntries(entries: PanelEntry[], mask: string): PanelEntry[] {
  if (mask.trim() === "") return entries;
  const regex = maskToRegex(mask);
  return entries.filter((entry) => regex.test(entry.name));
}
