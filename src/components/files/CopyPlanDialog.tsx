// The pre-flight report, before a copy runs (brief §3.3, §4.1).
//
// > *Explain before modify.*
//
// The failure this exists to prevent is the drip feed: a copy that stops on
// file 37 for a long name, on 52 because the disk filled, on 61 for a
// collision. One report, real numbers, one decision.

import { useTranslation } from "react-i18next";

import {
  planIsClean,
  planShortfall,
  type CopyPlan,
} from "@/lib/volumeWrite";
import type { OverwritePolicy } from "@/lib/sources";

function formatBytes(bytes: number): string {
  if (bytes >= 1024 * 1024) return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
  if (bytes >= 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${bytes} bytes`;
}

export function CopyPlanDialog({
  plan,
  destination,
  policy,
  onPolicyChange,
  onConfirm,
  onCancel,
}: {
  plan: CopyPlan;
  destination: string;
  policy: OverwritePolicy;
  onPolicyChange: (policy: OverwritePolicy) => void;
  onConfirm: () => void;
  onCancel: () => void;
}) {
  const { t } = useTranslation();
  const fits = plan.blocks_needed <= plan.blocks_free;
  const shortfall = planShortfall(plan);

  return (
    <div
      role="dialog"
      aria-label={t("components.copyPlan.ariaLabel")}
      className="card"
      style={{
        position: "fixed",
        top: "50%",
        left: "50%",
        transform: "translate(-50%, -50%)",
        zIndex: 50,
        width: 560,
        maxWidth: "94vw",
        maxHeight: "88vh",
        overflow: "auto",
        boxShadow: "0 8px 32px rgba(0,0,0,.5)",
      }}
    >
      <strong>{t("components.copyPlan.title")}</strong>
      <div className="faint" style={{ fontSize: 11, marginTop: 2, wordBreak: "break-all" }}>
        {t("components.copyPlan.intoDestination", { destination })}
      </div>

      <div style={{ fontSize: 13, marginTop: 10 }}>
        {t("components.copyPlan.filesCount", { count: plan.files })}
        {plan.directories > 0 &&
          t("components.copyPlan.andFoldersCount", { count: plan.directories })}
        , {formatBytes(plan.total_bytes)}
      </div>

      {/* Blocks, not just bytes: a thousand one-byte files cost a thousand
          blocks, and a report in bytes alone would say "1 KB" and be wrong
          about whether it fits. */}
      <div className="faint" style={{ fontSize: 11, marginTop: 2 }}>
        {t("components.copyPlan.blocksNeededFree", {
          needed: plan.blocks_needed.toLocaleString(),
          free: plan.blocks_free.toLocaleString(),
        })}
      </div>

      {shortfall && (
        <div className="badge badge-err" style={{ display: "block", marginTop: 10 }}>
          {shortfall}
        </div>
      )}

      {plan.name_problems.length > 0 && (
        <div style={{ marginTop: 10 }}>
          <div className="badge badge-warn" style={{ display: "block" }}>
            {t("components.copyPlan.nameProblemsCount", { count: plan.name_problems.length })}{" "}
            {plan.name_problems.every((problem) => problem.suggestion)
              ? t("components.copyPlan.nameProblemsWillUse")
              : t("components.copyPlan.nameProblemsLeftBehind")}
          </div>
          <div style={{ maxHeight: 160, overflow: "auto", marginTop: 6 }}>
            {plan.name_problems.map((problem) => (
              <div
                key={problem.relative}
                className="faint"
                style={{ fontSize: 11, padding: "2px 0", wordBreak: "break-all" }}
              >
                <code>{problem.name}</code>
                {problem.suggestion ? (
                  <>
                    {" → "}
                    <code>{problem.suggestion}</code>
                  </>
                ) : (
                  ` ${t("components.copyPlan.noUsableName")}`
                )}
                <div style={{ opacity: 0.75 }}>{problem.reason}</div>
              </div>
            ))}
          </div>
        </div>
      )}

      {plan.collisions.length > 0 && (
        <div style={{ marginTop: 10 }}>
          <div className="badge badge-warn" style={{ display: "block" }}>
            {t("components.copyPlan.collisionsCount", { count: plan.collisions.length })}{" "}
            {plan.collisions.slice(0, 8).join(", ")}
            {plan.collisions.length > 8 && " …"}
          </div>

          <div style={{ display: "flex", gap: 6, marginTop: 6, flexWrap: "wrap" }}>
            {(
              [
                ["skip", "components.copyPlan.policyLeaveAlone"],
                ["overwrite", "components.copyPlan.policyReplace"],
                ["rename", "components.copyPlan.policyKeepBoth"],
              ] as Array<[OverwritePolicy, string]>
            ).map(([value, labelKey]) => (
              <button
                key={value}
                className={`btn${policy === value ? " btn-primary" : ""}`}
                style={{ fontSize: 12 }}
                onClick={() => onPolicyChange(value)}
              >
                {t(labelKey)}
              </button>
            ))}
          </div>
        </div>
      )}

      {/* §7.1: a drawer copied without its .info is invisible on Workbench.
          Not an error — the user may well mean it — but never silent. */}
      {plan.split_icons.length > 0 && (
        <div className="badge badge-warn" style={{ display: "block", marginTop: 10 }}>
          {plan.split_icons.filter((pair) => !pair.icon_without_object).length > 0 && (
            <div>
              {t("components.copyPlan.splitIconsWithoutIcons", {
                list: plan.split_icons
                  .filter((pair) => !pair.icon_without_object)
                  .slice(0, 5)
                  .map((pair) => pair.relative)
                  .join(", "),
              })}
            </div>
          )}
          {plan.split_icons.filter((pair) => pair.icon_without_object).length > 0 && (
            <div>{t("components.copyPlan.splitIconsOrphaned")}</div>
          )}
        </div>
      )}

      {planIsClean(plan) && (
        <div className="badge badge-ok" style={{ display: "block", marginTop: 10 }}>
          {t("components.copyPlan.clean")}
        </div>
      )}

      <div style={{ display: "flex", gap: 6, marginTop: 14, flexWrap: "wrap" }}>
        <button
          className="btn btn-primary"
          onClick={onConfirm}
          disabled={!fits}
          title={fits ? undefined : t("components.copyPlan.notEnoughRoom")}
        >
          {t("components.copyPlan.copy")}
        </button>
        <button className="btn" onClick={onCancel}>
          {t("common.cancel")}
        </button>
      </div>
    </div>
  );
}
