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
  archiveForLine,
  pistormActivateConfigSet,
  pistormDeleteConfigSet,
  pistormCopyRom,
  pistormHardwareMatrix,
  pistormHardwareNotes,
  pistormIdentifyRom,
  pistormPreview,
  pistormPreviewActivateSet,
  pistormPreviewConfigSet,
  pistormProfile,
  pistormRenameConfigSet,
  pistormRomSuits,
  pistormSave,
  pistormScan,
  pistormTokens,
  pistormWriteConfigSet,
  DEFAULT_EMU68_OPTIONS,
  DEFAULT_FIRMWARE_CONFIG,
  DEFAULT_HARDWARE,
  type AmigaChoice,
  type CardRom,
  type ConfigSetPreview,
  type ConfigSetSource,
  type DisplayMode,
  type Emu68Line,
  type Emu68Options,
  type Emu68Profile,
  type FirmwareConfig,
  type HardwareNote,
  type PiChoice,
  type PistormCard,
  type PistormHardware,
  type PistormPreview,
  type RomInfo,
  type VariantChoice,
} from "@/lib/pistorm";
import { usePowerMode } from "@/lib/uxmode";
import { isOneOf, isTextOrNothing } from "@/lib/remembered";
import { useRemembered, useRememberedShape } from "@/lib/useRemembered";
import {
  EMU68_OPTION_SPEC,
  FIRMWARE_SPEC,
  HARDWARE_SPEC,
  visibleOptionGroups,
  type OptionGroup,
} from "@/lib/pistormOptions";
import {
  describeRom,
  isUsableRomName,
  romSuitabilityNote,
  suggestedRomName,
} from "@/lib/pistormRom";

const PROFILES: Emu68Profile[] = ["performance", "daily", "compatibility", "diagnostics"];

const EMU68_LINES: Emu68Line[] = ["stable", "alpha11"];
const isEmu68Line = isOneOf<Emu68Line>("stable", "alpha11");

const DISPLAY_MODES: Array<{ id: string; mode: DisplayMode; tokens: string }> = [
  { id: "auto", mode: "auto", tokens: "—" },
  { id: "dmt1080p60", mode: "dmt1080p60", tokens: "hdmi_group=2 hdmi_mode=82" },
  { id: "cea1080p50", mode: "cea1080p50", tokens: "hdmi_group=1 hdmi_mode=31" },
  { id: "cea720p60", mode: "cea720p60", tokens: "hdmi_group=1 hdmi_mode=4" },
];

function displayModeId(mode: DisplayMode): string {
  return typeof mode === "string" ? mode : "custom";
}

/** The `hdmi_group`/`hdmi_mode` pair a display setting writes, for showing
 *  beside the choice — the same numbers `firmware.rs` writes. */
