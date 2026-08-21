//! Amiga Machine Profile Studio (Phase 2 & Phase 17).
//!
//! Hardware machine profiles — CPU, chipset, memory layout, floppy drives,
//! display and which Kickstart the machine runs.
//!
//! The built-in set covers the **classic line end to end**: A1000, A500,
//! A500+, A600, A2000, A3000, A1200, A4000, CDTV and CD32. ART is not an A500
//! tool that tolerates other Amigas, and the presets are what make that true
//! rather than a claim — `AmigaProfile::all_presets` is what the WinUAE
//! launcher and the compatibility check read. Users add their own on top
//! (spec §33); a profile is data, not a code path.

use serde::{Deserialize, Serialize};

/// Fast RAM, in MB, in [`AmigaProfile::whdload_a1200`] — the shipped default
/// for every WHDLoad launch, and the number Settings' own control
/// (`launch.whdloadFastRamMb`) starts from before the user changes it.
///
/// Lives here rather than in `core::launch` because it is a property of the
/// profile, and `core::launch::DEFAULT_WHDLOAD_FAST_RAM_MB` is defined as
/// this constant so the two cannot drift: the value a user sees in Settings
/// and the value the profile carries are one number, not two that agree
/// today. See [`AmigaProfile::whdload_a1200`] for where 8 comes from and
/// what the sources do and do not say.
pub const WHDLOAD_PROFILE_FAST_RAM_MB: u32 = 8;

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

    /// The one machine every WHDLoad launch runs on — ART-152.
    ///
    /// **Why a named profile instead of the catalogue's own chipset.** A
    /// WHDLoad launch used to take its machine from what the catalogue said
    /// the *original game* needed: an OCS title got [`Self::a500_ocs`]
    /// (68000, OCS, 512 KB Chip, no Fast RAM) with Fast RAM bolted on for
    /// WHDLoad's headroom (ART-151). But the machine a WHDLoad launch needs
    /// is not the machine the game was written for — WHDLoad is a *loader*
    /// that runs under AmigaDOS on a modern Amiga and then hands the
    /// patched game control. Sizing that machine per title means as many
    /// launch configurations as there are catalogue entries, each of which
    /// has to be right on its own; one known-good machine is the owner's
    /// decision here, and it is the machine WHDLoad's own community
    /// guidance is written around.
    ///
    /// **What the sources actually say.** WHDLoad's requirements page
    /// (<https://www.whdload.de/docs/en/need.html>, read 2026-08-21) states
    /// only a **floor** — a 68000, "a minimum of 1.0 MiB RAM (sometimes
    /// more, it depends on the installed program)" and Kickstart 2.0 — and
    /// gives no recommended configuration and no chip-versus-fast guidance
    /// at all. So this profile is **not** WHDLoad's own recommendation and
    /// must not be described as one: it is community consensus (68020, 2 MB
    /// Chip, Fast RAM alongside, Kickstart 3.x — the stock A1200 plus a
    /// Fast RAM expansion) sitting well above a floor that the page does
    /// state. The floor itself is still enforced, separately and on the
    /// ROM, by `core::launch::WHDLOAD_MIN_KICKSTART_MAJOR`.
    ///
    /// **68EC020, and the config still says `68020`.** A real A1200's CPU is
    /// a 68EC020 — a 68020 with a 24-bit address bus — and
    /// `core::winuae::generate_uae_config` maps [`CpuModel::M68020`] and
    /// [`CpuModel::M68EC020`] to the same `cpu_model=68020`. Naming the real
    /// part rather than the general one costs nothing in the generated
    /// config and keeps the profile a description of an actual machine.
    ///
    /// **What is not established.** Whether routing an **OCS-only** title
    /// through this AGA/68020 machine runs it as well as the A500/OCS/68000
    /// machine ART used to pick has *not* been measured, and nothing in this
    /// codebase or in the sources above settles it — AGA is backwards
    /// compatible with OCS in hardware and WHDLoad slaves are written to be
    /// installed on exactly this machine, but that is an argument, not a
    /// measurement. The one title ART has measured (`1000 Miglia`, ART-151)
    /// reached its own logo on A500/OCS/68000 with 8 MB Fast; it has not been
    /// run on this profile. **The escape hatch is deliberate and not
    /// silent**: `LaunchArgs::machine_override` — TitleDetail's per-title
    /// machine picker — still outranks this choice, so a user who finds a
    /// title unhappy here can put it back on an A500 for that title alone,
    /// and the confirmation screen names the machine before anything starts.
    /// There is no hidden catalogue-derived fallback; if one is ever added it
    /// belongs here, in writing.
    ///
    /// Not part of [`Self::all_presets`] on purpose: that list is the
    /// Profile Studio's catalogue of **machines that existed**, and this is a
    /// launch configuration ART chose. Deliberately spelled out field by
    /// field rather than derived from [`Self::a1200_aga`], so editing the
    /// Profile Studio's A1200 cannot silently move what WHDLoad launches on.
    pub fn whdload_a1200() -> Self {
        Self {
            id: "whdload-a1200".into(),
            name: "WHDLoad A1200 (68020, 2MB Chip, 8MB Fast)".into(),
            description: "The machine every WHDLoad launch runs on: 68EC020 @ 14 MHz, AGA, 2MB \
                          Chip + 8MB Fast RAM, Kickstart 3.1"
                .into(),
            cpu: CpuModel::M68EC020,
            cpu_speed_mhz: 14.18,
            chipset: ChipsetModel::Aga,
            memory: MemoryConfig {
                chip_kb: 2048,
                slow_kb: 0,
                fast_mb: WHDLOAD_PROFILE_FAST_RAM_MB,
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
            // No pinned hash: which Kickstart 3.x a WHDLoad launch boots is
            // whichever suitable ROM the user actually owns, chosen by
            // `core::launch::plan_for` against
            // `WHDLOAD_MIN_KICKSTART_MAJOR`. Pinning one here would claim a
            // ROM ART does not require.
            preferred_rom_sha256: None,
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

    /// A1000 — the first Amiga.
    ///
    /// Its Kickstart is loaded from floppy into the Writable Control Store
    /// rather than living in ROM, which is why this profile pins no ROM hash:
    /// which Kickstart an A1000 runs is the user's disk, not the machine's.
    pub fn a1000() -> Self {
        Self {
            id: "a1000".into(),
            name: "Amiga 1000 (OCS 1.3)".into(),
            description:
                "The original Amiga: 68000 @ 7.09 MHz, OCS, 512KB Chip RAM, Kickstart loaded from \
                 floppy into the WCS"
                    .into(),
            cpu: CpuModel::M68000,
            cpu_speed_mhz: 7.09,
            chipset: ChipsetModel::Ocs,
            memory: MemoryConfig {
                chip_kb: 512,
                slow_kb: 0,
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
            preferred_rom_sha256: None,
            custom_rom_path: None,
            is_builtin: true,
        }
    }

    /// A2000 — the desktop OCS machine, with Zorro II slots.
    pub fn a2000() -> Self {
        Self {
            id: "a2000".into(),
            name: "Amiga 2000 (OCS 1.3)".into(),
            description: "Desktop Amiga with Zorro II expansion: 68000 @ 7.09 MHz, OCS, 1MB Chip \
                          RAM and a Fast RAM card"
                .into(),
            cpu: CpuModel::M68000,
            cpu_speed_mhz: 7.09,
            chipset: ChipsetModel::Ocs,
            memory: MemoryConfig {
                chip_kb: 1024,
                slow_kb: 0,
                fast_mb: 2,
                z3_fast_mb: 0,
            },
            floppy: FloppyConfig {
                drive_count: 2,
                speed_percent: 100,
            },
            display: DisplayConfig {
                width: 1280,
                height: 960,
                fullscreen: false,
                scanlines: false,
            },
            kickstart_version: "1.3".into(),
            preferred_rom_sha256: None,
            custom_rom_path: None,
            is_builtin: true,
        }
    }

    /// A3000 — 68030 and Zorro III, the ECS workstation.
    pub fn a3000() -> Self {
        Self {
            id: "a3000".into(),
            name: "Amiga 3000 (ECS 2.04)".into(),
            description:
                "ECS workstation: 68030 @ 25 MHz, 2MB Chip RAM, Zorro III Fast RAM, Kickstart 2.04"
                    .into(),
            cpu: CpuModel::M68030,
            cpu_speed_mhz: 25.0,
            chipset: ChipsetModel::Ecs,
            memory: MemoryConfig {
                chip_kb: 2048,
                slow_kb: 0,
                fast_mb: 0,
                z3_fast_mb: 16,
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
            preferred_rom_sha256: None,
            custom_rom_path: None,
            is_builtin: true,
        }
    }

    /// CDTV — the OCS machine with a CD drive and no floppy of its own.
    pub fn cdtv() -> Self {
        Self {
            id: "cdtv".into(),
            name: "Commodore CDTV (OCS 1.3)".into(),
            description: "CD-based OCS Amiga: 68000 @ 7.09 MHz, 1MB Chip RAM, Kickstart 1.3 plus \
                          the CDTV extended ROM"
                .into(),
            cpu: CpuModel::M68000,
            cpu_speed_mhz: 7.09,
            chipset: ChipsetModel::Ocs,
            memory: MemoryConfig {
                chip_kb: 1024,
                slow_kb: 0,
                fast_mb: 0,
                z3_fast_mb: 0,
            },
            // No internal floppy — the same shape the CD32 profile uses.
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
            kickstart_version: "1.3".into(),
            preferred_rom_sha256: None,
            custom_rom_path: None,
            is_builtin: true,
        }
    }

    /// Every built-in preset, roughly in the order the machines appeared.
    ///
    /// The classic line, end to end — ART is not an A500 tool that tolerates
    /// other Amigas. Where a machine's Kickstart is not a single known file
    /// (the A1000 loads it from disk; an A2000 or A3000 may run 1.3, 2.04 or
    /// 3.1), `preferred_rom_sha256` is `None` rather than a guess: a wrong
    /// hash would make ART reject the user's correct ROM (§10, §89).
    pub fn all_presets() -> Vec<Self> {
        vec![
            Self::a1000(),
            Self::a500_ocs(),
            Self::a500_plus(),
            Self::a600_ecs(),
            Self::a2000(),
            Self::a3000(),
            Self::a1200_aga(),
            Self::a4000_040(),
            Self::cdtv(),
            Self::cd32(),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The whole classic line, and no two presets claiming the same id — the
    /// id is what a saved profile and a WinUAE config are keyed on.
    #[test]
    fn the_presets_cover_the_classic_line_with_unique_ids() {
        let presets = AmigaProfile::all_presets();
        let ids: Vec<&str> = presets.iter().map(|p| p.id.as_str()).collect();

        for expected in [
            "a1000",
            "a500-ocs",
            "a500-plus",
            "a600-ecs",
            "a2000",
            "a3000",
            "a1200-aga",
            "a4000-040",
            "cdtv",
            "cd32",
        ] {
            assert!(ids.contains(&expected), "{expected} missing from {ids:?}");
        }

        let mut unique = ids.clone();
        unique.sort_unstable();
        unique.dedup();
        assert_eq!(unique.len(), ids.len(), "duplicate preset id in {ids:?}");
    }

    /// A preset either pins a Kickstart it is sure of or pins none at all. A
    /// wrong hash is worse than no hash: it rejects the ROM the user actually
    /// has.
    #[test]
    fn every_preset_is_internally_consistent() {
        for profile in AmigaProfile::all_presets() {
            assert!(profile.is_builtin, "{} is not marked builtin", profile.id);
            assert!(!profile.name.is_empty());
            assert!(!profile.kickstart_version.is_empty(), "{}", profile.id);
            assert!(profile.memory.chip_kb >= 512, "{}", profile.id);
            if let Some(hash) = &profile.preferred_rom_sha256 {
                assert_eq!(hash.len(), 64, "{} pins a non-SHA-256 value", profile.id);
                assert!(
                    hash.chars().all(|c| c.is_ascii_hexdigit()),
                    "{} pins a non-hex value",
                    profile.id
                );
            }
        }
    }

    /// The two CD machines have no floppy drive, and every other machine has
    /// at least one — a profile that says otherwise would generate a WinUAE
    /// config that cannot boot the media it was made for.
    #[test]
    fn only_the_cd_machines_have_no_floppy_drive() {
        for profile in AmigaProfile::all_presets() {
            let expected_none = profile.id == "cd32" || profile.id == "cdtv";
            assert_eq!(
                profile.floppy.drive_count == 0,
                expected_none,
                "{} has {} drives",
                profile.id,
                profile.floppy.drive_count
            );
        }
    }
}
