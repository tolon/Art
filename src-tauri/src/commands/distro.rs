//! Distro profile commands — the OS Builder's registry.
//!
//! Read-only, every one of them. Nothing here writes a card, and nothing here
//! reaches the network: ART never downloads a distro image
//! (`ART-research-distro-profiles.md` §2), and the registry it reads is
//! compiled into the binary.

use std::path::PathBuf;

use crate::core::distro::{
    check_card_size, profile, profiles, rom_family_matches, CardProblem, DistroProfile,
};
use crate::error::AppResult;

/// Every distro profile ART knows about.
#[tauri::command]
pub fn distro_profiles() -> AppResult<Vec<DistroProfile>> {
    Ok(profiles()?)
}

/// Whether a card of this size can hold the profile — checked before anything
/// is written, not two thirds of the way through a 17 GB copy.
#[tauri::command]
pub fn distro_check_card(id: String, card_bytes: u64) -> AppResult<Option<CardProblem>> {
    Ok(check_card_size(&profile(&id)?, card_bytes))
}

/// Whether an identified ROM belongs with this profile's base OS.
///
/// A note, not a gate. `None` when there is nothing to say — an unrecognised
/// ROM has no opinion attached to it.
#[tauri::command]
pub fn distro_rom_family_matches(id: String, rom_version: String) -> AppResult<Option<bool>> {
    Ok(rom_family_matches(&profile(&id)?, &rom_version))
}

/// What the user pointed ART at.
#[derive(Debug, Clone, serde::Serialize)]
pub struct SuppliedImage {
    pub path: String,
    pub size_bytes: u64,
    /// Whether it is a plain file at all — a folder or a missing path is a
    /// mistake worth naming before anything else happens.
    pub is_file: bool,
}

/// Measure a distro image the user downloaded themselves.
///
/// Size only. The checksum belongs to the build manifest (G7) and is recorded
/// when the card is prepared; hashing 17 GB here, before the user has even
/// chosen their hardware, would be a long wait for a number nothing yet uses.
#[tauri::command]
pub fn distro_measure_image(path: String) -> AppResult<SuppliedImage> {
    let target = PathBuf::from(&path);
    let metadata = std::fs::metadata(&target).ok();

    Ok(SuppliedImage {
        path,
        size_bytes: metadata.as_ref().map(|m| m.len()).unwrap_or(0),
        is_file: metadata.map(|m| m.is_file()).unwrap_or(false),
    })
}
