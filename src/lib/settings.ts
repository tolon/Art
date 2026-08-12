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

/** Shape of `OverwritePolicy` in `lib/sources.ts`, restated so this module
 *  stays free of feature types — the same reason `StoredMirror` below is. */
export type StoredOverwritePolicy = "skip" | "overwrite" | "rename";

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
  /** Whether the app sidebar is collapsed (Ctrl+B). A preference rather than
   *  view state: someone who works with the panes full-width expects them
   *  full-width the next time they open ART, not one keystroke away from it. */
  sidebarCollapsed: boolean;
  /**
   * Whether the Files screen shows the row of source buttons above each pane.
   *
   * Off by default, because the user's own `[Layout]` is `ButtonBar=0,
   * DriveBar1=0, DriveCombo=1` — he has run Total Commander with no button
   * bar for twenty years, and the source combo in the pane header reaches
   * every one of those buttons anyway (brief §1.3). Kept as a setting rather
   * than deleted: the buttons are the only mouse-discoverable way to see the
   * six kinds of thing a pane can open.
   */
  showSourceButtons: boolean;
  /**
   * What a copy does about a name the destination already holds.
   *
   * A *setting* since phase 2b task 3, not a control on the Files screen. The
   * commander used to carry a permanent footer asking the question whether or
   * not it was about to come up; Total Commander asks it in the copy dialog,
   * at the moment it matters, and `CopyPlanDialog` does exactly that when the
   * pre-flight finds a collision. This is the answer it starts from — and the
   * one the copies that never show a dialog (a single file, a copy out) use.
   *
   * `"skip"` by default: of the three, it is the only one that cannot lose
   * data the user already had.
   */
  overwritePolicy: StoredOverwritePolicy;
  /**
   * The Files screen's tabs, per pane, and which pane had the keyboard
   * (`Savepath=1`, `Savepanels=1` — brief §3.3).
   *
   * **Deliberately `unknown` here.** The shape belongs to `@/lib/paneTabs` and
   * `@/lib/paneHistory`; restating it would either drag feature types into
   * this module — which it is the whole point of `StoredMirror` and
   * `StoredOverwritePolicy` above to avoid — or fork it, so that a change in
   * one would silently not reach the other.
   *
   * What that costs is a type guard, and the guard is needed anyway: this is a
   * JSON file a user can edit and an older ART may have written, so it is
   * validated by `isUsableTabSet` on the way in. A commander that opens to a
   * blank screen because its settings file was half-written is worse than one
   * that forgot your tabs.
   */
  filesSession: unknown;
  /**
   * Per-filetype colour rules for the Files screen (brief Part 2).
   *
   * `unknown` for the same reason `filesSession` is: the shape belongs to
   * `@/lib/colourRules`, which validates it with `isUsableRuleList` on the way
   * in. `null` means "use the ones ART ships with" — which is *not* the same
   * as an empty list, and the difference matters: an empty list is a user who
   * has deliberately turned every rule off.
   */
  fileColourRules: unknown;
  /**
   * Right-click marks a row instead of doing nothing (`UseRightButton=1`).
   *
   * Off by default, and that is the honest default rather than a timid one:
   * right-click is the gesture every other Windows application uses for a
   * context menu, and ART will grow one. A user who has run Norton Commander's
   * right-button marking for thirty years turns it on and never thinks about
   * it again.
   */
  rightButtonSelects: boolean;
  /**
   * Where each pane of the Files screen starts, when there is no session to
   * come back to.
   *
   * ART is not a disk manager; it is an Amiga toolkit, and a user's Amiga
   * files live in one or two folders they already know. Opening on the first
   * enumerated drive is the habit of a general file manager and the wrong
   * default here — it costs two navigations at every cold start, every time,
   * for a program that only ever wants the same two places.
   *
   * `null` means "no preference", which falls back to the first enumerated
   * mount exactly as before.
   */
  defaultLeftPath: string | null;
  defaultRightPath: string | null;
  /**
   * Whether the default folders above win over the last session.
   *
   * **On by default**, and that is the user's own call: ART is used for one
   * purpose, so the two folders it should open in are the same two every time
   * — "hep oradan açılsın". Turning it off gets Total Commander's
   * `Savepath=1` behaviour instead, reopening wherever the panes were left.
   *
   * It costs nothing until the folders are actually set: with neither one
   * configured there is nothing to prefer, and the session is restored exactly
   * as it would have been.
   */
  alwaysUseDefaultFolders: boolean;
}

export const DEFAULT_SETTINGS: AppSettings = {
  theme: "dark",
  uxMode: "beginner",
  language: "en",
  lastCollectionDir: null,
  winuaePath: null,
  aminetRoot: null,
  aminetMirrors: null,
  sidebarCollapsed: false,
  showSourceButtons: false,
  overwritePolicy: "skip",
  filesSession: null,
  fileColourRules: null,
  rightButtonSelects: false,
  defaultLeftPath: null,
  defaultRightPath: null,
  alwaysUseDefaultFolders: true,
};

export async function getSettings(): Promise<AppSettings> {
  try {
    const s = await store();
    return {
      theme: (await s.get<Theme>("theme")) ?? DEFAULT_SETTINGS.theme,
      uxMode: (await s.get<UxMode>("uxMode")) ?? DEFAULT_SETTINGS.uxMode,
      language: (await s.get<string>("language")) ?? DEFAULT_SETTINGS.language,
      sidebarCollapsed:
        (await s.get<boolean>("sidebarCollapsed")) ?? DEFAULT_SETTINGS.sidebarCollapsed,
      showSourceButtons:
        (await s.get<boolean>("showSourceButtons")) ?? DEFAULT_SETTINGS.showSourceButtons,
      overwritePolicy:
        (await s.get<StoredOverwritePolicy>("overwritePolicy")) ??
        DEFAULT_SETTINGS.overwritePolicy,
      filesSession: (await s.get<unknown>("filesSession")) ?? DEFAULT_SETTINGS.filesSession,
      fileColourRules:
        (await s.get<unknown>("fileColourRules")) ?? DEFAULT_SETTINGS.fileColourRules,
      rightButtonSelects:
        (await s.get<boolean>("rightButtonSelects")) ?? DEFAULT_SETTINGS.rightButtonSelects,
      defaultLeftPath:
        (await s.get<string>("defaultLeftPath")) ?? DEFAULT_SETTINGS.defaultLeftPath,
      defaultRightPath:
        (await s.get<string>("defaultRightPath")) ?? DEFAULT_SETTINGS.defaultRightPath,
      alwaysUseDefaultFolders:
        (await s.get<boolean>("alwaysUseDefaultFolders")) ??
        DEFAULT_SETTINGS.alwaysUseDefaultFolders,
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
