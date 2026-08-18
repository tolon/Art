//! WinUAE emulator integration and configuration generator (Phase 2 & Phase 16).
//!
//! Detects local WinUAE executable installations, produces accurate `.uae`
//! configurations from Amiga hardware profiles, and launches emulation sessions.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

use super::profile::{AmigaProfile, ChipsetModel, CpuModel};
use crate::core::error::{CoreError, CoreResult};

/// Details of a detected WinUAE installation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WinUaeInstallation {
    pub found: bool,
    pub executable_path: Option<String>,
    pub version: Option<String>,
    pub is_64bit: bool,
}

/// Media attachments for an emulation launch session.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LaunchMedia {
    pub floppy_paths: Vec<String>,
    pub hardfile_paths: Vec<String>,
    pub kickstart_path: Option<String>,
    pub use_aros: bool,
    /// Mount hard drives read-only so the emulated system cannot modify the
    /// user's HDF images (spec §93 — originals are immutable by default).
    #[serde(default)]
    pub write_protect_hardfiles: bool,
    /// Host folders exposed to the emulated Amiga as `filesystem2=` volumes —
    /// a game's drawer, and/or ART's own boot directory (spec §4.3/§4.4 of the
    /// collection-wave-c design). `#[serde(default)]` so a `LaunchMedia`
    /// stored by an older build, which never wrote this field, still
    /// deserialises instead of failing to load.
    #[serde(default)]
    pub directories: Vec<DirMount>,
}

/// A host folder mounted as an Amiga volume (WinUAE `filesystem2=`).
///
/// `boot_priority` follows the same AmigaDOS `BootPri` convention as
/// `hardfile2=` and the real RDB field it mirrors (`core::rdb::PartitionSpec`,
/// `-128..=127`): during boot, AmigaDOS tries bootable devices in descending
/// priority order, so a *higher* number boots *first*. ART's own boot
/// directory is given the highest priority of anything mounted so it is
/// always the device AmigaDOS boots from — that is the entire mechanism
/// behind "one click starts the game" (Y2 in the design doc).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DirMount {
    pub host_path: String,
    /// The WinUAE device name, e.g. `DH1` — not an Amiga volume label.
    pub volume: String,
    /// The Amiga volume label the mounted device presents, e.g. `Game`.
    pub label: String,
    pub boot_priority: i8,
    pub read_only: bool,
}

/// Detect WinUAE on the host Windows environment.
///
/// Standard install locations are derived from the environment rather than
/// hard-coded drive letters, so the search still works on machines where
/// Windows does not live on `C:`.
pub fn detect_winuae(custom_path: Option<&str>) -> WinUaeInstallation {
    // 1. Check custom path if user specified one
    if let Some(cp) = custom_path {
        let p = Path::new(cp);
        if p.is_file() {
            return WinUaeInstallation {
                found: true,
                executable_path: Some(p.to_string_lossy().to_string()),
                version: Some("Configured".into()),
                is_64bit: cp.to_lowercase().contains("64"),
            };
        }
    }

    // 2. Check standard install locations, rooted at the real Program Files
    //    directories reported by the environment.
    let roots: Vec<PathBuf> = ["ProgramFiles", "ProgramFiles(x86)", "ProgramW6432"]
        .iter()
        .filter_map(std::env::var_os)
        .map(PathBuf::from)
        .collect();

    for root in &roots {
        for exe in ["winuae64.exe", "winuae.exe"] {
            let p = root.join("WinUAE").join(exe);
            if p.is_file() {
                let is_64bit = exe.contains("64");
                return WinUaeInstallation {
                    found: true,
                    executable_path: Some(p.to_string_lossy().to_string()),
                    version: Some(if is_64bit {
                        "WinUAE 64-bit".into()
                    } else {
                        "WinUAE 32-bit".into()
                    }),
                    is_64bit,
                };
            }
        }
    }

    WinUaeInstallation {
        found: false,
        executable_path: None,
        version: None,
        is_64bit: false,
    }
}

/// Reject a value that would break out of its `key=value` line, or shift the
/// comma-delimited fields around it.
///
/// `.uae` files are line-oriented, so a newline inside a media path would let
/// the rest of it be read as further configuration directives. `filesystem2=`
/// and `hardfile2=` are both comma-delimited WinUAE directives —
/// `hardfile2=<rw|ro>,<device>:<path>,<sectors>,<surfaces>,<reserved>,
/// <blocksize>,<bootpri>,<filesystem>,<controller>` and
/// `filesystem2=<rw|ro>,<device>:<volume label>:<host path>,<bootpri>` both
/// put unrelated fields after the path — so a comma inside a value (a Windows
/// folder named `Games, Amiga`, mounted as a directory volume) shifts every
/// field after it, including the boot priority, rather than being refused
/// outright. ART-142.
fn checked_config_value(label: &str, value: &str) -> CoreResult<String> {
    if value.contains('\n') || value.contains('\r') {
        return Err(CoreError::InvalidInput(format!(
            "{label} contains a line break, which would corrupt the WinUAE configuration"
        )));
    }
    if value.contains(',') {
        return Err(CoreError::InvalidInput(format!(
            "{label} contains a comma, which would shift the fields after it in the WinUAE configuration"
        )));
    }
    Ok(value.to_string())
}

