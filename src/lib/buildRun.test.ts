// The build run's phases, and the five endings each one can have.
//
// Two things have to be true of every sentence this module can produce: it is
// **its own** sentence (no two endings share a key), and it resolves in
// **both** catalogues. A `Phrase` whose key is not a leaf in `en.json` renders
// the raw dotted key on screen; one in `en.json` and not in `tr.json` renders
// English to a Turkish user. Neither fails to compile, and `parity.test.ts`
// compares the catalogues to each other rather than to this file — so, as in
// `chain.test.ts`, the key check is made here.

import { describe, expect, it } from "vitest";

import en from "@/i18n/en.json";
import tr from "@/i18n/tr.json";
import {
  FIRST_BOOT_PHASE_NAME,
  folderOf,
  phaseNextStepPhrase,
  phaseOutcomePhrase,
  phaseTone,
  runSummaryLines,
  sequenceFor,
  type Phase,
  type PhaseEnding,
  type PhaseReport,
  type SequenceInputs,
} from "@/lib/buildRun";
import type { ApplyOutcome, InstallPlan, TreeSummary } from "@/lib/osinstall";
import type { FirstBootWritten } from "@/lib/firstboot";

/** Whether `dotted` names a string leaf in `catalogue` — i18next's plural
 *  suffixes included, since `t("…updates", { count })` never resolves a bare
 *  `updates` leaf. */
function isLeafKey(catalogue: unknown, dotted: string): boolean {
  const leaf = (path: string): boolean => {
    let node: unknown = catalogue;
    for (const part of path.split(".")) {
      if (typeof node !== "object" || node === null) return false;
      node = (node as Record<string, unknown>)[part];
    }
    return typeof node === "string";
  };
  return leaf(dotted) || (leaf(`${dotted}_one`) && leaf(`${dotted}_other`));
}

function leafText(catalogue: unknown, dotted: string): string {
  let node: unknown = catalogue;
  for (const part of dotted.split(".")) {
    node = (node as Record<string, unknown>)[part];
  }
  if (typeof node !== "string") throw new Error(`${dotted} is not a string leaf`);
  return node;
}

