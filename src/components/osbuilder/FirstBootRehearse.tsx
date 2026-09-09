// Rehearsing first boot under WinUAE — cut out of `FirstBootPanel` in
// four-tabs round 5 and mounted in the WinUAE studio beside
// `AmigaInstallPanel`.
//
// **Why only the rehearsal is left.** `FirstBootPanel` was §92's whole shape
// on one screen: preview → write → rehearse → report. The wizard now owns
// two thirds of that — the tick that asks for first boot is tab 2's, the
// write is a phase of tab 4's build, and "this tree already carries a block"
// is tab 2's own hint — so what remained was a fourth place a person could
// write first boot from, saying the same thing in different words. The
// rehearsal is the one part neither tab can do: it opens an emulator window,
// which is the studio's business and nobody else's.
//
// **Rehearsing never touches the tree.** It boots a *copy* under WinUAE and
// reads back the Amiga's own report — proof the dispatcher runs and each step
// answers, never proof of the hardware branch, which always skips under an
// emulator. Four endings, four sentences: succeeded, a step refused, nobody
// answered, the window was closed. Collapsing any two is the defect CLAUDE.md
// names ("the failure that does not crash") — "watch the window next time" is
// the wrong advice for a window the user closed themselves.

import { useEffect, useRef, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { useTranslation } from "react-i18next";

import { errorText } from "@/lib/errorText";
import {
  firstbootRehearse,
  rehearsalNextStepPhrase,
  rehearsalOutcomePhrase,
  rehearsalTone,
  FIRSTBOOT_REHEARSAL_EVENT,
  type RehearsalResult,
} from "@/lib/firstboot";
import { awaitJobResult, isJobCancellation, jobCancel } from "@/lib/jobs";
import { useBuildSession } from "@/lib/useBuildSession";
import { useSettingsStore } from "@/stores/settingsStore";
import { Field } from "@/components/osbuilder/Field";
import { FirstBootReportPanel } from "@/components/card/FirstBootReportPanel";

export interface FirstBootRehearseProps {
  /** The distribution tree a copy of which is booted — resolved by the
   *  studio through the same `useChainTree` rule `AmigaInstallPanel` resolves
   *  its own with, so the two cards on that screen cannot speak about two
   *  different trees. */
  treeRoot: string | null;
}

export function FirstBootRehearse({ treeRoot }: FirstBootRehearseProps) {
  const { t } = useTranslation();
  const { session, setRom } = useBuildSession();
  const winuaePath = useSettingsStore((s) => s.settings.winuaePath);

  // One Kickstart for the build (ART-197's fourth row) — the same field
  // `AmigaInstallPanel` and the card step already share.
  const kickstart = session.rom.path;
  const setKickstart = setRom;

  const rehearsalJob = useRef<number | null>(null);
  const [rehearsing, setRehearsing] = useState(false);
  const [rehearsalResult, setRehearsalResult] = useState<RehearsalResult | null>(null);
  const [rehearsalError, setRehearsalError] = useState<string | null>(null);
  const [rehearsalCancelled, setRehearsalCancelled] = useState(false);

  // I3 (final review): `runRehearsal`'s `await` outlives the render it
  // started in, so the closure's own `treeRoot` is fixed at call time and
  // cannot tell a resolution "the user has since picked a different folder".
  // A ref updated on every render carries the current value into that later
  // moment; `runRehearsal` compares it against the tree it was launched for
  // and drops a stale result rather than rendering tree A's outcome under
  // tree B's.
  const treeRootRef = useRef(treeRoot);
  treeRootRef.current = treeRoot;

  // A rehearsal's outcome, report and copy path are about the tree that was
  // on screen when it ran. Picking a different folder must not leave tree A's
  // rehearsal sitting under tree B with nothing saying it belongs elsewhere —
  // the confident-wrong-sentence class CLAUDE.md names, here as "still true,
  // just not about this folder any more".
  //
  // It was the preview effect that did this reset while this lived in
  // `FirstBootPanel`; there is no preview here any more, so the reset is its
  // own effect and says what it is for. An in-flight rehearsal belongs to the
  // tree it was started against too: the running state is cleared so the
  // button re-enables for the new tree rather than staying disabled for a job
  // whose result this component is about to discard.
  useEffect(() => {
    setRehearsalResult(null);
    setRehearsalError(null);
    setRehearsalCancelled(false);
    setRehearsing(false);
    rehearsalJob.current = null;
  }, [treeRoot]);

  async function chooseKickstart() {
    const picked = await open({
      multiple: false,
      title: t("osinstall.amigaInstall.kickstart.chooseTitle"),
      filters: [{ name: "Kickstart ROM", extensions: ["rom", "bin"] }],
    });
    if (typeof picked === "string") setKickstart(picked);
  }

  async function runRehearsal() {
    if (!treeRoot || !kickstart) return;
    // Captured once, at launch — compared against `treeRootRef.current` (the
    // latest render's value) when the job settles, so a resolution that
    // arrives after the user has picked a different folder is dropped rather
    // than rendered under it (I3, final review).
    const forTree = treeRoot;
    setRehearsing(true);
    setRehearsalError(null);
    setRehearsalCancelled(false);
    setRehearsalResult(null);
    try {
      const result = await awaitJobResult<RehearsalResult, RehearsalResult>(
        FIRSTBOOT_REHEARSAL_EVENT,
        async () => {
          const id = await firstbootRehearse({ tree: treeRoot, kickstart }, winuaePath);
          rehearsalJob.current = id;
          return id;
        },
        (payload) => payload
      );
      if (treeRootRef.current !== forTree) return;
      setRehearsalResult(result);
    } catch (e) {
      if (treeRootRef.current !== forTree) return;
      if (isJobCancellation(e)) {
        setRehearsalCancelled(true);
      } else {
        setRehearsalError(errorText(t, e));
      }
    } finally {
      if (treeRootRef.current === forTree) {
        setRehearsing(false);
        rehearsalJob.current = null;
      }
    }
  }

  function stopRehearsal() {
    if (rehearsalJob.current !== null) void jobCancel(rehearsalJob.current);
  }

  const outcome = rehearsalResult ? rehearsalOutcomePhrase(rehearsalResult.outcome) : null;
  const nextStep = rehearsalResult ? rehearsalNextStepPhrase(rehearsalResult.outcome) : null;
  const tone = rehearsalResult ? rehearsalTone(rehearsalResult.outcome) : null;
  const toneClass =
    tone === "ok" ? "badge badge-ok" : tone === "err" ? "badge badge-err" : "badge badge-warn";

  return (
    <div>
      <h3 style={{ fontSize: 14 }}>{t("firstboot.panel.rehearse")}</h3>
      <p className="muted" style={{ fontSize: 12, margin: "4px 0 12px" }}>
        {t("firstboot.panel.rehearseIntro")}
      </p>

      <p
        className="badge badge-warn"
        data-testid="firstboot-rehearsal-window"
        style={{ display: "block", padding: "6px 12px", fontSize: 12, marginBottom: 12 }}
      >
        {t("firstboot.panel.window")}
      </p>

      <Field
        label={t("firstboot.panel.kickstart")}
        value={kickstart}
        empty={t("osinstall.amigaInstall.kickstart.none")}
        onChoose={() => void chooseKickstart()}
        choose={t("common.browse")}
        hint={t("osinstall.amigaInstall.kickstart.hint")}
      />
      <p className="faint" style={{ fontSize: 11, margin: "0 0 12px" }}>
        {t("firstboot.panel.winuae")}
      </p>

      <div style={{ display: "flex", gap: 8, alignItems: "center", marginBottom: 12 }}>
        <button
          className="btn"
          onClick={() => void runRehearsal()}
          disabled={!treeRoot || !kickstart || rehearsing}
        >
          {t(rehearsing ? "firstboot.panel.running" : "firstboot.panel.run")}
        </button>
        {rehearsing && (
          <button className="btn" onClick={stopRehearsal}>
            {t("firstboot.panel.stop")}
          </button>
        )}
      </div>

      {/* **A disabled button with nothing beside it is a screen that has
          refused and not said so.** This component has no tree field of its
          own — the tree is `AmigaInstallPanel`'s *Distribution tree* above
          it, or the destination chosen on the OS Builder's Kickstart and
          destination tab, which wins over it — so a reader looking for what
          to fill in has nothing on this card to look at. The sentence names
          both places rather than saying "no tree". Only the missing tree:
          the Kickstart is a field right here, already empty and labelled. */}
      {!treeRoot && (
        <p
          className="muted"
          data-testid="firstboot-rehearsal-needs-tree"
          style={{ fontSize: 11, margin: "-4px 0 12px" }}
        >
          {t("firstboot.rehearse.needsTree")}
        </p>
      )}

      {rehearsalCancelled && (
        <div
          className="badge badge-warn"
          data-testid="firstboot-rehearsal-cancelled"
          style={{ display: "block", padding: "8px 10px", fontSize: 12, marginBottom: 12 }}
        >
          {t("firstboot.panel.cancelled")}
        </div>
      )}

      {rehearsalError && (
        <div
          className="badge badge-err"
          data-testid="firstboot-rehearsal-error"
          style={{ display: "block", padding: "8px 10px", fontSize: 12, marginBottom: 12 }}
        >
          {rehearsalError}
        </div>
      )}

      {rehearsalResult && outcome && nextStep && (
        <div
          data-testid="firstboot-rehearsal-report"
          className={toneClass}
          style={{ display: "block", padding: "8px 10px", margin: "0 0 12px", fontSize: 12 }}
        >
          <p data-testid="firstboot-rehearsal-outcome" style={{ margin: "0 0 6px" }}>
            {t(outcome.key, outcome.params)}
          </p>
          <p data-testid="firstboot-rehearsal-next" style={{ margin: "0 0 8px" }}>
            {t(nextStep.key, nextStep.params)}
          </p>
          <FirstBootReportPanel report={rehearsalResult.outcome.report} />
          <p
            data-testid="firstboot-rehearsal-copy"
            style={{ margin: "8px 0 0", wordBreak: "break-all" }}
          >
            {rehearsalResult.discarded
              ? t("firstboot.panel.copyDiscarded")
              : t("firstboot.panel.copyKept", { path: rehearsalResult.copy })}
          </p>
        </div>
      )}
    </div>
  );
}
