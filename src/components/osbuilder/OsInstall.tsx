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
//     below carries which components are `required` and which carry a
//     `conditionMajor` (today, only `modules-a1200`'s "below Kickstart V47"),
//     and the component list's own JSX prints a sentence for every state
//     that was not the user's own click: required, condition-on,
//     condition-off and condition-overridden all say why.
//   - **Turning a condition-satisfied component off is a confirmation, not a
//     refusal.** It is the user's machine. `core/osinstall/plan.rs::resolve_components_on`
//     has no way to turn a satisfied `Condition` off through `chosen` — the
//     OR only ever adds — so this screen does not pretend `chosen` can do
//     it. Instead: `osinstallPlan` runs unmodified, then `withExclusions`
//     strips the excluded component's items, `componentsOn` entry and
//     `userStartup` contribution from the **plan object itself**, client
//     side, before it is shown or applied. That is safe only because
//     `osinstallApply` takes the exact plan it is handed and never
//     recomputes it (`src/lib/osinstall.ts`'s own module note) — the same
//     property `layoutApply` relies on. This is a real, disclosed
//     limitation of the engine as built through Task 12, not a shortcut
//     invented here; see the comment on `AMIGAOS_32_COMPONENTS` for the
//     matching gap on the read side.
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

import { useEffect, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { useTranslation } from "react-i18next";

import {
  isVerified,
  onOsInstallResult,
  osinstallApply,
  osinstallBlocker,
  osinstallPlan,
  osinstallScanMedia,
  osinstallVerify,
  refusalPhrase,
  type InstallPlan,
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

interface ComponentDef {
  id: string;
  /** The volume name inside the image — shown as the row's own label,
   *  unlocalized, the same way the preload screen prints a partition's
   *  `drive_name` untranslated: this is what the Amiga side calls it, not
   *  a sentence ART wrote. */
  media: string;
  required: boolean;
  available: boolean;
  /** `Condition::RomOlderThan { major }`, mirrored — the only condition
   *  shape the recipe carries today. `null` for an unconditional component. */
  conditionMajor: number | null;
  exclusiveGroup: string | null;
}

/**
 * Mirrors `src-tauri/src/core/osinstall/recipes/amigaos-3.2.json`, component
 * by component — id, `required`, `condition` and `exclusive_group` all have
 * to agree with the shipped recipe, because this list is what turns into the
 * checkboxes below.
 *
 * **This is a disclosed limitation, not an oversight.** Tasks 1 through 12
 * built four commands over the recipe — scan, plan, apply, verify — and none
 * of them hands the recipe itself to the frontend, so there is nothing this
 * screen can ask instead of hardcoding a mirror of it. If `amigaos-3.2.json`
 * ever gains, loses or renames a component and this list is not updated in
 * the same commit, the drift is silent on both sides: a component the
 * recipe knows about simply never appears as a checkbox here, and an id
 * that no longer exists in the recipe just never resolves — `osinstallPlan`
 * treats an unrecognised id in `chosen` as nothing to add, not as an error.
 * A fifth command exposing the recipe would close this properly; that is
 * out of this task's scope (three frontend files), so it is named here
 * instead of left implicit.
 */
const AMIGAOS_32_COMPONENTS: ComponentDef[] = [
  {
    id: "workbench-base",
    media: "Workbench3.2",
    required: true,
    available: true,
    conditionMajor: null,
    exclusiveGroup: null,
  },
  { id: "extras", media: "Extras3.2", required: false, available: true, conditionMajor: null, exclusiveGroup: null },
  { id: "locale-base", media: "Locale", required: false, available: true, conditionMajor: null, exclusiveGroup: null },
  { id: "locale-de", media: "Locale-DE", required: false, available: true, conditionMajor: null, exclusiveGroup: null },
  { id: "locale-dk", media: "Locale-DK", required: false, available: true, conditionMajor: null, exclusiveGroup: null },
  { id: "locale-en", media: "Locale-EN", required: false, available: true, conditionMajor: null, exclusiveGroup: null },
  { id: "locale-es", media: "Locale-ES", required: false, available: true, conditionMajor: null, exclusiveGroup: null },
  { id: "locale-fr", media: "Locale-FR", required: false, available: true, conditionMajor: null, exclusiveGroup: null },
  { id: "locale-gr", media: "Locale-GR", required: false, available: true, conditionMajor: null, exclusiveGroup: null },
  { id: "locale-it", media: "Locale-IT", required: false, available: true, conditionMajor: null, exclusiveGroup: null },
  { id: "locale-nl", media: "Locale-NL", required: false, available: true, conditionMajor: null, exclusiveGroup: null },
  { id: "locale-no", media: "Locale-NO", required: false, available: true, conditionMajor: null, exclusiveGroup: null },
  { id: "locale-pl", media: "Locale-PL", required: false, available: true, conditionMajor: null, exclusiveGroup: null },
  { id: "locale-pt", media: "Locale-PT", required: false, available: true, conditionMajor: null, exclusiveGroup: null },
  { id: "locale-ru", media: "Locale-RU", required: false, available: true, conditionMajor: null, exclusiveGroup: null },
  { id: "locale-se", media: "Locale-SE", required: false, available: true, conditionMajor: null, exclusiveGroup: null },
  { id: "locale-tr", media: "Locale-TR", required: false, available: true, conditionMajor: null, exclusiveGroup: null },
  { id: "locale-uk", media: "Locale-UK", required: false, available: true, conditionMajor: null, exclusiveGroup: null },
  {
    id: "modules-a1200",
    media: "ModulesA1200_3.2",
    required: false,
    available: true,
    conditionMajor: 47,
    exclusiveGroup: "modules",
  },
  {
    id: "update-3.2.1",
    media: "Update3.2.1",
    required: false,
    available: false,
    conditionMajor: null,
    exclusiveGroup: null,
  },
  { id: "fonts", media: "Fonts", required: false, available: true, conditionMajor: null, exclusiveGroup: null },
  { id: "classes", media: "Classes3.2", required: false, available: true, conditionMajor: null, exclusiveGroup: null },
  {
    id: "glowicons",
    media: "GlowIcons3.2",
    required: false,
    available: true,
    conditionMajor: null,
    exclusiveGroup: null,
  },
  {
    id: "backdrops",
    media: "Backdrops3.2",
    required: false,
    available: false,
    conditionMajor: null,
    exclusiveGroup: null,
  },
  { id: "diskdoctor", media: "DiskDoctor", required: false, available: true, conditionMajor: null, exclusiveGroup: null },
  { id: "mmulibs", media: "MMULibs", required: false, available: true, conditionMajor: null, exclusiveGroup: null },
  { id: "hdtools", media: "HDSetup3.2", required: false, available: true, conditionMajor: null, exclusiveGroup: null },
  { id: "storage", media: "Storage3.2", required: false, available: true, conditionMajor: null, exclusiveGroup: null },
];

function componentDef(id: string): ComponentDef | undefined {
  return AMIGAOS_32_COMPONENTS.find((c) => c.id === id);
}

function componentLabel(id: string): string {
  return componentDef(id)?.media ?? id;
}

/**
 * Whether `id` is switched on **only** because its own `Condition` is
 * satisfied — never because it is `required` or because the user put it in
 * `chosen`. `resolve_components_on` (Rust) computes `is_on` as
 * `required || chosen.contains(id)`, then ORs in the condition — it can only
 * ever add `true`, never remove one — so if `componentsOn` carries `id` and
 * neither of the first two is true, the condition is the only thing left
 * that could have done it. Used both to render the reason text and to
 * decide, on toggle, whether unchecking needs the "this will not boot"
 * confirmation or is an ordinary, harmless un-choosing.
 */
function isForcedOnByCondition(plan: InstallPlan | null, chosen: string[], id: string): boolean {
  const def = componentDef(id);
  if (!plan || !def || def.required) return false;
  return plan.componentsOn.includes(id) && !chosen.includes(id);
}

/**
 * The plan with every excluded component's contribution removed — see the
 * module doc comment for why this exists and why it is safe to send to
 * `osinstallApply`. Only ever *removes* entries the real `plan()` already
 * produced, so it can create no collision or refusal the backend did not
 * already clear.
 */
function withExclusions(plan: InstallPlan, excluded: string[]): InstallPlan {
  if (excluded.length === 0) return plan;
  const excludedSet = new Set(excluded);
  const items = plan.items.filter((item) => !excludedSet.has(item.component));
  const componentsOn = plan.componentsOn.filter((id) => !excludedSet.has(id));
  const userStartup = plan.userStartup.filter((c) => !excludedSet.has(c.component));
  const totalBytes = items.reduce((sum, item) => sum + item.bytes, 0);
  return { ...plan, items, componentsOn, userStartup, totalBytes };
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

export function OsInstall() {
  const { t } = useTranslation();

  // --- what the user chose, remembered -------------------------------------
  const [mediaFolder, setMediaFolder] = useRemembered<string | null>(
    "osinstall.mediaFolder",
    isTextOrNothing,
    null
  );
  const [romPath, setRomPath] = useRemembered<string | null>("osinstall.rom", isTextOrNothing, null);
  const [destination, setDestination] = useRemembered<string | null>(
    "osinstall.destination",
    isTextOrNothing,
    null
  );
  const [chosen, setChosen] = useRemembered<string[]>("osinstall.chosen", isTextList, []);
  /**
   * Condition-satisfied components the user has explicitly, with
   * confirmation, turned off. See the module doc comment and
   * `isForcedOnByCondition` — this is the only mechanism that can turn one
   * off, since `chosen` cannot. Remembered, unlike the preload screen's
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
  const [rawPlan, setRawPlan] = useState<PlanResult | null>(null);
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

  // The plan: read-only (§92's PREVIEW), so it is recomputed live whenever
  // the request changes rather than behind a separate "Preview" button —
  // there is no external tool cost here the way there is on the preload
  // screen, and this is also what lets the component list below explain a
  // conditional tick immediately, not only after a manual preview step.
  useEffect(() => {
    if (!mediaFolder) {
      setRawPlan(null);
      setPlanError(null);
      return;
    }
    let cancelled = false;
    const request: InstallRequest = {
      mediaFolder,
      rom: romPath,
      chosen: chosen.filter((id) => componentDef(id)?.available !== false),
      destination: destination ?? "",
    };
    osinstallPlan(request)
      .then((planned) => {
        if (cancelled) return;
        setRawPlan(planned);
        setPlanError(null);
        // Confirming an exclusion or the whole install describes the plan
        // that was on screen at the time; once the request changes, both
        // are stale — the same rule the preload screen's own
        // fingerprint/lastPlanned pair enforces, simplified here because
        // the plan is always fresh rather than sometimes stale.
        setConfirmed(false);
        setPendingExclusion(null);
        // Prune a stale exclusion: once the paired Kickstart no longer
        // triggers a component's condition, "turned off despite the
        // condition" has nothing left to override. Leaving it in the
        // remembered set would silently reapply the override the moment a
        // pre-V47 ROM came back — without the user ever confirming *that*
        // pairing — which is exactly the "nothing changes unless the user
        // changes it" rule read backwards.
        if (planned.outcome === "planned") {
          const plan = planned.plan;
          const pruned = excludedConditional.filter((id) => isForcedOnByCondition(plan, chosen, id));
          if (pruned.length !== excludedConditional.length) {
            setExcludedConditional(pruned);
          }
        }
      })
      .catch((e) => {
        if (cancelled) return;
        setRawPlan(null);
        setPlanError(String(e));
      });
    return () => {
      cancelled = true;
    };
    // `setExcludedConditional` is a stable identity from `useRemembered`.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [mediaFolder, romPath, chosen, destination]);

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

  const plannedOk = rawPlan?.outcome === "planned" ? rawPlan.plan : null;
  const effectivePlan = plannedOk ? withExclusions(plannedOk, excludedConditional) : null;
  const effectivePlanResult: PlanResult | null =
    rawPlan?.outcome === "planned" && effectivePlan ? { outcome: "planned", plan: effectivePlan } : rawPlan;
  const blocker = osinstallBlocker({ mediaFolder, destination, plan: effectivePlanResult });
  const romUnknown = plannedOk?.refusals.some((r) => r.refusal === "rom-unknown") ?? false;

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

  /** Every non-conditional toggle: an ordinary opt-in/opt-out through
   *  `chosen`, clearing any other member of the same `exclusiveGroup` on
   *  the way in. Also the opt-in half of a conditional component's own
   *  toggle (see `toggleConditional`) — choosing one whose condition does
   *  not currently hold is exactly this same "add to `chosen`" path. */
  function toggleChosen(id: string, exclusiveGroup: string | null) {
    if (chosen.includes(id)) {
      setChosen(chosen.filter((c) => c !== id));
      return;
    }
    const withoutGroup = exclusiveGroup
      ? chosen.filter((c) => componentDef(c)?.exclusiveGroup !== exclusiveGroup)
      : chosen;
    setChosen([...withoutGroup, id]);
  }

  function toggleConditional(def: ComponentDef) {
    if (excludedConditional.includes(def.id)) {
      // Undo an earlier override — the user is switching it back on.
      setExcludedConditional(excludedConditional.filter((id) => id !== def.id));
      return;
    }
    if (isForcedOnByCondition(plannedOk, chosen, def.id)) {
      // Turning off a condition-satisfied component is a confirmation, not
      // a plain uncheck — see the module doc comment.
      setPendingExclusion(def.id);
      return;
    }
    // Off, and not because of the condition: either an ordinary opt-in
    // (the condition does not currently hold) or undoing that same opt-in.
    // Note: if this component is ever both explicitly chosen *and* its
    // condition later starts holding too (the paired ROM changes), the
    // first uncheck only removes the `chosen` entry — the next plan still
    // shows it on, now correctly attributed to the condition, and a second
    // uncheck reaches the confirmation. It never lies about which one is
    // true at the moment it is shown; it only takes two clicks in that one
    // combination.
    toggleChosen(def.id, def.exclusiveGroup);
  }

  function confirmExclusion(id: string) {
    if (!excludedConditional.includes(id)) {
      setExcludedConditional([...excludedConditional, id]);
    }
    if (chosen.includes(id)) {
      setChosen(chosen.filter((x) => x !== id));
    }
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

  async function runVerify() {
    if (!verifyDistRoot || !verifyImage) return;
    const index = Number.parseInt(verifyIndexText, 10);
    if (!Number.isInteger(index) || index < 1) return;
    const slotText = verifySlotText.trim();
    const slot = slotText === "" ? null : Number.parseInt(slotText, 10);
    setVerifying(true);
    setVerifyError(null);
    setVerifyReport(null);
    try {
      setVerifyReport(await osinstallVerify(verifyImage, slot, index, verifyDistRoot));
    } catch (e) {
      setVerifyError(String(e));
    } finally {
      setVerifying(false);
    }
  }

  const verifyReady =
    !!verifyDistRoot && !!verifyImage && Number.isInteger(Number.parseInt(verifyIndexText, 10));

  return (
    <>
      <section className="card" style={{ marginBottom: 16 }}>
        <h2 style={{ fontSize: 16, marginTop: 0 }}>{t("osinstall.heading")}</h2>
        <p className="muted" style={{ fontSize: 12, margin: "4px 0 12px" }}>
          {t("osinstall.intro")}
        </p>

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
            const forcedOn = isForcedOnByCondition(plannedOk, chosen, def.id);
            const checked = def.required
              ? true
              : !def.available
                ? false
                : def.conditionMajor !== null
                  ? !!plannedOk?.componentsOn.includes(def.id) && !excluded
                  : chosen.includes(def.id);
            const disabled = def.required || !def.available;

            function handleChange() {
              if (disabled) return;
              if (def.conditionMajor !== null) {
                toggleConditional(def);
              } else {
                toggleChosen(def.id, def.exclusiveGroup);
              }
            }

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

                {!def.required && def.available && def.conditionMajor !== null && (
                  <>
                    {romUnknown && (
                      <p className="faint" style={{ fontSize: 11, margin: "4px 0 0" }}>
                        {t("osinstall.components.reason.romNeeded")}
                      </p>
                    )}
                    {!romUnknown && rom && excluded && (
                      <p
                        className="badge badge-warn"
                        style={{ fontSize: 11, margin: "4px 0 0", display: "inline-block" }}
                      >
                        {t("osinstall.components.reason.conditionOverridden", { major: def.conditionMajor })}
                      </p>
                    )}
                    {!romUnknown && rom && !excluded && forcedOn && (
                      <p
                        className="badge badge-warn"
                        style={{ fontSize: 11, margin: "4px 0 0", display: "inline-block" }}
                      >
                        {t("osinstall.components.reason.conditionOn", {
                          rom: rom.name,
                          major: def.conditionMajor,
                        })}
                      </p>
                    )}
                    {!romUnknown && rom && !excluded && !forcedOn && (
                      <p className="faint" style={{ fontSize: 11, margin: "4px 0 0" }}>
                        {t("osinstall.components.reason.conditionOff", {
                          rom: rom.name,
                          major: def.conditionMajor,
                        })}
                      </p>
                    )}
                  </>
                )}

                {pendingExclusion === def.id && (
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

      {plannedOk && plannedOk.refusals.length > 0 && (
        <section className="card" style={{ marginBottom: 16 }}>
          <h2 style={{ fontSize: 16, marginTop: 0 }}>{t("osinstall.refusals.heading")}</h2>
          <ul className="muted" style={{ fontSize: 12, margin: 0, paddingLeft: 20 }}>
            {plannedOk.refusals.map((r, i) => {
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
