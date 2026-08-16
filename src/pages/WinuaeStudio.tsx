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
import { isFlag, isTextOrNothing } from "@/lib/remembered";
import { useRemembered } from "@/lib/useRemembered";
import { useOpenObject } from "@/stores/openObjectStore";

export function WinuaeStudio() {
  const { t } = useTranslation();
  const location = useLocation();

  const [install, setInstall] = useState<WinUaeInstallation | null>(null);
  const [profiles, setProfiles] = useState<AmigaProfile[]>([]);
  const [selectedProfile, setSelectedProfile] = useState<AmigaProfile | null>(null);
  const [customExePath, setCustomExePath] = useState<string>("");

  /**
   * Which machine the user runs, by id.
   *
   * The id rather than the profile: the profiles themselves come from Rust and
   * may gain fields or change defaults between versions, and a whole profile
   * frozen into `settings.json` would go stale silently. An id that no longer
   * matches simply falls back to the first profile.
   */
  const [profileId, setProfileId] = useRemembered<string | null>(
    "winuae.profileId",
    isTextOrNothing,
    null
  );

  // Attached media. The ROM and the AROS choice are settings — the same
  // machine, launched again. The disk and the hard disk are what is being
  // worked on right now, and a path frozen into `settings.json` could name a
  // file deleted between two runs, which is worse than an empty slot.
  //
  // `useOpenObject` is neither of those: it holds them for **this run only**
  // (ART-085), so stepping over to the ADF studio to check something and
  // coming back finds the machine still loaded, while a fresh launch of ART
  // still starts with empty slots.
  const [df0Path, setDf0Path] = useOpenObject("winuae-floppy");
  const [hdfPath, setHdfPath] = useOpenObject("winuae-harddisk");
  const [kickstartPath, setKickstartPath] = useRemembered<string | null>(
    "winuae.kickstartPath",
    isTextOrNothing,
    null
  );
  const [kickstartInfo, setKickstartInfo] = useState<RomInfo | null>(null);
  const [useAros, setUseAros] = useRemembered("winuae.useAros", isFlag, false);

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
        // The machine the user last chose, by id. An id from an older ART that
        // no longer names a profile falls back to the first rather than
        // leaving the screen with nothing selected.
        setSelectedProfile(profList.find((p) => p.id === profileId) ?? profList[0]);
      }
    } catch (e) {
      setError(String(e));
    }
  }

  async function handleBrowseWinUae() {
    const sel = await open({
      multiple: false,
      filters: [{ name: "WinUAE Executable", extensions: ["exe"] }],
      title: t("winuae.locateDialogTitle"),
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
      title: t("winuae.selectFloppyTitle"),
    });
    if (typeof sel === "string") {
      setDf0Path(sel);
    }
  }

  async function handleBrowseHdf() {
    const sel = await open({
      multiple: false,
      filters: [{ name: "Hard Disk File (HDF)", extensions: ["hdf", "img"] }],
      title: t("winuae.selectHdfTitle"),
    });
    if (typeof sel === "string") {
      setHdfPath(sel);
    }
  }

  async function handleBrowseKickstart() {
    const sel = await open({
      multiple: false,
      filters: [{ name: "Kickstart ROM", extensions: ["rom", "bin", "a500", "a1200"] }],
      title: t("winuae.selectRomTitle"),
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
      setError(t("winuae.err.notFound"));
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
      setStatusMsg(t("winuae.msg.launched", { pid, name: selectedProfile.name }));
    } catch (e) {
      setError(t("winuae.err.launchFailed", { error: String(e) }));
    } finally {
      setBusy(false);
    }
  }

  return (
    <div>
      <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center" }}>
        <h1 style={{ fontSize: 20, margin: 0 }}>{t("nav.winuae")} — {t("winuae.title")}</h1>
        {install && (
          <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
            <span className={`badge ${install.found ? "badge-ok" : "badge-warn"}`}>
              {install.found
                ? install.version
                  ? t("winuae.status.foundVersion", { version: install.version })
                  : t("winuae.status.foundGeneric")
                : t("winuae.status.notFound")}
            </span>
            <button className="btn btn-sm" onClick={handleBrowseWinUae}>
              ⚙️ {t("winuae.locateButton")}
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
          <h2 style={{ fontSize: 16 }}>🕹️ {t("winuae.profilesHeading")}</h2>
          <p className="muted" style={{ fontSize: 12, margin: "4px 0 12px" }}>
            {t("winuae.profilesIntro")}
          </p>

          <div style={{ display: "flex", flexDirection: "column", gap: 8 }}>
            {profiles.map((p) => {
              const isSelected = selectedProfile?.id === p.id;
              return (
                <div
                  key={p.id}
                  onClick={() => {
                    setSelectedProfile(p);
                    setProfileId(p.id);
                  }}
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
          <h2 style={{ fontSize: 16 }}>🚀 {t("winuae.launcherHeading")}</h2>

          {selectedProfile && (
            <div style={{ background: "var(--bg)", padding: 10, borderRadius: "var(--radius-sm)" }}>
              <div style={{ fontSize: 12, color: "var(--text-muted)" }}>{t("winuae.targetProfileLabel")}</div>
              <strong style={{ fontSize: 14 }}>{selectedProfile.name}</strong>
            </div>
          )}

          {/* Floppy DF0: */}
          <div>
            <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center" }}>
              <label className="muted" style={{ fontSize: 12 }}>{t("winuae.df0Label")}</label>
              <button className="btn btn-sm" onClick={handleBrowseDf0}>
                {df0Path ? t("winuae.changeAdf") : t("winuae.insertAdf")}
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
              {df0Path ? `💾 ${df0Path}` : t("winuae.noDiskInserted")}
            </div>
          </div>

          {/* Hard Disk HDF */}
          <div>
            <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center" }}>
              <label className="muted" style={{ fontSize: 12 }}>{t("winuae.hdfLabel")}</label>
              <button className="btn btn-sm" onClick={handleBrowseHdf}>
                {hdfPath ? t("winuae.changeHdf") : t("winuae.attachHdf")}
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
              {hdfPath ? `💽 ${hdfPath}` : t("winuae.noHdfAttached")}
            </div>
          </div>

          {/* Kickstart ROM */}
          <div>
            <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center" }}>
              <label className="muted" style={{ fontSize: 12 }}>{t("winuae.kickstartLabel")}</label>
              <button className="btn btn-sm" onClick={handleBrowseKickstart}>
                {kickstartPath ? t("winuae.changeRom") : t("winuae.selectRom")}
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
                <span className="badge badge-ok">{t("winuae.arosBuiltin")}</span>
              ) : kickstartPath ? (
                <span>
                  🔑 {kickstartInfo ? kickstartInfo.name : kickstartPath}
                </span>
              ) : (
                <span className="muted">
                  {t("winuae.defaultKickstart", { version: selectedProfile?.kickstart_version ?? "1.3" })}
                </span>
              )}
            </div>
          </div>

          {/* Floppy Speed Slider */}
          {selectedProfile && (
            <div>
              <label className="muted" style={{ fontSize: 12, display: "block", marginBottom: 4 }}>
                {t("winuae.floppySpeedLabel", {
                  speed:
                    selectedProfile.floppy.speed_percent === 100
                      ? t("winuae.floppySpeed.accurate")
                      : t("winuae.floppySpeed.fast", { n: selectedProfile.floppy.speed_percent }),
                })}
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
            🚀 {t("common.launchInWinuae")}
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
            <h3 style={{ margin: "0 0 8px" }}>🔑 {t("winuae.modal.title")}</h3>
            <p className="muted" style={{ fontSize: 13, lineHeight: 1.5, margin: "0 0 16px" }}>
              {t("winuae.modal.body", { name: selectedProfile?.name ?? "" })}
            </p>

            <div style={{ display: "flex", flexDirection: "column", gap: 10 }}>
              <button
                className="btn btn-primary"
                style={{ padding: "10px", justifyContent: "center" }}
                onClick={handleBrowseKickstart}
              >
                📁 {t("winuae.modal.selectRom", { version: selectedProfile?.kickstart_version })}
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
                🚀 {t("winuae.modal.continueAros")}
              </button>

              <button
                className="btn btn-sm"
                style={{ marginTop: 8 }}
                onClick={() => setShowRomPromptModal(false)}
              >
                {t("common.cancel")}
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
