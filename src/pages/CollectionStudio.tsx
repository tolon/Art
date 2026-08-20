import { useEffect, useMemo, useRef, useState } from "react";
import { useLocation, useNavigate } from "react-router-dom";
import { confirm, open } from "@tauri-apps/plugin-dialog";
import { useTranslation } from "react-i18next";

import {
  catalogueAddRoot,
  catalogueLoad,
  catalogueRefresh,
  catalogueRemoveRoot,
  mediaKind,
  nameSuggestions,
  onCatalogueRefreshed,
  renameTitleFile,
  catalogueSetOverride,
  NO_OVERRIDE,
  type GameRecord,
  type NameSuggestion,
  type ChipsetRequirement,
  type EntryView,
  type Provenance,
  type RefreshMode,
  type RootView,
} from "@/lib/gameindex";
import {
  artworkAdoptLocal,
  artworkDefaults,
  artworkDir,
  artworkEnrich,
  artworkKnown,
  artworkRebindManual,
  onArtworkLocalResult,
  onArtworkResult,
  outcomePhrase,
  rebindProblemPhrase,
  sourcePhrase,
  type ConfiguredSource,
  type LocalOutcome,
  type RebindOutcome,
  type SourceOutcome,
} from "@/lib/artwork";
import { subscribeSafely } from "@/lib/jobs";
import { isOneOf } from "@/lib/remembered";
import { useRemembered } from "@/lib/useRemembered";
import { useSettingsStore } from "@/stores/settingsStore";
import { Guessed } from "@/components/collection/Guessed";
import { TitleDetail } from "@/components/collection/TitleDetail";

type ViewMode = "grid" | "table";
type MediaFilter = "all" | "floppies" | "hardfile" | "whdload";
type ChipsetFilter = "all" | ChipsetRequirement;

const isViewMode = isOneOf<ViewMode>("grid", "table");
const isFormatFilter = isOneOf<MediaFilter>(
  "all",
  "floppies",
  "hardfile",
  "whdload"
);
const isChipsetFilter = isOneOf<ChipsetFilter>("all", "ocsecs", "aga");

/**
 * What the list needs, flattened out of a `GameRecord` once.
 *
 * The record keeps every value beside the source that gave it; the screen
 * needs both, and doing the unwrapping in one place keeps `isStated` out of
 * a dozen render sites.
 */
interface Shown {
  id: string;
  path: string;
  /** Which catalogued folder this title came from. */
  root: string;
  /** Asked of the disk when the catalogue was loaded, never stored. */
  available: boolean;
  title: string;
  titleFrom: Provenance;
  publisher: string | null;
  publisherFrom: Provenance | null;
  year: number | null;
  yearFrom: Provenance | null;
  chipset: ChipsetRequirement | null;
  chipsetFrom: Provenance | null;
  media: "floppies" | "hardfile" | "whdload";
  diskCount: number;
  kickstart: string | null;
  /** The screenshot's entry name inside the package, when it carries one. */
  preview: string | null;
  /**
   * The record as the catalogue holds it. Kept alongside the flattened
   * fields above (rather than reached for through them) so a selected row
   * can be handed to `TitleDetail` as the `CatalogueEntry` it expects
   * without a second trip to the backend.
   */
  record: GameRecord;
}

function flatten(root: string, entry: EntryView): Shown {
  const r = entry.record;
  return {
    id: r.id,
    path: entry.path,
    root,
    available: entry.available,
    title: r.title.value,
    titleFrom: r.title.from,
    publisher: r.publisher?.value ?? null,
    publisherFrom: r.publisher?.from ?? null,
    year: r.year?.value ?? null,
    yearFrom: r.year?.from ?? null,
    chipset: r.chipset?.value ?? null,
    chipsetFrom: r.chipset?.from ?? null,
    media: mediaKind(r.media),
    diskCount: r.media.kind === "floppies" ? r.media.ordered.length : 1,
    kickstart: r.kickstart?.value.image ?? null,
    preview: r.preview,
    record: r,
  };
}

/**
 * The one-button fixes for a name ART could only take from a filename.
 *
 * Two buttons and not one, because they carry different risk and must not be
 * confused for each other. **Fix title** changes ART's own listing through the
 * override layer — reversible, and the file is untouched. **Rename file**
 * changes the user's data and asks first.
 *
 * A row with nothing worth proposing renders nothing at all: a button that
 * offers to fix a name already right teaches people to click without reading.
 */
