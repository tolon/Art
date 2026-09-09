// @vitest-environment jsdom
//
// The shell owns the run lock (round 5, task 3).
//
// `Sidebar.test.tsx` proves the sidebar draws a lock it is *handed*; this
// file proves there is something to hand it. The provider used to be
// `OsBuilder`'s own — inside the routed screen, so no sidebar entry and no
// dashboard card was ever inside it — and moving it out is the whole of this
// task. Delete the provider from `Layout` and every one of those component
// tests still passes, each one supplying its own context: the wiring is
// exactly the part a context test cannot see, so it is asserted here, once,
// end to end. A routed child sets the lock the way `BuildTab` does; the real
// sidebar beside it has to go dead.
//
// The shell's own furniture is mocked out. What is under test is which
// components sit inside the provider, not what the job bar has to say.

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, render, screen } from "@testing-library/react";
import { MemoryRouter, Route, Routes } from "react-router-dom";
import { useEffect } from "react";
import i18n from "i18next";

// Side-effecting: a real, synchronously-initialised i18next instance, so the
// case that names the refusal names the sentence and not the key.
import "@/i18n";

// The one global drop listener reaches Tauri on mount (`Layout` is the only
// place in ART that installs it — CLAUDE.md). Mocked at the `@/lib` wrapper.
vi.mock("@/lib/dnd", () => ({
  setupDragDrop: vi.fn().mockResolvedValue(() => {}),
}));
vi.mock("@/components/JobBar", () => ({ JobBar: () => <div data-testid="job-bar" /> }));
vi.mock("@/components/ScratchRootGate", () => ({
  ScratchRootGate: () => <div data-testid="scratch-gate" />,
}));
vi.mock("@/components/layout/TopBar", () => ({ TopBar: () => <div data-testid="top-bar" /> }));

const { Layout } = await import("./Layout");
const { useRunLock } = await import("@/pages/osbuilder/runLock");
const { useSettingsStore } = await import("@/stores/settingsStore");
const { DEFAULT_SETTINGS } = await import("@/lib/settings");

/** A routed screen that turns the lock on the way `BuildTab` does. */
function RunningScreen() {
  const { setRunning } = useRunLock();
  useEffect(() => {
    setRunning(true);
    return () => setRunning(false);
  }, [setRunning]);
  return <div data-testid="running-screen" />;
}

beforeEach(() => {
  useSettingsStore.setState({ loaded: true, settings: { ...DEFAULT_SETTINGS } });
});

afterEach(cleanup);

describe("the shell provides the run lock", () => {
  it("carries a routed screen's lock out to the sidebar", async () => {
    await act(async () => {
      render(
        <MemoryRouter initialEntries={["/os-builder"]}>
          <Routes>
            <Route element={<Layout />}>
              <Route path="/os-builder" element={<RunningScreen />} />
            </Route>
          </Routes>
        </MemoryRouter>
      );
    });

    await screen.findByTestId("running-screen");
    // The sidebar is a *sibling* of the outlet, so this fails the moment the
    // provider goes back inside the routed screen.
    expect(screen.getByTestId("sidebar-locked").textContent).toBe(
      i18n.t("osBuilder.build.navigationLocked")
    );
    expect(screen.queryAllByRole("link")).toHaveLength(0);
  });

  it("leaves the sidebar alone when no screen has set it", async () => {
    // The control, measured rather than assumed: a shell that were always
    // locked would pass the case above and prove nothing.
    await act(async () => {
      render(
        <MemoryRouter initialEntries={["/os-builder"]}>
          <Routes>
            <Route element={<Layout />}>
              <Route path="/os-builder" element={<div data-testid="quiet-screen" />} />
            </Route>
          </Routes>
        </MemoryRouter>
      );
    });

    await screen.findByTestId("quiet-screen");
    expect(screen.queryByTestId("sidebar-locked")).toBeNull();
    expect(screen.getAllByRole("link").length).toBeGreaterThan(1);
  });
});
