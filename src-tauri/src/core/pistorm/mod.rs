//! PiStorm / Emu68 cards: what the hardware is, what the options are, and how
//! the two files that carry them are read and written.
//!
//! Three modules, because the screen has three questions and they do not mix:
//!
//! - [`hardware`] — which Amiga, which PiStorm, which Pi. Everything else is
//!   derived from it: the kernel build, the storage device name, which tokens
//!   are even meaningful.
//! - [`options`] — the Emu68 `cmdline.txt` tokens. One field per documented
//!   token and nothing else, which is the whole of ART-090's fix.
//! - [`firmware`] — `config.txt`: the kernel, the Kickstart, the display.
//!
//! Both files are **merged, never regenerated** (spec §39, §40). They carry the
//! Raspberry Pi's own settings, and a regenerated `cmdline.txt` has no `root=`.

pub mod activation;
pub mod firmware;
pub mod hardware;
pub mod options;

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::core::error::{CoreError, CoreResult};
use crate::core::rom::{identify_rom, RomInfo};
use crate::core::safety::{guarded_write, BackupPolicy};
use crate::core::security::path::safe_join;
use firmware::{parse_config_txt, FirmwareConfig, KERNEL_IMAGE};
use hardware::{Emu68Line, PistormHardware};
use options::{parse_cmdline, unmanaged_tokens, Emu68Options};

/// Everything ART sets on one card.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct PistormSetup {
    pub hardware: PistormHardware,
    /// Which Emu68 release line the card is being built for.
    ///
    /// Not part of `hardware`, because it is not a fact about the machine —
    /// but it decides the kernel archive's *name*, and that name is not stable
    /// across the two lines (ART-091). It changes nothing about the tokens.
    pub line: Emu68Line,
    pub options: Emu68Options,
    pub firmware: FirmwareConfig,
}

/// A Kickstart-shaped file on the card, and what `core/rom` makes of it.
///
/// The name alone was never an answer. A card can carry `kick.rom` that is a
/// 1.3 image, a 3.1 image, a byte-swapped one or a text file somebody renamed —
/// and until this existed ART wrote whichever name it found into `initramfs`
/// and said nothing about it (F1). `core/rom` identifies by checksum, so the
/// screen can say *which* Kickstart, for which machines.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CardRom {
    pub file_name: String,
    /// What `core/rom` made of it.
    ///
    /// `None` only when the file could not be read or was refused outright —
    /// **not** the same as "not a known Kickstart", which comes back as a
    /// `RomInfo` whose version is `Custom`. A ROM ART does not recognise is
    /// still a ROM the user may want; unknown is a label, never a refusal.
    pub info: Option<RomInfo>,
}

/// Files worth putting through ROM identification.
///
/// Anything with a `.rom` extension, plus the handful of names cards
/// conventionally use without one. Cheap by construction: `identify_rom`
/// refuses anything over 4 MB before reading it, and a Kickstart is at most 1.
fn looks_like_a_rom(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    lower.ends_with(".rom")
        || matches!(
            lower.as_str(),
            "kick.rom" | "kickstart.rom" | "kick31.rom" | "kick32.rom" | "kick13.rom"
        )
}

/// What ART found on a card.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PistormCard {
    pub path: String,
    /// Whether this looks like a PiStorm card at all.
    pub is_pistorm_card: bool,
    /// The Emu68 kernel image.
    pub has_kernel: bool,
    /// What the kernel says about itself, when there is one and it says
    /// anything (F4). `None` for "no kernel"; a `KernelInfo` whose `version`
    /// is `None` for "a kernel that does not state one".
    pub kernel: Option<firmware::KernelInfo>,
    pub has_config_txt: bool,
    pub has_cmdline_txt: bool,
    /// Kickstart images actually present, identified — whatever `config.txt`
    /// names.
    pub kickstart_files: Vec<CardRom>,
    /// The settings read back off the card, under the given hardware.
    pub setup: PistormSetup,
    /// Boot parameters on `cmdline.txt` that are none of ART's business.
    ///
    /// Shown read-only on screen: it is the only way a user can see for
    /// themselves that their own parameters survive a save, and a promise in a
    /// tooltip is worth less than the list.
    pub unmanaged_cmdline: Vec<String>,
    /// The named `config_<name>.txt` sets beside `config.txt`, if any.
    pub config_sets: Vec<String>,
}

