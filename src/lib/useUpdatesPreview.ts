// What the ticked updates would replace in the tree they are about to land
// on — one preview per row, asked with **the file the readout resolved**
// (four-tab design § 3.2, ART-289).
//
// ## The defect this closes
//
// `osinstall_collisions` takes the user's own per-slot file choices exactly
// as `osinstall_add_package` does, and the deleted `PackagePanel` passed none
// of them. Over a folder holding two copies of BoingBag 1, the material
// readout said *the file you chose* while the preview two sections below said
// *more than one archive carries 'BoingBag3.9-1'* and disabled the button —
// two answers to one file on one screen, and the one that refused was the one
// that does the work. Here there is one answer: the row's file comes from the
// slot, its folder is that file's own folder, and the pair goes down as the
// override. The guard test asserts all four arguments of every call, because
// dropping any one of them is the same defect.
//
// ## Three rules, and each of them is a sentence somebody would otherwise
// have read wrongly
//
// **Only when the destination is already a tree.** `osinstall_apply` refuses
// a destination with anything in it (`refuse_unless_free`), so a build with a
// tree phase lands on an empty folder and there is nothing there to collide
// with. Asking anyway would start a job per row — each one extracting a real
// archive into scratch — to answer a question with no content.
//
// **One row at a time.** Each ask extracts a whole package archive; two at
// once is two extractions competing for one disk and one scratch root. The
// rows are asked in the order they arrive, which is the chain's order, which
// is the order the run will apply them in.
//
// **A total is stated only when every row has answered.** A sum over half the
// rows, or over a row whose preview threw, looks exactly like a complete
// answer — CLAUDE.md's failure that does not crash. `totals` is `null` until
// there is nothing outstanding and nothing errored.
//
// ## What `totals` can and cannot say, and why it is one number
//
// The component preview (`osinstall_component_collisions`) answers `placed`
// and `contested` alongside its reports, so tab 2's fold can partition what a
// component would place into *landed on nothing* / *landed on identical
// bytes* / *replaced something*. **`osinstall_collisions` answers reports and
// nothing else** — no `placed`, no `contested`, in the wrapper or in
// `commands::osinstall::OsInstallCollisionsResult` — so for an update row the
// only fact ART holds is how many files it would replace. `fresh` and
// `unchanged` would have to be invented, and inventing them as zero is the
// screen out-claiming the core. So this hook states the one count it has, and
// the caller that merges it with the component preview's three states which
// of them it is adding to.

import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";

import type { SequenceInputs } from "@/lib/buildRun";
import { folderOf } from "@/lib/buildRun";
import { errorText } from "@/lib/errorText";
import { osinstallCollisions, type CollisionReport } from "@/lib/osinstall";

/** One ticked update's own answer. */
export interface UpdatePreviewRow {
  packageId: string;
  /** The package's own name (ART-060), carried through so the caller can name
   *  the row without re-deriving it from an id. */
  name: string;
  /** What this row would replace, or `null` when ART could not find out —
   *  never an empty list for a question that was not answered. */
  collisions: CollisionReport[] | null;
  /** Why it could not, in the user's own language, or `null`. */
  error: string | null;
}

export interface UpdatesPreview {
  /** One row per ticked update, in the chain's order, appearing as each
   *  answer arrives. */
  previews: UpdatePreviewRow[];
  /**
   * How many files the ticked updates would replace in total — `null` while
   * any row is outstanding, when any row errored, and when there is no
   * destination to ask about.
   *
   * See the module comment for why this is one count and not three.
   */
  totals: { replaced: number } | null;
  loading: boolean;
}

export function useUpdatesPreview({
  destination,
  destinationIsTree,
  updates,
}: {
  /** The folder the build is about to write into. `null` is a question
   *  nobody can ask, and it is answered with `null` rather than with zero. */
  destination: string | null;
  /** `useDestinationCheck(destination).tree?.isTree` — see the module
   *  comment: a fresh destination has nothing to collide with. */
  destinationIsTree: boolean;
  /** The ticked, runnable rows with the files the slots resolved —
   *  `useTickedUpdates()` in `ChoiceTab.tsx`, which is the same chain and the
   *  same slot report tab 2 draws its list from. */
  updates: SequenceInputs["updates"];
}): UpdatesPreview {
  const { t } = useTranslation();
  const [previews, setPreviews] = useState<UpdatePreviewRow[]>([]);
  const [loading, setLoading] = useState(false);
  /** Whether the list on screen was asked in full — `previews.length` alone
   *  cannot say it, because a list that shrank to nothing and a list nobody
   *  asked about are both empty. */
  const [complete, setComplete] = useState(false);

  // **A primitive dependency, not the array** (ART-178, and the loop it named:
  // 2,149 preview jobs in one session). `useTickedUpdates` rebuilds this list
  // on every render, so an array identity here would re-extract every archive
  // on every render. Two equal strings are the same value to React; two equal
  // arrays are not.
  const updatesKey = JSON.stringify(updates);

  useEffect(() => {
    const list = JSON.parse(updatesKey) as SequenceInputs["updates"];
    // The previous list's answers go with it (`MaterialReadout`'s F9): counts
    // of a selection the user has already changed look current and are not.
    setPreviews([]);
    if (!destination || !destinationIsTree || list.length === 0) {
      setLoading(false);
      // Nothing to ask about is *complete* only when there was a folder to
      // ask about. With no destination, `totals` stays null: zero would be an
      // answer about a folder nobody has named.
      setComplete(Boolean(destination));
      return;
    }
    setLoading(true);
    setComplete(false);
    let cancelled = false;
    void (async () => {
      const answered: UpdatePreviewRow[] = [];
      for (const update of list) {
        // Checked **between** whole rows, never inside one: a cancelled
        // generation stops asking, and the ask already in flight is simply
        // dropped when it lands (CLAUDE.md's job rule, and ART-089's
        // mechanism for a superseded answer).
        if (cancelled) return;
        const folder = folderOf(update.file);
        try {
          const reports = folder
            ? await osinstallCollisions(destination, folder, [update.packageId], [
                [update.slotId, update.file],
              ])
            : // A file with no folder part is not a path this core produced —
              // it is asked about rather than assumed away, because
              // `osinstallCollisions` answers an empty folder with `[]`, and
              // "nothing in the way" is not the same fact as "nobody asked".
              null;
          if (cancelled) return;
          answered.push(
            reports
              ? { packageId: update.packageId, name: update.name, collisions: reports, error: null }
              : {
                  packageId: update.packageId,
                  name: update.name,
                  collisions: null,
                  error: t("osBuilder.build.previewNoFolder", { file: update.file }),
                }
          );
        } catch (e) {
          if (cancelled) return;
          // Named rather than swallowed, and per row: one row ART could not
          // preview must not read as a row with nothing to replace, and it
          // must not stop the rows after it either.
          answered.push({
            packageId: update.packageId,
            name: update.name,
            collisions: null,
            error: errorText(t, e),
          });
        }
        setPreviews([...answered]);
      }
      setLoading(false);
      setComplete(true);
    })();
    return () => {
      cancelled = true;
    };
    // `t` is deliberately absent, for `useHostPlacement`'s own reason: it
    // changes identity on a language switch, and re-running a folder scan —
    // here, a set of archive extractions — because somebody changed language
    // would be disk work for a translation.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [destination, destinationIsTree, updatesKey]);

  const answeredAll = complete && previews.every((row) => row.collisions !== null);
  return {
    previews,
    totals: answeredAll
      ? { replaced: previews.reduce((sum, row) => sum + (row.collisions?.length ?? 0), 0) }
      : null,
    loading,
  };
}
