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

interface ProfileChoice {
  id: PistormProfileMode;
  title: string;
  badge: string;
  badgeType: "ok" | "muted" | "warn";
  description: string;
  features: string[];
}

const PROFILE_CHOICES: ProfileChoice[] = [
  {
    id: "workstation",
    title: "⚡ AmigaOS Ultra Workstation",
    badge: "⭐ Recommended for AmigaOS & Productivity",
    badgeType: "ok",
    description: "Maximum JIT performance (~800+ MIPS) with 1080p TrueColor RTG desktop. Instant Workbench loading, web browsing, raytracing, and high productivity.",
    features: [
      "~800+ MIPS CPU speed (faster than 68060)",
      "1080p 32-bit TrueColor RTG HDMI output",
      "1 GB to 2 GB Fast RAM for multitasking",
      "High-speed emu68-sd.device disk access (20+ MB/s)",
    ],
  },
  {
    id: "balancedwhdload",
    title: "🕹️ Balanced Daily & WHDLoad Gaming",
    badge: "Optimal Daily Driver & Games",
    badgeType: "muted",
    description: "High JIT performance with WHDLoad MMU and cache flush optimizations. RTG desktop with automatic native OCS/ECS/AGA switching for games.",
    features: [
      "99% WHDLoad game compatibility",
      "Automatic screen-mode switching (RTG ↔ PAL/NTSC)",
      "512 MB Fast RAM allocation",
      "WHDLoad MMU-safe cache flushes",
    ],
  },
  {
    id: "classiccompat",
    title: "🎯 Classic & Demoscene Compatibility",
    badge: "Cycle-Exact & Demos",
    badgeType: "warn",
    description: "Conservative JIT execution for timing-sensitive demoscene productions and floppy-based legacy software.",
    features: [
      "Strict timing preservation",
      "128 MB to 256 MB Fast RAM footprint",
      "Native 15 kHz Amiga video pass-through priority",
      "Maximum legacy software stability",
    ],
  },
];

