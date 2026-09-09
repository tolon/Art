// Tab 4 — *Derle* (four-tab design § 3.4), round 4 task 4.
//
// **Four lines, one button, one report.** Everything the other three tabs
// hold is summarised here in four sentences; then either the reason this
// cannot be built — in the button's place, never beside it — or a
// confirmation and one Build button; then one row per phase of the run, each
// with the ending it earned and what to do about it; then the hand-off to the
// card lane. The screen that this replaces was the whole wizard scrolling
// past a single Derle at the bottom.
//
// **The run is `useBuildRun`'s, the sentences are `buildRun.ts`'s, and this
// file renders.** Nothing here composes a sentence out of the core's answers:
// every line is a `Phrase` from a mapper in `src/lib`, put through the
// translator at the point it is drawn. The one exception is the parameter `src/lib` cannot
// fill — a component **id** where a person needs the recipe's own label —
// which is resolved here, through the catalogue, exactly as the install
// screen resolves it for `resident-table-unreadable`.
//
// **Update mode: the tree phase is absent** (this round's ruling).
// `osinstall_apply` refuses a destination with anything in it
// (`refuse_unless_free`), so a destination that is already an ART tree is
// *updated*: no tree phase, the first summary line says so, and the
// occupied-destination refusal is not a blocker, because nothing is going to
// try to write a tree over it.
//
// **The plan's refusals, the wrong-folder sentence and the switch-release
// button came from `OsInstall.tsx`** in task 4 of this round; task 5 renamed
// that file `FilesTab.tsx` and deleted the originals, so this is the only
// place they are drawn. So is the media scan they are computed from, and the
// `label()` helper (which is `buildSummary.ts`'s).
//
// The four lines themselves, and everything they are computed from, are
// `buildSummary.ts`'s `useBuildSummary`. **This tab is its only reader**:
// the bar under the tabs shared it until task 5, which cost a second plan,
// chain and slot report on every one of tabs 1-3 for a one-line summary the
// session already holds.
//
// **What is deliberately not here:** a progress bar for a job that reports no
// total (CLAUDE.md — a fixed width that looks like progress and carries no
// information), a single "it did not work" for the five endings, and any
// write on render. `session.tree` and `session.firstboot.written` are written
// by the run's own endings, which is where today's screen writes them.

