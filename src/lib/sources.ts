// Software Sources Engine — Aminet (spec §41.5).
// Mirrors src-tauri/src/core/sources and src-tauri/src/commands/sources.rs.
//
// Search, browse and version comparison are served from the LOCAL catalog, so
// everything here except sync/fetch/readme works with nothing connected.

import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

import type { Phrase, PartialPhrase } from "@/lib/phrase";

/** How strongly ART believes something, and on what grounds (§14, §34). */
export type Confidence = "HIGH" | "MEDIUM" | "LOW" | "UNKNOWN";

export type ClaimSource = "index_column" | "readme_field" | "filename";

/**
 * A fact with its provenance. Never render a claim without its confidence —
 * a version guessed from a file name is not the same statement as one read
 * from a readme.
 */
export interface Claim<T> {
  value: T;
  confidence: Confidence;
  source: ClaimSource;
}

export interface PackageRef {
  provider: string;
  /** Repository-relative, e.g. "util/libs/AmiSSL-v5-OS3.lha". */
  path: string;
}

export interface PackageMeta {
  reference: PackageRef;
  name: string;
  directory: string;
  /** As CLAIMED by the index — the real size is checked at download time. */
  size_bytes: number;
  /**
   * Weeks since upload at the time the index was generated, not a date.
   * Aminet caps it at 999, so that value means "this old or older".
   */
  age_weeks: number | null;
  short: string;
  version: Claim<string> | null;
  requires: Claim<string>[];
  author: Claim<string> | null;
  distribution: Claim<string> | null;
}

/** Aminet's age column saturates here. */
export const AGE_CAP_WEEKS = 999;

export function ageIsCapped(pkg: PackageMeta): boolean {
  return pkg.age_weeks === AGE_CAP_WEEKS;
}

/**
 * The catalog and a readme disagreeing about which release is newest.
 * §41.5.2: show both, never pick.
 */
export interface VersionDisagreement {
  newest: PackageRef;
  newest_version: string | null;
  highest: PackageRef;
  highest_version: string;
}

export interface Resolution {
  best: PackageMeta;
  disagreement: VersionDisagreement | null;
  alternatives: PackageMeta[];
}

export interface CatalogStats {
  total: number;
  /** [provider, count] pairs. */
  providers: [string, number][];
}

export interface SkippedLine {
  line_number: number;
  reason: string;
  excerpt: string;
}

export interface SyncReport {
  parsed: number;
  /** Lines the parser could not read. A rising number means the format moved. */
  skipped: number;
  examples: SkippedLine[];
  truncated: boolean;
}

export interface SyncOutcome {
  provider: string;
  mirror: string;
  index_bytes: number;
  report: SyncReport;
  /**
   * False when the parse looked too damaged to trust. The previous catalog is
   * still in place — this is a success that deliberately changed nothing.
   */
  applied: boolean;
}

export interface FetchOutcome {
  sha256: string;
  path: string;
  bytes: number;
  mirror: string;
  from_cache: boolean;
}

export interface ReadmeFields {
  short: string | null;
  author: string | null;
  uploader: string | null;
  kind: string | null;
  version: string | null;
  requires: string[];
  distribution: string | null;
  /** Fields that appeared more than once with different values. */
  ambiguous: string[];
}

/** How results are ordered. Only `relevance` uses the search text. */
export type SortOrder =
  | "relevance"
  | "newest"
  | "oldest"
  | "largest"
  | "smallest"
  | "name";

export interface SearchFilters {
  /** Match the file name only, not the one-line description. */
  name_only: boolean;
  /** Keep entries no older than this many weeks. */
  max_age_weeks: number | null;
  min_size_bytes: number | null;
  max_size_bytes: number | null;
  /** Extensions without the dot; empty means any. */
  extensions: string[];
}

export function emptyFilters(): SearchFilters {
  return {
    name_only: false,
    max_age_weeks: null,
    min_size_bytes: null,
    max_size_bytes: null,
    extensions: [],
  };
}

export interface SearchOptions {
  directory?: string | null;
  limit?: number | null;
  sort?: SortOrder;
  filters?: SearchFilters;
}

/** What to do when the destination already holds a file of the same name. */
export type OverwritePolicy = "skip" | "overwrite" | "rename";

/** Where a downloaded file ended up in the user's folder. */
export interface Placement {
  path: string;
  /** Relative to the download folder; empty for the folder itself. */
  subfolder: string;
  bytes: number;
  /** True when an existing file was kept and nothing was written. */
  skipped_existing: boolean;
  /** True when it was saved under a different name to keep both. */
  renamed: boolean;
}

export interface LibraryInfo {
  root: string;
  subfolders: string[];
}

