import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { open, save } from "@tauri-apps/plugin-dialog";

import { useSettingsStore } from "@/stores/settingsStore";
import { getAppInfo } from "@/lib/api";
import {
  artworkCheckSource,
  artworkDefaults,
  sourcePhrase,
  type ConfiguredSource,
} from "@/lib/artwork";

/**
 * The project's mark, if one has been dropped in.
 *
 * `import.meta.glob` rather than a plain `import` for one reason: a plain
 * import of a file that is not there fails the build, and a logo is not
 * something ART needs in order to work. With the glob, the slot is empty until
 * a file lands in `src/assets/` and full the moment one does — no code change,
 * and no broken image in between.
 *
 * Any of the usual formats, so whatever the author has is the file they use.
 */
const APP_LOGO: string | undefined = Object.values(
  import.meta.glob<{ default: string }>("../assets/logo.{png,svg,jpg,jpeg,webp}", {
    eager: true,
  })
)[0]?.default;
import type { AppInfo } from "@/types";
import { LANGUAGE_NAMES, SUPPORTED_LANGUAGES } from "@/i18n";
import type { StoredOverwritePolicy, Theme, UxMode } from "@/lib/settings";
import {
  clampPaneFontSize,
  PANE_FONT_DEFAULT,
  PANE_FONT_MAX,
  PANE_FONT_MIN,
} from "@/lib/dockLayout";
import {
  clampZoom,
  zoomLabel,
  ZOOM_DEFAULT,
  ZOOM_MAX,
  ZOOM_MIN,
  ZOOM_STEP,
} from "@/lib/appZoom";
import {
  DEFAULT_COLOUR_RULES,
  isUsableRuleList,
  type ColourRule,
} from "@/lib/colourRules";
import {
  oplogExportTo,
  oplogPath,
  oplogRecent,
  statusLabel,
  succeeded,
  type OperationRecord,
} from "@/lib/oplog";
import { sourcesLibrary, sourcesSetLibraryRoot } from "@/lib/sources";
import {
  DEFAULT_WHDLOAD_FAST_RAM_MB,
  isMachine,
  isWhdloadFastRamMb,
  machinePhrase,
  type Machine,
} from "@/lib/launch";
import { isText, isTextOrNothing } from "@/lib/remembered";
import { useRemembered } from "@/lib/useRemembered";

