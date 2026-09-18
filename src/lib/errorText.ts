// Turning a Rust error into a sentence the user's own language can read
// (ART-060).
//
// ## What this can and cannot do, said first
//
// **A free-text English sentence cannot be translated.** A Turkish sentence
// can only be *built*, on this side, out of parts. So this module does not
// "translate errors"; it recognises the ones ART actually produces, pulls the
// parts out, and asks the catalogue to build the sentence again. Anything it
// does not recognise is returned **exactly as Rust wrote it** — English, with
// its error id — which is what happens today and is never worse than today.
//
// ## Why recognisers and not a wire change
//
// `AppError` serialises to a string, and 134 places render it with
// `String(e)`. Emitting a structured object instead would be the cleaner
// design and would break every one of them at once, on screens nobody can
// drive. The string ART sends has a shape it controls — `CoreError::user_message`
// is the one place that writes it — so reading it back is reliable in a way
// that scraping somebody else's output would not be.
//
// **The fragility is real and it is converted into a build failure.** Each
// recogniser below is pinned by a test against the exact sentence Rust
// produces, and the Rust side pins the same sentence from its end. Reword
// either and a test fails pointing at the other.
//
// ## Where the list comes from
//
// Not from reading the code: from the owner's own `operations.jsonl`. 37
// operations, 10 failures, **two distinct sentences**. There are 543 places in
// the crate that construct a free-text error; two of them are what a real
// person actually met. The list grows when somebody meets something new, not
// when somebody goes looking.

import type { CardRefusal } from "@/lib/cardOs";
import type { Phrase } from "@/lib/phrase";

/** The trailer `CoreError::user_message` appends, and the only thing here that
 *  depends on ART's own formatting rather than on a sentence. */
const ID_MARKER = "\n\nError ID: ";

export interface ParsedError {
  /** The stable `ART-*` id, or null when the string carries none. */
  id: string | null;
  /** Everything before the trailer — the sentence itself. */
  sentence: string;
  /** Exactly what Rust sent, for the fallback and for the log. */
  raw: string;
}

/** Split an error string into its sentence and its stable id. */
export function parseError(value: unknown): ParsedError {
  const raw = value instanceof Error ? value.message : String(value ?? "");
  const at = raw.lastIndexOf(ID_MARKER);
  if (at < 0) return { id: null, sentence: raw.trim(), raw };
  return {
    id: raw.slice(at + ID_MARKER.length).trim() || null,
    sentence: raw.slice(0, at).trim(),
    raw,
  };
}

/**
 * A sentence ART is known to produce, and the catalogue key that rebuilds it.
 *
 * `id` narrows before the pattern runs, so a regex can stay simple without
 * risking a match against an unrelated error that happens to read alike.
 */
interface Recogniser {
  id: string;
  pattern: RegExp;
  /** Names for the capture groups, in order. */
  captures: string[];
  key: string;
}

/**
 * A card refusal's `ART-*` code and the catalogue key that says it — round 4,
 * task 3's proof of shape, task 11's full set. Unlike [`RECOGNISERS`] below,
 * this side needs no regex: `CardOsEnding::{Refused,Failed}` already hand
 * over `code` and `params` typed (`error.details()`, `core/error.rs`), so
 * there is nothing to parse back out of a sentence.
 *
 * `key` is a function where one code reads several ways — `error.details()`
 * hands over a `kind`/`reason`/emptiness tag precisely so the sentence can be
 * chosen without parsing anything (`CardSourceUnusable`'s six `reason`s,
 * `CardDoesNotFit`'s five `kind`s, and three codes with a present/absent
 * field). Every field a sentence needs already exists in `details()` — round
 * 4 task 3 covered every code this task owns, so no Rust change was needed
 * here.
 */
interface CardRecogniser {
  code: string;
  key: string | ((params: Record<string, string>) => string);
}

const CARD_RECOGNISERS: CardRecogniser[] = [
  { code: "ART-CARD-SOURCE-UNUSABLE", key: cardSourceUnusableKey },
  { code: "ART-PFS3-DRIVER-NOT-FOUND", key: pfs3DriverNotFoundKey },
  { code: "ART-WHDLOAD-NOT-FOUND", key: "errors.whdloadNotFound" },
  { code: "ART-KICKSTART-NOT-PROPOSED", key: "errors.kickstartNotProposed" },
  { code: "ART-CARD-NAMES-NEED-HST", key: cardNamesNeedHstKey },
  { code: "ART-CARD-DOES-NOT-FIT", key: cardDoesNotFitKey },
  { code: "ART-NOT-ENOUGH-SPACE", key: notEnoughSpaceKey },
  { code: "ART-CARD-CHECK-FAILED", key: "errors.cardCheckFailed" },
  { code: "ART-HST-IMAGER-UNUSABLE", key: hstImagerUnusableKey },
  { code: "ART-KICKSTART-SOURCE-CHANGED", key: "errors.kickstartSourceChanged" },
  { code: "ART-CARD-PARTITION-TOO-MANY-ENTRIES", key: "errors.cardPartitionTooManyEntries" },
  { code: "ART-CARD-PARTIAL-EXISTS", key: "errors.cardPartialExists" },
];

