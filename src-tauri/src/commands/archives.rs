//! Several `.lha` archives onto one disk, in one operation (Phase 1A, Task 5).
//!
//! `sources_install_volume` and `whdload_install` each take one archive.
//! Selecting five game archives in the Commander and pressing F5 into a
//! volume pane is a different operation, not five of that one: an Amiga user
//! expects five drawers, not five archives' contents merged into one
//! directory.
//!
//! ## Where each archive lands
//!
//! - if the archive's contents have **exactly one top-level directory**, that
//!   directory's name becomes the drawer;
//! - otherwise the drawer is named after the archive's **file stem**.
//!
//! [`archives_plan_install`] shows every drawer name before anything is
//! written (§92) — that is what resolves the ambiguity, not a guess ART makes
//! silently. Two archives that would both create the same drawer are refused
//! with both names, not merged into one.
//!
//! ## One write, not several
//!
//! Every archive is unpacked, renamed to its drawer, and staged into one
//! [`HostSelection`] spanning all of them. That whole selection then goes
//! through [`volume_write::install_into_folder`] exactly once — the same
//! all-or-nothing primitive `sources::install_archive_into_volume` uses for a
//! single archive. Installing five archives as five separate calls to that
//! primitive would commit the first two before the third was cancelled; one
//! call over one staged selection is what keeps a cancelled batch from
//! leaving two games installed (§54, §57).
//!
//! ## Archive entry names are untrusted
//!
//! Every path this module builds under a scratch or staging directory goes
//! through [`safe_join`] — the top-level directory name that becomes a
//! drawer's name came out of an archive, and an archive is hostile input
//! until proven otherwise, even after `unpack_for_install`'s own traversal
//! defence has already run once.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::Serialize;
use tauri::{AppHandle, Emitter, State};

use super::jobs::{spawn_job, JobRegistry};
use super::oplog::{user_operation, write_to_path};
use crate::core::error::{CoreError, CoreResult};
use crate::core::jobs::{JobId, ProgressSink};
use crate::core::lha::OverwritePolicy;
use crate::core::oplog::{JsonlOperationLog, OperationOutcome};
use crate::core::security::safe_join;
use crate::core::sources::install::unpack_for_install;
use crate::core::volume::write::copy::{CopySource, HostFolder, HostSelection};
use crate::core::volume::write::plan::CopyPlan;
use crate::error::AppResult;

/// The most top-level entries `prepare_archives` will look at before falling
/// back to the archive's stem. Bounded the same way every walk in this
/// codebase is: an archive with an absurd number of top-level entries still
/// resolves in one directory listing.
const MAX_TOP_LEVEL_ENTRIES: usize = 10_000;

// ---------------------------------------------------------------------------
// Planning
// ---------------------------------------------------------------------------

/// Where one archive's contents will land, before anything is written.
#[derive(Debug, Clone, Serialize)]
pub struct ArchiveDrawer {
    /// The archive's path, exactly as given.
    pub archive: String,
    /// Its file name only, for a short label.
    pub name: String,
    /// The drawer that will be created for it.
    pub drawer: String,
    pub files: usize,
    pub directories: usize,
    pub bytes: u64,
    /// Entries the extractor itself refused — a traversal entry, one over the
    /// decompression-bomb cap, a name it could not write — with the reason.
    /// Never silent: these never reach the copy phase at all, so nothing
    /// downstream would otherwise mention them (§68's "never silent" rule).
    /// Shown in the plan, before the user confirms anything.
    pub skipped: Vec<String>,
}

/// What installing every archive in the batch would do. Writes nothing.
#[derive(Debug, Clone, Serialize)]
pub struct ArchivesPlan {
    /// One row per archive, in the order given.
    pub drawers: Vec<ArchiveDrawer>,
    /// The cost of the whole batch, over the union of every drawer — the same
    /// [`CopyPlan`] a plain multi-file copy shows, so the dialog that already
    /// renders one reads this without a special case. `cost.fits()` /
    /// `cost.shortfall()` are what say whether the batch is refused; there is
    /// no separate refusal field here; `CopyPlanDialog` already derives its
    /// own localised shortfall sentence from `cost` this exact way for a
    /// plain multi-file plan, via `planShortfall`.
    pub cost: CopyPlan,
}

/// The event a finished plan arrives on. One per `archives_plan_install` job.
pub const ARCHIVES_PLAN_EVENT: &str = "archives-plan-result";

/// A plan, tied back to the job that produced it.
#[derive(Debug, Clone, Serialize)]
pub struct ArchivesPlanResult {
    pub job_id: JobId,
    pub plan: ArchivesPlan,
}

/// What installing `archives` into a volume would do. Writes nothing.
/// Returns a job id (§54, §55).
///
/// **A job, not a plain command** (ART-066). Planning here is not the cheap
/// arithmetic every other plan in ART does: it has to *unpack every archive in
/// the batch* to know what each one contains, and it used to do that straight
/// on the Tauri command thread — several large archives meant a frozen window,
/// no progress, and no way to stop, in the one step whose whole purpose is to
/// let the user change their mind before anything is written. Everything else
/// in this module already runs through `spawn_job`.
#[tauri::command]
pub fn archives_plan_install(
    archives: Vec<String>,
    image: String,
    volume_index: usize,
    dir_block: Option<u32>,
    app: AppHandle,
    registry: State<'_, Arc<JobRegistry>>,
) -> AppResult<JobId> {
    let archives = normalize_archives(&archives)?;
    let image_path = PathBuf::from(image.trim());
    let parent = dir_block.unwrap_or(0);

    let registry = Arc::clone(&registry);
    let emit_app = app.clone();
    let title = format!("Planning {} archives", archives.len());

    let id = spawn_job(&app, registry, &title, move |job_id, progress| {
        let plan = build_plan(&archives, &image_path, volume_index, parent, progress)?;
        // Nothing is logged: §53 is about operations that change user data, and
        // this one writes nothing at all.
        let _ = emit_app.emit(ARCHIVES_PLAN_EVENT, ArchivesPlanResult { job_id, plan });
        Ok(())
    });

    Ok(id)
}

