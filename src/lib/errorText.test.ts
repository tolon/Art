// ART-060. The recognisers only work while Rust keeps saying what they were
// written against, so both sides pin the same sentence: `core::error`'s own
// `the_sentences_the_frontend_recognises_are_pinned_here` on the Rust side,
// and the literals below on this one. Reword either and a test fails naming
// the other.
//
// These strings are **copied from what Rust actually produces**, not written
// to match the regex. That direction matters: a pattern written against a
// sentence somebody invented proves nothing about the sentence somebody ships.

import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";

import en from "@/i18n/en.json";

import { errorPhrase, errorText, isTranslated, parseError } from "@/lib/errorText";
import type { CardRefusal } from "@/lib/cardOs";

/** Exactly what `refuse_unless_free` produces, trailer and all. */
const TREE_OCCUPIED =
  "operation refused to protect data: 'E:\\amiga\\Amigatolon\\hdf' already has something in it " +
  "— a distribution tree is never built over one that is already there. Choose an empty " +
  "folder, or a new one" +
  "\n\nError ID: ART-SAFETY-REFUSED";

/** Exactly what `packagevol::unpack` produces. */
const WRONG_ARCHIVE =
  "invalid input: 'E:\\amiga\\Amigatolon\\os39\\BoingBag39-1-UAE.lha' carries no " +
  "'BoingBag3.9-1' drawer, so it is not the archive this package's installer lives in; " +
  "it holds BoingBag3.9-1-UAE, BoingBag3.9-1-UAE.info" +
  "\n\nError ID: ART-INPUT-INVALID";

/** A `t` that shows what it was asked for, so a test can see the key *and*
 *  the parameters without a catalogue. */
const spy = (key: string, params?: Record<string, unknown>) =>
  `${key}|${JSON.stringify(params ?? {})}`;

describe("splitting an error into its sentence and its id", () => {
  it("takes the id off the trailer ART itself writes", () => {
    const parsed = parseError(TREE_OCCUPIED);
    expect(parsed.id).toBe("ART-SAFETY-REFUSED");
    expect(parsed.sentence.startsWith("operation refused to protect data:")).toBe(true);
    expect(parsed.sentence).not.toContain("Error ID");
  });

  it("a string with no trailer keeps all of itself and has no id", () => {
    const parsed = parseError("something went wrong");
    expect(parsed.id).toBeNull();
    expect(parsed.sentence).toBe("something went wrong");
  });

  it("an Error object is read from its message", () => {
    expect(parseError(new Error(TREE_OCCUPIED)).id).toBe("ART-SAFETY-REFUSED");
  });
});

describe("the two sentences a real person actually met", () => {
  it("rebuilds the occupied-destination refusal with the folder in it", () => {
    const phrase = errorPhrase(TREE_OCCUPIED);
    expect(phrase.key).toBe("errors.treeDestinationOccupied");
    // The folder is the actionable half. A translated sentence that lost it
    // would be worse than the English one it replaced.
    expect(phrase.params).toEqual({
      id: "ART-SAFETY-REFUSED",
      path: "E:\\amiga\\Amigatolon\\hdf",
    });
  });

  it("rebuilds the wrong-archive refusal with all three of its facts", () => {
    const phrase = errorPhrase(WRONG_ARCHIVE);
    expect(phrase.key).toBe("errors.packageWrongArchive");
    expect(phrase.params).toEqual({
      id: "ART-INPUT-INVALID",
      archive: "E:\\amiga\\Amigatolon\\os39\\BoingBag39-1-UAE.lha",
      expected: "BoingBag3.9-1",
      found: "BoingBag3.9-1-UAE, BoingBag3.9-1-UAE.info",
    });
  });
});

