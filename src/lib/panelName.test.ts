// Covers `splitName` directly — see its own comment, and `sort.test.ts`'s,
// for why a pure `src/lib` function gets its own test file instead of being
// exercised only through `FileManager.tsx`.
import { describe, expect, it } from "vitest";

import { splitName } from "./panelName";

describe("splitName", () => {
  it("splits an ordinary file on its dot", () => {
    expect(splitName("foo.txt", false)).toEqual({ stem: "foo", ext: "txt" });
  });

  it("splits on the last dot when there are several", () => {
    expect(splitName("archive.tar.gz", false)).toEqual({ stem: "archive.tar", ext: "gz" });
  });

  it("splits the file that motivated this column: the last extension only", () => {
    expect(splitName("MultibootOS128_2.2_65135ad.img.7z", false)).toEqual({
      stem: "MultibootOS128_2.2_65135ad.img",
      ext: "7z",
    });
  });

  it("treats a name with no dot as having no extension", () => {
    expect(splitName("README", false)).toEqual({ stem: "README", ext: "" });
  });

  it("treats a dotfile with no other dot as having no extension", () => {
    expect(splitName(".gitignore", false)).toEqual({ stem: ".gitignore", ext: "" });
  });

  it("keeps a trailing dot on the stem rather than producing an empty extension", () => {
    expect(splitName("archive.", false)).toEqual({ stem: "archive.", ext: "" });
  });

  it("a directory never gets an extension, even with dots in its name", () => {
    expect(splitName("Amiga PiStrom", true)).toEqual({ stem: "Amiga PiStrom", ext: "" });
    expect(splitName("v1.2.3-final", true)).toEqual({ stem: "v1.2.3-final", ext: "" });
    expect(splitName(".config", true)).toEqual({ stem: ".config", ext: "" });
  });

  it("an empty name has no extension", () => {
    expect(splitName("", false)).toEqual({ stem: "", ext: "" });
  });
});
