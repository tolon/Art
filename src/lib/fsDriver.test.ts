import { describe, expect, it } from "vitest";

import { driverFileName, driverRequirement, fileSystemInputsFor } from "@/lib/fsDriver";

describe("driverRequirement", () => {
  it("asks for nothing on a Fast File System disk", () => {
    // Kickstart already has it. Demanding a driver here would send the user
    // hunting for a file that does not need to exist.
    expect(driverRequirement("ffsstandard").required).toBe(false);
    expect(driverRequirement("ffsdircache").required).toBe(false);
  });

  it("asks for one on every filesystem Kickstart does not have", () => {
    expect(driverRequirement("pfs3directscsi")).toMatchObject({
      required: true,
      dosType: "PDS3",
    });
    expect(driverRequirement("pfs3standard")).toMatchObject({
      required: true,
      dosType: "PFS3",
    });
    expect(driverRequirement("sfs0")).toMatchObject({ required: true, dosType: "SFS0" });
  });
});

describe("fileSystemInputsFor", () => {
  it("embeds the driver the chosen filesystem needs", () => {
    const inputs = fileSystemInputsFor("pfs3directscsi", "F:\\drivers\\pfs3aio");
    expect(inputs).toEqual([{ path: "F:\\drivers\\pfs3aio", dos_type: "PDS3" }]);
  });

  it("states no version, leaving ART to read it from the driver", () => {
    // A number retyped from a readme is wrong more often than the binary is,
    // and a wrong one is not cosmetic: AmigaOS keeps the higher of the RDB's
    // version and the loaded one, so 0.0 means the driver is never used.
    const [input] = fileSystemInputsFor("sfs0", "F:\\SmartFilesystem");
    expect(input.version).toBeUndefined();
    expect(input.revision).toBeUndefined();
  });

  it("embeds nothing when the user has not picked a driver yet", () => {
    // The wizard warns instead — an image with no driver is still a useful
    // thing to make, it just will not mount on a real Amiga until one is added.
    expect(fileSystemInputsFor("pfs3directscsi", null)).toEqual([]);
  });

  it("does not embed a driver into a disk that has no use for it", () => {
    // A PFS3 driver on an FFS disk claims a DosType no partition asks for and
    // costs blocks in the reserved area for nothing.
    expect(fileSystemInputsFor("ffsstandard", "F:\\drivers\\pfs3aio")).toEqual([]);
  });
});

describe("driverFileName", () => {
  it("shows the file, not the path it came from", () => {
    expect(driverFileName("F:\\art-sd0\\driver\\pfs3aio")).toBe("pfs3aio");
    expect(driverFileName("/home/me/pfs3aio")).toBe("pfs3aio");
    expect(driverFileName("pfs3aio")).toBe("pfs3aio");
  });
});
