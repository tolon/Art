// @vitest-environment jsdom
//
// The WinUAE studio's own install section (four tabs, round 5, task 1).
//
// Two Amiga-side operations left the OS Builder's wizard this round: running
// a package's own installer under the emulator (`AmigaInstallPanel`) and
// rehearsing first boot (`FirstBootRehearse`). Both open an emulator window,
// neither writes into the build the wizard is assembling, and the studio is
// where a person already goes to run an Amiga. What this file proves is the
// one thing the two components' own suites cannot: that the studio actually
// mounts them, under a heading that comes from the catalogue.
//
// Both are **mocked**, deliberately. Their behaviour is tested where they
// live — `AmigaInstallPanel.test.tsx` (2 449 lines) and
// `FirstBootRehearse.test.tsx` — and mounting the real ones here would test
// those files again through a second screen while proving nothing about this
// one.

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";
import { MemoryRouter } from "react-router-dom";
import i18n from "i18next";

// Side-effecting: a real, synchronously-initialised i18next instance.
import "@/i18n";
import { useSettingsStore } from "@/stores/settingsStore";

const detectMock = vi.hoisted(() => vi.fn());
const listProfilesMock = vi.hoisted(() => vi.fn());
const launchMock = vi.hoisted(() => vi.fn());
const saveSettingsMock = vi.hoisted(() => vi.fn(async () => {}));

vi.mock("@/lib/winuae", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/lib/winuae")>()),
  winuaeDetect: detectMock,
  winuaeListProfiles: listProfilesMock,
  winuaeLaunch: launchMock,
}));

// `useRemembered` writes through `useSettingsStore.update()`, which calls the
// real `tauri-plugin-store` IPC on every tick — an unhandled rejection in
// jsdom (see `FilesTab.test.tsx`'s own note).
vi.mock("@/lib/settings", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/lib/settings")>()),
  saveSettings: saveSettingsMock,
  getSettings: vi.fn(async () => ({})),
}));

vi.mock("@tauri-apps/plugin-dialog", () => ({
  open: vi.fn(),
}));

vi.mock("@/components/osbuilder/AmigaInstallPanel", () => ({
  AmigaInstallPanel: () => <div data-testid="amiga-install-panel" />,
}));

vi.mock("@/components/osbuilder/FirstBootRehearse", () => ({
  FirstBootRehearse: () => <div data-testid="firstboot-rehearse" />,
}));

const { WinuaeStudio } = await import("@/pages/WinuaeStudio");

beforeEach(() => {
  vi.clearAllMocks();
  useSettingsStore.setState((state) => ({
    settings: { ...state.settings, uxMode: "beginner", winuaePath: null, remembered: {} },
  }));
  detectMock.mockResolvedValue({
    found: false,
    executable_path: null,
    version: null,
    is_64bit: false,
  });
  listProfilesMock.mockResolvedValue([]);
});

afterEach(() => {
  cleanup();
});

function renderStudio() {
  return render(
    <MemoryRouter>
      <WinuaeStudio />
    </MemoryRouter>
  );
}

describe("the studio's Amiga-side install section", () => {
  it("mounts both the package installer and the first-boot rehearsal", async () => {
    renderStudio();
    const section = await screen.findByTestId("winuae-install-section");
    // Inside the section, not merely somewhere on the page: the two belong
    // together under one heading, and a rehearsal card drifting out from
    // under it is exactly the kind of layout change nobody notices.
    expect(section.querySelector("[data-testid='amiga-install-panel']")).toBeTruthy();
    expect(section.querySelector("[data-testid='firstboot-rehearse']")).toBeTruthy();
  });

  it("heads the section with the catalogue's own sentence, not a hard-coded one", async () => {
    renderStudio();
    const section = await screen.findByTestId("winuae-install-section");
    const heading = section.querySelector("h2");
    expect(heading?.textContent).toBe(i18n.t("winuae.installHeading"));
    expect(section.textContent).toContain(i18n.t("winuae.installIntro"));
  });
});
