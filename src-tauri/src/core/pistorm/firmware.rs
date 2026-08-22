//! `config.txt` — the Raspberry Pi firmware side of a PiStorm card.
//!
//! Three things live here and nothing else does: which kernel the firmware
//! loads, which Kickstart it hands to it, and what the display and clock are
//! set to. Everything about the *Amiga* is a `cmdline.txt` token and belongs in
//! `super::options`.
//!
//! **Edited in place, never regenerated** (spec §40, ART-005). A real
//! `config.txt` carries `gpu_mem`, `dtparam`, several `dtoverlay` lines, `[pi4]`
//! conditional sections and the user's own comments. ART rewrites the handful of
//! keys it owns and lets every other line through untouched.

use serde::{Deserialize, Serialize};

/// What the firmware is told to output.
///
/// The mode numbers are the Raspberry Pi firmware's own: group 1 is CEA
/// (television timings), group 2 is DMT (monitor timings). They are shown on
/// screen beside the choice, so that what ART writes is never something the
/// user has to take on trust.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DisplayMode {
    /// Nothing written. The firmware asks the monitor, which is right far more
    /// often than a forced mode is.
    Auto,
    /// `hdmi_group=2 hdmi_mode=82` — DMT 1920×1080 at 60 Hz. The combination
    /// MultibootOS documents for a display that will not negotiate.
    Dmt1080p60,
    /// `hdmi_group=1 hdmi_mode=31` — CEA 1920×1080 at 50 Hz, for a PAL
    /// television.
    Cea1080p50,
    /// `hdmi_group=1 hdmi_mode=4` — CEA 1280×720 at 60 Hz.
    Cea720p60,
    /// Whatever the user typed. Power User Mode only: these two numbers decide
    /// whether anything appears on the screen at all.
    Custom { group: u32, mode: u32 },
}

impl DisplayMode {
    /// The two values to write, or `None` for "leave the firmware to it".
    pub fn group_and_mode(self) -> Option<(u32, u32)> {
        match self {
            Self::Auto => None,
            Self::Dmt1080p60 => Some((2, 82)),
            Self::Cea1080p50 => Some((1, 31)),
            Self::Cea720p60 => Some((1, 4)),
            Self::Custom { group, mode } => Some((group, mode)),
        }
    }
}

/// Running the Pi faster than it is sold to run.
///
/// Never part of a profile and never a default — it is heat, it is the quality
/// of somebody's power supply, and on a Pi it sets the warranty bit. Offered
/// because the community does it and ART pretending otherwise helps nobody, but
/// only ever as something the user turned on themselves.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Overclock {
    /// `arm_freq`, in MHz.
    pub arm_freq_mhz: u32,
    /// `over_voltage`, in the firmware's own steps.
    pub over_voltage: i32,
    /// `force_turbo` — hold the clock up rather than letting it scale.
    pub force_turbo: bool,
}

/// The firmware settings ART manages.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct FirmwareConfig {
    /// The Kickstart image on the FAT32 partition — the `initramfs` line.
    pub kickstart_file: String,
    pub display: DisplayMode,
    /// `None` unless the user has deliberately turned it on.
    pub overclock: Option<Overclock>,
    /// The kernel the `kernel=` line points at.
    ///
    /// **Whatever is actually on the card**, never a constant (ART-103). The
    /// Emu68 archive ships `Emu68-pistorm.gz` and says so in its own
    /// `config.txt`; ART used to rewrite that line to `Emu68.img`, which is a
    /// file the card does not have. `emu68_payload` sets this to the kernel it
    /// placed, and the reading side leaves it at [`KERNEL_IMAGE`].
    pub kernel_file: String,
    /// `dtoverlay=disable-bt`.
    ///
    /// An option rather than something ART adds on its own. Earlier versions
    /// wrote it into every card unasked, which is a decision about somebody
    /// else's hardware — it frees the UART, and it also turns off their
    /// Bluetooth.
    pub disable_bluetooth: bool,
}

impl Default for FirmwareConfig {
    fn default() -> Self {
        Self {
            kickstart_file: "kick.rom".into(),
            kernel_file: KERNEL_IMAGE.into(),
            display: DisplayMode::Auto,
            overclock: None,
            disable_bluetooth: false,
        }
    }
}

/// The `key=value` lines ART owns.
///
/// `dtoverlay` is deliberately absent: a Pi config may carry several, they are
/// all meaningful, and rewriting the key would discard the rest.
const MANAGED_KEYS: &[&str] = &[
    "arm_64bit",
    "kernel",
    "hdmi_group",
    "hdmi_mode",
    "arm_freq",
    "over_voltage",
    "force_turbo",
    // Written unasked before ART-090; removed once, then left alone.
    "hdmi_cvt",
    "framebuffer_width",
    "framebuffer_height",
    "framebuffer_depth",
];

