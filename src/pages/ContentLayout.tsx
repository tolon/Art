// Laying content out into a staging tree (SD-2 · G11).
//
// **ART cannot tell a demo from a game.** Nothing derivable from the bytes
// separates the two, so `core/layout` proposes only what it can justify and
// this screen's preview is editable — a checkbox per row, and a drawer
// control above the table that retargets every checked row at once through
// `retarget`. That editable table is the feature; a cleverer rule engine is
// not what this screen is trying to be.
//
// **Collisions after a retarget.** `retarget` recomputes only the collisions
// *within* the plan — a destination the staging tree already holds is a fact
// about the disk, and only the engine has looked at the disk. Re-running
// `layoutPlan` here would answer that, but it would also throw away every
// edit the user just made — it recomputes the whole tree from the policy, not
// from what is on screen, which is exactly the "opposite of preload" note in
// `src/lib/layout.ts`. So instead: every retarget is followed by
// `layoutRecheck`, which re-asks the engine for collisions against the plan's
// *current* destinations without walking or reclassifying anything, and its
// answer replaces `plan.collisions` outright. Apply is blocked by whatever
// that answer says and nothing else — there is no separate "stale" state to
// fall out of step with it.
//
// **The staging seam.** This writes to a folder on the PC, never to a card —
// a real PiStorm card is PFS3 and ART cannot write PFS3, so the OS Builder's
// card-preparation screen is what copies this tree onto a volume. The two
// screens are not wired together on purpose: each does one thing, and a
// button that jumps straight from here into a card write is a decision
// neither screen has earned.

import { useEffect, useRef, useState } from "react";
import { useLocation } from "react-router-dom";
import { open } from "@tauri-apps/plugin-dialog";
import { useTranslation } from "react-i18next";

import {
  droppedTotal,
  kindPhrase,
  layoutApply,
  layoutBlocker,
  layoutPlan,
  layoutRecheck,
  onLayoutPlanResult,
  onLayoutResult,
  refusalPhrase,
  retarget,
  type LayoutPlan,
  type LayoutRequest,
  type LayoutResult,
  type Policy,
} from "@/lib/layout";
import { onJobProgress, subscribeSafely } from "@/lib/jobs";
import type { Phrase } from "@/lib/phrase";
import { isTextList, isTextOrNothing } from "@/lib/remembered";
import { useRemembered } from "@/lib/useRemembered";
import { size } from "@/lib/size";
import { Field } from "@/components/osbuilder/Field";

/** The shipped defaults (`core/layout/policy.rs::Policy::default()`), mirrored
 *  here so the request has a policy to send and the retarget dropdown has
 *  drawers to offer. Not exposed as a setting — nothing on this screen lets
 *  the user rename a drawer, only move rows into the ones that exist. */
const DEFAULT_POLICY: Policy = {
  whdload: "unpack",
  games: "Games",
  floppies: "Floppies",
  hard_disks: "HardDisks",
  discs: "CDs",
  unsorted: "Unsorted",
};

/** The sentinel `<option>` value that means "read the free-text box instead". */
const CUSTOM_DRAWER = "__custom__";

function leafName(path: string): string {
  return path.split(/[/\\]/).pop() ?? path;
}

