// The React half of `@/lib/buildSession`.
//
// Every section reads through `useRememberedShape`, which rebuilds an object
// field by field — so a key added in a later ART costs the user nothing — and
// stabilises its identity with `sameRemembered`, so an effect depending on the
// session does not re-run on every render (ART-178/ART-195, measured at 2,149
// preview jobs in one session).
//
// **A facade, not a second store.** See `@/lib/buildSession`'s own comment for
// why: a parallel store would have to re-answer the late-landing load
// (ART-089), the identity churn, and the bad-persisted-value problem, all of
// which `settingsStore` and `useRemembered` already answer.
//
// `tree` is the one section with a **seeded** default rather than a constant
// one: its fallback is `seedTreeRoot`, which reaches the legacy keys. That is
// the whole migration, and it belongs here because `useRememberedShape`
// already does exactly the right thing with a fallback — it uses it while
// there is no stored value, and stops the moment there is one.

import { useCallback, useMemo } from "react";

import {
  CARD_SPEC,
  COMPONENT_SPEC,
  DEFAULT_FIRSTBOOT,
  DEFAULT_MEDIA,
  DEFAULT_PACKAGES,
  FIRSTBOOT_SPEC,
  LEGACY_KEYS,
  MATERIAL_SPEC,
  MEDIA_SPEC,
  PACKAGE_SPEC,
  ROM_SPEC,
  SESSION_KEYS,
  TREE_SPEC,
  canonicalFolder,
  firstUntaggedFolder,
  isBuildKind,
  seedCardImage,
  seedPackagesFolder,
  seedRom,
  seedTreeRoot,
  seededComponents,
  seededMaterial,
  withFolder,
  type BuildKind,
  type BuildSession,
  type CardChoice,
  type ComponentChoice,
  type FirstBootChoice,
  type MaterialChoice,
  type MaterialFolder,
  type MediaChoice,
  type PackageChoice,
  type TreeChoice,
} from "@/lib/buildSession";
import { isInstallRelease, type InstallRelease } from "@/lib/osinstall";
import { isFlag, isTextList, recall, recallInto } from "@/lib/remembered";
import { useRemembered, useRememberedShape } from "@/lib/useRemembered";
import { useSettingsStore } from "@/stores/settingsStore";

export interface BuildSessionApi {
  session: BuildSession;
  setKind: (next: BuildKind) => void;
  /** Replace the whole material folder list — what the `kaynak` step's own
   *  list control does on add, remove and re-tag. */
  setMaterial: (folders: MaterialFolder[]) => void;
  /** Append one folder, unless some spelling of it is already in the list.
   *  What a drop and the *Add folder* button reach for, and what
   *  `setMedia`/`setPackages` write through to. */
  addMaterialFolder: (path: string, layer?: string | null) => void;
  /**
   * @deprecated `media.folder` is a view onto `material.folders` now.
   * Setting it **adds** the folder to the list rather than replacing a
   * stored value; the other fields (`reuseScan`) still write through
   * normally. Passing `folder: null` changes nothing — a null is "nothing
   * chosen", not an instruction to drop somebody's first folder — so use
   * `setMaterial` to remove one.
   */
  setMedia: (change: Partial<MediaChoice>) => void;
  setRom: (path: string | null) => void;
  setRelease: (next: InstallRelease) => void;
  setTree: (change: Partial<TreeChoice>) => void;
  setComponents: (change: Partial<ComponentChoice>) => void;
  /** `chosen` is this panel's own; `folder` is the same deprecated view
   *  {@link BuildSessionApi.setMedia} carries, and adds to the list. */
  setPackages: (change: Partial<PackageChoice>) => void;
  /** The card this build writes and then prepares — one value for both steps
   *  (ART-197's remaining duplicate). */
  setCard: (image: string | null) => void;
  /** Whether first boot has been written into the build's tree. `setTree`
   *  resets this to `false` on its own whenever `root` changes — see
   *  `FirstBootChoice`'s own comment. */
  setFirstBoot: (change: Partial<FirstBootChoice>) => void;
}

