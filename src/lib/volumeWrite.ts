// Writing into a volume. Mirrors src-tauri/src/commands/volume_write.rs.
//
// Two things every caller should know:
//
// 1. **Plan before you copy.** `volumePlanCopy` is read-only and returns the
//    real numbers — blocks needed, blocks free, names AmigaDOS cannot store,
//    collisions. Show it, then call `volumeCopyIn`. Explain before modify.
// 2. **Recovery blocks writing.** If an operation died part-way, every write
//    on that image is refused until `volumeRecover` has run. `pending_recovery`
//    on the capability report is how the UI knows to offer it.

import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

import type { Phrase, PartialPhrase } from "@/lib/phrase";
import type { OverwritePolicy } from "@/lib/sources";

/** Which write pipeline ran. Power User Mode shows it; Beginner does not. */
export type WriteStrategy = "whole-file" | "block-journal";

export interface MutationResult {
  /** The header block of whatever was created, when something was. */
  block: number | null;
  blocks_touched: number;
  free_blocks: number;
  free_bytes: number;
  verified: boolean;
  strategy: WriteStrategy;
  /** Where the previous image went. Null under the block-journal strategy,
   *  where the journal is the way back rather than a whole-file copy. */
  backup: string | null;
  /**
   * Damage the volume already carried **before** this write.
   *
   * The gate refuses only what the operation introduced, because a disk that
   * leaked a block in 1993 has to stay writable (§89). That is a rule about
   * refusing, not a licence to write into a volume ART has just found
   * cross-linked and say nothing. Empty in the ordinary case.
   */
  pre_existing_damage: string[];
}

export interface RecoveryResult {
  description: string;
  blocks_restored: number;
  /** The journal ended mid-entry, so no image write had started. */
  was_truncated: boolean;
}

export interface WriteCapability {
  writable: boolean;
  /** Never null when `writable` is false. */
  reason: string | null;
  strategy: WriteStrategy;
  free_blocks: number;
  free_bytes: number;
  block_size: number;
  volume_name: string;
  filesystem: string;
  /** An unfinished operation waiting to be undone. */
  pending_recovery: string | null;
}

export interface NameProblem {
  relative: string;
  name: string;
  reason: string;
  /** What ART would call it instead. Null when nothing usable is left. */
  suggestion: string | null;
}

export interface SplitIconPair {
  relative: string;
  /** True when the `.info` is present and the object it describes is not. */
  icon_without_object: boolean;
}

export interface CopyPlan {
  files: number;
  directories: number;
  total_bytes: number;
  blocks_needed: number;
  blocks_free: number;
  block_size: number;
  name_problems: NameProblem[];
  collisions: string[];
  split_icons: SplitIconPair[];
}

export interface CopyReport {
  files_copied: number;
  directories_created: number;
  bytes_copied: number;
  files_verified: number;
  skipped: string[];
  renamed: string[];
  cancelled: boolean;
}

export interface ExtractReport {
  files_written: number;
  directories_created: number;
  bytes_written: number;
  sidecars_written: number;
  renamed: string[];
  skipped: string[];
  cancelled: boolean;
}

export interface CopyOptions {
  overwrite?: OverwritePolicy;
  /** Write `.uaem` sidecars. Default on in Power User Mode (§4.2). */
  sidecars?: boolean;
}

// ---------------------------------------------------------------------------
// Read-only
// ---------------------------------------------------------------------------

/** Whether ART can write here, plus what the pane footer shows. */
export async function volumeWriteCapability(
  path: string,
  volumeIndex: number
): Promise<WriteCapability> {
  return invoke<WriteCapability>("volume_write_capability", {
    path,
    volumeIndex,
  });
}

/** What copying a folder in would cost. Writes nothing. */
export async function volumePlanCopy(
  path: string,
  volumeIndex: number,
  dirBlock: number | null,
  source: string
): Promise<CopyPlan> {
  return invoke<CopyPlan>("volume_plan_copy", {
    path,
    volumeIndex,
    dirBlock,
    source,
  });
}

/**
 * What copying a whole selection — several files and folders picked at
 * once, each keeping its own name at the destination — would cost. Writes
 * nothing. A one-entry selection reads exactly as `volumePlanCopy` reads for
 * a single file, but a folder root is copied *as itself*, not flattened the
 * way `volumePlanCopy` treats a folder — see `CopyPlanDialog`.
 */
