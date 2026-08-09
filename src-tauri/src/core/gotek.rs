//! Gotek USB & FlashFloppy Drive Engine (Phase 5).
//!
//! Provides parsing, generation, and validation of FlashFloppy `FF.CFG` hardware
//! configurations, `IMAGE_A.CFG` Quickslot mappings, Indexed mode (`DSKA0000.ADF`),
//! and parametric navigation modes for real Amiga floppy drive emulators.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

use crate::core::error::{CoreError, CoreResult};

/// Parametric FlashFloppy Navigation Modes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FlashFloppyNavMode {
    /// Native folder browsing on OLED / LCD screen (Long filenames)
    NativeFolders,
    /// Quickslots Mode (IMAGE_A.CFG mapped slots 000..999)
    Quickslots,
    /// Indexed Mode (DSKA0000.ADF for classic 3-digit 7-segment displays)
    Indexed,
}

impl FlashFloppyNavMode {
    pub fn to_cfg_str(self) -> &'static str {
        match self {
            Self::NativeFolders => "native",
            Self::Quickslots => "quickslot",
            Self::Indexed => "indexed",
        }
    }

    pub fn from_cfg_str(s: &str) -> Self {
        match s.trim().to_lowercase().as_str() {
            "indexed" | "index" => Self::Indexed,
            "quickslot" | "slots" => Self::Quickslots,
            _ => Self::NativeFolders,
        }
    }
}

/// FlashFloppy Display Hardware Options.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FlashFloppyDisplay {
    Oled128x32,
    Oled128x64,
    Lcd16x2,
    Digit7Segment,
}

impl FlashFloppyDisplay {
    pub fn to_cfg_str(self) -> &'static str {
        match self {
            Self::Oled128x32 => "oled-128x32",
            Self::Oled128x64 => "oled-128x64",
            Self::Lcd16x2 => "lcd-16x2",
            Self::Digit7Segment => "7seg",
        }
    }

    pub fn from_cfg_str(s: &str) -> Self {
        let l = s.trim().to_lowercase();
        if l.contains("128x64") {
            Self::Oled128x64
        } else if l.contains("128x32") || l.contains("oled") {
            Self::Oled128x32
        } else if l.contains("lcd") || l.contains("16x2") {
            Self::Lcd16x2
        } else {
            Self::Digit7Segment
        }
    }
}

/// Rotary Encoder Mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RotaryMode {
    Track,
    Quickslot,
    Buttons,
    Half,
}

impl RotaryMode {
    pub fn to_cfg_str(self) -> &'static str {
        match self {
            Self::Track => "track",
            Self::Quickslot => "quickslot",
            Self::Buttons => "buttons",
            Self::Half => "half",
        }
    }

    pub fn from_cfg_str(s: &str) -> Self {
        match s.trim().to_lowercase().as_str() {
            "quickslot" | "slot" => Self::Quickslot,
            "buttons" => Self::Buttons,
            "half" => Self::Half,
            _ => Self::Track,
        }
    }
}

/// Comprehensive FlashFloppy Configuration (`FF.CFG`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlashFloppyConfig {
    pub nav_mode: FlashFloppyNavMode,
    pub display_type: FlashFloppyDisplay,
    pub oled_font: String,
    pub rotary: RotaryMode,
    pub step_volume: u8,
    pub interface: String,
    pub host: String,
    pub write_protect: bool,
    pub side_select_polarity: String,
}

impl Default for FlashFloppyConfig {
    fn default() -> Self {
        Self {
            nav_mode: FlashFloppyNavMode::NativeFolders,
            display_type: FlashFloppyDisplay::Oled128x32,
            oled_font: "6x13".into(),
            rotary: RotaryMode::Track,
            step_volume: 20,
            interface: "amiga".into(),
            host: "amiga".into(),
            write_protect: false,
            side_select_polarity: "high".into(),
        }
    }
}

/// A Quickslot mapping entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GotekSlot {
    pub slot_num: u16,
    pub file_path: String,
    pub title: String,
}

/// Complete information about a scanned Gotek USB Drive.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GotekDriveInfo {
    pub drive_path: String,
    pub is_flashfloppy: bool,
    pub config: FlashFloppyConfig,
    pub slots: Vec<GotekSlot>,
    pub adf_files: Vec<String>,
}