/// Read a PiStorm card — a mounted FAT32 folder, or a folder ART built.
///
/// `hardware` is the user's answer, not the card's: nothing on a card says
/// which Amiga it is going into, and guessing would be the same fabrication
/// this module exists to remove. It decides how the tokens are read — which
/// storage prefix, which options are meaningful.
pub fn scan_card(root: &Path, hardware: PistormHardware) -> CoreResult<PistormCard> {
    if !root.is_dir() {
        return Err(CoreError::InvalidInput(format!(
            "No folder at '{}'",
            root.display()
        )));
    }

    // Case-insensitively, because FAT32 is and because a card written by
    // another tool may say `CONFIG.TXT`.
    let find = |wanted: &str| -> Option<PathBuf> {
        let direct = root.join(wanted);
        if direct.is_file() {
            return Some(direct);
        }
        std::fs::read_dir(root).ok().and_then(|entries| {
            entries.flatten().find_map(|entry| {
                let path = entry.path();
                let name = path.file_name()?.to_str()?;
                (name.eq_ignore_ascii_case(wanted) && path.is_file()).then_some(path)
            })
        })
    };

    let config_path = find("config.txt");
    let cmdline_path = find("cmdline.txt");
    let kernel_path = find(KERNEL_IMAGE);
    let has_kernel = kernel_path.is_some();
    let kernel = kernel_path.as_deref().and_then(firmware::read_kernel);

    // Every ROM-shaped file, identified. Not a fixed list of names any more:
    // a card names its Kickstart whatever `initramfs` says, and a name ART did
    // not think of was previously a Kickstart ART could not see.
    let mut kickstart_files: Vec<CardRom> = Vec::new();
    if let Ok(entries) = std::fs::read_dir(root) {
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            let Some(file_name) = path.file_name().and_then(|name| name.to_str()) else {
                continue;
            };
            if !looks_like_a_rom(file_name) {
                continue;
            }
            kickstart_files.push(CardRom {
                file_name: file_name.to_string(),
                info: identify_rom(&path).ok(),
            });
        }
    }
    kickstart_files.sort_by(|a, b| a.file_name.cmp(&b.file_name));

    let cmdline_text = cmdline_path
        .as_deref()
        .and_then(|path| std::fs::read_to_string(path).ok());
    let config_text = config_path
        .as_deref()
        .and_then(|path| std::fs::read_to_string(path).ok());

    let setup = PistormSetup {
        hardware,
        // Nothing on a card says which Emu68 line it was built for, so this
        // stays the caller's answer rather than a guess read off the files.
        line: Emu68Line::default(),
        options: cmdline_text
            .as_deref()
            .map(|text| parse_cmdline(text, hardware))
            .unwrap_or_default(),
        firmware: config_text
            .as_deref()
            .map(parse_config_txt)
            .unwrap_or_default(),
    };

    Ok(PistormCard {
        path: root.to_string_lossy().into_owned(),
        is_pistorm_card: has_kernel || config_path.is_some(),
        has_kernel,
        kernel,
        has_config_txt: config_path.is_some(),
        has_cmdline_txt: cmdline_path.is_some(),
        kickstart_files,
        unmanaged_cmdline: cmdline_text
            .as_deref()
            .map(unmanaged_tokens)
            .unwrap_or_default(),
        setup,
        config_sets: list_config_sets(root),
    })
}

/// What a save would do, before it does it (spec §92).
///
/// Both files in full, so the screen can show the user the actual text rather
/// than a summary of it. A diff of two strings is something a person can check;
/// "3 settings will change" is something they have to believe.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PistormPreview {
    pub cmdline_before: String,
    pub cmdline_after: String,
    pub config_before: String,
    pub config_after: String,
    /// Boot parameters that are not ART's, as they will appear afterwards.
    pub unmanaged_cmdline: Vec<String>,
}

fn read_or_empty(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap_or_default()
}

/// Both files as they are and as they would be.
pub fn preview_save(root: &Path, setup: &PistormSetup) -> CoreResult<PistormPreview> {
    if !root.is_dir() {
        return Err(CoreError::InvalidInput(format!(
            "No folder at '{}'",
            root.display()
        )));
    }

    let cmdline_before = read_or_empty(&root.join("cmdline.txt"));
    let config_before = read_or_empty(&root.join("config.txt"));

    let cmdline_after = options::merge_cmdline(
        &setup.options,
        setup.hardware,
        Some(cmdline_before.as_str()).filter(|text| !text.is_empty()),
    );
    let config_after = firmware::merge_config_txt(
        &setup.firmware,
        Some(config_before.as_str()).filter(|text| !text.is_empty()),
    );

    Ok(PistormPreview {
        unmanaged_cmdline: unmanaged_tokens(&cmdline_after),
        cmdline_before,
        cmdline_after,
        config_before,
        config_after,
    })
}

/// Where the previous versions of the two files went.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PistormSaveOutcome {
    pub config_txt_backup: Option<String>,
    pub cmdline_txt_backup: Option<String>,
}

/// Write both files, backing up what was there.
///
/// `BackupPolicy::CONFIG` keeps five generations: these are files a user has
/// hand-tuned over months, and the cost of keeping them is a few kilobytes.
pub fn save_card(root: &Path, setup: &PistormSetup) -> CoreResult<PistormSaveOutcome> {
    let preview = preview_save(root, setup)?;

    Ok(PistormSaveOutcome {
        cmdline_txt_backup: guarded_write(
            &root.join("cmdline.txt"),
            preview.cmdline_after.as_bytes(),
            BackupPolicy::CONFIG,
        )?
        .map(|path| path.to_string_lossy().into_owned()),
        config_txt_backup: guarded_write(
            &root.join("config.txt"),
            preview.config_after.as_bytes(),
            BackupPolicy::CONFIG,
        )?
        .map(|path| path.to_string_lossy().into_owned()),
    })
}

/// Put a Kickstart the user picked onto the card (F1).
///
/// The ROM is **identified before it is copied**, so a file that cannot even be
/// read never reaches the card — and so the caller has something to show in the
/// confirmation. Identification never refuses on grounds of *recognition*: a
/// custom or byte-swapped ROM is the user's business and copies like any other.
///
/// `name` goes through `safe_join`: it arrives from the frontend, and a name
/// like `..\\..\\windows\\system32\\x` must land on the card or nowhere.
///
/// An existing file is refused unless `overwrite` is set — `SAFE_CREATE`, the
/// same rule every other create in ART follows. When it is set, the previous
/// file is backed up first; ART never deletes a ROM.
pub fn copy_rom_to_card(
    card: &Path,
    source: &Path,
    name: &str,
    overwrite: bool,
) -> CoreResult<RomCopyOutcome> {
    if !card.is_dir() {
        return Err(CoreError::InvalidInput(format!(
            "No folder at '{}'",
            card.display()
        )));
    }

    let destination = safe_join(card, name)
        .map_err(|e| CoreError::InvalidInput(format!("'{name}' is not a usable file name: {e}")))?;
    if destination.parent() != Some(card) {
        return Err(CoreError::InvalidInput(format!(
            "'{name}' would put the ROM in a folder of its own; give a plain file name"
        )));
    }

    let info = identify_rom(source)?;

    if destination.exists() && !overwrite {
        return Err(CoreError::InvalidInput(format!(
            "'{name}' is already on the card. Confirm replacing it, or choose another name."
        )));
    }

    let bytes = std::fs::read(source)?;
    let backup = guarded_write(&destination, &bytes, BackupPolicy::CONFIG)?;

    Ok(RomCopyOutcome {
        rom: CardRom {
            file_name: name.to_string(),
            info: Some(info),
        },
        backup: backup.map(|path| path.to_string_lossy().into_owned()),
    })
}

