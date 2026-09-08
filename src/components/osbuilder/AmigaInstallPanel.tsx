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

import { useBuildSession } from "@/lib/useBuildSession";

// ART-060: a Rust sentence ART recognises comes back in the user's own
// language; anything else is Rust's English verbatim, exactly as before.
import { errorText } from "@/lib/errorText";

import {
  amigaInstallArchiveKey,
  amigaInstallPreview,
  amigaInstallRun,
  amigainstallClassifyArchive,
  archiveFieldBlockerPhrase,
  dedupeBlockers,
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
  type ArchiveClassification,
} from "@/lib/amigainstall";
import {
  fileName,
  osinstallPackages,
  osinstallSlots,
  type InstallRelease,
  type PackageSummary,
  type SlotReport,
  type SlotState,
} from "@/lib/osinstall";
import { candidateLines, displayName, slotLines } from "@/lib/slots";
import type { Phrase } from "@/lib/phrase";
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
  /**
   * **Every folder this build's material is in** (design § 3.1), in list
   * order — what the slots are resolved against.
   *
   * Separate from `packageFolder` above, which is one folder and stays one:
   * `PackagePanel`'s own `osinstallCollisions`/`osinstallAddPackage` take a
   * single folder and this panel's dialogs need a place to open. What this
   * carries is the *question* — "given everything the user has, which file is
   * BoingBag 3.9-1?" — and it is a list because that is what the answer has
   * to be resolved over. A folder the user removed from the list therefore
   * stops filling these fields, which is the whole of design § 3.4.
   */
  materialFolders?: string[];
  /** Which AmigaOS release this build is for — the packages offered are a
   *  function of it (ART-209). This panel runs a package's own installer on
   *  the Amiga, and a BoingBag is AmigaOS 3.9's; offering one on a 3.2 build
   *  is offering to run an installer for an operating system that is not
   *  there. */
  release: InstallRelease;
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

/**
 * One artefact this panel needs, **as the slots already resolved it** (design
 * § 3.4).
 *
 * The panel used to ask three browse buttons three separate questions the
 * user had to answer from memory — which of the files in their downloads
 * folder is "the package's own archive", which is "its update archive", which
 * is "the disc the installer checks". `core::osinstall::slots` answers all
 * three from the material folders, so the field's job changes: it shows what
 * ART found and gets out of the way, and only becomes a question again when
 * ART cannot answer it.
 *
 * **Four states, and they stay four** — this screen's own rule, one control
 * further in:
 *
 *   - *filled*, by a find or by the user's own choice — a read-only line
 *     carrying the readout's own sentence for that row, so the panel and the
 *     `kaynak` step cannot say two different things about one file;
 *   - *ambiguous* — every candidate listed with its own evidence, and the
 *     user picks. ART never picks (design § 4);
 *   - *not found* — the browse row, with what ART expected as its hint;
 *   - *not needed* — the slot's own measured sentence, and no field at all.
 *
 * **The override always wins, and it survives a re-scan.** A path the user
 * picked by hand is a decision (`amigaInstallArchiveKey`, ART-277), and a
 * later answer from the resolver may never overwrite one — CLAUDE.md's
 * "nothing changes unless the user changes it", which is why *Use the found
 * one* is a button rather than something ART does when it learns more.
 */
