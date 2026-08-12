//! Which Amiga, which PiStorm, which Raspberry Pi — and what follows from it.
//!
//! **This is the foundation the PiStorm screen models.** What a PiStorm setup
//! can do is a function of three choices, not one: the Amiga the board is
//! fitted to, the board itself, and the Pi on the board. Everything downstream
//! is derived — which Emu68 build to fetch, what the storage device is called
//! in every generated hint, which `cmdline.txt` tokens are even meaningful, how
//! much Fast RAM there can be.
//!
//! The screen used to offer a single vague "model" dropdown and then invent the
//! rest (ART-090). Every table here comes from the sources listed in
//! `ART-brief-pistorm-studio-v2.md` — the official Emu68 documentation, the
//! PiStorm hardware page and FAQ, and the wiki.amiga.org board pages — and
//! nothing is here that they do not say.

use serde::{Deserialize, Serialize};

/// The Amiga the accelerator is fitted to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum AmigaTarget {
    A500,
    A1000,
    A2000,
    A600,
    A1200,
}

impl AmigaTarget {
    pub const ALL: &'static [Self] = &[
        Self::A500,
        Self::A1000,
        Self::A2000,
        Self::A600,
        Self::A1200,
    ];

    pub fn display_name(self) -> &'static str {
        match self {
            Self::A500 => "Amiga 500",
            Self::A1000 => "Amiga 1000",
            Self::A2000 => "Amiga 2000",
            Self::A600 => "Amiga 600",
            Self::A1200 => "Amiga 1200",
        }
    }

    /// Whether this machine has the trapdoor / ranger memory the slow-RAM
    /// tokens describe.
    ///
    /// `move_slow_to_chip`, `enable_c0_slow`, `enable_c8_slow` and
    /// `enable_d0_slow` are A500-family concepts. The official Emu68 FAQ's
    /// answer to "my A1200 reports the wrong RAM" is literally *remove those
    /// tokens* — so on an A1200 or A600 they are not merely useless, they are
    /// the documented cause of a bug. The screen hides them, and a profile
    /// that carries them drops them.
    pub fn has_slow_ram(self) -> bool {
        matches!(self, Self::A500 | Self::A1000 | Self::A2000)
    }
}

/// The PiStorm board.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PistormVariant {
    /// PiStorm, the original — CPU socket of the A500/A1000/A2000.
    Classic,
    /// PiStorm600 — the A600's PLCC socket.
    Pistorm600,
    /// PiStorm16 — A600, Compute Module only.
    Pistorm16,
    /// PiStorm32-lite — the A1200's CPU slot.
    Pistorm32Lite,
}

impl PistormVariant {
    pub const ALL: &'static [Self] = &[
        Self::Classic,
        Self::Pistorm600,
        Self::Pistorm16,
        Self::Pistorm32Lite,
    ];

    pub fn display_name(self) -> &'static str {
        match self {
            Self::Classic => "PiStorm",
            Self::Pistorm600 => "PiStorm600",
            Self::Pistorm16 => "PiStorm16",
            Self::Pistorm32Lite => "PiStorm32-lite",
        }
    }

    /// `one_slot` — forcing the single-slot protocol — exists only here.
    pub fn has_one_slot_option(self) -> bool {
        matches!(self, Self::Pistorm32Lite)
    }
}

/// Which line of Emu68 releases a card is being built against.
///
/// **Not a nicety.** The archive names are not stable across the two lines, and
/// one of them changes meaning: in 1.0.x, `Emu68-pistorm.zip` is the *classic*
/// board's firmware; in 1.1 alpha it is the **PiStorm32-lite and PiStorm16**
/// firmware, and the classic board's has been renamed
/// `Emu68-pistorm-classic.zip`. A user told "download Emu68-pistorm.zip" with
/// no line to go with it has a good chance of flashing the wrong one.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Emu68Line {
    /// The latest stable release — 1.0.7 when this was written.
    #[default]
    Stable,
    /// The 1.1 alpha line: `v1.1.0-alpha.1`, a GitHub prerelease, and the
    /// first Emu68 to support PiStorm16 at all.
    Alpha11,
}

impl Emu68Line {
    pub const ALL: &'static [Self] = &[Self::Stable, Self::Alpha11];
}

