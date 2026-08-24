// Building a PiStorm card image, from the user's own Emu68 release and their
// own Kickstart (SD-1 · G2).
//
// **Four questions, then Preview, then Build.** Everything else — the board,
// the release line, the FAT32 label, the boot partition's size, the Amiga
// disk's one partition — has a defaulted answer behind `Advanced`, and the
// hardware half of that writes to the *same* remembered keys the PiStorm
// studio uses, so changing it in either place changes it in both. A setting
// that means one thing on one screen and another on the next is the rule this
// project holds hardest.
//
// Nothing is written until the user has seen what would be written (§92), and
// `SAFE_CREATE` is answered by the plan rather than by a job that fails: the
// screen says "that file is already there" before the button, not after it.

import { useEffect, useRef, useState } from "react";
import { useOutletContext } from "react-router-dom";
import { open, save } from "@tauri-apps/plugin-dialog";
import { useTranslation } from "react-i18next";

import { useBuildSession } from "@/lib/useBuildSession";

import {
  buildBlocker,
  cardBuild,
  cardPlanBuild,
  cardCheckImage,
  defaultPartition,
  defaultSecondPartition,
  findingPhrase,
  cardIntake,
  healthCheckPhrase,
  healthVerdict,
  intakeFills,
  manualStepPhrase,
  rolePhrase,
  onCardBuildResult,
  payloadBytes,
  warningPhrase,
  cardFsChoices,
  type CardBuildPlan,
  type CardBuildRequest,
  type CardBuildResult,
  type CardIntakeItem,
  type HealthReport,
} from "@/lib/cardBuild";
import type { AmigaHardDiskFs, PartitionSpec } from "@/lib/hdf";
import {
  DEFAULT_EMU68_OPTIONS,
  DEFAULT_FIRMWARE_CONFIG,
  DEFAULT_HARDWARE,
  type Emu68Line,
  type Emu68Options,
  type FirmwareConfig,
  type PistormHardware,
} from "@/lib/pistorm";
import {
  EMU68_OPTION_SPEC,
  FIRMWARE_SPEC,
  HARDWARE_SPEC,
  isEmu68Line,
} from "@/lib/pistormOptions";
import { subscribeSafely } from "@/lib/jobs";
import { isFlag, isOneOf, isText, isTextOrNothing, isWholeNumberBetween } from "@/lib/remembered";
import { fileSystemInputsFor } from "@/lib/fsDriver";
import { FILESYSTEM_DRIVER_KEY } from "@/lib/preload";
import { useRemembered, useRememberedShape } from "@/lib/useRemembered";
import { usePowerMode } from "@/lib/uxmode";
import { GIB, gibNumber as gb, mibNumber as mib } from "@/lib/size";
import { Field } from "@/components/osbuilder/Field";
import { errorText } from "@/lib/errorText";

/** Card sizes people actually buy. */
const CARD_SIZES_GB = [2, 4, 8, 16, 32, 64, 128, 256];

const AMIGA_TARGETS = ["a500", "a1000", "a2000", "a600", "a1200"] as const;
const VARIANTS = ["classic", "pistorm600", "pistorm16", "pistorm32-lite"] as const;
const PI_MODELS = [
  "zero2-w",
  "pi3-a",
  "pi3-a-plus",
  "pi3-b",
  "pi3-b-plus",
  "pi4-b",
  "cm4",
] as const;
const LINES: Emu68Line[] = ["stable", "alpha11"];

