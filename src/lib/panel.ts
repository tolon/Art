// The two-pane file manager. Mirrors src-tauri/src/commands/panel.rs.
//
// A pane can show a local folder, an ADF image or an HDF image. `PanelEntry` is
// the one row shape they all produce, so the table does not care which is which.

import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

import { adfList, type AdfEntry } from "@/lib/adf";

export interface PanelEntry {
  name: string;
  is_dir: boolean;
  bytes: number;
  /** Set for a local entry. */
  path: string | null;
  /** Set for an ADF entry. */
  header_block: number | null;
  /**
   * Set for an ISO entry: its starting logical block. Deliberately a
   * separate field from `header_block` — a disc's directory addressing is
   * `(extent, length)`, not a block number, and this alone is only half of
   * that (see `bytes` for a directory's length).
   */
  iso_extent: number | null;
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
    iso_extent: null,
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

// ---------------------------------------------------------------------------
// How big a drawer really is (ART-087)
// ---------------------------------------------------------------------------

/**
 * What a drawer holds, once counted. Mirrors `core::dirsize::DirTotal`.
 *
 * `partial` is the field that matters: the walk is depth-bounded and skips
 * what it cannot read, so when it is set `bytes` is a **floor**, not a total.
 * A Size column that printed it as the answer would be quietly wrong by
 * however much it did not look at, which is the same silence ART-107 was.
 */
export interface DirTotal {
  bytes: number;
  files: number;
  directories: number;
  partial: boolean;
}

/** The payload `dir-size-result` carries. */
export interface DirSizeResult {
  jobId: number;
  /** The path or block the count was asked about — see the Rust side. */
  key: string;
  total: DirTotal;
}

/**
 * Start counting a local folder. Returns the **job id**, not the answer: the
 * count runs on a job thread and the answer arrives on `dir-size-result`
 * (§54), so a folder of forty thousand files neither blocks the command
 * thread nor becomes unstoppable.
 */
export async function panelDirectorySize(path: string): Promise<number> {
  return invoke<number>("panel_directory_size", { path });
}

/** The same, for a directory inside a volume. `dirBlock` is the row's own
 *  `header_block`; `null` means the volume's root. */
export async function volumeDirectorySize(
  path: string,
  volumeIndex: number,
  dirBlock: number | null
): Promise<number> {
  return invoke<number>("volume_directory_size", { path, volumeIndex, dirBlock });
}

/** Listen for finished counts. Both commands above answer on this one event. */
export async function onDirSizeResult(
  handler: (result: DirSizeResult) => void
): Promise<UnlistenFn> {
  return listen<DirSizeResult>("dir-size-result", (event) => handler(event.payload));
}

/** Where a counted directory sits in a pane's own map, per side. */
export type DirSizeState = { status: "counting" } | { status: "done"; total: DirTotal };

/**
 * The one key a directory row is counted under, or `null` when this pane kind
 * cannot be counted at all.
 *
 * A local row is its path; a volume row is its header block. An ISO or archive
 * row is neither — there is no command that counts one, and inventing a key
 * for a count that will never arrive would leave the row saying "counting…"
 * forever. `null` is what keeps Space on such a row doing exactly what it did
 * before: marking, and nothing else (§89 — do not offer what is not built).
 */
export function dirSizeKey(side: string, entry: PanelEntry): string | null {
  if (!entry.is_dir) return null;
  if (entry.path) return `${side}|${entry.path}`;
  if (entry.header_block != null) return `${side}|block:${entry.header_block}`;
  return null;
}

/** What a directory row's Size cell should show. */
export type DirSizeCell =
  | { kind: "dir" }
  | { kind: "counting" }
  | { kind: "counted"; bytes: number; partial: boolean };

/**
 * The Size column's third state (brief §3.2).
 *
 * Before ART-087 the column had two: a number, or `<DIR>`. Counting is not
 * instant on a drawer of forty thousand files, so "counting…" is a state of
 * its own rather than a blank or a stale `<DIR>` — and a count that stopped
 * short comes back `partial`, which the cell has to render as "at least this
 * much" rather than as the answer.
 */
export function dirSizeCell(state: DirSizeState | undefined): DirSizeCell {
  if (!state) return { kind: "dir" };
  if (state.status === "counting") return { kind: "counting" };
  return { kind: "counted", bytes: state.total.bytes, partial: state.total.partial };
}
