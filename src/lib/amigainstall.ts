// Running a package's own installer on the Amiga.
// Mirrors src-tauri/src/commands/amigainstall.rs and
// src-tauri/src/core/amigainstall/{mod,packagevol,run,stage,workvol}.rs.
//
// **Nothing here decrypts anything and no protection is bypassed.** Two
// AmigaOS BoingBags carry ZipCrypto-encrypted payloads whose password belongs
// to the package's own Amiga-side `Updater` (ART-166); this runs that Updater
// where it was always meant to run, inside an emulator, which is what every
// established distribution builder does.
//
// **Three volumes, not two** (ART-185). The run mounts the distribution tree,
// the package's own wrapper unpacked, and ART's boot volume. The package's
// wrapper is plain LHA and ART unpacks it itself; the encrypted payload inside
// stays encrypted and travels as an opaque blob for the Amiga-side Updater.
// Without the third volume the installer is on no mounted disk, `CD` fails and
// ART reports that the installer said no about a program that never started.
//
// **`amigaInstallPreview` writes nothing and starts nothing** — §92's PREVIEW.
// It answers what would run, on which tree, with which package, and whether
// the two things ART cannot supply (the user's own Kickstart, an emulator)
// are there. `amigaInstallRun` is the data-changing half and returns a job id.
//
// **Four endings, not two.** `RunOutcome` is a tagged union and each tag is a
// different sentence to the user: `failed` means the installer said no,
// `timed-out` means nobody answered a requester it put up, `emulator-closed`
// means the window was shut. Only `succeeded` promotes the copy over the
// user's tree; the other three leave the original untouched and the copy in
// place, and `SettlementReport` names both paths so a report can say both
// halves. Never collapse the three into "it did not work".

import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

import type { Phrase } from "@/lib/phrase";

// ---------------------------------------------------------------------------
// Types, mirroring the Rust exactly
// ---------------------------------------------------------------------------

/** A `std::time::Duration` as serde writes it. */
export interface WireDuration {
  secs: number;
  nanos: number;
}

/**
 * How a run ended. Mirrors `core::amigainstall::RunOutcome`, whose `kind` tags
 * are kebab-case — and whose struct-variant fields are **not** renamed with
 * it, because `#[serde(rename_all)]` on an enum renames variants and not their
 * fields. `every_run_outcome_has_the_shape_the_frontend_reads` pins the JSON
 * on the Rust side and `amigainstall.test.ts` checks these four names against
 * the Rust source.
 */
export type RunOutcome =
  | { kind: "succeeded" }
  | { kind: "failed" }
  | { kind: "timed-out"; waited: WireDuration }
  | { kind: "emulator-closed"; waited: WireDuration };

/** What happened to the copy the install ran against. */
export type SettlementReport =
  /** The run succeeded and the copy is now the tree. `leftBehind` is the
   *  previous tree when something held it open and it could not be removed —
   *  not an error, but the user should be told where it is. */
  | { kind: "promoted"; tree: string; leftBehind: string | null }
  /** The run did not succeed. `original` is untouched; `copy` is what the
   *  installer did, kept for the user to look at. */
  | { kind: "kept"; copy: string; original: string };

/** What the screen asks for — the same shape for the preview and the run, so
 *  a preview can never describe a run the confirm button would not perform. */
