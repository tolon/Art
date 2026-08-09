//! WinUAE & Machine Profile Tauri commands.

use std::path::PathBuf;

use crate::core::profile::AmigaProfile;
use crate::core::rom::{identify_rom, scan_rom_directory, RomInfo};
use crate::core::winuae::{
    detect_winuae, generate_uae_config, launch_winuae, LaunchMedia, WinUaeInstallation,
};
use crate::error::AppResult;

/// Detect WinUAE installation on the system.
#[tauri::command]
pub fn winuae_detect(custom_path: Option<String>) -> AppResult<WinUaeInstallation> {
    Ok(detect_winuae(custom_path.as_deref()))
}

/// List all available Amiga machine profiles.
#[tauri::command]
pub fn winuae_list_profiles() -> AppResult<Vec<AmigaProfile>> {
    Ok(AmigaProfile::all_presets())
}

/// Launch a WinUAE emulation session with a given profile and attached media.
#[tauri::command]
pub fn winuae_launch(
    profile: AmigaProfile,
    media: LaunchMedia,
    winuae_path: Option<String>,
) -> AppResult<u32> {
    let install = detect_winuae(winuae_path.as_deref());
    let exe_path_str = install
        .executable_path
        .ok_or("WinUAE executable not found. Please install WinUAE or configure its path.")?;

    let config_text = generate_uae_config(&profile, &media)?;
    let pid = launch_winuae(&PathBuf::from(exe_path_str), &config_text)?;
    Ok(pid)
}

/// Identify a Kickstart ROM file.
#[tauri::command]
pub fn rom_identify(path: String) -> AppResult<RomInfo> {
    let info = identify_rom(&PathBuf::from(path))?;
    Ok(info)
}

/// Scan a folder for Kickstart ROM files.
#[tauri::command]
pub fn rom_scan_dir(dir_path: String) -> AppResult<Vec<RomInfo>> {
    let list = scan_rom_directory(&PathBuf::from(dir_path))?;
    Ok(list)
}
