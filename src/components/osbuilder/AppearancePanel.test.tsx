// @vitest-environment jsdom
//
// The Görünüm panel — Task 10 of the prefs-and-wallpaper round, and the last
// hop in a chain the WHDLoad round's own lesson exists to guard against: an
// engine and a command that compile and test green, with nothing on screen
// that ever calls them.
//
// Mocked at the `@/lib/*` boundary, the house pattern `NetworkPanel.test.tsx`
// already established for this same step. `@/lib/settings` is mocked one
// layer further down for the reason `OsInstall.test.tsx` records:
// `useRemembered` writes through `useSettingsStore`, and the real
// `saveSettings` rejects in jsdom with nothing to catch it.

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import i18n from "@/i18n";
import { useSettingsStore } from "@/stores/settingsStore";

const applyMock = vi.hoisted(() => vi.fn());
const backdropsMock = vi.hoisted(() => vi.fn());

vi.mock("@/lib/appearance", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/lib/appearance")>()),
  appearanceApply: applyMock,
  appearanceBackdrops: backdropsMock,
}));

vi.mock("@/lib/settings", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/lib/settings")>()),
  getSettings: vi.fn(),
  saveSettings: vi.fn().mockResolvedValue(undefined),
}));

const { AppearancePanel } = await import("@/components/osbuilder/AppearancePanel");
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
  backdropsMock.mockReset().mockResolvedValue([]);
  applyMock.mockReset().mockResolvedValue({
    written: ["Prefs/Env-Archive/Sys/WBPattern.prefs"],
    backups: [],
    picturePlaced: null,
    amigaPath: "Sys:Prefs/Presets/Backdrops/default_pal.iff",
  });
});

afterEach(() => {
  cleanup();
  i18n.changeLanguage("en");
  useSettingsStore.setState({ loaded: false, settings: DEFAULT_SETTINGS });
});

/** Tick "change the wallpaper" — the section every other test needs open. */
async function enableWallpaper() {
  await userEvent.click(
    screen.getByRole("checkbox", { name: /change the desktop wallpaper/i })
  );
}

describe("it offers only the backdrops the built tree actually contains", () => {
  it("lists exactly what appearanceBackdrops returned, nothing invented", async () => {
    backdropsMock.mockResolvedValue(["Christmas.iff", "default_pal.iff"]);
    seedStore();
    render(<AppearancePanel />);
    await enableWallpaper();

    const select = await screen.findByRole("combobox", { name: /backdrop/i });
    await waitFor(() => expect(within(select).getAllByRole("option")).toHaveLength(3));

    const names = within(select)
      .getAllByRole("option")
      .map((option) => option.textContent);
    // The placeholder, then exactly the two names the tree actually holds —
    // never a fixed list ART invented.
    expect(names).toEqual([
      expect.stringMatching(/choose/i),
      "Christmas.iff",
      "default_pal.iff",
    ]);
  });

  it("says nothing is there yet when the tree carries no backdrops drawer", async () => {
    backdropsMock.mockResolvedValue([]);
    seedStore();
    render(<AppearancePanel />);
    await enableWallpaper();

    const select = await screen.findByRole("combobox", { name: /backdrop/i });
    await waitFor(() => expect(backdropsMock).toHaveBeenCalledWith(TREE));
    expect(within(select).getAllByRole("option")).toHaveLength(1);
    expect(document.body.textContent).toContain("Nothing has been placed there yet");
  });
});

describe("what it refuses, and where the sentence is", () => {
  it("shows the refusal text when a picture is neither PNG nor JPEG, exactly as core wrote it", async () => {
    // The real sentence `core::picture::decode` produces, with the real
    // trailer `AppError::user_message` appends — never reworded on this side
    // (ART-060).
    applyMock.mockRejectedValue(
      new Error(
        "malformed picture: ART can turn a PNG or a JPEG into a wallpaper; this file is " +
          "neither\n\nError ID: ART-FORMAT-MALFORMED"
      )
    );
    seedStore();
    render(<AppearancePanel />);
    await enableWallpaper();
    await userEvent.click(screen.getByRole("radio", { name: /a picture from this pc/i }));

    // Faking a chosen host path directly through the remembered store —
    // opening the native file dialog is not exercised in jsdom, the same
    // reason `VolumePreload.test.tsx`'s siblings do not open one either.
    const current = useSettingsStore.getState().settings;
    useSettingsStore.setState({
      settings: {
        ...current,
        remembered: {
          ...(current.remembered as Record<string, unknown>),
          "appearance.hostPath": "C:\\not-a-picture.png",
        },
      },
    });

    await userEvent.click(screen.getByRole("button", { name: /apply/i }));

    const said = await screen.findByTestId("appearance-error");
    expect(said.textContent).toContain("ART can turn a PNG or a JPEG into a wallpaper");
    expect(said.textContent).toContain("this file is neither");
    expect(said.textContent).toContain("ART-FORMAT-MALFORMED");
  });
});

