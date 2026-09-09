// @vitest-environment jsdom
//
// The bar under every tab of the install lane (four-tab design § 2): the
// destination, and one button that only navigates in this round. Two rules
// it must keep: it never writes a setting (it reads the same remembered
// destination the files tab writes), and its button is absent on the build
// tab, where round 4 puts the real Derle — one label, one effect.

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { MemoryRouter, Route, Routes, useLocation } from "react-router-dom";
import i18n from "i18next";

import "@/i18n";

vi.mock("@/lib/settings", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/lib/settings")>()),
  saveSettings: vi.fn().mockResolvedValue(undefined),
  getSettings: vi.fn(),
}));

// Round 4, task 4: the bar states the size of the build, through the very
// hook tab 4 states it with. So the wrappers behind that hook answer here
// too — mocked at the `@/lib/*` boundary, as everywhere else in this suite.
const componentsMock = vi.hoisted(() => vi.fn());
const layersForMock = vi.hoisted(() => vi.fn());
const planMock = vi.hoisted(() => vi.fn());
const componentCollisionsMock = vi.hoisted(() => vi.fn());
const collisionsMock = vi.hoisted(() => vi.fn());
const chainMock = vi.hoisted(() => vi.fn());
const slotsMock = vi.hoisted(() => vi.fn());
const describeTreeMock = vi.hoisted(() => vi.fn());
const destinationTakenMock = vi.hoisted(() => vi.fn());
const scanMediaMock = vi.hoisted(() => vi.fn());
const mediaEvidenceMock = vi.hoisted(() => vi.fn());
const releaseForMediaMock = vi.hoisted(() => vi.fn());
const identifyRomMock = vi.hoisted(() => vi.fn());
const firstbootPreviewMock = vi.hoisted(() => vi.fn());

vi.mock("@/lib/osinstall", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/lib/osinstall")>()),
  osinstallComponents: componentsMock,
  layersFor: layersForMock,
  osinstallPlan: planMock,
  osinstallComponentCollisions: componentCollisionsMock,
  osinstallCollisions: collisionsMock,
  osinstallChain: chainMock,
  osinstallSlots: slotsMock,
  osinstallDescribeTree: describeTreeMock,
  osinstallDestinationTaken: destinationTakenMock,
  osinstallScanMedia: scanMediaMock,
  osinstallMediaEvidence: mediaEvidenceMock,
  osinstallReleaseForMedia: releaseForMediaMock,
}));

vi.mock("@/lib/firstboot", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/lib/firstboot")>()),
  firstbootPreview: firstbootPreviewMock,
}));

vi.mock("@/lib/pistorm", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/lib/pistorm")>()),
  pistormIdentifyRom: identifyRomMock,
}));

const { useSettingsStore } = await import("@/stores/settingsStore");
const { DEFAULT_SETTINGS } = await import("@/lib/settings");
const { BuildBar } = await import("@/pages/osbuilder/BuildBar");

function seed(remembered: Record<string, unknown>) {
  useSettingsStore.setState({ loaded: true, settings: { ...DEFAULT_SETTINGS, remembered } });
}

function WhereAmI() {
  const location = useLocation();
  return <div data-testid="where">{location.pathname}</div>;
}

function renderAt(path: string) {
  return render(
    <MemoryRouter initialEntries={[path]}>
      <Routes>
        <Route
          path="/os-builder/*"
          element={
            <>
              <BuildBar />
              <WhereAmI />
            </>
          }
        />
      </Routes>
    </MemoryRouter>
  );
}

const MEDIA = "E:\\amiga\\os39";

/** A plan that places 1980 files, and a chain with one ticked update — the
 *  two facts the bar's own line states. */
beforeEach(() => {
  componentsMock.mockReset().mockResolvedValue([
    {
      id: "workbench-base",
      media: "AmigaOS3.9",
      labelKey: null,
      required: true,
      available: true,
      conditionMajor: null,
      requiresRomMajor: null,
      exclusiveGroup: null,
      overrides: [],
    },
  ]);
  layersForMock.mockReset().mockResolvedValue([]);
  planMock.mockReset().mockImplementation((req: { release: string }) =>
    Promise.resolve({
      outcome: "planned",
      plan: {
        release: req.release,
        items: [
          {
            component: "workbench-base",
            media: "AmigaOS3.9",
            from: "AmigaOS3.9:C/Format",
            to: "C/Format",
            isDir: false,
            decompress: false,
            bytes: 2048,
            mergeIcon: false,
          },
        ],
        refusals: [],
        totalBytes: 18_300_000,
        totalFiles: 1980,
        componentsOn: ["workbench-base"],
        mediaPaths: {},
        packages: [],
        packageMedia: {},
        userStartup: [],
        activations: [],
        mediaStamps: {},
        removals: [],
        layers: [],
      },
    })
  );
  componentCollisionsMock.mockReset().mockResolvedValue({ reports: [], placed: 0, contested: 0 });
  collisionsMock.mockReset().mockResolvedValue([]);
  chainMock.mockReset().mockResolvedValue({
    rows: [
      {
        position: 2,
        packageId: "boingbag-39-1",
        slotId: "package:boingbag-39-1",
        name: "BoingBag 3.9-1",
        sentenceFacts: { file: null, runsOnAmiga: false },
        state: { state: "ready" },
      },
    ],
    summary: { release: "AmigaOS 3.9", total: 1, installed: 0, notNeeded: 0 },
    unreadableFolders: [],
    crowdedFolders: [],
  });
  slotsMock.mockReset().mockResolvedValue({
    states: [
      {
        slot: {
          id: "package:boingbag-39-1",
          kind: "package",
          name: "package:boingbag-39-1",
          identity: "package:boingbag-39-1",
          artefact: null,
          required: false,
          filenames: [],
          provenance: null,
          position: 2,
          requires: [],
          supersededBy: [],
          expectsDirectories: [],
        },
        found: {
          path: `${MEDIA}\\BoingBag39-1.lha`,
          matchedBy: "hash",
          row: null,
          confirmed: null,
          bytesRead: { state: "read-no-row" },
        },
        candidates: [],
        installed: { state: "no" },
        chosenMissing: null,
        blockedBy: [],
        incomplete: null,
      },
    ],
    summary: {
      release: "AmigaOS 3.9",
      requiredTotal: 0,
      requiredFound: 0,
      optionalTotal: 1,
      optionalFound: 1,
    },
    unreadableFolders: [],
    crowdedFolders: [],
  });
  describeTreeMock.mockReset().mockResolvedValue({
    isTree: false,
    release: null,
    files: 0,
    components: [],
    amigaInstalled: [],
    problem: "holds no distribution.json",
  });
  destinationTakenMock.mockReset().mockResolvedValue(false);
  scanMediaMock.mockReset().mockResolvedValue({ outcome: "found", media: [] });
  mediaEvidenceMock.mockReset().mockResolvedValue(null);
  releaseForMediaMock.mockReset().mockResolvedValue(null);
  identifyRomMock.mockReset().mockResolvedValue(null);
  firstbootPreviewMock.mockReset().mockRejectedValue(new Error("no tree"));
});

