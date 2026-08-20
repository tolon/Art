// The two-pane file manager. Mirrors src-tauri/src/commands/panel.rs.
//
// A pane can show a local folder, an ADF image or an HDF image. `PanelEntry` is
// the one row shape they all produce, so the table does not care which is which.

import { invoke } from "@tauri-apps/api/core";
import type { Phrase } from "@/lib/phrase";
import { awaitJobResult } from "@/lib/jobs";

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

/**
 * Start counting a local folder. Returns the **job id**, not the answer: the
 * count runs on a job thread (§54), so a folder of forty thousand files
 * neither blocks the command thread nor becomes unstoppable.
 *
 * Prefer {@link countHostDirectory}, which hides the job behind an ordinary
 * promise and closes the subscribe-after-invoke race.
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

/** The event both directory-size commands answer on. */
export const DIR_SIZE_EVENT = "dir-size-result";

/** The payload `dir-size-result` carries. */
export interface DirSizeResult {
  job_id: number;
  /** The path or block the count was asked about — see the Rust side. */
  key: string;
  total: DirTotal;
}

/**
 * Count a local folder, as one ordinary promise (ART-087).
 *
 * # Why this is not "invoke, then listen"
 *
 * The first version of this registered the job id inside `.then()` on the
 * invoke promise, and matched the result event against that map. Rust's
 * `spawn_job` starts its thread **before** the command returns the id, so a
 * small folder finishes and emits while the frontend is still inside
 * `await invoke(...)` — the listener saw an id it did not know yet, dropped
 * the event, and the row said "counting…" for the rest of the session. That
 * is the same race `awaitJobResult`'s own doc comment records finding in
 * `osinstallCollisions`, in a new place.
 *
 * [`awaitJobResult`](@/lib/jobs) is the fix and the reason it exists:
 * it subscribes *first*, buffers anything that arrives before the id is
 * known, and matches retroactively. It also rejects on a failed or cancelled
 * job, which is what lets the caller drop the row back to `<DIR>` instead of
 * needing a second listener for the terminal state.
 */
export function countHostDirectory(path: string): Promise<DirTotal> {
  return awaitJobResult<DirSizeResult, DirTotal>(
    DIR_SIZE_EVENT,
    () => panelDirectorySize(path),
    (payload) => payload.total
  );
}

