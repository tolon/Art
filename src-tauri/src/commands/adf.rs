//! ADF Studio commands: open / list / extract / validate / create / mutate.

use std::path::PathBuf;

use tauri::State;

use super::oplog::{user_operation, write_result};
use crate::commands::volume_write::with_volume;
use crate::core::adf::{
    save_new_adf, AdfImage, AdfInfo, FileEntry, FileSystemType, MutationOutcome, ValidationReport,
};
use crate::core::error::{CoreError, CoreResult};
use crate::core::oplog::{JsonlOperationLog, OperationOutcome};
use crate::error::{AppError, AppResult};

/// Read the volume's fresh [`AdfInfo`] from the bytes a write session's own
/// device just produced.
///
/// Called from *inside* a [`with_volume`] closure, while the session's
/// device is still open — never from a second, independent file open after
/// the session ends. The retired `core::adf::mutate` writer's `mutate_disk_file`
/// built its `AdfInfo` from the same in-memory buffer it was about to write,
/// with no further I/O; this is that same shape for the volume writer, which is
/// now ART's only filesystem writer. Re-opening the file afterwards instead
/// would trade that guarantee for a race: the moment
/// this session's handle is released, another process (an antivirus
/// scan-on-close, a search indexer) can briefly lock the file, and a second
/// open racing that lock would report an already-durable, successful write as
/// a failure — exactly the bug this function exists to avoid.
fn info_from_session(writer: &crate::core::volume::write::VolumeWriter<'_>) -> CoreResult<AdfInfo> {
    AdfImage::from_bytes(writer.all_bytes()?)?.info()
}

/// One place where an ADF command becomes a volume write.
///
/// An ADF is a bare volume at index 0. Routing every mutation through the
/// volume writer is what stops ADF Studio and the file manager holding two
/// different ideas of the same disk.
///
/// Takes bytes already read by the caller, rather than a source path to read
/// itself: `adf_add_file` needs those same bytes for its oplog byte count, and
/// reading the file twice would open a TOCTOU window between the two reads.
fn add_file_at(
    image: &std::path::Path,
    dir_block: Option<u32>,
    name: &str,
    data: &[u8],
) -> CoreResult<MutationOutcome> {
    let target_dir = dir_block.unwrap_or(0);

    let (info, _, backup) = with_volume(image, 0, |writer| {
        if writer.find(target_dir, name)?.is_some() {
            return Err(CoreError::InvalidInput(format!(
                "'{name}' is already there"
            )));
        }
        writer.add_file(target_dir, name, data, Default::default())?;
        info_from_session(writer)
    })?;

    Ok(MutationOutcome {
        info,
        backup_path: backup,
    })
}

/// Create a directory through the volume writer — the body of
/// `adf_create_directory`, factored out so it is callable without a live
/// Tauri `State` (see [`add_file_at`]).
fn create_directory_at(
    image: &std::path::Path,
    parent_block: Option<u32>,
    name: &str,
) -> CoreResult<MutationOutcome> {
    let target_dir = parent_block.unwrap_or(0);
    let (info, _, backup) = with_volume(image, 0, |writer| {
        writer.make_dir(target_dir, name.trim())?;
        info_from_session(writer)
    })?;
    Ok(MutationOutcome {
        info,
        backup_path: backup,
    })
}

/// Delete an entry through the volume writer — the body of `adf_delete_entry`.
fn delete_entry_at(
    image: &std::path::Path,
    parent_block: Option<u32>,
    header_block: u32,
) -> CoreResult<MutationOutcome> {
    let target_dir = parent_block.unwrap_or(0);
    let (info, _, backup) = with_volume(image, 0, |writer| {
        writer.delete(target_dir, header_block)?;
        info_from_session(writer)
    })?;
    Ok(MutationOutcome {
        info,
        backup_path: backup,
    })
}

/// Rename an entry through the volume writer — the body of `adf_rename_entry`.
fn rename_entry_at(
    image: &std::path::Path,
    parent_block: Option<u32>,
    header_block: u32,
    new_name: &str,
) -> CoreResult<MutationOutcome> {
    let target_dir = parent_block.unwrap_or(0);
    let (info, _, backup) = with_volume(image, 0, |writer| {
        writer.rename(target_dir, header_block, new_name.trim())?;
        info_from_session(writer)
    })?;
    Ok(MutationOutcome {
        info,
        backup_path: backup,
    })
}

/// Open an ADF and return high-level info (volume, filesystem, capacity...).
#[tauri::command]
pub fn adf_open(path: String) -> AppResult<AdfInfo> {
    let img = AdfImage::open(&PathBuf::from(&path))?;
    Ok(img.info()?)
}

