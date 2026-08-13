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
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

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
  /** Entries the extractor itself refused — a traversal entry, one over the
   *  decompression-bomb cap, a name it could not write — with the reason.
   *  Never silent: these never reach the copy phase, so nothing else would
   *  mention them. Shown in the plan, before the user confirms anything. */
  skipped: string[];
}

/**
 * What installing every archive in the batch would do. Writes nothing.
 *
 * Whether the batch is refused lives entirely in `cost` (`planIsClean`/
 * `planShortfall` from `@/lib/volumeWrite`, the same way a plain multi-file
 * plan is read) — there is no separate refusal field here to fall out of
 * sync with it.
 */
export interface ArchivesPlan {
  /** One row per archive, in the order given. */
  drawers: ArchiveDrawer[];
  /** The cost of the whole batch, over the union of every drawer — the same
   *  shape a plain multi-file copy plan already renders. */
  cost: CopyPlan;
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

/** A finished plan, tied back to the job that produced it. */
export interface ArchivesPlanResult {
  job_id: number;
  plan: ArchivesPlan;
}

export const ARCHIVES_PLAN_EVENT = "archives-plan-result";

/**
 * What installing `archives` into a volume would do. Writes nothing.
 * Returns a **job id** (§54).
 *
 * Planning a batch of archives is not the cheap arithmetic every other plan in
 * ART is: it has to unpack every archive to know what is in it. That used to
 * happen on the command thread, so several large archives froze the window
 * with no progress and no way to stop — in the one step that exists to let the
 * user change their mind (ART-066). The plan itself arrives on
 * `ARCHIVES_PLAN_EVENT`; a cancelled or failed job simply never sends one, and
 * its terminal state comes through `onJobProgress` like any other job's.
 */
export async function archivesPlanInstall(
  archives: string[],
  image: string,
  volumeIndex: number,
  dirBlock: number | null
): Promise<number> {
  return invoke<number>("archives_plan_install", {
    archives,
    image,
    volumeIndex,
    dirBlock,
  });
}

/** Subscribe to finished plans. Returns an unlisten function. */
export async function onArchivesPlanResult(
  handler: (result: ArchivesPlanResult) => void
): Promise<UnlistenFn> {
  return listen<ArchivesPlanResult>(ARCHIVES_PLAN_EVENT, (event) => handler(event.payload));
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
