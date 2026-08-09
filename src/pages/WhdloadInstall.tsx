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
  onWhdloadResult,
  whdloadInstall,
  whdloadPlan,
  type WhdloadOutcome,
  type WhdloadPlan,
} from "@/lib/whdload";

export function WhdloadInstall() {
  const { t } = useTranslation();
  const powerMode = usePowerMode();
  const location = useLocation();

  const [archive, setArchive] = useState<string | null>(null);
  const [image, setImage] = useState<string | null>(null);
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
    setBusy("Looking inside the package…");
    setError(null);
    try {
      setPlan(await whdloadPlan(archive, image, volumeIndex));
    } catch (e) {
      setPlan(null);
      setError(String(e));
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
      filters: [{ name: "Amiga archive", extensions: ["lha", "lzh"] }],
    });
    if (typeof picked !== "string") return;
    setArchive(picked);
    setOutcome(null);
  }

  async function chooseImage() {
    const picked = await open({
      multiple: false,
      filters: [{ name: "Amiga hard disk image", extensions: ["hdf", "hda", "img", "adf"] }],
    });
    if (typeof picked !== "string") return;

    setError(null);
    setOutcome(null);
    setImage(picked);
    setVolumeIndex(null);
    setBusy("Reading the disk…");
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
            ? "ART found no volumes in that image."
            : "ART cannot write to any partition in that image — see the list below."
        );
      }
    } catch (e) {
      setVolumes(null);
      setError(String(e));
    } finally {
      setBusy(null);
    }
  }

  async function install() {
    if (!archive || !image || volumeIndex === null) return;
    setError(null);
    setBusy("Installing…");
    try {
      pending.current = await whdloadInstall(archive, image, volumeIndex);
    } catch (e) {
      setError(String(e));
      setBusy(null);
      pending.current = null;
    }
  }

  const ready = plan !== null && plan.refusal === null && busy === null;

  return (
    <div style={{ maxWidth: 780 }}>
      <h1 style={{ fontSize: 20 }}>{t("nav.whdload")}</h1>
      <p className="muted" style={{ marginTop: 4 }}>
        Put a WHDLoad package on a hard disk, in one step. ART checks what is in
        the archive, tells you exactly what it will write and where, and only
        then writes it — backing the image up first and reading every file back
        afterwards.
      </p>

      {error && (
        <div className="badge badge-err" style={{ display: "block", margin: "10px 0" }}>
          {error}
        </div>
      )}
      {busy && (
        <div className="muted" style={{ fontSize: 12, margin: "8px 0" }}>
          {busy} — progress is in the job bar, and you can stop it there.
        </div>
      )}

      {/* ---- 1. the package ---- */}
      <section className="card" style={{ marginBottom: 10 }}>
        <div style={{ display: "flex", gap: 8, alignItems: "center", flexWrap: "wrap" }}>
          <strong style={{ fontSize: 14 }}>1. The package</strong>
          <button className="btn" style={{ fontSize: 12 }} onClick={() => void chooseArchive()}>
            Choose an .lha…
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
          <strong style={{ fontSize: 14 }}>2. The disk</strong>
          <button
            className="btn"
            style={{ fontSize: 12 }}
            onClick={() => void chooseImage()}
            disabled={!archive}
            title={archive ? undefined : "Choose a package first"}
          >
            Choose an image…
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
          <strong style={{ fontSize: 14 }}>3. What ART will do</strong>
          <WhatHappens plan={plan} powerMode={powerMode} />
        </section>
      )}

      <button
        className="btn btn-primary"
        onClick={() => void install()}
        disabled={!ready}
        title={
          ready
            ? `Install to ${plan?.drawer}`
            : plan?.refusal ?? "Choose a package and a disk first"
        }
        style={{ fontSize: 14, padding: "8px 18px" }}
      >
        Install to {plan?.volume_name ?? "the disk"}
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
  const { verdict, layout } = plan;
  const confident = verdict.confidence === "HIGH" || verdict.confidence === "MEDIUM";

  return (
    <div style={{ marginTop: 8 }}>
      <div
        className={confident ? "badge badge-ok" : "badge badge-warn"}
        style={{ display: "block", fontSize: 12 }}
      >
        {describeVerdict(verdict)} — {verdict.notes}
      </div>

      <div style={{ fontSize: 13, marginTop: 6 }}>
        <strong>{layout.name}</strong>
        {layout.icon === null && (
          <span className="badge badge-warn" style={{ fontSize: 11, marginLeft: 6 }}>
            no icon — it will not show up on Workbench
          </span>
        )}
      </div>

      {powerMode && (
        <div className="faint" style={{ fontSize: 11, marginTop: 4 }}>
          <div>Slave: {layout.slave}</div>
          {layout.icon && <div>Icon: {layout.icon}</div>}
          {layout.root === "" && <div>The archive has no wrapper drawer.</div>}
        </div>
      )}

      {/* Not an error, and never silent: a user who expected the readme on the
          disk should be able to see that ART left it out on purpose. */}
      {layout.outside.length > 0 && (
        <div className="faint" style={{ fontSize: 11, marginTop: 4 }}>
          Not part of the game, so not installed:{" "}
          {layout.outside.slice(0, 4).join(", ")}
          {layout.outside.length > 4 && ` and ${layout.outside.length - 4} more`}
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
  return (
    <div style={{ marginTop: 8 }}>
      <div style={{ fontSize: 13 }}>
        Create the drawer <strong>{plan.drawer}</strong> and write{" "}
        {plan.cost.files} file{plan.cost.files === 1 ? "" : "s"}
        {plan.cost.directories > 0 &&
          ` in ${plan.cost.directories} folder${
            plan.cost.directories === 1 ? "" : "s"
          }`}
        , {formatBytes(plan.cost.total_bytes)}.
      </div>

      {/* Blocks, not just bytes: a game full of small files costs a block
          each, and a report in bytes alone would be wrong about whether it
          fits. */}
      <div className="faint" style={{ fontSize: 11, marginTop: 2 }}>
        {plan.cost.blocks_needed.toLocaleString()} blocks needed ·{" "}
        {plan.cost.blocks_free.toLocaleString()} free
      </div>

      {powerMode && (
        <div className="faint" style={{ fontSize: 11, marginTop: 4 }}>
          The image is backed up (or journalled, if it is large) before anything
          is written, and every file is read back out of the disk afterwards.
        </div>
      )}

      {plan.refusal ? (
        <div className="badge badge-err" style={{ display: "block", marginTop: 8 }}>
          {plan.refusal}
        </div>
      ) : (
        <div className="badge badge-ok" style={{ display: "block", marginTop: 8 }}>
          Everything fits, every name is one AmigaDOS can store, and nothing of
          that name is on the disk already.
        </div>
      )}
    </div>
  );
}

function Report({ outcome, powerMode }: { outcome: WhdloadOutcome; powerMode: boolean }) {
  const complete = outcome.verified === outcome.files && outcome.skipped.length === 0;

  return (
    <section className="card" style={{ marginTop: 12 }}>
      <div
        className={complete ? "badge badge-ok" : "badge badge-warn"}
        style={{ display: "block" }}
      >
        {describeOutcome(outcome)}
      </div>

      {outcome.skipped.length > 0 && (
        <div className="faint" style={{ fontSize: 11, marginTop: 6 }}>
          Not written: {outcome.skipped.slice(0, 5).join(" · ")}
        </div>
      )}

      {powerMode && outcome.backup && (
        <div className="faint" style={{ fontSize: 11, marginTop: 4, wordBreak: "break-all" }}>
          The previous image was kept at {outcome.backup}
        </div>
      )}
    </section>
  );
}
