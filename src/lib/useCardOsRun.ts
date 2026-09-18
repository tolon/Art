// The one-button card's run (round 4, task 10): one session, six rows, four
// endings.
//
// `cardOsRun.ts` decides *what the rows are* and *which sentence says how one
// ended*. This hook is the other half: it opens the session, runs the rows one
// at a time in order, pauses at the Kickstart agreement, and closes the
// session **exactly once on every path**.
//
// **The session is the whole point.** `card_os_open` is called when the user
// presses the button and never on mount (Q9, ART-344): a session opened by
// looking at a tab is a scratch folder nobody asked for. From then on:
//
//   the user gave up            `card_os_close` — the folder goes
//   a row before the build      `card_os_close` — the folder goes
//   `card_os_build` started     **no close**: Rust's own `end_build` removes
//                               the session after *every* ending, so a close
//                               here would be a second answer to a settled
//                               question — and an error about a session that
//                               is already gone
//
// `closeSession()` is the one place that calls it and it no-ops once the
// session is gone, so *exactly once* is a property of the code rather than of
// the path taken through it.
//
// **The phase name and the count come from two places, joined here** (Task
// 2's ruling). `CARD_OS_PHASE_EVENT` says *what* the core is doing — prepare,
// whdload, card, partitions, check — and the job's own progress stream says
// *how far*. A phase that cannot know its total reports `total: null`, and
// `cardBarFraction` refuses a bar for it.
//
// **The sequence stops at the first ending that is not a success**, and every
// row after it is *not attempted* — never *failed*, never silently absent.
//
// **Nothing is set after the screen is gone.** A run outlives its screen — the
// jobs are Rust's and keep going — so every state write goes through a
// `mounted` guard, the same shape `useBuildRun` uses.

import { useCallback, useEffect, useMemo, useRef, useState } from "react";

import { firstBootAsksPrefs } from "@/lib/buildSession";
import {
  cardOsBuild,
  cardOsClose,
  cardOsOpen,
  cardOsPrepare,
  onCardOsPhase,
  CARD_OS_BUILD_EVENT,
  CARD_OS_PREPARE_EVENT,
  type CardOsBuildRequest,
  type CardOsBuildResult,
  type CardOsEnding,
  type CardOsPrepareRequest,
  type CardOsPrepareResult,
  type CardRefusal,
  type KickstartProposal,
  type LeftBehind,
  type PreparedCard,
} from "@/lib/cardOs";
import {
  installRefusal,
  type CardPhase,
  type CardPhaseEnding,
  type CardPhaseReport,
} from "@/lib/cardOsRun";
import { firstbootWrite } from "@/lib/firstboot";
import {
  awaitJobResult,
  isJobCancellation,
  jobCancel,
  JobRefused,
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
} from "@/lib/osinstall";
import { size } from "@/lib/size";

/** Everything one press of the card's Build button needs. */
export interface CardRunInput {
  phases: CardPhase[];
  /** The plan the tree row runs. Without one there is no run at all. */
  plan: InstallPlan | null;
  /** `card_os_prepare`'s request minus the session, which is this hook's. */
  prepare: Omit<CardOsPrepareRequest, "session">;
  /** `card_os_build`'s request minus the session and the agreed Kickstarts,
   *  which are this hook's and the user's. */
  build: Omit<CardOsBuildRequest, "session" | "agreedKickstarts">;
}

export interface CardOsRunArgs {
  /**
   * The session's own `tree/`, the moment `card_os_open` answers it — and
   * `null` again the moment the session closes.
   *
   * **This is how the card section and the run agree about which tree is
   * measured** (Task 8's carry). The section measures against
   * `session.tree.root`; the card's tree only exists inside a session, so the
   * run hands it over while it lives and takes it back when it goes. Before a
   * run the section says its own sentence — *sizes appear once this build has
   * a system tree to measure* — which is true and asks the user for nothing
   * they cannot give.
   */
  onSessionTree(tree: string | null): void;
}

