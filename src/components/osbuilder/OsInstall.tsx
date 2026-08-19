// Building an AmigaOS distribution tree from the user's own install disks
// (SD-2 · G5). This is the screen for the engine `src-tauri/src/core/osinstall`
// and `src/lib/osinstall.ts` already built and nobody could reach: media
// folder → ROM → components → the file list → confirm → job → report, plus a
// secondary Verify section for checking a tree once it has been copied onto a
// real Amiga volume by the "Prepare Amiga volumes" screen.
//
// Three rules shape it, named directly in the brief:
//
//   - **Every conditional tick states its reason.** A tick ART decided and
//     did not explain is a tick the user cannot argue with. `AMIGAOS_32_COMPONENTS`
//     (`@/lib/osinstall`) carries which components are `required` and which
//     carry a `conditionMajor` (today, only `modules-a1200`'s "below
//     Kickstart V47"), and `conditionalReason` decides, from primitives, one
//     of exactly four reasons for every conditional row — never none, which
//     is the shape a review found this screen could render in its first
//     round (see below).
//   - **Turning a condition-satisfied component off is a confirmation, not a
//     refusal.** It is the user's machine. **This is now the engine's own
//     job, not this screen's.** The first version filtered a returned
//     `InstallPlan` in the browser — safe-looking, because `osinstallApply`
//     takes the exact plan it is handed, but wrong two ways a review found:
//     the filtered plan's `mediaPaths` still promised the excluded
//     component's own media (so `apply()`'s manifest recorded a medium not
//     one byte of which was installed, and a moved disk could fail a build
//     over a component the user turned off), and a `MediaMissing` refusal
//     `plan()` itself raised for the excluded component was never touched
//     at all — `osinstallBlocker` reads `refusals`, so "turning Modules off"
//     stayed a refusal, just a politer one. `core/osinstall/plan.rs`'s
//     `InstallRequest.excluded` fixes both at the source: subtracted inside
//     `resolve_components_on`, before the media-resolution loop, so an
//     excluded component's media is never opened, never recorded, and never
//     a source of a refusal. This screen keeps **two** plans for exactly
//     this reason — see the module doc on `basePlan`/`effectivePlan` below.
//   - **The file list is read-only.** Unlike G11's layout preview, where
//     retargeting a row *is* the feature, every destination here comes from
//     a recipe checked against real media — a hand-moved row would make
//     `distribution.json` describe a release that was never actually built.
//     Components are the only edit; the list itself has no controls.
//
// Remembered between runs, through `@/lib/remembered`'s guards: the media
// folder, the ROM, the destination, and the component selection (`chosen`
// plus `excludedConditional`). Nothing here arms a destructive action the
// way the preload screen's partition picks do — building a distribution
// tree only ever writes a *new* folder and refuses one that already exists
// (`SAFE_CREATE`), so there is nothing of that shape to protect against by
// leaving a choice unremembered.
//
// Every function doing anything other than rendering — the exclusion state
// machine, the reasoning behind a conditional tick, the Verify section's
// two small parsers — lives in `@/lib/osinstall` and is unit-tested there
// (`src/lib/osinstall.test.ts`). A review's own diagnosis of how the first
// Critical shipped: "no test can reach [it], because it lives inside the
// component." This screen is now the thin rendering layer that diagnosis
// asked for.

