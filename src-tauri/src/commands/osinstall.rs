//! Installing AmigaOS from the user's own media (SD-2 · G5) — the adapter
//! layer over `core::osinstall`. Thin only: deserialize, call core, serialize
//! back.
//!
//! Five commands. `osinstall_scan_media` and `osinstall_plan` both end up
//! opening every candidate in the media folder — directly, or through
//! `plan::plan`'s own call to `scan::find_media` — and a missing or
//! unreadable folder is the single most likely mistake after a bad ROM
//! (`core/osinstall/scan.rs`'s own doc comment; ART-060's class of problem).
//! `find_media` itself raises that as a bare `CoreError`, which would reach
//! the screen as an English sentence if either command let it propagate.
//! Both turn it into a typed refusal here, at the command boundary, instead
//! — `core/osinstall/plan.rs` is left otherwise untouched; the translation
//! happens only on this side of the wire, the same way `commands/adf.rs`
//! and `commands/layout.rs` keep their own core modules free of anything
//! Tauri-shaped.
//!
//! `osinstall_components` is the fifth, and the newest: the checklist the
//! user ticks is now a projection of the chosen release's own recipe rather
//! than a list hand-written in the screen. Read-only, opens no media.
//!
//! `osinstall_apply` takes the plan it is given, the way `layout_apply` does
//! and `preload_run` does not (see `commands/layout.rs`'s own module note):
//! the user's component choices *are* the plan, so recomputing it here would
//! let the screen preview one install and build another.
//!
//! ## Fix round 1 — the outbound direction, pinned for real
//!
//! Review found a live wire mismatch (`VerifyReport::not_checked` had no
//! `camelCase` rename, so `src/lib/osinstall.ts`'s `report.notChecked` was
//! always `undefined`) that the Task 12 wire test could not have caught: that
//! test only deserialises a payload the frontend *sends* — the inbound
//! direction. Nothing pinned what Rust *serialises back out*. The
//! `wire_shapes` test module below fixes that: every response type is
//! checked with `serde_json::to_value` against the exact key names
//! `src/lib/osinstall.ts` declares, so a missing or wrong `rename_all` (or a
//! field renamed on one side only) fails a test instead of shipping.
//!
//! ## Task 7 fix round — the packages screen's own review
//!
//! Four things changed shape after the packages screen's first review, all
//! in this file:
//!
//! - **`osinstall_collisions` runs on a job thread, not the command thread**
//!   (F4). A real BoingBag extracts ~211 files; doing that synchronously
//!   inside the `#[tauri::command]` handler is exactly the "long operation
//!   on the command thread" §54 forbids. The actual work moved to
//!   [`preview_collisions`], run inside [`super::jobs::spawn_job`]; the
//!   command itself only starts the job and returns its id.
//!   [`extract_package_items`] also gained a cache (an archive's own
//!   extraction is reused, keyed on its path/mtime/len, across repeated
//!   previews of the same selection — no more re-reading a package's bytes
//!   on every checkbox toggle), a bound
//!   ([`MAX_PREVIEW_FILES`]/[`MAX_PREVIEW_BYTES`]), and
//!   [`sweep_stale_preview_scratch_dirs`] reaps anything a crash or a killed
//!   job left behind under `%TEMP%`.
//! - **`osinstall_add_package` sends a typed refusal, not `{:?}` text**
//!   (F2). [`resolve_packages_for_add`] now answers
//!   `Result<Vec<(Package, PathBuf)>, Vec<RefusalReason>>` rather than
//!   folding every refusal into one `CoreError::InvalidInput` sentence built
//!   from `format!("{r:?}")` — the six sentences Task 7 wrote in both
//!   catalogues were unreachable dead code until this. [`AddPackageResult`]
//!   carries the refusals across the wire; `src/lib/osinstall.ts` renders
//!   them through the same `refusalPhrase` mirror the install screen's own
//!   refusals already go through.
//! - **[`resolve_package_archive`] replaces two hand-written copies of the
//!   same `MediaMatch` match** (F11) — one in the old
//!   `extract_incoming_for_preview`, one in the old
//!   `resolve_packages_for_add`. Both now call the one function, so
//!   "missing" and "ambiguous" cannot quietly mean something different
//!   between the preview path and the add path.
//! - **`osinstall_add_package` emits its own outcome** (F10, half of it —
//!   the other half, resetting the screen's confirmation after a run, is
//!   `PackagePanel.tsx`'s). `OSINSTALL_ADD_PACKAGE_EVENT` carries the summed
//!   `ApplyOutcome` the way `OSINSTALL_EVENT` already does for a fresh
//!   install; before this the counts were written to the oplog and nowhere
//!   else, so the screen could say only "Added." with no numbers behind it.

use std::collections::{BTreeMap, BTreeSet, HashMap, VecDeque};
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, State};

use crate::core::error::{CoreError, CoreResult};
use crate::core::jobs::ProgressSink;
use crate::core::oplog::{JsonlOperationLog, OperationOutcome, OperationRecord};
use crate::core::osinstall::apply::{
    add_package_staging_in, apply_staging_in, refuse_unless_free, ApplyOutcome,
    DistributionManifest, FileRecord, RemovalState, RemovalVerdict, MANIFEST_FILE_NAME,
};
use crate::core::osinstall::chain::{self, FoundTree, TreeSummary};
use crate::core::osinstall::collide::{self, CollisionReport, Incoming};
use crate::core::osinstall::package::{self, Package};
use crate::core::osinstall::plan::{
    detect_package_refusals, expand_rules, plan_with_cache_in, InstallPlan, InstallRequest,
    PlanItem, ScanCachePolicy,
};
use crate::core::osinstall::recipe;
use crate::core::osinstall::scan::{
    self, find_media, find_packages, open_package, package_for, FoundMedia, FoundPackage,
    MediaMatch, PackageMedium,
};
use crate::core::osinstall::scan_cache::ScanCache;
use crate::core::osinstall::source::MediaSource;
use crate::core::osinstall::verify::{verify_volume, VerifyReport};
use crate::core::osinstall::{
    destination_key, host_destination, HostPlacementBlock, RefusalReason,
};
use crate::error::{AppError, AppResult};

use super::jobs::{spawn_job, spawn_job_in_lane, JobRegistry};
use super::oplog::{user_operation, write, write_to_path};

// ---------------------------------------------------------------------------
// osinstall_scan_media
// ---------------------------------------------------------------------------

/// What scanning a media folder found, or why it could not be looked at.
///
/// A refusal, not `find_media`'s own `CoreError` sentence — see the module
/// doc comment.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "outcome", rename_all = "kebab-case")]
pub enum MediaScanResult {
    Found {
        media: Vec<FoundMedia>,
    },
    /// The folder does not exist, or ART cannot read it.
    FolderUnreadable {
        folder: String,
    },
}

/// Every install disk `find_media` can open directly inside `folder` —
/// before any ROM or component has been chosen, so the screen can show what
/// it found the moment a folder is picked. Writes nothing.
///
/// Only `find_media`'s own `CoreError::Io` becomes `FolderUnreadable` — that
/// is the only variant it can actually produce (`std::fs::read_dir` and the
/// directory-listing iterator are its sole fallible steps; every other file
/// it looks at is skipped, never propagated — see the module doc on
/// `core/osinstall/scan.rs`). A blanket `Err(_)` would have silently
/// relabelled any future, differently-shaped error the same way; matching
/// the one variant that can occur means a new kind of failure surfaces as
/// itself instead of being folded into "bad folder".
/// Whether something already sits where the tree would be built.
///
/// Read-only, and asked while the destination is being chosen rather than
/// after the button is pressed. `apply()` refuses an existing destination —
/// `SAFE_CREATE`: a distribution tree is never built over one already there,
/// and that refusal has protected real data. But a refusal the user only
/// meets *after* committing to a long operation reads as the application
/// doing nothing: three consecutive attempts in one session's operation log
/// were refused for this exact reason, each one silent on screen. The engine
/// keeps the refusal; this lets the screen say it first.
///
/// A path that cannot be examined answers `false` — not because it is known
/// to be free, but because `apply()` is the one that decides, and guessing
/// "taken" here would block an install the engine would have allowed.
///
/// **ART-203.** This asks `apply`'s own question through `apply`'s own
/// function rather than a second, similar one. It used to be
/// `destination.try_exists()`, which was the same answer the engine gave —
/// and both were wrong in the same way: a folder picker can only return a
/// folder that exists, so every destination a user could choose read as taken.
/// One question, one implementation, so the screen and the engine cannot
/// disagree about which destinations are usable.
#[tauri::command]
pub fn osinstall_destination_taken(destination: PathBuf) -> AppResult<bool> {
    Ok(refuse_unless_free(&destination).is_err())
}

/// What a folder is, so a field can say it the moment it is picked (ART-199).
///
/// Read-only, and it **never fails for a folder that is not a tree** — that is
/// an answer, not an error. See `core::osinstall::chain::describe_tree`.
#[tauri::command]
pub fn osinstall_describe_tree(tree: PathBuf) -> AppResult<TreeSummary> {
    Ok(chain::describe_tree(&tree))
}

#[tauri::command]
pub fn osinstall_scan_media(folder: PathBuf) -> AppResult<MediaScanResult> {
    match find_media(&folder) {
        Ok(media) => Ok(MediaScanResult::Found { media }),
        Err(CoreError::Io(_)) => Ok(MediaScanResult::FolderUnreadable {
            folder: folder.display().to_string(),
        }),
        Err(other) => Err(other.into()),
    }
}

/// Every distribution tree directly inside `folder`, and what each carries
/// (ART-197 wave 2, row 1).
///
/// The artefact picker's own question. A folder of builds is the ordinary
/// case — the owner keeps several, differing by which components went in —
/// and until now the only way to tell them apart was to point a step at one
/// and see what it refused.
///
/// A folder that cannot be read **is** an error here, unlike
/// `osinstall_describe_tree`: the user has just pointed at it, so "that path
/// is gone" is the true sentence and there is no folder to describe.
#[tauri::command]
pub fn osinstall_trees_in(folder: PathBuf) -> AppResult<Vec<FoundTree>> {
    Ok(chain::trees_in(&folder)?)
}

/// Which shipped release these volume names are the install media of, if any
/// (ART-208) — asked when a folder holds media and the chosen release wants
/// none of it, so the screen can say "this is your AmigaOS 3.9 folder"
/// instead of listing sixteen absences.
///
/// Volume names, not a folder: the caller has already scanned, and re-reading
/// thirty-five ADFs to answer a question about names already in hand would be
/// a second 31 MB pass for nothing.
#[tauri::command]
pub fn osinstall_release_for_media(volume_names: Vec<String>) -> AppResult<Option<String>> {
    Ok(crate::core::osinstall::identify::release_holding(
        &volume_names,
    )?)
}

// ---------------------------------------------------------------------------
// osinstall_plan
// ---------------------------------------------------------------------------

/// What planning an install found, or why the media folder itself could not
/// be looked at.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "outcome", rename_all = "kebab-case")]
pub enum PlanResult {
    // `Box`ed: G9's `paired_rom` pushed `InstallPlan` past clippy's
    // `large_enum_variant` threshold next to `FolderUnreadable`'s bare
    // `String`. Serde serialises `Box<T>` exactly as it would `T`, so this
    // changes nothing on the wire.
    Planned { plan: Box<InstallPlan> },
    FolderUnreadable { folder: String },
}

/// What installing the chosen components would do — or every reason it
/// cannot. Writes nothing (§92's PREVIEW).
///
/// `request.release` names which shipped recipe to plan from
/// (`recipe::by_release`) — refused, never defaulted, when it names a
/// release ART ships no recipe for: a default here would mean a caller
/// asking for one operating system and getting another written onto the
/// user's volume.
///
/// **Opens the media folder twice** — once in the guard below, once again
/// inside `plan()`'s own call to `find_media`. Accepted rather than fixed:
/// the alternative is threading a pre-scanned `Vec<FoundMedia>` into `plan()`
/// itself, which is `core/osinstall/plan.rs`'s call to make, not this
/// adapter's, and a floppy-sized image's "windowed" read is cheap enough
/// (see `scan.rs`'s own module doc: the window *is* the whole file for
/// anything ADF-sized) that a real media folder costs a second pass of a few
/// milliseconds, not a second scan of the disk.
#[tauri::command]
pub fn osinstall_plan(request: InstallRequest) -> AppResult<PlanResult> {
    // The same folder `plan()` would open through `find_media` — checked
    // here first so a bad path reaches the screen as a value it can
    // translate, never as `find_media`'s own English sentence. See the
    // module doc comment. Narrowed to `CoreError::Io` for the same reason
    // `osinstall_scan_media` narrows it — see that command's own comment.
    // **Every folder the user named, not only the first** (work-list item 8).
    // A 3.2.2.1 install reads from three, and a plan that checked one of them
    // would refuse on the button with `find_media`'s own English sentence
    // instead of naming the folder that went away.
    //
    // A layered recipe's folders are `media_folders`'s values, one per layer;
    // an unlayered one keeps the flat `media_folder` + `extra_media_folders`
    // pair it always had. Checked by presence of `media_folders`, not by
    // asking the recipe, because this loop runs before `recipe::by_release`
    // below and has no recipe to ask yet.
    let folders_to_check: Vec<PathBuf> = if request.media_folders.is_empty() {
        std::iter::once(request.media_folder.clone())
            .chain(request.extra_media_folders.iter().cloned())
            .collect()
    } else {
        request.media_folders.values().cloned().collect()
    };
    for folder in &folders_to_check {
        if let Err(CoreError::Io(_)) = find_media(folder) {
            return Ok(PlanResult::FolderUnreadable {
                folder: folder.display().to_string(),
            });
        }
    }
    let recipe = recipe::by_release(&request.release)?;
    // ART-196: the cache and any nested package payload both stage under the
    // root the user chose, and a root that has gone away refuses here rather
    // than writing to the system drive behind their back.
    let scratch_root = crate::scratch::root()?;
    let cache = scan_cache_for(request.scan_cache, &scratch_root);
    cache.sweep();
    Ok(PlanResult::Planned {
        plan: Box::new(plan_with_cache_in(
            &request,
            &recipe,
            &cache,
            &scratch_root,
        )?),
    })
}

/// The scan cache this shell hands to `core::osinstall` — `%TEMP%`, beside the
/// extraction cache that `preview_cache_dir` already writes into.
///
/// **The directory is chosen here and not in `core/`** (ART-194). Where a
/// platform keeps scratch files is a shell question; `core::osinstall::plan`
/// takes a `ScanCache` the way a long core function takes a `ProgressSink`,
/// and `plan()` itself passes one that is switched off, so nothing in `core/`
/// writes to a directory nobody chose for it.
///
/// `sweep()` at every plan, matching how `sweep_stale_preview_scratch_dirs` is
/// called: cheap (a `stat` per entry), and the only thing that ever removes an
/// entry whose medium has gone away for good.
fn scan_cache_for(policy: ScanCachePolicy, scratch_root: &Path) -> ScanCache {
    match policy {
        ScanCachePolicy::Reuse => ScanCache::in_dir(scratch_root),
        ScanCachePolicy::Ignore => ScanCache::off(),
    }
}

/// Forget every medium listing ART is holding, so the next preview reads the
/// discs again. Answers how many were dropped.
///
/// **ART-194's escape hatch, and it is not a convenience.** The cache is keyed
/// on `(path, size, mtime)`, which catches a medium changed in place — but a
/// restored backup can preserve its timestamps, and several AmigaOS 3.9 ISOs
/// are in circulation. "Same path, same size, same mtime, different disc" is a
/// real arrangement, and against it the cache would answer with complete
/// confidence and be wrong. That is this project's most expensive failure
/// shape: it does not crash, it tells the user something untrue (§89). This
/// command is what a user reaches for when they suspect it, and it is what
/// makes the cheap key safe to trust the rest of the time.
///
/// Read-only with respect to the user's data — it removes only ART's own
/// derived files, under this module's own prefix, inside `%TEMP%`.
#[tauri::command]
pub fn osinstall_rescan_media() -> AppResult<usize> {
    Ok(ScanCache::in_dir(crate::scratch::root()?).forget_all())
}

// ---------------------------------------------------------------------------
// osinstall_components
// ---------------------------------------------------------------------------

/// One component of a shipped recipe, in the shape the checklist on screen
/// needs — never the whole [`Component`], whose `rules`, `overrides` and
/// `user_startup` the screen has no use for and would only be able to
/// misrepresent.
///
/// This exists because the screen used to carry its own hand-written copy of
/// the AmigaOS 3.2 catalogue and render it whatever release was chosen: with
/// 3.9 selected the user saw 26 components for a recipe that holds one, and
/// 3.9's own base component was labelled `Workbench3.2` — one operating
/// system's parts shown while another's were installed, which is §89 on the
/// screen itself. A projection of the recipe cannot drift from the recipe.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ComponentSummary {
    pub id: String,
    /// The volume name inside the image (`Workbench3.2`) — what the Amiga
    /// side calls it, shown untranslated as the row's own label **unless**
    /// [`ComponentSummary::label_key`] names one.
    pub media: String,
    /// The recipe's own i18n key for this row (ART-224), or `None` to fall
    /// back to `media`.
    ///
    /// Not a sentence and not a translation — the key itself, resolved on the
    /// screen. AmigaOS 3.9's five components all come off one disc, so `media`
    /// labelled every row `AmigaOS3.9`; this is how a recipe says which row is
    /// which without the command layer inventing a name for it.
    pub label_key: Option<String>,
    pub required: bool,
    pub available: bool,
    /// [`Condition::RomOlderThan`]'s `major`, flattened — `None` for an
    /// unconditional component **and for one conditioned the other way**.
    /// Flattened rather than mirrored because the screen's whole
    /// conditional-reason vocabulary ("switches on below Kickstart V47",
    /// "your ROM is newer, so it stays off") is written in terms of this one
    /// variant; the `match` below is exhaustive, so a further `Condition`
    /// variant is a compile error here and cannot silently arrive on screen
    /// wearing a sentence that means the opposite.
    pub condition_major: Option<u16>,
    /// [`Condition::RomAtLeast`]'s `major` (ART-157) — the Kickstart floor
    /// this component's own files need, `None` when it declares none.
    ///
    /// A separate field rather than a second meaning for `condition_major`,
    /// for the reason that field's own comment gives: the two numbers read
    /// alike and say opposite things, and the screen has to be able to tell
    /// them apart to say either out loud.
    pub requires_rom_major: Option<u16>,
    pub exclusive_group: Option<String>,
    /// Which components this one declares it may write over (ART-175).
    ///
    /// Carried to the screen so it can ask for a preview of exactly the
    /// components that can be in another's way, and no others: previewing
    /// every switched-on component would mean reading the whole install off
    /// media to answer a question about a few dozen files.
    ///
    /// Five components declare one in shipped data, read off the recipes
    /// rather than assumed: AmigaOS 3.2's `extras`, `modules-a1200`,
    /// `classes` and `glowicons` (which layers over four components at
    /// once), and AmigaOS 3.9's `workbench-39` — the one that makes a tree
    /// 3.9 rather than 3.5. `src/lib/osinstall.test.ts` pins that list.
    pub overrides: Vec<String>,
}

impl From<&crate::core::osinstall::Component> for ComponentSummary {
    fn from(component: &crate::core::osinstall::Component) -> Self {
        use crate::core::osinstall::Condition;
        Self {
            id: component.id.clone(),
            media: component.media.clone(),
            label_key: component.label_key.clone(),
            required: component.required,
            available: component.available,
            condition_major: component.condition.and_then(|condition| match condition {
                Condition::RomOlderThan { major } => Some(major),
                Condition::RomAtLeast { .. } => None,
            }),
            requires_rom_major: component.condition.and_then(|condition| match condition {
                Condition::RomAtLeast { major } => Some(major),
                Condition::RomOlderThan { .. } => None,
            }),
            exclusive_group: component.exclusive_group.clone(),
            overrides: component.overrides.clone(),
        }
    }
}

/// Which components `release`'s own shipped recipe holds, in recipe order —
/// the order `plan()` itself walks them in, so the checklist and the file
/// list below it agree without either sorting.
///
/// Read-only: parses shipped JSON, opens no media, writes nothing. An
/// unknown release is refused by [`recipe::by_release`], never defaulted,
/// for the reason that function's own doc comment gives.
#[tauri::command]
pub fn osinstall_components(release: String) -> AppResult<Vec<ComponentSummary>> {
    let recipe = recipe::by_release(&release)?;
    Ok(recipe
        .components
        .iter()
        .map(ComponentSummary::from)
        .collect())
}

// ---------------------------------------------------------------------------
// osinstall_packages
// ---------------------------------------------------------------------------

