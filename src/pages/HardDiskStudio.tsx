import { useState, useEffect } from "react";
import { useLocation, useNavigate } from "react-router-dom";
import { open, save } from "@tauri-apps/plugin-dialog";
import { useTranslation } from "react-i18next";

import {
  hdfOpen,
  hdfCreate,
  type HdfInfo,
  type ParsedPartition,
  type PartitionSpec,
  type AmigaHardDiskFs,
} from "@/lib/hdf";

interface FsChoice {
  id: AmigaHardDiskFs;
  name: string;
  badge: string;
  badgeType: "ok" | "muted" | "warn";
  description: string;
  features: string[];
}

const FILESYSTEM_CHOICES: FsChoice[] = [
  {
    id: "pfs3directscsi",
    name: "PFS3-AIO (DirectSCSI — PDS\\3)",
    badge: "⭐ Recommended (Fast & Safe)",
    badgeType: "ok",
    description: "Professional File System 3: High speed, crash-resilient without validation delays, 64-bit DirectSCSI safe (>4GB support).",
    features: ["No 'Validating...' delay on crashes", "Ultra fast directory reads", "DirectSCSI 64-bit safe", "Low RAM usage"],
  },
  {
    id: "sfs0",
    name: "Smart File System (SFS\\0)",
    badge: "Advanced (Journaled)",
    badgeType: "muted",
    description: "Journaled filesystem designed for Amiga hard disks with automatic integrity recovery.",
    features: ["Journaling structure", "Defragmentation-free design", "Fast performance on large drives"],
  },
  {
    id: "ffsdircache",
    name: "Fast File System DC (DOS\\3)",
    badge: "Classic AmigaDOS",
    badgeType: "muted",
    description: "Standard AmigaDOS Directory Cache filesystem native to Kickstart 2.04 and 3.x.",
    features: ["Native AmigaDOS support", "Directory cache acceleration", "Compatible with WB 2.x/3.x"],
  },
  {
    id: "ffsstandard",
    name: "Fast File System (DOS\\1)",
    badge: "Kickstart 1.3 Compatible",
    badgeType: "warn",
    description: "Original Fast File System with maximum backward compatibility for Kickstart 1.3 / A500 setups.",
    features: ["Kickstart 1.3 compatible", "Universal legacy support", "Basic structure"],
  },
];

