// The seven endings a resolved slot can produce, and the one thing that has
// to be true of all of them: the sentence is in **both** catalogues.
//
// A `Phrase` whose key is not a leaf in `en.json` renders the raw dotted key
// on screen; one that is in `en.json` and not in `tr.json` renders English to
// a Turkish user. Neither fails to compile, and neither is caught by
// `parity.test.ts` (which compares the two catalogues to each other, not to
// this file) — so the key check here is the same one
// `src/i18n/literal-keys.test.ts` makes for `t("…")` call sites, applied to
// the keys a `src/lib` mapper builds.

import { describe, expect, it } from "vitest";

import en from "@/i18n/en.json";
import tr from "@/i18n/tr.json";
import type {
  BytesRead,
  Installed,
  SetSummary,
  SlotCandidate,
  SlotKind,
  SlotState,
} from "@/lib/osinstall";
import { readoutRunningLine, setLine, slotLines, unreadableFolderLines } from "@/lib/slots";

/** Whether `dotted` names a string leaf in `catalogue`. */
function isLeafKey(catalogue: unknown, dotted: string): boolean {
  let node: unknown = catalogue;
  for (const part of dotted.split(".")) {
    if (typeof node !== "object" || node === null) return false;
    node = (node as Record<string, unknown>)[part];
  }
  return typeof node === "string";
}

/** The sentence itself, for the few checks that are about the wording rather
 *  than about which key was picked. */
function leafText(catalogue: unknown, dotted: string): string {
  let node: unknown = catalogue;
  for (const part of dotted.split(".")) {
    node = (node as Record<string, unknown>)[part];
  }
  if (typeof node !== "string") throw new Error(`${dotted} is not a string leaf`);
  return node;
}

interface StateOptions {
  id?: string;
  kind?: SlotKind;
  name?: string;
  identity?: string;
  required?: boolean;
  filenames?: string[];
  position?: number;
  requires?: string[];
  found?: SlotState["found"];
  candidates?: SlotCandidate[];
  installed?: Installed;
  chosenMissing?: string | null;
  blockedBy?: string[];
  notNeeded?: string | null;
}

const NOT_READ: BytesRead = { state: "not-read" };
const NO_ROW: BytesRead = { state: "read-no-row" };
/** Hashed, and the table names those bytes as something else — F13's case. */
const OTHER: BytesRead = {
  state: "read-row",
  artefact: "boingbag-39-2",
  name: "BoingBag 3.9-2",
};

/** A candidate nobody has hashed — the ordinary state before the identify
 *  job has run over a folder. */
const unread = (path: string): SlotCandidate => ({ path, bytesRead: NOT_READ });
const read = (path: string): SlotCandidate => ({ path, bytesRead: NO_ROW });

function state(options: StateOptions = {}): SlotState {
  return {
    slot: {
      id: options.id ?? "package:boingbag-39-1",
      kind: options.kind ?? "package",
      name: options.name ?? "BoingBag 3.9-1",
      identity: options.identity ?? "BoingBag3.9-1",
      artefact: "boingbag-39-1",
      required: options.required ?? false,
      filenames: options.filenames ?? ["BoingBag39-1.lha"],
      provenance: "Haage and Partners (3.9)",
      position: options.position ?? 1,
      requires: options.requires ?? [],
      supersededBy: [],
    },
    found: options.found ?? null,
    candidates: options.candidates ?? [],
    installed: options.installed ?? { state: "no" },
    chosenMissing: options.chosenMissing ?? null,
    blockedBy: options.blockedBy ?? [],
    notNeeded: options.notNeeded ?? null,
  };
}

const foundBy = (matchedBy: string, path: string, bytesRead: BytesRead = NO_ROW) =>
  ({
    path,
    matchedBy,
    row: null,
    confirmed: null,
    bytesRead,
  }) as NonNullable<SlotState["found"]>;

