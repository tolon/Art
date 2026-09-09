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
//     a wrong archive names which package's it really is. A translated line
//     beside them says the thing ART can
//     say in the user's own language and that the English does not: nothing
//     was copied.
//   - **A run that did not succeed says where the copy is.** A user told "it
//     failed", and not told where the evidence went, has been given nothing.
//   - **The emulator is a window on this desktop, and the screen says so
//     before it opens** — an earlier round opened one repeatedly without
//     warning and that was a real annoyance.
//
// ## The chain is the list (round 3, task 2)
//
// This screen's spine is now the AmigaOS 3.9 update **chain** — one row per
// link, in the material's own order, from `core::osinstall::chain` through
// `osinstallChain` and `chainLines`. The package radio list it had before is
// what the chain replaced: a list of two runnable packages could not say
// what the material actually says, which is that the CD comes first, then
// BoingBag 1, then BoingBag 2, then four packages in no order between them,
// then BoingBags 3&4.
//
// Three rules the chain brings with it:
//
//   - **A row's state is the manifest's and the slots' word, never this
//     screen's.** Seven states, seven sentences, seven next steps, all of
//     them `@/lib/chain`'s (which is `core::osinstall::chain`'s). Nothing
//     here composes a sentence about a row.
//   - **One Run button, and it runs one row.** The row the user selected
//     when they selected one, the first *ready* row otherwise — and it says
//     which. A row that is installed, blocked, missing, not needed, refused
//     or not yet runnable can still be selected to read its facts, and Run
//     is then disabled with that row's own sentence beside it. Running a
//     different row from the one on screen is the confident-wrong action
//     this whole file exists to avoid.
//   - **Two routes, chosen by the row, never by the screen.** A row whose
//     package declares an Amiga-side installer runs through `compose` and an
//     emulator, exactly as before. A row ART places from Windows runs
//     through the `paketler` step's own path — `osinstall_collisions` then
//     `osinstall_add_package`, reused through `useHostPlacement` rather than
//     copied. `SentenceFacts::runs_on_amiga` is what decides, and it is
//     three-valued: the CD row and a package ART can neither place nor run
//     are neither route.
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
import { Link } from "react-router-dom";

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
  readinessBlockers,
  settlementPhrase,
  type AmigaInstallPreview,
  type AmigaInstallRequest,
  type AmigaInstallResult,
  type ArchiveClassification,
  slotOverrides,
} from "@/lib/amigainstall";
import {
  collisionCounts,
  fileName,
  notYetRunnablePanelKey,
  osinstallChain,
  osinstallPackages,
  osinstallSlots,
  type ApplyOutcome,
  type ChainReport,
  type ChainRow,
  type InstallRelease,
  type PackageSummary,
  type SlotReport,
  type SlotState,
} from "@/lib/osinstall";
import { chainLines, chainSummaryLine, type ChainLine } from "@/lib/chain";
import {
  candidateLines,
  crowdedFolderLines,
  displayName,
  slotLines,
  unreadableFolderLines,
} from "@/lib/slots";
import {
  HostPlacementPreview,
  HostPlacementReport,
  useHostPlacement,
} from "@/components/osbuilder/HostPlacement";
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
   *  starting folder. The run itself takes a whole file path, never a folder:
   *  the archive is chosen deliberately, not guessed at. */
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
   *  Two producers, never folded into one: the user's own choice, and the
   *  readout's own row for the file ART identified. */
  const filled: Phrase | null = override
    ? { key: "osinstall.slots.chosen", params: { file: fileName(override), name } }
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

/**
 * The folder a file sits in, by name alone.
 *
 * Used for one thing: handing `osinstall_add_package` the folder a
 * host-placed row's archive was actually **found** in, rather than assuming
 * it is the archives folder. This build's material is a list of folders and
 * the file may be in any of them; the slot already knows which.
 *
 * No path building happens here — the answer is a prefix of a path Rust
 * itself produced, and Rust validates it again on the way back in.
 */
