// Building an AmigaOS distribution tree from the user's own install disks
// (SD-2 · G5). This is the screen for the engine `src-tauri/src/core/osinstall`
// and `src/lib/osinstall.ts` already built and nobody could reach: media
// folder → ROM → components → the file list → confirm → job → report, plus a
// secondary Verify section for checking a tree once it has been copied onto a
// real Amiga volume by the "Prepare Amiga volumes" screen.
//
// Three rules shape it, named directly in the brief:
//
//   - **The checklist is the chosen release's own recipe.** `osinstallComponents`
//     loads it whenever the release changes; nothing here is hardcoded. It
//     used to be: a literal copy of the AmigaOS 3.2 catalogue, rendered
//     whatever the picker said, so choosing 3.9 showed 26 components for a
//     one-component recipe and labelled 3.9's base component `Workbench3.2`.
//     A whole-branch review called that the worst finding on its list, and it
//     was right — showing one operating system's parts while installing
//     another's is §89 on the screen itself.
//   - **Every conditional tick states its reason.** A tick ART decided and
//     did not explain is a tick the user cannot argue with. The loaded
//     catalogue carries which components are `required` and which carry a
//     `conditionMajor` (today, only 3.2's `modules-a1200`, "below Kickstart
//     V47"), and `conditionalReason` decides, from primitives, one of exactly
//     four reasons for every conditional row — never none, which is the shape
//     a review found this screen could render in its first round (see below).
//   - **Turning a condition-satisfied component off is a confirmation, not a
//     refusal.** It is the user's machine. **This is now the engine's own
//     job, not this screen's.** The first version filtered a returned
//     `InstallPlan` in the browser — safe-looking, because `osinstallApply`
//     takes the exact plan it is handed, but wrong two ways a review found:
//     the filtered plan's `mediaPaths` still promised the excluded
//     component's own media (so `apply()`'s manifest recorded a medium not
//     one byte of which was installed, and a moved disk could fail a build
//     over a component the user turned off), and a `MediaMissing` refusal
//     `plan()` itself raised for the excluded component was never touched
//     at all — `osinstallBlocker` reads `refusals`, so "turning Modules off"
//     stayed a refusal, just a politer one. `core/osinstall/plan.rs`'s
//     `InstallRequest.excluded` fixes both at the source: subtracted inside
//     `resolve_components_on`, before the media-resolution loop, so an
//     excluded component's media is never opened, never recorded, and never
//     a source of a refusal. This screen keeps **two** plans for exactly
//     this reason — see the module doc on `basePlan`/`effectivePlan` below.
//   - **The file list is read-only.** Unlike G11's layout preview, where
//     retargeting a row *is* the feature, every destination here comes from
//     a recipe checked against real media — a hand-moved row would make
//     `distribution.json` describe a release that was never actually built.
//     Components are the only edit; the list itself has no controls.
//
// Remembered between runs, through `@/lib/remembered`'s guards: the media
// folder, the ROM, the destination, the release, and the component selection
// (`chosen` plus `excludedConditional`) — the last two **per release**, since
// a component id means nothing outside the recipe that declares it and
// switching release must not destroy the choices made for the other one. Nothing here arms a destructive action the
// way the preload screen's partition picks do — building a distribution
// tree only ever writes a *new* folder and refuses one that already exists
// (`SAFE_CREATE`), so there is nothing of that shape to protect against by
// leaving a choice unremembered.
//
// Every function doing anything other than rendering — the exclusion state
// machine, the reasoning behind a conditional tick, the Verify section's
// two small parsers — lives in `@/lib/osinstall` and is unit-tested there
// (`src/lib/osinstall.test.ts`). A review's own diagnosis of how the first
// Critical shipped: "no test can reach [it], because it lives inside the
// component." This screen is now the thin rendering layer that diagnosis
// asked for.

