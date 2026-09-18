// The pure half of `@/lib/jobs`: the status word a job gets, and the fraction
// its bar shows. Both are plain functions over a `JobProgress`, so they are
// tested here rather than through `JobBar.tsx`. `awaitJobResult`'s own fast-
// path race (N1, Task 7's re-review) is tested here too, against a mocked
// `@tauri-apps/api/event`'s `listen` — no jsdom needed, this file stays
// plain Node.

import { beforeEach, describe, expect, it, vi } from "vitest";

import type { JobProgress } from "./jobs";

const listenMock = vi.hoisted(() => vi.fn());

vi.mock("@tauri-apps/api/event", () => ({
  listen: listenMock,
}));

const { awaitJobResult, fraction, isJobCancellation, JobRefused, jobStatusLabel } =
  await import("./jobs");

// Every `listen(event, handler)` call this test drives registers into here,
// keyed by event name — `awaitJobResult` opens two at once (its own result
// event and `onJobProgress`'s `"job-progress"`), so a single mock has to
// support more than one live handler per event and more than one event.
const handlers = new Map<string, ((event: { payload: unknown }) => void)[]>();

function emit(event: string, payload: unknown) {
  for (const handler of handlers.get(event) ?? []) handler({ payload });
}

beforeEach(() => {
  handlers.clear();
  listenMock.mockReset().mockImplementation((event: string, handler: (e: { payload: unknown }) => void) => {
    const list = handlers.get(event) ?? [];
    list.push(handler);
    handlers.set(event, list);
    return Promise.resolve(() => {
      const index = list.indexOf(handler);
      if (index >= 0) list.splice(index, 1);
    });
  });
});

function job(overrides: Partial<JobProgress> = {}): JobProgress {
  return {
    id: 1,
    title: { key: "components.jobBar.title.copyInto", params: { target: "DH0" } },
    done: 0,
    total: null,
    message: "",
    state: { state: "running" },
    ...overrides,
  };
}

describe("jobStatusLabel — a cancelled job says what it left behind (ART-058)", () => {
  it("says only 'cancelled' when nothing landed", () => {
    const phrase = jobStatusLabel(job({ state: { state: "cancelled", files_landed: null } }));
    expect(phrase.key).toBe("components.jobBar.status.cancelled");
    expect(phrase.params).toBeUndefined();
  });

  it("names the count when files did land", () => {
    // A large image is written file by file, each committed before the next
    // starts, so cancelling cannot take back what is already there. Saying
    // only "cancelled" for that reads as "nothing happened".
    const phrase = jobStatusLabel(job({ state: { state: "cancelled", files_landed: 12 } }));
    expect(phrase.key).toBe("components.jobBar.status.cancelledPartway");
    expect(phrase.params).toEqual({ count: 12 });
  });

  it("passes the number as `count`, which is what makes i18next pluralise", () => {
    // The ART-061 lesson: `_one`/`_other` in the catalogue do nothing unless
    // the interpolation variable is named `count`.
    const one = jobStatusLabel(job({ state: { state: "cancelled", files_landed: 1 } }));
    expect(one.params).toEqual({ count: 1 });
  });

  it("still labels the other three states", () => {
    expect(jobStatusLabel(job()).key).toBe("components.jobBar.status.running");
    expect(jobStatusLabel(job({ state: { state: "finished" } })).key).toBe(
      "components.jobBar.status.done"
    );
    const failed = jobStatusLabel(
      job({ state: { state: "failed", error_code: "ART-IO", message: "x" } })
    );
    expect(failed.key).toBe("components.jobBar.status.failed");
    expect(failed.params).toEqual({ code: "ART-IO" });
  });

  it("answers its own key for a refused job — never the failed key (round 4, Task 1)", () => {
    const refused = jobStatusLabel(
      job({
        state: { state: "refused", code: "ART-KICKSTART-NOT-PROPOSED", message: "x" },
      })
    );
    expect(refused.key).toBe("components.jobBar.status.refused");
    expect(refused.key).not.toBe("components.jobBar.status.failed");
  });
});

describe("fraction", () => {
  it("is null while the total is unknown, so the bar stays indeterminate", () => {
    expect(fraction(job({ done: 5 }))).toBeNull();
    expect(fraction(job({ done: 5, total: 0 }))).toBeNull();
  });

  it("never reports over 100%, however far a job overshoots its estimate", () => {
    expect(fraction(job({ done: 5, total: 10 }))).toBe(0.5);
    expect(fraction(job({ done: 20, total: 10 }))).toBe(1);
  });
});

interface TestResult {
  job_id: number;
  value: string;
}

