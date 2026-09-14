// Formatting a card's Amiga volumes and filling them (SD-2 · G3/G5, route E
// and native). Mirrors src-tauri/src/commands/preload.rs and
// src-tauri/src/core/preload/mod.rs.
//
// **ART-120: native by default, `hst-imager` a named fallback.** ART writes
// PFS3 and FFS itself (`core::preload::native::NativeFormatter`) and no
// longer needs `hst-imager` for an ordinary preload. The tool is kept as a
// setting (`hstImagerPath`, beside the WinUAE path) for the one gap left: any
// AmigaDOS name outside ASCII on a PFS3 volume, which this version of `libpfs3`
// cannot write (ART-113) — a fact about a `copy-in` step's own content, known
// only once the run tries it, and reported by `StepReport.fallback_reason`.
//
// **ART-117: ART edits a card's RDB itself.** Embedding or replacing a
// filesystem driver is `core::preload::embed`, with no fallback. It needs a
// backup path, chosen per run and never remembered; `preloadBlocker` asks for
// it when the plan shows an edit.
//
// **What ART cannot check afterwards.** There is no PFS3 reader here, so once
// a volume is formatted and filled ART can confirm the partition table, the
// embedded driver and the geometry — and not one file inside the volume. The
// result panel says so; a tick meaning "ART did not look" is the claim §89
// forbids.

import { invoke } from "@tauri-apps/api/core";

/**
 * Where the filesystem driver the user picked is remembered.
 *
 * **One key, two screens.** The card builder embeds a driver into the RDB it
 * writes and the volume step embeds one into a card that already exists; that
 * is one answer to one question — *which `pfs3aio` is yours* — and asking for
 * the same file twice in one wizard is exactly the drift ART-197 was filed
 * about. Named here rather than typed out in two components so the two cannot
 * quietly stop matching.
 *
 * Keeps the `preload.` prefix it was born with: renaming it would lose every
 * existing user's answer, and a setting that resets itself is the one outcome
 * this project forbids outright.
 */
export const FILESYSTEM_DRIVER_KEY = "preload.driver";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

import type { CardReport } from "@/lib/card";
import type { Phrase } from "@/lib/phrase";

/** What the formatter reports itself to be. */
export interface ToolVersion {
  raw: string;
}

export interface FormatterReport {
  version: ToolVersion;
  /** False when it is not the version ART was written against. Not a refusal. */
  is_tested_version: boolean;
  tested_version: string;
}

/** How much a copy moved.
 *
 * `comments_lost`/`dates_lost` (ART-116): on the PFS3 path, `libpfs3` 0.1.3
 * has no setter for a directory entry's comment or date — only its
 * protection bits — so these count how many entries carried one that could
 * not be written. Always `0` for an FFS copy, whose own writer keeps both.
 * Not a refusal; information the caller can choose to say something about. */
export interface CopySummary {
  files: number;
  directories: number;
  /** `null` when ART has no byte total, which is **not** zero (ART-125):
   *  `hst-imager` is asked afterwards and answers in rounded units
   *  (`12.2 MB`), and a rounded string is not a byte count worth inventing
   *  digits from. ART's own writer counts what it writes and always answers.
   *  One unanswered step makes the run's total unanswerable. */
  bytes: number | null;
  comments_lost: number;
  dates_lost: number;
}

/**
 * One partition to prepare.
 *
 * **Two numbers, and both are needed.** A card is a list of Amiga disks and
 * each carries its own RDB, so "partition 1" means nothing until you say which
 * disk (ART-095). Both count from one, the way the disk itself numbers them.
 */
export interface PreloadPartition {
  area: number;
  index: number;
  volume_name: string;
  /** A folder on the PC whose tree goes in. Null formats and stops. */
  content: string | null;
}

export interface PreloadRequest {
  image: string;
  /** A filesystem driver to embed, or to replace the card's own with when newer. */
  driver: string | null;
  partitions: PreloadPartition[];
  /** Where the RDB area is copied before an edit (ART-117). Never remembered. */
  rdb_backup: string | null;
}

/** A driver's version as the FSHD stores it: `19` and `3` for 19.3. */
export interface DriverVersion {
  version: number;
  revision: number;
}

export function versionText(v: DriverVersion): string {
  return `${v.version}.${v.revision}`;
}

