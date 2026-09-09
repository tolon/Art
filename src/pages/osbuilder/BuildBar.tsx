// The bar under the install lane's tabs (four-tab design § 2).
//
// One place that answers "where is this going" on every tab, and one button
// that takes the person to the build tab. **It only navigates**: a run
// starts from tab 4's own button, once, knowingly (§ 10.4 — a button that
// navigates on three tabs and runs on the fourth, under one label, is a
// defect waiting to happen; so this button is absent on `derle`).
//
// It reads the remembered destination through the same key and guard the
// files tab writes it with, and writes nothing: `remembered.ts`'s rule.

import { useTranslation } from "react-i18next";
import { useLocation, useNavigate } from "react-router-dom";

import { isTextOrNothing } from "@/lib/remembered";
import { rememberedComponentKey } from "@/lib/osinstall";
import { useRemembered } from "@/lib/useRemembered";
import { useBuildSession } from "@/lib/useBuildSession";

export function BuildBar() {
  const { t } = useTranslation();
  const { session } = useBuildSession();
  const location = useLocation();
  const navigate = useNavigate();
  const [destination] = useRemembered<string | null>(
    rememberedComponentKey("osinstall.destination", session.release),
    isTextOrNothing,
    null
  );

  if (session.kind !== "install") return null;
  const onBuildTab = location.pathname === "/os-builder/derle";

  return (
    <div
      data-testid="build-bar"
      style={{
        position: "sticky",
        bottom: 0,
        display: "flex",
        gap: 12,
        alignItems: "center",
        flexWrap: "wrap",
        padding: "8px 12px",
        marginTop: 16,
        borderTop: "1px solid var(--border)",
        background: "var(--bg)",
        fontSize: 12,
      }}
    >
      <span className="muted" data-testid="build-bar-destination" style={{ flex: 1, minWidth: "12em", wordBreak: "break-all" }}>
        {destination ? t("osBuilder.bar.destination", { path: destination }) : t("osBuilder.bar.noDestination")}
      </span>
      {!onBuildTab && (
        <button className="btn" data-testid="build-bar-go" onClick={() => navigate("/os-builder/derle")}>
          {t("osBuilder.bar.goToBuild")}
        </button>
      )}
    </div>
  );
}
