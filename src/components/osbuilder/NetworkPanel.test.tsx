// @vitest-environment jsdom
//
// The panel that asks for a card's network while the card is being set up
// (SD-3 G14) — the owner's decision, 2026-08-24: *"ART sorsun, kart kurarken
// WiFi bilgilerini girelim."*
//
// Mocked at the `@/lib/*` boundary, the house pattern. `@/lib/settings` is
// mocked one layer further down for the reason `OsInstall.test.tsx` records:
// `useRemembered` writes through `useSettingsStore` and the real
// `saveSettings` rejects in jsdom with nothing to catch it.
//
// What this establishes, and the first is the one that matters:
//
//   1. **The passphrase is never remembered.** It is not in the settings bag
//      after a write, and the field is empty again afterwards. ART's own
//      standing rule is that every choice comes back the way the user left it;
//      this is the single deliberate exception, and it is the kind of
//      exception that has to be proven rather than described.
//   2. What a write replaces is said **before** the button, not after.
//   3. The three sentences a refusal can be, and that the button is disabled
//      with the reason on screen rather than in a tooltip alone.

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import "@/i18n";
import { useSettingsStore } from "@/stores/settingsStore";

const seedMock = vi.hoisted(() => vi.fn());
const alreadyThereMock = vi.hoisted(() => vi.fn());

vi.mock("@/lib/amiganet", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/lib/amiganet")>()),
  seedNetwork: seedMock,
  networksAlreadyThere: alreadyThereMock,
}));

vi.mock("@/lib/settings", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/lib/settings")>()),
  getSettings: vi.fn(),
  saveSettings: vi.fn().mockResolvedValue(undefined),
}));

const { NetworkPanel } = await import("@/components/osbuilder/NetworkPanel");
const { DEFAULT_SETTINGS } = await import("@/lib/settings");

const TREE = "E:\\amiga\\dist-3.2";

function seedStore(remembered: Record<string, unknown> = {}) {
  useSettingsStore.setState({
    loaded: true,
    settings: {
      ...DEFAULT_SETTINGS,
      remembered: {
        "buildSession.tree": { root: TREE, builtHere: true },
        ...remembered,
      },
    },
  });
}

beforeEach(() => {
  useSettingsStore.setState({ loaded: false, settings: DEFAULT_SETTINGS });
  seedMock.mockReset().mockResolvedValue({
    written: ["ENVARC:Sys/Wireless.prefs"],
    replacedNetworks: null,
    tolunnetMerged: false,
    networks: 1,
  });
  alreadyThereMock.mockReset().mockResolvedValue(null);
});

afterEach(() => {
  cleanup();
  useSettingsStore.setState({ loaded: false, settings: DEFAULT_SETTINGS });
});

/** Tick WiFi and fill it in. */
async function fillWifi(ssid = "Tolun-Ev", psk = "correct-horse") {
  await userEvent.click(screen.getByRole("checkbox", { name: /set up wifi/i }));
  await userEvent.type(screen.getByRole("textbox", { name: /network name/i }), ssid);
  const field = screen.getByLabelText(/^passphrase$/i);
  await userEvent.type(field, psk);
  return field as HTMLInputElement;
}

describe("the passphrase is never remembered", () => {
  /// **ART's one deliberate exception to its own rule**, and the kind that has
  /// to be proven. A WiFi password kept in ART's settings file is a WiFi
  /// password in a file nobody thinks of as a secret.
  it("does not reach the settings bag, before or after a write", async () => {
    seedStore();
    render(<NetworkPanel />);
    await fillWifi();

    const bagBefore = JSON.stringify(useSettingsStore.getState().settings.remembered);
    expect(bagBefore).not.toContain("correct-horse");

    await userEvent.click(screen.getByRole("button", { name: /write it to the volume/i }));
    await waitFor(() => expect(seedMock).toHaveBeenCalled());

    const bagAfter = JSON.stringify(useSettingsStore.getState().settings.remembered);
    expect(bagAfter).not.toContain("correct-horse");
    // The SSID is remembered — it is not a secret, and retyping it every time
    // would be the settings-reset this project forbids.
    expect(bagAfter).toContain("Tolun-Ev");
  });

  it("clears the field after a write, so it is typed again next time", async () => {
    seedStore();
    render(<NetworkPanel />);
    const field = await fillWifi();
    expect(field.value).toBe("correct-horse");

    await userEvent.click(screen.getByRole("button", { name: /write it to the volume/i }));
    await waitFor(() => expect(field.value).toBe(""));
  });

  /// It is masked, and the screen says why it will have to be typed again —
  /// a field that silently forgets is a field somebody thinks is broken.
  it("masks it and says it is not kept", async () => {
    seedStore();
    render(<NetworkPanel />);
    await fillWifi();
    expect(screen.getByLabelText(/^passphrase$/i).getAttribute("type")).toBe("password");
    expect(document.body.textContent).toContain("keeps no copy");

    await userEvent.click(screen.getByRole("button", { name: /^show$/i }));
    expect(screen.getByLabelText(/^passphrase$/i).getAttribute("type")).toBe("text");
  });
});

