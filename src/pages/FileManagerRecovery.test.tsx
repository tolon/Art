// @vitest-environment jsdom
//
// ART-117 scoped re-review: the File Manager's card for a journal an operation
// left behind after it finished and was verified. Undoing that journal would
// take the finished change back out, so the card offers exactly one thing —
// "Delete the journal" — and never an undo.
//
// Rendered through the real `FileManager.tsx`, opened on an ADF the way a user
// opens one (a double-click on the row), and mocked at the same boundary as
// `FileManager.test.tsx`: the `@/lib/*` wrappers. `@tauri-apps/api/core` is
// mocked as well, only so the test can show the delete action goes through
// the typed `volumeRecover` wrapper and never reaches a raw `invoke` itself.
// The unfinished-journal card is the control: it still offers the undo, which
// is what makes "no undo button" on the finished card a finding rather than a
// test that cannot see buttons.

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { MemoryRouter } from "react-router-dom";

import { changeLanguage } from "@/i18n";
import { useSettingsStore } from "@/stores/settingsStore";
import type { AdfInfo } from "@/lib/adf";
import type { LocalListing, PanelEntry } from "@/lib/panel";
import type { WriteCapability } from "@/lib/volumeWrite";

// --- the IPC surface, mocked at `@/lib/*` ------------------------------------

const listLocalMock = vi.hoisted(() => vi.fn());
const localRootsMock = vi.hoisted(() => vi.fn());
const listAdfMock = vi.hoisted(() => vi.fn());
const onDirSizeResultMock = vi.hoisted(() => vi.fn());
const onVolumeWriteResultMock = vi.hoisted(() => vi.fn());
const capabilityMock = vi.hoisted(() => vi.fn());
const recoverMock = vi.hoisted(() => vi.fn());
const onJobProgressMock = vi.hoisted(() => vi.fn());
const onArchivesPlanResultMock = vi.hoisted(() => vi.fn());
const analyzePathsMock = vi.hoisted(() => vi.fn());
const adfOpenMock = vi.hoisted(() => vi.fn());
const invokeMock = vi.hoisted(() => vi.fn());

vi.mock("@tauri-apps/api/core", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@tauri-apps/api/core")>()),
  invoke: invokeMock,
}));

vi.mock("@/lib/panel", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/lib/panel")>()),
  panelListLocal: listLocalMock,
  panelLocalRoots: localRootsMock,
  panelListAdf: listAdfMock,
  onDirSizeResult: onDirSizeResultMock,
}));

vi.mock("@/lib/volumeWrite", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/lib/volumeWrite")>()),
  onVolumeWriteResult: onVolumeWriteResultMock,
  volumeWriteCapability: capabilityMock,
  volumeRecover: recoverMock,
}));

vi.mock("@/lib/adf", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/lib/adf")>()),
  adfOpen: adfOpenMock,
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
  startDrag: vi.fn(),
}));

vi.mock("@/lib/settings", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/lib/settings")>()),
  getSettings: vi.fn(),
  saveSettings: vi.fn().mockResolvedValue(undefined),
}));

const { FileManager } = await import("@/pages/FileManager");
const { DEFAULT_SETTINGS } = await import("@/lib/settings");

// --- fixtures ----------------------------------------------------------------

const IMAGE = "C:\\Amiga\\Turrican.adf";

const LISTING: LocalListing = {
  path: "C:\\Amiga",
  parent: "C:\\",
  truncated: false,
  entries: [
    {
      name: "Turrican.adf",
      is_dir: false,
      bytes: 901_120,
      path: IMAGE,
      header_block: null,
      attrs: null,
      comment: null,
      modified: null,
    } as unknown as PanelEntry,
  ],
};

const ADF: AdfInfo = {
  volume_name: "Turrican",
  fs_type: "ffs",
  international: false,
  dir_cache: false,
  bootable: true,
  checksum_valid: true,
  capacity_bytes: 901_120,
  used_bytes: 0,
  free_bytes: 901_120,
  file_count: 0,
  directory_count: 0,
  root_block: 880,
};

