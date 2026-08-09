//! One-click WHDLoad install onto a hard disk (spec §82).
//!
//! ```text
//! Game.lha → DROP → WHDLoad detected → Install to HDF → Backup → Apply → Verify
//! ```
//!
//! Every arrow but one already existed. This is the piece that joins them: the
//! archive is unpacked with the extractor the ADF install already uses, the
//! pack drawer is located inside it (`core/whdload`), and the drawer plus its
//! icon are written with the Stage W volume writer — which supplies the
//! backup, the journal and the per-file verification.
//!
//! ## Plan first, always
//!
//! [`whdload_plan`] writes nothing. It reports what was detected and with what
//! confidence, where the pack would land, what it will cost in blocks, and
//! anything the user should decide about — a name already taken, a missing
//! icon, an archive that turns out to need an Amiga to install itself. Only
//! then does [`whdload_install`] run. *Explain before modify* (§92).
//!
//! ## The archive is unpacked twice
//!
//! Once to plan and once to install, rather than the plan holding a scratch
//! directory open until the user decides. A WHDLoad archive is a few megabytes
//! and unpacking it is fast; a temp folder whose lifetime is "until someone
//! clicks something" is a leak waiting for a closed window.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::Serialize;
use tauri::{AppHandle, Emitter, State};

use super::jobs::{spawn_job, JobRegistry};
use super::oplog::{user_operation, write_to_path};
use crate::core::error::{CoreError, CoreResult};
use crate::core::jobs::{JobId, NoProgress, ProgressSink};
use crate::core::lha::whdload::{detect_whdload, WhdloadVerdict};
use crate::core::lha::{open_archive, OverwritePolicy};
use crate::core::oplog::{JsonlOperationLog, OperationOutcome};
use crate::core::sources::install::unpack_for_install;
use crate::core::volume::mount::mount;
use crate::core::volume::write::copy::{copy_into_volume, CopyReport, HostFolder};
use crate::core::volume::write::plan::{plan_copy, CopyPlan, SourceEntry};
use crate::core::volume::write::FileMeta;
use crate::core::whdload::{analyse, Entry, PackLayout};
use crate::error::AppResult;

/// The event a finished install arrives on.
pub const WHDLOAD_EVENT: &str = "whdload-result";

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
    pub refusal: Option<String>,
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
    /// Where the previous image went, for the whole-file strategy.
    pub backup: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum WhdloadResult {
    Installed {
        job_id: JobId,
        outcome: WhdloadOutcome,
    },
}

// ---------------------------------------------------------------------------
// Planning
// ---------------------------------------------------------------------------

/// What installing `archive` into a volume would do. Writes nothing.
#[tauri::command]
pub fn whdload_plan(
    archive: String,
    image: String,
    volume_index: usize,
    dir_block: Option<u32>,
) -> AppResult<WhdloadPlan> {
    let archive_path = PathBuf::from(archive.trim());
    let image_path = PathBuf::from(image.trim());

    Ok(build_plan(
        &archive_path,
        &image_path,
        volume_index,
        dir_block.unwrap_or(0),
    )?)
}