function folderOf(path: string): string | null {
  const cut = Math.max(path.lastIndexOf("\\"), path.lastIndexOf("/"));
  return cut > 0 ? path.slice(0, cut) : null;
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
  /**
   * The user's own per-slot file choices, as both `osinstall_slots` and
   * `osinstall_chain` take them (ART-284).
   *
   * **A string, deposited into a `useMemo`, because both effects below start
   * disk work** — the same rule `OsInstall.tsx` and `MaterialReadout.tsx`
   * already keep for this exact value: an array rebuilt each render is a
   * fresh identity and would re-scan the folders on every keystroke.
   */
  const rememberedBag = useSettingsStore((s) => s.settings.remembered);
  const overridesKey = JSON.stringify(slotOverrides(rememberedBag));

  const [packageId, setPackageId] = useRemembered<string | null>(
    "amigaInstall.package",
    isTextOrNothing,
    null
  );
  /**
   * The CD row selected, which is the one selection `packageId` cannot hold:
   * the first link of the chain is a **medium**, not a package, and writing
   * its slot id into a remembered key whose guard and whose sanitiser both
   * expect a package id would be dropped on the next render.
   *
   * Deliberately not remembered. Which row somebody is reading is not a
   * setting; which package they mean to run is.
   */
  const [mediumSelected, setMediumSelected] = useState(false);
  /**
   * Whether the row selection was made **in this session**, by a click.
   *
   * Deliberately not remembered, and it is the whole of ART-286's fix: a
   * click is somebody saying *this row*, and a restored `packageId` is only
   * where they were last looking. The two need different answers from the Run
   * button — see `target`.
   */
  const [pickedByHand, setPickedByHand] = useState(false);
  /**
   * The package's own archive, **scoped per package** (ART-277). There were
   * two of these until 2026-09-09; the update-archive field went with the
   * overlay machinery. `useRemembered` takes its key as a plain argument
   * re-read on every render, so a key that changes with `packageId` is
   * exactly what it already supports — no lower-level `@/lib/remembered`
   * call is needed. Switching the radio to
   * BoingBag 3.9-2 therefore reads BoingBag 3.9-2's own remembered archive
   * (empty the first time), while BoingBag 3.9-1's stays exactly where it
   * was under its own key. See `amigaInstallArchiveKey`'s own comment for
   * the defect this closes.
   */
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

  // A primitive dependency rather than the array: two equal strings are the
  // same value to React's own comparison, two equal arrays are not, and the
  // two effects it feeds both start disk work (ART-178/ART-195).
  const materialKey = materialFolders.join("\n");

  /**
   * **The chain** (round 3 § 2) — one row per link of the material's own
   * order, resolved against the same folders, tree and ROM the slots are.
   *
   * `null` means the question has not been answered — never that there is no
   * chain. A release ART knows no chain for answers with *no rows*, which is
   * a different fact and has a different consequence below: no rows means
   * the package list this panel had before the chain existed is what it
   * shows, so a screen is never left with nothing to select from.
   */
  const [chain, setChain] = useState<ChainReport | null>(null);
  /** Bumped after a run so the chain is asked again — a row that has just
   *  been applied has to *become* installed, and the next ready row has to
   *  be offered, without the user reloading the step. */
  const [chainAsked, setChainAsked] = useState(0);
  useEffect(() => {
    const list = materialKey ? materialKey.split("\n") : [];
    let cancelled = false;
    osinstallChain(release, list, treeRoot, kickstart, JSON.parse(overridesKey))
      .then((answer) => {
        if (!cancelled) setChain(answer);
      })
      .catch(() => {
        // Silent, and for the slots' own reason: a red box here would report
        // a fault in ART as though it were a statement about the user's
        // files. The panel falls back to the package list it had before.
        if (!cancelled) setChain(null);
      });
    return () => {
      cancelled = true;
    };
  }, [release, materialKey, treeRoot, kickstart, chainAsked, overridesKey]);

  /**
   * The rows and their sentences, paired **by index**: `chainLines` maps one
   * line per row, in order, so index is exact — and a row's `position` is a
   * rank the material may give twice (`locale-39` and `locale-39-turkish`
   * are both 4), which makes it useless as a key.
   */
  const entries: { row: ChainRow; line: ChainLine }[] = useMemo(() => {
    if (!chain) return [];
    const lines = chainLines(chain.rows);
    return chain.rows.map((row, index) => ({ row, line: lines[index] }));
  }, [chain]);
  const hasChain = entries.length > 0;

  /**
   * The row the user picked, or `null` when they have picked none.
   *
   * **`packageId === null` is "nothing picked", not "the CD row".** The CD
   * row is the one row whose package id is `null`, so matching on equality
   * alone would make a fresh panel — nothing ever selected — read as one
   * with the disc selected, and the Run button would then have a row and
   * offer none.
   */
  const selected = mediumSelected
    ? (entries.find((entry) => entry.row.packageId === null) ?? null)
    : packageId
      ? (entries.find((entry) => entry.row.packageId === packageId) ?? null)
      : null;
  /** The first row that is ready — where the one Run button goes when the
   *  user has selected nothing. `chainLines` decides which row that is, so
   *  there is one answer to "which row is next". */
  const firstReady = entries.find((entry) => entry.line.runnable) ?? null;
  /**
   * The row the Run button is about.
   *
   * **A row the user clicked is the target whatever state it is in**, and
   * that is round 3's own rule: selecting is reading, Run is then disabled,
   * and the row's own sentence beside it says why. `pickedByHand` is what
   * makes that a rule about a *click* rather than about a stored value.
   *
   * **A remembered selection is a default, not a decision** (ART-286,
   * 2026-09-09). `packageId` survives between sessions, so the panel opens on
   * whichever row somebody last looked at — and if that row is already in the
   * tree, targeting it made the one Run button read *"Add the chosen
   * packages"* over *"…already in this tree"*, a caption naming an action on a
   * package that needs none. A restored selection that cannot run therefore
   * falls back to the first ready row, exactly as no selection at all does,
   * and the screen says both things: `Next: X` for what Run will do, and the
   * remembered row's own sentence for why it is not that.
   *
   * `kind === "ready"` and not `line.runnable`: `runnable` marks the *first*
   * ready row only, so a second ready row at the same rank (`locale-39` and
   * `locale-39-turkish`) is a perfectly good target the user picked.
   */
  const selectionCanRun = selected?.line.kind === "ready";
  const target = selected && (pickedByHand || selectionCanRun) ? selected : firstReady;
  /**
   * The rows still owed — neither already in the tree nor made redundant by
   * something that is.
   *
   * **Not `installed === total`** (fix round 1, F1). A chain whose only
   * outstanding row is `not needed` is finished, and counting it as owed
   * would answer "nothing here can run yet" about a tree that has
   * everything. `summarize_chain` counts the two apart for the same reason.
   */
  const outstanding = entries.filter(
    (entry) => entry.line.kind !== "installed" && entry.line.kind !== "not-needed"
  );
  /** Whether this build has been given anywhere to look at all. */
  const noFolders = materialFolders.length === 0;
  /**
   * The sentence one row renders.
   *
   * `chainLines`' own in every case but one: **a `missing` row when the user
   * has named no folders at all** (fix round 1, F6). *"…is not in the folders
   * you named"* is strictly true of an empty set and useless — it is a fact
   * about a set the user has not populated, and somebody arriving here before
   * the source step reads it eight times over. Whether the list is empty is
   * this screen's own prop and something `core::osinstall::chain` cannot know,
   * so the substitution is here; the sentence itself is still a catalogue
   * entry and still names the row.
   *
   * One function, used by the rows, by the reason beside a dead Run button
   * and by F1's "nothing can run yet" line, so the three cannot disagree.
   */
  function sentenceFor(line: ChainLine): Phrase {
    // **The CD row's own sentence** (round 3 whole-branch review, M3).
    // `chain::medium_state` answers `Missing` for a tree whose `built_from`
    // names no such volume — including when the ISO is sitting in a folder
    // the user named, because this screen cannot build a tree from a disc
    // and finding the file changes nothing about that. *"…is not in the
    // folders you named"* would then be false with the file right there, so
    // the row says what is actually true and the `kaynak` link beside it is
    // the action.
    if (line.kind === "missing" && line.isMedium) {
      return { key: "osinstall.chain.mediumNotBuiltFrom", params: { name: line.name } };
    }
    // The component's own name, translated here because `@/lib/chain` does
    // not render (M4). A component with no `labelKey` shows its id — which
    // is what the components step shows for it too.
    if (line.kind === "blocked-component") {
      return {
        ...line.phrase,
        params: {
          ...line.phrase.params,
          components: line.components
            .map((component) => (component.labelKey ? t(component.labelKey) : component.id))
            .join(", "),
        },
      };
    }
    return noFolders && line.kind === "missing"
      ? { key: "osinstall.chain.missingNoFolders", params: { name: line.name } }
      : line.phrase;
  }
  /**
   * The package every field, every classification and the whole Amiga-side
   * request is about.
   *
   * With a chain it is the **target row's** package, so pressing Run with
   * nothing selected runs the row the button names rather than nothing at
   * all. With no chain it is the remembered pick, exactly as before.
   */
  const activePackageId = hasChain ? (target?.row.packageId ?? null) : packageId;
  /** Where the target row happens. `null` on both sides for a row that is
   *  neither — the CD, and a package ART can neither place nor run. */
  const targetRunsOnAmiga = hasChain ? (target?.row.sentenceFacts.runsOnAmiga ?? null) : true;
  const targetReady = hasChain ? target?.line.kind === "ready" : true;

  //
  // **Round 2 § 3.4: these are now the *override*, not the answer.** The
  // fields are filled from the slots; a path here is one the user picked by
  // hand, it wins over whatever ART resolved, and it survives a re-scan
  // because nothing but the user's own click ever writes or drops it. That
  // is why the third element — `forget` — is taken: setting `null` would
  // store a decision ("no file"), and the found file could then never fill
  // the field again.
  const [archive, setArchive, forgetArchive] = useRemembered<string | null>(
    amigaInstallArchiveKey("amigaInstall.archive", activePackageId),
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
  const [preview, setPreview] = useState<AmigaInstallPreview | null>(null);
  /** A refusal, exactly as Rust wrote it (ART-060). Never folded into one
   *  translated sentence: which reason applies is the whole content. */
  const [refusal, setRefusal] = useState<string | null>(null);
  const [previewing, setPreviewing] = useState(false);
  const [confirmed, setConfirmed] = useState(false);

  const job = useRef<number | null>(null);
  /**
   * Set when a run starts and read once the chain has re-answered: the
   * selection is only moved on by a run **this screen made**. A ref rather
   * than state — nothing renders from it, and a re-render on setting it
   * would re-run the effect that reads it.
   *
   * **Armed at the start of a run and disarmed by every ending that is not
   * a success** (fix round 2, N1): the four Amiga-side endings, a job that
   * failed or was cancelled mid-flight, a refusal raised before any job
   * started, and the host route's failure and refusal. Only a run that
   * succeeded can make its own row `installed`, so only a success may leave
   * this armed. Left armed after anything else it survives until the user
   * next selects an installed row **to read it**, and then clears that
   * selection — nothing changing because the user changed it.
   */
  const pendingAdvance = useRef(false);
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
   * **Which row the report on screen is about**, recorded when the run
   * starts rather than read back off the request afterwards.
   *
   * `AmigaInstallResult` carries the outcome and the settlement and no name,
   * and with a chain the request moves on the moment a run succeeds. A
   * report that outlives its request has to say what it was about or it is
   * the screen claiming something about the row now selected.
   */
  const [ranName, setRanName] = useState<string | null>(null);
  /**
   * A host placement's own two endings, **kept by this panel** for the same
   * reason `result` is: a successful placement makes the chain re-ask, the
   * row becomes installed and the button moves to the next one, which clears
   * `useHostPlacement`'s own copy. The screen would then say nothing at all
   * about a placement it had just made.
   */
  const [placedOutcome, setPlacedOutcome] = useState<ApplyOutcome | null>(null);
  const [placedError, setPlacedError] = useState<string | null>(null);

  /** Everything the last run said, dropped. Called when the next run starts
   *  and when the user picks another row — the two acts that make the report
   *  stop being about what is on screen. */
  function clearReport() {
    setResult(null);
    setRanName(null);
    setJobError(null);
    setWasCancelled(false);
    setLastReported(null);
    setPlacedOutcome(null);
    setPlacedError(null);
  }

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
  useEffect(() => {
    const list = materialKey ? materialKey.split("\n") : [];
    let cancelled = false;
    osinstallSlots(release, list, treeRoot, kickstart, JSON.parse(overridesKey))
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
    // `chainAsked` too: a package placed from Windows changes what the
    // manifest records, and the fields above are resolved against it.
  }, [release, materialKey, treeRoot, kickstart, chainAsked, overridesKey]);

  const slotStates = slotReport?.states ?? [];
  /** The chosen package's own slot. */
  const packageSlot =
    (activePackageId &&
      slotStates.find((state) => state.slot.id === `package:${activePackageId}`)) ||
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

  /** What the run actually uses: the user's own choice where they made one,
   *  ART's find otherwise. */
  const chosenArchive = archive ?? archiveFound;

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

  /**
   * The folder a **host-placed** row's archive is actually in.
   *
   * `osinstall_collisions` and `osinstall_add_package` each take one folder,
   * and this build's material is a *list* of them — so the folder is taken
   * from the row's own slot, which is the thing that already knows which
   * file this row is and where it was found. The archives folder is the
   * fallback for a row nothing has resolved, and it is a fallback rather
   * than the answer: sending `add_package` to a folder the file is not in
   * would be refused for a file the user has.
   */
  const targetSlot =
    (target?.row.slotId &&
      slotStates.find((state) => state.slot.id === target.row.slotId)) ||
    null;
  const targetFound = foundPath(targetSlot);
  const hostFolder = (targetFound && folderOf(targetFound)) || catalogueFolder;

  /**
   * The `paketler` step's own path, reused rather than rewritten (the
   * brief's rule): preview what the archive would replace, confirm the set,
   * place it, report what it did.
   *
   * `enabled` is this screen's guard: only a **ready, host-placed** row is
   * previewed. Anything else is a row whose own sentence already says why it
   * cannot run, and a second answer beside it would be the screen
   * contradicting the core.
   */
  const placement = useHostPlacement({
    treeRoot,
    packageFolder: hostFolder,
    chosen: activePackageId && targetRunsOnAmiga === false ? [activePackageId] : [],
    enabled: targetRunsOnAmiga === false && targetReady,
  });

  // **After a run, the chain re-asks.** A row that has just been applied has
  // to become *installed* and the next ready row has to be offered — and
  // both facts live in `distribution.json`, which only Rust can read. One
  // effect per route; each fires once, on the object the route answers with.
  useEffect(() => {
    if (result) setChainAsked((asked) => asked + 1);
  }, [result]);
  useEffect(() => {
    if (!placement.outcome) return;
    setPlacedOutcome(placement.outcome);
    setChainAsked((asked) => asked + 1);
  }, [placement.outcome]);
  useEffect(() => {
    if (!placement.applyError) return;
    setPlacedError(placement.applyError);
    // N1, the host route's half: a failed or cancelled placement wrote
    // nothing, so nothing may advance.
    pendingAdvance.current = false;
  }, [placement.applyError]);
  useEffect(() => {
    // …and a refusal, which is resolved before a byte moves and so never
    // reaches a job at all (Task 7's F2).
    if (placement.refusals && placement.refusals.length > 0) {
      pendingAdvance.current = false;
    }
  }, [placement.refusals]);

  /**
   * **…and the selection moves on with it** (fix round 1, F4).
   *
   * `selectRow` writes `packageId`, so after a run of a row the user had
   * *explicitly* selected the selection stayed on that row — which is now
   * `installed` — and the button read "already in this tree" with no next
   * row named. The brief's *"the next ready row is offered"* held only on
   * the path where nothing was selected.
   *
   * Cleared rather than moved: dropping the selection lets `target` fall
   * back to `firstReady`, which is the one answer to "which row is next"
   * and already names itself beside the button. Setting a new `packageId`
   * here would be ART making a choice the user did not.
   *
   * **Only after a run this screen made**, which is what `pendingAdvance`
   * is for — a selection the user made must survive every other refresh of
   * the chain, and this effect runs on all of them.
   */
  useEffect(() => {
    if (!pendingAdvance.current) return;
    if (!selected || selected.line.kind !== "installed") return;
    pendingAdvance.current = false;
    // Not `clearReport()`: the report of the run that just finished is
    // exactly what has to survive this, and it names its own row.
    setMediumSelected(false);
    setPackageId(null);
  }, [selected, setPackageId]);

  const packageBelongsHere = catalogueFolder
    ? // A folder is set, so an answer is coming: **wait for it.** Previewing
      // on the strength of a remembered id and retracting a moment later
      // would put a wrong sentence on screen — briefly, but this project's
      // own rule is that a confident wrong sentence is the expensive kind,
      // and "briefly" is exactly long enough to be read and believed.
      catalogue !== null && catalogue.some((p) => p.id === activePackageId)
    : // No folder, so no answer is coming and none is owed. The panel is
      // usable with an archive picked by hand, and refusing that would turn
      // "we never asked" into "the answer is no".
      true;

  const request: AmigaInstallRequest | null =
    treeRoot &&
    activePackageId &&
    packageBelongsHere &&
    chosenArchive &&
    kickstart &&
    // With a chain, only a **ready Amiga-side row** is composed: previewing
    // a row the chain has already refused, or one ART places from Windows,
    // would put a second, contradicting answer on the screen beside the
    // row's own sentence.
    targetRunsOnAmiga === true &&
    targetReady
      ? {
          tree: treeRoot,
          packageId: activePackageId,
          packageArchive: chosenArchive,
          kickstart,
        }
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
  // `compose` refuses it a moment later. One effect, because there is one
  // archive field — the second one went with the overlay machinery on
  // 2026-09-09, and with it the `"the-update-archive"` kind that told the user
  // which of two fields a file belonged in. Scoped to `release` (ART-277
  // review, Major 2): the same list the radio below actually offers.
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
    if (!chosenArchive || !activePackageId) {
      return;
    }
    let cancelled = false;
    amigainstallClassifyArchive(chosenArchive, activePackageId, release)
      .then((answer) => {
        if (!cancelled) setArchiveClassification(answer);
      })
      .catch(() => {
        if (!cancelled) setArchiveClassification(null);
      });
    return () => {
      cancelled = true;
    };
  }, [chosenArchive, activePackageId, release]);

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
    // **The report is not cleared here, and that is round 3's own change.**
    // A report used to describe "the run that was on screen when it
    // finished", so any change to the request wiped it. With a chain, the
    // request changes *by itself* the moment a run succeeds: the row becomes
    // installed and the button moves to the next ready row, which would have
    // erased the report of the run that had just finished — the screen
    // saying nothing about work it had actually done.
    //
    // So the report survives, and it **names the row it is about**
    // (`ranName`), which is what makes surviving honest: it can never be
    // read as a statement about the row now on screen. It is cleared when
    // the user picks another row and when the next run starts.
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
    // eslint-disable-next-line react-hooks/exhaustive-deps
    //
    // `targetRunsOnAmiga` and `targetReady` are the chain's own two gates on
    // the request, and a dependency list without them would leave the panel
    // previewing the row the user has just moved off.
  }, [
    treeRoot,
    activePackageId,
    chosenArchive,
    kickstart,
    winuaePath,
    packageBelongsHere,
    targetRunsOnAmiga,
    targetReady,
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
          // N1: a run that died mid-flight emits no result of its own, so
          // this is the only place its ending is seen. Nothing was
          // promoted; nothing may advance.
          pendingAdvance.current = false;
        } else if (update.state.state === "cancelled") {
          setWasCancelled(true);
          pendingAdvance.current = false;
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
        // **Three of the four endings disarm the advance** (fix round 2,
        // N1). Only a run that succeeded can make its row `installed`, and
        // only then may `pendingAdvance` be left armed for the effect that
        // reads it. Left armed after a refusal, a timeout or a closed
        // window, it fires on whatever installed row the user next selects
        // **to read** and drops that selection — a setting changing without
        // the user changing it, from the code written to respect the rule.
        if (answer.outcome.kind !== "succeeded") pendingAdvance.current = false;
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

  async function chooseKickstart() {
    const picked = await open({
      multiple: false,
      title: t("osinstall.amigaInstall.kickstart.chooseTitle"),
      filters: [{ name: "Kickstart ROM", extensions: ["rom", "bin"] }],
    });
    if (typeof picked === "string") setKickstart(picked);
  }

  /**
   * Pick a row.
   *
   * Selecting is reading: any row may be selected, including one that cannot
   * be run — its own sentence then says why, and Run is disabled. What
   * changes with the selection is which package every field below is about,
   * so the last run's report is dropped: it was about a different row.
   */
  function selectRow(row: ChainRow) {
    clearReport();
    setPickedByHand(true);
    setMediumSelected(row.packageId === null);
    if (row.packageId !== null) setPackageId(row.packageId);
  }

  async function runInstall() {
    if (!request) return;
    setBusy(true);
    clearReport();
    setRanName(target?.line.name ?? null);
    // F4: this run, and only this run, may move an explicit selection on.
    pendingAdvance.current = true;
    setProgress(null);
    try {
      job.current = await amigaInstallRun(request, winuaePath);
    } catch (e) {
      // Every refusal is raised before the job starts, so this is the same
      // English sentence the preview would have shown — never a job that
      // could only have gone red a moment later.
      setRefusal(errorText(t, e));
      setBusy(false);
      job.current = null;
      // N1: no job ran, so no row can have become installed.
      pendingAdvance.current = false;
    }
  }

  /** The other route: this row's files are placed from Windows, through the
   *  `paketler` step's own path. Its three endings are that path's. */
  async function runPlacement() {
    clearReport();
    setRanName(target?.line.name ?? null);
    pendingAdvance.current = true;
    await placement.run();
  }

  const runnable = (catalogue ?? []).filter((p) => p.amigaInstallable);
  const nameOf = (id: string) => catalogue?.find((p) => p.id === id)?.name ?? id;
  // ART-277. Asked the moment the file was chosen — never only after the
  // round trip through Rust's `compose` refusal — and rendered as an entry
  // in `blockers` below, directly above the confirm checkbox and the Run
  // button, rather than in a second box beside the field: ART-202's own
  // lesson, from this exact screen, is that a reason Run is dead has to say
  // so where the button is (review Medium 2).
  const selectedPackageName = activePackageId ? nameOf(activePackageId) : "";
  const archiveBlocker = archiveFieldBlockerPhrase(
    archiveClassification,
    chosenArchive ?? "",
    selectedPackageName,
    nameOf
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
  // as the React key directly, so every key was unique by construction. It
  // stopped being the only one, and the two producers can still agree: the
  // slot's own missing-archive sentence and the preview's are the same
  // `Phrase` for the same file. Rendering it twice is ART-202's own
  // "aynı uyarı tek ekranda 2 tane" mistake. (The case that bought this was
  // two archive *fields* holding one wrong file; the second field went on
  // 2026-09-09 and the rule outlived it.)
  //
  // **m5.** The archives ART resolved itself are named, so a file that has
  // gone since the scan is not reported as *"the archive you chose"* — the
  // user chose nothing, and the read-only line directly above still says the
  // file was identified by its bytes.
  const artsOwnArchives = [archive === null ? archiveFound : null].filter(
    (path): path is string => path !== null
  );
  const blockers = dedupeBlockers([
    ...(preview
      ? readinessBlockers(preview, artsOwnArchives).map((phrase) => ({
          field: "preview",
          phrase,
        }))
      : []),
    ...(missingArchiveBlocker ? [{ field: "slot", phrase: missingArchiveBlocker }] : []),
    ...(archiveBlocker ? [{ field: "package", phrase: archiveBlocker }] : []),
  ]);
  const outcome = result ? outcomePhrase(result.outcome) : null;
  const nextStep = result ? outcomeNextStepPhrase(result.outcome) : null;
  const settlement = result ? settlementPhrase(result.settlement) : null;
  const tone = result ? outcomeTone(result.outcome) : null;

  const chainSummary = chain ? chainSummaryLine(chain) : null;
  /**
   * The folders ART could not read, and the folders it stopped counting in
   * — the two fields the chain report has carried since task 1 and nothing
   * on this screen rendered (fix round 1, F5).
   *
   * `MaterialReadout`'s own phrases, from `@/lib/slots`: the same fact said
   * the same way on both screens, and a user who lives on this step no
   * longer has to visit the other one to learn that a folder was skipped.
   */
  const unreadableFolders = unreadableFolderLines(chain?.unreadableFolders ?? []);
  const crowdedFolders = crowdedFolderLines(chain?.crowdedFolders ?? []);
  /**
   * Whether the Amiga-side form belongs on screen at all.
   *
   * Two gates, and they are different questions. ART-212's: this release
   * carries nothing that can be run on the Amiga, so inputs for such a run
   * are the screen offering what it has just said it does not have. The
   * chain's: the row the button is about is placed from Windows (or is the
   * CD), so an archive field, a Kickstart and a disc are inputs its run does
   * not take.
   */
  const amigaSideForm = !nothingRunnableHere && (!hasChain || targetRunsOnAmiga === true);
  /**
   * …and its opposite: this row's files are placed from Windows **and it is
   * ready to be placed**.
   *
   * **`targetReady` is not redundant here, and leaving it out was ART-286.**
   * `useHostPlacement`'s own `enabled` already carries it, so for a non-ready
   * row no `osinstall_collisions` call is ever made — while this gate, without
   * it, still rendered the block. `placement.collisions` and
   * `placement.collisionsError` then stay `null` for ever, nothing is in
   * flight, nothing can error, and the heading falls back to *"Checking what
   * this would replace…"* permanently. Measured on the owner's screen:
   * 4 min 40 s, nothing clicked, on a remembered selection sitting on an
   * already-installed row.
   *
   * A fetch gate and a render gate that disagree is a progress indicator for a
   * request nobody made — the fixed-width bar CLAUDE.md names, wearing a
   * sentence. They are one expression now.
   */
  const hostSideRow = hasChain && targetRunsOnAmiga === false && targetReady;
  const placementCounts = placement.collisions ? collisionCounts(placement.collisions) : null;
  /** The one Run button's own two questions: is it busy, and how far. */
  const running = hostSideRow ? placement.busy : busy;
  const runProgress = hostSideRow ? placement.progress : progress;
  const pct = runProgress ? fraction(runProgress) : null;
  /**
   * Whether Run may be pressed.
   *
   * The chain's rule first: **only a ready row runs**, and a row that is
   * anything else is refused here rather than by the engine a round trip
   * later — its own sentence is rendered beside the button, in the state's
   * own words. Then each route's own gates, unchanged.
   *
   * `targetReady` here is deliberately redundant — a row that is not ready
   * is never previewed (`request` and `useHostPlacement`'s `enabled` both
   * carry the same gate), so neither `preview` nor `collisions` can be
   * non-null for one. Stated again at the button because that is where
   * somebody reading this code asks the question, and because a later
   * refactor of either preview must not be able to quietly make a blocked
   * row runnable. Mutating this line alone therefore survives the suite:
   * disclosed in the round's report rather than pretended away.
   */
  /**
   * What the button says.
   *
   * **The label is the row's, not the screen's** (fix round 1, F8). A row
   * that is neither route — the CD, and a package ART can neither place nor
   * run — used to fall through to *"Run the installer on the Amiga"*, which
   * is a sentence about an emulator that will not open. The button is
   * disabled in that state, so nothing followed from it; it was still a
   * word that is not true of the row.
   */
  const runLabel = hostSideRow
    ? running
      ? "osinstall.packages.apply.running"
      : "osinstall.packages.apply.run"
    : hasChain && targetRunsOnAmiga === null
      ? "osinstall.chain.runRow"
      : running
        ? "osinstall.amigaInstall.running"
        : "osinstall.amigaInstall.run";
  const canRun = hostSideRow
    ? targetReady && !placement.busy && placement.confirmed && placement.collisions !== null
    : targetReady && !busy && confirmed && preview !== null && blockers.length === 0;

  return (
    <section className="card" style={{ marginBottom: 16 }}>
      <h2 style={{ fontSize: 16, marginTop: 0 }}>{t("osinstall.amigaInstall.heading")}</h2>
      <p className="muted" style={{ fontSize: 12, margin: "4px 0 12px" }}>
        {t("osinstall.amigaInstall.intro")}
      </p>

      {/* Before anything else, and in both UX modes: a machine window is
          about to appear on this desktop. The last round opened one without
          saying so and that was a real annoyance.

          **Only for a row that opens one.** A row ART places from Windows
          starts no emulator, and a warning about a window that will not
          appear is the same defect as no warning at all — it teaches the
          reader that this screen's warnings are decoration. */}
      {amigaSideForm && (
        <>
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
        </>
      )}
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

      {/*
        **The chain** (round 3 § 2): one row per link, in the material's own
        order, each carrying the sentence `core::osinstall::chain` decided
        for it. This is the list — the package radio below renders only when
        ART knows no chain for this release, or when the question has not
        been answered at all.
      */}
      {hasChain && (
        <div data-testid="amiga-chain" style={{ marginBottom: 12 }}>
          <div style={{ fontSize: 12, fontWeight: 600, marginBottom: 2 }}>
            {chainSummary && t(chainSummary.key, chainSummary.params)}
          </div>
          {/* Which tree the count above is about. A fraction with no tree
              named is true of nothing in particular, and this panel's tree
              is the caller's to change. */}
          <p className="faint" data-testid="amiga-chain-tree" style={{ fontSize: 11, margin: "0 0 8px", wordBreak: "break-all" }}>
            {treeRoot
              ? t("osinstall.chain.tree", { root: treeRoot })
              : t("osinstall.chain.treeNone")}
          </p>
          {/* **Where ART was told to look, when it was told nowhere**
              (fix round 1, F6). Once, above the rows, carrying the next
              step; each row says the short form of it for itself. */}
          {noFolders && (
            <p className="badge badge-warn" data-testid="amiga-chain-no-folders" style={{ display: "block", padding: "6px 12px", fontSize: 12, margin: "0 0 8px" }}>
              {t("osinstall.chain.noFolders")}{" "}
              <Link to="/os-builder/kaynak">{t("osBuilder.step.kaynak")}</Link>
            </p>
          )}
          {/* **What was not looked at** (fix round 1, F5). A remembered
              folder on a drive nobody plugged in is the ordinary case, and
              without this line every row below reads *"not in the folders
              you named"* about a folder ART could not open — the
              confident-wrong sentence, arrived at honestly. The readout's
              own phrases, so the two screens cannot word it differently. */}
          {unreadableFolders.map(({ folder, phrase }) => (
            <p
              key={folder}
              className="badge badge-err"
              data-testid="amiga-chain-unreadable-folder"
              style={{ display: "block", padding: "6px 12px", fontSize: 11, margin: "0 0 6px", wordBreak: "break-all" }}
            >
              {t(phrase.key, phrase.params)}
            </p>
          ))}
          {crowdedFolders.map(({ folder, phrase }) => (
            <p
              key={folder}
              className="faint"
              data-testid="amiga-chain-crowded-folder"
              style={{ fontSize: 11, margin: "0 0 6px", wordBreak: "break-all" }}
            >
              {t(phrase.key, phrase.params)}
            </p>
          ))}
          {entries.map(({ row, line }) => {
            const chosenRow = selected?.line === line;
            return (
              <label
                key={line.id}
                data-testid="amiga-chain-row"
                style={{
                  display: "flex",
                  gap: 8,
                  alignItems: "baseline",
                  fontSize: 12,
                  padding: "3px 0",
                  // A row that cannot be run is muted and still selectable:
                  // reading its facts is the point of it being on screen.
                  opacity: line.kind === "ready" || line.kind === "installed" ? 1 : 0.75,
                }}
              >
                <input
                  type="radio"
                  name="amiga-install-chain"
                  checked={chosenRow}
                  onChange={() => selectRow(row)}
                />
                <span style={{ wordBreak: "break-word" }}>
                  <span className="faint" style={{ marginRight: 6 }}>
                    {line.position}
                  </span>
                  {t(sentenceFor(line).key, sentenceFor(line).params)}
                  {line.where && (
                    <span className="faint" style={{ marginLeft: 6 }}>
                      {t(line.where.key, line.where.params)}
                    </span>
                  )}
                  {/* The first link is a disc, and it is not obtained here.
                      The link says where it is chosen instead of leaving a
                      row nobody can act on. */}
                  {row.packageId === null && (
                    <span style={{ marginLeft: 6 }}>
                      <Link data-testid="amiga-chain-cd-link" to="/os-builder/kaynak">
                        {t("osinstall.chain.cdLink")}
                      </Link>
                    </span>
                  )}
                </span>
              </label>
            );
          })}
        </div>
      )}

      {!hasChain && (
        <div className="muted" style={{ fontSize: 12, marginBottom: 4 }}>
          {t("osinstall.amigaInstall.package.label")}
        </div>
      )}
      {!hasChain && catalogueError && (
        <p className="badge badge-err" style={{ fontSize: 11, display: "inline-block" }}>
          {t("osinstall.amigaInstall.package.unavailableHint")}
        </p>
      )}
      {!hasChain && !catalogueFolder && !catalogueError && (
        <p className="faint" style={{ fontSize: 11, margin: "0 0 12px" }}>
          {t("osinstall.amigaInstall.package.needsFolder")}
        </p>
      )}
      {!hasChain && catalogue !== null && runnable.length === 0 && (
        <p className="faint" style={{ fontSize: 11, margin: "0 0 12px" }}>
          {t("osinstall.amigaInstall.package.none")}
        </p>
      )}
      {/*
        A package whose recipe declares an installer **nobody has run** is
        shown here, disabled, with the recipe's own reason under it — §10's
        "register an unready action rather than hiding it". Hiding it would
        have ART claim it ships nothing for a package it ships a whole recipe
        for; offering it would be a Run button for a program ART has never
        seen finish. `compose` refuses such a request as well, so this is the
        sentence rather than the guard.
      */}
      {!hasChain &&
        runnable.map((pkg) => (
          <label
            key={pkg.id}
            data-testid="amiga-package-row"
            style={{
              display: "flex",
              gap: 8,
              alignItems: "baseline",
              fontSize: 12,
              padding: "3px 0",
              opacity: pkg.notYetRunnable ? 0.6 : 1,
            }}
          >
            <input
              type="radio"
              name="amiga-install-package"
              checked={packageId === pkg.id}
              disabled={pkg.notYetRunnable !== null}
              onChange={() => {
                clearReport();
                setPackageId(pkg.id);
              }}
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
              {pkg.notYetRunnable && (
                <div
                  className="faint"
                  data-testid="amiga-package-not-yet-runnable"
                  style={{ fontSize: 11, marginTop: 2 }}
                >
                  {t(notYetRunnablePanelKey(pkg.notYetRunnable))}
                </div>
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
      {amigaSideForm && (
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
        <Field
          label={t("osinstall.amigaInstall.kickstart.label")}
          value={kickstart}
          empty={t("osinstall.amigaInstall.kickstart.none")}
          onChoose={() => void chooseKickstart()}
          choose={t("common.browse")}
          hint={t("osinstall.amigaInstall.kickstart.hint")}
        />
      </div>
      )}

      {/* The refusal. English, from Rust (ART-060), verbatim: a missing
          prerequisite names what to install first and in what order, and an
          installer too old for an emulator names the archive that fixes it.
          One translated line goes with it, saying the half ART can say in
          the user's own language — that nothing was copied. */}

      {amigaSideForm && !request && !refusal && (
        <p className="faint" style={{ fontSize: 11, margin: "0 0 12px" }}>
          {t("osinstall.amigaInstall.preview.needsChoices")}
        </p>
      )}

      {/*
        The other route's preview, and it is the `paketler` step's own — the
        same call, the same grouping, the same refusal sentences. Nothing
        about "what this would replace" is decided twice.
      */}
      {hostSideRow && (
        <div data-testid="amiga-chain-placement" style={{ marginBottom: 12 }}>
          <h3 style={{ fontSize: 14, margin: "0 0 8px" }}>
            {placementCounts
              ? t("osinstall.packages.preview.heading", { ...placementCounts })
              : t("osinstall.packages.preview.loading")}
          </h3>
          {!hostFolder && (
            <p className="faint" style={{ fontSize: 11, margin: "0 0 12px" }}>
              {t("osinstall.packages.catalogue.needsFolder")}
            </p>
          )}
          <HostPlacementPreview placement={placement} />
          <label
            style={{
              display: "flex",
              gap: 8,
              alignItems: "center",
              fontSize: 12,
              margin: "0 0 10px",
            }}
          >
            <input
              type="checkbox"
              checked={placement.confirmed}
              disabled={!placement.collisions}
              onChange={(e) => placement.setConfirmed(e.target.checked)}
            />
            {t("osinstall.packages.confirm", { count: 1 })}
          </label>
        </div>
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

      {/* The emulator confirmation belongs to the Amiga-side route alone: a
          row placed from Windows opens no window and replaces no tree, and
          its own confirmation is the one above. */}
      {amigaSideForm && (
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
      )}

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
          {/* Which row this is about. With a chain the button moves on the
              moment a run succeeds, so a report that did not name its own
              row would read as a statement about the row now selected. */}
          {ranName && (
            <p data-testid="amiga-install-report-row" className="faint" style={{ margin: "0 0 6px" }}>
              {t("osinstall.chain.reportRow", { name: ranName })}
            </p>
          )}
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

      {/* The host route's own two endings, kept by this panel rather than by
          the hook — see `placedOutcome`. Named by the row they are about, on
          the same line the Amiga-side report names its own. */}
      {(placedOutcome || placedError) && ranName && (
        <p className="faint" data-testid="amiga-placement-report-row" style={{ fontSize: 11, margin: "0 0 4px" }}>
          {t("osinstall.chain.reportRow", { name: ranName })}
        </p>
      )}
      <HostPlacementReport outcome={placedOutcome} error={placedError} />

      <div style={{ display: "flex", gap: 8, alignItems: "center", flexWrap: "wrap" }}>
        <button
          className="btn btn-primary"
          onClick={() => void (hostSideRow ? runPlacement() : runInstall())}
          disabled={!canRun}
        >
          {t(runLabel)}
        </button>
        {/* **Which row this button is about.** Named whenever the button is
            about a row other than the one on screen as selected — nothing
            selected at all, or a remembered selection that cannot run
            (ART-286) — because a button that does not say which row it will
            run is the confident action this screen exists to prevent. */}
        {hasChain && target && target !== selected && (
          <span className="faint" data-testid="amiga-chain-next" style={{ fontSize: 11 }}>
            {t("osinstall.chain.next", { name: target.line.name })}
          </span>
        )}
        {/* And when the selected row cannot be run, **its own sentence** says
            why — the state's own words, never a second wording composed
            here. */}
        {hasChain && selected && !selectionCanRun && (
          <span className="faint" data-testid="amiga-chain-cannot-run" style={{ fontSize: 11 }}>
            {t(sentenceFor(selected.line).key, sentenceFor(selected.line).params)}
          </span>
        )}
        {/* **And when there is no row at all** (fix round 1, F1). No row is
            ready and the user has selected none: both branches above are
            false, and what was left was a dead button with nothing beside
            it — ART-202's own defect, learned on this exact screen, on a
            state that is not exotic at all (a fresh tree with only the CD,
            and a finished chain, are both this).

            **Two causes, two sentences, two next steps.** "Everything is
            accounted for" and "nothing here can run yet" are not one
            sentence about a button that does nothing, and the second one
            carries the first outstanding row's own words so it says what
            is actually needed rather than that something is. */}
        {hasChain && !target && (
          <span className="faint" data-testid="amiga-chain-none-ready" style={{ fontSize: 11 }}>
            {outstanding.length === 0
              ? t("osinstall.chain.allApplied")
              : `${t("osinstall.chain.noneReady")} ${t(
                  sentenceFor(outstanding[0].line).key,
                  sentenceFor(outstanding[0].line).params
                )}`}
          </span>
        )}
        {running && (
          <span className="faint" style={{ fontSize: 11 }}>
            {pct === null
              ? t("osinstall.packages.apply.progressStarting")
              : t("osinstall.packages.apply.progressPercent", {
                  percent: Math.round(pct * 100),
                  done: runProgress?.done ?? 0,
                  total: runProgress?.total ?? 0,
                })}
            {/* The phase ART is in, as ART reports it — English, from Rust
                (ART-060). The same question as the Major, asked of the
                *running* half of this channel: an install takes minutes and
                the emulator's window is the only other sign of life, so the
                line ART is already writing should not be thrown away. */}
            {runProgress?.message?.trim() ? (
              <span data-testid="amiga-install-phase" style={{ marginLeft: 8 }}>
                {runProgress.message}
              </span>
            ) : null}
          </span>
        )}
      </div>
    </section>
  );
}
