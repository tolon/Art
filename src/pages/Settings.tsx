import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { save } from "@tauri-apps/plugin-dialog";

import { useSettingsStore } from "@/stores/settingsStore";
import { LANGUAGE_NAMES, SUPPORTED_LANGUAGES } from "@/i18n";
import type { Theme, UxMode } from "@/lib/settings";
import {
  oplogExportTo,
  oplogPath,
  oplogRecent,
  statusLabel,
  succeeded,
  type OperationRecord,
} from "@/lib/oplog";

export function SettingsPage() {
  const { t } = useTranslation();
  const settings = useSettingsStore((s) => s.settings);
  const update = useSettingsStore((s) => s.update);

  return (
    <div style={{ maxWidth: 640, display: "flex", flexDirection: "column", gap: 20 }}>
      <h1 style={{ fontSize: 20 }}>{t("settings.title")}</h1>

      <section className="card">
        <h2 style={{ fontSize: 15 }}>{t("settings.appearance")}</h2>
        <Field label={t("settings.theme")}>
          <select
            className="btn"
            value={settings.theme}
            onChange={(e) => void update({ theme: e.target.value as Theme })}
          >
            <option value="dark">{t("settings.themeDark")}</option>
            <option value="light">{t("settings.themeLight")}</option>
          </select>
        </Field>
        <Field label={t("settings.uxMode")}>
          <select
            className="btn"
            value={settings.uxMode}
            onChange={(e) => void update({ uxMode: e.target.value as UxMode })}
          >
            <option value="beginner">{t("settings.uxBeginner")}</option>
            <option value="power">{t("settings.uxPower")}</option>
          </select>
          <p className="faint" style={{ fontSize: 11, margin: "4px 0 0" }}>
            {settings.uxMode === "power"
              ? t("settings.uxModePowerHint")
              : t("settings.uxModeBeginnerHint")}
          </p>
        </Field>
      </section>

      <section className="card">
        <h2 style={{ fontSize: 15 }}>{t("settings.general")}</h2>
        <Field label={t("settings.language")}>
          <select
            className="btn"
            value={settings.language}
            onChange={(e) => void update({ language: e.target.value })}
          >
            {SUPPORTED_LANGUAGES.map((lng) => (
              <option key={lng} value={lng}>
                {LANGUAGE_NAMES[lng]}
              </option>
            ))}
          </select>
        </Field>
      </section>

      <section className="card">
        <h2 style={{ fontSize: 15 }}>{t("settings.paths")}</h2>
        <Field label={t("settings.winuaePath")}>
          <input
            className="btn"
            style={{ width: "100%" }}
            placeholder="C:\\WinUAE\\winuae64.exe"
            value={settings.winuaePath ?? ""}
            onChange={(e) => void update({ winuaePath: e.target.value || null })}
          />
        </Field>
        <Field label={t("settings.collectionDir")}>
          <input
            className="btn"
            style={{ width: "100%" }}
            placeholder="D:\\Amiga"
            value={settings.lastCollectionDir ?? ""}
            onChange={(e) =>
              void update({ lastCollectionDir: e.target.value || null })
            }
          />
        </Field>
      </section>

      <OperationLogSection />
    </div>
  );
}

/**
 * The record of everything ART did to the user's data (spec §53).
 *
 * Kept in Settings because it is diagnostics, not a workflow: this is where a
 * user comes to check what happened, find where a backup went, or copy a
 * failure with its error ID into a bug report.
 */
function OperationLogSection() {
  const { t } = useTranslation();
  const [records, setRecords] = useState<OperationRecord[] | null>(null);
  const [logPath, setLogPath] = useState<string>("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    void (async () => {
      try {
        const [recent, path] = await Promise.all([oplogRecent(20), oplogPath()]);
        setRecords(recent);
        setLogPath(path);
      } catch (e) {
        setError(String(e));
      }
    })();
  }, []);

  async function handleExport() {
    setBusy(true);
    setError(null);
    try {
      const target = await save({
        title: t("settings.oplog.exportDialogTitle"),
        defaultPath: "art-operation-log.txt",
        filters: [{ name: t("settings.oplog.textFileType"), extensions: ["txt"] }],
      });
      if (typeof target === "string") {
        await oplogExportTo(target);
      }
    } catch (e) {
      setError(t("settings.oplog.exportFailed", { error: String(e) }));
    } finally {
      setBusy(false);
    }
  }

  return (
    <section className="card">
      <h2 style={{ fontSize: 15 }}>{t("settings.oplog.title")}</h2>
      <p className="muted" style={{ fontSize: 12, marginTop: 4 }}>
        {t("settings.oplog.description")}
      </p>

      {error && (
        <p className="badge badge-err" style={{ display: "inline-block" }}>
          {error}
        </p>
      )}

      {records === null ? (
        <p className="faint" style={{ fontSize: 12 }}>
          {t("settings.oplog.loading")}
        </p>
      ) : records.length === 0 ? (
        <p className="faint" style={{ fontSize: 12 }}>
          {t("settings.oplog.empty")}
        </p>
      ) : (
        <ul style={{ listStyle: "none", padding: 0, margin: "10px 0 0" }}>
          {records.map((r, i) => {
            const label = statusLabel(r);
            return (
              <li
                key={`${r.timestamp}-${i}`}
                style={{
                  padding: "6px 0",
                  borderTop: i === 0 ? "none" : "1px solid var(--border)",
                  fontSize: 12,
                }}
              >
                <span
                  className={`badge ${succeeded(r) ? "badge-ok" : "badge-err"}`}
                  style={{ marginRight: 6 }}
                >
                  {t(label.key, label.params)}
                </span>
                <strong>{r.operation}</strong>
                <span className="faint" style={{ marginLeft: 6 }}>
                  {new Date(r.timestamp * 1000).toLocaleString()}
                </span>
                {r.destination && (
                  <div className="muted" style={{ wordBreak: "break-all" }}>
                    {r.destination}
                  </div>
                )}
                {r.backup && (
                  <div className="faint" style={{ wordBreak: "break-all" }}>
                    {t("settings.oplog.backupLabel", { path: r.backup })}
                  </div>
                )}
                {r.outcome.result === "failure" && (
                  <div className="muted">
                    {r.outcome.message} ({r.outcome.error_code})
                  </div>
                )}
              </li>
            );
          })}
        </ul>
      )}

      <div style={{ marginTop: 10, display: "flex", gap: 8, alignItems: "center" }}>
        <button className="btn btn-sm" onClick={handleExport} disabled={busy}>
          {t("settings.oplog.export")}
        </button>
        {logPath && (
          <span className="faint" style={{ fontSize: 11, wordBreak: "break-all" }}>
            {logPath}
          </span>
        )}
      </div>
    </section>
  );
}

function Field({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <div style={{ margin: "10px 0", display: "flex", flexDirection: "column", gap: 4 }}>
      <label className="muted" style={{ fontSize: 12 }}>
        {label}
      </label>
      {children}
    </div>
  );
}
