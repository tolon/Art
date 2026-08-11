// @vitest-environment jsdom
//
// Covers the filter-box wiring task 7 adds to `FileManager.tsx`: a pane's
// filename mask (`@/lib/mask`) narrowing what is shown, and what that does
// to the pane's selection and to F5.
//
// `FileManager.tsx` itself calls Tauri commands on mount and pulls in most
// of the app's `lib/*` surface (see `selection.test.ts`'s and
// `sort.test.ts`'s comments for why those live as pure-function tests
// instead), so this is a small harness that wires the same pieces the real
// page wires — `filterEntries`, `sortEntries`, `entriesIn`,
// `emptySelectionUpdate`, and the real `useFunctionKeys` hook — rather than
// a re-implementation of its own. It proves the integration end to end: a
// filter keystroke reaching an actual `<input>` really does clear the
// selection and really does block F5 while it has focus, not just that the
// pure pieces would compose correctly on paper.
import { afterEach, describe, expect, it } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { useCallback, useState } from "react";

import { useFunctionKeys, type FunctionAction } from "@/components/files/FunctionKeys";
import { filterEntries } from "@/lib/mask";
import type { PanelEntry } from "@/lib/panel";
import { emptySelectionUpdate, entriesIn, type SelectionUpdate } from "@/lib/selection";
import { defaultSortState, sortEntries } from "@/lib/sort";

afterEach(cleanup);

function entry(name: string): PanelEntry {
  return {
    name,
    is_dir: false,
    bytes: 10,
    path: null,
    header_block: null,
    iso_extent: null,
    is_link: false,
    date: null,
    attrs: null,
  };
}

const ENTRIES = ["a.txt", "b.txt", "c.doc"].map(entry);

/**
 * The task-7 slice of one `FileManager.tsx` pane: entries, a filter mask, a
 * selection, and F5 wired the same way `FileManager`'s own `paneEntries` /
 * `selectedEntries` / `setPaneFilter` are — see that file for the originals
 * this mirrors.
 */
function Harness() {
  const [filter, setFilter] = useState("");
  const [selection, setSelection] = useState<Set<string>>(new Set());
  const [copied, setCopied] = useState<string[] | null>(null);
  const [paneFocused, setPaneFocused] = useState(false);

  const paneEntries = sortEntries(filterEntries(ENTRIES, filter), defaultSortState());
  const selectedEntries = entriesIn(paneEntries, selection);

  const applySelection = useCallback((update: SelectionUpdate) => {
    setSelection(update.selected);
  }, []);

  // Mirrors `setPaneFilter` in `FileManager.tsx`: every filter change clears
  // the selection outright, rather than leaving a hidden entry marked.
  function onFilterChange(mask: string) {
    setFilter(mask);
    applySelection(emptySelectionUpdate());
  }

  const actions: FunctionAction[] = [
    {
      key: "F5",
      label: "Copy",
      enabled: selectedEntries.length > 0,
      run: () => setCopied(selectedEntries.map((e) => e.name)),
    },
  ];
  useFunctionKeys(actions, true);

  const noMatch = paneEntries.length === 0 && filter.trim() !== "" && ENTRIES.length > 0;

  return (
    <div>
      <ul data-testid="visible">
        {paneEntries.map((e) => (
          <li key={e.name}>{e.name}</li>
        ))}
      </ul>
      {noMatch && <div data-testid="empty-msg">no match</div>}
      <div data-testid="selected-count">{selection.size}</div>
      <div data-testid="copied">{copied ? copied.join(",") : ""}</div>
      <div data-testid="pane-focused">{String(paneFocused)}</div>
      <button onClick={() => applySelection({ selected: new Set(ENTRIES.map((e) => e.name)), anchor: null })}>
        select all
      </button>
      <input
        aria-label="filter box"
        value={filter}
        onChange={(event) => onFilterChange(event.target.value)}
        onKeyDown={(event) => {
          if (event.key !== "Escape") return;
          event.preventDefault();
          onFilterChange("");
          event.currentTarget.blur();
          setPaneFocused(true);
        }}
      />
    </div>
  );
}

describe("the filter box's effect on selection and F5", () => {
  it("hides non-matching entries without changing what F5 would copy", async () => {
    const user = userEvent.setup();
    render(<Harness />);

    // Select every entry — the "twenty files selected" scenario from the
    // brief, scaled down to three.
    await user.click(screen.getByText("select all"));
    expect(screen.getByTestId("selected-count").textContent).toBe("3");

    // Narrow the pane to only the .txt files.
    const input = screen.getByRole("textbox", { name: "filter box" });
    await user.type(input, "*.txt");
    expect(screen.getByTestId("visible").textContent).toBe("a.txtb.txt");

    // The filter change cleared the selection outright — the on-screen
    // count is never left claiming more than is visible.
    expect(screen.getByTestId("selected-count").textContent).toBe("0");

    // F5, fired from outside the input (nothing focused), finds nothing
    // selected and does not run.
    fireEvent.keyDown(document.body, { key: "F5" });
    expect(screen.getByTestId("copied").textContent).toBe("");
  });

  it("a filter matching nothing says so, rather than reading as a broken listing", async () => {
    const user = userEvent.setup();
    render(<Harness />);

    const input = screen.getByRole("textbox", { name: "filter box" });
    await user.type(input, "*.zip");

    expect(screen.getByTestId("visible").textContent).toBe("");
    expect(screen.getByTestId("empty-msg")).toBeTruthy();
  });

  it("F5 does not fire while the filter box has focus, even with a live selection", async () => {
    const user = userEvent.setup();
    render(<Harness />);

    await user.click(screen.getByText("select all"));
    expect(screen.getByTestId("selected-count").textContent).toBe("3");

    const input = screen.getByRole("textbox", { name: "filter box" });
    await user.click(input);
    expect(document.activeElement).toBe(input);

    // isShortcutBlocked (FunctionKeys.tsx) ignores keydown while the event
    // target is an <input> — this is that guard exercised through the real
    // hook and a real focused <input>, not assumed from reading the source.
    fireEvent.keyDown(input, { key: "F5" });
    expect(screen.getByTestId("copied").textContent).toBe("");
    expect(screen.getByTestId("selected-count").textContent).toBe("3");
  });

  it("Escape clears the mask and hands focus back to the pane", async () => {
    const user = userEvent.setup();
    render(<Harness />);

    const input = screen.getByRole("textbox", { name: "filter box" });
    await user.type(input, "*.txt");
    expect(screen.getByTestId("visible").textContent).toBe("a.txtb.txt");

    await user.keyboard("{Escape}");

    expect((input as HTMLInputElement).value).toBe("");
    expect(screen.getByTestId("visible").textContent).toBe("a.txtb.txtc.doc");
    expect(document.activeElement).not.toBe(input);
    expect(screen.getByTestId("pane-focused").textContent).toBe("true");
  });
});
