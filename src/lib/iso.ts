// Typed wrappers for the disc (ISO9660/Joliet) commands. Mirrors
// src-tauri/src/commands/iso.rs.
//
// A disc is read-only end to end: there is no write/rename/delete/mkdir
// wrapper here, ever. The two directions that exist both write somewhere
// that is not the disc — `isoExtract` to a local folder, `isoCopyToVolume`
// into an Amiga volume — through the same job events `@/lib/volumeWrite`
// already listens for.

import { invoke } from "@tauri-apps/api/core";

import type { PanelEntry } from "@/lib/panel";
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
 * F5 out of a disc, to a local folder. `extent`/`bytes` name a directory the
 * same way `isoList` does; its contents land inside `dest`, not a folder
 * named after it. Returns a job id — the result arrives on
 * `onVolumeWriteResult` (`@/lib/volumeWrite`) as a `copy_out` result, the
 * same event an HDF/ADF extraction reports on.
 */
export async function isoExtract(
  path: string,
  extent: number,
  bytes: number,
  dest: string
): Promise<number> {
  return invoke<number>("iso_extract", { path, extent, bytes, dest });
}

/**
 * F5 out of a disc, the other way — into an Amiga volume. This is the whole
 * point of the feature: an AmigaOS install CD is only useful once its
 * contents reach an HDF. Returns a job id, reporting on `onVolumeWriteResult`
 * as a `copy_in` result, the same as `volumeCopyIn`.
 */
export async function isoCopyToVolume(
  isoPath: string,
  extent: number,
  bytes: number,
  path: string,
  volumeIndex: number,
  dirBlock: number | null,
  options?: CopyOptions
): Promise<number> {
  return invoke<number>("iso_copy_to_volume", {
    isoPath,
    extent,
    bytes,
    path,
    volumeIndex,
    dirBlock,
    options: options ?? null,
  });
}
