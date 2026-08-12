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
    copy_into_volume, extract_from_volume, windows_safe_name, CopyReport, CopySource,
    ExtractReport, HostFolder, HostSelection, StagedTree,
};
use crate::core::volume::write::plan::{plan_copy, CopyPlan, SourceEntry};
use crate::core::volume::write::{DeleteProtection, VolumeWriter, WriteOutcome};
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
/// only shares the device and the single write-back at the end, which is also
/// where the whole image is validated once, for all of them.
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
            // Read whole, mutate in memory, validate the whole result, then
            // one atomic guarded write. Nothing reaches the user's file until
            // that validation passed — see [`commit_whole_file`].
            let original = std::fs::read(image)?;
            let mut device = VecDevice::new(original.clone(), geometry.block_size)?;

            let outcome = {
                let mut writer = VolumeWriter::open(&mut device, geometry, image, 0)?;
                run(&mut writer)?
            };

            let backup = commit_whole_file(image, device.bytes())?;
            Ok((outcome, strategy, backup))
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
            // No whole-image validation here, deliberately: this strategy
            // exists for images too large to hold in memory, and reading a
            // multi-gigabyte HDF back before every commit would defeat it.
            // What protects the user here is per-operation: the journal holds
            // the previous contents of every block until the operation is
            // verified, and `core::volume::write::validate_touched` checks
            // what was written before it commits. No backup path either — the
            // journal is the way back, and a successful commit has already
            // deleted it.
            Ok((outcome, strategy, None))
        }
    }
}

/// Validate a whole in-memory image and only then let it reach the user's file.
///
/// The §57 pipeline's last two steps for the whole-file strategy:
/// `VALIDATE → BACKUP → APPLY`. The writer's own per-operation check
/// (`validate_touched`) sees only the blocks that operation touched, and only
/// their checksums; a volume whose blocks are each well-formed can still be
/// structurally wrong — a root block that is no longer a header block, an
/// image that stopped being an AmigaDOS volume at all. That is caught here,
/// with the image still only in memory, so a refusal leaves the file on disk
/// byte-for-byte as it was.
///
/// Only `Problem` findings refuse. A warning — a bootblock checksum that does
/// not match, trailing bytes past the last whole block — describes an image
/// that was already like that before ART touched it, and refusing those would
/// lock the user out of their own disk rather than protect it (§89).
///
/// The bytes are the whole file, which is what this strategy writes back, and
/// the volume is assumed to start at byte 0 of it — the same assumption the
/// branch above already makes when it opens the device.
fn commit_whole_file(image: &Path, bytes: &[u8]) -> CoreResult<Option<String>> {
    validate_whole_image(image, bytes)?;
    let backup = guarded_write(image, bytes, BackupPolicy::DISK_IMAGE)?;
    Ok(backup.map(|path| path.display().to_string()))
}

fn validate_whole_image(image: &Path, bytes: &[u8]) -> CoreResult<()> {
    // An image that cannot even be parsed as a volume is refused with the same
    // sentence as one that parses and is wrong: what the user needs to know
    // first is that their file was not touched.
    let report = match crate::core::adf::validate::validate_image(bytes) {
        Ok(report) => report,
        Err(err) => return Err(refused(image, &err.to_string())),
    };

    let problems: Vec<String> = report
        .findings
        .iter()
        .filter(|finding| finding.severity == crate::core::adf::HealthStatus::Problem)
        .map(|finding| format!("{} ({})", finding.message, finding.code))
        .collect();

    if problems.is_empty() {
        return Ok(());
    }
    Err(refused(image, &problems.join(" ")))
}

