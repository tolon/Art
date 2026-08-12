import { describe, expect, it } from "vitest";

import { partitionsMissingDriver } from "@/lib/rdbDrivers";
import type { HdfInfo, ParsedFileSystem, ParsedPartition } from "@/lib/hdf";

const PDS3 = 0x50445303;
const SFS0 = 0x53465300;
const DOS1 = 0x444f5301;
const DOS3 = 0x444f5303;

function partition(name: string, dostype: number): ParsedPartition {
  return {
    drive_name: name,
    dostype,
    dostype_str: "x",
    fs_type: "ffsstandard",
    low_cyl: 2,
    high_cyl: 100,
    cylinder_count: 99,
    size_bytes: 1024,
    bootable: true,
    boot_priority: 0,
    num_buffers: 30,
    block_location: 1,
    next_part_block: 0,
    checksum_valid: true,
  };
}

function driver(dosType: number): ParsedFileSystem {
  return {
    dos_type: dosType,
    dos_type_str: "x",
    version: 19,
    revision: 2,
    seg_list_block: 3,
    segment_blocks: 121,
    size_bytes: 59120,
    checksum_valid: true,
    truncated: false,
  };
}

function disk(
  partitions: ParsedPartition[],
  file_systems: ParsedFileSystem[],
  hdf_type: "rdb" | "plain" = "rdb"
): HdfInfo {
  return {
    path: "F:\\art-sd0\\sd0-test.img",
    total_bytes: 67108864,
    hdf_type,
    cylinders: 130,
    heads: 16,
    sectors: 63,
    block_size: 512,
    partitions,
    file_systems,
    free_bytes: 0,
    rdb_checksum_valid: true,
  };
}

describe("partitionsMissingDriver", () => {
  it("says nothing about an ordinary Fast File System disk", () => {
    // DOS\0 … DOS\7 are in Kickstart. Nothing has to be in the RDB for them,
    // so an FFS disk with no file systems listed is correct, not broken.
    expect(partitionsMissingDriver(disk([partition("DH0", DOS1)], []))).toEqual([]);
    expect(partitionsMissingDriver(disk([partition("DH0", DOS3)], []))).toEqual([]);
  });

  it("says nothing about a PFS3 disk that carries its driver", () => {
    // The image `hst-imager` builds. This is the shape that must stay quiet,
    // or the warning becomes noise and gets ignored when it matters.
    const info = disk([partition("DH0", PDS3)], [driver(PDS3)]);
    expect(partitionsMissingDriver(info)).toEqual([]);
  });

  it("names the partition that will not mount", () => {
    // The image ART's own New HDF wizard produces today (ART-084): a PDS3
    // partition and an empty RDB behind it. An Amiga ignores it silently.
    const missing = partitionsMissingDriver(disk([partition("DH0", PDS3)], []));
    expect(missing.map((p) => p.drive_name)).toEqual(["DH0"]);
  });

  it("checks each partition against its own DosType, not the first one", () => {
    // A disk can carry PFS3 and still have an SFS partition nothing provides.
    const info = disk(
      [partition("DH0", PDS3), partition("DH1", SFS0), partition("DH2", DOS3)],
      [driver(PDS3)]
    );
    expect(partitionsMissingDriver(info).map((p) => p.drive_name)).toEqual(["DH1"]);
  });

  it("stays silent about a plain hardfile", () => {
    // No RDB means nowhere for a driver to live and one volume for the whole
    // file — "no driver found" would be answering a question nobody asked.
    const info = disk([partition("DH0", PDS3)], [], "plain");
    expect(partitionsMissingDriver(info)).toEqual([]);
  });

  it("does not mistake a DosType that merely starts with DOS for a built-in", () => {
    // `DOS\8` and up are not Kickstart's; only 0–7 are.
    const eight = 0x444f5308;
    expect(partitionsMissingDriver(disk([partition("DH0", eight)], []))).toHaveLength(1);
  });
});
