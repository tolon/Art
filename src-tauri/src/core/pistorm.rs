//! PiStorm & Emu68 Modern Hardware Bridge Engine (Phase 6).
//!
//! Handles configuration generation, SD card layout analysis, RTG resolution tuning,
//! Fast RAM mapping, and parametric performance/compatibility profiles for
//! PiStorm (A500, A600, A1200-Lite, A2000) running Emu68 or Musashi.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

use crate::core::error::{CoreError, CoreResult};

/// Supported PiStorm hardware boards.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PistormBoard {
    /// PiStorm for Amiga 500 / 2000 (DIP-64 socket)
    A500A2000,
    /// PiStorm32-Lite for Amiga 1200 (Trapdoor edge connector)
    A1200Lite,
    /// PiStorm600 for Amiga 600 (PLCC socket)
    A600,
}

impl PistormBoard {
    pub fn display_name(self) -> &'static str {
        match self {
            Self::A500A2000 => "PiStorm A500 / A2000",
            Self::A1200Lite => "PiStorm32-Lite (Amiga 1200)",
            Self::A600 => "PiStorm600 (Amiga 600)",
        }
    }
}

/// Parametric Performance & Compatibility Profiles.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PistormProfileMode {
    /// ⚡ AmigaOS Ultra Workstation: Maximum JIT MIPS (~800+ MIPS), 1-2GB Fast RAM, 1080p RTG.
    Workstation,
    /// 🕹️ Balanced Daily & WHDLoad: High JIT + WHDLoad compatibility and RTG/Native switching.
    BalancedWHDLoad,
    /// 🎯 Classic & Demoscene: Cycle-accurate / conservative JIT for timing-sensitive demos.
    ClassicCompat,
}

/// RTG (Picasso96) Video Resolution.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RtgResolution {
    Res1080p,    // 1920x1080 @ 60Hz (Full HD)
    Res720p,     // 1280x720 @ 60Hz (HD Ready)
    Res1024x768, // 1024x768 @ 60Hz (Classic 4:3)
    Res800x600,  // 800x600 @ 60Hz
    Disabled,    // Native Amiga Chipset only
}

impl RtgResolution {
    pub fn to_dimensions(self) -> (u32, u32) {
        match self {
            Self::Res1080p => (1920, 1080),
            Self::Res720p => (1280, 720),
            Self::Res1024x768 => (1024, 768),
            Self::Res800x600 => (800, 600),
            Self::Disabled => (0, 0),
        }
    }
}

/// Complete PiStorm & Emu68 Configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PistormConfig {
    pub board: PistormBoard,
    pub profile_mode: PistormProfileMode,
    pub fast_ram_mb: u32,
    pub rtg_resolution: RtgResolution,
    pub enable_jit: bool,
    pub enable_mmu: bool,
    pub enable_wifi: bool,
    pub enable_sd_storage: bool,
    pub custom_kickstart_path: Option<String>,
}

impl Default for PistormConfig {
    fn default() -> Self {
        Self {
            board: PistormBoard::A500A2000,
            profile_mode: PistormProfileMode::Workstation,
            fast_ram_mb: 1024, // 1 GB Fast RAM
            rtg_resolution: RtgResolution::Res1080p,
            enable_jit: true,
            enable_mmu: true,
            enable_wifi: true,
            enable_sd_storage: true,
            custom_kickstart_path: Some("kick.rom".into()),
        }
    }
}

/// MicroSD Card Scan Result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PistormDriveInfo {
    pub drive_path: String,
    pub is_pistorm_sd: bool,
    pub has_emu68_img: bool,
    pub has_config_txt: bool,
    pub has_kickstart: bool,
    pub kickstart_name: Option<String>,
    pub detected_config: PistormConfig,
}

/// The `cmdline.txt` parameters ART manages. Everything else on the line —
/// `console=`, `root=`, `rootfstype=`, `dwc_otg.*` and friends — belongs to the
/// Raspberry Pi boot process and must survive untouched.
const MANAGED_CMDLINE_KEYS: &[&str] = &[
    "buptest.fastram_size",
    "emu68.jit",
    "emu68.mmu",
    "sd.unit0",
    "kickstart",
];

/// The Emu68 parameters ART wants the command line to contain.
fn managed_cmdline_params(config: &PistormConfig) -> Vec<(String, String)> {
    let mut params = vec![
        (
            "buptest.fastram_size".to_string(),
            config.fast_ram_mb.to_string(),
        ),
        (
            "emu68.jit".to_string(),
            if config.enable_jit { "1" } else { "0" }.to_string(),
        ),
    ];

    if config.enable_mmu {
        params.push(("emu68.mmu".to_string(), "1".to_string()));
    }
    if config.enable_sd_storage {
        params.push(("sd.unit0".to_string(), "0".to_string()));
    }
    if let Some(ref kick) = config.custom_kickstart_path {
        params.push(("kickstart".to_string(), kick.clone()));
    }

    params
}

