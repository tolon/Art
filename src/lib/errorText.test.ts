// ART-060. The recognisers only work while Rust keeps saying what they were
// written against, so both sides pin the same sentence: `core::error`'s own
// `the_sentences_the_frontend_recognises_are_pinned_here` on the Rust side,
// and the literals below on this one. Reword either and a test fails naming
// the other.
//
// These strings are **copied from what Rust actually produces**, not written
// to match the regex. That direction matters: a pattern written against a
// sentence somebody invented proves nothing about the sentence somebody ships.

import { describe, expect, it } from "vitest";

import { errorPhrase, errorText, isTranslated, parseError } from "@/lib/errorText";

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
