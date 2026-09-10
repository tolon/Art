// @vitest-environment jsdom
//
// ART-069: the first test that renders the **real** `FileManager.tsx`.
//
// Every frontend test this screen had before now extracted a pure piece and
// tested that — `@/lib/selection`, `@/lib/functionKeyPlan`, `usePaneTab` and
// `isShortcutBlocked` in `FunctionKeys.tsx` — or stood a harness up beside it
// (`FileManagerFilter.test.tsx`, `FileManagerFocus.test.tsx`, whose own
// headers say so). Each of those extractions is real, tested logic. None of
// them proves the page *wires* the extracted piece correctly: that the two
// result listeners are registered at mount before any button can start a job,
// that a click handler calls the selection function it looks like it calls,
// that an F-key's `run` acts on the row its `enabled` was computed from. A
// harness is guaranteed to pass those, because a harness is written from the
// same reading of the code the assertion is.
//
// Mocked at the same boundary the rest of this suite mocks at — the
// `@/lib/*` wrappers around `invoke`/`listen`, never `@tauri-apps/api` itself
// (`useRomPairing.test.tsx`, `FilesTab.test.tsx`). `@/lib/settings` is
// mocked one layer further down for the reason `FilesTab.test.tsx` records:
// `useRemembered` writes through `useSettingsStore.update()` into
// `saveSettings`, the real `tauri-plugin-store` boundary, which in jsdom
// rejects with nothing to catch it and fails the run as an unhandled
// rejection.
//
// What this establishes:
//   1. The screen mounts, with two real panes and a real listing in each.
//   2. Both write-result listeners are subscribed **at mount**, before any
//      control can start a job — the ordering ART-069 names.
//   3. Clicking a row selects it, and Ctrl+click adds to the selection: the
//      click handler really does reach `@/lib/selection`.
//   4. An F-key's `run` acts on the row that is selected — F5 with two local
//      panes refuses with the "both local" sentence, which is only reachable
//      if `run` read the same focused pane the plan did.
//   5. Nothing on screen is a raw i18n key or an unrendered
//      `{{interpolation}}`, in English and in Turkish (ART-062's automatable
//      half; the "does it fit" half is still a real-screen job).
//
// What this does not establish: layout. jsdom does no layout, so nothing here
// measures whether a Turkish label overflows its button.

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { MemoryRouter, useLocation } from "react-router-dom";

import { changeLanguage } from "@/i18n";
import { useSettingsStore } from "@/stores/settingsStore";
import type { LocalListing, PanelEntry } from "@/lib/panel";

// --- the IPC surface, mocked at `@/lib/*` ------------------------------------

const listLocalMock = vi.hoisted(() => vi.fn());
const localRootsMock = vi.hoisted(() => vi.fn());
const onVolumeWriteResultMock = vi.hoisted(() => vi.fn());
const onJobProgressMock = vi.hoisted(() => vi.fn());
const onArchivesPlanResultMock = vi.hoisted(() => vi.fn());
const onDirSizeResultMock = vi.hoisted(() => vi.fn());
const analyzePathsMock = vi.hoisted(() => vi.fn());
const startDragMock = vi.hoisted(() => vi.fn());

vi.mock("@/lib/panel", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/lib/panel")>()),
  panelListLocal: listLocalMock,
  panelLocalRoots: localRootsMock,
  onDirSizeResult: onDirSizeResultMock,
}));

vi.mock("@/lib/volumeWrite", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/lib/volumeWrite")>()),
  onVolumeWriteResult: onVolumeWriteResultMock,
}));

vi.mock("@/lib/jobs", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/lib/jobs")>()),
  onJobProgress: onJobProgressMock,
}));

vi.mock("@/lib/archives", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/lib/archives")>()),
  onArchivesPlanResult: onArchivesPlanResultMock,
}));

vi.mock("@/lib/api", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/lib/api")>()),
  analyzePaths: analyzePathsMock,
}));

vi.mock("@tauri-apps/plugin-dialog", () => ({
  open: vi.fn(),
  confirm: vi.fn().mockResolvedValue(false),
  save: vi.fn(),
}));

vi.mock("@crabnebula/tauri-plugin-drag", () => ({
  startDrag: startDragMock,
}));

vi.mock("@/lib/settings", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/lib/settings")>()),
  getSettings: vi.fn(),
  saveSettings: vi.fn().mockResolvedValue(undefined),
}));

