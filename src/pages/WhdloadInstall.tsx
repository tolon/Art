// One-click WHDLoad install (spec §82).
//
//     Game.lha → DROP → WHDLoad detected → Install to HDF → Backup → Apply → Verify
//
// The screen is that line, top to bottom, and nothing else. Every step shows
// what it found before the next one is offered, so the "one click" is the last
// thing on the page rather than the first.
//
// It is not a wizard with pages. Everything is visible at once and the button
// at the bottom is enabled or it is not — a user who wants to check the
// partition they picked should not have to go back a step to see it.

import { useCallback, useEffect, useRef, useState } from "react";
import { useLocation } from "react-router-dom";
import { useTranslation } from "react-i18next";
import { open } from "@tauri-apps/plugin-dialog";

import { onJobProgress } from "@/lib/jobs";
import { formatBytes } from "@/lib/panel";
import { Refusal } from "@/components/Refusal";
import { usePowerMode } from "@/lib/uxmode";
import {
  isMountable,
  volumeScan,
  type ImageVolumes,
  type VolumeEntry,
} from "@/lib/volume";
import {
  describeOutcome,
  describeVerdict,
  hasPack,
  onWhdloadResult,
  whdloadInstall,
  whdloadPlan,
  type WhdloadOutcome,
  type WhdloadPlan,
} from "@/lib/whdload";
import { useOpenObject } from "@/stores/openObjectStore";
import { errorText } from "@/lib/errorText";