export function SettingsPage() {
  const { t } = useTranslation();
  const settings = useSettingsStore((s) => s.settings);
  const update = useSettingsStore((s) => s.update);

  return (
    <div style={{ maxWidth: 640, display: "flex", flexDirection: "column", gap: 20 }}>
      <h1 style={{ fontSize: 20 }}>{t("settings.title")}</h1>

      <section className="card">
        <h2 style={{ fontSize: 15 }}>{t("settings.appearance")}</h2>
        <Field label={t("settings.theme")}>
          <select
            className="btn"
            value={settings.theme}
            onChange={(e) => void update({ theme: e.target.value as Theme })}
          >
            <option value="dark">{t("settings.themeDark")}</option>
            <option value="light">{t("settings.themeLight")}</option>
          </select>
        </Field>
        <Field label={t("settings.uxMode")}>
          <select
            className="btn"
            value={settings.uxMode}
            onChange={(e) => void update({ uxMode: e.target.value as UxMode })}
          >
            <option value="beginner">{t("settings.uxBeginner")}</option>
            <option value="power">{t("settings.uxPower")}</option>
          </select>
          <p className="faint" style={{ fontSize: 11, margin: "4px 0 0" }}>
            {settings.uxMode === "power"
              ? t("settings.uxModePowerHint")
              : t("settings.uxModeBeginnerHint")}
          </p>
        </Field>
        {/* How big the application is drawn. In Appearance rather than in
            Files, because it is the whole program: text, search boxes, icons,
            buttons, every screen. The listing has its own size below it, for
            the one wall of text dense enough to want its own answer. */}
        <Field label={t("settings.appZoom")}>
          <div style={{ display: "flex", gap: 8, alignItems: "center" }}>
            <input
              type="range"
              min={ZOOM_MIN}
              max={ZOOM_MAX}
              step={ZOOM_STEP}
              value={clampZoom(settings.appZoom)}
              onChange={(e) => void update({ appZoom: clampZoom(Number(e.target.value)) })}
              style={{ flex: "1 1 auto" }}
              aria-label={t("settings.appZoom")}
            />
            <span style={{ fontSize: 13, minWidth: "3.5em", textAlign: "right" }}>
              {zoomLabel(settings.appZoom)}
            </span>
            <button
              className="btn btn-sm"
              onClick={() => void update({ appZoom: ZOOM_DEFAULT })}
              disabled={clampZoom(settings.appZoom) === ZOOM_DEFAULT}
            >
              {t("settings.appZoomReset")}
            </button>
          </div>
          <p className="faint" style={{ fontSize: 11, margin: "4px 0 0" }}>
            {t("settings.appZoomHint")}
          </p>
        </Field>
      </section>

      {/* The Files screen's own chrome. Off by default — the pane header's
          source combo reaches every one of these buttons, and Total Commander
          with no button bar is what this phase's brief is built from. */}
      <section className="card">
        <h2 style={{ fontSize: 15 }}>{t("settings.files")}</h2>
        <Field label={t("settings.showSourceButtons")}>
          <label style={{ display: "flex", gap: 6, alignItems: "center", fontSize: 13 }}>
            <input
              type="checkbox"
              checked={settings.showSourceButtons}
              onChange={(e) => void update({ showSourceButtons: e.target.checked })}
            />
            {t("settings.showSourceButtonsLabel")}
          </label>
          <p className="faint" style={{ fontSize: 11, margin: "4px 0 0" }}>
            {t("settings.showSourceButtonsHint")}
          </p>
        </Field>
        {/* The permanent footer that used to ask this on the Files screen is
            gone. The copy dialog still asks, at the moment a collision is
            actually found — and answering it there changes this. */}
        <Field label={t("settings.overwritePolicy")}>
          <select
            className="btn"
            value={settings.overwritePolicy}
            onChange={(e) =>
              void update({ overwritePolicy: e.target.value as StoredOverwritePolicy })
            }
          >
            <option value="skip">{t("files.policy.skip")}</option>
            <option value="overwrite">{t("files.policy.overwrite")}</option>
            <option value="rename">{t("files.policy.rename")}</option>
          </select>
          <p className="faint" style={{ fontSize: 11, margin: "4px 0 0" }}>
            {t("settings.overwritePolicyHint")}
          </p>
        </Field>
        {/* Text size. First in this section on purpose: for a good share of
            this program's users — past fifty, reading glasses — it is not a
            preference but whether the screen is usable at all. A Ctrl+wheel
            shortcut alone would be a feature the people who most need it
            never find. */}
        <Field label={t("settings.paneFontSize")}>
          <div style={{ display: "flex", gap: 8, alignItems: "center" }}>
            <input
              type="range"
              min={PANE_FONT_MIN}
              max={PANE_FONT_MAX}
              step={1}
              value={clampPaneFontSize(settings.paneFontSize || PANE_FONT_DEFAULT)}
              aria-label={t("settings.paneFontSize")}
              style={{ flex: "1 1 auto" }}
              onChange={(e) => void update({ paneFontSize: Number(e.target.value) })}
            />
            <span
              style={{
                fontSize: clampPaneFontSize(settings.paneFontSize || PANE_FONT_DEFAULT),
                minWidth: "9ch",
                textAlign: "right",
              }}
            >
              {clampPaneFontSize(settings.paneFontSize || PANE_FONT_DEFAULT)} px
            </span>
            <button
              className="btn btn-sm"
              onClick={() => void update({ paneFontSize: PANE_FONT_DEFAULT })}
            >
              {t("settings.paneFontSizeReset")}
            </button>
          </div>
          <p className="faint" style={{ fontSize: 11, margin: "4px 0 0" }}>
            {t("settings.paneFontSizeHint")}
          </p>
        </Field>

        {/* Default folders — a file-manager setting, not a general path, so
            it lives here rather than under Paths. ART is used for one purpose:
            the two folders it should open in are the same two every time. */}
        <h3 style={{ fontSize: 13, margin: "16px 0 0" }}>{t("settings.defaultFolders")}</h3>
        <p className="faint" style={{ fontSize: 11, margin: "2px 0 0" }}>
          {t("settings.defaultFoldersHint")}
        </p>
        <PathField
          label={t("settings.defaultLeftPath")}
          placeholder="D:\\Amiga\\Games"
          value={settings.defaultLeftPath}
          pick="folder"
          onChange={(next) => void update({ defaultLeftPath: next })}
        />
        <PathField
          label={t("settings.defaultRightPath")}
          placeholder="D:\\Amiga\\Images"
          value={settings.defaultRightPath}
          pick="folder"
          onChange={(next) => void update({ defaultRightPath: next })}
        />
        <Field label={t("settings.alwaysUseDefaultFolders")}>
          <label style={{ display: "flex", gap: 6, alignItems: "center", fontSize: 13 }}>
            <input
              type="checkbox"
              checked={settings.alwaysUseDefaultFolders}
              onChange={(e) => void update({ alwaysUseDefaultFolders: e.target.checked })}
            />
            {t("settings.alwaysUseDefaultFoldersLabel")}
          </label>
          <p className="faint" style={{ fontSize: 11, margin: "4px 0 0" }}>
            {t("settings.alwaysUseDefaultFoldersHint")}
          </p>
        </Field>

        <Field label={t("settings.rightButtonSelects")}>
          <label style={{ display: "flex", gap: 6, alignItems: "center", fontSize: 13 }}>
            <input
              type="checkbox"
              checked={settings.rightButtonSelects}
              onChange={(e) => void update({ rightButtonSelects: e.target.checked })}
            />
            {t("settings.rightButtonSelectsLabel")}
          </label>
          <p className="faint" style={{ fontSize: 11, margin: "4px 0 0" }}>
            {t("settings.rightButtonSelectsHint")}
          </p>
        </Field>
      </section>

      <ColourRulesSection />

      <section className="card">
        <h2 style={{ fontSize: 15 }}>{t("settings.general")}</h2>
        <Field label={t("settings.language")}>
          <select
            className="btn"
            value={settings.language}
            onChange={(e) => void update({ language: e.target.value })}
          >
            {SUPPORTED_LANGUAGES.map((lng) => (
              <option key={lng} value={lng}>
                {LANGUAGE_NAMES[lng]}
              </option>
            ))}
          </select>
        </Field>
      </section>

      <section className="card">
        <h2 style={{ fontSize: 15 }}>{t("settings.paths")}</h2>
        <p className="muted" style={{ fontSize: 12, marginTop: 4 }}>
          {t("settings.pathsHint")}
        </p>
        {/* Every one of these takes a Browse button (ART-086). ART opens a
            native picker everywhere else it asks for a path; these four were
            simply the fields that never got it wired. */}
        <PathField
          label={t("settings.winuaePath")}
          placeholder="C:\\WinUAE\\winuae64.exe"
          value={settings.winuaePath}
          pick="file"
          onChange={(next) => void update({ winuaePath: next })}
        />
        <PathField
          label={t("settings.hstImagerPath")}
          placeholder="E:\\amiga\\hstimager\\hst.imager.exe"
          value={settings.hstImagerPath}
          pick="file"
          onChange={(next) => void update({ hstImagerPath: next })}
        />
        <PathField
          label={t("settings.collectionDir")}
          placeholder="D:\\Amiga"
          value={settings.lastCollectionDir}
          pick="folder"
          onChange={(next) => void update({ lastCollectionDir: next })}
        />
        <AminetDownloadField />
      </section>

      <PlaySettingsSection />
      <ArtworkSourcesSection />
      <OperationLogSection />
      <AboutSection />
    </div>
  );
}

