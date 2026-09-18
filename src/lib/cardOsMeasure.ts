// The card section's own sentences (round 4, task 8).
//
// `src/lib` never renders a string: every function here returns a `Phrase`
// — a catalogue key plus its parameters — and `CardSection.tsx` /
// `CardPartitionRow.tsx` call `t()` on it. That is what keeps this module
// unit-testable without booting a translator, and what keeps the wording in
// the catalogues rather than in the code.
//
// Nothing here re-derives a size, a name rule or a refusal: the numbers come
// from `card_os_measure`'s `MeasuredCard`, the classification from
// `card_os_classify`, and the name verdict from `card_os_check_volume_name`
// (owner decisions Q5, Q6, Q7). This module only decides which sentence says
// what the core already answered.
//
// **Decimal units, on purpose.** A card is sold in decimal gigabytes and
// `image_bytes_for_label` builds one in decimal gigabytes (ART-308), so a row
// that printed GiB beside a "64 GB card" would be two units in one line.

import type {
  CardImagePlan,
  CardRefusal,
  ClassifiedSource,
  FoundDriver,
  MeasuredPartition,
  PartitionInput,
  SourceKind,
  UnusableSource,
  VolumeNameVerdict,
} from "@/lib/cardOs";
import type { CardPartitionTarget } from "@/lib/cardTarget";
import type { Phrase } from "@/lib/phrase";

/** PFS3's block, the unit `SizingRefusal`'s `*_blocks` fields count in
 *  (`core::card::sizing::PFS3_BLOCK`). */
export const PFS3_BLOCK = 512;

/** The code `CoreError::CardDoesNotFit` carries (`core::error::code`). */
export const DOES_NOT_FIT = "ART-CARD-DOES-NOT-FIT";

/**
 * Whether a row is one of the two the card always has.
 *
 * **Neither is ever sent to `card_os_measure`.** `measure_card` measures
 * System from the session's tree, and `plan_card_image` appends Work — split
 * into `Work`, `Work_1`, … when the rest is larger than PFS3's ceiling — as
 * whatever is left. Sending either back would plan a second volume with the
 * same name, which is a card with two drawers called Work on it.
 */
export function isFixedPartition(name: string): boolean {
  return name === "System" || name === "Work" || /^Work_\d+$/.test(name);
}

/** The user's own partitions, as `card_os_measure` asks for them. */
export function measurableInputs(partitions: CardPartitionTarget[]): PartitionInput[] {
  return partitions
    .filter((partition) => !isFixedPartition(partition.name))
    .map((partition) => ({
      volumeName: partition.name,
      sources: partition.sources,
      ...(partition.floorBytes === undefined ? {} : { floorBytes: partition.floorBytes }),
    }));
}

/** What a source *is*, one phrase per kind the core recognises. */
export function sourceKindPhrase(kind: SourceKind): Phrase {
  switch (kind.kind) {
    case "folder":
      return { key: "cardSection.sourceKind.folder" };
    case "archive":
      return { key: "cardSection.sourceKind.archive", params: { format: kind.format } };
    case "whdload-hardfile":
      return { key: "cardSection.sourceKind.whdloadHardfile" };
    case "adf":
      return { key: "cardSection.sourceKind.adf" };
  }
}

/**
 * Why a source cannot be used — one sentence per reason, each naming its own
 * next step. **Never a shared "unknown"**: a user told only that something is
 * wrong has been told the one thing they cannot act on.
 */
export function unusableSourcePhrase(why: UnusableSource): Phrase {
  switch (why.reason) {
    case "missing":
      return { key: "cardSection.unusable.missing" };
    case "unreadable":
      return { key: "cardSection.unusable.unreadable", params: { detail: why.detail } };
    case "archive-unreadable":
      return { key: "cardSection.unusable.archiveUnreadable", params: { detail: why.detail } };
    case "hardfile-not-whdload":
      return { key: "cardSection.unusable.hardfileNotWhdload", params: { detail: why.detail } };
    case "not-an-amiga-source":
      return {
        key: "cardSection.unusable.notAnAmigaSource",
        params: { format: why.formatHint },
      };
    case "not-a-folder":
      return { key: "cardSection.unusable.notAFolder" };
  }
}

