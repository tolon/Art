// Tab 1 — *Amiga dosyaları* (four-tab design § 3.1): **the material, and
// nothing else.** Which release is being built, which folders hold its disks,
// and what ART makes of what is in them (SD-2 · G5).
//
// This file **is** `OsInstall.tsx`, renamed on 2026-09-09 by round 4 task 5
// with every section that had found another tab deleted out of it —
// `git log --follow` shows the rename, so the material's own history is
// unbroken. That screen was the whole wizard on one route (design § 1.1);
// what is left here is the one question this tab asks.
//
// **What this file holds:** the release select; the material readout
// (`MaterialReadout`, `source-primary`) beside the folder column
// (`MaterialFolders`, `source-secondary`), where a drop is appended to the
// list and never replaces it; the identity pass behind one folded, counted
// line (`media-identity-fold`), with the reuse toggle and *Scan again* inside
// it because they are about that pass.
//
// **`AmigaInstallPanel` is no longer at the foot** (round 5, task 1). Running
// a package's own installer opens an emulator window and writes into a
// distribution tree rather than into the material this tab is about; it is a
// section of the WinUAE studio now, and resolves the release, the folders and
// the tree for itself.
//
// **It does not plan** (round 4 task 5's fix round). The folder column needs
// two things of a plan and no more — which layers this release declares
// (`useLayers`) and which folders the request will actually read
// (`foldersForPlan`, pure) — so it asks for those two and never for
// `osinstall_plan`, which opens and walks every ADF and ISO in the list. The
// tabs that draw the plan are 2, 3 and 4.
//
// **What left, and where it went.** Nothing was rewritten in any of the
// moves — the rows are the same rows, drawn from the same catalogues:
//
//   - **Tab 2, `ChoiceTab.tsx`** (four-tab design § 3.2, round 3) — the
//     release's component rows with their ticks, the confirm-off dialog, the
//     AmigaOS 3.9 updates in the material's own order, one first-boot tick,
//     and the folded *what this would replace* line. The ticks are the build
//     session's now (`session.components`, ART-290), not this screen's own
//     remembered keys; `osinstall.chosen.<release>` and
//     `osinstall.excludedConditional.<release>` are read exactly once, by
//     `seededComponents`, and never written again.
//   - **Tab 3, `MachineTab.tsx`** (§ 3.3) — the Kickstart ROM field, the
//     keymap select and the destination field. All three values are still
//     *read* here, because the plan needs them, through the same remembered
//     keys and the same guards: one value, two tabs, never two answers.
//   - **Tab 4, `BuildTab.tsx`** (§ 3.4, round 4) — the plan error badge, the
//     plan's refusals with the folder's own evidence above them and the
//     switch-release button beside them, the summary of what will be
//     written, the confirmation, the Build button, the run and every phase's
//     own ending. The result card's five `statedRelease` verdicts and its
//     removed/icons lists are the tree phase's report there, and the ART-197
//     hand-off of a finished tree to the session is the tree phase's too.
//   - **`src/lib/useInstallPlan.ts`** — the plan computation itself
//     (`layersFor` → `osinstall_components` → `osinstall_plan` once or twice
//     → `osinstall_component_collisions`, with the sanitize and prune writes
//     and the cancellation). Tabs 2, 3 and 4 go through the one hook, so no
//     two of them can disagree about what is being planned, and the
//     `basePlan`/`effectivePlan` pair lives there. **This tab is not among
//     them**: it reads `useLayers` and `foldersForPlan` and asks for no plan
//     at all — see the note on `layers` below for what that cost when it did.
//   - **`src/lib/useChainTree.ts`** — the chain's tree, asked once with the
//     readout's own overrides, so tab 1 and tab 2 say the same thing about
//     one file.
//   - The Verify section left earlier, in wave 3, to the volumes step
//     (`components/osbuilder/VerifyAgainstCard.tsx`), where the card it
//     compares a tree against actually is.
//
// Three rules were named directly in the brief that built this screen. They
// govern rows that are **tab 2's** now — they were written here,
// `ChoiceTab.tsx` points back at them, and moving the JSX changed none of
// them, so they are kept where that pointer lands:
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
//
// A fourth rule went with the plan's own file list, which no tab draws any
// more: **it was read-only.** Unlike G11's layout preview, where retargeting
// a row *is* the feature, every destination in that list came from a recipe
// checked against real media, and a hand-moved row would have made
// `distribution.json` describe a release that was never actually built.
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