/** Every interpolation variable a catalogue entry asks for. */
function varsOf(text: string): string[] {
  return [...text.matchAll(/{{\s*(\w+)/g)].map((m) => m[1]).sort();
}

function hasLeaf(catalogue: unknown, dotted: string): boolean {
  let node: unknown = catalogue;
  for (const part of dotted.split(".")) {
    if (typeof node !== "object" || node === null) return false;
    node = (node as Record<string, unknown>)[part];
  }
  return typeof node === "string";
}

/** Both catalogues carry the key, and every param the phrase supplies is one
 *  the sentence actually asks for in both languages. A supplied param the
 *  sentence does not name is a value nobody sees; a named one nobody supplies
 *  renders as the literal `{{files}}` on screen. */
function expectRenderable(phrase: { key: string; params?: Record<string, string | number> }) {
  expect(isLeafKey(en, phrase.key), `${phrase.key} in en`).toBe(true);
  expect(isLeafKey(tr, phrase.key), `${phrase.key} in tr`).toBe(true);
  const supplied = Object.keys(phrase.params ?? {}).sort();
  for (const [name, catalogue] of [
    ["en", en],
    ["tr", tr],
  ] as const) {
    // Plural keys live under their suffixes; either spelling counts.
    const key = hasLeaf(catalogue, phrase.key) ? phrase.key : `${phrase.key}_other`;
    expect(varsOf(leafText(catalogue, key)), `${phrase.key} vars in ${name}`).toEqual(supplied);
  }
}

const INPUTS: SequenceInputs = {
  destination: "E:\\amiga\\dist",
  destinationIsTree: false,
  release: "AmigaOS 3.9",
  updates: [
    {
      packageId: "boingbag-39-1",
      slotId: "package:boingbag-39-1",
      name: "BoingBag 3.9-1",
      file: "E:\\amiga\\arsiv\\BoingBag39-1.lha",
    },
    {
      packageId: "boingbag-39-2",
      slotId: "package:boingbag-39-2",
      name: "BoingBag 3.9-2",
      file: "E:\\amiga\\arsiv\\BoingBag39-2.lha",
    },
  ],
  firstBootWanted: true,
};

const APPLIED: ApplyOutcome = {
  root: "E:\\amiga\\dist",
  files: 1242,
  directories: 105,
  bytes: 18_300_000,
  removed: [],
  icons: [],
  iconMergeFailures: 0,
  extraMembers: [],
  postPlace: [],
};

const WRITTEN: FirstBootWritten = {
  files: ["S/ART-FirstBoot", "S/ART-FirstBoot-Step", "S/FirstBoot/10-hardware"],
  userStartupBackup: null,
  userStartupCreated: false,
};

function phaseOf(kind: Phase["kind"]): Phase {
  const phase = sequenceFor(INPUTS).find((p) => p.kind === kind);
  if (!phase) throw new Error(`no ${kind} phase`);
  return phase;
}

function report(kind: Phase["kind"], ending: PhaseEnding): PhaseReport {
  return { phase: phaseOf(kind), ending };
}

/** One ending of each state, for a kind. `succeeded` carries the outcome the
 *  kind actually produces — an `ApplyOutcome` for the two job phases, a
 *  `FirstBootWritten` for the plain call. */
function everyEnding(kind: Phase["kind"]): PhaseEnding[] {
  return [
    { state: "pending" },
    { state: "running", progress: null },
    {
      state: "succeeded",
      outcome: kind === "firstboot" ? WRITTEN : APPLIED,
      elapsedMs: 4200,
    },
    { state: "refused", refusals: [{ refusal: "rom-unknown" }] },
    {
      state: "failed",
      message: "the disc changed since the preview",
      errorCode: "ART-101",
      done: 17,
      total: 1980,
    },
    { state: "cancelled", filesLanded: 33 },
    { state: "not-attempted" },
  ];
}

const KINDS: Phase["kind"][] = ["tree", "package", "firstboot"];

describe("sequenceFor", () => {
  it("puts the tree first, the updates in the order given, and first boot last", () => {
    const phases = sequenceFor(INPUTS);
    expect(phases.map((p) => p.kind)).toEqual(["tree", "package", "package", "firstboot"]);
    expect(phases.map((p) => p.name)).toEqual([
      "AmigaOS 3.9",
      "BoingBag 3.9-1",
      "BoingBag 3.9-2",
      FIRST_BOOT_PHASE_NAME,
    ]);
    expect(phases.map((p) => p.id)).toEqual([
      "tree",
      "package:boingbag-39-1",
      "package:boingbag-39-2",
      "firstboot",
    ]);
  });

  it("has no tree phase when the destination is already a tree", () => {
    // The ruling this whole update mode rests on: `osinstall_apply` refuses a
    // destination with anything in it, so an existing tree is *updated*, and
    // a tree phase would be a refusal ART walked into on purpose.
    const phases = sequenceFor({ ...INPUTS, destinationIsTree: true });
    expect(phases.map((p) => p.kind)).toEqual(["package", "package", "firstboot"]);
    expect(phases.some((p) => p.kind === "tree")).toBe(false);
  });

  it("carries each update's package, slot, file and folder", () => {
    const [, first] = sequenceFor(INPUTS);
    expect(first.packageId).toBe("boingbag-39-1");
    expect(first.slotId).toBe("package:boingbag-39-1");
    expect(first.file).toBe("E:\\amiga\\arsiv\\BoingBag39-1.lha");
    expect(first.folder).toBe("E:\\amiga\\arsiv");
  });

  it("adds the first-boot phase only when it is wanted", () => {
    expect(sequenceFor({ ...INPUTS, firstBootWanted: false }).map((p) => p.kind)).toEqual([
      "tree",
      "package",
      "package",
    ]);
  });

  it("is a tree alone when nothing is ticked and first boot is off", () => {
    expect(
      sequenceFor({ ...INPUTS, updates: [], firstBootWanted: false }).map((p) => p.id)
    ).toEqual(["tree"]);
  });

  it("can be empty — an existing tree with nothing asked of it", () => {
    expect(
      sequenceFor({ ...INPUTS, destinationIsTree: true, updates: [], firstBootWanted: false })
    ).toEqual([]);
  });
});

describe("folderOf", () => {
  it("takes the folder off a Windows path and off a POSIX one", () => {
    expect(folderOf("E:\\amiga\\arsiv\\BoingBag39-1.lha")).toBe("E:\\amiga\\arsiv");
    expect(folderOf("/home/x/arsiv/BoingBag39-1.lha")).toBe("/home/x/arsiv");
  });

  it("is null when there is no folder to take", () => {
    expect(folderOf("BoingBag39-1.lha")).toBeNull();
    expect(folderOf("/BoingBag39-1.lha")).toBeNull();
  });
});

describe("phaseOutcomePhrase", () => {
  it("gives every kind and every ending its own sentence, in both catalogues", () => {
    const seen = new Map<string, string>();
    for (const kind of KINDS) {
      for (const ending of everyEnding(kind)) {
        const phrase = phaseOutcomePhrase(report(kind, ending));
        expectRenderable(phrase);
        const owner = `${kind}/${ending.state}`;
        // `running` and `pending` are the same sentence for every kind — they
        // say what the row is doing, and the row's own name is in the params.
        // Every *ending* is per kind, and no two share a key.
        if (ending.state !== "running" && ending.state !== "pending") {
          expect(seen.has(phrase.key), `${phrase.key} already used by ${seen.get(phrase.key)}`).toBe(
            false
          );
          seen.set(phrase.key, owner);
        }
      }
    }
    // 3 kinds × 5 endings.
    expect(seen.size).toBe(15);
  });

  it("counts the tree's files and bytes from the outcome", () => {
    const phrase = phaseOutcomePhrase(
      report("tree", { state: "succeeded", outcome: APPLIED, elapsedMs: 1 })
    );
    expect(phrase.key).toBe("osBuilder.build.phase.tree.succeeded");
    expect(phrase.params).toEqual({ files: 1242, bytes: "17.5 MB" });
  });

  it("counts a package's own files, and names the package", () => {
    const phrase = phaseOutcomePhrase(
      report("package", { state: "succeeded", outcome: APPLIED, elapsedMs: 1 })
    );
    expect(phrase.params).toEqual({ name: "BoingBag 3.9-1", files: 1242, bytes: "17.5 MB" });
  });

  it("counts the first-boot files it actually wrote", () => {
    const phrase = phaseOutcomePhrase(
      report("firstboot", { state: "succeeded", outcome: WRITTEN, elapsedMs: 1 })
    );
    expect(phrase.key).toBe("osBuilder.build.phase.firstboot.succeeded");
    expect(phrase.params).toEqual({ files: 3 });
  });

  it("says the core's own words when a phase failed", () => {
    const phrase = phaseOutcomePhrase(
      report("tree", {
        state: "failed",
        message: "the disc changed since the preview",
        errorCode: "ART-101",
        done: 17,
        total: 1980,
      })
    );
    expect(phrase.key).toBe("osBuilder.build.phase.tree.failed");
    expect(phrase.params).toEqual({ message: "the disc changed since the preview" });
  });

  it("says which phase is running, by name", () => {
    const phrase = phaseOutcomePhrase(report("package", { state: "running", progress: null }));
    expect(phrase.key).toBe("osBuilder.build.phase.running");
    expect(phrase.params).toEqual({ name: "BoingBag 3.9-1" });
  });

  it("keeps *not attempted* apart from every other ending", () => {
    // The whole reason the fifth ending exists: a row after the one that
    // stopped the run was never tried, and saying "failed" about it would be
    // ART claiming something it never did.
    const notAttempted = phaseOutcomePhrase(report("package", { state: "not-attempted" }));
    const cancelled = phaseOutcomePhrase(report("package", { state: "cancelled", filesLanded: 0 }));
    expect(notAttempted.key).toBe("osBuilder.build.phase.package.notAttempted");
    expect(notAttempted.key).not.toBe(cancelled.key);
  });
});

describe("phaseNextStepPhrase", () => {
  it("tells a refused phase what to do about it, in both catalogues", () => {
    const phrase = phaseNextStepPhrase(
      report("package", { state: "refused", refusals: [{ refusal: "rom-unknown" }] }),
      "E:\\amiga\\dist"
    );
    expect(phrase?.key).toBe("osBuilder.build.next.refused");
    expectRenderable(phrase!);
  });

  it("names the tree and how far the work got when a phase failed", () => {
    // **Items, not files** (round 4 task 4). A failed `JobState` carries an
    // error code and a message and no count at all; the only number ART holds
    // is the progress stream's own `done` of `total`, which counts whole plan
    // items — directories included. So the sentence says what was measured.
    const phrase = phaseNextStepPhrase(
      report("tree", {
        state: "failed",
        message: "no",
        errorCode: null,
        done: 17,
        total: 1980,
      }),
      "E:\\amiga\\dist"
    );
    expect(phrase?.key).toBe("osBuilder.build.next.failed");
    expect(phrase?.params).toEqual({ root: "E:\\amiga\\dist", done: 17, total: 1980 });
    expectRenderable(phrase!);
    // Both languages have to *say* all three — a next step that drops the
    // root tells somebody their files are somewhere.
    for (const catalogue of [en, tr]) {
      expect(varsOf(leafText(catalogue, "osBuilder.build.next.failed"))).toEqual([
        "done",
        "root",
        "total",
      ]);
    }
  });

  it("claims no count at all when the job never reported one", () => {
    // The command threw before a job existed, so nothing was measured. The
    // old sentence printed `0` here — a confident claim that nothing was
    // written, which is the defect this project is named for.
    const phrase = phaseNextStepPhrase(
      report("tree", { state: "failed", message: "no", errorCode: null, done: null, total: null }),
      "E:\\amiga\\dist"
    );
    expect(phrase?.key).toBe("osBuilder.build.next.failedUnknown");
    expect(phrase?.params).toEqual({ root: "E:\\amiga\\dist" });
    expectRenderable(phrase!);
    for (const catalogue of [en, tr]) {
      expect(varsOf(leafText(catalogue, "osBuilder.build.next.failedUnknown"))).toEqual(["root"]);
    }
  });

  it("claims no count when the job counted but named no total", () => {
    // `JobProgress.total` is nullable, and "item 17 of null" is not a
    // sentence. One arm rather than a third key.
    const phrase = phaseNextStepPhrase(
      report("tree", { state: "failed", message: "no", errorCode: null, done: 17, total: null }),
      "E:\\amiga\\dist"
    );
    expect(phrase?.key).toBe("osBuilder.build.next.failedUnknown");
  });

  it("names the tree and the count a cancelled phase left behind", () => {
    const phrase = phaseNextStepPhrase(
      report("package", { state: "cancelled", filesLanded: 33 }),
      "E:\\amiga\\dist"
    );
    expect(phrase?.key).toBe("osBuilder.build.next.cancelled");
    expect(phrase?.params).toEqual({ root: "E:\\amiga\\dist", files: 33 });
    expectRenderable(phrase!);
    for (const catalogue of [en, tr]) {
      expect(varsOf(leafText(catalogue, "osBuilder.build.next.cancelled"))).toEqual([
        "files",
        "root",
      ]);
    }
  });

  it("has no next step for the endings that need none", () => {
    for (const ending of [
      { state: "pending" } as const,
      { state: "running", progress: null } as const,
      { state: "succeeded", outcome: APPLIED, elapsedMs: 1 } as const,
      { state: "not-attempted" } as const,
    ]) {
      expect(phaseNextStepPhrase(report("tree", ending), "E:\\amiga\\dist")).toBeNull();
    }
  });
});

describe("phaseTone", () => {
  it("colours each ending for what it is", () => {
    expect(phaseTone(report("tree", { state: "pending" }))).toBe("muted");
    expect(phaseTone(report("tree", { state: "running", progress: null }))).toBe("muted");
    expect(phaseTone(report("tree", { state: "not-attempted" }))).toBe("muted");
    expect(phaseTone(report("tree", { state: "succeeded", outcome: APPLIED, elapsedMs: 1 }))).toBe(
      "ok"
    );
    expect(
      phaseTone(report("tree", { state: "refused", refusals: [{ refusal: "rom-unknown" }] }))
    ).toBe("warn");
    expect(phaseTone(report("tree", { state: "cancelled", filesLanded: 0 }))).toBe("warn");
    expect(
      phaseTone(
        report("tree", { state: "failed", message: "no", errorCode: null, done: null, total: null })
      )
    ).toBe("err");
  });
});

const PLAN: InstallPlan = {
  release: "AmigaOS 3.9",
  items: [],
  refusals: [],
  totalBytes: 18_300_000,
  totalFiles: 1242,
  componentsOn: ["workbench-base", "locale"],
  mediaPaths: {},
  packages: [],
  packageMedia: {},
  userStartup: [],
  activations: [],
  mediaStamps: {},
  removals: [],
  layers: [],
};

const TREE: TreeSummary = {
  isTree: true,
  release: "AmigaOS 3.9",
  files: 1242,
  components: ["workbench-base"],
  amigaInstalled: [],
  problem: null,
};

const SUMMARY_ARGS = {
  plan: PLAN,
  destinationIsTree: false,
  treeSummary: null,
  updates: INPUTS.updates,
  firstBootWanted: true,
  replaces: { fresh: 41, unchanged: 3, replaced: 0 },
};

describe("runSummaryLines", () => {
  it("is four lines for a fresh build, all four in both catalogues", () => {
    const lines = runSummaryLines(SUMMARY_ARGS);
    expect(lines).toHaveLength(4);
    expect(lines.map((l) => l.key)).toEqual([
      "osBuilder.build.summary.tree",
      "osBuilder.build.summary.updates",
      "osBuilder.build.summary.firstboot",
      "osBuilder.build.summary.replaces",
    ]);
    for (const line of lines) expectRenderable(line);
  });

  it("takes the first line's numbers from the plan itself", () => {
    const [first] = runSummaryLines(SUMMARY_ARGS);
    expect(first.params).toEqual({
      files: 1242,
      bytes: "17.5 MB",
      components: "workbench-base, locale",
    });
  });

  it("says an existing tree is being updated, not built", () => {
    const [first] = runSummaryLines({
      ...SUMMARY_ARGS,
      destinationIsTree: true,
      treeSummary: TREE,
    });
    expect(first.key).toBe("osBuilder.build.summary.treeExisting");
    expect(first.params).toEqual({ release: "AmigaOS 3.9", count: 1242 });
    expectRenderable(first);
  });

  it("has no first line when there is nothing to say one from", () => {
    // No plan yet is not "0 files": the plan's own refusal stands in the
    // Build button's place, and inventing a line here would out-claim it.
    const lines = runSummaryLines({ ...SUMMARY_ARGS, plan: null });
    expect(lines).toHaveLength(3);
    expect(lines[0].key).toBe("osBuilder.build.summary.updates");

    const asTree = runSummaryLines({
      ...SUMMARY_ARGS,
      destinationIsTree: true,
      treeSummary: null,
    });
    expect(asTree).toHaveLength(3);
  });

  it("lists the updates in the order they will run", () => {
    const [, updates] = runSummaryLines(SUMMARY_ARGS);
    expect(updates.params).toEqual({ count: 2, names: "BoingBag 3.9-1 → BoingBag 3.9-2" });
  });

  it("says so when no update is ticked", () => {
    const lines = runSummaryLines({ ...SUMMARY_ARGS, updates: [] });
    expect(lines[1].key).toBe("osBuilder.build.summary.updatesNone");
    expectRenderable(lines[1]);
  });

  it("says whether first boot is on or off — never nothing", () => {
    expect(runSummaryLines(SUMMARY_ARGS)[2].key).toBe("osBuilder.build.summary.firstboot");
    const off = runSummaryLines({ ...SUMMARY_ARGS, firstBootWanted: false })[2];
    expect(off.key).toBe("osBuilder.build.summary.firstbootOff");
    expectRenderable(off);
  });

  it("counts what would be replaced, and says *not previewed* when it has not been", () => {
    const [, , , replaces] = runSummaryLines(SUMMARY_ARGS);
    expect(replaces.params).toEqual({ replaced: 0, fresh: 41, unchanged: 3 });

    const pending = runSummaryLines({ ...SUMMARY_ARGS, replaces: null })[3];
    expect(pending.key).toBe("osBuilder.build.summary.replacesPending");
    expect(pending.params).toBeUndefined();
    expectRenderable(pending);
  });
});
