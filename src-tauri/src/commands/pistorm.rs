//! PiStorm and Forensic Hex Analysis Tauri commands.

use std::path::PathBuf;

use tauri::State;

use super::oplog::{user_operation, write_result};
use crate::core::analysis::{read_hex_chunk, HexChunk};
use crate::core::oplog::{JsonlOperationLog, OperationOutcome};
use crate::core::pistorm::{
    save_pistorm_sd, scan_pistorm_sd, PistormConfig, PistormDriveInfo, PistormSaveOutcome,
};
use crate::error::AppResult;

/// Scan a PiStorm / Emu68 MicroSD card folder.
#[tauri::command]
pub fn pistorm_scan(drive_path: String) -> AppResult<PistormDriveInfo> {
    let info = scan_pistorm_sd(&PathBuf::from(drive_path))?;
    Ok(info)
}

/// Save PiStorm configuration to SD card.
///
/// Existing Raspberry Pi boot settings are merged rather than replaced, and the
/// previous files are backed up first.
#[tauri::command]
pub fn pistorm_save(
    drive_path: String,
    config: PistormConfig,
    oplog: State<'_, JsonlOperationLog>,
) -> AppResult<PistormSaveOutcome> {
    let result = save_pistorm_sd(&PathBuf::from(&drive_path), &config).map_err(Into::into);

    write_result(
        &oplog,
        user_operation("Save PiStorm configuration")
            .destination(&drive_path)
            .detail("Board", config.board.display_name())
            .detail("Fast RAM", format!("{} MB", config.fast_ram_mb)),
        &result,
        |record, outcome: &PistormSaveOutcome| {
            record
                .backup(outcome.config_txt_backup.clone())
                .outcome(OperationOutcome::success())
        },
    );

    result
}

/// Read a forensic hex chunk from any disk, ROM, or archive file.
#[tauri::command]
pub fn hex_read(path: String, offset: u64, length: usize) -> AppResult<HexChunk> {
    let chunk = read_hex_chunk(&PathBuf::from(path), offset, length)?;
    Ok(chunk)
}
