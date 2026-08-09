// Global settings store (Zustand). Mirrors the persisted JSON store, but
// holds the live state the UI reacts to.

import { create } from "zustand";
import {
  type AppSettings,
  DEFAULT_SETTINGS,
  getSettings,
  saveSettings,
} from "@/lib/settings";
import { changeLanguage, type Language } from "@/i18n";

interface SettingsState {
  settings: AppSettings;
  loaded: boolean;
  load: () => Promise<void>;
  update: (patch: Partial<AppSettings>) => Promise<void>;
}

export const useSettingsStore = create<SettingsState>((set, get) => ({
  settings: DEFAULT_SETTINGS,
  loaded: false,

  load: async () => {
    const settings = await getSettings();
    set({ settings, loaded: true });
  },

  update: async (patch) => {
    const next = { ...get().settings, ...patch };
    set({ settings: next });
    await saveSettings(patch);
    if (patch.language) {
      await changeLanguage(patch.language as Language);
    }
  },
}));
