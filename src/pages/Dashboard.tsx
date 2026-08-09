import { useEffect, useState } from "react";
import { useOutletContext, useNavigate } from "react-router-dom";
import { useTranslation } from "react-i18next";

import { DropZone } from "@/components/DropZone";
import { useSettingsStore } from "@/stores/settingsStore";
import { useRecentFilesStore } from "@/stores/recentFilesStore";
import { runWorkflow } from "@/lib/api";
import { usePowerMode } from "@/lib/uxmode";
import type { DroppedAnalysis, WorkflowInfo, WorkflowOutcome } from "@/types";

interface LayoutContext {
  analyses: DroppedAnalysis[];
  dragOver: boolean;
}

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
              🎯 What can I do? — Universal Workflow Engine
            </h2>
            <span className="badge badge-ok">
              {analyses.length} {analyses.length === 1 ? "Object Analyzed" : "Objects Analyzed"}
            </span>
          </div>

          <p className="muted" style={{ fontSize: 13, margin: "6px 0 16px" }}>
            ART has analyzed the dropped objects and ranked the recommended Amiga workflows:
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
          {[
            { key: "nav.diskTools", hint: "ADF", to: "/disk-tools" },
            { key: "nav.archiveTools", hint: "LHA", to: "/archive-tools" },
            { key: "nav.hardDisk", hint: "HDF", to: "/hard-disk" },
            { key: "nav.collection", hint: "Library", to: "/collection" },
            { key: "nav.gotek", hint: "USB", to: "/gotek" },
            { key: "nav.rom", hint: "Kickstart", to: "/rom" },
            { key: "nav.winuae", hint: "Emulator", to: "/winuae" },
            // Hex Tools is Power User territory (§47); hidden, not disabled.
            ...(powerMode
              ? [{ key: "nav.tools", hint: "Raw data", to: "/tools" }]
              : []),
          ].map((qa) => (
            <button
              key={qa.to}
              className="quick-action"
              onClick={() => navigate(qa.to)}
            >
              <span className="quick-action-label">{t(qa.key)}</span>
              <span className="quick-action-hint">{qa.hint}</span>
            </button>
          ))}
        </div>
      </section>

      <p className="faint" style={{ fontSize: 11 }}>
        {t("phase0.engineReady")} · {t("phase0.dragDropReady")} ·{" "}
        {t("phase0.dbReady")} · theme: {theme}
      </p>
    </div>
  );
}

function InteractiveDropResultCard({ analysis }: { analysis: DroppedAnalysis }) {
  const navigate = useNavigate();
  const powerMode = usePowerMode();
  const [outcome, setOutcome] = useState<WorkflowOutcome | null>(null);
  const [busy, setBusy] = useState(false);

  if (!analysis.ok || !analysis.plan) {
    return (
      <div className="card" style={{ background: "var(--bg-elevated)" }}>
        <span className="badge badge-err">{analysis.error ?? "error"}</span>
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
        message: String(e),
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
            : `${action.description} — coming in a later phase`
        }
      >
        {primary ? "⭐ " : ""}
        {action.name}
        {!action.available && " (Coming Later)"}
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
              {powerMode && `Confidence: ${Math.round(detection.confidence * 100)}% · `}
              Size: {formatBytes(detection.size)}
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
          ART does not recognise this file yet, so it has no actions to offer.
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
                Advanced
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
              ? "Verification: PASS"
              : outcome.verification === false
              ? "Verification: FAILED"
              : outcome.success
              ? "Done"
              : "Failed"}
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
