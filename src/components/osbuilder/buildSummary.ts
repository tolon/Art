// The build's own summary — what tab 4 states in four lines (four-tab design
// § 3.4, round 4 task 4).
//
// **`BuildTab.tsx`'s only reader since round 4 task 5.** The bar under the
// tabs read it too, for a size line of its own, until that turned out to mean
// a second plan, a second chain and a second slot report on every one of tabs
// 1-3; the bar states what the session carries now and asks nothing. The
// module stays where it is rather than folding back into `BuildTab.tsx`,
// because `steps.test.tsx` mocks that component and a hook imported out of a
// mocked module is a hook that does not exist.
//
// It lives beside the tab rather than in `src/lib` because it composes
// `useTickedUpdates`, which is `ChoiceTab`'s — `src/lib` may not import a
// component.

import { useTranslation } from "react-i18next";

import { useTickedUpdates, type TickedUpdates } from "@/components/osbuilder/ChoiceTab";
import { runSummaryLines, sequenceFor, type Phase } from "@/lib/buildRun";
import { componentDef, componentLabel, rememberedComponentKey, type InstallRelease } from "@/lib/osinstall";
import type { Phrase } from "@/lib/phrase";
import { isFlag, isText, isTextOrNothing } from "@/lib/remembered";
import { useBuildSession } from "@/lib/useBuildSession";
import { useDestinationCheck } from "@/lib/useDestinationCheck";
import { useInstallPlan, type InstallPlanState } from "@/lib/useInstallPlan";
import { useRemembered } from "@/lib/useRemembered";
import { useUpdatesPreview } from "@/lib/useUpdatesPreview";

/** What both this tab and the bar under it read off the build. */
export interface BuildSummary {
  release: InstallRelease;
  destination: string | null;
  plan: InstallPlanState;
  /** `osinstall_destination_taken`'s answer — a refusal for a *fresh* build,
   *  and not one in update mode. */
  taken: boolean;
  destinationIsTree: boolean;
  ticked: TickedUpdates;
  firstBootWanted: boolean;
  /** What the run will do, in order — empty when there is nothing to do. */
  phases: Phase[];
  /** The four summary lines, in the design's order. */
  lines: Phrase[];
  /** A component's own name for the screen, from the loaded catalogue. */
  label: (id: string) => string;
}

/**
 * Everything the build's summary is computed from, asked once.
 *
 * @param previewReplacements whether to ask `osinstall_collisions` for each
 *   ticked update — disk work, an archive opened per row, for the fourth
 *   line alone. A caller that does not draw that line passes `false` and
 *   reads {@link BuildSummary.lines} three long.
 */
export function useBuildSummary({
  previewReplacements,
}: {
  previewReplacements: boolean;
}): BuildSummary {
  const { t } = useTranslation();
  const { session, setComponents } = useBuildSession();
  const release = session.release;

  // The plan's inputs, read through the very keys tabs 1-3 read them
  // through: a second key would mean this tab summarising a build nobody
  // asked for. Read-only here — this tab writes none of them.
  const [keymap] = useRemembered<string>(
    rememberedComponentKey("osinstall.keymap", release),
    isText,
    ""
  );
  const [reuseScan] = useRemembered<boolean>("osinstall.reuseScan", isFlag, true);
  const [destination] = useRemembered<string | null>(
    rememberedComponentKey("osinstall.destination", release),
    isTextOrNothing,
    null
  );

  const plan = useInstallPlan({
    release,
    material: session.material,
    keymap,
    rom: session.rom.path,
    destination,
    reuseScan,
    // No *Scan again* button on this tab; nothing here bumps the nonce.
    rescanNonce: 0,
    components: session.components,
    setComponents,
  });

  const { taken, tree } = useDestinationCheck(destination);
  const destinationIsTree = tree?.isTree === true;

  const ticked = useTickedUpdates();
  const firstBootWanted = session.firstboot.wanted ?? true;

  const updatesPreview = useUpdatesPreview({
    destination,
    destinationIsTree,
    updates: previewReplacements ? ticked.rows : [],
  });

  /**
   * A component's own name for the screen — the recipe's `labelKey` when it
   * declares one, its media name otherwise.
   *
   * Moved from `OsInstall.tsx` in task 4; the original went with that file
   * when task 5 renamed it `FilesTab.tsx` and deleted everything the plan
   * drew. Resolved here rather than in `src/lib`, which holds no i18next
   * singleton.
   */
  function label(id: string): string {
    const key = componentDef(plan.catalogue ?? [], id)?.labelKey;
    return key ? t(key) : componentLabel(plan.catalogue ?? [], id);
  }

  /**
   * What the run would replace — **and what each half of that number is
   * about**, which is why the sentence attributes them.
   *
   * `osinstall_collisions` (the update rows) answers a list of collisions and
   * nothing else: it has no `placed`/`contested`, so an update row can only
   * contribute to `replaced`. `fresh` and `unchanged` are the component
   * preview's, and describe the release's own parts alone. Inventing the
   * other two halves for the updates would be the screen out-claiming the
   * core; leaving them out of the sentence would leave a reader to infer
   * them.
   *
   * `null` while either half is outstanding — the line then says it has not
   * been previewed rather than stating a total over half an answer. A plan
   * with **no** layering component at all is a settled zero, not a pending
   * one: nothing layers over anything, so nothing of the release's own parts
   * is replaced.
   */
  const componentReplaces = plan.componentPreview
    ? {
        fresh: plan.componentPreview.placed - plan.componentPreview.contested,
        unchanged: plan.componentPreview.contested - plan.componentPreview.reports.length,
        replaced: plan.componentPreview.reports.length,
      }
    : plan.effectivePlan && plan.layeringOn.length === 0 && !plan.componentPreviewError
      ? { fresh: 0, unchanged: 0, replaced: 0 }
      : null;
  const replaces =
    previewReplacements && componentReplaces && updatesPreview.totals
      ? {
          ...componentReplaces,
          replaced: componentReplaces.replaced + updatesPreview.totals.replaced,
        }
      : null;

  const phases = sequenceFor({
    destination: destination ?? "",
    destinationIsTree,
    release,
    updates: ticked.rows,
    firstBootWanted,
  });

  const lines = runSummaryLines({
    plan: plan.effectivePlan,
    destinationIsTree,
    treeSummary: tree,
    updates: ticked.rows,
    firstBootWanted,
    replaces,
  }).map((line) =>
    // The one parameter `src/lib` cannot fill: `componentsOn` is a list of
    // recipe **ids**, and a person reading "workbench-39, locale" has been
    // shown ART's filing system rather than what is going on the Amiga.
    line.key === "osBuilder.build.summary.tree"
      ? {
          ...line,
          params: {
            ...line.params,
            components: (plan.effectivePlan?.componentsOn ?? []).map(label).join(", "),
          },
        }
      : line
  );

  return {
    release,
    destination,
    plan,
    taken,
    destinationIsTree,
    ticked,
    firstBootWanted,
    phases,
    lines,
    label,
  };
}
