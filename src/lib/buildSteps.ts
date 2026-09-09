// The OS Builder's steps: which ones a build has, and whether one can act.
//
// **One step asks one question.** The screen this replaces put ten `<h2>`
// sections in a single scrolling column, and the owner's verdict on driving
// the release build was "bu işletim sistemi kurucusunda akış çok karmaşık
// gereksiz derecede uzun". The steps are sub-routes rather than internal
// state so browser back/forward and a jump to a step work at the router
// level, not through a switch somebody has to keep in sync.
//
// **Four numbered tabs for a tree**, after the owner's 2026-09-09 verdict on
// the five-step lane (four-tab design § 1.1): `dosyalar`, `secim`, `makine`,
// `derle`.
//
// **A step opens standalone.** Navigating straight to a step is legal:
// `readiness` is how the step knows whether it can act on what the session
// already holds or has to ask first. It never *blocks* — asking is a state,
// not a refusal, and no step is a gate in front of another. The Amiga-side
// install in particular stays optional, by the owner's own decision.
//
// Pure: no DOM, no i18next. `stepLabelKey` returns a **key**, because
// `src/lib` never renders a string.

import type { BuildKind, BuildSession } from "@/lib/buildSession";

/**
 * The steps that exist.
 *
 * Turkish path segments, deliberately untranslated (a URL that changed with
 * the language would break every remembered link and every
 * `builtin.rs::route` value). `hedef` is the entry every kind has; the four
 * after it are the install lane's numbered tabs (four-tab design § 2). The
 * retired `kaynak · paketler · amiga-kurulum · ilk-acilis` are redirects in
 * `pages/osbuilder/routes.tsx`, not steps — a step id no strip draws is dead
 * weight the next reader has to eliminate again.
 */
export const STEP_IDS = [
  "hedef",
  "dosyalar",
  "secim",
  "makine",
  "derle",
  "kart",
  "birimler",
] as const;

export type StepId = (typeof STEP_IDS)[number];

/**
 * Whether a step can act on what the session holds, or has to say something
 * first.
 *
 * **`wrong-folder` is ART-199.** A step that knew only whether *a path had
 * been chosen* looked ready on any folder at all: the owner pointed the
 * Amiga-side step at their own AmigaOS folder, the step showed no warning, and
 * the refusal arrived on the button — correct, and in the wrong place. Once
 * `describe_tree` has answered, the field can say it where the field is.
 */
export type Readiness = "ready" | "asks" | "wrong-folder";

/**
 * The steps one kind of build has.
 *
 * Not every kind has every step, and showing a card step to somebody building
 * a distribution tree is the "sections that do not belong on this screen"
 * complaint the owner made, in its own right. `hedef` is always first: it is
 * where the kind is chosen, so it is the one step every build has.
 */
export function stepsFor(kind: BuildKind): StepId[] {
  switch (kind) {
    case "install":
      return ["hedef", "dosyalar", "secim", "makine", "derle"];
    case "boot-card":
      return ["hedef", "kart"];
    case "prepare-volumes":
      return ["hedef", "birimler"];
    case "distro":
      // Every distro profile is registered `available: false` and rendered as
      // Coming Later (§96); there is no second step to offer yet.
      return ["hedef"];
  }
}

/**
 * Whether a step has what it needs.
 *
 * Only the tree-consuming step can be short of anything in this round.
 * `dosyalar`, `kart` and `birimler` each own their own inputs and ask for them
 * inline, exactly as they do today — gating them on a value they never read
 * would invent a dependency that does not exist. `makine` and `derle` hold
 * nothing of their own yet.
 */
export function readiness(
  session: BuildSession,
  step: StepId,
  /**
   * What `describe_tree` answered about `session.tree.root`, or `null` when
   * nothing has asked yet.
   *
   * `null` is deliberately **not** treated as "wrong": rendering an accusation
   * while the answer is still in flight would be a confident wrong sentence of
   * exactly the kind this round exists to remove.
   */
  treeIsDistribution: boolean | null = null,
  /**
   * **The root actually judged, when the caller has one of its own** (round 3
   * fix wave, Critical 1).
   *
   * Tab 2's list reads `useChainTree`'s tree — the destination when ART has
   * found a build in it, `session.tree.root` otherwise — **so its banner must
   * judge the same one**. Judging the session's tree while the list works on
   * the destination is two answers about one tab: the banner said "none is
   * chosen yet" over a list already answered against the folder the user
   * picked on tab 3.
   *
   * Omitted (`undefined`) means the session's own tree, which is every other
   * caller and what this function did before. `null` is a caller *stating*
   * there is no tree, and is judged as such — it is not the same as not
   * asking.
   */
  treeRoot?: string | null
): Readiness {
  switch (step) {
    case "secim":
      // `secim` holds the package ticks and the first-boot tick in this
      // round, both of which read `useChainTree`'s tree (the destination
      // when it is an ART tree, else the session's).
      //
      // No folder beats a bad one. "Pick one" is the useful sentence, and
      // "that is not a tree" said about nothing would be nonsense.
      if (!hasTree(session, treeRoot)) return "asks";
      return treeIsDistribution === false ? "wrong-folder" : "ready";
    default:
      return "ready";
  }
}

function hasTree(session: BuildSession, treeRoot?: string | null): boolean {
  const root = treeRoot === undefined ? session.tree.root : treeRoot;
  // An empty string is a folder nobody picked — a cleared field writes one,
  // and treating it as a path sends `""` to the backend, where the refusal
  // that comes back names a folder the user never chose.
  return typeof root === "string" && root.length > 0;
}

/**
 * The route for a step — a path, not display text.
 *
 * One place, so a link and a route cannot drift apart: the shell's strip, the
 * drop carry and the build bar's button all build their target from this.
 * Not an i18n key and never rendered as one — the segments are deliberately
 * untranslated, as `STEP_IDS` says.
 */
export function stepPath(step: StepId): string {
  return `/os-builder/${step}`;
}

/** The i18n key for a step's name in the progress strip. */
export function stepLabelKey(step: StepId): string {
  return `osBuilder.step.${step}`;
}

/** The i18n key for a kind's own name — the `hedef` chip in the strip. */
export function kindLabelKey(kind: BuildKind): string {
  switch (kind) {
    case "install":
      return "osBuilder.what.install";
    case "boot-card":
      return "osBuilder.what.bootCard";
    case "prepare-volumes":
      return "osBuilder.what.prepareVolumes";
    case "distro":
      return "osBuilder.what.distro";
  }
}
