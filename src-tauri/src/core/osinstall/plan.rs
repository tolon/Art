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
    find_media_across, find_media_in_layers, find_packages, media_for_layer, open_media_cached,
    open_package_staging_in, package_for, MediaMatch, PackageMedium,
};
use super::scan_cache::ScanCache;
use super::source::MediaSource;
use super::{Component, Condition, Recipe, RefusalReason, RuleKind};
use crate::core::archive::compress;
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
    /// The paired Kickstart's own resident modules — what
    /// [`Condition::ResidentOlderThan`] reads. Not folded into `major`: the
    /// header version and a resident's own version answer two different
    /// questions (the design's §5, reproduced on
    /// `core::rom::residents`'s own doc comment), and this task exists
    /// precisely because they were found to disagree.
    ///
    /// **Empty means two different things, and `residents_readable` is what
    /// tells them apart.** `core::rom::residents` failing on a dump this
    /// module's header already identified (an unrecognised image size) also
    /// produces an empty `Vec` here — the same shape a ROM that genuinely
    /// carries no such resident produces. Reading `residents` alone cannot
    /// distinguish "this Kickstart does not need the modules" from "ART
    /// could not tell", which is exactly the confident-wrong sentence
    /// fix round 1 of this task found: `condition_holds` checks
    /// `residents_readable` first and refuses
    /// ([`RefusalReason::ResidentTableUnreadable`]) rather than silently
    /// reading "unreadable" as "absent".
    pub residents: Vec<crate::core::rom::RomResident>,
    /// Whether `core::rom::residents` actually succeeded reading the table
    /// above — see that field's own doc comment for why this cannot be
    /// inferred from an empty `Vec`.
    pub residents_readable: bool,
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
    // A dump whose size `core::rom::residents` does not recognise (never a
    // plain 512 KiB or 256 KiB image) fails the *resident* scan without
    // failing `rom_facts` itself — the header above already answered the
    // question `rom_facts` exists to answer. But the failure is not
    // discarded: `residents_readable` carries it forward so
    // `condition_holds` can tell "this ROM does not carry the resident"
    // apart from "ART could not read this ROM's table at all" rather than
    // treating both as the same empty `Vec` (fix round 1, F1).
    let (residents, residents_readable) = match crate::core::rom::residents(&bytes) {
        Ok(table) => (table, true),
        Err(_) => (Vec::new(), false),
    };
    Ok(RomFacts {
        major,
        info,
        residents,
        residents_readable,
    })
}

/// Whether a conditional component switches on, given the facts already
/// read about the paired ROM — `None` when the ROM could not be identified
/// at all, which refuses rather than guessing (see the module doc comment).
///
/// `component` names the caller's own component id and is used for exactly
/// one thing: building [`RefusalReason::ResidentTableUnreadable`], which —
/// unlike [`RefusalReason::RomUnknown`] — is a fact about *this* component's
/// own question rather than about the ROM as a whole, so it has to say which
/// component asked (fix round 1, F1).
pub fn condition_holds(
    component: &str,
    condition: &Condition,
    rom: Option<&RomFacts>,
) -> Result<bool, RefusalReason> {
    let rom = rom.ok_or(RefusalReason::RomUnknown)?;
    match condition {
        Condition::RomOlderThan { major } => Ok(rom.major < *major),
        Condition::RomAtLeast { major } => Ok(rom.major >= *major),
        Condition::ResidentOlderThan {
            resident,
            major,
            minor,
        } => {
            // The ROM is identified fine — only its own module table could
            // not be read. That is a different, actionable fact from "the
            // ROM does not carry this resident", and folding the two
            // together is exactly what fix round 1 found: it would switch
            // the softkick modules component quietly off for a user whose
            // dump ART simply could not scan, with no refusal and nothing on
            // screen (see `RomFacts::residents`'s own doc comment).
            if !rom.residents_readable {
                return Err(RefusalReason::ResidentTableUnreadable {
                    component: component.to_string(),
                    resident: resident.clone(),
                });
            }
            Ok(resident_older_than(
                &rom.residents,
                resident,
                *major,
                *minor,
            ))
        }
    }
}

/// [`Condition::ResidentOlderThan`]'s own comparison, split out so it can be
/// exercised directly against hand-built facts (`resident_condition_holds`
/// in the tests below) without a real ROM file.
///
/// The comparison is lexicographic on `(major, minor)` — the major alone
/// when `minor` is `None`, which is how `strap`'s condition in the shipped
/// recipe is meant to be written: the design's §5 measured `strap` at 45.1
/// for 3.2 and 47.2 for both 3.2.1 and 3.2.2, so only the major actually
/// needs comparing there. **A resident the ROM does not carry never
/// satisfies this** — `find_map` below yields `None` for an absent name, and
/// `unwrap_or(false)` turns "unknown" into "does not hold", never into
/// "older".
fn resident_older_than(
    residents: &[crate::core::rom::RomResident],
    name: &str,
    major: u16,
    minor: Option<u16>,
) -> bool {
    let Some((found_major, found_minor)) = crate::core::rom::resident_revision(residents, name)
    else {
        // The ROM does not carry this resident at all — absent is not
        // "older" (see the module doc comment and this variant's own).
        return false;
    };
    match minor {
        Some(wanted_minor) => (found_major, found_minor) < (major, wanted_minor),
        None => found_major < major,
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

/// The block `S:User-Startup` carries the keymap selection under.
///
/// Not a component id: no component chose this, the **user** did. It is its
/// own block so that changing the keyboard rewrites one marked section and
/// leaves every component's lines — and every line the user wrote — alone.
pub const KEYMAP_SELECTION: &str = "keymap-selection";

/// Is the keymap the user chose actually among the files this plan places?
///
/// Asked of the plan's own items rather than of the media or of a list: what
/// matters is whether `Devs/Keymaps/<name>` will **exist** on the finished
/// tree, and only the items know that.
///
/// Compared with [`super::amiga_names_equal`], the same international fold
/// `scan::media_for` uses — a disc that spells it `TÜRKÇE` and a user who
/// typed `türkçe` mean the same keymap, and answering "missing" there would
/// be a refusal about nothing.
fn keymap_is_placed(items: &[PlanItem], keymap: &str) -> bool {
    items.iter().any(|item| {
        let mut parts = item.to.split('/');
        matches!(
            (parts.next(), parts.next(), parts.next(), parts.next()),
            (Some(devs), Some(keymaps), Some(name), None)
                if devs.eq_ignore_ascii_case("Devs")
                    && keymaps.eq_ignore_ascii_case("Keymaps")
                    && super::amiga_names_equal(name, keymap)
        )
    })
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
    /// What the **medium** holds, which for a compressed entry is not what
    /// gets written. See [`PlanItem::decompress`].
    pub bytes: u64,
    /// Whether these bytes are a `compress`-format `.Z` stream that `apply`
    /// must expand on the way in (ART-228).
    ///
    /// **The plan cannot predict the size this produces.** A `.Z` stream
    /// carries no expanded length, so the only way to know it is to expand
    /// it — and `plan()` runs live while the user ticks boxes, so reading and
    /// decompressing three thousand files to answer a preview would be paid
    /// for on every keystroke. So [`bytes`](Self::bytes) stays the medium's
    /// own figure, `total_bytes` with it, and the two stop being equal to
    /// what `apply` writes exactly when a tree carries compressed content.
    /// That is a real loss — ART-156 established that equality — and it is
    /// recorded here rather than papered over, because the alternative was to
    /// make the preview slow enough that nobody uses it.
    pub decompress: bool,
    /// Whether this item is a [`RuleKind::IconTooltypes`] rule — `apply`
    /// amends the icon already at `to` with `core::amigaicon::merge_tooltypes`
    /// rather than copying `from` over it. `false` for every `File` and
    /// `Subtree` item, which is every item a recipe produced before this
    /// rule kind existed.
    #[serde(default)]
    pub merge_icon: bool,
}

/// What a medium looked like when the plan was made.
///
/// # The hole this closes
///
/// `apply` re-identifies every medium by **volume name** and refuses one that
/// has been renamed, removed or replaced by something that is not install
/// media at all. What it could not see was a *different disc with the same
/// name*: swap `Workbench3.2.adf` for another `Workbench3.2.adf` between the
/// preview and the build, and ART would build from the new one while the
/// screen described the old one. The hash `apply` computes goes into
/// `distribution.json` **after** the fact, so nothing compared.
///
/// Found on 2026-08-23 while reviewing `jit06/emu68-bootstrap`, whose own FAQ
/// makes the principle its first line — *"the tool inspects file contents
/// rather than filenames"* — and applied to ART's own weak point rather than
/// to the problem that project has.
///
/// # Why `(size, mtime)` and not a hash
///
/// Free. `plan` runs again on every component the user ticks, and hashing a
/// 469 MB disc each time is what ART-195 was filed about. A `stat` costs
/// nothing and catches every ordinary swap, because a different file almost
/// never has both the same length and the same modification time.
///
/// **What it does not catch, said plainly**: a disc restored from a backup
/// that preserved its timestamps, of the same size, with different contents.
/// That is ART-194's own documented case — *"same path, same size, same
/// mtime, different disc is a real arrangement"* — and the answer to it is
/// the same one: **Scan again**, which forgets every listing and reads the
/// discs afresh.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MediaStamp {
    pub size: u64,
    /// `0` when the platform will not give a modification time — the check
    /// then rests on size alone, which is weaker and is still better than
    /// the name alone.
    pub mtime_nanos: u64,
}

/// One switch the finished tree will have flipped, and who asked for it.
///
/// Resolved at plan time from [`super::Component::activate`], and **checked
/// against the plan's own items**: a component that asks to switch on a driver
/// nothing places would otherwise produce a tree with a `Devs/DOSDrivers`
/// entry copied from nowhere. See [`InstallPlan::activations`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlannedActivation {
    /// The component that asked. Named so a screen can group them the way it
    /// groups everything else.
    pub component: String,
    /// What it is called — `CD0`, `NTSC`.
    pub name: String,
    /// Where the media leaves it, `/`-separated.
    pub from: String,
    /// Where AmigaOS will look for it.
    pub to: String,
}

/// One destination [`apply`](super::apply::apply) will delete from the tree
/// after every placement has run — see [`super::Component::removes`].
///
/// Resolved at plan time, from the same `components_on` set every other
/// per-component contribution (`user_startup`, `activations`) is, so `apply`
/// never has to consult the recipe again — the same reason those two travel
/// on the plan rather than being looked up a second time.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanRemoval {
    /// The component that asked — named so a screen can group this with
    /// everything else that component does, the same way [`PlannedActivation::component`]
    /// is.
    pub component: String,
    /// The destination to remove, `/`-separated, exactly as
    /// [`super::Component::removes`] states it.
    pub to: String,
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
    /// How many bytes of **file content** [`apply`](super::apply::apply) will
    /// write — the sum of `bytes` over the `items` that are files, and `0`
    /// whenever `items` is empty (the two are always true together, never
    /// computed separately).
    ///
    /// **Directories are excluded, and that is the whole of ART-156.** The
    /// sum used to run over every item. An ADF-sourced directory reports
    /// `bytes: 0` — an AmigaDOS directory block carries no byte-length field
    /// the way a file header does — so nothing was visibly wrong until a
    /// disc: on a CD a directory *is* an extent, and `IsoEntry::bytes` hands
    /// back its declared, sector-rounded extent length. `apply()` turns such
    /// an item into a plain host folder with no content of its own, so those
    /// bytes are real to the disc and imaginary to the tree. Measured against
    /// the owner's own AmigaOS 3.9 CD: `6,108,319` predicted against
    /// `6,054,225` written, a difference of exactly `54,094` — the sum over
    /// the plan's 75 directory items, every one of the 588 file items being
    /// byte-exact.
    ///
    /// **One destination is counted once, and that is ART-205.** A path two
    /// components both write is one file on disk and *two* `items` — that is
    /// what an `overrides` relationship is (ART-112 was a missing one) — so
    /// the sum folds by [`super::destination_key`] and keeps the **last**
    /// writer's size, exactly the rule `apply`'s own `TreeWriter::record`
    /// applies to the bytes it accounts for. ART-124 taught
    /// `ApplyOutcome::files` to count destinations rather than items and left
    /// this field summing items: on the owner's own AmigaOS 3.9 disc,
    /// `17,579,966` predicted against `14,883,492` written, 18% over and
    /// systematic.
    ///
    /// **The one thing it cannot predict** is a composed `S/User-Startup`:
    /// [`apply`](super::apply::apply) merges [`InstallPlan::user_startup`]
    /// into a file no `PlanItem` describes. Every shipped component
    /// contributes no lines, so the two agree today; a future recipe that
    /// contributes some makes this an under-statement.
    ///
    /// This is a progress bar's total, and the thing it is measuring progress
    /// through is bytes written, so it counts what gets written.
    pub total_bytes: u64,
    /// How many **files the tree will hold** when [`apply`](super::apply::apply)
    /// finishes — distinct destinations, the same fold
    /// [`InstallPlan::total_bytes`] performs and the same number
    /// `ApplyOutcome::files` reports afterwards.
    ///
    /// Its own field rather than `items.len()`, which is the count of the
    /// *work*: 1517 plan items produced 1242 files and 105 drawers on the
    /// owner's real 3.9 disc, and the screen quoting `items.len()` told them
    /// 1517 (ART-205, the same arithmetic as `total_bytes` from the other
    /// side). Directories are not predicted at all: `apply` creates the
    /// ancestors no rule names (`count_missing_prefixes`), so a count made
    /// here would be an under-statement nobody could act on.
    ///
    /// `#[serde(default)]` for the reason `paired_rom` and `packages` carry
    /// one: an `InstallPlan` round-trips through the wire, and one serialised
    /// before this field existed must still deserialise.
    #[serde(default)]
    pub total_files: u64,
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
    /// **Populated even when the plan as a whole refuses**, the same rule
    /// [`InstallPlan::components_on`] follows and for the same reason: a
    /// screen must be able to tell "no packages were asked for" from "these
    /// packages were asked for and refused", and an empty list reads as the
    /// first. `items` and `package_media` are the fields that go empty.
    ///
    /// When a refusal stopped the plan before the ordering could be worked
    /// out, this is the request's own list as given rather than a dependency
    /// order — there is no order to state for a selection ART has already
    /// said it cannot apply, and inventing one would be a claim about a run
    /// that will not happen.
    ///
    /// `#[serde(default)]` for the same reason `paired_rom` carries one: an
    /// `InstallPlan` round-trips through the wire, and a plan serialised
    /// before this field existed must still deserialise.
    #[serde(default)]
    pub packages: Vec<String>,
    /// Which folder each layer this build actually used its media from —
    /// carried straight into [`super::apply::DistributionManifest::layers`]
    /// (Task 9), so the manifest can say where each layer's media came from
    /// without `apply` having to re-derive it from `media_paths`, which
    /// names media, not folders.
    ///
    /// **Emptied on a refusal**, the same rule [`InstallPlan::media_paths`]
    /// follows: a folder nothing was built from is not a fact about the tree
    /// this plan describes, because this plan describes no tree.
    ///
    /// `#[serde(default)]` for the same reason every field added to this
    /// struct after it first shipped carries one.
    #[serde(default)]
    pub layers: Vec<super::LayerRecord>,
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
    /// What the finished tree will have switched **on** — see
    /// [`super::Activation`] for the gap this closes and why nothing shipped
    /// asks for any of it.
    ///
    /// `#[serde(default)]` for the reason `paired_rom` and `packages` carry
    /// one: an `InstallPlan` round-trips through the wire, and one serialised
    /// before this field existed must still deserialise.
    #[serde(default)]
    pub activations: Vec<PlannedActivation>,
    /// What each medium looked like when this plan was made — see
    /// [`MediaStamp`]. Keyed by volume name, like [`InstallPlan::media_paths`].
    ///
    /// `#[serde(default)]`: an `InstallPlan` round-trips through the wire, and
    /// one serialised before this field existed must still deserialise. An
    /// empty map means "this plan recorded nothing", which `apply` reads as
    /// nothing to check rather than as everything having changed.
    #[serde(default)]
    pub media_stamps: BTreeMap<String, MediaStamp>,
    /// Every destination a switched-on component removes — see
    /// [`super::Component::removes`] and [`PlanRemoval`]. Populated alongside
    /// `components_on`, the same rule [`InstallPlan::user_startup`] follows
    /// and for the same reason: every shipped component today removes
    /// nothing, so this is empty in practice until AmigaOS 3.2.2's own
    /// recipe uses the field.
    ///
    /// **Not emptied on a refusal**, unlike [`InstallPlan::activations`]:
    /// an activation is checked against the plan's own `items` and is
    /// meaningless once those are empty, but a removal names a destination
    /// declaratively — the same way a `user_startup` line does — so stating
    /// it costs nothing even when the plan as a whole cannot proceed.
    ///
    /// `#[serde(default)]` for the reason every other field added after
    /// `InstallPlan` first shipped carries one: a plan serialised before this
    /// field existed must still deserialise.
    #[serde(default)]
    pub removals: Vec<PlanRemoval>,
}