/**
 * What Play (the Collection screen's launch flow, Task 11) needs before it
 * can start anything: a Kickstart ROM folder, the machine to default to when
 * a title states no chipset, the bootable system a WHDLoad title mounts
 * beside its own drawer, and (ART-151) the Fast RAM headroom a WHDLoad
 * launch's profile gets on top of its stock preset.
 *
 * **Reuses `TitleDetail`'s own remembered keys** (`launch.romDir`,
 * `launch.defaultMachine`, `launch.systemVolume`, `launch.whdloadFastRamMb`)
 * rather than introducing a second copy of them — this is a second entry
 * point onto the same values, not a second set. A user who has never opened
 * a title's Play section still has somewhere to find them, which matters
 * most for the system volume: a WHDLoad launch cannot work at all without
 * one.
 */
function PlaySettingsSection() {
  const { t } = useTranslation();
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
  // ART-151: `1000 Miglia` reached WHDLoad on a stock A500 profile — 512 KB
  // Chip, 512 KB Slow, no Fast RAM at all — and WHDLoad itself refused with
  // DOS-Error #103, "not enough memory available". This is the headroom a
  // WHDLoad launch's profile gets on top of that stock preset; it never
  // applies to a floppy or a plain hardfile.
  const [whdloadFastRamMb, setWhdloadFastRamMb] = useRemembered<number>(
    "launch.whdloadFastRamMb",
    isWhdloadFastRamMb,
    DEFAULT_WHDLOAD_FAST_RAM_MB
  );

  return (
    <section className="card">
      <h2 style={{ fontSize: 15 }}>{t("settings.play.title")}</h2>
      <p className="muted" style={{ fontSize: 12, marginTop: 4 }}>
        {t("settings.play.hint")}
      </p>

      <PathField
        label={t("collection.detail.play.romDir")}
        placeholder="D:\\amiga\\kickstart"
        value={romDir || null}
        pick="folder"
        onChange={(next) => setRomDir(next ?? "")}
      />

      <Field label={t("collection.detail.play.defaultMachine")}>
        <select
          className="btn"
          value={defaultMachine}
          onChange={(e) => setDefaultMachine(e.target.value as Machine)}
        >
          <option value="a500">{t(machinePhrase("a500").key)}</option>
          <option value="a1200">{t(machinePhrase("a1200").key)}</option>
        </select>
      </Field>

      <PathField
        label={t("collection.detail.play.systemVolume")}
        placeholder="E:\\amiga\\amikit\\AmiKit.hdf"
        value={systemVolume}
        pick="file"
        onChange={setSystemVolume}
      />

      <Field label={t("collection.detail.play.whdloadFastRam")}>
        <select
          className="btn"
          value={whdloadFastRamMb}
          onChange={(e) => setWhdloadFastRamMb(Number(e.target.value))}
        >
          <option value={0}>0 MB</option>
          <option value={1}>1 MB</option>
          <option value={2}>2 MB</option>
          <option value={4}>4 MB</option>
          <option value={8}>8 MB</option>
        </select>
        <p className="faint" style={{ fontSize: 11, margin: "4px 0 0" }}>
          {t("collection.detail.play.whdloadFastRamHint")}
        </p>
      </Field>
    </section>
  );
}

