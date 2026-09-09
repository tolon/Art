// Which tree the update chain is asked about — one rule, one place.
//
// **The defect this exists for** (round 3 task 3, fix round 1, Important 2).
// Tab 2 asked `osinstall_chain` about the *destination* when
// `osinstall_describe_tree` called it a build; tab 1's readout and the
// Amiga-side panel were handed `session.tree.root`. Two lanes of one wizard,
// two trees, and therefore two `installed` answers about one file — the
// screen contradicting itself rather than the core, which is the same failure
// as the two package lists this round deleted.
//
// So the rule is written once and both tabs read it here:
//
//   - **the destination, when ART has looked at it and found a build** — the
//     user is building into that folder, so the run is an update of it;
//   - **the session's own tree otherwise** — what every screen used before
//     this hook, including a destination that is empty (a fresh build), one
//     that is not a build at all, and one ART could not look at.
//
// `settled` is the other half, and it is not a convenience. `isTree` is
// answered by a round trip, so for the first render or two a destination that
// *is* a build looks like one that is not — long enough for a caller to ask
// the chain about the wrong tree, get *ready* for a row the tree already
// carries, and then swap it for *installed* under the reader's eyes. A caller
// that starts disk work waits for `settled`; one that only renders a path
// need not.
//
// **A look that failed settles too** (fix round 2). `settled` reads
// `DestinationCheck.looked`, which counts both questions finishing rather
// than watching `tree` change — because a refusal leaves `tree` at exactly
// the `null` it had before the question was asked. Waiting on the value
// instead would leave a tab that asks nothing, for ever, because one IPC call
// threw: a screen that says nothing is worse than one that falls back to the
// tree the session already carries and says so.

import { useBuildSession } from "@/lib/useBuildSession";
import { useDestinationCheck, type DestinationCheck } from "@/lib/useDestinationCheck";

export interface ChainTree {
  /** The tree an update run is about, or `null` when this build has none. */
  treeRoot: string | null;
  /**
   * Whether {@link treeRoot} is the answer or a placeholder.
   *
   * `false` only while ART is still looking at a destination that is set.
   * With no destination there is nothing to wait for, so it is `true` from
   * the first render.
   */
  settled: boolean;
  /**
   * **Which of the two rules above produced {@link treeRoot}** — so a screen
   * that owns a *field* writing `session.tree.root` can tell whether that
   * field is the value it is showing (round 3 fix wave, Important 2).
   *
   * `AmigaInstallPanel`'s own *Distribution tree* Browse wrote the session's
   * tree and then watched the path snap back, because the destination had
   * won: a control that appears to work and changes nothing is the confident
   * wrong sentence in its purest form. It reads this and says where the tree
   * came from instead of offering a dead field.
   *
   * `"none"` is neither: no destination ART found a build in, and no session
   * tree either — there is no path to attribute.
   */
  source: "destination" | "session" | "none";
}

/**
 * @param check the caller's **own** already-computed answer for this same
 *   destination, when it has one. `FilesTab` asks
 *   `useDestinationCheck(destination, result)` for its occupied-folder
 *   refusal and hands the whole thing over, so one screen makes one round
 *   trip per path instead of two identical ones; it also means that screen's
 *   `revision` — a finished install — is honoured here without this hook
 *   growing a second parameter about it. Omit it and the hook asks for
 *   itself, which is what tab 2 does.
 */
export function useChainTree(destination: string | null, check?: DestinationCheck): ChainTree {
  const { session } = useBuildSession();
  // Hooks are unconditional, so the question is turned off by passing `null`
  // rather than by not calling: `useDestinationCheck(null)` asks nothing and
  // holds nothing.
  const own = useDestinationCheck(check ? null : destination);
  const answer = check ?? own;
  const fromDestination = Boolean(answer.tree?.isTree);
  const treeRoot = fromDestination ? destination : session.tree.root;
  return {
    treeRoot,
    settled: !destination || answer.looked,
    // Attributed to the rule that produced it, never guessed at by
    // comparing the two paths afterwards: a session tree that happens to
    // equal the destination is still the *session's*, and a caller that
    // hides a field on a string comparison would hide it on a coincidence.
    source: fromDestination ? "destination" : treeRoot ? "session" : "none",
  };
}
