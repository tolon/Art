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

import {
  INSTALL_RELEASES,
  rememberedComponentKey,
  type InstallLayer,
  type InstallRelease,
} from "@/lib/osinstall";
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

/**
 * One folder holding some of this build's material.
 *
 * `layer` keeps the meaning the per-layer fields had (design § 3.1): a folder
 * tagged with the layer of the release it holds — AmigaOS 3.2.2's `base` and
 * `update-3.2.2` — and `null` for a folder ART simply scans for everything.
 * A tag is only ever offered for a release whose recipe declares layers.
 */
export interface MaterialFolder {
  path: string;
  layer: string | null;
}

/**
 * **The one list of folders this build's material is in** (design § 3.1).
 *
 * The `kaynak` step used to ask for it in three shapes — one media folder,
 * a bag of extra folders, one field per media layer — and the `paketler`
 * step asked a fourth time for the archives. Four questions, four
 * resolutions, four different rules about what each field wanted. A person
 * who simply *has* the files had to know which sentence each one was asking
 * for.
 *
 * Per release, the way components are (`rememberedComponentKey`'s own
 * reason): a 3.2 folder list and a 3.9 folder list are two lists, and the
 * owner keeps 3.9's disc in `Amigatolon\iso` and 3.2's ADFs somewhere else
 * entirely. Carrying one into the other is ART-207, which cost sixteen true
 * `MediaMissing` refusals that together said something false.
 */
export interface MaterialChoice {
  folders: MaterialFolder[];
}

export interface MediaChoice {
  /**
   * @deprecated Derived from {@link MaterialChoice.folders} — the first
   * untagged entry — and no longer stored. Read `session.material.folders`
   * and, for a plan request, build the three request fields with
   * {@link foldersForPlan} rather than reaching for this. It stays on the
   * type so that nothing which reads it breaks in the round that introduces
   * the list.
   */
  folder: string | null;
  reuseScan: boolean;
}

export interface ComponentChoice {
  chosen: string[];
  excludedConditional: string[];
}

