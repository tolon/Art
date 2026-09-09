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
//     this hook, including a destination that is empty (a fresh build) or is
//     not a build at all.
//
// `settled` is the other half, and it is not a convenience. `isTree` is
// answered by a round trip, so for the first render or two a destination that
// *is* a build looks like one that is not — long enough for a caller to ask
// the chain about the wrong tree, get *ready* for a row the tree already
// carries, and then swap it for *installed* under the reader's eyes. A caller
// that starts disk work waits for `settled`; one that only renders a path
// need not.

import { useBuildSession } from "@/lib/useBuildSession";
import { useDestinationCheck } from "@/lib/useDestinationCheck";

export interface ChainTree {
  /** The tree an update run is about, or `null` when this build has none. */
  treeRoot: string | null;
  /**
   * Whether {@link treeRoot} is the answer or a placeholder.
   *
   * `false` only while a destination is set and ART has not finished looking
   * at it. With no destination there is nothing to wait for, so it is `true`
   * from the first render.
   */
  settled: boolean;
}

export function useChainTree(destination: string | null): ChainTree {
  const { session } = useBuildSession();
  const { tree } = useDestinationCheck(destination);
  return {
    treeRoot: tree?.isTree ? destination : session.tree.root,
    // `tree` is `null` both before ART has looked and when the look itself
    // failed, which `useDestinationCheck` cannot tell apart — a failed look
    // therefore never settles. That is deliberate rather than overlooked:
    // `osinstall_describe_tree` answers `isTree: false` with a `problem` for
    // every folder it can reach, so a rejection means the IPC bridge is gone,
    // and asking the chain about a guessed tree in that state would produce a
    // list of confident sentences with nothing behind them.
    settled: !destination || tree !== null,
  };
}