/// What the kernel archive for a board is called — or why there is no answer.
///
/// Three cases rather than a string, because two of them are real and a string
/// cannot hold them. Returning a plausible filename for a board the release
/// does not ship is exactly the slip this type exists to make impossible: ART
/// claimed `Emu68-pistorm16.zip` for months, and no Emu68 release has ever
/// contained a file by that name.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", tag = "kind", content = "name")]
pub enum KernelArchive {
    /// The asset's real filename in that release.
    Named(&'static str),
    /// That release line ships nothing for this board.
    Absent,
    /// The release exists for this board but its notes do not say which asset
    /// covers it. Not the same as absent, and not something to guess at.
    Unstated,
}

/// The Emu68 archive for a board, in a given release line.
///
/// Verified 2026-08-13 against the GitHub releases API and the project's own
/// SD-preparation tutorial:
///
/// | | 1.0.7 (stable) | 1.1.0-alpha.1 |
/// |---|---|---|
/// | PiStorm (classic) | `Emu68-pistorm.zip` | `Emu68-pistorm-classic.zip` |
/// | PiStorm600 | `Emu68-pistorm.zip` | not stated |
/// | PiStorm32-lite | `Emu68-pistorm32lite.zip` | `Emu68-pistorm.zip` |
/// | PiStorm16 | **no asset** | `Emu68-pistorm.zip` |
///
/// PiStorm600 sits under the classic archive in the stable line because the
/// tutorial says so in as many words — "the release for users of classic
/// PiStorm for A500, A600, A1000, or A2000". The 1.1 alpha notes name the
/// classic archive for "A500, A1000, A2000" and say nothing about the A600, so
/// that cell is `Unstated` rather than a guess either way.
pub fn kernel_archive(variant: PistormVariant, line: Emu68Line) -> KernelArchive {
    use Emu68Line::*;
    use PistormVariant::*;

    match (variant, line) {
        (Classic, Stable) => KernelArchive::Named("Emu68-pistorm.zip"),
        (Classic, Alpha11) => KernelArchive::Named("Emu68-pistorm-classic.zip"),
        (Pistorm600, Stable) => KernelArchive::Named("Emu68-pistorm.zip"),
        (Pistorm600, Alpha11) => KernelArchive::Unstated,
        (Pistorm32Lite, Stable) => KernelArchive::Named("Emu68-pistorm32lite.zip"),
        (Pistorm32Lite, Alpha11) => KernelArchive::Named("Emu68-pistorm.zip"),
        (Pistorm16, Stable) => KernelArchive::Absent,
        (Pistorm16, Alpha11) => KernelArchive::Named("Emu68-pistorm.zip"),
    }
}

/// The Raspberry Pi on the board.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PiModel {
    Zero2W,
    Pi3A,
    Pi3APlus,
    Pi3B,
    Pi3BPlus,
    Pi4B,
    Cm4,
}

impl PiModel {
    pub const ALL: &'static [Self] = &[
        Self::Zero2W,
        Self::Pi3A,
        Self::Pi3APlus,
        Self::Pi3B,
        Self::Pi3BPlus,
        Self::Pi4B,
        Self::Cm4,
    ];

    pub fn display_name(self) -> &'static str {
        match self {
            Self::Zero2W => "Raspberry Pi Zero 2 W",
            Self::Pi3A => "Raspberry Pi 3A",
            Self::Pi3APlus => "Raspberry Pi 3A+",
            Self::Pi3B => "Raspberry Pi 3B",
            Self::Pi3BPlus => "Raspberry Pi 3B+",
            Self::Pi4B => "Raspberry Pi 4B",
            Self::Cm4 => "Raspberry Pi Compute Module 4",
        }
    }

    /// How the Pi exposes its card, and therefore what the driver is called.
    ///
    /// The Pi 3 family answers to `brcm-sdhc.device`, the Pi 4 and CM4 to
    /// `brcm-emmc.device`. Every partition hint, HDToolBox instruction and
    /// generated note ART prints has to use the right one; the name is what
    /// the user types into a mountlist, and a wrong one simply does not mount.
    pub fn storage_device(self) -> &'static str {
        match self {
            Self::Pi4B | Self::Cm4 => "brcm-emmc.device",
            _ => "brcm-sdhc.device",
        }
    }

    /// The `cmdline.txt` prefix for the storage options — `sd.*` or `emmc.*`.
    pub fn storage_token_prefix(self) -> &'static str {
        match self {
            Self::Pi4B | Self::Cm4 => "emmc",
            _ => "sd",
        }
    }

    /// How much RAM this Pi has, in MB: a single figure, or a range for the
    /// models sold in several sizes.
    ///
    /// Informational, and only ever that. The Amiga's Fast RAM comes out of the
    /// Pi's RAM, but **Emu68 maps it automatically** — there is no size to set,
    /// which is why the old screen's Fast RAM slider was fiction.
    pub fn ram_mb(self) -> (u32, u32) {
        match self {
            Self::Zero2W | Self::Pi3A | Self::Pi3APlus => (512, 512),
            Self::Pi3B | Self::Pi3BPlus => (1024, 1024),
            Self::Pi4B | Self::Cm4 => (1024, 8192),
        }
    }
}

