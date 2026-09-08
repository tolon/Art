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
import {
  candidateLines,
  crowdedFolderLines,
  readoutRunningLine,
  setLine,
  slotLines,
  unreadableFolderLines,
} from "@/lib/slots";

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
  incomplete?: string | null;
  expectsDirectories?: string[];
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
      expectsDirectories: options.expectsDirectories ?? [],
    },
    found: options.found ?? null,
    candidates: options.candidates ?? [],
    installed: options.installed ?? { state: "no" },
    chosenMissing: options.chosenMissing ?? null,
    blockedBy: options.blockedBy ?? [],
    notNeeded: options.notNeeded ?? null,
    incomplete: options.incomplete ?? null,
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
  it("gives each of the eleven endings its own kind and its own key", () => {
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
      // The three the round-2 whole-branch review added, each because the
      // ending it was folded into said something false: a ROM is not missing
      // from a folder ART never looks in (M1), a slot the set line counts
      // found is not a red row (M2), and a disc that is here but short is not
      // absent (L6).
      [
        "rom-not-chosen",
        state({ kind: "rom", id: "rom", name: "Kickstart", identity: "40", filenames: [] }),
      ],
      ["installed-elsewhere", state({ installed: { state: "placed", at: null } })],
      [
        "incomplete",
        state({
          kind: "medium",
          id: "medium:AmigaOS3.9",
          found: foundBy("hash", "D:\\a\\AmigaOS39.iso"),
          incomplete: "Contribution",
        }),
      ],
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
    expect(line.candidates).toEqual(["D:\\a\\first.lha", "D:\\b\\second.lha"]);
    expect(line.phrase.params?.count).toBe(2);
    expect(line.phrase.params?.candidates).toBe("D:\\a\\first.lha, D:\\b\\second.lha");
  });

  /// **The ambiguity this row actually exists for** (fix round 1, F3). One
  /// artefact in two folders is what produces it, and its commonest shape is
  /// two copies under the *same* name -- for which file names rendered
  /// "BoingBag39-1.lha, BoingBag39-1.lha" and gave the user nothing to pick
  /// between. The folder is the whole of the information.
  it("keeps two copies of one name apart, which file names could not", () => {
    const [line] = slotLines([
      state({
        candidates: [
          read("D:\\disks\\BoingBag39-1.lha"),
          read("E:\\archives\\BoingBag39-1.lha"),
        ],
      }),
    ]);
    expect(line.kind).toBe("ambiguous");
    expect(new Set(line.candidates).size).toBe(2);
    expect(line.phrase.params?.candidates).toBe(
      "D:\\disks\\BoingBag39-1.lha, E:\\archives\\BoingBag39-1.lha"
    );
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
    const { phrase, ready } = setLine(summary({ requiredFound: 1 }), []);
    expect(phrase.key).toBe("osinstall.slots.setLine");
    expect(phrase.params).toMatchObject({
      release: "AmigaOS 3.9",
      found: 4,
      total: 6,
      missingRequired: 1,
    });
    expect(ready).toBe(false);
  });

  /// **The shortfall is named only when there is one** (round 2 review, L8).
  /// A complete set read `8 of 8 found · 0 required missing` in a green
  /// badge — this module's own doc rejecting "0 not needed" one clause along
  /// while printing "0 required missing".
  it("says a ready set is ready rather than counting nothing that is missing", () => {
    const { phrase, ready } = setLine(summary(), []);
    expect(phrase.key).toBe("osinstall.slots.setLineReady");
    expect(phrase.params).toMatchObject({ found: 5, total: 6 });
    // A set missing only optional files is ready to build — one fraction
    // cannot say that, which is why `ready` reads the required half alone.
    expect(ready).toBe(true);
    for (const catalogue of [en, tr]) {
      expect(leafText(catalogue, "osinstall.slots.setLineReady")).not.toContain(
        "{{missingRequired}}"
      );
    }
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
    const { phrase } = setLine(
      summary({ requiredFound: 1, optionalTotal: 3, optionalFound: 3 }),
      states
    );
    expect(phrase.key).toBe("osinstall.slots.setLineNotNeeded");
    expect(phrase.params?.notNeeded).toBe(1);
    expect(phrase.params?.total).toBe(5);

    // The ready half of the same pair (L8).
    expect(setLine(summary({ optionalTotal: 3, optionalFound: 3 }), states).phrase.key).toBe(
      "osinstall.slots.setLineReadyNotNeeded"
    );

    // And says nothing when there is nothing to say: "0 not needed" is noise.
    expect(setLine(summary(), states.slice(1)).phrase.key).toBe(
      "osinstall.slots.setLineReady"
    );

    for (const key of [
      "osinstall.slots.setLine",
      "osinstall.slots.setLineNotNeeded",
      "osinstall.slots.setLineReady",
      "osinstall.slots.setLineReadyNotNeeded",
    ]) {
      expect(isLeafKey(en, key), `${key} missing from en.json`).toBe(true);
      expect(isLeafKey(tr, key), `${key} missing from tr.json`).toBe(true);
    }
  });
});