describe("a choice survives a remount", () => {
  it("remembers the chosen placement", async () => {
    seedStore();
    const { unmount } = render(<AppearancePanel />);
    await enableWallpaper();

    await userEvent.selectOptions(
      screen.getByRole("combobox", { name: /placement/i }),
      "tile"
    );
    expect(
      (screen.getByRole("combobox", { name: /placement/i }) as HTMLSelectElement).value
    ).toBe("tile");

    // Remount without re-seeding — a real remount reads back whatever the
    // live settings store already holds, the same shape
    // `OsInstall.test.tsx`'s own remount test uses. `wallpaperOn` is itself
    // remembered and is already `true` from the click above, so the section
    // is open again without touching the checkbox a second time — clicking
    // it again would toggle it back off.
    unmount();
    render(<AppearancePanel />);
    await screen.findByRole("combobox", { name: /placement/i });

    expect(
      (screen.getByRole("combobox", { name: /placement/i }) as HTMLSelectElement).value
    ).toBe("tile");
  });
});

describe("what a successful apply says", () => {
  it("names where the previous version's backup went", async () => {
    backdropsMock.mockResolvedValue(["default_pal.iff"]);
    applyMock.mockResolvedValue({
      written: ["Prefs/Env-Archive/Sys/WBPattern.prefs"],
      backups: ["Prefs/Env-Archive/Sys/WBPattern.prefs.bak.1"],
      picturePlaced: null,
      amigaPath: "Sys:Prefs/Presets/Backdrops/default_pal.iff",
    });
    seedStore();
    render(<AppearancePanel />);
    await enableWallpaper();

    const select = await screen.findByRole("combobox", { name: /backdrop/i });
    await userEvent.selectOptions(select, "Sys:Prefs/Presets/Backdrops/default_pal.iff");
    await userEvent.click(screen.getByRole("button", { name: /apply/i }));

    const backup = await screen.findByTestId("appearance-backup");
    expect(backup.textContent).toContain("Prefs/Env-Archive/Sys/WBPattern.prefs.bak.1");
  });

  it("says nothing about a backup when nothing existed to back up", async () => {
    backdropsMock.mockResolvedValue(["default_pal.iff"]);
    applyMock.mockResolvedValue({
      written: ["Prefs/Env-Archive/Sys/WBPattern.prefs"],
      backups: [],
      picturePlaced: null,
      amigaPath: "Sys:Prefs/Presets/Backdrops/default_pal.iff",
    });
    seedStore();
    render(<AppearancePanel />);
    await enableWallpaper();

    const select = await screen.findByRole("combobox", { name: /backdrop/i });
    await userEvent.selectOptions(select, "Sys:Prefs/Presets/Backdrops/default_pal.iff");
    await userEvent.click(screen.getByRole("button", { name: /apply/i }));

    await screen.findByTestId("appearance-done");
    expect(screen.queryByTestId("appearance-backup")).toBeNull();
  });
});

