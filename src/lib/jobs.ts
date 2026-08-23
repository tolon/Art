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

export interface JobProgress {
  id: number;
  /** What the job is, in the user's language. */
  title: string;
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
export function awaitJobResult<TPayload extends { job_id: number }, TValue>(
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
     *  before returning `Ok(())`). Only the two failure states settle here. */
    function settleFromProgress(job: JobProgress) {
      if (settled || job.state.state === "running") return;
      if (job.state.state === "failed") {
        settled = true;
        cleanup();
        reject(new Error(`${job.state.message} (${job.state.error_code})`));
      } else if (job.state.state === "cancelled") {
        settled = true;
        cleanup();
        reject(new Error("cancelled"));
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
          if (event.payload.job_id !== jobId) return;
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
        const matchedResult = bufferedResults.find((payload) => payload.job_id === id);
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
