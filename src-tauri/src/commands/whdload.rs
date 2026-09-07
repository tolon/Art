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
use crate::core::jobs::{JobId, ProgressSink};
use crate::core::lha::OverwritePolicy;
use crate::core::oplog::{JsonlOperationLog, OperationOutcome};
use crate::core::volume::write::copy::{copy_into_volume, CopyReport, HostFolder};
use crate::core::volume::write::FileMeta;
use crate::core::whdload::install::{
    build_plan, install_pack, VolumeSession, WhdloadOutcome, WhdloadPlan,
};
use crate::error::AppResult;

/// The event a finished install arrives on.
pub const WHDLOAD_EVENT: &str = "whdload-result";

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
        &crate::scratch::root()?,
    )?)
}

// ---------------------------------------------------------------------------
// Installing
// ---------------------------------------------------------------------------

/// The [`VolumeSession`] `core::whdload::install` writes through outside of
/// tests — ART-242's live instance of the trait-in-`core`,
/// implementation-outside-it shape (`MirrorClient`, `VolumeFormatter`,
/// `HostRecycler` are the other three). `commands/volume_write.rs::with_volume`
/// is the session/backup machinery: one volume opened, backed up once and
/// committed once for the whole install, whichever write strategy the image's
/// size calls for. It stays a command-layer helper (moving it is a round of
/// its own), so this is the thin seam that lets `core::whdload::install` use
/// it without depending on it directly.
struct CommandVolumeSession;

impl VolumeSession for CommandVolumeSession {
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
        let ((report, icon_installed), _strategy, committed) = super::volume_write::with_volume(
            image,
            volume_index,
            move |writer| -> CoreResult<(CopyReport, bool)> {
                let drawer = writer.make_dir(parent, drawer_name)?.block.ok_or_else(|| {
                    CoreError::Malformed {
                        format: "volume".into(),
                        detail: "the drawer was created but ART lost track of it".into(),
                    }
                })?;

                let report = copy_into_volume(writer, drawer, folder, OverwritePolicy::Skip, sink)?;
                if report.cancelled {
                    // §54/§57: a WHDLoad pack missing files it never got to
                    // copy is not a partial success, it is a broken,
                    // non-bootable result. Returning here — before the icon
                    // is written and before this closure returns — is what
                    // keeps the whole-file strategy from ever reaching
                    // `commit_whole_file` for it, and what keeps `spawn_job`
                    // from logging this install verified.
                    return Err(CoreError::Cancelled);
                }

                // The icon goes **beside** the drawer, not inside it. Inside,
                // it describes nothing and the game stays invisible on
                // Workbench.
                let installed = match icon {
                    Some((name, bytes)) => {
                        writer.add_file(parent, name, bytes, FileMeta::default())?;
                        true
                    }
                    None => false,
                };

                Ok((report, installed))
            },
        )?;

