import { describe, expect, it } from "vitest";

import {
  asksWhatItIs,
  containerBreadcrumb,
  containerFor,
  type FormatCategory,
} from "@/lib/containerStep";

describe("containerFor", () => {
  it("maps every category that has a pane to that pane", () => {
    expect(containerFor("floppy-image")).toBe("adf");
    expect(containerFor("harddisk-image")).toBe("hdf");
    expect(containerFor("optical-image")).toBe("iso");
    expect(containerFor("archive")).toBe("archive");
    expect(containerFor("commodore-8bit")).toBe("c64");
  });

  it("maps everything else to nothing, ROM included", () => {
    // A ROM is a real detection and deliberately not a container: there is
    // nothing inside it to list, so Enter on one must not open an empty pane.
    for (const category of ["rom", "directory", "unknown"] as FormatCategory[]) {
      expect(containerFor(category), category).toBeNull();
    }
  });
});

describe("asksWhatItIs", () => {
  const file = { is_dir: false, path: "D:\\amiga\\Lotus.adf" };

  it("asks about a file in a host folder", () => {
    expect(asksWhatItIs(file, "local")).toBe(true);
  });

  it("never asks about a directory — the pane already knows how to walk in", () => {
    expect(asksWhatItIs({ is_dir: true, path: "D:\\amiga" }, "local")).toBe(false);
  });

  it("never asks about a row inside a container", () => {
    // ART opens an image by path, and a file inside an ADF, an archive, a
    // disc or a Commodore image has none. Extracting it somewhere temporary
    // so a pane could enter it would be a copy the user never asked for and
    // never sees; F5 is the answer, and it is already there.
    for (const kind of ["adf", "hdf", "iso", "archive", "c64"] as const) {
      expect(asksWhatItIs({ is_dir: false, path: null }, kind), kind).toBe(false);
      // Even if some future listing did carry a path, the pane kind alone
      // settles it — there is no route from inside a container to a reader.
      expect(asksWhatItIs(file, kind), kind).toBe(false);
    }
  });

  it("never asks about a row with no path at all", () => {
    expect(asksWhatItIs({ is_dir: false, path: null }, "local")).toBe(false);
    expect(asksWhatItIs({ is_dir: false, path: "" }, "local")).toBe(false);
  });
});

describe("containerBreadcrumb", () => {
  const lotus = { path: "E:\\amiga\\Games", name: "Lotus.adf" };

  it("writes the container step as though it were a folder", () => {
    // Total Commander's own convention, and the point of the feature: an
    // image *is* a folder as far as walking around is concerned.
    expect(containerBreadcrumb(lotus, "E:\\amiga\\Games\\Lotus.adf", ["Data"])).toEqual([
      "E:\\amiga\\Games\\Lotus.adf",
      "Data",
    ]);
  });

  it("keeps a POSIX path POSIX and a Windows path Windows", () => {
    expect(containerBreadcrumb({ path: "/media/sd", name: "work.hdf" }, "", [])).toEqual([
      "/media/sd/work.hdf",
    ]);
    expect(containerBreadcrumb({ path: "E:\\", name: "work.hdf" }, "", [])).toEqual([
      "E:\\work.hdf",
    ]);
  });

  it("leads with the pane's own location when it was not entered from a folder", () => {
    // An image opened straight from the source combo has no host to return to
    // — and its `location` is the same string the host join would have built.
    expect(containerBreadcrumb(null, "D:\\work.hdf", ["DH0", "Games"])).toEqual([
      "D:\\work.hdf",
      "DH0",
      "Games",
    ]);
  });

  it("drops empty interior steps rather than rendering a stray separator", () => {
    // An archive at its root has `archiveDir === ""`, which is a position, not
    // a name.
    expect(containerBreadcrumb({ path: "D:\\dl", name: "game.lha" }, "", [""])).toEqual([
      "D:\\dl\\game.lha",
    ]);
  });
});
