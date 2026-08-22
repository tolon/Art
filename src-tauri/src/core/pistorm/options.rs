//! The Emu68 `cmdline.txt` options — the real ones, and only the real ones.
//!
//! **Every field here is a documented token.** That is the rule the module
//! exists to hold: ART's PiStorm screen once offered a JIT switch (Emu68 *is* a
//! JIT and cannot be turned off), an MMU switch (Emu68 emulates no MMU at all)
//! and a Fast RAM slider (Emu68 maps RAM automatically), and wrote them out as
//! `emu68.jit`, `emu68.mmu` and `buptest.fastram_size` — three tokens Emu68 has
//! never read. That is ART-090, and spec §10/§89 in the one place a user is
//! most likely to trust the program.
//!
//! Anything that is genuinely worth telling a user but is not a token belongs
//! in prose, on screen or in the docs — never as a control that appears to do
//! something.
//!
//! **The line is merged, never regenerated** (spec §39, ART-004). `cmdline.txt`
//! is one line and it carries the Raspberry Pi's own boot parameters —
//! `console=`, `root=`, `rootfstype=`. Rewriting it drops `root=` and the Pi
//! stops booting.

use serde::{Deserialize, Serialize};

use super::hardware::PistormHardware;

/// How the card is exposed to the Amiga — `sd.unit0` / `emmc.unit0`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum StorageExposure {
    /// `off` — the Amiga does not see the card at all.
    Off,
    /// `ro` — visible, read-only. The safe answer, and the default: a mistake
    /// on the Amiga side cannot then damage the card ART just built.
    ReadOnly,
    /// `rw` — writable. What a multiboot selector needs.
    ReadWrite,
}

impl StorageExposure {
    fn token_value(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::ReadOnly => "ro",
            Self::ReadWrite => "rw",
        }
    }

    fn from_token_value(value: &str) -> Option<Self> {
        match value {
            "off" => Some(Self::Off),
            "ro" => Some(Self::ReadOnly),
            "rw" => Some(Self::ReadWrite),
            _ => None,
        }
    }
}

/// Every Emu68 `cmdline.txt` option ART manages.
///
/// One field per documented token, grouped as the documentation groups them.
/// A `bool` is a bare flag — the token is present or it is not; an `Option` is
/// a token with a value, absent when ART should not write it at all rather than
/// write a zero.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Emu68Options {
    // -- Storage (`sd.*` on the Pi 3 family, `emmc.*` on Pi 4 / CM4) --------
    pub storage_unit0: StorageExposure,
    /// `sd.verbose` / `emmc.verbose` — 0, 1 or 2.
    pub storage_verbose: Option<u8>,
    /// `sd.low_speed` — leave the 50 MHz mode off, for a card that will not
    /// take it.
    pub storage_low_speed: bool,
    /// `sd.clock` — clock override, in MHz.
    pub storage_clock_mhz: Option<u32>,

    // -- CPU ---------------------------------------------------------------
    /// `vbr_move` — the vector table into fast RAM.
    pub vbr_move: bool,
    /// `nofpu` — no FPU. Rare, and for compatibility only.
    pub nofpu: bool,

    // -- JIT ---------------------------------------------------------------
    /// `enable_cache` — the JIT cache active from startup.
    ///
    /// Note what this is *not*: a switch for the JIT. Emu68 is exclusively a
    /// JIT engine and there is no way to turn that off short of powering down.
    pub enable_cache: bool,

    // -- Memory ------------------------------------------------------------
    /// `limit_2g` — cap mapped RAM at 2 GB.
    pub limit_2g: bool,
    /// `z2_ram_size` — Zorro II RAM in MB: 0, 1, 2, 4 or 8.
    pub z2_ram_size: Option<u8>,
    /// `enable_c0_slow` — A500 family only (see `AmigaTarget::has_slow_ram`).
    pub enable_c0_slow: bool,
    /// `enable_c8_slow` — A500 family only.
    pub enable_c8_slow: bool,
    /// `enable_d0_slow` — A500 family only.
    pub enable_d0_slow: bool,
    /// `move_slow_to_chip` — the trapdoor 512K as CHIP RAM. A500 family only.
    pub move_slow_to_chip: bool,

    // -- ROM ---------------------------------------------------------------
    /// `copy_rom` — ROM into fast RAM, in KB: 256, 512, 1024 or 2048.
    pub copy_rom_kb: Option<u32>,
    /// `checksum_rom` — recalculate the ROM checksum.
    pub checksum_rom: bool,

    // -- RTG ---------------------------------------------------------------
    /// `vc4.mem` — how much video memory to report to Picasso96, in MB.
    pub vc4_mem_mb: Option<u32>,

    // -- Timing ------------------------------------------------------------
    /// `chip_slowdown` — approximate the original bus timing.
    pub chip_slowdown: bool,
    /// `cs_dist` — how coarsely, 1 to 8. Only meaningful with the above.
    pub cs_dist: Option<u8>,

    // -- Floppy ------------------------------------------------------------
    /// `swap_df0_with_dfN` — 1, 2 or 3.
    pub swap_df0_with: Option<u8>,

    // -- PiStorm32-lite ----------------------------------------------------
    /// `one_slot` — force the single-slot protocol.
    pub one_slot: bool,

    // -- Diagnostics -------------------------------------------------------
    pub debug: bool,
    pub disassemble: bool,
    pub async_log: bool,
    pub fast_serial: bool,
    /// `buptest` — a memory test at startup, in KB.
    pub buptest_kb: Option<u32>,
    /// `bupiter` — how many passes of it.
    pub bupiter: Option<u32>,
}