describe("slotLines", () => {
  it("gives each of the eight endings its own kind and its own key", () => {
    const cases: [string, SlotState][] = [
      ["found-by-hash", state({ found: foundBy("hash", "D:\\a\\BoingBag39-1.lha") })],
      [
        "found-by-name",
        state({ found: foundBy("top-level-directory", "D:\\a\\renamed.lha") }),
      ],
      [
        "chosen",
        state({
          kind: "rom",
          id: "rom",
          name: "Kickstart",
          identity: "40",
          found: foundBy("chosen", "D:\\roms\\kick.rom"),
        }),
      ],
      [
        "chosen-missing",
        state({
          kind: "rom",
          id: "rom",
          name: "Kickstart",
          identity: "40",
          chosenMissing: "E:\\roms\\kick.rom",
        }),
      ],
      ["guessed-by-filename", state({ candidates: [read("D:\\a\\BoingBag39-1.lha")] })],
      [
        "ambiguous",
        state({ candidates: [read("D:\\a\\one.lha"), read("D:\\b\\two.lha")] }),
      ],
      ["not-found", state()],
      ["not-needed", state({ notNeeded: "Updater 45.15" })],
    ];

    // Every ending is distinct — no two of these produce the same kind, which
    // is the property that keeps "ART could not confirm it" from being read
    // as "found" and "not needed" from being read as "missing".
    expect(cases.map(([kind]) => kind)).toEqual([...new Set(cases.map(([kind]) => kind))]);

    for (const [kind, one] of cases) {
      const [line] = slotLines([one]);
      expect(line.kind, `${kind} came out as ${line.kind}`).toBe(kind);
      expect(isLeafKey(en, line.phrase.key), `${line.phrase.key} missing from en.json`).toBe(true);
      expect(isLeafKey(tr, line.phrase.key), `${line.phrase.key} missing from tr.json`).toBe(true);
    }
  });

  it("says what it expected only when ART actually knows a name", () => {
    const [named] = slotLines([state()]);
    expect(named.phrase.key).toBe("osinstall.slots.notFound");
    expect(named.phrase.params?.filenames).toBe("BoingBag39-1.lha");

    // "Expected " with nothing after it is a sentence nobody can act on.
    const [unnamed] = slotLines([state({ filenames: [] })]);
    expect(unnamed.phrase.key).toBe("osinstall.slots.notFoundUnnamed");
    expect(unnamed.kind).toBe("not-found");
    expect(isLeafKey(en, unnamed.phrase.key)).toBe(true);
    expect(isLeafKey(tr, unnamed.phrase.key)).toBe(true);
  });

  it("carries the installed badge from the manifest, and nothing when it is silent", () => {
    const ran = slotLines([
      state({ installed: { state: "ran", command: "PKG:C/Updater AmigaOS-Update" } }),
    ])[0];
    expect(ran.installed?.key).toBe("osinstall.slots.installedRan");
    expect(ran.installed?.params?.command).toBe("PKG:C/Updater AmigaOS-Update");

    const placed = slotLines([state({ installed: { state: "placed", at: "C/Foo" } })])[0];
    expect(placed.installed?.key).toBe("osinstall.slots.installedPlaced");

    // Never "not installed": the manifest saying nothing is not a claim, and
    // there may be no tree chosen at all.
    expect(slotLines([state()])[0].installed).toBeNull();

    for (const key of ["osinstall.slots.installedRan", "osinstall.slots.installedPlaced"]) {
      expect(isLeafKey(en, key)).toBe(true);
      expect(isLeafKey(tr, key)).toBe(true);
    }
  });

  it("names what a blocked row waits for by name, never by slot id", () => {
    // Before a tree is chosen this is the most-rendered sentence in the
    // readout, and it used to read "needs package:boingbag-39-1,
    // medium:AmigaOS3.9 first" — ART's own bookkeeping, at the user.
    const [line] = slotLines([
      state({
        id: "package:boingbag-39-2",
        name: "BoingBag 3.9-2",
        position: 2,
        blockedBy: ["package:boingbag-39-1", "medium:AmigaOS3.9"],
      }),
      state({ id: "package:boingbag-39-1", name: "BoingBag 3.9-1", position: 1 }),
      state({ id: "medium:AmigaOS3.9", kind: "medium", name: "AmigaOS3.9", position: 0 }),
    ]).filter((row) => row.id === "package:boingbag-39-2");
    expect(line.blocked?.key).toBe("osinstall.slots.blocked");
    expect(line.blocked?.params?.needs).toBe("BoingBag 3.9-1, AmigaOS3.9");
    expect(isLeafKey(en, "osinstall.slots.blocked")).toBe(true);
    expect(isLeafKey(tr, "osinstall.slots.blocked")).toBe(true);
    expect(slotLines([state()])[0].blocked).toBeNull();

    // A requirement naming a slot this release ships none of still renders:
    // the id is a poor sentence, and no sentence at all is a worse one.
    const [orphan] = slotLines([state({ blockedBy: ["package:nothing-ships-this"] })]);
    expect(orphan.blocked?.params?.needs).toBe("package:nothing-ships-this");
  });

  it("sorts by the chain position, never by name or by the order it was handed", () => {
    const lines = slotLines([
      state({ id: "rom", kind: "rom", name: "Kickstart", identity: "40", position: 7 }),
      state({ id: "medium:AmigaOS3.9", kind: "medium", name: "AmigaOS3.9", position: 0 }),
      state({ id: "package:boingbag-39-1", position: 1 }),
    ]);
    expect(lines.map((line) => line.id)).toEqual([
      "medium:AmigaOS3.9",
      "package:boingbag-39-1",
      "rom",
    ]);
    // A Kickstart is the one slot the recipe names only by the major it asks
    // for, so the row says which.
    expect(lines[2].name).toBe("Kickstart 40");
  });

  it("shows every candidate of an ambiguous row rather than picking one", () => {
    const [line] = slotLines([
      state({ candidates: [read("D:\\a\\first.lha"), read("D:\\b\\second.lha")] }),
    ]);
    expect(line.kind).toBe("ambiguous");
    expect(line.file).toBeNull();
    expect(line.candidates).toEqual(["first.lha", "second.lha"]);
    expect(line.phrase.params?.count).toBe(2);
    expect(line.phrase.params?.candidates).toBe("first.lha, second.lha");
  });

  it("keeps 'ART did not look' apart from 'ART looked and found nothing'", () => {
    // The defect the review named: `osinstallSlots` hashes nothing, so before
    // the identify job has run this is the *default* state — and the row used
    // to assert a fact about a table nobody had asked.
    const hashed = slotLines([
      state({ found: foundBy("top-level-directory", "D:\\a\\renamed.lha", NO_ROW) }),
    ])[0];
    expect(hashed.kind).toBe("found-by-name");
    expect(hashed.phrase.key).toBe("osinstall.slots.foundByName");

    const unhashed = slotLines([
      state({ found: foundBy("top-level-directory", "D:\\a\\renamed.lha", NOT_READ) }),
    ])[0];
    expect(unhashed.kind).toBe("found-by-name");
    expect(unhashed.phrase.key).toBe("osinstall.slots.foundByNameUnread");

    // The same split for the guess row.
    const guessHashed = slotLines([state({ candidates: [read("D:\\a\\x.lha")] })])[0];
    expect(guessHashed.phrase.key).toBe("osinstall.slots.guessedByFilename");
    const guessUnhashed = slotLines([state({ candidates: [unread("D:\\a\\x.lha")] })])[0];
    expect(guessUnhashed.phrase.key).toBe("osinstall.slots.guessedByFilenameUnread");

    for (const key of [
      "osinstall.slots.foundByName",
      "osinstall.slots.foundByNameUnread",
      "osinstall.slots.guessedByFilename",
      "osinstall.slots.guessedByFilenameUnread",
    ]) {
      expect(isLeafKey(en, key), `${key} missing from en.json`).toBe(true);
      expect(isLeafKey(tr, key), `${key} missing from tr.json`).toBe(true);
    }
  });

  it("keeps 'in no table' apart from 'in the table, as something else'", () => {
    // F13. A disk relabelled `AmigaOS3.9` whose bytes are BoingBag 2's
    // matches this slot at rank 2 by the name it gives for itself, and the
    // old sentence said its bytes were "in no table ART has" — false about a
    // file the table knows perfectly well, and unhelpful: what the owner
    // needs to be told is what the bytes actually are.
    const relabelled = slotLines([
      state({ found: foundBy("volume-name", "D:\\a\\relabelled.iso", OTHER) }),
    ])[0];
    expect(relabelled.kind).toBe("found-by-name");
    expect(relabelled.phrase.key).toBe("osinstall.slots.foundByNameOtherArtefact");
    expect(relabelled.phrase.params?.other).toBe("BoingBag 3.9-2");

    // The same third answer on the guess rank.
    const guessed = slotLines([
      state({ candidates: [{ path: "D:\\a\\BoingBag39-1.lha", bytesRead: OTHER }] }),
    ])[0];
    expect(guessed.kind).toBe("guessed-by-filename");
    expect(guessed.phrase.key).toBe("osinstall.slots.guessedByFilenameOtherArtefact");
    expect(guessed.phrase.params?.other).toBe("BoingBag 3.9-2");

    // Three keys per ending, all distinct, all in both catalogues.
    const keys = [
      "osinstall.slots.foundByNameUnread",
      "osinstall.slots.foundByName",
      "osinstall.slots.foundByNameOtherArtefact",
      "osinstall.slots.guessedByFilenameUnread",
      "osinstall.slots.guessedByFilename",
      "osinstall.slots.guessedByFilenameOtherArtefact",
    ];
    expect(new Set(keys).size).toBe(keys.length);
    for (const key of keys) {
      expect(isLeafKey(en, key), `${key} missing from en.json`).toBe(true);
      expect(isLeafKey(tr, key), `${key} missing from tr.json`).toBe(true);
    }
  });

  it("names the file the user chose when that file is not there", () => {
    const [line] = slotLines([
      state({
        kind: "rom",
        id: "rom",
        name: "Kickstart",
        identity: "40",
        required: true,
        chosenMissing: "E:\\roms\\kick40068.rom",
      }),
    ]);
    expect(line.kind).toBe("chosen-missing");
    expect(line.phrase.key).toBe("osinstall.slots.chosenMissing");
    // The path itself, because "choose it again" without saying which file
    // has gone is a refusal nobody can act on.
    expect(line.phrase.params?.path).toBe("E:\\roms\\kick40068.rom");
    expect(line.phrase.params?.name).toBe("Kickstart 40");
    expect(line.file).toBe("kick40068.rom");
  });

  it("a not-needed slot is never reported as missing, even with nothing found", () => {
    // The row the design shows for BoingBag 1's UAE fix: present or absent,
    // an artefact ART has measured as unnecessary is neither found nor
    // missing, and "not found" here would be the confident wrong sentence.
    const [line] = slotLines([
      state({ kind: "overlay", filenames: ["BoingBag39-1-UAE.lha"], notNeeded: "Updater 45.15" }),
    ]);
    expect(line.kind).toBe("not-needed");
    expect(line.phrase.key).toBe("osinstall.slots.notNeeded");
    expect(line.phrase.params?.carries).toBe("Updater 45.15");
  });
});

