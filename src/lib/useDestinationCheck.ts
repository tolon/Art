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

  useEffect(() => {
    if (!path) {
      setTaken(false);
      setTree(null);
      return;
    }
    let cancelled = false;
    // Both fields belong to one generation: a new path or revision takes the
    // old answers down before asking, so a fast `taken` never sits beside a
    // stale `tree`.
    setTaken(false);
    setTree(null);
    void osinstallDestinationTaken(path)
      .then((answer) => {
        if (!cancelled) setTaken(answer);
      })
      .catch(() => {
        if (!cancelled) setTaken(false);
      });
    void osinstallDescribeTree(path)
      .then((summary) => {
        if (!cancelled) setTree(summary);
      })
      .catch(() => {
        if (!cancelled) setTree(null);
      });
    return () => {
      cancelled = true;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [path, revision]);

  return { taken, tree };
}
