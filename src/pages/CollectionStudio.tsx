import { useEffect, useMemo, useRef, useState } from "react";
import { useLocation, useNavigate } from "react-router-dom";
import { confirm, open } from "@tauri-apps/plugin-dialog";
import { useTranslation } from "react-i18next";

import {
  catalogueAddRoot,
  catalogueLoad,
  catalogueRefresh,
  catalogueRemoveRoot,
  isStated,
  mediaKind,
  onCatalogueRefreshed,
  provenancePhrase,
  type ChipsetRequirement,
  type EntryView,
  type Provenance,
  type RefreshMode,
  type RootView,
} from "@/lib/gameindex";
import { isOneOf } from "@/lib/remembered";
import { useRemembered } from "@/lib/useRemembered";
import { useSettingsStore } from "@/stores/settingsStore";

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
  };
}

/**
 * A small mark on any value the index **guessed** rather than read.
 *
 * This is the feature the provenance in the record exists for. `Agassi Tennis`
 * reads as AGA because the letters are in its filename; a slave that states
 * `ReqAGA` is a different claim entirely, and a screen showing the two the
 * same way throws away the only thing that separates them.
 */
function Guessed({ from }: { from: Provenance | null }) {
  const { t } = useTranslation();
  if (!from || isStated(from)) return null;
  const source = t(provenancePhrase(from).key);
  return (
    <span
      className="badge badge-muted"
      title={t("gameindex.guessedFrom", { source })}
      style={{ fontSize: 9, marginLeft: 4, verticalAlign: "middle" }}
    >
      ~{t("gameindex.guessed")}
    </span>
  );
}

export function CollectionStudio() {
  const { t } = useTranslation();
  const navigate = useNavigate();
  const location = useLocation();

  const [scanDir, setScanDir] = useState<string | null>(null);
  const [items, setItems] = useState<Shown[]>([]);
  const [searchTerm, setSearchTerm] = useState<string>("");

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
      setItems([...byId.values()]);
      setScanDir(loaded.length === 1 ? loaded[0].root : null);
      setError(null);
    } catch (e) {
      setError(String(e));
    }
  }

  useEffect(() => {
    void reload();

    let cancelled = false;
    let unlisten: (() => void) | undefined;
    void (async () => {
      const stop = await onCatalogueRefreshed(() => {
        setBusy(false);
        void reload();
      });
      if (cancelled) stop();
      else unlisten = stop;
    })();

    return () => {
      cancelled = true;
      unlisten?.();
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

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

  function handlePlay(item: Shown) {
    navigate("/winuae", { state: { path: item.path } });
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
      </section>

      {error && <div className="badge badge-err" style={{ margin: "12px 0", padding: "6px 12px" }}>{error}</div>}
      {statusMsg && <div className="badge badge-ok" style={{ margin: "12px 0", padding: "6px 12px" }}>{statusMsg}</div>}
      {busy && <div className="muted" style={{ margin: "12px 0" }}>{t("collection.status.scanningLong")}</div>}

      {/* Filter Toolbar */}
      {items.length > 0 && (
        <section className="card" style={{ margin: "14px 0", padding: "10px 14px" }}>
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
                style={{
                  display: "flex",
                  flexDirection: "column",
                  justifyContent: "space-between",
                  padding: "12px",
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
                </div>

                {/* Card Actions */}
                <div style={{ display: "flex", gap: 6, marginTop: 12, borderTop: "1px solid var(--border)", paddingTop: 8 }}>
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
                style={{ padding: "8px 12px" }}
              >
                <div className="file-row-main" style={{ gap: 10 }}>
                  <span className="file-row-icon">
                    {item.media === "whdload" ? "🕹️" : item.media === "floppies" ? "💾" : "💽"}
                  </span>
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
                  </div>
                </div>

                <div className="file-row-meta" style={{ gap: 10 }}>
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
  );
}
