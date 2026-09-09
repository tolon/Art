// The OS Builder's build run: its phases as data, and its endings as
// sentences (design § 3.4 of `2026-09-09-os-builder-four-tabs-design.md`).
//
// The run is one sequence — the tree, then each ticked update in the chain's
// own order, then first boot — and it stops at the first ending that is not a
// success. This file decides *what the sequence is* and *which sentence says
// how a phase ended*, and nothing else: no `invoke`, no `t()`, no state.
// `useBuildRun` runs it, `BuildTab` renders it.
//
// **Five endings per phase, and they stay five.** Succeeded, refused, failed,
// cancelled, and never attempted are five different things to tell somebody,
// and four of them lead to different next actions:
//
//   succeeded      nothing to do
//   refused        here is what ART will not do and what to fix first
//   failed         the core's own words, plus where the tree is and what
//                  landed before it stopped
//   cancelled      you stopped it; same two facts, no error to fix
//   not attempted  the run never got here — nothing was written, nothing
//                  failed, and nothing is claimed about it
//
// Collapsing *not attempted* into *failed* is this project's named defect in
// its purest form: a confident sentence about work that was never done. The
// key families below keep them apart by construction — a missing key is a
// test failure, not a screen that quietly says the wrong thing.

import type { FirstBootWritten } from "@/lib/firstboot";
import type { JobProgress } from "@/lib/jobs";
import type {
  ApplyOutcome,
  InstallPlan,
  InstallRelease,
  RefusalReason,
  StatedRelease,
  TreeSummary,
} from "@/lib/osinstall";
import type { Phrase } from "@/lib/phrase";
import { size } from "@/lib/size";

export type PhaseKind = "tree" | "package" | "firstboot";

/**
 * What the first-boot phase is called on screen.
 *
 * The artefact's own name, not a translated label: `src/lib` has no
 * translator, and hard-coding an English phrase here would smuggle an
 * untranslatable string past both catalogues. `S:ART-FirstBoot` is what
 * `core::firstboot::DISPATCHER_PATH` actually writes, so the row names the
 * thing it will put in the tree — the same rule the tree phase (named for the
 * release) and an update phase (named for the package) follow.
 */
export const FIRST_BOOT_PHASE_NAME = "S:ART-FirstBoot";

export interface Phase {
  /** Stable within one sequence: `"tree"`, `` `package:${packageId}` ``,
   *  `"firstboot"` — the React key, and how a report finds its phase. */
  id: string;
  kind: PhaseKind;
  /** The row's own name: the release, the update's name, or the first-boot
   *  script. Never a translated word — see {@link FIRST_BOOT_PHASE_NAME}. */
  name: string;
  packageId?: string;
  slotId?: string;
  /** The archive the update was actually found in (the slot's own answer),
   *  and the folder it sits in — `osinstall_add_package` takes both. */
  file?: string;
  folder?: string;
}

/**
 * How one phase ended, or where it is.
 *
 * `cancelled.filesLanded` is the job's own `files_landed` (`jobs.ts`,
 * ART-058): how many files were written and left in place when it stopped. A
 * job reports `null` when nothing was, which is why
 * {@link phaseNextStepPhrase} may honestly say zero for it.
 *
 * **A failed phase carries no such count, and does not pretend to** (round 4
 * task 4, carried from task 2's review). `JobState::Failed` has an
 * `error_code` and a `message` and nothing else — the only number ART holds
 * for a broken job is the progress stream's own `done` of `total`, which
 * counts whole plan **items** (directories included) rather than files. So
 * the failed arm carries those two under their own names and the sentence
 * says *"the work stopped at item N of M"*: reporting them as a file count
 * would be a confident wrong number, and reporting nothing would tell
 * somebody whose thousand files are on disk that nothing was written.
 */
export type PhaseEnding =
  | { state: "pending" }
  | { state: "running"; progress: JobProgress | null }
  | {
      state: "succeeded";
      outcome: ApplyOutcome | FirstBootWritten;
      /** Only the tree phase has one — `osinstall-result` carries it. */
      statedRelease?: StatedRelease;
      elapsedMs: number;
    }
  | { state: "refused"; refusals: RefusalReason[] }
  | {
      state: "failed";
      message: string;
      errorCode: string | null;
      /** The job's own progress counter when it stopped — whole plan items
       *  placed, not files. `null` when no progress was ever seen, which is
       *  the case where the command itself threw and no job existed. */
      done: number | null;
      /** How many items that job had to do, when it said. `null` for a job
       *  that reported no total (`JobProgress.total` is nullable). */
      total: number | null;
    }
  | { state: "cancelled"; filesLanded: number }
  | { state: "not-attempted" };

