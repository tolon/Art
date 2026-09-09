// @vitest-environment jsdom
//
// Regression guard for the sidebar's version display. It used to be a
// literal `v0.1` baked into the JSX — a second source of truth that was
// already wrong once the project passed 0.1.0 in substance if not in name.
// It now renders `import.meta.env.VITE_APP_VERSION`, set by vite.config.ts
// from package.json's own `version` field, so this test fails the moment
// the interface goes back to any hardcoded string: it does not know today's
// number in advance, it only knows the rendered text must match whatever
// `VITE_APP_VERSION` carries.
import { afterEach, describe, expect, it } from "vitest";
import { cleanup, render, screen, within } from "@testing-library/react";
import { MemoryRouter } from "react-router-dom";
import i18n from "i18next";

// Side-effecting: a real, synchronously-initialised i18next instance, so the
// case that names the refusal names the sentence and not the key.
import "@/i18n";

import { RunLockContext } from "@/pages/osbuilder/runLock";
import { Sidebar } from "./Sidebar";

afterEach(cleanup);

/** The sidebar under a lock that is on or off, on a route that is not `/`. */
function renderSidebar(running: boolean) {
  return render(
    <MemoryRouter initialEntries={["/files"]}>
      <RunLockContext.Provider value={{ running, setRunning: () => {} }}>
        <Sidebar />
      </RunLockContext.Provider>
    </MemoryRouter>,
  );
}

describe("Sidebar version", () => {
  it("renders VITE_APP_VERSION, not a hardcoded literal", () => {
    render(
      <MemoryRouter>
        <Sidebar />
      </MemoryRouter>,
    );

    expect(screen.getByText(`v${import.meta.env.VITE_APP_VERSION}`)).toBeTruthy();
  });
});

// ---------------------------------------------------------------------------
//
// The run lock reaches the shell (round 5, task 3).
//
// The strip inside the OS Builder went dead in round 4, and every one of the
// fifteen ways out of the lane down the left-hand side did not: a build was
// abandoned by clicking *Dosyalar* in the sidebar exactly as easily as by
// clicking a tab. The lock's provider is the shell's now, and the sidebar
// reads it — the same answer the strip gives, in the same words.
describe("the sidebar is locked while a build runs", () => {
  it("leaves every entry a link when nothing is running", () => {
    const { container } = renderSidebar(false);

    // The control, measured rather than assumed: a sidebar that were always
    // dead would pass the case below and prove nothing.
    expect(within(container).getAllByRole("link").length).toBeGreaterThan(1);
    expect(within(container).queryAllByText(i18n.t("nav.dashboard"))[0]?.closest("a")).toBeTruthy();
    expect(screen.queryByTestId("sidebar-locked")).toBeNull();
  });

  it("draws every entry as a dead span, keeps the active one marked, and says why", () => {
    // Both arms, side by side, so the count the locked arm has to match is
    // *measured* off the unlocked sidebar rather than copied from `NAV` —
    // a test that reads the table instead of the file is a copy, and copies
    // drift when an entry is added.
    const { container: open } = renderSidebar(false);
    const { container: shut } = renderSidebar(true);

    const entries = within(open).getAllByRole("link").length;

    // **Not links at all.** A `NavLink` styled to look disabled still
    // navigates on Enter, on a middle click and through a screen reader —
    // what has to go is the element that navigates.
    expect(within(shut).queryAllByRole("link")).toHaveLength(0);
    const dead = shut.querySelectorAll("span.sidebar-link[aria-disabled='true']");
    expect(dead).toHaveLength(entries);

    // A closed destination still reads as a destination, and the one the
    // user is standing on still looks like the one they are standing on.
    const files = Array.from(dead).find(
      (el) => el.textContent === i18n.t("nav.files")
    );
    expect(files).toBeTruthy();
    expect(files?.className).toContain("sidebar-link-active");

    // A refusal must be actionable, and this one names the control that
    // lifts it — the same sentence the OS Builder's own strip carries.
    expect(within(shut).getByTestId("sidebar-locked").textContent).toBe(
      i18n.t("osBuilder.build.navigationLocked")
    );
  });
});