/// One shipped package, in the shape the checklist on screen needs.
///
/// `available` is **not** "ART knows how to install this" — it is "an
/// archive carrying this package's own top-level directory name was
/// actually found in `package_folder`". A checkbox for a package whose file
/// is absent is a promise ART cannot keep, so the screen needs this before
/// it ever offers the tick.
///
/// `host_placement_block` is the *other* half of that promise, and it is
/// deliberately a separate field rather than a second reason to say
/// `available: false` (M3 of the final whole-branch review): "your archive
/// is not in this folder" and "this package cannot be placed from Windows
/// at all, however many copies of it you have" are different sentences, and
/// folding them into one boolean is what produced a screen saying "Archive
/// not found" about a file sitting right there. `Some` here means the row
/// must not be tickable at all — see [`HostPlacementBlock`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PackageSummary {
    pub id: String,
    /// Shown on screen, unlocalized — a package's own name, not ART's
    /// sentence about it (ART-060).
    pub name: String,
    pub requires: Vec<String>,
    pub requires_components: Vec<String>,
    pub available: bool,
    /// `Some` when ART cannot place this package's files from the host at
    /// all — never a folder problem, always a property of the package.
    pub host_placement_block: Option<HostPlacementBlock>,
    /// Whether this package declares an installer of its own that ART can
    /// run **on the Amiga**
    /// ([`AmigaInstaller`](crate::core::osinstall::package::AmigaInstaller)),
    /// which is the other half of [`host_placement_block`](Self::host_placement_block):
    /// a BoingBag cannot be placed from Windows and *can* be run inside an
    /// emulator, and the Amiga-side panel offers exactly the packages this
    /// is true of.
    ///
    /// Read from the recipe rather than left for the screen to hardcode.
    /// Without it the panel would either list three packages and refuse one
    /// of them a moment later — "ART ships no Amiga-side installer for
    /// 'locale-turkish'", met after the pick rather than before it — or
    /// carry a list of ids that a fourth recipe would silently not join.
    pub amiga_installable: bool,
    /// Every entry name this package's own archive carries that
    /// [`safe_join`](crate::core::security::safe_join) refused — a `..`, an
    /// absolute path, a Windows prefix — exactly as the archive spelled it,
    /// and empty for the ordinary archive (m6 of the final whole-branch
    /// review: `ArchiveSource` has collected these since Task 6 and nothing
    /// had ever shown them, so a package carrying `..\..\Startup` read on
    /// screen as an ordinary package).
    ///
    /// The **outer** archive's, since that is the one `find_packages`
    /// opens to learn a package's identity at all; a nested payload's own
    /// refused names are not visible without extracting it, which the
    /// checklist deliberately does not do.
    pub refused_names: Vec<String>,
}

/// Every package ART ships a recipe for, paired with whether its archive was
/// actually found in `package_folder` — never just the shipped list on its
/// own (spec §5: this round installs the packages it ships recipes for and
/// nothing else, and even one of those three needs its own archive present
/// to be truthfully offered).
///
/// Read-only: an unreadable `package_folder` is answered the same way an
/// empty one would be — every package `available: false` — rather than
/// refused, so the checklist itself always renders and only the ticks
/// reflect what could actually be found. `osinstall_collisions` and
/// `osinstall_add_package` are what actually open the folder for real and
/// refuse by name when they cannot.
///
/// **Scoped to `release` (ART-209).** The owner chose AmigaOS 3.2 and was
/// offered BoingBags, which are 3.9's; 3.2 has none. This used to take a
/// folder and nothing else, so there was no release to scope by even in
/// principle — see `package::Package::releases`. A release ART ships no
/// packages for answers with an empty list, and the screen says so.
#[tauri::command]
pub fn osinstall_packages(
    package_folder: PathBuf,
    release: String,
) -> AppResult<Vec<PackageSummary>> {
    let found = find_packages(&package_folder).unwrap_or_default();
    let packages = package::packages_for(&release)?;
    Ok(packages
        .into_iter()
        .map(|p| {
            // Through `package_for`, not a hand-written comparison: the
            // checklist and the two paths that actually open an archive
            // (`plan()` and `resolve_package_archive`) must not be able to
            // disagree about what "found" means — which is exactly what the
            // old `f.media == p.media` did once `package_for` learnt to
            // fold case (m5).
            let matched: Vec<&FoundPackage> =
                match package_for(&found, &p.media, p.distinguished_by.as_deref()) {
                    MediaMatch::Missing => Vec::new(),
                    MediaMatch::Found(one) => vec![one],
                    MediaMatch::Ambiguous(many) => many,
                };
            PackageSummary {
                id: p.id,
                name: p.name,
                requires: p.requires,
                requires_components: p.requires_components,
                available: !matched.is_empty(),
                host_placement_block: p.host_placement_block,
                amiga_installable: p.amiga_installer.is_some(),
                // Every claimant's, not only the first: an ambiguous name
                // is still offered as available (the refusal comes later,
                // by name), so saying nothing about the *other* claimant's
                // traversing entries would be the same silence m6 is about.
                refused_names: matched
                    .iter()
                    .flat_map(|f| f.refused_names.iter().cloned())
                    .collect(),
            }
        })
        .collect())
}

// ---------------------------------------------------------------------------
// osinstall_collisions
// ---------------------------------------------------------------------------

/// A ceiling on how much one preview call pulls out of a chosen package set
/// — real update packages are floppy-era software (a BoingBag's ~211 files
/// sum to a few megabytes), so this is generous headroom, not a tuning
/// knob: crossing it means something is wrong with the archive or the
/// selection, not that a legitimate preview needs more room (F4).
const MAX_PREVIEW_FILES: usize = 20_000;
const MAX_PREVIEW_BYTES: u64 = 512 * 1024 * 1024;

/// The OS Builder's component preview, and the package preview, each hold a
/// **lane**: a newer job in one cancels and replaces the unfinished one before
/// it (`spawn_job_in_lane`).
///
/// **ART-195, and the numbers are the owner's own.** Both previews are started
/// from a `useEffect` that re-runs whenever the selection changes, and neither
/// cancelled its predecessor. One session left staging roots numbered up to
/// **2,149** under `%TEMP%` — five of them created inside two seconds — each
/// job walking the same 468 MB AmigaOS 3.9 ISO, all of them competing for one
/// drive. The unbounded re-firing itself was `useRemembered` handing back a
/// fresh array identity every render (`src/lib/useRemembered.ts`); these lanes
/// are the other half, and the half that still matters once a user simply
/// clicks four checkboxes quickly.
///
/// Two lanes rather than one: a component preview and a package preview answer
/// different questions about different media, and one lane would have each
/// cancelling the other.
const COMPONENT_PREVIEW_LANE: &str = "osinstall-component-preview";
const PACKAGE_PREVIEW_LANE: &str = "osinstall-package-preview";

/// Every scratch directory this module writes lives under this prefix, so
/// [`sweep_stale_preview_scratch_dirs`] can find them (and only them) inside
/// a shared `%TEMP%`.
const PREVIEW_SCRATCH_PREFIX: &str = "art-osinstall-collisions-";

/// How old one of this module's own scratch directories has to be before
/// [`sweep_stale_preview_scratch_dirs`] removes it. Long enough that a
/// preview genuinely still in flight is never swept out from under itself;
/// short enough that a crash or a killed job does not accumulate files
/// under `%TEMP%` across a whole day of use (F4's own "no sweep" finding).
const PREVIEW_SCRATCH_MAX_AGE: std::time::Duration = std::time::Duration::from_secs(60 * 60);

/// Best-effort: remove any of this module's own preview scratch directories
/// older than [`PREVIEW_SCRATCH_MAX_AGE`]. Never fails the call it runs
/// inside — a directory this pass misses (a transient I/O error, a
/// directory that changed under it) is swept the next time instead, and a
/// cache hit that turns out to point at a just-swept file is caught by
/// [`extract_package_items`]'s own existence check, never served as a wrong
/// answer.
fn sweep_stale_preview_scratch_dirs(scratch_root: &Path) {
    let Ok(entries) = std::fs::read_dir(scratch_root) else {
        return;
    };
    let now = std::time::SystemTime::now();
    for entry in entries.flatten() {
        if !entry
            .file_name()
            .to_string_lossy()
            .starts_with(PREVIEW_SCRATCH_PREFIX)
        {
            continue;
        }
        let Ok(metadata) = entry.metadata() else {
            continue;
        };
        if !metadata.is_dir() {
            continue;
        }
        let Ok(modified) = metadata.modified() else {
            continue;
        };
        let Ok(age) = now.duration_since(modified) else {
            continue;
        };
        if age > PREVIEW_SCRATCH_MAX_AGE {
            let _ = std::fs::remove_dir_all(entry.path());
        }
    }
}

/// One extracted incoming file: its destination (`to`), the id of the
/// component (package) it would come from, and a real path to its bytes.
type ExtractedItem = (String, String, PathBuf);

/// Identifies one archive's own extracted contents for
/// [`extract_package_items`]'s cache — the archive's path plus enough of
/// its own metadata (`mtime`, `len`) that a changed file (a re-downloaded or
/// hand-edited archive) is never served stale bytes from a cache keyed on
/// the path alone.
type PreviewCacheKey = (PathBuf, u64, u64, Option<String>);

fn preview_cache_key(archive_path: &Path, member: Option<&str>) -> CoreResult<PreviewCacheKey> {
    let metadata = std::fs::metadata(archive_path)?;
    let mtime_nanos = metadata
        .modified()
        .ok()
        .and_then(|m| m.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0);
    Ok((
        archive_path.to_path_buf(),
        mtime_nanos,
        metadata.len(),
        member.map(str::to_string),
    ))
}

/// Where one cache key's extraction lives on disk — deterministic (a hash
/// of the key, not a timestamp or a counter), so repeated preview calls for
/// the same archive identity reuse the same directory on disk rather than
/// growing a new one under `%TEMP%` on every checkbox toggle (F4's own "no
/// cache" finding).
fn preview_cache_dir(key: &PreviewCacheKey, scratch_root: &Path) -> PathBuf {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    key.hash(&mut hasher);
    scratch_root.join(format!("{PREVIEW_SCRATCH_PREFIX}{:016x}", hasher.finish()))
}

/// A ceiling on how many distinct archive identities [`PreviewCache`] keeps
/// at once (N2, Task 7's re-review: the cache was unbounded, so every
/// archive a user previewed across a whole run of ART stayed in memory, and
/// on disk, until [`sweep_stale_preview_scratch_dirs`]'s hourly sweep
/// eventually caught up). Generous: even a long session cycling through
/// every shipped package's own archive many times over touches far fewer
/// than this many distinct identities.
const MAX_PREVIEW_CACHE_ENTRIES: usize = 64;

/// The in-process cache: an archive's identity -> the items already
/// extracted for it. Reused across preview calls within one run of ART;
/// not persisted, and not needed to be — [`preview_cache_dir`]'s own
/// deterministic naming means a cold cache after a restart still finds the
/// same directory on disk if [`sweep_stale_preview_scratch_dirs`] has not
/// yet reaped it, and [`extract_package_items`] verifies every cached path
/// still exists before trusting it either way.
///
/// Bounded at [`MAX_PREVIEW_CACHE_ENTRIES`], evicted oldest-inserted-first.
/// `order` exists only because `HashMap` itself remembers no insertion
/// order to evict by; eviction only ever drops the in-memory entry, never
/// the directory `preview_cache_dir` names for it — that stays for
/// `sweep_stale_preview_scratch_dirs` to reap by age, which is what keeps a
/// re-inserted identity (evicted, then asked for again) able to find its
/// own bytes still on disk rather than starting cold.
#[derive(Default)]
struct PreviewCache {
    entries: HashMap<PreviewCacheKey, Vec<ExtractedItem>>,
    order: VecDeque<PreviewCacheKey>,
}

impl PreviewCache {
    fn get(&self, key: &PreviewCacheKey) -> Option<&Vec<ExtractedItem>> {
        self.entries.get(key)
    }

    fn insert(&mut self, key: PreviewCacheKey, value: Vec<ExtractedItem>) {
        if !self.entries.contains_key(&key) {
            self.order.push_back(key.clone());
        }
        self.entries.insert(key, value);
        while self.entries.len() > MAX_PREVIEW_CACHE_ENTRIES {
            let Some(oldest) = self.order.pop_front() else {
                break;
            };
            self.entries.remove(&oldest);
        }
    }
}

static PREVIEW_CACHE: OnceLock<Mutex<PreviewCache>> = OnceLock::new();

fn preview_cache() -> &'static Mutex<PreviewCache> {
    PREVIEW_CACHE.get_or_init(Default::default)
}

/// Resolve `package`'s own archive against `found`, as a typed refusal
/// rather than a hand-written English sentence — the
/// `PackageArchiveMissing`/`PackageArchiveAmbiguous` refusals `plan()`'s own
/// package block already raises for the identical situation, reused here so
/// the preview path and the add path cannot silently disagree about what
/// counts as "missing" or "ambiguous" (F11: this used to be two hand-written
/// copies of the same `MediaMatch` match, one in each path).
fn resolve_package_archive<'a>(
    package: &Package,
    found: &'a [FoundPackage],
) -> Result<&'a FoundPackage, RefusalReason> {
    match package_for(found, &package.media, package.distinguished_by.as_deref()) {
        MediaMatch::Found(archive) => Ok(archive),
        MediaMatch::Missing => Err(RefusalReason::PackageArchiveMissing {
            package: package.id.clone(),
            media: package.media.clone(),
        }),
        MediaMatch::Ambiguous(matches) => Err(RefusalReason::PackageArchiveAmbiguous {
            package: package.id.clone(),
            media: package.media.clone(),
            paths: matches
                .iter()
                .map(|m| m.path.display().to_string())
                .collect(),
        }),
    }
}

/// A plain-English sentence for a package ART cannot place from the host —
/// the preview path's own `CoreError` text, for the same reason and with
/// the same caveat as [`describe_package_refusal`] below: Rust-side strings
/// stay English whatever the chosen language (ART-060). The *translated*
/// sentence a user actually reads comes from `HostPlacementBlock` reaching
/// the screen as a value, through [`PackageSummary::host_placement_block`]
/// and through `RefusalReason::PackageNotPlaceableOnHost`.
fn describe_host_placement_block(package: &str, block: HostPlacementBlock) -> String {
    match block {
        HostPlacementBlock::EncryptedPayload => format!(
            "'{package}' cannot be placed from Windows: its payload archive is \
             password-encrypted, and only the package's own Amiga-side Updater \
             holds the password (ART-166)"
        ),
    }
}

/// A plain-English sentence for one package refusal — used only for the
/// preview path's own `CoreError` (the add path sends the `RefusalReason`
/// itself across the wire as data; see [`AddPackageResult`]). Rust-side
/// strings stay English regardless of the chosen language (ART-060), the
/// same rule every other `CoreError` message in this codebase already
/// follows.
fn describe_package_refusal(reason: &RefusalReason) -> String {
    match reason {
        RefusalReason::PackageArchiveMissing { package, media } => {
            format!("no archive carries '{media}', the media '{package}' needs")
        }
        RefusalReason::PackageArchiveAmbiguous {
            package,
            media,
            paths,
        } => format!(
            "more than one archive carries '{media}', the media '{package}' needs: {}",
            paths.join(", ")
        ),
        // `resolve_package_archive` only ever produces the two variants
        // above; kept total rather than narrowing the return type so a
        // future caller passing some other `RefusalReason` in still gets a
        // sentence instead of a panic.
        other => format!("{other:?}"),
    }
}

/// Every non-directory item one package would place, as real files on disk
/// — reusing a cached extraction when the archive has not changed since the
/// last preview (F4), and refusing rather than continuing once either of
/// [`MAX_PREVIEW_FILES`]/[`MAX_PREVIEW_BYTES`] is crossed.
///
/// Checks `progress.is_cancelled()` once per file (N5, Task 7's re-review:
/// the extraction previously ignored the job's own cancel flag entirely,
/// which meant asking to stop a two-hundred-file preview did nothing until
/// it finished on its own). Each file is a whole unit of work — written in
/// full or not opened at all — so this can never leave a half-written one,
/// the same discipline every other cancellable job in this codebase follows.
fn extract_package_items(
    package: &Package,
    archive: &FoundPackage,
    total_files: &mut usize,
    total_bytes: &mut u64,
    scratch_root: &Path,
    progress: &dyn ProgressSink,
) -> CoreResult<Vec<ExtractedItem>> {
    let key = preview_cache_key(&archive.path, package.member.as_deref())?;

    if let Some(cached) = preview_cache().lock().unwrap().get(&key) {
        // A hit is still verified against the real filesystem before being
        // trusted — a cache entry whose directory `sweep_stale_preview_scratch_dirs`
        // has since reaped is a miss, never a wrong answer.
        if cached.iter().all(|(_, _, path)| path.is_file()) {
            *total_files += cached.len();
            *total_bytes += cached
                .iter()
                .filter_map(|(_, _, path)| std::fs::metadata(path).ok())
                .map(|m| m.len())
                .sum::<u64>();
            return Ok(cached.clone());
        }
    }

    let medium = PackageMedium {
        path: archive.path.clone(),
        member: package.member.clone(),
    };
    let mut source = open_package(&medium)?;

    let mut refusals = Vec::new();
    let items = expand_rules(&package.component, source.as_mut(), &mut refusals)?;
    if !refusals.is_empty() {
        return Err(CoreError::InvalidInput(format!(
            "'{}' does not resolve against '{}': {} rule(s) did not match",
            package.id,
            archive.path.display(),
            refusals.len()
        )));
    }

    let dir = preview_cache_dir(&key, scratch_root);
    std::fs::create_dir_all(&dir)?;

    let mut extracted = Vec::new();
    let items: Vec<_> = items.into_iter().filter(|item| !item.is_dir).collect();
    for (counter, item) in items.into_iter().enumerate() {
        if progress.is_cancelled() {
            return Err(CoreError::Cancelled);
        }
        *total_files += 1;
        if *total_files > MAX_PREVIEW_FILES {
            return Err(CoreError::InvalidInput(format!(
                "the chosen packages would preview more than {MAX_PREVIEW_FILES} files at once"
            )));
        }
        let bytes = source.read(&item.from)?;
        *total_bytes += bytes.len() as u64;
        if *total_bytes > MAX_PREVIEW_BYTES {
            return Err(CoreError::InvalidInput(format!(
                "the chosen packages would preview more than {MAX_PREVIEW_BYTES} bytes at once"
            )));
        }
        let path = dir.join(counter.to_string());
        std::fs::write(&path, &bytes)?;
        progress.report(*total_files as u64, None, &item.to);
        extracted.push((item.to, item.component, path));
    }

    preview_cache()
        .lock()
        .unwrap()
        .insert(key, extracted.clone());
    Ok(extracted)
}

/// Every non-directory item a chosen, ordered package set would place,
/// across every package in `ordered` — see [`extract_package_items`] for
/// the per-package half, its own cache and its own bound.
///
/// Resolves each package's archive through [`resolve_package_archive`] and
/// expands its rules through [`expand_rules`] — the same function `plan()`
/// itself calls — rather than a second, nearly-identical resolver.
fn extract_incoming_for_preview(
    package_folder: &Path,
    ordered: &[String],
    catalogue: &[Package],
    scratch_root: &Path,
    progress: &dyn ProgressSink,
) -> CoreResult<Vec<ExtractedItem>> {
    let found = find_packages(package_folder)?;
    let mut total_files = 0usize;
    let mut total_bytes = 0u64;
    let mut incoming = Vec::new();

    for id in ordered {
        // Checked once per package too, not only once per file inside
        // `extract_package_items` — a cache hit resolves a whole package in
        // one call with no file-level loop of its own to check from.
        if progress.is_cancelled() {
            return Err(CoreError::Cancelled);
        }
        let package = catalogue
            .iter()
            .find(|p| &p.id == id)
            .expect("order() only ever returns ids it read from this same catalogue");
        let archive = resolve_package_archive(package, &found)
            .map_err(|r| CoreError::InvalidInput(describe_package_refusal(&r)))?;
        incoming.extend(extract_package_items(
            package,
            archive,
            &mut total_files,
            &mut total_bytes,
            scratch_root,
            progress,
        )?);
    }

    Ok(incoming)
}

/// The read-only work `osinstall_collisions` does, pulled out so it can be
/// unit-tested without a live `AppHandle`/`State`/job registry — the same
/// shape `resolve_packages_for_add` and `osinstall_verify`'s own `verify_at`
/// already use.
///
/// Run from a background job in the real command (F4 of Task 7's own fix
/// round): a real BoingBag extracts on the order of two hundred files, and
/// doing that synchronously inside the Tauri command handler is exactly the
/// "long operation on the command thread" §54 forbids.
fn preview_collisions(
    tree_root: &Path,
    package_folder: &Path,
    ordered: &[String],
    catalogue: &[Package],
    scratch_root: &Path,
    progress: &dyn ProgressSink,
) -> CoreResult<Vec<CollisionReport>> {
    if ordered.is_empty() {
        return Ok(Vec::new());
    }
    // The backstop for M3. The screen refuses the tick and never sends a
    // blocked package here, and `osinstall_add_package` refuses it by type
    // (`PackageNotPlaceableOnHost`) — but this command is reachable on its
    // own, and the whole point of ART-166 is that the *first* thing said
    // about such a package must name what it needs, not whatever its
    // payload's reader happened to fail on. Checked before a single archive
    // is opened.
    for id in ordered {
        let Some(package) = catalogue.iter().find(|p| &p.id == id) else {
            continue;
        };
        if let Some(block) = package.host_placement_block {
            return Err(CoreError::InvalidInput(describe_host_placement_block(
                &package.id,
                block,
            )));
        }
    }
    sweep_stale_preview_scratch_dirs(scratch_root);
    let incoming =
        extract_incoming_for_preview(package_folder, ordered, catalogue, scratch_root, progress)?;
    let entries: Vec<Incoming> = incoming
        .iter()
        .map(|(to, component, bytes_at)| Incoming {
            to: to.clone(),
            component: component.clone(),
            bytes_at,
        })
        .collect();
    collide::preview(tree_root, &entries)
}

// ---------------------------------------------------------------------------
// osinstall_component_collisions — ART-175
// ---------------------------------------------------------------------------
//
// The user-facing half of ART-170. `collide::preview` has been able to answer
// for a *release recipe's* component since `declared_override` started
// resolving component ids against releases as well as packages
// (`shipped_component_overrides`), and nothing asked it: the only thing that
// builds `Incoming` rows is `extract_incoming_for_preview`, which takes
// package ids and opens package archives.
//
// **Why a release component cannot reuse a package's shape, and what replaces
// it.** A BoingBag is previewed against a tree that already exists, so
// `preview` has real files to compare against and a `distribution.json`
// saying which component owns each. A release component has neither: the
// install screen builds a *new* tree (`apply` is `SAFE_CREATE` and refuses an
// existing root), so the thing `workbench-39` would replace is not a file on
// disk at all — it is `workbench-base`'s own item, in the same plan, not yet
// written anywhere.
//
// So the tree to preview against is **staged**, and only the part that
// matters: for each destination the previewed component claims that an
// *earlier* component in the same plan also claims, the earlier component's
// bytes are written into a scratch root at that destination, together with a
// `distribution.json` naming its owner. That is enough for `classify_incoming`
// to read both sides honestly and for `declared_override` to answer, and it is
// forty files for AmigaOS 3.9's overlay rather than the six hundred a full
// staged tree would cost. Nothing else about `preview` changes.
//
// Order is the plan's own order, which is recipe declaration order — the same
// order `apply` writes in, so "earlier" here means exactly "what `apply` would
// have put there first".