/// The ROM that arrived, and where the one it replaced went.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RomCopyOutcome {
    pub rom: CardRom,
    /// Where the previous file went, when there was one. ART never deletes a
    /// ROM — replacing one keeps it.
    pub backup: Option<String>,
}

/// Whether a ROM suits the machine the user has chosen.
///
/// A **note**, never a block: people boot 1.3 on an A1200 on purpose, and an
/// unrecognised ROM has no opinion attached at all. `compatible_models` is a
/// list of names from `core/rom`'s table, so this is a name match and says so.
pub fn rom_suits(info: &RomInfo, amiga: hardware::AmigaTarget) -> Option<bool> {
    if info.compatible_models.is_empty() || info.version == "Custom" {
        return None;
    }
    if info
        .compatible_models
        .iter()
        .any(|model| model.eq_ignore_ascii_case("All Models"))
    {
        return Some(true);
    }
    let wanted = match amiga {
        hardware::AmigaTarget::A500 => "A500",
        hardware::AmigaTarget::A1000 => "A1000",
        hardware::AmigaTarget::A2000 => "A2000",
        hardware::AmigaTarget::A600 => "A600",
        hardware::AmigaTarget::A1200 => "A1200",
    };
    Some(
        info.compatible_models
            .iter()
            .any(|model| model.eq_ignore_ascii_case(wanted)),
    )
}

/// The named firmware sets beside `config.txt` — `config_<name>.txt`.
///
/// The pattern MultibootOS uses: one file per distribution, copied over
/// `config.txt` to choose which one boots. Listing them is honest and cheap;
/// what they contain is the user's, and ART does not interpret it.
pub fn list_config_sets(root: &Path) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(root) else {
        return Vec::new();
    };

    let mut names: Vec<String> = entries
        .flatten()
        .filter_map(|entry| {
            let path = entry.path();
            if !path.is_file() {
                return None;
            }
            let name = path.file_name()?.to_str()?.to_ascii_lowercase();
            let stem = name.strip_prefix("config_")?.strip_suffix(".txt")?;
            (!stem.is_empty()).then(|| stem.to_string())
        })
        .collect();

    names.sort();
    names.dedup();
    names
}

/// The file a named set lives in.
///
/// The name is checked rather than interpolated: it arrives from the frontend,
/// and `config_../../boot.txt` must land on the card or nowhere. Only letters,
/// digits, `-` and `_` — which is also what makes the name recognisable on a
/// FAT32 card read by an Amiga.
fn config_set_path(root: &Path, name: &str) -> CoreResult<PathBuf> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err(CoreError::InvalidInput("Give the set a name".into()));
    }
    if trimmed.len() > 32 {
        return Err(CoreError::InvalidInput(format!(
            "'{trimmed}' is too long for a set name; keep it to 32 characters"
        )));
    }
    if !trimmed
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return Err(CoreError::InvalidInput(format!(
            "'{trimmed}' has characters a set name cannot use; letters, digits, - and _ only"
        )));
    }

    safe_join(
        root,
        &format!("config_{}.txt", trimmed.to_ascii_lowercase()),
    )
    .map_err(|e| CoreError::InvalidInput(format!("'{trimmed}' is not a usable name: {e}")))
}

/// What writing a named set would do, before it does it (spec §92).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigSetPreview {
    /// The file that would be written.
    pub file_name: String,
    pub before: String,
    pub after: String,
    /// What the change *means*, which a diff cannot say.
    ///
    /// A line moving from one kernel name to another is one changed character
    /// to a diff and a card that does not boot to a Pi - [ART-103](../../../../docs/ISSUES.md)
    /// exactly, reached by a different road. See [`activation::ActivationEffect`].
    pub effects: Vec<activation::ActivationEffect>,
}

/// Where a named set's text would come from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ConfigSetSource {
    /// The card's current `config.txt`, verbatim.
    CurrentConfig,
    /// Another named set — a duplicate.
    Set,
    /// The settings on screen, as they would be written to `config.txt`.
    ScreenSettings,
}

fn config_set_text(
    root: &Path,
    source: ConfigSetSource,
    from: Option<&str>,
    setup: &PistormSetup,
) -> CoreResult<String> {
    match source {
        ConfigSetSource::CurrentConfig => Ok(read_or_empty(&root.join("config.txt"))),
        ConfigSetSource::Set => {
            let name = from.ok_or_else(|| {
                CoreError::InvalidInput("Say which set is being duplicated".into())
            })?;
            let path = config_set_path(root, name)?;
            if !path.is_file() {
                return Err(CoreError::InvalidInput(format!("There is no '{name}' set")));
            }
            Ok(std::fs::read_to_string(path)?)
        }
        ConfigSetSource::ScreenSettings => Ok(firmware::merge_config_txt(
            &setup.firmware,
            Some(read_or_empty(&root.join("config.txt")).as_str()).filter(|t| !t.is_empty()),
        )),
    }
}

/// What creating or duplicating a named set would write.
pub fn preview_config_set(
    root: &Path,
    name: &str,
    source: ConfigSetSource,
    from: Option<&str>,
    setup: &PistormSetup,
) -> CoreResult<ConfigSetPreview> {
    let path = config_set_path(root, name)?;
    Ok(ConfigSetPreview {
        file_name: path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default(),
        before: read_or_empty(&path),
        after: config_set_text(root, source, from, setup)?,
        // Empty on purpose. This preview writes `config_<name>.txt`, which is
        // not the file the Pi reads - `config.txt` is - so "the kernel
        // changes" would be a sentence about a file that boots nothing. The
        // check belongs where the set becomes the boot config, which is
        // `preview_activate_config_set`.
        effects: Vec::new(),
    })
}

