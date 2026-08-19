//! Turning a [`Recipe`], a media folder and (optionally) a ROM into an
//! [`InstallPlan`] — or into every reason it cannot proceed.
//!
//! ## Collect every refusal, never stop at the first
//!
//! The screen shows every problem at once; a user fixing them one dispatch
//! at a time is a bad afternoon. So [`plan`] never returns early on a
//! recoverable problem — a missing disk, a path the recipe expects that the
//! media does not have, an unresolved collision — it keeps walking and
//! collects every [`RefusalReason`] it finds. But any refusal at all empties
//! `items` and `media_paths`: a half-planned install, with some of the
//! files it would write and none of the ones it could not resolve, is not
//! something to preview.
//!
//! ## A green recipe says nothing about file-level collisions
//!
//! `recipe.rs`'s own collision test only checks `File`-kind rules against
//! each other, because a coinciding `Subtree` destination is a legal merge
//! point (fifteen locale disks all contribute to `Locale/Languages`) rather
//! than a claim. The check here is the same rule, applied one level lower —
//! over the *expanded* item list, after every `Subtree` rule has been
//! walked out into the real files the media actually holds. That is where a
//! genuine collision (two components writing the same destination *file*)
//! can actually be seen, and it is what the spec means by "a collision
//! inside a plan is a defect".
//!
//! ## `MediaMatch` is matched exhaustively, never `if let`
//!
//! `scan::MediaMatch` is `#[must_use]` and says so in its own doc: an
//! `if let MediaMatch::Found(..)` silently reads `Ambiguous` as `Missing`,
//! which is the arbitrary-winner failure the enum exists to rule out. This
//! module is the one place that resolves a component's media, so it is the
//! one place that has to get this right.
//!
//! ## `exclusive_group` is enforced against what resolved, not what was asked
//!
//! `Component::exclusive_group` existed since Task 1 with nothing checking
//! it — inert with the shipped recipe's one Modules disk, but a field named
//! `exclusive_group` that no code enforces is a claim the codebase does not
//! keep. [`detect_exclusive_group_conflicts`] checks
//! [`InstallPlan::components_on`] — the resolved set — rather than
//! `InstallRequest::chosen`, because a condition-satisfied component can be
//! switched on without ever being chosen (see
//! `a_conditional_component_is_on_without_being_chosen`); a check against
//! the request alone would miss exactly the case a `Condition` exists to
//! create.
//!
//! ## A rule whose `kind` disagrees with the media is refused, not emitted wrong
//!
//! Recipes are data — a future release arrives as a new JSON file, not new
//! code — so `plan()` is the only place a `File` rule that actually
//! resolves to a directory (or a `Subtree` rule over a plain file) can be
//! caught: `recipe.rs`'s `validate` has no media to resolve a path
//! against. Emitting it anyway would be silently wrong in two different
//! ways depending on direction — a `File`-over-directory item would carry
//! `is_dir: true` and slip past `detect_collisions`, which only looks at
//! files; a `Subtree`-over-file item would carry `bytes: 0` and be quietly
//! short. `RefusalReason::RuleKindMismatch` names the rule instead.
//!
//! ## The ROM's own header, never `KNOWN_ROMS`
//!
//! `Condition::RomOlderThan` exists because `Workbench3.2.adf:S/Startup-sequence`
//! opens with `Version exec.library version 47` / `If Warn` / … / `Quit` — a
//! 3.2 system installed on an older Kickstart, without `LIBS:Modules`, does
//! not boot at all. So the condition has to be decided, not skipped.
//!
//! It is decided from `core::rom::stated_version`, which reads the major and
//! minor a Kickstart states about itself at offset 12 in its own header —
//! never from `KNOWN_ROMS`, the curated table of dump checksums. ART-104 is
//! why: the user's own licensed A1200 Kickstart hashes to a dump that table
//! does not carry, so it comes back unidentified even though it is a
//! perfectly good 3.1 ROM. A condition resting on that table would misfire
//! on a ROM that is right; asking the ROM what it is costs nothing extra and
//! cannot be wrong about a dump nobody has catalogued yet.
//!
//! ## Refuse, never guess
//!
//! An unidentified ROM makes [`condition_holds`] return
//! `Err(RefusalReason::RomUnknown)` rather than picking a default. Guessing
//! "off" on a pre-V47 ROM produces a system that quits at boot; guessing
//! "on" on a V47 ROM wastes 800 KB installing modules nothing loads. Neither
//! is ART's to choose for the user, so neither is chosen.
//!
//! ## Packages are placed last, from a second folder
//!
//! An update package exists to land on top of what the release put down, so
//! its items are expanded **after** every switched-on component's, in
//! [`super::package::order`]'s order rather than the order the boxes were
//! ticked in. `apply` writes items in plan order and lets the last writer
//! win, so "after" is the whole mechanism by which a BoingBag's `C/Assign`
//! replaces the base disc's rather than the other way round.
//!
//! They come from a **second folder** ([`InstallRequest::package_folder`]),
//! never the media folder: the owner keeps discs in `Amigatolon\iso` and
//! archives in `Amigatolon\paketler`, so one path cannot answer both
//! questions, and [`super::scan::find_packages`] is a different scan asking
//! a different question of a different kind of file. Naming packages with no
//! folder is [`RefusalReason::PackageFolderMissing`], never an empty result
//! — the difference between "you did not give me the folder" and "there was
//! nothing to do" is the difference between a fixable mistake and a silent
//! one.
//!
//! ## Two functions, kept apart on purpose
//!
//! `condition_holds` is pure — it takes the facts already read, never a
//! `Path` — so Task 5 can call it once per conditional component in a recipe
//! without re-reading the ROM file each time. `rom_facts` is the one place
//! that touches disk, called once per install plan.

use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::package::Package;
use super::scan::{
    find_media, find_packages, media_for, open_media, open_package, package_for, MediaMatch,
    PackageMedium,
};
use super::source::MediaSource;
use super::{Component, Condition, Recipe, RefusalReason, RuleKind};
use crate::core::error::{CoreError, CoreResult};

/// What a planning decision needs to know about the paired Kickstart.
///
/// Only the major: every `Condition` variant so far (`RomOlderThan`) tests
/// the major alone, and `stated_version`'s minor is trivially available
/// later — by widening this struct — should a future condition ever need
/// finer granularity than "3.1 vs 3.2". Carrying it now, unread by anything,
/// would be a guess about a need that does not exist yet, which is exactly
/// what this module's own `RomOlderThan` rule is built to avoid making
/// about the ROM itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RomFacts {
    pub major: u16,
    /// Kept whole so `plan()` can record the pairing without reading the file
    /// twice — and so a future condition can ask about the machine.
    pub info: crate::core::rom::RomInfo,
}

/// Read the paired Kickstart's own stated major.
///
/// Goes through `core::rom::identify_rom`, which decodes a licensed Amiga
/// Forever ROM with the `rom.key` beside it (ART-128) instead of describing
/// its ciphertext — the previous version stripped the header and read the
/// encrypted bytes, so a licensed ROM refused the whole plan.
pub fn rom_facts(rom: &Path) -> CoreResult<RomFacts> {
    let info = crate::core::rom::identify_rom(rom)?;
    let bytes = crate::core::rom::decoded_image(rom)?;
    let (major, _minor) = crate::core::rom::stated_version(&bytes).ok_or_else(|| {
        CoreError::InvalidInput("this file does not state a Kickstart version".into())
    })?;
    Ok(RomFacts { major, info })
}

/// Whether a conditional component switches on, given the facts already
/// read about the paired ROM — `None` when the ROM could not be identified
/// at all, which refuses rather than guessing (see the module doc comment).
pub fn condition_holds(
    condition: &Condition,
    rom: Option<&RomFacts>,
) -> Result<bool, RefusalReason> {
    let rom = rom.ok_or(RefusalReason::RomUnknown)?;
    match condition {
        Condition::RomOlderThan { major } => Ok(rom.major < *major),
    }
}

/// One switched-on component's own contribution to `S:User-Startup` —
/// resolved here, at plan time, from the recipe's own [`super::Component::user_startup`],
/// for the same reason [`InstallPlan::components_on`] is resolved here
/// rather than left for `apply` to look up again: `apply` (`startup.rs`,
/// Task 7) only ever consumes an [`InstallPlan`], never the [`Recipe`]
/// itself, so whatever it needs to compose the file has to travel on the
/// plan. Carrying it also means a preview screen can show what would be
/// added before anything is written.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserStartupContribution {
    pub component: String,
    pub lines: Vec<String>,
}

/// One file or directory `apply` (a later task) will place in the
/// distribution tree.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanItem {
    pub component: String,
    /// The volume name the bytes came from — not the image's own filename.
    pub media: String,
    /// Where it lives on the media, `/`-separated, relative to the media's
    /// own root.
    pub from: String,
    /// Where it goes in the tree, `/`-separated.
    pub to: String,
    pub is_dir: bool,
    pub bytes: u64,
}

/// What [`plan`] produces: either a full description of what would be
/// written, or every reason it cannot proceed — never both. Any refusal at
/// all empties `items` and `media_paths` (see the module doc comment); the
/// UI can always tell the two cases apart by checking `refusals.is_empty()`.
///
/// `Deserialize` too, not just `Serialize` (Task 12): `commands::osinstall::osinstall_apply`
/// takes the plan the screen was shown rather than recomputing it — the same
/// rule `LayoutPlan` already follows for `layout_apply` — so this has to
/// cross the wire in both directions.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallPlan {
    pub release: String,
    pub items: Vec<PlanItem>,
    pub refusals: Vec<RefusalReason>,
    /// The sum of `items[].bytes` — `0` whenever `items` is empty, which is
    /// always true together, never computed separately.
    pub total_bytes: u64,
    /// Every component id that is switched on — required, explicitly
    /// chosen, or turned on by its own [`Condition`] — regardless of
    /// whether its media could actually be found. Populated even when the
    /// plan as a whole refuses, so the UI can explain *why* a refusal names
    /// a component the user never picked.
    pub components_on: Vec<String>,
    /// The Kickstart this plan was made against, and what the resulting tree
    /// needs of a future one (G9). `None` when no ROM was supplied.
    ///
    /// `#[serde(default)]` — `InstallPlan` derives `Deserialize` and is
    /// round-tripped through the wire (`osinstall_apply` takes back the plan
    /// `osinstall_plan` returned), so a plan value serialised before this
    /// field existed must still deserialise instead of refusing to load.
    #[serde(default)]
    pub paired_rom: Option<super::PairedRom>,
    /// Volume name -> the image it was found in. Resolved here so `apply`
    /// (a later task) can reopen the media without re-scanning the folder —
    /// and so the plan that was previewed is the plan that runs, even if
    /// the folder changed underneath it.
    pub media_paths: BTreeMap<String, PathBuf>,
    /// The chosen packages, in [`super::package::order`]'s order — the
    /// order `apply` places them in, which is not the order the user ticked
    /// the boxes in. Empty when none were asked for.
    ///
    /// Populated even when the plan as a whole refuses, the same rule
    /// [`InstallPlan::components_on`] follows and for the same reason: a
    /// refusal naming a package reads better beside the list it came from.
    /// `items` and `package_media` are the fields that go empty.
    ///
    /// `#[serde(default)]` for the same reason `paired_rom` carries one: an
    /// `InstallPlan` round-trips through the wire, and a plan serialised
    /// before this field existed must still deserialise.
    #[serde(default)]
    pub packages: Vec<String>,
    /// Package media name -> the archive it was found in, and the member
    /// inside it that holds the payload. The package half of
    /// [`InstallPlan::media_paths`], kept separate rather than folded into
    /// it because the two are opened by different readers and answered by
    /// different scans — see [`PackageMedium`]'s own doc comment for why a
    /// package is not a third `scan::MediaKind`.
    #[serde(default)]
    pub package_media: BTreeMap<String, PackageMedium>,
    /// Every member of `components_on` that carries its own `S:User-Startup`
    /// lines, in the same recipe order `components_on` itself is built in —
    /// which is also the order `apply` folds them into the file. Populated
    /// alongside `components_on`, for the same reason and regardless of
    /// whether the plan as a whole refuses: every shipped component today
    /// carries no lines at all (see `mod.rs`'s fixtures comment), so this is
    /// empty in practice until a future component uses the field.
    pub user_startup: Vec<UserStartupContribution>,
}

