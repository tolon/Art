// The OS Builder's build run: the sequence, over the wrappers that already
// exist (design § 3.4 of `2026-09-09-os-builder-four-tabs-design.md`).
//
// `buildRun.ts` decides *what the sequence is* and *which sentence says how a
// phase ended*. This hook is the other half: it runs the phases one at a
// time, in order, and gives each one the ending it earned. It calls no
// command of its own — `osinstallApply`, `osinstallAddPackage` and
// `firstbootWrite` are the typed wrappers the screens already use, and the
// jobs go through `awaitJobResult` exactly as `FirstBootRehearse` and
// `AppearancePanel` do.
//
// **The sequence stops at the first ending that is not a success**, and every
// phase after it is *not attempted* — never *failed*, never silently absent.
// Nothing is retried and nothing is undone: each phase leaves the tree where
// the core's own safety rules leave it (`atomic_write`, `guarded_write`),
// which is what makes a stopped run reportable rather than a disaster.
//
// **Where a stopped or broken job's numbers come from.** `awaitJobResult`
// rejects with a *sentence* and nothing else — `"<message> (<code>)"` for a
// failure, the bare `JOB_CANCELLED_MESSAGE` for a cancellation — so the
// rejection alone cannot tell a report how many files landed or which
// `ART-*` code it was. The job's own `job-progress` events carry both, so
// this hook keeps the last progress it saw for the job it is waiting on and
// reads the ending's numbers off that:
//
//   cancelled  `files_landed` from the job's terminal state. Authoritative:
//              Rust sends `Some(files)` for `CancelledPartway` and `None`
//              when nothing durable was written, so `None` is honestly zero.
//   failed     **`done` and `total` from the last progress seen**, carried
//              under those names rather than as a file count, because a failed
//              `JobState` carries `error_code` and `message` and *no count
//              at all* (`commands/jobs.rs`, `core/jobs/mod.rs`) — the plan
//              for this round assumed a `files_landed` that does not exist
//              on that arm. `done` is the job's own counter of whole plan
//              items placed (`apply.rs`'s `place`, reported before each
//              item, so it never runs ahead of what is on disk); it is the
//              only count ART actually has for a failure, and reporting
//              *nothing landed* for a job that wrote a thousand files would
//              be this project's named defect. `null` when no progress was
//              ever seen — which is the case where the command itself threw
//              and no job existed at all.
//   failed     `message`/`error_code` likewise come from the terminal
//              progress when there was one, so the report shows the core's
//              own sentence and its code apart rather than the concatenation
//              the rejection carries. With no progress seen, the rejection's
//              sentence stands and the code is `null` — never invented.
//
// **Cancelling.** `stop()` sets a flag, which is checked *between* phases,
// and asks the job that is running to stop (`jobCancel`). A phase's own
// `cancelled` ending is the job's, never this hook's guess: if the job
// finishes before the cancel lands, that phase succeeded and it is the *next*
// one that is not attempted. `firstbootWrite` is a plain call and cannot be
// cancelled part way (it is one `atomic_write` set), so Stop pressed during
// it lands after it.
//
// **Nothing is set after the screen is gone.** A run outlives its screen —
// the job is Rust's and keeps going — so every state write and both
// callbacks go through a `mounted` guard, and the loop stops before starting
// a phase for a screen nobody is looking at.

import { useCallback, useEffect, useMemo, useRef, useState } from "react";

import type { Phase, PhaseEnding, PhaseReport } from "@/lib/buildRun";
import { firstbootWrite } from "@/lib/firstboot";
import {
  awaitJobResult,
  isJobCancellation,
  jobCancel,
  onJobProgress,
  subscribeSafely,
  type JobProgress,
} from "@/lib/jobs";
import {
  osinstallAddPackage,
  osinstallApply,
  OSINSTALL_ADD_PACKAGE_EVENT,
  OSINSTALL_EVENT,
  type ApplyOutcome,
  type InstallPlan,
  type OsInstallAddPackageResult,
  type OsInstallResult,
  type RefusalReason,
  type SlotOverride,
} from "@/lib/osinstall";

export interface BuildRun {
  /** One per phase of the last (or current) run, in order. */
  reports: PhaseReport[];
  running: boolean;
  /** Every phase reached a terminal ending. An empty report is **not**
   *  finished: nothing having run and everything having run are two different
   *  things to tell somebody. */
  finished: boolean;
  /** Every phase succeeded. */
  succeeded: boolean;
  start(phases: Phase[], plan: InstallPlan | null): void;
  /** Ask the running job to stop; every later phase becomes not attempted. */
  stop(): void;
}

export interface BuildRunArgs {
  destination: string | null;
  /** The tree phase succeeded — the session's `setTree` hand-off (ART-197). */
  onTreeWritten(root: string): void;
  /** The first-boot phase succeeded — `session.firstboot.written`, which
   *  this phase is now the only writer of (round 5 deleted the panel that
   *  used to set it from a screen of its own). */
  onFirstBootWritten(): void;
}

