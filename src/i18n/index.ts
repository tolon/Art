// i18n setup.
//
// English and Turkish ship today; the architecture is ready for additional
// locales — drop a new JSON file and register it below. No component edits.
//
// Initialised synchronously at module load so `useTranslation` always has a
// ready instance — no async race with the first render.

import i18n from "i18next";
import { initReactI18next } from "react-i18next";
import en from "./en.json";
import tr from "./tr.json";

export const SUPPORTED_LANGUAGES = ["en", "tr"] as const;
export type Language = (typeof SUPPORTED_LANGUAGES)[number];

/** Shown in the switcher. A language is named in itself, never translated. */
export const LANGUAGE_NAMES: Record<Language, string> = {
  en: "English",
  tr: "Türkçe",
};

// Synchronous init — runs once when the module is imported.
i18n.use(initReactI18next).init({
  lng: "en",
  fallbackLng: "en",
  resources: {
    en: { translation: en },
    tr: { translation: tr },
  },
  interpolation: { escapeValue: false },
  // Don't suspend — we already have resources inlined.
  react: { useSuspense: false },
});

/** Re-export so App can change language after settings load. */
export async function changeLanguage(lng: Language): Promise<void> {
  await i18n.changeLanguage(lng);
}

export default i18n;