export function CardBuilder() {
  const { t } = useTranslation();
  const { session, setRom, setCard } = useBuildSession();
  const powerMode = usePowerMode();

  // --- what the user chose, remembered ------------------------------------
  const [archive, setArchive] = useRemembered<string | null>(
    "cardBuilder.archive",
    isTextOrNothing,
    null
  );
  /**
   * **One Kickstart for the build** (ART-197's fourth row, wave 2).
   *
   * This panel kept its own remembered key until 2026-08-23, so a user chose
   * the same ROM here, on the install step and on the card step. They are one
   * question wearing three labels — the ROM the tree is paired against, the
   * ROM the emulator boots, and the ROM written onto the card — and a build
   * where they differ is the mismatch G9's pairing check exists to catch.
   *
   * Changing it here changes it for the build, which the hint says out loud:
   * a carry the user cannot see is the same defect as one that never
   * happened (ART-197's own words).
   */
  const kickstart = session.rom.path;
  const setKickstart = setRom;
  /**
   * **One card for the build** (ART-197's remaining duplicate, wave 2).
   *
   * This panel kept its own remembered key until 2026-08-24, so the image ART
   * *wrote* and the image the volumes step *prepared* were two values joined
   * by nothing. That is ART-197's own defect in its second instance, and its
   * own sentence still covers it: a user who has just watched ART write
   * something should not be asked to go and find it.
   *
   * The pair the Kickstart field above already is, in the other direction —
   * and changing it here changes it for the build, which the hint says out
   * loud, because a carry the user cannot see is the same defect as one that
   * never happened.
   */
  const dest = session.card.image;
  const setDest = setCard;
  const [cardGb, setCardGb] = useRemembered(
    "cardBuilder.cardGb",
    isWholeNumberBetween(2, 2048),
    32
  );
  const [label, setLabel] = useRemembered("cardBuilder.label", isText, "ART CARD");
  // 0 means the measured 1.10 GiB both real cards carry — the engine's rule,
  // not a number this screen invents.
  const [bootMib, setBootMib] = useRemembered(
    "cardBuilder.bootMib",
    isWholeNumberBetween(0, 8192),
    0
  );
  const [driveName, setDriveName] = useRemembered(
    "cardBuilder.driveName",
    isText,
    defaultPartition().drive_name
  );
  const [partitionMb, setPartitionMb] = useRemembered(
    "cardBuilder.partitionMb",
    isWholeNumberBetween(1, 2_000_000),
    defaultPartition().size_mb
  );
  /**
   * The filesystem driver ART may embed — `pfs3aio`, from the user's own disk.
   *
   * **The same remembered key the volume step uses**, deliberately: it is one
   * answer to one question, and asking for the same file twice in one wizard
   * is the drift ART-197 was filed about. It folds into the build session
   * proper when wave 2 rehomes these panels.
   */
  const [fsDriver, setFsDriver] = useRemembered<string | null>(
    FILESYSTEM_DRIVER_KEY,
    isTextOrNothing,
    null
  );
  const fsChoices = cardFsChoices(fsDriver);
  /**
   * A second partition for the user's own files, on by default.
   *
   * Remembered like every other choice: somebody who turns it off gets one
   * partition next time too.
   */
  const [workPartition, setWorkPartition] = useRemembered(
    "cardBuilder.workPartition",
    isFlag,
    true
  );
  const [fsType, setFsType] = useRemembered<AmigaHardDiskFs>(
    "cardBuilder.fsType",
    isOneOf<AmigaHardDiskFs>(...fsChoices.map((choice) => choice.value)),
    defaultPartition().fs_type
  );

  // The same keys the PiStorm studio writes: one answer, two screens.
  const [hardware, applyHardware] = useRememberedShape<PistormHardware>(
    "pistorm.hardware",
    HARDWARE_SPEC,
    DEFAULT_HARDWARE
  );
  const [firmware] = useRememberedShape<FirmwareConfig>(
    "pistorm.firmware",
    FIRMWARE_SPEC,
    DEFAULT_FIRMWARE_CONFIG
  );
  const [options] = useRememberedShape<Emu68Options>(
    "pistorm.options",
    EMU68_OPTION_SPEC,
    DEFAULT_EMU68_OPTIONS
  );
  const [line, setLine] = useRemembered<Emu68Line>("pistorm.line", isEmu68Line, "stable");

  const [advanced, setAdvanced] = useRemembered(
    "cardBuilder.advanced",
    (v: unknown): v is boolean => typeof v === "boolean",
    false
  );

  // --- what the screen is doing -------------------------------------------
  const [plan, setPlan] = useState<CardBuildPlan | null>(null);
  const [result, setResult] = useState<CardBuildResult | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [report, setReport] = useState<HealthReport | null>(null);
  const [checking, setChecking] = useState(false);
  /** Which card the report on screen describes. */
  const [checkedPath, setCheckedPath] = useState<string | null>(null);
  /** What was last dropped on this screen, and what it became (G15). */
  const [dropped, setDropped] = useState<CardIntakeItem[]>([]);

  /**
   * `SDH0` for the system, and `SDH1` for everything else — the shape both
   * real cards have and the Imager's own default.
   *
   * A **toggle rather than a partition-list editor**, deliberately. The
   * reference layout is exactly two; three and more is multiboot, which is
   * SD-3's G16, and building a general editor for a case nothing yet asks for
   * would be offering a shape ART cannot finish (§96).
   */
  const partitions: PartitionSpec[] = [
    {
      drive_name: driveName,
      fs_type: fsType,
      size_mb: partitionMb,
      bootable: true,
      boot_priority: defaultPartition().boot_priority,
    },
    ...(workPartition ? [defaultSecondPartition(fsType)] : []),
  ];

  async function chooseFsDriver() {
    const picked = await open({
      multiple: false,
      directory: false,
      title: t("preload.driver.chooseTitle"),
    });
    if (typeof picked === "string") setFsDriver(picked);
  }

  const request: CardBuildRequest = {
    archive: archive ?? "",
    // Empty for FFS — Kickstart mounts that itself and a driver it never
    // asks for is dead weight in the reserved area (`fileSystemInputsFor`).
    file_systems: fileSystemInputsFor(fsType, fsDriver),
    kickstart,
    dest: dest ?? "",
    total_bytes: cardGb * GIB,
    boot_bytes: bootMib * 1024 * 1024,
    label,
    hardware,
    line,
    firmware,
    options,
    partitions,
  };
  // `built_at` is deliberately **not** part of `request`: the fingerprint
  // below is `JSON.stringify(request)`, and a clock in it would change on
  // every render and throw the plan away each time. It belongs to the build,
  // not to the plan, and is stamped at the call site.

  // A plan describes the request that produced it. Change any of the request
  // and the plan on screen stops being true — so it goes, rather than sitting
  // there describing a card nobody asked for any more.
  const fingerprint = JSON.stringify(request);
  const lastPlanned = useRef<string | null>(null);
  useEffect(() => {
    if (lastPlanned.current !== null && lastPlanned.current !== fingerprint) {
      setPlan(null);
    }
  }, [fingerprint]);

  // `subscribeSafely` (ART-165): the bare `.then((fn) => { unlisten = fn })`
  // shape this used to have could both leak the real Tauri listener (an
  // unmount before the promise resolved left nothing to call) and surface
  // an unhandled rejection (no IPC bridge to reach, e.g. under test).
  useEffect(() => {
    return subscribeSafely(() =>
      onCardBuildResult((built) => {
        setResult(built);
        setBusy(false);
        // A report describes the card it was run against, and this is a new
        // one. It goes with the old result rather than sitting under a new card.
        setReport(null);
        // The card is there now; the plan that described it no longer applies.
        setPlan(null);
      })
    );
  }, []);

  // G15: the drop pipeline is global (`Layout.tsx`), and this is the screen
  // asking it the *other* question — not "what can I do with this file" but
  // "what does it become in this card". A recognised archive or ROM fills its
  // field; everything else is answered rather than ignored.
  const dropContext = useOutletContext<{ analyses?: { path: string }[] }>();
  // A stable fingerprint of what was dropped, so the effect below runs
  // when the drop changes and not on every render.
  const droppedPaths = JSON.stringify((dropContext?.analyses ?? []).map((e) => e.path));
  useEffect(() => {
    const paths: string[] = JSON.parse(droppedPaths);
    if (paths.length === 0) return;
    let cancelled = false;
    void cardIntake(paths)
      .then((items) => {
        if (cancelled) return;
        setDropped(items);
        const fills = intakeFills(items);
        if (fills.archive) setArchive(fills.archive);
        if (fills.kickstart) setKickstart(fills.kickstart);
      })
      .catch((e) => setError(errorText(t, e)));
    return () => {
      cancelled = true;
    };
    // `setArchive`/`setKickstart` are stable store setters.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [droppedPaths]);

  async function chooseArchive() {
    const picked = await open({
      multiple: false,
      title: t("cardBuilder.archive.chooseTitle"),
      filters: [{ name: "Emu68 release", extensions: ["zip"] }],
    });
    if (typeof picked === "string") setArchive(picked);
  }

  async function chooseKickstart() {
    const picked = await open({
      multiple: false,
      title: t("cardBuilder.kickstart.chooseTitle"),
      filters: [{ name: "Kickstart ROM", extensions: ["rom", "bin"] }],
    });
    if (typeof picked === "string") setKickstart(picked);
  }

  async function chooseDest() {
    const picked = await save({
      title: t("cardBuilder.dest.chooseTitle"),
      defaultPath: "card.img",
      filters: [{ name: "Card image", extensions: ["img"] }],
    });
    if (typeof picked === "string") setDest(picked);
  }

  async function preview() {
    setBusy(true);
    setError(null);
    setResult(null);
    try {
      const made = await cardPlanBuild(request);
      setPlan(made);
      lastPlanned.current = fingerprint;
    } catch (e) {
      setPlan(null);
      setError(errorText(t, e));
    } finally {
      setBusy(false);
    }
  }

  async function build() {
    setBusy(true);
    setError(null);
    try {
      await cardBuild({ ...request, built_at: new Date().toISOString() });
      // `busy` is cleared by the result event, or here if the job never
      // starts. A cancelled or failed job is the job bar's to report.
    } catch (e) {
      setError(errorText(t, e));
      setBusy(false);
    }
  }

  /**
   * Check a card. `image` defaults to the one just built.
   *
   * **Any card, not only a fresh one.** The command has always accepted one —
   * a card with no manifest simply answers less — and a screen that could only
   * check what it had just made would be a smaller thing than the engine
   * behind it.
   */
  async function verify(image?: string, manifest?: string) {
    const target = image ?? result?.dest;
    if (!target) return;
    setChecking(true);
    setError(null);
    try {
      setReport(await cardCheckImage(target, manifest, hardware.pi));
      setCheckedPath(target);
    } catch (e) {
      setReport(null);
      setError(errorText(t, e));
    } finally {
      setChecking(false);
    }
  }

  async function chooseCardToCheck() {
    const picked = await open({
      multiple: false,
      title: t("cardBuilder.health.chooseTitle"),
      filters: [{ name: "Card image", extensions: ["img", "hdf"] }],
    });
    if (typeof picked === "string") await verify(picked);
  }

  const blocker = buildBlocker(request, plan);

  return (
    <>
      <section className="card" style={{ marginBottom: 16 }}>
        <h2 style={{ fontSize: 16, marginTop: 0 }}>{t("cardBuilder.heading")}</h2>
        <p className="muted" style={{ fontSize: 12, margin: "4px 0 12px" }}>
          {t("cardBuilder.intro")}
        </p>

        {/* What this card is and is not, at the top rather than at the end. */}
        <p
          className="badge badge-warn"
          style={{ display: "block", padding: "8px 12px", fontSize: 12, marginBottom: 12 }}
        >
          {t("cardBuilder.scope")}
        </p>

        {/* G15. What was dropped and what it became — including the files
            that have no place on *this* card yet, which is the answer SD-1
            owes most often and the one worth not swallowing. */}
        {dropped.length > 0 && (
          <div
            style={{
              border: "1px solid var(--border)",
              borderRadius: 4,
              padding: "8px 12px",
              marginBottom: 12,
            }}
          >
            <div className="muted" style={{ fontSize: 12, marginBottom: 6 }}>
              {t("cardBuilder.intake.heading")}
            </div>
            <ul style={{ margin: 0, paddingLeft: 0, listStyle: "none", fontSize: 11 }}>
              {dropped.map((item) => {
                const phrase = rolePhrase(item.role);
                const placed =
                  item.role.kind === "emu68-archive" || item.role.kind === "kickstart";
                return (
                  <li key={item.path} style={{ padding: "2px 0" }}>
                    <span
                      style={{
                        color: placed ? "var(--ok-text)" : "var(--text-faint)",
                        fontWeight: 700,
                        marginRight: 8,
                      }}
                    >
                      {placed ? "→" : "·"}
                    </span>
                    <code>{item.name}</code>{" "}
                    <span className={placed ? "muted" : "faint"}>
                      {t(phrase.key, phrase.params)}
                    </span>
                    {item.rom && (
                      <span className="faint"> — {item.rom.name}</span>
                    )}
                  </li>
                );
              })}
            </ul>
          </div>
        )}

        <Field
          label={t("cardBuilder.archive.label")}
          value={archive}
          empty={t("cardBuilder.archive.none")}
          onChoose={() => void chooseArchive()}
          choose={t("common.browse")}
          hint={t("cardBuilder.archive.hint")}
        />
        <Field
          label={t("cardBuilder.kickstart.label")}
          value={kickstart}
          empty={t("cardBuilder.kickstart.none")}
          onChoose={() => void chooseKickstart()}
          choose={t("common.browse")}
          hint={t("cardBuilder.kickstart.hint")}
          onClear={kickstart ? () => setKickstart(null) : undefined}
          clear={t("common.clear")}
        />

        <label style={{ display: "flex", flexDirection: "column", gap: 4, marginBottom: 12 }}>
          <span className="muted" style={{ fontSize: 12 }}>
            {t("cardBuilder.size.label")}
          </span>
          <select
            className="btn"
            value={cardGb}
            onChange={(e) => setCardGb(Number(e.target.value))}
            style={{ maxWidth: "12em" }}
          >
            {CARD_SIZES_GB.map((size) => (
              <option key={size} value={size}>
                {size} GB
              </option>
            ))}
          </select>
        </label>

        <Field
          label={t("cardBuilder.dest.label")}
          value={dest}
          empty={t("cardBuilder.dest.none")}
          onChoose={() => void chooseDest()}
          choose={t("common.browse")}
          hint={t("cardBuilder.dest.hint")}
        />

        {/* Everything with a defaulted answer. Hidden in Beginner mode, which
            hides and never disables (§47, §48). */}
        {powerMode && (
          <details
            open={advanced}
            onToggle={(e) => setAdvanced((e.target as HTMLDetailsElement).open)}
            style={{ marginTop: 12 }}
          >
            <summary style={{ cursor: "pointer", fontSize: 13 }}>
              {t("cardBuilder.advanced.heading")}
            </summary>

            <div style={{ display: "grid", gap: 10, marginTop: 10 }}>
              <div style={{ display: "flex", gap: 8, flexWrap: "wrap" }}>
                <SelectField
                  label={t("cardBuilder.advanced.amiga")}
                  value={hardware.amiga}
                  options={AMIGA_TARGETS}
                  onChange={(value) => applyHardware({ amiga: value })}
                />
                <SelectField
                  label={t("cardBuilder.advanced.variant")}
                  value={hardware.variant}
                  options={VARIANTS}
                  onChange={(value) => applyHardware({ variant: value })}
                />
                <SelectField
                  label={t("cardBuilder.advanced.pi")}
                  value={hardware.pi}
                  options={PI_MODELS}
                  onChange={(value) => applyHardware({ pi: value })}
                />
                <SelectField
                  label={t("cardBuilder.advanced.line")}
                  value={line}
                  options={LINES}
                  onChange={setLine}
                />
              </div>

              <p className="faint" style={{ fontSize: 11, margin: 0 }}>
                {t("cardBuilder.advanced.sharedWithPistorm")}
              </p>

              <div style={{ display: "flex", gap: 8, flexWrap: "wrap" }}>
                <TextField
                  label={t("cardBuilder.advanced.label")}
                  value={label}
                  onChange={setLabel}
                />
                <NumberField
                  label={t("cardBuilder.advanced.bootMib")}
                  value={bootMib}
                  onChange={setBootMib}
                  hint={t("cardBuilder.advanced.bootMibDefault")}
                />
              </div>

              <div style={{ display: "flex", gap: 8, flexWrap: "wrap", alignItems: "end" }}>
                <TextField
                  label={t("cardBuilder.advanced.driveName")}
                  value={driveName}
                  onChange={setDriveName}
                />
                <NumberField
                  label={t("cardBuilder.advanced.partitionMb")}
                  value={partitionMb}
                  onChange={setPartitionMb}
                />
                <label style={{ display: "flex", flexDirection: "column", gap: 4 }}>
                  <span className="muted" style={{ fontSize: 12 }}>
                    {t("cardBuilder.advanced.fsType")}
                  </span>
                  <select
                    className="btn"
                    value={fsType}
                    onChange={(e) => setFsType(e.target.value as AmigaHardDiskFs)}
                  >
                    {fsChoices.map((choice) => (
                      <option
                        key={choice.value}
                        value={choice.value}
                        disabled={choice.blocked !== null}
                      >
                        {choice.label}
                        {choice.blocked ? ` — ${t(choice.blocked.key, choice.blocked.params)}` : ""}
                      </option>
                    ))}
                  </select>
                </label>
              </div>

              {/* ART ships no `pfs3aio` and never will, the same rule as the
                  Kickstart above. Without one, PFS3 is shown and not
                  selectable rather than hidden — "why can I not have PFS3"
                  is a better question to be able to answer than to prevent
                  somebody asking. */}
              <label
                style={{ display: "flex", gap: 8, alignItems: "flex-start", fontSize: 12 }}
              >
                <input
                  type="checkbox"
                  checked={workPartition}
                  onChange={(e) => setWorkPartition(e.target.checked)}
                />
                <span>
                  {t("cardBuilder.advanced.workPartition")}
                  <span className="faint" style={{ display: "block", fontSize: 11 }}>
                    {t("cardBuilder.advanced.workPartitionHint")}
                  </span>
                </span>
              </label>

              <Field
                label={t("cardBuilder.advanced.fsDriver")}
                value={fsDriver}
                empty={t("preload.driver.none")}
                onChoose={() => void chooseFsDriver()}
                choose={t("common.browse")}
                hint={t("cardBuilder.advanced.fsDriverHint")}
                onClear={fsDriver ? () => setFsDriver(null) : undefined}
                clear={t("common.clear")}
              />

              <p className="faint" style={{ fontSize: 11, margin: 0 }}>
                {t("cardBuilder.advanced.oneDisk")}
              </p>
            </div>
          </details>
        )}

        <div style={{ display: "flex", gap: 8, marginTop: 16, alignItems: "center" }}>
          <button className="btn" onClick={() => void preview()} disabled={busy || !archive}>
            {t("cardBuilder.actions.preview")}
          </button>
          <button
            className="btn btn-primary"
            onClick={() => void build()}
            disabled={busy || blocker !== null}
            title={blocker ? t(blocker.key, blocker.params) : undefined}
          >
            {t("cardBuilder.actions.build")}
          </button>
          {blocker && (
            <span className="muted" style={{ fontSize: 11 }}>
              {t(blocker.key, blocker.params)}
            </span>
          )}
        </div>

        {error && (
          <p className="badge badge-err" style={{ display: "block", marginTop: 12, padding: "6px 12px", fontSize: 12 }}>
            {error}
          </p>
        )}
      </section>

      {plan && (
        <section className="card" style={{ marginBottom: 16 }}>
          <h2 style={{ fontSize: 16, marginTop: 0 }}>{t("cardBuilder.plan.heading")}</h2>

          {plan.warnings.map((warning) => {
            const phrase = warningPhrase(warning);
            return (
              <p
                key={phrase.key}
                className="badge badge-warn"
                style={{ display: "block", padding: "6px 12px", fontSize: 11, marginBottom: 8 }}
              >
                {t(phrase.key, phrase.params)}
              </p>
            );
          })}

          <table style={{ fontSize: 12, borderCollapse: "collapse", marginBottom: 12 }}>
            <tbody>
              <Row
                name={t("cardBuilder.plan.bootPartition")}
                value={t("cardBuilder.plan.atGb", {
                  size: gb(plan.layout.boot.sector_count * 512),
                  at: plan.layout.boot.start_lba * 512,
                })}
              />
              {plan.layout.areas.map((area, index) => (
                <Row
                  key={area.index}
                  name={t("cardBuilder.plan.amigaDisk", { n: index + 1 })}
                  value={t("cardBuilder.plan.atGb", {
                    size: gb(area.sector_count * 512),
                    at: area.start_lba * 512,
                  })}
                />
              ))}
              <Row name={t("cardBuilder.plan.kernel")} value={plan.kernel_file} />
              {plan.kickstart_file && (
                <Row
                  name={t("cardBuilder.plan.kickstartAs")}
                  value={plan.kickstart_file}
                />
              )}
              {plan.rom && (
                <Row
                  name={t("cardBuilder.plan.rom")}
                  value={`${plan.rom.name} (${plan.rom.revision})`}
                />
              )}
              <Row
                name={t("cardBuilder.plan.files")}
                value={t("cardBuilder.plan.filesTotal", {
                  count: plan.boot_files.length,
                  mib: mib(payloadBytes(plan.boot_files)),
                })}
              />
            </tbody>
          </table>

          <details>
            <summary style={{ cursor: "pointer", fontSize: 12 }}>
              {t("cardBuilder.plan.fileList")}
            </summary>
            <ul className="faint" style={{ fontSize: 11, margin: "8px 0 0", paddingLeft: 18 }}>
              {plan.boot_files.map((file) => (
                <li key={file.name}>
                  <code>{file.name}</code> — {mib(file.bytes)} MB
                </li>
              ))}
            </ul>
          </details>
        </section>
      )}

      {result && (
        <section className="card" style={{ marginBottom: 16 }}>
          <h2 style={{ fontSize: 16, marginTop: 0 }}>{t("cardBuilder.result.heading")}</h2>
          <p style={{ fontSize: 12, margin: "4px 0 8px", wordBreak: "break-all" }}>
            <code>{result.dest}</code>
          </p>
          {/* The card as ART's own reader sees it, not as the builder claims
              it to be. A build that cannot be read is not a build. */}
          <p className="muted" style={{ fontSize: 12, margin: 0 }}>
            {t("cardBuilder.result.readBack", {
              disks: result.verified.card.areas.length,
              partitions: result.verified.card.areas.reduce(
                (total, area) => total + area.rdb.partitions.length,
                0
              ),
            })}
          </p>
          <p className="faint" style={{ fontSize: 11, margin: "8px 0 0" }}>
            {t("cardBuilder.result.nextStep")}
          </p>
        </section>
      )}

      {/* G8, the last gate before the file is handed over — its own section
          and **always here**, because the command has always taken any card.
          One built a moment ago is only the commonest case; a card from last
          week is the same question, and one ART did not build simply answers
          less. G7's manifest comparison is a part of this report rather than a
          second button. */}
      <section className="card" style={{ marginBottom: 16 }}>
        <h2 style={{ fontSize: 16, marginTop: 0 }}>{t("cardBuilder.health.heading")}</h2>
        {result && (
          <p className="faint" style={{ fontSize: 11, margin: "4px 0 8px", wordBreak: "break-all" }}>
            {t("cardBuilder.health.manifestPath")} <code>{result.manifest_path}</code>
          </p>
        )}
        <div style={{ display: "flex", gap: 8, flexWrap: "wrap", marginTop: 8 }}>
          {result && (
            <button className="btn" onClick={() => void verify()} disabled={checking}>
              {t(checking ? "cardBuilder.health.running" : "cardBuilder.health.run")}
            </button>
          )}
          <button className="btn" onClick={() => void chooseCardToCheck()} disabled={checking}>
            {t(checking ? "cardBuilder.health.running" : "cardBuilder.health.chooseOther")}
          </button>
        </div>

        {report && (
          <>
            {checkedPath && (
              <p className="faint" style={{ fontSize: 11, margin: "10px 0 0", wordBreak: "break-all" }}>
                <code>{checkedPath}</code>
              </p>
            )}
            <HealthPanel report={report} />
          </>
        )}
      </section>
    </>
  );
}

/**
 * The checklist (§50), in three parts that must not look like each other:
 * what passed, what failed, and **what ART could not answer** — plus the steps
 * only the person at the machine can take. A tick that means "ART did not
 * look" reading like one that means "ART looked and it is right" is the claim
 * §89 forbids, so the third state gets its own mark and its own colour.
 */
function HealthPanel({ report }: { report: HealthReport }) {
  const { t } = useTranslation();
  const verdict = healthVerdict(report);
  const failed = report.items.some((item) => item.state === "fail");

  const mark: Record<HealthReport["items"][number]["state"], string> = {
    pass: "✓",
    fail: "✕",
    "not-checked": "–",
  };
  const colour: Record<HealthReport["items"][number]["state"], string> = {
    pass: "var(--ok-text)",
    fail: "var(--err-text)",
    "not-checked": "var(--text-faint)",
  };

  return (
    <div style={{ marginTop: 10 }}>
      <p
        className={`badge ${failed ? "badge-err" : "badge-ok"}`}
        style={{ display: "block", padding: "6px 12px", fontSize: 11, margin: 0 }}
      >
        {t(verdict.key, verdict.params)}
      </p>

      <ul style={{ fontSize: 11, margin: "10px 0 0", paddingLeft: 0, listStyle: "none" }}>
        {report.items.map((item, index) => {
          const phrase = healthCheckPhrase(item.check);
          return (
            <li key={index} style={{ padding: "2px 0" }}>
              <span
                style={{ color: colour[item.state], fontWeight: 700, marginRight: 8 }}
                title={t(`cardBuilder.health.state.${item.state === "not-checked" ? "notChecked" : item.state}`)}
              >
                {mark[item.state]}
              </span>
              <span className={item.state === "not-checked" ? "faint" : "muted"}>
                {t(phrase.key, phrase.params)}
              </span>
              {/* When the manifest disagrees, *what* disagrees — "one finding"
                  is not something anybody can act on. */}
              {item.check.kind === "manifest-agrees" && item.check.findings.length > 0 && (
                <ul className="faint" style={{ margin: "4px 0 0", paddingLeft: 24 }}>
                  {item.check.findings.map((finding, at) => {
                    const detail = findingPhrase(finding);
                    return <li key={at}>{t(detail.key, detail.params)}</li>;
                  })}
                </ul>
              )}
            </li>
          );
        })}
      </ul>

      <h4 style={{ fontSize: 12, margin: "14px 0 4px" }}>
        {t("cardBuilder.health.byHandHeading")}
      </h4>
      <ul className="muted" style={{ fontSize: 11, margin: 0, paddingLeft: 18 }}>
        {report.by_hand.map((step, index) => {
          const phrase = manualStepPhrase(step);
          return <li key={index}>{t(phrase.key, phrase.params)}</li>;
        })}
      </ul>
    </div>
  );
}

function Row({ name, value }: { name: string; value: string }) {
  return (
    <tr>
      <td className="muted" style={{ padding: "2px 12px 2px 0" }}>
        {name}
      </td>
      <td style={{ padding: "2px 0" }}>{value}</td>
    </tr>
  );
}

function SelectField<T extends string>({
  label,
  value,
  options,
  onChange,
}: {
  label: string;
  value: T;
  options: readonly T[];
  onChange: (value: T) => void;
}) {
  return (
    <label style={{ display: "flex", flexDirection: "column", gap: 4 }}>
      <span className="muted" style={{ fontSize: 12 }}>
        {label}
      </span>
      <select className="btn" value={value} onChange={(e) => onChange(e.target.value as T)}>
        {options.map((option) => (
          <option key={option} value={option}>
            {option}
          </option>
        ))}
      </select>
    </label>
  );
}

function TextField({
  label,
  value,
  onChange,
}: {
  label: string;
  value: string;
  onChange: (value: string) => void;
}) {
  return (
    <label style={{ display: "flex", flexDirection: "column", gap: 4 }}>
      <span className="muted" style={{ fontSize: 12 }}>
        {label}
      </span>
      <input
        className="input"
        value={value}
        onChange={(e) => onChange(e.target.value)}
        style={{ maxWidth: "12em" }}
      />
    </label>
  );
}

function NumberField({
  label,
  value,
  onChange,
  hint,
}: {
  label: string;
  value: number;
  onChange: (value: number) => void;
  hint?: string;
}) {
  return (
    <label style={{ display: "flex", flexDirection: "column", gap: 4 }}>
      <span className="muted" style={{ fontSize: 12 }}>
        {label}
      </span>
      <input
        className="input"
        type="number"
        value={value}
        onChange={(e) => onChange(Number(e.target.value))}
        style={{ maxWidth: "10em" }}
      />
      {hint && (
        <span className="faint" style={{ fontSize: 11 }}>
          {hint}
        </span>
      )}
    </label>
  );
}
