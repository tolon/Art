//! Gotek & FlashFloppy Tauri commands.

use std::path::PathBuf;

use tauri::State;

use super::oplog::{user_operation, write_result};
use crate::core::gotek::{
    save_gotek_drive, scan_gotek_drive, FlashFloppyConfig, GotekDriveInfo, GotekSaveOutcome,
    GotekSlot,
};
use crate::core::oplog::{JsonlOperationLog, OperationOutcome};
use crate::error::AppResult;

/// Scan a Gotek USB drive directory.
#[tauri::command]
pub fn gotek_scan(drive_path: String) -> AppResult<GotekDriveInfo> {
    let info = scan_gotek_drive(&PathBuf::from(drive_path))?;
    Ok(info)
}

/// Save FlashFloppy configuration and Quickslots to USB drive.
///
/// Settings ART does not manage are preserved, and the previous files are
/// backed up before anything is replaced.
#[tauri::command]
pub fn gotek_save(
    drive_path: String,
    config: FlashFloppyConfig,
    slots: Vec<GotekSlot>,
    oplog: State<'_, JsonlOperationLog>,
) -> AppResult<GotekSaveOutcome> {
    let result = save_gotek_drive(&PathBuf::from(&drive_path), &config, &slots).map_err(Into::into);

    write_result(
        &oplog,
        user_operation("Save Gotek configuration")
            .destination(&drive_path)
            .detail("Quickslots", slots.len().to_string()),
        &result,
        |record, outcome: &GotekSaveOutcome| {
            record
                .backup(outcome.ff_cfg_backup.clone())
                .outcome(OperationOutcome::success())
        },
    );

    result
}