import { useEffect, useLayoutEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { Link } from "react-router-dom";

import { useBuildSummary } from "@/components/osbuilder/buildSummary";
import {
  isFirstBootWritten,
  phaseNextStepPhrase,
  phaseOutcomePhrase,
  phaseTone,
  type PhaseReport,
} from "@/lib/buildRun";
import { stepPath } from "@/lib/buildSteps";
import { fraction } from "@/lib/jobs";
import {
  isInstallRelease,
  mediaEvidence,
  osinstallBlocker,
  osinstallMediaEvidence,
  osinstallReleaseForMedia,
  osinstallScanMedia,
  refusalPhrase,
  wrongMediaFolder,
  type ApplyOutcome,
  type InstallRelease,
  type MediaScanResult,
  type ReleaseEvidence,
  type StatedRelease,
} from "@/lib/osinstall";
import type { Phrase } from "@/lib/phrase";
import { useBuildRun } from "@/lib/useBuildRun";
import { useBuildSession } from "@/lib/useBuildSession";
import { useRunLock } from "@/pages/osbuilder/runLock";

/**
 * What the media folders actually hold, and what ART makes of it.
 *
 * **Moved from `OsInstall.tsx`** (the original goes with that file in task 5)
 * because the three answers it produces are what `osinstallBlocker`,
 * `wrongMediaFolder` and `mediaEvidence` are computed from, and those are the
 * sentences that stand in the Build button's place. Unchanged in meaning: one
 * `osinstall_scan_media` per folder the plan actually reads, the volume names
 * those scans found, and the two lookups over them.
 */
function useMediaEvidence(
  release: InstallRelease,
  materialKey: string,
  plannedFolderPaths: string[]
): { found: string[]; facts: ReleaseEvidence | null; releaseHolding: string | null } {
  const [folderScans, setFolderScans] = useState<Record<string, MediaScanResult | null>>({});
  useEffect(() => {
    const folders = materialKey ? materialKey.split("\n") : [];
    if (folders.length === 0) {
      setFolderScans((prev) => (Object.keys(prev).length === 0 ? prev : {}));
      return;
    }
    let cancelled = false;
    void Promise.all(
      folders.map(async (folder) => {
        try {
          return [folder, await osinstallScanMedia(folder)] as const;
        } catch {
          // A folder ART cannot read is the ordinary case here, not a badge.
          return [folder, null] as const;
        }
      })
    ).then((entries) => {
      if (!cancelled) setFolderScans(Object.fromEntries(entries));
    });
    return () => {
      cancelled = true;
    };
  }, [materialKey]);

  // A primitive dependency for the two lookups below, not a fresh array per
  // render (ART-178/ART-195: an identity there drives disk work in a loop).
  const foundKey = useMemo(() => {
    const seen = new Set<string>();
    const names: string[] = [];
    for (const folder of plannedFolderPaths) {
      const scan = folderScans[folder];
      if (scan?.outcome !== "found") continue;
      for (const medium of scan.media) {
        const folded = medium.volumeName.toLowerCase();
        if (seen.has(folded)) continue;
        seen.add(folded);
        names.push(medium.volumeName);
      }
    }
    return names.join("\n");
  }, [plannedFolderPaths, folderScans]);
  const found = useMemo(() => (foundKey ? foundKey.split("\n") : []), [foundKey]);

  const [facts, setFacts] = useState<ReleaseEvidence | null>(null);
  useEffect(() => {
    if (found.length === 0) {
      setFacts(null);
      return;
    }
    let cancelled = false;
    osinstallMediaEvidence(release, found)
      .then((answer) => {
        if (!cancelled) setFacts(answer);
      })
      // A release with no shipped recipe throws, and there is nothing
      // truthful to say about a folder against a recipe ART does not have.
      .catch(() => {
        if (!cancelled) setFacts(null);
      });
    return () => {
      cancelled = true;
    };
  }, [release, found]);

  const [releaseHolding, setReleaseHolding] = useState<string | null>(null);
  useEffect(() => {
    if (found.length === 0) {
      setReleaseHolding(null);
      return;
    }
    let cancelled = false;
    osinstallReleaseForMedia(found)
      .then((named) => {
        if (!cancelled) setReleaseHolding(named);
      })
      .catch(() => {
        if (!cancelled) setReleaseHolding(null);
      });
    return () => {
      cancelled = true;
    };
  }, [found]);

  return { found, facts, releaseHolding };
}

/** How far a running phase has got — **and nothing at all when its job has
 *  not said**. A bar of fixed width over an unknown total looks like progress
 *  and carries none (CLAUDE.md), so a job with no total gets the count it
 *  does have and no bar. */
function PhaseProgress({ progress }: { progress: PhaseReport["ending"] }) {
  const { t } = useTranslation();
  if (progress.state !== "running") return null;
  const job = progress.progress;
  const pct = job ? fraction(job) : null;
  return (
    <div style={{ marginTop: 4 }}>
      <span className="faint" data-testid="build-progress" style={{ fontSize: 11 }}>
        {job === null
          ? t("osinstall.run.progress.starting")
          : pct === null
            ? t("osBuilder.build.progress.soFar", { done: job.done })
            : t("osinstall.run.progress.percent", {
                percent: Math.round(pct * 100),
                done: job.done,
                total: job.total ?? 0,
              })}
      </span>
      {pct !== null && (
        <div
          aria-hidden
          data-testid="build-progress-bar"
          style={{
            height: 4,
            marginTop: 4,
            borderRadius: 2,
            background: "var(--border)",
            overflow: "hidden",
            maxWidth: 420,
          }}
        >
          <div
            style={{
              height: "100%",
              width: `${pct * 100}%`,
              background: "var(--accent)",
              transition: "width 120ms linear",
            }}
          />
        </div>
      )}
    </div>
  );
}

/** The five verdicts the tree's own release marker gets, the removed list and
 *  the icons list — **moved from `OsInstall.tsx`'s result card**, unchanged in
 *  meaning, with the keys they arrived with. Five sentences and never a
 *  pass/fail: ask the artefact. */
function TreePhaseDetail({
  outcome,
  statedRelease,
}: {
  outcome: ApplyOutcome;
  statedRelease: StatedRelease | undefined;
}) {
  const { t } = useTranslation();
  return (
    <>
      {statedRelease && (
        <p className="muted" data-testid="build-stated-release" style={{ fontSize: 11, margin: "4px 0 0" }}>
          {statedRelease.verdict === "confirmed"
            ? t("osinstall.result.statedRelease.confirmed", { stated: statedRelease.stated })
            : statedRelease.verdict === "mismatch"
              ? t("osinstall.result.statedRelease.mismatch", {
                  expected: statedRelease.expected,
                  stated: statedRelease.stated,
                })
              : statedRelease.verdict === "expected-unknown"
                ? t("osinstall.result.statedRelease.expectedUnknown", {
                    stated: statedRelease.stated,
                  })
                : statedRelease.verdict === "unreadable"
                  ? t("osinstall.result.statedRelease.unreadable", {
                      detail: statedRelease.detail,
                    })
                  : t("osinstall.result.statedRelease.unstated")}
        </p>
      )}
      {outcome.removed.length > 0 && (
        <div style={{ marginTop: 6 }}>
          <h4 style={{ fontSize: 12, margin: "0 0 2px" }}>{t("osinstall.result.removed.heading")}</h4>
          <ul style={{ margin: 0, paddingLeft: 18, fontSize: 11 }}>
            {outcome.removed.map((verdict) => (
              <li key={verdict.to}>
                {verdict.to} —{" "}
                {verdict.state === "removed"
                  ? t("osinstall.result.removed.state.removed")
                  : verdict.state === "not-present"
                    ? t("osinstall.result.removed.state.notPresent")
                    : t("osinstall.result.removed.state.failed", { detail: verdict.state.failed })}
              </li>
            ))}
          </ul>
        </div>
      )}
      {outcome.icons.length > 0 && (
        <div style={{ marginTop: 6 }}>
          <h4 style={{ fontSize: 12, margin: "0 0 2px" }}>{t("osinstall.result.icons.heading")}</h4>
          <ul style={{ margin: 0, paddingLeft: 18, fontSize: 11 }}>
            {outcome.icons.map((verdict) => (
              <li key={verdict.to}>
                {verdict.to} —{" "}
                {verdict.state === "merged"
                  ? t("osinstall.result.icons.state.merged")
                  : verdict.state === "destination-absent"
                    ? t("osinstall.result.icons.state.destinationAbsent")
                    : t("osinstall.result.icons.state.failed", { detail: verdict.state.failed })}
              </li>
            ))}
          </ul>
        </div>
      )}
    </>
  );
}

const TONE_CLASS: Record<ReturnType<typeof phaseTone>, string> = {
  ok: "badge badge-ok",
  warn: "badge badge-warn",
  err: "badge badge-err",
  muted: "muted",
};

/** One phase, in its own words: what happened, what to do about it, and the
 *  refusals themselves where there were any. */
function PhaseRow({
  report,
  destination,
  label,
}: {
  report: PhaseReport;
  destination: string;
  label: (id: string) => string;
}) {
  const { t } = useTranslation();
  const outcome = phaseOutcomePhrase(report);
  const next = phaseNextStepPhrase(report, destination);
  const ending = report.ending;
  const applyOutcome =
    ending.state === "succeeded" && !isFirstBootWritten(ending.outcome) ? ending.outcome : null;

  return (
    <div
      data-testid="build-phase-row"
      style={{ padding: "6px 0", borderTop: "1px solid var(--border)" }}
    >
      <div style={{ display: "flex", gap: 8, alignItems: "baseline", flexWrap: "wrap" }}>
        <span
          className={TONE_CLASS[phaseTone(report)]}
          style={{ fontSize: 12, display: "inline-block" }}
        >
          {t(outcome.key, outcome.params)}
        </span>
        {/* The design's "its own count **and time**". One decimal: a run
            reported to the millisecond claims a precision nobody measured. */}
        {ending.state === "succeeded" && (
          <span className="faint" data-testid="build-phase-elapsed" style={{ fontSize: 11 }}>
            {t("osBuilder.build.phase.elapsed", { seconds: (ending.elapsedMs / 1000).toFixed(1) })}
          </span>
        )}
      </div>
      <PhaseProgress progress={ending} />
      {/* The refusals themselves, under the advice. "Fix what the refusal
          names" with no refusal on screen names nothing. */}
      {ending.state === "refused" && (
        <ul className="muted" style={{ fontSize: 11, margin: "4px 0 0", paddingLeft: 20 }}>
          {ending.refusals.map((reason, at) => {
            const phrase = refusalPhrase(reason);
            const params =
              reason.refusal === "resident-table-unreadable"
                ? { ...phrase.params, component: label(reason.component) }
                : phrase.params;
            return (
              <li key={at} data-testid="build-phase-refusal">
                {t(phrase.key, params)}
              </li>
            );
          })}
        </ul>
      )}
      {next && (
        <p className="muted" data-testid="build-phase-next" style={{ fontSize: 11, margin: "4px 0 0" }}>
          {t(next.key, next.params)}
        </p>
      )}
      {report.phase.kind === "tree" && applyOutcome && (
        <TreePhaseDetail
          outcome={applyOutcome}
          statedRelease={ending.state === "succeeded" ? ending.statedRelease : undefined}
        />
      )}
    </div>
  );
}

export function BuildTab() {
  const { t } = useTranslation();
  const { session, setTree, setFirstBoot, setKind, setRelease } = useBuildSession();
  /**
   * **A run just finished, so the folder is not what it was** (fix round 1,
   * I2). `osinstall_destination_taken` and `osinstall_describe_tree` are
   * answers about a folder, and a successful build fills the folder it was
   * told to fill: without this the gate went on offering a fresh tree into
   * a folder that now holds one, and the summary went on calling it fresh.
   * Bumped once per completed run — `run.finished` is false again the moment
   * the next one starts.
   */
  const [runStamp, setRunStamp] = useState(0);
  const summary = useBuildSummary({ revision: runStamp });
  const { plan, destination, taken, ticked, phases, lines, label } = summary;
  const { effectivePlan, effectivePlanResult } = plan;

  // The folders the request actually reads — a layered release's tagged ones,
  // an unlayered release's whole list. Memoized because two effects below
  // depend on it (ART-178/ART-195).
  const plannedFolderPaths = useMemo(
    () =>
      plan.layers.length > 0
        ? plan.layers
            .map((layer) => plan.plannedFolders.mediaFolders[layer.id])
            .filter((f): f is string => !!f)
        : [plan.plannedFolders.mediaFolder, ...plan.plannedFolders.extraMediaFolders].filter(
            (f): f is string => !!f
          ),
    [plan.layers, plan.plannedFolders]
  );
  const materialKey = session.material.folders.map((folder) => folder.path).join("\n");
  const evidence = useMediaEvidence(summary.release, materialKey, plannedFolderPaths);

  // ART-208: one wrong folder is one sentence, not one absence per component.
  const wrongFolder = effectivePlan
    ? wrongMediaFolder(effectivePlan, evidence.found, evidence.facts)
    : null;
  const mediaEvidenceLine = effectivePlan
    ? mediaEvidence({
        plan: effectivePlan,
        found: evidence.found,
        releaseHolding: evidence.releaseHolding,
        release: summary.release,
        evidence: evidence.facts,
      })
    : null;

  const blocker = osinstallBlocker({
    // **No fallback** (review M2). This was `plannedFolderPaths[0] ??
    // summary.release`, which handed a *release name* to a field the blocker
    // reads as a folder path — an expression that can only ever fire when the
    // array is non-empty and its first entry is missing, which the line above
    // it has already excluded. A fallback nothing can reach is a claim the
    // next reader takes for a case somebody thought about.
    mediaFolder: plannedFolderPaths.length > 0 ? plannedFolderPaths[0] : null,
    destination,
    destinationTaken: taken,
    plan: effectivePlanResult,
    found: evidence.found,
    releaseHolding: evidence.releaseHolding,
    mediaFacts: evidence.facts,
  });

  const treePhaseNeeded = phases.some((phase) => phase.kind === "tree");

  /**
   * **A plan that is still being computed is not a plan nobody asked for**
   * (review I5).
   *
   * `osinstallBlocker` answers `notPlanned` — *"Preview it first — nothing is
   * built before you have seen what would be"* — for `plan === null`, which
   * was written for a screen with a Preview button. Tab 4 has no such control
   * and never had one: it plans on its own, and the only reason `plan` is null
   * with a folder in hand and no error is that the round trip has not landed.
   * So the person was told to press something that is not there, for a state
   * that clears itself in a second.
   *
   * The three conditions are the whole of *would plan*: no answer yet, no
   * error to explain the absence, and at least one folder for the request to
   * read. A plan that **failed** keeps `notPlanned` out of the way too — the
   * error badge above says what Rust said.
   */
  const planning =
    effectivePlanResult === null && plan.planError === null && plannedFolderPaths.length > 0;
  const planBlocker: Phrase | null =
    blocker?.key === "osinstall.blocked.notPlanned" && planning
      ? { key: "osinstall.blocked.planning" }
      : blocker;

  /**
   * What stands in the Build button's place, or `null` when the button stands.
   *
   * **The plan blocks only a build that runs the plan.** In update mode the
   * sequence has no tree phase, so a refusal about media the tree already
   * holds may not withhold a button that adds a BoingBag to it — the run
   * needs the update's own archive and nothing else. What can still block it
   * is having nothing to do at all, which is its own sentence rather than a
   * disabled button with no reason beside it.
   *
   * **This is also where update mode's own ruling lives, and it lives here
   * only.** The task's brief asked for it a second time, as
   * `destinationTaken: destinationIsTree ? false : taken` in the blocker's
   * input — the occupied-destination refusal is about writing a *tree*, and
   * update mode writes none. That expression was written, and then measured:
   * a mutation putting the plain `taken` back killed no test, because in
   * update mode nothing reads `blocker` at all. A second guard that no test
   * can move is worse than no second guard — it reads as though it were
   * holding a line it is not — so the ternary went and the rule is stated
   * once, here, where the mutation does fall.
   */
  const gate: Phrase | null = !summary.destinationChecked
    ? // **Nothing is offered over a guess** (fix round 1, M3). Which
      // sequence this build runs — with a tree phase or without one — is
      // decided by a round trip that has not landed, and a button pressed
      // before it lands runs the wrong one.
      { key: "osBuilder.build.summary.checking" }
    : treePhaseNeeded
      ? planBlocker
      : phases.length === 0
        ? { key: "osBuilder.build.blocked.nothingToRun" }
        : null;

  const [confirmed, setConfirmed] = useState(false);
  // A confirmation describes the plan that was on screen when it was given;
  // once the plan changes it is stale. `useLayoutEffect` so the button is
  // never briefly enabled against a plan nobody confirmed.
  useLayoutEffect(() => {
    setConfirmed(false);
  }, [plan.planVersion]);

  const run = useBuildRun({
    destination,
    // ART-197: the tree the next steps get is the one this run wrote.
    onTreeWritten: (root) => setTree({ root, builtHere: true }),
    onFirstBootWritten: () => setFirstBoot({ written: true }),
  });

  useEffect(() => {
    if (run.finished) setRunStamp((stamp) => stamp + 1);
  }, [run.finished]);

  /**
   * **The lane's strip goes dead while this runs** (round 4 whole-branch
   * review, I2; the defect is in `runLock.tsx`). The sequencer lives in this
   * component, so leaving tab 4 mid-run drops the rest of the phases and the
   * hand-off with no sentence anywhere.
   *
   * Two effects rather than one with a cleanup: a cleanup that also cleared on
   * every change would write `false` and then `true` for a run that is simply
   * carrying on. The second clears on **unmount alone** — a tab that goes away
   * for any other reason must not leave the lane locked with nothing left to
   * unlock it.
   */
  const { setRunning } = useRunLock();
  useEffect(() => {
    setRunning(run.running);
  }, [run.running, setRunning]);
  useEffect(() => () => setRunning(false), [setRunning]);

  return (
    <section className="card" data-testid="build-tab" style={{ marginBottom: 16 }}>
      <h2 style={{ fontSize: 16, marginTop: 0 }}>{t("osBuilder.step.derle")}</h2>

      {/* **The plan error belongs to whoever shows the plan** (round 4 task
          5). It was a badge at the top of the install screen; the summary
          below is the plan now, so the badge is above the summary. Without
          it the blocker's *"Preview it first"* was the only thing on screen
          for a plan that had been asked for and had **failed** — the right
          shape of sentence for the wrong reason. Rust's own words, through
          `useInstallPlan`'s `errorText`. */}
      {plan.planError && (
        <p
          className="badge badge-err"
          data-testid="build-plan-error"
          style={{ display: "block", padding: "8px 12px", fontSize: 12, margin: "0 0 12px" }}
        >
          {plan.planError}
        </p>
      )}

      <div style={{ margin: "8px 0 12px" }}>
        {lines.map((line) => (
          <p
            key={line.key}
            data-testid="build-summary-line"
            className="muted"
            style={{ fontSize: 12, margin: "2px 0", wordBreak: "break-word" }}
          >
            {t(line.key, line.params)}
          </p>
        ))}
      </div>

      {/* A ticked row ART could not resolve to a file it trusts. Named, and
          not run: a row the user ticked that simply vanishes from the plan is
          the confident wrong screen at its quietest. */}
      {ticked.unresolved.map((row) => (
        <p
          key={row.packageId}
          data-testid="build-unresolved"
          className="badge badge-warn"
          style={{ fontSize: 11, margin: "0 0 8px", display: "block", padding: "4px 8px" }}
        >
          {t("osBuilder.build.unresolved", { name: row.name })}
        </p>
      ))}

      {gate ? (
        <>
          {/* ART-208: suppressed for the one-wrong-folder case, whose own
              sentence is the blocker below — the same warning twice on one
              screen is ART-202. */}
          {effectivePlan && effectivePlan.refusals.length > 0 && !wrongFolder && (
            <div data-testid="build-refusals" style={{ marginBottom: 8 }}>
              <h3 style={{ fontSize: 13, margin: "0 0 4px" }}>{t("osinstall.refusals.heading")}</h3>
              {mediaEvidenceLine && (
                <p className="faint" style={{ fontSize: 11, margin: "0 0 6px" }}>
                  {t(mediaEvidenceLine.key, mediaEvidenceLine.params)}
                </p>
              )}
              <ul className="muted" style={{ fontSize: 11, margin: 0, paddingLeft: 20 }}>
                {effectivePlan.refusals.map((reason, at) => {
                  const phrase = refusalPhrase(reason);
                  // A raw recipe id is not a checkbox a person can find.
                  const params =
                    reason.refusal === "resident-table-unreadable"
                      ? { ...phrase.params, component: label(reason.component) }
                      : phrase.params;
                  return <li key={at}>{t(phrase.key, params)}</li>;
                })}
              </ul>
            </div>
          )}
          <p
            className="badge badge-warn"
            data-testid="build-blocker"
            style={{ fontSize: 12, margin: 0, display: "block", padding: "6px 10px" }}
          >
            {t(gate.key, gate.params)}
          </p>
          {/* ART-208's one click, beside the control it unblocks. */}
          {wrongFolder && evidence.releaseHolding && isInstallRelease(evidence.releaseHolding) && (
            <button
              className="btn btn-sm"
              data-testid="build-switch-release"
              style={{ marginTop: 8 }}
              onClick={() => setRelease(evidence.releaseHolding as InstallRelease)}
            >
              {t("osinstall.blocked.switchRelease", { release: evidence.releaseHolding })}
            </button>
          )}
        </>
      ) : (
        <>
          <label
            style={{ display: "flex", gap: 8, alignItems: "center", fontSize: 12, marginBottom: 10 }}
          >
            <input
              type="checkbox"
              data-testid="build-confirm"
              checked={confirmed}
              onChange={(e) => setConfirmed(e.target.checked)}
            />
            {t("osinstall.run.confirm")}
          </label>
          <div style={{ display: "flex", gap: 8, alignItems: "center", flexWrap: "wrap" }}>
            <button
              className="btn btn-primary"
              data-testid="build-run"
              disabled={!confirmed || run.running}
              onClick={() => run.start(phases, effectivePlan)}
            >
              {t(run.running ? "osBuilder.build.running" : "osBuilder.build.run")}
            </button>
            {run.running && (
              <button className="btn btn-sm" data-testid="build-stop" onClick={() => run.stop()}>
                {t("osBuilder.build.stop")}
              </button>
            )}
            {!run.running && (
              <span className="faint" style={{ fontSize: 11 }}>
                {t("osBuilder.build.confirmHint")}
              </span>
            )}
            {/* **The running phase is named once, in its own row** (round 4
                whole-branch review, M1). A second *"Running: BoingBag 3.9-1"*
                beside the button repeated a sentence the phase list below
                already carries — ART-202's "aynı uyarı tek ekranda 2 tane",
                and the copy that could go stale, since the row's is the
                report's own ending and this was a lookup beside it. */}
          </div>
        </>
      )}

      {run.reports.length > 0 && (
        <div style={{ marginTop: 12 }}>
          {run.reports.map((report) => (
            <PhaseRow
              key={report.phase.id}
              report={report}
              destination={destination ?? ""}
              label={label}
            />
          ))}
        </div>
      )}

      {run.succeeded && destination && (
        <p
          className="badge badge-ok"
          data-testid="build-handoff"
          style={{ fontSize: 12, marginTop: 12, display: "block", padding: "6px 10px" }}
        >
          {t("osBuilder.build.handoff", { root: destination })}{" "}
          {/* ART-197's last open row: the card lane is a different **kind** of
              build, and a lane entered without its kind renders the wrong
              steps with nothing saying why. */}
          <Link to={stepPath("kart")} onClick={() => setKind("boot-card")}>
            {t("osBuilder.build.handoffLink")}
          </Link>
        </p>
      )}
    </section>
  );
}
