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

pub mod firmware;
pub mod hardware;
pub mod options;

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::core::error::{CoreError, CoreResult};
use crate::core::safety::{guarded_write, BackupPolicy};
use firmware::{parse_config_txt, FirmwareConfig, KERNEL_IMAGE};
use hardware::PistormHardware;
use options::{parse_cmdline, unmanaged_tokens, Emu68Options};

/// Everything ART sets on one card.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PistormSetup {
    pub hardware: PistormHardware,
    pub options: Emu68Options,
    pub firmware: FirmwareConfig,
}

/// The Kickstart images a card is likely to carry.
///
/// A list of likely names, not a claim about contents: whether a file is really
/// a Kickstart, and which one, is `core/rom`'s question and it answers it by
/// checksum. This only decides what to offer first.
const KICKSTART_CANDIDATES: &[&str] = &[
    "kick.rom",
    "kickstart.rom",
    "kick31.rom",
    "kick32.rom",
    "kick13.rom",
];

/// What ART found on a card.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PistormCard {
    pub path: String,
    /// Whether this looks like a PiStorm card at all.
    pub is_pistorm_card: bool,
    /// The Emu68 kernel image.
    pub has_kernel: bool,
    pub has_config_txt: bool,
    pub has_cmdline_txt: bool,
    /// Kickstart images actually present, whatever `config.txt` names.
    pub kickstart_files: Vec<String>,
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
    let has_kernel = find(KERNEL_IMAGE).is_some();

    let kickstart_files: Vec<String> = KICKSTART_CANDIDATES
        .iter()
        .filter_map(|name| find(name).map(|_| (*name).to_string()))
        .collect();

    let cmdline_text = cmdline_path
        .as_deref()
        .and_then(|path| std::fs::read_to_string(path).ok());
    let config_text = config_path
        .as_deref()
        .and_then(|path| std::fs::read_to_string(path).ok());

    let setup = PistormSetup {
        hardware,
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
        let dir = std::env::temp_dir().join(format!(
            "art-pistorm-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        for (name, contents) in files {
            std::fs::write(dir.join(name), contents).unwrap();
        }
        Card(dir)
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
        assert_eq!(found.kickstart_files, vec!["kick31.rom"]);
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