describe("the ROM row is never about a folder (round 2 review, M1)", () => {
  const rom = (options: StateOptions = {}) =>
    state({ kind: "rom", id: "rom", name: "Kickstart", identity: "40", required: true, ...options });

  /// **The first row of every fresh build**, and it said the material was
  /// short. A ROM slot is not resolved from a folder at all — `resolve_one`
  /// answers `Chosen` for it and it carries no `filenames` — so
  /// *"Kickstart 40 is not in the folders you named"* told the user to look
  /// somewhere ART will never look, and turned the set line red over it.
  it("says no Kickstart is chosen yet, never that one is missing from a folder", () => {
    const [line] = slotLines([rom()]);
    expect(line.kind).toBe("rom-not-chosen");
    expect(line.phrase.key).toBe("osinstall.slots.romNotChosen");
    expect(line.phrase.params?.identity).toBe("40");
    for (const catalogue of [en, tr]) {
      const text = leafText(catalogue, line.phrase.key);
      expect(isLeafKey(catalogue, line.phrase.key)).toBe(true);
      // The sentence a user cannot act on, in either language.
      expect(text).not.toMatch(/folders you named|adını verdiğiniz klasörlerde/);
      expect(text).toContain("{{identity}}");
    }
  });

  /// A release stating no floor gets the same instruction without a number:
  /// "a Kickstart  or newer" is what interpolating an empty identity gives.
  it("drops the floor from the sentence when the release states none", () => {
    const [line] = slotLines([rom({ identity: "" })]);
    expect(line.phrase.key).toBe("osinstall.slots.romNotChosenNoFloor");
    for (const catalogue of [en, tr]) {
      expect(leafText(catalogue, line.phrase.key)).not.toContain("{{identity}}");
    }
  });

  /// And a ROM the user chose says what it satisfies and where it is — not
  /// the generic *chosen* sentence, whose "ART has not checked it" is noise
  /// about a file they picked from their own ROM folder.
  it("names the floor and the path for a Kickstart the user chose", () => {
    const [line] = slotLines([
      rom({ found: foundBy("chosen", "E:\\amiga\\Shared\\rom\\kick40068.A1200.rom") }),
    ]);
    expect(line.kind).toBe("chosen");
    expect(line.phrase.key).toBe("osinstall.slots.romChosen");
    expect(line.phrase.params).toMatchObject({
      identity: "40",
      path: "E:\\amiga\\Shared\\rom\\kick40068.A1200.rom",
    });
    for (const catalogue of [en, tr]) {
      expect(isLeafKey(catalogue, line.phrase.key)).toBe(true);
    }
  });
});

describe("a row may not disagree with the count above it (round 2 review, M2)", () => {
  /// `summarize` counts a slot found when `found.is_some() || installed !=
  /// No`. `slotLines` never looked at `installed` when picking the row's
  /// kind, so a tree whose manifest records the medium, with the archive no
  /// longer in the folders, drew a red ✖ *"not in the folders you named"*
  /// under a green *"0 required missing"*. The row said problem; the count
  /// said ready.
  it("says an installed artefact's archive has gone, and does not call it missing", () => {
    const [line] = slotLines([state({ installed: { state: "placed", at: null } })]);
    expect(line.kind).toBe("installed-elsewhere");
    expect(line.phrase.key).toBe("osinstall.slots.installedArchiveGone");
    expect(line.installed).not.toBeNull();
    for (const catalogue of [en, tr]) {
      expect(isLeafKey(catalogue, line.phrase.key)).toBe(true);
    }
  });

  /// The other arm, and what keeps this from swallowing a real absence: a
  /// slot the manifest says nothing about is still *not found*.
  it("leaves a slot no manifest mentions as not found", () => {
    expect(slotLines([state()])[0].kind).toBe("not-found");
  });

  /// And a file that **is** in the folders still gets its own find sentence,
  /// with the installed badge beside it rather than instead of it.
  it("prefers the find when the archive is there as well", () => {
    const [line] = slotLines([
      state({
        installed: { state: "placed", at: null },
        found: foundBy("hash", "E:\\material\\BoingBag39-1.lha"),
      }),
    ]);
    expect(line.kind).toBe("found-by-hash");
    expect(line.installed).not.toBeNull();
  });
});