function SlotField({
  testId,
  label,
  empty,
  hint,
  state,
  override,
  found,
  onChoose,
  onOverride,
  onUseFound,
  onClear,
}: {
  testId: string;
  label: string;
  /** What to say when the field is empty and ART knows of no slot at all. */
  empty: string;
  /** What ART expected here — filenames and provenance, already translated
   *  by the caller from that field's own `*.hint` key so the key stays a
   *  literal a test can check. `undefined` when there is no slot to expect
   *  anything from. */
  hint?: string;
  /** The resolved slot, or `null` when nothing has answered — no material
   *  folders, or the call failed. A `null` here renders exactly the browse
   *  row this panel always had. */
  state: SlotState | null;
  /** The path the user picked by hand for this field, when they did. */
  override: string | null;
  /** The path the slot fills the field with — a *find*, never a guess. */
  found: string | null;
  onChoose: () => void;
  onOverride: (path: string) => void;
  /** Drop the override and go back to what ART found. `forget` on the
   *  remembered key, and only ever from the user's own click. */
  onUseFound: () => void;
  onClear?: () => void;
}) {
  const { t } = useTranslation();

  // Nothing has answered. The browse row, exactly as before — a panel usable
  // with a hand-picked archive is the state ART-212 already ruled must keep
  // working, and "we have not asked" is not "the answer is no".
  if (!state) {
    return (
      <Field
        label={label}
        ariaLabel={label}
        testId={testId}
        value={override}
        empty={empty}
        hint={hint}
        onChoose={onChoose}
        choose={t("common.browse")}
        clear={override && onClear ? t("common.clear") : undefined}
        onClear={override && onClear ? onClear : undefined}
      />
    );
  }

  const name = displayName(state);

  /** The sentence a filled field carries, and `null` when it is not filled.
   *  Three producers, never folded into one: the user's own choice, ART's
   *  measurement that nobody has to obtain this at all, and the readout's own
   *  row for the file ART identified. */
  const filled: Phrase | null = override
    ? { key: "osinstall.slots.chosen", params: { file: fileName(override), name } }
    : state.notNeeded
      ? { key: "osinstall.slots.notNeeded", params: { name, carries: state.notNeeded } }
      : found
        ? slotLines([state])[0].phrase
        : null;

  if (filled) {
    return (
      <div data-testid={testId} style={{ margin: "0 0 10px" }}>
        <div className="muted" style={{ fontSize: 12 }}>
          {label}
        </div>
        <p style={{ fontSize: 12, margin: "2px 0 0", wordBreak: "break-all" }}>
          {t(filled.key, filled.params)}
        </p>
        <div style={{ display: "flex", gap: 8, marginTop: 4, flexWrap: "wrap" }}>
          <button
            className="btn"
            style={{ fontSize: 11 }}
            data-testid={`${testId}-choose-another`}
            // ART-240: three of these buttons render on one screen, and a
            // screen reader user tabbing through would otherwise hear the
            // same three words with nothing to tell them apart.
            aria-label={t("osinstall.amigaInstall.slotField.chooseAnotherAriaLabel", { label })}
            onClick={onChoose}
          >
            {t("osinstall.amigaInstall.slotField.chooseAnother")}
          </button>
          {override && found && (
            <button
              className="btn"
              style={{ fontSize: 11 }}
              data-testid={`${testId}-use-found`}
              aria-label={t("osinstall.amigaInstall.slotField.useFoundAriaLabel", { label })}
              onClick={onUseFound}
            >
              {t("osinstall.amigaInstall.slotField.useFound")}
            </button>
          )}
          {override && !found && onClear && (
            <button
              className="btn"
              style={{ fontSize: 11 }}
              data-testid={`${testId}-clear`}
              aria-label={t("osinstall.amigaInstall.slotField.clearAriaLabel", { label })}
              onClick={onClear}
            >
              {t("common.clear")}
            </button>
          )}
        </div>
      </div>
    );
  }

  // Two or more files could be this artefact. **ART does not choose** (design
  // § 4), and neither does the readout — it says so and stops. Here the user
  // can settle it, and picking one is exactly the same act as browsing to it:
  // it sets the override.
  if (state.candidates.length > 1) {
    const candidates = candidateLines(state);
    return (
      <div data-testid={testId} style={{ margin: "0 0 10px" }}>
        <div className="muted" style={{ fontSize: 12 }}>
          {label}
        </div>
        <p style={{ fontSize: 11, margin: "2px 0 4px" }}>
          {t("osinstall.amigaInstall.slotField.pickOne", { name, count: candidates.length })}
        </p>
        {candidates.map((candidate) => (
          <label
            key={candidate.path}
            data-testid={`${testId}-candidate`}
            style={{ display: "flex", gap: 8, alignItems: "baseline", fontSize: 12, padding: "2px 0" }}
          >
            <input
              type="radio"
              name={`${testId}-candidates`}
              checked={false}
              onChange={() => onOverride(candidate.path)}
            />
            <span style={{ wordBreak: "break-all" }}>
              <code>{candidate.path}</code>
              <span className="faint" style={{ marginLeft: 6 }}>
                {t(candidate.phrase.key, candidate.phrase.params)}
              </span>
            </span>
          </label>
        ))}
      </div>
    );
  }

  // Not found. The browse row, and the hint is now what ART actually expected
  // rather than a sentence somebody wrote about what the field is for.
  return (
    <Field
      label={label}
      ariaLabel={label}
      testId={testId}
      value={null}
      empty={empty}
      hint={hint}
      onChoose={onChoose}
      choose={t("common.browse")}
    />
  );
}

