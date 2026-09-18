// The one-button card's run: its phases as data, and its endings as
// sentences (round 4, task 10; design § 3, § 5 of
// `2026-09-11-one-button-card-design.md`).
//
// The counterpart of `buildRun.ts` for the card lane. This file decides
// *what the sequence is* and *which sentence says how a phase ended*, and
// nothing else: no `invoke`, no `t()`, no state. `useCardOsRun` runs it and
// owns the session; `BuildTab`'s card mode renders it.
//
// **Four endings, and they stay four** (the owner's Q10, CLAUDE.md's
// *Endings stay distinct*):
//
//   succeeded      nothing to do
//   refused        ART's own rules stopped it before anything was harmed —
//                  the next step is the user's: a name, a file, a setting
//   failed         something broke; the row says where the evidence is, and
//                  the report beside it says what became of the `.partial`
//   stopped        the user pressed Stop; which phase it stopped in
//
// plus the two non-endings a row can be in — `pending` and `running` — and
// `not-attempted`, which is neither a failure nor a silence.
//
// **One literal key per (kind, ending) pair.** Thirty of them where a
// `` `cardRun.phase.${kind}.${state}` `` template would have been one line:
// a template is invisible to `dead-keys.test.ts` and to
// `phrase-keys.test.ts`, so a pair nobody translated would reach the screen
// as a raw dotted key. `pending` and `running` are shared across the six
// kinds, exactly as `buildRun.ts` shares them — they say the same thing about
// every row, and the row's own name is drawn beside them rather than
// interpolated into them (an ending carries no name; the phase does).

import type {
  BuildPhase,
  CardOsPhaseName,
  CardRefusal,
  LeftBehind,
  PartialRemoval,
} from "@/lib/cardOs";
import { errorPhrase } from "@/lib/errorText";
import type { RefusalReason } from "@/lib/osinstall";
import type { Phrase } from "@/lib/phrase";

/**
 * The six kinds of row a card run has.
 *
 * The first three are the OS Builder's own phases, run into the session's
 * `tree/` rather than into a folder the user chose (owner's decision 2). The
 * last three are the card's: the staging `card_os_prepare` does, the pause at
 * the Kickstart agreement (Q8), and `card_os_build` itself.
 *
 * R3: the plan's prose calls this "five phases" — that is the user-visible
 * grouping, with the agreement sitting between prepare and build.
 */
export type CardPhaseKind = "tree" | "package" | "firstboot" | "prepare" | "agree" | "build";

/**
 * How one row ended, or where it is.
 *
 * `running` carries the job's own `done` of `total`, and **`total` is null
 * whenever the job has not said** — the screen then prints a count and draws
 * no bar, because a bar of a guessed width looks like progress and carries
 * none (CLAUDE.md).
 *
 * `stopped.phase` is `null` for every row but the card build's: `BuildPhase`
 * is the *build's* own four-part division (`whdload` · `card` · `partitions`
 * · `check`), and a stopped tree phase is in none of them. The task's brief
 * had it required; naming a build phase for a row that never reached the
 * build would be the screen out-claiming the core.
 */
export type CardPhaseEnding =
  | { state: "pending" }
  | { state: "running"; done: number; total: number | null }
  | { state: "succeeded"; detail?: Phrase }
  | { state: "refused"; refusal: CardRefusal }
  | { state: "failed"; refusal: CardRefusal }
  | { state: "stopped"; phase: BuildPhase | null }
  | { state: "not-attempted" };

export interface CardPhase {
  /** Stable within one run: `"tree"`, `` `package:${packageId}` ``,
   *  `"firstboot"`, `"prepare"`, `"agree"`, `"build"`. */
  id: string;
  kind: CardPhaseKind;
  /** The row's own name: the release, the update's name, the first-boot
   *  script, the card image. Never a translated word (`src/lib` has no `t`). */
  name: string;
  packageId?: string;
  slotId?: string;
  file?: string;
  folder?: string;
  askPrefs?: boolean;
}