/**
 * `osinstall_add_package`'s typed refusal, thrown only far enough to get out
 * of `awaitJobResult`'s `start` callback and straight back into the ending it
 * belongs to.
 *
 * The refusal has to be raised from inside that callback because that is
 * where `osinstallAddPackage` is invoked, and it is invoked there on purpose:
 * `awaitJobResult` registers its listeners *before* calling `start`, which is
 * the only ordering in which a job that finishes immediately cannot have its
 * result event missed (`jobs.ts`'s own module note — a real race, not a
 * theoretical one). So the refusal travels as a rejection for two stack
 * frames and becomes a `refused` ending here; it is never surfaced as an
 * error, and never reaches `failed`.
 */
class PackageRefused extends Error {
  constructor(readonly refusals: RefusalReason[]) {
    super("package refused");
    this.name = "PackageRefused";
  }
}

function isTerminal(ending: PhaseEnding): boolean {
  switch (ending.state) {
    case "pending":
    case "running":
      return false;
    case "succeeded":
    case "refused":
    case "failed":
    case "cancelled":
    case "not-attempted":
      return true;
  }
}

/** The error's own words when there is no job progress to read them off.
 *  Never rendered here — `src/lib` has no translator (ART-060); the message
 *  travels in the ending and `phaseOutcomePhrase` puts it in a key. */
function messageOf(err: unknown): string {
  return err instanceof Error ? err.message : String(err);
}

