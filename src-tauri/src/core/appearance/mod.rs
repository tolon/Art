//! Applying a wallpaper, a screen depth and the shell defaults to an already
//! built distribution tree — **Task 7 of the prefs-and-wallpaper round, and
//! the first real caller of Tasks 1-6.**
//!
//! Everything downstream of here (`iff`, `wbpattern`, `screenmode`, `env`,
//! `ilbm`, `picture`) was built and unit-tested in isolation. A previous
//! round in this project shipped two features that were unreachable from the
//! product while every test stayed green, because no task owned the new
//! producers' first real caller. This module is that owner: it is the one
//! place that reads a real `WBPattern.prefs`/`ScreenMode.prefs` out of a real
//! tree, decides what to change, and writes the result back.
//!
//! # Read, modify, write back — never construct a prefs file from scratch
//!
//! A release's `WBPattern.prefs` carries three `PTRN` chunks, and a user who
//! sets only the root backdrop must keep the other two exactly as the
//! release shipped them — `wbp_Revision` and every flag bit this module
//! cannot name included. So [`apply_appearance`] always: parses the file
//! with [`iff::parse`], finds the one chunk it is asked about, decodes it
//! with [`wbpattern::read_backdrop`]/[`screenmode::read_screen_mode`],
//! changes only the fields the caller named, and calls
//! [`iff::PrefsFile::replace_bodies`] naming **only that chunk's index**.
//! Every other chunk in the file — `PRHD`, the other two `PTRN`s, anything
//! this module does not understand — is copied back byte for byte.
//!
//! Even the chunk being edited keeps what this module has no field for:
//! [`wbpattern::DEFAULT_PICTURE_FLAGS`] fixes `Precision`/`Dither`/`no_remap`
//! for a picture backdrop the way the release itself sets them, and the
//! caller's requested [`wbpattern::Placement`] replaces just the placement
//! bits — but `other_flags` and `wbp_Revision` are carried forward from
//! whatever that chunk already held, because the user asked ART to change
//! the picture and the placement, not to invent a value for a byte nobody
//! named.
//!
//! # Refuse, never substitute
//!
//! A missing `WBPattern.prefs`, a picture that will not decode, a backdrop
//! name that already exists in the drawer — every one of these is refused
//! by name, before a single byte is written anywhere. [`apply_appearance`]
//! builds a full, validated plan for every part of the request first
//! (`plan_wallpaper`, `plan_screen_mode`, `plan_shell_defaults`) and only
//! then commits any of it — so a picture that turns out not to be a
//! picture, discovered while building the wallpaper plan, leaves a
//! requested screen-depth change uncommitted too, not half-applied.
//!
//! # The `.uaem` sidecar
//!
//! `core/osinstall/apply.rs::settle_sidecar` exists to keep a **re-copied**
//! file's Amiga protection bits, date and comment attached when the file
//! itself is rewritten from real install media. Nothing here re-copies
//! anything from media: a picture this module places is a brand-new
//! host-authored file with no prior Amiga-side existence to preserve, and
//! `core::volume::write::copy::host_metadata` already defines the right
//! answer for that case — a host file with no `.uaem` beside it gets the
//! file's own mtime and `default_protection()`, exactly the fallback every
//! other host-authored file in a distribution tree already relies on. So no
//! sidecar is written for the placed picture. `WBPattern.prefs` and
//! `ScreenMode.prefs` may already carry a sidecar from the original OS
//! install recording what the release shipped; this module only rewrites
//! their *content* in place at the same path, and touches no sidecar for
//! them either — the protection bits and comment a release shipped are still
//! correct after a content-only edit, and nothing here has a basis for
//! asserting a new Amiga-side date.

use std::path::{Path, PathBuf};

use crate::core::amigaicon;
use crate::core::amigaprefs::{env, iff, screenmode, wbpattern};
use crate::core::error::{CoreError, CoreResult};
use crate::core::icongrid;
use crate::core::ilbm;
use crate::core::jobs::{NoProgress, ProgressSink};
use crate::core::picture;
use crate::core::safety::backup::BACKUP_DIR;
use crate::core::safety::{guarded_write, BackupPolicy};

/// `Prefs/Env-Archive/Sys/WBPattern.prefs`, relative to a distribution
/// tree's root, resolved case-insensitively (see [`resolve_ci`]).
const WBPATTERN_REL: &str = "Prefs/Env-Archive/Sys/WBPattern.prefs";

/// `Prefs/Env-Archive/Sys/ScreenMode.prefs`, same rule.
const SCREENMODE_REL: &str = "Prefs/Env-Archive/Sys/ScreenMode.prefs";

/// `Prefs/Env-Archive`, the root the five [`env::SHELL_DEFAULTS`] values are
/// written under.
const ENV_ARCHIVE_REL: &str = "Prefs/Env-Archive";

/// Where ART places a host picture once it has been turned into an ILBM
/// backdrop — matches the measured `Sys:Prefs/Presets/Backdrops/…` path in
/// `wbpattern`'s own module doc. Also the drawer [`backdrops_in_tree`] lists.
pub const BACKDROPS_DRAWER: [&str; 3] = ["Prefs", "Presets", "Backdrops"];

/// Fallback screen dimensions used to [`picture::scale_to_fit`] a host
/// picture when the tree's own `ScreenMode.prefs` states no concrete size
/// (`Width`/`Height` == [`screenmode::USE_MODE_DEFAULT`]) or cannot be read
/// at all. Not measured — reasoned: it matches [`env::SHELL_DEFAULTS`]'s own
/// `Sys/def_width` / `Sys/def_height` (640x256), the release's own default
/// shell dimensions, rather than inventing an unrelated number.
const DEFAULT_SCREEN_SIZE: (u16, u16) = (640, 256);

/// Where a wallpaper's picture data comes from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WallpaperSource {
    /// A picture already present in the tree, named by its Amiga path (e.g.
    /// `"Sys:Prefs/Presets/Backdrops/default_pal.iff"`). Checked against the
    /// real tree before anything is written — see the module doc's
    /// "Refuse, never substitute".
    AlreadyInTree { amiga_path: String },
    /// A PNG or JPEG on the host, to be decoded, scaled, quantised to
    /// `colours` and encoded to ILBM.
    HostPicture { path: PathBuf, colours: usize },
}

/// One call to [`apply_appearance`]: any subset of a wallpaper assignment, a
/// screen depth, and the five shell defaults.
#[derive(Debug, Clone)]
pub struct AppearanceRequest {
    pub wallpaper: Option<(wbpattern::Which, WallpaperSource, wbpattern::Placement)>,
    pub screen_depth: Option<u16>,
    pub shell_defaults: bool,
    /// Lay out every icon in the tree whose position is
    /// [`amigaicon::NO_POSITION`] — the root (`SYS:`, [`icongrid::ROOT_INNER_WIDTH`])
    /// and every drawer ([`icongrid::DRAWER_INNER_WIDTH`]) alike. An icon the
    /// release already positioned keeps that position untouched — see
    /// [`plan_icon_arrangement`]'s own doc for the scoping rule and why.
    pub arrange_icons: bool,
}

/// What [`apply_appearance`] actually did, so the caller can tell the user
/// exactly where the previous version of anything went (spec §92).
#[derive(Debug, Clone, Default)]
pub struct AppearanceOutcome {
    /// Every file written, in the order it was written.
    pub written: Vec<PathBuf>,
    /// Every backup [`guarded_write`] took, in the same order as `written`
    /// (one entry per file that already existed).
    pub backups: Vec<PathBuf>,
    /// The host path of a newly placed backdrop picture, when the request
    /// carried a [`WallpaperSource::HostPicture`].
    pub picture_placed: Option<PathBuf>,
    /// The Amiga-side path now written into the `PTRN` chunk, when the
    /// request carried a wallpaper assignment at all.
    pub amiga_path: Option<String>,
    /// How many icons were newly given a position — always icons that were
    /// carrying [`amigaicon::NO_POSITION`] before this call. An icon the
    /// release already positioned is never counted here, no matter how many
    /// times its drawer was arranged.
    pub icons_placed: usize,
    /// How many directories (root or drawer) actually had at least one icon
    /// newly placed. A directory whose icons were already all positioned is
    /// not counted — see [`plan_icon_arrangement`]'s own doc.
    pub drawers_arranged: usize,
    /// Every `.info` that could not be parsed and was therefore left exactly
    /// as it was, named rather than silently dropped (CLAUDE.md: "never
    /// claim what you did not do"). One malformed icon must not cost the
    /// other icons in its drawer their layout, so this can be non-empty on
    /// an otherwise successful call.
    pub icons_skipped: Vec<PathBuf>,
}

fn malformed_or_missing(rel: &str, root: &Path) -> CoreError {
    CoreError::InvalidInput(format!(
        "'{rel}' was not found under this distribution tree ('{}')",
        root.display()
    ))
}

/// [`crate::core::osinstall::resolve_ci_optional`], refusing by name — naming
/// the full relative path asked for, so a user missing
/// `Prefs/Env-Archive/Sys/WBPattern.prefs` is told exactly that rather than a
/// generic "not found". `find_child_ci`/`resolve_ci_optional` themselves live
/// in `core::osinstall` — see that module's own doc comment on
/// `find_child_ci` for why: this module is built *on top of* `osinstall`
/// already (`amiga_names_equal` comes from there too), and the shared
/// case-insensitive walk lives in the lower-level module both this module
/// and `osinstall::verify::check_prefs_paths` (Task 8) import, rather than
/// either holding its own copy.
fn resolve_ci(root: &Path, rel: &str) -> CoreResult<PathBuf> {
    crate::core::osinstall::resolve_ci_optional(root, rel)?
        .ok_or_else(|| malformed_or_missing(rel, root))
}

/// Resolve `components` under `root`, matching an existing directory
/// case-insensitively and falling back to the exact-case join for whatever
/// does not exist yet — it does **not** create anything. Callers that must
/// actually create the missing pieces do so with `std::fs::create_dir_all`
/// on the path this returns, at commit time, never during planning (see the
/// module doc's "Refuse, never substitute").
fn resolve_dir_ci_or_default(root: &Path, components: &[&str]) -> CoreResult<PathBuf> {
    let mut current = root.to_path_buf();
    for &segment in components {
        current = match crate::core::osinstall::find_child_ci(&current, segment)? {
            Some(next) => next,
            None => current.join(segment),
        };
    }
    Ok(current)
}

/// The Amiga-side path ART writes into a `PTRN` chunk for a picture it just
/// placed in [`BACKDROPS_DRAWER`].
fn backdrop_amiga_path(file_name: &str) -> String {
    format!("Sys:{}/{file_name}", BACKDROPS_DRAWER.join("/"))
}

/// Resolve an Amiga path (`"Sys:…"`) claimed by
/// [`WallpaperSource::AlreadyInTree`] against the real tree, case-insensitive
/// component by component — the same rule Task 8's `check_prefs_paths` uses.
/// Refuses by name rather than trusting the claim (module doc: "Refuse,
/// never substitute").
fn resolve_amiga_path(tree: &Path, amiga_path: &str) -> CoreResult<PathBuf> {
    // The assign itself is folded case-insensitively, the same as every
    // other AmigaDOS name `resolve_ci` compares component by component below
    // — real material carries `SYS:` as well as `Sys:` (whole-branch review
    // finding I3; see `crate::core::osinstall::strip_sys_prefix_ci`).
    let rel = crate::core::osinstall::strip_sys_prefix_ci(amiga_path).ok_or_else(|| {
        CoreError::InvalidInput(format!(
            "'{amiga_path}' does not start with 'Sys:' — ART only places a backdrop on the \
             system volume"
        ))
    })?;
    resolve_ci(tree, rel)
}