impl Default for Emu68Options {
    fn default() -> Self {
        Self {
            // Read-only rather than off: the card is visible, which is what
            // nearly everyone wants, and a mistake on the Amiga side cannot
            // damage what ART just built. `rw` is a decision, not a default.
            storage_unit0: StorageExposure::ReadOnly,
            storage_verbose: None,
            storage_low_speed: false,
            storage_clock_mhz: None,
            vbr_move: false,
            nofpu: false,
            enable_cache: false,
            limit_2g: false,
            z2_ram_size: None,
            enable_c0_slow: false,
            enable_c8_slow: false,
            enable_d0_slow: false,
            move_slow_to_chip: false,
            copy_rom_kb: None,
            checksum_rom: false,
            vc4_mem_mb: None,
            chip_slowdown: false,
            cs_dist: None,
            swap_df0_with: None,
            one_slot: false,
            debug: false,
            disassemble: false,
            async_log: false,
            fast_serial: false,
            buptest_kb: None,
            bupiter: None,
        }
    }
}

/// The ready-made answers, each a set of real tokens.
///
/// The three cards were a good idea badly filled in: "99 % WHDLoad
/// compatibility", "~800+ MIPS", "20+ MB/s" — numbers nobody measured and
/// nobody can reproduce (ART-090). The idea is kept; every claim is now a
/// token the user can read on the line afterwards.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Emu68Profile {
    /// Fastest Workbench and RTG, where exact timing does not matter.
    Performance,
    /// Balanced defaults. WHDLoad runs in NOMMU mode — there is no MMU here to
    /// run any other way.
    Daily,
    /// Closer to the original bus timing: accuracy instead of speed.
    Compatibility,
    /// For hunting a problem, not for using the machine.
    Diagnostics,
}

impl Emu68Profile {
    pub const ALL: &'static [Self] = &[
        Self::Performance,
        Self::Daily,
        Self::Compatibility,
        Self::Diagnostics,
    ];
}

/// The options a profile stands for, on this hardware.
///
/// Hardware-dependent because some tokens are: the compatibility profile's
/// `move_slow_to_chip` is an A500-family concept, and asking an A1200 for it is
/// the documented cause of a wrong RAM report. A profile applied to an A1200
/// therefore *is* a different set of tokens, not the same set with a warning.
pub fn profile_options(profile: Emu68Profile, hardware: PistormHardware) -> Emu68Options {
    let base = Emu68Options::default();
    match profile {
        Emu68Profile::Performance => Emu68Options {
            vbr_move: true,
            copy_rom_kb: Some(1024),
            enable_cache: true,
            vc4_mem_mb: Some(64),
            ..base
        },
        Emu68Profile::Daily => Emu68Options {
            enable_cache: true,
            copy_rom_kb: Some(512),
            ..base
        },
        Emu68Profile::Compatibility => Emu68Options {
            chip_slowdown: true,
            cs_dist: Some(4),
            limit_2g: true,
            move_slow_to_chip: hardware.amiga.has_slow_ram(),
            ..base
        },
        Emu68Profile::Diagnostics => Emu68Options {
            storage_verbose: Some(2),
            debug: true,
            async_log: true,
            ..base
        },
    }
}

/// Options that cannot apply to this hardware, removed.
///
/// Applied to everything on its way out, so neither a profile nor a settings
/// file nor a hand-edited card can put an A500 token on an A1200.
pub fn gated_for(options: &Emu68Options, hardware: PistormHardware) -> Emu68Options {
    let mut gated = options.clone();
    if !hardware.amiga.has_slow_ram() {
        gated.enable_c0_slow = false;
        gated.enable_c8_slow = false;
        gated.enable_d0_slow = false;
        gated.move_slow_to_chip = false;
    }
    if !hardware.variant.has_one_slot_option() {
        gated.one_slot = false;
    }
    // `cs_dist` on its own says nothing: it is how coarse the slowdown is, and
    // there is no slowdown without `chip_slowdown`.
    if !gated.chip_slowdown {
        gated.cs_dist = None;
    }
    gated
}

/// The two storage-driver prefixes Emu68 documents, and ART writes both.
///
/// `brcm-sdhc.device` is Pi Zero2/3A+/3B/3B+; `brcm-emmc.device` is Pi4B+/CM4
/// — Emu68's own SD-preparation tutorial. Which one *binds* is Emu68's
/// decision, taken from the hardware at boot, so writing both costs nothing
/// and stops a card losing its setting when it changes board.
pub const STORAGE_PREFIXES: [&str; 2] = ["sd", "emmc"];