export interface PhaseReport {
  phase: Phase;
  ending: PhaseEnding;
}

export interface SequenceInputs {
  destination: string;
  /** True when the destination is already a tree ART built. Then there is no
   *  tree phase at all — `osinstall_apply` refuses a destination with
   *  anything in it (`refuse_unless_free`), so an existing tree is updated
   *  rather than rebuilt. */
  destinationIsTree: boolean;
  release: InstallRelease;
  /** The ticked, runnable, host-placeable update rows, already in the chain's
   *  order — this function does not reorder them, because the order is the
   *  chain's answer and not a preference of the run's. */
  updates: {
    packageId: string;
    slotId: string;
    name: string;
    file: string;
  }[];
  firstBootWanted: boolean;
}

/**
 * The folder part of a path, or null when there is none.
 *
 * Moved here from `AmigaInstallPanel.tsx` (round 4, task 1) because the build
 * run needs the same answer: `osinstall_add_package` is handed the folder a
 * host-placed row's archive was actually **found** in, rather than an assumed
 * archives folder. This build's material is a list of folders and the file may
 * be in any of them; the slot already knows which.
 *
 * No path building happens here — the answer is a prefix of a path Rust itself
 * produced, and Rust validates it again on the way back in.
 */
export function folderOf(path: string): string | null {
  const cut = Math.max(path.lastIndexOf("\\"), path.lastIndexOf("/"));
  return cut > 0 ? path.slice(0, cut) : null;
}

/** The phases this build will run, in order. */
export function sequenceFor(inputs: SequenceInputs): Phase[] {
  const phases: Phase[] = [];
  if (!inputs.destinationIsTree) {
    phases.push({ id: "tree", kind: "tree", name: inputs.release });
  }
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
    phases.push({ id: "firstboot", kind: "firstboot", name: FIRST_BOOT_PHASE_NAME });
  }
  return phases;
}

/** An `ApplyOutcome` counts files as a number; `FirstBootWritten` lists them.
 *  Discriminated on the shape rather than on the phase's kind, so a wrongly
 *  paired outcome cannot be read as the other one's fields.
 *
 *  Exported for the tree phase's report row, which renders `ApplyOutcome`'s
 *  own `removed` and `icons` verdicts and has to narrow the union to get at
 *  them — a cast there would be the screen asserting what this guard checks. */
export function isFirstBootWritten(
  outcome: ApplyOutcome | FirstBootWritten
): outcome is FirstBootWritten {
  return Array.isArray(outcome.files);
}

/**
 * The two numbers a written phase reports.
 *
 * The `FirstBootWritten` arm is unreachable for the two job phases — the run
 * pairs each outcome with the phase that produced it — and it is here so that
 * a mis-pairing prints its own list's length rather than reading a number off
 * a field that does not exist. `size(0)` for the bytes, because a first-boot
 * write is not weighed and a size ART never measured is not one it may print.
 */
function writtenCounts(outcome: ApplyOutcome | FirstBootWritten): {
  files: number;
  bytes: string;
} {
  return isFirstBootWritten(outcome)
    ? { files: outcome.files.length, bytes: size(0) }
    : { files: outcome.files, bytes: size(outcome.bytes) };
}

function succeededPhrase(phase: Phase, outcome: ApplyOutcome | FirstBootWritten): Phrase {
  switch (phase.kind) {
    case "tree":
      return { key: "osBuilder.build.phase.tree.succeeded", params: writtenCounts(outcome) };
    case "package":
      return {
        key: "osBuilder.build.phase.package.succeeded",
        params: { name: phase.name, ...writtenCounts(outcome) },
      };
    case "firstboot":
      // No byte count — see `writtenCounts`.
      return {
        key: "osBuilder.build.phase.firstboot.succeeded",
        params: { files: isFirstBootWritten(outcome) ? outcome.files.length : outcome.files },
      };
  }
}

function refusedPhrase(phase: Phase): Phrase {
  switch (phase.kind) {
    case "tree":
      return { key: "osBuilder.build.phase.tree.refused" };
    case "package":
      return { key: "osBuilder.build.phase.package.refused", params: { name: phase.name } };
    case "firstboot":
      return { key: "osBuilder.build.phase.firstboot.refused" };
  }
}

