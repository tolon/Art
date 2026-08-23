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
  COMPONENT_SPEC,
  DEFAULT_MEDIA,
  DEFAULT_PACKAGES,
  LEGACY_KEYS,
  MEDIA_SPEC,
  PACKAGE_SPEC,
  ROM_SPEC,
  SESSION_KEYS,
  TREE_SPEC,
  isBuildKind,
  seedRom,
  seedTreeRoot,
  seededComponents,
  type BuildKind,
  type BuildSession,
  type ComponentChoice,
  type MediaChoice,
  type PackageChoice,
  type TreeChoice,
} from "@/lib/buildSession";
import { isInstallRelease, type InstallRelease } from "@/lib/osinstall";
import { isFlag, isTextList, isTextOrNothing, recall } from "@/lib/remembered";
import { useRemembered, useRememberedShape } from "@/lib/useRemembered";
import { useSettingsStore } from "@/stores/settingsStore";

export interface BuildSessionApi {
  session: BuildSession;
  setKind: (next: BuildKind) => void;
  setMedia: (change: Partial<MediaChoice>) => void;
  setRom: (path: string | null) => void;
  setRelease: (next: InstallRelease) => void;
  setTree: (change: Partial<TreeChoice>) => void;
  setComponents: (change: Partial<ComponentChoice>) => void;
  setPackages: (change: Partial<PackageChoice>) => void;
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

  const [media, setMedia] = useRememberedShape<MediaChoice>(SESSION_KEYS.media, MEDIA_SPEC, {
    folder: recall(bag, LEGACY_KEYS.mediaFolder, isTextOrNothing, DEFAULT_MEDIA.folder),
    reuseScan: recall(bag, LEGACY_KEYS.reuseScan, isFlag, DEFAULT_MEDIA.reuseScan),
  });

  const [rom, setRomShape] = useRememberedShape<{ path: string | null }>(SESSION_KEYS.rom, ROM_SPEC, {
    // Through `seedRom`, which walks all three of the keys the three panels
    // used to keep separately (ART-197's fourth row) rather than only the
    // install step's own.
    path: seedRom(bag),
  });

  // The migration, in one line: an absent `buildSession.tree` falls back to
  // whichever legacy key the user's own history put a folder in.
  const [tree, setTree] = useRememberedShape<TreeChoice>(SESSION_KEYS.tree, TREE_SPEC, {
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

  const [packages, setPackages] = useRememberedShape<PackageChoice>(
    SESSION_KEYS.packages,
    PACKAGE_SPEC,
    {
      folder: recall(bag, LEGACY_KEYS.packagesFolder, isTextOrNothing, DEFAULT_PACKAGES.folder),
      chosen: recall(bag, LEGACY_KEYS.packagesChosen, isTextList, DEFAULT_PACKAGES.chosen),
    }
  );

  const setRom = useCallback((path: string | null) => setRomShape({ path }), [setRomShape]);

  const session = useMemo<BuildSession>(
    () => ({ kind, media, rom, release, tree, components, packages }),
    [kind, media, rom, release, tree, components, packages]
  );

  return {
    session,
    setKind,
    setMedia,
    setRom,
    setRelease,
    setTree,
    setComponents,
    setPackages,
  };
}