/// Every token name ART owns.
///
/// The merge below uses this to tell "a token whose feature the user has just
/// switched off" — which must be removed — from "a boot parameter that is none
/// of ART's business" — which must survive untouched. Both storage prefixes
/// are listed, and since 2026-08-22 both are also always *written* — see
/// [`STORAGE_PREFIXES`]. They stay listed here because a `verbose` or
/// `low_speed` the user switches off still has to be removed from a line that
/// already carries it, under either prefix.
pub const MANAGED_TOKENS: &[&str] = &[
    "sd.unit0",
    "sd.verbose",
    "sd.low_speed",
    "sd.clock",
    "emmc.unit0",
    "emmc.verbose",
    "emmc.low_speed",
    "emmc.clock",
    "vbr_move",
    "nofpu",
    "enable_cache",
    "limit_2g",
    "z2_ram_size",
    "enable_c0_slow",
    "enable_c8_slow",
    "enable_d0_slow",
    "move_slow_to_chip",
    "copy_rom",
    "checksum_rom",
    "vc4.mem",
    "chip_slowdown",
    "cs_dist",
    "swap_df0_with_df1",
    "swap_df0_with_df2",
    "swap_df0_with_df3",
    "one_slot",
    "debug",
    "disassemble",
    "async_log",
    "fast_serial",
    "buptest",
    "bupiter",
    // Written by ART before ART-090 and read by nothing: removed on the next
    // save so a card built by an older ART stops carrying them.
    "emu68.jit",
    "emu68.mmu",
    "buptest.fastram_size",
    "kickstart",
];

/// The same storage settings again, as `config.txt` overlay lines — the
/// mechanism a **newer** Emu68 reads (2026-08-22 research, see
/// `docs/superpowers/specs/2026-08-22-emu68-boot-config.md`).
///
/// # Why both files carry it
///
/// Emu68's own `documentation/overlays.md`: *"Starting with Emu68 1.1 the use
/// of cmdline.txt for adjusting Emu68 parameters is obsolete. Instead, device
/// tree overlays can be injected through config.txt."* The storage settings
/// moved to `sdhc.dtbo` / `emmc.dtbo`.
///
/// **The owner's own Emu68 has not moved yet, and that was measured rather
/// than assumed.** Their `Emu68-pistorm.gz` is `1.1.0-alpha.1 (09.02.2026)`
/// and its binary still contains every `sd.*` / `emmc.*` token ART writes,
/// while *not* containing master's `whole-drive-access` property. So
/// `cmdline.txt` is live for them and stays exactly as it is.
///
/// Writing both is what makes the card survive the day they update: an old
/// Emu68 has no `sdhc.dtbo` to load and the Pi bootloader **silently ignores
/// a missing overlay and keeps going**; a new one ignores the cmdline tokens.
/// Neither mechanism has to guess which is live.
///
/// # Why the lines are board-conditional and not simply listed
///
/// `start.c` decides which storage driver to enable from the hardware —
/// *"make brcm-emmc enabled on bcm2711 (Pi4, CM4, Pi400), disabled
/// otherwise"* — **but only when the node was not loaded from an overlay.**
/// An unconditional `dtoverlay=emmc` therefore enables `brcm-emmc` on a Pi3,
/// which has no eMMC. The sections come from the Raspberry Pi documentation's
/// own filter table: `[pi3]` is 3B/3B+/3A+/CM3/CM3+, `[pi02]` is the Zero 2 W,
/// and `[pi4]` **already covers CM4 and CM4S** — which is why there is no
/// `[cm4]` here. Closed with `[all]`, which *"resets all previously set
/// filters"*, so nothing written after the block inherits one.
pub fn storage_overlay_lines(options: &Emu68Options) -> Vec<String> {
    let mut params = vec![format!("unit0={}", options.storage_unit0.token_value())];
    if let Some(level) = options.storage_verbose {
        params.push(format!("verbose={level}"));
    }
    if options.storage_low_speed {
        params.push("low_speed".to_string());
    }
    if let Some(mhz) = options.storage_clock_mhz {
        params.push(format!("clock={mhz}"));
    }
    let params = params.join(",");

    vec![
        "[pi3]".to_string(),
        format!("dtoverlay=sdhc,{params}"),
        "[pi02]".to_string(),
        format!("dtoverlay=sdhc,{params}"),
        "[pi4]".to_string(),
        format!("dtoverlay=emmc,{params}"),
        "[all]".to_string(),
    ]
}