export async function volumePlanCopyMany(
  path: string,
  volumeIndex: number,
  dirBlock: number | null,
  sources: string[]
): Promise<CopyPlan> {
  return invoke<CopyPlan>("volume_plan_copy_many", {
    path,
    volumeIndex,
    dirBlock,
    sources,
  });
}

/** Whether the plan can run with nothing left for the user to decide. */
export function planIsClean(plan: CopyPlan): boolean {
  return (
    plan.blocks_needed <= plan.blocks_free &&
    plan.name_problems.length === 0 &&
    plan.collisions.length === 0
  );
}

/** One sentence for the user when a copy will not fit. */
export function planShortfall(plan: CopyPlan): Phrase | null {
  if (plan.blocks_needed <= plan.blocks_free) return null;
  const short = plan.blocks_needed - plan.blocks_free;
  return {
    key: "files.copyPlan.shortfall",
    params: {
      needed: plan.blocks_needed.toLocaleString(),
      free: plan.blocks_free.toLocaleString(),
      short: short.toLocaleString(),
    },
  };
}

// ---------------------------------------------------------------------------
// Inline mutations — fast enough not to need a job
// ---------------------------------------------------------------------------

/** F7 — create a folder inside a volume. */
export async function volumeMakeDir(
  path: string,
  volumeIndex: number,
  dirBlock: number | null,
  name: string
): Promise<MutationResult> {
  return invoke<MutationResult>("volume_make_dir", {
    path,
    volumeIndex,
    dirBlock,
    name,
  });
}

/**
 * F6 — rename, or move to another folder in the same volume.
 *
 * Pass `toDirBlock` to move; leave it out to rename in place.
 */
export async function volumeRename(
  path: string,
  volumeIndex: number,
  dirBlock: number | null,
  entryBlock: number,
  newName: string,
  toDirBlock?: number | null
): Promise<MutationResult> {
  return invoke<MutationResult>("volume_rename", {
    path,
    volumeIndex,
    dirBlock,
    entryBlock,
    newName,
    toDirBlock: toDirBlock ?? null,
  });
}

/**
 * F8 — delete an entry. Destructive: confirm twice before calling (§63).
 *
 * `overrideProtection` is the *third* answer: the entry's `d` bit says
 * AmigaDOS itself would refuse to delete it, the user was asked anyway, and
 * said yes. Leave it false and the writer refuses, naming the entry — which is
 * what any path that has not asked should get (ART-088).
 */
export async function volumeDelete(
  path: string,
  volumeIndex: number,
  dirBlock: number | null,
  entryBlock: number,
  overrideProtection = false
): Promise<MutationResult> {
  return invoke<MutationResult>("volume_delete", {
    path,
    volumeIndex,
    dirBlock,
    entryBlock,
    overrideProtection,
  });
}

/** What a batch delete did. */
export interface DeleteManyResult {
  deleted: number;
  blocks_touched: number;
  free_blocks: number;
  free_bytes: number;
  verified: boolean;
  strategy: WriteStrategy;
  /** Where the previous image went, taken once for the whole batch. */
  backup: string | null;
  /**
   * Damage the volume already carried **before** this write.
   *
   * The gate refuses only what the operation introduced, because a disk that
   * leaked a block in 1993 has to stay writable (§89). That is a rule about
   * refusing, not a licence to write into a volume ART has just found
   * cross-linked and say nothing. Empty in the ordinary case.
   */
  pre_existing_damage: string[];
}

/**
 * F8 on a multi-selection — delete every named entry from `dirBlock` as one
 * operation. All-or-nothing (§92): refuses the whole batch, before deleting
 * anything, the moment one entry cannot be removed. Destructive: confirm
 * twice before calling, the same as `volumeDelete` (§63).
 */
export async function volumeDeleteMany(
  path: string,
  volumeIndex: number,
  dirBlock: number | null,
  names: string[],
  overrideProtection = false
): Promise<DeleteManyResult> {
  return invoke<DeleteManyResult>("volume_delete_many", {
    path,
    volumeIndex,
    dirBlock,
    names,
    overrideProtection,
  });
}