/// Generate the Emu68 kernel command line.
///
/// When `existing` is supplied the line is **edited in place**: managed
/// parameters are updated where they already appear, unmanaged ones are kept in
/// their original order, and missing managed parameters are appended.
///
/// Regenerating this file from scratch would drop `root=` and `console=`, and a
/// Raspberry Pi missing its root parameter does not boot at all.
pub fn generate_emu68_cmdline(config: &PistormConfig, existing: Option<&str>) -> String {
    let managed = managed_cmdline_params(config);

    let Some(existing) = existing.filter(|t| !t.trim().is_empty()) else {
        return managed
            .iter()
            .map(|(k, v)| format!("{k}={v}"))
            .collect::<Vec<_>>()
            .join(" ");
    };

    let mut out: Vec<String> = Vec::new();
    let mut seen: Vec<&str> = Vec::new();

    for token in existing.split_whitespace() {
        let key = token.split_once('=').map(|(k, _)| k).unwrap_or(token);

        if let Some((mk, mv)) = managed.iter().find(|(mk, _)| mk == key) {
            seen.push(mk.as_str());
            out.push(format!("{mk}={mv}"));
        } else if MANAGED_CMDLINE_KEYS.contains(&key) {
            // A managed key whose feature is now switched off: drop it.
            continue;
        } else {
            // Unmanaged boot parameter — preserve verbatim.
            out.push(token.to_string());
        }
    }

    for (k, v) in &managed {
        if !seen.contains(&k.as_str()) {
            out.push(format!("{k}={v}"));
        }
    }

    out.join(" ")
}

/// Single-valued `config.txt` keys ART manages.
///
/// `dtoverlay` is deliberately absent: a Raspberry Pi config may carry several
/// `dtoverlay=` lines and they are all meaningful, so ART adds its own only
/// when missing rather than rewriting them.
fn managed_config_values(config: &PistormConfig) -> Vec<(&'static str, String)> {
    let mut values = vec![
        ("arm_64bit", "1".to_string()),
        ("kernel", "emu68.img".to_string()),
        ("enable_uart", "1".to_string()),
    ];

    if config.rtg_resolution != RtgResolution::Disabled {
        let (w, h) = config.rtg_resolution.to_dimensions();
        values.extend([
            ("hdmi_group", "2".to_string()),
            ("hdmi_mode", "82".to_string()), // 1080p 60Hz standard or custom CVT
            ("hdmi_cvt", format!("{w} {h} 60 6 0 0 0")),
            ("framebuffer_width", w.to_string()),
            ("framebuffer_height", h.to_string()),
            ("framebuffer_depth", "32".to_string()),
        ]);
    }

    values
}

/// Generate Raspberry Pi `config.txt`.
///
/// When `existing` is supplied the file is **edited, not regenerated**: only
/// the keys ART manages are rewritten, and every other line — `gpu_mem`,
/// `dtparam`, extra `dtoverlay` entries, `[pi4]` conditional sections, comments
/// — is preserved exactly.
///
/// Spec §40: *"ART must never overwrite PiStorm configuration silently."*
pub fn generate_rpi_config_txt(config: &PistormConfig, existing: Option<&str>) -> String {
    let managed = managed_config_values(config);

    let Some(existing) = existing.filter(|t| !t.trim().is_empty()) else {
        let mut lines = vec![
            "# Amiga Retro Toolkit (ART) PiStorm / Emu68 Configuration".to_string(),
            format!("# Board: {}", config.board.display_name()),
            String::new(),
        ];
        for (k, v) in &managed {
            lines.push(format!("{k}={v}"));
        }
        lines.push("dtoverlay=disable-bt".to_string());
        lines.push(String::new());
        return lines.join("\n");
    };

    let mut out: Vec<String> = Vec::new();
    let mut seen: Vec<&'static str> = Vec::new();
    let mut has_disable_bt = false;

    for line in existing.lines() {
        let trimmed = line.trim();

        if trimmed.is_empty() || trimmed.starts_with('#') {
            out.push(line.to_string());
            continue;
        }

        if trimmed.starts_with("dtoverlay=") {
            if trimmed.contains("disable-bt") {
                has_disable_bt = true;
            }
            out.push(line.to_string());
            continue;
        }

        if let Some((raw_key, _)) = trimmed.split_once('=') {
            let key = raw_key.trim();
            if let Some((mk, mv)) = managed.iter().find(|(mk, _)| *mk == key) {
                seen.push(mk);
                out.push(format!("{mk}={mv}"));
                continue;
            }
        }

        out.push(line.to_string());
    }

    let missing: Vec<&(&str, String)> = managed.iter().filter(|(k, _)| !seen.contains(k)).collect();

    if !missing.is_empty() || !has_disable_bt {
        out.push(String::new());
        out.push("# Added by Amiga Retro Toolkit (ART)".to_string());
        for (k, v) in missing {
            out.push(format!("{k}={v}"));
        }
        if !has_disable_bt {
            out.push("dtoverlay=disable-bt".to_string());
        }
    }

    out.push(String::new());
    out.join("\n")
}

