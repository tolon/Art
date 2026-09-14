// @vitest-environment jsdom
//
// ART-117, decision 11: a plan that edits the card's RDB cannot run until a
// backup path is chosen, and a replace says both versions before anything
// runs. Mocked at the `@/lib/*` boundary, the house pattern
// (`CardBuilder.test.tsx`). Sentences are literal, not fetched with `t()`, so
// a deleted key cannot pass by rendering its own name.

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { changeLanguage } from "@/i18n";
import type { CardReport } from "@/lib/card";
import type { ParsedPartition } from "@/lib/hdf";
import type { PreloadPlan } from "@/lib/preload";
import { useSettingsStore } from "@/stores/settingsStore";

const planMock = vi.hoisted(() => vi.fn());
const cardOpenMock = vi.hoisted(() => vi.fn());
const dialogSaveMock = vi.hoisted(() => vi.fn());

vi.mock("@/lib/preload", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/lib/preload")>()),
  preloadPlan: planMock,
  preloadRun: vi.fn(),
  preloadProbe: vi.fn(),
  onPreloadResult: vi.fn(() => Promise.resolve(() => {})),
}));
vi.mock("@/lib/card", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/lib/card")>()),
  cardOpen: cardOpenMock,
}));
vi.mock("@/lib/jobs", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/lib/jobs")>()),
  subscribeSafely: vi.fn(() => () => {}),
}));
vi.mock("@/lib/useRomPairing", () => ({
  useRomPairing: () => ({ checking: false, results: [] }),
}));
vi.mock("@/lib/useBuildSession", () => ({
  useBuildSession: () => ({
    session: { card: { image: "E:\\cards\\caffeine.img" } },
    setCard: vi.fn(),
  }),
}));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn(), save: dialogSaveMock }));
vi.mock("@/lib/settings", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/lib/settings")>()),
  getSettings: vi.fn(),
  saveSettings: vi.fn().mockResolvedValue(undefined),
}));

const { VolumePreload } = await import("@/components/osbuilder/VolumePreload");
const { DEFAULT_SETTINGS } = await import("@/lib/settings");

const PARTITION: ParsedPartition = {
  drive_name: "SDH0",
  dostype: 0x50445303,
  dostype_str: "PDS3",
  fs_type: "pfs3directscsi",
  low_cyl: 2,
  high_cyl: 535,
  cylinder_count: 534,
  size_bytes: 534 * 3072 * 512,
  bootable: true,
  boot_priority: 0,
  num_buffers: 600,
  block_location: 1,
  next_part_block: 2,
  checksum_valid: true,
};

const CARD: CardReport = {
  card: {
    path: "E:\\cards\\caffeine.img",
    total_bytes: 64 * 1024 * 1024 * 1024,
    mbr: { partitions: [] },
    areas: [
      {
        offset_bytes: 1_178_599_424,
        length_bytes: 60 * 1024 * 1024 * 1024,
        rdb: { partitions: [PARTITION], file_systems: [], checksum_valid: true },
      },
    ],
  },
  file_systems: [],
  unmountable: [],
};

const BACKUP = "E:\\backups\\caffeine-rdb-backup.bin";

function replacePlan(rdbBackup: string | null): PreloadPlan {
  return {
    image: "E:\\cards\\caffeine.img",
    steps: [
      {
        step: "replace-filesystem",
        slot: 2,
        driver: "E:\\drivers\\pfs3aio",
        dostype: "PDS3",
        name: "pfs3aio",
        card_version: { version: 19, revision: 2 },
        file_version: { version: 19, revision: 3 },
        blocks: [132, 260],
        rdb_blocks_hi_raised: null,
      },
      { step: "format-partition", slot: 2, index: 1, drive_name: "SDH0", volume_name: "SDH0" },
    ],
    notes: [],
    rdb_backup: rdbBackup,
  };
}

