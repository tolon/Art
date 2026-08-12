// Type-to-search: the letters you type move the cursor to the next matching
// name (brief §3.2 — the user's `AltSearch=1`).
//
// It is the shortcut that makes a commander feel fast, and the one most likely
// to be got subtly wrong, so the whole of the *decision* lives here where a
// test can reach it and only the keystroke plumbing stays in the page.
//
// Two rules taken from Total Commander and worth stating, because both are
// what stop the feature becoming annoying:
//
// 1. **A letter that would match nothing is rejected**, and the prefix stays
//    as it was. Otherwise one typo empties the search and the cursor jumps
//    somewhere arbitrary the moment the next letter lands.
// 2. **The search starts at the cursor, not after it.** Typing `l`, `o`, `t`
//    should refine down the same row while it still matches, rather than
//    skipping to the *next* `Lotus` on every keystroke.
//
// Pure: no timers (the idle reset that ends a search is the hook's business),
// no i18n, no DOM.

/** What a keystroke did to the search. */
export interface QuickSearchStep {
  /** The prefix after this keystroke — unchanged when the letter was rejected. */
  prefix: string;
  /** The name the cursor should move to, or `null` to leave it alone. */
  match: string | null;
  /** False when the letter matched nothing and was rejected. */
  accepted: boolean;
}

/**
 * The character a key event contributes to a search, or `null` if it is not
 * one.
 *
 * Single printable characters only, and **never a space**: Space is the mark
 * key, and a search that swallowed it would take away the shortcut a user
 * reaches for far more often than this one. Any modifier disqualifies the
 * keystroke — those belong to the shortcuts that ask for them.
 */
export function searchCharacter(event: {
  key: string;
  ctrlKey: boolean;
  altKey: boolean;
  metaKey: boolean;
}): string | null {
  if (event.ctrlKey || event.altKey || event.metaKey) return null;
  if (event.key.length !== 1) return null;
  if (event.key === " ") return null;
  return event.key;
}

/**
 * The first name matching `prefix`, searching from `from` and wrapping once.
 *
 * Case-insensitive, and matched against the *start* of the name — Total
 * Commander's own rule, and the one that makes typing feel like it is aiming
 * rather than filtering. `null` when nothing matches, which is what makes
 * rule 1 above possible.
 */
export function findByPrefix(
  names: string[],
  prefix: string,
  from: string | null
): string | null {
  if (prefix === "" || names.length === 0) return null;

  const needle = prefix.toLowerCase();
  const start = from === null ? 0 : Math.max(0, names.indexOf(from));

  for (let step = 0; step < names.length; step++) {
    const name = names[(start + step) % names.length];
    if (name.toLowerCase().startsWith(needle)) return name;
  }
  return null;
}

/**
 * Extend a search by one typed character.
 *
 * Rejects the character rather than the search when nothing would match — see
 * rule 1. A rejected keystroke leaves both the prefix and the cursor exactly
 * where they were, so the user can simply type the letter they meant.
 */
export function extendSearch(
  names: string[],
  prefix: string,
  character: string,
  cursor: string | null
): QuickSearchStep {
  const candidate = prefix + character;
  const match = findByPrefix(names, candidate, cursor);
  if (match === null) return { prefix, match: null, accepted: false };
  return { prefix: candidate, match, accepted: true };
}

/**
 * Backspace during a search — shorten the prefix by one.
 *
 * The cursor moves to whatever the shorter prefix now finds, searched from the
 * top rather than from where the cursor happens to be: a shortening search is
 * widening, and starting from the current row would keep it stuck at the
 * narrow answer it already had.
 */
export function shortenSearch(names: string[], prefix: string): QuickSearchStep {
  const shorter = prefix.slice(0, -1);
  if (shorter === "") return { prefix: "", match: null, accepted: true };
  return { prefix: shorter, match: findByPrefix(names, shorter, null), accepted: true };
}