/// The name ART looks for when *reading* a card it did not build.
///
/// **Not what a release ships.** `Emu68-pistorm.zip` carries
/// `Emu68-pistorm.gz` and its own `config.txt` points straight at that; a real
/// CaffeineOS card keeps three kernels in a `KERNEL/` folder with no extension
/// at all. This is a name from older material, kept because it is still what
/// some cards use and it costs nothing to look for — but writing it into a
/// `config.txt` over the release's own line is what ART-103 was, and a card
/// that names a kernel it does not carry does not boot.
pub const KERNEL_IMAGE: &str = "Emu68.img";

/// The largest `Emu68.img` ART will read to look for a version.
///
/// The real one is a couple of megabytes. The ceiling is here because the file
/// is whatever is on somebody's card (§56) — without it, a card carrying a
/// several-gigabyte file under that name would be pulled into memory.
const MAX_KERNEL_BYTES: u64 = 32 * 1024 * 1024;

/// What ART can say about the kernel on a card.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KernelInfo {
    pub file_name: String,
    pub size_bytes: u64,
    /// The version the build stamped into the image, when it is there.
    ///
    /// `None` is an honest answer and the screen says so, rather than guessing
    /// from a file date or a release note.
    pub version: Option<String>,
}

/// The version Emu68 stamps into its own image.
///
/// Not hopeful parsing: Emu68's build assembles
/// `"$VER: ${PROJECT_NAME} ${PROJECT_VERSION} ${PROJECT_DATE} git:${GIT_HASH}"`
/// in `cmake/verstring.cmake` and compiles it in, so the string is there by
/// construction and in a known shape. `None` when it is not — an image built
/// some other way, or not an Emu68 image at all.
///
/// Returns the whole `$VER:` line rather than a parsed version, because that
/// line is what the author wrote and it carries the date and the git hash a
/// user would need to report a problem.
pub fn version_from_kernel(image: &[u8]) -> Option<String> {
    const MARKER: &[u8] = b"$VER: Emu68 ";

    let start = image
        .windows(MARKER.len())
        .position(|window| window == MARKER)?
        + MARKER.len();

    // Up to the first control byte or the end of a generous window — the
    // string is NUL-terminated in the binary, and bounded here so a corrupt
    // image cannot hand back a megabyte of noise.
    let end = (start + 128).min(image.len());
    let text: String = image[start..end]
        .iter()
        .take_while(|byte| byte.is_ascii_graphic() || **byte == b' ')
        .map(|byte| *byte as char)
        .collect();

    let trimmed = text.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_string())
}

/// Read the kernel on a card and say what it is.
pub fn read_kernel(path: &std::path::Path) -> Option<KernelInfo> {
    let size_bytes = std::fs::metadata(path).ok()?.len();
    let file_name = path.file_name()?.to_str()?.to_string();

    let version = (size_bytes <= MAX_KERNEL_BYTES)
        .then(|| std::fs::read(path).ok())
        .flatten()
        .and_then(|bytes| version_from_kernel(&bytes));

    Some(KernelInfo {
        file_name,
        size_bytes,
        version,
    })
}

fn managed_lines(config: &FirmwareConfig) -> Vec<(&'static str, String)> {
    let mut lines = vec![
        ("arm_64bit", "1".to_string()),
        ("kernel", config.kernel_file.clone()),
    ];

    if let Some((group, mode)) = config.display.group_and_mode() {
        lines.push(("hdmi_group", group.to_string()));
        lines.push(("hdmi_mode", mode.to_string()));
    }

    if let Some(overclock) = config.overclock {
        lines.push(("arm_freq", overclock.arm_freq_mhz.to_string()));
        lines.push(("over_voltage", overclock.over_voltage.to_string()));
        if overclock.force_turbo {
            lines.push(("force_turbo", "1".to_string()));
        }
    }

    lines
}

/// `initramfs kick.rom` — a directive, not an assignment.
///
/// It is written with a space, which is why it cannot go through the
/// `key=value` path above and why a merge that only understood `=` would append
/// a second one every time.
fn initramfs_line(config: &FirmwareConfig) -> String {
    format!("initramfs {}", config.kickstart_file)
}

const BLUETOOTH_OVERLAY: &str = "dtoverlay=disable-bt";

/// The line that opens ART's storage block, and the reason the block can be
/// rewritten rather than accumulated.
///
/// The block is **section headers plus `dtoverlay=` lines**, so removing only
/// the `dtoverlay=` lines leaves `[pi3] [pi02] [pi4] [all]` behind and the
/// next save appends a whole new block underneath — which is what the first
/// version of this did, and what
/// `the_storage_block_is_written_once_however_often_the_card_is_saved`
/// caught. The marker makes the extent of the block explicit: from this line
/// to the `[all]` that closes it.
const STORAGE_BLOCK_MARKER: &str =
    "# Storage settings for Emu68 1.1 and newer — Amiga Retro Toolkit";