/** `CoreError::CardSourceUnusable`'s `why.reason` (`UnusableSource`'s own
 *  `details()`) — one sentence per reason, because "cannot be used" with no
 *  reason is the one thing a user cannot act on. */
function cardSourceUnusableKey(params: Record<string, string>): string {
  switch (params.reason) {
    case "missing":
      return "errors.cardSourceUnusable.missing";
    case "unreadable":
      return "errors.cardSourceUnusable.unreadable";
    case "archive-unreadable":
      return "errors.cardSourceUnusable.archiveUnreadable";
    case "hardfile-not-whdload":
      return "errors.cardSourceUnusable.hardfileNotWhdload";
    case "not-an-amiga-source":
      return "errors.cardSourceUnusable.notAnAmigaSource";
    case "not-a-folder":
      return "errors.cardSourceUnusable.notAFolder";
    default:
      // `UnusableSource` is a closed six-member enum on the Rust side; this
      // is unreachable today and only a defensive net against a seventh
      // member arriving here before its sentence does.
      return "errors.verbatim";
  }
}

/** `Pfs3DriverNotFound.unreadable` is said only when it is not empty — most
 *  searches never meet an archive ART could not read at all (same rule as
 *  `pfs3_driver_not_found_message` on the Rust side). */
function pfs3DriverNotFoundKey(params: Record<string, string>): string {
  return params.unreadable
    ? "errors.pfs3DriverNotFound.missingUnreadable"
    : "errors.pfs3DriverNotFound.missing";
}

/** `CardNamesNeedHstImager.more` is said only when it is over zero. */
function cardNamesNeedHstKey(params: Record<string, string>): string {
  return Number(params.more) > 0 ? "errors.cardNamesNeedHst.withMore" : "errors.cardNamesNeedHst.noMore";
}

/** `CoreError::CardDoesNotFit`'s `SizingRefusal.details()` `kind` tag — one
 *  sentence per sizing refusal, since each names a different set of fields
 *  (bytes for one, PFS3 blocks for another) that no single sentence could
 *  read. `does-not-fit` further splits on whether a `largest` partition was
 *  named, the same optional-field rule `Pfs3DriverNotFound` follows above. */
function cardDoesNotFitKey(params: Record<string, string>): string {
  switch (params.kind) {
    case "does-not-fit":
      return params.largest
        ? "errors.cardDoesNotFit.doesNotFit"
        : "errors.cardDoesNotFit.doesNotFitNoPartition";
    case "partition-too-large":
      return "errors.cardDoesNotFit.partitionTooLarge";
    case "card-too-small":
      return "errors.cardDoesNotFit.cardTooSmall";
    case "partition-content-does-not-fit":
      return "errors.cardDoesNotFit.partitionContentDoesNotFit";
    case "system-additions-do-not-fit":
      return "errors.cardDoesNotFit.systemAdditionsDoNotFit";
    default:
      // `SizingRefusal` is a closed five-member enum on the Rust side; see
      // `cardSourceUnusableKey`'s own note.
      return "errors.verbatim";
  }
}

/** `CoreError::NotEnoughSpace`'s `what` (`SpacePlace::tag`) — each place has
 *  its own next step (image folder vs. scratch folder in Settings). */
function notEnoughSpaceKey(params: Record<string, string>): string {
  switch (params.what) {
    case "image":
      return "errors.notEnoughSpace.image";
    case "scratch":
      return "errors.notEnoughSpace.scratch";
    case "image-and-scratch":
      return "errors.notEnoughSpace.imageAndScratch";
    default:
      // `SpacePlace` is a closed three-member enum on the Rust side; see
      // `cardSourceUnusableKey`'s own note.
      return "errors.verbatim";
  }
}

/** `CoreError::HstImagerUnusable`'s `path` is empty exactly when no
 *  hst-imager was given to this build at all (`refuse_without_hst_imager`'s
 *  `None` branch) — a different next step from one that was given and did
 *  not work. */
function hstImagerUnusableKey(params: Record<string, string>): string {
  return params.path ? "errors.hstImagerUnusable.toolUnusable" : "errors.hstImagerUnusable.noTool";
}

/** Whether `value` is a [`CardRefusal`] — the shape `CardOsEnding`'s
 *  `refused` and `failed` carry, as opposed to a raw string or `Error`. */
function isCardRefusal(value: unknown): value is CardRefusal {
  if (typeof value !== "object" || value === null) return false;
  const v = value as Partial<CardRefusal>;
  return (
    typeof v.code === "string" &&
    typeof v.message === "string" &&
    typeof v.params === "object" &&
    v.params !== null
  );
}