export interface CardPhaseReport {
  phase: CardPhase;
  ending: CardPhaseEnding;
  /**
   * An install phase's typed refusals, when it was refused.
   *
   * `CardPhaseEnding.refused` carries a `CardRefusal`, which is what the
   * card's own commands answer with; `osinstall_add_package` answers with
   * this typed list instead, and the list is the actionable half. Carried
   * beside the ending rather than squeezed into it, so the row renders
   * `refusalPhrase(reason)` for each one exactly as tab 4's folder run does.
   */
  refusals?: RefusalReason[];
  /**
   * Which sub-phase the row is in right now, from `CARD_OS_PHASE_EVENT` —
   * the name only. The count beside it is the job's own progress stream
   * (Task 2's ruling): the event says *what* is happening, the job says *how
   * far*, and joining them is `useCardOsRun`'s job.
   */
  now?: CardOsPhaseName;
}

/**
 * The code an install phase's refusal travels under.
 *
 * Its sentence is deliberately **not** here: `CardPhaseReport.refusals`
 * carries the typed reasons and the row renders those, so inventing an
 * English sentence in `src/lib` — which has no translator — would smuggle an
 * untranslatable string onto the screen. `params.count` is what the row's own
 * sentence says, and it is a fact rather than a guess.
 */
export const INSTALL_REFUSED_CODE = "ART-INSTALL-REFUSED";

export function installRefusal(reasons: RefusalReason[]): CardRefusal {
  return {
    code: INSTALL_REFUSED_CODE,
    message: "",
    params: { count: String(reasons.length) },
  };
}

/**
 * The sentence a refusal or a failure puts on screen.
 *
 * `errorPhrase` already knows every card `ART-*` code and falls back to
 * Rust's own English; a refusal ART built itself (an install phase's, or a
 * job that threw before any code was known) has no code, and
 * `errors.verbatim`'s *"Error ID: "* trailer over an empty id claims a code
 * that does not exist.
 */
export function cardRefusalPhrase(refusal: CardRefusal): Phrase | null {
  if (refusal.code === INSTALL_REFUSED_CODE) return null;
  if (!refusal.code) {
    return refusal.message
      ? { key: "errors.verbatimNoId", params: { sentence: refusal.message } }
      : null;
  }
  return errorPhrase(refusal);
}

export interface CardSequenceInputs {
  /** The release, which is the tree row's own name. */
  release: string;
  /** The ticked, runnable update rows, already in the chain's order — this
   *  function does not reorder them, because the order is the chain's answer
   *  and not a preference of the run's. */
  updates: { packageId: string; slotId: string; name: string; file: string }[];
  firstBootWanted: boolean;
  firstBootAskPrefs: boolean;
  /** Where the card image goes — the build row's own name. */
  image: string;
}

/** The folder part of a path, or null when there is none. `buildRun.ts`'s own
 *  helper, imported rather than restated. */
function folderOf(path: string): string | null {
  const cut = Math.max(path.lastIndexOf("\\"), path.lastIndexOf("/"));
  return cut > 0 ? path.slice(0, cut) : null;
}

/**
 * The rows this run will have, in order.
 *
 * **The tree row is always there**, unlike `sequenceFor`'s: the card's tree is
 * built into the session's own folder, which ART made empty a moment ago, so
 * there is no update mode to skip it for.
 */
export function cardSequence(inputs: CardSequenceInputs): CardPhase[] {
  const phases: CardPhase[] = [{ id: "tree", kind: "tree", name: inputs.release }];
  for (const update of inputs.updates) {
    phases.push({
      id: `package:${update.packageId}`,
      kind: "package",
      name: update.name,
      packageId: update.packageId,
      slotId: update.slotId,
      file: update.file,
      folder: folderOf(update.file) ?? undefined,
    });
  }
  if (inputs.firstBootWanted) {
    phases.push({
      id: "firstboot",
      kind: "firstboot",
      name: FIRST_BOOT_PHASE_NAME,
      askPrefs: inputs.firstBootAskPrefs,
    });
  }
  phases.push({ id: "prepare", kind: "prepare", name: inputs.image });
  phases.push({ id: "agree", kind: "agree", name: inputs.image });
  phases.push({ id: "build", kind: "build", name: inputs.image });
  return phases;
}

