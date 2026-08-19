// Landing an official (or unofficial) update package on an AmigaOS
// distribution tree that already exists (Task 7 — SD-2 · G5's content
// layer). The engine (`core/osinstall/{package,collide,apply}.rs`) has
// existed since Tasks 1-6; nothing about it was reachable from a screen
// until this component.
//
// Follows the shape `OsInstall.tsx` already worked out for its own
// checklist, named directly in this task's brief:
//
//   - **The checklist is loaded, not hardcoded**, and paired with whether
//     each package's own archive was actually found — `available: false`
//     on a package whose file is absent, so a checkbox never promises what
//     ART cannot deliver.
//   - **A late-landing load must not overwrite something the user just
//     touched.** Every effect below carries the same `cancelled` guard
//     `OsInstall.tsx`'s own component/plan effects do.
//   - **`treeRoot`, `packageFolder` and `chosen` are controlled**, the same
//     way `OsInstall.tsx` owns `chosen`/`excludedConditional` for release
//     components — this panel renders and asks; the caller decides how (or
//     whether) a pick gets remembered, through `on*Change`.
//
// Five things this screen owns that no other task does (from the brief):
//
//   1. The shipped packages, each with a checkbox, its name, and what it
//      requires — both of another **package** (`requires`) and of a
//      release **component** (`requiresComponents`, ART-162's own rule)
//      shown before any refusal, not only after one.
//   2. Once at least one is chosen, the preview — grouped by class,
//      downgrades first (`groupCollisionsForPreview`), with the shape
//      legible in the heading before the list is read
//      (`collisionCounts`). `Identical` never reaches this panel at all —
//      the core excludes it before a `CollisionReport` exists.
//   3. One confirmation for the whole set, never one per file — a BoingBag
//      replaces 211 files, and 211 confirmations teach a user to click
//      through.
//   4. An undeclared collision marked beside its own row, not folded into
//      the count.
//   5. A line saying what this round does not do: it can place only the
//      packages it ships a recipe for, and says so by name and by count —
//      never implying a fourth is coming.
//
// ## Fix round 1 (Task 7's own review)
//
// **F1 — a downgrade is marked as one, not just coloured differently.** The
// preview is now grouped into real, headed sections (downgrades first),
// and `collisionPhrase` gives `upgrade`/`downgrade` their own keys instead
// of sharing one — a downgrade row now says "Downgrade: 45.9 → 37.4" in
// its own words, not merely in a different colour from an upgrade's.
//
// **F2 — a refused add shows the real sentence.** `osinstallAddPackage` now
// answers `AddPackageResult`; a `"refused"` outcome renders every
// `RefusalReason` through `refusalPhrase`, the same mirror the install
// screen's own refusals go through, instead of starting a job that could
// only have failed with Rust's own debug text.
//
// **F3 — no more dead ends.** A remembered pick whose archive has vanished
// stays *tickable off* (`disabled` no longer fires for a checked box) and
// says why (`archiveGone`); a chosen set previewed with no package folder
// yet says so (`needsFolderToPreview`) instead of asking the backend to
// scan `""` and rendering nothing.
//
// **F7 — every subscription goes through `subscribeSafely`.** A promise
// that never settles because the effect tore down first no longer leaks a
// live Tauri listener, and a rejected one (no IPC bridge, e.g. under test)
// no longer reaches the console as unhandled.
//
// **F10 — the outcome is shown, and confirming resets.** `onOsInstallAddPackageResult`
// carries the files/directories/bytes actually written; `confirmed` clears
// once a run finishes, the same way `OsInstall.tsx`'s own run confirmation
// does not survive its build.

