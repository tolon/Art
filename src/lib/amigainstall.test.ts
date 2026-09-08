// The wire between `commands/amigainstall.rs` and this module.
//
// A TypeScript type is erased at run time, so nothing in the build notices
// when a Rust enum gains a variant and the union here does not — the screen
// would then fall through every branch it knows and say nothing at all about
// an ending that really happened. This reads both sources as text and checks
// the two lists against each other, the same way `osinstall.test.ts` checks
// the screen's assumptions against every shipped recipe.
//
// It deliberately does **not** re-derive the JSON shape: the Rust side pins
// that exactly (`every_run_outcome_has_the_shape_the_frontend_reads`,
// `settlement_is_camel_case_on_the_wire_including_inside_a_variant`). What
// only this side can see is whether the frontend knows about all of it.

import { describe, expect, it } from "vitest";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";

import { AMIGA_INSTALL_EVENT } from "@/lib/amigainstall";

const SRC = resolve(__dirname, "..", "..", "src-tauri", "src");
const CORE = readFileSync(resolve(SRC, "core", "amigainstall", "mod.rs"), "utf8");
const COMMAND = readFileSync(resolve(SRC, "commands", "amigainstall.rs"), "utf8");
const WRAPPER = readFileSync(resolve(__dirname, "amigainstall.ts"), "utf8");

/** The body of a `pub enum <name> { … }`, up to its closing brace at column 0. */
function enumBody(source: string, name: string): string {
  const start = source.indexOf(`pub enum ${name} {`);
  expect(start, `${name} must exist in the Rust`).toBeGreaterThan(-1);
  const end = source.indexOf("\n}", start);
  return source.slice(start, end);
}