/// What the user asked for.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallRequest {
    pub media_folder: PathBuf,
    /// The paired Kickstart, if the user supplied one. `None` refuses any
    /// component whose [`Condition`] needs it to be decided — see
    /// [`condition_holds`].
    pub rom: Option<PathBuf>,
    /// Component ids the user picked. `required` components and
    /// condition-satisfied ones are added on top of this, not instead of
    /// it — see [`InstallPlan::components_on`].
    pub chosen: Vec<String>,
    /// Condition-satisfied component ids the user has explicitly turned off,
    /// despite the condition holding (spec requirement 2 — "turning Modules
    /// off is a confirmation, not a refusal", never `required`'s to give).
    /// Subtracted inside [`resolve_components_on`], **before** anything else
    /// in `plan()` looks at `components_on` — media resolution, collision
    /// detection and `media_paths` all skip an excluded component entirely,
    /// so its media is never opened, never recorded in `media_paths` (and so
    /// never in `apply()`'s manifest `built_from`), and never a source of a
    /// `MediaMissing`/`MediaAmbiguous` refusal. A component the caller both
    /// `chose` and `excluded` is off — exclusion always wins over `chosen`,
    /// the same way a satisfied `Condition` always wins over neither; only
    /// `required` cannot be excluded.
    pub excluded: Vec<String>,
    /// Package ids the user picked, in whatever order the boxes were
    /// ticked. Reordered by [`super::package::order`] before anything is
    /// placed. `#[serde(default)]` so a caller that knows nothing about
    /// packages (`src/lib/osinstall.ts` today) keeps working unchanged.
    #[serde(default)]
    pub packages: Vec<String>,
    /// Where the package archives are. **A second folder, not the media
    /// folder**: the owner keeps discs in `Amigatolon\iso` and archives in
    /// `Amigatolon\paketler`, so one path cannot answer both. `None` means
    /// no packages were asked for; naming packages without it is
    /// [`RefusalReason::PackageFolderMissing`], never an empty result.
    #[serde(default)]
    pub package_folder: Option<PathBuf>,
    pub destination: PathBuf,
    /// Which shipped recipe to plan from. Named by the release string, not by
    /// an index — a numbered choice would silently mean a different operating
    /// system the moment a recipe is added between two others.
    pub release: String,
}

/// `entry_path`, relative to `from` rather than to the media root. `from`
/// itself maps to `""`, matching what a [`RuleKind::Subtree`] rule's own
/// root entry resolves to — so a rule's own directory lands at `to` and
/// everything under it lands at `to/…`.
///
/// **The strip is case-insensitive, because resolution is.** A
/// `MediaSource` resolves a recipe's `from` against the media
/// case-insensitively (AmigaDOS is, ART-012) but answers with paths in the
/// *media's* own casing: `CdSource::walk("STORAGE")` on a Joliet-pressed
/// disc yields `Storage/Aux`. A plain `strip_prefix` then fails to match
/// its own rule's `from`, and the `None` arm below hands the whole
/// media-rooted path to `destination_for` — so `to: "C"` over a walk of
/// `OS-VERSION3.9/WORKBENCH3.5/C` builds
/// `C/OS-Version3.9/Workbench3.5/C/List`: the system tree nested three
/// levels under itself, silently, with no refusal. Resolving one way and
/// stripping another is what made that possible; both are ASCII
/// case-insensitive now, matching `CdSource::find_by_path`.
///
/// The `None` arm is kept for a prefix that genuinely does not match — it
/// cannot be reached from a `walk` result, whose every path starts with the
/// resolved `from` by construction, and a caller passing something else is
/// better served by an unchanged path than by a panic.
fn relative_to(entry_path: &str, from: &str) -> String {
    if from.is_empty() {
        return entry_path.to_string();
    }
    // `get`, not a slice: `from.len()` may land mid-character in a
    // multi-byte path, which indexing would panic on and which cannot be a
    // case-insensitive match anyway.
    match entry_path.get(..from.len()) {
        Some(head) if head.eq_ignore_ascii_case(from) => {
            let rest = &entry_path[from.len()..];
            rest.strip_prefix('/').unwrap_or(rest).to_string()
        }
        _ => entry_path.to_string(),
    }
}

/// Where `relative` (already relative to a rule's `from`) lands under the
/// rule's `to`.
fn destination_for(to: &str, relative: &str) -> String {
    if relative.is_empty() {
        to.to_string()
    } else if to.is_empty() {
        relative.to_string()
    } else {
        format!("{to}/{relative}")
    }
}

/// Which components are switched on: `required`, explicitly `chosen`, or
/// turned on by a satisfied [`Condition`] — the user is never asked about
/// the last kind (see the doc comment on [`Condition`] itself). Every
/// conditional component in the recipe is decided, not only the ones the
/// user happened to pick, which is the point of a condition existing at
/// all. An unsatisfiable condition (`Err(RefusalReason::RomUnknown)`) is
/// reported at most once, however many conditional components share the
/// same unreadable ROM, so the refusal list names one problem instead of
/// repeating it.
///
/// `excluded` is subtracted **before** a conditional component's own
/// [`Condition`] is even evaluated, not just before it is added to the
/// result: a component the caller excluded is off regardless of `chosen` or
/// the condition, and skipping the condition check for it means an excluded
/// component's own unreadable ROM cannot report [`RefusalReason::RomUnknown`]
/// for a component the caller does not want anyway (requirement 2's "turning
/// Modules off is a confirmation, not a refusal" would otherwise still be a
/// refusal — just a politer one — the moment the paired ROM could not be
/// identified). `required` cannot be excluded; the check below is only
/// reached for a component that is not.
fn resolve_components_on(
    recipe: &Recipe,
    chosen: &[String],
    excluded: &[String],
    rom_facts: Option<&RomFacts>,
    refusals: &mut Vec<RefusalReason>,
) -> Vec<String> {
    let chosen: HashSet<&str> = chosen.iter().map(String::as_str).collect();
    let excluded: HashSet<&str> = excluded.iter().map(String::as_str).collect();
    let mut rom_unknown_reported = refusals.contains(&RefusalReason::RomUnknown);
    let mut on = Vec::new();

    for component in &recipe.components {
        if !component.required && excluded.contains(component.id.as_str()) {
            continue;
        }
        let mut is_on = component.required || chosen.contains(component.id.as_str());
        if let Some(condition) = &component.condition {
            match condition_holds(condition, rom_facts) {
                Ok(true) => is_on = true,
                Ok(false) => {}
                Err(reason) => {
                    if !rom_unknown_reported {
                        refusals.push(reason);
                        rom_unknown_reported = true;
                    }
                }
            }
        }
        if is_on {
            on.push(component.id.clone());
        }
    }
    on
}

/// Every `exclusive_group` with more than one of its members in the
/// **resolved** `components_on` set. Checked against what actually
/// resolved on, never against `InstallRequest::chosen` directly — a
/// condition-satisfied component can be switched on without being chosen
/// at all (that is the entire point of
/// `a_conditional_component_is_on_without_being_chosen`), so a check
/// against the request alone would miss exactly the case a condition
/// exists to create. One member of a group is the ordinary case and needs
/// no mention here; a group is inert until a second member exists to
/// conflict with the first (Task 1's review parked this for the same
/// reason — with one Modules disk shipped, the field could not be
/// violated).
fn detect_exclusive_group_conflicts(
    recipe: &Recipe,
    components_on: &[String],
) -> Vec<RefusalReason> {
    let mut by_group: BTreeMap<&str, Vec<String>> = BTreeMap::new();
    for id in components_on {
        let Some(component) = recipe.component(id) else {
            continue;
        };
        if let Some(group) = &component.exclusive_group {
            by_group.entry(group.as_str()).or_default().push(id.clone());
        }
    }

    let mut refusals = Vec::new();
    for (group, mut components) in by_group {
        if components.len() > 1 {
            components.sort();
            refusals.push(RefusalReason::ExclusiveGroupConflict {
                group: group.to_string(),
                components,
            });
        }
    }
    refusals
}

/// Every destination that two or more components write a **file** to
/// without one declaring an `overrides` entry that covers all the others.
/// `Subtree` destinations coinciding is not checked — see the module doc
/// comment on why that is a merge point, not a claim; this is the same rule
/// `recipe.rs`'s own
/// `no_two_components_claim_one_destination_without_declaring_it` applies
/// to the rules themselves, applied here to the walked-out file list
/// `plan` actually produces.
///
/// `components` is a lookup over **every** component that could have
/// claimed a destination in `items` — the recipe's own, plus the chosen
/// packages' (a package is one [`Component`] under the package's own id).
/// Resolving `overrides` against the recipe alone was correct while only a
/// recipe could produce items; with packages in the same list it would
/// report every BoingBag file as an undeclared collision with
/// `workbench-base`, since `recipe.component("boingbag-39-1")` is `None` and
/// a claimant that cannot be resolved can never be the winner. This is the
/// same boundary `recipe.rs`'s `all_shipped_component_ids` had to cross for
/// the static half of the identical check.
fn detect_collisions(
    items: &[PlanItem],
    components: &BTreeMap<&str, &Component>,
) -> Vec<RefusalReason> {
    let mut claimants: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for item in items.iter().filter(|item| !item.is_dir) {
        let claiming = claimants.entry(item.to.clone()).or_default();
        if !claiming.contains(&item.component) {
            claiming.push(item.component.clone());
        }
    }

    let mut refusals = Vec::new();
    for (path, claiming) in &claimants {
        if claiming.len() < 2 {
            continue;
        }

        let resolved = claiming.iter().any(|winner| {
            let Some(winner_component) = components.get(winner.as_str()) else {
                return false;
            };
            claiming
                .iter()
                .filter(|other| *other != winner)
                .all(|other| winner_component.overrides.contains(other))
        });

        if !resolved {
            let mut components = claiming.clone();
            components.sort();
            refusals.push(RefusalReason::DestinationCollision {
                path: path.clone(),
                components,
            });
        }
    }
    refusals
}

/// Expand one component's rules against its already-opened medium into the
/// items that component contributes, collecting a typed refusal for every
/// rule that does not resolve.
///
/// Extracted from [`plan`]'s own loop (Task 6) because a package is placed
/// through exactly this expansion — a package *is* one [`Component`] over
/// one medium — and because `apply::add_package` has to reproduce it
/// file-for-file on a tree that already exists. Two copies of "what does
/// this rule turn into" is precisely how the two entry points this round
/// adds would start disagreeing about what a package puts on a volume.
///
/// `component.media` is the volume name every emitted [`PlanItem`] carries,
/// so a package's own `media` (the archive's single top-level directory)
/// travels the same way a floppy's volume name does — `package::RawPackage`
/// sets `component.media` to exactly that.
pub(super) fn expand_rules(
    component: &Component,
    source: &mut dyn MediaSource,
    refusals: &mut Vec<RefusalReason>,
) -> CoreResult<Vec<PlanItem>> {
    let mut items = Vec::new();

    for rule in &component.rules {
        let Some(entry) = source.entry(&rule.from)? else {
            // The media is here and the path the recipe expects is not
            // — a refusal, not a skip (see the module doc comment and
            // the `RefusalReason::MediaPathMissing` doc comment).
            refusals.push(RefusalReason::MediaPathMissing {
                component: component.id.clone(),
                media: component.media.clone(),
                path: rule.from.clone(),
            });
            continue;
        };

        match rule.kind {
            RuleKind::File if entry.is_dir => {
                // A `File` rule resolving to a directory: emitting it
                // anyway would carry `is_dir: true`, which
                // `detect_collisions` filters out entirely (it only
                // looks at files) — a wrong recipe would silently
                // escape the one check meant to catch it. Refused by
                // name instead; see the `RuleKindMismatch` doc comment.
                refusals.push(RefusalReason::RuleKindMismatch {
                    component: component.id.clone(),
                    from: rule.from.clone(),
                    expected: RuleKind::File,
                    found: RuleKind::Subtree,
                });
            }
            RuleKind::File => {
                items.push(PlanItem {
                    component: component.id.clone(),
                    media: component.media.clone(),
                    from: rule.from.clone(),
                    to: rule.to.clone(),
                    is_dir: false,
                    bytes: entry.size,
                });
            }
            RuleKind::Subtree if !entry.is_dir => {
                // A `Subtree` rule resolving to a file: `source.walk`
                // refuses this itself — the trait says so and both
                // implementations now do it — but the wrong-shape rule
                // deserves a typed refusal naming the component and the
                // rule, not the bare `CoreError` `walk` would raise on
                // the way there.
                refusals.push(RefusalReason::RuleKindMismatch {
                    component: component.id.clone(),
                    from: rule.from.clone(),
                    expected: RuleKind::Subtree,
                    found: RuleKind::File,
                });
            }
            RuleKind::Subtree => {
                // The subtree's own root, so an empty drawer still gets
                // created — `walk` yields only what is *inside* `from`,
                // never `from` itself.
                items.push(PlanItem {
                    component: component.id.clone(),
                    media: component.media.clone(),
                    from: rule.from.clone(),
                    to: rule.to.clone(),
                    is_dir: true,
                    bytes: 0,
                });
                for walked in source.walk(&rule.from)? {
                    let relative = relative_to(&walked.path, &rule.from);
                    items.push(PlanItem {
                        component: component.id.clone(),
                        media: component.media.clone(),
                        from: walked.path.clone(),
                        to: destination_for(&rule.to, &relative),
                        is_dir: walked.is_dir,
                        bytes: walked.size,
                    });
                }
            }
        }
    }

    Ok(items)
}