export interface CardOsRun {
  /** One row per phase of the last (or current) run, in order. */
  reports: CardPhaseReport[];
  running: boolean;
  /** Every row reached a terminal ending. An empty report is **not**
   *  finished: nothing having run and everything having run are two different
   *  things to tell somebody. */
  finished: boolean;
  /** Every row succeeded. */
  succeeded: boolean;
  /** The session's tree while there is a session, `null` otherwise. */
  tree: string | null;
  /** The proposal the run is paused on, or `null`. */
  proposal: KickstartProposal | null;
  /** True while the run is waiting for the user at the agreement. */
  awaitingAgreement: boolean;
  /** The build's own answer, whatever its ending. */
  result: CardOsBuildResult | null;
  /** A session folder ART could not remove on a give-up. Never claimed gone. */
  scratchLeft: LeftBehind | null;
  /** `card_os_open` itself refused — no row ever ran. */
  openFailure: CardRefusal | null;
  start(input: CardRunInput): void;
  /** The user agreed to these Kickstart names; the build starts. */
  agree(names: string[]): void;
  /** Ask the running job to stop; every later row becomes not attempted. */
  stop(): void;
  /** Give the run up (at the agreement, or before it): the session goes. */
  giveUp(): void;
}

/**
 * `osinstall_add_package`'s typed refusal, thrown only far enough to get out
 * of `awaitJobResult`'s `start` callback and straight back into the ending it
 * belongs to — `useBuildRun`'s own device, and for its own reason: the command
 * is invoked inside that callback so the result listener is registered before
 * it runs.
 */
class PackageRefused extends Error {
  constructor(readonly refusals: RefusalReason[]) {
    super("package refused");
    this.name = "PackageRefused";
  }
}

/** The user gave the run up at the agreement. Its own class so the loop can
 *  tell it from a job that stopped. */
class GaveUp extends Error {
  constructor() {
    super("gave up");
    this.name = "GaveUp";
  }
}

/** What one row's run answers with: its ending, plus the typed refusals only
 *  an install row can have. One shape for every row, so the loop never has to
 *  ask which kind it just ran. */
interface RowAnswer {
  ending: CardPhaseEnding;
  refusals?: RefusalReason[];
}

function isTerminal(ending: CardPhaseEnding): boolean {
  switch (ending.state) {
    case "pending":
    case "running":
      return false;
    case "succeeded":
    case "refused":
    case "failed":
    case "stopped":
    case "not-attempted":
      return true;
  }
}

/** The error's own words when nothing typed came back. Never rendered here —
 *  `src/lib` has no translator; it travels in the refusal and the screen puts
 *  it through `cardRefusalPhrase`. */
function messageOf(err: unknown): string {
  return err instanceof Error ? err.message : String(err);
}

/** `CardOsEnding` — the build's own four endings — as this run's row. */
function endingOfBuild(ending: CardOsEnding): CardPhaseEnding {
  switch (ending.ending) {
    case "succeeded":
      return { state: "succeeded" };
    case "refused":
      return {
        state: "refused",
        refusal: { code: ending.code, message: ending.message, params: ending.params },
      };
    case "failed":
      return {
        state: "failed",
        refusal: { code: ending.code, message: ending.message, params: ending.params },
      };
    case "stopped":
      return { state: "stopped", phase: ending.phase };
  }
}