/// Community guidance, not a hardware limit: AmigaOS itself does not use more
/// than 2 GB of Fast RAM, so a Pi with more is not buying the Amiga anything.
pub const USEFUL_FAST_RAM_CEILING_MB: u32 = 2048;

/// How well a Pi is known to work on a board.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PiSupport {
    /// Listed by the project as supported.
    Supported,
    /// Reported working by users, not guaranteed by the project.
    Reported,
}

/// The three choices together.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct PistormHardware {
    pub amiga: AmigaTarget,
    pub variant: PistormVariant,
    pub pi: PiModel,
}

impl Default for PistormHardware {
    /// The setup this project was developed against: an A500 with a classic
    /// PiStorm and a Pi 3A. A default that exists is better than a blank
    /// screen, and one somebody has actually booted is better than a guess.
    fn default() -> Self {
        Self {
            amiga: AmigaTarget::A500,
            variant: PistormVariant::Classic,
            pi: PiModel::Pi3A,
        }
    }
}

/// The boards that fit a given Amiga.
pub fn variants_for(amiga: AmigaTarget) -> &'static [PistormVariant] {
    match amiga {
        // The A2000 needs a CPU adapter; the A1000 likewise. Both are the same
        // board, so they are the same answer here — the adapter is a note for
        // the docs panel, not a separate product.
        AmigaTarget::A500 | AmigaTarget::A1000 | AmigaTarget::A2000 => &[PistormVariant::Classic],
        AmigaTarget::A600 => &[PistormVariant::Pistorm600, PistormVariant::Pistorm16],
        AmigaTarget::A1200 => &[PistormVariant::Pistorm32Lite],
    }
}

/// The Pis that run on a given board, each with how well it is known to work.
pub fn pi_models_for(variant: PistormVariant) -> &'static [(PiModel, PiSupport)] {
    match variant {
        // Note the absence of the original Zero W: it is not merely slow here,
        // it is incompatible, and it is the single commonest mistake made when
        // buying parts for this board.
        PistormVariant::Classic => &[
            (PiModel::Zero2W, PiSupport::Supported),
            (PiModel::Pi3A, PiSupport::Supported),
            (PiModel::Pi3APlus, PiSupport::Supported),
            (PiModel::Pi3B, PiSupport::Supported),
            (PiModel::Pi3BPlus, PiSupport::Supported),
            (PiModel::Pi4B, PiSupport::Reported),
        ],
        PistormVariant::Pistorm600 => &[
            (PiModel::Zero2W, PiSupport::Supported),
            (PiModel::Pi3APlus, PiSupport::Supported),
            (PiModel::Pi3B, PiSupport::Supported),
            (PiModel::Pi3BPlus, PiSupport::Supported),
        ],
        PistormVariant::Pistorm16 => &[(PiModel::Cm4, PiSupport::Supported)],
        PistormVariant::Pistorm32Lite => &[
            (PiModel::Zero2W, PiSupport::Supported),
            (PiModel::Pi3A, PiSupport::Supported),
            (PiModel::Pi3APlus, PiSupport::Supported),
            (PiModel::Pi3B, PiSupport::Supported),
            (PiModel::Pi3BPlus, PiSupport::Supported),
            (PiModel::Pi4B, PiSupport::Supported),
            (PiModel::Cm4, PiSupport::Supported),
        ],
    }
}

