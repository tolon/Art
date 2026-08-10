// A translatable sentence, without a translator.
//
// `src/lib/*` is pure TypeScript — no i18next singleton, so its functions stay
// unit-testable without booting a translator and stay usable from a future CLI
// shell. A `Phrase` is the data a sentence needs: which catalogue key, and the
// values to interpolate into it. The component that renders it calls
// `t(phrase.key, phrase.params)` — see src/i18n/en.json for the catalogue.
export type Phrase = { key: string; params?: Record<string, string | number> };