/** Copy one file from the user's disk into a volume. */
export async function volumePutFile(
  path: string,
  volumeIndex: number,
  dirBlock: number | null,
  source: string,
  name?: string,
  overwrite?: OverwritePolicy
): Promise<MutationResult> {
  return invoke<MutationResult>("volume_put_file", {
    path,
    volumeIndex,
    dirBlock,
    source,
    name: name ?? null,
    overwrite: overwrite ?? null,
  });
}

// ---------------------------------------------------------------------------
// Attributes and viewing
// ---------------------------------------------------------------------------

export interface AttributesView {
  name: string;
  /** `HSPARWED` exactly as stored, RWED inversion included. */
  protection: number;
  /** The same bits already rendered, so the UI never re-derives the inversion. */
  bits: string;
  comment: string;
  is_dir: boolean;
  days: number;
  mins: number;
  ticks: number;
  /** The date as a person reads it — no calendar maths on the frontend. */
  date_text: string;
}

/** An entry's protection bits, comment and date. */
export async function volumeAttributes(
  path: string,
  volumeIndex: number,
  entryBlock: number
): Promise<AttributesView> {
  return invoke<AttributesView>("volume_attributes", {
    path,
    volumeIndex,
    entryBlock,
  });
}

/**
 * Change protection bits and comment.
 *
 * A field left `undefined` keeps what is there, so a comment edit does not
 * restamp the bits and vice versa.
 */
export async function volumeSetAttributes(
  path: string,
  volumeIndex: number,
  entryBlock: number,
  protection?: number,
  comment?: string
): Promise<MutationResult> {
  return invoke<MutationResult>("volume_set_attributes", {
    path,
    volumeIndex,
    entryBlock,
    protection: protection ?? null,
    comment: comment ?? null,
  });
}

/** F3 — read the head of a file for viewing. Read-only. */
export async function volumeReadHead(
  path: string,
  volumeIndex: number,
  entryBlock: number,
  maxBytes?: number
): Promise<number[]> {
  return invoke<number[]>("volume_read_head", {
    path,
    volumeIndex,
    entryBlock,
    maxBytes: maxBytes ?? null,
  });
}

// ---------------------------------------------------------------------------
// Recovery
// ---------------------------------------------------------------------------

/**
 * Undo an operation that did not finish, or discard its journal.
 *
 * `apply` true rolls the image back to exactly what it was before the
 * operation. `apply` false leaves the image alone and removes the journal —
 * for a stale one the user has decided about. Both are deliberate acts.
 */
export async function volumeRecover(
  path: string,
  apply: boolean
): Promise<RecoveryResult | null> {
  return invoke<RecoveryResult | null>("volume_recover", { path, apply });
}

// ---------------------------------------------------------------------------
// Jobs
// ---------------------------------------------------------------------------

export const VOLUME_WRITE_EVENT = "volume-write-result";

/**
 * What an archive's extraction reports — the archive gate's own outcome
 * (`core::archive::extract::ExtractOutcome`), not a volume's `ExtractReport`.
 * It counts entries refused by name and entries whose declared size was a
 * lie, which a volume has no equivalent for.
 */
export interface ArchiveExtractOutcome {
  total_files: number;
  total_bytes: number;
  errors: string[];
  aborted: boolean;
  abort_reason: string | null;
  skipped_existing: number;
}

export type VolumeWriteResult =
  | { kind: "copy_in"; job_id: number; report: CopyReport; backup: string | null }
  | { kind: "copy_out"; job_id: number; report: ExtractReport }
  | { kind: "archive_out"; job_id: number; report: ArchiveExtractOutcome };

/** One listener for every background write result (§54). */
export function onVolumeWriteResult(
  handler: (result: VolumeWriteResult) => void
): Promise<() => void> {
  return listen<VolumeWriteResult>(VOLUME_WRITE_EVENT, (event) =>
    handler(event.payload)
  );
}

/** F5 — copy a folder from the user's disk into a volume. Returns a job id. */
export async function volumeCopyIn(
  path: string,
  volumeIndex: number,
  dirBlock: number | null,
  source: string,
  options?: CopyOptions
): Promise<number> {
  return invoke<number>("volume_copy_in", {
    path,
    volumeIndex,
    dirBlock,
    source,
    options: options ?? null,
  });
}

