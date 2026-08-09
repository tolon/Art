//! Amiga Machine Profile Studio (Phase 2 & Phase 17).
//!
//! Defines hardware machine profiles (A500, A1200, A4000, CD32, Custom),
//! including CPU architecture, chipset, memory layout, and expansion parameters.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CpuModel {
    M68000,
    M68010,
    M68020,
    M68EC020,
    M68030,
    M68040,
    M68060,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ChipsetModel {
    Ocs,
    Ecs,
    Aga,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryConfig {
    /// Chip RAM in KB (512, 1024, 2048)
    pub chip_kb: u32,
    /// Slow / Bogo RAM in KB (0, 512, 1024, 1536)
    pub slow_kb: u32,
    /// Fast RAM (24-bit) in MB (0, 1, 2, 4, 8)
    pub fast_mb: u32,
    /// Zorro III Fast RAM (32-bit) in MB (0, 16, 32, 64, 128, 256, 512)
    pub z3_fast_mb: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FloppyConfig {
    /// Drive count enabled (1..4)
    pub drive_count: u8,
    /// Emulation speed (100 = 1x, 200 = 2x, 400 = 4x, 800 = 8x, 0 = Turbo)
    pub speed_percent: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DisplayConfig {
    pub width: u32,
    pub height: u32,
    pub fullscreen: bool,
    pub scanlines: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AmigaProfile {
    pub id: String,
    pub name: String,
    pub description: String,
    pub cpu: CpuModel,
    pub cpu_speed_mhz: f32,
    pub chipset: ChipsetModel,
    pub memory: MemoryConfig,
    pub floppy: FloppyConfig,
    pub display: DisplayConfig,
    pub kickstart_version: String,
    pub preferred_rom_sha256: Option<String>,
    pub custom_rom_path: Option<String>,
    pub is_builtin: bool,
}

impl AmigaProfile {
    /// A500 OCS (The Gaming Classic)
    pub fn a500_ocs() -> Self {
        Self {
            id: "a500-ocs".into(),
            name: "Amiga 500 (OCS 1.3)".into(),
            description:
                "Classic gaming setup: Motorola 68000 @ 7.09 MHz, OCS, 512KB Chip + 512KB Slow RAM"
                    .into(),
            cpu: CpuModel::M68000,
            cpu_speed_mhz: 7.09,
            chipset: ChipsetModel::Ocs,
            memory: MemoryConfig {
                chip_kb: 512,
                slow_kb: 512,
                fast_mb: 0,
                z3_fast_mb: 0,
            },
            floppy: FloppyConfig {
                drive_count: 1,
                speed_percent: 100,
            },
            display: DisplayConfig {
                width: 1280,
                height: 960,
                fullscreen: false,
                scanlines: false,
            },
            kickstart_version: "1.3".into(),
            preferred_rom_sha256: Some(
                "895e3110292723c34898687265ea87f58c7386008ab5e9d99d3e8e2eb0cc04ef".into(),
            ),
            custom_rom_path: None,
            is_builtin: true,
        }
    }

    /// A500+ ECS (Enhanced Chip Set)
    pub fn a500_plus() -> Self {
        Self {
            id: "a500-plus".into(),
            name: "Amiga 500+ (ECS 2.04)".into(),
            description: "ECS chipset, 1MB Chip RAM + 1MB Fast RAM, Kickstart 2.04".into(),
            cpu: CpuModel::M68000,
            cpu_speed_mhz: 7.09,
            chipset: ChipsetModel::Ecs,
            memory: MemoryConfig {
                chip_kb: 1024,
                slow_kb: 0,
                fast_mb: 1,
                z3_fast_mb: 0,
            },
            floppy: FloppyConfig {
                drive_count: 1,
                speed_percent: 100,
            },
            display: DisplayConfig {
                width: 1280,
                height: 960,
                fullscreen: false,
                scanlines: false,
            },
            kickstart_version: "2.04".into(),
            preferred_rom_sha256: Some(
                "0c476717596ff1e604f3fb0cfb9024fccae978bb15c61307b369ec2646d6d7e0".into(),
            ),
            custom_rom_path: None,
            is_builtin: true,
        }
    }

    /// A600 ECS (Compact Amiga)
    pub fn a600_ecs() -> Self {
        Self {
            id: "a600-ecs".into(),
            name: "Amiga 600 (ECS 2.05)".into(),
            description: "Compact ECS Amiga: 2MB Chip + 4MB Fast RAM, Kickstart 2.05".into(),
            cpu: CpuModel::M68000,
            cpu_speed_mhz: 7.09,
            chipset: ChipsetModel::Ecs,
            memory: MemoryConfig {
                chip_kb: 2048,
                slow_kb: 0,
                fast_mb: 4,
                z3_fast_mb: 0,
            },
            floppy: FloppyConfig {
                drive_count: 1,
                speed_percent: 200,
            },
            display: DisplayConfig {
                width: 1280,
                height: 960,
                fullscreen: false,
                scanlines: false,
            },
            kickstart_version: "2.05".into(),
            preferred_rom_sha256: Some(
                "17b8f9e6d8a39d8e7887e597f8c142c38865e94b281f9b01cdfc2d1bf2758117".into(),
            ),
            custom_rom_path: None,
            is_builtin: true,
        }
    }

    /// A1200 AGA (WHDLoad Workhorse)
    pub fn a1200_aga() -> Self {
        Self {
            id: "a1200-aga".into(),
            name: "Amiga 1200 (AGA 3.1 — WHDLoad)".into(),
            description: "The ideal WHDLoad setup: 68EC020 @ 14 MHz, AGA chipset, 2MB Chip + 8MB Fast RAM, Kickstart 3.1".into(),
            cpu: CpuModel::M68EC020,
            cpu_speed_mhz: 14.18,
            chipset: ChipsetModel::Aga,
            memory: MemoryConfig {
                chip_kb: 2048,
                slow_kb: 0,
                fast_mb: 8,
                z3_fast_mb: 0,
            },
            floppy: FloppyConfig {
                drive_count: 1,
                speed_percent: 400,
            },
            display: DisplayConfig {
                width: 1280,
                height: 960,
                fullscreen: false,
                scanlines: false,
            },
            kickstart_version: "3.1".into(),
            preferred_rom_sha256: Some("e40a5dfb3d017ba335127d85ea15c34cb27a2444230e963b7b6a1e378774d9b4".into()),
            custom_rom_path: None,
            is_builtin: true,
        }
    }

    /// A4000 040 Powerhouse
    pub fn a4000_040() -> Self {
        Self {
            id: "a4000-040".into(),
            name: "Amiga 4000 (040 AGA Powerhouse)".into(),
            description: "High performance workstation: 68040 @ 25 MHz, AGA chipset, 2MB Chip + 64MB Z3 Fast RAM".into(),
            cpu: CpuModel::M68040,
            cpu_speed_mhz: 25.0,
            chipset: ChipsetModel::Aga,
            memory: MemoryConfig {
                chip_kb: 2048,
                slow_kb: 0,
                fast_mb: 0,
                z3_fast_mb: 64,
            },
            floppy: FloppyConfig {
                drive_count: 1,
                speed_percent: 800,
            },
            display: DisplayConfig {
                width: 1280,
                height: 960,
                fullscreen: false,
                scanlines: false,
            },
            kickstart_version: "3.1".into(),
            preferred_rom_sha256: Some("931215b22596ab03b573d842b036ca6d50ff01b6e42b2da116ea28b52fb1c4ea".into()),
            custom_rom_path: None,
            is_builtin: true,
        }
    }

    /// CD32 Console
    pub fn cd32() -> Self {
        Self {
            id: "cd32".into(),
            name: "Amiga CD32 (Console)".into(),
            description: "Amiga CD32 console with Akiko chip, 68EC020, AGA, 2MB Chip RAM".into(),
            cpu: CpuModel::M68EC020,
            cpu_speed_mhz: 14.18,
            chipset: ChipsetModel::Aga,
            memory: MemoryConfig {
                chip_kb: 2048,
                slow_kb: 0,
                fast_mb: 0,
                z3_fast_mb: 0,
            },
            floppy: FloppyConfig {
                drive_count: 0,
                speed_percent: 100,
            },
            display: DisplayConfig {
                width: 1280,
                height: 960,
                fullscreen: false,
                scanlines: false,
            },
            kickstart_version: "3.1".into(),
            preferred_rom_sha256: Some(
                "5f8924d013d879e6cf23a73c1d9dfd70a48a4c843813fffa8403d15b2909180f".into(),
            ),
            custom_rom_path: None,
            is_builtin: true,
        }
    }

    /// Return list of all default presets.
    pub fn all_presets() -> Vec<Self> {
        vec![
            Self::a500_ocs(),
            Self::a1200_aga(),
            Self::a500_plus(),
            Self::a600_ecs(),
            Self::a4000_040(),
            Self::cd32(),
        ]
    }
}
