import { useEffect, useMemo, useRef, useState } from "react";
import { useLocation, useNavigate } from "react-router-dom";
import { open } from "@tauri-apps/plugin-dialog";
import { useTranslation } from "react-i18next";

import {
  collectionScan,
  onScanResult,
  type CollectionItem,
  type MediaKind,
  type ChipsetRequirement,
} from "@/lib/collection";

type ViewMode = "grid" | "table";
type FormatFilter = "all" | MediaKind;
type ChipsetFilter = "all" | ChipsetRequirement;

export function CollectionStudio() {
  const { t } = useTranslation();
  const navigate = useNavigate();
  const location = useLocation();

  const [scanDir, setScanDir] = useState<string | null>(null);
  const [items, setItems] = useState<CollectionItem[]>([]);
  const [searchTerm, setSearchTerm] = useState<string>("");
  const [viewMode, setViewMode] = useState<ViewMode>("grid");
  const [formatFilter, setFormatFilter] = useState<FormatFilter>("all");
  const [chipsetFilter, setChipsetFilter] = useState<ChipsetFilter>("all");

  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [statusMsg, setStatusMsg] = useState<string | null>(null);

  // A scan runs as a background job: it reports through the global job bar and
  // delivers its titles here when it finishes (spec §54).
  useEffect(() => {
    let cancelled = false;
    let unlisten: (() => void) | undefined;

    void (async () => {
      const stop = await onScanResult((result) => {
        setScanDir(result.dir_path);
        setItems(result.items);
        setStatusMsg(
          `Indexed ${result.items.length} game/software title(s) in collection.`
        );
        setBusy(false);
      });
      if (cancelled) stop();
      else unlisten = stop;
    })();

    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, []);

  async function startScan(dir: string) {
    setBusy(true);
    setError(null);
    setStatusMsg("Scanning… you can carry on working while it runs.");
    try {
      await collectionScan(dir);
    } catch (e) {
      setError(String(e));
      setBusy(false);
    }
  }

  async function handleScanFolder() {
    const sel = await open({
      directory: true,
      multiple: false,
      title: "Select folder containing Amiga games and software",
    });
    if (typeof sel !== "string") return;
    await startScan(sel);
  }

  // Another screen can send us a folder to index — Aminet does this after a
  // download so the package shows up next to the rest of the library. The ref
  // keeps StrictMode's double mount from starting the same scan twice.
  const arrivedWith = useRef<string | null>(null);
  useEffect(() => {
    const state = location.state as { path?: string } | null;
    const wanted = state?.path;
    if (!wanted || arrivedWith.current === wanted) return;
    arrivedWith.current = wanted;
    void startScan(wanted);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [location.state]);

  // Real-time filtered items
  const filteredItems = useMemo(() => {
    return items.filter((item) => {
      // Search term
      if (searchTerm.trim()) {
        const q = searchTerm.toLowerCase();
        const matchTitle = item.clean_title.toLowerCase().includes(q);
        const matchPub = item.publisher?.toLowerCase().includes(q) ?? false;
        const matchYear = item.year?.toString().includes(q) ?? false;
        if (!matchTitle && !matchPub && !matchYear) return false;
      }

      // Format filter
      if (formatFilter !== "all" && item.media_kind !== formatFilter) {
        return false;
      }

      // Chipset filter
      if (chipsetFilter !== "all" && item.chipset !== chipsetFilter) {
        return false;
      }

      return true;
    });
  }, [items, searchTerm, formatFilter, chipsetFilter]);

  function handlePlay(item: CollectionItem) {
    navigate("/winuae", { state: { path: item.primary_path } });
  }

  return (
    <div style={{ maxWidth: 1040 }}>
      {/* Top Header */}
      <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center", flexWrap: "wrap", gap: 10 }}>
        <div>
          <h1 style={{ fontSize: 20, margin: 0 }}>{t("nav.collection")} — Amiga Collection Studio</h1>
          {scanDir && (
            <div className="muted" style={{ fontSize: 12, marginTop: 2, wordBreak: "break-all" }}>
              Library: {scanDir} ({items.length} titles)
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
              🔲 Grid
            </button>
            <button
              className={`btn btn-sm ${viewMode === "table" ? "btn-primary" : ""}`}
              onClick={() => setViewMode("table")}
              style={{ borderRadius: 0, padding: "4px 10px" }}
            >
              📋 Table
            </button>
          </div>

          <button className="btn btn-sm btn-primary" onClick={handleScanFolder} disabled={busy}>
            📂 Scan Library Folder…
          </button>
        </div>
      </div>

      {error && <div className="badge badge-err" style={{ margin: "12px 0", padding: "6px 12px" }}>{error}</div>}
      {statusMsg && <div className="badge badge-ok" style={{ margin: "12px 0", padding: "6px 12px" }}>{statusMsg}</div>}
      {busy && <div className="muted" style={{ margin: "12px 0" }}>Scanning games and indexing collection…</div>}

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
                placeholder="🔍 Search titles, publishers, or years (e.g. Monkey Island, 1992, Ocean)…"
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
              <span className="muted" style={{ fontSize: 12, alignSelf: "center" }}>Format:</span>
              {(["all", "lhawhdload", "adf", "hdf"] as const).map((fmt) => (
                <button
                  key={fmt}
                  className={`btn btn-sm ${formatFilter === fmt ? "btn-primary" : ""}`}
                  onClick={() => setFormatFilter(fmt)}
                  style={{ padding: "3px 8px", fontSize: 11 }}
                >
                  {fmt === "all" ? "All" : fmt === "lhawhdload" ? "🕹️ WHDLoad" : fmt === "adf" ? "💾 Floppy" : "💽 HDF"}
                </button>
              ))}
            </div>

            {/* Chipset Filter Chips */}
            <div style={{ display: "flex", gap: 6, flexWrap: "wrap" }}>
              <span className="muted" style={{ fontSize: 12, alignSelf: "center" }}>Chipset:</span>
              {(["all", "ocsecs", "aga"] as const).map((cs) => (
                <button
                  key={cs}
                  className={`btn btn-sm ${chipsetFilter === cs ? "btn-primary" : ""}`}
                  onClick={() => setChipsetFilter(cs)}
                  style={{ padding: "3px 8px", fontSize: 11 }}
                >
                  {cs === "all" ? "All" : cs === "aga" ? "AGA (A1200/4000)" : "OCS/ECS (A500/600)"}
                </button>
              ))}
            </div>
          </div>
        </section>
      )}

      {/* Catalog Results Header */}
      {items.length > 0 && (
        <div style={{ display: "flex", justifyContent: "space-between", fontSize: 12, margin: "6px 2px 10px" }}>
          <span className="muted">Showing <strong>{filteredItems.length}</strong> of {items.length} titles</span>
        </div>
      )}

      {/* VIEW MODE 1: VISUAL GRID VIEW */}
      {viewMode === "grid" && filteredItems.length > 0 && (
        <div style={{ display: "grid", gridTemplateColumns: "repeat(auto-fill, minmax(280px, 1fr))", gap: 12 }}>
          {filteredItems.map((item) => {
            const isAga = item.chipset === "aga";
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
                    <span className={`badge ${isAga ? "badge-warn" : "badge-ok"}`} style={{ fontSize: 10 }}>
                      {isAga ? "AGA (1200/4000)" : "OCS / ECS (500)"}
                    </span>
                    <span className="badge badge-muted" style={{ fontSize: 10 }}>
                      {item.media_kind === "lhawhdload" ? "WHDLoad LHA" : item.media_kind === "adf" ? `Floppy (${item.disk_count} Disk${item.disk_count > 1 ? "s" : ""})` : "Hardfile HDF"}
                    </span>
                  </div>

                  {/* Title & Publisher */}
                  <strong style={{ fontSize: 14, color: "var(--text)", display: "block", marginBottom: 2 }}>
                    {item.clean_title}
                  </strong>
                  <div className="muted" style={{ fontSize: 12 }}>
                    {item.publisher ?? "Unknown Publisher"} {item.year ? `(${item.year})` : ""}
                  </div>
                </div>

                {/* Card Actions */}
                <div style={{ display: "flex", gap: 6, marginTop: 12, borderTop: "1px solid var(--border)", paddingTop: 8 }}>
                  <button
                    className="btn btn-sm btn-primary"
                    style={{ flex: 1, justifyContent: "center" }}
                    onClick={() => handlePlay(item)}
                  >
                    🚀 Play in WinUAE
                  </button>
                  {item.media_kind === "adf" && (
                    <button
                      className="btn btn-sm"
                      title="Open in ADF Studio"
                      onClick={() => navigate("/disk-tools", { state: { path: item.primary_path } })}
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
                    {item.media_kind === "lhawhdload" ? "🕹️" : item.media_kind === "adf" ? "💾" : "💽"}
                  </span>
                  <div>
                    <strong>{item.clean_title}</strong>
                    <div className="faint" style={{ fontSize: 11 }}>
                      {item.publisher ?? "Unknown"} {item.year ? `· ${item.year}` : ""}
                    </div>
                  </div>
                </div>

                <div className="file-row-meta" style={{ gap: 10 }}>
                  <span className={`badge ${item.chipset === "aga" ? "badge-warn" : "badge-ok"}`} style={{ fontSize: 10 }}>
                    {item.chipset === "aga" ? "AGA" : "OCS/ECS"}
                  </span>
                  <span className="badge badge-muted" style={{ fontSize: 10 }}>
                    {item.disk_count} Disks
                  </span>
                  <button
                    className="btn btn-sm btn-primary"
                    onClick={() => handlePlay(item)}
                    style={{ padding: "3px 8px", fontSize: 11 }}
                  >
                    🚀 Play
                  </button>
                </div>
              </div>
            ))}
          </div>
        </section>
      )}

      {items.length === 0 && !busy && (
        <p className="muted" style={{ textAlign: "center", marginTop: 36 }}>
          No collection loaded. Click "Scan Library Folder" to catalog your Amiga games, WHDLoad archives, and disks.
        </p>
      )}
    </div>
  );
}
