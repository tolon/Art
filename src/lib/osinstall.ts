// Installing AmigaOS from the user's own media (SD-2 · G5).
// Mirrors src-tauri/src/commands/osinstall.rs and
// src-tauri/src/core/osinstall/{plan,apply,verify,scan}.rs.
//
// **`osinstallApply` takes the plan it is given and does not recompute it.**
// The same rule `layoutApply` follows, for the same reason (see
// src/lib/layout.ts's own note): the user's component choices *are* the
// plan, so a screen that previewed one install must not be able to build
// another. `osinstallPlan` and `osinstallScanMedia` are read-only previews —
// §92's PREVIEW — and neither writes anything.
//
// **A bad media folder is a value, not a thrown error.** Both
// `osinstallScanMedia` and `osinstallPlan` answer with an `outcome` tag
// rather than rejecting — `"folder-unreadable"` for the single most likely
// mistake after a bad ROM (a wrong path, or a folder ART cannot read), so
// the screen can translate it (ART-060) instead of showing a raw sentence.
//
// **`verified` is never just `failed === 0`.** A `VerifyReport` carries all
// three counts — `passed`, `failed`, `notChecked` — because "ART did not
// look" is not "ART found nothing wrong" (§89). Show all three.

import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

import { awaitJobResult } from "@/lib/jobs";
import { formatBytes } from "@/lib/panel";
import type { Phrase } from "@/lib/phrase";

// ---------------------------------------------------------------------------
// Types, mirroring core::osinstall exactly
// ---------------------------------------------------------------------------

/** Which reader opened this medium. Mirrors `core::osinstall::scan::MediaKind`. */
export type MediaKind = "floppy" | "disc";

/** One install disk or disc `osinstallScanMedia` opened successfully. */
export interface FoundMedia {
  path: string;
  /** Read from **inside** the image — never derived from `path`. */
  volumeName: string;
  kind: MediaKind;
  /**
   * Which media layer this disk was found in, by the layer's own id.
   *
   * Absent, not `null` — an **unlayered** scan has no layer to name, which is
   * every scan before layered recipes existed and still every scan
   * `osinstallScanMedia` runs today. The Rust side skips the field entirely
   * rather than serialising it as `null` (`#[serde(skip_serializing_if)]`),
   * so an old-shaped result and a layered one are told apart by whether the
   * key is there at all.
   */
  layer?: string;
}

/** What scanning a media folder found, or why it could not be looked at. */
export type MediaScanResult =
  | { outcome: "found"; media: FoundMedia[] }
  | { outcome: "folder-unreadable"; folder: string };

/**
 * Whether ART may reuse a medium's listing from an earlier scan (ART-194).
 *
 * Kebab-case because the Rust enum is `#[serde(rename_all = "kebab-case")]`.
 * Leaving the field off means `"reuse"` — the fast path is the ordinary one.
 */
export type ScanCachePolicy = "reuse" | "ignore";

export interface InstallRequest {
  mediaFolder: string;
  /**
   * More folders holding install media, read alongside `mediaFolder`
   * (work-list item 8).
   *
   * **AmigaOS 3.2.2.1 is not one folder of disks**: it is the user's own 3.2
   * ADFs plus the update disks plus the hotfix disk, and Hyperion ships the
   * last two as `ADFs/Update/` and `ADFs/Hotfix/` inside a single download. A
   * model with one media folder cannot express that install at all.
   *
   * A second field rather than a list, because the first folder is the
   * question this screen has always asked and the one a wrong-folder verdict
   * is about. **The order is not a precedence rule**: the same volume name in
   * two folders is refused by name, never resolved by which came first.
   *
   * Optional on the wire (`#[serde(default)]` on the Rust side), so a request
   * built before this existed still deserialises.
   */
  extraMediaFolders?: string[];
  /**
   * One media folder per layer a layered recipe declares, keyed by the
   * layer's own id (`core::osinstall::MediaLayer::id`).
   *
   * `mediaFolder` and `extraMediaFolders` above stay for the reason they
   * were given `#[serde(default)]` in the first place: a request built
   * before this field existed must still work. When this map is empty they
   * are read exactly as before, onto the single implicit layer — an
   * unlayered recipe never reads this field at all.
   */
  mediaFolders?: Record<string, string>;
  /**
   * The keyboard layout the finished system boots with — a name in
   * `Devs/Keymaps` (ART-226's other half).
   *
   * The `keymaps` component *places* every layout the media carries; nothing
   * selected one, so a tree built for a Turkish user rendered `ç ü ş Ğ` in its
   * menus and still typed on an American keyboard. ART writes
   * `SetKeyboard <name>` into its own marked block in `S:User-Startup`, which
   * both the 3.2 and the 3.9 tree's own `S/Startup-Sequence` ends by
   * executing.
   *
   * Optional on the wire, and omitting it leaves the system on the ROM's
   * `usa` — a default here would be ART choosing somebody's keyboard.
   */
  keymap?: string | null;
  /** The paired Kickstart, if supplied. `null` refuses any component whose
   *  condition needs it decided. */
  rom: string | null;
  /** Component ids the user picked. `required` and condition-satisfied ones
   *  are added on top of this, not instead of it. */
  chosen: string[];
  /**
   * Condition-satisfied component ids the user has explicitly turned off
   * (spec requirement 2 — "turning Modules off is a confirmation, not a
   * refusal"). Subtracted **on the Rust side**, inside
   * `core::osinstall::plan::resolve_components_on`, before that
   * component's own media is ever resolved — never client-side.
   *
   * A fix-round correction, not the original shape: the first version of
   * this screen filtered a returned `InstallPlan` in the browser, which
   * left `mediaPaths` (and so `apply()`'s manifest `built_from`) stale, and
   * left an excluded component's own missing-media refusal standing —
   * exactly the "turning Modules off is still a refusal, just a politer
   * one" failure requirement 2 exists to prevent. Only the engine can
   * un-derive a refusal it itself raised; see `plan.rs`'s own doc comment
   * on `InstallRequest::excluded` for the Rust half of this.
   */
  excluded: string[];
  destination: string;
  /** Which shipped recipe to plan from — a `Recipe.release` string. */
  release: string;
  /**
   * Whether ART may reuse an unchanged medium's listing (ART-194). Optional,
   * and absent means `"reuse"` — the Rust side's own `#[serde(default)]`, so
   * the fast path costs no caller anything to opt into.
   */
  scanCache?: ScanCachePolicy;
}

/**
 * The releases ART ships a recipe for, in the order the picker lists them.
 *
 * Hand-maintained against `core::osinstall::recipe::releases()`. The Rust
 * side refuses a name it does not know rather than defaulting, so a stale
 * entry here surfaces as a refusal on screen and never as the wrong
 * operating system being written.
 */
export const INSTALL_RELEASES = ["AmigaOS 3.2", "AmigaOS 3.2.2", "AmigaOS 3.9"] as const;

export type InstallRelease = (typeof INSTALL_RELEASES)[number];

/**
 * Whether a persisted value is still one of the releases ART ships.
 *
 * Takes `unknown`, not `string` — this is what `useRemembered` needs to call
 * it as a `Guard<InstallRelease>` against a raw settings-file value, which
 * may not even be a string (a hand-edited or stale settings file).
 */
export function isInstallRelease(value: unknown): value is InstallRelease {
  return typeof value === "string" && (INSTALL_RELEASES as readonly string[]).includes(value);
}

/** Whether a rule takes one file, a whole subtree, or amends an icon already
 *  in the tree. `"icon-tooltypes"` merges an icon's tool types and stack
 *  size into the file already at `to`, rather than copying `from` over it —
 *  see `PlanItem.mergeIcon`. */
export type RuleKind = "file" | "subtree" | "icon-tooltypes";

/** Why an install cannot proceed. A value, never a sentence (ART-060) — the
 *  screen translates it. */
/**
 * Why ART cannot place a package's files from the host at all — a property
 * of the *package*, never of the user's folder. Mirrors
 * `core::osinstall::HostPlacementBlock`.
 *
 * A union of string literals rather than a `string`, so every place that
 * has to say something about a block (`refusalPhrase` below, the checklist
 * row in `PackagePanel`) fails to compile rather than silently rendering
 * "unavailable, no reason given" when a second kind arrives.
 *
 * `"encrypted-payload"` is ART-166: both shipped BoingBag recipes name a
 * payload archive whose every entry is password-encrypted, and the password
 * belongs to the BoingBag's own Amiga-side `Updater`. ART will not bypass
 * it (the owner's recorded decision), so the tick must be refused with a
 * sentence naming the Updater rather than accepted and answered later with
 * a raw English ZIP error.
 *
 * `"needs-fixfonts"` is Euro-Update: it replaces bitmap fonts and ships its
 * own `.font` descriptors, and its own installer runs `FixFonts` afterwards
 * to rebuild those indexes from what the drawer actually holds. ART cannot
 * rebuild one, so placing the files alone would leave the index naming only
 * the sizes this package happens to ship — a font that quietly stops being
 * available, with nothing failing and nothing logged.
 *
 * `"needs-installer-script"` is BoingBags 3&4: plain files, and an Installer
 * script that chooses among them by CPU, by machine, by the languages it
 * asks for and by which assigns exist, then edits the startup-sequence. ART
 * places a fixed set of paths and cannot answer those questions.
 */
export type HostPlacementBlock =
  | "encrypted-payload"
  | "needs-fixfonts"
  | "needs-installer-script";

/**
 * Why a declared Amiga-side installer is not one ART will start. Mirrors
 * `core::osinstall::package::NotYetRunnable`.
 *
 * A union of string literals rather than a `string`, for the reason
 * {@link HostPlacementBlock} is one: every place that has to say something
 * about it fails to compile rather than rendering a blank explanation when a
 * second kind arrives.
 *
 * `"installer-not-measured"` is BoingBags 3&4: it installs through an
 * Installer *script*, and nobody has measured whether that script finishes
 * without a person at the window.
 */
export type NotYetRunnable = "installer-not-measured";

export type RefusalReason =
  | { refusal: "media-missing"; component: string; volume_name: string }
  | { refusal: "media-path-missing"; component: string; media: string; path: string }
  // ART-119 (#5). The disk is there and cannot be opened or walked — the
  // third face of the same per-component fact `media-missing` and
  // `media-path-missing` already carry, and no longer a hard error that
  // blanked every other component's plan with it. `reason` is the Rust
  // reader's own English sentence (ART-060), shown after the translated one.
  | {
      refusal: "media-unreadable";
      component: string;
      volume_name: string;
      path: string;
      reason: string;
    }
  | { refusal: "rom-unknown" }
  // The paired Kickstart WAS identified — only its own resident module
  // table could not be read, so a `resident-older-than` condition naming
  // `resident` cannot be decided for `component`. A different fact from
  // `rom-unknown` (review fix round 1, F1): that one is undecidable because
  // ART cannot tell what ROM this is at all; this one names a ROM that is
  // fine and a specific question about it ART could not answer, and it is
  // named per component rather than deduplicated across the whole plan,
  // because two components can ask about two different residents.
  | { refusal: "resident-table-unreadable"; component: string; resident: string }
  | { refusal: "destination-collision"; path: string; components: string[] }
  | {
      refusal: "media-ambiguous";
      component: string;
      volume_name: string;
      paths: string[];
    }
  // Two of a layered recipe's own layers were pointed at one folder — caught
  // before any component's media is even looked for, because the folder
  // that would tell the base release's disk apart from the update's disk
  // sharing its name was never given.
  | { refusal: "layers-share-folder"; layers: string[]; folder: string }
  | { refusal: "exclusive-group-conflict"; group: string; components: string[] }
  | {
      refusal: "rule-kind-mismatch";
      component: string;
      from: string;
      expected: RuleKind;
      found: RuleKind;
    }
  // ---- Packages (Task 7) — the six variants `plan()` could already raise
  // but nothing on screen could reach until packages became selectable.
  // `rename_all = "kebab-case"` on `RefusalReason` renames the `refusal` tag
  // only, never a struct variant's own field names (see
  // `commands/osinstall.rs`'s `refusal_reason_tag_and_field_spellings_for_every_variant`,
  // which pins exactly this) — so every field below stays spelled as Rust
  // wrote it, same as the seven above.
  | { refusal: "package-unknown"; package: string }
  | { refusal: "package-folder-missing"; packages: string[] }
  // **Both fields are display *names*, not ids** (fix round 1, M1): the
  // sentence renders them verbatim, and `locale-turkish` is ART's own
  // bookkeeping.
  | { refusal: "package-requirement-missing"; package: string; requires: string }
  /**
   * The required package is one that **runs on the Amiga**, so *"tick that
   * one too"* is advice about a checkbox `PackagePanel` disables.
   *
   * Correcting `locale-turkish`'s `requires` to what the material states
   * made it need BoingBag 3.9-2 — `encrypted-payload` blocked, and
   * untickable by design — so the owner's own Turkish catalogue pack became
   * unaddable with an instruction they could not follow. This variant names
   * the step that can actually do it.
   */
  | {
      refusal: "package-requirement-needs-amiga-run";
      package: string;
      requirement: string;
    }
  | { refusal: "package-component-missing"; package: string; component: string }
  | { refusal: "package-archive-missing"; package: string; media: string }
  | {
      refusal: "package-archive-ambiguous";
      package: string;
      media: string;
      paths: string[];
    }
  // ---- M3 / ART-166: a package ART cannot place from the host at all.
  | { refusal: "package-not-placeable-on-host"; package: string; block: HostPlacementBlock }
  /** A component asks to switch something on that no rule puts on the tree.
   *  See `Activation` on the Rust side for the gap this closes. */
  | {
      refusal: "activation-source-missing";
      component: string;
      name: string;
      from: string;
    };

/** One file or directory `osinstallApply` would place in the distribution
 *  tree. */
export interface PlanItem {
  component: string;
  /** The volume name the bytes came from — not the image's own filename. */
  media: string;
  from: string;
  to: string;
  isDir: boolean;
  /** Whether ART expands these bytes on the way in (ART-228).
   *
   *  AmigaOS 3.2's media ships most of its Locale content `compress`-format,
   *  and the release's own Installer expands it and drops the `.Z`. `to`
   *  already carries the name without the suffix, so this is the only field
   *  that says the file on the medium and the file in the tree are not the
   *  same bytes. */
  decompress: boolean;
  bytes: number;
  /** Whether this is a `"icon-tooltypes"` rule's item — `osinstallApply`
   *  amends the icon already at `to` with the source's tool types and stack
   *  size rather than copying `from` over it. `false` for every `"file"` and
   *  `"subtree"` item, which is every item a recipe produced before this
   *  rule kind existed. */
  mergeIcon: boolean;
}

/** One switched-on component's own contribution to `S:User-Startup`. */
export interface UserStartupContribution {
  component: string;
  lines: string[];
}

/**
 * What planning an install produced: either a full description of what
 * would be written, or every reason it cannot proceed — never both. Any
 * refusal at all empties `items` and `mediaPaths`; check `refusals.length`
 * to tell the two cases apart.
 */
/** What a medium looked like when the plan was made — enough to notice a
 *  different disc of the same name, and deliberately not a hash: planning
 *  runs again on every tick, and hashing a 469 MB disc each time is what
 *  ART-195 was about. */
export interface MediaStamp {
  size: number;
  /** 0 when the platform gave no modification time. */
  mtimeNanos: number;
}

/** One switch the finished tree will have flipped, and who asked for it. */
export interface PlannedActivation {
  component: string;
  /** `CD0`, `NTSC`. */
  name: string;
  /** Where the media leaves it — `Storage/Monitors/NTSC`. */
  from: string;
  /** Where AmigaOS will look — `Devs/Monitors/NTSC`. */
  to: string;
}

/** One destination `osinstallApply` will delete from the tree after every
 *  placement has run — an update deleting a file its own base release
 *  placed, `Tools/TextEditFileTypes/Default4Types` in AmigaOS 3.2.2 being
 *  the real case. */
export interface PlanRemoval {
  component: string;
  to: string;
}