/** What `core::firstboot::DISPATCHER_PATH` actually writes — the artefact's
 *  own name, the same one `buildRun.ts` uses, restated rather than imported so
 *  the card run does not depend on the folder run's module. */
const FIRST_BOOT_PHASE_NAME = "S:ART-FirstBoot";

// ---------------------------------------------------------------------------
// One literal key per (kind, ending) pair
// ---------------------------------------------------------------------------

function succeededKey(kind: CardPhaseKind): string {
  switch (kind) {
    case "tree":
      return "cardRun.phase.tree.succeeded";
    case "package":
      return "cardRun.phase.package.succeeded";
    case "firstboot":
      return "cardRun.phase.firstboot.succeeded";
    case "prepare":
      return "cardRun.phase.prepare.succeeded";
    case "agree":
      return "cardRun.phase.agree.succeeded";
    case "build":
      return "cardRun.phase.build.succeeded";
  }
}

function refusedKey(kind: CardPhaseKind): string {
  switch (kind) {
    case "tree":
      return "cardRun.phase.tree.refused";
    case "package":
      return "cardRun.phase.package.refused";
    case "firstboot":
      return "cardRun.phase.firstboot.refused";
    case "prepare":
      return "cardRun.phase.prepare.refused";
    case "agree":
      return "cardRun.phase.agree.refused";
    case "build":
      return "cardRun.phase.build.refused";
  }
}

function failedKey(kind: CardPhaseKind): string {
  switch (kind) {
    case "tree":
      return "cardRun.phase.tree.failed";
    case "package":
      return "cardRun.phase.package.failed";
    case "firstboot":
      return "cardRun.phase.firstboot.failed";
    case "prepare":
      return "cardRun.phase.prepare.failed";
    case "agree":
      return "cardRun.phase.agree.failed";
    case "build":
      return "cardRun.phase.build.failed";
  }
}

function stoppedKey(kind: CardPhaseKind): string {
  switch (kind) {
    case "tree":
      return "cardRun.phase.tree.stopped";
    case "package":
      return "cardRun.phase.package.stopped";
    case "firstboot":
      return "cardRun.phase.firstboot.stopped";
    case "prepare":
      return "cardRun.phase.prepare.stopped";
    case "agree":
      return "cardRun.phase.agree.stopped";
    case "build":
      return "cardRun.phase.build.stopped";
  }
}

function notAttemptedKey(kind: CardPhaseKind): string {
  switch (kind) {
    case "tree":
      return "cardRun.phase.tree.notAttempted";
    case "package":
      return "cardRun.phase.package.notAttempted";
    case "firstboot":
      return "cardRun.phase.firstboot.notAttempted";
    case "prepare":
      return "cardRun.phase.prepare.notAttempted";
    case "agree":
      return "cardRun.phase.agree.notAttempted";
    case "build":
      return "cardRun.phase.build.notAttempted";
  }
}

/**
 * What happened to this row, in one sentence — a different key for every
 * (kind, ending) pair.
 *
 * The parameters are the row's own facts and nothing derived: the refusal's
 * `ART-*` code travels as `code` so the short sentence can name it, and the
 * refusal's full sentence is rendered separately by `cardRefusalPhrase`.
 */
export function cardPhasePhrase(kind: CardPhaseKind, ending: CardPhaseEnding): Phrase {
  switch (ending.state) {
    case "pending":
      return { key: "cardRun.phase.pending" };
    case "running":
      return { key: "cardRun.phase.running" };
    case "succeeded":
      return { key: succeededKey(kind) };
    case "refused":
      return { key: refusedKey(kind), params: { code: ending.refusal.code } };
    case "failed":
      return { key: failedKey(kind), params: { code: ending.refusal.code } };
    case "stopped":
      return { key: stoppedKey(kind) };
    case "not-attempted":
      return { key: notAttemptedKey(kind) };
  }
}