/**
 * One source, said in one phrase: what it is, or why it cannot be used.
 *
 * `undefined` is its own answer — the classification has not come back yet,
 * which is not the same thing as a source the core looked at and refused.
 */
export function sourcePhrase(source: ClassifiedSource | undefined): Phrase {
  if (!source) return { key: "cardSection.source.pending" };
  if (source.why) return unusableSourcePhrase(source.why);
  if (source.kind) return sourceKindPhrase(source.kind);
  return { key: "cardSection.source.pending" };
}

/** One decimal place, the way every card figure on this screen is printed. */
function scaled(bytes: number, unit: number): string {
  return (bytes / unit).toFixed(1);
}

/** A size in the unit it is readable in. */
export function sizePhrase(bytes: number): Phrase {
  if (bytes >= 1_000_000_000) {
    return { key: "cardSection.size.gb", params: { value: scaled(bytes, 1_000_000_000) } };
  }
  if (bytes >= 1_000_000) {
    return { key: "cardSection.size.mb", params: { value: scaled(bytes, 1_000_000) } };
  }
  if (bytes >= 1_000) {
    return { key: "cardSection.size.kb", params: { value: scaled(bytes, 1_000) } };
  }
  return { key: "cardSection.size.bytes", params: { value: bytes } };
}

/**
 * What the plan gives a partition, by name — `null` when the plan has no such
 * partition, which is what a row says before anything has been measured.
 *
 * A split Work (`Work`, `Work_1`, …) is **one row on screen and one figure**:
 * the split is the planner's answer to PFS3's ceiling, not a partition the
 * user asked for.
 */
export function partitionBytes(plan: CardImagePlan | null, volumeName: string): number | null {
  if (!plan) return null;
  const matches = plan.partitions.filter((partition) =>
    volumeName === "Work"
      ? partition.volume_name === "Work" || /^Work_\d+$/.test(partition.volume_name)
      : partition.volume_name === volumeName
  );
  if (matches.length === 0) return null;
  return matches.reduce((sum, partition) => sum + partition.bytes, 0);
}

/** What a partition's sources hold, counted rather than guessed. */
export function partitionContentPhrase(measured: MeasuredPartition | undefined): Phrase {
  if (!measured || measured.sources.length === 0) return { key: "cardSection.content.none" };
  const count = measured.sources.reduce((sum, source) => sum + source.files, 0);
  return {
    key: "cardSection.content.summary",
    params: { sources: measured.sources.length, count },
  };
}

/**
 * The total line: what the partitions come to, against the card image's own
 * bytes (the mock-up's "free of total" register, U § 4.3).
 */
export function totalPhrase(plan: CardImagePlan): Phrase {
  const used = plan.partitions.reduce((sum, partition) => sum + partition.bytes, 0);
  return {
    key: "cardSection.total.line",
    params: {
      used: scaled(used, 1_000_000_000),
      total: scaled(plan.image_bytes, 1_000_000_000),
      percent: plan.image_bytes === 0 ? 0 : Math.round((used / plan.image_bytes) * 100),
    },
  };
}

function gbOf(params: Record<string, string>, field: string): string {
  return scaled(Number(params[field] ?? 0), 1_000_000_000);
}

function gbOfBlocks(params: Record<string, string>, field: string): string {
  return scaled(Number(params[field] ?? 0) * PFS3_BLOCK, 1_000_000_000);
}

/**
 * An overflow, named: **which partition and how many bytes**, never "it does
 * not fit". A user told only that the card is too small has to guess which of
 * their own partitions to cut.
 *
 * `null` for any refusal that is not about size — those are `errorPhrase`'s,
 * and a sizing sentence over a missing driver would be the screen
 * out-claiming the core.
 */
