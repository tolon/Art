import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";

import {
  fraction,
  isRunning,
  jobCancel,
  jobClearFinished,
  jobList,
  jobStatusLabel,
  onJobProgress,
  type JobProgress,
} from "@/lib/jobs";

/**
 * Global indicator for background work (spec §54, §55).
 *
 * Lives in the app shell rather than in each studio: a scan started in the
 * Collection is still running when the user walks over to ADF Studio, and they
 * should be able to see and stop it from wherever they are.
 *
 * Renders nothing when there is nothing to say.
 */
export function JobBar() {
  const { t } = useTranslation();
  const [jobs, setJobs] = useState<JobProgress[]>([]);

  useEffect(() => {
    let cancelled = false;
    let unlisten: (() => void) | undefined;

    void (async () => {
      // Pick up anything already running — the bar may mount mid-job.
      try {
        const existing = await jobList();
        if (!cancelled) setJobs(existing);
      } catch {
        // A missing job registry is not worth surfacing to the user.
      }

      const stop = await onJobProgress((update) => {
        setJobs((current) => {
          const index = current.findIndex((j) => j.id === update.id);
          if (index === -1) return [...current, update];
          const next = [...current];
          next[index] = update;
          return next;
        });
      });

      if (cancelled) stop();
      else unlisten = stop;
    })();

    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, []);

  const running = jobs.filter(isRunning);
  const finished = jobs.filter((j) => !isRunning(j));

  // Finished jobs are worth a moment's acknowledgement, but only failures and
  // cancellations are worth keeping on screen once the user has seen them.
  const notable = finished.filter(
    (j) => j.state.state === "failed" || j.state.state === "cancelled"
  );

  if (running.length === 0 && notable.length === 0) return null;

  async function dismiss() {
    try {
      await jobClearFinished();
    } finally {
      setJobs((current) => current.filter(isRunning));
    }
  }

  return (
    <div
      className="card job-bar"
      style={{
        margin: "0 0 12px",
        background: "var(--bg-elevated)",
        display: "flex",
        flexDirection: "column",
        gap: 8,
      }}
    >
      {running.map((job) => (
        <JobRow key={job.id} job={job} />
      ))}

      {notable.map((job) => (
        <JobRow key={job.id} job={job} />
      ))}

      {notable.length > 0 && (
        <div>
          <button className="btn btn-sm" onClick={() => void dismiss()}>
            {t("components.jobBar.dismiss")}
          </button>
        </div>
      )}
    </div>
  );
}

function JobRow({ job }: { job: JobProgress }) {
  const { t } = useTranslation();
  const pct = fraction(job);
  const running = isRunning(job);
  const failed = job.state.state === "failed";
  const status = jobStatusLabel(job);

  return (
    <div style={{ fontSize: 12 }}>
      <div
        style={{
          display: "flex",
          alignItems: "center",
          gap: 8,
          justifyContent: "space-between",
        }}
      >
        <span>
          <strong>{job.title}</strong>
          <span className="faint" style={{ marginLeft: 6 }}>
            {t(status.key, status.params)}
            {running && pct !== null && ` · ${Math.round(pct * 100)}%`}
            {running && pct === null && job.done > 0 && ` · ${job.done}`}
          </span>
        </span>
        {running && (
          <button className="btn btn-sm" onClick={() => void jobCancel(job.id)}>
            {t("components.jobBar.stop")}
          </button>
        )}
      </div>

      {running && (
        <div
          aria-hidden
          style={{
            height: 4,
            marginTop: 4,
            borderRadius: 2,
            background: "var(--border)",
            overflow: "hidden",
          }}
        >
          <div
            style={{
              height: "100%",
              // An unknown total gets a fixed sliver rather than a bar that
              // pretends to know how far along the work is.
              width: pct === null ? "25%" : `${pct * 100}%`,
              background: "var(--accent)",
              transition: "width 120ms linear",
            }}
          />
        </div>
      )}

      {job.message && running && (
        <div className="muted" style={{ wordBreak: "break-all" }}>
          {job.message}
        </div>
      )}

      {failed && job.state.state === "failed" && (
        <div className="muted">{job.state.message}</div>
      )}
    </div>
  );
}
