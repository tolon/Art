// The OS Builder's session: the values one build carries from step to step.
//
// **The defect this exists for (ART-197).** `OsInstall` remembered where it
// *wrote* a distribution tree as `osinstall.destination`; the two panels
// rendered directly beneath it read the tree they *operate on* from
// `osinstall.packages.treeRoot`. Nothing joined the two, so a user who had
// just watched ART write 1915 files into a folder was asked, immediately
// below, to locate a "distribution tree" — a term naming nothing visible on
// their own disk. The owner, who has read every document in this repository,
// could not answer the field.
//
// The fix is structural rather than a wire: there is now **one** `tree.root`,
// so the carry holds because there is nothing to forget. Hand-wiring
// `destination → treeRoot` would have left two variables and a habit.
//
// **A facade, not a second store.** Everything here persists through
// `settings.remembered` — the same bag `useRemembered` uses — because that bag
// already answers three problems a parallel store would have to answer again:
// a settings load landing after the user acted (ART-089), a fresh object
// identity per render turning an effect into a loop (ART-178/ART-195), and a
// hand-edited value reaching the screen (the guards). This module is pure; the
// React half is `@/lib/useBuildSession`.
//
// **Legacy keys are read once and then left alone.** They are never written
// again and never deleted: a user who rolls back to an earlier ART finds their
// paths where that version looks for them. A session key wins whenever it
// exists, so each fallback fires exactly once.
//
// **The spec's key count was wrong and it mattered.** The design says 22
// remembered keys across the builder; there are 36, measured across
// `src/pages/OsBuilder.tsx` and `src/components/osbuilder/*.tsx` on
// 2026-08-21. This wave takes over eleven — the ten in `OsInstall.tsx` plus
// `osBuilder.kind` — and leaves the other twenty-five exactly where they are,
// still owned by their own panels. A migration that never touches them cannot
// lose them.

import { rememberedComponentKey, type InstallRelease } from "@/lib/osinstall";
import { isFlag, isText, isTextList, isTextOrNothing, type Guard } from "@/lib/remembered";

/** What the screen is being asked for. Unchanged from `OsBuilder.tsx`. */
export type BuildKind = "distro" | "boot-card" | "install" | "prepare-volumes";

/**
 * The AmigaOS folder this build is about.
 *
 * `builtHere` records that ART wrote this tree in this session rather than the
 * user pointing at one. The two deserve different sentences — a summary step
 * saying "the tree you built" about a folder somebody merely selected is the
 * confident-and-wrong shape this project keeps paying for.
 */
export interface TreeChoice {
  root: string | null;
  builtHere: boolean;
}

export interface MediaChoice {
  folder: string | null;
  reuseScan: boolean;
}

export interface ComponentChoice {
  chosen: string[];
  excludedConditional: string[];
}

export interface PackageChoice {
  folder: string | null;
  chosen: string[];
}

/**
 * The card this build writes and then prepares.
 *
 * **ART-197's remaining duplicate.** The card builder remembered where it was
 * about to *create* an image (`cardBuilder.dest`); the volumes step remembered
 * which image it was about to *prepare* (`preload.image`). Two keys joined by
 * nothing — and in a build they are one card: ART writes it, then puts volumes
 * on it. That is ART-197's own defect in its second instance, and its own
 * words for it still apply: a user who has just watched ART write something
 * should not be asked to go and find it.
 *
 * One field, not two. "Where it goes" and "which one to prepare" are the same
 * question asked at two moments, exactly as the three Kickstart fields were.
 */
export interface CardChoice {
  image: string | null;
}

/**
 * One build, as the steps see it.
 *
 * Wave 1 carries what wave 1 wires. `amigaInstall`, `card` and `output` join
 * it in wave 2, in the task that rehomes those panels — declaring them now
 * would add fields nothing reads or writes.
 */
export interface BuildSession {
  kind: BuildKind;
  media: MediaChoice;
  rom: { path: string | null };
  release: InstallRelease;
  tree: TreeChoice;
  components: ComponentChoice;
  packages: PackageChoice;
  card: CardChoice;
}

