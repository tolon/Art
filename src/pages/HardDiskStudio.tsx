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
import {
  bootPartition,
  cardOpen,
  isCard,
  partitionCount,
  type CardReport,
} from "@/lib/card";
import { hdfSizeWarning, parseCustomSize, type HdfFsId } from "@/lib/hdfSize";
import { partitionsMissingDriver } from "@/lib/rdbDrivers";
import { driverFileName, driverRequirement, fileSystemInputsFor } from "@/lib/fsDriver";
import {
  isFlag,
  isOneOf,
  isText,
  isTextOrNothing,
  isWholeNumberBetween,
} from "@/lib/remembered";
import { useRemembered } from "@/lib/useRemembered";
import { useOpenObject } from "@/stores/openObjectStore";

/** The filesystems the wizard offers. A remembered value that is not one of
 *  them — an older ART's, or a hand-edited file's — falls back rather than
 *  reaching `to_dostype_u32` with something it has no branch for. */
const isFilesystem = isOneOf<AmigaHardDiskFs>(
  "pfs3directscsi",
  "pfs3standard",
  "sfs0",
  "ffsdircache",
  "ffsstandard"
);

interface FsChoice {
  id: AmigaHardDiskFs;
  name: string;
  badgeType: "ok" | "muted" | "warn";
  badgeKey: string;
  descriptionKey: string;
  featureKeys: string[];
}

const FILESYSTEM_CHOICES: FsChoice[] = [
  {
    id: "pfs3directscsi",
    name: "PFS3-AIO (DirectSCSI — PDS\\3)",
    badgeType: "ok",
    badgeKey: "hardDisk.fs.pfs3.badge",
    descriptionKey: "hardDisk.fs.pfs3.description",
    featureKeys: ["hardDisk.fs.pfs3.feature1", "hardDisk.fs.pfs3.feature2", "hardDisk.fs.pfs3.feature3", "hardDisk.fs.pfs3.feature4"],
  },
  {
    id: "sfs0",
    name: "Smart File System (SFS\\0)",
    badgeType: "muted",
    badgeKey: "hardDisk.fs.sfs0.badge",
    descriptionKey: "hardDisk.fs.sfs0.description",
    featureKeys: ["hardDisk.fs.sfs0.feature1", "hardDisk.fs.sfs0.feature2", "hardDisk.fs.sfs0.feature3"],
  },
  {
    id: "ffsdircache",
    name: "Fast File System DC (DOS\\3)",
    badgeType: "muted",
    badgeKey: "hardDisk.fs.ffsdircache.badge",
    descriptionKey: "hardDisk.fs.ffsdircache.description",
    featureKeys: ["hardDisk.fs.ffsdircache.feature1", "hardDisk.fs.ffsdircache.feature2", "hardDisk.fs.ffsdircache.feature3"],
  },
  {
    id: "ffsstandard",
    name: "Fast File System (DOS\\1)",
    badgeType: "warn",
    badgeKey: "hardDisk.fs.ffsstandard.badge",
    descriptionKey: "hardDisk.fs.ffsstandard.description",
    featureKeys: ["hardDisk.fs.ffsstandard.feature1", "hardDisk.fs.ffsstandard.feature2", "hardDisk.fs.ffsstandard.feature3"],
  },
];