/// Parse `FF.CFG` text into a structured config.
pub fn parse_ff_cfg(text: &str) -> FlashFloppyConfig {
    let mut cfg = FlashFloppyConfig::default();
    let mut map = HashMap::new();

    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with(';') {
            continue;
        }
        if let Some((k, v)) = trimmed.split_once('=') {
            map.insert(k.trim().to_lowercase(), v.trim().to_string());
        }
    }

    if let Some(v) = map.get("nav-mode") {
        cfg.nav_mode = FlashFloppyNavMode::from_cfg_str(v);
    } else if let Some(v) = map.get("indexed-mode") {
        if v.eq_ignore_ascii_case("yes") || v.eq_ignore_ascii_case("true") {
            cfg.nav_mode = FlashFloppyNavMode::Indexed;
        }
    }

    if let Some(v) = map.get("display-type") {
        cfg.display_type = FlashFloppyDisplay::from_cfg_str(v);
    }

    if let Some(v) = map.get("oled-font") {
        cfg.oled_font = v.clone();
    }

    if let Some(v) = map.get("rotary") {
        cfg.rotary = RotaryMode::from_cfg_str(v);
    }

    if let Some(v) = map.get("step-volume") {
        if let Ok(vol) = v.parse::<u8>() {
            cfg.step_volume = vol.min(100);
        }
    }

    if let Some(v) = map.get("interface") {
        cfg.interface = v.clone();
    }

    if let Some(v) = map.get("host") {
        cfg.host = v.clone();
    }

    if let Some(v) = map.get("write-protect") {
        cfg.write_protect = v.eq_ignore_ascii_case("yes") || v.eq_ignore_ascii_case("true");
    }

    cfg
}

/// The `FF.CFG` keys ART understands and is allowed to rewrite.
///
/// FlashFloppy supports dozens of settings beyond these, and Gotek owners tune
/// them by hand. Anything not in this list is none of ART's business.
fn managed_values(config: &FlashFloppyConfig) -> Vec<(&'static str, String)> {
    vec![
        ("interface", config.interface.clone()),
        ("host", config.host.clone()),
        ("nav-mode", config.nav_mode.to_cfg_str().to_string()),
        ("display-type", config.display_type.to_cfg_str().to_string()),
        ("oled-font", config.oled_font.clone()),
        ("rotary", config.rotary.to_cfg_str().to_string()),
        ("step-volume", config.step_volume.to_string()),
        (
            "write-protect",
            if config.write_protect { "yes" } else { "no" }.to_string(),
        ),
        ("side-select-polarity", config.side_select_polarity.clone()),
    ]
}

/// Generate `FF.CFG` text.
///
/// When `existing` is supplied the file is **edited, not regenerated**: comments,
/// ordering, and every setting ART does not manage are passed through verbatim,
/// and only the managed keys are updated in place. Managed keys the file lacks
/// are appended at the end.
///
/// Spec §39 requires exactly this — *"Unknown settings must be preserved. Never
/// silently discard configuration entries."* Rewriting the file from scratch
/// would wipe hand-tuned settings such as `pin02`, `head-settle-ms` or
/// `display-order`, which for a Gotek owner is data loss.
pub fn generate_ff_cfg(config: &FlashFloppyConfig, existing: Option<&str>) -> String {
    let managed = managed_values(config);

    let Some(existing) = existing.filter(|t| !t.trim().is_empty()) else {
        // No file yet: write a fresh one containing only what ART manages.
        let mut lines = vec![
            "# Amiga Retro Toolkit (ART) Generated FlashFloppy Configuration".to_string(),
            "# Compatible with FlashFloppy v3.x / v4.x on Commodore Amiga".to_string(),
            String::new(),
        ];
        for (k, v) in &managed {
            lines.push(format!("{k} = {v}"));
        }
        lines.push(String::new());
        return lines.join("\n");
    };

    let mut seen: Vec<&'static str> = Vec::new();
    let mut out: Vec<String> = Vec::new();

    for line in existing.lines() {
        let trimmed = line.trim();

        // Blank lines and comments survive untouched.
        if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with(';') {
            out.push(line.to_string());
            continue;
        }

        if let Some((raw_key, _)) = trimmed.split_once('=') {
            let key = raw_key.trim().to_lowercase();
            if let Some((mk, mv)) = managed.iter().find(|(mk, _)| *mk == key) {
                seen.push(mk);
                out.push(format!("{mk} = {mv}"));
                continue;
            }
        }

        // Unknown key, or a line we cannot parse: preserve it exactly.
        out.push(line.to_string());
    }

    let missing: Vec<&(&str, String)> = managed.iter().filter(|(k, _)| !seen.contains(k)).collect();

    if !missing.is_empty() {
        out.push(String::new());
        out.push("# Added by Amiga Retro Toolkit (ART)".to_string());
        for (k, v) in missing {
            out.push(format!("{k} = {v}"));
        }
    }

    out.push(String::new());
    out.join("\n")
}