/// Every reason a chosen package set cannot be applied that is about the
/// *selection* rather than about the folder it would be read from — an id
/// ART ships nothing for, a `requires` that was not itself chosen, a
/// `requires_components` that is not switched on.
///
/// Typed, and computed here rather than read out of
/// [`super::package::order`]'s own `Err`: `order` answers in English
/// sentences (ART-060) and stops at the first problem, and this module's
/// own rule is that every refusal reaches the screen at once. `order` is
/// still what decides the *order*; this decides what is refusable.
fn detect_package_refusals(
    chosen: &[String],
    all: &[Package],
    components_on: &[String],
) -> Vec<RefusalReason> {
    let mut refusals = Vec::new();
    let chosen_set: HashSet<&str> = chosen.iter().map(String::as_str).collect();

    for id in chosen {
        let Some(package) = all.iter().find(|p| &p.id == id) else {
            refusals.push(RefusalReason::PackageUnknown {
                package: id.clone(),
            });
            continue;
        };
        for need in &package.requires {
            if !chosen_set.contains(need.as_str()) {
                refusals.push(RefusalReason::PackageRequirementMissing {
                    package: id.clone(),
                    requires: need.clone(),
                });
            }
        }
        // Against the **resolved** set, never `InstallRequest::chosen` — a
        // component can be switched on by `required` or by its own
        // `Condition` without ever being chosen, the same reasoning
        // `detect_exclusive_group_conflicts` states for itself.
        for need in &package.requires_components {
            if !components_on.iter().any(|on| on == need) {
                refusals.push(RefusalReason::PackageComponentMissing {
                    package: id.clone(),
                    component: need.clone(),
                });
            }
        }
    }

    refusals
}

/// Turn a recipe, a media folder and (optionally) a ROM into a description
/// of what would be written — or into every reason it cannot proceed.
///
/// Order: read the ROM once, decide which components are on, check that
/// resolved set for `exclusive_group` conflicts, resolve each on
/// component's media by volume name, resolve every one of its rules against
/// that media — refusing a `RuleKindMismatch` rather than emitting an item
/// of the wrong shape — and expand every rule that does match with
/// [`MediaSource::walk`]. Then check the whole walked-out item list for
/// file-level collisions, and sum. See the module doc comment for why
/// refusals never stop the walk and why any refusal empties `items`.
pub fn plan(request: &InstallRequest, recipe: &Recipe) -> CoreResult<InstallPlan> {
    plan_over(request, recipe, &super::package::packages()?)
}

/// [`plan`]'s own body, parameterised over the package catalogue — so a
/// test can plan against a small, hand-built [`Package`] set instead of the
/// three shipped ones, which is the only way the two entry points this
/// round adds can be compared over a package built to overwrite a known
/// base file. Exactly the reason [`super::package::order_over`] and
/// `package::parse_all` are parameterised, applied one level up; [`plan`]
/// is the thin wrapper that passes the real catalogue.
pub(super) fn plan_over(
    request: &InstallRequest,
    recipe: &Recipe,
    catalogue: &[Package],
) -> CoreResult<InstallPlan> {
    let mut refusals: Vec<RefusalReason> = Vec::new();

    let rom_facts = match &request.rom {
        Some(path) => match rom_facts(path) {
            Ok(facts) => Some(facts),
            Err(_) => {
                // A ROM path was given and it could not be identified — a
                // typed refusal, never the `CoreError` sentence `rom_facts`
                // itself raises (ART-060; the join Task 4's review carried
                // into this task by name).
                refusals.push(RefusalReason::RomUnknown);
                None
            }
        },
        None => None,
    };

    let components_on = resolve_components_on(
        recipe,
        &request.chosen,
        &request.excluded,
        rom_facts.as_ref(),
        &mut refusals,
    );
    refusals.extend(detect_exclusive_group_conflicts(recipe, &components_on));

    let found = find_media(&request.media_folder)?;
    let mut media_paths: BTreeMap<String, PathBuf> = BTreeMap::new();
    let mut items: Vec<PlanItem> = Vec::new();

    for component_id in &components_on {
        // `components_on` only ever names ids `resolve_components_on` read
        // from this same `recipe`, so this is provably unreachable — but
        // the release profile's `panic = "abort"` turns an `expect` here
        // into the whole application going down over a case that cannot
        // actually happen, which is a worse failure than silently skipping
        // an id that isn't there. See CLAUDE.md's bounds-checking rule.
        let Some(component) = recipe.component(component_id) else {
            continue;
        };

        // Never `if let MediaMatch::Found(..)` — see the module doc comment.
        let found_media = match media_for(&found, &component.media) {
            MediaMatch::Missing => {
                refusals.push(RefusalReason::MediaMissing {
                    component: component.id.clone(),
                    volume_name: component.media.clone(),
                });
                continue;
            }
            MediaMatch::Ambiguous(matches) => {
                refusals.push(RefusalReason::MediaAmbiguous {
                    component: component.id.clone(),
                    volume_name: component.media.clone(),
                    paths: matches
                        .iter()
                        .map(|m| m.path.display().to_string())
                        .collect(),
                });
                continue;
            }
            MediaMatch::Found(found_media) => found_media,
        };
        let media_path = found_media.path.clone();
        media_paths.insert(component.media.clone(), media_path.clone());

        let mut source = open_media(found_media)?;
        items.extend(expand_rules(component, source.as_mut(), &mut refusals)?);
    }

    // ---- packages, after the release's own components -------------------
    //
    // After, deliberately: an update package exists to land on top of what
    // the release put down, so its items have to be placed later for the
    // last-writer-wins rule `apply` already applies to be the right way
    // round. `order()` decides the order among them (BoingBag 3.9-2 after
    // 3.9-1 whatever order the boxes were ticked in).
    let mut packages: Vec<String> = Vec::new();
    let mut package_media: BTreeMap<String, PackageMedium> = BTreeMap::new();
    let mut chosen_packages: Vec<&Package> = Vec::new();

    if !request.packages.is_empty() {
        refusals.extend(detect_package_refusals(
            &request.packages,
            catalogue,
            &components_on,
        ));

        match &request.package_folder {
            None => refusals.push(RefusalReason::PackageFolderMissing {
                packages: request.packages.clone(),
            }),
            Some(folder) if refusals.is_empty() => {
                // Only once the selection itself is sound: `order` refuses
                // an unsatisfied `requires` with an English sentence, and
                // `detect_package_refusals` has already named that case
                // properly above. What can still come back `Err` here is a
                // duplicate id or a cycle in the shipped data — neither a
                // user situation, both a bug in whoever built the request or
                // in the shipped JSON, so both stay hard errors.
                packages = super::package::order_over(&request.packages, catalogue)?;
                let found = find_packages(folder)?;

                for id in &packages {
                    let package = catalogue
                        .iter()
                        .find(|p| &p.id == id)
                        .expect("detect_package_refusals refused every unknown id above");

                    // Never `if let MediaMatch::Found(..)` — see the module
                    // doc comment; the rule is the enum's, not `FoundMedia`'s.
                    let archive = match package_for(&found, &package.media) {
                        MediaMatch::Missing => {
                            refusals.push(RefusalReason::PackageArchiveMissing {
                                package: package.id.clone(),
                                media: package.media.clone(),
                            });
                            continue;
                        }
                        MediaMatch::Ambiguous(matches) => {
                            refusals.push(RefusalReason::PackageArchiveAmbiguous {
                                package: package.id.clone(),
                                media: package.media.clone(),
                                paths: matches
                                    .iter()
                                    .map(|m| m.path.display().to_string())
                                    .collect(),
                            });
                            continue;
                        }
                        MediaMatch::Found(archive) => archive,
                    };

                    let medium = PackageMedium {
                        path: archive.path.clone(),
                        member: package.member.clone(),
                    };
                    let mut source = open_package(&medium)?;
                    items.extend(expand_rules(
                        &package.component,
                        source.as_mut(),
                        &mut refusals,
                    )?);
                    package_media.insert(package.media.clone(), medium);
                    chosen_packages.push(package);
                }
            }
            // The selection already refused above; resolving archives for a
            // set ART has already said it cannot apply would only add noise
            // about files nobody is going to read.
            Some(_) => {}
        }
    }

    // Every component that could have claimed a destination in `items` —
    // see `detect_collisions`'s own doc comment for why a package's has to
    // be in here.
    let mut claimable: BTreeMap<&str, &Component> = recipe
        .components
        .iter()
        .map(|component| (component.id.as_str(), component))
        .collect();
    for package in &chosen_packages {
        claimable.insert(package.id.as_str(), &package.component);
    }
    refusals.extend(detect_collisions(&items, &claimable));

    // Same source as `components_on` itself (`recipe.component`), and in
    // the same order — resolved regardless of whether the plan as a whole
    // refuses, matching `components_on`'s own rule: a component that never
    // resolved still explains itself, and this costs nothing to compute
    // when it does.
    let user_startup = components_on
        .iter()
        .filter_map(|id| recipe.component(id))
        .filter(|component| !component.user_startup.is_empty())
        .map(|component| UserStartupContribution {
            component: component.id.clone(),
            lines: component.user_startup.clone(),
        })
        .collect();

    let (items, media_paths, package_media) = if refusals.is_empty() {
        (items, media_paths, package_media)
    } else {
        // Any refusal at all empties everything a preview would act on —
        // see the module doc comment. `package_media` follows `media_paths`
        // for the same reason: half a package's files, with none of the ones
        // that could not be resolved, is not something to preview either.
        (Vec::new(), BTreeMap::new(), BTreeMap::new())
    };
    let total_bytes = items.iter().map(|item| item.bytes).sum();

    let paired_rom = rom_facts.map(|facts| super::PairedRom {
        name: facts.info.name.clone(),
        sha256: facts.info.sha256.clone(),
        stated_major: Some(facts.major),
        compatible_models: facts.info.compatible_models.clone(),
        requires_major: rom_requirement(recipe, &components_on),
    });

    Ok(InstallPlan {
        release: recipe.release.clone(),
        items,
        refusals,
        total_bytes,
        components_on,
        paired_rom,
        media_paths,
        packages,
        package_media,
        user_startup,
    })
}

/// What a tree with these components needs of a future ROM.
///
/// A component with a `RomOlderThan` condition that is **off** is one whose
/// modules are absent, so the tree needs a ROM the condition would not have
/// fired for: at least `major`. A component that is *on* brought its modules
/// with it and needs nothing.
fn rom_requirement(recipe: &Recipe, components_on: &[String]) -> Option<u16> {
    recipe
        .components
        .iter()
        .filter_map(|component| {
            component
                .condition
                .map(|Condition::RomOlderThan { major }| (component.id.as_str(), major))
        })
        .filter(|(id, _)| !components_on.iter().any(|on| on == id))
        .map(|(_, major)| major)
        .max()
}

#[cfg(test)]
mod condition_tests {
    use super::*;

    /// `RomFacts` now carries the whole `RomInfo` (G9), which
    /// `condition_holds` never reads — only `major` matters to a
    /// `Condition`. This fills the rest with placeholder values so a test
    /// about the condition does not have to state facts it does not use.
    fn fake_rom_facts(major: u16) -> RomFacts {
        RomFacts {
            major,
            info: crate::core::rom::RomInfo {
                name: "Test ROM".to_string(),
                version: "Custom".to_string(),
                revision: String::new(),
                size_bytes: 0,
                sha256: String::new(),
                crc32: String::new(),
                is_cloanto: false,
                key_available: false,
                is_aros: false,
                checksum: crate::core::rom::RomChecksum::NotChecked,
                compatible_models: Vec::new(),
                file_path: String::new(),
                major: Some(major),
            },
        }
    }

