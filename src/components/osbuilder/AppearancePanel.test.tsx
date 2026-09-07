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
import type { AppearanceOutcome } from "@/lib/appearance";
import type { JobProgress } from "@/lib/jobs";

const applyMock = vi.hoisted(() => vi.fn());
const backdropsMock = vi.hoisted(() => vi.fn());
const awaitJobResultMock = vi.hoisted(() => vi.fn());
const jobCancelMock = vi.hoisted(() => vi.fn());
const onJobProgressMock = vi.hoisted(() => vi.fn());

vi.mock("@/lib/appearance", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/lib/appearance")>()),
  appearanceApply: applyMock,
  appearanceBackdrops: backdropsMock,
}));

// ART-248: `appearanceApply` now only starts the job (it resolves with a job
// id) and the outcome arrives through `awaitJobResult` — the same seam
// `FirstBootPanel.test.tsx` mocks for its own rehearsal job. `isJobCancellation`
// stays real, the same reason: it is a pure predicate and the whole point of
// the "stopped, not an error" test below is that the panel's own use of it is
// correct. `fraction` and `subscribeSafely` stay real for the identical
// reason — they are pure, and the progress test is exercising the panel's
// real use of them.
vi.mock("@/lib/jobs", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/lib/jobs")>()),
  awaitJobResult: awaitJobResultMock,
  jobCancel: jobCancelMock,
  onJobProgress: onJobProgressMock,
}));

vi.mock("@/lib/settings", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/lib/settings")>()),
  getSettings: vi.fn(),
  saveSettings: vi.fn().mockResolvedValue(undefined),
}));

const { AppearancePanel } = await import("@/components/osbuilder/AppearancePanel");
const { DEFAULT_SETTINGS } = await import("@/lib/settings");
const { JOB_CANCELLED_MESSAGE } = await import("@/lib/jobs");

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

const DEFAULT_OUTCOME: AppearanceOutcome = {
  written: ["Prefs/Env-Archive/Sys/WBPattern.prefs"],
  backups: [],
  picturePlaced: null,
  amigaPath: "Sys:Prefs/Presets/Backdrops/default_pal.iff",
  iconsPlaced: 0,
  drawersArranged: 0,
  iconsSkipped: [],
};

/** What the job's own `awaitJobResult` promise settles with, by default —
 *  overridden per test either by reassigning this before clicking Apply, or
 *  by a one-off `awaitJobResultMock.mockImplementationOnce(...)` for a
 *  rejection (an error or a cancellation). */
let currentOutcome: AppearanceOutcome = DEFAULT_OUTCOME;

/** The one live `onJobProgress` handler, so a test can deliver a progress
 *  update the way the backend would — the same shape
 *  `AmigaInstallPanel.test.tsx` uses for its own job channel. */
let report: ((progress: JobProgress) => void) | null = null;