export function ContentLayout() {
  const { t } = useTranslation();
  const location = useLocation();

  // --- what the user chose, remembered ------------------------------------
  const [root, setRoot] = useRemembered<string | null>("layout.root", isTextOrNothing, null);
  const [paths, setPaths] = useRemembered<string[]>("layout.paths", isTextList, []);

  // --- what the screen is doing --------------------------------------------
  const [plan, setPlan] = useState<LayoutPlan | null>(null);
  /**
   * Which rows are checked for the next retarget.
   *
   * **Deliberately not remembered**, the one exception this screen shares
   * with the preload screen's partition picks: a screen that came back
   * already armed to move rows somewhere would turn "nothing changes unless
   * the user changes it" into a hazard rather than a comfort.
   */
  const [checked, setChecked] = useState<Set<number>>(new Set());
  /**
   * True when `plan.collisions` is `retarget`'s in-plan-only list and a
   * `layoutRecheck` to verify it against disk has been tried and failed.
   *
   * Not the old `stale` flag: `stale` blocked on a question nobody had
   * asked, and its only exit destroyed the user's edit. This blocks on a
   * question that **was** asked and failed — previewing again is a genuine
   * remedy, and Apply must not read an unverified list as an all-clear
   * (§89). Cleared by anything that gives `plan.collisions` a real answer
   * again: a successful recheck, or a fresh preview.
   */
  const [collisionsUnknown, setCollisionsUnknown] = useState(false);
  const [drawerChoice, setDrawerChoice] = useState<string>(DEFAULT_POLICY.games);
  const [customDrawer, setCustomDrawer] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [result, setResult] = useState<LayoutResult | null>(null);
  /**
   * The apply job that is running, so a job that ends **without** a result —
   * cancelled, or failed — can stop the screen waiting for one (ART-110).
   *
   * `onLayoutResult` only ever fires on success. The comment that used to sit
   * in `apply()` said a cancelled or failed job was "the job bar's to
   * report", which is true of the *message* and was not true of the `busy`
   * flag: nothing cleared it, so Preview and Apply both stayed disabled until
   * the user navigated away and back — on the one screen you most need to
   * re-run after a failure.
   */
  const pendingApply = useRef<number | null>(null);
  /**
   * The preview job that is running, for the same reason `pendingApply` above
   * exists — and it exists at all because planning stopped being cheap.
   *
   * Comparing a destination's **content** (ART-177) means a plan over a
   * staging tree that already holds its output reads every one of those files
   * in full: 138 898 ms on the owner's own 1 697-item collection, against
   * 797 ms for a first plan. Two and a quarter minutes on the command thread
   * is a frozen window (§54), so `layoutPlan` is a job and the plan arrives
   * on an event.
   */
  const pendingPlan = useRef<number | null>(null);
  /** The request the running preview was started for. */
  const plannedFor = useRef<string | null>(null);

  // What Apply will actually copy: every item that is not already exactly
  // where it is going. Derived rather than stored, so a retarget cannot leave
  // it disagreeing with the list it summarises.
  const newCount = plan ? plan.items.length - plan.alreadyInPlace.length : 0;

  const policy = DEFAULT_POLICY;
  // The five drawer names, in the same order the plan proposes them —
  // deliberately not `Object.values(policy)`, which would also pick up
  // `policy.whdload` (`"unpack"` or `"as-archive"` — a placement, not a
  // drawer) since it is a string field too.
  const drawers = [
    ...new Set([policy.games, policy.floppies, policy.hard_disks, policy.discs, policy.unsorted]),
  ];

  const request: LayoutRequest = { root: root ?? "", paths, policy };

  // A plan describes the request that produced it. Change any of the request
  // and the plan on screen stops being true — so it goes, and the selection
  // goes with it.
  const fingerprint = JSON.stringify(request);
  const lastPlanned = useRef<string | null>(null);
  useEffect(() => {
    if (lastPlanned.current !== null && lastPlanned.current !== fingerprint) {
      setPlan(null);
      setChecked(new Set());
      setCollisionsUnknown(false);
    }
  }, [fingerprint]);

  // A folder that arrived from ART's drop pipeline (ART-108). The workflow
  // catalogue's `dir.organise` navigates here with the dropped path in router
  // state, the same way every other studio is reached from a drop; before it
  // existed nothing dropped could reach this screen at all.
  //
  // Added to the source list rather than acted on: a drop says what to lay
  // out, not where to lay it out, and the staging root is still the user's to
  // choose. The ref keeps StrictMode's double mount from adding it twice.
  const arrivedWith = useRef<string | null>(null);
  useEffect(() => {
    const wanted = (location.state as { path?: string } | null)?.path;
    if (!wanted || arrivedWith.current === wanted) return;
    arrivedWith.current = wanted;
    if (!paths.includes(wanted)) setPaths([...paths, wanted]);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [location.state]);

  // A job that ended without a result: cancelled, or failed (ART-110). Only
  // this stops the screen waiting — the two result events fire on success
  // alone. Both jobs this screen starts are watched here, because a preview
  // that fails halfway through a 1 700-item collection has to release the
  // buttons exactly as a failed apply does.
  useEffect(() => {
    return subscribeSafely(() =>
      onJobProgress((job) => {
        if (job.state.state === "running") return;
        const waiting = job.id === pendingApply.current || job.id === pendingPlan.current;
        if (!waiting) return;
        if (job.id === pendingApply.current) pendingApply.current = null;
        if (job.id === pendingPlan.current) pendingPlan.current = null;
        setBusy(false);
        if (job.state.state === "failed") {
          setError(`${job.state.message} (${job.state.error_code})`);
        }
      })
    );
  }, []);

  // The plan itself, when the preview job finishes.
  useEffect(() => {
    return subscribeSafely(() =>
      onLayoutPlanResult((result) => {
        if (result.job_id !== pendingPlan.current) return;
        pendingPlan.current = null;
        setPlan(result.plan);
        setChecked(new Set());
        setCollisionsUnknown(false);
        lastPlanned.current = plannedFor.current;
        setBusy(false);
      })
    );
  }, []);

  // `subscribeSafely` (ART-165): the bare `.then((fn) => { unlisten = fn })`
  // shape this used to have could both leak the real Tauri listener (an
  // unmount before the promise resolved left nothing to call) and surface
  // an unhandled rejection (no IPC bridge to reach, e.g. under test).
  useEffect(() => {
    return subscribeSafely(() =>
      onLayoutResult((done) => {
        setResult(done);
        pendingApply.current = null;
        setBusy(false);
        // It has been laid out. A second run needs a second preview.
        setPlan(null);
        setChecked(new Set());
        setCollisionsUnknown(false);
      })
    );
  }, []);

  async function chooseRoot() {
    const picked = await open({
      directory: true,
      multiple: false,
      title: t("layout.root.chooseTitle"),
    });
    if (typeof picked === "string") setRoot(picked);
  }

  async function addFiles() {
    const picked = await open({ multiple: true, title: t("layout.sources.addFilesTitle") });
    if (!picked || !Array.isArray(picked)) return;
    setPaths([...new Set([...paths, ...picked])]);
  }

  async function addFolder() {
    const picked = await open({
      directory: true,
      multiple: false,
      title: t("layout.sources.addFolderTitle"),
    });
    if (typeof picked !== "string") return;
    if (paths.includes(picked)) return;
    setPaths([...paths, picked]);
  }

  function removePath(path: string) {
    setPaths(paths.filter((p) => p !== path));
  }

  async function preview() {
    if (!root?.trim() || paths.length === 0) return;
    setBusy(true);
    setError(null);
    setResult(null);
    try {
      // The plan arrives on `onLayoutPlanResult`; `busy` stays set until it
      // does, or until the job-progress listener sees the job end some other
      // way. Deliberately no `setBusy(false)` here — the work has only just
      // started.
      plannedFor.current = fingerprint;
      pendingPlan.current = await layoutPlan(request);
    } catch (e) {
      pendingPlan.current = null;
      setPlan(null);
      setError(String(e));
      setBusy(false);
    }
  }

  async function apply() {
    if (!plan) return;
    setBusy(true);
    setError(null);
    try {
      // The id is what lets a job that ends *without* a result stop the
      // waiting: `onLayoutResult` fires on success only, so a cancelled or
      // failed run reaches the screen through `onJobProgress` above (ART-110).
      pendingApply.current = await layoutApply(plan);
    } catch (e) {
      pendingApply.current = null;
      setError(String(e));
      setBusy(false);
    }
  }

  function toggleRow(index: number, on: boolean) {
    setChecked((current) => {
      const next = new Set(current);
      if (on) next.add(index);
      else next.delete(index);
      return next;
    });
  }

  function toggleAll(on: boolean) {
    if (!plan) return;
    setChecked(on ? new Set(plan.items.map((_, index) => index)) : new Set());
  }

  /** The typed drawer, with the slashes a user might type around it (`Demos/`,
   *  `/Demos`) stripped. `retarget` builds `${drawer}/${leaf}` verbatim, so a
   *  trailing slash would show as `Demos//Turrican` on screen until
   *  `safe_join` normalises it at Apply time — display-only, but it looks
   *  like a bug. This is a screen-input concern, not `retarget`'s: that
   *  function is shipped and tested as it is. */
  function normalizedCustomDrawer(): string {
    return customDrawer.trim().replace(/^\/+/, "").replace(/\/+$/, "");
  }

  async function moveChecked() {
    if (!plan || checked.size === 0) return;
    const target = drawerChoice === CUSTOM_DRAWER ? normalizedCustomDrawer() : drawerChoice;
    if (!target) return;
    const retargeted = retarget(plan, [...checked], target);
    setPlan(retargeted);
    setChecked(new Set());
    // `retarget` only knows about collisions within the plan; whether either
    // new destination already exists on disk is a fact only the engine has
    // looked at, so it is re-asked here rather than left for a "stale" flag
    // to gate Apply on. See `retarget`'s doc comment in `src/lib/layout.ts`.
    setBusy(true);
    setError(null);
    try {
      const rechecked = await layoutRecheck(retargeted);
      // Compared by identity against `retargeted`, not just "is there a
      // plan": if another retarget landed while this recheck was in flight,
      // `plan` has already moved on to a different object, and this answer
      // was computed for a plan that is no longer on screen — applying it
      // now would splice a response into a plan it does not describe.
      // Both halves land together, because the engine computed them from one
      // walk: a destination that is already exactly right is not a collision,
      // and splicing only one of the two back would leave the screen holding
      // a pair that disagree (ART-177).
      setPlan((current) =>
        current === retargeted
          ? {
              ...current,
              collisions: rechecked.collisions,
              alreadyInPlace: rechecked.already_in_place,
            }
          : current
      );
      setCollisionsUnknown(false);
    } catch (e) {
      setError(String(e));
      // The plan on screen still carries `retarget`'s in-plan-only
      // collisions — real, but blind to the disk. Saying nothing here would
      // let a stale, unverified list read as an all-clear to `layoutBlocker`,
      // which is exactly the false all-clear §89 forbids. Block until a
      // preview or a later recheck actually answers the question.
      setCollisionsUnknown(true);
    } finally {
      setBusy(false);
    }
  }

  const blocker: Phrase | null =
    layoutBlocker({ root, paths, plan }) ??
    (collisionsUnknown ? { key: "layout.blocked.couldNotRecheck" } : null);

  // `busy` as well as the selection/drawer checks: `moveChecked` is async
  // (it awaits `layoutRecheck`), so without this a second click while the
  // first is still in flight could fire an overlapping recheck.
  const moveDisabled =
    busy || checked.size === 0 || (drawerChoice === CUSTOM_DRAWER && !normalizedCustomDrawer());
  // ART-100: a disabled control says why, and the Apply button four lines
  // down already does — this covers the Move button's own two reasons. While
  // busy the button is disabled for a third reason that needs no title of
  // its own — the same convention the Preview and Apply buttons already
  // follow, which rely on their label changing rather than a tooltip.
  const moveTitle = busy
    ? undefined
    : !moveDisabled
      ? undefined
      : checked.size === 0
        ? t("layout.retarget.blockedNoSelection")
        : t("layout.retarget.blockedNoDrawerName");

  return (
    <div>
      <h1 style={{ fontSize: 20 }}>{t("nav.layout")}</h1>
      <p className="muted" style={{ fontSize: 12, margin: "4px 0 16px" }}>
        {t("layout.intro")}
      </p>

      <section className="card" style={{ marginBottom: 16 }}>
        <h2 style={{ fontSize: 16, marginTop: 0 }}>{t("layout.heading")}</h2>
        <p
          className="badge badge-warn"
          style={{ display: "block", padding: "8px 12px", fontSize: 12, marginBottom: 12 }}
        >
          {t("layout.scope")}
        </p>

        {error && (
          <div
            className="badge badge-err"
            style={{ display: "block", padding: "6px 12px", fontSize: 12, marginBottom: 12 }}
          >
            {error}
          </div>
        )}

        <Field
          label={t("layout.root.label")}
          value={root}
          empty={t("layout.root.none")}
          onChoose={() => void chooseRoot()}
          choose={t("common.browse")}
        />
      </section>

      <section className="card" style={{ marginBottom: 16 }}>
        <h2 style={{ fontSize: 16, marginTop: 0 }}>{t("layout.sources.heading")}</h2>
        <p className="muted" style={{ fontSize: 12, margin: "4px 0 12px" }}>
          {t("layout.sources.intro")}
        </p>

        <div style={{ display: "flex", gap: 8, marginBottom: 12 }}>
          <button className="btn" onClick={() => void addFiles()}>
            {t("layout.sources.addFiles")}
          </button>
          <button className="btn" onClick={() => void addFolder()}>
            {t("layout.sources.addFolder")}
          </button>
        </div>

        {paths.length === 0 ? (
          <p className="faint" style={{ fontSize: 12, margin: 0 }}>
            {t("layout.sources.none")}
          </p>
        ) : (
          <ul style={{ margin: 0, paddingLeft: 0, listStyle: "none", fontSize: 12 }}>
            {paths.map((path) => (
              <li
                key={path}
                style={{
                  display: "flex",
                  gap: 8,
                  alignItems: "center",
                  padding: "3px 0",
                }}
              >
                <span style={{ wordBreak: "break-all" }}>{path}</span>
                <button className="btn" onClick={() => removePath(path)}>
                  {t("layout.sources.remove")}
                </button>
              </li>
            ))}
          </ul>
        )}

        <div style={{ display: "flex", gap: 8, alignItems: "center", marginTop: 12 }}>
          <button
            className="btn btn-primary"
            onClick={() => void preview()}
            disabled={busy || !root?.trim() || paths.length === 0}
          >
            {t(busy && !plan ? "layout.preview.running" : "layout.preview.run")}
          </button>
        </div>
      </section>

      {plan && (
        <section className="card" style={{ marginBottom: 16 }}>
          <h2 style={{ fontSize: 16, marginTop: 0 }}>{t("layout.plan.heading")}</h2>
          <p style={{ fontSize: 12, margin: "0 0 12px" }}>
            {t("layout.plan.total", { bytes: size(plan.bytes) })}
          </p>
          {/* ART-177. Three numbers, and the third is a promise rather than a
              statistic: ART never overwrites, so "overwrites 0" is what that
              guarantee looks like on screen. "Already in place" is what makes
              re-running a stopped run finish it — the count says so plainly,
              so "nothing happened" and "it was already done" never read the
              same. */}
          <p style={{ fontSize: 12, margin: "0 0 12px" }}>
            <strong>{t("layout.plan.counts.new", { count: newCount })}</strong>
            {" · "}
            {t("layout.plan.counts.alreadyInPlace", { count: plan.alreadyInPlace.length })}
            {" · "}
            {t("layout.plan.counts.overwrites", { count: 0 })}
          </p>

          {plan.items.length > 0 && (
            <>
              <div
                style={{
                  display: "flex",
                  gap: 8,
                  alignItems: "center",
                  flexWrap: "wrap",
                  marginBottom: 10,
                  padding: "8px 12px",
                  border: "1px solid var(--border)",
                  borderRadius: 4,
                }}
              >
                <span className="muted" style={{ fontSize: 12 }}>
                  {t("layout.retarget.label")}
                </span>
                <select
                  className="btn"
                  value={drawerChoice}
                  onChange={(e) => setDrawerChoice(e.target.value)}
                >
                  {drawers.map((drawer) => (
                    <option key={drawer} value={drawer}>
                      {drawer}
                    </option>
                  ))}
                  <option value={CUSTOM_DRAWER}>{t("layout.retarget.custom")}</option>
                </select>
                {drawerChoice === CUSTOM_DRAWER && (
                  <input
                    className="input"
                    value={customDrawer}
                    onChange={(e) => setCustomDrawer(e.target.value)}
                    placeholder={t("layout.retarget.customPlaceholder")}
                    style={{ maxWidth: "14em" }}
                  />
                )}
                <button
                  className="btn"
                  onClick={() => void moveChecked()}
                  disabled={moveDisabled}
                  title={moveTitle}
                >
                  {t("layout.retarget.apply")}
                </button>
                <span className="faint" style={{ fontSize: 11 }}>
                  {t("layout.retarget.hint")}
                </span>
              </div>

              <div style={{ overflowX: "auto" }}>
                <table style={{ fontSize: 12, borderCollapse: "collapse", width: "100%" }}>
                  <thead>
                    <tr>
                      <th style={{ textAlign: "left", padding: "2px 8px 2px 0" }}>
                        <input
                          type="checkbox"
                          aria-label={t("layout.table.selectAll")}
                          checked={checked.size === plan.items.length}
                          onChange={(e) => toggleAll(e.target.checked)}
                        />
                      </th>
                      <th className="muted" style={{ textAlign: "left", padding: "2px 8px" }}>
                        {t("layout.table.kind")}
                      </th>
                      <th className="muted" style={{ textAlign: "left", padding: "2px 8px" }}>
                        {t("layout.table.source")}
                      </th>
                      <th className="muted" style={{ textAlign: "left", padding: "2px 8px" }}>
                        {t("layout.table.destination")}
                      </th>
                      <th className="muted" style={{ textAlign: "right", padding: "2px 0" }}>
                        {t("layout.table.size")}
                      </th>
                    </tr>
                  </thead>
                  <tbody>
                    {plan.items.map((item, index) => {
                      const kind = kindPhrase(item.kind);
                      return (
                        <tr key={`${item.source}-${index}`}>
                          <td style={{ padding: "2px 8px 2px 0" }}>
                            <input
                              type="checkbox"
                              checked={checked.has(index)}
                              onChange={(e) => toggleRow(index, e.target.checked)}
                            />
                          </td>
                          <td style={{ padding: "2px 8px" }}>{t(kind.key, kind.params)}</td>
                          <td style={{ padding: "2px 8px", wordBreak: "break-all" }}>
                            {leafName(item.source)}
                          </td>
                          <td style={{ padding: "2px 8px", wordBreak: "break-all" }}>
                            {item.destination}
                          </td>
                          <td style={{ padding: "2px 0", textAlign: "right" }}>
                            {size(item.bytes)}
                          </td>
                        </tr>
                      );
                    })}
                  </tbody>
                </table>
              </div>
            </>
          )}

          {plan.items.length === 0 && (
            <p className="faint" style={{ fontSize: 12, margin: 0 }}>
              {t("layout.table.none")}
            </p>
          )}

          {plan.collisions.length > 0 && (
            <div style={{ marginTop: 14 }}>
              <h3 style={{ fontSize: 13, margin: "0 0 6px" }}>{t("layout.collisions.heading")}</h3>
              <p className="muted" style={{ fontSize: 12, margin: "0 0 8px" }}>
                {t("layout.collisions.intro")}
              </p>
              <ul style={{ margin: 0, paddingLeft: 18, fontSize: 12 }}>
                {plan.collisions.map((collision) => (
                  <li key={collision.destination} style={{ color: "var(--err, #ff5252)" }}>
                    {t("layout.collisions.entry", {
                      destination: collision.destination,
                      sources: collision.sources.map(leafName).join(", "),
                    })}
                  </li>
                ))}
              </ul>
            </div>
          )}

          {plan.refused.length > 0 && (
            <div style={{ marginTop: 14 }}>
              <h3 style={{ fontSize: 13, margin: "0 0 6px" }}>{t("layout.refusals.heading")}</h3>
              <p className="muted" style={{ fontSize: 12, margin: "0 0 8px" }}>
                {t("layout.refusals.intro")}
              </p>
              <ul style={{ margin: 0, paddingLeft: 18, fontSize: 12 }}>
                {plan.refused.map((refusal) => {
                  const reason = refusalPhrase(refusal.reason);
                  return (
                    <li key={refusal.source} className="faint">
                      <code>{leafName(refusal.source)}</code> — {t(reason.key, reason.params)}
                    </li>
                  );
                })}
              </ul>
            </div>
          )}

          {/* **ART-107.** Two ways a scan could quietly not describe what the
              user dropped. Neither blocks apply — a plan that is short in one
              corner is still worth applying — but neither is silent any more,
              which is the whole of the issue: the plan used to come back
              missing files with nothing on screen admitting it. */}
          {droppedTotal(plan.tooDeep) > 0 && (
            <div style={{ marginTop: 14 }}>
              <h3 style={{ fontSize: 13, margin: "0 0 6px" }}>{t("layout.tooDeep.heading")}</h3>
              <p className="muted" style={{ fontSize: 12, margin: "0 0 8px" }}>
                {t("layout.tooDeep.intro", { count: droppedTotal(plan.tooDeep) })}
              </p>
              <ul style={{ margin: 0, paddingLeft: 18, fontSize: 12 }}>
                {plan.tooDeep.paths.map((path) => (
                  <li key={path} className="faint">
                    <code>{path}</code>
                  </li>
                ))}
                {plan.tooDeep.more > 0 && (
                  <li className="faint">{t("layout.tooDeep.more", { count: plan.tooDeep.more })}</li>
                )}
              </ul>
            </div>
          )}

          {droppedTotal(plan.duplicates) > 0 && (
            <div style={{ marginTop: 14 }}>
              <h3 style={{ fontSize: 13, margin: "0 0 6px" }}>
                {t("layout.duplicates.heading")}
              </h3>
              <p className="muted" style={{ fontSize: 12, margin: "0 0 8px" }}>
                {t("layout.duplicates.intro", { count: droppedTotal(plan.duplicates) })}
              </p>
              <ul style={{ margin: 0, paddingLeft: 18, fontSize: 12 }}>
                {plan.duplicates.paths.map((path) => (
                  <li key={path} className="faint">
                    <code>{leafName(path)}</code>
                  </li>
                ))}
                {plan.duplicates.more > 0 && (
                  <li className="faint">
                    {t("layout.duplicates.more", { count: plan.duplicates.more })}
                  </li>
                )}
              </ul>
            </div>
          )}

          <div style={{ display: "flex", gap: 8, alignItems: "center", marginTop: 14 }}>
            <button
              className="btn btn-primary"
              onClick={() => void apply()}
              disabled={busy || blocker !== null}
              title={blocker ? t(blocker.key, blocker.params) : undefined}
            >
              {t(busy ? "layout.apply.running" : "layout.apply.run")}
            </button>
            {blocker && (
              <span className="faint" style={{ fontSize: 11 }}>
                {t(blocker.key, blocker.params)}
              </span>
            )}
          </div>
        </section>
      )}

      {result && (
        <section className="card" style={{ marginBottom: 16 }}>
          <h2 style={{ fontSize: 16, marginTop: 0 }}>{t("layout.result.heading")}</h2>
          <p style={{ fontSize: 12, margin: "4px 0 8px" }}>
            {t("layout.result.placed", {
              count: result.outcome.placed,
              bytes: size(result.outcome.bytes),
            })}
            {result.outcome.skipped > 0 && (
              <> {t("layout.result.skipped", { count: result.outcome.skipped })}</>
            )}
          </p>
          <p className="muted" style={{ fontSize: 12, margin: "0 0 8px", wordBreak: "break-all" }}>
            {t("layout.result.root", { root: result.root })}
          </p>
          <p className="faint" style={{ fontSize: 11, margin: 0 }}>
            {t("layout.result.nextStep")}
          </p>
        </section>
      )}
    </div>
  );
}