/// Scan a PiStorm / Emu68 MicroSD card folder.
pub fn scan_pistorm_sd(root: &Path) -> CoreResult<PistormDriveInfo> {
    if !root.is_dir() {
        return Err(CoreError::InvalidInput(format!(
            "Directory not found at '{}'",
            root.display()
        )));
    }

    let config_txt = root.join("config.txt");
    let emu68_img = root.join("emu68.img");
    let cmdline_txt = root.join("cmdline.txt");

    let has_config_txt = config_txt.is_file();
    let has_emu68_img = emu68_img.is_file();

    // Check for Kickstart ROM candidates
    let kick_candidates = [
        "kick.rom",
        "kickstart.rom",
        "kick32.rom",
        "kick31.rom",
        "amiga-os-320.rom",
    ];
    let mut kickstart_name = None;
    for k in &kick_candidates {
        if root.join(k).is_file() {
            kickstart_name = Some(k.to_string());
            break;
        }
    }
    let has_kickstart = kickstart_name.is_some();

    let mut detected_config = PistormConfig::default();

    if cmdline_txt.is_file() {
        if let Ok(content) = std::fs::read_to_string(&cmdline_txt) {
            let mut map = HashMap::new();
            for part in content.split_whitespace() {
                if let Some((k, v)) = part.split_once('=') {
                    map.insert(k, v);
                }
            }
            if let Some(ram_str) = map.get("buptest.fastram_size") {
                if let Ok(ram) = ram_str.parse::<u32>() {
                    detected_config.fast_ram_mb = ram;
                }
            }
            if let Some(jit_str) = map.get("emu68.jit") {
                detected_config.enable_jit = *jit_str == "1";
            }
            if let Some(kick) = map.get("kickstart") {
                detected_config.custom_kickstart_path = Some(kick.to_string());
            }
        }
    }

    let is_pistorm_sd = has_emu68_img || has_config_txt || has_kickstart;

    Ok(PistormDriveInfo {
        drive_path: root.to_string_lossy().to_string(),
        is_pistorm_sd,
        has_emu68_img,
        has_config_txt,
        has_kickstart,
        kickstart_name,
        detected_config,
    })
}

/// Save PiStorm configuration files (`config.txt` and `cmdline.txt`) to SD card.
/// Paths of the backups taken while saving a PiStorm SD card.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PistormSaveOutcome {
    pub config_txt_backup: Option<String>,
    pub cmdline_txt_backup: Option<String>,
}

