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
//
// It is the bar under the **tabs**, so it is absent on `hedef`, which is the
// entry chip rather than a tab of the lane.
//
// **The size line** (round 4, task 4) is tab 4's own first two summary lines —
// what will be written, and what will be updated — computed by the one hook
// tab 4 computes them with (`useBuildSummary`), so the bar and the Build
// button can never state two different builds. It is drawn on tabs 1-3 and
// **not** on `derle`, for the same reason the button is not: tab 4 renders
// all four lines two inches above this bar, and the same sentence twice on
// one screen is ART-202 ("aynı uyarı tek ekranda 2 tane"). Mounting the hook
// in a child rather than here is what keeps that a real saving — on tab 4
// nothing here asks anything at all.

import { useTranslation } from "react-i18next";
import { useLocation, useNavigate } from "react-router-dom";

import { useBuildSummary } from "@/components/osbuilder/buildSummary";
import { isTextOrNothing } from "@/lib/remembered";
import { rememberedComponentKey } from "@/lib/osinstall";
import { useRemembered } from "@/lib/useRemembered";
import { useBuildSession } from "@/lib/useBuildSession";
import { stepPath } from "@/lib/buildSteps";

/**
 * How big this build is, in the bar.
 *
 * `previewReplacements: false` because the fourth summary line — *what this
 * would replace* — costs an `osinstall_collisions` per ticked update, which
 * opens an archive each. The bar does not draw that line, so it must not pay
 * for it on every tab.
 */
function BuildBarSize() {
  const { t } = useTranslation();
  const { sizeLines } = useBuildSummary({ previewReplacements: false });
  return (
    <span
      className="faint"
      data-testid="build-bar-size"
      style={{ flex: 1, minWidth: "12em", wordBreak: "break-word" }}
    >
      {sizeLines.map((line) => t(line.key, line.params)).join(" · ")}
    </span>
  );
}

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
  // **Not on the entry chip.** `hedef` is where the kind is chosen, not a tab
  // of the lane (four-tab design § 2): a bar naming a destination and
  // offering "go to Build" under the *picker* claims a build is already under
  // way when the person may be about to choose a different kind entirely.
  if (location.pathname === stepPath("hedef") || location.pathname === "/os-builder") return null;
  const onBuildTab = location.pathname === stepPath("derle");

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
      {!onBuildTab && <BuildBarSize />}
      {!onBuildTab && (
        <button className="btn" data-testid="build-bar-go" onClick={() => navigate(stepPath("derle"))}>
          {t("osBuilder.bar.goToBuild")}
        </button>
      )}
    </div>
  );
}
