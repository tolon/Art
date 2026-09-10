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
  seedPackagesChosen,
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
import { forget, isFlag, recall, recallInto } from "@/lib/remembered";
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
  /**
   * The updates ticked for this release — **and nothing else**.
   *
   * `folder` is not a parameter any more (round 5, task 2; spec § 5).
   * `packages.folder` is seeded from an older settings file and read; the
   * one write left is `setMaterial` forgetting it when the user removes that
   * folder from the list (ART-291). A screen that has just been handed an
   * archives folder
   * calls {@link BuildSessionApi.addMaterialFolder}, which is where every
   * other folder in the build goes and what the slots resolve against.
   */
  setPackages: (change: { chosen?: string[] }) => void;
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

  // **`folder` is read here and written nowhere** (round 5, task 2; spec
  // § 5). It is still *seeded*, because a settings file written by an older
  // ART holds one and losing it would repoint a user's archives folder at
  // their disks folder — F1's own defect. What changed is that its one
  // writer went with `PackagePanel` in round 3: an archives folder chosen
  // today is added to `material.folders`, the list every slot resolves
  // against, and `folder` is the seed plus a view onto that list.
  const [packagesShape, setPackagesShape] = useRememberedShape<PackageChoice>(
    SESSION_KEYS.packages(release),
    PACKAGE_SPEC,
    {
      folder: seedPackagesFolder(bag, release),
      chosen: seedPackagesChosen(bag, release),
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
   * Replace the list — and **forget the stored archives folder when the user
   * has just taken it out** (ART-291, the owner's ruling of 2026-09-10).
   *
   * Round 5 made this write nothing (spec § 5), which left a folder an older
   * ART seeded steering the archive dialogs after the user removed it from
   * the list. The owner reversed that rule: a folder taken out of the list is
   * one the user no longer wants remembered, and ART leaves no record behind
   * that nobody needs.
   *
   * **Only a folder the user removed.** The clear fires when the seed was in
   * the list before this call and is not in it after — never merely because
   * the list does not hold it. An archives folder carried in from an older
   * ART and never in the list (fix round 1's F1 population) is not touched by
   * an edit that did not remove it; the gate first proposed for ART-291
   * dropped exactly those, and seven panel tests said so.
   *
   * **Cleared so that nothing is left for nothing.** With no updates ticked
   * and no older key that would hand the folder back on the next start, the
   * per-release record is forgotten outright. Otherwise the folder is written
   * down as `null`: the ticked updates stay, and a `null` is the one record
   * that stops `seedPackagesFolder` resurrecting the folder from an older
   * ART's key — which is itself left alone, for a rollback.
   *
   * Read from the store rather than the render closure, the rule
   * `addMaterialFolder` keeps for the same reason (F10's fix round 1, L10),
   * and compared through `canonicalFolder`, the lane's one rule for "the
   * same folder".
   */
  const setMaterial = useCallback(
    (folders: MaterialFolder[]) => {
      const latest = useSettingsStore.getState().settings.remembered;
      const before = recallInto<MaterialChoice>(
        latest,
        SESSION_KEYS.material(release),
        MATERIAL_SPEC,
        seededMaterial(latest, release)
      ).folders;
      const stored = recallInto<PackageChoice>(
        latest,
        SESSION_KEYS.packages(release),
        PACKAGE_SPEC,
        {
          folder: seedPackagesFolder(latest, release),
          chosen: seedPackagesChosen(latest, release),
        }
      );
      setMaterialShape({ folders });

      const seed = stored.folder;
      if (!seed) return;
      const wanted = canonicalFolder(seed);
      const holds = (list: MaterialFolder[]) =>
        list.some((entry) => canonicalFolder(entry.path) === wanted);
      if (!holds(before) || holds(folders)) return;

      const olderKeyWouldReturnIt = seedPackagesFolder(latest) !== null;
      if (!olderKeyWouldReturnIt && stored.chosen.length === 0) {
        const now = useSettingsStore.getState().settings.remembered;
        void useSettingsStore
          .getState()
          .update({ remembered: forget(now, SESSION_KEYS.packages(release)) });
      } else {
        setPackagesShape({ folder: null });
      }
    },
    [release, setMaterialShape, setPackagesShape]
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

  // The ticks, and only the ticks (round 5, task 2; spec § 5). F1's "both" —
  // the stored folder *and* the list — was written for a panel whose own
  // Browse wrote this key; that panel went in round 3, and `AmigaInstallPanel`
  // adds the folder its dialog returns to the material list directly. The
  // folder a Browse hands over is not lost by that: the list is what the
  // slots resolve against, and the panel keeps the folder it just picked for
  // its own dialogs for the rest of the session.
  const setPackages = useCallback(
    (change: { chosen?: string[] }) => setPackagesShape(change),
    [setPackagesShape]
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
  //
  // Since round 5 the stored side can only be a **seed** — a folder an older
  // ART wrote here — because nothing writes a folder into this key any more;
  // since ART-291 the one write is `setMaterial` clearing it when the user
  // removes that folder from the list. It
  // is kept ahead of the derived folder for the population F1 was written
  // for: their archives folder is not their disks folder, and repointing it
  // at one is the defect, not the fix. What it still decides is which folder
  // `AmigaInstallPanel`'s dialogs open on and which folder its catalogue is
  // asked about — both of which the panel overrides with the folder the user
  // picked this session, and neither of which resolves a file.
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
