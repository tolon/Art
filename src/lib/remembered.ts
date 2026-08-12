// Choices the user made, kept between runs.
//
// The rule this module exists to serve, in the user's words: *"program yapılan
// ayarların hepsini hatırlamalı, kullanıcı değiştirmeden hiçbir ayar
// değişmemeli"* — every setting is remembered, and nothing changes unless the
// user changes it. That is a rule about the whole application, not about one
// screen, so it needs one mechanism rather than a dozen `useState`s that each
// forget in their own way.
//
// ART is not a general-purpose tool someone opens once. It is opened again and
// again for the same narrow job, by people who should not have to re-pick the
// same filesystem, the same view, the same folder every time.
//
// **What comes back off disk is data, not a promise.** `settings.json` is a
// file a user can edit, an older ART may have written, and a half-finished
// write may have truncated. So nothing here casts: every value is checked
// against the guard the caller supplies, and a value that fails is replaced by
// the fallback rather than handed on. A screen that will not open because its
// remembered view mode was the string `"grd"` is worse than one that forgot it.
//
// The `@/lib/paneSession` module does the same job for the Files screen and
// predates this one; it stays as it is, because a tab set is a structure with
// its own rules rather than a bag of independent choices.

/** The bag of remembered values, as it sits in `settings.json`. */
export type RememberedStore = Record<string, unknown>;

/** A check that also tells TypeScript what it proved. */
export type Guard<T> = (value: unknown) => value is T;

function asStore(value: unknown): RememberedStore {
  return typeof value === "object" && value !== null && !Array.isArray(value)
    ? (value as RememberedStore)
    : {};
}

/**
 * One remembered value, or the fallback when there is nothing usable.
 *
 * Missing and invalid are deliberately the same answer. The caller cannot act
 * differently on them — there is no sensible "your saved value was corrupt"
 * dialog for a view mode — and pretending otherwise would only spread the
 * check outward.
 */
export function recall<T>(store: unknown, key: string, isValid: Guard<T>, fallback: T): T {
  const held = asStore(store)[key];
  return isValid(held) ? held : fallback;
}

/**
 * A remembered object, rebuilt **field by field**.
 *
 * Not `recall` with an object guard, and the difference matters over time: a
 * config that gains a field in a later ART would fail a whole-object guard,
 * and the user would silently lose every choice they had made in it. Here each
 * field stands on its own — the known ones come back, anything new takes its
 * default, and a single bad field costs only itself.
 *
 * Fields the spec does not mention are dropped, so a key removed from ART does
 * not linger in the settings file forever.
 */
export function recallInto<T extends object>(
  store: unknown,
  key: string,
  spec: { [K in keyof T]: Guard<T[K]> },
  fallback: T
): T {
  const held = asStore(asStore(store)[key]);
  const result = { ...fallback };
  for (const field of Object.keys(spec) as (keyof T)[]) {
    const value = held[field as string];
    if (spec[field](value)) result[field] = value;
  }
  return result;
}

/** The store with one value written. Returns a new object; nothing is mutated. */
export function remember(store: unknown, key: string, value: unknown): RememberedStore {
  return { ...asStore(store), [key]: value };
}

/** The store with one value dropped — for a "forget this" the user asked for. */
export function forget(store: unknown, key: string): RememberedStore {
  const next = { ...asStore(store) };
  delete next[key];
  return next;
}

// ---------------------------------------------------------------------------
// Guards
//
// Small and named for what they mean rather than for the JavaScript type,
// because the call sites read as sentences: `recall(store, "viewMode",
// isOneOf("grid", "list"), "grid")`.
// ---------------------------------------------------------------------------

export function isFlag(value: unknown): value is boolean {
  return typeof value === "boolean";
}

export function isText(value: unknown): value is string {
  return typeof value === "string";
}

export function isTextOrNothing(value: unknown): value is string | null {
  return value === null || typeof value === "string";
}

/** A list of strings — a command history, a set of chosen names. */
export function isTextList(value: unknown): value is string[] {
  return Array.isArray(value) && value.every((entry) => typeof entry === "string");
}

/**
 * One of a fixed set.
 *
 * The set is the code's, not the file's: a value that was legal in an older ART
 * and is not one now falls back, rather than reaching a `switch` that has no
 * branch for it.
 */
export function isOneOf<T extends string>(...allowed: readonly T[]): Guard<T> {
  return (value: unknown): value is T =>
    typeof value === "string" && (allowed as readonly string[]).includes(value);
}

/**
 * A whole number inside a range.
 *
 * Ranges rather than bare numbers because these values reach layout and
 * geometry: a remembered font size of `-1` or `1e9` is not a preference, it is
 * a broken screen. `NaN` and `Infinity` fail, which `typeof` alone would not
 * catch.
 */
export function isWholeNumberBetween(min: number, max: number): Guard<number> {
  return (value: unknown): value is number =>
    typeof value === "number" && Number.isInteger(value) && value >= min && value <= max;
}

/** A finite number inside a range, for the values that are genuinely fractional. */
export function isNumberBetween(min: number, max: number): Guard<number> {
  return (value: unknown): value is number =>
    typeof value === "number" && Number.isFinite(value) && value >= min && value <= max;
}