/// Parse `IMAGE_A.CFG` slot mappings.
pub fn parse_image_a_cfg(text: &str) -> Vec<GotekSlot> {
    let mut slots = Vec::new();

    for (idx, line) in text.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        // Format can be: "001=Games/MonkeyIsland1.adf" or simply "Games/MonkeyIsland1.adf"
        let (slot_num, path) = if let Some((k, v)) = trimmed.split_once('=') {
            let num = k.trim().parse::<u16>().unwrap_or(idx as u16);
            (num, v.trim().to_string())
        } else {
            (idx as u16, trimmed.to_string())
        };

        let title = Path::new(&path)
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| path.clone());

        slots.push(GotekSlot {
            slot_num,
            file_path: path,
            title,
        });
    }

    slots.sort_by_key(|s| s.slot_num);
    slots
}

/// Generate `IMAGE_A.CFG` text representation.
pub fn generate_image_a_cfg(slots: &[GotekSlot]) -> String {
    let mut lines = Vec::new();
    lines.push("# Amiga Retro Toolkit (ART) FlashFloppy Quickslots".to_string());
    lines.push("".to_string());

    for s in slots {
        lines.push(format!("{:03}={}", s.slot_num, s.file_path));
    }

    lines.join("\n")
}

/// Scan a Gotek USB drive root directory.
pub fn scan_gotek_drive(root: &Path) -> CoreResult<GotekDriveInfo> {
    if !root.is_dir() {
        return Err(CoreError::InvalidInput(format!(
            "Directory not found at '{}'",
            root.display()
        )));
    }

    let ff_cfg_path = root.join("FF.CFG");
    let image_a_path = root.join("IMAGE_A.CFG");

    let config = if ff_cfg_path.is_file() {
        let txt = std::fs::read_to_string(&ff_cfg_path).unwrap_or_default();
        parse_ff_cfg(&txt)
    } else {
        FlashFloppyConfig::default()
    };

    let slots = if image_a_path.is_file() {
        let txt = std::fs::read_to_string(&image_a_path).unwrap_or_default();
        parse_image_a_cfg(&txt)
    } else {
        Vec::new()
    };

    // Scan for all ADF files in the drive
    let mut adf_files = Vec::new();
    find_adfs_recursive(root, root, &mut adf_files);

    let is_flashfloppy = ff_cfg_path.is_file() || image_a_path.is_file() || !adf_files.is_empty();

    Ok(GotekDriveInfo {
        drive_path: root.to_string_lossy().to_string(),
        is_flashfloppy,
        config,
        slots,
        adf_files,
    })
}

fn find_adfs_recursive(base: &Path, current: &Path, acc: &mut Vec<String>) {
    if let Ok(entries) = std::fs::read_dir(current) {
        for entry in entries.flatten() {
            let p = entry.path();
            if p.is_dir() {
                find_adfs_recursive(base, &p, acc);
            } else if p.is_file() {
                if let Some(ext) = p.extension() {
                    let ext_str = ext.to_string_lossy().to_lowercase();
                    if ext_str == "adf" || ext_str == "adz" || ext_str == "hfe" {
                        if let Ok(rel) = p.strip_prefix(base) {
                            acc.push(rel.to_string_lossy().replace('\\', "/"));
                        }
                    }
                }
            }
        }
    }
}

/// Save FlashFloppy configuration and Quickslot mappings to USB drive.
/// Paths of the backups taken while saving a Gotek drive.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GotekSaveOutcome {
    pub ff_cfg_backup: Option<String>,
    pub image_a_backup: Option<String>,
}

