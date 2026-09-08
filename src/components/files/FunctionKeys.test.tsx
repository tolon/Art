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

import type { CursorMove } from "@/lib/cursorKeys";

import {
  useCommandLineKey,
  useCursorKeys,
  useFunctionKeys,
  useInsertToggle,
  useMarkKeys,
  useNavigationKeys,
  usePaneHistoryKeys,
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

// Walking in and out of things (brief §3.1). Enter is the key that turns a
// file into a pane, and Backspace is the only way back out of one — a guard
// regression in either direction leaves a user inside a disk image.

function NavigationHarness({ active = true }: { active?: boolean }) {
  const [log, setLog] = useState<string[]>([]);
  useNavigationKeys(
    {
      onOpen: () => setLog((l) => [...l, "open"]),
      onUp: () => setLog((l) => [...l, "up"]),
    },
    active
  );
  return (
    <div>
      <div data-testid="log">{log.join(",")}</div>
      <input aria-label="filter box" />
    </div>
  );
}

describe("useNavigationKeys", () => {
  it("opens on Enter and on Ctrl+PgDn, and goes up on Backspace and Ctrl+PgUp", async () => {
    const user = userEvent.setup();
    render(<NavigationHarness />);

    await user.keyboard("{Enter}");
    await user.keyboard("{Control>}{PageDown}{/Control}");
    await user.keyboard("{Backspace}");
    await user.keyboard("{Control>}{PageUp}{/Control}");
    expect(screen.getByTestId("log").textContent).toBe("open,open,up,up");
  });

  it("does not fire while typing — Backspace in a text box deletes a character", async () => {
    const user = userEvent.setup();
    render(<NavigationHarness />);

    const input = screen.getByRole("textbox", { name: "filter box" });
    await user.click(input);
    await user.keyboard("abc{Backspace}{Enter}");
    expect(screen.getByTestId("log").textContent).toBe("");
    expect((input as HTMLInputElement).value).toBe("ab");
  });

  it("does nothing while inactive (a dialog is on top)", async () => {
    const user = userEvent.setup();
    render(<NavigationHarness active={false} />);

    await user.keyboard("{Enter}{Backspace}");
    expect(screen.getByTestId("log").textContent).toBe("");
  });
});

function HistoryHarness({ active = true }: { active?: boolean }) {
  const [log, setLog] = useState<string[]>([]);
  usePaneHistoryKeys(
    {
      onBack: () => setLog((l) => [...l, "back"]),
      onForward: () => setLog((l) => [...l, "forward"]),
    },
    active
  );
  return (
    <div>
      <div data-testid="log">{log.join(",")}</div>
      <input aria-label="filter box" />
    </div>
  );
}

describe("usePaneHistoryKeys", () => {
  it("fires on Alt+Left and Alt+Right", async () => {
    const user = userEvent.setup();
    render(<HistoryHarness />);

    await user.keyboard("{Alt>}{ArrowLeft}{/Alt}");
    await user.keyboard("{Alt>}{ArrowRight}{/Alt}");
    expect(screen.getByTestId("log").textContent).toBe("back,forward");
  });

  it("ignores the arrows without Alt, and Alt with another modifier", async () => {
    // Plain arrows belong to the pane's own cursor movement, not to history.
    const user = userEvent.setup();
    render(<HistoryHarness />);

    await user.keyboard("{ArrowLeft}{ArrowRight}");
    await user.keyboard("{Control>}{Alt>}{ArrowLeft}{/Alt}{/Control}");
    expect(screen.getByTestId("log").textContent).toBe("");
  });

  it("does not fire while a text field has focus", async () => {
    const user = userEvent.setup();
    render(<HistoryHarness />);

    const input = screen.getByRole("textbox", { name: "filter box" });
    await user.click(input);
    await user.keyboard("{Alt>}{ArrowLeft}{/Alt}");
    expect(screen.getByTestId("log").textContent).toBe("");
  });
});

// ART-275: the commander had no cursor keys at all — Up/Down/Home/End/
// PageUp/PageDown moved nothing, which is what the owner met driving the
// Windows 11 build mouse-free. `useCursorKeys` is the fix's keyboard half;
// `cursorKeys.test.ts` covers the arithmetic it calls into.

function CursorHarness({ active = true }: { active?: boolean }) {
  const [log, setLog] = useState<string[]>([]);
  useCursorKeys(
    (move: CursorMove, shift: boolean) => setLog((l) => [...l, `${move}:${shift}`]),
    active
  );
  return (
    <div>
      <div data-testid="log">{log.join(",")}</div>
      <input aria-label="filter box" />
    </div>
  );
}

describe("useCursorKeys", () => {
  it("reaches the handler with the right move for each of the six keys", async () => {
    const user = userEvent.setup();
    render(<CursorHarness />);

    await user.keyboard("{ArrowUp}{ArrowDown}{Home}{End}{PageUp}{PageDown}");
    expect(screen.getByTestId("log").textContent).toBe(
      "up:false,down:false,home:false,end:false,pageUp:false,pageDown:false"
    );
  });

  it("passes Shift through", async () => {
    const user = userEvent.setup();
    render(<CursorHarness />);

    await user.keyboard("{Shift>}{ArrowDown}{/Shift}");
    expect(screen.getByTestId("log").textContent).toBe("down:true");
  });

  it("Ctrl+PageDown does not reach it — that combination belongs to useNavigationKeys", async () => {
    const user = userEvent.setup();
    render(<CursorHarness />);

    await user.keyboard("{Control>}{PageDown}{/Control}");
    await user.keyboard("{Control>}{PageUp}{/Control}");
    expect(screen.getByTestId("log").textContent).toBe("");
  });

  it("does not fire while a text field has focus", async () => {
    const user = userEvent.setup();
    render(<CursorHarness />);

    const input = screen.getByRole("textbox", { name: "filter box" });
    await user.click(input);
    await user.keyboard("{ArrowUp}{ArrowDown}{Home}{End}{PageUp}{PageDown}");
    expect(screen.getByTestId("log").textContent).toBe("");
  });

  it("does nothing while inactive (a dialog is on top)", async () => {
    const user = userEvent.setup();
    render(<CursorHarness active={false} />);

    await user.keyboard("{ArrowDown}");
    expect(screen.getByTestId("log").textContent).toBe("");
  });
});

// ART-275's other half: Ctrl+Space, the owner's own `wincmd.ini` line
// (`C+SPACE=cm_ExecuteDOS`), focuses the command line.

function CommandLineHarness({ active = true }: { active?: boolean }) {
  const [count, setCount] = useState(0);
  useCommandLineKey(() => setCount((n) => n + 1), active);
  return (
    <div>
      <div data-testid="count">{count}</div>
      <input aria-label="filter box" />
    </div>
  );
}

describe("useCommandLineKey", () => {
  it("fires on Ctrl+Space", async () => {
    const user = userEvent.setup();
    render(<CommandLineHarness />);

    await user.keyboard("{Control>}{ }{/Control}");
    expect(screen.getByTestId("count").textContent).toBe("1");
  });

  it("ignores plain Space and Ctrl+Space combined with another modifier", async () => {
    const user = userEvent.setup();
    render(<CommandLineHarness />);

    await user.keyboard(" ");
    expect(screen.getByTestId("count").textContent).toBe("0");

    await user.keyboard("{Control>}{Alt>}{ }{/Alt}{/Control}");
    expect(screen.getByTestId("count").textContent).toBe("0");
  });

  it("does not fire while typing in a text field", async () => {
    const user = userEvent.setup();
    render(<CommandLineHarness />);

    const input = screen.getByRole("textbox", { name: "filter box" });
    await user.click(input);
    await user.keyboard("{Control>}{ }{/Control}");
    expect(screen.getByTestId("count").textContent).toBe("0");
  });

  it("does nothing while inactive (a dialog is on top)", async () => {
    const user = userEvent.setup();
    render(<CommandLineHarness active={false} />);

    await user.keyboard("{Control>}{ }{/Control}");
    expect(screen.getByTestId("count").textContent).toBe("0");
  });
});

// Ctrl+Space must never also reach `useMarkKeys`'s plain-Space handler — the
// two hooks are wired side by side in `FileManager.tsx`, and a shared `window`
// keydown listener that let both fire off one keystroke would both focus the
// command line *and* mark the row under the cursor.

function MarkSpaceHarness({ active = true }: { active?: boolean }) {
  const [count, setCount] = useState(0);
  useMarkKeys(
    {
      onSpace: () => setCount((n) => n + 1),
      onMarkByMask: () => {},
      onUnmarkByMask: () => {},
      onInvert: () => {},
    },
    active
  );
  return <div data-testid="count">{count}</div>;
}

describe("useMarkKeys — Ctrl+Space does not mark", () => {
  it("fires onSpace for plain Space but not for Ctrl+Space", async () => {
    const user = userEvent.setup();
    render(<MarkSpaceHarness />);

    await user.keyboard("{Control>}{ }{/Control}");
    expect(screen.getByTestId("count").textContent).toBe("0");

    await user.keyboard(" ");
    expect(screen.getByTestId("count").textContent).toBe("1");
  });
});

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
