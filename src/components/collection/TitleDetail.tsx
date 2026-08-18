import { useEffect, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { useTranslation } from "react-i18next";

import { Guessed } from "@/components/collection/Guessed";
import { canLaunch, diskList, kindPhrase, mediaPhrase } from "@/lib/collectionDetail";
import {
  artworkAttach,
  artworkDetach,
  artworkDir,
  artworkForTitle,
  isSupportedPicture,
  type ArtKind,
} from "@/lib/artwork";
import type { CatalogueEntry } from "@/lib/gameindex";
import {
  launchKindPhrase,
  launchPlan,
  launchTitle,
  machinePhrase,
  notePhrase,
  refusalPhrase,
  type LaunchPreview,
  type Machine,
} from "@/lib/launch";
import { isFlag, isOneOf, isText, isTextOrNothing } from "@/lib/remembered";
import { useRemembered } from "@/lib/useRemembered";
import { usePowerMode } from "@/lib/uxmode";

const isMachine = isOneOf<Machine>("a500", "a1200");
type MachineChoice = "auto" | Machine;
const isMachineChoice = isOneOf<MachineChoice>("auto", "a500", "a1200");

/**
 * The A500/A1200 picker, shared between the global default and the
 * per-title override — the same small control either way, just with a
 * different option set and a different remembered value behind it. Kept as
 * its own component so the translated call over `machinePhrase(...).key` is
 * one source occurrence (one dynamic call site for `literal-keys.test.ts` to
 * count) rather than two.
 */
function MachineChoiceButtons({
  options,
  value,
  onChange,
}: {
  options: readonly MachineChoice[];
  value: string;
  onChange: (choice: MachineChoice) => void;
}) {
  const { t } = useTranslation();
  return (
    <>
      {options.map((option) => (
        <button
          key={option}
          className={`btn btn-sm ${value === option ? "btn-primary" : ""}`}
          onClick={() => onChange(option)}
        >
          {option === "auto"
            ? t("collection.detail.play.machineAuto")
            : t(machinePhrase(option).key)}
        </button>
      ))}
    </>
  );
}

/**
 * The detail panel a title's card opens into (Collection · wave C).
 *
 * `art` is the one picture the screen already resolves per title — the
 * `ArtKind` the artwork cache prefers first for whichever titles are on
 * screen (`CollectionStudio`'s `art` map, built from `artworkKnown()`). It is
 * also this panel's fallback while its own query (`artworkForTitle`) is in
 * flight, or once it resolves to nothing.
 *
 * When a title holds more than one kind of picture — a hand-attached one
 * beside the `.rp9` snap, say — the panel offers a switch between them,
 * defaulting to the first in preference order (the one the grid already
 * shows).
 */
export function TitleDetail({
  entry,
  art,
  hasManualArt,
  onArtChanged,
  onClose,
  playRequest,
}: {
  entry: CatalogueEntry;
  art: string | undefined;
  /** Whether this title's cached picture is one the user attached by hand. */
  hasManualArt: boolean;
  /** Re-read the artwork cache — the same re-read the screen already does
   *  after an artwork job finishes. */
  onArtChanged: () => void;
  onClose: () => void;
  /**
   * Bumped by the grid/table Play button (Collection · wave C, Task 11) to
   * fetch a launch plan the moment the panel opens for it — the button that
   * used to navigate to a dead `/winuae` route now opens the real thing. `0`
   * means "not asked for yet"; the panel's own Play button still works with
   * no bump at all.
   */
  playRequest: number;
}) {
  const { t } = useTranslation();
  const power = usePowerMode();
  const record = entry.record;
  const disks = diskList(record.media);
  const media = mediaPhrase(record.media);
  const [artError, setArtError] = useState<string | null>(null);
  const [pictures, setPictures] = useState<{ kind: ArtKind; src: string }[]>([]);
  const [chosenKind, setChosenKind] = useState<ArtKind | null>(null);

  // Launch settings that apply to every title (a ROM folder, the default
  // machine, the bootable system a WHDLoad title mounts) — remembered under
  // fixed keys rather than per record id, and shown here because this is the
  // only screen with anywhere to put them.
  const [romDir, setRomDir] = useRemembered<string>("launch.romDir", isText, "");
  const [defaultMachine, setDefaultMachine] = useRemembered<Machine>(
    "launch.defaultMachine",
    isMachine,
    "a500"
  );
  const [systemVolume, setSystemVolume] = useRemembered<string | null>(
    "launch.systemVolume",
    isTextOrNothing,
    null
  );

  // This title's own choices — keyed by record id, so switching titles does
  // not carry one game's override onto another's.
  const [machineChoice, setMachineChoice] = useRemembered<MachineChoice>(
    `launch.machine.${record.id}`,
    isMachineChoice,
    "auto"
  );
  const [oneClick, setOneClick] = useRemembered<boolean>(
    `launch.oneClick.${record.id}`,
    isFlag,
    true
  );

  const [preview, setPreview] = useState<LaunchPreview | null>(null);
  const [planning, setPlanning] = useState(false);
  const [launching, setLaunching] = useState(false);
  const [launchedPid, setLaunchedPid] = useState<number | null>(null);
  const [launchError, setLaunchError] = useState<string | null>(null);

  function launchArgs() {
    return {
      id: record.id,
      title: record.title.value,
      path: entry.path,
      media: record.media,
      chipset: record.chipset?.value ?? null,
      rom_dir: romDir,
      default_machine: machineChoice === "auto" ? defaultMachine : machineChoice,
      system_volume: systemVolume,
      one_click: oneClick,
    };
  }

  async function runPlan() {
    setPlanning(true);
    setLaunchError(null);
    setLaunchedPid(null);
    try {
      setPreview(await launchPlan(launchArgs()));
    } catch (e) {
      setPreview(null);
      setLaunchError(String(e));
    } finally {
      setPlanning(false);
    }
  }

  async function runLaunch() {
    setLaunching(true);
    setLaunchError(null);
    try {
      setLaunchedPid(await launchTitle(launchArgs()));
    } catch (e) {
      setLaunchError(String(e));
    } finally {
      setLaunching(false);
    }
  }

  async function pickRomDir() {
    const sel = await open({
      directory: true,
      multiple: false,
      title: t("collection.detail.play.romDirDialog"),
    });
    if (typeof sel === "string") setRomDir(sel);
  }

  async function pickSystemVolume() {
    const sel = await open({
      multiple: false,
      title: t("collection.detail.play.systemVolumeDialog"),
    });
    if (typeof sel === "string") setSystemVolume(sel);
  }

  // The grid/table Play button asked for this title specifically — fetch its
  // plan right away, the same read-only preview the panel's own Play button
  // produces, so one click gets the user to a confirmation rather than only
  // to an open panel they have to act in a second time.
  useEffect(() => {
    if (playRequest === 0) return;
    void runPlan();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [playRequest]);

  // A stale plan for the last title must not be shown as though it were
  // this one's.
  useEffect(() => {
    setPreview(null);
    setLaunchError(null);
    setLaunchedPid(null);
  }, [record.id]);

  /**
   * Every picture this title has, built the same way `CollectionStudio`
   * builds the grid's thumbnails: `artworkDir()` plus each `ArtRef.file`,
   * through `convertFileSrc`. The chosen kind resets to the first of the
   * list — the preferred one, the picture the grid was already showing.
   */
  async function loadPictures() {
    try {
      const [dir, refs] = await Promise.all([artworkDir(), artworkForTitle(record.title.value)]);
      const { convertFileSrc } = await import("@tauri-apps/api/core");
      const next = refs.map((ref) => ({
        kind: ref.kind,
        src: convertFileSrc(`${dir}/${ref.file}`),
      }));
      setPictures(next);
      setChosenKind(next[0]?.kind ?? null);
    } catch {
      // A cache that cannot be read is a panel without a switch, not a panel
      // that fails to open — the `art` prop still stands on its own.
      setPictures([]);
      setChosenKind(null);
    }
  }

  useEffect(() => {
    void loadPictures();
    // The chosen kind is meant to reset on every title change, so this
    // depends only on the title, not on `loadPictures` itself.
  }, [record.title.value]);

  async function attach() {
    setArtError(null);
    const chosen = await open({
      multiple: false,
      filters: [{ name: t("collection.detail.art.filter"), extensions: ["png", "jpg", "jpeg"] }],
      title: t("collection.detail.art.dialog"),
    });
    if (typeof chosen !== "string") return;
    // The dialog's own filter already narrows to PNG/JPEG, but a translated
    // refusal here — rather than surfacing Rust's English-only one (ART-060)
    // — needs its own check, kept identical to the Rust gate on purpose.
    if (!isSupportedPicture(chosen)) {
      setArtError(t("collection.detail.art.rejected"));
      return;
    }
    try {
      await artworkAttach(record.title.value, record.id, chosen);
      onArtChanged();
      void loadPictures();
    } catch (e) {
      setArtError(String(e));
    }
  }

  async function detach() {
    setArtError(null);
    try {
      await artworkDetach(record.title.value, record.id);
      onArtChanged();
      void loadPictures();
    } catch (e) {
      setArtError(String(e));
    }
  }

  // The chosen kind's picture, falling back to the `art` prop while the
  // query is in flight or once it resolves to nothing — so nothing regresses
  // for a title the artwork cache does not know about yet.
  const chosenPicture = pictures.find((picture) => picture.kind === chosenKind)?.src ?? art;

  return (
    <section className="card" style={{ display: "flex", flexDirection: "column", gap: 10 }}>
      <div style={{ display: "flex", justifyContent: "space-between", alignItems: "start" }}>
        <h2 style={{ fontSize: 16, margin: 0 }}>{record.title.value}</h2>
        <button className="btn btn-sm" onClick={onClose}>
          {t("common.close")}
        </button>
      </div>

      {chosenPicture && (
        <img
          src={chosenPicture}
          alt=""
          style={{ display: "block", width: "100%", maxHeight: 260, objectFit: "contain" }}
        />
      )}

      {/* A control with one option is noise — only shown once this title
          holds more than one kind of picture. */}
      {pictures.length > 1 && (
        <div style={{ display: "flex", gap: 6, flexWrap: "wrap" }}>
          {pictures.map((picture) => (
            <button
              key={picture.kind}
              className={`btn btn-sm ${chosenKind === picture.kind ? "btn-primary" : ""}`}
              onClick={() => setChosenKind(picture.kind)}
            >
              {t(kindPhrase(picture.kind).key)}
            </button>
          ))}
        </div>
      )}

      <div style={{ display: "flex", gap: 6 }}>
        <button className="btn btn-sm" onClick={() => void attach()}>
          {t("collection.detail.art.attach")}
        </button>
        {hasManualArt && (
          <button className="btn btn-sm" onClick={() => void detach()}>
            {t("collection.detail.art.remove")}
          </button>
        )}
      </div>
      {artError && (
        <div className="badge badge-err" style={{ fontSize: 11 }}>
          {artError}
        </div>
      )}

      <div className="muted" style={{ fontSize: 13 }}>
        {t(media.key, media.params)}
      </div>

      {/* The facts, each keeping the `Guessed` mark the card already uses —
          a value ART inferred must not read as one it was told. */}
      <dl style={{ display: "grid", gridTemplateColumns: "auto 1fr", gap: "4px 10px", margin: 0, fontSize: 13 }}>
        <dt className="muted">{t("collection.detail.publisher")}</dt>
        <dd style={{ margin: 0 }}>
          {record.publisher?.value ?? t("common.unknown")}
          {record.publisher && <Guessed from={record.publisher.from} />}
        </dd>
        <dt className="muted">{t("collection.detail.year")}</dt>
        <dd style={{ margin: 0 }}>
          {record.year?.value ?? t("common.unknown")}
          {record.year && <Guessed from={record.year.from} />}
        </dd>
        <dt className="muted">{t("collection.detail.genre")}</dt>
        <dd style={{ margin: 0 }}>
          {record.genre?.value ?? t("common.unknown")}
          {record.genre && <Guessed from={record.genre.from} />}
        </dd>
        <dt className="muted">{t("collection.detail.rating")}</dt>
        <dd style={{ margin: 0 }}>{record.rating?.value ?? t("common.unknown")}</dd>
      </dl>

      {/* `KickstartNeed.image` is nullable — a slave can declare a size and a
          CRC and no name at all — so the guard is on the image, not on the
          need. Rendering `null` into the sentence is the bug this avoids. */}
      {record.kickstart?.value.image && (
        <div className="faint" style={{ fontSize: 12 }}>
          {t("gameindex.kickstartNeeded", { image: record.kickstart.value.image })}
        </div>
      )}

      {disks.length > 0 && (
        <ol style={{ fontSize: 12, margin: 0, paddingLeft: 20 }}>
          {disks.map((disk) => (
            <li key={disk}>{disk}</li>
          ))}
        </ol>
      )}

      {/* Play (Collection · wave C, Task 11). The confirmation comes before
          the destructive-feeling step: `runPlan` only reads a ROM folder and
          decides, `runLaunch` is the one call that actually starts a
          process, and it is reached through its own button once the plan is
          on screen — never automatically. */}
      {canLaunch(record.media) && (
        <section
          style={{
            borderTop: "1px solid var(--border)",
            paddingTop: 10,
            display: "flex",
            flexDirection: "column",
            gap: 8,
          }}
        >
          <strong style={{ fontSize: 13 }}>{t("collection.detail.play.title")}</strong>

          {/* Settings that apply to every title, remembered globally. */}
          <div style={{ display: "flex", flexDirection: "column", gap: 6, fontSize: 12 }}>
            <label style={{ display: "flex", flexDirection: "column", gap: 2 }}>
              {t("collection.detail.play.romDir")}
              <div style={{ display: "flex", gap: 6 }}>
                <input
                  type="text"
                  value={romDir}
                  onChange={(e) => setRomDir(e.target.value)}
                  style={{
                    flex: 1,
                    minWidth: 0,
                    padding: "3px 6px",
                    background: "var(--bg)",
                    color: "var(--text)",
                    border: "1px solid var(--border)",
                    borderRadius: 3,
                    fontSize: 12,
                  }}
                />
                <button className="btn btn-sm" onClick={() => void pickRomDir()}>
                  {t("collection.detail.play.browse")}
                </button>
              </div>
            </label>

            <div style={{ display: "flex", gap: 6, alignItems: "center", flexWrap: "wrap" }}>
              <span className="muted">{t("collection.detail.play.defaultMachine")}</span>
              <MachineChoiceButtons
                options={["a500", "a1200"]}
                value={defaultMachine}
                onChange={(choice) => choice !== "auto" && setDefaultMachine(choice)}
              />
            </div>

            {record.media.kind === "whdload-drawer" && (
              <label style={{ display: "flex", flexDirection: "column", gap: 2 }}>
                {t("collection.detail.play.systemVolume")}
                <div style={{ display: "flex", gap: 6 }}>
                  <input
                    type="text"
                    value={systemVolume ?? ""}
                    readOnly
                    placeholder={t("collection.detail.play.systemVolumeNone")}
                    style={{
                      flex: 1,
                      minWidth: 0,
                      padding: "3px 6px",
                      background: "var(--bg)",
                      color: "var(--text)",
                      border: "1px solid var(--border)",
                      borderRadius: 3,
                      fontSize: 12,
                    }}
                  />
                  <button className="btn btn-sm" onClick={() => void pickSystemVolume()}>
                    {t("collection.detail.play.browse")}
                  </button>
                </div>
              </label>
            )}
          </div>

          {/* This title's own choices. */}
          <div style={{ display: "flex", gap: 6, alignItems: "center", flexWrap: "wrap", fontSize: 12 }}>
            <span className="muted">{t("collection.detail.play.machineForThisTitle")}</span>
            <MachineChoiceButtons
              options={["auto", "a500", "a1200"]}
              value={machineChoice}
              onChange={setMachineChoice}
            />
          </div>

          {record.media.kind === "whdload-drawer" && (
            <div style={{ display: "flex", gap: 6, alignItems: "center", fontSize: 12 }}>
              <button
                className={`btn btn-sm ${oneClick ? "btn-primary" : ""}`}
                onClick={() => setOneClick(true)}
              >
                {t("collection.detail.play.oneClick")}
              </button>
              <button
                className={`btn btn-sm ${!oneClick ? "btn-primary" : ""}`}
                onClick={() => setOneClick(false)}
              >
                {t("collection.detail.play.mountOnly")}
              </button>
            </div>
          )}

          <button
            className="btn btn-sm btn-primary"
            onClick={() => void runPlan()}
            disabled={planning}
          >
            🚀 {planning ? t("collection.detail.play.planning") : t("collection.detail.play.action")}
          </button>

          {preview?.refusal && (
            <div className="badge badge-err" style={{ fontSize: 11 }}>
              {t(refusalPhrase(preview.refusal).key, refusalPhrase(preview.refusal).params)}
            </div>
          )}

          {preview?.plan && (
            <div
              style={{
                display: "flex",
                flexDirection: "column",
                gap: 6,
                fontSize: 12,
                border: "1px solid var(--border)",
                borderRadius: 4,
                padding: 8,
              }}
            >
              <div>
                {t("collection.detail.play.willUse", {
                  machine: t(machinePhrase(preview.plan.machine).key),
                  rom: preview.plan.rom.name,
                })}
              </div>
              <div>
                {t(launchKindPhrase(preview.plan.kind).key, launchKindPhrase(preview.plan.kind).params)}
              </div>
              {preview.plan.kind.kind === "floppies" && preview.plan.kind.images.length > 0 && (
                <ol style={{ margin: 0, paddingLeft: 18 }}>
                  {preview.plan.kind.images.map((image) => (
                    <li key={image}>{image}</li>
                  ))}
                </ol>
              )}
              {preview.plan.notes.map((note, index) => (
                <div key={index} className="faint">
                  {t(notePhrase(note).key, notePhrase(note).params)}
                </div>
              ))}
              <button
                className="btn btn-sm btn-primary"
                onClick={() => void runLaunch()}
                disabled={launching}
              >
                {launching
                  ? t("collection.detail.play.starting")
                  : t("collection.detail.play.start")}
              </button>
            </div>
          )}

          {launchedPid !== null && (
            <div className="badge badge-ok" style={{ fontSize: 11 }}>
              {t("collection.detail.play.started", { pid: launchedPid })}
            </div>
          )}
          {launchError && (
            <div className="badge badge-err" style={{ fontSize: 11 }}>
              {launchError}
            </div>
          )}
        </section>
      )}

      {/* Beginner mode hides the raw path — and hides only. No action below
          is disabled by the mode (§47, §48). */}
      {power && (
        <div className="faint" style={{ fontSize: 11, wordBreak: "break-all" }}>
          {entry.path}
        </div>
      )}
    </section>
  );
}