/// Something the user should know about the combination they picked.
///
/// Identified rather than worded: `CoreError` aside, `core/` produces no
/// user-facing English, and these have to arrive in the user's own language
/// (§68). The UI resolves the id through `pistorm.note.*`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum HardwareNote {
    /// This Pi is reported working on this board but is not guaranteed by the
    /// project.
    PiNotGuaranteed,
    /// A CM4 with onboard eMMC cannot use the microSD slot at the same time —
    /// hence the community's "CM4 Lite, no eMMC" recommendation.
    Cm4NeedsLiteForSdCard,
    /// A 3B or 3B+ on a classic PiStorm needs port removal or a taller
    /// stacking header to fit.
    PiPhysicalFit,
    /// The 3B has no activity-LED support here.
    NoActivityLed,
    /// An A2000 or A1000 needs a CPU adapter to take this board.
    NeedsCpuAdapter,
    /// A Pi that browns out silently underclocks itself, which reads as "my
    /// PiStorm is slow" and is the commonest reported performance mystery.
    PowerSupplyQuality,
    /// This Pi has more RAM than AmigaOS can use as Fast RAM.
    RamBeyondWhatAmigaOsUses,
    /// No stable Emu68 supports this board; the 1.1 alpha is the first that
    /// does.
    NeedsPrereleaseEmu68,
    /// `Emu68-pistorm.zip` means a **different board** in the stable line. The
    /// name alone is not enough to download by.
    ArchiveNameDiffersByRelease,
    /// The release notes for this line do not say which asset covers this
    /// board.
    ArchiveNotStatedForThisRelease,
}

/// Everything worth saying about one combination.
pub fn notes_for(hardware: PistormHardware, line: Emu68Line) -> Vec<HardwareNote> {
    let mut notes = Vec::new();

    match kernel_archive(hardware.variant, line) {
        KernelArchive::Absent => notes.push(HardwareNote::NeedsPrereleaseEmu68),
        KernelArchive::Unstated => notes.push(HardwareNote::ArchiveNotStatedForThisRelease),
        KernelArchive::Named(name) => {
            // The one dangerous case: the same filename is the classic board's
            // firmware in the other line, so a user who writes the name down
            // and fetches "the latest Emu68" gets firmware for another board.
            let means_something_else = Emu68Line::ALL.iter().any(|other| {
                *other != line
                    && kernel_archive(PistormVariant::Classic, *other) == KernelArchive::Named(name)
                    && hardware.variant != PistormVariant::Classic
            });
            if means_something_else {
                notes.push(HardwareNote::ArchiveNameDiffersByRelease);
            }
        }
    }

    if pi_models_for(hardware.variant)
        .iter()
        .any(|(pi, support)| *pi == hardware.pi && *support == PiSupport::Reported)
    {
        notes.push(HardwareNote::PiNotGuaranteed);
    }

    if hardware.pi == PiModel::Cm4 {
        notes.push(HardwareNote::Cm4NeedsLiteForSdCard);
    }

    if hardware.variant == PistormVariant::Classic
        && matches!(hardware.pi, PiModel::Pi3B | PiModel::Pi3BPlus)
    {
        notes.push(HardwareNote::PiPhysicalFit);
    }

    if hardware.pi == PiModel::Pi3B {
        notes.push(HardwareNote::NoActivityLed);
    }

    if matches!(hardware.amiga, AmigaTarget::A1000 | AmigaTarget::A2000) {
        notes.push(HardwareNote::NeedsCpuAdapter);
    }

    if hardware.pi.ram_mb().1 > USEFUL_FAST_RAM_CEILING_MB {
        notes.push(HardwareNote::RamBeyondWhatAmigaOsUses);
    }

    // Last because it applies to every combination — it is the closing line of
    // the panel, not a reaction to a choice.
    notes.push(HardwareNote::PowerSupplyQuality);
    notes
}

/// Whether the three choices go together at all.
pub fn is_coherent(hardware: PistormHardware) -> bool {
    variants_for(hardware.amiga).contains(&hardware.variant)
        && pi_models_for(hardware.variant)
            .iter()
            .any(|(pi, _)| *pi == hardware.pi)
}

