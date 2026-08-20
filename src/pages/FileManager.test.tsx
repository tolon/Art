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
// (`useRomPairing.test.tsx`, `OsInstall.test.tsx`). `@/lib/settings` is
// mocked one layer further down for the reason `OsInstall.test.tsx` records:
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
import { cleanup, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { MemoryRouter } from "react-router-dom";

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
    expect(text).not.toMatch(/\bfiles\.[a-z][a-zA-Z]*\.[a-z]/);
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
