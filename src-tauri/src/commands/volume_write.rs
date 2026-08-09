//! Writing into a volume — the commands behind F5 … F8 (brief §5, §8).
//!
//! Thin adapters, as everywhere: deserialize, open the right device for the
//! image's size, call `core/volume/write`, serialize back. The two decisions
//! that live here rather than in core are shell decisions:
//!
//! - **Which write strategy.** An image of 16 MiB or less is read whole,
//!   mutated in memory, validated and written back atomically through
//!   `core/safety` — the existing ADF pipeline, unchanged. Anything larger is
//!   written in place under the undo journal. Callers never see the difference
//!   (§1).
//! - **What runs as a job.** A single rename or mkdir is fast and runs inline;
//!   a multi-file copy runs on the Job Queue with cancel between files (§54).
//!
//! ## Recovery comes first
//!
//! Every write opens through [`with_writer`], which refuses outright when an
//! unfinished operation's journal is sitting next to the image. Writing over a
//! half-written volume would make the journal unusable and lose the only route
//! back to the state before the crash.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, State};

use super::jobs::{spawn_job, JobRegistry};
use super::oplog::{user_operation, write_result};
use crate::core::error::{CoreError, CoreResult};
use crate::core::jobs::{JobId, ProgressSink};
use crate::core::lha::OverwritePolicy;
use crate::core::oplog::{JsonlOperationLog, OperationOutcome};
use crate::core::safety::{guarded_write, BackupPolicy};
use crate::core::volume::device::{FileRegionMut, VecDevice};
use crate::core::volume::journal::find_journal;
use crate::core::volume::mount::{mount, scan_image, VolumeEntry};
use crate::core::volume::write::copy::{
    copy_into_volume, extract_from_volume, CopyReport, ExtractReport, HostFolder, StagedTree,
};
use crate::core::volume::write::plan::{plan_copy, CopyPlan, SourceEntry};
use crate::core::volume::write::{VolumeWriter, WriteOutcome};
use crate::core::volume::{read_block_vec, BlockDeviceMut, VolumeGeometry, WriteStrategy};
use crate::error::{AppError, AppResult};

/// The event background volume-write results arrive on.
pub const VOLUME_WRITE_EVENT: &str = "volume-write-result";

/// What a write command reports back.
#[derive(Debug, Clone, Serialize)]
pub struct MutationResult {
    /// The header block of what was created, when something was.
    pub block: Option<u32>,
    pub blocks_touched: usize,
    pub free_blocks: usize,
    pub free_bytes: u64,
    pub verified: bool,
    /// Which strategy ran, for Power User Mode. Beginner never sees it.
    pub strategy: String,
    /// Where the previous image went, for the whole-file strategy.
    pub backup: Option<String>,
}

/// An unfinished operation found next to an image.
#[derive(Debug, Clone, Serialize)]
pub struct RecoveryResult {
    pub description: String,
    pub blocks_restored: usize,
    /// The journal ended mid-entry, so no image write had started.
    pub was_truncated: bool,
}

fn describe(strategy: WriteStrategy) -> &'static str {
    match strategy {
        WriteStrategy::WholeFile => "whole-file",
        WriteStrategy::BlockJournal => "block-journal",
    }
}

// ---------------------------------------------------------------------------
// The shared open-write-commit path
// ---------------------------------------------------------------------------