const { FileManager } = await import("@/pages/FileManager");
const { DEFAULT_SETTINGS } = await import("@/lib/settings");

// --- fixtures ----------------------------------------------------------------

function entry(name: string, overrides: Partial<PanelEntry> = {}): PanelEntry {
  return {
    name,
    is_dir: false,
    bytes: 1024,
    path: `C:\\Amiga\\${name}`,
    header_block: null,
    attrs: null,
    comment: null,
    modified: null,
    ...overrides,
  } as PanelEntry;
}

const LISTING: LocalListing = {
  path: "C:\\Amiga",
  parent: "C:\\",
  truncated: false,
  entries: [
    entry("Games", { is_dir: true, bytes: 0, path: "C:\\Amiga\\Games" }),
    entry("Turrican.adf"),
    entry("Xenon2.adf"),
  ],
};

/** A subscribe mock shaped like the real one: a promise of an unlisten fn. */
function subscription() {
  return vi.fn().mockResolvedValue(() => {});
}

beforeEach(() => {
  listLocalMock.mockReset().mockResolvedValue(LISTING);
  localRootsMock.mockReset().mockResolvedValue(["C:\\"]);
  onVolumeWriteResultMock.mockReset().mockImplementation(subscription());
  onJobProgressMock.mockReset().mockImplementation(subscription());
  onArchivesPlanResultMock.mockReset().mockImplementation(subscription());
  onDirSizeResultMock.mockReset().mockImplementation(subscription());
  analyzePathsMock.mockReset().mockResolvedValue([]);
  startDragMock.mockReset();

  // The cold-start effect is gated on the settings having arrived
  // (`settingsLoaded`, ART-089): without this the panes never open at all and
  // every assertion below would be about an empty screen.
  useSettingsStore.setState({
    loaded: true,
    settings: {
      ...DEFAULT_SETTINGS,
      defaultLeftPath: "C:\\Amiga",
      defaultRightPath: "C:\\Amiga",
      alwaysUseDefaultFolders: true,
    },
  });
});

afterEach(async () => {
  cleanup();
  useSettingsStore.setState({ loaded: false, settings: DEFAULT_SETTINGS });
  await changeLanguage("en");
});

async function renderScreen() {
  const utils = render(
    <MemoryRouter>
      <FileManager />
    </MemoryRouter>
  );
  // Both panes open from the same local root, so the fixture's rows appear
  // twice. Waiting on that is waiting on the screen being *finished*, not
  // merely mounted.
  await waitFor(() => expect(screen.getAllByText("Turrican.adf").length).toBe(2));
  return utils;
}

// -----------------------------------------------------------------------------

describe("FileManager renders", () => {
  it("mounts with two panes, each showing a real listing", async () => {
    const { container } = await renderScreen();

    // A drawer renders in Norton Commander's brackets, which is itself worth
    // pinning: it is the one visual difference between a folder row and a
    // file row in the name column.
    expect(screen.getAllByText("[Games]").length).toBe(2);
    expect(screen.getAllByText("Xenon2.adf").length).toBe(2);
    expect(panes(container).length).toBe(2);
    // Both panes asked for their own listing — a screen that opened one pane
    // and left the other blank would still satisfy a bare "did it mount".
    expect(listLocalMock).toHaveBeenCalledWith("C:\\Amiga");
    expect(listLocalMock.mock.calls.length).toBeGreaterThanOrEqual(2);
  });

  /// The ordering ART-069 names: the two result listeners must be registered
  /// before any control can start a job, or a job that finishes quickly
  /// reports into nothing.
  it("subscribes both write-result listeners at mount", async () => {
    await renderScreen();

    expect(onVolumeWriteResultMock).toHaveBeenCalled();
    expect(onJobProgressMock).toHaveBeenCalled();
    // And with a handler, not merely called: `listen(event, undefined)` would
    // satisfy the line above and deliver nothing.
    expect(typeof onVolumeWriteResultMock.mock.calls[0][0]).toBe("function");
    expect(typeof onJobProgressMock.mock.calls[0][0]).toBe("function");
  });
});

