// @vitest-environment jsdom
//
// The dashboard's drop-result cards, under the shell's run lock (round 5,
// task 3).
//
// A dropped file's plan offers routes: *open this in the OS Builder*, *open
// this in the disk tools*. Every one of them is a navigation, and while a
// build is in flight a navigation out of the lane is the very thing round 4
// went to the trouble of refusing on the strip. So the card's route buttons
// go dead here too, with the same sentence — a refusal that names the control
// that lifts it, rather than a button that quietly abandons a run.
//
// What is *not* under test here: the drop pipeline. The analyses arrive
// through the layout's outlet context (one global listener, `Layout.tsx`),
// and this file hands them over directly.

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";
import { MemoryRouter, Outlet, Route, Routes } from "react-router-dom";
import i18n from "i18next";

// Side-effecting: a real, synchronously-initialised i18next instance, so a
// case that names a sentence names the sentence and not the key.
import "@/i18n";

// The recent-files list reaches SQLite on mount. Mocked at the `@/lib`
// wrapper, the boundary this suite mocks at everywhere else.
vi.mock("@/lib/db", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/lib/db")>()),
  getRecentFiles: vi.fn().mockResolvedValue([]),
}));

const { Dashboard } = await import("./Dashboard");
const { RunLockContext } = await import("@/lib/runLock");
const { useSettingsStore } = await import("@/stores/settingsStore");
const { DEFAULT_SETTINGS } = await import("@/lib/settings");
const { useRecentFilesStore } = await import("@/stores/recentFilesStore");

import type { DroppedAnalysis } from "@/types";

/** One dropped ISO whose plan offers a route into the OS Builder. */
const DROPPED: DroppedAnalysis = {
  path: "E:\\amiga\\iso\\AmigaOS39.iso",
  ok: true,
  plan: {
    detection: {
      category: "optical-image",
      format_hint: "iso",
      confidence: 0.98,
      size: 650 * 1024 * 1024,
      is_dir: false,
    },
    recommendations: [],
    candidates: [
      {
        id: "os.install-from-disc",
        name: "Install from disc",
        description: "Open this disc in the OS Builder",
        category: "recommended",
        safety: "safe",
        priority: 10,
        available: true,
        kind: { kind: "navigate", route: "/os-builder" },
      },
    ],
  },
};

function renderDashboard(running: boolean) {
  return render(
    <MemoryRouter initialEntries={["/"]}>
      <RunLockContext.Provider value={{ running, setRunning: () => {} }}>
        <Routes>
          <Route
            path="/"
            element={<Outlet context={{ analyses: [DROPPED], dragOver: false }} />}
          >
            <Route index element={<Dashboard />} />
          </Route>
        </Routes>
      </RunLockContext.Provider>
    </MemoryRouter>
  );
}

beforeEach(() => {
  useSettingsStore.setState({ loaded: true, settings: { ...DEFAULT_SETTINGS } });
  useRecentFilesStore.setState({ files: [], loaded: true });
});

afterEach(cleanup);

describe("the drop cards are locked while a build runs", () => {
  it("leaves a route action live when nothing is running", async () => {
    // The control, measured rather than assumed: a button that were always
    // disabled would pass the case below and prove nothing.
    renderDashboard(false);

    const button = await screen.findByRole("button", { name: /Install from disc/ });
    expect((button as HTMLButtonElement).disabled).toBe(false);
    expect(screen.queryByTestId("dashboard-locked")).toBeNull();
  });

  it("disables a route action and says why", async () => {
    renderDashboard(true);

    const button = await screen.findByRole("button", { name: /Install from disc/ });
    expect((button as HTMLButtonElement).disabled).toBe(true);
    // The title says it on the control itself, for the hand that is already
    // on it…
    // **The lane's own sentence, not the strip's** (round 5, task 4): from
    // the dashboard, *"Stop it before leaving this tab"* names a control on
    // a screen the reader is not looking at, and *this tab* names no tab at
    // all. `navigationLockedLane` names the OS Builder's Build tab.
    expect(button.getAttribute("title")).toBe(i18n.t("osBuilder.build.navigationLockedLane"));
    // …and once under the cards, for the eye that is reading the screen.
    expect(screen.getByTestId("dashboard-locked").textContent).toBe(
      i18n.t("osBuilder.build.navigationLockedLane")
    );
    expect(screen.getByTestId("dashboard-locked").textContent).not.toBe(
      i18n.t("osBuilder.build.navigationLocked")
    );
  });
});