export function AmigaInstallPanel({
  treeRoot,
  onTreeRootChange,
  packageFolder = null,
  materialFolders = [],
  release,
}: AmigaInstallPanelProps) {
  const { t } = useTranslation();
  const { session, setRom } = useBuildSession();
  const power = usePowerMode();
  const winuaePath = useSettingsStore((s) => s.settings.winuaePath);

  const [packageId, setPackageId] = useRemembered<string | null>(
    "amigaInstall.package",
    isTextOrNothing,
    null
  );
  /**
   * The two archives, **scoped per package** (ART-277). `useRemembered`
   * takes its key as a plain argument re-read on every render, so a key that
   * changes with `packageId` is exactly what it already supports — no
   * lower-level `@/lib/remembered` call is needed. Switching the radio to
   * BoingBag 3.9-2 therefore reads BoingBag 3.9-2's own remembered archive
   * (empty the first time), while BoingBag 3.9-1's stays exactly where it
   * was under its own key. See `amigaInstallArchiveKey`'s own comment for
   * the defect this closes.
   */
  //
  // **Round 2 § 3.4: these are now the *override*, not the answer.** The
  // fields are filled from the slots; a path here is one the user picked by
  // hand, it wins over whatever ART resolved, and it survives a re-scan
  // because nothing but the user's own click ever writes or drops it. That
  // is why the third element — `forget` — is taken: setting `null` would
  // store a decision ("no file"), and the found file could then never fill
  // the field again.
  const [archive, setArchive, forgetArchive] = useRemembered<string | null>(
    amigaInstallArchiveKey("amigaInstall.archive", packageId),
    isTextOrNothing,
    null
  );
  const [overlayArchive, setOverlayArchive, forgetOverlayArchive] = useRemembered<string | null>(
    amigaInstallArchiveKey("amigaInstall.overlayArchive", packageId),
    isTextOrNothing,
    null
  );
  /**
   * **One Kickstart for the build** (ART-197's fourth row, wave 2).
   *
   * This panel kept its own remembered key until 2026-08-23, so a user chose
   * the same ROM here, on the install step and on the card step. They are one
   * question wearing three labels — the ROM the tree is paired against, the
   * ROM the emulator boots, and the ROM written onto the card — and a build
   * where they differ is the mismatch G9's pairing check exists to catch.
   *
   * Changing it here changes it for the build, which the hint says out loud:
   * a carry the user cannot see is the same defect as one that never
   * happened (ART-197's own words).
   */
  const kickstart = session.rom.path;
  const setKickstart = setRom;
  /**
   * The user's own copy of the disc a package's installer verifies
   * (ART-193). Remembered like every other choice on this screen: nothing
   * the user chose resets itself between runs.
   *
   * **Deliberately global, unlike `archive`/`overlayArchive` (ART-277).**
   * The AmigaOS 3.9 CD image is one fact about the *build*, not about which
   * package is selected — both BoingBags verify the same `AmigaOS3.9:`
   * volume — so scoping this per package would make the owner re-browse to
   * the same file for BoingBag 3.9-2 having just given it for BoingBag
   * 3.9-1, which is exactly the annoyance this module exists to prevent.
   */
  const [medium, setMedium, forgetMedium] = useRemembered<string | null>(
    "amigaInstall.medium",
    isTextOrNothing,
    null
  );

  const [catalogue, setCatalogue] = useState<PackageSummary[] | null>(null);
  const [catalogueError, setCatalogueError] = useState(false);
  /**
   * What Rust says each archive field actually holds, asked the moment it is
   * picked (or restored) rather than only discovered after the round trip
   * through `compose` (ART-277). `null` means "not asked, or nothing to ask
   * about" — never "wrong": a field with nothing in it is not misclassified,
   * it is empty.
   */
  const [archiveClassification, setArchiveClassification] = useState<ArchiveClassification | null>(
    null
  );
  const [overlayClassification, setOverlayClassification] =
    useState<ArchiveClassification | null>(null);
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

  /**
   * **What the material folders turn out to hold** (design § 3.4) — the same
   * single answer the `kaynak` step's readout renders, asked again here
   * because this panel is the other half of the same question.
   *
   * Asked **even with no folders at all**: `slots_for` is derived from the
   * recipes and is pure, so an empty list still answers with every slot this
   * release has, its expected filenames and its provenance — which is exactly
   * what an empty field's hint and a missing-artefact blocker need to say.
   * The alternative (skip the call, render no hint) would make ART silent
   * about a fact it holds regardless of where the user has pointed it.
   *
   * `null` means the question has not been answered — not that nothing was
   * found. Every field falls back to the plain browse row on `null`, so a
   * failed call costs the hints and nothing else.
   */
  const [slotReport, setSlotReport] = useState<SlotReport | null>(null);
  // A primitive dependency rather than the array: two equal strings are the
  // same value to React's own comparison, two equal arrays are not, and this
  // effect starts disk work (ART-178/ART-195).
  const materialKey = materialFolders.join("\n");
  useEffect(() => {
    const list = materialKey ? materialKey.split("\n") : [];
    let cancelled = false;
    osinstallSlots(release, list, treeRoot, kickstart)
      .then((answer) => {
        if (!cancelled) setSlotReport(answer);
      })
      .catch(() => {
        // Silent, and deliberately: this is an *enhancement* to a panel that
        // works without it. A red box here would report a fault in ART as
        // though it were a statement about the user's files — which is
        // exactly what the readout's own `readoutFailed` line exists to keep
        // apart, on the screen that is about the folders.
        if (!cancelled) setSlotReport(null);
      });
    return () => {
      cancelled = true;
    };
  }, [release, materialKey, treeRoot, kickstart]);

  const slotStates = slotReport?.states ?? [];
  /** The chosen package's own slot. */
  const packageSlot =
    (packageId && slotStates.find((state) => state.slot.id === `package:${packageId}`)) || null;
  /** Its overlay, when the recipe declares one — the id shape is
   *  `overlay:<package>:<drawer>`, so the package's own prefix is what names
   *  it without this screen having to know the drawer. */
  const overlaySlot =
    (packageId &&
      slotStates.find(
        (state) => state.slot.kind === "overlay" && state.slot.id.startsWith(`overlay:${packageId}:`)
      )) ||
    null;
  /**
   * The disc this package's installer verifies — **read off the package
   * slot's own `requires`**, never a `medium:AmigaOS3.9` written here.
   *
   * `slots_for` turns a recipe's `required_medium` into a `requires` entry
   * naming the medium slot it already built, so the link is data. A hardcoded
   * volume name would be this screen knowing something the recipes are the
   * authority on, and it would be wrong the day a package for another release
   * declares a disc.
   */
  const mediumSlot =
    (packageSlot &&
      slotStates.find(
        (state) => state.slot.kind === "medium" && packageSlot.slot.requires.includes(state.slot.id)
      )) ||
    null;

  /**
   * The path a slot fills a field with.
   *
   * **A find, never a guess** — `matched_by: filename` never reaches `found`
   * in the resolver at all, and `chosen` is the user's own doing rather than
   * an identification ART made. Filling a field from either would be the
   * screen out-claiming the core.
   */
  function foundPath(state: SlotState | null): string | null {
    if (!state?.found) return null;
    switch (state.found.matchedBy) {
      case "hash":
      case "volume-name":
      case "top-level-directory":
        return state.found.path;
      default:
        return null;
    }
  }

  /**
   * What ART expected in a field: the names this artefact ships under and
   * where it came from.
   *
   * **This replaced the three hand-written hints.** They told a person what
   * the field was *for* — "the archive as you downloaded it" — which is the
   * one thing somebody reading a field labelled "the package's own archive"
   * already knows. What they cannot know is which of the forty files in their
   * downloads folder ART will accept, and the recipes state exactly that.
   *
   * Both halves have a fallback rather than an empty interpolation: "Expected
   * ." is a sentence nobody can act on, and this project's own rule is that a
   * refusal must name something.
   */
  /// **`null` answers too, and that is fix round 1's m6.** No slot means no
  /// answer arrived — no material folders, or `osinstall_slots` refused,
  /// which it does for a chosen tree carrying no `distribution.json`, so this
  /// is not only the exotic case. The hint used to vanish entirely then,
  /// taking the field's whole explanation with it. The two "ART has no note"
  /// phrases exist for exactly this, and saying them costs one line.
  function expectation(state: SlotState | null): { filenames: string; provenance: string } {
    return {
      filenames:
        state && state.slot.filenames.length > 0
          ? state.slot.filenames.join(", ")
          : t("osinstall.slots.filenamesUnknown"),
      provenance: state?.slot.provenance ?? t("osinstall.slots.provenanceUnknown"),
    };
  }

  const archiveFound = foundPath(packageSlot);
  // A slot ART measured as unnecessary fills nothing: the wrapper already
  // carries an `Updater` new enough, so there is no second archive to obtain
  // and none to pass to the run.
  const overlayFound = overlaySlot?.notNeeded ? null : foundPath(overlaySlot);
  const mediumFound = foundPath(mediumSlot);

  /** What the run actually uses: the user's own choice where they made one,
   *  ART's find otherwise. */
  const chosenArchive = archive ?? archiveFound;
  const chosenOverlay = overlayArchive ?? overlayFound;
  const chosenMedium = medium ?? mediumFound;

  // The archives, wrapper first. The order is the wire's own: everything
  // after the first is an overlay medium, matched by what it carries.
  const archives = useMemo(
    () =>
      [chosenArchive, chosenOverlay].filter(
        (path): path is string => path !== null && path !== ""
      ),
    [chosenArchive, chosenOverlay]
  );

  // The disc is **not** part of the "have you chosen enough to preview"
  // test. Whether this package needs one is the recipe's answer, not this
  // screen's, and Rust refuses by name when it is required and missing — a
  // sentence that says which disc and which volume. Requiring it here would
  // replace that with a silent grey panel for every package, including the
  // ones that need no disc at all.
  /**
   * The chosen package, **as the chosen release's own catalogue holds it**
   * (ART-212).
   *
   * The owner reached the OS Builder's fourth step with AmigaOS 3.2 chosen.
   * The list above correctly said ART carries no runnable package for 3.2 —
   * [ART-209] doing its job — and directly underneath, this panel still
   * showed `BoingBag39-1.lha`, `BoingBag39-2.lha`, `AmigaOS39.iso` and a
   * full "what will run" card reading
   * `ARTPkg:BoingBag3.9-1/C/Updater AmigaOS-Update DH0:`. A screen offering
   * to run an installer for an operating system that is not there.
   *
   * `packageId` is remembered, and nothing checked it against what the
   * release actually offers — the same gap `sanitizeChosen` closes for the
   * component checklist, which this panel never had.
   *
   * **A `null` catalogue allows, it does not block** — and the first version
   * of this fix had it the other way round, which the rest of this suite
   * caught immediately. `null` means *not loaded*, which happens whenever no
   * package folder has been chosen — and this panel is usable that way, with
   * an archive picked by hand. Blocking on "not loaded" turned "we have not
   * asked yet" into "the answer is no", which is a different sentence and a
   * broken screen. Only a catalogue that has actually answered, and does not
   * hold the id, refuses.
   */
  /** This release offers nothing that can be run on the Amiga — asked, not
   *  assumed: `null` means nobody has asked yet (ART-212). */
  const nothingRunnableHere = catalogue !== null && !catalogue.some((p) => p.amigaInstallable);

  /**
   * The one folder the **catalogue** is read from, and the folder the file
   * dialogs open on.
   *
   * `packages.folder` is the user's own archives folder when they have one,
   * and the material list's first *untagged* entry otherwise — so a build
   * whose folders are **all** layer-tagged (AmigaOS 3.2.2, every folder said
   * to hold a part of the release) leaves it `null`, and this panel then
   * printed *"choose the update packages folder above"* and offered no
   * package radio at all, while its own slots had just found everything in
   * those same folders. The catalogue was gated on one folder and the fields
   * were not (fix round 1, L9).
   *
   * Falling back to the list's first entry is safe here and nowhere else:
   * which packages a release offers is a function of the *recipes*, not of
   * the folder (`osinstall_packages` answers the same list whichever folder
   * it is given — only `available` depends on it, and this panel never reads
   * `available`, filtering on `amigaInstallable` alone). So the fallback
   * changes which folder the picker opens on and nothing else about what is
   * offered.
   */
  const catalogueFolder = packageFolder ?? materialFolders[0] ?? null;

  const packageBelongsHere = catalogueFolder
    ? // A folder is set, so an answer is coming: **wait for it.** Previewing
      // on the strength of a remembered id and retracting a moment later
      // would put a wrong sentence on screen — briefly, but this project's
      // own rule is that a confident wrong sentence is the expensive kind,
      // and "briefly" is exactly long enough to be read and believed.
      catalogue !== null && catalogue.some((p) => p.id === packageId)
    : // No folder, so no answer is coming and none is owed. The panel is
      // usable with an archive picked by hand, and refusing that would turn
      // "we never asked" into "the answer is no".
      true;

  const request: AmigaInstallRequest | null =
    treeRoot && packageId && packageBelongsHere && chosenArchive && kickstart
      ? { tree: treeRoot, packageId, packageArchives: archives, kickstart, medium: chosenMedium }
      : null;

  /**
   * A remembered id this release does not carry is dropped for good, once
   * the catalogue has actually answered (ART-212).
   *
   * Guarded on `catalogue !== null` for ART-089's reason, and on
   * `packageId !== null` so this cannot loop. `sanitizeChosen`'s own
   * comment on `OsInstall.tsx` says the same thing about the same hazard.
   */
  useEffect(() => {
    if (!catalogue || !packageId) return;
    if (!catalogue.some((p) => p.id === packageId)) setPackageId(null);
  }, [catalogue, packageId, setPackageId]);

  // The catalogue. Loaded whenever the package folder changes, `null` (never
  // `[]`) until something arrives, so "not loaded yet" and "loaded and empty"
  // stay different states — the distinction `OsInstall.tsx` already draws.
  useEffect(() => {
    if (!catalogueFolder) {
      setCatalogue(null);
      setCatalogueError(false);
      return;
    }
    let cancelled = false;
    osinstallPackages(catalogueFolder, release)
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
  }, [catalogueFolder, release]);

  // ART-277: ask what an archive *is* at the moment it is picked (or
  // restored from a remembered choice), rather than finding out only once
  // `compose` refuses it a moment later. Two effects, one per field, because
  // the two fields ask two different questions of the same file — the
  // package's own field expects `"the-package"`, the second field expects
  // `"the-update-archive"` — and `archiveFieldBlockerPhrase` reads which is
  // which. Scoped to `release` (ART-277 review, Major 2): the same list the
  // radio below actually offers.
  //
  // **M1 (round 1 whole-branch review).** `setArchiveClassification(null)`
  // is the *first* statement, before the new question is even asked —
  // clearing was previously reached only on the "field is empty" branch, so
  // changing the archive (a second Browse, or switching to a package with
  // its own remembered one) left the *previous* file's verdict rendered
  // against the *new* file's path for the whole round trip: a confident
  // sentence about a file that is not the one it names. Clearing first means
  // the box says nothing while the question is outstanding, which is the
  // honest state.
  //
  // Asked of the **effective** path — the user's override where there is one,
  // ART's own find otherwise. A file ART resolved is still a file this screen
  // is about to hand to a run, so it gets the same question; the alternative
  // would be classifying only what the user typed and staying silent about
  // what ART itself chose.
  useEffect(() => {
    setArchiveClassification(null);
    if (!chosenArchive || !packageId) {
      return;
    }
    let cancelled = false;
    amigainstallClassifyArchive(chosenArchive, packageId, release)
      .then((answer) => {
        if (!cancelled) setArchiveClassification(answer);
      })
      .catch(() => {
        if (!cancelled) setArchiveClassification(null);
      });
    return () => {
      cancelled = true;
    };
  }, [chosenArchive, packageId, release]);

  useEffect(() => {
    setOverlayClassification(null);
    if (!chosenOverlay || !packageId) {
      return;
    }
    let cancelled = false;
    amigainstallClassifyArchive(chosenOverlay, packageId, release)
      .then((answer) => {
        if (!cancelled) setOverlayClassification(answer);
      })
      .catch(() => {
        if (!cancelled) setOverlayClassification(null);
      });
    return () => {
      cancelled = true;
    };
  }, [chosenOverlay, packageId, release]);

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
        setRefusal(errorText(t, e));
        setPreviewing(false);
      });
    return () => {
      cancelled = true;
    };
    // `request` is rebuilt on every render; its parts are the identity.
    //
    // `packageBelongsHere` is one of them (ART-212) and has to be: it is what
    // flips the request from `null` to real once the catalogue answers, and a
    // dependency list without it left the panel previewing nothing for ever —
    // the request became valid and no effect ever noticed. A boolean, so it
    // is a stable dependency and not a fresh identity per render.
    //
    // `chosenMedium` is listed where the raw `medium` never was, and that was
    // a real gap: the disc is part of `request` and the preview names the
    // volume the image itself states, so choosing one and not re-previewing
    // left that line describing the previous disc. It matters more now that a
    // disc can arrive from the slots rather than only from a click.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [
    treeRoot,
    packageId,
    chosenArchive,
    chosenOverlay,
    chosenMedium,
    kickstart,
    winuaePath,
    packageBelongsHere,
  ]);

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
      defaultPath: catalogueFolder ?? undefined,
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
      setRefusal(errorText(t, e));
      setBusy(false);
      job.current = null;
    }
  }

  const runnable = (catalogue ?? []).filter((p) => p.amigaInstallable);
  const nameOf = (id: string) => catalogue?.find((p) => p.id === id)?.name ?? id;
  // ART-277. Asked the moment the file was chosen — never only after the
  // round trip through Rust's `compose` refusal — and rendered as an entry
  // in `blockers` below, directly above the confirm checkbox and the Run
  // button, rather than in a second box beside the field: ART-202's own
  // lesson, from this exact screen, is that a reason Run is dead has to say
  // so where the button is (review Medium 2).
  const selectedPackageName = packageId ? nameOf(packageId) : "";
  // ART-277 re-review: the two fields' own labels, named rather than
  // "above"/"below" — a "wrong field" sentence rendered a screen away from
  // either field must say which one by name, and interpolating the exact
  // label the `Field` below renders is what keeps the two from drifting
  // apart if either wording ever changes.
  const fieldLabels = {
    package: t("osinstall.amigaInstall.archive.label"),
    overlay: t("osinstall.amigaInstall.overlayArchive.label"),
  };
  const archiveBlocker = archiveFieldBlockerPhrase(
    archiveClassification,
    "package",
    chosenArchive ?? "",
    selectedPackageName,
    nameOf,
    fieldLabels
  );
  const overlayBlocker = archiveFieldBlockerPhrase(
    overlayClassification,
    "overlay",
    chosenOverlay ?? "",
    selectedPackageName,
    nameOf,
    fieldLabels
  );
  /**
   * **The package's own archive is not there, said where the button is.**
   *
   * Run is already dead without one — `request` is `null`, so no preview
   * exists — but "dead with no reason rendered near it" is ART-202's own
   * defect on this exact screen. The sentence names the files ART expected,
   * which is a refusal the user can act on: it is the difference between "you
   * have not chosen an archive" (which they cannot fix without knowing which)
   * and "BoingBag39-1.lha is not in the folders you named".
   *
   * Only when a slot actually answered. Nothing resolved means nothing was
   * asked, and a blocker then would be ART claiming a folder is missing a
   * file it never looked for.
   */
  const missingArchiveBlocker: Phrase | null =
    packageSlot && !chosenArchive
      ? packageSlot.slot.filenames.length > 0
        ? {
            key: "osinstall.amigaInstall.blocker.slotMissing",
            params: {
              name: packageSlot.slot.name,
              filenames: packageSlot.slot.filenames.join(", "),
            },
          }
        : // ART knows of no name this artefact ships under, so it says only
          // what it does know. "Expected ." is the sentence a user cannot act
          // on — `slotLines` makes the same choice for the same reason.
          { key: "osinstall.slots.notFoundUnnamed", params: { name: packageSlot.slot.name } }
      : null;
  // One mechanism disables Run and the confirm checkbox: `blockers.length >
  // 0` (review Medium 2 — a second, separate `wrongPackageArchive` boolean
  // used to disable Run alone, so the checkbox could still be ticked over a
  // request that could never succeed and Run would die with no reason
  // rendered anywhere near it).
  //
  // **M2 (round 1 whole-branch review).** Keyed and deduplicated —
  // `readinessBlockers` was the only producer when `blocker.key` was used
  // as the React key directly, so every key was unique by construction.
  // Both archive fields can now hold the same wrong archive (the identical
  // `Phrase`, same key and params, from two different fields), which used
  // to render as a duplicate React key and the same sentence twice —
  // ART-202's own "aynı uyarı tek ekranda 2 tane" mistake, reached through
  // a producer this screen did not have when that rule was written.
  //
  // **m5.** The archives ART resolved itself are named, so a file that has
  // gone since the scan is not reported as *"the archive you chose"* — the
  // user chose nothing, and the read-only line directly above still says the
  // file was identified by its bytes.
  const artsOwnArchives = [
    archive === null ? archiveFound : null,
    overlayArchive === null ? overlayFound : null,
  ].filter((path): path is string => path !== null);
  const blockers = dedupeBlockers([
    ...(preview
      ? readinessBlockers(preview, artsOwnArchives).map((phrase) => ({
          field: "preview",
          phrase,
        }))
      : []),
    ...(missingArchiveBlocker ? [{ field: "slot", phrase: missingArchiveBlocker }] : []),
    ...(archiveBlocker ? [{ field: "package", phrase: archiveBlocker }] : []),
    ...(overlayBlocker ? [{ field: "overlay", phrase: overlayBlocker }] : []),
  ]);
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
      {!catalogueFolder && !catalogueError && (
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

      {/*
        ART-212. When this release carries nothing that can be run on the
        Amiga at all, the sentence above says so and **the form stops there**.
        The owner reached this step with AmigaOS 3.2 chosen and read exactly
        that sentence — and then, underneath it, four filled-in fields naming
        BoingBag39-1.lha, BoingBag39-2.lha and AmigaOS39.iso. Inputs for a run
        that cannot be configured are not neutral: they are the screen
        offering something it has just finished saying it does not have.

        `catalogue === null` (no folder chosen, so nothing asked) still shows
        them — the panel is usable with an archive picked by hand.
      */}
      {!nothingRunnableHere && (
      <div style={{ marginTop: 12 }}>
        {/* Design § 3.4: the three browse buttons become three slots. Each
            field shows what ART resolved and only asks a question when it
            could not — and what it asks for is the slot's own expectation,
            not a sentence somebody wrote about what the field is for. */}
        <SlotField
          testId="amiga-slot-archive"
          label={t("osinstall.amigaInstall.archive.label")}
          empty={t("osinstall.amigaInstall.archive.none")}
          hint={t("osinstall.amigaInstall.archive.hint", expectation(packageSlot))}
          state={packageSlot}
          override={archive}
          found={archiveFound}
          onChoose={() =>
            void chooseArchive(setArchive, t("osinstall.amigaInstall.archive.chooseTitle"))
          }
          onOverride={setArchive}
          onUseFound={forgetArchive}
          onClear={forgetArchive}
        />
        <SlotField
          testId="amiga-slot-overlay"
          label={t("osinstall.amigaInstall.overlayArchive.label")}
          empty={t("osinstall.amigaInstall.overlayArchive.none")}
          hint={t("osinstall.amigaInstall.overlayArchive.hint", expectation(overlaySlot))}
          state={overlaySlot}
          override={overlayArchive}
          found={overlayFound}
          onChoose={() =>
            void chooseArchive(
              setOverlayArchive,
              t("osinstall.amigaInstall.overlayArchive.chooseTitle")
            )
          }
          onOverride={setOverlayArchive}
          onUseFound={forgetOverlayArchive}
          onClear={forgetOverlayArchive}
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
        <SlotField
          testId="amiga-slot-medium"
          label={t("osinstall.amigaInstall.medium.label")}
          empty={t("osinstall.amigaInstall.medium.none")}
          hint={t("osinstall.amigaInstall.medium.hint", expectation(mediumSlot))}
          state={mediumSlot}
          override={medium}
          found={mediumFound}
          onChoose={() => void chooseMedium()}
          onOverride={setMedium}
          onUseFound={forgetMedium}
          onClear={forgetMedium}
        />
      </div>
      )}

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
              <li key={blocker.id} style={{ padding: "2px 0", wordBreak: "break-all" }}>
                {t(blocker.phrase.key, blocker.phrase.params)}
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
