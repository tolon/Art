import { useEffect, useState } from "react";
import { useOutletContext, useNavigate } from "react-router-dom";
import { useTranslation } from "react-i18next";

import { DropZone } from "@/components/DropZone";
import { useSettingsStore } from "@/stores/settingsStore";
import { useRecentFilesStore } from "@/stores/recentFilesStore";
import { runWorkflow } from "@/lib/api";
import { usePowerMode } from "@/lib/uxmode";
import type { DroppedAnalysis, WorkflowInfo, WorkflowOutcome } from "@/types";
import { errorText } from "@/lib/errorText";
import { QuickIcon, type QuickIconName } from "@/components/layout/QuickIcon";
import { NavIcon } from "@/components/layout/NavIcon";

interface LayoutContext {
  analyses: DroppedAnalysis[];
  dragOver: boolean;
}

/**
 * The six tiles from the owner-approved design canvas — the icons, labels
 * and hints `gen.py`'s `dashboard()` draws with, one for one. Not the same
 * set the old quick-actions grid carried (disk/archive tools, Gotek, ROM):
 * every one of those studios is still a full sidebar entry, so nothing here
 * is a lost route, only a shorter shortcut tray. `nav.tools` (Hex Tools)
 * stays a separate, Power-User-only tile below rather than a seventh here,
 * since it is not one of the canvas's own six and carries no two-tone icon
 * of its own.
 */
const QUICK_ACTIONS: Array<{ to: string; icon: QuickIconName; key: string; hintKey: string }> = [
  { to: "/files", icon: "files", key: "nav.files", hintKey: "dashboard.quickActionHints.filesManager" },
  {
    to: "/collection",
    icon: "collection",
    key: "nav.collection",
    hintKey: "dashboard.quickActionHints.library",
  },
  { to: "/winuae", icon: "winuae", key: "nav.winuae", hintKey: "dashboard.quickActionHints.emulator" },
  {
    to: "/hard-disk",
    icon: "card",
    key: "nav.hardDisk",
    hintKey: "dashboard.quickActionHints.hardDiskImages",
  },
  {
    to: "/os-builder",
    icon: "install",
    key: "nav.osBuilder",
    hintKey: "dashboard.quickActionHints.osTree",
  },
  {
    to: "/whdload",
    icon: "whdload",
    key: "nav.whdload",
    hintKey: "dashboard.quickActionHints.oneClickInstall",
  },
];

export function Dashboard() {
  const { t } = useTranslation();
  const navigate = useNavigate();
  const powerMode = usePowerMode();
  const context = useOutletContext<LayoutContext>() ?? { analyses: [], dragOver: false };
  const analyses = context.analyses ?? [];
  const dragOver = context.dragOver ?? false;
  const theme = useSettingsStore((s) => s.settings.theme);
  const recent = useRecentFilesStore((s) => s.files);
  const loadRecent = useRecentFilesStore((s) => s.load);

  useEffect(() => {
    void loadRecent().catch(() => {});
  }, [loadRecent]);

  return (
    <div className="dashboard-grid">
      <DropZone active={dragOver} />

      {analyses.length > 0 && (
        <section className="card" style={{ border: "1px solid var(--accent)" }}>
          <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center" }}>
            <h2 style={{ fontSize: 16, margin: 0 }}>
              🎯 {t("dashboard.plan.title")}
            </h2>
            <span className="badge badge-ok">
              {t("dashboard.plan.objectsAnalyzed", { count: analyses.length })}
            </span>
          </div>

          <p className="muted" style={{ fontSize: 13, margin: "6px 0 16px" }}>
            {t("dashboard.plan.subtitle")}
          </p>

          <div className="drop-results" style={{ display: "flex", flexDirection: "column", gap: 12 }}>
            {analyses.map((a, i) => (
              <InteractiveDropResultCard key={`${a.path}-${i}`} analysis={a} />
            ))}
          </div>
        </section>
      )}

      <section className="card">
        <h2 style={{ fontSize: 15 }}>{t("dashboard.recent")}</h2>
        {recent.length === 0 ? (
          <p className="muted">{t("dashboard.noRecent")}</p>
        ) : (
          <ul className="recent-list">
            {recent.map((f) => (
              <li
                className="recent-item"
                key={f.id}
                style={{ cursor: "pointer" }}
                onClick={() => {
                  if (f.kind === "adf" || f.kind.includes("floppy")) {
                    navigate("/disk-tools", { state: { path: f.path } });
                  } else if (f.kind === "lha" || f.kind.includes("archive")) {
                    navigate("/archive-tools", { state: { path: f.path } });
                  }
                }}
              >
                <span>
                  <span className="recent-name">{f.name}</span>{" "}
                  <span className="badge badge-muted">{f.kind}</span>
                </span>
                <span className="recent-path">{f.path}</span>
              </li>
            ))}
          </ul>
        )}
      </section>

      <section className="card">
        <h2 style={{ fontSize: 15 }}>{t("dashboard.quickActions")}</h2>
        <div className="quick-actions">
          {QUICK_ACTIONS.map((qa) => (
            <button key={qa.to} className="quick-action" onClick={() => navigate(qa.to)}>
              <span className="quick-action-icon" aria-hidden>
                <QuickIcon name={qa.icon} />
              </span>
              <span className="quick-action-text">
                <span className="quick-action-label">{t(qa.key)}</span>
                <span className="quick-action-hint">{t(qa.hintKey)}</span>
              </span>
            </button>
          ))}
          {/* Hex Tools is Power User territory (§47); hidden, not disabled.
              Not one of the canvas's own six tiles, so it keeps a plain
              stroke icon rather than a two-tone one invented for it. */}
          {powerMode && (
            <button className="quick-action" onClick={() => navigate("/tools")}>
              <span className="quick-action-icon" aria-hidden>
                <NavIcon name="wrench" />
              </span>
              <span className="quick-action-text">
                <span className="quick-action-label">{t("nav.tools")}</span>
                <span className="quick-action-hint">
                  {t("dashboard.quickActionHints.rawData")}
                </span>
              </span>
            </button>
          )}
        </div>
      </section>

      <p className="faint" style={{ fontSize: 11 }}>
        {t("phase0.engineReady")} · {t("phase0.dragDropReady")} ·{" "}
        {t("phase0.dbReady")} · {t("dashboard.footer.theme", { theme })}
      </p>
    </div>
  );
}

