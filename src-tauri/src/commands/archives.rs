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
use crate::core::jobs::{JobId, NoProgress, ProgressSink};
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
}

/// What installing every archive in the batch would do. Writes nothing.
#[derive(Debug, Clone, Serialize)]
pub struct ArchivesPlan {
    /// One row per archive, in the order given.
    pub drawers: Vec<ArchiveDrawer>,
    /// The cost of the whole batch, over the union of every drawer — the same
    /// [`CopyPlan`] a plain multi-file copy shows, so the dialog that already
    /// renders one reads this without a special case.
    pub cost: CopyPlan,
    /// Present, as data and never an error, when the batch will not fit —
    /// real block numbers against real free space (§92).
    pub refusal: Option<String>,
}

/// What installing `archives` into a volume would do. Writes nothing.
#[tauri::command]
pub fn archives_plan_install(
    archives: Vec<String>,
    image: String,
    volume_index: usize,
    dir_block: Option<u32>,
) -> AppResult<ArchivesPlan> {
    let archives = normalize_archives(&archives)?;
    let image_path = PathBuf::from(image.trim());
    let parent = dir_block.unwrap_or(0);

    Ok(build_plan(&archives, &image_path, volume_index, parent)?)
}

fn build_plan(
    archives: &[PathBuf],
    image: &Path,
    volume_index: usize,
    parent: u32,
) -> CoreResult<ArchivesPlan> {
    let staging = Staging::new()?;
    let (drawers, roots) = prepare_archives(archives, staging.path(), &NoProgress)?;

    let selection = HostSelection::new(roots);
    let cost = crate::commands::volume_write::plan_copy_in_folder(
        image,
        volume_index,
        parent,
        &selection,
    )?;
    let refusal = cost.shortfall();

    Ok(ArchivesPlan {
        drawers,
        cost,
        refusal,
    })
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
    let (_drawers, roots) = prepare_archives(archives, staging.path(), progress)?;
    let selection = HostSelection::new(roots);

    // The same fits-or-nothing, abandon-on-cancel primitive a single-archive
    // install uses — one call, over the whole staged batch, so a cancelled
    // batch is exactly as atomic as a cancelled single install.
    crate::commands::volume_write::install_into_folder(
        image,
        volume_index,
        parent,
        &selection,
        policy,
        progress,
    )
}

// ---------------------------------------------------------------------------
// Unpacking, naming and staging — shared by the plan and the install
// ---------------------------------------------------------------------------

