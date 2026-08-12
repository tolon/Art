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

/**
 * Keys the user has changed in this run.
 *
 * `App` starts `load()` and deliberately does not await it, so a fast user can
 * change something while the read is still in the air. Without this the read
 * would land afterwards and put the old value back — a setting changing
 * without the user changing it, which is the one thing that must not happen.
 * The same class of bug as ART-089, from the other direction: there the cold
 * start overwrote the file, here the file would overwrite the user.
 *
 * Module-level rather than in the store because it is bookkeeping about the
 * store, not state anything renders.
 */
const changedByUser = new Set<keyof AppSettings>();

export const useSettingsStore = create<SettingsState>((set, get) => ({
  settings: DEFAULT_SETTINGS,
  loaded: false,

  load: async () => {
    const settings = await getSettings();
    const current = get().settings;
    const preserved: Partial<AppSettings> = {};
    for (const key of changedByUser) {
      (preserved as Record<string, unknown>)[key] = current[key];
    }
    set({ settings: { ...settings, ...preserved }, loaded: true });
  },

  update: async (patch) => {
    for (const key of Object.keys(patch) as (keyof AppSettings)[]) {
      changedByUser.add(key);
    }
    const next = { ...get().settings, ...patch };
    set({ settings: next });
    await saveSettings(patch);
    if (patch.language) {
      await changeLanguage(patch.language as Language);
    }
  },
}));
