// i18n setup.
//
// v1.x ships English only, but the architecture is ready for additional
// locales — drop a new JSON file and register it below. No component edits.
//
// Initialised synchronously at module load so `useTranslation` always has a
// ready instance — no async race with the first render.

import i18n from "i18next";
import { initReactI18next } from "react-i18next";
import en from "./en.json";

export const SUPPORTED_LANGUAGES = ["en"] as const;
export type Language = (typeof SUPPORTED_LANGUAGES)[number];

// Synchronous init — runs once when the module is imported.
i18n.use(initReactI18next).init({
  lng: "en",
  fallbackLng: "en",
  resources: {
    en: { translation: en },
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