/** Where each section persists inside `settings.remembered`. */
export const SESSION_KEYS = {
  kind: "buildSession.kind",
  media: "buildSession.media",
  rom: "buildSession.rom",
  release: "buildSession.release",
  tree: "buildSession.tree",
  packages: "buildSession.packages",
  card: "buildSession.card",
  /** Per release, for the reason `rememberedComponentKey` exists: a component
   *  id means something only inside the recipe that declares it. */
  components: (release: InstallRelease): string => `buildSession.components.${release}`,
} as const;

/**
 * The keys this wave reads from, once.
 *
 * Listed in one place so the fallback is auditable rather than scattered
 * through the file — and so a later reader can see exactly which of the 36 the
 * session took over.
 */
export const LEGACY_KEYS = {
  kind: "osBuilder.kind",
  mediaFolder: "osinstall.mediaFolder",
  reuseScan: "osinstall.reuseScan",
  rom: "osinstall.rom",
  release: "osinstall.release",
  destination: "osinstall.destination",
  packagesTreeRoot: "osinstall.packages.treeRoot",
  packagesFolder: "osinstall.packages.folder",
  packagesChosen: "osinstall.packages.chosen",
  /** The card step's own Kickstart, before wave 2 made it one value. */
  cardKickstart: "cardBuilder.kickstart",
  /** The Amiga-side install step's own, likewise. */
  amigaKickstart: "amigaInstall.kickstart",
  chosen: "osinstall.chosen",
  excludedConditional: "osinstall.excludedConditional",
  /** Where the card builder wrote an image, before wave 2 made it one value. */
  cardDest: "cardBuilder.dest",
  /** Which image the volumes step was preparing, likewise. */
  preloadImage: "preload.image",
} as const;

export function isBuildKind(value: unknown): value is BuildKind {
  return (
    value === "distro" ||
    value === "boot-card" ||
    value === "install" ||
    value === "prepare-volumes"
  );
}

// The shape guards `useRememberedShape` needs, one per section.
//
// `isTextOrNothing` is `@/lib/remembered`'s own guard — deliberately not a
// second one declared here, or two names would end up meaning one check.

export const TREE_SPEC: { [K in keyof TreeChoice]: Guard<TreeChoice[K]> } = {
  root: isTextOrNothing,
  builtHere: isFlag,
};

export const MEDIA_SPEC: { [K in keyof MediaChoice]: Guard<MediaChoice[K]> } = {
  folder: isTextOrNothing,
  reuseScan: isFlag,
};

export const COMPONENT_SPEC: { [K in keyof ComponentChoice]: Guard<ComponentChoice[K]> } = {
  chosen: isTextList,
  excludedConditional: isTextList,
};

export const PACKAGE_SPEC: { [K in keyof PackageChoice]: Guard<PackageChoice[K]> } = {
  folder: isTextOrNothing,
  chosen: isTextList,
};

export const ROM_SPEC: { path: Guard<string | null> } = { path: isTextOrNothing };

export const CARD_SPEC: { [K in keyof CardChoice]: Guard<CardChoice[K]> } = {
  image: isTextOrNothing,
};

export const DEFAULT_TREE: TreeChoice = { root: null, builtHere: false };
export const DEFAULT_MEDIA: MediaChoice = { folder: null, reuseScan: true };
export const DEFAULT_COMPONENTS: ComponentChoice = { chosen: [], excludedConditional: [] };
export const DEFAULT_PACKAGES: PackageChoice = { folder: null, chosen: [] };
export const DEFAULT_CARD: CardChoice = { image: null };

function bagOf(store: unknown): Record<string, unknown> {
  return typeof store === "object" && store !== null && !Array.isArray(store)
    ? (store as Record<string, unknown>)
    : {};
}

function textAt(bag: Record<string, unknown>, key: string): string | null {
  const held = bag[key];
  return isText(held) ? held : null;
}