import { useEffect, useMemo, useRef, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { useTranslation } from "react-i18next";

// ART-060: a Rust sentence ART recognises comes back in the user's own
// language; anything else is Rust's English verbatim, exactly as before.
import { errorText } from "@/lib/errorText";

import { hostParentDir } from "@/lib/hostPath";
import {
  collisionGroupHeadingKey,
  collisionPhrase,
  componentDef,
  componentLabel,
  confirmComponentOff,
  conditionalReason,
  conditionalReasonText,
  conditionalToggleAction,
  hasRomUnknownRefusal,
  INSTALL_RELEASES,
  isForcedOnByCondition,
  isInstallRelease,
  groupCollisionsForPreview,
  isVerified,
  onOsInstallResult,
  osinstallApply,
  osinstallComponentCollisions,
  osinstallComponents,
  osinstallBlocker,
  osinstallDestinationTaken,
  osinstallPlan,
  osinstallRescanMedia,
  osinstallReleaseForMedia,
  osinstallScanMedia,
  osinstallVerify,
  parseOptionalSlot,
  parsePartitionIndex,
  pruneStaleExclusions,
  refusalPhrase,
  wrongMediaFolder,
  rememberedComponentKey,
  type ScanCachePolicy,
  sanitizeChosen,
  toggleChosen,
  withoutExcluded,
  type ComponentDef,
  type ComponentPreview,
  type InstallPlan,
  type InstallRelease,
  type InstallRequest,
  type MediaScanResult,
  type OsInstallResult,
  type PlanResult,
  type VerifyReport,
} from "@/lib/osinstall";
import { pistormIdentifyRom, type RomInfo } from "@/lib/pistorm";
import { isFlag, isTextList, isTextOrNothing } from "@/lib/remembered";
import { useRemembered } from "@/lib/useRemembered";
import { useBuildSession } from "@/lib/useBuildSession";
import { fraction, onJobProgress, subscribeSafely, type JobProgress } from "@/lib/jobs";
import { Field } from "@/components/osbuilder/Field";
import { PackagePanel } from "@/components/osbuilder/PackagePanel";
import { AmigaInstallPanel } from "@/components/osbuilder/AmigaInstallPanel";

const GIB = 1024 * 1024 * 1024;

/** A size the way the rest of the OS Builder prints one. */
function size(bytes: number): string {
  if (bytes >= GIB) return `${Math.round((bytes / GIB) * 100) / 100} GB`;
  return `${Math.round((bytes / (1024 * 1024)) * 10) / 10} MB`;
}

/** Plan items, grouped by component, in the order the plan itself already
 *  lists them (recipe order — `plan()` walks `recipe.components` in
 *  declaration order, so this needs no sort of its own). */
function groupByComponent(plan: InstallPlan): { component: string; items: InstallPlan["items"] }[] {
  const order: string[] = [];
  const byComponent = new Map<string, InstallPlan["items"]>();
  for (const item of plan.items) {
    if (!byComponent.has(item.component)) {
      byComponent.set(item.component, []);
      order.push(item.component);
    }
    byComponent.get(item.component)!.push(item);
  }
  return order.map((component) => ({ component, items: byComponent.get(component)! }));
}

/**
 * One arrival of a disc dropped on the panel. `arrivalKey` is
 * `location.key` from `OsBuilder` — unique per navigation, so a second drop
 * of the exact same `path` is still a distinct value the effect below can
 * react to. `path` alone cannot do that: two drops of the same file produce
 * value-equal strings, which a dependency array treats as no change.
 */
export type DroppedMedia = { path: string; arrivalKey: string } | null;

/**
 * How far the install has got, beside the button that started it.
 *
 * Three things, and each is there because its absence was the complaint:
 * a percentage, the file count behind it, and the file landing right now.
 * "Installing…" on its own says a job exists, not that it is moving — a
 * 588-file tree took twenty seconds with nothing on screen changing, and a
 * frozen application and a working one looked identical.
 *
 * A job with no total gets a fixed sliver and no percentage rather than a
 * bar that pretends to know how far along it is — the same choice `JobBar`
 * makes, for the same reason (§89: ART does not state what it has not
 * measured).
 */
function InstallProgress({ progress }: { progress: JobProgress | null }) {
  const { t } = useTranslation();

  const pct = progress ? fraction(progress) : null;
  return (
    <div style={{ flex: 1, minWidth: 160, maxWidth: 420 }}>
      <div style={{ display: "flex", justifyContent: "space-between", fontSize: 11, gap: 8 }}>
        <span>
          {pct === null
            ? t("osinstall.run.progress.starting")
            : t("osinstall.run.progress.percent", {
                percent: Math.round(pct * 100),
                done: progress?.done ?? 0,
                total: progress?.total ?? 0,
              })}
        </span>
      </div>
      <div
        aria-hidden
        style={{
          height: 4,
          marginTop: 4,
          borderRadius: 2,
          background: "var(--border)",
          overflow: "hidden",
        }}
      >
        <div
          style={{
            height: "100%",
            width: pct === null ? "25%" : `${pct * 100}%`,
            background: "var(--accent)",
            transition: "width 120ms linear",
          }}
        />
      </div>
      {progress?.message && (
        <div className="faint" style={{ fontSize: 11, marginTop: 2, wordBreak: "break-all" }}>
          {progress.message}
        </div>
      )}
    </div>
  );
}

export function OsInstall({ droppedMedia = null }: { droppedMedia?: DroppedMedia }) {
  const { t } = useTranslation();

  // --- what the user chose, remembered -------------------------------------
  /**
   * Which shipped recipe to plan from — **the build session's own value**,
   * not a second one of this screen's (ART-211).
   *
   * `useBuildSession` has held `buildSession.release` and offered a
   * `setRelease` since ART-197, and nothing called it: this screen owned
   * `osinstall.release` instead, so the release the user picked and the
   * release the session carried were two variables joined by nothing but a
   * one-time legacy read that happens before the picker is ever touched.
   * The OS Builder's step routes read the session, which is why they could
   * not scope anything by release at all — the second layer of the owner's
   * model (a base, the updates on that base, the program sets on those) had
   * no way to ask what the base was.
   *
   * That is ART-197's own shape one field over, and CLAUDE.md states the
   * rule it broke: the build's own values live in one place, and adding a
   * field means adding it there rather than another `useRemembered` in a
   * panel. The legacy key is still read once by the session's own fallback,
   * so nobody's remembered choice is lost.
   *
   * **First, because the folders below are keyed on it** (ART-207).
   */
  const { session, setTree, setPackages, setRelease, setRom: setSessionRom } = useBuildSession();
  const release = session.release;

  /**
   * The folder holding this release's install media — remembered **per
   * release** (ART-207), for the reason `rememberedComponentKey` already
   * exists: a folder means something only inside the recipe that reads it.
   *
   * The owner chose AmigaOS 3.2 with the folder their 3.9 disc lives in
   * still remembered, and every one of the 3.2 recipe's sixteen components
   * refused `MediaMissing`. Sixteen true sentences that together said "a lot
   * of programs are missing" about a folder that was simply the wrong one.
   * A media folder cannot serve two releases — the owner keeps 3.9's disc in
   * `Amigatolon\iso` and 3.2's ADFs in `Amigatolon\paketler` — so carrying
   * one into the other can only ever produce that screen.
   */
  const [mediaFolder, setMediaFolder] = useRemembered<string | null>(
    rememberedComponentKey("osinstall.mediaFolder", release),
    isTextOrNothing,
    null
  );

  // A disc dropped on the panel names a *file*; the scanner takes the folder
  // that holds it, which is also where its sibling discs and ADFs live. The
  // user dropped it, so this is them setting the value — it goes through the
  // remembered setter and is kept, like every other choice on this screen.
  //
  // Do NOT clear `mediaFolder` when `droppedMedia` is null — arriving at this
  // screen without a drop must leave the remembered folder alone (ART-089).
  //
  // Depends on `arrivalKey`, not just `path`: dropping the same disc twice —
  // with the folder changed by hand in between — must set it back, and a
  // dependency array keyed only on the (unchanged) path string would never
  // re-run for that second, identical-looking drop.
  useEffect(() => {
    const folder = droppedMedia ? hostParentDir(droppedMedia.path) : null;
    if (folder) setMediaFolder(folder);
  }, [droppedMedia?.path, droppedMedia?.arrivalKey, setMediaFolder]);
  /**
   * **Not** per release, and that is the decision rather than an oversight
   * (ART-207). A Kickstart is a property of the machine the tree is being
   * built for, not of the release being installed: the owner's one licensed
   * A1200 ROM is the right answer for 3.2 and for 3.9 alike, and making them
   * pick it again per release would be a choice resetting itself for no
   * reason anybody could state.
   */
  /**
   * **One Kickstart for the build** (ART-197's fourth row, wave 2).
   *
   * This panel kept its own remembered key until 2026-08-23, so a user chose
   * the same ROM here, on the install step and on the card step. They are one
   * question wearing three labels — the ROM the tree is paired against, the
   * ROM the emulator boots, and the ROM written onto the card — and a build
   * where they differ is the mismatch G9's pairing check exists to catch.
   *
   * Changing it here changes it for the build, which the hint says out loud:
   * a carry the user cannot see is the same defect as one that never
   * happened (ART-197's own words).
   */
  const romPath = session.rom.path;
  const setRomPath = setSessionRom;
  /**
   * Where the tree goes — per release, like the media folder above and for
   * the same reason (ART-207). `E:\…\os39\art3` is a fine destination for a
   * 3.9 build and a misleading one for a 3.2 build; a folder named for one
   * release holding another release's tree is the quiet kind of wrongness
   * `distribution.json` exists to make impossible.
   */
  const [destination, setDestination] = useRemembered<string | null>(
    rememberedComponentKey("osinstall.destination", release),
    isTextOrNothing,
    null
  );
  /**
   * The components the user ticked, remembered **per release** — see
   * `rememberedComponentKey`. A component id means something only inside the
   * recipe that declares it (both shipped recipes carry a `workbench-base`,
   * for different media), so one shared set would either send 3.2's ids into
   * a 3.9 plan or destroy them the moment the user looked at 3.9. Switching
   * release and switching back now finds the earlier selection untouched.
   */
  const [chosen, setChosen] = useRemembered<string[]>(
    rememberedComponentKey("osinstall.chosen", release),
    isTextList,
    []
  );
  /**
   * Condition-satisfied components the user has explicitly, with
   * confirmation, turned off — sent to the engine as
   * `InstallRequest.excluded`. Remembered, unlike the preload screen's
   * partition picks: nothing here is destructive by itself (see the module
   * doc comment on the remembered set as a whole).
   */
  const [excludedConditional, setExcludedConditional] = useRemembered<string[]>(
    rememberedComponentKey("osinstall.excludedConditional", release),
    isTextList,
    []
  );

  /**
   * Whether ART may reuse a medium's listing from an earlier scan (ART-194).
   *
   * Remembered, and guarded, like every other choice on this screen: it is
   * something the user decided, so it comes back tomorrow. `isFlag` is the
   * guard, so a hand-edited or older `settings.json` holding anything else
   * falls back to `true` rather than putting a bad value on screen.
   *
   * `true` by default — cached is the ordinary path, and no screen should have
   * to explain why it is not rescanning a disc that has not changed.
   */
  const [reuseScan, setReuseScan] = useRemembered<boolean>(
    "osinstall.reuseScan",
    isFlag,
    true
  );
  /** How many listings the last "Scan again" dropped, or `null` when the user
   *  has not asked this session. Session-only: it describes an action just
   *  taken, not a choice to remember. */
  const [rescanned, setRescanned] = useState<number | null>(null);
  /**
   * Bumped by "Scan again" to make the plan effect run once more.
   *
   * A counter and not `setMediaFolder(mediaFolder)`: setting a state to the
   * value it already holds is a no-op React bails out of, so the effect would
   * never fire and the button would do nothing visible — which is precisely
   * the "control that silently ignores the user" this round has been about.
   */
  const [rescanNonce, setRescanNonce] = useState(0);

  /**
   * The tree, the folder holding update archives and the package ids ticked
   * now live in the **session** (`@/lib/buildSession`), not in three keys of
   * this screen's own.
   *
   * **ART-197.** They used to be `osinstall.packages.treeRoot` and its two
   * neighbours, while the folder this screen *wrote into* was
   * `osinstall.destination` — two variables for one folder, joined by
   * nothing. So a user who had just watched ART write 1915 files was asked,
   * immediately below, to locate a "distribution tree": a term naming nothing
   * visible on their own disk. The owner could not answer the field.
   *
   * The comment that stood here objected to filling the tree in from a
   * finished build, on the grounds that overwriting a remembered pick without
   * the user acting is exactly what the remembered-settings rule forbids.
   * **That objection is answered rather than dropped.** Pressing Build *is*
   * the user acting — they chose the folder and asked ART to fill it — and
   * the change is stated on screen (`osinstall.result.carried`) instead of
   * being made silently. A user who wants a different tree still picks one;
   * the picker never goes away.
   */

  const packagesTreeRoot = session.tree.root;
  const packagesFolder = session.packages.folder;
  const packagesChosen = session.packages.chosen;

  // --- what the screen is doing --------------------------------------------
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
   * below reads `components` directly; everything reads `catalogue`, which is
   * `null` until the two agree.
   */
  const [components, setComponents] = useState<{ release: string; list: ComponentDef[] } | null>(
    null
  );
  const [componentsError, setComponentsError] = useState(false);
  const catalogue = components?.release === release ? components.list : null;
  /**
   * One component's row label (ART-224).
   *
   * `media` — the volume the component comes from — is what every row used
   * to show, and for AmigaOS 3.2's sixteen disks it is better than any
   * sentence ART could write. AmigaOS 3.9 is five components off one disc,
   * so every row read `AmigaOS3.9`: three identical labels before ART-159
   * added two more, two of them tick-boxes the user is asked to decide
   * about. A recipe now names its own key where that happens, and the Rust
   * side refuses a recipe that shares a medium without naming one.
   *
   * Resolved here rather than in `src/lib`, which holds no i18next singleton
   * and returns `Phrase`s instead of sentences — a `labelKey` is a whole key
   * with no parameters, so it needs no `Phrase` wrapper, only a `t` call at
   * the place that draws it.
   */
  function label(id: string): string {
    const key = componentDef(catalogue ?? [], id)?.labelKey;
    return key ? t(key) : componentLabel(catalogue ?? [], id);
  }
  const [mediaScan, setMediaScan] = useState<MediaScanResult | null>(null);
  const [rom, setRom] = useState<RomInfo | null>(null);
  const [romError, setRomError] = useState(false);
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
  const [confirmed, setConfirmed] = useState(false);
  /** The one component id currently showing the "this will not boot"
   *  confirmation, or `null`. Only one at a time — a second click elsewhere
   *  replaces it, matching how the preload screen's own single-confirmation
   *  shape works. */
  const [pendingExclusion, setPendingExclusion] = useState<string | null>(null);
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
  const [busy, setBusy] = useState(false);
  /**
   * The install job this screen started, and its latest progress.
   *
   * `apply()` already reports per file — `sink.report(done, total, item.to)`
   * — and that has always reached the webview as `job-progress`. Nothing
   * here listened, so the screen said "Installing…" and nothing else for
   * however long a 588-file tree takes. A ref rather than state for the id:
   * it is read inside the listener and must not re-subscribe on every tick.
   */
  const installJob = useRef<number | null>(null);
  const [progress, setProgress] = useState<JobProgress | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [result, setResult] = useState<OsInstallResult | null>(null);

  // --- the secondary Verify section, session-only (see the module doc) ----
  const [verifyDistRoot, setVerifyDistRoot] = useState<string | null>(null);
  const [verifyImage, setVerifyImage] = useState<string | null>(null);
  const [verifySlotText, setVerifySlotText] = useState("");
  const [verifyIndexText, setVerifyIndexText] = useState("1");
  const [verifying, setVerifying] = useState(false);
  const [verifyReport, setVerifyReport] = useState<VerifyReport | null>(null);
  const [verifyError, setVerifyError] = useState<string | null>(null);

  // The checklist is the chosen release's recipe, fetched when the release
  // changes. `setComponents(null)` first, so a switch shows "loading" rather
  // than the previous release's components for as long as the round trip
  // takes — a stale checklist is the exact defect this replaces, and showing
  // it for 20 ms is showing it.
  //
  // The `cancelled` flag is what keeps a slow load for a release the user has
  // since switched away from out of the state it no longer describes.
  useEffect(() => {
    let cancelled = false;
    setComponents(null);
    setComponentsError(false);
    osinstallComponents(release)
      .then((list) => {
        if (!cancelled) setComponents({ release, list });
      })
      .catch(() => {
        if (!cancelled) setComponentsError(true);
      });
    return () => {
      cancelled = true;
    };
  }, [release]);

  /**
   * **ART-210 — nothing computed for one release survives a switch to
   * another.**
   *
   * The owner, driving this screen: *"3.2 kurayım diyorsun, 3.9'un
   * seçenekleri, hataları vb ekranda duruyor asla değişmiyor."* Only two
   * effects here depended on `release` — the component list above and the
   * plan below. Every other answer on this screen is a `useState` nothing
   * invalidated, so a finished install's report, a failed preview, a plan
   * error and a half-asked confirmation all stayed put, describing an
   * operating system the user had moved away from. The plan updating
   * underneath them made it worse rather than better: one release's plan
   * beside another release's result is a screen contradicting itself.
   *
   * **What is deliberately NOT cleared**, so a later reader does not "fix"
   * it: the ROM (a property of the machine, not the release — ART-207), the
   * media scan (its own effect re-runs, because ART-207 keyed the folder per
   * release and the folder therefore changed too), and the Verify section,
   * which names the tree and image it is talking about on screen and is not
   * about the picker at all.
   *
   * **An honest limit.** This is a list, and a list can be forgotten: a
   * future card added without a line here would sit stale exactly as these
   * did. The structural answer is to remount the body on a `key={release}`,
   * which cannot be forgotten — but that needs `release` owned by a parent,
   * which is the `OsInstall.tsx` split already on the work list. Said here
   * rather than discovered later.
   */
  useEffect(() => {
    setResult(null);
    setError(null);
    setPlanError(null);
    setComponentPreview(null);
    setComponentPreviewError(null);
    setPendingExclusion(null);
    setRescanned(null);
    setConfirmed(false);
  }, [release]);

  // Re-scan whatever folder was remembered, so one since emptied or moved is
  // noticed rather than shown as still holding what it held last run.
  useEffect(() => {
    if (!mediaFolder) {
      setMediaScan(null);
      return;
    }
    let cancelled = false;
    osinstallScanMedia(mediaFolder)
      .then((r) => {
        if (!cancelled) setMediaScan(r);
      })
      .catch(() => {
        if (!cancelled) setMediaScan(null);
      });
    return () => {
      cancelled = true;
    };
  }, [mediaFolder]);

  // Re-identify whatever ROM was remembered, for the same reason.
  useEffect(() => {
    if (!romPath) {
      setRom(null);
      setRomError(false);
      return;
    }
    let cancelled = false;
    pistormIdentifyRom(romPath)
      .then((r) => {
        if (cancelled) return;
        setRom(r);
        setRomError(false);
      })
      .catch(() => {
        if (cancelled) return;
        setRom(null);
        setRomError(true);
      });
    return () => {
      cancelled = true;
    };
  }, [romPath]);

  // The two plans: read-only (§92's PREVIEW), so both are recomputed live
  // whenever the request changes rather than behind a separate "Preview"
  // button — there is no external tool cost here the way there is on the
  // preload screen, and this is also what lets the component list below
  // explain a conditional tick immediately, not only after a manual preview
  // step.
  useEffect(() => {
    if (!mediaFolder) {
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
      setChosen(sanitized);
    }

    const shared = {
      mediaFolder,
      rom: romPath,
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
    // disc image, so that is real work on every keystroke in the media,
    // ROM and destination fields alike.
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
        // the plan is always fresh rather than sometimes stale.
        setConfirmed(false);
        setPendingExclusion(null);
        if (base.outcome === "planned" && catalogue) {
          const pruned = pruneStaleExclusions(catalogue, base.plan, sanitized, excludedConditional);
          if (pruned.length !== excludedConditional.length) {
            setExcludedConditional(pruned);
          }
        }
      })
      .catch((e) => {
        if (cancelled) return;
        setBasePlanResult(null);
        setEffectivePlanResult(null);
        setPlanError(errorText(t, e));
      });
    return () => {
      cancelled = true;
    };
    // `setChosen`/`setExcludedConditional` are deliberately out of the
    // dependency list. They are *not* stable identities — `useRemembered`
    // rebuilds each setter when its key changes, and the component keys are
    // now per release — but listing them would re-plan on a release switch
    // twice: once for `release`, once for the setters that changed with it.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [mediaFolder, romPath, chosen, destination, excludedConditional, release, catalogue, reuseScan, rescanNonce]);

  // `subscribeSafely` (Task 7's own fix round, F7/ART-165): the bare
  // `.then((fn) => { unlisten = fn })` shape this used to have could both
  // leak the real Tauri listener (an unmount before the promise resolved
  // left nothing to call) and surface an unhandled rejection (no IPC bridge
  // to reach, e.g. under test) — see `docs/ISSUES.md`'s ART-165.
  useEffect(() => {
    return subscribeSafely(() =>
      onOsInstallResult((r) => {
        setResult(r);
        setBusy(false);
        setConfirmed(false);
        setVerifyDistRoot(r.destination);
        // ART-197: the tree ART has just written **is** the tree the next
        // steps act on. One variable, so the carry holds by structure rather
        // than by anyone remembering to wire it — and the result card says so
        // (`osinstall.result.carried`), because a carry the user cannot see
        // is the same defect as one that never happened.
        setTree({ root: r.destination, builtHere: true });
        installJob.current = null;
        setProgress(null);
      })
    );
  }, [setTree]);

  // The install's own progress. `job-progress` is application-wide, so every
  // update is checked against this screen's job id — an Aminet download
  // running alongside must not move this bar. A cancelled or failed job never
  // sends an `osinstall-result`, so the terminal states are cleared here too,
  // otherwise the bar would sit at its last percentage for ever.
  useEffect(() => {
    return subscribeSafely(() =>
      onJobProgress((job) => {
        if (job.id !== installJob.current) return;
        setProgress(job);
        if (job.state.state === "running") return;

        installJob.current = null;
        setBusy(false);
        // A finished install announces itself through `osinstall-result`. A
        // **failed or cancelled** one never sends that event, so without this
        // the screen simply stopped saying "Installing…" and said nothing at
        // all — the user's own report: "it did something, but did it install?
        // what did it install? there is no feedback on screen." A job that
        // ended badly has to say so where the button is.
        if (job.state.state === "failed") {
          setError(`${job.state.message} (${job.state.error_code})`);
        } else if (job.state.state === "cancelled") {
          setError(
            t("osinstall.run.cancelled", {
              files: job.state.files_landed ?? 0,
            })
          );
        }
      })
    );
  }, [t]);

  // Is the destination already occupied? Asked while it is being chosen, so
  // the refusal `apply()` would raise is on screen before the button, not
  // after a long operation appears to have done nothing.
  const [destinationTaken, setDestinationTaken] = useState(false);
  useEffect(() => {
    if (!destination) {
      setDestinationTaken(false);
      return;
    }
    let cancelled = false;
    void osinstallDestinationTaken(destination)
      .then((taken) => {
        if (!cancelled) setDestinationTaken(taken);
      })
      // A path ART cannot examine is not a path ART may declare occupied:
      // `apply()` decides, and blocking here would refuse an install the
      // engine would have allowed.
      .catch(() => {
        if (!cancelled) setDestinationTaken(false);
      });
    return () => {
      cancelled = true;
    };
  }, [destination, result]);

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
    if (!plan || layeringOn.length === 0) {
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
  }, [effectivePlanResult, layeringOn]);

  const basePlan = basePlanResult?.outcome === "planned" ? basePlanResult.plan : null;
  const effectivePlan = effectivePlanResult?.outcome === "planned" ? effectivePlanResult.plan : null;

  /**
   * The volume names the scan actually read out of the folder — memoized on
   * `mediaScan` itself so this is one identity per scan and not one per
   * render. ART-195 was a fresh `[]` per render driving an effect into a
   * loop; the effect below lists this among its dependencies.
   */
  const foundVolumeNames = useMemo(
    () => (mediaScan?.outcome === "found" ? mediaScan.media.map((m) => m.volumeName) : []),
    [mediaScan]
  );
  /**
   * ART-208. Non-null when the folder holds media and this release wants
   * none of it — the owner's own screen, where sixteen `MediaMissing`
   * refusals meant one wrong folder rather than sixteen missing disks. A
   * string or `null`, so it is a stable dependency for the lookup below.
   */
  const wrongFolder = effectivePlan ? wrongMediaFolder(effectivePlan, foundVolumeNames) : null;
  const [releaseHolding, setReleaseHolding] = useState<string | null>(null);
  useEffect(() => {
    if (!wrongFolder) {
      setReleaseHolding(null);
      return;
    }
    let cancelled = false;
    osinstallReleaseForMedia(foundVolumeNames)
      .then((named) => {
        if (!cancelled) setReleaseHolding(named);
      })
      // A folder ART cannot put a name to is the ordinary case, not an error
      // worth a badge: the sentence without a release still says what is
      // wrong and what to do about it.
      .catch(() => {
        if (!cancelled) setReleaseHolding(null);
      });
    return () => {
      cancelled = true;
    };
  }, [wrongFolder, foundVolumeNames]);

  const blocker = osinstallBlocker({
    mediaFolder,
    destination,
    destinationTaken,
    plan: effectivePlanResult,
    found: foundVolumeNames,
    releaseHolding,
  });
  const baseRomUnknown = basePlan ? hasRomUnknownRefusal(basePlan) : false;

  async function chooseMediaFolder() {
    const picked = await open({
      directory: true,
      multiple: false,
      title: t("osinstall.media.chooseTitle"),
    });
    if (typeof picked === "string") setMediaFolder(picked);
  }

  /**
   * Drop every remembered listing and re-plan against the discs themselves.
   *
   * Two steps on purpose. Forgetting is **durable** — the entries are removed
   * from disk, so a later preview cannot serve the same stale answer again —
   * and only then is the plan re-run. A "rescan" that merely skipped the cache
   * for one call would leave the wrong answer sitting there for the next one.
   *
   * `setRescanned` is what makes the button answer for itself: a control that
   * does its work silently is one the user cannot tell apart from one that
   * ignored them, and that is the exact defect this round has been chasing.
   */
  async function rescanMedia() {
    try {
      const dropped = await osinstallRescanMedia();
      setRescanned(dropped);
      // Re-plan against what is actually on the discs now. A new object
      // identity is the point: the plan effect keys on these values, and
      // nothing else about the request has changed.
      setRescanNonce((n) => n + 1);
      if (mediaFolder) void osinstallScanMedia(mediaFolder).then(setMediaScan).catch(() => {});
    } catch (e) {
      setPlanError(errorText(t, e));
    }
  }

  async function chooseRom() {
    const picked = await open({
      multiple: false,
      title: t("osinstall.rom.chooseTitle"),
      filters: [{ name: "Kickstart ROM", extensions: ["rom", "bin"] }],
    });
    if (typeof picked === "string") setRomPath(picked);
  }

  async function chooseDestination() {
    const picked = await open({
      directory: true,
      multiple: false,
      title: t("osinstall.destination.chooseTitle"),
    });
    if (typeof picked === "string") setDestination(picked);
  }

  function toggleConditional(def: ComponentDef, catalogue: ComponentDef[]) {
    const excluded = excludedConditional.includes(def.id);
    const forcedOn = isForcedOnByCondition(catalogue, basePlan, chosen, def.id);
    switch (conditionalToggleAction(excluded, forcedOn)) {
      case "undo-exclusion":
        setExcludedConditional(withoutExcluded(excludedConditional, def.id));
        return;
      case "confirm-off":
        // Turning off a condition-satisfied component is a confirmation,
        // not a plain uncheck — see the module doc comment.
        setPendingExclusion(def.id);
        return;
      case "toggle-chosen":
        // Off, and not because of the condition: either an ordinary opt-in
        // (the condition does not currently hold) or undoing that same
        // opt-in.
        setChosen(toggleChosen(catalogue, chosen, def.id));
    }
  }

  function confirmExclusion(id: string) {
    const next = confirmComponentOff(chosen, excludedConditional, id);
    setChosen(next.chosen);
    setExcludedConditional(next.excluded);
    setPendingExclusion(null);
  }

  async function runInstall() {
    if (!destination || !effectivePlan) return;
    setBusy(true);
    setError(null);
    setProgress(null);
    try {
      // The job id is what tells this screen's progress from every other
      // job's: `job-progress` is a single application-wide event, and an
      // install running beside an Aminet download would otherwise drive
      // this bar with the download's numbers.
      installJob.current = await osinstallApply(effectivePlan, destination);
      // `busy` clears on the result event, or here if the job never starts.
    } catch (e) {
      setError(errorText(t, e));
      setBusy(false);
      installJob.current = null;
      setProgress(null);
    }
  }

  async function chooseVerifyDistRoot() {
    const picked = await open({
      directory: true,
      multiple: false,
      title: t("osinstall.verify.distRoot.chooseTitle"),
    });
    if (typeof picked === "string") setVerifyDistRoot(picked);
  }

  async function chooseVerifyImage() {
    const picked = await open({
      multiple: false,
      title: t("osinstall.verify.image.chooseTitle"),
      filters: [{ name: "Amiga volume image", extensions: ["img", "hdf"] }],
    });
    if (typeof picked === "string") setVerifyImage(picked);
  }

  const verifySlot = parseOptionalSlot(verifySlotText);
  const verifyIndex = parsePartitionIndex(verifyIndexText);
  const verifyReady = !!verifyDistRoot && !!verifyImage && verifySlot.ok && verifyIndex !== null;

  async function runVerify() {
    if (!verifyDistRoot || !verifyImage || !verifySlot.ok || verifyIndex === null) return;
    setVerifying(true);
    setVerifyError(null);
    setVerifyReport(null);
    try {
      setVerifyReport(await osinstallVerify(verifyImage, verifySlot.value, verifyIndex, verifyDistRoot));
    } catch (e) {
      setVerifyError(errorText(t, e));
    } finally {
      setVerifying(false);
    }
  }

  return (
    <>
      <section className="card" style={{ marginBottom: 16 }}>
        <h2 style={{ fontSize: 16, marginTop: 0 }}>{t("osinstall.heading")}</h2>
        <p className="muted" style={{ fontSize: 12, margin: "4px 0 12px" }}>
          {t("osinstall.intro")}
        </p>

        <label style={{ display: "flex", flexDirection: "column", gap: 4, marginBottom: 12 }}>
          <span className="muted" style={{ fontSize: 12 }}>
            {t("osinstall.release.label")}
          </span>
          <select
            className="btn"
            style={{ maxWidth: "16em" }}
            value={release}
            onChange={(e) => setRelease(e.target.value as InstallRelease)}
          >
            {INSTALL_RELEASES.map((option) => (
              <option key={option} value={option}>
                {option}
              </option>
            ))}
          </select>
        </label>

        <Field
          label={t("osinstall.media.label")}
          value={mediaFolder}
          empty={t("osinstall.media.none")}
          onChoose={() => void chooseMediaFolder()}
          choose={t("common.browse")}
        />
        {mediaScan?.outcome === "folder-unreadable" && (
          <p className="badge badge-err" style={{ fontSize: 11, margin: "0 0 12px", display: "inline-block" }}>
            {t("osinstall.media.unreadable")}
          </p>
        )}
        {mediaScan?.outcome === "found" && mediaScan.media.length === 0 && (
          <p className="faint" style={{ fontSize: 11, margin: "0 0 12px" }}>
            {t("osinstall.media.empty")}
          </p>
        )}
        {mediaScan?.outcome === "found" && mediaScan.media.length > 0 && (
          <p className="faint" style={{ fontSize: 11, margin: "0 0 12px" }}>
            {t("osinstall.media.found", {
              count: mediaScan.media.length,
              names: mediaScan.media.map((m) => m.volumeName).join(", "),
            })}
          </p>
        )}

        {/*
          ART-194's two controls, and they belong together: the toggle says
          whether ART may trust what it remembers, and the button is what a
          user reaches for when it should not have. Shown in Beginner mode as
          well as Power — "the disc I am looking at is not the disc I put in"
          is not an advanced problem, and `usePowerMode` only ever hides
          things the user can do without.
        */}
        <div style={{ display: "flex", alignItems: "center", gap: 12, margin: "0 0 4px", flexWrap: "wrap" }}>
          <label style={{ display: "flex", alignItems: "center", gap: 6, fontSize: 12 }}>
            <input
              type="checkbox"
              checked={reuseScan}
              onChange={(e) => {
                setReuseScan(e.target.checked);
                setRescanned(null);
              }}
            />
            {t("osinstall.media.reuseScan")}
          </label>
          <button className="btn btn-sm" onClick={() => void rescanMedia()}>
            {t("osinstall.media.rescan")}
          </button>
        </div>
        <p className="faint" style={{ fontSize: 11, margin: "0 0 4px" }}>
          {t("osinstall.media.reuseScanHelp")}
        </p>
        {rescanned !== null && (
          <p className="faint" style={{ fontSize: 11, margin: "0 0 12px" }}>
            {rescanned === 0
              ? t("osinstall.media.rescannedNone")
              : t("osinstall.media.rescanned", { count: rescanned })}
          </p>
        )}

        <Field
          label={t("osinstall.rom.label")}
          value={romPath}
          empty={t("osinstall.rom.none")}
          onChoose={() => void chooseRom()}
          choose={t("common.browse")}
          hint={t("osinstall.rom.hint")}
        />
        {romError && (
          <p className="badge badge-err" style={{ fontSize: 11, margin: "0 0 12px", display: "inline-block" }}>
            {t("osinstall.rom.unreadable")}
          </p>
        )}
        {rom && (
          <p className="faint" style={{ fontSize: 11, margin: "0 0 12px" }}>
            {t("osinstall.rom.identified", { rom: rom.name })}
          </p>
        )}

        <Field
          label={t("osinstall.destination.label")}
          value={destination}
          empty={t("osinstall.destination.none")}
          onChoose={() => void chooseDestination()}
          choose={t("common.browse")}
          hint={t("osinstall.destination.hint")}
        />
      </section>

      <section className="card" style={{ marginBottom: 16 }}>
        <h2 style={{ fontSize: 16, marginTop: 0 }}>{t("osinstall.components.heading")}</h2>
        <p className="muted" style={{ fontSize: 12, margin: "4px 0 12px" }}>
          {t("osinstall.components.intro")}
        </p>

        {componentsError && (
          <p className="badge badge-err" style={{ fontSize: 11, margin: "0 0 12px", display: "inline-block" }}>
            {t("osinstall.components.unavailable")}
          </p>
        )}
        {!componentsError && catalogue === null && (
          <p className="faint" style={{ fontSize: 11, margin: "0 0 12px" }}>
            {t("osinstall.components.loading")}
          </p>
        )}

        <div style={{ display: "flex", flexDirection: "column", gap: 6 }}>
          {(catalogue ?? []).map((def) => {
            const excluded = excludedConditional.includes(def.id);
            const checked = def.required
              ? true
              : !def.available
                ? false
                : def.conditionMajor !== null
                  ? (effectivePlan?.componentsOn.includes(def.id) ?? false)
                  : chosen.includes(def.id);
            const disabled = def.required || !def.available;

            // Non-null inside this map by construction — `catalogue ?? []`
            // above yields no rows at all while it is loading.
            const loaded = catalogue ?? [];

            function handleChange() {
              if (disabled) return;
              if (def.conditionMajor !== null) {
                toggleConditional(def, loaded);
              } else {
                setChosen(toggleChosen(loaded, chosen, def.id));
              }
            }

            // Exactly one of four reasons, computed the same way for every
            // conditional row — see `conditionalReason`'s own doc comment
            // for why this can never fall through to nothing.
            //
            // ART-119 (#4): gated on `!def.required && def.available` —
            // dropped in an earlier fix round and re-added here. Unreachable
            // against today's shipped recipe (no component is both
            // conditional and either required or unavailable), but a
            // required or coming-later row already renders its own
            // "required"/"coming later" line above; a future recipe that
            // combined the two should not additionally show a rom-needed or
            // condition-on/off badge that contradicts it.
            const reason =
              def.conditionMajor !== null && !def.required && def.available
                ? conditionalReason(
                    def.conditionMajor,
                    isForcedOnByCondition(loaded, basePlan, chosen, def.id),
                    excluded,
                    baseRomUnknown,
                    rom?.name ?? null
                  )
                : null;
            const reasonText = reason ? conditionalReasonText(reason) : null;

            return (
              <div
                key={def.id}
                style={{
                  border: "1px solid var(--border)",
                  borderRadius: 4,
                  padding: "6px 10px",
                  background: checked ? "var(--bg-hover)" : "var(--bg)",
                }}
              >
                <label style={{ display: "flex", gap: 8, alignItems: "center", fontSize: 13 }}>
                  <input type="checkbox" checked={checked} disabled={disabled} onChange={handleChange} />
                  <strong>{label(def.id)}</strong>
                  {def.required && (
                    <span className="badge badge-muted" style={{ fontSize: 10 }}>
                      {t("osinstall.components.required")}
                    </span>
                  )}
                  {!def.available && (
                    <span className="badge badge-muted" style={{ fontSize: 10 }}>
                      {t("common.comingLater")}
                    </span>
                  )}
                </label>

                {def.required && (
                  <p className="faint" style={{ fontSize: 11, margin: "4px 0 0" }}>
                    {t("osinstall.components.reason.required")}
                  </p>
                )}

                {/* ART-157. A Kickstart *minimum* is a fact about what this
                    component's files need to run, not a switch — it never
                    turns a row on or off, so it is rendered on its own
                    rather than through `conditionalReason`, whose four
                    branches all describe `rom-older-than`'s switching. Shown
                    for every row that declares one, `required` included:
                    AmigaOS 3.9's floor sits on `workbench-base`, which is
                    required, and that is precisely the row a user needs to
                    read it on. */}
                {def.requiresRomMajor !== null && (
                  <p className="faint" style={{ fontSize: 11, margin: "4px 0 0" }}>
                    {t("osinstall.components.reason.romAtLeast", { major: def.requiresRomMajor })}
                  </p>
                )}

                {/* ART-119 (#2). This used to be four independent `&&`
                    guards, one per kind, so a fifth kind would have rendered
                    nothing at all — a conditional row ticked with no
                    explanation, the same defect a review already found here
                    once. `conditionalReasonText` is an exhaustive `switch`
                    over the union with a `never` fallthrough, so a fifth kind
                    is now a compile error instead of a blank line, and there
                    is one place deciding the wording rather than four. */}
                {reasonText && (
                  <p
                    className={reasonText.tone === "warn" ? "badge badge-warn" : "faint"}
                    style={{
                      fontSize: 11,
                      margin: "4px 0 0",
                      ...(reasonText.tone === "warn" ? { display: "inline-block" as const } : {}),
                    }}
                  >
                    {t(reasonText.phrase.key, reasonText.phrase.params)}
                  </p>
                )}

                {pendingExclusion === def.id && def.conditionMajor !== null && (
                  <div
                    className="badge badge-err"
                    style={{ display: "block", padding: "8px 10px", margin: "6px 0 0", fontSize: 11 }}
                  >
                    <p style={{ margin: "0 0 8px" }}>
                      {t("osinstall.components.confirmOff.warning", { major: def.conditionMajor })}
                    </p>
                    <div style={{ display: "flex", gap: 8 }}>
                      <button className="btn" onClick={() => confirmExclusion(def.id)}>
                        {t("osinstall.components.confirmOff.confirm")}
                      </button>
                      <button className="btn" onClick={() => setPendingExclusion(null)}>
                        {t("common.cancel")}
                      </button>
                    </div>
                  </div>
                )}
              </div>
            );
          })}
        </div>
      </section>

      {planError && (
        <section className="card" style={{ marginBottom: 16 }}>
          <p className="badge badge-err" style={{ display: "block", padding: "8px 12px", fontSize: 12 }}>
            {planError}
          </p>
        </section>
      )}

      {/*
        ART-208. One wrong folder is one sentence, not one absence per
        component — so this whole card is suppressed for that case and the
        sentence is said once, beside the button it blocks (`osinstallBlocker`
        returns it; see the run card below). Sixteen copies of "this disk is
        not in the folder", about a folder holding somebody else's release,
        is sixteen true sentences adding up to a false impression: the owner
        read them as "a lot of programs are missing".

        The list stays for every other case, which is most of them — one
        absent disk in an otherwise right folder has to say *which* disk.
      */}
      {effectivePlan && effectivePlan.refusals.length > 0 && !wrongFolder && (
        <section className="card" style={{ marginBottom: 16 }}>
          <h2 style={{ fontSize: 16, marginTop: 0 }}>{t("osinstall.refusals.heading")}</h2>
          <ul className="muted" style={{ fontSize: 12, margin: 0, paddingLeft: 20 }}>
            {effectivePlan.refusals.map((r, i) => {
              const phrase = refusalPhrase(r);
              return (
                <li key={i} style={{ padding: "2px 0" }}>
                  {t(phrase.key, phrase.params)}
                </li>
              );
            })}
          </ul>
        </section>
      )}

      {/* ART-175. What the switched-on layering components would replace,
          before the build runs — the informed-consent half of §92's PREVIEW.
          Above the plan's own file list on purpose: "what will be replaced"
          is the question a user has before "what will be placed". */}
      {(componentPreview || componentPreviewError) && (
        <section className="card" style={{ marginBottom: 16 }}>
          <h2 style={{ fontSize: 16, marginTop: 0 }}>{t("osinstall.replaces.heading")}</h2>
          {componentPreviewError && (
            <p className="badge badge-err" style={{ fontSize: 11, display: "inline-block" }}>
              {t("osinstall.replaces.failed", { error: componentPreviewError })}
            </p>
          )}
          {componentPreview && (
            <>
              <p className="muted" style={{ fontSize: 12, margin: "4px 0 10px" }}>
                {t("osinstall.replaces.summary", {
                  components: layeringOn.map(label).join(", "),
                  placed: componentPreview.placed,
                  // **`contested`, not `reports.length`** (review F4).
                  // `collide::preview` drops identical rows before returning,
                  // so `placed - reports.length` counted a file landing
                  // byte-for-byte on another component's copy as *new* — 130
                  // of them on the AmigaOS 3.9 overlay. The three counts
                  // partition what would be placed, and each is its own fact:
                  // landed on nothing, landed on identical bytes, replaced
                  // something. Stated in full because an empty report means
                  // "nothing is in the way", never "nothing happens" (§89).
                  fresh: componentPreview.placed - componentPreview.contested,
                  unchanged: componentPreview.contested - componentPreview.reports.length,
                  replaced: componentPreview.reports.length,
                })}
              </p>
              {componentPreview.reports.length > 0 && (
                <div
                  style={{
                    maxHeight: 260,
                    overflowY: "auto",
                    border: "1px solid var(--border)",
                    borderRadius: 4,
                    padding: "6px 10px",
                  }}
                >
                  {groupCollisionsForPreview(componentPreview.reports).map((group) => (
                    <div key={group.kind} style={{ marginBottom: 10 }}>
                      <div
                        className={group.kind === "downgrade" ? "badge badge-err" : "muted"}
                        style={{
                          fontSize: 11,
                          fontWeight: 600,
                          margin: "4px 0",
                          display: group.kind === "downgrade" ? "inline-block" : "block",
                        }}
                      >
                        {t(collisionGroupHeadingKey(group.kind), { count: group.reports.length })}
                      </div>
                      {group.reports.map((report) => {
                        const phrase = collisionPhrase(report.collision);
                        return (
                          <div
                            key={report.path}
                            data-testid="component-collision-row"
                            style={{
                              fontSize: 11,
                              padding: "3px 0",
                              borderBottom: "1px solid var(--border)",
                              display: "flex",
                              justifyContent: "space-between",
                              gap: 8,
                            }}
                          >
                            <span style={{ wordBreak: "break-all" }}>{report.path}</span>
                            <span
                              className={group.kind === "downgrade" ? undefined : "faint"}
                              style={
                                group.kind === "downgrade"
                                  ? { color: "var(--err-text)" }
                                  : undefined
                              }
                            >
                              {t(phrase.key, phrase.params)}
                            </span>
                          </div>
                        );
                      })}
                    </div>
                  ))}
                </div>
              )}
            </>
          )}
        </section>
      )}

      {effectivePlan && effectivePlan.items.length > 0 && (
        <section className="card" style={{ marginBottom: 16 }}>
          <h2 style={{ fontSize: 16, marginTop: 0 }}>{t("osinstall.plan.heading")}</h2>
          <p className="muted" style={{ fontSize: 12, margin: "4px 0 12px" }}>
            {t("osinstall.plan.summary", {
              count: effectivePlan.totalFiles,
              bytes: size(effectivePlan.totalBytes),
            })}{" "}
            {/* The tree and the work are two numbers, and the second explains
                why the list below is longer than the first: a file two
                components both write is planned twice and lands once
                (ART-205). */}
            {t("osinstall.plan.summaryItems", { count: effectivePlan.items.length })}
          </p>
          <div
            style={{
              maxHeight: 360,
              overflowY: "auto",
              border: "1px solid var(--border)",
              borderRadius: 4,
              padding: "6px 10px",
            }}
          >
            {groupByComponent(effectivePlan).map(({ component, items }) => (
              <div key={component} style={{ marginBottom: 8 }}>
                <div className="muted" style={{ fontSize: 11, fontWeight: 600, margin: "6px 0 2px" }}>
                  {label(component)}
                </div>
                {items.map((item, i) => (
                  <div
                    key={i}
                    style={{
                      display: "flex",
                      justifyContent: "space-between",
                      gap: 8,
                      fontSize: 11,
                      padding: "1px 0",
                    }}
                  >
                    <span style={{ wordBreak: "break-all" }}>
                      {item.to}
                      {item.isDir ? "/" : ""}
                    </span>
                    <span className="faint">{item.isDir ? "" : size(item.bytes)}</span>
                  </div>
                ))}
              </div>
            ))}
          </div>
        </section>
      )}

      <section className="card" style={{ marginBottom: 16 }}>
        <h2 style={{ fontSize: 16, marginTop: 0 }}>{t("osinstall.run.heading")}</h2>

        {error && (
          <div className="badge badge-err" style={{ display: "block", padding: "6px 12px", fontSize: 12, marginBottom: 12 }}>
            {error}
          </div>
        )}

        <label style={{ display: "flex", gap: 8, alignItems: "center", fontSize: 12, marginBottom: 10 }}>
          <input
            type="checkbox"
            checked={confirmed}
            disabled={!!blocker}
            onChange={(e) => setConfirmed(e.target.checked)}
          />
          {t("osinstall.run.confirm")}
        </label>

        <div style={{ display: "flex", gap: 8, alignItems: "center" }}>
          <button className="btn btn-primary" onClick={() => void runInstall()} disabled={busy || !confirmed || !!blocker}>
            {t(busy ? "osinstall.run.running" : "osinstall.run.run")}
          </button>
          {busy && <InstallProgress progress={progress} />}
          {blocker && (
            <span className="faint" style={{ fontSize: 11 }}>
              {t(blocker.key, blocker.params)}
            </span>
          )}
          {/*
            ART-208's one click, beside the control it unblocks. The refusals
            card above is suppressed for this case (see there), so this is the
            only place the sentence appears — ART-202's rule, which the owner
            stated for us: "aynı uyarı tek ekranda 2 tane".
          */}
          {wrongFolder && releaseHolding && isInstallRelease(releaseHolding) && (
            <button className="btn btn-sm" onClick={() => setRelease(releaseHolding)}>
              {t("osinstall.blocked.switchRelease", { release: releaseHolding })}
            </button>
          )}
        </div>
      </section>

      {result && (
        <section className="card" style={{ marginBottom: 16 }}>
          <h2 style={{ fontSize: 16, marginTop: 0 }}>{t("osinstall.result.heading")}</h2>
          <p style={{ fontSize: 12, margin: "4px 0 8px" }}>
            {t("osinstall.result.summary", {
              files: result.outcome.files,
              directories: result.outcome.directories,
              bytes: size(result.outcome.bytes),
            })}
          </p>
          <p className="faint" style={{ fontSize: 11, margin: "0 0 8px", wordBreak: "break-all" }}>
            {t("osinstall.result.root", { root: result.destination })}
          </p>
          {/* ART-197: the screen may not change what the next step points at
              without saying so. */}
          <p className="muted" style={{ fontSize: 12, margin: "0 0 8px", wordBreak: "break-all" }}>
            {t("osinstall.result.carried", { root: result.destination })}
          </p>
          <p className="muted" style={{ fontSize: 12, margin: 0 }}>
            {t("osinstall.result.nextStep")}
          </p>
        </section>
      )}

      <PackagePanel
        treeRoot={packagesTreeRoot}
        onTreeRootChange={(root) => setTree({ root, builtHere: false })}
        packageFolder={packagesFolder}
        onPackageFolderChange={(folder) => setPackages({ folder })}
        chosen={packagesChosen}
        onChosenChange={(chosen) => setPackages({ chosen })}
        release={release}
      />

      {/* The other half of the same question, and deliberately a second
          panel rather than a mode of the first: `PackagePanel` places a
          package's files from Windows, and this runs a package's own
          installer on the Amiga because its files cannot be placed from
          Windows at all (ART-166). They share the tree and the archive
          folder — it is one tree and one folder of downloads — and nothing
          else. */}
      <AmigaInstallPanel
        treeRoot={packagesTreeRoot}
        onTreeRootChange={(root) => setTree({ root, builtHere: false })}
        packageFolder={packagesFolder}
        release={release}
      />

      <section className="card" style={{ marginBottom: 16 }}>
        <h2 style={{ fontSize: 16, marginTop: 0 }}>{t("osinstall.verify.heading")}</h2>
        <p className="muted" style={{ fontSize: 12, margin: "4px 0 12px" }}>
          {t("osinstall.verify.intro")}
        </p>

        <Field
          label={t("osinstall.verify.distRoot.label")}
          value={verifyDistRoot}
          empty={t("osinstall.verify.distRoot.none")}
          onChoose={() => void chooseVerifyDistRoot()}
          choose={t("common.browse")}
          hint={t("osinstall.verify.distRoot.hint")}
        />
        <Field
          label={t("osinstall.verify.image.label")}
          value={verifyImage}
          empty={t("osinstall.verify.image.none")}
          onChoose={() => void chooseVerifyImage()}
          choose={t("common.browse")}
          hint={t("osinstall.verify.image.hint")}
        />

        <div style={{ display: "flex", gap: 16, marginBottom: 12, flexWrap: "wrap" }}>
          <label style={{ display: "flex", flexDirection: "column", gap: 4 }}>
            <span className="muted" style={{ fontSize: 12 }}>
              {t("osinstall.verify.slot.label")}
            </span>
            <input
              className="input"
              value={verifySlotText}
              onChange={(e) => setVerifySlotText(e.target.value)}
              style={{ maxWidth: "8em" }}
            />
            <span className="faint" style={{ fontSize: 10 }}>
              {t("osinstall.verify.slot.hint")}
            </span>
          </label>
          <label style={{ display: "flex", flexDirection: "column", gap: 4 }}>
            <span className="muted" style={{ fontSize: 12 }}>
              {t("osinstall.verify.index.label")}
            </span>
            <input
              className="input"
              value={verifyIndexText}
              onChange={(e) => setVerifyIndexText(e.target.value)}
              style={{ maxWidth: "8em" }}
            />
          </label>
        </div>

        {verifyError && (
          <div className="badge badge-err" style={{ display: "block", padding: "6px 12px", fontSize: 12, marginBottom: 12 }}>
            {t("osinstall.verify.error", { message: verifyError })}
          </div>
        )}

        <div style={{ display: "flex", gap: 8, alignItems: "center", marginBottom: 12 }}>
          <button className="btn" onClick={() => void runVerify()} disabled={verifying || !verifyReady}>
            {t(verifying ? "osinstall.verify.running" : "osinstall.verify.run")}
          </button>
          {!verifyReady && (
            <span className="faint" style={{ fontSize: 11 }}>
              {t("osinstall.verify.needsInputs")}
            </span>
          )}
        </div>

        {verifyReport && (
          <>
            {/* isVerified is failed === 0 && notChecked === 0 — never
                failed === 0 alone. "ART did not look" is not "ART found
                nothing wrong" (§89), so a NotChecked count above zero keeps
                this a warning, never a tick. */}
            <p
              className={isVerified(verifyReport) ? "badge badge-ok" : "badge badge-warn"}
              style={{ display: "block", padding: "8px 12px", fontSize: 12, marginBottom: 8 }}
            >
              {t(isVerified(verifyReport) ? "osinstall.verify.verified" : "osinstall.verify.notVerified")}
            </p>
            <p className="muted" style={{ fontSize: 12, margin: "0 0 8px" }}>
              {t("osinstall.verify.summary", {
                passed: verifyReport.passed,
                failed: verifyReport.failed,
                notChecked: verifyReport.notChecked,
              })}
            </p>
            <div
              style={{
                maxHeight: 280,
                overflowY: "auto",
                border: "1px solid var(--border)",
                borderRadius: 4,
                padding: "6px 10px",
              }}
            >
              {verifyReport.files.map((file, i) => (
                <div key={i} style={{ fontSize: 11, padding: "2px 0" }}>
                  <div style={{ display: "flex", justifyContent: "space-between", gap: 8 }}>
                    <span style={{ wordBreak: "break-all" }}>{file.path}</span>
                    {file.state === "pass" && (
                      <span className="badge badge-ok" style={{ fontSize: 10 }}>
                        {t("osinstall.verify.state.pass")}
                      </span>
                    )}
                    {file.state === "fail" && (
                      <span className="badge badge-err" style={{ fontSize: 10 }}>
                        {t("osinstall.verify.state.fail")}
                      </span>
                    )}
                    {file.state === "not-checked" && (
                      <span className="badge badge-warn" style={{ fontSize: 10 }}>
                        {t("osinstall.verify.state.notChecked")}
                      </span>
                    )}
                  </div>
                  {/* Rust-side detail text stays English (ART-060) — the
                      same rule CoreError messages and WhdloadRefusal follow. */}
                  {file.detail && (
                    <div className="faint" style={{ fontSize: 10 }}>
                      {file.detail}
                    </div>
                  )}
                </div>
              ))}
            </div>
          </>
        )}
      </section>
    </>
  );
}
