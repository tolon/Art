// Tab 1 — *Kaynak* (four-tab design § 3.1): building an AmigaOS distribution
// tree from the user's own install disks (SD-2 · G5). This is the screen for
// the engine `src-tauri/src/core/osinstall` and `src/lib/osinstall.ts` already
// built and nobody could reach: material → the file list → confirm → job →
// report.
//
// **What this file holds, after round 3 of the four-tab rewrite
// (2026-09-09):** the release select; the material readout
// (`MaterialReadout`, `source-primary`) beside the folder column
// (`MaterialFolders`, `source-secondary`), where a drop is appended to the
// list and never replaces it; the identity pass behind one folded, counted
// line (`media-identity-fold`), with the reuse toggle and *Scan again* inside
// it because they are about that pass; the plan error badge; the refusals
// card; the plan itself — the file list, grouped by component; the keymap;
// the Run card with its blocker sentence; the result; and
// `AmigaInstallPanel` at the foot, the run on a real or emulated Amiga,
// which stays here until round 5 moves it.
//
// **What left, and where it went.** Nothing was rewritten in the move — the
// rows are the same rows, drawn from the same catalogues:
//
//   - **Tab 2, `ChoiceTab.tsx`** (four-tab design § 3.2, round 3) — the
//     release's component rows with their ticks, the confirm-off dialog, the
//     AmigaOS 3.9 updates in the material's own order, one first-boot tick,
//     and the folded *what this would replace* line. The ticks are the build
//     session's now (`session.components`, ART-290), not this screen's own
//     remembered keys; `osinstall.chosen.<release>` and
//     `osinstall.excludedConditional.<release>` are read exactly once, by
//     `seededComponents`, and never written again.
//   - **Tab 3, `MachineTab.tsx`** (§ 3.3) — the Kickstart ROM field and the
//     destination field. Both values are still *read* here, because the plan
//     needs both, through the same remembered keys and the same guards: one
//     value, two tabs, never two answers.
//   - **`src/lib/useInstallPlan.ts`** — the plan computation itself
//     (`layersFor` → `osinstall_components` → `osinstall_plan` once or twice
//     → `osinstall_component_collisions`, with the sanitize and prune writes
//     and the cancellation). Both tabs go through the one hook, so they
//     cannot disagree about what is being planned, and the
//     `basePlan`/`effectivePlan` pair lives there — the third rule below is
//     why there are two.
//   - **`src/lib/useChainTree.ts`** — the chain's tree, asked once with the
//     readout's own overrides, so tab 1 and tab 2 say the same thing about
//     one file.
//   - The Verify section left earlier, in wave 3, to the volumes step
//     (`components/osbuilder/VerifyAgainstCard.tsx`), where the card it
//     compares a tree against actually is.
//
// Four rules shape the screen, named directly in the brief. The first three
// govern rows that are **tab 2's** now — they were written here,
// `ChoiceTab.tsx` points back at them, and moving the JSX changed none of
// them; the fourth is still this file's own:
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
//     a source of a refusal. The build keeps **two** plans for exactly this
//     reason — see the module doc on `basePlan`/`effectivePlan` in
//     `src/lib/useInstallPlan.ts`, which owns both since round 3.
//   - **The file list is read-only.** Unlike G11's layout preview, where
//     retargeting a row *is* the feature, every destination here comes from
//     a recipe checked against real media — a hand-moved row would make
//     `distribution.json` describe a release that was never actually built.
//     The components are the only edit in the whole build, and they are tab
//     2's; the list itself has no controls.
//
// Remembered between runs, through `@/lib/remembered`'s guards: the keymap
// and the destination **per release** (`rememberedComponentKey`), because a
// keymap name and a component id mean nothing outside the recipe that
// declares them and switching release must not destroy the choices made for
// the other one, plus the identity pass's reuse toggle, which is one
// question about ART rather than about a release. The release itself, the
// media folders, the ROM and the component selection are the build session's
// — the selection per release too, seeded once from the two legacy keys and
// written back only to the session (ART-290). Nothing here arms a
// destructive action the way the preload screen's partition picks do —
// building a distribution tree only ever writes a *new* folder and refuses
// one that already exists (`SAFE_CREATE`), so there is nothing of that shape
// to protect against by leaving a choice unremembered.
//
// Every function doing anything other than rendering — the exclusion state
// machine, the reasoning behind a conditional tick, the Verify section's
// two small parsers — lives in `@/lib/osinstall` and is unit-tested there
// (`src/lib/osinstall.test.ts`); since round 3 the plan's own sequencing is
// out too, in `useInstallPlan` with its own tests. A review's own diagnosis
// of how the first Critical shipped: "no test can reach [it], because it
// lives inside the component." This screen is now the thin rendering layer
// that diagnosis asked for.

import { useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { useTranslation } from "react-i18next";

// ART-060: a Rust sentence ART recognises comes back in the user's own
// language; anything else is Rust's English verbatim, exactly as before.
import { errorText } from "@/lib/errorText";

import { hostParentDir } from "@/lib/hostPath";
import {
  componentDef,
  componentLabel,
  INSTALL_RELEASES,
  isInstallRelease,
  layerForMedia,
  mediaEvidence,
  onOsInstallResult,
  osinstallApply,
  osinstallBlocker,
  osinstallIdentifyMedia,
  osinstallRescanMedia,
  osinstallReleaseForMedia,
  keymapsIn,
  mediaIdentityFolderLines,
  mediaIdentityLines,
  mediaIdentitySummary,
  osinstallMediaEvidence,
  osinstallScanMedia,
  refusalPhrase,
  wrongMediaFolder,
  rememberedComponentKey,
  type InstallLayer,
  type ReleaseEvidence,
  type InstallPlan,
  type InstallRelease,
  type MediaFolderOutcome,
  type MediaIdentification,
  type MediaIdentityState,
  type MediaScanResult,
  type OsInstallResult,
  type SlotOverride,
} from "@/lib/osinstall";
import { slotOverrides } from "@/lib/amigainstall";
import { useSettingsStore } from "@/stores/settingsStore";
import { isFlag, isText, isTextOrNothing } from "@/lib/remembered";
import { useChainTree } from "@/lib/useChainTree";
import { useDestinationCheck } from "@/lib/useDestinationCheck";
import { useRemembered } from "@/lib/useRemembered";
import { type MaterialFolder } from "@/lib/buildSession";
import { hostAmigaForeverFolders } from "@/lib/api";
import { useBuildSession } from "@/lib/useBuildSession";
import { useInstallPlan } from "@/lib/useInstallPlan";
import {
  fraction,
  isJobCancellation,
  onJobProgress,
  subscribeSafely,
  type JobProgress,
} from "@/lib/jobs";
import { MaterialFolders } from "@/components/osbuilder/MaterialFolders";
import { MaterialReadout } from "@/components/osbuilder/MaterialReadout";
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
 * Keeps the same object identity across a no-op reset (ART-260):
 * `layerScans`, `layerIdentified` and `extraScans` all clear to `{}` when
 * there is nothing left to hold, and a fresh `{}` every settled render would
 * be a new identity for nothing. Harmless while nothing reads the record
 * itself as a dependency — but that is exactly the shape ART-178/ART-195
 * were, so every one of the three resets goes through this rather than its
 * own inline `{}`.
 */
export function resetIfEmpty<T>(prev: Record<string, T>): Record<string, T> {
  return Object.keys(prev).length === 0 ? prev : {};
}

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
  const {
    session,
    setTree,
    setRelease,
    setMaterial,
    setComponents,
    addMaterialFolder,
  } = useBuildSession();
  const release = session.release;

  /**
   * **The one list of folders this build's material is in** (design § 3.1) —
   * the build session's own value, per release.
   *
   * It replaces three separate questions this screen used to ask: the
   * install-disks field (`osinstall.mediaFolder.<release>`), the bag of added
   * folders under it (`osinstall.extraMediaFolders.<release>`), and one
   * labelled field per media layer. They were three shapes for one thing, and
   * a person who simply *has* the files had to work out which sentence each
   * field wanted. `buildSession.material.<release>` seeds from all three (and
   * from the packages step's folder) once, in `seededMaterial`.
   *
   * Per release, for the reason ART-207 bought: the owner chose AmigaOS 3.2
   * with the folder their 3.9 disc lives in still remembered, and every one
   * of the 3.2 recipe's sixteen components refused `MediaMissing` — sixteen
   * true sentences that together said "a lot of programs are missing" about a
   * folder that was simply the wrong one.
   *
   * **Order is not precedence.** The same volume name in two folders is
   * refused by name (`scan::media_for`), never resolved by which was added
   * first — picking one would be ART choosing between two of somebody's disks
   * on the strength of the order they clicked.
   */
  const materialFolders = session.material.folders;
  /** Every folder in the list, in order, as a primitive dependency — two
   *  equal strings are the same value to React, unlike two equal arrays
   *  (ART-178/ART-195). */
  const materialKey = materialFolders
    .map((entry) => `${entry.layer ?? ""}=${entry.path}`)
    .join("\n");

  /**
   * **The keyboard the finished system boots with** (ART-226's other half).
   *
   * The `keymaps` component places every layout the media carries; until this
   * existed nothing selected one, so a Turkish tree rendered `ç ü ş Ğ` in
   * its menus and still typed on an American keyboard — the owner's own
   * complaint, and the one that opened the issue.
   *
   * Per release, like the media folder (ART-207): a layout is a name in *that*
   * release's `Devs/Keymaps`.
   *
   * Empty means the ROM's `usa`, exactly as before. **No default**: choosing
   * somebody's keyboard for them is not ART's to do.
   */
  const [keymap, setKeymap] = useRemembered<string>(
    rememberedComponentKey("osinstall.keymap", release),
    isText,
    ""
  );

  // A disc dropped on the panel names a *file*; the scanner takes the folder
  // that holds it, which is also where its sibling discs and ADFs live. The
  // user dropped it, so this is them setting the value — it goes through the
  // session's own setter and is kept, like every other choice on this screen.
  //
  // **Appended, never replacing** (design § 3.1). The flat field had one slot,
  // so a drop overwrote whatever was in it; the list has room, and a person
  // dropping a second disc from a second folder means both folders. A folder
  // already in the list is not added twice (`withFolder`).
  //
  // Do NOT touch the list when `droppedMedia` is null — arriving at this
  // screen without a drop must leave the remembered folders alone (ART-089).
  //
  // Depends on `arrivalKey`, not just `path`: dropping the same disc twice —
  // with the list changed by hand in between — must add it back, and a
  // dependency array keyed only on the (unchanged) path string would never
  // re-run for that second, identical-looking drop.
  useEffect(() => {
    const folder = droppedMedia ? hostParentDir(droppedMedia.path) : null;
    if (folder) addMaterialFolder(folder);
  }, [droppedMedia?.path, droppedMedia?.arrivalKey, addMaterialFolder]);
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
   *
   * **The field itself moved to tab 3** on 2026-09-09 (`MachineTab`,
   * four-tab design § 3.3), and the *identity* moved to tab 2 with the
   * component rows a day later — a conditional row's reason line is the one
   * place the Kickstart's own name was read here. What is left is the path:
   * the plan sends it, and the run runs against it.
   */
  const romPath = session.rom.path;
  /**
   * Where the tree goes — per release, like the media folder above and for
   * the same reason (ART-207). `E:\…\os39\art3` is a fine destination for a
   * 3.9 build and a misleading one for a 3.2 build; a folder named for one
   * release holding another release's tree is the quiet kind of wrongness
   * `distribution.json` exists to make impossible.
   *
   * Read-only here since 2026-09-09: the picker is `MachineTab`'s, through
   * this very key. This screen still plans into it and still runs into it.
   */
  const [destination] = useRemembered<string | null>(
    rememberedComponentKey("osinstall.destination", release),
    isTextOrNothing,
    null
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
   * **The plan, computed in one place** — `useInstallPlan` (round 3 of the
   * four-tab rewrite, design § 3.2).
   *
   * The release's media layers, its component catalogue, the two plans and
   * the collision preview used to be six states and four effects in this
   * file. Tab 2 needs every one of them to draw the release's parts, and two
   * components computing them separately would be two answers to one
   * question — each costing its own walk of real install media. So the
   * computation left this screen whole; what stays here is what a screen does
   * with an answer. The hook's own module comment carries the ART-119,
   * ART-178/ART-195 and ART-089 rules the effects travel with.
   *
   * **The ticks are the build session's now** (ART-290). They were this
   * panel's own `osinstall.chosen.<release>` and
   * `osinstall.excludedConditional.<release>`; `buildSession.components` was
   * seeded from them and never written back, so the session's copy went stale
   * the moment anybody ticked a box. The legacy keys are still read once by
   * `seededComponents`, never written again and never deleted, so nobody's
   * remembered selection is lost.
   */
  const plan = useInstallPlan({
    release,
    material: session.material,
    keymap,
    rom: romPath,
    destination,
    reuseScan,
    rescanNonce,
    components: session.components,
    setComponents,
  });
  // What this screen still draws of the answer. The catalogue is here for
  // the one thing left that needs it — resolving a component id to its own
  // label, in a refusal's sentence and above each group of planned files.
  // The rows themselves, the ticks and the collision preview are `ChoiceTab`'s
  // since 2026-09-09 (four-tab design § 3.2), which reads this same hook.
  const {
    layers,
    layersKnown,
    plannedFolders,
    catalogue,
    effectivePlanResult,
    effectivePlan,
    planError,
    setPlanError,
  } = plan;

  /**
   * The folders the request actually reads, in list order — a layered
   * release's tagged ones, an unlayered release's whole list. Memoized so it
   * is one identity per plan-folder answer rather than one per render, since
   * two effects below depend on it (ART-178/ART-195).
   */
  const plannedFolderPaths = useMemo(
    () =>
      layers.length > 0
        ? layers.map((layer) => plannedFolders.mediaFolders[layer.id]).filter((f): f is string => !!f)
        : [plannedFolders.mediaFolder, ...plannedFolders.extraMediaFolders].filter((f) => !!f),
    [layers, plannedFolders]
  );

  /** A layer's own field label — the recipe's own `labelKey`, translated,
   *  when it names one, the bare layer id otherwise (a recipe with no
   *  `label_key` is not a shape any shipped one uses, but a screen must
   *  still render *something* rather than an empty label). */
  function layerLabel(layer: InstallLayer): string {
    return layer.labelKey ? t(layer.labelKey) : layer.id;
  }

  function layerById(id: string): InstallLayer | undefined {
    return layers.find((l) => l.id === id);
  }

  /**
   * **One scan per folder in the list, keyed by the folder's own path.**
   *
   * This used to be three states — `mediaScan` for the flat field,
   * `extraScans` for the folders added under it (ART-256), `layerScans` for a
   * layered release's own fields (fix round 1, Finding 1) — because there
   * were three folder controls. There is one control now, so there is one
   * record, and the three ways a folder could end up unscanned collapse into
   * the one they always were.
   *
   * The same `osinstallScanMedia` all three called, once per folder. A folder
   * that throws holds `null` — absence, not a badge, since a folder ART
   * cannot read is the ordinary case here — and the row itself says so from
   * its own `folder-unreadable` outcome.
   */
  const [folderScans, setFolderScans] = useState<Record<string, MediaScanResult | null>>({});
  useEffect(() => {
    const folders = materialFolders.map((entry) => entry.path);
    if (folders.length === 0) {
      // The previous object is kept when it is already empty (ART-260): a
      // fresh `{}` per run is a new identity for nothing, and
      // `foundVolumeNames` is memoized on this one — ART-178/ART-195 were
      // exactly a per-render identity driving an effect that starts disk
      // work.
      setFolderScans(resetIfEmpty);
      return;
    }
    let cancelled = false;
    Promise.all(
      folders.map(async (folder) => {
        try {
          return [folder, await osinstallScanMedia(folder)] as const;
        } catch {
          return [folder, null] as const;
        }
      })
    ).then((entries) => {
      if (!cancelled) setFolderScans(Object.fromEntries(entries));
    });
    return () => {
      cancelled = true;
    };
    // `materialKey`, not `materialFolders` — see its own doc comment on why
    // a derived string is the dependency rather than the array (ART-178).
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [materialKey]);

  /** The volume names one **folder** actually holds — `[]` when nothing was
   *  scanned, nothing was found, or the folder could not be read, which are
   *  three ways to nothing the wrong-layer hint below deliberately collapses
   *  (it is not that hint's question). */
  function folderVolumeNames(folder: string): string[] {
    const scan = folderScans[folder];
    return scan?.outcome === "found" ? scan.media.map((m) => m.volumeName) : [];
  }

  /**
   * Which layer each **folder** actually looks like —
   * `osinstall_layer_for_media`'s own answer, asked only once a scan has
   * found something (an empty pile decides nothing, the same gate
   * `layer_holding` itself applies).
   *
   * **Keyed by the folder's own path, not by layer id** (fix round 1, F5).
   * Two rows may carry the same tag — `foldersForPlan`'s `unusedForPlan`
   * exists precisely because that state is reachable — and keying by layer
   * meant the second row rendered a sentence computed from the *first* row's
   * scan, under the same DOM id. A hint is a claim about the folder in front
   * of it, so it is measured from that folder.
   */
  const [folderIdentified, setFolderIdentified] = useState<Record<string, string | null>>({});
  useEffect(() => {
    const tagged = materialFolders.filter((entry) => entry.layer !== null);
    if (layers.length === 0 || tagged.length === 0) {
      // Same guard as `folderScans` (ART-260): keep the previous object when
      // it is already empty, so a no-op reset does not manufacture a fresh
      // identity for nothing.
      setFolderIdentified(resetIfEmpty);
      return;
    }
    let cancelled = false;
    Promise.all(
      tagged.map(async (entry) => {
        const found = folderVolumeNames(entry.path);
        if (found.length === 0) return [entry.path, null] as const;
        try {
          return [entry.path, await layerForMedia(release, found)] as const;
        } catch {
          return [entry.path, null] as const;
        }
      })
    ).then((entries) => {
      if (!cancelled) setFolderIdentified(Object.fromEntries(entries));
    });
    return () => {
      cancelled = true;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [layers, folderScans, materialKey, release]);

  /**
   * The sentence for one **row**, when the media in that row's own folder
   * identifies as a different layer of this same release — `null` otherwise,
   * which covers "nothing scanned yet", "the scan agrees with the tag" and
   * "the scan found nothing distinguishing" alike, none of which are this
   * row's problem to report.
   */
  function wrongLayerHintFor(entry: MaterialFolder): string | null {
    if (!entry.layer) return null;
    const actualId = folderIdentified[entry.path];
    if (!actualId || actualId === entry.layer) return null;
    const actual = layerById(actualId);
    return t("osinstall.layer.wrongLayer", {
      actual: actual ? layerLabel(actual) : actualId,
      found: folderVolumeNames(entry.path).join(", "),
    });
  }

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

  const packagesFolder = session.packages.folder;

  // --- what the screen is doing --------------------------------------------
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
  /**
   * **Amiga Forever, offered** (design § 3.5) — the path the host answers
   * with, or `null` when there is nothing to offer or the user has said no.
   *
   * `AMIGAFOREVERDATA` is set by Amiga Forever itself and is a fact about
   * this machine, so ART can find the disks without asking. What it may not
   * do is *use* them: adding a folder nobody chose is exactly the
   * settings-change-without-a-user the remembered-settings rule forbids. So
   * this is a sentence with an Add button, shown only while the material list
   * is empty — a person who has already pointed ART somewhere is not
   * looking for a suggestion.
   *
   * Dismissal is a `useState`, never a remembered key: **a suggestion is not
   * a setting**, and writing "they said no once" to disk would be storing a
   * choice about every future build from one click.
   */
  const [amigaForeverAdf, setAmigaForeverAdf] = useState<string | null>(null);
  const [amigaForeverDismissed, setAmigaForeverDismissed] = useState(false);
  useEffect(() => {
    let cancelled = false;
    // Only `adf` is read here since 2026-09-09: the ROM half of this offer
    // went to tab 3 with the Kickstart field it answers (`MachineTab`),
    // which asks the host itself. One cheap command, no shared state.
    hostAmigaForeverFolders()
      .then((found) => {
        if (cancelled) return;
        setAmigaForeverAdf(found.adf);
      })
      // A host that cannot answer is a host with nothing to suggest. There is
      // no sentence to write about a suggestion that could not be made.
      .catch(() => {
        if (cancelled) return;
        setAmigaForeverAdf(null);
      });
    return () => {
      cancelled = true;
    };
  }, []);
  const amigaForeverOffer =
    materialFolders.length === 0 && !amigaForeverDismissed ? amigaForeverAdf : null;

  const [confirmed, setConfirmed] = useState(false);
  /**
   * **The run confirmation describes the plan that was on screen when it was
   * given**, so a new plan answer retires it — the same rule the preload
   * screen's own fingerprint/lastPlanned pair enforces, simplified here
   * because the plan is always fresh rather than sometimes stale.
   *
   * `planVersion` bumps on every answer, a refusal included: a plan that
   * could not be computed is not a plan the user confirmed either.
   *
   * `useLayoutEffect`, not `useEffect`: a `useEffect` runs *after* the
   * browser has painted, so a confirmation given against the previous plan
   * would be on screen for one frame beside a plan it does not describe —
   * and one frame is enough to press Build. (The component checklist's own
   * confirmation moved to `ChoiceTab` with the rows; it retires itself on
   * `planVersion` there, for this same reason.)
   */
  useLayoutEffect(() => {
    setConfirmed(false);
  }, [plan.planVersion]);
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

  // The Verify section is not here any more. Row 3 moved it out whole (it
  // shared no state with the install) and wave 3 moved it to the volumes
  // step, where the card it compares against actually is:
  // `components/osbuilder/VerifyAgainstCard.tsx`, rendered by `StepBirimler`.

  /**
   * **ART-210 — nothing computed for one release survives a switch to
   * another.**
   *
   * The owner, driving this screen: *"3.2 kurayım diyorsun, 3.9'un
   * seçenekleri, hataları vb ekranda duruyor asla değişmiyor."* Only two
   * effects here depended on `release` — the component list and the plan,
   * both `useInstallPlan`'s since round 3, which clears its own three
   * answers on the same rule. Every other answer on this screen is a `useState` nothing
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
    // The component checklist's pending confirmation is not on this list any
    // more — it moved to `ChoiceTab` with the rows, where it retires itself
    // on `planVersion`, which a release change bumps.
    setRescanned(null);
    setConfirmed(false);
  }, [release]);

  /**
   * **What the files in those folders are by content**, additively to the
   * volume names read above (design §4.3).
   *
   * Two facts from two sources, and they are never joined: `mediaScan` says
   * what each disk calls itself, this says what Emu68 Hatcher's table makes
   * of its bytes. A miss here removes nothing from the line above it — the
   * two are computed separately, rendered separately, and neither reads the
   * other.
   *
   * **One folder at a time, awaited in turn.** `osinstall_identify_media`
   * runs in its own job lane, and starting a second job in a lane supersedes
   * the first — whose promise then never settles. Firing one per folder with
   * `Promise.all` would therefore hang on every folder but the last. The
   * loop below has at most one job in flight, so nothing is superseded by
   * this screen's own doing.
   *
   * **The dependency is a primitive string, not the folder array.**
   * ART-178/ART-195 were a fresh identity per render driving an effect into a
   * loop; `identifyFoldersKey` is built fresh every render but two equal
   * strings are the same value to React, exactly as `useInstallPlan`'s own
   * `layerFoldersKey` relies on (it left this file with the plan in round 3
   * of the four-tab rewrite, so "above" is no longer where it is).
   *
   * **Every `set` is behind `cancelled`** (ART-089's mechanism): a folder
   * switched while a 700 MB disc is being read must not have the previous
   * folder's answer land on top of the new one.
   */
  const [mediaIdentity, setMediaIdentity] = useState<MediaIdentityState>({ kind: "not-asked" });
  /** Bumped by "Scan again", which drops the remembered hashes as well as
   *  the remembered listings — so the pass has to run again to say anything
   *  true. */
  const [identifyNonce, setIdentifyNonce] = useState(0);
  // **Every folder in the list**, whatever the plan can do with it. The
  // identify pass answers what a file *is*, which is a question about the
  // folder the user pointed at rather than about the request — and the
  // material readout reads its answers (`bytesRead`), so a folder left out
  // here would leave that readout saying nobody had looked.
  //
  // De-duplicated all the same: `withFolder` folds two spellings of one
  // folder on add, but a hand-edited settings file need not have, and hashing
  // one folder twice is both wasted work and a duplicate React key at the
  // render site (M5, fix wave 2).
  const identifyFoldersKey = Array.from(
    new Set(materialFolders.map((entry) => entry.path).filter((folder) => !!folder))
  ).join("\n");
  useEffect(() => {
    const folders = identifyFoldersKey ? identifyFoldersKey.split("\n") : [];
    if (folders.length === 0) {
      setMediaIdentity({ kind: "not-asked" });
      return;
    }
    let cancelled = false;
    setMediaIdentity({ kind: "identifying" });
    void (async () => {
      // Merged across folders, because the five endings are per *file* and a
      // user who added a second folder is looking at one pile of disks.
      const merged: MediaIdentification = {
        matches: [],
        unreadable: [],
        hashed: 0,
        remembered: 0,
        skipped: [],
      };
      // What became of each folder, by name. `core/hostfs.rs`'s rule one
      // layer up: the loop below is per entry and a folder already hashed
      // cannot be un-hashed by a later one failing, so every folder ends up
      // with its own result rather than the whole pass having one. Every
      // folder starts as "never opened", which is what is true of it before
      // its turn comes.
      const outcomes: MediaFolderOutcome[] = folders.map((folder) => ({
        folder,
        result: "not-reached",
      }));
      for (let i = 0; i < folders.length; i += 1) {
        try {
          const found = await osinstallIdentifyMedia(folders[i]);
          if (cancelled) return;
          merged.matches.push(...found.matches);
          merged.unreadable.push(...found.unreadable);
          merged.hashed += found.hashed;
          merged.remembered += found.remembered;
          merged.skipped.push(...found.skipped);
          outcomes[i] = { folder: folders[i], result: "identified" };
        } catch (err) {
          // ART-089's guard first, and before anything is put on screen: a
          // folder the user has already navigated away from must not land
          // its ending over the newer one.
          if (cancelled) return;
          // **Two rejections, two endings.** `awaitJobResult` rejects with
          // exactly `JOB_CANCELLED_MESSAGE` when the user pressed Stop and
          // with the job's own message otherwise, so the two are told apart
          // by the one predicate that owns that contract rather than by a
          // string compared here. Telling a user who stopped the pass that
          // ART "could not identify these files" is the §89 collapse — and
          // their next step (scan again) is not a failure's.
          if (isJobCancellation(err)) {
            outcomes[i] = { folder: folders[i], result: "stopped" };
            setMediaIdentity({ kind: "cancelled", identification: merged, folders: outcomes });
          } else {
            outcomes[i] = { folder: folders[i], result: "unreadable" };
            setMediaIdentity({ kind: "failed", identification: merged, folders: outcomes });
          }
          // Either way `merged` goes with it. Discarding what folders 1 and 2
          // identified because folder 3 could not be opened would be ART
          // claiming it had not done work it had — "never claim what you did
          // not do", read the other way round.
          return;
        }
      }
      if (!cancelled) setMediaIdentity({ kind: "identified", identification: merged });
    })();
    return () => {
      cancelled = true;
    };
  }, [identifyFoldersKey, identifyNonce]);
  /** The sentences, built by `src/lib` and translated here — `src/lib/*` has
   *  no translator (see `phrase.ts`), so it hands back keys and params and
   *  this is the only place that renders one. */
  const identityLines = useMemo(() => mediaIdentityLines(mediaIdentity), [mediaIdentity]);
  const identitySummary = useMemo(() => mediaIdentitySummary(mediaIdentity), [mediaIdentity]);
  /** Per folder, by name and by result — empty unless the pass stopped early,
   *  in which case the per-file list above cannot be read as a full report. */
  const identityFolders = useMemo(
    () => mediaIdentityFolderLines(mediaIdentity),
    [mediaIdentity]
  );
  /**
   * **What tells the material readout that the scan cache has moved** (fix
   * round 1, F2).
   *
   * `osinstall_slots` hashes nothing: it reads the cache the pass above
   * fills. Nothing in the readout's own dependency list changed when that
   * pass finished, so a first visit with a cold cache told the user "nobody
   * has read its bytes yet -- identify this folder by content" directly above
   * a section reporting that it had just hashed them. Two halves of one
   * screen disagreeing about the same files.
   *
   * A primitive, and one that moves exactly once per completed pass: the
   * "Scan again" nonce plus one while the pass is settled. It deliberately
   * does **not** move when a pass *starts* -- re-asking mid-pass would read
   * the same cold cache and put the same wrong sentence back.
   */
  const identifiedPass = identifyNonce + (mediaIdentity.kind === "identified" ? 1 : 0);

  /**
   * **The archives the user picked by hand on the Amiga-side step** (design
   * § 3.4; round 2 review, M5).
   *
   * This step is the one place that can see both: the readout below and the
   * remembered keys the panel writes. Without them the two screens said
   * opposite things about one artefact — *"not in the folders you named"*
   * here about a file the run over there was going to use.
   *
   * A string dependency rather than the array, for `MaterialReadout`'s own
   * reason: this effect chain starts disk work (ART-178/ART-195).
   */
  const remembered = useSettingsStore((s) => s.settings.remembered);
  const overridesKey = JSON.stringify(slotOverrides(remembered));
  const materialOverrides = useMemo(
    () => JSON.parse(overridesKey) as SlotOverride[],
    [overridesKey]
  );

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
        // ART-197: the tree ART has just written **is** the tree the next
        // steps act on. One variable, so the carry holds by structure rather
        // than by anyone remembering to wire it — and the result card says so
        // (`osinstall.result.carried`), because a carry the user cannot see
        // is the same defect as one that never happened.
        //
        // This used to be followed by `setVerifyDistRoot(r.destination)` as
        // well. `VerifyAgainstCard` now reads `session.tree.root` itself, so
        // the one line below carries both — and carries it to a step that has
        // not been built yet, which is what wave 3 needs.
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
  // after a long operation appears to have done nothing. `useDestinationCheck`
  // holds the rule: a path ART cannot examine is not a path ART may declare
  // occupied — `apply()` decides, and blocking here would refuse an install
  // the engine would have allowed.
  const destinationCheck = useDestinationCheck(destination, result);
  const { taken: destinationTaken } = destinationCheck;

  /**
   * **The same tree tab 2 asks about** (`useChainTree`, round 3 task 3's fix
   * round). This was `session.tree.root` alone while the choice tab asked the
   * chain about the *destination* when that turned out to be a build — two
   * lanes, two trees, and two `installed` answers about one file. The rule
   * now lives in one hook and both read it: the destination when ART has
   * looked at it and found a build, the session's own tree otherwise, which
   * is exactly what this screen had before in every other case.
   *
   * The check above is **handed over** rather than asked for again (fix round
   * 2): this screen already has the answer for this path, and two identical
   * `osinstall_describe_tree` round trips per destination is a second reader
   * of one fact — the shape this hook exists to remove. It also carries this
   * screen's own `revision`, so a folder a finished install has just filled
   * is re-examined here as well.
   */
  const { treeRoot: packagesTreeRoot } = useChainTree(destination, destinationCheck);

  /**
   * The volume names the scans actually read out of **every folder the plan
   * request carries** — the main one and each added one for an unlayered
   * release (ART-256), each layer's own for a layered one (ART-257), never
   * the main one alone. Memoized on the scans themselves so this is one identity
   * per scan and not one per render: ART-195 was a fresh `[]` per render
   * driving an effect into a loop, and both effects below list this among
   * their dependencies.
   *
   * **Duplicates are collapsed, first spelling kept.** Two folders can hold
   * the same volume name; the core refuses that by name (`scan::media_for`
   * raises `media-ambiguous`, and the refusals list says so disk by disk), so
   * this line naming `Workbench3.2` twice would be a second, worse account of
   * a problem already stated properly. Folded case-insensitively, because
   * `Workbench3.2` and `WORKBENCH3.2` are one volume name to AmigaDOS
   * (`core::osinstall::amiga_names_equal`) and would otherwise both appear.
   */
  const foundVolumeNames = useMemo(() => {
    const names: string[] = [];
    const seen = new Set<string>();
    const take = (scan: MediaScanResult | null | undefined) => {
      if (scan?.outcome !== "found") return;
      for (const medium of scan.media) {
        const folded = medium.volumeName.toLowerCase();
        if (seen.has(folded)) continue;
        seen.add(folded);
        names.push(medium.volumeName);
      }
    };
    // **Which folders this release actually reads**, said once and read off
    // the same `foldersForPlan` answer the request is built from (ART-256's
    // rule, now structural rather than repeated): a layered release plans
    // from its tagged folders alone, an unlayered one from every folder in
    // the list. A line here counting disks the request never sees is the
    // screen out-claiming the core.
    //
    // `layersKnown` for the reason a test found rather than a reading:
    // `layers === []` is one value with two causes — "this release is
    // unlayered" and "nobody has asked yet" — and the wrong branch
    // describes a folder set the request never carries (ART-256/ART-257).
    if (!layersKnown) return names;
    // In list order, so the sentence reads in the order the rows are drawn.
    for (const folder of plannedFolderPaths) take(folderScans[folder]);
    return names;
  }, [layersKnown, plannedFolderPaths, folderScans]);
  /**
   * ART-208. Non-null when the folder holds media and this release wants
   * none of it — the owner's own screen, where sixteen `MediaMissing`
   * refusals meant one wrong folder rather than sixteen missing disks. A
   * string or `null`, so it is a stable dependency for the lookup below.
   */
  /**
   * ART-253. What the release being built makes of the names in the folder —
   * the only thing that can *check* `wrongMediaFolder`'s claim rather than
   * infer it. Held as its own state, and `null` until it lands: the sentence
   * withdraws while it is in flight, because a specific claim with nothing
   * to check it against is what the defect was.
   */
  const [mediaFacts, setMediaFacts] = useState<ReleaseEvidence | null>(null);
  useEffect(() => {
    if (foundVolumeNames.length === 0) {
      setMediaFacts(null);
      return;
    }
    let cancelled = false;
    osinstallMediaEvidence(release, foundVolumeNames)
      .then((facts) => {
        if (!cancelled) setMediaFacts(facts);
      })
      // A release with no shipped recipe throws, and there is nothing
      // truthful to say about a folder against a recipe ART does not have.
      // `null` is "not checked", which withdraws the claim.
      .catch(() => {
        if (!cancelled) setMediaFacts(null);
      });
    return () => {
      cancelled = true;
    };
  }, [release, foundVolumeNames]);
  const wrongFolder = effectivePlan
    ? wrongMediaFolder(effectivePlan, foundVolumeNames, mediaFacts)
    : null;
  const [releaseHolding, setReleaseHolding] = useState<string | null>(null);
  useEffect(() => {
    // Looked up whenever the folder holds media, not only for the
    // all-or-nothing `wrongFolder` sentence above: the ordinary partial
    // case's own evidence line (`mediaEvidence` below, refusal-evidence
    // round Task 2) needs the same answer to tell "this release's own
    // media, some disks short" from "somebody else's media" apart.
    if (foundVolumeNames.length === 0) {
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
  }, [foundVolumeNames]);
  /**
   * ART-208's follow-on (refusal-evidence round, Task 2). What the folder
   * holds, for the ordinary partial case the refusals list already names
   * disk-by-disk — `mediaEvidence` itself refuses to speak over
   * `wrongMediaFolder`'s own sentence, so the two can never both render.
   */
  const mediaEvidenceLine = effectivePlan
    ? mediaEvidence({
        plan: effectivePlan,
        found: foundVolumeNames,
        releaseHolding,
        release,
        evidence: mediaFacts,
      })
    : null;

  /**
   * What the plan would really put in `Devs/Keymaps` — the picker's options,
   * so nothing offered can be refused for not being there.
   *
   * **Held across a re-plan**, and that is not a nicety. `effectivePlan` goes
   * `null` while a new plan is in the air, and reading the list straight off it
   * made the whole section vanish and come back on every keystroke elsewhere
   * on the screen — including on the user's own choice of keyboard, which
   * unmounted the control they had just used. A control that disappears under
   * somebody's hand is the same defect as one that answers wrongly.
   *
   * A plan that **has** arrived always supersedes, empty included: turning the
   * `keymaps` component off has to empty the list, not leave a stale one.
   */
  const [availableKeymaps, setAvailableKeymaps] = useState<string[]>([]);
  useEffect(() => {
    if (effectivePlan) setAvailableKeymaps(keymapsIn(effectivePlan));
  }, [effectivePlan]);

  // `osinstallBlocker` asks one question of `mediaFolder`: has *any* material
  // been pointed at yet. Answered from the folders the request actually
  // carries, so a layered release with two tagged folders is not told "no
  // folder chosen" because it sets no flat one, and an unlayered release
  // with a list is not told it either.
  const blocker = osinstallBlocker({
    mediaFolder: plannedFolderPaths.length > 0 ? (plannedFolderPaths[0] ?? release) : null,
    destination,
    destinationTaken,
    plan: effectivePlanResult,
    found: foundVolumeNames,
    releaseHolding,
    mediaFacts,
  });

  /**
   * The one folder picker (design § 3.1). Adding the same folder twice is
   * not an error and not a second folder: the core reads it once either way,
   * and `withFolder` folds it, which is the screen agreeing with the core
   * rather than contradicting it.
   */
  async function addFolder() {
    const picked = await open({
      directory: true,
      multiple: false,
      title: t("osinstall.media.addTitle"),
    });
    if (typeof picked === "string") addMaterialFolder(picked);
  }

  /** Drop one folder out of the list. */
  /**
   * Take a folder out of the list.
   *
   * The stored archives folder follows it out — in `useBuildSession`, not
   * here, because the list is the session's own value and every writer of it
   * owes the same guarantee (round 2, task 3's F10).
   */
  function removeFolder(path: string) {
    setMaterial(materialFolders.filter((entry) => entry.path !== path));
  }

  /** Tag a folder with the layer it holds, or untag it (`""` from the
   *  select). Only ever offered for a release whose recipe declares layers. */
  function tagFolder(path: string, layer: string) {
    setMaterial(
      materialFolders.map((entry) =>
        entry.path === path ? { path: entry.path, layer: layer || null } : entry
      )
    );
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
      // `forget_all` drops the remembered *hashes* too, not only the
      // remembered listings — so the content-hash lines above are now about
      // nothing and have to be recomputed. Leaving them on screen would be
      // the stale answer this button exists to escape, still being shown
      // after the user pressed the escape hatch.
      setIdentifyNonce((n) => n + 1);
      // Every folder in the list, not the first alone: they are all read on
      // the next plan, and a screen showing one folder's fresh listing beside
      // another's stale one is the same stale answer this button exists to
      // escape, half-hidden.
      void Promise.all(
        materialFolders.map(async (entry) => {
          try {
            return [entry.path, await osinstallScanMedia(entry.path)] as const;
          } catch {
            return [entry.path, null] as const;
          }
        })
      ).then((entries) => setFolderScans(Object.fromEntries(entries)));
    } catch (e) {
      setPlanError(errorText(t, e));
    }
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

        {/*
          **The answer first, the question beside it** (simplification design
          § 4). Two columns at a comfortable width, one under the other at a
          narrow one — a wrapping flex row, no stylesheet, no media query,
          because the builder is inline-styled and a query cannot see the
          shell's zoom (dockLayout.ts's own argument). The readout is the
          primary column and comes **first in the DOM**: first for a screen
          reader, and on top when the row wraps.

          **With no folders the primary column still speaks.** The readout
          renders nothing for an empty list — right, because a set line about
          a release the user has pointed nothing at would be a claim about
          their disk — so the step says what to do next in its place. A
          question nobody has asked is not an answer of "nothing found", and
          the two never share a sentence.
        */}
        <div
          data-testid="source-columns"
          style={{ display: "flex", flexWrap: "wrap", gap: 16, alignItems: "flex-start", margin: "0 0 12px" }}
        >
          <div data-testid="source-primary" style={{ flex: "1 1 26em", minWidth: 0 }}>
            {materialFolders.length === 0 && (
              <p className="muted" data-testid="material-ask" style={{ fontSize: 12, margin: 0 }}>
                {t("osinstall.material.askFolders")}
              </p>
            )}
            {/*
              **The readout: one row per artefact this release can use**
              (intake design § 3.3). Per *slot* — what the release needs and
              which file fills it — where the identity lines further down are
              per *file*.
            */}
            <MaterialReadout
              release={release}
              folders={materialFolders.map((entry) => entry.path)}
              treeRoot={packagesTreeRoot}
              rom={romPath}
              identifiedPass={identifiedPass}
              overrides={materialOverrides}
            />
          </div>
          <div data-testid="source-secondary" style={{ flex: "1 1 18em", minWidth: 0 }}>
            <MaterialFolders
              release={release}
              folders={materialFolders}
              layers={layers}
              layerLabel={layerLabel}
              folderScans={folderScans}
              wrongLayerHint={wrongLayerHintFor}
              unusedForPlan={plannedFolders.unusedForPlan}
              amigaForeverOffer={amigaForeverOffer}
              foundVolumeNames={foundVolumeNames}
              onAdd={() => void addFolder()}
              onRemove={removeFolder}
              onTag={tagFolder}
              onAmigaForeverAdd={(path) => {
                addMaterialFolder(path);
                setAmigaForeverDismissed(true);
              }}
              onAmigaForeverDismiss={() => setAmigaForeverDismissed(true)}
            />
          </div>
        </div>

        {/*
          **What the same files are by content** — design §4.3, and the whole
          risk surface of the hash round. Deliberately *below* the found line
          in the folder column and deliberately separate from it: that line
          is what the disks call themselves, this is what Emu68 Hatcher's
          table makes of their bytes, and a row's `volume` is measurably not
          a disk's volume name (0 of 12 matched). Two facts from two
          sources, never one reconciled answer.

          Nothing here is gated on Power mode. `usePowerMode` only ever hides
          what a user can do without, and "is this disk the one the table
          knows" is not an advanced question — it is the question somebody
          with a folder of ADFs of uncertain provenance actually has.
        */}
        {/*
          **The identity wall, folded** (four-tab design § 3.1). One line per
          file over every folder — 45 on the owner's three, 23 of them game
          discs — was half the screen's noise. The pass still runs (the
          readout's rank-2 and rank-3 rows read the cache it fills, F2); only
          the lines are behind a closed line whose count is the pass's own.
          The reuse toggle and Scan again are about that pass, so they live
          inside it.
        */}
        {mediaIdentity.kind !== "not-asked" && (
          <details data-testid="media-identity-fold" style={{ margin: "0 0 12px" }}>
            {/*
              **Only a finished pass may be counted.** A summary counting the
              lines reads "what 0 files … are" over a folder ART is at that
              moment reading, and — worse — a pass that died on its first
              folder or one the user cancelled at 12 of 45 would read like a
              finished one (§89: endings stay distinct). Running, failed and
              cancelled each have their own sentence with their own counts in
              `mediaIdentitySummary`, so the closed line says that sentence;
              only `identified` counts the lines behind it.
            */}
            <summary className="muted" style={{ fontSize: 12, cursor: "pointer" }}>
              {mediaIdentity.kind !== "identified" && identitySummary
                ? t(identitySummary.key, identitySummary.params)
                : t("osinstall.mediaId.foldSummary", { count: identityLines.length })}
            </summary>
          <div data-testid="media-identity" style={{ margin: "0 0 12px" }}>
            {identityLines.length > 0 && (
              <p className="faint" style={{ fontSize: 11, margin: "0 0 4px", fontWeight: 600 }}>
                {t("osinstall.mediaId.heading")}
              </p>
            )}
            {identityLines.map((line) => (
              <p
                key={line.path}
                data-testid={`media-identity-${line.kind}`}
                className={line.kind === "unreadable" ? "badge badge-err" : "faint"}
                style={{
                  fontSize: 11,
                  margin: "0 0 3px",
                  ...(line.kind === "unreadable" ? { display: "inline-block" } : {}),
                }}
                title={line.path}
              >
                {t(line.phrase.key, line.phrase.params)}
              </p>
            ))}
            {/*
              The other three states have their sentence on the `<summary>`
              line above, and this paragraph would repeat it word for word
              inside the fold — the screen answering the same question twice.
              Only the identified state's provenance line is unsaid up there
              (the summary counts the lines instead), so only it renders here.
            */}
            {mediaIdentity.kind === "identified" && identitySummary && (
              <p
                className="faint"
                data-testid="media-identity-summary"
                style={{ fontSize: 11, margin: "0 0 4px" }}
              >
                {t(identitySummary.key, identitySummary.params)}
              </p>
            )}
            {/*
              A pass that stopped early, reported **per folder, by name and by
              result** — `core/hostfs.rs`'s rule for an operation that cannot be
              undone as a whole. Nothing renders here when the pass covered
              every folder: the per-file list above is then the whole report,
              and a second list repeating it would be the screen answering the
              same question twice.
            */}
            {identityFolders.map((line) => (
              <p
                key={line.folder}
                data-testid={`media-identity-folder-${line.result}`}
                className={line.result === "unreadable" ? "badge badge-err" : "faint"}
                style={{
                  fontSize: 11,
                  margin: "0 0 3px",
                  ...(line.result === "unreadable" ? { display: "inline-block" } : {}),
                }}
                title={line.folder}
              >
                {t(line.phrase.key, line.phrase.params)}
              </p>
            ))}
          </div>

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
          </details>
        )}
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
          {/* Refusal-evidence round, Task 2: context the list below cannot
              give on its own — what the folder actually holds. Adds to the
              per-disk list, never replaces it (`wrongMediaFolder` above owns
              the all-or-nothing sentence instead, so the two never both
              show). */}
          {mediaEvidenceLine && (
            <p className="faint" style={{ fontSize: 12, margin: "0 0 8px" }}>
              {t(mediaEvidenceLine.key, mediaEvidenceLine.params)}
            </p>
          )}
          <ul className="muted" style={{ fontSize: 12, margin: 0, paddingLeft: 20 }}>
            {effectivePlan.refusals.map((r, i) => {
              const phrase = refusalPhrase(r);
              // Final whole-branch review, Finding F. `refusalPhrase` (pure
              // `src/lib`, no catalogue) can only carry the raw recipe
              // component id — every other refusal names it purely for
              // identification, but this is the one that tells the user to
              // go and tick it themselves, and a raw id is not a checkbox a
              // person can find. Resolved here, through the same `label()`
              // the component list itself uses, rather than in
              // `refusalPhrase`, which has no catalogue to resolve it with.
              const params =
                r.refusal === "resident-table-unreadable"
                  ? { ...phrase.params, component: label(r.component) }
                  : phrase.params;
              return (
                <li key={i} style={{ padding: "2px 0" }}>
                  {t(phrase.key, params)}
                </li>
              );
            })}
          </ul>
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

      {/* **ART-226's other half: choose the keyboard, having placed it.**
          The options are read off the plan's own items, so the list is exactly
          what will really be in `Devs/Keymaps` — a layout offered here cannot
          then be refused for not being there. Nothing selected leaves the
          system on the ROM's `usa`, which is what every ART tree did until
          now; a default would be ART choosing somebody's keyboard. */}
      {availableKeymaps.length > 0 && (
        <section className="card" style={{ marginBottom: 16 }} data-testid="keymap-section">
          <h2 style={{ fontSize: 16, marginTop: 0 }}>{t("osinstall.keymap.heading")}</h2>
          <p className="muted" style={{ fontSize: 12, margin: "4px 0 12px" }}>
            {t("osinstall.keymap.intro", { count: availableKeymaps.length })}
          </p>
          <label style={{ display: "flex", flexDirection: "column", gap: 4, maxWidth: "24em" }}>
            <span className="muted" style={{ fontSize: 12 }}>
              {t("osinstall.keymap.label")}
            </span>
            <select
              className="input"
              value={keymap}
              onChange={(e) => setKeymap(e.target.value)}
              aria-label={t("osinstall.keymap.label")}
            >
              <option value="">{t("osinstall.keymap.rom")}</option>
              {availableKeymaps.map((name) => (
                <option key={name} value={name}>
                  {name}
                </option>
              ))}
            </select>
            <span className="faint" style={{ fontSize: 10 }}>
              {t("osinstall.keymap.hint")}
            </span>
          </label>
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
          <p className="muted" style={{ fontSize: 12, margin: "0 0 8px" }}>
            {t("osinstall.result.nextStep")}
          </p>
          {/* Task 9: what the tree's own release marker says, compared with
              what this build was for — five distinct sentences, never
              folded into a pass/fail (CLAUDE.md's answer to the round that
              shipped AmigaOS 3.5 labelled 3.9: ask the artefact, never
              assert). "unreadable" (fix round 1, Finding 1) is never
              rendered as "unstated" — the tree may state a release just
              fine, ART simply could not read it. "expected-unknown" (final
              whole-branch review, Finding E) is never rendered as
              "mismatch" — the Rust side only asserts disagreement for a
              release it has actually measured a correct tree's marker for,
              so a correct AmigaOS 3.9 tree is never told it is wrong. */}
          <p className="muted" style={{ fontSize: 12, margin: 0 }}>
            {result.stated_release.verdict === "confirmed"
              ? t("osinstall.result.statedRelease.confirmed", {
                  stated: result.stated_release.stated,
                })
              : result.stated_release.verdict === "mismatch"
                ? t("osinstall.result.statedRelease.mismatch", {
                    expected: result.stated_release.expected,
                    stated: result.stated_release.stated,
                  })
                : result.stated_release.verdict === "expected-unknown"
                  ? t("osinstall.result.statedRelease.expectedUnknown", {
                      stated: result.stated_release.stated,
                    })
                  : result.stated_release.verdict === "unreadable"
                    ? t("osinstall.result.statedRelease.unreadable", {
                        detail: result.stated_release.detail,
                      })
                    : t("osinstall.result.statedRelease.unstated")}
          </p>
          {/* An update's own `removes` (Component.removes) — reported per
              entry, by name and by result, so the screen never claims a file
              was removed when the core said it could not (CLAUDE.md). Hidden
              entirely when nothing removed anything, which is every shipped
              recipe until AmigaOS 3.2.2's own recipe uses the field. */}
          {result.outcome.removed.length > 0 && (
            <div style={{ marginTop: 8 }}>
              <h3 style={{ fontSize: 13, margin: "0 0 4px" }}>
                {t("osinstall.result.removed.heading")}
              </h3>
              <ul style={{ margin: 0, paddingLeft: 18, fontSize: 11 }}>
                {result.outcome.removed.map((verdict) => (
                  <li key={verdict.to}>
                    {verdict.to} —{" "}
                    {verdict.state === "removed"
                      ? t("osinstall.result.removed.state.removed")
                      : verdict.state === "not-present"
                        ? t("osinstall.result.removed.state.notPresent")
                        : t("osinstall.result.removed.state.failed", {
                            detail: verdict.state.failed,
                          })}
                  </li>
                ))}
              </ul>
            </div>
          )}
          {/* An `icon-tooltypes` rule (RuleKind) — reported per entry, by
              name and by result, the same discipline `removed` follows just
              above and for the same reason: "not present" is a legitimate
              build (the component that would have placed the icon may be
              switched off), never a failure. Hidden entirely when nothing
              amended an icon, which is every shipped recipe until AmigaOS
              3.2.2's own recipe uses the rule. */}
          {result.outcome.icons.length > 0 && (
            <div style={{ marginTop: 8 }}>
              <h3 style={{ fontSize: 13, margin: "0 0 4px" }}>
                {t("osinstall.result.icons.heading")}
              </h3>
              <ul style={{ margin: 0, paddingLeft: 18, fontSize: 11 }}>
                {result.outcome.icons.map((verdict) => (
                  <li key={verdict.to}>
                    {verdict.to} —{" "}
                    {verdict.state === "merged"
                      ? t("osinstall.result.icons.state.merged")
                      : verdict.state === "destination-absent"
                        ? t("osinstall.result.icons.state.destinationAbsent")
                        : t("osinstall.result.icons.state.failed", {
                            detail: verdict.state.failed,
                          })}
                  </li>
                ))}
              </ul>
            </div>
          )}
        </section>
      )}

      {/* **The update checklist is gone from this screen** (four-tabs round
          3, task 3). `PackagePanel` was a flat catalogue that never joined
          the chain: two independent lists of one release's packages, on one
          lane, able to say different things about one file. Tab 2 draws the
          chain's own rows now — one tick each, one sentence each, the
          material's order — and this panel's remaining job is the *other*
          route, the one that cannot be placed from Windows at all (ART-166)
          and runs a package's own installer on the Amiga. It stays until
          round 5 moves it to the WinUAE studio.

          `packageFolder` is still what its dialogs open on and what its
          catalogue is loaded from; the list is what its *slots* are resolved
          against, which is the question "which of these files is BoingBag
          3.9-1" — and that has to be asked of everything the user has, not
          of one folder (design § 3.4). */}
      <AmigaInstallPanel
        treeRoot={packagesTreeRoot}
        onTreeRootChange={(root) => setTree({ root, builtHere: false })}
        packageFolder={packagesFolder}
        materialFolders={materialFolders.map((entry) => entry.path)}
        release={release}
      />

    </>
  );
}
