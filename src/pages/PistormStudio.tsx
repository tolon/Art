import { useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { useTranslation } from "react-i18next";

import {
  pistormScan,
  pistormSave,
  type PistormBoard,
  type PistormConfig,
  type PistormProfileMode,
  type RtgResolution,
} from "@/lib/pistorm";
import {
  isFlag,
  isOneOf,
  isTextOrNothing,
  isWholeNumberBetween,
  type Guard,
} from "@/lib/remembered";
import { useRemembered, useRememberedShape } from "@/lib/useRemembered";

const DEFAULT_PISTORM_CONFIG: PistormConfig = {
  board: "a500a2000",
  profile_mode: "workstation",
  fast_ram_mb: 1024,
  rtg_resolution: "res1080p",
  enable_jit: true,
  enable_mmu: true,
  enable_wifi: true,
  enable_sd_storage: true,
  custom_kickstart_path: "kick.rom",
};

/**
 * How a remembered PiStorm config is checked on the way back in.
 *
 * The Fast RAM bound is the hardware's, not a guess: a PiStorm addresses up to
 * 2 GB of Fast RAM, and a remembered `-1` or `1e9` would reach a generated
 * `emu68.cfg` and be handed to a real machine.
 */
const PISTORM_CONFIG_SPEC: { [K in keyof PistormConfig]: Guard<PistormConfig[K]> } = {
  board: isOneOf<PistormBoard>("a500a2000", "a1200lite", "a600"),
  profile_mode: isOneOf<PistormProfileMode>(
    "workstation",
    "balancedwhdload",
    "classiccompat"
  ),
  fast_ram_mb: isWholeNumberBetween(0, 2048),
  rtg_resolution: isOneOf<RtgResolution>(
    "res1080p",
    "res720p",
    "res1024x768",
    "res800x600",
    "disabled"
  ),
  enable_jit: isFlag,
  enable_mmu: isFlag,
  enable_wifi: isFlag,
  enable_sd_storage: isFlag,
  custom_kickstart_path: isTextOrNothing,
};

interface ProfileChoice {
  id: PistormProfileMode;
  emoji: string;
  titleKey: string;
  badgeKey: string;
  badgeType: "ok" | "muted" | "warn";
  descriptionKey: string;
  featureKeys: string[];
}

const PROFILE_CHOICES: ProfileChoice[] = [
  {
    id: "workstation",
    emoji: "⚡",
    titleKey: "pistorm.profile.workstation.title",
    badgeKey: "pistorm.profile.workstation.badge",
    badgeType: "ok",
    descriptionKey: "pistorm.profile.workstation.description",
    featureKeys: [
      "pistorm.profile.workstation.feature1",
      "pistorm.profile.workstation.feature2",
      "pistorm.profile.workstation.feature3",
      "pistorm.profile.workstation.feature4",
    ],
  },
  {
    id: "balancedwhdload",
    emoji: "🕹️",
    titleKey: "pistorm.profile.balanced.title",
    badgeKey: "pistorm.profile.balanced.badge",
    badgeType: "muted",
    descriptionKey: "pistorm.profile.balanced.description",
    featureKeys: [
      "pistorm.profile.balanced.feature1",
      "pistorm.profile.balanced.feature2",
      "pistorm.profile.balanced.feature3",
      "pistorm.profile.balanced.feature4",
    ],
  },
  {
    id: "classiccompat",
    emoji: "🎯",
    titleKey: "pistorm.profile.classic.title",
    badgeKey: "pistorm.profile.classic.badge",
    badgeType: "warn",
    descriptionKey: "pistorm.profile.classic.description",
    featureKeys: [
      "pistorm.profile.classic.feature1",
      "pistorm.profile.classic.feature2",
      "pistorm.profile.classic.feature3",
      "pistorm.profile.classic.feature4",
    ],
  },
];

export function PistormStudio() {
  const { t } = useTranslation();

  // The board, the profile and every toggle below are the user's machine
  // described — the single most expensive thing in ART to re-enter, and the
  // thing a user comes back to across sessions while building one image. So it
  // is remembered field by field (`@/lib/useRemembered`): a config that gains
  // an option in a later ART keeps everything already chosen.
  const [sdPath, setSdPath] = useRemembered<string | null>(
    "pistorm.sdPath",
    isTextOrNothing,
    null
  );
  const [config, applyConfig] = useRememberedShape<PistormConfig>(
    "pistorm.config",
    PISTORM_CONFIG_SPEC,
    DEFAULT_PISTORM_CONFIG
  );
  const setConfig = applyConfig;

  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [statusMsg, setStatusMsg] = useState<string | null>(null);

  async function handleSelectSd() {
    const sel = await open({
      directory: true,
      multiple: false,
      title: t("pistorm.selectSdTitle"),
    });
    if (typeof sel !== "string") return;

    setBusy(true);
    setError(null);
    setStatusMsg(null);
    try {
      const scanned = await pistormScan(sel);
      setSdPath(sel);
      setConfig(scanned.detected_config);
      const firmware = scanned.has_emu68_img
        ? t("pistorm.scan.firmwareFound")
        : t("pistorm.scan.firmwareMissing");
      const kickstart = scanned.has_kickstart
        ? t("pistorm.scan.kickstartFound", { name: scanned.kickstart_name })
        : t("pistorm.scan.kickstartMissing");
      setStatusMsg(t("pistorm.scan.summary", { firmware, kickstart }));
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  function handleProfileSelect(mode: PistormProfileMode) {
    if (mode === "workstation") {
      setConfig({
        ...config,
        profile_mode: mode,
        enable_jit: true,
        enable_mmu: true,
        fast_ram_mb: 1024,
        rtg_resolution: "res1080p",
      });
    } else if (mode === "balancedwhdload") {
      setConfig({
        ...config,
        profile_mode: mode,
        enable_jit: true,
        enable_mmu: true,
        fast_ram_mb: 512,
        rtg_resolution: "res720p",
      });
    } else {
      setConfig({
        ...config,
        profile_mode: mode,
        enable_jit: false,
        enable_mmu: false,
        fast_ram_mb: 256,
        rtg_resolution: "disabled",
      });
    }
  }

  async function handleSave() {
    if (!sdPath) return;
    setBusy(true);
    setError(null);
    setStatusMsg(null);
    try {
      await pistormSave(sdPath, config);
      setStatusMsg(t("pistorm.msg.saved"));
    } catch (e) {
      setError(t("pistorm.msg.saveFailed", { error: String(e) }));
    } finally {
      setBusy(false);
    }
  }

  return (
    <div>
      <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center" }}>
        <h1 style={{ fontSize: 20, margin: 0 }}>{t("nav.pistorm")} — {t("pistorm.title")}</h1>
        <div style={{ display: "flex", gap: 8 }}>
          <button className="btn btn-sm" onClick={handleSelectSd} disabled={busy}>
            📂 {t("pistorm.selectSd")}
          </button>
          {sdPath && (
            <button className="btn btn-sm btn-primary" onClick={handleSave} disabled={busy}>
              💾 {t("pistorm.saveSync")}
            </button>
          )}
        </div>
      </div>

      {sdPath && (
        <div style={{ margin: "8px 0 12px", fontSize: 12 }}>
          <span className="muted">{t("pistorm.sdCardLabel")}</span>{" "}
          <strong style={{ wordBreak: "break-all" }}>{sdPath}</strong>
        </div>
      )}

      {error && <div className="badge badge-err" style={{ marginBottom: 12, padding: "6px 12px" }}>{error}</div>}
      {statusMsg && <div className="badge badge-ok" style={{ marginBottom: 12, padding: "6px 12px" }}>{statusMsg}</div>}
      {busy && <div className="muted" style={{ marginBottom: 12 }}>{t("pistorm.working")}</div>}

      {/* Profile Modes: Parametric Workstation vs Gaming Explanations */}
      <section className="card" style={{ marginTop: 12 }}>
        <h2 style={{ fontSize: 15 }}>🎯 {t("pistorm.profilesHeading")}</h2>
        <p className="muted" style={{ fontSize: 12, margin: "2px 0 12px" }}>
          {t("pistorm.profilesIntro")}
        </p>

        <div style={{ display: "grid", gridTemplateColumns: "repeat(auto-fit, minmax(280px, 1fr))", gap: 10 }}>
          {PROFILE_CHOICES.map((p) => {
            const isSel = config.profile_mode === p.id;
            return (
              <div
                key={p.id}
                onClick={() => handleProfileSelect(p.id)}
                style={{
                  padding: "12px",
                  borderRadius: "var(--radius-sm)",
                  border: isSel ? "1px solid var(--accent)" : "1px solid var(--border)",
                  background: isSel ? "var(--bg-hover)" : "var(--bg)",
                  cursor: "pointer",
                  display: "flex",
                  flexDirection: "column",
                  justifyContent: "space-between",
                }}
              >
                <div>
                  <div style={{ display: "flex", justifyContent: "space-between", alignItems: "flex-start", gap: 6 }}>
                    <strong style={{ fontSize: 14 }}>{p.emoji} {t(p.titleKey)}</strong>
                    <span className={`badge badge-${p.badgeType}`} style={{ fontSize: 10 }}>
                      {p.badgeType === "ok" ? "⭐ " : ""}
                      {t(p.badgeKey)}
                    </span>
                  </div>
                  <p className="muted" style={{ fontSize: 12, margin: "6px 0 10px" }}>
                    {t(p.descriptionKey)}
                  </p>
                </div>
                <ul style={{ margin: 0, paddingLeft: 18, fontSize: 11, color: "var(--text-muted)" }}>
                  {p.featureKeys.map((key) => (
                    <li key={key}>{t(key)}</li>
                  ))}
                </ul>
              </div>
            );
          })}
        </div>
      </section>

      {/* Hardware & Parameter Tuning Grid */}
      <div style={{ display: "grid", gridTemplateColumns: "1fr 1fr", gap: 16, marginTop: 16 }}>
        {/* Left: Hardware & RTG Settings */}
        <section className="card" style={{ display: "flex", flexDirection: "column", gap: 12 }}>
          <h2 style={{ fontSize: 15 }}>🖥️ {t("pistorm.hardwareHeading")}</h2>

          {/* PiStorm Board Model */}
          <div>
            <label className="muted" style={{ fontSize: 12, display: "block", marginBottom: 4 }}>
              {t("pistorm.boardLabel")}
            </label>
            <select
              value={config.board}
              onChange={(e) => setConfig({ ...config, board: e.target.value as PistormBoard })}
              style={{ width: "100%", padding: "6px 8px", background: "var(--bg)", color: "var(--text)", border: "1px solid var(--border)", borderRadius: 4 }}
            >
              <option value="a500a2000">{t("pistorm.board.a500a2000")}</option>
              <option value="a1200lite">{t("pistorm.board.a1200lite")}</option>
              <option value="a600">{t("pistorm.board.a600")}</option>
            </select>
          </div>

          {/* RTG HDMI Video Output */}
          <div>
            <label className="muted" style={{ fontSize: 12, display: "block", marginBottom: 4 }}>
              {t("pistorm.rtgLabel")}
            </label>
            <select
              value={config.rtg_resolution}
              onChange={(e) => setConfig({ ...config, rtg_resolution: e.target.value as RtgResolution })}
              style={{ width: "100%", padding: "6px 8px", background: "var(--bg)", color: "var(--text)", border: "1px solid var(--border)", borderRadius: 4 }}
            >
              <option value="res1080p">{t("pistorm.rtg.res1080p")}</option>
              <option value="res720p">{t("pistorm.rtg.res720p")}</option>
              <option value="res1024x768">{t("pistorm.rtg.res1024")}</option>
              <option value="res800x600">{t("pistorm.rtg.res800")}</option>
              <option value="disabled">{t("pistorm.rtg.disabled")}</option>
            </select>
          </div>

          {/* Kickstart ROM */}
          <div>
            <label className="muted" style={{ fontSize: 12, display: "block", marginBottom: 4 }}>
              {t("pistorm.kickstartLabel")}
            </label>
            <input
              type="text"
              value={config.custom_kickstart_path ?? "kick.rom"}
              onChange={(e) => setConfig({ ...config, custom_kickstart_path: e.target.value })}
              placeholder="kick.rom"
              style={{ width: "100%", padding: "6px 8px", background: "var(--bg)", color: "var(--text)", border: "1px solid var(--border)", borderRadius: 4 }}
            />
          </div>
        </section>

        {/* Right: CPU JIT & Fast RAM Parameters */}
        <section className="card" style={{ display: "flex", flexDirection: "column", gap: 12 }}>
          <h2 style={{ fontSize: 15 }}>⚡ {t("pistorm.performanceHeading")}</h2>

          {/* Fast RAM Slider */}
          <div>
            <div style={{ display: "flex", justifyContent: "space-between", fontSize: 12 }}>
              <span className="muted">{t("pistorm.fastRamLabel")}</span>
              <strong>{config.fast_ram_mb >= 1024 ? `${config.fast_ram_mb / 1024} GB` : `${config.fast_ram_mb} MB`}</strong>
            </div>
            <input
              type="range"
              min="128"
              max="2048"
              step="128"
              value={config.fast_ram_mb}
              onChange={(e) => setConfig({ ...config, fast_ram_mb: Number(e.target.value) })}
              style={{ width: "100%", marginTop: 4 }}
            />
            <div className="faint" style={{ fontSize: 10, marginTop: 2 }}>
              {t("pistorm.fastRamHint")}
            </div>
          </div>

          {/* JIT Recompiler */}
          <label style={{ display: "flex", alignItems: "center", gap: 8, fontSize: 13, cursor: "pointer" }}>
            <input
              type="checkbox"
              checked={config.enable_jit}
              onChange={(e) => setConfig({ ...config, enable_jit: e.target.checked })}
            />
            <div>
              <strong>{t("pistorm.jitLabel")}</strong>
              <div className="muted" style={{ fontSize: 11 }}>
                {t("pistorm.jitHint")}
              </div>
            </div>
          </label>

          {/* MMU */}
          <label style={{ display: "flex", alignItems: "center", gap: 8, fontSize: 13, cursor: "pointer" }}>
            <input
              type="checkbox"
              checked={config.enable_mmu}
              onChange={(e) => setConfig({ ...config, enable_mmu: e.target.checked })}
            />
            <div>
              <strong>{t("pistorm.mmuLabel")}</strong>
              <div className="muted" style={{ fontSize: 11 }}>
                {t("pistorm.mmuHint")}
              </div>
            </div>
          </label>

          {/* SD Card Direct Storage */}
          <label style={{ display: "flex", alignItems: "center", gap: 8, fontSize: 13, cursor: "pointer" }}>
            <input
              type="checkbox"
              checked={config.enable_sd_storage}
              onChange={(e) => setConfig({ ...config, enable_sd_storage: e.target.checked })}
            />
            <div>
              <strong>{t("pistorm.sdStorageLabel")}</strong>
              <div className="muted" style={{ fontSize: 11 }}>
                {t("pistorm.sdStorageHint")}
              </div>
            </div>
          </label>
        </section>
      </div>
    </div>
  );
}