/** One thing the run would do, in order. */
export type PreloadStep =
  | {
      step: "import-filesystem";
      /** Which MBR slot holds the Amiga disk; null for a plain image whose RDB
       *  is at offset zero. */
      slot: number | null;
      driver: string;
      dostype: string;
      name: string;
      file_version: DriverVersion;
      /** `[first, last]` RDB blocks the edit writes. */
      blocks: [number, number];
      /** `[before, after]` when `RDBBlocksHi` is raised over empty blocks. */
      rdb_blocks_hi_raised: [number, number] | null;
    }
  | {
      step: "replace-filesystem";
      slot: number | null;
      driver: string;
      dostype: string;
      name: string;
      card_version: DriverVersion;
      file_version: DriverVersion;
      blocks: [number, number];
      rdb_blocks_hi_raised: [number, number] | null;
    }
  | {
      step: "format-partition";
      slot: number | null;
      index: number;
      drive_name: string;
      volume_name: string;
    }
  | { step: "copy-in"; slot: number | null; drive_name: string; source: string };

/** Something the preview must say that is not a step (ART-117). */
export type PlanNote =
  | {
      note: "driver-kept";
      dostype: string;
      card_version: DriverVersion;
      file_version: DriverVersion | null;
    }
  | {
      note: "replace-refused";
      dostype: string;
      card_version: DriverVersion;
      code: string;
      detail: string;
    }
  | {
      /** The card's driver and the chosen file name different programs, or
       *  either side's `$VER:` names none (spec decision 13). `null` is "no
       *  readable name"; a file with no `$VER:` at all is a `driver-kept`. */
      note: "different-driver";
      dostype: string;
      card_version: DriverVersion;
      card_name: string | null;
      file_name: string | null;
    }
  | { note: "second-edit-skipped"; dostype: string };

export interface PreloadPlan {
  image: string;
  steps: PreloadStep[];
  notes: PlanNote[];
  rdb_backup: string | null;
}

/** What a finished RDB edit did. */
export interface EmbedReport {
  slot: number | null;
  dostype: string;
  card_version: DriverVersion | null;
  file_version: DriverVersion;
  first_block: number;
  last_block: number;
  rdb_blocks_hi_raised: [number, number] | null;
  backup: string;
}

export interface PreloadOutcome {
  formatted: string[];
  copied: CopySummary;
  tool: ToolVersion | null;
  embedded: EmbedReport | null;
}

/**
 * Why one step ran on the fallback tool instead of natively (ART-120). A
 * value, never a sentence (ART-060) — {@link fallbackPhrase} translates it.
 *
 * One capability gap and the pairing it forces, matching
 * `commands/preload.rs`'s own doc comment — nothing else ever falls back.
 */
export type FallbackReason =
  | { reason: "non-ascii-pfs3-names"; paths: string[]; more: number }
  /** ART-122: a format that followed its own partition's copy onto the
   *  fallback tool, because a volume is formatted and filled by one tool. */
  | { reason: "paired-with-fallback-copy"; drive: string };

/** Which tool actually performed one step, and why, if it was not the
 *  default. Present for every step the run reached — a plain `"native"` is
 *  as much a report as a fallback is. */
export interface StepReport {
  step: PreloadStep;
  /** `"native"`, or the fallback tool's own probed version string. */
  tool: string;
  fallback_reason: FallbackReason | null;
}

export const PRELOAD_EVENT = "preload-result";

/** Why a run stopped: the job's own error sentence and its id. */
export interface StopReport {
  code: string;
  message: string;
}

export interface PreloadResult {
  job_id: number;
  image: string;
  outcome: PreloadOutcome;
  steps: StepReport[];
  /** Set when the run stopped after changing the card's RDB (final review
   *  I3): `outcome` is then what it had done before it stopped. */
  stopped: StopReport | null;
}

// ---------------------------------------------------------------------------
// The commands
// ---------------------------------------------------------------------------

/** Ask the configured tool what it is. Runs it with `--version` and nothing else. */
export async function preloadProbe(toolPath: string): Promise<FormatterReport> {
  return invoke<FormatterReport>("preload_probe", { toolPath });
}

/** What a preload would do. Writes nothing (§92's PREVIEW). */
export async function preloadPlan(
  request: PreloadRequest,
  toolPath: string
): Promise<PreloadPlan> {
  return invoke<PreloadPlan>("preload_plan", {
    command: { ...request, tool_path: toolPath },
  });
}

