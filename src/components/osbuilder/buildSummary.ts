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
import {
  runSummaryLines,
  sequenceFor,
  type Phase,
  type ReplacesSummary,
} from "@/lib/buildRun";
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
  /** Whether ART has finished asking about the destination folder. Until it
   *  has, *fresh build* and *existing tree* are both guesses — and they are
   *  two different sequences, so nothing is offered on one. */
  destinationChecked: boolean;
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
 * @param revision anything whose change means the destination folder may have
 *   changed under ART — tab 4 passes its run's completion, because a run that
 *   just succeeded has filled the folder it was told to fill and the next
 *   answer about it is a different one (fix round 1, I2).
 */
export function useBuildSummary({
  revision = null,
}: {
  revision?: unknown;
} = {}): BuildSummary {
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

  // **Before the plan**, because the plan is now asked a question that
  // depends on the answer: whether this build writes a tree at all.
  const { taken, tree, looked } = useDestinationCheck(destination, revision);
  const destinationIsTree = tree?.isTree === true;
  // With no path there is nothing to have looked at, and `looked` is false
  // for that too — which is not the same fact and must not read as one.
  const destinationChecked = destination === null || looked;

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
    // **Not in update mode, and not before ART knows** (fix round 1, C1).
    // The component preview partitions what the release's own parts would
    // place, and in update mode they are not placed at all — so the answer
    // is neither shown nor asked for. Asking while the destination check is
    // still in flight would ask it for exactly the folders that turn out to
    // be trees, which is the work this saves and the answer this must not
    // hold.
    previewCollisions: destinationChecked && !destinationIsTree,
  });

  const ticked = useTickedUpdates();
  const firstBootWanted = session.firstboot.wanted ?? true;

  // **Every ticked row, always.** There was a `previewReplacements: false`
  // arm here for the bar under the tabs, which drew a size line and not the
  // replace line and must not have paid an archive-per-row for one it did
  // not draw. The bar stopped reading this hook in round 4 task 5 and tab 4
  // is its only caller, so the gate had one value — and a gate with one
  // value reads as though it were holding a line it is not.
  const updatesPreview = useUpdatesPreview({
    destination,
    destinationIsTree,
    updates: ticked.rows,
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

  const replaces: ReplacesSummary = destinationIsTree
    ? // **Update mode says only what the updates would do** (fix round 1,
      // C1). The component preview describes the release's own parts
      // landing in an empty folder, and in update mode they do not land at
      // all — printing its counts here is a claim about work that will not
      // run, in the sentence that is meant to tell the user what will.
      updatesPreview.totals
      ? { state: "updates", replaced: updatesPreview.totals.replaced }
      : { state: "pending" }
    : plan.componentPreviewError
      ? // A preview ART could not produce is its own ending (fix round 1,
        // I5): "not previewed yet" says one is still coming, which is a
        // promise nothing is going to keep.
        { state: "failed", detail: plan.componentPreviewError }
      : componentReplaces && updatesPreview.totals
        ? {
            state: "components",
            ...componentReplaces,
            replaced: componentReplaces.replaced + updatesPreview.totals.replaced,
          }
        : { state: "pending" };

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
    destinationChecked,
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
    destinationChecked,
    ticked,
    firstBootWanted,
    phases,
    lines,
    label,
  };
}