/// Generate a complete `.uae` config text representation.
pub fn generate_uae_config(profile: &AmigaProfile, media: &LaunchMedia) -> CoreResult<String> {
    let mut lines = Vec::new();

    lines.push("# Amiga Retro Toolkit (ART) Generated WinUAE Configuration".to_string());
    lines.push(format!("# Profile: {}", profile.name));
    lines.push("use_gui=no".into());
    lines.push("show_leds=true".into());

    // CPU Configuration
    let cpu_type_str = match profile.cpu {
        CpuModel::M68000 => "68000",
        CpuModel::M68010 => "68010",
        CpuModel::M68020 | CpuModel::M68EC020 => "68020",
        CpuModel::M68030 => "68030",
        CpuModel::M68040 => "68040",
        CpuModel::M68060 => "68060",
    };
    lines.push(format!("cpu_type={cpu_type_str}"));
    lines.push(format!("cpu_model={cpu_type_str}"));
    lines.push(format!(
        "cpu_compatible={}",
        if profile.cpu == CpuModel::M68000 {
            "true"
        } else {
            "false"
        }
    ));
    lines.push("cpu_speed=real".into());

    // Chipset
    let chipset_str = match profile.chipset {
        ChipsetModel::Ocs => "ocs",
        ChipsetModel::Ecs => "ecs",
        ChipsetModel::Aga => "aga",
    };
    lines.push(format!("chipset={chipset_str}"));
    lines.push("chipset_compatible=generic".into());

    // Memory (Chip RAM in 512KB units)
    let chip_units = (profile.memory.chip_kb / 512).max(1);
    lines.push(format!("chipmem_size={chip_units}"));
    let bogo_units = profile.memory.slow_kb / 512;
    lines.push(format!("bogomem_size={bogo_units}"));
    lines.push(format!("fastmem_size={}", profile.memory.fast_mb));
    lines.push(format!("z3fastmem_size={}", profile.memory.z3_fast_mb));

    // Kickstart ROM
    if media.use_aros || (media.kickstart_path.is_none() && profile.custom_rom_path.is_none()) {
        lines.push("kickstart_rom_file=:AROS".into());
    } else if let Some(ref kp) = media.kickstart_path {
        let kp = checked_config_value("Kickstart ROM path", kp)?;
        lines.push(format!("kickstart_rom_file={kp}"));
    } else if let Some(ref cp) = profile.custom_rom_path {
        let cp = checked_config_value("profile ROM path", cp)?;
        lines.push(format!("kickstart_rom_file={cp}"));
    } else {
        lines.push("kickstart_rom_file=:AROS".into());
    }

    // Floppy Drives & Speeds. WinUAE supports at most four drives (DF0–DF3).
    lines.push(format!("floppyspeed={}", profile.floppy.speed_percent));
    for (i, fp) in media.floppy_paths.iter().take(4).enumerate() {
        let fp = checked_config_value("floppy image path", fp)?;
        lines.push(format!("floppy{i}={fp}"));
        lines.push(format!("floppy{i}type=0"));
    }

    // Hard Drives (HDFs).
    //
    // Format (WinUAE cfgfile.cpp):
    //   hardfile2=<rw|ro>,<device>:<path>,<sectors>,<surfaces>,<reserved>,
    //             <blocksize>,<bootpri>,<filesystem>,<controller>
    //
    // Each image needs its own device name — emitting several bare `hardfile=`
    // lines made every drive after the first unreachable.
    let access = if media.write_protect_hardfiles {
        "ro"
    } else {
        "rw"
    };
    for (i, hp) in media.hardfile_paths.iter().enumerate() {
        let hp = checked_config_value("hardfile path", hp)?;
        // First hardfile boots (priority 0); later ones mount without booting.
        let boot_priority = if i == 0 { 0 } else { -128 };
        lines.push(format!(
            "hardfile2={access},DH{i}:{hp},32,1,2,512,{boot_priority},,uae"
        ));
    }

    // Directory volumes (WinUAE cfgfile.cpp):
    //   filesystem2=<rw|ro>,<device>:<volume label>:<host path>,<bootpri>
    //
    // Each entry's own `read_only` decides access — unlike the hardfiles
    // above, a directory mount is typically the game's own writable drawer
    // (WHDLoad keeps save games there) sitting beside ART's own read-write
    // boot directory, so there is no single flag that applies to all of them.
    for dm in &media.directories {
        let host_path = checked_config_value("directory mount host path", &dm.host_path)?;
        let volume = checked_config_value("directory mount volume", &dm.volume)?;
        let label = checked_config_value("directory mount label", &dm.label)?;
        let dm_access = if dm.read_only { "ro" } else { "rw" };
        lines.push(format!(
            "filesystem2={dm_access},{volume}:{label}:{host_path},{}",
            dm.boot_priority
        ));
    }

    // Display & Graphics
    lines.push(format!("gfx_width_win={}", profile.display.width));
    lines.push(format!("gfx_height_win={}", profile.display.height));
    lines.push(format!(
        "gfx_fullscreen_amiga={}",
        profile.display.fullscreen
    ));
    lines.push("gfx_framerate=1".into());
    lines.push("gfx_autoresolution=1".into());

    // Sound
    lines.push("sound_output=exact".into());
    lines.push("sound_channels=stereo".into());
    lines.push("sound_stereo_separation=7".into());

    Ok(lines.join("\n"))
}

