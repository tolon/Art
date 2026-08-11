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
  /** Last-modified time, Unix seconds. `null` when the source has none. */
  date: number | null;
  /**
   * The Attr column: `rahs`-shape (Windows attributes) for a local file,
   * `hsparwed`-shape (Amiga protection bits) for an ADF/HDF entry — already
   * formatted on the Rust side by `core::volume::write::uaem::format_bits`,
   * the same function `AttributesDialog` uses. `null` only on a non-Windows
   * build listing a local folder.
   */
  attrs: string | null;
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
    date: entry.unix_date,
    attrs: entry.attrs,
  }));
}

export function formatBytes(bytes: number): string {
  if (bytes >= 1024 * 1024) return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
  if (bytes >= 1024) return `${Math.round(bytes / 1024)} KB`;
  return `${bytes} B`;
}