/// The nearest coherent setup to one that is not.
///
/// A settings file from an older ART, or a user who changed the Amiga and left
/// the rest, can produce an A1200 with a PiStorm600. Rather than refuse to draw
/// the screen, each field falls back to the first choice its predecessor allows
/// — which is what the dropdowns do anyway when the one above them changes.
pub fn nearest_coherent(hardware: PistormHardware) -> PistormHardware {
    let variant = if variants_for(hardware.amiga).contains(&hardware.variant) {
        hardware.variant
    } else {
        variants_for(hardware.amiga)[0]
    };
    let pis = pi_models_for(variant);
    let pi = if pis.iter().any(|(pi, _)| *pi == hardware.pi) {
        hardware.pi
    } else {
        pis[0].0
    };
    PistormHardware {
        amiga: hardware.amiga,
        variant,
        pi,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The notes for a setup on the stable release line — what most of these
    /// tests are asking about, with the release line held still.
    fn notes_for_stable(hardware: PistormHardware) -> Vec<HardwareNote> {
        notes_for(hardware, Emu68Line::Stable)
    }

    #[test]
    fn every_amiga_has_a_board_and_every_board_has_a_pi() {
        // The screen's three dropdowns filter each other. A machine with no
        // board, or a board with no Pi, would leave the next field empty and
        // the one after it undefined — and `variants_for(..)[0]` below would
        // panic rather than produce it.
        for amiga in AmigaTarget::ALL {
            let variants = variants_for(*amiga);
            assert!(!variants.is_empty(), "{amiga:?} has no board");
            for variant in variants {
                assert!(!pi_models_for(*variant).is_empty(), "{variant:?} has no Pi");
            }
        }
    }

    #[test]
    fn the_original_zero_w_is_not_offered_anywhere() {
        // It is not slow here, it is incompatible — and buying one is the
        // commonest mistake made when assembling this. It is absent from
        // `PiModel` entirely, which is the strongest way to not offer it.
        for pi in PiModel::ALL {
            assert_ne!(pi.display_name(), "Raspberry Pi Zero W");
        }
    }

    #[test]
    fn the_storage_device_name_follows_the_pi_and_not_the_board() {
        // This name reaches the user's mountlist. `emu68-sd.device`, which the
        // screen printed before ART-090, mounts nothing at all — the `emu68-`
        // prefix belongs to the RTG card (`emu68-vc4.card`), not to storage.
        assert_eq!(PiModel::Pi3A.storage_device(), "brcm-sdhc.device");
        assert_eq!(PiModel::Zero2W.storage_device(), "brcm-sdhc.device");
        assert_eq!(PiModel::Pi4B.storage_device(), "brcm-emmc.device");
        assert_eq!(PiModel::Cm4.storage_device(), "brcm-emmc.device");
    }

    #[test]
    fn a_pistorm32_lite_answers_to_both_names_depending_on_its_pi() {
        // The same board, two answers — which is exactly why the Pi has to be
        // a field of its own rather than implied by the board.
        assert_eq!(
            PiModel::Pi3BPlus.storage_device(),
            "brcm-sdhc.device",
            "a 32-lite on a Pi 3"
        );
        assert_eq!(
            PiModel::Cm4.storage_device(),
            "brcm-emmc.device",
            "a 32-lite on a CM4"
        );
    }

    #[test]
    fn the_storage_token_prefix_matches_the_device() {
        for pi in PiModel::ALL {
            let expected = if pi.storage_device() == "brcm-emmc.device" {
                "emmc"
            } else {
                "sd"
            };
            assert_eq!(pi.storage_token_prefix(), expected, "{pi:?}");
        }
    }

    #[test]
    fn slow_ram_tokens_belong_to_the_a500_family_only() {
        // The Emu68 FAQ's own answer to "my A1200 shows the wrong RAM" is to
        // remove them, so on an A1200 they are the documented cause of a bug
        // rather than a harmless extra.
        assert!(AmigaTarget::A500.has_slow_ram());
        assert!(AmigaTarget::A1000.has_slow_ram());
        assert!(AmigaTarget::A2000.has_slow_ram());
        assert!(!AmigaTarget::A600.has_slow_ram());
        assert!(!AmigaTarget::A1200.has_slow_ram());
    }

    #[test]
    fn one_slot_is_offered_only_where_it_exists() {
        assert!(PistormVariant::Pistorm32Lite.has_one_slot_option());
        for variant in PistormVariant::ALL {
            if *variant != PistormVariant::Pistorm32Lite {
                assert!(!variant.has_one_slot_option(), "{variant:?}");
            }
        }
    }

    #[test]
    fn a_pistorm16_is_a_compute_module_board_and_says_so() {
        let pis = pi_models_for(PistormVariant::Pistorm16);
        assert_eq!(pis.len(), 1);
        assert_eq!(pis[0].0, PiModel::Cm4);
    }

    #[test]
    fn the_default_setup_is_one_somebody_has_booted() {
        // An A500 with a classic PiStorm and a Pi 3A — this project's own
        // bench. A default that has never been assembled is a guess.
        let hardware = PistormHardware::default();
        assert!(is_coherent(hardware));
        assert_eq!(
            kernel_archive(hardware.variant, Emu68Line::Stable),
            KernelArchive::Named("Emu68-pistorm.zip")
        );
        assert_eq!(hardware.pi.storage_device(), "brcm-sdhc.device");
        assert!(hardware.amiga.has_slow_ram());
    }

    /// ART-091. Verified 2026-08-13 against
    /// `api.github.com/repos/michalsc/Emu68/releases` and
    /// `pistorm.github.io/tutorials/sd_setup/`.
    #[test]
    fn the_kernel_archive_names_are_the_ones_the_releases_actually_ship() {
        use Emu68Line::*;
        use PistormVariant::*;

        // 1.0.7 ships exactly three assets: Emu68-pistorm.zip,
        // Emu68-pistorm32lite.zip, Emu68-raspi.zip.
        assert_eq!(
            kernel_archive(Classic, Stable),
            KernelArchive::Named("Emu68-pistorm.zip")
        );
        assert_eq!(
            kernel_archive(Pistorm600, Stable),
            KernelArchive::Named("Emu68-pistorm.zip"),
            "the tutorial names the A600 under classic PiStorm"
        );
        assert_eq!(
            kernel_archive(Pistorm32Lite, Stable),
            KernelArchive::Named("Emu68-pistorm32lite.zip")
        );

        // 1.1.0-alpha.1 ships Emu68-pistorm-classic.zip, Emu68-pistorm.zip,
        // Emu68-raspi.zip and VideoCore.card.
        assert_eq!(
            kernel_archive(Classic, Alpha11),
            KernelArchive::Named("Emu68-pistorm-classic.zip")
        );
        assert_eq!(
            kernel_archive(Pistorm32Lite, Alpha11),
            KernelArchive::Named("Emu68-pistorm.zip")
        );
        assert_eq!(
            kernel_archive(Pistorm16, Alpha11),
            KernelArchive::Named("Emu68-pistorm.zip")
        );
    }

    #[test]
    fn no_archive_name_is_one_no_release_has_ever_contained() {
        // ART claimed `Emu68-pistorm16.zip` for months. No Emu68 release has
        // ever shipped a file by that name — the PiStorm16 build is in
        // `Emu68-pistorm.zip`, and only from 1.1 alpha onward.
        for variant in PistormVariant::ALL {
            for line in Emu68Line::ALL {
                if let KernelArchive::Named(name) = kernel_archive(*variant, *line) {
                    assert!(
                        [
                            "Emu68-pistorm.zip",
                            "Emu68-pistorm-classic.zip",
                            "Emu68-pistorm32lite.zip",
                        ]
                        .contains(&name),
                        "{name} is not an asset any verified Emu68 release ships"
                    );
                }
            }
        }
    }

    #[test]
    fn a_pistorm16_has_no_stable_release_and_says_so() {
        assert_eq!(
            kernel_archive(PistormVariant::Pistorm16, Emu68Line::Stable),
            KernelArchive::Absent
        );

        let notes = notes_for(
            PistormHardware {
                amiga: AmigaTarget::A600,
                variant: PistormVariant::Pistorm16,
                pi: PiModel::Cm4,
            },
            Emu68Line::Stable,
        );
        assert!(notes.contains(&HardwareNote::NeedsPrereleaseEmu68));
    }

    #[test]
    fn a_name_that_means_another_board_in_the_other_release_is_flagged() {
        // The trap: a PiStorm32-lite or PiStorm16 user on the 1.1 alpha line is
        // told `Emu68-pistorm.zip` — which, in the stable line they are far
        // more likely to land on, is the *classic* board's firmware.
        for variant in [PistormVariant::Pistorm32Lite, PistormVariant::Pistorm16] {
            let notes = notes_for(
                PistormHardware {
                    amiga: AmigaTarget::A1200,
                    variant,
                    pi: PiModel::Cm4,
                },
                Emu68Line::Alpha11,
            );
            assert!(
                notes.contains(&HardwareNote::ArchiveNameDiffersByRelease),
                "{variant:?}"
            );
        }

        // The classic board's own name is not a trap for the classic board.
        let classic = notes_for(PistormHardware::default(), Emu68Line::Stable);
        assert!(!classic.contains(&HardwareNote::ArchiveNameDiffersByRelease));
    }

    #[test]
    fn a_board_the_notes_do_not_cover_is_not_guessed_at() {
        // The 1.1 alpha notes name the classic archive for "A500, A1000,
        // A2000" and say nothing about the A600's own board.
        assert_eq!(
            kernel_archive(PistormVariant::Pistorm600, Emu68Line::Alpha11),
            KernelArchive::Unstated
        );
        let notes = notes_for(
            PistormHardware {
                amiga: AmigaTarget::A600,
                variant: PistormVariant::Pistorm600,
                pi: PiModel::Zero2W,
            },
            Emu68Line::Alpha11,
        );
        assert!(notes.contains(&HardwareNote::ArchiveNotStatedForThisRelease));
    }

    #[test]
    fn an_incoherent_setup_is_repaired_rather_than_refused() {
        // What a settings file from an older ART, or half-finished editing,
        // looks like. Refusing to draw the screen over it helps nobody.
        let broken = PistormHardware {
            amiga: AmigaTarget::A1200,
            variant: PistormVariant::Pistorm600,
            pi: PiModel::Cm4,
        };
        assert!(!is_coherent(broken));

        let fixed = nearest_coherent(broken);
        assert!(is_coherent(fixed));
        assert_eq!(
            fixed.amiga,
            AmigaTarget::A1200,
            "the user's own choice wins"
        );
        assert_eq!(fixed.variant, PistormVariant::Pistorm32Lite);
    }

    #[test]
    fn repairing_a_coherent_setup_changes_nothing() {
        for amiga in AmigaTarget::ALL {
            for variant in variants_for(*amiga) {
                for (pi, _) in pi_models_for(*variant) {
                    let hardware = PistormHardware {
                        amiga: *amiga,
                        variant: *variant,
                        pi: *pi,
                    };
                    assert_eq!(nearest_coherent(hardware), hardware);
                }
            }
        }
    }

    #[test]
    fn a_pi_that_is_only_reported_working_is_labelled_as_such() {
        // Honesty about somebody else's hardware budget: "reported working,
        // not guaranteed" is what the project says, so it is what ART says.
        let notes = notes_for_stable(PistormHardware {
            amiga: AmigaTarget::A500,
            variant: PistormVariant::Classic,
            pi: PiModel::Pi4B,
        });
        assert!(notes.contains(&HardwareNote::PiNotGuaranteed));

        let supported = notes_for_stable(PistormHardware::default());
        assert!(!supported.contains(&HardwareNote::PiNotGuaranteed));
    }

    #[test]
    fn a_cm4_is_told_about_its_emmc_before_the_card_is_built() {
        let notes = notes_for_stable(PistormHardware {
            amiga: AmigaTarget::A1200,
            variant: PistormVariant::Pistorm32Lite,
            pi: PiModel::Cm4,
        });
        assert!(notes.contains(&HardwareNote::Cm4NeedsLiteForSdCard));
    }

    #[test]
    fn every_combination_gets_the_power_supply_note() {
        // Undervoltage makes a Pi silently underclock, and "my PiStorm is
        // slow" is the commonest thing it is mistaken for.
        for amiga in AmigaTarget::ALL {
            for variant in variants_for(*amiga) {
                for (pi, _) in pi_models_for(*variant) {
                    let notes = notes_for_stable(PistormHardware {
                        amiga: *amiga,
                        variant: *variant,
                        pi: *pi,
                    });
                    assert!(notes.contains(&HardwareNote::PowerSupplyQuality));
                }
            }
        }
    }

    #[test]
    fn a_pi_with_more_ram_than_amigaos_uses_says_so() {
        let notes = notes_for_stable(PistormHardware {
            amiga: AmigaTarget::A1200,
            variant: PistormVariant::Pistorm32Lite,
            pi: PiModel::Pi4B,
        });
        assert!(notes.contains(&HardwareNote::RamBeyondWhatAmigaOsUses));

        // A 512 MB Pi has nothing to say about a 2 GB ceiling.
        let small = notes_for_stable(PistormHardware::default());
        assert!(!small.contains(&HardwareNote::RamBeyondWhatAmigaOsUses));
    }
}