/// The two overlays ART owns in `config.txt`, by name.
///
/// A `dtoverlay=sdhc,…` or `dtoverlay=emmc,…` line is **ART's**, dropped on
/// merge and re-emitted from the current options rather than passed through.
/// Without that the block would be appended again on every save and the file
/// would end up carrying two contradictory settings — the same accumulation
/// `initramfs` already had to be protected from.
///
/// Every other `dtoverlay=` is somebody else's and survives verbatim.
const MANAGED_OVERLAYS: [&str; 2] = ["dtoverlay=sdhc,", "dtoverlay=emmc,"];

fn is_managed_overlay(trimmed: &str) -> bool {
    MANAGED_OVERLAYS.iter().any(|o| trimmed.starts_with(o))
}

/// Merge ART's firmware settings into an existing `config.txt`.
/// Whether this `config.txt` selects its boot per board.
///
/// **ART-204.** A Raspberry Pi `config.txt` is a *conditional-section* format:
/// `[pi4]`, `[all]`, `[gpio24=0]`. Emu68 uses it to boot a different kernel
/// depending on which PiStorm is fitted — the board is detected **at boot**,
/// from a GPIO, not chosen when the card is written. A real release therefore
/// names `kernel=` once per stanza, and an `initramfs` that is a *firmware*
/// rather than a Kickstart.
///
/// When a file is shaped like that, **the release knows its own boot layout
/// and ART does not.** ART's single `kernel_file` is for the other case: a
/// file it is writing from nothing, or a flat one it wrote itself.
fn selects_boot_per_board(existing: &str) -> bool {
    existing
        .lines()
        .map(str::trim)
        .any(|line| line.starts_with("[gpio"))
}

/// The section a line belongs to — `""` before the first header.
fn section_header(line: &str) -> Option<&str> {
    let trimmed = line.trim();
    (trimmed.starts_with('[') && trimmed.ends_with(']')).then_some(trimmed)
}

/// The thin wrapper: no storage overlay block. Tests and any caller that has
/// no `Emu68Options` to hand; the product goes through
/// [`merge_config_txt_with_overlays`].
pub fn merge_config_txt(config: &FirmwareConfig, existing: Option<&str>) -> String {
    merge_config_txt_with_overlays(config, &[], existing)
}

