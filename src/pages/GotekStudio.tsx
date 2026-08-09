import { useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { useTranslation } from "react-i18next";

import {
  gotekScan,
  gotekSave,
  type FlashFloppyConfig,
  type GotekSlot,
  type FlashFloppyNavMode,
  type FlashFloppyDisplay,
} from "@/lib/gotek";

export function GotekStudio() {
  const { t } = useTranslation();

  const [drivePath, setDrivePath] = useState<string | null>(null);
  const [config, setConfig] = useState<FlashFloppyConfig>({
    nav_mode: "native",
    display_type: "oled-128x32",
    oled_font: "6x13",
    rotary: "track",
    step_volume: 25,
    interface: "amiga",
    host: "amiga",
    write_protect: false,
    side_select_polarity: "high",
  });
  const [slots, setSlots] = useState<GotekSlot[]>([]);
  const [selectedSlotIdx, setSelectedSlotIdx] = useState<number | null>(null);

  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [statusMsg, setStatusMsg] = useState<string | null>(null);

  async function handleSelectUsb() {
    const sel = await open({
      directory: true,
      multiple: false,
      title: "Select Gotek USB Flash Drive Root Folder",
    });
    if (typeof sel !== "string") return;

    setBusy(true);
    setError(null);
    setStatusMsg(null);
    try {
      const scanned = await gotekScan(sel);
      setDrivePath(sel);
      setConfig(scanned.config);
      setSlots(scanned.slots);
      setStatusMsg(`Scanned USB drive: Found ${scanned.adf_files.length} disk images, ${scanned.slots.length} quickslots.`);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function handleAddDisksToSlots() {
    const sel = await open({
      multiple: true,
      filters: [{ name: "Amiga Floppy Disk", extensions: ["adf", "adz", "hfe"] }],
      title: "Select ADF files to assign into Gotek Slots",
    });
    if (!sel || !Array.isArray(sel)) return;

    const newSlots = [...slots];
    let nextNum = newSlots.length > 0 ? Math.max(...newSlots.map((s) => s.slot_num)) + 1 : 0;

    for (const filePath of sel) {
      const fileName = filePath.split(/[/\\]/).pop() ?? filePath;
      // Use relative path if inside drive
      let relPath = fileName;
      if (drivePath && filePath.startsWith(drivePath)) {
        relPath = filePath.slice(drivePath.length).replace(/^[/\\]+/, "").replace(/\\/g, "/");
      }
      newSlots.push({
        slot_num: nextNum++,
        file_path: relPath,
        title: fileName,
      });
    }

    setSlots(newSlots);
    setStatusMsg(`Added ${sel.length} disk(s) to slot list.`);
  }

  function handleRemoveSlot(slotNum: number) {
    setSlots(slots.filter((s) => s.slot_num !== slotNum));
    if (selectedSlotIdx !== null && slots[selectedSlotIdx]?.slot_num === slotNum) {
      setSelectedSlotIdx(null);
    }
  }

  async function handleSave() {
    if (!drivePath) return;
    setBusy(true);
    setError(null);
    setStatusMsg(null);
    try {
      await gotekSave(drivePath, config, slots);
      setStatusMsg("✓ Successfully saved FF.CFG and IMAGE_A.CFG to USB drive!");
    } catch (e) {
      setError(`Failed to save: ${String(e)}`);
    } finally {
      setBusy(false);
    }
  }

  const activeSlot = selectedSlotIdx !== null ? slots[selectedSlotIdx] : slots[0];

  return (
    <div>
      <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center" }}>
        <h1 style={{ fontSize: 20, margin: 0 }}>{t("nav.gotek")} — Gotek & FlashFloppy Studio</h1>
        <div style={{ display: "flex", gap: 8 }}>
          <button className="btn btn-sm" onClick={handleSelectUsb} disabled={busy}>
            📂 Select USB Drive…
          </button>
          {drivePath && (
            <button className="btn btn-sm btn-primary" onClick={handleSave} disabled={busy}>
              💾 Save & Sync to USB
            </button>
          )}
        </div>
      </div>

      {drivePath && (
        <div style={{ margin: "8px 0 12px", fontSize: 12 }}>
          <span className="muted">USB Drive:</span>{" "}
          <strong style={{ wordBreak: "break-all" }}>{drivePath}</strong>
        </div>
      )}

      {error && <div className="badge badge-err" style={{ marginBottom: 12, padding: "6px 12px" }}>{error}</div>}
      {statusMsg && <div className="badge badge-ok" style={{ marginBottom: 12, padding: "6px 12px" }}>{statusMsg}</div>}
      {busy && <div className="muted" style={{ marginBottom: 12 }}>Scanning USB drive…</div>}

      {/* Main Grid: Display Simulator & Hardware Configurator */}
      <div style={{ display: "grid", gridTemplateColumns: "1fr 1fr", gap: 16, marginTop: 12 }}>
        {/* Left: Gotek Hardware Screen Simulator */}
        <section className="card">
          <h2 style={{ fontSize: 15 }}>📺 Gotek Screen Simulator</h2>
          <p className="muted" style={{ fontSize: 11, margin: "2px 0 12px" }}>
            Live preview of how your display and selected disk will render on real hardware:
          </p>

          {/* Simulated Display Screen Box */}
          <div
            style={{
              background: config.display_type === "lcd-16x2" ? "#1e3a1e" : "#0d1117",
              border: "3px solid #30363d",
              borderRadius: 6,
              padding: "16px",
              minHeight: 100,
              display: "flex",
              flexDirection: "column",
              justifyContent: "center",
              boxShadow: "inset 0 0 12px rgba(0,0,0,0.8)",
            }}
          >
            {config.display_type === "7seg" ? (
              <div
                style={{
                  fontFamily: "monospace",
                  fontSize: 48,
                  fontWeight: "bold",
                  color: "#ff3b30",
                  textAlign: "center",
                  letterSpacing: 8,
                  textShadow: "0 0 8px rgba(255, 59, 48, 0.7)",
                }}
              >
                {activeSlot ? String(activeSlot.slot_num).padStart(3, "0") : "000"}
              </div>
            ) : config.display_type === "lcd-16x2" ? (
              <div
                style={{
                  fontFamily: "monospace",
                  fontSize: 16,
                  color: "#7ee787",
                  lineHeight: 1.6,
                  textShadow: "0 0 4px rgba(126, 231, 135, 0.5)",
                }}
              >
                <div>001/004 T:00.0</div>
                <div style={{ overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>
                  {activeSlot ? activeSlot.title : "Insert USB Media"}
                </div>
              </div>
            ) : (
              // OLED Display (128x32 or 128x64)
              <div
                style={{
                  fontFamily: "monospace",
                  fontSize: config.display_type === "oled-128x64" ? 14 : 12,
                  color: "#58a6ff",
                  lineHeight: 1.5,
                  textShadow: "0 0 5px rgba(88, 166, 255, 0.6)",
                }}
              >
                <div style={{ display: "flex", justifyContent: "space-between", borderBottom: "1px solid #21262d", paddingBottom: 2 }}>
                  <span>💾 DF0:</span>
                  <span>T:00.0 · RD</span>
                  <span>{config.nav_mode === "quickslot" ? `Slot ${activeSlot?.slot_num ?? 0}` : "DIR"}</span>
                </div>
                <div style={{ marginTop: 6, fontWeight: "bold", overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>
                  {activeSlot ? activeSlot.title : "(No disk loaded)"}
                </div>
                {config.display_type === "oled-128x64" && (
                  <div className="muted" style={{ fontSize: 11, color: "#8b949e", marginTop: 4 }}>
                    Amiga 880 KB OFS/FFS · Track 0/80
                  </div>
                )}
              </div>
            )}
          </div>

          {/* Parametric Navigation Mode Selector */}
          <div style={{ marginTop: 16 }}>
            <label className="muted" style={{ fontSize: 12, display: "block", marginBottom: 6 }}>
              🕹️ Parametric Navigation Mode:
            </label>
            <div style={{ display: "grid", gridTemplateColumns: "1fr 1fr", gap: 6 }}>
              {[
                {
                  id: "native" as FlashFloppyNavMode,
                  title: "Direct Folder Mode",
                  hint: "Browse folders & long filenames directly on OLED",
                },
                {
                  id: "quickslot" as FlashFloppyNavMode,
                  title: "Quickslot Mode",
                  hint: "Numeric slots 000..999 via IMAGE_A.CFG",
                },
              ].map((m) => (
                <div
                  key={m.id}
                  onClick={() => setConfig({ ...config, nav_mode: m.id })}
                  style={{
                    padding: "8px 10px",
                    borderRadius: 4,
                    border: config.nav_mode === m.id ? "1px solid var(--accent)" : "1px solid var(--border)",
                    background: config.nav_mode === m.id ? "var(--bg-hover)" : "var(--bg)",
                    cursor: "pointer",
                  }}
                >
                  <strong style={{ fontSize: 12 }}>{m.title}</strong>
                  <div className="muted" style={{ fontSize: 10, marginTop: 2 }}>{m.hint}</div>
                </div>
              ))}
            </div>
          </div>
        </section>

        {/* Right: Hardware Settings Panel */}
        <section className="card" style={{ display: "flex", flexDirection: "column", gap: 12 }}>
          <h2 style={{ fontSize: 15 }}>⚙️ FlashFloppy Hardware Setup (`FF.CFG`)</h2>

          {/* Display Type */}
          <div>
            <label className="muted" style={{ fontSize: 12, display: "block", marginBottom: 4 }}>
              Display Hardware:
            </label>
            <select
              value={config.display_type}
              onChange={(e) => setConfig({ ...config, display_type: e.target.value as FlashFloppyDisplay })}
              style={{ width: "100%", padding: "6px 8px", background: "var(--bg)", color: "var(--text)", border: "1px solid var(--border)", borderRadius: 4 }}
            >
              <option value="oled-128x32">OLED 0.91" (128x32 Pixels)</option>
              <option value="oled-128x64">OLED 0.96" / 1.3" (128x64 Pixels)</option>
              <option value="lcd-16x2">LCD 16x2 Characters</option>
              <option value="7seg">Classic 3-Digit 7-Segment LED</option>
            </select>
          </div>

          {/* Step Sound Volume */}
          <div>
            <div style={{ display: "flex", justifyContent: "space-between", fontSize: 12 }}>
              <span className="muted">Floppy Step Sound Volume (Piezo Buzzer):</span>
              <strong>{config.step_volume}%</strong>
            </div>
            <input
              type="range"
              min="0"
              max="100"
              step="5"
              value={config.step_volume}
              onChange={(e) => setConfig({ ...config, step_volume: Number(e.target.value) })}
              style={{ width: "100%", marginTop: 4 }}
            />
          </div>

          {/* Rotary Mode */}
          <div>
            <label className="muted" style={{ fontSize: 12, display: "block", marginBottom: 4 }}>
              Rotary Encoder Control:
            </label>
            <select
              value={config.rotary}
              onChange={(e) => setConfig({ ...config, rotary: e.target.value as any })}
              style={{ width: "100%", padding: "6px 8px", background: "var(--bg)", color: "var(--text)", border: "1px solid var(--border)", borderRadius: 4 }}
            >
              <option value="track">Track Seek (Turn = Track / Slot)</option>
              <option value="quickslot">Quickslot Select</option>
              <option value="buttons">Buttons Emulation</option>
            </select>
          </div>

          {/* Write Protect */}
          <label style={{ display: "flex", alignItems: "center", gap: 8, fontSize: 13, cursor: "pointer", marginTop: 4 }}>
            <input
              type="checkbox"
              checked={config.write_protect}
              onChange={(e) => setConfig({ ...config, write_protect: e.target.checked })}
            />
            <span>🔒 Force Write Protect (Read-Only)</span>
          </label>
        </section>
      </div>

      {/* Slots Rack Manager */}
      <section className="card" style={{ marginTop: 16 }}>
        <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center" }}>
          <h2 style={{ fontSize: 15 }}>🗃️ Gotek Quickslot Rack ({slots.length} Assigned)</h2>
          <button className="btn btn-sm" onClick={handleAddDisksToSlots} disabled={busy}>
            ➕ Add Disks to Slots…
          </button>
        </div>

        {slots.length === 0 ? (
          <p className="muted" style={{ textAlign: "center", padding: "16px 0", fontSize: 13 }}>
            No disks assigned to slots yet. Click "Add Disks to Slots" to map your games to slots 000..999.
          </p>
        ) : (
          <div className="file-list-container" style={{ marginTop: 10 }}>
            {slots.map((s, idx) => {
              const isSel = selectedSlotIdx === idx;
              return (
                <div
                  key={s.slot_num}
                  className="file-row"
                  onClick={() => setSelectedSlotIdx(idx)}
                  style={{
                    cursor: "pointer",
                    borderColor: isSel ? "var(--accent)" : "transparent",
                  }}
                >
                  <div className="file-row-main">
                    <span className="badge badge-muted" style={{ width: 44, textAlign: "center" }}>
                      {String(s.slot_num).padStart(3, "0")}
                    </span>
                    <span className="file-row-icon" style={{ marginLeft: 6 }}>💾</span>
                    <strong style={{ fontSize: 13 }}>{s.title}</strong>
                  </div>
                  <div className="file-row-meta" style={{ gap: 8 }}>
                    <span className="faint" style={{ fontSize: 11, wordBreak: "break-all" }}>
                      {s.file_path}
                    </span>
                    <button
                      className="btn btn-sm"
                      onClick={(e) => {
                        e.stopPropagation();
                        handleRemoveSlot(s.slot_num);
                      }}
                      style={{ padding: "2px 6px", fontSize: 11 }}
                    >
                      ✕
                    </button>
                  </div>
                </div>
              );
            })}
          </div>
        )}
      </section>
    </div>
  );
}