    /// `Workbench3.2.adf:S/Startup-sequence` opens with
    /// `Version exec.library version 47 … If Warn … Quit`. So a 3.2 system on a
    /// 3.1 ROM without `LIBS:Modules` does not boot at all.
    #[test]
    fn a_pre_v47_rom_turns_the_modules_component_on() {
        let facts = fake_rom_facts(40);
        let holds = condition_holds(&Condition::RomOlderThan { major: 47 }, Some(&facts));
        assert_eq!(holds, Ok(true));
    }

    #[test]
    fn a_v47_rom_leaves_it_off() {
        let facts = fake_rom_facts(47);
        let holds = condition_holds(&Condition::RomOlderThan { major: 47 }, Some(&facts));
        assert_eq!(holds, Ok(false));
    }

    /// Guessing costs 800 KB, or a system that quits at boot. Neither is ART's
    /// to choose.
    #[test]
    fn an_unidentified_rom_refuses_rather_than_guessing() {
        let holds = condition_holds(&Condition::RomOlderThan { major: 47 }, None);
        assert_eq!(holds, Err(RefusalReason::RomUnknown));
    }

    /// The ROM's own header, not `KNOWN_ROMS` — the user's licensed A1200 dump
    /// is not in that table (ART-104) and is still a perfectly good 3.1 ROM.
    ///
    /// The brief's own version of this test used `tempfile::tempdir()`, but
    /// this project deliberately does not depend on `tempfile` (see
    /// `fixtures::scratch`'s doc comment) — `scratch` is the repository's
    /// existing way to get a private directory for one test.
    #[test]
    fn the_major_comes_from_the_roms_own_header() {
        let dir = super::super::fixtures::scratch("plan-rom-header");
        let path = dir.join("fake.rom");
        let mut bytes = vec![0u8; 512 * 1024];
        bytes[12..14].copy_from_slice(&40u16.to_be_bytes());
        bytes[14..16].copy_from_slice(&68u16.to_be_bytes());
        std::fs::write(&path, &bytes).unwrap();

        assert_eq!(rom_facts(&path).unwrap().major, 40);
    }

    /// **Superseded by ART-128 (G9).** This test used to prove `rom_facts`
    /// read a Cloanto-headered dump's stated major by stripping the header
    /// alone — but that is exactly the bug ART-128 fixed: a real Amiga
    /// Forever dump is XOR-encoded behind that header, so reading the bytes
    /// straight after it (as this test's own fixture did, with no XOR
    /// applied) never matched what a real licensed ROM looks like. Now that
    /// `rom_facts` goes through `core::rom::decoded_image`, a Cloanto header
    /// with no `rom.key` beside it is refused rather than misread — which is
    /// the correct behaviour, and what this proves instead.
    /// `a_licensed_rom_with_its_key_beside_it_states_its_version` (below, in
    /// `plan_tests`) is this test's replacement for the case with a key
    /// actually present.
    #[test]
    fn a_cloanto_header_with_no_key_beside_it_is_refused_not_misread() {
        let dir = super::super::fixtures::scratch("plan-rom-cloanto-no-key");
        let path = dir.join("cloanto.rom");

        let mut bytes = b"AMIROMTYPE1".to_vec();
        let mut body = vec![0u8; 512 * 1024];
        body[12..14].copy_from_slice(&40u16.to_be_bytes());
        body[14..16].copy_from_slice(&68u16.to_be_bytes());
        bytes.extend_from_slice(&body);
        std::fs::write(&path, &bytes).unwrap();

        assert!(rom_facts(&path).is_err());
    }

    /// A file that exists and reads fine but is not a ROM at all — plain
    /// text, far too short to carry a version field. This is the case
    /// `an_unreadable_rom_is_a_core_error_not_a_panic` (a *missing* file)
    /// could not pin: that test's `is_err()` would pass for an I/O failure
    /// just as readily as for a content problem, so it never proved
    /// `rom_facts` actually rejects bad content rather than merely
    /// propagating `std::fs::read`'s own error. This one names the exact
    /// variant.
    #[test]
    fn content_that_is_not_a_rom_is_refused_as_invalid_input() {
        let dir = super::super::fixtures::scratch("plan-rom-not-a-rom");
        let path = dir.join("readme.txt");
        std::fs::write(&path, b"this is not a Kickstart image").unwrap();

        assert!(matches!(rom_facts(&path), Err(CoreError::InvalidInput(_))));
    }
}

#[cfg(test)]
mod plan_tests {
    use super::*;
    use crate::core::osinstall::{Component, PathRule};

    /// The common case: media that matches the shipped recipe exactly, so a
    /// test only has to state which components are chosen and which disks
    /// are in the folder. Rom major `47` — V47 or later — keeps
    /// `modules-a1200`'s own condition off by default, so a test that is
    /// not about the ROM does not have to think about it.
    fn plan_with(chosen: &[&str], present: &[&str]) -> InstallPlan {
        crate::core::osinstall::fixtures::planned_with(chosen, present, Some(47)).0
    }

    /// The variant that *is* about the ROM. `Workbench3.2` alone is enough
    /// media for `workbench-base` (required) to resolve without noise; the
    /// point of this helper is `components_on`, not whether
    /// `modules-a1200`'s own media happens to be present too.
    fn plan_with_rom(chosen: &[&str], rom_major: u16) -> InstallPlan {
        crate::core::osinstall::fixtures::planned_with(chosen, &["Workbench3.2"], Some(rom_major)).0
    }

    /// `Workbench3.2` built completely valid — every one of `workbench-base`'s
    /// rules resolves, exactly as [`plan_with`] would build it. `Extras3.2`
    /// starts from the same recipe-derived content and then has its `L`
    /// entry removed — the one path this test means to break, and the only
    /// difference from an ordinary, fully-satisfied `Extras3.2`.
    fn plan_where_extras_has_no_l() -> InstallPlan {
        let dir = crate::core::osinstall::fixtures::scratch("plan-extras-no-l");
        let folder = dir.join("media");
        std::fs::create_dir(&folder).unwrap();
        let recipe = crate::core::osinstall::recipe::amigaos_32().unwrap();

        let wb = crate::core::osinstall::fixtures::entries_for(&recipe, "Workbench3.2");
        let wb_refs: Vec<(&str, &[u8], u32)> = wb
            .iter()
            .map(|(path, bytes, protection)| (path.as_str(), bytes.as_slice(), *protection))
            .collect();
        crate::core::osinstall::fixtures::media(&folder, "Workbench3.2", "wb.adf", &wb_refs);
        crate::core::osinstall::fixtures::required_media(&folder, &recipe, &["Workbench3.2"]);

        let extras: Vec<(String, Vec<u8>, u32)> =
            crate::core::osinstall::fixtures::entries_for(&recipe, "Extras3.2")
                .into_iter()
                .filter(|(path, _, _)| path != "L/placeholder")
                .collect();
        let extras_refs: Vec<(&str, &[u8], u32)> = extras
            .iter()
            .map(|(path, bytes, protection)| (path.as_str(), bytes.as_slice(), *protection))
            .collect();
        crate::core::osinstall::fixtures::media(&folder, "Extras3.2", "extras.adf", &extras_refs);

        let request = InstallRequest {
            packages: Vec::new(),
            package_folder: None,
            release: "AmigaOS 3.2".to_string(),
            media_folder: folder,
            rom: Some(crate::core::osinstall::fixtures::fake_rom(&dir, 47)),
            chosen: vec!["extras".to_string()],
            destination: dir.join("dist"),
            excluded: Vec::new(),
        };
        plan(&request, &recipe).unwrap()
    }

    /// Carried item 4, made falsifiable: the collision `recipe.rs`'s own
    /// static check cannot see at all, because it only exists once a
    /// `Subtree` rule has been walked into a real file. `subtree-owner`
    /// claims the whole `D` drawer (`Subtree "D" -> "D"`) from media that
    /// genuinely holds `D/x`; `file-writer` writes a `File` rule straight
    /// at `x -> D/x` from different media. Neither declares an `overrides`.
    /// `plan_with_colliding_recipe` (`File` vs `File`) is the shape
    /// `recipe.rs` already covers statically; this is the shape only
    /// `plan()` — after `walk` — can see.
    fn plan_with_a_walked_file_colliding_with_a_direct_file() -> InstallPlan {
        let dir = crate::core::osinstall::fixtures::scratch("plan-walked-collision");
        let folder = dir.join("media");
        std::fs::create_dir(&folder).unwrap();
        crate::core::osinstall::fixtures::media(
            &folder,
            "Owner",
            "owner.adf",
            &[("D/x", b"one", 0)],
        );
        crate::core::osinstall::fixtures::media(
            &folder,
            "Writer",
            "writer.adf",
            &[("x", b"two", 0)],
        );

        let recipe = Recipe {
            release: "Test".to_string(),
            components: vec![
                Component {
                    id: "subtree-owner".to_string(),
                    media: "Owner".to_string(),
                    rules: vec![PathRule {
                        from: "D".to_string(),
                        to: "D".to_string(),
                        kind: RuleKind::Subtree,
                    }],
                    required: false,
                    condition: None,
                    overrides: vec![],
                    user_startup: vec![],
                    exclusive_group: None,
                    available: true,
                },
                Component {
                    id: "file-writer".to_string(),
                    media: "Writer".to_string(),
                    rules: vec![PathRule {
                        from: "x".to_string(),
                        to: "D/x".to_string(),
                        kind: RuleKind::File,
                    }],
                    required: false,
                    condition: None,
                    overrides: vec![],
                    user_startup: vec![],
                    exclusive_group: None,
                    available: true,
                },
            ],
        };

        let request = InstallRequest {
            packages: Vec::new(),
            package_folder: None,
            release: "AmigaOS 3.2".to_string(),
            media_folder: folder,
            rom: None,
            chosen: vec!["subtree-owner".to_string(), "file-writer".to_string()],
            destination: dir.join("dist"),
            excluded: Vec::new(),
        };
        plan(&request, &recipe).unwrap()
    }

    /// A single-component recipe whose one rule declares `kind: "file"`
    /// against `C`, a path that is actually a directory on its own media
    /// (it holds `C/inner`). Neither the shipped recipe nor `recipe.rs`'s
    /// `validate` can produce or catch this — `validate` has no media to
    /// resolve `C` against, and a real Workbench disk's `C` genuinely is a
    /// directory. Only a hand-built, deliberately wrong recipe can put a
    /// `File` rule over a directory in front of `plan()`.
    fn plan_with_a_file_rule_over_a_directory() -> InstallPlan {
        let dir = crate::core::osinstall::fixtures::scratch("plan-kind-file-over-dir");
        let folder = dir.join("media");
        std::fs::create_dir(&folder).unwrap();
        crate::core::osinstall::fixtures::media(&folder, "M", "m.adf", &[("C/inner", b"x", 0)]);

        let recipe = Recipe {
            release: "Test".to_string(),
            components: vec![Component {
                id: "a".to_string(),
                media: "M".to_string(),
                rules: vec![PathRule {
                    from: "C".to_string(),
                    to: "C".to_string(),
                    kind: RuleKind::File,
                }],
                required: false,
                condition: None,
                overrides: vec![],
                user_startup: vec![],
                exclusive_group: None,
                available: true,
            }],
        };

        let request = InstallRequest {
            packages: Vec::new(),
            package_folder: None,
            release: "AmigaOS 3.2".to_string(),
            media_folder: folder,
            rom: None,
            chosen: vec!["a".to_string()],
            destination: dir.join("dist"),
            excluded: Vec::new(),
        };
        plan(&request, &recipe).unwrap()
    }

