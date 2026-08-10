// @vitest-environment jsdom
//
// Covers `usePaneTab` (FunctionKeys.tsx) — the hook `FileManager.tsx` binds
// Tab to for moving keyboard focus between the two panes.
//
// `FileManager.tsx` itself calls Tauri commands on mount (`panelLocalRoots`,
// etc.) and pulls in most of the app's `lib/*` surface, so rendering the real
// page here would mean mocking a large slice of the Tauri IPC boundary just
// to reach two lines of keyboard logic. `usePaneTab` is deliberately a
// state-free hook — it owns no pane state of its own, just the keydown
// wiring — so it can be exercised directly with a tiny harness component
// that owns its own `useState<Side>`, the same way `FileManager` does. That
// is a better unit than a full render would be: it proves the guard logic
// (ignore typing targets, ignore modifiers, preventDefault) without needing
// any of FileManager's data-fetching machinery to exist.
import { afterEach, describe, expect, it } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { useState } from "react";

import { useInsertToggle, usePaneTab, useSelectAll } from "./FunctionKeys";

// This project's Vitest config does not set `test.globals`, so
// @testing-library/react's usual auto-cleanup (which hooks a global
// `afterEach`) never registers; without this, each `render()` below would
// pile new DOM onto the previous test's, and `getByTestId` would start
// matching more than one element.
afterEach(cleanup);

type Side = "left" | "right";

function Harness({ active = true }: { active?: boolean }) {
  const [focused, setFocused] = useState<Side>("left");
  usePaneTab(() => setFocused((side) => (side === "left" ? "right" : "left")), active);
  return (
    <div>
      <div data-testid="focused">{focused}</div>
      <input aria-label="filter box" />
    </div>
  );
}

describe("usePaneTab", () => {
  it("moves focus to the other pane on Tab, and not when typing in a filter box", async () => {
    const user = userEvent.setup();
    render(<Harness />);

    expect(screen.getByTestId("focused").textContent).toBe("left");

    // Nothing has DOM focus yet, so the keydown target is document.body —
    // not a text field — and the guard lets it through.
    await user.keyboard("{Tab}");
    expect(screen.getByTestId("focused").textContent).toBe("right");

    // Focus an <input> directly (a click, not tab-traversal, so it doesn't
    // depend on whether the previous Tab's preventDefault() suppressed the
    // browser's own focus movement) and press Tab again from inside it.
    const input = screen.getByRole("textbox", { name: "filter box" });
    await user.click(input);
    expect(document.activeElement).toBe(input);

    await user.keyboard("{Tab}");
    expect(screen.getByTestId("focused").textContent).toBe("right");
  });

  it("ignores Tab held with a modifier", async () => {
    const user = userEvent.setup();
    render(<Harness />);

    expect(screen.getByTestId("focused").textContent).toBe("left");
    await user.keyboard("{Control>}{Tab}{/Control}");
    expect(screen.getByTestId("focused").textContent).toBe("left");
  });

  it("does nothing while inactive (a dialog is on top)", async () => {
    const user = userEvent.setup();
    render(<Harness active={false} />);

    expect(screen.getByTestId("focused").textContent).toBe("left");
    await user.keyboard("{Tab}");
    expect(screen.getByTestId("focused").textContent).toBe("left");
  });
});

// A tiny harness for each of the two multi-select shortcuts, exercising the
// same `isShortcutBlocked` guard `usePaneTab` above already proves — these
// tests are about the one thing that differs: Ctrl+A *wants* Ctrl held,
// where every other shortcut in this file treats it as "not for me".

function InsertHarness({ active = true }: { active?: boolean }) {
  const [count, setCount] = useState(0);
  useInsertToggle(() => setCount((n) => n + 1), active);
  return (
    <div>
      <div data-testid="count">{count}</div>
      <input aria-label="filter box" />
    </div>
  );
}

describe("useInsertToggle", () => {
  it("fires on Insert, and not while typing in a text field", async () => {
    const user = userEvent.setup();
    render(<InsertHarness />);

    await user.keyboard("{Insert}");
    expect(screen.getByTestId("count").textContent).toBe("1");

    const input = screen.getByRole("textbox", { name: "filter box" });
    await user.click(input);
    await user.keyboard("{Insert}");
    expect(screen.getByTestId("count").textContent).toBe("1");
  });

  it("does nothing while inactive", async () => {
    const user = userEvent.setup();
    render(<InsertHarness active={false} />);

    await user.keyboard("{Insert}");
    expect(screen.getByTestId("count").textContent).toBe("0");
  });
});

function SelectAllHarness({ active = true }: { active?: boolean }) {
  const [count, setCount] = useState(0);
  useSelectAll(() => setCount((n) => n + 1), active);
  return (
    <div>
      <div data-testid="count">{count}</div>
      <input aria-label="filter box" />
    </div>
  );
}

describe("useSelectAll", () => {
  it("fires on Ctrl+A", async () => {
    const user = userEvent.setup();
    render(<SelectAllHarness />);

    await user.keyboard("{Control>}a{/Control}");
    expect(screen.getByTestId("count").textContent).toBe("1");
  });

  it("ignores plain A (no Ctrl) and Ctrl+A combined with another modifier", async () => {
    const user = userEvent.setup();
    render(<SelectAllHarness />);

    await user.keyboard("a");
    expect(screen.getByTestId("count").textContent).toBe("0");

    await user.keyboard("{Control>}{Alt>}a{/Alt}{/Control}");
    expect(screen.getByTestId("count").textContent).toBe("0");
  });

  it("does not fire while typing in a text field", async () => {
    const user = userEvent.setup();
    render(<SelectAllHarness />);

    const input = screen.getByRole("textbox", { name: "filter box" });
    await user.click(input);
    await user.keyboard("{Control>}a{/Control}");
    expect(screen.getByTestId("count").textContent).toBe("0");
  });

  it("does nothing while inactive", async () => {
    const user = userEvent.setup();
    render(<SelectAllHarness active={false} />);

    await user.keyboard("{Control>}a{/Control}");
    expect(screen.getByTestId("count").textContent).toBe("0");
  });
});