function InteractiveDropResultCard({ analysis }: { analysis: DroppedAnalysis }) {
  const { t } = useTranslation();
  const navigate = useNavigate();
  const powerMode = usePowerMode();
  const [outcome, setOutcome] = useState<WorkflowOutcome | null>(null);
  const [busy, setBusy] = useState(false);

  if (!analysis.ok || !analysis.plan) {
    return (
      <div className="card" style={{ background: "var(--bg-elevated)" }}>
        <span className="badge badge-err">{analysis.error ?? t("dashboard.plan.genericError")}</span>
        <span className="drop-result-path" style={{ marginLeft: 8 }}>{analysis.path}</span>
      </div>
    );
  }

  const { detection, candidates } = analysis.plan;

  // The engine decides what is offered and in what order; this component only
  // renders it and knows how to open a route (spec §46, §95).
  const starred = candidates.filter((c) => c.category === "recommended");
  const secondary = candidates.filter((c) => c.category === "standard");
  const advanced = candidates.filter((c) => c.category === "advanced");

  async function activate(action: WorkflowInfo) {
    if (!action.available) return;

    if (action.kind.kind === "navigate") {
      navigate(action.kind.route, { state: { path: analysis.path } });
      return;
    }

    setBusy(true);
    setOutcome(null);
    try {
      setOutcome(await runWorkflow(analysis.path, action.id));
    } catch (e) {
      setOutcome({
        workflow_id: action.id,
        success: false,
        message: errorText(t, e),
        verification: null,
      });
    } finally {
      setBusy(false);
    }
  }

  function ActionButton({ action, primary }: { action: WorkflowInfo; primary?: boolean }) {
    return (
      <button
        className={`btn btn-sm ${primary ? "btn-primary" : ""}`}
        onClick={() => activate(action)}
        disabled={busy || !action.available}
        title={
          action.available
            ? action.description
            : t("dashboard.plan.comingLaterTitle", { description: action.description })
        }
      >
        {primary ? "⭐ " : ""}
        {action.name}
        {!action.available && ` (${t("common.comingLater")})`}
      </button>
    );
  }

  return (
    <div
      className="card"
      style={{
        background: "var(--bg-elevated)",
        display: "flex",
        flexDirection: "column",
        gap: 10,
      }}
    >
      <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center", flexWrap: "wrap", gap: 8 }}>
        <div>
          <span className="badge badge-ok" style={{ fontSize: 12, marginRight: 6 }}>
            {detection.format_hint.toUpperCase()}
          </span>
          <span className="badge badge-muted" style={{ marginRight: 8 }}>
            {detection.category}
          </span>
          {!detection.is_dir && (
            <span className="faint" style={{ fontSize: 12 }}>
              {powerMode &&
                `${t("dashboard.plan.confidence", { percent: Math.round(detection.confidence * 100) })} · `}
              {t("dashboard.plan.size", { size: formatBytes(detection.size) })}
            </span>
          )}
        </div>
        <span className="muted" style={{ fontSize: 12, wordBreak: "break-all" }}>
          {analysis.path}
        </span>
      </div>

      {/* "What can I do?" — driven entirely by the engine's plan (spec §46) */}
      {candidates.length === 0 ? (
        <p className="muted" style={{ fontSize: 12, margin: 0 }}>
          {t("dashboard.plan.noActions")}
        </p>
      ) : (
        <>
          <div className="drop-workflow-bar">
            {starred.map((a) => (
              <ActionButton key={a.id} action={a} primary />
            ))}
            {secondary.map((a) => (
              <ActionButton key={a.id} action={a} />
            ))}
          </div>

          {powerMode && advanced.length > 0 && (
            <details>
              <summary className="faint" style={{ fontSize: 12, cursor: "pointer" }}>
                {t("dashboard.plan.advanced")}
              </summary>
              <div className="drop-workflow-bar" style={{ marginTop: 6 }}>
                {advanced.map((a) => (
                  <ActionButton key={a.id} action={a} />
                ))}
              </div>
            </details>
          )}
        </>
      )}

      {outcome && (
        <div style={{ fontSize: 12, marginTop: 4 }}>
          <span
            className={`badge ${
              !outcome.success
                ? "badge-err"
                : outcome.verification === false
                ? "badge-warn"
                : "badge-ok"
            }`}
          >
            {outcome.verification === true
              ? t("dashboard.plan.verificationPass")
              : outcome.verification === false
              ? t("dashboard.plan.verificationFailed")
              : outcome.success
              ? t("dashboard.plan.done")
              : t("dashboard.plan.failed")}
          </span>
          <p
            className="muted"
            style={{ margin: "4px 0 0", wordBreak: "break-all" }}
          >
            {outcome.message}
          </p>
        </div>
      )}
    </div>
  );
}

function formatBytes(n: number): string {
  if (n < 1024) return `${n} B`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(0)} KB`;
  return `${(n / (1024 * 1024)).toFixed(1)} MB`;
}