export interface AmigaInstallRequest {
  /** The distribution tree. Never written to: the install runs against a copy
   *  and the copy replaces this only on success (§92). */
  tree: string;
  /** A package ART ships a recipe for. Anything else is refused — this does
   *  not make ART able to run whatever a user points at. */
  packageId: string;
  /** The Amiga volume the tree is mounted as, e.g. `DH0`. A bare name with no
   *  colon; `null` takes ART's default. */
  systemVolume?: string | null;
  /** The package's **own** archives. The first is the wrapper the user
   *  downloaded, `BoingBag39-1.lha`; ART unpacks it and mounts it as a third
   *  volume.
   *
   *  Required, and added by ART-185: a BoingBag's payload cannot be placed
   *  into the tree from the host at all, which is why this round exists, so
   *  the installer is on no volume ART mounts unless it comes from here.
   *
   *  **A list, and that is ART-186.** BoingBag 3.9-1's own `Updater` states
   *  45.13, which cannot install a BoingBag under an emulator; the fix
   *  shipped as a second archive, whose own readme says to copy its
   *  `BoingBag3.9-1` drawer over the package's. Every archive after the first
   *  is an overlay medium, matched by what it carries rather than by the
   *  order it was picked in. */
  packageArchives: string[];
  /** Where the package's own files sit inside that unpacked wrapper,
   *  `/`-separated — `BoingBag3.9-1`. `null` takes the package's own recipe
   *  `media`, which is that same drawer as shipped data; `""` means the
   *  wrapper's own root. */
  packageDir?: string | null;
  /** The user's **own** licensed Kickstart. ART ships none and never will. */
  kickstart: string;
  /** The user's **own** copy of the medium the package's installer verifies,
   *  as a disc image (ART-193). `null` for a package that requires none, and
   *  a refusal for one that does.
   *
   *  Both AmigaOS 3.9 BoingBags check named files on a volume called
   *  `AmigaOS3.9:` before they will do anything; without it the installer
   *  opens its own window and never finishes. Supplying the disc is meeting
   *  that check, not bypassing one — nothing here decrypts anything.
   *
   *  ART opens the image and compares the volume it *states* against the one
   *  the package's recipe declares, so the wrong disc is refused by name
   *  rather than discovered inside the emulator. */
  medium?: string | null;
  /** A machine preset id (`profileList`). `null` takes ART's default. */
  profile?: string | null;
}

/** What would run, on which tree, with which package. Read-only. */
export interface AmigaInstallPreview {
  packageId: string;
  /** The package's own name, untranslated — a package's name is its own, the
   *  way a volume's is (ART-060). */
  packageName: string;
  tree: string;
  systemVolume: string;
  /** The drawer the installer is run from, as AmigaDOS sees it. */
  workingDirectory: string | null;
  /** The installer's whole AmigaDOS path. */
  program: string;
  /** Its arguments, each its own token, in the order they are passed. */
  args: string[];
  /** ART's own volume, mounted alongside the tree and booted first. The user
   *  will see it on the Workbench, so say it exists. */
  workVolume: string;
  /** The volume the package's own unpacked wrapper is mounted as — the third,
   *  and the one ART-185 was missing. The user sees this one too. */
  packageVolume: string;
  /** The package's own archives, as the user chose them — the wrapper first,
   *  then any overlay medium. */
  packageArchives: string[];
  /** Whether **every** one of them is actually there. A preview that did not
   *  ask would be describing a run with nothing to run; asking only about the
   *  first would describe a run ART would refuse a moment later. */
  packageArchivesPresent: boolean;
  /** The overlay media this package declares, by the path inside such an
   *  archive that identifies one — so the screen can say what a second file
   *  would have to be before the user goes looking for it (ART-186). */
  declaredOverlays: string[];
  /** The lowest version the package's installer may state, `"45.15"`, or
   *  `null` when no build of it is known to be unfit. Named on the screen
   *  because it is why a second archive may be needed at all. */
  minimumInstallerVersion: string | null;
  /** The drawer inside that archive the installer is expected in, or `null`
   *  for the archive's own root. */
  packageDir: string | null;
  /** The disc the run will put in the emulated machine's CD drive, or `null`
   *  when the package needs none. A person should not be surprised by a disc
   *  appearing any more than by the machine window itself. */
  medium: string | null;
  /** The volume that image **states it has** — read from the image, never
   *  from its filename. */
  mediumVolume: string | null;
  /** What the package's own installer requires — *"the original AmigaOS 3.9
   *  CD-ROM"* — when it requires one. `null` for a package that verifies no
   *  medium, and for a disc supplied unasked. */
  requiredMedium: string | null;
  /** The file the Amiga writes and the host polls. */
  resultFile: string;
  /** How long the run may go without an answer before ART ends the emulator
   *  it started. Not optional: an Amiga Installer is interactive by nature. */
  deadlineSeconds: number;
  kickstart: string;
  /** Whether that Kickstart is actually there. The run refuses without one
   *  rather than falling back to AROS. */
  kickstartPresent: boolean;
  /** The emulator ART would start, or `null` when it found none. **A person
   *  should not be surprised by a machine window** — say this before the
   *  confirm button, not after. */
  emulator: string | null;
  profileId: string;
  profileName: string;
}

