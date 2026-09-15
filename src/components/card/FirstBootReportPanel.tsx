// Reading `S:FirstBoot.log`'s parsed shape — a plain presentational read of
// `FirstBootReport` (Task 9 of the first-boot round; Task 11 mounts this on
// the card screen and adds `source`, which says *which* copy was read: the
// Amiga volume's own `S/FirstBoot.log`, or the FAT partition's
// `art-firstboot.log`).
//
// **Endings stay distinct** (CLAUDE.md). `endingPhrase` gives each ending its
// own sentence, and a card that has never booted gets *only* that sentence —
// no table, no system line, no "unknown" section, because there is nothing
// underneath any of them to show. `stepOutcomeTone` colours a row, but the
// wording from `stepOutcomePhrase` is what actually says what happened; the
// tone rides beside it as `data-tone`, never replacing the sentence.
//
// **Beginner mode hides the AmigaDOS noise, not the outcome** (spec §9;
// I4, final review). `details` (raw script lines), `unknown` (lines a newer
// ART would carry) and a refusal's `rc=` return code are all script-internal
// detail meant for someone who would recognise a `frobnicate 3` line — they
// are gated on `power` here. This sentence used to add "…the same way
// `FirstBootPanel` already gates the tree-path column and the written-file
// list"; that panel was deleted in four-tabs round 5, its preview table with
// it, so the precedent is gone and this gate is the only one left in the
// first-boot lane. **The rule outlived the precedent.** What is never hidden
// is *that* a step was refused: `stepOutcomePhrase`'s beginner variant still
// says so, in one sentence, without the code underneath it.
//
// **A restart is told apart** (first-boot phase 3 design §6): restarted, no
// command, not back yet — and the wizard's own detail lines read as
// sentences in both modes, because *which windows opened* is the outcome,
// not AmigaDOS noise.

import { useTranslation } from "react-i18next";

import {
  endingPhrase,
  rebootReportPhrase,
  reportSourcePhrase,
  stepOutcomePhrase,
  stepOutcomeTone,
  wizardDetail,
  wizardWindowPhrase,
  WIZARD_STEP_NAME,
  type FirstBootReport,
  type ReportSource,
  type WizardDetail,
  type WizardWindow,
} from "@/lib/firstboot";
import { usePowerMode } from "@/lib/uxmode";

export interface FirstBootReportPanelProps {
  report: FirstBootReport;
  /** Which copy was read. Optional so the OS Builder's rehearsal call site
   *  (a live report, no card, no source) does not have to invent one;
   *  `"none"` renders nothing about a source, same as omitting the prop. */
  source?: ReportSource;
}

const TONE_CLASS: Record<string, string> = {
  ok: "badge-ok",
  muted: "faint",
  warn: "badge-warn",
  err: "badge-err",
};

/**
 * The heading a rendered report and `HardDiskStudio`'s own "the read failed
 * outright" card both need (leftover round). Pulled out because the two used
 * to carry their own copy of the same paragraph, one file apart, with
 * nothing keeping them in sync but a person noticing.
 */
export function FirstBootReportHeading() {
  const { t } = useTranslation();
  return (
    <p className="muted" style={{ fontSize: 12, fontWeight: 600, margin: "0 0 6px" }}>
      {t("firstboot.report.heading")}
    </p>
  );
}

