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
  /** The package's **own** archive — the wrapper the user downloaded,
   *  `BoingBag39-1.lha`. ART unpacks it and mounts that as a third volume.
   *
   *  Required, and added by ART-185: a BoingBag's payload cannot be placed
   *  into the tree from the host at all, which is why this round exists, so
   *  the installer is on no volume ART mounts unless it comes from here. */
  packageArchive: string;
  /** Where the package's own files sit inside that unpacked wrapper,
   *  `/`-separated — `BoingBag3.9-1`. `null` takes the package's own recipe
   *  `media`, which is that same drawer as shipped data; `""` means the
   *  wrapper's own root. */
  packageDir?: string | null;
  /** The user's **own** licensed Kickstart. ART ships none and never will. */
  kickstart: string;
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
  /** The package's own archive, as the user chose it. */
  packageArchive: string;
  /** Whether it is actually there. A preview that did not ask would be
   *  describing a run with nothing to run. */
  packageArchivePresent: boolean;
  /** The drawer inside that archive the installer is expected in, or `null`
   *  for the archive's own root. */
  packageDir: string | null;
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

/** The event a finished run's own answer arrives on. */
export const AMIGA_INSTALL_EVENT = "amiga-install-result";

/** Subscribe to finished runs. A cancelled or failed job never sends one —
 *  the job bar is where those are seen. */
export async function onAmigaInstallResult(
  handler: (result: AmigaInstallResult) => void
): Promise<UnlistenFn> {
  return listen<AmigaInstallResult>(AMIGA_INSTALL_EVENT, (event) => handler(event.payload));
}
