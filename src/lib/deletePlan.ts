// What F8 (Delete) actually removes, and in how many operations.
//
// **One route, and a single entry is a one-entry batch** (ART-081). This
// exists because the screen had two: `volumeDelete` for one entry and
// `volumeDeleteMany` for several. They were not the same promise. The batch
// accumulates into one `BlockSet` and one journalled commit that rolls back
// whole on either write strategy (ART-073); the single call committed on its
// own, the instant it returned. So the *commoner* case — one file — took the
// weaker guarantee, and one act of the user's could be half-done in a way the
// same act over two files could not.
//
// Worse, the one place a single delete really is two removals took the worst
// of it: a file and its Workbench icon went as two separate committed
// operations, with a confirmation *between* them. A failure there left the
// file gone and `Turrican.info` behind — an icon that opens nothing, which is
// exactly the §7.1 clutter the icon question exists to prevent.
//
// So the decision is made here, as data, and the screen dispatches it: one
// list of names, one call, whatever the count. Pure — no Tauri, no i18n
// singleton — so the rule can be pinned by a test rather than read off a
// 4,800-line page.

/** Just enough of a `PanelEntry` to decide about it. */
export interface DeleteEntry {
  name: string;
  /** `HSPARWED` as the pane read it, or `null` when the listing carried
   *  none. Only [`isDeleteProtected`](./protection) reads it; this module
   *  takes the answer, not the bits. */
  deleteProtected: boolean;
}

/** The icon `volumeIconFor` found beside an entry, if it found one. */
export interface DeleteIcon {
  name: string;
}

export interface DeletePlan {
  /** Every name to remove, in one batch. Never empty. */
  names: string[];
  /**
   * Whether the user was shown the third question — the one about an entry
   * AmigaDOS itself protects — and said yes (ART-088).
   *
   * **Taken from the entry, never from the icon.** An `.info` is not
   * separately protected in any case ART has met, and inferring an override
   * from one would let a protected *file* be removed on the strength of a
   * question that was never asked about it. False is the safe answer and the
   * writer refuses on it, naming the entry.
   */
  overrideProtection: boolean;
  /** Whether the icon is going too — the screen's own wording depends on it. */
  withIcon: boolean;
}

/**
 * What to remove for one F8, given the answers already collected.
 *
 * `alsoIcon` is the user's answer to the icon question, which the screen must
 * ask **before** anything is deleted: asking afterwards is what made the icon
 * a second operation, and a second operation is a second chance to end up
 * half-done.
 *
 * The icon is appended, not prepended, so the entry the user actually named
 * is first in the batch and first in anything that reports on it.
 */
export function planDelete(
  entry: DeleteEntry,
  icon: DeleteIcon | null,
  alsoIcon: boolean
): DeletePlan {
  const withIcon = icon !== null && alsoIcon;
  return {
    names: withIcon ? [entry.name, (icon as DeleteIcon).name] : [entry.name],
    overrideProtection: entry.deleteProtected,
    withIcon,
  };
}

/**
 * What to remove for an F8 over a whole selection.
 *
 * The same shape as {@link planDelete} and deliberately so: the batch case is
 * not a different function with different guarantees, it is the same one with
 * more names. No icon question here — that prompt does not scale to a
 * selection, and an icon stays selectable on its own.
 *
 * `overrideProtection` is true when **any** entry in the selection is
 * protected, because the screen asks one question naming them all. It is the
 * answer to that question, not a per-entry flag: the writer takes one.
 */
export function planDeleteSelection(entries: DeleteEntry[]): DeletePlan {
  return {
    names: entries.map((entry) => entry.name),
    overrideProtection: entries.some((entry) => entry.deleteProtected),
    withIcon: false,
  };
}
