//! What does this file become in *this* card? (SD-1 · G15)
//!
//! ART has exactly one drag & drop pipeline and it is architectural rather
//! than a convenience: `analyze_paths` → `WorkflowEngine::plan` → "what can I
//! do with this file". The card builder needs the other question — **what does
//! this file become in the card being built** — and the gap analysis is right
//! that what is missing is the question, not the pipeline.
//!
//! ## The honest answer is narrow, and says so
//!
//! An SD-1 card has a FAT32 boot partition and Amiga areas whose volumes are
//! **not formatted**. So the only files that have a place on it are the ones
//! the Raspberry Pi's firmware reads: the Emu68 release and a Kickstart. A
//! WHDLoad archive, an ADF, an AmigaOS ISO — all of them belong on an Amiga
//! volume, and this card has none yet.
//!
//! That is [`CardRole::ForAnAmigaVolume`], and it exists so the answer is
//! *"not yet, and here is why"* rather than a shrug or, worse, a silent drop.
//! When SD-2 formats a volume, those files get a real answer and this is where
//! it goes.
//!
//! ## Names, not bytes, for the archive — and that is deliberate
//!
//! Everywhere else ART decides what a file is by reading it (`core/detect`).
//! An Emu68 release is a plain zip: its *bytes* say "zip", and which board and
//! which release line it is for lives only in its **name** — which is exactly
//! what ART-091 was about. So the name is what is matched, against the table
//! in `core/pistorm/hardware`, and the answer carries which board and line the
//! name implies rather than asserting the file is right for the user's setup.
//! `emu68_payload` still makes that decision, and still refuses.

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::core::detect::FormatCategory;
use crate::core::pistorm::hardware::{kernel_archive, Emu68Line, KernelArchive, PistormVariant};

/// Which board and release line an Emu68 archive's *name* implies.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArchiveNameMeans {
    pub variant: PistormVariant,
    pub line: Emu68Line,
}

/// What a dropped file would become on the card being built.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum CardRole {
    /// The Emu68 release archive — the boot partition's payload.
    ///
    /// `means` is what the *name* implies, and may be more than one entry:
    /// `Emu68-pistorm.zip` is the classic board on the stable line and the
    /// PiStorm600 on it too (ART-091). Never a claim that it suits the user's
    /// board — `emu68_payload` decides that and refuses.
    Emu68Archive { means: Vec<ArchiveNameMeans> },
    /// A Kickstart ROM for the boot partition.
    Kickstart,
    /// `config_<name>.txt` — a distribution's own Pi config, the static half
    /// of multiboot. **Recognised, not used**: choosing between them is an
    /// Amiga-side selector and ART's own code to write (G16).
    DistroConfig { name: String },
    /// It belongs on an Amiga volume, and this card has none formatted yet.
    ForAnAmigaVolume { what: FormatCategory },
    /// ART knows what it is and it has no place on a card at all.
    NoPlaceOnACard { what: FormatCategory },
}

/// `Emu68-raspi.zip` is Emu68 on a Pi by itself, not firmware for a PiStorm.
/// It sits in the same release and is the commonest thing to pick by mistake.
const RASPI_ARCHIVE: &str = "emu68-raspi.zip";

/// Every board and line whose archive is called `name`.
///
/// Several, because one name means different boards in different lines, and
/// the drop target has no business picking one for the user.
fn archive_name_means(name: &str) -> Vec<ArchiveNameMeans> {
    use Emu68Line::*;
    use PistormVariant::*;

    let mut found = Vec::new();
    for variant in [Classic, Pistorm600, Pistorm16, Pistorm32Lite] {
        for line in [Stable, Alpha11] {
            if let KernelArchive::Named(expected) = kernel_archive(variant, line) {
                if expected.eq_ignore_ascii_case(name) {
                    found.push(ArchiveNameMeans { variant, line });
                }
            }
        }
    }
    found
}