function NameFixes({
  current,
  suggestion,
  undo,
  onTitle,
  onRename,
  onUndo,
}: {
  current: string;
  suggestion: NameSuggestion | undefined;
  undo: string | undefined;
  onTitle: (proposed: string) => void;
  onRename: (proposed: string) => void;
  onUndo: () => void;
}) {
  const { t } = useTranslation();
  const [typing, setTyping] = useState(false);
  const [draft, setDraft] = useState(current);

  // The hand-edit is always shown, because the rules deliberately give up on
  // the cases they cannot settle. `brian the lion 2` has no disk one anywhere,
  // so nothing can tell a missing first disk from a sequel — and that is
  // exactly where a person types the answer instead of a rule guessing at it.
  if (typing) {
    return (
      <div style={{ display: "flex", gap: 6, alignItems: "center", marginTop: 6 }}>
        <input
          type="text"
          value={draft}
          autoFocus
          onChange={(e) => setDraft(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter" && draft.trim()) {
              onTitle(draft.trim());
              setTyping(false);
            }
            if (e.key === "Escape") setTyping(false);
          }}
          style={{
            flex: 1,
            minWidth: 0,
            padding: "2px 6px",
            fontSize: 11,
            background: "var(--bg)",
            color: "var(--text)",
            border: "1px solid var(--border)",
            borderRadius: 3,
          }}
        />
        <button
          className="btn btn-sm"
          disabled={!draft.trim()}
          onClick={() => {
            onTitle(draft.trim());
            setTyping(false);
          }}
          style={{ padding: "2px 8px", fontSize: 10 }}
        >
          {t("cleanup.save")}
        </button>
        <button
          className="btn btn-sm"
          onClick={() => setTyping(false)}
          style={{ padding: "2px 8px", fontSize: 10 }}
        >
          {t("cleanup.cancel")}
        </button>
      </div>
    );
  }

  return (
    <div style={{ display: "flex", gap: 6, flexWrap: "wrap", marginTop: 6 }}>
      {/* Deliberately a size up from the two beside it. This is the one that
          is always available and the one a person reaches for when the rules
          gave up — the suggestions are the shortcut, not the main road. */}
      <button
        className="btn btn-sm"
        title={t("cleanup.editHint")}
        onClick={() => {
          setDraft(current);
          setTyping(true);
        }}
        style={{ padding: "3px 9px", fontSize: 11.5, fontWeight: 500 }}
      >
        ✏ {t("cleanup.edit")}
      </button>
      {suggestion?.title && (
        <button
          className="btn btn-sm"
          title={t("cleanup.fixTitleHint", { proposed: suggestion.title })}
          onClick={() => onTitle(suggestion.title as string)}
          style={{ padding: "2px 8px", fontSize: 10 }}
        >
          ✎ {t("cleanup.fixTitle")}
        </button>
      )}
      {suggestion?.fileName && (
        <button
          className="btn btn-sm"
          title={t("cleanup.renameFileHint", { proposed: suggestion.fileName })}
          onClick={() => onRename(suggestion.fileName as string)}
          style={{ padding: "2px 8px", fontSize: 10 }}
        >
          🗂 {t("cleanup.renameFile")}
        </button>
      )}
      {undo && (
        <button
          className="btn btn-sm"
          onClick={onUndo}
          style={{ padding: "2px 8px", fontSize: 10 }}
        >
          ↺ {t("cleanup.undo")}
        </button>
      )}
    </div>
  );
}