import { useEffect, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { useTranslation } from "react-i18next";

import { hostParentDir } from "@/lib/hostPath";
import {
  AMIGAOS_32_COMPONENTS,
  componentLabel,
  confirmComponentOff,
  conditionalReason,
  conditionalToggleAction,
  hasRomUnknownRefusal,
  INSTALL_RELEASES,
  isForcedOnByCondition,
  isInstallRelease,
  isVerified,
  onOsInstallResult,
  osinstallApply,
  osinstallBlocker,
  osinstallPlan,
  osinstallScanMedia,
  osinstallVerify,
  parseOptionalSlot,
  parsePartitionIndex,
  pruneStaleExclusions,
  refusalPhrase,
  sanitizeChosen,
  toggleChosen,
  withoutExcluded,
  type ComponentDef,
  type InstallPlan,
  type InstallRelease,
  type InstallRequest,
  type MediaScanResult,
  type OsInstallResult,
  type PlanResult,
  type VerifyReport,
} from "@/lib/osinstall";
import { pistormIdentifyRom, type RomInfo } from "@/lib/pistorm";
import { isTextList, isTextOrNothing } from "@/lib/remembered";
import { useRemembered } from "@/lib/useRemembered";
import { Field } from "@/components/osbuilder/Field";

const GIB = 1024 * 1024 * 1024;

/** A size the way the rest of the OS Builder prints one. */
function size(bytes: number): string {
  if (bytes >= GIB) return `${Math.round((bytes / GIB) * 100) / 100} GB`;
  return `${Math.round((bytes / (1024 * 1024)) * 10) / 10} MB`;
}

/** Plan items, grouped by component, in the order the plan itself already
 *  lists them (recipe order — `plan()` walks `recipe.components` in
 *  declaration order, so this needs no sort of its own). */
function groupByComponent(plan: InstallPlan): { component: string; items: InstallPlan["items"] }[] {
  const order: string[] = [];
  const byComponent = new Map<string, InstallPlan["items"]>();
  for (const item of plan.items) {
    if (!byComponent.has(item.component)) {
      byComponent.set(item.component, []);
      order.push(item.component);
    }
    byComponent.get(item.component)!.push(item);
  }
  return order.map((component) => ({ component, items: byComponent.get(component)! }));
}

export function OsInstall({ droppedMedia = null }: { droppedMedia?: string | null }) {
  const { t } = useTranslation();

  // --- what the user chose, remembered -------------------------------------
  const [mediaFolder, setMediaFolder] = useRemembered<string | null>(
    "osinstall.mediaFolder",
    isTextOrNothing,
    null
  );

  // A disc dropped on the panel names a *file*; the scanner takes the folder
  // that holds it, which is also where its sibling discs and ADFs live. The
  // user dropped it, so this is them setting the value — it goes through the
  // remembered setter and is kept, like every other choice on this screen.
  //
  // Do NOT clear `mediaFolder` when `droppedMedia` is null — arriving at this
  // screen without a drop must leave the remembered folder alone (ART-089).
  useEffect(() => {
    const folder = droppedMedia ? hostParentDir(droppedMedia) : null;
    if (folder) setMediaFolder(folder);
  }, [droppedMedia, setMediaFolder]);
  const [romPath, setRomPath] = useRemembered<string | null>("osinstall.rom", isTextOrNothing, null);
  const [destination, setDestination] = useRemembered<string | null>(
    "osinstall.destination",
    isTextOrNothing,
    null
  );
  /**
   * Which shipped recipe to plan from. `"AmigaOS 3.2"` is the fallback on
   * purpose: it is what this screen has always planned, so nobody's
   * remembered setup changes meaning because a second recipe arrived.
   * `isInstallRelease` turns a hand-edited or since-removed value back into
   * that default instead of putting it on screen.
   */
  const [release, setRelease] = useRemembered<InstallRelease>(
    "osinstall.release",
    isInstallRelease,
    "AmigaOS 3.2"
  );
  const [chosen, setChosen] = useRemembered<string[]>("osinstall.chosen", isTextList, []);
  /**
   * Condition-satisfied components the user has explicitly, with
   * confirmation, turned off — sent to the engine as
   * `InstallRequest.excluded`. Remembered, unlike the preload screen's
   * partition picks: nothing here is destructive by itself (see the module
   * doc comment on the remembered set as a whole).
   */
  const [excludedConditional, setExcludedConditional] = useRemembered<string[]>(
    "osinstall.excludedConditional",
    isTextList,
    []
  );

  // --- what the screen is doing --------------------------------------------
  const [mediaScan, setMediaScan] = useState<MediaScanResult | null>(null);
  const [rom, setRom] = useState<RomInfo | null>(null);
  const [romError, setRomError] = useState(false);
  /**
   * Two plans, requested identically except for `excluded` — both read-only
   * previews (§92), both recomputed live on every change, neither an
   * external-tool cost the way the preload screen's plan is.
   *
   * `basePlan` always asks with `excluded: []`. It exists purely to reason
   * about a conditional component's *true* state — whether its own
   * `Condition` is satisfied at all — because a plan requested *with* a
   * component excluded never carries it in `componentsOn` (the engine skips
   * it entirely), which would make "is this condition-satisfied" and "is
   * this excluded" indistinguishable from the one plan a screen that only
   * asked once would have.
   *
   * `effectivePlan` asks with the real `excludedConditional`. It is what
   * the file list shows, what `osinstallBlocker` reads, and — unmodified —
   * what `osinstallApply` receives. Never the other way around: applying
   * `basePlan` would silently undo every exclusion the user confirmed.
   */
  const [basePlanResult, setBasePlanResult] = useState<PlanResult | null>(null);
  const [effectivePlanResult, setEffectivePlanResult] = useState<PlanResult | null>(null);
  const [planError, setPlanError] = useState<string | null>(null);
  const [confirmed, setConfirmed] = useState(false);
  /** The one component id currently showing the "this will not boot"
   *  confirmation, or `null`. Only one at a time — a second click elsewhere
   *  replaces it, matching how the preload screen's own single-confirmation
   *  shape works. */
  const [pendingExclusion, setPendingExclusion] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [result, setResult] = useState<OsInstallResult | null>(null);

  // --- the secondary Verify section, session-only (see the module doc) ----
  const [verifyDistRoot, setVerifyDistRoot] = useState<string | null>(null);
  const [verifyImage, setVerifyImage] = useState<string | null>(null);
  const [verifySlotText, setVerifySlotText] = useState("");
  const [verifyIndexText, setVerifyIndexText] = useState("1");
  const [verifying, setVerifying] = useState(false);
  const [verifyReport, setVerifyReport] = useState<VerifyReport | null>(null);
  const [verifyError, setVerifyError] = useState<string | null>(null);

  // Re-scan whatever folder was remembered, so one since emptied or moved is
  // noticed rather than shown as still holding what it held last run.
  useEffect(() => {
    if (!mediaFolder) {
      setMediaScan(null);
      return;
    }
    let cancelled = false;
    osinstallScanMedia(mediaFolder)
      .then((r) => {
        if (!cancelled) setMediaScan(r);
      })
      .catch(() => {
        if (!cancelled) setMediaScan(null);
      });
    return () => {
      cancelled = true;
    };
  }, [mediaFolder]);

  // Re-identify whatever ROM was remembered, for the same reason.
  useEffect(() => {
    if (!romPath) {
      setRom(null);
      setRomError(false);
      return;
    }
    let cancelled = false;
    pistormIdentifyRom(romPath)
      .then((r) => {
        if (cancelled) return;
        setRom(r);
        setRomError(false);
      })
      .catch(() => {
        if (cancelled) return;
        setRom(null);
        setRomError(true);
      });
    return () => {
      cancelled = true;
    };
  }, [romPath]);

  // The two plans: read-only (§92's PREVIEW), so both are recomputed live
  // whenever the request changes rather than behind a separate "Preview"
  // button — there is no external tool cost here the way there is on the
  // preload screen, and this is also what lets the component list below
  // explain a conditional tick immediately, not only after a manual preview
  // step.
  useEffect(() => {
    if (!mediaFolder) {
      setBasePlanResult(null);
      setEffectivePlanResult(null);
      setPlanError(null);
      return;
    }
    let cancelled = false;

    const sanitized = sanitizeChosen(chosen);
    if (sanitized.length !== chosen.length) {
      // A stale remembered id (renamed or removed since) actually clears,
      // rather than being filtered again on every read.
      setChosen(sanitized);
    }

    const shared = {
      mediaFolder,
      rom: romPath,
      chosen: sanitized,
      destination: destination ?? "",
      release,
    };
    const baseRequest: InstallRequest = { ...shared, excluded: [] };
    const effectiveRequest: InstallRequest = { ...shared, excluded: excludedConditional };

    Promise.all([osinstallPlan(baseRequest), osinstallPlan(effectiveRequest)])
      .then(([base, effective]) => {
        if (cancelled) return;
        setBasePlanResult(base);
        setEffectivePlanResult(effective);
        setPlanError(null);
        // Confirming an exclusion or the whole install describes the plan
        // that was on screen at the time; once the request changes, both
        // are stale — the same rule the preload screen's own
        // fingerprint/lastPlanned pair enforces, simplified here because
        // the plan is always fresh rather than sometimes stale.
        setConfirmed(false);
        setPendingExclusion(null);
        if (base.outcome === "planned") {
          const pruned = pruneStaleExclusions(base.plan, sanitized, excludedConditional);
          if (pruned.length !== excludedConditional.length) {
            setExcludedConditional(pruned);
          }
        }
      })
      .catch((e) => {
        if (cancelled) return;
        setBasePlanResult(null);
        setEffectivePlanResult(null);
        setPlanError(String(e));
      });
    return () => {
      cancelled = true;
    };
    // `setChosen`/`setExcludedConditional` are stable identities from
    // `useRemembered`.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [mediaFolder, romPath, chosen, destination, excludedConditional, release]);

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    void onOsInstallResult((r) => {
      setResult(r);
      setBusy(false);
      setConfirmed(false);
      setVerifyDistRoot(r.destination);
    }).then((fn) => {
      unlisten = fn;
    });
    return () => unlisten?.();
  }, []);

  const basePlan = basePlanResult?.outcome === "planned" ? basePlanResult.plan : null;
  const effectivePlan = effectivePlanResult?.outcome === "planned" ? effectivePlanResult.plan : null;
  const blocker = osinstallBlocker({ mediaFolder, destination, plan: effectivePlanResult });
  const baseRomUnknown = basePlan ? hasRomUnknownRefusal(basePlan) : false;

  async function chooseMediaFolder() {
    const picked = await open({
      directory: true,
      multiple: false,
      title: t("osinstall.media.chooseTitle"),
    });
    if (typeof picked === "string") setMediaFolder(picked);
  }

  async function chooseRom() {
    const picked = await open({
      multiple: false,
      title: t("osinstall.rom.chooseTitle"),
      filters: [{ name: "Kickstart ROM", extensions: ["rom", "bin"] }],
    });
    if (typeof picked === "string") setRomPath(picked);
  }

  async function chooseDestination() {
    const picked = await open({
      directory: true,
      multiple: false,
      title: t("osinstall.destination.chooseTitle"),
    });
    if (typeof picked === "string") setDestination(picked);
  }

  function toggleConditional(def: ComponentDef) {
    const excluded = excludedConditional.includes(def.id);
    const forcedOn = isForcedOnByCondition(basePlan, chosen, def.id);
    switch (conditionalToggleAction(excluded, forcedOn)) {
      case "undo-exclusion":
        setExcludedConditional(withoutExcluded(excludedConditional, def.id));
        return;
      case "confirm-off":
        // Turning off a condition-satisfied component is a confirmation,
        // not a plain uncheck — see the module doc comment.
        setPendingExclusion(def.id);
        return;
      case "toggle-chosen":
        // Off, and not because of the condition: either an ordinary opt-in
        // (the condition does not currently hold) or undoing that same
        // opt-in.
        setChosen(toggleChosen(chosen, def.id));
    }
  }

  function confirmExclusion(id: string) {
    const next = confirmComponentOff(chosen, excludedConditional, id);
    setChosen(next.chosen);
    setExcludedConditional(next.excluded);
    setPendingExclusion(null);
  }

  async function runInstall() {
    if (!destination || !effectivePlan) return;
    setBusy(true);
    setError(null);
    try {
      await osinstallApply(effectivePlan, destination);
      // `busy` clears on the result event, or here if the job never starts.
    } catch (e) {
      setError(String(e));
      setBusy(false);
    }
  }

  async function chooseVerifyDistRoot() {
    const picked = await open({
      directory: true,
      multiple: false,
      title: t("osinstall.verify.distRoot.chooseTitle"),
    });
    if (typeof picked === "string") setVerifyDistRoot(picked);
  }

  async function chooseVerifyImage() {
    const picked = await open({
      multiple: false,
      title: t("osinstall.verify.image.chooseTitle"),
      filters: [{ name: "Amiga volume image", extensions: ["img", "hdf"] }],
    });
    if (typeof picked === "string") setVerifyImage(picked);
  }

  const verifySlot = parseOptionalSlot(verifySlotText);
  const verifyIndex = parsePartitionIndex(verifyIndexText);
  const verifyReady = !!verifyDistRoot && !!verifyImage && verifySlot.ok && verifyIndex !== null;

  async function runVerify() {
    if (!verifyDistRoot || !verifyImage || !verifySlot.ok || verifyIndex === null) return;
    setVerifying(true);
    setVerifyError(null);
    setVerifyReport(null);
    try {
      setVerifyReport(await osinstallVerify(verifyImage, verifySlot.value, verifyIndex, verifyDistRoot));
    } catch (e) {
      setVerifyError(String(e));
    } finally {
      setVerifying(false);
    }
  }

  return (
    <>
      <section className="card" style={{ marginBottom: 16 }}>
        <h2 style={{ fontSize: 16, marginTop: 0 }}>{t("osinstall.heading")}</h2>
        <p className="muted" style={{ fontSize: 12, margin: "4px 0 12px" }}>
          {t("osinstall.intro")}
        </p>

        <label style={{ display: "flex", flexDirection: "column", gap: 4, marginBottom: 12 }}>
          <span className="muted" style={{ fontSize: 12 }}>
            {t("osinstall.release.label")}
          </span>
          <select
            className="btn"
            style={{ maxWidth: "16em" }}
            value={release}
            onChange={(e) => setRelease(e.target.value as InstallRelease)}
          >
            {INSTALL_RELEASES.map((option) => (
              <option key={option} value={option}>
                {option}
              </option>
            ))}
          </select>
        </label>

        <Field
          label={t("osinstall.media.label")}
          value={mediaFolder}
          empty={t("osinstall.media.none")}
          onChoose={() => void chooseMediaFolder()}
          choose={t("common.browse")}
        />
        {mediaScan?.outcome === "folder-unreadable" && (
          <p className="badge badge-err" style={{ fontSize: 11, margin: "0 0 12px", display: "inline-block" }}>
            {t("osinstall.media.unreadable")}
          </p>
        )}
        {mediaScan?.outcome === "found" && mediaScan.media.length === 0 && (
          <p className="faint" style={{ fontSize: 11, margin: "0 0 12px" }}>
            {t("osinstall.media.empty")}
          </p>
        )}
        {mediaScan?.outcome === "found" && mediaScan.media.length > 0 && (
          <p className="faint" style={{ fontSize: 11, margin: "0 0 12px" }}>
            {t("osinstall.media.found", {
              count: mediaScan.media.length,
              names: mediaScan.media.map((m) => m.volumeName).join(", "),
            })}
          </p>
        )}

        <Field
          label={t("osinstall.rom.label")}
          value={romPath}
          empty={t("osinstall.rom.none")}
          onChoose={() => void chooseRom()}
          choose={t("common.browse")}
        />
        {romError && (
          <p className="badge badge-err" style={{ fontSize: 11, margin: "0 0 12px", display: "inline-block" }}>
            {t("osinstall.rom.unreadable")}
          </p>
        )}
        {rom && (
          <p className="faint" style={{ fontSize: 11, margin: "0 0 12px" }}>
            {t("osinstall.rom.identified", { rom: rom.name })}
          </p>
        )}

        <Field
          label={t("osinstall.destination.label")}
          value={destination}
          empty={t("osinstall.destination.none")}
          onChoose={() => void chooseDestination()}
          choose={t("common.browse")}
          hint={t("osinstall.destination.hint")}
        />
      </section>

      <section className="card" style={{ marginBottom: 16 }}>
        <h2 style={{ fontSize: 16, marginTop: 0 }}>{t("osinstall.components.heading")}</h2>
        <p className="muted" style={{ fontSize: 12, margin: "4px 0 12px" }}>
          {t("osinstall.components.intro")}
        </p>

        <div style={{ display: "flex", flexDirection: "column", gap: 6 }}>
          {AMIGAOS_32_COMPONENTS.map((def) => {
            const excluded = excludedConditional.includes(def.id);
            const checked = def.required
              ? true
              : !def.available
                ? false
                : def.conditionMajor !== null
                  ? (effectivePlan?.componentsOn.includes(def.id) ?? false)
                  : chosen.includes(def.id);
            const disabled = def.required || !def.available;

            function handleChange() {
              if (disabled) return;
              if (def.conditionMajor !== null) {
                toggleConditional(def);
              } else {
                setChosen(toggleChosen(chosen, def.id));
              }
            }

            // Exactly one of four reasons, computed the same way for every
            // conditional row — see `conditionalReason`'s own doc comment
            // for why this can never fall through to nothing.
            //
            // ART-119 (#4): gated on `!def.required && def.available` —
            // dropped in an earlier fix round and re-added here. Unreachable
            // against today's shipped recipe (no component is both
            // conditional and either required or unavailable), but a
            // required or coming-later row already renders its own
            // "required"/"coming later" line above; a future recipe that
            // combined the two should not additionally show a rom-needed or
            // condition-on/off badge that contradicts it.
            const reason =
              def.conditionMajor !== null && !def.required && def.available
                ? conditionalReason(
                    def.conditionMajor,
                    isForcedOnByCondition(basePlan, chosen, def.id),
                    excluded,
                    baseRomUnknown,
                    rom?.name ?? null
                  )
                : null;

            return (
              <div
                key={def.id}
                style={{
                  border: "1px solid var(--border)",
                  borderRadius: 4,
                  padding: "6px 10px",
                  background: checked ? "var(--bg-hover)" : "var(--bg)",
                }}
              >
                <label style={{ display: "flex", gap: 8, alignItems: "center", fontSize: 13 }}>
                  <input type="checkbox" checked={checked} disabled={disabled} onChange={handleChange} />
                  <strong>{def.media}</strong>
                  {def.required && (
                    <span className="badge badge-muted" style={{ fontSize: 10 }}>
                      {t("osinstall.components.required")}
                    </span>
                  )}
                  {!def.available && (
                    <span className="badge badge-muted" style={{ fontSize: 10 }}>
                      {t("common.comingLater")}
                    </span>
                  )}
                </label>

                {def.required && (
                  <p className="faint" style={{ fontSize: 11, margin: "4px 0 0" }}>
                    {t("osinstall.components.reason.required")}
                  </p>
                )}

                {reason?.kind === "rom-needed" && (
                  <p className="faint" style={{ fontSize: 11, margin: "4px 0 0" }}>
                    {t("osinstall.components.reason.romNeeded")}
                  </p>
                )}
                {reason?.kind === "condition-overridden" && (
                  <p
                    className="badge badge-warn"
                    style={{ fontSize: 11, margin: "4px 0 0", display: "inline-block" }}
                  >
                    {t("osinstall.components.reason.conditionOverridden", { major: reason.major })}
                  </p>
                )}
                {reason?.kind === "condition-on" && (
                  <p
                    className="badge badge-warn"
                    style={{ fontSize: 11, margin: "4px 0 0", display: "inline-block" }}
                  >
                    {t("osinstall.components.reason.conditionOn", { rom: reason.rom, major: reason.major })}
                  </p>
                )}
                {reason?.kind === "condition-off" && (
                  <p className="faint" style={{ fontSize: 11, margin: "4px 0 0" }}>
                    {t("osinstall.components.reason.conditionOff", { rom: reason.rom, major: reason.major })}
                  </p>
                )}

                {pendingExclusion === def.id && def.conditionMajor !== null && (
                  <div
                    className="badge badge-err"
                    style={{ display: "block", padding: "8px 10px", margin: "6px 0 0", fontSize: 11 }}
                  >
                    <p style={{ margin: "0 0 8px" }}>
                      {t("osinstall.components.confirmOff.warning", { major: def.conditionMajor })}
                    </p>
                    <div style={{ display: "flex", gap: 8 }}>
                      <button className="btn" onClick={() => confirmExclusion(def.id)}>
                        {t("osinstall.components.confirmOff.confirm")}
                      </button>
                      <button className="btn" onClick={() => setPendingExclusion(null)}>
                        {t("common.cancel")}
                      </button>
                    </div>
                  </div>
                )}
              </div>
            );
          })}
        </div>
      </section>

      {planError && (
        <section className="card" style={{ marginBottom: 16 }}>
          <p className="badge badge-err" style={{ display: "block", padding: "8px 12px", fontSize: 12 }}>
            {planError}
          </p>
        </section>
      )}

      {effectivePlan && effectivePlan.refusals.length > 0 && (
        <section className="card" style={{ marginBottom: 16 }}>
          <h2 style={{ fontSize: 16, marginTop: 0 }}>{t("osinstall.refusals.heading")}</h2>
          <ul className="muted" style={{ fontSize: 12, margin: 0, paddingLeft: 20 }}>
            {effectivePlan.refusals.map((r, i) => {
              const phrase = refusalPhrase(r);
              return (
                <li key={i} style={{ padding: "2px 0" }}>
                  {t(phrase.key, phrase.params)}
                </li>
              );
            })}
          </ul>
        </section>
      )}

      {effectivePlan && effectivePlan.items.length > 0 && (
        <section className="card" style={{ marginBottom: 16 }}>
          <h2 style={{ fontSize: 16, marginTop: 0 }}>{t("osinstall.plan.heading")}</h2>
          <p className="muted" style={{ fontSize: 12, margin: "4px 0 12px" }}>
            {t("osinstall.plan.summary", { items: effectivePlan.items.length, bytes: size(effectivePlan.totalBytes) })}
          </p>
          <div
            style={{
              maxHeight: 360,
              overflowY: "auto",
              border: "1px solid var(--border)",
              borderRadius: 4,
              padding: "6px 10px",
            }}
          >
            {groupByComponent(effectivePlan).map(({ component, items }) => (
              <div key={component} style={{ marginBottom: 8 }}>
                <div className="muted" style={{ fontSize: 11, fontWeight: 600, margin: "6px 0 2px" }}>
                  {componentLabel(component)}
                </div>
                {items.map((item, i) => (
                  <div
                    key={i}
                    style={{
                      display: "flex",
                      justifyContent: "space-between",
                      gap: 8,
                      fontSize: 11,
                      padding: "1px 0",
                    }}
                  >
                    <span style={{ wordBreak: "break-all" }}>
                      {item.to}
                      {item.isDir ? "/" : ""}
                    </span>
                    <span className="faint">{item.isDir ? "" : size(item.bytes)}</span>
                  </div>
                ))}
              </div>
            ))}
          </div>
        </section>
      )}

      <section className="card" style={{ marginBottom: 16 }}>
        <h2 style={{ fontSize: 16, marginTop: 0 }}>{t("osinstall.run.heading")}</h2>

        {error && (
          <div className="badge badge-err" style={{ display: "block", padding: "6px 12px", fontSize: 12, marginBottom: 12 }}>
            {error}
          </div>
        )}

        <label style={{ display: "flex", gap: 8, alignItems: "center", fontSize: 12, marginBottom: 10 }}>
          <input
            type="checkbox"
            checked={confirmed}
            disabled={!!blocker}
            onChange={(e) => setConfirmed(e.target.checked)}
          />
          {t("osinstall.run.confirm")}
        </label>

        <div style={{ display: "flex", gap: 8, alignItems: "center" }}>
          <button className="btn btn-primary" onClick={() => void runInstall()} disabled={busy || !confirmed || !!blocker}>
            {t(busy ? "osinstall.run.running" : "osinstall.run.run")}
          </button>
          {blocker && (
            <span className="faint" style={{ fontSize: 11 }}>
              {t(blocker.key, blocker.params)}
            </span>
          )}
        </div>
      </section>

      {result && (
        <section className="card" style={{ marginBottom: 16 }}>
          <h2 style={{ fontSize: 16, marginTop: 0 }}>{t("osinstall.result.heading")}</h2>
          <p style={{ fontSize: 12, margin: "4px 0 8px" }}>
            {t("osinstall.result.summary", {
              files: result.outcome.files,
              directories: result.outcome.directories,
              bytes: size(result.outcome.bytes),
            })}
          </p>
          <p className="faint" style={{ fontSize: 11, margin: "0 0 8px", wordBreak: "break-all" }}>
            {t("osinstall.result.root", { root: result.destination })}
          </p>
          <p className="muted" style={{ fontSize: 12, margin: 0 }}>
            {t("osinstall.result.nextStep")}
          </p>
        </section>
      )}

      <section className="card" style={{ marginBottom: 16 }}>
        <h2 style={{ fontSize: 16, marginTop: 0 }}>{t("osinstall.verify.heading")}</h2>
        <p className="muted" style={{ fontSize: 12, margin: "4px 0 12px" }}>
          {t("osinstall.verify.intro")}
        </p>

        <Field
          label={t("osinstall.verify.distRoot.label")}
          value={verifyDistRoot}
          empty={t("osinstall.verify.distRoot.none")}
          onChoose={() => void chooseVerifyDistRoot()}
          choose={t("common.browse")}
        />
        <Field
          label={t("osinstall.verify.image.label")}
          value={verifyImage}
          empty={t("osinstall.verify.image.none")}
          onChoose={() => void chooseVerifyImage()}
          choose={t("common.browse")}
        />

        <div style={{ display: "flex", gap: 16, marginBottom: 12, flexWrap: "wrap" }}>
          <label style={{ display: "flex", flexDirection: "column", gap: 4 }}>
            <span className="muted" style={{ fontSize: 12 }}>
              {t("osinstall.verify.slot.label")}
            </span>
            <input
              className="input"
              value={verifySlotText}
              onChange={(e) => setVerifySlotText(e.target.value)}
              style={{ maxWidth: "8em" }}
            />
            <span className="faint" style={{ fontSize: 10 }}>
              {t("osinstall.verify.slot.hint")}
            </span>
          </label>
          <label style={{ display: "flex", flexDirection: "column", gap: 4 }}>
            <span className="muted" style={{ fontSize: 12 }}>
              {t("osinstall.verify.index.label")}
            </span>
            <input
              className="input"
              value={verifyIndexText}
              onChange={(e) => setVerifyIndexText(e.target.value)}
              style={{ maxWidth: "8em" }}
            />
          </label>
        </div>

        {verifyError && (
          <div className="badge badge-err" style={{ display: "block", padding: "6px 12px", fontSize: 12, marginBottom: 12 }}>
            {t("osinstall.verify.error", { message: verifyError })}
          </div>
        )}

        <div style={{ display: "flex", gap: 8, alignItems: "center", marginBottom: 12 }}>
          <button className="btn" onClick={() => void runVerify()} disabled={verifying || !verifyReady}>
            {t(verifying ? "osinstall.verify.running" : "osinstall.verify.run")}
          </button>
          {!verifyReady && (
            <span className="faint" style={{ fontSize: 11 }}>
              {t("osinstall.verify.needsInputs")}
            </span>
          )}
        </div>

        {verifyReport && (
          <>
            {/* isVerified is failed === 0 && notChecked === 0 — never
                failed === 0 alone. "ART did not look" is not "ART found
                nothing wrong" (§89), so a NotChecked count above zero keeps
                this a warning, never a tick. */}
            <p
              className={isVerified(verifyReport) ? "badge badge-ok" : "badge badge-warn"}
              style={{ display: "block", padding: "8px 12px", fontSize: 12, marginBottom: 8 }}
            >
              {t(isVerified(verifyReport) ? "osinstall.verify.verified" : "osinstall.verify.notVerified")}
            </p>
            <p className="muted" style={{ fontSize: 12, margin: "0 0 8px" }}>
              {t("osinstall.verify.summary", {
                passed: verifyReport.passed,
                failed: verifyReport.failed,
                notChecked: verifyReport.notChecked,
              })}
            </p>
            <div
              style={{
                maxHeight: 280,
                overflowY: "auto",
                border: "1px solid var(--border)",
                borderRadius: 4,
                padding: "6px 10px",
              }}
            >
              {verifyReport.files.map((file, i) => (
                <div key={i} style={{ fontSize: 11, padding: "2px 0" }}>
                  <div style={{ display: "flex", justifyContent: "space-between", gap: 8 }}>
                    <span style={{ wordBreak: "break-all" }}>{file.path}</span>
                    {file.state === "pass" && (
                      <span className="badge badge-ok" style={{ fontSize: 10 }}>
                        {t("osinstall.verify.state.pass")}
                      </span>
                    )}
                    {file.state === "fail" && (
                      <span className="badge badge-err" style={{ fontSize: 10 }}>
                        {t("osinstall.verify.state.fail")}
                      </span>
                    )}
                    {file.state === "not-checked" && (
                      <span className="badge badge-warn" style={{ fontSize: 10 }}>
                        {t("osinstall.verify.state.notChecked")}
                      </span>
                    )}
                  </div>
                  {/* Rust-side detail text stays English (ART-060) — the
                      same rule CoreError messages and WhdloadRefusal follow. */}
                  {file.detail && (
                    <div className="faint" style={{ fontSize: 10 }}>
                      {file.detail}
                    </div>
                  )}
                </div>
              ))}
            </div>
          </>
        )}
      </section>
    </>
  );
}