/// What the user asked for.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallRequest {
    pub media_folder: PathBuf,
    /// More folders holding install media, read alongside
    /// [`InstallRequest::media_folder`].
    ///
    /// **Work-list item 8.** AmigaOS 3.2.2.1 is not one folder of disks: it is
    /// the user's own 3.2 ADFs plus the update disks plus the hotfix disk, and
    /// Hyperion ships the last two as `ADFs/Update/` and `ADFs/Hotfix/` inside
    /// a single download. A model with one media folder cannot express that
    /// install at all.
    ///
    /// A **second field** rather than turning `media_folder` into a list, and
    /// that is deliberate: the first folder is the question the screen has
    /// always asked and the one [`InstallPlan`] reports a wrong-folder verdict
    /// about, so widening it would have rewritten every caller to say "the
    /// first one" — which is a precedence rule, and there is none here.
    /// Duplicated volume names across folders are **refused by name**
    /// (`scan::media_for_layer`, asked with `layer: None` here — this field
    /// only exists for the unlayered case; a layered recipe asks the same
    /// question once per layer instead, against `media_folders` below),
    /// never resolved by order.
    ///
    /// `#[serde(default)]` for the reason the fields below carry one: a
    /// request serialised before this existed must still deserialise.
    #[serde(default)]
    pub extra_media_folders: Vec<PathBuf>,
    /// One media folder per layer the recipe declares, keyed by
    /// [`super::MediaLayer::id`].
    ///
    /// `media_folder` and `extra_media_folders` above stay for the reason
    /// they were given a `#[serde(default)]` in the first place: a request
    /// serialised before this field existed must still deserialise. When this
    /// map is empty they are read exactly as before, onto the single layer.
    #[serde(default)]
    pub media_folders: BTreeMap<String, PathBuf>,
    /// The keyboard layout the finished system should boot with — a name in
    /// `Devs/Keymaps`, e.g. `türkçe`.
    ///
    /// **[ART-226]'s other half.** The `keymaps` component *places* every
    /// keymap the media carries; nothing selected one, so a tree built for a
    /// Turkish user rendered `ç ü ş Ğ` in its menus and still typed on an
    /// American keyboard — which is the complaint that opened the issue.
    ///
    /// Measured on the trees ART has actually built, rather than recalled:
    /// both the 3.2 and the 3.9 tree carry `C/SetKeyboard`, and both their own
    /// `S/Startup-Sequence` files end with
    ///
    /// ```text
    /// IF EXISTS S:User-Startup
    ///   Execute S:User-Startup
    /// ```
    ///
    /// so a `SetKeyboard <name>` line inside ART's own marked block in
    /// `S:User-Startup` is read at every boot — and it runs **after** `IPrefs`,
    /// so it is the last word. The alternative, writing `ENVARC:Sys/input.prefs`,
    /// is a binary file ART would be overwriting on the user's behalf; a line
    /// in a block ART already owns is neither.
    ///
    /// `None` leaves the system on the ROM's `usa`, exactly as before.
    ///
    /// [ART-226]: ../../../../docs/ISSUES.md
    #[serde(default)]
    pub keymap: Option<String>,
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
    /// Whether ART may reuse a medium's listing from an earlier scan
    /// (ART-194). **Absent means [`ScanCachePolicy::Reuse`]** — the fast path
    /// is the ordinary path, and a caller that has never heard of the cache
    /// gets it. Only a user who deliberately switched it off sends
    /// `Ignore`.
    #[serde(default)]
    pub scan_cache: ScanCachePolicy,
}

/// What the user decided about reusing an earlier scan.
///
/// A named two-state enum rather than a bare `bool`, because `#[serde(default)]`
/// on a `bool` is `false`, and `false` would have meant *uncached* for every
/// caller that did not know to say otherwise — the opposite of ART-194's first
/// requirement. Here the default is the variant marked `#[default]`, and it
/// says which one that is out loud.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ScanCachePolicy {
    /// Reuse a listing whose medium's `(path, size, mtime)` are unchanged.
    #[default]
    Reuse,
    /// Read every medium again, and record nothing. A cache the user switched
    /// off that kept filling `%TEMP%` would be a control that quietly ignores
    /// them.
    Ignore,
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
        // International, not ASCII-only (fix round 1, F1) — see
        // `super::fold_amiga_case`. Byte-length slicing stays valid: a
        // Latin-1 upper/lower pair is the same width in UTF-8.
        Some(head) if super::amiga_names_equal(head, from) => {
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
        // **A `required` component's condition is not evaluated here, and
        // that is a decision rather than an oversight (ART-157).** A
        // `Condition` can only turn a component *on* — the `Ok(false)` arm
        // below does nothing — so for a component that is already on
        // unconditionally the evaluation cannot change the outcome. What it
        // *could* do is push `RefusalReason::RomUnknown` and refuse the
        // whole plan over a ROM whose answer was never going to matter.
        //
        // That became reachable the moment AmigaOS 3.9's `workbench-base`
        // grew a `RomAtLeast` stating the release's Kickstart floor: without
        // this skip, building a 3.9 tree would have started requiring a
        // paired ROM that nothing in the build actually needs, purely
        // because the recipe now says out loud what the release needs to
        // *boot*. The requirement is still recorded — `rom_requirement`
        // reads the recipe directly, not this resolved set — and still
        // checked, by G9, against the card the tree is written to.
        if component.required {
            on.push(component.id.clone());
            continue;
        }
        if let Some(condition) = &component.condition {
            match condition_holds(&component.id, condition, rom_facts) {
                Ok(true) => is_on = true,
                Ok(false) => {}
                // `RomUnknown` is one fact about the whole plan's ROM and is
                // deduped so it is not repeated once per conditional
                // component sharing it. `ResidentTableUnreadable` (and any
                // future per-component refusal) is a fact about *this*
                // component's own question — pushed every time, never
                // suppressed by an unrelated component's `RomUnknown`, and
                // never deduped against itself, since a component can carry
                // at most one `Condition` and so can raise this at most once.
                Err(RefusalReason::RomUnknown) => {
                    if !rom_unknown_reported {
                        refusals.push(RefusalReason::RomUnknown);
                        rom_unknown_reported = true;
                    }
                }
                Err(reason) => refusals.push(reason),
            }
        }
        if is_on {
            on.push(component.id.clone());
        }
    }
    on
}

/// Every `exclusive_group` with more than one of its members in the
/// **resolved** `components_on` set, and no `overrides` relationship between
/// them. Checked against what actually resolved on, never against
/// `InstallRequest::chosen` directly — a condition-satisfied component can be
/// switched on without being chosen at all (that is the entire point of
/// `a_conditional_component_is_on_without_being_chosen`), so a check against
/// the request alone would miss exactly the case a condition exists to
/// create. One member of a group is the ordinary case and needs no mention
/// here; a group is inert until a second member exists to conflict with the
/// first (Task 1's review parked this for the same reason — with one Modules
/// disk shipped, the field could not be violated. **That stopped being true
/// the moment a layered recipe could inherit a base component into the same
/// group a derived one declares** — see below.)
///
/// **The rule is `overrides`, not `layer` (ART-238/ART-239).** A first fix
/// scoped this check by `(group, layer)`, which closed the case it was
/// written for — `modules-a1200` (`base` layer, `rom-older-than 47`) and
/// `update-322-modules-a1200` (`update-3.2.2` layer,
/// `resident-older-than exec 47.10`) both switching on for the same pre-47
/// machine, two halves of one release's answer rather than a user's
/// competing choice — but it was wrong in both directions at once. Too
/// blunt: nothing then read the update component's own `overrides:
/// ["modules-a1200"]` at all, so deleting that entry left the suite green
/// with no guard noticing (ART-238). Too weak: two components in the same
/// group but *different* layers could then never conflict, even a future
/// update-layer Modules component for a **different machine** than the base
/// layer's own (ART-239) — layer was never the fact that distinguished the
/// two cases.
///
/// `overrides` already means "these two are not competing — one writes over
/// the other", which is exactly the fact that separates "two halves of one
/// answer" from "two competing choices", independent of which layer either
/// component's own `layer` field names. So a group resolves, whichever
/// layers its members are declared in, exactly when one member's own
/// `overrides` covers every other member directly — the same shape
/// `detect_collisions` already uses to resolve two components claiming one
/// file, and no transitivity beyond that: `A` overriding `B` says nothing
/// about `C` unless `A` (or some other single member) names `C` too. A group
/// with no `overrides` relationship among any of its on-resolved members is
/// still refused, in one layer or across several: that is the case the group
/// exists to catch, and dropping the layer scope gives ART-239's missing
/// cross-layer, different-machine case the same refusal a same-layer one
/// always got.
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
        if components.len() < 2 {
            continue;
        }
        components.sort();

        // Resolved exactly when one member's own `overrides` names every
        // other member of the group directly — see this function's own doc
        // comment for why that is the right rule and not a chain.
        let resolved = components.iter().any(|winner| {
            let Some(winner_component) = recipe.component(winner) else {
                return false;
            };
            components
                .iter()
                .filter(|other| *other != winner)
                .all(|other| winner_component.overrides.contains(other))
        });
        if !resolved {
            refusals.push(RefusalReason::ExclusiveGroupConflict {
                group: group.to_string(),
                components,
            });
        }
    }
    refusals
}

