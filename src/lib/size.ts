/**
 * The OS Builder's own way of printing a size (ART-105).
 *
 * The same five lines had been copied into three screens — `ContentLayout`,
 * `VolumePreload` and `CardBuilder` — identically in the first two and split
 * into two halves in the third. Two copies was a judgement call; three is
 * where it stops being one.
 *
 * ## Why this is not `panel.ts`'s `formatBytes`
 *
 * `formatBytes` prints a *file's* size for a directory listing: B, KB, MB.
 * These screens print a *volume's* — a card, a partition, a whole staging
 * tree — where the interesting range starts at hundreds of megabytes and runs
 * to hundreds of gigabytes, and a number in KB would be unreadable. Two
 * formatters because there are two questions, not because nobody looked.
 *
 * ## GiB, printed as "GB"
 *
 * The divisor is 1024³, which is a gibibyte. The label says "GB" because that
 * is what an Amiga user, a card's packaging and every other number on these
 * screens says — `core/card`'s own sizes are computed the same way. Consistent
 * with ART's own arithmetic beats pedantically correct against neither.
 */

/** 1024³ — see the module comment on the unit label. */
export const GIB = 1024 * 1024 * 1024;

const MIB = 1024 * 1024;

/**
 * Gibibytes, to two decimal places, as a bare number.
 *
 * Bare because the caller supplies the unit: `CardBuilder` interpolates it
 * into a translated sentence, where a hard-coded "GB" inside the number would
 * be an untranslatable string smuggled past the i18n catalogues.
 */
export function gibNumber(bytes: number): string {
  return (Math.round((bytes / GIB) * 100) / 100).toString();
}

/** Mebibytes, to one decimal place, as a bare number. See {@link gibNumber}. */
export function mibNumber(bytes: number): string {
  return (Math.round((bytes / MIB) * 10) / 10).toString();
}

/**
 * A size the way the OS Builder screens print one: GB at a gibibyte and
 * above, MB below it.
 *
 * The unit is part of the string here because these call sites print the
 * size on its own rather than inside a sentence, and "GB"/"MB" are the same
 * two letters in both catalogues.
 */
export function size(bytes: number): string {
  if (bytes >= GIB) return `${gibNumber(bytes)} GB`;
  return `${mibNumber(bytes)} MB`;
}