/** A finished run's own answer. `job_id` is snake_case to match every other
 *  job result in ART. */
export interface AmigaInstallResult {
  job_id: number;
  outcome: RunOutcome;
  settlement: SettlementReport;
}

// ---------------------------------------------------------------------------
// Commands
// ---------------------------------------------------------------------------

/** What running this package's own installer would do. Writes nothing and
 *  starts nothing (§92's PREVIEW). */
export async function amigaInstallPreview(
  request: AmigaInstallRequest,
  winuaePath?: string | null
): Promise<AmigaInstallPreview> {
  return invoke<AmigaInstallPreview>("amiga_install_preview", {
    request,
    winuaePath: winuaePath ?? null,
  });
}

/**
 * Run the installer inside an emulator, against a copy of the tree. Returns a
 * job id (§54) — progress on the ordinary `job-progress` event, the answer on
 * [`AMIGA_INSTALL_EVENT`].
 *
 * A cancelled job discards the copy; a failed, timed-out or closed one keeps
 * it and the result says where. Refusals — an unknown package, a package with
 * no Amiga-side installer, no emulator — reject synchronously, before any job
 * starts.
 */
export async function amigaInstallRun(
  request: AmigaInstallRequest,
  winuaePath?: string | null
): Promise<number> {
  return invoke<number>("amiga_install_run", {
    request,
    winuaePath: winuaePath ?? null,
  });
}

/**
 * What a chosen archive is, judged against a selected package — **before**
 * it goes into a request (ART-277). The wrapper is plain LHA and this reads
 * its listing alone; nothing is unpacked and the encrypted payload an update
 * archive might carry is never opened.
 *
 * `kind` is one of six shapes: `"the-package"` (the selected package's own
 * archive), `"the-update-archive"` (the selected package's own second
 * archive), `` `another-package:${id}` `` (another release package's own
 * archive, and one this screen's radio actually offers), ``
 * `another-packages-update-archive:${id}` `` (that other package's *own*
 * update archive, not its package archive), ``
 * `other-artefact:${topLevelName}` `` (the archive names something real —
 * either two or more release packages share that identity and ART will
 * never pick one of them arbitrarily, or the one package that does claim it
 * is not one this screen can run at all), or `"unknown"` (ART recognises
 * none of the above — accepted without comment, since Rust still validates
 * the real thing at `compose`).
 */
export interface ArchiveClassification {
  kind:
    | "the-package"
    | "the-update-archive"
    | "unknown"
    | `another-package:${string}`
    | `another-packages-update-archive:${string}`
    | `other-artefact:${string}`;
  /** What the archive's own listing carries at its top level — a drawer and,
   *  usually, its sibling `.info` icon. Shown when ART cannot say more. */
  topLevel: string[];
  /** The **selected** package's own `media` — what the package field itself
   *  expects. `null` when the selected id is not a package this release
   *  ships. `PackageSummary` never carries a recipe's `media`, which is why
   *  this travels with the answer rather than being looked up again. */
  expectedMedia: string | null;
  /** The selected package's own declared overlay drawers — what the
   *  update-archive field expects. Empty for a package that declares none. */
  expectedOverlays: string[];
}

/** Ask Rust what an archive is, against the package currently selected and
 *  the release currently being built — the same list the radio offers
 *  (ART-277 review, Major 2: naming a package from a *different* release, or
 *  one this screen does not even list, is an instruction the user cannot
 *  act on here). Read-only and lenient: an archive ART cannot make sense of
 *  answers `"unknown"` rather than rejecting the promise — this is asked on
 *  every file pick, and a query must not turn "I could not tell" into an
 *  error the user cannot get past. */
export async function amigainstallClassifyArchive(
  path: string,
  packageId: string,
  release: string
): Promise<ArchiveClassification> {
  return invoke<ArchiveClassification>("amigainstall_classify_archive", {
    path,
    packageId,
    release,
  });
}