/**
 * The two the owner actually met, from their own operation log.
 *
 * Each is pinned against the real sentence in `errorText.test.ts`, and the
 * Rust side pins the same wording in `core::error`'s own tests.
 */
const RECOGNISERS: Recogniser[] = [
  {
    // "operation refused to protect data: '…' already has something in it — a
    //  distribution tree is never built over one that is already there.
    //  Choose an empty folder, or a new one"
    //
    // The owner's log carries the *older* wording of this one ("already
    // exists"), from before ART-203 changed it. That is the whole argument
    // for the pin in `core::error`'s tests: a sentence gets reworded and
    // nothing notices until somebody reads a screen.
    id: "ART-SAFETY-REFUSED",
    pattern: /^operation refused to protect data: '(.+?)' already has something in it\b/,
    captures: ["path"],
    key: "errors.treeDestinationOccupied",
  },
  {
    // "invalid input: '…\BoingBag39-1-UAE.lha' carries no 'BoingBag3.9-1'
    //  drawer, so it is not the archive this package's installer lives in;
    //  it holds BoingBag3.9-1-UAE, BoingBag3.9-1-UAE.info"
    id: "ART-INPUT-INVALID",
    pattern:
      /^invalid input: '(.+?)' carries no '(.+?)' drawer, so it is not the archive this package's installer lives in; it holds (.+)$/,
    captures: ["archive", "expected", "found"],
    key: "errors.packageWrongArchive",
  },
];

/**
 * What to put on screen for `value`.
 *
 * A `Phrase` rather than a string so the caller renders it the way it renders
 * every other sentence, and so `src/lib` stays free of the i18next singleton
 * (CLAUDE.md's rule). `errors.verbatim` is the fallback: it takes Rust's own
 * English as a parameter and does not pretend to be a translation of it.
 */
export function errorPhrase(value: unknown): Phrase {
  if (isCardRefusal(value)) {
    const recogniser = CARD_RECOGNISERS.find((r) => r.code === value.code);
    const key = recogniser
      ? typeof recogniser.key === "function"
        ? recogniser.key(value.params)
        : recogniser.key
      : null;
    if (key && key !== "errors.verbatim") {
      return { key, params: { id: value.code, ...value.params } };
    }
    // Either not one of the card codes this recogniser knows (a code outside
    // task 11's list, e.g. `ART-CARD-FINISH-LEFT-BOTH-NAMES`), or a known
    // code whose own `kind`/`reason` tag was not one of its closed enum's
    // members: the same two fallbacks as below, built from the typed fields
    // directly rather than through `parseError`, since `message` carries no
    // `Error ID:` trailer (`CardOsEnding.message` is `to_string()`, not
    // `user_message()` — research-tree.md §2.6, note 2).
    return { key: "errors.verbatim", params: { sentence: value.message, id: value.code } };
  }

  const parsed = parseError(value);

  for (const recogniser of RECOGNISERS) {
    if (parsed.id !== recogniser.id) continue;
    const match = recogniser.pattern.exec(parsed.sentence);
    if (!match) continue;
    const params: Record<string, string> = { id: parsed.id };
    recogniser.captures.forEach((name, at) => {
      params[name] = match[at + 1] ?? "";
    });
    return { key: recogniser.key, params };
  }

  // **Two fallbacks, because an empty id is worse than none.** With
  // `errors.verbatim` alone, an error carrying no `ART-*` trailer rendered a
  // bare "Error ID:" with nothing after it — a line telling the user to quote
  // something that is not there. Held by `errorText.test.ts`'s
  // `an error with no id renders the sentence and nothing else` — which
  // is the sort of thing this project keeps learning: the defect was in
  // the sentence, and only a test that read the sentence saw it.
  if (parsed.id === null) {
    return { key: "errors.verbatimNoId", params: { sentence: parsed.sentence } };
  }
  return {
    key: "errors.verbatim",
    params: { sentence: parsed.sentence, id: parsed.id },
  };
}

/**
 * Whether ART can say this one in the user's own language.
 *
 * Exported for the tests and for a future screen that wants to mark the
 * difference; nothing renders differently on it today.
 */
export function isTranslated(value: unknown): boolean {
  const key = errorPhrase(value).key;
  return key !== "errors.verbatim" && key !== "errors.verbatimNoId";
}

/**
 * The sentence, already rendered — for a component that has `t` to hand.
 *
 * `translate` is passed in rather than imported: `src/lib` is pure TypeScript
 * with no i18next singleton (CLAUDE.md), which is also why [`errorPhrase`]
 * returns a `Phrase` rather than a string.
 */
export function errorText(
  translate: (key: string, params?: Record<string, unknown>) => string,
  value: unknown
): string {
  const phrase = errorPhrase(value);
  return translate(phrase.key, phrase.params);
}