function customValues(mode: DisplayMode): { group: number; mode: number } | null {
  return typeof mode === "string" ? null : mode.custom;
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
  const [line, setLine] = useRemembered<Emu68Line>("pistorm.line", isEmu68Line, "stable");

  // --- what the screen is doing -------------------------------------------
  const [matrix, setMatrix] = useState<AmigaChoice[]>([]);
  const [notes, setNotes] = useState<HardwareNote[]>([]);
  const [tokens, setTokens] = useState<string[]>([]);
  const [card, setCard] = useState<PistormCard | null>(null);
  const [preview, setPreview] = useState<PistormPreview | null>(null);
  const [romSuits, setRomSuits] = useState<boolean | null>(null);
  const [pendingRom, setPendingRom] = useState<{
    source: string;
    info: RomInfo;
    name: string;
  } | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [statusMsg, setStatusMsg] = useState<string | null>(null);

  // Where "What will be written" lives, so applying a profile can bring the
  // change into view rather than altering the line out of sight (F5.3).
  const tokenPanel = useRef<HTMLDivElement | null>(null);

  const setup = useMemo(
    () => ({ hardware, line, options, firmware }),
    [hardware, line, options, firmware]
  );

  useEffect(() => {
    pistormHardwareMatrix().then(setMatrix).catch((e) => setError(String(e)));
  }, []);

  // The notes and the token line both follow the hardware, and both come from
  // Rust — so that what the screen shows is what will be written, not a second
  // implementation of the same rules.
  useEffect(() => {
    pistormHardwareNotes(hardware, line)
      .then(setNotes)
      .catch(() => setNotes([]));
  }, [hardware, line]);

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
      // The token line is what actually changed, and it is further down the
      // page than the card that changed it. Bring it into view rather than
      // leaving the user to wonder whether anything happened (F5.3).
      tokenPanel.current?.scrollIntoView({ behavior: "smooth", block: "center" });
    } catch (e) {
      setError(String(e));
    }
  }

  // --- Kickstart (F1) ------------------------------------------------------

  /**
   * Whether the Kickstart named in `config.txt` suits the chosen machine.
   *
   * A note, never a block. `null` for a ROM ART does not recognise, because an
   * unidentified file has no opinion attached to it.
   */
  const chosenRom: CardRom | undefined = card?.kickstart_files.find(
    (rom) => rom.file_name.toLowerCase() === firmware.kickstart_file.toLowerCase()
  );

  useEffect(() => {
    if (!chosenRom?.info) {
      setRomSuits(null);
      return;
    }
    pistormRomSuits(chosenRom.info, hardware.amiga)
      .then(setRomSuits)
      .catch(() => setRomSuits(null));
  }, [chosenRom?.info, hardware.amiga]);

  async function chooseRomFile() {
    const picked = await open({
      multiple: false,
      title: t("pistorm.kickstart.chooseTitle"),
      filters: [{ name: "Kickstart ROM", extensions: ["rom", "bin"] }],
    });
    if (typeof picked !== "string") return;

    setError(null);
    try {
      // Identified before anything is copied, so the confirmation can say what
      // the file actually is.
      const info = await pistormIdentifyRom(picked);
      setPendingRom({ source: picked, info, name: suggestedRomName(picked) });
    } catch (e) {
      setError(String(e));
    }
  }

  async function copyPendingRom(overwrite: boolean) {
    if (!cardPath || !pendingRom) return;
    setBusy(true);
    setError(null);
    try {
      const outcome = await pistormCopyRom(
        cardPath,
        pendingRom.source,
        pendingRom.name,
        overwrite
      );
      applyFirmware({ kickstart_file: outcome.rom.file_name });
      setPendingRom(null);
      setStatusMsg(
        outcome.backup
          ? t("pistorm.kickstart.copiedOver", {
              name: outcome.rom.file_name,
              path: outcome.backup,
            })
          : t("pistorm.kickstart.copied", { name: outcome.rom.file_name })
      );
      await scan(cardPath, true);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function showPreview() {
    if (!cardPath) return;
    setBusy(true);
    setError(null);
    try {
      setPreview(await pistormPreview(cardPath, setup));
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
      const outcome = await pistormSave(cardPath, setup);
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

          {/* Which Emu68, because the archive name is not the same in both
              lines — and in one case names a different board (ART-091). */}
          <label style={{ display: "flex", flexDirection: "column", gap: 4, flex: "1 1 180px" }}>
            <span className="muted" style={{ fontSize: 12 }}>
              {t("pistorm.hardware.line")}
            </span>
            <select
              className="btn"
              value={line}
              onChange={(e) => setLine(e.target.value as Emu68Line)}
            >
              {EMU68_LINES.map((entry) => (
                <option key={entry} value={entry}>
                  {t(`pistorm.line.${entry}`)}
                </option>
              ))}
            </select>
          </label>
        </div>

        {/* What follows from the four, stated rather than implied. */}
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
            <dd style={{ margin: 0, fontFamily: "monospace" }}>
              <KernelArchiveLabel variant={variantChoice} line={line} />
            </dd>
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
              {!card.has_kernel
                ? t("pistorm.card.kernelMissing")
                : card.kernel?.version
                  ? t("pistorm.card.kernelVersion", { version: card.kernel.version })
                  : t("pistorm.card.kernelUnknownVersion")}
            </li>
            <li>
              {card.kickstart_files.length > 0
                ? t("pistorm.card.kickstartsFound", {
                    list: card.kickstart_files.map((rom) => rom.file_name).join(", "),
                  })
                : t("pistorm.card.kickstartsMissing")}
            </li>
          </ul>
        )}
      </section>

      {/* 3 — The Kickstart, identified rather than merely named (F1). */}
      <section className="card" style={{ marginBottom: 16 }}>
        <h2 style={{ fontSize: 16, marginTop: 0 }}>{t("pistorm.kickstart.label")}</h2>

        {card && card.kickstart_files.length > 0 && (
          <label style={{ display: "flex", flexDirection: "column", gap: 4 }}>
            <span className="muted" style={{ fontSize: 12 }}>
              {t("pistorm.kickstart.onTheCard")}
            </span>
            {/* The picker is for everyone. Power mode *adds* free typing
                below; it does not take the picker away. */}
            <select
              className="btn"
              value={firmware.kickstart_file}
              onChange={(e) => applyFirmware({ kickstart_file: e.target.value })}
            >
              {card.kickstart_files.map((rom) => {
                const phrase = describeRom(rom);
                return (
                  <option key={rom.file_name} value={rom.file_name}>
                    {t(phrase.key, phrase.params)}
                  </option>
                );
              })}
            </select>
          </label>
        )}

        {(powerMode || !card || card.kickstart_files.length === 0) && (
          <label style={{ display: "flex", flexDirection: "column", gap: 4, marginTop: 10 }}>
            <span className="muted" style={{ fontSize: 12 }}>
              {t("pistorm.kickstart.fileName")}
            </span>
            <input
              className="btn"
              value={firmware.kickstart_file}
              onChange={(e) => applyFirmware({ kickstart_file: e.target.value })}
              style={{ fontFamily: "monospace" }}
            />
          </label>
        )}

        <div style={{ display: "flex", gap: 8, alignItems: "center", marginTop: 10 }}>
          <button className="btn" onClick={() => void chooseRomFile()} disabled={!cardPath || busy}>
            {t("pistorm.kickstart.choose")}
          </button>
          <span className="faint" style={{ fontSize: 11 }}>
            {t("pistorm.kickstart.hint")}
          </span>
        </div>

        {/* A note, never a block — people boot odd combinations on purpose. */}
        {(() => {
          const note = romSuitabilityNote(
            romSuits,
            chosenRom?.info ?? null,
            amigaChoice?.name ?? ""
          );
          return note ? (
            <p
              className="badge badge-warn"
              style={{ fontSize: 11, margin: "10px 0 0", display: "inline-block" }}
            >
              {t(note.key, note.params)}
            </p>
          ) : null;
        })()}
      </section>

      {pendingRom && cardPath && (
        <RomCopyDialog
          pending={pendingRom}
          alreadyThere={
            card?.kickstart_files.some(
              (rom) => rom.file_name.toLowerCase() === pendingRom.name.toLowerCase()
            ) ?? false
          }
          onRename={(name) => setPendingRom({ ...pendingRom, name })}
          onCancel={() => setPendingRom(null)}
          onConfirm={(overwrite) => void copyPendingRom(overwrite)}
        />
      )}

      {/* 4 — Profiles. Every card shows the tokens it stands for. */}
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

          {/* A card can carry a pair that matches no preset. Without this row
              nothing was selected at all, which read as "auto" and would have
              silently removed the user's own forcing on the next save (F5.2). */}
          {(() => {
            const custom = customValues(firmware.display);
            if (!custom && !powerMode) return null;
            const values = custom ?? { group: 2, mode: 82 };
            return (
              <label style={{ display: "flex", gap: 8, alignItems: "baseline", fontSize: 13 }}>
                <input
                  type="radio"
                  name="pistorm-display"
                  checked={custom !== null}
                  onChange={() => applyFirmware({ display: { custom: values } })}
                />
                <span style={{ flex: "0 0 auto" }}>{t("pistorm.display.custom")}</span>
                {custom && powerMode ? (
                  <span style={{ display: "flex", gap: 6, alignItems: "baseline" }}>
                    <code style={{ fontSize: 11 }}>hdmi_group</code>
                    <input
                      className="btn btn-sm"
                      type="number"
                      min={0}
                      max={3}
                      value={custom.group}
                      onChange={(e) =>
                        applyFirmware({
                          display: {
                            custom: { group: Number(e.target.value), mode: custom.mode },
                          },
                        })
                      }
                      style={{ width: "5em" }}
                    />
                    <code style={{ fontSize: 11 }}>hdmi_mode</code>
                    <input
                      className="btn btn-sm"
                      type="number"
                      min={0}
                      max={255}
                      value={custom.mode}
                      onChange={(e) =>
                        applyFirmware({
                          display: {
                            custom: { group: custom.group, mode: Number(e.target.value) },
                          },
                        })
                      }
                      style={{ width: "5em" }}
                    />
                  </span>
                ) : (
                  <code className="faint" style={{ fontSize: 11 }}>
                    {custom
                      ? `hdmi_group=${custom.group} hdmi_mode=${custom.mode}`
                      : t("pistorm.display.customHint")}
                  </code>
                )}
              </label>
            );
          })()}
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
      <section className="card" style={{ marginBottom: 16 }} ref={tokenPanel}>
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

      {/* Named firmware sets — the MultibootOS pattern (F3). */}
      {cardPath && (
        <ConfigSetsSection
          cardPath={cardPath}
          sets={card?.config_sets ?? []}
          setup={setup}
          onChanged={() => void scan(cardPath, true)}
          onError={setError}
          onStatus={setStatusMsg}
        />
      )}

      {/* Declared, not pretended (spec §96). */}
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
 * What the kernel archive is called — or why there is no answer (ART-091).
 *
 * Three cases, and two of them are real: the PiStorm16 has no stable release
 * asset at all, and the 1.1 alpha notes do not say which archive covers the
 * PiStorm600. A filename invented to fill either gap is exactly the slip this
 * round is fixing.
 */
function KernelArchiveLabel({
  variant,
  line,
}: {
  variant: VariantChoice;
  line: Emu68Line;
}) {
  const { t } = useTranslation();
  const archive = archiveForLine(variant, line);

  if (!archive || archive.kind === "absent") {
    return <span className="muted">{t("pistorm.archive.absent")}</span>;
  }
  if (archive.kind === "unstated") {
    return <span className="muted">{t("pistorm.archive.unstated")}</span>;
  }
  return <>{archive.name}</>;
}

/**
 * Confirming a Kickstart before it is copied onto the card.
 *
 * The ROM has already been identified, so this can say what it is rather than
 * only where it came from — and an unrecognised one is labelled and copied all
 * the same. Replacing a file on the card is a separate, explicit answer.
 */
function RomCopyDialog({
  pending,
  alreadyThere,
  onRename,
  onCancel,
  onConfirm,
}: {
  pending: { source: string; info: RomInfo; name: string };
  alreadyThere: boolean;
  onRename: (name: string) => void;
  onCancel: () => void;
  onConfirm: (overwrite: boolean) => void;
}) {
  const { t } = useTranslation();
  const usable = isUsableRomName(pending.name);
  const recognised = pending.info.version !== "Custom";

  return (
    <div
      role="dialog"
      aria-label={t("pistorm.kickstart.copyTitle")}
      style={{
        position: "fixed",
        inset: 0,
        background: "rgba(0,0,0,0.5)",
        display: "flex",
        alignItems: "center",
        justifyContent: "center",
        zIndex: 50,
      }}
      onClick={onCancel}
    >
      <div
        className="card"
        style={{ maxWidth: 560, width: "90%" }}
        onClick={(event) => event.stopPropagation()}
      >
        <h2 style={{ fontSize: 16, marginTop: 0 }}>{t("pistorm.kickstart.copyTitle")}</h2>

        <p style={{ fontSize: 13, margin: "0 0 4px" }}>
          {recognised
            ? t("pistorm.kickstart.picked", {
                rom: pending.info.name,
                revision: pending.info.revision,
                models: pending.info.compatible_models.join(", "),
              })
            : t("pistorm.kickstart.pickedUnrecognised")}
        </p>
        <p className="faint" style={{ fontSize: 11, margin: "0 0 12px", wordBreak: "break-all" }}>
          {pending.source}
        </p>

        <label style={{ display: "flex", flexDirection: "column", gap: 4 }}>
          <span className="muted" style={{ fontSize: 12 }}>
            {t("pistorm.kickstart.nameOnCard")}
          </span>
          <input
            className="btn"
            value={pending.name}
            onChange={(e) => onRename(e.target.value)}
            style={{ fontFamily: "monospace" }}
          />
        </label>
        {!usable && (
          <p className="badge badge-warn" style={{ fontSize: 11, margin: "6px 0 0" }}>
            {t("pistorm.kickstart.badName")}
          </p>
        )}
        {usable && alreadyThere && (
          <p className="badge badge-warn" style={{ fontSize: 11, margin: "6px 0 0" }}>
            {t("pistorm.kickstart.willReplace", { name: pending.name })}
          </p>
        )}

        <div style={{ display: "flex", justifyContent: "flex-end", gap: 8, marginTop: 12 }}>
          <button className="btn" onClick={onCancel}>
            {t("common.cancel")}
          </button>
          <button
            className="btn btn-primary"
            disabled={!usable}
            onClick={() => onConfirm(alreadyThere)}
          >
            {alreadyThere
              ? t("pistorm.kickstart.replaceButton")
              : t("pistorm.kickstart.copyButton")}
          </button>
        </div>
      </div>
    </div>
  );
}

/**
 * Named firmware sets — `config_<name>.txt` beside `config.txt` (F3).
 *
 * The MultibootOS pattern: one file per system, copied over `config.txt` to
 * choose which one boots. Every action here goes through the same
 * preview → backup → write as any other config write, and activating shows the
 * §92 diff first. ART does not interpret what is inside a set.
 */
function ConfigSetsSection({
  cardPath,
  sets,
  setup,
  onChanged,
  onError,
  onStatus,
}: {
  cardPath: string;
  sets: string[];
  setup: Parameters<typeof pistormPreviewConfigSet>[4];
  onChanged: () => void;
  onError: (message: string | null) => void;
  onStatus: (message: string | null) => void;
}) {
  const { t } = useTranslation();
  const [name, setName] = useState("");
  const [source, setSource] = useState<ConfigSetSource>("current-config");
  const [duplicateOf, setDuplicateOf] = useState<string>("");
  const [renaming, setRenaming] = useState<{ from: string; to: string } | null>(null);
  const [preview, setPreview] = useState<(ConfigSetPreview & { confirm: () => void }) | null>(
    null
  );

  async function previewWrite() {
    onError(null);
    try {
      const from = source === "set" ? duplicateOf || sets[0] || null : null;
      const result = await pistormPreviewConfigSet(cardPath, name, source, from, setup);
      setPreview({
        ...result,
        confirm: () => {
          void (async () => {
            try {
              await pistormWriteConfigSet(cardPath, name, source, from, setup);
              setPreview(null);
              setName("");
              onStatus(t("pistorm.sets.saved", { name }));
              onChanged();
            } catch (e) {
              onError(String(e));
            }
          })();
        },
      });
    } catch (e) {
      onError(String(e));
    }
  }

  async function previewActivate(setName: string) {
    onError(null);
    try {
      const result = await pistormPreviewActivateSet(cardPath, setName);
      setPreview({
        ...result,
        confirm: () => {
          void (async () => {
            try {
              const backup = await pistormActivateConfigSet(cardPath, setName);
              setPreview(null);
              onStatus(
                backup
                  ? t("pistorm.sets.activatedWithBackup", { name: setName, path: backup })
                  : t("pistorm.sets.activated", { name: setName })
              );
              onChanged();
            } catch (e) {
              onError(String(e));
            }
          })();
        },
      });
    } catch (e) {
      onError(String(e));
    }
  }

  async function removeSet(setName: string) {
    // Destructive, so it asks — and says what "delete" actually means here,
    // because ART keeps a copy (ART-092).
    if (!window.confirm(t("pistorm.sets.confirmDelete", { name: setName }))) return;
    onError(null);
    try {
      const backup = await pistormDeleteConfigSet(cardPath, setName);
      onStatus(
        backup
          ? t("pistorm.sets.deletedWithBackup", { name: setName, path: backup })
          : t("pistorm.sets.deleted", { name: setName })
      );
      onChanged();
    } catch (e) {
      onError(String(e));
    }
  }

  async function commitRename() {
    if (!renaming) return;
    onError(null);
    try {
      await pistormRenameConfigSet(cardPath, renaming.from, renaming.to);
      setRenaming(null);
      onStatus(t("pistorm.sets.renamed", { from: renaming.from, to: renaming.to }));
      onChanged();
    } catch (e) {
      onError(String(e));
    }
  }

  return (
    <section className="card" style={{ marginBottom: 16 }}>
      <h2 style={{ fontSize: 16, marginTop: 0 }}>{t("pistorm.sets.heading")}</h2>
      <p className="muted" style={{ fontSize: 12, margin: "4px 0 12px" }}>
        {t("pistorm.sets.intro")}
      </p>

      {sets.length === 0 ? (
        <p className="faint" style={{ fontSize: 12, margin: 0 }}>
          {t("pistorm.sets.none")}
        </p>
      ) : (
        <ul style={{ listStyle: "none", padding: 0, margin: "0 0 12px" }}>
          {sets.map((entry) => (
            <li
              key={entry}
              style={{
                display: "flex",
                gap: 8,
                alignItems: "center",
                padding: "4px 0",
                fontSize: 13,
              }}
            >
              <code style={{ flex: "1 1 auto" }}>config_{entry}.txt</code>
              <button className="btn btn-sm" onClick={() => void previewActivate(entry)}>
                {t("pistorm.sets.activate")}
              </button>
              <button
                className="btn btn-sm"
                onClick={() => setRenaming({ from: entry, to: entry })}
              >
                {t("pistorm.sets.rename")}
              </button>
              <button className="btn btn-sm" onClick={() => void removeSet(entry)}>
                {t("pistorm.sets.delete")}
              </button>
            </li>
          ))}
        </ul>
      )}

      {renaming && (
        <div style={{ display: "flex", gap: 8, alignItems: "center", marginBottom: 12 }}>
          <input
            className="btn"
            value={renaming.to}
            onChange={(e) => setRenaming({ ...renaming, to: e.target.value })}
            style={{ fontFamily: "monospace", flex: "1 1 auto" }}
          />
          <button className="btn btn-primary btn-sm" onClick={() => void commitRename()}>
            {t("pistorm.sets.rename")}
          </button>
          <button className="btn btn-sm" onClick={() => setRenaming(null)}>
            {t("common.cancel")}
          </button>
        </div>
      )}

      <div style={{ display: "flex", gap: 8, alignItems: "flex-end", flexWrap: "wrap" }}>
        <label style={{ display: "flex", flexDirection: "column", gap: 4, flex: "1 1 160px" }}>
          <span className="muted" style={{ fontSize: 12 }}>
            {t("pistorm.sets.newName")}
          </span>
          <input
            className="btn"
            value={name}
            onChange={(e) => setName(e.target.value)}
            placeholder="os39"
            style={{ fontFamily: "monospace" }}
          />
        </label>
        <label style={{ display: "flex", flexDirection: "column", gap: 4, flex: "1 1 200px" }}>
          <span className="muted" style={{ fontSize: 12 }}>
            {t("pistorm.sets.from")}
          </span>
          <select
            className="btn"
            value={source}
            onChange={(e) => setSource(e.target.value as ConfigSetSource)}
          >
            <option value="current-config">{t("pistorm.sets.fromCurrent")}</option>
            <option value="screen-settings">{t("pistorm.sets.fromScreen")}</option>
            {sets.length > 0 && <option value="set">{t("pistorm.sets.fromSet")}</option>}
          </select>
        </label>
        {source === "set" && (
          <select
            className="btn"
            value={duplicateOf || sets[0] || ""}
            onChange={(e) => setDuplicateOf(e.target.value)}
          >
            {sets.map((entry) => (
              <option key={entry} value={entry}>
                {entry}
              </option>
            ))}
          </select>
        )}
        <button
          className="btn btn-primary"
          disabled={name.trim().length === 0}
          onClick={() => void previewWrite()}
        >
          {t("pistorm.sets.create")}
        </button>
      </div>

      <p className="faint" style={{ fontSize: 11, margin: "10px 0 0" }}>
        {t("pistorm.sets.deleteKeeps")}
      </p>

      {preview && (
        <div
          role="dialog"
          aria-label={t("pistorm.sets.previewTitle")}
          style={{
            position: "fixed",
            inset: 0,
            background: "rgba(0,0,0,0.5)",
            display: "flex",
            alignItems: "center",
            justifyContent: "center",
            zIndex: 50,
          }}
          onClick={() => setPreview(null)}
        >
          <div
            className="card"
            style={{ maxWidth: 700, width: "90%", maxHeight: "80vh", overflowY: "auto" }}
            onClick={(event) => event.stopPropagation()}
          >
            <h2 style={{ fontSize: 16, marginTop: 0 }}>{t("pistorm.sets.previewTitle")}</h2>
            <FileDiff
              name={preview.file_name}
              before={preview.before}
              after={preview.after}
            />
            <div style={{ display: "flex", justifyContent: "flex-end", gap: 8 }}>
              <button className="btn" onClick={() => setPreview(null)}>
                {t("common.cancel")}
              </button>
              <button className="btn btn-primary" onClick={preview.confirm}>
                {t("pistorm.sets.write")}
              </button>
            </div>
          </div>
        </div>
      )}
    </section>
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