export interface PackageChoice {
  /**
   * The folder the Amiga-side install panel reads archives out of. It was
   * two panels' until round 3 task 3 deleted `PackagePanel`.
   *
   * **Still stored, and that is a decision rather than an oversight** (fix
   * round 1, F1). It is *also* in {@link MaterialChoice.folders} — adding a
   * folder here adds it there — but it keeps its own value while
   * `AmigaInstallPanel` still takes one folder. Making
   * it a pure view onto the list's first untagged entry broke two things at
   * once for a user whose archives live apart from their disks: on upgrade
   * the panels were handed the *disks* folder and their own already-chosen
   * packages sat above a catalogue that could not see them, and afterwards
   * the panel's own Browse button became a no-op, because it appended to the
   * list while the field went on showing the list's head. "Nothing changes
   * unless the user changes it", running backwards: they changed it and
   * nothing changed.
   *
   * `null` here means *nothing stored*, and only then does the session derive
   * {@link firstUntaggedFolder} — which is what makes a user who never had a
   * separate archives folder get the one list's answer for free.
   */
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
 * Whether first boot has been written into this build's tree.
 *
 * One value, not two panels each remembering their own — the same shape as
 * `CardChoice` and for the same reason. It belongs to **one tree**, so
 * `useBuildSession.setTree` resets it to `false` whenever the tree's `root`
 * changes: carrying "written" across to a different folder would be ART-197's
 * defect running the other way, telling a screen that a first-boot block
 * exists on a tree that has never seen one.
 */
export interface FirstBootChoice {
  written: boolean;
  /**
   * Whether the build should write a first-boot block at all — the choice
   * tab's third group (four-tab design § 3.2), and a different question from
   * `written`, which is a fact about a folder.
   *
   * **Absent means ticked, and absent is the shipped default.** The
   * first-boot block is what makes a PiStorm tree boot its own hardware
   * (first-boot design § 3), so the wanted state is on — but writing `true`
   * into everybody's `settings.json` on first render would be ART's own
   * "nothing changes unless the user changes it" broken by the screen that
   * merely displays the default. `recallInto` is what makes the optional
   * field expressible: it starts from the fallback and overwrites a field
   * only where the guard accepts a **stored** value, so a
   * `buildSession.firstboot` written by today's ART — `{ written: true }` —
   * comes back with `wanted` still absent, and only a user who touches the
   * tick puts a boolean there. A stored `false` is a boolean and survives.
   */
  wanted?: boolean;
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
  /** The one folder list (design § 3.1). `media.folder` and
   *  `packages.folder` are views onto it. */
  material: MaterialChoice;
  media: MediaChoice;
  rom: { path: string | null };
  release: InstallRelease;
  tree: TreeChoice;
  components: ComponentChoice;
  packages: PackageChoice;
  card: CardChoice;
  firstboot: FirstBootChoice;
}

/** Where each section persists inside `settings.remembered`. */
export const SESSION_KEYS = {
  kind: "buildSession.kind",
  media: "buildSession.media",
  rom: "buildSession.rom",
  release: "buildSession.release",
  tree: "buildSession.tree",
  card: "buildSession.card",
  firstboot: "buildSession.firstboot",
  /** Per release, for the reason `rememberedComponentKey` exists: a component
   *  id means something only inside the recipe that declares it. */
  components: (release: InstallRelease): string => `buildSession.components.${release}`,
  /** Per release too, and for the same reason one level out: a folder means
   *  something only inside the release that reads it (ART-207). */
  material: (release: InstallRelease): string => `buildSession.material.${release}`,
  /**
   * **Per release as well** (round 2 whole-branch review, M3).
   *
   * It was global while `material` was per release, and F10's guard — which
   * drops the stored archives folder when the material list no longer holds
   * it — compares against *the release being built*. So: build 3.9 with
   * an archives folder stored, switch to 3.2.2 (whose list seeds that folder
   * too), remove it there because it holds no 3.2 media, and 3.9's archives
   * folder is silently repointed at its disks folder. F1's own defect,
   * arriving through F1's own fix.
   *
   * A folder for a build, and a build is per release. `chosen` travels with
   * it for the reason `components` is per release: a package id means
   * something only inside the recipe that declares it (ART-209).
   */
  packages: (release: InstallRelease): string => `buildSession.packages.${release}`,
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
  /** The bag of added folders the flat field grew (work-list item 8), per
   *  release like the field itself. */
  extraMediaFolders: "osinstall.extraMediaFolders",
  reuseScan: "osinstall.reuseScan",
  rom: "osinstall.rom",
  release: "osinstall.release",
  destination: "osinstall.destination",
  packagesTreeRoot: "osinstall.packages.treeRoot",
  /** The **global** `buildSession.packages` an ART between the two waves
   *  wrote, now that the section is per release (round 2 review, M3). Read
   *  once as a migration source and never written again. */
  packagesSession: "buildSession.packages",
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

/**
 * **`reuseScan` alone** (fix round 1, F7).
 *
 * `MediaChoice.folder` is derived from {@link MaterialChoice.folders} and is
 * neither read nor written any more. Leaving it in the spec kept
 * `useRememberedShape`'s setter persisting it on every unrelated change — a
 * value seeded from the *unsuffixed* legacy key, written for ever, read by
 * nobody. The type keeps the field because callers still read
 * `session.media.folder`; the store does not.
 */
export const MEDIA_SPEC: { reuseScan: Guard<boolean> } = {
  reuseScan: isFlag,
};

/**
 * A stored folder list, checked entry by entry.
 *
 * A whole-list guard rather than `recallInto`'s field-by-field one because
 * the field *is* the list; one bad entry drops the whole list back to the
 * seed, which is `recall`'s own rule for a value that came off disk. An entry
 * with no `path` is not a folder, and a `layer` that is neither a string nor
 * `null` is a hand-edited file rather than something to guess about.
 */
export function isMaterialFolders(value: unknown): value is MaterialFolder[] {
  return (
    Array.isArray(value) &&
    value.every(
      (entry) =>
        typeof entry === "object" &&
        entry !== null &&
        typeof (entry as MaterialFolder).path === "string" &&
        (entry as MaterialFolder).path !== "" &&
        ((entry as MaterialFolder).layer === null ||
          typeof (entry as MaterialFolder).layer === "string")
    )
  );
}

export const MATERIAL_SPEC: { [K in keyof MaterialChoice]: Guard<MaterialChoice[K]> } = {
  folders: isMaterialFolders,
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

export const FIRSTBOOT_SPEC: { [K in keyof FirstBootChoice]: Guard<FirstBootChoice[K]> } = {
  written: isFlag,
  // Guarded like every other field, and **not** in `DEFAULT_FIRSTBOOT`: the
  // guard is what lets a stored `false` through, and the absence from the
  // default is what keeps an untouched tick out of `settings.json`. See
  // `FirstBootChoice.wanted`.
  wanted: isFlag,
};

export const DEFAULT_MATERIAL: MaterialChoice = { folders: [] };
export const DEFAULT_TREE: TreeChoice = { root: null, builtHere: false };
export const DEFAULT_MEDIA: MediaChoice = { folder: null, reuseScan: true };
export const DEFAULT_COMPONENTS: ComponentChoice = { chosen: [], excludedConditional: [] };
export const DEFAULT_PACKAGES: PackageChoice = { folder: null, chosen: [] };
export const DEFAULT_CARD: CardChoice = { image: null };
export const DEFAULT_FIRSTBOOT: FirstBootChoice = { written: false };

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

// ---------------------------------------------------------------------------
// The one material folder list (design § 3.1)
// ---------------------------------------------------------------------------

/**
 * A folder path folded for comparison only — never for display, never for
 * the wire.
 *
 * Windows: separators are interchangeable, a trailing one means nothing, and
 * case does not distinguish two folders. Two spellings of one folder in the
 * list would make ART read it twice, and `find_media_across` would then
 * refuse every disk in it as ambiguous with itself — a refusal with no
 * decision behind it, which is exactly what `addMediaFolder`'s own
 * same-folder check already existed to prevent.
 *
 * This is a *comparison*, not a resolution: it cannot see through a symlink,
 * a substituted drive or a UNC alias. `commands::osinstall::osinstall_slots`
 * canonicalises for real, with the filesystem in hand; this only has to stop
 * the same string being added twice.
 */
export function canonicalFolder(path: string): string {
  return path.replace(/[\\/]+/g, "\\").replace(/\\+$/, "").toLowerCase();
}

/** `folders` with `entry` appended, unless some spelling of it is already
 *  there. The existing entry — and its `layer` tag — is left exactly as it
 *  is: re-adding a folder is not a way to silently re-tag one. */
export function withFolder(folders: MaterialFolder[], entry: MaterialFolder): MaterialFolder[] {
  const wanted = canonicalFolder(entry.path);
  if (folders.some((held) => canonicalFolder(held.path) === wanted)) return folders;
  return [...folders, entry];
}

/**
 * Whether `id` names one of the shipped releases rather than a media layer.
 *
 * The per-layer keys and the per-release keys are the same shape for AmigaOS
 * 3.2 — `rememberedComponentKey` leaves that one release unsuffixed, so
 * `osinstall.mediaFolder.base` (a layer) and `osinstall.mediaFolder.AmigaOS
 * 3.9` (another release's flat folder) are told apart by nothing but this.
 * Reading the second as a layer would pull the 3.9 folder into the 3.2 list,
 * which is ART-207 running backwards.
 */
function namesARelease(id: string): boolean {
  return (INSTALL_RELEASES as readonly string[]).includes(id);
}

/**
 * The material folders one release starts with, from whichever keys the
 * user's own history put folders in.
 *
 * **The order *is* the migration**, the same way `seedTreeRoot`'s and
 * `seedCardImage`'s are, and it is the order the fields were drawn in:
 *
 * 1. `osinstall.mediaFolder.<release>` — the install-disks field;
 * 2. `osinstall.extraMediaFolders.<release>` — the folders added under it,
 *    in the order they were added;
 * 3. every `osinstall.mediaFolder.<layer>.<release>` — a layered release's
 *    own labelled fields, tagged with the layer they were asked for;
 * 4. `buildSession.packages.folder` — the archives the `paketler` step and
 *    the Amiga-side panel read.
 *
 * Deduplicated by {@link canonicalFolder}, first spelling kept, so a user who
 * had pointed two fields at one folder gets one entry rather than a folder
 * ART would read twice.
 *
 * **The legacy keys are read and never written again**, like every other
 * migration here: a user who rolls back to an earlier ART finds their paths
 * where that version looks for them. And a key the user has touched this run
 * never reaches this function at all — `useRememberedShape` uses a fallback
 * only while there is nothing stored, which is the whole of ART-089's
 * mechanism and the reason this is a seed rather than a write.
 *
 * The layer keys are walked in **sorted layer-id order**, not the recipe's:
 * this module has no recipe in hand, and the alternative — the order the
 * settings file happens to list its keys in — is not an order at all. What a
 * layer folder means is its tag, not its position.
 */
export function seededMaterial(store: unknown, release: InstallRelease): MaterialChoice {
  const bag = bagOf(store);
  let folders: MaterialFolder[] = [];

  const flat = textAt(bag, rememberedComponentKey(LEGACY_KEYS.mediaFolder, release));
  if (flat) folders = withFolder(folders, { path: flat, layer: null });

  const extras = bag[rememberedComponentKey(LEGACY_KEYS.extraMediaFolders, release)];
  if (isTextList(extras)) {
    for (const extra of extras) {
      if (extra) folders = withFolder(folders, { path: extra, layer: null });
    }
  }

  // The per-layer fields. Their keys are `osinstall.mediaFolder.<layer>`
  // suffixed by release, and the layer ids are not knowable here — the
  // recipe answers that over a round trip — so they are read back off the
  // key shape, with `namesARelease` keeping another release's flat folder
  // out. A layer id ART no longer declares still migrates: the folder is the
  // user's, and the readout will simply scan it.
  const prefix = `${LEGACY_KEYS.mediaFolder}.`;
  const suffix = rememberedComponentKey("", release);
  const layered: MaterialFolder[] = [];
  for (const key of Object.keys(bag)) {
    if (!key.startsWith(prefix)) continue;
    if (suffix && !key.endsWith(suffix)) continue;
    const layer = key.slice(prefix.length, key.length - suffix.length);
    if (!layer || namesARelease(layer)) continue;
    const path = textAt(bag, key);
    if (path) layered.push({ path, layer });
  }
  layered.sort((a, b) => (a.layer ?? "").localeCompare(b.layer ?? ""));
  for (const entry of layered) folders = withFolder(folders, entry);

  // The packages step's own folder, last: it is one question further along
  // the wizard, and a user who filled both meant the install folder first.
  // It stays a stored value of its own as well — see `PackageChoice.folder`.
  const packages = seedPackagesFolder(bag);
  if (packages) folders = withFolder(folders, { path: packages, layer: null });

  return { folders };
}

/**
 * The archives folder this session starts with, from whichever key the user's
 * own history put one in.
 *
 * Three spellings now, newest first: the **per-release** key a current ART
 * writes, the **global** `buildSession.packages` an ART between the two waves
 * wrote, and the legacy `osinstall.packages.folder` an older settings file
 * holds. Taking fewer would lose the folder for exactly one population of
 * users, and losing it is F1's own defect.
 *
 * `release` is optional because `seededMaterial`'s last entry is a migration
 * source rather than a per-release read: a folder the user once chose belongs
 * in every release's material list, since the archives in it do not stop
 * existing when the release picker moves.
 *
 * Stated once and used twice — here and as `useBuildSession`'s fallback for
 * the stored value itself — so the two cannot answer differently.
 */
export function seedPackagesFolder(store: unknown, release?: InstallRelease): string | null {
  const bag = bagOf(store);
  if (release) {
    const own = bagOf(bag[SESSION_KEYS.packages(release)]);
    if (isText(own.folder)) return own.folder;
  }
  const held = bagOf(bag[LEGACY_KEYS.packagesSession]);
  if (isText(held.folder)) return held.folder;
  return textAt(bag, LEGACY_KEYS.packagesFolder);
}

/** The packages ticked on the packages step, migrated the same way. */
export function seedPackagesChosen(store: unknown, release: InstallRelease): string[] {
  const bag = bagOf(store);
  const own = bagOf(bag[SESSION_KEYS.packages(release)]);
  if (isTextList(own.chosen)) return own.chosen;
  const held = bagOf(bag[LEGACY_KEYS.packagesSession]);
  if (isTextList(held.chosen)) return held.chosen;
  const legacy = bag[LEGACY_KEYS.packagesChosen];
  return isTextList(legacy) ? legacy : DEFAULT_PACKAGES.chosen;
}

/** The first folder ART scans for everything — what `media.folder` and
 *  `packages.folder` are now views onto. `null` when the list is empty or
 *  every entry carries a layer tag. */
export function firstUntaggedFolder(material: MaterialChoice): string | null {
  return material.folders.find((entry) => entry.layer === null)?.path ?? null;
}

/**
 * What a plan request carries, built from the one list.
 *
 * `InstallRequest` keeps its three folder fields — `plan()` reads a layered
 * recipe's media through `media_folders` and an unlayered one's through
 * `media_folder` plus `extra_media_folders`, and this round does not touch
 * the engine — so this is the single place the list is turned into them. One
 * function with tests, so the list the readout resolves and the folders the
 * planner reads cannot drift.
 */
export interface PlanFolders {
  mediaFolder: string;
  extraMediaFolders: string[];
  mediaFolders: Record<string, string>;
  /**
   * Folders in the list the request **cannot carry**, so the step can say so.
   *
   * A layered recipe reads only `media_folders` (`plan.rs` ignores the flat
   * fields outright for one), and that is a map — one folder per layer. So an
   * untagged folder, a second folder carrying a tag another already holds,
   * and a folder tagged with a layer this release does not declare are all
   * folders ART resolves in the readout and does **not** plan from. Naming
   * them is the difference between a screen that agrees with the core and one
   * that quietly drops a folder somebody added on purpose.
   *
   * Empty for an unlayered release, which reads every folder in the list.
   */
  unusedForPlan: string[];
}

export function foldersForPlan(material: MaterialChoice, layers: InstallLayer[]): PlanFolders {
  if (layers.length === 0) {
    // An unlayered release reads every folder, tag or no tag. A tag can only
    // be set while a release declares layers, so one here is a hand-edited
    // settings file or a recipe that dropped a layer — and using the folder
    // is what the user asked for either way. Dropping it would be losing a
    // choice they made.
    const paths = material.folders.map((entry) => entry.path);
    return {
      mediaFolder: paths[0] ?? "",
      extraMediaFolders: paths.slice(1),
      mediaFolders: {},
      unusedForPlan: [],
    };
  }

  const declared = new Set(layers.map((layer) => layer.id));
  const mediaFolders: Record<string, string> = {};
  const unusedForPlan: string[] = [];
  for (const entry of material.folders) {
    if (entry.layer === null || !declared.has(entry.layer) || entry.layer in mediaFolders) {
      unusedForPlan.push(entry.path);
      continue;
    }
    mediaFolders[entry.layer] = entry.path;
  }
  return { mediaFolder: "", extraMediaFolders: [], mediaFolders, unusedForPlan };
}
