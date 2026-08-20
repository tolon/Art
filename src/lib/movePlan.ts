// What F6 (Move) is allowed to do, and why it refuses when it does not.
//
// Total Commander's F6 moves the marked entries to the other pane; ART's does
// the same, but a move is a copy *and a delete*, so it is the one function key
// that can destroy the original. §92's pipeline applies in full — nothing is
// deleted before the copy has been verified — and this module is the
// VALIDATE/RECOMMEND half of it: everything that can be decided before a byte
// moves is decided here, where a test can reach it, rather than inside the
// 3,000-line page.
//
// The rule the refusals all come from: **a move must never be able to end with
// less data than it started with.** Each one below is a case where going ahead
// could.
//
// Pure, like `@/lib/selection` and `@/lib/paneSources`: no Tauri, no i18n
// singleton — a refusal carries a `Phrase` for the caller to render.

import type { PaneKind } from "@/lib/isoPane";
import type { Phrase } from "@/lib/phrase";

/** Just enough of a `PanelEntry` to decide about it. */
export interface MoveEntry {
  name: string;
  isDir: boolean;
}

export type MovePlan =
  | { kind: "move"; entries: MoveEntry[] }
  | { kind: "refused"; reason: Phrase };

export interface MoveInput {
  sourceKind: PaneKind;
  targetKind: PaneKind;
  /** `writableVolume(source) !== null` — the source volume accepts writes,
   *  which for a move means it accepts the *delete* half. Not consulted when
   *  the source is a host folder: those are deleted through the Recycle Bin,
   *  not the volume writer (ART-080). */
  sourceWritable: boolean;
  /**
   * Whether the source pane is a **drive root** rather than a folder inside
   * one (ART-080).
   *
   * Only meaningful for a `local` source, and the one case a host move is
   * still refused in. `C:\` is where `Windows` and `Program Files` live, and
   * the confirmations a user learns to click through for a game are the same
   * ones here.
   */
  sourceIsRoot: boolean;
  /** `writableVolume(target) !== null`. Not required when the target is a
   *  host folder: ART writes those through the extract path, not the volume
   *  writer. */
  targetWritable: boolean;
  /** The marked entries, in pane order. */
  entries: MoveEntry[];
  /** Every name the destination directory already holds — read from the
   *  destination's *unfiltered* listing, since a filename mask hiding a
   *  colliding name must not make the collision invisible. */
  takenNames: string[];
  /**
   * Whether both panes address the **same volume** — the same image file *and*
   * the same partition inside it (ART-081).
   *
   * A move between two *directories of one volume* is not a copy-and-delete at
   * all — it is a relink, and doing it as copy-then-delete would stage the
   * tree out, write it back into the same volume, and then remove the
   * original: twice the free space needed, and a failure between the two
   * halves losing the only copy. F5 has always refused this
   * (`files.err.sameImage`); F6 could reach it and did not, which mattered
   * only while F6 was restricted to one directory.
   *
   * **The same *image* is not enough** (review F9). One HDF holds several
   * partitions, and `DH0:` to `DH1:` is a move between two volumes that share
   * a file — real free space is consumed at the destination and real space is
   * freed at the source, which is exactly the case a copy-and-delete is for.
   * Refusing it would have refused the commonest move on a real PiStorm card.
   * The caller compares the partition too.
   */
  sameImage: boolean;
}

/** The three pane kinds ART only ever reads. Nothing can be moved *out* of
 *  one (there is no delete) or *into* one (there is no write). */
function isReadOnlyContainer(kind: PaneKind): boolean {
  return kind === "iso" || kind === "archive" || kind === "c64";
}

/**
 * Which of `names` the destination already holds, compared case-insensitively.
 *
 * Case-insensitively because AmigaDOS is: a volume holding `Docs` will not
 * take a second entry called `docs`, and a comparison that missed that would
 * hand the collision to the copy engine to discover halfway through — which
 * for a move is exactly the moment the source is about to be deleted
 * (the same weakness ART-072 records for the copy path's own check).
 */
export function collidingNames(names: string[], takenNames: string[]): string[] {
  const taken = new Set(takenNames.map((name) => name.toLowerCase()));
  return names.filter((name) => taken.has(name.toLowerCase()));
}

