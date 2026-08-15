// Formatting a card's Amiga volumes and filling them (SD-2 · G3, route E).
//
// **The step after the card exists.** SD-1 writes a card with partitions an
// Amiga can see and nothing inside them; this is what puts a filesystem in
// them and copies content in, so the card arrives ready rather than offering
// to format itself on first boot.
//
// Three rules shape the screen:
//
//   - **Nothing is chosen to start with.** Formatting is `Destructive` and
//     there is no backup step — the volumes this is aimed at are tens of
//     gigabytes and the guard is the preview, not a copy of what was there.
//     So the user picks what gets erased, one partition at a time.
//   - **§92 in full**: read the card → choose → PREVIEW → confirm the count →
//     APPLY as a job → report. The plan on screen is thrown away the moment
//     the request behind it changes, and the confirmation goes with it.
//   - **What ART cannot check, it says.** There is no PFS3 reader here, so
//     once the volume is filled its contents are the tool's word and not
//     ART's. The result panel says exactly that (§89).
//
// The tool itself is not ART's and is not bundled: `hstImagerPath` is a
// setting beside `winuaePath`, and this screen can set it too so somebody who
// arrives here first is not sent to Settings and back.

import { useEffect, useRef, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { useTranslation } from "react-i18next";

import { cardOpen, type CardReport } from "@/lib/card";
import {
  formatCount,
  onPreloadResult,
  picksFor,
  preloadBlocker,
  preloadPlan,
  preloadProbe,
  preloadRun,
  stepPhrase,
  toRequest,
  type FormatterReport,
  type PartitionPick,
  type PreloadPlan,
  type PreloadResult,
} from "@/lib/preload";
import { isTextOrNothing } from "@/lib/remembered";
import { useRemembered } from "@/lib/useRemembered";
import { usePowerMode } from "@/lib/uxmode";
import { useSettingsStore } from "@/stores/settingsStore";
import { Field } from "@/components/osbuilder/Field";

const GIB = 1024 * 1024 * 1024;

/** A size the way the rest of the OS Builder prints one. */
function size(bytes: number): string {
  if (bytes >= GIB) return `${Math.round((bytes / GIB) * 100) / 100} GB`;
  return `${Math.round((bytes / (1024 * 1024)) * 10) / 10} MB`;
}

export function VolumePreload() {
  const { t } = useTranslation();
  const powerMode = usePowerMode();
  const toolPath = useSettingsStore((state) => state.settings.hstImagerPath);
  const updateSettings = useSettingsStore((state) => state.update);

  // --- what the user chose, remembered ------------------------------------
  const [imagePath, setImagePath] = useRemembered<string | null>(
    "preload.image",
    isTextOrNothing,
    null
  );
  const [driver, setDriver] = useRemembered<string | null>(
    "preload.driver",
    isTextOrNothing,
    null
  );

  // --- what the screen is doing -------------------------------------------
  const [report, setReport] = useState<CardReport | null>(null);
  const [cardError, setCardError] = useState<string | null>(null);
  /**
   * Which partitions are to be prepared, and how.
   *
   * **Deliberately not remembered**, unlike the paths above. Every other
   * choice in ART comes back the way the user left it; a screen that came back
   * already armed to erase two partitions would be that rule turned into a
   * hazard. The card is remembered, what to destroy on it is not.
   */
  const [picks, setPicks] = useState<PartitionPick[]>([]);
  const [plan, setPlan] = useState<PreloadPlan | null>(null);
  const [confirmed, setConfirmed] = useState(false);
  const [tool, setTool] = useState<FormatterReport | null>(null);
  const [probing, setProbing] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [result, setResult] = useState<PreloadResult | null>(null);

  // Re-read whatever card was remembered, so one since deleted or unplugged is
  // noticed rather than shown as still there.
  useEffect(() => {
    if (!imagePath) {
      setReport(null);
      setPicks([]);
      setCardError(null);
      return;
    }
    let cancelled = false;
    cardOpen(imagePath)
      .then((read) => {
        if (cancelled) return;
        setReport(read);
        setPicks(picksFor(read));
        setCardError(null);
      })
      .catch((e) => {
        if (cancelled) return;
        setReport(null);
        setPicks([]);
        setCardError(String(e));
      });
    return () => {
      cancelled = true;
    };
  }, [imagePath]);

  const request = toRequest(imagePath ?? "", driver, picks);

  // A plan describes the request that produced it. Change any of the request
  // and the plan on screen stops being true — so it goes, and the confirmation
  // goes with it. Confirming a count and then changing what would be erased is
  // exactly what this stops.
  const fingerprint = JSON.stringify(request);
  const lastPlanned = useRef<string | null>(null);
  useEffect(() => {
    if (lastPlanned.current !== null && lastPlanned.current !== fingerprint) {
      setPlan(null);
      setConfirmed(false);
    }
  }, [fingerprint]);

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    void onPreloadResult((done) => {
      setResult(done);
      setBusy(false);
      // It has been run. A second run needs a second preview.
      setPlan(null);
      setConfirmed(false);
    }).then((fn) => {
      unlisten = fn;
    });
    return () => unlisten?.();
  }, []);

  async function chooseTool() {
    const picked = await open({
      multiple: false,
      title: t("preload.tool.label"),
      filters: [{ name: "hst.imager", extensions: ["exe"] }],
    });
    if (typeof picked === "string") {
      await updateSettings({ hstImagerPath: picked });
      setTool(null);
    }
  }

  async function probe() {
    if (!toolPath?.trim()) return;
    setProbing(true);
    setError(null);
    try {
      setTool(await preloadProbe(toolPath));
    } catch (e) {
      setTool(null);
      setError(String(e));
    } finally {
      setProbing(false);
    }
  }

  async function chooseCard() {
    const picked = await open({
      multiple: false,
      title: t("preload.card.chooseTitle"),
      filters: [{ name: "Card image", extensions: ["img", "hdf"] }],
    });
    if (typeof picked === "string") setImagePath(picked);
  }

  async function chooseDriver() {
    const picked = await open({
      multiple: false,
      title: t("preload.driver.chooseTitle"),
      filters: [{ name: "Filesystem driver", extensions: ["lha", "bin", ""] }],
    });
    if (typeof picked === "string") setDriver(picked);
  }

  async function chooseContent(at: number) {
    const picked = await open({
      directory: true,
      multiple: false,
      title: t("preload.partitions.chooseContentTitle"),
    });
    if (typeof picked === "string") {
      setPicks((current) =>
        current.map((pick, index) => (index === at ? { ...pick, content: picked } : pick))
      );
    }
  }

  function change(at: number, patch: Partial<PartitionPick>) {
    setPicks((current) =>
      current.map((pick, index) => (index === at ? { ...pick, ...patch } : pick))
    );
  }

  async function preview() {
    if (!imagePath || !toolPath) return;
    setBusy(true);
    setError(null);
    setResult(null);
    try {
      const made = await preloadPlan(request, toolPath);
      setPlan(made);
      setConfirmed(false);
      lastPlanned.current = fingerprint;
    } catch (e) {
      setPlan(null);
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function apply() {
    if (!toolPath) return;
    setBusy(true);
    setError(null);
    try {
      await preloadRun(request, toolPath);
      // `busy` is cleared by the result event, or here if the job never
      // starts. A cancelled or failed job is the job bar's to report.
    } catch (e) {
      setError(String(e));
      setBusy(false);
    }
  }

  const blocker = preloadBlocker({ image: imagePath, toolPath, picks, plan });
  const erases = plan ? formatCount(plan) : 0;

  return (
    <>
      <section className="card" style={{ marginBottom: 16 }}>
        <h2 style={{ fontSize: 16, marginTop: 0 }}>{t("preload.heading")}</h2>
        <p className="muted" style={{ fontSize: 12, margin: "4px 0 12px" }}>
          {t("preload.intro")}
        </p>
        <p
          className="badge badge-warn"
          style={{ display: "block", padding: "8px 12px", fontSize: 12, marginBottom: 12 }}
        >
          {t("preload.scope")}
        </p>

        {error && (
          <div
            className="badge badge-err"
            style={{ display: "block", padding: "6px 12px", fontSize: 12, marginBottom: 12 }}
          >
            {error}
          </div>
        )}

        {/* The tool. ART does not ship it, so this is a path like any other —
            and the same setting Settings shows, not a second copy of it. */}
        <Field
          label={t("preload.tool.label")}
          value={toolPath}
          empty={t("preload.tool.none")}
          onChoose={() => void chooseTool()}
          choose={t("common.browse")}
          hint={t("preload.tool.hint")}
        />
        {toolPath && (
          <div style={{ marginBottom: 12 }}>
            <button className="btn" onClick={() => void probe()} disabled={probing}>
              {t(probing ? "preload.tool.probing" : "preload.tool.probe")}
            </button>
            {tool && (
              <>
                <p className="muted" style={{ fontSize: 11, margin: "6px 0 0" }}>
                  {t("preload.tool.is", { version: tool.version.raw })}
                </p>
                {!tool.is_tested_version && (
                  <p
                    className="badge badge-warn"
                    style={{ fontSize: 11, margin: "6px 0 0", display: "inline-block" }}
                  >
                    {t("preload.tool.untested", { tested: tool.tested_version })}
                  </p>
                )}
              </>
            )}
          </div>
        )}

        <Field
          label={t("preload.card.label")}
          value={imagePath}
          empty={t("preload.card.none")}
          onChoose={() => void chooseCard()}
          choose={t("common.browse")}
        />
        {cardError && (
          <p
            className="badge badge-err"
            style={{ fontSize: 11, margin: "0 0 12px", display: "inline-block" }}
          >
            {t("preload.card.unreadable")}
          </p>
        )}
        {report && (
          <p className="faint" style={{ fontSize: 11, margin: "0 0 12px" }}>
            {t("preload.card.read", {
              disks: report.card.areas.length,
              partitions: picks.length,
            })}
          </p>
        )}

        <Field
          label={t("preload.driver.label")}
          value={driver}
          empty={t("preload.driver.none")}
          onChoose={() => void chooseDriver()}
          choose={t("common.browse")}
          hint={t("preload.driver.hint")}
          onClear={driver ? () => setDriver(null) : undefined}
          clear={t("common.clear")}
        />
      </section>

      <section className="card" style={{ marginBottom: 16 }}>
        <h2 style={{ fontSize: 16, marginTop: 0 }}>{t("preload.partitions.heading")}</h2>
        <p className="muted" style={{ fontSize: 12, margin: "4px 0 12px" }}>
          {t("preload.partitions.intro")}
        </p>

        {picks.length === 0 && (
          <p className="faint" style={{ fontSize: 12, margin: 0 }}>
            {t(report ? "preload.card.noPartitions" : "preload.partitions.none")}
          </p>
        )}

        {report?.card.areas.map((area, areaIndex) => (
          <div key={areaIndex} style={{ marginBottom: 12 }}>
            <div className="muted" style={{ fontSize: 12, marginBottom: 6 }}>
              {/* The byte offset is a block-level number, and Beginner mode
                  does not show those (§47, §48). Which disk it is, it does. */}
              {powerMode
                ? t("preload.partitions.disk", {
                    n: areaIndex + 1,
                    offset: size(area.offset_bytes),
                  })
                : t("preload.partitions.disk", { n: areaIndex + 1, offset: "" })}
            </div>
            {area.rdb.partitions.map((partition, partitionIndex) => {
              const at = picks.findIndex(
                (pick) => pick.area === areaIndex + 1 && pick.index === partitionIndex + 1
              );
              const pick = picks[at];
              if (!pick) return null;
              return (
                <div
                  key={partition.drive_name + partitionIndex}
                  style={{
                    border: "1px solid var(--border)",
                    borderRadius: 4,
                    padding: "8px 12px",
                    marginBottom: 8,
                    background: pick.chosen ? "var(--bg-hover)" : "var(--bg)",
                  }}
                >
                  <label
                    style={{ display: "flex", gap: 8, alignItems: "center", fontSize: 13 }}
                  >
                    <input
                      type="checkbox"
                      checked={pick.chosen}
                      onChange={(e) => change(at, { chosen: e.target.checked })}
                    />
                    <strong>{partition.drive_name}</strong>
                    <span className="faint" style={{ fontSize: 11 }}>
                      {t("preload.partitions.sizeAndType", {
                        size: size(partition.size_bytes),
                        dostype: partition.dostype_str,
                      })}
                    </span>
                  </label>

                  {pick.chosen && (
                    <div style={{ marginTop: 8, display: "grid", gap: 8 }}>
                      <label style={{ display: "flex", flexDirection: "column", gap: 4 }}>
                        <span className="muted" style={{ fontSize: 12 }}>
                          {t("preload.partitions.volumeName")}
                        </span>
                        <input
                          className="input"
                          value={pick.volumeName}
                          onChange={(e) => change(at, { volumeName: e.target.value })}
                          style={{ maxWidth: "16em" }}
                        />
                      </label>
                      <div>
                        <div className="muted" style={{ fontSize: 12, marginBottom: 4 }}>
                          {t("preload.partitions.content")}
                        </div>
                        <div
                          style={{
                            display: "flex",
                            gap: 8,
                            alignItems: "center",
                            flexWrap: "wrap",
                          }}
                        >
                          <button className="btn" onClick={() => void chooseContent(at)}>
                            {t("preload.partitions.chooseContent")}
                          </button>
                          <span style={{ fontSize: 12, wordBreak: "break-all" }}>
                            {pick.content ?? t("preload.partitions.noContent")}
                          </span>
                          {pick.content && (
                            <button
                              className="btn"
                              onClick={() => change(at, { content: null })}
                            >
                              {t("preload.partitions.clearContent")}
                            </button>
                          )}
                        </div>
                      </div>
                    </div>
                  )}
                </div>
              );
            })}
          </div>
        ))}

        <div style={{ display: "flex", gap: 8, alignItems: "center", marginTop: 12 }}>
          <button
            className="btn btn-primary"
            onClick={() => void preview()}
            disabled={busy || !imagePath || !toolPath?.trim()}
          >
            {t(busy && !plan ? "preload.preview.running" : "preload.preview.run")}
          </button>
          {blocker && (
            <span className="faint" style={{ fontSize: 11 }}>
              {t(blocker.key, blocker.params)}
            </span>
          )}
        </div>
      </section>

      {plan && (
        <section className="card" style={{ marginBottom: 16 }}>
          <h2 style={{ fontSize: 16, marginTop: 0 }}>{t("preload.plan.heading")}</h2>

          <p
            className="badge badge-err"
            style={{ display: "block", padding: "8px 12px", fontSize: 12, margin: "4px 0 12px" }}
          >
            {t("preload.plan.willErase", { count: erases })}
          </p>

          <ol className="muted" style={{ fontSize: 12, margin: 0, paddingLeft: 20 }}>
            {plan.steps.map((step, index) => {
              const phrase = stepPhrase(step);
              return (
                <li key={index} style={{ padding: "2px 0" }}>
                  {t(phrase.key, phrase.params)}
                </li>
              );
            })}
          </ol>

          <label
            style={{
              display: "flex",
              gap: 8,
              alignItems: "center",
              fontSize: 12,
              margin: "14px 0 10px",
            }}
          >
            <input
              type="checkbox"
              checked={confirmed}
              onChange={(e) => setConfirmed(e.target.checked)}
            />
            {t("preload.confirm", { count: erases })}
          </label>

          <button
            className="btn btn-primary"
            onClick={() => void apply()}
            disabled={busy || !confirmed}
          >
            {t(busy ? "preload.running" : "preload.run")}
          </button>
        </section>
      )}

      {result && (
        <section className="card" style={{ marginBottom: 16 }}>
          <h2 style={{ fontSize: 16, marginTop: 0 }}>{t("preload.result.heading")}</h2>
          <p style={{ fontSize: 12, margin: "4px 0 8px" }}>
            {t("preload.result.formatted", {
              volumes: result.outcome.formatted.join(", "),
            })}
          </p>
          <p className="muted" style={{ fontSize: 12, margin: "0 0 8px" }}>
            {t("preload.result.copied", {
              files: result.outcome.copied.files,
              directories: result.outcome.copied.directories,
              bytes: result.outcome.copied.bytes,
            })}
          </p>
          {result.outcome.tool && (
            <p className="faint" style={{ fontSize: 11, margin: "0 0 8px" }}>
              {t("preload.result.tool", { version: result.outcome.tool.raw })}
            </p>
          )}
          {/* The one thing this screen must not let a green panel imply. */}
          <p
            className="badge badge-warn"
            style={{ display: "block", padding: "6px 12px", fontSize: 11, margin: 0 }}
          >
            {t("preload.result.notVerified")}
          </p>
        </section>
      )}
    </>
  );
}