/**
 * Format the partitions and copy the content in. Returns a job id (§54).
 *
 * The engine recomputes the plan rather than taking the one the screen showed,
 * so a screen that previewed one thing cannot run another.
 */
export async function preloadRun(
  request: PreloadRequest,
  toolPath: string
): Promise<number> {
  return invoke<number>("preload_run", {
    command: { ...request, tool_path: toolPath },
  });
}

/** Subscribe to finished preloads. A cancelled or failed job sends one only
 *  when it had already changed the card's RDB, with `stopped` set (final
 *  review I3); every other stop is the job bar's to report. */
export async function onPreloadResult(
  handler: (result: PreloadResult) => void
): Promise<UnlistenFn> {
  return listen<PreloadResult>(PRELOAD_EVENT, (event) => handler(event.payload));
}

/** What ART can say about the ROM on a card and the tree going onto it (G9). */
export type Pairing =
  | { verdict: "paired" }
  | { verdict: "suitable"; rom: string }
  | { verdict: "unsuitable"; needs: number; found: number | null; rom: string }
  | {
      verdict: "not-checked";
      /** `check-failed` is the frontend's own: the command rejected, and a
       *  rejection that renders as silence is indistinguishable from the
       *  reassuring verdict. `core::rom::pairing` never produces it. */
      why: "tree-records-no-rom" | "card-records-no-rom" | "check-failed";
    };

/** Ask whether the card's Kickstart suits the tree. Reads two manifests. */
export async function preloadRomPairing(image: string, content: string): Promise<Pairing> {
  return invoke<Pairing>("preload_rom_pairing", { image, content });
}

/**
 * Whether a `Pairing` fetched for one request still describes the request
 * on screen now — the invalidation check the pairing effect must make
 * *before* issuing a new fetch, not only after one resolves.
 *
 * `heldFor` is the fingerprint the currently-held verdict was fetched
 * for (`null` when nothing has been fetched yet, or nothing was held).
 * `current` is the fingerprint of the request about to be asked about.
 *
 * **The bug this exists to prevent:** the pairing effect used to clear the
 * held verdict only in its "nothing to check" branch, so re-clicking Preview
 * for a *different* card or folder left the previous verdict on screen
 * until the new fetch resolved — and `preloadPlan`'s own sibling effect
 * clears synchronously, so a stale `paired` silence could sit beside a
 * fresh plan it says nothing true about. Calling this before every fetch
 * (and clearing when it answers `false`) closes that window.
 */
export function pairingStillApplies(heldFor: string | null, current: string): boolean {
  return heldFor !== null && heldFor === current;
}

/**
 * The sentence for a pairing, or `null` when there is nothing to say.
 *
 * `paired` renders nothing on purpose: silence is the right report for "the
 * ROM you built this for is the ROM on the card", and a tick that means
 * "checked and fine" invites the reader to trust the *absence* of one.
 */
export function pairingPhrase(pairing: Pairing): Phrase | null {
  switch (pairing.verdict) {
    case "paired":
      return null;
    case "suitable":
      return { key: "preload.pairing.suitable", params: { rom: pairing.rom } };
    case "unsuitable":
      if (pairing.found === null) {
        return {
          key: "preload.pairing.unsuitableUnknown",
          params: { needs: pairing.needs, rom: pairing.rom },
        };
      }
      return {
        // The quoted message is what AmigaOS 3.2 itself prints, observed on
        // a screen — and it names V47. The threshold is read from the
        // recipe, so a recipe naming 45 would otherwise have ART quote a
        // sentence about 47 beside its own "built for V45". The quote is
        // kept where it is true and dropped where it is not.
        key: pairing.needs === 47 ? "preload.pairing.unsuitable47" : "preload.pairing.unsuitable",
        params: { needs: pairing.needs, found: pairing.found, rom: pairing.rom },
      };
    case "not-checked":
      switch (pairing.why) {
        case "tree-records-no-rom":
          return { key: "preload.pairing.notChecked.tree" };
        case "card-records-no-rom":
          return { key: "preload.pairing.notChecked.card" };
        case "check-failed":
          return { key: "preload.pairing.notChecked.failed" };
      }
  }
}

/** One folder about to go onto one partition, and the verdict on it. */
export interface PairingFor {
  /** `DH1`, as the card's own RDB names it. */
  driveName: string;
  pairing: Pairing;
}

/** A verdict that has something to say, and which drive it is about. */
export interface PairingLine {
  driveName: string;
  verdict: Pairing["verdict"];
  phrase: Phrase;
}

