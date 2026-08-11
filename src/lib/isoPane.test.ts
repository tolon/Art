// Covers the disc pane's routing in `isoPane.ts` directly — see that file's
// header comment, and `sort.test.ts`'s, for why: `FileManager.tsx` calls
// Tauri commands on mount and is not something to render for pure routing
// checks.
import { describe, expect, it } from "vitest";

import {
  copyDirection,
  enterIsoTrail,
  isVolumeKind,
  leaveIsoTrail,
  type PaneKind,
} from "./isoPane";

describe("isVolumeKind", () => {
  it("is true for adf and hdf, false for local and iso", () => {
    expect(isVolumeKind("adf")).toBe(true);
    expect(isVolumeKind("hdf")).toBe(true);
    expect(isVolumeKind("local")).toBe(false);
    expect(isVolumeKind("iso")).toBe(false);
  });
});

describe("copyDirection", () => {
  it("routes local to local as a plain host copy", () => {
    expect(copyDirection("local", "local")).toEqual({ kind: "local-to-local" });
  });

  it("routes local to a volume as F5 in, for both adf and hdf targets", () => {
    expect(copyDirection("local", "adf")).toEqual({ kind: "local-to-volume" });
    expect(copyDirection("local", "hdf")).toEqual({ kind: "local-to-volume" });
  });

  it("routes a volume to local as F5 out, for both adf and hdf sources", () => {
    expect(copyDirection("adf", "local")).toEqual({ kind: "volume-to-local" });
    expect(copyDirection("hdf", "local")).toEqual({ kind: "volume-to-local" });
  });

  it("routes volume to volume as the staged Amiga-to-Amiga copy, any combination", () => {
    expect(copyDirection("adf", "hdf")).toEqual({ kind: "volume-to-volume" });
    expect(copyDirection("hdf", "adf")).toEqual({ kind: "volume-to-volume" });
    expect(copyDirection("adf", "adf")).toEqual({ kind: "volume-to-volume" });
  });

  it("routes a disc to local as the disc extraction command", () => {
    expect(copyDirection("iso", "local")).toEqual({ kind: "iso-to-local" });
  });

  it("routes a disc to a volume as the disc-to-Amiga copy, for both adf and hdf targets", () => {
    expect(copyDirection("iso", "adf")).toEqual({ kind: "iso-to-volume" });
    expect(copyDirection("iso", "hdf")).toEqual({ kind: "iso-to-volume" });
  });

  it("refuses every direction that would write into a disc", () => {
    const sources: PaneKind[] = ["local", "adf", "hdf", "iso"];
    for (const source of sources) {
      const result = copyDirection(source, "iso");
      expect(result.kind).toBe("refused");
      if (result.kind === "refused") {
        expect(result.reason.key).toBe("files.writeRefusal.iso");
      }
    }
  });

  it("a disc targeting itself is refused for being a write target, not treated as a copy", () => {
    // Guards against `target === "iso"` losing its priority check: if
    // `source === "iso"` were tested first, this would come back
    // "iso-to-volume" or similar instead of refused.
    expect(copyDirection("iso", "iso").kind).toBe("refused");
  });
});

describe("enterIsoTrail / leaveIsoTrail", () => {
  it("push then pop returns exactly where the pane was before entering", () => {
    const trail = enterIsoTrail([], "Tools", 20, 2048);
    expect(trail).toEqual([{ name: "Tools", extent: 20, length: 2048 }]);

    const back = leaveIsoTrail(trail);
    expect(back).toEqual({ extent: 20, length: 2048, trail: [] });
  });

  it("nests several levels and unwinds them in reverse order", () => {
    let trail = enterIsoTrail([], "Tools", 20, 2048);
    trail = enterIsoTrail(trail, "Editor", 40, 2048);
    trail = enterIsoTrail(trail, "Config", 60, 2048);
    expect(trail.map((t) => t.name)).toEqual(["Tools", "Editor", "Config"]);

    const up1 = leaveIsoTrail(trail)!;
    expect(up1.extent).toBe(60); // back to what Config's entry captured
    expect(up1.trail.map((t) => t.name)).toEqual(["Tools", "Editor"]);

    const up2 = leaveIsoTrail(up1.trail)!;
    expect(up2.extent).toBe(40);
    expect(up2.trail.map((t) => t.name)).toEqual(["Tools"]);

    const up3 = leaveIsoTrail(up2.trail)!;
    expect(up3.extent).toBe(20);
    expect(up3.trail).toEqual([]);
  });

  it("returns null at the root, where there is nothing to go up to", () => {
    expect(leaveIsoTrail([])).toBeNull();
  });

  it("leaveIsoTrail never mutates the trail it was given", () => {
    const trail = enterIsoTrail([], "Tools", 20, 2048);
    const frozen = JSON.stringify(trail);
    leaveIsoTrail(trail);
    expect(JSON.stringify(trail)).toBe(frozen);
  });
});
