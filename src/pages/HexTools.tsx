import { useState, useEffect } from "react";
import { useLocation } from "react-router-dom";
import { open } from "@tauri-apps/plugin-dialog";
import { useTranslation } from "react-i18next";

import { hexRead, type HexChunk } from "@/lib/analysis";

export function HexTools() {
  const { t } = useTranslation();
  const location = useLocation();

  const [path, setPath] = useState<string | null>(null);
  const [offset, setOffset] = useState<number>(0);
  const [chunk, setChunk] = useState<HexChunk | null>(null);
  const [jumpBlockInput, setJumpBlockInput] = useState<string>("880");

  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    const navState = location.state as { path?: string } | undefined;
    if (navState?.path && navState.path !== path) {
      void loadHex(navState.path, 0);
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
      title: "Select Amiga disk, ROM, or archive for Forensic Hex Inspection",
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
    <div style={{ maxWidth: 980 }}>
      <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center" }}>
        <h1 style={{ fontSize: 20, margin: 0 }}>{t("nav.tools")} — Forensic Hex & Sector Inspector</h1>
        <button className="btn btn-sm btn-primary" onClick={handleOpenFile} disabled={busy}>
          📂 Open File for Hex Inspection…
        </button>
      </div>

      {path && (
        <div style={{ margin: "8px 0 12px", fontSize: 12 }}>
          <span className="muted">Inspecting:</span>{" "}
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
              <div><span className="muted">Offset:</span> <code>0x{offset.toString(16).toUpperCase().padStart(6, "0")}</code> ({offset} B)</div>
              {chunk.block !== null && <div><span className="muted">Block:</span> <strong>{chunk.block}</strong></div>}
              {chunk.track !== null && <div><span className="muted">Track / Sec:</span> <strong>{chunk.track} / {chunk.sector}</strong></div>}
            </div>

            {/* Jump Presets */}
            <div style={{ display: "flex", gap: 6 }}>
              <button className="btn btn-sm" onClick={() => handleJumpBlock(0)} disabled={busy}>
                Bootblock (0)
              </button>
              <button className="btn btn-sm" onClick={() => handleJumpBlock(880)} disabled={busy}>
                Root Block (880)
              </button>
              <button className="btn btn-sm" onClick={() => handleJumpBlock(881)} disabled={busy}>
                Bitmap (881)
              </button>
            </div>

            {/* Manual Block Jump */}
            <div style={{ display: "flex", alignItems: "center", gap: 6 }}>
              <input
                type="number"
                value={jumpBlockInput}
                onChange={(e) => setJumpBlockInput(e.target.value)}
                placeholder="Block #"
                style={{ width: 80, padding: "4px 6px", background: "var(--bg)", color: "var(--text)", border: "1px solid var(--border)", borderRadius: 4 }}
              />
              <button
                className="btn btn-sm"
                onClick={() => handleJumpBlock(Number(jumpBlockInput) || 0)}
                disabled={busy}
              >
                Go
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
              🔍 Found Signature: <strong>{s.signature}</strong> ({s.description})
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
            <span style={{ width: 90 }}>OFFSET</span>
            <span style={{ flex: 1 }}>HEX BYTES (00 01 02 03 04 05 06 07 08 09 0A 0B 0C 0D 0E 0F)</span>
            <span style={{ width: 150 }}>ASCII</span>
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
            ◀ Previous Block (512B)
          </button>
          <span className="muted" style={{ fontSize: 12 }}>
            Viewing byte {offset} to {offset + chunk.length} of {chunk.total_file_size}
          </span>
          <button
            className="btn btn-sm"
            onClick={handleNextBlock}
            disabled={offset + chunk.length >= chunk.total_file_size || busy}
          >
            Next Block (512B) ▶
          </button>
        </div>
      )}

      {!path && !busy && (
        <p className="muted" style={{ textAlign: "center", marginTop: 32 }}>
          Select any ADF floppy disk, HDF hard drive, or Kickstart ROM to inspect sector-by-sector hexadecimal data and file headers.
        </p>
      )}
    </div>
  );
}