export interface InstallPlan {
  release: string;
  items: PlanItem[];
  refusals: RefusalReason[];
  /** The bytes the finished tree will hold — one count per destination, so a
   *  path two components both write is counted once (ART-205). Not the bytes
   *  `apply` will read: an override reads two files and leaves one. */
  totalBytes: number;
  /** What the finished tree will have switched **on** — a driver or commodity
   *  the media leaves in `Storage/` or `Tools/Commodities`, copied to where
   *  AmigaOS actually reads it. Empty unless a recipe asks; nothing shipped
   *  does. */
  activations: PlannedActivation[];
  /** What each medium looked like when this plan was made, by volume name.
   *  `apply` refuses a disc that has changed since the preview — the name
   *  check alone could not tell one `Workbench3.2` from another. */
  mediaStamps: Record<string, MediaStamp>;
  /** The files the finished tree will hold, on the same one-per-destination
   *  rule. Not `items.length`, which is the work: the owner's real 3.9 disc
   *  planned 1517 items and produced 1242 files and 105 drawers. */
  totalFiles: number;
  /** Every component id switched on — required, chosen, or turned on by its
   *  own condition — regardless of whether its media could be found. */
  componentsOn: string[];
  /** Volume name -> the image it was found in. */
  mediaPaths: Record<string, string>;
  /** The chosen package ids, in the order the engine applies them — which
   *  is dependency order, not the order the boxes were ticked in.
   *
   *  Carried even when the plan refuses, the same way `componentsOn` is, so
   *  a screen can tell "no packages asked for" from "these were asked for
   *  and refused". When a refusal stopped the plan before the order could be
   *  worked out, this is the request's own list as given — the Rust side
   *  states no order for a run that will not happen. */
  packages: string[];
  /** Package media name -> the archive it was found in, and the member
   *  inside it that holds the payload. The package half of `mediaPaths`;
   *  kept apart because the two are found by different scans of different
   *  folders. */
  packageMedia: Record<string, { path: string; member: string | null }>;
  userStartup: UserStartupContribution[];
  /** Every destination a switched-on component removes. Not emptied on a
   *  refusal, unlike `activations` — a removal names a destination
   *  declaratively, the same way a `userStartup` line does. Empty for every
   *  shipped recipe until AmigaOS 3.2.2's own recipe uses the field. */
  removals: PlanRemoval[];
  /** Which folder each of the recipe's own layers was actually read from —
   *  empty for an unlayered recipe (every shipped recipe until AmigaOS
   *  3.2.2's own two-layer one) and emptied on a refusal, the same rule
   *  `mediaPaths` follows. Not optional: the Rust side has no
   *  `skip_serializing_if` on this field, so it is always present on the
   *  wire — a plan from an older ART build is never what this type
   *  describes, since the frontend only ever receives a plan the running
   *  backend just serialized. */
  layers: LayerRecord[];
}

/** One entry in [`InstallPlan.layers`] — which folder a layer's media
 *  actually came from. `id` matches a recipe's own `MediaLayer.id`
 *  (`"base"`, `"update-3.2.2"`), never the empty string an unlayered
 *  recipe's own internal bookkeeping uses. */
export interface LayerRecord {
  id: string;
  folder: string;
}

/** What planning found, or why the media folder itself could not be looked
 *  at. */
export type PlanResult =
  | { outcome: "planned"; plan: InstallPlan }
  | { outcome: "folder-unreadable"; folder: string };

/** What happened when `osinstallApply` tried to remove one destination —
 *  see `PlanRemoval`. `"not-present"` is its own outcome, not a failure: the
 *  component that would have placed the path may simply have been switched
 *  off. `failed` carries the core's own sentence — the screen must never
 *  claim a removal succeeded when the core said it could not. */
export type RemovalState = "removed" | "not-present" | { failed: string };

export interface RemovalVerdict {
  to: string;
  state: RemovalState;
}

/** What happened when `osinstallApply` tried to amend an icon already in the
 *  tree with a `mergeIcon` item. `"destination-absent"` is its own outcome,
 *  not a failure — modeled on `RemovalState` for the identical reason: the
 *  component that would have placed the icon may simply be switched off.
 *  `failed` carries the core's own sentence. */
export type IconMergeState = "merged" | "destination-absent" | { failed: string };

export interface IconMergeVerdict {
  to: string;
  state: IconMergeState;
}

/** What `osinstallApply` actually did. */
export interface ApplyOutcome {
  root: string;
  files: number;
  directories: number;
  bytes: number;
  /** One verdict per `InstallPlan.removals` entry, by name — never
   *  collapsed, and never silent (CLAUDE.md's "reported per entry, by name
   *  and by result"). */
  removed: RemovalVerdict[];
  /** One verdict per `mergeIcon` item, by destination — never folded into
   *  `files`/`bytes`, which already account for the icon through whichever
   *  item placed it first. */
  icons: IconMergeVerdict[];
  /** How many entries in `icons` came back `failed` — `"destination-absent"`
   *  does not count (a skip is not a failure).
   *
   *  Named for exactly what it counts: it does **not** include a `removed`
   *  entry whose state is `failed`. There is deliberately no single
   *  outcome-wide failure tally (fix round 1) — the per-entry verdict lists
   *  (`icons`, `removed`) are the truth, and a screen wanting a combined
   *  "did anything fail" computes it from both rather than trusting one
   *  number that could quietly disagree with them. */
  iconMergeFailures: number;
  /** One verdict per extra payload unit the package declares — **including
   *  the ones that did not run**. "Not needed, the tree is already newer"
   *  and "never considered" are two different things to tell somebody, and
   *  an absent row says the second while meaning the first. Empty for every
   *  package that declares none, which is all but BoingBag 3.9-2. */
  extraMembers: ExtraMemberVerdict[];
  /** What the package's own host-side after-steps did to the tree once the
   *  last file was written, in order. Reported rather than implied:
   *  `Devs/AmigaOS ROM Update` being rotated is the difference between a
   *  tree whose ROM update loads and one whose does not. */
  postPlace: AppliedStep[];
}

/** Whether an extra payload unit ran, and what the tree said when it was
 *  asked. Four states, and `"not-checked"` must never read as `"applied"` —
 *  that would be ART claiming a version reading it never made. */
export type ExtraMemberState =
  | { state: "unconditional" }
  | { state: "applied"; path: string; stated: string; at_least: number }
  | { state: "not-needed"; path: string; stated: string; at_least: number }
  | { state: "not-checked"; path: string };

export interface ExtraMemberVerdict {
  /** The member's own name inside the wrapper — `XAD-Update`. */
  member: string;
  state: ExtraMemberState;
  /** How many files this unit placed; `0` for one that did not run. */
  files: number;
}

/**
 * What one host-side after-step actually did — `core::amigainstall::finish`'s
 * `AppliedStep`, tagged by `step` with **that enum's own variant names**.
 *
 * **Corrected 2026-09-08 (review F2).** These used to read `"protect"` and
 * `"replace-keeping-backup"`, which are `PostStep`'s spellings — a different
 * enum, the one a *recipe* writes. `finish::AppliedStep` derives
 * `#[serde(tag = "step", rename_all = "kebab-case")]` over `Protected` and
 * `Replaced`, so the wire carries `"protected"` and `"replaced"`, and the
 * second arm carries `replacement` as well. Nothing consumed `postPlace` yet,
 * which is exactly why it would have bitten: the first screen to render what
 * the after-steps did would have matched neither arm and rendered nothing —
 * a screen saying less than the core did, silently.
 *
 * `apply_outcome_serializes_with_the_keys_the_frontend_declares`
 * (`commands/osinstall.rs`) pins both tags against a non-empty list.
 */
export type AppliedStep =
  | { step: "protected"; path: string; was: string; now: string }
  | {
      step: "replaced";
      target: string;
      replacement: string;
      backup: string | null;
    };

export const OSINSTALL_EVENT = "osinstall-result";

/** What the finished tree's own `Prefs/Env-Archive/Versions/Release` states,
 *  compared with the release this build was for (Task 9) — CLAUDE.md's
 *  answer to the round that shipped AmigaOS 3.5 labelled 3.9. Five states,
 *  never one pass/fail bit: a confirmed marker, a mismatched one naming both
 *  sides, one that differs for a release nobody has measured, none at all,
 *  or one ART could not even read — five different sentences with five
 *  different next steps. `"unstated"` is not a failure — most releases ART
 *  ships have never had an `Update/Release` to write one. `"unreadable"`
 *  (fix round 1, Finding 1) is never the same sentence as `"unstated"`: the
 *  tree may well state a release, ART simply could not read it — an
 *  oversized or otherwise unreadable marker file, the same defect shape
 *  Task 7's `"resident-table-unreadable"` refusal exists to keep apart from
 *  a genuine absence. `"expected-unknown"` (final whole-branch review,
 *  Finding E) is never `"mismatch"` either: the Rust side only reports
 *  `"mismatch"` for a release it has actually measured a correct tree's own
 *  marker for (AmigaOS 3.2 and 3.2.2 today) — for any other release, a
 *  differing marker comes back here instead, so a correct AmigaOS 3.9 tree
 *  is never told it disagrees with a formula nobody has checked. */
export type StatedRelease =
  | { verdict: "confirmed"; stated: string }
  | { verdict: "mismatch"; expected: string; stated: string }
  | { verdict: "unstated" }
  | { verdict: "unreadable"; detail: string }
  | { verdict: "expected-unknown"; stated: string };

export interface OsInstallResult {
  job_id: number;
  destination: string;
  outcome: ApplyOutcome;
  stated_release: StatedRelease;
}

/** Whether one claim about a file was confirmed, contradicted, or never
 *  looked at. `not-checked` is not a soft pass — see the module note. */
export type CheckState = "pass" | "fail" | "not-checked";

export interface FileVerdict {
  path: string;
  state: CheckState;
  /** Why, whenever `state` is anything but a clean pass. Always present when
   *  `state` is `"not-checked"`. */
  detail: string | null;
}

export interface VerifyReport {
  files: FileVerdict[];
  passed: number;
  failed: number;
  notChecked: number;
}

// ---------------------------------------------------------------------------
// The commands
// ---------------------------------------------------------------------------

/**
 * Every install disk found directly inside `mediaFolder` — before any ROM or
 * component has been chosen, so the screen can show what it found the
 * moment a folder is picked. Writes nothing.
 */
/**
 * The keyboard layouts this plan would actually place, sorted.
 *
 * **Read off the plan's own items**, never from a list ART keeps: the picker
 * can then only offer a layout that will really be in `Devs/Keymaps`, so the
 * `keymap-missing` refusal is reachable only by a stale remembered value —
 * which is exactly when it should fire.
 */
export function keymapsIn(plan: InstallPlan | null): string[] {
  if (!plan) return [];
  const names = new Set<string>();
  for (const item of plan.items) {
    if (item.isDir) continue;
    const parts = item.to.split("/");
    if (
      parts.length === 3 &&
      parts[0].toLowerCase() === "devs" &&
      parts[1].toLowerCase() === "keymaps" &&
      // `.info` is the icon beside the keymap, not a layout anybody can pick.
      !parts[2].toLowerCase().endsWith(".info")
    ) {
      names.add(parts[2]);
    }
  }
  return [...names].sort((a, b) => a.localeCompare(b));
}

export async function osinstallScanMedia(mediaFolder: string): Promise<MediaScanResult> {
  return invoke<MediaScanResult>("osinstall_scan_media", { folder: mediaFolder });
}

// ---------------------------------------------------------------------------
// Identifying media by content hash — mirrors `core::osinstall::mediahash`
// ---------------------------------------------------------------------------

/** What kind of medium a row's bytes are. Mirrors
 *  `core::osinstall::mediahash::MediaKindTag`. An adopted row states none of
 *  this — Hatcher's data has no such field — and defaults to `"floppy"`,
 *  which is what all 186 of them are; never render that default as a claim
 *  the row actually made (see {@link MediaRow.tableOrigin}). */
export type MediaKindTag = "floppy" | "disc" | "archive";

/** Which of the two compiled-in tables a row came from. Mirrors
 *  `core::osinstall::mediahash::Origin`. */
export type MediaTableOrigin = "adopted" | "own";

/**
 * One row of the install-media tables ART compiles in — 186 rows adopted
 * from Emu68 Hatcher (MIT), plus ART's own table of AmigaOS 3.9 update
 * archives the adopted one does not carry (design's §3.6, 2026-09-08).
 * Mirrors `core::osinstall::mediahash::MediaRow`.
 *
 * Every field is **as the table states it** and none of them may be
 * re-derived here. Two traps the Rust side documents and this side inherits:
 * the mapping is many-to-one (one logical disk has many hashes —
 * `Workbench3_1` has eleven), and the table's own `version` and `source`
 * disagree for the Hotfix Pack, which is the source's disagreement to report
 * rather than ART's to resolve.
 */
export interface MediaRow {
  /** 32 lowercase hex characters. */
  md5: string;
  version: string;
  /**
   * **Hatcher's identifier for the disk — not the disk's AmigaDOS volume
   * name, and never to be compared with one.**
   *
   * Measured 2026-09-06 against the owner's own AmigaOS 3.2 media: 0 of 12
   * matched. This says `Backdrops3_2`, `LocaleDE3_2`, `DiskDoctor3_2`; the
   * same disks' own root blocks say `Backdrops3.2`, `Locale-DE`,
   * `DiskDoctor`. Joining them would have put all 35 of the owner's good
   * disks in conflict with the table. Never render this as the disk's name —
   * {@link MediaMatch.volumeName} is that.
   *
   * For an own-table row this is the archive's own single top-level
   * directory (or the ISO's own volume id) instead of a Hatcher identifier —
   * still never the disk's AmigaDOS volume name to compare against, since
   * `core::osinstall::scan::find_packages` already reads that value itself.
   */
  volume: string;
  /** A human-readable label for the disk, as the table names it. */
  name: string;
  /** Where the table says this dump came from. */
  source: string;
  /** The disk's position in its set, when the source states one. */
  sequence: number | null;
  /** What kind of medium this is. `"floppy"` for every adopted row (a
   *  default, not a claim — see {@link MediaKindTag}). */
  kind: MediaKindTag;
  /** The shipped package/recipe id this row's bytes are the media of, when
   *  ART has one — `"boingbag-39-1"`, `"amigaos-39-cd"`. `null` for every
   *  adopted row: Hatcher's data names no ART id, and inventing one on its
   *  behalf would be a claim this table does not get to make. */
  artefact: string | null;
  /** Names ART has actually seen this artefact ship under. A hint for a
   *  drop-folder guide, never a requirement — a file can be renamed and
   *  still hash to this row. Empty for every adopted row. */
  filenames: string[];
  /** Which of the two compiled-in files this row came from — the fact a
   *  match's provenance sentence names (see {@link mediaIdentityLines}). */
  tableOrigin: MediaTableOrigin;
}

/**
 * What one file in a media folder turned out to be. Mirrors
 * `core::osinstall::mediahash::MediaMatch`.
 *
 * The two middle fields come from two different sources on purpose and are
 * never reconciled: `volumeName` is the disk's own answer, `row` is the
 * table's.
 */
export interface MediaMatch {
  path: string;
  /**
   * What the **disk** says it is called, off its own root block. `null` when
   * this file is not something ART can open as media at all (an `.lha`, or a
   * damaged image) — which does not stop it being hashed and looked up.
   */
  volumeName: string | null;
  /**
   * What the **table** says about these bytes, or `null` when no row claims
   * them.
   *
   * `null` is **"not in the table"**, which is a claim about the table and
   * not about the disk: 151 of the 186 rows are themselves unconfirmed, and
   * a re-imaged disk is a legitimate miss. It must never read as "not
   * genuine", and it must never weaken what `volumeName` already said.
   */
  row: MediaRow | null;
  /** The key the lookup was made with, 32 lowercase hex characters. */
  md5: string;
  /**
   * The check that confirmed {@link row} against a real disk, when one has.
   * Mirrors `core::osinstall::mediahash::Confirmation`.
   *
   * `null` beside a non-null `row` is **"matched a row nobody has ever
   * checked against a real disk"** — 151 of the table's 186 rows, and a
   * weaker sentence than a confirmed match. Collapsing the two would hand
   * those 151 a confidence nobody earned. Always `null` when `row` is
   * `null`: a confirmation is about a row, and there is no row.
   */
  confirmed: MediaConfirmation | null;
}

/**
 * A check somebody really ran, and what they ran it against. Mirrors
 * `core::osinstall::mediahash::Confirmation`.
 *
 * Both fields are rendered verbatim, because a confirmation that cannot say
 * what confirmed it is a badge rather than a citation.
 */
export interface MediaConfirmation {
  /** ISO date the check was run. */
  checked: string;
  /** What it was run against, in the words the screen shows. */
  against: string;
}

/**
 * What one pass over a media folder found. Mirrors
 * `core::osinstall::mediahash::Identification`, flattened beside the job id.
 *
 * `unreadable` and the two counts are here so the screen can keep four
 * endings distinct rather than collapsing them: matched, not in the table,
 * could not be read, and not hashed yet are four different sentences with
 * four different next steps (§89).
 */
