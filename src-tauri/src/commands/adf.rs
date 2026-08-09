//! ADF Studio commands: open / list / extract / validate / create / mutate.

use std::path::PathBuf;

use tauri::State;

use super::oplog::{user_operation, write_result};
use crate::commands::volume_write::{with_volume, MutationResult};
use crate::core::adf::{
    save_new_adf, AdfImage, AdfInfo, FileEntry, FileSystemType, MutationOutcome, ValidationReport,
};
use crate::core::error::{CoreError, CoreResult};
use crate::core::oplog::{JsonlOperationLog, OperationOutcome};
use crate::error::{AppError, AppResult};

/// One place where an ADF command becomes a volume write.
///
/// An ADF is a bare volume at index 0. Routing every mutation through the
/// volume writer is what stops ADF Studio and the file manager holding two
/// different ideas of the same disk.
fn add_file_at(
    image: &std::path::Path,
    dir_block: Option<u32>,
    source: &std::path::Path,
    name: Option<String>,
) -> CoreResult<MutationResult> {
    let data = std::fs::read(source)?;
    let chosen = name.unwrap_or_else(|| {
        source
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default()
    });

    crate::commands::volume_write::write_bytes_into(
        image,
        0,
        dir_block.unwrap_or(0),
        &chosen,
        &data,
        false,
    )
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
        .clone()
        .or_else(|| source.file_name().map(|s| s.to_string_lossy().to_string()))
        .unwrap_or_else(|| "unnamed".to_string());

    let byte_count = std::fs::metadata(&source).map(|m| m.len()).unwrap_or(0);
    let result = add_file_at(&image, dir_block, &source, Some(name.clone()))
        .and_then(|mutation| MutationOutcome::from_write(&image, mutation.backup))
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
    let target_dir = parent_block.unwrap_or(0);
    let result = with_volume(&image, 0, |writer| writer.make_dir(target_dir, name.trim()))
        .and_then(|(_, _, backup)| MutationOutcome::from_write(&image, backup))
        .map_err(AppError::from);

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
    let target_dir = parent_block.unwrap_or(0);
    let result = with_volume(&image, 0, |writer| writer.delete(target_dir, header_block))
        .and_then(|(_, _, backup)| MutationOutcome::from_write(&image, backup))
        .map_err(AppError::from);

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
    let target_dir = parent_block.unwrap_or(0);
    let result = with_volume(&image, 0, |writer| {
        writer.rename(target_dir, header_block, new_name.trim())
    })
    .and_then(|(_, _, backup)| MutationOutcome::from_write(&image, backup))
    .map_err(AppError::from);

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
        let dir = std::env::temp_dir().join(format!("art-cmd-adf-{name}-{}", std::process::id()));
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
        std::fs::write(&source, b"hello from ART").unwrap();

        let outcome = add_file_at(&path, None, &source, Some("Readme".into())).unwrap();
        assert!(outcome.verified, "the volume writer verifies what it wrote");

        // Read it back through the volume path, which is now the only path.
        let entry = crate::commands::volume_write::pick_volume(&path, 0).unwrap();
        let (device, geometry) = crate::core::volume::mount::mount(&path, &entry).unwrap();
        let set = crate::core::volume::write::layout::BlockSet::new(geometry.block_size);
        let found = crate::core::volume::write::dir::find_entry(
            &device,
            &set,
            &geometry,
            geometry.root_block,
            "Readme",
        )
        .unwrap()
        .expect("the file must be on the disk");
        assert_eq!(
            crate::core::volume::write::file::read_file(&device, &set, &geometry, found.block)
                .unwrap(),
            b"hello from ART"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }
}