/// Launch WinUAE with a generated configuration.
pub fn launch_winuae(winuae_path: &Path, config_text: &str) -> CoreResult<u32> {
    if !winuae_path.is_file() {
        return Err(CoreError::InvalidInput(format!(
            "WinUAE executable not found at '{}'",
            winuae_path.display()
        )));
    }

    // A unique name per launch: a fixed filename means two sessions started
    // close together overwrite each other's configuration.
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let temp_config_path = std::env::temp_dir().join(format!("art_launch_{stamp}.uae"));
    std::fs::write(&temp_config_path, config_text)?;

    // Arguments are passed as a structured argv, never through a shell, so
    // paths containing spaces or shell metacharacters cannot be reinterpreted
    // as commands (spec §56).
    let child = std::process::Command::new(winuae_path)
        .arg("-f")
        .arg(&temp_config_path)
        .spawn()
        .map_err(CoreError::Io)?;

    Ok(child.id())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_a500_config_with_adf() {
        let profile = AmigaProfile::a500_ocs();
        let media = LaunchMedia {
            floppy_paths: vec![r"C:\Games\MonkeyIsland.adf".into()],
            kickstart_path: Some(r"C:\ROMs\kick13.rom".into()),
            ..Default::default()
        };

        let uae = generate_uae_config(&profile, &media).unwrap();
        assert!(uae.contains("cpu_type=68000"));
        assert!(uae.contains("chipset=ocs"));
        assert!(uae.contains("chipmem_size=1"));
        assert!(uae.contains("bogomem_size=1"));
        assert!(uae.contains(r"floppy0=C:\Games\MonkeyIsland.adf"));
        assert!(uae.contains(r"kickstart_rom_file=C:\ROMs\kick13.rom"));
    }

    #[test]
    fn generate_a1200_config_with_aros_fallback() {
        let profile = AmigaProfile::a1200_aga();
        let media = LaunchMedia {
            hardfile_paths: vec![r"C:\HDFs\WHDGames.hdf".into()],
            use_aros: true,
            ..Default::default()
        };

        let uae = generate_uae_config(&profile, &media).unwrap();
        assert!(uae.contains("cpu_type=68020"));
        assert!(uae.contains("chipset=aga"));
        assert!(uae.contains("chipmem_size=4")); // 2048 KB / 512 = 4
        assert!(uae.contains("fastmem_size=8"));
        assert!(uae.contains("kickstart_rom_file=:AROS"));
        assert!(uae.contains(r"hardfile2=rw,DH0:C:\HDFs\WHDGames.hdf,32,1,2,512,0,,uae"));
    }

    /// Several HDFs used to produce repeated bare `hardfile=` lines with no
    /// device names, so only the first image was reachable.
    #[test]
    fn each_hardfile_gets_its_own_device() {
        let profile = AmigaProfile::a1200_aga();
        let media = LaunchMedia {
            hardfile_paths: vec![
                r"C:\HDFs\System.hdf".into(),
                r"C:\HDFs\Games.hdf".into(),
                r"C:\HDFs\Data.hdf".into(),
            ],
            use_aros: true,
            ..Default::default()
        };

        let uae = generate_uae_config(&profile, &media).unwrap();
        assert!(uae.contains(r"hardfile2=rw,DH0:C:\HDFs\System.hdf,32,1,2,512,0,,uae"));
        assert!(uae.contains(r"hardfile2=rw,DH1:C:\HDFs\Games.hdf,32,1,2,512,-128,,uae"));
        assert!(uae.contains(r"hardfile2=rw,DH2:C:\HDFs\Data.hdf,32,1,2,512,-128,,uae"));
        assert_eq!(uae.matches("hardfile2=").count(), 3);
    }

    #[test]
    fn write_protection_mounts_hardfiles_read_only() {
        let profile = AmigaProfile::a1200_aga();
        let media = LaunchMedia {
            hardfile_paths: vec![r"C:\HDFs\Precious.hdf".into()],
            use_aros: true,
            write_protect_hardfiles: true,
            ..Default::default()
        };

        let uae = generate_uae_config(&profile, &media).unwrap();
        assert!(uae.contains(r"hardfile2=ro,DH0:C:\HDFs\Precious.hdf"));
    }

    #[test]
    fn only_four_floppy_drives_are_emitted() {
        let profile = AmigaProfile::a500_ocs();
        let media = LaunchMedia {
            floppy_paths: (0..6).map(|i| format!(r"C:\d{i}.adf")).collect(),
            use_aros: true,
            ..Default::default()
        };

        let uae = generate_uae_config(&profile, &media).unwrap();
        assert!(uae.contains(r"floppy3=C:\d3.adf"));
        assert!(!uae.contains("floppy4="));
        assert!(!uae.contains("floppy5="));
    }

    /// A path carrying a newline could otherwise inject arbitrary `.uae`
    /// directives into the generated configuration.
    #[test]
    fn line_breaks_in_paths_are_rejected() {
        let profile = AmigaProfile::a500_ocs();
        let media = LaunchMedia {
            floppy_paths: vec!["C:\\ok.adf\nkickstart_rom_file=C:\\evil.rom".into()],
            use_aros: true,
            ..Default::default()
        };

        let err = generate_uae_config(&profile, &media).unwrap_err();
        assert!(matches!(err, CoreError::InvalidInput(_)));
    }

    /// ART-142. `filesystem2=` and `hardfile2=` are comma-delimited, so a
    /// Windows folder the user actually named — `Games, Amiga` — would shift
    /// the boot priority and every other field after it rather than being
    /// refused. Fixed by rejecting a comma the same way a line break already
    /// is.
    #[test]
    fn a_comma_in_a_directory_mount_path_is_rejected() {
        let profile = AmigaProfile::a1200_aga();
        let media = LaunchMedia {
            directories: vec![DirMount {
                host_path: r"D:\Games, Amiga\Turrican".into(),
                volume: "DH1".into(),
                label: "Game".into(),
                boot_priority: 0,
                read_only: false,
            }],
            use_aros: true,
            ..Default::default()
        };

        let err = generate_uae_config(&profile, &media).unwrap_err();
        assert!(matches!(err, CoreError::InvalidInput(_)));
    }

    /// The same defect, pre-existing since before this wave — the review
    /// judged it far more likely to fire now that a Windows folder (not an
    /// ADF filename) can be mounted directly.
    #[test]
    fn a_comma_in_a_hardfile_path_is_rejected() {
        let profile = AmigaProfile::a1200_aga();
        let media = LaunchMedia {
            hardfile_paths: vec![r"D:\Games, Amiga\System.hdf".into()],
            use_aros: true,
            ..Default::default()
        };

        let err = generate_uae_config(&profile, &media).unwrap_err();
        assert!(matches!(err, CoreError::InvalidInput(_)));
    }

    /// Y1 and Y2 both need a host folder to appear as an Amiga volume, and the
    /// system image beside it must not be writable.
    #[test]
    fn a_directory_mount_and_a_write_protected_system_reach_the_configuration() {
        let profile = AmigaProfile::a1200_aga();
        let media = LaunchMedia {
            hardfile_paths: vec![r"E:\amiga\amikit\AmiKit.hdf".into()],
            write_protect_hardfiles: true,
            directories: vec![
                DirMount {
                    host_path: r"D:\games\Turrican".into(),
                    volume: "DH1".into(),
                    label: "Game".into(),
                    boot_priority: 0,
                    read_only: false,
                },
                DirMount {
                    host_path: r"C:\Users\x\AppData\Roaming\art\launch\boot".into(),
                    volume: "DH2".into(),
                    label: "ARTBoot".into(),
                    boot_priority: 10,
                    read_only: false,
                },
            ],
            ..Default::default()
        };

        let uae = generate_uae_config(&profile, &media).unwrap();

        assert!(
            uae.contains(r"filesystem2=rw,DH1:Game:D:\games\Turrican,0"),
            "{uae}"
        );
        assert!(
            uae.contains(
                r"filesystem2=rw,DH2:ARTBoot:C:\Users\x\AppData\Roaming\art\launch\boot,10"
            ),
            "the boot directory outranks everything, which is what makes Y2 one click"
        );
        assert!(
            uae.contains("hardfile2=ro,"),
            "the user's own system image is mounted read-only"
        );
    }
}