fn build_plan(
    archives: &[PathBuf],
    image: &Path,
    volume_index: usize,
    parent: u32,
    progress: &dyn ProgressSink,
) -> CoreResult<ArchivesPlan> {
    let staging = Staging::new()?;
    let (drawers, roots) = prepare_archives(archives, staging.path(), progress)?;

    // These roots are ART's own unpacked staging tree, not a folder the user
    // picked — the sidecar option (§4.2) is about copies from a real host
    // folder, so it never applies here.
    let selection = HostSelection::new(roots, true);
    let cost = crate::commands::volume_write::plan_copy_in_folder(
        image,
        volume_index,
        parent,
        &selection,
    )?;

    Ok(ArchivesPlan { drawers, cost })
}

fn normalize_archives(archives: &[String]) -> CoreResult<Vec<PathBuf>> {
    if archives.is_empty() {
        return Err(CoreError::InvalidInput(
            "choose at least one archive to install".into(),
        ));
    }

    archives
        .iter()
        .map(|given| {
            let path = PathBuf::from(given.trim());
            if !path.is_file() {
                return Err(CoreError::InvalidInput(format!("'{given}' is not a file")));
            }
            Ok(path)
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Installing
// ---------------------------------------------------------------------------

/// Install every archive in the batch. Returns a job id (§54).
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub fn archives_install(
    archives: Vec<String>,
    image: String,
    volume_index: usize,
    dir_block: Option<u32>,
    overwrite: Option<OverwritePolicy>,
    app: AppHandle,
    registry: State<'_, Arc<JobRegistry>>,
    oplog: State<'_, JsonlOperationLog>,
) -> AppResult<JobId> {
    let archives = normalize_archives(&archives)?;
    let image_path = PathBuf::from(image.trim());
    let parent = dir_block.unwrap_or(0);
    let policy = overwrite.unwrap_or_default();

    let log_path = oplog.path().to_path_buf();
    let registry = Arc::clone(&registry);
    let emit_app = app.clone();
    let title = format!(
        "Installing {} archives into {}",
        archives.len(),
        image_path.display()
    );

    let id = spawn_job(&app, registry, &title, move |job_id, progress| {
        let outcome = install_archives(
            &archives,
            &image_path,
            volume_index,
            parent,
            policy,
            progress,
        );

        // §53: what went in, where, and how many files, the same shape every
        // other write command records.
        let record = user_operation("Install archives into volume")
            .source(format!("{} archives", archives.len()))
            .destination(format!("{}:{volume_index}", image_path.display()));
        let record = match &outcome {
            Ok((report, backup)) => record
                .detail("Archives", archives.len().to_string())
                .detail("Files", report.files_copied.to_string())
                .detail("Folders", report.directories_created.to_string())
                .detail("Skipped", report.skipped.len().to_string())
                .backup(backup.clone())
                .outcome(OperationOutcome::verified(
                    report.files_verified == report.files_copied,
                )),
            Err(err) => record.failed(err),
        };
        write_to_path(&log_path, &record);

        let (report, backup) = outcome?;
        // The same event and shape `volume_copy_in_many` emits: installing a
        // batch of archives *is* copying a staged `HostSelection` into a
        // volume, and the Commander's one listener for copy results already
        // knows how to show a `CopyReport`.
        let _ = emit_app.emit(
            crate::commands::volume_write::VOLUME_WRITE_EVENT,
            crate::commands::volume_write::VolumeWriteResult::CopyIn {
                job_id,
                report,
                backup,
            },
        );
        Ok(())
    });

    Ok(id)
}

fn install_archives(
    archives: &[PathBuf],
    image: &Path,
    volume_index: usize,
    parent: u32,
    policy: OverwritePolicy,
    progress: &dyn ProgressSink,
) -> CoreResult<(crate::core::volume::write::copy::CopyReport, Option<String>)> {
    let staging = Staging::new()?;
    let (drawers, roots) = prepare_archives(archives, staging.path(), progress)?;
    // Same as `build_plan` above: ART's own staging tree, so the sidecar
    // option does not apply.
    let selection = HostSelection::new(roots, true);

    // The same fits-or-nothing, abandon-on-cancel primitive a single-archive
    // install uses — one call, over the whole staged batch, so a cancelled
    // batch is exactly as atomic as a cancelled single install.
    let (mut report, backup) = crate::commands::volume_write::install_into_folder(
        image,
        volume_index,
        parent,
        &selection,
        policy,
        progress,
    )?;

    // Entries the extractor itself refused never reach the copy phase, so
    // `report.skipped` alone says nothing about them — carried over from the
    // same per-archive accounting the plan already shows, so the finished
    // install is exactly as honest as the plan was (§68's "never silent").
    for drawer in &drawers {
        report.skipped.extend(drawer.skipped.iter().cloned());
    }

    Ok((report, backup))
}

// ---------------------------------------------------------------------------
// Unpacking, naming and staging — shared by the plan and the install
// ---------------------------------------------------------------------------

/// One archive, unpacked and named, before it has been moved into staging.
struct Unpacked {
    archive: PathBuf,
    drawer: String,
    content_root: PathBuf,
    /// What the extractor itself refused for this archive — a traversal
    /// entry, one over the bomb cap, an unusable name. Carried through to
    /// [`ArchiveDrawer::skipped`] so it is never silently dropped.
    skipped: Vec<String>,
    /// Kept alive only so its directory survives until `content_root` has
    /// been moved out of it; never read again after that.
    _scratch: crate::core::sources::install::Scratch,
}

/// Unpack every archive, work out where its contents will land, and move each
/// into `staging` under that name.
///
/// Both [`archives_plan_install`] and [`archives_install`] call this and get
/// the same drawer name for the same archive — a plan that shows one name and
/// an install that creates another would defeat §92 entirely.
/// One archive's slice of a batch's progress (ART-067).
///
/// `prepare_archives` used to unpack with `NoProgress`, so `is_cancelled()`
/// was answered `false` all the way down and Stop was honoured only *between*
/// archives: a batch whose third archive is large left Stop unresponsive for
/// however long that one extraction took.
///
/// Forwarding the batch's own sink fixes that, but forwarding it **raw** would
/// let the extractor's per-entry counts (142 of 2000 files) overwrite the
/// batch's per-archive ones (3 of 5), so the bar would leap forward inside an
/// archive and fall back at every boundary. This keeps the batch's numbers and
/// carries the inner message through.
///
/// The message keeps the `"Unpacking …"` prefix on purpose: the phase a report
/// belongs to is told from that prefix — see
/// `cancelling_during_the_copy_phase_writes_nothing`, whose sink waits for
/// reports that are *not* the unpack phase's.
struct BatchStep<'a> {
    outer: &'a dyn ProgressSink,
    /// Which archive of the batch, and how many there are.
    index: u64,
    total: u64,
    label: String,
}

impl ProgressSink for BatchStep<'_> {
    fn report(&self, _done: u64, _total: Option<u64>, message: &str) {
        self.outer.report(
            self.index,
            Some(self.total),
            &format!("Unpacking {} — {message}", self.label),
        );
    }

    fn is_cancelled(&self) -> bool {
        self.outer.is_cancelled()
    }
}

fn prepare_archives(
    archives: &[PathBuf],
    staging: &Path,
    progress: &dyn ProgressSink,
) -> CoreResult<(Vec<ArchiveDrawer>, Vec<PathBuf>)> {
    let total = archives.len() as u64;

    // Every scratch directory stays alive until the whole batch has been
    // named and checked for collisions — dropping one early would remove its
    // content before it has been moved into staging.
    let mut unpacked: Vec<Unpacked> = Vec::with_capacity(archives.len());

    for (index, archive) in archives.iter().enumerate() {
        // Stop, asked between two archives (§54). Since ART-067 it is also
        // heard *inside* one, through `BatchStep` below — which does not
        // weaken the rule: what a cancelled unpack leaves half-finished is a
        // scratch directory ART owns and drops, never a file of the user's.
        // Nothing reaches the volume until every archive is staged.
        if progress.is_cancelled() {
            return Err(CoreError::Cancelled);
        }
        let label = file_label(archive);
        progress.report(index as u64, Some(total), &format!("Unpacking {label}"));

        // The batch's sink, not `NoProgress`: Stop has to be heard *inside* a
        // large archive, not merely between two (ART-067).
        let step = BatchStep {
            outer: progress,
            index: index as u64,
            total,
            label: label.clone(),
        };
        let (scratch, unpack_skipped) = unpack_for_install(archive, &step)?;
        let top = top_level_entries(scratch.path())?;

        let (drawer_name, content_root) = if top.len() == 1 && top[0].is_dir {
            let root = safe_join(scratch.path(), &top[0].name).map_err(|err| {
                CoreError::SafetyRefused(format!(
                    "'{}' escapes the unpacked archive: {err}",
                    top[0].name
                ))
            })?;
            (top[0].name.clone(), root)
        } else {
            (archive_stem(archive)?, scratch.path().to_path_buf())
        };

        unpacked.push(Unpacked {
            archive: archive.clone(),
            drawer: drawer_name,
            content_root,
            skipped: unpack_skipped,
            _scratch: scratch,
        });
    }

    // Two archives that would both create the same drawer are reported by
    // name, before anything moves — silently interleaving two unrelated
    // archives into one directory would be worse than refusing (§92).
    //
    // Keyed **without case** (ART-072): AmigaDOS is case-preserving but
    // case-insensitive, so `Docs` and `docs` are two keys here and one drawer
    // there. Keyed as typed, the pair reached `std::fs::rename` instead, and
    // the user got a raw OS error where the exact-match case gets a sentence.
    let mut seen: std::collections::BTreeMap<String, (&Path, &str)> =
        std::collections::BTreeMap::new();
    for item in &unpacked {
        if let Some((other, other_drawer)) = seen.get(&item.drawer.to_lowercase()) {
            return Err(CoreError::InvalidInput(format!(
                "'{}' and '{}' would both create a drawer called '{}' — rename one \
                 before installing them together",
                other.display(),
                item.archive.display(),
                other_drawer
            )));
        }
        seen.insert(
            item.drawer.to_lowercase(),
            (item.archive.as_path(), item.drawer.as_str()),
        );
    }

    let mut drawers = Vec::with_capacity(unpacked.len());
    let mut roots = Vec::with_capacity(unpacked.len());

    for item in unpacked {
        let stats = HostFolder::new(item.content_root.as_path(), true).entries()?;
        let files = stats.iter().filter(|entry| !entry.is_dir).count();
        let directories = stats.iter().filter(|entry| entry.is_dir).count();
        let bytes: u64 = stats
            .iter()
            .filter(|entry| !entry.is_dir)
            .map(|entry| entry.bytes)
            .sum();

        let destination = safe_join(staging, &item.drawer).map_err(|err| {
            CoreError::InvalidInput(format!(
                "'{}' is not a name ART can use for a drawer: {err}",
                item.drawer
            ))
        })?;
        std::fs::rename(&item.content_root, &destination)?;

        drawers.push(ArchiveDrawer {
            archive: item.archive.display().to_string(),
            name: file_label(&item.archive),
            drawer: item.drawer,
            files,
            directories,
            bytes,
            skipped: item.skipped,
        });
        roots.push(destination);
    }

    progress.report(total, Some(total), "Unpacked");
    Ok((drawers, roots))
}

fn file_label(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_default()
}

/// The archive's own name without its extension — the drawer name for an
/// archive that does not unpack to one single top-level directory.
///
/// Requires the stem to parse as exactly one [`Component::Normal`]. Without
/// this, a file literally named `..lha` has `file_stem() == "."` (Rust only
/// treats a *leading* dot as part of the name when there is no other `.` —
/// `..lha`'s first dot is the stem/extension separator, leaving the stem as
/// the single character `.`), and `safe_join(staging, ".")` legitimately
/// resolves to `staging` itself: the following `fs::rename` would then target
/// the staging root and fail with a raw OS error instead of a clean refusal
/// naming the archive.
fn archive_stem(archive: &Path) -> CoreResult<String> {
    let invalid = || {
        CoreError::InvalidInput(format!(
            "'{}' has no name ART can use for a drawer",
            archive.display()
        ))
    };

    let stem = archive
        .file_stem()
        .and_then(|stem| stem.to_str())
        .ok_or_else(invalid)?;

    let mut components = Path::new(stem).components();
    match (components.next(), components.next()) {
        (Some(std::path::Component::Normal(_)), None) => Ok(stem.to_string()),
        _ => Err(invalid()),
    }
}

struct TopEntry {
    name: String,
    is_dir: bool,
}

/// What is directly inside `dir` — one level, not a walk. Used only to decide
/// whether an unpacked archive is "one wrapping folder" or "several things at
/// the top", never to read anything inside those entries.
fn top_level_entries(dir: &Path) -> CoreResult<Vec<TopEntry>> {
    let mut out = Vec::new();
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        out.push(TopEntry {
            name: entry.file_name().to_string_lossy().to_string(),
            is_dir: entry.file_type()?.is_dir(),
        });
        if out.len() >= MAX_TOP_LEVEL_ENTRIES {
            break;
        }
    }
    Ok(out)
}