export interface MediaIdentification {
  /** One entry per candidate file that could be hashed, in path order. */
  matches: MediaMatch[];
  /** Candidates whose bytes could not be read at all — reported, never
   *  silently dropped: a file missing from `matches` reads as a file that is
   *  not in the folder. */
  unreadable: string[];
  /** How many files this pass actually read and hashed. */
  hashed: number;
  /** How many were answered out of ART's scan cache without being read. */
  remembered: number;
  /** Discs ART deliberately did not hash, because their own volume name is
   *  not one any shipped recipe installs from (m7). Reported, never dropped:
   *  a file in none of the lists reads as a file that is not in the folder. */
  skipped: string[];
}

/** The event `osinstall_identify_media`'s own background job answers on. */
export const OSINSTALL_IDENTIFY_MEDIA_EVENT = "osinstall-identify-media-result";

interface OsInstallIdentifyMediaResult extends MediaIdentification {
  job_id: number;
}

/**
 * What every install disk in `folder` turns out to be, by content hash,
 * looked up in the table ART compiles in. Reads the files and writes nothing
 * to them.
 *
 * **Additive to {@link osinstallScanMedia}, never a replacement for it.** A
 * disk's volume name and its table row are two facts from two sources; this
 * adds the second one and takes nothing away from the first.
 *
 * Runs as a background job on the Rust side (§54 — the first pass over the
 * owner's own 3.2 folder reads 31 MB), in its own lane so picking a second
 * folder supersedes the first pass instead of stacking on it. This wrapper
 * hides that behind an ordinary promise the way {@link osinstallCollisions}
 * does, by starting the job and awaiting its own result event.
 *
 * A second call over an unchanged folder hashes nothing: the result is kept
 * against the same `(path, size, mtime)` identity ART's scan cache already
 * uses, and `remembered` says how many answers came from there.
 */
export async function osinstallIdentifyMedia(folder: string): Promise<MediaIdentification> {
  if (!folder) return { matches: [], unreadable: [], hashed: 0, remembered: 0, skipped: [] };
  // `awaitJobResult` subscribes before it calls `start` — see its own doc
  // comment: a pass answered entirely from the cache can finish before the
  // frontend has even learnt its job id.
  return awaitJobResult<OsInstallIdentifyMediaResult, MediaIdentification>(
    OSINSTALL_IDENTIFY_MEDIA_EVENT,
    () => invoke<number>("osinstall_identify_media", { folder }),
    ({ matches, unreadable, hashed, remembered, skipped }) => ({
      matches,
      unreadable,
      hashed,
      remembered,
      skipped,
    })
  );
}

// ---------------------------------------------------------------------------
// The sentences a hash result is allowed to produce (design §4.3)
// ---------------------------------------------------------------------------
//
// **This is the round's whole risk surface.** Everything under it either
// matched or did not; these functions are where that becomes something a
// person reads and acts on, and this project's most expensive defects are
// exactly the confident wrong sentence. Four rules, all of them from §4.3 and
// none of them negotiable by a later edit:
//
// 1. **A match is a claim about the row, not about the disk.** ART says
//    *"this file matches the row Emu68 Hatcher's table calls X"* — never
//    *"this is X"*. That sentence stays true even if the row is wrong,
//    because it reports what was checked and where the claim came from. The
//    attribution is inside the sentence, not a footnote under the list.
// 2. **A confirmed row and an unconfirmed one are two sentences.** 35 of the
//    186 rows have been hashed off a real disk (`media_hashes_confirmed.json`,
//    generated by `scripts/media-table-check.py --emit-confirmed`); 151 have
//    not. Showing all 186 with one voice would give 151 rows a confidence
//    nobody earned.
// 3. **A miss is not an accusation, and takes nothing away.** Any
//    modification, re-imaging or different revision breaks the hash. "Not in
//    the table" is a fact about the table. It never reads as "not genuine",
//    and it never weakens or removes what the disk's own volume name already
//    established — which is a *different* line on the screen, from a
//    different source, and it stays exactly as it was.
// 4. **"Could not be read" is not "not in the table", and "not hashed yet"
//    is neither.** Five endings, five next steps: fix the file, nothing,
//    wait, nothing, and choose a folder. Collapsing any pair of them is the
//    §89 defect.
//
// **The disk's own name is deliberately absent from every sentence below.**
// A row's `volume` is not the disk's volume name — measured 0 of 12 matching
// — so the two are shown as two facts from two sources: the existing
// `osinstall.media.found` line says what the disks call themselves, and these
// lines say what the table makes of their bytes. Nothing here joins them and
// nothing here reconciles them.

/**
 * What happened to **one** media folder in a pass.
 *
 * Four results and never three, for the reason `core/hostfs.rs` states about
 * recycling files: an operation that runs per entry and cannot be undone as a
 * whole is **reported per entry, by name and by result**, because "the pass
 * did not finish" says nothing about which entries did. A pass over three
 * folders that dies on the second one produced one of each of the first three
 * results below, and a screen that showed only the failure would be claiming
 * ART did not do something it did.
 */
export type MediaFolderResult =
  /** Its files were hashed and are in the list. */
  | "identified"
  /** ART asked and the pass failed on this folder. */
  | "unreadable"
  /** The user pressed Stop while this folder was being read. Not the same as
   *  `unreadable`: nothing is wrong with the folder and the next step is to
   *  scan again, not to fix anything. */
  | "stopped"
  /** The pass ended before this folder's turn, so it was never opened. Not
   *  the same as either failure above: nothing is known about it at all. */
  | "not-reached";

/** One folder, by name, and what became of it. */
export type MediaFolderOutcome = { folder: string; result: MediaFolderResult };

/** What ART currently knows about a folder's contents by content hash. */
export type MediaIdentityState =
  /** No folder, or nothing has been asked yet. */
  | { kind: "not-asked" }
  /** A pass is running right now. Distinct from `not-asked`: the next step is
   *  to wait, not to do something. */
  | { kind: "identifying" }
  /** The pass stopped on a folder ART could not read. Distinct from an empty
   *  result, which would read as "ART looked and found nothing" — and it
   *  carries whatever earlier folders **did** identify, because throwing that
   *  away and reporting "the pass failed" would be ART denying work it had
   *  already finished. */
  | { kind: "failed"; identification: MediaIdentification; folders: MediaFolderOutcome[] }
  /** The user pressed Stop. **Not a failure**, and it must never render as
   *  one: the next step is "scan again", not "something is wrong with your
   *  media". Its own `kind` rather than a second message on `failed`, so the
   *  next edit cannot quietly collapse the two back into one ending. */
  | { kind: "cancelled"; identification: MediaIdentification; folders: MediaFolderOutcome[] }
  | { kind: "identified"; identification: MediaIdentification };

/** One file, and the one sentence that is true about it. */
export type MediaIdentityLine = {
  /**
   * Which of the **five** per-file endings this is. Kept on the object so a
   * screen can style them differently without re-deriving which is which
   * from the phrase key.
   *
   * `"skipped"` joined the four in round 3's whole-branch fix (m7): a disc
   * whose own volume name is not one any shipped recipe installs from is
   * never hashed at all. It is emphatically **not** `"not-in-table"` — that
   * one means ART read the disc and the table did not know the hash, and this
   * one means ART never asked, deliberately, because the file is not install
   * media. Nor is it `"unreadable"`, which is a problem; this is not.
   */
  kind: "confirmed" | "unconfirmed" | "not-in-table" | "unreadable" | "skipped";
  /** The file's own name, which is what the user recognises it by. */
  file: string;
  /** Its full path, for a `key` and a tooltip. */
  path: string;
  phrase: Phrase;
};

/** The last path segment, on either separator. Media folders are chosen by
 *  the user and carry Windows paths, but a test fixture and a future CLI
 *  shell carry POSIX ones.
 *
 *  Exported since `slots.ts` needed the same answer: two copies of this would
 *  be two ways of naming the same file on two rows of the same screen. */
export function fileName(path: string): string {
  const parts = path.split(/[\\/]/);
  return parts[parts.length - 1] || path;
}

/**
 * One line per file, in path order, each carrying the single sentence that is
 * true about it.
 *
 * Unreadable files are folded into the same list rather than kept in a
 * footnote, because a file that appears nowhere reads as a file that is not
 * in the folder — but they keep their own `kind` and their own key, so
 * "ART could not read this" can never be mistaken for "the table does not
 * know this".
 *
 * **A pass that failed or was stopped still lists what it did identify.** The
 * per-file endings are per file; a folder ART could not open says nothing
 * about the thirty-five disks it already read in the folder before it. What
 * changes for those two states is the summary below, which says the pass did
 * not finish and names the folders it did not cover — never this list, which
 * would then be claiming ART had not done work it had.
 */
export function mediaIdentityLines(state: MediaIdentityState): MediaIdentityLine[] {
  if (state.kind === "not-asked" || state.kind === "identifying") return [];
  const { matches, unreadable, skipped } = state.identification;

  const matched: MediaIdentityLine[] = matches.map((found) => {
    const file = fileName(found.path);
    if (!found.row) {
      return {
        kind: "not-in-table",
        file,
        path: found.path,
        phrase: { key: "osinstall.mediaId.notInTable", params: { file } },
      };
    }
    // The row's own three fields, as the table states them — never
    // re-derived, and never resolved when they disagree (the Hotfix Pack's
    // `version` and `source` do).
    const row = { name: found.row.name, version: found.row.version, source: found.row.source };
    // Which table answered gets its own sentence (design's §3.6) — the
    // existing "confirmed"/"unconfirmed" phrases name Emu68 Hatcher's table
    // by name and have no slot for a different one, so an own-table match
    // takes its own key rather than a wrong attribution.
    const own = found.row.tableOrigin === "own";
    if (found.confirmed) {
      return {
        kind: "confirmed",
        file,
        path: found.path,
        phrase: {
          key: own ? "osinstall.mediaId.confirmedOwn" : "osinstall.mediaId.confirmed",
          params: {
            file,
            ...row,
            checked: found.confirmed.checked,
            against: found.confirmed.against,
          },
        },
      };
    }
    return {
      kind: "unconfirmed",
      file,
      path: found.path,
      phrase: {
        key: own ? "osinstall.mediaId.unconfirmedOwn" : "osinstall.mediaId.unconfirmed",
        params: { file, ...row },
      },
    };
  });

  const unread: MediaIdentityLine[] = unreadable.map((path) => ({
    kind: "unreadable",
    file: fileName(path),
    path,
    phrase: { key: "osinstall.mediaId.unreadable", params: { file: fileName(path) } },
  }));

  // The fifth ending (m7). In the same list rather than a footnote, for the
  // reason `unreadable` is: a file that appears nowhere reads as a file that
  // is not in the folder — and somebody who put a game disc in their material
  // folder should be told ART left it alone, not left to wonder.
  const untouched: MediaIdentityLine[] = skipped.map((path) => ({
    kind: "skipped",
    file: fileName(path),
    path,
    phrase: { key: "osinstall.mediaId.skipped", params: { file: fileName(path) } },
  }));

  return [...matched, ...unread, ...untouched].sort((a, b) => a.path.localeCompare(b.path));
}

/**
 * The one line about the pass itself — what ART did, not what it found.
 *
 * `null` when there is nothing to say about *how the pass answered* —
 * `hashed + remembered === 0`, which is two different folders in practice:
 * one with no `.adf`/`.iso`/`.lha` in it at all (`osinstall.media.empty`
 * already owns that sentence, and a second one counting the same zero would
 * be this screen contradicting itself), and one whose every candidate came
 * back `unreadable` (that sentence is owned per file, above, in
 * {@link mediaIdentityLines} — nothing here needs to repeat it, and nothing
 * untrue is said by staying silent). Both share the one condition that
 * matters to this function: there were zero hashes to report where they came
 * from.
 *
 * **The identified case states where its answer came from, and that is
 * deliberate.** The hash is cached against `(path, size, mtime)` — the same
 * identity `scan_cache` keys a listing on — so a restored backup that keeps
 * its timestamps is answered, with complete confidence, out of the previous
 * file's hash. A stale listing is a stale list of files; a stale *hash* is a
 * wrong **name** for a disk, which is worse. So the line says how many
 * answers were read now and how many were remembered, and names the escape
 * hatch (ART-194's "Re-scan media" button, which drops the hash as well as
 * the listing) instead of leaving the user to discover the possibility.
 */
export function mediaIdentitySummary(state: MediaIdentityState): Phrase | null {
  switch (state.kind) {
    case "not-asked":
      return { key: "osinstall.mediaId.notHashedYet" };
    case "identifying":
      return { key: "osinstall.mediaId.identifying" };
    case "failed": {
      const { done, total } = foldersDone(state.folders);
      // The folder it died on, by name — a refusal that cannot say *which*
      // folder is one the user cannot act on.
      const folder = state.folders.find((f) => f.result === "unreadable")?.folder ?? "";
      return { key: "osinstall.mediaId.failed", params: { folder, identified: done, total } };
    }
    case "cancelled": {
      const { done, total } = foldersDone(state.folders);
      return { key: "osinstall.mediaId.cancelled", params: { identified: done, total } };
    }
    case "identified": {
      const { hashed, remembered } = state.identification;
      if (hashed + remembered === 0) return null;
      return { key: "osinstall.mediaId.provenance", params: { hashed, remembered } };
    }
  }
}

/** How many of the pass's folders got all the way through, and how many there
 *  were. Counted off the outcomes rather than tracked separately, so the
 *  number in the sentence and the list of folders below it cannot disagree. */
function foldersDone(folders: MediaFolderOutcome[]): { done: number; total: number } {
  return {
    done: folders.filter((f) => f.result === "identified").length,
    total: folders.length,
  };
}

/** One folder, and the one sentence that is true about it. */
export type MediaFolderLine = {
  folder: string;
  result: MediaFolderResult;
  phrase: Phrase;
};

/**
 * One line per **folder**, for a pass that did not finish.
 *
 * `core/hostfs.rs`'s rule, applied one layer up: the pass walks the folders
 * one at a time and a folder already hashed cannot be un-hashed by a later
 * one failing, so the outcome is reported per entry, by name and by result.
 * The four results are four different sentences with four different next
 * steps — fix the folder, scan again, scan again, and nothing — and
 * collapsing any pair of them is the §89 defect this round is about.
 *
 * Empty for the three states where there is nothing per-folder to say: a pass
 * that has not run, one still running, and one that covered every folder (for
 * which the per-file list *is* the report).
 */
export function mediaIdentityFolderLines(state: MediaIdentityState): MediaFolderLine[] {
  if (state.kind !== "failed" && state.kind !== "cancelled") return [];
  return state.folders.map(({ folder, result }) => ({
    folder,
    result,
    phrase: { key: FOLDER_RESULT_KEYS[result], params: { folder } },
  }));
}

/** One key per result, as a total record rather than a `switch`, so a fifth
 *  result cannot compile without a sentence of its own.
 *  `src/i18n/phrase-keys.test.ts` is what proves all four keys exist. */
const FOLDER_RESULT_KEYS: Record<MediaFolderResult, string> = {
  identified: "osinstall.mediaId.folderIdentified",
  unreadable: "osinstall.mediaId.folderUnreadable",
  stopped: "osinstall.mediaId.folderStopped",
  "not-reached": "osinstall.mediaId.folderNotReached",
};

/**
 * Which shipped release these volume names are the install media of, or
 * `null` when they are nobody's or more than one release's (ART-208).
 *
 * Asked only when a folder holds media the chosen release wants none of, so
 * the screen can say "this is your AmigaOS 3.9 folder" instead of listing
 * one absence per component. Names, not a folder: they have already been
 * read, and re-opening thirty-five ADFs to answer a question about names
 * already in hand would be a second pass for nothing.
 */
export async function osinstallReleaseForMedia(volumeNames: string[]): Promise<string | null> {
  return invoke<string | null>("osinstall_release_for_media", { volumeNames });
}

/**
 * What **one** release's own signature made of the volume names in hand —
 * mirrors `core::osinstall::identify::ReleaseEvidence`.
 *
 * Not the same question as {@link osinstallReleaseForMedia}, and the
 * difference is the point: that one asks "which release is this folder", and
 * answers `null` for a folder holding only `Fonts` and `Locale` because those
 * cannot separate 3.1 from 3.2. This asks "how much of *the release being
 * built* is in this folder", and answers `shared: ["Locale", "Fonts"]` for
 * the same folder — media this release does ask for, whatever else also asks
 * for it.
 */
