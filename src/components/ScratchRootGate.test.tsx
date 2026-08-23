// @vitest-environment jsdom
//
// ART-196. Where ART stages work it will throw away became a choice, and the
// choice is asked once. Two things this file guards that nothing else can:
//
//  - **The question is asked once and then never again.** Not "usually once":
//    a screen that re-asks after a restart is a setting resetting itself,
//    which is the one outcome this project's own rule forbids outright.
//  - **The remembered answer is pushed back to Rust at start-up**, before
//    anything can stage. Rust holds the root only for the lifetime of the
//    process, so a remembered folder that is never pushed is a folder that
//    silently stops being used the moment ART restarts — the defect wearing
//    the fix's clothes.

import { render, screen, act, cleanup, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { DEFAULT_SETTINGS, type AppSettings } from "@/lib/settings";

const setRootMock = vi.fn();
const openMock = vi.fn();

vi.mock("@/lib/scratch", () => ({
  scratchSetRoot: (path: string | null) => setRootMock(path),
  scratchRoot: () => Promise.resolve({ inUse: null, chosen: null, default: "T", unreachable: null }),
}));

vi.mock("@tauri-apps/plugin-dialog", () => ({
  open: (opts: unknown) => openMock(opts),
}));

// The key **and** its parameters. A mock that returned the key alone would
// hide whether a sentence carries the thing that makes it actionable — which
// is the half `errorPhrase` exists to preserve (ART-060).
vi.mock("react-i18next", () => ({
  useTranslation: () => ({
    t: (key: string, params?: Record<string, unknown>) =>
      params ? `${key} ${Object.values(params).join(" ")}` : key,
  }),
}));

let settings: AppSettings = { ...DEFAULT_SETTINGS };
let loaded = true;
const updateMock = vi.fn();

vi.mock("@/stores/settingsStore", () => ({
  useSettingsStore: (select: (s: { settings: AppSettings; loaded: boolean; update: unknown }) => unknown) =>
    select({ settings, loaded, update: updateMock }),
}));

const { ScratchRootGate } = await import("@/components/ScratchRootGate");

beforeEach(() => {
  // Vitest is not configured with `globals`, so Testing Library's automatic
  // cleanup never runs — the previous test's card would still be on screen.
  cleanup();
  settings = { ...DEFAULT_SETTINGS };
  loaded = true;
  setRootMock.mockReset().mockResolvedValue({
    root: { inUse: null, chosen: null, default: "T", unreachable: null },
    previous: "T",
  });
  openMock.mockReset().mockResolvedValue(null);
  updateMock.mockReset().mockResolvedValue(undefined);
});

describe("asking once", () => {
  it("asks when it never has", async () => {
    await act(async () => {
      render(<ScratchRootGate />);
    });
    expect(screen.getByText("scratch.ask.heading")).toBeTruthy();
  });

  it("says nothing at all once it has been asked", async () => {
    settings = { ...DEFAULT_SETTINGS, scratchRootAsked: true };
    await act(async () => {
      render(<ScratchRootGate />);
    });
    expect(screen.queryByText("scratch.ask.heading")).toBeNull();
  });

  /// Pressing the first button is choosing the default, and choosing the
  /// default is still an answer — it must not come back next run.
  it("keeping the system drive counts as answered and chooses no folder", async () => {
    await act(async () => {
      render(<ScratchRootGate />);
    });
    await act(async () => {
      screen.getByText("scratch.ask.keepDefault").click();
    });
    expect(updateMock).toHaveBeenCalledWith({ scratchRootAsked: true });
    expect(updateMock).not.toHaveBeenCalledWith(
      expect.objectContaining({ scratchRoot: expect.anything() }),
    );
  });

  it("a chosen folder is remembered together with having been asked", async () => {
    openMock.mockResolvedValue("E:\\scratch");
    await act(async () => {
      render(<ScratchRootGate />);
    });
    await act(async () => {
      screen.getByText("scratch.ask.choose").click();
    });
    await waitFor(() => expect(setRootMock).toHaveBeenCalledWith("E:\\scratch"));
    expect(updateMock).toHaveBeenCalledWith({
      scratchRoot: "E:\\scratch",
      scratchRootAsked: true,
    });
  });

  /// **A folder ART refused is not a setting.** Storing it anyway would have
  /// the next run start with a root that does not work, and the question
  /// would not come back to fix it.
  it("a folder Rust refuses is not remembered, and the reason is shown", async () => {
    openMock.mockResolvedValue("E:\\gone");
    setRootMock.mockRejectedValue("ART stages its work in 'E:\\gone', and cannot right now");
    await act(async () => {
      render(<ScratchRootGate />);
    });
    await act(async () => {
      screen.getByText("scratch.ask.choose").click();
    });
    await waitFor(() => expect(screen.getByText(/cannot right now/)).toBeTruthy());
    expect(updateMock).not.toHaveBeenCalled();
    expect(screen.getByText("scratch.ask.heading")).toBeTruthy();
  });
});

describe("the remembered answer reaches Rust", () => {
  it("pushes the stored root at start-up, before anything can stage", async () => {
    settings = { ...DEFAULT_SETTINGS, scratchRoot: "E:\\scratch", scratchRootAsked: true };
    await act(async () => {
      render(<ScratchRootGate />);
    });
    await waitFor(() => expect(setRootMock).toHaveBeenCalledWith("E:\\scratch"));
  });

  /// **An unrelated setting changing must not re-send the root.** The effect
  /// is keyed on the value, so a theme change or a collapsed sidebar does not
  /// have ART talk to Rust about a path it already has.
  ///
  /// Written this way after the first version of this test turned out to be
  /// inert: it re-rendered with everything unchanged, which a dependency
  /// array stops on its own, so it passed just as happily with the guard it
  /// was written for removed. This one falls when the dependencies are
  /// widened to the settings object.
  it("does not re-send the root when an unrelated setting changes", async () => {
    settings = { ...DEFAULT_SETTINGS, scratchRoot: "E:\\scratch", scratchRootAsked: true };
    const view = await act(async () => render(<ScratchRootGate />));
    await waitFor(() => expect(setRootMock).toHaveBeenCalledTimes(1));

    settings = { ...settings, theme: "light", sidebarCollapsed: true };
    await act(async () => {
      view.rerender(<ScratchRootGate />);
    });
    expect(setRootMock).toHaveBeenCalledTimes(1);
  });

  /// Nothing is pushed before the settings file has been read — otherwise the
  /// default would be sent first and the user's own folder second, and any
  /// job in between would stage on the system drive.
  it("waits for the settings to be loaded", async () => {
    loaded = false;
    settings = { ...DEFAULT_SETTINGS, scratchRoot: "E:\\scratch", scratchRootAsked: true };
    await act(async () => {
      render(<ScratchRootGate />);
    });
    expect(setRootMock).not.toHaveBeenCalled();
  });

  /// A remembered folder that has since been unplugged must not put an error
  /// on the Dashboard the moment ART opens. The refusal comes from the
  /// operation that actually needed to stage.
  it("a refused push at start-up is silent", async () => {
    settings = { ...DEFAULT_SETTINGS, scratchRoot: "E:\\gone", scratchRootAsked: true };
    setRootMock.mockRejectedValue("unreachable");
    await act(async () => {
      render(<ScratchRootGate />);
    });
    await waitFor(() => expect(setRootMock).toHaveBeenCalled());
    expect(screen.queryByText("unreachable")).toBeNull();
  });
});