/**
 * About — the notice GPLv3 §5 asks an interactive program to display.
 *
 * Static, and deliberately so: no version check, no update button, no network
 * call of any kind. A panel that says who wrote the program and under what
 * terms has no business reaching the internet to do it.
 *
 * The version comes from the build (`CARGO_PKG_VERSION`, through `app_info`)
 * rather than from a string here, so it cannot drift from the installer's.
 *
 * The licence is named and its full text ships beside the program — there is
 * no link, because opening one would need a plugin ART does not have, and a
 * link that silently does nothing is worse than a sentence that does not
 * pretend to be one.
 */
function AboutSection() {
  const { t } = useTranslation();
  const [info, setInfo] = useState<AppInfo | null>(null);

  useEffect(() => {
    getAppInfo()
      .then(setInfo)
      .catch(() => setInfo(null));
  }, []);

  return (
    <section className="card">
      <h2 style={{ fontSize: 15 }}>{t("settings.about.heading")}</h2>

      <div style={{ display: "flex", gap: 12, alignItems: "center", marginTop: 8 }}>
        {APP_LOGO && (
          // `alt` deliberately empty: the name is right beside it, and a
          // screen reader announcing "logo" before reading it is noise.
          <img
            src={APP_LOGO}
            alt=""
            width={56}
            height={56}
            style={{ borderRadius: 8, flex: "0 0 auto", objectFit: "contain" }}
          />
        )}
        <div>
          <p style={{ fontSize: 13, margin: 0 }}>
            <strong>{info?.name ?? "Amiga Retro Toolkit"}</strong>{" "}
            {info ? <span className="muted">{info.version}</span> : null}
          </p>
          <p className="faint" style={{ fontSize: 11, margin: "2px 0 0" }}>
            {t("app.tagline")}
          </p>
        </div>
      </div>

      <dl
        style={{
          display: "grid",
          gridTemplateColumns: "auto 1fr",
          gap: "4px 12px",
          margin: "10px 0 0",
          fontSize: 12,
        }}
      >
        <dt className="muted">{t("settings.about.developer")}</dt>
        <dd style={{ margin: 0 }}>tolon</dd>
        <dt className="muted">{t("settings.about.source")}</dt>
        <dd style={{ margin: 0 }}>
          <code>https://github.com/tolon/Art</code>
        </dd>
        <dt className="muted">{t("settings.about.licenceLabel")}</dt>
        <dd style={{ margin: 0 }}>{t("settings.about.licence")}</dd>
        <dt className="muted">{t("settings.about.copyright")}</dt>
        <dd style={{ margin: 0 }}>Copyright (C) 2026 tolon</dd>
      </dl>

      <p className="faint" style={{ fontSize: 11, margin: "12px 0 0" }}>
        {t("settings.about.warranty")}
      </p>
      <p className="faint" style={{ fontSize: 11, margin: "6px 0 0" }}>
        {t("settings.about.whereTheLicenceIs")}
      </p>
    </section>
  );
}