export function useBuildSession(): BuildSessionApi {
  const bag = useSettingsStore((s) => s.settings.remembered);

  const [kind, setKind] = useRemembered<BuildKind>(
    SESSION_KEYS.kind,
    isBuildKind,
    recall(bag, LEGACY_KEYS.kind, isBuildKind, "boot-card")
  );

  const [release, setRelease] = useRemembered<InstallRelease>(
    SESSION_KEYS.release,
    isInstallRelease,
    recall(bag, LEGACY_KEYS.release, isInstallRelease, "AmigaOS 3.2")
  );

  /**
   * **The one folder list** (design § 3.1), per release like the components.
   *
   * The migration is `seededMaterial`, and it is the whole of it: an absent
   * `buildSession.material.<release>` falls back to the four legacy keys in
   * the order the fields were drawn in. `useRememberedShape` uses a fallback
   * only while nothing is stored, so a list the user has touched this run is
   * never reseeded — ART-089's mechanism, unchanged.
   */
  const [material, setMaterialShape] = useRememberedShape<MaterialChoice>(
    SESSION_KEYS.material(release),
    MATERIAL_SPEC,
    seededMaterial(bag, release)
  );

  // **`reuseScan` only, in the shape as well as in the read** (fix round 1,
  // F7). `media.folder` is derived below from `material.folders`, so the
  // stored `buildSession.media.folder` is neither read nor written any more
  // — two places holding one folder is the defect the list replaces, and
  // `useRememberedShape`'s setter persists `{ ...current, ...change }`, so
  // leaving `folder` in `MEDIA_SPEC` meant every `reuseScan` toggle wrote a
  // folder nobody reads, seeded from the unsuffixed legacy key.
  const [mediaShape, setMediaShape] = useRememberedShape<{ reuseScan: boolean }>(
    SESSION_KEYS.media,
    MEDIA_SPEC,
    { reuseScan: recall(bag, LEGACY_KEYS.reuseScan, isFlag, DEFAULT_MEDIA.reuseScan) }
  );

  const [rom, setRomShape] = useRememberedShape<{ path: string | null }>(SESSION_KEYS.rom, ROM_SPEC, {
    // Through `seedRom`, which walks all three of the keys the three panels
    // used to keep separately (ART-197's fourth row) rather than only the
    // install step's own.
    path: seedRom(bag),
  });

  // The migration, in one line: an absent `buildSession.tree` falls back to
  // whichever legacy key the user's own history put a folder in.
  const [tree, setTreeShape] = useRememberedShape<TreeChoice>(SESSION_KEYS.tree, TREE_SPEC, {
    root: seedTreeRoot(bag),
    builtHere: false,
  });

  // Per release: switching release and switching back must find the earlier
  // ticks, which is why this key is derived rather than fixed. A component id
  // means something only inside the recipe that declares it.
  const [components, setComponents] = useRememberedShape<ComponentChoice>(
    SESSION_KEYS.components(release),
    COMPONENT_SPEC,
    seededComponents(bag, release)
  );

  // **`folder` is still stored here** (fix round 1, F1) — see
  // `PackageChoice.folder` for why. It is in `material.folders` too, but a
  // user whose archives live apart from their disks must keep *their* folder
  // for the two package panels while those panels each take one.
  const [packagesShape, setPackagesShape] = useRememberedShape<PackageChoice>(
    SESSION_KEYS.packages,
    PACKAGE_SPEC,
    {
      folder: seedPackagesFolder(bag),
      chosen: recall(bag, LEGACY_KEYS.packagesChosen, isTextList, DEFAULT_PACKAGES.chosen),
    }
  );

  // ART-197's remaining duplicate: the card builder's destination and the
  // volumes step's image were two keys for one card. `seedCardImage` walks
  // both, hand-picked first.
  const [card, setCardShape] = useRememberedShape<CardChoice>(SESSION_KEYS.card, CARD_SPEC, {
    image: seedCardImage(bag),
  });

  // No legacy key: first boot did not exist before this round, so there is
  // nothing to migrate — a plain constant default, like `DEFAULT_MEDIA`.
  const [firstboot, setFirstBoot] = useRememberedShape<FirstBootChoice>(
    SESSION_KEYS.firstboot,
    FIRSTBOOT_SPEC,
    DEFAULT_FIRSTBOOT
  );

  /**
   * Replace the list — and **drop the stored archives folder when the list no
   * longer holds it** (round 2, task 3's F10).
   *
   * `packages.folder` keeps a value of its own as well as being a list entry
   * (see `PackageChoice.folder`: `PackagePanel` hands a single folder to
   * `osinstallCollisions` and `osinstallAddPackage`, and neither has a
   * list-shaped form). So a user who removed that folder from the list used
   * to leave the stored copy behind, and the step then said two things at
   * once: the Amiga Forever offer appeared — shown only while the list is
   * empty, so ART is claiming to have nothing — directly above two package
   * panels still reading archives out of the folder just removed. One
   * gesture, half applied.
   *
   * **Only the stored value is dropped, and only when it is stored.** When
   * nothing is stored, `packages.folder` is a *view* onto the list's first
   * untagged entry and follows the removal by itself; writing `null` there
   * would create a stored value where the user has none and switch that view
   * off for good — a setting changing without the user changing it, which is
   * the rule this is meant to be keeping.
   *
   * Compared through `canonicalFolder`, the same rule the list dedupes with,
   * so `E:\pkg` and `E:/pkg/` are one folder here too.
   */
  const setMaterial = useCallback(
    (folders: MaterialFolder[]) => {
      setMaterialShape({ folders });
      // **Read from the store, not from the render closure** (fix round 1,
      // L10) — the rule `addMaterialFolder` below already keeps and for the
      // same reason: two controls writing in one tick (a drop landing while a
      // Browse dialog resolves) must not decide against a stale copy. Rebuilt
      // through `recallInto` with the section's own spec and fallback, so this
      // reads exactly what `packagesShape.folder` would have been — a stored
      // `null` stays `null` here rather than falling back to a legacy key and
      // resurrecting a folder the user has already had removed.
      const latest = useSettingsStore.getState().settings.remembered;
      const stored = recallInto<PackageChoice>(latest, SESSION_KEYS.packages, PACKAGE_SPEC, {
        folder: seedPackagesFolder(latest),
        chosen: DEFAULT_PACKAGES.chosen,
      }).folder;
      if (!stored) return;
      const wanted = canonicalFolder(stored);
      if (!folders.some((entry) => canonicalFolder(entry.path) === wanted)) {
        setPackagesShape({ folder: null });
      }
    },
    [setMaterialShape, setPackagesShape]
  );

  // Reads the store rather than the rendered `material`, exactly as
  // `useRememberedShape`'s own setter does: two controls appending in one
  // tick — a drop landing while a Browse dialog resolves — must not overwrite
  // each other with a stale copy of the list.
  const addMaterialFolder = useCallback(
    (path: string, layer: string | null = null) => {
      if (!path) return;
      const latest = useSettingsStore.getState().settings.remembered;
      const held = recallInto<MaterialChoice>(
        latest,
        SESSION_KEYS.material(release),
        MATERIAL_SPEC,
        seededMaterial(latest, release)
      );
      setMaterialShape({ folders: withFolder(held.folders, { path, layer }) });
    },
    [release, setMaterialShape]
  );

  /** The folder both deprecated views answer with — the first entry ART
   *  scans for everything. */
  const derivedFolder = firstUntaggedFolder(material);

  const setMedia = useCallback(
    (change: Partial<MediaChoice>) => {
      const { folder, ...rest } = change;
      if (folder) addMaterialFolder(folder);
      if (Object.keys(rest).length > 0) setMediaShape(rest);
    },
    [addMaterialFolder, setMediaShape]
  );

  // **Both**, and that is F1's fix: the panel's own field has to follow the
  // pick (the stored value) *and* the folder has to reach the readout and the
  // planner (the list). Writing only the list made Browse a no-op for anyone
  // whose archives were not already the list's head.
  const setPackages = useCallback(
    (change: Partial<PackageChoice>) => {
      if (change.folder) addMaterialFolder(change.folder);
      setPackagesShape(change);
    },
    [addMaterialFolder, setPackagesShape]
  );

  const setRom = useCallback((path: string | null) => setRomShape({ path }), [setRomShape]);
  const setCard = useCallback(
    (image: string | null) => setCardShape({ image }),
    [setCardShape]
  );

  // First boot belongs to **one tree** (see `FirstBootChoice`'s own comment).
  // A change that sets `root` clears `written` on the same tick, so a screen
  // can never read "already written" about a folder that has never carried
  // one — ART-197's own defect, running the other way round.
  const setTree = useCallback(
    (change: Partial<TreeChoice>) => {
      if (change.root !== undefined && change.root !== tree.root) {
        setFirstBoot({ written: false });
      }
      setTreeShape(change);
    },
    [setTreeShape, tree.root, setFirstBoot]
  );

  // The two deprecated views are rebuilt here rather than stored, and the
  // memo keys on the derived string, so a render that changes neither the
  // folder nor `reuseScan` hands back the same object identity (ART-178).
  const media = useMemo<MediaChoice>(
    () => ({ folder: derivedFolder, reuseScan: mediaShape.reuseScan }),
    [derivedFolder, mediaShape.reuseScan]
  );
  // The stored archives folder **when there is one**, the list's first
  // untagged folder otherwise (fix round 1, F1). A user who never kept a
  // separate archives folder gets the one list's answer for free; a user who
  // did keeps theirs.
  const packages = useMemo<PackageChoice>(
    () => ({ folder: packagesShape.folder ?? derivedFolder, chosen: packagesShape.chosen }),
    [derivedFolder, packagesShape.folder, packagesShape.chosen]
  );

  const session = useMemo<BuildSession>(
    () => ({ kind, material, media, rom, release, tree, components, packages, card, firstboot }),
    [kind, material, media, rom, release, tree, components, packages, card, firstboot]
  );

  return {
    session,
    setKind,
    setMaterial,
    addMaterialFolder,
    setMedia,
    setRom,
    setRelease,
    setTree,
    setComponents,
    setPackages,
    setCard,
    setFirstBoot,
  };
}
