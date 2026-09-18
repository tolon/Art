// @vitest-environment jsdom
//
// ART-117, decision 11: a plan that edits the card's RDB cannot run until a
// backup path is chosen, and a replace says both versions before anything
// runs. Mocked at the `@/lib/*` boundary, the house pattern
// (`CardBuilder.test.tsx`). Sentences are literal, not fetched with `t()`, so
// a deleted key cannot pass by rendering its own name.

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { changeLanguage } from "@/i18n";
import type { CardReport } from "@/lib/card";
import type { ParsedPartition } from "@/lib/hdf";
import { subscribeSafely } from "@/lib/jobs";
import { onPreloadResult, type PreloadPlan, type PreloadResult } from "@/lib/preload";
import type { VolumeNameVerdict } from "@/lib/cardOs";
import { useSettingsStore } from "@/stores/settingsStore";

const planMock = vi.hoisted(() => vi.fn());
const cardOpenMock = vi.hoisted(() => vi.fn());
const dialogSaveMock = vi.hoisted(() => vi.fn());
const checkVolumeNameMock = vi.hoisted(() => vi.fn());

vi.mock("@/lib/preload", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/lib/preload")>()),
  preloadPlan: planMock,
  preloadRun: vi.fn(),
  preloadProbe: vi.fn(),
  onPreloadResult: vi.fn(() => Promise.resolve(() => {})),
}));
// Q7: the core's own name rule, never restated on this side — mocked so the
// panel's own wiring to `card_os_check_volume_name` is what is under test,
// not the rule itself (`preload.test.ts` owns the mapping).
vi.mock("@/lib/cardOs", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/lib/cardOs")>()),
  cardOsCheckVolumeName: checkVolumeNameMock,
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
        mbr_slot: 2,
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
  checkVolumeNameMock.mockResolvedValue({ ok: true } satisfies VolumeNameVerdict);
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

  // Final review I3: a run that stops after its RDB edit — a later format
  // failed — still says the card's RDB was changed, with the driver, the
  // versions and the backup, and does not say "Done".
  it("says the RDB change stays when the run stops after it", async () => {
    let deliver: ((result: PreloadResult) => void) | null = null;
    vi.mocked(subscribeSafely).mockImplementationOnce((start) => {
      void start();
      return () => {};
    });
    vi.mocked(onPreloadResult).mockImplementationOnce((handler) => {
      deliver = handler;
      return Promise.resolve(() => {});
    });
    render(<VolumePreload />);
    await waitFor(() => expect(deliver).not.toBeNull());

    act(() =>
      deliver!({
        job_id: 1,
        image: "E:\\cards\\caffeine.img",
        outcome: {
          formatted: [],
          copied: { files: 0, directories: 0, bytes: 0, comments_lost: 0, dates_lost: 0 },
          tool: null,
          embedded: {
            slot: 2,
            dostype: "PDS3",
            card_version: { version: 19, revision: 2 },
            file_version: { version: 19, revision: 3 },
            first_block: 132,
            last_block: 260,
            rdb_blocks_hi_raised: null,
            backup: BACKUP,
          },
        },
        steps: [],
        stopped: {
          code: "ART-FORMAT-MALFORMED",
          message: "malformed hst-imager: the tool said no",
        },
      })
    );

    expect(await screen.findByText("Stopped before it finished")).toBeTruthy();
    expect(
      screen.getByText(
        `The run stopped before it finished, but it had already replaced PDS3 19.2 with 19.3 in RDB blocks 132–260, and that change stays on the card. The RDB backup is at ${BACKUP}.`
      )
    ).toBeTruthy();
    expect(screen.getByText(/the tool said no/)).toBeTruthy();
    expect(screen.queryByText("Done")).toBeNull();
  });
});

