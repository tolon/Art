// Placing a package's files from Windows — the machinery two screens now
// share (round 3, task 2).
//
// `PackagePanel` has owned this since Task 7: preview what the archive would
// replace (`osinstall_collisions`), let the user confirm the *set* rather
// than each file, apply it (`osinstall_add_package`), and report what
// happened. The chain screen needs exactly that for its host-placed rows —
// Locale 3.9, the Turkish slice, Contribution — and the brief's rule is
// **reuse, not a second copy**: two implementations of "what would this
// replace" is two answers to one question, and the one that drifts is the
// one nobody is looking at.
//
// So the state machine lives in {@link useHostPlacement} and the two report
// blocks live in {@link HostPlacementPreview} and {@link HostPlacementReport}.
// What stays with each screen is what each screen *decides*: which packages
// are in the set, whether the set may be previewed at all, and what its Run
// button is allowed to say.
//
// **Three endings, and they stay three** (CLAUDE.md's own rule, and F2's
// from Task 7): the placement was applied, it was *refused* before a byte
// moved — with the reason typed and named — or the job itself failed and
// says so. A refusal is never a job that went red later, and a failure is
// never reported as "added".

import { useEffect, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";

import {
  collisionGroupHeadingKey,
  collisionPhrase,
  groupCollisionsForPreview,
  onOsInstallAddPackageResult,
  osinstallAddPackage,
  osinstallCollisions,
  refusalPhrase,
  type ApplyOutcome,
  type CollisionReport,
  type RefusalReason,
} from "@/lib/osinstall";
import { onJobProgress, subscribeSafely, type JobProgress } from "@/lib/jobs";
import { errorText } from "@/lib/errorText";
import { formatBytes } from "@/lib/panel";

/** What {@link useHostPlacement} holds and what the two blocks below read. */
export interface HostPlacement {
  /** What the chosen set would replace, or `null` while nothing has been
   *  asked or nothing has answered — never `[]`, which is a real preview
   *  saying "nothing on the tree would be replaced". */
  collisions: CollisionReport[] | null;
  /** The preview call's own rejection, already turned into a sentence. */
  collisionsError: string | null;
  /** A refused selection, typed and resolved **before anything was
   *  written** — never a job, never `busy`, never progress (Task 7's F2). */
  refusals: RefusalReason[] | null;
  confirmed: boolean;
  setConfirmed: (value: boolean) => void;
  busy: boolean;
  progress: JobProgress | null;
  /** A failed or cancelled job, or a call that never started one. */
  applyError: string | null;
  /** What the placement actually did. */
  outcome: ApplyOutcome | null;
  /** Ask for the placement. Refusals come back typed and stop here. */
  run: () => Promise<void>;
}

/**
 * The preview → confirm → apply → report machine for a host-placed set.
 *
 * `enabled` is the caller's own answer to "may this set be previewed at
 * all": `PackagePanel` says no while its catalogue has not landed or while
 * the selection holds a package that cannot be placed from the host, and the
 * chain screen says no unless the row it is about is *ready*. Previewing a
 * set that can never be applied reaches the payload's own reader and comes
 * back as a raw English `Password required to decrypt file` — after the user
 * has committed to the selection (Task 7's M3).
 */
export function useHostPlacement({
  treeRoot,
  packageFolder,
  chosen,
  enabled,
}: {
  treeRoot: string | null;
  /** The one folder the archives are read from. `osinstall_collisions` and
   *  `osinstall_add_package` both take a single folder; the list-shaped
   *  question ("which of everything the user has is this archive") is the
   *  slots', and the caller resolves it before it gets here. */
  packageFolder: string | null;
  chosen: string[];
  enabled: boolean;
}): HostPlacement {
  const { t } = useTranslation();

  const [collisions, setCollisions] = useState<CollisionReport[] | null>(null);
  const [collisionsError, setCollisionsError] = useState<string | null>(null);
  const [refusals, setRefusals] = useState<RefusalReason[] | null>(null);
  const [confirmed, setConfirmed] = useState(false);
  const [busy, setBusy] = useState(false);
  const [progress, setProgress] = useState<JobProgress | null>(null);
  const [applyError, setApplyError] = useState<string | null>(null);
  const [outcome, setOutcome] = useState<ApplyOutcome | null>(null);

  const applyJob = useRef<number | null>(null);

  // A primitive dependency rather than the array itself: the chain screen
  // builds this list from one row and would hand a fresh array on every
  // render, which as an effect dependency is an endless re-preview. Two
  // equal strings are the same value to React's own comparison; two equal
  // arrays are not. A package id never holds a newline.
  const chosenKey = chosen.join("\n");
  const ids = useMemo(() => (chosenKey ? chosenKey.split("\n") : []), [chosenKey]);

  // §3's PREVIEW: read-only, recomputed whenever the request changes, and
  // only once at least one package is chosen — an empty selection previews
  // nothing rather than asking the engine a question with no content. A
  // chosen set with no package folder yet is its own, explained state (the
  // caller's), never a call asking the backend to scan `""`.
  useEffect(() => {
    setConfirmed(false);
    setOutcome(null);
    setRefusals(null);
    if (!treeRoot || !packageFolder || ids.length === 0 || !enabled) {
      setCollisions(null);
      setCollisionsError(null);
      return;
    }
    let cancelled = false;
    osinstallCollisions(treeRoot, packageFolder, ids)
      .then((reports) => {
        if (!cancelled) {
          setCollisions(reports);
          setCollisionsError(null);
        }
      })
      .catch((e) => {
        if (!cancelled) {
          setCollisions(null);
          setCollisionsError(errorText(t, e));
        }
      });
    return () => {
      cancelled = true;
    };
    // `t` is stable for a language and re-running on a language change would
    // re-ask the backend for a sentence it already answered.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [treeRoot, packageFolder, ids, enabled]);

  // The add job's own progress — `job-progress` is application-wide, so
  // every update is checked against this placement's own job id first.
  useEffect(() => {
    return subscribeSafely(() =>
      onJobProgress((job) => {
        if (job.id !== applyJob.current) return;
        setProgress(job);
        if (job.state.state === "running") return;

        applyJob.current = null;
        setBusy(false);
        if (job.state.state === "failed") {
          setApplyError(`${job.state.message} (${job.state.error_code})`);
        } else if (job.state.state === "cancelled") {
          setApplyError(t("osinstall.packages.apply.cancelled"));
        }
        // A clean finish says nothing here — its own outcome (files,
        // directories, bytes) arrives on the add-package event below, and
        // that is what flips this into "done".
      })
    );
  }, [t]);

  useEffect(() => {
    return subscribeSafely(() =>
      onOsInstallAddPackageResult((result) => {
        if (result.job_id !== applyJob.current) return;
        setOutcome(result.outcome);
        // Confirming describes the run that was on screen when it was
        // ticked; once that run has finished, the tick is stale.
        setConfirmed(false);
      })
    );
  }, []);

  async function run() {
    if (!treeRoot || !packageFolder || ids.length === 0) return;
    setBusy(true);
    setApplyError(null);
    setProgress(null);
    setOutcome(null);
    setRefusals(null);
    try {
      const result = await osinstallAddPackage(treeRoot, packageFolder, ids);
      if (result.outcome === "refused") {
        // F2: typed, not a job that could only have failed later with
        // Rust's own debug text — nothing was written.
        setBusy(false);
        setRefusals(result.refusals);
        return;
      }
      applyJob.current = result.job_id;
      // `busy` clears on the job's own progress event, or here if the job
      // never actually started.
    } catch (e) {
      setApplyError(errorText(t, e));
      setBusy(false);
      applyJob.current = null;
    }
  }

  return {
    collisions,
    collisionsError,
    refusals,
    confirmed,
    setConfirmed,
    busy,
    progress,
    applyError,
    outcome,
    run,
  };
}

/**
 * What the placement would replace, and why it was refused — the half that
 * belongs **above** the confirmation, because it is what the confirmation is
 * about.
 *
 * Grouped by class with downgrades first (`groupCollisionsForPreview`), the
 * shape legible in the heading before the list is read, and a downgrade's
 * own heading marked by more than colour alone.
 */
export function HostPlacementPreview({ placement }: { placement: HostPlacement }) {
  const { t } = useTranslation();
  const groups = placement.collisions ? groupCollisionsForPreview(placement.collisions) : [];

  return (
    <>
      {placement.collisionsError && (
        <p
          className="badge badge-err"
          data-testid="host-placement-preview-error"
          style={{ display: "block", padding: "6px 12px", fontSize: 12, marginBottom: 12 }}
        >
          {placement.collisionsError}
        </p>
      )}

      {placement.collisions && (
        <div
          data-testid="host-placement-collisions"
          style={{
            maxHeight: 360,
            overflowY: "auto",
            border: "1px solid var(--border)",
            borderRadius: 4,
            padding: "6px 10px",
            marginBottom: 10,
          }}
        >
          {groups.length === 0 && (
            <p className="faint" style={{ fontSize: 11, margin: "4px 0" }}>
              {t("osinstall.packages.preview.nothing")}
            </p>
          )}
          {groups.map((group) => (
            <div key={group.kind} style={{ marginBottom: 10 }}>
              {/* F1: a real heading per class, not a flat sorted list —
                  downgrades' own heading is styled with the error palette so
                  it is marked by more than colour alone (its text already
                  says "Downgrades", not just red). */}
              <div
                className={group.kind === "downgrade" ? "badge badge-err" : "muted"}
                style={{
                  fontSize: 11,
                  fontWeight: 600,
                  margin: "4px 0",
                  display: group.kind === "downgrade" ? "inline-block" : "block",
                }}
              >
                {t(collisionGroupHeadingKey(group.kind), { count: group.reports.length })}
              </div>
              {group.reports.map((report) => {
                const phrase = collisionPhrase(report.collision);
                return (
                  <div
                    key={report.path}
                    data-testid="collision-row"
                    style={{
                      fontSize: 11,
                      padding: "3px 0",
                      borderBottom: "1px solid var(--border)",
                    }}
                  >
                    <div style={{ display: "flex", justifyContent: "space-between", gap: 8 }}>
                      <span style={{ wordBreak: "break-all" }}>{report.path}</span>
                      <span
                        className={group.kind === "downgrade" ? undefined : "faint"}
                        style={
                          group.kind === "downgrade" ? { color: "var(--err-text)" } : undefined
                        }
                      >
                        {t(phrase.key, phrase.params)}
                      </span>
                    </div>
                    {!report.declared && (
                      <span className="badge badge-warn" style={{ fontSize: 10 }}>
                        {t("osinstall.packages.undeclared")}
                      </span>
                    )}
                  </div>
                );
              })}
            </div>
          ))}
        </div>
      )}

      {placement.refusals && placement.refusals.length > 0 && (
        <div
          className="badge badge-err"
          data-testid="host-placement-refusals"
          style={{ display: "block", padding: "8px 10px", margin: "0 0 12px", fontSize: 11 }}
        >
          <p style={{ margin: "0 0 6px", fontWeight: 600 }}>
            {t("osinstall.packages.refused.heading")}
          </p>
          <ul style={{ margin: 0, paddingLeft: 18 }}>
            {placement.refusals.map((reason, i) => {
              const phrase = refusalPhrase(reason);
              return (
                <li key={i} style={{ padding: "2px 0" }}>
                  {t(phrase.key, phrase.params)}
                </li>
              );
            })}
          </ul>
        </div>
      )}
    </>
  );
}

/**
 * What the placement *did* — the half that belongs beside the button that
 * asked for it (ART-202: a run that ended has to say so where the control
 * is).
 *
 * **Takes the two values rather than the whole placement**, because the two
 * screens hold them for different lengths of time. `PackagePanel` hands
 * straight through. The chain screen keeps its own copy: a successful
 * placement makes the chain re-ask, the row becomes installed and the button
 * moves on — which would clear the hook's own outcome and leave the screen
 * saying nothing about work it had just done.
 */
export function HostPlacementReport({
  outcome,
  error,
}: {
  outcome: ApplyOutcome | null;
  error: string | null;
}) {
  const { t } = useTranslation();

  return (
    <>
      {error && (
        <div
          className="badge badge-err"
          data-testid="host-placement-error"
          style={{ display: "block", padding: "6px 12px", fontSize: 12, marginBottom: 12 }}
        >
          {error}
        </div>
      )}
      {outcome && (
        <div
          className="badge badge-ok"
          data-testid="host-placement-outcome"
          style={{ display: "block", padding: "6px 12px", fontSize: 12, marginBottom: 12 }}
        >
          {t("osinstall.packages.apply.done")}{" "}
          {t("osinstall.packages.apply.outcome", {
            files: outcome.files,
            directories: outcome.directories,
            bytes: formatBytes(outcome.bytes),
          })}
        </div>
      )}
    </>
  );
}
