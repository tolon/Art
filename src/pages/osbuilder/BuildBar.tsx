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
// **The size line** (round 4, task 4; rewritten in task 5) says what the
// *session* carries: how many updates are ticked, and whether first boot is
// on. It asks nothing — no plan, no chain, no slots. It computed tab 4's own
// first two summary lines through `useBuildSummary` until 2026-09-09, which
// meant a second plan, a second chain and a second slot report on every one
// of tabs 1-3, beside the tab's own, for a line under the fold of the
// screen. The plan's totals are tab 4's alone; what the bar owes a person
// halfway through the wizard is what *they* have chosen so far, and that is
// two fields of the session.
//
// It is drawn on tabs 1-3 and **not** on `derle`, for the same reason the
// button is not: tab 4 states both facts in full two inches above this bar,
// and the same sentence twice on one screen is ART-202 ("aynı uyarı tek
// ekranda 2 tane").

import { useTranslation } from "react-i18next";
import { useLocation, useNavigate } from "react-router-dom";

import { isTextOrNothing } from "@/lib/remembered";
import { rememberedComponentKey } from "@/lib/osinstall";
import { useRemembered } from "@/lib/useRemembered";
import { useBuildSession } from "@/lib/useBuildSession";
import { stepPath } from "@/lib/buildSteps";

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

  // **Absent means ticked**, the same rule tab 2's own tick reads it by
  // (`session.firstboot.wanted ?? true`): a build nobody has said anything
  // about gets a first-boot block, and the bar says what will happen rather
  // than what has been stored.
  const firstBootWanted = session.firstboot.wanted ?? true;

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
      {!onBuildTab && (
        <span
          className="faint"
          data-testid="build-bar-size"
          style={{ flex: 1, minWidth: "12em", wordBreak: "break-word" }}
        >
          {t("osBuilder.bar.size", {
            updates: session.packages.chosen.length,
            firstboot: t(firstBootWanted ? "osBuilder.bar.firstboot.on" : "osBuilder.bar.firstboot.off"),
          })}
        </span>
      )}
      {!onBuildTab && (
        <button className="btn" data-testid="build-bar-go" onClick={() => navigate(stepPath("derle"))}>
          {t("osBuilder.bar.goToBuild")}
        </button>
      )}
    </div>
  );
}
