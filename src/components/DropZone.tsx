import { useTranslation } from "react-i18next";

interface DropZoneProps {
  active: boolean;
}

/**
 * The large drag target shown on the Dashboard.
 *
 * The actual drop listening is centralised in `Layout` (one global listener);
 * this component is purely visual. `active` reflects the current drag-over
 * phase so the target lights up while the user is dragging.
 */
export function DropZone({ active }: DropZoneProps) {
  const { t } = useTranslation();
  return (
    <div className={`dropzone ${active ? "dropzone-active" : ""}`} role="region"
         aria-label={t("dashboard.dropHere")}>
      <p className="dropzone-title">{t("dashboard.dropHere")}</p>
      <p className="dropzone-hint">{t("dashboard.dropHint")}</p>
      <p className="dropzone-subhint">{t("dashboard.dropSubhint")}</p>
    </div>
  );
}