export function HardDiskStudio() {
  const { t } = useTranslation();
  const location = useLocation();
  const navigate = useNavigate();

  const [path, setPath] = useState<string | null>(null);
  const [info, setInfo] = useState<HdfInfo | null>(null);
  const [selectedPart, setSelectedPart] = useState<ParsedPartition | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [statusMsg, setStatusMsg] = useState<string | null>(null);

  // Wizard Modal State
  const [showCreateModal, setShowCreateModal] = useState(false);
  const [createPresetSizeMb, setCreatePresetSizeMb] = useState<number>(1024); // 1 GB
  const [createTemplate, setCreateTemplate] = useState<"single" | "split">("split");
  const [selectedFs, setSelectedFs] = useState<AmigaHardDiskFs>("pfs3directscsi");

  useEffect(() => {
    const navState = location.state as { path?: string } | undefined;
    if (navState?.path && navState.path !== path) {
      void loadHdf(navState.path);
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [location.state]);

  async function loadHdf(p: string) {
    setBusy(true);
    setError(null);
    setStatusMsg(null);
    try {
      const hdfInfo = await hdfOpen(p);
      setInfo(hdfInfo);
      setPath(p);
      if (hdfInfo.partitions.length > 0) {
        setSelectedPart(hdfInfo.partitions[0]);
      }
    } catch (e) {
      setError(String(e));
      setInfo(null);
    } finally {
      setBusy(false);
    }
  }

  async function handleOpen() {
    const sel = await open({
      multiple: false,
      filters: [{ name: "Hard Disk File (HDF / IMG)", extensions: ["hdf", "img"] }],
      title: "Open Amiga Hard Disk Image",
    });
    if (typeof sel === "string") {
      await loadHdf(sel);
    }
  }

  async function handleCreateConfirm() {
    setShowCreateModal(false);
    const defaultName = `Amiga_${createPresetSizeMb >= 1024 ? `${createPresetSizeMb / 1024}GB` : `${createPresetSizeMb}MB`}.hdf`;
    const dest = await save({
      defaultPath: defaultName,
      filters: [{ name: "Hard Disk File (HDF)", extensions: ["hdf"] }],
      title: "Save New Hard Disk File",
    });
    if (!dest) return;

    setBusy(true);
    setError(null);
    setStatusMsg(null);

    try {
      const partitions: PartitionSpec[] = [];
      if (createTemplate === "single") {
        partitions.push({
          drive_name: "DH0",
          fs_type: selectedFs,
          size_mb: createPresetSizeMb,
          bootable: true,
          boot_priority: 0,
          num_buffers: 100,
        });
      } else {
        // Split: 500 MB (or 25%) System + Rest Work
        const sysMb = Math.min(500, Math.floor(createPresetSizeMb / 3));
        const workMb = createPresetSizeMb - sysMb;
        partitions.push({
          drive_name: "DH0",
          fs_type: selectedFs,
          size_mb: sysMb,
          bootable: true,
          boot_priority: 5,
          num_buffers: 100,
        });
        partitions.push({
          drive_name: "DH1",
          fs_type: selectedFs,
          size_mb: workMb,
          bootable: false,
          boot_priority: 0,
          num_buffers: 100,
        });
      }

      const totalBytes = (createPresetSizeMb as number) * 1024 * 1024;
      const created = await hdfCreate(dest, totalBytes, true, partitions);
      setInfo(created);
      setPath(dest);
      setStatusMsg(`Created new RDB Hard Disk Image '${dest}' (${createPresetSizeMb} MB) with ${partitions.length} partition(s).`);
      if (created.partitions.length > 0) {
        setSelectedPart(created.partitions[0]);
      }
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  const partitionColors = ["#388bfd", "#3fb950", "#d29922", "#a371f7", "#f85149"];

  return (
    <div>
      <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center" }}>
        <h1 style={{ fontSize: 20, margin: 0 }}>{t("nav.hardDisk")} — Hard Disk Studio</h1>
        <div style={{ display: "flex", gap: 8, flexWrap: "wrap" }}>
          {path && (
            <button
              className="btn btn-sm btn-primary"
              onClick={() => navigate("/winuae", { state: { path } })}
              disabled={busy}
            >
              🚀 Launch in WinUAE
            </button>
          )}
          <button
            className="btn btn-sm"
            onClick={() => setShowCreateModal(true)}
            disabled={busy}
          >
            ➕ New HDF Wizard…
          </button>
          <button className="btn btn-sm" onClick={handleOpen} disabled={busy}>
            {t("common.open")} HDF…
          </button>
        </div>
      </div>

      {path && (
        <div style={{ margin: "8px 0 12px", fontSize: 12 }}>
          <span className="muted">Disk Image:</span>{" "}
          <strong style={{ wordBreak: "break-all" }}>{path}</strong>
        </div>
      )}

      {error && <div className="badge badge-err" style={{ marginBottom: 12, padding: "6px 12px" }}>{error}</div>}
      {statusMsg && <div className="badge badge-ok" style={{ marginBottom: 12, padding: "6px 12px" }}>{statusMsg}</div>}
      {busy && <div className="muted" style={{ marginBottom: 12 }}>Working on hard disk…</div>}

      {/* Disk Overview & Visual Partition Bar */}
      {info && (
        <section className="card" style={{ marginBottom: 16 }}>
          <div style={{ display: "flex", justifyContent: "space-between", alignItems: "baseline" }}>
            <h2 style={{ fontSize: 16 }}>
              {info.hdf_type === "rdb" ? "Rigid Disk Block (RDB) Partitioned Drive" : "Plain / Raw Single Partition Drive"}
            </h2>
            <span className={`badge ${info.rdb_checksum_valid ? "badge-ok" : "badge-warn"}`}>
              {info.rdb_checksum_valid ? "RDB Checksum: OK" : "RDB Checksum: Invalid"}
            </span>
          </div>

          <div
            style={{
              display: "grid",
              gridTemplateColumns: "repeat(auto-fit, minmax(160px, 1fr))",
              gap: 12,
              fontSize: 13,
              marginTop: 10,
            }}
          >
            <div><span className="muted">Capacity:</span> <strong>{fmtBytes(info.total_bytes)}</strong></div>
            <div><span className="muted">Partitions:</span> {info.partitions.length}</div>
            <div><span className="muted">Cylinders:</span> {info.cylinders}</div>
            <div><span className="muted">Heads / Sectors:</span> {info.heads} / {info.sectors}</div>
          </div>

          {/* Visual Disk Map Bar */}
          <div style={{ marginTop: 16 }}>
            <div style={{ display: "flex", justifyContent: "space-between", fontSize: 12, marginBottom: 4 }}>
              <span className="muted">Visual Partition Layout</span>
              <span className="faint">{fmtBytes(info.total_bytes)}</span>
            </div>
            <div
              style={{
                display: "flex",
                height: 28,
                borderRadius: "var(--radius-sm)",
                overflow: "hidden",
                border: "1px solid var(--border)",
                background: "#161b22",
              }}
            >
              {info.partitions.map((p, idx) => {
                const pct = Math.max(12, Math.round((p.size_bytes / info.total_bytes) * 100));
                const color = partitionColors[idx % partitionColors.length];
                const isSel = selectedPart?.drive_name === p.drive_name;
                return (
                  <div
                    key={p.drive_name}
                    onClick={() => setSelectedPart(p)}
                    style={{
                      width: `${pct}%`,
                      background: color,
                      color: "#000",
                      fontWeight: 600,
                      fontSize: 11,
                      display: "flex",
                      alignItems: "center",
                      justifyContent: "center",
                      cursor: "pointer",
                      padding: "0 4px",
                      overflow: "hidden",
                      textOverflow: "ellipsis",
                      whiteSpace: "nowrap",
                      outline: isSel ? "2px solid #fff" : "none",
                      outlineOffset: -2,
                    }}
                    title={`${p.drive_name} (${p.dostype_str}) — ${fmtBytes(p.size_bytes)}`}
                  >
                    {p.drive_name}: {fmtBytes(p.size_bytes)}
                  </div>
                );
              })}
            </div>
          </div>
        </section>
      )}

      {/* Partitions Table & Details */}
      {info && (
        <div style={{ display: "grid", gridTemplateColumns: selectedPart ? "1.2fr 0.8fr" : "1fr", gap: 16 }}>
          {/* Partitions List */}
          <section className="card">
            <h2 style={{ fontSize: 15 }}>🗄️ Partitions Table ({info.partitions.length})</h2>
            <div className="file-list-container" style={{ marginTop: 10 }}>
              {info.partitions.map((p) => {
                const isSel = selectedPart?.drive_name === p.drive_name;
                return (
                  <div
                    key={p.drive_name}
                    className="file-row"
                    onClick={() => setSelectedPart(p)}
                    style={{
                      cursor: "pointer",
                      borderColor: isSel ? "var(--accent)" : "transparent",
                    }}
                  >
                    <div className="file-row-main">
                      <span className="file-row-icon">💽</span>
                      <div>
                        <strong>{p.drive_name}:</strong>{" "}
                        <span className="badge badge-muted" style={{ marginLeft: 4 }}>
                          {p.dostype_str}
                        </span>
                        {p.bootable && (
                          <span className="badge badge-ok" style={{ marginLeft: 4 }}>
                            Bootable (Pri {p.boot_priority})
                          </span>
                        )}
                      </div>
                    </div>
                    <div className="file-row-meta">
                      <span className="file-row-size" style={{ width: 85 }}>
                        {fmtBytes(p.size_bytes)}
                      </span>
                    </div>
                  </div>
                );
              })}
            </div>
          </section>

          {/* Selected Partition Deep Inspector */}
          {selectedPart && (
            <section className="card">
              <h2 style={{ fontSize: 15 }}>🔬 Partition Properties</h2>
              <div style={{ display: "flex", flexDirection: "column", gap: 10, marginTop: 10, fontSize: 13 }}>
                <div><span className="muted">Device Name:</span> <strong>{selectedPart.drive_name}:</strong></div>
                <div><span className="muted">DosType Signature:</span> <code>{selectedPart.dostype_str} (0x{selectedPart.dostype.toString(16).toUpperCase()})</code></div>
                <div><span className="muted">Filesystem:</span> {selectedPart.fs_type.toUpperCase()}</div>
                <div><span className="muted">Cylinders:</span> {selectedPart.low_cyl} → {selectedPart.high_cyl} ({selectedPart.cylinder_count} cyls)</div>
                <div><span className="muted">Partition Size:</span> {fmtBytes(selectedPart.size_bytes)}</div>
                <div><span className="muted">Bootable:</span> <strong>{selectedPart.bootable ? "Yes" : "No"}</strong></div>
                <div><span className="muted">Boot Priority:</span> {selectedPart.boot_priority}</div>
                <div><span className="muted">Buffers:</span> {selectedPart.num_buffers}</div>
              </div>
            </section>
          )}
        </div>
      )}

      {!path && !busy && (
        <p className="muted" style={{ textAlign: "center", marginTop: 24 }}>
          Open an existing .hdf image or click "New HDF Wizard" to create an Amiga hard disk.
        </p>
      )}

      {/* Modal: Create New HDF Wizard */}
      {showCreateModal && (
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
          <div className="card" style={{ width: 560, maxWidth: "92vw", maxHeight: "90vh", overflowY: "auto" }}>
            <h3 style={{ margin: "0 0 12px" }}>➕ Create New Amiga Hard Disk (HDF)</h3>

            {/* Step 1: Disk Capacity Presets */}
            <div style={{ marginBottom: 14 }}>
              <label className="muted" style={{ fontSize: 12, display: "block", marginBottom: 6 }}>
                1. Select Disk Size:
              </label>
              <div style={{ display: "grid", gridTemplateColumns: "repeat(auto-fit, minmax(100px, 1fr))", gap: 6 }}>
                {[
                  { size: 500, label: "500 MB", hint: "Compact" },
                  { size: 1024, label: "1 GB", hint: "Classic" },
                  { size: 2048, label: "2 GB", hint: "WHDLoad" },
                  { size: 4096, label: "4 GB", hint: "Standard" },
                  { size: 8192, label: "8 GB", hint: "Large Disk" },
                ].map((s) => (
                  <button
                    key={s.size}
                    className={`btn btn-sm ${createPresetSizeMb === s.size ? "btn-primary" : ""}`}
                    onClick={() => setCreatePresetSizeMb(s.size)}
                    style={{ flexDirection: "column", padding: "6px" }}
                  >
                    <strong>{s.label}</strong>
                    <span className="faint" style={{ fontSize: 10 }}>{s.hint}</span>
                  </button>
                ))}
              </div>
            </div>

            {/* Step 2: Partitioning Layout Template */}
            <div style={{ marginBottom: 14 }}>
              <label className="muted" style={{ fontSize: 12, display: "block", marginBottom: 6 }}>
                2. Partitioning Template:
              </label>
              <div style={{ display: "grid", gridTemplateColumns: "1fr 1fr", gap: 8 }}>
                <div
                  onClick={() => setCreateTemplate("split")}
                  style={{
                    padding: "8px 10px",
                    borderRadius: 4,
                    border: createTemplate === "split" ? "1px solid var(--accent)" : "1px solid var(--border)",
                    background: createTemplate === "split" ? "var(--bg-hover)" : "var(--bg)",
                    cursor: "pointer",
                  }}
                >
                  <strong>⭐ Classic Split (Recommended)</strong>
                  <div className="muted" style={{ fontSize: 11, marginTop: 2 }}>
                    DH0: System (Bootable) + DH1: Work/Games
                  </div>
                </div>

                <div
                  onClick={() => setCreateTemplate("single")}
                  style={{
                    padding: "8px 10px",
                    borderRadius: 4,
                    border: createTemplate === "single" ? "1px solid var(--accent)" : "1px solid var(--border)",
                    background: createTemplate === "single" ? "var(--bg-hover)" : "var(--bg)",
                    cursor: "pointer",
                  }}
                >
                  <strong>Single Partition</strong>
                  <div className="muted" style={{ fontSize: 11, marginTop: 2 }}>
                    DH0: System (100% capacity)
                  </div>
                </div>
              </div>
            </div>

            {/* Step 3: Parametric Filesystem Choice with Explanations */}
            <div style={{ marginBottom: 16 }}>
              <label className="muted" style={{ fontSize: 12, display: "block", marginBottom: 6 }}>
                3. Choose Amiga Filesystem:
              </label>
              <div style={{ display: "flex", flexDirection: "column", gap: 6 }}>
                {FILESYSTEM_CHOICES.map((fs) => {
                  const isSel = selectedFs === fs.id;
                  return (
                    <div
                      key={fs.id}
                      onClick={() => setSelectedFs(fs.id)}
                      style={{
                        padding: "8px 10px",
                        borderRadius: 4,
                        border: isSel ? "1px solid var(--accent)" : "1px solid var(--border)",
                        background: isSel ? "var(--bg-hover)" : "var(--bg)",
                        cursor: "pointer",
                      }}
                    >
                      <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center" }}>
                        <strong style={{ fontSize: 13 }}>{fs.name}</strong>
                        <span className={`badge badge-${fs.badgeType}`} style={{ fontSize: 10 }}>
                          {fs.badge}
                        </span>
                      </div>
                      <p className="muted" style={{ margin: "3px 0 0", fontSize: 11 }}>
                        {fs.description}
                      </p>
                    </div>
                  );
                })}
              </div>
            </div>

            {/* Actions */}
            <div style={{ display: "flex", justifyContent: "flex-end", gap: 8 }}>
              <button className="btn" onClick={() => setShowCreateModal(false)}>
                Cancel
              </button>
              <button className="btn btn-primary" onClick={handleCreateConfirm}>
                Create HDF File…
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}

function fmtBytes(n: number): string {
  if (n < 1024) return `${n} B`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(0)} KB`;
  if (n < 1024 * 1024 * 1024) return `${(n / (1024 * 1024)).toFixed(0)} MB`;
  return `${(n / (1024 * 1024 * 1024)).toFixed(2)} GB`;
}