/// Create or replace a named set. Returns the backup path, if there was one.
pub fn write_config_set(
    root: &Path,
    name: &str,
    source: ConfigSetSource,
    from: Option<&str>,
    setup: &PistormSetup,
) -> CoreResult<Option<String>> {
    let preview = preview_config_set(root, name, source, from, setup)?;
    if preview.after.trim().is_empty() {
        // A set of nothing would activate as a `config.txt` of nothing, and a
        // Pi with an empty `config.txt` does not boot. Refused rather than
        // written and discovered later.
        return Err(CoreError::InvalidInput(
            "There is nothing to save into that set yet — the card has no config.txt".into(),
        ));
    }

    let path = config_set_path(root, name)?;
    Ok(
        guarded_write(&path, preview.after.as_bytes(), BackupPolicy::CONFIG)?
            .map(|backup| backup.to_string_lossy().into_owned()),
    )
}

/// Give a named set another name. The file is copied, then the old one is
/// removed — the one place ART deletes a config set, and only ever the one it
/// has just finished copying.
pub fn rename_config_set(root: &Path, from: &str, to: &str) -> CoreResult<()> {
    let source = config_set_path(root, from)?;
    let destination = config_set_path(root, to)?;

    if !source.is_file() {
        return Err(CoreError::InvalidInput(format!("There is no '{from}' set")));
    }
    if source == destination {
        return Ok(());
    }
    if destination.exists() {
        return Err(CoreError::InvalidInput(format!(
            "There is already a '{to}' set"
        )));
    }

    let text = std::fs::read(&source)?;
    crate::core::safety::atomic_write(&destination, &text)?;
    std::fs::remove_file(&source)?;
    Ok(())
}

/// Delete a named firmware set (ART-092).
///
/// **Destructive, and the only thing in this module that is** — so it behaves
/// like the rest of ART's destructive operations rather than like a button: the
/// file is backed up before it goes, so "deleted" here means "moved out of the
/// way and recoverable", not "gone".
///
/// The active `config.txt` is never a set and cannot be reached from here: the
/// name is turned into `config_<name>.txt` by [`config_set_path`], which has no
/// spelling that produces the plain one.
///
/// Refuses the set that is currently active, in the sense that matters — if its
/// text is byte-for-byte what `config.txt` holds, deleting it takes away the
/// only copy of the configuration the card boots from.
pub fn delete_config_set(root: &Path, name: &str) -> CoreResult<Option<String>> {
    let path = config_set_path(root, name)?;
    if !path.is_file() {
        return Err(CoreError::InvalidInput(format!("There is no '{name}' set")));
    }

    let text = std::fs::read_to_string(&path)?;
    if !text.trim().is_empty() && text == read_or_empty(&root.join("config.txt")) {
        return Err(CoreError::InvalidInput(format!(
            "'{name}' is the configuration the card is set up with right now. \
             Make another set active first, then delete this one."
        )));
    }

    let backup = crate::core::safety::backup_file(&path, BackupPolicy::CONFIG)?;
    std::fs::remove_file(&path)?;
    Ok(backup.map(|path| path.to_string_lossy().into_owned()))
}

/// What activating a set would do to `config.txt` (spec §92).
/// The file names directly in the boot partition's root.
///
/// Directories are left out: `kernel=` and `initramfs` name files, and a
/// folder that happens to share a name is not one of them. An unreadable
/// folder gives an empty list, which makes every name look absent - said here
/// because that is the failure mode, and it is the safe direction: a warning
/// nobody needed rather than a silent one nobody got.
fn file_names_in(root: &Path) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(root) else {
        return Vec::new();
    };
    entries
        .flatten()
        .filter(|entry| entry.path().is_file())
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect()
}

pub fn preview_activate_config_set(root: &Path, name: &str) -> CoreResult<ConfigSetPreview> {
    let path = config_set_path(root, name)?;
    if !path.is_file() {
        return Err(CoreError::InvalidInput(format!("There is no '{name}' set")));
    }
    let before = read_or_empty(&root.join("config.txt"));
    let after = std::fs::read_to_string(path)?;
    Ok(ConfigSetPreview {
        file_name: "config.txt".into(),
        effects: activation::activation_effects(&before, &after, &file_names_in(root)),
        before,
        after,
    })
}

/// Copy a named set over `config.txt` — the MultibootOS pattern.
///
/// The set itself is untouched, so activating one is reversible by activating
/// another. The `config.txt` being replaced is backed up first.
pub fn activate_config_set(root: &Path, name: &str) -> CoreResult<Option<String>> {
    let preview = preview_activate_config_set(root, name)?;
    Ok(guarded_write(
        &root.join("config.txt"),
        preview.after.as_bytes(),
        BackupPolicy::CONFIG,
    )?
    .map(|backup| backup.to_string_lossy().into_owned()))
}

#[cfg(test)]
mod tests {
    use super::hardware::{AmigaTarget, PiModel, PistormVariant};
    use super::*;

    /// A card built in a temp directory, the way every fixture in this
    /// repository is: synthetic, made at run time, and carrying no Amiga
    /// content ART does not own.
    struct Card(PathBuf);