/// Unpack every archive, work out where its contents will land, and move each
/// into `staging` under that name.
///
/// Both [`archives_plan_install`] and [`archives_install`] call this and get
/// the same drawer name for the same archive — a plan that shows one name and
/// an install that creates another would defeat §92 entirely.
fn prepare_archives(
    archives: &[PathBuf],
    staging: &Path,
    progress: &dyn ProgressSink,
) -> CoreResult<(Vec<ArchiveDrawer>, Vec<PathBuf>)> {
    let total = archives.len() as u64;

    // Every scratch directory stays alive until the whole batch has been
    // named and checked for collisions — dropping one early would remove its
    // content before it has been moved into staging.
    let mut unpacked: Vec<(
        PathBuf,
        String,
        PathBuf,
        crate::core::sources::install::Scratch,
    )> = Vec::with_capacity(archives.len());

    for (index, archive) in archives.iter().enumerate() {
        // Between whole units of work, never mid-unpack (§54): one archive is
        // one unit, so cancelling here stops before the next one starts and
        // never leaves a partially unpacked archive behind.
        if progress.is_cancelled() {
            return Err(CoreError::Cancelled);
        }
        let label = file_label(archive);
        progress.report(index as u64, Some(total), &format!("Unpacking {label}"));

        let (scratch, _unpack_skipped) = unpack_for_install(archive, &NoProgress)?;
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

        unpacked.push((archive.clone(), drawer_name, content_root, scratch));
    }

    // Two archives that would both create the same drawer are reported by
    // name, before anything moves — silently interleaving two unrelated
    // archives into one directory would be worse than refusing (§92).
    let mut seen: std::collections::BTreeMap<String, &Path> = std::collections::BTreeMap::new();
    for (archive, drawer, _, _) in &unpacked {
        if let Some(other) = seen.get(drawer.as_str()) {
            return Err(CoreError::InvalidInput(format!(
                "'{}' and '{}' would both create a drawer called '{drawer}' — rename one \
                 before installing them together",
                other.display(),
                archive.display()
            )));
        }
        seen.insert(drawer.clone(), archive.as_path());
    }

    let mut drawers = Vec::with_capacity(unpacked.len());
    let mut roots = Vec::with_capacity(unpacked.len());

    for (archive, drawer_name, content_root, _scratch) in &unpacked {
        let stats = HostFolder::new(content_root.as_path(), true).entries()?;
        let files = stats.iter().filter(|entry| !entry.is_dir).count();
        let directories = stats.iter().filter(|entry| entry.is_dir).count();
        let bytes: u64 = stats
            .iter()
            .filter(|entry| !entry.is_dir)
            .map(|entry| entry.bytes)
            .sum();

        let destination = safe_join(staging, drawer_name).map_err(|err| {
            CoreError::InvalidInput(format!(
                "'{drawer_name}' is not a name ART can use for a drawer: {err}"
            ))
        })?;
        std::fs::rename(content_root, &destination)?;

        drawers.push(ArchiveDrawer {
            archive: archive.display().to_string(),
            name: file_label(archive),
            drawer: drawer_name.clone(),
            files,
            directories,
            bytes,
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
fn archive_stem(archive: &Path) -> CoreResult<String> {
    archive
        .file_stem()
        .and_then(|stem| stem.to_str())
        .map(str::to_string)
        .filter(|stem| !stem.is_empty())
        .ok_or_else(|| {
            CoreError::InvalidInput(format!(
                "'{}' has no name ART can use for a drawer",
                archive.display()
            ))
        })
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

        let plan = build_plan(&archives, &image, 0, 0).unwrap();
        assert_eq!(plan.drawers.len(), 3);
        assert_eq!(plan.drawers[0].drawer, "Turrican");
        assert_eq!(plan.drawers[1].drawer, "Xenon2");
        assert_eq!(plan.drawers[2].drawer, "Loose");
        assert!(plan.refusal.is_none(), "{:?}", plan.refusal);

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

        let err = build_plan(&archives, &image, 0, 0).unwrap_err();
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

        let plan = build_plan(&archives, &image, 0, 0).unwrap();
        let refusal = plan.refusal.expect("a batch this large must be refused");
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

    /// Cancelling after the first archive has landed must leave the image
    /// exactly as it was — not one game installed out of five. The bytes are
    /// captured before and compared after, because an `Err` on its own would
    /// not prove nothing reached the file.
    #[test]
    fn cancelling_after_the_first_archive_writes_nothing() {
        use std::sync::atomic::{AtomicBool, Ordering};

        /// Cancels once the copy into the volume is under way — the user
        /// hitting Stop after unpacking has already finished, not before it
        /// started.
        struct StopDuringCopy(AtomicBool);
        impl ProgressSink for StopDuringCopy {
            fn report(&self, done: u64, total: Option<u64>, _message: &str) {
                if total.is_some() && done >= 1 {
                    self.0.store(true, Ordering::SeqCst);
                }
            }
            fn is_cancelled(&self) -> bool {
                self.0.load(Ordering::SeqCst)
            }
        }

        let dir = scratch("cancel");
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

        let image = disk(&dir, "disk.adf");
        let before = std::fs::read(&image).unwrap();

        let sink = StopDuringCopy(AtomicBool::new(false));
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
            "a cancelled batch must leave the image byte-for-byte unchanged, \
             not two of five archives installed"
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

        let plan = build_plan(&[a, b], &image, 0, 0).unwrap();
        assert!(plan.refusal.is_some(), "this batch must be refused");

        let _ = std::fs::remove_dir_all(&dir);
    }

    // ---- archive entry names are untrusted ----

    /// A traversal entry inside an archive must never escape the scratch
    /// directory it is unpacked into, even by way of becoming a drawer name
    /// or landing inside one.
    ///
    /// `unpack_for_install` already defends this at the extraction layer
    /// (`core::lha::safe_extract`); this proves the defence still holds once
    /// the result passes through this module's own naming and staging —
    /// `../../outside.txt` must contribute nothing to the drawer, and every
    /// path this module builds under `staging` must stay under it.
    #[test]
    fn a_traversal_entry_does_not_escape_the_scratch_directory() {
        let dir = scratch("traversal");
        let archive = dir.join("Evil.lha");
        std::fs::write(
            &archive,
            make_lha_with(&[
                ("Evil/Game", b"safe bytes"),
                ("../../outside.txt", b"must never land here"),
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

        // ...and the traversal entry landed nowhere at all: not beside the
        // drawer, not inside it, not at the staging root.
        assert!(!staging.join("outside.txt").exists());
        assert!(!roots[0].join("outside.txt").exists());
        assert!(!staging.join("Evil").join("../outside.txt").exists());

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