describe("chosenMissing states only what was checked", () => {
  /// The re-review's own constructed case: `path.is_file()` answers `false`
  /// identically for "does not exist", "is a directory now" and "exists but
  /// the metadata call failed" (permission denied, an inaccessible share).
  /// The sentence used to name one specific cause — *"plug in the drive it
  /// was on"* — which the check never established, and which is simply wrong
  /// advice in the permission case.
  it("does not name a cause the existence check never established", () => {
    for (const catalogue of [en, tr]) {
      const sentence = leafText(catalogue, "osinstall.slots.chosenMissing");
      expect(sentence).toContain("{{path}}");
      expect(sentence.toLowerCase()).not.toContain("drive");
      expect(sentence.toLowerCase()).not.toContain("sürücü");
    }
  });
});

describe("setLine", () => {
  const summary = (over: Partial<SetSummary> = {}): SetSummary => ({
    release: "AmigaOS 3.9",
    requiredTotal: 2,
    requiredFound: 2,
    optionalTotal: 4,
    optionalFound: 3,
    ...over,
  });

  it("counts found over the whole set and names the required shortfall apart", () => {
    const { phrase, ready } = setLine(summary(), []);
    expect(phrase.key).toBe("osinstall.slots.setLine");
    expect(phrase.params).toMatchObject({
      release: "AmigaOS 3.9",
      found: 5,
      total: 6,
      missingRequired: 0,
    });
    // A set missing only optional files is ready to build — one fraction
    // cannot say that, which is why `ready` reads the required half alone.
    expect(ready).toBe(true);
  });

  it("is not ready the moment a required slot is missing", () => {
    const { phrase, ready } = setLine(summary({ requiredFound: 1 }), []);
    expect(ready).toBe(false);
    expect(phrase.params?.missingRequired).toBe(1);
  });

  it("says how many are not needed, so a shrinking denominator is not a disappearance", () => {
    // `slots::summarize` leaves a not-needed slot out of **both** totals, so
    // "5 of 6" legitimately becomes "5 of 5" as ART learns more. Without this
    // clause on the line, that reads as material vanishing.
    const states = [
      state({ id: "overlay:uae", notNeeded: "Updater 45.15" }),
      state({ id: "package:boingbag-39-1" }),
    ];
    const { phrase } = setLine(summary({ optionalTotal: 3, optionalFound: 3 }), states);
    expect(phrase.key).toBe("osinstall.slots.setLineNotNeeded");
    expect(phrase.params?.notNeeded).toBe(1);
    expect(phrase.params?.total).toBe(5);

    // And says nothing when there is nothing to say: "0 not needed" is noise.
    expect(setLine(summary(), states.slice(1)).phrase.key).toBe("osinstall.slots.setLine");

    for (const key of ["osinstall.slots.setLine", "osinstall.slots.setLineNotNeeded"]) {
      expect(isLeafKey(en, key), `${key} missing from en.json`).toBe(true);
      expect(isLeafKey(tr, key), `${key} missing from tr.json`).toBe(true);
    }
  });
});