export function useCardOsRun(args: CardOsRunArgs): CardOsRun {
  const [reports, setReports] = useState<CardPhaseReport[]>([]);
  const [running, setRunning] = useState(false);
  const [tree, setTree] = useState<string | null>(null);
  const [proposal, setProposal] = useState<KickstartProposal | null>(null);
  const [awaitingAgreement, setAwaitingAgreement] = useState(false);
  const [result, setResult] = useState<CardOsBuildResult | null>(null);
  const [scratchLeft, setScratchLeft] = useState<LeftBehind | null>(null);
  const [openFailure, setOpenFailure] = useState<CardRefusal | null>(null);

  /** The caller's own callbacks, read when they are needed rather than
   *  captured: `start` is stable and a run must call the ones the screen
   *  holds now. */
  const argsRef = useRef(args);
  argsRef.current = args;

  const mounted = useRef(true);
  const runningRef = useRef(false);
  const stopRequested = useRef(false);
  const currentJobId = useRef<number | null>(null);
  /** The session, and whether it is still this side's to close. */
  const sessionRef = useRef<{ id: number; live: boolean } | null>(null);
  /** The agreement's own promise handles, while the run waits at it. */
  const agreeRef = useRef<{
    resolve: (names: string[]) => void;
    reject: (err: unknown) => void;
  } | null>(null);
  /** What the user agreed to, read by the build row. */
  const agreedRef = useRef<string[]>([]);

  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
    };
  }, []);

  /** The live count: the job's own `done` of `total`, on whichever row is
   *  running. `total` stays `null` when the job did not say (the bar rule). */
  useEffect(
    () =>
      subscribeSafely(() =>
        onJobProgress((job: JobProgress) => {
          if (!mounted.current) return;
          if (job.id !== currentJobId.current) return;
          if (job.state.state !== "running") return;
          setReports((prev) =>
            prev.map((row) =>
              row.ending.state === "running"
                ? { ...row, ending: { state: "running", done: job.done, total: job.total } }
                : row
            )
          );
        })
      ),
    []
  );

  /** The phase **name**: what the core says it is doing now. The count beside
   *  it is the stream above — two sources, joined here (Task 2's ruling). */
  useEffect(
    () =>
      subscribeSafely(() =>
        onCardOsPhase((event) => {
          if (!mounted.current) return;
          if (event.jobId !== currentJobId.current) return;
          setReports((prev) =>
            prev.map((row) => (row.ending.state === "running" ? { ...row, now: event.phase } : row))
          );
        })
      ),
    []
  );

  const setRow = useCallback(
    (index: number, ending: CardPhaseEnding, refusals?: RefusalReason[]) => {
      if (!mounted.current) return;
      setReports((prev) =>
        prev.map((row, at) => (at === index ? { phase: row.phase, ending, refusals } : row))
      );
    },
    []
  );

  /** Every row from `from` on that never started. Only a `pending` row is
   *  touched: a row that already ended keeps the ending it earned. */
  const markNotAttempted = useCallback((from: number) => {
    if (!mounted.current) return;
    setReports((prev) =>
      prev.map((row, at) =>
        at >= from && row.ending.state === "pending"
          ? { phase: row.phase, ending: { state: "not-attempted" } }
          : row
      )
    );
  }, []);

  /**
   * Close the session — **the only place that does**, and a no-op once it is
   * gone. A build that started hands the session to Rust, whose `end_build`
   * removes it after every ending, so `live` is false from that moment.
   */
  const closeSession = useCallback(async () => {
    const session = sessionRef.current;
    sessionRef.current = null;
    if (mounted.current) {
      setTree(null);
    }
    argsRef.current.onSessionTree(null);
    if (!session || !session.live) return;
    try {
      const closed = await cardOsClose(session.id);
      if (mounted.current) setScratchLeft(closed.scratchLeft);
    } catch {
      // A session Rust has already forgotten is not news, and a close that
      // fails must not replace the ending the run earned.
    }
  }, []);

  const runRow = useCallback(
    async (
      phase: CardPhase,
      input: CardRunInput,
      session: number,
      treeRoot: string
    ): Promise<RowAnswer> => {
      currentJobId.current = null;

      /** The job id, the moment `awaitJobResult`'s `start` learns it: what
       *  `stop()` cancels, and what both event streams are filtered by. */
      const noteJob = (id: number): number => {
        currentJobId.current = id;
        if (stopRequested.current) void jobCancel(id).catch(() => {});
        return id;
      };

      try {
        switch (phase.kind) {
          case "tree": {
            // Unreachable by construction — `start` refuses a sequence with no
            // plan — and *not attempted* if it ever were: no command ran, so
            // nothing may be claimed about it.
            const plan = input.plan;
            if (!plan) return { ending: { state: "not-attempted" } };
            const answer = await awaitJobResult<OsInstallResult, OsInstallResult>(
              OSINSTALL_EVENT,
              async () => noteJob(await osinstallApply(plan, treeRoot)),
              (payload) => payload
            );
            return {
              ending: {
                state: "succeeded",
                detail: {
                  key: "cardRun.detail.written",
                  params: { files: answer.outcome.files, bytes: size(answer.outcome.bytes) },
                },
              },
            };
          }
          case "package": {
            const { packageId, folder, slotId, file } = phase;
            if (!packageId || !folder) return { ending: { state: "not-attempted" } };
            const overrides = slotId && file ? [[slotId, file] as [string, string]] : undefined;
            const outcome = await awaitJobResult<OsInstallAddPackageResult, ApplyOutcome>(
              OSINSTALL_ADD_PACKAGE_EVENT,
              async () => {
                const answer = await osinstallAddPackage(treeRoot, folder, [packageId], overrides);
                if (answer.outcome === "refused") throw new PackageRefused(answer.refusals);
                return noteJob(answer.job_id);
              },
              (payload) => payload.outcome
            );
            return {
              ending: {
                state: "succeeded",
                detail: {
                  key: "cardRun.detail.written",
                  params: { files: outcome.files, bytes: size(outcome.bytes) },
                },
              },
            };
          }
          case "firstboot": {
            const written = await firstbootWrite(treeRoot, firstBootAsksPrefs(phase.askPrefs));
            return {
              ending: {
                state: "succeeded",
                detail: written.userStartupBackup
                  ? {
                      key: "osBuilder.build.phase.firstboot.backup",
                      params: { path: written.userStartupBackup },
                    }
                  : written.userStartupCreated
                    ? { key: "osBuilder.build.phase.firstboot.created" }
                    : undefined,
              },
            };
          }
          case "prepare": {
            const prepared = await awaitJobResult<CardOsPrepareResult, PreparedCard>(
              CARD_OS_PREPARE_EVENT,
              async () => noteJob(await cardOsPrepare({ ...input.prepare, session })),
              (payload) => payload.prepared
            );
            if (mounted.current) setProposal(prepared.kickstarts);
            return { ending: { state: "succeeded" } };
          }
          case "agree": {
            // **A real pause.** Nothing is ticked by ART (the owner's rule of
            // 2026-08-21), so the build cannot start until the user acts —
            // and ticking nothing is an answer too.
            if (mounted.current) setAwaitingAgreement(true);
            try {
              agreedRef.current = await new Promise<string[]>((resolve, reject) => {
                agreeRef.current = { resolve, reject };
              });
              return { ending: { state: "succeeded" } };
            } finally {
              agreeRef.current = null;
              if (mounted.current) setAwaitingAgreement(false);
            }
          }
          case "build": {
            const answer = await awaitJobResult<CardOsBuildResult, CardOsBuildResult>(
              CARD_OS_BUILD_EVENT,
              async () => {
                const id = await cardOsBuild({
                  ...input.build,
                  session,
                  agreedKickstarts: agreedRef.current,
                });
                // From here the session is Rust's: `end_build` removes it
                // after every ending, so this side must never close it.
                sessionRef.current = { id: session, live: false };
                return noteJob(id);
              },
              (payload) => payload
            );
            if (mounted.current) {
              setResult(answer);
              setScratchLeft(answer.scratchLeft);
            }
            return { ending: endingOfBuild(answer.ending) };
          }
        }
      } catch (err) {
        if (err instanceof PackageRefused) {
          return {
            ending: { state: "refused", refusal: installRefusal(err.refusals) },
            refusals: err.refusals,
          };
        }
        if (err instanceof GaveUp || isJobCancellation(err)) {
          return { ending: { state: "stopped", phase: null } };
        }
        if (err instanceof JobRefused) {
          return {
            ending: {
              state: "refused",
              refusal: { code: err.code, message: err.message, params: {} },
            },
          };
        }
        return {
          ending: { state: "failed", refusal: { code: "", message: messageOf(err), params: {} } },
        };
      }
    },
    []
  );

  const runSequence = useCallback(
    async (input: CardRunInput) => {
      try {
        // **Only when the user pressed the button** (Q9): a session opened on
        // mount is a scratch folder nobody asked for.
        let opened: { session: number; tree: string };
        try {
          opened = await cardOsOpen();
        } catch (err) {
          if (mounted.current) {
            setOpenFailure({ code: "", message: messageOf(err), params: {} });
            markNotAttempted(0);
          }
          return;
        }
        sessionRef.current = { id: opened.session, live: true };
        if (mounted.current) setTree(opened.tree);
        argsRef.current.onSessionTree(opened.tree);

        for (let index = 0; index < input.phases.length; index++) {
          // Between whole rows, never inside one — the same rule the core
          // itself follows for `is_cancelled()`.
          if (stopRequested.current || !mounted.current) {
            markNotAttempted(index);
            break;
          }
          setRow(index, { state: "running", done: 0, total: null });
          const answer = await runRow(input.phases[index], input, opened.session, opened.tree);
          currentJobId.current = null;
          if (!mounted.current) break;
          const { ending, refusals } = answer;
          setRow(index, ending, refusals);
          if (ending.state !== "succeeded") {
            markNotAttempted(index + 1);
            break;
          }
        }
      } finally {
        await closeSession();
        runningRef.current = false;
        if (mounted.current) setRunning(false);
      }
    },
    [closeSession, markNotAttempted, runRow, setRow]
  );

  const start = useCallback(
    (input: CardRunInput) => {
      if (runningRef.current) return;
      if (input.phases.length === 0) return;
      // The tree row is `osinstall_apply`, which takes the plan the screen was
      // shown. Without one there is nothing to run and nothing to say.
      if (input.plan === null && input.phases.some((phase) => phase.kind === "tree")) return;

      runningRef.current = true;
      stopRequested.current = false;
      currentJobId.current = null;
      agreedRef.current = [];
      setOpenFailure(null);
      setScratchLeft(null);
      setProposal(null);
      setResult(null);
      setReports(input.phases.map((phase) => ({ phase, ending: { state: "pending" } })));
      setRunning(true);
      void runSequence(input);
    },
    [runSequence]
  );

  const agree = useCallback((names: string[]) => {
    agreeRef.current?.resolve(names);
  }, []);

  const stop = useCallback(() => {
    stopRequested.current = true;
    const id = currentJobId.current;
    // Best effort, never fatal: a job that already ended answers false, and a
    // rejection here must not replace the ending the row earns.
    if (id !== null) void jobCancel(id).catch(() => {});
    // A run waiting at the agreement has no job to cancel — Stop there is the
    // same act as giving up.
    agreeRef.current?.reject(new GaveUp());
  }, []);

  const giveUp = useCallback(() => {
    stopRequested.current = true;
    const waiting = agreeRef.current;
    if (waiting) {
      waiting.reject(new GaveUp());
      return;
    }
    const id = currentJobId.current;
    if (id !== null) void jobCancel(id).catch(() => {});
  }, []);

  const finished = reports.length > 0 && reports.every((row) => isTerminal(row.ending));
  const succeeded = reports.length > 0 && reports.every((row) => row.ending.state === "succeeded");

  return useMemo(
    () => ({
      reports,
      running,
      finished,
      succeeded,
      tree,
      proposal,
      awaitingAgreement,
      result,
      scratchLeft,
      openFailure,
      start,
      agree,
      stop,
      giveUp,
    }),
    [
      reports,
      running,
      finished,
      succeeded,
      tree,
      proposal,
      awaitingAgreement,
      result,
      scratchLeft,
      openFailure,
      start,
      agree,
      stop,
      giveUp,
    ]
  );
}
