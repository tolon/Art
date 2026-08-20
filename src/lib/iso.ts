// Typed wrappers for the disc (ISO9660/Joliet) commands. Mirrors
// src-tauri/src/commands/iso.rs.
//
// A disc is read-only end to end: there is no write/rename/delete/mkdir
// wrapper here, ever. The two directions that exist both write somewhere
// that is not the disc — `isoExtract` to a local folder, `isoCopyToVolume`
// into an Amiga volume — through the same job events `@/lib/volumeWrite`
// already listens for.

import { invoke } from "@tauri-apps/api/core";

import type { ExtractedTo, PanelEntry } from "@/lib/panel";
import type { CopyOptions } from "@/lib/volumeWrite";

/** What opening a disc reports: enough to show the pane and start walking it. */
export interface IsoInfo {
  volume_name: string;
  joliet: boolean;
  /**
   * The root directory's `(extent, length)`. An ISO directory is addressed
   * by this pair, not by a block number — there is nothing here to overload
   * onto `dirBlock`.
   */
  root_extent: number;
  root_length: number;
}

/**
 * Open a disc.
 *
 * `formatHint` is `Detection.format_hint` when the pane was reached from the
 * drop panel, which already knows the sector layout — passing it skips
 * probing the file for `CD001` a second time. Leave it out for a disc opened
 * any other way (a saved path, a dialog pick); ART probes for itself.
 */
export async function isoOpen(path: string, formatHint?: string | null): Promise<IsoInfo> {
  return invoke<IsoInfo>("iso_open", { path, formatHint: formatHint ?? null });
}

/**
 * List one directory of a disc as panel rows.
 *
 * `extent`/`length` come from `IsoInfo.root_extent`/`root_length` or from a
 * previous listing's directory entry (`PanelEntry.iso_extent`, `bytes`).
 */
export async function isoList(path: string, extent: number, length: number): Promise<PanelEntry[]> {
  return invoke<PanelEntry[]>("iso_list", { path, extent, length });
}

/**
 * Copy one file out of a disc to a local folder — the single-entry fast path
 * of F5, the same asymmetry `volumeExtractTo`/`volumeCopyOut` give an ADF or
 * HDF: a lone file copies straight through, synchronously, and only a whole
 * directory (`isoExtract`, below) needs a job. `name` is the entry's own
 * name from the listing that found it (`PanelEntry.name`).
 *
 * `dirExtent`/`dirLength` are the pane's *open directory* — not the file's.
 * A file's Amiga protection bits and comment live in its directory record,
 * so Rust re-reads that record to write the `.uaem` sidecar beside the copy
 * (ART-078). They are the address of the record, never the bits themselves:
 * a protection byte sent from here would be one Rust did not verify.
 */
export async function isoExtractFile(
  path: string,
  extent: number,
  bytes: number,
  name: string,
  destDir: string,
  overwrite?: boolean,
  dirExtent?: number | null,
  dirLength?: number | null
): Promise<ExtractedTo> {
  return invoke<ExtractedTo>("iso_extract_file", {
    path,
    extent,
    bytes,
    name,
    destDir,
    overwrite: overwrite ?? null,
    dirExtent: dirExtent ?? null,
    dirLength: dirLength ?? null,
  });
}

/**
 * F5 out of a disc, to a local folder. `extent`/`bytes` name a directory the
 * same way `isoList` does; its contents land in a folder called `name` inside
 * `destDir`. The two are passed separately and joined in Rust, by
 * `folder_destination` — a name off a disc is a name ART did not write, and a
 * caller that concatenated it into the destination first would be handing a
 * security boundary a path it can no longer check.
 *
 * `options.overwrite` is the same collision setting an ADF copied out obeys.
 * Returns a job id — the result arrives on `onVolumeWriteResult`
 * (`@/lib/volumeWrite`) as a `copy_out` result, the same event an HDF/ADF
 * extraction reports on.
 */
export async function isoExtract(
  path: string,
  extent: number,
  bytes: number,
  destDir: string,
  name: string,
  options?: CopyOptions
): Promise<number> {
  return invoke<number>("iso_extract", {
    path,
    extent,
    bytes,
    destDir,
    name,
    options: options ?? null,
  });
}

/**
 * F5 out of a disc, the other way — into an Amiga volume. This is the whole
 * point of the feature: an AmigaOS install CD is only useful once its
 * contents reach an HDF. Returns a job id, reporting on `onVolumeWriteResult`
 * as a `copy_in` result, the same as `volumeCopyIn`.
 *
 * `name`/`isDir`/`date` are the picked row's own fields (`PanelEntry`). They
 * decide what is copied: a directory carries its whole subtree, a file
 * carries exactly itself — without them a single selected file could only be
 * copied as the directory around it, which on an install CD is the disc.
 *
 * `dirExtent`/`dirLength` are the *source* pane's open directory — the
 * address of the directory record the picked file sits in, which is where its
 * Amiga protection bits and comment are (ART-078). `dirBlock` is the
 * destination's; the two are unrelated and deliberately not one field.
 */
export async function isoCopyToVolume(
  isoPath: string,
  extent: number,
  bytes: number,
  name: string,
  isDir: boolean,
  date: number | null,
  path: string,
  volumeIndex: number,
  dirBlock: number | null,
  options?: CopyOptions,
  dirExtent?: number | null,
  dirLength?: number | null
): Promise<number> {
  return invoke<number>("iso_copy_to_volume", {
    isoPath,
    extent,
    bytes,
    name,
    isDir,
    date,
    path,
    volumeIndex,
    dirBlock,
    dirExtent: dirExtent ?? null,
    dirLength: dirLength ?? null,
    options: options ?? null,
  });
}