fn build_plan(
    archive: &Path,
    image: &Path,
    volume_index: usize,
    dir_block: u32,
) -> CoreResult<WhdloadPlan> {
    // The verdict comes from the archive's own entry list, before anything is
    // unpacked — a package ART is not confident about should not cost the user
    // a decompression first.
    let info = open_archive(archive)?;
    let verdict = detect_whdload(&info.entries);

    let (scratch, unpack_skipped) = unpack_for_install(archive, &NoProgress)?;
    let entries = walk(scratch.path())?;
    let layout = analyse(&entries)?;

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
    let mut sources: Vec<SourceEntry> = {
        use crate::core::volume::write::copy::CopySource;
        folder.entries()?
    };
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

    let entry = super::volume_write::pick_volume(image, volume_index)?;
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

    let volume_name = read_volume_name(&device, &geometry).unwrap_or_else(|| entry.name.clone());

    let refusal = refuse(&verdict, &layout, &cost, name_taken, &unpack_skipped);

    Ok(WhdloadPlan {
        verdict,
        drawer: format!("{volume_name}:{}", layout.name),
        volume_name,
        layout,
        cost,
        name_taken,
        refusal,
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
) -> Option<String> {
    use crate::core::workflow::types::Confidence;

    if matches!(verdict.confidence, Confidence::Low | Confidence::Unknown) {
        return Some(format!(
            "ART is not confident this is a WHDLoad package ({}). {} \
             Install it by hand from the Files screen if you know it is one.",
            describe_confidence(verdict.confidence),
            verdict.notes
        ));
    }

    if layout.needs_installer {
        return Some(
            "This archive holds an Install script, which means the game has not been \
             installed yet. Running it needs an Amiga — install it in WinUAE first, then \
             bring the finished drawer back here."
                .into(),
        );
    }

    if name_taken {
        return Some(format!(
            "'{}' is already on that volume. Rename or remove it first — ART will not \
             write a game over one that is already there.",
            layout.name
        ));
    }

    if cost.blocks_needed > cost.blocks_free {
        return Some(format!(
            "This needs {} blocks and {} are free.",
            cost.blocks_needed, cost.blocks_free
        ));
    }

    if !cost.name_problems.is_empty() {
        return Some(format!(
            "{} name(s) in this package are ones AmigaDOS cannot store. Installing it \
             under different names would give you a game whose files no longer match \
             what its slave looks for.",
            cost.name_problems.len()
        ));
    }

    if !unpack_skipped.is_empty() {
        return Some(format!(
            "The archive did not unpack completely ({}). Installing part of a game \
             produces one that does not start.",
            unpack_skipped.first().cloned().unwrap_or_default()
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

    if depth > MAX_COPY_DEPTH || out.len() >= crate::core::whdload::MAX_ENTRIES {
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

/// Install a WHDLoad package onto a volume. Returns a job id (§54).
#[tauri::command]
pub fn whdload_install(
    archive: String,
    image: String,
    volume_index: usize,
    dir_block: Option<u32>,
    app: AppHandle,
    registry: State<'_, Arc<JobRegistry>>,
    oplog: State<'_, JsonlOperationLog>,
) -> AppResult<JobId> {
    let archive_path = PathBuf::from(archive.trim());
    let image_path = PathBuf::from(image.trim());
    let parent = dir_block.unwrap_or(0);

    let log_path = oplog.path().to_path_buf();
    let registry = Arc::clone(&registry);
    let emit_app = app.clone();
    let title = format!(
        "Installing {}",
        archive_path
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
            .unwrap_or_default()
    );

    let id = spawn_job(&app, registry, &title, move |job_id, progress| {
        let outcome = run_install(&archive_path, &image_path, volume_index, parent, progress);

        // §53's example is this operation by name. It records what went in,
        // where, how many files, and whether verification passed.
        let record = user_operation("Install WHDLoad")
            .source(archive_path.display().to_string())
            .destination(format!("{}:{volume_index}", image_path.display()));
        let record = match &outcome {
            Ok(installed) => record
                .detail("Drawer", installed.drawer.clone())
                .detail("Files", installed.files.to_string())
                .detail("Icon", installed.icon_installed.to_string())
                .outcome(OperationOutcome::verified(
                    installed.verified == installed.files,
                )),
            Err(err) => record.failed(err),
        };
        write_to_path(&log_path, &record);

        let installed = outcome?;
        let _ = emit_app.emit(
            WHDLOAD_EVENT,
            WhdloadResult::Installed {
                job_id,
                outcome: installed,
            },
        );
        Ok(())
    });

    Ok(id)
}

/// Unpack, re-plan, and write — all three in one volume session.
///
/// The plan is rebuilt here rather than carried from the UI. A plan the user
/// looked at five minutes ago describes a disk that may have changed since,
/// and installing against a stale one is how a "there is room" turns into a
/// half-written game.
fn run_install(
    archive: &Path,
    image: &Path,
    volume_index: usize,
    parent: u32,
    sink: &dyn ProgressSink,
) -> CoreResult<WhdloadOutcome> {
    sink.report(0, None, "Checking the package");
    let plan = build_plan(archive, image, volume_index, parent)?;
    if !plan.can_install() {
        return Err(CoreError::SafetyRefused(
            plan.refusal
                .unwrap_or_else(|| "ART will not install this".into()),
        ));
    }

    sink.report(0, None, "Unpacking");
    let (scratch, _) = unpack_for_install(archive, sink)?;
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

    // One session, so the whole install is one backup rather than three
    // generations of the same image (§1).
    let ((report, icon_installed), _strategy, backup) = super::volume_write::with_volume(
        image,
        volume_index,
        move |writer| -> CoreResult<(CopyReport, bool)> {
            let drawer = writer
                .make_dir(parent, &drawer_name)?
                .block
                .ok_or_else(|| CoreError::Malformed {
                    format: "volume".into(),
                    detail: "the drawer was created but ART lost track of it".into(),
                })?;

            let report = copy_into_volume(writer, drawer, &folder, OverwritePolicy::Skip, sink)?;

            // The icon goes **beside** the drawer, not inside it. Inside, it
            // describes nothing and the game stays invisible on Workbench.
            let installed = match icon_bytes {
                Some(bytes) => {
                    writer.add_file(parent, &icon_name, &bytes, FileMeta::default())?;
                    true
                }
                None => false,
            };

            Ok((report, installed))
        },
    )?;

    Ok(WhdloadOutcome {
        drawer: plan.drawer,
        files: report.files_copied,
        directories: report.directories_created,
        bytes: report.bytes_copied,
        verified: report.files_verified,
        icon_installed,
        skipped: report.skipped,
        backup,
    })
}

// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::volume::fixture::ffs_volume;
    use crate::core::volume::DosType;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("art-whd-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
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
        let dir = scratch("walk");
        unpacked_pack(&dir);

        let entries = walk(&dir).unwrap();
        let layout = analyse(&entries).unwrap();

        assert_eq!(layout.root, "Turrican");
        assert_eq!(layout.name, "Turrican");
        assert_eq!(layout.icon.as_deref(), Some("Turrican.info"));
        assert_eq!(layout.outside, vec!["Turrican.readme"]);

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The whole point of §82's arrow: the drawer, its contents and its icon
    /// all land, and the icon lands *beside* the drawer.
    #[test]
    fn an_install_creates_the_drawer_its_contents_and_its_icon() {
        let dir = scratch("install");
        let source = dir.join("unpacked");
        std::fs::create_dir_all(&source).unwrap();
        unpacked_pack(&source);

        let image = dir.join("games.hdf");
        let (bytes, geometry) = ffs_volume(1760, DosType::new(*b"DOS\x01"));
        std::fs::write(&image, &bytes).unwrap();

        let folder = HostFolder::new(source.join("Turrican"), true);
        let icon = std::fs::read(source.join("Turrican.info")).unwrap();

        let ((report, icon_installed), _, _) = super::super::volume_write::with_volume(
            &image,
            0,
            move |writer| -> CoreResult<(CopyReport, bool)> {
                let drawer = writer.make_dir(0, "Turrican")?.block.unwrap();
                let report =
                    copy_into_volume(writer, drawer, &folder, OverwritePolicy::Skip, &NoProgress)?;
                writer.add_file(0, "Turrican.info", &icon, FileMeta::default())?;
                Ok((report, true))
            },
        )
        .unwrap();

        assert!(icon_installed);
        assert_eq!(report.files_copied, 3, "slave, executable, one data file");
        assert_eq!(report.files_verified, 3, "every file read back");
        assert_eq!(report.directories_created, 1, "the data drawer");

        // And the volume actually holds it, read back through a separate path.
        let entry = super::super::volume_write::pick_volume(&image, 0).unwrap();
        let (device, geometry_read) = mount(&image, &entry).unwrap();
        let set = crate::core::volume::write::layout::BlockSet::new(geometry.block_size);
        let root = crate::core::volume::write::dir::entries_in(
            &device,
            &set,
            &geometry_read,
            geometry_read.root_block,
        )
        .unwrap();

        let drawer = root.iter().find(|e| e.name == "Turrican").unwrap();
        assert!(drawer.is_dir);
        assert!(
            root.iter().any(|e| e.name == "Turrican.info" && !e.is_dir),
            "the icon must sit beside the drawer, not inside it — without it the \
             game is invisible on Workbench"
        );

        let inside = crate::core::volume::write::dir::entries_in(
            &device,
            &set,
            &geometry_read,
            drawer.block,
        )
        .unwrap();
        assert!(inside.iter().any(|e| e.name == "Turrican.slave"));
        assert!(inside.iter().any(|e| e.name == "data" && e.is_dir));

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Three operations in one session must produce **one** backup, not three
    /// generations of the same image.
    #[test]
    fn a_whole_install_backs_the_image_up_once() {
        let dir = scratch("one-backup");
        let source = dir.join("unpacked");
        std::fs::create_dir_all(&source).unwrap();
        unpacked_pack(&source);

        let image = dir.join("games.adf");
        let (bytes, _) = ffs_volume(1760, DosType::new(*b"DOS\x01"));
        std::fs::write(&image, &bytes).unwrap();

        let folder = HostFolder::new(source.join("Turrican"), true);
        let icon = std::fs::read(source.join("Turrican.info")).unwrap();

        super::super::volume_write::with_volume(&image, 0, move |writer| -> CoreResult<()> {
            let drawer = writer.make_dir(0, "Turrican")?.block.unwrap();
            copy_into_volume(writer, drawer, &folder, OverwritePolicy::Skip, &NoProgress)?;
            writer.add_file(0, "Turrican.info", &icon, FileMeta::default())?;
            Ok(())
        })
        .unwrap();

        let siblings = std::fs::read_dir(&dir)
            .unwrap()
            .filter_map(|entry| entry.ok())
            .filter(|entry| entry.file_name().to_string_lossy().starts_with("games.adf"))
            .count();
        assert!(
            siblings <= 2,
            "the image and at most one backup, found {siblings}"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    // ---- §82, end to end ----

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

    /// The whole of §82 in one test:
    ///
    /// ```text
    /// Game.lha → WHDLoad detected → Install to HDF → Backup → Apply → Verify
    /// ```
    ///
    /// Nothing here reaches inside the install. It hands over an archive and a
    /// disk, and then reads the disk back to see what actually landed.
    #[test]
    fn a_whdload_archive_installs_onto_a_disk_and_reads_back() {
        let dir = scratch("e2e");
        let archive = dir.join("Turrican.lha");
        whdload_archive(&archive);

        let image = dir.join("Games.hdf");
        let (bytes, _) = ffs_volume(1760, DosType::new(*b"DOS\x01"));
        std::fs::write(&image, &bytes).unwrap();
        let before = std::fs::read(&image).unwrap();

        // ---- the plan, which must write nothing ----
        let plan = build_plan(&archive, &image, 0, 0).unwrap();
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
        let outcome = run_install(&archive, &image, 0, 0, &NoProgress).unwrap();

        assert_eq!(outcome.files, 3, "slave, executable, one data file");
        assert_eq!(
            outcome.verified, outcome.files,
            "every file is read back out of the disk"
        );
        assert!(outcome.icon_installed);
        assert!(outcome.skipped.is_empty(), "{:?}", outcome.skipped);
        assert!(
            outcome.backup.is_some(),
            "a floppy-sized image is backed up before it is replaced"
        );

        // ---- what is actually on the disk ----
        let entry = super::super::volume_write::pick_volume(&image, 0).unwrap();
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

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Installing the same game twice must not silently write over the first
    /// one — and the refusal has to arrive before anything is touched.
    #[test]
    fn installing_the_same_pack_twice_is_refused_and_changes_nothing() {
        let dir = scratch("e2e-twice");
        let archive = dir.join("Turrican.lha");
        whdload_archive(&archive);

        let image = dir.join("Games.hdf");
        let (bytes, _) = ffs_volume(1760, DosType::new(*b"DOS\x01"));
        std::fs::write(&image, &bytes).unwrap();

        run_install(&archive, &image, 0, 0, &NoProgress).unwrap();
        let after_first = std::fs::read(&image).unwrap();

        let err = run_install(&archive, &image, 0, 0, &NoProgress).unwrap_err();
        assert!(err.to_string().contains("already on that volume"), "{err}");
        assert_eq!(
            std::fs::read(&image).unwrap(),
            after_first,
            "a refused install must leave the image byte-for-byte unchanged"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// An archive with no slave in it is not a WHDLoad package, and copying an
    /// arbitrary folder onto someone's hard disk is not what they clicked.
    #[test]
    fn an_ordinary_archive_is_refused_before_anything_is_written() {
        use crate::core::lha::tests::make_lha_with;

        let dir = scratch("e2e-not-whd");
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

        assert!(build_plan(&archive, &image, 0, 0).is_err());
        assert!(run_install(&archive, &image, 0, 0, &NoProgress).is_err());
        assert_eq!(std::fs::read(&image).unwrap(), before);

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A pack that will not fit is refused with the numbers, before the disk is
    /// touched — not discovered half way through.
    #[test]
    fn a_pack_too_big_for_the_disk_is_refused_with_the_numbers() {
        use crate::core::lha::tests::make_lha_with;

        let dir = scratch("e2e-toobig");
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

        let plan = build_plan(&archive, &image, 0, 0).unwrap();
        let refusal = plan
            .refusal
            .expect("a pack that does not fit must be refused");
        assert!(refusal.contains("blocks"), "{refusal}");
        assert!(refusal.contains("are free"), "{refusal}");

        assert!(run_install(&archive, &image, 0, 0, &NoProgress).is_err());
        assert_eq!(std::fs::read(&image).unwrap(), before);

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Install a pack and leave the disk for `scripts/oracle-check.py`.
    ///
    /// The claim §82 makes is that the game is *installed* — not that ART can
    /// read back what ART wrote. Only an outside implementation can say
    /// whether the drawer, its contents and its icon are where AmigaOS looks
    /// for them, which is the difference between a game that appears on
    /// Workbench and one that does not.
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

        run_install(&archive, &dest, 0, 0, &NoProgress).unwrap();
        let _ = std::fs::remove_file(&archive);
    }

    // ---- refusals ----

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
        .expect("a low-confidence detection must be refused");

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
        .expect("a source pack must be refused");
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
        .expect("a taken name must be refused");
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
        .expect("a pack that does not fit must be refused");
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
            .expect("unstorable names must refuse the install");
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
        .expect("an incomplete unpack must be refused");
        assert!(reason.contains("did not unpack completely"), "{reason}");
    }
}