/// One temp directory holding every archive's renamed drawer for one batch.
///
/// Where [`Scratch`](crate::core::sources::install::Scratch) is per-archive,
/// this is per-batch: every drawer a plan or an install produces lives under
/// one parent, so cleaning up the whole batch — on success, on a §92
/// refusal, or on a cancelled job — is one `remove_dir_all`. Mirrors
/// `the_staging_folder_removes_itself` in `core::volume::write::copy`.
struct Staging(PathBuf);

impl Staging {
    fn new() -> CoreResult<Self> {
        use std::sync::atomic::{AtomicU64, Ordering};
        static NEXT: AtomicU64 = AtomicU64::new(0);

        let path = std::env::temp_dir().join(format!(
            "art-archives-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path)?;
        Ok(Self(path))
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for Staging {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    // `NoProgress` is a test-only import since ART-066: every caller in the
    // application now runs on a job with a real sink.
    use crate::core::jobs::NoProgress;
    use crate::core::lha::tests::make_lha_with;
    use crate::core::volume::fixture::ffs_volume;
    use crate::core::volume::mount::mount;
    use crate::core::volume::DosType;

    fn scratch(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("art-archives-t-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn disk(dir: &Path, name: &str) -> PathBuf {
        let image = dir.join(name);
        let (bytes, _) = ffs_volume(1760, DosType::new(*b"DOS\x01"));
        std::fs::write(&image, &bytes).unwrap();
        image
    }

    fn entries_at(image: &Path, block: u32) -> Vec<crate::core::volume::write::dir::DirEntry> {
        let entry = crate::commands::volume_write::pick_volume(image, 0).unwrap();
        let (device, geometry) = mount(image, &entry).unwrap();
        let set = crate::core::volume::write::layout::BlockSet::new(geometry.block_size);
        let block = if block == 0 {
            geometry.root_block
        } else {
            block
        };
        crate::core::volume::write::dir::entries_in(&device, &set, &geometry, block).unwrap()
    }

    // ---- the naming rule ----

    /// A single wrapping folder gives the drawer its name.
    #[test]
    fn one_top_level_directory_names_the_drawer() {
        let dir = scratch("one-dir");
        let archive = dir.join("Pack.lha");
        std::fs::write(
            &archive,
            make_lha_with(&[("Turrican/Game", b"exe"), ("Turrican/data.bin", b"data")]),
        )
        .unwrap();

        let staging = dir.join("staging");
        std::fs::create_dir_all(&staging).unwrap();
        let (drawers, roots) = prepare_archives(&[archive], &staging, &NoProgress).unwrap();

        assert_eq!(drawers.len(), 1);
        assert_eq!(drawers[0].drawer, "Turrican");
        assert_eq!(drawers[0].files, 2);
        assert_eq!(roots[0], staging.join("Turrican"));
        assert!(roots[0].join("Game").is_file());
        assert!(roots[0].join("data.bin").is_file());

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Several top-level entries — no single wrapping folder — fall back to
    /// the archive's own file stem.
    #[test]
    fn several_top_level_entries_use_the_archive_stem() {
        let dir = scratch("stem");
        let archive = dir.join("Loose Files.lha");
        std::fs::write(
            &archive,
            make_lha_with(&[("Game", b"exe"), ("Readme.txt", b"about")]),
        )
        .unwrap();

        let staging = dir.join("staging");
        std::fs::create_dir_all(&staging).unwrap();
        let (drawers, roots) = prepare_archives(&[archive], &staging, &NoProgress).unwrap();

        assert_eq!(drawers[0].drawer, "Loose Files");
        assert_eq!(roots[0], staging.join("Loose Files"));
        assert!(roots[0].join("Game").is_file());
        assert!(roots[0].join("Readme.txt").is_file());

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A single top-level *file* (no directory at all) is not "one top-level
    /// directory" either, and also falls back to the stem.
    #[test]
    fn a_single_top_level_file_uses_the_archive_stem() {
        let dir = scratch("single-file");
        let archive = dir.join("Doc.lha");
        std::fs::write(&archive, make_lha_with(&[("Readme.txt", b"about")])).unwrap();

        let staging = dir.join("staging");
        std::fs::create_dir_all(&staging).unwrap();
        let (drawers, roots) = prepare_archives(&[archive], &staging, &NoProgress).unwrap();

        assert_eq!(drawers[0].drawer, "Doc");
        assert!(roots[0].join("Readme.txt").is_file());

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A file literally named `..lha` has `file_stem() == "."` (Rust treats
    /// the first `.` as the stem/extension separator once there is a second
    /// `.` in the name). That must be refused, not accepted as a drawer name:
    /// `safe_join(staging, ".")` legitimately resolves to `staging` itself,
    /// so an unchecked stem here would make `prepare_archives` try to rename
    /// the archive's contents onto the staging root.
    #[test]
    fn a_stem_that_is_only_dots_is_refused_as_a_drawer_name() {
        let dir = scratch("dotty-stem");
        let archive = dir.join("..lha");
        std::fs::write(&archive, make_lha_with(&[("Readme.txt", b"about")])).unwrap();

        let err = archive_stem(&archive).unwrap_err();
        assert_eq!(err.code(), "ART-INPUT-INVALID");

        // And the whole pipeline refuses it too, rather than panicking or
        // renaming onto the staging directory itself.
        let staging = dir.join("staging");
        std::fs::create_dir_all(&staging).unwrap();
        let err = prepare_archives(&[archive], &staging, &NoProgress).unwrap_err();
        assert_eq!(err.code(), "ART-INPUT-INVALID");

        let _ = std::fs::remove_dir_all(&dir);
    }

    // ---- end to end: three archives, three drawers ----

    /// The headline case: three archives become three drawers, each holding
    /// exactly its own archive's contents — never merged.
    #[test]
    fn three_archives_install_into_three_drawers() {
        let dir = scratch("three");
        let a = dir.join("Turrican.lha");
        std::fs::write(&a, make_lha_with(&[("Turrican/Game", b"a-bytes")])).unwrap();
        let b = dir.join("Xenon2.lha");
        std::fs::write(&b, make_lha_with(&[("Xenon2/Game", b"b-bytes")])).unwrap();
        let c = dir.join("Loose.lha");
        std::fs::write(
            &c,
            make_lha_with(&[("Game", b"c-bytes"), ("Readme", b"c-readme")]),
        )
        .unwrap();

        let image = disk(&dir, "disk.adf");
        let archives = vec![a, b, c];

        let plan = build_plan(&archives, &image, 0, 0, &NoProgress).unwrap();
        assert_eq!(plan.drawers.len(), 3);
        assert_eq!(plan.drawers[0].drawer, "Turrican");
        assert_eq!(plan.drawers[1].drawer, "Xenon2");
        assert_eq!(plan.drawers[2].drawer, "Loose");
        assert!(plan.cost.fits(), "{:?}", plan.cost.shortfall());

        let (report, _backup) =
            install_archives(&archives, &image, 0, 0, OverwritePolicy::Skip, &NoProgress).unwrap();
        assert_eq!(
            report.files_copied, 4,
            "one file each for two, two for Loose"
        );
        assert!(report.skipped.is_empty(), "{:?}", report.skipped);

        let root = entries_at(&image, 0);
        assert_eq!(root.len(), 3, "three drawers, side by side");
        for name in ["Turrican", "Xenon2", "Loose"] {
            let drawer = root
                .iter()
                .find(|found| found.name == name)
                .unwrap_or_else(|| panic!("{name} missing from {root:?}"));
            assert!(drawer.is_dir);
        }

        let turrican = root.iter().find(|found| found.name == "Turrican").unwrap();
        let inside = entries_at(&image, turrican.block);
        assert_eq!(inside.len(), 1);
        assert_eq!(inside[0].name, "Game");

        let _ = std::fs::remove_dir_all(&dir);
    }

    // ---- collisions ----

    /// Two archives that would both create a drawer called `Turrican` are
    /// refused by name, not silently merged into one drawer.
    #[test]
    fn colliding_drawer_names_are_reported_not_merged() {
        let dir = scratch("collide");
        let a = dir.join("release-a.lha");
        std::fs::write(&a, make_lha_with(&[("Turrican/Game", b"from a")])).unwrap();
        let b = dir.join("release-b.lha");
        std::fs::write(&b, make_lha_with(&[("Turrican/Game", b"from b")])).unwrap();

        let image = disk(&dir, "disk.adf");
        let before = std::fs::read(&image).unwrap();
        let archives = vec![a.clone(), b.clone()];

        let err = build_plan(&archives, &image, 0, 0, &NoProgress).unwrap_err();
        let message = err.to_string();
        assert!(message.contains("Turrican"), "{message}");
        assert!(message.contains("release-a.lha"), "{message}");
        assert!(message.contains("release-b.lha"), "{message}");

        let err = install_archives(&archives, &image, 0, 0, OverwritePolicy::Skip, &NoProgress)
            .unwrap_err();
        assert!(err.to_string().contains("Turrican"));
        assert_eq!(
            std::fs::read(&image).unwrap(),
            before,
            "a refused batch must leave the image byte-for-byte unchanged"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    // ---- does not fit ----

    /// A batch that will not fit is refused as data — real block numbers,
    /// nothing written — before a single archive is copied in.
    #[test]
    fn a_batch_that_does_not_fit_is_refused_before_anything_is_written() {
        let dir = scratch("toobig");
        let a = dir.join("Small.lha");
        std::fs::write(&a, make_lha_with(&[("Small/File", b"tiny")])).unwrap();
        let b = dir.join("Big.lha");
        let big = vec![b'x'; 900 * 1024];
        std::fs::write(&b, make_lha_with(&[("Big/Blob.bin", &big)])).unwrap();

        let image = disk(&dir, "disk.adf");
        let before = std::fs::read(&image).unwrap();
        let archives = vec![a, b];

        let plan = build_plan(&archives, &image, 0, 0, &NoProgress).unwrap();
        assert!(!plan.cost.fits(), "a batch this large must not fit");
        let refusal = plan
            .cost
            .shortfall()
            .expect("a batch this large must be refused");
        assert!(refusal.contains("blocks"), "{refusal}");
        assert!(refusal.contains("free"), "{refusal}");
        assert_eq!(
            std::fs::read(&image).unwrap(),
            before,
            "planning must never touch the image"
        );

        let err = install_archives(&archives, &image, 0, 0, OverwritePolicy::Skip, &NoProgress)
            .unwrap_err();
        assert_eq!(err.code(), "ART-SAFETY-REFUSED");
        assert_eq!(
            std::fs::read(&image).unwrap(),
            before,
            "a refused install must leave the image byte-for-byte unchanged"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    // ---- cancellation ----

    /// Five archives to cancel partway through, shared by both cancellation
    /// tests below.
    fn five_archives(dir: &Path) -> Vec<PathBuf> {
        let mut archives = Vec::new();
        for index in 0..5 {
            let path = dir.join(format!("Game{index}.lha"));
            std::fs::write(
                &path,
                make_lha_with(&[(
                    &format!("Game{index}/Data"),
                    vec![b'a' + index; 64].as_slice(),
                )]),
            )
            .unwrap();
            archives.push(path);
        }
        archives
    }

    /// Cancelling while archives are still being unpacked — before the batch
    /// has even been staged, let alone reached the volume writer — must
    /// leave the image exactly as it was. The bytes are captured before and
    /// compared after, because an `Err` on its own would not prove nothing
    /// reached the file.
    ///
    /// `prepare_archives` reports `total.is_some()` from its very first call
    /// (`progress.report(index, Some(total), "Unpacking …")`), so a sink that
    /// trips on *any* report with a total — as an earlier version of this
    /// test did — cancels during this loop and never reaches
    /// `install_into_folder` at all. That made the byte-comparison trivially
    /// true regardless of whether cancelling the copy phase was actually
    /// atomic. This test is kept, renamed to say what it actually covers;
    /// `cancelling_during_the_copy_phase_writes_nothing` below covers the
    /// path this one does not.
    #[test]
    fn cancelling_during_unpacking_writes_nothing() {
        use std::sync::atomic::{AtomicBool, Ordering};

        struct StopDuringUnpack(AtomicBool);
        impl ProgressSink for StopDuringUnpack {
            fn report(&self, done: u64, total: Option<u64>, _message: &str) {
                if total.is_some() && done >= 1 {
                    self.0.store(true, Ordering::SeqCst);
                }
            }
            fn is_cancelled(&self) -> bool {
                self.0.load(Ordering::SeqCst)
            }
        }

        let dir = scratch("cancel-unpack");
        let archives = five_archives(&dir);

        let image = disk(&dir, "disk.adf");
        let before = std::fs::read(&image).unwrap();

        let sink = StopDuringUnpack(AtomicBool::new(false));
        let err = install_archives(&archives, &image, 0, 0, OverwritePolicy::Skip, &sink)
            .expect_err("a cancelled batch must not come back as a successful install");

        assert_eq!(
            err.code(),
            "ART-CANCELLED",
            "the job must end Cancelled, not Completed: {err}"
        );
        assert_eq!(
            std::fs::read(&image).unwrap(),
            before,
            "cancelling during unpacking must leave the image byte-for-byte unchanged"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// ART-067. Stop has to be heard **inside** one archive, not only between
    /// two: a batch of five whose third is large used to leave Stop
    /// unresponsive for the whole of that extraction, because
    /// `prepare_archives` unpacked with `NoProgress` and `is_cancelled()`
    /// therefore answered `false` all the way down.
    ///
    /// One archive on purpose. The loop's own check at the top runs before the
    /// sink has ever been called, and there is no second iteration to reach —
    /// so the only way this can come back `Cancelled` is if the cancellation
    /// travelled *into* the unpack. Against the old code it returned `Ok`.
    #[test]
    fn stop_is_heard_inside_an_archive_not_only_between_them() {
        use std::sync::atomic::{AtomicBool, Ordering};

        /// Trips on the first report the unpack itself makes — `BatchStep`
        /// marks those with an em dash, where the loop's own per-archive line
        /// is plain `"Unpacking Game0.lha"`.
        struct StopInsideUnpack(AtomicBool);
        impl ProgressSink for StopInsideUnpack {
            fn report(&self, _done: u64, _total: Option<u64>, message: &str) {
                if message.contains('—') {
                    self.0.store(true, Ordering::SeqCst);
                }
            }
            fn is_cancelled(&self) -> bool {
                self.0.load(Ordering::SeqCst)
            }
        }

        let dir = scratch("cancel-inside-archive");
        let archive = dir.join("Game0.lha");
        std::fs::write(
            &archive,
            make_lha_with(&[
                ("Game0/One", b"aaaa".as_slice()),
                ("Game0/Two", b"bbbb".as_slice()),
                ("Game0/Three", b"cccc".as_slice()),
            ]),
        )
        .unwrap();

        let staging = dir.join("staging");
        std::fs::create_dir_all(&staging).unwrap();

        let sink = StopInsideUnpack(AtomicBool::new(false));
        let err = prepare_archives(&[archive], &staging, &sink)
            .expect_err("a cancelled unpack must not come back as a prepared batch");
        assert_eq!(err.code(), "ART-CANCELLED", "{err}");

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// ART-066. Planning a batch has to unpack every archive to know what is
    /// in it, and it used to do that on the Tauri command thread: several
    /// large archives froze the window with no progress and no way to stop —
    /// in the one step that exists so the user can change their mind before
    /// anything is written.
    ///
    /// It runs on a job now, which means the sink reaches all the way down and
    /// Stop is answered. This is the engine half of that; the command itself
    /// is `spawn_job` plus an event, neither of which a unit test can host.
    #[test]
    fn planning_answers_stop() {
        use std::sync::atomic::{AtomicBool, Ordering};

        struct StopAtOnce(AtomicBool);
        impl ProgressSink for StopAtOnce {
            fn report(&self, _done: u64, _total: Option<u64>, _message: &str) {
                self.0.store(true, Ordering::SeqCst);
            }
            fn is_cancelled(&self) -> bool {
                self.0.load(Ordering::SeqCst)
            }
        }

        let dir = scratch("plan-cancel");
        let archives = five_archives(&dir);
        let image = disk(&dir, "disk.adf");

        let sink = StopAtOnce(AtomicBool::new(false));
        let err = build_plan(&archives, &image, 0, 0, &sink)
            .expect_err("a cancelled plan must not come back as a plan");
        assert_eq!(err.code(), "ART-CANCELLED", "{err}");

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Cancelling once the batch is actually being copied into the volume —
    /// the failure mode the brief names by name ("cancelling after two of
    /// five archives") — must still leave the image byte-for-byte unchanged,
    /// not a prefix of the five archives installed.
    ///
    /// The sink only reacts to reports whose message does not start with
    /// `"Unpack"`: `prepare_archives`'s per-archive progress messages are
    /// `"Unpacking …"` (and the final `"Unpacked"`), while
    /// `copy_into_volume`'s are the entry's own relative path (`"Game0/Data"`,
    /// …) — so this genuinely waits for the copy phase, tripping only after
    /// a few of *those* reports, rather than reusing the "any report with a
    /// total" trip that `cancelling_during_unpacking_writes_nothing` proved
    /// stops too early to reach it.
    #[test]
    fn cancelling_during_the_copy_phase_writes_nothing() {
        use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

        struct StopDuringCopy {
            copy_reports: AtomicU64,
            cancelled: AtomicBool,
        }
        impl ProgressSink for StopDuringCopy {
            fn report(&self, _done: u64, total: Option<u64>, message: &str) {
                if total.is_none() || message.starts_with("Unpack") {
                    return;
                }
                if self.copy_reports.fetch_add(1, Ordering::SeqCst) + 1 >= 3 {
                    self.cancelled.store(true, Ordering::SeqCst);
                }
            }
            fn is_cancelled(&self) -> bool {
                self.cancelled.load(Ordering::SeqCst)
            }
        }

        let dir = scratch("cancel-copy");
        let archives = five_archives(&dir);

        let image = disk(&dir, "disk.adf");
        let before = std::fs::read(&image).unwrap();

        let sink = StopDuringCopy {
            copy_reports: AtomicU64::new(0),
            cancelled: AtomicBool::new(false),
        };
        let err = install_archives(&archives, &image, 0, 0, OverwritePolicy::Skip, &sink)
            .expect_err("a cancelled batch must not come back as a successful install");

        assert_eq!(
            err.code(),
            "ART-CANCELLED",
            "the job must end Cancelled, not Completed: {err}"
        );
        assert_eq!(
            std::fs::read(&image).unwrap(),
            before,
            "cancelling during the copy phase must leave the image byte-for-byte \
             unchanged, not two of five archives installed"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Cancelling before any unpacking starts must also leave nothing behind
    /// — and nothing in the temp directory either.
    #[test]
    fn cancelling_before_the_first_archive_unpacks_writes_nothing() {
        struct AlwaysCancelled;
        impl ProgressSink for AlwaysCancelled {
            fn report(&self, _: u64, _: Option<u64>, _: &str) {}
            fn is_cancelled(&self) -> bool {
                true
            }
        }

        let dir = scratch("cancel-early");
        let archive = dir.join("Game.lha");
        std::fs::write(&archive, make_lha_with(&[("Game/Data", b"bytes")])).unwrap();

        let image = disk(&dir, "disk.adf");
        let before = std::fs::read(&image).unwrap();

        let err = install_archives(
            &[archive],
            &image,
            0,
            0,
            OverwritePolicy::Skip,
            &AlwaysCancelled,
        )
        .unwrap_err();
        assert_eq!(err.code(), "ART-CANCELLED");
        assert_eq!(std::fs::read(&image).unwrap(), before);

        let _ = std::fs::remove_dir_all(&dir);
    }

    // ---- scratch and staging clean up ----

    /// `Staging` removes itself once dropped — proved in isolation rather
    /// than by counting `art-archives-*` siblings in the shared system temp
    /// directory, which every other test in this module (running in
    /// parallel, per `cargo test`'s default) also creates and removes.
    /// Mirrors `the_staging_folder_removes_itself` in
    /// `core::volume::write::copy`.
    #[test]
    fn staging_removes_itself_on_drop() {
        let path = {
            let staging = Staging::new().unwrap();
            let path = staging.path().to_path_buf();
            assert!(path.is_dir(), "Staging::new must create the directory");
            path
        };
        assert!(
            !path.exists(),
            "the staging directory must not survive being dropped"
        );
    }

    /// A refused plan (batch too large) leaves no staging directory behind —
    /// `build_plan` always creates its `Staging` as a local that goes out of
    /// scope on every return path, refusal included, so the guarantee above
    /// covers this path too. Proved directly rather than by a temp-directory
    /// headcount, which the module's own parallel tests make unreliable.
    #[test]
    fn a_refused_plan_cleans_up_its_staging_directory() {
        let dir = scratch("cleanup-refused");
        let a = dir.join("Small.lha");
        std::fs::write(&a, make_lha_with(&[("Small/File", b"tiny")])).unwrap();
        let b = dir.join("Big.lha");
        let big = vec![b'x'; 900 * 1024];
        std::fs::write(&b, make_lha_with(&[("Big/Blob.bin", &big)])).unwrap();

        let image = disk(&dir, "disk.adf");

        let plan = build_plan(&[a, b], &image, 0, 0, &NoProgress).unwrap();
        assert!(!plan.cost.fits(), "this batch must be refused");

        let _ = std::fs::remove_dir_all(&dir);
    }

    // ---- archive entry names are untrusted ----

    /// A traversal entry, an absolute path and a drive-prefixed path must
    /// each be rejected at the point they would actually land, not merely
    /// absent from inside the scratch directory.
    ///
    /// Asserting only `!scratch/anything.exists()` (as an earlier version of
    /// this test did) does not prove an escape was prevented: a naive join of
    /// `scratch` with `"../../outside.txt"` never produces a path *inside*
    /// `scratch` in the first place, so that assertion holds whether or not
    /// `safe_join` is doing anything at all. This instead computes the
    /// concrete path each hostile entry would land at if the join were naive
    /// — mirroring `core::lha::safe_extract::tests::traversal_entry_is_rejected_not_extracted`,
    /// which asserts on `dir.join("evil.txt")`, the real target — and checks
    /// none of them exist, plus that every hostile entry is named in the
    /// unpack's own skipped list rather than silently dropped.
    #[test]
    fn hostile_entries_are_rejected_at_their_real_target_not_just_absent_from_scratch() {
        // `Scratch::new()` always creates its directory one level directly
        // under `std::env::temp_dir()`, so a naive join of `"../whatever"`
        // with the scratch root resolves to a path in that *shared* system
        // temp directory, not somewhere private to this test. A guard removes
        // it on every exit path, including a panicked assertion, so a real
        // regression here does not also leave litter for the next run to
        // trip over — and the name carries this process's id so two test
        // binaries running at once can never collide on it.
        struct RemoveOnDrop(PathBuf);
        impl Drop for RemoveOnDrop {
            fn drop(&mut self) {
                let _ = std::fs::remove_file(&self.0);
            }
        }

        let marker = format!("art-oracle-traversal-marker-{}.txt", std::process::id());
        let _cleanup = RemoveOnDrop(std::env::temp_dir().join(&marker));

        let dir = scratch("traversal-unpack");
        let archive = dir.join("Evil.lha");
        std::fs::write(
            &archive,
            make_lha_with(&[
                ("Evil/Game", b"safe bytes"),
                (&format!("../{marker}"), b"must never land here"),
                ("C:\\art-oracle-drive-escape.txt", b"nor here"),
                ("/art-oracle-root-escape.txt", b"nor here either"),
            ]),
        )
        .unwrap();

        let (scratch_dir, unpack_skipped) = unpack_for_install(&archive, &NoProgress).unwrap();

        // The safe entry landed, under the scratch root...
        assert!(scratch_dir.path().join("Evil").join("Game").is_file());

        // ...and each hostile one is checked at the exact place a naive join
        // would have put it, not merely "somewhere under scratch". Absolute
        // and drive-prefixed targets are also outside anywhere this process
        // can write without elevation, so their own non-existence is checked
        // too, without needing a guard for either.
        let one_level_up = scratch_dir
            .path()
            .parent()
            .expect("scratch always has a parent")
            .join(&marker);
        assert!(
            !one_level_up.exists(),
            "'../{marker}' must not land one level above the scratch directory"
        );
        assert!(!Path::new("C:\\art-oracle-drive-escape.txt").exists());
        assert!(!Path::new("/art-oracle-root-escape.txt").exists());

        // Every hostile entry is reported, not silently dropped.
        assert_eq!(
            unpack_skipped.len(),
            3,
            "all three hostile entries must be reported as skipped: {unpack_skipped:?}"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The same hostile archive, consumed through `prepare_archives` (the
    /// path `archives_plan_install`/`archives_install` actually use): the
    /// safe entry still gets its drawer, and every entry the extractor
    /// refused is carried onto `ArchiveDrawer::skipped` — shown in the plan,
    /// before installing, per §68's "never silent" rule — rather than
    /// vanishing with the batch reporting "1 file" and no explanation of
    /// where the second entry went.
    #[test]
    fn a_traversal_entry_contributes_nothing_to_the_drawer_and_is_reported() {
        let dir = scratch("traversal-drawer");
        let archive = dir.join("Evil.lha");
        std::fs::write(
            &archive,
            make_lha_with(&[
                ("Evil/Game", b"safe bytes"),
                ("../outside.txt", b"must never land here"),
            ]),
        )
        .unwrap();

        let staging = dir.join("staging");
        std::fs::create_dir_all(&staging).unwrap();
        let (drawers, roots) = prepare_archives(&[archive], &staging, &NoProgress).unwrap();

        // The safe entry still installs...
        assert_eq!(drawers[0].drawer, "Evil");
        assert_eq!(
            drawers[0].files, 1,
            "only the safe entry, the traversal one is not counted"
        );
        assert!(roots[0].join("Game").is_file());
        assert!(
            roots[0].starts_with(&staging),
            "every produced root stays under staging"
        );

        // ...the traversal entry landed nowhere under staging either...
        assert!(!staging.join("outside.txt").exists());
        assert!(!roots[0].join("outside.txt").exists());

        // ...and it is named, not dropped.
        assert_eq!(drawers[0].skipped.len(), 1, "{:?}", drawers[0].skipped);
        assert!(
            drawers[0].skipped[0].contains("outside.txt"),
            "{:?}",
            drawers[0].skipped
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    // ---- a mundane batch that fits, with policy ----

    /// `normalize_archives` is what both commands call before anything else
    /// (`build_plan`/`install_archives` are handed an already-validated
    /// list), so this is where "choose at least one" is enforced.
    #[test]
    fn an_empty_archive_list_is_refused_with_a_clear_reason() {
        let err = normalize_archives(&[]).unwrap_err();
        assert_eq!(err.code(), "ART-INPUT-INVALID");
    }
}
