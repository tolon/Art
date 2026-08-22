// The curated package catalogue's own screen (spec addendum §41.5, Task 7 of
// the package-bundles plan). Mounted inside Aminet Studio.
//
// `bundlesList()` is read-only and fetches nothing (`@/lib/bundles`'s own
// doc comment), so it is safe to call on mount. `bundlesDownload()` is the
// one call that touches the network, and this screen never calls it except
// from `run()`, which only ever runs from the button's own click handler —
// nothing here fetches on mount, on a tick, or on a re-render.
//
// Selection is per **set**, not per entry: `bundles.intro` says "pick a set",
// and every entry in a ticked set is handed to `bundlesDownload` in the
// catalogue's own order. Entries ART cannot honestly promise to fetch today
// — `user-supplied` files, and `mirror`-sourced ones (no named-mirror
// registry exists yet, so every one of them resolves to `Refused`) — are
// never offered a tick that implies otherwise (§10/§89): they get their own
// sentence instead, inside the set's card.

import { useEffect, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";

import {
  bundlesDownload,
  bundlesList,
  onBundleDownloadResult,
  type BundleReport,
  type BundleSummary,
  type EntryReport,
  type EntrySummary,
} from "@/lib/bundles";
import { onJobProgress, type JobProgress } from "@/lib/jobs";
import { isTextList } from "@/lib/remembered";
import { useRemembered } from "@/lib/useRemembered";

/** Tally of the six `EntryOutcome` kinds a finished report can hold. Six,
 *  not five — `not-placed` is Task 5 review's own Critical fix, and a report
 *  that only ever shows five sentences is the defect it guards against. */
interface OutcomeTally {
  downloaded: number;
  alreadyHave: number;
  notPlaced: number;
  refused: number;
  failed: number;
  skipped: number;
}

function tally(report: BundleReport): OutcomeTally {
  const counts: OutcomeTally = {
    downloaded: 0,
    alreadyHave: 0,
    notPlaced: 0,
    refused: 0,
    failed: 0,
    skipped: 0,
  };
  for (const entry of report.entries) {
    switch (entry.outcome.outcome) {
      case "downloaded":
        counts.downloaded += 1;
        break;
      case "already-have":
        counts.alreadyHave += 1;
        break;
      case "not-placed":
        counts.notPlaced += 1;
        break;
      case "refused":
        counts.refused += 1;
        break;
      case "failed":
        counts.failed += 1;
        break;
      case "skipped":
        counts.skipped += 1;
        break;
    }
  }
  return counts;
}

/** The three kinds this screen still cannot honestly promise to fetch, on
 *  top of `mirror`. `resolve.rs` (`core/sources/bundle/resolve.rs`) refuses
 *  every `aminet-search` and every `github-release` unconditionally too —
 *  ART has no version-resolution engine and no GitHub mirror configured yet
 *  — so a set built entirely of one of these (`emu68` is 4/4
 *  `github-release`) must not offer a tick that can do nothing (§10/§89). */
type UnfetchableKind = Exclude<EntrySummary["kind"], "aminet">;

/** Entries this screen cannot honestly promise to fetch. Rendered with their
 *  own sentence, never a tick. */
function cannotFetch(kind: EntrySummary["kind"]): kind is UnfetchableKind {
  return kind !== "aminet";
}

/** The i18n key for one of the four unfetchable kinds' own **complete**
 *  sentence — never a shared wrapper with a reason spliced in. The four
 *  kinds are not the same fact: `user-supplied` genuinely is the user's to
 *  bring, so only that one says "you supply it"; `mirror`, `github-release`
 *  and `aminet-search` are gaps in ART itself (no mirror configured, no
 *  release-asset fetcher, no version-resolution engine) and telling a user
 *  to supply one of those themselves would be false (CLAUDE.md, "The
 *  failure that does not crash"). */
function sentenceKey(kind: UnfetchableKind): string {
  switch (kind) {
    case "mirror":
      return "bundles.entry.mirror";
    case "user-supplied":
      return "bundles.entry.userSupplied";
    case "github-release":
      return "bundles.entry.githubRelease";
    case "aminet-search":
      return "bundles.entry.aminetSearch";
  }
}

/** How many of a set's entries ART can actually fetch today. */
function fetchableCount(entries: EntrySummary[]): number {
  return entries.filter((entry) => !cannotFetch(entry.kind)).length;
}

/** One line per outcome kind, carrying whatever string that outcome itself
 *  carries — `path`, `existing`, `why` or `error`. The count badges above
 *  stay as a summary; this is the detail underneath naming which entries
 *  and what happened to each (CLAUDE.md: "a refusal must be actionable" and
 *  "a user told 'it failed' ... has been given nothing"). */
function entrySentence(t: (key: string, options?: Record<string, unknown>) => string, entry: EntryReport): string {
  switch (entry.outcome.outcome) {
    case "downloaded":
      return t("bundles.result.entry.downloaded", { path: entry.outcome.path });
    case "already-have":
      return t("bundles.result.entry.alreadyHave", { path: entry.outcome.path });
    case "not-placed":
      return t("bundles.result.entry.notPlaced", { existing: entry.outcome.existing });
    case "refused":
      return t("bundles.result.entry.refused", { why: entry.outcome.why });
    case "failed":
      return t("bundles.result.entry.failed", { error: entry.outcome.error });
    case "skipped":
      return t("bundles.result.entry.skipped");
  }
}

/** Entries sharing a non-null `exclusiveGroup`, grouped in catalogue order. */
function exclusiveGroups(entries: EntrySummary[]): Map<string, EntrySummary[]> {
  const groups = new Map<string, EntrySummary[]>();
  for (const entry of entries) {
    if (!entry.exclusiveGroup) continue;
    const list = groups.get(entry.exclusiveGroup) ?? [];
    list.push(entry);
    groups.set(entry.exclusiveGroup, list);
  }
  return groups;
}

export function BundlePanel() {
  const { t } = useTranslation();
  const [sets, setSets] = useState<BundleSummary[] | null>(null);
  const [listError, setListError] = useState<string | null>(null);
  // Which sets are ticked — remembered, not `useState`: "nothing changes
  // unless the user changes it" holds for the whole product
  // (CLAUDE.md, and the user's own words are "ürünün tamamı için"), and a
  // set list is no exception just because an earlier pass here treated it
  // as transient. A guarded `isTextList` means a hand-edited or stale
  // settings file falls back to an empty selection rather than putting a
  // bad value on screen.
  const [chosenIds, setChosenIds] = useRemembered<string[]>("bundles.chosenSets", isTextList, []);
  const [busy, setBusy] = useState(false);
  const [runError, setRunError] = useState<string | null>(null);
  const [report, setReport] = useState<BundleReport | null>(null);
  const [progress, setProgress] = useState<JobProgress | null>(null);
  const pendingJob = useRef<number | null>(null);

  // Read-only, and the one round trip this screen makes without the user
  // asking — `bundlesList` parses the shipped catalogue and fetches nothing.
  useEffect(() => {
    let cancelled = false;
    bundlesList()
      .then((result) => {
        if (!cancelled) setSets(result);
      })
      .catch((e) => {
        if (!cancelled) setListError(String(e));
      });
    return () => {
      cancelled = true;
    };
  }, []);

  // The one listener a finished download arrives on. Nothing else in ART
  // triggers `bundles-download-result`, so this is not filtered by job id —
  // the same shape a screen with exactly one job lane can safely take.
  useEffect(() => {
    const unlisten = onBundleDownloadResult((result) => {
      setReport(result.report);
      setBusy(false);
      setProgress(null);
      pendingJob.current = null;
    });
    return () => {
      void unlisten.then((off) => off());
    };
  }, []);

  // Progress and the job's own terminal state (§54, §55) — a job that fails
  // or is cancelled emits no bundle-download-result, so this is what stops
  // the button from staying disabled forever. Filtered by id: `job-progress`
  // is shared across every job in the app, not just this screen's own.
  useEffect(() => {
    const unlisten = onJobProgress((job) => {
      if (job.id !== pendingJob.current) return;
      setProgress(job);
      if (job.state.state === "failed") {
        setRunError(`${job.state.message} (${job.state.error_code})`);
        setBusy(false);
        pendingJob.current = null;
      } else if (job.state.state === "cancelled" || job.state.state === "superseded") {
        setBusy(false);
        pendingJob.current = null;
      }
    });
    return () => {
      void unlisten.then((off) => off());
    };
  }, []);

  function toggleSet(id: string) {
    setChosenIds(
      chosenIds.includes(id) ? chosenIds.filter((chosen) => chosen !== id) : [...chosenIds, id]
    );
  }

  // The `hepsi` set the design specifies: "everything", computed rather than
  // ever listed as catalogue data, so it cannot drift from the 14 shipped
  // sets. A plain select-all control over their ids — but only the ids of
  // sets with at least one fetchable entry. A set like `emu68` (4/4
  // `github-release`) has its own card checkbox individually `disabled`
  // (line ~460 below); ticking it here anyway would render it checked and
  // disabled at once, telling the user two things at the same time.
  // Second re-review, item 3.
  const selectableSetIds = useMemo(
    () => (sets ?? []).filter((set) => fetchableCount(set.entries) > 0).map((set) => set.id),
    [sets]
  );
  const allChosen =
    selectableSetIds.length > 0 && selectableSetIds.every((id) => chosenIds.includes(id));
  function toggleAll() {
    setChosenIds(allChosen ? [] : selectableSetIds);
  }

  async function run() {
    setRunError(null);
    setReport(null);
    const ids = (sets ?? [])
      .filter((set) => chosenIds.includes(set.id))
      .flatMap((set) => set.entries.map((entry) => entry.id));
    if (ids.length === 0) {
      setRunError(t("bundles.blocked.nothingChosen"));
      return;
    }
    setBusy(true);
    setProgress(null);
    try {
      pendingJob.current = await bundlesDownload(ids);
    } catch (e) {
      setRunError(String(e));
      setBusy(false);
      pendingJob.current = null;
    }
  }

  const counts = useMemo(() => (report ? tally(report) : null), [report]);

  return (
    <section className="card" style={{ marginBottom: 12 }}>
      <h2 style={{ fontSize: 16, margin: 0 }}>{t("bundles.heading")}</h2>
      <p className="muted" style={{ fontSize: 13 }}>
        {t("bundles.intro")}
      </p>

      {listError && (
        <div className="badge badge-err" style={{ display: "block" }}>
          {t("bundles.err.listFailed", { error: listError })}
        </div>
      )}

      {sets === null && !listError && (
        <p className="muted" style={{ fontSize: 13 }}>
          {t("bundles.loading")}
        </p>
      )}

      {sets && sets.length > 0 && (
        <>
          <label
            style={{
              display: "flex",
              alignItems: "baseline",
              gap: 6,
              cursor: "pointer",
              marginTop: 8,
            }}
          >
            <input
              type="checkbox"
              checked={allChosen}
              onChange={toggleAll}
              disabled={busy}
              aria-label={t("bundles.set.hepsi")}
            />
            <strong style={{ fontSize: 13 }}>{t("bundles.set.hepsi")}</strong>
          </label>

          <div
            style={{
              display: "grid",
              gap: 10,
              gridTemplateColumns: "repeat(auto-fit, minmax(260px, 1fr))",
              marginTop: 8,
            }}
          >
            {sets.map((set) => (
              <BundleCard
                key={set.id}
                set={set}
                checked={chosenIds.includes(set.id)}
                onToggle={() => toggleSet(set.id)}
                disabled={busy}
              />
            ))}
          </div>
        </>
      )}

      <div
        style={{
          display: "flex",
          gap: 8,
          alignItems: "center",
          marginTop: 12,
          flexWrap: "wrap",
        }}
      >
        <button
          className="btn btn-primary"
          onClick={() => void run()}
          disabled={busy || sets === null}
        >
          {busy ? t("bundles.running") : t("bundles.run")}
        </button>
        {busy && progress && (
          <span className="muted" style={{ fontSize: 12 }}>
            {t("bundles.progress", {
              name: progress.message,
              done: progress.done,
              total: progress.total ?? progress.done,
            })}
          </span>
        )}
      </div>

      {runError && (
        <div className="badge badge-err" style={{ display: "block", marginTop: 8 }}>
          {runError}
        </div>
      )}

      {report && counts && (
        <div style={{ marginTop: 10, fontSize: 13 }} data-testid="bundle-report">
          {counts.downloaded > 0 && (
            <div className="badge badge-ok" style={{ display: "block", marginBottom: 4 }}>
              {t("bundles.result.downloaded", { count: counts.downloaded })}
            </div>
          )}
          {counts.alreadyHave > 0 && (
            <div className="badge" style={{ display: "block", marginBottom: 4 }}>
              {t("bundles.result.alreadyHave", { count: counts.alreadyHave })}
            </div>
          )}
          {counts.notPlaced > 0 && (
            <div className="badge badge-warn" style={{ display: "block", marginBottom: 4 }}>
              {t("bundles.result.notPlaced", { count: counts.notPlaced })}
            </div>
          )}
          {counts.refused > 0 && (
            <div className="badge badge-warn" style={{ display: "block", marginBottom: 4 }}>
              {t("bundles.result.refused", { count: counts.refused })}
            </div>
          )}
          {counts.failed > 0 && (
            <div className="badge badge-err" style={{ display: "block", marginBottom: 4 }}>
              {t("bundles.result.failed", { count: counts.failed })}
            </div>
          )}
          {counts.skipped > 0 && (
            <div className="badge" style={{ display: "block", marginBottom: 4 }}>
              {t("bundles.result.skipped", { count: counts.skipped })}
            </div>
          )}

          {/* The counts above are a summary; this names each entry and the
              string its own outcome carries — which four ART could not
              fetch and why, which one failed and how, where the rest
              landed. A count alone cannot answer any of that. */}
          <ul
            data-testid="bundle-report-detail"
            style={{ listStyle: "none", margin: "6px 0 0", padding: 0 }}
          >
            {report.entries.map((entry) => (
              <li key={entry.id} style={{ padding: "2px 0" }}>
                <strong>{entry.name}</strong>
                {" — "}
                {entrySentence(t, entry)}
              </li>
            ))}
          </ul>
        </div>
      )}
    </section>
  );
}

function BundleCard({
  set,
  checked,
  onToggle,
  disabled,
}: {
  set: BundleSummary;
  checked: boolean;
  onToggle: () => void;
  disabled: boolean;
}) {
  const { t } = useTranslation();
  // The one dynamic translate call this file makes — every one of the 14
  // shipped set ids has its own `bundles.set.<id>` leaf (see the key table
  // in the task brief); `src/i18n/dead-keys.test.ts` treats a
  // template-literal prefix ending in `.${` as reachable for this pattern.
  const label = t(`bundles.set.${set.id}`);

  // Flagged before the tick, never after — the sentence is informational,
  // not a gate, and it must not be discoverable only once the file is
  // already on disk.
  const flagged = set.entries.filter((entry) => entry.permission !== null);
  const groups = exclusiveGroups(set.entries);
  const fetchable = fetchableCount(set.entries);

  return (
    <div className="card" style={{ padding: 10 }}>
      {flagged.length > 0 && (
        <div
          className="badge badge-warn"
          style={{ display: "block", fontSize: 12, marginBottom: 6 }}
          data-testid="bundle-permission-warning"
        >
          {t("bundles.permission.warning", {
            names: flagged.map((entry) => entry.name).join(", "),
          })}
        </div>
      )}

      <label style={{ display: "flex", alignItems: "baseline", gap: 6, cursor: "pointer" }}>
        <input
          type="checkbox"
          checked={checked}
          // Nothing can come of ticking a set ART cannot fetch a single
          // entry from — `emu68` is 4/4 `github-release`, and offering a
          // working-looking tick over it is exactly the "offer what it
          // cannot do" defect §10/§89 forbid.
          disabled={disabled || fetchable === 0}
          onChange={onToggle}
          aria-label={`${set.id} — ${label}`}
        />
        <strong style={{ fontSize: 14 }}>{label}</strong>
      </label>
      <div className="faint" style={{ fontSize: 11, marginTop: 2 }}>
        {t("bundles.entryCount", { count: set.entries.length })}
      </div>
      {fetchable < set.entries.length && (
        <div className="faint" style={{ fontSize: 11 }}>
          {t("bundles.fetchableCount", { fetchable, total: set.entries.length })}
        </div>
      )}

      {/* Shown, never enforced (the spec's own correction): two entries
          sharing a group are alternatives *to install*, and downloading
          both is a legitimate thing to want — so nothing here disables
          either tick. There is no per-entry tick to disable in the first
          place; this note is purely informational. */}
      {[...groups.entries()].map(([group, entries]) => (
        <div key={group} className="faint" style={{ fontSize: 11, marginTop: 4 }}>
          {t("bundles.entry.alternatives", {
            names: entries.map((entry) => entry.name).join(" / "),
          })}
        </div>
      ))}

      <ul style={{ listStyle: "none", margin: "6px 0 0", padding: 0, fontSize: 12 }}>
        {set.entries.map((entry) => (
          <li key={entry.id} style={{ padding: "2px 0" }}>
            {cannotFetch(entry.kind) ? (
              <span className="muted">
                <strong>{entry.name}</strong>
                {" — "}
                {t(sentenceKey(entry.kind))}
              </span>
            ) : (
              <span>{entry.name}</span>
            )}
          </li>
        ))}
      </ul>
    </div>
  );
}
