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

export interface HdfInfo {
  path: string;
  total_bytes: number;
  hdf_type: "rdb" | "plain";
  cylinders: number;
  heads: number;
  sectors: number;
  block_size: number;
  partitions: ParsedPartition[];
  free_bytes: number;
  rdb_checksum_valid: boolean;
}

export async function hdfOpen(path: string): Promise<HdfInfo> {
  return invoke<HdfInfo>("hdf_open", { path });
}

export async function hdfCreate(
  path: string,
  totalBytes: number,
  isRdb: boolean,
  partitions: PartitionSpec[]
): Promise<HdfInfo> {
  return invoke<HdfInfo>("hdf_create", {
    path,
    totalBytes,
    isRdb,
    partitions,
  });
}
