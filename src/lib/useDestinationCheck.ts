// Two facts about the destination folder (four-tab design § 3.3): whether
// `apply` would refuse it as occupied, and whether it is a tree ART built.
// Asked once, here, for both tabs. A path ART cannot examine is **not**
// declared taken — `apply()` decides, and blocking here would refuse an
// install the engine would have allowed (the rule the effect carried in
// `OsInstall.tsx` since ART-119).

import { useEffect, useState } from "react";

import { osinstallDescribeTree, osinstallDestinationTaken, type TreeSummary } from "@/lib/osinstall";

export interface DestinationCheck {
  taken: boolean;
  /** `null` until ART has looked, or when there is no path. Once looked, a
   *  non-tree answers `isTree: false` with its `problem` — never `null`,
   *  so "not asked" and "not a tree" stay apart. */
  tree: TreeSummary | null;
  /**
   * Whether both questions of the **current** generation have finished —
   * answered *or* refused.
   *
   * `tree === null` cannot say this on its own: it is the value before ART
   * has looked and the value after a look that threw, and a caller who waits
   * for one of them waits for ever on the other (round 3 task 3, fix round
   * 2). A screen that will not ask anything until the destination is known
   * needs to know that ART has *stopped* asking, which is a different fact
   * from what it found.
   *
   * `false` while a look is in flight, and `false` with no path at all —
   * there is nothing to have looked at, and the fields beside it are their
   * empty values for the same reason. A caller with no path is not waiting
   * for anything and should not read this as though it were.
   */
  looked: boolean;
}

/**
 * @param revision anything whose change means the folder may have changed
 *   under us — the install screen passes the last install result, because a
 *   successful apply fills the folder it was told to fill and the next run
 *   must be refused, as it was before this hook existed.
 */
export function useDestinationCheck(path: string | null, revision: unknown = null): DestinationCheck {
  const [taken, setTaken] = useState(false);
  const [tree, setTree] = useState<TreeSummary | null>(null);
  const [looked, setLooked] = useState(false);

  useEffect(() => {
    if (!path) {
      setTaken(false);
      setTree(null);
      setLooked(false);
      return;
    }
    let cancelled = false;
    // Both fields belong to one generation: a new path or revision takes the
    // old answers down before asking, so a fast `taken` never sits beside a
    // stale `tree`.
    setTaken(false);
    setTree(null);
    setLooked(false);
    // **Counted, not inferred from the values.** A refusal leaves a field at
    // exactly the value it had before the question was asked, so the only
    // way to know both questions are over is to count them being over.
    // Local to this effect run, so a superseded generation counts its own
    // two down and never touches the new one's.
    let outstanding = 2;
    const settled = () => {
      outstanding -= 1;
      if (!cancelled && outstanding === 0) setLooked(true);
    };
    void osinstallDestinationTaken(path)
      .then((answer) => {
        if (!cancelled) setTaken(answer);
      })
      .catch(() => {
        if (!cancelled) setTaken(false);
      })
      .finally(settled);
    void osinstallDescribeTree(path)
      .then((summary) => {
        if (!cancelled) setTree(summary);
      })
      .catch(() => {
        if (!cancelled) setTree(null);
      })
      .finally(settled);
    return () => {
      cancelled = true;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [path, revision]);

  return { taken, tree, looked };
}
