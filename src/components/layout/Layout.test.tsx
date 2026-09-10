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
import { RouterProvider, createMemoryRouter } from "react-router-dom";
import { useEffect, type ReactElement } from "react";
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
const { useRunLock } = await import("@/lib/runLock");
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

/**
 * The shell at `entries`, the last one current, with `screens` as its routed
 * children — through a **data router**, because that is what `App` builds and
 * what `useBlocker` needs (ART-292). A plain `MemoryRouter` would make the
 * shell's own history guard throw, which is a harness that no longer matches
 * the application.
 */
function shellAt(entries: string[], screens: { path: string; element: ReactElement }[]) {
  const router = createMemoryRouter([{ element: <Layout />, children: screens }], {
    initialEntries: entries,
    initialIndex: entries.length - 1,
  });
  render(<RouterProvider router={router} />);
  return router;
}

beforeEach(() => {
  useSettingsStore.setState({ loaded: true, settings: { ...DEFAULT_SETTINGS } });
});

afterEach(cleanup);

describe("the shell provides the run lock", () => {
  it("carries a routed screen's lock out to the sidebar", async () => {
    await act(async () => {
      shellAt(["/os-builder"], [{ path: "/os-builder", element: <RunningScreen /> }]);
    });

    await screen.findByTestId("running-screen");
    // The sidebar is a *sibling* of the outlet, so this fails the moment the
    // provider goes back inside the routed screen.
    expect(screen.getByTestId("sidebar-locked").textContent).toBe(
      i18n.t("osBuilder.build.navigationLockedLane")
    );
    expect(screen.queryAllByRole("link")).toHaveLength(0);
  });

  it("leaves the sidebar alone when no screen has set it", async () => {
    // The control, measured rather than assumed: a shell that were always
    // locked would pass the case above and prove nothing.
    await act(async () => {
      shellAt(["/os-builder"], [
        { path: "/os-builder", element: <div data-testid="quiet-screen" /> },
      ]);
    });

    await screen.findByTestId("quiet-screen");
    expect(screen.queryByTestId("sidebar-locked")).toBeNull();
    expect(screen.getAllByRole("link").length).toBeGreaterThan(1);
  });
});

describe("the browser's back and forward are inside the run lock (ART-292)", () => {
  // Alt+Left and a mouse's back button were the one door the lock did not
  // cover: leaving the build tab unmounts the loop that starts the next
  // phase, so the ticked updates and first boot silently never ran.
  it("keeps a running build's screen when history goes back", async () => {
    let router!: ReturnType<typeof createMemoryRouter>;
    await act(async () => {
      router = shellAt(
        ["/", "/os-builder"],
        [
          { path: "/", element: <div data-testid="home-screen" /> },
          { path: "/os-builder", element: <RunningScreen /> },
        ]
      );
    });
    await screen.findByTestId("running-screen");

    await act(async () => {
      await router.navigate(-1);
    });

    expect(router.state.location.pathname).toBe("/os-builder");
    expect(screen.getByTestId("running-screen")).toBeTruthy();
    expect(screen.queryByTestId("home-screen")).toBeNull();
    // Refused where the user can read why: the sidebar's own sentence names
    // the OS Builder and Stop.
    expect(screen.getByTestId("sidebar-locked").textContent).toBe(
      i18n.t("osBuilder.build.navigationLockedLane")
    );
  });

  // The control: with nothing running, history goes back as it always did.
  // A shell that blocked every POP would pass the case above and break the
  // browser for every screen.
  it("lets history go back when no build is running", async () => {
    let router!: ReturnType<typeof createMemoryRouter>;
    await act(async () => {
      router = shellAt(
        ["/", "/os-builder"],
        [
          { path: "/", element: <div data-testid="home-screen" /> },
          { path: "/os-builder", element: <div data-testid="quiet-screen" /> },
        ]
      );
    });
    await screen.findByTestId("quiet-screen");

    await act(async () => {
      await router.navigate(-1);
    });

    expect(router.state.location.pathname).toBe("/");
    expect(await screen.findByTestId("home-screen")).toBeTruthy();
  });
});

describe("the shell lets go of history when the build does (ART-292)", () => {
  /** A build that is running until its own Stop is pressed. */
  function StoppableScreen() {
    const { setRunning } = useRunLock();
    useEffect(() => {
      setRunning(true);
    }, [setRunning]);
    return (
      <button data-testid="stop-build" onClick={() => setRunning(false)}>
        stop
      </button>
    );
  }

  // A refused Back must not leave the router holding the refused move: once
  // the build stops, the very same Back goes through. This is what the
  // blocker's reset is for, and the case above cannot see it.
  it("goes back once the build has stopped, after refusing while it ran", async () => {
    let router!: ReturnType<typeof createMemoryRouter>;
    await act(async () => {
      router = shellAt(
        ["/", "/os-builder"],
        [
          { path: "/", element: <div data-testid="home-screen" /> },
          { path: "/os-builder", element: <StoppableScreen /> },
        ]
      );
    });
    await screen.findByTestId("stop-build");

    await act(async () => {
      await router.navigate(-1);
    });
    expect(router.state.location.pathname).toBe("/os-builder");

    await act(async () => {
      screen.getByTestId("stop-build").click();
    });
    await act(async () => {
      await router.navigate(-1);
    });

    expect(router.state.location.pathname).toBe("/");
    expect(await screen.findByTestId("home-screen")).toBeTruthy();
  });
});