import { useEffect, useRef, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { useTranslation } from "react-i18next";

import {
  collisionCounts,
  collisionGroupHeadingKey,
  collisionPhrase,
  groupCollisionsForPreview,
  onOsInstallAddPackageResult,
  osinstallAddPackage,
  osinstallCollisions,
  osinstallPackages,
  refusalPhrase,
  type ApplyOutcome,
  type CollisionReport,
  type PackageSummary,
  type RefusalReason,
} from "@/lib/osinstall";
import { fraction, onJobProgress, subscribeSafely, type JobProgress } from "@/lib/jobs";
import { formatBytes } from "@/lib/panel";
import { Field } from "@/components/osbuilder/Field";

export interface PackagePanelProps {
  /** The distribution tree's own root — where `distribution.json` lives.
   *  Controlled by the caller: a package can land on any tree ART or the
   *  user already built, not only the one an install just finished — the
   *  same reasoning `OsInstall.tsx`'s own Verify section already applies to
   *  `verifyDistRoot`. */
  treeRoot: string | null;
  onTreeRootChange?: (path: string | null) => void;
  /** Where the update archives are — a **second** folder, never the media
   *  folder (`core/osinstall/plan.rs`'s own module doc comment: the owner
   *  keeps discs in one folder and archives in another). `null`/omitted
   *  renders the checklist with nothing marked available and previews
   *  nothing until one is chosen anyway. */
  packageFolder?: string | null;
  onPackageFolderChange?: (path: string | null) => void;
  /** The package ids the user has ticked. Controlled, the same way
   *  `OsInstall.tsx` owns `chosen` for release components — persisting it
   *  (or not) is the caller's decision, through `onChosenChange`. */
  chosen: string[];
  onChosenChange?: (ids: string[]) => void;
}

export function PackagePanel({
  treeRoot,
  onTreeRootChange,
  packageFolder = null,
  onPackageFolderChange,
  chosen,
  onChosenChange,
}: PackagePanelProps) {
  const { t } = useTranslation();

  const [catalogue, setCatalogue] = useState<PackageSummary[] | null>(null);
  const [catalogueError, setCatalogueError] = useState(false);
  const [collisions, setCollisions] = useState<CollisionReport[] | null>(null);
  const [collisionsError, setCollisionsError] = useState<string | null>(null);
  const [confirmed, setConfirmed] = useState(false);

  const applyJob = useRef<number | null>(null);
  const [busy, setBusy] = useState(false);
  const [progress, setProgress] = useState<JobProgress | null>(null);
  const [applyError, setApplyError] = useState<string | null>(null);
  const [applyOutcome, setApplyOutcome] = useState<ApplyOutcome | null>(null);
  /** F2: a refused selection, resolved and typed before anything was
   *  written — never a job, never `busy`, never progress. */
  const [refusals, setRefusals] = useState<RefusalReason[] | null>(null);

  // The checklist: loaded whenever the package folder changes. Left `null`
  // (never `[]`) while nothing has arrived, so a row is never dropped just
  // because a fetch has not landed yet — the same distinction `OsInstall.tsx`
  // draws between "not loaded" and "loaded empty".
  useEffect(() => {
    if (!packageFolder) {
      setCatalogue(null);
      setCatalogueError(false);
      return;
    }
    let cancelled = false;
    osinstallPackages(packageFolder)
      .then((list) => {
        if (!cancelled) {
          setCatalogue(list);
          setCatalogueError(false);
        }
      })
      .catch(() => {
        if (!cancelled) {
          setCatalogue(null);
          setCatalogueError(true);
        }
      });
    return () => {
      cancelled = true;
    };
  }, [packageFolder]);

  // The preview: read-only (§3's PREVIEW), recomputed whenever the request
  // changes, and only once at least one package is chosen — an empty
  // selection previews nothing rather than asking the engine a question
  // with no content. F3/F5: a chosen set with no package folder yet is its
  // own, explained state — never a call asking the backend to scan `""`.
  useEffect(() => {
    setConfirmed(false);
    setApplyOutcome(null);
    setRefusals(null);
    if (!treeRoot || chosen.length === 0 || !packageFolder) {
      setCollisions(null);
      setCollisionsError(null);
      return;
    }
    let cancelled = false;
    osinstallCollisions(treeRoot, packageFolder, chosen)
      .then((reports) => {
        if (!cancelled) {
          setCollisions(reports);
          setCollisionsError(null);
        }
      })
      .catch((e) => {
        if (!cancelled) {
          setCollisions(null);
          setCollisionsError(String(e));
        }
      });
    return () => {
      cancelled = true;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [treeRoot, packageFolder, chosen]);

  // The add job's own progress — `job-progress` is application-wide, so
  // every update is checked against this panel's own job id first.
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
        // directories, bytes) arrives on `OSINSTALL_ADD_PACKAGE_EVENT`
        // below, and that is what flips this panel into "done".
      })
    );
  }, [t]);

  // The add job's own outcome (F10) — logged before this task's fix round,
  // and nowhere else; this is what lets the panel say more than "Added."
  useEffect(() => {
    return subscribeSafely(() =>
      onOsInstallAddPackageResult((result) => {
        if (result.job_id !== applyJob.current) return;
        setApplyOutcome(result.outcome);
        // F10: confirming describes the run that was on screen when it was
        // ticked; once that run has finished, the tick is stale — the same
        // rule `OsInstall.tsx`'s own build confirmation follows.
        setConfirmed(false);
      })
    );
  }, []);

  async function chooseTreeRoot() {
    const picked = await open({
      directory: true,
      multiple: false,
      title: t("osinstall.packages.treeRoot.chooseTitle"),
    });
    if (typeof picked === "string") onTreeRootChange?.(picked);
  }

  async function choosePackageFolder() {
    const picked = await open({
      directory: true,
      multiple: false,
      title: t("osinstall.packages.packageFolder.chooseTitle"),
    });
    if (typeof picked === "string") onPackageFolderChange?.(picked);
  }

  function toggle(id: string) {
    const next = chosen.includes(id) ? chosen.filter((x) => x !== id) : [...chosen, id];
    onChosenChange?.(next);
  }

  async function runApply() {
    if (!treeRoot || !packageFolder || chosen.length === 0) return;
    setBusy(true);
    setApplyError(null);
    setProgress(null);
    setApplyOutcome(null);
    setRefusals(null);
    try {
      const result = await osinstallAddPackage(treeRoot, packageFolder, chosen);
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
      setApplyError(String(e));
      setBusy(false);
      applyJob.current = null;
    }
  }

  const counts = collisions ? collisionCounts(collisions) : null;
  const groups = collisions ? groupCollisionsForPreview(collisions) : [];
  const pct = progress ? fraction(progress) : null;
  const shippedCount = catalogue?.length ?? 3;
  const previewNeedsFolder = treeRoot !== null && chosen.length > 0 && !packageFolder;

  return (
    <section className="card" style={{ marginBottom: 16 }}>
      <h2 style={{ fontSize: 16, marginTop: 0 }}>{t("osinstall.packages.heading")}</h2>
      <p className="muted" style={{ fontSize: 12, margin: "4px 0 12px" }}>
        {t("osinstall.packages.intro")}
      </p>
      {/* Requirement 5: what this round does not do, stated plainly rather
          than left for the user to conclude their own archive is broken. */}
      <p className="faint" style={{ fontSize: 11, margin: "0 0 12px" }}>
        {t("osinstall.packages.scopeNote", { count: shippedCount })}
      </p>

      <Field
        label={t("osinstall.packages.treeRoot.label")}
        value={treeRoot}
        empty={t("osinstall.packages.treeRoot.none")}
        onChoose={() => void chooseTreeRoot()}
        choose={t("common.browse")}
        hint={t("osinstall.packages.treeRoot.hint")}
      />
      <Field
        label={t("osinstall.packages.packageFolder.label")}
        value={packageFolder}
        empty={t("osinstall.packages.packageFolder.none")}
        onChoose={() => void choosePackageFolder()}
        choose={t("common.browse")}
        hint={t("osinstall.packages.packageFolder.hint")}
      />

      {catalogueError && (
        <p className="badge badge-err" style={{ fontSize: 11, margin: "0 0 12px", display: "inline-block" }}>
          {t("osinstall.packages.catalogue.unavailable")}
        </p>
      )}
      {!packageFolder && (
        <p className="faint" style={{ fontSize: 11, margin: "0 0 12px" }}>
          {t("osinstall.packages.catalogue.needsFolder")}
        </p>
      )}

      <div style={{ display: "flex", flexDirection: "column", gap: 6, marginBottom: 12 }}>
        {(catalogue ?? []).map((pkg) => {
          const checked = chosen.includes(pkg.id);
          // F3: a checked-but-now-unavailable package must stay untickable
          // off — only *checking* an unavailable one is refused, never
          // unchecking it. The old `disabled={!pkg.available}` disabled
          // both directions at once, so a remembered pick whose archive had
          // since vanished could never be cleared from the screen.
          const disabled = !pkg.available && !checked;
          return (
            <div
              key={pkg.id}
              style={{
                border: "1px solid var(--border)",
                borderRadius: 4,
                padding: "6px 10px",
                background: checked ? "var(--bg-hover)" : "var(--bg)",
              }}
            >
              <label style={{ display: "flex", gap: 8, alignItems: "center", fontSize: 13 }}>
                <input
                  type="checkbox"
                  checked={checked}
                  disabled={disabled}
                  onChange={() => toggle(pkg.id)}
                />
                <strong>{pkg.name}</strong>
                {!pkg.available && (
                  <span className="badge badge-muted" style={{ fontSize: 10 }}>
                    {t("osinstall.packages.archiveMissing")}
                  </span>
                )}
              </label>
              {checked && !pkg.available && (
                <p className="faint" style={{ fontSize: 11, margin: "4px 0 0" }}>
                  {t("osinstall.packages.archiveGone")}
                </p>
              )}
              {pkg.requires.length > 0 && (
                <p className="faint" style={{ fontSize: 11, margin: "4px 0 0" }}>
                  {t("osinstall.packages.requiresPackages", { list: pkg.requires.join(", ") })}
                </p>
              )}
              {pkg.requiresComponents.length > 0 && (
                <p className="faint" style={{ fontSize: 11, margin: "4px 0 0" }}>
                  {t("osinstall.packages.requiresComponents", {
                    list: pkg.requiresComponents.join(", "),
                  })}
                </p>
              )}
            </div>
          );
        })}
      </div>

      {chosen.length > 0 && (
        <div style={{ marginTop: 8 }}>
          <h3 style={{ fontSize: 14, margin: "0 0 8px" }}>
            {counts
              ? t("osinstall.packages.preview.heading", { ...counts })
              : t("osinstall.packages.preview.loading")}
          </h3>

          {previewNeedsFolder && (
            <p className="faint" style={{ fontSize: 11, margin: "0 0 12px" }}>
              {t("osinstall.packages.needsFolderToPreview", { count: chosen.length })}
            </p>
          )}

          {collisionsError && (
            <p className="badge badge-err" style={{ display: "block", padding: "6px 12px", fontSize: 12, marginBottom: 12 }}>
              {collisionsError}
            </p>
          )}

          {collisions && (
            <div
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
                      downgrades' own heading is styled with the error
                      palette so it is marked by more than colour alone
                      (its text already says "Downgrades", not just red). */}
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
                        style={{ fontSize: 11, padding: "3px 0", borderBottom: "1px solid var(--border)" }}
                      >
                        <div style={{ display: "flex", justifyContent: "space-between", gap: 8 }}>
                          <span style={{ wordBreak: "break-all" }}>{report.path}</span>
                          <span
                            className={group.kind === "downgrade" ? undefined : "faint"}
                            style={group.kind === "downgrade" ? { color: "var(--err-text)" } : undefined}
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

          {refusals && refusals.length > 0 && (
            <div
              className="badge badge-err"
              style={{ display: "block", padding: "8px 10px", margin: "0 0 12px", fontSize: 11 }}
            >
              <p style={{ margin: "0 0 6px", fontWeight: 600 }}>{t("osinstall.packages.refused.heading")}</p>
              <ul style={{ margin: 0, paddingLeft: 18 }}>
                {refusals.map((reason, i) => {
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

          {/* One confirmation for the whole set — never one per file (see
              the module doc comment's requirement 3). */}
          <label style={{ display: "flex", gap: 8, alignItems: "center", fontSize: 12, margin: "0 0 10px" }}>
            <input
              type="checkbox"
              checked={confirmed}
              disabled={!collisions}
              onChange={(e) => setConfirmed(e.target.checked)}
            />
            {t("osinstall.packages.confirm", { count: chosen.length })}
          </label>

          {applyError && (
            <div className="badge badge-err" style={{ display: "block", padding: "6px 12px", fontSize: 12, marginBottom: 12 }}>
              {applyError}
            </div>
          )}
          {applyOutcome && (
            <div className="badge badge-ok" style={{ display: "block", padding: "6px 12px", fontSize: 12, marginBottom: 12 }}>
              {t("osinstall.packages.apply.done")}{" "}
              {t("osinstall.packages.apply.outcome", {
                files: applyOutcome.files,
                directories: applyOutcome.directories,
                bytes: formatBytes(applyOutcome.bytes),
              })}
            </div>
          )}

          <div style={{ display: "flex", gap: 8, alignItems: "center" }}>
            <button
              className="btn btn-primary"
              onClick={() => void runApply()}
              disabled={busy || !confirmed || !treeRoot || !packageFolder}
            >
              {t(busy ? "osinstall.packages.apply.running" : "osinstall.packages.apply.run")}
            </button>
            {busy && (
              <span className="faint" style={{ fontSize: 11 }}>
                {pct === null
                  ? t("osinstall.packages.apply.progressStarting")
                  : t("osinstall.packages.apply.progressPercent", {
                      percent: Math.round(pct * 100),
                      done: progress?.done ?? 0,
                      total: progress?.total ?? 0,
                    })}
              </span>
            )}
          </div>
        </div>
      )}
    </section>
  );
}