/// What previewing one or more switched-on components would do.
///
/// `reports` is `preview`'s own answer, unchanged. The count beside it exists
/// because a collision report is, by construction, a list of what *clashes* —
/// a component that lands six hundred files on nothing produces an empty
/// report, and "nothing to report" and "nothing to place" must not look the
/// same on screen (§89). `placed` is every non-directory item the chosen
/// components would write; `reports.len()` of those land on something another
/// component already claimed, and the rest are new.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ComponentPreview {
    pub reports: Vec<CollisionReport>,
    /// Every non-directory item the chosen components would place.
    pub placed: usize,
    /// How many of those land on a destination an **earlier** component in the
    /// same plan also claims.
    ///
    /// **Without this, "new" is a lie by exactly the number of identical
    /// files** (review F4). `collide::preview` drops `Identical` rows before
    /// returning — that is its own rule and a good one, since an identical
    /// file is nothing to warn about — so `placed - reports.len()` counts a
    /// file that lands byte-for-byte on another component's copy as *new*. On
    /// AmigaOS 3.9's overlay that is 130 files, and it is the difference
    /// between this preview's numbers and the ones
    /// `apply::tests::layer_the_real_39_overlay_when_asked` measured off the
    /// real disc.
    ///
    /// With it, the three counts the screen and the census both want are
    /// derivable and agree with ART-169's table:
    ///
    /// ```text
    /// new       = placed - contested        (landed on nothing)
    /// unchanged = contested - reports.len() (landed on identical bytes)
    /// replaced  = reports.len()
    /// ```
    pub contested: usize,
}

/// Every non-directory `PlanItem` belonging to one of `components`, in plan
/// order.
fn items_of<'a>(plan: &'a InstallPlan, components: &[String]) -> Vec<&'a PlanItem> {
    plan.items
        .iter()
        .filter(|item| !item.is_dir && components.iter().any(|id| id == &item.component))
        .collect()
}

/// Read one plan item's bytes out of the medium it names.
///
/// `sources` is opened lazily and keyed by volume name, the same way
/// `apply()` opens its own: a component's rules can name a hundred files on
/// one disk, and re-opening the image per file would re-walk a 469 MB disc a
/// hundred times (ART-161's own lesson, applied to the preview path).
fn read_from_media(
    plan: &InstallPlan,
    item: &PlanItem,
    sources: &mut BTreeMap<String, Box<dyn MediaSource>>,
) -> CoreResult<Vec<u8>> {
    if !sources.contains_key(&item.media) {
        let path = plan.media_paths.get(&item.media).ok_or_else(|| {
            CoreError::InvalidInput(format!(
                "this plan places '{}' from volume '{}' and records no path for it",
                item.to, item.media
            ))
        })?;
        // Through `identify`, never `AdfSource::open` — a plan's
        // `media_paths` carries a path and no `MediaKind`, and a real
        // AmigaOS 3.9 disc refuses the floppy reader outright (ART-153).
        let identified = scan::identify(path).ok_or_else(|| {
            CoreError::InvalidInput(format!(
                "'{}' no longer identifies as install media (expected volume '{}')",
                path.display(),
                item.media
            ))
        })?;
        sources.insert(item.media.clone(), scan::open_media(&identified)?);
    }
    let source = sources
        .get_mut(&item.media)
        .expect("inserted immediately above if it was absent");
    source.read(&item.from)
}

/// The two ceilings, in one place, so the staged half and the incoming half
/// cannot drift apart on which one they enforce.
fn check_preview_ceilings(files: usize, bytes: u64) -> CoreResult<()> {
    if files > MAX_PREVIEW_FILES {
        return Err(CoreError::InvalidInput(format!(
            "this preview would read more than {MAX_PREVIEW_FILES} files at once"
        )));
    }
    if bytes > MAX_PREVIEW_BYTES {
        return Err(CoreError::InvalidInput(format!(
            "this preview would read more than {MAX_PREVIEW_BYTES} bytes at once"
        )));
    }
    Ok(())
}

/// Where one component preview stages its scratch tree — **unique per call**.
///
/// It was deterministic (a hash of the plan's media and the components asked
/// about), on the reasoning that toggling a checkbox back and forth would
/// reuse one directory rather than growing one per click. That reasoning was
/// wrong twice over, and the review (F3) was right to call it: nothing ever
/// *reused* the directory, because the contents depend on media that may have
/// changed and so it was `remove_dir_all`'d on entry anyway — and two previews
/// of the same thing therefore shared a root, with the second wiping the
/// first's staging out from under it.
///
/// That is not a rare interleaving. [ART-178](../../../docs/ISSUES.md) makes
/// the OS Builder's plan effect settle **twice** with an identical request, so
/// two concurrent identical previews are the *normal* case, and there is no
/// `jobCancel` on the first when the second starts. The first preview then
/// finds its staged files gone, `classify_incoming` finds nothing at the
/// destination, and every Replace row silently degrades to "new" — a wrong
/// preview, in the screen a user reads before ticking the component that
/// decides which operating system they end up with.
///
/// So: process id plus a counter that never repeats within the process. That
/// is the same shape `core::test_scratch_id` uses and for the same measured
/// reason — a timestamp does not advance between two calls in one clock tick
/// on Windows, which is how `core::iso`, `core::cbm` and `net`'s test server
/// each lost a race before it (ART-115, ART-164, ART-173). This is the sixth
/// instance of that class, and the first outside a test.
///
/// Cleaned up by [`StagingDir`] rather than left for the hourly sweep, so a
/// preview's staged AmigaOS bytes do not sit in `%TEMP%` after it has
/// answered.
fn scratch_root_for(scratch_root: &Path) -> PathBuf {
    use std::hash::{Hash, Hasher};
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);

    // The thread is in the name as well as the process (ART-182). The counter
    // alone already makes every root unique, so this is not about collisions:
    // it is about being able to ask "which work left this behind" of a
    // directory found in `%TEMP%` after a crash, when previews run on job
    // threads and several can be in flight at once.
    //
    // It also gives the tests a namespace of their own. `cargo test` runs the
    // whole binary in ONE process on many threads, so a test that filtered on
    // the process id alone was counting sixteen other tests' in-flight
    // directories as its own — which is what made
    // `staging_is_removed_however_the_preview_ends` fail three runs in six.
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    std::thread::current().id().hash(&mut hasher);
    scratch_root.join(format!(
        "{PREVIEW_SCRATCH_PREFIX}component-{}-{:08x}-{}",
        std::process::id(),
        hasher.finish() as u32,
        NEXT.fetch_add(1, Ordering::Relaxed)
    ))
}

/// A staging root that removes itself, however its call ends.
///
/// **Why a guard and not a `remove_dir_all` at the end** (review F6). The
/// preview has half a dozen exits — two ceiling refusals, a cancel, every
/// `?` on a media read — and only one of them is the bottom of the function.
/// A cleanup written there runs on success and on nothing else, which is how
/// the staged bytes of a *cancelled* preview came to sit in `%TEMP%` until the
/// hourly sweep. `Drop` runs on all of them.
///
/// Best-effort: a directory that cannot be removed is left for
/// [`sweep_stale_preview_scratch_dirs`], which is what that sweep is for. It
/// must never turn a good preview into a failed one.
struct StagingDir(PathBuf);

impl Drop for StagingDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// The read-only work `osinstall_component_collisions` does, pulled out so it
/// can be unit-tested without a live `AppHandle`/`State`/job registry — the
/// same shape [`preview_collisions`] and `verify_at` already use.
///
/// Bounded by the same [`MAX_PREVIEW_FILES`]/[`MAX_PREVIEW_BYTES`] the package
/// preview uses, and cancelled between whole files, never mid-write.
fn preview_component_collisions(
    plan: &InstallPlan,
    components: &[String],
    scratch_root: &Path,
    progress: &dyn ProgressSink,
) -> CoreResult<ComponentPreview> {
    let incoming_items = items_of(plan, components);
    if incoming_items.is_empty() {
        return Ok(ComponentPreview::default());
    }

    sweep_stale_preview_scratch_dirs(scratch_root);
    // Unique per call, and removed however this call ends — see
    // `scratch_root_for` and `StagingDir`. Two concurrent previews of the same
    // components must not share a root: the second `remove_dir_all` would take
    // the first's staging with it and every Replace row would degrade to
    // "new" (review F3).
    let staging = StagingDir(scratch_root_for(scratch_root));
    let scratch = staging.0.clone();
    std::fs::create_dir_all(&scratch)?;

    // What an *earlier* component in the same plan would have put at each of
    // the previewed items' destinations.
    //
    // **Earlier means earlier in plan order, not merely "not selected"** — a
    // component that writes *after* the one being previewed is not in its
    // way, it is on top of it, and reporting it as the thing being replaced
    // inverts the answer (`a_later_component_is_not_what_is_being_replaced`
    // is the test that caught exactly that). So this is one pass in plan
    // order: an unselected item records itself as the current owner of its
    // destination, and a selected item takes whatever owner had been recorded
    // *by then*.
    //
    // Keyed by `destination_key`, not by the path as spelled: two entries the
    // filesystem and AmigaDOS agree are one file are one destination here too
    // (the defect `plan::detect_collisions`, `apply::undeclared_overwrites`
    // and `collide::preview` each had to fix separately).
    let mut owner_so_far: HashMap<String, &PlanItem> = HashMap::new();
    let mut replaces: HashMap<usize, &PlanItem> = HashMap::new();
    let mut incoming_index = 0usize;
    for item in plan.items.iter().filter(|item| !item.is_dir) {
        let key = destination_key(&item.to);
        if components.iter().any(|id| id == &item.component) {
            if let Some(existing) = owner_so_far.get(&key) {
                replaces.insert(incoming_index, existing);
            }
            incoming_index += 1;
        } else {
            owner_so_far.insert(key, item);
        }
    }

    let mut sources: BTreeMap<String, Box<dyn MediaSource>> = BTreeMap::new();
    let mut total_files = 0usize;
    let mut total_bytes = 0u64;

    // ---- stage the part of the tree that is actually in the way ----
    let mut manifest_files: Vec<FileRecord> = Vec::new();
    for (at, item) in incoming_items.iter().enumerate() {
        let Some(existing) = replaces.get(&at) else {
            continue;
        };
        if progress.is_cancelled() {
            return Err(CoreError::Cancelled);
        }
        let bytes = read_from_media(plan, existing, &mut sources)?;
        total_files += 1;
        total_bytes += bytes.len() as u64;
        check_preview_ceilings(total_files, total_bytes)?;

        // Staged under the **incoming** item's own spelling, not the
        // owner's. They are the same destination by construction — that is
        // what put them in `owner_of` — but only up to `destination_key`'s
        // case fold, and `classify_incoming` will look for the staged file
        // under `host_destination(root, entry.to)`. Using the incoming
        // spelling makes the two agree exactly rather than by courtesy of a
        // case-insensitive filesystem.
        let target = host_destination(&scratch, &item.to)?;
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&target, &bytes)?;
        manifest_files.push(FileRecord {
            path: existing.to.clone(),
            component: existing.component.clone(),
            media: existing.media.clone(),
            // `declared_override` reads only `path` and `component` out of
            // this. Stating a hash ART did not compute would be a claim about
            // bytes nobody verified, so the digest is left empty and the byte
            // count is the real one.
            sha256: String::new(),
            bytes: bytes.len() as u64,
            protection: None,
            // Nothing was overwritten to get here, and the host name is
            // whatever `host_destination` made of `to` on both sides alike —
            // recording one would be describing a tree this scratch root is
            // not.
            overwrote: None,
            host_path: None,
        });
    }

    // Counted before the manifest takes ownership of the rows: an item was
    // contested exactly when an earlier component's bytes were staged for it
    // to be compared against (review F4).
    let contested = manifest_files.len();

    // Written even when empty: its *absence* would reach `declared_override`
    // as an I/O error rather than as a "nothing was there" answer.
    let manifest = DistributionManifest {
        release: plan.release.clone(),
        built_from: Vec::new(),
        files: manifest_files,
        paired_rom: None,
        amiga_installed: Vec::new(),
    };
    std::fs::write(
        scratch.join(MANIFEST_FILE_NAME),
        serde_json::to_vec_pretty(&manifest).map_err(|err| CoreError::Malformed {
            format: "distribution manifest".into(),
            detail: err.to_string(),
        })?,
    )?;

    // ---- extract the incoming side ----
    //
    // Under `__incoming`, which is inside the scratch root but can never be a
    // destination: `host_destination` refuses anything that leaves the root,
    // and no AmigaDOS path in a shipped recipe begins with a double
    // underscore. Keeping both halves under one root is what lets the whole
    // preview be removed by one `remove_dir_all`.
    let bytes_dir = scratch.join("__incoming");
    std::fs::create_dir_all(&bytes_dir)?;
    let mut extracted: Vec<ExtractedItem> = Vec::with_capacity(incoming_items.len());
    for (counter, item) in incoming_items.iter().enumerate() {
        if progress.is_cancelled() {
            return Err(CoreError::Cancelled);
        }
        let bytes = read_from_media(plan, item, &mut sources)?;
        total_files += 1;
        total_bytes += bytes.len() as u64;
        check_preview_ceilings(total_files, total_bytes)?;

        let at = bytes_dir.join(counter.to_string());
        std::fs::write(&at, &bytes)?;
        progress.report(total_files as u64, None, &item.to);
        extracted.push((item.to.clone(), item.component.clone(), at));
    }

    let entries: Vec<Incoming> = extracted
        .iter()
        .map(|(to, component, bytes_at)| Incoming {
            to: to.clone(),
            component: component.clone(),
            bytes_at,
        })
        .collect();

    Ok(ComponentPreview {
        reports: collide::preview(&scratch, &entries)?,
        placed: extracted.len(),
        contested,
    })
}

/// The event a finished component preview arrives on.
pub const OSINSTALL_COMPONENT_COLLISIONS_EVENT: &str = "osinstall-component-collisions-result";

// `job_id`, not `jobId` — the same spelling every other result in this module
// uses, and the one `src/lib/osinstall.ts` declares.
#[derive(Debug, Clone, Serialize)]
pub struct OsInstallComponentCollisionsResult {
    pub job_id: u64,
    #[serde(flatten)]
    pub preview: ComponentPreview,
}

/// What switching these recipe components on would replace, file by file
/// (ART-175, §92's PREVIEW).
///
/// Takes the plan the screen is already showing rather than re-planning:
/// `osinstall_apply`'s own rule — the user's component choices *are* the plan,
/// and a screen that previewed one thing must not be able to describe another.
/// Returns a job id (§54): the preview reads every file the chosen components
/// would place, off real install media.
#[tauri::command]
pub fn osinstall_component_collisions(
    plan: InstallPlan,
    components: Vec<String>,
    app: AppHandle,
    registry: State<'_, Arc<JobRegistry>>,
) -> AppResult<u64> {
    let title = format!(
        "Previewing {} component(s) of {}",
        components.len(),
        plan.release
    );
    let emit_app = app.clone();
    let registry = Arc::clone(&registry);

    // Resolved here rather than inside the job: a scratch root that has
    // gone away is the user's to fix, and they should hear it from the
    // button they pressed (ART-196).
    let scratch_root = crate::scratch::root()?;

    let id = spawn_job_in_lane(
        &app,
        registry,
        &title,
        COMPONENT_PREVIEW_LANE,
        move |job_id, progress| {
            let preview =
                preview_component_collisions(&plan, &components, &scratch_root, progress)?;
            let _ = emit_app.emit(
                OSINSTALL_COMPONENT_COLLISIONS_EVENT,
                OsInstallComponentCollisionsResult { job_id, preview },
            );
            Ok(())
        },
    );

    Ok(id)
}

/// The event a finished collision preview arrives on.
pub const OSINSTALL_COLLISIONS_EVENT: &str = "osinstall-collisions-result";

// Deliberately not camelCased — `job_id` matches `OsInstallResult` and its
// siblings (`LayoutResult`, `PreloadResult`), and `src/lib/osinstall.ts`
// declares `job_id` to match.
#[derive(Debug, Clone, Serialize)]
pub struct OsInstallCollisionsResult {
    pub job_id: u64,
    pub reports: Vec<CollisionReport>,
}

/// What landing the chosen packages on `tree_root` would actually do to the
/// files already there (spec §3's PREVIEW). Returns a job id (§54) — see
/// [`preview_collisions`]'s own doc comment for why this now runs as a job
/// rather than answering synchronously. `src/lib/osinstall.ts`'s own
/// `osinstallCollisions` hides the job underneath its usual
/// `Promise<CollisionReport[]>` shape by awaiting
/// [`OSINSTALL_COLLISIONS_EVENT`] itself — nothing about the public TS
/// contract changed, only what runs behind it. A failed preview reaches the
/// screen through the ordinary `job-progress` failed state, the same as any
/// other job.
#[tauri::command]
pub fn osinstall_collisions(
    tree_root: PathBuf,
    package_folder: PathBuf,
    packages: Vec<String>,
    app: AppHandle,
    registry: State<'_, Arc<JobRegistry>>,
) -> AppResult<u64> {
    let catalogue = package::packages()?;
    let ordered = package::order(&packages)?;
    let title = format!(
        "Previewing {} package(s) against {}",
        ordered.len(),
        tree_root.display()
    );
    let emit_app = app.clone();
    let registry = Arc::clone(&registry);

    // Resolved here rather than inside the job: a scratch root that has
    // gone away is the user's to fix, and they should hear it from the
    // button they pressed (ART-196).
    let scratch_root = crate::scratch::root()?;

    let id = spawn_job_in_lane(
        &app,
        registry,
        &title,
        PACKAGE_PREVIEW_LANE,
        move |job_id, progress| {
            let reports = preview_collisions(
                &tree_root,
                &package_folder,
                &ordered,
                &catalogue,
                &scratch_root,
                progress,
            )?;
            let _ = emit_app.emit(
                OSINSTALL_COLLISIONS_EVENT,
                OsInstallCollisionsResult { job_id, reports },
            );
            Ok(())
        },
    );

    Ok(id)
}

// ---------------------------------------------------------------------------
// osinstall_add_package
// ---------------------------------------------------------------------------

/// `osinstall_add_package`'s own answer: either a job started, or every
/// typed reason it could not (F2 of Task 7's own fix round — see
/// [`resolve_packages_for_add`]). `src/lib/osinstall.ts` renders `refusals`
/// through the same `refusalPhrase` mirror the install screen's own
/// refusals already go through, instead of the Rust `{:?}` debug text a
/// plain `CoreError::InvalidInput` used to produce.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "outcome", rename_all = "kebab-case")]
pub enum AddPackageResult {
    Started { job_id: u64 },
    Refused { refusals: Vec<RefusalReason> },
}

/// [`resolve_packages_for_add`]'s own answer: `Ok` with the resolved
/// package/archive pairs, or `Err` with every typed refusal. A named alias
/// rather than the bare nested `Result` inline — clippy's own
/// `type_complexity` lint, and a second reader's, agree that a three-deep
/// generic is worth naming.
type PackageResolution = Result<Vec<(Package, PathBuf)>, Vec<RefusalReason>>;

/// The selection-resolution half of [`osinstall_add_package`], pulled out so
/// it can be unit-tested directly without a live `AppHandle`/`State` — the
/// same shape `osinstall_verify`'s own `verify_at` is factored out in, and
/// for the same reason: a `#[tauri::command]` needs a running Tauri app to
/// construct its `State` arguments, so the logic worth testing on its own
/// has to live somewhere a plain `#[test]` can reach.
///
/// `Ok(Ok(resolved))` is the happy path. `Ok(Err(refusals))` is every typed
/// reason the selection cannot proceed — an id ART ships no recipe for, a
/// `requires_components` this tree's own `distribution.json` does not
/// record (there is no recipe left to resolve once a tree already exists,
/// so the manifest is the only record of what is really on it), or a
/// missing/ambiguous archive — collected all at once, never stopping at the
/// first, the same rule `plan()` itself follows. The outer `AppResult` is
/// reserved for what is not a user selection problem at all: an unreadable
/// `distribution.json`, or a cycle in the shipped package data.
fn resolve_packages_for_add(
    tree_root: &Path,
    package_folder: &Path,
    packages: &[String],
) -> AppResult<PackageResolution> {
    if packages.is_empty() {
        return Err(CoreError::InvalidInput("no packages were chosen".into()).into());
    }

    let catalogue = package::packages()?;
    let manifest = read_manifest(tree_root)?;
    let components_on: Vec<String> = {
        let mut set: BTreeSet<String> = BTreeSet::new();
        for file in &manifest.files {
            set.insert(file.component.clone());
        }
        set.into_iter().collect()
    };

    let mut refusals = detect_package_refusals(packages, &catalogue, &components_on);

    let found = find_packages(package_folder)?;
    for id in packages {
        let Some(package) = catalogue.iter().find(|p| &p.id == id) else {
            // Already named by `PackageUnknown` above (from
            // `detect_package_refusals`); resolving an archive for an id
            // that names no shipped package would only repeat it.
            continue;
        };
        if let Err(refusal) = resolve_package_archive(package, &found) {
            refusals.push(refusal);
        }
    }

    if !refusals.is_empty() {
        return Ok(Err(refusals));
    }

    // Only reordered once the selection is known-good — `order` refuses an
    // unsatisfied `requires` or a cycle with its own English sentence
    // (ART-060 not fully paid off for this one case: a cycle is a bug in
    // the shipped data, not a user situation), and `detect_package_refusals`
    // above has already named the ordinary case — a `requires` that was not
    // itself chosen — by type.
    let ordered = package::order(packages)?;
    let mut resolved = Vec::new();
    for id in &ordered {
        let package = catalogue
            .iter()
            .find(|p| &p.id == id)
            .expect("order() only ever returns ids it read from this same catalogue")
            .clone();
        let archive = resolve_package_archive(&package, &found)
            .expect("every id in `packages` was already resolved without refusal above")
            .path
            .clone();
        resolved.push((package, archive));
    }

    Ok(Ok(resolved))
}