export function overflowPhrase(refusal: CardRefusal): Phrase | null {
  if (refusal.code !== DOES_NOT_FIT) return null;
  const params = refusal.params ?? {};
  switch (params.kind) {
    case "does-not-fit": {
      const shared = {
        needed: gbOf(params, "needed"),
        available: gbOf(params, "available"),
        over: scaled(
          Math.max(0, Number(params.needed ?? 0) - Number(params.available ?? 0)),
          1_000_000_000
        ),
      };
      return params.largest
        ? {
            key: "cardSection.overflow.doesNotFit",
            params: { ...shared, partition: params.largest },
          }
        : { key: "cardSection.overflow.doesNotFitNoPartition", params: shared };
    }
    case "partition-too-large":
      return {
        key: "cardSection.overflow.partitionTooLarge",
        params: { partition: params.volumeName ?? "", gb: gbOf(params, "bytes") },
      };
    case "card-too-small":
      return {
        key: "cardSection.overflow.cardTooSmall",
        params: { cardGb: params.cardGb ?? "" },
      };
    case "partition-content-does-not-fit":
      return {
        key: "cardSection.overflow.partitionContent",
        params: {
          partition: params.volumeName ?? "",
          needed: gbOfBlocks(params, "neededBlocks"),
          available: gbOfBlocks(params, "availableBlocks"),
        },
      };
    case "system-additions-do-not-fit":
      return {
        key: "cardSection.overflow.systemAdditions",
        params: {
          needed: gbOfBlocks(params, "neededBlocks"),
          available: gbOfBlocks(params, "availableBlocks"),
          tree: gbOfBlocks(params, "treeBlocks"),
          kickstarts: params.kickstarts ?? "",
        },
      };
    default:
      // The code is a sizing refusal but its `kind` is one this screen has
      // not met: Rust's own sentence, rather than a guess at which partition
      // it was about.
      return { key: "cardSection.overflow.other", params: { sentence: refusal.message } };
  }
}

/**
 * The PFS3 driver line the design draws — "pfs3aio 19.2 — from
 * paketler\pfs3aio.lha".
 *
 * **Two sentences, because where it came from is two different facts.** A
 * driver unpacked from an archive names the archive; a loose file names the
 * file. Saying "from pfs3aio.lha" about a loose `pfs3aio` would be a claim
 * about its origin that nobody made — the *ask the artefact* rule, in one
 * line of a row.
 */
export function driverPhrase(driver: FoundDriver): Phrase {
  const version = `${driver.version}.${driver.revision}`;
  return driver.fromArchive
    ? { key: "cardSection.driver.fromArchive", params: { version, from: driver.fromArchive } }
    : { key: "cardSection.driver.loose", params: { version, at: driver.path } };
}

/** The code `CoreError::Pfs3DriverNotFound` carries. */
export const DRIVER_NOT_FOUND = "ART-PFS3-DRIVER-NOT-FOUND";

/**
 * No driver anywhere ART looked — **what to add, and where it looked**, so a
 * refusal one download fixes does not read like one the user cannot fix. The
 * archives ART could not read get their own sentence, and only when there
 * were any: `Pfs3DriverNotFound`'s own rule, kept rather than restated.
 *
 * `null` for every other refusal.
 */
export function driverMissingPhrase(refusal: CardRefusal): Phrase | null {
  if (refusal.code !== DRIVER_NOT_FOUND) return null;
  const searched = refusal.params?.searched ?? "";
  const unreadable = refusal.params?.unreadable ?? "";
  return unreadable
    ? { key: "cardSection.driver.missingUnreadable", params: { searched, unreadable } }
    : { key: "cardSection.driver.missing", params: { searched } };
}

/**
 * Which AmigaDOS rule a name broke, as `card_os_check_volume_name` answered
 * it (Q7). `null` for a name the core accepted — this module never decides
 * whether a name is legal, only how the core's verdict reads.
 */
export function volumeNameProblemPhrase(verdict: VolumeNameVerdict): Phrase | null {
  if (verdict.ok) return null;
  switch (verdict.why) {
    case "empty":
      return { key: "cardSection.nameProblem.empty" };
    case "too-long":
      return {
        key: "cardSection.nameProblem.tooLong",
        params: { max: verdict.maxBytes },
      };
    case "reserved-character":
      return { key: "cardSection.nameProblem.reservedCharacter" };
  }
}