/// Best-effort screen size to scale a host picture to. Reads the tree's own
/// `ScreenMode.prefs` when it is present and its `Width`/`Height` state a
/// concrete size; falls back to [`DEFAULT_SCREEN_SIZE`] for anything else —
/// a missing file, one that fails to parse, or a sentinel `Width`/`Height`.
/// This is a scaling hint, not a fact ART asserts to the user, so a
/// corruption unrelated to the wallpaper request must not block it.
fn screen_size_for_scaling(tree: &Path) -> (u16, u16) {
    crate::core::osinstall::resolve_ci_optional(tree, SCREENMODE_REL)
        .ok()
        .flatten()
        .and_then(|path| std::fs::read(path).ok())
        .and_then(|bytes| iff::parse(&bytes).ok())
        .and_then(|prefs| {
            let index = prefs.chunks().iter().position(|c| c.id == *b"SCRM")?;
            let body = prefs.body(index).ok()?;
            screenmode::read_screen_mode(body).ok()
        })
        .map(|mode| {
            let width = if mode.width == screenmode::USE_MODE_DEFAULT {
                DEFAULT_SCREEN_SIZE.0
            } else {
                mode.width
            };
            let height = if mode.height == screenmode::USE_MODE_DEFAULT {
                DEFAULT_SCREEN_SIZE.1
            } else {
                mode.height
            };
            (width, height)
        })
        .unwrap_or(DEFAULT_SCREEN_SIZE)
}

/// The index of the one `PTRN` chunk whose `Which` matches. Refused by name
/// rather than defaulting to "the first PTRN" — a release that, for
/// whatever reason, does not carry a slot ART was asked to set is a state
/// this module reports rather than guesses past.
fn find_ptrn_index(prefs: &iff::PrefsFile, which: wbpattern::Which) -> CoreResult<usize> {
    for (index, chunk) in prefs.chunks().iter().enumerate() {
        if chunk.id != *b"PTRN" {
            continue;
        }
        let backdrop = wbpattern::read_backdrop(prefs.body(index)?)?;
        if backdrop.which == which {
            return Ok(index);
        }
    }
    Err(CoreError::InvalidInput(format!(
        "'{WBPATTERN_REL}' carries no PTRN chunk for {which:?}"
    )))
}

/// A fully validated wallpaper change, ready to commit: nothing here can
/// still fail for a reason `apply_appearance` did not already check.
struct WallpaperPlan {
    prefs_path: PathBuf,
    /// The whole rebuilt `WBPattern.prefs`, produced by
    /// [`iff::PrefsFile::replace_bodies`] naming only the one chunk index
    /// that changed.
    prefs_bytes: Vec<u8>,
    /// `(host path, ILBM bytes)` when the source was a [`WallpaperSource::HostPicture`].
    /// `None` for [`WallpaperSource::AlreadyInTree`] — nothing new to place.
    picture: Option<(PathBuf, Vec<u8>)>,
    amiga_path: String,
}

fn plan_wallpaper(
    tree: &Path,
    which: wbpattern::Which,
    source: &WallpaperSource,
    placement: wbpattern::Placement,
) -> CoreResult<WallpaperPlan> {
    let prefs_path = resolve_ci(tree, WBPATTERN_REL)?;
    let original = std::fs::read(&prefs_path)?;
    let prefs = iff::parse(&original)?;
    let chunk_index = find_ptrn_index(&prefs, which)?;
    let existing = wbpattern::read_backdrop(prefs.body(chunk_index)?)?;

    let (amiga_path, picture) = match source {
        WallpaperSource::AlreadyInTree { amiga_path } => {
            resolve_amiga_path(tree, amiga_path)?;
            (amiga_path.clone(), None)
        }
        WallpaperSource::HostPicture { path, colours } => {
            let source_bytes = std::fs::read(path)?;
            let rgb = picture::decode(&source_bytes)?;
            let (max_w, max_h) = screen_size_for_scaling(tree);
            let scaled = picture::scale_to_fit(&rgb, max_w, max_h);
            let indexed = picture::quantise::quantise(&scaled, *colours);
            let ilbm_bytes = ilbm::encode(&indexed)?;

            let stem = path
                .file_stem()
                .and_then(|s| s.to_str())
                .filter(|s| !s.is_empty())
                .ok_or_else(|| {
                    CoreError::InvalidInput(format!(
                        "'{}' has no usable file name to derive a backdrop name from",
                        path.display()
                    ))
                })?;
            let file_name = format!("{stem}.iff");

            // SAFE_CREATE: refuse before anything is written, whether or not
            // the drawer itself exists yet.
            let drawer = resolve_dir_ci_or_default(tree, &BACKDROPS_DRAWER);
            let drawer = drawer?;
            if crate::core::osinstall::find_child_ci(&drawer, &file_name)?.is_some() {
                return Err(CoreError::SafetyRefused(format!(
                    "'{file_name}' already exists in the backdrops drawer; ART does not \
                     overwrite an existing backdrop"
                )));
            }

            let amiga_path = backdrop_amiga_path(&file_name);
            (amiga_path, Some((drawer.join(&file_name), ilbm_bytes)))
        }
    };

    // Everything ART has no field for on the chunk being edited — the
    // release's own unknown flag bits, `wbp_Revision` and the 16 reserved
    // bytes — is carried forward rather than reset; only the content and the
    // requested placement are the user's own choice. See the module doc.
    let new_backdrop = wbpattern::Backdrop {
        reserved: existing.reserved,
        which: existing.which,
        placement,
        precision: wbpattern::Precision::Image,
        dither: wbpattern::Dither::Good,
        no_remap: false,
        other_flags: existing.other_flags,
        revision: existing.revision,
        content: wbpattern::Content::Picture(amiga_path.clone()),
    };
    let new_body = wbpattern::write_backdrop(&new_backdrop)?;
    let prefs_bytes = prefs.replace_bodies(&[(chunk_index, new_body)])?;

    Ok(WallpaperPlan {
        prefs_path,
        prefs_bytes,
        picture,
        amiga_path,
    })
}

/// A fully validated screen-depth change.
struct ScreenModePlan {
    path: PathBuf,
    bytes: Vec<u8>,
}

fn plan_screen_mode(tree: &Path, depth: u16) -> CoreResult<ScreenModePlan> {
    let path = resolve_ci(tree, SCREENMODE_REL)?;
    let original = std::fs::read(&path)?;
    let prefs = iff::parse(&original)?;
    let index = prefs
        .chunks()
        .iter()
        .position(|c| c.id == *b"SCRM")
        .ok_or_else(|| {
            CoreError::InvalidInput(format!("'{SCREENMODE_REL}' carries no SCRM chunk"))
        })?;
    let mut mode = screenmode::read_screen_mode(prefs.body(index)?)?;
    mode.depth = depth;
    let new_body = screenmode::write_screen_mode(&mode)?;
    let bytes = prefs.replace_bodies(&[(index, new_body)])?;
    Ok(ScreenModePlan { path, bytes })
}

/// A fully validated `Prefs/Env-Archive/<name>` write.
struct ShellDefaultPlan {
    path: PathBuf,
    bytes: Vec<u8>,
}

fn plan_shell_defaults(tree: &Path) -> CoreResult<Vec<ShellDefaultPlan>> {
    let archive = resolve_ci(tree, ENV_ARCHIVE_REL)?;
    let mut plans = Vec::with_capacity(env::SHELL_DEFAULTS.len());
    for (name, value) in env::SHELL_DEFAULTS {
        let components: Vec<&str> = name.split('/').collect();
        // `str::split` always yields at least one substring — even `"".split('/')`
        // produces `[""]` — so `components` is never empty and `components.len() - 1`
        // never underflows. Not a runtime guard (by this round's own rule: unreachable
        // by ART's own arithmetic, not by a third party's behaviour) — a `debug_assert!`
        // records the invariant instead of a `CoreError` branch nothing can reach.
        debug_assert!(
            !components.is_empty(),
            "str::split always yields at least one element"
        );
        let (dir_components, file_component) = components.split_at(components.len() - 1);
        let dir = resolve_dir_ci_or_default(&archive, dir_components)?;
        let file_name = file_component[0];
        let path = match crate::core::osinstall::find_child_ci(&dir, file_name)? {
            Some(existing) => existing,
            None => dir.join(file_name),
        };
        let bytes = env::encode_value(value)?;
        plans.push(ShellDefaultPlan { path, bytes });
    }
    Ok(plans)
}

/// One entry inside a directory being arranged: the entry's own `.info`
/// icon path, the icon's original bytes (read once and reused for both the
/// pre-flight check and the eventual write), whether the release already
/// placed it, and the [`icongrid::Cell`] [`icongrid::arrange`] sees for it.
struct IconEntry {
    icon_path: PathBuf,
    original_bytes: Vec<u8>,
    already_placed: bool,
    cell: icongrid::Cell,
}

/// A fully validated icon-arrangement plan across the **whole** tree — root
/// through every drawer — built entirely before [`apply_appearance`] writes
/// anything. See [`plan_icon_arrangement`] for the walk and the scoping
/// rule.
#[derive(Default)]
struct IconArrangementPlan {
    /// `(icon path, new bytes)` for every icon whose position was
    /// [`amigaicon::NO_POSITION`] and has now been given one by
    /// [`icongrid::arrange`].
    writes: Vec<(PathBuf, Vec<u8>)>,
    icons_placed: usize,
    drawers_arranged: usize,
    /// A `.info` that failed to parse, named rather than aborting the whole
    /// run (CLAUDE.md: "never claim what you did not do" — one bad icon must
    /// not cost the other icons in its drawer their layout).
    icons_skipped: Vec<PathBuf>,
}

/// Lay out every icon in `tree` whose position is [`amigaicon::NO_POSITION`]
/// — the owner's own scoping rule (spec §2.4), measured across 798 real
/// icons: 361 carried the sentinel and 437 were positioned by the release.
/// **Only** the 361 are ever touched; an icon that already has a position
/// keeps it byte for byte, and it is still handed to [`icongrid::arrange`]
/// as a [`icongrid::Cell`] so the *computed grid* reserves a slot for it —
/// but see [`plan_icons_in_dir`]'s own doc for exactly what that does and
/// does not guarantee against the icon's real, on-screen position.
///
/// The whole tree is walked and planned — every directory, root included —
/// **before** a single byte is written anywhere (this function does no
/// writing at all): a `.info` that fails to parse is skipped and named in
/// [`IconArrangementPlan::icons_skipped`] rather than aborting the rest of
/// its drawer, but a directory ART cannot even *list*, or a `.info` ART
/// cannot even *read* — a real I/O failure, not a malformed byte layout —
/// aborts the whole call, because that is not "one bad icon", it is "this
/// tree cannot be trusted to plan safely".
///
/// The tree's own top level is the `SYS:` window and uses
/// [`icongrid::ROOT_INNER_WIDTH`] — narrower than
/// [`icongrid::DRAWER_INNER_WIDTH`], because the real window geometry lives
/// in a `disk.info` ART does not write (see `icongrid`'s own module doc).
/// Every other directory, at any depth, is an ordinary drawer and uses
/// [`icongrid::DRAWER_INNER_WIDTH`].
///
/// A directory with no icons at all, or one whose icons are all already
/// placed, is left alone entirely: no [`icongrid::arrange`] call, no write,
/// no backup, and it does not count towards
/// [`IconArrangementPlan::drawers_arranged`].
fn plan_icon_arrangement(tree: &Path) -> CoreResult<IconArrangementPlan> {
    let mut plan = IconArrangementPlan::default();
    plan_icons_in_dir(tree, tree, &mut plan)?;
    Ok(plan)
}

