//! ADF Studio commands: open / list / extract / validate / create / mutate.

use std::path::PathBuf;

use tauri::State;

use super::oplog::{user_operation, write_result};
use crate::core::adf::{
    add_file, create_directory, delete_entry, mutate_disk_file, rename_entry, save_new_adf,
    AdfImage, AdfInfo, FileEntry, FileSystemType, MutationOutcome, ValidationReport,
};
use crate::core::error::CoreError;
use crate::core::oplog::{JsonlOperationLog, OperationOutcome};
use crate::error::AppResult;

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
    let file_data = std::fs::read(&source_file_path)?;
    let p = PathBuf::from(&source_file_path);
    let name = target_name
        .or_else(|| p.file_name().map(|s| s.to_string_lossy().to_string()))
        .unwrap_or_else(|| "unnamed".to_string());

    let target_dir = dir_block.unwrap_or(0);
    let byte_count = file_data.len();
    let result = mutate_disk_file(&PathBuf::from(&path), |img, fs_type| {
        add_file(img, target_dir, &name, &file_data, fs_type)?;
        Ok(())
    })
    .map_err(Into::into);

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
    let target_dir = parent_block.unwrap_or(0);
    let result = mutate_disk_file(&PathBuf::from(&path), |img, _| {
        create_directory(img, target_dir, &name)?;
        Ok(())
    })
    .map_err(Into::into);

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
    let target_dir = parent_block.unwrap_or(0);
    let result = mutate_disk_file(&PathBuf::from(&path), |img, _| {
        delete_entry(img, target_dir, header_block)?;
        Ok(())
    })
    .map_err(Into::into);

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
    let target_dir = parent_block.unwrap_or(0);
    let result = mutate_disk_file(&PathBuf::from(&path), |img, _| {
        rename_entry(img, target_dir, header_block, &new_name)?;
        Ok(())
    })
    .map_err(Into::into);

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
