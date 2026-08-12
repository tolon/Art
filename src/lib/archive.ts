// Typed wrappers for the archive (LHA/ZIP/7z) commands. Mirrors
// src-tauri/src/commands/archive.rs.
//
// An archive is read-only end to end: there is no write/rename/delete/mkdir
// wrapper here, ever — the same rule `@/lib/iso` follows for a disc. The two
// directions that exist both write somewhere that is not the archive.
//
// A folder inside an archive is addressed by its path (`"Tools/Sub"`, `""`
// for the root) rather than by a block or an extent, because an archive has
// no directories of its own: the folders exist because the entry names say
// so. `dir` and `name` are always passed separately and joined in Rust — a
// name out of an archive is a name ART did not write.

import { invoke } from "@tauri-apps/api/core";

import type { ExtractedTo, PanelEntry } from "@/lib/panel";
import type { CopyOptions } from "@/lib/volumeWrite";

/** What opening an archive reports — including what it cannot show. */
export interface ArchiveInfo {
  /** `"lha"`, `"zip"` or `"7z"`, from the file's own bytes. */
  format: string;
  entry_count: number;
  total_bytes: number;
  /**
   * Entries whose names are not plain relative paths (`../escape`, `C:\…`),
   * so the pane cannot show them as anything to walk into. Counted rather
   * than hidden: a listing that shows seven of ten entries has to say so.
   */
  unusable_names: number;
  /** Entries dropped because another had claimed that name already. */
  duplicates: number;
}

/** Open an archive and report enough to show the pane. */
export async function archiveOpen(path: string): Promise<ArchiveInfo> {
  return invoke<ArchiveInfo>("archive_open", { path });
}

/**
 * List one folder of an archive. `dir` is `""` for the root, or a path from a
 * previous listing. A folder that is not there is an error rather than an
 * empty listing — empty and missing must not look the same.
 */
export async function archiveList(path: string, dir: string): Promise<PanelEntry[]> {
  return invoke<PanelEntry[]>("archive_list", { path, dir });
}

/**
 * Copy one file out of an archive to a local folder — the single-entry fast
 * path of F5, synchronous, the same asymmetry `volumeExtractTo` and
 * `isoExtractFile` give a volume and a disc.
 */
export async function archiveExtractFile(
  path: string,
  dir: string,
  name: string,
  destDir: string,
  overwrite?: boolean
): Promise<ExtractedTo> {
  return invoke<ExtractedTo>("archive_extract_file", {
    path,
    dir,
    name,
    destDir,
    overwrite: overwrite ?? null,
  });
}

/**
 * F5 out of an archive, to a local folder. The folder `dir`/`name` lands as a
 * folder called `name` inside `destDir`, keeping its own shape. Returns a job
 * id; the result arrives on `onVolumeWriteResult` as an `archive_out` result.
 */
export async function archiveExtract(
  path: string,
  dir: string,
  name: string,
  destDir: string,
  options?: CopyOptions
): Promise<number> {
  return invoke<number>("archive_extract", {
    path,
    dir,
    name,
    destDir,
    options: options ?? null,
  });
}

/**
 * F5 out of an archive, the other way — into an Amiga volume. The chosen
 * folder (or single file) is unpacked to a scratch directory and copied in
 * through the Stage W writer, which is what installing a downloaded package
 * already does. Returns a job id, reporting as a `copy_in` result.
 */
export async function archiveCopyToVolume(
  archivePath: string,
  dir: string,
  name: string,
  path: string,
  volumeIndex: number,
  dirBlock: number | null,
  options?: CopyOptions
): Promise<number> {
  return invoke<number>("archive_copy_to_volume", {
    archivePath,
    dir,
    name,
    path,
    volumeIndex,
    dirBlock,
    options: options ?? null,
  });
}
