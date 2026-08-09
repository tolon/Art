import { useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { useTranslation } from "react-i18next";

import { romIdentify, romScanDir, type RomInfo } from "@/lib/rom";

export function RomStudio() {
  const { t } = useTranslation();
  const [roms, setRoms] = useState<RomInfo[]>([]);
  const [selectedRom, setSelectedRom] = useState<RomInfo | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [statusMsg, setStatusMsg] = useState<string | null>(null);

  async function handleScanDir() {
    const sel = await open({
      directory: true,
      multiple: false,
      title: "Select folder containing Kickstart ROMs",
    });
    if (typeof sel !== "string") return;

    setBusy(true);
    setError(null);
    setStatusMsg(null);
    try {
      const list = await romScanDir(sel);
      setRoms(list);
      setStatusMsg(`Found and identified ${list.length} Kickstart ROM(s) in folder.`);
      if (list.length > 0) setSelectedRom(list[0]);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function handleIdentifySingle() {
    const sel = await open({
      multiple: false,
      filters: [{ name: "Kickstart ROM", extensions: ["rom", "bin", "a500", "a1200"] }],
      title: "Select Kickstart ROM file",
    });
    if (typeof sel !== "string") return;

    setBusy(true);
    setError(null);
    setStatusMsg(null);
    try {
      const info = await romIdentify(sel);
      setRoms((prev) => {
        const filtered = prev.filter((r) => r.file_path !== info.file_path);
        return [info, ...filtered];
      });
      setSelectedRom(info);
      setStatusMsg(`Identified: ${info.name}`);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  return (
    <div style={{ maxWidth: 980 }}>
      <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center" }}>
        <h1 style={{ fontSize: 20, margin: 0 }}>{t("nav.rom")} — Kickstart ROM Studio</h1>
        <div style={{ display: "flex", gap: 8 }}>
          <button className="btn btn-sm" onClick={handleIdentifySingle} disabled={busy}>
            🔍 Identify ROM File…
          </button>
          <button className="btn btn-primary btn-sm" onClick={handleScanDir} disabled={busy}>
            📂 Scan ROM Folder…
          </button>
        </div>
      </div>

      {error && <div className="badge badge-err" style={{ margin: "12px 0", padding: "6px 12px" }}>{error}</div>}
      {statusMsg && <div className="badge badge-ok" style={{ margin: "12px 0", padding: "6px 12px" }}>{statusMsg}</div>}
      {busy && <div className="muted" style={{ margin: "12px 0" }}>Scanning ROMs…</div>}

      {/* Information Banner */}
      <div className="card" style={{ margin: "14px 0", background: "var(--bg-elevated)" }}>
        <div style={{ fontWeight: 600, fontSize: 13 }}>💡 Kickstart ROM & Copyright Notice</div>
        <p className="muted" style={{ fontSize: 12, margin: "4px 0 0" }}>
          Commodore Amiga Kickstart ROMs are proprietary and copyrighted. ART does not distribute ROM binaries.
          You can scan your legally owned Kickstart collection or use the built-in <strong>AROS Open-Source Replacement ROM</strong>.
        </p>
      </div>

      {/* ROMs Table & Details */}
      <div style={{ display: "grid", gridTemplateColumns: roms.length > 0 ? "1fr 1fr" : "1fr", gap: 16 }}>
        {/* List of Detected ROMs */}
        <section className="card">
          <h2 style={{ fontSize: 15 }}>📚 Detected Kickstart Library ({roms.length})</h2>
          {roms.length === 0 ? (
            <p className="muted" style={{ fontSize: 13, padding: "16px 0", textAlign: "center" }}>
              No ROMs loaded yet. Click "Scan ROM Folder" to detect your Kickstart files.
            </p>
          ) : (
            <div style={{ display: "flex", flexDirection: "column", gap: 6, marginTop: 10 }}>
              {roms.map((r, i) => {
                const isSelected = selectedRom?.file_path === r.file_path;
                return (
                  <div
                    key={i}
                    onClick={() => setSelectedRom(r)}
                    style={{
                      padding: "8px 10px",
                      borderRadius: "var(--radius-sm)",
                      background: isSelected ? "var(--bg-hover)" : "var(--bg)",
                      border: isSelected ? "1px solid var(--accent)" : "1px solid var(--border)",
                      cursor: "pointer",
                    }}
                  >
                    <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center" }}>
                      <strong style={{ fontSize: 13 }}>{r.name}</strong>
                      <span className={`badge ${r.checksum_valid ? "badge-ok" : "badge-warn"}`}>
                        {r.checksum_valid ? "OK" : "CRC ERR"}
                      </span>
                    </div>
                    <div className="faint" style={{ fontSize: 11, marginTop: 4, wordBreak: "break-all" }}>
                      {r.file_path}
                    </div>
                  </div>
                );
              })}
            </div>
          )}
        </section>

        {/* Selected ROM Deep Inspector */}
        {selectedRom && (
          <section className="card">
            <h2 style={{ fontSize: 15 }}>🔬 ROM Details</h2>
            <div style={{ display: "flex", flexDirection: "column", gap: 10, marginTop: 10, fontSize: 13 }}>
              <div>
                <span className="muted">Title:</span> <strong>{selectedRom.name}</strong>
              </div>
              <div>
                <span className="muted">Version / Rev:</span> {selectedRom.version} ({selectedRom.revision})
              </div>
              <div>
                <span className="muted">Size:</span> {selectedRom.size_bytes / 1024} KB ({selectedRom.size_bytes} bytes)
              </div>
              <div>
                <span className="muted">CRC32:</span> <code style={{ color: "var(--accent)" }}>{selectedRom.crc32}</code>
              </div>
              <div>
                <span className="muted">SHA256:</span>
                <div style={{ fontSize: 11, wordBreak: "break-all", background: "var(--bg)", padding: 6, borderRadius: 4, marginTop: 2 }}>
                  {selectedRom.sha256}
                </div>
              </div>
              <div>
                <span className="muted">Compatible Amiga Models:</span>
                <div style={{ display: "flex", gap: 4, flexWrap: "wrap", marginTop: 4 }}>
                  {selectedRom.compatible_models.map((m) => (
                    <span key={m} className="badge badge-muted">
                      {m}
                    </span>
                  ))}
                </div>
              </div>
              {selectedRom.is_cloanto && (
                <div className="badge badge-ok">
                  ✓ Cloanto Encrypted Header Stripped
                </div>
              )}
            </div>
          </section>
        )}
      </div>
    </div>
  );
}