/**
 * F5 on a multi-selection — copy everything the user picked, from the user's
 * disk, into a volume, as one job. Each root keeps its own name at the
 * destination (see `volumePlanCopyMany`). Returns a job id.
 *
 * Unlike `volumeCopyIn`, a cancelled batch commits nothing: the job either
 * lands everything or leaves the image exactly as it was, so a selection the
 * user picked by hand and then stopped is never mistaken for a finished one.
 */
export async function volumeCopyInMany(
  path: string,
  volumeIndex: number,
  dirBlock: number | null,
  sources: string[],
  options?: CopyOptions
): Promise<number> {
  return invoke<number>("volume_copy_in_many", {
    path,
    volumeIndex,
    dirBlock,
    sources,
    options: options ?? null,
  });
}

/**
 * F5 the other way — copy a folder out of a volume. Returns a job id.
 *
 * `destDir` is the folder the user picked and `name` is the directory's own
 * name from the listing. They are passed separately and joined in Rust, by
 * `folder_destination`: `name` came out of an image ART did not write, and a
 * caller that concatenated the two first would be handing a security boundary
 * a path it can no longer check.
 */
export async function volumeCopyOut(
  path: string,
  volumeIndex: number,
  dirBlock: number | null,
  destDir: string,
  name: string,
  options?: CopyOptions
): Promise<number> {
  return invoke<number>("volume_copy_out", {
    path,
    volumeIndex,
    dirBlock,
    destDir,
    name,
    options: options ?? null,
  });
}

/**
 * One row the user picked in a volume pane, as the batch commands take it.
 *
 * The name goes to Rust as the volume holds it, never escaped or joined here:
 * `host_target` decides what NTFS will accept and `safe_join` decides whether
 * it stays inside the folder the user picked. A caller that pre-joined would
 * be handing that boundary a path it can no longer check — the same reason
 * `volumeCopyOut` takes `destDir` and `name` apart.
 */
export type SelectedEntry = {
  header_block: number;
  name: string;
  is_dir: boolean;
};

/**
 * F5 on a multi-selection in a volume pane, out to the user's disk — one job
 * for the lot (ART-065). Returns a job id.
 *
 * This replaced a `Promise.all` of one `volumeCopyOut` job per folder and one
 * `volumeExtractTo` call per file. Each was safe on its own; the batch was
 * not — a selection of ten where the seventh failed left six on disk, three
 * never attempted, and no report tying any of it back to one selection. The
 * result arrives on `onVolumeWriteResult` as a single `copy_out`.
 */
export async function volumeExtractMany(
  path: string,
  volumeIndex: number,
  entries: SelectedEntry[],
  destDir: string,
  options?: CopyOptions
): Promise<number> {
  return invoke<number>("volume_extract_many", {
    path,
    volumeIndex,
    entries,
    destDir,
    options: options ?? null,
  });
}

/**
 * F5 on a multi-selection between two images — one staged batch (ART-064).
 * Returns a job id.
 *
 * Everything picked is staged out of the source volume into one temp folder
 * and then copied in as a single operation, so the destination gets one
 * backup and one commit. A cancelled batch commits nothing, the same
 * guarantee `volumeCopyInMany` gives the other direction.
 */
export async function volumeCopyBetweenMany(
  fromPath: string,
  fromVolume: number,
  entries: SelectedEntry[],
  toPath: string,
  toVolume: number,
  toDir: number | null,
  options?: CopyOptions
): Promise<number> {
  return invoke<number>("volume_copy_between_many", {
    fromPath,
    fromVolume,
    entries,
    toPath,
    toVolume,
    toDir,
    options: options ?? null,
  });
}

/**
 * How to describe a copy's outcome in one line.
 *
 * Every branch needs `what` — the "N files and N folders" clause, which
 * joins two independently pluralised fragments this function has no
 * translator to render or join with — so every branch returns a
 * `PartialPhrase` naming it as missing. The caller resolves it (see
 * `FileManager.tsx`'s `copyResultText`) and supplies it as a param.
 */
export function describeCopy(report: CopyReport): PartialPhrase<"what"> {
  if (report.cancelled) {
    return { key: "files.status.copyResult.stopped" } as PartialPhrase<"what">;
  }
  if (report.skipped.length > 0) {
    return {
      key: "files.status.copyResult.leftAlone",
      params: { count: report.skipped.length },
    } as unknown as PartialPhrase<"what">;
  }
  return { key: "files.status.copyResult.allVerified" } as PartialPhrase<"what">;
}
