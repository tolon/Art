// First boot on the Amiga — the screen for `core/firstboot`, `commands/firstboot.rs`
// and `@/lib/firstboot` (Task 9 of the first-boot round). Tasks 1-8 built the
// mechanism; this is where a person reaches it.
//
// **§92's shape, the same as every other data-changing screen in ART:**
// preview → write → (optionally) rehearse → report. `firstbootPreview` writes
// nothing and is where a refused tree surfaces, before any button exists to
// press. Writing is `Safe` (spec's own classification for this operation): no
// confirmation dialog, but the result names every file it touched and, where
// one existed, the backup it made of `S/User-Startup` — because a merge into
// a file the user already had is not nothing, and "it worked" without saying
// what changed is not enough (CLAUDE.md, "never claim what you did not do").
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
  fatMountPhrase,
  firstbootPreview,
  firstbootRehearse,
  firstbootWrite,
  rehearsalNextStepPhrase,
  rehearsalOutcomePhrase,
  rehearsalTone,
  FIRSTBOOT_REHEARSAL_EVENT,
  type FirstBootPlan,
  type FirstBootWritten,
  type RehearsalResult,
} from "@/lib/firstboot";
import { awaitJobResult, isJobCancellation, jobCancel } from "@/lib/jobs";
import { useBuildSession } from "@/lib/useBuildSession";
import { usePowerMode } from "@/lib/uxmode";
import { useSettingsStore } from "@/stores/settingsStore";
import { Field } from "@/components/osbuilder/Field";
import { FirstBootReportPanel } from "@/components/card/FirstBootReportPanel";

export interface FirstBootPanelProps {
  /** The distribution tree first boot writes into — controlled by the
   *  caller, exactly like `AmigaInstallPanel`'s own `treeRoot`. */
  treeRoot: string | null;
  onTreeRootChange?: (path: string | null) => void;
}

/** A refusal from `firstbootPreview`, in the shape every refusal on this
 *  screen takes: Rust's own sentence (ART-060), and beneath it the one thing
 *  ART can say in the user's own language — that nothing was written. */
function Refusal({ text }: { text: string }) {
  const { t } = useTranslation();
  return (
    <div
      className="badge badge-err"
      data-testid="firstboot-refusal"
      style={{ display: "block", padding: "8px 10px", margin: "0 0 12px", fontSize: 12 }}
    >
      <p style={{ margin: "0 0 6px" }}>{text}</p>
      <p style={{ margin: 0, fontSize: 11 }}>{t("firstboot.panel.nothingWritten")}</p>
    </div>
  );
}