/** Variant identifiers, ignoring doc comments and attributes. */
function variants(body: string): string[] {
  return body
    .split("\n")
    .map((line) => line.trim())
    .filter((line) => /^[A-Z][A-Za-z0-9]*\s*(\{|,|$)/.test(line))
    .map((line) => line.replace(/[^A-Za-z0-9].*$/, ""));
}

/** `EmulatorClosed` → `emulator-closed`, which is what `rename_all` does. */
function kebab(variant: string): string {
  return variant
    .replace(/([a-z0-9])([A-Z])/g, "$1-$2")
    .toLowerCase();
}

/** Every `kind: "…"` the TypeScript union declares, between two markers. */
function kindsBetween(from: string, to: string): string[] {
  const start = WRAPPER.indexOf(from);
  const end = WRAPPER.indexOf(to);
  expect(start, `${from} must exist`).toBeGreaterThan(-1);
  expect(end).toBeGreaterThan(start);
  return [...WRAPPER.slice(start, end).matchAll(/kind: "([a-z-]+)"/g)].map((m) => m[1]);
}

describe("the four endings reach the frontend", () => {
  it("declares every RunOutcome the Rust has, under the same tag", () => {
    const rust = variants(enumBody(CORE, "RunOutcome")).map(kebab);
    const ts = kindsBetween("export type RunOutcome", "export type SettlementReport");

    expect(rust).toEqual(["succeeded", "failed", "timed-out", "emulator-closed"]);
    expect([...ts].sort()).toEqual([...rust].sort());
  });

  it("declares every SettlementReport the Rust has", () => {
    const rust = variants(enumBody(COMMAND, "SettlementReport")).map(kebab);
    const ts = kindsBetween("export type SettlementReport", "export interface AmigaInstallRequest");

    expect(rust).toEqual(["promoted", "kept"]);
    expect([...ts].sort()).toEqual([...rust].sort());
  });

  it("carries the overlay a BoingBag 1 run may need (ART-186)", () => {
    // The wrapper takes a **list**, not one path: BoingBag 3.9-1's own
    // `Updater` is 45.13 and cannot install under an emulator, so a second
    // archive supplies 45.15. A TypeScript declaration still saying `string`
    // would make the one screen that matters unable to offer the second file.
    expect(WRAPPER).toContain("packageArchives: string[];");
    // And the preview says what that second file would have to be, and why —
    // so the screen can name it before the user goes looking.
    expect(COMMAND).toMatch(/pub declared_overlays: Vec<String>,/);
    expect(COMMAND).toMatch(/pub minimum_installer_version: Option<String>,/);
    expect(WRAPPER).toContain("declaredOverlays: string[];");
    expect(WRAPPER).toContain("minimumInstallerVersion: string | null;");
    expect(WRAPPER).toContain("packageArchivesPresent: boolean;");
  });

  it("names the same event as the Rust", () => {
    const declared = COMMAND.match(/AMIGA_INSTALL_EVENT: &str = "([^"]+)"/);
    expect(declared?.[1]).toBe(AMIGA_INSTALL_EVENT);
  });

  it("asks for the package's own archive, which ART-185 made required", () => {
    // The Rust field is not optional — no `#[serde(default)]`, no
    // `Option` — so a request without it fails to deserialise and the
    // command rejects before anything runs. TypeScript is the only place
    // that can say so *before* the call, and only if the field is declared
    // required here too. Without the archive the installer is on no mounted
    // volume and the run reports that it said no about a program that never
    // started, which is the whole of ART-185.
    expect(COMMAND).toMatch(/pub package_archives: Vec<PathBuf>,/);
    expect(COMMAND).not.toMatch(/#\[serde\(default\)\]\s*\r?\n\s*pub package_archives/);
    expect(WRAPPER).toContain("packageArchives: string[];");
    expect(WRAPPER).not.toContain("packageArchives?");

    // And the third volume the run mounts is named on both sides, because
    // the user will see it on the Workbench (design §4).
    const declared = CORE.match(/PACKAGE_VOLUME: &str = "([^"]+)"/);
    expect(declared?.[1]).toBeTruthy();
    expect(WRAPPER).toContain("packageVolume: string;");
  });

  it("keeps the fields a struct variant carries, which serde does not rename for free", () => {
    // `#[serde(rename_all)]` on an enum renames variants, not the fields of a
    // struct variant, so `leftBehind` only arrives camelCased because the
    // variant carries its own attribute. If that attribute is ever dropped
    // the Rust test fails; if the field is dropped here, the screen silently
    // stops telling the user about a tree it could not remove.
    expect(COMMAND).toMatch(/#\[serde\(rename_all = "camelCase"\)\]\s*\r?\n\s*Promoted \{/);
    expect(WRAPPER).toContain("leftBehind: string | null");
  });
});

// ---------------------------------------------------------------------------
// The sentences the screen says
// ---------------------------------------------------------------------------
//
// Pure mappers, tested here rather than through the panel, for the reason
// `OsInstall.tsx`'s own review gave about the Critical it shipped: "no test
// can reach [it], because it lives inside the component."

import {
  amigaInstallArchiveKey,
  archiveFieldBlockerPhrase,
  dedupeBlockers,
  outcomeNextStepPhrase,
  outcomePhrase,
  outcomeTone,
  overlayAdvicePhrase,
  parseClassification,
  readinessBlockers,
  settlementPhrase,
  waitedSeconds,
  type AmigaInstallPreview,
  type ArchiveClassification,
  type RunOutcome,
  type SettlementReport,
} from "@/lib/amigainstall";

/** The four endings, each exactly as the Rust writes it on the wire. */
const ENDINGS: RunOutcome[] = [
  { kind: "succeeded" },
  { kind: "failed" },
  { kind: "timed-out", waited: { secs: 1800, nanos: 0 } },
  { kind: "emulator-closed", waited: { secs: 42, nanos: 0 } },
];

/** A preview of the shipped BoingBag 3.9-1 run, everything present. */
function preview(over: Partial<AmigaInstallPreview> = {}): AmigaInstallPreview {
  return {
    packageId: "boingbag-39-1",
    packageName: "BoingBag 3.9-1",
    tree: "D:/amiga/os39",
    systemVolume: "DH0",
    workingDirectory: "ARTPkg:BoingBag3.9-1",
    program: "ARTPkg:BoingBag3.9-1/C/Updater",
    args: ["AmigaOS-Update", "DH0:"],
    workVolume: "ARTWork",
    packageVolume: "ARTPkg",
    packageArchives: ["D:/amiga/pkg/BoingBag39-1.lha"],
    packageArchivesPresent: true,
    declaredOverlays: ["BoingBag3.9-1-UAE/BoingBag3.9-1"],
    minimumInstallerVersion: "45.15",
    packageDir: "BoingBag3.9-1",
    medium: "E:/amiga/os39/AmigaOS39.iso",
    mediumVolume: "AmigaOS3.9",
    requiredMedium: "the original AmigaOS 3.9 CD-ROM",
    resultFile: "art-result.txt",
    deadlineSeconds: 1800,
    kickstart: "D:/roms/kick31.rom",
    kickstartPresent: true,
    emulator: "C:/Program Files/WinUAE/winuae64.exe",
    profileId: "a1200-aga",
    profileName: "Amiga 1200 (AGA)",
    ...over,
  };
}

describe("the four endings stay four sentences", () => {
  // The defect this whole round kept producing is a confidently wrong
  // sentence, and the cheapest way to produce one here is to map two
  // endings onto one key: "nobody answered — watch the window next time" is
  // the wrong advice for a window the owner shut themselves.
  it("gives every ending its own key, and every ending its own next step", () => {
    const said = ENDINGS.map((o) => outcomePhrase(o).key);
    expect(new Set(said).size).toBe(ENDINGS.length);
    const next = ENDINGS.map((o) => outcomeNextStepPhrase(o).key);
    expect(new Set(next).size).toBe(ENDINGS.length);
    // And neither list may be a rename of the other's — a "next step" that
    // repeats the outcome is not a next step.
    expect(said.some((key, i) => key === next[i])).toBe(false);
  });

  it("separates a refusal from a run that never got an answer", () => {
    expect(outcomePhrase({ kind: "failed" }).key).not.toBe(
      outcomePhrase({ kind: "timed-out", waited: { secs: 1, nanos: 0 } }).key
    );
    expect(outcomeTone({ kind: "failed" })).toBe("err");
    expect(outcomeTone({ kind: "timed-out", waited: { secs: 1, nanos: 0 } })).toBe("warn");
    expect(outcomeTone({ kind: "succeeded" })).toBe("ok");
  });

  it("says how long a timeout or a closed window waited", () => {
    const timedOut = outcomePhrase({ kind: "timed-out", waited: { secs: 1800, nanos: 0 } });
    expect(timedOut.params).toEqual({ seconds: 1800 });
    // Sub-second nanos still round to a whole second rather than printing 0.
    expect(waitedSeconds({ secs: 0, nanos: 900_000_000 })).toBe(1);
  });
});

describe("where the tree and the copy are afterwards", () => {
  it("names the copy and says the original is untouched when a run did not succeed", () => {
    const kept: SettlementReport = {
      kind: "kept",
      copy: "D:/amiga/os39.art-run",
      original: "D:/amiga/os39",
    };
    const phrase = settlementPhrase(kept);
    expect(phrase.params).toEqual({
      copy: "D:/amiga/os39.art-run",
      original: "D:/amiga/os39",
    });
  });

  it("distinguishes a promotion that left a retired tree behind from one that did not", () => {
    const clean = settlementPhrase({ kind: "promoted", tree: "D:/t", leftBehind: null });
    const stuck = settlementPhrase({
      kind: "promoted",
      tree: "D:/t",
      leftBehind: "D:/t.art-old",
    });
    expect(clean.key).not.toBe(stuck.key);
    expect(stuck.params).toEqual({ tree: "D:/t", leftBehind: "D:/t.art-old" });
  });
});

describe("the second archive a BoingBag 1 run needs (ART-186)", () => {
  it("names the archive to go and find when only the wrapper was chosen", () => {
    const advice = overlayAdvicePhrase(preview());
    expect(advice?.key).toBe("osinstall.amigaInstall.overlay.needed");
    expect(advice?.params).toEqual({
      version: "45.15",
      overlays: "BoingBag3.9-1-UAE/BoingBag3.9-1",
    });
  });

  it("says something different once that archive is there", () => {
    const advice = overlayAdvicePhrase(
      preview({ packageArchives: ["a.lha", "BoingBag39-1-UAE.lha"] })
    );
    expect(advice?.key).toBe("osinstall.amigaInstall.overlay.supplied");
  });

  it("invents no requirement for a package that declares none", () => {
    expect(
      overlayAdvicePhrase(preview({ minimumInstallerVersion: null, declaredOverlays: [] }))
    ).toBeNull();
  });
});

describe("what a previewed run still lacks", () => {
  it("says nothing when everything ART cannot supply is there", () => {
    expect(readinessBlockers(preview())).toEqual([]);
  });

  it("names each missing thing separately rather than one 'not ready'", () => {
    const keys = readinessBlockers(
      preview({ packageArchivesPresent: false, kickstartPresent: false, emulator: null })
    ).map((p) => p.key);
    expect(new Set(keys).size).toBe(3);
    expect(keys).toContain("osinstall.amigaInstall.blocker.noEmulator");
  });

  it("names the Kickstart it looked for, so the user can see which path is wrong", () => {
    const [blocker] = readinessBlockers(preview({ kickstartPresent: false }));
    expect(blocker.params).toEqual({ path: "D:/roms/kick31.rom" });
  });
});

// ---------------------------------------------------------------------------
// ART-277: per-package remembered keys, and classifying an archive before
// it ever reaches a request.
// ---------------------------------------------------------------------------

describe("amigaInstallArchiveKey", () => {
  it("scopes the key by package once one is chosen", () => {
    expect(amigaInstallArchiveKey("amigaInstall.archive", "boingbag-39-1")).toBe(
      "amigaInstall.archive.boingbag-39-1"
    );
  });

  it("gives two packages two different keys", () => {
    const one = amigaInstallArchiveKey("amigaInstall.archive", "boingbag-39-1");
    const two = amigaInstallArchiveKey("amigaInstall.archive", "boingbag-39-2");
    expect(one).not.toBe(two);
  });

  it("stays unscoped while no package is chosen at all", () => {
    expect(amigaInstallArchiveKey("amigaInstall.archive", null)).toBe("amigaInstall.archive");
  });
});

/** A minimal, valid `ArchiveClassification` with the given `kind` — every
 *  test below cares about `kind` alone unless it says otherwise. */
function classification(
  kind: ArchiveClassification["kind"],
  over: Partial<ArchiveClassification> = {}
): ArchiveClassification {
  return { kind, topLevel: [], expectedMedia: null, expectedOverlays: [], sharedBy: [], ...over };
}

describe("parseClassification", () => {
  it("reads the id out of an another-package classification", () => {
    expect(parseClassification(classification("another-package:boingbag-39-2"))).toEqual({
      kind: "another-package",
      id: "boingbag-39-2",
    });
  });

  it("reads the id out of an another-packages-update-archive classification", () => {
    expect(
      parseClassification(classification("another-packages-update-archive:boingbag-39-1"))
    ).toEqual({ kind: "another-packages-update-archive", id: "boingbag-39-1" });
  });

  it("reads the media out of an other-artefact classification", () => {
    expect(parseClassification(classification("other-artefact:Locale3.9"))).toEqual({
      kind: "other-artefact",
      media: "Locale3.9",
    });
  });

  it("reads the media out of a shared-artefact classification", () => {
    expect(parseClassification(classification("shared-artefact:Locale3.9"))).toEqual({
      kind: "shared-artefact",
      media: "Locale3.9",
    });
  });

  it("passes the three plain kinds through unchanged, and null through null", () => {
    expect(parseClassification(classification("the-package"))).toEqual({ kind: "the-package" });
    expect(parseClassification(classification("the-update-archive"))).toEqual({
      kind: "the-update-archive",
    });
    expect(parseClassification(classification("unknown"))).toEqual({ kind: "unknown" });
    expect(parseClassification(null)).toBeNull();
  });
});

describe("archiveFieldBlockerPhrase", () => {
  const otherName = (id: string) => (id === "boingbag-39-2" ? "BoingBag 3.9-2" : id);
  // The real labels the two `Field`s render — not stand-ins — so the
  // "belongs in ..." assertions below prove the phrase actually carries the
  // label a reader would see on screen (ART-277 re-review).
  const labels = {
    package: "The package's own archive",
    overlay: "The package's update archive (only if it needs one)",
  };

  it("names both packages when the archive is recognisably another release package's own", () => {
    const c = classification("another-package:boingbag-39-2");
    const phrase = archiveFieldBlockerPhrase(
      c,
      "package",
      "D:/pkg/x.lha",
      "BoingBag 3.9-1",
      otherName,
      labels
    );
    expect(phrase).toEqual({
      key: "osinstall.amigaInstall.classify.anotherPackage",
      params: { other: "BoingBag 3.9-2", selected: "BoingBag 3.9-1" },
    });
    // And the same regardless of which field it sits in — a wrong package is
    // wrong wherever it was put.
    expect(
      archiveFieldBlockerPhrase(c, "overlay", "D:/pkg/x.lha", "BoingBag 3.9-1", otherName, labels)
    ).toEqual(phrase);
  });

  it("says it is that package's own update archive, not its own archive", () => {
    const c = classification("another-packages-update-archive:boingbag-39-2");
    const phrase = archiveFieldBlockerPhrase(
      c,
      "overlay",
      "D:/pkg/x.lha",
      "BoingBag 3.9-1",
      otherName,
      labels
    );
    expect(phrase).toEqual({
      key: "osinstall.amigaInstall.classify.anotherPackagesUpdateArchive",
      params: { other: "BoingBag 3.9-2", selected: "BoingBag 3.9-1" },
    });
  });

  it("names both packages, by display name, when two or more release packages share one identity", () => {
    // ART-277 re-review, L5: never resolved by picking one (ART-276's own
    // trap), and — unlike `other-artefact` — makes no claim about which
    // step handles the file, since more than one package does.
    const c = classification("shared-artefact:Locale3.9", {
      sharedBy: ["locale-39", "locale-39-turkish"],
    });
    const otherPackageName = (id: string) =>
      id === "locale-39" ? "Locale 3.9" : id === "locale-39-turkish" ? "Türkçe Locale 3.9" : id;
    const phrase = archiveFieldBlockerPhrase(
      c,
      "package",
      "D:/pkg/Locale3.9.lha",
      "BoingBag 3.9-1",
      otherPackageName,
      labels
    );
    expect(phrase).toEqual({
      key: "osinstall.amigaInstall.classify.sharedArtefact",
      params: {
        path: "D:/pkg/Locale3.9.lha",
        media: "Locale3.9",
        packages: "Locale 3.9, Türkçe Locale 3.9",
      },
    });
  });

  it("names the shared artefact and what this field expects, when the selected package declares it", () => {
    const c = classification("other-artefact:Locale3.9", { expectedMedia: "BoingBag3.9-1" });
    const phrase = archiveFieldBlockerPhrase(
      c,
      "package",
      "D:/pkg/Locale3.9.lha",
      "BoingBag 3.9-1",
      otherName,
      labels
    );
    expect(phrase).toEqual({
      key: "osinstall.amigaInstall.classify.otherArtefact",
      params: {
        path: "D:/pkg/Locale3.9.lha",
        media: "Locale3.9",
        selected: "BoingBag 3.9-1",
        expected: "BoingBag3.9-1",
      },
    });
  });

  it("falls back to a generic sentence when the field expects nothing in particular", () => {
    // The overlay field, for a selected package that declares no overlay at
    // all — `expectedOverlays` is empty, so there is nothing to name.
    const c = classification("other-artefact:Locale3.9", { expectedOverlays: [] });
    const phrase = archiveFieldBlockerPhrase(
      c,
      "overlay",
      "D:/pkg/Locale3.9.lha",
      "BoingBag 3.9-2",
      otherName,
      labels
    );
    expect(phrase).toEqual({
      key: "osinstall.amigaInstall.classify.otherArtefactGeneric",
      params: { path: "D:/pkg/Locale3.9.lha", media: "Locale3.9" },
    });
  });

  it("names the update archive's own field, not by direction, when it is in the package field", () => {
    const phrase = archiveFieldBlockerPhrase(
      classification("the-update-archive"),
      "package",
      "D:/pkg/x.lha",
      "BoingBag 3.9-1",
      otherName,
      labels
    );
    expect(phrase?.key).toBe("osinstall.amigaInstall.classify.wrongFieldOverlay");
    // ART-277 re-review's own finding: the old wording said "the second
    // field below", which is spatially backwards once this renders in the
    // `blockers` box below both fields. The label must be carried, not a
    // direction — asserted on the params a `t()` call actually reads.
    expect(phrase?.params).toEqual({
      packageLabel: labels.package,
      overlayLabel: labels.overlay,
    });
  });

  it("names the package's own field, not by direction, when its archive is in the overlay field", () => {
    const phrase = archiveFieldBlockerPhrase(
      classification("the-package"),
      "overlay",
      "D:/pkg/x.lha",
      "BoingBag 3.9-1",
      otherName,
      labels
    );
    expect(phrase?.key).toBe("osinstall.amigaInstall.classify.wrongFieldPackage");
    expect(phrase?.params).toEqual({
      packageLabel: labels.package,
      overlayLabel: labels.overlay,
    });
  });

  it("says nothing for the right archive in the right field, or an unrecognised one", () => {
    expect(
      archiveFieldBlockerPhrase(
        classification("the-package"),
        "package",
        "D:/pkg/x.lha",
        "BoingBag 3.9-1",
        otherName,
        labels
      )
    ).toBeNull();
    expect(
      archiveFieldBlockerPhrase(
        classification("the-update-archive"),
        "overlay",
        "D:/pkg/x.lha",
        "BoingBag 3.9-1",
        otherName,
        labels
      )
    ).toBeNull();
    expect(
      archiveFieldBlockerPhrase(
        classification("unknown"),
        "package",
        "D:/pkg/x.lha",
        "BoingBag 3.9-1",
        otherName,
        labels
      )
    ).toBeNull();
    expect(
      archiveFieldBlockerPhrase(null, "package", "D:/pkg/x.lha", "BoingBag 3.9-1", otherName, labels)
    ).toBeNull();
  });
});

describe("dedupeBlockers", () => {
  it("keeps every entry when no two phrases are identical", () => {
    const out = dedupeBlockers([
      { field: "preview", phrase: { key: "a" } },
      { field: "package", phrase: { key: "b", params: { x: 1 } } },
      { field: "overlay", phrase: { key: "b", params: { x: 2 } } },
    ]);
    expect(out.map((k) => k.id)).toEqual(["preview:a", "package:b", "overlay:b"]);
  });

  // ART-277 round 1 whole-branch review, M2: two fields agreeing on the
  // same wrong archive produce the identical `Phrase` — dropped after the
  // first, not rendered twice.
  it("drops a later entry whose key and params exactly match an earlier one", () => {
    const phrase = {
      key: "osinstall.amigaInstall.classify.anotherPackage",
      params: { other: "BoingBag 3.9-2", selected: "BoingBag 3.9-1" },
    };
    const out = dedupeBlockers([
      { field: "package", phrase },
      { field: "overlay", phrase: { ...phrase, params: { ...phrase.params } } },
    ]);
    expect(out).toHaveLength(1);
    expect(out[0].id).toBe("package:osinstall.amigaInstall.classify.anotherPackage");
  });

  it("gives two different phrases two different ids even when their keys collide by field alone", () => {
    const out = dedupeBlockers([
      { field: "preview", phrase: { key: "osinstall.amigaInstall.blocker.noEmulator" } },
      { field: "preview", phrase: { key: "osinstall.amigaInstall.blocker.kickstartMissing" } },
    ]);
    expect(new Set(out.map((k) => k.id)).size).toBe(2);
  });
});