/**
 * Whether F6 can move this selection, and what to say when it cannot.
 *
 * Two refusals are worth their own note, because both are ART lacking
 * something rather than the user asking for something silly:
 *
 * - **A host folder can be the source now** (ART-080), and the refusal that
 *   used to stand here is gone. Every delete ART owned went *into* a disk
 *   image; removing a file from the user's own disk needed a decision about
 *   where it goes, and the owner made it: the **Windows Recycle Bin**. ART
 *   invents no recovery mechanism of its own and uses the one the operating
 *   system already has — the one place a user already knows to look.
 *
 *   Two things about that reach this module rather than staying in the engine.
 *   A host delete is **not** all-or-nothing — a host filesystem has no journal
 *   — so the caller has to be ready for a partial result and say which names
 *   did not go. And the source pane still has to be a *folder*: a host
 *   **root** (`C:\`) is refused below, because a selection there can name a
 *   system directory and the confirmation a user clicks through is the same
 *   one they click through for a game.
 * - **Between two images** used to be refused for anything but a single
 *   directory (ART-081), and is not any more. Both halves now have the same
 *   shape and the same guarantee: `volumeCopyBetweenMany` stages exactly what
 *   was marked into one operation with one backup and one commit (ART-176),
 *   and `volumeDeleteMany` removes exactly those names as one journalled
 *   operation that rolls back whole on either write strategy (ART-073). A
 *   single entry is a one-entry batch through the same route, which is the
 *   rule this whole area now follows rather than keeping a second path for
 *   the common case.
 *
 *   The sequencing that made this a decision rather than a line of code is in
 *   `FileManager.tsx`'s own `moveSelection`, unchanged by this: the
 *   destination is **re-listed** and every name looked for before anything is
 *   deleted, so the worst a failed move can leave is a duplicate — never a
 *   gap. What a batch adds is that the delete is one operation, so it cannot
 *   half-happen either.
 *
 * A name already taken at the destination is refused rather than resolved by
 * the overwrite policy. "Leave it alone" would skip the copy and then delete
 * the source — the file is simply gone — and "replace it" would destroy the
 * destination's copy to make room. A move is not the place to be asked.
 */
export function planMove(input: MoveInput): MovePlan {
  const { sourceKind, targetKind, entries } = input;
  const names = entries.map((entry) => entry.name);

  if (entries.length === 0) {
    return { kind: "refused", reason: { key: "files.move.refuseNothing" } };
  }

  if (sourceKind === "local" && input.sourceIsRoot) {
    // A drive root is not a folder anyone moves *out of* — it is where
    // `Windows` and `Program Files` live, and the two confirmations a user
    // learns to click through for a game are the same two here. Explorer
    // itself refuses to recycle a drive root's own protected contents; ART
    // simply does not offer the case.
    return { kind: "refused", reason: { key: "files.move.refuseLocalRoot" } };
  }
  if (isReadOnlyContainer(sourceKind)) {
    return { kind: "refused", reason: { key: "files.move.refuseReadOnlySource" } };
  }
  // A host folder's delete goes through the Recycle Bin, not the volume
  // writer, so `sourceWritable` is a question about volumes only.
  if (sourceKind !== "local" && !input.sourceWritable) {
    return { kind: "refused", reason: { key: "files.move.refuseSourceNotWritable" } };
  }

  if (isReadOnlyContainer(targetKind)) {
    return { kind: "refused", reason: { key: "files.move.refuseReadOnlyTarget" } };
  }
  // **Host to host is not ART's job** (review F8), and refusing it *here* is
  // the point: it used to be planned as allowed and then refused by the page
  // after **both** confirmations — the user answered "yes, move these" and
  // "yes, overwrite the protected one" and only then learnt ART would not.
  // A refusal a plan can reach has to be reached in the plan.
  //
  // Explorer moves files between folders, and a commander that copies through
  // ART's volume writer to do it would be a worse Explorer with an extra
  // failure mode. F5 copies; the OS moves.
  if (sourceKind === "local" && targetKind === "local") {
    return { kind: "refused", reason: { key: "files.move.refuseHostToHost" } };
  }
  if (targetKind !== "local" && !input.targetWritable) {
    return { kind: "refused", reason: { key: "files.move.refuseTargetNotWritable" } };
  }

  if (targetKind !== "local" && input.sameImage) {
    return { kind: "refused", reason: { key: "files.err.sameImage" } };
  }

  const collisions = collidingNames(names, input.takenNames);
  if (collisions.length > 0) {
    return {
      kind: "refused",
      reason: {
        key: "files.move.refuseCollision",
        params: { names: collisions.slice(0, 3).join(", "), count: collisions.length },
      },
    };
  }

  return { kind: "move", entries };
}