/// Save PiStorm / Emu68 configuration to an SD card.
///
/// Both files are read first and merged rather than regenerated, backed up, and
/// written atomically — spec §40 forbids silently overwriting a PiStorm setup.
pub fn save_pistorm_sd(root: &Path, config: &PistormConfig) -> CoreResult<PistormSaveOutcome> {
    use crate::core::safety::{guarded_write, BackupPolicy};

    if !root.is_dir() {
        return Err(CoreError::InvalidInput("Invalid SD Card path".into()));
    }

    let config_path = root.join("config.txt");
    let cmdline_path = root.join("cmdline.txt");

    let existing_config = std::fs::read_to_string(&config_path).ok();
    let existing_cmdline = std::fs::read_to_string(&cmdline_path).ok();

    let config_text = generate_rpi_config_txt(config, existing_config.as_deref());
    let cmdline_text = generate_emu68_cmdline(config, existing_cmdline.as_deref());

    Ok(PistormSaveOutcome {
        config_txt_backup: guarded_write(
            &config_path,
            config_text.as_bytes(),
            BackupPolicy::CONFIG,
        )?
        .map(|p| p.to_string_lossy().into_owned()),
        cmdline_txt_backup: guarded_write(
            &cmdline_path,
            cmdline_text.as_bytes(),
            BackupPolicy::CONFIG,
        )?
        .map(|p| p.to_string_lossy().into_owned()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_workstation_config() {
        let cfg = PistormConfig {
            board: PistormBoard::A1200Lite,
            fast_ram_mb: 2048,
            rtg_resolution: RtgResolution::Res1080p,
            ..Default::default()
        };

        let cmdline = generate_emu68_cmdline(&cfg, None);
        assert!(cmdline.contains("buptest.fastram_size=2048"));
        assert!(cmdline.contains("emu68.jit=1"));
        assert!(cmdline.contains("kickstart=kick.rom"));

        let config_txt = generate_rpi_config_txt(&cfg, None);
        assert!(config_txt.contains("PiStorm32-Lite"));
        assert!(config_txt.contains("kernel=emu68.img"));
        assert!(config_txt.contains("framebuffer_width=1920"));
        assert!(config_txt.contains("framebuffer_height=1080"));
    }

    /// Losing `root=` from cmdline.txt means the Raspberry Pi never boots.
    /// This is the single most destructive thing ART could do to an SD card.
    #[test]
    fn cmdline_round_trip_preserves_boot_parameters() {
        let original = "console=serial0,115200 console=tty1 root=PARTUUID=a1b2c3d4-02 \
                        rootfstype=ext4 elevator=deadline fsck.repair=yes \
                        buptest.fastram_size=128 emu68.jit=0 rootwait";

        let cfg = PistormConfig {
            fast_ram_mb: 512,
            enable_jit: true,
            ..Default::default()
        };

        let out = generate_emu68_cmdline(&cfg, Some(original));

        // Boot parameters survive.
        assert!(out.contains("root=PARTUUID=a1b2c3d4-02"), "got: {out}");
        assert!(out.contains("rootfstype=ext4"));
        assert!(out.contains("console=serial0,115200"));
        assert!(out.contains("console=tty1"));
        assert!(out.contains("elevator=deadline"));
        assert!(out.contains("fsck.repair=yes"));
        assert!(out.contains("rootwait"));

        // Managed parameters updated in place, not duplicated.
        assert!(out.contains("buptest.fastram_size=512"));
        assert!(!out.contains("buptest.fastram_size=128"));
        assert_eq!(out.matches("buptest.fastram_size").count(), 1);
        assert!(out.contains("emu68.jit=1"));
        assert_eq!(out.matches("emu68.jit").count(), 1);
    }

    #[test]
    fn config_txt_round_trip_preserves_unmanaged_settings() {
        let original = "\
# My Pi setup
gpu_mem=128
dtparam=audio=on
dtoverlay=vc4-kms-v3d
dtoverlay=disable-bt
arm_64bit=0
[pi4]
over_voltage=2
";
        let cfg = PistormConfig::default();
        let out = generate_rpi_config_txt(&cfg, Some(original));

        // Everything ART does not manage is still there.
        assert!(out.contains("gpu_mem=128"), "got:\n{out}");
        assert!(out.contains("dtparam=audio=on"));
        assert!(out.contains("dtoverlay=vc4-kms-v3d"));
        assert!(out.contains("[pi4]"));
        assert!(out.contains("over_voltage=2"));
        assert!(out.contains("# My Pi setup"));

        // The managed key was corrected in place.
        assert!(out.contains("arm_64bit=1"));
        assert!(!out.contains("arm_64bit=0"));

        // disable-bt was already present, so it is not added a second time.
        assert_eq!(out.matches("dtoverlay=disable-bt").count(), 1);
    }

    #[test]
    fn saving_backs_up_the_previous_sd_configuration() {
        let dir = std::env::temp_dir().join(format!(
            "art-pistorm-save-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("config.txt"), "gpu_mem=128\narm_64bit=0\n").unwrap();
        std::fs::write(
            dir.join("cmdline.txt"),
            "root=PARTUUID=deadbeef-02 rootwait",
        )
        .unwrap();

        let cfg = PistormConfig::default();
        let outcome = save_pistorm_sd(&dir, &cfg).unwrap();

        assert!(outcome.config_txt_backup.is_some());
        assert!(outcome.cmdline_txt_backup.is_some());

        let saved_cmdline = std::fs::read_to_string(dir.join("cmdline.txt")).unwrap();
        assert!(
            saved_cmdline.contains("root=PARTUUID=deadbeef-02"),
            "the SD card must still boot after ART writes to it"
        );

        let saved_config = std::fs::read_to_string(dir.join("config.txt")).unwrap();
        assert!(saved_config.contains("gpu_mem=128"));

        std::fs::remove_dir_all(&dir).ok();
    }
}