/// [`merge_config_txt`], also writing `overlays` as one managed block at the
/// end of the file.
///
/// `overlays` is [`super::options::storage_overlay_lines`]'s output: section
/// headers and `dtoverlay=` lines together, already closed with `[all]`.
/// Appended at the **end** deliberately — a section header inserted into the
/// middle of a release's own `[gpio24=0]` stanzas would change which board
/// the lines after it apply to, which is ART-204 from the other direction.
pub fn merge_config_txt_with_overlays(
    config: &FirmwareConfig,
    overlays: &[String],
    existing: Option<&str>,
) -> String {
    let managed = managed_lines(config);

    let Some(existing) = existing.filter(|text| !text.trim().is_empty()) else {
        let mut lines = vec![
            "# Amiga Retro Toolkit — PiStorm / Emu68".to_string(),
            String::new(),
        ];
        lines.extend(managed.iter().map(|(key, value)| format!("{key}={value}")));
        lines.push(initramfs_line(config));
        if config.disable_bluetooth {
            lines.push(BLUETOOTH_OVERLAY.to_string());
        }
        if !overlays.is_empty() {
            lines.push(String::new());
            lines.extend(overlays.iter().cloned());
        }
        lines.push(String::new());
        return lines.join("\n");
    };

    // ART-204. A file that boots a different kernel per board keeps its own
    // `kernel=` and `initramfs` lines verbatim: they are the release's, one
    // per stanza, and ART has one of each to offer. Everything else is merged
    // as before — but **per section**, because that is what the format means.
    let per_board = selects_boot_per_board(existing);

    let mut out: Vec<String> = Vec::new();
    let mut written: Vec<(String, &str)> = Vec::new();
    let mut section = String::new();
    let mut wrote_initramfs = false;
    let mut names_the_kickstart = false;
    let mut has_bluetooth_overlay = false;
    // Inside ART's own storage block, which is dropped whole and re-emitted
    // from the current options — see `STORAGE_BLOCK_MARKER`.
    let mut in_storage_block = false;

    for line in existing.lines() {
        let trimmed = line.trim();

        if in_storage_block {
            // The block ends at the `[all]` it is written with. Consuming
            // that header too is deliberate: it belongs to the block, and
            // leaving it would make the next line inherit no filter when the
            // block is gone — which is the same answer, but arrived at by
            // accident rather than on purpose.
            if trimmed == "[all]" {
                in_storage_block = false;
                section = String::new();
            }
            continue;
        }
        if trimmed == STORAGE_BLOCK_MARKER {
            in_storage_block = true;
            continue;
        }

        if let Some(header) = section_header(line) {
            section = header.to_string();
            out.push(line.to_string());
            continue;
        }

        if trimmed.is_empty() || trimmed.starts_with('#') {
            out.push(line.to_string());
            continue;
        }

        if trimmed.starts_with("initramfs") {
            if per_board {
                // The stanza's own — a stealth firmware, or a ROM this
                // release names for this board. Replacing it takes away the
                // thing the stanza exists for.
                if trimmed.contains(&config.kickstart_file) {
                    names_the_kickstart = true;
                }
                out.push(line.to_string());
                continue;
            }
            // Flat file: rewritten where it stands rather than appended, so a
            // card that already names a Kickstart does not end up naming two.
            if !wrote_initramfs {
                wrote_initramfs = true;
                names_the_kickstart = true;
                out.push(initramfs_line(config));
            }
            continue;
        }

        if trimmed.starts_with("dtoverlay=") {
            // An ART overlay line **outside** a marked block — somebody
            // deleted the marker, or an older ART wrote one. Dropped, and
            // re-emitted below from the current options.
            //
            // Its section headers are not dropped with it, because without
            // the marker there is nothing that says where the block ended and
            // eating a `[pi4]` the user wrote themselves would be worse than
            // leaving an empty one. An empty board section is inert; this is
            // untidy rather than wrong, and it is said here rather than left
            // to be discovered.
            if is_managed_overlay(trimmed) {
                continue;
            }
            if trimmed == BLUETOOTH_OVERLAY {
                has_bluetooth_overlay = true;
                // The user turning the option off is an instruction to remove
                // the line ART put there; anything else they wrote stays.
                if !config.disable_bluetooth {
                    continue;
                }
            }
            out.push(line.to_string());
            continue;
        }

        if let Some((raw_key, _)) = trimmed.split_once('=') {
            let key = raw_key.trim();

            // The release's per-board choice, left exactly as it is.
            if per_board && key == "kernel" {
                out.push(line.to_string());
                continue;
            }

            if let Some((managed_key, value)) = managed.iter().find(|(k, _)| *k == key) {
                // Keyed on **(section, key)**: the same key in two sections is
                // two settings, not one written twice, and dropping the second
                // is what left three boards with no kernel at all (ART-204).
                let here = (section.clone(), *managed_key);
                if !written.contains(&here) {
                    written.push(here);
                    out.push(format!("{managed_key}={value}"));
                }
                continue;
            }
            if MANAGED_KEYS.contains(&key) {
                // Ours, and no longer wanted — an overclock switched off, or a
                // `framebuffer_*` line an older ART wrote unasked.
                continue;
            }
        }

        out.push(line.to_string());
    }

    // A managed key is "missing" only when **no** section carried it. A
    // per-board file keeps its own `kernel=` lines, so ART must not then
    // append one of its own underneath them.
    let missing: Vec<&(&str, String)> = managed
        .iter()
        .filter(|(key, _)| !written.iter().any(|(_, written_key)| written_key == key))
        .filter(|(key, _)| !(per_board && *key == "kernel"))
        .collect();

    let needs_bluetooth = config.disable_bluetooth && !has_bluetooth_overlay;
    // The Kickstart still has to be named somewhere, whatever shape the file
    // is: keeping a release's own lines must not mean refusing to add ART's.
    let needs_initramfs = if per_board {
        !names_the_kickstart
    } else {
        !wrote_initramfs
    };

    if !missing.is_empty() || needs_initramfs || needs_bluetooth {
        out.push(String::new());
        out.push("# Added by Amiga Retro Toolkit".to_string());
        for (key, value) in missing {
            out.push(format!("{key}={value}"));
        }
        if needs_initramfs {
            out.push(initramfs_line(config));
        }
        if needs_bluetooth {
            out.push(BLUETOOTH_OVERLAY.to_string());
        }
    }

    if !overlays.is_empty() {
        // The blank line separating the block belongs to the block. Without
        // trimming first, the one written last time survives the round trip
        // and a new one is added on top of it, so the file grows a blank line
        // per save forever — which is the same accumulation the marker exists
        // to stop, in its quietest form.
        while out.last().is_some_and(|line| line.trim().is_empty()) {
            out.pop();
        }
        out.push(String::new());
        out.push(STORAGE_BLOCK_MARKER.to_string());
        out.extend(overlays.iter().cloned());
    }

    out.push(String::new());
    out.join("\n")
}

