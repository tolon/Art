// Several `.lha` archives onto one disk, in one operation (Phase 1A, Task 5).
// Mirrors src-tauri/src/commands/archives.rs.
//
// Where each archive lands:
//   - exactly one top-level directory in the archive → that directory's name
//   - otherwise → the archive's own file stem
//
// `archivesPlanInstall` writes nothing and shows the drawer name it worked
// out for every archive, so the user can see all of them — and cancel —
// before a single block is written (§92). Only then `archivesInstall`.

import { invoke } from "@tauri-apps/api/core";

import type { OverwritePolicy } from "@/lib/sources";
import type { CopyPlan } from "@/lib/volumeWrite";

/** Where one archive's contents will land, before anything is written. */
export interface ArchiveDrawer {
  /** The archive's path, exactly as given. */
  archive: string;
  /** Its file name only, for a short label. */
  name: string;
  /** The drawer that will be created for it. */
  drawer: string;
  files: number;
  directories: number;
  bytes: number;
}

/** What installing every archive in the batch would do. Writes nothing. */
export interface ArchivesPlan {
  /** One row per archive, in the order given. */
  drawers: ArchiveDrawer[];
  /** The cost of the whole batch, over the union of every drawer — the same
   *  shape a plain multi-file copy plan already renders. */
  cost: CopyPlan;
  /** Present, as data and never an error, when the batch will not fit. */
  refusal: string | null;
}

/**
 * An `.lha` file, by extension, case-insensitively.
 *
 * The only thing that decides whether F5 on a local selection runs the
 * archive installer instead of a plain file copy — see `FileManager.tsx`.
 */
export function isArchivePath(path: string): boolean {
  return path.toLowerCase().endsWith(".lha");
}

/** What installing `archives` into a volume would do. Writes nothing. */
export async function archivesPlanInstall(
  archives: string[],
  image: string,
  volumeIndex: number,
  dirBlock: number | null
): Promise<ArchivesPlan> {
  return invoke<ArchivesPlan>("archives_plan_install", {
    archives,
    image,
    volumeIndex,
    dirBlock,
  });
}

/**
 * Install every archive in the batch. Returns a job id (§54).
 *
 * All-or-nothing: a cancelled batch leaves the image exactly as it was, never
 * a prefix of the archives installed and the rest missing. Its result arrives
 * on the same `volume-write-result` event a plain multi-file copy uses
 * (`onVolumeWriteResult` in `@/lib/volumeWrite`) — installing a batch of
 * archives *is* copying a staged selection into a volume, so the Commander's
 * one listener for copy results already knows how to show it.
 */
export async function archivesInstall(
  archives: string[],
  image: string,
  volumeIndex: number,
  dirBlock: number | null,
  overwrite?: OverwritePolicy
): Promise<number> {
  return invoke<number>("archives_install", {
    archives,
    image,
    volumeIndex,
    dirBlock,
    overwrite: overwrite ?? null,
  });
}
