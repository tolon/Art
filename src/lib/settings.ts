// JSON key/value settings via tauri-plugin-store.
// Used for preferences the UI needs before the SQL DB is ready, and for
// anything that doesn't need relational queries.

import { load } from "@tauri-apps/plugin-store";

const STORE_FILE = "settings.json";

let _store: Awaited<ReturnType<typeof load>> | null = null;
let _storeFailed = false;

async function store() {
  if (_storeFailed) throw new Error("store unavailable");
  if (_store) return _store;
  _store = await load(STORE_FILE, { autoSave: 100 });
  return _store;
}

export type Theme = "dark" | "light";
export type UxMode = "beginner" | "power";

/** Shape of `MirrorInfo` in `lib/sources.ts`, restated so this module stays
 *  free of feature types. */
export interface StoredMirror {
  name: string;
  base_url: string;
}

export interface AppSettings {
  theme: Theme;
  uxMode: UxMode;
  language: string;
  lastCollectionDir: string | null;
  winuaePath: string | null;
  /** Download folder for Aminet packages. The Rust side holds it only for the
   *  lifetime of the process, so it is remembered here (§41.5.6). */
  aminetRoot: string | null;
  /** A custom mirror order. Null means "use the ones ART ships with", which is
   *  not the same as an empty list — an empty list would disable syncing. */
  aminetMirrors: StoredMirror[] | null;
}

export const DEFAULT_SETTINGS: AppSettings = {
  theme: "dark",
  uxMode: "beginner",
  language: "en",
  lastCollectionDir: null,
  winuaePath: null,
  aminetRoot: null,
  aminetMirrors: null,
};

export async function getSettings(): Promise<AppSettings> {
  try {
    const s = await store();
    return {
      theme: (await s.get<Theme>("theme")) ?? DEFAULT_SETTINGS.theme,
      uxMode: (await s.get<UxMode>("uxMode")) ?? DEFAULT_SETTINGS.uxMode,
      language: (await s.get<string>("language")) ?? DEFAULT_SETTINGS.language,
      lastCollectionDir:
        (await s.get<string>("lastCollectionDir")) ?? DEFAULT_SETTINGS.lastCollectionDir,
      winuaePath: (await s.get<string>("winuaePath")) ?? DEFAULT_SETTINGS.winuaePath,
      aminetRoot: (await s.get<string>("aminetRoot")) ?? DEFAULT_SETTINGS.aminetRoot,
      aminetMirrors:
        (await s.get<StoredMirror[]>("aminetMirrors")) ?? DEFAULT_SETTINGS.aminetMirrors,
    };
  } catch (e) {
    console.warn("[ART] settings load failed, using defaults:", e);
    _storeFailed = true;
    return { ...DEFAULT_SETTINGS };
  }
}

export async function saveSettings(patch: Partial<AppSettings>): Promise<void> {
  try {
    const s = await store();
    for (const [key, value] of Object.entries(patch)) {
      if (value === undefined) continue;
      await s.set(key, value);
    }
    await s.save();
  } catch (e) {
    console.warn("[ART] settings save failed:", e);
  }
}