fn refused(image: &Path, detail: &str) -> CoreError {
    CoreError::Malformed {
        format: "volume".into(),
        detail: format!(
            "the result of this operation is not a valid volume, so nothing was written \
             and '{}' is exactly as it was: {detail}",
            image.display(),
        ),
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
///
/// `override_protection` says the user was shown the *third* question — the
/// one about an entry the Amiga itself protects — and said yes. False is the
/// safe answer, and it is what any caller that has not asked will send
/// (ART-088).
#[tauri::command]
pub fn volume_delete(
    path: String,
    volume_index: usize,
    dir_block: Option<u32>,
    entry_block: u32,
    override_protection: bool,
    oplog: State<'_, JsonlOperationLog>,
) -> AppResult<MutationResult> {
    let image = PathBuf::from(path.trim());
    let parent = dir_block.unwrap_or(0);
    let protection = if override_protection {
        DeleteProtection::Override
    } else {
        DeleteProtection::Honour
    };

    let result = with_writer(&image, volume_index, |writer| {
        writer.delete_with(parent, entry_block, protection)
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

/// What a batch delete did.
#[derive(Debug, Clone, Serialize)]
pub struct DeleteManyResult {
    pub deleted: usize,
    pub blocks_touched: usize,
    pub free_blocks: usize,
    pub free_bytes: u64,
    pub verified: bool,
    pub strategy: String,
    /// Where the previous image went, taken **once** for the whole batch.
    pub backup: Option<String>,
}

/// Look up every name in `dir`, refusing the whole batch — before anything
/// is deleted — the moment one entry cannot be. A name that is not there any
/// more or a directory that still has something in it costs the entire
/// selection, not just itself: §92 says explain before modify, and a delete
/// that silently removed nine of a ten-entry pick because the tenth turned
/// out to have something in it would leave the user unable to tell which
/// nine.
fn check_batch_deletable<D: crate::core::volume::BlockDevice + ?Sized>(
    device: &D,
    geometry: &VolumeGeometry,
    dir: u32,
    names: &[String],
) -> CoreResult<()> {
    let set = crate::core::volume::write::layout::BlockSet::new(geometry.block_size);
    let existing = crate::core::volume::write::dir::entries_in(device, &set, geometry, dir)?;

    for name in names {
        let Some(found) = existing.iter().find(|e| e.name.eq_ignore_ascii_case(name)) else {
            return Err(CoreError::InvalidInput(format!(
                "'{name}' is not in this directory any more — nothing in this batch was deleted"
            )));
        };
        if found.is_dir {
            let inside =
                crate::core::volume::write::dir::entries_in(device, &set, geometry, found.block)?;
            if !inside.is_empty() {
                return Err(CoreError::InvalidInput(format!(
                    "'{name}' still has things in it. Empty it first — nothing in this batch \
                     was deleted."
                )));
            }
        }
    }
    Ok(())
}

/// Drop later duplicates of a name already seen, comparing the way AmigaDOS
/// does (case-insensitively) rather than byte-for-byte.
///
/// `["A.txt", "a.txt"]` names the same directory entry twice. Without this,
/// the case-insensitive pre-check in [`check_batch_deletable`] passes (both
/// spellings resolve to the one entry that is there), the writer session
/// deletes it for the first name, and the *second* name's `find` then comes
/// back empty — the exact "checked, then vanished mid-batch" shape §92 exists
/// to prevent, self-inflicted by the caller's own list rather than by
/// anything changing underneath it. Deduping first makes the batch see one
/// name once, the same as a selection that only ever named it once would.
fn dedupe_case_insensitive(names: &[String]) -> Vec<String> {
    let mut seen = std::collections::BTreeSet::new();
    let mut out = Vec::with_capacity(names.len());
    for name in names {
        if seen.insert(name.to_lowercase()) {
            out.push(name.clone());
        }
    }
    out
}

/// The batch-delete pipeline, without the Tauri `State` the command wrapper
/// needs: pre-check every name, then delete everything inside one writer
/// session so the whole-file strategy takes exactly one backup for the batch.
///
/// All-or-nothing *for the whole-file strategy* (§92): every name is
/// resolved and checked *before* the writer session opens, so a batch that
/// cannot fully succeed deletes nothing rather than stopping partway and
/// leaving the user unsure which half of their selection is still there —
/// proven here because every test in this module runs a floppy-sized image.
///
/// The block-journal strategy (large HDFs) cannot make the same promise:
/// each delete inside the session below is its own committed, journalled
/// operation, already durable in the file the moment `writer.delete` returns.
/// An error partway through the loop after the pre-check passed — a name
/// resolving differently than it did a moment ago, say — still leaves the
/// earlier deletes standing. Tracked as an open defect rather than fixed
/// here; see `docs/ISSUES.md`.
fn delete_many(
    image: &Path,
    volume_index: usize,
    dir_block: Option<u32>,
    names: &[String],
    protection: DeleteProtection,
) -> CoreResult<DeleteManyResult> {
    let parent = dir_block.unwrap_or(0);
    let names = dedupe_case_insensitive(names);
    let names = names.as_slice();

    {
        let entry = pick(image, volume_index)?;
        let (device, geometry) = mount(image, &entry)?;
        let dir = if parent == 0 {
            geometry.root_block
        } else {
            parent
        };
        check_batch_deletable(&device, &geometry, dir, names)?;
    }

    let (outcomes, strategy, backup) = with_volume(image, volume_index, |writer| {
        let mut outcomes = Vec::with_capacity(names.len());
        for name in names {
            let Some(found) = writer.find(parent, name)? else {
                return Err(CoreError::InvalidInput(format!(
                    "'{name}' is not in this directory any more"
                )));
            };
            outcomes.push(writer.delete_with(parent, found.block, protection)?);
        }
        Ok(outcomes)
    })?;

    let block_size = outcome_block_size(image, volume_index);
    let deleted = outcomes.len();
    let blocks_touched = outcomes.iter().map(|o| o.blocks_touched).sum();
    let verified = outcomes.iter().all(|o| o.verified);
    let free_blocks = outcomes.last().map(|o| o.free_blocks).unwrap_or(0);

    Ok(DeleteManyResult {
        deleted,
        blocks_touched,
        free_blocks,
        free_bytes: free_blocks as u64 * block_size as u64,
        verified,
        strategy: describe(strategy).into(),
        backup,
    })
}

/// F8 on a multi-selection — delete every named entry from `dir_block` as
/// one operation. See [`delete_many`] for the all-or-nothing pipeline; this
/// is the thin Tauri wrapper that logs the result.
#[tauri::command]
pub fn volume_delete_many(
    path: String,
    volume_index: usize,
    dir_block: Option<u32>,
    names: Vec<String>,
    override_protection: bool,
    oplog: State<'_, JsonlOperationLog>,
) -> AppResult<DeleteManyResult> {
    let image = PathBuf::from(path.trim());
    let protection = if override_protection {
        DeleteProtection::Override
    } else {
        DeleteProtection::Honour
    };

    let result =
        delete_many(&image, volume_index, dir_block, &names, protection).map_err(AppError::from);

    write_result(
        &oplog,
        user_operation("Delete a selection from volume")
            .source(format!("{path}:{}", names.join(", ")))
            .detail("Count", names.len().to_string()),
        &result,
        |record, made: &DeleteManyResult| {
            record
                .detail("Deleted", made.deleted.to_string())
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
        // `Override` because this is a replace, not a delete: the entry is
        // going away only so the same name can carry new bytes, and AmigaDOS
        // governs overwriting with the `w` bit rather than `d`. ART does not
        // check `w` yet either — ART-094.
        writer.delete_with(dir_block, entry_block, DeleteProtection::Override)?;
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
    let entries: Vec<SourceEntry> = folder.entries()?;

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

/// What copying a whole selection — several files and folders picked at
/// once, each keeping its own name at the destination — would cost.
///
/// Read-only, same promise as [`volume_plan_copy`]: nothing is written. The
/// only difference is the source, [`HostSelection`] instead of a single
/// [`HostFolder`], so a one-entry selection goes through exactly the same
/// `plan_copy` a single-root copy does and reads the same either way.
#[tauri::command]
pub fn volume_plan_copy_many(
    path: String,
    volume_index: usize,
    dir_block: Option<u32>,
    sources: Vec<String>,
) -> AppResult<CopyPlan> {
    let image = PathBuf::from(path.trim());
    let entry = pick(&image, volume_index)?;
    let (device, geometry) = mount(&image, &entry)?;

    // The sidecar flag never reaches planning: `entries()` only lists names
    // and sizes, it never calls `metadata()`, so which value is passed here
    // cannot change the answer. `true` is arbitrary but harmless.
    let selection = HostSelection::new(
        sources
            .iter()
            .map(|s| PathBuf::from(s.trim()))
            .collect::<Vec<_>>(),
        true,
    );
    let entries: Vec<SourceEntry> = selection.entries()?;

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
    /// A folder copied out of an *archive*. Its own variant rather than a
    /// borrowed `CopyOut`: an archive's report is the extraction gate's
    /// (`core::archive::extract::ExtractOutcome`), which counts entries
    /// refused by name and entries whose declared size was a lie — things a
    /// volume's `ExtractReport` has no field for and no reason to grow one.
    ArchiveOut {
        job_id: JobId,
        report: crate::core::archive::extract::ExtractOutcome,
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

        // F5 is a plain user-driven copy: a cancel keeps whatever already
        // landed and the report says how much that was — spelled out here,
        // rather than inherited from `run_copy_in_folder`'s default, so the
        // choice is explicit at this call site and not just in its doc
        // comment.
        let outcome = run_copy_in_folder_with(
            &image,
            volume_index,
            parent,
            &folder,
            policy,
            OnCancel::KeepWhatLanded,
            progress,
        );

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

/// The full pipeline `volume_copy_in_many`'s job runs: build the selection
/// from the picked roots — honouring the same sidecar flag a single-folder
/// copy takes — and copy it in as one batch.
///
/// Its own function, called from the job closure rather than inlined in it,
/// so the one thing that must never regress silently — [`OnCancel::Abandon`],
/// not `KeepWhatLanded` — is exercised through the exact code the command
/// runs. A test that re-implemented this instead (as the closure body used
/// to be, inline) would pass unchanged if the command's own choice were ever
/// flipped; see `a_batch_copy_command_abandons_a_cancelled_batch` for the
/// mutation-checked proof that this one does not.
pub(crate) fn copy_selection_into_volume(
    image: &Path,
    volume_index: usize,
    parent: u32,
    roots: Vec<PathBuf>,
    sidecars: bool,
    policy: OverwritePolicy,
    progress: &dyn ProgressSink,
) -> CoreResult<(CopyReport, Option<String>)> {
    let selection = HostSelection::new(roots, sidecars);

    // Abandon, not KeepWhatLanded: a batch the user picked by hand and then
    // cancelled must not commit a random prefix of it and call that a
    // success. See `volume_copy_in_many`'s doc comment.
    run_copy_in_folder_with(
        image,
        volume_index,
        parent,
        &selection,
        policy,
        OnCancel::Abandon,
        progress,
    )
}

/// F5 on a multi-selection — copy everything the user picked, from either
/// side, into a volume as one job.
///
/// Each root keeps its own name at the destination
/// ([`HostSelection`](crate::core::volume::write::copy::HostSelection)), so a
/// selection of `Game/` and `Readme.txt` lands as `Game/` and `Readme.txt`
/// side by side — the same shape [`volume_copy_in`] gives a single folder.
///
/// The one deliberate difference from [`volume_copy_in`] is
/// [`OnCancel::Abandon`]: a batch the user picked by hand and then cancelled
/// must not commit a random prefix of it and call that a success. ART has
/// shipped that mistake twice already in other code paths — see
/// [`copy_selection_into_volume`] for where that choice actually lives now.
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub fn volume_copy_in_many(
    path: String,
    volume_index: usize,
    dir_block: Option<u32>,
    sources: Vec<String>,
    options: Option<CopyOptions>,
    app: AppHandle,
    registry: State<'_, Arc<JobRegistry>>,
    oplog: State<'_, JsonlOperationLog>,
) -> AppResult<JobId> {
    let image = PathBuf::from(path.trim());
    let roots: Vec<PathBuf> = sources.iter().map(|s| PathBuf::from(s.trim())).collect();
    let options = options.unwrap_or_default();
    let policy = options.overwrite.unwrap_or_default();
    let sidecars = options.sidecars.unwrap_or(true);
    let parent = dir_block.unwrap_or(0);

    let log_path = oplog.path().to_path_buf();
    let registry = Arc::clone(&registry);
    let emit_app = app.clone();
    let title = format!("Copying a selection into {}", image.display());

    let id = spawn_job(&app, registry, &title, move |job_id, progress| {
        let outcome = copy_selection_into_volume(
            &image,
            volume_index,
            parent,
            roots,
            sidecars,
            policy,
            progress,
        );

        let record = user_operation("Copy a selection into volume")
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

/// What a cancelled batch means to the caller.
///
/// `copy_into_volume` stops between files and reports how many landed, which
/// is the right answer for a general-purpose copy and the wrong one for an
/// install: half a WHDLoad pack is not "four files copied", it is a broken
/// game the user was told had installed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OnCancel {
    /// Keep what landed and let the caller read `report.cancelled` — F5.
    KeepWhatLanded,
    /// Abandon the batch with [`CoreError::Cancelled`] instead of committing
    /// it, so a cancelled operation leaves nothing behind to be mistaken for
    /// a finished one.
    Abandon,
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
    source: &dyn CopySource,
    policy: OverwritePolicy,
    progress: &dyn ProgressSink,
) -> CoreResult<(CopyReport, Option<String>)> {
    run_copy_in_folder_with(
        image,
        volume_index,
        parent,
        source,
        policy,
        OnCancel::KeepWhatLanded,
        progress,
    )
}

/// [`run_copy_in_folder`], with a say in what a cancellation means.
///
/// With [`OnCancel::Abandon`] the whole-file strategy returns before
/// [`commit_whole_file`] is ever reached, so the user's file is byte-for-byte
/// what it was — cancelling an install leaves no half-installed package at all.
///
/// The block-journal strategy cannot offer that: it exists for images too large
/// to hold in memory, and each file it copied is its own committed, verified
/// operation already durable in the file. What `Abandon` buys there is honesty
/// — the job ends `Cancelled` rather than reporting a successful install of a
/// package that is missing most of itself.
#[allow(clippy::too_many_arguments)]
pub fn run_copy_in_folder_with(
    image: &Path,
    volume_index: usize,
    parent: u32,
    source: &dyn CopySource,
    policy: OverwritePolicy,
    on_cancel: OnCancel,
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
                copy_into_volume(&mut writer, parent, source, policy, progress)?
            };
            if report.cancelled && on_cancel == OnCancel::Abandon {
                // Everything so far happened in `device`, which is a buffer.
                // Returning here — before `commit_whole_file` — is what leaves
                // the user's image exactly as it was (§57).
                return Err(CoreError::Cancelled);
            }
            let backup = commit_whole_file(image, device.bytes())?;
            Ok((report, backup))
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
                copy_into_volume(&mut writer, parent, source, policy, progress)?
            };
            device.sync()?;
            // No whole-image validation and no backup, for the reasons in
            // [`with_volume`]: the journal and the per-operation check are what
            // protect an image too large to hold in memory.
            if report.cancelled && on_cancel == OnCancel::Abandon {
                // Synced first, deliberately: the files that did land are
                // complete, journalled operations and belong on disk. What is
                // refused is calling this a success.
                return Err(CoreError::Cancelled);
            }
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
///
/// Takes `&dyn CopySource` rather than `&HostFolder` specifically, so a batch
/// install over several roots ([`HostSelection`]) reads and refuses exactly
/// the way a single-archive install does — one pre-flight, not two.
pub fn plan_copy_in_folder(
    image: &Path,
    volume_index: usize,
    parent: u32,
    source: &dyn CopySource,
) -> CoreResult<CopyPlan> {
    let entry = pick(image, volume_index)?;
    let (device, geometry) = mount(image, &entry)?;

    let entries: Vec<SourceEntry> = source.entries()?;

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

/// Copy `source` into a volume as one all-or-nothing operation (§92, §54, §57):
/// refuse atomically, with real block numbers, before a single block is
/// written, when it will not fit; abandon it — leaving the volume untouched —
/// if it is cancelled partway through. An install promises "this works" or
/// "this was refused", never a landed prefix.
///
/// The one primitive both `sources::install_archive_into_volume` (one archive,
/// flattened at `parent`) and `archives::install_archives` (several archives,
/// each its own drawer, one batch) build on — the difference between them is
/// only which [`CopySource`] they hand it.
pub(crate) fn install_into_folder(
    image: &Path,
    volume_index: usize,
    parent: u32,
    source: &dyn CopySource,
    policy: OverwritePolicy,
    progress: &dyn ProgressSink,
) -> CoreResult<(CopyReport, Option<String>)> {
    let plan = plan_copy_in_folder(image, volume_index, parent, source)?;
    if !plan.fits() {
        return Err(CoreError::SafetyRefused(plan.shortfall().unwrap_or_else(
            || "this will not fit; nothing was changed".into(),
        )));
    }

    run_copy_in_folder_with(
        image,
        volume_index,
        parent,
        source,
        policy,
        OnCancel::Abandon,
        progress,
    )
}

/// The folder a copy-out lands in, from the destination the user picked and
/// the name of the directory being copied out.
///
/// **This is a security boundary.** `name` is a name ART did not write — a
/// directory record on a disc ART only ever reads, a header block in an ADF
/// that arrived from somewhere else. The frontend used to concatenate it into
/// the destination string *before* the call, which made this boundary one that
/// could be handed `C:\Users\me\Downloads/..\..\..\Users\Public\Startup` as a
/// single opaque string, and `create_dir_all` would make it. A boundary that
/// takes a pre-joined string can always be handed a joined-in escape; a
/// boundary that joins for itself cannot, which is why both `volume_copy_out`
/// and `iso_extract` now take `dest_dir` and `name` separately and call this.
///
/// Two questions, in this order, and never merged into one:
///
/// 1. **Containment**, of the *raw* name. `windows_safe_name` would turn
///    `..\..\Startup` into `_.._..Startup` — a name that passes containment
///    trivially — so asking containment after it would be asking a question
///    whose answer had already been changed. A name that was trying to leave
///    the folder the user picked is refused outright and said so, not quietly
///    renamed into something that looks harmless.
/// 2. **Host legality**, of the escaped name: what NTFS will actually accept
///    (`AUX`, `Prices: 1993`, a trailing dot).
pub(crate) fn folder_destination(dest_dir: &Path, name: &str) -> CoreResult<PathBuf> {
    let refuse = |err: crate::core::security::PathTraversalError| {
        CoreError::SafetyRefused(format!(
            "'{name}' cannot be written under {}: {err}",
            dest_dir.display()
        ))
    };

    crate::core::security::safe_join(dest_dir, name).map_err(refuse)?;
    crate::core::security::safe_join(dest_dir, &windows_safe_name(name)).map_err(refuse)
}

/// The whole of what [`volume_copy_out`]'s job runs: resolve the destination
/// folder from `dest_dir` + `name`, then extract into it.
///
/// Its own function, called from the job closure rather than reimplemented in
/// a test — the same reason [`copy_between_volumes`] below is one. A test that
/// rebuilt this sequence for itself could not catch the destination being
/// resolved the wrong way; calling this can.
#[allow(clippy::too_many_arguments)]
pub(crate) fn copy_out_folder(
    image: &Path,
    volume_index: usize,
    dir_block: u32,
    dest_dir: &Path,
    name: &str,
    write_sidecars: bool,
    policy: OverwritePolicy,
    progress: &dyn ProgressSink,
) -> CoreResult<ExtractReport> {
    // Before the image is even opened: a name that cannot be written is a
    // refusal, not work done and then thrown away.
    let destination = folder_destination(dest_dir, name)?;

    let entry = pick(image, volume_index)?;
    let (device, geometry) = mount(image, &entry)?;
    extract_from_volume(
        &device,
        &geometry,
        dir_block,
        &destination,
        write_sidecars,
        policy,
        progress,
    )
}

/// F5 the other way — copy a folder out of a volume onto the user's disk.
///
/// `dest_dir` is the folder the user picked and `name` is the directory's own
/// name from the listing; they are joined *here*, by
/// [`folder_destination`], never by the caller.
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
    dest_dir: String,
    name: String,
    options: Option<CopyOptions>,
    app: AppHandle,
    registry: State<'_, Arc<JobRegistry>>,
    oplog: State<'_, JsonlOperationLog>,
) -> AppResult<JobId> {
    let image = PathBuf::from(path.trim());
    let destination = PathBuf::from(dest_dir.trim());
    let options = options.unwrap_or_default();
    let policy = options.overwrite.unwrap_or_default();
    let sidecars = options.sidecars.unwrap_or(true);
    let parent = dir_block.unwrap_or(0);

    let log_path = oplog.path().to_path_buf();
    let registry = Arc::clone(&registry);
    let emit_app = app.clone();
    let title = format!("Copying out of {}", image.display());

    let id = spawn_job(&app, registry, &title, move |job_id, progress| {
        let outcome = copy_out_folder(
            &image,
            volume_index,
            parent,
            &destination,
            &name,
            sidecars,
            policy,
            progress,
        );

        let record = user_operation("Copy folder out of volume")
            .source(format!("{}:{volume_index}", image.display()))
            .destination(format!("{}/{name}", destination.display()));
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

/// The one refusal `volume_copy_between` makes before anything is staged:
/// both sides naming the same image file.
///
/// Its own function so the guard has a direct test. The `#[tauri::command]`
/// wrapper itself needs a live `AppHandle`, which needs a real Wry runtime —
/// nothing in this codebase constructs one in a test, for this command or any
/// other `spawn_job` command — so this is what "confirm the command still
/// refuses same-image-both-sides" can actually exercise.
fn refuse_same_image(from: &Path, to: &Path) -> CoreResult<()> {
    if from == to {
        return Err(CoreError::InvalidInput(
            "that is the same image on both sides. Use Move or Rename instead.".into(),
        ));
    }
    Ok(())
}

/// The whole pipeline `volume_copy_between`'s job runs: stage the source side
/// out of `from_image`/`from_volume`/`from_dir`, then insert the staged tree
/// into `to_image`/`to_volume`/`to_dir`.
///
/// Its own function, called from the job closure rather than reimplemented in
/// a test, so the argument mapping itself — `from_volume` feeding the
/// *source* mount and `to_volume`/`to_dir` feeding the *destination* insert,
/// never swapped — is exercised through the exact code the command runs.
/// Reimplementing the sequence in a test (as Task 8's original test did)
/// cannot catch two arguments being swapped at this call site; calling this
/// function can.
#[allow(clippy::too_many_arguments)]
fn copy_between_volumes(
    from_image: &Path,
    from_volume: usize,
    from_dir: u32,
    to_image: &Path,
    to_volume: usize,
    to_dir: u32,
    policy: OverwritePolicy,
    cache: &Path,
    progress: &dyn ProgressSink,
) -> CoreResult<(CopyReport, Option<String>)> {
    let entry = pick(from_image, from_volume)?;
    let (device, geometry) = mount(from_image, &entry)?;
    let staged = StagedTree::stage(&device, &geometry, from_dir, cache, progress)?;

    run_copy_in_staged(to_image, to_volume, to_dir, &staged, policy, progress)
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

    refuse_same_image(&source_image, &target_image)?;

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
        let outcome = copy_between_volumes(
            &source_image,
            from_volume,
            from_dir.unwrap_or(0),
            &target_image,
            to_volume,
            to_dir.unwrap_or(0),
            policy,
            &cache,
            progress,
        );

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
            let backup = commit_whole_file(image, device.bytes())?;
            Ok((report, backup))
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
            // As in [`with_volume`]: the journal, not a whole-image read, is
            // what makes this safe at hard-disk sizes.
            Ok((report, None))
        }
    }
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

        fn listing_of(&self, dir_block: u32) -> Vec<crate::core::volume::write::dir::DirEntry> {
            let entry = pick(&self.path, 0).unwrap();
            let (device, geometry) = mount(&self.path, &entry).unwrap();
            let set = BlockSet::new(geometry.block_size);
            crate::core::volume::write::dir::entries_in(&device, &set, &geometry, dir_block)
                .unwrap()
        }

        /// A file's bytes read back out of the volume — what proves a copy
        /// landed, rather than trusting the report that says it did.
        fn contents(&self, header_block: u32) -> Vec<u8> {
            let entry = pick(&self.path, 0).unwrap();
            let (device, geometry) = mount(&self.path, &entry).unwrap();
            let set = BlockSet::new(geometry.block_size);
            crate::core::volume::write::file::read_file(&device, &set, &geometry, header_block)
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

    /// The gate this whole path exists for: a write whose *blocks* are all
    /// well-formed but whose *volume* is not must never reach the user's file.
    ///
    /// `validate_touched` cannot catch this — every block written here carries
    /// a checksum that adds up, which is all it looks at. Only validating the
    /// finished image sees that the root block stopped being a header block.
    /// The assertion that matters is the second one: an `Err` alone would not
    /// prove the file was left alone.
    #[test]
    fn a_write_that_would_not_validate_never_reaches_the_file() {
        use crate::core::volume::write::layout;

        let image = Image::new("invalid-result", 1760);
        with_writer(&image.path, 0, |writer| writer.make_dir(0, "Tools")).unwrap();
        let before = image.bytes();

        let err = with_volume(&image.path, 0, |writer| {
            // Turn the root block into a data block, checksum and all. Every
            // block the operation touches is internally valid; the volume is
            // not one any more.
            let root = writer.geometry().root_block;
            let block_size = writer.geometry().block_size;
            let all = writer.all_bytes()?;
            let start = root as usize * block_size;
            let mut bytes = all[start..start + block_size].to_vec();
            layout::set_i32(&mut bytes, layout::TYPE_OFFSET, 8)?;

            let mut set = layout::BlockSet::new(block_size);
            set.put(root, bytes)?;
            set.checksum(root, layout::CHECKSUM_OFFSET)?;
            writer.commit_blocks("Deliberately not a volume any more", set)
        })
        .unwrap_err();

        let message = err.to_string();
        assert_eq!(err.code(), "ART-FORMAT-MALFORMED");
        assert!(message.contains("nothing was written"), "{message}");
        assert!(message.contains("rootblock.type"), "{message}");

        assert_eq!(
            image.bytes(),
            before,
            "a result that does not validate must leave the file byte-for-byte unchanged"
        );
        assert_eq!(
            image.listing().len(),
            1,
            "…and the volume that was there is still readable"
        );
    }

    /// The other half of the gate: it must refuse *only* what is broken. A
    /// gate that refused everything would satisfy the test above and destroy
    /// the feature.
    #[test]
    fn a_valid_write_still_commits_through_the_gate() {
        let image = Image::new("valid-commits", 1760);
        let before = image.bytes();

        let (outcome, _, backup) =
            with_writer(&image.path, 0, |writer| writer.make_dir(0, "Tools")).unwrap();

        assert!(outcome.verified);
        assert!(backup.is_some());
        assert_ne!(image.bytes(), before, "the change has to have landed");
        let names: Vec<String> = image.listing().into_iter().map(|e| e.name).collect();
        assert_eq!(names, vec!["Tools".to_string()]);
    }

    /// The gate reads the image's geometry from the image. Wired in against a
    /// hardcoded 1760 blocks it would refuse every HD floppy instead — a
    /// safety check that is really a wall.
    #[test]
    fn an_hd_floppy_is_written_through_the_gate() {
        let image = Image::new("hd-floppy", 3520);

        let (outcome, strategy, _) =
            with_writer(&image.path, 0, |writer| writer.make_dir(0, "Tools")).unwrap();

        assert_eq!(strategy, WriteStrategy::WholeFile);
        assert!(outcome.verified);
        let names: Vec<String> = image.listing().into_iter().map(|e| e.name).collect();
        assert_eq!(names, vec!["Tools".to_string()]);
    }

    /// And the same for a hard-disk image, where nothing about a floppy's
    /// geometry applies. 20 000 blocks is 10 MB — still under the whole-file
    /// threshold, so it really does go through this gate.
    #[test]
    fn a_hard_disk_image_is_written_through_the_gate() {
        let image = Image::new("hard-disk", 20_000);

        let (outcome, strategy, _) = with_writer(&image.path, 0, |writer| {
            writer.add_file(0, "Payload.bin", &[7u8; 4096], Default::default())
        })
        .unwrap();

        assert_eq!(
            strategy,
            WriteStrategy::WholeFile,
            "10 MB must still take the whole-file path"
        );
        assert!(outcome.verified);
        let names: Vec<String> = image.listing().into_iter().map(|e| e.name).collect();
        assert_eq!(names, vec!["Payload.bin".to_string()]);
    }

    /// Moved from `core/adf/mod.rs`'s `mutation_backs_up_the_previous_version`
    /// when `mutate_disk_file` was retired. The backup is only worth taking if
    /// it holds the image as it was — not a re-serialisation of it, and not the
    /// version that has just replaced it. §92: the user is told where the
    /// previous version went, so the previous version has to be what is there.
    #[test]
    fn a_write_backs_up_the_previous_version_byte_for_byte() {
        let image = Image::new("backup-contents", 1760);
        let before = image.bytes();

        let (_, _, backup) =
            with_writer(&image.path, 0, |writer| writer.make_dir(0, "Tools")).unwrap();

        let backup = backup.expect("the whole-file path must take a backup");
        assert_eq!(
            std::fs::read(&backup).unwrap(),
            before,
            "the backup must hold the pre-modification image, byte for byte"
        );
        assert_ne!(
            image.bytes(),
            before,
            "…and the live file really did change"
        );
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

    // ---- Batch commands (Task 4): a selection acts as one operation ----

    /// The batch equivalent of `a_copy_plan_reports_the_cost_without_touching_the_image`:
    /// several picked roots, one plan, nothing written.
    #[test]
    fn a_batch_plan_reports_the_cost_without_touching_the_image() {
        let image = Image::new("batch-plan", 1760);
        let before = image.bytes();

        let picks = image.dir.join("picks");
        let game = picks.join("Game");
        std::fs::create_dir_all(&game).unwrap();
        std::fs::write(game.join("Loader"), vec![1u8; 2000]).unwrap();
        let readme = picks.join("Readme.txt");
        std::fs::write(&readme, vec![2u8; 100]).unwrap();

        let plan = volume_plan_copy_many(
            image.text(),
            0,
            None,
            vec![game.display().to_string(), readme.display().to_string()],
        )
        .unwrap();

        assert_eq!(plan.files, 2, "Loader and Readme.txt");
        assert_eq!(plan.directories, 1, "Game");
        assert!(plan.fits());
        assert!(plan.is_clean());
        assert_eq!(image.bytes(), before, "planning must not write");
    }

    /// A one-entry batch is still a batch, and it keeps its own semantics: a
    /// single *file* picked and planned through the batch command reads
    /// exactly as it would through a selection of one, with the file's own
    /// name at the top level — not "1 items", and not the contents of a
    /// folder spilled flat the way `volume_plan_copy` treats a folder root.
    #[test]
    fn a_one_entry_batch_plan_names_the_single_file() {
        let image = Image::new("batch-plan-one", 1760);
        let picks = image.dir.join("picks");
        std::fs::create_dir_all(&picks).unwrap();
        let file = picks.join("A.txt");
        std::fs::write(&file, vec![1u8; 3000]).unwrap();

        let plan =
            volume_plan_copy_many(image.text(), 0, None, vec![file.display().to_string()]).unwrap();

        assert_eq!(plan.files, 1);
        assert_eq!(plan.directories, 0);
        assert_eq!(plan.total_bytes, 3000);
        assert!(plan.fits());
        assert!(plan.is_clean());
    }

    /// Every root keeps its own name at the destination, side by side — the
    /// same shape `HostSelection`'s own tests already prove, exercised here
    /// through the command path a batch copy job actually runs.
    #[test]
    fn a_batch_copy_lands_everything_from_every_root() {
        let image = Image::new("batch-copy", 1760);
        let picks = image.dir.join("picks");
        let game = picks.join("Game");
        std::fs::create_dir_all(&game).unwrap();
        std::fs::write(game.join("Loader"), b"loader").unwrap();
        let readme = picks.join("Readme.txt");
        std::fs::write(&readme, b"readme").unwrap();

        let selection = HostSelection::new(vec![game.clone(), readme.clone()], true);
        let (report, backup) = run_copy_in_folder_with(
            &image.path,
            0,
            0,
            &selection,
            OverwritePolicy::Skip,
            OnCancel::Abandon,
            &NoProgress,
        )
        .unwrap();

        assert!(report.is_complete(), "{report:?}");
        assert_eq!(report.files_copied, 2);
        assert!(backup.is_some());

        let entries = image.listing();
        assert_eq!(entries.len(), 2, "Game and Readme.txt land side by side");
        assert!(entries.iter().any(|e| e.name == "Game" && e.is_dir));
        assert!(entries.iter().any(|e| e.name == "Readme.txt" && !e.is_dir));
    }

    /// §54, and the rule ART has been bitten by twice already: a cancelled
    /// batch commits nothing, not a random prefix of it. `OnCancel::Abandon`
    /// is what this proves — an `Err` alone would not be enough, so the
    /// assertion that matters is the image bytes matching exactly.
    #[test]
    fn a_cancelled_batch_leaves_the_image_byte_for_byte_unchanged() {
        struct StopAfter(std::sync::atomic::AtomicU64, u64);
        impl ProgressSink for StopAfter {
            fn report(&self, done: u64, _total: Option<u64>, _message: &str) {
                self.0.store(done, std::sync::atomic::Ordering::SeqCst);
            }
            fn is_cancelled(&self) -> bool {
                self.0.load(std::sync::atomic::Ordering::SeqCst) >= self.1
            }
        }

        let image = Image::new("batch-cancel", 1760);
        let before = image.bytes();

        let picks = image.dir.join("picks");
        let a = picks.join("A");
        std::fs::create_dir_all(&a).unwrap();
        for index in 0..5 {
            std::fs::write(a.join(format!("F{index}.txt")), b"x").unwrap();
        }
        let b = picks.join("B");
        std::fs::create_dir_all(&b).unwrap();
        for index in 0..5 {
            std::fs::write(b.join(format!("F{index}.txt")), b"x").unwrap();
        }

        let selection = HostSelection::new(vec![a, b], true);
        let sink = StopAfter(std::sync::atomic::AtomicU64::new(0), 3);

        let err = run_copy_in_folder_with(
            &image.path,
            0,
            0,
            &selection,
            OverwritePolicy::Skip,
            OnCancel::Abandon,
            &sink,
        )
        .unwrap_err();

        assert!(matches!(err, CoreError::Cancelled), "{err}");
        assert_eq!(
            image.bytes(),
            before,
            "a cancelled batch must commit nothing, not a partial prefix of it"
        );
        assert!(image.listing().is_empty(), "…and nothing landed either");
    }

    /// Finding 1 of the phase-1a whole-branch review: `volume_copy_in_many`'s
    /// cancel policy had no test that would notice if the constant it passes
    /// were ever flipped from `OnCancel::Abandon` to `KeepWhatLanded` — the
    /// existing cancel test above calls `run_copy_in_folder_with` directly
    /// with `OnCancel::Abandon` written into the *test*, so it can never see
    /// what the command itself chose. This test calls
    /// [`copy_selection_into_volume`] — the function the job closure actually
    /// runs, containing the real `OnCancel::Abandon` — so a regression there
    /// fails here.
    ///
    /// Mutation-checked by hand: with `copy_selection_into_volume`'s
    /// `OnCancel::Abandon` flipped to `OnCancel::KeepWhatLanded`, this test
    /// failed on the `.unwrap_err()` — the call returned `Ok` instead of
    /// `Err(CoreError::Cancelled)`:
    ///
    /// ```text
    /// called `Result::unwrap_err()` on an `Ok` value: (CopyReport {
    /// files_copied: 2, directories_created: 2, bytes_copied: 2,
    /// files_verified: 2, skipped: [], renamed: [], cancelled: true },
    /// Some(".../.art-backup/disk.adf.01786458933-040650000.bak"))
    /// ```
    ///
    /// i.e. the cancelled batch committed the two files it had landed and a
    /// backup was taken for that partial commit — exactly the ART-052 /
    /// ART-055 shape. Flipped back, this test passes again.
    #[test]
    fn a_batch_copy_command_abandons_a_cancelled_batch() {
        struct StopAfter(std::sync::atomic::AtomicU64, u64);
        impl ProgressSink for StopAfter {
            fn report(&self, done: u64, _total: Option<u64>, _message: &str) {
                self.0.store(done, std::sync::atomic::Ordering::SeqCst);
            }
            fn is_cancelled(&self) -> bool {
                self.0.load(std::sync::atomic::Ordering::SeqCst) >= self.1
            }
        }

        let image = Image::new("batch-command-cancel", 1760);
        let before = image.bytes();

        let picks = image.dir.join("picks");
        let a = picks.join("A");
        std::fs::create_dir_all(&a).unwrap();
        for index in 0..5 {
            std::fs::write(a.join(format!("F{index}.txt")), b"x").unwrap();
        }
        let b = picks.join("B");
        std::fs::create_dir_all(&b).unwrap();
        for index in 0..5 {
            std::fs::write(b.join(format!("F{index}.txt")), b"x").unwrap();
        }

        let sink = StopAfter(std::sync::atomic::AtomicU64::new(0), 3);

        let err = copy_selection_into_volume(
            &image.path,
            0,
            0,
            vec![a, b],
            true,
            OverwritePolicy::Skip,
            &sink,
        )
        .unwrap_err();

        assert!(matches!(err, CoreError::Cancelled), "{err}");
        assert_eq!(
            image.bytes(),
            before,
            "a cancelled batch must commit nothing, not a partial prefix of it"
        );
        assert!(image.listing().is_empty(), "…and nothing landed either");
    }

    /// The delete-side precedent to `a_batch_copy_backs_the_image_up_once_not_once_per_file`:
    /// five deletes inside one batch must cost one backup, not five.
    #[test]
    fn a_batch_delete_removes_everything_it_named_and_backs_up_once() {
        let image = Image::new("batch-delete-ok", 1760);
        for index in 0..5 {
            with_writer(&image.path, 0, |writer| {
                writer.add_file(0, &format!("F{index}.txt"), b"x", Default::default())
            })
            .unwrap();
        }
        assert_eq!(image.listing().len(), 5);

        let names: Vec<String> = (0..5).map(|i| format!("F{i}.txt")).collect();
        let result = delete_many(&image.path, 0, None, &names, DeleteProtection::Honour).unwrap();

        assert_eq!(result.deleted, 5);
        assert!(result.verified);
        assert!(result.backup.is_some());
        assert!(image.listing().is_empty());

        let backups = std::fs::read_dir(&image.dir)
            .unwrap()
            .filter_map(|entry| entry.ok())
            .filter(|entry| entry.file_name().to_string_lossy().contains("disk.adf"))
            .count();
        assert!(
            backups <= 2,
            "five deletes should not produce a backup each; found {backups} files"
        );
    }

    /// §92, atomically: one entry in the batch cannot be deleted (it still
    /// has something in it) and that refuses the whole batch before anything
    /// is touched — not "delete the other two and report the third failed".
    #[test]
    fn a_batch_delete_that_cannot_complete_deletes_nothing() {
        let image = Image::new("batch-delete-refuse", 1760);
        with_writer(&image.path, 0, |writer| writer.make_dir(0, "Empty")).unwrap();
        with_writer(&image.path, 0, |writer| writer.make_dir(0, "NotEmpty")).unwrap();
        let not_empty = image
            .listing()
            .into_iter()
            .find(|e| e.name == "NotEmpty")
            .unwrap();
        with_writer(&image.path, 0, |writer| {
            writer.make_dir(not_empty.block, "Inside")
        })
        .unwrap();
        with_writer(&image.path, 0, |writer| {
            writer.add_file(0, "Readme.txt", b"hi", Default::default())
        })
        .unwrap();

        let before = image.bytes();

        let names = vec![
            "Empty".to_string(),
            "NotEmpty".to_string(),
            "Readme.txt".to_string(),
        ];
        let err = delete_many(&image.path, 0, None, &names, DeleteProtection::Honour).unwrap_err();
        let message = err.to_string();
        assert!(message.contains("NotEmpty"), "{message}");

        assert_eq!(image.bytes(), before, "nothing in the batch was deleted");
        assert_eq!(
            image.listing().len(),
            3,
            "Empty, NotEmpty and Readme.txt are all still there"
        );
    }

    /// The same atomic refusal, but for a name that is simply not there any
    /// more (picked, then removed by something else before the batch ran) —
    /// a different reason to refuse, held to the same all-or-nothing rule.
    #[test]
    fn a_batch_delete_refuses_a_missing_name_without_touching_the_rest() {
        let image = Image::new("batch-delete-missing", 1760);
        with_writer(&image.path, 0, |writer| {
            writer.add_file(0, "Real.txt", b"x", Default::default())
        })
        .unwrap();
        let before = image.bytes();

        let names = vec!["Real.txt".to_string(), "Ghost.txt".to_string()];
        let err = delete_many(&image.path, 0, None, &names, DeleteProtection::Honour).unwrap_err();
        assert!(err.to_string().contains("Ghost.txt"), "{err}");

        assert_eq!(image.bytes(), before);
        assert_eq!(image.listing().len(), 1, "Real.txt is still there");
    }

    /// The cheap half of the review's `delete_many` finding: `["A.txt",
    /// "a.txt"]` names the one entry that is there twice. Before deduping,
    /// the case-insensitive pre-check passed (both spellings resolve), the
    /// writer session deleted it on the first name, and the second name's
    /// `find` then failed — a batch refusing itself over its own duplicate
    /// input, not over anything external. Deduping first means the batch
    /// sees the name once and deletes it once.
    #[test]
    fn a_batch_delete_dedupes_case_different_spellings_of_the_same_name() {
        let image = Image::new("batch-delete-dedupe", 1760);
        with_writer(&image.path, 0, |writer| {
            writer.add_file(0, "A.txt", b"x", Default::default())
        })
        .unwrap();

        let names = vec!["A.txt".to_string(), "a.txt".to_string()];
        let result = delete_many(&image.path, 0, None, &names, DeleteProtection::Honour).unwrap();

        assert_eq!(result.deleted, 1, "one entry, named twice, deletes once");
        assert!(image.listing().is_empty());
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

    // ---- Task 8: `volume_copy_between`'s own pipeline, not just staging ----

    /// The gap the survey found: `a_tree_copies_from_one_image_to_another`
    /// (in `core::volume::write::copy`) already proves the bare staging
    /// primitives — `StagedTree::stage` and `copy_into_volume` against a raw
    /// device — work. Nothing exercised the pipeline `volume_copy_between`'s
    /// job closure actually runs: [`copy_between_volumes`], which does
    /// `pick`/`mount` to find the destination volume and calls
    /// [`run_copy_in_staged`] — the same journal-check, whole-image-validate,
    /// guarded-backup path every other write in this module goes through.
    ///
    /// Also the fix for the review's Task 8 minor: the test used to
    /// reimplement `pick`/`mount`/`StagedTree::stage`/`run_copy_in_staged`
    /// inline instead of calling the actual function, which meant the
    /// closure's own argument mapping — `from_volume`/`to_volume`,
    /// `from_dir`/`to_dir` — was never exercised; swapping two of those
    /// arguments at the real call site would still have passed. Calling
    /// [`copy_between_volumes`] directly, with a non-root directory on *each*
    /// side that only the right mapping can satisfy, closes that: the source
    /// tree lives inside `Tools` (not the source root) and lands inside
    /// `Dest` (not the destination root) — a `from_dir`/`to_dir` swap would
    /// either fail to resolve `Dest`'s block number against the *source*
    /// volume or dump the files at the destination's root instead of inside
    /// `Dest`, and either way an assertion below catches it.
    ///
    /// Two writers are never open at once — staging the source into a temp
    /// folder first is what lets the insert half be an ordinary `with_writer`
    /// session, rather than holding both images' mutable borrows open across
    /// a verification step in between.
    #[test]
    fn a_tree_copies_between_two_images_through_the_command_pipeline() {
        let from = Image::new("between-from", 1760);
        let to = Image::new("between-to", 1760);

        // Build the tree inside a subfolder on the source — this is the
        // volume-to-volume path, not F5's host-to-volume one, which already
        // has its own coverage above.
        with_writer(&from.path, 0, |writer| writer.make_dir(0, "Tools")).unwrap();
        let tools = from
            .listing()
            .into_iter()
            .find(|e| e.name == "Tools")
            .unwrap();
        with_writer(&from.path, 0, |writer| {
            writer.add_file(tools.block, "Editor", b"editor bytes", Default::default())
        })
        .unwrap();
        with_writer(&from.path, 0, |writer| {
            writer.add_file(tools.block, "Readme", b"read me", Default::default())
        })
        .unwrap();
        let before_source = from.bytes();

        // And an empty subfolder, also not the root, on the destination.
        with_writer(&to.path, 0, |writer| writer.make_dir(0, "Dest")).unwrap();
        let dest = to.listing().into_iter().find(|e| e.name == "Dest").unwrap();

        let cache = scratch("between-cache");
        let (report, backup) = copy_between_volumes(
            &from.path,
            0,
            tools.block,
            &to.path,
            0,
            dest.block,
            OverwritePolicy::Skip,
            &cache,
            &NoProgress,
        )
        .unwrap();

        assert_eq!(report.files_copied, 2);
        assert_eq!(report.files_verified, 2);
        assert!(
            backup.is_some(),
            "the destination is a floppy, so it takes the whole-file path and is backed up \
             before being replaced"
        );

        let dest_root = to.listing();
        assert_eq!(
            dest_root.len(),
            1,
            "only Dest at the destination root — nothing landed loose at the root, which is \
             what a from_dir/to_dir swap would do: {dest_root:?}"
        );
        assert_eq!(dest_root[0].name, "Dest");

        let inside_dest = to.listing_of(dest.block);
        assert_eq!(inside_dest.len(), 2, "Editor and Readme, inside Dest");
        let editor = inside_dest.iter().find(|e| e.name == "Editor").unwrap();
        assert_eq!(to.contents(editor.block), b"editor bytes");
        let readme = inside_dest.iter().find(|e| e.name == "Readme").unwrap();
        assert_eq!(to.contents(readme.block), b"read me");

        assert_eq!(
            from.bytes(),
            before_source,
            "the source image must be byte-for-byte unchanged — the copy out of it is \
             read-only staging into a temp folder, never a write back to the source"
        );

        let _ = std::fs::remove_dir_all(&cache);
    }

    /// [`refuse_same_image`] is the guard `volume_copy_between` calls before
    /// staging anything. See its doc comment for why this is what stands in
    /// for calling the `#[tauri::command]` wrapper directly.
    #[test]
    fn a_copy_between_the_same_image_on_both_sides_is_refused() {
        let image = Image::new("between-same-image", 1760);
        let err = refuse_same_image(&image.path, &image.path).unwrap_err();
        assert!(err.to_string().contains("same image"), "{err}");
    }

    /// The name a directory carries out of an image is a name ART did not
    /// write. Joined here rather than by the caller, a traversal in it is a
    /// refusal — and one that never renames its way into looking harmless
    /// (`..\..\Startup` escaped first would be `_.._..Startup`, which passes
    /// containment trivially and lands a file the user never asked for).
    #[test]
    fn a_name_that_leaves_the_chosen_folder_is_refused_not_escaped() {
        let dir = scratch("folder-destination");

        for name in [r"..\..\Startup", "../../Startup", r"C:\Windows\Temp"] {
            let err = folder_destination(&dir, name).unwrap_err();
            assert_eq!(err.code(), "ART-SAFETY-REFUSED", "{name}: {err}");
        }

        // The ordinary case still joins, and a name NTFS refuses is escaped
        // rather than rejected — two different questions, two different
        // answers.
        assert_eq!(
            folder_destination(&dir, "Tools").unwrap(),
            dir.join("Tools")
        );
        assert_eq!(
            folder_destination(&dir, "Prices: 1993").unwrap(),
            dir.join(windows_safe_name("Prices: 1993"))
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// And the whole of what the job runs: a directory copied out lands in a
    /// folder of its own name *inside* the folder the user picked, with the
    /// destination resolved by the command rather than by whoever called it.
    #[test]
    fn copying_a_folder_out_lands_it_under_the_folder_the_user_picked() {
        let image = Image::new("copy-out-folder", 1760);
        with_writer(&image.path, 0, |writer| writer.make_dir(0, "Tools")).unwrap();
        let tools = image
            .listing()
            .into_iter()
            .find(|e| e.name == "Tools")
            .unwrap();
        with_writer(&image.path, 0, |writer| {
            writer.add_file(tools.block, "Editor", b"editor bytes", Default::default())
        })
        .unwrap();

        let dest = scratch("copy-out-folder-dest");
        let report = copy_out_folder(
            &image.path,
            0,
            tools.block,
            &dest,
            "Tools",
            false,
            OverwritePolicy::Skip,
            &NoProgress,
        )
        .unwrap();

        assert_eq!(report.files_written, 1, "{report:?}");
        assert_eq!(
            std::fs::read(dest.join("Tools").join("Editor")).unwrap(),
            b"editor bytes"
        );

        // The same call with a traversing name writes nothing at all.
        let err = copy_out_folder(
            &image.path,
            0,
            tools.block,
            &dest,
            r"..\Escaped",
            false,
            OverwritePolicy::Skip,
            &NoProgress,
        )
        .unwrap_err();
        assert_eq!(err.code(), "ART-SAFETY-REFUSED", "{err}");
        assert!(!dest.parent().unwrap().join("Escaped").exists());

        let _ = std::fs::remove_dir_all(&dest);
    }
}
