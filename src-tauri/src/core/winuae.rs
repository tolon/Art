//! WinUAE emulator integration and configuration generator (Phase 2 & Phase 16).
//!
//! Detects local WinUAE executable installations, produces accurate `.uae`
//! configurations from Amiga hardware profiles, and launches emulation sessions.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

use super::hdf::HardfileShape;
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
    /// The on-disk shape of each entry in `hardfile_paths`, at the same
    /// index (ART-146). Deciding this needs to read the file — RDB blocks,
    /// filesystem signatures — which this module deliberately cannot do: it
    /// takes a `LaunchMedia` of strings and must not start touching the
    /// filesystem itself. The decision is made where the image is actually
    /// available (`commands/launch.rs`, `commands/winuae.rs`, via
    /// `core::hdf::detect_hardfile_shape`) and travels here as data.
    ///
    /// `#[serde(default)]` so a `LaunchMedia` stored by a build before this
    /// field existed still deserialises, and a short or empty vector here
    /// means every hardfile past its end falls back to `HardfileShape::Bare`
    /// — the forced geometry this module already emitted for every hardfile
    /// before this fix, so nothing already working regresses.
    #[serde(default)]
    pub hardfile_shapes: Vec<HardfileShape>,
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
    /// A CD image (`.iso`/`.cue`) placed in the emulated machine's CD drive.
    ///
    /// **ART-193.** A package's own installer may verify the medium it was
    /// shipped on: AmigaOS 3.9's BoingBag `Updater` checks for named files on
    /// a volume called `AmigaOS3.9:` — its own strings say so — and without a
    /// CD it opens its screen and never finishes. Giving it the disc the tree
    /// was built from is *meeting* that check, not bypassing one; nothing here
    /// decrypts anything.
    ///
    /// **The path is data, and reading it is not this module's job** — the
    /// same rule [`hardfile_shapes`](Self::hardfile_shapes) is written under.
    /// Whether the image is really the disc a package asked for is decided
    /// where the file can actually be opened (`commands/amigainstall.rs`, via
    /// [`crate::core::iso::IsoImage::volume_name`]) and travels here as a
    /// string.
    ///
    /// `#[serde(default)]` so a `LaunchMedia` stored before this field existed
    /// still deserialises, and `None` emits nothing at all: a launch that
    /// needs no disc gets the configuration it always got.
    #[serde(default)]
    pub cd_image_path: Option<String>,
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

    // The CD drive (ART-193).
    //
    // Three lines, and each was read out of WinUAE's own documentation rather
    // than recalled — this project has paid for guessing:
    //
    //   `cdimage0=<path>`      `Docs/winuaechangelog.txt`: *"cdimage0=<path to
    //                          .cue/.iso> in config file, command line
    //                          parameter -cdimage=<path to .cue/.iso> can also
    //                          be used"*, and the key is in the binary's own
    //                          option table as `cdimage%d`. The same file
    //                          warns that a path may be followed by `,delay`
    //                          — which is why `checked_config_value`'s comma
    //                          rule matters here as much as it does for
    //                          `hardfile2=`.
    //   `scsi=true`            uaescsi.device. The changelog: *"cdimage0
    //                          pointing to image file and uaescsi.device set
    //                          in configuration: mount image on
    //                          uaescsi.device:0"*. Confirmed as a real key in
    //                          a WinUAE-written configuration on this machine
    //                          (`E:\amiga\Caffeine\WinUaexec\Configurations\
    //                          Caffeine_Storm.uae`, line 64).
    //   `win32.map_cd_drives`  the GUI's *"CDFS automount CD/DVD drives"*.
    //                          With uaescsi.device enabled this mounts the CD
    //                          through WinUAE's own built-in CDFS — *"There is
    //                          no need to install Amiga-side CDFS anymore"* —
    //                          so the Amiga sees the disc under its own volume
    //                          label without the tree's `L:CDFileSystem` being
    //                          mounted at all. Same file, line 21.
    //
    // Emitted only when there is a disc: a launch with no CD gets exactly the
    // configuration it got before this field existed, host CD/DVD drives
    // included (which is to say, not mounted).
    if let Some(ref cd) = media.cd_image_path {
        let cd = checked_config_value("CD image path", cd)?;
        lines.push("scsi=true".into());
        lines.push("win32.map_cd_drives=true".into());
        lines.push(format!("cdimage0={cd}"));
    }

    // Hard Drives (HDFs).
    //
    // Format (WinUAE cfgfile.cpp):
    //   hardfile2=<rw|ro>,<device>:<path>,<sectors>,<surfaces>,<reserved>,
    //             <blocksize>,<bootpri>,<filesystem>,<controller>
    //
    // Each image needs its own device name — emitting several bare `hardfile=`
    // lines made every drive after the first unreachable.
    //
    // That forced geometry (32 sectors, 1 surface, 2 reserved, 512 blocksize)
    // is only correct for a *bare* filesystem image — what `<device>:` names
    // too, since AmigaDOS has nothing else to call it. `HardfileShape::Rdb`
    // and `::Unknown` both skip it (ART-146): the e-uae configuration syntax
    // WinUAE inherits (`docs/configuration.txt`) states that blocksize `0`
    // marks an RDB hard file and that "all other components ... will be
    // ignored apart from <path> and <access>" — its own example
    // (`hardfile2=rw,:/path,0,0,0,0,0,`) leaves `<device>` empty, which is
    // the right call here too: a forced `DH{i}:` would be meaningless on a
    // disk that carries its own device names in its own RDB (or, for
    // anything else including a VHD container, no meaning ART can supply at
    // all) — so the geometry fields are left at `0` and the device name is
    // left empty, and WinUAE reads the disk itself.
    //
    // ART-149: `sectors=32` here is deliberate, not an oversight, even though
    // WinUAE's `hardfile.cpp::getchs2` floors `filesize / blocksize /
    // (sectors * surfaces)` with integer division and so drops any partial
    // last cylinder — a file that is not a whole multiple of 32*512 = 16384
    // bytes loses up to 31 blocks off the end when *presented* this way. The
    // mechanism is real; changing `sectors` to `1` to avoid it was tried and
    // reverted, because the images this matters for were not truncated by
    // WinUAE — they were **built** at 32-sectors/1-surface geometry in the
    // first place, and the filesystem inside them is sized to the truncated
    // whole-cylinder block count, not to the file's raw byte length. An FFS
    // root block sits at half the volume's block count
    // (`core::volume::VolumeGeometry::root_block_for`), so the root block's
    // position tells you what block count the filesystem inside was built
    // for. Measured across six real WHDLoad hardfiles from this user's
    // collection (`E:\amiga\Amigatolon\WHDload\HDF_Games_WHDLoad_by_Enzo_[A]\`)
    // by scanning for the block whose first longword is `2` (T_SHORT) and
    // whose last is `1` (ST_ROOT):
    //
    //   file blocks | root block | 2 * root | blocks truncated to a multiple of 32
    //   ------------|-----------|----------|--------------------------------------
    //          1843 |       912 |     1824 | 1824
    //          1331 |       656 |     1312 | 1312
    //          3993 |      1984 |     3968 | 3968
    //          5734 |      2864 |     5728 | 5728
    //          5038 |      2512 |     5024 | 5024
    //
    // Six for six: `2 * root_block` always equals the file's block count
    // truncated down to a multiple of 32, never the raw block count. Passing
    // `sectors=1` (so WinUAE presents the full, untruncated block count)
    // makes AmigaDOS compute the root block at the *raw* half-count instead —
    // for a 2334-block image that is 1167, where the filesystem's own root
    // block is really at 1152 (half of 2304, the truncated count) — so the
    // volume fails to validate and AmigaDOS reports "not a DOS disk", which
    // is exactly what the user saw after the `sectors=1` change shipped.
    // `sectors=32` is not a truncation bug to fix; it is the geometry these
    // images were built for, and WinUAE must be told that geometry back.
    //
    // `reserved` stays at `2`. e-uae's own `docs/configuration.txt` documents
    // it as "the number of reserved blocks at the start of the partition
    // (typically 2)" and gives exactly this case — a bare, non-RDB partition
    // image — as its worked example: `hardfile2=rw,DH1:/home/.../myhardfile,
    // 32,1,2,512,1,`. Those two reserved blocks are the boot block (the
    // `DOS\x` signature and checksum) that sits at the very start of the
    // filesystem's own data — present whether or not an RDB precedes the
    // image — so `reserved=2` describes this bare image correctly.
    let access = if media.write_protect_hardfiles {
        "ro"
    } else {
        "rw"
    };
    for (i, hp) in media.hardfile_paths.iter().enumerate() {
        let hp = checked_config_value("hardfile path", hp)?;
        // First hardfile boots (priority 0); later ones mount without booting.
        let boot_priority = if i == 0 { 0 } else { -128 };
        let shape = media
            .hardfile_shapes
            .get(i)
            .copied()
            .unwrap_or(HardfileShape::Bare);
        match shape {
            HardfileShape::Bare => lines.push(format!(
                "hardfile2={access},DH{i}:{hp},32,1,2,512,{boot_priority},,uae"
            )),
            HardfileShape::Rdb | HardfileShape::Unknown => lines.push(format!(
                "hardfile2={access},:{hp},0,0,0,0,{boot_priority},,uae"
            )),
        }
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

/// A WinUAE process **ART started**, and therefore the only one ART may end.
///
/// It exists because an Amiga-side install has to be able to stop the emulator
/// when its deadline expires, and a bare pid is not enough to do that safely.
/// Terminating by pid alone means asking the operating system to kill a number,
/// and a number can be reused: between the moment ART reads a pid and the
/// moment it acts on it, the process can exit and the pid be handed to
/// something else. Holding the [`std::process::Child`] holds the OS handle
/// too, so [`terminate`](Self::terminate) can only ever reach the process this
/// struct was created from — never a stranger that inherited its number, and
/// never a WinUAE the owner started themselves.
///
/// Keeping the child also keeps this platform-independent: `Child::kill` and
/// `Child::try_wait` are `std`, so `core/` needs no Windows API to own a
/// process it started (the core-independence rule).
#[derive(Debug)]
pub struct WinUaeProcess {
    child: std::process::Child,
}

impl WinUaeProcess {
    /// The process id, for reporting. Never terminate by this — see the type's
    /// own documentation for why the handle is what does that.
    pub fn pid(&self) -> u32 {
        self.child.id()
    }

    /// Whether the process is still alive.
    ///
    /// A poll rather than a wait: an Amiga-side install must keep reading the
    /// result file while the emulator runs, so it can never afford to block on
    /// the process.
    pub fn is_running(&mut self) -> CoreResult<bool> {
        Ok(self.child.try_wait().map_err(CoreError::Io)?.is_none())
    }

    /// End the process, and reap it.
    ///
    /// Idempotent on purpose: a run terminates the emulator on every ending it
    /// has, including the ones where the emulator has already gone, so "it was
    /// not there" is a success rather than something to report.
    pub fn terminate(&mut self) -> CoreResult<()> {
        if !self.is_running()? {
            return Ok(());
        }
        self.child.kill().map_err(CoreError::Io)?;
        self.child.wait().map_err(CoreError::Io)?;
        Ok(())
    }

    /// Give up ownership: the process keeps running and ART can no longer end
    /// it. This is what a fire-and-forget launch is, stated as a deliberate
    /// act rather than left implicit in a dropped handle.
    pub fn release(self) -> u32 {
        self.child.id()
    }
}

/// Launch WinUAE with a generated configuration, keeping the process.
///
/// The sibling of [`launch_winuae`], which throws the handle away. Anything
/// that has to be able to *stop* the emulator later must use this one.
pub fn launch_winuae_process(winuae_path: &Path, config_text: &str) -> CoreResult<WinUaeProcess> {
    launch_winuae_inner(winuae_path, config_text).map(|child| WinUaeProcess { child })
}

/// Launch WinUAE with a generated configuration.
pub fn launch_winuae(winuae_path: &Path, config_text: &str) -> CoreResult<u32> {
    Ok(launch_winuae_process(winuae_path, config_text)?.release())
}

fn launch_winuae_inner(winuae_path: &Path, config_text: &str) -> CoreResult<std::process::Child> {
    if !winuae_path.is_file() {
        return Err(CoreError::InvalidInput(format!(
            "WinUAE executable not found at '{}'",
            winuae_path.display()
        )));
    }

    // A unique name per launch: a fixed filename means two sessions started
    // close together overwrite each other's configuration — and the stamp
    // alone does not give one. Two launches can share a nanosecond, which is
    // the same defect ART-164 and ART-173 were filed for in test fixtures;
    // this is its production instance, on the path an Amiga-side install run
    // uses repeatedly. The counter is what makes the name unique; the stamp
    // only makes it readable.
    static NEXT_LAUNCH: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let seq = NEXT_LAUNCH.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let temp_config_path = std::env::temp_dir().join(format!("art_launch_{stamp}_{seq}.uae"));
    std::fs::write(&temp_config_path, config_text)?;

    // Arguments are passed as a structured argv, never through a shell, so
    // paths containing spaces or shell metacharacters cannot be reinterpreted
    // as commands (spec §56).
    let child = std::process::Command::new(winuae_path)
        .arg("-f")
        .arg(&temp_config_path)
        .spawn()
        .map_err(CoreError::Io)?;

    Ok(child)
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

    /// ART-149: `1000 Miglia v1.2.hdf`
    /// (`E:\amiga\Amigatolon\WHDload\HDF_Games_WHDLoad_by_Enzo_[A]\`), a real
    /// self-booting WHDLoad hardfile, is 1,195,008 bytes = 2334 blocks of
    /// 512 — not a whole number of 32-sector/1-surface cylinders (72.9375 of
    /// them). A change that presented the *raw* 2334-block count instead
    /// (`sectors=1`) was tried and reverted: measured against six real
    /// hardfiles from this user's collection, `2 * root_block` always equals
    /// the file's block count truncated down to a multiple of 32, never the
    /// raw count — these images were **built** at 32-sectors/1-surface
    /// geometry, so the filesystem inside expects the truncated count, and
    /// presenting the untruncated one instead is what actually produced
    /// "not a DOS disk" on this exact file. `sectors=32` is correct and
    /// pinned here so it is not "fixed" again the same way.
    #[test]
    fn bare_geometry_truncates_to_the_built_cylinder_count() {
        let total_bytes: u64 = 1_195_008;
        assert_eq!(total_bytes, 2334 * 512, "the measured file size, in blocks");
        // Not a whole number of 32-sector/1-surface cylinders — this file's
        // block count (2334) truncates to 2304 (72 cylinders) under
        // sectors=32, which is the geometry these images were built at.
        assert_ne!(
            total_bytes % (32 * 512),
            0,
            "this file must not be a whole number of 32-sector cylinders, \
             or it would not distinguish the two geometries"
        );

        let profile = AmigaProfile::a1200_aga();
        let path =
            r"E:\amiga\Amigatolon\WHDload\HDF_Games_WHDLoad_by_Enzo_[A]\1000 Miglia v1.2.hdf";
        let media = LaunchMedia {
            hardfile_paths: vec![path.into()],
            hardfile_shapes: vec![HardfileShape::Bare],
            use_aros: true,
            ..Default::default()
        };

        let uae = generate_uae_config(&profile, &media).unwrap();
        assert!(
            uae.contains(&format!("hardfile2=rw,DH0:{path},32,1,2,512,0,,uae")),
            "{uae}"
        );
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

    /// ART-146, the RDB shape: `AmiKit.hdf`'s real `RDSK` (behind its VHD
    /// header, but the principle is the same for a bare RDB image too) must
    /// not be forced through bare-image geometry — WinUAE reads the RDB
    /// itself once blocksize is `0`, per e-uae's `docs/configuration.txt`.
    #[test]
    fn an_rdb_hardfile_gets_no_forced_geometry() {
        let profile = AmigaProfile::a1200_aga();
        let media = LaunchMedia {
            hardfile_paths: vec![r"E:\amiga\Amigatolon\hdf\RdbSystem.hdf".into()],
            hardfile_shapes: vec![HardfileShape::Rdb],
            use_aros: true,
            ..Default::default()
        };

        let uae = generate_uae_config(&profile, &media).unwrap();
        assert_eq!(
            uae.matches("hardfile2=").count(),
            1,
            "no forced-geometry line should also be emitted for the same image"
        );
        assert!(
            uae.contains(r"hardfile2=rw,:E:\amiga\Amigatolon\hdf\RdbSystem.hdf,0,0,0,0,0,,uae"),
            "{uae}"
        );
    }

    /// ART-146, the "anything else" shape: a VHD container (`conectix` at
    /// offset 0, `AmiKit.hdf`'s actual bytes) is not a signature ART
    /// recognises, so it gets the same treatment as an RDB — WinUAE detects
    /// VHD on its own, and forcing geometry over it produced "Not a DOS disk
    /// in unit 0" against the user's real image.
    #[test]
    fn an_unrecognised_hardfile_shape_gets_no_forced_geometry() {
        let profile = AmigaProfile::a1200_aga();
        let media = LaunchMedia {
            hardfile_paths: vec![r"E:\amiga\amikit\AmiKit.hdf".into()],
            hardfile_shapes: vec![HardfileShape::Unknown],
            write_protect_hardfiles: true,
            use_aros: true,
            ..Default::default()
        };

        let uae = generate_uae_config(&profile, &media).unwrap();
        assert!(
            uae.contains(r"hardfile2=ro,:E:\amiga\amikit\AmiKit.hdf,0,0,0,0,0,,uae"),
            "{uae}"
        );
    }

    /// No `hardfile_shapes` entry at all — a `LaunchMedia` built by code that
    /// predates this field, or one whose vector is simply shorter than
    /// `hardfile_paths` — must keep emitting the forced-geometry line the
    /// WinUAE screen has always relied on, not silently switch shape.
    #[test]
    fn a_missing_shape_entry_defaults_to_bare_geometry() {
        let profile = AmigaProfile::a1200_aga();
        let media = LaunchMedia {
            hardfile_paths: vec![r"C:\HDFs\WHDGames.hdf".into()],
            use_aros: true,
            ..Default::default()
        };
        assert!(media.hardfile_shapes.is_empty());

        let uae = generate_uae_config(&profile, &media).unwrap();
        assert!(uae.contains(r"hardfile2=rw,DH0:C:\HDFs\WHDGames.hdf,32,1,2,512,0,,uae"));
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

    /// ART-193. A package's own installer can verify the medium it shipped
    /// on, so a launch has to be able to put a disc in the machine — and the
    /// three lines it takes were read out of WinUAE's own documentation, not
    /// recalled. Measured end to end on 2026-08-21: with exactly these lines
    /// the emulated A1200 reported `CD0: 467M ... Read Only AmigaOS3.9` and
    /// listed `AmigaOS3.9` among its mounted volumes, which is the name the
    /// `Updater` looks for.
    #[test]
    fn a_cd_image_reaches_the_configuration_with_the_device_that_mounts_it() {
        let profile = AmigaProfile::a1200_aga();
        let media = LaunchMedia {
            kickstart_path: Some(r"C:\ROMs\kick31.rom".into()),
            cd_image_path: Some(r"E:\amiga\Amigatolon\os39\AmigaOS39.iso".into()),
            ..Default::default()
        };

        let uae = generate_uae_config(&profile, &media).unwrap();
        assert!(
            uae.contains(r"cdimage0=E:\amiga\Amigatolon\os39\AmigaOS39.iso"),
            "{uae}"
        );
        assert!(
            uae.contains("scsi=true"),
            "uaescsi.device is what the image is mounted on"
        );
        assert!(
            uae.contains("win32.map_cd_drives=true"),
            "and WinUAE's own CDFS is what makes it an Amiga volume, so the tree's own L:CDFileSystem never has to be mounted"
        );
    }

    /// A launch with no disc must be byte-for-byte the configuration ART
    /// wrote before this field existed — in particular it must not turn on
    /// uaescsi.device or start mounting the owner's physical CD/DVD drives
    /// into an emulated Amiga that never asked for one.
    #[test]
    fn no_cd_image_emits_no_cd_lines_at_all() {
        let profile = AmigaProfile::a1200_aga();
        let media = LaunchMedia {
            use_aros: true,
            ..Default::default()
        };
        assert!(media.cd_image_path.is_none());

        let uae = generate_uae_config(&profile, &media).unwrap();
        assert!(!uae.contains("cdimage0="), "{uae}");
        assert!(!uae.contains("scsi=true"), "{uae}");
        assert!(!uae.contains("map_cd_drives"), "{uae}");
    }

    /// WinUAE reads a comma after a CD path as the start of its own `delay`
    /// option (`Docs/winuaechangelog.txt`), so a disc image in a folder the
    /// user named `Amiga, ISOs` would be silently misread rather than
    /// refused — the same defect ART-142 fixed for directory mounts.
    #[test]
    fn a_comma_in_a_cd_image_path_is_rejected() {
        let profile = AmigaProfile::a1200_aga();
        let media = LaunchMedia {
            cd_image_path: Some(r"E:\Amiga, ISOs\AmigaOS39.iso".into()),
            use_aros: true,
            ..Default::default()
        };

        let err = generate_uae_config(&profile, &media).unwrap_err();
        assert!(matches!(err, CoreError::InvalidInput(_)));
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

#[cfg(test)]
mod real_boot_hook {
    //! Boot a real distribution tree under WinUAE — the bar this project
    //! sets for a recipe and the one thing its own tests cannot reach.
    //!
    //! A 3.2 tree was proved by booting AmigaOS to a clean Workbench with
    //! the owner's licensed ROM; §89 says a tree that has not booted is not
    //! a tree ART may call working. This hook builds the config ART itself
    //! would write — `generate_uae_config`, not a hand-typed `.uae` — mounts
    //! the tree as a `filesystem2=` directory volume, and launches. What
    //! happens on screen is the finding.
    //!
    //! Gated and `#[ignore]`d: it needs the user's own tree, their own
    //! licensed Kickstart, and an installed WinUAE, none of which exist in
    //! CI. Nothing here is written to the repository.

    use super::*;
    use crate::core::profile::AmigaProfile;
    use std::path::PathBuf;

    #[test]
    #[ignore = "needs a real tree, a licensed ROM and WinUAE; run explicitly"]
    fn boot_a_distribution_tree_when_asked() {
        let (Ok(tree), Ok(rom)) = (
            std::env::var("ART_BOOT_TREE"),
            std::env::var("ART_BOOT_ROM"),
        ) else {
            return;
        };

        let profile = AmigaProfile::a1200_aga();
        let media = LaunchMedia {
            floppy_paths: Vec::new(),
            hardfile_paths: Vec::new(),
            hardfile_shapes: Vec::new(),
            kickstart_path: Some(rom.clone()),
            use_aros: false,
            write_protect_hardfiles: false,
            directories: vec![DirMount {
                host_path: tree.clone(),
                volume: "DH0".into(),
                // The label AmigaOS itself expects for a system volume. A
                // startup-sequence assigns against `SYS:`, which WinUAE binds
                // to whatever device it booted from, but tools and icons
                // written by the install refer to the label.
                label: "Workbench".into(),
                boot_priority: 0,
                read_only: false,
            }],
            cd_image_path: None,
        };

        let config = generate_uae_config(&profile, &media).expect("ART must be able to write this");
        let out = PathBuf::from(&tree).join("..").join("art-boot-39.uae");
        std::fs::write(&out, &config).unwrap();
        println!("config written to {}", out.display());
        println!("--- config ---\n{config}\n--- end ---");

        if let Ok(winuae) = std::env::var("ART_WINUAE") {
            let pid = launch_winuae(&PathBuf::from(&winuae), &config).expect("WinUAE must start");
            println!("WinUAE started, pid {pid}");
        }
    }
}

#[cfg(test)]
mod real_version_hook {
    //! **Ask a booted tree what it is, and read the answer on the host.**
    //!
    //! The bar the Amiga-side install round sets for itself is that a
    //! BoingBag'd tree *boots and shows its update* — and the method that
    //! found this project's biggest defect applies: **ask the running system,
    //! do not infer.** This project once shipped an AmigaOS **3.5** tree under
    //! the name 3.9 because it booted cleanly and a copyright line was read as
    //! proof. A directory name, a file size and a copyright line are each
    //! consistent with several answers; `Version FULL` is not.
    //!
    //! It does not interrupt the tree's own `Startup-Sequence` to reach a
    //! shell — a healthy tree resists interruption by design, which is itself
    //! a thing this project learned by measuring. Instead it uses the same
    //! mechanism [`crate::core::amigainstall`] uses for a run: ART's own work
    //! volume, mounted at the highest boot priority, carrying one script ART
    //! wrote. **The tree is mounted as data and is never written to.**
    //!
    //! The script asks four questions and redirects the answers to ART's own
    //! volume, where the host reads them:
    //!
    //! - `Version FULL` — the running system's Kickstart and Workbench.
    //! - `Version version.library FULL` — the library `Version` reports the
    //!   Workbench number *from*, so the two can be compared rather than one
    //!   being taken on trust.
    //! - `Version workbench.library FULL`.
    //! - `Version resource.library FULL` — because a real `Updater` failed
    //!   against a real tree with `Cannot open "resource.library", version
    //!   44.` (2026-08-21), and the difference between *the library is not
    //!   there* and *the library is there and will not open* is the difference
    //!   between two entirely different fixes.
    //!
    //! Gated and `#[ignore]`d, like every hook of this shape: it needs the
    //! user's own tree, their own licensed Kickstart and an installed WinUAE,
    //! none of which exist in CI.
    //!
    //! ```text
    //!   ART_BOOT_TREE=E:\amiga\ProjeART\bb-run\p2 ^
    //!   ART_BOOT_ROM="E:\...\Kickstart v3.1 rev 40.68 (1993)(Commodore)(A1200).rom" ^
    //!   ART_WINUAE="C:\Program Files\WinUAE\winuae64.exe" ^
    //!   cargo test ask_a_tree_its_version_when_asked -- --ignored --nocapture
    //! ```

    use super::*;
    use crate::core::amigainstall::WORK_VOLUME;
    use crate::core::profile::AmigaProfile;
    use std::path::PathBuf;
    use std::time::{Duration, Instant};

    /// What the Amiga writes and the host reads.
    const ANSWER: &str = "art-version.txt";
    /// Written last, so the host never reads a half-written answer.
    const DONE: &str = "art-version-done.txt";

    /// The script, which is entirely ART's own text.
    ///
    /// `SetPatch` is here for the same reason it is in a run's script: a tree
    /// carrying `Devs/AmigaOS ROM Update` is a tree whose disk libraries
    /// expect it, and `SetPatch` **resets the machine** after loading it — so
    /// the guard below is on a marker written *after* that line, or the second
    /// pass would answer nothing.
    ///
    /// **The environment it builds deliberately mirrors
    /// [`crate::core::amigainstall::workvol::startup_sequence`]'s** — the same
    /// assigns, the same `LIBS: … Classes ADD`, the same `ENV:`, the same
    /// `SetPatch`. An instrument that set up a *different* machine from the one
    /// the product sets up would answer questions about a machine nobody runs,
    /// and on 2026-08-21 a diagnostic run from this hook did exactly that: it
    /// had no `ENV:` while the product script did, so what it measured was the
    /// requester ART-192 had already fixed, not the question being asked. Keep
    /// the two in step — `AddDataTypes` (ART-193) is here for that reason and
    /// no other, since a `Version` needs no datatypes at all.
    fn probe_script(volume: &str, extra: &str) -> String {
        format!(
            "; Written by ART to ask a tree what it is. It writes nothing to the tree.\n\
             {volume}:C/Assign C: {volume}:C\n\
             FailAt 2000000000\n\
             If EXISTS {WORK_VOLUME}:{DONE}\n\
             \x20 Echo \"ART: already asked.\"\n\
             Else\n\
             \x20 Assign SYS: {volume}:\n\
             \x20 Assign S: {volume}:S\n\
             \x20 Assign L: {volume}:L\n\
             \x20 Assign LIBS: {volume}:Libs\n\
             \x20 Assign LIBS: {volume}:Classes ADD\n\
             \x20 Assign DEVS: {volume}:Devs\n\
             \x20 Assign FONTS: {volume}:Fonts\n\
             \x20 MakeDir RAM:T RAM:Clipboards RAM:ENV RAM:ENV/Sys\n\
             \x20 Assign T: RAM:T\n\
             \x20 Assign CLIPS: RAM:Clipboards\n\
             \x20 Assign ENV: RAM:ENV\n\
             \x20 Copy ENVARC: RAM:ENV ALL QUIET NOREQ\n\
             \x20 If EXISTS {volume}:C/SetPatch\n\
             \x20   {volume}:C/SetPatch QUIET\n\
             \x20 EndIf\n\
             \x20 If EXISTS {volume}:C/AddDataTypes\n\
             \x20   {volume}:C/AddDataTypes REFRESH QUIET\n\
             \x20 EndIf\n\
             \x20 Version >{WORK_VOLUME}:{ANSWER} FULL\n\
             \x20 Version >>{WORK_VOLUME}:{ANSWER} version.library FULL\n\
             \x20 Version >>{WORK_VOLUME}:{ANSWER} workbench.library FULL\n\
             \x20 Version >>{WORK_VOLUME}:{ANSWER} resource.library FULL\n\
             \x20 Version >>{WORK_VOLUME}:{ANSWER} FILE {volume}:Libs/resource.library FULL\n\
             {extra}\
             \x20 Echo >{WORK_VOLUME}:{DONE} \"done\"\n\
             EndIf\n"
        )
    }

    /// Extra AmigaDOS lines for one investigation, from `ART_BOOT_PROBE`,
    /// `;`-separated and indented into the script's own arm.
    ///
    /// **The instrument stays general.** Today's question is why a real
    /// `Updater` could not open `resource.library`; tomorrow's will be a
    /// different one, and a hook that could only ask today's gets rewritten
    /// every time — which is how a diagnostic stops being re-runnable, and the
    /// next reader ends up re-trusting a report instead of repeating it.
    ///
    /// This is the **one** place in ART where a generated AmigaDOS line is not
    /// ART's own text, and it is `#[cfg(test)]`, `#[ignore]`d, and gated on an
    /// environment variable that exists only on the machine of the person
    /// typing it. Nothing the product ships can reach it. The product's rule —
    /// that a script ART generates is assembled only from strings ART authored,
    /// enforced by [`crate::core::security::refuse_shell_metacharacters`] — is
    /// unchanged, and is exactly why this can live nowhere but a hook.
    fn extra_probes() -> String {
        std::env::var("ART_BOOT_PROBE")
            .unwrap_or_default()
            .split(';')
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .map(|line| format!("  {line}\n"))
            .collect()
    }

    #[test]
    #[ignore = "opens WinUAE against the owner's own tree and ROM; run explicitly"]
    fn ask_a_tree_its_version_when_asked() {
        let (Ok(tree), Ok(rom), Ok(winuae)) = (
            std::env::var("ART_BOOT_TREE"),
            std::env::var("ART_BOOT_ROM"),
            std::env::var("ART_WINUAE"),
        ) else {
            return;
        };

        let work = crate::core::ScratchDir::new("art-version", "probe");
        std::fs::create_dir_all(work.join("S")).unwrap();
        let script = probe_script("DH0", &extra_probes());
        println!("--- the script ---\n{script}--- end ---");
        std::fs::write(work.join("S/Startup-Sequence"), &script).unwrap();

        let mut directories = vec![
            DirMount {
                host_path: tree.clone(),
                volume: "DH0".into(),
                label: "Workbench".into(),
                boot_priority: 0,
                read_only: false,
            },
            DirMount {
                host_path: work.path().to_string_lossy().to_string(),
                volume: "DH9".into(),
                label: WORK_VOLUME.into(),
                boot_priority: 10,
                read_only: false,
            },
        ];
        // An unpacked package, when the question being asked is about one —
        // `ART_BOOT_PKG`, mounted as `DH8:` and never the boot device. The
        // instrument needs it to be able to ask *why* a real installer
        // refused, which is a question about the installer and the tree
        // together and cannot be asked of either alone.
        if let Ok(package) = std::env::var("ART_BOOT_PKG") {
            directories.push(DirMount {
                host_path: package,
                volume: "DH8".into(),
                label: "ARTPkg".into(),
                boot_priority: -1,
                read_only: false,
            });
        }

        // A disc in the emulated CD drive, when the question being asked is
        // about one — `ART_BOOT_CD`. ART-193's whole diagnosis is that a
        // package's installer verifies a medium ART did not mount, and
        // "does the Amiga see it, and under what name" is a question about a
        // running machine that no host-side reading can answer.
        let media = LaunchMedia {
            kickstart_path: Some(rom),
            directories,
            cd_image_path: std::env::var("ART_BOOT_CD").ok(),
            ..LaunchMedia::default()
        };

        let config = generate_uae_config(&AmigaProfile::a1200_aga(), &media).unwrap();
        let mut process = launch_winuae_process(&PathBuf::from(&winuae), &config).unwrap();
        println!("WinUAE pid {}", process.pid());

        let started = Instant::now();
        let done = work.join(DONE);
        // Three minutes answers a `Version`; a probe that runs a real
        // installer needs longer, and re-editing the constant per question is
        // how an instrument stops being re-runnable. `ART_BOOT_DEADLINE` is
        // in seconds.
        let deadline = Duration::from_secs(
            std::env::var("ART_BOOT_DEADLINE")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(180),
        );
        while started.elapsed() < deadline && !done.is_file() {
            std::thread::sleep(Duration::from_millis(500));
            if !process.is_running().unwrap_or(false) {
                break;
            }
        }
        let waited = started.elapsed();
        let _ = process.terminate();

        // Read as **bytes**, not as a `String`. The Amiga writes Latin-1, and
        // a single high-bit byte anywhere in the answer — a `List` of a
        // drawer with an accented name, a `Status` line — made
        // `read_to_string` fail and threw away an entire measured run
        // (2026-08-21). An instrument that discards its own answer over an
        // encoding detail is worse than one that shows a replacement
        // character.
        match std::fs::read(work.join(ANSWER)) {
            Ok(bytes) => println!(
                "--- the tree's own answer, after {waited:.1?} ---\n{}",
                String::from_utf8_lossy(&bytes)
            ),
            Err(err) => println!("no answer after {waited:.1?}: {err}"),
        }
    }
}