/// Open a volume for writing and run one operation against it.
///
/// This is where the two strategies diverge and rejoin. Whichever ran, the
/// caller gets a [`MutationResult`] and the guarantee that either the whole
/// operation landed or none of it did.
fn with_writer<F>(
    image: &Path,
    volume_index: usize,
    run: F,
) -> CoreResult<(WriteOutcome, WriteStrategy, Option<String>)>
where
    F: FnOnce(&mut VolumeWriter<'_>) -> CoreResult<WriteOutcome>,
{
    with_volume(image, volume_index, run)
}

/// Open a volume for writing and run a whole **session** against it.
///
/// The same open-and-finalise as [`with_writer`], but the closure may perform
/// several operations and return anything. That matters for the whole-file
/// strategy: three operations opened separately would read, back up and
/// replace the image three times, burning three backup generations for what
/// the user asked for once.
///
/// Each operation inside is still individually journalled and validated — this
/// only shares the device and the single write-back at the end.
pub fn with_volume<F, T>(
    image: &Path,
    volume_index: usize,
    run: F,
) -> CoreResult<(T, WriteStrategy, Option<String>)>
where
    F: FnOnce(&mut VolumeWriter<'_>) -> CoreResult<T>,
{
    let entry = pick(image, volume_index)?;
    let geometry = geometry_of(image, &entry)?;

    // An unfinished operation blocks every write. Overwriting a half-written
    // volume would leave its journal describing blocks that no longer hold
    // what it recorded — the one state from which nothing can be recovered.
    if let Some(pending) = find_journal(image)? {
        return Err(CoreError::SafetyRefused(format!(
            "'{}' has an unfinished operation ({}) waiting to be undone. \
             Recover it first — ART will not write over it.",
            image.display(),
            pending.header().description
        )));
    }

    let bytes = std::fs::metadata(image)?.len();
    let strategy = WriteStrategy::for_image(bytes);

    match strategy {
        WriteStrategy::WholeFile => {
            // Read whole, mutate in memory, then one atomic guarded write.
            // Nothing reaches the user's file until the operation validated.
            let original = std::fs::read(image)?;
            let mut device = VecDevice::new(original.clone(), geometry.block_size)?;

            let outcome = {
                let mut writer = VolumeWriter::open(&mut device, geometry, image, 0)?;
                run(&mut writer)?
            };

            let backup = guarded_write(image, device.bytes(), BackupPolicy::DISK_IMAGE)?;
            Ok((
                outcome,
                strategy,
                backup.map(|path| path.display().to_string()),
            ))
        }
        WriteStrategy::BlockJournal => {
            let mut device = FileRegionMut::open(
                image,
                entry.byte_offset,
                entry.byte_length,
                entry.block_size,
            )?;
            let outcome = {
                let mut writer =
                    VolumeWriter::open(&mut device, geometry, image, entry.byte_offset)?;
                run(&mut writer)?
            };
            device.sync()?;
            // No backup path: the journal is the way back, and it has already
            // been deleted by a successful commit.
            Ok((outcome, strategy, None))
        }
    }
}

/// The geometry of one volume, as the writer needs it.
///
/// Taken from what the device actually covers rather than what the partition
/// table claimed: a clamped partition must not be written as if the missing
/// blocks were there.
fn geometry_of(image: &Path, entry: &VolumeEntry) -> CoreResult<VolumeGeometry> {
    let (_, geometry) = mount(image, entry)?;
    Ok(geometry)
}

/// Find the volume the frontend asked for.
///
/// Public so the checkout commands can reach the same volume by the same index
/// the pane is showing, rather than scanning the image a second way and
/// possibly disagreeing about which partition is which.
pub fn pick_volume(image: &Path, index: usize) -> CoreResult<VolumeEntry> {
    pick(image, index)
}

fn pick(image: &Path, index: usize) -> CoreResult<VolumeEntry> {
    let found = scan_image(image)?;
    found.volumes.get(index).cloned().ok_or_else(|| {
        CoreError::InvalidInput(format!(
            "this image has no volume {index} ({} found)",
            found.volumes.len()
        ))
    })
}

fn result_of(
    outcome: WriteOutcome,
    strategy: WriteStrategy,
    backup: Option<String>,
    block_size: usize,
) -> MutationResult {
    MutationResult {
        block: outcome.block,
        blocks_touched: outcome.blocks_touched,
        free_blocks: outcome.free_blocks,
        free_bytes: outcome.free_blocks as u64 * block_size as u64,
        verified: outcome.verified,
        strategy: describe(strategy).into(),
        backup,
    }
}

// ---------------------------------------------------------------------------
// Inline operations — fast enough not to need a job (§8)
// ---------------------------------------------------------------------------

/// F7 — create a folder inside a volume.
#[tauri::command]
pub fn volume_make_dir(
    path: String,
    volume_index: usize,
    dir_block: Option<u32>,
    name: String,
    oplog: State<'_, JsonlOperationLog>,
) -> AppResult<MutationResult> {
    let image = PathBuf::from(path.trim());
    let parent = dir_block.unwrap_or(0);

    let result = with_writer(&image, volume_index, |writer| {
        writer.make_dir(parent, name.trim())
    })
    .map(|(outcome, strategy, backup)| {
        let block_size = outcome_block_size(&image, volume_index);
        result_of(outcome, strategy, backup, block_size)
    })
    .map_err(AppError::from);

    write_result(
        &oplog,
        user_operation("New folder in volume")
            .destination(format!("{path}:{name}"))
            .detail("Volume", volume_index.to_string()),
        &result,
        |record, made: &MutationResult| {
            record
                .detail("Blocks touched", made.blocks_touched.to_string())
                .detail("Strategy", made.strategy.clone())
                .outcome(OperationOutcome::verified(made.verified))
        },
    );

    result
}

/// F6 — rename an entry, or move it to another folder in the same volume.
#[tauri::command]
pub fn volume_rename(
    path: String,
    volume_index: usize,
    dir_block: Option<u32>,
    entry_block: u32,
    new_name: String,
    to_dir_block: Option<u32>,
    oplog: State<'_, JsonlOperationLog>,
) -> AppResult<MutationResult> {
    let image = PathBuf::from(path.trim());
    let from = dir_block.unwrap_or(0);
    let to = to_dir_block.unwrap_or(from);
    let wanted = new_name.trim().to_string();

    let result = with_writer(&image, volume_index, |writer| {
        if to == from {
            writer.rename(from, entry_block, &wanted)
        } else {
            writer.relink(from, entry_block, to, &wanted)
        }
    })
    .map(|(outcome, strategy, backup)| {
        let block_size = outcome_block_size(&image, volume_index);
        result_of(outcome, strategy, backup, block_size)
    })
    .map_err(AppError::from);

    write_result(
        &oplog,
        user_operation(if to == from {
            "Rename in volume"
        } else {
            "Move in volume"
        })
        .source(format!("{path}:{entry_block}"))
        .destination(new_name.clone()),
        &result,
        |record, made: &MutationResult| {
            record
                .detail("Blocks touched", made.blocks_touched.to_string())
                .detail("Strategy", made.strategy.clone())
                .outcome(OperationOutcome::verified(made.verified))
        },
    );

    result
}

/// F8 — delete an entry.
///
/// `Destructive` (§63): the frontend double-confirms before calling this. A
/// directory must be empty, so deleting a tree is the caller's loop and each
/// entry is its own journalled operation.
#[tauri::command]
pub fn volume_delete(
    path: String,
    volume_index: usize,
    dir_block: Option<u32>,
    entry_block: u32,
    oplog: State<'_, JsonlOperationLog>,
) -> AppResult<MutationResult> {
    let image = PathBuf::from(path.trim());
    let parent = dir_block.unwrap_or(0);

    let result = with_writer(&image, volume_index, |writer| {
        writer.delete(parent, entry_block)
    })
    .map(|(outcome, strategy, backup)| {
        let block_size = outcome_block_size(&image, volume_index);
        result_of(outcome, strategy, backup, block_size)
    })
    .map_err(AppError::from);

    write_result(
        &oplog,
        user_operation("Delete from volume").source(format!("{path}:{entry_block}")),
        &result,
        |record, made: &MutationResult| {
            record
                .detail("Blocks touched", made.blocks_touched.to_string())
                .detail("Strategy", made.strategy.clone())
                .outcome(OperationOutcome::verified(made.verified))
        },
    );

    result
}

/// Write one file into a volume — the single-file fast path of F5.
#[tauri::command]
pub fn volume_put_file(
    path: String,
    volume_index: usize,
    dir_block: Option<u32>,
    source: String,
    name: Option<String>,
    overwrite: Option<OverwritePolicy>,
    oplog: State<'_, JsonlOperationLog>,
) -> AppResult<MutationResult> {
    let image = PathBuf::from(path.trim());
    let source_path = PathBuf::from(source.trim());
    let parent = dir_block.unwrap_or(0);

    let chosen = name.unwrap_or_else(|| {
        source_path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default()
    });

    let result = (|| -> CoreResult<(WriteOutcome, WriteStrategy, Option<String>)> {
        let size = std::fs::metadata(&source_path)?.len();
        if size > crate::core::volume::write::copy::MAX_COPY_FILE_BYTES {
            return Err(CoreError::InvalidInput(format!(
                "'{}' is {size} bytes, more than ART copies in one go",
                source_path.display()
            )));
        }
        let data = std::fs::read(&source_path)?;

        with_writer(&image, volume_index, |writer| {
            if let Some(existing) = writer.find(parent, &chosen)? {
                match overwrite.unwrap_or_default() {
                    OverwritePolicy::Skip => {
                        return Err(CoreError::InvalidInput(format!(
                            "'{chosen}' is already there. Choose Replace or Keep both."
                        )))
                    }
                    OverwritePolicy::Overwrite => {
                        writer.delete(parent, existing.block)?;
                    }
                    OverwritePolicy::Rename => {
                        return Err(CoreError::InvalidInput(format!(
                            "'{chosen}' is already there. Pick a different name."
                        )))
                    }
                }
            }
            writer.add_file(parent, &chosen, &data, Default::default())
        })
    })()
    .map(|(outcome, strategy, backup)| {
        let block_size = outcome_block_size(&image, volume_index);
        result_of(outcome, strategy, backup, block_size)
    })
    .map_err(AppError::from);

    write_result(
        &oplog,
        user_operation("Copy file into volume")
            .source(source.clone())
            .destination(format!("{path}:{}", volume_index)),
        &result,
        |record, made: &MutationResult| {
            record
                .detail("Blocks touched", made.blocks_touched.to_string())
                .detail("Strategy", made.strategy.clone())
                .outcome(OperationOutcome::verified(made.verified))
        },
    );

    result
}

/// Replace a file's contents in place — the checkin half of F4 (§6).
///
/// Delete then create, in two journalled operations rather than one: growing a
/// file means a different set of data blocks, and freeing the old ones before
/// allocating the new ones is what lets an edit that grows a file still fit on
/// a nearly full disk.
///
/// The delete is safe to do first because the bytes are already in memory —
/// they came from the temp file, not from the image.
pub fn replace_file(
    image: &Path,
    volume_index: usize,
    dir_block: u32,
    entry_block: u32,
    name: &str,
    data: &[u8],
) -> CoreResult<MutationResult> {
    let (outcome, strategy, backup) = with_writer(image, volume_index, |writer| {
        writer.delete(dir_block, entry_block)?;
        writer.add_file(dir_block, name, data, Default::default())
    })?;

    Ok(result_of(
        outcome,
        strategy,
        backup,
        outcome_block_size(image, volume_index),
    ))
}

/// The block size of a volume, for turning free blocks into free bytes.
///
/// Falls back to 512 rather than failing: the number is for display, and an
/// operation that already succeeded must not be reported as failed because a
/// second scan of the image did not work.
fn outcome_block_size(image: &Path, volume_index: usize) -> usize {
    pick(image, volume_index)
        .map(|entry| entry.block_size)
        .unwrap_or(crate::core::volume::SECTOR_BYTES)
}

// ---------------------------------------------------------------------------
// Pre-flight
// ---------------------------------------------------------------------------

/// What copying a folder into a volume would cost (§3.3).
///
/// Read-only. The frontend shows this and only then calls
/// [`volume_copy_in`] — *explain before modify*.
#[tauri::command]
pub fn volume_plan_copy(
    path: String,
    volume_index: usize,
    dir_block: Option<u32>,
    source: String,
) -> AppResult<CopyPlan> {
    let image = PathBuf::from(path.trim());
    let entry = pick(&image, volume_index)?;
    let (device, geometry) = mount(&image, &entry)?;

    let folder = HostFolder::new(PathBuf::from(source.trim()), true);
    let entries: Vec<SourceEntry> = {
        use crate::core::volume::write::copy::CopySource;
        folder.entries()?
    };

    let dir = dir_block.unwrap_or(geometry.root_block);
    let existing: Vec<String> = {
        let set = crate::core::volume::write::layout::BlockSet::new(geometry.block_size);
        crate::core::volume::write::dir::entries_in(&device, &set, &geometry, dir)?
            .into_iter()
            .map(|entry| entry.name)
            .collect()
    };

    Ok(plan_copy(&device, &geometry, &entries, &existing)?)
}

/// Whether ART can write to this volume, and why not when it cannot (§3.4).
#[derive(Debug, Clone, Serialize)]
pub struct WriteCapability {
    pub writable: bool,
    /// Never null when `writable` is false.
    pub reason: Option<String>,
    pub strategy: String,
    pub free_blocks: usize,
    pub free_bytes: u64,
    pub block_size: usize,
    pub volume_name: String,
    pub filesystem: String,
    /// An unfinished operation blocking every write.
    pub pending_recovery: Option<String>,
}

/// What the pane footer shows, and whether F5–F8 are offered (§8).
#[tauri::command]
pub fn volume_write_capability(path: String, volume_index: usize) -> AppResult<WriteCapability> {
    let image = PathBuf::from(path.trim());
    let entry = pick(&image, volume_index)?;

    let pending_recovery =
        find_journal(&image)?.map(|journal| journal.header().description.clone());

    // A volume ART cannot even mount cannot be written either, and the reason
    // is the more specific of the two.
    let Ok((device, geometry)) = mount(&image, &entry) else {
        return Ok(WriteCapability {
            writable: false,
            reason: Some(
                entry
                    .unsupported
                    .clone()
                    .unwrap_or_else(|| "ART cannot open this volume".into()),
            ),
            strategy: describe(WriteStrategy::for_image(
                std::fs::metadata(&image).map(|m| m.len()).unwrap_or(0),
            ))
            .into(),
            free_blocks: 0,
            free_bytes: 0,
            block_size: entry.block_size,
            volume_name: entry.name.clone(),
            filesystem: entry.filesystem.clone(),
            pending_recovery,
        });
    };

    let refusal = crate::core::volume::write::write_refusal(&geometry);

    // Free space needs a valid bitmap. When there is not one, the refusal
    // says so and the footer shows no number rather than a wrong one.
    let free_blocks = crate::core::volume::write::bitmap::Allocator::load(&device, &geometry)
        .map(|allocator| allocator.free_count())
        .unwrap_or(0);

    let bytes = std::fs::metadata(&image)?.len();
    let volume_name = read_volume_name(&device, &geometry).unwrap_or_else(|| entry.name.clone());

    Ok(WriteCapability {
        writable: refusal.is_none() && pending_recovery.is_none(),
        reason: refusal.or_else(|| {
            pending_recovery.as_ref().map(|description| {
                format!("an unfinished operation ({description}) is waiting to be undone")
            })
        }),
        strategy: describe(WriteStrategy::for_image(bytes)).into(),
        free_blocks,
        free_bytes: free_blocks as u64 * geometry.block_size as u64,
        block_size: geometry.block_size,
        volume_name,
        filesystem: entry.filesystem.clone(),
        pending_recovery,
    })
}

fn read_volume_name(
    device: &dyn crate::core::volume::BlockDevice,
    geometry: &VolumeGeometry,
) -> Option<String> {
    let block = read_block_vec(device, geometry.root_block).ok()?;
    let root = crate::core::adf::blocks::RootBlock::parse(&block).ok()?;
    (!root.volume_name.is_empty()).then_some(root.volume_name)
}

// ---------------------------------------------------------------------------
// Recovery
// ---------------------------------------------------------------------------

/// Undo an operation that did not finish (§2).
///
/// `apply` false discards the journal and leaves the image exactly as it is —
/// for a stale journal the user has decided about. Both are deliberate acts;
/// neither happens on its own.
#[tauri::command]
pub fn volume_recover(
    path: String,
    apply: bool,
    oplog: State<'_, JsonlOperationLog>,
) -> AppResult<Option<RecoveryResult>> {
    let image = PathBuf::from(path.trim());

    let result = (|| -> CoreResult<Option<RecoveryResult>> {
        let Some(journal) = find_journal(&image)? else {
            return Ok(None);
        };
        if !apply {
            journal.discard()?;
            return Ok(None);
        }
        let report = journal.roll_back()?;
        Ok(Some(RecoveryResult {
            description: report.description,
            blocks_restored: report.blocks_restored,
            was_truncated: report.was_truncated,
        }))
    })()
    .map_err(AppError::from);

    write_result(
        &oplog,
        user_operation(if apply {
            "Undo an unfinished operation"
        } else {
            "Discard an unfinished operation's journal"
        })
        .source(path.clone()),
        &result,
        |record, recovered: &Option<RecoveryResult>| match recovered {
            Some(report) => record
                .detail("Blocks restored", report.blocks_restored.to_string())
                .detail("Operation", report.description.clone())
                .outcome(OperationOutcome::verified(true)),
            None => record.outcome(OperationOutcome::verified(true)),
        },
    );

    result
}

// ---------------------------------------------------------------------------
// Jobs — anything that touches many files (§54)
// ---------------------------------------------------------------------------

/// What a background volume-write job produced.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum VolumeWriteResult {
    CopyIn {
        job_id: JobId,
        report: CopyReport,
        backup: Option<String>,
    },
    CopyOut {
        job_id: JobId,
        report: ExtractReport,
    },
}

/// Options a copy carries from the UI.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct CopyOptions {
    pub overwrite: Option<OverwritePolicy>,
    /// Write `.uaem` sidecars on the way out. Default on in Power User Mode,
    /// off in Beginner (§4.2).
    pub sidecars: Option<bool>,
}

