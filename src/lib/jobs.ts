// Background jobs (spec §54, §55). Mirrors src-tauri/src/core/jobs
// and src-tauri/src/commands/jobs.rs.

import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

import type { Phrase } from "@/lib/phrase";

export type JobState =
  | { state: "running" }
  | { state: "finished" }
  /**
   * `files_landed` is how many files were already written and left in place
   * when the job stopped, `null` when nothing was (ART-058).
   *
   * A large image is written file by file, each one committed and journalled
   * before the next starts, so cancelling cannot take back what already
   * landed — and saying only "cancelled" for that undersells what happened to
   * the volume. A small image is written whole and cancelling leaves nothing,
   * which is the `null` case and the common one.
   */
  | { state: "cancelled"; files_landed: number | null }
  | { state: "failed"; error_code: string; message: string }
  /**
   * Refused — the fourth ending (card round 4, Task 1). Something ART's own
   * rules stopped it doing, decided *before* anything was harmed: never
   * "not succeeded" and never `failed`, because a refused card build's next
   * step is the user's (a name, a file, a setting), not "try again".
   * `code` is the refusal's own `ART-*` id, exactly what the refusing
   * `CoreError` on the Rust side carries.
   */
  | { state: "refused"; code: string; message: string }
  /**
   * Cancelled by ART itself because a newer job in the same lane replaced it
   * (ART-195) — a live preview the screen re-asked for.
   *
   * The user did not ask for this one to stop; they asked for the *next* one
   * to start. So it is not news, and `JobBar` takes the row off the bar
   * rather than adding a "cancelled" one beside the preview that is still
   * running. Stopping is the same cancel token and the same `is_cancelled()`
   * check the Stop button uses — only who has to be told is different.
   */
  | { state: "superseded" };

/**
 * Every catalogue key a job title may name (ART-301).
 *
 * A job's title is decided in Rust — `JobTitle::new("…")` in
 * `src-tauri/src/commands/*.rs` — and rendered by `JobBar.tsx` through a
 * variable, so no TypeScript code would otherwise name these keys. This list
 * is where they are named. `dead-keys.test.ts` counts them as reachable
 * because they are written out here. `src/i18n/job-title-keys.test.ts`
 * resolves each one in both catalogues and reads the Rust tree. It fails on a
 * Rust key missing from this list, on a key here that no Rust site names, and
 * on values a sentence does not use.
 *
 * Sorted, one entry per key. A plural key is named without `_one` / `_other`.
 */
export const JOB_TITLE_KEYS = [
  "components.jobBar.title.addPackages",
  "components.jobBar.title.applyAppearance",
  "components.jobBar.title.applyLayout",
  "components.jobBar.title.buildCard",
  "components.jobBar.title.buildCardOs",
  "components.jobBar.title.copyArchiveInto",
  "components.jobBar.title.copyDiscInto",
  "components.jobBar.title.copyInto",
  "components.jobBar.title.copyOutOf",
  "components.jobBar.title.copySelectionBetween",
  "components.jobBar.title.copySelectionInto",
  "components.jobBar.title.copySelectionOutOf",
  "components.jobBar.title.countFolder",
  "components.jobBar.title.countVolumeBlock",
  "components.jobBar.title.deleteItems",
  "components.jobBar.title.downloadPackage",
  "components.jobBar.title.downloadPackages",
  "components.jobBar.title.fetchArtwork",
  "components.jobBar.title.identifyMedia",
  "components.jobBar.title.indexTitles",
  "components.jobBar.title.installArchive",
  "components.jobBar.title.installArchiveInto",
  "components.jobBar.title.installArchivesInto",
  "components.jobBar.title.installOnAmiga",
  "components.jobBar.title.installRelease",
  "components.jobBar.title.measureCardOs",
  "components.jobBar.title.planArchives",
  "components.jobBar.title.planLayout",
  "components.jobBar.title.prepareCardOs",
  "components.jobBar.title.preparePartitions",
  "components.jobBar.title.previewComponents",
  "components.jobBar.title.previewPackages",
  "components.jobBar.title.readLocalPictures",
  "components.jobBar.title.readReadme",
  "components.jobBar.title.refreshCatalogue",
  "components.jobBar.title.rehearseFirstBoot",
  "components.jobBar.title.restorePictures",
  "components.jobBar.title.syncAminet",
  "components.jobBar.title.writeIgameData",
] as const;

export type JobTitleKey = (typeof JOB_TITLE_KEYS)[number];

