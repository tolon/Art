//! Installing a WHDLoad pack onto a hard disk — the install half of spec §82.
//!
//! ```text
//! Game.lha → DROP → WHDLoad detected → Install to HDF → Backup → Apply → Verify
//! ```
//!
//! [`super`] (the parent module) works out *what* inside an archive is the
//! game. This module does the rest: re-plans against the disk's current
//! state, unpacks, joins the pack to the user's own game catalogue, and
//! writes the drawer and its icon.
//!
//! **ART-242.** Moved out of `commands/whdload.rs::run_install`, which did
//! all of this inline in the Tauri command — exactly the "technical
//! complexity" CLAUDE.md's core-independence rule says belongs in `core/`,
//! not in a command adapter. [`install_pack`] is the one function that
//! replaces it: the command now deserializes its arguments, calls this, and
//! serializes the result back. Nothing about the command's arguments, its
//! result JSON or its events changed in the move (pinned by
//! `commands::whdload::tests::the_install_result_event_keeps_its_wire_shape`
//! and `..::the_plan_result_keeps_its_wire_shape`).
//!
//! ## The one boundary this module cannot cross
//!
//! The actual disk write goes through a *session* — one volume opened,
//! backed up once and committed once for however many operations happen
//! inside it (`commands/volume_write.rs::with_volume`). That machinery is
//! itself already core-independent Rust — no Tauri, no Windows API, no
//! network — but it lives in `commands/` rather than `core/`, and moving it
//! is a round of its own, not this one's (ART-242's scope is `run_install`
//! and `igame_data_for_pack`). So [`VolumeSession`] is declared here and
//! implemented outside `core/`, in `commands/whdload.rs` — the same shape as
//! `MirrorClient`, `VolumeFormatter` and `HostRecycler`: trait in `core/`,
//! implementation outside it.

use std::path::Path;

use serde::Serialize;

use crate::core::error::{CoreError, CoreResult};
use crate::core::jobs::ProgressSink;
use crate::core::lha::open_archive;
use crate::core::lha::whdload::{detect_whdload, WhdloadVerdict};
use crate::core::sources::install::{unpack_for_install, Scratch};
use crate::core::volume::mount::{mount, scan_image, VolumeEntry};
use crate::core::volume::write::copy::{CopyReport, CopySource, HostFolder};
use crate::core::volume::write::plan::{plan_copy, CopyPlan, SourceEntry};
use crate::core::whdload::{analyse, Entry, PackLayout, MAX_ENTRIES};

/// Everything the user should see before deciding.
#[derive(Debug, Clone, Serialize)]
pub struct WhdloadPlan {
    /// What ART thinks this archive is, and how sure it is (§14, §34).
    pub verdict: WhdloadVerdict,
    /// Where the pack is inside the archive.
    pub layout: PackLayout,
    /// The drawer that will be created, as it will appear on the Amiga.
    pub drawer: String,
    /// The volume it will land on.
    pub volume_name: String,
    /// What it costs and what will not work.
    pub cost: CopyPlan,
    /// True when the destination already holds a drawer of that name.
    pub name_taken: bool,
    /// Why ART will not run this install, or `None` when it will.
    ///
    /// Never null when the install is refused, so the UI never has to invent a
    /// message.
    pub refusal: Option<WhdloadRefusal>,
}

/// Why ART will not run an install, and what the user can do about it.
///
/// The remedy travels with the reason rather than being a fixed sentence in
/// the panel: only one of these refusals is fixed by copying the archive by
/// hand, and telling someone whose disk is full — or whose archive needs an
/// Amiga to install itself — to do that is advice that cannot work.
#[derive(Debug, Clone, Serialize)]
pub struct WhdloadRefusal {
    /// Why not, in complete sentences. Carries no `ART-*` identifier: a
    /// refusal is an answer, not a fault (§68).
    pub reason: String,
    /// What to do instead, when there is something else to do. `None` when the
    /// reason already says it — repeating it would read as two suggestions.
    pub suggestion: Option<String>,
}

impl WhdloadRefusal {
    /// A reason whose own sentence already carries the remedy.
    fn plain(reason: impl Into<String>) -> Self {
        Self {
            reason: reason.into(),
            suggestion: None,
        }
    }

    fn with(reason: impl Into<String>, suggestion: impl Into<String>) -> Self {
        Self {
            reason: reason.into(),
            suggestion: Some(suggestion.into()),
        }
    }
}

impl WhdloadPlan {
    fn can_install(&self) -> bool {
        self.refusal.is_none()
    }
}

/// What the install did.
#[derive(Debug, Clone, Serialize)]
pub struct WhdloadOutcome {
    pub drawer: String,
    pub files: usize,
    pub directories: usize,
    pub bytes: u64,
    /// Files read back out of the volume and checked. Equals `files`.
    pub verified: usize,
    /// True when the pack's `.info` landed beside the drawer, so the game is
    /// visible on Workbench.
    pub icon_installed: bool,
    /// Anything left behind, with the reason. Never silent.
    pub skipped: Vec<String>,
    /// What ART knew about this title but could not fit into `igame.data` —
    /// a title too long for iGame's line, most often. Empty when nothing was
    /// left out, and empty (not an error) when nothing was written at all
    /// because ART had no route to write it — this is best-effort metadata,
    /// never a reason to call an otherwise-successful install a failure.
    pub igame_omitted: Vec<String>,
    /// Where the previous image went, for the whole-file strategy.
    pub backup: Option<String>,
}

// ---------------------------------------------------------------------------
// The volume-write boundary
// ---------------------------------------------------------------------------

/// Runs one WHDLoad pack's disk write inside a single volume session.
///
/// Declared here and implemented outside `core/` — see the module doc for
/// why the session machinery itself (`commands/volume_write.rs::with_volume`)
/// cannot move here yet.
pub trait VolumeSession {
    /// Create `drawer_name` under `parent`, copy `folder`'s contents into it,
    /// and place `icon` (name, bytes) beside the drawer — all inside one
    /// session, so a whole install produces one backup rather than several
    /// generations of the same image.
    ///
    /// Returns the copy report, whether the icon landed, and where the
    /// previous image went (the whole-file strategy only; `None` otherwise).
    #[allow(clippy::too_many_arguments)]
    fn install_drawer(
        &self,
        image: &Path,
        volume_index: usize,
        parent: u32,
        drawer_name: &str,
        folder: &HostFolder,
        icon: Option<(&str, &[u8])>,
        sink: &dyn ProgressSink,
    ) -> CoreResult<(CopyReport, bool, Option<String>)>;
}

// ---------------------------------------------------------------------------
// Planning
// ---------------------------------------------------------------------------

/// What installing `archive` into a volume would do. Writes nothing.
///
/// Unpacks the archive once, to a scratch directory that is discarded when
/// this returns. [`install_pack`] needs that same unpacked tree a moment
/// later to actually copy from — see [`build_plan_with_scratch`], which this
/// is now a thin wrapper over, for why a second call here would be the
/// leftover double extraction rather than a fresh one.
pub fn build_plan(
    archive: &Path,
    image: &Path,
    volume_index: usize,
    dir_block: u32,
    scratch_root: &Path,
) -> CoreResult<WhdloadPlan> {
    let (plan, _scratch) = build_plan_with_scratch(
        archive,
        image,
        volume_index,
        dir_block,
        scratch_root,
        &crate::core::jobs::NoProgress,
    )?;
    Ok(plan)
}