/// Every group of two or more layers the caller pointed at **the same
/// folder**.
///
/// A layer changes which question `media_for_layer` asks, never how the
/// answer is treated — and pointing `base` and `up` at one folder is not "two
/// layers agreeing on a folder", it is one folder unable to answer two
/// different questions at once. Left unchecked, a component in `base` and one
/// in `up` naming the same volume would both resolve to the identical file,
/// silently discarding the whole reason layers exist: telling the 3.2 disk
/// apart from the 3.2.2 disk that shares its name.
///
/// Compared by canonical path, the same normalisation
/// [`find_media_across`](super::scan::find_media_across) already applies to
/// tell "the same folder twice" from "two different folders" — a user who
/// reaches the same directory through two different-looking paths (a drive
/// letter and a UNC path, say) has still pointed both layers at one place.
fn layers_sharing_a_folder(layers: &[(String, PathBuf)]) -> Vec<RefusalReason> {
    let mut by_folder: BTreeMap<PathBuf, Vec<String>> = BTreeMap::new();
    for (layer, folder) in layers {
        let canonical = std::fs::canonicalize(folder).unwrap_or_else(|_| folder.clone());
        by_folder.entry(canonical).or_default().push(layer.clone());
    }

    let mut refusals = Vec::new();
    for (folder, mut sharing) in by_folder {
        if sharing.len() > 1 {
            sharing.sort();
            refusals.push(RefusalReason::LayersShareFolder {
                layers: sharing,
                folder: folder.display().to_string(),
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
    // Keyed by `destination_key`, not by the path as spelled: the Joliet-less
    // 3.9 disc yields `C/ASSIGN` where a BoingBag's ZIP payload yields
    // `C/Assign`, and an exact key made all ~211 of that package's real
    // collisions invisible here — see `destination_key`'s own doc comment.
    // The first spelling seen is carried alongside, so a refusal names a
    // path the user can actually find rather than a folded one.
    let mut claimants: BTreeMap<String, (String, Vec<String>)> = BTreeMap::new();
    for item in items.iter().filter(|item| !item.is_dir) {
        let (_, claiming) = claimants
            .entry(super::destination_key(&item.to))
            .or_insert_with(|| (item.to.clone(), Vec::new()));
        if !claiming.contains(&item.component) {
            claiming.push(item.component.clone());
        }
    }

    let mut refusals = Vec::new();
    for (path, claiming) in claimants.values() {
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

/// Every activation whose source **nothing on the plan places**.
///
/// A component asking to switch on `Storage/Monitors/NTSC` when no rule puts
/// `NTSC` there would give a tree a `Devs/Monitors` entry copied from a file
/// that is not on the disk — which `apply` would then either skip in silence
/// or fail on halfway through. Refused at plan time, by name, with the path
/// nobody writes (§89).
///
/// Compared through [`super::destination_key`] like every other destination
/// question in this module: a Joliet-less disc yields `STORAGE/MONITORS/NTSC`
/// where an ADF yields `Storage/Monitors/NTSC`, and those are one file.
fn detect_missing_activations(
    items: &[PlanItem],
    activations: &[PlannedActivation],
) -> Vec<RefusalReason> {
    let placed: std::collections::BTreeSet<String> = items
        .iter()
        .map(|item| super::destination_key(&item.to))
        .collect();

    activations
        .iter()
        .filter(|activation| {
            // **Exact, and that is the whole check.** A `Subtree` rule does
            // not place a drawer and leave its contents implied: `expand_rules`
            // walks the medium and emits one item per file inside it, so the
            // file an activation names is on the plan by its own path or it is
            // not on the medium at all.
            //
            // Written first with an "or any ancestor drawer is placed"
            // fallback, on the assumption that a subtree was one item. The
            // end-to-end test refused nothing when asked to switch on a
            // monitor the medium does not carry — because the drawer was
            // placed and the fallback took it. Reading `expand_rules` settled
            // it; the fallback was not a safety net, it was the hole.
            !placed.contains(&super::destination_key(&activation.from))
        })
        .map(|activation| RefusalReason::ActivationSourceMissing {
            component: activation.component.clone(),
            name: activation.name.clone(),
            from: activation.from.clone(),
        })
        .collect()
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
///
/// `pub(crate)`, not `pub(super)`: `commands::osinstall::osinstall_collisions`
/// (Task 7) needs exactly this expansion to build the preview it shows
/// against a tree that already exists — the module doc comment's own rule
/// ("build them the same way `plan()` does rather than growing a second,
/// nearly-identical resolver") applies across the `core`/`commands`
/// boundary too, not only within `core::osinstall`.
pub(crate) fn expand_rules(
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
                // A `File` rule names its own destination, so a recipe that
                // points at a `.Z` and asks for the suffixed name gets it —
                // deliberately. Nothing shipped does; if one ever should, it
                // says so by writing the name it wants.
                let compressed = compress::is_compressed_name(&rule.from);
                items.push(PlanItem {
                    component: component.id.clone(),
                    media: component.media.clone(),
                    from: rule.from.clone(),
                    to: compress::name_without_suffix(&rule.to)
                        .filter(|_| compressed)
                        .map(str::to_string)
                        .unwrap_or_else(|| rule.to.clone()),
                    is_dir: false,
                    bytes: entry.size,
                    decompress: compressed,
                    merge_icon: false,
                });
            }
            RuleKind::IconTooltypes if entry.is_dir => {
                // Same shape as the `File` mismatch just above, for the same
                // reason: emitting it anyway would carry `is_dir: true` and
                // escape `detect_collisions`, which only looks at files.
                refusals.push(RefusalReason::RuleKindMismatch {
                    component: component.id.clone(),
                    from: rule.from.clone(),
                    expected: RuleKind::IconTooltypes,
                    found: RuleKind::Subtree,
                });
            }
            RuleKind::IconTooltypes => {
                // Unlike a `File` rule, `to` is never renamed for a `.Z`
                // suffix — an icon is never `compress`-format — and `apply`
                // reads `from`'s bytes as the *source* of a splice into
                // whatever is already at `to`, never writes them there
                // directly. See `PlanItem::merge_icon`.
                items.push(PlanItem {
                    component: component.id.clone(),
                    media: component.media.clone(),
                    from: rule.from.clone(),
                    to: rule.to.clone(),
                    is_dir: false,
                    // The source icon's own on-media size, not the merged
                    // result's — `apply` cannot know the spliced length
                    // without doing the splice, the same pre-existing
                    // estimate-vs-actual gap `total_bytes`'s own doc comment
                    // already documents for a `.Z` stream (ART-156).
                    bytes: entry.size,
                    decompress: false,
                    merge_icon: true,
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
                    decompress: false,
                    merge_icon: false,
                });
                for walked in source.walk(&rule.from)? {
                    let relative = relative_to(&walked.path, &rule.from);
                    // ART-228: the release's own Installer drops the `.Z`
                    // when it expands a file, so the tree holds
                    // `dos.catalog`, never `dos.catalog.Z`. A directory is
                    // never a stream, whatever it is called.
                    let compressed = !walked.is_dir && compress::is_compressed_name(&walked.path);
                    let placed = destination_for(&rule.to, &relative);
                    items.push(PlanItem {
                        component: component.id.clone(),
                        media: component.media.clone(),
                        from: walked.path.clone(),
                        to: if compressed {
                            compress::name_without_suffix(&placed)
                                .map(str::to_string)
                                .unwrap_or(placed)
                        } else {
                            placed
                        },
                        is_dir: walked.is_dir,
                        bytes: walked.size,
                        decompress: compressed,
                        merge_icon: false,
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
///
/// `pub(crate)`: `commands::osinstall::osinstall_add_package` (Task 7) calls
/// this directly, against `components_on` derived from an existing tree's
/// own `distribution.json` rather than from a fresh `plan()`'s resolved
/// set — there is no recipe left to resolve once a tree already exists, so
/// the manifest is the only record of which components actually put files
/// there. Reusing this function rather than a second copy is what keeps
/// `PackageComponentMissing` (ART-162's own rule) meaning the same thing in
/// both places it can fire.
pub(crate) fn detect_package_refusals(
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
        // Before anything about the folder, the archives or the other
        // packages: this one is a property of the package itself, so it is
        // true whatever else the selection looks like, and it is the
        // sentence the user actually needs (ART-166, M3 of the final
        // whole-branch review — the screen used to accept the tick and let
        // a raw English ZIP error arrive after the confirmation).
        if let Some(block) = package.host_placement_block {
            refusals.push(RefusalReason::PackageNotPlaceableOnHost {
                package: id.clone(),
                block,
            });
        }
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
/// [`InstallPlan::total_bytes`] and [`InstallPlan::total_files`] — the tree
/// `apply()` will leave behind, not the reading it will do to get there.
///
/// Two rules, one per defect that produced them:
///
/// - **A directory item contributes nothing** (ART-156). An ADF-sourced
///   drawer reports `bytes: 0`, but a CD-sourced one reports its ISO9660
///   extent length, and `apply()` turns both into a host folder with no
///   content of its own.
/// - **A destination is counted once** (ART-205), folded by
///   [`super::destination_key`] with the **last** writer's size kept — the
///   same key and the same last-writer-wins rule `apply`'s `TreeWriter::record`
///   uses, because these two numbers describe the same tree and may not be
///   worked out two different ways.
///
/// Its own function rather than an inline `sum`, so a test can ask the real
/// arithmetic instead of restating it: the defect ART-156 closed was one where
/// the plan and a test recomputing the same formula agreed with each other
/// and with nothing that was actually written.
fn tree_totals(items: &[PlanItem]) -> (u64, u64) {
    let mut per_destination: BTreeMap<String, u64> = BTreeMap::new();
    for item in items.iter().filter(|item| !item.is_dir) {
        per_destination.insert(super::destination_key(&item.to), item.bytes);
    }
    (per_destination.values().sum(), per_destination.len() as u64)
}

pub fn plan(request: &InstallRequest, recipe: &Recipe) -> CoreResult<InstallPlan> {
    plan_with_cache(request, recipe, &ScanCache::off())
}

/// [`plan`], reusing a medium's listing from `cache` when the medium is
/// unchanged (ART-194).
///
/// **The cache directory is the caller's, never this module's.** `core/` is
/// platform-independent, and where a platform keeps scratch files is not a
/// question it gets to answer — `core::artwork::cache` takes its directory the
/// same way. `commands::osinstall` is the one production caller and passes
/// `%TEMP%`, beside the extraction cache; `plan` itself passes
/// [`ScanCache::off`], so a test or a future CLI shell that has not chosen a
/// directory reads the medium every time rather than writing somewhere it did
/// not ask for.
pub fn plan_with_cache(
    request: &InstallRequest,
    recipe: &Recipe,
    cache: &ScanCache,
) -> CoreResult<InstallPlan> {
    plan_with_cache_in(request, recipe, cache, &std::env::temp_dir())
}

/// [`plan_with_cache`], unpacking a nested package payload under
/// `scratch_root` rather than under the platform's own temp directory.
///
/// **This is what the product calls** (ART-196). Reading a package with a
/// `member` means writing its inner archive out first, and that is the user's
/// disk to choose — the same reason `cache` is a parameter and not a guess.
pub fn plan_with_cache_in(
    request: &InstallRequest,
    recipe: &Recipe,
    cache: &ScanCache,
    scratch_root: &Path,
) -> CoreResult<InstallPlan> {
    plan_over_with_cache(
        request,
        recipe,
        &super::package::packages()?,
        cache,
        scratch_root,
    )
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
    plan_over_with_cache(
        request,
        recipe,
        catalogue,
        &ScanCache::off(),
        &std::env::temp_dir(),
    )
}

fn plan_over_with_cache(
    request: &InstallRequest,
    recipe: &Recipe,
    catalogue: &[Package],
    cache: &ScanCache,
    scratch_root: &Path,
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

    // A layered recipe reads each component's media from the layer the
    // recipe names it under; an unlayered one keeps the flat, ordered list
    // work-list item 8 added — `media_folder` plus every
    // `extra_media_folders` entry, one implicit layer with no name.
    let layers: Vec<(String, PathBuf)> = if recipe.is_layered() {
        recipe
            .layers
            .iter()
            .filter_map(|l| {
                request
                    .media_folders
                    .get(&l.id)
                    .map(|f| (l.id.clone(), f.clone()))
            })
            .collect()
    } else {
        let mut folders = vec![request.media_folder.clone()];
        folders.extend(request.extra_media_folders.iter().cloned());
        folders.into_iter().map(|f| (String::new(), f)).collect()
    };
    // Only a layered recipe's ids are real `MediaLayer::id`s a user could be
    // told apart — an unlayered recipe hands every folder the same `""`
    // sentinel (see `layers` above), and running this check over it produces
    // a refusal naming no layer at all for a release that declares none. The
    // manifest side of this same sentinel was closed in Task 9's fix round;
    // this is the same boundary one call site along.
    // Only a layered recipe's ids are real `MediaLayer::id`s a user could be
    // told apart — an unlayered recipe hands every folder the same `""`
    // sentinel (see `layers` above), and running this check over it produces
    // a refusal naming no layer at all for a release that declares none. The
    // manifest side of this same sentinel was closed in Task 9's fix round;
    // this is the same boundary one call site along.
    if recipe.is_layered() {
        refusals.extend(layers_sharing_a_folder(&layers));
    }
    let found = if recipe.is_layered() {
        find_media_in_layers(&layers)?
    } else {
        find_media_across(&layers.iter().map(|(_, f)| f.clone()).collect::<Vec<_>>())?
    };
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
        // Resolved inside the component's own layer (`Component::layer`) —
        // `None` for an unlayered recipe, which asks across the whole flat
        // list exactly as before layers existed.
        let found_media =
            match media_for_layer(&found, component.layer.as_deref(), &component.media) {
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

        // Never `?` on either of these — ART-119 (#5). A disk that is
        // present but unreadable is a fact about one component, the same
        // way `MediaMissing` is; propagating it would fail the whole plan
        // and blank every other component's file list with it. Both calls
        // are wrapped, because "the medium cannot be opened" and "the
        // medium opened and then could not be walked" are the same fact to
        // a user holding a damaged floppy image. `expand_rules` appends to
        // `refusals` as it goes, so anything it managed to say before
        // failing is kept; its *items* are dropped, which is right — a
        // component built from half an unreadable disk is not a component.
        let mut source = match open_media_cached(found_media, cache) {
            Ok(source) => source,
            Err(e) => {
                refusals.push(RefusalReason::MediaUnreadable {
                    component: component.id.clone(),
                    volume_name: component.media.clone(),
                    path: media_path.display().to_string(),
                    reason: e.to_string(),
                });
                continue;
            }
        };
        media_paths.insert(component.media.clone(), media_path.clone());
        match expand_rules(component, source.as_mut(), &mut refusals) {
            Ok(expanded) => items.extend(expanded),
            Err(e) => refusals.push(RefusalReason::MediaUnreadable {
                component: component.id.clone(),
                volume_name: component.media.clone(),
                path: media_path.display().to_string(),
                reason: e.to_string(),
            }),
        }
    }

    // ---- packages, after the release's own components -------------------
    //
    // After, deliberately: an update package exists to land on top of what
    // the release put down, so its items have to be placed later for the
    // last-writer-wins rule `apply` already applies to be the right way
    // round. `order()` decides the order among them (BoingBag 3.9-2 after
    // 3.9-1 whatever order the boxes were ticked in).
    // Seeded with what was asked for, so a refused plan still names the
    // packages the refusal is about — the same rule `components_on`
    // follows, and what this field's own doc comment promises. Replaced by
    // `order()`'s answer once the selection resolved well enough to have
    // one.
    let mut packages: Vec<String> = request.packages.clone();
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
                    let archive = match package_for(
                        &found,
                        &package.media,
                        package.distinguished_by.as_deref(),
                    ) {
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
                    let mut source = open_package_staging_in(&medium, scratch_root)?;
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
    let mut user_startup: Vec<UserStartupContribution> = components_on
        .iter()
        .filter_map(|id| recipe.component(id))
        .filter(|component| !component.user_startup.is_empty())
        .map(|component| UserStartupContribution {
            component: component.id.clone(),
            lines: component.user_startup.clone(),
        })
        .collect();

    // ART-226's other half: select the keymap, having placed it. The line is
    // only written when the keymap is **really going to be there** — checked
    // against the items this very plan produces, not against a name the caller
    // typed. `S/SetKeyboard` on the owner's own 3.9 tree ends with
    // `echo "ERROR: Can't load keymap"`, and a line that prints that at every
    // boot is worse than no line at all.
    if let Some(chosen) = request
        .keymap
        .as_deref()
        .map(str::trim)
        .filter(|k| !k.is_empty())
    {
        if keymap_is_placed(&items, chosen) {
            user_startup.push(UserStartupContribution {
                component: KEYMAP_SELECTION.to_string(),
                lines: vec![format!("SetKeyboard {chosen}")],
            });
        } else {
            refusals.push(RefusalReason::KeymapMissing {
                keymap: chosen.to_string(),
            });
        }
    }

    // Resolved from the same `components_on` the rules were, and checked
    // against the items those rules produced — see `detect_missing_activations`.
    let activations: Vec<PlannedActivation> = components_on
        .iter()
        .filter_map(|id| recipe.component(id))
        .flat_map(|component| {
            component
                .activate
                .iter()
                .map(|activation| PlannedActivation {
                    component: component.id.clone(),
                    name: activation.name().to_string(),
                    from: activation.from(),
                    to: activation.to(),
                })
        })
        .collect();
    refusals.extend(detect_missing_activations(&items, &activations));

    // Same source as `user_startup` above (`recipe.component`, over
    // `components_on`, in recipe order) and for the same reason: `apply`
    // only ever consumes an `InstallPlan`, never the `Recipe` itself, so
    // whatever it needs to perform a removal has to travel on the plan.
    let removals: Vec<PlanRemoval> = components_on
        .iter()
        .filter_map(|id| recipe.component(id))
        .flat_map(|component| {
            component.removes.iter().map(|to| PlanRemoval {
                component: component.id.clone(),
                to: to.clone(),
            })
        })
        .collect();

    // Stamped from the paths the plan resolved, before anything empties them.
    // A medium that cannot be stat-ed is simply not stamped: `apply` reads a
    // missing stamp as "nothing was recorded", never as "it changed".
    let media_stamps: BTreeMap<String, MediaStamp> = media_paths
        .iter()
        .filter_map(|(volume, path)| {
            super::scan_cache::identity_of(path).map(|id| {
                (
                    volume.clone(),
                    MediaStamp {
                        size: id.size,
                        mtime_nanos: id.mtime_nanos,
                    },
                )
            })
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
    // A refusal empties what a preview would act on, and a switch to flip on
    // a tree that will not be built is one of those things.
    let activations = if refusals.is_empty() {
        activations
    } else {
        Vec::new()
    };
    // `layers` follows `media_paths`'s own rule (immediately above): a folder
    // nothing was built from is not a fact about the tree this plan
    // describes, because a refused plan describes no tree.
    //
    // **Only for a layered recipe** (fix round 1, Finding 2). An unlayered
    // build's own `layers` local above is a flat list of folders paired with
    // an empty-string id — real internally, so `layers_sharing_a_folder` and
    // `find_media_in_layers` have something to iterate — but `""` is not a
    // real `MediaLayer::id`, and writing it into `distribution.json` would
    // give a manifest reader two different spellings of "this tree carries
    // no layers": an absent key on an older tree, and a list of empty-id
    // records on a new one. A reader that ever matched on an id would match
    // `""`. So an unlayered build reports no layers at all, the same as a
    // tree built before this field existed.
    let layers: Vec<super::LayerRecord> = if refusals.is_empty() && recipe.is_layered() {
        layers
            .into_iter()
            .map(|(id, folder)| super::LayerRecord { id, folder })
            .collect()
    } else {
        Vec::new()
    };
    let (total_bytes, total_files) = tree_totals(&items);

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
        total_files,
        components_on,
        paired_rom,
        media_paths,
        packages,
        package_media,
        user_startup,
        activations,
        media_stamps,
        removals,
        layers,
    })
}

/// What a tree with these components needs of a future ROM.
///
/// The two condition kinds contribute from **opposite sides of the switch**,
/// and both mean the same thing about the finished tree — "this needs a
/// Kickstart of at least `major`":
///
/// - [`Condition::RomOlderThan`] contributes when the component is **off**.
///   The component is the fallback (`modules-a1200` carries `LIBS:Modules`),
///   so a tree without it needs the ROM the condition would not have fired
///   for. A tree that *has* it brought its own modules and needs nothing.
/// - [`Condition::RomAtLeast`] contributes when the component is **on**
///   (ART-157). The component's own files are what need the newer ROM, so a
///   tree that carries them carries the requirement; a tree that left them
///   out does not.
///
/// The maximum across every contributor is the tree's floor.
///
/// `pub(crate)` rather than private (fix round 1, F2): the end-to-end test
/// that this requirement actually reaches G9 has to live in `commands/`,
/// because `rom_pairing_for` reads two manifests and `core/` may not depend
/// on `commands/`. The function is pure — a recipe and a list of ids in, a
/// number out — so widening it costs nothing and buys the one hop the
/// original test skipped.
pub(crate) fn rom_requirement(recipe: &Recipe, components_on: &[String]) -> Option<u16> {
    recipe
        .components
        .iter()
        .filter_map(|component| {
            let is_on = components_on.iter().any(|on| on == &component.id);
            match &component.condition {
                Some(Condition::RomOlderThan { major }) if !is_on => Some(*major),
                Some(Condition::RomOlderThan { .. }) => None,
                Some(Condition::RomAtLeast { major }) if is_on => Some(*major),
                Some(Condition::RomAtLeast { .. }) => None,
                // A resident's own version does not state a Kickstart floor
                // for the whole tree the way `RomOlderThan`/`RomAtLeast` do
                // — it is answered from the paired ROM's resident table, not
                // the header `rom_requirement` reasons about here.
                Some(Condition::ResidentOlderThan { .. }) => None,
                None => None,
            }
        })
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
                whdload_crc16: None,
            },
            residents: Vec::new(),
            residents_readable: true,
        }
    }

    /// Facts carrying only the two residents AmigaOS 3.2.2's Modules step
    /// asks about, built from plain numbers — never a real ROM file, which
    /// ART neither ships nor needs to read for this.
    fn residents_of(exec: (u16, u16), strap: (u16, u16)) -> RomFacts {
        let mut facts = fake_rom_facts(47);
        facts.residents = vec![
            crate::core::rom::RomResident {
                name: "exec.library".into(),
                version: exec.0 as u8,
                id: format!("exec {}.{} (test)", exec.0, exec.1),
            },
            crate::core::rom::RomResident {
                name: "strap".into(),
                version: strap.0 as u8,
                id: format!("strap {}.{} (test)", strap.0, strap.1),
            },
        ];
        facts
    }

    /// `condition_holds` wrapped for a `Condition` known to be a
    /// `ResidentOlderThan` against facts known to identify a ROM **and**
    /// carry a readable resident table — both true of every call these tests
    /// make, so the `Result`'s other cases (an unrelated condition kind, an
    /// unidentified ROM, an unreadable resident table) never arise here. The
    /// component name is irrelevant to every assertion built on this helper,
    /// so a fixed placeholder stands in for it.
    fn resident_condition_holds(condition: &Condition, facts: &RomFacts) -> bool {
        condition_holds("modules-a1200", condition, Some(facts))
            .expect("a resident condition against known, readable facts")
    }

    /// `Workbench3.2.adf:S/Startup-sequence` opens with
    /// `Version exec.library version 47 … If Warn … Quit`. So a 3.2 system on a
    /// 3.1 ROM without `LIBS:Modules` does not boot at all.
    #[test]
    fn a_pre_v47_rom_turns_the_modules_component_on() {
        let facts = fake_rom_facts(40);
        let holds = condition_holds(
            "modules-a1200",
            &Condition::RomOlderThan { major: 47 },
            Some(&facts),
        );
        assert_eq!(holds, Ok(true));
    }

    #[test]
    fn a_v47_rom_leaves_it_off() {
        let facts = fake_rom_facts(47);
        let holds = condition_holds(
            "modules-a1200",
            &Condition::RomOlderThan { major: 47 },
            Some(&facts),
        );
        assert_eq!(holds, Ok(false));
    }

    /// **ART-157.** The mirror of the two above: `RomAtLeast` holds for a
    /// ROM at or above its major and not below it.
    ///
    /// Both edges of the boundary are asserted, because an off-by-one here
    /// would be invisible — 40 is the number AmigaOS 3.9's own installer
    /// names ("You have to install Kickstart 3.1 ROMs before installing
    /// Workbench 3.9", `OS-Version3.9/OS3.9Install`), so a V40 ROM must
    /// satisfy it and a V39 must not.
    #[test]
    fn a_rom_at_least_condition_holds_from_its_own_major_upwards() {
        for (major, expected) in [(37u16, false), (39, false), (40, true), (47, true)] {
            let facts = fake_rom_facts(major);
            assert_eq!(
                condition_holds(
                    "workbench-base",
                    &Condition::RomAtLeast { major: 40 },
                    Some(&facts)
                ),
                Ok(expected),
                "a V{major} ROM against a V40 floor"
            );
        }
    }

    /// The two condition kinds contribute to `rom_requirement` from
    /// opposite sides of the switch, and this asserts both against the
    /// **shipped** recipes rather than a synthetic pair — the number that
    /// matters is the one a real tree records.
    ///
    /// Vacuity guard built in: the 3.2 half would pass unchanged if
    /// `RomAtLeast` had never been added, so the 3.9 half is what the issue
    /// is about, and the "off" case for 3.9 pins that the contribution
    /// really is conditional on the component being on rather than a
    /// constant.
    #[test]
    fn each_condition_kind_contributes_its_requirement_from_its_own_side() {
        let os32 = super::super::recipe::by_release("AmigaOS 3.2").unwrap();
        // `modules-a1200` off: the tree lacks LIBS:Modules, so it needs the
        // ROM the condition would have fired for.
        assert_eq!(
            rom_requirement(&os32, &["workbench-base".to_string()]),
            Some(47)
        );
        // On: it brought its own modules and needs nothing.
        assert_eq!(
            rom_requirement(
                &os32,
                &["workbench-base".to_string(), "modules-a1200".to_string()]
            ),
            None
        );

        let os39 = super::super::recipe::by_release("AmigaOS 3.9").unwrap();
        // `workbench-base` on: its files are what need the newer ROM.
        assert_eq!(
            rom_requirement(&os39, &["workbench-base".to_string()]),
            Some(40),
            "AmigaOS 3.9 states Kickstart 3.1 (V40) as its own floor"
        );
        // Off (not a real selection — `workbench-base` is `required` — but
        // the direction has to be the opposite one to its sibling's, or the
        // two kinds are not actually distinguished).
        assert_eq!(rom_requirement(&os39, &[]), None);
    }

    /// **A minimum nothing checks is the same as no minimum.**
    ///
    /// **What this test proves, exactly** (fix round 1, F2 — the claim that
    /// stood here said "the whole chain", and it was not): the shipped 3.9
    /// recipe → `rom_requirement` → a `TreeRom` carrying the two fields
    /// `commands::preload::rom_pairing_for` builds one from → G9's
    /// `core::rom::pairing::compare`. The `TreeRom` here is **hand-built**,
    /// so the manifest hop — `PairedRom` written into `distribution.json` by
    /// `apply`, read back and mapped by `rom_pairing_for` — is *not* covered
    /// by this test. It is covered by
    /// `commands::preload::tests::a_39_trees_recorded_minimum_survives_the_manifest_and_reaches_g9`,
    /// which writes a real manifest and calls `rom_pairing_for` itself.
    ///
    /// Neither test says anything about a real card or a real boot. Nothing
    /// on this branch does; see FEATURES.md's 🟡 row and ART-159.
    ///
    /// Before ART-157 the first step answered `None` for every 3.9 tree, so
    /// the last one answered `Suitable` for a card carrying a V37 ROM.
    #[test]
    fn a_39_tree_reports_unsuitable_against_a_card_carrying_a_pre_v40_rom() {
        use crate::core::rom::pairing::{compare, CardRom, Pairing, TreeRom};

        let os39 = super::super::recipe::by_release("AmigaOS 3.9").unwrap();
        let requires_major = rom_requirement(&os39, &["workbench-base".to_string()]);
        assert_eq!(requires_major, Some(40), "the recipe states the floor");

        // The same mapping `commands::preload::rom_pairing_for` performs.
        let tree = TreeRom {
            sha256: "a".repeat(64),
            requires_major,
        };

        let old_card = CardRom {
            name: "kick.rom".to_string(),
            sha256: "b".repeat(64),
            stated_major: Some(37),
        };
        assert_eq!(
            compare(Some(&tree), Some(&old_card)),
            Pairing::Unsuitable {
                needs: 40,
                found: Some(37),
                rom: "kick.rom".to_string(),
            },
            "a 3.9 tree on a Kickstart 2.0 card must not read as fine"
        );

        let good_card = CardRom {
            name: "kick.rom".to_string(),
            sha256: "b".repeat(64),
            stated_major: Some(40),
        };
        assert_eq!(
            compare(Some(&tree), Some(&good_card)),
            Pairing::Suitable {
                rom: "kick.rom".to_string()
            },
            "and a Kickstart 3.1 card must not be refused"
        );
    }

    /// Guessing costs 800 KB, or a system that quits at boot. Neither is ART's
    /// to choose.
    #[test]
    fn an_unidentified_rom_refuses_rather_than_guessing() {
        let holds = condition_holds(
            "modules-a1200",
            &Condition::RomOlderThan { major: 47 },
            None,
        );
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

    /// The evidence for [`Condition::ResidentOlderThan`] existing at all
    /// (design §5): the release's own Modules step asks a running machine
    /// for `exec.library`'s revision and for `strap`'s version, and a header
    /// proxy collapses the 3.2 and 3.2.1 rows into one outcome.
    ///
    /// **The 3.2.1 row lives in its own test**
    /// ([`a_3_2_1_rom_gets_the_smaller_module_set`]) rather than as a third
    /// assertion here (review fix round 1, F2). It is the row that actually
    /// tells a real `(major, minor)` comparison apart from a major-only one
    /// — 3.2 and 3.2.2 both happen to come out right under a major-only
    /// comparison too, so a mutation that drops the minor entirely still
    /// passed this test's *first* assertion and never reached the row the
    /// distinction is about. Named on its own, that mutation has nowhere
    /// else to hide.
    #[test]
    fn the_modules_condition_answers_what_the_release_answers_for_3_2_and_3_2_2() {
        let exec_older = Condition::ResidentOlderThan {
            resident: "exec".into(),
            major: 47,
            minor: Some(10),
        };
        let strap_older = Condition::ResidentOlderThan {
            resident: "strap".into(),
            major: 47,
            minor: None,
        };
        // (exec, strap), measured out of the owner's own A1200 Kickstarts.
        let kick_32 = residents_of((47, 7), (45, 1));
        let kick_322 = residents_of((47, 10), (47, 2));

        assert!(resident_condition_holds(&exec_older, &kick_32));
        assert!(
            resident_condition_holds(&strap_older, &kick_32),
            "3.2's ROM gets the larger file set"
        );

        assert!(!resident_condition_holds(&exec_older, &kick_322));
        assert!(
            !resident_condition_holds(&strap_older, &kick_322),
            "3.2.2's own ROM needs no softkicked modules at all"
        );
    }

    /// **The row a header proxy — and a major-only comparison — gets wrong**
    /// (review fix round 1, F2; this is the entire reason this task was
    /// rewritten away from `RomOlderThan`'s header version). Split out of
    /// `the_modules_condition_answers_what_the_release_answers_for_3_2_and_3_2_2`
    /// so this specific row fails on its own rather than being reachable
    /// only after two earlier assertions that a weaker guard can satisfy by
    /// accident.
    #[test]
    fn a_3_2_1_rom_gets_the_smaller_module_set() {
        let exec_older = Condition::ResidentOlderThan {
            resident: "exec".into(),
            major: 47,
            minor: Some(10),
        };
        let strap_older = Condition::ResidentOlderThan {
            resident: "strap".into(),
            major: 47,
            minor: None,
        };
        let kick_321 = residents_of((47, 8), (47, 2));

        assert!(resident_condition_holds(&exec_older, &kick_321));
        assert!(
            !resident_condition_holds(&strap_older, &kick_321),
            "3.2.1's ROM gets the smaller set - Shell-Seg and the three libraries are \
             withheld, which is exactly what the header proxy got wrong"
        );
    }

    #[test]
    fn a_condition_naming_a_resident_the_rom_does_not_carry_does_not_hold() {
        let c = Condition::ResidentOlderThan {
            resident: "nosuchthing".into(),
            major: 47,
            minor: None,
        };
        assert!(
            !resident_condition_holds(&c, &residents_of((47, 7), (45, 1))),
            "an absent resident switches nothing on - never a default of `older`"
        );
    }

    /// Facts identifying a ROM whose header parsed fine but whose resident
    /// table could not be read — the case `core::rom::residents` returning
    /// `Err` on an image `rom_facts` already identified produces.
    fn unreadable_resident_facts() -> RomFacts {
        let mut facts = fake_rom_facts(47);
        facts.residents_readable = false;
        facts
    }

    /// **Review fix round 1, F1.** Before this fix, `rom_facts` folded a
    /// failed resident scan into the same empty `Vec` a ROM that genuinely
    /// carries no such resident produces — so a component conditioned on
    /// `ResidentOlderThan` was silently switched off, with no refusal and
    /// nothing on screen, exactly the "endings stay distinct" rule this
    /// project keeps re-learning the cost of breaking.
    ///
    /// **Absence from `components_on` has more than one cause here, so this
    /// does not assert that alone** — a legitimately-unsatisfied condition
    /// also leaves the component off `components_on`, and a test that
    /// stopped there would pass against both the fix and the defect it
    /// fixes. This asserts the specific refusal by value: which component,
    /// which resident.
    #[test]
    fn an_unreadable_resident_table_refuses_the_component_by_name_rather_than_silently_switching_it_off(
    ) {
        let recipe = Recipe {
            layers: vec![],
            base: None,
            release: "Test".to_string(),
            components: vec![Component {
                layer: None,
                id: "modules-a1200".to_string(),
                media: "ModulesA1200".to_string(),
                rules: vec![],
                required: false,
                condition: Some(Condition::ResidentOlderThan {
                    resident: "exec".into(),
                    major: 47,
                    minor: Some(10),
                }),
                overrides: vec![],
                user_startup: vec![],
                activate: vec![],
                exclusive_group: None,
                label_key: None,
                available: true,
                removes: Vec::new(),
            }],
        };
        let facts = unreadable_resident_facts();
        let mut refusals = Vec::new();
        let on = resolve_components_on(&recipe, &[], &[], Some(&facts), &mut refusals);

        assert!(
            !on.contains(&"modules-a1200".to_string()),
            "an undecidable condition must not switch the component on"
        );
        assert_eq!(
            refusals,
            vec![RefusalReason::ResidentTableUnreadable {
                component: "modules-a1200".to_string(),
                resident: "exec".to_string(),
            }],
            "the component must be refused by this specific, named sentence - not merely \
             absent from components_on, which a legitimately-unsatisfied condition would also \
             produce"
        );
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
            extra_media_folders: Vec::new(),
            media_folders: BTreeMap::new(),
            keymap: None,
            rom: Some(crate::core::osinstall::fixtures::fake_rom(&dir, 47)),
            chosen: vec!["extras".to_string()],
            destination: dir.join("dist"),
            excluded: Vec::new(),
            scan_cache: Default::default(),
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
            layers: vec![],
            base: None,
            release: "Test".to_string(),
            components: vec![
                Component {
                    layer: None,
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
                    activate: vec![],
                    exclusive_group: None,
                    label_key: None,
                    available: true,
                    removes: Vec::new(),
                },
                Component {
                    layer: None,
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
                    activate: vec![],
                    exclusive_group: None,
                    label_key: None,
                    available: true,
                    removes: Vec::new(),
                },
            ],
        };

        let request = InstallRequest {
            packages: Vec::new(),
            package_folder: None,
            release: "AmigaOS 3.2".to_string(),
            media_folder: folder,
            extra_media_folders: Vec::new(),
            media_folders: BTreeMap::new(),
            keymap: None,
            rom: None,
            chosen: vec!["subtree-owner".to_string(), "file-writer".to_string()],
            destination: dir.join("dist"),
            excluded: Vec::new(),
            scan_cache: Default::default(),
        };
        plan(&request, &recipe).unwrap()
    }

    // -----------------------------------------------------------------
    // ART-226's other half: selecting the keymap, having placed it.
    // -----------------------------------------------------------------

    /// A recipe whose one component places two keymaps, so a selection has
    /// something real to be checked against.
    fn keymap_fixture() -> (PathBuf, Recipe, PathBuf) {
        let dir = crate::core::osinstall::fixtures::scratch("plan-keymap");
        let folder = dir.join("media");
        std::fs::create_dir_all(&folder).unwrap();
        crate::core::osinstall::fixtures::media(
            &folder,
            "Shelf",
            "shelf.adf",
            &[("Keymaps/türkçe", b"tr", 0), ("Keymaps/usa", b"us", 0)],
        );
        let recipe = Recipe {
            layers: vec![],
            base: None,
            release: "Test".to_string(),
            components: vec![Component {
                layer: None,
                id: "keymaps".to_string(),
                media: "Shelf".to_string(),
                rules: vec![PathRule {
                    from: "Keymaps".to_string(),
                    to: "Devs/Keymaps".to_string(),
                    kind: RuleKind::Subtree,
                }],
                required: true,
                condition: None,
                overrides: vec![],
                user_startup: vec![],
                activate: vec![],
                exclusive_group: None,
                label_key: None,
                available: true,
                removes: Vec::new(),
            }],
        };
        (dir, recipe, folder)
    }

    fn keymap_request(folder: &Path, dest: PathBuf, keymap: Option<&str>) -> InstallRequest {
        InstallRequest {
            packages: Vec::new(),
            package_folder: None,
            release: "AmigaOS 3.2".to_string(),
            media_folder: folder.to_path_buf(),
            extra_media_folders: Vec::new(),
            media_folders: BTreeMap::new(),
            keymap: keymap.map(str::to_string),
            rom: None,
            chosen: vec!["keymaps".to_string()],
            destination: dest,
            excluded: Vec::new(),
            scan_cache: Default::default(),
        }
    }

    /// **The complaint that opened ART-226, answered.** The tree rendered
    /// Turkish and typed American, because every keymap was placed and none
    /// was selected.
    ///
    /// The line goes into `S:User-Startup`, which both the 3.2 and the 3.9
    /// tree's own `S/Startup-Sequence` ends by executing — measured on the
    /// trees ART has built, not recalled — and the command is `SetKeyboard`,
    /// which is what `C/` in those same trees actually carries.
    #[test]
    fn choosing_a_keymap_writes_the_line_that_selects_it() {
        let (dir, recipe, folder) = keymap_fixture();
        let request = keymap_request(&folder, dir.join("dist"), Some("türkçe"));

        let planned = plan(&request, &recipe).unwrap();
        assert!(planned.refusals.is_empty(), "{:?}", planned.refusals);

        let selection = planned
            .user_startup
            .iter()
            .find(|c| c.component == KEYMAP_SELECTION)
            .expect("the keymap selection must contribute a line");
        assert_eq!(selection.lines, vec!["SetKeyboard türkçe".to_string()]);
    }

    /// **Nothing chosen is nothing written**, and the system stays on the
    /// ROM's `usa` exactly as before. A default here would be ART choosing
    /// somebody's keyboard for them.
    #[test]
    fn choosing_nothing_leaves_the_startup_file_alone() {
        let (dir, recipe, folder) = keymap_fixture();
        let request = keymap_request(&folder, dir.join("dist"), None);

        let planned = plan(&request, &recipe).unwrap();
        assert!(planned
            .user_startup
            .iter()
            .all(|c| c.component != KEYMAP_SELECTION));
    }

    /// **The one that matters most.** The tree's own `S/SetKeyboard` script
    /// ends with `echo "ERROR: Can't load keymap"`, so a line naming a keymap
    /// the install does not place prints that at every boot — and looks like
    /// ART did the thing while the keyboard is still American.
    ///
    /// Checked against the plan's **own items**, never against a list or a
    /// name the caller typed.
    #[test]
    fn a_keymap_this_install_would_not_place_is_refused_rather_than_written() {
        let (dir, recipe, folder) = keymap_fixture();
        let request = keymap_request(&folder, dir.join("dist"), Some("norsk"));

        let planned = plan(&request, &recipe).unwrap();
        assert!(
            planned
                .user_startup
                .iter()
                .all(|c| c.component != KEYMAP_SELECTION),
            "no line may be written"
        );
        assert!(
            planned
                .refusals
                .iter()
                .any(|r| matches!(r, RefusalReason::KeymapMissing { keymap } if keymap == "norsk")),
            "and it says so: {:?}",
            planned.refusals
        );
    }

    /// A disc that spells it `TÜRKÇE` and a user who typed `türkçe` mean the
    /// same keymap. Refusing there would be a refusal about nothing — the same
    /// international fold `scan::media_for` already uses.
    #[test]
    fn the_selection_is_compared_the_way_amigados_compares_names() {
        let (dir, recipe, folder) = keymap_fixture();
        let request = keymap_request(&folder, dir.join("dist"), Some("TÜRKÇE"));

        let planned = plan(&request, &recipe).unwrap();
        assert!(planned.refusals.is_empty(), "{:?}", planned.refusals);
        // Written as the **user** typed it, which is what they will read back.
        let selection = planned
            .user_startup
            .iter()
            .find(|c| c.component == KEYMAP_SELECTION)
            .expect("must resolve");
        assert_eq!(selection.lines, vec!["SetKeyboard TÜRKÇE".to_string()]);
    }

    /// Its own block, not a component's. Nobody's component chose this, and a
    /// later change of keyboard must rewrite one marked section and leave
    /// every other line — ART's and the user's — alone.
    #[test]
    fn the_selection_gets_its_own_block() {
        assert!(
            !crate::core::osinstall::recipe::amigaos_32()
                .unwrap()
                .components
                .iter()
                .any(|c| c.id == KEYMAP_SELECTION),
            "'{KEYMAP_SELECTION}' must not collide with a component id"
        );
    }

    // -----------------------------------------------------------------
    // Work-list item 8: an install whose media is in more than one folder.
    // -----------------------------------------------------------------

    /// **The shape AmigaOS 3.2.2.1 ships in**, at the level that matters: a
    /// plan whose components come from two folders at once, refusing nothing.
    ///
    /// This is the capability the recipes were blocked behind. Before it, a
    /// user with their 3.2 ADFs in one folder and Hyperion's `ADFs/Update/` in
    /// another could not express the install at all — whichever folder they
    /// named, every component from the other one came back `MediaMissing`.
    #[test]
    fn a_plan_reads_media_out_of_every_folder_it_was_given() {
        let dir = crate::core::osinstall::fixtures::scratch("plan-two-folders");
        let base = dir.join("base");
        let update = dir.join("Update");
        std::fs::create_dir_all(&base).unwrap();
        std::fs::create_dir_all(&update).unwrap();
        crate::core::osinstall::fixtures::media(&base, "Base", "base.adf", &[("C/One", b"1", 0)]);
        crate::core::osinstall::fixtures::media(
            &update,
            "Later",
            "later.adf",
            &[("C/Two", b"2", 0)],
        );

        let component = |id: &str, media: &str, from: &str| Component {
            id: id.to_string(),
            media: media.to_string(),
            rules: vec![PathRule {
                from: from.to_string(),
                to: from.to_string(),
                kind: RuleKind::File,
            }],
            required: true,
            condition: None,
            overrides: vec![],
            user_startup: vec![],
            activate: vec![],
            exclusive_group: None,
            label_key: None,
            layer: None,
            available: true,
            removes: Vec::new(),
        };
        let recipe = Recipe {
            layers: vec![],
            base: None,
            release: "Test".to_string(),
            components: vec![
                component("from-base", "Base", "C/One"),
                component("from-update", "Later", "C/Two"),
            ],
        };

        let request = InstallRequest {
            packages: Vec::new(),
            package_folder: None,
            release: "AmigaOS 3.2".to_string(),
            media_folder: base.clone(),
            extra_media_folders: vec![update.clone()],
            media_folders: BTreeMap::new(),
            keymap: None,
            rom: None,
            chosen: vec!["from-base".to_string(), "from-update".to_string()],
            destination: dir.join("dist"),
            excluded: Vec::new(),
            scan_cache: Default::default(),
        };

        let planned = plan(&request, &recipe).unwrap();
        assert!(
            planned.refusals.is_empty(),
            "both folders were named: {:?}",
            planned.refusals
        );
        assert_eq!(planned.items.len(), 2);

        // **And the other arm**, without which the test above passes on a
        // plan that reads the whole disk anyway: naming only the first folder
        // has to refuse the component that is in the second.
        let one_folder = InstallRequest {
            extra_media_folders: Vec::new(),
            media_folders: BTreeMap::new(),
            keymap: None,
            ..request
        };
        let narrower = plan(&one_folder, &recipe).unwrap();
        assert_eq!(
            narrower.refusals.len(),
            1,
            "the update disk is in a folder nobody named: {:?}",
            narrower.refusals
        );

        let _ = std::fs::remove_dir_all(&dir);
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
            layers: vec![],
            base: None,
            release: "Test".to_string(),
            components: vec![Component {
                layer: None,
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
                activate: vec![],
                exclusive_group: None,
                label_key: None,
                available: true,
                removes: Vec::new(),
            }],
        };

        let request = InstallRequest {
            packages: Vec::new(),
            package_folder: None,
            release: "AmigaOS 3.2".to_string(),
            media_folder: folder,
            extra_media_folders: Vec::new(),
            media_folders: BTreeMap::new(),
            keymap: None,
            rom: None,
            chosen: vec!["a".to_string()],
            destination: dir.join("dist"),
            excluded: Vec::new(),
            scan_cache: Default::default(),
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
            layers: vec![],
            base: None,
            release: "Test".to_string(),
            components: vec![Component {
                layer: None,
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
                activate: vec![],
                exclusive_group: None,
                label_key: None,
                available: true,
                removes: Vec::new(),
            }],
        };

        let request = InstallRequest {
            packages: Vec::new(),
            package_folder: None,
            release: "AmigaOS 3.2".to_string(),
            media_folder: folder,
            extra_media_folders: Vec::new(),
            media_folders: BTreeMap::new(),
            keymap: None,
            rom: None,
            chosen: vec!["a".to_string()],
            destination: dir.join("dist"),
            excluded: Vec::new(),
            scan_cache: Default::default(),
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
            layer: None,
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
            activate: vec![],
            exclusive_group: None,
            label_key: None,
            available: true,
            removes: Vec::new(),
        };
        let recipe = Recipe {
            layers: vec![],
            base: None,
            release: "Test".to_string(),
            components: vec![make("a", "A"), make("b", "B")],
        };

        let request = InstallRequest {
            packages: Vec::new(),
            package_folder: None,
            release: "AmigaOS 3.2".to_string(),
            media_folder: folder,
            extra_media_folders: Vec::new(),
            media_folders: BTreeMap::new(),
            keymap: None,
            rom: None,
            chosen: vec!["a".to_string(), "b".to_string()],
            destination: dir.join("dist"),
            excluded: Vec::new(),
            scan_cache: Default::default(),
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
            layer: None,
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
            activate: vec![],
            exclusive_group: Some("modules".to_string()),
            label_key: None,
            available: true,
            removes: Vec::new(),
        };
        let recipe = Recipe {
            layers: vec![],
            base: None,
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
            extra_media_folders: Vec::new(),
            media_folders: BTreeMap::new(),
            keymap: None,
            // major 40 < 47, so `modules-b` switches on by its own
            // condition — never named in `chosen`.
            rom: Some(crate::core::osinstall::fixtures::fake_rom(&dir, 40)),
            chosen: vec!["modules-a".to_string()],
            destination: dir.join("dist"),
            excluded: Vec::new(),
            scan_cache: Default::default(),
        };
        plan(&request, &recipe).unwrap()
    }

    /// Two members of one `exclusive_group`, each given its own `layer` and
    /// each `chosen` outright (no `Condition` involved — media resolution
    /// does not matter to this check either, so `media_folders` is left
    /// empty exactly like `the_shipped_322_recipe_plans_against_a_pre_47_rom_…`
    /// does: a **layered** recipe with no folder named for either layer
    /// resolves `layers` to an empty list and never touches the filesystem,
    /// same as that test). ART-238/ART-239: whether they conflict must
    /// depend on `overrides`, never on which layer either one names.
    fn plan_with_group_members_in_layers(
        layer_a: Option<&str>,
        layer_b: Option<&str>,
        b_overrides_a: bool,
    ) -> InstallPlan {
        let dir = crate::core::osinstall::fixtures::scratch("plan-exclusive-group-layers");
        let make = |id: &str, to: &str, layer: Option<&str>, overrides: Vec<String>| Component {
            layer: layer.map(str::to_string),
            id: id.to_string(),
            media: "Unused".to_string(),
            rules: vec![PathRule {
                from: "C/LoadModule".to_string(),
                to: to.to_string(),
                kind: RuleKind::File,
            }],
            required: false,
            condition: None,
            overrides,
            user_startup: vec![],
            activate: vec![],
            exclusive_group: Some("modules".to_string()),
            label_key: None,
            available: true,
            removes: Vec::new(),
        };
        let recipe = Recipe {
            // Non-empty so `Recipe::is_layered()` is true and `plan()` reads
            // `layer` at all — see this function's own doc comment.
            layers: vec![
                crate::core::osinstall::MediaLayer {
                    id: "base".to_string(),
                    label_key: None,
                },
                crate::core::osinstall::MediaLayer {
                    id: "update".to_string(),
                    label_key: None,
                },
            ],
            base: None,
            release: "Test".to_string(),
            components: vec![
                make("modules-a", "C/ModuleA", layer_a, Vec::new()),
                make(
                    "modules-b",
                    "C/ModuleB",
                    layer_b,
                    if b_overrides_a {
                        vec!["modules-a".to_string()]
                    } else {
                        Vec::new()
                    },
                ),
            ],
        };

        let request = InstallRequest {
            packages: Vec::new(),
            package_folder: None,
            release: "Test".to_string(),
            media_folder: dir.join("unused"),
            extra_media_folders: Vec::new(),
            media_folders: BTreeMap::new(),
            keymap: None,
            rom: None,
            chosen: vec!["modules-a".to_string(), "modules-b".to_string()],
            destination: dir.join("dist"),
            excluded: Vec::new(),
            scan_cache: Default::default(),
        };
        plan(&request, &recipe).unwrap()
    }

    /// **ART-239.** The bug the layer-scoped check could never catch: two
    /// members of one group, in two *different* layers, with no `overrides`
    /// relationship between them — a future update-layer Modules component
    /// for a different machine than the base layer's own, per the issue's
    /// own example. The old `(group, layer)` scoping put these two keys
    /// apart and never compared them at all.
    #[test]
    fn two_members_of_one_group_in_different_layers_without_overrides_still_conflict() {
        let plan = plan_with_group_members_in_layers(Some("base"), Some("update"), false);
        assert!(
            plan.refusals
                .iter()
                .any(|r| matches!(r, RefusalReason::ExclusiveGroupConflict { group, .. } if group == "modules")),
            "different layers must not excuse a genuine conflict when nothing \
             declares one component overrides the other: {:?}",
            plan.refusals
        );
    }

    /// The mirror image: two members of one group in different layers, but
    /// this time the second declares `overrides` over the first — the
    /// shipped 3.2.2 recipe's own shape for `modules-a1200` /
    /// `update-322-modules-a1200`, reduced to a synthetic recipe so this
    /// test does not depend on real media resolving. `overrides`, not the
    /// layer split, is what must excuse this pair.
    #[test]
    fn two_members_of_one_group_in_different_layers_with_overrides_is_not_a_conflict() {
        let plan = plan_with_group_members_in_layers(Some("base"), Some("update"), true);
        assert!(
            !plan
                .refusals
                .iter()
                .any(|r| matches!(r, RefusalReason::ExclusiveGroupConflict { .. })),
            "an overrides relationship must resolve the group regardless of \
             which layers the two members are declared in: {:?}",
            plan.refusals
        );
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
            layers: vec![],
            base: None,
            release: "Test".to_string(),
            components: vec![Component {
                layer: None,
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
                activate: vec![],
                exclusive_group: None,
                label_key: None,
                available: true,
                removes: Vec::new(),
            }],
        };

        let request = InstallRequest {
            packages: Vec::new(),
            package_folder: None,
            release: "AmigaOS 3.2".to_string(),
            media_folder: folder,
            extra_media_folders: Vec::new(),
            media_folders: BTreeMap::new(),
            keymap: None,
            rom: None,
            chosen: vec!["a".to_string()],
            destination: dir.join("dist"),
            excluded: Vec::new(),
            scan_cache: Default::default(),
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
            layers: vec![],
            base: None,
            release: "Test".to_string(),
            components: vec![Component {
                layer: None,
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
                activate: vec![],
                exclusive_group: None,
                label_key: None,
                available: true,
                removes: Vec::new(),
            }],
        };

        let request = InstallRequest {
            packages: Vec::new(),
            package_folder: None,
            release: "AmigaOS 3.2".to_string(),
            media_folder: folder,
            extra_media_folders: Vec::new(),
            media_folders: BTreeMap::new(),
            keymap: None,
            rom: None,
            chosen: vec!["a".to_string()],
            destination: scratch.join("dist"),
            excluded: Vec::new(),
            scan_cache: Default::default(),
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

    /// One claimant a `File` rule, one an `IconTooltypes` rule, over the
    /// same destination, with no `overrides` declared — `RuleKind::IconTooltypes`'s
    /// own doc comment promises it "participates in the destination-collision
    /// check exactly like a `File` rule", and this is that promise, exercised
    /// against `detect_collisions` itself rather than only asserted in a
    /// comment. Task 8's own AmigaOS 3.2.2 recipe does not exist yet, so this
    /// is a synthetic two-component recipe built for exactly this test —
    /// Task 8 runs the identical mutation (a `merge_icon` item quietly exempt
    /// from the collision check) against its own shipped recipe.
    fn plan_with_icon_rule_and_file_rule_colliding() -> InstallPlan {
        let dir = crate::core::osinstall::fixtures::scratch("plan-icon-collision");
        let folder = dir.join("media");
        std::fs::create_dir(&folder).unwrap();
        crate::core::osinstall::fixtures::media(
            &folder,
            "A",
            "a.adf",
            &[("Tools/IconEdit.info", b"one", 0)],
        );
        crate::core::osinstall::fixtures::media(
            &folder,
            "B",
            "b.adf",
            &[("Tools/IconEdit.info", b"two", 0)],
        );

        let recipe = Recipe {
            layers: vec![],
            base: None,
            release: "Test".to_string(),
            components: vec![
                Component {
                    layer: None,
                    id: "a".to_string(),
                    media: "A".to_string(),
                    rules: vec![PathRule {
                        from: "Tools/IconEdit.info".to_string(),
                        to: "Tools/IconEdit.info".to_string(),
                        kind: RuleKind::File,
                    }],
                    required: false,
                    condition: None,
                    overrides: vec![],
                    user_startup: vec![],
                    activate: vec![],
                    exclusive_group: None,
                    label_key: None,
                    available: true,
                    removes: Vec::new(),
                },
                Component {
                    layer: None,
                    id: "b".to_string(),
                    media: "B".to_string(),
                    rules: vec![PathRule {
                        from: "Tools/IconEdit.info".to_string(),
                        to: "Tools/IconEdit.info".to_string(),
                        kind: RuleKind::IconTooltypes,
                    }],
                    required: false,
                    condition: None,
                    // Deliberately undeclared — the point of the test.
                    overrides: vec![],
                    user_startup: vec![],
                    activate: vec![],
                    exclusive_group: None,
                    label_key: None,
                    available: true,
                    removes: Vec::new(),
                },
            ],
        };

        let request = InstallRequest {
            packages: Vec::new(),
            package_folder: None,
            release: "AmigaOS 3.2".to_string(),
            media_folder: folder,
            extra_media_folders: Vec::new(),
            media_folders: BTreeMap::new(),
            keymap: None,
            rom: None,
            chosen: vec!["a".to_string(), "b".to_string()],
            destination: dir.join("dist"),
            excluded: Vec::new(),
            scan_cache: Default::default(),
        };
        plan(&request, &recipe).unwrap()
    }

    #[test]
    fn an_icon_rule_and_a_file_rule_over_one_path_without_an_override_is_a_collision() {
        let plan = plan_with_icon_rule_and_file_rule_colliding();
        assert!(matches!(
            plan.refusals.as_slice(),
            [RefusalReason::DestinationCollision { path, components }]
                if path == "Tools/IconEdit.info" && components.len() == 2
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

    /// The same shape as [`a_file_rule_over_a_directory_is_a_kind_mismatch`],
    /// for `IconTooltypes`: it resolves against a media file exactly like a
    /// `File` rule, so a directory at `from` is refused by name rather than
    /// silently emitted as a `merge_icon` item nothing can actually merge.
    fn plan_with_an_icon_rule_over_a_directory() -> InstallPlan {
        let dir = crate::core::osinstall::fixtures::scratch("plan-kind-icon-over-dir");
        let folder = dir.join("media");
        std::fs::create_dir(&folder).unwrap();
        crate::core::osinstall::fixtures::media(&folder, "M", "m.adf", &[("C/inner", b"x", 0)]);

        let recipe = Recipe {
            layers: vec![],
            base: None,
            release: "Test".to_string(),
            components: vec![Component {
                layer: None,
                id: "a".to_string(),
                media: "M".to_string(),
                rules: vec![PathRule {
                    from: "C".to_string(),
                    to: "C".to_string(),
                    kind: RuleKind::IconTooltypes,
                }],
                required: false,
                condition: None,
                overrides: vec![],
                user_startup: vec![],
                activate: vec![],
                exclusive_group: None,
                label_key: None,
                available: true,
                removes: Vec::new(),
            }],
        };

        let request = InstallRequest {
            packages: Vec::new(),
            package_folder: None,
            release: "AmigaOS 3.2".to_string(),
            media_folder: folder,
            extra_media_folders: Vec::new(),
            media_folders: BTreeMap::new(),
            keymap: None,
            rom: None,
            chosen: vec!["a".to_string()],
            destination: dir.join("dist"),
            excluded: Vec::new(),
            scan_cache: Default::default(),
        };
        plan(&request, &recipe).unwrap()
    }

    #[test]
    fn an_icon_rule_over_a_directory_is_a_kind_mismatch() {
        let plan = plan_with_an_icon_rule_over_a_directory();
        assert!(matches!(
            plan.refusals.as_slice(),
            [RefusalReason::RuleKindMismatch { component, from, expected, found }]
                if component == "a"
                    && from == "C"
                    && *expected == RuleKind::IconTooltypes
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

    // ---- Activation (2026-08-23) ----

    fn planned_activation(from: &str) -> PlannedActivation {
        PlannedActivation {
            component: "storage".into(),
            name: from.rsplit('/').next().unwrap().into(),
            from: from.into(),
            to: format!("Devs/Monitors/{}", from.rsplit('/').next().unwrap()),
        }
    }

    fn item_at(to: &str, is_dir: bool) -> PlanItem {
        PlanItem {
            component: "storage".into(),
            media: "Storage3.2".into(),
            from: to.into(),
            to: to.into(),
            is_dir,
            bytes: 4,
            decompress: false,
            merge_icon: false,
        }
    }

    /// A switch whose source the plan really places is fine.
    #[test]
    fn an_activation_whose_source_is_placed_is_not_a_refusal() {
        let items = vec![item_at("Storage/Monitors/NTSC", false)];
        let refusals =
            detect_missing_activations(&items, &[planned_activation("Storage/Monitors/NTSC")]);
        assert!(refusals.is_empty(), "{refusals:?}");
    }

    /// **A drawer being placed is not the file being placed.**
    ///
    /// This test asserted the opposite when it was written, on the assumption
    /// that a `Subtree` rule produced one item for the drawer. It does not —
    /// `expand_rules` walks the medium and emits one item per file — so
    /// treating the drawer as sufficient accepted a monitor the medium does
    /// not carry. The end-to-end test found it.
    #[test]
    fn a_drawer_alone_does_not_satisfy_an_activation_inside_it() {
        let items = vec![item_at("Storage/Monitors", true)];
        let refusals =
            detect_missing_activations(&items, &[planned_activation("Storage/Monitors/NTSC")]);
        assert_eq!(refusals.len(), 1, "{refusals:?}");
    }

    /// Nothing places it, so the tree would get a `Devs/Monitors` entry
    /// copied from a file that is not on the disk. Refused **by name**, with
    /// the path nobody writes.
    #[test]
    fn an_activation_nothing_places_is_refused_by_name() {
        let items = vec![item_at("Storage/DOSDrivers", true)];
        let refusals =
            detect_missing_activations(&items, &[planned_activation("Storage/Monitors/NTSC")]);

        assert!(
            matches!(
                refusals.as_slice(),
                [RefusalReason::ActivationSourceMissing { component, name, from }]
                    if component == "storage" && name == "NTSC" && from == "Storage/Monitors/NTSC"
            ),
            "{refusals:?}"
        );
    }

    /// Compared through `destination_key` like every other destination
    /// question here: a Joliet-less disc yields `STORAGE/MONITORS` where an
    /// ADF yields `Storage/Monitors`, and those are one drawer (ART-012).
    #[test]
    fn the_source_is_matched_the_way_every_other_destination_is() {
        let items = vec![item_at("STORAGE/MONITORS/NTSC", false)];
        let refusals =
            detect_missing_activations(&items, &[planned_activation("Storage/Monitors/NTSC")]);
        assert!(refusals.is_empty(), "{refusals:?}");
    }

    /// A path that merely *starts* the same is not the file. Kept after the
    /// prefix rule was removed, because an exact check has to stay exact.
    #[test]
    fn a_path_that_only_shares_a_prefix_does_not_satisfy_it() {
        let items = vec![item_at("Storage/Monitors/NTSC-old", false)];
        let refusals =
            detect_missing_activations(&items, &[planned_activation("Storage/Monitors/NTSC")]);
        assert_eq!(refusals.len(), 1, "{refusals:?}");
    }

    /// A recipe that asks to switch something on, planned for real.
    ///
    /// `switched_on` is placed by the media; `absent` is not. Which one the
    /// component asks for is the caller's, so one helper serves both the
    /// refusal and the acceptance.
    fn plan_with_an_activation(tag: &str, ask_for: &str) -> InstallPlan {
        let dir = crate::core::osinstall::fixtures::scratch(tag);
        let folder = dir.join("media");
        std::fs::create_dir(&folder).unwrap();
        crate::core::osinstall::fixtures::media(
            &folder,
            "Shelf",
            "shelf.adf",
            &[("Storage/Monitors/NTSC", b"mon", 0)],
        );

        let recipe = Recipe {
            layers: vec![],
            base: None,
            release: "Test".to_string(),
            components: vec![Component {
                layer: None,
                id: "storage".to_string(),
                media: "Shelf".to_string(),
                rules: vec![PathRule {
                    from: "Storage/Monitors".to_string(),
                    to: "Storage/Monitors".to_string(),
                    kind: RuleKind::Subtree,
                }],
                required: true,
                condition: None,
                overrides: vec![],
                user_startup: vec![],
                activate: vec![super::super::Activation::Monitor {
                    name: ask_for.to_string(),
                }],
                exclusive_group: None,
                label_key: None,
                available: true,
                removes: Vec::new(),
            }],
        };

        let request = InstallRequest {
            packages: Vec::new(),
            package_folder: None,
            release: "AmigaOS 3.2".to_string(),
            media_folder: folder,
            extra_media_folders: Vec::new(),
            media_folders: BTreeMap::new(),
            keymap: None,
            rom: None,
            chosen: vec!["storage".to_string()],
            destination: dir.join("dist"),
            excluded: Vec::new(),
            scan_cache: Default::default(),
        };
        plan(&request, &recipe).unwrap()
    }

    /// **Through `plan()` itself**, not through the check in isolation.
    ///
    /// Written after a mutation survived: removing the *call* to
    /// `detect_missing_activations` broke nothing, because every test asked
    /// the function directly. A guard that does not cover its own call site
    /// is not a guard.
    #[test]
    fn plan_refuses_an_activation_whose_source_it_does_not_place() {
        let plan = plan_with_an_activation("plan-activation-missing", "PAL");
        assert!(
            plan.refusals.iter().any(|r| matches!(
                r,
                RefusalReason::ActivationSourceMissing { name, .. } if name == "PAL"
            )),
            "{:?}",
            plan.refusals
        );
        assert!(plan.items.is_empty(), "a refusal empties the plan");
        assert!(
            plan.activations.is_empty(),
            "and a switch to flip on a tree that will not be built"
        );
    }

    /// The other half: the one the media really carries is planned, carried
    /// on `InstallPlan::activations`, and resolved to both its ends.
    #[test]
    fn plan_carries_an_activation_whose_source_it_places() {
        let plan = plan_with_an_activation("plan-activation-ok", "NTSC");
        assert!(plan.refusals.is_empty(), "{:?}", plan.refusals);
        assert_eq!(plan.activations.len(), 1);
        let activation = &plan.activations[0];
        assert_eq!(activation.component, "storage");
        assert_eq!(activation.name, "NTSC");
        assert_eq!(activation.from, "Storage/Monitors/NTSC");
        assert_eq!(
            activation.to, "Devs/Monitors/NTSC",
            "the drawer AmigaOS actually reads"
        );
    }

    /// **Nothing shipped switches anything on.** Which monitor somebody
    /// wants, and whether their Amiga has a CD drive, are facts about
    /// somebody else's machine — the same reason `disable_bluetooth` is an
    /// option ART offers rather than something it writes unasked. A recipe
    /// quietly gaining one is a decision nobody made.
    #[test]
    fn no_shipped_recipe_switches_anything_on_by_itself() {
        for recipe in [
            super::super::recipe::amigaos_32().unwrap(),
            super::super::recipe::amigaos_39().unwrap(),
        ] {
            for component in &recipe.components {
                assert!(
                    component.activate.is_empty(),
                    "{} in {} asks to switch on {:?}",
                    component.id,
                    recipe.release,
                    component.activate
                );
            }
        }
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

    // ---- ART-119 (#5): a disk that is present and unreadable ----

    /// A damaged medium is a **refusal**, not a `CoreError` that takes the
    /// screen with it.
    ///
    /// This used to be `open_media(found_media)?`, so one unreadable disk
    /// failed `plan()` outright. The OS Builder made that worse than it
    /// sounds: it requests two plans through one `Promise.all`, one of them
    /// deliberately with nothing excluded, so a disk the user had *already
    /// excluded* still blanked both plans — including the one it was
    /// excluded from — and the screen showed a raw English `CoreError`
    /// sentence instead of a refusal card naming the disk (ART-060).
    ///
    /// Both halves are asserted here, because the second is the one a user
    /// meets: with the component excluded, the same folder plans completely.
    /// (An unexcluded plan's `items` are empty either way — *any* refusal
    /// empties the preview, which is this module's own pre-existing rule and
    /// applies to `MediaMissing` identically. What changed is that there now
    /// *is* a plan, carrying a named refusal, rather than no plan at all.)
    ///
    /// **The fixture is a real gap, not a contrived one.** `identify` reads
    /// a disc's name off its volume descriptor and stops (ART-161), while
    /// `open_media` walks the tree — so a disc past `MAX_WALK_DEPTH`
    /// (ART-158) is genuinely found by the scan, genuinely named from inside
    /// itself, and genuinely refused when something tries to read it. It is
    /// the same fixture `scan.rs`'s own
    /// `a_disc_is_identified_from_its_descriptor_without_walking_its_tree`
    /// uses to prove that gap exists.
    #[test]
    fn an_unreadable_disk_is_a_refusal_and_excluding_it_still_plans() {
        use crate::core::iso::fixture::{dir, file, IsoBuilder};

        let scratch = crate::core::osinstall::fixtures::scratch("plan-media-unreadable");
        let folder = scratch.join("media");
        std::fs::create_dir(&folder).unwrap();
        let recipe = crate::core::osinstall::recipe::amigaos_32().unwrap();

        let wb = crate::core::osinstall::fixtures::entries_for(&recipe, "Workbench3.2");
        let wb_refs: Vec<(&str, &[u8], u32)> = wb
            .iter()
            .map(|(path, bytes, protection)| (path.as_str(), bytes.as_slice(), *protection))
            .collect();
        crate::core::osinstall::fixtures::media(&folder, "Workbench3.2", "wb.adf", &wb_refs);
        crate::core::osinstall::fixtures::required_media(&folder, &recipe, &["Workbench3.2"]);

        // `Extras3.2` as a disc ART can name but cannot walk: seventeen
        // levels, one past `MAX_WALK_DEPTH`.
        let mut node = file("DEEP.TXT", "deep.txt", b"bottom");
        for level in (0..17).rev() {
            node = dir(&format!("L{level}"), &format!("L{level}"), vec![node]);
        }
        let bytes = IsoBuilder {
            volume: "Extras3.2".to_string(),
            joliet_volume: "Extras3.2".to_string(),
            joliet: true,
            children: vec![node],
            ..Default::default()
        }
        .build();
        let extras_path = folder.join("extras.iso");
        std::fs::write(&extras_path, bytes).unwrap();

        let request = |excluded: Vec<String>| InstallRequest {
            packages: Vec::new(),
            package_folder: None,
            release: "AmigaOS 3.2".to_string(),
            media_folder: folder.clone(),
            extra_media_folders: Vec::new(),
            media_folders: BTreeMap::new(),
            keymap: None,
            rom: Some(crate::core::osinstall::fixtures::fake_rom(&scratch, 47)),
            chosen: vec!["extras".to_string()],
            destination: scratch.join("dist"),
            excluded,
            scan_cache: Default::default(),
        };

        // `Ok`, not `Err` — the whole point. `unwrap` here *is* the
        // assertion: before this fix it panicked on the `LimitExceeded` the
        // walk raises.
        let refused = plan(&request(Vec::new()), &recipe).unwrap();

        let refusal = refused
            .refusals
            .iter()
            .find(|r| matches!(r, RefusalReason::MediaUnreadable { .. }))
            .unwrap_or_else(|| {
                panic!(
                    "expected a MediaUnreadable refusal, got {:?}",
                    refused.refusals
                )
            });
        match refusal {
            RefusalReason::MediaUnreadable {
                component,
                volume_name,
                path,
                reason,
            } => {
                assert_eq!(component, "extras");
                assert_eq!(volume_name, "Extras3.2");
                assert_eq!(path, &extras_path.display().to_string());
                // The reader's own sentence, not an empty string — "which
                // disk, and what is wrong with it" is the user's next
                // question, and the file name alone does not answer it.
                assert!(
                    reason.contains("16 levels"),
                    "the refusal must carry the reader's own diagnosis, got {reason:?}"
                );
            }
            other => unreachable!("{other:?}"),
        }
        // Named once, not once per rule the component declares.
        assert_eq!(
            refused
                .refusals
                .iter()
                .filter(|r| matches!(r, RefusalReason::MediaUnreadable { .. }))
                .count(),
            1,
            "{:?}",
            refused.refusals
        );

        // And the half a user actually meets: turn the damaged disk's
        // component off and the same folder plans completely. Before the fix
        // this plan could not even be requested — the *other* plan the screen
        // asks for alongside it hard-errored and blanked both.
        let excluded = plan(&request(vec!["extras".to_string()]), &recipe).unwrap();
        assert!(
            excluded.refusals.is_empty(),
            "excluding the damaged disk must leave nothing refused, {:?}",
            excluded.refusals
        );
        assert!(
            excluded
                .items
                .iter()
                .any(|i| i.component == "workbench-base"),
            "the readable disks must still plan, {:?}",
            excluded.items
        );
        assert!(
            !excluded.items.iter().any(|i| i.component == "extras"),
            "nothing may be planned from a disk that was never read, {:?}",
            excluded.items
        );
        assert!(!excluded.media_paths.contains_key("Extras3.2"));
    }

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
            extra_media_folders: Vec::new(),
            media_folders: BTreeMap::new(),
            keymap: None,
            rom: Some(crate::core::osinstall::fixtures::fake_rom(&dir, 40)),
            chosen: vec!["workbench-base".to_string()],
            excluded: vec!["modules-a1200".to_string()],
            destination: dir.join("dist"),
            scan_cache: Default::default(),
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
            extra_media_folders: Vec::new(),
            media_folders: BTreeMap::new(),
            keymap: None,
            rom: Some(crate::core::osinstall::fixtures::fake_rom(&dir, 47)),
            chosen: vec!["workbench-base".to_string(), "extras".to_string()],
            excluded: vec!["extras".to_string()],
            destination: dir.join("dist"),
            scan_cache: Default::default(),
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
            extra_media_folders: Vec::new(),
            media_folders: BTreeMap::new(),
            keymap: None,
            rom: Some(crate::core::osinstall::fixtures::fake_rom(&dir, 47)),
            chosen: vec![],
            excluded: vec!["workbench-base".to_string()],
            destination: dir.join("dist"),
            scan_cache: Default::default(),
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

    /// **F7.** Against the fixture's own bytes, not against the expression
    /// `plan()` used.
    ///
    /// This assertion used to read `assert_eq!(plan.total_bytes,
    /// content_bytes(&plan.items))`, which computes the identical expression
    /// on both sides — it agreed with a broken `plan()` exactly as readily as
    /// with a correct one, which is the same tautology the pre-ART-156
    /// version had and the same one this test was rewritten to escape.
    #[test]
    fn the_total_is_the_sum_of_what_will_actually_be_written() {
        let plan = plan_with(&["workbench-base"], &["Workbench3.2"]);

        // Every fixture file is `b"data"` — 4 bytes, from
        // `fixtures::entries_for` — so the total is grounded in the
        // fixture's own constant and the number of *file* items, neither of
        // which is the expression `plan()` evaluated.
        const FIXTURE_FILE_BYTES: u64 = 4;
        let files = plan.items.iter().filter(|item| !item.is_dir).count() as u64;
        let dirs = plan.items.iter().filter(|item| item.is_dir).count();

        assert_eq!(
            plan.total_bytes,
            FIXTURE_FILE_BYTES * files,
            "{files} file item(s) and {dirs} directory item(s): {:#?}",
            plan.items
        );
        // Pinned, so a recipe change is a failure somebody looks at rather
        // than a number that quietly follows whatever `plan()` now does.
        assert_eq!(plan.total_bytes, 44);
        assert_eq!(files, 11);
    }

    /// **ART-156.** A directory item's `bytes` never reaches the total, even
    /// when it is not zero.
    ///
    /// A directory sourced from an ADF reports `0`, so this could not be
    /// asked of the ADF fixtures at all — it was a CD that made the defect
    /// visible, where a directory is an extent with a declared length. Asked
    /// here of the arithmetic directly, over a hand-built item list carrying
    /// exactly the shape a disc produces, rather than only of the gated hook
    /// that needs the owner's own 469 MiB disc to run.
    #[test]
    fn a_directory_item_s_own_extent_length_is_not_content() {
        let items = vec![
            PlanItem {
                component: "workbench-base".into(),
                media: "AmigaOS39".into(),
                from: "C".into(),
                to: "C".into(),
                is_dir: true,
                // What an ISO9660 directory record declares for itself: a
                // real, sector-rounded number.
                bytes: 2048,
                decompress: false,
                merge_icon: false,
            },
            PlanItem {
                component: "workbench-base".into(),
                media: "AmigaOS39".into(),
                from: "C/Assign".into(),
                to: "C/Assign".into(),
                is_dir: false,
                bytes: 100,
                decompress: false,
                merge_icon: false,
            },
        ];
        assert_eq!(
            tree_totals(&items),
            (100, 1),
            "the drawer's 2048 extent bytes are not content, and it is not a file"
        );
    }

    /// **ART-205, at the arithmetic itself.** The end-to-end half is
    /// `apply.rs`'s `the_plan_predicts_the_bytes_the_tree_will_hold_not_the_bytes_it_reads`,
    /// which weighs a real tree on disk; this asks the one function directly,
    /// over the shape an `overrides` relationship produces — two items, one
    /// destination — so a regression is named here rather than only in a test
    /// that has to build a tree to see it.
    ///
    /// The **later** item's size is the one that survives, because that is
    /// the one whose bytes are on disk when `apply` finishes.
    #[test]
    fn a_destination_two_components_write_is_one_file_in_the_totals() {
        let item = |component: &str, to: &str, bytes: u64| PlanItem {
            component: component.into(),
            media: "Workbench3.2".into(),
            from: to.into(),
            to: to.into(),
            is_dir: false,
            bytes,
            decompress: false,
            merge_icon: false,
        };
        let items = vec![
            item("workbench-base", "C/Format", 100),
            item("classes", "C/Format", 250),
            item("classes", "C/New", 7),
        ];
        assert_eq!(
            tree_totals(&items),
            (257, 2),
            "one file at 250 (the overrider's, written last) plus one at 7"
        );
    }

    /// The same fold, through `destination_key` rather than through an exact
    /// string: a Joliet-less disc yields `C/ASSIGN` where a package's ZIP
    /// payload yields `C/Assign`, and `apply` writes **one** file — the same
    /// reason `detect_collisions` keys on it (and the reason ~211 of a real
    /// BoingBag's collisions were once invisible).
    #[test]
    fn two_spellings_of_one_destination_are_one_file_in_the_totals() {
        let item = |component: &str, to: &str, bytes: u64| PlanItem {
            component: component.into(),
            media: "AmigaOS39".into(),
            from: to.into(),
            to: to.into(),
            is_dir: false,
            bytes,
            decompress: false,
            merge_icon: false,
        };
        let items = vec![
            item("workbench-base", "C/ASSIGN", 100),
            item("boingbag-39-1", "C/Assign", 250),
        ];
        assert_eq!(tree_totals(&items), (250, 1));
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
            extra_media_folders: Vec::new(),
            media_folders: BTreeMap::new(),
            keymap: None,
            rom: Some(bad_rom),
            chosen: vec!["workbench-base".to_string()],
            destination: dir.join("dist"),
            excluded: Vec::new(),
            scan_cache: Default::default(),
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
        // **The second copy differs**, and that is the case this test is
        // about. Two byte-identical disks of one name are one disk
        // (`scan::dedupe_identical_disks`, 2026-08-25) - there is no decision
        // for anybody to make - so a fixture that made them identical would
        // now be asserting ambiguity over a question that has one answer.
        let mut differing = wb_refs.clone();
        differing.push(("C/OnlyHere", b"newer".as_slice(), 0));
        crate::core::osinstall::fixtures::media(
            &folder,
            "Workbench3.2",
            "wb-copy-2.adf",
            &differing,
        );

        let request = InstallRequest {
            packages: Vec::new(),
            package_folder: None,
            release: "AmigaOS 3.2".to_string(),
            media_folder: folder,
            extra_media_folders: Vec::new(),
            media_folders: BTreeMap::new(),
            keymap: None,
            rom: Some(crate::core::osinstall::fixtures::fake_rom(&dir, 47)),
            chosen: vec!["workbench-base".to_string()],
            destination: dir.join("dist"),
            excluded: Vec::new(),
            scan_cache: Default::default(),
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
            layer: None,
            activate: vec![],
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
            label_key: None,
            available: true,
            removes: Vec::new(),
        };
        let recipe = Recipe {
            layers: vec![],
            base: None,
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
            extra_media_folders: Vec::new(),
            media_folders: BTreeMap::new(),
            keymap: None,
            rom: None,
            // Reverse of the recipe's own declaration order — see the doc
            // comment above.
            chosen: vec!["gamma".to_string(), "beta".to_string(), "alpha".to_string()],
            destination: dir.join("dist"),
            excluded: Vec::new(),
            scan_cache: Default::default(),
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
            extra_media_folders: Vec::new(),
            media_folders: BTreeMap::new(),
            keymap: None,
            rom: None,
            chosen: Vec::new(),
            excluded: Vec::new(),
            packages: packages.iter().map(|s| s.to_string()).collect(),
            package_folder: package_folder.map(Path::to_path_buf),
            destination: dir.join("dist"),
            scan_cache: Default::default(),
        }
    }

    /// One more package of the same shape, so ordering and dependencies can
    /// be exercised without three more fixtures.
    fn extra_package(id: &str, media: &str, requires: &[&str]) -> Package {
        let component = Component {
            layer: None,
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
            activate: vec![],
            exclusive_group: None,
            label_key: None,
            available: true,
            removes: Vec::new(),
        };
        Package {
            id: id.to_string(),
            releases: vec!["AmigaOS 3.9".to_string()],
            name: id.to_string(),
            media: media.to_string(),
            member: None,
            distinguished_by: None,
            amiga_installer: None,
            requires: requires.iter().map(|s| s.to_string()).collect(),
            requires_components: Vec::new(),
            host_placement_block: None,
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
        assert_eq!(plan.packages, vec!["test-package".to_string()]);
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
        // Refused before the ordering could be worked out, so this is the
        // request's own list rather than a dependency order — stated in the
        // field's doc comment, and checked here so the two cannot drift.
        assert_eq!(plan.packages, vec!["pack-b".to_string()]);
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

    /// **M3 / ART-166.** A package ART cannot place from the host at all is
    /// refused by *type*, from the selection alone — before the package
    /// folder is scanned, before its archive is opened, and whatever else
    /// is wrong or right about the request. That ordering is the whole
    /// point: the sentence the user reads has to name what the package
    /// needs (its own Amiga-side `Updater`), not whatever the payload's
    /// reader happened to fail on first.
    #[test]
    fn a_package_that_cannot_be_placed_from_the_host_is_refused_by_type() {
        use super::super::HostPlacementBlock;

        let (dir, media, packages) = package_dirs("host-blocked");
        fixtures::package_test_archive(&packages, "pack.zip");

        let mut package = fixtures::package_test_package();
        package.host_placement_block = Some(HostPlacementBlock::EncryptedPayload);

        let request = package_request(&dir, &media, Some(&packages), &["test-package"]);
        let plan = plan_over(&request, &fixtures::package_test_recipe(), &[package]).unwrap();

        assert!(plan
            .refusals
            .contains(&RefusalReason::PackageNotPlaceableOnHost {
                package: "test-package".to_string(),
                block: HostPlacementBlock::EncryptedPayload,
            }));
        assert!(
            plan.items.is_empty(),
            "nothing of a package ART cannot place may reach the item list"
        );
    }

    /// The same request with the block removed plans cleanly — so the
    /// refusal above is about the block and not about the fixture.
    #[test]
    fn the_same_package_without_a_block_plans_cleanly() {
        let (dir, media, packages) = package_dirs("host-unblocked");
        fixtures::package_test_archive(&packages, "pack.zip");

        let package = fixtures::package_test_package();
        assert_eq!(package.host_placement_block, None);

        let request = package_request(&dir, &media, Some(&packages), &["test-package"]);
        let plan = plan_over(&request, &fixtures::package_test_recipe(), &[package]).unwrap();

        assert!(
            !plan
                .refusals
                .iter()
                .any(|r| matches!(r, RefusalReason::PackageNotPlaceableOnHost { .. })),
            "{:?}",
            plan.refusals
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

    // -----------------------------------------------------------------
    // Layered media (Task 3): resolution inside `plan()` itself.
    // -----------------------------------------------------------------

    /// A minimal layered recipe: one component per layer, both naming a
    /// volume the fixture writes into both folders.
    ///
    /// Built by hand rather than reaching for `AmigaOS 3.2.2` — that recipe
    /// does not exist until Task 8, and the behaviour under test is the
    /// layer mechanism itself, not the shipped recipe.
    fn two_layer_recipe() -> Recipe {
        crate::core::osinstall::recipe::parse(
            r#"{"release":"T","layers":[{"id":"base"},{"id":"up"}],
                "components":[
                  {"id":"a","media":"DiskDoctor","layer":"base","required":true,
                   "rules":[{"from":"C/DiskDoctor","to":"C/DiskDoctor","kind":"file"}]},
                  {"id":"b","media":"DiskDoctor","layer":"up","required":true,
                   "overrides":["a"],
                   "rules":[{"from":"C/DiskDoctor","to":"C/DiskDoctor","kind":"file"}]}
                ]}"#,
        )
        .unwrap()
    }

    /// A bare-minimum request, so a test that only cares about
    /// `media_folders` does not have to restate every other field.
    fn request_for_scratch(dir: &Path) -> InstallRequest {
        InstallRequest {
            release: "T".to_string(),
            media_folder: dir.join("unused"),
            extra_media_folders: Vec::new(),
            media_folders: BTreeMap::new(),
            keymap: None,
            rom: None,
            chosen: Vec::new(),
            excluded: Vec::new(),
            packages: Vec::new(),
            package_folder: None,
            destination: dir.join("dist"),
            scan_cache: Default::default(),
        }
    }

    #[test]
    fn two_layers_on_one_folder_refuse_by_naming_the_fields() {
        let dir = fixtures::scratch("plan-same-folder");
        let one = dir.join("everything");
        std::fs::create_dir_all(&one).unwrap();
        fixtures::media(&one, "DiskDoctor", "dd.adf", &[("C/DiskDoctor", b"x", 0)]);

        let request = InstallRequest {
            media_folders: BTreeMap::from([
                ("base".to_string(), one.clone()),
                ("up".to_string(), one.clone()),
            ]),
            ..request_for_scratch(&dir)
        };
        let plan = plan(&request, &two_layer_recipe()).unwrap();

        let same_folder = plan
            .refusals
            .iter()
            .find(|r| matches!(r, RefusalReason::LayersShareFolder { .. }))
            .expect("the refusal names the fields, not the disks");
        let RefusalReason::LayersShareFolder { layers, .. } = same_folder else {
            unreachable!()
        };
        assert_eq!(layers.len(), 2);
    }

    /// **Final review, Finding A.** `layers_sharing_a_folder` used to run
    /// unconditionally, over a list whose ids are the empty-string sentinel
    /// for every unlayered request (see `plan_over_with_cache`) — so naming
    /// one real folder twice (`media_folder` and an `extra_media_folders`
    /// entry pointed at the same place) refused with `LayersShareFolder {
    /// layers: ["", ""], .. }` for a release that declares no layers at all.
    /// That regressed the documented, shipped guarantee at
    /// `docs/FEATURES.md:197` — "one folder named twice is one folder" —
    /// which `find_media_across`'s canonical-path dedupe already gives an
    /// unlayered plan silently. Gating the check on `recipe.is_layered()`
    /// closes it; this pins the unlayered arm.
    #[test]
    fn an_unlayered_plan_naming_one_folder_twice_still_plans() {
        let dir = crate::core::osinstall::fixtures::scratch("plan-unlayered-duplicate-folder");
        let folder = dir.join("media");
        std::fs::create_dir(&folder).unwrap();
        let recipe = crate::core::osinstall::recipe::amigaos_32().unwrap();
        crate::core::osinstall::fixtures::required_media(&folder, &recipe, &[]);

        let request = InstallRequest {
            packages: Vec::new(),
            package_folder: None,
            release: "AmigaOS 3.2".to_string(),
            media_folder: folder.clone(),
            extra_media_folders: vec![folder.clone()],
            media_folders: BTreeMap::new(),
            keymap: None,
            rom: None,
            chosen: Vec::new(),
            excluded: Vec::new(),
            destination: dir.join("dist"),
            scan_cache: Default::default(),
        };

        let planned = plan(&request, &recipe).unwrap();
        assert!(
            !planned
                .refusals
                .iter()
                .any(|r| matches!(r, RefusalReason::LayersShareFolder { .. })),
            "one folder named twice on an unlayered release is one folder, not two \
             layers sharing it: {:?}",
            planned.refusals
        );
    }

    /// **Fix round 1, Finding 2.** An unlayered recipe (every shipped
    /// recipe until Task 8) has no real `MediaLayer::id` to report, and
    /// `""` is not one — writing a `LayerRecord { id: "", .. }` per media
    /// folder would give `distribution.json` two different spellings of
    /// "this tree carries no layers" (an absent key on an older tree, a
    /// list of empty-id records on a new one). An unlayered plan's own
    /// `layers` must be empty, matching how an older manifest reads back.
    #[test]
    fn an_unlayered_plan_reports_no_layers_at_all() {
        let plan = plan_with(&["workbench-base"], &["Workbench3.2"]);
        assert!(plan.refusals.is_empty(), "{:?}", plan.refusals);
        assert!(
            plan.layers.is_empty(),
            "an unlayered recipe has no real layer ids to report: {:?}",
            plan.layers
        );
    }

    // -----------------------------------------------------------------
    // Final whole-branch review, Finding B: the shipped 3.2.2 recipe
    // against a pre-47 Kickstart.
    // -----------------------------------------------------------------

    /// A synthetic Kickstart image carrying **both** facts a pre-3.2
    /// Kickstart states: a header major below 47 (what the base layer's own
    /// `modules-a1200` — `rom-older-than 47` — reads) and a readable
    /// `exec.library` resident older than 47.10 (what
    /// `update-322-modules-a1200` — `resident-older-than exec 47.10` —
    /// reads). A real Kickstart 3.1 (V40) satisfies both at once, which is
    /// the ordinary case for the real Amigas this project exists for — never
    /// a real dump, ART ships none.
    ///
    /// Built the same way `core::rom::mod::tests::rom_with_resident` builds
    /// its own fixture (a 512 KiB image, a `Resident` at a known offset,
    /// `rt_MatchTag` pointing at its own match word) — that helper is private
    /// to `core::rom`'s own test module, so this is a second, small copy
    /// rather than a visibility change to a module this one does not
    /// otherwise depend on.
    fn fake_pre_47_rom_with_old_exec(dir: &Path) -> PathBuf {
        const BASE: u32 = 0xF8_0000;
        let mut bytes = vec![0u8; 512 * 1024];
        // Header: major 40 (a Kickstart 3.1 / V40 header), minor arbitrary.
        bytes[12..14].copy_from_slice(&40u16.to_be_bytes());
        bytes[14..16].copy_from_slice(&68u16.to_be_bytes());

        // One `Resident`, naming `exec.library` at revision 40.10 — older
        // than the update's own `exec 47.10` floor.
        let offset = 0x400usize;
        let name = "exec.library";
        let id = "exec 40.10 (test)";
        let name_at = offset + 64;
        let id_at = offset + 128;
        bytes[offset..offset + 2].copy_from_slice(&0x4AFCu16.to_be_bytes());
        bytes[offset + 2..offset + 6].copy_from_slice(&(BASE + offset as u32).to_be_bytes());
        bytes[offset + 11] = 40;
        bytes[offset + 14..offset + 18].copy_from_slice(&(BASE + name_at as u32).to_be_bytes());
        bytes[offset + 18..offset + 22].copy_from_slice(&(BASE + id_at as u32).to_be_bytes());
        bytes[name_at..name_at + name.len()].copy_from_slice(name.as_bytes());
        bytes[id_at..id_at + id.len()].copy_from_slice(id.as_bytes());

        let path = dir.join("kick-pre47-with-old-exec.rom");
        std::fs::write(&path, &bytes).unwrap();
        path
    }

    /// **Final review, Finding B; the rule since ART-238/ART-239 is
    /// `overrides`, not layer.** Before the first fix,
    /// `detect_exclusive_group_conflicts` compared `exclusive_group` across
    /// the whole merged recipe with no `overrides` check at all, so a
    /// pre-47 Kickstart — which switches on *both* the inherited base
    /// `modules-a1200` (`rom-older-than 47`) and the update's own
    /// `update-322-modules-a1200` (`resident-older-than exec 47.10`), since
    /// both conditions are true of the same ROM — refused the whole plan
    /// with an `ExclusiveGroupConflict` naming two components the user
    /// never chose and cannot un-choose. That is not a hypothetical ROM: it
    /// is a real Kickstart 3.1, the ordinary case for the Amigas this
    /// project targets. What actually closes it is
    /// `update-322-modules-a1200`'s own `overrides: ["modules-a1200"]`
    /// (`recipes/amigaos-3.2.2.json`) — this pins the shipped recipe against
    /// a synthetic ROM that switches both components on, so deleting that
    /// `overrides` entry fails this test (ART-238's own missing guard).
    #[test]
    fn the_shipped_322_recipe_plans_against_a_pre_47_rom_without_an_exclusive_group_refusal() {
        let dir = crate::core::osinstall::fixtures::scratch("plan-322-pre47-rom");
        let rom = fake_pre_47_rom_with_old_exec(&dir);
        let recipe = crate::core::osinstall::recipe::by_release("AmigaOS 3.2.2")
            .expect("the shipped 3.2.2 recipe must load");

        let request = InstallRequest {
            packages: Vec::new(),
            package_folder: None,
            release: "AmigaOS 3.2.2".to_string(),
            media_folder: dir.join("unused"),
            extra_media_folders: Vec::new(),
            // No real media named: this test is about which components
            // *resolve on* for this ROM, not about whether their media can
            // be found — asserted below by checking the refusal list for
            // the one specific variant under test, never emptiness.
            media_folders: BTreeMap::new(),
            keymap: None,
            rom: Some(rom),
            chosen: Vec::new(),
            excluded: Vec::new(),
            destination: dir.join("dist"),
            scan_cache: Default::default(),
        };

        let planned = plan(&request, &recipe).unwrap();

        assert!(
            planned.components_on.iter().any(|id| id == "modules-a1200"),
            "the base layer's own modules-a1200 (rom-older-than 47) must be on for a \
             pre-47 header: {:?}",
            planned.components_on
        );
        assert!(
            planned
                .components_on
                .iter()
                .any(|id| id == "update-322-modules-a1200"),
            "update-322-modules-a1200 (resident-older-than exec 47.10) must be on for \
             this ROM's own exec.library: {:?}",
            planned.components_on
        );
        assert!(
            !planned
                .refusals
                .iter()
                .any(|r| matches!(r, RefusalReason::ExclusiveGroupConflict { .. })),
            "a base component and an update component for the same machine are two \
             halves of one release's answer, not a user's competing choice: {:?}",
            planned.refusals
        );
    }
}