/**
 * What a job is: a catalogue key and the values its sentence needs, never a
 * sentence (ART-301). The same shape as `Phrase`, narrowed to the keys above.
 * Values are paths, names and counts, and pass through untranslated.
 */
export interface JobTitle {
  key: JobTitleKey;
  params?: Record<string, string | number>;
}

export interface JobProgress {
  id: number;
  /** What the job is — render with `t(title.key, title.params)`. */
  title: JobTitle;
  done: number;
  /** Null while the size is unknown — show an indeterminate indicator, not a fake bar. */
  total: number | null;
  /** What is happening right now, e.g. the current file. */
  message: string;
  state: JobState;
}

export const JOB_EVENT = "job-progress";

export async function jobList(): Promise<JobProgress[]> {
  return invoke<JobProgress[]>("job_list");
}

/** Ask a job to stop at its next safe point. False when it already ended. */
export async function jobCancel(id: number): Promise<boolean> {
  return invoke<boolean>("job_cancel", { id });
}

export async function jobClearFinished(): Promise<void> {
  return invoke<void>("job_clear_finished");
}

/** Subscribe to progress updates. Returns an unlisten function. */
export async function onJobProgress(
  handler: (job: JobProgress) => void
): Promise<UnlistenFn> {
  return listen<JobProgress>(JOB_EVENT, (event) => handler(event.payload));
}

export function isRunning(job: JobProgress): boolean {
  return job.state.state === "running";
}

/** Completion as 0–1, or null when the total is unknown. */
export function fraction(job: JobProgress): number | null {
  if (job.total === null || job.total <= 0) return null;
  return Math.min(1, Math.max(0, job.done / job.total));
}

/** A short status word for the UI. */
export function jobStatusLabel(job: JobProgress): Phrase {
  switch (job.state.state) {
    case "running":
      return { key: "components.jobBar.status.running" };
    case "finished":
      return { key: "components.jobBar.status.done" };
    case "cancelled":
      // `count` is not decoration: i18next only pluralises when it is passed
      // under that name, which is the half ART-061 was missing.
      return job.state.files_landed === null
        ? { key: "components.jobBar.status.cancelled" }
        : {
            key: "components.jobBar.status.cancelledPartway",
            params: { count: job.state.files_landed },
          };
    case "failed":
      return { key: "components.jobBar.status.failed", params: { code: job.state.error_code } };
    case "refused":
      return { key: "components.jobBar.status.refused" };
    case "superseded":
      // Never actually rendered — `JobBar` drops a superseded job rather than
      // showing it — but the switch has to be total, and "cancelled" is the
      // true word for what happened to it. Deliberately not a catalogue key
      // of its own: an unused key fails `pnpm test`, and rightly.
      return { key: "components.jobBar.status.cancelled" };
  }
}

// ---------------------------------------------------------------------------
// Subscribing safely (Task 7's own fix round, F7)
//
// Every `on*` wrapper here and in `@/lib/osinstall` returns a
// `Promise<UnlistenFn>` (that is what `listen()` itself returns), and the
// ordinary way to use one in a `useEffect` —
//
//   let unlisten: (() => void) | undefined;
//   void onJobProgress(handler).then((fn) => { unlisten = fn; });
//   return () => unlisten?.();
//
// — has two real defects, not just a style complaint. First, `listen()`'s
// promise rejects when there is no Tauri IPC bridge to reach (a jsdom test
// with nothing mocking `@/lib/jobs`, but just as truly a real webview whose
// bridge is not ready yet), and nothing here ever catches that — an
// unhandled rejection in production, and the whole reason ART-163 kept
// resurfacing in tests that forgot to mock this module. Second, if the
// component unmounts *before* the promise resolves, the cleanup above runs
// with `unlisten` still `undefined` — by the time the promise finally
// settles and assigns it, nothing calls it, and the real Tauri listener
// stays registered forever: a leak, not just a missed no-op.
// ---------------------------------------------------------------------------

/**
 * Subscribe through `subscribe` (any `on*` wrapper here or in
 * `@/lib/osinstall`, e.g. `() => onJobProgress(handler)`) safely: a failure
 * to establish the listener is caught rather than left unhandled, and a
 * caller that tears this down before the subscribe promise has resolved has
 * its listener removed the instant it arrives rather than leaked. Returns a
 * plain teardown function, meant to be a `useEffect`'s own return value.
 */