/**
 * Which folders the ROM question is about.
 *
 * **Every chosen partition that has content, not the first one.** The screen
 * takes a content folder per partition and the plan emits a `copy-in` for
 * each, so asking about one of them and rendering an unqualified sentence
 * described a folder that was not necessarily the one at risk — and, when the
 * first folder's verdict was `paired`, rendered nothing at all while a second
 * folder needing a newer Kickstart went onto the card unwarned.
 */
export function foldersToCheck(picks: PartitionPick[]): Array<{
  driveName: string;
  content: string;
}> {
  return picks
    .filter((pick) => pick.chosen && pick.content)
    .map((pick) => ({ driveName: pick.driveName, content: pick.content as string }));
}

/**
 * One line per folder that has something to say, in the order the partitions
 * are on the card.
 *
 * A `paired` folder still contributes nothing — silence remains the right
 * report for "the ROM you built this for". What changed is that another
 * folder's silence can no longer swallow this one's warning.
 */
export function pairingLines(results: PairingFor[]): PairingLine[] {
  return results.flatMap((result) => {
    const phrase = pairingPhrase(result.pairing);
    return phrase
      ? [{ driveName: result.driveName, verdict: result.pairing.verdict, phrase }]
      : [];
  });
}

// ---------------------------------------------------------------------------
// What the screen holds, and the rules over it
// ---------------------------------------------------------------------------

/**
 * One row of the screen: a partition on the card, and what the user has said
 * about it.
 *
 * `driveName` is carried alongside the two numbers so a refusal can name the
 * drive rather than "partition 1 of disk 1", which is not what the user is
 * looking at.
 */
export interface PartitionPick {
  area: number;
  index: number;
  driveName: string;
  chosen: boolean;
  volumeName: string;
  content: string | null;
}

/**
 * Every partition on the card, as a row with nothing chosen.
 *
 * The volume name starts as the drive's own name — a fact read off the card
 * rather than a `Work` this screen would be inventing. Formatting is
 * `Destructive`, so nothing is chosen: the user picks what gets erased.
 */
export function picksFor(report: CardReport): PartitionPick[] {
  return report.card.areas.flatMap((area, areaIndex) =>
    area.rdb.partitions.map((partition, partitionIndex) => ({
      area: areaIndex + 1,
      index: partitionIndex + 1,
      driveName: partition.drive_name,
      chosen: false,
      volumeName: partition.drive_name,
      content: null,
    }))
  );
}

/** The request the commands take. Only the chosen rows reach it. */
export function toRequest(
  image: string,
  driver: string | null,
  picks: PartitionPick[],
  rdbBackup: string | null
): PreloadRequest {
  return {
    image,
    driver: driver?.trim() ? driver.trim() : null,
    partitions: picks
      .filter((pick) => pick.chosen)
      .map((pick) => ({
        area: pick.area,
        index: pick.index,
        volume_name: pick.volumeName.trim(),
        content: pick.content,
      })),
    rdb_backup: rdbBackup?.trim() ? rdbBackup.trim() : null,
  };
}

/** AmigaDOS names stop at thirty characters — `core/volume/write/dir.rs`'s
 *  `MAX_NAME_LEN`, restated here so a refusal can say the number. */
export const MAX_VOLUME_NAME = 30;

/** Whether this plan writes into the card's RDB (ART-117). */
export function editsRdb(plan: PreloadPlan): boolean {
  return plan.steps.some(
    (step) => step.step === "import-filesystem" || step.step === "replace-filesystem"
  );
}

/**
 * Why the preload cannot run yet, or null when it can.
 *
 * A reason rather than a boolean: a disabled button that does not say why is
 * the defect ART-100 was. The volume-name rules are the two
 * `core/volume/write/dir.rs::check_name` already holds.
 *
 * **ART-117: an RDB edit needs a backup path.** The engine refuses without one
 * too (`PreloadPlan::ready_to_run`); asking here keeps Run disabled rather than
 * letting the job fail.
 */