function capability(journal: { pending?: string; finished?: string }): WriteCapability {
  return {
    writable: !journal.pending && !journal.finished,
    reason: null,
    strategy: "whole-file",
    free_blocks: 1758,
    free_bytes: 1758 * 512,
    block_size: 512,
    volume_name: "Turrican",
    filesystem: "DOS\\1",
    pending_recovery: journal.pending ?? null,
    finished_journal: journal.finished ?? null,
  };
}

function subscription() {
  return vi.fn().mockResolvedValue(() => {});
}

beforeEach(() => {
  listLocalMock.mockReset().mockResolvedValue(LISTING);
  localRootsMock.mockReset().mockResolvedValue(["C:\\"]);
  listAdfMock.mockReset().mockResolvedValue([]);
  onDirSizeResultMock.mockReset().mockImplementation(subscription());
  onVolumeWriteResultMock.mockReset().mockImplementation(subscription());
  onJobProgressMock.mockReset().mockImplementation(subscription());
  onArchivesPlanResultMock.mockReset().mockImplementation(subscription());
  analyzePathsMock
    .mockReset()
    .mockResolvedValue([{ plan: { detection: { category: "floppy-image" } } }]);
  adfOpenMock.mockReset().mockResolvedValue(ADF);
  // Every open after the first finds no journal: the one the test decides
  // about is gone once it has been decided about.
  capabilityMock.mockReset().mockResolvedValue(capability({}));
  recoverMock.mockReset().mockResolvedValue(null);
  invokeMock.mockReset().mockRejectedValue(new Error("no Tauri backend in jsdom"));

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

/** Open the screen, then the ADF in the left pane, with `found` beside it. */
async function openImageWith(found: WriteCapability, label: string) {
  capabilityMock.mockResolvedValueOnce(found);
  render(
    <MemoryRouter>
      <FileManager />
    </MemoryRouter>
  );
  await waitFor(() => expect(screen.getAllByText("Turrican.adf").length).toBe(2));
  fireEvent.doubleClick(screen.getAllByText("Turrican.adf")[0]);
  return screen.findByRole("alertdialog", { name: label });
}

// -----------------------------------------------------------------------------

describe("the File Manager's journal card", () => {
  it("offers a finished operation's journal for deletion only, never for undoing", async () => {
    const card = await openImageWith(
      capability({ finished: "Embed PDS3 19.3 in the RDB" }),
      "Journal left behind"
    );

    expect(
      within(card).getByText("An operation on this image finished, and its journal was left behind")
    ).toBeTruthy();
    expect(card.textContent).toContain(
      "“Embed PDS3 19.3 in the RDB” finished and was verified; only its journal file could not be " +
        "removed. Do not undo it — that would take the finished change back out. Delete the " +
        "journal to write to this image again."
    );

    const buttons = within(card).getAllByRole("button");
    expect(buttons.map((button) => button.textContent)).toEqual(["Delete the journal"]);
    expect(within(card).queryByRole("button", { name: /undo/i })).toBeNull();
    expect(within(card).queryByRole("button", { name: "Leave the image alone" })).toBeNull();

    fireEvent.click(buttons[0]);
    await waitFor(() => expect(recoverMock).toHaveBeenCalledWith(IMAGE, false));
    expect(recoverMock).not.toHaveBeenCalledWith(IMAGE, true);
    expect(invokeMock.mock.calls.some(([command]) => command === "volume_recover")).toBe(false);
  });

  it("still offers an unfinished operation's undo (the control)", async () => {
    const card = await openImageWith(
      capability({ pending: "Copy in Payload.bin" }),
      "Unfinished operation"
    );

    expect(within(card).getByText("An operation on this image did not finish")).toBeTruthy();
    expect(within(card).getAllByRole("button").map((button) => button.textContent)).toEqual([
      "Undo it",
      "Leave the image alone",
    ]);
  });
});