/// List entries in a directory of an ADF. `dir_block` 0 (or omitted) = root.
#[tauri::command]
pub fn adf_list(path: String, dir_block: Option<u32>) -> AppResult<Vec<FileEntry>> {
    let img = AdfImage::open(&PathBuf::from(&path))?;
    match dir_block {
        Some(0) | None => Ok(img.list_root()?),
        Some(b) => Ok(img.list_dir(b)?),
    }
}

/// Validate an ADF image.
#[tauri::command]
pub fn adf_validate(path: String) -> AppResult<ValidationReport> {
    let img = AdfImage::open(&PathBuf::from(&path))?;
    Ok(img.validate()?)
}

/// Extract a single file by its header block number.
#[tauri::command]
pub fn adf_extract_file(path: String, header_block: u32) -> AppResult<Vec<u8>> {
    let img = AdfImage::open(&PathBuf::from(&path))?;
    Ok(img.extract(header_block)?)
}

/// Create a new blank/formatted ADF image.
#[tauri::command]
pub fn adf_create_blank(
    path: String,
    volume_name: String,
    fs_type: String,
    bootable: bool,
    oplog: State<'_, JsonlOperationLog>,
) -> AppResult<AdfInfo> {
    let ft = match fs_type.to_lowercase().as_str() {
        "ofs" => FileSystemType::Ofs,
        "ffs" => FileSystemType::Ffs,
        _ => {
            return Err(CoreError::InvalidInput(format!(
                "unsupported filesystem type '{fs_type}' (use 'ofs' or 'ffs')"
            ))
            .into())
        }
    };
    let result =
        save_new_adf(&PathBuf::from(&path), &volume_name, ft, bootable).map_err(Into::into);

    write_result(
        &oplog,
        user_operation("Create blank disk")
            .destination(&path)
            .detail("Volume", &volume_name)
            .detail("Filesystem", fs_type.to_uppercase())
            .detail("Bootable", if bootable { "yes" } else { "no" }),
        &result,
        |record, _info: &AdfInfo| record.outcome(OperationOutcome::verified(true)),
    );

    result
}

/// Add a file from the host filesystem into an ADF directory.
#[tauri::command]
pub fn adf_add_file(
    path: String,
    dir_block: Option<u32>,
    source_file_path: String,
    target_name: Option<String>,
    oplog: State<'_, JsonlOperationLog>,
) -> AppResult<MutationOutcome> {
    let image = PathBuf::from(&path);
    let source = PathBuf::from(&source_file_path);
    let name = target_name
        .or_else(|| source.file_name().map(|s| s.to_string_lossy().to_string()))
        .unwrap_or_else(|| "unnamed".to_string());

    // Read once: the same bytes feed both the write and the oplog's byte
    // count, rather than a second `fs::metadata` call racing what the write
    // actually reads (a TOCTOU window between the two).
    let data = std::fs::read(&source).map_err(CoreError::from);
    let byte_count = data.as_ref().map(|d| d.len()).unwrap_or(0);
    let result = data
        .and_then(|data| add_file_at(&image, dir_block, &name, &data))
        .map_err(AppError::from);

    write_result(
        &oplog,
        user_operation("Add file to disk")
            .source(&source_file_path)
            .destination(&path)
            .detail("Name on disk", &name)
            .detail("Bytes", byte_count.to_string()),
        &result,
        |record, outcome: &MutationOutcome| {
            record
                .backup(outcome.backup_path.clone())
                .outcome(OperationOutcome::verified(true))
        },
    );

    result
}

/// Create a new directory inside an ADF folder.
#[tauri::command]
pub fn adf_create_directory(
    path: String,
    parent_block: Option<u32>,
    name: String,
    oplog: State<'_, JsonlOperationLog>,
) -> AppResult<MutationOutcome> {
    let image = PathBuf::from(&path);
    let result = create_directory_at(&image, parent_block, &name).map_err(AppError::from);

    write_result(
        &oplog,
        user_operation("Create directory on disk")
            .destination(&path)
            .detail("Name", &name),
        &result,
        |record, outcome: &MutationOutcome| {
            record
                .backup(outcome.backup_path.clone())
                .outcome(OperationOutcome::verified(true))
        },
    );

    result
}