beforeEach(() => {
  useSettingsStore.setState({ loaded: false, settings: DEFAULT_SETTINGS });
  backdropsMock.mockReset().mockResolvedValue([]);
  applyMock.mockReset().mockResolvedValue(1);
  currentOutcome = DEFAULT_OUTCOME;
  report = null;
  jobCancelMock.mockReset().mockResolvedValue(true);
  onJobProgressMock.mockReset().mockImplementation(async (handler: (p: JobProgress) => void) => {
    report = handler;
    return () => {};
  });
  // `awaitJobResult`'s own contract: it calls `start` itself (which is what
  // actually invokes `appearanceApply` and sets the panel's own job id) and
  // hands back a promise this default resolves with `currentOutcome` — the
  // same shape `FirstBootPanel.test.tsx`'s own `awaitJobResultMock` uses.
  awaitJobResultMock.mockReset().mockImplementation((_event: string, start: () => Promise<number>) => {
    void start();
    return Promise.resolve(currentOutcome);
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
    // (ART-060). ART-248: this refusal now happens inside the job (planning
    // runs on the job thread), so it arrives as `awaitJobResult`'s own
    // rejection, not `appearanceApply`'s — the initial `invoke` still
    // resolves with a job id.
    awaitJobResultMock.mockImplementationOnce(
      (_event: string, start: () => Promise<number>) => {
        void start();
        return Promise.reject(
          new Error(
            "malformed picture: ART can turn a PNG or a JPEG into a wallpaper; this file is " +
              "neither\n\nError ID: ART-FORMAT-MALFORMED"
          )
        );
      }
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

  it("remembers the icon-arrangement choice", async () => {
    // `isFlag` guard, the same as every other checkbox on this panel —
    // ticking it and remounting without re-seeding must read back `true`
    // from the live settings store rather than resetting to the default.
    seedStore();
    const { unmount } = render(<AppearancePanel />);
    await userEvent.click(
      screen.getByRole("checkbox", { name: /arrange the drawer icons/i })
    );
    expect(
      (
        screen.getByRole("checkbox", {
          name: /arrange the drawer icons/i,
        }) as HTMLInputElement
      ).checked
    ).toBe(true);

    unmount();
    render(<AppearancePanel />);

    expect(
      (
        screen.getByRole("checkbox", {
          name: /arrange the drawer icons/i,
        }) as HTMLInputElement
      ).checked
    ).toBe(true);
  });
});

describe("what a successful apply says", () => {
  it("names where the previous version's backup went", async () => {
    backdropsMock.mockResolvedValue(["default_pal.iff"]);
    currentOutcome = {
      written: ["Prefs/Env-Archive/Sys/WBPattern.prefs"],
      backups: ["Prefs/Env-Archive/Sys/WBPattern.prefs.bak.1"],
      picturePlaced: null,
      amigaPath: "Sys:Prefs/Presets/Backdrops/default_pal.iff",
      iconsPlaced: 0,
      drawersArranged: 0,
      iconsSkipped: [],
    };
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
    currentOutcome = {
      written: ["Prefs/Env-Archive/Sys/WBPattern.prefs"],
      backups: [],
      picturePlaced: null,
      amigaPath: "Sys:Prefs/Presets/Backdrops/default_pal.iff",
      iconsPlaced: 0,
      drawersArranged: 0,
      iconsSkipped: [],
    };
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

  it("sends arrangeIcons when the box is ticked, and the other capabilities stay absent", async () => {
    // Task 8 of the drawer-icons round, and the standard set above: a
    // request that always sends everything would pass a weaker assertion —
    // this one fails unless wallpaper/screenDepth/shellDefaults are actually
    // absent while only arrangeIcons is true.
    seedStore();
    render(<AppearancePanel />);
    await userEvent.click(
      screen.getByRole("checkbox", { name: /arrange the drawer icons/i })
    );
    await userEvent.click(screen.getByRole("button", { name: /apply/i }));
    await waitFor(() => expect(applyMock).toHaveBeenCalled());

    const [tree, request] = applyMock.mock.calls.at(-1)!;
    expect(tree).toBe(TREE);
    expect(request.arrangeIcons).toBe(true);
    expect(request.wallpaper).toBeNull();
    expect(request.screenDepth).toBeNull();
    expect(request.shellDefaults).toBe(false);
  });
});

describe("what a successful icon arrangement says", () => {
  it("says how many icons were placed after a successful run, by number", async () => {
    currentOutcome = {
      written: ["Utilities.info"],
      backups: [],
      picturePlaced: null,
      amigaPath: null,
      iconsPlaced: 7,
      drawersArranged: 3,
      iconsSkipped: [],
    };
    seedStore();
    render(<AppearancePanel />);
    await userEvent.click(
      screen.getByRole("checkbox", { name: /arrange the drawer icons/i })
    );
    await userEvent.click(screen.getByRole("button", { name: /apply/i }));

    const placed = await screen.findByTestId("appearance-icons-placed");
    // The specific numbers, not "it worked" — a screen that shows a fixed
    // success badge regardless of the outcome carries no information
    // (CLAUDE.md's own rule for a progress bar applies just as much here).
    expect(placed.textContent).toContain("7");
    expect(placed.textContent).toContain("3");
  });

  it("names the icon that could not be read, rather than silently dropping it", async () => {
    currentOutcome = {
      written: [],
      backups: [],
      picturePlaced: null,
      amigaPath: null,
      iconsPlaced: 2,
      drawersArranged: 1,
      iconsSkipped: ["E:\\amiga\\dist-3.2\\Utilities\\Bad.info"],
    };
    seedStore();
    render(<AppearancePanel />);
    await userEvent.click(
      screen.getByRole("checkbox", { name: /arrange the drawer icons/i })
    );
    await userEvent.click(screen.getByRole("button", { name: /apply/i }));

    const skipped = await screen.findByTestId("appearance-icons-skipped");
    expect(skipped.textContent).toContain("E:\\amiga\\dist-3.2\\Utilities\\Bad.info");
  });

  it("says nothing needed writing, rather than a bare written line, when every icon was already placed", async () => {
    // C7 (final whole-branch review): ticking only "Arrange icons" on a tree
    // whose icons are all already placed used to render `Written: ` with
    // nothing after it — the same blank shape a real failure could produce,
    // which is exactly the "must not be the same screen" defect CLAUDE.md
    // names. A directory with nothing to place commits nothing at all
    // (`core::appearance::plan_icon_arrangement`'s own doc), so `written`,
    // `iconsPlaced` and `drawersArranged` are all zero here.
    currentOutcome = {
      written: [],
      backups: [],
      picturePlaced: null,
      amigaPath: null,
      iconsPlaced: 0,
      drawersArranged: 0,
      iconsSkipped: [],
    };
    seedStore();
    render(<AppearancePanel />);
    await userEvent.click(
      screen.getByRole("checkbox", { name: /arrange the drawer icons/i })
    );
    await userEvent.click(screen.getByRole("button", { name: /apply/i }));

    const nothing = await screen.findByTestId("appearance-nothing-written");
    expect(nothing.textContent).toBeTruthy();
    expect(screen.queryByTestId("appearance-icons-placed")).toBeNull();
    expect(screen.queryByTestId("appearance-icons-skipped")).toBeNull();
  });
});

describe("ART-248: the apply runs as a cancellable job", () => {
  it("shows a progress line once the job reports, naming a real count", async () => {
    // The job never settles in this test — `awaitJobResultMock` is
    // overridden with a promise nothing resolves, the same shape
    // `AmigaInstallPanel.test.tsx` uses to hold a job "running" so its own
    // progress channel can be exercised.
    awaitJobResultMock.mockImplementationOnce(
      (_event: string, start: () => Promise<number>) => {
        void start();
        return new Promise(() => {});
      }
    );
    seedStore();
    render(<AppearancePanel />);
    await userEvent.click(
      screen.getByRole("checkbox", { name: /arrange the drawer icons/i })
    );
    await userEvent.click(screen.getByRole("button", { name: /apply/i }));
    await waitFor(() => expect(applyMock).toHaveBeenCalled());

    report!({
      id: 1,
      title: "Applying appearance to distribution tree",
      done: 3,
      total: 6,
      message: "Utilities.info",
      state: { state: "running" },
    });

    const progress = await screen.findByTestId("appearance-progress");
    // A count, not a fixed-width bar with no information (CLAUDE.md).
    expect(progress.textContent).toContain("3");
    expect(progress.textContent).toContain("6");
    expect(progress.textContent).toContain("Utilities.info");
  });

  it("calls jobCancel with the running job's own id when Stop is pressed", async () => {
    awaitJobResultMock.mockImplementationOnce(
      (_event: string, start: () => Promise<number>) => {
        void start();
        return new Promise(() => {});
      }
    );
    applyMock.mockReset().mockResolvedValue(9);
    seedStore();
    render(<AppearancePanel />);
    await userEvent.click(
      screen.getByRole("checkbox", { name: /arrange the drawer icons/i })
    );
    await userEvent.click(screen.getByRole("button", { name: /apply/i }));
    await waitFor(() => expect(applyMock).toHaveBeenCalled());

    await userEvent.click(screen.getByRole("button", { name: /stop/i }));
    expect(jobCancelMock).toHaveBeenCalledWith(9);
  });

  it("renders a stopped sentence rather than an error when the job is cancelled", async () => {
    // `isJobCancellation` is real in this suite — this proves the panel's
    // own use of it, not the predicate itself.
    awaitJobResultMock.mockImplementationOnce(
      (_event: string, start: () => Promise<number>) => {
        void start();
        return Promise.reject(new Error(JOB_CANCELLED_MESSAGE));
      }
    );
    seedStore();
    render(<AppearancePanel />);
    await userEvent.click(
      screen.getByRole("checkbox", { name: /arrange the drawer icons/i })
    );
    await userEvent.click(screen.getByRole("button", { name: /apply/i }));

    const cancelled = await screen.findByTestId("appearance-cancelled");
    expect(cancelled.textContent).toBeTruthy();
    // Endings stay distinct (CLAUDE.md): the user's own stop is not the
    // error sentence, and the two must never render together.
    expect(screen.queryByTestId("appearance-error")).toBeNull();
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
      "Tick the wallpaper, the screen depth, the shell defaults, the icon arrangement, or " +
        "some of them."
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
    // Task 8's own checkbox, in Turkish — the round's addition is not an
    // English afterthought bolted onto an otherwise-translated screen.
    expect(
      screen.getByRole("checkbox", {
        name: /workbench'in hiç yerleştirmediği çekmece simgelerini düzenle/i,
      })
    ).toBeTruthy();
    expect(document.body.textContent).not.toMatch(/appearance\.[a-zA-Z.]+/);
    expect(document.body.textContent).not.toMatch(/\{\{[^}]+\}\}/);
  });
});
