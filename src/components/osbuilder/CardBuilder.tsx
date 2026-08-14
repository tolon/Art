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
import { open, save } from "@tauri-apps/plugin-dialog";
import { useTranslation } from "react-i18next";

import {
  buildBlocker,
  cardBuild,
  cardPlanBuild,
  cardVerifyManifest,
  defaultPartition,
  findingPhrase,
  manifestVerdict,
  onCardBuildResult,
  payloadBytes,
  warningPhrase,
  CARD_FS_CHOICES,
  type CardBuildPlan,
  type CardBuildRequest,
  type CardBuildResult,
  type ManifestReport,
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
import { isOneOf, isText, isTextOrNothing, isWholeNumberBetween } from "@/lib/remembered";
import { useRemembered, useRememberedShape } from "@/lib/useRemembered";
import { usePowerMode } from "@/lib/uxmode";

const GIB = 1024 * 1024 * 1024;

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

function gb(bytes: number): string {
  return (Math.round((bytes / GIB) * 100) / 100).toString();
}

function mib(bytes: number): string {
  return (Math.round((bytes / (1024 * 1024)) * 10) / 10).toString();
}

export function CardBuilder() {
  const { t } = useTranslation();
  const powerMode = usePowerMode();

  // --- what the user chose, remembered ------------------------------------
  const [archive, setArchive] = useRemembered<string | null>(
    "cardBuilder.archive",
    isTextOrNothing,
    null
  );
  const [kickstart, setKickstart] = useRemembered<string | null>(
    "cardBuilder.kickstart",
    isTextOrNothing,
    null
  );
  const [dest, setDest] = useRemembered<string | null>(
    "cardBuilder.dest",
    isTextOrNothing,
    null
  );
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
  const [fsType, setFsType] = useRemembered<AmigaHardDiskFs>(
    "cardBuilder.fsType",
    isOneOf<AmigaHardDiskFs>(...CARD_FS_CHOICES.map((choice) => choice.value)),
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
  const [report, setReport] = useState<ManifestReport | null>(null);
  const [checking, setChecking] = useState(false);

  const partitions: PartitionSpec[] = [
    {
      drive_name: driveName,
      fs_type: fsType,
      size_mb: partitionMb,
      bootable: true,
      boot_priority: 0,
    },
  ];

  const request: CardBuildRequest = {
    archive: archive ?? "",
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

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    void onCardBuildResult((built) => {
      setResult(built);
      setBusy(false);
      // A report describes the card it was run against, and this is a new
      // one. It goes with the old result rather than sitting under a new card.
      setReport(null);
      // The card is there now; the plan that described it no longer applies.
      setPlan(null);
    }).then((fn) => {
      unlisten = fn;
    });
    return () => unlisten?.();
  }, []);

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
      setError(String(e));
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
      setError(String(e));
      setBusy(false);
    }
  }

  async function verify() {
    if (!result) return;
    setChecking(true);
    setError(null);
    try {
      setReport(await cardVerifyManifest(result.dest, result.manifest_path));
    } catch (e) {
      setReport(null);
      setError(String(e));
    } finally {
      setChecking(false);
    }
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
                    {CARD_FS_CHOICES.map((choice) => (
                      <option key={choice.value} value={choice.value}>
                        {choice.label}
                      </option>
                    ))}
                  </select>
                </label>
              </div>

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

          {/* G7. The manifest is the record of what this card was built from,
              and the button is §92's VERIFY made available after the fact —
              a card can be checked against it any time, not only now. */}
          <div style={{ marginTop: 16, borderTop: "1px solid var(--border)", paddingTop: 12 }}>
            <h3 style={{ fontSize: 14, margin: "0 0 4px" }}>
              {t("cardBuilder.manifest.heading")}
            </h3>
            <p className="faint" style={{ fontSize: 11, margin: "0 0 8px", wordBreak: "break-all" }}>
              {t("cardBuilder.manifest.path")} <code>{result.manifest_path}</code>
            </p>
            <button className="btn" onClick={() => void verify()} disabled={checking}>
              {t(checking ? "cardBuilder.manifest.verifying" : "cardBuilder.manifest.verify")}
            </button>

            {report && (
              <div style={{ marginTop: 10 }}>
                <p
                  className={`badge ${report.findings.length === 0 ? "badge-ok" : "badge-warn"}`}
                  style={{ display: "block", padding: "6px 12px", fontSize: 11, margin: 0 }}
                >
                  {t(manifestVerdict(report).key, manifestVerdict(report).params)}
                </p>
                <ul className="muted" style={{ fontSize: 11, margin: "8px 0 0", paddingLeft: 18 }}>
                  {report.findings.map((finding, index) => {
                    const phrase = findingPhrase(finding);
                    return <li key={index}>{t(phrase.key, phrase.params)}</li>;
                  })}
                </ul>
              </div>
            )}
          </div>
        </section>
      )}
    </>
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

function Field({
  label,
  value,
  empty,
  choose,
  onChoose,
  hint,
  clear,
  onClear,
}: {
  label: string;
  value: string | null;
  empty: string;
  choose: string;
  onChoose: () => void;
  hint?: string;
  clear?: string;
  onClear?: () => void;
}) {
  return (
    <div style={{ marginBottom: 12 }}>
      <div className="muted" style={{ fontSize: 12, marginBottom: 4 }}>
        {label}
      </div>
      <div style={{ display: "flex", gap: 8, alignItems: "center", flexWrap: "wrap" }}>
        <button className="btn" onClick={onChoose}>
          {choose}
        </button>
        <span style={{ fontSize: 12, wordBreak: "break-all" }}>{value ?? empty}</span>
        {onClear && (
          <button className="btn" onClick={onClear}>
            {clear}
          </button>
        )}
      </div>
      {hint && (
        <p className="faint" style={{ fontSize: 11, margin: "4px 0 0" }}>
          {hint}
        </p>
      )}
    </div>
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