/**
 * Per-filetype colour rules for the Files screen (brief Part 2).
 *
 * Total Commander-shaped and deliberately so: a filename mask on the left, a
 * colour on the right, **first match wins**, so the order is the user's
 * instrument — a rule above the container rule picks one `.adf` out of the
 * rest. The masks are the same `*`/`?` the filter box takes, and several go in
 * one rule separated by `;`, because "every container" is a list rather than
 * ten rules to keep in order.
 *
 * A row no rule claims keeps the colour ART's own classification gave it, so
 * emptying this list is not the same as breaking it.
 */
function ColourRulesSection() {
  const { t } = useTranslation();
  const stored = useSettingsStore((s) => s.settings.fileColourRules);
  const update = useSettingsStore((s) => s.update);
  const rules: ColourRule[] = isUsableRuleList(stored) ? stored : DEFAULT_COLOUR_RULES;

  function save(next: ColourRule[]) {
    void update({ fileColourRules: next });
  }

  function patch(index: number, change: Partial<ColourRule>) {
    save(rules.map((rule, i) => (i === index ? { ...rule, ...change } : rule)));
  }

  function move(index: number, by: number) {
    const to = index + by;
    if (to < 0 || to >= rules.length) return;
    const next = [...rules];
    [next[index], next[to]] = [next[to], next[index]];
    save(next);
  }

  return (
    <section className="card">
      <h2 style={{ fontSize: 15 }}>{t("settings.colourRules.title")}</h2>
      <p className="muted" style={{ fontSize: 12, marginTop: 4 }}>
        {t("settings.colourRules.description")}
      </p>

      <ul style={{ listStyle: "none", padding: 0, margin: "10px 0 0" }}>
        {rules.map((rule, index) => (
          <li
            key={index}
            style={{
              display: "flex",
              gap: 6,
              alignItems: "center",
              padding: "4px 0",
              flexWrap: "wrap",
            }}
          >
            <input
              className="btn"
              style={{ flex: "0 1 120px", minWidth: 0 }}
              value={rule.label}
              aria-label={t("settings.colourRules.labelAria")}
              onChange={(e) => patch(index, { label: e.target.value })}
            />
            <input
              className="btn"
              style={{ flex: "1 1 200px", minWidth: 0, fontFamily: "ui-monospace, monospace" }}
              value={rule.patterns}
              placeholder="*.adf;*.hdf"
              aria-label={t("settings.colourRules.patternsAria")}
              onChange={(e) => patch(index, { patterns: e.target.value })}
            />
            <input
              type="color"
              className="btn"
              style={{ flex: "0 0 44px", padding: 2 }}
              value={/^#[0-9a-fA-F]{6}$/.test(rule.colour) ? rule.colour : "#ffffff"}
              aria-label={t("settings.colourRules.colourAria")}
              onChange={(e) => patch(index, { colour: e.target.value })}
            />
            {/* Order is meaning here, so it has to be changeable. */}
            <button
              className="btn btn-sm"
              onClick={() => move(index, -1)}
              disabled={index === 0}
              title={t("settings.colourRules.moveUp")}
            >
              ↑
            </button>
            <button
              className="btn btn-sm"
              onClick={() => move(index, 1)}
              disabled={index === rules.length - 1}
              title={t("settings.colourRules.moveDown")}
            >
              ↓
            </button>
            <button
              className="btn btn-sm"
              onClick={() => save(rules.filter((_, i) => i !== index))}
              title={t("settings.colourRules.remove")}
            >
              ✕
            </button>
          </li>
        ))}
      </ul>

      <div style={{ display: "flex", gap: 6, marginTop: 10, flexWrap: "wrap" }}>
        <button
          className="btn btn-sm"
          onClick={() =>
            save([...rules, { label: t("settings.colourRules.newLabel"), patterns: "", colour: "#ffffff" }])
          }
        >
          {t("settings.colourRules.add")}
        </button>
        <button className="btn btn-sm" onClick={() => save(DEFAULT_COLOUR_RULES)}>
          {t("settings.colourRules.reset")}
        </button>
      </div>
    </section>
  );
}

/**
 * The record of everything ART did to the user's data (spec §53).
 *
 * Kept in Settings because it is diagnostics, not a workflow: this is where a
 * user comes to check what happened, find where a backup went, or copy a
 * failure with its error ID into a bug report.
 */
function OperationLogSection() {
  const { t } = useTranslation();
  const [records, setRecords] = useState<OperationRecord[] | null>(null);
  const [logPath, setLogPath] = useState<string>("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    void (async () => {
      try {
        const [recent, path] = await Promise.all([oplogRecent(20), oplogPath()]);
        setRecords(recent);
        setLogPath(path);
      } catch (e) {
        setError(String(e));
      }
    })();
  }, []);

  async function handleExport() {
    setBusy(true);
    setError(null);
    try {
      const target = await save({
        title: t("settings.oplog.exportDialogTitle"),
        defaultPath: "art-operation-log.txt",
        filters: [{ name: t("settings.oplog.textFileType"), extensions: ["txt"] }],
      });
      if (typeof target === "string") {
        await oplogExportTo(target);
      }
    } catch (e) {
      setError(t("settings.oplog.exportFailed", { error: String(e) }));
    } finally {
      setBusy(false);
    }
  }

  return (
    <section className="card">
      <h2 style={{ fontSize: 15 }}>{t("settings.oplog.title")}</h2>
      <p className="muted" style={{ fontSize: 12, marginTop: 4 }}>
        {t("settings.oplog.description")}
      </p>

      {error && (
        <p className="badge badge-err" style={{ display: "inline-block" }}>
          {error}
        </p>
      )}

      {records === null ? (
        <p className="faint" style={{ fontSize: 12 }}>
          {t("settings.oplog.loading")}
        </p>
      ) : records.length === 0 ? (
        <p className="faint" style={{ fontSize: 12 }}>
          {t("settings.oplog.empty")}
        </p>
      ) : (
        <ul style={{ listStyle: "none", padding: 0, margin: "10px 0 0" }}>
          {records.map((r, i) => {
            const label = statusLabel(r);
            return (
              <li
                key={`${r.timestamp}-${i}`}
                style={{
                  padding: "6px 0",
                  borderTop: i === 0 ? "none" : "1px solid var(--border)",
                  fontSize: 12,
                }}
              >
                <span
                  className={`badge ${succeeded(r) ? "badge-ok" : "badge-err"}`}
                  style={{ marginRight: 6 }}
                >
                  {t(label.key, label.params)}
                </span>
                <strong>{r.operation}</strong>
                <span className="faint" style={{ marginLeft: 6 }}>
                  {new Date(r.timestamp * 1000).toLocaleString()}
                </span>
                {r.destination && (
                  <div className="muted" style={{ wordBreak: "break-all" }}>
                    {r.destination}
                  </div>
                )}
                {r.backup && (
                  <div className="faint" style={{ wordBreak: "break-all" }}>
                    {t("settings.oplog.backupLabel", { path: r.backup })}
                  </div>
                )}
                {r.outcome.result === "failure" && (
                  <div className="muted">
                    {r.outcome.message} ({r.outcome.error_code})
                  </div>
                )}
              </li>
            );
          })}
        </ul>
      )}

      <div style={{ marginTop: 10, display: "flex", gap: 8, alignItems: "center" }}>
        <button className="btn btn-sm" onClick={handleExport} disabled={busy}>
          {t("settings.oplog.export")}
        </button>
        {logPath && (
          <span className="faint" style={{ fontSize: 11, wordBreak: "break-all" }}>
            {logPath}
          </span>
        )}
      </div>
    </section>
  );
}

