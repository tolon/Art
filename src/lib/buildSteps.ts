// The OS Builder's steps: which ones a build has, and whether one can act.
//
// **One step asks one question.** The screen this replaces put ten `<h2>`
// sections in a single scrolling column, and the owner's verdict on driving
// the release build was "bu işletim sistemi kurucusunda akış çok karmaşık
// gereksiz derecede uzun". The steps are sub-routes rather than internal
// state so browser back/forward and a jump to a step work at the router
// level, not through a switch somebody has to keep in sync.
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
 * The steps that exist today.
 *
 * Turkish path segments, matching the design. Paths are deliberately not
 * translated: a URL that changed with the language would break every
 * remembered link and every `builtin.rs::route` value, and a route is not
 * user-facing copy.
 *
 * `bilesenler` (components) and `ozet` (summary) are **not here yet** — the
 * components live inside `OsInstall.tsx` until wave 2 splits it, and the
 * summary is wave 3's own scope. A route that renders nothing is worse than a
 * route that does not exist; §96's "Coming Later" is about actions ART
 * offers, not about empty pages.
 */
export const STEP_IDS = [
  "hedef",
  "kaynak",
  "paketler",
  "amiga-kurulum",
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
      return ["hedef", "kaynak", "paketler", "amiga-kurulum"];
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
 * Only the two tree-consuming steps can be short of anything in this wave.
 * `kaynak`, `kart` and `birimler` each own their own inputs and ask for them
 * inline, exactly as they do today — gating them on a value they never read
 * would invent a dependency that does not exist.
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
  treeIsDistribution: boolean | null = null
): Readiness {
  switch (step) {
    case "paketler":
    case "amiga-kurulum":
      // No folder beats a bad one. "Pick one" is the useful sentence, and
      // "that is not a tree" said about nothing would be nonsense.
      if (!hasTree(session)) return "asks";
      return treeIsDistribution === false ? "wrong-folder" : "ready";
    default:
      return "ready";
  }
}

function hasTree(session: BuildSession): boolean {
  // An empty string is a folder nobody picked — a cleared field writes one,
  // and treating it as a path sends `""` to the backend, where the refusal
  // that comes back names a folder the user never chose.
  return typeof session.tree.root === "string" && session.tree.root.length > 0;
}

/** The i18n key for a step's name in the progress strip. */
export function stepLabelKey(step: StepId): string {
  return `osBuilder.step.${step}`;
}
