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

use std::collections::{BTreeSet, HashMap, VecDeque};
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, State};

use crate::core::error::{CoreError, CoreResult};
use crate::core::jobs::ProgressSink;
use crate::core::oplog::{JsonlOperationLog, OperationOutcome, OperationRecord};
use crate::core::osinstall::apply::{
    add_package, apply, ApplyOutcome, DistributionManifest, MANIFEST_FILE_NAME,
};
use crate::core::osinstall::collide::{self, CollisionReport, Incoming};
use crate::core::osinstall::package::{self, Package};
use crate::core::osinstall::plan::{
    detect_package_refusals, expand_rules, plan, InstallPlan, InstallRequest,
};
use crate::core::osinstall::recipe;
use crate::core::osinstall::scan::{
    find_media, find_packages, open_package, package_for, FoundMedia, FoundPackage, MediaMatch,
    PackageMedium,
};
use crate::core::osinstall::verify::{verify_volume, VerifyReport};
use crate::core::osinstall::RefusalReason;
use crate::error::{AppError, AppResult};

use super::jobs::{spawn_job, JobRegistry};
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
#[tauri::command]
pub fn osinstall_destination_taken(destination: PathBuf) -> AppResult<bool> {
    Ok(destination.try_exists().unwrap_or(false))
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
    if let Err(CoreError::Io(_)) = find_media(&request.media_folder) {
        return Ok(PlanResult::FolderUnreadable {
            folder: request.media_folder.display().to_string(),
        });
    }
    let recipe = recipe::by_release(&request.release)?;
    Ok(PlanResult::Planned {
        plan: Box::new(plan(&request, &recipe)?),
    })
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
    /// side calls it, shown untranslated as the row's own label.
    pub media: String,
    pub required: bool,
    pub available: bool,
    /// [`Condition::RomOlderThan`]'s `major`, flattened — `None` for an
    /// unconditional component. Flattened rather than mirrored because the
    /// screen's whole conditional-reason vocabulary is written in terms of a
    /// major number; the `match` below is exhaustive, so a second `Condition`
    /// variant is a compile error here and cannot silently arrive on screen
    /// as "unconditional".
    pub condition_major: Option<u16>,
    pub exclusive_group: Option<String>,
}