function failedPhrase(phase: Phase, message: string): Phrase {
  switch (phase.kind) {
    case "tree":
      return { key: "osBuilder.build.phase.tree.failed", params: { message } };
    case "package":
      return {
        key: "osBuilder.build.phase.package.failed",
        params: { name: phase.name, message },
      };
    case "firstboot":
      return { key: "osBuilder.build.phase.firstboot.failed", params: { message } };
  }
}

function cancelledPhrase(phase: Phase): Phrase {
  switch (phase.kind) {
    case "tree":
      return { key: "osBuilder.build.phase.tree.cancelled" };
    case "package":
      return { key: "osBuilder.build.phase.package.cancelled", params: { name: phase.name } };
    case "firstboot":
      return { key: "osBuilder.build.phase.firstboot.cancelled" };
  }
}

function notAttemptedPhrase(phase: Phase): Phrase {
  switch (phase.kind) {
    case "tree":
      return { key: "osBuilder.build.phase.tree.notAttempted" };
    case "package":
      return { key: "osBuilder.build.phase.package.notAttempted", params: { name: phase.name } };
    case "firstboot":
      return { key: "osBuilder.build.phase.firstboot.notAttempted" };
  }
}

/**
 * What happened to this phase, in one sentence — **a different key for every
 * (kind, ending) pair**.
 *
 * Fifteen literal keys where a `${kind}.${state}` template would have been
 * three lines: the literals are what `dead-keys.test.ts` can see, and they are
 * what makes a missing translation a failing test instead of a raw dotted key
 * on screen.
 */
export function phaseOutcomePhrase(report: PhaseReport): Phrase {
  const { phase, ending } = report;
  switch (ending.state) {
    case "pending":
      return { key: "osBuilder.build.phase.pending" };
    case "running":
      return { key: "osBuilder.build.phase.running", params: { name: phase.name } };
    case "succeeded":
      return succeededPhrase(phase, ending.outcome);
    case "refused":
      return refusedPhrase(phase);
    case "failed":
      return failedPhrase(phase, ending.message);
    case "cancelled":
      return cancelledPhrase(phase);
    case "not-attempted":
      return notAttemptedPhrase(phase);
  }
}

/**
 * What to do about it, or null when there is nothing to do.
 *
 * `destination` is the second argument because the two endings that need it —
 * failed and cancelled — have to name **where the tree is**. A `PhaseReport`
 * carries what the phase did; the destination is the run's, and a report told
 * "it failed" without being told where the evidence went has been given
 * nothing (CLAUDE.md, "never claim what you did not do").
 */
export function phaseNextStepPhrase(report: PhaseReport, destination: string): Phrase | null {
  switch (report.ending.state) {
    case "refused":
      return { key: "osBuilder.build.next.refused" };
    case "failed":
      // **Only when the job actually counted.** `done`/`total` are the job's
      // own progress, and a job that never reported any has told ART nothing
      // about how far it got — so the sentence says where the tree is and
      // stops, rather than printing a zero it did not measure.
      return report.ending.done !== null && report.ending.total !== null
        ? {
            key: "osBuilder.build.next.failed",
            params: {
              root: destination,
              done: report.ending.done,
              total: report.ending.total,
            },
          }
        : { key: "osBuilder.build.next.failedUnknown", params: { root: destination } };
    case "cancelled":
      return {
        key: "osBuilder.build.next.cancelled",
        params: { root: destination, files: report.ending.filesLanded },
      };
    case "pending":
    case "running":
    case "succeeded":
    case "not-attempted":
      return null;
  }
}

/** How a phase's row is coloured. Never the only signal — every ending
 *  already says which it is in words — but a success, a refusal and a phase
 *  nobody reached must not look alike at a glance either. */
export function phaseTone(report: PhaseReport): "ok" | "warn" | "err" | "muted" {
  switch (report.ending.state) {
    case "succeeded":
      return "ok";
    case "failed":
      return "err";
    case "refused":
    case "cancelled":
      return "warn";
    case "pending":
    case "running":
    case "not-attempted":
      return "muted";
  }
}

/**
 * What the fourth summary line may claim, and it is **four different things**
 * rather than a number and a null (round 4 task 4, fix round 1's C1 and I5).
 *
 * `components` is the fresh-build answer: the component preview partitions
 * what the release's own parts would place into landed-on-nothing,
 * landed-on-identical-bytes and replaced-something, and the ticked updates
 * add to the last of those alone (`osinstall_collisions` answers a list of
 * collisions and nothing else — it has no `placed`/`contested`).
 *
 * `updates` is the **update-mode** answer, and it exists because the other
 * one was a claim about work that will not run: with the destination already
 * a tree there is no tree phase, so the release's parts are not placed at
 * all and their preview describes a build nobody asked for. Only the ticked
 * updates' own count survives.
 *
 * `failed` is the preview ART could not produce, which is not the preview
 * that found nothing (§89) and not one still coming either.
 */
