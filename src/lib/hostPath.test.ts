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
});