export function preloadBlocker(input: {
  image: string | null;
  rdbBackup: string | null;
  picks: PartitionPick[];
  plan: PreloadPlan | null;
}): Phrase | null {
  if (!input.image?.trim()) return { key: "preload.blocked.noCard" };

  const chosen = input.picks.filter((pick) => pick.chosen);
  if (chosen.length === 0) return { key: "preload.blocked.nothingChosen" };

  for (const pick of chosen) {
    const name = pick.volumeName.trim();
    if (!name) return { key: "preload.blocked.blankName", params: { drive: pick.driveName } };
    if (name.includes(":") || name.includes("/")) {
      return { key: "preload.blocked.badName", params: { drive: pick.driveName } };
    }
    // Characters, not bytes: thirty accented characters are thirty characters.
    if ([...name].length > MAX_VOLUME_NAME) {
      return {
        key: "preload.blocked.longName",
        params: { drive: pick.driveName, max: MAX_VOLUME_NAME },
      };
    }
  }

  if (!input.plan) return { key: "preload.blocked.notPlanned" };
  if (editsRdb(input.plan) && !input.rdbBackup?.trim()) {
    return { key: "preload.blocked.noBackup" };
  }
  return null;
}

/**
 * What a finished copy moved, for the result panel — with the byte total
 * when there is one, and **without the clause entirely** when there is not
 * (ART-125). Saying "0 bytes" for a twelve-megabyte copy is the shape of
 * claim §89 forbids, and the honest alternative is to say less rather than
 * to round a rounded number back into digits.
 */
export function copiedPhrase(copied: CopySummary): Phrase {
  const params = { files: copied.files, directories: copied.directories };
  return copied.bytes === null
    ? { key: "preload.result.copiedNoBytes", params }
    : { key: "preload.result.copied", params: { ...params, bytes: copied.bytes } };
}

/** The sentence for why one step ran on the fallback tool, for the result
 *  panel to render beside it. */
export function fallbackPhrase(reason: FallbackReason): Phrase {
  switch (reason.reason) {
    case "non-ascii-pfs3-names":
      return {
        key: "preload.fallback.nonAsciiPfs3Names",
        params: { count: reason.paths.length + reason.more, paths: reason.paths.join(", ") },
      };
    case "paired-with-fallback-copy":
      return {
        key: "preload.fallback.pairedWithFallbackCopy",
        params: { drive: reason.drive },
      };
  }
}

/** How many partitions this plan would erase. */
export function formatCount(plan: PreloadPlan): number {
  return plan.steps.filter((step) => step.step === "format-partition").length;
}

/**
 * Which writer a planned step is expected to use, for the preview to say
 * **before** the confirmation checkbox — the destructive operation's writer
 * changed under ART-120 and the screen never said so (fix-wave finding 3).
 *
 * The two RDB edits are a static fact: ART's own editor, never a fallback
 * (ART-117). A `copy-in`'s ART-113 gap — a non-ASCII AmigaDOS name on a PFS3
 * partition — is a fact about that step's own content this file's own header
 * comment already says cannot be known until the run tries it, so this names
 * the *possibility* rather than a verdict it cannot make.
 *
 * **A `format-partition` inherits its own partition's copy (ART-122).** The
 * two are no longer independent: a volume is formatted and filled by one
 * tool, so a format paired with a copy is exactly as conditional as that
 * copy is, and saying "ART's own writer does this" against a destructive
 * step that may well run on `hst-imager` would be the same kind of untrue
 * label the previous fix wave added this function to remove. A format with
 * no copy after it is unconditional, because nothing can pull it across.
 * Which is why this takes the whole plan: the pairing is a fact about the
 * plan, not about the step in isolation.
 */
export function plannedToolPhrase(step: PreloadStep, plan: PreloadPlan): Phrase {
  switch (step.step) {
    case "import-filesystem":
    case "replace-filesystem":
      return { key: "preload.plan.step.tool.nativeEmbed" };
    case "format-partition":
      return hasPairedCopy(step, plan)
        ? { key: "preload.plan.step.tool.formatConditional" }
        : { key: "preload.plan.step.tool.native" };
    case "copy-in":
      return { key: "preload.plan.step.tool.nativeConditional" };
  }
}

/** Whether this plan fills the volume this step formats — matched on the
 *  same two fields `commands/preload.rs::paired_copy_forces_fallback` pairs
 *  them by, since a mismatch here would label a step the run then treats
 *  differently. */
function hasPairedCopy(
  step: Extract<PreloadStep, { step: "format-partition" }>,
  plan: PreloadPlan,
): boolean {
  return plan.steps.some(
    (other) =>
      other.step === "copy-in" &&
      other.slot === step.slot &&
      other.drive_name === step.drive_name,
  );
}

