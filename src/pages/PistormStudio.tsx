// PiStorm Studio (spec §40, brief `ART-brief-pistorm-studio-v2.md`).
//
// **Every control on this screen writes a documented Emu68 token or a
// Raspberry Pi firmware key.** Nothing else is a control. That rule is the
// whole of ART-090's fix: the screen this replaces offered a JIT switch (Emu68
// *is* a JIT and cannot be turned off), an MMU switch (Emu68 emulates no MMU —
// WHDLoad runs NOMMU), a Fast RAM slider (Emu68 maps RAM itself), and profile
// cards claiming "99 % WHDLoad compatibility" and "~800+ MIPS". It wrote
// `emu68.jit`, `emu68.mmu` and `buptest.fastram_size`, three tokens Emu68 has
// never read.
//
// Things worth telling a user that are *not* tokens are prose, in the notes
// panel — never a control that appears to do something.
//
// The hardware matrix comes from Rust (`pistorm_hardware_matrix`), which is
// where it is tested. Three fields filtering each other, because what a setup
// can do is a function of all three: the kernel build follows the board, the
// storage device name follows the Pi, and which tokens even apply follows the
// Amiga.

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { useTranslation } from "react-i18next";

import {
  pistormHardwareMatrix,
  pistormHardwareNotes,
  pistormPreview,
  pistormProfile,
  pistormSave,
  pistormScan,
  pistormTokens,
  DEFAULT_EMU68_OPTIONS,
  DEFAULT_FIRMWARE_CONFIG,
  DEFAULT_HARDWARE,
  type AmigaChoice,
  type DisplayMode,
  type Emu68Options,
  type Emu68Profile,
  type FirmwareConfig,
  type HardwareNote,
  type PiChoice,
  type PistormCard,
  type PistormHardware,
  type PistormPreview,
  type VariantChoice,
} from "@/lib/pistorm";
import { usePowerMode } from "@/lib/uxmode";
import { isTextOrNothing } from "@/lib/remembered";
import { useRemembered, useRememberedShape } from "@/lib/useRemembered";
import {
  EMU68_OPTION_SPEC,
  FIRMWARE_SPEC,
  HARDWARE_SPEC,
  visibleOptionGroups,
  type OptionGroup,
} from "@/lib/pistormOptions";

const PROFILES: Emu68Profile[] = ["performance", "daily", "compatibility", "diagnostics"];

const DISPLAY_MODES: Array<{ id: string; mode: DisplayMode; tokens: string }> = [
  { id: "auto", mode: "auto", tokens: "—" },
  { id: "dmt1080p60", mode: "dmt1080p60", tokens: "hdmi_group=2 hdmi_mode=82" },
  { id: "cea1080p50", mode: "cea1080p50", tokens: "hdmi_group=1 hdmi_mode=31" },
  { id: "cea720p60", mode: "cea720p60", tokens: "hdmi_group=1 hdmi_mode=4" },
];

function displayModeId(mode: DisplayMode): string {
  return typeof mode === "string" ? mode : "custom";
}

