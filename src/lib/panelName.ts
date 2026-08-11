// Splitting a panel row's name into a Total Commander-style "stem" and
// "extension" (task 6b): TC shows the extension as its own column, taken
// from the *last* dot in the name — "archive.tar.gz" splits into
// "archive.tar" / "gz", not "archive" / "tar.gz". Pure and tested on its own
// for the same reason `src/lib/sort.ts` and `src/lib/selection.ts` are: no
// Tauri call anywhere near it, and the edge cases (a dotfile, a trailing dot)
// are exactly the kind of thing worth pinning down once rather than
// re-deriving inline wherever a row renders.

export interface SplitName {
  /** The name with its extension removed, or the whole name when there is none. */
  stem: string;
  /** Without the dot. Empty when the name has no extension. */
  ext: string;
}

/**
 * Split `name` the way Total Commander's Name/Ext columns do.
 *
 * A directory never gets one — `isDir` short-circuits before any of the dot
 * logic runs, so `[Amiga PiStrom]` never grows a spurious ".PiStrom" column
 * even though its raw name has a dot in it.
 *
 * For a file, the split is on the *last* dot, with two cases folded back into
 * "no extension" rather than producing an empty or misleading one:
 *
 *   - the name has no dot at all ("README") — the whole thing is the stem;
 *   - the only dot is the first character (".gitignore") — a leading dot is
 *     not an extension separator (Explorer and TC agree here), so again the
 *     whole name is the stem;
 *   - the name ends in a dot ("archive.") — splitting there would produce an
 *     empty extension, which is not really an extension either; the trailing
 *     dot stays part of the stem instead of being silently dropped.
 */
export function splitName(name: string, isDir: boolean): SplitName {
  if (isDir) return { stem: name, ext: "" };

  const lastDot = name.lastIndexOf(".");
  if (lastDot <= 0 || lastDot === name.length - 1) {
    return { stem: name, ext: "" };
  }

  return { stem: name.slice(0, lastDot), ext: name.slice(lastDot + 1) };
}
