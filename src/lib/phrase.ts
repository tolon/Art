// A translatable sentence, without a translator.
//
// `src/lib/*` is pure TypeScript — no i18next singleton, so its functions stay
// unit-testable without booting a translator and stay usable from a future CLI
// shell. A `Phrase` is the data a sentence needs: which catalogue key, and the
// values to interpolate into it. The component that renders it calls
// `t(phrase.key, phrase.params)` — see src/i18n/en.json for the catalogue.
export type Phrase = { key: string; params?: Record<string, string | number> };

// A handful of `src/lib` functions have no translator to finish the sentence
// with — they resolve to a catalogue value whose placeholders need another
// Phrase rendered first (e.g. two `formatSize` results), or a join of two
// independently pluralised fragments. Those return `PartialPhrase<K>`
// instead of `Phrase`: same shape at runtime (`{ key, params }`), but `K`
// names the interpolation variables the function did *not* supply, and
// `params`'s type is poisoned so it cannot be mistaken for a complete one.
//
// i18next renders a genuinely missing interpolation variable as the literal
// `{{now}}` on screen, not as nothing — so the obvious-looking
// `t(phrase.key, phrase.params)` is exactly the bug this type exists to
// catch at compile time instead of on screen. Concretely:
//
//   - `PartialPhrase<K>` is never assignable to `Phrase`, so a signature
//     that asks for a complete `Phrase` rejects an incomplete one outright.
//   - `params`'s value type for every key in `K` is `MissingParam`, which is
//     not assignable to `string | number` — so passing `phrase.params`
//     straight to a `t(key, params?: Record<string, string | number>)` call
//     fails to compile too, not just an assignment.
//
// The caller fixes it by building the interpolation object itself and
// spreading `phrase.params` into it — see `AminetStudio.tsx`'s `updateText`,
// `FileManager.tsx`'s `copyResultText`, and `WhdloadInstall.tsx`'s `Report`
// for the pattern. Constructing a `PartialPhrase` (inside the three
// functions that return one) needs a type assertion, since the real
// returned object never has real values for `K` — that is expected and
// confined to those three definitions, not to their callers.
declare const MISSING_PARAM: unique symbol;
type MissingParam = { readonly [MISSING_PARAM]: true };

export type PartialPhrase<K extends string> = {
  key: string;
  params: { [P in K]: MissingParam };
};