/** [`ArchiveClassification.kind`], taken apart into something a `switch` can
 *  read — every shape it can be, spelled out rather than parsed again at
 *  every call site. */
export type ParsedClassification =
  | { kind: "the-package" }
  | { kind: "the-update-archive" }
  | { kind: "unknown" }
  | { kind: "another-package"; id: string }
  | { kind: "another-packages-update-archive"; id: string }
  | { kind: "other-artefact"; media: string };

export function parseClassification(
  classification: ArchiveClassification | null
): ParsedClassification | null {
  if (!classification) return null;
  const { kind } = classification;
  if (kind === "the-package" || kind === "the-update-archive" || kind === "unknown") {
    return { kind };
  }
  if (kind.startsWith("another-packages-update-archive:")) {
    return { kind: "another-packages-update-archive", id: kind.slice("another-packages-update-archive:".length) };
  }
  if (kind.startsWith("another-package:")) {
    return { kind: "another-package", id: kind.slice("another-package:".length) };
  }
  if (kind.startsWith("other-artefact:")) {
    return { kind: "other-artefact", media: kind.slice("other-artefact:".length) };
  }
  return { kind: "unknown" };
}

/**
 * Where one package's own archive/overlay-archive choice is remembered
 * (ART-277).
 *
 * **Scoped per package**, the way `rememberedComponentKey`
 * (`@/lib/osinstall`) scopes a release's component picks: a component id
 * means something only inside the recipe that declares it, and a chosen
 * archive path means something only for the package it was picked for.
 * Before this, `AmigaInstall.tsx` kept one global `"amigaInstall.archive"`
 * key for every package, so switching the panel's radio from BoingBag 1 to
 * BoingBag 2 carried BoingBag 1's own path straight into a BoingBag 2
 * request — the owner's own defect: *"'…BoingBag39-2.lha' is not this
 * package's update archive: it carries none of
 * 'BoingBag3.9-1-UAE/BoingBag3.9-1'"*, produced by a stale `packageId`
 * nothing had cleared.
 *
 * **Nothing is cleared** (CLAUDE.md: nothing changes unless the user changes
 * it) — the carry is closed structurally instead: switching packages reads a
 * *different* key, so BoingBag 1's own choice is still there, under its own
 * name, the next time BoingBag 1 is selected again.
 *
 * `null` (no package chosen yet, or the panel used with an archive picked by
 * hand before any package folder exists) keeps the base, unscoped key —
 * there is no second package to collide with while none is selected at all.
 */
export function amigaInstallArchiveKey(base: string, packageId: string | null): string {
  return packageId === null ? base : `${base}.${packageId}`;
}

/** The event a finished run's own answer arrives on. */
export const AMIGA_INSTALL_EVENT = "amiga-install-result";

/** Subscribe to finished runs. A cancelled or failed job never sends one —
 *  the job bar is where those are seen. */
export async function onAmigaInstallResult(
  handler: (result: AmigaInstallResult) => void
): Promise<UnlistenFn> {
  return listen<AmigaInstallResult>(AMIGA_INSTALL_EVENT, (event) => handler(event.payload));
}

// ---------------------------------------------------------------------------
// The sentences the screen says
// ---------------------------------------------------------------------------
//
// `src/lib` has no i18next singleton, so everything below answers a `Phrase`
// and `AmigaInstallPanel` calls `t()` on it. They live here rather than in the
// component for the reason `OsInstall.tsx`'s own review gave: "no test can
// reach [it], because it lives inside the component."
//
// **The four endings are four, and the refusals name their reason.** That is
// the whole point of this section. Three separate defects in this round were
// the same shape — ART telling the user a confidently wrong sentence — so a
// mapper here that returned one key for two endings, or a component that
// rendered "it did not work" for all four, is the defect, not a shortcut.


/** Whole seconds out of a `Duration` on the wire, rounded to the nearest —
 *  `nanos` is never large enough to matter to a sentence about minutes, but
 *  dropping it entirely would print `0` for a run that took 900 ms. */
export function waitedSeconds(waited: WireDuration): number {
  return Math.round(waited.secs + waited.nanos / 1_000_000_000);
}