describe("awaitJobResult — the fast-path race (N1, Task 7's re-review)", () => {
  it("still resolves when the result event arrives before `start` returns the job id", async () => {
    // The exact race a real cache-hit preview can produce: `spawn_job`
    // starts its background thread — and can finish it — before the
    // `#[tauri::command]` even returns the job id to the frontend. The old
    // shape of `awaitJobResult` took a bare `jobId` and only subscribed once
    // the caller already had it, so this event would have been lost
    // outright: the promise never settling, with no timeout, and both
    // listeners leaked.
    let startResolve!: (id: number) => void;
    const start = () => new Promise<number>((resolve) => (startResolve = resolve));

    const promise = awaitJobResult<TestResult, string>(
      "test-result-fast",
      start,
      (payload) => payload.value
    );

    // Fires before `start()` has resolved — only possible at all because
    // `awaitJobResult` subscribes synchronously, before calling `start`.
    emit("test-result-fast", { job_id: 7, value: "fast" });

    // `invoke` "returns" the id the event already named.
    startResolve(7);

    await expect(promise).resolves.toBe("fast");
  });

  it("ignores a same-event payload for a different job encountered before the id is known", async () => {
    let startResolve!: (id: number) => void;
    const start = () => new Promise<number>((resolve) => (startResolve = resolve));

    const promise = awaitJobResult<TestResult, string>(
      "test-result-other-job",
      start,
      (payload) => payload.value
    );

    // Another job's own result, racing in first — must not be mistaken for
    // this call's own answer once the real id arrives.
    emit("test-result-other-job", { job_id: 999, value: "not this one" });
    startResolve(7);
    emit("test-result-other-job", { job_id: 7, value: "this one" });

    await expect(promise).resolves.toBe("this one");
  });

  it("still resolves the ordinary way when the event arrives after the id is known", async () => {
    const promise = awaitJobResult<TestResult, string>(
      "test-result-ordinary",
      () => Promise.resolve(3),
      (payload) => payload.value
    );
    await Promise.resolve(); // let `start()`'s own `.then` settle `jobId`
    emit("test-result-ordinary", { job_id: 3, value: "ordinary" });
    await expect(promise).resolves.toBe("ordinary");
  });

  it("rejects from a buffered failed job-progress update, not only a live one", async () => {
    let startResolve!: (id: number) => void;
    const start = () => new Promise<number>((resolve) => (startResolve = resolve));

    const promise = awaitJobResult<TestResult, string>(
      "test-result-never-fires",
      start,
      (payload) => payload.value
    );

    // The job failed before this side even knew its own id — the
    // `job-progress` counterpart of the same race.
    emit("job-progress", {
      id: 9,
      title: { key: "components.jobBar.title.copyInto", params: { target: "DH0" } },
      done: 0,
      total: null,
      message: "",
      state: { state: "failed", error_code: "ART-IO", message: "disk full" },
    });
    startResolve(9);

    await expect(promise).rejects.toThrow("disk full (ART-IO)");
  });
});

// ---------------------------------------------------------------------------
// The fourth ending (round 4, Task 1) reaching the one function that waits
// ---------------------------------------------------------------------------

describe("awaitJobResult — a refused job (round 4, Task 10)", () => {
  // `settleFromProgress` knew three job states and `refused` was not one of
  // them, so a job that ended refused with no result event left the promise
  // unsettled for ever: no timeout, both listeners leaked, and the screen
  // sitting on a spinner with nothing to say. Task 1 carried this forward.
  it("settles rather than hanging when the job ends refused", async () => {
    const promise = awaitJobResult<TestResult, string>(
      "test-result-refused",
      () => Promise.resolve(5),
      (payload) => payload.value
    );
    await Promise.resolve();
    emit("job-progress", {
      id: 5,
      title: { key: "components.jobBar.title.buildCardOs", params: { target: "E:\kart.img" } },
      done: 0,
      total: null,
      message: "",
      state: { state: "refused", code: "ART-KICKSTART-NOT-PROPOSED", message: "not proposed" },
    });
    await expect(promise).rejects.toThrow("not proposed");
  });

  // **A refusal is never a failure and never a cancellation**: its next step
  // is the user's, so the rejection has to be tellable apart by its type and
  // has to carry the `ART-*` code the screen builds its sentence from.
  it("rejects with the refusal's own code, apart from a failure and a cancellation", async () => {
    const promise = awaitJobResult<TestResult, string>(
      "test-result-refused-code",
      () => Promise.resolve(6),
      (payload) => payload.value
    );
    await Promise.resolve();
    emit("job-progress", {
      id: 6,
      title: { key: "components.jobBar.title.buildCardOs", params: { target: "E:\kart.img" } },
      done: 0,
      total: null,
      message: "",
      state: { state: "refused", code: "ART-CARD-DOES-NOT-FIT", message: "it does not fit" },
    });
    const error = await promise.then(
      () => null,
      (e: unknown) => e
    );
    expect(error).toBeInstanceOf(JobRefused);
    expect((error as InstanceType<typeof JobRefused>).code).toBe("ART-CARD-DOES-NOT-FIT");
    expect(isJobCancellation(error)).toBe(false);
  });
});

// ---------------------------------------------------------------------------
// The card commands answer camelCase (round 4, Task 10)
// ---------------------------------------------------------------------------

interface CamelResult {
  jobId: number;
  value: string;
}

describe("awaitJobResult — a result event whose id is camelCase", () => {
  // `card_os_prepare` and `card_os_build` serialise with
  // `#[serde(rename_all = "camelCase")]`, so their result events carry
  // `jobId`, not `job_id`. Filtering on `job_id` alone read `undefined` for
  // every one of them: a payload that could never match its own job, and a
  // promise that never settled.
  it("matches the job by `jobId` as readily as by `job_id`", async () => {
    const promise = awaitJobResult<CamelResult, string>(
      "card-os-build-result",
      () => Promise.resolve(13),
      (payload) => payload.value
    );
    await Promise.resolve();
    emit("card-os-build-result", { jobId: 13, value: "card" });
    await expect(promise).resolves.toBe("card");
  });

  it("still ignores another job's camelCase payload", async () => {
    let startResolve!: (id: number) => void;
    const start = () => new Promise<number>((resolve) => (startResolve = resolve));
    const promise = awaitJobResult<CamelResult, string>(
      "card-os-prepare-result",
      start,
      (payload) => payload.value
    );
    emit("card-os-prepare-result", { jobId: 99, value: "not this one" });
    startResolve(13);
    emit("card-os-prepare-result", { jobId: 13, value: "this one" });
    await expect(promise).resolves.toBe("this one");
  });
});
