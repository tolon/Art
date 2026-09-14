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
//
// ART-301. A job's title is a catalogue key Rust names, rendered in the user's
// language. The owner saw *"Adding 1 package(s) to …"* on a Turkish screen.

import { render, screen, act, cleanup } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { changeLanguage } from "@/i18n";
import type { JobProgress, JobTitle } from "@/lib/jobs";

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

const COMPONENTS: JobTitle = {
  key: "components.jobBar.title.previewComponents",
  params: { count: 3, release: "AmigaOS 3.2" },
};
const PACKAGES: JobTitle = {
  key: "components.jobBar.title.previewPackages",
  params: { count: 1, target: "Work.hdf" },
};
const INSTALL: JobTitle = {
  key: "components.jobBar.title.installRelease",
  params: { release: "AmigaOS 3.2", target: "Tree" },
};

function running(id: number, title: JobTitle): JobProgress {
  return { id, title, done: 100 + id, total: null, message: "", state: { state: "running" } };
}

beforeEach(async () => {
  // Vitest is not configured with `globals`, so Testing Library's automatic
  // cleanup never runs — without this the previous test's bar is still in the
  // document and `getAllByRole` finds its buttons too.
  cleanup();
  emit = null;
  jobListMock.mockReset().mockResolvedValue([]);
  jobCancelMock.mockReset().mockResolvedValue(true);
  jobClearFinishedMock.mockReset().mockResolvedValue(undefined);
  await changeLanguage("en");
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

    await send(running(1, COMPONENTS));
    await send(running(2, PACKAGES));

    // The bar is genuinely populated first. Without this the test would pass
    // against a bar that never showed anything at all — one of the two
    // vacuous shapes this round has been producing.
    expect(screen.getByText("Previewing 3 components of AmigaOS 3.2")).toBeTruthy();
    expect(screen.getByText("Previewing 1 package against Work.hdf")).toBeTruthy();

    await send({ ...running(1, COMPONENTS), state: { state: "superseded" } });

    expect(screen.queryByText("Previewing 3 components of AmigaOS 3.2")).toBeNull();
    // …and only that one. The newer preview is still running and must stay.
    expect(screen.getByText("Previewing 1 package against Work.hdf")).toBeTruthy();
  });

  it("still keeps a job the user cancelled, which is news", async () => {
    render(<JobBar />);
    await act(async () => {});

    await send(running(1, INSTALL));
    expect(screen.getByText("Installing AmigaOS 3.2 into Tree")).toBeTruthy();

    await send({ ...running(1, INSTALL), state: { state: "cancelled", files_landed: null } });

    // The contrast is the point: superseded disappears, cancelled does not.
    // A fix that simply hid every terminal job would pass the test above and
    // fail this one.
    expect(screen.getByText("Installing AmigaOS 3.2 into Tree")).toBeTruthy();
  });

  it("wires its Stop button to jobCancel with that row's own id", async () => {
    // The owner reported that stopping "started a new job". It did not: the
    // button was always wired to `jobCancel`, and what produced the new job
    // was the render loop that is fixed in `useRemembered`. Pinned here so
    // the innocent half stays innocent.
    render(<JobBar />);
    await act(async () => {});
    await send(running(7, COMPONENTS));

    const stop = screen.getAllByRole("button").find((b) => b.textContent?.length);
    expect(stop).toBeTruthy();
    await act(async () => {
      stop!.click();
    });
    expect(jobCancelMock).toHaveBeenCalledWith(7);
  });

  it("names a job in Turkish when Turkish is chosen (ART-301)", async () => {
    await changeLanguage("tr");
    render(<JobBar />);
    await act(async () => {});

    await send(
      running(1, { key: "components.jobBar.title.addPackages", params: { count: 1, target: "Work.hdf" } })
    );

    expect(screen.getByText("Work.hdf içine 1 paket ekleniyor")).toBeTruthy();
    // The owner's screenshot, exactly: an English title on a Turkish screen.
    expect(screen.queryByText(/package/)).toBeNull();
  });

  it("picks the plural from count, not from a hand-built (s)", async () => {
    render(<JobBar />);
    await act(async () => {});

    await send(
      running(1, { key: "components.jobBar.title.addPackages", params: { count: 1, target: "Work.hdf" } })
    );
    await send(
      running(2, { key: "components.jobBar.title.addPackages", params: { count: 2, target: "Work.hdf" } })
    );

    expect(screen.getByText("Adding 1 package to Work.hdf")).toBeTruthy();
    expect(screen.getByText("Adding 2 packages to Work.hdf")).toBeTruthy();
    expect(screen.queryByText(/\(s\)/)).toBeNull();
  });
});