export interface ReleaseEvidence {
  /** The recipe's own release name, echoed back. */
  release: string;
  /** Present, named by this release, and named by no other release ART knows
   *  of. */
  distinguishing: string[];
  /** Present and named by this release, but also carried by another release
   *  — `Fonts`, `Locale`. Reported, never counted as identification. */
  shared: string[];
  /** Media this release marks `required` that the folder does not hold. */
  missingRequired: string[];
}

/**
 * How much of `release`'s own install media `volumeNames` are.
 *
 * **Why the screen needs this** (the 2026-09-06 review's Critical, ART-253):
 * `wrongMediaFolder` claims "none of the disks in this folder are ones this
 * release asks for", and had no way to check it. It inferred the claim from
 * an empty `plan.items` and from whether a *missing* disk was somehow in the
 * folder — neither of which any plan the core can emit ever satisfies
 * differently, so the sentence rendered over a folder holding `Workbench3.2`.
 * The claim is about this release's media, so it is answered from this
 * release's recipe.
 *
 * Throws for a release ART ships no recipe for. An empty evidence *means*
 * "this folder holds none of it", so answering that for an unknown release
 * would manufacture the very sentence this exists to make honest.
 */
export async function osinstallMediaEvidence(
  release: string,
  volumeNames: string[]
): Promise<ReleaseEvidence> {
  return invoke<ReleaseEvidence>("osinstall_media_evidence", { release, volumeNames });
}

/**
 * One media folder the chosen release asks for, in the recipe's own order.
 *
 * Mirrors `core::osinstall::MediaLayer` — `labelKey` is an **i18n key**, not
 * a sentence, resolved the same way `ComponentDef.labelKey` already is (see
 * `OsInstall.tsx`'s own `label()`): a recipe is data with no compiler between
 * it and the screen, so the key travels and the translation happens at the
 * place that draws it.
 */
export interface InstallLayer {
  id: string;
  labelKey: string | null;
}

/**
 * The media layers `release`'s own shipped recipe declares, in the recipe's
 * own order.
 *
 * **An empty array means unlayered** (every shipped recipe except AmigaOS
 * 3.2.2) — the media step reads that as "render the single folder field this
 * screen has always rendered", never as an error. Read-only: parses shipped
 * JSON, opens no media.
 */
export async function layersFor(release: InstallRelease): Promise<InstallLayer[]> {
  return invoke<InstallLayer[]>("osinstall_layers", { release });
}

/**
 * Which of `release`'s own layers `volumeNames` look like — **never** which
 * release, that is `osinstallReleaseForMedia`'s own job. Asked once a
 * layer's own field holds media, so the screen can say "this folder holds
 * your update disks, not the base set" against that field itself, instead of
 * letting the plan's own `MediaMissing` refusals name disks the user does own
 * (Task 10 fix round, Finding 1 — `wrongMediaFolder` and `osinstallScanMedia`
 * both answer for the flat, unlayered field only, and cannot see the mistake
 * a two-field screen invites: the update disks pointed at the base field, or
 * the reverse, which is still "AmigaOS 3.2.2 media" at the release level).
 *
 * `null` for an unlayered release (nothing to tell apart) and for a folder
 * whose names do not distinguish one layer from another.
 */
export async function layerForMedia(
  release: InstallRelease,
  volumeNames: string[]
): Promise<string | null> {
  return invoke<string | null>("osinstall_layer_for_media", { release, volumeNames });
}

/** What installing the chosen components would do — or every reason it
 *  cannot. Writes nothing (§92's PREVIEW). */
export async function osinstallPlan(request: InstallRequest): Promise<PlanResult> {
  return invoke<PlanResult>("osinstall_plan", { request });
}

/**
 * Forget every medium listing ART is holding, so the next preview reads the
 * discs again. Answers how many were dropped.
 *
 * **The escape hatch, not a convenience** (ART-194). The cache is keyed on
 * `(path, size, mtime)`, and a restored backup can keep its timestamps —
 * several AmigaOS 3.9 ISOs circulate and people keep their own copies, so
 * "same path, same size, same mtime, different disc" is real. Against that the
 * cache answers with complete confidence and is wrong, which is the worst
 * shape a defect takes here: it does not crash, it says something untrue.
 *
 * Removes only ART's own derived files under `%TEMP%`; nothing of the user's.
 */
export async function osinstallRescanMedia(): Promise<number> {
  return invoke<number>("osinstall_rescan_media");
}

/**
 * Build the distribution tree. Returns a job id (§54).
 *
 * Takes the plan exactly as the screen was shown it — see the module note
 * above for why this does not recompute the way `preloadRun` does.
 */
export async function osinstallApply(plan: InstallPlan, destination: string): Promise<number> {
  return invoke<number>("osinstall_apply", { request: { plan, destination } });
}

/** Subscribe to finished installs. A cancelled or failed job never sends
 *  one — the job bar is where those are seen. */
export async function onOsInstallResult(
  handler: (result: OsInstallResult) => void
): Promise<UnlistenFn> {
  return listen<OsInstallResult>(OSINSTALL_EVENT, (event) => handler(event.payload));
}

/**
 * Read the volume back and check it against the manifest `osinstallApply`
 * wrote (§92's VERIFY step). `distRoot` is the distribution tree's own
 * root — where `distribution.json` was written — not the manifest file
 * itself.
 */
export async function osinstallVerify(
  image: string,
  slot: number | null,
  index: number,
  distRoot: string
): Promise<VerifyReport> {
  return invoke<VerifyReport>("osinstall_verify", {
    request: { image, slot, index, distRoot },
  });
}

// ---------------------------------------------------------------------------
// Update packages — landing an official (or unofficial) package on a
// distribution tree that already exists (Task 7). `Collision`/`CollisionReport`
// mirror `core::osinstall::collide` exactly, including the `kind` tag —
// `Collision::SameVersion`/`::Unversioned` carry an explicit
// `#[serde(rename)]` on their byte counts (`collide.rs`'s own doc comment
// explains why `rename_all = "kebab-case"` alone would not have camelCased
// them), so `fromBytes`/`toBytes` here really is what crosses the wire.
//
// ## Fix round 1 (Task 7's own review)
//
// **F2 — refusals are data, not debug text.** `osinstallAddPackage` used to
// return a bare job id; a refused selection reached the screen as Rust's
// own `{:?}` output. It now returns [`AddPackageResult`], and a `Refused`
// answer carries the same six-variant `RefusalReason` the install screen's
// own refusals already render through [`refusalPhrase`].
//
// **F4 — the preview runs off the command thread.** `osinstall_collisions`
// now starts a job and answers with its id; [`osinstallCollisions`] hides
// that behind its own unchanged `Promise<CollisionReport[]>` shape by
// awaiting [`OSINSTALL_COLLISIONS_EVENT`] through [`awaitJobResult`] — no
// caller of this function had to change.
//
// ## Fix round 2 (N1, the re-review)
//
// `awaitJobResult` used to take a `jobId: number` directly, which meant
// `osinstallCollisions` had to `await invoke(...)` *before* calling it — a
// real race: a fast job (a cache hit especially) can finish and emit its
// result event while that `await` is still in flight, strictly before
// `awaitJobResult` had a job id to filter on at all, losing the event
// forever with no timeout. `awaitJobResult` now takes `start`, a function
// that performs the `invoke` itself, and calls it only after its own
// listeners are already registered — see its own doc comment in
// `@/lib/jobs.ts` for the buffering that lets a job id learned *after* an
// event still match it.
// ---------------------------------------------------------------------------

/** What landing one incoming file over an existing one would actually do —
 *  mirrors `core::osinstall::collide::Collision`. `Identical` never reaches
 *  this type: the core excludes it before a `CollisionReport` is ever built
 *  (see `preview`'s own doc comment), so there is nothing to render it as. */
export type Collision =
  | { kind: "upgrade"; from: string; to: string }
  | { kind: "downgrade"; from: string; to: string }
  | { kind: "same-version"; version: string; fromBytes: number; toBytes: number }
  | { kind: "unversioned"; fromBytes: number; toBytes: number };

/** One planned item that would land on something already in the tree —
 *  mirrors `core::osinstall::collide::CollisionReport`. */
export interface CollisionReport {
  /** Where in the tree, `/`-separated. */
  path: string;
  collision: Collision;
  /** Whether the package's own recipe declared it may write over what is
   *  there (its `overrides`). An `undeclared` row is not necessarily wrong —
   *  it just was not promised, and the panel says so beside the row rather
   *  than silently treating every collision alike. */
  declared: boolean;
}

/** One package ART ships a recipe for, in the shape the checklist needs.
 *  Mirrors `commands::osinstall::PackageSummary`. */
export interface PackageSummary {
  id: string;
  /** Shown on screen, unlocalized — a package's own name (ART-060). */
  name: string;
  /** Other **packages** this one needs applied first (`BoingBag 3.9-2`
   *  needs `BoingBag 3.9-1`) — dependency order, not the order ticked. */
  requires: string[];
  /** Recipe **components** this package needs switched on on the tree it
   *  lands on (`locale-turkish` needs `locale-base`, ART-162) — a different
   *  relationship from `requires`: this names a component, never a package. */
  requiresComponents: string[];
  /** Whether an archive carrying this package's own top-level directory
   *  name was actually found in the package folder. `false` is not "ART
   *  cannot install this" — every shipped package always can — it is "the
   *  file is not here yet", and a checkbox for a package whose file is
   *  absent is a promise ART cannot keep. */
  available: boolean;
  /** `null` for the ordinary package. Non-null means ART cannot place this
   *  package's files from the host **at all** — not "your archive is
   *  missing", which is what `available: false` says. A row with a block
   *  must not be tickable: the user has to learn this before committing to
   *  it, not after (M3, ART-166). */
  hostPlacementBlock: HostPlacementBlock | null;
  /** Whether this package's own recipe declares an installer ART can run
   *  **on the Amiga** — the other half of `hostPlacementBlock`. A BoingBag
   *  cannot be placed from Windows and *can* be run inside an emulator, and
   *  `AmigaInstallPanel` offers exactly the packages this is true of, read
   *  from the recipe rather than from a list of ids written here. */
  amigaInstallable: boolean;
  /**
   * `null` for a package ART has actually run. Non-null is the recipe's own
   * English sentence saying **what has not been measured yet** — the row is
   * rendered *disabled with that sentence*, never hidden.
   *
   * Its own field rather than `amigaInstallable: false`, because the two are
   * different sentences with different next steps: "ART ships no Amiga-side
   * installer for this" is a fact about the recipe, and "nobody has run this
   * one unattended yet" is a fact about what has been measured. §10 asks for
   * the unready action to be registered, not hidden.
   *
   * A **value**, not prose (fix round 1, m6): it shipped as free English
   * recipe text interpolated into a translated frame.
   */
  notYetRunnable: NotYetRunnable | null;
  /** Every entry name this package's own archive carries that `safe_join`
   *  refused — a `..`, an absolute path, a Windows prefix — exactly as the
   *  archive spelled it, and `[]` for the ordinary archive. Shown beside
   *  the row: a package holding `..\..\Startup` is a fact worth seeing
   *  before a confirmation, and until now it was collected and shown
   *  nowhere (m6). */
  refusedNames: string[];
}

/** Every package ART ships a recipe for, paired with whether its own archive
 *  was actually found in `packageFolder` — never assumed from the shipped
 *  list alone. Read-only; an unreadable folder answers the same way an empty
 *  one would (every package `available: false`) rather than refusing, so the
 *  checklist itself always renders. */
export async function osinstallPackages(
  packageFolder: string,
  release: InstallRelease
): Promise<PackageSummary[]> {
  return invoke<PackageSummary[]>("osinstall_packages", { packageFolder, release });
}

// ---------------------------------------------------------------------------
// Slots — what a release needs, resolved once (design §3.2)
// ---------------------------------------------------------------------------
//
// Mirrors `core::osinstall::slots` exactly. The sentences these types are
// allowed to produce live in `src/lib/slots.ts`, not here and not in a
// component: a slot is data, and what is said about it is a `Phrase`.

/** What kind of artefact a slot wants. Mirrors `slots::SlotKind`. */
export type SlotKind = "medium" | "package" | "overlay" | "rom";

/**
 * How ART came to believe a file fills a slot. Mirrors `slots::MatchedBy`,
 * **ranked** — see that module's doc comment. The order matters on screen as
 * much as in Rust:
 *
 * - `hash` is a fact about the bytes;
 * - `volume-name` / `top-level-directory` is what the medium says about
 *   itself, which a rename cannot break;
 * - `filename` is a **guess** and never reaches {@link SlotState.found} — a
 *   file bearing the expected name arrives in {@link SlotState.candidates}
 *   instead, so the row can say ART could not confirm it;
 * - `chosen` is a file the user named by hand. Not an identification ART
 *   made, and it must never be rendered as one.
 */
export type MatchedBy = "hash" | "volume-name" | "top-level-directory" | "filename" | "chosen";

/** One artefact a release's build can use. Mirrors `slots::Slot`. */
export interface Slot {
  /** `medium:AmigaOS3.9`, `package:boingbag-39-1`,
   *  `overlay:boingbag-39-1:BoingBag3.9-1-UAE`, `rom`. What
   *  {@link Slot.requires} and {@link SlotState.blockedBy} name. */
  id: string;
  kind: SlotKind;
  /** What the recipe calls it — a package's own name (ART-060), a medium's
   *  volume name, an overlay's own drawer. Never translated. */
  name: string;
  /** The name the artefact gives for **itself**. A `rom` slot has none, so it
   *  carries the Kickstart major the recipe states it needs (`"40"`), or `""`
   *  when the recipe states no floor. */
  identity: string;
  /** The artefact id the media rows are looked up by, or `null` when neither
   *  table names these bytes — which means the hash rank cannot fire, never
   *  that the artefact is unknown or unwanted. */
  artefact: string | null;
  /** A medium a required component reads from, or a ROM the release states a
   *  floor for. A package is a choice, so never required. */
  required: boolean;
  /** Names ART has seen this artefact ship under. A hint for the guide text
   *  and for the *guess* rank — never a requirement. */
  filenames: string[];
  /** The media row's own `source`, verbatim. `null` when no row names it. */
  provenance: string | null;
  /** Chain order: media, then packages requires-first with each overlay right
   *  after its package, then the ROM. Sort the readout by this. */
  position: number;
  requires: string[];
  /** Reserved for round 3; empty today. */
  supersededBy: string[];
  /** Directories a disc filling this slot must carry at its root — design
   *  § 3.6's structural check, as data on the artefact map. Empty for
   *  everything ART records no expectation for. */
  expectsDirectories: string[];
}

/**
 * What is known about one file's **bytes**. Mirrors `slots::BytesRead`.
 *
 * Three answers, because two of them were one for a round (fix round 1's F1,
 * then the re-review's F13):
 *
 * - `not-read` — nobody has hashed this file. `osinstallSlots` asks the scan
 *   cache and hashes nothing, so this is the state of every file in a folder
 *   `osinstallIdentifyMedia`'s job has not run over. Saying "its bytes are in
 *   no table ART has" here reports a lookup nobody made.
 * - `read-no-row` — hashed, and no row in either table claims those bytes.
 *   The one answer that may say "in no table ART has".
 * - `read-row` — hashed, and a row **does** claim them. At rank 1 that row is
 *   the slot's own artefact; at ranks 2 and 3 it is by construction a
 *   different one — a relabelled or mislabelled disk — and the row's own
 *   `name` is what lets the readout say which artefact the bytes actually
 *   are instead of claiming the table does not know them.
 */
export type BytesRead =
  | { state: "not-read" }
  | { state: "read-no-row" }
  | { state: "read-row"; artefact: string | null; name: string };

/** The file that fills a slot, and how ART knows. Mirrors `slots::Found`. */
export interface SlotFound {
  path: string;
  matchedBy: MatchedBy;
  /** The table row — **only** for `matchedBy === "hash"`, which is the rank a
   *  row is the evidence for. A file matched by the name it gives for itself
   *  may be in the table under some other artefact, and showing that row here
   *  would present one artefact's provenance as another's. */
  row: MediaRow | null;
  confirmed: MediaConfirmation | null;
  /** What the table lookup for these bytes came back with — see
   *  {@link BytesRead}. */
  bytesRead: BytesRead;
}

/** One file that might fill a slot. Mirrors `slots::Candidate`. */
export interface SlotCandidate {
  path: string;
  /** See {@link BytesRead} — a guess nobody has hashed, one that was hashed
   *  and matched nothing, and one whose bytes are some *other* catalogued
   *  artefact are three different rows. */
  bytesRead: BytesRead;
}