/**
 * A path setting with a Browse button beside it (ART-086).
 *
 * The field stays editable — a path can be pasted, and somebody who knows
 * where they are going types faster than they click — but it is no longer the
 * *only* way in. ART already opens a native picker on the Files screen, in
 * both studios and in the WHDLoad installer; these were the fields that never
 * got it wired.
 *
 * An empty box means "not set" and is stored as `null` rather than `""`, so
 * "the user cleared this" and "the user never touched it" stay the same thing
 * — which is what the callers' `?? fallback` already assumes.
 */
/**
 * Where Aminet downloads land (§41.5.6).
 *
 * Not a plain {@link PathField}, because this path is not merely remembered —
 * the engine owns it, creates it if it is missing, and can refuse it. So the
 * folder is offered to Rust first and only written to settings once Rust has
 * taken it, the same rule the Aminet studio's own picker follows: a folder that
 * failed validation must not be the one waiting at the next launch.
 *
 * It is also shown *read back from the engine* rather than echoed from the
 * input, so "where downloads go" is the truth and not the intention. Cleared,
 * it reverts to ART's own folder — which is why the engine's answer is still
 * shown when the field is empty.
 */
/**
 * Whether a stored value is a source row rather than whatever a hand-edited
 * settings file happens to hold.
 *
 * Checked field by field, the same reason `recallInto` exists: a row that has
 * lost one field must fall back on that field rather than putting `undefined`
 * on screen and into a fetch.
 */