export function FirstBootPanel({ treeRoot, onTreeRootChange }: FirstBootPanelProps) {
  const { t } = useTranslation();
  const power = usePowerMode();
  const { session, setRom, setFirstBoot } = useBuildSession();
  const winuaePath = useSettingsStore((s) => s.settings.winuaePath);

  // One Kickstart for the build (ART-197's fourth row) — the same field
  // `AmigaInstallPanel` and the card step already share.
  const kickstart = session.rom.path;
  const setKickstart = setRom;

  const [preview, setPreview] = useState<FirstBootPlan | null>(null);
  const [previewRefusal, setPreviewRefusal] = useState<string | null>(null);

  const [writing, setWriting] = useState(false);
  const [writeResult, setWriteResult] = useState<FirstBootWritten | null>(null);
  const [writeError, setWriteError] = useState<string | null>(null);

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
  // tree B's fresh preview.
  const treeRootRef = useRef(treeRoot);
  treeRootRef.current = treeRoot;

  // §92's PREVIEW: read-only, and where a tree that is not a first-boot
  // candidate is refused — before either button exists to press.
  useEffect(() => {
    setWriteResult(null);
    setWriteError(null);
    // A rehearsal's outcome, report and copy path are about the tree that was
    // on screen when it ran. Picking a different folder must not leave tree
    // A's rehearsal sitting beside tree B's fresh preview with nothing saying
    // it belongs elsewhere — the confident-wrong-sentence class CLAUDE.md
    // names, here as "still true, just not about this folder any more".
    setRehearsalResult(null);
    setRehearsalError(null);
    setRehearsalCancelled(false);
    // An in-flight rehearsal belongs to the tree it was started against too:
    // clear the running state so the button re-enables for the new tree
    // rather than staying disabled for a job whose result this screen is
    // about to discard.
    setRehearsing(false);
    rehearsalJob.current = null;
    if (!treeRoot) {
      setPreview(null);
      setPreviewRefusal(null);
      return;
    }
    let cancelled = false;
    firstbootPreview(treeRoot)
      .then((plan) => {
        if (cancelled) return;
        setPreview(plan);
        setPreviewRefusal(null);
      })
      .catch((e) => {
        if (cancelled) return;
        setPreview(null);
        setPreviewRefusal(errorText(t, e));
      });
    return () => {
      cancelled = true;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [treeRoot]);

  async function chooseTreeRoot() {
    const picked = await open({ directory: true, multiple: false });
    if (typeof picked === "string") onTreeRootChange?.(picked);
  }

  async function chooseKickstart() {
    const picked = await open({
      multiple: false,
      title: t("osinstall.amigaInstall.kickstart.chooseTitle"),
      filters: [{ name: "Kickstart ROM", extensions: ["rom", "bin"] }],
    });
    if (typeof picked === "string") setKickstart(picked);
  }

  async function runWrite() {
    if (!treeRoot) return;
    // Leftover round: captured the same way `runRehearsal` already captures
    // its own `forTree` below, and compared against `treeRootRef.current`
    // (the latest render's value) when the refresh resolves — the same I3
    // guard the preview effect carries for its own fetch, one call further
    // on. Without it, writing on tree A and switching to tree B before the
    // refresh resolves would overwrite B's own fresh preview with A's.
    const forTree = treeRoot;
    setWriting(true);
    setWriteError(null);
    setWriteResult(null);
    try {
      const result = await firstbootWrite(treeRoot);
      setWriteResult(result);
      setFirstBoot({ written: true });
      // Refresh the preview so "already written" and the button's own label
      // reflect what is on disk now — a re-read, never a guess.
      firstbootPreview(treeRoot)
        .then((plan) => {
          if (treeRootRef.current !== forTree) return;
          setPreview(plan);
        })
        .catch(() => {
          // The write itself already succeeded and is reported; a preview
          // that cannot re-run a moment later must not turn that into a
          // failure it was not.
        });
    } catch (e) {
      setWriteError(errorText(t, e));
    } finally {
      setWriting(false);
    }
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

  const fat = preview ? fatMountPhrase(preview.fatMount) : null;
  const outcome = rehearsalResult ? rehearsalOutcomePhrase(rehearsalResult.outcome) : null;
  const nextStep = rehearsalResult ? rehearsalNextStepPhrase(rehearsalResult.outcome) : null;
  const tone = rehearsalResult ? rehearsalTone(rehearsalResult.outcome) : null;
  const toneClass =
    tone === "ok" ? "badge badge-ok" : tone === "err" ? "badge badge-err" : "badge badge-warn";

  return (
    <section className="card" style={{ marginBottom: 16 }}>
      <h2 style={{ fontSize: 16, marginTop: 0 }}>{t("firstboot.panel.heading")}</h2>
      <p className="muted" style={{ fontSize: 12, margin: "4px 0 12px" }}>
        {t("firstboot.panel.intro")}
      </p>

      <Field
        label={t("firstboot.panel.tree")}
        value={treeRoot}
        empty={t("osinstall.packages.treeRoot.none")}
        onChoose={() => void chooseTreeRoot()}
        choose={t("common.browse")}
        hint={t("osinstall.packages.treeRoot.hint")}
      />

      {previewRefusal && <Refusal text={previewRefusal} />}

      {preview && (
        <div
          data-testid="firstboot-preview"
          style={{
            border: "1px solid var(--border)",
            borderRadius: 4,
            padding: "8px 10px",
            marginBottom: 12,
          }}
        >
          <div className="muted" style={{ fontSize: 12, fontWeight: 600, marginBottom: 6 }}>
            {t("firstboot.panel.preview")}
          </div>
          <div className="faint" style={{ fontSize: 11, marginBottom: 4 }}>
            {t("firstboot.panel.steps")}
          </div>
          <div style={{ overflowX: "auto", marginBottom: 8 }}>
            <table style={{ borderCollapse: "collapse", width: "100%", fontSize: 12 }}>
              <tbody>
                {preview.steps.map((step) => (
                  <tr key={step.name} data-testid="firstboot-step-row">
                    <td style={{ padding: "2px 8px 2px 0", whiteSpace: "nowrap" }}>{step.name}</td>
                    {power && (
                      <td className="faint" style={{ padding: "2px 0", wordBreak: "break-all" }}>
                        {step.treePath}
                      </td>
                    )}
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
          {fat && (
            <p data-testid="firstboot-fat-mount" style={{ fontSize: 12, margin: "0 0 6px" }}>
              {t(fat.key, fat.params)}
            </p>
          )}
          <p className="faint" style={{ fontSize: 11, margin: "0 0 6px" }}>
            {t("firstboot.panel.bytes", { kb: Math.round(preview.bytesAdded / 1024) })}
          </p>
          {preview.alreadyWritten && (
            <p
              data-testid="firstboot-already-written"
              className="badge badge-warn"
              style={{ display: "block", padding: "4px 8px", fontSize: 11, margin: "0 0 6px" }}
            >
              {t("firstboot.panel.alreadyWritten")}
            </p>
          )}
        </div>
      )}

      <div style={{ display: "flex", gap: 8, alignItems: "center", marginBottom: 16 }}>
        <button
          className="btn btn-primary"
          onClick={() => void runWrite()}
          disabled={!treeRoot || !preview || writing}
        >
          {t(preview?.alreadyWritten ? "firstboot.panel.writeAgain" : "firstboot.panel.write")}
        </button>
      </div>

      {writeError && (
        <div
          className="badge badge-err"
          data-testid="firstboot-write-error"
          style={{ display: "block", padding: "8px 10px", fontSize: 12, marginBottom: 12 }}
        >
          {writeError}
        </div>
      )}

      {writeResult && (
        <div
          data-testid="firstboot-write-result"
          className="badge badge-ok"
          style={{ display: "block", padding: "8px 10px", fontSize: 12, marginBottom: 16 }}
        >
          <p style={{ margin: "0 0 6px" }}>
            {t("firstboot.panel.written", { count: writeResult.files.length })}
          </p>
          {power && (
            <ul style={{ margin: "0 0 6px", paddingLeft: 18 }}>
              {writeResult.files.map((file) => (
                <li key={file} style={{ wordBreak: "break-all" }}>
                  {file}
                </li>
              ))}
            </ul>
          )}
          {writeResult.userStartupBackup && (
            <p data-testid="firstboot-write-backup" style={{ margin: "0 0 4px", wordBreak: "break-all" }}>
              {t("firstboot.panel.backup", { path: writeResult.userStartupBackup })}
            </p>
          )}
          {writeResult.userStartupCreated && (
            <p data-testid="firstboot-write-created" style={{ margin: 0 }}>
              {t("firstboot.panel.created")}
            </p>
          )}
        </div>
      )}

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
    </section>
  );
}