/**
 * What happened, in one sentence — **a different key for every ending**.
 *
 * `failed` means the package's own installer said no. `timed-out` means it
 * put a question up and nobody answered it. `emulator-closed` means the
 * window was shut. Collapsing any two of those tells a user to do the wrong
 * thing next: "watch the window next time" is useless advice to somebody who
 * closed it themselves.
 */
export function outcomePhrase(outcome: RunOutcome): Phrase {
  switch (outcome.kind) {
    case "succeeded":
      return { key: "osinstall.amigaInstall.outcome.succeeded" };
    case "failed":
      return { key: "osinstall.amigaInstall.outcome.failed" };
    case "timed-out":
      return {
        key: "osinstall.amigaInstall.outcome.timedOut",
        params: { seconds: waitedSeconds(outcome.waited) },
      };
    case "emulator-closed":
      return {
        key: "osinstall.amigaInstall.outcome.emulatorClosed",
        params: { seconds: waitedSeconds(outcome.waited) },
      };
  }
}

/** What to do about it — again one per ending, because the next step is what
 *  actually differs between them. */
export function outcomeNextStepPhrase(outcome: RunOutcome): Phrase {
  switch (outcome.kind) {
    case "succeeded":
      return { key: "osinstall.amigaInstall.next.succeeded" };
    case "failed":
      return { key: "osinstall.amigaInstall.next.failed" };
    case "timed-out":
      return { key: "osinstall.amigaInstall.next.timedOut" };
    case "emulator-closed":
      return { key: "osinstall.amigaInstall.next.emulatorClosed" };
  }
}

/** How the report is coloured. Never the only signal — each ending already
 *  says which it is in words — but a success and a refusal must not look
 *  alike at a glance either. */
export function outcomeTone(outcome: RunOutcome): "ok" | "warn" | "err" {
  switch (outcome.kind) {
    case "succeeded":
      return "ok";
    case "failed":
      return "err";
    case "timed-out":
    case "emulator-closed":
      return "warn";
  }
}

/**
 * Where the tree and the copy are now.
 *
 * **A user told "it failed" and not told where the evidence went has been
 * given nothing** — so a `kept` settlement always names the copy *and* says
 * the original was not touched, and a `promoted` one with a `leftBehind`
 * names the retired tree ART could not delete rather than staying silent
 * about a directory the user did not ask for.
 */
export function settlementPhrase(settlement: SettlementReport): Phrase {
  if (settlement.kind === "promoted") {
    return settlement.leftBehind === null
      ? { key: "osinstall.amigaInstall.settlement.promoted", params: { tree: settlement.tree } }
      : {
          key: "osinstall.amigaInstall.settlement.promotedLeftBehind",
          params: { tree: settlement.tree, leftBehind: settlement.leftBehind },
        };
  }
  return {
    key: "osinstall.amigaInstall.settlement.kept",
    params: { copy: settlement.copy, original: settlement.original },
  };
}

/**
 * What a package's declared minimum installer version means for the archives
 * the user has picked — ART-186, and the obligation task 4's review handed
 * this screen.
 *
 * BoingBag 3.9-1's shipped `Updater` states 45.13 and cannot install under an
 * emulator; the fix is 45.15, carried by a **separate** archive
 * (`BoingBag39-1-UAE.lha`, whose own readme says "You can install the BoingBag
 * on UAE now"). ART refuses the older build — and a refusal a user could have
 * avoided with one download must be visible *before* the run, naming the
 * archive to go and get.
 *
 * `null` for a package that declares no minimum: inventing a requirement
 * nobody stated is the same §89 mistake from the other side.
 */
export function overlayAdvicePhrase(preview: AmigaInstallPreview): Phrase | null {
  if (preview.minimumInstallerVersion === null) return null;
  const overlays = preview.declaredOverlays.join(", ");
  return preview.packageArchives.length > 1
    ? {
        key: "osinstall.amigaInstall.overlay.supplied",
        params: { version: preview.minimumInstallerVersion, overlays },
      }
    : {
        key: "osinstall.amigaInstall.overlay.needed",
        params: { version: preview.minimumInstallerVersion, overlays },
      };
}