/// F5 — copy a folder from the user's disk into a volume.
///
/// One journalled operation per file, all of them under one job, so cancelling
/// leaves a consistent volume and a report that says how many landed.
#[tauri::command]
// A Tauri command's parameters are its wire protocol: the names here are the
// keys the frontend sends. Folding them into a struct to satisfy the argument
// count would move the same fields one level down and make the call site
// longer, not shorter.
#[allow(clippy::too_many_arguments)]
pub fn volume_copy_in(
    path: String,
    volume_index: usize,
    dir_block: Option<u32>,
    source: String,
    options: Option<CopyOptions>,
    app: AppHandle,
    registry: State<'_, Arc<JobRegistry>>,
    oplog: State<'_, JsonlOperationLog>,
) -> AppResult<JobId> {
    let image = PathBuf::from(path.trim());
    let source_path = PathBuf::from(source.trim());
    let options = options.unwrap_or_default();
    let policy = options.overwrite.unwrap_or_default();
    let parent = dir_block.unwrap_or(0);

    let log_path = oplog.path().to_path_buf();
    let registry = Arc::clone(&registry);
    let emit_app = app.clone();
    let title = format!("Copying into {}", image.display());

    let id = spawn_job(&app, registry, &title, move |job_id, progress| {
        let folder = HostFolder::new(&source_path, options.sidecars.unwrap_or(true));

        let outcome = run_copy_in_folder(&image, volume_index, parent, &folder, policy, progress);

        let record = user_operation("Copy folder into volume")
            .source(source_path.display().to_string())
            .destination(format!("{}:{volume_index}", image.display()));
        let record = match &outcome {
            Ok((report, _)) => record
                .detail("Files", report.files_copied.to_string())
                .detail("Folders", report.directories_created.to_string())
                .detail("Skipped", report.skipped.len().to_string())
                .outcome(OperationOutcome::verified(
                    report.files_verified == report.files_copied,
                )),
            Err(err) => record.failed(err),
        };
        super::oplog::write_to_path(&log_path, &record);

        let (report, backup) = outcome?;
        let _ = emit_app.emit(
            VOLUME_WRITE_EVENT,
            VolumeWriteResult::CopyIn {
                job_id,
                report,
                backup,
            },
        );
        Ok(())
    });

    Ok(id)
}