describe("a card refusal's typed parameters, not a sentence to parse (round 4, task 3)", () => {
  // Round 3 ruled `CardSourceUnusable.why` a prose `String`; round 4 reverses
  // that (T §2.5/§2.6). `CardOsEnding::{Refused,Failed}` now hand over
  // `code`/`message`/`params` structured — `errorPhrase` must recognise the
  // shape itself, not scrape `message` (which carries no `Error ID:`
  // trailer: `to_string()`, not `user_message()`).
  it("answers a card key with the typed params, not errors.verbatim", () => {
    const refusal = {
      code: "ART-CARD-SOURCE-UNUSABLE",
      message:
        "'E:\\amiga\\NotThere' in Games cannot be used: it does not exist. Remove it from " +
        "Games or fix it.",
      params: { partition: "Games", source: "E:\\amiga\\NotThere", reason: "missing" },
    };
    const phrase = errorPhrase(refusal);
    expect(phrase.key).toBe("errors.cardSourceUnusable.missing");
    expect(phrase.key).not.toBe("errors.verbatim");
    expect(phrase.params).toEqual({
      id: "ART-CARD-SOURCE-UNUSABLE",
      partition: "Games",
      source: "E:\\amiga\\NotThere",
      reason: "missing",
    });
  });

  // `ART-CARD-FINISH-LEFT-BOTH-NAMES` is a real code (`CoreError::CardFinishLeftBothNames`)
  // that is deliberately outside this round's scope (T §2.5/§7 name eleven codes plus
  // `ART-KICKSTART-NOT-PROPOSED`, `ART-CARD-CHECK-FAILED`, `ART-HST-IMAGER-UNUSABLE`, plus
  // `ART-CARD-SOURCE-UNUSABLE`'s reasons — this one is not among them), so it still proves
  // the fallback path once every code task 11 owns is recognised.
  it("a card code this recogniser does not know yet falls back on the typed fields", () => {
    const refusal = {
      code: "ART-CARD-FINISH-LEFT-BOTH-NAMES",
      message: "ART could not finish naming its card",
      params: { image: "x.hdf", partial: "x.hdf.partial", why: "access denied" },
    };
    const phrase = errorPhrase(refusal);
    expect(phrase.key).toBe("errors.verbatim");
    expect(phrase.params).toEqual({
      sentence: "ART could not finish naming its card",
      id: "ART-CARD-FINISH-LEFT-BOTH-NAMES",
    });
  });
});

/**
 * Every card `ART-*` code task 11 owns (round 4, T §2.5/§7 plus the three named
 * explicitly plus `ART-CARD-SOURCE-UNUSABLE`'s six reasons), table-driven: each row is
 * exactly what `error.details()` on the Rust side hands `CardOsEnding.params` for one
 * shape of that refusal, and each must answer its **own** key with the **exact** params
 * its sentence needs — never `errors.verbatim`.
 *
 * The `{{placeholders}}` check is the other half of what `i18n/parity.test.ts`'s
 * "interpolate the same variables in both languages" already covers (en vs tr): this
 * asserts the params `errorPhrase` actually builds carry every variable the resolved
 * catalogue sentence asks for.
 */
function leafText(dotted: string): string {
  const parts = dotted.split(".");
  let node: unknown = en;
  for (const part of parts) {
    if (typeof node !== "object" || node === null) {
      throw new Error(`${dotted} is not a leaf in en.json`);
    }
    node = (node as Record<string, unknown>)[part];
  }
  if (typeof node !== "string") throw new Error(`${dotted} is not a leaf in en.json`);
  return node;
}