/// The tokens ART wants the line to contain, in documentation order.
pub fn tokens_for(options: &Emu68Options, hardware: PistormHardware) -> Vec<String> {
    let options = gated_for(options, hardware);
    let mut tokens = Vec::new();

    // **Both prefixes, always** — the storage settings are written for
    // `brcm-sdhc.device` *and* `brcm-emmc.device`, so one card serves a Pi3
    // and a Pi4 (2026-08-22 research, `docs/superpowers/specs/2026-08-22-emu68-boot-config.md`).
    //
    // ART used to write only the prefix its configured Pi model implies. That
    // was not wrong, but it made the card carry a setting that silently
    // stopped applying the moment somebody moved it to the other board — and
    // moving a card between boards is a thing people do with these.
    //
    // Safe because **Emu68 picks the driver itself, from the hardware**:
    // `start.c` enables `brcm-emmc` on bcm2711 and `brcm-sdhc` otherwise, so
    // the prefix that does not match sets properties on a device that is
    // disabled. The Emu68 Imager writes both for the same reason, which is
    // the practical evidence that both are tolerated.
    //
    // The `ro` default is Emu68's own, and its own SD-preparation tutorial
    // warns against writing to unit 0 — the *whole card*, partition table and
    // FAT32 boot partition included. ART does not adopt the Imager's `rw`:
    // that serves the Imager's own Install-folder mechanism, which ART has
    // no equivalent of.
    for prefix in STORAGE_PREFIXES {
        tokens.push(format!(
            "{prefix}.unit0={}",
            options.storage_unit0.token_value()
        ));
        if let Some(level) = options.storage_verbose {
            tokens.push(format!("{prefix}.verbose={level}"));
        }
        if options.storage_low_speed {
            tokens.push(format!("{prefix}.low_speed"));
        }
        if let Some(mhz) = options.storage_clock_mhz {
            tokens.push(format!("{prefix}.clock={mhz}"));
        }
    }

    if options.vbr_move {
        tokens.push("vbr_move".into());
    }
    if options.nofpu {
        tokens.push("nofpu".into());
    }
    if options.enable_cache {
        tokens.push("enable_cache".into());
    }

    if options.limit_2g {
        tokens.push("limit_2g".into());
    }
    if let Some(mb) = options.z2_ram_size {
        tokens.push(format!("z2_ram_size={mb}"));
    }
    if options.enable_c0_slow {
        tokens.push("enable_c0_slow".into());
    }
    if options.enable_c8_slow {
        tokens.push("enable_c8_slow".into());
    }
    if options.enable_d0_slow {
        tokens.push("enable_d0_slow".into());
    }
    if options.move_slow_to_chip {
        tokens.push("move_slow_to_chip".into());
    }

    if let Some(kb) = options.copy_rom_kb {
        tokens.push(format!("copy_rom={kb}"));
    }
    if options.checksum_rom {
        tokens.push("checksum_rom".into());
    }

    if let Some(mb) = options.vc4_mem_mb {
        tokens.push(format!("vc4.mem={mb}"));
    }

    if options.chip_slowdown {
        tokens.push("chip_slowdown".into());
        if let Some(distance) = options.cs_dist {
            tokens.push(format!("cs_dist={distance}"));
        }
    }

    if let Some(drive) = options.swap_df0_with {
        tokens.push(format!("swap_df0_with_df{drive}"));
    }

    if options.one_slot {
        tokens.push("one_slot".into());
    }

    if options.debug {
        tokens.push("debug".into());
    }
    if options.disassemble {
        tokens.push("disassemble".into());
    }
    if options.async_log {
        tokens.push("async_log".into());
    }
    if options.fast_serial {
        tokens.push("fast_serial".into());
    }
    if let Some(kb) = options.buptest_kb {
        tokens.push(format!("buptest={kb}"));
    }
    if let Some(passes) = options.bupiter {
        tokens.push(format!("bupiter={passes}"));
    }

    tokens
}

/// The name part of a token — `sd.unit0` from `sd.unit0=rw`, `vbr_move` from
/// itself.
fn token_name(token: &str) -> &str {
    token.split_once('=').map(|(name, _)| name).unwrap_or(token)
}

/// Everything on the line that is not ART's.
///
/// Shown read-only on the screen, which is the only way a user can see for
/// themselves that their own boot parameters survived — a claim in a tooltip is
/// worth less than the list.
pub fn unmanaged_tokens(existing: &str) -> Vec<String> {
    existing
        .split_whitespace()
        .filter(|token| !MANAGED_TOKENS.contains(&token_name(token)))
        .map(str::to_string)
        .collect()
}

/// The `cmdline.txt` line, with ART's tokens merged into the existing one.
///
/// Unmanaged parameters keep their place and their text; a managed token that
/// is still wanted is rewritten where it stands, so a hand-ordered line stays
/// recognisable; a managed token whose feature is now off is removed; anything
/// new is appended.
pub fn merge_cmdline(
    options: &Emu68Options,
    hardware: PistormHardware,
    existing: Option<&str>,
) -> String {
    let wanted = tokens_for(options, hardware);

    let Some(existing) = existing.filter(|line| !line.trim().is_empty()) else {
        return wanted.join(" ");
    };

    let mut out: Vec<String> = Vec::new();
    let mut written: Vec<&str> = Vec::new();

    for token in existing.split_whitespace() {
        let name = token_name(token);

        if let Some(replacement) = wanted.iter().find(|w| token_name(w) == name) {
            if !written.contains(&name) {
                written.push(name);
                out.push(replacement.clone());
            }
        } else if MANAGED_TOKENS.contains(&name) {
            // Ours, and no longer wanted.
            continue;
        } else {
            // The Pi's own — `root=`, `console=`, `rootfstype=`. Verbatim.
            out.push(token.to_string());
        }
    }

    for token in &wanted {
        if !written.contains(&token_name(token)) {
            out.push(token.clone());
        }
    }

    out.join(" ")
}

