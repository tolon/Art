import { useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { useTranslation } from "react-i18next";

import { checksumBadge, romIdentify, romScanDir, type RomInfo } from "@/lib/rom";
import { errorText } from "@/lib/errorText";

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
      title: t("rom.dialogs.selectFolderTitle"),
    });
    if (typeof sel !== "string") return;

    setBusy(true);
    setError(null);
    setStatusMsg(null);
    try {
      const list = await romScanDir(sel);
      setRoms(list);
      setStatusMsg(t("rom.status.scanFound", { count: list.length }));
      if (list.length > 0) setSelectedRom(list[0]);
    } catch (e) {
      setError(errorText(t, e));
    } finally {
      setBusy(false);
    }
  }

  async function handleIdentifySingle() {
    const sel = await open({
      multiple: false,
      filters: [{ name: t("rom.filters.romFile"), extensions: ["rom", "bin", "a500", "a1200"] }],
      title: t("rom.dialogs.selectFileTitle"),
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
      setStatusMsg(t("rom.status.identified", { name: info.name }));
    } catch (e) {
      setError(errorText(t, e));
    } finally {
      setBusy(false);
    }
  }

  return (
    <div>
      <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center" }}>
        <h1 style={{ fontSize: 20, margin: 0 }}>{t("nav.rom")} — {t("rom.title")}</h1>
        <div style={{ display: "flex", gap: 8 }}>
          <button className="btn btn-sm" onClick={handleIdentifySingle} disabled={busy}>
            🔍 {t("rom.actions.identify")}
          </button>
          <button className="btn btn-primary btn-sm" onClick={handleScanDir} disabled={busy}>
            📂 {t("rom.actions.scan")}
          </button>
        </div>
      </div>

      {error && <div className="badge badge-err" style={{ margin: "12px 0", padding: "6px 12px" }}>{error}</div>}
      {statusMsg && <div className="badge badge-ok" style={{ margin: "12px 0", padding: "6px 12px" }}>{statusMsg}</div>}
      {busy && <div className="muted" style={{ margin: "12px 0" }}>{t("rom.status.scanning")}</div>}

      {/* Information Banner */}
      <div className="card" style={{ margin: "14px 0", background: "var(--bg-elevated)" }}>
        <div style={{ fontWeight: 600, fontSize: 13 }}>💡 {t("rom.banner.title")}</div>
        <p className="muted" style={{ fontSize: 12, margin: "4px 0 0" }}>
          {t("rom.banner.intro")}{" "}
          {t("rom.banner.prefix")} <strong>{t("rom.banner.arosLabel")}</strong>{t("rom.banner.suffix")}
        </p>
      </div>

      {/* ROMs Table & Details */}
      <div style={{ display: "grid", gridTemplateColumns: roms.length > 0 ? "1fr 1fr" : "1fr", gap: 16 }}>
        {/* List of Detected ROMs */}
        <section className="card">
          <h2 style={{ fontSize: 15 }}>📚 {t("rom.library.title", { count: roms.length })}</h2>
          {roms.length === 0 ? (
            <p className="muted" style={{ fontSize: 13, padding: "16px 0", textAlign: "center" }}>
              {t("rom.library.empty")}
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
                      <span className={`badge ${checksumBadge(r).tone}`}>
                        {t(checksumBadge(r).phrase.key)}
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
            <h2 style={{ fontSize: 15 }}>🔬 {t("rom.details.title")}</h2>
            <div style={{ display: "flex", flexDirection: "column", gap: 10, marginTop: 10, fontSize: 13 }}>
              <div>
                <span className="muted">{t("rom.details.titleLabel")}</span> <strong>{selectedRom.name}</strong>
              </div>
              <div>
                <span className="muted">{t("rom.details.versionLabel")}</span> {selectedRom.version} ({selectedRom.revision})
              </div>
              <div>
                <span className="muted">{t("rom.details.sizeLabel")}</span>{" "}
                {t("rom.details.sizeValue", { kb: selectedRom.size_bytes / 1024, bytes: selectedRom.size_bytes })}
              </div>
              <div>
                <span className="muted">{t("rom.details.crc32Label")}</span> <code style={{ color: "var(--accent-text)" }}>{selectedRom.crc32}</code>
              </div>
              <div>
                <span className="muted">{t("rom.details.sha256Label")}</span>
                <div style={{ fontSize: 11, wordBreak: "break-all", background: "var(--bg)", padding: 6, borderRadius: 4, marginTop: 2 }}>
                  {selectedRom.sha256}
                </div>
              </div>
              <div>
                <span className="muted">{t("rom.details.modelsLabel")}</span>
                {/* Empty means ART's source named no machine — the Remus
                    database says nothing about a CDTV extended ROM's model —
                    and a bare gap read as a missing feature (ART-138). */}
                {selectedRom.compatible_models.length === 0 ? (
                  <div className="faint" style={{ fontSize: 12, marginTop: 4 }}>
                    {t("rom.details.noModels")}
                  </div>
                ) : (
                  <div style={{ display: "flex", gap: 4, flexWrap: "wrap", marginTop: 4 }}>
                    {selectedRom.compatible_models.map((m) => (
                      <span key={m} className="badge badge-muted">
                        {m}
                      </span>
                    ))}
                  </div>
                )}
              </div>
              {/* A green tick for every Amiga Forever ROM was a false
                  reassurance: stripping the header leaves the image still
                  encrypted, and without the buyer's `rom.key` beside it ART
                  reads nothing at all (ART-128). */}
              {selectedRom.is_cloanto && (
                <div
                  className={
                    selectedRom.key_available ? "badge badge-ok" : "badge badge-warn"
                  }
                >
                  {selectedRom.key_available
                    ? `✓ ${t("rom.details.cloantoDecoded")}`
                    : t("rom.details.cloantoNeedsKey")}
                </div>
              )}
            </div>
          </section>
        )}
      </div>
    </div>
  );
}
