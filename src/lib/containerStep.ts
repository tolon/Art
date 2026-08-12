// What a row opens into (brief §3.1 — the headline of phase 2b).
//
// For twenty years, Enter on an ADF in Total Commander has meant *step inside
// it*: the pane navigates into the image and `[..]` comes back out. ART has
// every reader that needs — ADF, HDF partitions, ISO9660, LHA/ZIP/7z, D64/T64
// — and had none of the semantics: a container was something you opened a
// *studio* for, which is a different screen, a different mental model, and a
// place you cannot F5 out of.
//
// Two rules this module exists to keep:
//
// 1. **What a file is comes from its bytes**, not its extension. Phase 2a made
//    `detect()` content-first for exactly this; so nothing here maps a suffix
//    to a reader. `containerFor` maps the *category* `analyze_paths` already
//    returned, and `asksWhatItIs` decides only whether the question is worth
//    asking at all.
// 2. **A container is only enterable from a host folder.** ART opens an image
//    by path, and a file that lives *inside* an ADF or an archive has no path
//    — extracting it silently to somewhere temporary so a pane could enter it
//    would be a copy the user never asked for and never sees. Enter on one
//    does nothing, which is honest, and F5 copies it out, which is the answer.
//
// Pure, and no i18n singleton, like every decision module in `src/lib`.

import type { PaneKind } from "@/lib/isoPane";

/** The five container kinds a pane can hold. `local` is not one — it is the
 *  host a container is entered *from*. */
export type ContainerKind = Exclude<PaneKind, "local">;

/**
 * `FormatCategory` as `core::detect` serializes it (kebab-case — see
 * CLAUDE.md's DROP pipeline section). Restated here rather than imported so
 * this module stays a leaf.
 */
export type FormatCategory =
  | "floppy-image"
  | "harddisk-image"
  | "archive"
  | "optical-image"
  | "commodore-8bit"
  | "rom"
  | "directory"
  | "unknown";

/** Where a pane came back to, and what to put the cursor on when it does. */
export interface HostReturn {
  /** The host folder the container was entered from. */
  path: string;
  /** The container file's own name, so leaving lands the cursor back on it
   *  rather than at the top of a folder of four hundred files. */
  name: string;
}

/**
 * Which pane kind opens this category, or `null` when nothing does.
 *
 * A ROM is a real detection and deliberately not a container: there is nothing
 * inside it to list. `unknown` and `directory` are handled before this is
 * reached.
 */
export function containerFor(category: FormatCategory): ContainerKind | null {
  switch (category) {
    case "floppy-image":
      return "adf";
    case "harddisk-image":
      return "hdf";
    case "optical-image":
      return "iso";
    case "archive":
      return "archive";
    case "commodore-8bit":
      return "c64";
    case "rom":
    case "directory":
    case "unknown":
      return null;
  }
}

/**
 * Whether Enter on this row should go and ask what the file actually is.
 *
 * Only a file, only in a host folder, only with a path — see rule 2 in this
 * file's header for why a row inside a container is not a candidate no matter
 * what it is called. Directories are not candidates either: they are handled
 * by the pane's own navigation, which needs no detection round trip.
 */
export function asksWhatItIs(
  entry: { is_dir: boolean; path: string | null },
  paneKind: PaneKind
): boolean {
  return paneKind === "local" && !entry.is_dir && typeof entry.path === "string" && entry.path !== "";
}

/**
 * The breadcrumb a pane shows, container step included.
 *
 * Total Commander writes the container step as though it were a folder —
 * `E:\amiga\Games\Lotus.adf\` — and that is exactly right: the whole point of
 * the feature is that an image *is* a folder as far as walking around is
 * concerned. So a pane entered from a folder leads with the container's full
 * path, and one opened straight from the source combo leads with its own
 * `location`, which is the same string.
 *
 * `interior` is whatever the pane renders for its position inside the image (a
 * partition name, a trail, an archive path). Empty steps are dropped: an
 * archive at its root has `archiveDir === ""`, which is a position, not a name.
 */
export function containerBreadcrumb(
  host: HostReturn | null,
  location: string,
  interior: string[]
): string[] {
  const inside = interior.filter((step) => step !== "");
  return [host ? joinHostPath(host.path, host.name) : location, ...inside];
}

/** Join a host folder and a file name with the separator the path already
 *  uses, so a Windows path stays a Windows path and a POSIX one stays POSIX. */
function joinHostPath(path: string, name: string): string {
  if (path === "") return name;
  const separator = path.includes("\\") ? "\\" : "/";
  return path.endsWith(separator) ? `${path}${name}` : `${path}${separator}${name}`;
}