/**
 * What to do about it, or `null` when there is nothing to do.
 *
 * `image` is the second argument because the two endings that need it —
 * failed and stopped — have to name **where the evidence is**: a user told
 * "it failed" without being told where what ART wrote went has been given
 * nothing (CLAUDE.md).
 */
export function cardNextStepPhrase(
  ending: CardPhaseEnding,
  image: string | null
): Phrase | null {
  switch (ending.state) {
    case "refused":
      return { key: "cardRun.next.refused" };
    case "failed":
      return image
        ? { key: "cardRun.next.failed", params: { image } }
        : { key: "cardRun.next.failedUnknown" };
    case "stopped":
      return image
        ? { key: "cardRun.next.stopped", params: { image } }
        : { key: "cardRun.next.stoppedUnknown" };
    case "pending":
    case "running":
    case "succeeded":
    case "not-attempted":
      return null;
  }
}

/** How a row is coloured. Never the only signal — every ending says which it
 *  is in words — but the four must not look alike at a glance either. */
export function cardPhaseTone(ending: CardPhaseEnding): "ok" | "warn" | "err" | "muted" {
  switch (ending.state) {
    case "succeeded":
      return "ok";
    case "failed":
      return "err";
    case "refused":
    case "stopped":
      return "warn";
    case "pending":
    case "running":
    case "not-attempted":
      return "muted";
  }
}

/**
 * The live count under a running row, or `null` when there is none.
 *
 * **Two sentences, because there are two facts.** With a total, "N of M";
 * without one, "N so far" — and {@link cardBarFraction} draws a bar only for
 * the first.
 */
export function cardCountPhrase(ending: CardPhaseEnding): Phrase | null {
  if (ending.state !== "running") return null;
  return ending.total === null || ending.total <= 0
    ? { key: "cardRun.count.soFar", params: { done: ending.done } }
    : { key: "cardRun.count.of", params: { done: ending.done, total: ending.total } };
}

/**
 * How full the bar is, or `null` when there may be no bar at all.
 *
 * **The bar rule, stated once** (CLAUDE.md): a fixed width over a total
 * nobody stated looks like progress and carries none. Stated here rather than
 * in the component so a second screen cannot state it differently.
 */
export function cardBarFraction(ending: CardPhaseEnding): number | null {
  if (ending.state !== "running") return null;
  if (ending.total === null || ending.total <= 0) return null;
  return Math.min(1, Math.max(0, ending.done / ending.total));
}

/** The name of the sub-phase `CARD_OS_PHASE_EVENT` just announced. */
export function cardSubPhasePhrase(phase: CardOsPhaseName): Phrase {
  switch (phase) {
    case "prepare":
      return { key: "cardRun.sub.prepare" };
    case "whdload":
      return { key: "cardRun.sub.whdload" };
    case "card":
      return { key: "cardRun.sub.card" };
    case "partitions":
      return { key: "cardRun.sub.partitions" };
    case "check":
      return { key: "cardRun.sub.check" };
  }
}

/** What became of `<image>.partial` — always said, on every ending. Four
 *  states and four sentences: *not removed* is not *removed*, and a file
 *  still on the disk that a screen calls gone is this project's named
 *  defect. */
export function cardPartialPhrase(partial: PartialRemoval): Phrase {
  switch (partial.outcome) {
    case "not-created":
      return { key: "cardRun.partial.notCreated" };
    case "removed":
      return { key: "cardRun.partial.removed", params: { path: partial.path } };
    case "already-gone":
      return { key: "cardRun.partial.alreadyGone", params: { path: partial.path } };
    case "not-removed":
      return {
        key: "cardRun.partial.notRemoved",
        params: { path: partial.path, why: partial.why },
      };
  }
}

/** The session folder ART could not remove, named — or `null` when it went.
 *  A host filesystem has no journal, so this is reported rather than assumed
 *  (architecture.md). */
export function cardScratchLeftPhrase(left: LeftBehind | null): Phrase | null {
  return left ? { key: "cardRun.scratchLeft", params: { path: left.path, why: left.why } } : null;
}
