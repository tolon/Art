// Typed wrappers for the Commodore 8-bit commands. Mirrors
// src-tauri/src/commands/cbm.rs.
//
// Read-only, like the disc and archive wrappers beside it: nothing here
// writes a D64, a D71, a D81 or a T64, ever.
//
// These media are **flat** — a 1541 directory has no subdirectories and
// neither does a T64 — so there is no `dir` parameter anywhere in this file.
// What `cbmList` returns is the whole image.

import { invoke } from "@tauri-apps/api/core";

import type { ExtractedTo, PanelEntry } from "@/lib/panel";
import type { CopyOptions } from "@/lib/volumeWrite";

/** What opening a Commodore image reports. */
export interface CbmInfo {
  /** `"d64"`, `"d71"`, `"d81"` or `"t64"`, from the file itself. */
  format: string;
  /** The disk's name, or a tape archive's container name. */
  volume_name: string;
  /** A disk's two-character id; empty for a tape. */
  disk_id: string;
  entry_count: number;
  /**
   * Things the image says about itself that ART had to work around, already
   * written as sentences. A T64 whose header count disagrees with its records
   * is the common one — common enough that saying nothing would be the
   * surprising choice.
   */
  notes: string[];
}

/** Open a Commodore disk or tape image. */
export async function cbmOpen(path: string): Promise<CbmInfo> {
  return invoke<CbmInfo>("cbm_open", { path });
}

/** Every file the image holds — there is only one listing. */
export async function cbmList(path: string): Promise<PanelEntry[]> {
  return invoke<PanelEntry[]>("cbm_list", { path });
}

/**
 * Copy one file out to a local folder. `name` is the row's own label, as
 * `cbmList` returned it. The Commodore file type becomes the extension on the
 * way out, so a folder of extracted files is readable.
 */
export async function cbmExtractFile(
  path: string,
  name: string,
  destDir: string,
  overwrite?: boolean
): Promise<ExtractedTo> {
  return invoke<ExtractedTo>("cbm_extract_file", {
    path,
    name,
    destDir,
    overwrite: overwrite ?? null,
  });
}

/**
 * F5 with several rows picked: all of them into `destDir`, as one job. An
 * empty `names` means the whole image.
 *
 * One job rather than one per file — the same gesture a volume's
 * multi-selection makes, but a Commodore image is small enough to walk once,
 * so one progress bar and one Stop cover the lot. Returns a job id; the
 * result arrives on `onVolumeWriteResult` as an `archive_out` result.
 */
export async function cbmExtract(
  path: string,
  names: string[],
  destDir: string,
  options?: CopyOptions
): Promise<number> {
  return invoke<number>("cbm_extract", {
    path,
    names,
    destDir,
    options: options ?? null,
  });
}