/// What `path`, already detected as `category`, becomes on a card.
///
/// `category` comes from `core::detect` — this does not re-detect, so the
/// drop pipeline's one answer stays the one answer.
pub fn role_for(path: &Path, category: FormatCategory) -> CardRole {
    let name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    let lower = name.to_lowercase();

    if category == FormatCategory::Rom {
        return CardRole::Kickstart;
    }

    if category == FormatCategory::Archive {
        // `Emu68-raspi.zip` is in the release and is not for a PiStorm. Named
        // for what it is rather than swept into "belongs on a volume".
        if lower == RASPI_ARCHIVE {
            return CardRole::NoPlaceOnACard {
                what: FormatCategory::Archive,
            };
        }
        let means = archive_name_means(&name);
        if !means.is_empty() {
            return CardRole::Emu68Archive { means };
        }
    }

    // `config_<name>.txt` — the static half of the multiboot mechanism that
    // ships in the field today (SD-0 §3.2).
    if let Some(rest) = lower.strip_prefix("config_") {
        if let Some(stem) = rest.strip_suffix(".txt") {
            if !stem.is_empty() {
                return CardRole::DistroConfig {
                    name: stem.to_string(),
                };
            }
        }
    }

    match category {
        // Everything an Amiga would keep on a volume. This card has none
        // formatted, so the answer is "not yet", with the reason.
        FormatCategory::FloppyImage
        | FormatCategory::HardDiskImage
        | FormatCategory::OpticalImage
        | FormatCategory::Archive
        | FormatCategory::Directory => CardRole::ForAnAmigaVolume { what: category },
        // A 1541 disk has no business anywhere near a PiStorm card, and
        // saying so beats leaving it in the "not yet" pile.
        FormatCategory::Commodore8Bit | FormatCategory::Unknown => {
            CardRole::NoPlaceOnACard { what: category }
        }
        FormatCategory::Rom => CardRole::Kickstart,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn role(name: &str, category: FormatCategory) -> CardRole {
        role_for(&PathBuf::from(format!("E:\\downloads\\{name}")), category)
    }

    /// The one that makes the feature worth having: drop the release archive
    /// and the form knows what it is.
    #[test]
    fn the_emu68_release_archive_is_recognised_by_name() {
        let CardRole::Emu68Archive { means } = role("Emu68-pistorm.zip", FormatCategory::Archive)
        else {
            panic!("not recognised");
        };
        assert!(means.contains(&ArchiveNameMeans {
            variant: PistormVariant::Classic,
            line: Emu68Line::Stable,
        }));
    }

    /// **ART-091, carried into the drop target.** One name means different
    /// boards in different release lines, so the answer carries every reading
    /// rather than picking one — the user's own setting decides, and
    /// `emu68_payload` refuses if it disagrees.
    #[test]
    fn a_name_that_means_two_boards_says_both() {
        let CardRole::Emu68Archive { means } = role("Emu68-pistorm.zip", FormatCategory::Archive)
        else {
            panic!("not recognised");
        };
        assert!(
            means.len() > 1,
            "`Emu68-pistorm.zip` is not one board's archive: {means:?}"
        );
    }

    /// The alpha line's classic archive has its own name, and it is a
    /// different one — which is the whole reason the line is a field.
    #[test]
    fn the_alpha_lines_classic_archive_is_recognised_too() {
        let CardRole::Emu68Archive { means } =
            role("Emu68-pistorm-classic.zip", FormatCategory::Archive)
        else {
            panic!("not recognised");
        };
        assert_eq!(
            means,
            vec![ArchiveNameMeans {
                variant: PistormVariant::Classic,
                line: Emu68Line::Alpha11,
            }]
        );
    }

    /// The commonest wrong download in the release, named for what it is
    /// rather than left in the "belongs on a volume" pile.
    #[test]
    fn the_raspi_archive_is_refused_for_what_it_is() {
        assert_eq!(
            role("Emu68-raspi.zip", FormatCategory::Archive),
            CardRole::NoPlaceOnACard {
                what: FormatCategory::Archive
            }
        );
    }

    #[test]
    fn a_rom_is_a_kickstart() {
        assert_eq!(
            role("Kickstart 3.1.rom", FormatCategory::Rom),
            CardRole::Kickstart
        );
    }

    /// Recognised, and explicitly not used: choosing between distributions is
    /// an Amiga-side selector, and G16's work.
    #[test]
    fn a_distro_config_is_recognised_and_named() {
        assert_eq!(
            role("config_caffeineos.txt", FormatCategory::Unknown),
            CardRole::DistroConfig {
                name: "caffeineos".into()
            }
        );
    }

    /// **The answer SD-1 owes most often.** A game, a disk, an OS CD — every
    /// one of them lives on an Amiga volume, and this card has none formatted.
    /// "Not yet, and here is why" rather than a shrug.
    #[test]
    fn amiga_content_is_told_it_needs_a_volume_this_card_has_not_got() {
        for (name, category) in [
            ("Turrican.lha", FormatCategory::Archive),
            ("workbench.adf", FormatCategory::FloppyImage),
            ("os32.iso", FormatCategory::OpticalImage),
            ("Games", FormatCategory::Directory),
            ("work.hdf", FormatCategory::HardDiskImage),
        ] {
            assert_eq!(
                role(name, category),
                CardRole::ForAnAmigaVolume { what: category },
                "{name}"
            );
        }
    }

    /// A 1541 disk near a PiStorm card is a mistake worth naming, not a
    /// "maybe later".
    #[test]
    fn a_commodore_disk_has_no_place_on_a_card() {
        assert_eq!(
            role("elite.d64", FormatCategory::Commodore8Bit),
            CardRole::NoPlaceOnACard {
                what: FormatCategory::Commodore8Bit
            }
        );
    }

    /// A zip that is not an Emu68 release is just an archive, and archives
    /// live on volumes.
    #[test]
    fn an_ordinary_archive_is_not_mistaken_for_the_release() {
        assert_eq!(
            role("holiday-photos.zip", FormatCategory::Archive),
            CardRole::ForAnAmigaVolume {
                what: FormatCategory::Archive
            }
        );
    }
}
