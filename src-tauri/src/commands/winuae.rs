//! WinUAE & Machine Profile Tauri commands.

use std::path::PathBuf;

use crate::core::hdf::detect_hardfile_shape;
use crate::core::profile::AmigaProfile;
use crate::core::rom::{identify_rom, scan_rom_directory, RomInfo};
use crate::core::winuae::{
    detect_winuae, generate_uae_config, launch_winuae, LaunchMedia, WinUaeInstallation,
};
use crate::error::AppResult;

/// ART-146: `winuae_launch` is WinUAE Studio's own manual launch, where the
/// user picks an arbitrary `.hdf` from a file dialog (`WinuaeStudio.tsx`) —
/// nothing upstream of this command has read the file yet, so nothing has
/// decided its `HardfileShape`. Detecting it here, right before the config
/// is generated, is what stops this screen forcing bare-image geometry over
/// an RDB or VHD image exactly as `commands/launch.rs::media_for_plan` does
/// for the catalogued-title launch path.
fn with_detected_hardfile_shapes(mut media: LaunchMedia) -> AppResult<LaunchMedia> {
    media.hardfile_shapes = media
        .hardfile_paths
        .iter()
        .map(|p| detect_hardfile_shape(std::path::Path::new(p)))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(media)
}

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

    let media = with_detected_hardfile_shapes(media)?;
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::hdf::HardfileShape;

    fn scratch(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "art-winuae-cmd-{tag}-{}",
            crate::core::test_scratch_id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// ART-146: WinUAE Studio's manual launch hands over a raw
    /// `hardfile_paths` list with no shape attached — this is where it gets
    /// one, from the file itself, before `generate_uae_config` ever runs.
    #[test]
    fn detects_the_shape_of_a_manually_selected_hardfile() {
        let dir = scratch("vhd");
        let hdf = dir.join("AmiKit.hdf");
        let mut image = vec![0u8; 512 * 4];
        image[0..8].copy_from_slice(b"conectix");
        std::fs::write(&hdf, &image).unwrap();

        let media = LaunchMedia {
            hardfile_paths: vec![hdf.to_string_lossy().to_string()],
            ..Default::default()
        };

        let media = with_detected_hardfile_shapes(media).unwrap();
        assert_eq!(media.hardfile_shapes, vec![HardfileShape::Unknown]);

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// No hardfile selected (floppy-only session) must not error.
    #[test]
    fn no_hardfiles_means_no_shapes_to_detect() {
        let media = LaunchMedia::default();
        let media = with_detected_hardfile_shapes(media).unwrap();
        assert!(media.hardfile_shapes.is_empty());
    }
}
