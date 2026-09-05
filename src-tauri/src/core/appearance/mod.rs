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

use crate::core::amigaprefs::{env, iff, screenmode, wbpattern};
use crate::core::error::{CoreError, CoreResult};
use crate::core::ilbm;
use crate::core::picture;
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
}

fn malformed_or_missing(rel: &str, root: &Path) -> CoreError {
    CoreError::InvalidInput(format!(
        "'{rel}' was not found under this distribution tree ('{}')",
        root.display()
    ))
}

/// Find a direct child of `dir` whose name matches `name` under AmigaDOS's
/// own case-folding rule (`core::osinstall::amiga_names_equal` — the same
/// fold `AdfSource`/`CdSource` already use to resolve a recipe path against
/// real media, extended here to a real host directory because nothing in
/// this codebase yet resolves a *host* tree path that way). A missing `dir`
/// is "not found", not an error — the caller decides whether that is fatal.
fn find_child_ci(dir: &Path, name: &str) -> CoreResult<Option<PathBuf>> {
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(err) => return Err(CoreError::Io(err)),
    };
    for entry in entries {
        let entry = entry?;
        let file_name = entry.file_name();
        let file_name = file_name.to_string_lossy();
        if crate::core::osinstall::amiga_names_equal(&file_name, name) {
            return Ok(Some(entry.path()));
        }
    }
    Ok(None)
}

/// Resolve a `/`-separated path under `root`, one component at a time,
/// case-insensitively. `Ok(None)` means some component was not found; it is
/// not itself an error, so a caller that treats "absent" as a legitimate
/// state (an optional file, a drawer that may not exist yet) is not forced
/// into matching on an `Err`.
fn resolve_ci_optional(root: &Path, rel: &str) -> CoreResult<Option<PathBuf>> {
    let mut current = root.to_path_buf();
    for segment in rel.split('/') {
        match find_child_ci(&current, segment)? {
            Some(next) => current = next,
            None => return Ok(None),
        }
    }
    Ok(Some(current))
}

/// [`resolve_ci_optional`], refusing by name — naming the full relative path
/// asked for, so a user missing `Prefs/Env-Archive/Sys/WBPattern.prefs` is
/// told exactly that rather than a generic "not found".
fn resolve_ci(root: &Path, rel: &str) -> CoreResult<PathBuf> {
    resolve_ci_optional(root, rel)?.ok_or_else(|| malformed_or_missing(rel, root))
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
        current = match find_child_ci(&current, segment)? {
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
    let rel = amiga_path.strip_prefix("Sys:").ok_or_else(|| {
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
    resolve_ci_optional(tree, SCREENMODE_REL)
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
            if find_child_ci(&drawer, &file_name)?.is_some() {
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
    // release's own unknown flag bits and `wbp_Revision` — is carried
    // forward rather than reset; only the content and the requested
    // placement are the user's own choice. See the module doc.
    let new_backdrop = wbpattern::Backdrop {
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
        let path = match find_child_ci(&dir, file_name)? {
            Some(existing) => existing,
            None => dir.join(file_name),
        };
        let bytes = env::encode_value(value)?;
        plans.push(ShellDefaultPlan { path, bytes });
    }
    Ok(plans)
}

/// Apply any combination of a wallpaper assignment, a screen depth and the
/// shell defaults to a distribution tree.
///
/// Every fallible step — resolving a path, decoding a picture, checking a
/// backdrop name is free — happens while building a plan for each requested
/// part, **before** this function writes anything at all. Only once every
/// requested part has a validated plan does it commit them, through
/// [`guarded_write`] (`BackupPolicy::CONFIG`) for every file. A refusal at
/// any point therefore leaves the whole tree — every prefs file, every
/// backdrop — exactly as it was.
pub fn apply_appearance(tree: &Path, req: &AppearanceRequest) -> CoreResult<AppearanceOutcome> {
    // ---- Plan: everything that can fail happens here. ----
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

    if let Some(plan) = wallpaper_plan {
        if let Some((picture_path, bytes)) = plan.picture {
            if let Some(parent) = picture_path.parent() {
                commit_mkdir(parent, &committed)?;
            }
            commit_write(picture_path.clone(), &bytes, &mut committed)?;
            picture_placed = Some(picture_path);
        }

        commit_write(plan.prefs_path, &plan.prefs_bytes, &mut committed)?;
        amiga_path = Some(plan.amiga_path);
    }

    if let Some(plan) = screen_plan {
        commit_write(plan.path, &plan.bytes, &mut committed)?;
    }

    for plan in shell_plans {
        if let Some(parent) = plan.path.parent() {
            commit_mkdir(parent, &committed)?;
        }
        commit_write(plan.path, &plan.bytes, &mut committed)?;
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
    })
}

/// Write one file through [`guarded_write`] and record it in `committed`.
/// See [`apply_appearance`]'s own comment on `committed` for why a failure
/// here is wrapped with what already succeeded rather than reported bare.
fn commit_write(
    path: PathBuf,
    bytes: &[u8],
    committed: &mut Vec<(PathBuf, Option<PathBuf>)>,
) -> CoreResult<()> {
    match guarded_write(&path, bytes, BackupPolicy::CONFIG) {
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
    CoreError::InvalidInput(format!("{err}, but {}", already.join("; ")))
}

/// List the backdrops a distribution tree actually holds — the file names
/// present under [`BACKDROPS_DRAWER`], sorted. A tree with no such drawer at
/// all (nothing has ever placed a backdrop) is an empty list, not an error:
/// listing what is there is not a claim that something ought to be.
pub fn backdrops_in_tree(tree: &Path) -> CoreResult<Vec<String>> {
    let drawer_rel = BACKDROPS_DRAWER.join("/");
    let Some(drawer) = resolve_ci_optional(tree, &drawer_rel)? else {
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
        revision: i8,
        other_flags: u16,
        content: wbpattern::Content,
    ) -> wbpattern::Backdrop {
        wbpattern::Backdrop {
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
    /// keeps them too.
    fn wbpattern_bytes_full(
        root_revision: i8,
        root_other_flags: u16,
        root_amiga_path: &str,
    ) -> Vec<u8> {
        let root = backdrop(
            wbpattern::Which::Root,
            root_revision,
            root_other_flags,
            wbpattern::Content::Picture(root_amiga_path.to_string()),
        );
        let drawer = backdrop(
            wbpattern::Which::Drawer,
            7,
            0x0040,
            wbpattern::Content::Picture("Sys:Prefs/Presets/Backdrops/drawer.iff".to_string()),
        );
        let screen = backdrop(
            wbpattern::Which::Screen,
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
        };
        let outcome = apply_appearance(&tree, &req).unwrap();
        assert!(outcome.picture_placed.is_none(), "nothing new was placed");
        assert_eq!(
            outcome.amiga_path.as_deref(),
            Some("Sys:Prefs/Presets/Backdrops/Christmas.iff")
        );
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
    /// request actually named (`placement`) really did change.
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
}
