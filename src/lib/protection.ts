// Reading an Amiga entry's protection bits off the row (brief §3.4).
//
// `PanelEntry.attrs` arrives already formatted by Rust
// (`core::volume::write::uaem::format_bits`) as eight characters, most
// significant first — `hsparwed` — with a `-` wherever the bit is not set. A
// letter therefore means *the thing is allowed*, which is the normalisation
// the Rust side already did: in the raw protection field the low four bits are
// inverted, and nothing above this line should have to know that.
//
// This exists for one question: **is this entry delete-protected?** Total
// Commander's `[Confirmation]` keeps "overwrite read-only" on in the user's
// own config, and the Amiga's nearest equivalent is the `d` bit — a file with
// it cleared is one AmigaDOS itself will refuse to delete.

/** The eight bits, in the order `format_bits` writes them. */
const BIT_ORDER = ["h", "s", "p", "a", "r", "w", "e", "d"] as const;

/** Whether `attrs` has a named bit set. */
export function hasBit(attrs: string | null, bit: (typeof BIT_ORDER)[number]): boolean {
  if (!attrs || attrs.length !== BIT_ORDER.length) return false;
  const index = BIT_ORDER.indexOf(bit);
  return attrs[index] === bit;
}

/**
 * Whether AmigaDOS would refuse to delete this entry.
 *
 * A local file is never "delete-protected" as far as this is concerned: its
 * `attrs` are Windows attributes (`rahs`), a different alphabet with a
 * different length, and reading `d` out of them would be reading a bit that is
 * not there. `hasBit` returns false for anything that is not eight characters,
 * which is what makes that safe rather than accidental.
 *
 * **Unknown counts as deletable.** A row with no attributes at all — a source
 * that does not report them — must not become undeletable because ART could
 * not tell; the confirmation this feeds is a warning, not a lock.
 */
export function isDeleteProtected(attrs: string | null): boolean {
  if (!attrs || attrs.length !== BIT_ORDER.length) return false;
  return !hasBit(attrs, "d");
}

/** The names in `entries` that AmigaDOS would refuse to delete. */
export function deleteProtectedNames(
  entries: Array<{ name: string; attrs: string | null }>
): string[] {
  return entries.filter((entry) => isDeleteProtected(entry.attrs)).map((entry) => entry.name);
}
