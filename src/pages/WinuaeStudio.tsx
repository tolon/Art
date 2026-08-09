import { useState, useEffect } from "react";
import { useLocation } from "react-router-dom";
import { open } from "@tauri-apps/plugin-dialog";
import { useTranslation } from "react-i18next";

import {
  winuaeDetect,
  winuaeListProfiles,
  winuaeLaunch,
  type AmigaProfile,
  type WinUaeInstallation,
  type LaunchMedia,
} from "@/lib/winuae";
import { romIdentify, type RomInfo } from "@/lib/rom";

export function WinuaeStudio() {
  const { t } = useTranslation();
  const location = useLocation();

  const [install, setInstall] = useState<WinUaeInstallation | null>(null);
  const [profiles, setProfiles] = useState<AmigaProfile[]>([]);
  const [selectedProfile, setSelectedProfile] = useState<AmigaProfile | null>(null);
  const [customExePath, setCustomExePath] = useState<string>("");

  // Attached media
  const [df0Path, setDf0Path] = useState<string | null>(null);
  const [hdfPath, setHdfPath] = useState<string | null>(null);
  const [kickstartPath, setKickstartPath] = useState<string | null>(null);
  const [kickstartInfo, setKickstartInfo] = useState<RomInfo | null>(null);
  const [useAros, setUseAros] = useState<boolean>(false);

  // Missing ROM confirmation modal
  const [showRomPromptModal, setShowRomPromptModal] = useState<boolean>(false);

  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [statusMsg, setStatusMsg] = useState<string | null>(null);

  // Initialize and check navigation state
  useEffect(() => {
    void init();
    const navState = location.state as { path?: string; kind?: string } | undefined;
    if (navState?.path) {
      if (navState.path.toLowerCase().endsWith(".adf")) {
        setDf0Path(navState.path);
      } else if (navState.path.toLowerCase().endsWith(".hdf")) {
        setHdfPath(navState.path);
      }
    }
  }, [location.state]);

  async function init() {
    try {
      const [inst, profList] = await Promise.all([
        winuaeDetect(),
        winuaeListProfiles(),
      ]);
      setInstall(inst);
      setProfiles(profList);
      if (profList.length > 0) {
        setSelectedProfile(profList[0]);
      }
    } catch (e) {
      setError(String(e));
    }
  }

  async function handleBrowseWinUae() {
    const sel = await open({
      multiple: false,
      filters: [{ name: "WinUAE Executable", extensions: ["exe"] }],
      title: "Locate winuae64.exe or winuae.exe",
    });
    if (typeof sel === "string") {
      setCustomExePath(sel);
      const inst = await winuaeDetect(sel);
      setInstall(inst);
    }
  }

  async function handleBrowseDf0() {
    const sel = await open({
      multiple: false,
      filters: [{ name: "Floppy Disk (ADF)", extensions: ["adf", "adz"] }],
      title: "Select Floppy Disk for DF0:",
    });
    if (typeof sel === "string") {
      setDf0Path(sel);
    }
  }

  async function handleBrowseHdf() {
    const sel = await open({
      multiple: false,
      filters: [{ name: "Hard Disk File (HDF)", extensions: ["hdf", "img"] }],
      title: "Select Hard Disk File (HDF)",
    });
    if (typeof sel === "string") {
      setHdfPath(sel);
    }
  }

  async function handleBrowseKickstart() {
    const sel = await open({
      multiple: false,
      filters: [{ name: "Kickstart ROM", extensions: ["rom", "bin", "a500", "a1200"] }],
      title: "Select Kickstart ROM File",
    });
    if (typeof sel === "string") {
      setKickstartPath(sel);
      setUseAros(false);
      try {
        const info = await romIdentify(sel);
        setKickstartInfo(info);
      } catch {
        setKickstartInfo(null);
      }
      setShowRomPromptModal(false);
    }
  }

  async function doLaunch(forceAros = false) {
    if (!selectedProfile) return;
    if (!install || !install.found) {
      setError("WinUAE executable was not found. Please specify its path.");
      return;
    }

    const effectiveAros = forceAros || useAros;
    // Check if Kickstart ROM is missing and AROS is not selected
    if (!effectiveAros && !kickstartPath && !selectedProfile.custom_rom_path) {
      setShowRomPromptModal(true);
      return;
    }

    setBusy(true);
    setError(null);
    setStatusMsg(null);
    try {
      const media: LaunchMedia = {
        floppy_paths: df0Path ? [df0Path] : [],
        hardfile_paths: hdfPath ? [hdfPath] : [],
        kickstart_path: kickstartPath,
        use_aros: effectiveAros,
      };

      const pid = await winuaeLaunch(
        selectedProfile,
        media,
        customExePath || install.executable_path || undefined
      );
      setStatusMsg(`🚀 WinUAE launched successfully (PID: ${pid}) with ${selectedProfile.name}`);
    } catch (e) {
      setError(`Failed to launch WinUAE: ${String(e)}`);
    } finally {
      setBusy(false);
    }
  }

  return (
    <div>
      <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center" }}>
        <h1 style={{ fontSize: 20, margin: 0 }}>{t("nav.winuae")} — WinUAE Studio</h1>
        {install && (
          <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
            <span className={`badge ${install.found ? "badge-ok" : "badge-warn"}`}>
              {install.found ? `✓ ${install.version ?? "WinUAE Found"}` : "⚠️ WinUAE Not Found"}
            </span>
            <button className="btn btn-sm" onClick={handleBrowseWinUae}>
              ⚙️ Locate WinUAE…
            </button>
          </div>
        )}
      </div>

      {error && <div className="badge badge-err" style={{ margin: "12px 0", padding: "6px 12px" }}>{error}</div>}
      {statusMsg && <div className="badge badge-ok" style={{ margin: "12px 0", padding: "6px 12px" }}>{statusMsg}</div>}

      {/* Main Grid: Profiles & Runner */}
      <div style={{ display: "grid", gridTemplateColumns: "1fr 1fr", gap: 16, marginTop: 16 }}>
        {/* Left: Amiga Machine Profiles Catalog */}
        <section className="card">
          <h2 style={{ fontSize: 16 }}>🕹️ Amiga Hardware Profiles</h2>
          <p className="muted" style={{ fontSize: 12, margin: "4px 0 12px" }}>
            Choose an accurate hardware configuration preset:
          </p>

          <div style={{ display: "flex", flexDirection: "column", gap: 8 }}>
            {profiles.map((p) => {
              const isSelected = selectedProfile?.id === p.id;
              return (
                <div
                  key={p.id}
                  onClick={() => setSelectedProfile(p)}
                  style={{
                    padding: "10px 12px",
                    borderRadius: "var(--radius-sm)",
                    background: isSelected ? "var(--bg-hover)" : "var(--bg)",
                    border: isSelected ? "1px solid var(--accent)" : "1px solid var(--border)",
                    cursor: "pointer",
                    transition: "all 0.12s",
                  }}
                >
                  <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center" }}>
                    <strong>{p.name}</strong>
                    <span className="badge badge-muted">{p.chipset.toUpperCase()}</span>
                  </div>
                  <div className="muted" style={{ fontSize: 12, marginTop: 4 }}>
                    {p.description}
                  </div>
                  <div className="faint" style={{ fontSize: 11, marginTop: 4 }}>
                    CPU: {p.cpu.toUpperCase()} · Chip: {p.memory.chip_kb} KB · Fast: {p.memory.fast_mb} MB · Kickstart {p.kickstart_version}
                  </div>
                </div>
              );
            })}
          </div>
        </section>

        {/* Right: Media Attachments & 1-Click Launch */}
        <section className="card" style={{ display: "flex", flexDirection: "column", gap: 14 }}>
          <h2 style={{ fontSize: 16 }}>🚀 Emulation Launcher</h2>

          {selectedProfile && (
            <div style={{ background: "var(--bg)", padding: 10, borderRadius: "var(--radius-sm)" }}>
              <div style={{ fontSize: 12, color: "var(--text-muted)" }}>Target Profile:</div>
              <strong style={{ fontSize: 14 }}>{selectedProfile.name}</strong>
            </div>
          )}

          {/* Floppy DF0: */}
          <div>
            <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center" }}>
              <label className="muted" style={{ fontSize: 12 }}>Floppy Drive DF0:</label>
              <button className="btn btn-sm" onClick={handleBrowseDf0}>
                {df0Path ? "Change ADF…" : "Insert ADF…"}
              </button>
            </div>
            <div
              style={{
                marginTop: 4,
                padding: "6px 10px",
                background: "var(--bg)",
                border: "1px solid var(--border)",
                borderRadius: 4,
                fontSize: 12,
                wordBreak: "break-all",
              }}
            >
              {df0Path ? `💾 ${df0Path}` : "(No disk inserted)"}
            </div>
          </div>

          {/* Hard Disk HDF */}
          <div>
            <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center" }}>
              <label className="muted" style={{ fontSize: 12 }}>Hard Disk (HDF):</label>
              <button className="btn btn-sm" onClick={handleBrowseHdf}>
                {hdfPath ? "Change HDF…" : "Attach HDF…"}
              </button>
            </div>
            <div
              style={{
                marginTop: 4,
                padding: "6px 10px",
                background: "var(--bg)",
                border: "1px solid var(--border)",
                borderRadius: 4,
                fontSize: 12,
                wordBreak: "break-all",
              }}
            >
              {hdfPath ? `💽 ${hdfPath}` : "(No hard disk attached)"}
            </div>
          </div>

          {/* Kickstart ROM */}
          <div>
            <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center" }}>
              <label className="muted" style={{ fontSize: 12 }}>Kickstart ROM:</label>
              <button className="btn btn-sm" onClick={handleBrowseKickstart}>
                {kickstartPath ? "Change ROM…" : "Select ROM…"}
              </button>
            </div>
            <div
              style={{
                marginTop: 4,
                padding: "6px 10px",
                background: "var(--bg)",
                border: "1px solid var(--border)",
                borderRadius: 4,
                fontSize: 12,
              }}
            >
              {useAros ? (
                <span className="badge badge-ok">AROS Open-Source Replacement (Built-in)</span>
              ) : kickstartPath ? (
                <span>
                  🔑 {kickstartInfo ? kickstartInfo.name : kickstartPath}
                </span>
              ) : (
                <span className="muted">Default: Kickstart {selectedProfile?.kickstart_version ?? "1.3"} (Will prompt if missing)</span>
              )}
            </div>
          </div>

          {/* Floppy Speed Slider */}
          {selectedProfile && (
            <div>
              <label className="muted" style={{ fontSize: 12, display: "block", marginBottom: 4 }}>
                Floppy Emulation Speed: {selectedProfile.floppy.speed_percent === 100 ? "100% (Accurate)" : `${selectedProfile.floppy.speed_percent}% (Fast)`}
              </label>
              <input
                type="range"
                min="100"
                max="800"
                step="100"
                value={selectedProfile.floppy.speed_percent}
                onChange={(e) => {
                  setSelectedProfile({
                    ...selectedProfile,
                    floppy: { ...selectedProfile.floppy, speed_percent: Number(e.target.value) },
                  });
                }}
                style={{ width: "100%" }}
              />
            </div>
          )}

          {/* Big Launch Button */}
          <button
            className="btn btn-primary"
            style={{ padding: "12px", fontSize: 15, justifyContent: "center", marginTop: "auto" }}
            onClick={() => doLaunch(false)}
            disabled={busy || !install?.found}
          >
            🚀 Launch in WinUAE
          </button>
        </section>
      </div>

      {/* Modal: Missing Kickstart ROM Prompt */}
      {showRomPromptModal && (
        <div
          style={{
            position: "fixed",
            inset: 0,
            background: "rgba(0,0,0,0.65)",
            display: "flex",
            alignItems: "center",
            justifyContent: "center",
            zIndex: 100,
          }}
        >
          <div className="card" style={{ width: 440, maxWidth: "90vw" }}>
            <h3 style={{ margin: "0 0 8px" }}>🔑 Kickstart ROM Required</h3>
            <p className="muted" style={{ fontSize: 13, lineHeight: 1.5, margin: "0 0 16px" }}>
              To emulate <strong>{selectedProfile?.name}</strong>, a Kickstart ROM is needed.
              Commercial Kickstarts are copyrighted and not distributed with ART.
            </p>

            <div style={{ display: "flex", flexDirection: "column", gap: 10 }}>
              <button
                className="btn btn-primary"
                style={{ padding: "10px", justifyContent: "center" }}
                onClick={handleBrowseKickstart}
              >
                📁 Select Kickstart ROM File ({selectedProfile?.kickstart_version})…
              </button>

              <button
                className="btn"
                style={{ padding: "10px", justifyContent: "center" }}
                onClick={() => {
                  setUseAros(true);
                  setShowRomPromptModal(false);
                  void doLaunch(true);
                }}
              >
                🚀 Continue with Open-Source AROS ROM
              </button>

              <button
                className="btn btn-sm"
                style={{ marginTop: 8 }}
                onClick={() => setShowRomPromptModal(false)}
              >
                Cancel
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