/// Write FlashFloppy configuration and Quickslots to a USB drive.
///
/// Both files are backed up first and written atomically, and `FF.CFG` is
/// *edited* rather than regenerated so hand-tuned settings survive.
pub fn save_gotek_drive(
    root: &Path,
    config: &FlashFloppyConfig,
    slots: &[GotekSlot],
) -> CoreResult<GotekSaveOutcome> {
    use crate::core::safety::{guarded_write, BackupPolicy};

    if !root.is_dir() {
        return Err(CoreError::InvalidInput("Invalid USB root path".into()));
    }

    let mut outcome = GotekSaveOutcome::default();

    // Read the drive's current FF.CFG so unmanaged settings are carried over.
    let ff_path = root.join("FF.CFG");
    let existing = std::fs::read_to_string(&ff_path).ok();
    let ff_text = generate_ff_cfg(config, existing.as_deref());
    outcome.ff_cfg_backup = guarded_write(&ff_path, ff_text.as_bytes(), BackupPolicy::CONFIG)?
        .map(|p| p.to_string_lossy().into_owned());

    if !slots.is_empty() || config.nav_mode == FlashFloppyNavMode::Quickslots {
        let slots_path = root.join("IMAGE_A.CFG");
        let slots_text = generate_image_a_cfg(slots);
        outcome.image_a_backup =
            guarded_write(&slots_path, slots_text.as_bytes(), BackupPolicy::CONFIG)?
                .map(|p| p.to_string_lossy().into_owned());
    }

    Ok(outcome)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_and_generate_ff_cfg() {
        let sample = r#"
        interface = amiga
        host = amiga
        nav-mode = native
        display-type = oled-128x64
        step-volume = 40
        write-protect = yes
        "#;

        let parsed = parse_ff_cfg(sample);
        assert_eq!(parsed.display_type, FlashFloppyDisplay::Oled128x64);
        assert_eq!(parsed.nav_mode, FlashFloppyNavMode::NativeFolders);
        assert_eq!(parsed.step_volume, 40);
        assert!(parsed.write_protect);

        let out = generate_ff_cfg(&parsed, None);
        assert!(out.contains("display-type = oled-128x64"));
        assert!(out.contains("step-volume = 40"));
        assert!(out.contains("write-protect = yes"));
    }

    /// Spec §39: FlashFloppy has dozens of settings ART knows nothing about,
    /// and Gotek owners tune them by hand. Saving must never drop them.
    #[test]
    fn round_trip_preserves_unknown_settings() {
        let original = "\
# My hand-tuned Gotek — do not clobber
interface = amiga
pin02 = nrdy
pin34 = rdy
head-settle-ms = 12
display-order = 1,2,0
step-volume = 40
; a semicolon comment
ejected-on-startup = yes
";
        let mut cfg = parse_ff_cfg(original);
        cfg.step_volume = 15;

        let out = generate_ff_cfg(&cfg, Some(original));

        // Every unmanaged key survives, byte for byte.
        assert!(out.contains("pin02 = nrdy"), "got:\n{out}");
        assert!(out.contains("pin34 = rdy"));
        assert!(out.contains("head-settle-ms = 12"));
        assert!(out.contains("display-order = 1,2,0"));
        assert!(out.contains("ejected-on-startup = yes"));

        // Comments survive too.
        assert!(out.contains("# My hand-tuned Gotek — do not clobber"));
        assert!(out.contains("; a semicolon comment"));

        // The managed key really was updated, and only once.
        assert!(out.contains("step-volume = 15"));
        assert!(!out.contains("step-volume = 40"));
        assert_eq!(out.matches("step-volume").count(), 1);
    }

    #[test]
    fn round_trip_appends_managed_keys_the_file_lacks() {
        let original = "pin02 = nrdy\n";
        let cfg = FlashFloppyConfig::default();

        let out = generate_ff_cfg(&cfg, Some(original));

        assert!(out.contains("pin02 = nrdy"));
        assert!(out.contains("nav-mode = native"));
        assert!(out.contains("interface = amiga"));
    }

    #[test]
    fn round_trip_keeps_the_original_key_order() {
        let original = "step-volume = 40\ninterface = amiga\nhost = amiga\n";
        let cfg = parse_ff_cfg(original);

        let out = generate_ff_cfg(&cfg, Some(original));
        let body: Vec<&str> = out
            .lines()
            .filter(|l| !l.trim().is_empty() && !l.starts_with('#'))
            .collect();

        assert_eq!(body[0], "step-volume = 40");
        assert_eq!(body[1], "interface = amiga");
        assert_eq!(body[2], "host = amiga");
    }

    #[test]
    fn empty_existing_file_produces_a_fresh_config() {
        let cfg = FlashFloppyConfig::default();
        let out = generate_ff_cfg(&cfg, Some("   \n\n"));
        assert!(out.contains("Amiga Retro Toolkit"));
        assert!(out.contains("interface = amiga"));
    }

    #[test]
    fn saving_backs_up_the_previous_config() {
        let dir = std::env::temp_dir().join(format!(
            "art-gotek-save-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let ff = dir.join("FF.CFG");
        std::fs::write(&ff, "pin02 = nrdy\nstep-volume = 40\n").unwrap();

        let mut cfg = parse_ff_cfg(&std::fs::read_to_string(&ff).unwrap());
        cfg.step_volume = 5;
        let outcome = save_gotek_drive(&dir, &cfg, &[]).unwrap();

        let backup = outcome.ff_cfg_backup.expect("a backup was expected");
        assert!(std::fs::read_to_string(&backup)
            .unwrap()
            .contains("step-volume = 40"));

        let saved = std::fs::read_to_string(&ff).unwrap();
        assert!(saved.contains("step-volume = 5"));
        assert!(saved.contains("pin02 = nrdy"));

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn parse_and_generate_image_a_cfg() {
        let sample = "000=Games/Lemmings_Disk1.adf\n001=Games/Lemmings_Disk2.adf";
        let slots = parse_image_a_cfg(sample);
        assert_eq!(slots.len(), 2);
        assert_eq!(slots[0].slot_num, 0);
        assert_eq!(slots[0].file_path, "Games/Lemmings_Disk1.adf");
        assert_eq!(slots[1].slot_num, 1);

        let out = generate_image_a_cfg(&slots);
        assert!(out.contains("000=Games/Lemmings_Disk1.adf"));
        assert!(out.contains("001=Games/Lemmings_Disk2.adf"));
    }
}
