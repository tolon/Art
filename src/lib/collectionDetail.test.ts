import { describe, expect, it } from "vitest";

import { canLaunch, diskList, mediaPhrase } from "./collectionDetail";
import type { Media } from "./gameindex";

const floppies: Media = { kind: "floppies", ordered: ["Dune2 Disk1.adf", "Dune2 Disk2.adf"] };
const hardfile: Media = { kind: "hardfile", file: "Agony.hdf" };
const drawer: Media = { kind: "whdload-drawer", slave: "Turrican.slave" };

describe("mediaPhrase", () => {
  it("counts the disks of a floppy set", () => {
    expect(mediaPhrase(floppies)).toEqual({
      key: "collection.detail.media.floppies",
      params: { count: 2 },
    });
  });

  it("names the slave of a WHDLoad drawer", () => {
    expect(mediaPhrase(drawer)).toEqual({
      key: "collection.detail.media.whdload",
      params: { slave: "Turrican.slave" },
    });
  });

  it("names the image of a hardfile", () => {
    expect(mediaPhrase(hardfile)).toEqual({
      key: "collection.detail.media.hardfile",
      params: { file: "Agony.hdf" },
    });
  });
});

describe("diskList", () => {
  it("keeps the order the catalogue recorded — it is the order the game asks for", () => {
    expect(diskList(floppies)).toEqual(["Dune2 Disk1.adf", "Dune2 Disk2.adf"]);
  });

  it("is empty for media that is not a disk set", () => {
    expect(diskList(hardfile)).toEqual([]);
    expect(diskList(drawer)).toEqual([]);
  });
});

describe("canLaunch", () => {
  it("says yes for every medium this wave launches", () => {
    expect(canLaunch(floppies)).toBe(true);
    expect(canLaunch(hardfile)).toBe(true);
    expect(canLaunch(drawer)).toBe(true);
  });
});