export function useBuildRun(args: BuildRunArgs): BuildRun {
  const [reports, setReports] = useState<PhaseReport[]>([]);
  const [running, setRunning] = useState(false);

  /** The caller's own values, read at the moment they are needed rather than
   *  captured: `start` is stable, and a run started with one set of callbacks
   *  must still call the ones the screen holds now. */
  const argsRef = useRef(args);
  argsRef.current = args;

  const mounted = useRef(true);
  const runningRef = useRef(false);
  const stopRequested = useRef(false);
  const currentJobId = useRef<number | null>(null);
  /** The last `job-progress` seen per job id, cleared at every phase
   *  boundary so it holds only the phase that is running. Where a failed or
   *  cancelled ending's numbers come from — see the module note. */
  const lastProgress = useRef(new Map<number, JobProgress>());

  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
    };
  }, []);

  useEffect(
    () =>
      subscribeSafely(() =>
        onJobProgress((job) => {
          lastProgress.current.set(job.id, job);
          if (!mounted.current) return;
          if (job.id !== currentJobId.current) return;
          if (job.state.state !== "running") return;
          setReports((prev) =>
            prev.map((r) =>
              r.ending.state === "running" ? { phase: r.phase, ending: { state: "running", progress: job } } : r
            )
          );
        })
      ),
    []
  );

  const setEnding = useCallback((index: number, ending: PhaseEnding) => {
    if (!mounted.current) return;
    setReports((prev) => prev.map((r, i) => (i === index ? { phase: r.phase, ending } : r)));
  }, []);

  /** Every phase from `from` on that never started. Only a `pending` row is
   *  touched: a phase that already ended keeps the ending it earned. */
  const markNotAttempted = useCallback((from: number) => {
    if (!mounted.current) return;
    setReports((prev) =>
      prev.map((r, i) =>
        i >= from && r.ending.state === "pending"
          ? { phase: r.phase, ending: { state: "not-attempted" } }
          : r
      )
    );
  }, []);

  const runPhase = useCallback(
    async (phase: Phase, plan: InstallPlan | null, destination: string): Promise<PhaseEnding> => {
      lastProgress.current.clear();
      currentJobId.current = null;
      const startedAt = performance.now();

      /** The job id, the moment `awaitJobResult`'s `start` learns it: what
       *  `stop()` cancels, and what the progress stream is filtered by. Stop
       *  pressed in the window before the id existed is honoured here rather
       *  than lost. */
      const noteJob = (id: number): number => {
        currentJobId.current = id;
        if (stopRequested.current) void jobCancel(id).catch(() => {});
        return id;
      };

      try {
        switch (phase.kind) {
          case "tree": {
            // Unreachable by construction — `start` refuses a sequence with a
            // tree phase and no plan — and *not attempted* if it ever were:
            // no command ran, so nothing may be claimed about it.
            if (!plan) return { state: "not-attempted" };
            const result = await awaitJobResult<OsInstallResult, OsInstallResult>(
              OSINSTALL_EVENT,
              async () => noteJob(await osinstallApply(plan, destination)),
              (payload) => payload
            );
            if (mounted.current) argsRef.current.onTreeWritten(destination);
            return {
              state: "succeeded",
              outcome: result.outcome,
              statedRelease: result.stated_release,
              elapsedMs: performance.now() - startedAt,
            };
          }
          case "package": {
            const { packageId, slotId, file, folder } = phase;
            // Same shape as the tree's guard: `sequenceFor` fills both for
            // every package phase, and a phase without them had no command
            // run for it.
            if (!packageId || !folder) return { state: "not-attempted" };
            const overrides: SlotOverride[] | undefined =
              slotId && file ? [[slotId, file]] : undefined;
            const outcome = await awaitJobResult<OsInstallAddPackageResult, ApplyOutcome>(
              OSINSTALL_ADD_PACKAGE_EVENT,
              async () => {
                const answer = await osinstallAddPackage(
                  destination,
                  folder,
                  [packageId],
                  overrides
                );
                if (answer.outcome === "refused") throw new PackageRefused(answer.refusals);
                return noteJob(answer.job_id);
              },
              (payload) => payload.outcome
            );
            return { state: "succeeded", outcome, elapsedMs: performance.now() - startedAt };
          }
          case "firstboot": {
            const written = await firstbootWrite(destination);
            if (mounted.current) argsRef.current.onFirstBootWritten();
            return { state: "succeeded", outcome: written, elapsedMs: performance.now() - startedAt };
          }
        }
      } catch (err) {
        if (err instanceof PackageRefused) return { state: "refused", refusals: err.refusals };
        const last =
          currentJobId.current === null ? undefined : lastProgress.current.get(currentJobId.current);
        if (isJobCancellation(err)) {
          return {
            state: "cancelled",
            // The job's own answer; `None` means nothing durable was written.
            filesLanded: last?.state.state === "cancelled" ? (last.state.files_landed ?? 0) : 0,
          };
        }
        const failure = last?.state.state === "failed" ? last.state : null;
        return {
          state: "failed",
          message: failure ? failure.message : messageOf(err),
          errorCode: failure ? failure.error_code : null,
          // The job's own counter, under its own name: `done` of `total` is
          // whole plan *items* placed, and calling it a file count was the
          // one thing task 2 flagged about this arm. `null` for both when
          // no progress was ever seen — nothing measured, nothing claimed.
          done: last ? last.done : null,
          total: last ? last.total : null,
        };
      }
    },
    []
  );

  const runSequence = useCallback(
    async (phases: Phase[], plan: InstallPlan | null, destination: string) => {
      // **`finally`, not a line at the end** (round 4 task 4, carried from
      // task 2's review). `runPhase` catches its own failures, but anything
      // this loop does outside it — a `setReports` that throws during a
      // render — would leave `runningRef` true for the life of the screen,
      // and the Build button would then do nothing at all with no sentence
      // saying why. The flag is released whatever happens.
      try {
        for (let index = 0; index < phases.length; index++) {
          // Between whole phases, never inside one — the same rule the core
          // itself follows for `is_cancelled()`.
          if (stopRequested.current || !mounted.current) {
            markNotAttempted(index);
            break;
          }
          setEnding(index, { state: "running", progress: null });
          const ending = await runPhase(phases[index], plan, destination);
          currentJobId.current = null;
          if (!mounted.current) break;
          setEnding(index, ending);
          if (ending.state !== "succeeded") {
            markNotAttempted(index + 1);
            break;
          }
        }
      } finally {
        runningRef.current = false;
        if (mounted.current) setRunning(false);
      }
    },
    [markNotAttempted, runPhase, setEnding]
  );

  const start = useCallback(
    (phases: Phase[], plan: InstallPlan | null) => {
      // A run already going is not restarted, and nothing about it changes.
      if (runningRef.current) return;
      const destination = argsRef.current.destination;
      if (destination === null || phases.length === 0) return;
      // A tree phase is `osinstall_apply`, which takes the plan the screen
      // was shown. Without one there is nothing to run and nothing to say —
      // an empty report rather than a row claiming an attempt.
      if (plan === null && phases.some((phase) => phase.kind === "tree")) return;

      runningRef.current = true;
      stopRequested.current = false;
      currentJobId.current = null;
      lastProgress.current.clear();
      setReports(phases.map((phase) => ({ phase, ending: { state: "pending" } })));
      setRunning(true);
      void runSequence(phases, plan, destination);
    },
    [runSequence]
  );

  const stop = useCallback(() => {
    stopRequested.current = true;
    const id = currentJobId.current;
    // Best effort, and never fatal: a job that already ended answers false,
    // and a rejection here must not replace the ending the phase earns.
    if (id !== null) void jobCancel(id).catch(() => {});
  }, []);

  const finished = reports.length > 0 && reports.every((r) => isTerminal(r.ending));
  const succeeded = reports.length > 0 && reports.every((r) => r.ending.state === "succeeded");

  return useMemo(
    () => ({ reports, running, finished, succeeded, start, stop }),
    [reports, running, finished, succeeded, start, stop]
  );
}
