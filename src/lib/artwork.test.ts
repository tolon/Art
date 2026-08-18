import { describe, expect, it } from "vitest";

import { isSupportedPicture } from "./artwork";

describe("isSupportedPicture", () => {
  // The same cases `picture_extension`'s Rust test asserts, so the two gates
  // cannot silently drift apart.
  it("accepts PNG and JPEG, case-insensitively, and nothing else", () => {
    expect(isSupportedPicture("cover.png")).toBe(true);
    expect(isSupportedPicture("cover.PNG")).toBe(true);
    expect(isSupportedPicture("cover.jpg")).toBe(true);
    expect(isSupportedPicture("cover.jpeg")).toBe(true);
    expect(isSupportedPicture("cover.iff")).toBe(false);
    expect(isSupportedPicture("cover")).toBe(false);
  });

  it("looks at the file name, not the whole path", () => {
    expect(isSupportedPicture("C:\\Users\\me\\Pictures\\cover.png")).toBe(true);
    expect(isSupportedPicture("/home/me/pictures/cover.jpeg")).toBe(true);
    expect(isSupportedPicture("/home/me.png/cover")).toBe(false);
  });

  it("a dotfile with nothing before the dot has no extension", () => {
    expect(isSupportedPicture(".png")).toBe(false);
  });
});
