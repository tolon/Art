// @vitest-environment jsdom
//
// The bar under every tab of the install lane (four-tab design § 2): the
// destination, and one button that only navigates in this round. Two rules
// it must keep: it never writes a setting (it reads the same remembered
// destination the files tab writes), and its button is absent on the build
// tab, where round 4 puts the real Derle — one label, one effect.

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { MemoryRouter, Route, Routes, useLocation } from "react-router-dom";
import i18n from "i18next";

import "@/i18n";

vi.mock("@/lib/settings", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/lib/settings")>()),
  saveSettings: vi.fn().mockResolvedValue(undefined),
  getSettings: vi.fn(),
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

afterEach(() => {
  cleanup();
  useSettingsStore.setState({ loaded: false, settings: DEFAULT_SETTINGS });
});

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

  it("is not drawn for the other lanes", () => {
    seed({ "buildSession.kind": "boot-card" });
    renderAt("/os-builder/kart");
    expect(screen.queryByTestId("build-bar")).toBeNull();
  });

  it("never writes a setting by rendering", () => {
    renderAt("/os-builder/dosyalar");
    const bag = useSettingsStore.getState().settings.remembered as Record<string, unknown>;
    expect(Object.keys(bag).filter((k) => k.startsWith("osinstall.destination"))).toEqual([]);
  });
});
