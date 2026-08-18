// Typed wrappers for the game index (SD-2 · G10).
//
// The mirror of `core::gameindex::record`. Every optional field is a `Fact`
// rather than a bare value, because the same field arrives from two tiers and
// they disagree: a slave's `ReqAGA` bit against a filename containing the
// letters "AGA" is a real case in this collection, and which one won has to
// stay visible on screen.

import { invoke } from "@tauri-apps/api/core";

import type { Phrase } from "./phrase";

/** Where a fact came from. Mirrors `core::gameindex::record::Provenance`. */
export type Provenance =
  | "rp9-manifest"
  | "whdload-slave"
  | "tosec-name"
  | "drawer-name";

/**
 * Every provenance the core can emit, in the order the record documents them:
 * the two that **state** first, the two that **suggest** after.
 */
export const ALL_PROVENANCES: readonly Provenance[] = [
  "rp9-manifest",
  "whdload-slave",
  "tosec-name",
  "drawer-name",
] as const;

/**
 * Whether this source declared the value rather than implying it.
 *
 * The same split as `Provenance::is_stated` in Rust. Kept in both places
 * deliberately: the screen needs the answer without a round trip, and the two
 * lists are four words long each.
 */
export function isStated(from: Provenance): boolean {
  return from === "rp9-manifest" || from === "whdload-slave";
}

/** A value and the source that gave it. */
export interface Fact<T> {
  value: T;
  from: Provenance;
}

export type TitleKind =
  | "game"
  | "demo"
  | "system"
  | "gallery"
  | "video"
  | { other: string };

export type ChipsetRequirement = "ocsecs" | "aga";

/** One Kickstart a slave will accept, and the checksum identifying it. */
export interface KickstartAlternative {
  image: string;
  crc16: number;
}

export interface KickstartNeed {
  /** The one image, or the first of several the slave accepts. */
  image: string | null;
  size: number | null;
  /** Never the `0xffff` list sentinel — that is a marker, not a checksum. */
  crc16: number | null;
  rom_version: number | null;
  /**
   * Every image the slave accepts, when it accepts more than one.
   *
   * An AGA title commonly names three — an A600, an A1200 and an A4000 ROM.
   * Empty for a slave that names exactly one.
   */
  alternatives: KickstartAlternative[];
}

/**
 * A hardfile that boots itself — no RDB, a WHDLoad drawer and an
 * `S/startup-sequence` that runs it (ART-147; `core::gameindex::readers::
 * whdhdf`'s own header). `file` is the image, mounted and booted directly:
 * no system volume, no boot directory, no Y1/Y2. `slave` is not the media's
 * kind but a fact carried alongside it — what named the title.
 */
export type Media =
  | { kind: "floppies"; ordered: string[] }
  | { kind: "hardfile"; file: string }
  | { kind: "whdload-hardfile"; file: string; slave: string };

export interface SourceRef {
  name: string;
  sha256: string;
  bytes: number;
}

export interface GameRecord {
  schema: number;
  id: string;
  title: Fact<string>;
  kind: Fact<TitleKind> | null;
  year: Fact<number> | null;
  publisher: Fact<string> | null;
  genre: Fact<string> | null;
  rating: Fact<number> | null;
  chipset: Fact<ChipsetRequirement> | null;
  kickstart: Fact<KickstartNeed> | null;
  media: Media;
  preview: string | null;
  source: SourceRef;
}

/** One catalogued title and where its file sits. */
export interface CatalogueEntry {
  path: string;
  record: GameRecord;
}

/** The name of a source, for the marker beside a guessed value. */
export function provenancePhrase(from: Provenance): Phrase {
  return { key: `gameindex.provenance.${from}` };
}

/**
 * Start indexing a folder. Returns a job id straight away — indexing reads
 * inside every hardfile and hashes every file (§54/§55), and the catalogue
 * arrives in a `gameindex-result` event.
 */
export async function gameindexScan(dirPath: string): Promise<number> {
  return invoke<number>("gameindex_scan", { dirPath });
}

/** Payload of the `gameindex-result` event. */
export interface IndexResult {
  job_id: number;
  dir_path: string;
  entries: CatalogueEntry[];
}

export const INDEX_RESULT_EVENT = "gameindex-result";

/** Subscribe to finished indexes. Returns an unlisten function. */
export async function onIndexResult(
  handler: (result: IndexResult) => void
): Promise<() => void> {
  const { listen } = await import("@tauri-apps/api/event");
  return listen<IndexResult>(INDEX_RESULT_EVENT, (e) => handler(e.payload));
}

// ---------------------------------------------------------------------------
// The saved catalogue (wave A). Nothing here starts a scan on its own.
// ---------------------------------------------------------------------------