/**
 * Whether the artefact is already part of the tree, **as the tree's own
 * `distribution.json` states it**. Mirrors `slots::Installed`.
 *
 * A file being in a folder is never `"placed"` or `"ran"` — that is the whole
 * reason this comes from the manifest and not from the scan.
 */
export type Installed =
  | { state: "placed"; at: string | null }
  | { state: "ran"; command: string }
  | { state: "no" };

/** One slot, resolved. Mirrors `slots::SlotState`. */
export interface SlotState {
  slot: Slot;
  /** The one file ART will say fills this slot, or `null` whenever it could
   *  not decide — nothing matched, several did, or the only evidence was a
   *  file name. */
  found: SlotFound | null;
  /** Everything that might fill it, in sorted path order. Empty when `found`
   *  is set. */
  candidates: SlotCandidate[];
  installed: Installed;
  /** The path the user chose for this slot when **that file is not there** —
   *  a remembered ROM on a drive nobody plugged in. Its own ending: not
   *  *chosen* (a sentence about a file that is not there) and not *not found*
   *  (which would say nothing about the choice already made). `found` is
   *  `null` alongside it, so it is not counted. */
  chosenMissing: string | null;
  /** Every requirement that is not installed yet, by slot id. */
  blockedBy: string[];
  /**
   * The first directory {@link Slot.expectsDirectories} names that the disc
   * filling this slot does **not** carry (design § 3.6).
   *
   * *Matched but incomplete* is a different sentence from *not found*: a
   * disc ART recognises whose root is missing `Emergency-Boot` is a
   * re-master or a partial copy, and telling somebody it is absent would
   * send them looking for a file sitting right there. `null` when ART has
   * nothing to check or everything expected is there.
   */
  incomplete: string | null;
}

/**
 * How much of this set is here. Mirrors `slots::SetSummary`.
 *
 * **Required and optional are counted apart, and must be shown apart.** A set
 * missing only optional files is ready to build, and one fraction cannot say
 * that. A slot ART measured as not needed is in neither total.
 */
export interface SetSummary {
  release: string;
  requiredTotal: number;
  requiredFound: number;
  optionalTotal: number;
  optionalFound: number;
}

/** What `osinstallSlots` answers. Mirrors `commands::osinstall::SlotReport`. */
export interface SlotReport {
  states: SlotState[];
  summary: SetSummary;
  /** Material folders ART could not read at all, as the user spelled them.
   *  Empty is the normal answer, and it is a different sentence from "found
   *  nothing": a remembered path on a drive nobody plugged in must not make
   *  the disks in the folder beside it read as missing. */
  unreadableFolders: string[];
  /** Folders holding more archives than ART opens in one pass, as
   *  `[folder, bound]` — design § 6's "bound the count and **name the
   *  bound**". An artefact ART never reached must not read as one that is
   *  not there, so the readout says so and quotes the number. */
  crowdedFolders: [string, number][];
}

/**
 * Resolve everything `release` can use against the material folders, the
 * chosen tree and the chosen ROM — the one answer that replaces the four
 * separate resolutions the OS Builder used to make.
 *
 * Read-only, and it hashes nothing: hash answers come out of the scan cache
 * that `osinstallIdentifyMedia`'s own job fills, so a folder nobody has
 * identified yet simply resolves by what its files call themselves. Call this
 * again after that job finishes to pick the stronger rank up.
 *
 * An unreadable material folder is answered rather than refused (the readout
 * always renders); a chosen `tree` that carries no `distribution.json` is a
 * refusal, because the user just pointed at it.
 */
export async function osinstallSlots(
  release: InstallRelease,
  folders: string[],
  tree?: string | null,
  rom?: string | null,
  /**
   * The files the user picked by hand, as `[slot id, path]` — design § 3.4's
   * *"a file the user picked by hand wins over the slot, and the readout says
   * chosen by you for it"*.
   *
   * An override outranks every rank ART has: it is not an identification ART
   * made and cannot be compared with one. Without them the readout said *"not
   * in the folders you named"* about a file the run was going to use, while
   * the Amiga-side panel said the opposite — two screens, one artefact.
   *
   * An overlay's entry may name `overlay:<packageId>` rather than a
   * particular drawer; the screen holding these has one *"update archive"*
   * field per package.
   */
  overrides?: SlotOverride[]
): Promise<SlotReport> {
  return invoke<SlotReport>("osinstall_slots", {
    release,
    folders,
    tree: tree || null,
    rom: rom || null,
    overrides: overrides && overrides.length > 0 ? overrides : null,
  });
}

/** One file the user picked by hand, as `osinstall_slots` takes it. */
export type SlotOverride = [slot: string, path: string];

// ---------------------------------------------------------------------------
// The chain — one row per link of the material's own order
// ---------------------------------------------------------------------------

/**
 * What one link of the AmigaOS 3.9 update chain is. Mirrors
 * `chain::ChainState`.
 *
 * **Seven states, and they never collapse into "not done".** Installed,
 * ready, waiting for something else, the file is not here, the material
 * itself makes it redundant, ART will not do it, and nobody has measured it
 * yet are seven different next steps. Folding any two of them would tell
 * somebody to go and find a file they already have, or to wait for something
 * that has already happened.
 *
 * `when` is always `null` today: `distribution.json` records no date against
 * a component, a medium or a run. The screen renders nothing rather than a
 * guess.
 */
export type ChainState =
  | { state: "installed"; when: string | null }
  | { state: "ready" }
  | { state: "blocked-by"; names: string[] }
  | { state: "blocked-by-component"; components: BlockedComponent[] }
  | { state: "missing"; expected: string[] }
  | { state: "not-needed"; supersededBy: string }
  | { state: "refused"; reason: RefusedBecause }
  | { state: "not-yet-runnable"; reason: NotYetRunnable };

/**
 * A component a package needs and the tree was not built with (ART-162).
 * Mirrors `chain::BlockedComponent`.
 *
 * `labelKey` is an i18n key, not a name: the recipe is data in the Rust tree
 * and the words are the catalogue's (ART-060). `null` for a component the
 * components screen labels by its medium, and the screen then shows `id` —
 * the same thing that screen shows, so the two cannot disagree.
 */
export interface BlockedComponent {
  id: string;
  labelKey: string | null;
}

/** Why a chain row is refused. A value, never a sentence — this file turns it
 *  into one. Mirrors `chain::RefusedBecause`. */
export type RefusedBecause =
  | { because: "ambiguous"; candidates: string[] }
  | { because: "not-placeable"; block: HostPlacementBlock };

/** The facts a row's sentence needs beside its state. Mirrors
 *  `chain::SentenceFacts`. */
export interface SentenceFacts {
  /** The file filling this row's slot, by name alone. `null` when nothing
   *  fills it — including a row that is installed and whose archive has
   *  since left the folder, which is ordinary. */
  file: string | null;
  /** `true` on the Amiga through the package's own installer, `false` placed
   *  from Windows by ART, `null` for a row that is neither — the CD, and a
   *  package ART can neither place nor run. Three states, because "ART
   *  places it" and "ART can do neither" are not the same claim. */
  runsOnAmiga: boolean | null;
}

/** One row of the chain screen. Mirrors `chain::ChainRow`. */
export interface ChainRow {
  /** The rank the material gives this link — the CD is 1, and two rows may
   *  share a rank where the material states no order between them. Rows
   *  arrive sorted by `(position, id)`. */
  position: number;
  /** The package's own id, or `null` for the CD row, which is a medium. */
  packageId: string | null;
  /** The slot feeding this row, when the release has one. */
  slotId: string | null;
  /** The package's own name, or the medium's (ART-060). */
  name: string;
  state: ChainState;
  /** The facts the row's sentence needs beside its state. Named as the brief
   *  named it (fix round 1, m5). */
  sentenceFacts: SentenceFacts;
}

/** How much of the chain is done. Mirrors `chain::ChainSummary`.
 *
 *  A row the material makes redundant is counted apart — never as applied
 *  (which would claim ART did something it did not) and never as outstanding
 *  (which would send somebody after a file whose contents they have). */
export interface ChainSummary {
  release: string;
  total: number;
  installed: number;
  notNeeded: number;
}

/** What `osinstallChain` answers. Mirrors `commands::osinstall::ChainReport`. */
export interface ChainReport {
  rows: ChainRow[];
  summary: ChainSummary;
  /** Material folders ART could not read at all — the same field
   *  {@link SlotReport} carries, and empty is a different sentence from
   *  "found nothing". */
  unreadableFolders: string[];
  /** Folders holding more archives than ART opens in one pass, as
   *  `[folder, bound]`. */
  crowdedFolders: [string, number][];
}

/**
 * The whole chain for `release`, resolved against the same folders, tree and
 * ROM the material readout is resolved against.
 *
 * The fact gathering is `osinstallSlots`' own, in Rust: two questions about
 * one set of files, so the two screens cannot disagree about which archive is
 * which. Read-only, and it hashes nothing.
 *
 * A chosen `tree` with no `distribution.json` is a refusal rather than an
 * empty chain — every *installed* state here comes from that file alone.
 *
 * `overrides` is the user's own per-slot file choices, in the same shape
 * `osinstallSlots` takes them (`slotOverrides` in `@/lib/amigainstall`).
 * **ART-284**: without them the source step's readout said *the file you
 * chose* about the owner's own BoingBag 1 build while this screen called the
 * same slot ambiguous and blocked the row — one resolver, two callers, one of
 * them not handed the decision.
 */
export async function osinstallChain(
  release: InstallRelease,
  folders: string[],
  tree?: string | null,
  rom?: string | null,
  overrides?: SlotOverride[]
): Promise<ChainReport> {
  return invoke<ChainReport>("osinstall_chain", {
    release,
    folders,
    tree: tree || null,
    rom: rom || null,
    overrides: overrides && overrides.length > 0 ? overrides : null,
  });
}

/**
 * What writing the drop-folder guide did. Mirrors
 * `commands::osinstall::GuideOutcome`.
 *
 * **Two endings, and they stay two.** *Written* and *there already* are
 * different next steps — one is done, the other asks the user to delete a
 * file — and a third, a real failure, arrives as a rejection rather than as
 * an outcome. `already-there` is not an error: `SAFE_CREATE` means the file
 * on disk was not touched, which is the guarantee; what to say about it is a
 * sentence, and a sentence has to be translatable (ART-060).
 */
export type GuideOutcome =
  | { state: "written"; path: string }
  | { state: "alreadyThere"; path: string };

/**
 * Write *"what goes in this folder"* into one of the build's material folders
 * (design § 3.7).
 *
 * **Only ever from a click.** ART does not write into a user's folder because
 * they pointed at it; this exists so a person can ask for the list, in the
 * folder where it is useful. The text is composed on the Rust side from the
 * release's own slots, so it cannot drift from what ART actually accepts, and
 * its **filename comes from the same data** — which is why this takes a
 * language and not a name.
 *
 * `language` is the UI's own code (`i18n.language`, without its region);
 * anything Rust has no guide for falls back to English rather than refusing.
 */
export async function osinstallWriteMaterialGuide(
  folder: string,
  release: InstallRelease,
  language: string
): Promise<GuideOutcome> {
  return invoke<GuideOutcome>("osinstall_write_material_guide", {
    folder,
    release,
    language,
  });
}

/** The event `osinstall_collisions`'s own background job answers on. */
export const OSINSTALL_COLLISIONS_EVENT = "osinstall-collisions-result";

interface OsInstallCollisionsResult {
  job_id: number;
  reports: CollisionReport[];
}

/**
 * What landing the chosen packages on `treeRoot` would actually do to the
 * files already there (§3's PREVIEW) — writes nothing. `packages` empty
 * answers `[]` **without calling into Tauri at all** (F5 of Task 7's own fix
 * round — an empty selection, or no package folder chosen yet, is guarded
 * here rather than asked of the backend), matching the panel's own rule
 * that the preview only appears once at least one package is chosen.
 *
 * Runs as a background job on the Rust side (F4 — a real BoingBag extracts
 * on the order of two hundred files, which is too long for the command
 * thread); this wrapper hides that behind the same
 * `Promise<CollisionReport[]>` shape it always had, by starting the job and
 * awaiting its own result event.
 *
 * `overrides` is the user's own per-slot file choices, in the same shape
 * `osinstallSlots` and `osinstallChain` take them (`slotOverrides` in
 * `@/lib/amigainstall`). **ART-288**: without them this path resolved a
 * package's archive by identity alone, so a folder holding two builds of one
 * BoingBag refused here while the chain row above it named the file it would
 * use — two screens, two answers, and the one that refused was the one that
 * does the work.
 */
export async function osinstallCollisions(
  treeRoot: string,
  packageFolder: string,
  packages: string[],
  overrides?: SlotOverride[]
): Promise<CollisionReport[]> {
  if (!treeRoot || !packageFolder || packages.length === 0) return [];
  // `awaitJobResult` itself subscribes before calling `start` (the `invoke`
  // below) — see its own doc comment (N1, Task 7's re-review): a fast job,
  // a cache hit especially, can finish before the frontend even learns its
  // own job id, and subscribing only after `invoke` resolved lost that
  // event outright.
  return awaitJobResult<OsInstallCollisionsResult, CollisionReport[]>(
    OSINSTALL_COLLISIONS_EVENT,
    () =>
      invoke<number>("osinstall_collisions", {
        treeRoot,
        packageFolder,
        packages,
        overrides: overrides && overrides.length > 0 ? overrides : null,
      }),
    (payload) => payload.reports
  );
}

/** The event a finished component preview arrives on. */
export const OSINSTALL_COMPONENT_COLLISIONS_EVENT = "osinstall-component-collisions-result";

/**
 * What previewing one or more switched-on recipe components would do
 * (ART-175). Mirrors `commands::osinstall::ComponentPreview`.
 *
 * `placed` is here because `reports` is, by construction, a list of what
 * *clashes*: AmigaOS 3.9's overlay lands hundreds of files on nothing and
 * replaces a few dozen, so an empty `reports` for a component that places six
 * hundred files means "nothing is in the way", not "nothing happens". The two
 * must not read the same on screen (§89).
 */
export interface ComponentPreview {
  reports: CollisionReport[];
  /** Every non-directory item the chosen components would place. */
  placed: number;
  /**
   * How many of those land on a destination an **earlier** component in the
   * same plan also claims.
   *
   * Without it, "new" is wrong by exactly the number of identical files:
   * `collide::preview` drops `Identical` rows before returning — its own rule,
   * and a good one, since an identical file is nothing to warn about — so
   * `placed - reports.length` counts a file landing byte-for-byte on another
   * component's copy as *new*. On AmigaOS 3.9's overlay that is 130 files.
   *
   * ```text
   * new       = placed - contested        (landed on nothing)
   * unchanged = contested - reports.length (landed on identical bytes)
   * replaced  = reports.length
   * ```
   */
  contested: number;
}

interface OsInstallComponentCollisionsResult extends ComponentPreview {
  job_id: number;
}

/**
 * What switching these recipe components on would replace, file by file
 * (ART-175, §92's PREVIEW).
 *
 * [ART-170] made `collide::preview` able to answer for a release recipe's
 * component — resolving its id against the shipped releases as well as the
 * packages — and nothing asked it. This is the ask. The engine already
 * *refuses* an undeclared overlap at plan time, so nothing was unguarded;
 * what was missing is the informed-consent half, and it is the half that
 * matters most for AmigaOS 3.9, whose `workbench-39` is the component that
 * makes a tree 3.9 rather than 3.5. It is not the only layering component in
 * shipped data, though ART-175's own entry says so: AmigaOS 3.2 has four,
 * `glowicons` layering over four other components at once. The recipe-parity
 * test pins all five.
 *
 * Takes the plan the screen is already showing rather than re-planning:
 * {@link osinstallApply}'s own rule — the user's component choices *are* the
 * plan, and a screen that previewed one thing must not describe another. A
 * job underneath (it reads every file those components would place, off real
 * install media), hidden behind an ordinary promise the same way
 * {@link osinstallCollisions} hides its own.
 */
export async function osinstallComponentCollisions(
  plan: InstallPlan,
  components: string[]
): Promise<ComponentPreview> {
  if (components.length === 0) return { reports: [], placed: 0, contested: 0 };
  return awaitJobResult<OsInstallComponentCollisionsResult, ComponentPreview>(
    OSINSTALL_COMPONENT_COLLISIONS_EVENT,
    () => invoke<number>("osinstall_component_collisions", { plan, components }),
    (payload) => ({
      reports: payload.reports,
      placed: payload.placed,
      contested: payload.contested,
    })
  );
}

