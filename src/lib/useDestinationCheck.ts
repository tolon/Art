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

export function useDestinationCheck(path: string | null): DestinationCheck {
  const [taken, setTaken] = useState(false);
  const [tree, setTree] = useState<TreeSummary | null>(null);

  useEffect(() => {
    if (!path) {
      setTaken(false);
      setTree(null);
      return;
    }
    let cancelled = false;
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
  }, [path]);

  return { taken, tree };
}
