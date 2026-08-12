import { describe, expect, it } from "vitest";

import { parseCommandLine } from "@/lib/commandLine";

describe("the commander's command line", () => {
  it("does nothing with nothing", () => {
    expect(parseCommandLine("")).toEqual({ kind: "none" });
    expect(parseCommandLine("   ")).toEqual({ kind: "none" });
  });

  it("goes up on `cd ..`, `cd..` and a bare `..`", () => {
    for (const text of ["cd ..", "cd..", "CD ..", "..", "  cd  ..  "]) {
      expect(parseCommandLine(text)).toEqual({ kind: "up" });
    }
  });

  it("goes up on a bare `cd`, the way a shell would", () => {
    expect(parseCommandLine("cd")).toEqual({ kind: "up" });
  });

  it("opens an absolute path, with or without `cd`", () => {
    expect(parseCommandLine("D:\\Amiga\\Games")).toEqual({
      kind: "open",
      path: "D:\\Amiga\\Games",
    });
    expect(parseCommandLine("cd D:/Amiga")).toEqual({ kind: "open", path: "D:/Amiga" });
    expect(parseCommandLine("\\\\nas\\amiga")).toEqual({ kind: "open", path: "\\\\nas\\amiga" });
    expect(parseCommandLine("/media/sd")).toEqual({ kind: "open", path: "/media/sd" });
  });

  it("refuses a relative path rather than joining strings", () => {
    // A pane can be showing the inside of an ADF, where "the current
    // directory plus a name" is not a path at all — so this asks for a full
    // one instead of guessing at what `Games` meant.
    expect(parseCommandLine("cd Games")).toEqual({
      kind: "refused",
      reason: { key: "files.commandLine.refuseRelative" },
    });
  });

  it("treats a wildcard as a filter mask", () => {
    expect(parseCommandLine("*.adf")).toEqual({ kind: "filter", mask: "*.adf" });
    expect(parseCommandLine("Lotus?.lha")).toEqual({ kind: "filter", mask: "Lotus?.lha" });
  });

  it("lets an explicit `cd` win over a wildcard", () => {
    // `cd *backup*` is someone navigating, badly — answering with "filter
    // set" would be a different thing happening from the one that was asked
    // for.
    expect(parseCommandLine("cd *backup*")).toEqual({
      kind: "refused",
      reason: { key: "files.commandLine.refuseRelative" },
    });
  });

  it("refuses anything else by name — it is not a shell, and says so", () => {
    // §56: ART does not run what a user types. A box that swallowed the
    // keystroke silently would read as ART having crashed.
    for (const text of ["dir", "format c:", "winuae", "notepad readme.txt"]) {
      expect(parseCommandLine(text)).toEqual({
        kind: "refused",
        reason: { key: "files.commandLine.refuseNotAShell" },
      });
    }
  });
});