function isConfiguredSource(value: unknown): value is ConfiguredSource {
  if (typeof value !== "object" || value === null) return false;
  const row = value as Record<string, unknown>;
  return (
    typeof row.id === "string" &&
    typeof row.enabled === "boolean" &&
    typeof row.mirrorBase === "string" &&
    typeof row.imageBase === "string"
  );
}

/**
 * The artwork sources, and the two things a user may change about each.
 *
 * The list comes from Rust rather than being written here, so there is one
 * place mirror URLs live. A source ART ships but the saved list has never seen
 * — because this ART is newer than the settings file — is appended rather than
 * ignored; the alternative is a source the user can never reach.
 *
 * An edited address is checked before it is saved. A base that cannot become a
 * `Mirror` would otherwise sit in the file looking fine until a run failed on
 * it, which is the same "refused at the door" rule the rest of ART follows.
 */
function ArtworkSourcesSection() {
  const { t } = useTranslation();
  const stored = useSettingsStore((s) => s.settings.artworkSources);
  const update = useSettingsStore((s) => s.update);
  const [rows, setRows] = useState<ConfiguredSource[] | null>(null);
  const [errors, setErrors] = useState<Record<string, string>>({});

  useEffect(() => {
    let cancelled = false;
    void (async () => {
      const shipped = await artworkDefaults().catch(() => [] as ConfiguredSource[]);
      const saved = Array.isArray(stored) ? stored.filter(isConfiguredSource) : [];
      // Saved rows first, in the user's order; then anything this ART ships
      // that the file has never heard of.
      const merged = [
        ...saved,
        ...shipped.filter((s) => !saved.some((row) => row.id === s.id)),
      ];
      if (!cancelled) setRows(merged.length > 0 ? merged : shipped);
    })();
    return () => {
      cancelled = true;
    };
    // Deliberately once: re-reading on every store write would fight the user's
    // typing, which is the shape of ART-089.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  async function apply(next: ConfiguredSource[], changed: ConfiguredSource) {
    setRows(next);
    try {
      await artworkCheckSource(changed);
      setErrors((prev) => {
        const rest = { ...prev };
        delete rest[changed.id];
        return rest;
      });
      await update({ artworkSources: next });
    } catch (e) {
      // Kept on screen so the user can go on editing, but not saved: a base
      // ART refused must not be the one waiting at the next launch.
      setErrors((prev) => ({ ...prev, [changed.id]: String(e) }));
    }
  }

  function edit(id: string, patch: Partial<ConfiguredSource>) {
    if (!rows) return;
    const next = rows.map((row) => (row.id === id ? { ...row, ...patch } : row));
    const changed = next.find((row) => row.id === id);
    if (changed) void apply(next, changed);
  }

  async function restore() {
    const shipped = await artworkDefaults().catch(() => null);
    if (!shipped) return;
    setRows(shipped);
    setErrors({});
    await update({ artworkSources: shipped });
  }

  if (!rows) return null;

  return (
    <section className="card">
      <h2>{t("artwork.title")}</h2>
      <p className="faint" style={{ fontSize: 12, marginTop: -4 }}>
        {t("artwork.intro")}
      </p>

      {rows.map((row) => (
        <div key={row.id} style={{ marginTop: 12 }}>
          <label style={{ display: "flex", alignItems: "center", gap: 8 }}>
            <input
              type="checkbox"
              checked={row.enabled}
              onChange={(e) => edit(row.id, { enabled: e.target.checked })}
            />
            <strong>{t(sourcePhrase(row.id).key, sourcePhrase(row.id).params)}</strong>
          </label>

          <label style={{ display: "block", marginTop: 6, fontSize: 12 }}>
            {t("artwork.indexMirror")}
            <input
              type="text"
              value={row.mirrorBase}
              onChange={(e) => edit(row.id, { mirrorBase: e.target.value })}
              style={{ width: "100%" }}
            />
          </label>

          <label style={{ display: "block", marginTop: 6, fontSize: 12 }}>
            {t("artwork.imageMirror")}
            <input
              type="text"
              value={row.imageBase}
              placeholder={t("artwork.imageMirrorSame")}
              onChange={(e) => edit(row.id, { imageBase: e.target.value })}
              style={{ width: "100%" }}
            />
          </label>

          {errors[row.id] ? (
            <p className="badge badge-warn" style={{ fontSize: 11, margin: "4px 0 0" }}>
              {t("artwork.invalid", { reason: errors[row.id] })}
            </p>
          ) : null}
        </div>
      ))}

      <button type="button" style={{ marginTop: 12 }} onClick={() => void restore()}>
        {t("artwork.reset")}
      </button>
    </section>
  );
}

/**
 * Where downloads go — **all of them**, not only Aminet's (ART-214).
 *
 * The field was called "Aminet Download Folder" and its hint said "left
 * empty, ART uses its own folder". Three things were wrong with that by the
 * time the owner went looking for it:
 *
 *  - **The name had stopped being true.** Package sets download through the
 *    same library, so somebody hunting for where their sets landed had no
 *    reason to read a field labelled Aminet.
 *  - **"ART uses its own folder" is not what happens.** `lib.rs` takes the
 *    OS download folder, which on Windows is the user's own
 *    `Downloads\Amiga Retro Toolkit` — their disk, not ART's. The owner read
 *    the old sentence and reasonably concluded ART had chosen somewhere of
 *    its own.
 *  - **Changing it moves nothing.** `sources_relocate` moves one file between
 *    subfolders *inside* a library; there is no whole-library move. So a user
 *    who repoints this leaves everything already fetched behind, and nothing
 *    said so.
 *
 * The default stays the OS download folder, and that is the owner's own
 * decision rather than an oversight: *"varsayılan download alanı c olabilir
 * onda sorun yok, bazı insanların tek diski olur."*
 */
function AminetDownloadField() {
  const { t } = useTranslation();
  const stored = useSettingsStore((s) => s.settings.aminetRoot);
  const update = useSettingsStore((s) => s.update);
  const [inUse, setInUse] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    // Best-effort: the studio may never have been opened, and a Settings
    // screen that errors because a subsystem is idle helps nobody.
    sourcesLibrary()
      .then((library) => setInUse(library.root))
      .catch(() => setInUse(null));
  }, [stored]);

  async function apply(next: string | null) {
    setError(null);
    if (next === null) {
      await update({ aminetRoot: null });
      return;
    }
    try {
      const library = await sourcesSetLibraryRoot(next);
      setInUse(library.root);
      await update({ aminetRoot: next });
    } catch (e) {
      setError(String(e));
    }
  }

  return (
    <>
      <PathField
        label={t("settings.aminetRoot")}
        placeholder="D:\\Amiga\\Aminet"
        value={stored}
        pick="folder"
        onChange={(next) => void apply(next)}
      />
      {error ? (
        <p className="badge badge-warn" style={{ fontSize: 11, margin: "-4px 0 0" }}>
          {error}
        </p>
      ) : (
        <p className="faint" style={{ fontSize: 11, margin: "-4px 0 0" }}>
          {inUse
            ? t("settings.aminetRootInUse", { path: inUse })
            : t("settings.aminetRootHint")}
        </p>
      )}
    </>
  );
}