    impl Card {
        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for Card {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn card(files: &[(&str, &str)]) -> Card {
        let dir =
            std::env::temp_dir().join(format!("art-pistorm-{}", crate::core::test_scratch_id()));
        std::fs::create_dir_all(&dir).unwrap();
        for (name, contents) in files {
            std::fs::write(dir.join(name), contents).unwrap();
        }
        Card(dir)
    }

    /// **The check reaching a real folder**, which is the half the pure tests
    /// in `activation.rs` cannot cover: they are handed a file list, and this
    /// is where the list comes from.
    ///
    /// The set names a kernel no release has ever carried - ART-103's own
    /// wrong name - and the card does not have it. Before this, activating it
    /// showed a one-character diff and produced a card that would not boot.
    #[test]
    fn activating_a_set_that_names_a_missing_kernel_says_so() {
        let card = card(&[
            (
                "config.txt",
                "kernel=Emu68-pistorm.gz
initramfs kick.rom
",
            ),
            ("Emu68-pistorm.gz", "x"),
            ("kick.rom", "x"),
            (
                "config_broken.txt",
                "kernel=Emu68.img
initramfs kick.rom
",
            ),
        ]);

        let preview = preview_activate_config_set(card.path(), "broken").unwrap();
        assert!(
            preview
                .effects
                .contains(&activation::ActivationEffect::KernelNotOnTheCard {
                    name: "Emu68.img".into()
                }),
            "{:?}",
            preview.effects
        );
    }

    /// The other arm. Without it the test above passes on a preview that calls
    /// everything missing - which is exactly what an empty file list would do.
    #[test]
    fn activating_a_set_whose_files_are_all_there_reports_only_the_change() {
        let card = card(&[
            (
                "config.txt",
                "kernel=Emu68-pistorm.gz
initramfs kick.rom
",
            ),
            ("Emu68-pistorm.gz", "x"),
            ("kick.rom", "x"),
            ("kick31.rom", "x"),
            (
                "config_31.txt",
                "kernel=Emu68-pistorm.gz
initramfs kick31.rom
",
            ),
        ]);

        let preview = preview_activate_config_set(card.path(), "31").unwrap();
        assert_eq!(
            preview.effects,
            vec![activation::ActivationEffect::KickstartChanges {
                from: Some("kick.rom".into()),
                to: "kick31.rom".into()
            }]
        );
    }

    /// A 256 KB block that passes `verify_kickstart_checksum`.
    ///
    /// Synthetic, and it has to be: ART ships no Amiga content, ever. It is not
    /// a real Kickstart and `identify_rom` will not claim it is one — which is
    /// exactly the case worth testing, because "not a known Kickstart" must
    /// stay a label and never a refusal.
    fn rom_bytes(size: usize) -> Vec<u8> {
        vec![0x11u8; size]
    }

    #[test]
    fn a_rom_on_the_card_is_identified_rather_than_merely_named() {
        // F1. The name was never an answer: `kick.rom` can be a 1.3 image, a
        // 3.1 image, or a text file somebody renamed.
        let dir = card(&[("kick31.rom", "")]);
        std::fs::write(dir.path().join("kick31.rom"), rom_bytes(512 * 1024)).unwrap();

        let found = scan_card(dir.path(), PistormHardware::default()).unwrap();
        assert_eq!(found.kickstart_files.len(), 1);
        let rom = &found.kickstart_files[0];
        assert_eq!(rom.file_name, "kick31.rom");
        let info = rom
            .info
            .as_ref()
            .expect("a verdict, even an unflattering one");
        assert_eq!(info.size_bytes, 512 * 1024);
    }

    #[test]
    fn a_rom_art_does_not_recognise_is_labelled_not_refused() {
        // Unknown is a label. The file may be byte-swapped, custom, or a
        // Kickstart newer than ART's table — all of them the user's business.
        let dir = card(&[]);
        std::fs::write(dir.path().join("mystery.rom"), rom_bytes(4096)).unwrap();

        let found = scan_card(dir.path(), PistormHardware::default()).unwrap();
        assert_eq!(found.kickstart_files.len(), 1);
        let info = found.kickstart_files[0]
            .info
            .as_ref()
            .expect("an unrecognised ROM still gets a verdict");
        assert_eq!(info.version, "Custom");
    }

    #[test]
    fn a_kickstart_under_a_name_art_never_thought_of_is_still_found() {
        // The old fixed list could not see this one, and `initramfs` may name
        // anything at all.
        let dir = card(&[]);
        std::fs::write(dir.path().join("ThreePointOne.rom"), rom_bytes(4096)).unwrap();

        let found = scan_card(dir.path(), PistormHardware::default()).unwrap();
        assert_eq!(found.kickstart_files.len(), 1);
        assert_eq!(found.kickstart_files[0].file_name, "ThreePointOne.rom");
    }

    #[test]
    fn a_rom_is_copied_onto_the_card_under_the_name_it_is_given() {
        let dir = card(&[]);
        let source = dir.path().join("source-kick.rom");
        std::fs::write(&source, rom_bytes(4096)).unwrap();

        let copied = copy_rom_to_card(dir.path(), &source, "kick.rom", false).unwrap();
        assert_eq!(copied.rom.file_name, "kick.rom");
        assert!(copied.rom.info.is_some());
        assert_eq!(copied.backup, None, "nothing was replaced");
        assert_eq!(
            std::fs::read(dir.path().join("kick.rom")).unwrap(),
            rom_bytes(4096),
            "the bytes must arrive unaltered"
        );
    }

    #[test]
    fn a_rom_never_lands_outside_the_card() {
        // The name arrives from the frontend. `safe_join` is the only way an
        // archive entry name becomes a path anywhere in ART, and this is no
        // different for being typed rather than read out of a zip.
        let dir = card(&[]);
        let source = dir.path().join("source.rom");
        std::fs::write(&source, rom_bytes(4096)).unwrap();

        for name in [
            "../escaped.rom",
            "..\\escaped.rom",
            "sub/kick.rom",
            "C:\\kick.rom",
        ] {
            let err = copy_rom_to_card(dir.path(), &source, name, false).unwrap_err();
            assert!(
                err.to_string().contains(name) || err.to_string().contains("folder"),
                "{name}: {err}"
            );
            assert!(
                !dir.path().parent().unwrap().join("escaped.rom").exists(),
                "{name} escaped the card"
            );
        }
    }

    #[test]
    fn an_existing_rom_on_the_card_is_not_replaced_without_being_asked() {
        // SAFE_CREATE, the same rule every other create in ART follows.
        let dir = card(&[("kick.rom", "the one already there")]);
        let source = dir.path().join("source.rom");
        std::fs::write(&source, rom_bytes(4096)).unwrap();

        let err = copy_rom_to_card(dir.path(), &source, "kick.rom", false).unwrap_err();
        assert!(err.to_string().contains("kick.rom"), "{err}");
        assert_eq!(
            std::fs::read_to_string(dir.path().join("kick.rom")).unwrap(),
            "the one already there",
            "the original must be untouched"
        );
    }

    #[test]
    fn replacing_a_rom_on_purpose_keeps_the_previous_one() {
        let dir = card(&[("kick.rom", "the one already there")]);
        let source = dir.path().join("source.rom");
        std::fs::write(&source, rom_bytes(4096)).unwrap();

        let copied = copy_rom_to_card(dir.path(), &source, "kick.rom", true).unwrap();
        assert_eq!(
            std::fs::read(dir.path().join("kick.rom")).unwrap(),
            rom_bytes(4096)
        );

        // ART never deletes a ROM: replacing one keeps it, and says where.
        let backup = copied.backup.expect("the replaced ROM was not backed up");
        assert_eq!(
            std::fs::read_to_string(&backup).unwrap(),
            "the one already there"
        );
    }

    #[test]
    fn a_file_that_cannot_be_a_rom_never_reaches_the_card() {
        // Identified before it is copied, so a mistaken pick is refused rather
        // than written and discovered on the Amiga.
        let dir = card(&[]);
        let source = dir.path().join("empty.rom");
        std::fs::write(&source, b"").unwrap();

        assert!(copy_rom_to_card(dir.path(), &source, "kick.rom", false).is_err());
        assert!(!dir.path().join("kick.rom").exists());
    }

    #[test]
    fn rom_suitability_is_an_opinion_and_only_where_there_is_one() {
        use hardware::AmigaTarget;

        let known = RomInfo {
            name: "Kickstart 3.1".into(),
            version: "3.1".into(),
            revision: "40.63".into(),
            size_bytes: 512 * 1024,
            sha256: String::new(),
            crc32: String::new(),
            is_cloanto: false,
            key_available: false,
            is_aros: false,
            checksum: crate::core::rom::RomChecksum::Valid,
            compatible_models: vec!["A500".into(), "A600".into(), "A2000".into()],
            file_path: String::new(),
            major: Some(40),
            whdload_crc16: None,
        };
        assert_eq!(rom_suits(&known, AmigaTarget::A500), Some(true));
        // A note, not a block: people boot odd combinations on purpose.
        assert_eq!(rom_suits(&known, AmigaTarget::A1200), Some(false));

        let unknown = RomInfo {
            version: "Custom".into(),
            compatible_models: vec!["Unknown".into()],
            ..known.clone()
        };
        assert_eq!(
            rom_suits(&unknown, AmigaTarget::A500),
            None,
            "an unrecognised ROM has no opinion attached"
        );
    }

    // ---- F3: named firmware sets ------------------------------------------

    #[test]
    fn a_named_set_is_created_from_the_cards_current_config() {
        let dir = card(&[("config.txt", "arm_64bit=1\ngpu_mem=64\n")]);
        write_config_set(
            dir.path(),
            "os39",
            ConfigSetSource::CurrentConfig,
            None,
            &PistormSetup::default(),
        )
        .unwrap();

        assert_eq!(
            std::fs::read_to_string(dir.path().join("config_os39.txt")).unwrap(),
            "arm_64bit=1\ngpu_mem=64\n"
        );
        let found = scan_card(dir.path(), PistormHardware::default()).unwrap();
        assert_eq!(found.config_sets, vec!["os39"]);
    }

    #[test]
    fn a_set_is_duplicated_verbatim() {
        let dir = card(&[("config.txt", "x=1\n"), ("config_os39.txt", "os39 only\n")]);
        write_config_set(
            dir.path(),
            "os39-copy",
            ConfigSetSource::Set,
            Some("os39"),
            &PistormSetup::default(),
        )
        .unwrap();
        assert_eq!(
            std::fs::read_to_string(dir.path().join("config_os39-copy.txt")).unwrap(),
            "os39 only\n"
        );
    }

    #[test]
    fn duplicating_a_set_that_is_not_there_says_so() {
        let dir = card(&[("config.txt", "x=1\n")]);
        let err = write_config_set(
            dir.path(),
            "copy",
            ConfigSetSource::Set,
            Some("nope"),
            &PistormSetup::default(),
        )
        .unwrap_err();
        assert!(err.to_string().contains("nope"), "{err}");
    }

    #[test]
    fn a_set_name_never_becomes_a_path() {
        // It arrives from the frontend, so it goes through the same gate every
        // other name-to-path in ART goes through.
        let dir = card(&[("config.txt", "x=1\n")]);
        for name in ["../boot", "..\\boot", "a/b", "a b", "os39!"] {
            assert!(
                write_config_set(
                    dir.path(),
                    name,
                    ConfigSetSource::CurrentConfig,
                    None,
                    &PistormSetup::default()
                )
                .is_err(),
                "{name} was accepted"
            );
        }
        assert!(!dir
            .path()
            .parent()
            .unwrap()
            .join("config_boot.txt")
            .exists());
    }

    #[test]
    fn an_empty_set_is_refused_rather_than_written() {
        // A set of nothing activates as a `config.txt` of nothing, and a Pi
        // with an empty `config.txt` does not boot.
        let dir = card(&[]);
        let err = write_config_set(
            dir.path(),
            "os39",
            ConfigSetSource::CurrentConfig,
            None,
            &PistormSetup::default(),
        )
        .unwrap_err();
        assert!(err.to_string().contains("config.txt"), "{err}");
        assert!(!dir.path().join("config_os39.txt").exists());
    }

    #[test]
    fn activating_a_set_replaces_config_txt_and_keeps_the_old_one() {
        let dir = card(&[
            ("config.txt", "the one that was active\n"),
            ("config_os39.txt", "the os39 one\n"),
        ]);

        let preview = preview_activate_config_set(dir.path(), "os39").unwrap();
        assert_eq!(preview.before, "the one that was active\n");
        assert_eq!(preview.after, "the os39 one\n");
        // Nothing written yet.
        assert_eq!(
            std::fs::read_to_string(dir.path().join("config.txt")).unwrap(),
            "the one that was active\n"
        );

        let backup = activate_config_set(dir.path(), "os39")
            .unwrap()
            .expect("a backup");
        assert_eq!(
            std::fs::read_to_string(dir.path().join("config.txt")).unwrap(),
            "the os39 one\n"
        );
        assert_eq!(
            std::fs::read_to_string(&backup).unwrap(),
            "the one that was active\n"
        );
        // The set itself is untouched, so activating another is reversible.
        assert_eq!(
            std::fs::read_to_string(dir.path().join("config_os39.txt")).unwrap(),
            "the os39 one\n"
        );
    }

    #[test]
    fn activating_a_set_that_is_not_there_is_refused_before_anything_is_touched() {
        let dir = card(&[("config.txt", "unchanged\n")]);
        assert!(activate_config_set(dir.path(), "nope").is_err());
        assert_eq!(
            std::fs::read_to_string(dir.path().join("config.txt")).unwrap(),
            "unchanged\n"
        );
    }

    /// ART-092. Deferred out of the fix round because removing a user's
    /// configuration needs its own shape, not a bare button — this is that
    /// shape: backed up first, so "deleted" means recoverable.
    #[test]
    fn a_set_is_deleted_and_kept() {
        let dir = card(&[
            ("config.txt", "the active one\n"),
            ("config_os39.txt", "the os39 one\n"),
        ]);

        let backup = delete_config_set(dir.path(), "os39")
            .unwrap()
            .expect("a deleted set must be recoverable");
        assert!(!dir.path().join("config_os39.txt").exists());
        assert_eq!(std::fs::read_to_string(&backup).unwrap(), "the os39 one\n");
    }

    #[test]
    fn the_set_the_card_is_running_is_not_deletable() {
        // Deleting it would take away the only copy of the configuration the
        // card boots from. Make another active first.
        let dir = card(&[
            ("config.txt", "shared text\n"),
            ("config_os39.txt", "shared text\n"),
        ]);

        let err = delete_config_set(dir.path(), "os39").unwrap_err();
        assert!(err.to_string().contains("os39"), "{err}");
        assert!(dir.path().join("config_os39.txt").exists());
    }

    #[test]
    fn deleting_cannot_reach_the_active_config_or_anything_outside_the_card() {
        let dir = card(&[("config.txt", "the active one\n")]);

        // There is no name that spells the plain `config.txt`…
        assert!(delete_config_set(dir.path(), "").is_err());
        assert!(delete_config_set(dir.path(), ".txt").is_err());
        // …and none that leaves the card.
        for name in ["../config", "..\\config", "a/b"] {
            assert!(delete_config_set(dir.path(), name).is_err(), "{name}");
        }
        assert_eq!(
            std::fs::read_to_string(dir.path().join("config.txt")).unwrap(),
            "the active one\n",
            "config.txt must be untouched"
        );
    }

    #[test]
    fn deleting_a_set_that_is_not_there_says_so() {
        let dir = card(&[("config.txt", "x\n")]);
        let err = delete_config_set(dir.path(), "nope").unwrap_err();
        assert!(err.to_string().contains("nope"), "{err}");
    }

    #[test]
    fn a_set_is_renamed_and_the_old_name_stops_existing() {
        let dir = card(&[("config_os39.txt", "text\n")]);
        rename_config_set(dir.path(), "os39", "workbench").unwrap();
        assert!(!dir.path().join("config_os39.txt").exists());
        assert_eq!(
            std::fs::read_to_string(dir.path().join("config_workbench.txt")).unwrap(),
            "text\n"
        );
    }

    #[test]
    fn a_rename_onto_a_name_already_taken_is_refused_and_loses_nothing() {
        let dir = card(&[("config_os39.txt", "a\n"), ("config_os32.txt", "b\n")]);
        assert!(rename_config_set(dir.path(), "os39", "os32").is_err());
        assert_eq!(
            std::fs::read_to_string(dir.path().join("config_os39.txt")).unwrap(),
            "a\n"
        );
        assert_eq!(
            std::fs::read_to_string(dir.path().join("config_os32.txt")).unwrap(),
            "b\n"
        );
    }

    #[test]
    fn two_sets_and_a_switch_between_them() {
        // The acceptance case from the brief, end to end.
        let dir = card(&[("config.txt", "arm_64bit=1\nhdmi_group=2\n")]);
        write_config_set(
            dir.path(),
            "os32",
            ConfigSetSource::CurrentConfig,
            None,
            &PistormSetup::default(),
        )
        .unwrap();
        std::fs::write(dir.path().join("config.txt"), "arm_64bit=1\nos39 flavour\n").unwrap();
        write_config_set(
            dir.path(),
            "os39",
            ConfigSetSource::CurrentConfig,
            None,
            &PistormSetup::default(),
        )
        .unwrap();

        let found = scan_card(dir.path(), PistormHardware::default()).unwrap();
        assert_eq!(found.config_sets, vec!["os32", "os39"]);

        activate_config_set(dir.path(), "os32").unwrap();
        assert_eq!(
            std::fs::read_to_string(dir.path().join("config.txt")).unwrap(),
            "arm_64bit=1\nhdmi_group=2\n"
        );
    }

    #[test]
    fn an_empty_folder_is_not_a_pistorm_card() {
        let dir = card(&[]);
        let found = scan_card(dir.path(), PistormHardware::default()).unwrap();
        assert!(!found.is_pistorm_card);
        assert!(!found.has_kernel);
        assert!(found.kickstart_files.is_empty());
    }

    #[test]
    fn a_missing_folder_is_refused_by_name() {
        let dir = card(&[]);
        let err = scan_card(&dir.path().join("nope"), PistormHardware::default()).unwrap_err();
        assert!(err.to_string().contains("nope"), "{err}");
    }

    #[test]
    fn a_card_is_read_back_with_the_settings_it_carries() {
        let dir = card(&[
            (
                "cmdline.txt",
                "root=/dev/mmcblk0p2 vbr_move copy_rom=1024 sd.unit0=rw",
            ),
            (
                "config.txt",
                "arm_64bit=1\nkernel=Emu68.img\ninitramfs kick31.rom\ngpu_mem=64\n",
            ),
            ("Emu68.img", "not really a kernel"),
            ("kick31.rom", "not really a rom"),
        ]);

        let found = scan_card(dir.path(), PistormHardware::default()).unwrap();
        assert!(found.is_pistorm_card);
        assert!(found.has_kernel);
        assert!(found.has_config_txt && found.has_cmdline_txt);
        assert_eq!(
            found
                .kickstart_files
                .iter()
                .map(|rom| rom.file_name.as_str())
                .collect::<Vec<_>>(),
            vec!["kick31.rom"]
        );
        assert!(found.setup.options.vbr_move);
        assert_eq!(found.setup.options.copy_rom_kb, Some(1024));
        assert_eq!(found.setup.firmware.kickstart_file, "kick31.rom");
        // The user's own boot parameters, listed so they can see them survive.
        assert_eq!(found.unmanaged_cmdline, vec!["root=/dev/mmcblk0p2"]);
    }

    #[test]
    fn a_card_written_by_another_tool_is_found_whatever_its_case() {
        // FAT32 is case-insensitive and other imagers write `CONFIG.TXT`.
        let dir = card(&[
            ("CONFIG.TXT", "arm_64bit=1\n"),
            ("CMDLINE.TXT", "root=/dev/x"),
        ]);
        let found = scan_card(dir.path(), PistormHardware::default()).unwrap();
        assert!(found.has_config_txt);
        assert!(found.has_cmdline_txt);
        assert!(found.is_pistorm_card);
    }

    #[test]
    fn a_preview_shows_both_files_whole_before_anything_is_written() {
        let dir = card(&[
            ("cmdline.txt", "root=/dev/mmcblk0p2 emu68.jit=1"),
            ("config.txt", "gpu_mem=64\n"),
        ]);
        let mut setup = PistormSetup::default();
        setup.options.enable_cache = true;

        let preview = preview_save(dir.path(), &setup).unwrap();
        assert!(preview.cmdline_before.contains("emu68.jit"));
        assert!(!preview.cmdline_after.contains("emu68.jit"));
        assert!(preview.cmdline_after.contains("enable_cache"));
        assert!(preview.cmdline_after.contains("root=/dev/mmcblk0p2"));
        assert!(preview.config_after.contains("gpu_mem=64"));

        // And nothing has been written yet.
        assert_eq!(
            std::fs::read_to_string(dir.path().join("cmdline.txt")).unwrap(),
            "root=/dev/mmcblk0p2 emu68.jit=1"
        );
    }

    #[test]
    fn saving_backs_up_what_was_there() {
        let dir = card(&[
            ("cmdline.txt", "root=/dev/mmcblk0p2"),
            ("config.txt", "gpu_mem=64\n"),
        ]);
        let outcome = save_card(dir.path(), &PistormSetup::default()).unwrap();

        let cmdline_backup = outcome.cmdline_txt_backup.expect("a backup path");
        assert_eq!(
            std::fs::read_to_string(&cmdline_backup).unwrap(),
            "root=/dev/mmcblk0p2",
            "the backup must hold what was there before"
        );
        assert!(outcome.config_txt_backup.is_some());
    }

    #[test]
    fn saving_never_loses_the_parameter_the_pi_boots_by() {
        // The one that matters most: without `root=` the Pi does not start,
        // and nothing on the Amiga side would ever explain why.
        let dir = card(&[(
            "cmdline.txt",
            "console=tty1 root=/dev/mmcblk0p2 rootfstype=ext4 rootwait",
        )]);
        save_card(dir.path(), &PistormSetup::default()).unwrap();

        let after = std::fs::read_to_string(dir.path().join("cmdline.txt")).unwrap();
        assert!(after.contains("root=/dev/mmcblk0p2"), "{after}");
        assert!(after.contains("rootfstype=ext4"), "{after}");
        assert!(after.contains("rootwait"), "{after}");
    }

    #[test]
    fn what_a_card_reads_back_as_is_what_was_saved_to_it() {
        let dir = card(&[
            ("cmdline.txt", "root=/dev/x"),
            ("config.txt", "gpu_mem=64\n"),
        ]);
        let hardware = PistormHardware {
            amiga: AmigaTarget::A1200,
            variant: PistormVariant::Pistorm32Lite,
            pi: PiModel::Cm4,
        };
        let setup = PistormSetup {
            hardware,
            line: Emu68Line::default(),
            options: options::profile_options(options::Emu68Profile::Performance, hardware),
            firmware: FirmwareConfig {
                kickstart_file: "kick31.rom".into(),
                display: firmware::DisplayMode::Dmt1080p60,
                ..FirmwareConfig::default()
            },
        };

        save_card(dir.path(), &setup).unwrap();
        let found = scan_card(dir.path(), hardware).unwrap();
        assert_eq!(found.setup.options, setup.options);
        assert_eq!(found.setup.firmware, setup.firmware);
    }

    #[test]
    fn named_config_sets_are_listed_and_the_plain_one_is_not_among_them() {
        let dir = card(&[
            ("config.txt", ""),
            ("config_os39.txt", ""),
            ("config_os32.txt", ""),
            ("config_.txt", ""),
            ("notes.txt", ""),
        ]);
        let found = scan_card(dir.path(), PistormHardware::default()).unwrap();
        assert_eq!(found.config_sets, vec!["os32", "os39"]);
    }
}