/** The same, for a directory inside a volume. See {@link countHostDirectory}. */
export function countVolumeDirectory(
  path: string,
  volumeIndex: number,
  dirBlock: number | null
): Promise<DirTotal> {
  return awaitJobResult<DirSizeResult, DirTotal>(
    DIR_SIZE_EVENT,
    () => volumeDirectorySize(path, volumeIndex, dirBlock),
    (payload) => payload.total
  );
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

// ---------------------------------------------------------------------------
// Deleting on the user's own disk (ART-080)
// ---------------------------------------------------------------------------

/** Where a deleted host file goes. Mirrors `core::hostfs::RecycleTarget`. */
export type RecycleTarget = "windows-recycle-bin";

/** What happened to one named entry. */
export interface HostDeleteRow {
  /** The name as the pane listed it — what the user recognises. */
  name: string;
  removed: boolean;
  /** Why not, in the host's own words, when it did not. English (ART-060),
   *  shown after the translated sentence rather than instead of it. */
  problem: string | null;
}

/** What one host delete did. Mirrors `core::hostfs::HostDeleteOutcome`. */
export interface HostDeleteOutcome {
  rows: HostDeleteRow[];
  /** Where the removed ones went. `null` when nothing was removed — naming a
   *  destination for a delete that did not happen would be an invention. */
  target: RecycleTarget | null;
  /** How many names were **asked** for, which is not `rows.length` when the
   *  pass stopped early. */
  asked: number;
  /** Whether the user stopped it partway. Without this a twelve-name request
   *  cancelled after three reads as a complete three-file delete. */
  cancelled: boolean;
}

export const HOST_DELETE_EVENT = "panel-host-delete-result";

/** The sentence for where a deleted host file went. A `Phrase`, so the
 *  component translates it — `src/lib` has no i18next singleton.
 *
 *  Exhaustive `switch` with a `never` fallthrough: a second target must be a
 *  compile error here rather than a delete whose destination the screen
 *  cannot name. "Where did my file go" is the question this whole feature
 *  turns on — a delete the user cannot find is the same as one they cannot
 *  undo. */
export function recycleTargetPhrase(target: RecycleTarget): Phrase {
  switch (target) {
    case "windows-recycle-bin":
      return { key: "files.hostDelete.target.recycleBin" };
    default: {
      const unreachable: never = target;
      return unreachable;
    }
  }
}

/**
 * Send named entries of a host folder to the Recycle Bin (ART-080).
 *
 * **A folder and names, never paths.** `core::hostfs::recycle_many` resolves
 * each through `safe_join`, so nothing sent from here can name a file outside
 * the folder the pane is showing — and a name that escapes, or one that is not
 * there, refuses the whole pass before a file is touched.
 *
 * **Not all-or-nothing, and the outcome says so.** A host filesystem has no
 * journal, so twelve files sent one by one are twelve completed operations and
 * the thirteenth failing cannot undo them. Every name comes back with what
 * became of it; the screen names the ones that did not go.
 *
 * A job underneath, hidden behind an ordinary promise by `awaitJobResult` —
 * the same shape `dirTotal` above and `osinstallCollisions` already use.
 */
export async function panelDeleteMany(
  folder: string,
  names: string[]
): Promise<HostDeleteOutcome> {
  return awaitJobResult<{ job_id: number } & HostDeleteOutcome, HostDeleteOutcome>(
    HOST_DELETE_EVENT,
    () => invoke<number>("panel_delete_many", { folder, names }),
    (payload) => ({
      rows: payload.rows,
      target: payload.target,
      asked: payload.asked,
      cancelled: payload.cancelled,
    })
  );
}

/**
 * What to say after a host delete — including, always, **where the files
 * went**.
 *
 * A delete the user cannot find is the same as one they cannot undo, so the
 * destination is in every sentence that reports a removal. `target` comes from
 * the engine and is `null` only when nothing was removed, which is the one
 * case with nowhere to name.
 *
 * Three sentences, not one with holes in it: all of them went, some of them
 * went, or none did. The middle case names the ones that did **not** — "eleven
 * of twelve" is not something a user can act on; the twelfth's name is.
 *
 * A `PartialPhrase`-free `Phrase`: `target` is itself a key, so this returns
 * the params it can and the component renders the nested one. `targetPhrase`
 * is handed back beside it rather than interpolated here, because `src/lib`
 * has no translator to resolve it with.
 */
export interface HostDeleteMessage {
  key: string;
  params: Record<string, string | number>;
  /** The destination's own key, for the caller to translate and pass in as
   *  `target`. `null` when nothing was removed. */
  targetPhrase: Phrase | null;
}

export function describeHostDelete(outcome: HostDeleteOutcome): HostDeleteMessage {
  const removed = outcome.rows.filter((row) => row.removed).length;
  const failed = outcome.rows.filter((row) => !row.removed).map((row) => row.name);
  const targetPhrase = outcome.target ? recycleTargetPhrase(outcome.target) : null;
  const asked = outcome.asked;

  if (removed === 0) {
    return {
      key: "files.hostDelete.noneRemoved",
      params: { names: failed.slice(0, 3).join(", "), count: failed.length },
      targetPhrase: null,
    };
  }
  // **Stopped is its own sentence** (review F1). A twelve-name request
  // cancelled after three has three rows, all successful, and every count
  // derived from `rows` alone then reads "3 item(s) went to the Recycle Bin" —
  // true of the three, silent about the nine, and indistinguishable from a
  // complete three-file delete. The count asked for, and the fact that it
  // stopped, are both part of what happened.
  if (outcome.cancelled) {
    return {
      key: "files.hostDelete.cancelled",
      params: { removed, asked, count: asked - removed },
      targetPhrase,
    };
  }
  if (failed.length > 0) {
    return {
      key: "files.hostDelete.partial",
      params: {
        removed,
        asked,
        names: failed.slice(0, 3).join(", "),
        count: failed.length,
      },
      targetPhrase,
    };
  }
  return {
    key: "files.hostDelete.sentTo",
    params: { count: removed },
    targetPhrase,
  };
}
