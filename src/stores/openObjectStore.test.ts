// The store behind ART-085, on its own terms. The half that proves the actual
// defect — that leaving a screen used to lose the file — needs a mounted
// component and lives in `openObjectSurvivesNavigation.test.tsx`.

import { beforeEach, describe, expect, it } from "vitest";

import { resetOpenObjects, useOpenObjectStore } from "./openObjectStore";

beforeEach(resetOpenObjects);

describe("what ART has open", () => {
  it("is nothing at all to begin with", () => {
    expect(useOpenObjectStore.getState().open.adf ?? null).toBeNull();
  });

  it("keeps one entry per kind, so opening an ADF leaves the HDF alone", () => {
    const { setOpen } = useOpenObjectStore.getState();
    setOpen("harddisk", "C:\\work\\system.hdf");
    setOpen("adf", "C:\\disks\\df0.adf");

    const { open } = useOpenObjectStore.getState();
    expect(open.adf).toBe("C:\\disks\\df0.adf");
    // The studios address different kinds of thing. One global "current object"
    // would mean opening a floppy changed what the Hard Disk studio was looking
    // at, which is a different bug wearing this fix's clothes.
    expect(open.harddisk).toBe("C:\\work\\system.hdf");
  });

  it("closes a kind when it is given nothing", () => {
    const { setOpen } = useOpenObjectStore.getState();
    setOpen("lha", "C:\\dl\\game.lha");
    setOpen("lha", null);

    expect(useOpenObjectStore.getState().open.lha).toBeNull();
  });

  it("replaces rather than accumulates when a second file is opened", () => {
    const { setOpen } = useOpenObjectStore.getState();
    setOpen("adf", "C:\\disks\\one.adf");
    setOpen("adf", "C:\\disks\\two.adf");

    expect(useOpenObjectStore.getState().open.adf).toBe("C:\\disks\\two.adf");
  });
});