export function WhdloadInstall() {
  const { t } = useTranslation();
  const powerMode = usePowerMode();
  const location = useLocation();

  // Both halves of the job outlive this screen (ART-085), for the length of the
  // run: going to look something up in ADF Studio and coming back must not
  // throw away a package and a target the user has already chosen.
  const [archive, setArchive] = useOpenObject("whdload-archive");
  const [image, setImage] = useOpenObject("whdload-image");
  const [volumes, setVolumes] = useState<ImageVolumes | null>(null);
  const [volumeIndex, setVolumeIndex] = useState<number | null>(null);

  const [plan, setPlan] = useState<WhdloadPlan | null>(null);
  const [outcome, setOutcome] = useState<WhdloadOutcome | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const pending = useRef<number | null>(null);

  // The archive can arrive from the drop panel, which is the whole point of
  // §82: dropping a package is what starts this.
  const arrivedWith = useRef<string | null>(null);
  useEffect(() => {
    const state = location.state as { path?: string } | null;
    const wanted = state?.path;
    if (!wanted || arrivedWith.current === wanted) return;
    arrivedWith.current = wanted;
    setArchive(wanted);
  }, [location.state]);

  // Coming back to a job already set up: the archive is a path and needs
  // nothing, but the image's partitions live only in this component and have to
  // be read again (ART-085). `volumes === null` is what "not loaded here" looks
  // like on a fresh mount, whatever the store remembers.
  useEffect(() => {
    if (image && volumes === null) void loadImage(image);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // One listener for the install job's result (§54).
  useEffect(() => {
    const unlisten = onWhdloadResult((result) => {
      if (result.job_id !== pending.current) return;
      pending.current = null;
      setBusy(null);
      setOutcome(result.outcome);
      setPlan(null);
    });
    return () => {
      void unlisten.then((off) => off());
    };
  }, []);

  // A job that fails emits no result, so its terminal state is what says
  // "stop waiting" — and it carries the error id worth showing (§68).
  useEffect(() => {
    const unlisten = onJobProgress((job) => {
      if (job.state.state === "running" || job.id !== pending.current) return;
      pending.current = null;
      setBusy(null);
      if (job.state.state === "failed") {
        setError(`${job.state.message} (${job.state.error_code})`);
      }
    });
    return () => {
      void unlisten.then((off) => off());
    };
  }, []);

  const refreshPlan = useCallback(async () => {
    if (!archive || !image || volumeIndex === null) {
      setPlan(null);
      return;
    }
    setBusy(t("whdload.busy.planning"));
    setError(null);
    try {
      setPlan(await whdloadPlan(archive, image, volumeIndex));
    } catch (e) {
      setPlan(null);
      setError(errorText(t, e));
    } finally {
      setBusy(null);
    }
  }, [archive, image, volumeIndex]);

  useEffect(() => {
    void refreshPlan();
  }, [refreshPlan]);

  async function chooseArchive() {
    const picked = await open({
      multiple: false,
      filters: [{ name: t("whdload.dialog.archiveFilter"), extensions: ["lha", "lzh"] }],
    });
    if (typeof picked !== "string") return;
    setArchive(picked);
    setOutcome(null);
  }

  async function chooseImage() {
    const picked = await open({
      multiple: false,
      filters: [
        { name: t("whdload.dialog.imageFilter"), extensions: ["hdf", "hda", "img", "adf"] },
      ],
    });
    if (typeof picked !== "string") return;
    await loadImage(picked);
  }

  /**
   * Read an image's partitions and pick one if there is nothing to pick.
   *
   * Separate from `chooseImage` because it happens twice: once when the user
   * picks the file, and again when they come back to this screen and the image
   * is still attached (ART-085). `volumes` is what the screen holds and the
   * store does not, so returning here re-reads it rather than restoring an
   * image with no partitions under it.
   */
  async function loadImage(picked: string) {
    setError(null);
    setOutcome(null);
    setImage(picked);
    setVolumeIndex(null);
    setBusy(t("whdload.busy.readingDisk"));
    try {
      const found = await volumeScan(picked);
      setVolumes(found);

      const usable = found.volumes
        .map((volume, index) => ({ volume, index }))
        .filter(({ volume }) => isMountable(volume));

      // One usable partition is not a choice, so do not make the user make it.
      if (usable.length === 1) setVolumeIndex(usable[0].index);
      else if (usable.length === 0) {
        setError(
          found.volumes.length === 0
            ? t("whdload.error.noVolumes")
            : t("whdload.error.noWritablePartition")
        );
      }
    } catch (e) {
      setVolumes(null);
      setError(errorText(t, e));
    } finally {
      setBusy(null);
    }
  }

  async function install() {
    if (!archive || !image || volumeIndex === null) return;
    setError(null);
    setBusy(t("whdload.busy.installing"));
    try {
      pending.current = await whdloadInstall(archive, image, volumeIndex);
    } catch (e) {
      setError(errorText(t, e));
      setBusy(null);
      pending.current = null;
    }
  }

  const ready = plan !== null && plan.refusal === null && busy === null;

  return (
    <div>
      <h1 style={{ fontSize: 20 }}>{t("nav.whdload")}</h1>
      <p className="muted" style={{ marginTop: 4 }}>
        {t("whdload.intro")}
      </p>

      {error && (
        <div className="badge badge-err" style={{ display: "block", margin: "10px 0" }}>
          {error}
        </div>
      )}
      {busy && (
        <div className="muted" style={{ fontSize: 12, margin: "8px 0" }}>
          {t("whdload.busyNote", { busy })}
        </div>
      )}

      {/* ---- 1. the package ---- */}
      <section className="card" style={{ marginBottom: 10 }}>
        <div style={{ display: "flex", gap: 8, alignItems: "center", flexWrap: "wrap" }}>
          <strong style={{ fontSize: 14 }}>{t("whdload.step.package")}</strong>
          <button className="btn" style={{ fontSize: 12 }} onClick={() => void chooseArchive()}>
            {t("whdload.step.chooseArchive")}
          </button>
        </div>
        {archive && (
          <div className="faint" style={{ fontSize: 11, marginTop: 4, wordBreak: "break-all" }}>
            {archive}
          </div>
        )}

        {plan && <Detection plan={plan} powerMode={powerMode} />}
      </section>

      {/* ---- 2. the disk ---- */}
      <section className="card" style={{ marginBottom: 10 }}>
        <div style={{ display: "flex", gap: 8, alignItems: "center", flexWrap: "wrap" }}>
          <strong style={{ fontSize: 14 }}>{t("whdload.step.disk")}</strong>
          <button
            className="btn"
            style={{ fontSize: 12 }}
            onClick={() => void chooseImage()}
            disabled={!archive}
            title={archive ? undefined : t("whdload.step.chooseArchiveFirst")}
          >
            {t("whdload.step.chooseImage")}
          </button>
        </div>
        {image && (
          <div className="faint" style={{ fontSize: 11, marginTop: 4, wordBreak: "break-all" }}>
            {image}
          </div>
        )}

        {volumes && (
          <PartitionChoice
            volumes={volumes}
            chosen={volumeIndex}
            onChoose={setVolumeIndex}
          />
        )}
      </section>

      {/* ---- 3. what will happen ---- */}
      {plan && (
        <section className="card" style={{ marginBottom: 10 }}>
          <strong style={{ fontSize: 14 }}>{t("whdload.step.whatArtWillDo")}</strong>
          <WhatHappens plan={plan} powerMode={powerMode} />
        </section>
      )}

      <button
        className="btn btn-primary"
        onClick={() => void install()}
        disabled={!ready}
        title={
          ready
            ? t("whdload.install.titleReady", { drawer: plan?.drawer })
            : plan?.refusal?.reason ?? t("whdload.install.chooseFirst")
        }
        style={{ fontSize: 14, padding: "8px 18px" }}
      >
        {/* `||`, not `??`: a refused plan carries an *empty* volume name, not a
            missing one, and "Install to " with nothing after it reads as a
            half-rendered screen. */}
        {t("whdload.install.button", {
          volume: plan?.volume_name || t("whdload.install.theDisk"),
        })}
      </button>

      {outcome && <Report outcome={outcome} powerMode={powerMode} />}
    </div>
  );
}

/**
 * What ART found, with its confidence (§14, §34).
 *
 * A guess is never shown at the same strength as a fact, so the confidence is
 * part of the sentence rather than a colour.
 */
function Detection({ plan, powerMode }: { plan: WhdloadPlan; powerMode: boolean }) {
  const { t } = useTranslation();
  const { verdict, layout } = plan;
  const confident = verdict.confidence === "HIGH" || verdict.confidence === "MEDIUM";
  // No pack means no name to print and no icon to be missing. Rendering them
  // anyway shows an empty name badged "no icon — it will not show up on
  // Workbench", complaining about the icon of an archive ART has just said is
  // not a WHDLoad package at all.
  const found = hasPack(plan);
  const verdictPhrase = describeVerdict(verdict);

  return (
    <div style={{ marginTop: 8 }}>
      <div
        className={confident ? "badge badge-ok" : "badge badge-warn"}
        style={{ display: "block", fontSize: 12 }}
      >
        {t(verdictPhrase.key, verdictPhrase.params)} — {verdict.notes}
      </div>

      {found && (
        <div style={{ fontSize: 13, marginTop: 6 }}>
          <strong>{layout.name}</strong>
          {layout.icon === null && (
            <span className="badge badge-warn" style={{ fontSize: 11, marginLeft: 6 }}>
              {t("whdload.detection.noIcon")}
            </span>
          )}
        </div>
      )}

      {found && powerMode && (
        <div className="faint" style={{ fontSize: 11, marginTop: 4 }}>
          <div>{t("whdload.detection.slaveLabel", { slave: layout.slave })}</div>
          {layout.icon && (
            <div>{t("whdload.detection.iconLabel", { icon: layout.icon })}</div>
          )}
          {layout.root === "" && <div>{t("whdload.detection.noWrapper")}</div>}
        </div>
      )}

      {/* Not an error, and never silent: a user who expected the readme on the
          disk should be able to see that ART left it out on purpose. */}
      {layout.outside.length > 0 && (
        <div className="faint" style={{ fontSize: 11, marginTop: 4 }}>
          {t("whdload.detection.notPartOfGame")}{" "}
          {layout.outside.slice(0, 4).join(", ")}
          {layout.outside.length > 4 &&
            ` ${t("whdload.detection.andMore", { count: layout.outside.length - 4 })}`}
        </div>
      )}
    </div>
  );
}

/**
 * The partition list.
 *
 * §2.5's rule again: a partition ART cannot use is still listed, by name and
 * with the reason. A short list would say "your disk is broken" when it is not.
 */
function PartitionChoice({
  volumes,
  chosen,
  onChoose,
}: {
  volumes: ImageVolumes;
  chosen: number | null;
  onChoose: (index: number) => void;
}) {
  return (
    <div style={{ marginTop: 8 }}>
      {volumes.volumes.map((volume, index) => (
        <PartitionRow
          key={index}
          volume={volume}
          index={index}
          chosen={chosen === index}
          onChoose={onChoose}
        />
      ))}
      {volumes.note && (
        <div className="faint" style={{ fontSize: 11, marginTop: 4 }}>
          {volumes.note}
        </div>
      )}
    </div>
  );
}

function PartitionRow({
  volume,
  index,
  chosen,
  onChoose,
}: {
  volume: VolumeEntry;
  index: number;
  chosen: boolean;
  onChoose: (index: number) => void;
}) {
  const usable = isMountable(volume);

  return (
    <button
      className={`btn${chosen ? " btn-primary" : ""}`}
      style={{
        display: "block",
        width: "100%",
        textAlign: "left",
        fontSize: 12,
        marginBottom: 4,
      }}
      disabled={!usable}
      onClick={() => onChoose(index)}
      title={usable ? undefined : volume.unsupported ?? undefined}
    >
      <strong>{volume.name}</strong> · {volume.filesystem} ·{" "}
      {formatBytes(volume.byte_length)}
      {!usable && (
        <span className="faint"> — {volume.unsupported}</span>
      )}
    </button>
  );
}

/** The plan, and the refusal when there is one. */
function WhatHappens({ plan, powerMode }: { plan: WhdloadPlan; powerMode: boolean }) {
  const { t } = useTranslation();
  // When no pack was found there is nothing to cost: the drawer name is empty,
  // every number is zero and the disk was never read. Printing the sentence
  // anyway asserts an action — "create the drawer  and write 0 files, 0 B" —
  // immediately above the panel saying ART will not do it.
  const found = hasPack(plan);

  return (
    <div style={{ marginTop: 8 }}>
      {found && (
        <>
          <div style={{ fontSize: 13 }}>
            {t("whdload.plan.createDrawer")} <strong>{plan.drawer}</strong>{" "}
            {t("whdload.plan.writeFiles", { count: plan.cost.files })}
            {plan.cost.directories > 0 &&
              ` ${t("whdload.plan.inFolders", { count: plan.cost.directories })}`}
            , {formatBytes(plan.cost.total_bytes)}.
          </div>

          {/* Blocks, not just bytes: a game full of small files costs a block
              each, and a report in bytes alone would be wrong about whether it
              fits. */}
          <div className="faint" style={{ fontSize: 11, marginTop: 2 }}>
            {t("whdload.plan.blocks", {
              needed: plan.cost.blocks_needed.toLocaleString(),
              free: plan.cost.blocks_free.toLocaleString(),
            })}
          </div>
        </>
      )}

      {found && powerMode && (
        <div className="faint" style={{ fontSize: 11, marginTop: 4 }}>
          {t("whdload.plan.backupNote")}
        </div>
      )}

      {plan.refusal ? (
        // The remedy comes from Rust with the reason (see `WhdloadRefusal`).
        // A refusal with nothing useful to add says nothing rather than
        // repeating advice that does not apply to it.
        <Refusal
          title={t("whdload.plan.refusalTitle")}
          reason={plan.refusal.reason}
          suggestion={plan.refusal.suggestion ?? undefined}
        />
      ) : (
        <div className="badge badge-ok" style={{ display: "block", marginTop: 8 }}>
          {t("whdload.plan.allGood")}
        </div>
      )}
    </div>
  );
}

function Report({ outcome, powerMode }: { outcome: WhdloadOutcome; powerMode: boolean }) {
  const { t } = useTranslation();
  const complete = outcome.verified === outcome.files && outcome.skipped.length === 0;

  // `describeOutcome` cannot render the "N files" and "verified" clauses
  // itself (see its doc comment) — this mirrors how `whdload.plan.writeFiles`
  // is resolved above, in `WhatHappens`.
  const outcomePhrase = describeOutcome(outcome);
  const files = t("whdload.outcome.filesCount", { count: outcome.files });
  const verified =
    outcome.verified === outcome.files
      ? t("whdload.outcome.allVerified")
      : t("whdload.outcome.verifiedCount", { count: outcome.verified });

  return (
    <section className="card" style={{ marginTop: 12 }}>
      <div
        className={complete ? "badge badge-ok" : "badge badge-warn"}
        style={{ display: "block" }}
      >
        {t(outcomePhrase.key, { ...outcomePhrase.params, files, verified })}
      </div>

      {outcome.skipped.length > 0 && (
        <div className="faint" style={{ fontSize: 11, marginTop: 6 }}>
          {t("whdload.report.notWritten")} {outcome.skipped.slice(0, 5).join(" · ")}
        </div>
      )}

      {/* I2: a title too long for iGame's line, most often — named rather
          than silently dropped, and never a reason to call an install a
          failure by itself (`igame.data` is best-effort metadata WHDLoad
          itself never reads). */}
      {outcome.igame_omitted.length > 0 && (
        <div className="faint" style={{ fontSize: 11, marginTop: 6 }}>
          {t("whdload.report.igameOmitted")} {outcome.igame_omitted.join(" · ")}
        </div>
      )}

      {powerMode && outcome.backup && (
        <div className="faint" style={{ fontSize: 11, marginTop: 4, wordBreak: "break-all" }}>
          {t("whdload.report.backupKept", { backup: outcome.backup })}
        </div>
      )}
    </section>
  );
}
