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

import {
  useFunctionKeys,
  useInsertToggle,
  usePaneTab,
  useRefreshKey,
  useSelectAll,
  type FunctionAction,
} from "./FunctionKeys";

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

// Refresh matters more than the other three here: phase 2b task 3 hides the
// button strip Refresh used to live in, so from that task on these two keys
// are the *only* way to re-read a pane. A guard regression would leave a
// commander that cannot see a file the user just wrote from somewhere else.

function RefreshHarness({ active = true }: { active?: boolean }) {
  const [count, setCount] = useState(0);
  useRefreshKey(() => setCount((n) => n + 1), active);
  return (
    <div>
      <div data-testid="count">{count}</div>
      <input aria-label="filter box" />
    </div>
  );
}

describe("useRefreshKey", () => {
  it("fires on F2 and on Ctrl+R", async () => {
    const user = userEvent.setup();
    render(<RefreshHarness />);

    await user.keyboard("{F2}");
    expect(screen.getByTestId("count").textContent).toBe("1");

    await user.keyboard("{Control>}r{/Control}");
    expect(screen.getByTestId("count").textContent).toBe("2");
  });

  it("ignores plain R, and F2 or Ctrl+R held with another modifier", async () => {
    const user = userEvent.setup();
    render(<RefreshHarness />);

    await user.keyboard("r");
    await user.keyboard("{Control>}{F2}{/Control}");
    await user.keyboard("{Alt>}{F2}{/Alt}");
    await user.keyboard("{Control>}{Alt>}r{/Alt}{/Control}");
    expect(screen.getByTestId("count").textContent).toBe("0");
  });

  it("does not fire while typing in a text field", async () => {
    const user = userEvent.setup();
    render(<RefreshHarness />);

    const input = screen.getByRole("textbox", { name: "filter box" });
    await user.click(input);
    await user.keyboard("{F2}");
    await user.keyboard("{Control>}r{/Control}");
    expect(screen.getByTestId("count").textContent).toBe("0");
  });

  it("does nothing while inactive (a dialog is on top)", async () => {
    const user = userEvent.setup();
    render(<RefreshHarness active={false} />);

    await user.keyboard("{F2}");
    expect(screen.getByTestId("count").textContent).toBe("0");
  });
});

// F6 and Shift+F6 are two different operations on one key — Move, which
// deletes the original, and rename, which does not. Getting the match wrong
// in either direction means a keystroke doing something the user did not ask
// for, and one of the two directions destroys data. So it is checked here
// rather than inferred from reading the `find` call.

function ShiftHarness() {
  const [fired, setFired] = useState<string[]>([]);
  const actions: FunctionAction[] = [
    { key: "F6", label: "Move", enabled: true, run: () => setFired((f) => [...f, "move"]) },
    {
      key: "F6",
      shift: true,
      label: "Rename",
      enabled: true,
      run: () => setFired((f) => [...f, "rename"]),
    },
  ];
  useFunctionKeys(actions, true);
  return <div data-testid="fired">{fired.join(",")}</div>;
}

describe("useFunctionKeys and the shifted variant of a key", () => {
  it("runs F6 for F6 and Shift+F6 for Shift+F6, never both and never the wrong one", async () => {
    const user = userEvent.setup();
    render(<ShiftHarness />);

    await user.keyboard("{F6}");
    expect(screen.getByTestId("fired").textContent).toBe("move");

    await user.keyboard("{Shift>}{F6}{/Shift}");
    expect(screen.getByTestId("fired").textContent).toBe("move,rename");
  });
});