/// The copy itself, with the strategy chosen the same way single operations
/// choose it.
///
/// The whole-file branch keeps the image in memory for the whole batch and
/// writes once at the end: a hundred files means a hundred journalled
/// operations against the buffer and **one** backup, rather than a hundred
/// generational backups of the same floppy.
pub fn run_copy_in_folder(
    image: &Path,
    volume_index: usize,
    parent: u32,
    folder: &HostFolder,
    policy: OverwritePolicy,
    progress: &dyn ProgressSink,
) -> CoreResult<(CopyReport, Option<String>)> {
    let entry = pick(image, volume_index)?;
    let geometry = geometry_of(image, &entry)?;

    if let Some(pending) = find_journal(image)? {
        return Err(CoreError::SafetyRefused(format!(
            "'{}' has an unfinished operation ({}) waiting to be undone.",
            image.display(),
            pending.header().description
        )));
    }

    let bytes = std::fs::metadata(image)?.len();
    match WriteStrategy::for_image(bytes) {
        WriteStrategy::WholeFile => {
            let original = std::fs::read(image)?;
            let mut device = VecDevice::new(original, geometry.block_size)?;
            let report = {
                let mut writer = VolumeWriter::open(&mut device, geometry, image, 0)?;
                copy_into_volume(&mut writer, parent, folder, policy, progress)?
            };
            let backup = guarded_write(image, device.bytes(), BackupPolicy::DISK_IMAGE)?;
            Ok((report, backup.map(|p| p.display().to_string())))
        }
        WriteStrategy::BlockJournal => {
            let mut device = FileRegionMut::open(
                image,
                entry.byte_offset,
                entry.byte_length,
                entry.block_size,
            )?;
            let report = {
                let mut writer =
                    VolumeWriter::open(&mut device, geometry, image, entry.byte_offset)?;
                copy_into_volume(&mut writer, parent, folder, policy, progress)?
            };
            device.sync()?;
            Ok((report, None))
        }
    }
}

/// Refuse a copy into a volume before anything is written, when it will not
/// fit — with the real block numbers, the way [`volume_plan_copy`] already
/// explains a copy to the user before [`volume_copy_in`] runs it (§3.3, §92).
///
/// The install commands call this before [`run_copy_in_folder`] rather than
/// discovering the disk is full mid-copy: an install that only half lands —
/// a WHDLoad pack missing its `.slave`, say — is not a partial success, it is
/// a broken result the user was never warned about. `run_copy_in_folder`
/// itself stays as it is for the general-purpose F5 copy, which is allowed to
/// land what fits and report the rest (already proven by
/// `core::volume::write::copy`'s own tests); an install is not that — it
/// promises "this works" or "this was refused", nothing in between.
pub fn plan_copy_in_folder(
    image: &Path,
    volume_index: usize,
    parent: u32,
    folder: &HostFolder,
) -> CoreResult<CopyPlan> {
    let entry = pick(image, volume_index)?;
    let (device, geometry) = mount(image, &entry)?;

    let entries: Vec<SourceEntry> = {
        use crate::core::volume::write::copy::CopySource;
        folder.entries()?
    };

    // `0` means the root everywhere a `parent`/`dir_block` is taken
    // (`VolumeWriter::resolve_directory`), but `entries_in` takes a raw block
    // number and does not do that translation itself.
    let dir = if parent == 0 {
        geometry.root_block
    } else {
        parent
    };
    let existing: Vec<String> = {
        let set = crate::core::volume::write::layout::BlockSet::new(geometry.block_size);
        crate::core::volume::write::dir::entries_in(&device, &set, &geometry, dir)?
            .into_iter()
            .map(|entry| entry.name)
            .collect()
    };

    plan_copy(&device, &geometry, &entries, &existing)
}

