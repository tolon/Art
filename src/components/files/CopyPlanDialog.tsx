// The pre-flight report, before a copy runs (brief §3.3, §4.1).
//
// > *Explain before modify.*
//
// The failure this exists to prevent is the drip feed: a copy that stops on
// file 37 for a long name, on 52 because the disk filled, on 61 for a
// collision. One report, real numbers, one decision.

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
  const fits = plan.blocks_needed <= plan.blocks_free;
  const shortfall = planShortfall(plan);

  return (
    <div
      role="dialog"
      aria-label="Copy plan"
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
      <strong>Before copying</strong>
      <div className="faint" style={{ fontSize: 11, marginTop: 2, wordBreak: "break-all" }}>
        into {destination}
      </div>

      <div style={{ fontSize: 13, marginTop: 10 }}>
        {plan.files} file{plan.files === 1 ? "" : "s"}
        {plan.directories > 0 &&
          ` and ${plan.directories} folder${plan.directories === 1 ? "" : "s"}`}
        , {formatBytes(plan.total_bytes)}
      </div>

      {/* Blocks, not just bytes: a thousand one-byte files cost a thousand
          blocks, and a report in bytes alone would say "1 KB" and be wrong
          about whether it fits. */}
      <div className="faint" style={{ fontSize: 11, marginTop: 2 }}>
        {plan.blocks_needed.toLocaleString()} blocks needed ·{" "}
        {plan.blocks_free.toLocaleString()} free
      </div>

      {shortfall && (
        <div className="badge badge-err" style={{ display: "block", marginTop: 10 }}>
          {shortfall}
        </div>
      )}

      {plan.name_problems.length > 0 && (
        <div style={{ marginTop: 10 }}>
          <div className="badge badge-warn" style={{ display: "block" }}>
            {plan.name_problems.length} name
            {plan.name_problems.length === 1 ? "" : "s"} AmigaDOS cannot store.
            {plan.name_problems.every((problem) => problem.suggestion)
              ? " ART will use the names on the right."
              : " Some have no usable alternative and will be left behind."}
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
                  " — no usable name left"
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
            {plan.collisions.length} name
            {plan.collisions.length === 1 ? " is" : "s are"} already there:{" "}
            {plan.collisions.slice(0, 8).join(", ")}
            {plan.collisions.length > 8 && " …"}
          </div>

          <div style={{ display: "flex", gap: 6, marginTop: 6, flexWrap: "wrap" }}>
            {(
              [
                ["skip", "Leave them alone"],
                ["overwrite", "Replace them"],
                ["rename", "Keep both"],
              ] as Array<[OverwritePolicy, string]>
            ).map(([value, label]) => (
              <button
                key={value}
                className={`btn${policy === value ? " btn-primary" : ""}`}
                style={{ fontSize: 12 }}
                onClick={() => onPolicyChange(value)}
              >
                {label}
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
            <>
              Some folders are being copied without their <code>.info</code>{" "}
              icons, so they will not show up on Workbench:{" "}
              {plan.split_icons
                .filter((pair) => !pair.icon_without_object)
                .slice(0, 5)
                .map((pair) => pair.relative)
                .join(", ")}
              .{" "}
            </>
          )}
          {plan.split_icons.filter((pair) => pair.icon_without_object).length > 0 && (
            <>
              Some <code>.info</code> icons have nothing to describe in this
              copy.
            </>
          )}
        </div>
      )}

      {planIsClean(plan) && (
        <div className="badge badge-ok" style={{ display: "block", marginTop: 10 }}>
          Everything fits and every name is one AmigaDOS can store.
        </div>
      )}

      <div style={{ display: "flex", gap: 6, marginTop: 14, flexWrap: "wrap" }}>
        <button
          className="btn btn-primary"
          onClick={onConfirm}
          disabled={!fits}
          title={fits ? undefined : "There is not enough room on the disk"}
        >
          Copy
        </button>
        <button className="btn" onClick={onCancel}>
          Cancel
        </button>
      </div>
    </div>
  );
}