impl From<&crate::core::osinstall::Component> for ComponentSummary {
    fn from(component: &crate::core::osinstall::Component) -> Self {
        use crate::core::osinstall::Condition;
        Self {
            id: component.id.clone(),
            media: component.media.clone(),
            required: component.required,
            available: component.available,
            condition_major: component.condition.map(|condition| match condition {
                Condition::RomOlderThan { major } => major,
            }),
            exclusive_group: component.exclusive_group.clone(),
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
/// `available` is **not** "ART knows how to install this" (every shipped
/// package always does, or it would not be shipped) — it is "an archive
/// carrying this package's own top-level directory name was actually found
/// in `package_folder`". A checkbox for a package whose file is absent is a
/// promise ART cannot keep, so the screen needs this before it ever offers
/// the tick.
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
#[tauri::command]
pub fn osinstall_packages(package_folder: PathBuf) -> AppResult<Vec<PackageSummary>> {
    let found = find_packages(&package_folder).unwrap_or_default();
    let packages = package::packages()?;
    Ok(packages
        .into_iter()
        .map(|p| {
            let available = found.iter().any(|f| f.media == p.media);
            PackageSummary {
                id: p.id,
                name: p.name,
                requires: p.requires,
                requires_components: p.requires_components,
                available,
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
fn sweep_stale_preview_scratch_dirs() {
    let Ok(entries) = std::fs::read_dir(std::env::temp_dir()) else {
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
fn preview_cache_dir(key: &PreviewCacheKey) -> PathBuf {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    key.hash(&mut hasher);
    std::env::temp_dir().join(format!("{PREVIEW_SCRATCH_PREFIX}{:016x}", hasher.finish()))
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
    match package_for(found, &package.media) {
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

    let dir = preview_cache_dir(&key);
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
    progress: &dyn ProgressSink,
) -> CoreResult<Vec<CollisionReport>> {
    if ordered.is_empty() {
        return Ok(Vec::new());
    }
    sweep_stale_preview_scratch_dirs();
    let incoming = extract_incoming_for_preview(package_folder, ordered, catalogue, progress)?;
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

    let id = spawn_job(&app, registry, &title, move |job_id, progress| {
        let reports =
            preview_collisions(&tree_root, &package_folder, &ordered, &catalogue, progress)?;
        let _ = emit_app.emit(
            OSINSTALL_COLLISIONS_EVENT,
            OsInstallCollisionsResult { job_id, reports },
        );
        Ok(())
    });

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
            };
            let mut failure: Option<CoreError> = None;
            for (package, archive) in &resolved {
                match add_package(&root, package, archive, progress) {
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

    let id = spawn_job(&app, registry, &title, move |job_id, progress| {
        let outcome = apply(&plan, &root, progress);

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
            Ok(done) => record
                .detail("Files", done.files.to_string())
                .detail("Directories", done.directories.to_string())
                .detail("Bytes", done.bytes.to_string())
                // Verification is its own step (`osinstall_verify`), run
                // against the volume this tree is later copied onto — not
                // here, where nothing has been read back yet.
                .outcome(OperationOutcome::verified(false)),
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
        let dir =
            std::env::temp_dir().join(format!("art-osinstall-cmd-{tag}-{}", std::process::id()));
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
        }
    }

    /// `locale-turkish`'s own archive, shaped exactly like the real one
    /// measured against `BoingBag39-2-turkce.lha` (`locale-turkish.json`'s
    /// own doc comment): loose files (no nested `member`) under
    /// `locale/catalogs`, lower-case, inside a top-level `LocaleUpdate`
    /// drawer (the package's own `media`).
    fn write_locale_turkish_archive(folder: &Path, file_name: &str, catalog_bytes: &[u8]) {
        std::fs::write(
            folder.join(file_name),
            crate::core::archive::zip::tests::make_zip_with(&[(
                "LocaleUpdate/locale/catalogs/x.catalog",
                catalog_bytes,
            )]),
        )
        .unwrap();
    }

    /// The checklist always lists all three shipped packages, and never
    /// claims one is available when its own archive was never provided —
    /// "a checkbox for a package whose file is absent is a promise ART
    /// cannot keep."
    #[test]
    fn osinstall_packages_reports_whether_each_archive_was_actually_found() {
        let dir = scratch("packages-availability");
        let folder = dir.join("packages");
        std::fs::create_dir_all(&folder).unwrap();
        write_locale_turkish_archive(&folder, "turkish.zip", b"catalog bytes");

        let summaries = osinstall_packages(folder).unwrap();
        assert_eq!(summaries.len(), 3, "ART ships exactly three packages today");

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

    /// An unreadable/nonexistent package folder is answered the same way an
    /// empty one is — every package `available: false` — never refused: the
    /// checklist itself must still render.
    #[test]
    fn osinstall_packages_over_a_missing_folder_lists_nothing_as_available() {
        let dir = scratch("packages-missing-folder");
        let missing = dir.join("does-not-exist");

        let summaries = osinstall_packages(missing).unwrap();
        assert_eq!(summaries.len(), 3);
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
        std::fs::create_dir_all(tree.join("Locale").join("Catalogs")).unwrap();
        std::fs::write(
            tree.join("Locale").join("Catalogs").join("x.catalog"),
            b"$VER: x.catalog 1.0 (1.1.20)",
        )
        .unwrap();
        write_test_manifest(
            &tree,
            vec![locale_base_file_record("Locale/Catalogs/x.catalog")],
        );

        let packages_dir = dir.join("packages");
        std::fs::create_dir_all(&packages_dir).unwrap();
        write_locale_turkish_archive(
            &packages_dir,
            "turkish.zip",
            b"$VER: x.catalog 2.0 (1.1.21)",
        );

        // `preview_collisions`, not the `#[tauri::command]` itself — the
        // real command now needs a live `AppHandle`/`State` to spawn its
        // job (F4 of Task 7's own fix round), so this tests the same
        // read-only work directly, the way `resolve_packages_for_add` and
        // `osinstall_verify`'s own `verify_at` already are.
        let catalogue = package::packages().unwrap();
        let ordered = package::order(&["locale-turkish".to_string()]).unwrap();
        let reports =
            preview_collisions(&tree, &packages_dir, &ordered, &catalogue, &NoProgress).unwrap();

        assert_eq!(reports.len(), 1, "{reports:?}");
        assert_eq!(reports[0].path, "Locale/Catalogs/x.catalog");
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

    /// No packages chosen previews as nothing to report, without opening
    /// either folder — the empty selection is the common case every time
    /// the panel loads before a checkbox is ticked.
    #[test]
    fn osinstall_collisions_with_nothing_chosen_is_empty() {
        let dir = scratch("collisions-empty");
        let tree = dir.join("does-not-exist-tree");
        let packages_dir = dir.join("does-not-exist-packages");

        let catalogue = package::packages().unwrap();
        let reports =
            preview_collisions(&tree, &packages_dir, &[], &catalogue, &NoProgress).unwrap();
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
        write_locale_turkish_archive(&packages_dir, "turkish.zip", b"catalog bytes");

        let catalogue = package::packages().unwrap();
        let package = catalogue.iter().find(|p| p.id == "locale-turkish").unwrap();
        let found = crate::core::osinstall::scan::find_packages(&packages_dir).unwrap();
        let archive = resolve_package_archive(package, &found).unwrap();

        let mut files = 0usize;
        let mut bytes = 0u64;
        let first =
            extract_package_items(package, archive, &mut files, &mut bytes, &NoProgress).unwrap();
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
        let second =
            extract_package_items(package, archive, &mut files2, &mut bytes2, &NoProgress).unwrap();
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
        write_locale_turkish_archive(&packages_dir, "turkish.zip", b"catalog bytes");

        let catalogue = package::packages().unwrap();
        let package = catalogue.iter().find(|p| p.id == "locale-turkish").unwrap();
        let found = crate::core::osinstall::scan::find_packages(&packages_dir).unwrap();
        let archive = resolve_package_archive(package, &found).unwrap();

        let mut files = 0usize;
        let mut bytes = 0u64;
        let cancel = crate::core::osinstall::fixtures::CancelAfter::new(0);
        let err =
            extract_package_items(package, archive, &mut files, &mut bytes, &cancel).unwrap_err();
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
        write_locale_turkish_archive(&packages_dir, "turkish.zip", b"catalog bytes");

        let catalogue = package::packages().unwrap();
        let cancel = crate::core::osinstall::fixtures::CancelAfter::new(0);
        let err = extract_incoming_for_preview(
            &packages_dir,
            &["locale-turkish".to_string()],
            &catalogue,
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

        sweep_stale_preview_scratch_dirs();

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
            }],
        );

        let packages_dir = dir.join("packages");
        std::fs::create_dir_all(&packages_dir).unwrap();
        write_locale_turkish_archive(&packages_dir, "turkish.zip", b"catalog bytes");

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
        write_locale_turkish_archive(&packages_dir, "turkish.zip", b"catalog bytes");

        let resolved =
            resolve_packages_for_add(&tree, &packages_dir, &["locale-turkish".to_string()])
                .unwrap()
                .unwrap();
        assert_eq!(resolved.len(), 1);
        assert_eq!(resolved[0].0.id, "locale-turkish");
        assert_eq!(resolved[0].1, packages_dir.join("turkish.zip"));
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
            rom: None,
            chosen: vec!["workbench-base".to_string()],
            excluded: Vec::new(),
            destination: dir.join("dist"),
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
            rom: None,
            chosen: vec!["workbench-base".to_string()],
            excluded: Vec::new(),
            destination: dir.join("dist"),
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
            }],
            paired_rom: None,
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
            };
            let value = serde_json::to_value(&media).unwrap();
            expect_keys(&value, &["path", "volumeName", "kind"]);
            assert_eq!(value["kind"], "floppy");
        }

        /// `ComponentSummary` is the checklist on screen, so a key renamed
        /// on one side only would empty a row rather than fail a build —
        /// `src/lib/osinstall.ts`'s `ComponentDef` declares exactly these
        /// six.
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
                    "required",
                    "available",
                    "conditionMajor",
                    "exclusiveGroup",
                ],
            );
            // A real conditional, exclusive-group component — so both
            // optional fields are pinned as values, not merely as present
            // nulls.
            assert_eq!(value["conditionMajor"], 47);
            assert_eq!(value["exclusiveGroup"], "modules");
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
            };
            let value = serde_json::to_value(&outcome).unwrap();
            expect_keys(&value, &["root", "files", "directories", "bytes"]);
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
                    "totalBytes",
                    "componentsOn",
                    "pairedRom",
                    "mediaPaths",
                    "packages",
                    "packageMedia",
                    "userStartup",
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
            };
            let value = serde_json::to_value(&item).unwrap();
            expect_keys(
                &value,
                &["component", "media", "from", "to", "isDir", "bytes"],
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
            // fourteenth `RefusalReason` variant exists without an arm
            // here, which is the signal to add its own case below too.
            #[allow(dead_code)]
            fn every_variant_is_matched(reason: RefusalReason) {
                match reason {
                    RefusalReason::MediaMissing { .. }
                    | RefusalReason::MediaPathMissing { .. }
                    | RefusalReason::RomUnknown
                    | RefusalReason::DestinationCollision { .. }
                    | RefusalReason::MediaAmbiguous { .. }
                    | RefusalReason::ExclusiveGroupConflict { .. }
                    | RefusalReason::RuleKindMismatch { .. }
                    | RefusalReason::PackageUnknown { .. }
                    | RefusalReason::PackageFolderMissing { .. }
                    | RefusalReason::PackageRequirementMissing { .. }
                    | RefusalReason::PackageComponentMissing { .. }
                    | RefusalReason::PackageArchiveMissing { .. }
                    | RefusalReason::PackageArchiveAmbiguous { .. } => {}
                }
            }

            let cases: Vec<(RefusalReason, &str, &[&str])> = vec![
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
            ];

            for (reason, tag, fields) in cases {
                let value = serde_json::to_value(&reason).unwrap();
                assert_eq!(value["refusal"], tag, "{value}");
                expect_keys(&value, fields);
            }
        }
    }
}