/// Read the firmware settings back off a card.
pub fn parse_config_txt(existing: &str) -> FirmwareConfig {
    let mut config = FirmwareConfig::default();
    let mut group: Option<u32> = None;
    let mut mode: Option<u32> = None;
    let mut arm_freq: Option<u32> = None;
    let mut over_voltage: Option<i32> = None;
    let mut force_turbo = false;

    for line in existing.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        if let Some(rest) = trimmed.strip_prefix("initramfs") {
            let name = rest.split_whitespace().next().unwrap_or_default();
            if !name.is_empty() {
                config.kickstart_file = name.to_string();
            }
            continue;
        }

        if trimmed == BLUETOOTH_OVERLAY {
            config.disable_bluetooth = true;
            continue;
        }

        let Some((key, value)) = trimmed.split_once('=') else {
            continue;
        };
        match key.trim() {
            // Read back rather than assumed (ART-103). What a card boots is
            // whatever its own `config.txt` says, and a release that names its
            // kernel `Emu68-pistorm.gz` must not come back as `Emu68.img`.
            "kernel" if !value.trim().is_empty() => config.kernel_file = value.trim().to_string(),
            "hdmi_group" => group = value.trim().parse().ok(),
            "hdmi_mode" => mode = value.trim().parse().ok(),
            "arm_freq" => arm_freq = value.trim().parse().ok(),
            "over_voltage" => over_voltage = value.trim().parse().ok(),
            "force_turbo" => force_turbo = value.trim() == "1",
            _ => {}
        }
    }

    config.display = match (group, mode) {
        (Some(2), Some(82)) => DisplayMode::Dmt1080p60,
        (Some(1), Some(31)) => DisplayMode::Cea1080p50,
        (Some(1), Some(4)) => DisplayMode::Cea720p60,
        (Some(group), Some(mode)) => DisplayMode::Custom { group, mode },
        _ => DisplayMode::Auto,
    };

    // `over_voltage` alone is not an overclock, and neither is `force_turbo`:
    // without a frequency there is nothing to report as one.
    config.overclock = arm_freq.map(|arm_freq_mhz| Overclock {
        arm_freq_mhz,
        over_voltage: over_voltage.unwrap_or(0),
        force_turbo,
    });

    config
}

#[cfg(test)]
mod tests {

    // -----------------------------------------------------------------
    // ART-204: `config.txt` is a conditional-section format.
    // -----------------------------------------------------------------

