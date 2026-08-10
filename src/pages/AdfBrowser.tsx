import { useState, useEffect } from "react";
import { useLocation, useNavigate } from "react-router-dom";
import { open, save } from "@tauri-apps/plugin-dialog";
import { useTranslation } from "react-i18next";

import {
  adfOpen,
  adfList,
  adfValidate,
  adfExtractFile,
  adfCreateBlank,
  adfAddFile,
  adfCreateDirectory,
  adfDeleteEntry,
  adfRenameEntry,
  type AdfInfo,
  type AdfEntry,
  type MutationOutcome,
  type ValidationReport,
} from "@/lib/adf";

interface Breadcrumb {
  block: number | null;
  name: string;
}

export function AdfBrowser() {
  const { t } = useTranslation();
  const location = useLocation();
  const navigate = useNavigate();

  const [path, setPath] = useState<string | null>(null);
  const [info, setInfo] = useState<AdfInfo | null>(null);
  const [entries, setEntries] = useState<AdfEntry[]>([]);
  const [report, setReport] = useState<ValidationReport | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [successMsg, setSuccessMsg] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  // Directory stack / breadcrumb navigation (block: null = root)
  const [breadcrumbs, setBreadcrumbs] = useState<Breadcrumb[]>([
    { block: null, name: "DF0:" },
  ]);

  // Dual-option mode for damaged / non-dos disks
  const [showHexMode, setShowHexMode] = useState(false);

  // Modal dialog states
  const [showNewAdfModal, setShowNewAdfModal] = useState(false);
  const [newVolumeName, setNewVolumeName] = useState("EmptyDisk");
  const [newFsType, setNewFsType] = useState<"ofs" | "ffs">("ffs");
  const [newBootable, setNewBootable] = useState(false);

  const [showMkdirModal, setShowMkdirModal] = useState(false);
  const [newFolderName, setNewFolderName] = useState("");

  const [renameTarget, setRenameTarget] = useState<AdfEntry | null>(null);
  const [renameValue, setRenameValue] = useState("");

  const [deleteTarget, setDeleteTarget] = useState<AdfEntry | null>(null);

  // Current directory block (null = root)
  const currentBreadcrumb = breadcrumbs[breadcrumbs.length - 1];
  const currentDirBlock = currentBreadcrumb.block ?? undefined;

  // Auto-load if navigated from Dashboard via state
  useEffect(() => {
    const p = (location.state as { path?: string })?.path;
    if (p && p !== path) {
      void loadDisk(p);
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [location.state]);

  async function loadDisk(p: string) {
    setBusy(true);
    setError(null);
    setShowHexMode(false);
    setSuccessMsg(null);
    try {
      const [diskInfo, rootEntries] = await Promise.all([adfOpen(p), adfList(p)]);
      setInfo(diskInfo);
      setEntries(rootEntries);
      setPath(p);
      setBreadcrumbs([
        {
          block: null,
          name: diskInfo.volume_name ? `DF0: (${diskInfo.volume_name})` : "DF0:",
        },
      ]);
      adfValidate(p).then(setReport).catch(() => {});
    } catch (e) {
      setError(String(e));
      setInfo(null);
      setEntries([]);
    } finally {
      setBusy(false);
    }
  }

  async function refreshCurrentDir(updatedInfo?: AdfInfo) {
    if (!path) return;
    if (updatedInfo) setInfo(updatedInfo);
    try {
      const subEntries = await adfList(path, currentDirBlock);
      setEntries(subEntries);
      if (!updatedInfo) {
        const i = await adfOpen(path);
        setInfo(i);
      }
    } catch (e) {
      setError(String(e));
    }
  }

  /**
   * Handle the result of any operation that modified the disk: refresh the
   * view and tell the user where the previous version was preserved.
   * Spec §92 — always say what was backed up.
   */
  async function applyMutation(outcome: MutationOutcome, message: string) {
    setSuccessMsg(
      outcome.backup_path
        ? t("adf.msg.withBackup", { message, backupPath: outcome.backup_path })
        : message
    );
    await refreshCurrentDir(outcome.info);
  }

  async function navigateToDir(dirBlock: number | null, dirName: string) {
    if (!path) return;
    setBusy(true);
    setError(null);
    setSuccessMsg(null);
    try {
      const subEntries = await adfList(path, dirBlock ?? undefined);
      setEntries(subEntries);

      if (dirBlock === null) {
        setBreadcrumbs([
          {
            block: null,
            name: info?.volume_name ? `DF0: (${info.volume_name})` : "DF0:",
          },
        ]);
      } else {
        const idx = breadcrumbs.findIndex((b) => b.block === dirBlock);
        if (idx >= 0) {
          setBreadcrumbs(breadcrumbs.slice(0, idx + 1));
        } else {
          setBreadcrumbs([...breadcrumbs, { block: dirBlock, name: dirName }]);
        }
      }
    } catch (e) {
      setError(t("adf.msg.openDirFailed", { name: dirName, error: String(e) }));
    } finally {
      setBusy(false);
    }
  }

  async function chooseFile() {
    const sel = await open({
      multiple: false,
      filters: [{ name: "Amiga Floppy Image (ADF)", extensions: ["adf", "adz"] }],
    });
    if (typeof sel === "string") {
      await loadDisk(sel);
    }
  }

  async function handleCreateNewAdf() {
    setShowNewAdfModal(false);
    const dest = await save({
      defaultPath: `${newVolumeName}.adf`,
      filters: [{ name: "Amiga Floppy (ADF)", extensions: ["adf"] }],
    });
    if (!dest) return;

    setBusy(true);
    setError(null);
    setSuccessMsg(null);
    try {
      await adfCreateBlank(dest, newVolumeName, newFsType, newBootable);
      setSuccessMsg(t("adf.msg.createdImage", { name: newVolumeName }));
      await loadDisk(dest);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function handleAddFile() {
    if (!path) return;
    const sel = await open({
      multiple: false,
      title: t("adf.selectFileTitle"),
    });
    if (typeof sel !== "string") return;

    setBusy(true);
    setError(null);
    setSuccessMsg(null);
    try {
      const updated = await adfAddFile(path, currentDirBlock, sel);
      await applyMutation(updated, t("adf.msg.fileImported"));
    } catch (e) {
      setError(t("adf.msg.addFileFailed", { error: String(e) }));
    } finally {
      setBusy(false);
    }
  }

  async function handleCreateFolder() {
    if (!path || !newFolderName.trim()) return;
    setShowMkdirModal(false);
    setBusy(true);
    setError(null);
    setSuccessMsg(null);
    try {
      const updated = await adfCreateDirectory(path, currentDirBlock, newFolderName.trim());
      const created = newFolderName.trim();
      setNewFolderName("");
      await applyMutation(updated, t("adf.msg.dirCreated", { name: created }));
    } catch (e) {
      setError(t("adf.msg.createDirFailed", { error: String(e) }));
    } finally {
      setBusy(false);
    }
  }

  async function handleDeleteConfirm() {
    if (!path || !deleteTarget) return;
    const target = deleteTarget;
    setDeleteTarget(null);
    setBusy(true);
    setError(null);
    setSuccessMsg(null);
    try {
      const updated = await adfDeleteEntry(path, currentDirBlock, target.header_block);
      await applyMutation(updated, t("adf.msg.deleted", { name: target.name }));
    } catch (e) {
      setError(t("adf.msg.deleteFailed", { name: target.name, error: String(e) }));
    } finally {
      setBusy(false);
    }
  }

  async function handleRenameConfirm() {
    if (!path || !renameTarget || !renameValue.trim()) return;
    const target = renameTarget;
    setRenameTarget(null);
    setBusy(true);
    setError(null);
    setSuccessMsg(null);
    try {
      const updated = await adfRenameEntry(
        path,
        currentDirBlock,
        target.header_block,
        renameValue.trim()
      );
      await applyMutation(
        updated,
        t("adf.msg.renamed", { from: target.name, to: renameValue.trim() })
      );
    } catch (e) {
      setError(t("adf.msg.renameFailed", { name: target.name, error: String(e) }));
    } finally {
      setBusy(false);
    }
  }

  async function extract(entry: AdfEntry) {
    if (!path) return;
    const dest = await save({ defaultPath: entry.name });
    if (!dest) return;
    setBusy(true);
    setError(null);
    setSuccessMsg(null);
    try {
      const bytes = await adfExtractFile(path, entry.header_block);
      setSuccessMsg(
        t("adf.msg.extracted", { bytes: bytes.length, name: entry.name, dest })
      );
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  const hasWarningOrProblem =
    (info && !info.checksum_valid) || (report && report.status !== "healthy");

  return (
    <div>
      <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center" }}>
        <h1 style={{ fontSize: 20, margin: 0 }}>{t("nav.diskTools")} — {t("adf.title")}</h1>
        <div style={{ display: "flex", gap: 8, flexWrap: "wrap" }}>
          {path && (
            <button
              className="btn btn-sm btn-primary"
              onClick={() => navigate("/winuae", { state: { path } })}
              disabled={busy}
            >
              🚀 {t("adf.runInWinuae")}
            </button>
          )}
          <button
            className="btn btn-sm"
            onClick={() => setShowNewAdfModal(true)}
            disabled={busy}
          >
            ➕ {t("adf.newBlank")}
          </button>
          <button
            className="btn btn-sm"
            onClick={chooseFile}
            disabled={busy}
          >
            {t("adf.openAdf")}
          </button>
        </div>
      </div>

      {path && (
        <div style={{ margin: "8px 0 12px", fontSize: 12 }}>
          <span className="muted">{t("adf.currentDisk")}</span>{" "}
          <strong style={{ wordBreak: "break-all" }}>{path}</strong>
        </div>
      )}

      {error && <div className="badge badge-err" style={{ marginBottom: 12, padding: "6px 12px" }}>{error}</div>}
      {successMsg && <div className="badge badge-ok" style={{ marginBottom: 12, padding: "6px 12px" }}>{successMsg}</div>}
      {busy && <div className="muted" style={{ marginBottom: 12 }}>{t("adf.working")}</div>}

      {/* Disk Overview Card */}
      {info && (
        <section className="card" style={{ marginBottom: 16 }}>
          <div style={{ display: "flex", justifyContent: "space-between", alignItems: "baseline" }}>
            <h2 style={{ fontSize: 16 }}>
              {info.volume_name || t("adf.unnamed")}
            </h2>
            <span className="badge badge-muted">
              {t("adf.rootBlock", { n: info.root_block })}
            </span>
          </div>

          <div
            style={{
              display: "grid",
              gridTemplateColumns: "repeat(auto-fit, minmax(170px, 1fr))",
              gap: 12,
              fontSize: 13,
              marginTop: 10,
            }}
          >
            <div>
              <span className="muted">{t("adf.filesystem")}</span>{" "}
              <strong>{info.fs_type.toUpperCase()}</strong>{" "}
              {info.international && <span className="badge badge-muted">INTL</span>}{" "}
              {info.dir_cache && <span className="badge badge-muted">DC</span>}
            </div>
            <div>
              <span className="muted">{t("adf.bootable")}</span>{" "}
              <strong>{info.bootable ? t("common.yes") : t("common.no")}</strong>
            </div>
            <div>
              <span className="muted">{t("adf.capacity")}</span> {fmt(info.capacity_bytes)}
            </div>
            <div>
              <span className="muted">{t("adf.used")}</span> {fmt(info.used_bytes)}
            </div>
            <div>
              <span className="muted">{t("adf.free")}</span> {fmt(info.free_bytes)}
            </div>
            <div>
              <span className="muted">{t("adf.filesCount")}</span> {info.file_count}
            </div>
            <div>
              <span className="muted">{t("adf.dirsCount")}</span> {info.directory_count}
            </div>
            <div>
              <span className="muted">{t("adf.checksum")}</span>{" "}
              <span className={`badge ${info.checksum_valid ? "badge-ok" : "badge-warn"}`}>
                {info.checksum_valid ? t("common.ok") : t("adf.checksumMismatch")}
              </span>
            </div>
          </div>

          {report && report.findings.length > 0 && (
            <div style={{ marginTop: 12 }}>
              <span
                className={`badge ${
                  report.status === "healthy"
                    ? "badge-ok"
                    : report.status === "warning"
                    ? "badge-warn"
                    : "badge-err"
                }`}
              >
                {t(`status.${report.status}`)}
              </span>
              <ul style={{ margin: "8px 0 0", paddingLeft: 20, fontSize: 12 }}>
                {report.findings.map((f, i) => (
                  <li key={i} className="muted">
                    {f.message}
                  </li>
                ))}
              </ul>
            </div>
          )}
        </section>
      )}

      {/* Dual Option Banner for Custom/Non-DOS or Warning Disks */}
      {hasWarningOrProblem && info && (
        <div className="dual-choice-banner">
          <div style={{ fontWeight: 600, fontSize: 14 }}>
            ⚠️ {t("adf.warning.title")}
          </div>
          <p className="muted" style={{ margin: "6px 0 0", fontSize: 13 }}>
            {t("adf.warning.description")}
          </p>
          <div className="dual-choice-actions">
            <button
              className={`btn btn-sm ${showHexMode ? "btn-primary" : ""}`}
              onClick={() => setShowHexMode(!showHexMode)}
            >
              🔍 {showHexMode ? t("adf.warning.hideHex") : t("adf.warning.option1")}
            </button>
            <button
              className="btn btn-sm"
              onClick={() => navigate("/tools")}
            >
              🔬 {t("adf.warning.option2")}
            </button>
          </div>

          {showHexMode && (
            <div className="hex-view">
              <div style={{ color: "#79c0ff", marginBottom: 6 }}>
                {t("adf.hex.blockLabel", { n: info.root_block.toString().padStart(4, "0") })}
              </div>
              <div className="hex-line">
                <span className="hex-offset">00000000</span>
                <span className="hex-bytes">44 4F 53 00  00 00 00 00  00 00 03 70  00 00 00 00</span>
                <span className="hex-ascii">DOS........p....</span>
              </div>
              <div className="hex-line">
                <span className="hex-offset">00000010</span>
                <span className="hex-bytes">4E 75 00 00  00 00 00 00  00 00 00 00  00 00 00 00</span>
                <span className="hex-ascii">Nu..............</span>
              </div>
            </div>
          )}
        </div>
      )}

      {/* Directory Browser with Actions */}
      {info && (
        <section className="card">
          {/* Action Bar (Add File / New Folder) */}
          <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center", marginBottom: 10, flexWrap: "wrap", gap: 8 }}>
            <div style={{ fontWeight: 600, fontSize: 14 }}>
              📁 {t("adf.dirContents")}
            </div>
            <div style={{ display: "flex", gap: 8 }}>
              <button
                className="btn btn-sm"
                onClick={() => setShowMkdirModal(true)}
                disabled={busy}
              >
                📁 {t("adf.newFolder")}
              </button>
              <button
                className="btn btn-primary btn-sm"
                onClick={handleAddFile}
                disabled={busy}
              >
                📥 {t("adf.addFile")}
              </button>
            </div>
          </div>

          {/* Breadcrumb Navigation Bar */}
          <div className="breadcrumb-bar">
            {breadcrumbs.map((b, idx) => {
              const isLast = idx === breadcrumbs.length - 1;
              return (
                <span key={idx} style={{ display: "inline-flex", alignItems: "center", gap: 6 }}>
                  {idx > 0 && <span className="breadcrumb-separator">/</span>}
                  <button
                    className={`breadcrumb-item ${isLast ? "active" : ""}`}
                    onClick={() => !isLast && navigateToDir(b.block, b.name)}
                    disabled={isLast || busy}
                  >
                    {idx === 0 ? "💾 " : "📁 "}
                    {b.name}
                  </button>
                </span>
              );
            })}

            {breadcrumbs.length > 1 && (
              <button
                className="btn btn-sm"
                style={{ marginLeft: "auto" }}
                onClick={() => {
                  const parentIdx = breadcrumbs.length - 2;
                  const parent = breadcrumbs[parentIdx];
                  navigateToDir(parent.block, parent.name);
                }}
                disabled={busy}
              >
                ⬆ {t("adf.upOneLevel")}
              </button>
            )}
          </div>

          {/* Entries Table */}
          {entries.length === 0 ? (
            <p className="muted" style={{ padding: "16px 0", margin: 0, textAlign: "center" }}>
              {t("adf.emptyDir", { addFile: t("adf.addFile"), newFolder: t("adf.newFolder") })}
            </p>
          ) : (
            <div className="file-list-container">
              {entries.map((e) => {
                const isDir = e.kind === "directory";
                return (
                  <div key={e.header_block} className="file-row">
                    <div className="file-row-main">
                      <span className="file-row-icon">{isDir ? "📁" : "📄"}</span>
                      {isDir ? (
                        <button
                          className="file-row-dir-btn"
                          onClick={() => navigateToDir(e.header_block, e.name)}
                          disabled={busy}
                        >
                          <span className="file-row-name">{e.name}</span>
                          <span className="badge badge-muted">{t("adf.folderBadge")}</span>
                        </button>
                      ) : (
                        <span className="file-row-name">{e.name}</span>
                      )}
                    </div>

                    <div className="file-row-meta">
                      <span className="file-row-size">
                        {isDir ? "-" : fmt(e.byte_size)}
                      </span>
                      <span className="file-row-date">
                        {e.unix_date > 0
                          ? new Date(e.unix_date * 1000).toLocaleDateString()
                          : "-"}
                      </span>
                      {isDir ? (
                        <button
                          className="btn btn-sm"
                          onClick={() => navigateToDir(e.header_block, e.name)}
                          disabled={busy}
                        >
                          {t("common.open")}
                        </button>
                      ) : (
                        <button
                          className="btn btn-sm"
                          onClick={() => extract(e)}
                          disabled={busy}
                        >
                          {t("adf.extract")}
                        </button>
                      )}
                      <button
                        className="btn btn-sm"
                        title={t("adf.renameTitle")}
                        onClick={() => {
                          setRenameTarget(e);
                          setRenameValue(e.name);
                        }}
                        disabled={busy}
                      >
                        ✏️
                      </button>
                      <button
                        className="btn btn-sm btn-warn"
                        title={t("adf.deleteTitle")}
                        onClick={() => setDeleteTarget(e)}
                        disabled={busy}
                      >
                        🗑️
                      </button>
                    </div>
                  </div>
                );
              })}
            </div>
          )}
        </section>
      )}

      {!path && !busy && (
        <p className="muted" style={{ marginTop: 24, textAlign: "center" }}>
          {t("adf.noDiskOpen")}
        </p>
      )}

      {/* Modal: Create New ADF */}
      {showNewAdfModal && (
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
          <div className="card" style={{ width: 420, maxWidth: "90vw" }}>
            <h3>➕ {t("adf.modal.newAdfTitle")}</h3>
            <div style={{ display: "flex", flexDirection: "column", gap: 12, marginTop: 12 }}>
              <div>
                <label className="muted" style={{ fontSize: 12, display: "block", marginBottom: 4 }}>
                  {t("adf.modal.volumeName")}
                </label>
                <input
                  type="text"
                  value={newVolumeName}
                  onChange={(e) => setNewVolumeName(e.target.value)}
                  style={{
                    width: "100%",
                    padding: "8px",
                    background: "var(--bg)",
                    border: "1px solid var(--border)",
                    color: "var(--text)",
                    borderRadius: 4,
                  }}
                />
              </div>

              <div>
                <label className="muted" style={{ fontSize: 12, display: "block", marginBottom: 4 }}>
                  {t("adf.modal.fsType")}
                </label>
                <select
                  value={newFsType}
                  onChange={(e) => setNewFsType(e.target.value as "ofs" | "ffs")}
                  style={{
                    width: "100%",
                    padding: "8px",
                    background: "var(--bg)",
                    border: "1px solid var(--border)",
                    color: "var(--text)",
                    borderRadius: 4,
                  }}
                >
                  <option value="ffs">{t("adf.modal.ffsOption")}</option>
                  <option value="ofs">{t("adf.modal.ofsOption")}</option>
                </select>
              </div>

              <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
                <input
                  type="checkbox"
                  id="bootable-chk"
                  checked={newBootable}
                  onChange={(e) => setNewBootable(e.target.checked)}
                />
                <label htmlFor="bootable-chk" style={{ cursor: "pointer", fontSize: 13 }}>
                  {t("adf.modal.bootableLabel")}
                </label>
              </div>

              <div style={{ display: "flex", justifyContent: "flex-end", gap: 8, marginTop: 12 }}>
                <button className="btn" onClick={() => setShowNewAdfModal(false)}>
                  {t("common.cancel")}
                </button>
                <button
                  className="btn btn-primary"
                  onClick={handleCreateNewAdf}
                  disabled={!newVolumeName.trim()}
                >
                  {t("adf.modal.createSave")}
                </button>
              </div>
            </div>
          </div>
        </div>
      )}

      {/* Modal: New Folder */}
      {showMkdirModal && (
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
          <div className="card" style={{ width: 380, maxWidth: "90vw" }}>
            <h3>📁 {t("adf.modal.newFolderTitle")}</h3>
            <div style={{ marginTop: 12 }}>
              <input
                type="text"
                placeholder={t("adf.modal.folderNamePlaceholder")}
                value={newFolderName}
                onChange={(e) => setNewFolderName(e.target.value)}
                autoFocus
                style={{
                  width: "100%",
                  padding: "8px",
                  background: "var(--bg)",
                  border: "1px solid var(--border)",
                  color: "var(--text)",
                  borderRadius: 4,
                }}
              />
            </div>
            <div style={{ display: "flex", justifyContent: "flex-end", gap: 8, marginTop: 16 }}>
              <button className="btn" onClick={() => setShowMkdirModal(false)}>
                {t("common.cancel")}
              </button>
              <button
                className="btn btn-primary"
                onClick={handleCreateFolder}
                disabled={!newFolderName.trim()}
              >
                {t("adf.modal.createFolder")}
              </button>
            </div>
          </div>
        </div>
      )}

      {/* Modal: Rename */}
      {renameTarget && (
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
          <div className="card" style={{ width: 380, maxWidth: "90vw" }}>
            <h3>✏️ {t("adf.modal.renameEntryTitle")}</h3>
            <div style={{ marginTop: 12 }}>
              <input
                type="text"
                value={renameValue}
                onChange={(e) => setRenameValue(e.target.value)}
                autoFocus
                style={{
                  width: "100%",
                  padding: "8px",
                  background: "var(--bg)",
                  border: "1px solid var(--border)",
                  color: "var(--text)",
                  borderRadius: 4,
                }}
              />
            </div>
            <div style={{ display: "flex", justifyContent: "flex-end", gap: 8, marginTop: 16 }}>
              <button className="btn" onClick={() => setRenameTarget(null)}>
                {t("common.cancel")}
              </button>
              <button
                className="btn btn-primary"
                onClick={handleRenameConfirm}
                disabled={!renameValue.trim() || renameValue.trim() === renameTarget.name}
              >
                {t("adf.renameTitle")}
              </button>
            </div>
          </div>
        </div>
      )}

      {/* Modal: Delete Confirmation */}
      {deleteTarget && (
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
          <div className="card" style={{ width: 400, maxWidth: "90vw" }}>
            <h3 style={{ color: "var(--err)" }}>🗑️ {t("adf.modal.deleteTitle")}</h3>
            <p className="muted" style={{ margin: "8px 0 16px", fontSize: 13 }}>
              {t("adf.modal.deleteConfirm", { name: deleteTarget.name })}
            </p>
            <div style={{ display: "flex", justifyContent: "flex-end", gap: 8 }}>
              <button className="btn" onClick={() => setDeleteTarget(null)}>
                {t("common.cancel")}
              </button>
              <button className="btn btn-warn" onClick={handleDeleteConfirm}>
                {t("adf.modal.deleteButton")}
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}

function fmt(n: number): string {
  if (n < 1024) return `${n} B`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KB`;
  return `${(n / (1024 * 1024)).toFixed(2)} MB`;
}
