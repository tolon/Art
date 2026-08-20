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
   *  which for a move means it accepts the *delete* half. */
  sourceWritable: boolean;
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
 * - **A host folder cannot be the source.** ART has no command that deletes a
 *   file on the user's own disk, by design — every delete it owns goes into a
 *   disk image through `core/volume/write`. Moving *out* of a folder would
 *   need one (recorded as ART-080), and inventing it inside a UI task is
 *   exactly the "smuggled in" engine work this phase's plan rules out.
 * - **Several entries between two images at once** is ART-064's gap seen from
 *   the *move* side. The copy half exists now (`volumeCopyBetweenMany`, one
 *   staged batch under one backup and one commit); what a move also needs is
 *   a batch delete of the source entries with the same all-or-nothing
 *   guarantee, sequenced after the destination verifies. Until that is built
 *   deliberately, this refuses.
 * - **A single *file* between two images** is ART-081. Its copy half exists
 *   now too: since ART-176 there is one route between two images and it
 *   stages exactly what was marked, so a lone file is a one-entry batch and
 *   nothing beside it rides along. What is still missing is the *delete*
 *   half — a move is a copy **and** a delete, and sequencing those safely
 *   (verify, then remove, and what to do when the second fails) is the
 *   decision ART-081 records. This refusal stays until it is made.
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

  if (sourceKind === "local") {
    return { kind: "refused", reason: { key: "files.move.refuseLocalSource" } };
  }
  if (isReadOnlyContainer(sourceKind)) {
    return { kind: "refused", reason: { key: "files.move.refuseReadOnlySource" } };
  }
  if (!input.sourceWritable) {
    return { kind: "refused", reason: { key: "files.move.refuseSourceNotWritable" } };
  }

  if (isReadOnlyContainer(targetKind)) {
    return { kind: "refused", reason: { key: "files.move.refuseReadOnlyTarget" } };
  }
  if (targetKind !== "local" && !input.targetWritable) {
    return { kind: "refused", reason: { key: "files.move.refuseTargetNotWritable" } };
  }

  if (targetKind !== "local") {
    if (entries.length > 1) {
      return { kind: "refused", reason: { key: "files.err.batchBetweenVolumes" } };
    }
    if (!entries[0].isDir) {
      return { kind: "refused", reason: { key: "files.move.refuseFileBetweenImages" } };
    }
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