export function HardDiskStudio() {
  const { t } = useTranslation();
  const location = useLocation();
  const navigate = useNavigate();

  // The open image outlives this screen (ART-085), for the length of the run.
  const [path, setPath] = useOpenObject("harddisk");
  const [info, setInfo] = useState<HdfInfo | null>(null);
  /**
   * Set instead of `info` when the file turns out to be a **card** — an MBR
   * with a FAT32 boot partition and one to three Amiga disks inside it, each
   * with its own RDB at its own offset.
   *
   * The two are exclusive on purpose: a card is not one hard disk with extra
   * fields, and rendering it through the single-disk view is how ART came to
   * report a working card as broken (ART-097).
   */
  const [card, setCard] = useState<CardReport | null>(null);
  const [selectedPart, setSelectedPart] = useState<ParsedPartition | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [statusMsg, setStatusMsg] = useState<string | null>(null);

  // Wizard Modal State.
  //
  // Everything the wizard asks is remembered (`@/lib/useRemembered`). Somebody
  // building a set of disks answers the same four questions the same way every
  // time, and the driver especially: having found `pfs3aio` once, nobody should
  // have to go and find it again.
  const [showCreateModal, setShowCreateModal] = useState(false);
  const [createPresetSizeMb, setCreatePresetSizeMb] = useRemembered(
    "hdf.presetSizeMb",
    isWholeNumberBetween(10, 4 * 1024 * 1024),
    1024 // 1 GB
  );
  const [createTemplate, setCreateTemplate] = useRemembered<"single" | "split">(
    "hdf.template",
    isOneOf("single", "split"),
    "split"
  );
  const [selectedFs, setSelectedFs] = useRemembered<AmigaHardDiskFs>(
    "hdf.filesystem",
    isFilesystem,
    "pfs3directscsi"
  );
  /** Whether the size is being typed rather than picked from the five
   *  presets. The presets are common answers, not the range (ART-083). */
  const [customSize, setCustomSize] = useRemembered("hdf.customSize", isFlag, false);
  const [customText, setCustomText] = useRemembered("hdf.customText", isText, "");
  const [customUnit, setCustomUnit] = useRemembered<"mb" | "gb">(
    "hdf.customUnit",
    isOneOf("mb", "gb"),
    "gb"
  );
  /** The filesystem driver to embed in the new RDB, if one is needed. */
  const [driverPath, setDriverPath] = useRemembered<string | null>(
    "hdf.driverPath",
    isTextOrNothing,
    null
  );

  // Parsing and warning both live in `@/lib/hdfSize`, with their own tests —
  // the rule that a fraction of a megabyte is refused rather than rounded
  // away matters more than a component can prove about itself.
  const parsedCustom = parseCustomSize(customText, customUnit);
  const effectiveSizeMb = customSize
    ? parsedCustom.ok
      ? parsedCustom.mb
      : null
    : createPresetSizeMb;
  const sizeWarning =
    effectiveSizeMb === null
      ? null
      : hdfSizeWarning(effectiveSizeMb, createTemplate, selectedFs as HdfFsId);

  // Whether this filesystem is one Kickstart has. PFS3 and SFS are not, and a
  // disk that names one without carrying it is a disk an Amiga ignores in
  // silence — which is exactly what this wizard used to produce (ART-084).
  const driverNeed = driverRequirement(selectedFs);

  // Router state names a file when the Dashboard or the drop panel sent us
  // here; otherwise the studio reopens whatever it had open (ART-085). `info`
  // is null on every fresh mount, so it is what tells "nothing is loaded here"
  // apart from "nothing is open at all".
  useEffect(() => {
    const fromNav = (location.state as { path?: string } | undefined)?.path;
    const target = fromNav ?? path;
    if (target && (info === null || target !== path)) {
      void loadHdf(target);
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [location.state]);

  async function loadHdf(p: string) {
    setBusy(true);
    setError(null);
    setStatusMsg(null);
    try {
      // Ask the card reader first, whatever the file is. It answers for both
      // kinds (`core/card.rs`): a plain HDF comes back as one area at offset
      // zero with no partition table, so this branches on what was *found*
      // rather than on the extension or on a guess. It matters because
      // `hdf_open` cannot open a card at all — it looks for the RDB in the
      // first blocks of the file, and on a card those are the MBR and the
      // FAT32 boot partition, with the Amiga's own table about a gigabyte
      // further in (ART-095).
      const report = await cardOpen(p);
      if (isCard(report)) {
        setCard(report);
        setInfo(null);
        setSelectedPart(report.card.areas[0]?.rdb.partitions[0] ?? null);
        setPath(p);
        return;
      }

      setCard(null);
      const hdfInfo = await hdfOpen(p);
      setInfo(hdfInfo);
      setPath(p);
      if (hdfInfo.partitions.length > 0) {
        setSelectedPart(hdfInfo.partitions[0]);
      }
    } catch (e) {
      setError(String(e));
      setInfo(null);
      setCard(null);
    } finally {
      setBusy(false);
    }
  }

  async function handleOpen() {
    const sel = await open({
      multiple: false,
      filters: [{ name: "Hard Disk File (HDF / IMG)", extensions: ["hdf", "img"] }],
      title: t("hardDisk.openDialogTitle"),
    });
    if (typeof sel === "string") {
      await loadHdf(sel);
    }
  }

  async function handlePickDriver() {
    // No extension filter: an Amiga executable has no extension, and one
    // would only hide the file the user came here to pick.
    const sel = await open({
      multiple: false,
      title: t("hardDisk.modal.driver.dialogTitle"),
    });
    if (typeof sel === "string") {
      setDriverPath(sel);
    }
  }

  async function handleCreateConfirm() {
    // `effectiveSizeMb` is null exactly when a typed size has not parsed, and
    // the confirm button is disabled then — this is the belt to that braces,
    // so a stray call can never reach `hdfCreate` with a guessed number.
    const sizeMb = effectiveSizeMb;
    if (sizeMb === null) return;

    setShowCreateModal(false);
    const defaultName = `Amiga_${sizeMb >= 1024 ? `${Math.round((sizeMb / 1024) * 10) / 10}GB` : `${sizeMb}MB`}.hdf`;
    const dest = await save({
      defaultPath: defaultName,
      filters: [{ name: "Hard Disk File (HDF)", extensions: ["hdf"] }],
      title: t("hardDisk.saveDialogTitle"),
    });
    if (!dest) return;

    setBusy(true);
    setError(null);
    setStatusMsg(null);

    try {
      // No `num_buffers` anywhere below, and that is the fix for ART-096: the
      // screen has never asked the user for a buffer count, so it has nothing
      // to say about one. The core fills in the measured default (600); the
      // three literal 100s that used to sit here silently outvoted it.
      const partitions: PartitionSpec[] = [];
      if (createTemplate === "single") {
        partitions.push({
          drive_name: "DH0",
          fs_type: selectedFs,
          size_mb: sizeMb,
          bootable: true,
          boot_priority: 0,
        });
      } else {
        // Split: 500 MB (or 25%) System + Rest Work
        const sysMb = Math.min(500, Math.floor(sizeMb / 3));
        const workMb = sizeMb - sysMb;
        partitions.push({
          drive_name: "DH0",
          fs_type: selectedFs,
          size_mb: sysMb,
          bootable: true,
          boot_priority: 5,
        });
        partitions.push({
          drive_name: "DH1",
          fs_type: selectedFs,
          size_mb: workMb,
          bootable: false,
          boot_priority: 0,
        });
      }

      const totalBytes = (sizeMb as number) * 1024 * 1024;
      const created = await hdfCreate(
        dest,
        totalBytes,
        true,
        partitions,
        fileSystemInputsFor(selectedFs, driverPath)
      );
      setInfo(created);
      setPath(dest);
      setStatusMsg(
        t("hardDisk.msgCreated", { dest, size: sizeMb, count: partitions.length })
      );
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

  /** Partitions naming a filesystem this disk does not carry — the question
   *  ART could not answer before it read FSHD/LSEG. */
  const missingDriver = info ? partitionsMissingDriver(info) : [];

  return (
    <div>
      <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center" }}>
        <h1 style={{ fontSize: 20, margin: 0 }}>{t("nav.hardDisk")} — {t("hardDisk.title")}</h1>
        <div style={{ display: "flex", gap: 8, flexWrap: "wrap" }}>
          {path && (
            <button
              className="btn btn-sm btn-primary"
              onClick={() => navigate("/winuae", { state: { path } })}
              disabled={busy}
            >
              🚀 {t("common.launchInWinuae")}
            </button>
          )}
          <button
            className="btn btn-sm"
            onClick={() => setShowCreateModal(true)}
            disabled={busy}
          >
            ➕ {t("hardDisk.newWizard")}
          </button>
          <button className="btn btn-sm" onClick={handleOpen} disabled={busy}>
            {t("hardDisk.openHdf")}
          </button>
        </div>
      </div>

      {path && (
        <div style={{ margin: "8px 0 12px", fontSize: 12 }}>
          <span className="muted">{t("hardDisk.diskImageLabel")}</span>{" "}
          <strong style={{ wordBreak: "break-all" }}>{path}</strong>
        </div>
      )}

      {error && <div className="badge badge-err" style={{ marginBottom: 12, padding: "6px 12px" }}>{error}</div>}
      {statusMsg && <div className="badge badge-ok" style={{ marginBottom: 12, padding: "6px 12px" }}>{statusMsg}</div>}
      {busy && <div className="muted" style={{ marginBottom: 12 }}>{t("hardDisk.working")}</div>}

      {/* A card, which is a *list of disks* rather than a disk (ART-095,
          ART-097). Everything here is read-only and says so: ART can read a
          card today and cannot write one, and a screen that left that
          ambiguous would be the ART-090 mistake again. */}
      {card && (
        <div style={{ display: "flex", flexDirection: "column", gap: 16 }}>
          <section className="card">
            <div style={{ display: "flex", justifyContent: "space-between", alignItems: "baseline" }}>
              <h2 style={{ fontSize: 16 }}>💳 {t("hardDisk.card.title")}</h2>
              <span className="badge">{t("hardDisk.card.readOnly")}</span>
            </div>

            <div
              style={{
                display: "grid",
                gridTemplateColumns: "repeat(auto-fit, minmax(160px, 1fr))",
                gap: 12,
                fontSize: 13,
                marginTop: 12,
              }}
            >
              <div>
                <span className="muted">{t("hardDisk.capacity")}</span>{" "}
                <strong>{fmtBytes(card.card.total_bytes)}</strong>
              </div>
              <div>
                <span className="muted">{t("hardDisk.card.disksLabel")}</span>{" "}
                {card.card.areas.length}
              </div>
              <div>
                <span className="muted">{t("hardDisk.partitionsLabel")}</span>{" "}
                {partitionCount(card)}
              </div>
              <div>
                <span className="muted">{t("hardDisk.card.bootLabel")}</span>{" "}
                {bootPartition(card)
                  ? fmtBytes(bootPartition(card)!.sector_count * 512)
                  : t("hardDisk.card.noBoot")}
              </div>
            </div>

            {/* The four primary slots, as the card's own documentation numbers
                them — a listing that renumbers disagrees with the user's notes. */}
            <table style={{ width: "100%", fontSize: 12, marginTop: 12 }}>
              <thead>
                <tr className="muted" style={{ textAlign: "left" }}>
                  <th>{t("hardDisk.card.slot")}</th>
                  <th>{t("hardDisk.card.type")}</th>
                  <th>{t("hardDisk.card.start")}</th>
                  <th>{t("hardDisk.partitionSize")}</th>
                </tr>
              </thead>
              <tbody>
                {card.card.mbr?.partitions.map((p) => (
                  <tr key={p.index}>
                    <td>{p.index + 1}</td>
                    <td>
                      {p.kind.kind === "fat32"
                        ? t("hardDisk.card.kind.fat32")
                        : p.kind.kind === "amiga-rdb"
                          ? t("hardDisk.card.kind.amiga")
                          : t("hardDisk.card.kind.other", {
                              code: `0x${p.type_byte.toString(16).toUpperCase()}`,
                            })}
                    </td>
                    <td>{fmtBytes(p.start_lba * 512)}</td>
                    <td>{fmtBytes(p.sector_count * 512)}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </section>

          {/* One section per Amiga disk. The m68k side sees each `0x76` area
              as a separate drive, so this is not a formatting choice — it is
              what the machine sees. */}
          {card.card.areas.map((area, index) => (
            <section className="card" key={area.offset_bytes}>
              <div style={{ display: "flex", justifyContent: "space-between", alignItems: "baseline" }}>
                <h2 style={{ fontSize: 15 }}>
                  🗄️ {t("hardDisk.card.disk", { n: index + 1 })}
                </h2>
                <span className={`badge ${area.rdb.checksum_valid ? "badge-ok" : "badge-warn"}`}>
                  {area.rdb.checksum_valid
                    ? t("hardDisk.checksum.ok")
                    : t("hardDisk.checksum.invalid")}
                </span>
              </div>
              <p className="muted" style={{ fontSize: 12, marginTop: 4 }}>
                {t("hardDisk.card.diskAt", {
                  offset: fmtBytes(area.offset_bytes),
                  size: area.length_bytes > 0 ? fmtBytes(area.length_bytes) : "—",
                })}
              </p>

              <ul style={{ listStyle: "none", padding: 0, margin: "10px 0 0", fontSize: 13 }}>
                {area.rdb.partitions.map((p) => (
                  <li key={`${p.block_location}-${p.drive_name}`}>
                    <button
                      className="btn btn-sm"
                      style={{ width: "100%", textAlign: "left", marginBottom: 4 }}
                      onClick={() => setSelectedPart(p)}
                    >
                      <strong>{p.drive_name}:</strong>{" "}
                      <code>{p.dostype_str}</code>{" "}
                      <span className="muted">{fmtBytes(p.size_bytes)}</span>
                      {p.bootable && (
                        <span className="badge badge-ok" style={{ marginLeft: 6, fontSize: 10 }}>
                          {t("hardDisk.bootablePri", { n: p.boot_priority })}
                        </span>
                      )}
                    </button>
                  </li>
                ))}
              </ul>
            </section>
          ))}

          {/* Drivers are the **card's**, not the area's. MultibootOS 2.2
              carries PFS3 in its first RDB and not its second, and all fifteen
              of its partitions are PFS3 — asking one area in isolation named
              them all as broken (ART-097). The union is computed in Rust; this
              only renders it. */}
          <section className="card">
            <h2 style={{ fontSize: 15 }}>{t("hardDisk.card.driversTitle")}</h2>
            {card.file_systems.length === 0 ? (
              <p className="muted" style={{ fontSize: 12, marginTop: 6 }}>
                {t("hardDisk.drivers.none")}
              </p>
            ) : (
              <ul style={{ listStyle: "none", padding: 0, margin: "8px 0 0", fontSize: 13 }}>
                {card.file_systems.map((fs, i) => (
                  <li key={i} style={{ padding: "3px 0" }}>
                    <code>{fs.dos_type_str}</code>{" "}
                    <span className="muted">
                      {t("hardDisk.drivers.entry", {
                        version: `${fs.version}.${fs.revision}`,
                        size: fmtBytes(fs.size_bytes),
                        blocks: fs.segment_blocks,
                      })}
                    </span>
                  </li>
                ))}
              </ul>
            )}

            {card.unmountable.length > 0 && (
              <p className="badge badge-warn" style={{ display: "block", marginTop: 10 }}>
                {t("hardDisk.card.unmountable", {
                  count: card.unmountable.length,
                  names: card.unmountable
                    .map((p) =>
                      t("hardDisk.card.unmountableEntry", {
                        name: p.drive_name,
                        dosType: p.dostype_str,
                        n: p.area + 1,
                      })
                    )
                    .join(", "),
                })}
              </p>
            )}
          </section>
        </div>
      )}

      {/* Disk Overview & Visual Partition Bar */}
      {info && (
        <section className="card" style={{ marginBottom: 16 }}>
          <div style={{ display: "flex", justifyContent: "space-between", alignItems: "baseline" }}>
            <h2 style={{ fontSize: 16 }}>
              {info.hdf_type === "rdb" ? t("hardDisk.driveType.rdb") : t("hardDisk.driveType.raw")}
            </h2>
            <span className={`badge ${info.rdb_checksum_valid ? "badge-ok" : "badge-warn"}`}>
              {info.rdb_checksum_valid ? t("hardDisk.checksum.ok") : t("hardDisk.checksum.invalid")}
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
            <div><span className="muted">{t("hardDisk.capacity")}</span> <strong>{fmtBytes(info.total_bytes)}</strong></div>
            <div><span className="muted">{t("hardDisk.partitionsLabel")}</span> {info.partitions.length}</div>
            <div><span className="muted">{t("hardDisk.cylinders")}</span> {info.cylinders}</div>
            <div><span className="muted">{t("hardDisk.headsSectors")}</span> {info.heads} / {info.sectors}</div>
          </div>

          {/* Visual Disk Map Bar */}
          <div style={{ marginTop: 16 }}>
            <div style={{ display: "flex", justifyContent: "space-between", fontSize: 12, marginBottom: 4 }}>
              <span className="muted">{t("hardDisk.visualLayout")}</span>
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
            <h2 style={{ fontSize: 15 }}>🗄️ {t("hardDisk.partitionsTable", { count: info.partitions.length })}</h2>
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
                            {t("hardDisk.bootablePri", { n: p.boot_priority })}
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

          {/* What drivers this disk carries, and which partitions are left
              without one (G4's reading half).

              The banner is the point: a `PDS\3` partition with no `PDS\3`
              driver in the RDB is one an Amiga ignores in silence — no error,
              no icon, nothing to search for. Until ART could read FSHD/LSEG it
              could only warn in general (ART-084); now it can name the
              partition. */}
          {info.hdf_type === "rdb" && (
            <section className="card">
              <h2 style={{ fontSize: 15 }}>{t("hardDisk.drivers.title")}</h2>
              {info.file_systems.length === 0 ? (
                <p className="muted" style={{ fontSize: 12, marginTop: 6 }}>
                  {t("hardDisk.drivers.none")}
                </p>
              ) : (
                <ul style={{ listStyle: "none", padding: 0, margin: "8px 0 0", fontSize: 13 }}>
                  {info.file_systems.map((fs, i) => (
                    <li key={i} style={{ padding: "3px 0" }}>
                      <code>{fs.dos_type_str}</code>{" "}
                      <span className="muted">
                        {t("hardDisk.drivers.entry", {
                          version: `${fs.version}.${fs.revision}`,
                          size: fmtBytes(fs.size_bytes),
                          blocks: fs.segment_blocks,
                        })}
                      </span>
                      {fs.truncated && (
                        <span className="badge badge-warn" style={{ marginLeft: 6, fontSize: 10 }}>
                          {t("hardDisk.drivers.truncated")}
                        </span>
                      )}
                    </li>
                  ))}
                </ul>
              )}

              {missingDriver.length > 0 && (
                <p className="badge badge-warn" style={{ display: "block", marginTop: 10 }}>
                  {t("hardDisk.drivers.missing", {
                    count: missingDriver.length,
                    names: missingDriver
                      .map((p) => `${p.drive_name} (${p.dostype_str})`)
                      .join(", "),
                  })}
                </p>
              )}
            </section>
          )}

        </div>
      )}

      {/* The partition inspector serves both views: a partition on a card is
          the same `ParsedPartition` an HDF's is, and reading one is the same
          act whichever kind of file it came out of. */}
      {selectedPart && (
        <div style={{ marginTop: 16 }}>
            <section className="card">
              <h2 style={{ fontSize: 15 }}>🔬 {t("hardDisk.partitionProperties")}</h2>
              <div style={{ display: "flex", flexDirection: "column", gap: 10, marginTop: 10, fontSize: 13 }}>
                <div><span className="muted">{t("hardDisk.deviceName")}</span> <strong>{selectedPart.drive_name}:</strong></div>
                <div><span className="muted">{t("hardDisk.dosTypeSignature")}</span> <code>{selectedPart.dostype_str} (0x{selectedPart.dostype.toString(16).toUpperCase()})</code></div>
                <div><span className="muted">{t("hardDisk.filesystem")}</span> {selectedPart.fs_type.toUpperCase()}</div>
                <div><span className="muted">{t("hardDisk.cylinders")}</span> {t("hardDisk.cylinderRange", { low: selectedPart.low_cyl, high: selectedPart.high_cyl, count: selectedPart.cylinder_count })}</div>
                <div><span className="muted">{t("hardDisk.partitionSize")}</span> {fmtBytes(selectedPart.size_bytes)}</div>
                <div><span className="muted">{t("hardDisk.bootableLabel")}</span> <strong>{selectedPart.bootable ? t("common.yes") : t("common.no")}</strong></div>
                <div><span className="muted">{t("hardDisk.bootPriority")}</span> {selectedPart.boot_priority}</div>
                <div><span className="muted">{t("hardDisk.buffers")}</span> {selectedPart.num_buffers}</div>
              </div>
            </section>
        </div>
      )}

      {!path && !busy && (
        <p className="muted" style={{ textAlign: "center", marginTop: 24 }}>
          {t("hardDisk.emptyState", { wizard: t("hardDisk.newWizard") })}
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
            <h3 style={{ margin: "0 0 12px" }}>➕ {t("hardDisk.modal.title")}</h3>

            {/* Step 1: Disk Capacity Presets */}
            <div style={{ marginBottom: 14 }}>
              <label className="muted" style={{ fontSize: 12, display: "block", marginBottom: 6 }}>
                {t("hardDisk.modal.selectSize")}
              </label>
              <div style={{ display: "grid", gridTemplateColumns: "repeat(auto-fit, minmax(100px, 1fr))", gap: 6 }}>
                {[
                  { size: 500, label: "500 MB", hintKey: "hardDisk.modal.sizeHints.compact" },
                  { size: 1024, label: "1 GB", hintKey: "hardDisk.modal.sizeHints.classic" },
                  { size: 2048, label: "2 GB", hintKey: "hardDisk.modal.sizeHints.whdload" },
                  { size: 4096, label: "4 GB", hintKey: "hardDisk.modal.sizeHints.standard" },
                  { size: 8192, label: "8 GB", hintKey: "hardDisk.modal.sizeHints.largeDisk" },
                ].map((s) => (
                  <button
                    key={s.size}
                    className={`btn btn-sm ${!customSize && createPresetSizeMb === s.size ? "btn-primary" : ""}`}
                    onClick={() => {
                      setCustomSize(false);
                      setCreatePresetSizeMb(s.size);
                    }}
                    style={{ flexDirection: "column", padding: "6px" }}
                  >
                    <strong>{s.label}</strong>
                    <span className="faint" style={{ fontSize: 10 }}>{t(s.hintKey)}</span>
                  </button>
                ))}
                {/* The presets are five common answers, not the range. There
                    was never an 8 GB limit in the engine — `create_rdb_layout`
                    refuses below 10 MB and then only when the cylinder count
                    will not fit a u32 (ART-083). */}
                <button
                  className={`btn btn-sm ${customSize ? "btn-primary" : ""}`}
                  onClick={() => setCustomSize(true)}
                  style={{ flexDirection: "column", padding: "6px" }}
                >
                  <strong>{t("hardDisk.modal.custom.button")}</strong>
                  <span className="faint" style={{ fontSize: 10 }}>
                    {t("hardDisk.modal.custom.hint")}
                  </span>
                </button>
              </div>

              {customSize && (
                <div style={{ display: "flex", gap: 6, alignItems: "center", marginTop: 8 }}>
                  <input
                    className="btn"
                    style={{ flex: "1 1 auto", minWidth: 0 }}
                    inputMode="decimal"
                    autoFocus
                    value={customText}
                    placeholder={t("hardDisk.modal.custom.placeholder")}
                    aria-label={t("hardDisk.modal.custom.ariaLabel")}
                    onChange={(e) => setCustomText(e.target.value)}
                  />
                  <select
                    className="btn"
                    value={customUnit}
                    aria-label={t("hardDisk.modal.custom.unitAriaLabel")}
                    onChange={(e) => setCustomUnit(e.target.value as "mb" | "gb")}
                  >
                    <option value="mb">MB</option>
                    <option value="gb">GB</option>
                  </select>
                </div>
              )}

              {/* A refusal names what is wrong instead of clamping the number
                  to something the user did not type. */}
              {customSize && !parsedCustom.ok && customText.trim() !== "" && (
                <p className="badge badge-err" style={{ display: "block", marginTop: 6, fontSize: 11 }}>
                  {t(parsedCustom.reason.key, parsedCustom.reason.params)}
                </p>
              )}

              {/* And a warning is a warning, not a block: the image may be for
                  an emulator, or for a machine that has what it needs. */}
              {sizeWarning && (
                <p className="badge badge-warn" style={{ display: "block", marginTop: 6, fontSize: 11 }}>
                  {t(sizeWarning.key, sizeWarning.params)}
                </p>
              )}
            </div>

            {/* Step 2: Partitioning Layout Template */}
            <div style={{ marginBottom: 14 }}>
              <label className="muted" style={{ fontSize: 12, display: "block", marginBottom: 6 }}>
                {t("hardDisk.modal.selectTemplate")}
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
                  <strong>⭐ {t("hardDisk.modal.splitTitle")}</strong>
                  <div className="muted" style={{ fontSize: 11, marginTop: 2 }}>
                    {t("hardDisk.modal.splitDesc")}
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
                  <strong>{t("hardDisk.modal.singleTitle")}</strong>
                  <div className="muted" style={{ fontSize: 11, marginTop: 2 }}>
                    {t("hardDisk.modal.singleDesc")}
                  </div>
                </div>
              </div>
            </div>

            {/* Step 3: Parametric Filesystem Choice with Explanations */}
            <div style={{ marginBottom: 16 }}>
              <label className="muted" style={{ fontSize: 12, display: "block", marginBottom: 6 }}>
                {t("hardDisk.modal.selectFs")}
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
                          {fs.badgeType === "ok" ? "⭐ " : ""}
                          {t(fs.badgeKey)}
                        </span>
                      </div>
                      <p className="muted" style={{ margin: "3px 0 0", fontSize: 11 }}>
                        {t(fs.descriptionKey)}
                      </p>
                    </div>
                  );
                })}
              </div>
            </div>

            {/* Step 4: the driver, for a filesystem Kickstart does not have */}
            {driverNeed.required && (
              <div style={{ marginBottom: 16 }}>
                <label
                  className="muted"
                  style={{ fontSize: 12, display: "block", marginBottom: 6 }}
                >
                  {t("hardDisk.modal.driver.label")}
                </label>
                <div style={{ display: "flex", gap: 8, alignItems: "center" }}>
                  <button className="btn" onClick={handlePickDriver}>
                    {t("hardDisk.modal.driver.browse")}
                  </button>
                  <span style={{ fontSize: 12, flex: 1, wordBreak: "break-all" }}>
                    {driverPath ? driverFileName(driverPath) : t("hardDisk.modal.driver.none")}
                  </span>
                  {driverPath && (
                    <button className="btn" onClick={() => setDriverPath(null)}>
                      {t("hardDisk.modal.driver.clear")}
                    </button>
                  )}
                </div>
                <p className="muted" style={{ margin: "6px 0 0", fontSize: 11 }}>
                  {t("hardDisk.modal.driver.explain", {
                    dosType: driverNeed.dosType,
                    hint: driverNeed.hint,
                  })}
                </p>
                {!driverPath && (
                  <p
                    className="badge badge-warn"
                    style={{ margin: "6px 0 0", fontSize: 11, display: "inline-block" }}
                  >
                    {t("hardDisk.modal.driver.warning")}
                  </p>
                )}
              </div>
            )}

            {/* Actions */}
            <div style={{ display: "flex", justifyContent: "flex-end", gap: 8 }}>
              <button className="btn" onClick={() => setShowCreateModal(false)}>
                {t("common.cancel")}
              </button>
              <button
                className="btn btn-primary"
                onClick={handleCreateConfirm}
                disabled={effectiveSizeMb === null}
                title={
                  effectiveSizeMb === null ? t("hardDisk.modal.custom.needsSize") : undefined
                }
              >
                {t("hardDisk.modal.createButton")}
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