export function CollectionStudio() {
  const { t } = useTranslation();
  const navigate = useNavigate();
  const location = useLocation();

  const [scanDir, setScanDir] = useState<string | null>(null);
  const [items, setItems] = useState<Shown[]>([]);
  const [searchTerm, setSearchTerm] = useState<string>("");

  // Which title's detail panel is open. Not remembered: this is what the
  // screen is merely doing right now — the row it has open — not a choice
  // the user would be annoyed to make again tomorrow (`@/lib/useRemembered`'s
  // own line between the two).
  const [selectedId, setSelectedId] = useState<string | null>(null);

  // Bumped every time a Play button is pressed, and handed to `TitleDetail`
  // so it fetches that title's launch plan the moment the panel opens —
  // one click reaches a confirmation, not only an open panel. Not a boolean:
  // pressing Play again for the same title (say, after changing the ROM
  // folder) must re-trigger the fetch even though `selectedId` did not
  // change.
  const [playRequest, setPlayRequest] = useState(0);

  // How the library is being looked at is the user's choice, not the screen's
  // (see `@/lib/useRemembered`). A grid-and-AGA view set on Monday is still
  // grid-and-AGA on Tuesday.
  const [viewMode, setViewMode] = useRemembered<ViewMode>(
    "collection.viewMode",
    isViewMode,
    "grid"
  );
  const [formatFilter, setFormatFilter] = useRemembered<MediaFilter>(
    "collection.formatFilter",
    isFormatFilter,
    "all"
  );
  const [chipsetFilter, setChipsetFilter] = useRemembered<ChipsetFilter>(
    "collection.chipsetFilter",
    isChipsetFilter,
    "all"
  );

  // The folder shares `lastCollectionDir` with Settings rather than living in
  // the bag: it is a path the user can also set there, and two places holding
  // the same answer separately is how they come to disagree.
  const rememberedDir = useSettingsStore((s) => s.settings.lastCollectionDir);
  const updateSettings = useSettingsStore((s) => s.update);

  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [statusMsg, setStatusMsg] = useState<string | null>(null);

  const [roots, setRoots] = useState<RootView[]>([]);

  // Artwork, kept beside the items rather than folded into them: a picture
  // arrives long after the row it belongs to, and merging would mean rebuilding
  // every row each time one lands. Keyed by the entry id the row already has.
  const [art, setArt] = useState<Map<string, string>>(new Map());
  // Ids whose cached picture came from the user attaching one by hand
  // (`ArtRef.source === "manual"`), rather than a fetched or `.rp9` one. The
  // override layer that records the choice is keyed by record id and carries
  // no title, so this is what the screen sends as `artworkEnrich`'s pinned
  // list, and what tells `TitleDetail` whether to offer "Remove".
  const [manualArt, setManualArt] = useState<Set<string>>(new Set());
  const [artOutcome, setArtOutcome] = useState<SourceOutcome[] | null>(null);
  // Separate from `busy`, which a catalogue refresh also sets. Only an
  // artwork run should make the screen re-read the artwork cache.
  const [artBusy, setArtBusy] = useState(false);
  const [localOutcome, setLocalOutcome] = useState<LocalOutcome | null>(null);
  /**
   * What the last re-materialise pass managed (ART-143), or `null` when there
   * was nothing to say — which is every ordinary run.
   *
   * Shown rather than silent because *something changed on the user's disk
   * without them asking on this run*. Restoring is honouring a choice they
   * already made, so it needs no confirmation; but pictures reappearing with
   * no explanation, and pictures that could **not** reappear disappearing with
   * none, are both things a person is owed a sentence about.
   */
  const [rebindOutcome, setRebindOutcome] = useState<RebindOutcome | null>(null);

  const storedSources = useSettingsStore((s) => s.settings.artworkSources);

  // What ART would propose for a name it could only take from a filename,
  // keyed by entry id. Asked once per load and never applied on its own.
  const [suggestions, setSuggestions] = useState<Map<string, NameSuggestion>>(
    new Map()
  );
  // The title a row had before its last one-button fix, so the fix can be
  // undone from the same place it was applied.
  const [undoable, setUndoable] = useState<Map<string, string>>(new Map());

  /**
   * Ask what ART would propose for these rows' names.
   *
   * Read-only. Nothing is written, nothing is renamed, and a row with nothing
   * worth proposing gets no entry — so the screen shows a button only where
   * there is something to do.
   */
  async function loadSuggestions(rows: Shown[]) {
    if (rows.length === 0) return;
    try {
      const answers = await nameSuggestions(
        rows.map((row) => row.path),
        rows.map((row) => row.title)
      );
      const next = new Map<string, NameSuggestion>();
      answers.forEach((answer, index) => {
        if (answer.title || answer.fileName) next.set(rows[index].id, answer);
      });
      setSuggestions(next);
    } catch {
      // A screen that cannot suggest is a screen without buttons, not a
      // screen that fails to open.
      setSuggestions(new Map());
    }
  }

  /** Apply a proposed title as the user's own correction (top provenance). */
  async function applyTitle(item: Shown, proposed: string) {
    try {
      await catalogueSetOverride(item.id, { ...NO_OVERRIDE, title: proposed });
      setUndoable((prev) => new Map(prev).set(item.id, item.title));
      setStatusMsg(t("cleanup.titleFixed", { name: proposed }));
      await reload();
    } catch (e) {
      setError(String(e));
    }
  }

  /** Take the correction back off, leaving no trace of it. */
  async function undoTitle(item: Shown) {
    try {
      await catalogueSetOverride(item.id, NO_OVERRIDE);
      setUndoable((prev) => {
        const next = new Map(prev);
        next.delete(item.id);
        return next;
      });
      setStatusMsg(t("cleanup.undone"));
      await reload();
    } catch (e) {
      setError(String(e));
    }
  }

  /**
   * Rename the file on disk, once the user has said so.
   *
   * The confirmation shows both names in full: this is the half of the tool
   * that changes their data, and a dialog that only says "are you sure?" is a
   * dialog people learn to click through.
   */
  async function applyRename(item: Shown, proposed: string) {
    const agreed = await confirm(
      t("cleanup.renameConfirm", { from: item.path, to: proposed }),
      { title: t("cleanup.renameFile"), kind: "warning" }
    );
    if (!agreed) return;
    try {
      const renamed = await renameTitleFile(item.path, proposed);
      setStatusMsg(t("cleanup.renamed", { name: renamed }));
      await reload();
    } catch (e) {
      setError(String(e));
    }
  }

  /**
   * Ask which titles already have a picture, and where it is.
   *
   * **Reads the cache; fetches nothing.** Rust does the normalising because the
   * two matching rules live there and a copy here would drift from them.
   */
  /**
   * Put back any hand-attached picture whose cache entry has gone, then load
   * the artwork (ART-143).
   *
   * **Why this runs on load rather than behind a button.** The artwork cache
   * is a *sibling* of the catalogue directory precisely so a user can delete
   * 1.6 GB of pictures and keep the index — and until now, doing so silently
   * lost every picture they had attached by hand, even though the file each
   * one came from is still named in the override beside it. There is nothing
   * for the user to decide here: the choice is already theirs and already
   * recorded; ART simply stopped honouring it. A pass over an intact cache is
   * one metadata call per hand-attached title and no writes at all, so this is
   * not a fetch and is nothing like {@link handleEnrich}, which stays the
   * user's own action (ART-132).
   *
   * Failures are swallowed on purpose: a screen full of rows is worth more
   * than a screen refusing to open because a picture could not be copied.
   */
  async function restoreOwnPictures(rows: Shown[]) {
    if (rows.length === 0) return;
    try {
      const outcome = await artworkRebindManual(
        rows.map((row) => ({ id: row.id, title: row.title }))
      );
      // Silent when there was nothing to do, which is every ordinary run.
      if (outcome.restored.length > 0 || outcome.missed.length > 0) {
        setRebindOutcome(outcome);
      }
    } catch {
      // See above.
    }
  }

  async function loadArtwork(rows: Shown[]) {
    if (rows.length === 0) return;
    await restoreOwnPictures(rows);
    try {
      const [dir, known] = await Promise.all([
        artworkDir(),
        artworkKnown(rows.map((row) => row.title)),
      ]);
      const { convertFileSrc } = await import("@tauri-apps/api/core");
      const next = new Map<string, string>();
      const manual = new Set<string>();
      known.forEach((ref, index) => {
        if (!ref) return;
        next.set(rows[index].id, convertFileSrc(`${dir}/${ref.file}`));
        if (ref.source === "manual") manual.add(rows[index].id);
      });
      setArt(next);
      setManualArt(manual);
    } catch {
      // A cache that cannot be read is a screen without pictures, not a screen
      // that fails to open. The rows stand on their own.
      setArt(new Map());
      setManualArt(new Set());
    }
  }

  /**
   * Fetch artwork for everything on screen.
   *
   * The user's action, never a side effect of opening the screen — the same
   * rule that took the automatic scan out (ART-132).
   */
  async function handleEnrich(rows: Shown[]) {
    const shipped = await artworkDefaults().catch(() => [] as ConfiguredSource[]);
    const saved = Array.isArray(storedSources)
      ? (storedSources as ConfiguredSource[])
      : null;
    const sources = saved ?? shipped;

    if (rows.length === 0) {
      setError(t("artwork.enrich.noTitles"));
      return;
    }
    if (!sources.some((source) => source.enabled)) {
      setError(t("artwork.enrich.noSources"));
      return;
    }

    setBusy(true);
    setArtBusy(true);
    setError(null);
    setArtOutcome(null);
    setStatusMsg(t("artwork.enrich.running"));
    try {
      // The rows whose cached picture the user attached by hand — no source
      // may overwrite that choice.
      const pinned = rows
        .filter((row) => manualArt.has(row.id))
        .map((row) => row.title);
      await artworkEnrich(
        rows.map((row) => row.title),
        sources,
        pinned
      );
    } catch (e) {
      setError(String(e));
      setBusy(false);
      setArtBusy(false);
    }
  }

  /**
   * Pull the pictures already embedded in the rows shown right now.
   *
   * Touches no network: every `.rp9` on screen with a `preview` entry is
   * opened and its screenshot copied into the artwork cache. The user's
   * action, never a side effect of opening the screen (ART-132's rule).
   */
  async function handleLocalPictures() {
    const previews = filteredItems
      .filter((item) => item.preview)
      .map((item) => ({
        title: item.title,
        package: item.path,
        entry: item.preview as string,
      }));
    if (previews.length === 0) {
      setError(t("artwork.local.none"));
      return;
    }

    setBusy(true);
    setArtBusy(true);
    setError(null);
    setLocalOutcome(null);
    setStatusMsg(t("artwork.local.running", { count: previews.length }));
    try {
      await artworkAdoptLocal(previews);
    } catch (e) {
      setError(String(e));
      setBusy(false);
      setArtBusy(false);
    }
  }

  /**
   * Load the saved catalogue. **Starts nothing.**
   *
   * Opening this screen used to scan; that is what ART-132's double scan came
   * out of, and the user's rule is that ART begins no work by itself. The
   * catalogue is on disk, so it appears at once and stays until asked to
   * change.
   */
  async function reload() {
    try {
      const loaded = await catalogueLoad();
      setRoots(loaded);
      // A title in two folders is one entry on screen, keeping the first. The
      // merge happens here rather than in storage, because storage that merged
      // could not remove one folder without rewriting the other.
      //
      // **An available copy always wins.** Two entries share an id when they
      // share bytes, and keeping whichever came first by path order showed the
      // copy on an unplugged drive while hiding the one right there — which is
      // what happened the first time a file was renamed while the screen was
      // open.
      const byId = new Map<string, Shown>();
      for (const root of loaded) {
        for (const entry of root.entries) {
          const shown = flatten(root.root, entry);
          const existing = byId.get(shown.id);
          if (!existing || (!existing.available && shown.available)) {
            byId.set(shown.id, shown);
          }
        }
      }
      const shown = [...byId.values()];
      setItems(shown);
      void loadArtwork(shown);
      void loadSuggestions(shown);
      setScanDir(loaded.length === 1 ? loaded[0].root : null);
      setError(null);
    } catch (e) {
      setError(String(e));
    }
  }

  // `subscribeSafely` (ART-165): all three listeners here already guarded
  // against a leak on early unmount by hand (`if (cancelled) stop() else
  // unlisten = stop`), but none of the three async IIFEs had a `try`/`catch`
  // — a rejected `listen()` (no IPC bridge to reach, e.g. under test) became
  // an unhandled rejection from a `void`-discarded promise, once per
  // listener. The helper folds the same guard into one call per listener and
  // catches the rejection too.
  useEffect(() => {
    void reload();

    const unsubscribeCatalogue = subscribeSafely(() =>
      onCatalogueRefreshed(() => {
        setBusy(false);
        void reload();
      })
    );
    const unsubscribeArt = subscribeSafely(() =>
      onArtworkResult((result) => {
        setBusy(false);
        setArtBusy(false);
        setStatusMsg(null);
        setArtOutcome(result.perSource);
        // Re-read the cache rather than patching from the event: the run also
        // records what it could not find, and the screen should reflect the
        // cache it will read on the next open.
        setItems((rows) => {
          void loadArtwork(rows);
          return rows;
        });
      })
    );
    const unsubscribeLocal = subscribeSafely(() =>
      onArtworkLocalResult((result) => {
        setBusy(false);
        setArtBusy(false);
        setStatusMsg(null);
        setLocalOutcome(result.outcome);
        // Same reasoning as the enrichment listener above: re-read the cache
        // rather than patch from the event.
        setItems((rows) => {
          void loadArtwork(rows);
          return rows;
        });
      })
    );

    return () => {
      unsubscribeCatalogue();
      unsubscribeArt();
      unsubscribeLocal();
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  /**
   * While a fetch is running, show pictures as they land.
   *
   * The engine writes its index every three seconds, so the screen reads it on
   * the same cadence. Waiting for the run to finish would mean staring at an
   * unchanged list for the better part of an hour — the user's objection, and
   * a fair one: a picture that has arrived should be visible.
   */
  useEffect(() => {
    if (!artBusy) return;
    const id = window.setInterval(() => {
      setItems((rows) => {
        void loadArtwork(rows);
        return rows;
      });
    }, 3000);
    return () => window.clearInterval(id);
    // `loadArtwork` is stable in behaviour and reads `items` through the
    // setter, so it is deliberately not a dependency — listing it would
    // restart the interval on every render.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [artBusy]);

  async function refresh(root: string, mode: RefreshMode) {
    setBusy(true);
    setError(null);
    setStatusMsg(t("collection.status.scanning"));
    try {
      await catalogueRefresh(root, mode);
    } catch (e) {
      setError(String(e));
      setBusy(false);
    }
  }

  async function handleAddFolder() {
    const sel = await open({
      directory: true,
      multiple: false,
      title: t("collection.dialog.selectFolderTitle"),
    });
    if (typeof sel !== "string") return;
    try {
      await catalogueAddRoot(sel);
      // Remembered so Settings and the Aminet hand-off keep working; the
      // catalogue itself is the list of folders now, and this is one of them.
      if (sel !== rememberedDir) await updateSettings({ lastCollectionDir: sel });
      await reload();
    } catch (e) {
      setError(String(e));
    }
  }

  async function handleRemoveFolder(root: string) {
    // The plugin's `confirm`, not `window.confirm` — ART-133: wry disables
    // WebView2's own script dialogs, so the browser one returns without asking
    // and this folder was being removed with nothing shown at all.
    if (!(await confirm(t("gameindex.catalogue.removeConfirm", { root })))) return;
    try {
      await catalogueRemoveRoot(root);
      await reload();
    } catch (e) {
      setError(String(e));
    }
  }

  // A folder sent by another screen — Aminet does this after a download — is
  // added and refreshed. One explicit action rather than a silent scan, and the
  // ref keeps StrictMode's double mount from doing it twice.
  const arrivedWith = useRef<string | null>(null);
  useEffect(() => {
    const wanted = (location.state as { path?: string } | null)?.path;
    if (!wanted || arrivedWith.current === wanted) return;
    arrivedWith.current = wanted;
    void (async () => {
      await catalogueAddRoot(wanted);
      await reload();
      await refresh(wanted, "update");
    })();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [location.state]);

  // Real-time filtered items
  const filteredItems = useMemo(() => {
    return items.filter((item) => {
      // Search term
      if (searchTerm.trim()) {
        const q = searchTerm.toLowerCase();
        const matchTitle = item.title.toLowerCase().includes(q);
        const matchPub = item.publisher?.toLowerCase().includes(q) ?? false;
        const matchYear = item.year?.toString().includes(q) ?? false;
        if (!matchTitle && !matchPub && !matchYear) return false;
      }

      // Media filter
      if (formatFilter !== "all" && item.media !== formatFilter) {
        return false;
      }

      // Chipset filter. A record whose chipset nothing stated is `null`, and
      // it must not be swept into either bucket — that is the default this
      // whole index exists to stop presenting as a fact.
      if (chipsetFilter !== "all" && item.chipset !== chipsetFilter) {
        return false;
      }

      return true;
    });
  }, [items, searchTerm, formatFilter, chipsetFilter]);

  /**
   * The screen's one Play control (Collection · wave C, Task 11).
   *
   * This used to `navigate("/winuae", ...)` — a route that opens WinUAE
   * Studio empty-handed and launches nothing, so the button labelled "Play"
   * did not play anything. The real launch flow — a read-only plan, a
   * confirmation showing the machine, the ROM and the media, and only then
   * a Start button — lives in `TitleDetail`'s Play section, because that is
   * where there is room to show it and where the per-title choices already
   * live. This opens that panel and asks it to fetch the plan immediately,
   * so one click reaches the confirmation rather than a second empty panel
   * the user has to act in again.
   */
  function handlePlay(item: Shown) {
    setSelectedId(item.id);
    setPlayRequest((n) => n + 1);
  }

  // Looked up from every loaded item, not the filtered set: changing a
  // filter after opening a title must not close the panel out from under it.
  const selectedItem = selectedId
    ? (items.find((item) => item.id === selectedId) ?? null)
    : null;

  /** Clicking a card opens it; clicking the same card again closes it. */
  function handleSelect(item: Shown) {
    setSelectedId((current) => (current === item.id ? null : item.id));
  }

  return (
    <div>
      {/* Top Header */}
      <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center", flexWrap: "wrap", gap: 10 }}>
        <div>
          <h1 style={{ fontSize: 20, margin: 0 }}>{t("nav.collection")} — {t("collection.subtitle")}</h1>
          {scanDir && (
            <div className="muted" style={{ fontSize: 12, marginTop: 2, wordBreak: "break-all" }}>
              {t("collection.header.library", { dir: scanDir, count: items.length })}
            </div>
          )}
        </div>

        <div style={{ display: "flex", gap: 8, alignItems: "center" }}>
          {/* View Mode Toggle: Grid vs Table */}
          <div style={{ display: "flex", border: "1px solid var(--border)", borderRadius: "var(--radius-sm)", overflow: "hidden" }}>
            <button
              className={`btn btn-sm ${viewMode === "grid" ? "btn-primary" : ""}`}
              onClick={() => setViewMode("grid")}
              style={{ borderRadius: 0, padding: "4px 10px" }}
            >
              🔲 {t("collection.toolbar.gridView")}
            </button>
            <button
              className={`btn btn-sm ${viewMode === "table" ? "btn-primary" : ""}`}
              onClick={() => setViewMode("table")}
              style={{ borderRadius: 0, padding: "4px 10px" }}
            >
              📋 {t("collection.toolbar.tableView")}
            </button>
          </div>

          <button className="btn btn-sm btn-primary" onClick={handleAddFolder} disabled={busy}>
            📂 {t("gameindex.catalogue.addFolder")}
          </button>
        </div>
      </div>

      {/*
        The catalogued folders. Everything here is explicit: nothing scans on
        open, and Update/Rescan/Remove are the only things that change anything.
      */}
      <section className="card" style={{ margin: "14px 0", padding: "10px 14px" }}>
        <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center", marginBottom: roots.length ? 8 : 0 }}>
          <strong style={{ fontSize: 13 }}>{t("gameindex.catalogue.folders")}</strong>
          <span className="muted" style={{ fontSize: 12 }}>
            {t("gameindex.catalogue.titleCount", { count: items.length })}
          </span>
        </div>

        {roots.length === 0 && (
          <p className="muted" style={{ fontSize: 12, margin: 0 }}>
            {t("gameindex.catalogue.empty")}
          </p>
        )}

        {roots.map((root) => (
          <div
            key={root.root}
            style={{
              display: "flex",
              alignItems: "center",
              gap: 10,
              flexWrap: "wrap",
              padding: "6px 0",
              borderTop: "1px solid var(--border)",
            }}
          >
            <div style={{ flex: 1, minWidth: 220 }}>
              <div style={{ fontSize: 12, wordBreak: "break-all" }}>{root.root}</div>
              <div className="faint" style={{ fontSize: 11 }}>
                {root.scanned_at
                  ? t("gameindex.catalogue.scannedAt", {
                      when: new Date(Number(root.scanned_at) * 1000).toLocaleString(),
                    })
                  : t("gameindex.catalogue.never")}
                {" · "}
                {t("gameindex.catalogue.titleCount", { count: root.entries.length })}
              </div>
              {/*
                Said, not acted on. A reader fix means an update would produce
                better facts from the same files — but the user chose that ART
                starts no work by itself, so this is a sentence and not a scan.
              */}
              {root.stale && (
                <div className="badge badge-warn" style={{ fontSize: 10, marginTop: 3 }}>
                  {t("gameindex.catalogue.stale")}
                </div>
              )}
              {root.entries.length === 0 && !root.stale && (
                <div className="faint" style={{ fontSize: 11 }}>
                  {t("gameindex.catalogue.emptyRoot")}
                </div>
              )}
            </div>

            <button
              className="btn btn-sm btn-primary"
              disabled={busy}
              title={t("gameindex.catalogue.updateHint")}
              onClick={() => void refresh(root.root, "update")}
              style={{ padding: "3px 10px", fontSize: 11 }}
            >
              {t("gameindex.catalogue.update")}
            </button>
            <button
              className="btn btn-sm"
              disabled={busy}
              title={t("gameindex.catalogue.rescanHint")}
              onClick={() => void refresh(root.root, "rescan")}
              style={{ padding: "3px 10px", fontSize: 11 }}
            >
              {t("gameindex.catalogue.rescan")}
            </button>
            <button
              className="btn btn-sm"
              disabled={busy}
              onClick={() => void handleRemoveFolder(root.root)}
              style={{ padding: "3px 10px", fontSize: 11 }}
            >
              {t("gameindex.catalogue.removeFolder")}
            </button>
          </div>
        ))}

        {items.length > 0 && (
          <div style={{ marginTop: 12, display: "flex", alignItems: "center", gap: 10 }}>
            <button
              className="btn btn-sm"
              disabled={busy}
              // `filteredItems`, matching `handleLocalPictures` beside it —
              // both buttons act on what the screen is actually showing right
              // now, not the whole unfiltered library behind it.
              onClick={() => void handleEnrich(filteredItems)}
              style={{ padding: "3px 10px", fontSize: 11 }}
            >
              {t("artwork.enrich.action")}
            </button>
            <button
              className="btn btn-sm"
              onClick={() => void handleLocalPictures()}
              disabled={busy}
              style={{ padding: "3px 10px", fontSize: 11 }}
            >
              🖼 {t("artwork.local.action")}
            </button>
            {artOutcome?.map((outcome) => (
              <span key={outcome.id} className="faint" style={{ fontSize: 11 }}>
                {t(sourcePhrase(outcome.id).key, sourcePhrase(outcome.id).params)}:{" "}
                {t(outcomePhrase(outcome).key, outcomePhrase(outcome).params)}
              </span>
            ))}
            {localOutcome && (
              <span className="faint" style={{ fontSize: 11 }}>
                {t("artwork.local.done", { ...localOutcome })}
              </span>
            )}
          </div>
        )}
      </section>

      {/* ART-143. Pictures the user attached by hand, put back from the files
          they originally chose after the artwork cache went. Dismissible
          rather than transient: the list of what could *not* be restored names
          files the user may want to go and find. */}
      {rebindOutcome && (
        <div className="card" style={{ margin: "12px 0", padding: "8px 12px", fontSize: 12 }}>
          {rebindOutcome.restored.length > 0 && (
            <p style={{ margin: 0 }}>
              {t("collection.rebind.restored", { count: rebindOutcome.restored.length })}
            </p>
          )}
          {rebindOutcome.missed.length > 0 && (
            <>
              <p style={{ margin: "6px 0 4px" }}>{t("collection.rebind.missedHeading")}</p>
              <ul style={{ margin: 0, paddingLeft: 18 }}>
                {rebindOutcome.missed.map((miss) => (
                  <li key={miss.id} className="faint">
                    <strong>{miss.title}</strong> — {t(rebindProblemPhrase(miss.problem).key)} (
                    {miss.chosen})
                  </li>
                ))}
              </ul>
            </>
          )}
          <button
            className="btn btn-sm"
            onClick={() => setRebindOutcome(null)}
            style={{ marginTop: 6, padding: "2px 8px", fontSize: 10 }}
          >
            {t("collection.rebind.dismiss")}
          </button>
        </div>
      )}

      {error && <div className="badge badge-err" style={{ margin: "12px 0", padding: "6px 12px" }}>{error}</div>}
      {statusMsg && <div className="badge badge-ok" style={{ margin: "12px 0", padding: "6px 12px" }}>{statusMsg}</div>}
      {busy && <div className="muted" style={{ margin: "12px 0" }}>{t("collection.status.scanningLong")}</div>}

      {/* Filter Toolbar.

          Stuck to the top of the scroll area on purpose: this library is 1700
          rows, and a filter you have to scroll back up to reach is a filter you
          stop using. `.app-content` is the scrolling ancestor, `.card` is
          opaque (`--bg-panel`), and the z-index keeps rows from painting over
          it as they pass underneath. */}
      {items.length > 0 && (
        <section
          className="card"
          style={{
            margin: "14px 0",
            padding: "10px 14px",
            position: "sticky",
            top: 0,
            zIndex: 5,
          }}
        >
          <div style={{ display: "flex", gap: 12, flexWrap: "wrap", alignItems: "center", justifyContent: "space-between" }}>
            {/* Search Input */}
            <div style={{ flex: 1, minWidth: 220 }}>
              <input
                type="text"
                value={searchTerm}
                onChange={(e) => setSearchTerm(e.target.value)}
                placeholder={t("collection.filters.searchPlaceholder")}
                style={{
                  width: "100%",
                  padding: "6px 10px",
                  background: "var(--bg)",
                  color: "var(--text)",
                  border: "1px solid var(--border)",
                  borderRadius: 4,
                  fontSize: 13,
                }}
              />
            </div>

            {/* Media Format Filter Chips */}
            <div style={{ display: "flex", gap: 6, flexWrap: "wrap" }}>
              <span className="muted" style={{ fontSize: 12, alignSelf: "center" }}>{t("collection.filters.formatLabel")}</span>
              {(["all", "whdload", "floppies", "hardfile"] as const).map((fmt) => (
                <button
                  key={fmt}
                  className={`btn btn-sm ${formatFilter === fmt ? "btn-primary" : ""}`}
                  onClick={() => setFormatFilter(fmt)}
                  style={{ padding: "3px 8px", fontSize: 11 }}
                >
                  {fmt === "all" ? t("collection.filters.all") : fmt === "whdload" ? "🕹️ WHDLoad" : fmt === "floppies" ? `💾 ${t("collection.filters.floppy")}` : "💽 HDF"}
                </button>
              ))}
            </div>

            {/* Chipset Filter Chips */}
            <div style={{ display: "flex", gap: 6, flexWrap: "wrap" }}>
              <span className="muted" style={{ fontSize: 12, alignSelf: "center" }}>{t("collection.filters.chipsetLabel")}</span>
              {(["all", "ocsecs", "aga"] as const).map((cs) => (
                <button
                  key={cs}
                  className={`btn btn-sm ${chipsetFilter === cs ? "btn-primary" : ""}`}
                  onClick={() => setChipsetFilter(cs)}
                  style={{ padding: "3px 8px", fontSize: 11 }}
                >
                  {cs === "all" ? t("collection.filters.all") : cs === "aga" ? "AGA (A1200/4000)" : "OCS/ECS (A500/600)"}
                </button>
              ))}
            </div>
          </div>
        </section>
      )}

      {/* Catalog Results Header */}
      {items.length > 0 && (
        <div style={{ display: "flex", justifyContent: "space-between", fontSize: 12, margin: "6px 2px 10px" }}>
          <span className="muted">
            {t("collection.results.summary", { shown: filteredItems.length, total: items.length })}
          </span>
        </div>
      )}

      {/* The listing and, once a title is opened, its detail panel beside it
          — a two-column grid that collapses to one column at the same width
          the app's own sidebar already does (`layout.css`, 1000px). This
          screen has no stylesheet of its own to hang that media query on, so
          the query travels with the one class it applies to. Application
          Size is untouched either way: neither column gets a fixed pixel
          height (ART-082). */}
      <style>{`
        .collection-with-detail {
          display: grid;
          grid-template-columns: 2fr 1fr;
          gap: 14px;
          align-items: start;
        }
        /* **A class, not a media query** — ART-174, the same defect ART-101
           fixed in the shell. \`.app-shell\` carries \`zoom\`, so
           \`@media (max-width: 1000px)\` is evaluated against a window this
           layout does not live in: at 200 % the shell has 629 px to work with
           and the query still answers "wide". \`.app-shell-narrow\` is set from
           \`innerWidth / zoom\` in \`@/lib/appZoom\`, is on an ancestor of this
           grid, and carries the identical 1000 px threshold
           (\`SIDEBAR_ICONS_BELOW\`) — so this stacks exactly when the sidebar
           collapses, which is what the original rule meant. */
        .app-shell-narrow .collection-with-detail {
          grid-template-columns: 1fr;
        }
      `}</style>
      <div className={selectedItem ? "collection-with-detail" : undefined}>
        <div>
      {/* VIEW MODE 1: VISUAL GRID VIEW */}
      {viewMode === "grid" && filteredItems.length > 0 && (
        <div style={{ display: "grid", gridTemplateColumns: "repeat(auto-fill, minmax(280px, 1fr))", gap: 12 }}>
          {filteredItems.map((item) => {
            const isAga = item.chipset === "aga";
            const chipsetKnown = item.chipset !== null;
            return (
              <div
                key={item.id}
                className="card"
                onClick={() => handleSelect(item)}
                style={{
                  display: "flex",
                  flexDirection: "column",
                  justifyContent: "space-between",
                  padding: "12px",
                  cursor: "pointer",
                  borderColor: item.id === selectedId ? "var(--accent)" : undefined,
                  transition: "transform 0.1s, border-color 0.1s",
                }}
              >
                <div>
                  {/* Top Badges */}
                  <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center", marginBottom: 6 }}>
                    <span
                      className={`badge ${!chipsetKnown ? "badge-muted" : isAga ? "badge-warn" : "badge-ok"}`}
                      style={{ fontSize: 10 }}
                    >
                      {!chipsetKnown
                        ? t("common.unknown")
                        : isAga
                          ? "AGA (1200/4000)"
                          : "OCS / ECS (500)"}
                      <Guessed from={item.chipsetFrom} />
                    </span>
                    <span
                      className={`badge ${item.available ? "badge-muted" : "badge-warn"}`}
                      style={{ fontSize: 10 }}
                      title={
                        item.available
                          ? undefined
                          : t("gameindex.catalogue.unavailableHint", { path: item.path })
                      }
                    >
                      {!item.available
                        ? `⚠ ${t("gameindex.catalogue.unavailable")}`
                        : item.media === "whdload" ? "WHDLoad" : item.media === "floppies" ? t("collection.item.floppyDisks", { count: item.diskCount }) : t("collection.item.hardfile")}
                    </span>
                  </div>

                  {/* The picture, when the cache has one. A row without one is
                      the ordinary case until the user runs a fetch, so nothing
                      reserves space for an image that may never arrive. */}
                  {art.get(item.id) && (
                    <img
                      src={art.get(item.id)}
                      alt=""
                      loading="lazy"
                      style={{
                        display: "block",
                        width: "100%",
                        maxHeight: 160,
                        objectFit: "contain",
                        marginBottom: 6,
                      }}
                    />
                  )}

                  {/* Title & Publisher */}
                  <strong style={{ fontSize: 14, color: "var(--text)", display: "block", marginBottom: 2 }}>
                    {item.title}
                    <Guessed from={item.titleFrom} />
                  </strong>
                  <div className="muted" style={{ fontSize: 12 }}>
                    {item.publisher ?? `${t("common.unknown")} ${t("collection.item.publisher")}`}
                    <Guessed from={item.publisherFrom} />
                    {item.year ? ` (${item.year})` : ""}
                    <Guessed from={item.yearFrom} />
                  </div>
                  {item.kickstart && (
                    <div className="faint" style={{ fontSize: 11, marginTop: 2 }}>
                      {t("gameindex.kickstartNeeded", { image: item.kickstart })}
                    </div>
                  )}
                  {/* A click on any of these fixes is a decision about the
                      name, not a card selection — it must not also toggle
                      the detail panel open or closed underneath it. */}
                  <div onClick={(e) => e.stopPropagation()}>
                    <NameFixes
                      current={item.title}
                      suggestion={suggestions.get(item.id)}
                      undo={undoable.get(item.id)}
                      onTitle={(proposed) => void applyTitle(item, proposed)}
                      onRename={(proposed) => void applyRename(item, proposed)}
                      onUndo={() => void undoTitle(item)}
                    />
                  </div>
                </div>

                {/* Card Actions. Stopped from bubbling for the same reason:
                    Play and the ADF-Studio shortcut are their own actions,
                    not a way to open the detail panel. */}
                <div
                  onClick={(e) => e.stopPropagation()}
                  style={{ display: "flex", gap: 6, marginTop: 12, borderTop: "1px solid var(--border)", paddingTop: 8 }}
                >
                  {/*
                    Disabled, not hidden. A game whose drive is unplugged is
                    still in the library, and hiding it would look like ART
                    lost it.
                  */}
                  <button
                    className="btn btn-sm btn-primary"
                    style={{ flex: 1, justifyContent: "center" }}
                    disabled={!item.available}
                    title={
                      item.available
                        ? undefined
                        : t("gameindex.catalogue.unavailableHint", { path: item.path })
                    }
                    onClick={() => handlePlay(item)}
                  >
                    🚀 {t("common.launchInWinuae")}
                  </button>
                  {item.media === "floppies" && (
                    <button
                      className="btn btn-sm"
                      disabled={!item.available}
                      title={t("collection.item.openInAdfStudio")}
                      onClick={() => navigate("/disk-tools", { state: { path: item.path } })}
                    >
                      💾
                    </button>
                  )}
                </div>
              </div>
            );
          })}
        </div>
      )}

      {/* VIEW MODE 2: COMPACT TABLE VIEW */}
      {viewMode === "table" && filteredItems.length > 0 && (
        <section className="card" style={{ padding: 0, overflow: "hidden" }}>
          <div className="file-list-container">
            {filteredItems.map((item) => (
              <div
                key={item.id}
                className="file-row"
                onClick={() => handleSelect(item)}
                style={{
                  padding: "8px 12px",
                  cursor: "pointer",
                  // The same selected-row affordance `HardDiskStudio`'s
                  // partition list already uses for a `.file-row` — not a
                  // new highlight invented for this screen.
                  borderColor: item.id === selectedId ? "var(--accent)" : "transparent",
                }}
              >
                <div className="file-row-main" style={{ gap: 10 }}>
                  {/* The picture stands in for the media glyph when there is
                      one, so a row never grows taller than the rows around it. */}
                  {art.get(item.id) ? (
                    <img
                      src={art.get(item.id)}
                      alt=""
                      loading="lazy"
                      style={{ width: 28, height: 28, objectFit: "contain", flexShrink: 0 }}
                    />
                  ) : (
                    <span className="file-row-icon">
                      {item.media === "whdload" ? "🕹️" : item.media === "floppies" ? "💾" : "💽"}
                    </span>
                  )}
                  <div>
                    <strong>
                      {item.title}
                      <Guessed from={item.titleFrom} />
                    </strong>
                    <div className="faint" style={{ fontSize: 11 }}>
                      {item.publisher ?? t("common.unknown")}
                      <Guessed from={item.publisherFrom} />
                      {item.year ? ` · ${item.year}` : ""}
                    </div>
                    {/* A name fix is a decision about the title, not a row
                        selection — it must not also toggle the panel. */}
                    <div onClick={(e) => e.stopPropagation()}>
                      <NameFixes
                        current={item.title}
                        suggestion={suggestions.get(item.id)}
                        undo={undoable.get(item.id)}
                        onTitle={(proposed) => void applyTitle(item, proposed)}
                        onRename={(proposed) => void applyRename(item, proposed)}
                        onUndo={() => void undoTitle(item)}
                      />
                    </div>
                  </div>
                </div>

                <div
                  className="file-row-meta"
                  onClick={(e) => e.stopPropagation()}
                  style={{ gap: 10 }}
                >
                  <span
                    className={`badge ${item.chipset === null ? "badge-muted" : item.chipset === "aga" ? "badge-warn" : "badge-ok"}`}
                    style={{ fontSize: 10 }}
                  >
                    {item.chipset === null ? "—" : item.chipset === "aga" ? "AGA" : "OCS/ECS"}
                    <Guessed from={item.chipsetFrom} />
                  </span>
                  <span className="badge badge-muted" style={{ fontSize: 10 }}>
                    {t("collection.item.disksBadge", { count: item.diskCount })}
                  </span>
                  {!item.available && (
                    <span
                      className="badge badge-warn"
                      style={{ fontSize: 10 }}
                      title={t("gameindex.catalogue.unavailableHint", { path: item.path })}
                    >
                      ⚠ {t("gameindex.catalogue.unavailable")}
                    </span>
                  )}
                  <button
                    className="btn btn-sm btn-primary"
                    disabled={!item.available}
                    onClick={() => handlePlay(item)}
                    style={{ padding: "3px 8px", fontSize: 11 }}
                  >
                    🚀 {t("collection.item.play")}
                  </button>
                </div>
              </div>
            ))}
          </div>
        </section>
      )}

      {items.length === 0 && !busy && (
        <p className="muted" style={{ textAlign: "center", marginTop: 36 }}>
          {t("collection.empty.noCollection", { buttonLabel: t("collection.toolbar.scanFolder") })}
        </p>
      )}
        </div>

        {selectedItem && (
          <TitleDetail
            entry={{ path: selectedItem.path, record: selectedItem.record }}
            art={art.get(selectedItem.id)}
            hasManualArt={manualArt.has(selectedItem.id)}
            onArtChanged={() => void loadArtwork(items)}
            onClose={() => setSelectedId(null)}
            playRequest={playRequest}
          />
        )}
      </div>
    </div>
  );
}