/// F5 the other way — copy a folder out of a volume onto the user's disk.
#[tauri::command]
// A Tauri command's parameters are its wire protocol: the names here are the
// keys the frontend sends. Folding them into a struct to satisfy the argument
// count would move the same fields one level down and make the call site
// longer, not shorter.
#[allow(clippy::too_many_arguments)]
pub fn volume_copy_out(
    path: String,
    volume_index: usize,
    dir_block: Option<u32>,
    dest: String,
    options: Option<CopyOptions>,
    app: AppHandle,
    registry: State<'_, Arc<JobRegistry>>,
    oplog: State<'_, JsonlOperationLog>,
) -> AppResult<JobId> {
    let image = PathBuf::from(path.trim());
    let destination = PathBuf::from(dest.trim());
    let options = options.unwrap_or_default();
    let policy = options.overwrite.unwrap_or_default();
    let sidecars = options.sidecars.unwrap_or(true);
    let parent = dir_block.unwrap_or(0);

    let log_path = oplog.path().to_path_buf();
    let registry = Arc::clone(&registry);
    let emit_app = app.clone();
    let title = format!("Copying out of {}", image.display());

    let id = spawn_job(&app, registry, &title, move |job_id, progress| {
        let outcome = (|| -> CoreResult<ExtractReport> {
            let entry = pick(&image, volume_index)?;
            let (device, geometry) = mount(&image, &entry)?;
            extract_from_volume(
                &device,
                &geometry,
                parent,
                &destination,
                sidecars,
                policy,
                progress,
            )
        })();

        let record = user_operation("Copy folder out of volume")
            .source(format!("{}:{volume_index}", image.display()))
            .destination(destination.display().to_string());
        let record = match &outcome {
            Ok(report) => record
                .detail("Files", report.files_written.to_string())
                .detail("Folders", report.directories_created.to_string())
                .detail("Sidecars", report.sidecars_written.to_string())
                .outcome(OperationOutcome::verified(report.is_complete())),
            Err(err) => record.failed(err),
        };
        super::oplog::write_to_path(&log_path, &record);

        let report = outcome?;
        let _ = emit_app.emit(
            VOLUME_WRITE_EVENT,
            VolumeWriteResult::CopyOut { job_id, report },
        );
        Ok(())
    });

    Ok(id)
}

/// Copy a folder from one image into another (§4.3).
///
/// Staged through a temp folder rather than streamed: the images can be
/// gigabytes, and both halves — extract-and-verify, then insert-and-verify —
/// are already the tested paths above.
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub fn volume_copy_between(
    from_path: String,
    from_volume: usize,
    from_dir: Option<u32>,
    to_path: String,
    to_volume: usize,
    to_dir: Option<u32>,
    options: Option<CopyOptions>,
    app: AppHandle,
    registry: State<'_, Arc<JobRegistry>>,
    oplog: State<'_, JsonlOperationLog>,
) -> AppResult<JobId> {
    let source_image = PathBuf::from(from_path.trim());
    let target_image = PathBuf::from(to_path.trim());
    let policy = options.unwrap_or_default().overwrite.unwrap_or_default();

    if source_image == target_image {
        return Err(CoreError::InvalidInput(
            "that is the same image on both sides. Use Move or Rename instead.".into(),
        )
        .into());
    }

    // Staged trees live in the app cache directory, so a copy that is
    // interrupted leaves its temp folder somewhere the OS and the user both
    // expect to find scratch files.
    let cache = {
        use tauri::Manager;
        app.path()
            .app_cache_dir()
            .unwrap_or_else(|_| std::env::temp_dir())
    };
    let log_path = oplog.path().to_path_buf();
    let registry = Arc::clone(&registry);
    let emit_app = app.clone();
    let title = format!("Copying between {} and {}", from_path, to_path);

    let id = spawn_job(&app, registry, &title, move |job_id, progress| {
        let outcome = (|| -> CoreResult<(CopyReport, Option<String>)> {
            let entry = pick(&source_image, from_volume)?;
            let (device, geometry) = mount(&source_image, &entry)?;
            let staged =
                StagedTree::stage(&device, &geometry, from_dir.unwrap_or(0), &cache, progress)?;

            run_copy_in_staged(
                &target_image,
                to_volume,
                to_dir.unwrap_or(0),
                &staged,
                policy,
                progress,
            )
        })();

        let record = user_operation("Copy between volumes")
            .source(format!("{from_path}:{from_volume}"))
            .destination(format!("{to_path}:{to_volume}"));
        let record = match &outcome {
            Ok((report, _)) => record
                .detail("Files", report.files_copied.to_string())
                .outcome(OperationOutcome::verified(
                    report.files_verified == report.files_copied,
                )),
            Err(err) => record.failed(err),
        };
        super::oplog::write_to_path(&log_path, &record);

        let (report, backup) = outcome?;
        let _ = emit_app.emit(
            VOLUME_WRITE_EVENT,
            VolumeWriteResult::CopyIn {
                job_id,
                report,
                backup,
            },
        );
        Ok(())
    });

    Ok(id)
}

/// The insert half of a volume-to-volume copy.
fn run_copy_in_staged(
    image: &Path,
    volume_index: usize,
    parent: u32,
    staged: &StagedTree,
    policy: OverwritePolicy,
    progress: &dyn ProgressSink,
) -> CoreResult<(CopyReport, Option<String>)> {
    let entry = pick(image, volume_index)?;
    let geometry = geometry_of(image, &entry)?;

    if let Some(pending) = find_journal(image)? {
        return Err(CoreError::SafetyRefused(format!(
            "'{}' has an unfinished operation ({}) waiting to be undone.",
            image.display(),
            pending.header().description
        )));
    }

    let bytes = std::fs::metadata(image)?.len();
    match WriteStrategy::for_image(bytes) {
        WriteStrategy::WholeFile => {
            let original = std::fs::read(image)?;
            let mut device = VecDevice::new(original, geometry.block_size)?;
            let report = {
                let mut writer = VolumeWriter::open(&mut device, geometry, image, 0)?;
                copy_into_volume(&mut writer, parent, staged.source(), policy, progress)?
            };
            let backup = guarded_write(image, device.bytes(), BackupPolicy::DISK_IMAGE)?;
            Ok((report, backup.map(|p| p.display().to_string())))
        }
        WriteStrategy::BlockJournal => {
            let mut device = FileRegionMut::open(
                image,
                entry.byte_offset,
                entry.byte_length,
                entry.block_size,
            )?;
            let report = {
                let mut writer =
                    VolumeWriter::open(&mut device, geometry, image, entry.byte_offset)?;
                copy_into_volume(&mut writer, parent, staged.source(), policy, progress)?
            };
            device.sync()?;
            Ok((report, None))
        }
    }
}

