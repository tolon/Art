// The pure half of `@/lib/jobs`: the status word a job gets, and the fraction
// its bar shows. Both are plain functions over a `JobProgress`, so they are
// tested here rather than through `JobBar.tsx`.

import { describe, expect, it } from "vitest";

import { fraction, jobStatusLabel, type JobProgress } from "./jobs";

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
