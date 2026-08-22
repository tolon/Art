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
  // something that is not there. Caught by
  // `OsInstall.test.tsx`'s own preview-failure test, which is the sort of
  // thing this project keeps learning: the defect was in the sentence, and
  // only a test that read the sentence saw it.
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
