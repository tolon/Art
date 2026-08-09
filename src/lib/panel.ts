// The two-pane file manager. Mirrors src-tauri/src/commands/panel.rs.
//
// A pane can show a local folder, an ADF image or an HDF image. `PanelEntry` is
// the one row shape they all produce, so the table does not care which is which.

import { invoke } from "@tauri-apps/api/core";

import { adfList, type AdfEntry } from "@/lib/adf";

export interface PanelEntry {
  name: string;
  is_dir: boolean;
  bytes: number;
  /** Set for a local entry. */
  path: string | null;
  /** Set for an ADF entry. */
  header_block: number | null;
  /** Reported, never followed. */
  is_link: boolean;
}

export interface LocalListing {
  path: string;
  parent: string | null;
  entries: PanelEntry[];
  /** True when the folder held more than ART will list. */
  truncated: boolean;
}

export interface ExtractedTo {
  path: string;
  bytes: number;
  skipped_existing: boolean;
}

export async function panelListLocal(path: string): Promise<LocalListing> {
  return invoke<LocalListing>("panel_list_local", { path });
}

/** Drive letters on Windows, `/` elsewhere. */
export async function panelLocalRoots(): Promise<string[]> {
  return invoke<string[]>("panel_local_roots");
}

/**
 * Copy one file out of an ADF into a local folder.
 *
 * The bytes go host-side to host-side; nothing passes through the webview.
 */
export async function adfExtractTo(
  path: string,
  headerBlock: number,
  name: string,
  destDir: string,
  overwrite = false
): Promise<ExtractedTo> {
  return invoke<ExtractedTo>("adf_extract_to", {
    path,
    headerBlock,
    name,
    destDir,
    overwrite,
  });
}

/** List an ADF directory as panel rows. */
export async function panelListAdf(
  image: string,
  dirBlock: number | null
): Promise<PanelEntry[]> {
  const entries: AdfEntry[] = await adfList(image, dirBlock ?? undefined);
  return entries.map((entry) => ({
    name: entry.name,
    is_dir: entry.kind === "directory",
    bytes: entry.byte_size,
    path: null,
    header_block: entry.header_block,
    is_link: false,
  }));
}

/** One file inside a folder that is about to be copied. */
export interface PlannedCopy {
  source: string;
  /** Path relative to the folder being copied, with forward slashes. */
  relative: string;
  bytes: number;
}

export interface CopyPlan {
  files: PlannedCopy[];
  total_bytes: number;
  /** Folders to create first, parents before children. */
  directories: string[];
  /** What the walk refused, and why. */
  skipped: string[];
}

/**
 * Work out what copying a local folder would move, without moving anything.
 *
 * The copy itself runs file by file so a failure part-way leaves a partial
 * copy the user can see, rather than an unexplained error.
 */
export async function panelPlanFolderCopy(path: string): Promise<CopyPlan> {
  return invoke<CopyPlan>("panel_plan_folder_copy", { path });
}

export function formatBytes(bytes: number): string {
  if (bytes >= 1024 * 1024) return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
  if (bytes >= 1024) return `${Math.round(bytes / 1024)} KB`;
  return `${bytes} B`;
}