export interface DownloadOptions {
  subfolder?: string;
  overwrite?: OverwritePolicy;
}

/** What installing a package into a disk image did. */
export interface InstallOutcome {
  files: number;
  directories: number;
  bytes: number;
  /** Folder inside the image; empty for the root. */
  into: string;
  /** Where the previous image was kept (§57). */
  backup: string | null;
  /** Entries the archive held that ART did not write, with the reason. */
  skipped: string[];
}

export interface MirrorInfo {
  name: string;
  base_url: string;
}

export interface ProviderInfo {
  id: string;
  index_path: string;
  mirrors: MirrorInfo[];
  /** Power User Mode only (§41.5.6). */
  cache_path: string;
}

// ---------------------------------------------------------------------------
// Local catalog — no network
// ---------------------------------------------------------------------------

export async function sourcesStats(): Promise<CatalogStats> {
  return invoke<CatalogStats>("sources_stats");
}

export async function sourcesSearch(
  query: string,
  options: SearchOptions = {}
): Promise<PackageMeta[]> {
  return invoke<PackageMeta[]>("sources_search", { query, options });
}

export async function sourcesResolve(
  directory: string,
  stem: string
): Promise<Resolution | null> {
  return invoke<Resolution | null>("sources_resolve", {
    directory,
    stem,
    provider: null,
  });
}

export async function sourcesProvider(): Promise<ProviderInfo> {
  return invoke<ProviderInfo>("sources_provider");
}

export async function sourcesSetMirrors(
  mirrors: MirrorInfo[]
): Promise<ProviderInfo> {
  return invoke<ProviderInfo>("sources_set_mirrors", { mirrors });
}

/** Restore the mirrors ART ships with. */
export async function sourcesResetMirrors(): Promise<ProviderInfo> {
  return invoke<ProviderInfo>("sources_reset_mirrors");
}

// ---------------------------------------------------------------------------
// The update view (§41.5.6)
// ---------------------------------------------------------------------------

/** Why ART thinks a download is out of date. */
export type NewerReason =
  | { kind: "version"; had: string; now: string }
  | { kind: "size_changed"; had: number; now: number }
  | { kind: "reuploaded"; had_weeks: number; now_weeks: number };

export type UpdateState =
  | { state: "current" }
  | { state: "newer"; reason: NewerReason }
  /** Aminet no longer lists it. The copy on disk is still the user's. */
  | { state: "withdrawn" }
  /** ART's record points at a file that has been moved or deleted. */
  | { state: "file_missing" };

export interface PackageUpdate {
  reference: PackageRef;
  name: string;
  file_path: string;
  state: UpdateState;
  /** The catalogue's current entry, when there still is one. */
  current: PackageMeta | null;
}

/**
 * Compare every download against the catalogue. Local only — no network, so
 * the answer is "as of your last sync".
 */
export async function sourcesCheckUpdates(): Promise<PackageUpdate[]> {
  return invoke<PackageUpdate[]>("sources_check_updates");
}

/** Drop a package from the update view. Does not touch the file. */
export async function sourcesForgetDownload(path: string): Promise<void> {
  return invoke<void>("sources_forget_download", { path });
}

/**
 * How to describe an update row, in the user's words.
 *
 * Every branch is a complete `Phrase` except `size_changed`: it embeds two
 * formatted sizes, and this function has no translator to render
 * {@link formatSize}'s own phrase with — so it returns a `PartialPhrase`
 * naming `now`/`had` as still missing. The caller resolves `formatSize(now)`
 * and `formatSize(had)` to text and supplies them as those params (see
 * `AminetStudio.tsx`'s `updateText`).
 */
export function describeUpdate(row: PackageUpdate): Phrase | PartialPhrase<"now" | "had"> {
  switch (row.state.state) {
    case "current":
      return { key: "aminet.updates.desc.current" };
    case "withdrawn":
      return { key: "aminet.updates.desc.withdrawn" };
    case "file_missing":
      return { key: "aminet.updates.desc.fileMissing" };
    case "newer":
      switch (row.state.reason.kind) {
        case "version":
          return {
            key: "aminet.updates.desc.version",
            params: { now: row.state.reason.now, had: row.state.reason.had },
          };
        case "size_changed":
          // No formatted sizes to give it here — see the doc comment above.
          return { key: "aminet.updates.desc.sizeChanged" } as PartialPhrase<"now" | "had">;
        case "reuploaded":
          return { key: "aminet.updates.desc.reuploaded" };
      }
  }
}

// ---------------------------------------------------------------------------
// Background work — everything that touches the network is a job (§54, §55)
// ---------------------------------------------------------------------------

export const SOURCES_EVENT = "sources-result";