/** The sentence for one planned step, for the component to render. */
export function stepPhrase(step: PreloadStep): Phrase {
  switch (step.step) {
    case "import-filesystem":
      return {
        key: "preload.plan.step.import",
        params: { name: step.name, dostype: step.dostype, version: versionText(step.file_version) },
      };
    case "replace-filesystem":
      return {
        key: "preload.plan.step.replace",
        params: {
          name: step.name,
          dostype: step.dostype,
          card: versionText(step.card_version),
          file: versionText(step.file_version),
        },
      };
    case "format-partition":
      return {
        key: "preload.plan.step.format",
        params: { drive: step.drive_name, volume: step.volume_name },
      };
    case "copy-in":
      return {
        key: "preload.plan.step.copy",
        params: { drive: step.drive_name, source: step.source },
      };
  }
}

/** The lines under an RDB edit step: which blocks, and where the backup goes. */
export function embedDetailPhrases(step: PreloadStep, plan: PreloadPlan): Phrase[] {
  if (step.step !== "import-filesystem" && step.step !== "replace-filesystem") return [];
  const [first, last] = step.blocks;
  const blocks: Phrase = step.rdb_blocks_hi_raised
    ? {
        key: "preload.plan.step.blocksRaised",
        params: { first, last, from: step.rdb_blocks_hi_raised[0], to: step.rdb_blocks_hi_raised[1] },
      }
    : { key: "preload.plan.step.blocks", params: { first, last } };
  const backup: Phrase = plan.rdb_backup
    ? { key: "preload.plan.step.backup", params: { backup: plan.rdb_backup } }
    : { key: "preload.plan.step.backupMissing" };
  return [blocks, backup];
}

/** The sentence for a plan note — an ignored driver is never silent. */
export function planNotePhrase(note: PlanNote): Phrase {
  switch (note.note) {
    case "driver-kept":
      return note.file_version
        ? {
            key: "preload.plan.note.kept",
            params: {
              dostype: note.dostype,
              card: versionText(note.card_version),
              file: versionText(note.file_version),
            },
          }
        : {
            key: "preload.plan.note.keptNoVersion",
            params: { dostype: note.dostype, card: versionText(note.card_version) },
          };
    case "replace-refused":
      return {
        key: "preload.plan.note.replaceRefused",
        params: {
          dostype: note.dostype,
          card: versionText(note.card_version),
          detail: note.detail,
          code: note.code,
        },
      };
    case "different-driver": {
      const base = { dostype: note.dostype, card: versionText(note.card_version) };
      if (note.card_name && note.file_name) {
        return {
          key: "preload.plan.note.differentDriver",
          params: { ...base, cardName: note.card_name, fileName: note.file_name },
        };
      }
      if (note.file_name) {
        return {
          key: "preload.plan.note.differentDriverUnknown",
          params: { ...base, fileName: note.file_name },
        };
      }
      // Final review I1: the file's `$VER:` states a version and no program.
      return note.card_name
        ? {
            key: "preload.plan.note.differentDriverFileUnnamed",
            params: { ...base, cardName: note.card_name },
          }
        : { key: "preload.plan.note.differentDriverBothUnnamed", params: base };
    }
    case "second-edit-skipped":
      return { key: "preload.plan.note.secondEdit", params: { dostype: note.dostype } };
  }
}

/** The result panel's sentence for a finished RDB edit — and, when the run
 *  `stopped` after it, a sentence that says the run did not finish and the
 *  RDB change stays (final review I3). */
export function embeddedPhrase(report: EmbedReport, stopped = false): Phrase {
  const params = {
    dostype: report.dostype,
    file: versionText(report.file_version),
    first: report.first_block,
    last: report.last_block,
    backup: report.backup,
  };
  if (report.card_version) {
    return {
      key: stopped ? "preload.result.stoppedAfterReplaced" : "preload.result.replaced",
      params: { ...params, card: versionText(report.card_version) },
    };
  }
  return {
    key: stopped ? "preload.result.stoppedAfterEmbedded" : "preload.result.embedded",
    params,
  };
}

/** The save dialog's suggestion: `<card stem>-rdb-backup.bin` (decision 11). */
export function backupDefaultName(image: string): string {
  const base = image.split(/[\\/]/).pop() || "card";
  const dot = base.lastIndexOf(".");
  const stem = dot > 0 ? base.slice(0, dot) : base;
  return `${stem}-rdb-backup.bin`;
}