export function subscribeSafely(subscribe: () => Promise<UnlistenFn>): () => void {
  let cancelled = false;
  let unlisten: UnlistenFn | undefined;

  subscribe()
    .then((fn) => {
      if (cancelled) {
        fn();
      } else {
        unlisten = fn;
      }
    })
    .catch(() => {
      // Nothing was ever registered, so there is nothing to tear down. A
      // real IPC failure is rare in production and already silent to the
      // user by design (the caller's own state simply never updates); in a
      // test environment with no Tauri bridge at all this is the expected,
      // ordinary case.
    });

  return () => {
    cancelled = true;
    unlisten?.();
  };
}

// ---------------------------------------------------------------------------
// Awaiting one job's own result (Task 7, F4)
// ---------------------------------------------------------------------------

/**
 * The message {@link awaitJobResult} rejects with when the job was
 * **cancelled** rather than failed.
 *
 * Exported, and read back through {@link isJobCancellation}, because a caller
 * that cannot tell those two rejections apart has only one ending for both —
 * and "the user pressed Stop" and "ART could not do this" are two endings with
 * two different next steps (`CLAUDE.md`, *Endings stay distinct*). The
 * constant and the `reject` below are the same value so the two cannot drift.
 */
export const JOB_CANCELLED_MESSAGE = "cancelled";

/**
 * Was this rejection the user stopping the job, rather than the job failing?
 *
 * Deliberately narrow: only an `Error` carrying exactly
 * {@link JOB_CANCELLED_MESSAGE}. A failed job rejects with its own message and
 * its `ART-*` code appended, so it can never be mistaken for this one — and a
 * rejection from anywhere else (a dropped connection, a command that threw
 * before the job started) is a failure, which is the safer of the two to
 * report.
 */
export function isJobCancellation(err: unknown): boolean {
  return err instanceof Error && err.message === JOB_CANCELLED_MESSAGE;
}

/**
 * The rejection a **refused** job produces — the fourth ending (card round 4,
 * Task 1) reaching the one function that waits on a job.
 *
 * Its own class rather than a message convention, because a refusal is not a
 * failure and not a cancellation: its next step is the user's, and the screen
 * builds that sentence from the `ART-*` code, which a formatted message would
 * have to be parsed back out of. `message` stays the core's own sentence, so
 * an `errorText` fallback still says something true.
 *
 * Added in Task 10 with {@link awaitJobResult}'s `refused` arm: before it,
 * `settleFromProgress` knew three job states and a job that ended refused
 * without emitting its result event left the promise unsettled for ever.
 */
export class JobRefused extends Error {
  constructor(
    readonly code: string,
    message: string
  ) {
    super(message);
    this.name = "JobRefused";
  }
}

/**
 * The rejection a **failed** job produces, carrying the code it failed with
 * (round 4 final review, C2).
 *
 * The message was — and still is — `"<sentence> (<ART-CODE>)"`, which reads
 * well in a badge and is unreadable to a recogniser: `parseError` looks for
 * the `\n\nError ID: ` trailer that `CoreError::user_message` writes, and a
 * code in parentheses is not it. So an error that travelled this way arrived
 * at `errorPhrase` with no id at all and was rendered as
 * `errors.verbatimNoId` — Rust's English, trailer and all. Carrying the code
 * as a field costs nothing and keeps the `ART-*` id a caller can build a
 * sentence from, even for a failure nobody has written a recogniser for.
 *
 * A sibling of {@link JobRefused}, and deliberately a *different* class: a
 * failure and a refusal are two endings with two different next steps.
 */
export class JobFailed extends Error {
  constructor(
    readonly code: string,
    message: string
  ) {
    super(message);
    this.name = "JobFailed";
  }
}

/**
 * Wait for exactly one job to finish, resolving with the value its own
 * result event carries — or rejecting with a readable sentence if the job
 * fails or is cancelled first. `resultEvent` is a Tauri event name whose
 * payload always carries `job_id`.
 *
 * **`start` is what actually invokes the command, and it runs *after* both
 * listeners below are already registered — not before.** A re-review of the
 * first version of this function (which took a bare `jobId: number` and
 * subscribed only once the caller already had it) found a real race: Rust's
 * `spawn_job` starts its background thread *before* the `#[tauri::command]`
 * even returns the job id, so a fast job — a cache hit especially, see
 * `commands/osinstall.rs`'s own preview cache — can finish and emit its
 * result event while the frontend is still sitting inside `await invoke(...)`,
 * strictly before `awaitJobResult` had a `jobId` to filter on at all. The old
 * shape lost that event forever: the promise never settled (no timeout
 * either), and both listeners leaked. Subscribing first closes the window
 * entirely — nothing this job does can happen before this function is
 * already listening for it — at the cost of not yet knowing which job id to
 * filter on, which the buffering below exists to resolve.
 *
 * What lets `osinstallCollisions` keep its original `Promise<CollisionReport[]>`
 * shape even though the work behind it moved onto a background job (F4 —
 * `commands/osinstall.rs`'s own module doc comment explains why): this
 * function is the part that hides the job underneath an ordinary promise.
 */