describe("what reaches the wire", () => {
  // Fix round 1: the wallpaper path was tested to the standard of "assert the
  // exact object handed to core", but screen depth and shell defaults —
  // added on this task's own initiative, alongside the wallpaper the brief
  // named — were only exercised through the pure `appearanceBlocker`
  // function. A test that never ticks the box and never reads
  // `applyMock.mock.calls` cannot tell "the value reaches the request" from
  // "the field renders and does nothing".
  it("sends the screen depth, and the wallpaper stays absent", async () => {
    seedStore();
    render(<AppearancePanel />);
    await userEvent.click(
      screen.getByRole("checkbox", { name: /change the screen depth/i })
    );
    const depthField = screen.getByRole("textbox", { name: /depth/i });
    // A single `change` event carrying the whole new value, rather than
    // `userEvent.type` keystroke by keystroke: the field is controlled and
    // only commits a value inside its guarded range (1-8), so a `clear()`
    // that briefly leaves it empty is correctly refused rather than
    // committed — this is the field working as intended, not a test
    // workaround for a bug.
    fireEvent.change(depthField, { target: { value: "6" } });

    await userEvent.click(screen.getByRole("button", { name: /apply/i }));
    await waitFor(() => expect(applyMock).toHaveBeenCalled());

    const [tree, request] = applyMock.mock.calls.at(-1)!;
    expect(tree).toBe(TREE);
    expect(request.screenDepth).toBe(6);
    // The test cannot pass on a request that simply contains everything —
    // the wallpaper was never ticked, so it must be absent, not merely
    // "some value or other".
    expect(request.wallpaper).toBeNull();
    expect(request.shellDefaults).toBe(false);
  });

  it("sends the shell defaults flag, and the wallpaper and screen depth stay absent", async () => {
    seedStore();
    render(<AppearancePanel />);
    await userEvent.click(
      screen.getByRole("checkbox", { name: /write the shell's own defaults/i })
    );
    await userEvent.click(screen.getByRole("button", { name: /apply/i }));
    await waitFor(() => expect(applyMock).toHaveBeenCalled());

    const [tree, request] = applyMock.mock.calls.at(-1)!;
    expect(tree).toBe(TREE);
    expect(request.shellDefaults).toBe(true);
    expect(request.wallpaper).toBeNull();
    expect(request.screenDepth).toBeNull();
  });
});

describe("a stale or hand-edited settings file cannot put a bad value on screen", () => {
  it("falls back to the default depth when the stored value is outside the guard's range", async () => {
    // `isWholeNumberBetween(1, 8)`'s whole purpose: a value a guard rejects
    // must fall back to the default (4) rather than reach the screen —
    // ART's own "nothing changes unless the user changes it" rule read from
    // the other side, for a file the user never touched but something else
    // (an older ART, a hand edit) did.
    seedStore({ "appearance.screenDepthOn": true, "appearance.screenDepth": 999 });
    render(<AppearancePanel />);

    const depthField = await screen.findByRole("textbox", { name: /depth/i });
    expect((depthField as HTMLInputElement).value).toBe("4");
  });
});

describe("what it refuses before ever calling core, and where the sentence is", () => {
  it("says there is no system volume, on screen rather than in a tooltip alone", async () => {
    useSettingsStore.setState({
      loaded: true,
      settings: { ...DEFAULT_SETTINGS, remembered: {} },
    });
    render(<AppearancePanel />);
    await enableWallpaper();

    const button = screen.getByRole("button", { name: /apply/i });
    expect((button as HTMLButtonElement).disabled).toBe(true);
    expect(document.body.textContent).toContain("Choose a system volume first.");
  });

  it("asks for something to be ticked before offering to apply nothing", async () => {
    seedStore();
    render(<AppearancePanel />);
    expect(document.body.textContent).toContain(
      "Tick the wallpaper, the screen depth, the shell defaults, or some of them."
    );
  });

  it("asks for a backdrop once the wallpaper is ticked and nothing is chosen", async () => {
    seedStore();
    render(<AppearancePanel />);
    await enableWallpaper();
    expect(document.body.textContent).toContain("Choose a backdrop already in the tree.");
  });
});

describe("the panel in Turkish", () => {
  it("renders in Turkish when the language is tr", async () => {
    await i18n.changeLanguage("tr");
    seedStore();
    render(<AppearancePanel />);

    expect(
      screen.getByRole("checkbox", { name: /masaüstü duvar kağıdını değiştir/i })
    ).toBeTruthy();
    expect(document.body.textContent).not.toMatch(/appearance\.[a-zA-Z.]+/);
    expect(document.body.textContent).not.toMatch(/\{\{[^}]+\}\}/);
  });
});
