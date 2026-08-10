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
import { describeUpdate, type PackageUpdate } from "@/lib/sources";
import { describeLayout, type ImageLayout } from "@/lib/volume";
import { describeVerdict, describeOutcome, type WhdloadOutcome, type WhdloadVerdict } from "@/lib/whdload";

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
      { ...base, state: { state: "cancelled" } },
      { ...base, state: { state: "failed", error_code: "ART-001", message: "x" } },
    ];
    for (const job of jobs) {
      const phrase = jobStatusLabel(job);
      expect(isLeafKey(phrase.key), phrase.key).toBe(true);
    }
  });
});