describe("unreadableFolderLines", () => {
  /// Fix round 1's F2 put the field on the wire and nothing rendered it. A
  /// remembered path on a drive nobody plugged in is the ordinary case, and
  /// the rows resolved against the *other* folders stay true — so this names
  /// what was not counted rather than casting doubt over them.
  it("names each folder that could not be read, and says nothing when there are none", () => {
    const lines = unreadableFolderLines(["E:\gone", "F:\also gone"]);
    expect(lines.map((line) => line.folder)).toEqual(["E:\gone", "F:\also gone"]);
    expect(lines[0].phrase.key).toBe("osinstall.slots.unreadableFolder");
    expect(lines[0].phrase.params?.folder).toBe("E:\gone");
    expect(unreadableFolderLines([])).toEqual([]);
    expect(isLeafKey(en, "osinstall.slots.unreadableFolder")).toBe(true);
    expect(isLeafKey(tr, "osinstall.slots.unreadableFolder")).toBe(true);
  });
});

describe("readoutRunningLine", () => {
  /// CLAUDE.md: a bar with no total looks like progress and carries none.
  /// `osinstall_slots` is one round trip and reports no progress of its own,
  /// so the honest running statement is the count it was given.
  it("states a count rather than a fraction ART cannot fill in", () => {
    const phrase = readoutRunningLine(3);
    expect(phrase.key).toBe("osinstall.slots.reading");
    expect(phrase.params?.count).toBe(3);
    // Pluralised, so both arms have to be in both catalogues.
    for (const key of ["osinstall.slots.reading_one", "osinstall.slots.reading_other"]) {
      expect(isLeafKey(en, key), `${key} missing from en.json`).toBe(true);
      expect(isLeafKey(tr, key), `${key} missing from tr.json`).toBe(true);
    }
  });
});