afterEach(() => {
  cleanup();
  useSettingsStore.setState({ loaded: false, settings: DEFAULT_SETTINGS });
});

/** Everything the size line is computed from. */
const BUILD: Record<string, unknown> = {
  "buildSession.kind": "install",
  "buildSession.release": "AmigaOS 3.9",
  "buildSession.material.AmigaOS 3.9": { folders: [{ path: MEDIA, layer: null }] },
  "buildSession.packages.AmigaOS 3.9": { folder: null, chosen: ["boingbag-39-1"] },
  "osinstall.destination.AmigaOS 3.9": "E:\\amiga\\Amigatolon\\sonuclar",
};

describe("the build bar", () => {
  beforeEach(() => seed({ "buildSession.kind": "install", "buildSession.release": "AmigaOS 3.9" }));

  it("shows the remembered destination for the release, and says so when there is none", () => {
    seed({
      "buildSession.kind": "install",
      "buildSession.release": "AmigaOS 3.9",
      "osinstall.destination.AmigaOS 3.9": "E:\\amiga\\Amigatolon\\sonuclar",
    });
    renderAt("/os-builder/dosyalar");
    expect(screen.getByTestId("build-bar-destination").textContent).toContain("E:\\amiga\\Amigatolon\\sonuclar");

    cleanup();
    seed({ "buildSession.kind": "install", "buildSession.release": "AmigaOS 3.9" });
    renderAt("/os-builder/dosyalar");
    expect(screen.getByTestId("build-bar-destination").textContent).toBe(i18n.t("osBuilder.bar.noDestination"));
  });

  it("goes to the build tab and starts nothing", async () => {
    renderAt("/os-builder/secim");
    await userEvent.setup().click(screen.getByTestId("build-bar-go"));
    expect(screen.getByTestId("where").textContent).toBe("/os-builder/derle");
  });

  it("has no button on the build tab itself — one label, one effect", () => {
    renderAt("/os-builder/derle");
    expect(screen.getByTestId("build-bar")).toBeTruthy();
    expect(screen.queryByTestId("build-bar-go")).toBeNull();
  });

  it("is not drawn on the entry chip, which is not a tab of the lane", () => {
    renderAt("/os-builder/hedef");
    expect(screen.queryByTestId("build-bar")).toBeNull();
  });

  it("is not drawn for the other lanes", () => {
    seed({ "buildSession.kind": "boot-card" });
    renderAt("/os-builder/kart");
    expect(screen.queryByTestId("build-bar")).toBeNull();
  });

  // Round 4, task 4.
  it("says how big the build is and which updates it carries", async () => {
    seed(BUILD);
    renderAt("/os-builder/dosyalar");
    const line = await screen.findByTestId("build-bar-size");
    await waitFor(() => expect(line.textContent).toContain("1980"));
    // The same two sentences tab 4 draws — the plan's totals and the chain's
    // own order — and not the first-boot tick or the replace preview, which
    // are tab 4's alone.
    await waitFor(() => expect(line.textContent).toContain("BoingBag 3.9-1"));
    expect(line.textContent).not.toContain(i18n.t("osBuilder.build.summary.firstboot"));
    expect(line.textContent).not.toContain(i18n.t("osBuilder.build.summary.replacesPending"));
  });

  it("does not repeat it on the build tab, which states it in full", async () => {
    seed(BUILD);
    renderAt("/os-builder/derle");
    expect(screen.getByTestId("build-bar")).toBeTruthy();
    expect(screen.queryByTestId("build-bar-size")).toBeNull();
    // …and nothing is asked for a line that is not drawn.
    await waitFor(() => expect(screen.getByTestId("build-bar-destination")).toBeTruthy());
    expect(planMock).not.toHaveBeenCalled();
    expect(chainMock).not.toHaveBeenCalled();
  });

  it("never writes a setting by rendering", () => {
    renderAt("/os-builder/dosyalar");
    const bag = useSettingsStore.getState().settings.remembered as Record<string, unknown>;
    expect(Object.keys(bag).filter((k) => k.startsWith("osinstall.destination"))).toEqual([]);
  });
});
