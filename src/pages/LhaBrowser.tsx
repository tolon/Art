import { useState, useEffect } from "react";
import { useLocation } from "react-router-dom";
import { open } from "@tauri-apps/plugin-dialog";
import { useTranslation } from "react-i18next";

import {
  lhaOpen,
  lhaExtract,
  lhaDetectWhdload,
  type LhaInfo,
  type ExtractOutcome,
  type WhdloadResult,
} from "@/lib/lha";

export function LhaBrowser() {
  const { t } = useTranslation();
  const location = useLocation();
  const [path, setPath] = useState<string | null>(null);
  const [info, setInfo] = useState<LhaInfo | null>(null);
  const [whdload, setWhdload] = useState<WhdloadResult | null>(null);
  const [outcome, setOutcome] = useState<ExtractOutcome | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  // Auto-load if navigated from Dashboard via state
  useEffect(() => {
    const p = (location.state as { path?: string })?.path;
    if (p && p !== path) {
      void load(p);
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [location.state]);

  async function load(p: string) {
    setBusy(true);
    setError(null);
    setOutcome(null);
    try {
      const i = await lhaOpen(p);
      setInfo(i);
      setPath(p);
      lhaDetectWhdload(p).then(setWhdload).catch(() => setWhdload(null));
    } catch (e) {
      setError(String(e));
      setInfo(null);
    } finally {
      setBusy(false);
    }
  }

  async function chooseArchive() {
    const sel = await open({
      multiple: false,
      filters: [{ name: t("lha.toolbar.filterName"), extensions: ["lha", "lzh"] }],
    });
    if (typeof sel === "string") await load(sel);
  }

  async function doExtract() {
    if (!path) return;
    const dest = await open({ directory: true, multiple: false });
    if (typeof dest !== "string") return;
    setBusy(true);
    setError(null);
    try {
      const out = await lhaExtract(path, dest);
      setOutcome(out);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  return (
    <div>
      <h1 style={{ fontSize: 20 }}>{t("nav.archiveTools")} — {t("lha.title")}</h1>
      <div style={{ display: "flex", gap: 8, margin: "12px 0", flexWrap: "wrap" }}>
        <button className="btn btn-primary" onClick={chooseArchive} disabled={busy}>
          {t("lha.toolbar.open")}
        </button>
        {path && info && (
          <button className="btn" onClick={doExtract} disabled={busy}>
            {t("lha.toolbar.extractAll")}
          </button>
        )}
        {path && <span className="muted" style={{ alignSelf: "center", wordBreak: "break-all" }}>{path}</span>}
      </div>

      {error && <div className="badge badge-err" style={{ marginBottom: 12 }}>{error}</div>}
      {busy && <div className="muted" style={{ marginBottom: 12 }}>{t("lha.status.working")}</div>}

      {whdload && whdload.confidence !== "UNKNOWN" && (
        <section className="card" style={{ marginBottom: 16, borderColor: "var(--accent)" }}>
          <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center" }}>
            <strong>{t("lha.whdload.detected")}</strong>
            <span
              className={`badge ${
                whdload.confidence === "HIGH"
                  ? "badge-ok"
                  : whdload.confidence === "MEDIUM"
                  ? "badge-warn"
                  : "badge-muted"
              }`}
            >
              {t("lha.whdload.confidenceBadge", {
                level: t(
                  `lha.whdload.confidenceLevel.${whdload.confidence.toLowerCase()}`
                ),
              })}
            </span>
          </div>
          <p className="muted" style={{ fontSize: 12, margin: "6px 0 0" }}>{whdload.notes}</p>
          {whdload.slave && <div className="faint" style={{ fontSize: 12, marginTop: 4 }}>{t("lha.whdload.slave", { name: whdload.slave })}</div>}
          {whdload.executable && <div className="faint" style={{ fontSize: 12 }}>{t("lha.whdload.executable", { name: whdload.executable })}</div>}
        </section>
      )}

      {info && (
        <section className="card" style={{ marginBottom: 16 }}>
          <div style={{ display: "grid", gridTemplateColumns: "repeat(auto-fit, minmax(140px, 1fr))", gap: 8, fontSize: 13 }}>
            <div><span className="muted">{t("lha.info.entries")}</span> {info.entry_count}</div>
            <div><span className="muted">{t("lha.info.compressed")}</span> {fmt(info.total_compressed)}</div>
            <div><span className="muted">{t("lha.info.uncompressed")}</span> {fmt(info.total_uncompressed)}</div>
          </div>
        </section>
      )}

      {outcome && (
        <section className="card" style={{ marginBottom: 16 }}>
          <h2 style={{ fontSize: 15 }}>
            {outcome.aborted
              ? t("lha.outcome.aborted")
              : t("lha.outcome.success", { count: outcome.extracted.length })}
          </h2>
          {outcome.abort_reason && <div className="badge badge-err">{outcome.abort_reason}</div>}
          {outcome.errors.length > 0 && (
            <ul style={{ fontSize: 12, color: "var(--err)", paddingLeft: 20 }}>
              {outcome.errors.map((e, i) => <li key={i}>{e}</li>)}
            </ul>
          )}
        </section>
      )}

      {info && info.entries.length > 0 && (
        <section className="card">
          <ul style={{ listStyle: "none", padding: 0, margin: 0 }}>
            {info.entries.map((e, i) => (
              <li key={i} className="recent-item">
                <span>
                  {e.is_dir ? "📁" : "📄"}{" "}
                  <span className="recent-name">{e.path}</span>{" "}
                  <span className="faint" style={{ fontSize: 11 }}>
                    {e.method} · {fmt(e.uncompressed_size)}
                  </span>
                </span>
              </li>
            ))}
          </ul>
        </section>
      )}

      {!path && !busy && (
        <p className="muted">{t("lha.emptyState")}</p>
      )}
    </div>
  );
}

function fmt(n: number): string {
  if (n < 1024) return `${n} B`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(0)} KB`;
  return `${(n / (1024 * 1024)).toFixed(1)} MB`;
}
