import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { open } from "@tauri-apps/plugin-dialog";

import { scratchSetRoot } from "@/lib/scratch";
import { useSettingsStore } from "@/stores/settingsStore";
import { errorText } from "@/lib/errorText";

/**
 * Where ART stages work it will throw away — asked once, up front (ART-196).
 *
 * ## Why it is a question and not only a Settings entry
 *
 * The owner's ruling, in their own words: *"ART kendi kurulumunda temp
 * klasorunun nerede olacagini kullaniciya sormali, default olarak c diski
 * olur ama belki kullanici baska bir disk yada klasor secebilir."* Asked
 * once, with today's behaviour as the answer you get by pressing the first
 * button. Nobody who does not care is made to care, and anybody who does
 * never has to discover a Settings page **after** their system drive has
 * already filled.
 *
 * It is asked here rather than by the installer, and that is a considered
 * trade rather than an oversight. ART ships an MSI (WiX) *and* an NSIS
 * bundle; a genuine custom dialog in either means owning a template, and
 * doing it twice. Asking on first run costs neither, and covers a case
 * neither installer can — somebody running the executable without installing
 * it at all.
 *
 * ## The two jobs, and why they are in one component
 *
 * Rust holds the root only for the lifetime of the process, so the remembered
 * answer has to be **pushed back at start-up** — and it has to be pushed
 * before anything can stage. That push and the first-run question are the
 * same concern seen from two runs, so they live together rather than being
 * two effects that have to agree about ordering.
 *
 * ## What it deliberately does not do
 *
 * It does not block the app. A modal that must be answered before ART will
 * open is a worse first impression than a folder on `C:`, and the answer can
 * be changed at any time under Settings. It is dismissible, and dismissing it
 * counts as "asked" — pressing Escape is choosing the default, which is what
 * the default is for.
 */
export function ScratchRootGate() {
  const { t } = useTranslation();
  const loaded = useSettingsStore((s) => s.loaded);
  const scratchRoot = useSettingsStore((s) => s.settings.scratchRoot);
  const asked = useSettingsStore((s) => s.settings.scratchRootAsked);
  const update = useSettingsStore((s) => s.update);

  const [error, setError] = useState<string | null>(null);

  // Keyed on the value, not on the settings object: the push happens once per
  // *root*, so an unrelated setting changing does not send Rust a path it
  // already has. A `useRef` guard was written here first and then removed —
  // it survived its own mutation, which is this project's own definition of
  // a guard that is not one.
  useEffect(() => {
    if (!loaded) return;
    // Best effort, and silent: a remembered folder that has since been
    // unplugged must not put an error on the Dashboard the moment ART opens.
    // The refusal comes from the operation that actually needed to stage,
    // where it names what the user was trying to do — and Settings shows the
    // same thing standing still.
    void scratchSetRoot(scratchRoot).catch(() => {});
  }, [loaded, scratchRoot]);

  if (!loaded || asked) return null;

  async function keepDefault() {
    await update({ scratchRootAsked: true });
  }

  async function choose() {
    setError(null);
    const picked = await open({ directory: true, multiple: false });
    if (typeof picked !== "string") return;
    try {
      await scratchSetRoot(picked);
      // Only remembered once Rust has accepted it: a folder ART refused is
      // not a setting, and storing it would have the next run start with a
      // root that does not work.
      await update({ scratchRoot: picked, scratchRootAsked: true });
    } catch (e) {
      setError(errorText(t, e));
    }
  }

  return (
    <div className="card" style={{ margin: "0 0 12px", borderColor: "var(--accent)" }}>
      <h2 style={{ fontSize: 15, marginTop: 0 }}>{t("scratch.ask.heading")}</h2>
      <p style={{ fontSize: 13, margin: "4px 0 8px" }}>{t("scratch.ask.body")}</p>
      <p className="faint" style={{ fontSize: 12, margin: "0 0 12px" }}>
        {t("scratch.ask.note")}
      </p>
      {error ? (
        <p className="badge badge-warn" style={{ fontSize: 11, margin: "0 0 10px" }}>
          {error}
        </p>
      ) : null}
      <div style={{ display: "flex", gap: 8, flexWrap: "wrap" }}>
        <button type="button" onClick={() => void keepDefault()}>
          {t("scratch.ask.keepDefault")}
        </button>
        <button type="button" onClick={() => void choose()}>
          {t("scratch.ask.choose")}
        </button>
      </div>
    </div>
  );
}