/**
 * Which job a result payload is about.
 *
 * Two spellings, because the Rust side has two: the older commands serialise
 * their result struct as it is written (`job_id`), and everything under
 * `#[serde(rename_all = "camelCase")]` — the whole card path, `card_os_prepare`
 * and `card_os_build` among them — sends `jobId`. Filtering on `job_id` alone
 * read `undefined` for every card event, which matches no job at all: the
 * promise simply never settled (card round 4, Task 10).
 */
function jobIdOf(payload: { job_id: number } | { jobId: number }): number {
  return "job_id" in payload ? payload.job_id : payload.jobId;
}

export function awaitJobResult<
  TPayload extends { job_id: number } | { jobId: number },
  TValue,
>(
  resultEvent: string,
  start: () => Promise<number>,
  extract: (payload: TPayload) => TValue
): Promise<TValue> {
  return new Promise<TValue>((resolve, reject) => {
    let settled = false;
    // `null` until `start()` resolves — an event or a progress update that
    // arrives before then cannot yet be matched to a job id, so it is kept
    // rather than dropped, and matched retroactively the moment the id is
    // known (see `start().then(...)` below).
    let jobId: number | null = null;
    const bufferedResults: TPayload[] = [];
    const bufferedProgress: JobProgress[] = [];

    const teardown: (() => void)[] = [];
    const cleanup = () => {
      for (const fn of teardown.splice(0)) fn();
    };

    /** `"finished"` is not itself a rejection or a resolution — the result
     *  event is what carries the actual value, and it is expected to arrive
     *  at essentially the same moment (the Rust side emits it immediately
     *  before returning `Ok(())`). Only the three ending states settle here,
     *  and they settle **apart**: a failure, a cancellation and a refusal are
     *  three different things to tell somebody. */
    function settleFromProgress(job: JobProgress) {
      if (settled || job.state.state === "running") return;
      if (job.state.state === "failed") {
        settled = true;
        cleanup();
        // The same sentence as before — 134 callers render `String(e)` — plus
        // the code as a field, for the ones that can do better with it (C2).
        reject(new JobFailed(job.state.error_code, `${job.state.message} (${job.state.error_code})`));
      } else if (job.state.state === "cancelled") {
        settled = true;
        cleanup();
        reject(new Error(JOB_CANCELLED_MESSAGE));
      } else if (job.state.state === "refused") {
        // Card round 4, Task 10. `card_os_build` emits its own result event
        // for every ending including this one, so this arm is the safety net
        // rather than the ordinary path — but without it a refused job whose
        // result event never arrived left this promise unsettled for ever,
        // which is a spinner with nothing behind it.
        settled = true;
        cleanup();
        reject(new JobRefused(job.state.code, job.state.message));
      }
    }

    teardown.push(
      subscribeSafely(() =>
        listen<TPayload>(resultEvent, (event) => {
          if (settled) return;
          if (jobId === null) {
            bufferedResults.push(event.payload);
            return;
          }
          if (jobIdOf(event.payload) !== jobId) return;
          settled = true;
          cleanup();
          resolve(extract(event.payload));
        })
      )
    );

    teardown.push(
      subscribeSafely(() =>
        onJobProgress((job) => {
          if (settled) return;
          if (jobId === null) {
            bufferedProgress.push(job);
            return;
          }
          if (job.id !== jobId) return;
          settleFromProgress(job);
        })
      )
    );

    start()
      .then((id) => {
        if (settled) return;
        jobId = id;
        // Catch up on whatever arrived in the gap between subscribing and
        // learning the id — the whole reason this is buffered rather than
        // simply filtered from the start.
        const matchedResult = bufferedResults.find((payload) => jobIdOf(payload) === id);
        if (matchedResult) {
          settled = true;
          cleanup();
          resolve(extract(matchedResult));
          return;
        }
        const matchedProgress = bufferedProgress.find((job) => job.id === id);
        if (matchedProgress) settleFromProgress(matchedProgress);
      })
      .catch((e: unknown) => {
        if (settled) return;
        settled = true;
        cleanup();
        // Wrapped, not rendered: whoever catches this puts it on screen
        // through `errorText`, and `src/lib` has no `t` (ART-060).
        reject(e instanceof Error ? e : new Error(String(e)));
      });
  });
}