/** `osinstall_add_package`'s own answer — either a job started, or every
 *  typed reason it could not (F2). Mirrors `commands::osinstall::AddPackageResult`
 *  exactly, including the `outcome` tag. */
export type AddPackageResult =
  | { outcome: "started"; job_id: number }
  | { outcome: "refused"; refusals: RefusalReason[] };

/**
 * Add every chosen package to a distribution tree that already exists.
 * One job for the whole set, never one per package and never one per file,
 * matching the panel's single confirmation for the whole set. A refused
 * selection never reaches the job system at all — see [`AddPackageResult`].
 * Progress for a started job is the ordinary `job-progress` event, filtered
 * by its id, the same way `osinstallApply`'s own install job is tracked;
 * its outcome (files/directories/bytes actually written) arrives on
 * [`OSINSTALL_ADD_PACKAGE_EVENT`].
 */
export async function osinstallAddPackage(
  treeRoot: string,
  packageFolder: string,
  packages: string[],
  overrides?: SlotOverride[]
): Promise<AddPackageResult> {
  return invoke<AddPackageResult>("osinstall_add_package", {
    treeRoot,
    packageFolder,
    packages,
    overrides: overrides && overrides.length > 0 ? overrides : null,
  });
}

/** The event a finished (or failed) package-add job's own outcome arrives
 *  on (F10 — the counts used to reach nowhere but the oplog). */
export const OSINSTALL_ADD_PACKAGE_EVENT = "osinstall-add-package-result";

export interface OsInstallAddPackageResult {
  job_id: number;
  outcome: ApplyOutcome;
}

export async function onOsInstallAddPackageResult(
  handler: (result: OsInstallAddPackageResult) => void
): Promise<UnlistenFn> {
  return listen<OsInstallAddPackageResult>(OSINSTALL_ADD_PACKAGE_EVENT, (event) =>
    handler(event.payload)
  );
}

/**
 * `reports`, split into the five classes the preview groups by — downgrades
 * first ("a downgrade is what the user most needs to see": spec §3's whole
 * reason for existing is `ModulesA1200_3.2.adf`'s thirteen stale commands),
 * then upgrades, then same-version, then unversioned. `same-version` is
 * never folded into `unversioned`, here or anywhere else — the two mean
 * different things (both sides agree on a version vs. neither side says
 * anything at all). A group with nothing in it is omitted, not shown empty
 * (F1 of Task 7's own fix round — this replaces a flat, unlabelled sort
 * that gave a downgrade the same key, and so the same look, as an upgrade).
 */
export interface CollisionGroup {
  kind: Collision["kind"];
  reports: CollisionReport[];
}

const COLLISION_GROUP_ORDER: Collision["kind"][] = [
  "downgrade",
  "upgrade",
  "same-version",
  "unversioned",
];

export function groupCollisionsForPreview(reports: CollisionReport[]): CollisionGroup[] {
  return COLLISION_GROUP_ORDER.map((kind) => ({
    kind,
    reports: reports.filter((r) => r.collision.kind === kind),
  })).filter((group) => group.reports.length > 0);
}

/** The heading label for one group — its own translation key, not a value
 *  interpolated into a shared one, so "Downgrades" and "Upgrades" can read
 *  differently rather than only differing by colour (F1: colour alone is
 *  not marking). */
export function collisionGroupHeadingKey(kind: Collision["kind"]): string {
  switch (kind) {
    case "downgrade":
      return "osinstall.packages.preview.group.downgrade";
    case "upgrade":
      return "osinstall.packages.preview.group.upgrade";
    case "same-version":
      return "osinstall.packages.preview.group.sameVersion";
    case "unversioned":
      return "osinstall.packages.preview.group.unversioned";
  }
}

/** How many of each class `reports` holds — the preview heading's own
 *  counts, shown before the list so its shape ("3 downgrades, 41 upgrades,
 *  13 unversioned") is legible before the list itself is read. */
export interface CollisionCounts {
  downgrades: number;
  upgrades: number;
  sameVersion: number;
  unversioned: number;
}

/**
 * The one heading above a host-placement preview — **three states, and they
 * stay three** (ART-287).
 *
 * It was a two-way ternary in both panels: counts when the answer had
 * arrived, *"Checking what this would replace…"* otherwise. *Otherwise*
 * covers a refusal, so a preview that finished and **failed** kept the
 * checking sentence over the top of its own red refusal box — which
 * `docs/assets/chain.png` caught: the screen saying it was still working
 * directly above a finished failure. Checking, done and refused are three
 * things a person does three different things about, and CLAUDE.md's rule is
 * that endings stay distinct.
 *
 * `null` for a preview that has neither started nor been asked for; the
 * caller renders no heading at all then, which is its own fourth state and
 * not this function's to invent.
 */
export function previewHeadingPhrase(
  collisions: CollisionReport[] | null,
  collisionsError: string | null
): Phrase | null {
  if (collisionsError !== null) return { key: "osinstall.packages.preview.refused" };
  if (collisions !== null) {
    return {
      key: "osinstall.packages.preview.heading",
      params: { ...collisionCounts(collisions) },
    };
  }
  return { key: "osinstall.packages.preview.loading" };
}

export function collisionCounts(reports: CollisionReport[]): CollisionCounts {
  const counts: CollisionCounts = { downgrades: 0, upgrades: 0, sameVersion: 0, unversioned: 0 };
  for (const report of reports) {
    switch (report.collision.kind) {
      case "downgrade":
        counts.downgrades++;
        break;
      case "upgrade":
        counts.upgrades++;
        break;
      case "same-version":
        counts.sameVersion++;
        break;
      case "unversioned":
        counts.unversioned++;
        break;
    }
  }
  return counts;
}

/** The sentence for one row's own collision — what it would replace and
 *  with what, or the sizes alone when neither side names a version.
 *
 *  **F1** — `upgrade` and `downgrade` now read different keys (they used to
 *  share one, `collision.versioned`, which is why a downgrade rendered
 *  exactly like an upgrade with nothing but colour to tell them apart).
 *  **F12** — byte counts go through `formatBytes` (`@/lib/panel.ts`, the
 *  same helper the rest of the app already uses) rather than crossing raw:
 *  spec §3 shows sizes the way a person reads them, in KB/MB, not as an
 *  unbroken run of digits. */
export function collisionPhrase(collision: Collision): Phrase {
  switch (collision.kind) {
    case "upgrade":
      return {
        key: "osinstall.packages.collision.upgrade",
        params: { from: collision.from, to: collision.to },
      };
    case "downgrade":
      return {
        key: "osinstall.packages.collision.downgrade",
        params: { from: collision.from, to: collision.to },
      };
    case "same-version":
      return {
        key: "osinstall.packages.collision.sameVersion",
        params: { version: collision.version },
      };
    case "unversioned":
      return {
        key: "osinstall.packages.collision.unversioned",
        params: {
          fromBytes: formatBytes(collision.fromBytes),
          toBytes: formatBytes(collision.toBytes),
        },
      };
  }
}

// ---------------------------------------------------------------------------
// What the screen holds, and the rules over it
// ---------------------------------------------------------------------------

/**
 * Why the install cannot run yet, or null when it can.
 *
 * A reason rather than a boolean: a disabled button that does not say why is
 * the defect ART-100 was.
 */
/**
 * Whether something already sits where the tree would go.
 *
 * Read-only. `apply()` refuses an existing destination and always has; this
 * exists so the screen can say so *before* the button rather than after.
 */
export async function osinstallDestinationTaken(destination: string): Promise<boolean> {
  return invoke<boolean>("osinstall_destination_taken", { destination });
}

/**
 * What a folder is, asked of the folder (ART-199).
 *
 * A step used to look ready on any folder at all, because all it knew was
 * whether *a path had been chosen*; the user found out otherwise from a
 * refusal on the button. This is the question a field can ask the moment a
 * folder is picked.
 *
 * Read-only, and it does not throw for a folder that is not a tree — that is
 * an answer, `isTree: false` with a `problem` saying which.
 */
export interface TreeSummary {
  isTree: boolean;
  release: string | null;
  files: number;
  /** Which components built it. What a tree carries is what decides whether a
   *  package can go on it, so a picker showing trees shows this. */
  components: string[];
  amigaInstalled: string[];
  /** Why it is not a tree, when it is not. English, from Rust (ART-060). */
  problem: string | null;
}

export async function osinstallDescribeTree(tree: string): Promise<TreeSummary> {
  return invoke<TreeSummary>("osinstall_describe_tree", { tree });
}

/** One build found inside a folder of builds, and what it carries. */
export interface FoundTree {
  /** The tree's own folder, absolute. */
  path: string;
  /** Its folder name — what the picker shows, so no component splits a path. */
  name: string;
  /** Always `isTree: true`; anything else is not returned. */
  summary: TreeSummary;
}

/**
 * Every distribution tree directly inside `folder` (ART-197 wave 2, row 1).
 *
 * The artefact picker's question. A folder of builds is the ordinary case —
 * the owner keeps several that differ by which components went in — and the
 * only way to tell them apart used to be pointing a step at one and reading
 * the refusal.
 *
 * **This one does throw** where `osinstallDescribeTree` does not: the user has
 * just pointed at the folder, so "that path is gone" is the true sentence.
 */
export async function osinstallTreesIn(folder: string): Promise<FoundTree[]> {
  return invoke<FoundTree[]>("osinstall_trees_in", { folder });
}

/**
 * When a folder holds install media and this release wants **none** of it,
 * the volume names it does hold — otherwise `null` (ART-208).
 *
 * The owner chose AmigaOS 3.2 with the folder holding their AmigaOS 3.9 disc
 * still selected, and the screen answered with sixteen `MediaMissing`
 * refusals: one per component, every one true, and together read as "a lot of
 * programs are missing" about a folder that was simply the wrong one.
 *
 * # ART-253: it said that about the right folder too
 *
 * This used to gate the claim on five conditions, and **two of them could not
 * fire**. `InstallPlan` carries an invariant (`core/osinstall/plan.rs`): a
 * plan is *either* a full description of what would be written *or* every
 * reason it cannot proceed, never both, and the one production construction
 * site empties `items` whenever there is any refusal at all. So
 * `plan.items.length > 0` is never true here, and asking whether a
 * **missing** disk is in the folder is near-tautologically false in the
 * ordinary partial case. Neither withdrew. A folder holding `Workbench3.2`,
 * `Fonts` and `Locale` with `Extras3.2` absent rendered *"None of the disks
 * in this folder are ones this release asks for. It holds: Workbench3.2,
 * Fonts, Locale."* — a false sentence that sends a user away from the correct
 * folder, which is this project's named failure class.
 *
 * Both are gone. The claim is now **checked against the release's own
 * recipe**, which is the only thing that knows what it asks for:
 * `evidence.distinguishing` plus `evidence.shared` is exactly "media this
 * release names that the folder holds", and if that is non-empty the sentence
 * is false and this withdraws in favour of the per-disk list and
 * {@link mediaEvidence}.
 *
 * `shared` counts here even though it never counts for *identification*:
 * `Fonts` and `Locale` cannot tell 3.1 from 3.2, but they are still disks the
 * 3.2 recipe asks for, so a folder holding them is not a folder holding none
 * of this release's media.
 *
 * **The conditions that remain, each still load-bearing:**
 *
 *  - `found` non-empty — an empty folder is `osinstall.media.empty`, said the
 *    moment it was picked. "None of these disks are the right ones" is a
 *    claim about disks that are not there.
 *  - at least one refusal — with none there is nothing to explain.
 *  - **every** refusal about media — an unreadable ROM, a destination
 *    collision or an exclusive-group conflict is a different problem with a
 *    different fix, and must never be papered over with a sentence about
 *    folders.
 *  - **nothing this release asks for is in the folder**, from `evidence`.
 *
 * @param evidence what `osinstallMediaEvidence` said about the release being
 *   built — `null` while the lookup is in flight or after it failed, and that
 *   is answered with silence. The sentence makes a specific claim and there
 *   is nothing to check it against; the per-disk refusal list shown instead
 *   is true either way.
 *
 * # ART-254: evidence about a *different* release is not evidence
 *
 * ART-253's guard checks the plan against the evidence, and that only means
 * anything while the two are about the same release. On the screen they are
 * fetched by two uncoordinated effects (`OsInstall.tsx`), so pressing the
 * "switch to the release this folder holds" button leaves a window in which
 * the plan is the new release's and the evidence is still the old one's —
 * and 3.1 evidence is legitimately empty for a folder full of 3.2 disks,
 * which is exactly the input that makes this sentence fire falsely again.
 *
 * Both artefacts state their own identity, so this is settled by an
 * **invariant** rather than by a reset, an ordering or a loading flag
 * (`CLAUDE.md`: *"anything timing-dependent gets an invariant, not a wait"*).
 * `ReleaseEvidence.release` is the recipe's own `release` and `plan.release`
 * is the same string from the same recipe (`plan.rs`: `recipe.release.clone()`),
 * so they are compared exactly, the way `releaseHolding === release` and
 * `isInstallRelease` compare release names everywhere else here — no folding,
 * no trimming, because a release name is not user-entered text.
 *
 * Mismatched evidence is treated as `null`: not checked, so nothing claimed.
 */
export function wrongMediaFolder(
  plan: InstallPlan,
  found: string[],
  evidence: ReleaseEvidence | null
): string | null {
  if (found.length === 0) return null;
  if (plan.refusals.length === 0) return null;
  const missing = plan.refusals.filter(
    (refusal): refusal is Extract<RefusalReason, { refusal: "media-missing" }> =>
      refusal.refusal === "media-missing"
  );
  if (missing.length !== plan.refusals.length) return null;
  if (evidence === null) return null;
  // ART-254 — see the doc comment above. Stale evidence cannot answer for
  // this plan's release, so it answers for nothing.
  if (evidence.release !== plan.release) return null;
  if (evidence.distinguishing.length + evidence.shared.length > 0) return null;
  return found.join(", ");
}

/**
 * Context for the ordinary partial-media case — the folder holds some of this
 * release's own disks and is missing others — which is exactly the case
 * {@link wrongMediaFolder} above refuses to speak in, because there its
 * sentence would be false. The refusals list already names which component
 * wants which disk; this adds what `wrongMediaFolder` cannot reach there:
 * what the folder itself looks like.
 *
 * **Not "at least one component is installable"**, which is what this said
 * until ART-253 and is a state the core cannot emit: any refusal at all
 * empties `items` (`plan.rs:1866`). The two are told apart by the release's
 * own evidence, never by counting plan items.
 *
 * Four endings, never collapsed into each other (this project's own named
 * failure class — see CLAUDE.md, "The failure that does not crash"):
 *
 *  - nothing missing, or no folder chosen yet — silent, both owned
 *    elsewhere (`osinstall.blocked.noFolder` for the second).
 *  - `wrongMediaFolder` owns the all-or-nothing case — silent here too, so
 *    the two callers can never both produce a sentence about the same plan.
 *  - **This release's own media, partly here** — `sameRelease`, naming what
 *    is present and what is still absent. Either because `identify` named
 *    this very release, or because this release's own recipe claims disks in
 *    the pile that no other release names (ART-257 — a based release's
 *    inherited base set is exactly that, and `identify` calls it the base).
 *    The sentence claims only that: this release's own media is among what
 *    the folder holds, which is what was checked (ART-258).
 *  - **Identified**, a *different* release, and none of this release's own
 *    **distinguishing** media in the pile — `otherRelease`, naming which
 *    one, so the user can tell "wrong folder" from "one disk short". `shared`
 *    can still be non-empty here (`Fonts`, `Locale` — media more than one
 *    release asks for), so the sentence says only that the folder looks like
 *    the named release, never that none of it is this release's own (M1,
 *    2026-09-06 review): `wrongMediaFolder` three functions up reads that
 *    same `shared` field as exactly the media this release does ask for.
 *  - **Ambiguous or unknown, and none of this release's own evidence in the
 *    pile** — `releaseHolding` is `null` for both (`recipe::release_holding`
 *    collapses them, ART-208's own type), and since this cannot tell them
 *    apart it says the weaker thing rather than guess: `unidentified`,
 *    naming only what is present. Claiming a release here would be exactly
 *    the confident-wrong sentence this project pays most for.
 *
 *    **`releaseHolding === null` is not on its own enough to reach this
 *    ending** (ART-259). A based release's update-only folder — `identify`
 *    answers `Unknown` for it, the same as a folder of `Fonts` and `Locale`
 *    — is the *first* ending above, not this one: this release's own recipe
 *    claims those disks (`holdsThisReleasesOwnMedia`), and that check runs
 *    before this one so the sentence said is `sameRelease`, never
 *    `unidentified`, over media that really is this release's own.
 *
 *    **The sentence states no reason, and that is deliberate** (ART-255).
 *    It used to end *"— some disks carry no version of their own"*, which is
 *    true of the Unknown case a folder of `Fonts` and `Locale` produces and
 *    false of the other three that reach this key: an **Ambiguous** folder
 *    (`Workbench3.2` *and* `AmigaOS3.9` — disks that do carry versions, two
 *    releases named, ART declining to choose), the render before the lookup
 *    lands, and the catch path. Three of the four states got a reason that
 *    was never checked, pointing the user at the wrong problem. `null` is
 *    one value with four causes, so the honest sentence is the one true of
 *    all of them: the contents do not settle which release this is.
 */