    /// The other direction: `kind: "subtree"` against `readme`, a path that
    /// is actually a plain file on its own media.
    fn plan_with_a_subtree_rule_over_a_file() -> InstallPlan {
        let dir = crate::core::osinstall::fixtures::scratch("plan-kind-subtree-over-file");
        let folder = dir.join("media");
        std::fs::create_dir(&folder).unwrap();
        crate::core::osinstall::fixtures::media(&folder, "M", "m.adf", &[("readme", b"x", 0)]);

        let recipe = Recipe {
            release: "Test".to_string(),
            components: vec![Component {
                id: "a".to_string(),
                media: "M".to_string(),
                rules: vec![PathRule {
                    from: "readme".to_string(),
                    to: "readme".to_string(),
                    kind: RuleKind::Subtree,
                }],
                required: false,
                condition: None,
                overrides: vec![],
                user_startup: vec![],
                exclusive_group: None,
                available: true,
            }],
        };

        let request = InstallRequest {
            packages: Vec::new(),
            package_folder: None,
            release: "AmigaOS 3.2".to_string(),
            media_folder: folder,
            rom: None,
            chosen: vec!["a".to_string()],
            destination: dir.join("dist"),
            excluded: Vec::new(),
        };
        plan(&request, &recipe).unwrap()
    }

    /// A recipe built by hand, not the shipped one: two components with no
    /// declared `overrides` both write `C/Assign`. The shipped recipe's own
    /// `no_two_components_claim_one_destination_without_declaring_it` test
    /// proves this shape never occurs in it, so the collision guard needs a
    /// fixture that manufactures the shape on purpose.
    fn plan_with_colliding_recipe() -> InstallPlan {
        let dir = crate::core::osinstall::fixtures::scratch("plan-collision");
        let folder = dir.join("media");
        std::fs::create_dir(&folder).unwrap();
        crate::core::osinstall::fixtures::media(&folder, "A", "a.adf", &[("C/Assign", b"one", 0)]);
        crate::core::osinstall::fixtures::media(&folder, "B", "b.adf", &[("C/Assign", b"two", 0)]);

        let make = |id: &str, media: &str| Component {
            id: id.to_string(),
            media: media.to_string(),
            rules: vec![PathRule {
                from: "C/Assign".to_string(),
                to: "C/Assign".to_string(),
                kind: RuleKind::File,
            }],
            required: false,
            condition: None,
            overrides: vec![],
            user_startup: vec![],
            exclusive_group: None,
            available: true,
        };
        let recipe = Recipe {
            release: "Test".to_string(),
            components: vec![make("a", "A"), make("b", "B")],
        };

        let request = InstallRequest {
            packages: Vec::new(),
            package_folder: None,
            release: "AmigaOS 3.2".to_string(),
            media_folder: folder,
            rom: None,
            chosen: vec!["a".to_string(), "b".to_string()],
            destination: dir.join("dist"),
            excluded: Vec::new(),
        };
        plan(&request, &recipe).unwrap()
    }

    /// A recipe built by hand: two components share one `exclusive_group`,
    /// with **different** destinations (`C/ModuleA` / `C/ModuleB`) so this
    /// exercises the group conflict alone, not `detect_collisions` too.
    /// `modules-b` is deliberately not in `chosen` at all — it switches on
    /// through its own `Condition` — so this is the shape the coordinator
    /// asked for: a conflict the *resolved* set holds that the *requested*
    /// set never would have shown.
    fn plan_with_exclusive_group_conflict() -> InstallPlan {
        let dir = crate::core::osinstall::fixtures::scratch("plan-exclusive-conflict");
        let folder = dir.join("media");
        std::fs::create_dir(&folder).unwrap();
        crate::core::osinstall::fixtures::media(
            &folder,
            "ModulesA",
            "a.adf",
            &[("C/LoadModule", b"x", 0)],
        );
        crate::core::osinstall::fixtures::media(
            &folder,
            "ModulesB",
            "b.adf",
            &[("C/LoadModule", b"x", 0)],
        );

        let make = |id: &str, media: &str, to: &str, condition: Option<Condition>| Component {
            id: id.to_string(),
            media: media.to_string(),
            rules: vec![PathRule {
                from: "C/LoadModule".to_string(),
                to: to.to_string(),
                kind: RuleKind::File,
            }],
            required: false,
            condition,
            overrides: vec![],
            user_startup: vec![],
            exclusive_group: Some("modules".to_string()),
            available: true,
        };
        let recipe = Recipe {
            release: "Test".to_string(),
            components: vec![
                make("modules-a", "ModulesA", "C/ModuleA", None),
                make(
                    "modules-b",
                    "ModulesB",
                    "C/ModuleB",
                    Some(Condition::RomOlderThan { major: 47 }),
                ),
            ],
        };

        let request = InstallRequest {
            packages: Vec::new(),
            package_folder: None,
            release: "AmigaOS 3.2".to_string(),
            media_folder: folder,
            // major 40 < 47, so `modules-b` switches on by its own
            // condition — never named in `chosen`.
            rom: Some(crate::core::osinstall::fixtures::fake_rom(&dir, 40)),
            chosen: vec!["modules-a".to_string()],
            destination: dir.join("dist"),
            excluded: Vec::new(),
        };
        plan(&request, &recipe).unwrap()
    }

    /// The planner must open a disc as a disc. Before this, `plan()`
    /// hardcoded `AdfSource::open` and a found ISO produced a hard error
    /// rather than a plan — discovery could see media the planner could not
    /// use.
    #[test]
    fn a_component_whose_media_is_a_disc_is_planned_from_the_disc() {
        use crate::core::iso::fixture::{file, IsoBuilder};

        let dir = crate::core::osinstall::fixtures::scratch("plan-disc-media");
        let folder = dir.join("media");
        std::fs::create_dir(&folder).unwrap();

        // The same fixture shape `source_cd.rs`'s own tests build — Joliet
        // on, so the volume and file names round-trip exactly as typed,
        // matching a real AmigaOS 3.9 disc.
        let bytes = IsoBuilder {
            volume: "AmigaOS3.9".to_string(),
            joliet_volume: "AmigaOS3.9".to_string(),
            joliet: true,
            children: vec![file("README.;1", "readme.txt", b"install disc")],
            ..Default::default()
        }
        .build();
        std::fs::write(folder.join("os39.iso"), bytes).unwrap();

        let recipe = Recipe {
            release: "Test".to_string(),
            components: vec![Component {
                id: "a".to_string(),
                media: "AmigaOS3.9".to_string(),
                rules: vec![PathRule {
                    from: "readme.txt".to_string(),
                    to: "readme.txt".to_string(),
                    kind: RuleKind::File,
                }],
                required: false,
                condition: None,
                overrides: vec![],
                user_startup: vec![],
                exclusive_group: None,
                available: true,
            }],
        };

        let request = InstallRequest {
            packages: Vec::new(),
            package_folder: None,
            release: "AmigaOS 3.2".to_string(),
            media_folder: folder,
            rom: None,
            chosen: vec!["a".to_string()],
            destination: dir.join("dist"),
            excluded: Vec::new(),
        };
        let plan = plan(&request, &recipe).unwrap();

        // Assert on the file, not merely on `is_ok()` — a plan that silently
        // contains nothing would pass an `is_ok()` test.
        assert!(plan.refusals.is_empty(), "{:?}", plan.refusals);
        assert!(
            plan.items
                .iter()
                .any(|item| item.to == "readme.txt" && !item.is_dir && item.bytes > 0),
            "{:?}",
            plan.items
        );
    }

    /// The Joliet case. A disc pressed with Joliet carries its names in
    /// their natural mixed case (`OS-Version3.9`), the shipped recipe spells
    /// its `from` in the uppercase the Primary tree uses, and resolution
    /// bridges the two case-insensitively — so `walk` answers in the
    /// **disc's** casing. `relative_to` used to strip `from` case-sensitively
    /// against that, fail, and fall through to the media-rooted path, so
    /// `to: "C"` landed the whole subtree at `C/OS-Version3.9/Workbench3.5/C/…`
    /// — nested under itself, with no refusal. The exact-case disc plan test
    /// above cannot see this; only a fixture whose casing differs from the
    /// rule's can.
    #[test]
    fn a_subtree_from_a_disc_cased_unlike_the_recipe_lands_at_to_not_under_itself() {
        use crate::core::iso::fixture::{dir, file, IsoBuilder};

        let scratch = crate::core::osinstall::fixtures::scratch("plan-disc-case");
        let folder = scratch.join("media");
        std::fs::create_dir(&folder).unwrap();

        // Joliet names in mixed case; the recipe below asks in uppercase.
        let bytes = IsoBuilder {
            volume: "AmigaOS3.9".to_string(),
            joliet_volume: "AmigaOS3.9".to_string(),
            joliet: true,
            children: vec![dir(
                "OS-VERSION3.9",
                "OS-Version3.9",
                vec![dir(
                    "WORKBENCH3.5",
                    "Workbench3.5",
                    vec![dir("C", "C", vec![file("LIST.;1", "List", b"list-bytes")])],
                )],
            )],
            ..Default::default()
        }
        .build();
        std::fs::write(folder.join("os39.iso"), bytes).unwrap();

        let recipe = Recipe {
            release: "Test".to_string(),
            components: vec![Component {
                id: "a".to_string(),
                media: "AmigaOS3.9".to_string(),
                rules: vec![PathRule {
                    from: "OS-VERSION3.9/WORKBENCH3.5/C".to_string(),
                    to: "C".to_string(),
                    kind: RuleKind::Subtree,
                }],
                required: false,
                condition: None,
                overrides: vec![],
                user_startup: vec![],
                exclusive_group: None,
                available: true,
            }],
        };

        let request = InstallRequest {
            packages: Vec::new(),
            package_folder: None,
            release: "AmigaOS 3.2".to_string(),
            media_folder: folder,
            rom: None,
            chosen: vec!["a".to_string()],
            destination: scratch.join("dist"),
            excluded: Vec::new(),
        };
        let plan = plan(&request, &recipe).unwrap();

        assert!(plan.refusals.is_empty(), "{:?}", plan.refusals);
        let destinations: Vec<&str> = plan.items.iter().map(|i| i.to.as_str()).collect();
        assert!(destinations.contains(&"C/List"), "{destinations:?}");
        assert!(
            !destinations.iter().any(|d| d.contains("C/OS-Version")),
            "the subtree landed nested under itself: {destinations:?}"
        );
    }

    #[test]
    fn a_component_whose_media_is_absent_names_the_component_and_the_disk() {
        let plan = plan_with(&["extras"], /* media present: */ &["Workbench3.2"]);
        assert!(plan.refusals.contains(&RefusalReason::MediaMissing {
            component: "extras".into(),
            volume_name: "Extras3.2".into(),
        }));
        assert!(plan.items.is_empty());
    }

    /// The media is here and the path is not — the recipe is wrong about this
    /// media. Skipping it silently gives a system missing a library.
    #[test]
    fn a_path_the_recipe_expects_and_the_media_lacks_is_a_refusal_not_a_skip() {
        let plan = plan_where_extras_has_no_l();
        assert!(matches!(
            plan.refusals.as_slice(),
            [RefusalReason::MediaPathMissing { component, path, .. }]
                if component == "extras" && path == "L"
        ));
        assert!(
            plan.items.is_empty(),
            "nothing is planned once the media is wrong"
        );
    }

    #[test]
    fn two_components_wanting_one_path_without_an_override_is_a_collision() {
        let plan = plan_with_colliding_recipe();
        assert!(matches!(
            plan.refusals.as_slice(),
            [RefusalReason::DestinationCollision { path, components }]
                if path == "C/Assign" && components.len() == 2
        ));
        assert!(plan.items.is_empty());
    }

    /// Carried item 4, closed: the shape above is `File` vs `File`, which
    /// `recipe.rs`'s own static check already covers, so it does not prove
    /// `detect_collisions` looks at *walked* items at all. This is the test
    /// the coordinator's review named directly — a file a `Subtree` rule
    /// walks out of one component's media colliding with a file a plain
    /// `File` rule writes from another's. Falsification: with the walk
    /// output excluded from the collision check's input (kept in `items`
    /// otherwise, exactly the mutation the reviewer proposed), this is the
    /// one test in the module that fails — see the report for the
    /// before/after run.
    #[test]
    fn a_walked_file_colliding_with_a_directly_written_file_is_a_collision() {
        let plan = plan_with_a_walked_file_colliding_with_a_direct_file();
        assert!(matches!(
            plan.refusals.as_slice(),
            [RefusalReason::DestinationCollision { path, components }]
                if path == "D/x" && components.len() == 2
        ));
        assert!(plan.items.is_empty());
    }

