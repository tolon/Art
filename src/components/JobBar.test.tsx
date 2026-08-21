// @vitest-environment jsdom
//
// ART-195. The owner photographed the job bar with four preview rows stacked
// on it, counts jumping about, and pressing Stop appeared to add a fifth. The
// producing side is fixed elsewhere (`spawn_job_in_lane` cancels the previous
// preview; `useRemembered` no longer re-fires the effect that starts them).
// This file is about the bar itself: when ART supersedes a job, the row has to
// come **off**.
//
// Getting that wrong is not a cosmetic miss. `JobBar` keeps failed *and
// cancelled* jobs on screen as "notable", so a superseded preview reported as
// `cancelled` would have swapped four stacked running rows for four stacked
// cancelled ones — the complaint, wearing the fix's clothes.

import { render, screen, act, cleanup } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

import type { JobProgress } from "@/lib/jobs";

const jobListMock = vi.fn();
const jobCancelMock = vi.fn();
const jobClearFinishedMock = vi.fn();
let emit: ((job: JobProgress) => void) | null = null;

vi.mock("@/lib/jobs", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/lib/jobs")>()),
  jobList: () => jobListMock(),
  jobCancel: (id: number) => jobCancelMock(id),
  jobClearFinished: () => jobClearFinishedMock(),
  onJobProgress: (handler: (job: JobProgress) => void) => {
    emit = handler;
    return Promise.resolve(() => {
      emit = null;
    });
  },
}));

const { JobBar } = await import("@/components/JobBar");

function running(id: number, title: string): JobProgress {
  return { id, title, done: 100 + id, total: null, message: "", state: { state: "running" } };
}

beforeEach(() => {
  // Vitest is not configured with `globals`, so Testing Library's automatic
  // cleanup never runs — without this the previous test's bar is still in the
  // document and `getAllByRole` finds its buttons too.
  cleanup();
  emit = null;
  jobListMock.mockReset().mockResolvedValue([]);
  jobCancelMock.mockReset().mockResolvedValue(true);
  jobClearFinishedMock.mockReset().mockResolvedValue(undefined);
});

async function send(job: JobProgress) {
  await act(async () => {
    emit?.(job);
  });
}

describe("the job bar", () => {
  it("takes a superseded job off the bar instead of restating it", async () => {
    render(<JobBar />);
    await act(async () => {});

    await send(running(1, "Previewing components"));
    await send(running(2, "Previewing packages"));

    // The bar is genuinely populated first. Without this the test would pass
    // against a bar that never showed anything at all — one of the two
    // vacuous shapes this round has been producing.
    expect(screen.getByText("Previewing components")).toBeTruthy();
    expect(screen.getByText("Previewing packages")).toBeTruthy();

    await send({ ...running(1, "Previewing components"), state: { state: "superseded" } });

    expect(screen.queryByText("Previewing components")).toBeNull();
    // …and only that one. The newer preview is still running and must stay.
    expect(screen.getByText("Previewing packages")).toBeTruthy();
  });

  it("still keeps a job the user cancelled, which is news", async () => {
    render(<JobBar />);
    await act(async () => {});

    await send(running(1, "Installing AmigaOS"));
    expect(screen.getByText("Installing AmigaOS")).toBeTruthy();

    await send({
      ...running(1, "Installing AmigaOS"),
      state: { state: "cancelled", files_landed: null },
    });

    // The contrast is the point: superseded disappears, cancelled does not.
    // A fix that simply hid every terminal job would pass the test above and
    // fail this one.
    expect(screen.getByText("Installing AmigaOS")).toBeTruthy();
  });

  it("wires its Stop button to jobCancel with that row's own id", async () => {
    // The owner reported that stopping "started a new job". It did not: the
    // button was always wired to `jobCancel`, and what produced the new job
    // was the render loop that is fixed in `useRemembered`. Pinned here so
    // the innocent half stays innocent.
    render(<JobBar />);
    await act(async () => {});
    await send(running(7, "Previewing components"));

    const stop = screen.getAllByRole("button").find((b) => b.textContent?.length);
    expect(stop).toBeTruthy();
    await act(async () => {
      stop!.click();
    });
    expect(jobCancelMock).toHaveBeenCalledWith(7);
  });
});