export type ReplacesSummary =
  | { state: "components"; fresh: number; unchanged: number; replaced: number }
  | { state: "updates"; replaced: number }
  | { state: "failed"; detail: string }
  | { state: "pending" };

export interface RunSummaryArgs {
  plan: InstallPlan | null;
  destinationIsTree: boolean;
  treeSummary: TreeSummary | null;
  updates: SequenceInputs["updates"];
  firstBootWanted: boolean;
  /**
   * Whether ART has finished looking at the destination folder.
   *
   * `false` means *neither* "a fresh build" nor "an existing tree" may be
   * said yet: the two are one round trip apart, and the first line is the
   * sentence that states which of them this build is. A screen that guesses
   * for a render and then swaps is the confident-wrong screen at its
   * shortest-lived.
   */
  destinationChecked: boolean;
  /** The collision preview — see {@link ReplacesSummary}. */
  replaces: ReplacesSummary;
}

/**
 * The four lines above the Build button: what will be written, what will be
 * updated, whether first boot is on, and what would be replaced.
 *
 * Three of them always have something to say. The first does not: with no plan
 * (or no summary of the tree being updated) there is no line, because the
 * plan's own refusal is what stands in the button's place, and a line invented
 * here would out-claim it.
 */
export function runSummaryLines(args: RunSummaryArgs): Phrase[] {
  const lines: Phrase[] = [];

  if (!args.destinationChecked) {
    // One round trip decides whether this is a fresh build or an update, and
    // that is what the first line states. Until it lands the line says it is
    // being looked at, which is a fact rather than a guess with a 50 % hit
    // rate that swaps under the reader.
    lines.push({ key: "osBuilder.build.summary.checking" });
  } else if (args.destinationIsTree) {
    if (args.treeSummary) {
      lines.push({
        key: "osBuilder.build.summary.treeExisting",
        params: {
          // `release` is null only for a folder that is not a tree, which
          // this branch is not; the fallback follows `MachineTab`'s own.
          release: args.treeSummary.release ?? "",
          count: args.treeSummary.files,
        },
      });
    }
  } else if (args.plan) {
    lines.push({
      key: "osBuilder.build.summary.tree",
      params: {
        // `totalFiles`, not `items.length`: the second is the work, and a
        // file two components both write is planned twice and lands once
        // (ART-205). The line says what the tree will hold.
        files: args.plan.totalFiles,
        bytes: size(args.plan.totalBytes),
        // Raw component ids. A component with a catalogue can replace this
        // param with its own resolved labels — the way `BuildTab.tsx`'s
        // `label()` resolves `resident-table-unreadable`'s component — since
        // `src/lib` has no catalogue to resolve them with.
        components: args.plan.componentsOn.join(", "),
      },
    });
  }

  lines.push(
    args.updates.length === 0
      ? { key: "osBuilder.build.summary.updatesNone" }
      : {
          key: "osBuilder.build.summary.updates",
          params: {
            count: args.updates.length,
            names: args.updates.map((u) => u.name).join(" → "),
          },
        }
  );

  lines.push({
    key: args.firstBootWanted
      ? "osBuilder.build.summary.firstboot"
      : "osBuilder.build.summary.firstbootOff",
  });

  lines.push(replacesLine(args.replaces));

  return lines;
}

/** The fourth line, one arm per {@link ReplacesSummary} state — four literal
 *  keys, because four different things are being said and a reader shown one
 *  of them infers the others wrongly. */
function replacesLine(replaces: ReplacesSummary): Phrase {
  switch (replaces.state) {
    case "components":
      return {
        key: "osBuilder.build.summary.replaces",
        params: {
          replaced: replaces.replaced,
          fresh: replaces.fresh,
          unchanged: replaces.unchanged,
        },
      };
    case "updates":
      return {
        key: "osBuilder.build.summary.replacesUpdates",
        params: { replaced: replaces.replaced },
      };
    case "failed":
      // The install screen's own key for a preview that could not be
      // produced, reused rather than reworded: it is the same fact, and one
      // sentence for it is one sentence to keep true.
      return { key: "osinstall.replaces.failed", params: { error: replaces.detail } };
    case "pending":
      return { key: "osBuilder.build.summary.replacesPending" };
  }
}