/// Read the options back off a card.
///
/// Tokens ART does not know are ignored here and preserved by `merge_cmdline`;
/// this is about showing the user what their card is currently set to, so that
/// the screen opens on the truth rather than on defaults.
pub fn parse_cmdline(existing: &str, hardware: PistormHardware) -> Emu68Options {
    let prefix = hardware.pi.storage_token_prefix();
    let mut options = Emu68Options::default();

    for token in existing.split_whitespace() {
        let (name, value) = match token.split_once('=') {
            Some((name, value)) => (name, Some(value)),
            None => (token, None),
        };

        // The storage tokens are read under either prefix. A card written for a
        // Pi 3 and moved to a CM4 still says `sd.unit0`, and reading it as
        // nothing would silently reset the user's choice — the merge will
        // rewrite it under the right prefix on the way out.
        let storage = name
            .strip_prefix("sd.")
            .or_else(|| name.strip_prefix("emmc."));

        match (storage, name, value) {
            (Some("unit0"), _, Some(value)) => {
                if let Some(exposure) = StorageExposure::from_token_value(value) {
                    options.storage_unit0 = exposure;
                }
            }
            (Some("verbose"), _, Some(value)) => options.storage_verbose = value.parse().ok(),
            (Some("low_speed"), _, _) => options.storage_low_speed = true,
            (Some("clock"), _, Some(value)) => options.storage_clock_mhz = value.parse().ok(),
            (_, "vbr_move", _) => options.vbr_move = true,
            (_, "nofpu", _) => options.nofpu = true,
            (_, "enable_cache", _) => options.enable_cache = true,
            (_, "limit_2g", _) => options.limit_2g = true,
            (_, "z2_ram_size", Some(value)) => options.z2_ram_size = value.parse().ok(),
            (_, "enable_c0_slow", _) => options.enable_c0_slow = true,
            (_, "enable_c8_slow", _) => options.enable_c8_slow = true,
            (_, "enable_d0_slow", _) => options.enable_d0_slow = true,
            (_, "move_slow_to_chip", _) => options.move_slow_to_chip = true,
            (_, "copy_rom", Some(value)) => options.copy_rom_kb = value.parse().ok(),
            (_, "checksum_rom", _) => options.checksum_rom = true,
            (_, "vc4.mem", Some(value)) => options.vc4_mem_mb = value.parse().ok(),
            (_, "chip_slowdown", _) => options.chip_slowdown = true,
            (_, "cs_dist", Some(value)) => options.cs_dist = value.parse().ok(),
            (_, "swap_df0_with_df1", _) => options.swap_df0_with = Some(1),
            (_, "swap_df0_with_df2", _) => options.swap_df0_with = Some(2),
            (_, "swap_df0_with_df3", _) => options.swap_df0_with = Some(3),
            (_, "one_slot", _) => options.one_slot = true,
            (_, "debug", _) => options.debug = true,
            (_, "disassemble", _) => options.disassemble = true,
            (_, "async_log", _) => options.async_log = true,
            (_, "fast_serial", _) => options.fast_serial = true,
            (_, "buptest", Some(value)) => options.buptest_kb = value.parse().ok(),
            (_, "bupiter", Some(value)) => options.bupiter = value.parse().ok(),
            _ => {}
        }
    }

    let _ = prefix;
    gated_for(&options, hardware)
}

#[cfg(test)]
mod tests {
    use super::super::hardware::{AmigaTarget, PiModel, PistormVariant};
    use super::*;

    fn a500() -> PistormHardware {
        PistormHardware::default()
    }

    fn a1200_cm4() -> PistormHardware {
        PistormHardware {
            amiga: AmigaTarget::A1200,
            variant: PistormVariant::Pistorm32Lite,
            pi: PiModel::Cm4,
        }
    }

    #[test]
    fn no_token_art_writes_is_one_emu68_does_not_read() {
        // ART-090's core claim. `emu68.jit`, `emu68.mmu` and
        // `buptest.fastram_size` were written for months and read by nothing.
        // They survive in `MANAGED_TOKENS` only so that a card carrying them
        // gets them removed.
        let every_option = Emu68Options {
            storage_verbose: Some(2),
            storage_low_speed: true,
            storage_clock_mhz: Some(25),
            vbr_move: true,
            nofpu: true,
            enable_cache: true,
            limit_2g: true,
            z2_ram_size: Some(8),
            enable_c0_slow: true,
            enable_c8_slow: true,
            enable_d0_slow: true,
            move_slow_to_chip: true,
            copy_rom_kb: Some(2048),
            checksum_rom: true,
            vc4_mem_mb: Some(64),
            chip_slowdown: true,
            cs_dist: Some(8),
            swap_df0_with: Some(1),
            one_slot: true,
            debug: true,
            disassemble: true,
            async_log: true,
            fast_serial: true,
            buptest_kb: Some(1024),
            bupiter: Some(3),
            ..Emu68Options::default()
        };

        let line = tokens_for(&every_option, a500()).join(" ");
        for fabricated in ["emu68.jit", "emu68.mmu", "buptest.fastram_size"] {
            assert!(!line.contains(fabricated), "{fabricated} in: {line}");
        }
    }

