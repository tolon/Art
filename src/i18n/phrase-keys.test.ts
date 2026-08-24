// The discriminated-union mappers in `src/lib/*` return a `Phrase` — a
// catalogue key plus params — instead of a string, so a component can
// translate it (see `src/lib/phrase.ts`). Nothing in the build catches a
// `Phrase` pointing at a key nobody added: it would just render the raw key
// string on screen. This test enumerates every variant of every such mapper
// and asserts its key resolves to an actual leaf in en.json.

import { describe, expect, it } from "vitest";
import en from "./en.json";

import { ALL_PROVENANCES, provenancePhrase } from "@/lib/gameindex";
import { kindPhrase as artworkKindPhrase, mediaPhrase } from "@/lib/collectionDetail";
import { networkBlocker } from "@/components/osbuilder/NetworkPanel";
import type { Media } from "@/lib/gameindex";
import {
  outcomePhrase,
  rebindProblemPhrase,
  sourcePhrase,
  type ArtKind,
  type RebindProblem,
  type SourceOutcome,
} from "@/lib/artwork";

import { describeCheckout, type CheckoutRow } from "@/lib/checkout";
import { jobStatusLabel, type JobProgress } from "@/lib/jobs";
import { statusLabel, type OperationRecord } from "@/lib/oplog";
import {
  AGE_CAP_WEEKS,
  describeSource,
  describeUpdate,
  formatAge,
  formatSize,
  type ClaimSource,
  type PackageMeta,
  type PackageUpdate,
} from "@/lib/sources";
import { checksumBadge, type RomChecksum } from "@/lib/rom";
import { copyDirection, ISO_WRITE_REFUSAL } from "@/lib/isoPane";
import { parseCommandLine } from "@/lib/commandLine";
import { planMove, type MoveInput } from "@/lib/movePlan";
import { describeLayout, type ImageLayout } from "@/lib/volume";
import {
  describeCopy,
  planShortfall,
  type CopyPlan,
  type CopyReport,
} from "@/lib/volumeWrite";
import { describeVerdict, describeOutcome, type WhdloadOutcome, type WhdloadVerdict } from "@/lib/whdload";
import {
  buildBlocker,
  defaultPartition,
  findingPhrase,
  healthCheckPhrase,
  healthVerdict,
  manualStepPhrase,
  rolePhrase,
  warningPhrase,
  type CardBuildPlan,
  type CardBuildRequest,
  type CardBuildWarning,
  type HealthCheck,
  type ManifestFinding,
  type CardRole,
  type ManualStep,
} from "@/lib/cardBuild";
import {
  copiedPhrase,
  fallbackPhrase,
  formatCount,
  pairingPhrase,
  plannedToolPhrase,
  preloadBlocker,
  stepPhrase,
  type FallbackReason,
  type Pairing,
  type PartitionPick,
  type PreloadPlan,
  type PreloadStep,
} from "@/lib/preload";
import {
  kindPhrase,
  layoutBlocker,
  refusalPhrase,
  type ItemKind,
  type LayoutPlan,
  type RefusalReason,
} from "@/lib/layout";
import {
  DEFAULT_EMU68_OPTIONS,
  DEFAULT_FIRMWARE_CONFIG,
  DEFAULT_HARDWARE,
} from "@/lib/pistorm";
import {
  collisionGroupHeadingKey,
  collisionPhrase,
  conditionalReason,
  conditionalReasonText,
  hostPlacementBlockKey,
  osinstallBlocker,
  refusalPhrase as osinstallRefusalPhrase,
  type Collision,
  type ConditionalReason,
  type HostPlacementBlock,
  type PlanResult,
  type RefusalReason as OsInstallRefusalReason,
} from "@/lib/osinstall";
import {
  outcomeNextStepPhrase as amigaNextStepPhrase,
  outcomePhrase as amigaOutcomePhrase,
  overlayAdvicePhrase,
  readinessBlockers,
  settlementPhrase,
  type AmigaInstallPreview,
  type RunOutcome,
  type SettlementReport,
} from "@/lib/amigainstall";
import {
  launchKindPhrase,
  machinePhrase,
  mountNotePhrase,
  notePhrase,
  refusalPhrase as launchRefusalPhrase,
  type LaunchKind,
  type LaunchNote,
  type LaunchRefusal,
  type Machine,
  type MountNote,
} from "@/lib/launch";
import { STEP_IDS, stepLabelKey } from "@/lib/buildSteps";

/** Whether `dotted` (e.g. "whdload.outcome.installed") names a string leaf. */
function isLeafKey(dotted: string): boolean {
  const parts = dotted.split(".");
  let node: unknown = en;
  for (const part of parts) {
    if (typeof node !== "object" || node === null) return false;
    node = (node as Record<string, unknown>)[part];
  }
  return typeof node === "string";
}

/**
 * The same, allowing for i18next pluralisation: a `Phrase` carrying a
 * `{{count}}` resolves against `key_one` / `key_other`, and there is no bare
 * `key` in the catalogue at all (`literal-keys.test.ts` has the same helper,
 * for the same reason).
 */
function resolvesAtRuntime(dotted: string): boolean {
  return isLeafKey(dotted) || isLeafKey(`${dotted}_one`) || isLeafKey(`${dotted}_other`);
}

