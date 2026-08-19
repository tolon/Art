import { describe, expect, it } from "vitest";

import { hostParentDir } from "./hostPath";

describe("hostParentDir", () => {
  it("takes the folder off a Windows path", () => {
    expect(hostParentDir("E:\\amiga\\Amigatolon\\iso\\AmigaOS39.iso")).toBe(
      "E:\\amiga\\Amigatolon\\iso"
    );
  });

  it("takes the folder off a POSIX path", () => {
    expect(hostParentDir("/mnt/amiga/iso/AmigaOS39.iso")).toBe("/mnt/amiga/iso");
  });

  it("handles a path that mixes both separators", () => {
    expect(hostParentDir("E:\\amiga/iso\\AmigaOS39.iso")).toBe("E:\\amiga/iso");
  });

  // A drive root is its own parent — returning "E:" would name a *relative*
  // location on Windows, which is a different folder entirely.
  it("keeps the trailing separator on a drive root", () => {
    expect(hostParentDir("E:\\AmigaOS39.iso")).toBe("E:\\");
  });

  it("keeps the separator on a POSIX root", () => {
    expect(hostParentDir("/AmigaOS39.iso")).toBe("/");
  });

  it("has no answer for a bare name", () => {
    expect(hostParentDir("AmigaOS39.iso")).toBeNull();
  });

  it("has no answer for an empty string", () => {
    expect(hostParentDir("")).toBeNull();
  });

  it("takes the folder off a UNC path", () => {
    expect(hostParentDir("\\\\server\\share\\iso\\AmigaOS39.iso")).toBe(
      "\\\\server\\share\\iso"
    );
  });

  // A path that already names a folder (trailing separator) loses that
  // separator rather than climbing up a further level — there is nothing
  // after the last separator to cut off, so the folder is everything before
  // it, exactly as for any other path.
  it("drops a trailing separator instead of climbing a further level", () => {
    expect(hostParentDir("E:\\amiga\\iso\\")).toBe("E:\\amiga\\iso");
  });
});
