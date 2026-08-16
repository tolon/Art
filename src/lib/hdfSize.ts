// What size a new hard disk image may be, and what is worth saying about the
// size that was asked for.
//
// The wizard used to offer five buttons — 500 MB to 8 GB — and nothing else.
// There was never an 8 GB limit in the engine: `create_rdb_layout`
// (`core/rdb.rs`) refuses below 10 MB and then only fails when the cylinder
// count will not fit a `u32`, which at 516,096 bytes per cylinder is measured
// in petabytes. The cap was a list of five numbers in a component
// (ART-083).
//
// So the rule here is: **accept what the user asks for, and say what is true
// about it.** A size ART cannot build is a refusal with a reason; a size ART
// can build but a real Amiga will struggle with is a warning, not a block —
// the user may be building an image for an emulator, or for a machine with
// TD64/NSD, and ART does not get to decide that for them.
//
// Pure, and no i18n singleton: refusals and warnings travel as `Phrase`.

import type { Phrase } from "@/lib/phrase";

/** `core/rdb.rs::create_rdb_layout` refuses anything smaller. */
export const HDF_MIN_MB = 10;

/**
 * Where a partition stops being addressable by standard AmigaDOS.
 *
 * `scsi.device`'s 32-bit block addressing tops out at 4 GB; past it a disk
 * needs TD64 or NSD commands *and* a filesystem that issues them. This is a
 * property of the partition, not of the image, which is why a 16 GB image
 * split into four 4 GB partitions is fine and a 5 GB single partition is not.
 */
export const FFS_PARTITION_LIMIT_MB = 4096;

/** The four DosTypes the wizard offers, as `AmigaHardDiskFs` spells them. */
export type HdfFsId = "pfs3directscsi" | "sfs0" | "ffsdircache" | "ffsstandard";

/** The two layouts the wizard offers. */
export type HdfTemplate = "single" | "split";

export type HdfSize =
  | { ok: true; mb: number }
  | { ok: false; reason: Phrase };

/**
 * Read a typed size.
 *
 * Rejects rather than rounds: a user who typed `12.5` and got 12 would have
 * been given a different disk from the one they asked for, and this is the
 * one number in the dialog that cannot be changed afterwards.
 */
export function parseCustomSize(text: string, unit: "mb" | "gb"): HdfSize {
  const trimmed = text.trim().replace(",", ".");
  if (trimmed === "") return { ok: false, reason: { key: "hardDisk.modal.custom.empty" } };

  const value = Number(trimmed);
  if (!Number.isFinite(value) || value <= 0) {
    return { ok: false, reason: { key: "hardDisk.modal.custom.notANumber" } };
  }

  const mb = unit === "gb" ? value * 1024 : value;
  if (!Number.isInteger(mb)) {
    return { ok: false, reason: { key: "hardDisk.modal.custom.notWholeMegabytes" } };
  }
  if (mb < HDF_MIN_MB) {
    return {
      ok: false,
      reason: { key: "hardDisk.modal.custom.tooSmall", params: { min: HDF_MIN_MB } },
    };
  }

  return { ok: true, mb };
}

/**
 * How big the biggest partition in this layout will be.
 *
 * Mirrors `handleCreateConfirm`'s own split — 500 MB or a third of the disk,
 * whichever is smaller, for DH0, and the rest for DH1 — because the 4 GB
 * ceiling below applies per partition, and a 6 GB disk split in two is under
 * it while the same 6 GB as one partition is not.
 */
export function largestPartitionMb(totalMb: number, template: HdfTemplate): number {
  if (template === "single") return totalMb;
  const systemMb = Math.min(500, Math.floor(totalMb / 3));
  return Math.max(systemMb, totalMb - systemMb);
}

/**
 * What is worth saying about this **size** before the image is made.
 *
 * One thing: a partition past 4 GB needs TD64/NSD, which standard FFS does not
 * issue. A warning, not a refusal — the image may be for an emulator or for a
 * machine that has both.
 *
 * PFS3 and SFS are exempt, not silently but genuinely: they address large
 * disks, which is most of why anyone chooses them. The other question they
 * raise — whether the disk carries the driver at all — used to be answered
 * here as a flat "it does not" (ART-084). It no longer belongs here: the
 * wizard can embed one now, so the answer depends on what the user picked,
 * and `@/lib/fsDriver` is what knows.
 */
export function hdfSizeWarning(
  totalMb: number,
  template: HdfTemplate,
  fs: HdfFsId
): Phrase | null {
  if (fs === "pfs3directscsi" || fs === "sfs0") {
    return null;
  }

  const largest = largestPartitionMb(totalMb, template);
  if (largest > FFS_PARTITION_LIMIT_MB) {
    return {
      key: "hardDisk.modal.warnOver4Gb",
      params: { size: Math.round((largest / 1024) * 10) / 10 },
    };
  }

  return null;
}