        Ok((report, icon_installed, committed.backup))
    }
}

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

    // Resolved here rather than inside the job: a scratch root that has
    // gone away is the user's to fix, and they should hear it from the
    // button they pressed (ART-196).
    let scratch_root = crate::scratch::root()?;
    // Same reasoning as `scratch_root`: `AppHandle` does not survive the
    // `move` into the job closure below, so the one path this install needs
    // out of it is resolved here, on the command thread, once.
    let catalogue_dir = super::gameindex::catalogue_dir(&app);

    let id = spawn_job(&app, registry, &title, move |job_id, progress| {
        let outcome = install_pack(
            &archive_path,
            &image_path,
            volume_index,
            parent,
            &scratch_root,
            &catalogue_dir,
            &CommandVolumeSession,
            progress,
        );

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

// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::jobs::NoProgress;
    use crate::core::volume::fixture::ffs_volume;
    use crate::core::volume::mount::mount;
    use crate::core::volume::DosType;
    use crate::core::whdload::install::WhdloadRefusal;

    /// The wire, in the exact shape `src/lib/whdload.ts` reads (ART-242).
    ///
    /// `run_install`'s body is about to move into `core::whdload::install`.
    /// Nothing about what the frontend receives may change in that move: the
    /// event's `kind`/`job_id`/`outcome` tag shape, and every field
    /// `WhdloadOutcome` carries. Written as JSON literals rather than built
    /// from the Rust types and read back, so a field renamed or re-typed on
    /// either side of the move shows up here rather than only in the two
    /// sides silently agreeing with each other.
    #[test]
    fn the_install_result_event_keeps_its_wire_shape() {
        let result = WhdloadResult::Installed {
            job_id: 7,
            outcome: WhdloadOutcome {
                drawer: "Games:Turrican".into(),
                files: 3,
                directories: 1,
                bytes: 4321,
                verified: 3,
                icon_installed: true,
                skipped: vec!["Extra.dat (name too long)".into()],
                igame_omitted: vec!["title (too long for igame.data)".into()],
                backup: Some("Games.hdf.bak-1".into()),
            },
        };

        let value = serde_json::to_value(&result).unwrap();
        assert_eq!(
            value,
            serde_json::json!({
                "kind": "installed",
                "job_id": 7,
                "outcome": {
                    "drawer": "Games:Turrican",
                    "files": 3,
                    "directories": 1,
                    "bytes": 4321,
                    "verified": 3,
                    "icon_installed": true,
                    "skipped": ["Extra.dat (name too long)"],
                    "igame_omitted": ["title (too long for igame.data)"],
                    "backup": "Games.hdf.bak-1"
                }
            }),
            "src/lib/whdload.ts::WhdloadResult/WhdloadOutcome must match this field for field"
        );
    }

    /// The plan's wire, the other half of what the frontend reads before it
    /// ever calls install (`whdloadPlan` in `src/lib/whdload.ts`). `refusal`
    /// serializes to `null`, never an absent field, because `hasPack()` and
    /// the panel both branch on it being present.
    ///
    /// Built from the public struct literals rather than `build_plan`'s own
    /// `verdict()`/`layout()`/`cost()` test helpers (those moved to
    /// `core::whdload::install`'s own test module with the rest of
    /// `build_plan`/`refuse`) — the wire shape is what this pins, not how a
    /// plan gets produced.
    #[test]
    fn the_plan_result_keeps_its_wire_shape() {
        use crate::core::lha::whdload::WhdloadVerdict;
        use crate::core::volume::write::plan::CopyPlan;
        use crate::core::whdload::PackLayout;

        let plan = WhdloadPlan {
            verdict: WhdloadVerdict {
                confidence: crate::core::workflow::types::Confidence::High,
                slave: Some("Game/Game.slave".into()),
                executable: Some("Game/Game".into()),
                has_data_dir: true,
                has_icon: true,
                notes: "test".into(),
            },
            layout: PackLayout {
                root: "Game".into(),
                name: "Game".into(),
                slave: "Game/Game.slave".into(),
                icon: Some("Game.info".into()),
                outside: Vec::new(),
                needs_installer: false,
            },
            drawer: "Games:Game".into(),
            volume_name: "Games".into(),
            cost: CopyPlan {
                files: 3,
                directories: 1,
                total_bytes: 4000,
                blocks_needed: 10,
                blocks_free: 1000,
                block_size: 512,
                name_problems: Vec::new(),
                collisions: Vec::new(),
                split_icons: Vec::new(),
            },
            name_taken: false,
            refusal: None,
        };

        let value = serde_json::to_value(&plan).unwrap();
        assert_eq!(value["refusal"], serde_json::Value::Null);
        assert_eq!(value["drawer"], "Games:Game");
        assert_eq!(value["name_taken"], false);
        assert_eq!(value["layout"]["name"], "Game");
        assert_eq!(value["cost"]["blocks_needed"], 10);

        // `WhdloadRefusal`'s own wire shape, from its public fields — the
        // private `plain`/`with` constructors stayed with `refuse()` in
        // `core::whdload::install`.
        let refused = WhdloadRefusal {
            reason: "no room".into(),
            suggestion: Some("free some up".into()),
        };
        let refused_value = serde_json::to_value(&refused).unwrap();
        assert_eq!(
            refused_value,
            serde_json::json!({ "reason": "no room", "suggestion": "free some up" })
        );
    }

    fn scratch(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("art-whd-{name}-{}", crate::core::test_scratch_id()));
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

    // ---- `CommandVolumeSession`, the real one ----
    //
    // Fix round 1 (review of ART-242's own move): `CommandVolumeSession` was
    // instantiated only inside `whdload_install`'s `spawn_job` closure and
    // exercised by no test. `core::whdload::install::tests::TestVolumeSession`
    // proves `install_pack`'s own plumbing, but it is a *different*
    // implementation — an in-memory `VecDevice` committed with a bare
    // `std::fs::write` — and says nothing about this one, which runs through
    // the real `with_volume` (real write-strategy selection, real
    // backup/atomic-write, real journal). Before ART-242's move,
    // `a_cancelled_run_install_writes_nothing_and_does_not_report_success`
    // drove exactly this cancellation guard through that real path; these two
    // tests are its direct analogue, restored against the code that replaced
    // it.

    /// A real `.lha` holding a real WHDLoad pack, sized so a cancellation
    /// sink has room to fire partway through the copy rather than before it
    /// starts. Local to this file rather than reused from
    /// `core::whdload::install::tests` — these tests are specifically about
    /// `CommandVolumeSession`, not the core engine's own plumbing.
    fn cancellable_whdload_archive(path: &Path) {
        use crate::core::lha::tests::make_lha_with;

        let mut entries: Vec<(String, Vec<u8>)> = vec![
            (
                "Turrican/Turrican.slave".into(),
                b"WHDLOADSLAVE\x00\x00\x00\x0a".to_vec(),
            ),
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
        std::fs::write(path, make_lha_with(&borrowed)).unwrap();
    }

    /// §54/§57 and the data-safety rule, through the **real** session this
    /// time. Cancelling partway through the copy must leave the image
    /// byte-for-byte unchanged (hashed — compared byte for byte — before and
    /// after) and must not report success. Deleting the
    /// `if report.cancelled { return Err(CoreError::Cancelled) }` guard in
    /// `CommandVolumeSession::install_drawer` makes this fall.
    ///
    /// **`StopDuringCopy` is phase-aware, and has to be.** `install_pack` runs
    /// two per-entry loops that report `Some(total)`: unpacking the archive
    /// (`extract_with_backend`, `core/archive/extract.rs`) *and* copying into
    /// the volume (`copy_into_volume`) — both report `done == 0` at their
    /// first entry and both check `is_cancelled()` between entries. A sink
    /// armed on a bare `done >= N` fires during the **first** such phase every
    /// time, because unpacking always runs first — which is a real survivor
    /// found while building this very test: the original threshold cancelled
    /// during unpacking, before `CommandVolumeSession` was ever reached, so
    /// removing its guard changed nothing. Counting phase boundaries (a
    /// `done == 0` report marks a new one) and arming only once the **second**
    /// phase is under way is what actually reaches the copy.
    #[test]
    fn a_cancelled_install_through_the_real_session_writes_nothing() {
        use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

        struct StopDuringCopy {
            phase: AtomicUsize,
            cancel: AtomicBool,
        }
        impl ProgressSink for StopDuringCopy {
            fn report(&self, done: u64, total: Option<u64>, _message: &str) {
                if total.is_none() {
                    return;
                }
                if done == 0 {
                    self.phase.fetch_add(1, Ordering::SeqCst);
                }
                if self.phase.load(Ordering::SeqCst) >= 2 && done >= 2 {
                    self.cancel.store(true, Ordering::SeqCst);
                }
            }
            fn is_cancelled(&self) -> bool {
                self.cancel.load(Ordering::SeqCst)
            }
        }

        let dir = crate::core::ScratchDir::new("art-whd-cmd", "cancel");
        let archive = dir.join("Turrican.lha");
        cancellable_whdload_archive(&archive);

        let image = dir.join("Games.hdf");
        let (bytes, _) = ffs_volume(1760, DosType::new(*b"DOS\x01"));
        std::fs::write(&image, &bytes).unwrap();
        let before = std::fs::read(&image).unwrap();

        let sink = StopDuringCopy {
            phase: AtomicUsize::new(0),
            cancel: AtomicBool::new(false),
        };
        let err = install_pack(
            &archive,
            &image,
            0,
            0,
            dir.path(),
            dir.path(),
            &CommandVolumeSession,
            &sink,
        )
        .expect_err("a cancelled install through the real session must not report success");

        assert_eq!(
            err.code(),
            "ART-CANCELLED",
            "the job must end Cancelled, not Completed: {err}"
        );
        assert_eq!(
            std::fs::read(&image).unwrap(),
            before,
            "a cancelled install through the real with_volume session must leave the \
             image byte-for-byte unchanged"
        );
    }

    /// The happy path through the same real session, once — so `with_volume`
    /// itself (write-strategy selection, backup, atomic commit) is exercised
    /// end to end by this file, not only by the core engine's own
    /// `TestVolumeSession` double.
    #[test]
    fn install_pack_through_the_real_session_installs_and_reads_back() {
        use crate::core::lha::tests::make_lha_with;

        let dir = crate::core::ScratchDir::new("art-whd-cmd", "happy");
        let archive = dir.join("Turrican.lha");
        std::fs::write(
            &archive,
            make_lha_with(&[
                ("Turrican/Turrican.slave", b"WHDLOADSLAVE\x00\x00\x00\x0a"),
                ("Turrican/Turrican", b"host executable bytes"),
                ("Turrican/data/level1.bin", &vec![7u8; 4000]),
                ("Turrican.info", b"\xe3\x10\x00\x01icon"),
            ]),
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
            dir.path(),
            dir.path(),
            &CommandVolumeSession,
            &NoProgress,
        )
        .unwrap();

        assert_eq!(
            outcome.files, 4,
            "slave, executable, one data file, and igame.data (uncatalogued, title only)"
        );
        assert_eq!(
            outcome.verified, outcome.files,
            "every file is read back out of the disk"
        );
        assert!(outcome.icon_installed);
        assert!(
            outcome.backup.is_some(),
            "the real session backs up a floppy-sized image before replacing it"
        );

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
        assert!(
            root.iter()
                .any(|found| found.name == "Turrican.info" && !found.is_dir),
            "the icon must sit beside the drawer, read back through the real session"
        );
    }
}
