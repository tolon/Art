// Which filesystems need a driver inside the RDB, and how to name it.
//
// The reading half of this question lives in `@/lib/rdbDrivers`, which looks at
// a disk that already exists and reports the partitions nothing will mount.
// This is the half that runs *before* the disk exists, so the wizard can carry
// the driver in rather than produce the same broken image again (ART-084).

import type { AmigaHardDiskFs, FileSystemInput } from "@/lib/hdf";

export interface DriverRequirement {
  /** Kickstart has no driver for this DosType; the RDB must carry one. */
  required: boolean;
  /**
   * What the FSHD must claim, written the way a person says it. The command
   * turns the last character into the digit **value** an Amiga expects —
   * `PDS3` is `P`, `D`, `S`, `0x03`, not four printable characters.
   */
  dosType: string;
  /** The file usually named, shown as a hint. Nothing is enforced on it. */
  hint: string;
}

const REQUIREMENTS: Record<string, DriverRequirement> = {
  pfs3directscsi: { required: true, dosType: "PDS3", hint: "pfs3aio" },
  pfs3standard: { required: true, dosType: "PFS3", hint: "pfs3aio" },
  sfs0: { required: true, dosType: "SFS0", hint: "SmartFilesystem" },
};

/**
 * Whether a new disk using `fs` has to carry its own filesystem driver.
 *
 * `DOS\0`…`DOS\7` are in every Kickstart, so an FFS disk needs nothing. PFS3
 * and SFS are not, and a partition naming one on a disk that does not carry it
 * is a partition an Amiga ignores in silence.
 */
export function driverRequirement(fs: AmigaHardDiskFs): DriverRequirement {
  return REQUIREMENTS[fs] ?? { required: false, dosType: "", hint: "" };
}

/**
 * The drivers to embed while creating the image.
 *
 * Empty whenever there is nothing to embed *or* nothing to embed it for: a
 * PFS3 driver on an FFS disk would be dead weight the Amiga still has to load
 * past, and would claim a DosType no partition on the disk asks for.
 *
 * Version and revision are deliberately absent — ART reads them out of the
 * driver's own `$VER:` string, which is right more often than a person
 * retyping a number from a readme.
 */
export function fileSystemInputsFor(
  fs: AmigaHardDiskFs,
  driverPath: string | null
): FileSystemInput[] {
  const requirement = driverRequirement(fs);
  if (!requirement.required || !driverPath) return [];
  return [{ path: driverPath, dos_type: requirement.dosType }];
}

/** The short file name at the end of a path, for showing what was picked. */
export function driverFileName(path: string): string {
  const cut = Math.max(path.lastIndexOf("\\"), path.lastIndexOf("/"));
  return cut >= 0 ? path.slice(cut + 1) : path;
}