/** How much a refresh is willing to trust. Mirrors `store::Refresh`. */
export type RefreshMode = "update" | "rescan";

/**
 * Whether a string is one of the two words the command accepts.
 *
 * The Rust side refuses a third word rather than guessing at it; this stops one
 * being sent in the first place, and narrows the type on the way.
 */
export function isRefreshMode(value: string): value is RefreshMode {
  return value === "update" || value === "rescan";
}

/** One title's hand corrections. `null` means "no opinion". */
export interface RecordOverride {
  title: string | null;
  year: number | null;
  publisher: string | null;
  genre: string | null;
  chipset: ChipsetRequirement | null;
}

/**
 * An override with nothing in it.
 *
 * Sending this is how the screen says "forget my corrections for this title":
 * the Rust side removes an empty override rather than storing one, so changing
 * your mind leaves no trace.
 */
export const NO_OVERRIDE: RecordOverride = {
  title: null,
  year: null,
  publisher: null,
  genre: null,
  chipset: null,
};

/** One title as the catalogue presents it. */
export interface EntryView {
  path: string;
  /** Asked of the disk at load time, never stored. */
  available: boolean;
  record: GameRecord;
}

/** One catalogued folder. */
export interface RootView {
  root: string;
  /** Seconds since the epoch, as a string. `null` when never scanned. */
  scanned_at: string | null;
  /** Read by an older reader; an update would improve these entries. */
  stale: boolean;
  entries: EntryView[];
}

/** Load the saved catalogue. **Starts no scan.** */
export async function catalogueLoad(): Promise<RootView[]> {
  return invoke<RootView[]>("catalogue_load");
}

export async function catalogueAddRoot(root: string): Promise<void> {
  return invoke<void>("catalogue_add_root", { root });
}

/** Remove a folder and its titles. The files themselves are untouched. */
export async function catalogueRemoveRoot(root: string): Promise<void> {
  return invoke<void>("catalogue_remove_root", { root });
}

/** Refresh one folder. Returns a job id; watch it through `@/lib/jobs`. */
export async function catalogueRefresh(
  root: string,
  mode: RefreshMode
): Promise<number> {
  return invoke<number>("catalogue_refresh", { root, mode });
}

/** Record or clear one title's corrections. Returns where the backup went. */
export async function catalogueSetOverride(
  id: string,
  edit: RecordOverride
): Promise<string | null> {
  return invoke<string | null>("catalogue_set_override", { id, edit });
}

// ---------------------------------------------------------------------------
// Names ART could only take from a filename, and what it would propose instead.
//
// Nothing here applies anything. The project's decision was that ART must not
// guess at a name: it proposes, and the user accepts — which is why both fields
// are null far more often than not, and why a row with nothing to propose shows
// no button at all.
// ---------------------------------------------------------------------------

export interface NameSuggestion {
  /** A cleaner catalogue title. Applying it writes a user override. */
  title: string | null;
  /** A cleaner filename, extension included. Applying it renames a real file. */
  fileName: string | null;
}

/**
 * What ART would propose for these paths, one answer each, in order.
 *
 * Read-only: nothing is written, so a screen can ask about a whole library.
 */
export async function nameSuggestions(
  paths: string[],
  titles: string[]
): Promise<NameSuggestion[]> {
  return invoke<NameSuggestion[]>("name_suggestions", { paths, titles });
}

/**
 * Rename a title's file on disk. Returns the new full path.
 *
 * Changes the user's data, so it behaves like every other mutating command:
 * the new name is a file name and never a path, an existing target is refused
 * rather than replaced, and the operation is logged either way. The catalogue
 * is not rewritten — an entry's id comes from its content, so the next refresh
 * recognises the file at its new path as the same title.
 */
export async function renameTitleFile(
  path: string,
  newName: string
): Promise<string> {
  return invoke<string>("rename_title_file", { path, newName });
}

export const REFRESHED_EVENT = "catalogue-refreshed";

export interface RefreshedRoot {
  job_id: number;
  root: string;
}

/** Subscribe to finished refreshes. Returns an unlisten function. */
export async function onCatalogueRefreshed(
  handler: (result: RefreshedRoot) => void
): Promise<() => void> {
  const { listen } = await import("@tauri-apps/api/event");
  return listen<RefreshedRoot>(REFRESHED_EVENT, (e) => handler(e.payload));
}

/** What kind of media a record describes, for a filter or a badge. */
export function mediaKind(media: Media): "floppies" | "hardfile" | "whdload" {
  switch (media.kind) {
    case "floppies":
      return "floppies";
    case "hardfile":
      return "hardfile";
    case "whdload-hardfile":
      return "whdload";
  }
}
