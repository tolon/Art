// Running a package's own installer inside an emulator, against a copy of a
// distribution tree — the screen for `core/amigainstall`, `commands/amigainstall.rs`
// and `@/lib/amigainstall`, all of which existed before anything could reach
// them (Task 6 of the Amiga-side install round).
//
// **Nothing here decrypts anything and no protection is bypassed.** Two
// AmigaOS BoingBags carry ZipCrypto-encrypted payloads whose password belongs
// to the package's own Amiga-side `Updater` (ART-166). That Updater runs where
// it was written to run. This screen is how a person asks for that.
//
// ## The one thing this screen must not get wrong
//
// This round produced the same defect three times, and never as a crash:
// **ART telling the user a confidently wrong sentence.** Nothing mounted the
// package, so ART would have said "the installer ran and said no" about a
// program that never started (ART-185); the owner's BoingBag 1 carries an
// `Updater` that cannot work under an emulator, which would have produced the
// same sentence about a program that could not run (ART-186); and a
// successful install was reported as a failure because recording it failed
// (ART-186 fix round 1). §89 forbids all three.
//
// So, on this screen:
//
//   - **The four endings stay four.** Succeeded, the installer refused, the
//     deadline expired, the owner closed the window — each is its own
//     sentence and its own next step, mapped in `@/lib/amigainstall` and
//     tested there and here. "Timed out — watch the window next time" is
//     the wrong advice for a window the user shut themselves, which is why
//     collapsing any two of them is a defect and not a simplification.
//   - **A refusal says which reason applies.** The sentences come from Rust
//     and are English whatever language is chosen (ART-060). They are shown
//     verbatim rather than replaced by one translated "it was refused",
//     because the whole value of a refusal is *which* one it is — a missing
//     prerequisite names the package to install first and in what order, and
//     an `Updater` too old to run under an emulator names the second archive
//     that fixes it. A translated line beside them says the thing ART can
//     say in the user's own language and that the English does not: nothing
//     was copied.
//   - **A run that did not succeed says where the copy is.** A user told "it
//     failed", and not told where the evidence went, has been given nothing.
//   - **The emulator is a window on this desktop, and the screen says so
//     before it opens** — an earlier round opened one repeatedly without
//     warning and that was a real annoyance.
//
// ## Shape
//
// §92's, the same as every other data-changing screen in ART: **preview →
// confirm → job → report.** `amigaInstallPreview` writes nothing and starts
// nothing, and it is also where every refusal surfaces — `compose` refuses
// the prerequisite chain *before anything is copied*, and the preview goes
// through the very same function, so a chain the tree cannot carry is a
// sentence here instead of a confirm button followed by a red job.
//
// Which packages are offered is read from the recipes
// (`PackageSummary.amigaInstallable`), never a list of ids written here: a
// fourth package is a JSON file, and a panel with a hardcoded list would
// silently not join it.
//
// Remembered through `@/lib/remembered`'s guards — the package, its two
// archives and the Kickstart, because every one of them is a decision the
// user made and would be annoyed to make again tomorrow. The tree itself is
// the caller's (`OsInstall.tsx` remembers one for both package panels): which
// tree a screen has open is not a setting, which the collection wave already
// ruled.

