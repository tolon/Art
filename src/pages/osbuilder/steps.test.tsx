// @vitest-environment jsdom
//
// The OS Builder's steps, as routes.
//
// What this file is for: a step navigated to **on its own** must act on what
// the session holds, or *ask* — never render an empty card and never throw.
// That is the design's fourth named mutation, and the reason the steps are
// sub-routes at all: the owner's verdict on the single scrolling column was
// "çok karmaşık gereksiz derecede uzun".
//
// The panels are replaced by markers. Each reaches Tauri on mount, and this
// file is about routing and what a step hands its panel — `OsInstall.test.tsx`
// and the two panel test files cover the real components.

import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";
import { MemoryRouter, Route, Routes } from "react-router-dom";

vi.mock("@/lib/settings", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/lib/settings")>()),
  getSettings: vi.fn(),
  saveSettings: vi.fn().mockResolvedValue(undefined),
}));

vi.mock("@/components/osbuilder/PackagePanel", () => ({
  PackagePanel: ({ treeRoot }: { treeRoot: string | null }) => (
    <div data-testid="packages">{treeRoot ?? "(no tree)"}</div>
  ),
}));
vi.mock("@/components/osbuilder/AmigaInstallPanel", () => ({
  AmigaInstallPanel: ({ treeRoot }: { treeRoot: string | null }) => (
    <div data-testid="amiga">{treeRoot ?? "(no tree)"}</div>
  ),
}));
vi.mock("@/components/osbuilder/CardBuilder", () => ({
  CardBuilder: () => <div data-testid="card" />,
}));
vi.mock("@/components/osbuilder/VolumePreload", () => ({
  VolumePreload: () => <div data-testid="volumes" />,
}));
vi.mock("@/components/osbuilder/OsInstall", () => ({
  OsInstall: ({ droppedMedia }: { droppedMedia?: { path: string } | null }) => (
    <div data-testid="install">{droppedMedia?.path ?? "(no drop)"}</div>
  ),
}));

const { useSettingsStore } = await import("@/stores/settingsStore");
const { DEFAULT_SETTINGS } = await import("@/lib/settings");
const { OsBuilder } = await import("@/pages/OsBuilder");
const { StepPaketler, StepAmigaKurulum, StepKaynak, StepKart } = await import(
  "@/pages/osbuilder/steps"
);

function seed(remembered: Record<string, unknown>) {
  useSettingsStore.setState({
    loaded: true,
    settings: { ...DEFAULT_SETTINGS, remembered },
  });
}

function renderAt(path: string, state?: unknown) {
  return render(
    <MemoryRouter initialEntries={[{ pathname: path, state }]}>
      <Routes>
        <Route path="/os-builder" element={<OsBuilder />}>
          <Route path="kaynak" element={<StepKaynak />} />
          <Route path="paketler" element={<StepPaketler />} />
          <Route path="amiga-kurulum" element={<StepAmigaKurulum />} />
          <Route path="kart" element={<StepKart />} />
        </Route>
      </Routes>
    </MemoryRouter>
  );
}

afterEach(() => {
  cleanup();
  useSettingsStore.setState({ loaded: false, settings: DEFAULT_SETTINGS });
});

describe("a step opened on its own", () => {
  it("acts on the session's tree when there is one", () => {
    seed({
      "buildSession.kind": "install",
      "buildSession.tree": { root: "E:\\dist", builtHere: true },
    });
    renderAt("/os-builder/paketler");
    expect(screen.getByTestId("packages").textContent).toBe("E:\\dist");
  });

  it("asks rather than rendering empty when there is no tree", () => {
    // The design's fourth mutation: a step navigated to cold must *ask*,
    // never throw and never render a blank card.
    seed({ "buildSession.kind": "install" });
    renderAt("/os-builder/paketler");

    expect(screen.getByTestId("packages").textContent).toBe("(no tree)");
    // A rendered sentence, not the raw key — asserting on the key would pass
    // on the very failure this catches, a missing catalogue entry.
    expect(screen.getByText(/AmigaOS folder/i)).toBeTruthy();
    expect(screen.queryByText(/osBuilder\.step\./)).toBeNull();
  });

  it("asks on the Amiga-side step too, and does not gate it", () => {
    // Optional stays optional: asking is a state, not a refusal. The panel is
    // still mounted and still usable.
    seed({ "buildSession.kind": "install" });
    renderAt("/os-builder/amiga-kurulum");

    expect(screen.getByText(/AmigaOS folder/i)).toBeTruthy();
    expect(screen.getByTestId("amiga")).toBeTruthy();
  });

  it("does not ask on a step that never reads a tree", () => {
    seed({ "buildSession.kind": "boot-card" });
    renderAt("/os-builder/kart");

    expect(screen.getByTestId("card")).toBeTruthy();
    expect(screen.queryByText(/AmigaOS folder/i)).toBeNull();
  });
});

describe("the progress strip", () => {
  it("shows the steps this kind has, and not the others", () => {
    seed({
      "buildSession.kind": "install",
      "buildSession.tree": { root: "E:\\dist", builtHere: true },
    });
    renderAt("/os-builder/paketler");

    // `install` has four steps and no card step.
    expect(screen.getAllByRole("link").length).toBe(4);
    expect(screen.queryByTestId("card")).toBeNull();
    expect(screen.getByRole("link", { name: /Update packages/i })).toBeTruthy();
    expect(screen.queryByRole("link", { name: /card image/i })).toBeNull();
  });

  it("shows the card job's own steps instead", () => {
    seed({ "buildSession.kind": "boot-card" });
    renderAt("/os-builder/kart");

    expect(screen.getAllByRole("link").length).toBe(2);
    expect(screen.getByRole("link", { name: /card image/i })).toBeTruthy();
  });
});

describe("a disc dropped on the panel", () => {
  it("reaches the media step rather than stopping at the shell", () => {
    // The drop workflow routes to `/os-builder` carrying the file in router
    // state. Under sub-routes the shell has to carry it on to the step that
    // acts on it, or the whole "disc dropped on the panel offers the OS
    // Builder" path does nothing visible.
    seed({ "buildSession.kind": "boot-card" });
    renderAt("/os-builder", { path: "E:\\amiga\\iso\\AmigaOS39.iso" });

    expect(screen.getByTestId("install").textContent).toBe("E:\\amiga\\iso\\AmigaOS39.iso");
  });
});