export type SourcesResult =
  | { kind: "sync"; job_id: number; outcome: SyncOutcome }
  | {
      kind: "fetch";
      job_id: number;
      path: string;
      outcome: FetchOutcome;
      placement: Placement;
    }
  | { kind: "install"; job_id: number; image: string; outcome: InstallOutcome }
  | {
      kind: "readme";
      job_id: number;
      path: string;
      /** Verbatim — ART never strips a package's own licence text (§41.5.5). */
      text: string;
      fields: ReadmeFields;
    };

/** Start a catalog sync. Returns a job id; watch `onSourcesResult`. */
export async function sourcesSync(): Promise<number> {
  return invoke<number>("sources_sync");
}

/**
 * Download a package. It is verified into the cache and a named copy is put in
 * the download folder, under `options.subfolder`. Nothing is installed.
 */
export async function sourcesFetch(
  path: string,
  options: DownloadOptions = {}
): Promise<number> {
  return invoke<number>("sources_fetch", { path, options });
}

/** Where downloads are kept, and the subfolders already there. */
export async function sourcesLibrary(): Promise<LibraryInfo> {
  return invoke<LibraryInfo>("sources_library");
}

/** Choose the download folder. Created if it does not exist. */
export async function sourcesSetLibraryRoot(path: string): Promise<LibraryInfo> {
  return invoke<LibraryInfo>("sources_set_library_root", { path });
}

/** Move an already-downloaded file to another subfolder. */
export async function sourcesRelocate(
  path: string,
  subfolder: string,
  overwrite: OverwritePolicy = "skip"
): Promise<Placement> {
  return invoke<Placement>("sources_relocate", { path, subfolder, overwrite });
}

/**
 * Install a downloaded package into a floppy image.
 *
 * `archive` must already be on disk — normally the file a download placed in
 * the download folder. For a hard disk partition, use
 * {@link sourcesInstallVolume}.
 */
export async function sourcesInstallAdf(
  archive: string,
  adf: string,
  into = ""
): Promise<number> {
  return invoke<number>("sources_install_adf", { archive, adf, into });
}

/**
 * Install a downloaded package into a hard disk partition.
 *
 * The same unpack as the ADF path, then a folder copy through the Stage W
 * writer — one install path, two destinations. Returns a job id.
 */
export async function sourcesInstallVolume(
  archive: string,
  image: string,
  volumeIndex: number,
  dirBlock: number | null = null,
  overwrite: OverwritePolicy = "skip"
): Promise<number> {
  return invoke<number>("sources_install_volume", {
    archive,
    image,
    volumeIndex,
    dirBlock,
    overwrite,
  });
}

/** Fetch a package's readme. */
export async function sourcesReadme(path: string): Promise<number> {
  return invoke<number>("sources_readme", { path, provider: null });
}

export async function onSourcesResult(
  handler: (result: SourcesResult) => void
): Promise<UnlistenFn> {
  return listen<SourcesResult>(SOURCES_EVENT, (event) => handler(event.payload));
}

// ---------------------------------------------------------------------------
// Presentation helpers
// ---------------------------------------------------------------------------

/** The size the catalog claims, in the units the index uses. */
export function formatSize(bytes: number): Phrase {
  if (bytes >= 1024 * 1024) {
    return { key: "aminet.units.mb", params: { value: (bytes / (1024 * 1024)).toFixed(1) } };
  }
  if (bytes >= 1024) return { key: "aminet.units.kb", params: { value: Math.round(bytes / 1024) } };
  return { key: "aminet.units.bytes", params: { value: bytes } };
}

/**
 * Aminet's age in words. The cap has to read as an approximation, because
 * that is what it is.
 */
export function formatAge(pkg: PackageMeta): Phrase {
  if (pkg.age_weeks === null) return { key: "aminet.age.dateUnknown" };
  if (pkg.age_weeks === AGE_CAP_WEEKS) return { key: "aminet.age.capped" };
  if (pkg.age_weeks === 0) return { key: "aminet.age.thisWeek" };
  if (pkg.age_weeks < 8) return { key: "aminet.age.weeksAgo", params: { n: pkg.age_weeks } };
  const years = pkg.age_weeks / 52;
  if (years < 1) {
    return { key: "aminet.age.monthsAgo", params: { n: Math.round(pkg.age_weeks / 4) } };
  }
  return { key: "aminet.age.yearsAgo", params: { n: years.toFixed(years < 10 ? 1 : 0) } };
}

/** How to describe where a claim came from. */
export function describeSource(source: ClaimSource): Phrase {
  switch (source) {
    case "index_column":
      return { key: "aminet.claim.indexColumn" };
    case "readme_field":
      return { key: "aminet.claim.readmeField" };
    case "filename":
      return { key: "aminet.claim.filename" };
  }
}