export function mediaEvidence(input: {
  plan: InstallPlan;
  /** Volume names the media scan actually found in the folder. */
  found: string[];
  /** Which shipped release those names are the install media of, when ART
   *  can tell — `null` when they are nobody's, or more than one release's. */
  releaseHolding: string | null;
  /** The release being built, to tell "this folder's media" from "someone
   *  else's media" apart. */
  release: string;
  /** What that release's own recipe made of `found` — passed straight
   *  through to {@link wrongMediaFolder}, which is the only thing here that
   *  reads it, and which refuses it when it answers for a release other than
   *  the plan's (ART-254). `null` while the lookup is in flight. */
  evidence: ReleaseEvidence | null;
}): Phrase | null {
  const { plan, found, releaseHolding, release } = input;
  const evidence = input.evidence;
  // ART-254. **The plan, the evidence and the release being built must all
  // name the same release**, and the three-way agreement takes three
  // comparisons, not two: this one; `evidence` against `plan.release` inside
  // {@link wrongMediaFolder} below (which is where it has to live —
  // `osinstallBlocker` calls that function with no `release` of its own);
  // and `evidence.release === release` directly, written out as `checkable`
  // further down and gating all three reads of `evidence` past this point.
  //
  // The third does **not** follow from the first two, which is why it is
  // written out rather than left as decoration (ART-253's own ruling: a
  // condition that cannot fire is decoration, and this one can). Calling
  // `wrongMediaFolder` below returns null far more often for reasons that
  // have nothing to do with the release match — a mixed refusal list, no
  // media missing at all — than because its own `evidence.release !==
  // plan.release` check fired, so reaching this point with `plan.release
  // === release` and a falsy `wrongMediaFolder` result does not establish
  // that `evidence` is about this release. Stale evidence can still be
  // sitting in `evidence` here, and `checkable` is what keeps it from being
  // read as if it were current. Removing the `evidence.release === release`
  // half of `checkable` is exactly the mutation
  // `will not call a folder somebody else's media without this release's own
  // evidence` (osinstall.test.ts) exists to catch.
  //
  // The plan is checked here rather than in the sibling because this is the
  // only place that receives the release being built, and it withdraws the
  // whole sentence rather than one clause: `missing` is read straight off the
  // plan, so a plan the release switch has left behind would have this line
  // naming *the previous release's* absent disks under the new release's
  // name. Two stale artefacts agreeing with each other do not make one
  // current sentence.
  if (plan.release !== release) return null;
  // No folder chosen — `osinstall.blocked.noFolder` already owns this
  // sentence; a second one here would answer the same question twice.
  if (found.length === 0) return null;
  const missing = plan.refusals.filter(
    (refusal): refusal is Extract<RefusalReason, { refusal: "media-missing" }> =>
      refusal.refusal === "media-missing"
  );
  if (missing.length === 0) return null;
  // The all-or-nothing case belongs to the sibling above; never both speak.
  if (wrongMediaFolder(plan, found, evidence)) return null;

  const foundNames = found.join(", ");
  const missingNames = missing.map((refusal) => refusal.volume_name).join(", ");

  /**
   * **A based release's inherited media is its own media** (ART-257).
   *
   * `identify` answers "which release is this pile", and for a folder holding
   * nothing but AmigaOS 3.2's disks that answer is *"AmigaOS 3.2"* even when
   * the release being built is AmigaOS 3.2.2 — the based recipe is dropped
   * from the candidates because it found none of its **own** update disks
   * (`identify.rs`'s base-subsumption pass, and it is right to: a based
   * release must not be named on its base's evidence alone). Measured, not
   * assumed: `evidence_for("AmigaOS 3.2.2", <the 3.2 base set>)` answers
   * `distinguishing: [Workbench3.2, Install3.2, Extras3.2]`,
   * `missing_required: [Update3.2.2, Classes3.2.2]`, while
   * `release_holding` of the same names answers `"AmigaOS 3.2"`
   * (`identify.rs::a_based_releases_own_evidence_claims_the_base_set`).
   *
   * So `releaseHolding !== release` alone does **not** mean somebody else's
   * media, and saying *"that looks like AmigaOS 3.2 media"* over a half-built
   * 3.2.2 would be a misleading sentence sending a user to look for a
   * different folder — with the disks they need already in it.
   *
   * The release's **own recipe** is what settles it, and it is asked rather
   * than reasoned about: `distinguishing` is "present, named by this release,
   * named by no other" (`Fonts` and `Locale` are in `shared` and never count
   * here), so a non-empty one means at least one disk in this pile is media
   * only the release being built asks for. That is the same claim
   * `sameRelease` makes, so it is the sentence said.
   *
   * Not a fourth ending: "the base is here, the update disks are not" and
   * "some of this release's disks are here, others are not" are one state
   * with one next step, and `missing` already names exactly which disks.
   * Splitting them would be a second sentence for one answer, not a distinct
   * ending.
   *
   * Stale or in-flight evidence cannot answer for this release (ART-254), so
   * it does not: the check requires the evidence to say which release it is
   * about, and `releaseHolding === release` above still stands on its own
   * for every folder `identify` can name outright.
   *
   * **This check must run before the `releaseHolding === null` branch below**
   * (ART-259, caught in this fix wave's own re-review of ART-257). A folder
   * holding only AmigaOS 3.2.2's *update* disks (`Update3.2.2`,
   * `Classes3.2.2`) and none of the base set makes `identify` answer
   * `Unknown` — a based release with none of its base present has a
   * non-empty `missing_required` and is dropped from the named candidates,
   * and no other release claims update-disk names — so `releaseHolding` is
   * `null` for exactly the folder this check exists to recognise. Testing
   * `releaseHolding === null` first, as the original ART-257 fix did, made
   * this whole block unreachable for that folder and answered
   * `unidentified` — *"this folder does not identify a release"* — about a
   * folder that names update disks nothing else in the catalogue does.
   */
  const checkable = evidence !== null && evidence.release === release;
  const holdsThisReleasesOwnMedia = checkable && evidence.distinguishing.length > 0;
  if (releaseHolding === release || holdsThisReleasesOwnMedia) {
    return {
      key: "osinstall.evidence.sameRelease",
      params: { found: foundNames, missing: missingNames },
    };
  }
  // Reached only once this release's own evidence has already been asked and
  // has nothing to say (`holdsThisReleasesOwnMedia` is false, or the evidence
  // cannot answer for this release at all). `releaseHolding === null` here
  // covers Unknown, Ambiguous, the lookup still in flight and the catch path
  // alike (ART-255) — none of which this release's own recipe told apart
  // from any other, so the weaker sentence, naming no release, is the honest
  // one.
  if (releaseHolding === null) {
    return { key: "osinstall.evidence.unidentified", params: { found: foundNames } };
  }
  // `otherRelease` no longer claims the folder holds none of this release's
  // own media (M1, 2026-09-06 review): `evidence.shared` can be non-empty
  // here even though `distinguishing` is empty, and `wrongMediaFolder` three
  // functions up already reads that same field as media this release does
  // ask for, so the old wording overclaimed exactly what that sibling
  // function was careful not to. The sentence now says only what `identify`
  // and the refusals list support — the folder looks like a named different
  // release, and these disks are still missing.
  //
  // Reaching this point still needs `checkable`: while evidence is in
  // flight, failed, or a release behind (ART-254), it might yet turn out
  // this release's own distinguishing media is in the pile too
  // (`holdsThisReleasesOwnMedia`), so nothing is said until evidence can
  // rule that out. The user is not left without an answer: the per-disk
  // refusals list is below it and is true whatever the evidence turns out
  // to be.
  if (!checkable) return null;
  return {
    key: "osinstall.evidence.otherRelease",
    params: { found: foundNames, release: releaseHolding, missing: missingNames },
  };
}

export function osinstallBlocker(input: {
  mediaFolder: string | null;
  destination: string | null;
  destinationTaken: boolean;
  plan: PlanResult | null;
  /** Volume names the media scan actually found in the folder. */
  found: string[];
  /** Which shipped release those names are the install media of, when ART
   *  can tell — `null` when they are nobody's, or more than one release's
   *  (`recipe::release_holding`). */
  releaseHolding: string | null;
  /** What the release being built makes of those names — see
   *  {@link wrongMediaFolder}, which is what reads it. */
  mediaFacts: ReleaseEvidence | null;
}): Phrase | null {
  if (!input.mediaFolder?.trim()) return { key: "osinstall.blocked.noFolder" };
  if (!input.destination?.trim()) return { key: "osinstall.blocked.noDestination" };
  // Before the plan, because it is true regardless of what the plan says and
  // is the one blocker the user can fix without understanding any of the rest.
  if (input.destinationTaken) {
    return { key: "osinstall.blocked.destinationExists", params: { path: input.destination } };
  }
  if (!input.plan) return { key: "osinstall.blocked.notPlanned" };
  if (input.plan.outcome === "folder-unreadable") {
    return { key: "osinstall.blocked.folderUnreadable" };
  }
  if (input.plan.plan.refusals.length > 0) {
    const wrongFolder = wrongMediaFolder(input.plan.plan, input.found, input.mediaFacts);
    if (wrongFolder) {
      return input.releaseHolding
        ? {
            key: "osinstall.blocked.wrongFolderIsRelease",
            params: { release: input.releaseHolding, found: wrongFolder },
          }
        : { key: "osinstall.blocked.wrongFolder", params: { found: wrongFolder } };
    }
    return { key: "osinstall.blocked.refusals" };
  }
  if (input.plan.plan.items.length === 0) return { key: "osinstall.blocked.nothingToInstall" };
  return null;
}

/** Whether this report can honestly be called verified — never `failed ===
 *  0` alone (§89): a file ART never looked at is not a file ART cleared. */
export function isVerified(report: VerifyReport): boolean {
  return report.failed === 0 && report.notChecked === 0;
}

/** The sentence for why one file's install cannot proceed, for the
 *  component to render. */
export function refusalPhrase(reason: RefusalReason): Phrase {
  switch (reason.refusal) {
    case "media-missing":
      return {
        key: "osinstall.refusal.mediaMissing",
        params: { component: reason.component, volume: reason.volume_name },
      };
    case "media-path-missing":
      return {
        key: "osinstall.refusal.mediaPathMissing",
        params: { component: reason.component, media: reason.media, path: reason.path },
      };
    case "media-unreadable":
      return {
        key: "osinstall.refusal.mediaUnreadable",
        params: {
          component: reason.component,
          volume: reason.volume_name,
          path: reason.path,
          reason: reason.reason,
        },
      };
    case "rom-unknown":
      return { key: "osinstall.refusal.romUnknown" };
    case "resident-table-unreadable":
      return {
        key: "osinstall.refusal.residentTableUnreadable",
        params: { component: reason.component, resident: reason.resident },
      };
    case "destination-collision":
      return {
        key: "osinstall.refusal.destinationCollision",
        params: { path: reason.path, components: reason.components.join(", ") },
      };
    case "media-ambiguous":
      return {
        key: "osinstall.refusal.mediaAmbiguous",
        params: {
          component: reason.component,
          volume: reason.volume_name,
          paths: reason.paths.join(", "),
        },
      };
    case "layers-share-folder":
      return {
        key: "osinstall.refusal.layersShareFolder",
        params: { layers: reason.layers.join(", "), folder: reason.folder },
      };
    case "exclusive-group-conflict":
      return {
        key: "osinstall.refusal.exclusiveGroupConflict",
        params: { group: reason.group, components: reason.components.join(", ") },
      };
    case "rule-kind-mismatch":
      return {
        key: "osinstall.refusal.ruleKindMismatch",
        params: {
          component: reason.component,
          from: reason.from,
          expected: reason.expected,
          found: reason.found,
        },
      };
    case "package-unknown":
      return { key: "osinstall.refusal.packageUnknown", params: { package: reason.package } };
    case "package-folder-missing":
      return {
        key: "osinstall.refusal.packageFolderMissing",
        params: { packages: reason.packages.join(", ") },
      };
    case "package-requirement-needs-amiga-run":
      return {
        key: "osinstall.refusal.packageRequirementNeedsAmigaRun",
        params: { package: reason.package, requirement: reason.requirement },
      };
    case "package-requirement-missing":
      return {
        key: "osinstall.refusal.packageRequirementMissing",
        params: { package: reason.package, requires: reason.requires },
      };
    case "package-component-missing":
      return {
        key: "osinstall.refusal.packageComponentMissing",
        params: { package: reason.package, component: reason.component },
      };
    case "package-archive-missing":
      return {
        key: "osinstall.refusal.packageArchiveMissing",
        params: { package: reason.package, media: reason.media },
      };
    case "package-archive-ambiguous":
      return {
        key: "osinstall.refusal.packageArchiveAmbiguous",
        params: { package: reason.package, media: reason.media, paths: reason.paths.join(", ") },
      };
    case "activation-source-missing":
      return {
        key: "osinstall.refusal.activationSourceMissing",
        params: { name: reason.name, from: reason.from, component: reason.component },
      };
    case "package-not-placeable-on-host":
      // Keyed on the block, not on the refusal alone: what the user has to
      // be told is what the *package* needs, and a second kind of block
      // would need a different sentence entirely.
      return {
        key: `osinstall.refusal.packageNotPlaceableOnHost.${hostPlacementBlockSuffix(reason.block)}`,
        params: { package: reason.package },
      };
  }
}

/**
 * The catalogue-key fragment naming one [`HostPlacementBlock`] — the single
 * `switch` every sentence about a block is built from, so a second kind
 * cannot arrive with one of its two sentences missing.
 *
 * Two keys, not one shared sentence: the checklist row explains the block
 * with no package name (it is already sitting under the package's own name),
 * while the refusal list needs `{{package}}` because several refusals can
 * appear at once. Handing the row a sentence carrying `{{package}}` with no
 * parameter would render the literal braces on screen — the exact bug
 * `PartialPhrase` exists to catch elsewhere.
 */
export function hostPlacementBlockSuffix(block: HostPlacementBlock): string {
  switch (block) {
    case "encrypted-payload":
      return "encryptedPayload";
    case "needs-fixfonts":
      return "needsFixfonts";
    case "needs-installer-script":
      return "needsInstallerScript";
  }
}

/** The i18n key explaining one [`HostPlacementBlock`] on a checklist row —
 *  no parameters; see [`hostPlacementBlockSuffix`]. */
export function hostPlacementBlockKey(block: HostPlacementBlock): string {
  return `osinstall.packages.blocked.${hostPlacementBlockSuffix(block)}`;
}

/**
 * The i18n key for a **chain row** refused because ART cannot place the
 * package from the host — takes `{{name}}`.
 *
 * A third key per block, and it is the same lesson `notYetRunnableChainKey`
 * carries one entry down (fix round 1, m6): the checklist's own sentence
 * sits directly under the package's name and must not repeat it, while a
 * chain row is one line among nine and has to say what it is about. The
 * chain rendered the checklist's key until round 3's fix round 1, so
 * Euro-Update's row read *"This package replaces the bitmap fonts…"* with
 * nothing naming the package — and on every tree without BoingBags 3&4 that
 * is the state Euro-Update is actually in, so it is the ordinary row rather
 * than a corner of one.
 */
export function hostPlacementBlockChainKey(block: HostPlacementBlock): string {
  return `osinstall.chain.refusedNotPlaceable.${hostPlacementBlockSuffix(block)}`;
}

/**
 * The catalogue-key fragment naming one {@link NotYetRunnable} — the single
 * `switch` both sentences about it are built from, exactly as
 * {@link hostPlacementBlockSuffix} is for a block (fix round 1, m6).
 *
 * Two keys per reason, not one shared sentence: the chain row names the
 * package (it is one line in a list of nine), while the Amiga-side panel's
 * row sits directly under the package's own name and must not repeat it.
 */