/// [`build_plan`]'s own body, plus the unpacked archive it would otherwise
/// throw away.
///
/// The archive used to be unpacked twice for one install: once here (called
/// from `install_pack` to re-plan against the disk's *current* state) and
/// once more by `install_pack` itself to actually copy from. Both unpacks
/// produce the identical tree from the identical archive, so the second one
/// bought nothing but the time to decompress a package a second time. This
/// keeps the plan's own unpack alive and hands it back — `Some(scratch)` when
/// there was a pack worth keeping it for, `None` when `analyse` found none
/// (the refusal path never reaches a write, so there is nothing worth
/// carrying an extra temp directory for). `install_pack` is the only other
/// caller; `build_plan` keeps its own contract by discarding what comes back.
fn build_plan_with_scratch(
    archive: &Path,
    image: &Path,
    volume_index: usize,
    dir_block: u32,
    scratch_root: &Path,
    sink: &dyn ProgressSink,
) -> CoreResult<(WhdloadPlan, Option<Scratch>)> {
    // The verdict comes from the archive's own entry list, before anything is
    // unpacked — a package ART is not confident about should not cost the user
    // a decompression first.
    let info = open_archive(archive)?;
    let verdict = detect_whdload(&info.entries);

    let (scratch, unpack_skipped) = unpack_for_install(archive, scratch_root, sink)?;
    let entries = walk(scratch.path())?;

    // `analyse` failing here means the archive unpacked fine but holds no
    // WHDLoad pack — that is an answer about the archive, not a fault in ART
    // (§68's identifiers are for faults). There is no drawer to name and
    // nothing to cost, so the plan reports the refusal with those fields at
    // their honest empty/zero rather than guessing at a layout that was never
    // found. `volume_name` is left blank too: reading it would mean mounting
    // the disk for a refusal that has nothing to do with the disk.
    let layout = match analyse(&entries) {
        Ok(layout) => layout,
        Err(CoreError::InvalidInput(reason)) => {
            return Ok((
                WhdloadPlan {
                    verdict,
                    layout: PackLayout {
                        root: String::new(),
                        name: String::new(),
                        slave: String::new(),
                        icon: None,
                        outside: Vec::new(),
                        needs_installer: false,
                    },
                    drawer: String::new(),
                    volume_name: String::new(),
                    cost: CopyPlan {
                        files: 0,
                        directories: 0,
                        total_bytes: 0,
                        blocks_needed: 0,
                        blocks_free: 0,
                        block_size: 0,
                        name_problems: Vec::new(),
                        collisions: Vec::new(),
                        split_icons: Vec::new(),
                    },
                    name_taken: false,
                    // The one refusal a hand copy actually answers: ART found
                    // no pack, but the archive still holds files the user may
                    // want.
                    refusal: Some(WhdloadRefusal::with(
                        reason,
                        "You can still copy it by hand from the Files screen.",
                    )),
                },
                None,
            ));
        }
        // Any other error out of `analyse` (there is none today, but the
        // match stays exhaustive on purpose) is a real fault, not an answer
        // about the archive — it keeps its identifier and reaches the error
        // banner.
        Err(other) => return Err(other),
    };

    let pack_root = if layout.root.is_empty() {
        scratch.path().to_path_buf()
    } else {
        crate::core::security::path::safe_join(scratch.path(), &layout.root).map_err(|err| {
            CoreError::SafetyRefused(format!(
                "the pack's folder is not inside the archive: {err}"
            ))
        })?
    };

    // The cost of the drawer's contents, plus the drawer itself and its icon.
    let folder = HostFolder::new(&pack_root, true);
    let mut sources: Vec<SourceEntry> = folder.entries()?;
    sources.push(SourceEntry {
        relative: layout.name.clone(),
        is_dir: true,
        bytes: 0,
    });
    if let Some(icon) = &layout.icon {
        sources.push(SourceEntry {
            relative: layout.icon_name(),
            is_dir: false,
            bytes: std::fs::metadata(scratch.path().join(icon))
                .map(|meta| meta.len())
                .unwrap_or(0),
        });
    }

    let entry = pick_volume(image, volume_index)?;
    let (device, geometry) = mount(image, &entry)?;
    let dir = if dir_block == 0 {
        geometry.root_block
    } else {
        dir_block
    };

    let set = crate::core::volume::write::layout::BlockSet::new(geometry.block_size);
    let existing: Vec<String> =
        crate::core::volume::write::dir::entries_in(&device, &set, &geometry, dir)?
            .into_iter()
            .map(|found| found.name)
            .collect();

    let name_taken = existing
        .iter()
        .any(|name| name.eq_ignore_ascii_case(&layout.name));

    let mut cost = plan_copy(&device, &geometry, &sources, &existing)?;
    // Anything the extractor refused belongs in the same report as everything
    // else the user is about to decide on.
    cost.name_problems
        .retain(|problem| !problem.relative.is_empty());

    // M2: `install_pack` writes `igame.data` into the pack's own drawer
    // alongside everything counted above, but nothing above ever measured
    // it — the archive never carries this file, so no `SourceEntry` names
    // it. A rendered file is a few dozen bytes, but AmigaDOS allocates whole
    // blocks: a header block plus at least one data block (the same
    // reasoning `igame::free_bytes_in_hardfile`'s own doc uses). Reserved
    // here as a small, fixed margin rather than by rendering the real file
    // early — that would mean plumbing the catalogue lookup into a planning
    // path that has no need of it otherwise, for two blocks out of what is
    // usually thousands. On a volume with room to spare this changes
    // nothing; on one within two blocks of the edge, it is the difference
    // between a preview that lied and one that did not.
    cost.blocks_needed += 2;

    let volume_name = read_volume_name(&device, &geometry).unwrap_or_else(|| entry.name.clone());

    let refusal = refuse(&verdict, &layout, &cost, name_taken, &unpack_skipped);

    Ok((
        WhdloadPlan {
            verdict,
            drawer: format!("{volume_name}:{}", layout.name),
            volume_name,
            layout,
            cost,
            name_taken,
            refusal,
        },
        Some(scratch),
    ))
}

/// Find the volume at `index` inside `image`.
///
/// `commands/volume_write.rs::pick_volume` answers the identical question for
/// the checkout commands, from the same `scan_image` call — this is not a
/// second way of reading a partition table, only a second five-line wrapper
/// around the same one, because this module must not depend on `commands/`
/// (the core-independence rule) to reach the wrapper that already exists
/// there.
fn pick_volume(image: &Path, index: usize) -> CoreResult<VolumeEntry> {
    let found = scan_image(image)?;
    // This message is duplicated in `commands/volume_write.rs::pick` — keep
    // the two in sync if either wording changes; a user hitting one should
    // not read a different sentence than one hitting the other for the same
    // mistake.
    found.volumes.get(index).cloned().ok_or_else(|| {
        CoreError::InvalidInput(format!(
            "this image has no volume {index} ({} found)",
            found.volumes.len()
        ))
    })
}

/// Why ART will not run this install, in the user's words.
///
/// Each of these is a case where going ahead would produce something that
/// looks installed and does not work — which is worse than a refusal, because
/// the user finds out later and somewhere else.
fn refuse(
    verdict: &WhdloadVerdict,
    layout: &PackLayout,
    cost: &CopyPlan,
    name_taken: bool,
    unpack_skipped: &[String],
) -> Option<WhdloadRefusal> {
    use crate::core::workflow::types::Confidence;

    if matches!(verdict.confidence, Confidence::Low | Confidence::Unknown) {
        return Some(WhdloadRefusal::plain(format!(
            "ART is not confident this is a WHDLoad package ({}). {} \
             Install it by hand from the Files screen if you know it is one.",
            describe_confidence(verdict.confidence),
            verdict.notes
        )));
    }

    if layout.needs_installer {
        return Some(WhdloadRefusal::plain(
            "This archive holds an Install script, which means the game has not been \
             installed yet. Running it needs an Amiga — install it in WinUAE first, then \
             bring the finished drawer back here.",
        ));
    }

    if name_taken {
        return Some(WhdloadRefusal::plain(format!(
            "'{}' is already on that volume. Rename or remove it first — ART will not \
             write a game over one that is already there.",
            layout.name
        )));
    }

    if cost.blocks_needed > cost.blocks_free {
        return Some(WhdloadRefusal::with(
            format!(
                "This needs {} blocks and {} are free.",
                cost.blocks_needed, cost.blocks_free
            ),
            "Free some space on that volume, or choose another partition — copying it \
             by hand would run out of room in the same place.",
        ));
    }

    if !cost.name_problems.is_empty() {
        return Some(WhdloadRefusal::plain(format!(
            "{} name(s) in this package are ones AmigaDOS cannot store. Installing it \
             under different names would give you a game whose files no longer match \
             what its slave looks for.",
            cost.name_problems.len()
        )));
    }

    if !unpack_skipped.is_empty() {
        return Some(WhdloadRefusal::with(
            format!(
                "The archive did not unpack completely ({}). Installing part of a game \
                 produces one that does not start.",
                unpack_skipped.first().cloned().unwrap_or_default()
            ),
            "Download the package again — an archive that unpacks short usually \
             arrived damaged.",
        ));
    }

    None
}

fn describe_confidence(confidence: crate::core::workflow::types::Confidence) -> &'static str {
    use crate::core::workflow::types::Confidence;
    match confidence {
        Confidence::High => "high confidence",
        Confidence::Medium => "medium confidence",
        Confidence::Low => "low confidence",
        Confidence::Unknown => "no WHDLoad markers found",
    }
}

fn read_volume_name(
    device: &dyn crate::core::volume::BlockDevice,
    geometry: &crate::core::volume::VolumeGeometry,
) -> Option<String> {
    let block = crate::core::volume::read_block_vec(device, geometry.root_block).ok()?;
    let root = crate::core::adf::blocks::RootBlock::parse(&block).ok()?;
    (!root.volume_name.is_empty()).then_some(root.volume_name)
}

/// Everything under `root`, as relative paths.
fn walk(root: &Path) -> CoreResult<Vec<Entry>> {
    let mut out = Vec::new();
    walk_into(root, "", 0, &mut out)?;
    Ok(out)
}

