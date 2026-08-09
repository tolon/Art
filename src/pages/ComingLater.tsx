import { useTranslation } from "react-i18next";

interface ComingLaterProps {
  titleKey: string;
}

/**
 * Placeholder for modules that are not implemented in Phase 0 (spec §96).
 * Clearly labelled "Coming Later" — never fakes functionality.
 */
export function ComingLater({ titleKey }: ComingLaterProps) {
  const { t } = useTranslation();
  return (
    <div className="coming-later">
      <div className="coming-later-icon" aria-hidden>
        🚧
      </div>
      <h1>{t(titleKey)}</h1>
      <p>{t("common.comingLater")}</p>
      <p className="faint" style={{ fontSize: 12, maxWidth: 420 }}>
        This module is part of the ART roadmap but is not implemented in Phase 0.
        It will arrive in a later development phase.
      </p>
    </div>
  );
}