/// The event a finished (or failed) package-add job's own outcome arrives
/// on — F10: the counts used to be written to the oplog and nowhere else,
/// so the screen could say only "Added." with no numbers behind it. Mirrors
/// `OSINSTALL_EVENT`/`OsInstallResult` for a fresh install.
pub const OSINSTALL_ADD_PACKAGE_EVENT: &str = "osinstall-add-package-result";

// Deliberately not camelCased — see `OsInstallCollisionsResult`'s own
// comment just above for why.
#[derive(Debug, Clone, Serialize)]
pub struct OsInstallAddPackageResult {
    pub job_id: u64,
    pub outcome: ApplyOutcome,
}

/// Add every chosen package to a distribution tree that already exists — one
/// job for the whole set, not one per package and never one per file: the
/// screen asks its own single confirmation before this is ever called (spec
/// §3), and a job that stopped to ask again per file would teach a user to
/// click through it. Returns [`AddPackageResult`] — either a job id (§54),
/// with progress on the ordinary `job-progress` event and its outcome on
/// [`OSINSTALL_ADD_PACKAGE_EVENT`], or every typed refusal, resolved and
/// checked **before anything is written** (see
/// [`resolve_packages_for_add`]). Every per-file collision refusal —
/// `add_package`'s own undeclared-overwrite check — is `core`'s, unchanged;
/// this command adds nothing on top of it.
#[tauri::command]
pub fn osinstall_add_package(
    tree_root: PathBuf,
    package_folder: PathBuf,
    packages: Vec<String>,
    app: AppHandle,
    registry: State<'_, Arc<JobRegistry>>,
    oplog: State<'_, JsonlOperationLog>,
) -> AppResult<AddPackageResult> {
    let resolved = match resolve_packages_for_add(&tree_root, &package_folder, &packages)? {
        Ok(resolved) => resolved,
        Err(refusals) => return Ok(AddPackageResult::Refused { refusals }),
    };

    let root = tree_root.clone();
    let for_log = tree_root.display().to_string();
    let package_names: Vec<String> = resolved.iter().map(|(p, _)| p.id.clone()).collect();
    let title = format!("Adding {} package(s) to {for_log}", resolved.len());
    let log_path = oplog.path().to_path_buf();
    let emit_app = app.clone();

    // Resolved here rather than inside the job: a scratch root that has
    // gone away is the user's to fix, and they should hear it from the
    // button they pressed (ART-196).
    let scratch_root = crate::scratch::root()?;

    let id = spawn_job(
        &app,
        Arc::clone(&registry),
        &title,
        move |job_id, progress| {
            let mut total = ApplyOutcome {
                root: root.clone(),
                files: 0,
                directories: 0,
                bytes: 0,
                ..Default::default()
            };
            let mut failure: Option<CoreError> = None;
            for (package, archive) in &resolved {
                match add_package_staging_in(&root, package, archive, &scratch_root, progress) {
                    Ok(outcome) => {
                        total.files += outcome.files;
                        total.directories += outcome.directories;
                        total.bytes += outcome.bytes;
                    }
                    Err(err) => {
                        failure = Some(err);
                        break;
                    }
                }
            }

            let record = user_operation("Add update package(s) to an AmigaOS distribution tree")
                .source(package_folder.display().to_string())
                .destination(&for_log)
                .detail("Packages", package_names.join(", "));
            let record = match &failure {
                None => record
                    .detail("Files", total.files.to_string())
                    .detail("Directories", total.directories.to_string())
                    .detail("Bytes", total.bytes.to_string())
                    // A distribution tree is only ever verified against a real
                    // volume (`osinstall_verify`, run after the tree is copied
                    // onto one) — nothing has been read back here either.
                    .outcome(OperationOutcome::verified(false)),
                Some(err) => record.failure(err.code(), err.to_string()),
            };
            write_to_path(&log_path, &record);

            if let Some(err) = failure {
                return Err(err);
            }

            let _ = emit_app.emit(
                OSINSTALL_ADD_PACKAGE_EVENT,
                OsInstallAddPackageResult {
                    job_id,
                    outcome: total,
                },
            );
            Ok(())
        },
    );

    Ok(AddPackageResult::Started { job_id: id })
}

// ---------------------------------------------------------------------------
// osinstall_apply
// ---------------------------------------------------------------------------

/// `osinstall_apply`'s own request: the plan the screen showed, plus where it
/// goes. `InstallPlan` carries no destination of its own — `plan()` never
/// even reads `InstallRequest::destination` — and `apply()` takes `root`
/// separately, so this does too.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApplyRequest {
    pub plan: InstallPlan,
    pub destination: PathBuf,
}

/// The event a finished install arrives on.
pub const OSINSTALL_EVENT: &str = "osinstall-result";

#[derive(Debug, Clone, Serialize)]
pub struct OsInstallResult {
    pub job_id: u64,
    pub destination: String,
    pub outcome: ApplyOutcome,
}

/// One line per [`RemovalVerdict`], for [`osinstall_apply`]'s own oplog
/// record — never a raw `{:?}`, so the log reads the way every other detail
/// in this file does: a name, then a plain-English state.
///
/// `RemovalState::Failed`'s own sentence is carried too, because a removal
/// that failed is exactly the kind of thing an operator reading the log
/// later needs the reason for, not only the fact.
fn removed_detail(removed: &[RemovalVerdict]) -> String {
    removed
        .iter()
        .map(|verdict| {
            let state = match &verdict.state {
                RemovalState::Removed => "removed".to_string(),
                RemovalState::NotPresent => "not present".to_string(),
                RemovalState::Failed(detail) => format!("failed: {detail}"),
            };
            format!("{}: {state}", verdict.to)
        })
        .collect::<Vec<_>>()
        .join("; ")
}

/// Build the distribution tree. Returns a job id (§54) — an install copies an
/// entire operating system, and `apply()` already reports progress per file
/// it places (`sink.report(done, Some(total), &item.to)`, `core/osinstall/apply.rs`),
/// which reaches the screen through the ordinary `job-progress` event with
/// no extra plumbing here: `done`/`total` move item by item and `message`
/// names the file currently landing, so a bar that only moves at the end is
/// not what this produces.
#[tauri::command]
pub fn osinstall_apply(
    request: ApplyRequest,
    app: AppHandle,
    registry: State<'_, Arc<JobRegistry>>,
    oplog: State<'_, JsonlOperationLog>,
) -> AppResult<u64> {
    let destination = request.destination.display().to_string();
    let log_path = oplog.path().to_path_buf();
    let registry = Arc::clone(&registry);
    let emit_app = app.clone();
    let title = format!("Installing {} into {destination}", request.plan.release);
    let for_log = destination.clone();
    let plan = request.plan;
    let root = request.destination;

    // Resolved here rather than inside the job: a scratch root that has
    // gone away is the user's to fix, and they should hear it from the
    // button they pressed (ART-196).
    let scratch_root = crate::scratch::root()?;

    let id = spawn_job(&app, registry, &title, move |job_id, progress| {
        let outcome = apply_staging_in(&plan, &root, &scratch_root, progress);

        // Background jobs run on their own thread and cannot carry a Tauri
        // `State` across it, so this logs through `write_to_path` rather
        // than `write_result` — the same shape `layout_apply` and
        // `preload_run` already use for the identical reason.
        //
        // `source` is every medium the plan actually resolved, not the media
        // folder as a whole — `plan.media_paths` already names exactly the
        // files this run read from, so there is no reason to guess at a
        // single "the source" the way a one-image operation would.
        let source = plan
            .media_paths
            .values()
            .map(|path| path.display().to_string())
            .collect::<Vec<_>>()
            .join(", ");
        let record = user_operation("Build an AmigaOS distribution tree")
            .source(source)
            .destination(&for_log)
            .detail("Release", plan.release.clone())
            .detail("Components", plan.components_on.join(", "));
        let record = match &outcome {
            Ok(done) => {
                let record = record
                    .detail("Files", done.files.to_string())
                    .detail("Directories", done.directories.to_string())
                    .detail("Bytes", done.bytes.to_string());
                // Removals go through the log the same way placements do —
                // one record for the whole run, with a detail naming every
                // entry, never one log line per file (CLAUDE.md; the same
                // shape `commands/adf.rs` already uses for every other
                // operation this module logs). Omitted entirely when nothing
                // was asked to be removed, which is every shipped recipe
                // until AmigaOS 3.2.2's own recipe uses the field.
                let record = if done.removed.is_empty() {
                    record
                } else {
                    record.detail("Removed", removed_detail(&done.removed))
                };
                record
                    // Verification is its own step (`osinstall_verify`), run
                    // against the volume this tree is later copied onto — not
                    // here, where nothing has been read back yet.
                    .outcome(OperationOutcome::verified(false))
            }
            Err(err) => record.failure(err.code(), err.to_string()),
        };
        write_to_path(&log_path, &record);

        let outcome = outcome?;
        let _ = emit_app.emit(
            OSINSTALL_EVENT,
            OsInstallResult {
                job_id,
                destination: for_log,
                outcome,
            },
        );
        Ok(())
    });

    Ok(id)
}

// ---------------------------------------------------------------------------
// osinstall_verify
// ---------------------------------------------------------------------------

/// What `osinstall_verify` needs: which card partition to read, and where the
/// distribution tree's own manifest is.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VerifyRequest {
    pub image: PathBuf,
    pub slot: Option<usize>,
    pub index: usize,
    /// The distribution tree's own root — `apply()` wrote
    /// `distribution.json` (`MANIFEST_FILE_NAME`) there, read back by that
    /// name rather than asked of the caller.
    pub dist_root: PathBuf,
}

fn read_manifest(dist_root: &Path) -> AppResult<DistributionManifest> {
    let manifest_path = dist_root.join(MANIFEST_FILE_NAME);
    let text = std::fs::read_to_string(&manifest_path)?;
    serde_json::from_str(&text).map_err(|err| {
        AppError::from(CoreError::Malformed {
            format: "distribution manifest".into(),
            detail: err.to_string(),
        })
    })
}

fn verify_at(request: &VerifyRequest) -> AppResult<VerifyReport> {
    let manifest = read_manifest(&request.dist_root)?;
    Ok(verify_volume(
        &request.image,
        request.slot,
        request.index,
        &manifest,
    )?)
}

/// The record `osinstall_verify` writes, built from the outcome alone.
///
/// Factored out from the command (fix round 1) so requirement 5's own
/// property — `verified` is `failed == 0 && not_checked == 0`, **never**
/// `failed == 0` alone, because "ART did not look" is not "ART found nothing
/// wrong" (§89) — can be tested directly against a real `VerifyReport`
/// without a live Tauri `State` to write through. The record carries all
/// three counts as details too, so the log agrees with what the screen
/// shows, not just with the one boolean.
fn verify_record(
    dist_root: &str,
    image: &str,
    result: &AppResult<VerifyReport>,
) -> OperationRecord {
    let record = user_operation("Verify an AmigaOS install against its manifest")
        .source(dist_root)
        .destination(image);
    match result {
        Ok(report) => record
            .detail("Passed", report.passed.to_string())
            .detail("Failed", report.failed.to_string())
            .detail("Not checked", report.not_checked.to_string())
            .outcome(OperationOutcome::verified(
                report.failed == 0 && report.not_checked == 0,
            )),
        Err(err) => record.failure(err.code(), err.to_string()),
    }
}

/// Read the volume back and check it against the manifest `osinstall_apply`
/// wrote (§92's VERIFY step, Task 10's `verify_volume`). See
/// [`verify_record`] for what gets logged and why.
#[tauri::command]
pub fn osinstall_verify(
    request: VerifyRequest,
    oplog: State<'_, JsonlOperationLog>,
) -> AppResult<VerifyReport> {
    let image = request.image.display().to_string();
    let dist_root = request.dist_root.display().to_string();
    let result = verify_at(&request);

    write(&oplog, verify_record(&dist_root, &image, &result));

    result
}

// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::jobs::NoProgress;
    use crate::core::ScratchDir;

    /// **ART-203.** The screen asks this while the folder is being picked, and
    /// a folder picker can only hand back a folder that exists. If an empty
    /// one read as taken, every destination a user could choose would be
    /// blocked — which is what happened, and why no tree was ever built from
    /// the screen.
    #[test]
    fn an_empty_directory_is_not_taken() {
        let dir = ScratchDir::new("art-osinstall-cmd", "dest-empty");
        let root = dir.join("dist");
        std::fs::create_dir_all(&root).unwrap();
        assert!(!osinstall_destination_taken(root).unwrap());
    }

    #[test]
    fn a_directory_with_anything_in_it_is_taken() {
        let dir = ScratchDir::new("art-osinstall-cmd", "dest-occupied");
        let root = dir.join("dist");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("work.txt"), "mine").unwrap();
        assert!(osinstall_destination_taken(root).unwrap());
    }

    #[test]
    fn a_path_that_is_not_there_is_not_taken() {
        let dir = ScratchDir::new("art-osinstall-cmd", "dest-absent");
        assert!(!osinstall_destination_taken(dir.join("nothing-here")).unwrap());
    }

    /// The screen and the engine answer the same question through the same
    /// function, so they cannot drift apart — which is the defect this whole
    /// entry is about, seen from the other side.
    #[test]
    fn the_screen_and_the_engine_agree_about_every_shape() {
        let dir = ScratchDir::new("art-osinstall-cmd", "dest-agree");
        let empty = dir.join("empty");
        std::fs::create_dir_all(&empty).unwrap();
        let occupied = dir.join("occupied");
        std::fs::create_dir_all(&occupied).unwrap();
        std::fs::write(occupied.join("x"), "x").unwrap();
        let absent = dir.join("absent");

        for path in [empty, occupied, absent] {
            let engine_refuses = refuse_unless_free(&path).is_err();
            let screen_says_taken = osinstall_destination_taken(path.clone()).unwrap();
            assert_eq!(
                engine_refuses,
                screen_says_taken,
                "screen and engine disagree about {}",
                path.display()
            );
        }
    }

    /// **The wire, written down.** `src/lib/osinstall.ts` builds this object
    /// by hand; nothing else in either build checks that the two agree.
    #[test]
    fn the_payload_the_frontend_sends_deserialises() {
        let json = r#"{
            "mediaFolder": "E:\\media",
            "rom": "E:\\kick.rom",
            "chosen": ["workbench-base", "extras"],
            "excluded": ["modules-a1200"],
            "destination": "E:\\dist",
            "release": "AmigaOS 3.2"
        }"#;
        let request: InstallRequest = serde_json::from_str(json).unwrap();
        assert_eq!(request.chosen.len(), 2);
        assert_eq!(request.excluded, vec!["modules-a1200".to_string()]);
    }

    /// The checklist is the recipe, not a copy of one release's. Asserted
    /// against both shipped releases at once: they must differ, and each
    /// must be its own recipe's components in its own recipe's order. The
    /// defect this replaces was a hardcoded 3.2 list rendered for 3.9 —
    /// 26 rows for a one-component recipe, with 3.9's base component
    /// labelled `Workbench3.2`.
    #[test]
    fn the_component_list_is_the_chosen_releases_own_recipe() {
        for release in recipe::releases() {
            let recipe = recipe::by_release(release).unwrap();
            let listed = osinstall_components(release.to_string()).unwrap();
            assert_eq!(
                listed.iter().map(|c| c.id.as_str()).collect::<Vec<_>>(),
                recipe
                    .components
                    .iter()
                    .map(|c| c.id.as_str())
                    .collect::<Vec<_>>(),
                "{release}: same ids, same order as the recipe"
            );
            for (summary, component) in listed.iter().zip(&recipe.components) {
                assert_eq!(summary.media, component.media, "{release}/{}", summary.id);
                assert_eq!(
                    summary.required, component.required,
                    "{release}/{}",
                    summary.id
                );
                assert_eq!(
                    summary.available, component.available,
                    "{release}/{}",
                    summary.id
                );
            }
        }

        // The specific mislabelling the screen showed: both recipes carry a
        // component called `workbench-base`, and its media is *not* the same
        // volume in each.
        let base_media = |release: &str| -> String {
            osinstall_components(release.to_string())
                .unwrap()
                .into_iter()
                .find(|c| c.id == "workbench-base")
                .expect("every shipped recipe has a base component")
                .media
        };
        assert_ne!(
            base_media("AmigaOS 3.2"),
            base_media("AmigaOS 3.9"),
            "the two releases' base components name different media; a list \
             hardcoded to one of them labels the other wrongly"
        );
    }

    /// An unknown release is refused rather than answered with a default
    /// catalogue — the same rule `osinstall_plan` follows, and the reason
    /// this command takes a release at all instead of a boolean.
    #[test]
    fn an_unknown_release_has_no_component_list() {
        assert!(osinstall_components("AmigaOS 5.0".to_string()).is_err());
    }

    /// The request carries the release **on the wire** — that, and only
    /// that, is what this test checks: `InstallRequest.release` has no
    /// `#[serde(default)]`, so a payload omitting it fails to deserialise
    /// rather than quietly planning 3.2.
    ///
    /// The refusal of an *unknown* release is not here and was never here,
    /// though this comment used to claim it: the guarantee lives one layer
    /// down, in `recipe::by_release`'s exhaustive match, and is tested there
    /// (`recipe::tests::an_unknown_release_is_refused_by_name`).
    /// `osinstall_plan` and `osinstall_components` both reach it with `?`
    /// and neither adds a fallback of its own — which is the whole of what
    /// this adapter contributes to the guarantee.
    #[test]
    fn a_plan_request_names_the_release_it_wants() {
        let json = r#"{
            "mediaFolder": "C:\\nowhere",
            "rom": null,
            "chosen": [],
            "excluded": [],
            "destination": "C:\\nowhere\\dist",
            "release": "AmigaOS 3.9"
        }"#;
        let request: InstallRequest =
            serde_json::from_str(json).expect("the wire shape must carry release");
        assert_eq!(request.release, "AmigaOS 3.9");
    }

    /// The wire in the other direction: the plan `osinstall_plan` hands the
    /// screen has to be exactly what `osinstall_apply` accepts back, because
    /// `ApplyRequest` takes the plan it is given rather than recomputing it
    /// (see the module doc comment) — a plan that could serialize out but
    /// not deserialize back in would silently break that rule the moment a
    /// screen tried to apply what it was shown. Built from a real `plan()`
    /// run (`fixtures::planned_with`), not a hand-typed literal, so this
    /// exercises the whole struct — `items`, `media_paths`, `user_startup` —
    /// not just the fields a hand-written JSON blob happened to include.
    #[test]
    fn the_plan_the_frontend_sends_back_deserialises_into_an_apply_request() {
        let (plan, _dir) = crate::core::osinstall::fixtures::planned_with(
            &["workbench-base"],
            &["Workbench3.2"],
            Some(47),
        );
        assert!(plan.refusals.is_empty(), "{:?}", plan.refusals);
        let plan_json = serde_json::to_string(&plan).unwrap();

        let payload = format!(r#"{{"plan":{plan_json},"destination":"E:\\dist"}}"#);
        let request: ApplyRequest = serde_json::from_str(&payload).unwrap();

        assert_eq!(request.plan.release, "AmigaOS 3.2");
        assert_eq!(request.plan.items.len(), plan.items.len());
        assert_eq!(request.destination, PathBuf::from("E:\\dist"));
    }

    fn scratch(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "art-osinstall-cmd-{tag}-{}",
            crate::core::test_scratch_id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    // -------------------------------------------------------------------
    // osinstall_packages / osinstall_collisions / osinstall_add_package
    // (Task 7 — the screen packages became reachable from)
    // -------------------------------------------------------------------

    /// A minimal `distribution.json`, naming whichever files the test needs
    /// — the same shape `collide.rs`'s own `write_manifest` test helper
    /// uses, duplicated here rather than shared because that one is
    /// `#[cfg(test)]`-private to a different module.
    fn write_test_manifest(tree: &Path, files: Vec<crate::core::osinstall::apply::FileRecord>) {
        let manifest = DistributionManifest {
            release: "AmigaOS 3.9".into(),
            built_from: vec![crate::core::osinstall::apply::MediaRecord {
                volume_name: "OS-Version3.9".into(),
                sha256: "0".repeat(64),
            }],
            files,
            paired_rom: None,
            amiga_installed: Vec::new(),
        };
        std::fs::write(
            tree.join(MANIFEST_FILE_NAME),
            serde_json::to_string_pretty(&manifest).unwrap(),
        )
        .unwrap();
    }

    fn locale_base_file_record(path: &str) -> crate::core::osinstall::apply::FileRecord {
        crate::core::osinstall::apply::FileRecord {
            path: path.into(),
            component: "locale-base".into(),
            media: "OS-Version3.9".into(),
            sha256: "0".repeat(64),
            bytes: 0,
            protection: None,
            overwrote: None,
            host_path: None,
        }
    }

    /// `locale-turkish`'s own archive, shaped like the real one measured
    /// against `BoingBag39-2-turkce.lha`: loose files (no nested `member`)
    /// under `locale/catalogs`, lower-case, inside a top-level
    /// `LocaleUpdate` drawer (the package's own `media`).
    ///
    /// **ART-167 corrected this fixture, and the correction is the point.**
    /// It used to write `LocaleUpdate/locale/catalogs/x.catalog` — a
    /// catalog sitting directly in `catalogs`, with no language drawer at
    /// all. No real language pack looks like that: every one of the owner's
    /// eight puts its catalogs one level deeper, in a drawer named for the
    /// language (`scripts/lha-package-identity.py`, 2026-08-20), and that
    /// drawer is the *only* thing separating the eight from each other.
    /// A fixture without it could never have shown the bug.
    ///
    /// Now a real level-0 LHA with Latin-1 names, exactly as the owner's
    /// archives are — a ZIP with a UTF-8 name would have exercised a
    /// decoder no real package archive of this kind goes through.
    fn write_locale_turkish_archive(folder: &Path, file_name: &str, catalog_bytes: &[u8]) {
        let mut catalog: Vec<u8> = b"LocaleUpdate\\locale\\catalogs\\".to_vec();
        // `türkçe`, Latin-1 — the bytes in the real archive's own header.
        catalog.extend_from_slice(&[0x74, 0xFC, 0x72, 0x6B, 0xE7, 0x65]);
        catalog.extend_from_slice(b"\\x.catalog");
        std::fs::write(
            folder.join(file_name),
            crate::core::lha::tests::make_lha_with_raw_names(&[(&catalog, catalog_bytes)]),
        )
        .unwrap();
    }

    /// Where [`write_locale_turkish_archive`]'s one catalog lands on a
    /// tree, and where the base disc's own copy of it already sits — the
    /// destination `locale-turkish.json`'s single `subtree` rule produces.
    const TURKISH_CATALOG_ON_TREE: &str = "Locale/Catalogs/t\u{FC}rk\u{E7}e/x.catalog";

    /// The checklist always lists all three shipped packages, and never
    /// claims one is available when its own archive was never provided —
    /// "a checkbox for a package whose file is absent is a promise ART
    /// cannot keep."
    #[test]
    fn osinstall_packages_reports_whether_each_archive_was_actually_found() {
        let dir = scratch("packages-availability");
        let folder = dir.join("packages");
        std::fs::create_dir_all(&folder).unwrap();
        write_locale_turkish_archive(&folder, "turkish.lha", b"catalog bytes");

        let summaries = osinstall_packages(folder, "AmigaOS 3.9".to_string()).unwrap();
        assert_eq!(summaries.len(), 5, "ART ships exactly five packages today");

        let turkish = summaries
            .iter()
            .find(|p| p.id == "locale-turkish")
            .expect("locale-turkish is one of the three");
        assert!(turkish.available, "its own archive is right there");
        assert_eq!(turkish.requires_components, vec!["locale-base".to_string()]);

        for other in summaries.iter().filter(|p| p.id != "locale-turkish") {
            assert!(
                !other.available,
                "'{}' was never given an archive of its own",
                other.id
            );
        }
    }

    /// The Amiga-side panel offers exactly the packages whose recipes
    /// declare an installer, and it reads that from here rather than from a
    /// list of ids in TypeScript. Asserted **both ways**: the two BoingBags
    /// declare one, `locale-turkish` does not — a test that only checked
    /// the `true` side would pass against a field hardcoded to `true`,
    /// which would put a package on the panel that `compose` refuses by
    /// name a moment after it is picked.
    /// **ART-209.** The owner chose AmigaOS 3.2 and the update-packages
    /// panel still offered BoingBags — 3.9's archives, of which 3.2 has none:
    /// *"3.2 ile 3.9 secenekleri GUI'de karismis birbirine girmis."*
    ///
    /// The archive is deliberately **present** in the folder. The old
    /// behaviour would have reported it available and offered the tick; the
    /// question this test asks is not "was the file found" but "does this
    /// package belong on the release being built", and those are different
    /// questions that used to have one answer.
    #[test]
    fn osinstall_packages_offers_no_update_package_for_amigaos_32() {
        let dir = scratch("packages-release-scope");
        let folder = dir.join("packages");
        std::fs::create_dir_all(&folder).unwrap();
        write_locale_turkish_archive(&folder, "turkish.lha", b"catalog bytes");

        let for_32 = osinstall_packages(folder.clone(), "AmigaOS 3.2".to_string()).unwrap();
        assert!(
            for_32.is_empty(),
            "ART ships no update package for AmigaOS 3.2, got {:?}",
            for_32.iter().map(|p| &p.id).collect::<Vec<_>>()
        );

        // ...and the same folder, for the release these packages do belong
        // to, still offers all four. A filter that answered "none" for
        // everything would pass the assertion above.
        let for_39 = osinstall_packages(folder, "AmigaOS 3.9".to_string()).unwrap();
        assert_eq!(for_39.len(), 5);
    }

    #[test]
    fn osinstall_packages_says_which_packages_can_be_run_on_the_amiga() {
        let dir = scratch("packages-amiga-installable");
        let folder = dir.join("packages");
        std::fs::create_dir_all(&folder).unwrap();

        let summaries = osinstall_packages(folder, "AmigaOS 3.9".to_string()).unwrap();
        for summary in &summaries {
            let declares = package::by_id(&summary.id)
                .unwrap()
                .amiga_installer
                .is_some();
            assert_eq!(
                summary.amiga_installable, declares,
                "'{}' must report what its own recipe declares",
                summary.id
            );
        }
        assert!(
            summaries
                .iter()
                .any(|p| p.id == "boingbag-39-1" && p.amiga_installable),
            "BoingBag 3.9-1 declares C/Updater"
        );
        assert!(
            summaries
                .iter()
                .any(|p| p.id == "locale-turkish" && !p.amiga_installable),
            "a package with no Amiga-side installer must not be offered one"
        );
    }

    /// An unreadable/nonexistent package folder is answered the same way an
    /// empty one is — every package `available: false` — never refused: the
    /// checklist itself must still render.
    #[test]
    fn osinstall_packages_over_a_missing_folder_lists_nothing_as_available() {
        let dir = scratch("packages-missing-folder");
        let missing = dir.join("does-not-exist");

        let summaries = osinstall_packages(missing, "AmigaOS 3.9".to_string()).unwrap();
        assert_eq!(summaries.len(), 5);
        assert!(summaries.iter().all(|p| !p.available));
    }

    /// The preview a chosen package would produce against a tree that
    /// already exists — built the same way `plan()` builds a package's own
    /// items (`expand_rules`), and marked `declared` because
    /// `locale-turkish` names `overrides: ["locale-base"]` over exactly the
    /// component `distribution.json` records as this file's owner.
    #[test]
    fn osinstall_collisions_previews_a_real_package_against_an_existing_tree() {
        let dir = scratch("collisions-preview");
        let tree = dir.join("tree");
        let drawer = tree
            .join("Locale")
            .join("Catalogs")
            .join("t\u{FC}rk\u{E7}e");
        std::fs::create_dir_all(&drawer).unwrap();
        std::fs::write(drawer.join("x.catalog"), b"$VER: x.catalog 1.0 (1.1.20)").unwrap();
        write_test_manifest(
            &tree,
            vec![locale_base_file_record(TURKISH_CATALOG_ON_TREE)],
        );

        let packages_dir = dir.join("packages");
        std::fs::create_dir_all(&packages_dir).unwrap();
        write_locale_turkish_archive(
            &packages_dir,
            "turkish.lha",
            b"$VER: x.catalog 2.0 (1.1.21)",
        );

        // `preview_collisions`, not the `#[tauri::command]` itself — the
        // real command now needs a live `AppHandle`/`State` to spawn its
        // job (F4 of Task 7's own fix round), so this tests the same
        // read-only work directly, the way `resolve_packages_for_add` and
        // `osinstall_verify`'s own `verify_at` already are.
        let catalogue = package::packages().unwrap();
        let ordered = package::order(&["locale-turkish".to_string()]).unwrap();
        let reports = preview_collisions(
            &tree,
            &packages_dir,
            &ordered,
            &catalogue,
            &std::env::temp_dir(),
            &NoProgress,
        )
        .unwrap();

        assert_eq!(reports.len(), 1, "{reports:?}");
        assert_eq!(reports[0].path, TURKISH_CATALOG_ON_TREE);
        assert!(
            reports[0].declared,
            "locale-turkish declares overrides: [locale-base]"
        );
        assert!(
            matches!(
                reports[0].collision,
                crate::core::osinstall::collide::Collision::Upgrade { .. }
            ),
            "{:?}",
            reports[0].collision
        );
    }

    // ---- ART-175: previewing a release recipe's own component ----

    /// A plan whose two components both claim `C/Format`, built from two real
    /// ADFs so the bytes really are read off media the way `apply` would read
    /// them.
    ///
    /// `overrider` is the id to put on the *second* component. `workbench-39`
    /// declares `overrides: ["workbench-base"]` in the shipped AmigaOS 3.9
    /// recipe; passing anything else is how the undeclared case is reached
    /// without inventing a recipe.
    fn plan_over_two_media(
        tag: &str,
        overrider: &str,
        base_bytes: &[u8],
        overlay_bytes: &[u8],
    ) -> (PathBuf, InstallPlan) {
        let dir = scratch(tag);
        let folder = dir.join("media");
        std::fs::create_dir_all(&folder).unwrap();

        crate::core::osinstall::fixtures::media(
            &folder,
            "BaseDisk",
            "base.adf",
            &[("C/Format", base_bytes, 0u32)],
        );
        crate::core::osinstall::fixtures::media(
            &folder,
            "OverlayDisk",
            "overlay.adf",
            &[("C/Format", overlay_bytes, 0u32), ("C/New", b"NEW", 0u32)],
        );

        let mut media_paths = BTreeMap::new();
        media_paths.insert("BaseDisk".to_string(), folder.join("base.adf"));
        media_paths.insert("OverlayDisk".to_string(), folder.join("overlay.adf"));

        let item = |component: &str, media: &str, from: &str, to: &str, bytes: u64| PlanItem {
            component: component.to_string(),
            media: media.to_string(),
            from: from.to_string(),
            to: to.to_string(),
            is_dir: false,
            bytes,
            decompress: false,
            merge_icon: false,
        };

        let plan = InstallPlan {
            release: "AmigaOS 3.9".to_string(),
            // Declaration order is the order `apply` writes in, so the base
            // component is first here for the same reason it is first there.
            items: vec![
                item(
                    "workbench-base",
                    "BaseDisk",
                    "C/Format",
                    "C/Format",
                    base_bytes.len() as u64,
                ),
                item(
                    overrider,
                    "OverlayDisk",
                    "C/Format",
                    "C/Format",
                    overlay_bytes.len() as u64,
                ),
                item(overrider, "OverlayDisk", "C/New", "C/New", 3),
            ],
            refusals: Vec::new(),
            // Inert here — this fixture is about `preview`, which never
            // reads either total. Left at zero rather than computed, so a
            // reader is not invited to trust a number nothing checks.
            total_bytes: 0,
            total_files: 0,
            components_on: vec!["workbench-base".to_string(), overrider.to_string()],
            paired_rom: None,
            media_paths,
            packages: Vec::new(),
            package_media: BTreeMap::new(),
            user_startup: Vec::new(),
            activations: Vec::new(),
            media_stamps: BTreeMap::new(),
            removals: Vec::new(),
        };
        (dir, plan)
    }

    /// **The issue itself.** `collide::preview` could already answer for a
    /// release recipe's component ([ART-170]) and nothing asked it. This is
    /// the ask: switching `workbench-39` on and being told, file by file,
    /// what it would replace.
    ///
    /// Two things are asserted that a count alone would not catch — that the
    /// row is an *upgrade* (so both files' `$VER:` strings were really read,
    /// off real media, rather than a size being compared), and that the file
    /// landing on nothing produces no row at all while still being counted in
    /// `placed`. "Nothing to report" and "nothing to place" must not look the
    /// same on screen.
    #[test]
    fn switching_a_component_on_previews_what_it_would_replace() {
        let (dir, plan) = plan_over_two_media(
            "component-preview",
            "workbench-39",
            b"$VER: format 44.5 (1.1.99)",
            b"$VER: format 45.1 (1.1.00)",
        );

        let preview = preview_component_collisions(
            &plan,
            &["workbench-39".to_string()],
            &std::env::temp_dir(),
            &NoProgress,
        )
        .unwrap();

        assert_eq!(preview.placed, 2, "both of the overlay's files are placed");
        assert_eq!(preview.reports.len(), 1, "{:?}", preview.reports);
        assert_eq!(preview.reports[0].path, "C/Format");
        assert!(
            preview.reports[0].declared,
            "workbench-39 declares overrides: [workbench-base] in the shipped recipe"
        );
        match &preview.reports[0].collision {
            crate::core::osinstall::collide::Collision::Upgrade { from, to } => {
                assert_eq!(from, "44.5");
                assert_eq!(to, "45.1");
            }
            other => panic!("expected an upgrade read off both files' own $VER:, got {other:?}"),
        }

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// **An identical file is not a new one** (review F4).
    ///
    /// `collide::preview` drops `Identical` rows before returning, which is
    /// its own rule and a good one — an identical file is nothing to warn
    /// about. But it means `placed - reports.len()` counts such a file as
    /// *new*, and on AmigaOS 3.9's overlay that is 130 files: the exact gap
    /// between this preview's numbers and the 622 that
    /// `apply::tests::layer_the_real_39_overlay_when_asked` measured off the
    /// real disc. `contested` is what makes the three counts add up and the
    /// two hooks comparable.
    #[test]
    fn an_identical_file_counts_as_unchanged_and_never_as_new() {
        let same = b"$VER: format 44.5 (1.1.99)";
        let (dir, plan) =
            plan_over_two_media("component-preview-counts", "workbench-39", same, same);

        let preview = preview_component_collisions(
            &plan,
            &["workbench-39".to_string()],
            &std::env::temp_dir(),
            &NoProgress,
        )
        .unwrap();

        // Two files placed: `C/Format` lands on `workbench-base`'s identical
        // copy, `C/New` lands on nothing.
        assert_eq!(preview.placed, 2);
        assert_eq!(preview.contested, 1, "one of them had something under it");
        assert_eq!(
            preview.reports.len(),
            0,
            "and it was identical, so nothing to report"
        );

        let replaced = preview.reports.len();
        let unchanged = preview.contested - replaced;
        let fresh = preview.placed - preview.contested;
        assert_eq!((fresh, unchanged, replaced), (1, 1, 0));

        // The old arithmetic said two new files, and one of them was a file
        // that already existed byte-for-byte.
        assert_ne!(
            preview.placed - preview.reports.len(),
            fresh,
            "this is the miscount the review found"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The three counts always partition what was placed — the property the
    /// census asserts and the screen renders.
    #[test]
    fn the_three_counts_always_add_up_to_what_was_placed() {
        for (base, overlay) in [
            (
                &b"$VER: format 44.5 (1.1.99)"[..],
                &b"$VER: format 45.1 (1.1.00)"[..],
            ),
            (
                &b"$VER: format 44.5 (1.1.99)"[..],
                &b"$VER: format 44.5 (1.1.99)"[..],
            ),
            (
                &b"no version here"[..],
                &b"none here either, and longer"[..],
            ),
        ] {
            let (dir, plan) =
                plan_over_two_media("component-preview-partition", "workbench-39", base, overlay);
            let preview = preview_component_collisions(
                &plan,
                &["workbench-39".to_string()],
                &std::env::temp_dir(),
                &NoProgress,
            )
            .unwrap();
            let replaced = preview.reports.len();
            assert!(preview.contested >= replaced, "{preview:?}");
            let unchanged = preview.contested - replaced;
            let fresh = preview.placed - preview.contested;
            assert_eq!(fresh + unchanged + replaced, preview.placed, "{preview:?}");
            let _ = std::fs::remove_dir_all(&dir);
        }
    }

    /// **Two previews of the same components must not corrupt each other**
    /// (review F3).
    ///
    /// The staging root used to be a hash of the plan and the components, and
    /// it is `remove_dir_all`'d on entry — so two concurrent previews of the
    /// same thing shared a root and the second wiped the first's staged files.
    /// The first then found nothing at the destination and every Replace row
    /// degraded silently to "new": a wrong preview, in the screen a user reads
    /// before ticking. ART-178 makes that interleaving the normal case rather
    /// than a rare one.
    ///
    /// **The guard is `two_previews_never_share_a_staging_root` below, not
    /// this test.** This one runs the real thing on two threads and is worth
    /// having, but it cannot *force* the interleaving — with the old shared
    /// root it still passes, because the two previews are fast enough that one
    /// often finishes before the other clears the directory. A test that
    /// passes against the defect is not a test of the defect, so the property
    /// that actually fixes it is asserted directly, and this stands beside it
    /// as a smoke check that concurrent previews work at all.
    #[test]
    fn two_concurrent_previews_of_the_same_components_both_answer() {
        let (dir, plan) = plan_over_two_media(
            "component-preview-concurrent",
            "workbench-39",
            b"$VER: format 44.5 (1.1.99)",
            b"$VER: format 45.1 (1.1.00)",
        );
        let plan = std::sync::Arc::new(plan);

        let handles: Vec<_> = (0..2)
            .map(|_| {
                let plan = std::sync::Arc::clone(&plan);
                std::thread::spawn(move || {
                    preview_component_collisions(
                        &plan,
                        &["workbench-39".to_string()],
                        &std::env::temp_dir(),
                        &NoProgress,
                    )
                })
            })
            .collect();

        for handle in handles {
            let preview = handle.join().expect("no panic").expect("no error");
            assert_eq!(
                preview.reports.len(),
                1,
                "a preview whose staging was wiped reports no collision at all: {preview:?}"
            );
            assert_eq!(preview.contested, 1);
            assert_eq!(preview.placed, 2);
        }

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// **Two previews never share a staging root** — the property that fixes
    /// review F3, asserted where it can fail deterministically.
    ///
    /// The root used to be a hash of the plan and the components, and the
    /// preview `remove_dir_all`s it on entry: two previews of the same thing
    /// therefore shared a directory and the second wiped the first's staged
    /// files, after which `classify_incoming` found nothing at the destination
    /// and every Replace row degraded silently to "new". Reverting
    /// `scratch_root_for` to a hash fails this line and nothing else has to be
    /// timed for it to.
    #[test]
    fn two_previews_never_share_a_staging_root() {
        let mut seen = std::collections::HashSet::new();
        for _ in 0..64 {
            assert!(
                seen.insert(scratch_root_for(&std::env::temp_dir())),
                "a staging root repeated: {seen:?}"
            );
        }
    }

    /// The staged AmigaOS bytes do not outlive the call (review F6) — on
    /// success *or* on cancellation, which is the exit a cleanup written at
    /// the bottom of the function misses.
    #[test]
    fn two_threads_never_stage_into_one_namespace() {
        // ART-182, and **this is the guard**. The flake it was filed for
        // reproduced three runs in six on one machine and none in six on
        // another, so "the suite is green now" proves nothing about it. What
        // can be proved is the property underneath: two threads must not share
        // a staging namespace, because `cargo test` runs the whole binary in
        // one process and sixteen tests reach this function.
        //
        // Written this way for the same reason ART-181's guard freezes the
        // clock: a test that waits for a race to happen is a coin toss, and a
        // test that asserts the invariant the race violates is not.
        fn namespace_of(root: &Path) -> String {
            let name = root.file_name().unwrap().to_string_lossy().to_string();
            // Drop the trailing per-call counter; what must differ is the rest.
            name.rsplit_once('-').unwrap().0.to_string()
        }

        let mine = namespace_of(&scratch_root_for(&std::env::temp_dir()));
        let theirs = std::thread::spawn(|| namespace_of(&scratch_root_for(&std::env::temp_dir())))
            .join()
            .unwrap();

        assert_ne!(
            mine, theirs,
            "two threads share a staging namespace, so each counts the other's work as its own",
        );
    }

    #[test]
    fn staging_is_removed_however_the_preview_ends() {
        fn staging_dirs() -> Vec<PathBuf> {
            // Keyed on this thread as well as the process, or the count picks
            // up the other tests running beside this one (ART-182).
            use std::hash::{Hash, Hasher};
            let mut hasher = std::collections::hash_map::DefaultHasher::new();
            std::thread::current().id().hash(&mut hasher);
            let prefix = format!(
                "{PREVIEW_SCRATCH_PREFIX}component-{}-{:08x}-",
                std::process::id(),
                hasher.finish() as u32
            );
            std::fs::read_dir(std::env::temp_dir())
                .into_iter()
                .flatten()
                .flatten()
                .map(|entry| entry.path())
                .filter(|path| {
                    path.file_name()
                        .map(|name| name.to_string_lossy().starts_with(&prefix))
                        .unwrap_or(false)
                })
                .collect()
        }

        struct Stopped;
        impl ProgressSink for Stopped {
            fn report(&self, _done: u64, _total: Option<u64>, _label: &str) {}
            fn is_cancelled(&self) -> bool {
                true
            }
        }

        let (dir, plan) = plan_over_two_media(
            "component-preview-cleanup",
            "workbench-39",
            b"$VER: format 44.5 (1.1.99)",
            b"$VER: format 45.1 (1.1.00)",
        );

        let before = staging_dirs().len();
        preview_component_collisions(
            &plan,
            &["workbench-39".to_string()],
            &std::env::temp_dir(),
            &NoProgress,
        )
        .unwrap();
        assert_eq!(
            staging_dirs().len(),
            before,
            "nothing left behind on success"
        );

        let _ = preview_component_collisions(
            &plan,
            &["workbench-39".to_string()],
            &std::env::temp_dir(),
            &Stopped,
        );
        assert_eq!(
            staging_dirs().len(),
            before,
            "nor on the exit a bottom-of-function cleanup never reaches"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The `declared` flag is not decorative: it is the difference between
    /// "this component said it would do this" and "this component is about to
    /// stand on another's file without saying so". A component id no shipped
    /// recipe knows cannot claim an override, and `declared_override` refuses
    /// rather than assuming one — which is what keeps a preview from
    /// reassuring a user about a component ART cannot vouch for.
    #[test]
    fn a_component_that_declared_no_override_cannot_be_reported_as_declaring_one() {
        let (dir, plan) = plan_over_two_media(
            "component-preview-undeclared",
            "locale-base",
            b"$VER: format 44.5 (1.1.99)",
            b"$VER: format 45.1 (1.1.00)",
        );

        let preview = preview_component_collisions(
            &plan,
            &["locale-base".to_string()],
            &std::env::temp_dir(),
            &NoProgress,
        )
        .unwrap();

        assert_eq!(preview.reports.len(), 1, "{:?}", preview.reports);
        assert!(
            !preview.reports[0].declared,
            "locale-base declares no override over workbench-base"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Identical bytes are not a collision — the same rule `preview` applies
    /// to a package, reaching a release component now that one asks. Without
    /// this, a component that re-places a file byte-for-byte would be
    /// reported as replacing it, and a user would refuse a build over
    /// nothing.
    #[test]
    fn a_component_that_replaces_a_file_with_the_same_bytes_reports_nothing() {
        let same = b"$VER: format 44.5 (1.1.99)";
        let (dir, plan) = plan_over_two_media("component-preview-same", "workbench-39", same, same);

        let preview = preview_component_collisions(
            &plan,
            &["workbench-39".to_string()],
            &std::env::temp_dir(),
            &NoProgress,
        )
        .unwrap();

        assert_eq!(preview.reports, Vec::new(), "{:?}", preview.reports);
        assert_eq!(
            preview.placed, 2,
            "and it still placed both files — silence is not absence"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Nothing switched on previews nothing, without opening a single
    /// medium — the state the screen is in before any box is ticked, and on
    /// every release whose recipe has no layering component at all.
    #[test]
    fn previewing_no_components_opens_no_media() {
        let dir = scratch("component-preview-empty");
        let plan = InstallPlan {
            release: "AmigaOS 3.2".to_string(),
            items: Vec::new(),
            refusals: Vec::new(),
            total_bytes: 0,
            total_files: 0,
            components_on: Vec::new(),
            paired_rom: None,
            // A path that does not exist: reaching for it at all would fail.
            media_paths: {
                let mut m = BTreeMap::new();
                m.insert("Nowhere".to_string(), dir.join("no-such.adf"));
                m
            },
            packages: Vec::new(),
            package_media: BTreeMap::new(),
            user_startup: Vec::new(),
            activations: Vec::new(),
            media_stamps: BTreeMap::new(),
            removals: Vec::new(),
        };

        let preview =
            preview_component_collisions(&plan, &[], &std::env::temp_dir(), &NoProgress).unwrap();
        assert_eq!(preview.placed, 0);
        assert_eq!(preview.reports, Vec::new());

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// **"Earlier" means earlier in plan order, not merely "not selected".**
    ///
    /// This test found a real defect on its first run: previewing the *base*
    /// component reported it as downgrading `C/Format` from 45.1 to 44.5,
    /// because the overlay — which writes **after** it — was being staged as
    /// though it were already on the tree. A component that writes later is
    /// not in the way; it is on top. Getting this backwards would tell a user
    /// their base release is about to undo their overlay, which is the exact
    /// opposite of what would happen.
    ///
    /// It also pins the re-staging rule: the scratch root is deterministic (so
    /// a checkbox toggled back and forth reuses one directory rather than
    /// growing one per click), which is what makes clearing it on every call
    /// load-bearing rather than tidy.
    #[test]
    fn a_later_component_is_not_what_is_being_replaced() {
        let (dir, plan) = plan_over_two_media(
            "component-preview-restage",
            "workbench-39",
            b"$VER: format 44.5 (1.1.99)",
            b"$VER: format 45.1 (1.1.00)",
        );

        let first = preview_component_collisions(
            &plan,
            &["workbench-39".to_string()],
            &std::env::temp_dir(),
            &NoProgress,
        )
        .unwrap();
        assert_eq!(first.reports.len(), 1);

        // Now preview the *base* component instead. Nothing precedes it, so
        // nothing is in its way — and the previous call's staged `C/Format`
        // must not be mistaken for a file already on the tree.
        let second = preview_component_collisions(
            &plan,
            &["workbench-base".to_string()],
            &std::env::temp_dir(),
            &NoProgress,
        )
        .unwrap();
        assert_eq!(second.placed, 1);
        assert_eq!(
            second.reports,
            Vec::new(),
            "nothing precedes the base component, so nothing is in its way: {:?}",
            second.reports
        );

        // And the same selection again answers the same way, from a root it
        // re-staged rather than one it accumulated into.
        let again = preview_component_collisions(
            &plan,
            &["workbench-39".to_string()],
            &std::env::temp_dir(),
            &NoProgress,
        )
        .unwrap();
        assert_eq!(again.reports.len(), 1, "{:?}", again.reports);
        assert_eq!(again.placed, 2);

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Cancelling stops the preview rather than finishing it — the same
    /// discipline `extract_package_items` follows, checked between whole
    /// files.
    #[test]
    fn a_cancelled_component_preview_stops() {
        use crate::core::jobs::ProgressSink;

        struct Stopped;
        impl ProgressSink for Stopped {
            fn report(&self, _done: u64, _total: Option<u64>, _label: &str) {}
            fn is_cancelled(&self) -> bool {
                true
            }
        }

        let (dir, plan) = plan_over_two_media(
            "component-preview-cancel",
            "workbench-39",
            b"$VER: format 44.5 (1.1.99)",
            b"$VER: format 45.1 (1.1.00)",
        );

        let err = preview_component_collisions(
            &plan,
            &["workbench-39".to_string()],
            &std::env::temp_dir(),
            &Stopped,
        )
        .unwrap_err();
        assert!(matches!(err, CoreError::Cancelled), "{err:?}");

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// **The overlay census, from the real discs.** ART-175's own figure, and
    /// the reason this hook exists rather than the figure being quoted.
    ///
    /// "622 new files, 19 upgrades, 0 downgrades" has been repeated several
    /// times about AmigaOS 3.9's `workbench-39`, and nothing checked in
    /// produced it — which is the exact class of claim this project has been
    /// caught by more than once. This is the thing that produces it. Run it
    /// and the numbers are measured; do not run it and there are no numbers to
    /// quote.
    ///
    /// Skipped cleanly unless both environment variables are set, the same
    /// convention `apply.rs`'s own
    /// `run_the_real_engine_against_the_users_own_media_when_asked` and
    /// `core/preload/native.rs`'s oracle hooks already use:
    ///
    /// ```text
    /// ART_OSINSTALL_MEDIA="E:\amiga\Amigatolon\iso" ^
    /// ART_OSINSTALL_ROM="E:\amiga\Amigatolon\kickstart\Kickstart v3.1 rev 40.68 (1993)(Commodore)(A1200).rom" ^
    /// cargo test census_the_overlay_against_the_users_own_media_when_asked -- --nocapture --ignored
    /// ```
    ///
    /// (`--ignored` as well as the env gate, belt and braces: a plain
    /// `cargo test` must never reach outside the repo's own tempdir.)
    ///
    /// **Read-only.** It plans and previews; it writes nothing but a scratch
    /// directory under `%TEMP%` that the preview sweeps itself. The
    /// destination it hands `plan()` is a path that does not exist and is
    /// never created.
    ///
    /// It prints a census rather than asserting numbers, because the answer
    /// depends on which discs the user actually has: a hard-coded 622 here
    /// would pass on a machine with a different pressing and prove nothing on
    /// any. What it *does* assert is the shape — that the release layers at
    /// all, and that the preview can say so — and it prints every row, so the
    /// comparison against ART-169's table is a reading rather than a
    /// recollection.
    /// How many files the tree's own `distribution.json` accounts for.
    ///
    /// Read back through the real type, not by counting lines: a manifest
    /// that no longer parses would otherwise read as "no rows" and make the
    /// assertion that uses this pass for the wrong reason.
    fn read_manifest_rows(tree: &Path) -> usize {
        let text = std::fs::read_to_string(tree.join("distribution.json"))
            .expect("the tree accounts for itself");
        let manifest: crate::core::osinstall::apply::DistributionManifest =
            serde_json::from_str(&text).expect("and the manifest still parses");
        manifest.files.len()
    }

    /// Every file under `dir`, recursively. Sidecars included — the caller
    /// filters them, because "which of these is a sidecar" is the question it
    /// is asking.
    fn walkdir_files(dir: &Path) -> Vec<PathBuf> {
        let mut out = Vec::new();
        let mut stack = vec![dir.to_path_buf()];
        while let Some(next) = stack.pop() {
            let Ok(entries) = std::fs::read_dir(&next) else {
                continue;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    stack.push(path);
                } else {
                    out.push(path);
                }
            }
        }
        out
    }

    /// **ART-171: spec §8.3's hazard — a top-level drawer arriving on a tree
    /// for the first time — and the measurement that says why it never has.**
    ///
    /// The prediction was that a package's payload carries drawers a base
    /// tree may not have at all, so applying one would be the first time
    /// `apply` **creates** a top-level drawer rather than writing into one the
    /// release already made: its `.uaem` sidecar, its manifest rows, all of it
    /// on a path nothing had claimed.
    ///
    /// **Why it has never happened, measured rather than assumed.** Read the
    /// shipped AmigaOS 3.9 recipe: `workbench-39` is `required`, and among its
    /// thirteen rules are `OS-VERSION3.9/WORKBENCH3.9/LOCALE → Locale` and
    /// `…/WBSTARTUP → WBStartup`. Every top-level drawer a readable package
    /// in the catalogue could introduce is therefore already on every 3.9 tree
    /// ART builds — including the two §8.3 named. The hazard is not
    /// unexercised because nobody got to it; it cannot occur with the content
    /// that ships. ([ART-193](docs/ISSUES.md) settled the other route: both
    /// BoingBags really did add top-level content on the owner's tree, and
    /// every file of it was written by the Amiga's own `Updater` inside the
    /// emulator, not by `apply`.)
    ///
    /// **So the code path is what needs the test, and it gets a real one.**
    /// The tree is built from the owner's own disc and the payload is the
    /// owner's own archive; the only thing this test arranges is the
    /// *absence*, by removing the `Locale` drawer that `workbench-39` placed.
    /// That is stated plainly rather than hidden behind a fixture: everything
    /// `apply` then reads — the names, the bytes, the tree it writes into — is
    /// real, and the drawer it has to create is genuinely not there.
    #[test]
    #[ignore = "needs the user's own AmigaOS 3.9 disc and language pack; set ART_172_MEDIA and ART_172_PACKAGES"]
    fn a_package_creating_a_top_level_drawer_accounts_for_it() {
        let (Ok(media), Ok(packages)) = (
            std::env::var("ART_172_MEDIA"),
            std::env::var("ART_172_PACKAGES"),
        ) else {
            eprintln!("skipped: set ART_172_MEDIA and ART_172_PACKAGES");
            return;
        };

        let scratch = ScratchDir::new("art-171", "new-top-drawer");
        let tree = scratch.path().join("tree");
        let recipe = recipe::by_release("AmigaOS 3.9").expect("the shipped 3.9 recipe");

        let request = InstallRequest {
            packages: Vec::new(),
            package_folder: None,
            release: "AmigaOS 3.9".to_string(),
            media_folder: PathBuf::from(&media),
            extra_media_folders: Vec::new(),
            media_folders: BTreeMap::new(),
            keymap: None,
            rom: None,
            chosen: Vec::new(),
            excluded: Vec::new(),
            destination: tree.clone(),
            scan_cache: Default::default(),
        };
        let plan = crate::core::osinstall::plan::plan(&request, &recipe).expect("plan the tree");
        crate::core::osinstall::apply::apply(&plan, &tree, &NoProgress).expect("build the tree");

        // **The measurement above, asserted.** If a future recipe stops
        // placing `Locale`, this test is arranging an absence that is already
        // there and the sentence in its doc comment has gone stale.
        let locale = tree.join("Locale");
        assert!(
            locale.is_dir(),
            "workbench-39 is required and places Locale; §8.3's hazard cannot \
             occur while that is true, and this test's premise depends on it"
        );
        std::fs::remove_dir_all(&locale).expect("arrange the absence");

        let package = crate::core::osinstall::package::by_id("locale-turkish")
            .expect("the shipped Turkish pack");
        let archive = PathBuf::from(&packages).join("BoingBag39-2-turkce.lha");
        assert!(archive.is_file(), "{} is not there", archive.display());

        let before = read_manifest_rows(&tree);
        let outcome = crate::core::osinstall::apply::add_package_staging_in(
            &tree,
            &package,
            &archive,
            scratch.path(),
            &NoProgress,
        )
        .expect("add the package onto the tree");

        println!("\n=== ART-171: a top-level drawer arriving ===");
        println!(
            "placed {} files into a drawer that was not there",
            outcome.files
        );

        // 1. The drawer exists, with the payload really in it.
        assert!(locale.is_dir(), "the drawer has to have been created");
        let catalogs = locale.join("Catalogs");
        assert!(catalogs.is_dir(), "and the one below it");
        let placed: Vec<PathBuf> = walkdir_files(&catalogs);
        assert!(
            !placed.is_empty(),
            "a created drawer with nothing in it is the failure this is for"
        );
        println!("Locale/Catalogs now holds {} files", placed.len());

        // 2. **No `.uaem` sidecar is invented for any of them, and that is
        //    the correct answer** — which is the opposite of what this test
        //    asserted when it was first written, and the first run said so.
        //
        //    The assumption was that a tree is only a system volume if
        //    metadata came with the bytes, so a drawer created outside the
        //    release's own pass is where a sidecar would be skipped. ART's
        //    rule is the other way round and is written down in
        //    `apply::settle_sidecar`: an archive states no AmigaDOS
        //    protection, date or comment at all — `source_archive.rs` calls
        //    its values *declared defaults, never a reading*, and §89 forbids
        //    treating a declared default as evidence. On a path the release
        //    placed, the sidecar already beside it therefore stands; on a path
        //    nothing has ever written, there is nothing anybody has stated,
        //    and writing one would be ART claiming a fact it does not have.
        //
        //    So the §8.3 bookkeeping for a first-time drawer is: real bytes,
        //    a manifest row, and **no invented metadata**. Pinned here because
        //    the plausible-looking mistake is the other one.
        let invented: Vec<String> = placed
            .iter()
            .filter(|f| f.extension().and_then(|e| e.to_str()) == Some("uaem"))
            .map(|f| f.display().to_string())
            .collect();
        assert!(
            invented.is_empty(),
            "an archive states nothing about AmigaDOS metadata; these were \
             invented: {invented:?}"
        );

        // 3. The manifest accounts for them. A file on the tree that
        //    `distribution.json` does not name cannot be removed, replaced or
        //    explained later — it is exactly the bookkeeping §8.3 predicted
        //    would be skipped on a first-time drawer.
        let after = read_manifest_rows(&tree);
        let added = after.saturating_sub(before);
        println!("manifest rows: {before} -> {after} (+{added})");
        assert!(
            added > 0,
            "the tree has to be able to account for what arrived in it"
        );
    }

    /// **ART-172: spec §8.4's hazard, measured instead of predicted.**
    ///
    /// A language pack lands on top of catalogs the base release already
    /// placed. The round that was supposed to exercise it reported `rows=0
    /// upgrade=0 downgrade=0 same-version=0 unversioned=0` and that number
    /// was measuring nothing: ART-168 decoded the archive's Latin-1 drawer
    /// name as `t<U+FFFD>rk<U+FFFD>e`, so all 36 incoming files were compared
    /// against a destination nothing had ever written and every one came back
    /// *new*. The cleanest-looking number of the round was the wrong
    /// question, answered confidently.
    ///
    /// **Why this could not be a synthetic fixture.** Both halves of the
    /// hazard are properties of real bytes: the disc spells the drawer
    /// `TÜRKÇE` in its Primary tree (no Joliet descriptor on `AmigaOS39.iso`
    /// at all), and the archive spells it `türkçe` in Latin-1
    /// (`74 FC 72 6B E7 65`). A fixture would encode whichever pair the
    /// author believed in, which is exactly how the first measurement went
    /// wrong. So this builds `locale-base` from the disc and previews the
    /// real package against it.
    ///
    /// It prints the census and asserts the **shape** — that the two sides
    /// meet at all, and that what meets is declared — rather than a row
    /// count, because the count depends on which pressing and which
    /// BoingBag the user has, and a hard-coded 34 would pass on one machine
    /// and prove nothing on any.
    ///
    /// Read-only with respect to the user's material: the disc and the
    /// archive are opened, and everything written goes into a scratch
    /// directory that removes itself.
    #[test]
    #[ignore = "needs the user's own AmigaOS 3.9 disc and language pack; set ART_172_MEDIA and ART_172_PACKAGES"]
    fn the_language_pack_really_does_land_on_the_base_locale() {
        let (Ok(media), Ok(packages)) = (
            std::env::var("ART_172_MEDIA"),
            std::env::var("ART_172_PACKAGES"),
        ) else {
            eprintln!(
                "skipped: set ART_172_MEDIA (folder holding AmigaOS39.iso) and \
                 ART_172_PACKAGES (folder holding BoingBag39-2-turkce.lha)"
            );
            return;
        };

        let scratch = ScratchDir::new("art-172", "locale-collision");
        let tree = scratch.path().join("tree");
        let recipe = recipe::by_release("AmigaOS 3.9").expect("the shipped 3.9 recipe");

        // `locale-base` only, plus whatever the recipe marks required. This is
        // the smallest real tree the hazard can happen on.
        let request = InstallRequest {
            packages: Vec::new(),
            package_folder: None,
            release: "AmigaOS 3.9".to_string(),
            media_folder: PathBuf::from(&media),
            extra_media_folders: Vec::new(),
            media_folders: BTreeMap::new(),
            keymap: None,
            rom: None,
            chosen: vec!["locale-base".to_string()],
            excluded: Vec::new(),
            destination: tree.clone(),
            scan_cache: Default::default(),
        };

        let plan = crate::core::osinstall::plan::plan(&request, &recipe).expect("plan the tree");
        println!("\n=== ART-172: the base tree ===");
        println!("plan: {} items", plan.items.len());
        for refusal in &plan.refusals {
            println!("  refused: {refusal:?}");
        }

        let outcome = crate::core::osinstall::apply::apply(&plan, &tree, &NoProgress)
            .expect("build the base tree");
        println!("placed: {} files", outcome.files);

        // What the disc actually wrote, so the comparison below is against a
        // name that is on this disk rather than one this test believes in.
        let catalogs = tree.join("Locale").join("Catalogs");
        let drawers: Vec<String> = std::fs::read_dir(&catalogs)
            .expect("the base release placed Locale/Catalogs")
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        println!("Locale/Catalogs holds {} drawers", drawers.len());
        let turkish: Vec<&String> = drawers
            .iter()
            .filter(|d| {
                crate::core::osinstall::fold_amiga_case(d)
                    == crate::core::osinstall::fold_amiga_case("türkçe")
            })
            .collect();
        println!("  folding onto 'türkçe': {turkish:?}");
        assert!(
            !turkish.is_empty(),
            "the disc's own Turkish catalog drawer is what the pack lands on; \
             Locale/Catalogs held {drawers:?}"
        );

        // Now the pack, read by the fixed reader, previewed against that tree.
        let catalogue = crate::core::osinstall::package::packages_for("AmigaOS 3.9")
            .expect("the shipped package catalogue");
        let ordered = vec!["locale-turkish".to_string()];
        let reports = preview_collisions(
            &tree,
            &PathBuf::from(&packages),
            &ordered,
            &catalogue,
            scratch.path(),
            &NoProgress,
        )
        .expect("preview the language pack");

        let mut by_class: BTreeMap<String, usize> = BTreeMap::new();
        for report in &reports {
            *by_class
                .entry(
                    format!("{:?}", report.collision)
                        .split('{')
                        .next()
                        .unwrap()
                        .trim()
                        .to_string(),
                )
                .or_default() += 1;
        }
        println!("\n=== ART-172: the collision census ===");
        println!("rows: {}", reports.len());
        for (class, count) in &by_class {
            println!("  {class:<20} {count}");
        }
        for report in reports.iter().take(5) {
            println!(
                "  e.g. {} declared={} {:?}",
                report.path, report.declared, report.collision
            );
        }

        // **The assertion the 0-row run could not make.** Not a count: that
        // the hazard happens at all, and that every collision is one the
        // package declared. An undeclared row would mean a package writing
        // over something no `overrides` mentions, which is the thing the
        // declaration exists to make visible.
        assert!(
            !reports.is_empty(),
            "spec §8.4 predicted this collision and ART-168 hid it; an empty \
             report here means it is hidden again"
        );
        let undeclared: Vec<&str> = reports
            .iter()
            .filter(|r| !r.declared)
            .map(|r| r.path.as_str())
            .collect();
        assert!(
            undeclared.is_empty(),
            "locale-turkish declares overrides: [locale-base]; these were not \
             covered by it: {undeclared:?}"
        );
    }

    #[test]
    #[ignore = "needs the user's own AmigaOS media; set ART_OSINSTALL_MEDIA and ART_OSINSTALL_ROM"]
    fn census_the_overlay_against_the_users_own_media_when_asked() {
        let (Ok(media), Ok(rom)) = (
            std::env::var("ART_OSINSTALL_MEDIA"),
            std::env::var("ART_OSINSTALL_ROM"),
        ) else {
            eprintln!(
                "skipped: set ART_OSINSTALL_MEDIA and ART_OSINSTALL_ROM to run the overlay census"
            );
            return;
        };

        let release =
            std::env::var("ART_OSINSTALL_RELEASE").unwrap_or_else(|_| "AmigaOS 3.9".to_string());
        let recipe = recipe::by_release(&release).unwrap();

        // Every component the recipe marks reachable, so the plan is the one a
        // user ticking everything would get.
        let chosen: Vec<String> = recipe
            .components
            .iter()
            .filter(|c| c.available && !c.required)
            .map(|c| c.id.clone())
            .collect();

        let request = InstallRequest {
            packages: Vec::new(),
            package_folder: None,
            release: release.clone(),
            media_folder: PathBuf::from(&media),
            extra_media_folders: Vec::new(),
            media_folders: BTreeMap::new(),
            keymap: None,
            rom: Some(PathBuf::from(&rom)),
            chosen,
            excluded: Vec::new(),
            // Never created: `plan()` does not touch it, and nothing here
            // calls `apply()`.
            destination: std::env::temp_dir().join("art-overlay-census-no-such-tree"),
            scan_cache: Default::default(),
        };

        let plan = crate::core::osinstall::plan::plan(&request, &recipe).expect("the plan itself");
        println!("\n=== {release}: overlay census ===");
        println!("media folder : {media}");
        println!(
            "plan         : {} items, {} refusals",
            plan.items.len(),
            plan.refusals.len()
        );
        for refusal in &plan.refusals {
            println!("  refused    : {refusal:?}");
        }

        // The layering components, from the recipe rather than from a memory
        // of which they are.
        let layering: Vec<String> = recipe
            .components
            .iter()
            .filter(|c| !c.overrides.is_empty() && plan.components_on.contains(&c.id))
            .map(|c| c.id.clone())
            .collect();
        println!("layering on  : {layering:?}");
        assert!(
            !layering.is_empty(),
            "this release declares no layering component that is switched on, so there is \
             nothing to census — check ART_OSINSTALL_RELEASE"
        );

        for id in &layering {
            let preview = preview_component_collisions(
                &plan,
                std::slice::from_ref(id),
                &std::env::temp_dir(),
                &NoProgress,
            )
            .expect("the preview");
            let mut upgrades = 0usize;
            let mut downgrades = 0usize;
            let mut same = 0usize;
            let mut unversioned = 0usize;
            for report in &preview.reports {
                match report.collision {
                    crate::core::osinstall::collide::Collision::Upgrade { .. } => upgrades += 1,
                    crate::core::osinstall::collide::Collision::Downgrade { .. } => downgrades += 1,
                    crate::core::osinstall::collide::Collision::SameVersion { .. } => same += 1,
                    crate::core::osinstall::collide::Collision::Unversioned { .. } => {
                        unversioned += 1
                    }
                    crate::core::osinstall::collide::Collision::Identical => {}
                }
            }
            // **The same three counts `layer_the_real_39_overlay_when_asked`
            // reports**, so the two are comparable rather than merely both
            // printed (review F4). `new` used to be
            // `placed - reports.len()`, which counts an identical file as a
            // new one — 130 of them on the 3.9 overlay, which is exactly why
            // 622 and this hook's number could never have matched.
            let replaced = preview.reports.len();
            let unchanged = preview.contested - replaced;
            let fresh = preview.placed - preview.contested;
            println!(
                "
{id}: placed {}, new {fresh}, identical {unchanged}, replaced {replaced}",
                preview.placed
            );
            // The arithmetic said out loud, because the point of this hook is
            // that a reader can check it against ART-169's table without
            // doing sums in their head.
            println!(
                "  check: new {fresh} + identical {unchanged} + replaced {replaced} = {}",
                fresh + unchanged + replaced
            );
            assert_eq!(
                fresh + unchanged + replaced,
                preview.placed,
                "every placed file falls in exactly one of the three"
            );
            println!(
                "  upgrades {upgrades}, downgrades {downgrades}, same-version {same}, unversioned {unversioned}"
            );
            // Every row, so the census is a record and not a summary anyone
            // has to trust.
            for report in &preview.reports {
                println!(
                    "  {:<48} {:?}{}",
                    report.path,
                    report.collision,
                    if report.declared {
                        ""
                    } else {
                        "  [UNDECLARED]"
                    }
                );
            }
        }
        println!("\n=== end of census ===\n");
    }

    /// No packages chosen previews as nothing to report, without opening
    /// either folder — the empty selection is the common case every time
    /// the panel loads before a checkbox is ticked.
    #[test]
    fn osinstall_collisions_with_nothing_chosen_is_empty() {
        let dir = scratch("collisions-empty");
        let tree = dir.join("does-not-exist-tree");
        let packages_dir = dir.join("does-not-exist-packages");

        let catalogue = package::packages().unwrap();
        let reports = preview_collisions(
            &tree,
            &packages_dir,
            &[],
            &catalogue,
            &std::env::temp_dir(),
            &NoProgress,
        )
        .unwrap();
        assert_eq!(reports, Vec::new());
    }

    /// F4 of Task 7's own fix round: a second preview of the same archive
    /// must not re-read it. Proved by deleting the archive after the first
    /// call — if the second call still succeeds and returns the same
    /// items, it can only have come from the cache, never from the (now
    /// missing) file on disk.
    #[test]
    fn extract_package_items_reuses_a_cached_extraction_without_rereading_the_archive() {
        let dir = scratch("preview-cache-reuse");
        let packages_dir = dir.join("packages");
        std::fs::create_dir_all(&packages_dir).unwrap();
        write_locale_turkish_archive(&packages_dir, "turkish.lha", b"catalog bytes");

        let catalogue = package::packages().unwrap();
        let package = catalogue.iter().find(|p| p.id == "locale-turkish").unwrap();
        let found = crate::core::osinstall::scan::find_packages(&packages_dir).unwrap();
        let archive = resolve_package_archive(package, &found).unwrap();

        let mut files = 0usize;
        let mut bytes = 0u64;
        let first = extract_package_items(
            package,
            archive,
            &mut files,
            &mut bytes,
            &std::env::temp_dir(),
            &NoProgress,
        )
        .unwrap();
        assert_eq!(first.len(), 1);
        assert!(first[0].2.is_file());

        // Corrupted **in place, same length, same mtime restored** — so the
        // cache key (path/mtime/len/member) the second call computes is
        // identical to the first, and only the cache — never a real re-open
        // of this now-broken zip — can explain a second call that still
        // succeeds and still returns `first`'s own (correct) bytes.
        let original_modified = std::fs::metadata(&archive.path)
            .unwrap()
            .modified()
            .unwrap();
        let original_len = std::fs::metadata(&archive.path).unwrap().len();
        let garbage = vec![0u8; original_len as usize];
        std::fs::write(&archive.path, &garbage).unwrap();
        std::fs::File::options()
            .write(true)
            .open(&archive.path)
            .and_then(|f| f.set_modified(original_modified))
            .unwrap();
        assert!(
            open_package(&PackageMedium {
                path: archive.path.clone(),
                member: package.member.clone(),
            })
            .is_err(),
            "the corrupted archive must not itself still open as a real one, \
             or this test would not distinguish a cache hit from a fresh read"
        );

        let mut files2 = 0usize;
        let mut bytes2 = 0u64;
        let second = extract_package_items(
            package,
            archive,
            &mut files2,
            &mut bytes2,
            &std::env::temp_dir(),
            &NoProgress,
        )
        .unwrap();
        assert_eq!(second, first, "the cached extraction, unchanged");
    }

    /// N5, Task 7's re-review: the extraction used to ignore the job's own
    /// cancel flag entirely. `CancelAfter::new(0)` is cancelled from the
    /// first check, so this proves the flag is actually read, not just
    /// threaded through unused — and that nothing is written before it is.
    #[test]
    fn extract_package_items_stops_at_the_first_cancellation_check() {
        let dir = scratch("preview-cancel-file-level");
        let packages_dir = dir.join("packages");
        std::fs::create_dir_all(&packages_dir).unwrap();
        write_locale_turkish_archive(&packages_dir, "turkish.lha", b"catalog bytes");

        let catalogue = package::packages().unwrap();
        let package = catalogue.iter().find(|p| p.id == "locale-turkish").unwrap();
        let found = crate::core::osinstall::scan::find_packages(&packages_dir).unwrap();
        let archive = resolve_package_archive(package, &found).unwrap();

        let mut files = 0usize;
        let mut bytes = 0u64;
        let cancel = crate::core::osinstall::fixtures::CancelAfter::new(0);
        let err = extract_package_items(
            package,
            archive,
            &mut files,
            &mut bytes,
            &std::env::temp_dir(),
            &cancel,
        )
        .unwrap_err();
        assert!(matches!(err, CoreError::Cancelled), "{err}");
    }

    /// The same check one level up: `extract_incoming_for_preview` stops
    /// before ever opening the first package's own archive, not only inside
    /// a package already being read.
    #[test]
    fn extract_incoming_for_preview_stops_before_opening_the_first_package() {
        let dir = scratch("preview-cancel-package-level");
        let packages_dir = dir.join("packages");
        std::fs::create_dir_all(&packages_dir).unwrap();
        write_locale_turkish_archive(&packages_dir, "turkish.lha", b"catalog bytes");

        let catalogue = package::packages().unwrap();
        let cancel = crate::core::osinstall::fixtures::CancelAfter::new(0);
        let err = extract_incoming_for_preview(
            &packages_dir,
            &["locale-turkish".to_string()],
            &catalogue,
            &std::env::temp_dir(),
            &cancel,
        )
        .unwrap_err();
        assert!(matches!(err, CoreError::Cancelled), "{err}");
    }

    /// N2, Task 7's re-review: `PREVIEW_CACHE` was unbounded — every archive
    /// identity a session ever previewed stayed in memory for the life of
    /// the process. Exercised against a fresh, private `PreviewCache`
    /// rather than the shared process-global `preview_cache()`: the global
    /// one is touched by every other test in this module, some of which run
    /// concurrently, and evicting an entry another test still expects would
    /// make this test flaky about the wrong thing.
    #[test]
    fn preview_cache_evicts_the_oldest_entry_once_the_bound_is_crossed() {
        let mut cache = PreviewCache::default();
        for i in 0..(MAX_PREVIEW_CACHE_ENTRIES + 3) {
            let key: PreviewCacheKey = (PathBuf::from(format!("archive-{i}")), 0, 0, None);
            cache.insert(key, Vec::new());
        }
        assert_eq!(cache.entries.len(), MAX_PREVIEW_CACHE_ENTRIES);

        // The first three inserted are the ones evicted — FIFO, not LRU;
        // `PreviewCache`'s own doc comment names that as the deliberate
        // choice (simplicity over hit-rate optimality for a cache this
        // small).
        for i in 0..3 {
            let evicted: PreviewCacheKey = (PathBuf::from(format!("archive-{i}")), 0, 0, None);
            assert!(
                cache.get(&evicted).is_none(),
                "archive-{i} should have been evicted"
            );
        }
        let newest: PreviewCacheKey = (
            PathBuf::from(format!("archive-{}", MAX_PREVIEW_CACHE_ENTRIES + 2)),
            0,
            0,
            None,
        );
        assert!(
            cache.get(&newest).is_some(),
            "the most recent insert must survive"
        );
    }

    /// F4's own "no sweep" finding: a preview scratch directory older than
    /// the retention window is reaped; a fresh one, and anything outside
    /// this module's own prefix, is left alone.
    #[test]
    fn sweep_stale_preview_scratch_dirs_removes_only_old_directories_under_its_own_prefix() {
        // Opening a plain directory as a `File` is refused on Windows
        // (`ERROR_ACCESS_DENIED`) unless `FILE_FLAG_BACKUP_SEMANTICS` is
        // set — there is no portable `std` way to back-date a directory's
        // own mtime, so this reaches for the one Windows-specific flag that
        // makes `set_modified` work on one. The whole project only ever
        // builds for `x86_64-pc-windows-msvc` (CLAUDE.md), so a test-only
        // `#[cfg(windows)]` helper costs nothing real.
        fn set_dir_modified(path: &Path, time: std::time::SystemTime) {
            use std::os::windows::fs::OpenOptionsExt;
            const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x0200_0000;
            std::fs::OpenOptions::new()
                .read(true)
                .write(true)
                .custom_flags(FILE_FLAG_BACKUP_SEMANTICS)
                .open(path)
                .and_then(|f| f.set_modified(time))
                .unwrap();
        }

        let temp = std::env::temp_dir();

        let stale = temp.join(format!("{PREVIEW_SCRATCH_PREFIX}sweep-test-stale"));
        let _ = std::fs::remove_dir_all(&stale);
        std::fs::create_dir_all(&stale).unwrap();
        set_dir_modified(
            &stale,
            std::time::SystemTime::now() - (PREVIEW_SCRATCH_MAX_AGE * 2),
        );

        let fresh = temp.join(format!("{PREVIEW_SCRATCH_PREFIX}sweep-test-fresh"));
        let _ = std::fs::remove_dir_all(&fresh);
        std::fs::create_dir_all(&fresh).unwrap();

        sweep_stale_preview_scratch_dirs(&std::env::temp_dir());

        assert!(!stale.exists(), "a stale scratch directory must be swept");
        assert!(fresh.exists(), "a fresh scratch directory must survive");

        let _ = std::fs::remove_dir_all(&fresh);
    }

    /// ART-162's own rule, reachable from this command: `locale-turkish`
    /// needs `locale-base` switched on, and a tree whose manifest never
    /// recorded that component is refused **before** anything is written —
    /// checked directly against [`resolve_packages_for_add`], the way
    /// `osinstall_verify`'s own `verify_at` is tested, since the real
    /// command needs a live Tauri `AppHandle`/`State` this test has none of.
    #[test]
    fn resolve_packages_for_add_refuses_locale_turkish_without_locale_base() {
        let dir = scratch("add-missing-component");
        let tree = dir.join("tree");
        std::fs::create_dir_all(&tree).unwrap();
        // A manifest that names no `locale-base` component at all.
        write_test_manifest(
            &tree,
            vec![crate::core::osinstall::apply::FileRecord {
                path: "C/List".into(),
                component: "workbench-base".into(),
                media: "OS-Version3.9".into(),
                sha256: "0".repeat(64),
                bytes: 0,
                protection: None,
                overwrote: None,
                host_path: None,
            }],
        );

        let packages_dir = dir.join("packages");
        std::fs::create_dir_all(&packages_dir).unwrap();
        write_locale_turkish_archive(&packages_dir, "turkish.lha", b"catalog bytes");

        let refusals =
            resolve_packages_for_add(&tree, &packages_dir, &["locale-turkish".to_string()])
                .unwrap()
                .unwrap_err();
        assert!(
            refusals.iter().any(|r| matches!(
                r,
                crate::core::osinstall::RefusalReason::PackageComponentMissing { .. }
            )),
            "{refusals:?}"
        );
    }

    /// The positive case beside it: once `locale-base` really is on the
    /// tree, the same selection resolves to exactly one package, ready for
    /// `add_package` to place.
    #[test]
    fn resolve_packages_for_add_resolves_locale_turkish_once_locale_base_is_on_the_tree() {
        let dir = scratch("add-component-present");
        let tree = dir.join("tree");
        std::fs::create_dir_all(&tree).unwrap();
        write_test_manifest(
            &tree,
            vec![locale_base_file_record("Locale/Languages/turkish.language")],
        );

        let packages_dir = dir.join("packages");
        std::fs::create_dir_all(&packages_dir).unwrap();
        write_locale_turkish_archive(&packages_dir, "turkish.lha", b"catalog bytes");

        let resolved =
            resolve_packages_for_add(&tree, &packages_dir, &["locale-turkish".to_string()])
                .unwrap()
                .unwrap();
        assert_eq!(resolved.len(), 1);
        assert_eq!(resolved[0].0.id, "locale-turkish");
        assert_eq!(resolved[0].1, packages_dir.join("turkish.lha"));
    }

    /// A missing archive is refused by name, not left for `add_package` to
    /// discover partway through a job.
    #[test]
    fn resolve_packages_for_add_refuses_a_missing_archive() {
        let dir = scratch("add-missing-archive");
        let tree = dir.join("tree");
        std::fs::create_dir_all(&tree).unwrap();
        write_test_manifest(
            &tree,
            vec![locale_base_file_record("Locale/Catalogs/x.catalog")],
        );

        let packages_dir = dir.join("packages");
        std::fs::create_dir_all(&packages_dir).unwrap();
        // No archive ever written here.

        let refusals =
            resolve_packages_for_add(&tree, &packages_dir, &["locale-turkish".to_string()])
                .unwrap()
                .unwrap_err();
        assert!(
            refusals.iter().any(|r| matches!(
                r,
                crate::core::osinstall::RefusalReason::PackageArchiveMissing { media, .. }
                    if media == "LocaleUpdate"
            )),
            "{refusals:?}"
        );
    }

    /// The carried-forward review point: a media folder that does not exist
    /// must reach the screen as a value it can translate, never as
    /// `find_media`'s own English `CoreError` sentence.
    #[test]
    fn scanning_a_missing_folder_is_a_typed_refusal_not_a_sentence() {
        let dir = scratch("scan-missing");
        let missing = dir.join("does-not-exist");

        let result = osinstall_scan_media(missing.clone()).unwrap();

        assert_eq!(
            result,
            MediaScanResult::FolderUnreadable {
                folder: missing.display().to_string(),
            }
        );
    }

    #[test]
    fn scanning_a_real_folder_finds_its_media() {
        let dir = scratch("scan-real");
        crate::core::osinstall::fixtures::workbench(&dir);

        let result = osinstall_scan_media(dir).unwrap();

        match result {
            MediaScanResult::Found { media } => {
                assert_eq!(media.len(), 1);
                assert_eq!(media[0].volume_name, "Workbench3.2");
            }
            other => panic!("expected Found, got {other:?}"),
        }
    }

    /// The same carried-forward point, for `osinstall_plan`: a bad media
    /// folder path is at least as likely to be discovered here, at the
    /// screen's own "preview" step, as through a separate scan call.
    #[test]
    fn planning_against_a_missing_folder_is_a_typed_refusal_not_a_sentence() {
        let dir = scratch("plan-missing");
        let missing = dir.join("does-not-exist");

        let result = osinstall_plan(InstallRequest {
            packages: Vec::new(),
            package_folder: None,
            release: "AmigaOS 3.2".to_string(),
            media_folder: missing.clone(),
            extra_media_folders: Vec::new(),
            media_folders: BTreeMap::new(),
            keymap: None,
            rom: None,
            chosen: vec!["workbench-base".to_string()],
            excluded: Vec::new(),
            destination: dir.join("dist"),
            scan_cache: Default::default(),
        })
        .unwrap();

        match result {
            PlanResult::FolderUnreadable { folder } => {
                assert_eq!(folder, missing.display().to_string());
            }
            other => panic!("expected FolderUnreadable, got {other:?}"),
        }
    }

    #[test]
    fn planning_against_a_real_folder_returns_the_plan() {
        let dir = scratch("plan-real");
        crate::core::osinstall::fixtures::workbench(&dir);

        let result = osinstall_plan(InstallRequest {
            packages: Vec::new(),
            package_folder: None,
            release: "AmigaOS 3.2".to_string(),
            media_folder: dir.clone(),
            extra_media_folders: Vec::new(),
            media_folders: BTreeMap::new(),
            keymap: None,
            rom: None,
            chosen: vec!["workbench-base".to_string()],
            excluded: Vec::new(),
            destination: dir.join("dist"),
            scan_cache: Default::default(),
        })
        .unwrap();

        match result {
            PlanResult::Planned { plan } => assert_eq!(plan.release, "AmigaOS 3.2"),
            other => panic!("expected Planned, got {other:?}"),
        }
    }

    // ---- Requirement 5, tested directly (fix round 1) ----
    //
    // The earlier version of this coverage called `verify_at` and inspected
    // `VerifyReport` alone — that is `verify_volume`, which Task 10 already
    // covers, and its own final assertion followed from the two
    // `assert_eq!`s above it. Requirement 5 names the oplog *record*, not
    // the report, so these test `verify_record` — the function that decides
    // what `osinstall_verify` actually logs — directly, against every one
    // of the three shapes a `VerifyReport` can take.

    fn one_file_report(state: crate::core::osinstall::verify::CheckState) -> VerifyReport {
        use crate::core::osinstall::verify::FileVerdict;
        let (passed, failed, not_checked) = match state {
            crate::core::osinstall::verify::CheckState::Pass => (1, 0, 0),
            crate::core::osinstall::verify::CheckState::Fail => (0, 1, 0),
            crate::core::osinstall::verify::CheckState::NotChecked => (0, 0, 1),
        };
        VerifyReport {
            files: vec![FileVerdict {
                path: "C/LoadModule".into(),
                state,
                detail: Some("detail".into()),
            }],
            passed,
            failed,
            not_checked,
        }
    }

    /// The property §89 is about: a report with nothing failed but something
    /// not-checked must not be logged as verified. A `verified(failed == 0)`
    /// regression passes every other test in this module but flips this one.
    #[test]
    fn a_report_with_something_not_checked_is_not_verified_in_the_log() {
        use crate::core::osinstall::verify::CheckState;

        let report = one_file_report(CheckState::NotChecked);
        let record = verify_record("dist", "image.hdf", &Ok(report));

        assert_eq!(
            record.outcome,
            OperationOutcome::Success {
                verification: Some(false)
            },
            "{record:?}"
        );
    }

    #[test]
    fn a_fully_passing_report_is_verified_in_the_log() {
        use crate::core::osinstall::verify::CheckState;

        let report = one_file_report(CheckState::Pass);
        let record = verify_record("dist", "image.hdf", &Ok(report));

        assert_eq!(
            record.outcome,
            OperationOutcome::Success {
                verification: Some(true)
            },
            "{record:?}"
        );
    }

    #[test]
    fn a_report_with_a_real_failure_is_not_verified_in_the_log() {
        use crate::core::osinstall::verify::CheckState;

        let report = one_file_report(CheckState::Fail);
        let record = verify_record("dist", "image.hdf", &Ok(report));

        assert_eq!(
            record.outcome,
            OperationOutcome::Success {
                verification: Some(false)
            },
            "{record:?}"
        );
    }

    /// `verify_at` itself still gets one end-to-end exercise — a genuinely
    /// unreadable filesystem family really does come back `not_checked`, not
    /// `failed` — but the property under test here is only that `verify_at`
    /// runs and returns a real report; requirement 5's own claim is proved
    /// by the three tests above, against `verify_record` directly.
    #[test]
    fn verify_at_reports_an_unreadable_family_as_not_checked() {
        use crate::core::hdf::create_hdf;
        use crate::core::osinstall::apply::{FileRecord, MediaRecord};
        use crate::core::rdb::{AmigaHardDiskFs, PartitionSpec};

        let dir = scratch("verify-not-checked");
        let image = dir.join("card.hdf");
        create_hdf(
            &image,
            10 * 1024 * 1024,
            true,
            &[PartitionSpec {
                drive_name: "DH0".into(),
                fs_type: AmigaHardDiskFs::Sfs0, // a family ART cannot read
                size_mb: 8,
                bootable: true,
                boot_priority: 0,
                num_buffers: 0,
            }],
            &[],
        )
        .unwrap();

        let dist_root = dir.join("dist");
        std::fs::create_dir_all(&dist_root).unwrap();
        let manifest = DistributionManifest {
            release: "AmigaOS 3.2".into(),
            built_from: vec![MediaRecord {
                volume_name: "Workbench3.2".into(),
                sha256: "0".repeat(64),
            }],
            files: vec![FileRecord {
                path: "C/LoadModule".into(),
                component: "workbench-base".into(),
                media: "Workbench3.2".into(),
                sha256: "0".repeat(64),
                bytes: 3,
                protection: Some(0x20),
                overwrote: None,
                host_path: None,
            }],
            paired_rom: None,
            amiga_installed: Vec::new(),
        };
        std::fs::write(
            dist_root.join(MANIFEST_FILE_NAME),
            serde_json::to_string(&manifest).unwrap(),
        )
        .unwrap();

        let report = verify_at(&VerifyRequest {
            image,
            slot: None,
            index: 1,
            dist_root,
        })
        .unwrap();

        assert_eq!(report.failed, 0, "{:?}", report.files);
        assert_eq!(report.not_checked, 1);
    }

    // -------------------------------------------------------------------
    // Outbound wire shapes (fix round 1) — every response type, checked
    // with `serde_json::to_value` against the exact keys and tag spellings
    // `src/lib/osinstall.ts` declares. This is what would have caught
    // `VerifyReport::not_checked` shipping without its `camelCase` rename:
    // the Task 12 wire test only pins the inbound direction (what Rust
    // *deserialises*), and a round trip through the same Rust types (as
    // `the_plan_the_frontend_sends_back_deserialises_into_an_apply_request`
    // does) cannot catch a `rename_all` mistake either, because both sides
    // of that round trip move together. These tests instead compare against
    // literal strings, the same way the coordinator's own review did by
    // hand.
    // -------------------------------------------------------------------
    mod wire_shapes {
        use super::*;
        use std::collections::BTreeSet;

        fn key_set(value: &serde_json::Value) -> BTreeSet<String> {
            value
                .as_object()
                .unwrap_or_else(|| panic!("expected a JSON object, got {value}"))
                .keys()
                .cloned()
                .collect()
        }

        fn expect_keys(value: &serde_json::Value, expected: &[&str]) {
            let got = key_set(value);
            let want: BTreeSet<String> = expected.iter().map(|s| s.to_string()).collect();
            assert_eq!(got, want, "value was: {value}");
        }

        /// The TS mirror (`src/lib/osinstall.ts`) is maintained by hand, not
        /// generated from this type — nothing here can see it, so this test
        /// only pins the Rust side's own wire shape, not a cross-check
        /// against the frontend's declared keys.
        #[test]
        fn found_media_serializes_with_the_keys_this_test_pins() {
            use crate::core::osinstall::scan::MediaKind;

            let media = FoundMedia {
                path: PathBuf::from("E:\\wb.adf"),
                volume_name: "Workbench3.2".into(),
                kind: MediaKind::Floppy,
                layer: None,
            };
            let value = serde_json::to_value(&media).unwrap();
            // `layer` is absent, not `null`, when the scan was unlayered —
            // see `FoundMedia::layer`'s own doc comment.
            expect_keys(&value, &["path", "volumeName", "kind"]);
            assert_eq!(value["kind"], "floppy");
        }

        /// `ComponentSummary` is the checklist on screen, so a key renamed
        /// on one side only would empty a row rather than fail a build —
        /// `src/lib/osinstall.ts`'s `ComponentDef` declares exactly these
        /// nine.
        ///
        /// **The two `major` fields are pinned against both shipped
        /// recipes, and against each other (ART-157).** They carry numbers
        /// that read alike and mean opposite things — "switches on below
        /// V47" and "needs at least V40" — so a projection that put a
        /// minimum into `conditionMajor` would render the screen's
        /// `rom-older-than` vocabulary over a `rom-at-least` fact and say
        /// the reverse of the truth. Each condition kind must fill exactly
        /// one of the two and leave the other null.
        #[test]
        fn component_summary_serializes_with_the_keys_the_checklist_reads() {
            let recipe = recipe::by_release("AmigaOS 3.2").unwrap();
            let modules = recipe
                .components
                .iter()
                .find(|c| c.id == "modules-a1200")
                .expect("the shipped 3.2 recipe carries the conditional component");
            let value = serde_json::to_value(ComponentSummary::from(modules)).unwrap();
            expect_keys(
                &value,
                &[
                    "id",
                    "media",
                    "labelKey",
                    "required",
                    "available",
                    "conditionMajor",
                    "requiresRomMajor",
                    "exclusiveGroup",
                    "overrides",
                ],
            );
            // A real conditional, exclusive-group component — so both
            // optional fields are pinned as values, not merely as present
            // nulls. It also declares an override, which is what ART-175's
            // preview keys off: `modules-a1200` layers over `storage`.
            assert_eq!(value["conditionMajor"], 47);
            assert_eq!(value["exclusiveGroup"], "modules");
            assert_eq!(value["overrides"], serde_json::json!(["storage"]));
            assert!(
                value["requiresRomMajor"].is_null(),
                "a maximum must never arrive on screen as a minimum"
            );

            // The other kind, off the other shipped recipe.
            let os39 = recipe::by_release("AmigaOS 3.9").unwrap();
            let base = os39
                .components
                .iter()
                .find(|c| c.id == "workbench-base")
                .expect("the shipped 3.9 recipe carries the Kickstart floor");
            let value = serde_json::to_value(ComponentSummary::from(base)).unwrap();
            assert_eq!(
                value["requiresRomMajor"], 40,
                "AmigaOS 3.9's own installer: 'You have to install Kickstart 3.1 ROMs'"
            );
            assert!(
                value["conditionMajor"].is_null(),
                "a minimum must never arrive on screen as a maximum"
            );
        }

        /// The regression this whole module exists to prevent: without
        /// `VerifyReport`'s `rename_all = "camelCase"`, this key would be
        /// `not_checked` and `value.get("notChecked")` would be `None`.
        #[test]
        fn verify_report_serializes_not_checked_as_camelcase() {
            use crate::core::osinstall::verify::{CheckState, FileVerdict};

            let report = VerifyReport {
                files: vec![FileVerdict {
                    path: "C/LoadModule".into(),
                    state: CheckState::NotChecked,
                    detail: Some("why".into()),
                }],
                passed: 0,
                failed: 0,
                not_checked: 1,
            };
            let value = serde_json::to_value(&report).unwrap();

            expect_keys(&value, &["files", "passed", "failed", "notChecked"]);
            assert_eq!(value["notChecked"], 1);
            assert!(
                value.get("not_checked").is_none(),
                "the un-camelCased name must not leak onto the wire: {value}"
            );

            let verdict = &value["files"][0];
            expect_keys(verdict, &["path", "state", "detail"]);
            assert_eq!(verdict["state"], "not-checked");
        }

        #[test]
        fn apply_outcome_serializes_with_the_keys_the_frontend_declares() {
            let outcome = ApplyOutcome {
                root: PathBuf::from("E:\\dist"),
                files: 3,
                directories: 1,
                bytes: 42,
                ..Default::default()
            };
            let value = serde_json::to_value(&outcome).unwrap();
            expect_keys(
                &value,
                &[
                    "root",
                    "files",
                    "directories",
                    "bytes",
                    "removed",
                    // Task 6: one verdict per `merge_icon` item, and the
                    // run's own failure tally — see `ApplyOutcome::icons`
                    // and `ApplyOutcome::failed`.
                    "icons",
                    "failed",
                ],
            );
        }

        /// Deliberately **not** camelCased — `job_id` matches `LayoutResult`
        /// and `PreloadResult`, which do the same, and `src/lib/osinstall.ts`
        /// declares `job_id` to match. Pinned so a future edit cannot drift
        /// one side without the other noticing.
        #[test]
        fn os_install_result_keeps_job_id_unrenamed_like_its_siblings() {
            let result = OsInstallResult {
                job_id: 7,
                destination: "E:\\dist".into(),
                outcome: ApplyOutcome::default(),
            };
            let value = serde_json::to_value(&result).unwrap();
            expect_keys(&value, &["job_id", "destination", "outcome"]);
        }

        #[test]
        fn install_plan_top_level_keys_are_camelcase() {
            let (plan, _dir) = crate::core::osinstall::fixtures::planned_with(
                &["workbench-base"],
                &["Workbench3.2"],
                Some(47),
            );
            let value = serde_json::to_value(&plan).unwrap();
            expect_keys(
                &value,
                &[
                    "release",
                    "items",
                    "refusals",
                    "activations",
                    "mediaStamps",
                    "totalBytes",
                    "totalFiles",
                    "componentsOn",
                    "pairedRom",
                    "mediaPaths",
                    "packages",
                    "packageMedia",
                    "userStartup",
                    "removals",
                ],
            );
        }

        #[test]
        fn plan_item_serializes_is_dir_as_camelcase() {
            use crate::core::osinstall::plan::PlanItem;

            let item = PlanItem {
                component: "workbench-base".into(),
                media: "Workbench3.2".into(),
                from: "C".into(),
                to: "C".into(),
                is_dir: true,
                bytes: 0,
                decompress: false,
                merge_icon: false,
            };
            let value = serde_json::to_value(&item).unwrap();
            expect_keys(
                &value,
                &[
                    "component",
                    "media",
                    "from",
                    "to",
                    "isDir",
                    "bytes",
                    // ART-228: whether `apply` expands these bytes on the
                    // way in. On the wire because the file list is the only
                    // place a person can see that `dos.catalog.Z` on the
                    // medium becomes `dos.catalog` in the tree.
                    "decompress",
                    // Whether this is a `RuleKind::IconTooltypes` item — see
                    // `PlanItem::merge_icon`'s own doc comment.
                    "mergeIcon",
                ],
            );
            assert_eq!(value["isDir"], true);
        }

        #[test]
        fn user_startup_contribution_serializes_with_the_keys_the_frontend_declares() {
            use crate::core::osinstall::plan::UserStartupContribution;

            let contribution = UserStartupContribution {
                component: "amissl".into(),
                lines: vec!["Assign AmiSSL: SYS:".into()],
            };
            let value = serde_json::to_value(&contribution).unwrap();
            expect_keys(&value, &["component", "lines"]);
        }

        #[test]
        fn media_scan_result_tag_and_field_spellings() {
            let found = MediaScanResult::Found { media: vec![] };
            let value = serde_json::to_value(&found).unwrap();
            assert_eq!(value["outcome"], "found");
            expect_keys(&value, &["outcome", "media"]);

            let unreadable = MediaScanResult::FolderUnreadable {
                folder: "E:\\x".into(),
            };
            let value = serde_json::to_value(&unreadable).unwrap();
            assert_eq!(value["outcome"], "folder-unreadable");
            expect_keys(&value, &["outcome", "folder"]);
        }

        #[test]
        fn plan_result_tag_and_field_spellings() {
            let (plan, _dir) = crate::core::osinstall::fixtures::planned_with(
                &["workbench-base"],
                &["Workbench3.2"],
                Some(47),
            );
            let value = serde_json::to_value(&PlanResult::Planned {
                plan: Box::new(plan),
            })
            .unwrap();
            assert_eq!(value["outcome"], "planned");
            assert!(value.get("plan").is_some());

            let value = serde_json::to_value(&PlanResult::FolderUnreadable {
                folder: "E:\\x".into(),
            })
            .unwrap();
            assert_eq!(value["outcome"], "folder-unreadable");
            expect_keys(&value, &["outcome", "folder"]);
        }

        /// **G9 fix round.** `install_plan_top_level_keys_are_camelcase`
        /// only checks `InstallPlan`'s own top-level keys, so it never looked
        /// inside `pairedRom` at all — `rename_all = "camelCase"` on a
        /// container does not propagate to a nested struct's own fields, and
        /// `PairedRom` shipped without its own `rename_all` attribute in
        /// review. Checked in both places `PairedRom` is nested — `InstallPlan`
        /// (the plan the frontend receives from `osinstall_plan`) and
        /// `DistributionManifest` (`distribution.json`, read back by a later
        /// task's own card-time check, which is exactly why the coordinator
        /// wants this camelCase everywhere `PairedRom` lands) — so a fix that
        /// only renamed one container's field would still be caught here.
        #[test]
        fn paired_rom_nested_inside_a_plan_or_a_manifest_serialises_with_camelcase_keys() {
            let (plan, _dir) = crate::core::osinstall::fixtures::planned_with(
                &["workbench-base"],
                &["Workbench3.2"],
                Some(47),
            );
            let paired = plan
                .paired_rom
                .clone()
                .expect("planned_with a ROM records the pairing");

            let plan_value = serde_json::to_value(&plan).unwrap();
            expect_keys(
                &plan_value["pairedRom"],
                &[
                    "name",
                    "sha256",
                    "statedMajor",
                    "compatibleModels",
                    "requiresMajor",
                ],
            );

            let manifest = DistributionManifest {
                release: "AmigaOS 3.2".into(),
                built_from: Vec::new(),
                files: Vec::new(),
                paired_rom: Some(paired),
                amiga_installed: Vec::new(),
            };
            let manifest_value = serde_json::to_value(&manifest).unwrap();
            expect_keys(
                &manifest_value["pairedRom"],
                &[
                    "name",
                    "sha256",
                    "statedMajor",
                    "compatibleModels",
                    "requiresMajor",
                ],
            );
        }

        #[test]
        fn rule_kind_spellings() {
            use crate::core::osinstall::RuleKind;

            assert_eq!(serde_json::to_value(RuleKind::File).unwrap(), "file");
            assert_eq!(serde_json::to_value(RuleKind::Subtree).unwrap(), "subtree");
        }

        /// Task 7's fix round, F2: `AddPackageResult::Refused` carries the
        /// typed refusals across the wire — this pins that its own tag and
        /// field spellings are what `src/lib/osinstall.ts`'s `AddPackageResult`
        /// declares, the same discipline every other outbound type here gets.
        #[test]
        fn add_package_result_tag_and_field_spellings() {
            let started = AddPackageResult::Started { job_id: 7 };
            let value = serde_json::to_value(&started).unwrap();
            assert_eq!(value["outcome"], "started");
            expect_keys(&value, &["outcome", "job_id"]);

            let refused = AddPackageResult::Refused {
                refusals: vec![
                    crate::core::osinstall::RefusalReason::PackageComponentMissing {
                        package: "locale-turkish".into(),
                        component: "locale-base".into(),
                    },
                ],
            };
            let value = serde_json::to_value(&refused).unwrap();
            assert_eq!(value["outcome"], "refused");
            expect_keys(&value, &["outcome", "refusals"]);
        }

        #[test]
        fn os_install_collisions_result_serializes_with_the_keys_the_frontend_declares() {
            let result = OsInstallCollisionsResult {
                job_id: 3,
                reports: Vec::new(),
            };
            let value = serde_json::to_value(&result).unwrap();
            expect_keys(&value, &["job_id", "reports"]);
        }

        #[test]
        fn os_install_add_package_result_serializes_with_the_keys_the_frontend_declares() {
            let result = OsInstallAddPackageResult {
                job_id: 3,
                outcome: ApplyOutcome::default(),
            };
            let value = serde_json::to_value(&result).unwrap();
            expect_keys(&value, &["job_id", "outcome"]);
        }

        /// `rename_all = "kebab-case"` on `RefusalReason` renames the
        /// **variant** (the `refusal` tag) only — struct-variant field names
        /// (`volume_name`, and so on) are untouched by it, which is the one
        /// place this whole wire does *not* use `camelCase`. Confirmed here
        /// against literal strings rather than assumed, for every variant.
        #[test]
        fn refusal_reason_tag_and_field_spellings_for_every_variant() {
            use crate::core::osinstall::{RefusalReason, RuleKind};

            // Exhaustive **by construction** (F6 of Task 7's own fix round —
            // the six package variants were simply absent from `cases`
            // below until that review, and a plain `Vec` cannot itself
            // notice a missing entry). This match has no body worth
            // running; its only job is to fail to *compile* the moment a
            // fifteenth `RefusalReason` variant exists without an arm
            // here, which is the signal to add its own case below too.
            #[allow(dead_code)]
            fn every_variant_is_matched(reason: RefusalReason) {
                match reason {
                    RefusalReason::KeymapMissing { .. }
                    | RefusalReason::MediaMissing { .. }
                    | RefusalReason::MediaPathMissing { .. }
                    | RefusalReason::MediaUnreadable { .. }
                    | RefusalReason::RomUnknown
                    | RefusalReason::DestinationCollision { .. }
                    | RefusalReason::MediaAmbiguous { .. }
                    | RefusalReason::LayersShareFolder { .. }
                    | RefusalReason::ExclusiveGroupConflict { .. }
                    | RefusalReason::RuleKindMismatch { .. }
                    | RefusalReason::PackageUnknown { .. }
                    | RefusalReason::PackageFolderMissing { .. }
                    | RefusalReason::PackageRequirementMissing { .. }
                    | RefusalReason::PackageComponentMissing { .. }
                    | RefusalReason::PackageArchiveMissing { .. }
                    | RefusalReason::PackageArchiveAmbiguous { .. }
                    | RefusalReason::PackageNotPlaceableOnHost { .. }
                    | RefusalReason::ActivationSourceMissing { .. } => {}
                }
            }

            let cases: Vec<(RefusalReason, &str, &[&str])> = vec![
                (
                    RefusalReason::ActivationSourceMissing {
                        component: "storage".into(),
                        name: "NTSC".into(),
                        from: "Storage/Monitors/NTSC".into(),
                    },
                    "activation-source-missing",
                    &["refusal", "component", "name", "from"],
                ),
                (
                    RefusalReason::MediaMissing {
                        component: "extras".into(),
                        volume_name: "Extras3.2".into(),
                    },
                    "media-missing",
                    &["refusal", "component", "volume_name"],
                ),
                (
                    RefusalReason::MediaPathMissing {
                        component: "extras".into(),
                        media: "Extras3.2".into(),
                        path: "L".into(),
                    },
                    "media-path-missing",
                    &["refusal", "component", "media", "path"],
                ),
                (
                    RefusalReason::MediaUnreadable {
                        component: "extras".into(),
                        volume_name: "Extras3.2".into(),
                        path: r"D:\media\extras.iso".into(),
                        reason: "walk depth exceeded".into(),
                    },
                    "media-unreadable",
                    &["refusal", "component", "volume_name", "path", "reason"],
                ),
                (RefusalReason::RomUnknown, "rom-unknown", &["refusal"]),
                (
                    RefusalReason::DestinationCollision {
                        path: "C/Assign".into(),
                        components: vec!["a".into(), "b".into()],
                    },
                    "destination-collision",
                    &["refusal", "path", "components"],
                ),
                (
                    RefusalReason::MediaAmbiguous {
                        component: "workbench-base".into(),
                        volume_name: "Workbench3.2".into(),
                        paths: vec!["a".into()],
                    },
                    "media-ambiguous",
                    &["refusal", "component", "volume_name", "paths"],
                ),
                (
                    RefusalReason::LayersShareFolder {
                        layers: vec!["base".into(), "up".into()],
                        folder: r"D:\media\everything".into(),
                    },
                    "layers-share-folder",
                    &["refusal", "layers", "folder"],
                ),
                (
                    RefusalReason::ExclusiveGroupConflict {
                        group: "modules".into(),
                        components: vec!["a".into(), "b".into()],
                    },
                    "exclusive-group-conflict",
                    &["refusal", "group", "components"],
                ),
                (
                    RefusalReason::RuleKindMismatch {
                        component: "a".into(),
                        from: "C".into(),
                        expected: RuleKind::File,
                        found: RuleKind::Subtree,
                    },
                    "rule-kind-mismatch",
                    &["refusal", "component", "from", "expected", "found"],
                ),
                (
                    RefusalReason::PackageUnknown {
                        package: "locale-turkish".into(),
                    },
                    "package-unknown",
                    &["refusal", "package"],
                ),
                (
                    RefusalReason::PackageFolderMissing {
                        packages: vec!["locale-turkish".into()],
                    },
                    "package-folder-missing",
                    &["refusal", "packages"],
                ),
                (
                    RefusalReason::PackageRequirementMissing {
                        package: "boingbag-39-2".into(),
                        requires: "boingbag-39-1".into(),
                    },
                    "package-requirement-missing",
                    &["refusal", "package", "requires"],
                ),
                (
                    RefusalReason::PackageComponentMissing {
                        package: "locale-turkish".into(),
                        component: "locale-base".into(),
                    },
                    "package-component-missing",
                    &["refusal", "package", "component"],
                ),
                (
                    RefusalReason::PackageArchiveMissing {
                        package: "locale-turkish".into(),
                        media: "LocaleUpdate".into(),
                    },
                    "package-archive-missing",
                    &["refusal", "package", "media"],
                ),
                (
                    RefusalReason::PackageArchiveAmbiguous {
                        package: "locale-turkish".into(),
                        media: "LocaleUpdate".into(),
                        paths: vec!["a".into(), "b".into()],
                    },
                    "package-archive-ambiguous",
                    &["refusal", "package", "media", "paths"],
                ),
                (
                    RefusalReason::PackageNotPlaceableOnHost {
                        package: "boingbag-39-1".into(),
                        block: HostPlacementBlock::EncryptedPayload,
                    },
                    "package-not-placeable-on-host",
                    &["refusal", "package", "block"],
                ),
            ];

            for (reason, tag, fields) in cases {
                let value = serde_json::to_value(&reason).unwrap();
                assert_eq!(value["refusal"], tag, "{value}");
                expect_keys(&value, fields);
            }
        }
    }
}