    /// Promoted from Minor after review: a `File` rule that actually
    /// resolves to a directory used to be emitted anyway, with
    /// `is_dir: true` — which `detect_collisions` filters out entirely, so
    /// a wrong recipe would escape the one check meant to catch a bad
    /// destination. Unreachable from the shipped recipe (which is why it
    /// was Minor), but recipes are data, and this is the only place a
    /// future release's recipe file can have this checked at all.
    #[test]
    fn a_file_rule_over_a_directory_is_a_kind_mismatch() {
        let plan = plan_with_a_file_rule_over_a_directory();
        assert!(matches!(
            plan.refusals.as_slice(),
            [RefusalReason::RuleKindMismatch { component, from, expected, found }]
                if component == "a"
                    && from == "C"
                    && *expected == RuleKind::File
                    && *found == RuleKind::Subtree
        ));
        assert!(plan.items.is_empty());
    }

    /// The other direction: a `Subtree` rule over a plain file would
    /// otherwise carry `bytes: 0` and be silently short, rather than
    /// refused by name.
    #[test]
    fn a_subtree_rule_over_a_file_is_a_kind_mismatch() {
        let plan = plan_with_a_subtree_rule_over_a_file();
        assert!(matches!(
            plan.refusals.as_slice(),
            [RefusalReason::RuleKindMismatch { component, from, expected, found }]
                if component == "a"
                    && from == "readme"
                    && *expected == RuleKind::Subtree
                    && *found == RuleKind::File
        ));
        assert!(plan.items.is_empty());
    }

    /// The change the coordinator asked for after review: `exclusive_group`
    /// existed since Task 1 with nothing enforcing it. `modules-b` is
    /// switched on by its own `Condition`, never `chosen` — proving the
    /// check reads `components_on` (the resolved set) rather than the
    /// request, which is the one way this could be implemented wrong and
    /// still pass a test that chose both components explicitly.
    #[test]
    fn two_members_of_one_exclusive_group_both_resolved_on_is_a_conflict() {
        let plan = plan_with_exclusive_group_conflict();
        assert!(matches!(
            plan.refusals.as_slice(),
            [RefusalReason::ExclusiveGroupConflict { group, components }]
                if group == "modules" && components.len() == 2
        ));
        assert!(plan.items.is_empty());
    }

    #[test]
    fn a_declared_override_is_not_a_collision() {
        let plan = plan_with(
            &["workbench-base", "extras"],
            &["Workbench3.2", "Extras3.2"],
        );
        assert!(!plan
            .refusals
            .iter()
            .any(|r| matches!(r, RefusalReason::DestinationCollision { .. })));
    }

    #[test]
    fn a_conditional_component_is_on_without_being_chosen() {
        let plan = plan_with_rom(&["workbench-base"], 40);
        assert!(plan.components_on.iter().any(|c| c == "modules-a1200"));
    }

    // ---- `InstallRequest::excluded` (fix round: the coordinator's review
    // of Task 13, Criticals 1 and 2) ----
    //
    // Requirement 2 ("turning Modules off is a confirmation, not a refusal")
    // cannot be met by editing a `plan()` result after the fact: a
    // condition-satisfied component with no media in the folder produces a
    // `MediaMissing` refusal that empties `items`, and `osinstallBlocker`
    // reads `refusals`, not `componentsOn` — so a client-side removal of the
    // component's own items leaves the refusal standing and the whole
    // install still blocked. `excluded` has to be subtracted here, inside
    // `resolve_components_on`, before the media-resolution loop ever runs.

    /// Critical 2, at the engine level. Without exclusion, a pre-V47 ROM and
    /// no `ModulesA1200_3.2` in the folder is exactly the review's own
    /// worked example: the confirmation says the system will not boot, and
    /// the install is refused anyway. Excluding the component must make
    /// that refusal not apply at all, not merely reword it.
    #[test]
    fn excluding_a_condition_satisfied_component_with_no_media_present_is_not_a_refusal() {
        let unexcluded = plan_with_rom(&["workbench-base"], 40);
        assert!(
            unexcluded.refusals.iter().any(|r| matches!(
                r,
                RefusalReason::MediaMissing { component, .. } if component == "modules-a1200"
            )),
            "sanity: without exclusion this must be the ordinary MediaMissing \
             case the review described, {:?}",
            unexcluded.refusals
        );

        let dir = crate::core::osinstall::fixtures::scratch("plan-excluded-no-media");
        let folder = dir.join("media");
        std::fs::create_dir(&folder).unwrap();
        let recipe = crate::core::osinstall::recipe::amigaos_32().unwrap();
        let wb = crate::core::osinstall::fixtures::entries_for(&recipe, "Workbench3.2");
        let wb_refs: Vec<(&str, &[u8], u32)> = wb
            .iter()
            .map(|(path, bytes, protection)| (path.as_str(), bytes.as_slice(), *protection))
            .collect();
        crate::core::osinstall::fixtures::media(&folder, "Workbench3.2", "wb.adf", &wb_refs);
        crate::core::osinstall::fixtures::required_media(&folder, &recipe, &["Workbench3.2"]);

        let request = InstallRequest {
            packages: Vec::new(),
            package_folder: None,
            release: "AmigaOS 3.2".to_string(),
            media_folder: folder,
            rom: Some(crate::core::osinstall::fixtures::fake_rom(&dir, 40)),
            chosen: vec!["workbench-base".to_string()],
            excluded: vec!["modules-a1200".to_string()],
            destination: dir.join("dist"),
        };
        let excluded_plan = plan(&request, &recipe).unwrap();

        assert!(
            excluded_plan.refusals.is_empty(),
            "{:?}",
            excluded_plan.refusals
        );
        assert!(!excluded_plan
            .components_on
            .iter()
            .any(|c| c == "modules-a1200"));
        assert!(!excluded_plan
            .items
            .iter()
            .any(|i| i.component == "modules-a1200"));
        assert!(
            !excluded_plan.media_paths.contains_key("ModulesA1200_3.2"),
            "{:?}",
            excluded_plan.media_paths
        );
    }

    /// Exclusion wins over an explicit `chosen` entry for the same id. The
    /// screen keeps the two mutually exclusive by construction, but the
    /// engine must not depend on that: a request naming one component in
    /// both `chosen` and `excluded` is off, not on.
    #[test]
    fn excluding_a_component_wins_over_it_also_being_chosen() {
        let recipe = crate::core::osinstall::recipe::amigaos_32().unwrap();
        let dir = crate::core::osinstall::fixtures::scratch("plan-excluded-over-chosen");
        let folder = dir.join("media");
        std::fs::create_dir(&folder).unwrap();
        let wb = crate::core::osinstall::fixtures::entries_for(&recipe, "Workbench3.2");
        let wb_refs: Vec<(&str, &[u8], u32)> = wb
            .iter()
            .map(|(path, bytes, protection)| (path.as_str(), bytes.as_slice(), *protection))
            .collect();
        crate::core::osinstall::fixtures::media(&folder, "Workbench3.2", "wb.adf", &wb_refs);

        let request = InstallRequest {
            packages: Vec::new(),
            package_folder: None,
            release: "AmigaOS 3.2".to_string(),
            media_folder: folder,
            rom: Some(crate::core::osinstall::fixtures::fake_rom(&dir, 47)),
            chosen: vec!["workbench-base".to_string(), "extras".to_string()],
            excluded: vec!["extras".to_string()],
            destination: dir.join("dist"),
        };
        let result = plan(&request, &recipe).unwrap();

        assert!(!result.components_on.iter().any(|c| c == "extras"));
    }

    /// `required` cannot be excluded — the frontend never offers the
    /// control (the checkbox is disabled), but the engine is the one place
    /// this actually has to hold, defensively, regardless of what a caller
    /// sends.
    #[test]
    fn required_cannot_be_excluded() {
        let recipe = crate::core::osinstall::recipe::amigaos_32().unwrap();
        let dir = crate::core::osinstall::fixtures::scratch("plan-required-not-excludable");
        let folder = dir.join("media");
        std::fs::create_dir(&folder).unwrap();
        let wb = crate::core::osinstall::fixtures::entries_for(&recipe, "Workbench3.2");
        let wb_refs: Vec<(&str, &[u8], u32)> = wb
            .iter()
            .map(|(path, bytes, protection)| (path.as_str(), bytes.as_slice(), *protection))
            .collect();
        crate::core::osinstall::fixtures::media(&folder, "Workbench3.2", "wb.adf", &wb_refs);

        let request = InstallRequest {
            packages: Vec::new(),
            package_folder: None,
            release: "AmigaOS 3.2".to_string(),
            media_folder: folder,
            rom: Some(crate::core::osinstall::fixtures::fake_rom(&dir, 47)),
            chosen: vec![],
            excluded: vec!["workbench-base".to_string()],
            destination: dir.join("dist"),
        };
        let result = plan(&request, &recipe).unwrap();

        assert!(result.components_on.iter().any(|c| c == "workbench-base"));
    }

    /// The coordinator's own hardware runs Kickstart 3.1 rev 40.68, so
    /// `major: 40` — Modules **on** — is the path that actually runs on a
    /// real machine, not a hypothetical branch. Every other test in this
    /// module defaults `plan_with` to `rom_major: Some(47)` (Modules off);
    /// `a_conditional_component_is_on_without_being_chosen` above only
    /// proves `modules-a1200` gets *marked* on, since it never gives that
    /// component's own media, so its `MediaMissing` refusal empties `items`
    /// before the on-path is actually exercised. This one gives
    /// `ModulesA1200_3.2` too, so the on-path resolves end to end: no
    /// refusal, and `modules-a1200`'s own file genuinely lands in `items`.
    ///
    /// All four of `modules-a1200`'s rules are checked, not just its one
    /// `File` rule (`C/LoadModule`) — the review that asked for this test
    /// pointed out that the destination for none of its three `Subtree`
    /// rules (`Libs/Modules`, `Devs/A1200`, `Libs/A1200`) was asserted, so
    /// a bug in the subtree destination mapping specifically would not have
    /// shown on the one path that runs on real hardware.
    #[test]
    fn the_modules_component_resolves_its_own_media_when_its_condition_is_on() {
        let (plan, _dir) = crate::core::osinstall::fixtures::planned_with(
            &["workbench-base"],
            &["Workbench3.2", "ModulesA1200_3.2"],
            Some(40),
        );
        assert!(plan.refusals.is_empty(), "{:?}", plan.refusals);
        assert!(plan
            .items
            .iter()
            .any(|item| item.component == "modules-a1200" && item.to == "C/LoadModule"));
        for expected in [
            "Libs/Modules/placeholder",
            "Devs/A1200/placeholder",
            "Libs/A1200/placeholder",
        ] {
            assert!(
                plan.items
                    .iter()
                    .any(|item| item.component == "modules-a1200" && item.to == expected),
                "missing '{expected}' from modules-a1200's items: {:#?}",
                plan.items
            );
        }
    }

    #[test]
    fn the_total_is_the_sum_of_what_will_actually_be_written() {
        let plan = plan_with(&["workbench-base"], &["Workbench3.2"]);
        assert_eq!(
            plan.total_bytes,
            plan.items.iter().map(|i| i.bytes).sum::<u64>()
        );
    }

    // ---- coverage beyond the brief's own six, closing gaps a
    // falsification pass found ----

    /// The brief's own total-bytes test is satisfied by construction: both
    /// sides of the assertion compute the identical formula, so a `plan()`
    /// that quietly returned `total_bytes: 0` while still filling `items`
    /// correctly would pass it just as well as a correct one, as long as it
    /// stayed self-consistent. This pins the number against a real, known
    /// item list instead, so a broken sum cannot hide behind agreeing with
    /// itself.
    #[test]
    fn the_total_is_a_real_positive_number_not_just_self_consistent() {
        let plan = plan_with(&["workbench-base"], &["Workbench3.2"]);
        assert!(
            plan.total_bytes > 0,
            "workbench-base's media carries real bytes"
        );
        assert!(!plan.items.is_empty());
    }