describe("what a write replaces is said before the button", () => {
  it("names the count when the volume already holds networks", async () => {
    alreadyThereMock.mockResolvedValue(3);
    seedStore();
    render(<NetworkPanel />);
    await userEvent.click(screen.getByRole("checkbox", { name: /set up wifi/i }));

    const said = await screen.findByTestId("network-replacing");
    expect(said.textContent).toContain("3");
    // And it is above the action, not a report afterwards.
    expect(screen.queryByTestId("network-done")).toBeNull();
  });

  it("says nothing when there is nothing to replace", async () => {
    alreadyThereMock.mockResolvedValue(0);
    seedStore();
    render(<NetworkPanel />);
    await userEvent.click(screen.getByRole("checkbox", { name: /set up wifi/i }));
    await waitFor(() => expect(alreadyThereMock).toHaveBeenCalled());
    expect(screen.queryByTestId("network-replacing")).toBeNull();
  });
});

describe("what it sends", () => {
  it("hands the core one profile and the tree", async () => {
    seedStore();
    render(<NetworkPanel />);
    await fillWifi();
    await userEvent.click(screen.getByRole("button", { name: /write it to the volume/i }));

    await waitFor(() => expect(seedMock).toHaveBeenCalled());
    const [tree, networks, tolunnet] = seedMock.mock.calls.at(-1)!;
    expect(tree).toBe(TREE);
    expect(networks).toEqual([
      { ssid: "Tolun-Ev", security: "wpa", psk: "correct-horse", priority: 0 },
    ]);
    // The stack was not ticked, so nothing is sent for it — an untouched
    // `tolunnet.config` rather than one written with defaults.
    expect(tolunnet).toBeNull();
  });

  /// An open network carries no passphrase, and ART must not send the one
  /// left in the field from before the user changed the dropdown.
  it("sends no passphrase for an open network", async () => {
    seedStore();
    render(<NetworkPanel />);
    await fillWifi();
    await userEvent.selectOptions(screen.getByRole("combobox", { name: /security/i }), "open");
    await userEvent.click(screen.getByRole("button", { name: /write it to the volume/i }));

    await waitFor(() => expect(seedMock).toHaveBeenCalled());
    const [, networks] = seedMock.mock.calls.at(-1)!;
    expect(networks[0].psk).toBe("");
    expect(networks[0].security).toBe("open");
  });

  it("sends the stack's own settings when that half is ticked", async () => {
    seedStore();
    render(<NetworkPanel />);
    await userEvent.click(screen.getByRole("checkbox", { name: /tcp\/ip stack/i }));
    await userEvent.click(screen.getByRole("button", { name: /write it to the volume/i }));

    await waitFor(() => expect(seedMock).toHaveBeenCalled());
    const [, networks, tolunnet] = seedMock.mock.calls.at(-1)!;
    // WiFi was not ticked, so no profile is sent.
    expect(networks).toEqual([]);
    expect(tolunnet).toEqual({
      device: "wifipi.device",
      unit: 0,
      address: { how: "dhcp" },
    });
  });
});

describe("what it refuses, and where the sentence is", () => {
  it("says there is no system volume, on screen rather than in a tooltip", async () => {
    useSettingsStore.setState({
      loaded: true,
      settings: { ...DEFAULT_SETTINGS, remembered: {} },
    });
    render(<NetworkPanel />);

    const button = screen.getByRole("button", { name: /write it to the volume/i });
    expect((button as HTMLButtonElement).disabled).toBe(true);
    expect(document.body.textContent).toContain("Choose a system volume first.");
  });

  it("asks for something to be ticked before offering to write nothing", async () => {
    seedStore();
    render(<NetworkPanel />);
    expect(document.body.textContent).toContain("Tick WiFi, the stack, or both.");
  });

  /// The refusal says the length and the range and **never the value**.
  it("refuses a short passphrase without quoting it", async () => {
    seedStore();
    render(<NetworkPanel />);
    await fillWifi("Tolun-Ev", "short");

    const said = document.body.textContent ?? "";
    expect(said).toContain("8");
    expect(said).toContain("63");
    expect(said).not.toContain("short");
    expect(
      (screen.getByRole("button", { name: /write it to the volume/i }) as HTMLButtonElement)
        .disabled
    ).toBe(true);
    expect(seedMock).not.toHaveBeenCalled();
  });

  it("carries no raw key and no unrendered interpolation", async () => {
    seedStore();
    render(<NetworkPanel />);
    await fillWifi();
    await userEvent.click(screen.getByRole("checkbox", { name: /tcp\/ip stack/i }));

    const panel = screen.getByTestId("network-panel");
    expect(panel.textContent).not.toMatch(/network\.[a-zA-Z.]+/);
    expect(panel.textContent).not.toMatch(/\{\{[^}]+\}\}/);
  });
});
