//! HDF & RDB Hard Disk Tauri commands.

use std::path::PathBuf;

use tauri::State;

use super::oplog::{user_operation, write_result};
use crate::core::hdf::{create_hdf, open_hdf, HdfInfo};
use crate::core::oplog::{JsonlOperationLog, OperationOutcome};
use crate::core::rdb::PartitionSpec;
use crate::error::AppResult;

/// Open and inspect an HDF image.
#[tauri::command]
pub fn hdf_open(path: String) -> AppResult<HdfInfo> {
    let info = open_hdf(&PathBuf::from(path))?;
    Ok(info)
}

/// Create a new HDF image file.
#[tauri::command]
pub fn hdf_create(
    path: String,
    total_bytes: u64,
    is_rdb: bool,
    partitions: Vec<PartitionSpec>,
    oplog: State<'_, JsonlOperationLog>,
) -> AppResult<HdfInfo> {
    let result =
        create_hdf(&PathBuf::from(&path), total_bytes, is_rdb, &partitions).map_err(Into::into);

    write_result(
        &oplog,
        user_operation("Create hard disk image")
            .destination(&path)
            .detail("Size", format!("{} MB", total_bytes / (1024 * 1024)))
            .detail("Layout", if is_rdb { "RDB partitioned" } else { "Plain" })
            .detail("Partitions", partitions.len().to_string()),
        &result,
        |record, info: &HdfInfo| {
            record.outcome(OperationOutcome::verified(info.rdb_checksum_valid))
        },
    );

    result
}