/// One directory's worth of [`plan_icon_arrangement`], then its
/// subdirectories — depth-first, order otherwise unspecified. See that
/// function's own doc for the scoping rule and the skip-vs-abort split.
///
/// **What "occupies a slot" does and does not guarantee.** Every icon in
/// the directory — placed and unplaced alike — is handed to
/// [`icongrid::arrange`] as a [`icongrid::Cell`], so an already-placed
/// icon's size and label really do shape the tidy grid `arrange` computes:
/// the unplaced icons are laid out into the *other* cells of that grid, not
/// on top of the placed one's cell. But `icongrid::arrange` is pure
/// arithmetic over cell sizes (see its own module doc) — it has no concept
/// of an icon's **real** screen coordinate, and the already-placed icon's
/// own computed placement in that grid is discarded, never written. Where
/// that icon's real, on-disk position happens to fall outside the area the
/// freshly computed grid occupies, nothing here checks for it, and nothing
/// here guarantees a newly placed icon avoids it. This only prevents two
/// cells from colliding inside one computed grid; it is not a collision
/// check against an arbitrary real coordinate elsewhere on screen. Making
/// that guarantee real — reading every placed icon's actual position and
/// keeping the grid clear of it — is a design question for a later round,
/// and one only a real rendered Workbench window can settle.
fn plan_icons_in_dir(
    dir: &Path,
    tree_root: &Path,
    plan: &mut IconArrangementPlan,
) -> CoreResult<()> {
    let inner_width = if dir == tree_root {
        icongrid::ROOT_INNER_WIDTH
    } else {
        icongrid::DRAWER_INNER_WIDTH
    };

    let mut entries: Vec<IconEntry> = Vec::new();
    let mut subdirs: Vec<PathBuf> = Vec::new();

    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        let name = entry.file_name().to_string_lossy().into_owned();
        let lower = name.to_ascii_lowercase();

        // `.info` files are icons, not entries an icon describes; `.uaem`
        // sidecars and ART's own backup drawer are not part of the Amiga
        // side of the tree at all — see `core::volume::write::uaem` and
        // `core::safety::backup` respectively.
        if lower.ends_with(".info") || lower.ends_with(".uaem") || name == BACKUP_DIR {
            continue;
        }

        let entry_path = entry.path();
        if file_type.is_dir() {
            subdirs.push(entry_path);
        }

        let icon_path = dir.join(format!("{name}.info"));
        if !icon_path.is_file() {
            // No icon for this entry at all — nothing this function can lay
            // out.
            continue;
        }
        // A real read failure here (permissions, a vanished file) is not
        // "one bad icon" — it is refused by propagating the `Io` error and
        // aborting the whole plan, per this function's own doc.
        let bytes = std::fs::read(&icon_path)?;

        let rendered = match amigaicon::render::rendered_size(&bytes) {
            Ok(r) => r,
            Err(CoreError::Malformed { .. }) => {
                plan.icons_skipped.push(icon_path);
                continue;
            }
            Err(err) => return Err(err),
        };
        let position = match amigaicon::position(&bytes) {
            Ok(p) => p,
            Err(CoreError::Malformed { .. }) => {
                plan.icons_skipped.push(icon_path);
                continue;
            }
            Err(err) => return Err(err),
        };
        let kind = match amigaicon::icon_type(&bytes) {
            Ok(k) => k,
            Err(CoreError::Malformed { .. }) => {
                plan.icons_skipped.push(icon_path);
                continue;
            }
            Err(err) => return Err(err),
        };

        let cell = icongrid::Cell {
            label: name,
            width: rendered.width,
            height: rendered.height,
            is_container: matches!(
                kind,
                amigaicon::IconType::Drawer | amigaicon::IconType::Disk
            ),
            framed: rendered.framed,
        };

        entries.push(IconEntry {
            icon_path,
            original_bytes: bytes,
            already_placed: position.is_some(),
            cell,
        });
    }

    if !entries.is_empty() && entries.iter().any(|e| !e.already_placed) {
        let cells: Vec<icongrid::Cell> = entries.iter().map(|e| e.cell.clone()).collect();
        let result = icongrid::arrange(&cells, inner_width);
        // `icongrid::arrange` returns exactly one placement per input cell
        // (its own module doc: "a caller can zip a placement straight back
        // to the icon file it describes") — never fewer — so the outer
        // `.any(|e| !e.already_placed)` guard above already guarantees at
        // least one placement below belongs to an unplaced entry. Tracking
        // "did we actually place one" here would be validation that cannot
        // fire, so this directory is counted as arranged unconditionally
        // once that guard has passed.
        plan.drawers_arranged += 1;
        for placement in &result.placements {
            let entry = &entries[placement.index];
            if entry.already_placed {
                // Occupies a cell in the computed grid (its size and label
                // shaped the layout above), but its own real position is
                // the release's and is never overwritten.
                continue;
            }
            let new_bytes =
                amigaicon::set_position(&entry.original_bytes, Some((placement.x, placement.y)))?;
            plan.writes.push((entry.icon_path.clone(), new_bytes));
            plan.icons_placed += 1;
        }
    }

    for sub in subdirs {
        plan_icons_in_dir(&sub, tree_root, plan)?;
    }
    Ok(())
}

/// Apply any combination of a wallpaper assignment, a screen depth and the
/// shell defaults to a distribution tree. The thin, synchronous form — see
/// [`apply_appearance_with`] for the one that reports progress and honours
/// cancellation (ART-248), the same `scan_titles`/`scan_titles_with` split
/// `core::gameindex::scan` already uses.
pub fn apply_appearance(tree: &Path, req: &AppearanceRequest) -> CoreResult<AppearanceOutcome> {
    apply_appearance_with(tree, req, &NoProgress)
}

/// Apply any combination of a wallpaper assignment, a screen depth and the
/// shell defaults to a distribution tree.
///
/// Every fallible step — resolving a path, decoding a picture, checking a
/// backdrop name is free — happens while building a plan for each requested
/// part, **before** this function writes anything at all. Only once every
/// requested part has a validated plan does it commit them, through
/// [`guarded_write`] — `BackupPolicy::CONFIG` for a prefs file or a placed
/// backdrop, `BackupPolicy::NONE` for an icon arrangement write (C6, final
/// whole-branch review: hundreds of small, easily-reproduced icon writes
/// fanning a `.art-backup` drawer into essentially every drawer the tree has
/// is a different cost/value question than one hand-tuned prefs file, and
/// answered the same way `BackupPolicy::LARGE_IMAGE` already answers it for
/// a multi-gigabyte image). A refusal at any point therefore leaves the
/// whole tree — every prefs file, every backdrop, every icon — exactly as
/// it was; only the *kept generations* differ by which kind of file it is.
///
/// **ART-248: cancellation.** Planning is reported as one indefinite phase
/// (its size is not known until it finishes, so `total` stays `None` —
/// CLAUDE.md: never a fixed-width bar for an unknown total). Once every plan
/// exists, the number of files the commit phase will write **is** known, so
/// `sink.report` carries a real total from here on and `sink.is_cancelled()`
/// is checked **between** whole committed files, never mid-write — every
/// write already goes through [`guarded_write`], which is atomic on its own.
/// A commit-phase cancellation is not different in kind from a commit-phase
/// I/O failure, which this function already could leave partial (see
/// `committed` below and [`partial_commit_error`]): a plain
/// [`CoreError::Cancelled`] is returned when nothing has landed yet, and
/// [`CoreError::CancelledPartway`] naming the real count once at least one
/// file has — the same split `core::osinstall::apply::apply` already uses
/// for the identical reason.
pub fn apply_appearance_with(
    tree: &Path,
    req: &AppearanceRequest,
    sink: &dyn ProgressSink,
) -> CoreResult<AppearanceOutcome> {
    // ---- Plan: everything that can fail happens here. ----
    sink.report(0, None, "Working out what needs to change…");
    let wallpaper_plan = match &req.wallpaper {
        Some((which, source, placement)) => Some(plan_wallpaper(tree, *which, source, *placement)?),
        None => None,
    };
    let screen_plan = match req.screen_depth {
        Some(depth) => Some(plan_screen_mode(tree, depth)?),
        None => None,
    };
    let shell_plans = if req.shell_defaults {
        plan_shell_defaults(tree)?
    } else {
        Vec::new()
    };
    let icon_plan = if req.arrange_icons {
        Some(plan_icon_arrangement(tree)?)
    } else {
        None
    };

    // ---- Commit: nothing above touched disk. ----
    //
    // A host filesystem has no journal (`core/hostfs.rs` answers the same
    // question the same way for a removal): if a later write in this call
    // fails after an earlier one already landed, that is not "the operation
    // failed" — one or more files really did change. `committed` tracks
    // every write that has already succeeded in this call so a failure can
    // name them (CLAUDE.md: "never claim what you did not do").
    let mut committed: Vec<(PathBuf, Option<PathBuf>)> = Vec::new();
    let mut picture_placed = None;
    let mut amiga_path = None;

    // Known now — every plan above either exists or does not, and an icon
    // plan already knows exactly how many files it will touch.
    let total: u64 = wallpaper_plan
        .as_ref()
        .map_or(0, |p| 1 + p.picture.is_some() as u64)
        + screen_plan.as_ref().map_or(0, |_| 1)
        + shell_plans.len() as u64
        + icon_plan.as_ref().map_or(0, |p| p.writes.len() as u64);
    let mut done: u64 = 0;
    sink.report(0, Some(total), "Applying the appearance changes…");

    if let Some(plan) = wallpaper_plan {
        if let Some((picture_path, bytes)) = plan.picture {
            check_cancelled(sink, &committed)?;
            if let Some(parent) = picture_path.parent() {
                commit_mkdir(parent, &committed)?;
            }
            let message = picture_path.display().to_string();
            commit_write(
                picture_path.clone(),
                &bytes,
                BackupPolicy::CONFIG,
                &mut committed,
            )?;
            done += 1;
            sink.report(done, Some(total), &message);
            picture_placed = Some(picture_path);
        }

        check_cancelled(sink, &committed)?;
        let message = plan.prefs_path.display().to_string();
        commit_write(
            plan.prefs_path,
            &plan.prefs_bytes,
            BackupPolicy::CONFIG,
            &mut committed,
        )?;
        done += 1;
        sink.report(done, Some(total), &message);
        amiga_path = Some(plan.amiga_path);
    }

    if let Some(plan) = screen_plan {
        check_cancelled(sink, &committed)?;
        let message = plan.path.display().to_string();
        commit_write(plan.path, &plan.bytes, BackupPolicy::CONFIG, &mut committed)?;
        done += 1;
        sink.report(done, Some(total), &message);
    }

    for plan in shell_plans {
        check_cancelled(sink, &committed)?;
        if let Some(parent) = plan.path.parent() {
            commit_mkdir(parent, &committed)?;
        }
        let message = plan.path.display().to_string();
        commit_write(plan.path, &plan.bytes, BackupPolicy::CONFIG, &mut committed)?;
        done += 1;
        sink.report(done, Some(total), &message);
    }

    let mut icons_placed = 0usize;
    let mut drawers_arranged = 0usize;
    let mut icons_skipped = Vec::new();
    if let Some(plan) = icon_plan {
        icons_placed = plan.icons_placed;
        drawers_arranged = plan.drawers_arranged;
        icons_skipped = plan.icons_skipped;
        for (path, bytes) in plan.writes {
            check_cancelled(sink, &committed)?;
            // C6 (final whole-branch review): every icon write used to go
            // through the same `BackupPolicy::CONFIG` as a hand-tuned prefs
            // file. A prefs write is one file, rarely touched; arranging
            // icons across a 3.9-scale tree writes hundreds — 361 of the
            // owner's own 798 real icons are unplaced — so that policy
            // fanned a `.art-backup` drawer into essentially every drawer
            // the tree has, not the two or three round 1 shipped. The change
            // this writes is small (eight bytes, a position moving off the
            // `NO_POSITION` sentinel) and easily reproduced by re-running
            // arrangement, unlike a hand-tuned `WBPattern.prefs` a user
            // edited themselves — the same cost/value question
            // `BackupPolicy::LARGE_IMAGE` already answers "no" to for a
            // multi-gigabyte image, opted out of the same way here.
            let message = path.display().to_string();
            commit_write(path, &bytes, BackupPolicy::NONE, &mut committed)?;
            done += 1;
            sink.report(done, Some(total), &message);
        }
    }

    let mut written = Vec::with_capacity(committed.len());
    let mut backups = Vec::new();
    for (path, backup) in committed {
        written.push(path);
        if let Some(b) = backup {
            backups.push(b);
        }
    }

    Ok(AppearanceOutcome {
        written,
        backups,
        picture_placed,
        amiga_path,
        icons_placed,
        drawers_arranged,
        icons_skipped,
    })
}