import { useCallback, useEffect, useMemo, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { useTranslation } from "react-i18next";

// ART-060: a Rust sentence ART recognises comes back in the user's own
// language; anything else is Rust's English verbatim, exactly as before.
import { errorText } from "@/lib/errorText";

import { hostParentDir } from "@/lib/hostPath";
import {
  INSTALL_RELEASES,
  layerForMedia,
  osinstallIdentifyMedia,
  osinstallRescanMedia,
  mediaIdentityFolderLines,
  mediaIdentityLines,
  mediaIdentitySummary,
  osinstallScanMedia,
  rememberedComponentKey,
  type InstallLayer,
  type InstallRelease,
  type MediaFolderOutcome,
  type MediaIdentification,
  type MediaIdentityState,
  type MediaScanResult,
  type SlotOverride,
  osinstallInstallMedia,
} from "@/lib/osinstall";
import { slotArchiveKey, slotOverrides } from "@/lib/amigainstall";
import { useSettingsStore } from "@/stores/settingsStore";
import { isFlag, isTextOrNothing, remember } from "@/lib/remembered";
import { useChainTree } from "@/lib/useChainTree";
import { useDestinationCheck } from "@/lib/useDestinationCheck";
import { useRemembered } from "@/lib/useRemembered";
import { foldersForPlan, type MaterialFolder } from "@/lib/buildSession";
import { hostAmigaForeverFolders } from "@/lib/api";
import { useBuildSession } from "@/lib/useBuildSession";
import { useLayers } from "@/lib/useLayers";
import { isJobCancellation } from "@/lib/jobs";
import { MaterialFolders } from "@/components/osbuilder/MaterialFolders";
import { MaterialReadout } from "@/components/osbuilder/MaterialReadout";

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

export function FilesTab({ droppedMedia = null }: { droppedMedia?: DroppedMedia }) {
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
    setRelease,
    setMaterial,
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

  // **The keyboard the finished system boots with is not read here at all**
  // any more (ART-226's other half). The select is `MachineTab`'s since round
  // 4 task 4 and the value only ever reached the plan; this tab stopped
  // planning in that round's fix pass, so it stopped reading it.

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
  /** Why the last "Scan again" could not forget anything, or `null`. Session
   *  only, like the count beside it, and rendered in the fold next to the
   *  button — a control that answers for itself when it works has to answer
   *  for itself when it does not. */
  const [rescanError, setRescanError] = useState<string | null>(null);
  /**
   * **Tab 1 does not plan** (round 4 task 5's fix round).
   *
   * This screen computed the whole plan — `useInstallPlan`, as it had since
   * round 3 — and read three fields of the answer: `layers`, `layersKnown`
   * and `plannedFolders`. Those three cost one `osinstall_layers`, a read of
   * a shipped JSON recipe. The rest of that hook costs `osinstall_components`
   * and then `osinstall_plan`, which **opens and walks every ADF and ISO in
   * the material list**, and nothing on this tab read a byte of the result.
   *
   * That is ART-119's own cost — the same work done for an answer nobody
   * uses — on the tab the user opens first and stays on longest, adding
   * folders, while every added folder starts the walk again. So the layers
   * effect is `src/lib/useLayers.ts` now, `useInstallPlan` calls that same
   * hook (one implementation of *which release is this the answer for*), and
   * tab 1 asks for the layers alone.
   *
   * **`plannedFolders` is pure and composed here.** `foldersForPlan` is
   * `buildSession.ts`'s, takes the material list and the layers, and reaches
   * nothing: a layered release plans from its tagged folders alone, an
   * unlayered one from every folder in the list. Memoized on the two, so it
   * is one identity per answer rather than one per render — ART-178/ART-195
   * was a fresh identity per render driving an effect that starts disk work.
   *
   * What the tabs that *do* plan read is unchanged: `ChoiceTab` and
   * `BuildTab` both go through `useInstallPlan`, so no two tabs can describe
   * two builds.
   */
  const { layers, layersKnown } = useLayers(release);
  const plannedFolders = useMemo(
    () => foldersForPlan(session.material, layers),
    [session.material, layers]
  );

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
   * the change is stated on screen instead of being made silently. A user who
   * wants a different tree still picks one; the picker never goes away.
   *
   * The writing itself is **tab 4's** since round 4 task 5: the tree phase's
   * report is where a finished build says what it did and where. This tab
   * only ever *reads* the value.
   *
   * **`session.packages.folder` is not read here at all any more** (round 5,
   * task 1): its one reader on this tab was `AmigaInstallPanel`'s
   * `packageFolder` prop, and the panel reads the session for itself in the
   * studio. The tree below is still read, by the readout's `installed` badge.
   */

  // --- what the screen is doing --------------------------------------------
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
   * **The list is one line long now** — the *Scan again* count, the only
   * answer this tab still holds that is about a release. The install report,
   * the plan error and the run confirmation went to `BuildTab` in round 4
   * task 5 and clear themselves there; the component checklist's pending
   * confirmation went to `ChoiceTab` in round 3, where it retires itself on
   * `planVersion`, which a release change bumps.
   *
   * **What is deliberately NOT cleared**, so a later reader does not "fix"
   * it: the ROM (a property of the machine, not the release — ART-207) and
   * the media scan (its own effect re-runs, because ART-207 keyed the folder
   * per release and the folder therefore changed too).
   *
   * **An honest limit.** This is a list, and a list can be forgotten: a
   * future card added without a line here would sit stale exactly as these
   * did. The structural answer is to remount the body on a `key={release}`,
   * which cannot be forgotten — but that needs `release` owned by a parent.
   * Said here rather than discovered later.
   */
  useEffect(() => {
    setRescanned(null);
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

  /**
   * **The other half of the same pair: the user answering an ambiguous row**
   * (round 4 whole-branch review, C1).
   *
   * The readout draws the question, this screen stores the answer, and it is
   * stored under the key `slotOverrides` above reads back — `slotArchiveKey`
   * is that function written backwards, so the writer and the reader cannot
   * spell one prefix two ways. The row then re-resolves as `chosen`, the
   * chain stops answering `Refused{Ambiguous}` for it, and tab 4's
   * *unresolved* warning goes with it.
   *
   * **On the click alone** (CLAUDE.md: nothing changes unless the user
   * changes it). No effect, no render-time write, and nothing at all for a
   * slot that is not a package's — `slotArchiveKey` answers `null` there and
   * this stores nothing rather than inventing an unscoped key.
   *
   * The store is read through `getState()` rather than the rendered bag, for
   * `useRemembered`'s own reason: two writers in one tick must not overwrite
   * each other with a stale copy.
   */
  const updateSettings = useSettingsStore((s) => s.update);
  const chooseSlotFile = useCallback(
    (slotId: string, path: string) => {
      const key = slotArchiveKey(slotId);
      if (!key) return;
      const latest = useSettingsStore.getState().settings.remembered;
      void updateSettings({ remembered: remember(latest, key, path) });
    },
    [updateSettings]
  );

  // **Asked here for the tree below, not for a blocker.** Whether the
  // destination is occupied is `BuildTab`'s question since round 4 task 5 —
  // it owns the button that refusal blocks. What this tab needs of the same
  // answer is the other half: whether the destination is already an ART
  // tree, which is what `useChainTree` decides the readout's `installed`
  // badge from. One `osinstall_describe_tree` per destination, handed over
  // rather than asked twice.
  //
  // **No `revision`**: nothing on this tab fills the folder, so there is no
  // moment at which the answer goes stale under it. The install's own result
  // bumped it while the run was here; the run is tab 4's now, and so is its
  // check.
  const destinationCheck = useDestinationCheck(destination);

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
   * the main one alone. **The folder column's own found line** — the
   * evidence sentences that used to read this too (`mediaEvidence`,
   * `wrongMediaFolder`, the blocker) are `BuildTab`'s since round 4 task 5,
   * where the refusals they qualify are. Memoized on the scans themselves so
   * this is one identity per scan and not one per render: ART-195 was a fresh
   * `[]` per render driving an effect into a loop, and this has been an
   * effect's dependency for most of its life.
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
   * Which of `foundVolumeNames` are install media at all (ART-285) — the
   * core's answer, read off every shipped recipe. The folder column used to
   * count every disc it could read a name from, and said *"23 install disks
   * found"* over a folder of CD32 games.
   *
   * `null` while the question is out and after it failed: the column claims
   * nothing then, rather than falling back to the count this replaced. Keyed
   * on a joined string rather than the array, the ART-178 habit.
   */
  const [installVolumeNames, setInstallVolumeNames] = useState<string[] | null>(null);
  const foundNamesKey = foundVolumeNames.join("\u0000");
  useEffect(() => {
    if (foundVolumeNames.length === 0) {
      setInstallVolumeNames([]);
      return;
    }
    let cancelled = false;
    setInstallVolumeNames(null);
    osinstallInstallMedia(foundVolumeNames).then(
      (names) => {
        if (!cancelled) setInstallVolumeNames(names);
      },
      () => {
        if (!cancelled) setInstallVolumeNames(null);
      }
    );
    return () => {
      cancelled = true;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [foundNamesKey]);
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
      setRescanError(null);
      setRescanned(dropped);
      // **No plan nonce here since round 4 task 5's fix round.** This used to
      // bump a counter the plan effect keyed on, so the request went out
      // again against the discs themselves. Tab 1 does not plan, and the tabs
      // that do read `osinstall.reuseScan` — the toggle beside this button —
      // through the same remembered key: `forget_all` has already deleted the
      // listings, so their next plan cannot be served a stale one either.
      //
      // `forget_all` drops the remembered *hashes* too, not only the
      // remembered listings — so the content-hash lines above are now about
      // nothing and have to be recomputed. Leaving them on screen would be
      // the stale answer this button exists to escape, still being shown
      // after the user pressed the escape hatch.
      setIdentifyNonce((n) => n + 1);
      // Every folder in the list, not the first alone: a screen showing one
      // folder's fresh listing beside another's stale one is the same stale
      // answer this button exists to escape, half-hidden.
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
      // **Said beside the button that failed** (round 4 task 5). This used to
      // set the plan's own error, which the plan-error badge at the top of
      // this screen drew; the badge is `BuildTab`'s now, and routing a
      // *Scan again* failure into another tab's badge would be a control
      // that silently ignored the user — the exact defect the `setRescanned`
      // line above exists against.
      setRescanError(errorText(t, e));
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
              onChoose={chooseSlotFile}
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
              installVolumeNames={installVolumeNames}
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
          {/* A button that did nothing has to say so where it is (round 4
              task 5). Rust's own sentence, through `errorText` — no key of
              this screen's, because there is nothing this screen knows about
              the failure that the core did not say better. */}
          {rescanError && (
            <p
              className="badge badge-err"
              data-testid="rescan-error"
              style={{ display: "inline-block", fontSize: 11, margin: "0 0 12px" }}
            >
              {rescanError}
            </p>
          )}
          </details>
        )}
      </section>

    </>
  );
}
