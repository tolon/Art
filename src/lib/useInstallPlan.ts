// The OS Builder's plan, computed in one place for whichever tab is showing
// it.
//
// **Why a hook and not two copies.** The four-tab rewrite (design § 3.2)
// splits one screen into four: tab 1 keeps the plan, the refusals and the
// Run button until round 4, and tab 2 draws the release's own parts as a
// tick list. Both need the same five answers — the release's media layers,
// its component catalogue, the base plan, the effective plan, and what the
// switched-on layering components would replace — and each of those costs a
// round trip that opens real install media. Two components computing them
// separately would be two answers to one question, which is the defect this
// project keeps meeting under other names. So the computation moved out of
// `OsInstall.tsx` whole, into this hook, and the tabs read it.
//
// **ART-290 — the ticks are the build session's own value now.** The
// component selection lived in two panel-owned keys,
// `osinstall.chosen.<release>` and `osinstall.excludedConditional.<release>`.
// `buildSession.components.<release>` was added with `seededComponents` to
// migrate them and *nothing ever wrote it back*, so the session carried a
// one-time copy that went stale the moment anybody ticked a box. This hook
// takes `components` and `setComponents` from `useBuildSession` instead: the
// legacy keys are still read once by the seed, never written again and never
// deleted, so nobody's remembered selection is lost.
//
// Three rules travel with the effects below, and they are the reason the
// bodies moved verbatim rather than being rewritten:
//
//   - **ART-119.** With nothing excluded the two plan requests are
//     *identical*, so asking twice planned the same media twice and threw
//     one answer away. One call, one answer, given to both.
//   - **ART-178/ART-195.** A dependency rebuilt per render drives an effect
//     that starts disk work into a loop — one session reached 2,149 preview
//     jobs, five of them inside two seconds, each walking a 468 MB ISO. So
//     the layers' folders reach the plan effect as a derived *string*
//     (`layerFoldersKey`), and `plannedFolders` is memoized on the material
//     list rather than rebuilt.
//   - **ART-089.** Nothing here writes a tick set against a catalogue that
//     has not arrived: `sanitizeChosen` and `pruneStaleExclusions` would drop
//     *everything* against an empty list and persist the drop, which is a
//     setting changing without the user changing it.
//
// Read-only throughout (§92's PREVIEW): every command called here parses
// shipped JSON or reads media, and none of them writes a byte.

import { useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";

import { errorText } from "@/lib/errorText";
import { foldersForPlan, type ComponentChoice, type MaterialChoice } from "@/lib/buildSession";
import { useLayers } from "@/lib/useLayers";
import {
  osinstallComponentCollisions,
  osinstallComponents,
  osinstallPlan,
  pruneStaleExclusions,
  sanitizeChosen,
  type ComponentDef,
  type ComponentPreview,
  type InstallLayer,
  type InstallPlan,
  type InstallRelease,
  type InstallRequest,
  type PlanResult,
  type ScanCachePolicy,
} from "@/lib/osinstall";

export interface InstallPlanInputs {
  release: InstallRelease;
  /** The build's own folder list — `session.material`. */
  material: MaterialChoice;
  /** `""` means "leave it to the ROM's `usa`" (ART-226), and is sent as
   *  `null` rather than as an empty string the Rust side would have to trim. */
  keymap: string;
  rom: string | null;
  destination: string | null;
  /** ART-194 — whether a medium's listing may come from an earlier scan. */
  reuseScan: boolean;
  /** Bumped by "Scan again" to make the plan run once more against discs
   *  whose remembered listings have just been dropped. */
  rescanNonce: number;
  /** `session.components` — the ticks and the confirmed exclusions. */
  components: ComponentChoice;
  /** `useBuildSession`'s own setter. The hook writes through it in exactly
   *  two places, both of them a stale id clearing itself (ART-290). */
  setComponents: (change: Partial<ComponentChoice>) => void;
  /**
   * Whether to ask `osinstall_component_collisions` at all. Defaults to true,
   * which is every caller that draws the *what would this replace* fold.
   *
   * Tab 4 passes `false` in update mode (round 4 task 4, fix round 1's C1):
   * the preview partitions what the release's own **parts** would place, and
   * with the destination already a tree there is no tree phase, so those
   * parts are not placed at all. Asking would be work for an answer that may
   * not be shown — and it *was* shown, which is the defect this closes.
   */
  previewCollisions?: boolean;
}

export interface InstallPlanState {
  layers: InstallLayer[];
  /** Whether `layers` is this release's own answer rather than the previous
   *  one's, or none at all (ART-256) — a primitive, so it is a stable effect
   *  dependency. */
  plannedFolders: ReturnType<typeof foldersForPlan>;
  layerFoldersKey: string;
  /** `null` until the catalogue has loaded **for this release** — never `[]`,
   *  which is a different state. */
  catalogue: ComponentDef[] | null;
  componentsError: boolean;
  basePlanResult: PlanResult | null;
  effectivePlanResult: PlanResult | null;
  basePlan: InstallPlan | null;
  effectivePlan: InstallPlan | null;
  planError: string | null;
  /**
   * The plan-error slot's own setter, for the one writer outside this hook:
   * the "Scan again" button, whose `forget_all` failure has always been
   * reported here. It is the same slot, so the next plan answer clears it —
   * exactly as it did while the state lived in the screen.
   */
  setPlanError: (message: string | null) => void;
  layeringOn: string[];
  componentPreview: ComponentPreview | null;
  componentPreviewError: string | null;
  /** Bumps on every plan answer (success or failure). A consumer that holds a
   *  confirmation about the plan on screen resets it on this. */
  planVersion: number;
}

export function useInstallPlan(inputs: InstallPlanInputs): InstallPlanState {
  const {
    release,
    material,
    keymap,
    rom,
    destination,
    reuseScan,
    rescanNonce,
    components,
    setComponents,
    previewCollisions = true,
  } = inputs;
  const { t } = useTranslation();
  const chosen = components.chosen;
  const excludedConditional = components.excludedConditional;

  /**
   * The media layers the chosen release's own recipe declares, in the
   * recipe's own order — **one labelled folder question per layer**, rather
   * than one folder plus a bag of extra ones the user has to guess the
   * meaning of.
   *
   * Fetched whenever the release changes; **empty is the unlayered answer**
   * (every shipped recipe until AmigaOS 3.2.2's own two-layer one), and it is
   * what makes "an unlayered release renders exactly what it renders today"
   * true rather than accidental — nothing below ever branches on `release`
   * itself, only on whether this array is empty.
   *
   * **`useLayers`, not a copy of its effect** (round 4 task 5's fix round).
   * (round 4 task 5's fix round). `FilesTab` needs the layers and nothing
   * else of this hook — the folder column is drawn from them — so the effect
   * moved to `src/lib/useLayers.ts` and this hook calls it. Two
   * implementations of *which release is this the answer for* would be two
   * answers to that question, and `layersKnown` (ART-256) is that answer.
   *
   * **`layersKnown` is not re-exported from here**, and the mutation pass is
   * why: putting `layersKnown: true` back in this hook killed no test,
   * because `FilesTab` was its only reader and reads `useLayers` directly
   * now. A field nothing consults is not a guard that needs strengthening —
   * it is output that has to go, or the next reader will take its presence
   * for a promise this hook keeps. A tab that needs it calls `useLayers`.
   */
  const { layers } = useLayers(release);

  /**
   * **What the request carries, built from the one list** — `foldersForPlan`,
   * which is also what the step's `unusedForPlan` line is read from.
   *
   * A layer's folder used to be its own remembered key
   * (`osinstall.mediaFolder.<layerId>.<release>`) and the flat field another;
   * both are now entries in `material.folders`, an entry's `layer` being the
   * tag that says which labelled question it answers. `seededMaterial`
   * migrates each of those keys once and they are never written again.
   */
  const plannedFolders = useMemo(() => foldersForPlan(material, layers), [material, layers]);

  /** A stable, primitive dependency for the layers' own folders — see the
   *  plan effect below. Built fresh every render, but as a *string*: unlike
   *  an object or array, two equal strings are the same value to React's own
   *  dependency comparison, so this needs no `useStabilised`-style memo the
   *  way an array or object read off the remembered bag would (ART-178). */
  const layerFoldersKey = layers
    .map((l) => `${l.id}=${plannedFolders.mediaFolders[l.id] ?? ""}`)
    .join("|");

  /**
   * The chosen release's own component catalogue, loaded from its recipe —
   * **carrying the release it describes**, never a bare list.
   *
   * `null` means "not loaded yet", and it is a distinct state from `[]` on
   * purpose: everything that filters a remembered id against this list —
   * `sanitizeChosen`, `pruneStaleExclusions` — would drop *everything*
   * against an empty list and persist the drop, which is a setting changing
   * without the user changing it (ART-089's shape, from the other side).
   *
   * The `release` field is the same guard against a subtler version of the
   * same thing, and it is not hypothetical — a test caught it: for one render
   * after the picker changes, `release` is already the new one (so `chosen`
   * is read from the new release's remembered key) while this state still
   * holds the *old* release's catalogue. Sanitizing the one against the other
   * writes an empty list over a selection the user never touched. So nothing
   * below reads `catalogueState` directly; everything reads `catalogue`,
   * which is `null` until the two agree.
   */
  const [catalogueState, setCatalogueState] = useState<{
    release: string;
    list: ComponentDef[];
  } | null>(null);
  const [componentsError, setComponentsError] = useState(false);
  const catalogue = catalogueState?.release === release ? catalogueState.list : null;

  // The checklist is the chosen release's recipe, fetched when the release
  // changes. `setCatalogueState(null)` first, so a switch shows "loading"
  // rather than the previous release's components for as long as the round
  // trip takes — a stale checklist is the exact defect this replaces, and
  // showing it for 20 ms is showing it.
  //
  // The `cancelled` flag is what keeps a slow load for a release the user has
  // since switched away from out of the state it no longer describes.
  useEffect(() => {
    let cancelled = false;
    setCatalogueState(null);
    setComponentsError(false);
    osinstallComponents(release)
      .then((list) => {
        if (!cancelled) setCatalogueState({ release, list });
      })
      .catch(() => {
        if (!cancelled) setComponentsError(true);
      });
    return () => {
      cancelled = true;
    };
  }, [release]);

  /**
   * Two plans, requested identically except for `excluded` — both read-only
   * previews (§92), both recomputed live on every change, neither an
   * external-tool cost the way the preload screen's plan is.
   *
   * `basePlan` always asks with `excluded: []`. It exists purely to reason
   * about a conditional component's *true* state — whether its own
   * `Condition` is satisfied at all — because a plan requested *with* a
   * component excluded never carries it in `componentsOn` (the engine skips
   * it entirely), which would make "is this condition-satisfied" and "is
   * this excluded" indistinguishable from the one plan a screen that only
   * asked once would have.
   *
   * `effectivePlan` asks with the real `excludedConditional`. It is what
   * the file list shows, what `osinstallBlocker` reads, and — unmodified —
   * what `osinstallApply` receives. Never the other way around: applying
   * `basePlan` would silently undo every exclusion the user confirmed.
   */
  const [basePlanResult, setBasePlanResult] = useState<PlanResult | null>(null);
  const [effectivePlanResult, setEffectivePlanResult] = useState<PlanResult | null>(null);
  const [planError, setPlanError] = useState<string | null>(null);
  /**
   * What the switched-on layering components would replace, file by file
   * (ART-175).
   *
   * **Why this needs its own preview at all.** `plan::detect_collisions`
   * already *refuses* an undeclared overlap at plan time, so nothing is
   * unguarded — but a component that declares one is allowed to stand on
   * another's file silently, and AmigaOS 3.9's `workbench-39` is exactly
   * that: the component that turns a 3.5 tree into a 3.9 one, by replacing
   * files `workbench-base` placed. §92's PREVIEW is the informed-consent
   * half, and it was the half nobody built. `collide::preview` has been able
   * to answer since ART-170 and nothing asked it.
   *
   * `null` means "not asked" (nothing layering is switched on, or the plan is
   * not ready); a value with an empty `reports` means "asked, and nothing is
   * in the way", which is a different sentence.
   */
  const [componentPreview, setComponentPreview] = useState<ComponentPreview | null>(null);
  const [componentPreviewError, setComponentPreviewError] = useState<string | null>(null);
  /**
   * One answer, counted (§54's own shape one level down). The screen holds a
   * confirmation *about a plan* — the install checkbox and a half-asked
   * exclusion — and both are stale the moment a new answer lands, success or
   * failure alike. A counter rather than the plan's own identity, so a
   * consumer resets on the event rather than having to compare two objects.
   */
  const [planVersion, setPlanVersion] = useState(0);

  /**
   * **ART-210 — nothing computed for one release survives a switch to
   * another** (this hook's own third of it; the screen clears the rest).
   *
   * The owner, driving the install screen: *"3.2 kurayım diyorsun, 3.9'un
   * seçenekleri, hataları vb ekranda duruyor asla değişmiyor."* A plan error
   * and a collision preview are answers about the release that was chosen
   * when they were computed, and the plan below does not clear them on its
   * own: it re-plans, which leaves the previous release's error on screen for
   * the whole of a round trip that reads real media.
   */
  useEffect(() => {
    setPlanError(null);
    setComponentPreview(null);
    setComponentPreviewError(null);
  }, [release]);

  // The two plans: read-only (§92's PREVIEW), so both are recomputed live
  // whenever the request changes rather than behind a separate "Preview"
  // button — there is no external tool cost here the way there is on the
  // preload screen, and this is also what lets the component list explain a
  // conditional tick immediately, not only after a manual preview step.
  useEffect(() => {
    // Whether the request has any folder at all to read. A layered release
    // is gated on **any** tagged folder, the same way an unlayered one is
    // gated on the list holding anything. Partial is fine: `plan()` reads a
    // layer nobody has tagged yet as that layer's own components reporting
    // media-missing (Task 3), not as a reason to refuse planning altogether.
    const hasMedia =
      layers.length > 0
        ? Object.keys(plannedFolders.mediaFolders).length > 0
        : !!plannedFolders.mediaFolder;
    if (!hasMedia) {
      setBasePlanResult(null);
      setEffectivePlanResult(null);
      setPlanError(null);
      return;
    }
    let cancelled = false;

    // Only sanitize against a catalogue that has actually arrived. Against
    // `null` the remembered ids are passed through untouched and nothing is
    // written back: dropping every id because a fetch has not landed yet
    // would be ART-089 exactly — a setting changing without the user
    // changing it.
    const sanitized = catalogue ? sanitizeChosen(catalogue, chosen) : chosen;
    if (sanitized.length !== chosen.length) {
      // A stale remembered id — one this release's recipe does not hold, or
      // holds as Coming Later — actually clears, rather than being filtered
      // again on every read. Safe to persist now that the key is per
      // release: this can only ever drop an id from the release it was
      // chosen for, never from the one the user just switched away from.
      setComponents({ chosen: sanitized });
    }

    const shared = {
      // **All three from `foldersForPlan`**, which is the one place the
      // material list becomes a request (design § 3.1). A layered release
      // reads `mediaFolders` alone and gets an empty flat folder and no
      // extras; an unlayered one gets the list's first folder plus the rest.
      // Two hand-written branches here and one in `src/lib` is how the folder
      // list the readout resolves and the folders the planner reads would
      // drift apart.
      mediaFolder: plannedFolders.mediaFolder,
      extraMediaFolders: plannedFolders.extraMediaFolders,
      mediaFolders: plannedFolders.mediaFolders,
      // ART-226: empty means "leave it on the ROM's usa", so it is sent as
      // null rather than as an empty string the Rust side would have to trim.
      keymap: keymap.trim() ? keymap : null,
      rom,
      chosen: sanitized,
      destination: destination ?? "",
      release,
      // ART-194. Sent every time rather than only when off, so the request
      // says what it asked for and two plans that differ in this cannot look
      // identical in a log.
      scanCache: (reuseScan ? "reuse" : "ignore") as ScanCachePolicy,
    };
    const baseRequest: InstallRequest = { ...shared, excluded: [] };
    const effectiveRequest: InstallRequest = { ...shared, excluded: excludedConditional };

    // ART-119 (#1). With nothing excluded — which is every run until the
    // user confirms an override, and most runs after — the two requests are
    // *identical*, so the second call planned the same media twice and threw
    // one answer away. `plan()` opens and walks every switched-on component's
    // disc image, so that is real work on every keystroke in the media
    // fields.
    //
    // One call, one answer, given to both. Safe because nothing here mutates
    // a `PlanResult` — every reader takes `.plan`, `.items`, `.refusals` or
    // `.componentsOn`, and `osinstallApply` receives the effective plan
    // unmodified — and because nothing compares the two by identity. It also
    // does not change *when* a round-trip happens: the remaining call is
    // made in the same effect, in the same tick, on exactly the same
    // dependency change as before. The moment anything is excluded, both
    // requests are made again, because then they genuinely differ.
    const planning = excludedConditional.length === 0
      ? osinstallPlan(baseRequest).then((both): [PlanResult, PlanResult] => [both, both])
      : Promise.all([osinstallPlan(baseRequest), osinstallPlan(effectiveRequest)]);

    planning
      .then(([base, effective]) => {
        if (cancelled) return;
        setBasePlanResult(base);
        setEffectivePlanResult(effective);
        setPlanError(null);
        // Confirming an exclusion or the whole install describes the plan
        // that was on screen at the time; once the request changes, both
        // are stale — the same rule the preload screen's own
        // fingerprint/lastPlanned pair enforces, simplified here because
        // the plan is always fresh rather than sometimes stale. The consumer
        // holds those confirmations, so what it gets is the count.
        setPlanVersion((v) => v + 1);
        if (base.outcome === "planned" && catalogue) {
          const pruned = pruneStaleExclusions(catalogue, base.plan, sanitized, excludedConditional);
          if (pruned.length !== excludedConditional.length) {
            setComponents({ excludedConditional: pruned });
          }
        }
      })
      .catch((e) => {
        if (cancelled) return;
        setBasePlanResult(null);
        setEffectivePlanResult(null);
        setPlanError(errorText(t, e));
        // A refusal is an answer too: a confirmation held about the plan that
        // was on screen describes a plan there no longer is.
        setPlanVersion((v) => v + 1);
      });
    return () => {
      cancelled = true;
    };
    // `setComponents` is deliberately out of the dependency list. It is *not*
    // a stable identity — `useRememberedShape` rebuilds its setter when its
    // key changes, and the session's component key is per release — but
    // listing it would re-plan on a release switch twice: once for `release`,
    // once for the setter that changed with it.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  // `extraMediaFolders` is in here for the reason the test that found it
  // names: without it, adding a folder changed what ART *would* read and
  // left the screen showing the refusals from before it was added — a
  // preview describing a plan nobody asked for any more. `chosen` and
  // `excludedConditional` are arrays in this list already, so the identity
  // question ART-178/ART-195 raise is one `useRememberedShape` has answered.
  //
  // `layers` (a fresh array from `layersFor` whenever `release` changes) and
  // `layerFoldersKey` (a derived *string* — see its own doc comment) stand in
  // for a layered release's own folders here, the same role `mediaFolder`
  // and `extraMediaFolders` play for an unlayered one.
  }, [
    plannedFolders,
    layers,
    layerFoldersKey,
    keymap,
    rom,
    chosen,
    destination,
    excludedConditional,
    release,
    catalogue,
    reuseScan,
    rescanNonce,
  ]);

  /**
   * Ask what the layering components would replace, whenever the plan
   * changes (ART-175).
   *
   * **Only the components that can be in another's way**, never all of them:
   * the preview reads every file the components it is asked about would
   * place, off real install media, and asking about all twenty-six would
   * mean reading a whole AmigaOS install to answer a question about a few
   * dozen files. `ComponentDef.overrides` is what the recipe declares, and
   * `src/lib/osinstall.test.ts` pins which five components carry one.
   *
   * Read-only (§92's PREVIEW) and recomputed rather than remembered: a
   * preview of a plan the user has since changed is worse than none.
   */
  const layeringOn = useMemo(() => {
    const plan = effectivePlanResult?.outcome === "planned" ? effectivePlanResult.plan : null;
    if (!plan || !catalogue) return [];
    return catalogue
      .filter((def) => def.overrides.length > 0 && plan.componentsOn.includes(def.id))
      .map((def) => def.id);
  }, [effectivePlanResult, catalogue]);

  useEffect(() => {
    const plan = effectivePlanResult?.outcome === "planned" ? effectivePlanResult.plan : null;
    // `previewCollisions` first: a caller that does not want this answer must
    // not have it asked for, and must not be handed a stale one either.
    if (!previewCollisions || !plan || layeringOn.length === 0) {
      setComponentPreview(null);
      setComponentPreviewError(null);
      return;
    }
    let cancelled = false;
    osinstallComponentCollisions(plan, layeringOn)
      .then((preview) => {
        if (cancelled) return;
        setComponentPreview(preview);
        setComponentPreviewError(null);
      })
      .catch((e) => {
        if (cancelled) return;
        setComponentPreview(null);
        // Named rather than swallowed: a preview that could not be produced
        // must not look like a preview that found nothing (§89).
        setComponentPreviewError(errorText(t, e));
      });
    return () => {
      cancelled = true;
    };
  }, [effectivePlanResult, layeringOn, previewCollisions]);

  const basePlan = basePlanResult?.outcome === "planned" ? basePlanResult.plan : null;
  const effectivePlan = effectivePlanResult?.outcome === "planned" ? effectivePlanResult.plan : null;

  return {
    layers,
    plannedFolders,
    layerFoldersKey,
    catalogue,
    componentsError,
    basePlanResult,
    effectivePlanResult,
    basePlan,
    effectivePlan,
    planError,
    setPlanError,
    layeringOn,
    componentPreview,
    componentPreviewError,
    planVersion,
  };
}
