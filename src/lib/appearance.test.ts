// Pure TS, no DOM needed — vitest's environment is `node` by default and
// jsdom only applies to `src/**/*.test.tsx` (vite.config.ts), so this stays a
// `.test.ts` file.
//
// What this actually proves: the wrapper passes its arguments through to
// `invoke` unchanged and returns exactly what `invoke` resolved, with no
// reshaping — the specific risk this task calls out, since a feature that
// compiles and tests green but is never wired into `invoke_handler![]` (or
// whose wrapper silently drops or renames an argument) is a shipped-twice
// mistake in this project (CLAUDE.md).

import { beforeEach, describe, expect, it, vi } from "vitest";

const invokeMock = vi.fn();

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

beforeEach(() => {
  invokeMock.mockReset();
});

import {
  appearanceApply,
  appearanceBackdrops,
  type AppearanceApplyRequest,
  type AppearanceOutcome,
} from "@/lib/appearance";

describe("appearanceBackdrops", () => {
  it("passes the tree argument through unchanged and returns invoke's result", async () => {
    const names = ["default_pal.iff", "Christmas.iff"];
    invokeMock.mockResolvedValueOnce(names);

    const result = await appearanceBackdrops("E:\\builds\\amigaos32");

    expect(invokeMock).toHaveBeenCalledTimes(1);
    expect(invokeMock).toHaveBeenCalledWith("appearance_backdrops", {
      tree: "E:\\builds\\amigaos32",
    });
    expect(result).toBe(names);
  });
});

describe("appearanceApply", () => {
  it("passes tree and request through unchanged and returns invoke's result verbatim", async () => {
    const request: AppearanceApplyRequest = {
      wallpaper: {
        which: "root",
        source: {
          kind: "host-picture",
          path: "C:\\Users\\me\\wallpaper.png",
          colours: 16,
        },
        placement: "center",
      },
      screenDepth: 8,
      shellDefaults: true,
    };
    const outcome: AppearanceOutcome = {
      written: ["Prefs/Env-Archive/Sys/WBPattern.prefs"],
      backups: ["Prefs/Env-Archive/Sys/WBPattern.prefs.bak"],
      picturePlaced: "Prefs/Presets/Backdrops/wallpaper.iff",
      amigaPath: "Sys:Prefs/Presets/Backdrops/wallpaper.iff",
    };
    invokeMock.mockResolvedValueOnce(outcome);

    const result = await appearanceApply("E:\\builds\\amigaos32", request);

    expect(invokeMock).toHaveBeenCalledTimes(1);
    expect(invokeMock).toHaveBeenCalledWith("appearance_apply", {
      tree: "E:\\builds\\amigaos32",
      request,
    });
    // No reshaping: the exact object `invoke` resolved is what the caller
    // gets back.
    expect(result).toBe(outcome);
  });

  it("passes an already-in-tree wallpaper source through unchanged", async () => {
    const request: AppearanceApplyRequest = {
      wallpaper: {
        which: "drawer",
        source: {
          kind: "already-in-tree",
          amigaPath: "Sys:Prefs/Presets/Backdrops/Christmas.iff",
        },
        placement: "tile",
      },
      screenDepth: null,
      shellDefaults: false,
    };
    invokeMock.mockResolvedValueOnce({
      written: [],
      backups: [],
      picturePlaced: null,
      amigaPath: "Sys:Prefs/Presets/Backdrops/Christmas.iff",
    });

    await appearanceApply("E:\\builds\\amigaos32", request);

    expect(invokeMock).toHaveBeenCalledWith("appearance_apply", {
      tree: "E:\\builds\\amigaos32",
      request,
    });
  });

  it("rejects with the error invoke rejected with, unchanged", async () => {
    const message =
      "'Prefs/Env-Archive/Sys/WBPattern.prefs' was not found under this distribution tree " +
      "('E:\\builds\\amigaos32')\n\nError ID: ART-CORE-INVALID-INPUT";
    invokeMock.mockRejectedValueOnce(message);

    await expect(
      appearanceApply("E:\\builds\\amigaos32", {
        wallpaper: null,
        screenDepth: 8,
        shellDefaults: false,
      })
    ).rejects.toBe(message);
  });
});