function PathField({
  label,
  placeholder,
  value,
  pick,
  onChange,
}: {
  label: string;
  placeholder: string;
  value: string | null;
  pick: "file" | "folder";
  onChange: (next: string | null) => void;
}) {
  const { t } = useTranslation();

  async function browse() {
    const picked = await open({
      directory: pick === "folder",
      multiple: false,
      defaultPath: value ?? undefined,
      title: label,
    });
    if (typeof picked === "string") onChange(picked);
  }

  return (
    <Field label={label}>
      <div style={{ display: "flex", gap: 6 }}>
        <input
          className="btn"
          style={{ flex: "1 1 auto", minWidth: 0 }}
          placeholder={placeholder}
          value={value ?? ""}
          onChange={(e) => onChange(e.target.value || null)}
        />
        <button className="btn btn-sm" onClick={() => void browse()}>
          {t("settings.browse")}
        </button>
        {value && (
          <button
            className="btn btn-sm"
            onClick={() => onChange(null)}
            title={t("settings.clearPath")}
          >
            ✕
          </button>
        )}
      </div>
    </Field>
  );
}

function Field({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <div style={{ margin: "10px 0", display: "flex", flexDirection: "column", gap: 4 }}>
      <label className="muted" style={{ fontSize: 12 }}>
        {label}
      </label>
      {children}
    </div>
  );
}