    /// `condition_holds(_, None)` already refuses rather than guessing
    /// (Task 4's own `an_unidentified_rom_refuses_rather_than_guessing`).
    /// This is the join Task 4's review carried into this task by name: a
    /// ROM file `plan()` cannot read as a Kickstart at all must still reach
    /// the caller as `RefusalReason::RomUnknown`, not as the bare
    /// `CoreError` sentence `rom_facts` itself raises.
    #[test]
    fn a_rom_that_does_not_state_a_version_is_romunknown_not_a_core_error() {
        let dir = crate::core::osinstall::fixtures::scratch("plan-bad-rom");
        let folder = dir.join("media");
        std::fs::create_dir(&folder).unwrap();
        crate::core::osinstall::fixtures::workbench(&folder);
        let bad_rom = dir.join("not-a-rom.bin");
        std::fs::write(&bad_rom, b"nope").unwrap();

        let recipe = crate::core::osinstall::recipe::amigaos_32().unwrap();
        let request = InstallRequest {
            packages: Vec::new(),
            package_folder: None,
            release: "AmigaOS 3.2".to_string(),
            media_folder: folder,
            rom: Some(bad_rom),
            chosen: vec!["workbench-base".to_string()],
            destination: dir.join("dist"),
            excluded: Vec::new(),
        };

        let plan = plan(&request, &recipe).unwrap();
        assert!(plan.refusals.contains(&RefusalReason::RomUnknown));
        assert!(plan.items.is_empty());
    }

    /// Carried concern #2 from Task 3's review: `MediaAmbiguous` existed
    /// with nothing to raise it. Two files in the folder both carry the
    /// volume name `workbench-base` wants — `scan::find_media` already
    /// keeps both rather than guessing at one; this is the proof that
    /// `plan()` actually turns that into the refusal naming the one
    /// component actually affected, not a silently arbitrary pick of
    /// either file.
    #[test]
    fn two_files_claiming_one_volume_name_is_media_ambiguous_for_the_component_that_needs_it() {
        let dir = crate::core::osinstall::fixtures::scratch("plan-ambiguous");
        let folder = dir.join("media");
        std::fs::create_dir(&folder).unwrap();
        let recipe = crate::core::osinstall::recipe::amigaos_32().unwrap();

        let wb = crate::core::osinstall::fixtures::entries_for(&recipe, "Workbench3.2");
        let wb_refs: Vec<(&str, &[u8], u32)> = wb
            .iter()
            .map(|(path, bytes, protection)| (path.as_str(), bytes.as_slice(), *protection))
            .collect();
        crate::core::osinstall::fixtures::media(&folder, "Workbench3.2", "wb-copy-1.adf", &wb_refs);
        crate::core::osinstall::fixtures::media(&folder, "Workbench3.2", "wb-copy-2.adf", &wb_refs);

        let request = InstallRequest {
            packages: Vec::new(),
            package_folder: None,
            release: "AmigaOS 3.2".to_string(),
            media_folder: folder,
            rom: Some(crate::core::osinstall::fixtures::fake_rom(&dir, 47)),
            chosen: vec!["workbench-base".to_string()],
            destination: dir.join("dist"),
            excluded: Vec::new(),
        };
        let plan = plan(&request, &recipe).unwrap();

        assert!(plan.refusals.iter().any(|r| matches!(
            r,
            RefusalReason::MediaAmbiguous { component, volume_name, paths }
                if component == "workbench-base" && volume_name == "Workbench3.2" && paths.len() == 2
        )));
        assert!(plan.items.is_empty());
    }

    // ---- Task 7: `user_startup` resolved onto the plan ----

    /// A hand-built recipe, since every component in the shipped
    /// `amigaos-3.2.json` currently declares no `user_startup` lines at all
    /// (see `mod.rs`'s fixtures comment) — this is the only way to exercise
    /// the field until a real component uses it.
    ///
    /// Three components, not two: `alpha` and `beta` both declare lines —
    /// review item 3 pointed out that the original two-component version
    /// gave `beta` an empty list, so only one component ever contributed
    /// and no test against it could actually distinguish recipe order from
    /// request order. `gamma` carries the empty list instead, so the
    /// "switched on with no lines contributes nothing" case still has a
    /// component to prove itself against. `chosen` below lists them in the
    /// reverse of their recipe declaration order, so a test asserting
    /// recipe order can tell the two apart.
    ///
    /// A fresh scratch directory every call (a counter, not a fixed tag):
    /// this helper is called from more than one test, several of which run
    /// in parallel threads of the same test binary (same pid) — `planned()`
    /// in `apply.rs` documents the exact race a shared tag causes here.
    fn plan_with_user_startup_components() -> InstallPlan {
        static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let n = COUNTER.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let dir = crate::core::osinstall::fixtures::scratch(&format!("plan-user-startup-{n}"));
        let folder = dir.join("media");
        std::fs::create_dir(&folder).unwrap();
        crate::core::osinstall::fixtures::media(&folder, "A", "a.adf", &[("x", b"one", 0)]);
        crate::core::osinstall::fixtures::media(&folder, "B", "b.adf", &[("y", b"two", 0)]);
        crate::core::osinstall::fixtures::media(&folder, "C", "c.adf", &[("z", b"three", 0)]);

        let make = |id: &str, media: &str, from: &str, lines: &[&str]| Component {
            id: id.to_string(),
            media: media.to_string(),
            rules: vec![PathRule {
                from: from.to_string(),
                to: from.to_string(),
                kind: RuleKind::File,
            }],
            required: false,
            condition: None,
            overrides: vec![],
            user_startup: lines.iter().map(|s| s.to_string()).collect(),
            exclusive_group: None,
            available: true,
        };
        let recipe = Recipe {
            release: "Test".to_string(),
            components: vec![
                make("alpha", "A", "x", &["Assign Alpha: SYS:"]),
                make("beta", "B", "y", &["Assign Beta: SYS:"]),
                make("gamma", "C", "z", &[]),
            ],
        };

        let request = InstallRequest {
            packages: Vec::new(),
            package_folder: None,
            release: "AmigaOS 3.2".to_string(),
            media_folder: folder,
            rom: None,
            // Reverse of the recipe's own declaration order — see the doc
            // comment above.
            chosen: vec!["gamma".to_string(), "beta".to_string(), "alpha".to_string()],
            destination: dir.join("dist"),
            excluded: Vec::new(),
        };
        plan(&request, &recipe).unwrap()
    }

    /// The property `apply` (Task 7) actually leans on: a switched-on
    /// component that declares `user_startup` lines carries them on the
    /// plan, under its own id.
    #[test]
    fn a_component_with_user_startup_lines_is_carried_on_the_plan() {
        let plan = plan_with_user_startup_components();
        let contribution = plan
            .user_startup
            .iter()
            .find(|c| c.component == "alpha")
            .expect("alpha declared lines and is switched on");
        assert_eq!(contribution.lines, vec!["Assign Alpha: SYS:".to_string()]);
    }

    /// The negative half: `gamma` is switched on (it is in `components_on`)
    /// but declares no lines at all, so it must not appear in
    /// `user_startup` — a version that carried every on-component
    /// unconditionally, with an empty `Vec`, would still pass a test that
    /// only checked `alpha`'s presence.
    #[test]
    fn a_switched_on_component_with_no_lines_contributes_nothing() {
        let plan = plan_with_user_startup_components();
        assert!(plan.components_on.iter().any(|id| id == "gamma"));
        assert!(!plan.user_startup.iter().any(|c| c.component == "gamma"));
    }

    /// Review item 3: the field doc, `plan.rs`'s own comment and `apply`'s
    /// fold all commit to recipe order, but nothing tested it — the
    /// original fixture could not, since only one component ever
    /// contributed. `chosen` above lists `gamma`, `beta`, `alpha` — the
    /// reverse of the recipe's own declaration order — so this can only
    /// pass if `user_startup` tracks the recipe, not the request.
    #[test]
    fn user_startup_is_carried_in_recipe_order_not_request_order() {
        let plan = plan_with_user_startup_components();
        let ids: Vec<&str> = plan
            .user_startup
            .iter()
            .map(|c| c.component.as_str())
            .collect();
        assert_eq!(ids, vec!["alpha", "beta"]);
    }

    /// Every shipped component's `user_startup` is empty today, so
    /// `workbench-base` — required, always on — must resolve to no
    /// contribution at all. This is the check that would catch a
    /// `filter(!component.user_startup.is_empty())` accidentally inverted
    /// or dropped.
    #[test]
    fn the_shipped_recipe_contributes_no_user_startup_lines_yet() {
        let plan = plan_with(&["workbench-base"], &["Workbench3.2"]);
        assert!(plan.refusals.is_empty(), "{:?}", plan.refusals);
        assert!(plan.user_startup.is_empty());
    }

    // ---- G9: the plan records the ROM it was planned against ----