export function PistormStudio() {
  const { t } = useTranslation();

  const [sdPath, setSdPath] = useState<string | null>(null);
  const [config, setConfig] = useState<PistormConfig>({
    board: "a500a2000",
    profile_mode: "workstation",
    fast_ram_mb: 1024,
    rtg_resolution: "res1080p",
    enable_jit: true,
    enable_mmu: true,
    enable_wifi: true,
    enable_sd_storage: true,
    custom_kickstart_path: "kick.rom",
  });

  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [statusMsg, setStatusMsg] = useState<string | null>(null);

  async function handleSelectSd() {
    const sel = await open({
      directory: true,
      multiple: false,
      title: "Select PiStorm / Emu68 MicroSD Card Root Folder",
    });
    if (typeof sel !== "string") return;

    setBusy(true);
    setError(null);
    setStatusMsg(null);
    try {
      const scanned = await pistormScan(sel);
      setSdPath(sel);
      setConfig(scanned.detected_config);
      setStatusMsg(
        `Scanned MicroSD: ${scanned.has_emu68_img ? "✓ Emu68 Firmware Found" : "⚠️ Emu68 kernel missing"}, ${
          scanned.has_kickstart ? `✓ Kickstart (${scanned.kickstart_name})` : "⚠️ Kickstart ROM missing"
        }`
      );
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
      setStatusMsg("✓ Successfully saved PiStorm config.txt and cmdline.txt to MicroSD card!");
    } catch (e) {
      setError(`Failed to save: ${String(e)}`);
    } finally {
      setBusy(false);
    }
  }

  return (
    <div>
      <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center" }}>
        <h1 style={{ fontSize: 20, margin: 0 }}>{t("nav.pistorm")} — PiStorm & Emu68 Studio</h1>
        <div style={{ display: "flex", gap: 8 }}>
          <button className="btn btn-sm" onClick={handleSelectSd} disabled={busy}>
            📂 Select MicroSD Card…
          </button>
          {sdPath && (
            <button className="btn btn-sm btn-primary" onClick={handleSave} disabled={busy}>
              💾 Save & Sync PiStorm SD
            </button>
          )}
        </div>
      </div>

      {sdPath && (
        <div style={{ margin: "8px 0 12px", fontSize: 12 }}>
          <span className="muted">MicroSD Card:</span>{" "}
          <strong style={{ wordBreak: "break-all" }}>{sdPath}</strong>
        </div>
      )}

      {error && <div className="badge badge-err" style={{ marginBottom: 12, padding: "6px 12px" }}>{error}</div>}
      {statusMsg && <div className="badge badge-ok" style={{ marginBottom: 12, padding: "6px 12px" }}>{statusMsg}</div>}
      {busy && <div className="muted" style={{ marginBottom: 12 }}>Working on PiStorm configuration…</div>}

      {/* Profile Modes: Parametric Workstation vs Gaming Explanations */}
      <section className="card" style={{ marginTop: 12 }}>
        <h2 style={{ fontSize: 15 }}>🎯 Parametric Usage Profiles</h2>
        <p className="muted" style={{ fontSize: 12, margin: "2px 0 12px" }}>
          PiStorm transforms your Amiga into a high-performance workstation while preserving custom chipset compatibility for games:
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
                    <strong style={{ fontSize: 14 }}>{p.title}</strong>
                    <span className={`badge badge-${p.badgeType}`} style={{ fontSize: 10 }}>
                      {p.badge}
                    </span>
                  </div>
                  <p className="muted" style={{ fontSize: 12, margin: "6px 0 10px" }}>
                    {p.description}
                  </p>
                </div>
                <ul style={{ margin: 0, paddingLeft: 18, fontSize: 11, color: "var(--text-muted)" }}>
                  {p.features.map((f, i) => (
                    <li key={i}>{f}</li>
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
          <h2 style={{ fontSize: 15 }}>🖥️ Hardware & RTG Display Setup</h2>

          {/* PiStorm Board Model */}
          <div>
            <label className="muted" style={{ fontSize: 12, display: "block", marginBottom: 4 }}>
              PiStorm Hardware Board:
            </label>
            <select
              value={config.board}
              onChange={(e) => setConfig({ ...config, board: e.target.value as PistormBoard })}
              style={{ width: "100%", padding: "6px 8px", background: "var(--bg)", color: "var(--text)", border: "1px solid var(--border)", borderRadius: 4 }}
            >
              <option value="a500a2000">PiStorm A500 / A2000 (DIP-64 Socket)</option>
              <option value="a1200lite">PiStorm32-Lite (Amiga 1200 Trapdoor)</option>
              <option value="a600">PiStorm600 (Amiga 600 PLCC)</option>
            </select>
          </div>

          {/* RTG HDMI Video Output */}
          <div>
            <label className="muted" style={{ fontSize: 12, display: "block", marginBottom: 4 }}>
              RTG Video Output (Picasso96 HDMI):
            </label>
            <select
              value={config.rtg_resolution}
              onChange={(e) => setConfig({ ...config, rtg_resolution: e.target.value as RtgResolution })}
              style={{ width: "100%", padding: "6px 8px", background: "var(--bg)", color: "var(--text)", border: "1px solid var(--border)", borderRadius: 4 }}
            >
              <option value="res1080p">1920x1080 @ 60Hz (Full HD TrueColor 32-bit)</option>
              <option value="res720p">1280x720 @ 60Hz (HD Ready TrueColor 32-bit)</option>
              <option value="res1024x768">1024x768 @ 60Hz (Classic 4:3 RTG)</option>
              <option value="res800x600">800x600 @ 60Hz</option>
              <option value="disabled">Disabled (Native Amiga Video Output Only)</option>
            </select>
          </div>

          {/* Kickstart ROM */}
          <div>
            <label className="muted" style={{ fontSize: 12, display: "block", marginBottom: 4 }}>
              Kickstart ROM File on SD Card:
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
          <h2 style={{ fontSize: 15 }}>⚡ CPU Performance & Memory Map</h2>

          {/* Fast RAM Slider */}
          <div>
            <div style={{ display: "flex", justifyContent: "space-between", fontSize: 12 }}>
              <span className="muted">Fast RAM Allocation:</span>
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
              Fast RAM is mapped directly into Amiga 32-bit address space.
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
              <strong>Enable JIT Dynamic Recompiler (Emu68 JIT)</strong>
              <div className="muted" style={{ fontSize: 11 }}>
                Translates 68k instructions to ARM64 on the fly for ~800+ MIPS.
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
              <strong>Enable 68040 MMU Emulation</strong>
              <div className="muted" style={{ fontSize: 11 }}>
                Required for WHDLoad MuForce and advanced memory protections.
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
              <strong>Enable High-Speed MicroSD Storage (`emu68-sd.device`)</strong>
              <div className="muted" style={{ fontSize: 11 }}>
                High speed hard drive access (20+ MB/s) directly from SD card partitions.
              </div>
            </div>
          </label>
        </section>
      </div>
    </div>
  );
}