export function PistormStudio() {
  const { t } = useTranslation();
  const powerMode = usePowerMode();

  // --- what the user has chosen, remembered across sessions ---------------
  const [hardware, applyHardware] = useRememberedShape<PistormHardware>(
    "pistorm.hardware",
    HARDWARE_SPEC,
    DEFAULT_HARDWARE
  );
  const [options, applyOptions] = useRememberedShape<Emu68Options>(
    "pistorm.options",
    EMU68_OPTION_SPEC,
    DEFAULT_EMU68_OPTIONS
  );
  const [firmware, applyFirmware] = useRememberedShape<FirmwareConfig>(
    "pistorm.firmware",
    FIRMWARE_SPEC,
    DEFAULT_FIRMWARE_CONFIG
  );
  const [cardPath, setCardPath] = useRemembered<string | null>(
    "pistorm.cardPath",
    isTextOrNothing,
    null
  );

  // --- what the screen is doing -------------------------------------------
  const [matrix, setMatrix] = useState<AmigaChoice[]>([]);
  const [notes, setNotes] = useState<HardwareNote[]>([]);
  const [tokens, setTokens] = useState<string[]>([]);
  const [card, setCard] = useState<PistormCard | null>(null);
  const [preview, setPreview] = useState<PistormPreview | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [statusMsg, setStatusMsg] = useState<string | null>(null);

  useEffect(() => {
    pistormHardwareMatrix().then(setMatrix).catch((e) => setError(String(e)));
  }, []);

  // The notes and the token line both follow the hardware, and both come from
  // Rust — so that what the screen shows is what will be written, not a second
  // implementation of the same rules.
  useEffect(() => {
    pistormHardwareNotes(hardware).then(setNotes).catch(() => setNotes([]));
  }, [hardware]);

  useEffect(() => {
    pistormTokens(options, hardware).then(setTokens).catch(() => setTokens([]));
  }, [options, hardware]);

  const amigaChoice = matrix.find((entry) => entry.amiga === hardware.amiga);
  const variantChoice: VariantChoice | undefined = amigaChoice?.variants.find(
    (entry) => entry.variant === hardware.variant
  );
  const piChoice: PiChoice | undefined = variantChoice?.pi_models.find(
    (entry) => entry.model === hardware.pi
  );

  // Changing one field can leave the ones below it naming something that does
  // not exist — an A1200 with a PiStorm600. Each falls back to the first
  // choice its predecessor allows, which is what the dropdown would show anyway.
  function chooseAmiga(next: AmigaChoice) {
    const variant = next.variants[0];
    applyHardware({
      amiga: next.amiga,
      variant: variant.variant,
      pi: variant.pi_models[0].model,
    });
  }

  function chooseVariant(next: VariantChoice) {
    applyHardware({ variant: next.variant, pi: next.pi_models[0].model });
  }

  const scan = useCallback(
    async (path: string, announce: boolean) => {
      setBusy(true);
      setError(null);
      try {
        const found = await pistormScan(path, hardware);
        setCard(found);
        setCardPath(path);
        // What is on the card wins over what ART remembered: the card is the
        // thing being edited, and showing anything else would be editing a
        // copy of somebody's settings rather than their settings.
        if (found.has_cmdline_txt) applyOptions(found.setup.options);
        if (found.has_config_txt) applyFirmware(found.setup.firmware);
        setStatusMsg(
          found.is_pistorm_card
            ? t("pistorm.scan.found", { path })
            : t("pistorm.scan.notACard", { path })
        );
      } catch (e) {
        // A folder reopened from last time may simply not be mounted, which is
        // the normal state of a card reader with no card in it.
        if (announce) setError(String(e));
      } finally {
        setBusy(false);
      }
    },
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [hardware, t]
  );

  const reopened = useRef<string | null>(null);
  useEffect(() => {
    if (!cardPath || reopened.current === cardPath) return;
    reopened.current = cardPath;
    void scan(cardPath, false);
  }, [cardPath, scan]);

  async function chooseCard() {
    const picked = await open({
      directory: true,
      multiple: false,
      title: t("pistorm.chooseCardTitle"),
    });
    if (typeof picked !== "string") return;
    await scan(picked, true);
  }

  async function applyProfile(profile: Emu68Profile) {
    setError(null);
    try {
      const { options: next } = await pistormProfile(profile, hardware);
      applyOptions(next);
      setStatusMsg(t("pistorm.profile.applied", { name: t(`pistorm.profile.${profile}.title`) }));
    } catch (e) {
      setError(String(e));
    }
  }

  async function showPreview() {
    if (!cardPath) return;
    setBusy(true);
    setError(null);
    try {
      setPreview(await pistormPreview(cardPath, { hardware, options, firmware }));
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function save() {
    if (!cardPath) return;
    setBusy(true);
    setError(null);
    try {
      const outcome = await pistormSave(cardPath, { hardware, options, firmware });
      setPreview(null);
      setStatusMsg(
        outcome.cmdline_txt_backup
          ? t("pistorm.saved.withBackup", { path: outcome.cmdline_txt_backup })
          : t("pistorm.saved.plain")
      );
      await scan(cardPath, true);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  const groups: OptionGroup[] = useMemo(
    () => visibleOptionGroups(hardware, amigaChoice, variantChoice),
    [hardware, amigaChoice, variantChoice]
  );

  return (
    <div>
      <h1 style={{ fontSize: 20 }}>
        {t("nav.pistorm")} — {t("pistorm.title")}
      </h1>
      <p className="muted" style={{ fontSize: 12, margin: "4px 0 16px" }}>
        {t("pistorm.intro")}
      </p>

      {error && (
        <div className="badge badge-err" style={{ margin: "12px 0", padding: "6px 12px" }}>
          {error}
        </div>
      )}
      {statusMsg && !error && (
        <div className="badge badge-ok" style={{ margin: "12px 0", padding: "6px 12px" }}>
          {statusMsg}
        </div>
      )}

      {/* 1 — Hardware. Everything downstream derives from these three. */}
      <section className="card" style={{ marginBottom: 16 }}>
        <h2 style={{ fontSize: 16, marginTop: 0 }}>{t("pistorm.hardware.heading")}</h2>
        <p className="muted" style={{ fontSize: 12, margin: "4px 0 12px" }}>
          {t("pistorm.hardware.intro")}
        </p>

        <div style={{ display: "flex", gap: 12, flexWrap: "wrap" }}>
          <label style={{ display: "flex", flexDirection: "column", gap: 4, flex: "1 1 180px" }}>
            <span className="muted" style={{ fontSize: 12 }}>
              {t("pistorm.hardware.amiga")}
            </span>
            <select
              className="btn"
              value={hardware.amiga}
              onChange={(e) => {
                const next = matrix.find((entry) => entry.amiga === e.target.value);
                if (next) chooseAmiga(next);
              }}
            >
              {matrix.map((entry) => (
                <option key={entry.amiga} value={entry.amiga}>
                  {entry.name}
                </option>
              ))}
            </select>
          </label>

          <label style={{ display: "flex", flexDirection: "column", gap: 4, flex: "1 1 180px" }}>
            <span className="muted" style={{ fontSize: 12 }}>
              {t("pistorm.hardware.variant")}
            </span>
            <select
              className="btn"
              value={hardware.variant}
              onChange={(e) => {
                const next = amigaChoice?.variants.find((v) => v.variant === e.target.value);
                if (next) chooseVariant(next);
              }}
            >
              {(amigaChoice?.variants ?? []).map((entry) => (
                <option key={entry.variant} value={entry.variant}>
                  {entry.name}
                </option>
              ))}
            </select>
          </label>

          <label style={{ display: "flex", flexDirection: "column", gap: 4, flex: "1 1 180px" }}>
            <span className="muted" style={{ fontSize: 12 }}>
              {t("pistorm.hardware.pi")}
            </span>
            <select
              className="btn"
              value={hardware.pi}
              onChange={(e) => applyHardware({ pi: e.target.value as PistormHardware["pi"] })}
            >
              {(variantChoice?.pi_models ?? []).map((entry) => (
                <option key={entry.model} value={entry.model}>
                  {entry.name}
                  {entry.support === "reported" ? " *" : ""}
                </option>
              ))}
            </select>
          </label>
        </div>

        {/* What follows from the three, stated rather than implied. */}
        {variantChoice && piChoice && (
          <dl
            style={{
              display: "grid",
              gridTemplateColumns: "auto 1fr",
              gap: "4px 12px",
              margin: "12px 0 0",
              fontSize: 12,
            }}
          >
            <dt className="muted">{t("pistorm.derived.kernel")}</dt>
            <dd style={{ margin: 0, fontFamily: "monospace" }}>{variantChoice.kernel_archive}</dd>
            <dt className="muted">{t("pistorm.derived.storageDevice")}</dt>
            <dd style={{ margin: 0, fontFamily: "monospace" }}>{piChoice.storage_device}</dd>
            <dt className="muted">{t("pistorm.derived.piRam")}</dt>
            <dd style={{ margin: 0 }}>
              {piChoice.ram_min_mb === piChoice.ram_max_mb
                ? t("pistorm.derived.ramFixed", { mb: piChoice.ram_min_mb })
                : t("pistorm.derived.ramRange", {
                    min: piChoice.ram_min_mb,
                    max: piChoice.ram_max_mb,
                  })}
            </dd>
          </dl>
        )}

        {notes.length > 0 && (
          <ul className="muted" style={{ fontSize: 11, margin: "12px 0 0", paddingLeft: 18 }}>
            {notes.map((note) => (
              <li key={note}>{t(`pistorm.note.${note}`)}</li>
            ))}
          </ul>
        )}
      </section>

      {/* 2 — The card, and the Kickstart on it. */}
      <section className="card" style={{ marginBottom: 16 }}>
        <h2 style={{ fontSize: 16, marginTop: 0 }}>{t("pistorm.card.heading")}</h2>
        <div style={{ display: "flex", gap: 8, alignItems: "center", flexWrap: "wrap" }}>
          <button className="btn btn-primary" onClick={() => void chooseCard()} disabled={busy}>
            {t("pistorm.card.choose")}
          </button>
          <span style={{ fontSize: 12, wordBreak: "break-all" }}>
            {cardPath ?? t("pistorm.card.none")}
          </span>
        </div>

        {card && (
          <ul className="muted" style={{ fontSize: 12, margin: "10px 0 0", paddingLeft: 18 }}>
            <li>
              {card.has_kernel ? t("pistorm.card.kernelFound") : t("pistorm.card.kernelMissing")}
            </li>
            <li>
              {card.kickstart_files.length > 0
                ? t("pistorm.card.kickstartsFound", { list: card.kickstart_files.join(", ") })
                : t("pistorm.card.kickstartsMissing")}
            </li>
            {card.config_sets.length > 0 && (
              <li>{t("pistorm.card.configSets", { list: card.config_sets.join(", ") })}</li>
            )}
          </ul>
        )}

        <label style={{ display: "flex", flexDirection: "column", gap: 4, marginTop: 12 }}>
          <span className="muted" style={{ fontSize: 12 }}>
            {t("pistorm.kickstart.label")}
          </span>
          {card && card.kickstart_files.length > 0 && !powerMode ? (
            <select
              className="btn"
              value={firmware.kickstart_file}
              onChange={(e) => applyFirmware({ kickstart_file: e.target.value })}
            >
              {card.kickstart_files.map((name) => (
                <option key={name} value={name}>
                  {name}
                </option>
              ))}
            </select>
          ) : (
            <input
              className="btn"
              value={firmware.kickstart_file}
              onChange={(e) => applyFirmware({ kickstart_file: e.target.value })}
              style={{ fontFamily: "monospace" }}
            />
          )}
          <span className="faint" style={{ fontSize: 11 }}>
            {t("pistorm.kickstart.hint")}
          </span>
        </label>
      </section>

      {/* 3 — Profiles. Every card shows the tokens it stands for. */}
      <section className="card" style={{ marginBottom: 16 }}>
        <h2 style={{ fontSize: 16, marginTop: 0 }}>{t("pistorm.profiles.heading")}</h2>
        <p className="muted" style={{ fontSize: 12, margin: "4px 0 12px" }}>
          {t("pistorm.profiles.intro")}
        </p>
        <div style={{ display: "flex", flexDirection: "column", gap: 8 }}>
          {PROFILES.filter((profile) => profile !== "diagnostics" || powerMode).map((profile) => (
            <ProfileCard
              key={profile}
              profile={profile}
              hardware={hardware}
              onApply={() => void applyProfile(profile)}
            />
          ))}
        </div>
      </section>

      {/* 4 — Display and clock. */}
      <section className="card" style={{ marginBottom: 16 }}>
        <h2 style={{ fontSize: 16, marginTop: 0 }}>{t("pistorm.display.heading")}</h2>
        <div style={{ display: "flex", flexDirection: "column", gap: 6 }}>
          {DISPLAY_MODES.map((entry) => (
            <label
              key={entry.id}
              style={{ display: "flex", gap: 8, alignItems: "baseline", fontSize: 13 }}
            >
              <input
                type="radio"
                name="pistorm-display"
                checked={displayModeId(firmware.display) === entry.id}
                onChange={() => applyFirmware({ display: entry.mode })}
              />
              <span style={{ flex: "0 0 auto" }}>{t(`pistorm.display.${entry.id}`)}</span>
              <code className="faint" style={{ fontSize: 11 }}>
                {entry.tokens}
              </code>
            </label>
          ))}
        </div>

        <OverclockField
          value={firmware.overclock}
          onChange={(overclock) => applyFirmware({ overclock })}
        />

        <label
          style={{ display: "flex", gap: 6, alignItems: "center", fontSize: 13, marginTop: 12 }}
        >
          <input
            type="checkbox"
            checked={firmware.disable_bluetooth}
            onChange={(e) => applyFirmware({ disable_bluetooth: e.target.checked })}
          />
          {t("pistorm.display.disableBluetooth")}
          <code className="faint" style={{ fontSize: 11 }}>
            dtoverlay=disable-bt
          </code>
        </label>
      </section>

      {/* 5 — The full inventory, for anyone who wants it. */}
      {powerMode && (
        <section className="card" style={{ marginBottom: 16 }}>
          <h2 style={{ fontSize: 16, marginTop: 0 }}>{t("pistorm.advanced.heading")}</h2>
          <p className="muted" style={{ fontSize: 12, margin: "4px 0 12px" }}>
            {t("pistorm.advanced.intro")}
          </p>
          {groups.map((group) => (
            <div key={group.id} style={{ marginBottom: 14 }}>
              <h3 style={{ fontSize: 13, margin: "0 0 6px" }}>{t(`pistorm.group.${group.id}`)}</h3>
              <div style={{ display: "flex", flexDirection: "column", gap: 6 }}>
                {group.fields.map((field) => (
                  <OptionField
                    key={field.key}
                    field={field}
                    options={options}
                    storagePrefix={piChoice?.storage_device === "brcm-emmc.device" ? "emmc" : "sd"}
                    onChange={applyOptions}
                  />
                ))}
              </div>
            </div>
          ))}
        </section>
      )}

      {/* What the line will say — the honesty mechanism for everything above. */}
      <section className="card" style={{ marginBottom: 16 }}>
        <h2 style={{ fontSize: 16, marginTop: 0 }}>{t("pistorm.tokens.heading")}</h2>
        <pre
          style={{
            fontSize: 12,
            margin: 0,
            padding: "8px 10px",
            background: "var(--bg)",
            border: "1px solid var(--border)",
            borderRadius: 4,
            whiteSpace: "pre-wrap",
            wordBreak: "break-all",
          }}
        >
          {tokens.length > 0 ? tokens.join(" ") : t("pistorm.tokens.none")}
        </pre>
        {card && card.unmanaged_cmdline.length > 0 && (
          <>
            <h3 style={{ fontSize: 13, margin: "12px 0 4px" }}>
              {t("pistorm.tokens.yours")}
            </h3>
            <p className="faint" style={{ fontSize: 11, margin: "0 0 6px" }}>
              {t("pistorm.tokens.yoursHint")}
            </p>
            <pre
              className="muted"
              style={{
                fontSize: 12,
                margin: 0,
                padding: "8px 10px",
                background: "var(--bg)",
                border: "1px solid var(--border)",
                borderRadius: 4,
                whiteSpace: "pre-wrap",
                wordBreak: "break-all",
              }}
            >
              {card.unmanaged_cmdline.join(" ")}
            </pre>
          </>
        )}

        <div style={{ display: "flex", gap: 8, marginTop: 12 }}>
          <button className="btn" onClick={() => void showPreview()} disabled={!cardPath || busy}>
            {t("pistorm.preview.button")}
          </button>
          <button
            className="btn btn-primary"
            onClick={() => void save()}
            disabled={!cardPath || busy}
          >
            {t("pistorm.save.button")}
          </button>
        </div>
      </section>

      {preview && (
        <PreviewDialog
          preview={preview}
          onClose={() => setPreview(null)}
          onConfirm={() => void save()}
        />
      )}

      {/* 6 & 7 — declared, not pretended (spec §96). */}
      <section className="card" style={{ marginBottom: 16, opacity: 0.75 }}>
        <h2 style={{ fontSize: 16, marginTop: 0 }}>
          {t("pistorm.wifi.heading")}{" "}
          <span className="badge badge-muted" style={{ fontSize: 10 }}>
            {t("common.comingLater")}
          </span>
        </h2>
        <p className="muted" style={{ fontSize: 12, margin: "4px 0 0" }}>
          {t("pistorm.wifi.explain")}
        </p>
      </section>
    </div>
  );
}

/**
 * One profile, with the tokens it means.
 *
 * The tokens are fetched rather than restated, so the card cannot drift from
 * what applying it would actually write — which is precisely how the previous
 * cards came to promise MIPS figures nobody measured.
 */
function ProfileCard({
  profile,
  hardware,
  onApply,
}: {
  profile: Emu68Profile;
  hardware: PistormHardware;
  onApply: () => void;
}) {
  const { t } = useTranslation();
  const [tokens, setTokens] = useState<string[]>([]);

  useEffect(() => {
    pistormProfile(profile, hardware)
      .then((preview) => setTokens(preview.tokens))
      .catch(() => setTokens([]));
  }, [profile, hardware]);

  return (
    <div
      style={{
        padding: "10px 12px",
        borderRadius: 4,
        border: "1px solid var(--border)",
        background: "var(--bg)",
      }}
    >
      <div style={{ display: "flex", justifyContent: "space-between", gap: 8 }}>
        <strong style={{ fontSize: 13 }}>{t(`pistorm.profile.${profile}.title`)}</strong>
        <button className="btn btn-sm" onClick={onApply}>
          {t("pistorm.profile.apply")}
        </button>
      </div>
      <p className="muted" style={{ margin: "4px 0 6px", fontSize: 11 }}>
        {t(`pistorm.profile.${profile}.description`)}
      </p>
      <code className="faint" style={{ fontSize: 11, wordBreak: "break-all" }}>
        {tokens.join(" ")}
      </code>
    </div>
  );
}

/**
 * The overclock, off unless somebody turns it on.
 *
 * Never in a profile and never a default: it is heat, it is the quality of the
 * user's power supply, and on a Pi it sets the warranty bit.
 */
function OverclockField({
  value,
  onChange,
}: {
  value: FirmwareConfig["overclock"];
  onChange: (next: FirmwareConfig["overclock"]) => void;
}) {
  const { t } = useTranslation();
  const on = value !== null;

  return (
    <div style={{ marginTop: 14 }}>
      <label style={{ display: "flex", gap: 6, alignItems: "center", fontSize: 13 }}>
        <input
          type="checkbox"
          checked={on}
          onChange={(e) =>
            onChange(
              e.target.checked ? { arm_freq_mhz: 1300, over_voltage: 2, force_turbo: false } : null
            )
          }
        />
        {t("pistorm.overclock.enable")}
      </label>
      <p className="badge badge-warn" style={{ fontSize: 11, margin: "6px 0 0", display: "inline-block" }}>
        {t("pistorm.overclock.warning")}
      </p>
      {on && value && (
        <div style={{ display: "flex", gap: 12, marginTop: 8, flexWrap: "wrap" }}>
          <label style={{ display: "flex", flexDirection: "column", gap: 4, fontSize: 12 }}>
            <span className="muted">arm_freq</span>
            <input
              className="btn"
              type="number"
              min={600}
              max={2400}
              value={value.arm_freq_mhz}
              onChange={(e) =>
                onChange({ ...value, arm_freq_mhz: Number(e.target.value) || value.arm_freq_mhz })
              }
              style={{ width: "7em" }}
            />
          </label>
          <label style={{ display: "flex", flexDirection: "column", gap: 4, fontSize: 12 }}>
            <span className="muted">over_voltage</span>
            <input
              className="btn"
              type="number"
              min={-16}
              max={8}
              value={value.over_voltage}
              onChange={(e) => onChange({ ...value, over_voltage: Number(e.target.value) })}
              style={{ width: "7em" }}
            />
          </label>
          <label style={{ display: "flex", gap: 6, alignItems: "center", fontSize: 12 }}>
            <input
              type="checkbox"
              checked={value.force_turbo}
              onChange={(e) => onChange({ ...value, force_turbo: e.target.checked })}
            />
            force_turbo
          </label>
        </div>
      )}
    </div>
  );
}

/** One row of the full option inventory, labelled with its real token name. */
function OptionField({
  field,
  options,
  storagePrefix,
  onChange,
}: {
  field: OptionGroup["fields"][number];
  options: Emu68Options;
  storagePrefix: string;
  onChange: (change: Partial<Emu68Options>) => void;
}) {
  const { t } = useTranslation();
  const tokenName = field.token.replace("{prefix}", storagePrefix);
  const value = options[field.key];

  return (
    <label style={{ display: "flex", gap: 8, alignItems: "baseline", fontSize: 13 }}>
      {field.kind === "flag" ? (
        <input
          type="checkbox"
          checked={value === true}
          onChange={(e) => onChange({ [field.key]: e.target.checked } as Partial<Emu68Options>)}
        />
      ) : field.kind === "choice" ? (
        <select
          className="btn btn-sm"
          value={String(value)}
          onChange={(e) => onChange({ [field.key]: e.target.value } as Partial<Emu68Options>)}
        >
          {(field.choices ?? []).map((choice) => (
            <option key={choice} value={choice}>
              {t(`pistorm.choice.${field.key}.${choice}`)}
            </option>
          ))}
        </select>
      ) : (
        <input
          className="btn btn-sm"
          type="number"
          value={value === null || value === undefined ? "" : String(value)}
          placeholder={t("pistorm.advanced.unset")}
          onChange={(e) =>
            onChange({
              [field.key]: e.target.value === "" ? null : Number(e.target.value),
            } as Partial<Emu68Options>)
          }
          style={{ width: "7em" }}
        />
      )}
      <code style={{ fontSize: 11 }}>{tokenName}</code>
      <span className="muted" style={{ fontSize: 11 }}>
        {t(`pistorm.option.${field.key}`)}
      </span>
    </label>
  );
}

/** Both files, before and after — spec §92's preview step. */
function PreviewDialog({
  preview,
  onClose,
  onConfirm,
}: {
  preview: PistormPreview;
  onClose: () => void;
  onConfirm: () => void;
}) {
  const { t } = useTranslation();

  return (
    <div
      role="dialog"
      aria-label={t("pistorm.preview.title")}
      style={{
        position: "fixed",
        inset: 0,
        background: "rgba(0,0,0,0.5)",
        display: "flex",
        alignItems: "center",
        justifyContent: "center",
        zIndex: 50,
      }}
      onClick={onClose}
    >
      <div
        className="card"
        style={{ maxWidth: 760, width: "90%", maxHeight: "80vh", overflowY: "auto" }}
        onClick={(event) => event.stopPropagation()}
      >
        <h2 style={{ fontSize: 16, marginTop: 0 }}>{t("pistorm.preview.title")}</h2>
        <FileDiff
          name="cmdline.txt"
          before={preview.cmdline_before}
          after={preview.cmdline_after}
        />
        <FileDiff name="config.txt" before={preview.config_before} after={preview.config_after} />
        <div style={{ display: "flex", justifyContent: "flex-end", gap: 8, marginTop: 12 }}>
          <button className="btn" onClick={onClose}>
            {t("common.cancel")}
          </button>
          <button className="btn btn-primary" onClick={onConfirm}>
            {t("pistorm.save.button")}
          </button>
        </div>
      </div>
    </div>
  );
}

function FileDiff({ name, before, after }: { name: string; before: string; after: string }) {
  const { t } = useTranslation();
  const unchanged = before.trim() === after.trim();

  return (
    <div style={{ marginBottom: 12 }}>
      <h3 style={{ fontSize: 13, margin: "0 0 4px" }}>
        <code>{name}</code>{" "}
        {unchanged && (
          <span className="badge badge-muted" style={{ fontSize: 10 }}>
            {t("pistorm.preview.unchanged")}
          </span>
        )}
      </h3>
      {!unchanged && (
        <div style={{ display: "grid", gap: 6 }}>
          <pre className="muted" style={diffStyle}>
            {before || t("pistorm.preview.absent")}
          </pre>
          <pre style={diffStyle}>{after}</pre>
        </div>
      )}
    </div>
  );
}

const diffStyle: React.CSSProperties = {
  fontSize: 11,
  margin: 0,
  padding: "6px 8px",
  background: "var(--bg)",
  border: "1px solid var(--border)",
  borderRadius: 4,
  whiteSpace: "pre-wrap",
  wordBreak: "break-all",
  maxHeight: 180,
  overflowY: "auto",
};