    /// **G9.** A tree is planned against one Kickstart, and which one decides
    /// what is in it — `modules-a1200` switches on for a pre-V47 ROM and not
    /// otherwise. The plan records that pairing so the check at card time
    /// needs no re-planning and no media.
    #[test]
    fn the_plan_records_the_rom_it_was_planned_against() {
        let (plan, dir) = crate::core::osinstall::fixtures::planned_with(
            &["workbench-base"],
            &["Workbench3.2"],
            Some(47),
        );
        let paired = plan.paired_rom.expect("a plan with a ROM records it");

        assert_eq!(paired.stated_major, Some(47));
        // `modules-a1200`'s own condition (`RomOlderThan { major: 47 }`) does
        // not fire for a V47 ROM, so the tree this plan describes carries no
        // ROM modules at all — which means it needs a real ROM's own copy of
        // them, i.e. a future ROM of at least V47. See
        // `a_tree_planned_on_v47_without_modules_requires_v47`, below, for
        // the same fact stated as its own dedicated test.
        assert_eq!(
            paired.requires_major,
            Some(47),
            "the tree carries no ROM modules for a V47-planned install, so it \
             needs a future ROM to be at least V47 itself"
        );
        assert!(!paired.sha256.is_empty());

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The other half, and the load-bearing one: a tree built for an older
    /// ROM carries the modules that let an older ROM run it, so it requires
    /// nothing — `requires_major` is `None` for the opposite reason.
    #[test]
    fn a_tree_built_for_a_pre_v47_rom_requires_nothing_of_the_card() {
        let (plan, dir) = crate::core::osinstall::fixtures::planned_with(
            &["workbench-base"],
            &["Workbench3.2", "ModulesA1200_3.2"],
            Some(40),
        );
        assert!(plan.components_on.iter().any(|id| id == "modules-a1200"));
        let paired = plan.paired_rom.expect("a plan with a ROM records it");

        assert_eq!(paired.stated_major, Some(40));
        assert_eq!(paired.requires_major, None, "it brings its own modules");

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// And the case the check exists for: a V47-planned tree whose modules
    /// component is *not* on states what a future ROM has to be.
    #[test]
    fn a_tree_planned_on_v47_without_modules_requires_v47() {
        let (plan, dir) = crate::core::osinstall::fixtures::planned_with(
            &["workbench-base"],
            &["Workbench3.2", "ModulesA1200_3.2"],
            Some(47),
        );
        assert!(!plan.components_on.iter().any(|id| id == "modules-a1200"));
        let paired = plan.paired_rom.unwrap();
        assert_eq!(paired.requires_major, Some(47));

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// **ART-128, from the planning side.** `rom_facts` used to strip the
    /// header and read the ciphertext, so a licensed Amiga Forever ROM
    /// refused the whole install with "does not state a Kickstart version".
    #[test]
    fn a_licensed_rom_with_its_key_beside_it_states_its_version() {
        let dir = crate::core::osinstall::fixtures::scratch("rom-facts-cloanto");
        let key = b"a key".to_vec();
        std::fs::write(dir.join("rom.key"), &key).unwrap();

        let mut plain = vec![0u8; 524_288];
        plain[0..2].copy_from_slice(&0x1114u16.to_be_bytes());
        plain[12..14].copy_from_slice(&47u16.to_be_bytes());
        plain[14..16].copy_from_slice(&102u16.to_be_bytes());

        let mut encoded = b"AMIROMTYPE1".to_vec();
        encoded.extend(
            plain
                .iter()
                .enumerate()
                .map(|(at, byte)| byte ^ key[at % key.len()]),
        );
        let path = dir.join("amiga-os-321-a1200.rom");
        std::fs::write(&path, &encoded).unwrap();

        assert_eq!(rom_facts(&path).unwrap().major, 47);

        let _ = std::fs::remove_dir_all(&dir);
    }

    // ---- packages (Task 6) ----------------------------------------------

    use crate::core::osinstall::fixtures;

    /// A scratch directory with the media folder and the package folder as
    /// two separate directories — which is the point: the owner keeps discs
    /// in one folder and archives in another, so a fixture that put them in
    /// one place would be testing a folder layout nobody has.
    fn package_dirs(tag: &str) -> (PathBuf, PathBuf, PathBuf) {
        static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let n = COUNTER.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let dir = fixtures::scratch(&format!("plan-packages-{tag}-{n}"));
        let media = dir.join("media");
        let packages = dir.join("packages");
        std::fs::create_dir(&media).unwrap();
        std::fs::create_dir(&packages).unwrap();
        fixtures::package_test_media(&media);
        (dir, media, packages)
    }

    fn package_request(
        dir: &Path,
        media: &Path,
        package_folder: Option<&Path>,
        packages: &[&str],
    ) -> InstallRequest {
        InstallRequest {
            release: "Test OS".to_string(),
            media_folder: media.to_path_buf(),
            rom: None,
            chosen: Vec::new(),
            excluded: Vec::new(),
            packages: packages.iter().map(|s| s.to_string()).collect(),
            package_folder: package_folder.map(Path::to_path_buf),
            destination: dir.join("dist"),
        }
    }

    /// One more package of the same shape, so ordering and dependencies can
    /// be exercised without three more fixtures.
    fn extra_package(id: &str, media: &str, requires: &[&str]) -> Package {
        let component = Component {
            id: id.to_string(),
            media: media.to_string(),
            rules: vec![PathRule {
                from: "C".to_string(),
                to: "C".to_string(),
                kind: RuleKind::Subtree,
            }],
            required: false,
            condition: None,
            overrides: Vec::new(),
            user_startup: Vec::new(),
            exclusive_group: None,
            available: true,
        };
        Package {
            id: id.to_string(),
            name: id.to_string(),
            media: media.to_string(),
            member: None,
            requires: requires.iter().map(|s| s.to_string()).collect(),
            requires_components: Vec::new(),
            component,
        }
    }

    /// An archive for [`extra_package`]: one file whose name is the
    /// package's own, so two of them never claim one destination. No
    /// directory entries, matching a real package archive — see
    /// `fixtures::package_test_archive`.
    fn extra_archive(folder: &Path, media: &str, file_name: &str) -> PathBuf {
        let path = folder.join(file_name);
        std::fs::write(
            &path,
            crate::core::archive::zip::tests::make_zip_with(&[(
                &format!("{media}/C/{media}Cmd"),
                b"cmd" as &[u8],
            )]),
        )
        .unwrap();
        path
    }

    #[test]
    fn a_chosen_package_is_planned_after_the_releases_own_components() {
        let (dir, media, packages) = package_dirs("after");
        fixtures::package_test_archive(&packages, "pack.zip");

        let request = package_request(&dir, &media, Some(&packages), &["test-package"]);
        let plan = plan_over(
            &request,
            &fixtures::package_test_recipe(),
            &[fixtures::package_test_package()],
        )
        .unwrap();

        assert!(plan.refusals.is_empty(), "{:?}", plan.refusals);
        assert_eq!(plan.packages, vec!["test-package".to_string()]);
        assert_eq!(
            plan.package_media
                .get("TestPack")
                .expect("the archive must be resolved onto the plan")
                .path,
            packages.join("pack.zip")
        );

        // The order is the whole mechanism by which a package's file wins:
        // `apply` writes in plan order and lets the last writer win.
        let base_at = plan
            .items
            .iter()
            .rposition(|i| i.component == "base-c")
            .expect("the base component must contribute items");
        let package_at = plan
            .items
            .iter()
            .position(|i| i.component == "test-package")
            .expect("the package must contribute items");
        assert!(
            package_at > base_at,
            "every package item must come after every component item"
        );

        // And it really is the same destination, or the ordering proves
        // nothing.
        assert!(plan
            .items
            .iter()
            .any(|i| i.component == "test-package" && i.to == fixtures::OVERWRITTEN_PATH));
    }

    #[test]
    fn packages_are_planned_in_dependency_order_not_the_order_they_were_chosen() {
        let (dir, media, packages) = package_dirs("order");
        extra_archive(&packages, "PackA", "a.zip");
        extra_archive(&packages, "PackB", "b.zip");
        let catalogue = vec![
            extra_package("pack-a", "PackA", &[]),
            extra_package("pack-b", "PackB", &["pack-a"]),
        ];

        // Chosen the wrong way round on purpose.
        let request = package_request(&dir, &media, Some(&packages), &["pack-b", "pack-a"]);
        let plan = plan_over(&request, &fixtures::package_test_recipe(), &catalogue).unwrap();

        assert!(plan.refusals.is_empty(), "{:?}", plan.refusals);
        assert_eq!(
            plan.packages,
            vec!["pack-a".to_string(), "pack-b".to_string()]
        );
        let a_at = plan
            .items
            .iter()
            .position(|i| i.component == "pack-a")
            .unwrap();
        let b_at = plan
            .items
            .iter()
            .position(|i| i.component == "pack-b")
            .unwrap();
        assert!(a_at < b_at, "pack-a's items must be placed first");
    }

    #[test]
    fn packages_named_with_no_package_folder_are_refused_saying_which() {
        let (dir, media, _packages) = package_dirs("no-folder");

        let request = package_request(&dir, &media, None, &["test-package"]);
        let plan = plan_over(
            &request,
            &fixtures::package_test_recipe(),
            &[fixtures::package_test_package()],
        )
        .unwrap();

        assert!(plan
            .refusals
            .contains(&RefusalReason::PackageFolderMissing {
                packages: vec!["test-package".to_string()],
            }));
        assert!(plan.items.is_empty(), "a refused plan carries no items");
    }

    /// No packages asked for is not the same thing: it plans, and it is
    /// silent about a folder nobody needed.
    #[test]
    fn no_packages_and_no_folder_is_not_a_refusal() {
        let (dir, media, _packages) = package_dirs("none");

        let request = package_request(&dir, &media, None, &[]);
        let plan = plan_over(&request, &fixtures::package_test_recipe(), &[]).unwrap();

        assert!(plan.refusals.is_empty(), "{:?}", plan.refusals);
        assert!(plan.packages.is_empty());
        assert!(!plan.items.is_empty());
    }

    #[test]
    fn a_package_whose_archive_is_not_in_the_folder_is_refused_by_name() {
        let (dir, media, packages) = package_dirs("missing");
        // The folder exists and holds nothing this package answers to.

        let request = package_request(&dir, &media, Some(&packages), &["test-package"]);
        let plan = plan_over(
            &request,
            &fixtures::package_test_recipe(),
            &[fixtures::package_test_package()],
        )
        .unwrap();

        assert!(plan
            .refusals
            .contains(&RefusalReason::PackageArchiveMissing {
                package: "test-package".to_string(),
                media: "TestPack".to_string(),
            }));
    }

    /// The real case: one package archive beside its language variants. Two
    /// archives claiming one top-level name must be refused by name, never
    /// resolved by whichever sorted first.
    #[test]
    fn two_archives_claiming_one_package_name_are_ambiguous_not_a_guess() {
        let (dir, media, packages) = package_dirs("ambiguous");
        fixtures::package_test_archive(&packages, "pack.zip");
        fixtures::package_test_archive(&packages, "pack-copy.zip");

        let request = package_request(&dir, &media, Some(&packages), &["test-package"]);
        let plan = plan_over(
            &request,
            &fixtures::package_test_recipe(),
            &[fixtures::package_test_package()],
        )
        .unwrap();

        match plan
            .refusals
            .iter()
            .find(|r| matches!(r, RefusalReason::PackageArchiveAmbiguous { .. }))
        {
            Some(RefusalReason::PackageArchiveAmbiguous { package, paths, .. }) => {
                assert_eq!(package, "test-package");
                assert_eq!(paths.len(), 2, "every claimant is named: {paths:?}");
            }
            other => panic!("expected an ambiguity refusal, got {other:?}"),
        }
        assert!(plan.items.is_empty());
    }

    /// `package::order` refuses this too, with an English sentence. It has
    /// to arrive as a typed refusal on the plan instead — a screen showing
    /// every problem at once cannot show one that was raised as an error.
    #[test]
    fn a_requirement_that_was_not_chosen_reaches_refusals_not_a_hard_error() {
        let (dir, media, packages) = package_dirs("requires");
        extra_archive(&packages, "PackB", "b.zip");
        let catalogue = vec![
            extra_package("pack-a", "PackA", &[]),
            extra_package("pack-b", "PackB", &["pack-a"]),
        ];

        let request = package_request(&dir, &media, Some(&packages), &["pack-b"]);
        let plan = plan_over(&request, &fixtures::package_test_recipe(), &catalogue).unwrap();

        assert!(plan
            .refusals
            .contains(&RefusalReason::PackageRequirementMissing {
                package: "pack-b".to_string(),
                requires: "pack-a".to_string(),
            }));
        assert!(plan.items.is_empty());
    }

    /// ART-162 arriving through the selection: the Turkish catalogs without
    /// the Locale component is thirty-six catalogs in a drawer nothing can
    /// open. `requires` cannot say this — `locale-base` is a recipe
    /// component, not a package — so the plan refuses the combination by
    /// name.
    #[test]
    fn a_package_needing_a_component_that_is_off_is_refused_by_name() {
        let (dir, media, packages) = package_dirs("needs-component");
        fixtures::package_test_archive(&packages, "pack.zip");

        let mut package = fixtures::package_test_package();
        package.requires_components = vec!["locale-base".to_string()];

        let request = package_request(&dir, &media, Some(&packages), &["test-package"]);
        let plan = plan_over(&request, &fixtures::package_test_recipe(), &[package]).unwrap();

        assert!(plan
            .refusals
            .contains(&RefusalReason::PackageComponentMissing {
                package: "test-package".to_string(),
                component: "locale-base".to_string(),
            }));
        assert!(
            plan.items.is_empty(),
            "the catalogs must not be planned into a tree that cannot read them"
        );
    }

    /// The same package, with the component it needs switched on, plans
    /// cleanly — so the refusal above is about the missing component and
    /// not about the declaration itself.
    #[test]
    fn a_package_needing_a_component_that_is_on_plans_cleanly() {
        let (dir, media, packages) = package_dirs("has-component");
        fixtures::package_test_archive(&packages, "pack.zip");

        let mut package = fixtures::package_test_package();
        package.requires_components = vec!["base-c".to_string()];

        let request = package_request(&dir, &media, Some(&packages), &["test-package"]);
        let plan = plan_over(&request, &fixtures::package_test_recipe(), &[package]).unwrap();

        assert!(plan.refusals.is_empty(), "{:?}", plan.refusals);
    }

    #[test]
    fn an_unknown_package_id_is_refused_rather_than_skipped() {
        let (dir, media, packages) = package_dirs("unknown");

        let request = package_request(&dir, &media, Some(&packages), &["no-such-package"]);
        let plan = plan_over(&request, &fixtures::package_test_recipe(), &[]).unwrap();

        assert!(plan.refusals.contains(&RefusalReason::PackageUnknown {
            package: "no-such-package".to_string(),
        }));
    }

    /// A package landing on a base file it never declared it may replace is
    /// the same defect two components colliding is — and it can only be seen
    /// once `detect_collisions` can resolve a *package's* own `overrides`,
    /// which is not in the recipe at all.
    #[test]
    fn a_package_overwriting_a_base_file_without_declaring_it_is_a_collision() {
        let (dir, media, packages) = package_dirs("undeclared");
        fixtures::package_test_archive(&packages, "pack.zip");

        let mut package = fixtures::package_test_package();
        package.component.overrides.clear();

        let request = package_request(&dir, &media, Some(&packages), &["test-package"]);
        let plan = plan_over(&request, &fixtures::package_test_recipe(), &[package]).unwrap();

        assert!(
            plan.refusals
                .contains(&RefusalReason::DestinationCollision {
                    path: fixtures::OVERWRITTEN_PATH.to_string(),
                    components: vec!["base-c".to_string(), "test-package".to_string()],
                }),
            "{:?}",
            plan.refusals
        );
    }

    /// And with the declaration, it is not — which is what makes the
    /// declaration mean something rather than being decoration.
    #[test]
    fn a_declared_package_override_is_not_a_collision() {
        let (dir, media, packages) = package_dirs("declared");
        fixtures::package_test_archive(&packages, "pack.zip");

        let request = package_request(&dir, &media, Some(&packages), &["test-package"]);
        let plan = plan_over(
            &request,
            &fixtures::package_test_recipe(),
            &[fixtures::package_test_package()],
        )
        .unwrap();

        assert!(plan.refusals.is_empty(), "{:?}", plan.refusals);
    }
}
