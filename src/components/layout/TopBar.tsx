import { useTranslation } from "react-i18next";
import { useSettingsStore } from "@/stores/settingsStore";
import type { UxMode } from "@/lib/settings";

export function TopBar() {
  const { t } = useTranslation();
  const uxMode = useSettingsStore((s) => s.settings.uxMode);
  const update = useSettingsStore((s) => s.update);

  const toggleMode = () => {
    const next: UxMode = uxMode === "beginner" ? "power" : "beginner";
    void update({ uxMode: next });
  };

  return (
    <header className="topbar">
      <div className="topbar-title">{t("app.tagline")}</div>
      <div className="topbar-actions">
        <span className={`badge ${uxMode === "power" ? "badge-ok" : "badge-muted"}`}>
          {uxMode === "power" ? t("settings.uxPower") : t("settings.uxBeginner")}
        </span>
        <button className="btn" onClick={toggleMode} title={t("settings.uxMode")}>
          {uxMode === "power" ? "◐" : "◑"}
        </button>
      </div>
    </header>
  );
}