describe("matched, and the copy is short (design § 3.6, review L6)", () => {
  /// A disc ART recognises whose root is missing `Contribution` is a
  /// re-master or a partial copy: *look at the copy you have* rather than
  /// *go and find it*, which is what "not found" would have said about a
  /// disc sitting in the folder.
  it("names the missing directory and stays a find, never a not-found", () => {
    const [line] = slotLines([
      state({
        kind: "medium",
        id: "medium:AmigaOS3.9",
        name: "AmigaOS3.9",
        identity: "AmigaOS3.9",
        found: foundBy("hash", "E:\\material\\AmigaOS39.iso"),
        incomplete: "Contribution",
      }),
    ]);
    expect(line.kind).toBe("incomplete");
    expect(line.phrase.key).toBe("osinstall.slots.incomplete");
    expect(line.phrase.params).toMatchObject({
      file: "AmigaOS39.iso",
      directory: "Contribution",
    });
    for (const catalogue of [en, tr]) {
      expect(isLeafKey(catalogue, line.phrase.key)).toBe(true);
    }
  });

  it("says nothing when the disc carries everything expected", () => {
    const [line] = slotLines([
      state({
        kind: "medium",
        id: "medium:AmigaOS3.9",
        found: foundBy("hash", "E:\\material\\AmigaOS39.iso"),
      }),
    ]);
    expect(line.kind).toBe("found-by-hash");
  });
});

describe("crowdedFolderLines", () => {
  /// Design § 6: *"bound the count and **name the bound**."* Stopping is
  /// right; stopping silently would make an artefact ART never reached read
  /// as an artefact that is not there.
  it("names the folder and the number ART stopped at", () => {
    const [line] = crowdedFolderLines([["E:\\aminet", 200]]);
    expect(line.folder).toBe("E:\\aminet");
    expect(line.phrase.key).toBe("osinstall.slots.crowdedFolder");
    expect(line.phrase.params).toEqual({ folder: "E:\\aminet", bound: 200 });
    for (const catalogue of [en, tr]) {
      expect(leafText(catalogue, line.phrase.key)).toContain("{{bound}}");
    }
    // Empty is the normal answer and says nothing.
    expect(crowdedFolderLines([])).toEqual([]);
  });
});

describe("what a chosen file's sentence may claim", () => {
  /// **Fix round 1, L7.** *"ART did not identify it"* is printed after the
  /// user picks a row out of the ambiguous list — about which
  /// `candidateLines` has just told them, on that very row, what ART knows
  /// about the bytes. An under-claim rather than an over-claim, but it is a
  /// sentence the screen has evidence against. What is true is that ART has
  /// not *checked* the file the user named.
  it("says ART has not checked the file, never that it learned nothing about it", () => {
    const [line] = slotLines([
      state({ found: foundBy("chosen", "D:\\pkg\\my-own-copy.lha") }),
    ]);
    expect(line.kind).toBe("chosen");
    for (const catalogue of [en, tr]) {
      const text = leafText(catalogue, line.phrase.key);
      expect(text).not.toMatch(/did not identify|tanımadı/);
    }
    expect(leafText(en, line.phrase.key)).toContain("has not checked");
  });
});

describe("candidateLines", () => {
  /// The ambiguous row says *that* ART will not choose and lists the paths.
  /// A screen that offers the choice needs the evidence beside each option,
  /// and the evidence differs per candidate: one file nobody has hashed and
  /// one whose bytes the table says are a different artefact are not the same
  /// offer, and picking between them on the paths alone is picking blind.
  it("gives every candidate its own sentence, chosen by what is known about its bytes", () => {
    const lines = candidateLines(
      state({
        candidates: [
          unread("D:\\a\\BoingBag39-1.lha"),
          read("E:\\b\\BoingBag39-1.lha"),
          { path: "F:\\c\\BoingBag39-1.lha", bytesRead: OTHER },
        ],
      })
    );

    expect(lines.map((line) => line.path)).toEqual([
      "D:\\a\\BoingBag39-1.lha",
      "E:\\b\\BoingBag39-1.lha",
      "F:\\c\\BoingBag39-1.lha",
    ]);
    // Three candidates, three different sentences — never one "these might be
    // it" covering all three.
    expect(lines.map((line) => line.phrase.key)).toEqual([
      "osinstall.slots.guessedByFilenameUnread",
      "osinstall.slots.guessedByFilename",
      "osinstall.slots.guessedByFilenameOtherArtefact",
    ]);
    // The one entitled to name another artefact names it; the other two pass
    // an empty string and their sentences never interpolate it.
    expect(lines[2].phrase.params?.other).toBe("BoingBag 3.9-2");
    expect(lines[0].phrase.params?.other).toBe("");
    expect(lines[0].phrase.params?.name).toBe("BoingBag 3.9-1");
    for (const line of lines) {
      expect(isLeafKey(en, line.phrase.key), `${line.phrase.key} missing from en.json`).toBe(true);
      expect(isLeafKey(tr, line.phrase.key), `${line.phrase.key} missing from tr.json`).toBe(true);
    }
  });

  it("answers nothing for a slot with no candidates", () => {
    expect(candidateLines(state())).toEqual([]);
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
