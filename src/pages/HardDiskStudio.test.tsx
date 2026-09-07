// @vitest-environment jsdom
//
// Task 11: the card screen reads a card's first-boot report once `cardOpen`
// resolves and renders it below the partition table (`FirstBootReportPanel`,
// via `cardFirstBootReport` — never `invoke` directly). This is the smallest
// harness for that wiring, not a full page test: the wizard, the plain-HDF
// view and the rest of the screen already work and are untouched by this
// task, so they are not re-tested here.
//
// Mocked at the `@/lib/*` boundary, same as every other page test in this
// suite (`FileManager.test.tsx`'s header explains why: never mock
// `@tauri-apps/api` itself).

import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import { MemoryRouter } from "react-router-dom";

import "@/i18n";
import { resetOpenObjects } from "@/stores/openObjectStore";
import type { CardReport } from "@/lib/card";
import type { CardFirstBootReport, FirstBootReport } from "@/lib/firstboot";

const cardOpenMock = vi.hoisted(() => vi.fn());
const cardFirstBootReportMock = vi.hoisted(() => vi.fn());

vi.mock("@/lib/card", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/lib/card")>()),
  cardOpen: cardOpenMock,
}));

vi.mock("@/lib/firstboot", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/lib/firstboot")>()),
  cardFirstBootReport: cardFirstBootReportMock,
}));

vi.mock("@tauri-apps/plugin-dialog", () => ({
  open: vi.fn(),
  save: vi.fn(),
}));

const { HardDiskStudio } = await import("@/pages/HardDiskStudio");

function cardReport(): CardReport {
  return {
    card: {
      path: "E:/pistorm.img",
      total_bytes: 8_000_000_000,
      mbr: { partitions: [] },
      areas: [
        {
          offset_bytes: 0,
          length_bytes: 0,
          rdb: { partitions: [], file_systems: [], checksum_valid: true },
        },
      ],
    },
    file_systems: [],
    unmountable: [],
  };
}

function report(over: Partial<FirstBootReport> = {}): FirstBootReport {
  return {
    version: 1,
    system: { system: "PiStorm", rpi: "RPi4", kick: "3.2" },
    steps: [],
    ending: "done-all",
    fatCopyFailed: false,
    rebootRequestedBy: null,
    unknown: [],
    ...over,
  };
}

afterEach(() => {
  cleanup();
  resetOpenObjects();
  vi.clearAllMocks();
});

function renderAt(path: string) {
  return render(
    <MemoryRouter initialEntries={[{ pathname: "/harddisk", state: { path } }]}>
      <HardDiskStudio />
    </MemoryRouter>
  );
}

describe("the card screen's first-boot report", () => {
  it("reads it once the card opens and renders it below the partition table", async () => {
    cardOpenMock.mockResolvedValue(cardReport());
    const cfbr: CardFirstBootReport = { source: "amiga-volume", report: report() };
    cardFirstBootReportMock.mockResolvedValue(cfbr);

    renderAt("E:/pistorm.img");

    await waitFor(() => expect(cardFirstBootReportMock).toHaveBeenCalledWith("E:/pistorm.img"));
    expect(await screen.findByTestId("firstboot-report")).toBeTruthy();
    expect(screen.getByTestId("firstboot-report-source").textContent).toContain(
      "S/FirstBoot.log"
    );
    // The read is a second, independent question — it never disturbs the
    // partition table `cardOpen` already resolved.
    expect(screen.getByText("💳", { exact: false })).toBeTruthy();
    expect(screen.queryByTestId("firstboot-report-error")).toBeNull();
  });

  it("renders the read's own error under the panel heading, and the partition table stays", async () => {
    cardOpenMock.mockResolvedValue(cardReport());
    cardFirstBootReportMock.mockRejectedValue(new Error("could not read the card"));

    renderAt("E:/pistorm.img");

    await waitFor(() => expect(cardFirstBootReportMock).toHaveBeenCalled());
    expect(await screen.findByTestId("firstboot-report-error")).toBeTruthy();
    expect(screen.queryByTestId("firstboot-report")).toBeNull();
    // The card table this task must not disturb is still on screen.
    expect(screen.getByText("💳", { exact: false })).toBeTruthy();
  });
});