fn walk_into(base: &Path, relative: &str, depth: usize, out: &mut Vec<Entry>) -> CoreResult<()> {
    use crate::core::volume::write::copy::MAX_COPY_DEPTH;

    if depth > MAX_COPY_DEPTH || out.len() >= MAX_ENTRIES {
        return Ok(());
    }

    let here = if relative.is_empty() {
        base.to_path_buf()
    } else {
        crate::core::security::path::safe_join(base, relative).map_err(|err| {
            CoreError::SafetyRefused(format!("'{relative}' escapes the unpacked archive: {err}"))
        })?
    };

    for entry in std::fs::read_dir(&here)? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().to_string();
        let child = if relative.is_empty() {
            name
        } else {
            format!("{relative}/{name}")
        };

        let kind = entry.file_type()?;
        if kind.is_dir() {
            out.push(Entry::dir(&child));
            walk_into(base, &child, depth + 1, out)?;
        } else if kind.is_file() {
            out.push(Entry::file(&child));
        }
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Installing
// ---------------------------------------------------------------------------

/// Re-plan, unpack once, and write — all in one volume session.
///
/// The plan is rebuilt here rather than carried from the UI. A plan the user
/// looked at five minutes ago describes a disk that may have changed since,
/// and installing against a stale one is how a "there is room" turns into a
/// half-written game. Re-planning still means unpacking the archive — the
/// cost is re-run against the disk's *current* state — but that unpack is the
/// same one the actual copy reads from: [`build_plan_with_scratch`] hands its
/// scratch directory back rather than this function unpacking a second time
/// (the leftover this fixes; the two used to disagree about nothing, because
/// they extracted the identical archive into two different directories and
/// read only one of them).
///
/// The actual volume write goes through `session` ([`VolumeSession`]) — see
/// its doc, and the module doc, for why that boundary exists rather than a
/// direct call.
#[allow(clippy::too_many_arguments)]
pub fn install_pack(
    archive: &Path,
    image: &Path,
    volume_index: usize,
    parent: u32,
    scratch_root: &Path,
    catalogue_dir: &Path,
    session: &dyn VolumeSession,
    sink: &dyn ProgressSink,
) -> CoreResult<WhdloadOutcome> {
    sink.report(0, None, "Checking the package");
    let (plan, scratch) =
        build_plan_with_scratch(archive, image, volume_index, parent, scratch_root, sink)?;
    if !plan.can_install() {
        return Err(CoreError::SafetyRefused(
            plan.refusal
                .map(|refusal| refusal.reason)
                .unwrap_or_else(|| "ART will not install this".into()),
        ));
    }
    // `build_plan_with_scratch` only ever returns `None` on the refusal path
    // (no pack found), and that refusal was just handled above — an
    // installable plan always found a pack, and finding one always keeps the
    // scratch directory it was found in.
    let scratch =
        scratch.expect("an installable plan always keeps the scratch directory it was built from");
    let layout = plan.layout;

    let pack_root = if layout.root.is_empty() {
        scratch.path().to_path_buf()
    } else {
        crate::core::security::path::safe_join(scratch.path(), &layout.root).map_err(|err| {
            CoreError::SafetyRefused(format!(
                "the pack's folder is not inside the archive: {err}"
            ))
        })?
    };

    // iGame's own launcher reads a small file from the same directory as the
    // slave, and this drawer is one ART made moments ago by unpacking the
    // archive — the "no ceremony" default path, because nothing of the
    // user's is being touched. Written into `pack_root` *before* the folder
    // below is walked, so it rides along with everything else `install_drawer`
    // places into the drawer, the same way any other file the archive shipped
    // would.
    //
    // `igame_data_for_pack` looks the pack up in the user's own catalogue by
    // its content-derived identity and returns the fuller record when there
    // is one — a pack ART has never catalogued yields a title-only file
    // instead, and that is not a defect (see the function's own doc).
    //
    // Best-effort and disclosed rather than tested: a failure anywhere in
    // that lookup, or in the write itself, must not turn an
    // otherwise-successful install into a reported failure over a file
    // WHDLoad itself never reads. `BackupPolicy::NONE` because this drawer is
    // one ART unpacked moments ago — nothing of the user's exists yet to
    // preserve.
    //
    // I2: what did not fit is carried into the outcome rather than dropped —
    // a title too long for iGame's line used to produce an *empty*
    // `igame.data` that this install then counted and reported as written and
    // verified, about a file that said nothing. `write_beside` itself now
    // refuses to write that empty file at all (`WriteOutcome::NothingFit`),
    // so the file simply will not be among what the session finds below — no
    // separate accounting needed here for that half of the fix.
    let igame_data = igame_data_for_pack(&pack_root, catalogue_dir, &layout.name);
    let igame_omitted = match crate::core::gameindex::igame::write_beside(
        &pack_root,
        &igame_data,
        crate::core::safety::BackupPolicy::NONE,
    ) {
        Ok(written) => crate::core::gameindex::igame::notable_omissions(&written.omitted),
        // `BackupPolicy::NONE` above means `failure.backup` is always `None`
        // on this path — nothing of the user's is being touched, so there is
        // never anything to preserve — but the field still flows through
        // rather than being silently dropped, the same as every other caller.
        Err(failure) => {
            log::debug!(
                "whdload: could not write igame.data beside '{}': {}",
                layout.name,
                failure.error
            );
            Vec::new()
        }
    };

    // Sidecars on: the archive may carry `.uaem` files, and a slave's bits are
    // the difference between a game that starts and one that does not (§7.2).
    let folder = HostFolder::new(&pack_root, true);
    let icon_bytes = layout
        .icon
        .as_ref()
        .map(|icon| std::fs::read(scratch.path().join(icon)))
        .transpose()?;

    let drawer_name = layout.name.clone();
    let icon_name = layout.icon_name();
    let icon_arg = icon_bytes
        .as_ref()
        .map(|bytes| (icon_name.as_str(), bytes.as_slice()));

    // One session, so the whole install is one backup rather than three
    // generations of the same image (§1).
    let (report, icon_installed, backup) = session.install_drawer(
        image,
        volume_index,
        parent,
        &drawer_name,
        &folder,
        icon_arg,
        sink,
    )?;

    Ok(WhdloadOutcome {
        drawer: plan.drawer,
        files: report.files_copied,
        directories: report.directories_created,
        bytes: report.bytes_copied,
        verified: report.files_verified,
        icon_installed,
        skipped: report.skipped,
        igame_omitted,
        backup,
    })
}

/// The `igame.data` a freshly-unpacked pack should carry — "the catalogue
/// record that named it," in the plan's own words.
///
/// **Why this is a real join and not a guess.** A `GameRecord`'s identity is
/// content-derived (`record::derive_id(title, sha256-of-slave-bytes)`), never
/// path-derived, so it does not matter *how* this exact pack was catalogued
/// before today — as an unpacked drawer (`readers::drawer`) or still sitting
/// inside an archive (`readers::lhadrawer`) — both hash the slave's own bytes
/// (`lhadrawer` bounds its read to 2 MB; a real WHDLoad slave is kilobytes,
/// so the bound is never the difference) and derive the same id from the same
/// title. Calling `readers::drawer::read_drawer` on the pack this install
/// just unpacked, rather than re-deriving that id by hand a second time,
/// means this join breaks on the same day the identity rule does — not a day
/// later, from a second copy of it going quietly out of step.
///
/// **No match is not a defect.** A pack ART has never catalogued — most
/// WHDLoad archives on a first install — yields a title-only file, because
/// that is exactly what ART knows about it. The same fallback covers any
/// failure along the way (the pack cannot be read back as a drawer, the
/// catalogue cannot be loaded): a metadata lookup must never turn an
/// otherwise-successful install into a reported failure.
///
/// **`players` is always `None`.** Not a gap in the join: `GameRecord` has no
/// player-count field anywhere in ART's catalogue today, so there is nothing
/// for any lookup to find.
fn igame_data_for_pack(
    pack_root: &Path,
    catalogue_dir: &Path,
    fallback_title: &str,
) -> crate::core::gameindex::igame::IGameData {
    use crate::core::gameindex::igame::IGameData;
    use crate::core::gameindex::readers::drawer::read_drawer;
    use crate::core::gameindex::store;

    let fallback = || IGameData {
        title: Some(fallback_title.to_string()),
        ..Default::default()
    };

    let Ok(Some(fresh)) = read_drawer(pack_root) else {
        return fallback();
    };
    let Ok(roots) = store::load(catalogue_dir) else {
        return fallback();
    };
    let Some(catalogued) = roots
        .iter()
        .flat_map(|root| root.entries.iter())
        .map(|entry| &entry.record)
        .find(|record| record.id == fresh.id)
    else {
        return fallback();
    };

    IGameData {
        title: Some(catalogued.title.value.clone()),
        chipset: catalogued
            .chipset
            .as_ref()
            .map(|fact| fact.value.display_name().to_string()),
        genre: catalogued.genre.as_ref().map(|fact| fact.value.clone()),
        year: catalogued.year.as_ref().map(|fact| fact.value),
        players: None,
        exe: None,
    }
}

// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    use crate::core::jobs::NoProgress;
    use crate::core::lha::OverwritePolicy;
    use crate::core::volume::fixture::ffs_volume;
    use crate::core::volume::write::copy::copy_into_volume;
    use crate::core::volume::write::{FileMeta, VolumeWriter};
    use crate::core::volume::DosType;
    use crate::core::ScratchDir;

    /// Minor (2026-09-07 final review): these nine tests moved here with a
    /// hand-rolled directory and a trailing `remove_dir_all` — exactly
    /// ART-184's shape, where the cleanup is skipped precisely when the test
    /// panics. `ScratchDir`'s `Drop` runs on the panicking path too, so the
    /// guard is returned alongside the path rather than the path alone
    /// (`ScratchDir::join` is deliberately not `Deref`, so a bare `PathBuf`
    /// call site would have to change everywhere `dir` is used as a path;
    /// returning the tuple keeps every existing `dir.join(..)` call site
    /// unchanged while the guard's own lifetime stays visible at the call
    /// site, the same shape `amigainstall/finish.rs::tests::tree` uses).
    fn scratch(name: &str) -> (ScratchDir, PathBuf) {
        let guard = ScratchDir::new("art-whd-core", name);
        let dir = guard.path().to_path_buf();
        (guard, dir)
    }

    /// A wrapped pack laid out on disk, the way an unpacked archive looks.
    fn unpacked_pack(root: &Path) {
        std::fs::create_dir_all(root.join("Turrican/data")).unwrap();
        std::fs::write(root.join("Turrican/Turrican.slave"), b"slave bytes").unwrap();
        std::fs::write(root.join("Turrican/Turrican"), b"host executable").unwrap();
        std::fs::write(root.join("Turrican/data/level1.bin"), vec![7u8; 4000]).unwrap();
        std::fs::write(root.join("Turrican.info"), b"icon bytes").unwrap();
        std::fs::write(root.join("Turrican.readme"), b"about this pack").unwrap();
    }

    #[test]
    fn walking_an_unpacked_archive_finds_the_pack() {
        let (_scratch, dir) = scratch("walk");
        unpacked_pack(&dir);

        let entries = walk(&dir).unwrap();
        let layout = analyse(&entries).unwrap();

        assert_eq!(layout.root, "Turrican");
        assert_eq!(layout.name, "Turrican");
        assert_eq!(layout.icon.as_deref(), Some("Turrican.info"));
        assert_eq!(layout.outside, vec!["Turrican.readme"]);
    }

    /// A `VolumeSession` built entirely from `core` primitives — the write
    /// strategy, backup generations and the deep-integrity gate that
    /// `commands/volume_write.rs::with_volume` provides are *not* under test
    /// here (they have their own tests, in `commands/whdload.rs` and
    /// `commands/volume_write.rs`). What this proves is `install_pack`'s own
    /// plumbing: the session is asked to write, and only on success is the
    /// image file touched at all — mirroring the real whole-file strategy's
    /// "validate in memory, then one write" shape closely enough that a
    /// mid-way failure here (a copy that never finishes) leaves the file
    /// byte-for-byte as it was, the same guarantee production code gives.
    struct TestVolumeSession;

    impl VolumeSession for TestVolumeSession {
        fn install_drawer(
            &self,
            image: &Path,
            volume_index: usize,
            parent: u32,
            drawer_name: &str,
            folder: &HostFolder,
            icon: Option<(&str, &[u8])>,
            sink: &dyn ProgressSink,
        ) -> CoreResult<(CopyReport, bool, Option<String>)> {
            let entry = pick_volume(image, volume_index)?;
            let (_, geometry) = mount(image, &entry)?;

            let original = std::fs::read(image)?;
            let start = entry.byte_offset as usize;
            let end = (start + entry.byte_length as usize).min(original.len());
            let mut device = crate::core::volume::device::VecDevice::new(
                original[start..end].to_vec(),
                entry.block_size,
            )?;

            let (report, installed) = {
                let mut writer =
                    VolumeWriter::open(&mut device, geometry, image, entry.byte_offset)?;
                let drawer = writer.make_dir(parent, drawer_name)?.block.ok_or_else(|| {
                    CoreError::Malformed {
                        format: "volume".into(),
                        detail: "the drawer was created but ART lost track of it".into(),
                    }
                })?;

                let report =
                    copy_into_volume(&mut writer, drawer, folder, OverwritePolicy::Skip, sink)?;
                if report.cancelled {
                    // Same rule `install_pack` relies on in production: a
                    // WHDLoad pack missing files it never got to copy is not
                    // a partial success. Returning here — before the icon is
                    // written and before this closure reaches the commit
                    // below — is what keeps the image untouched.
                    return Err(CoreError::Cancelled);
                }

                let installed = match icon {
                    Some((name, bytes)) => {
                        writer.add_file(parent, name, bytes, FileMeta::default())?;
                        true
                    }
                    None => false,
                };

                (report, installed)
            };

            // Committed only now, on success — the same "nothing reaches the
            // user's file until the whole operation validated" shape the real
            // whole-file strategy uses, just without its backup/deep-check
            // machinery (that belongs to `with_volume`'s own tests).
            let mut whole = original;
            whole[start..end].copy_from_slice(device.bytes());
            std::fs::write(image, &whole)?;

            Ok((report, installed, None))
        }
    }

    fn verdict(confidence: crate::core::workflow::types::Confidence) -> WhdloadVerdict {
        WhdloadVerdict {
            confidence,
            slave: Some("Game/Game.slave".into()),
            executable: Some("Game/Game".into()),
            has_data_dir: true,
            has_icon: true,
            notes: "test".into(),
        }
    }

    fn layout() -> PackLayout {
        PackLayout {
            root: "Game".into(),
            name: "Game".into(),
            slave: "Game/Game.slave".into(),
            icon: Some("Game.info".into()),
            outside: Vec::new(),
            needs_installer: false,
        }
    }

    fn cost(needed: usize, free: usize) -> CopyPlan {
        CopyPlan {
            files: 3,
            directories: 1,
            total_bytes: 4000,
            blocks_needed: needed,
            blocks_free: free,
            block_size: 512,
            name_problems: Vec::new(),
            collisions: Vec::new(),
            split_icons: Vec::new(),
        }
    }

    /// §14 and §34: an uncertain detection is never acted on as if it were a
    /// fact. Copying an arbitrary folder onto a hard disk because it *might*
    /// be a game is not a one-click feature, it is a mess to clean up.
    #[test]
    fn a_low_confidence_detection_is_refused_with_the_reason() {
        use crate::core::workflow::types::Confidence;

        let reason = refuse(
            &verdict(Confidence::Low),
            &layout(),
            &cost(10, 1000),
            false,
            &[],
        )
        .expect("a low-confidence detection must be refused")
        .reason;

        assert!(reason.contains("not confident"), "{reason}");
        assert!(reason.contains("by hand"), "and offers the alternative");
    }

    #[test]
    fn a_confident_detection_with_room_is_allowed() {
        use crate::core::workflow::types::Confidence;

        assert!(refuse(
            &verdict(Confidence::High),
            &layout(),
            &cost(10, 1000),
            false,
            &[]
        )
        .is_none());
    }

    /// A source pack needs an Amiga to install itself. Copying its raw disk
    /// images into a drawer produces something that looks installed and does
    /// not start.
    #[test]
    fn an_archive_needing_its_installer_is_refused() {
        use crate::core::workflow::types::Confidence;

        let mut needs = layout();
        needs.needs_installer = true;

        let reason = refuse(
            &verdict(Confidence::High),
            &needs,
            &cost(10, 1000),
            false,
            &[],
        )
        .expect("a source pack must be refused")
        .reason;
        assert!(reason.contains("Install script"), "{reason}");
        assert!(reason.contains("WinUAE"), "and says what to do instead");
    }

    /// Writing a game over one that is already there is not something a
    /// one-click button gets to decide.
    #[test]
    fn a_name_already_on_the_volume_is_refused() {
        use crate::core::workflow::types::Confidence;

        let reason = refuse(
            &verdict(Confidence::High),
            &layout(),
            &cost(10, 1000),
            true,
            &[],
        )
        .expect("a taken name must be refused")
        .reason;
        assert!(reason.contains("already on that volume"), "{reason}");
    }

    #[test]
    fn a_pack_that_does_not_fit_is_refused_with_the_numbers() {
        use crate::core::workflow::types::Confidence;

        let reason = refuse(
            &verdict(Confidence::High),
            &layout(),
            &cost(5000, 100),
            false,
            &[],
        )
        .expect("a pack that does not fit must be refused")
        .reason;
        assert!(reason.contains("5000 blocks"), "{reason}");
        assert!(reason.contains("100 are free"), "{reason}");
    }

    /// A slave looks for its files by name. Installing them under names ART
    /// invented gives a game that starts and then cannot find anything.
    #[test]
    fn names_amigados_cannot_store_refuse_the_install() {
        use crate::core::volume::write::plan::NameProblem;
        use crate::core::workflow::types::Confidence;

        let mut costs = cost(10, 1000);
        costs.name_problems.push(NameProblem {
            relative: "Game/a-very-long-name-indeed-far-too-long".into(),
            name: "a-very-long-name-indeed-far-too-long".into(),
            reason: "too long".into(),
            suggestion: Some("a-very-long-name-indeed-far-to".into()),
        });

        let reason = refuse(&verdict(Confidence::High), &layout(), &costs, false, &[])
            .expect("unstorable names must refuse the install")
            .reason;
        assert!(reason.contains("no longer match"), "{reason}");
    }

    /// Half a game is not a game.
    #[test]
    fn an_archive_that_did_not_unpack_completely_is_refused() {
        use crate::core::workflow::types::Confidence;

        let reason = refuse(
            &verdict(Confidence::High),
            &layout(),
            &cost(10, 1000),
            false,
            &["Game/data/big.bin (too large)".into()],
        )
        .expect("an incomplete unpack must be refused")
        .reason;
        assert!(reason.contains("did not unpack completely"), "{reason}");
    }

    /// A real `.lha` holding a real WHDLoad pack, on disk.
    ///
    /// Level-0 stored entries with `/` in the names, which is how an Amiga
    /// archive carries a folder — the same fixture shape the LHA tests use.
    fn whdload_archive(path: &Path) {
        use crate::core::lha::tests::make_lha_with;

        let slave = b"WHDLOADSLAVE\x00\x00\x00\x0a";
        let executable = b"host executable bytes";
        let level = vec![7u8; 4000];
        let icon = b"\xe3\x10\x00\x01icon";

        let bytes = make_lha_with(&[
            ("Turrican/Turrican.slave", slave),
            ("Turrican/Turrican", executable),
            ("Turrican/data/level1.bin", &level),
            ("Turrican.info", icon),
            ("Turrican.readme", b"about this pack"),
        ]);
        std::fs::write(path, bytes).unwrap();
    }

    /// **The core function, end to end.** A pack installs into a volume image
    /// and the catalogue join produces the record — the two things ART-242
    /// asks a core-level test to prove. Nothing here reaches inside
    /// `install_pack`; it hands over an archive, a disk and a `VolumeSession`,
    /// then reads the disk back to see what actually landed.
    ///
    /// The backup-path assertion the command-level version of this test
    /// carried is deliberately not repeated here: producing a backup is
    /// `VolumeSession`'s job, not `install_pack`'s, and it is already proved
    /// by `commands/whdload.rs::tests::a_whole_install_backs_the_image_up_once`
    /// against the real session.
    #[test]
    fn a_whdload_archive_installs_onto_a_disk_and_reads_back() {
        let (_scratch, dir) = scratch("e2e");
        let archive = dir.join("Turrican.lha");
        whdload_archive(&archive);

        let image = dir.join("Games.hdf");
        let (bytes, _) = ffs_volume(1760, DosType::new(*b"DOS\x01"));
        std::fs::write(&image, &bytes).unwrap();
        let before = std::fs::read(&image).unwrap();

        // ---- the plan, which must write nothing ----
        let plan = build_plan(&archive, &image, 0, 0, &std::env::temp_dir()).unwrap();
        assert_eq!(
            std::fs::read(&image).unwrap(),
            before,
            "planning must not touch the image"
        );
        assert!(
            plan.refusal.is_none(),
            "a well-formed pack that fits should be installable: {:?}",
            plan.refusal
        );
        assert_eq!(plan.layout.name, "Turrican");
        assert_eq!(plan.layout.icon.as_deref(), Some("Turrican.info"));
        assert_eq!(
            plan.layout.outside,
            vec!["Turrican.readme"],
            "the readme is not part of the game"
        );
        assert!(plan.drawer.ends_with(":Turrican"), "{}", plan.drawer);

        // ---- the install ----
        let outcome = install_pack(
            &archive,
            &image,
            0,
            0,
            &std::env::temp_dir(),
            &std::env::temp_dir(),
            &TestVolumeSession,
            &NoProgress,
        )
        .unwrap();

        assert_eq!(
            outcome.files, 4,
            "slave, executable, one data file, and igame.data"
        );
        assert_eq!(
            outcome.verified, outcome.files,
            "every file is read back out of the disk"
        );
        assert!(outcome.icon_installed);
        assert!(outcome.skipped.is_empty(), "{:?}", outcome.skipped);

        // ---- what is actually on the disk ----
        let entry = pick_volume(&image, 0).unwrap();
        let (device, geometry) = mount(&image, &entry).unwrap();
        let set = crate::core::volume::write::layout::BlockSet::new(geometry.block_size);

        let root = crate::core::volume::write::dir::entries_in(
            &device,
            &set,
            &geometry,
            geometry.root_block,
        )
        .unwrap();

        let drawer = root
            .iter()
            .find(|found| found.name == "Turrican")
            .expect("the drawer must be on the disk");
        assert!(drawer.is_dir);

        // The icon beside the drawer, not inside it. Inside, it describes
        // nothing and the game stays invisible on Workbench.
        assert!(
            root.iter()
                .any(|found| found.name == "Turrican.info" && !found.is_dir),
            "the icon must sit beside the drawer: {:?}",
            root.iter().map(|f| &f.name).collect::<Vec<_>>()
        );

        // The readme must NOT be there: it is not part of the game.
        assert!(
            !root.iter().any(|found| found.name == "Turrican.readme"),
            "the readme is not part of the game and must not be installed"
        );

        let inside =
            crate::core::volume::write::dir::entries_in(&device, &set, &geometry, drawer.block)
                .unwrap();

        let slave = inside
            .iter()
            .find(|found| found.name == "Turrican.slave")
            .expect("the slave must be in the drawer");
        assert_eq!(
            crate::core::volume::write::file::read_file(&device, &set, &geometry, slave.block)
                .unwrap(),
            b"WHDLOADSLAVE\x00\x00\x00\x0a",
            "the slave's bytes must survive the whole trip"
        );

        let data = inside
            .iter()
            .find(|found| found.name == "data" && found.is_dir)
            .expect("the data drawer must be in the pack");
        let level =
            crate::core::volume::write::dir::entries_in(&device, &set, &geometry, data.block)
                .unwrap();
        assert_eq!(level.len(), 1);
        assert_eq!(
            crate::core::volume::write::file::read_file(&device, &set, &geometry, level[0].block)
                .unwrap()
                .len(),
            4000,
            "a nested data file must arrive whole"
        );

        // The catalogue join: this archive was never catalogued (no
        // catalogue_dir seeded for this test), so this doubles as the "no
        // match" case — `igame_data_for_pack`'s fallback, exercised end to
        // end.
        let igame = inside
            .iter()
            .find(|found| found.name == crate::core::gameindex::igame::FILE_NAME)
            .expect("igame.data must be beside the slave in the installed drawer");
        assert_eq!(
            crate::core::volume::write::file::read_file(&device, &set, &geometry, igame.block)
                .unwrap(),
            b"title=Turrican\n",
            "ART's own drawer name, for iGame's own launcher to read"
        );
    }

    /// The catalogue-hit half: a pack whose slave bytes and title already
    /// match a record in the user's own catalogue gets that record's fuller
    /// facts, not just the drawer's own name. This is what "the catalogue
    /// join produces the record" means — the fuller record, not just a title.
    ///
    /// `read_drawer` is called on a stand-in directory carrying the exact
    /// same slave bytes to get the exact content-derived id production code
    /// will compute — the same function, not a hand-rederived hash, so the
    /// test cannot drift from what `igame_data_for_pack` actually does. The
    /// catalogue is then seeded with a record sharing that id but carrying a
    /// genre, a year and a chipset no WHDLoad slave header ever states, plus
    /// a title a user typed by hand — proving the join is what supplied
    /// them, not a second read of the slave.
    #[test]
    fn a_catalogued_pack_gets_its_igame_data_from_the_catalogue_record() {
        use crate::core::gameindex::readers::drawer::read_drawer;
        use crate::core::gameindex::record::{ChipsetRequirement, Fact, Provenance};
        use crate::core::gameindex::store::{CachedEntry, CatalogueRoot, CATALOGUE_SCHEMA};

        let (_scratch, dir) = scratch("igame-catalogue-hit");
        let slave = crate::core::gameindex::readers::slave::tests_support::build_slave(
            "Turrican",
            "1992 Someone",
            16,
        );

        let archive = dir.join("Turrican.lha");
        std::fs::write(
            &archive,
            crate::core::lha::tests::make_lha_with(&[("Turrican/Turrican.slave", &slave)]),
        )
        .unwrap();

        // The same content, read the same way production code will read it,
        // to get the real id — never hand-derived.
        let probe_dir = dir.join("probe").join("Turrican");
        std::fs::create_dir_all(&probe_dir).unwrap();
        std::fs::write(probe_dir.join("Turrican.slave"), &slave).unwrap();
        let fresh = read_drawer(&probe_dir).unwrap().expect("this is a title");

        let catalogue_dir = dir.join("catalogue");
        let collection_root = dir.join("collection");
        std::fs::create_dir_all(&collection_root).unwrap();
        let root_key = collection_root.to_string_lossy().into_owned();
        crate::core::gameindex::store::add_root(&catalogue_dir, Path::new(&root_key)).unwrap();
        let mut record = fresh.clone();
        record.title = Fact::new(
            "Turrican II: Definitive Edition".into(),
            Provenance::UserEdit,
        );
        record.genre = Some(Fact::new("Shoot'em up".into(), Provenance::UserEdit));
        record.year = Some(Fact::new(1991, Provenance::UserEdit));
        record.chipset = Some(Fact::new(ChipsetRequirement::Aga, Provenance::UserEdit));
        crate::core::gameindex::store::write_root(
            &catalogue_dir,
            &CatalogueRoot {
                schema: CATALOGUE_SCHEMA,
                root: root_key,
                scanned_at: None,
                index_schema: crate::core::gameindex::record::GAMEINDEX_SCHEMA,
                entries: vec![CachedEntry {
                    path: probe_dir
                        .join("Turrican.slave")
                        .to_string_lossy()
                        .into_owned(),
                    size: 0,
                    mtime_ms: 0,
                    record,
                }],
            },
        )
        .unwrap();

        let image = dir.join("Games.hdf");
        let (bytes, _) = ffs_volume(1760, DosType::new(*b"DOS\x01"));
        std::fs::write(&image, &bytes).unwrap();

        install_pack(
            &archive,
            &image,
            0,
            0,
            &std::env::temp_dir(),
            &catalogue_dir,
            &TestVolumeSession,
            &NoProgress,
        )
        .unwrap();

        let entry = pick_volume(&image, 0).unwrap();
        let (device, geometry) = mount(&image, &entry).unwrap();
        let set = crate::core::volume::write::layout::BlockSet::new(geometry.block_size);
        let drawer = crate::core::volume::write::dir::entries_in(
            &device,
            &set,
            &geometry,
            geometry.root_block,
        )
        .unwrap()
        .into_iter()
        .find(|found| found.name == "Turrican")
        .expect("the drawer must be on the disk");
        let igame =
            crate::core::volume::write::dir::entries_in(&device, &set, &geometry, drawer.block)
                .unwrap()
                .into_iter()
                .find(|found| found.name == crate::core::gameindex::igame::FILE_NAME)
                .expect("igame.data must be beside the slave");
        let text = String::from_utf8(
            crate::core::volume::write::file::read_file(&device, &set, &geometry, igame.block)
                .unwrap(),
        )
        .unwrap();

        assert!(
            text.contains("title=Turrican II: Definitive Edition"),
            "the catalogue's own title, not the slave's or the drawer's: {text}"
        );
        assert!(text.contains("genre=Shoot'em up"), "{text}");
        assert!(text.contains("year=1991"), "{text}");
        assert!(text.contains("chipset=AGA"), "{text}");
    }

    /// The uncatalogued half of the join: a pack ART has never catalogued
    /// still installs cleanly and still gets an `igame.data`, carrying only
    /// what ART actually knows about it.
    #[test]
    fn an_uncatalogued_pack_still_gets_a_title_only_igame_data() {
        let (_scratch, dir) = scratch("igame-no-catalogue");
        let archive = dir.join("Tag.lha");
        let slave = crate::core::gameindex::readers::slave::tests_support::build_slave(
            "Tag",
            "1993 Someone",
            16,
        );
        std::fs::write(
            &archive,
            crate::core::lha::tests::make_lha_with(&[("Tag/Tag.slave", &slave)]),
        )
        .unwrap();

        let image = dir.join("Games.hdf");
        let (bytes, _) = ffs_volume(1760, DosType::new(*b"DOS\x01"));
        std::fs::write(&image, &bytes).unwrap();

        // A catalogue directory that has never seen this pack — the ordinary
        // first-install case.
        let catalogue_dir = dir.join("catalogue");

        let outcome = install_pack(
            &archive,
            &image,
            0,
            0,
            &std::env::temp_dir(),
            &catalogue_dir,
            &TestVolumeSession,
            &NoProgress,
        )
        .unwrap();
        assert_eq!(outcome.files, 2, "slave and igame.data");
    }

    /// **I2, from the install path.** A catalogued title over iGame's line
    /// length, with nothing else known about it, must not produce an empty
    /// `igame.data` counted as a written, verified file.
    #[test]
    fn a_catalogued_title_too_long_for_igame_writes_no_empty_file() {
        use crate::core::gameindex::readers::drawer::read_drawer;
        use crate::core::gameindex::record::{Fact, Provenance};
        use crate::core::gameindex::store::{CachedEntry, CatalogueRoot, CATALOGUE_SCHEMA};

        let (_scratch, dir) = scratch("igame-nothing-fits");
        let slave = crate::core::gameindex::readers::slave::tests_support::build_slave(
            "Turrican",
            "1992 Someone",
            16,
        );

        let archive = dir.join("Turrican.lha");
        std::fs::write(
            &archive,
            crate::core::lha::tests::make_lha_with(&[("Turrican/Turrican.slave", &slave)]),
        )
        .unwrap();

        let probe_dir = dir.join("probe").join("Turrican");
        std::fs::create_dir_all(&probe_dir).unwrap();
        std::fs::write(probe_dir.join("Turrican.slave"), &slave).unwrap();
        let fresh = read_drawer(&probe_dir).unwrap().expect("this is a title");

        let catalogue_dir = dir.join("catalogue");
        let collection_root = dir.join("collection");
        std::fs::create_dir_all(&collection_root).unwrap();
        let root_key = collection_root.to_string_lossy().into_owned();
        crate::core::gameindex::store::add_root(&catalogue_dir, Path::new(&root_key)).unwrap();
        let mut record = fresh.clone();
        record.title = Fact::new("T".repeat(80), Provenance::UserEdit);
        record.year = None;
        crate::core::gameindex::store::write_root(
            &catalogue_dir,
            &CatalogueRoot {
                schema: CATALOGUE_SCHEMA,
                root: root_key,
                scanned_at: None,
                index_schema: crate::core::gameindex::record::GAMEINDEX_SCHEMA,
                entries: vec![CachedEntry {
                    path: probe_dir
                        .join("Turrican.slave")
                        .to_string_lossy()
                        .into_owned(),
                    size: 0,
                    mtime_ms: 0,
                    record,
                }],
            },
        )
        .unwrap();

        let image = dir.join("Games.hdf");
        let (bytes, _) = ffs_volume(1760, DosType::new(*b"DOS\x01"));
        std::fs::write(&image, &bytes).unwrap();

        let outcome = install_pack(
            &archive,
            &image,
            0,
            0,
            &std::env::temp_dir(),
            &catalogue_dir,
            &TestVolumeSession,
            &NoProgress,
        )
        .unwrap();

        assert_eq!(
            outcome.files, 1,
            "the slave only — no igame.data with nothing in it counted as a file"
        );
        assert!(
            outcome.igame_omitted.iter().any(|o| o.contains("title")),
            "what did not fit must be named: {:?}",
            outcome.igame_omitted
        );
    }

    /// A refusal (a pack that is not WHDLoad) refuses before any write — the
    /// core function's own guarantee, independent of whichever `VolumeSession`
    /// it is given: an archive with no slave never reaches `session` at all.
    #[test]
    fn a_non_whdload_pack_is_refused_before_any_write() {
        use crate::core::lha::tests::make_lha_with;

        let (_scratch, dir) = scratch("e2e-not-whd");
        let archive = dir.join("Docs.lha");
        std::fs::write(
            &archive,
            make_lha_with(&[("Docs/readme.txt", b"just some documents")]),
        )
        .unwrap();

        let image = dir.join("Games.hdf");
        let (bytes, _) = ffs_volume(1760, DosType::new(*b"DOS\x01"));
        std::fs::write(&image, &bytes).unwrap();
        let before = std::fs::read(&image).unwrap();

        // Not a fault: the plan builds fine and reports why ART will not
        // install it.
        let plan = build_plan(&archive, &image, 0, 0, &std::env::temp_dir()).unwrap();
        assert!(
            plan.refusal.is_some(),
            "an archive with no slave must be refused, not errored"
        );
        assert!(install_pack(
            &archive,
            &image,
            0,
            0,
            &std::env::temp_dir(),
            &std::env::temp_dir(),
            &TestVolumeSession,
            &NoProgress
        )
        .is_err());
        assert_eq!(
            std::fs::read(&image).unwrap(),
            before,
            "a refused install must never reach the session at all"
        );
    }

    /// The split this task exists for: a plan that cannot find a pack is a
    /// refusal (data, `Ok` with `refusal` set, no `ART-*` identifier), while an
    /// archive ART genuinely cannot read is a fault (`Err`, with one).
    #[test]
    fn a_missing_pack_is_a_refusal_and_a_broken_archive_is_still_an_error() {
        let (_scratch, dir) = scratch("split");

        let ordinary = dir.join("Docs.lha");
        std::fs::write(
            &ordinary,
            crate::core::lha::tests::make_lha_with(&[("Docs/readme.txt", b"just documents")]),
        )
        .unwrap();

        let image = dir.join("Games.hdf");
        let (bytes, _) = ffs_volume(1760, DosType::new(*b"DOS\x01"));
        std::fs::write(&image, &bytes).unwrap();

        let plan = build_plan(&ordinary, &image, 0, 0, &std::env::temp_dir())
            .expect("a plan without a pack is still a plan, not an error");
        let refusal = plan
            .refusal
            .expect("no .slave in the archive must be reported as a refusal");
        assert!(refusal.reason.contains("not a WHDLoad pack"), "{refusal:?}");
        assert!(
            !refusal.reason.contains("ART-"),
            "a refusal carries no error identifier: {refusal:?}"
        );
        assert_eq!(
            refusal.suggestion.as_deref(),
            Some("You can still copy it by hand from the Files screen.")
        );

        let broken = dir.join("Broken.lha");
        std::fs::write(&broken, b"not an lha file at all").unwrap();

        let err = build_plan(&broken, &image, 0, 0, &std::env::temp_dir())
            .expect_err("an unreadable archive must still be a real error");
        assert!(
            matches!(err, CoreError::Malformed { .. }),
            "expected a malformed-archive error, got {err:?}"
        );
    }

    /// A pack that will not fit is refused with the numbers, before the disk
    /// is touched.
    #[test]
    fn a_pack_too_big_for_the_disk_is_refused_with_the_numbers() {
        use crate::core::lha::tests::make_lha_with;

        let (_scratch, dir) = scratch("e2e-toobig");
        let archive = dir.join("Huge.lha");
        std::fs::write(
            &archive,
            make_lha_with(&[
                ("Huge/Huge.slave", b"WHDLOADSLAVE"),
                ("Huge/Huge", b"host"),
                ("Huge/data/blob.bin", &vec![3u8; 1_400_000]),
                ("Huge.info", b"icon"),
            ]),
        )
        .unwrap();

        let image = dir.join("Games.hdf");
        let (bytes, _) = ffs_volume(1760, DosType::new(*b"DOS\x01"));
        std::fs::write(&image, &bytes).unwrap();
        let before = std::fs::read(&image).unwrap();

        let plan = build_plan(&archive, &image, 0, 0, &std::env::temp_dir()).unwrap();
        let refusal = plan
            .refusal
            .expect("a pack that does not fit must be refused");
        assert!(refusal.reason.contains("blocks"), "{refusal:?}");
        assert!(refusal.reason.contains("are free"), "{refusal:?}");

        assert!(install_pack(
            &archive,
            &image,
            0,
            0,
            &std::env::temp_dir(),
            &std::env::temp_dir(),
            &TestVolumeSession,
            &NoProgress
        )
        .is_err());
        assert_eq!(std::fs::read(&image).unwrap(), before);
    }

    /// The pack's own block cost, **without** the fix's +2 reservation —
    /// mirrors `build_plan` up to (not including) the line under test, so it
    /// gives an answer that does not move when that line is mutated away.
    fn raw_pack_cost(archive: &Path, image: &Path) -> CopyPlan {
        let info = open_archive(archive).unwrap();
        let _ = detect_whdload(&info.entries);
        let (scratch, _) = unpack_for_install(archive, &std::env::temp_dir(), &NoProgress).unwrap();
        let entries = walk(scratch.path()).unwrap();
        let layout = analyse(&entries).unwrap();
        let pack_root = if layout.root.is_empty() {
            scratch.path().to_path_buf()
        } else {
            crate::core::security::path::safe_join(scratch.path(), &layout.root).unwrap()
        };
        let folder = HostFolder::new(&pack_root, true);
        let mut sources: Vec<SourceEntry> = folder.entries().unwrap();
        sources.push(SourceEntry {
            relative: layout.name.clone(),
            is_dir: true,
            bytes: 0,
        });
        if let Some(icon) = &layout.icon {
            sources.push(SourceEntry {
                relative: layout.icon_name(),
                is_dir: false,
                bytes: std::fs::metadata(scratch.path().join(icon))
                    .map(|meta| meta.len())
                    .unwrap_or(0),
            });
        }
        let entry = pick_volume(image, 0).unwrap();
        let (device, geometry) = mount(image, &entry).unwrap();
        plan_copy(&device, &geometry, &sources, &[]).unwrap()
    }

    /// **M2.** `igame.data` is written into the pack's own drawer, but
    /// `build_plan`'s cost never measured it. Finds the exact volume size
    /// whose free space matches the pack's own **raw** cost — enough for the
    /// archive's own files, and not one block more — and asserts `build_plan`
    /// still refuses it.
    #[test]
    fn the_free_space_check_reserves_room_for_igame_data_too() {
        let (_scratch, dir) = scratch("m2-margin");
        let archive = dir.join("Turrican.lha");
        whdload_archive(&archive);
        let image = dir.join("Games.hdf");

        let (bytes, _) = ffs_volume(1760, DosType::new(*b"DOS\x01"));
        std::fs::write(&image, &bytes).unwrap();
        let raw_needed = raw_pack_cost(&archive, &image).blocks_needed;

        let plan_at = |total_blocks: u32| -> WhdloadPlan {
            let (bytes, _) = ffs_volume(total_blocks, DosType::new(*b"DOS\x01"));
            std::fs::write(&image, &bytes).unwrap();
            build_plan(&archive, &image, 0, 0, &std::env::temp_dir()).unwrap()
        };

        let (mut lo, mut hi) = (8u32, 1760u32);
        while lo + 1 < hi {
            let mid = lo + (hi - lo) / 2;
            if plan_at(mid).cost.blocks_free >= raw_needed {
                hi = mid;
            } else {
                lo = mid;
            }
        }
        let exact = plan_at(hi);
        assert_eq!(
            exact.cost.blocks_free, raw_needed,
            "expected the search to land exactly on the raw need (free space moves by one \
             block per total_blocks near this size)"
        );
        assert!(
            exact.refusal.is_some(),
            "there is room for the pack's own files and nothing else — the two-block \
             igame.data reservation is the only thing that should refuse this: {exact:?}"
        );
        assert!(exact.refusal.as_ref().unwrap().reason.contains("blocks"));
    }

    /// Installing the same game twice must not silently write over the first
    /// one — and the refusal has to arrive before anything is touched.
    #[test]
    fn installing_the_same_pack_twice_is_refused_and_changes_nothing() {
        let (_scratch, dir) = scratch("e2e-twice");
        let archive = dir.join("Turrican.lha");
        whdload_archive(&archive);

        let image = dir.join("Games.hdf");
        let (bytes, _) = ffs_volume(1760, DosType::new(*b"DOS\x01"));
        std::fs::write(&image, &bytes).unwrap();

        install_pack(
            &archive,
            &image,
            0,
            0,
            &std::env::temp_dir(),
            &std::env::temp_dir(),
            &TestVolumeSession,
            &NoProgress,
        )
        .unwrap();
        let after_first = std::fs::read(&image).unwrap();

        let err = install_pack(
            &archive,
            &image,
            0,
            0,
            &std::env::temp_dir(),
            &std::env::temp_dir(),
            &TestVolumeSession,
            &NoProgress,
        )
        .unwrap_err();
        assert!(err.to_string().contains("already on that volume"), "{err}");
        assert_eq!(
            std::fs::read(&image).unwrap(),
            after_first,
            "a refused install must leave the image byte-for-byte unchanged"
        );
    }

    /// The leftover this batch fixes: `install_pack` used to call
    /// `build_plan` to re-plan against the disk's current state, which
    /// unpacked the archive into its own scratch directory and then threw it
    /// away, and then unpacked the same archive a *second* time to actually
    /// copy from. `build_plan_with_scratch` now hands that first unpack back
    /// instead of discarding it, so a single install unpacks the archive
    /// exactly once.
    ///
    /// Proved by counting, not by reading the source: `extract_with_backend`
    /// (`core/archive/extract.rs`) reports the archive's own entry name for
    /// every file it writes to the scratch directory, and this fixture's
    /// slave is named uniquely within it (`Turrican/Turrican.slave`) — a name
    /// `copy_into_volume` never reports, since it reports paths relative to
    /// the drawer's own root instead (`Turrican.slave`, no prefix; see the
    /// cancel-sink tests above for the same distinction put to a different
    /// use). Counting how many times that one message is reported counts how
    /// many times the archive was actually unpacked to disk, independent of
    /// `pick_volume`/`mount`'s own bookkeeping or anything else `install_pack`
    /// happens to report along the way.
    ///
    /// **Mutation:** re-inserting the removed second extraction (`let (scratch,
    /// _) = unpack_for_install(archive, scratch_root, sink)?;` right after the
    /// plan, mirroring the code this replaced) makes the count 2 and this
    /// assertion fall.
    #[test]
    fn install_pack_unpacks_the_archive_exactly_once() {
        use std::sync::atomic::{AtomicUsize, Ordering};

        struct CountUnpacks(AtomicUsize);
        impl ProgressSink for CountUnpacks {
            fn report(&self, _done: u64, _total: Option<u64>, message: &str) {
                if message == "Turrican/Turrican.slave" {
                    self.0.fetch_add(1, Ordering::SeqCst);
                }
            }
            fn is_cancelled(&self) -> bool {
                false
            }
        }

        let (_scratch, dir) = scratch("count-unpack");
        let archive = dir.join("Turrican.lha");
        whdload_archive(&archive);

        let image = dir.join("Games.hdf");
        let (bytes, _) = ffs_volume(1760, DosType::new(*b"DOS\x01"));
        std::fs::write(&image, &bytes).unwrap();

        let sink = CountUnpacks(AtomicUsize::new(0));
        install_pack(
            &archive,
            &image,
            0,
            0,
            &std::env::temp_dir(),
            &std::env::temp_dir(),
            &TestVolumeSession,
            &sink,
        )
        .unwrap();

        assert_eq!(
            sink.0.load(Ordering::SeqCst),
            1,
            "the archive must be unpacked exactly once per install"
        );
    }

    /// §54/§57, and the data-safety rule at the core level: cancelling
    /// one-click WHDLoad install must install *nothing*, and a failure
    /// part-way through the copy must leave the image byte-for-byte
    /// unchanged — hashed (compared byte for byte) before and after.
    ///
    /// `copy_into_volume` stops between files and reports how many landed,
    /// which is right for a general-purpose copy — an install is not that.
    /// Half a WHDLoad pack is a game that will not start, and reporting it as
    /// a finished install would be worse than any error. This drives
    /// `install_pack` itself, not `copy_into_volume` and not
    /// `TestVolumeSession`'s own commit step — deleting the guard inside
    /// `TestVolumeSession::install_drawer` makes this test fail rather than
    /// leaving it trivially green (see the mutation note on `install_pack`).
    ///
    /// **`StopDuringCopy` anchors on the copy phase's own message, not on
    /// ordinal position** (leftover from ART-242 fix round 1). `install_pack`
    /// runs two per-entry loops that report `Some(total)`: unpacking the
    /// archive (`extract_with_backend`, `core/archive/extract.rs`) and then
    /// copying into the volume (`copy_into_volume`, `core/volume/write/copy.rs`).
    /// The first version of this test armed on a bare `done >= N` and cancelled
    /// during the **first** such phase every time, since unpacking always runs
    /// first — a survivor that never actually reached `TestVolumeSession` at
    /// all, so a guard removed there changed nothing. Counting phase
    /// boundaries by ordinal ("the second total-bearing phase") fixed that,
    /// but the ordinal itself is borrowed knowledge: a third total-bearing
    /// phase inserted before the copy would silently become "the second
    /// phase" and move the cancel there instead, with nothing here noticing.
    ///
    /// So the sink now arms on `copy_into_volume`'s own message shape instead:
    /// it reports each entry's path *relative to the drawer's own root*
    /// (`entry.relative` — "Turrican.slave", not "Turrican/Turrican.slave"),
    /// while the unpack phase reports the archive's own entry names, which for
    /// this fixture always carry the drawer name as a leading path segment.
    /// "Turrican.slave" bare is a message only `copy_into_volume` can produce.
    /// The ordinal phase counter stays, purely as an independent witness: it
    /// records which phase was current when the message-based arm fired, and
    /// the assertion below requires that to be phase 2 — so a phase inserted
    /// ahead of the copy, or a wrong anchor, fails loudly instead of quietly
    /// moving where the cancel lands.
    #[test]
    fn a_cancelled_install_writes_nothing_and_does_not_report_success() {
        use crate::core::lha::tests::make_lha_with;
        use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

        struct StopDuringCopy {
            phase: AtomicUsize,
            armed_phase: AtomicUsize,
            cancel: AtomicBool,
        }
        impl ProgressSink for StopDuringCopy {
            fn report(&self, done: u64, total: Option<u64>, message: &str) {
                if total.is_none() {
                    return;
                }
                if done == 0 {
                    self.phase.fetch_add(1, Ordering::SeqCst);
                }
                // The intrinsic anchor: `copy_into_volume` is the only phase
                // that ever reports this drawer-relative, prefix-free name
                // (see the doc comment above). `armed_phase` is recorded once,
                // the first time it fires, so a later report cannot overwrite
                // which phase actually triggered the arm.
                if message == "Turrican.slave" {
                    let _ = self.armed_phase.compare_exchange(
                        0,
                        self.phase.load(Ordering::SeqCst),
                        Ordering::SeqCst,
                        Ordering::SeqCst,
                    );
                }
                if self.armed_phase.load(Ordering::SeqCst) != 0 {
                    self.cancel.store(true, Ordering::SeqCst);
                }
            }
            fn is_cancelled(&self) -> bool {
                self.cancel.load(Ordering::SeqCst)
            }
        }

        let (_scratch, dir) = scratch("e2e-cancel");
        let archive = dir.join("Turrican.lha");
        let slave = b"WHDLOADSLAVE\x00\x00\x00\x0a";
        let mut entries: Vec<(String, Vec<u8>)> = vec![
            ("Turrican/Turrican.slave".into(), slave.to_vec()),
            (
                "Turrican/Turrican".into(),
                b"host executable bytes".to_vec(),
            ),
            ("Turrican/data/level1.bin".into(), vec![7u8; 4000]),
        ];
        for index in 0..6 {
            entries.push((
                format!("Turrican/Extra{index}.dat"),
                vec![b'a' + index as u8; 64],
            ));
        }
        entries.push(("Turrican.info".into(), b"\xe3\x10\x00\x01icon".to_vec()));
        let borrowed: Vec<(&str, &[u8])> = entries
            .iter()
            .map(|(name, data)| (name.as_str(), data.as_slice()))
            .collect();
        std::fs::write(&archive, make_lha_with(&borrowed)).unwrap();

        let image = dir.join("Games.hdf");
        let (bytes, _) = ffs_volume(1760, DosType::new(*b"DOS\x01"));
        std::fs::write(&image, &bytes).unwrap();
        let before = std::fs::read(&image).unwrap();

        let sink = StopDuringCopy {
            phase: AtomicUsize::new(0),
            armed_phase: AtomicUsize::new(0),
            cancel: AtomicBool::new(false),
        };
        let err = install_pack(
            &archive,
            &image,
            0,
            0,
            &std::env::temp_dir(),
            &std::env::temp_dir(),
            &TestVolumeSession,
            &sink,
        )
        .expect_err("a cancelled install must not come back as a successful one");

        assert_eq!(
            err.code(),
            "ART-CANCELLED",
            "the job must end Cancelled, not Completed: {err}"
        );
        assert_eq!(
            std::fs::read(&image).unwrap(),
            before,
            "a cancelled/mid-way-failed install must leave the image byte-for-byte unchanged"
        );
        assert_eq!(
            sink.armed_phase.load(Ordering::SeqCst),
            2,
            "the cancel must fire during the copy phase (phase 2), not wherever the anchor \
             happened to match — a phase inserted before the copy must move this number, not \
             the cancel point"
        );
    }

    /// Install a pack and leave the disk for `scripts/oracle-check.py`.
    #[test]
    fn export_whdload_install_for_oracle_when_asked() {
        let Ok(dest) = std::env::var("ART_WHD_OUT") else {
            return;
        };
        let dest = PathBuf::from(dest);

        let dir = dest.parent().unwrap_or(Path::new(".")).to_path_buf();
        std::fs::create_dir_all(&dir).unwrap();

        let archive = dir.join("art-whd-oracle.lha");
        whdload_archive(&archive);

        let (bytes, _) = ffs_volume(1760, DosType::new(*b"DOS\x01"));
        std::fs::write(&dest, &bytes).unwrap();

        install_pack(
            &archive,
            &dest,
            0,
            0,
            &std::env::temp_dir(),
            &std::env::temp_dir(),
            &TestVolumeSession,
            &NoProgress,
        )
        .unwrap();
        let _ = std::fs::remove_file(&archive);
    }
}
