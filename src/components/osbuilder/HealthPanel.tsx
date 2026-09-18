// The card's health checklist (§50), lifted out of `CardBuilder.tsx` in round
// 4's task 10 so the one-button run and the manual card builder draw the
// **same** checklist rather than two that can drift.
//
// Unchanged in meaning by the move: the same three states, the same marks, the
// same colours, and the same catalogue keys. It renders a `HealthReport` and
// asks nothing — every sentence is a `Phrase` from `@/lib/cardBuild`, put
// through the translator here.

import { useTranslation } from "react-i18next";

import {
  findingPhrase,
  healthCheckPhrase,
  healthVerdict,
  manualStepPhrase,
  type HealthReport,
} from "@/lib/cardBuild";

/**
 * The checklist (§50), in three parts that must not look like each other:
 * what passed, what failed, and **what ART could not answer** — plus the steps
 * only the person at the machine can take. A tick that means "ART did not
 * look" reading like one that means "ART looked and it is right" is the claim
 * §89 forbids, so the third state gets its own mark and its own colour.
 */
export function HealthPanel({ report }: { report: HealthReport }) {
  const { t } = useTranslation();
  const verdict = healthVerdict(report);
  const failed = report.items.some((item) => item.state === "fail");

  const mark: Record<HealthReport["items"][number]["state"], string> = {
    pass: "✓",
    fail: "✕",
    "not-checked": "–",
  };
  const colour: Record<HealthReport["items"][number]["state"], string> = {
    pass: "var(--ok-text)",
    fail: "var(--err-text)",
    "not-checked": "var(--text-faint)",
  };

  return (
    <div style={{ marginTop: 10 }}>
      <p
        className={`badge ${failed ? "badge-err" : "badge-ok"}`}
        style={{ display: "block", padding: "6px 12px", fontSize: 11, margin: 0 }}
      >
        {t(verdict.key, verdict.params)}
      </p>

      <ul style={{ fontSize: 11, margin: "10px 0 0", paddingLeft: 0, listStyle: "none" }}>
        {report.items.map((item, index) => {
          const phrase = healthCheckPhrase(item.check);
          return (
            <li key={index} style={{ padding: "2px 0" }}>
              <span
                style={{ color: colour[item.state], fontWeight: 700, marginRight: 8 }}
                title={t(`cardBuilder.health.state.${item.state === "not-checked" ? "notChecked" : item.state}`)}
              >
                {mark[item.state]}
              </span>
              <span className={item.state === "not-checked" ? "faint" : "muted"}>
                {t(phrase.key, phrase.params)}
              </span>
              {/* When the manifest disagrees, *what* disagrees — "one finding"
                  is not something anybody can act on. */}
              {item.check.kind === "manifest-agrees" && item.check.findings.length > 0 && (
                <ul className="faint" style={{ margin: "4px 0 0", paddingLeft: 24 }}>
                  {item.check.findings.map((finding, at) => {
                    const detail = findingPhrase(finding);
                    return <li key={at}>{t(detail.key, detail.params)}</li>;
                  })}
                </ul>
              )}
            </li>
          );
        })}
      </ul>

      <h4 style={{ fontSize: 12, margin: "14px 0 4px" }}>
        {t("cardBuilder.health.byHandHeading")}
      </h4>
      <ul className="muted" style={{ fontSize: 11, margin: 0, paddingLeft: 18 }}>
        {report.by_hand.map((step, index) => {
          const phrase = manualStepPhrase(step);
          return <li key={index}>{t(phrase.key, phrase.params)}</li>;
        })}
      </ul>
    </div>
  );
}
