//! PiStorm and Forensic Hex Analysis Tauri commands.

use std::path::PathBuf;

use tauri::State;

use super::oplog::{user_operation, write_result};
use crate::core::analysis::{read_hex_chunk, HexChunk};
use crate::core::oplog::{JsonlOperationLog, OperationOutcome};
use crate::core::pistorm::hardware::{
    notes_for, pi_models_for, variants_for, AmigaTarget, HardwareNote, PiModel, PiSupport,
    PistormHardware, PistormVariant,
};
use crate::core::pistorm::options::{profile_options, tokens_for, Emu68Options, Emu68Profile};
use crate::core::pistorm::{
    preview_save, save_card, scan_card, PistormCard, PistormPreview, PistormSaveOutcome,
    PistormSetup,
};
use crate::error::AppResult;

/// Read a PiStorm card — a mounted FAT32 folder, or one ART built.
///
/// The hardware comes from the caller, not from the card: nothing on a card
/// says which Amiga it is going into, and the storage tokens are named
/// differently depending on the Pi.
#[tauri::command]
pub fn pistorm_scan(path: String, hardware: PistormHardware) -> AppResult<PistormCard> {
    Ok(scan_card(&PathBuf::from(path), hardware)?)
}

/// What a save would do, in full, before it does it (spec §92).
#[tauri::command]
pub fn pistorm_preview(path: String, setup: PistormSetup) -> AppResult<PistormPreview> {
    Ok(preview_save(&PathBuf::from(path), &setup)?)
}

/// Write `cmdline.txt` and `config.txt`, merging and backing up first.
#[tauri::command]
pub fn pistorm_save(
    path: String,
    setup: PistormSetup,
    oplog: State<'_, JsonlOperationLog>,
) -> AppResult<PistormSaveOutcome> {
    let result = save_card(&PathBuf::from(&path), &setup).map_err(Into::into);

    write_result(
        &oplog,
        user_operation("Save PiStorm card configuration")
            .destination(&path)
            .detail("Amiga", setup.hardware.amiga.display_name())
            .detail("Board", setup.hardware.variant.display_name())
            .detail("Raspberry Pi", setup.hardware.pi.display_name())
            // The tokens themselves, so the log says what was actually written
            // rather than a name for it.
            .detail(
                "Emu68 options",
                tokens_for(&setup.options, setup.hardware).join(" "),
            ),
        &result,
        |record, outcome: &PistormSaveOutcome| {
            record
                .backup(outcome.cmdline_txt_backup.clone())
                .outcome(OperationOutcome::success())
        },
    );

    result
}

/// One row of the hardware matrix, ready for a dropdown.
#[derive(serde::Serialize)]
pub struct PiChoice {
    pub model: PiModel,
    pub name: &'static str,
    pub support: PiSupport,
    pub storage_device: &'static str,
    pub ram_min_mb: u32,
    pub ram_max_mb: u32,
}

/// One board, with the Pis it takes.
#[derive(serde::Serialize)]
pub struct VariantChoice {
    pub variant: PistormVariant,
    pub name: &'static str,
    pub kernel_archive: &'static str,
    pub has_one_slot_option: bool,
    pub pi_models: Vec<PiChoice>,
}

/// One Amiga, with the boards that fit it.
#[derive(serde::Serialize)]
pub struct AmigaChoice {
    pub amiga: AmigaTarget,
    pub name: &'static str,
    pub has_slow_ram: bool,
    pub variants: Vec<VariantChoice>,
}

/// The whole matrix, so the screen's three dropdowns filter each other from
/// one answer rather than from a copy of the tables kept in TypeScript.
#[tauri::command]
pub fn pistorm_hardware_matrix() -> Vec<AmigaChoice> {
    AmigaTarget::ALL
        .iter()
        .map(|amiga| AmigaChoice {
            amiga: *amiga,
            name: amiga.display_name(),
            has_slow_ram: amiga.has_slow_ram(),
            variants: variants_for(*amiga)
                .iter()
                .map(|variant| VariantChoice {
                    variant: *variant,
                    name: variant.display_name(),
                    kernel_archive: variant.kernel_archive(),
                    has_one_slot_option: variant.has_one_slot_option(),
                    pi_models: pi_models_for(*variant)
                        .iter()
                        .map(|(pi, support)| {
                            let (ram_min_mb, ram_max_mb) = pi.ram_mb();
                            PiChoice {
                                model: *pi,
                                name: pi.display_name(),
                                support: *support,
                                storage_device: pi.storage_device(),
                                ram_min_mb,
                                ram_max_mb,
                            }
                        })
                        .collect(),
                })
                .collect(),
        })
        .collect()
}

/// What is worth saying about one combination — ids, resolved to sentences by
/// the UI so they arrive in the user's own language.
#[tauri::command]
pub fn pistorm_hardware_notes(hardware: PistormHardware) -> Vec<HardwareNote> {
    notes_for(hardware)
}

/// A ready-made profile, as the exact options and tokens it stands for.
#[derive(serde::Serialize)]
pub struct ProfilePreview {
    pub options: Emu68Options,
    /// What the line will actually say. The screen shows this beside the card,
    /// because a profile that claims something the tokens do not is how the
    /// last set of profile cards went wrong (ART-090).
    pub tokens: Vec<String>,
}

#[tauri::command]
pub fn pistorm_profile(profile: Emu68Profile, hardware: PistormHardware) -> ProfilePreview {
    let options = profile_options(profile, hardware);
    ProfilePreview {
        tokens: tokens_for(&options, hardware),
        options,
    }
}

/// The tokens a set of options comes to, for the screen's live preview.
#[tauri::command]
pub fn pistorm_tokens(options: Emu68Options, hardware: PistormHardware) -> Vec<String> {
    tokens_for(&options, hardware)
}

/// Read a forensic hex chunk from any disk, ROM, or archive file.
#[tauri::command]
pub fn hex_read(path: String, offset: u64, length: usize) -> AppResult<HexChunk> {
    let chunk = read_hex_chunk(&PathBuf::from(path), offset, length)?;
    Ok(chunk)
}
