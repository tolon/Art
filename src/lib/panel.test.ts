import { describe, expect, it } from "vitest";

import { dirSizeCell, dirSizeKey, type PanelEntry } from "@/lib/panel";

function entry(over: Partial<PanelEntry>): PanelEntry {
  return {
    name: "Games",
    is_dir: true,
    bytes: 0,
    path: null,
    header_block: null,
    iso_extent: null,
    is_link: false,
    date: null,
    attrs: null,
    ...over,
  };
}

/**
 * ART-087. Space on a directory marks it *and* counts it, and the count has to
 * land on the row that asked for it — in the pane that asked.
 */
describe("dirSizeKey", () => {
  it("keys a local row by its path, per pane", () => {
    const row = entry({ path: "E:\\Games" });
    expect(dirSizeKey("left", row)).toBe("left|E:\\Games");
    expect(dirSizeKey("right", row)).not.toBe(dirSizeKey("left", row));
  });

  it("keys a volume row by its header block", () => {
    expect(dirSizeKey("left", entry({ header_block: 880 }))).toBe("left|block:880");
  });

  it("refuses a file: only a drawer has a size to count", () => {
    expect(dirSizeKey("left", entry({ is_dir: false, path: "E:\\a.adf" }))).toBeNull();
  });

  it("refuses a row no command can count", () => {
    // An ISO or archive row has neither a host path nor a header block, and
    // there is no command that counts one. A key here would leave the row
    // saying "counting…" for a job that never starts.
    expect(dirSizeKey("left", entry({ iso_extent: 24 }))).toBeNull();
  });

  it("does not confuse block 0 with no block at all", () => {
    expect(dirSizeKey("left", entry({ header_block: 0 }))).toBe("left|block:0");
  });
});

describe("dirSizeCell", () => {
  it("shows <DIR> until somebody asks", () => {
    expect(dirSizeCell(undefined)).toEqual({ kind: "dir" });
  });

  it("has a third state while the job runs", () => {
    // The whole reason the column needs three states rather than two: a
    // drawer of forty thousand files is not counted instantly, and a blank
    // or a stale `<DIR>` would read as nothing having happened.
    expect(dirSizeCell({ status: "counting" })).toEqual({ kind: "counting" });
  });

  it("carries the real total once it lands", () => {
    expect(
      dirSizeCell({
        status: "done",
        total: { bytes: 1234, files: 9, directories: 2, partial: false },
      })
    ).toEqual({ kind: "counted", bytes: 1234, partial: false });
  });

  it("keeps `partial` so a floor is never printed as an answer", () => {
    // `core::dirsize` stops at its depth cap and skips what it cannot read.
    // A number shown as the total when it is a floor is worse than no number
    // — the same rule ART-107 settled for the layout scan.
    expect(
      dirSizeCell({
        status: "done",
        total: { bytes: 7, files: 1, directories: 0, partial: true },
      })
    ).toEqual({ kind: "counted", bytes: 7, partial: true });
  });
});