/// [`ProgressSink::is_cancelled`], turned into the right flavour of
/// [`CoreError`] — a plain [`CoreError::Cancelled`] when nothing has
/// committed yet, [`CoreError::CancelledPartway`] naming the real count once
/// at least one file has. Checked **between** whole committed files only,
/// never mid-write — see [`apply_appearance_with`]'s own doc comment.
fn check_cancelled(
    sink: &dyn ProgressSink,
    committed: &[(PathBuf, Option<PathBuf>)],
) -> CoreResult<()> {
    if !sink.is_cancelled() {
        return Ok(());
    }
    Err(if committed.is_empty() {
        CoreError::Cancelled
    } else {
        CoreError::CancelledPartway {
            files: committed.len() as u64,
        }
    })
}

/// Write one file through [`guarded_write`] and record it in `committed`.
/// See [`apply_appearance`]'s own comment on `committed` for why a failure
/// here is wrapped with what already succeeded rather than reported bare.
///
/// `policy` is the caller's choice, not a fixed `BackupPolicy::CONFIG` —
/// see C6 (final whole-branch review, `core::appearance::apply_appearance`'s
/// own comment on the icon-write call site) for why a icon write and a
/// prefs write need different answers to "is a generation of this worth
/// keeping".
fn commit_write(
    path: PathBuf,
    bytes: &[u8],
    policy: BackupPolicy,
    committed: &mut Vec<(PathBuf, Option<PathBuf>)>,
) -> CoreResult<()> {
    match guarded_write(&path, bytes, policy) {
        Ok(backup) => {
            committed.push((path, backup));
            Ok(())
        }
        Err(err) => Err(partial_commit_error(err, committed)),
    }
}

/// Create a directory during the commit phase, wrapping a failure the same
/// way [`commit_write`] does — creating a drawer to hold a new backdrop is
/// as much a commit-phase step as writing the file into it.
fn commit_mkdir(parent: &Path, committed: &[(PathBuf, Option<PathBuf>)]) -> CoreResult<()> {
    std::fs::create_dir_all(parent)
        .map_err(|err| partial_commit_error(CoreError::Io(err), committed))
}

/// Wrap a commit-phase failure with what has already been written in this
/// same call to [`apply_appearance`], so a caller told "this failed" is also
/// told what did not. A failure before anything in this call has landed yet
/// is returned unchanged — there is nothing yet to report.
///
/// **Capped, not exhaustive** (C5, final whole-branch review): a wallpaper or
/// shell-defaults commit touches a handful of files, but arranging icons
/// across a 3.9-scale tree can commit hundreds before one fails — 361 of the
/// owner's own 798 real icons are unplaced, so a failure partway through a
/// full arrangement pass could join that many path-plus-backup pairs into one
/// sentence. `crate::core::osinstall::apply::some_of` already solves exactly
/// this for a package refusal naming up to 211 real files; reused here rather
/// than inventing a second capping style with its own threshold.
fn partial_commit_error(err: CoreError, committed: &[(PathBuf, Option<PathBuf>)]) -> CoreError {
    if committed.is_empty() {
        return err;
    }
    let already: Vec<String> = committed
        .iter()
        .map(|(path, backup)| match backup {
            Some(b) => format!(
                "'{}' was already changed (backup at '{}')",
                path.display(),
                b.display()
            ),
            None => format!("'{}' was already written", path.display()),
        })
        .collect();
    CoreError::InvalidInput(format!(
        "{err}, but {}",
        crate::core::osinstall::apply::some_of(&already)
    ))
}