    /// **Both prefixes, whatever the Pi** — changed deliberately on
    /// 2026-08-22, and this test used to assert the opposite.
    ///
    /// ART wrote only the prefix its configured model implies, which meant a
    /// card carried from a Pi3 to a Pi4 silently stopped honouring the
    /// setting. Emu68 picks the driver from the hardware itself, so the
    /// prefix that does not match writes to a disabled device — and the Emu68
    /// Imager writes both for the same reason.
    #[test]
    fn both_storage_prefixes_are_written_whatever_the_pi_is() {
        let options = Emu68Options {
            storage_unit0: StorageExposure::ReadWrite,
            storage_verbose: Some(1),
            ..Emu68Options::default()
        };

        for (what, hardware) in [("Pi3", a500()), ("CM4", a1200_cm4())] {
            let line = tokens_for(&options, hardware).join(" ");
            for expected in [
                "sd.unit0=rw",
                "emmc.unit0=rw",
                "sd.verbose=1",
                "emmc.verbose=1",
            ] {
                assert!(
                    line.contains(expected),
                    "on {what}, missing {expected}: {line}"
                );
            }
        }
    }

    /// The `config.txt` half of the same settings, and the two must agree —
    /// they are one choice written for two Emu68 generations.
    #[test]
    fn the_overlay_block_says_what_the_cmdline_says() {
        let options = Emu68Options {
            storage_unit0: StorageExposure::ReadWrite,
            storage_verbose: Some(2),
            storage_low_speed: true,
            storage_clock_mhz: Some(40),
            ..Emu68Options::default()
        };
        let lines = storage_overlay_lines(&options);
        let block = lines.join("\n");

        // Every parameter the cmdline carries, carried here too.
        for param in ["unit0=rw", "verbose=2", "low_speed", "clock=40"] {
            assert!(block.contains(param), "missing {param}: {block}");
        }

        // **The hazard this shape exists for.** `start.c` only picks the
        // driver by hardware when the node was *not* loaded from an overlay,
        // so an unconditional `dtoverlay=emmc` enables brcm-emmc on a Pi3.
        // Every overlay line must sit under a board filter.
        let mut section = String::new();
        for line in &lines {
            if line.starts_with('[') {
                section = line.clone();
            } else if line.starts_with("dtoverlay=sdhc") {
                assert!(
                    section == "[pi3]" || section == "[pi02]",
                    "sdhc under {section}: {block}"
                );
            } else if line.starts_with("dtoverlay=emmc") {
                assert!(section == "[pi4]", "emmc under {section}: {block}");
            }
        }

        // `[pi4]` already covers CM4 and CM4S in the Raspberry Pi filter
        // table, so a `[cm4]` section would be a second place to keep in step
        // with the first.
        assert!(!block.contains("[cm4]"), "{block}");

        // Closed with `[all]`, which resets the filters — otherwise whatever
        // ART or the user writes next inherits `[pi4]`.
        assert_eq!(lines.last().map(String::as_str), Some("[all]"), "{block}");
    }

    /// The default is `ro`, in both files, and it is stated rather than left
    /// to Emu68's own default — the same reasoning as `MaxTransfer` in
    /// `rdb.rs`: an unstated value is not a conservative one.
    #[test]
    fn the_overlay_block_is_read_only_until_somebody_says_otherwise() {
        let block = storage_overlay_lines(&Emu68Options::default()).join("\n");
        assert!(block.contains("dtoverlay=sdhc,unit0=ro"), "{block}");
        assert!(block.contains("dtoverlay=emmc,unit0=ro"), "{block}");
        assert!(!block.contains("rw"), "{block}");
    }

    #[test]
    fn the_card_is_read_only_until_somebody_says_otherwise() {
        // The safe answer of the three: a mistake on the Amiga side cannot then
        // damage the card ART has just built.
        let line = tokens_for(&Emu68Options::default(), a500()).join(" ");
        assert!(line.contains("sd.unit0=ro"), "{line}");
    }