function placeholdersOf(text: string): string[] {
  return [...text.matchAll(/{{\s*(\w+)/g)].map((m) => m[1]);
}

interface CardCase {
  name: string;
  refusal: CardRefusal;
  key: string;
}

const CARD_CASES: CardCase[] = [
  {
    name: "Pfs3DriverNotFound, nothing unreadable",
    refusal: {
      code: "ART-PFS3-DRIVER-NOT-FOUND",
      message: "no PFS3 driver was found",
      params: { searched: "paketler, malzeme", unreadable: "" },
    },
    key: "errors.pfs3DriverNotFound.missing",
  },
  {
    name: "Pfs3DriverNotFound, something unreadable",
    refusal: {
      code: "ART-PFS3-DRIVER-NOT-FOUND",
      message: "no PFS3 driver was found",
      params: { searched: "paketler", unreadable: "pfs3aio.lha (access denied)" },
    },
    key: "errors.pfs3DriverNotFound.missingUnreadable",
  },
  {
    name: "WhdloadNotFound",
    refusal: {
      code: "ART-WHDLOAD-NOT-FOUND",
      message: "no WHDLoad was found",
      params: { titles: "3", searched: "paketler, malzeme" },
    },
    key: "errors.whdloadNotFound",
  },
  {
    name: "KickstartNotProposed, not named",
    refusal: {
      code: "ART-KICKSTART-NOT-PROPOSED",
      message: "cannot be placed",
      params: { name: "Kickstart 3.1 (A1200)", reason: "not-named" },
    },
    key: "errors.kickstartNotProposed.notNamed",
  },
  {
    name: "KickstartNotProposed, not offered",
    refusal: {
      code: "ART-KICKSTART-NOT-PROPOSED",
      message: "cannot be placed",
      params: { name: "Kickstart 3.1 (A1200)", reason: "not-offered" },
    },
    key: "errors.kickstartNotProposed.notOffered",
  },
  {
    // The reason whose actionable half used to arrive in English inside the
    // Turkish sentence's parentheses.
    name: "KickstartNotProposed, .RTB missing",
    refusal: {
      code: "ART-KICKSTART-NOT-PROPOSED",
      message: "cannot be placed",
      params: {
        name: "Kickstart 3.1 (A1200)",
        reason: "rtb-missing",
        package: "Aminet util/boot/skick346",
      },
    },
    key: "errors.kickstartNotProposed.rtbMissing",
  },
  {
    name: "CardSourceUnusable, missing",
    refusal: {
      code: "ART-CARD-SOURCE-UNUSABLE",
      message: "cannot be used",
      params: { partition: "Games", source: "E:\\amiga\\NotThere", reason: "missing" },
    },
    key: "errors.cardSourceUnusable.missing",
  },
  {
    name: "CardSourceUnusable, unreadable",
    refusal: {
      code: "ART-CARD-SOURCE-UNUSABLE",
      message: "cannot be used",
      params: {
        partition: "Games",
        source: "E:\\amiga\\Locked",
        reason: "unreadable",
        detail: "access denied",
      },
    },
    key: "errors.cardSourceUnusable.unreadable",
  },
  {
    name: "CardSourceUnusable, archive-unreadable",
    refusal: {
      code: "ART-CARD-SOURCE-UNUSABLE",
      message: "cannot be used",
      params: {
        partition: "Games",
        source: "E:\\amiga\\x.lha",
        reason: "archive-unreadable",
        detail: "truncated",
      },
    },
    key: "errors.cardSourceUnusable.archiveUnreadable",
  },
  {
    name: "CardSourceUnusable, hardfile-not-whdload",
    refusal: {
      code: "ART-CARD-SOURCE-UNUSABLE",
      message: "cannot be used",
      params: {
        partition: "Games",
        source: "E:\\amiga\\x.hdf",
        reason: "hardfile-not-whdload",
        detail: "no slave",
      },
    },
    key: "errors.cardSourceUnusable.hardfileNotWhdload",
  },
  {
    name: "CardSourceUnusable, not-an-amiga-source",
    refusal: {
      code: "ART-CARD-SOURCE-UNUSABLE",
      message: "cannot be used",
      params: {
        partition: "Games",
        source: "E:\\amiga\\x.exe",
        reason: "not-an-amiga-source",
        detail: "exe",
      },
    },
    key: "errors.cardSourceUnusable.notAnAmigaSource",
  },
  {
    name: "CardSourceUnusable, not-a-folder",
    refusal: {
      code: "ART-CARD-SOURCE-UNUSABLE",
      message: "cannot be used",
      params: { partition: "System", source: "E:\\amiga\\tree.txt", reason: "not-a-folder" },
    },
    key: "errors.cardSourceUnusable.notAFolder",
  },
  {
    name: "CardNamesNeedHstImager, nothing more",
    refusal: {
      code: "ART-CARD-NAMES-NEED-HST",
      message: "holds names ART's own PFS3 writer cannot write",
      params: { partition: "Games", paths: "français.lha", more: "0" },
    },
    key: "errors.cardNamesNeedHst.noMore",
  },
  {
    name: "CardNamesNeedHstImager, more",
    refusal: {
      code: "ART-CARD-NAMES-NEED-HST",
      message: "holds names ART's own PFS3 writer cannot write",
      params: { partition: "Games", paths: "français.lha, español.lha", more: "4" },
    },
    key: "errors.cardNamesNeedHst.withMore",
  },
  {
    name: "CardDoesNotFit, does-not-fit with a largest partition",
    refusal: {
      code: "ART-CARD-DOES-NOT-FIT",
      message: "these partitions do not fit this card",
      params: { kind: "does-not-fit", needed: "2000", available: "1000", largest: "Games" },
    },
    key: "errors.cardDoesNotFit.doesNotFit",
  },
  {
    name: "CardDoesNotFit, does-not-fit with no largest partition",
    refusal: {
      code: "ART-CARD-DOES-NOT-FIT",
      message: "these partitions do not fit this card",
      params: { kind: "does-not-fit", needed: "2000", available: "1000" },
    },
    key: "errors.cardDoesNotFit.doesNotFitNoPartition",
  },
  {
    name: "CardDoesNotFit, partition-too-large",
    refusal: {
      code: "ART-CARD-DOES-NOT-FIT",
      message: "would need more bytes than one PFS3 partition can hold",
      params: { kind: "partition-too-large", volumeName: "Games", bytes: "5000000000" },
    },
    key: "errors.cardDoesNotFit.partitionTooLarge",
  },
  {
    name: "CardDoesNotFit, card-too-small",
    refusal: {
      code: "ART-CARD-DOES-NOT-FIT",
      message: "too small to hold System and the RDB",
      params: { kind: "card-too-small", cardGb: "1" },
    },
    key: "errors.cardDoesNotFit.cardTooSmall",
  },
  {
    name: "CardDoesNotFit, partition-content-does-not-fit",
    refusal: {
      code: "ART-CARD-DOES-NOT-FIT",
      message: "tree needs more blocks than the partition has room for",
      params: {
        kind: "partition-content-does-not-fit",
        volumeName: "System",
        neededBlocks: "9000",
        availableBlocks: "8000",
      },
    },
    key: "errors.cardDoesNotFit.partitionContentDoesNotFit",
  },
  {
    name: "CardDoesNotFit, system-additions-do-not-fit",
    refusal: {
      code: "ART-CARD-DOES-NOT-FIT",
      message: "System needs more blocks with its additions than it has room for",
      params: {
        kind: "system-additions-do-not-fit",
        neededBlocks: "9000",
        availableBlocks: "8000",
        treeBlocks: "7000",
        kickstarts: "Kickstart 3.1 (A1200), Kickstart 3.1 (A600)",
      },
    },
    key: "errors.cardDoesNotFit.systemAdditionsDoNotFit",
  },
  {
    // The optional field `details()` supplies when the card adds a WHDLoad —
    // which no case named until the key-set check below found it dropped.
    name: "CardDoesNotFit, system-additions-do-not-fit with a WHDLoad",
    refusal: {
      code: "ART-CARD-DOES-NOT-FIT",
      message: "System needs more blocks with its additions than it has room for",
      params: {
        kind: "system-additions-do-not-fit",
        neededBlocks: "9000",
        availableBlocks: "8000",
        treeBlocks: "7000",
        kickstarts: "Kickstart 3.1 (A1200)",
        whdload: "WHDLoad 18.9",
      },
    },
    key: "errors.cardDoesNotFit.systemAdditionsDoNotFitWithWhdload",
  },
  {
    name: "NotEnoughSpace, image",
    refusal: {
      code: "ART-NOT-ENOUGH-SPACE",
      message: "has less free space than the build needs",
      params: { place: "E:\\amiga\\card.hdf", needed: "2000", available: "1000", what: "image" },
    },
    key: "errors.notEnoughSpace.image",
  },
  {
    name: "NotEnoughSpace, scratch",
    refusal: {
      code: "ART-NOT-ENOUGH-SPACE",
      message: "has less free space than the build needs",
      params: { place: "E:\\amiga\\scratch", needed: "2000", available: "1000", what: "scratch" },
    },
    key: "errors.notEnoughSpace.scratch",
  },
  {
    name: "NotEnoughSpace, image-and-scratch",
    refusal: {
      code: "ART-NOT-ENOUGH-SPACE",
      message: "has less free space than the build needs",
      params: {
        place: "E:\\amiga",
        needed: "2000",
        available: "1000",
        what: "image-and-scratch",
      },
    },
    key: "errors.notEnoughSpace.imageAndScratch",
  },
  {
    name: "CardCheckFailed",
    refusal: {
      code: "ART-CARD-CHECK-FAILED",
      message: "failed its checks",
      params: { failures: "2", checks: "root-block, boot-block" },
    },
    key: "errors.cardCheckFailed",
  },
  {
    name: "HstImagerUnusable, no tool given",
    refusal: {
      code: "ART-HST-IMAGER-UNUSABLE",
      message: "hold names only hst-imager can write",
      params: { partitions: "Games", path: "", why: "" },
    },
    key: "errors.hstImagerUnusable.noTool",
  },
  {
    name: "HstImagerUnusable, tool given but unusable",
    refusal: {
      code: "ART-HST-IMAGER-UNUSABLE",
      message: "hold names only hst-imager can write",
      params: {
        partitions: "Games",
        path: "E:\\tools\\hst.imager.exe",
        why: "the file does not exist",
      },
    },
    key: "errors.hstImagerUnusable.toolUnusable",
  },
  {
    name: "KickstartSourceChanged",
    refusal: {
      code: "ART-KICKSTART-SOURCE-CHANGED",
      message: "cannot be placed",
      params: {
        name: "Kickstart 3.1 (A1200)",
        path: "E:\\amiga\\roms\\kick.rom",
        why: "is no longer there",
      },
    },
    key: "errors.kickstartSourceChanged",
  },
  {
    name: "CardPartitionTooManyEntries",
    refusal: {
      code: "ART-CARD-PARTITION-TOO-MANY-ENTRIES",
      message: "would hold more files and folders than ART counts back",
      params: { partition: "Games", entries: "50000", bound: "40000" },
    },
    key: "errors.cardPartitionTooManyEntries",
  },
  {
    name: "PartialImageExists",
    refusal: {
      code: "ART-CARD-PARTIAL-EXISTS",
      message: "is a half-built card image from an earlier run",
      params: { path: "E:\\amiga\\card.hdf.partial" },
    },
    key: "errors.cardPartialExists",
  },
];

describe("every card ART-* code task 11 owns (round 4)", () => {
  for (const { name, refusal, key } of CARD_CASES) {
    it(`${name} answers ${key}, not errors.verbatim, with every placeholder covered`, () => {
      const phrase = errorPhrase(refusal);
      expect(phrase.key, name).toBe(key);
      expect(phrase.key, name).not.toBe("errors.verbatim");

      // Every param `errorPhrase` captured is passed straight through, plus `id`.
      expect(phrase.params, name).toEqual({ id: refusal.code, ...refusal.params });

      // The catalogue sentence's own `{{placeholders}}` are all satisfied by
      // what `errorPhrase` handed over — the other half of what
      // `i18n/parity.test.ts` checks between en and tr.
      const text = leafText(key);
      const missing = placeholdersOf(text).filter(
        (name) => !Object.prototype.hasOwnProperty.call(phrase.params ?? {}, name)
      );
      expect(missing, `${name}: ${text}`).toEqual([]);
    });
  }

  it("covers every code task 11 owns, and only once each key that does not branch", () => {
    const codes = new Set(CARD_CASES.map((c) => c.refusal.code));
    expect([...codes].sort()).toEqual(
      [
        "ART-CARD-CHECK-FAILED",
        "ART-CARD-DOES-NOT-FIT",
        "ART-CARD-NAMES-NEED-HST",
        "ART-CARD-PARTIAL-EXISTS",
        "ART-CARD-PARTITION-TOO-MANY-ENTRIES",
        "ART-CARD-SOURCE-UNUSABLE",
        "ART-HST-IMAGER-UNUSABLE",
        "ART-KICKSTART-NOT-PROPOSED",
        "ART-KICKSTART-SOURCE-CHANGED",
        "ART-NOT-ENOUGH-SPACE",
        "ART-PFS3-DRIVER-NOT-FOUND",
        "ART-WHDLOAD-NOT-FOUND",
      ].sort()
    );
  });
});

describe("everything else is Rust's own English, unchanged", () => {
  it("an unrecognised sentence under a known id falls back verbatim", () => {
    const raw = "operation refused to protect data: something else entirely\n\nError ID: ART-SAFETY-REFUSED";
    const phrase = errorPhrase(raw);
    expect(phrase.key).toBe("errors.verbatim");
    expect(phrase.params?.sentence).toBe(
      "operation refused to protect data: something else entirely"
    );
    expect(isTranslated(raw)).toBe(false);
  });

  /// **The id narrows before the pattern runs.** Without that, a sentence
  /// under a different id that happened to read alike would be rebuilt as
  /// the wrong error — with the wrong id printed under it.
  it("the same words under a different id are not recognised", () => {
    const raw =
      "operation refused to protect data: 'E:\\x' already has something in it" +
      "\n\nError ID: ART-IO";
    expect(errorPhrase(raw).key).toBe("errors.verbatim");
  });

  /// **An empty id is worse than none.** The first version of this rendered
  /// `errors.verbatim` with `id: ""`, which put a bare "Error ID:" on screen
  /// telling the user to quote something that was not there. Its own key now.
  it("an error with no id renders the sentence and nothing else", () => {
    const phrase = errorPhrase("the emulator would not start");
    expect(phrase.key).toBe("errors.verbatimNoId");
    expect(phrase.params).toEqual({ sentence: "the emulator would not start" });
    expect(phrase.params).not.toHaveProperty("id");
  });
});

describe("errorText renders through the caller's own translator", () => {
  it("passes the key and the captured parts straight through", () => {
    expect(errorText(spy, TREE_OCCUPIED)).toBe(
      'errors.treeDestinationOccupied|{"id":"ART-SAFETY-REFUSED","path":"E:\\\\amiga\\\\Amigatolon\\\\hdf"}'
    );
  });
});

// ---------------------------------------------------------------------------
// The table above, checked against the Rust that fills it (round 4 final
// review, I3)
// ---------------------------------------------------------------------------

/**
 * **`CARD_CASES` is a hand-written copy of `CoreError::details()`, and copies
 * drift** (CLAUDE.md, *a test that reads a table instead of the file is a
 * copy*).
 *
 * Nothing pinned the two together. `core/error.rs`'s own
 * `every_card_error_s_details_are_not_empty` asserts only that `details()`
 * answers *something*, and the cases above assert only that every
 * `{{placeholder}}` of the catalogue sentence is covered by the params **the
 * table itself supplies**. So renaming one key on the Rust side — `source` to
 * `source_path`, say — left both sides green while the rendered Turkish
 * sentence kept a literal `{{source}}` in it, over the exact wire the owner's
 * decision 4 depends on.
 *
 * This reads the literals out of `core/error.rs` the way
 * `i18n/job-title-keys.test.ts` reads `JobTitle::new` out of the Rust tree,
 * and checks the two directions that matter: no case may name a key Rust does
 * not supply for that code, and no key Rust supplies for a code may be
 * unnamed by every case for it. Rename either end and this is red, pointing
 * at the other.
 */
const ERROR_RS = resolve(__dirname, "..", "..", "src-tauri", "src", "core", "error.rs");

/** Every `("literal", …)` key in one `details()` arm. */
function keysIn(arm: string): string[] {
  return [...arm.matchAll(/\("([a-zA-Z_]+)",/g)].map((m) => m[1]);
}

/** The body of a `fn details(…)` that starts at `from`, to its `match`'s end. */
function detailsBodyAt(source: string, from: number): string {
  const start = source.indexOf("match self {", from);
  const end = source.indexOf("\n    }", start);
  return source.slice(start, end);
}

/** The keys the `details()` of one sub-enum's own `impl` block supplies. */
function subEnumKeys(source: string, name: string): string[] {
  const block = source.indexOf(`impl ${name} {`);
  expect(block, `core/error.rs should hold an impl for ${name}`).toBeGreaterThan(-1);
  const head = source.indexOf("fn details(&self)", block);
  expect(head, `${name} should have a details()`).toBeGreaterThan(-1);
  return keysIn(detailsBodyAt(source, head));
}

/**
 * Which sub-enum each delegating arm folds in. Named per variant rather than
 * sniffed from the expression, because two of them spell it `why.details()`
 * and a parser that read the expression alone would give them each other's
 * keys — the exact class of quiet wrongness this whole check exists against.
 */
const DELEGATES: Record<string, string> = {
  CardSourceUnusable: "UnusableSource",
  KickstartNotProposed: "KickstartNotProposedWhy",
  CardDoesNotFit: "SizingRefusal",
};

function readRustDetailKeys(): Map<string, Set<string>> {
  const source = readFileSync(ERROR_RS, "utf8");

  const subKeys = new Map<string, string[]>();
  for (const name of new Set(Object.values(DELEGATES))) {
    subKeys.set(name, subEnumKeys(source, name));
  }

  // `CoreError`'s own is the only `pub fn details`; the sub-enums' are private.
  const coreHead = source.indexOf("pub fn details(&self)");
  expect(coreHead, "core/error.rs should hold CoreError::details").toBeGreaterThan(-1);
  const coreBody = detailsBodyAt(source, coreHead);

  // Variant → `ART-*` code, straight out of `code()`'s own match arms.
  const codeOf = new Map<string, string>();
  for (const match of source.matchAll(/Self::(\w+)[^\n]*=> "(ART-[A-Z0-9-]+)"/g)) {
    codeOf.set(match[1], match[2]);
  }

  const byCode = new Map<string, Set<string>>();
  const arms = coreBody.split(/\n            Self::/).slice(1);
  for (const arm of arms) {
    const variant = /^(\w+)/.exec(arm)?.[1];
    const code = variant ? codeOf.get(variant) : undefined;
    if (!code) continue;
    const keys = new Set(keysIn(arm));
    // An arm that hands the question to a sub-enum can carry every one of
    // that enum's own keys under this code.
    const delegate = DELEGATES[variant!];
    if (delegate) {
      expect(arm, `${variant} should delegate to ${delegate}`).toMatch(/\.details\(\)/);
      for (const key of subKeys.get(delegate) ?? []) keys.add(key);
    }
    byCode.set(code, keys);
  }
  return byCode;
}

describe("the card table and Rust's own details() cannot drift (I3)", () => {
  const rust = readRustDetailKeys();

  it("finds a key set for every code the table covers", () => {
    for (const code of new Set(CARD_CASES.map((c) => c.refusal.code))) {
      expect(rust.get(code), `${code} has no details() arm in core/error.rs`).toBeTruthy();
    }
  });

  it("names no parameter Rust does not supply for that code", () => {
    for (const { name, refusal } of CARD_CASES) {
      const supplied = rust.get(refusal.code);
      if (!supplied) continue;
      const invented = Object.keys(refusal.params).filter((key) => !supplied.has(key));
      expect(invented, `${name}: Rust supplies ${[...supplied].sort().join(", ")}`).toEqual([]);
    }
  });

  it("leaves no parameter Rust supplies unnamed by every case for its code", () => {
    for (const [code, supplied] of rust) {
      const cases = CARD_CASES.filter((c) => c.refusal.code === code);
      if (cases.length === 0) continue;
      const named = new Set(cases.flatMap((c) => Object.keys(c.refusal.params)));
      const unused = [...supplied].filter((key) => !named.has(key));
      expect(unused, `${code}: no case supplies these`).toEqual([]);
    }
  });
});