/// List the backdrops a distribution tree actually holds — the file names
/// present under [`BACKDROPS_DRAWER`], sorted. A tree with no such drawer at
/// all (nothing has ever placed a backdrop) is an empty list, not an error:
/// listing what is there is not a claim that something ought to be.
pub fn backdrops_in_tree(tree: &Path) -> CoreResult<Vec<String>> {
    let drawer_rel = BACKDROPS_DRAWER.join("/");
    let Some(drawer) = crate::core::osinstall::resolve_ci_optional(tree, &drawer_rel)? else {
        return Ok(Vec::new());
    };
    let mut names: Vec<String> = std::fs::read_dir(&drawer)?
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.file_type().map(|t| t.is_file()).unwrap_or(false))
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    Ok(names)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::amigaprefs::iff::tests_support::synthetic_prefs;
    use crate::core::picture::tests_support::synthetic_png;
    use crate::core::ScratchDir;

    fn backdrop(
        which: wbpattern::Which,
        reserved: [u8; 16],
        revision: i8,
        other_flags: u16,
        content: wbpattern::Content,
    ) -> wbpattern::Backdrop {
        wbpattern::Backdrop {
            reserved,
            which,
            placement: wbpattern::Placement::Scale,
            precision: wbpattern::Precision::Image,
            dither: wbpattern::Dither::Good,
            no_remap: false,
            other_flags,
            revision,
            content,
        }
    }

    /// The three-`PTRN` `WBPattern.prefs` every real release ships, with the
    /// root entry's `wbp_Revision`/unknown flag bits given explicitly. The
    /// drawer and screen entries carry a nonzero `wbp_Revision` and an
    /// unknown flag bit **on purpose** — `setting_the_root_backdrop_…` below
    /// exists to prove those survive an edit to the *root* entry untouched,
    /// and `the_edited_chunk_keeps_its_revision_and_unknown_flag_bits` gives
    /// the *root* entry the same treatment to prove the chunk being edited
    /// keeps them too. The root entry's `reserved` is [`RESERVED_PATTERN`],
    /// not `[0u8; 16]`, for the same reason: a fixture that started at
    /// all-zero could not tell "carried forward" from "coincidentally still
    /// zero" (finding I2).
    fn wbpattern_bytes_full(
        root_revision: i8,
        root_other_flags: u16,
        root_amiga_path: &str,
    ) -> Vec<u8> {
        let root = backdrop(
            wbpattern::Which::Root,
            RESERVED_PATTERN,
            root_revision,
            root_other_flags,
            wbpattern::Content::Picture(root_amiga_path.to_string()),
        );
        let drawer = backdrop(
            wbpattern::Which::Drawer,
            [0u8; 16],
            7,
            0x0040,
            wbpattern::Content::Picture("Sys:Prefs/Presets/Backdrops/drawer.iff".to_string()),
        );
        let screen = backdrop(
            wbpattern::Which::Screen,
            [0u8; 16],
            3,
            0x0080,
            wbpattern::Content::Pattern {
                depth: 0,
                planes: vec![0u8; 256],
            },
        );
        synthetic_prefs(&[
            (*b"PTRN", wbpattern::write_backdrop(&root).unwrap()),
            (*b"PTRN", wbpattern::write_backdrop(&drawer).unwrap()),
            (*b"PTRN", wbpattern::write_backdrop(&screen).unwrap()),
        ])
    }

    fn wbpattern_bytes(root_amiga_path: &str) -> Vec<u8> {
        wbpattern_bytes_full(0, 0, root_amiga_path)
    }

    /// `reserved` deliberately non-zero (unlike a bare `[0u8; 16]`) so a test
    /// asserting the reserved bytes survive an edit actually distinguishes
    /// "carried forward" from "coincidentally still zero" — the same reason
    /// `screenmode::tests::the_reserved_bytes_are_carried_not_zeroed` does not
    /// use an all-zero fixture either.
    const RESERVED_PATTERN: [u8; 16] = [0x5A, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0xA5];

    fn screenmode_bytes(depth: u16) -> Vec<u8> {
        let mode = screenmode::ScreenMode {
            reserved: RESERVED_PATTERN,
            display_id: 0x0002_9000,
            width: screenmode::USE_MODE_DEFAULT,
            height: screenmode::USE_MODE_DEFAULT,
            depth,
            control: 1,
        };
        synthetic_prefs(&[(*b"SCRM", screenmode::write_screen_mode(&mode).unwrap())])
    }

    fn wbpattern_path(tree: &Path) -> PathBuf {
        tree.join("Prefs")
            .join("Env-Archive")
            .join("Sys")
            .join("WBPattern.prefs")
    }

    fn screenmode_path(tree: &Path) -> PathBuf {
        tree.join("Prefs")
            .join("Env-Archive")
            .join("Sys")
            .join("ScreenMode.prefs")
    }

    /// A synthetic distribution tree: `Prefs/Env-Archive/Sys/WBPattern.prefs`
    /// (three `PTRN`s) and `Prefs/Env-Archive/Sys/ScreenMode.prefs` (one
    /// `SCRM`), matching what a real OS-install component set actually
    /// writes.
    fn build_tree(tag: &str) -> (ScratchDir, PathBuf) {
        let scratch = ScratchDir::new("art-appearance", tag);
        let tree = scratch.path().to_path_buf();
        let sys_dir = tree.join("Prefs").join("Env-Archive").join("Sys");
        std::fs::create_dir_all(&sys_dir).unwrap();
        std::fs::write(
            sys_dir.join("WBPattern.prefs"),
            wbpattern_bytes("Sys:Prefs/Presets/Backdrops/default_pal.iff"),
        )
        .unwrap();
        std::fs::write(sys_dir.join("ScreenMode.prefs"), screenmode_bytes(4)).unwrap();
        (scratch, tree)
    }

    fn write_picture(tree: &Path, name: &str) -> PathBuf {
        let path = tree.join(name);
        std::fs::write(&path, synthetic_png(4, 4, [200, 50, 10])).unwrap();
        path
    }

    #[test]
    fn setting_the_root_backdrop_leaves_the_other_two_chunks_byte_for_byte() {
        let (_scratch, tree) = build_tree("root-untouched");
        let prefs_path = wbpattern_path(&tree);
        let before = std::fs::read(&prefs_path).unwrap();
        let before_parsed = iff::parse(&before).unwrap();

        let picture_path = write_picture(&tree, "wallpaper.png");
        let req = AppearanceRequest {
            wallpaper: Some((
                wbpattern::Which::Root,
                WallpaperSource::HostPicture {
                    path: picture_path,
                    colours: 8,
                },
                wbpattern::Placement::Center,
            )),
            screen_depth: None,
            shell_defaults: false,
            arrange_icons: false,
        };
        apply_appearance(&tree, &req).unwrap();

        let after = std::fs::read(&prefs_path).unwrap();
        let after_parsed = iff::parse(&after).unwrap();

        assert_eq!(
            after_parsed.body(0).unwrap(),
            before_parsed.body(0).unwrap(),
            "PRHD must survive verbatim"
        );
        assert_ne!(
            after_parsed.body(1).unwrap(),
            before_parsed.body(1).unwrap(),
            "the root PTRN was the one asked to change"
        );
        assert_eq!(
            after_parsed.body(2).unwrap(),
            before_parsed.body(2).unwrap(),
            "the drawer PTRN — carrying a nonzero revision and an unknown flag bit — must \
             survive verbatim"
        );
        assert_eq!(
            after_parsed.body(3).unwrap(),
            before_parsed.body(3).unwrap(),
            "the screen PTRN must survive verbatim"
        );
    }

    #[test]
    fn a_host_picture_is_encoded_to_ilbm_and_placed_in_the_backdrops_drawer() {
        let (_scratch, tree) = build_tree("host-picture");
        let picture_path = write_picture(&tree, "mypic.png");

        let req = AppearanceRequest {
            wallpaper: Some((
                wbpattern::Which::Root,
                WallpaperSource::HostPicture {
                    path: picture_path,
                    colours: 8,
                },
                wbpattern::Placement::Tile,
            )),
            screen_depth: None,
            shell_defaults: false,
            arrange_icons: false,
        };
        let outcome = apply_appearance(&tree, &req).unwrap();

        let placed = outcome
            .picture_placed
            .clone()
            .expect("a host picture must be placed");
        let bytes = std::fs::read(&placed).unwrap();
        assert_eq!(&bytes[0..4], b"FORM");
        assert_eq!(&bytes[8..12], b"ILBM");

        let expected_amiga_path = "Sys:Prefs/Presets/Backdrops/mypic.iff";
        assert_eq!(outcome.amiga_path.as_deref(), Some(expected_amiga_path));

        let prefs_bytes = std::fs::read(wbpattern_path(&tree)).unwrap();
        let prefs = iff::parse(&prefs_bytes).unwrap();
        let root = wbpattern::read_backdrop(prefs.body(1).unwrap()).unwrap();
        assert_eq!(
            root.content,
            wbpattern::Content::Picture(expected_amiga_path.to_string())
        );
    }

    #[test]
    fn the_previous_prefs_file_is_backed_up_and_the_path_is_returned() {
        let (_scratch, tree) = build_tree("backup");
        let prefs_path = wbpattern_path(&tree);
        let original = std::fs::read(&prefs_path).unwrap();
        let picture_path = write_picture(&tree, "pic.png");

        let req = AppearanceRequest {
            wallpaper: Some((
                wbpattern::Which::Root,
                WallpaperSource::HostPicture {
                    path: picture_path,
                    colours: 4,
                },
                wbpattern::Placement::Scale,
            )),
            screen_depth: None,
            shell_defaults: false,
            arrange_icons: false,
        };
        let outcome = apply_appearance(&tree, &req).unwrap();

        assert!(
            !outcome.backups.is_empty(),
            "the previous WBPattern.prefs must be backed up"
        );
        let backup_path = outcome
            .backups
            .iter()
            .find(|p| p.to_string_lossy().contains("WBPattern.prefs"))
            .expect("a WBPattern.prefs backup must be among them");
        assert_eq!(std::fs::read(backup_path).unwrap(), original);
    }

    #[test]
    fn refusing_to_overwrite_a_backdrop_that_is_already_there() {
        let (_scratch, tree) = build_tree("no-overwrite");
        let picture_path = write_picture(&tree, "same.png");

        let req = AppearanceRequest {
            wallpaper: Some((
                wbpattern::Which::Root,
                WallpaperSource::HostPicture {
                    path: picture_path,
                    colours: 4,
                },
                wbpattern::Placement::Scale,
            )),
            screen_depth: None,
            shell_defaults: false,
            arrange_icons: false,
        };
        apply_appearance(&tree, &req).unwrap();

        let prefs_before_second = std::fs::read(wbpattern_path(&tree)).unwrap();
        let drawer_target = tree
            .join("Prefs")
            .join("Presets")
            .join("Backdrops")
            .join("same.iff");
        let placed_before_second = std::fs::read(&drawer_target).unwrap();

        let err = apply_appearance(&tree, &req).unwrap_err();
        let text = format!("{err}");
        assert!(text.contains("same.iff"), "must name the file: {text}");
        assert!(text.contains("already exists"), "must say why: {text}");

        assert_eq!(
            std::fs::read(&drawer_target).unwrap(),
            placed_before_second,
            "the existing backdrop must be unchanged"
        );
        assert_eq!(
            std::fs::read(wbpattern_path(&tree)).unwrap(),
            prefs_before_second,
            "a refused apply must not touch the prefs file either"
        );
    }

    #[test]
    fn a_failed_apply_leaves_every_prefs_file_unchanged() {
        let (_scratch, tree) = build_tree("failed-apply");
        let not_a_picture = tree.join("not-a-picture.png");
        std::fs::write(&not_a_picture, b"this is not a png or jpeg").unwrap();

        let prefs_before = std::fs::read(wbpattern_path(&tree)).unwrap();

        let req = AppearanceRequest {
            wallpaper: Some((
                wbpattern::Which::Root,
                WallpaperSource::HostPicture {
                    path: not_a_picture,
                    colours: 8,
                },
                wbpattern::Placement::Scale,
            )),
            screen_depth: None,
            shell_defaults: false,
            arrange_icons: false,
        };
        let err = apply_appearance(&tree, &req).unwrap_err();
        let text = format!("{err}");
        assert!(
            text.contains("PNG") && text.contains("JPEG"),
            "must be the picture-decode refusal: {text}"
        );

        assert_eq!(
            std::fs::read(wbpattern_path(&tree)).unwrap(),
            prefs_before,
            "a failed apply must not touch WBPattern.prefs"
        );
    }

    #[test]
    fn backdrops_in_tree_lists_what_the_release_actually_shipped() {
        let (_scratch, tree) = build_tree("listing");
        let drawer = tree.join("Prefs").join("Presets").join("Backdrops");
        std::fs::create_dir_all(&drawer).unwrap();
        std::fs::write(drawer.join("default_pal.iff"), b"FORM....ILBM").unwrap();
        std::fs::write(drawer.join("Christmas.iff"), b"FORM....ILBM").unwrap();

        let names = backdrops_in_tree(&tree).unwrap();
        assert_eq!(
            names,
            vec!["Christmas.iff".to_string(), "default_pal.iff".to_string()]
        );
    }

    #[test]
    fn a_tree_with_no_backdrops_drawer_lists_nothing_rather_than_failing() {
        let scratch = ScratchDir::new("art-appearance", "no-drawer");
        let names = backdrops_in_tree(scratch.path()).unwrap();
        assert!(names.is_empty());
    }

    #[test]
    fn a_tree_with_no_wbpattern_prefs_is_refused_by_name() {
        let scratch = ScratchDir::new("art-appearance", "no-wbpattern");
        let tree = scratch.path().to_path_buf();
        std::fs::create_dir_all(tree.join("Prefs")).unwrap();

        let req = AppearanceRequest {
            wallpaper: Some((
                wbpattern::Which::Root,
                WallpaperSource::AlreadyInTree {
                    amiga_path: "Sys:Prefs/Presets/Backdrops/default_pal.iff".to_string(),
                },
                wbpattern::Placement::Scale,
            )),
            screen_depth: None,
            shell_defaults: false,
            arrange_icons: false,
        };
        let err = apply_appearance(&tree, &req).unwrap_err();
        let text = format!("{err}");
        assert!(
            text.contains("Prefs/Env-Archive/Sys/WBPattern.prefs"),
            "the refusal must name which file is missing: {text}"
        );
    }

    #[test]
    fn an_already_in_tree_amiga_path_that_does_not_resolve_is_refused() {
        let (_scratch, tree) = build_tree("already-in-tree-missing");
        let prefs_before = std::fs::read(wbpattern_path(&tree)).unwrap();

        let req = AppearanceRequest {
            wallpaper: Some((
                wbpattern::Which::Root,
                WallpaperSource::AlreadyInTree {
                    amiga_path: "Sys:Prefs/Presets/Backdrops/does_not_exist.iff".to_string(),
                },
                wbpattern::Placement::Scale,
            )),
            screen_depth: None,
            shell_defaults: false,
            arrange_icons: false,
        };
        let err = apply_appearance(&tree, &req).unwrap_err();
        assert!(format!("{err}").contains("does_not_exist.iff"));
        assert_eq!(
            std::fs::read(wbpattern_path(&tree)).unwrap(),
            prefs_before,
            "a refused apply must not touch the prefs file"
        );
    }

    #[test]
    fn an_already_in_tree_backdrop_is_pointed_to_directly_with_no_new_file() {
        let (_scratch, tree) = build_tree("already-in-tree-ok");
        let drawer = tree.join("Prefs").join("Presets").join("Backdrops");
        std::fs::create_dir_all(&drawer).unwrap();
        std::fs::write(drawer.join("Christmas.iff"), b"FORM....ILBM").unwrap();

        let req = AppearanceRequest {
            wallpaper: Some((
                wbpattern::Which::Root,
                WallpaperSource::AlreadyInTree {
                    amiga_path: "Sys:Prefs/Presets/Backdrops/Christmas.iff".to_string(),
                },
                wbpattern::Placement::ScaleGood,
            )),
            screen_depth: None,
            shell_defaults: false,
            arrange_icons: false,
        };
        let outcome = apply_appearance(&tree, &req).unwrap();
        assert!(outcome.picture_placed.is_none(), "nothing new was placed");
        assert_eq!(
            outcome.amiga_path.as_deref(),
            Some("Sys:Prefs/Presets/Backdrops/Christmas.iff")
        );
    }

    /// **Whole-branch review finding I3.** `resolve_amiga_path` used to
    /// compare the `Sys:` assign literally while every path *component*
    /// after it is already folded case-insensitively through `resolve_ci` —
    /// so a request naming `SYS:…`, the exact case the round's own pre-flight
    /// scan measured on real material, was refused as "does not start with
    /// 'Sys:'" even though `SYS:` names the very same assign.
    #[test]
    fn an_already_in_tree_backdrop_is_resolved_with_an_uppercase_sys_assign() {
        let (_scratch, tree) = build_tree("already-in-tree-uppercase-sys");
        let drawer = tree.join("Prefs").join("Presets").join("Backdrops");
        std::fs::create_dir_all(&drawer).unwrap();
        std::fs::write(drawer.join("Christmas.iff"), b"FORM....ILBM").unwrap();

        let req = AppearanceRequest {
            wallpaper: Some((
                wbpattern::Which::Root,
                WallpaperSource::AlreadyInTree {
                    amiga_path: "SYS:Prefs/Presets/Backdrops/Christmas.iff".to_string(),
                },
                wbpattern::Placement::ScaleGood,
            )),
            screen_depth: None,
            shell_defaults: false,
            arrange_icons: false,
        };
        let outcome = apply_appearance(&tree, &req).unwrap_or_else(|err| {
            panic!("an upper-case SYS: assign must resolve like Sys: does: {err}")
        });
        assert!(outcome.picture_placed.is_none(), "nothing new was placed");
    }

    #[test]
    fn setting_screen_depth_rewrites_only_the_scrm_body() {
        let (_scratch, tree) = build_tree("screen-depth");
        let path = screenmode_path(&tree);
        let before = std::fs::read(&path).unwrap();

        let req = AppearanceRequest {
            wallpaper: None,
            screen_depth: Some(8),
            shell_defaults: false,
            arrange_icons: false,
        };
        apply_appearance(&tree, &req).unwrap();

        let after = std::fs::read(&path).unwrap();
        assert_ne!(after, before);
        let prefs = iff::parse(&after).unwrap();
        let index = prefs
            .chunks()
            .iter()
            .position(|c| c.id == *b"SCRM")
            .unwrap();
        let mode = screenmode::read_screen_mode(prefs.body(index).unwrap()).unwrap();
        assert_eq!(mode.depth, 8);
        assert_eq!(
            mode.display_id, 0x0002_9000,
            "everything else in the chunk must survive"
        );
        assert_eq!(
            mode.reserved, RESERVED_PATTERN,
            "the 16 reserved bytes must be carried, not zeroed — a fixture that started at \
             all-zero could not tell 'carried' from 'coincidentally still zero'"
        );
    }

    #[test]
    fn shell_defaults_are_written_into_env_archive() {
        let (_scratch, tree) = build_tree("shell-defaults");

        let req = AppearanceRequest {
            wallpaper: None,
            screen_depth: None,
            shell_defaults: true,
            arrange_icons: false,
        };
        let outcome = apply_appearance(&tree, &req).unwrap();
        assert_eq!(outcome.written.len(), env::SHELL_DEFAULTS.len());

        for (name, value) in env::SHELL_DEFAULTS {
            let path = tree.join("Prefs").join("Env-Archive").join(name);
            let bytes =
                std::fs::read(&path).unwrap_or_else(|e| panic!("{name} was not written: {e}"));
            assert_eq!(env::decode_value(&bytes), value);
        }
    }

    /// **The round's central guard, on the chunk that is actually edited.**
    /// `setting_the_root_backdrop_leaves_the_other_two_chunks_byte_for_byte`
    /// proves the untouched chunks survive, but its root `PTRN` fixture has
    /// `revision = 0, other_flags = 0` — a regression that reset those two
    /// fields to zero on the *edited* chunk would still pass every other
    /// test. This one gives the root entry a nonzero revision and an unknown
    /// flag bit too, and proves both survive the edit, while the field the
    /// request actually named (`placement`) really did change. The root
    /// fixture's `reserved` is [`RESERVED_PATTERN`] too, so this is also the
    /// guard for finding I2: `write_backdrop` used to emit `[0u8; 16]`
    /// unconditionally, and no fixture built by ART's own `write_backdrop`
    /// could ever have caught that — every one of them was zero-reserved by
    /// construction.
    #[test]
    fn the_edited_chunk_keeps_its_revision_and_unknown_flag_bits() {
        let (_scratch, tree) = build_tree("edited-chunk-keeps-fields");
        std::fs::write(
            wbpattern_path(&tree),
            wbpattern_bytes_full(7, 0x0040, "Sys:Prefs/Presets/Backdrops/default_pal.iff"),
        )
        .unwrap();
        let picture_path = write_picture(&tree, "wallpaper.png");

        let req = AppearanceRequest {
            wallpaper: Some((
                wbpattern::Which::Root,
                WallpaperSource::HostPicture {
                    path: picture_path,
                    colours: 8,
                },
                wbpattern::Placement::Center,
            )),
            screen_depth: None,
            shell_defaults: false,
            arrange_icons: false,
        };
        apply_appearance(&tree, &req).unwrap();

        let prefs_bytes = std::fs::read(wbpattern_path(&tree)).unwrap();
        let prefs = iff::parse(&prefs_bytes).unwrap();
        let after = wbpattern::read_backdrop(prefs.body(1).unwrap()).unwrap();

        assert_eq!(after.revision, 7, "the edited chunk keeps its revision");
        assert_eq!(
            after.other_flags, 0x0040,
            "the edited chunk keeps unknown flag bits"
        );
        assert_eq!(
            after.reserved, RESERVED_PATTERN,
            "the edited chunk keeps its 16 reserved bytes, not zeroed"
        );
        assert_eq!(
            after.placement,
            wbpattern::Placement::Center,
            "the field the request DID name must actually have changed"
        );
    }

    /// **The coordinator's ruling.** A host filesystem has no journal: a
    /// call that writes the wallpaper prefs successfully and then fails
    /// writing the screen mode must not be reported as if nothing happened.
    /// Provoked cheaply, as suggested: `ScreenMode.prefs` is made read-only
    /// before the call, so its own write fails in the commit phase, after
    /// the wallpaper's own writes (picture + `WBPattern.prefs`) already
    /// landed.
    #[test]
    fn a_write_that_fails_after_an_earlier_one_succeeded_names_what_was_already_written() {
        let (_scratch, tree) = build_tree("partial-commit");
        let screenmode_file = screenmode_path(&tree);
        let mut perms = std::fs::metadata(&screenmode_file).unwrap().permissions();
        perms.set_readonly(true);
        std::fs::set_permissions(&screenmode_file, perms).unwrap();

        let picture_path = write_picture(&tree, "wallpaper.png");
        let req = AppearanceRequest {
            wallpaper: Some((
                wbpattern::Which::Root,
                WallpaperSource::HostPicture {
                    path: picture_path,
                    colours: 8,
                },
                wbpattern::Placement::Center,
            )),
            screen_depth: Some(8),
            shell_defaults: false,
            arrange_icons: false,
        };

        let result = apply_appearance(&tree, &req);

        // Clear the read-only bit before any assertion can panic, or the
        // ScratchDir's own cleanup on Drop would fail to remove the tree.
        // `set_readonly(false)` is exactly the right call on this Windows-only
        // test suite (clippy's warning is about the Unix world-writable
        // consequence, which does not apply here).
        let mut perms = std::fs::metadata(&screenmode_file).unwrap().permissions();
        #[allow(clippy::permissions_set_readonly_false)]
        perms.set_readonly(false);
        std::fs::set_permissions(&screenmode_file, perms).unwrap();

        let err = result.expect_err(
            "writing to a read-only ScreenMode.prefs must fail — if it did not, this test \
             provoked nothing and proves nothing",
        );
        let text = format!("{err}");
        assert!(
            text.contains("WBPattern.prefs"),
            "must name the file already changed before the failure: {text}"
        );
        assert!(
            text.contains("backup at"),
            "must say where its backup went: {text}"
        );

        // And the wallpaper's own writes really did land — the error is
        // honest about what already happened, not merely worded as if it
        // did.
        assert_eq!(
            wbpattern::read_backdrop(
                iff::parse(&std::fs::read(wbpattern_path(&tree)).unwrap())
                    .unwrap()
                    .body(1)
                    .unwrap()
            )
            .unwrap()
            .placement,
            wbpattern::Placement::Center
        );
    }

    // -----------------------------------------------------------------
    // Task 6: the icon-arrangement applier — the first real caller of
    // `core::amigaicon`'s position/type/rendered-size readers and writer
    // and `core::icongrid::arrange`.
    // -----------------------------------------------------------------

    /// A minimal valid icon with the given rendered size, placed at
    /// `position` (or unplaced when `None`) — composed from `amigaicon`'s
    /// own test fixtures and its own public [`amigaicon::set_position`],
    /// rather than a second hand-rolled copy of the `DiskObject` byte layout
    /// (see that module's own doc comment on `tests_support` for why).
    fn icon_bytes(width: u16, height: u16, position: Option<(i32, i32)>) -> Vec<u8> {
        let bytes = crate::core::amigaicon::tests_support::synthetic_icon_sized(width, height);
        amigaicon::set_position(&bytes, position).unwrap()
    }

    /// Write entry `name` — a plain empty stand-in file; this module never
    /// reads an entry's own content, only its icon's — and its `.info` icon
    /// `icon` into `dir`.
    fn write_icon_entry(dir: &Path, name: &str, icon: &[u8]) {
        std::fs::write(dir.join(name), b"").unwrap();
        std::fs::write(dir.join(format!("{name}.info")), icon).unwrap();
    }

    /// **The round's central guard.** A drawer holding one icon the release
    /// already placed and one it did not: after arranging, the first is
    /// still byte-identical (and still reads back at the exact position it
    /// started at) and the second has been given a position.
    #[test]
    fn an_icon_the_release_positioned_is_left_exactly_where_it_was() {
        let scratch = ScratchDir::new("art-appearance-icons", "already-placed");
        let tree = scratch.path().to_path_buf();
        write_icon_entry(&tree, "Placed", &icon_bytes(20, 20, Some((13, 4))));
        write_icon_entry(&tree, "Unplaced", &icon_bytes(20, 20, None));
        let placed_before = std::fs::read(tree.join("Placed.info")).unwrap();

        let req = AppearanceRequest {
            wallpaper: None,
            screen_depth: None,
            shell_defaults: false,
            arrange_icons: true,
        };
        let outcome = apply_appearance(&tree, &req).unwrap();

        let placed_after = std::fs::read(tree.join("Placed.info")).unwrap();
        assert_eq!(
            placed_after, placed_before,
            "an icon the release already positioned must be byte-identical after arranging"
        );
        assert_eq!(
            amigaicon::position(&placed_after).unwrap(),
            Some((13, 4)),
            "and must still read back at exactly the position it started at"
        );

        let unplaced_after = std::fs::read(tree.join("Unplaced.info")).unwrap();
        assert!(
            amigaicon::position(&unplaced_after).unwrap().is_some(),
            "the unplaced icon must have been given a position"
        );
        assert_eq!(outcome.icons_placed, 1);
    }

    /// Everything outside the eight position bytes (58..66, matching
    /// `amigaicon`'s own `setting_a_position_changes_eight_bytes_and_nothing_else`)
    /// must survive arranging untouched.
    #[test]
    fn an_unplaced_icon_gets_a_position_and_nothing_else_changes() {
        let scratch = ScratchDir::new("art-appearance-icons", "only-position-changes");
        let tree = scratch.path().to_path_buf();
        let before = icon_bytes(30, 20, None);
        write_icon_entry(&tree, "Solo", &before);

        let req = AppearanceRequest {
            wallpaper: None,
            screen_depth: None,
            shell_defaults: false,
            arrange_icons: true,
        };
        apply_appearance(&tree, &req).unwrap();

        let after = std::fs::read(tree.join("Solo.info")).unwrap();
        assert_eq!(after.len(), before.len(), "no length change");
        for i in (0..before.len()).filter(|i| !(58..66).contains(i)) {
            assert_eq!(after[i], before[i], "byte {i} changed and should not have");
        }
        assert!(
            amigaicon::position(&after).unwrap().is_some(),
            "the icon must have been given a position"
        );
    }

    /// **C6 (final whole-branch review).** Rewriting an unplaced icon must
    /// leave no `.art-backup` drawer behind — `BackupPolicy::NONE`, not the
    /// `CONFIG` a hand-tuned prefs file gets. Round 1 shipped two or three of
    /// these drawers; without this, arranging icons across a 3.9-scale tree
    /// (361 of the owner's own 798 real icons are unplaced) would fan one
    /// into essentially every drawer the tree has, and `core/preload`'s
    /// `collect_into` copies everything but `.uaem` onto a PiStorm card, so
    /// every one of them would ship. Two icons in two different drawers
    /// (`Solo` at the tree root, `Nested/Other`) both get a position and
    /// neither leaves a backup anywhere.
    #[test]
    fn arranging_icons_leaves_no_art_backup_drawer_anywhere() {
        let scratch = ScratchDir::new("art-appearance-icons", "no-backup-drawer");
        let tree = scratch.path().to_path_buf();
        write_icon_entry(&tree, "Solo", &icon_bytes(30, 20, None));
        std::fs::create_dir_all(tree.join("Nested")).unwrap();
        write_icon_entry(&tree.join("Nested"), "Other", &icon_bytes(30, 20, None));

        let req = AppearanceRequest {
            wallpaper: None,
            screen_depth: None,
            shell_defaults: false,
            arrange_icons: true,
        };
        let outcome = apply_appearance(&tree, &req).unwrap();

        assert_eq!(outcome.icons_placed, 2, "both icons were unplaced");
        assert!(
            outcome.backups.is_empty(),
            "an icon write must take no backup: {:?}",
            outcome.backups
        );
        assert!(
            !tree.join(BACKUP_DIR).exists(),
            "no .art-backup at the tree root"
        );
        assert!(
            !tree.join("Nested").join(BACKUP_DIR).exists(),
            "no .art-backup inside the nested drawer either"
        );
    }

    /// No write, no backup, and the outcome counts zero — a drawer where
    /// every icon is already placed is not rewritten at all.
    #[test]
    fn a_drawer_where_every_icon_is_already_placed_is_not_rewritten_at_all() {
        let scratch = ScratchDir::new("art-appearance-icons", "all-already-placed");
        let tree = scratch.path().to_path_buf();
        write_icon_entry(&tree, "A", &icon_bytes(20, 20, Some((5, 5))));
        write_icon_entry(&tree, "B", &icon_bytes(20, 20, Some((50, 5))));
        let before_a = std::fs::read(tree.join("A.info")).unwrap();
        let before_b = std::fs::read(tree.join("B.info")).unwrap();

        let req = AppearanceRequest {
            wallpaper: None,
            screen_depth: None,
            shell_defaults: false,
            arrange_icons: true,
        };
        let outcome = apply_appearance(&tree, &req).unwrap();

        assert_eq!(std::fs::read(tree.join("A.info")).unwrap(), before_a);
        assert_eq!(std::fs::read(tree.join("B.info")).unwrap(), before_b);
        assert!(outcome.written.is_empty(), "no write at all");
        assert!(outcome.backups.is_empty(), "no backup at all");
        assert_eq!(outcome.icons_placed, 0);
        assert_eq!(outcome.drawers_arranged, 0);
        assert!(
            !tree.join(BACKUP_DIR).exists(),
            "nothing rewritten means no backup drawer either"
        );
    }

    /// The tree's own top level is the `SYS:` window and uses
    /// `icongrid::ROOT_INNER_WIDTH` (420), narrower than
    /// `icongrid::DRAWER_INNER_WIDTH` (520) every ordinary drawer gets.
    /// Six 100x100 icons are enough to make the two budgets choose a
    /// different column count — confirmed independently below with
    /// `icongrid::arrange` itself before ever calling `apply_appearance` —
    /// so an icon's real, on-disk position after arranging the root can be
    /// pinned to exactly the narrower layout.
    #[test]
    fn the_root_uses_the_narrower_budget() {
        let scratch = ScratchDir::new("art-appearance-icons", "root-narrow-budget");
        let tree = scratch.path().to_path_buf();

        let names: Vec<String> = (0..6).map(|i| format!("Icon{i}")).collect();
        for name in &names {
            write_icon_entry(&tree, name, &icon_bytes(100, 100, None));
        }

        let cells: Vec<icongrid::Cell> = names
            .iter()
            .map(|n| icongrid::Cell {
                label: n.clone(),
                width: 100,
                height: 100,
                is_container: false,
                framed: true,
            })
            .collect();
        let at_root = icongrid::arrange(&cells, icongrid::ROOT_INNER_WIDTH);
        let at_drawer_width = icongrid::arrange(&cells, icongrid::DRAWER_INNER_WIDTH);
        assert_ne!(
            at_root.placements, at_drawer_width.placements,
            "the fixture must actually distinguish the two budgets, or this test proves nothing"
        );

        let req = AppearanceRequest {
            wallpaper: None,
            screen_depth: None,
            shell_defaults: false,
            arrange_icons: true,
        };
        apply_appearance(&tree, &req).unwrap();

        for name in &names {
            let bytes = std::fs::read(tree.join(format!("{name}.info"))).unwrap();
            let pos = amigaicon::position(&bytes).unwrap().unwrap();
            let expected = at_root
                .placements
                .iter()
                .find(|p| cells[p.index].label == *name)
                .expect("every cell was placed");
            assert_eq!(
                pos,
                (expected.x, expected.y),
                "{name} must land where ROOT_INNER_WIDTH puts it, not DRAWER_INNER_WIDTH"
            );
        }
    }

    /// One malformed `.info` must not cost the other icons in its drawer
    /// their layout — it is skipped and named in the outcome, never claimed
    /// as done and never allowed to fail the rest of the run.
    #[test]
    fn a_malformed_icon_is_skipped_and_named_rather_than_failing_the_whole_run() {
        let scratch = ScratchDir::new("art-appearance-icons", "malformed-skip");
        let tree = scratch.path().to_path_buf();
        write_icon_entry(&tree, "Good1", &icon_bytes(20, 20, None));
        write_icon_entry(&tree, "Good2", &icon_bytes(20, 20, None));
        std::fs::write(tree.join("Bad"), b"").unwrap();
        let bad_icon_path = tree.join("Bad.info");
        std::fs::write(&bad_icon_path, b"not an amiga icon at all, no magic here").unwrap();

        let req = AppearanceRequest {
            wallpaper: None,
            screen_depth: None,
            shell_defaults: false,
            arrange_icons: true,
        };
        let outcome = apply_appearance(&tree, &req).unwrap();

        assert_eq!(
            outcome.icons_skipped,
            vec![bad_icon_path.clone()],
            "the outcome must name exactly the icon that could not be parsed"
        );
        assert_eq!(
            outcome.icons_placed, 2,
            "the other two icons must still get positions"
        );
        assert!(
            amigaicon::position(&std::fs::read(tree.join("Good1.info")).unwrap())
                .unwrap()
                .is_some()
        );
        assert!(
            amigaicon::position(&std::fs::read(tree.join("Good2.info")).unwrap())
                .unwrap()
                .is_some()
        );
        assert_eq!(
            std::fs::read(&bad_icon_path).unwrap(),
            b"not an amiga icon at all, no magic here",
            "the malformed icon itself must be left exactly as it was"
        );
    }

    /// **The plan-then-commit guard.** A drawer's own icon is fully arranged
    /// (in memory) before this walk descends into its sub-drawer, whose one
    /// icon cannot even be read — opened with no sharing, so the plan hits a
    /// real I/O failure, not a malformed byte layout, and the whole run must
    /// abort. A version that wrote each icon's new position to disk as soon
    /// as `arrange` computed it (rather than deferring every write in the
    /// tree until the whole plan succeeds) would leave the first drawer's
    /// icon already rewritten despite the call failing.
    #[test]
    fn a_failed_arrange_leaves_every_icon_byte_for_byte_unchanged() {
        use std::os::windows::fs::OpenOptionsExt;

        let scratch = ScratchDir::new("art-appearance-icons", "failed-arrange-untouched");
        let tree = scratch.path().to_path_buf();

        let good_dir = tree.join("GoodDrawer");
        std::fs::create_dir_all(&good_dir).unwrap();
        write_icon_entry(&good_dir, "Widget", &icon_bytes(20, 20, None));
        let widget_before = std::fs::read(good_dir.join("Widget.info")).unwrap();

        // Visited only after GoodDrawer's own icons have already been
        // arranged: this walk arranges a directory's own entries before
        // descending into its subdirectories.
        let bad_dir = good_dir.join("BadSubDrawer");
        std::fs::create_dir_all(&bad_dir).unwrap();
        write_icon_entry(&bad_dir, "Blocked", &icon_bytes(20, 20, None));
        let blocked_icon = bad_dir.join("Blocked.info");
        let blocked_before = std::fs::read(&blocked_icon).unwrap();
        let _lock = std::fs::OpenOptions::new()
            .read(true)
            .share_mode(0)
            .open(&blocked_icon)
            .unwrap();

        let req = AppearanceRequest {
            wallpaper: None,
            screen_depth: None,
            shell_defaults: false,
            arrange_icons: true,
        };
        let err = apply_appearance(&tree, &req).unwrap_err();
        drop(_lock);

        assert!(
            matches!(err, CoreError::Io(_)),
            "a real read failure must abort as an I/O error, not be swallowed: {err}"
        );
        assert_eq!(
            std::fs::read(good_dir.join("Widget.info")).unwrap(),
            widget_before,
            "GoodDrawer's own icon was arranged before the failure further down the tree — it \
             must still be untouched on disk, because nothing is committed until the whole \
             tree's plan succeeds"
        );
        assert_eq!(
            std::fs::read(&blocked_icon).unwrap(),
            blocked_before,
            "the icon that could not even be read must be untouched too"
        );
    }

    /// The outcome's counts are exact: icons already placed do not count as
    /// placed, and a drawer with nothing newly placed does not count as
    /// arranged.
    #[test]
    fn the_outcome_counts_what_was_actually_placed() {
        let scratch = ScratchDir::new("art-appearance-icons", "counts");
        let tree = scratch.path().to_path_buf();

        write_icon_entry(&tree, "RootPlaced", &icon_bytes(20, 20, Some((1, 1))));
        write_icon_entry(&tree, "RootUnplaced", &icon_bytes(20, 20, None));

        let drawer_a = tree.join("DrawerA");
        std::fs::create_dir_all(&drawer_a).unwrap();
        write_icon_entry(&drawer_a, "A1", &icon_bytes(20, 20, None));
        write_icon_entry(&drawer_a, "A2", &icon_bytes(20, 20, None));

        let drawer_b = tree.join("DrawerB");
        std::fs::create_dir_all(&drawer_b).unwrap();
        write_icon_entry(&drawer_b, "B1", &icon_bytes(20, 20, Some((0, 0))));

        let req = AppearanceRequest {
            wallpaper: None,
            screen_depth: None,
            shell_defaults: false,
            arrange_icons: true,
        };
        let outcome = apply_appearance(&tree, &req).unwrap();

        assert_eq!(
            outcome.icons_placed, 3,
            "1 at the root (RootUnplaced) + 2 in DrawerA (A1, A2)"
        );
        assert_eq!(
            outcome.drawers_arranged, 2,
            "the root and DrawerA each got a new placement; DrawerB — already fully placed — \
             did not"
        );
        assert!(outcome.icons_skipped.is_empty());
    }

    /// **Task 3 → Task 5 → Task 6, closed.** `Rendered.framed` must reach
    /// `Cell.framed`, not be defaulted — every other fixture in this file
    /// goes through `synthetic_icon_sized`, which never attaches a ColorIcon
    /// `FACE` chunk, so `rendered_size` always falls back to `framed: true`
    /// and a `Cell.framed` hardcoded to `true` would pass every other test
    /// here. This one builds a genuinely frameless icon (Task 3's own
    /// `synthetic_colour_icon`, reused rather than a third hand-rolled
    /// builder — a `FACE` chunk with the frameless bit set, backed by an
    /// `IMAG` chunk since `rendered_size` only trusts `FACE` when one is
    /// present) alongside a genuinely framed one of identical rendered size.
    ///
    /// A framed cell pads its footprint by `2*EMBOSS`, a frameless one by
    /// `EMBOSS` (`icongrid`'s own module doc). "AAAA" (frameless) sorts
    /// before "BBBB" (framed) and lands in the earlier column, so BBBB's x
    /// depends on AAAA's own column width — which depends on AAAA's framed
    /// bit. The expected positions are computed independently with
    /// `icongrid::arrange` itself, once with the real (mixed) framed values
    /// and once with both forced framed (simulating a `Cell.framed`
    /// default-to-true regression), and the two are asserted to differ —
    /// exactly the amount the emboss rule predicts, since that arithmetic
    /// lives in and is separately tested by `icongrid` itself — before the
    /// real, on-disk result is pinned to the correct one.
    #[test]
    fn a_frameless_icon_gets_a_narrower_footprint_than_a_framed_one() {
        use crate::core::amigaicon::render::tests_support::synthetic_colour_icon;

        let scratch = ScratchDir::new("art-appearance-icons", "framed-vs-frameless");
        let tree = scratch.path().to_path_buf();

        let frameless_icon =
            amigaicon::set_position(&synthetic_colour_icon(40, 40, 40, 40, true, true), None)
                .unwrap();
        let framed_icon =
            amigaicon::set_position(&synthetic_colour_icon(40, 40, 40, 40, true, false), None)
                .unwrap();
        // Confirm the fixtures actually landed on the framed bit this test
        // means to exercise, not merely "some size".
        assert!(
            !amigaicon::render::rendered_size(&frameless_icon)
                .unwrap()
                .framed
        );
        assert!(
            amigaicon::render::rendered_size(&framed_icon)
                .unwrap()
                .framed
        );
        write_icon_entry(&tree, "AAAA", &frameless_icon);
        write_icon_entry(&tree, "BBBB", &framed_icon);

        let cells_correct = vec![
            icongrid::Cell {
                label: "AAAA".to_string(),
                width: 40,
                height: 40,
                is_container: false,
                framed: false,
            },
            icongrid::Cell {
                label: "BBBB".to_string(),
                width: 40,
                height: 40,
                is_container: false,
                framed: true,
            },
        ];
        // The regression this test exists to catch: AAAA's own framed bit
        // lost to a hardcoded default.
        let cells_if_defaulted = vec![
            icongrid::Cell {
                framed: true,
                ..cells_correct[0].clone()
            },
            cells_correct[1].clone(),
        ];
        let expected_correct = icongrid::arrange(&cells_correct, icongrid::ROOT_INNER_WIDTH);
        let expected_if_defaulted =
            icongrid::arrange(&cells_if_defaulted, icongrid::ROOT_INNER_WIDTH);
        assert_ne!(
            expected_correct.placements, expected_if_defaulted.placements,
            "the fixture must actually distinguish a copied `framed` bit from a defaulted one, \
             or this test proves nothing"
        );

        let req = AppearanceRequest {
            wallpaper: None,
            screen_depth: None,
            shell_defaults: false,
            arrange_icons: true,
        };
        apply_appearance(&tree, &req).unwrap();

        let find = |label: &str, result: &icongrid::GridResult, cells: &[icongrid::Cell]| {
            result
                .placements
                .iter()
                .find(|p| cells[p.index].label == label)
                .map(|p| (p.x, p.y))
                .expect("every cell was placed")
        };
        let aaaa_pos = amigaicon::position(&std::fs::read(tree.join("AAAA.info")).unwrap())
            .unwrap()
            .unwrap();
        let bbbb_pos = amigaicon::position(&std::fs::read(tree.join("BBBB.info")).unwrap())
            .unwrap()
            .unwrap();
        assert_eq!(
            aaaa_pos,
            find("AAAA", &expected_correct, &cells_correct),
            "the frameless icon must land exactly where a genuinely frameless AAAA does"
        );
        assert_eq!(
            bbbb_pos,
            find("BBBB", &expected_correct, &cells_correct),
            "the framed icon's own x depends on AAAA's real (narrower) column width — it must \
             land exactly where AAAA being genuinely frameless puts it, not where a \
             defaulted-to-framed AAAA would put it"
        );
    }

    // -----------------------------------------------------------------
    // ART-248: `apply_appearance` becomes a job — `apply_appearance_with`
    // reports progress and honours cancellation between whole units of
    // work (one committed file each), never mid-write.
    // -----------------------------------------------------------------

    use crate::core::jobs::{CancelToken, ProgressSink};
    use std::sync::atomic::{AtomicU64, Ordering};

    /// Cancels once `report` has been called more than `after` times — the
    /// same shape `core::gameindex::scan::tests::the_scan_stops_when_asked`
    /// and `core::osinstall::apply`'s own fixtures use.
    struct CancelAfter {
        seen: AtomicU64,
        after: u64,
        token: CancelToken,
    }

    impl ProgressSink for CancelAfter {
        fn report(&self, _done: u64, _total: Option<u64>, _message: &str) {
            if self.seen.fetch_add(1, Ordering::SeqCst) > self.after {
                self.token.cancel();
            }
        }
        fn is_cancelled(&self) -> bool {
            self.token.is_cancelled()
        }
    }

    /// The central guard for ART-248: cancelling partway through arranging
    /// icons leaves the icons already written with a real position, leaves
    /// every icon not yet reached exactly as it was (still unplaced), and
    /// reports `CancelledPartway` with the true count — not a bare
    /// `Cancelled` that could not tell "stopped before anything landed" from
    /// "stopped after three of six".
    #[test]
    fn cancelling_between_icon_writes_leaves_written_files_intact_and_the_rest_untouched() {
        let scratch = ScratchDir::new("art-appearance-icons", "cancel-partway");
        let tree = scratch.path().to_path_buf();
        let names = ["Aaa", "Bbb", "Ccc", "Ddd", "Eee", "Fff"];
        for name in names {
            write_icon_entry(&tree, name, &icon_bytes(20, 20, None));
        }
        let before: Vec<Vec<u8>> = names
            .iter()
            .map(|name| std::fs::read(tree.join(format!("{name}.info"))).unwrap())
            .collect();

        let req = AppearanceRequest {
            wallpaper: None,
            screen_depth: None,
            shell_defaults: false,
            arrange_icons: true,
        };
        // `report` is called once for the indefinite planning phase, once
        // for the definite "applying" start, and then once per committed
        // file — cancelling after 2 reports past that leaves some icons
        // written and some not, which is the only case this test can prove
        // anything with.
        let sink = CancelAfter {
            seen: AtomicU64::new(0),
            after: 3,
            token: CancelToken::default(),
        };
        let err = apply_appearance_with(&tree, &req, &sink).unwrap_err();

        let files = match err {
            CoreError::CancelledPartway { files } => files,
            other => panic!("expected CancelledPartway with a real count, got {other:?}"),
        };
        assert!(files > 0, "the sink cancelled after some files landed");
        assert!(
            files < names.len() as u64,
            "and before all of them did, or this test proves nothing"
        );

        let mut written = 0u64;
        for (name, original) in names.iter().zip(before.iter()) {
            let after = std::fs::read(tree.join(format!("{name}.info"))).unwrap();
            if amigaicon::position(&after).unwrap().is_some() {
                written += 1;
            } else {
                assert_eq!(
                    &after, original,
                    "{name} was never reached and must be byte-for-byte unchanged"
                );
            }
        }
        assert_eq!(
            written, files,
            "the count in CancelledPartway must match how many icons actually landed"
        );
    }

    /// The thin wrapper produces the identical tree `apply_appearance_with`
    /// does when nothing ever cancels — the same split
    /// `core::gameindex::scan_titles`/`scan_titles_with` already uses, and
    /// the reason existing callers (every other test in this module) are
    /// unaffected by ART-248's job wiring.
    #[test]
    fn the_noprogress_wrapper_matches_the_sink_taking_form_byte_for_byte() {
        let (_scratch_a, tree_a) = build_tree("wrapper-parity-a");
        let (_scratch_b, tree_b) = build_tree("wrapper-parity-b");
        let picture_a = write_picture(&tree_a, "wallpaper.png");
        let picture_b = write_picture(&tree_b, "wallpaper.png");

        let req_a = AppearanceRequest {
            wallpaper: Some((
                wbpattern::Which::Root,
                WallpaperSource::HostPicture {
                    path: picture_a,
                    colours: 8,
                },
                wbpattern::Placement::ScaleGood,
            )),
            screen_depth: Some(6),
            shell_defaults: true,
            arrange_icons: false,
        };
        let req_b = AppearanceRequest {
            wallpaper: Some((
                wbpattern::Which::Root,
                WallpaperSource::HostPicture {
                    path: picture_b,
                    colours: 8,
                },
                wbpattern::Placement::ScaleGood,
            )),
            screen_depth: Some(6),
            shell_defaults: true,
            arrange_icons: false,
        };

        apply_appearance(&tree_a, &req_a).unwrap();
        apply_appearance_with(&tree_b, &req_b, &crate::core::jobs::NoProgress).unwrap();

        assert_eq!(
            std::fs::read(wbpattern_path(&tree_a)).unwrap(),
            std::fs::read(wbpattern_path(&tree_b)).unwrap()
        );
        assert_eq!(
            std::fs::read(screenmode_path(&tree_a)).unwrap(),
            std::fs::read(screenmode_path(&tree_b)).unwrap()
        );
        let backdrop_a = tree_a
            .join("Prefs")
            .join("Presets")
            .join("Backdrops")
            .join("wallpaper.iff");
        let backdrop_b = tree_b
            .join("Prefs")
            .join("Presets")
            .join("Backdrops")
            .join("wallpaper.iff");
        assert_eq!(
            std::fs::read(backdrop_a).unwrap(),
            std::fs::read(backdrop_b).unwrap()
        );
    }
}
