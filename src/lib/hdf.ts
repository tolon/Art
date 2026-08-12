// Typed wrappers for Hard Disk File (HDF) and RDB commands.

import { invoke } from "@tauri-apps/api/core";

export type AmigaHardDiskFs =
  | "pfs3directscsi"
  | "pfs3standard"
  | "sfs0"
  | "ffsdircache"
  | "ffsstandard";

export interface PartitionSpec {
  drive_name: string;
  fs_type: AmigaHardDiskFs;
  size_mb: number;
  bootable: boolean;
  boot_priority: number;
  num_buffers: number;
}

export interface ParsedPartition {
  drive_name: string;
  dostype: number;
  dostype_str: string;
  fs_type: AmigaHardDiskFs;
  low_cyl: number;
  high_cyl: number;
  cylinder_count: number;
  size_bytes: number;
  bootable: boolean;
  boot_priority: number;
  num_buffers: number;
  block_location: number;
  next_part_block: number;
  checksum_valid: boolean;
}

/**
 * One filesystem driver embedded in the RDB — mirrors `ParsedFileSystem`.
 *
 * PFS3 and SFS are not in Kickstart: a partition whose DosType names one
 * mounts only if the driver for it sits inside the RDB. A disk that names a
 * filesystem it does not carry is one the Amiga silently ignores, which is
 * the difference this type exists to let ART report.
 */
export interface ParsedFileSystem {
  dos_type: number;
  dos_type_str: string;
  version: number;
  revision: number;
  seg_list_block: number;
  segment_blocks: number;
  size_bytes: number;
  checksum_valid: boolean;
  /** The LSEG chain could not be followed to its end. */
  truncated: boolean;
}

export interface HdfInfo {
  path: string;
  total_bytes: number;
  hdf_type: "rdb" | "plain";
  cylinders: number;
  heads: number;
  sectors: number;
  block_size: number;
  partitions: ParsedPartition[];
  /** Empty is normal for a disk using only what Kickstart already has. */
  file_systems: ParsedFileSystem[];
  free_bytes: number;
  rdb_checksum_valid: boolean;
}

/**
 * A driver to embed while creating the image — the writing half of the above.
 *
 * The **path** travels, not the bytes: the Rust side reads the file, so a
 * 60 KB driver never has to cross the bridge as a JSON array of numbers.
 * `dos_type` is written the way a person says it — `"PDS3"` — and the command
 * turns the last character into the digit **value** an Amiga expects.
 *
 * Leaving the version out is the normal case: ART reads it from the driver's
 * own `$VER:` string. Supplying one is for a driver that says nothing about
 * itself — there is no safe default, because AmigaOS keeps the higher of the
 * version in the disk and the one already loaded.
 */
export interface FileSystemInput {
  path: string;
  dos_type: string;
  version?: number;
  revision?: number;
}

export async function hdfOpen(path: string): Promise<HdfInfo> {
  return invoke<HdfInfo>("hdf_open", { path });
}

export async function hdfCreate(
  path: string,
  totalBytes: number,
  isRdb: boolean,
  partitions: PartitionSpec[],
  fileSystems: FileSystemInput[] = []
): Promise<HdfInfo> {
  return invoke<HdfInfo>("hdf_create", {
    path,
    totalBytes,
    isRdb,
    partitions,
    fileSystems,
  });
}
