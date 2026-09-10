// @vitest-environment jsdom
//
// The bar under every tab of the install lane (four-tab design § 2): the
// destination, what the person has chosen so far, and one button that only
// navigates in this round. Three rules it must keep: it never writes a
// setting (it reads the same remembered destination the files tab writes),
// its button is absent on the build tab, where round 4 puts the real Derle —
// one label, one effect — and **it asks Rust nothing at all**.
//
// That last rule is round 4 task 5's. Until then the bar computed tab 4's own
// first two summary lines through `useBuildSummary`, which meant a second
// plan, a second chain and a second slot report on every one of tabs 1-3,
// beside the tab's own. The line says what the *session* carries now — the
// ticked update count and the first-boot tick — so every wrapper below is
// mocked to prove it is never reached, not to answer.

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

// Every wrapper the bar reached through `useBuildSummary` before round 4 task
// 5, still mocked — **as an assertion, not as an answer.** A mock that is
// never called is how "the bar asks nothing" is measured; leaving them
// unmocked would only prove that jsdom has no IPC bridge.
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
}));

vi.mock("@/lib/firstboot", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/lib/firstboot")>()),
  firstbootPreview: firstbootPreviewMock,
}));

const { useSettingsStore } = await import("@/stores/settingsStore");
const { DEFAULT_SETTINGS } = await import("@/lib/settings");
const { BuildBar } = await import("@/pages/osbuilder/BuildBar");

/** Every wrapper that must stay untouched, by name, so a failure says which
 *  one the bar started asking. */
const NEVER_ASKED = {
  osinstallPlan: planMock,
  osinstallChain: chainMock,
  osinstallSlots: slotsMock,
  osinstallComponents: componentsMock,
  layersFor: layersForMock,
  osinstallCollisions: collisionsMock,
  osinstallComponentCollisions: componentCollisionsMock,
  osinstallDescribeTree: describeTreeMock,
  osinstallDestinationTaken: destinationTakenMock,
  osinstallScanMedia: scanMediaMock,
  firstbootPreview: firstbootPreviewMock,
};

function expectNothingAsked() {
  for (const [name, mock] of Object.entries(NEVER_ASKED)) {
    expect({ [name]: mock.mock.calls.length }).toEqual({ [name]: 0 });
  }
}

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

beforeEach(() => {
  for (const mock of Object.values(NEVER_ASKED)) mock.mockReset();
});

afterEach(() => {
  cleanup();
  useSettingsStore.setState({ loaded: false, settings: DEFAULT_SETTINGS });
});

/** Everything the size line is computed from: one ticked update, and no
 *  first-boot answer at all — absent means ticked. */
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

  // The owner, 2026-09-10: "direkt derleye git diye buton var, bir sonraki
  // adıma gitmeli". The bar's button takes the person one tab on, in the
  // lane's own order, and names where it goes. Straight to Build from the
  // first tab skipped two tabs of choices.
  it("goes to the next tab in the lane's order, names it, and starts nothing", async () => {
    const user = userEvent.setup();
    for (const [from, to] of [
      ["dosyalar", "secim"],
      ["secim", "makine"],
      ["makine", "derle"],
    ] as const) {
      renderAt(`/os-builder/${from}`);
      const button = screen.getByTestId("build-bar-go");
      expect(button.textContent).toBe(
        i18n.t("osBuilder.bar.next", { step: i18n.t(`osBuilder.step.${to}`) })
      );
      await user.click(button);
      expect(screen.getByTestId("where").textContent).toBe(`/os-builder/${to}`);
      cleanup();
    }
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

  // Round 4, task 5.
  it("says how many updates are ticked and that first boot is on", async () => {
    seed(BUILD);
    renderAt("/os-builder/dosyalar");
    const line = await screen.findByTestId("build-bar-size");
    // The whole sentence, both parameters — a line that said "1 updates
    // ticked" and left the tick unnamed would pass a `toContain("1")`.
    expect(line.textContent).toBe(
      i18n.t("osBuilder.bar.size", {
        updates: 1,
        firstboot: i18n.t("osBuilder.bar.firstboot.on"),
      })
    );
    // Absent means ticked, and this fixture stores no `firstboot` at all.
    expect(line.textContent).toContain(i18n.t("osBuilder.bar.firstboot.on"));
  });

  it("says first boot is off once the person has unticked it", async () => {
    seed({ ...BUILD, "buildSession.firstboot": { written: false, wanted: false } });
    renderAt("/os-builder/dosyalar");
    const line = await screen.findByTestId("build-bar-size");
    expect(line.textContent).toBe(
      i18n.t("osBuilder.bar.size", {
        updates: 1,
        firstboot: i18n.t("osBuilder.bar.firstboot.off"),
      })
    );
  });

  // **The rule this file exists for since round 4 task 5.** Not "it is fast":
  // a counted zero against every wrapper the old hook reached, on the tab
  // where the bar draws the most.
  it("asks Rust nothing at all — no plan, no chain, no slots", async () => {
    seed(BUILD);
    renderAt("/os-builder/dosyalar");
    await screen.findByTestId("build-bar-size");
    // Waited on a *later* answer than the line, so this is not simply a
    // snapshot taken before anything could have been asked: the destination
    // sentence and the size line are both settled here.
    await waitFor(() => expect(screen.getByTestId("build-bar-destination")).toBeTruthy());
    expectNothingAsked();
  });

  it("does not repeat the size line on the build tab, which states it in full", async () => {
    seed(BUILD);
    renderAt("/os-builder/derle");
    expect(screen.getByTestId("build-bar")).toBeTruthy();
    expect(screen.queryByTestId("build-bar-size")).toBeNull();
    await waitFor(() => expect(screen.getByTestId("build-bar-destination")).toBeTruthy());
    expectNothingAsked();
  });

  it("never writes a setting by rendering", () => {
    renderAt("/os-builder/dosyalar");
    const bag = useSettingsStore.getState().settings.remembered as Record<string, unknown>;
    expect(Object.keys(bag).filter((k) => k.startsWith("osinstall.destination"))).toEqual([]);
  });
});
