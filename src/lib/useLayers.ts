// Which media layers a release's own recipe declares — the cheapest question
// the OS Builder asks about a release, and the only one tab 1 needs.
//
// **Why this is its own hook** (round 4 task 5's fix round). `FilesTab.tsx`
// called `useInstallPlan` and read three fields of it: `layers`, `layersKnown`
// and `plannedFolders`. Those three cost one `osinstall_layers` — a read of a
// shipped JSON recipe. The rest of that hook costs `osinstall_components` and
// then `osinstall_plan`, which opens and walks every ADF and ISO in the
// material list, and on tab 1 nothing read the answer. That is ART-119's own
// cost — work done for an answer nobody uses — on the tab the user opens
// first and stays on longest while they are still adding folders.
//
// So the layers effect lives here and `useInstallPlan` calls it, rather than
// keeping a second copy: two implementations of "which release is this the
// answer for" is two answers to that question, which is the shape of defect
// the four-tab rewrite exists to remove.
//
// `foldersForPlan(material, layers)` is pure and is `buildSession.ts`'s
// already, so a caller that wants `plannedFolders` composes the two itself —
// tab 1 does, in one `useMemo`.

import { useEffect, useState } from "react";

import { layersFor, type InstallLayer, type InstallRelease } from "@/lib/osinstall";

export interface LayersState {
  /**
   * The release's declared media layers, in the recipe's own order — `[]` for
   * every shipped recipe until AmigaOS 3.2.2's own two-layer one.
   */
  layers: InstallLayer[];
  /**
   * **Whether `layers` is *this* release's answer** (ART-256).
   *
   * `layers` alone cannot say it: `[]` is one value with two causes, "this
   * release is unlayered" and "nobody has asked yet", and this project's own
   * rule is that a state with more than one cause is not a state anything may
   * branch on. Anything scoping itself with `layers.length > 0` reads a
   * layered release as unlayered until `layersFor` resolves — which is how a
   * folder a layered release never sends came to be scanned.
   */
  layersKnown: boolean;
}

/**
 * Ask the recipe which layers it declares.
 *
 * A failed lookup answers `[]` *for this release* rather than staying
 * unknown: a release with no shipped recipe has no layers to speak of, and a
 * caller waiting for an answer that will never come waits for ever.
 */
export function useLayers(release: InstallRelease): LayersState {
  const [layers, setLayers] = useState<InstallLayer[]>([]);
  /** Which release `layers` is the answer for — `null` before the first
   *  answer lands, and the *previous* release's name for the moment after a
   *  switch. */
  const [layersRelease, setLayersRelease] = useState<string | null>(null);
  useEffect(() => {
    let cancelled = false;
    layersFor(release)
      .then((ls) => {
        if (cancelled) return;
        setLayers(ls);
        setLayersRelease(release);
      })
      .catch(() => {
        if (cancelled) return;
        setLayers([]);
        setLayersRelease(release);
      });
    return () => {
      cancelled = true;
    };
  }, [release]);

  return { layers, layersKnown: layersRelease === release };
}