/// Delete an entry (file or empty directory) from an ADF.
#[tauri::command]
pub fn adf_delete_entry(
    path: String,
    parent_block: Option<u32>,
    header_block: u32,
    oplog: State<'_, JsonlOperationLog>,
) -> AppResult<MutationOutcome> {
    let image = PathBuf::from(&path);
    let result = delete_entry_at(&image, parent_block, header_block).map_err(AppError::from);

    write_result(
        &oplog,
        user_operation("Delete entry from disk")
            .destination(&path)
            .detail("Header block", header_block.to_string()),
        &result,
        |record, outcome: &MutationOutcome| {
            record
                .backup(outcome.backup_path.clone())
                .outcome(OperationOutcome::verified(true))
        },
    );

    result
}

/// Rename an entry on an ADF.
#[tauri::command]
pub fn adf_rename_entry(
    path: String,
    parent_block: Option<u32>,
    header_block: u32,
    new_name: String,
    oplog: State<'_, JsonlOperationLog>,
) -> AppResult<MutationOutcome> {
    let image = PathBuf::from(&path);
    let result =
        rename_entry_at(&image, parent_block, header_block, &new_name).map_err(AppError::from);

    write_result(
        &oplog,
        user_operation("Rename entry on disk")
            .destination(&path)
            .detail("New name", &new_name),
        &result,
        |record, outcome: &MutationOutcome| {
            record
                .backup(outcome.backup_path.clone())
                .outcome(OperationOutcome::verified(true))
        },
    );

    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::adf::create::create_blank_adf;
    use crate::core::adf::FileSystemType;

    fn scratch(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "art-cmd-adf-{name}-{}",
            crate::core::test_scratch_id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// The same operation the file manager performs, through the ADF commands.
    /// Both must land on `core/volume` so the two screens cannot disagree.
    #[test]
    fn adding_a_file_goes_through_the_volume_writer() {
        let dir = scratch("add");
        let path = dir.join("disk.adf");
        std::fs::write(
            &path,
            create_blank_adf("Work", FileSystemType::Ffs, false).unwrap(),
        )
        .unwrap();

        let source = dir.join("Readme");
        let data = b"hello from ART";
        std::fs::write(&source, data).unwrap();

        let outcome = add_file_at(&path, None, "Readme", data).unwrap();
        assert!(
            outcome.backup_path.is_some(),
            "the whole-file strategy backs the image up before replacing it"
        );
        assert_eq!(
            outcome.info.file_count, 1,
            "the returned info must be a real reading of the disk, not a guess"
        );

        // Read it back through the volume path, which is now the only path.
        let found = find_in_root(&path, "Readme");
        assert_eq!(
            crate::core::volume::write::file::read_file(
                &found.0,
                &found.1,
                &found.2,
                found.3.block
            )
            .unwrap(),
            data
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Same proof as the add-file test, for the other three commands: each
    /// must land on `core/volume`, which is now ART's only filesystem writer
    /// (`core/adf/mutate` was deleted once this held for all four).
    #[test]
    fn creating_a_directory_goes_through_the_volume_writer() {
        let dir = scratch("mkdir");
        let path = dir.join("disk.adf");
        std::fs::write(
            &path,
            create_blank_adf("Work", FileSystemType::Ffs, false).unwrap(),
        )
        .unwrap();

        let outcome = create_directory_at(&path, None, "Tools").unwrap();
        assert!(outcome.backup_path.is_some());
        assert_eq!(outcome.info.directory_count, 1);

        let found = find_in_root(&path, "Tools");
        assert!(
            crate::core::volume::write::dir::find_entry(
                &found.0,
                &found.1,
                &found.2,
                found.3.block,
                "anything",
            )
            .unwrap()
            .is_none(),
            "the new folder must really be empty on disk"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn deleting_an_entry_goes_through_the_volume_writer() {
        let dir = scratch("delete");
        let path = dir.join("disk.adf");
        std::fs::write(
            &path,
            create_blank_adf("Work", FileSystemType::Ffs, false).unwrap(),
        )
        .unwrap();
        add_file_at(&path, None, "Doomed", b"bye").unwrap();
        let target = find_in_root(&path, "Doomed").3.block;

        let outcome = delete_entry_at(&path, None, target).unwrap();
        assert!(outcome.backup_path.is_some());
        assert_eq!(outcome.info.file_count, 0);

        let entry = crate::commands::volume_write::pick_volume(&path, 0).unwrap();
        let (device, geometry) = crate::core::volume::mount::mount(&path, &entry).unwrap();
        let set = crate::core::volume::write::layout::BlockSet::new(geometry.block_size);
        assert!(
            crate::core::volume::write::dir::find_entry(
                &device,
                &set,
                &geometry,
                geometry.root_block,
                "Doomed"
            )
            .unwrap()
            .is_none(),
            "the entry must really be gone from disk"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn renaming_an_entry_goes_through_the_volume_writer() {
        let dir = scratch("rename");
        let path = dir.join("disk.adf");
        std::fs::write(
            &path,
            create_blank_adf("Work", FileSystemType::Ffs, false).unwrap(),
        )
        .unwrap();
        let target = {
            add_file_at(&path, None, "Old", b"same bytes").unwrap();
            find_in_root(&path, "Old").3.block
        };

        let outcome = rename_entry_at(&path, None, target, "New").unwrap();
        assert!(outcome.backup_path.is_some());
        assert_eq!(outcome.info.file_count, 1);

        let entry = crate::commands::volume_write::pick_volume(&path, 0).unwrap();
        let (device, geometry) = crate::core::volume::mount::mount(&path, &entry).unwrap();
        let set = crate::core::volume::write::layout::BlockSet::new(geometry.block_size);
        assert!(
            crate::core::volume::write::dir::find_entry(
                &device,
                &set,
                &geometry,
                geometry.root_block,
                "Old"
            )
            .unwrap()
            .is_none(),
            "the old name must be gone"
        );
        assert!(
            crate::core::volume::write::dir::find_entry(
                &device,
                &set,
                &geometry,
                geometry.root_block,
                "New"
            )
            .unwrap()
            .is_some(),
            "the new name must be there"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The safety invariant the reviewer's finding rests on: once the volume
    /// writer's session has produced a real, disk-shaped result, nothing
    /// downstream may turn it into a reported failure. Reading `AdfInfo`
    /// after the fact used to mean a second, independent `AdfImage::open` of
    /// the file *after* `with_volume` had already committed it — a window
    /// where an external lock (an antivirus scan-on-close, a search indexer)
    /// racing that reopen could make an already-durable, successful write
    /// come back as `Err`, with the backup path lost along with it.
    ///
    /// `info_from_session` removes that window structurally rather than
    /// papering over it: it reads through the session's own still-open
    /// device, and for the whole-file strategy every ADF takes, that read
    /// happens *inside* the closure `with_volume` runs — strictly before
    /// `with_volume` ever calls `guarded_write`. So there is no code path left
    /// in `commands/adf.rs` that reopens the file after a write commits, and
    /// no seam left to provoke that specific failure through.
    ///
    /// What *is* still provable, and is the sharper form of the same
    /// invariant, is that a failure at that point in the closure — this test
    /// stands in for `info_from_session` failing — aborts before any byte
    /// reaches the user's file at all, rather than silently committing the
    /// mutation and then reporting it lost. That is a strictly stronger
    /// guarantee than "the backup path survives": the original is not
    /// touched in the first place.
    #[test]
    fn a_failure_after_the_mutation_never_reaches_the_disk() {
        let dir = scratch("post-mutation-failure");
        let path = dir.join("disk.adf");
        std::fs::write(
            &path,
            create_blank_adf("Work", FileSystemType::Ffs, false).unwrap(),
        )
        .unwrap();
        let before = std::fs::read(&path).unwrap();

        let err = with_volume(&path, 0, |writer| {
            writer.make_dir(0, "Tools")?;
            // Standing in for `info_from_session` failing: the mutation
            // above landed in the in-memory image, but the closure has not
            // returned `Ok` yet, so nothing may have been committed.
            Err::<(), _>(CoreError::InvalidInput(
                "simulated failure after the mutation".into(),
            ))
        })
        .unwrap_err();
        assert!(err.to_string().contains("simulated failure"));

        assert_eq!(
            std::fs::read(&path).unwrap(),
            before,
            "a failure after the mutation must leave the original untouched, \
             not silently commit and then report the write as failed"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Reads an entry back out of the root directory through `core/volume`,
    /// independently of whatever the command under test returned — so a test
    /// can only pass if the mutation is really on disk.
    fn find_in_root(
        path: &std::path::Path,
        name: &str,
    ) -> (
        crate::core::volume::device::FileRegion,
        crate::core::volume::write::layout::BlockSet,
        crate::core::volume::VolumeGeometry,
        crate::core::volume::write::dir::DirEntry,
    ) {
        let entry = crate::commands::volume_write::pick_volume(path, 0).unwrap();
        let (device, geometry) = crate::core::volume::mount::mount(path, &entry).unwrap();
        let set = crate::core::volume::write::layout::BlockSet::new(geometry.block_size);
        let found = crate::core::volume::write::dir::find_entry(
            &device,
            &set,
            &geometry,
            geometry.root_block,
            name,
        )
        .unwrap()
        .unwrap_or_else(|| panic!("'{name}' must be on the disk"));
        (device, set, geometry, found)
    }
}
