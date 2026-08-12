// The single-entry half of the function-key table (F3 View, F4 Edit,
// Shift+F6 Rename, F9 Attributes), factored out of `FileManager.tsx` so it
// can be tested without rendering the page — which calls Tauri commands on
// mount, the same reason `@/lib/selection` exists (see its own header
// comment).
//
// F5 Copy, F6 Move and F8 Delete act on the *whole* selection (one entry or
// several) and are not covered here — F6's own rules live in
// `@/lib/movePlan`, which needs both panes rather than one. This module is
// only the four keys that must
// refuse outright rather than guess at an entry when more than one is
// selected (finding 4 of the phase-1a whole-branch review: nothing failed
// if one of these `run` closures were changed to pick an arbitrary entry
// out of several, because nothing outside a full page render exercised the
// table at all).
//
// `FileManager.tsx` builds each `FunctionAction.run` from the same `target`
// this returns for `enabled`, rather than re-deriving "the selected entry"
// a second time — so a regression that swaps one but not the other would at
// least be a visible one-line diff, even though only a full render (deferred,
// see ART-* debt) can prove `run` itself was not changed to bypass `target`.

import type { PanelEntry } from "@/lib/panel";
import { singleSelected } from "@/lib/selection";

/** One single-entry key's availability and what it would act on. `target` is
 * `null` exactly when `enabled` is false due to the selection shape — either
 * nothing or more than one entry — which is also true when `enabled` is
 * false for another reason (not in a volume, read-only, busy): `enabled`
 * alone is what a caller must check before using `target`. */
export interface SingleKeyPlan {
  enabled: boolean;
  target: PanelEntry | null;
}

export interface FunctionKeyPlanInput {
  entries: PanelEntry[];
  selected: Set<string>;
  /** The pane holds an open ADF/HDF volume — what F3 and F9 require. */
  inVolume: boolean;
  /** The pane's volume accepts writes — what F4 and Shift+F6 require. */
  canWrite: boolean;
  /** A background operation is running — F4 and Shift+F6 wait for it, the
   * same way their button in the bar disables while `busy !== null`. */
  busy: boolean;
}

export interface FunctionKeyPlan {
  /** The one entry a single-entry action may act on — `null` both when
   * nothing is selected and when more than one entry is. */
  single: PanelEntry | null;
  /** More than one entry marked — what disables F3/F4/Shift+F6/F9 and drives
   * their "select only one" reason text. */
  multipleSelected: boolean;
  /** At least one entry marked — what F5/F6/F8 gate on instead. */
  hasSelection: boolean;
  f3: SingleKeyPlan;
  f4: SingleKeyPlan;
  /** Rename in place. Shift+F6 since phase 2b — F6 alone is Move now, Total
   * Commander's own semantics. */
  shiftF6: SingleKeyPlan;
  f9: SingleKeyPlan;
}

/**
 * The four single-entry keys' enabled state and target, from the pane's
 * entries and selection plus the three runtime facts (volume, write
 * capability, busy) none of `@/lib/selection` knows about.
 *
 * A directory cannot be viewed or edited (F3, F4 refuse one even when it is
 * the sole selection); Shift+F6 and F9 do not care — renaming and setting
 * attributes both work on a directory.
 */
export function planFunctionKeys(input: FunctionKeyPlanInput): FunctionKeyPlan {
  const single = singleSelected(input.entries, input.selected);
  const multipleSelected = input.selected.size > 1;
  const hasSelection = input.selected.size > 0;
  const notBusy = !input.busy;

  return {
    single,
    multipleSelected,
    hasSelection,
    f3: {
      enabled: Boolean(single && !single.is_dir && input.inVolume),
      target: single,
    },
    f4: {
      enabled: Boolean(single && !single.is_dir && input.canWrite && notBusy),
      target: single,
    },
    shiftF6: {
      enabled: Boolean(single) && input.canWrite && notBusy,
      target: single,
    },
    f9: {
      enabled: Boolean(single) && input.inVolume,
      target: single,
    },
  };
}