beforeEach(() => {
  useSettingsStore.setState({
    loaded: true,
    settings: { ...DEFAULT_SETTINGS, remembered: { "preload.driver": "E:\\drivers\\pfs3aio" } },
  });
  cardOpenMock.mockResolvedValue(CARD);
});

afterEach(async () => {
  cleanup();
  vi.clearAllMocks();
  useSettingsStore.setState({ loaded: false, settings: DEFAULT_SETTINGS });
  await changeLanguage("en");
});

async function previewWithSdh0Chosen(user: ReturnType<typeof userEvent.setup>) {
  await waitFor(() => expect(screen.getByText("SDH0")).toBeTruthy());
  await user.click(screen.getAllByRole("checkbox")[0]);
  await user.click(screen.getByRole("button", { name: "Preview" }));
}

function runButton(): HTMLButtonElement {
  return screen.getByRole("button", { name: "Format and fill" }) as HTMLButtonElement;
}

describe("the volume preload screen and an RDB edit", () => {
  it("names both versions and keeps Run disabled until a backup path is chosen", async () => {
    const user = userEvent.setup();
    planMock.mockResolvedValueOnce(replacePlan(null));
    render(<VolumePreload />);

    await previewWithSdh0Chosen(user);
    await waitFor(() =>
      expect(screen.getByText("Replace PDS3 19.2 on the card with 19.3 from pfs3aio")).toBeTruthy()
    );
    expect(screen.getByText("RDB blocks 132–260.")).toBeTruthy();
    expect(screen.getByText("Choose where the RDB backup goes before running.")).toBeTruthy();

    const checkboxes = screen.getAllByRole("checkbox");
    await user.click(checkboxes[checkboxes.length - 1]);
    expect(runButton().disabled).toBe(true);
    expect(
      screen.getByText("This plan changes the card's RDB — choose where its backup goes first.")
    ).toBeTruthy();

    dialogSaveMock.mockResolvedValueOnce(BACKUP);
    await user.click(
      within(screen.getByTestId("preload-rdb-backup")).getByRole("button", { name: "Browse…" })
    );
    expect(dialogSaveMock).toHaveBeenCalledWith(
      expect.objectContaining({ defaultPath: "caffeine-rdb-backup.bin" })
    );

    // A new backup path is a new request: the plan goes, and is asked again.
    planMock.mockResolvedValueOnce(replacePlan(BACKUP));
    await waitFor(() => expect(screen.queryByRole("button", { name: "Format and fill" })).toBeNull());
    await user.click(screen.getByRole("button", { name: "Preview" }));
    await waitFor(() =>
      expect(screen.getByText(`The RDB area is copied to ${BACKUP} first.`)).toBeTruthy()
    );
    expect(planMock.mock.calls[1][0].rdb_backup).toBe(BACKUP);

    const again = screen.getAllByRole("checkbox");
    await user.click(again[again.length - 1]);
    expect(runButton().disabled).toBe(false);
  });

  it("says why the card's driver stays, and asks for no backup when nothing is edited", async () => {
    const user = userEvent.setup();
    planMock.mockResolvedValueOnce({
      image: "E:\\cards\\caffeine.img",
      steps: [{ step: "format-partition", slot: 2, index: 1, drive_name: "SDH0", volume_name: "SDH0" }],
      notes: [
        {
          note: "driver-kept",
          dostype: "PDS3",
          card_version: { version: 19, revision: 2 },
          file_version: { version: 19, revision: 2 },
        },
      ],
      rdb_backup: null,
    } satisfies PreloadPlan);
    render(<VolumePreload />);

    await previewWithSdh0Chosen(user);
    await waitFor(() =>
      expect(
        screen.getByText("PDS3 19.2 on the card stays: the file you chose states 19.2, which is not newer.")
      ).toBeTruthy()
    );
    const checkboxes = screen.getAllByRole("checkbox");
    await user.click(checkboxes[checkboxes.length - 1]);
    expect(runButton().disabled).toBe(false);
  });
});
