// The discriminated-union mappers in `src/lib/*` return a `Phrase` — a
// catalogue key plus params — instead of a string, so a component can
// translate it (see `src/lib/phrase.ts`). Nothing in the build catches a
// `Phrase` pointing at a key nobody added: it would just render the raw key
// string on screen. This test enumerates every variant of every such mapper
// and asserts its key resolves to an actual leaf in en.json.

import { describe, expect, it } from "vitest";
import en from "./en.json";

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
  fallbackPhrase,
  formatCount,
  preloadBlocker,
  stepPhrase,
  type FallbackReason,
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
  osinstallBlocker,
  refusalPhrase as osinstallRefusalPhrase,
  type PlanResult,
  type RefusalReason as OsInstallRefusalReason,
} from "@/lib/osinstall";

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
    };
    const refusals: MoveInput[] = [
      { ...base, entries: [] },
      { ...base, sourceKind: "local", targetKind: "adf" },
      { ...base, sourceKind: "iso" },
      { ...base, sourceWritable: false },
      { ...base, targetKind: "archive" },
      { ...base, targetKind: "hdf", targetWritable: false },
      {
        ...base,
        targetKind: "hdf",
        entries: [
          { name: "Lotus", isDir: true },
          { name: "Turrican", isDir: true },
        ],
      },
      { ...base, targetKind: "hdf", entries: [{ name: "Readme", isDir: false }] },
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

  it("fallbackPhrase: every FallbackReason variant resolves", () => {
    const reasons: FallbackReason[] = [
      { reason: "foreign-rdb-embed" },
      { reason: "non-ascii-pfs3-names", paths: ["Locale/español"], more: 3 },
    ];
    for (const reason of reasons) {
      expect(resolvesAtRuntime(fallbackPhrase(reason).key), reason.reason).toBe(true);
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
            bytes: 0,
          },
        ],
        refusals: [],
        totalBytes: 0,
        componentsOn: ["workbench-base"],
        mediaPaths: { Workbench3_2: "E:\\wb.adf" },
        userStartup: [],
      },
    };
    const ready = { mediaFolder: "E:\\media", destination: "E:\\dist", plan: planned };

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
    ];
    for (const reason of reasons) {
      const phrase = osinstallRefusalPhrase(reason);
      expect(isLeafKey(phrase.key), phrase.key).toBe(true);
    }
  });
});