import { useEffect, useMemo, useRef, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { useTranslation } from "react-i18next";

import {
  amigaInstallPreview,
  amigaInstallRun,
  onAmigaInstallResult,
  outcomeNextStepPhrase,
  outcomePhrase,
  outcomeTone,
  overlayAdvicePhrase,
  readinessBlockers,
  settlementPhrase,
  type AmigaInstallPreview,
  type AmigaInstallRequest,
  type AmigaInstallResult,
} from "@/lib/amigainstall";
import { osinstallPackages, type PackageSummary } from "@/lib/osinstall";
import { fraction, onJobProgress, subscribeSafely, type JobProgress } from "@/lib/jobs";
import { isTextOrNothing } from "@/lib/remembered";
import { useRemembered } from "@/lib/useRemembered";
import { usePowerMode } from "@/lib/uxmode";
import { useSettingsStore } from "@/stores/settingsStore";
import { Field } from "@/components/osbuilder/Field";

export interface AmigaInstallPanelProps {
  /** The distribution tree the installer runs against — controlled by the
   *  caller, which is what lets this and `PackagePanel` speak about the same
   *  tree without either owning it. */
  treeRoot: string | null;
  onTreeRootChange?: (path: string | null) => void;
  /** Where the user keeps their update archives. Used for the catalogue —
   *  which packages ART ships a recipe for — and as the file dialogs'
   *  starting folder. The run itself takes whole file paths, never a folder:
   *  the second archive is chosen deliberately, not guessed at. */
  packageFolder?: string | null;
}

/**
 * A refusal, as the screen says it.
 *
 * The sentence in the middle is **Rust's, verbatim and in English** (ART-060):
 * a missing prerequisite names what to install first and in what order, and an
 * installer too old for an emulator names the archive that fixes it. Replacing
 * it with one translated "it was refused" would lose the half that matters.
 * The line under it is the half ART *can* say in the user's own language —
 * that nothing was copied.
 *
 * A component, and rendered twice, because of ART-202: once beside the fields
 * the refusal is about, once beside the button that asked for it.
 */
function Refusal({ text, testId }: { text: string; testId: string }) {
  const { t } = useTranslation();
  return (
    <div
      className="badge badge-err"
      data-testid={testId}
      style={{ display: "block", padding: "8px 10px", margin: "0 0 12px", fontSize: 12 }}
    >
      <p style={{ margin: "0 0 6px", fontWeight: 600 }}>
        {t("osinstall.amigaInstall.refused.heading")}
      </p>
      <p style={{ margin: "0 0 6px" }}>{text}</p>
      <p style={{ margin: 0, fontSize: 11 }}>
        {t("osinstall.amigaInstall.refused.nothingCopied")}
      </p>
    </div>
  );
}

export function AmigaInstallPanel({
  treeRoot,
  onTreeRootChange,
  packageFolder = null,
}: AmigaInstallPanelProps) {
  const { t } = useTranslation();
  const power = usePowerMode();
  const winuaePath = useSettingsStore((s) => s.settings.winuaePath);

  const [packageId, setPackageId] = useRemembered<string | null>(
    "amigaInstall.package",
    isTextOrNothing,
    null
  );
  const [archive, setArchive] = useRemembered<string | null>(
    "amigaInstall.archive",
    isTextOrNothing,
    null
  );
  const [overlayArchive, setOverlayArchive] = useRemembered<string | null>(
    "amigaInstall.overlayArchive",
    isTextOrNothing,
    null
  );
  const [kickstart, setKickstart] = useRemembered<string | null>(
    "amigaInstall.kickstart",
    isTextOrNothing,
    null
  );
  /** The user's own copy of the disc a package's installer verifies
   *  (ART-193). Remembered like every other choice on this screen: nothing
   *  the user chose resets itself between runs. */
  const [medium, setMedium] = useRemembered<string | null>(
    "amigaInstall.medium",
    isTextOrNothing,
    null
  );

  const [catalogue, setCatalogue] = useState<PackageSummary[] | null>(null);
  const [catalogueError, setCatalogueError] = useState(false);
  const [preview, setPreview] = useState<AmigaInstallPreview | null>(null);
  /** A refusal, exactly as Rust wrote it (ART-060). Never folded into one
   *  translated sentence: which reason applies is the whole content. */
  const [refusal, setRefusal] = useState<string | null>(null);
  const [previewing, setPreviewing] = useState(false);
  const [confirmed, setConfirmed] = useState(false);

  const job = useRef<number | null>(null);
  const [busy, setBusy] = useState(false);
  const [progress, setProgress] = useState<JobProgress | null>(null);
  const [jobError, setJobError] = useState<string | null>(null);
  const [wasCancelled, setWasCancelled] = useState(false);
  /**
   * ART's own last word about the run, taken off the **terminal** job event.
   *
   * This is fix round 1's Major, and it is the round's signature defect
   * arriving through a door nobody had checked. A run that goes wrong
   * *mid-flight* — not one of the four endings — leaves the copy on disk by
   * design, and `commands/amigainstall.rs::perform` reports
   * `"'<original>' was not touched; the copy ART installed into is at
   * '<copy>'"` immediately before returning the error. That sentence travels
   * as a `job-progress` **message**, and this panel used to render only
   * `state.message` + the error code — so on the one path where a copy really
   * is orphaned, the user was told it failed and never told where the
   * evidence went. Exactly the thing the four endings were made distinct to
   * prevent.
   *
   * It cannot be lost to the event throttle: `JobRegistry::update` always
   * stores the message even when it does not emit, `finish` replaces only the
   * *state*, and the terminal event ignores the throttle — so the last thing
   * reported always arrives, on the same event the error does.
   *
   * Rendered for a failure **and** for a cancellation. The cancelled path has
   * its own instance of the same defect: `perform` reports when a cancelled
   * run's copy could **not** be removed, and the screen used to answer that
   * with a flat translated "the copy has been discarded" — a wrong sentence
   * about litter still sitting on the user's disk. That sentence no longer
   * claims anything about the copy; ART's own line says what happened to it.
   *
   * The trade-off, stated rather than hidden: on an ordinary cancellation
   * nothing new is reported at the end, so what shows is whatever the run
   * last said (a staging line). True, and less informative than silence would
   * be. Closing that properly means a `report` on the successful discard in
   * Rust, which is not this task's to change.
   */
  const [lastReported, setLastReported] = useState<string | null>(null);
  const [result, setResult] = useState<AmigaInstallResult | null>(null);

  // The archives, wrapper first. The order is the wire's own: everything
  // after the first is an overlay medium, matched by what it carries.
  const archives = useMemo(
    () => [archive, overlayArchive].filter((path): path is string => path !== null && path !== ""),
    [archive, overlayArchive]
  );

  // The disc is **not** part of the "have you chosen enough to preview"
  // test. Whether this package needs one is the recipe's answer, not this
  // screen's, and Rust refuses by name when it is required and missing — a
  // sentence that says which disc and which volume. Requiring it here would
  // replace that with a silent grey panel for every package, including the
  // ones that need no disc at all.
  const request: AmigaInstallRequest | null =
    treeRoot && packageId && archive && kickstart
      ? { tree: treeRoot, packageId, packageArchives: archives, kickstart, medium }
      : null;

  // The catalogue. Loaded whenever the package folder changes, `null` (never
  // `[]`) until something arrives, so "not loaded yet" and "loaded and empty"
  // stay different states — the distinction `OsInstall.tsx` already draws.
  useEffect(() => {
    if (!packageFolder) {
      setCatalogue(null);
      setCatalogueError(false);
      return;
    }
    let cancelled = false;
    osinstallPackages(packageFolder)
      .then((list) => {
        if (!cancelled) {
          setCatalogue(list);
          setCatalogueError(false);
        }
      })
      .catch(() => {
        if (!cancelled) {
          setCatalogue(null);
          setCatalogueError(true);
        }
      });
    return () => {
      cancelled = true;
    };
  }, [packageFolder]);

  // §92's PREVIEW: read-only, recomputed whenever the request changes, and
  // the place every refusal lands — `compose` is shared with the run, so a
  // prerequisite the tree does not have is refused here, before a confirm
  // button is ever offered and before one byte is copied.
  //
  // The `cancelled` guard is ART-089's, the same one every effect on
  // `OsInstall.tsx` carries: a late-landing answer must not overwrite what
  // the user has since chosen.
  useEffect(() => {
    setConfirmed(false);
    // A report describes the run that was on screen when it finished; once
    // any part of the request changes it is about something else.
    setResult(null);
    setJobError(null);
    setWasCancelled(false);
    setLastReported(null);
    if (!request) {
      setPreview(null);
      setRefusal(null);
      return;
    }
    let cancelled = false;
    setPreviewing(true);
    amigaInstallPreview(request, winuaePath)
      .then((answer) => {
        if (cancelled) return;
        setPreview(answer);
        setRefusal(null);
        setPreviewing(false);
      })
      .catch((e) => {
        if (cancelled) return;
        setPreview(null);
        setRefusal(String(e));
        setPreviewing(false);
      });
    return () => {
      cancelled = true;
    };
    // `request` is rebuilt on every render; its four parts are the identity.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [treeRoot, packageId, archive, overlayArchive, kickstart, winuaePath]);

  // The job's own progress. `job-progress` is application-wide, so every
  // update is checked against this panel's job id first.
  useEffect(() => {
    return subscribeSafely(() =>
      onJobProgress((update) => {
        if (update.id !== job.current) return;
        setProgress(update);
        if (update.state.state === "running") return;

        job.current = null;
        setBusy(false);
        // Before anything else: whatever ART reported last. See
        // `lastReported` — on a mid-run failure this is the sentence naming
        // where the copy was left.
        setLastReported(update.message.trim() === "" ? null : update.message);
        if (update.state.state === "failed") {
          setJobError(`${update.state.message} (${update.state.error_code})`);
        } else if (update.state.state === "cancelled") {
          setWasCancelled(true);
        }
        // A clean finish says nothing here: the run's own answer — which of
        // the four endings, and what happened to the copy — arrives on
        // `AMIGA_INSTALL_EVENT` below, and *that* is the report.
      })
    );
  }, []);

  useEffect(() => {
    return subscribeSafely(() =>
      onAmigaInstallResult((answer) => {
        // The run's answer is emitted from inside the job closure, so it
        // always arrives before the runner's own terminal progress event
        // clears `job.current` — the same ordering `PackagePanel` relies on.
        if (answer.job_id !== job.current) return;
        setResult(answer);
        setConfirmed(false);
      })
    );
  }, []);

  async function chooseTreeRoot() {
    const picked = await open({
      directory: true,
      multiple: false,
      title: t("osinstall.packages.treeRoot.chooseTitle"),
    });
    if (typeof picked === "string") onTreeRootChange?.(picked);
  }

  async function chooseArchive(set: (path: string | null) => void, title: string) {
    const picked = await open({
      multiple: false,
      title,
      defaultPath: packageFolder ?? undefined,
      filters: [{ name: "Package archive", extensions: ["lha", "lzh", "zip", "7z"] }],
    });
    if (typeof picked === "string") set(picked);
  }

  async function chooseMedium() {
    const picked = await open({
      multiple: false,
      title: t("osinstall.amigaInstall.medium.chooseTitle"),
      filters: [{ name: "Disc image", extensions: ["iso", "cue", "bin", "img"] }],
    });
    if (typeof picked === "string") setMedium(picked);
  }

  async function chooseKickstart() {
    const picked = await open({
      multiple: false,
      title: t("osinstall.amigaInstall.kickstart.chooseTitle"),
      filters: [{ name: "Kickstart ROM", extensions: ["rom", "bin"] }],
    });
    if (typeof picked === "string") setKickstart(picked);
  }

  async function runInstall() {
    if (!request) return;
    setBusy(true);
    setJobError(null);
    setWasCancelled(false);
    setLastReported(null);
    setProgress(null);
    setResult(null);
    try {
      job.current = await amigaInstallRun(request, winuaePath);
    } catch (e) {
      // Every refusal is raised before the job starts, so this is the same
      // English sentence the preview would have shown — never a job that
      // could only have gone red a moment later.
      setRefusal(String(e));
      setBusy(false);
      job.current = null;
    }
  }

  const runnable = (catalogue ?? []).filter((p) => p.amigaInstallable);
  const nameOf = (id: string) => catalogue?.find((p) => p.id === id)?.name ?? id;
  const blockers = preview ? readinessBlockers(preview) : [];
  const overlayAdvice = preview ? overlayAdvicePhrase(preview) : null;
  const pct = progress ? fraction(progress) : null;
  const outcome = result ? outcomePhrase(result.outcome) : null;
  const nextStep = result ? outcomeNextStepPhrase(result.outcome) : null;
  const settlement = result ? settlementPhrase(result.settlement) : null;
  const tone = result ? outcomeTone(result.outcome) : null;

  return (
    <section className="card" style={{ marginBottom: 16 }}>
      <h2 style={{ fontSize: 16, marginTop: 0 }}>{t("osinstall.amigaInstall.heading")}</h2>
      <p className="muted" style={{ fontSize: 12, margin: "4px 0 12px" }}>
        {t("osinstall.amigaInstall.intro")}
      </p>

      {/* Before anything else, and in both UX modes: a machine window is
          about to appear on this desktop. The last round opened one without
          saying so and that was a real annoyance. */}
      <p
        className="badge badge-warn"
        data-testid="emulator-window-warning"
        style={{ display: "block", padding: "6px 12px", fontSize: 12, marginBottom: 12 }}
      >
        {t("osinstall.amigaInstall.emulatorWindow")}
      </p>
      <p className="faint" style={{ fontSize: 11, margin: "0 0 8px" }}>
        {t("osinstall.amigaInstall.copyNote")}
      </p>
      <p className="faint" style={{ fontSize: 11, margin: "0 0 12px" }}>
        {t("osinstall.amigaInstall.chainNote")}
      </p>

      <Field
        label={t("osinstall.packages.treeRoot.label")}
        value={treeRoot}
        empty={t("osinstall.packages.treeRoot.none")}
        onChoose={() => void chooseTreeRoot()}
        choose={t("common.browse")}
        hint={t("osinstall.packages.treeRoot.hint")}
      />

      <div className="muted" style={{ fontSize: 12, marginBottom: 4 }}>
        {t("osinstall.amigaInstall.package.label")}
      </div>
      {catalogueError && (
        <p className="badge badge-err" style={{ fontSize: 11, display: "inline-block" }}>
          {t("osinstall.amigaInstall.package.unavailableHint")}
        </p>
      )}
      {!packageFolder && !catalogueError && (
        <p className="faint" style={{ fontSize: 11, margin: "0 0 12px" }}>
          {t("osinstall.amigaInstall.package.needsFolder")}
        </p>
      )}
      {catalogue !== null && runnable.length === 0 && (
        <p className="faint" style={{ fontSize: 11, margin: "0 0 12px" }}>
          {t("osinstall.amigaInstall.package.none")}
        </p>
      )}
      {runnable.map((pkg) => (
        <label
          key={pkg.id}
          data-testid="amiga-package-row"
          style={{ display: "flex", gap: 8, alignItems: "baseline", fontSize: 12, padding: "3px 0" }}
        >
          <input
            type="radio"
            name="amiga-install-package"
            checked={packageId === pkg.id}
            onChange={() => setPackageId(pkg.id)}
          />
          <span>
            {pkg.name}
            {pkg.requires.length > 0 && (
              <span className="faint" style={{ fontSize: 11, marginLeft: 6 }}>
                {t("osinstall.packages.requiresPackages", {
                  list: pkg.requires.map(nameOf).join(", "),
                })}
              </span>
            )}
          </span>
        </label>
      ))}

      <div style={{ marginTop: 12 }}>
        <Field
          label={t("osinstall.amigaInstall.archive.label")}
          value={archive}
          empty={t("osinstall.amigaInstall.archive.none")}
          onChoose={() =>
            void chooseArchive(setArchive, t("osinstall.amigaInstall.archive.chooseTitle"))
          }
          choose={t("common.browse")}
          hint={t("osinstall.amigaInstall.archive.hint")}
        />
        <Field
          label={t("osinstall.amigaInstall.overlayArchive.label")}
          value={overlayArchive}
          empty={t("osinstall.amigaInstall.overlayArchive.none")}
          onChoose={() =>
            void chooseArchive(
              setOverlayArchive,
              t("osinstall.amigaInstall.overlayArchive.chooseTitle")
            )
          }
          choose={t("common.browse")}
          hint={t("osinstall.amigaInstall.overlayArchive.hint")}
          clear={overlayArchive ? t("common.clear") : undefined}
          onClear={overlayArchive ? () => setOverlayArchive(null) : undefined}
        />
        <Field
          label={t("osinstall.amigaInstall.kickstart.label")}
          value={kickstart}
          empty={t("osinstall.amigaInstall.kickstart.none")}
          onChoose={() => void chooseKickstart()}
          choose={t("common.browse")}
          hint={t("osinstall.amigaInstall.kickstart.hint")}
        />
        {/* ART-193. Optional on the screen because it is optional for some
            packages; the refusal above says when it is not. */}
        <Field
          label={t("osinstall.amigaInstall.medium.label")}
          value={medium}
          empty={t("osinstall.amigaInstall.medium.none")}
          onChoose={() => void chooseMedium()}
          choose={t("common.browse")}
          hint={t("osinstall.amigaInstall.medium.hint")}
          clear={medium ? t("common.clear") : undefined}
          onClear={medium ? () => setMedium(null) : undefined}
        />
      </div>

      {/* The refusal. English, from Rust (ART-060), verbatim: a missing
          prerequisite names what to install first and in what order, and an
          installer too old for an emulator names the archive that fixes it.
          One translated line goes with it, saying the half ART can say in
          the user's own language — that nothing was copied. */}

      {!request && !refusal && (
        <p className="faint" style={{ fontSize: 11, margin: "0 0 12px" }}>
          {t("osinstall.amigaInstall.preview.needsChoices")}
        </p>
      )}
      {previewing && (
        <p className="faint" style={{ fontSize: 11, margin: "0 0 12px" }}>
          {t("osinstall.amigaInstall.preview.loading")}
        </p>
      )}

      {preview && (
        <div
          data-testid="amiga-install-preview"
          style={{
            border: "1px solid var(--border)",
            borderRadius: 4,
            padding: "8px 10px",
            marginBottom: 12,
          }}
        >
          <div className="muted" style={{ fontSize: 12, fontWeight: 600, marginBottom: 6 }}>
            {t("osinstall.amigaInstall.preview.heading")}
          </div>
          <div style={{ fontSize: 12 }}>
            {t("osinstall.amigaInstall.preview.package")}: {preview.packageName}
          </div>
          <div style={{ fontSize: 12, wordBreak: "break-all" }}>
            {t("osinstall.amigaInstall.preview.tree")}: {preview.tree}
          </div>
          {preview.emulator && (
            <div style={{ fontSize: 12, wordBreak: "break-all" }}>
              {t("osinstall.amigaInstall.preview.emulator")}: {preview.emulator}
            </div>
          )}
          {/* ART-193. Not in the power-mode block below: a disc going into
              the machine is something the run does, like the machine window
              itself, and design §4 says a person should not be surprised by
              it. The volume shown is the one the **image itself states** —
              read from the image, never from its filename. */}
          {preview.medium && (
            <div style={{ fontSize: 12, wordBreak: "break-all" }}>
              {t("osinstall.amigaInstall.preview.medium", {
                volume: preview.mediumVolume ?? "",
              })}
              : {preview.medium}
            </div>
          )}
          <p className="faint" style={{ fontSize: 11, margin: "6px 0 0" }}>
            {t("osinstall.amigaInstall.preview.deadline", {
              minutes: Math.round(preview.deadlineSeconds / 60),
            })}
          </p>
          {/* Beginner mode only *hides* (§47/§48): the machine, the AmigaDOS
              command line, the three volume names and the result file are
              detail a beginner cannot act on. Nothing above is hidden, and
              nothing ART does changes with the mode. */}
          {power && (
            <div data-testid="amiga-install-detail" style={{ marginTop: 6 }}>
              <div style={{ fontSize: 12 }}>
                {t("osinstall.amigaInstall.preview.machine")}: {preview.profileName}
              </div>
              <div style={{ fontSize: 12, wordBreak: "break-all" }}>
                {t("osinstall.amigaInstall.preview.program")}:{" "}
                <code>{[preview.program, ...preview.args].join(" ")}</code>
              </div>
              <p className="faint" style={{ fontSize: 11, margin: "6px 0 0" }}>
                {t("osinstall.amigaInstall.preview.volumes", {
                  system: preview.systemVolume,
                  package: preview.packageVolume,
                  work: preview.workVolume,
                })}
              </p>
              <p className="faint" style={{ fontSize: 11, margin: "4px 0 0" }}>
                {t("osinstall.amigaInstall.preview.resultFile", { file: preview.resultFile })}
              </p>
            </div>
          )}
        </div>
      )}

      {/* ART-186, and the obligation task 4's review handed this screen: a
          refusal the user can fix with one download must be visible before
          the run, naming the archive to go and get. */}
      {overlayAdvice && (
        <p
          className="badge badge-warn"
          data-testid="amiga-install-overlay-advice"
          style={{ display: "block", padding: "6px 12px", fontSize: 12, marginBottom: 12 }}
        >
          {t(overlayAdvice.key, overlayAdvice.params)}
        </p>
      )}

      {blockers.length > 0 && (
        <div
          className="badge badge-err"
          data-testid="amiga-install-blockers"
          style={{ display: "block", padding: "8px 10px", margin: "0 0 12px", fontSize: 12 }}
        >
          <p style={{ margin: "0 0 6px", fontWeight: 600 }}>
            {t("osinstall.amigaInstall.blocker.heading")}
          </p>
          <ul style={{ margin: 0, paddingLeft: 18 }}>
            {blockers.map((blocker) => (
              <li key={blocker.key} style={{ padding: "2px 0", wordBreak: "break-all" }}>
                {t(blocker.key, blocker.params)}
              </li>
            ))}
          </ul>
        </div>
      )}

      <label
        style={{ display: "flex", gap: 8, alignItems: "center", fontSize: 12, margin: "0 0 10px" }}
      >
        <input
          type="checkbox"
          checked={confirmed}
          disabled={!preview || blockers.length > 0}
          onChange={(e) => setConfirmed(e.target.checked)}
        />
        {t("osinstall.amigaInstall.confirm")}
      </label>

      {jobError && (
        <div
          className="badge badge-err"
          data-testid="amiga-install-job-error"
          style={{ display: "block", padding: "8px 10px", fontSize: 12, marginBottom: 12 }}
        >
          <p style={{ margin: 0 }}>{jobError}</p>
          {/* The Major of fix round 1: ART's own last word, which on a
              mid-run failure names the tree it did not touch and the copy it
              left behind. English, from Rust, verbatim (ART-060) — the same
              rule the refusal above follows, and for the same reason: which
              sentence it is *is* the content. */}
          {lastReported && (
            <p
              data-testid="amiga-install-last-reported"
              style={{ margin: "6px 0 0", wordBreak: "break-all" }}
            >
              {lastReported}
            </p>
          )}
        </div>
      )}
      {wasCancelled && (
        <div
          className="badge badge-warn"
          data-testid="amiga-install-cancelled"
          style={{ display: "block", padding: "8px 10px", fontSize: 12, marginBottom: 12 }}
        >
          {/* This sentence deliberately claims nothing about the copy. It
              used to say the copy had been discarded, which is false on the
              one cancelled path where the discard itself fails — and that
              path is precisely the one ART reports on. */}
          <p style={{ margin: 0 }}>{t("osinstall.amigaInstall.cancelled")}</p>
          {lastReported && (
            <p
              data-testid="amiga-install-last-reported"
              style={{ margin: "6px 0 0", wordBreak: "break-all" }}
            >
              {lastReported}
            </p>
          )}
        </div>
      )}

      {/* The report. Four endings, four sentences, four next steps — and
          always where the tree and the copy are now, because "it failed"
          without that has told the user nothing. */}
      {result && outcome && nextStep && settlement && (
        <div
          data-testid="amiga-install-report"
          className={tone === "ok" ? "badge badge-ok" : tone === "err" ? "badge badge-err" : "badge badge-warn"}
          style={{ display: "block", padding: "8px 10px", margin: "0 0 12px", fontSize: 12 }}
        >
          <p style={{ margin: "0 0 6px", fontWeight: 600 }}>
            {t("osinstall.amigaInstall.report.heading")}
          </p>
          <p data-testid="amiga-install-outcome" style={{ margin: "0 0 6px" }}>
            {t(outcome.key, outcome.params)}
          </p>
          <p data-testid="amiga-install-settlement" style={{ margin: "0 0 6px", wordBreak: "break-all" }}>
            {t(settlement.key, settlement.params)}
          </p>
          <p data-testid="amiga-install-next" style={{ margin: 0 }}>
            {t(nextStep.key, nextStep.params)}
          </p>
        </div>
      )}

      {/* ART-202: the refusal, **once**, where the button is.
          It used to render only at the top of the panel — 209 lines of JSX
          above this control — so on a maximised window pressing the button
          changed nothing the reader could see, and the honest conclusion
          available to them was that it had done nothing. The owner's operation
          log recorded seven identical runs of an unchanged request.
          `OsInstall.tsx` already carried this lesson in the owner's own words:
          a job that ended badly has to say so where the button is.

          The first fix rendered it in **both** places, and the owner read the
          result as two separate errors — *"aynı uyarı tek ekranda 2 tane"*.
          They were right, and it is cheap to say why: a refusal means the
          preview did not succeed, so there is no preview card and the panel is
          short. The two boxes land within a screen of each other and duplicate
          rather than reassure. One box, at the control, is what the rule
          actually asks for. */}
      {refusal && <Refusal text={refusal} testId="amiga-install-refusal" />}

      <div style={{ display: "flex", gap: 8, alignItems: "center" }}>
        <button
          className="btn btn-primary"
          onClick={() => void runInstall()}
          disabled={busy || !confirmed || !preview || blockers.length > 0}
        >
          {t(busy ? "osinstall.amigaInstall.running" : "osinstall.amigaInstall.run")}
        </button>
        {busy && (
          <span className="faint" style={{ fontSize: 11 }}>
            {pct === null
              ? t("osinstall.packages.apply.progressStarting")
              : t("osinstall.packages.apply.progressPercent", {
                  percent: Math.round(pct * 100),
                  done: progress?.done ?? 0,
                  total: progress?.total ?? 0,
                })}
            {/* The phase ART is in, as ART reports it — English, from Rust
                (ART-060). The same question as the Major, asked of the
                *running* half of this channel: an install takes minutes and
                the emulator's window is the only other sign of life, so the
                line ART is already writing should not be thrown away. */}
            {progress?.message?.trim() ? (
              <span data-testid="amiga-install-phase" style={{ marginLeft: 8 }}>
                {progress.message}
              </span>
            ) : null}
          </span>
        )}
      </div>
    </section>
  );
}