export function notYetRunnableSuffix(reason: NotYetRunnable): string {
  switch (reason) {
    case "installer-not-measured":
      return "installerNotMeasured";
  }
}

/** The i18n key for the Amiga-side panel's own disabled row — no package
 *  name, because it renders under one. */
export function notYetRunnablePanelKey(reason: NotYetRunnable): string {
  return `osinstall.amigaInstall.package.notYetRunnable.${notYetRunnableSuffix(reason)}`;
}

/** The i18n key for the chain row's sentence — takes `{{name}}`. */
export function notYetRunnableChainKey(reason: NotYetRunnable): string {
  return `osinstall.chain.notYetRunnable.${notYetRunnableSuffix(reason)}`;
}

/** Whether a plan's own refusals include the one that means "the paired
 *  Kickstart could not be identified" — used both for the run blocker (via
 *  `refusalPhrase`) and by the component list's own reasoning below. */
export function hasRomUnknownRefusal(plan: InstallPlan): boolean {
  return plan.refusals.some((r) => r.refusal === "rom-unknown");
}

// ---------------------------------------------------------------------------
// The component catalogue — the chosen release's own recipe, loaded
//
// **Not a hand-written mirror any more.** This module used to carry
// `AMIGAOS_32_COMPONENTS`, a literal copy of
// `src-tauri/src/core/osinstall/recipes/amigaos-3.2.json`, and the OS Install
// screen rendered it whatever release the picker had selected. With
// "AmigaOS 3.9" chosen the user saw 26 components for a recipe that holds
// one, and 3.9's own `workbench-base` was labelled `Workbench3.2` — both
// recipes use that component id, so a label resolved against the wrong
// recipe named a floppy volume that has nothing to do with the disc being
// installed from. Nothing was ever written wrongly (the engine is the
// authority and ignores an id its recipe does not hold), but showing one
// operating system's parts while installing another's is §89 on the screen
// itself.
//
// A parity test cannot fix that, only a second copy of the same mistake:
// the list has to *be* the recipe. `osinstallComponents` asks the Rust side,
// which projects the chosen release's own `Recipe` and refuses a release it
// ships no recipe for. Everything below therefore takes the loaded list as
// its first argument rather than reaching for a module constant — a
// component id means nothing without knowing which release is being
// installed.
// ---------------------------------------------------------------------------

export interface ComponentDef {
  id: string;
  /** The volume name inside the image — shown as the row's own label,
   *  unlocalized, the same way the preload screen prints a partition's
   *  `drive_name` untranslated: this is what the Amiga side calls it, not a
   *  sentence ART wrote. Overridden by `labelKey` where the recipe names one. */
  media: string;
  /** The recipe's own i18n key for this row (ART-224), or `null`.
   *
   *  `media` is the right label while every component comes off its own
   *  volume, which is AmigaOS 3.2's whole sixteen-disk shape. AmigaOS 3.9 is
   *  five components off **one disc**, so every row read `AmigaOS3.9` — true,
   *  identical, and useless for deciding which box to tick. The recipe names
   *  the key; the screen resolves it; `src/i18n/recipe-component-keys.test.ts`
   *  checks every key against both catalogues, and the Rust side refuses a
   *  recipe that shares a medium without labelling its rows. */
  labelKey: string | null;
  required: boolean;
  available: boolean;
  /** `Condition::RomOlderThan { major }`, flattened by the command — `null`
   *  for an unconditional component **and for one conditioned the other
   *  way**. The Rust side's `match` is exhaustive, so a further `Condition`
   *  variant is a compile error there rather than a silent `null` here. */
  conditionMajor: number | null;
  /** `Condition::RomAtLeast { major }` (ART-157) — the Kickstart floor this
   *  component's own files need, `null` when it declares none.
   *
   *  A separate field, not a second meaning for `conditionMajor`: the two
   *  numbers read alike and say opposite things ("switches on below V47" vs
   *  "needs at least V40"), and `conditionalReason`'s whole vocabulary is
   *  written for the first. Rendering a minimum through it would tell the
   *  user the reverse of the truth. */
  requiresRomMajor: number | null;
  exclusiveGroup: string | null;
  /** Which components this one declares it may write over (ART-175).
   *
   *  The screen asks for a collision preview of exactly the switched-on
   *  components whose list is non-empty, and no others — previewing every
   *  component would mean reading a whole install off media to answer a
   *  question about a few dozen files. Five components declare one in
   *  shipped data (3.2's `extras`, `modules-a1200`, `classes` and
   *  `glowicons`; 3.9's `workbench-39`) — see the recipe-parity test, which
   *  pins that list rather than trusting it. */
  overrides: string[];
}

/**
 * Which components `release`'s own shipped recipe holds, in recipe order.
 *
 * Read-only: parses shipped JSON, opens no media, writes nothing. An unknown
 * release rejects rather than answering with a default catalogue.
 */
export async function osinstallComponents(release: string): Promise<ComponentDef[]> {
  return invoke<ComponentDef[]>("osinstall_components", { release });
}

/**
 * Where one release's component picks are remembered.
 *
 * **Per release, not shared.** A component id means something only inside
 * the recipe that declares it, and the two shipped recipes both use
 * `workbench-base` for different media. Sharing one remembered set would
 * mean either carrying ids the current release cannot install, or dropping
 * ids the *other* release legitimately holds the moment the user switched —
 * and a choice destroyed by switching away and back is exactly the thing
 * this project's remembered-settings rule forbids.
 *
 * `AmigaOS 3.2` keeps the unsuffixed key it has always used. It is the only
 * release that existed before there was a picker, so anyone upgrading into
 * this version finds the selection they last made still ticked, rather than
 * an empty list under a key nothing ever wrote.
 */
const RELEASE_BEFORE_THE_PICKER = "AmigaOS 3.2";

export function rememberedComponentKey(base: string, release: string): string {
  return release === RELEASE_BEFORE_THE_PICKER ? base : `${base}.${release}`;
}

export function componentDef(components: ComponentDef[], id: string): ComponentDef | undefined {
  return components.find((c) => c.id === id);
}

/** A component's own label **when the recipe names no i18n key for it** —
 *  the volume name, or the bare id when the loaded release does not hold the
 *  component (a plan item from a release whose list has not arrived yet,
 *  never a fabricated volume name).
 *
 *  Deliberately not translation-aware. `src/lib` holds no i18next singleton
 *  and never renders a sentence, so a row that has a `labelKey` is resolved
 *  by the component that draws it — see `OsInstall.tsx`'s `label()`. This
 *  stays the fallback both sides call. */
export function componentLabel(components: ComponentDef[], id: string): string {
  return componentDef(components, id)?.media ?? id;
}

/** `chosen`, with anything that is not a real, available component id of
 *  `components` dropped — a stale remembered id (an old ART's component that
 *  was renamed or removed) or an unavailable one (Coming Later) can never
 *  reach `InstallRequest.chosen`.
 *
 *  **Only call this with a list that has actually loaded.** Against an empty
 *  `components` it drops everything, and persisting that would be a setting
 *  changing without the user changing it (ART-089's shape). The screen holds
 *  `null` for "not loaded yet" and does not sanitize until it is a list. */
export function sanitizeChosen(components: ComponentDef[], chosen: string[]): string[] {
  return chosen.filter((id) => componentDef(components, id)?.available === true);
}

// ---------------------------------------------------------------------------
// Conditional components — reasoning and the toggle state machine
//
// Pulled out of the screen component so it can be unit-tested directly —
// the fix-round answer to a review finding that a browser pass was the only
// way to exercise `withExclusions`/`isForcedOnByCondition`/the toggle
// handlers, and that a Critical defect (a stale `mediaPaths`) had shipped
// inside exactly that untested logic. `withExclusions` itself is gone: the
// engine now performs the exclusion (`InstallRequest.excluded`), so there is
// nothing left to filter on this side — only *reasoning about* the plan the
// engine already excluded correctly.
// ---------------------------------------------------------------------------

/**
 * Whether `id` is switched on **only** because its own `Condition` is
 * satisfied — never because it is `required` or because the caller put it
 * in `chosen`. Rust's `resolve_components_on` computes `is_on` as
 * `required || chosen.contains(id)`, then ORs in the condition — it can
 * only ever add `true`, never remove one — so if `plan.componentsOn`
 * carries `id` and neither of the first two is true, the condition is the
 * only thing left that could have done it.
 *
 * Always call this with a plan requested with an **empty** `excluded` list
 * (a "base" plan) — a plan requested with `id` itself excluded will never
 * carry `id` in `componentsOn` at all (the engine skips it entirely), which
 * would make this function report `false` for a component that is, in
 * truth, still condition-satisfied and merely turned off. The screen keeps
 * two plans for exactly this reason: one reflecting the real exclusions,
 * for the file list and for `osinstallApply`; one with none, purely to
 * reason about what *would* be on.
 */
export function isForcedOnByCondition(
  components: ComponentDef[],
  basePlan: InstallPlan | null,
  chosen: string[],
  id: string
): boolean {
  const def = componentDef(components, id);
  if (!basePlan || !def || def.required) return false;
  return basePlan.componentsOn.includes(id) && !chosen.includes(id);
}

/**
 * Every excluded id whose component is still, in truth, condition-satisfied
 * — the ones worth keeping. An excluded id whose condition no longer holds
 * (the paired ROM changed) is pruned: leaving it in the remembered set would
 * silently reapply the override the moment a pre-V47 ROM came back, without
 * the user ever confirming *that* pairing, which is "nothing changes unless
 * the user changes it" read backwards.
 */
export function pruneStaleExclusions(
  components: ComponentDef[],
  basePlan: InstallPlan,
  chosen: string[],
  excluded: string[]
): string[] {
  return excluded.filter((id) => isForcedOnByCondition(components, basePlan, chosen, id));
}

/** Why a conditional component's row is ticked or not — always exactly one
 *  of these four, never none: a branch chain that can fall through to
 *  nothing is the defect a review found here once already (a tick with no
 *  explanation, when the frontend's own ROM read and the plan's own ROM
 *  read disagreed). `rom-needed` is the catch-all: whenever the plan itself
 *  could not decide the condition (no ROM, or one it could not identify),
 *  every other branch is unreachable by construction below. */
export type ConditionalReason =
  | { kind: "rom-needed" }
  | { kind: "condition-on"; rom: string; major: number }
  | { kind: "condition-off"; rom: string; major: number }
  | { kind: "condition-overridden"; major: number };

/**
 * Decide a conditional component's reason, from primitives rather than from
 * `InstallPlan`/`RomInfo` directly — easier to test exhaustively (every
 * combination of three booleans and an optional string, sixteen cases) and
 * it cannot itself misread a plan that was requested with the wrong
 * `excluded` list, because it never sees one.
 *
 * `romUnknown` should come from the **base** plan's own refusals
 * (`hasRomUnknownRefusal`), not the effective one — the effective plan can
 * legitimately have no `RomUnknown` refusal at all once the one conditional
 * component that needed the ROM decided has been excluded (that is the
 * point of requirement 2: no ROM, no Modules, nothing left to decide), and
 * that must not be read as "the ROM is fine".
 */
export function conditionalReason(
  major: number,
  forcedOn: boolean,
  excluded: boolean,
  romUnknown: boolean,
  rom: string | null
): ConditionalReason {
  if (romUnknown || rom === null) return { kind: "rom-needed" };
  if (excluded && forcedOn) return { kind: "condition-overridden", major };
  if (forcedOn) return { kind: "condition-on", rom, major };
  return { kind: "condition-off", rom, major };
}

/**
 * How a `ConditionalReason` reads on screen — the catalogue key, and whether
 * it is a warning badge or a quiet line.
 *
 * **Why this exists rather than four `&&` blocks in the JSX (ART-119 #2).**
 * The screen used to render each kind as its own independent guard, so a
 * fifth kind would have rendered *nothing at all* — a conditional row ticked
 * with no explanation, which is the exact defect a review already found here
 * once (`conditionalReason`'s own doc comment says so). `ConditionalReason`
 * is a discriminated union, so a `switch` with a `never` fallthrough turns
 * that into a compile error instead. Living in `src/lib` rather than in the
 * component is what makes it testable and what puts its keys in front of
 * `src/i18n/phrase-keys.test.ts`, which is the only thing that catches a
 * `Phrase` pointing at a key nobody added.
 *
 * `tone` is the rendering decision the four guards also disagreed on:
 * `condition-on` and `condition-overridden` are warnings (something is about
 * to happen, or has been overridden), the other two are statements of fact.
 */
export type ConditionalReasonText = { phrase: Phrase; tone: "warn" | "faint" };

export function conditionalReasonText(reason: ConditionalReason): ConditionalReasonText {
  switch (reason.kind) {
    case "rom-needed":
      return { phrase: { key: "osinstall.components.reason.romNeeded" }, tone: "faint" };
    case "condition-overridden":
      return {
        phrase: {
          key: "osinstall.components.reason.conditionOverridden",
          params: { major: reason.major },
        },
        tone: "warn",
      };
    case "condition-on":
      return {
        phrase: {
          key: "osinstall.components.reason.conditionOn",
          params: { rom: reason.rom, major: reason.major },
        },
        tone: "warn",
      };
    case "condition-off":
      return {
        phrase: {
          key: "osinstall.components.reason.conditionOff",
          params: { rom: reason.rom, major: reason.major },
        },
        tone: "faint",
      };
    default: {
      // A fifth kind lands here and fails to compile, which is the point.
      const unreachable: never = reason;
      return unreachable;
    }
  }
}

/** What checking or unchecking a conditional component's box should do,
 *  decided from state alone so the screen's `onChange` handler is a plain
 *  dispatch over the result. */
export type ConditionalToggleAction = "undo-exclusion" | "confirm-off" | "toggle-chosen";

export function conditionalToggleAction(excluded: boolean, forcedOn: boolean): ConditionalToggleAction {
  if (excluded) return "undo-exclusion";
  if (forcedOn) return "confirm-off";
  return "toggle-chosen";
}

/** Ordinary opt-in/opt-out through `chosen` for a non-conditional
 *  component (and the opt-in half of a conditional one's own toggle, when
 *  its condition does not currently hold) — clears any other member of the
 *  same `exclusiveGroup` on the way in. */
export function toggleChosen(components: ComponentDef[], chosen: string[], id: string): string[] {
  if (chosen.includes(id)) return chosen.filter((c) => c !== id);
  const group = componentDef(components, id)?.exclusiveGroup ?? null;
  const withoutGroup = group
    ? chosen.filter((c) => componentDef(components, c)?.exclusiveGroup !== group)
    : chosen;
  return [...withoutGroup, id];
}

/** Confirming "turn this off anyway": excluded and chosen are kept mutually
 *  exclusive by construction, so confirming an exclusion also drops any
 *  stray `chosen` entry for the same id. */
export function confirmComponentOff(
  chosen: string[],
  excluded: string[],
  id: string
): { chosen: string[]; excluded: string[] } {
  return {
    chosen: chosen.filter((x) => x !== id),
    excluded: excluded.includes(id) ? excluded : [...excluded, id],
  };
}

export function withoutExcluded(excluded: string[], id: string): string[] {
  return excluded.filter((x) => x !== id);
}

// ---------------------------------------------------------------------------
// The Verify section's own small parsers — pulled out for the same reason:
// Minor findings both traced back to ad hoc `Number.parseInt` at the two
// call sites disagreeing with each other instead of sharing one answer.
// ---------------------------------------------------------------------------

/** A PiStorm area number, or `null` for "a plain HDF" — `""` is legal and
 *  means the latter; anything else has to be a non-negative integer, never
 *  `NaN` silently reaching the wire as `null` (`JSON.stringify(NaN)` is
 *  `"null"`, which is indistinguishable from a deliberate "no slot"). */
export function parseOptionalSlot(text: string): { ok: true; value: number | null } | { ok: false } {
  const trimmed = text.trim();
  if (trimmed === "") return { ok: true, value: null };
  if (!/^\d+$/.test(trimmed)) return { ok: false };
  return { ok: true, value: Number.parseInt(trimmed, 10) };
}

/** A partition index, 1-based — `null` for anything that is not a whole
 *  number `>= 1`. The one function both the Verify button's `disabled` and
 *  `runVerify`'s own guard call, so the two cannot silently disagree about
 *  what counts as ready the way an enabled-but-inert button once did. */
export function parsePartitionIndex(text: string): number | null {
  const trimmed = text.trim();
  if (!/^\d+$/.test(trimmed)) return null;
  const n = Number.parseInt(trimmed, 10);
  return n >= 1 ? n : null;
}