    /// A `config.txt` shaped like the one a real Emu68 release ships: the
    /// board is detected at boot from a GPIO, so **the same key appears once
    /// per section** and each section names a different kernel.
    ///
    /// **The structure is the fixture.** Every `config.txt` this module was
    /// tested against before ART-204 was a flat file ART had written itself,
    /// which is a reader and a writer agreeing with each other — the shape
    /// that already cost ART-032..035 and ART-079. The comment prose is not
    /// copied; the sections, keys and order are, because those are what the
    /// merge has to survive.
    fn sectioned_config() -> &'static str {
        "\
# a real release's own file
[pi4]
arm_boost=1
[all]
arm_64bit=1
total_mem=2048
gpu_mem=32

#-Pistorm detection-#
gpio=0-27=ip
gpio=0-27=pu

## stealth: boots the stock Amiga when reset is held
[gpio4=0]
kernel=Emu68-pistorm32lite
initramfs ps32lite-stealth-firmware.gz

[all]
## PiStorm32lite
[gpio24=0]
kernel=Emu68-pistorm32lite

[all]
## PiStorm16
[gpio24=1]
kernel=Emu68-pistorm16

[all]
## PiStorm
[gpio17=0]
kernel=Emu68-pistorm
"
    }

    fn config_for(kernel: &str, rom: &str) -> FirmwareConfig {
        FirmwareConfig {
            kernel_file: kernel.to_string(),
            kickstart_file: rom.to_string(),
            ..Default::default()
        }
    }

    /// Lines, trimmed, that start with `what`.
    fn lines_starting(text: &str, what: &str) -> Vec<String> {
        text.lines()
            .map(str::trim)
            .filter(|line| line.starts_with(what))
            .map(str::to_string)
            .collect()
    }

    fn storage_block() -> Vec<String> {
        crate::core::pistorm::options::storage_overlay_lines(&Default::default())
    }

    /// Written twice, present once. The merge drops ART's own overlay lines
    /// and re-emits them, so a card saved repeatedly does not accumulate
    /// contradictory settings — which is what would have happened if
    /// `dtoverlay=` lines had gone on being passed through verbatim.
    #[test]
    fn the_storage_block_is_written_once_however_often_the_card_is_saved() {
        let config = FirmwareConfig::default();
        let once = merge_config_txt_with_overlays(&config, &storage_block(), Some("gpu_mem=64\n"));
        let twice = merge_config_txt_with_overlays(&config, &storage_block(), Some(&once));
        let thrice = merge_config_txt_with_overlays(&config, &storage_block(), Some(&twice));

        assert_eq!(twice, thrice, "the merge must settle");
        assert_eq!(
            twice.matches("dtoverlay=sdhc,").count(),
            2,
            "one per board section, no more: {twice}"
        );
        assert_eq!(twice.matches("dtoverlay=emmc,").count(), 1, "{twice}");
    }

    /// A setting the user changed reaches the file, rather than landing
    /// beside the old one.
    #[test]
    fn changing_the_storage_setting_replaces_the_block_it_finds() {
        use crate::core::pistorm::options::{storage_overlay_lines, Emu68Options, StorageExposure};

        let config = FirmwareConfig::default();
        let before = merge_config_txt_with_overlays(&config, &storage_block(), None);
        assert!(before.contains("unit0=ro"), "{before}");

        let writable = storage_overlay_lines(&Emu68Options {
            storage_unit0: StorageExposure::ReadWrite,
            ..Emu68Options::default()
        });
        let after = merge_config_txt_with_overlays(&config, &writable, Some(&before));

        assert!(
            !after.contains("unit0=ro"),
            "the old setting is gone: {after}"
        );
        assert_eq!(after.matches("unit0=rw").count(), 3, "{after}");
    }

    /// Somebody else's `dtoverlay=` is not ART's to touch — only `sdhc` and
    /// `emmc` are managed.
    #[test]
    fn an_overlay_art_does_not_own_survives_verbatim() {
        let existing = "dtoverlay=vc4-kms-v3d\ndtoverlay=unicam,boot\ngpu_mem=64\n";
        let merged = merge_config_txt_with_overlays(
            &FirmwareConfig::default(),
            &storage_block(),
            Some(existing),
        );
        assert!(merged.contains("dtoverlay=vc4-kms-v3d"), "{merged}");
        assert!(merged.contains("dtoverlay=unicam,boot"), "{merged}");
    }

    /// **ART-204 from the other direction.** The block carries section
    /// headers, so appending it in the middle of a release's own `[gpio…]`
    /// stanzas would change which board the lines after it apply to. It goes
    /// at the end, after everything the release wrote.
    #[test]
    fn the_block_goes_after_a_releases_own_stanzas_never_between_them() {
        let existing = sectioned_config();
        let merged = merge_config_txt_with_overlays(
            &FirmwareConfig::default(),
            &storage_block(),
            Some(existing),
        );

        let last_release_line = merged
            .find("kernel=Emu68-pistorm\n")
            .expect("the release's last stanza is still there");
        let block_at = merged.find("[pi3]").expect("the block is there");
        assert!(
            block_at > last_release_line,
            "the block must not split the release's own stanzas: {merged}"
        );
        // And every one of the release's own gpio stanzas still has its kernel.
        assert_eq!(merged.matches("kernel=").count(), 4, "{merged}");
    }

    /// Nothing is written when there is nothing to write — the thin wrapper's
    /// behaviour, unchanged, so every existing caller is unaffected.
    #[test]
    fn no_overlays_means_no_block_at_all() {
        let merged = merge_config_txt(&FirmwareConfig::default(), Some("gpu_mem=64\n"));
        assert!(!merged.contains("dtoverlay=sdhc"), "{merged}");
        assert!(!merged.contains("[pi3]"), "{merged}");
    }

    #[test]
    fn every_board_keeps_its_own_kernel() {
        // The defect: four `kernel=` lines in, one out, and the three board
        // stanzas left with none. A card written from a real release booted
        // nothing on three of four boards.
        let merged = merge_config_txt(
            &config_for("Emu68-pistorm.gz", "kick.rom"),
            Some(sectioned_config()),
        );
        let kernels = lines_starting(&merged, "kernel=");
        assert_eq!(
            kernels,
            vec![
                "kernel=Emu68-pistorm32lite",
                "kernel=Emu68-pistorm32lite",
                "kernel=Emu68-pistorm16",
                "kernel=Emu68-pistorm",
            ],
            "a file that names a kernel per board keeps every one of them"
        );
    }

    #[test]
    fn every_gpio_stanza_survives() {
        let merged = merge_config_txt(
            &config_for("Emu68-pistorm.gz", "kick.rom"),
            Some(sectioned_config()),
        );
        for stanza in [
            "[gpio4=0]",
            "[gpio24=0]",
            "[gpio24=1]",
            "[gpio17=0]",
            "[pi4]",
        ] {
            assert!(
                merged.lines().any(|line| line.trim() == stanza),
                "{stanza} must survive the merge:\n{merged}"
            );
        }
    }

    #[test]
    fn a_stanzas_own_initramfs_is_not_replaced_by_the_kickstart() {
        // The stealth stanza loads a *firmware*, not a ROM. Rewriting it to
        // the Kickstart takes away the thing that stanza exists for.
        let merged = merge_config_txt(
            &config_for("Emu68-pistorm.gz", "kick.rom"),
            Some(sectioned_config()),
        );
        assert!(
            merged.contains("initramfs ps32lite-stealth-firmware.gz"),
            "the stealth stanza keeps its own firmware:\n{merged}"
        );
    }

    #[test]
    fn a_managed_key_is_rewritten_once_in_each_section_that_has_it() {
        // `arm_64bit` appears in `[all]`. A file with it in two sections must
        // come back with it in both — flat de-duplication is the defect.
        let existing = "[all]\narm_64bit=0\n\n[pi4]\narm_64bit=0\n";
        let merged = merge_config_txt(&config_for("k", "rom"), Some(existing));
        assert_eq!(
            lines_starting(&merged, "arm_64bit=").len(),
            2,
            "one per section, not one per file:\n{merged}"
        );
    }

    #[test]
    fn a_flat_config_still_behaves_exactly_as_before() {
        // The common case, and the one every earlier test covered: a file ART
        // wrote itself, one section, one kernel. Section-awareness must not
        // change it.
        let existing = "arm_64bit=1\nkernel=old-kernel\ninitramfs old.rom\n";
        let merged = merge_config_txt(&config_for("Emu68-pistorm.gz", "kick.rom"), Some(existing));
        assert_eq!(
            lines_starting(&merged, "kernel="),
            vec!["kernel=Emu68-pistorm.gz"]
        );
        assert_eq!(
            lines_starting(&merged, "initramfs"),
            vec!["initramfs kick.rom"]
        );
    }

    #[test]
    fn a_sectioned_file_still_gains_the_kickstart_it_had_no_line_for() {
        // Keeping a file's own lines must not mean refusing to add ART's.
        let merged = merge_config_txt(
            &config_for("Emu68-pistorm.gz", "kick.rom"),
            Some(sectioned_config()),
        );
        assert!(
            merged
                .lines()
                .any(|line| line.trim() == "initramfs kick.rom"),
            "the Kickstart still has to be named somewhere:\n{merged}"
        );
    }

    use super::*;

    #[test]
    fn a_users_own_config_survives_the_merge() {
        // The whole reason this is a merge. `gpu_mem`, `dtparam`, a second
        // `dtoverlay`, a `[pi4]` section and the comments are all somebody
        // else's decisions about their own hardware.
        let existing = "\
# my pi
gpu_mem=64
dtparam=audio=on
dtoverlay=vc4-kms-v3d
arm_64bit=1

[pi4]
arm_boost=1
";
        let merged = merge_config_txt(&FirmwareConfig::default(), Some(existing));
        for line in [
            "# my pi",
            "gpu_mem=64",
            "dtparam=audio=on",
            "dtoverlay=vc4-kms-v3d",
            "[pi4]",
            "arm_boost=1",
        ] {
            assert!(merged.contains(line), "{line} lost from:\n{merged}");
        }
    }

    #[test]
    fn the_kickstart_line_is_rewritten_not_repeated() {
        // `initramfs` is space-separated, so a merge that only understood `=`
        // would append a second one on every save until the firmware had a
        // list of Kickstarts to choose from.
        let existing = "arm_64bit=1\ninitramfs kick13.rom\ngpu_mem=64\n";
        let config = FirmwareConfig {
            kickstart_file: "kick31.rom".into(),
            ..FirmwareConfig::default()
        };
        let merged = merge_config_txt(&config, Some(existing));
        assert_eq!(merged.matches("initramfs").count(), 1, "{merged}");
        assert!(merged.contains("initramfs kick31.rom"), "{merged}");
        assert!(!merged.contains("kick13.rom"), "{merged}");
    }

    #[test]
    fn saving_the_same_card_twice_changes_nothing_the_second_time() {
        // The property that makes a merge safe to run repeatedly, and the one
        // an append-only bug shows up in immediately.
        let config = FirmwareConfig {
            kickstart_file: "kick31.rom".into(),
            display: DisplayMode::Dmt1080p60,
            disable_bluetooth: true,
            ..FirmwareConfig::default()
        };
        let once = merge_config_txt(&config, Some("gpu_mem=64\n"));
        let twice = merge_config_txt(&config, Some(&once));
        assert_eq!(once, twice);
    }

    #[test]
    fn a_forced_display_writes_the_two_numbers_it_names() {
        let config = FirmwareConfig {
            display: DisplayMode::Dmt1080p60,
            ..FirmwareConfig::default()
        };
        let merged = merge_config_txt(&config, None);
        assert!(merged.contains("hdmi_group=2"), "{merged}");
        assert!(merged.contains("hdmi_mode=82"), "{merged}");
    }

    #[test]
    fn going_back_to_auto_removes_the_forcing() {
        let existing = "hdmi_group=2\nhdmi_mode=82\ngpu_mem=64\n";
        let merged = merge_config_txt(&FirmwareConfig::default(), Some(existing));
        assert!(!merged.contains("hdmi_group"), "{merged}");
        assert!(!merged.contains("hdmi_mode"), "{merged}");
        assert!(merged.contains("gpu_mem=64"), "{merged}");
    }

    #[test]
    fn no_card_is_overclocked_unless_somebody_asked() {
        let merged = merge_config_txt(&FirmwareConfig::default(), None);
        for key in ["arm_freq", "over_voltage", "force_turbo"] {
            assert!(!merged.contains(key), "{key} in:\n{merged}");
        }
    }

    #[test]
    fn turning_an_overclock_off_takes_it_back_off_the_card() {
        let existing = "arm_freq=1400\nover_voltage=4\nforce_turbo=1\ngpu_mem=64\n";
        let merged = merge_config_txt(&FirmwareConfig::default(), Some(existing));
        for key in ["arm_freq", "over_voltage", "force_turbo"] {
            assert!(!merged.contains(key), "{key} in:\n{merged}");
        }
        assert!(merged.contains("gpu_mem=64"), "{merged}");
    }

    #[test]
    fn bluetooth_is_only_disabled_when_the_user_says_so() {
        // Earlier versions wrote this into every card unasked, which is a
        // decision about somebody else's Bluetooth.
        let plain = merge_config_txt(&FirmwareConfig::default(), None);
        assert!(!plain.contains("disable-bt"), "{plain}");

        let asked = merge_config_txt(
            &FirmwareConfig {
                disable_bluetooth: true,
                ..FirmwareConfig::default()
            },
            None,
        );
        assert!(asked.contains(BLUETOOTH_OVERLAY), "{asked}");
    }

    #[test]
    fn turning_bluetooth_back_on_removes_only_that_overlay() {
        let existing = "dtoverlay=disable-bt\ndtoverlay=vc4-kms-v3d\n";
        let merged = merge_config_txt(&FirmwareConfig::default(), Some(existing));
        assert!(!merged.contains("disable-bt"), "{merged}");
        assert!(merged.contains("dtoverlay=vc4-kms-v3d"), "{merged}");
    }

    #[test]
    fn the_framebuffer_lines_an_older_art_wrote_are_cleaned_up() {
        // ART used to write `hdmi_cvt` and three `framebuffer_*` keys for a
        // "RTG resolution" that Emu68 does not take from `config.txt` at all.
        let existing = "\
hdmi_cvt=1920 1080 60 6 0 0 0
framebuffer_width=1920
framebuffer_height=1080
framebuffer_depth=32
gpu_mem=64
";
        let merged = merge_config_txt(&FirmwareConfig::default(), Some(existing));
        for key in [
            "hdmi_cvt",
            "framebuffer_width",
            "framebuffer_height",
            "framebuffer_depth",
        ] {
            assert!(!merged.contains(key), "{key} in:\n{merged}");
        }
        assert!(merged.contains("gpu_mem=64"), "{merged}");
    }

    #[test]
    fn the_firmware_settings_survive_a_round_trip() {
        let config = FirmwareConfig {
            kickstart_file: "kick31.rom".into(),
            kernel_file: "Emu68-pistorm.gz".into(),
            display: DisplayMode::Cea1080p50,
            overclock: Some(Overclock {
                arm_freq_mhz: 1400,
                over_voltage: 4,
                force_turbo: true,
            }),
            disable_bluetooth: true,
        };
        let text = merge_config_txt(&config, None);
        assert_eq!(parse_config_txt(&text), config);
    }

    #[test]
    fn a_display_nobody_has_a_name_for_still_reads_back() {
        let text = merge_config_txt(
            &FirmwareConfig {
                display: DisplayMode::Custom { group: 2, mode: 16 },
                ..FirmwareConfig::default()
            },
            None,
        );
        assert_eq!(
            parse_config_txt(&text).display,
            DisplayMode::Custom { group: 2, mode: 16 }
        );
    }

    #[test]
    fn the_kernel_states_its_own_version() {
        // The shape Emu68's own build assembles in `cmake/verstring.cmake`
        // (verified 2026-08-13): `$VER: Emu68 <version> <date> git:<hash>`.
        let mut image = vec![0x7fu8; 4096];
        image.extend_from_slice(b"$VER: Emu68 1.0.7 (18.05.2026) git:a1b2c3d\0");
        image.extend_from_slice(&[0u8; 512]);

        assert_eq!(
            version_from_kernel(&image).as_deref(),
            Some("1.0.7 (18.05.2026) git:a1b2c3d"),
            "the whole line, because the date and hash are what a bug report needs"
        );
    }

    #[test]
    fn an_image_with_no_version_says_nothing_rather_than_guessing() {
        // A card built some other way, or a file that is not an Emu68 image at
        // all. "Unknown" is an answer; a version inferred from a file date is
        // not.
        assert_eq!(version_from_kernel(&[0u8; 8192]), None);
        assert_eq!(version_from_kernel(b"$VER: SomethingElse 1.0"), None);
        assert_eq!(version_from_kernel(b"$VER: Emu68 "), None);
    }

    #[test]
    fn the_version_search_does_not_run_off_the_end_of_a_short_image() {
        let full = b"$VER: Emu68 1.0.7";
        for len in 0..=full.len() {
            let _ = version_from_kernel(&full[..len]);
        }
    }

    #[test]
    fn a_corrupt_image_cannot_hand_back_a_megabyte_of_noise() {
        let mut image = b"$VER: Emu68 ".to_vec();
        image.extend(std::iter::repeat_n(b'A', 1024 * 1024));
        let version = version_from_kernel(&image).expect("a version");
        assert!(version.len() <= 128, "{}", version.len());
    }

    #[test]
    fn over_voltage_on_its_own_is_not_reported_as_an_overclock() {
        // Without a frequency there is nothing to call one, and reporting it
        // would light the warning card on a card nobody overclocked.
        let config = parse_config_txt("over_voltage=2\nforce_turbo=1\n");
        assert_eq!(config.overclock, None);
    }
}
