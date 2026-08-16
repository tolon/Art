import { useState, useEffect } from "react";
import { useLocation } from "react-router-dom";
import { open } from "@tauri-apps/plugin-dialog";
import { useTranslation } from "react-i18next";

import { hexRead, type HexChunk } from "@/lib/analysis";
import { useOpenObject } from "@/stores/openObjectStore";

export function HexTools() {
  const { t } = useTranslation();
  const location = useLocation();

  // The open file outlives this screen (ART-085), for the length of the run.
  // The *offset* does not: coming back puts you at the start of the file, not
  // where you were reading. That is the same complaint one level down and is
  // deliberately left for its own change rather than smuggled in here.
  const [path, setPath] = useOpenObject("hex");
  const [offset, setOffset] = useState<number>(0);
  const [chunk, setChunk] = useState<HexChunk | null>(null);
  const [jumpBlockInput, setJumpBlockInput] = useState<string>("880");

  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // Router state names a file when something sent us here; otherwise the screen
  // reopens the one already open (ART-085). `chunk` is null on a fresh mount
  // whatever the store holds, so it is the test of "loaded here, now".
  useEffect(() => {
    const fromNav = (location.state as { path?: string } | undefined)?.path;
    const target = fromNav ?? path;
    if (target && (chunk === null || target !== path)) {
      void loadHex(target, 0);
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [location.state]);

  async function loadHex(filePath: string, targetOffset: number) {
    setBusy(true);
    setError(null);
    try {
      const data = await hexRead(filePath, targetOffset, 512);
      setPath(filePath);
      setOffset(targetOffset);
      setChunk(data);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function handleOpenFile() {
    const sel = await open({
      multiple: false,
      title: t("hex.dialogTitle"),
    });
    if (typeof sel === "string") {
      await loadHex(sel, 0);
    }
  }

  function handleJumpBlock(blockNum: number) {
    if (!path) return;
    const targetOffset = blockNum * 512;
    void loadHex(path, targetOffset);
  }

  function handleNextBlock() {
    if (!path || !chunk) return;
    const nextOffset = Math.min(chunk.total_file_size - 512, offset + 512);
    void loadHex(path, nextOffset);
  }

  function handlePrevBlock() {
    if (!path) return;
    const prevOffset = Math.max(0, offset - 512);
    void loadHex(path, prevOffset);
  }

  return (
    <div>
      <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center" }}>
        <h1 style={{ fontSize: 20, margin: 0 }}>{t("nav.tools")} — {t("hex.subtitle")}</h1>
        <button className="btn btn-sm btn-primary" onClick={handleOpenFile} disabled={busy}>
          📂 {t("hex.openFile")}
        </button>
      </div>

      {path && (
        <div style={{ margin: "8px 0 12px", fontSize: 12 }}>
          <span className="muted">{t("hex.inspecting")}</span>{" "}
          <strong style={{ wordBreak: "break-all" }}>{path}</strong>
        </div>
      )}

      {error && <div className="badge badge-err" style={{ marginBottom: 12, padding: "6px 12px" }}>{error}</div>}

      {/* Navigation Toolbar */}
      {chunk && (
        <section className="card" style={{ marginBottom: 12, padding: "10px 14px" }}>
          <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center", flexWrap: "wrap", gap: 10 }}>
            {/* Sector / Block Metadata */}
            <div style={{ display: "flex", gap: 12, fontSize: 13 }}>
              <div><span className="muted">{t("hex.fields.offset")}</span> <code>0x{offset.toString(16).toUpperCase().padStart(6, "0")}</code> ({offset} B)</div>
              {chunk.block !== null && <div><span className="muted">{t("hex.fields.block")}</span> <strong>{chunk.block}</strong></div>}
              {chunk.track !== null && <div><span className="muted">{t("hex.fields.trackSector")}</span> <strong>{chunk.track} / {chunk.sector}</strong></div>}
            </div>

            {/* Jump Presets */}
            <div style={{ display: "flex", gap: 6 }}>
              <button className="btn btn-sm" onClick={() => handleJumpBlock(0)} disabled={busy}>
                {t("hex.jump.bootblock")}
              </button>
              <button className="btn btn-sm" onClick={() => handleJumpBlock(880)} disabled={busy}>
                {t("hex.jump.rootBlock")}
              </button>
              <button className="btn btn-sm" onClick={() => handleJumpBlock(881)} disabled={busy}>
                {t("hex.jump.bitmap")}
              </button>
            </div>

            {/* Manual Block Jump */}
            <div style={{ display: "flex", alignItems: "center", gap: 6 }}>
              <input
                type="number"
                value={jumpBlockInput}
                onChange={(e) => setJumpBlockInput(e.target.value)}
                placeholder={t("hex.jump.placeholder")}
                style={{ width: 80, padding: "4px 6px", background: "var(--bg)", color: "var(--text)", border: "1px solid var(--border)", borderRadius: 4 }}
              />
              <button
                className="btn btn-sm"
                onClick={() => handleJumpBlock(Number(jumpBlockInput) || 0)}
                disabled={busy}
              >
                {t("hex.jump.go")}
              </button>
            </div>
          </div>
        </section>
      )}

      {/* Detected Signatures */}
      {chunk && chunk.signatures.length > 0 && (
        <div style={{ display: "flex", gap: 8, marginBottom: 12, flexWrap: "wrap" }}>
          {chunk.signatures.map((s, i) => (
            <span key={i} className="badge badge-ok" style={{ padding: "4px 8px" }}>
              🔍 {t("hex.signature.found")} <strong>{s.signature}</strong> ({s.description})
            </span>
          ))}
        </div>
      )}

      {/* Hex + ASCII Data View */}
      {chunk && (
        <section
          className="card"
          style={{
            fontFamily: "'Courier New', Courier, monospace",
            fontSize: 12,
            background: "#0d1117",
            color: "#c9d1d9",
            overflowX: "auto",
            padding: 12,
          }}
        >
          <div style={{ display: "flex", borderBottom: "1px solid #21262d", paddingBottom: 4, marginBottom: 6, color: "var(--text-muted)", fontWeight: "bold" }}>
            <span style={{ width: 90 }}>{t("hex.columns.offset")}</span>
            <span style={{ flex: 1 }}>{t("hex.columns.hexBytes")}</span>
            <span style={{ width: 150 }}>{t("hex.columns.ascii")}</span>
          </div>

          {chunk.lines.map((l) => (
            <div key={l.offset} style={{ display: "flex", lineHeight: 1.5 }}>
              <span style={{ width: 90, color: "#8b949e" }}>{l.offset_hex}</span>
              <span style={{ flex: 1, color: "#79c0ff", letterSpacing: 1 }}>{l.bytes_hex}</span>
              <span style={{ width: 150, color: "#7ee787", letterSpacing: 1 }}>{l.ascii}</span>
            </div>
          ))}
        </section>
      )}

      {/* Paging Buttons */}
      {chunk && (
        <div style={{ display: "flex", justifyContent: "space-between", marginTop: 12 }}>
          <button className="btn btn-sm" onClick={handlePrevBlock} disabled={offset === 0 || busy}>
            {t("hex.paging.previous")}
          </button>
          <span className="muted" style={{ fontSize: 12 }}>
            {t("hex.paging.viewing", { start: offset, end: offset + chunk.length, total: chunk.total_file_size })}
          </span>
          <button
            className="btn btn-sm"
            onClick={handleNextBlock}
            disabled={offset + chunk.length >= chunk.total_file_size || busy}
          >
            {t("hex.paging.next")}
          </button>
        </div>
      )}

      {!path && !busy && (
        <p className="muted" style={{ textAlign: "center", marginTop: 32 }}>
          {t("hex.emptyState")}
        </p>
      )}
    </div>
  );
}