describe("Phrase keys returned by the discriminated-union mappers", () => {
  it("amigainstall: every ending, every settlement, every blocker resolves", () => {
    // The four endings are four sentences and four next steps (§89, and the
    // three defects this round produced); a key nobody added would render as
    // its own dotted name on screen, which is how a "different sentence per
    // ending" quietly becomes four raw keys that all look alike.
    const endings: RunOutcome[] = [
      { kind: "succeeded" },
      { kind: "failed" },
      { kind: "timed-out", waited: { secs: 60, nanos: 0 } },
      { kind: "emulator-closed", waited: { secs: 60, nanos: 0 } },
    ];
    for (const outcome of endings) {
      expect(resolvesAtRuntime(amigaOutcomePhrase(outcome).key)).toBe(true);
      expect(resolvesAtRuntime(amigaNextStepPhrase(outcome).key)).toBe(true);
    }

    const settlements: SettlementReport[] = [
      { kind: "promoted", tree: "D:/t", leftBehind: null },
      { kind: "promoted", tree: "D:/t", leftBehind: "D:/t.old" },
      { kind: "kept", copy: "D:/t.copy", original: "D:/t" },
    ];
    for (const settlement of settlements) {
      expect(resolvesAtRuntime(settlementPhrase(settlement).key)).toBe(true);
    }

    const preview: AmigaInstallPreview = {
      packageId: "boingbag-39-1",
      packageName: "BoingBag 3.9-1",
      tree: "D:/t",
      systemVolume: "DH0",
      workingDirectory: "ARTPkg:BoingBag3.9-1",
      program: "ARTPkg:BoingBag3.9-1/C/Updater",
      args: ["AmigaOS-Update", "DH0:"],
      workVolume: "ARTWork",
      packageVolume: "ARTPkg",
      packageArchives: ["a.lha"],
      packageArchivesPresent: false,
      declaredOverlays: ["BoingBag3.9-1-UAE/BoingBag3.9-1"],
      minimumInstallerVersion: "45.15",
      packageDir: "BoingBag3.9-1",
      medium: "E:/amiga/os39/AmigaOS39.iso",
      mediumVolume: "AmigaOS3.9",
      requiredMedium: "the original AmigaOS 3.9 CD-ROM",
      resultFile: "art-result.txt",
      deadlineSeconds: 1800,
      kickstart: "D:/kick.rom",
      kickstartPresent: false,
      emulator: null,
      profileId: "a1200-aga",
      profileName: "Amiga 1200 (AGA)",
    };
    for (const blocker of readinessBlockers(preview)) {
      expect(resolvesAtRuntime(blocker.key)).toBe(true);
    }
    for (const archives of [["a.lha"], ["a.lha", "b.lha"]]) {
      const advice = overlayAdvicePhrase({ ...preview, packageArchives: archives });
      expect(advice).not.toBeNull();
      expect(resolvesAtRuntime(advice!.key)).toBe(true);
    }
  });

  it("describeUpdate: every PackageUpdate.state variant resolves", () => {
    const base = {
      reference: { provider: "aminet", path: "util/libs/AmiSSL.lha" },
      name: "AmiSSL",
      file_path: "C:/downloads/AmiSSL.lha",
      current: null,
    };
    const rows: PackageUpdate[] = [
      { ...base, state: { state: "current" } },
      { ...base, state: { state: "withdrawn" } },
      { ...base, state: { state: "file_missing" } },
      {
        ...base,
        state: { state: "newer", reason: { kind: "version", had: "1.0", now: "2.0" } },
      },
      {
        ...base,
        state: {
          state: "newer",
          reason: { kind: "size_changed", had: 100, now: 200 },
        },
      },
      {
        ...base,
        state: {
          state: "newer",
          reason: { kind: "reuploaded", had_weeks: 10, now_weeks: 1 },
        },
      },
    ];
    for (const row of rows) {
      const phrase = describeUpdate(row);
      expect(isLeafKey(phrase.key), phrase.key).toBe(true);
    }
  });

  it("describeVerdict: every confidence variant resolves", () => {
    const base = {
      slave: null,
      executable: null,
      has_data_dir: false,
      has_icon: false,
      notes: "",
    };
    const verdicts: WhdloadVerdict[] = [
      { ...base, confidence: "HIGH" },
      { ...base, confidence: "MEDIUM" },
      { ...base, confidence: "LOW" },
      { ...base, confidence: "UNKNOWN" },
    ];
    for (const verdict of verdicts) {
      const phrase = describeVerdict(verdict);
      expect(isLeafKey(phrase.key), phrase.key).toBe(true);
    }
  });

  it("describeOutcome: both icon_installed variants resolve", () => {
    const base: WhdloadOutcome = {
      drawer: "Games:Turrican",
      files: 3,
      directories: 1,
      bytes: 12345,
      verified: 3,
      icon_installed: true,
      skipped: [],
      backup: null,
    };
    for (const icon_installed of [true, false]) {
      const phrase = describeOutcome({ ...base, icon_installed });
      expect(isLeafKey(phrase.key), phrase.key).toBe(true);
    }
  });

  it("describeLayout: every ImageLayout variant resolves", () => {
    const layouts: ImageLayout[] = ["rdb", "bare_volume", "unknown"];
    for (const layout of layouts) {
      const phrase = describeLayout(layout);
      expect(isLeafKey(phrase.key), phrase.key).toBe(true);
    }
  });

  it("describeCheckout: every CheckoutRow.state variant resolves", () => {
    const base = {
      id: "abc",
      image: "C:/disks/work.hdf",
      volume_index: 0,
      dir_block: 880,
      entry_block: 42,
      name: "s/startup-sequence",
      temp_path: "C:/temp/startup-sequence",
      bytes: 512,
    };
    const rows: CheckoutRow[] = [
      { ...base, state: { state: "unchanged" } },
      { ...base, state: { state: "missing" } },
      { ...base, state: { state: "modified", bytes: 600, gained_crlf: false } },
      { ...base, state: { state: "modified", bytes: 600, gained_crlf: true } },
    ];
    for (const row of rows) {
      const phrase = describeCheckout(row);
      expect(isLeafKey(phrase.key), phrase.key).toBe(true);
    }
  });

  it("statusLabel: every OperationRecord outcome variant resolves", () => {
    const base = {
      timestamp: 0,
      operation: "copy",
      source: null,
      destination: null,
      backup: null,
      details: [] as [string, string][],
      origin: "user_interface" as const,
    };
    const records: OperationRecord[] = [
      { ...base, outcome: { result: "failure", error_code: "ART-001", message: "x" } },
      { ...base, outcome: { result: "success", verification: true } },
      { ...base, outcome: { result: "success", verification: false } },
      { ...base, outcome: { result: "success", verification: null } },
    ];
    for (const record of records) {
      const phrase = statusLabel(record);
      expect(isLeafKey(phrase.key), phrase.key).toBe(true);
    }
  });

  // Found alongside the six named in the task, while auditing `src/lib` for
  // other functions returning an English sentence: `jobStatusLabel` has the
  // same shape (a discriminated union mapped to a status word) and was
  // translated the same way.
  it("jobStatusLabel: every JobState variant resolves", () => {
    const base = {
      id: 1,
      title: "Copying",
      done: 1,
      total: null,
      message: "",
    };
    const jobs: JobProgress[] = [
      { ...base, state: { state: "running" } },
      { ...base, state: { state: "finished" } },
      { ...base, state: { state: "cancelled", files_landed: null } },
      // Both branches of the cancelled case (ART-058): nothing landed, and
      // some did. The second resolves through i18next's plural suffixes, so
      // `isLeafKey` is asked about `…_one`/`…_other` rather than the bare key.
      { ...base, state: { state: "cancelled", files_landed: 1 } },
      { ...base, state: { state: "cancelled", files_landed: 12 } },
      { ...base, state: { state: "failed", error_code: "ART-001", message: "x" } },
    ];
    for (const job of jobs) {
      const phrase = jobStatusLabel(job);
      // `resolvesAtRuntime`, not `isLeafKey`: the partway case carries a
      // `{{count}}` and lives in the catalogue only as `_one`/`_other`.
      expect(resolvesAtRuntime(phrase.key), phrase.key).toBe(true);
    }
  });

  // The switch-based mappers above are partly protected by TypeScript: a new
  // union variant left unhandled is a compile error. `formatSize`,
  // `formatAge`, `describeCopy` and `planShortfall` are if-chains over
  // numbers and booleans instead, so a new branch with a typo'd key is
  // caught by nothing at all — these four, plus `describeSource` (untested
  // only because it was missed alongside the seven above), close that gap.

  it("describeSource: every ClaimSource variant resolves", () => {
    const sources: ClaimSource[] = ["index_column", "readme_field", "filename"];
    for (const source of sources) {
      const phrase = describeSource(source);
      expect(isLeafKey(phrase.key), phrase.key).toBe(true);
    }
  });

  it("formatSize: the bytes, KB and MB branches all resolve", () => {
    for (const bytes of [512, 2048, 5 * 1024 * 1024]) {
      const phrase = formatSize(bytes);
      expect(isLeafKey(phrase.key), phrase.key).toBe(true);
    }
  });

  it("formatAge: every branch resolves", () => {
    const base: Omit<PackageMeta, "age_weeks"> = {
      reference: { provider: "aminet", path: "util/libs/AmiSSL.lha" },
      name: "AmiSSL",
      directory: "util/libs",
      size_bytes: 1024,
      short: "AmiSSL",
      version: null,
      requires: [],
      author: null,
      distribution: null,
    };
    const ageWeeks = [
      null, // date unknown
      0, // this week
      3, // weeks ago (< 8)
      20, // months ago (< 1 year)
      104, // years ago (>= 1 year)
      AGE_CAP_WEEKS, // capped
    ];
    for (const age_weeks of ageWeeks) {
      const phrase = formatAge({ ...base, age_weeks });
      // `resolvesAtRuntime`, not `isLeafKey`: two of these branches are
      // pluralised, so the catalogue holds `_one`/`_other` and no bare key
      // (ART-061).
      expect(resolvesAtRuntime(phrase.key), phrase.key).toBe(true);
    }
  });

  it("formatAge: the pluralised branches carry the count i18next selects on", () => {
    // The half a key check cannot see. i18next picks the plural form from
    // `count` and from nothing else, so a phrase that says `{ n }` renders the
    // `_other` form at every number — which is the "1 weeks ago" this fixed.
    const base: Omit<PackageMeta, "age_weeks"> = {
      reference: { provider: "aminet", path: "util/libs/AmiSSL.lha" },
      name: "AmiSSL",
      directory: "util/libs",
      size_bytes: 1024,
      short: "AmiSSL",
      version: null,
      requires: [],
      author: null,
      distribution: null,
    };
    for (const age_weeks of [1, 3, 20, 40]) {
      const phrase = formatAge({ ...base, age_weeks });
      expect(phrase.params, phrase.key).toHaveProperty("count");
    }
  });

  it("describeCopy: every CopyReport branch resolves", () => {
    const base: CopyReport = {
      files_copied: 3,
      directories_created: 1,
      bytes_copied: 4096,
      files_verified: 3,
      skipped: [],
      renamed: [],
      cancelled: false,
    };
    const reports: CopyReport[] = [
      { ...base, cancelled: true }, // stopped
      { ...base, skipped: ["Foo"] }, // left alone
      base, // all verified
    ];
    for (const report of reports) {
      const phrase = describeCopy(report);
      expect(isLeafKey(phrase.key), phrase.key).toBe(true);
    }
  });

  // Only one variant to enumerate — every direction that writes into a disc
  // resolves to the same `Phrase` — but it is still a `Phrase` reaching the
  // screen through `t()` rather than a literal `t("…")` call, so it is the
  // shape `literal-keys.test.ts` cannot check for itself.
  it("copyDirection: a refused direction's reason resolves, for every source kind", () => {
    expect(isLeafKey(ISO_WRITE_REFUSAL.key)).toBe(true);
    for (const source of ["local", "adf", "hdf", "iso"] as const) {
      const result = copyDirection(source, "iso");
      expect(result.kind).toBe("refused");
      if (result.kind === "refused") {
        expect(isLeafKey(result.reason.key), result.reason.key).toBe(true);
      }
    }
  });

  // F6's refusals are the most important `Phrase`s on the Files screen: each
  // one is the sentence standing between the user and a move that could lose
  // data, and a key nobody added would put a raw dotted string there instead.
  it("planMove: every refusal's reason resolves", () => {
    const base: MoveInput = {
      sourceKind: "adf",
      targetKind: "local",
      sourceWritable: true,
      targetWritable: true,
      entries: [{ name: "Lotus", isDir: true }],
      takenNames: [],
      sameImage: false,
      sourceIsRoot: false,
    };
    const refusals: MoveInput[] = [
      { ...base, entries: [] },
      // ART-080: a host *folder* is a legitimate source now; a host **root**
      // is the one case still refused.
      { ...base, sourceKind: "local", targetKind: "adf", sourceIsRoot: true },
      { ...base, sourceKind: "iso" },
      // Review F8: host to host, refused in the plan rather than by the page
      // after two confirmations.
      { ...base, sourceKind: "local", targetKind: "local" },
      { ...base, sourceWritable: false },
      { ...base, targetKind: "archive" },
      { ...base, targetKind: "hdf", targetWritable: false },
      // ART-081: the two "between two images" refusals are gone (a selection
      // is a batch like any other now), and this one replaced them — a move
      // between two directories of the *same* image is a relink, not a
      // copy-and-delete.
      { ...base, targetKind: "hdf", sameImage: true },
      { ...base, takenNames: ["Lotus"] },
    ];
    for (const input of refusals) {
      const plan = planMove(input);
      expect(plan.kind, JSON.stringify(input)).toBe("refused");
      if (plan.kind === "refused") {
        expect(resolvesAtRuntime(plan.reason.key), plan.reason.key).toBe(true);
      }
    }
  });

  it("parseCommandLine: both refusals resolve", () => {
    for (const text of ["cd Games", "notepad readme.txt"]) {
      const action = parseCommandLine(text);
      expect(action.kind).toBe("refused");
      if (action.kind === "refused") {
        expect(resolvesAtRuntime(action.reason.key), action.reason.key).toBe(true);
      }
    }
  });

  it("planShortfall: the shortfall branch resolves; the fitting branch is null", () => {
    const base: CopyPlan = {
      files: 1,
      directories: 0,
      total_bytes: 1024,
      blocks_needed: 10,
      blocks_free: 20,
      block_size: 512,
      name_problems: [],
      collisions: [],
      split_icons: [],
    };
    expect(planShortfall(base)).toBeNull();

    const short = planShortfall({ ...base, blocks_needed: 30 });
    expect(short).not.toBeNull();
    expect(isLeafKey(short!.key), short!.key).toBe(true);
  });

  it("warningPhrase: every CardBuildWarning variant resolves", () => {
    const warnings: CardBuildWarning[] = [
      { kind: "no-kickstart" },
      { kind: "rom-unrecognised" },
      { kind: "rom-machine-unknown", rom: "Kickstart 3.1 (40.068)" },
      { kind: "rom-wrong-machine", rom: "Kickstart 3.1 (A600)" },
      { kind: "volumes-unformatted" },
    ];
    for (const warning of warnings) {
      const phrase = warningPhrase(warning);
      expect(isLeafKey(phrase.key), phrase.key).toBe(true);
    }
  });

  it("findingPhrase: every ManifestFinding variant resolves", () => {
    const findings: ManifestFinding[] = [
      { kind: "schema-too-new", found: 2, understood: 1 },
      { kind: "size-changed", expected: 1, found: 2 },
      { kind: "partition-table-changed" },
      { kind: "area-count-changed", expected: 1, found: 2 },
      { kind: "area-moved", area: 0, expected: 1, found: 2 },
      { kind: "area-resized", area: 0, expected: 1, found: 2 },
      { kind: "rdb-changed", area: 0 },
      { kind: "partition-count-changed", area: 0, expected: 1, found: 2 },
      { kind: "partition-changed", area: 0, name: "SDH0" },
    ];
    for (const finding of findings) {
      const phrase = findingPhrase(finding);
      expect(isLeafKey(phrase.key), phrase.key).toBe(true);
    }
  });

  it("healthCheckPhrase: every HealthCheck variant resolves", () => {
    const checks: HealthCheck[] = [
      { kind: "boot-partition-first" },
      { kind: "boot-partition-aligned", lba: 2048 },
      { kind: "amiga-area-count", count: 1 },
      { kind: "areas-aligned" },
      { kind: "nothing-overlaps" },
      { kind: "everything-inside-the-image" },
      { kind: "area-has-rdb", area: 0 },
      { kind: "area-rdb-checksum", area: 0 },
      { kind: "every-partition-can-mount", unmountable: 0 },
      { kind: "manifest-agrees", findings: [] },
      { kind: "boot-file", role: "config", name: "config.txt" },
      { kind: "boot-file", role: "cmdline", name: "cmdline.txt" },
      { kind: "boot-file", role: "kernel", name: "Emu68-pistorm.gz" },
      { kind: "boot-file", role: "kickstart", name: "kick.rom" },
    ];
    for (const check of checks) {
      const phrase = healthCheckPhrase(check);
      expect(isLeafKey(phrase.key), phrase.key).toBe(true);
    }
  });

  it("manualStepPhrase and healthVerdict resolve", () => {
    const steps: ManualStep[] = [
      { kind: "flash-the-card" },
      { kind: "hdmi-before-power" },
      { kind: "pi-model-matches", pi: "pi3-a-plus" },
      { kind: "volumes-need-formatting", count: 1 },
    ];
    for (const step of steps) {
      expect(resolvesAtRuntime(manualStepPhrase(step).key), step.kind).toBe(true);
    }
    const pass = { check: { kind: "areas-aligned" }, state: "pass" } as const;
    const gap = { check: { kind: "areas-aligned" }, state: "not-checked" } as const;
    const bad = { check: { kind: "areas-aligned" }, state: "fail" } as const;
    for (const items of [[pass], [pass, gap], [bad]]) {
      const verdict = healthVerdict({ items: [...items], by_hand: [] });
      expect(resolvesAtRuntime(verdict.key), verdict.key).toBe(true);
    }
  });

  it("rolePhrase: every CardRole variant resolves", () => {
    const roles: CardRole[] = [
      { kind: "emu68-archive", means: [{ variant: "classic", line: "stable" }] },
      { kind: "kickstart" },
      { kind: "distro-config", name: "caffeineos" },
      { kind: "for-an-amiga-volume", what: "archive" },
      { kind: "no-place-on-a-card", what: "commodore-8bit" },
    ];
    for (const role of roles) {
      const phrase = rolePhrase(role);
      expect(isLeafKey(phrase.key), phrase.key).toBe(true);
    }
  });

  it("buildBlocker: every reason a card cannot be built resolves", () => {
    const request: CardBuildRequest = {
      archive: "Emu68-pistorm.zip",
      kickstart: null,
      dest: "card.img",
      total_bytes: 2 * 1024 * 1024 * 1024,
      boot_bytes: 0,
      label: "ART CARD",
      hardware: DEFAULT_HARDWARE,
      line: "stable",
      firmware: DEFAULT_FIRMWARE_CONFIG,
      options: DEFAULT_EMU68_OPTIONS,
      file_systems: [],
      partitions: [defaultPartition()],
    };
    const plan = {
      layout: { total_sectors: 0, boot: {}, areas: [] },
      boot_files: [],
      kernel_file: "Emu68-pistorm.gz",
      kickstart_file: null,
      rom: null,
      warnings: [],
      dest_exists: false,
    } as unknown as CardBuildPlan;

    const blockers = [
      buildBlocker({ ...request, archive: "" }, plan),
      buildBlocker({ ...request, dest: "" }, plan),
      buildBlocker({ ...request, partitions: [] }, plan),
      buildBlocker(request, null),
      buildBlocker(request, { ...plan, dest_exists: true }),
    ];
    for (const blocker of blockers) {
      expect(blocker).not.toBeNull();
      expect(isLeafKey(blocker!.key), blocker!.key).toBe(true);
    }
    expect(buildBlocker(request, plan)).toBeNull();
  });

  it("networkBlocker: every reason a card's network cannot be written resolves", () => {
    const ok = {
      tree: "E:\dist",
      wifiOn: true,
      ssid: "Tolun-Ev",
      security: "wpa" as const,
      psk: "correct-horse",
      stackOn: true,
      device: "wifipi.device",
      dhcp: true,
      ip: "",
      netmask: "",
      gateway: "",
      dns: "",
    };
    const blockers = [
      networkBlocker({ ...ok, tree: null }),
      networkBlocker({ ...ok, wifiOn: false, stackOn: false }),
      networkBlocker({ ...ok, ssid: "  " }),
      networkBlocker({ ...ok, psk: "short" }),
      networkBlocker({ ...ok, device: "" }),
      networkBlocker({ ...ok, dhcp: false }),
    ];
    for (const blocker of blockers) {
      expect(blocker).not.toBeNull();
      expect(isLeafKey(blocker!.key), blocker!.key).toBe(true);
    }
    // Nothing wrong is nothing said. And an open network needs no passphrase,
    // which is the branch that would otherwise be blocked by its own length.
    expect(networkBlocker(ok)).toBeNull();
    expect(networkBlocker({ ...ok, security: "open", psk: "" })).toBeNull();
    // A 64-character key is not a passphrase and is not measured as one.
    expect(networkBlocker({ ...ok, psk: "a".repeat(64) })).toBeNull();
    // A fixed address with every field filled in.
    expect(
      networkBlocker({
        ...ok,
        dhcp: false,
        ip: "192.168.1.50",
        netmask: "255.255.255.0",
        gateway: "192.168.1.1",
        dns: "192.168.1.1",
      })
    ).toBeNull();
  });

  it("stepPhrase: every PreloadStep variant resolves", () => {
    const steps: PreloadStep[] = [
      {
        step: "import-filesystem",
        slot: 2,
        driver: "pfs3aio.lha",
        dostype: "PDS3",
        name: "pfs3aio",
      },
      { step: "format-partition", slot: 2, index: 1, drive_name: "DH0", volume_name: "Work" },
      { step: "copy-in", slot: 2, drive_name: "DH0", source: "E:\\tree" },
    ];
    for (const step of steps) {
      expect(resolvesAtRuntime(stepPhrase(step).key), step.step).toBe(true);
    }
    expect(formatCount({ image: "card.img", steps })).toBe(1);
  });

  it("plannedToolPhrase: every PreloadStep variant resolves", () => {
    const steps: PreloadStep[] = [
      {
        step: "import-filesystem",
        slot: 2,
        driver: "pfs3aio.lha",
        dostype: "PDS3",
        name: "pfs3aio",
      },
      { step: "format-partition", slot: 2, index: 1, drive_name: "DH0", volume_name: "Work" },
      { step: "copy-in", slot: 2, drive_name: "DH0", source: "E:\\tree" },
    ];
    // This plan fills the volume it formats, so the format resolves through
    // ART-122's conditional branch.
    const paired = { image: "card.img", steps };
    for (const step of steps) {
      expect(isLeafKey(plannedToolPhrase(step, paired).key), step.step).toBe(true);
    }
    // And once more with nothing copied in, which is the *other* branch a
    // format can take — a plan of only the three steps above would never
    // reach it, leaving `preload.plan.step.tool.native` unchecked here.
    const formatOnly = steps[1];
    expect(
      isLeafKey(plannedToolPhrase(formatOnly, { image: "card.img", steps: [formatOnly] }).key),
      "format-partition, unpaired",
    ).toBe(true);
  });

  it("preloadBlocker: every reason a preload cannot run resolves", () => {
    const pick: PartitionPick = {
      area: 1,
      index: 1,
      driveName: "DH0",
      chosen: true,
      volumeName: "Work",
      content: null,
    };
    const plan: PreloadPlan = {
      image: "card.img",
      steps: [
        { step: "format-partition", slot: 2, index: 1, drive_name: "DH0", volume_name: "Work" },
      ],
    };
    // ART-117 — the one case the native path always refuses, so the plan
    // itself already shows the tool is needed.
    const importPlan: PreloadPlan = {
      image: "card.img",
      steps: [
        {
          step: "import-filesystem",
          slot: 2,
          driver: "pfs3aio.lha",
          dostype: "PDS3",
          name: "pfs3aio",
        },
        { step: "format-partition", slot: 2, index: 1, drive_name: "DH0", volume_name: "Work" },
      ],
    };
    const ready = { image: "card.img", toolPath: "hst.imager.exe", picks: [pick], plan };

    const blockers = [
      preloadBlocker({ ...ready, image: null }),
      preloadBlocker({ ...ready, picks: [{ ...pick, chosen: false }] }),
      preloadBlocker({ ...ready, picks: [{ ...pick, volumeName: " " }] }),
      preloadBlocker({ ...ready, picks: [{ ...pick, volumeName: "Work:" }] }),
      preloadBlocker({ ...ready, picks: [{ ...pick, volumeName: "W".repeat(31) }] }),
      preloadBlocker({ ...ready, plan: null }),
      // The one case where the tool is genuinely needed (ART-117) and is
      // not configured.
      preloadBlocker({ ...ready, plan: importPlan, toolPath: null }),
    ];
    for (const blocker of blockers) {
      expect(blocker).not.toBeNull();
      expect(isLeafKey(blocker!.key), blocker!.key).toBe(true);
    }
    expect(preloadBlocker(ready)).toBeNull();
    // **ART-120, mutation-checked at this layer too**: native is the
    // default, so no tool configured is not a blocker when the plan does not
    // need one — a regression back to "the tool is always required" would
    // fail this specific assertion even though every other case above still
    // passes.
    expect(preloadBlocker({ ...ready, toolPath: null })).toBeNull();
    expect(preloadBlocker({ ...ready, toolPath: "" })).toBeNull();
    // A configured tool is fine even when the plan does need it.
    expect(preloadBlocker({ ...ready, plan: importPlan })).toBeNull();
  });

  it("copiedPhrase: both the with-bytes and the without-bytes sentence resolve", () => {
    const counts = { files: 1, directories: 1, comments_lost: 0, dates_lost: 0 };
    for (const bytes of [42, null]) {
      expect(
        resolvesAtRuntime(copiedPhrase({ ...counts, bytes }).key),
        String(bytes)
      ).toBe(true);
    }
  });

  it("fallbackPhrase: every FallbackReason variant resolves", () => {
    const reasons: FallbackReason[] = [
      { reason: "foreign-rdb-embed" },
      { reason: "non-ascii-pfs3-names", paths: ["Locale/español"], more: 3 },
      { reason: "paired-with-fallback-copy", drive: "DH0" },
    ];
    for (const reason of reasons) {
      expect(resolvesAtRuntime(fallbackPhrase(reason).key), reason.reason).toBe(true);
    }
  });

  it("pairingPhrase: every Pairing variant resolves, or is deliberately null", () => {
    const pairings: Pairing[] = [
      { verdict: "paired" },
      { verdict: "suitable", rom: "kick.rom" },
      { verdict: "unsuitable", needs: 47, found: 40, rom: "kick.rom" },
      // A recipe naming a threshold other than 47 takes the other sentence —
      // the one that does not quote a message about V47.
      { verdict: "unsuitable", needs: 45, found: 40, rom: "kick.rom" },
      { verdict: "unsuitable", needs: 47, found: null, rom: "kick.rom" },
      { verdict: "not-checked", why: "tree-records-no-rom" },
      { verdict: "not-checked", why: "card-records-no-rom" },
      { verdict: "not-checked", why: "check-failed" },
    ];
    for (const pairing of pairings) {
      const phrase = pairingPhrase(pairing);
      if (phrase === null) {
        expect(pairing.verdict, pairing.verdict).toBe("paired");
      } else {
        expect(resolvesAtRuntime(phrase.key), pairing.verdict).toBe(true);
      }
    }
  });

  it("kindPhrase: every ItemKind variant resolves", () => {
    const kinds: ItemKind[] = [
      { kind: "whdload-archive", name: "Turrican" },
      { kind: "whdload-drawer", name: "Turrican" },
      { kind: "floppy-image" },
      { kind: "hard-disk-image" },
      { kind: "optical-image" },
      { kind: "archive" },
      { kind: "unknown" },
      { kind: "rom" },
      { kind: "commodore8-bit" },
    ];
    for (const kind of kinds) {
      expect(resolvesAtRuntime(kindPhrase(kind).key), kind.kind).toBe(true);
    }
  });

  it("refusalPhrase: every RefusalReason variant resolves", () => {
    const reasons: RefusalReason[] = ["belongs-on-boot-partition", "no-place-on-an-amiga-volume"];
    for (const reason of reasons) {
      expect(resolvesAtRuntime(refusalPhrase(reason).key), reason).toBe(true);
    }
  });

  it("layoutBlocker: every reason a layout cannot be applied resolves", () => {
    const plan: LayoutPlan = {
      root: "E:\\staging",
      items: [
        {
          source: "E:\\a\\Turrican.lha",
          kind: { kind: "whdload-archive", name: "Turrican" },
          destination: "Games/Turrican",
          placement: "unpack-whdload",
          bytes: 100,
        },
      ],
      refused: [],
      collisions: [],
      tooDeep: { paths: [], more: 0 },
      duplicates: { paths: [], more: 0 },
  alreadyInPlace: [],
      bytes: 100,
    };
    const ready = { root: "E:\\staging", paths: ["E:\\a"], plan };

    const blockers = [
      layoutBlocker({ ...ready, root: null }),
      layoutBlocker({ ...ready, paths: [] }),
      layoutBlocker({ ...ready, plan: null }),
      layoutBlocker({ ...ready, plan: { ...plan, items: [] } }),
      layoutBlocker({
        ...ready,
        plan: { ...plan, collisions: [{ destination: "Games/Turrican", sources: ["a", "b"] }] },
      }),
    ];
    for (const blocker of blockers) {
      expect(blocker).not.toBeNull();
      expect(isLeafKey(blocker!.key), blocker!.key).toBe(true);
    }
    expect(layoutBlocker(ready)).toBeNull();
  });

  // Fix round 1 (Task 12 review): `osinstallBlocker` and `refusalPhrase`
  // (`src/lib/osinstall.ts`) emitted keys under `osinstall.*` that were in
  // neither catalogue and never registered here — `pnpm test` passed only
  // because nothing knew these mappers existed.

  it("osinstallBlocker: every reason an install cannot run resolves", () => {
    const planned: PlanResult = {
      outcome: "planned",
      plan: {
        release: "AmigaOS 3.2",
        items: [
          {
            component: "workbench-base",
            media: "Workbench3.2",
            from: "C",
            to: "C",
            isDir: true,
            decompress: false,
            bytes: 0,
          },
        ],
        refusals: [],
        totalBytes: 0,
        totalFiles: 0,
        componentsOn: ["workbench-base"],
        mediaPaths: { Workbench3_2: "E:\\wb.adf" },
        packages: [],
        packageMedia: {},
        userStartup: [],
        activations: [],
        mediaStamps: {},
      },
    };
    const ready = {
      mediaFolder: "E:\media",
      destination: "E:\dist",
      destinationTaken: false,
      plan: planned,
      found: ["Workbench3.2"],
      releaseHolding: null,
    };

    const blockers = [
      osinstallBlocker({ ...ready, mediaFolder: null }),
      osinstallBlocker({ ...ready, destination: null }),
      osinstallBlocker({ ...ready, plan: null }),
      osinstallBlocker({
        ...ready,
        plan: { outcome: "folder-unreadable", folder: "E:\\media" },
      }),
      osinstallBlocker({
        ...ready,
        plan: {
          ...planned,
          plan: { ...planned.plan, refusals: [{ refusal: "rom-unknown" }] },
        },
      }),
      osinstallBlocker({
        ...ready,
        plan: { ...planned, plan: { ...planned.plan, items: [] } },
      }),
      osinstallBlocker({ ...ready, destinationTaken: true }),
      // ART-208's two, which only arise together: a folder holding media,
      // nothing in it this release wants, and every refusal about media.
      osinstallBlocker({
        ...ready,
        found: ["AmigaOS3.9"],
        plan: {
          ...planned,
          plan: {
            ...planned.plan,
            items: [],
            refusals: [
              { refusal: "media-missing", component: "workbench-base", volume_name: "Workbench3.2" },
            ],
          },
        },
      }),
      osinstallBlocker({
        ...ready,
        found: ["AmigaOS3.9"],
        releaseHolding: "AmigaOS 3.9",
        plan: {
          ...planned,
          plan: {
            ...planned.plan,
            items: [],
            refusals: [
              { refusal: "media-missing", component: "workbench-base", volume_name: "Workbench3.2" },
            ],
          },
        },
      }),
    ];
    for (const blocker of blockers) {
      expect(blocker).not.toBeNull();
      expect(isLeafKey(blocker!.key), blocker!.key).toBe(true);
    }
    expect(osinstallBlocker(ready)).toBeNull();
  });

  it("osinstall refusalPhrase: every RefusalReason variant resolves", () => {
    const reasons: OsInstallRefusalReason[] = [
      { refusal: "media-missing", component: "extras", volume_name: "Extras3.2" },
      { refusal: "media-path-missing", component: "extras", media: "Extras3.2", path: "L" },
      // ART-119 (#5): the disk is present and unreadable — a per-component
      // refusal now, rather than a `CoreError` that failed the whole plan.
      {
        refusal: "media-unreadable",
        component: "extras",
        volume_name: "Extras3.2",
        path: "D:\media\Extras3.2.adf",
        reason: "not an AmigaDOS volume",
      },
      { refusal: "rom-unknown" },
      { refusal: "destination-collision", path: "C/Assign", components: ["a", "b"] },
      {
        refusal: "media-ambiguous",
        component: "workbench-base",
        volume_name: "Workbench3.2",
        paths: ["a", "b"],
      },
      { refusal: "exclusive-group-conflict", group: "modules", components: ["a", "b"] },
      {
        refusal: "rule-kind-mismatch",
        component: "a",
        from: "C",
        expected: "file",
        found: "subtree",
      },
      // Task 7 (SD-2 · G5's content layer, packages): six variants
      // `plan()`'s own package block could already raise, unreachable until
      // packages became selectable.
      { refusal: "package-unknown", package: "locale-turkish" },
      { refusal: "package-folder-missing", packages: ["locale-turkish"] },
      {
        refusal: "package-requirement-missing",
        package: "boingbag-39-2",
        requires: "boingbag-39-1",
      },
      {
        refusal: "package-component-missing",
        package: "locale-turkish",
        component: "locale-base",
      },
      { refusal: "package-archive-missing", package: "locale-turkish", media: "LocaleUpdate" },
      {
        refusal: "package-archive-ambiguous",
        package: "locale-turkish",
        media: "LocaleUpdate",
        paths: ["a", "b"],
      },
      {
        refusal: "package-not-placeable-on-host",
        package: "boingbag-39-1",
        block: "encrypted-payload",
      },
      {
        refusal: "activation-source-missing",
        component: "storage",
        name: "NTSC",
        from: "Storage/Monitors/NTSC",
      },
    ];
    for (const reason of reasons) {
      const phrase = osinstallRefusalPhrase(reason);
      expect(isLeafKey(phrase.key), phrase.key).toBe(true);
    }
  });

  // ART-143: why a hand-attached picture could not be put back after the
  // artwork cache was deleted. Four values, one exhaustive `switch`, so a
  // fifth cannot arrive with its sentence missing.
  it("rebindProblemPhrase: every RebindProblem resolves", () => {
    const problems: RebindProblem[] = [
      "source-gone",
      "not-a-picture",
      "too-large",
      "unreadable",
    ];
    for (const problem of problems) {
      const phrase = rebindProblemPhrase(problem);
      expect(isLeafKey(phrase.key), phrase.key).toBe(true);
    }
  });

  // ART-119 (#2): the four reasons a conditional component's row is ticked
  // or not. This used to be four independent `&&` guards in `OsInstall.tsx`,
  // where a fifth kind would have rendered nothing at all; it is one
  // exhaustive `switch` now, and this is what proves all four of its keys
  // exist rather than only that the switch compiles.
  it("conditionalReasonText: every ConditionalReason variant resolves", () => {
    const reasons: ConditionalReason[] = [
      { kind: "rom-needed" },
      { kind: "condition-on", rom: "Kickstart 3.1", major: 40 },
      { kind: "condition-off", rom: "Kickstart 3.2", major: 47 },
      { kind: "condition-overridden", major: 47 },
    ];
    for (const reason of reasons) {
      const { phrase } = conditionalReasonText(reason);
      expect(isLeafKey(phrase.key), phrase.key).toBe(true);
    }
    // And every one of them is reachable from `conditionalReason` itself —
    // otherwise this enumerates a union the screen can never actually
    // produce, which is a fixture more helpful than reality.
    const produced = new Set(
      [
        conditionalReason(47, false, false, true, null).kind,
        conditionalReason(47, true, true, false, "Kickstart 3.1").kind,
        conditionalReason(47, true, false, false, "Kickstart 3.1").kind,
        conditionalReason(47, false, false, false, "Kickstart 3.2").kind,
      ].map(String)
    );
    expect([...produced].sort()).toEqual([
      "condition-off",
      "condition-on",
      "condition-overridden",
      "rom-needed",
    ]);
  });

  // M3, ART-166: the *other* sentence about the same fact — the one the
  // checklist row shows before the tick. Two keys, one `switch`, so a
  // second kind of block cannot arrive with one of them missing.
  it("hostPlacementBlockKey: every HostPlacementBlock resolves", () => {
    const blocks: HostPlacementBlock[] = ["encrypted-payload"];
    for (const block of blocks) {
      expect(isLeafKey(hostPlacementBlockKey(block)), hostPlacementBlockKey(block)).toBe(true);
    }
  });

  it("collisionPhrase: every Collision variant resolves", () => {
    const collisions: Collision[] = [
      { kind: "upgrade", from: "37.4", to: "45.9" },
      { kind: "downgrade", from: "45.9", to: "37.4" },
      { kind: "same-version", version: "44.1", fromBytes: 100, toBytes: 102 },
      { kind: "unversioned", fromBytes: 1024, toBytes: 2048 },
    ];
    for (const collision of collisions) {
      const phrase = collisionPhrase(collision);
      expect(isLeafKey(phrase.key), phrase.key).toBe(true);
    }
  });

  it("collisionGroupHeadingKey: every Collision kind resolves (N3, Task 7's re-review)", () => {
    // `PackagePanel.tsx`'s own group headings — unpinned by any test until
    // this, unlike every other key-returning mapper on this list.
    const kinds: Collision["kind"][] = ["downgrade", "upgrade", "same-version", "unversioned"];
    for (const kind of kinds) {
      const key = collisionGroupHeadingKey(kind);
      expect(isLeafKey(key), key).toBe(true);
    }
  });

  it("provenancePhrase: every Provenance variant resolves", () => {
    // `ALL_PROVENANCES` mirrors `core::gameindex::record::Provenance`, and
    // `gameindex.test.ts` pins that list to the four the core defines. What
    // *this* test adds is the half that file cannot check: that each key it
    // builds is a real leaf in the catalogue rather than a plausible-looking
    // dotted string.
    for (const from of ALL_PROVENANCES) {
      const phrase = provenancePhrase(from);
      expect(isLeafKey(phrase.key), phrase.key).toBe(true);
    }
  });
  it("sourcePhrase: every shipped source, and an unknown one, resolve", () => {
    // The ids are the strings `core::artwork::config::source_for` matches on.
    // The default arm matters as much as the named ones: a settings file
    // carrying a source a later ART dropped must still render a row.
    for (const id of ["libretro", "whdload-de", "something-else"]) {
      const phrase = sourcePhrase(id);
      expect(isLeafKey(phrase.key), phrase.key).toBe(true);
    }
  });
  it("outcomePhrase: both branches resolve", () => {
    const outcomes: SourceOutcome[] = [
      { id: "libretro", written: 12, adopted: 0, matched: 14, missed: 2, reachable: true, note: null },
      // The branch a resumed run takes — pictures already on disk from a run
      // that never got to save its index.
      { id: "libretro", written: 3, adopted: 790, matched: 5, missed: 2, reachable: true, note: null },
      {
        id: "whdload-de",
        written: 0,
        adopted: 0,
        matched: 0,
        missed: 0,
        reachable: false,
        note: "404 https://www.whdload.de/",
      },
    ];
    for (const outcome of outcomes) {
      const phrase = outcomePhrase(outcome);
      expect(isLeafKey(phrase.key), phrase.key).toBe(true);
    }
  });
  it("checksumBadge: every verdict, and both readings of not-checked", () => {
    const verdicts: RomChecksum[] = ["valid", "invalid", "not-checked"];
    for (const checksum of verdicts) {
      for (const rom of [
        { checksum, is_cloanto: false, key_available: false },
        // The encrypted-with-no-key reading of `not-checked`, which is a
        // different sentence from "not a Kickstart" (ART-138).
        { checksum, is_cloanto: true, key_available: false },
      ]) {
        const { phrase } = checksumBadge(rom);
        expect(isLeafKey(phrase.key), phrase.key).toBe(true);
      }
    }
  });

  it("mediaPhrase: every medium resolves", () => {
    const media: Media[] = [
      { kind: "floppies", ordered: ["a.adf"] },
      { kind: "hardfile", file: "a.hdf" },
      { kind: "whdload-hardfile", file: "a.hdf", slave: "a.slave" },
    ];
    for (const one of media) {
      const phrase = mediaPhrase(one);
      expect(resolvesAtRuntime(phrase.key), phrase.key).toBe(true);
    }
  });

  // Collection wave C, Task 6b: the detail panel's picture switch labels each
  // button with `kindPhrase` (`@/lib/collectionDetail`) — not the identically
  // named mapper from `@/lib/layout` above, which is a different `kindPhrase`
  // over a different union and already enumerated by its own test.
  it("collectionDetail's kindPhrase: every ArtKind resolves", () => {
    const kinds: ArtKind[] = ["boxart", "snap", "title", "logo", "icon"];
    for (const kind of kinds) {
      const phrase = artworkKindPhrase(kind);
      expect(isLeafKey(phrase.key), phrase.key).toBe(true);
    }
  });

  // Collection wave C, Task 11 (Play): the four mappers in `@/lib/launch`
  // that turn `core::launch`'s decision into what the confirmation panel
  // shows.

  it("launch machinePhrase: every Machine resolves", () => {
    const machines: Machine[] = ["a500", "a1200"];
    for (const machine of machines) {
      expect(isLeafKey(machinePhrase(machine).key), machine).toBe(true);
    }
  });

  it("launch refusalPhrase: every LaunchRefusal variant resolves", () => {
    const refusals: LaunchRefusal[] = [
      { kind: "no-suitable-rom", machine: "a1200" },
      { kind: "no-rom-meets-whdload-minimum", machine: "a500" },
      { kind: "no-whdload-machine-rom", machine: "a1200" },
      { kind: "no-system-volume" },
      { kind: "file-missing", path: "D:\\g\\a.adf" },
      { kind: "nothing-to-mount" },
    ];
    for (const refusal of refusals) {
      const phrase = launchRefusalPhrase(refusal);
      expect(isLeafKey(phrase.key), phrase.key).toBe(true);
    }
  });

  it("launch notePhrase: the only LaunchNote variant resolves", () => {
    const note: LaunchNote = { kind: "more-disks-than-drives", total: 6, mounted: 4 };
    expect(isLeafKey(notePhrase(note).key)).toBe(true);
  });

  it("launch mountNotePhrase: every MountNote variant resolves", () => {
    const mounts: MountNote[] = [
      { kind: "floppies", count: 2 },
      { kind: "hardfile", read_only: true },
      { kind: "hardfile", read_only: false },
      { kind: "whdload-hardfile", read_only: true },
      { kind: "whdload-hardfile", read_only: false },
      { kind: "whdload", one_click: true },
      { kind: "whdload", one_click: false },
    ];
    for (const mount of mounts) {
      const phrase = mountNotePhrase(mount);
      // `resolvesAtRuntime`, not `isLeafKey`: the floppies variant carries a
      // count and is pluralised (`…_one`/`…_other`), the same reason
      // `launchKindPhrase`'s own floppies case above uses it.
      expect(resolvesAtRuntime(phrase.key), phrase.key).toBe(true);
    }
  });

  it("launchKindPhrase: every LaunchKind variant resolves", () => {
    const kinds: LaunchKind[] = [
      { kind: "floppies", images: ["a.adf"] },
      { kind: "hardfile", image: "a.hdf" },
      {
        kind: "whdload",
        drawer: "D:\\games\\Turrican",
        slave: "Turrican.slave",
        system: "E:\\amikit\\AmiKit.hdf",
        one_click: true,
      },
      {
        kind: "whdload",
        drawer: "D:\\games\\Turrican",
        slave: "Turrican.slave",
        system: "E:\\amikit\\AmiKit.hdf",
        one_click: false,
      },
    ];
    for (const kind of kinds) {
      expect(resolvesAtRuntime(launchKindPhrase(kind).key), kind.kind).toBe(true);
    }
  });

  it("stepLabelKey: every OS Builder step resolves", () => {
    // The progress strip builds its labels from the step id — a template, not
    // a literal — so nothing in the build would catch a step whose label was
    // never added: the strip would simply render `osBuilder.step.kart` at the
    // user. Every id is enumerated here, and `stepsFor` cannot return one
    // outside `STEP_IDS` (its own test asserts that), so this covers the
    // strip whatever kind is being built.
    for (const step of STEP_IDS) {
      expect(resolvesAtRuntime(stepLabelKey(step)), step).toBe(true);
    }
  });
});
