// Volumes inside an image file. Mirrors src-tauri/src/commands/volume.rs.
//
// An .hdf is either a partitioned disk, a single bare volume, or something ART
// cannot identify. Every partition is always listed — with its contents when
// ART can read the filesystem, and with the exact reason when it cannot.
//
// Read-only: Stage R browses, Stage W writes.

import { invoke } from "@tauri-apps/api/core";

import type { ExtractedTo, PanelEntry } from "@/lib/panel";

export type ImageLayout = "rdb" | "bare_volume" | "unknown";

export interface VolumeEntry {
  /** `DH0`, or the file's own name for a bare volume. */
  name: string;
  /** `FFS INTL`, `PFS3`, … */
  filesystem: string;
  dos_type: number[];
  byte_offset: number;
  byte_length: number;
  block_size: number;
  reserved: number;
  bootable: boolean;
  boot_priority: number;
  /** Null when ART can open it; otherwise why it can only be listed. */
  unsupported: string | null;
  /** The partition claims more space than the file holds. */
  clamped: boolean;
}

export interface ImageVolumes {
  layout: ImageLayout;
  volumes: VolumeEntry[];
  /** Something worth saying beyond the list. */
  note: string | null;
}

export interface VolumeListing {
  volume_name: string;
  root_block: number;
  total_blocks: number;
  block_size: number;
  filesystem: string;
  entries: PanelEntry[];
  /** Health findings. On a read-only mount these are warnings, not failures. */
  warnings: string[];
}

/** What is inside an image file. */
export async function volumeScan(path: string): Promise<ImageVolumes> {
  return invoke<ImageVolumes>("volume_scan", { path });
}

/** List a directory inside one volume. `dirBlock` null means the root. */
export async function volumeList(
  path: string,
  volumeIndex: number,
  dirBlock: number | null
): Promise<VolumeListing> {
  return invoke<VolumeListing>("volume_list", {
    path,
    volumeIndex,
    dirBlock,
  });
}

/** Copy one file out of a volume into a local folder. */
export async function volumeExtractTo(
  path: string,
  volumeIndex: number,
  headerBlock: number,
  destDir: string,
  overwrite = false
): Promise<ExtractedTo> {
  return invoke<ExtractedTo>("volume_extract_to", {
    path,
    volumeIndex,
    headerBlock,
    destDir,
    overwrite,
  });
}

export function isMountable(volume: VolumeEntry): boolean {
  return volume.unsupported === null;
}

/** How to describe an image whose layout ART could not identify. */
export function describeLayout(layout: ImageLayout): string {
  switch (layout) {
    case "rdb":
      return "partitioned disk";
    case "bare_volume":
      return "single volume";
    case "unknown":
      return "not recognised";
  }
}
