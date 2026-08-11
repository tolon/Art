// Covers `paneStatusCounts` directly — see `sort.test.ts`'s comment for why
// a pure `src/lib` function gets its own test file rather than being
// exercised only through `FileManager.tsx`.
import { describe, expect, it } from "vitest";

import { paneStatusCounts } from "./panelStatus";
import type { PanelEntry } from "./panel";

function entry(name: string, overrides: Partial<PanelEntry> = {}): PanelEntry {
  return {
    name,
    is_dir: false,
    bytes: 100,
    path: null,
    header_block: null,
    is_link: false,
    date: null,
    attrs: null,
    ...overrides,
  };
}

describe("paneStatusCounts", () => {
  it("counts nothing for an empty pane", () => {
    expect(paneStatusCounts([], new Set())).toEqual({
      selectedBytes: 0,
      totalBytes: 0,
      selectedFiles: 0,
      totalFiles: 0,
      selectedDirs: 0,
      totalDirs: 0,
    });
  });

  it("splits files and directories into separate counts, directories never contributing bytes", () => {
    const entries = [
      entry("a.txt", { bytes: 100 }),
      entry("b.txt", { bytes: 200 }),
      entry("Tools", { is_dir: true, bytes: 0 }),
      entry("assets", { is_dir: true, bytes: 0 }),
      entry("assets2", { is_dir: true, bytes: 0 }),
    ];

    const counts = paneStatusCounts(entries, new Set());
    expect(counts.totalFiles).toBe(2);
    expect(counts.totalDirs).toBe(3);
    expect(counts.totalBytes).toBe(300);
  });

  it("counts only the selected names toward the selected totals", () => {
    const entries = [
      entry("a.txt", { bytes: 100 }),
      entry("b.txt", { bytes: 200 }),
      entry("Tools", { is_dir: true }),
    ];

    const counts = paneStatusCounts(entries, new Set(["b.txt", "Tools"]));
    expect(counts.selectedFiles).toBe(1);
    expect(counts.selectedBytes).toBe(200);
    expect(counts.selectedDirs).toBe(1);
    expect(counts.totalFiles).toBe(2);
    expect(counts.totalDirs).toBe(1);
  });

  it("ignores a selected name that names no entry in the pane", () => {
    const entries = [entry("a.txt", { bytes: 100 })];
    const counts = paneStatusCounts(entries, new Set(["stale-name"]));
    expect(counts.selectedFiles).toBe(0);
    expect(counts.selectedBytes).toBe(0);
  });
});
