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

const { awaitJobResult, fraction, jobStatusLabel } = await import("./jobs");

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
    title: "Copying",
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
      title: "x",
      done: 0,
      total: null,
      message: "",
      state: { state: "failed", error_code: "ART-IO", message: "disk full" },
    });
    startResolve(9);

    await expect(promise).rejects.toThrow("disk full (ART-IO)");
  });
});