/// Write bytes into a volume, replacing an existing entry when asked.
///
/// Shared by the `volume_write_bytes` command and the ADF commands, so both
/// take the same route through the writer rather than disagreeing about the
/// same disk.
pub fn write_bytes_into(
    image: &Path,
    volume_index: usize,
    dir_block: u32,
    name: &str,
    contents: &[u8],
    replace: bool,
) -> CoreResult<MutationResult> {
    let (outcome, strategy, backup) = with_writer(image, volume_index, |writer| {
        if let Some(existing) = writer.find(dir_block, name)? {
            if !replace {
                return Err(CoreError::InvalidInput(format!(
                    "'{name}' is already there"
                )));
            }
            writer.delete(dir_block, existing.block)?;
        }
        writer.add_file(dir_block, name, contents, Default::default())
    })?;

    Ok(result_of(
        outcome,
        strategy,
        backup,
        outcome_block_size(image, volume_index),
    ))
}

/// Write a file the frontend supplied as bytes.
///
/// Used by the checkout/checkin round trip (§6) and the small-file drop path,
/// where the bytes are already in the webview and a temp file would be a
/// pointless round trip through the disk.
#[tauri::command]
pub fn volume_write_bytes(
    path: String,
    volume_index: usize,
    dir_block: Option<u32>,
    name: String,
    contents: Vec<u8>,
    replace: Option<bool>,
    oplog: State<'_, JsonlOperationLog>,
) -> AppResult<MutationResult> {
    let image = PathBuf::from(path.trim());
    let parent = dir_block.unwrap_or(0);
    let chosen = name.trim().to_string();

    let result = write_bytes_into(
        &image,
        volume_index,
        parent,
        &chosen,
        &contents,
        replace.unwrap_or(false),
    )
    .map_err(AppError::from);

    write_result(
        &oplog,
        user_operation("Write file into volume").destination(format!("{path}:{chosen}")),
        &result,
        |record, made: &MutationResult| {
            record
                .detail("Bytes", contents.len().to_string())
                .detail("Strategy", made.strategy.clone())
                .outcome(OperationOutcome::verified(made.verified))
        },
    );

    result
}

// ---------------------------------------------------------------------------
// Attributes (§7.2)
// ---------------------------------------------------------------------------

/// What the attributes dialog shows.
///
/// The bits arrive twice: as the raw field, and as the `hsparwed` string the
/// dialog renders. The frontend never re-derives the inversion — getting it
/// backwards there would show the user permissions they do not have.
#[derive(Debug, Clone, Serialize)]
pub struct AttributesView {
    pub name: String,
    pub protection: u32,
    /// `hsparwed`, in WinUAE's spelling.
    pub bits: String,
    pub comment: String,
    pub is_dir: bool,
    /// Days since 1978-01-01, and the time of day.
    pub days: u32,
    pub mins: u32,
    pub ticks: u32,
    /// The date as a person reads it, so the UI does not do calendar maths.
    pub date_text: String,
}

/// Read an entry's protection bits, comment and date.
#[tauri::command]
pub fn volume_attributes(
    path: String,
    volume_index: usize,
    entry_block: u32,
) -> AppResult<AttributesView> {
    let image = PathBuf::from(path.trim());
    let entry = pick(&image, volume_index)?;
    let geometry = geometry_of(&image, &entry)?;

    // Reading needs no journal and no write strategy, but `VolumeWriter` is
    // where the field offsets live, so it is opened over a throwaway in-memory
    // copy of the blocks rather than duplicating them here.
    let mut device = FileRegionMut::open(
        &image,
        entry.byte_offset,
        entry.byte_length,
        entry.block_size,
    )?;
    let writer = VolumeWriter::open(&mut device, geometry, &image, entry.byte_offset)?;
    let attributes = writer.attributes(entry_block)?;

    Ok(view_of(attributes))
}

fn view_of(attributes: crate::core::volume::write::EntryAttributes) -> AttributesView {
    use crate::core::volume::write::uaem;

    let sidecar = uaem::Sidecar {
        protection: attributes.protection,
        date: attributes.date,
        comment: String::new(),
    };
    // The renderer already knows how to spell a date the way an Amiga would;
    // taking the first two fields of the line it produces avoids a second
    // calendar implementation on the frontend.
    let line = uaem::render(&sidecar);
    let date_text = line
        .split_whitespace()
        .skip(1)
        .take(2)
        .collect::<Vec<_>>()
        .join(" ");

    AttributesView {
        name: attributes.name,
        protection: attributes.protection,
        bits: uaem::format_bits(attributes.protection),
        comment: attributes.comment,
        is_dir: attributes.is_dir,
        days: attributes.date.days,
        mins: attributes.date.mins,
        ticks: attributes.date.ticks,
        date_text,
    }
}

/// Change an entry's protection bits and comment.
///
/// A field left out keeps what is there: a dialog that only changed the
/// comment must not silently restamp the date.
#[tauri::command]
pub fn volume_set_attributes(
    path: String,
    volume_index: usize,
    entry_block: u32,
    protection: Option<u32>,
    comment: Option<String>,
    oplog: State<'_, JsonlOperationLog>,
) -> AppResult<MutationResult> {
    let image = PathBuf::from(path.trim());

    let result = with_writer(&image, volume_index, |writer| {
        writer.set_attributes(entry_block, protection, comment.as_deref(), None)
    })
    .map(|(outcome, strategy, backup)| {
        let block_size = outcome_block_size(&image, volume_index);
        result_of(outcome, strategy, backup, block_size)
    })
    .map_err(AppError::from);

    write_result(
        &oplog,
        user_operation("Change attributes in volume")
            .source(format!("{path}:{entry_block}"))
            .detail(
                "Bits",
                protection
                    .map(crate::core::volume::write::uaem::format_bits)
                    .unwrap_or_else(|| "unchanged".into()),
            ),
        &result,
        |record, made: &MutationResult| {
            record
                .detail("Strategy", made.strategy.clone())
                .outcome(OperationOutcome::verified(made.verified))
        },
    );

    result
}

/// Read a file out of a volume for viewing (F3).
///
/// Capped: the viewer shows the head of a large file rather than pulling a
/// megabyte through the webview to display the first screen of it.
#[tauri::command]
pub fn volume_read_head(
    path: String,
    volume_index: usize,
    entry_block: u32,
    max_bytes: Option<usize>,
) -> AppResult<Vec<u8>> {
    const DEFAULT_VIEW_BYTES: usize = 256 * 1024;
    const MAX_VIEW_BYTES: usize = 4 * 1024 * 1024;

    let image = PathBuf::from(path.trim());
    let entry = pick(&image, volume_index)?;
    let (device, geometry) = mount(&image, &entry)?;

    let set = crate::core::volume::write::layout::BlockSet::new(geometry.block_size);
    let mut data =
        crate::core::volume::write::file::read_file(&device, &set, &geometry, entry_block)?;

    let limit = max_bytes.unwrap_or(DEFAULT_VIEW_BYTES).min(MAX_VIEW_BYTES);
    data.truncate(limit);
    Ok(data)
}

// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::jobs::NoProgress;
    use crate::core::volume::fixture::ffs_volume;
    use crate::core::volume::write::layout::BlockSet;
    use crate::core::volume::DosType;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("art-cmd-write-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    struct Image {
        dir: PathBuf,
        path: PathBuf,
    }

    impl Image {
        /// A bare FFS volume in a file — the shape `scan_image` reports as one
        /// volume at index 0, which is every ADF.
        fn new(name: &str, total_blocks: u32) -> Self {
            let dir = scratch(name);
            let path = dir.join("disk.adf");
            let (bytes, _) = ffs_volume(total_blocks, DosType::new(*b"DOS\x01"));
            std::fs::write(&path, &bytes).unwrap();
            Self { dir, path }
        }

        fn text(&self) -> String {
            self.path.display().to_string()
        }

        fn bytes(&self) -> Vec<u8> {
            std::fs::read(&self.path).unwrap()
        }

        fn listing(&self) -> Vec<crate::core::volume::write::dir::DirEntry> {
            let entry = pick(&self.path, 0).unwrap();
            let (device, geometry) = mount(&self.path, &entry).unwrap();
            let set = BlockSet::new(geometry.block_size);
            crate::core::volume::write::dir::entries_in(
                &device,
                &set,
                &geometry,
                geometry.root_block,
            )
            .unwrap()
        }
    }

    impl Drop for Image {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }

    /// A floppy is well under the threshold and must take the audited
    /// whole-file path; a real hard disk must not.
    #[test]
    fn a_floppy_takes_the_whole_file_path_and_a_hard_disk_the_journal() {
        assert_eq!(
            describe(WriteStrategy::for_image(901_120)),
            "whole-file",
            "an ADF must keep the pipeline that has been audited for it"
        );
        assert_eq!(
            describe(WriteStrategy::for_image(64 * 1024 * 1024)),
            "block-journal"
        );
    }

    #[test]
    fn a_folder_is_created_and_reported() {
        let image = Image::new("mkdir", 1760);

        let (outcome, strategy, backup) =
            with_writer(&image.path, 0, |writer| writer.make_dir(0, "Tools")).unwrap();

        assert_eq!(strategy, WriteStrategy::WholeFile);
        assert!(outcome.verified);
        assert!(outcome.block.is_some());
        assert!(
            backup.is_some(),
            "the whole-file path backs the image up before replacing it"
        );

        let entries = image.listing();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "Tools");
    }

    /// The whole-file promise: the file on disk is either the old image or the
    /// new one, never a partly written one.
    #[test]
    fn a_refused_operation_leaves_the_image_byte_for_byte_unchanged() {
        let image = Image::new("refused", 1760);
        with_writer(&image.path, 0, |writer| writer.make_dir(0, "Tools")).unwrap();
        let before = image.bytes();

        let err = with_writer(&image.path, 0, |writer| writer.make_dir(0, "TOOLS")).unwrap_err();
        assert!(err.to_string().contains("already exists"), "{err}");
        assert_eq!(image.bytes(), before);
    }

    /// The rule the whole recovery design rests on: never write over a volume
    /// whose journal describes blocks that no longer hold what it recorded.
    #[test]
    fn a_pending_journal_blocks_every_write_until_it_is_recovered() {
        let image = Image::new("pending", 1760);
        let before = image.bytes();

        // Leave a journal behind, the way a crash would.
        {
            let entry = pick(&image.path, 0).unwrap();
            let mut device = FileRegionMut::open(
                &image.path,
                entry.byte_offset,
                entry.byte_length,
                entry.block_size,
            )
            .unwrap();
            let mut journal = crate::core::volume::journal::Journalled::begin(
                &mut device,
                &image.path,
                0,
                "Copy in Something",
                &[900, 901],
            )
            .unwrap();
            journal.write_block(900, &[0xAA; 512]).unwrap();
            std::mem::forget(journal);
        }

        let err = with_writer(&image.path, 0, |writer| writer.make_dir(0, "Tools")).unwrap_err();
        let message = err.to_string();
        assert!(message.contains("unfinished operation"), "{message}");
        assert!(message.contains("Copy in Something"), "{message}");

        // And recovery puts it back.
        let journal = find_journal(&image.path).unwrap().unwrap();
        journal.roll_back().unwrap();
        assert_eq!(image.bytes(), before);

        // After which writing works again.
        assert!(with_writer(&image.path, 0, |writer| writer.make_dir(0, "Tools")).is_ok());
    }

    #[test]
    fn the_capability_report_says_what_the_footer_shows() {
        let image = Image::new("capability", 1760);

        let capability = volume_write_capability(image.text(), 0).unwrap();
        assert!(capability.writable);
        assert!(capability.reason.is_none());
        assert_eq!(capability.strategy, "whole-file");
        assert_eq!(capability.block_size, 512);
        assert!(capability.free_blocks > 0);
        assert_eq!(
            capability.free_bytes,
            capability.free_blocks as u64 * 512,
            "the footer's bytes must be the blocks it reports"
        );
        assert_eq!(capability.volume_name, "Work");
        assert!(capability.pending_recovery.is_none());
    }

    /// §3.4 and §89: never hide the volume, always give the reason.
    #[test]
    fn a_dircache_volume_reports_itself_as_read_only_rather_than_missing() {
        let dir = scratch("dircache-capability");
        let path = dir.join("disk.adf");
        let (mut bytes, _) = ffs_volume(1760, DosType::new(*b"DOS\x01"));
        bytes[3] = 5; // DOS\5 — FFS INTL with a directory cache.
        std::fs::write(&path, &bytes).unwrap();

        let capability = volume_write_capability(path.display().to_string(), 0).unwrap();
        assert!(!capability.writable);
        let reason = capability.reason.unwrap();
        assert!(reason.contains("dircache"), "{reason}");
        assert!(
            !capability.volume_name.is_empty(),
            "the volume is still named, never hidden"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_capability_report_names_an_unfinished_operation() {
        let image = Image::new("capability-pending", 1760);
        {
            let entry = pick(&image.path, 0).unwrap();
            let mut device = FileRegionMut::open(
                &image.path,
                entry.byte_offset,
                entry.byte_length,
                entry.block_size,
            )
            .unwrap();
            let journal = crate::core::volume::journal::Journalled::begin(
                &mut device,
                &image.path,
                0,
                "Delete Readme",
                &[900],
            )
            .unwrap();
            std::mem::forget(journal);
        }

        let capability = volume_write_capability(image.text(), 0).unwrap();
        assert!(!capability.writable);
        assert_eq!(
            capability.pending_recovery.as_deref(),
            Some("Delete Readme")
        );
        assert!(capability.reason.unwrap().contains("Delete Readme"));
    }

    /// Explain before modify: the plan carries real numbers and nothing has
    /// been written when it comes back.
    #[test]
    fn a_copy_plan_reports_the_cost_without_touching_the_image() {
        let image = Image::new("plan", 1760);
        let before = image.bytes();

        let source = image.dir.join("source");
        std::fs::create_dir_all(&source).unwrap();
        std::fs::write(source.join("A.txt"), vec![1u8; 3000]).unwrap();
        std::fs::write(source.join("B.txt"), vec![2u8; 100]).unwrap();

        let plan = volume_plan_copy(image.text(), 0, None, source.display().to_string()).unwrap();
        assert_eq!(plan.files, 2);
        assert_eq!(plan.total_bytes, 3100);
        assert!(plan.fits());
        assert!(plan.is_clean());
        assert_eq!(image.bytes(), before, "planning must not write");
    }

    #[test]
    fn a_plan_that_does_not_fit_says_so_before_anything_is_written() {
        let image = Image::new("plan-toobig", 1760);
        let before = image.bytes();

        let source = image.dir.join("source");
        std::fs::create_dir_all(&source).unwrap();
        std::fs::write(source.join("Huge.bin"), vec![0u8; 4 * 1024 * 1024]).unwrap();

        let plan = volume_plan_copy(image.text(), 0, None, source.display().to_string()).unwrap();
        assert!(!plan.fits());
        assert!(plan.shortfall().unwrap().contains("are free"));
        assert_eq!(image.bytes(), before);
    }

    #[test]
    fn a_folder_copies_in_and_every_file_verifies() {
        let image = Image::new("copy-in", 1760);
        let source = image.dir.join("source");
        std::fs::create_dir_all(source.join("Sub")).unwrap();
        std::fs::write(source.join("Top.txt"), b"top").unwrap();
        std::fs::write(source.join("Sub/Deep.txt"), b"deep").unwrap();

        let folder = HostFolder::new(&source, true);
        let (report, backup) = run_copy_in_folder(
            &image.path,
            0,
            0,
            &folder,
            OverwritePolicy::Skip,
            &NoProgress,
        )
        .unwrap();

        assert_eq!(report.files_copied, 2);
        assert_eq!(report.files_verified, 2);
        assert_eq!(report.directories_created, 1);
        assert!(backup.is_some(), "a floppy is backed up before replacement");

        let entries = image.listing();
        assert_eq!(entries.len(), 2);
    }

    /// A hundred files under the whole-file strategy must produce **one**
    /// backup, not a hundred generations of the same floppy.
    #[test]
    fn a_batch_copy_backs_the_image_up_once_not_once_per_file() {
        let image = Image::new("copy-in-backups", 1760);
        let source = image.dir.join("source");
        std::fs::create_dir_all(&source).unwrap();
        for index in 0..20 {
            std::fs::write(source.join(format!("F{index}.txt")), b"x").unwrap();
        }

        let folder = HostFolder::new(&source, true);
        run_copy_in_folder(
            &image.path,
            0,
            0,
            &folder,
            OverwritePolicy::Skip,
            &NoProgress,
        )
        .unwrap();

        // BackupPolicy::DISK_IMAGE keeps three generations; the count is what
        // proves the whole batch was one write, not twenty.
        let backups = std::fs::read_dir(&image.dir)
            .unwrap()
            .filter_map(|entry| entry.ok())
            .filter(|entry| entry.file_name().to_string_lossy().contains("disk.adf"))
            .count();
        assert!(
            backups <= 4,
            "twenty files should not produce a backup each; found {backups} files"
        );
        assert_eq!(image.listing().len(), 20);
    }

    #[test]
    fn a_folder_copies_out_with_its_structure() {
        let image = Image::new("copy-out", 1760);
        let source = image.dir.join("source");
        std::fs::create_dir_all(source.join("Sub")).unwrap();
        std::fs::write(source.join("Sub/Deep.txt"), b"deep").unwrap();

        let folder = HostFolder::new(&source, true);
        run_copy_in_folder(
            &image.path,
            0,
            0,
            &folder,
            OverwritePolicy::Skip,
            &NoProgress,
        )
        .unwrap();

        let out = image.dir.join("out");
        let entry = pick(&image.path, 0).unwrap();
        let (device, geometry) = mount(&image.path, &entry).unwrap();
        let report = extract_from_volume(
            &device,
            &geometry,
            0,
            &out,
            true,
            OverwritePolicy::Overwrite,
            &NoProgress,
        )
        .unwrap();

        assert_eq!(report.files_written, 1);
        assert_eq!(report.directories_created, 1);
        assert_eq!(std::fs::read(out.join("Sub/Deep.txt")).unwrap(), b"deep");
    }

    #[test]
    fn recovery_reports_what_it_restored_and_leaves_no_journal() {
        let image = Image::new("recover", 1760);
        let before = image.bytes();

        {
            let entry = pick(&image.path, 0).unwrap();
            let mut device = FileRegionMut::open(
                &image.path,
                entry.byte_offset,
                entry.byte_length,
                entry.block_size,
            )
            .unwrap();
            let mut journal = crate::core::volume::journal::Journalled::begin(
                &mut device,
                &image.path,
                0,
                "Copy in Payload.bin",
                &[900, 901, 902],
            )
            .unwrap();
            journal.write_block(900, &[0xAA; 512]).unwrap();
            std::mem::forget(journal);
        }

        let pending = find_journal(&image.path).unwrap().unwrap();
        let report = pending.roll_back().unwrap();

        assert_eq!(report.description, "Copy in Payload.bin");
        assert_eq!(report.blocks_restored, 3);
        assert_eq!(image.bytes(), before);
        assert!(find_journal(&image.path).unwrap().is_none());
    }

    /// Discarding is not a rollback: it is for a stale journal the user has
    /// decided about, and it must leave the image exactly as it is.
    #[test]
    fn discarding_a_journal_leaves_the_image_alone() {
        let image = Image::new("discard", 1760);

        {
            let entry = pick(&image.path, 0).unwrap();
            let mut device = FileRegionMut::open(
                &image.path,
                entry.byte_offset,
                entry.byte_length,
                entry.block_size,
            )
            .unwrap();
            let mut journal = crate::core::volume::journal::Journalled::begin(
                &mut device,
                &image.path,
                0,
                "Half-done",
                &[900],
            )
            .unwrap();
            journal.write_block(900, &[0xAA; 512]).unwrap();
            std::mem::forget(journal);
        }
        let mid_write = image.bytes();

        find_journal(&image.path)
            .unwrap()
            .unwrap()
            .discard()
            .unwrap();
        assert_eq!(image.bytes(), mid_write);
        assert!(find_journal(&image.path).unwrap().is_none());
    }

    #[test]
    fn asking_for_a_volume_that_is_not_there_is_an_error_not_a_panic() {
        let image = Image::new("no-volume", 1760);
        assert!(volume_write_capability(image.text(), 7).is_err());
        assert!(with_writer(&image.path, 7, |writer| writer.make_dir(0, "X")).is_err());
    }
}