    #[test]
    fn slow_ram_tokens_are_dropped_on_a_machine_that_has_no_slow_ram() {
        // Not hidden — *dropped*. The Emu68 FAQ's own answer to "my A1200
        // reports the wrong RAM" is to remove them, so leaving them on the line
        // would ship the bug it describes.
        let options = Emu68Options {
            move_slow_to_chip: true,
            enable_c0_slow: true,
            enable_c8_slow: true,
            enable_d0_slow: true,
            ..Emu68Options::default()
        };
        let line = tokens_for(&options, a1200_cm4()).join(" ");
        for token in [
            "move_slow_to_chip",
            "enable_c0_slow",
            "enable_c8_slow",
            "enable_d0_slow",
        ] {
            assert!(!line.contains(token), "{token} in: {line}");
        }

        // And are written where they mean something.
        let on_an_a500 = tokens_for(&options, a500()).join(" ");
        assert!(on_an_a500.contains("move_slow_to_chip"), "{on_an_a500}");
    }

    #[test]
    fn one_slot_is_written_only_on_the_board_that_has_it() {
        let options = Emu68Options {
            one_slot: true,
            ..Emu68Options::default()
        };
        assert!(!tokens_for(&options, a500()).join(" ").contains("one_slot"));
        assert!(tokens_for(&options, a1200_cm4())
            .join(" ")
            .contains("one_slot"));
    }

    #[test]
    fn cs_dist_is_not_written_without_the_slowdown_it_measures() {
        let options = Emu68Options {
            chip_slowdown: false,
            cs_dist: Some(4),
            ..Emu68Options::default()
        };
        assert!(!tokens_for(&options, a500()).join(" ").contains("cs_dist"));
    }

    #[test]
    fn the_pis_own_boot_parameters_survive_a_merge() {
        // The reason this is a merge at all: a regenerated `cmdline.txt` has no
        // `root=`, and a Raspberry Pi without one does not boot.
        let existing = "console=serial0,115200 console=tty1 root=/dev/mmcblk0p2 \
                        rootfstype=ext4 elevator=deadline fsck.repair=yes rootwait";
        let merged = merge_cmdline(&Emu68Options::default(), a500(), Some(existing));

        for parameter in [
            "root=/dev/mmcblk0p2",
            "rootfstype=ext4",
            "console=serial0,115200",
            "console=tty1",
            "rootwait",
        ] {
            assert!(
                merged.contains(parameter),
                "{parameter} lost from: {merged}"
            );
        }
    }

    #[test]
    fn a_token_switched_off_is_removed_rather_than_left_behind() {
        let existing = "root=/dev/mmcblk0p2 vbr_move enable_cache copy_rom=1024";
        let merged = merge_cmdline(&Emu68Options::default(), a500(), Some(existing));
        assert!(!merged.contains("vbr_move"), "{merged}");
        assert!(!merged.contains("enable_cache"), "{merged}");
        assert!(!merged.contains("copy_rom"), "{merged}");
        assert!(merged.contains("root=/dev/mmcblk0p2"), "{merged}");
    }

    #[test]
    fn a_token_that_is_still_wanted_keeps_its_place_on_the_line() {
        // A hand-ordered line should still look like itself afterwards.
        let existing = "root=/dev/x vbr_move console=tty1";
        let options = Emu68Options {
            vbr_move: true,
            ..Emu68Options::default()
        };
        let merged = merge_cmdline(&options, a500(), Some(existing));
        let order: Vec<&str> = merged.split_whitespace().collect();
        assert_eq!(order[0], "root=/dev/x");
        assert_eq!(order[1], "vbr_move");
        assert_eq!(order[2], "console=tty1");
    }

    /// The other prefix is no longer *stale* — it is wanted — but a value
    /// the user changed still has to reach **both**, and a card written by an
    /// older ART carries only one.
    ///
    /// This is the same card the old
    /// `a_stale_token_from_the_other_storage_prefix_is_cleaned_up` was about,
    /// asking the question the new rule makes of it: the `rw` on the line is
    /// ART's own token, so it is replaced with what the options now say, and
    /// the prefix that was missing is added rather than left out.
    #[test]
    fn a_line_from_an_older_art_gains_the_prefix_it_lacks() {
        let existing = "root=/dev/x sd.unit0=rw sd.verbose=2";
        let merged = merge_cmdline(&Emu68Options::default(), a1200_cm4(), Some(existing));

        assert!(merged.contains("sd.unit0=ro"), "{merged}");
        assert!(merged.contains("emmc.unit0=ro"), "{merged}");
        assert!(
            !merged.contains("verbose"),
            "a verbose the options no longer ask for is still removed: {merged}"
        );
        assert!(merged.contains("root=/dev/x"), "{merged}");
        // Once each, under each prefix — a merge that appended rather than
        // replaced would read as two contradictory settings.
        assert_eq!(merged.matches("unit0=").count(), 2, "{merged}");
    }

    #[test]
    fn a_card_written_by_an_older_art_has_its_fictional_tokens_removed() {
        let existing = "root=/dev/x emu68.jit=1 emu68.mmu=1 buptest.fastram_size=1024";
        let merged = merge_cmdline(&Emu68Options::default(), a500(), Some(existing));
        assert!(!merged.contains("emu68."), "{merged}");
        assert!(!merged.contains("buptest.fastram_size"), "{merged}");
        assert!(merged.contains("root=/dev/x"), "{merged}");
    }