export function FirstBootReportPanel({ report, source }: FirstBootReportPanelProps) {
  const { t } = useTranslation();
  const power = usePowerMode();
  const ending = endingPhrase(report.ending);
  const sourcePhrase = source ? reportSourcePhrase(source) : null;
  const reboot = rebootReportPhrase(report);
  /** A window's label, never its file name. */
  const windowLabel = (window: WizardWindow) => t(wizardWindowPhrase(window).key);

  if (report.ending === "not-booted") {
    return (
      <div data-testid="firstboot-report" className="card" style={{ padding: "8px 10px" }}>
        <p data-testid="firstboot-report-ending" style={{ margin: 0 }}>
          {t(ending.key, ending.params)}
        </p>
      </div>
    );
  }

  return (
    <div data-testid="firstboot-report" className="card" style={{ padding: "8px 10px" }}>
      <FirstBootReportHeading />

      {sourcePhrase && (
        <p data-testid="firstboot-report-source" className="faint" style={{ fontSize: 11, margin: "0 0 8px" }}>
          {t(sourcePhrase.key, sourcePhrase.params)}
        </p>
      )}

      {report.system && (
        <p data-testid="firstboot-report-system" style={{ fontSize: 12, margin: "0 0 8px" }}>
          {t("firstboot.report.system", {
            system: report.system.system,
            rpi: report.system.rpi,
            kick: report.system.kick,
          })}
        </p>
      )}

      <div
        data-testid="firstboot-report-table"
        style={{ overflowX: "auto", marginBottom: 8 }}
      >
        <table style={{ borderCollapse: "collapse", width: "100%", fontSize: 12 }}>
          <tbody>
            {report.steps.map((step, at) => {
              const outcome = stepOutcomePhrase(step.outcome, { beginner: !power });
              const tone = stepOutcomeTone(step.outcome);
              return (
                <tr
                  key={`${step.name}-${at}`}
                  data-testid="firstboot-report-step"
                  data-tone={tone}
                >
                  <td style={{ padding: "2px 8px 2px 0", whiteSpace: "nowrap" }}>{step.name}</td>
                  <td className={TONE_CLASS[tone]} style={{ padding: "2px 0" }}>
                    {t(outcome.key, outcome.params)}
                    {step.name === WIZARD_STEP_NAME &&
                      (() => {
                        const read = step.details
                          .map(wizardDetail)
                          .filter((d): d is WizardDetail => d !== null);
                        return read.length > 0 ? (
                          <div data-testid="firstboot-report-wizard" className="faint" style={{ fontSize: 11 }}>
                            {read.map((d, i) => (
                              <div key={`${d.window}-${i}`}>{t(d.phrase.key, { window: windowLabel(d.window) })}</div>
                            ))}
                          </div>
                        ) : null;
                      })()}
                    {/* `90-prefs` excluded (task 9): its own detail lines are
                        read as the sentences above, and drawing both here and
                        there would say one fact twice. */}
                    {power && step.name !== WIZARD_STEP_NAME && step.details.length > 0 && (
                      <div className="faint" style={{ fontSize: 11 }}>
                        <div style={{ fontWeight: 600 }}>{t("firstboot.report.details")}</div>
                        {step.details.map((detail) => (
                          <div key={detail}>{detail}</div>
                        ))}
                      </div>
                    )}
                  </td>
                </tr>
              );
            })}
          </tbody>
        </table>
      </div>

      <p data-testid="firstboot-report-ending" style={{ fontSize: 12, margin: "0 0 6px" }}>
        {t(ending.key, ending.params)}
      </p>

      {reboot && (
        <p data-testid="firstboot-report-reboot" style={{ fontSize: 12, margin: "0 0 6px" }}>
          {t(reboot.key, reboot.params)}
        </p>
      )}

      {report.fatCopyFailed && (
        <p
          data-testid="firstboot-report-fat-copy-failed"
          className="badge badge-warn"
          style={{ display: "block", padding: "4px 8px", fontSize: 11, margin: "0 0 6px" }}
        >
          {t("firstboot.report.fatCopyFailed")}
        </p>
      )}

      {power && report.unknown.length > 0 && (
        <div data-testid="firstboot-report-unknown" style={{ fontSize: 11 }}>
          <p className="faint" style={{ margin: "0 0 4px" }}>
            {t("firstboot.report.unknown")}
          </p>
          {report.unknown.map((line, at) => (
            <div key={`${line}-${at}`} style={{ wordBreak: "break-all" }}>
              {line}
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