describe("FileManager wiring", () => {
  it("clicking a row selects it, and Ctrl+click adds to the selection", async () => {
    const user = userEvent.setup();
    const { container } = await renderScreen();

    // Scoped to the left pane deliberately: both panes hold the same names,
    // and an unscoped query would pass while clicking in the pane the user is
    // not in — which is exactly the wiring mistake this is here to catch.
    const left = panes(container)[0];
    expect(selectedNames(left)).toEqual([]);

    await user.click(within(left).getByText("Turrican.adf"));
    await waitFor(() => expect(selectedNames(left)).toEqual(["Turrican.adf"]));
    expect(selectedNames(panes(container)[1])).toEqual([]);

    await user.keyboard("{Control>}");
    await user.click(within(left).getByText("Xenon2.adf"));
    await user.keyboard("{/Control}");

    await waitFor(() =>
      expect(selectedNames(left).sort()).toEqual(["Turrican.adf", "Xenon2.adf"])
    );
  });

  /// F5 between two **local** panes is refused by name, and the sentence only
  /// appears if `run` read the same focused pane the enablement did. A screen
  /// whose F-key ran against the other pane would copy instead of refusing.
  it("F5 with two local panes refuses by name rather than doing nothing", async () => {
    const user = userEvent.setup();
    const { container } = await renderScreen();

    await user.click(within(panes(container)[0]).getByText("Turrican.adf"));
    await user.keyboard("{F5}");

    await waitFor(() =>
      expect(screen.getByText(/Both panes are local folders/i)).toBeTruthy()
    );
  });
});