/**
 * Everything the previewed run still lacks, each named — never one "not
 * ready" badge over three different problems, and never a Run button that
 * starts a job only for the command layer to refuse it a moment later.
 *
 * Empty means every one of the three things ART cannot supply itself (the
 * user's own Kickstart, the package's own archives, an emulator) is there.
 */
export function readinessBlockers(preview: AmigaInstallPreview): Phrase[] {
  const blockers: Phrase[] = [];
  if (!preview.packageArchivesPresent) {
    blockers.push({
      key: "osinstall.amigaInstall.blocker.archiveMissing",
      params: { count: preview.packageArchives.length },
    });
  }
  if (!preview.kickstartPresent) {
    blockers.push({
      key: "osinstall.amigaInstall.blocker.kickstartMissing",
      params: { path: preview.kickstart },
    });
  }
  if (preview.emulator === null) {
    blockers.push({ key: "osinstall.amigaInstall.blocker.noEmulator" });
  }
  return blockers;
}

/**
 * What an archive field's own classification means for Run — ART-277,
 * folded into ART-277 review's Medium 2 fix.
 *
 * `field` says which slot this classification is for: `"package"` is the
 * package's own archive field, `"overlay"` the second/update-archive field.
 * A `Phrase` here is meant to be pushed into the **same** `readinessBlockers`
 * list the preview's own blockers render in — directly above the confirm
 * checkbox and the Run button — rather than shown a second time beside the
 * field. ART-202's own lesson, from this exact screen: a reason Run is dead
 * has to say so where the button is, and the first attempt at this said it
 * only beside the field, a screen's height away.
 *
 * Four outcomes:
 * - **Another release package's own archive**, or its own update archive
 *   (Medium 1) — named by `otherName`, telling the user to select that
 *   package instead, because this screen's radio actually offers it.
 * - **Something real ART cannot offer a selection for** — two or more
 *   release packages share this identity (ART-276's own shape, reached from
 *   a different door: never resolved by guessing), or the one package that
 *   does claim it is not Amiga-installable at all (`other-artefact`, ART-277
 *   review's Major 2 fix). Named by what it is and what this field expects,
 *   never by an id the user cannot act on here.
 * - **This package's own archive, in the wrong field** — `the-package` in
 *   the *overlay* field (Major 1: the mirror of the update archive already
 *   caught by the equivalent case at the other end, which Rust's own preview
 *   refuses on its own) and `the-update-archive` in the *package* field.
 * - **Nothing** — an archive ART does not recognise, or one that is exactly
 *   right, is accepted silently: Rust still validates the real thing at
 *   `compose`, and inventing a warning for "unknown" would be a confident
 *   guess about a file this function cannot actually place.
 */
export function archiveFieldBlockerPhrase(
  classification: ArchiveClassification | null,
  field: "package" | "overlay",
  path: string,
  selectedName: string,
  otherName: (id: string) => string
): Phrase | null {
  const parsed = parseClassification(classification);
  if (!parsed) return null;
  switch (parsed.kind) {
    case "another-package":
      return {
        key: "osinstall.amigaInstall.classify.anotherPackage",
        params: { other: otherName(parsed.id), selected: selectedName },
      };
    case "another-packages-update-archive":
      return {
        key: "osinstall.amigaInstall.classify.anotherPackagesUpdateArchive",
        params: { other: otherName(parsed.id), selected: selectedName },
      };
    case "other-artefact": {
      const expected =
        field === "package"
          ? (classification?.expectedMedia ?? null)
          : (classification?.expectedOverlays.join(" or ") ?? null) || null;
      return expected
        ? {
            key: "osinstall.amigaInstall.classify.otherArtefact",
            params: { path, media: parsed.media, selected: selectedName, expected },
          }
        : {
            key: "osinstall.amigaInstall.classify.otherArtefactGeneric",
            params: { path, media: parsed.media },
          };
    }
    case "the-package":
      return field === "overlay" ? { key: "osinstall.amigaInstall.classify.wrongFieldPackage" } : null;
    case "the-update-archive":
      return field === "package" ? { key: "osinstall.amigaInstall.classify.wrongFieldOverlay" } : null;
    default:
      return null;
  }
}