/**
 * The tree this session starts on.
 *
 * The order **is** the migration: the session's own key first, then the folder
 * the packages step was pointing at, then the folder ART last wrote a tree
 * into. The middle one comes before the last because a user who picked a tree
 * by hand must find that pick unchanged — moving a setting is still changing
 * it, which the remembered-settings rule forbids. The last one is ART-197's
 * own user, who never picked, because nothing ever told them they had to.
 */
export function seedTreeRoot(store: unknown): string | null {
  const bag = bagOf(store);
  const held = bagOf(bag[SESSION_KEYS.tree]);
  if (isText(held.root)) return held.root;
  return textAt(bag, LEGACY_KEYS.packagesTreeRoot) ?? textAt(bag, LEGACY_KEYS.destination);
}

/**
 * The components ticked for one release.
 *
 * Per release, because a component id means something only inside the recipe
 * that declares it — both shipped recipes carry a `workbench-base`, for
 * different media. A migration reading a *fixed* key list would drop every
 * non-3.2 selection, since `rememberedComponentKey` suffixes all of them.
 *
 * The session key is preferred whole rather than field by field: a user who
 * has ticked components under the new key and has an empty `chosen` there
 * means an empty `chosen`, not "fall back to what the old key held".
 */
/**
 * The Kickstart this build is for, from whichever key the user's own history
 * put one in.
 *
 * **Three panels asked for it and each remembered its own** — ART-197's fourth
 * row. They are one question wearing three labels: the ROM the tree is paired
 * against (G9), the ROM the emulator boots to run a package's installer, and
 * the ROM written onto the card. A build where those differ is not a
 * configuration, it is the mismatch G9's pairing check exists to catch.
 *
 * Order matters and is not alphabetical. `osinstall.rom` first because it is
 * the one the pairing check reads and the one a user meets first; then the
 * card's, then the emulator's. A user who only ever filled the last of the
 * three still finds their ROM.
 *
 * Read once, like every other legacy key here — never written again and never
 * deleted, so a rollback still finds them.
 */
export function seedRom(store: unknown): string | null {
  const bag = bagOf(store);
  const held = bagOf(bag[SESSION_KEYS.rom]);
  if (isText(held.path)) return held.path;
  return (
    textAt(bag, LEGACY_KEYS.rom) ??
    textAt(bag, LEGACY_KEYS.cardKickstart) ??
    textAt(bag, LEGACY_KEYS.amigaKickstart)
  );
}

/**
 * The card this session starts on, from whichever key the user's history has.
 *
 * **The order is the migration, and it is the same order `seedTreeRoot` uses
 * for the same reason.** `preload.image` is a card the user went and *picked*,
 * and moving a setting is still changing it — the remembered-settings rule
 * forbids that, so a hand-made pick wins. `cardBuilder.dest` is where ART last
 * wrote one, which is ART-197's own user: they never picked, because nothing
 * ever told them they had to.
 *
 * Read once, never written again and never deleted, so a rollback still finds
 * them.
 */
export function seedCardImage(store: unknown): string | null {
  const bag = bagOf(store);
  const held = bagOf(bag[SESSION_KEYS.card]);
  if (isText(held.image)) return held.image;
  return textAt(bag, LEGACY_KEYS.preloadImage) ?? textAt(bag, LEGACY_KEYS.cardDest);
}

export function seededComponents(store: unknown, release: InstallRelease): ComponentChoice {
  const bag = bagOf(store);
  const held = bag[SESSION_KEYS.components(release)];
  if (held !== undefined) {
    const shape = bagOf(held);
    return {
      chosen: isTextList(shape.chosen) ? shape.chosen : [],
      excludedConditional: isTextList(shape.excludedConditional) ? shape.excludedConditional : [],
    };
  }
  const chosen = bag[rememberedComponentKey(LEGACY_KEYS.chosen, release)];
  const excluded = bag[rememberedComponentKey(LEGACY_KEYS.excludedConditional, release)];
  return {
    chosen: isTextList(chosen) ? chosen : [],
    excludedConditional: isTextList(excluded) ? excluded : [],
  };
}