describe("FileManager strings", () => {
  it.each(["en", "tr"] as const)("renders no raw keys or interpolations in %s", async (language) => {
    await changeLanguage(language);
    const { container } = await renderScreen();

    const text = container.textContent ?? "";
    expect(text).not.toMatch(/\{\{[a-zA-Z]/);
    // A missing key renders as the key itself: `files.functionKeys.copy`.
    //
    // **No `\b` here.** It was there once, and it made this assertion dead:
    // the function-key bar renders its label immediately after the key name,
    // so the screen's text reads `…F3files.functionKeys.view…`, and `3` to
    // `f` is not a word boundary at all. A genuinely leaked key rendered and
    // this test passed. The namespaces are listed rather than matched by a
    // generic `a.b.c` shape, which would fire on a filename or a path.
    expect(text).not.toMatch(/(files|common|components)\.[a-zA-Z]+\.[a-zA-Z]/);
  });
});

/** The two `.tc-pane` elements, left then right, in document order. */
function panes(container: HTMLElement): HTMLElement[] {
  return Array.from(container.querySelectorAll<HTMLElement>(".tc-pane"));
}

/**
 * The names of the rows a pane currently has selected.
 *
 * Read off the row's own text colour, because that is how the screen shows a
 * selection: `--tc-selected-text` for a marked row, the cursor's colour or
 * the file-type colour otherwise. Reading the DOM rather than a test-only
 * attribute is the point — an attribute added for this test would be a thing
 * the test keeps true rather than a thing the user sees.
 */
function selectedNames(pane: HTMLElement): string[] {
  return Array.from(pane.querySelectorAll<HTMLElement>("li.tc-row"))
    .filter((row) => row.style.color === "var(--tc-selected-text)")
    .map((row) => row.querySelector(".tc-name-text")?.textContent ?? "")
    .map((name) => name.replace(/^\[(.*)\]$/, "$1"));
}

// -----------------------------------------------------------------------------
// ART-283 — a path handed over by navigation must not repoint a remembered tab
// -----------------------------------------------------------------------------

/** What the router's current entry carries, so a test can see the handover
 *  being consumed rather than replayed on the next mount. */
function LocationProbe() {
  const location = useLocation();
  return <span data-testid="location-state">{JSON.stringify(location.state)}</span>;
}

describe("a path handed over by navigation (ART-283)", () => {
  const sort = { column: "name", direction: "asc" };
  /** Two remembered tabs on the left, the second one active; one on the right. */
  const SAVED_SESSION = {
    left: {
      active: 1,
      tabs: [
        { id: "tab-1", location: { kind: "local", path: "D:\\test" }, sort, filter: "" },
        { id: "tab-2", location: { kind: "local", path: "E:\\iso" }, sort, filter: "" },
      ],
    },
    right: {
      active: 0,
      tabs: [{ id: "tab-3", location: { kind: "local", path: "F:\\" }, sort, filter: "" }],
    },
    focused: "left",
    commandHistory: [],
  };

  function leftPaths(): string[] {
    const session = useSettingsStore.getState().settings.filesSession as typeof SAVED_SESSION;
    return session.left.tabs.map((tab) => tab.location.path);
  }

  beforeEach(() => {
    // Every folder lists as itself, so the pane's location is the path asked for.
    listLocalMock.mockImplementation(async (path: string) => ({ ...LISTING, path }));
    // The dashboard hands a folder over as `directory`.
    analyzePathsMock.mockImplementation(async (paths: string[]) =>
      paths.map((path) => ({ path, plan: { detection: { category: "directory" } } }))
    );
    useSettingsStore.setState({
      loaded: true,
      settings: {
        ...DEFAULT_SETTINGS,
        filesSession: SAVED_SESSION,
        defaultLeftPath: null,
        defaultRightPath: null,
        alwaysUseDefaultFolders: false,
      },
    });
  });

  // The defect as measured on the owner's settings file: opening /files with a
  // folder in `location.state` rewrote `left.tabs[1].location.path` from the
  // remembered `iso` to the dropped `adf` and saved it. The user asked to see
  // a folder; they did not ask for a remembered tab to point somewhere else.
  it("opens the folder in a new tab and leaves every remembered tab where it was", async () => {
    render(
      <MemoryRouter initialEntries={[{ pathname: "/files", state: { path: "E:\\adf" } }]}>
        <FileManager />
        <LocationProbe />
      </MemoryRouter>
    );

    await waitFor(() => expect(leftPaths()).toContain("E:\\adf"));
    // Settle: the restore's own opens and the handover have all landed.
    await waitFor(() => expect(screen.getByTestId("location-state").textContent).toBe("null"));

    expect(leftPaths()).toEqual(["D:\\test", "E:\\iso", "E:\\adf"]);
    const session = useSettingsStore.getState().settings.filesSession as typeof SAVED_SESSION;
    expect(session.left.tabs[session.left.active].location.path).toBe("E:\\adf");
    expect(session.right.tabs.map((tab) => tab.location.path)).toEqual(["F:\\"]);
  });

  // The handover is one-shot. Left in the history entry it replays on every
  // mount of this screen — which is how "opening /files" moved the settings
  // file's hash every time — and, with the fix above, would spawn a tab per
  // visit.
  it("consumes the handover so a later mount of the same entry does not replay it", async () => {
    render(
      <MemoryRouter initialEntries={[{ pathname: "/files", state: { path: "E:\\adf" } }]}>
        <FileManager />
        <LocationProbe />
      </MemoryRouter>
    );
    await waitFor(() => expect(leftPaths()).toContain("E:\\adf"));
    await waitFor(() => expect(screen.getByTestId("location-state").textContent).toBe("null"));
    expect(leftPaths().filter((path) => path === "E:\\adf")).toHaveLength(1);
  });

  it("does not open a second tab when the active tab already shows that folder", async () => {
    render(
      <MemoryRouter initialEntries={[{ pathname: "/files", state: { path: "E:\\iso" } }]}>
        <FileManager />
        <LocationProbe />
      </MemoryRouter>
    );
    await waitFor(() => expect(screen.getByTestId("location-state").textContent).toBe("null"));
    expect(leftPaths()).toEqual(["D:\\test", "E:\\iso"]);
  });
});

// -----------------------------------------------------------------------------
// The commander's columns — design § 5 of the 2026-09-08 simplification spec,
// asked for by the owner on 2026-09-10: *"ad uzantı boyut tarih vb aralıkları
// manuel ayarlanamıyor. Ayarlanabilmeli ve yapılan ayarı unutmamalı."*

/** jsdom has no `PointerEvent`; a `MouseEvent` carrying a `pointerId` is what
 *  the grip reads. */
if (!("PointerEvent" in window)) {
  class TestPointerEvent extends MouseEvent {
    pointerId: number;
    constructor(type: string, init: PointerEventInit = {}) {
      super(type, init);
      this.pointerId = init.pointerId ?? 0;
    }
  }
  (window as unknown as { PointerEvent: typeof TestPointerEvent }).PointerEvent = TestPointerEvent;
}

function storedColumns(): unknown {
  // `remembered` is `null` until the first remembered value is written.
  const remembered = useSettingsStore.getState().settings.remembered as Record<
    string,
    unknown
  > | null;
  return remembered?.["files.columns"];
}

function seedColumns(value: unknown) {
  const settings = useSettingsStore.getState().settings;
  useSettingsStore.setState({
    settings: {
      ...settings,
      remembered: { ...(settings.remembered as Record<string, unknown>), "files.columns": value },
    },
  });
}

function headerRows(container: HTMLElement): HTMLElement[] {
  return Array.from(container.querySelectorAll<HTMLElement>(".tc-header-row"));
}

function columnsVar(container: HTMLElement): string {
  return container
    .querySelector<HTMLElement>(".tc-commander")!
    .style.getPropertyValue("--tc-columns");
}

describe("the commander's columns", () => {
  it("leaves the screen exactly as it was while nothing is stored", async () => {
    const { container } = await renderScreen();
    expect(columnsVar(container)).toBe("");
    expect(headerRows(container)[0].querySelectorAll(".tc-cell").length).toBe(5);
  });

  it("hides a column from the header's menu, in both panes, and remembers it", async () => {
    const { container } = await renderScreen();
    fireEvent.contextMenu(headerRows(container)[0]);
    await userEvent.click(screen.getByRole("menuitemcheckbox", { name: "Ext" }));

    expect((storedColumns() as { shown: string[] }).shown).toEqual(["name", "size", "date", "attr"]);
    for (const row of headerRows(container)) expect(row.querySelector(".tc-cell-ext")).toBeNull();
    expect(container.querySelectorAll(".tc-row-list .tc-cell-ext").length).toBe(0);
    expect(columnsVar(container)).toBe("minmax(0, 1fr) 10.7em 9.8em 5.3em 4.8em");
  });

  it("forgets the set on Reset rather than storing the defaults", async () => {
    seedColumns({
      shown: ["name", "ext", "size", "attr"],
      widthsEm: { ext: 4.3, size: 10.7, date: 9.8, attr: 5.3 },
    });
    const { container } = await renderScreen();
    expect(headerRows(container)[0].querySelector(".tc-cell-date")).toBeNull();

    fireEvent.contextMenu(headerRows(container)[0]);
    await userEvent.click(screen.getByRole("menuitem", { name: "Reset columns" }));

    expect(storedColumns()).toBeUndefined();
    expect(headerRows(container)[0].querySelector(".tc-cell-date")).not.toBeNull();
    expect(columnsVar(container)).toBe("");
  });

  it("resizes a column by its grip, live, and writes it once, in em", async () => {
    const { container } = await renderScreen();
    const grip = within(headerRows(container)[0]).getByRole("separator", { name: /Ext/ });

    fireEvent.pointerDown(grip, { clientX: 100, pointerId: 1 });
    fireEvent.pointerMove(grip, { clientX: 130, pointerId: 1 });
    fireEvent.pointerMove(grip, { clientX: 160, pointerId: 1 });
    // Drawn as it moves — 4.3 em plus 60 px of 12 px text — and not yet stored.
    expect(columnsVar(container)).toBe("minmax(0, 1fr) 9.3em 10.7em 9.8em 5.3em 4.8em");
    expect(storedColumns()).toBeUndefined();

    fireEvent.pointerUp(grip, { clientX: 160, pointerId: 1 });
    expect((storedColumns() as { widthsEm: Record<string, number> }).widthsEm.ext).toBe(9.3);
  });

  it("narrows by hiding, and never writes the narrow set back", async () => {
    const chosen = {
      shown: ["name", "ext", "size", "date", "attr"],
      widthsEm: { ext: 4.3, size: 10.7, date: 14, attr: 5.3 },
    };
    seedColumns(chosen);
    // 200 px of 12 px text is 16.7 em, well under the 29.6 em threshold.
    const width = vi.spyOn(HTMLElement.prototype, "offsetWidth", "get").mockReturnValue(200);
    try {
      const { container } = await renderScreen();
      await waitFor(() =>
        expect(headerRows(container)[0].querySelector(".tc-cell-date")).toBeNull()
      );
      expect(headerRows(container)[0].querySelector(".tc-cell-attr")).toBeNull();
      expect(columnsVar(container)).toBe("minmax(0, 1fr) 4.3em 10.7em 4.8em");
      expect(storedColumns()).toEqual(chosen);
    } finally {
      width.mockRestore();
    }
  });

  it("opens from the keyboard and gives the focus back to the header", async () => {
    const { container } = await renderScreen();
    const header = headerRows(container)[0];
    header.focus();
    await userEvent.keyboard("{Shift>}{F10}{/Shift}");
    expect(screen.getByRole("menu", { name: "Columns" })).toBeTruthy();

    await userEvent.keyboard("{Escape}");
    expect(screen.queryByRole("menu", { name: "Columns" })).toBeNull();
    expect(document.activeElement).toBe(header);
  });
});