    #[test]
    fn a_repeated_token_is_written_once() {
        // Hand-edited lines do this. Writing our replacement twice would be
        // worse than what we found.
        let existing = "vbr_move root=/dev/x vbr_move";
        let options = Emu68Options {
            vbr_move: true,
            ..Emu68Options::default()
        };
        let merged = merge_cmdline(&options, a500(), Some(existing));
        assert_eq!(merged.matches("vbr_move").count(), 1, "{merged}");
    }

    #[test]
    fn an_empty_or_missing_line_gets_just_our_tokens() {
        for existing in [None, Some(""), Some("   \n ")] {
            let merged = merge_cmdline(&Emu68Options::default(), a500(), existing);
            assert_eq!(merged, "sd.unit0=ro emmc.unit0=ro", "{existing:?}");
        }
    }

    #[test]
    fn what_is_not_ours_can_be_shown_to_the_user() {
        let existing = "root=/dev/x vbr_move rootwait enable_cache";
        assert_eq!(unmanaged_tokens(existing), vec!["root=/dev/x", "rootwait"]);
    }

    #[test]
    fn options_survive_a_round_trip_through_the_line() {
        let options = Emu68Options {
            storage_unit0: StorageExposure::ReadWrite,
            storage_verbose: Some(2),
            storage_low_speed: true,
            storage_clock_mhz: Some(25),
            vbr_move: true,
            enable_cache: true,
            limit_2g: true,
            z2_ram_size: Some(4),
            move_slow_to_chip: true,
            copy_rom_kb: Some(512),
            checksum_rom: true,
            vc4_mem_mb: Some(32),
            chip_slowdown: true,
            cs_dist: Some(6),
            swap_df0_with: Some(2),
            debug: true,
            buptest_kb: Some(2048),
            bupiter: Some(2),
            ..Emu68Options::default()
        };
        let line = merge_cmdline(&options, a500(), None);
        assert_eq!(parse_cmdline(&line, a500()), options);
    }

    #[test]
    fn reading_a_card_written_for_another_pi_does_not_lose_the_choice() {
        // `sd.unit0=rw` on a card now in a CM4 still means "the user chose
        // writable". Reading it as nothing would silently reset it, and the
        // save afterwards would write the reset value back.
        let options = parse_cmdline("root=/dev/x sd.unit0=rw sd.verbose=1", a1200_cm4());
        assert_eq!(options.storage_unit0, StorageExposure::ReadWrite);
        assert_eq!(options.storage_verbose, Some(1));
    }

    #[test]
    fn reading_a_card_does_not_import_a_token_this_machine_cannot_use() {
        let options = parse_cmdline("move_slow_to_chip enable_c0_slow", a1200_cm4());
        assert!(!options.move_slow_to_chip);
        assert!(!options.enable_c0_slow);
    }

    #[test]
    fn every_profile_is_made_of_real_tokens_only() {
        // The guard against the old cards' invented claims coming back in a
        // different shape: whatever a profile promises, the line has to be
        // able to say it.
        for profile in Emu68Profile::ALL {
            for hardware in [a500(), a1200_cm4()] {
                let options = profile_options(*profile, hardware);
                for token in tokens_for(&options, hardware) {
                    let name = token_name(&token);
                    assert!(
                        MANAGED_TOKENS.contains(&name),
                        "{profile:?} writes {name}, which is not a token ART owns"
                    );
                }
            }
        }
    }

    #[test]
    fn the_performance_profile_is_the_four_tokens_it_claims() {
        let line = tokens_for(&profile_options(Emu68Profile::Performance, a500()), a500());
        for token in ["vbr_move", "copy_rom=1024", "enable_cache", "vc4.mem=64"] {
            assert!(
                line.contains(&token.to_string()),
                "{token} missing: {line:?}"
            );
        }
    }

    #[test]
    fn the_compatibility_profile_is_a_different_set_on_an_a1200() {
        // Not the same set with a warning: `move_slow_to_chip` is the
        // documented cause of a wrong RAM report there, so it is simply not
        // part of the profile on that machine.
        let on_an_a500 = profile_options(Emu68Profile::Compatibility, a500());
        assert!(on_an_a500.move_slow_to_chip);

        let on_an_a1200 = profile_options(Emu68Profile::Compatibility, a1200_cm4());
        assert!(!on_an_a1200.move_slow_to_chip);
        // The rest of it is unchanged — the profile still means what it means.
        assert!(on_an_a1200.chip_slowdown);
        assert_eq!(on_an_a1200.cs_dist, Some(4));
        assert!(on_an_a1200.limit_2g);
    }

    #[test]
    fn the_daily_profile_leaves_the_machine_alone_apart_from_two_tokens() {
        let options = profile_options(Emu68Profile::Daily, a500());
        assert!(options.enable_cache);
        assert_eq!(options.copy_rom_kb, Some(512));
        assert!(!options.debug, "a default profile must not log");
        assert!(!options.chip_slowdown);
        assert!(!options.vbr_move);
    }
}