// Q7: `preloadBlocker`'s two restated name rules now read
// `card_os_check_volume_name`'s own verdict — this is the panel's own wiring
// to that command, not the rule itself (`preload.test.ts` owns the mapping).
describe("the volume preload screen and the core's own name rule (Q7)", () => {
  it("blocks Run on a name the core's own verdict rejects, and clears once it is fixed", async () => {
    const user = userEvent.setup();
    planMock.mockResolvedValue({
      image: "E:\\cards\\caffeine.img",
      steps: [{ step: "format-partition", slot: 2, index: 1, drive_name: "SDH0", volume_name: "SDH0" }],
      notes: [],
      rdb_backup: null,
    } satisfies PreloadPlan);
    checkVolumeNameMock.mockImplementation((name: string) =>
      Promise.resolve(
        name.includes(":")
          ? ({ ok: false, why: "reserved-character", maxChars: 30 } satisfies VolumeNameVerdict)
          : ({ ok: true } satisfies VolumeNameVerdict)
      )
    );
    render(<VolumePreload />);

    await previewWithSdh0Chosen(user);
    const confirm = () => {
      const checkboxes = screen.getAllByRole("checkbox");
      return user.click(checkboxes[checkboxes.length - 1]);
    };
    await confirm();
    expect(runButton().disabled).toBe(false);

    // Editing the name is a new request — the plan and the confirmation go
    // with it (unrelated to this task; `VolumePreload`'s own existing rule).
    // Preview again to get the button back, so the block that follows is
    // isolated to the name, not to a stale or missing plan.
    const nameField = screen.getByDisplayValue("SDH0");
    await user.clear(nameField);
    await user.type(nameField, "Games:");
    await waitFor(() => expect(checkVolumeNameMock).toHaveBeenCalledWith("Games:"));
    await waitFor(() => expect(screen.queryByRole("button", { name: "Format and fill" })).toBeNull());
    await user.click(screen.getByRole("button", { name: "Preview" }));
    await waitFor(() => expect(runButton()).toBeTruthy());
    await confirm();

    await waitFor(() => expect(runButton().disabled).toBe(true));
    expect(
      screen.getByText(
        "SDH0's volume name contains ':' or '/', which separate paths on the Amiga and cannot appear in a name."
      )
    ).toBeTruthy();

    await user.clear(nameField);
    await user.type(nameField, "Games");
    await waitFor(() => expect(checkVolumeNameMock).toHaveBeenCalledWith("Games"));
    await waitFor(() => expect(screen.queryByRole("button", { name: "Format and fill" })).toBeNull());
    await user.click(screen.getByRole("button", { name: "Preview" }));
    await waitFor(() => expect(runButton()).toBeTruthy());
    await confirm();
    expect(runButton().disabled).toBe(false);
  });

  /**
   * **The question is asked once, and the answer is kept** (round 4 fix-wave
   * re-review, N1).
   *
   * The names in flight were held in `useState` **and listed in the checking
   * effect's own dependencies**, so adding a name re-ran the effect and the
   * re-run's cleanup cancelled the answer already on its way. The answer then
   * arrived, was discarded for "not current", and removed the name from the
   * in-flight list — which re-ran the effect once more, found the name in
   * neither map, and asked again. In the running app that is
   * `card_os_check_volume_name` at IPC rate for as long as a name is chosen,
   * `nameVerdicts` never receiving an entry (so a name the core refuses never
   * blocks Run — the regression the in-flight list was added to fix), and a
   * flickering blocker.
   *
   * **Why the existing case above cannot see it**: its mock resolves as a
   * microtask, which beats the re-render React has already scheduled, so the
   * cleanup never runs before the answer lands. The round trip here resolves
   * on a **macrotask**, which is the only thing an `invoke` can do, and the
   * two assertions are the two halves of the loop: how many times one
   * unchanged name was asked about, and whether its verdict was stored.
   */
  it("asks the core once for one unchanged name, and keeps the verdict that lands late", async () => {
    const user = userEvent.setup();
    let answer: ((verdict: VolumeNameVerdict) => void) | null = null;
    checkVolumeNameMock.mockImplementation(
      () =>
        new Promise<VolumeNameVerdict>((resolve) => {
          answer = (verdict) => setTimeout(() => resolve(verdict), 0);
        })
    );
    render(<VolumePreload />);

    await waitFor(() => expect(screen.getByText("SDH0")).toBeTruthy());
    await user.click(screen.getAllByRole("checkbox")[0]);

    // Asked, and said so — with nothing but the asking having changed since.
    await waitFor(() => expect(checkVolumeNameMock).toHaveBeenCalledWith("SDH0"));
    await act(async () => {
      await Promise.resolve();
    });
    expect(
      screen.getByText("ART is checking whether SDH0's volume name is one AmigaDOS accepts.")
    ).toBeTruthy();

    // The answer lands a macrotask later, after React has re-rendered.
    await act(async () => {
      answer?.({ ok: false, why: "too-long", maxChars: 30 });
      await new Promise((resolve) => setTimeout(resolve, 0));
    });

    expect(
      screen.getByText(
        "SDH0's volume name is longer than 30 characters, which is where AmigaDOS names stop."
      )
    ).toBeTruthy();
    expect(checkVolumeNameMock.mock.calls.filter(([name]) => name === "SDH0")).toHaveLength(1);
  });
});
