//! Distro profiles — which AmigaOS distributions the OS Builder knows about,
//! and what each one honestly requires of the user.
//!
//! **A profile is data, not code** (`ART-research-distro-profiles.md` §4). The
//! registry is a JSON file in this directory, compiled in with `include_str!`:
//! reviewable in a diff, shipped without a network, and unable to grow a code
//! path of its own.
//!
//! **The legal line, and it is not negotiable** (§2):
//!
//! - ART **never downloads a distro image**. No URL list, no fetch button. The
//!   `homepage` field is where the *user* goes; ART then accepts a local file.
//!   The same rule ART already applies to Kickstart ROMs.
//! - ART bundles no OS, no ROM and no distro content, in the repository or in a
//!   release.
//! - Adapting an image the user already has is fine — it is their copy, and ART
//!   is a tool operating on it.
//!
//! What each profile *says* about its licence is therefore part of the data,
//! and the screen leads with it: "you download this yourself", "you must own
//! AmigaOS 3.2", "built from your own licensed media".
//!
//! Nothing here builds a card yet. Every profile is registered `available:
//! false` — described, and shown as Coming Later rather than hidden (§96) —
//! because the adaptation checklist is blocked on inspecting a real
//! distribution's layout by hand, which the research document parks
//! deliberately rather than guesses at (§8.2).

use serde::{Deserialize, Serialize};

use crate::core::error::{CoreError, CoreResult};

/// What the user has to do about the licence before ART can help.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum LicenceModel {
    /// Distributed free by its makers, and carrying AmigaOS material they
    /// cannot license. ART does not police the user's copy and does not fetch
    /// it either. Both such projects ship the same sentence, and so does ART:
    /// if you paid for this, ask for your money back.
    FreeGrey,
    /// A commercial product built on an AmigaOS the user must own. ART prepares
    /// the card their product expects; it does not reproduce their layer.
    UserLicensed,
    /// Built by ART from the user's own licensed media, and shareable as a
    /// recipe — never as an image.
    ArtBaseline,
}

/// Where the material comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Acquisition {
    /// The user downloads it and points ART at the file.
    UserSuppliesImage,
    /// ART assembles it from the user's own media and packages.
    ArtBuilds,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ImageFormat {
    RawImg,
    SevenZipImg,
    BuildRecipe,
}

/// Which AmigaOS the profile is built on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum BaseOs {
    Os32,
    Os39,
    NoneDeclared,
}

impl BaseOs {
    /// The Kickstart family this base needs.
    ///
    /// **Not cosmetic.** CoffinOS warns that a Hyperion 3.1.4 or 3.2 Kickstart
    /// breaks it, and that warning generalises: a 3.9-era system wants a
    /// *classic* 3.1 ROM, and a 3.2 system wants a 3.2 ROM. Getting this wrong
    /// produces a card that fails at boot with nothing to explain it.
    pub fn rom_family(self) -> Option<&'static str> {
        match self {
            Self::Os32 => Some("3.2"),
            Self::Os39 => Some("3.1"),
            Self::NoneDeclared => None,
        }
    }
}

/// The Kickstart a profile needs, and what to call it on the card.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RomRequirement {
    /// `"3.1"` or `"3.2"` — matched against `core/rom`'s identification.
    pub family: String,
    /// The name the file takes on the FAT32 partition, written into
    /// `initramfs`.
    pub drop_name: String,
}

/// Where this profile sits on a multiboot card.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MultibootSlot {
    /// The `config_<name>.txt` this profile owns — the mechanism
    /// `core/pistorm`'s named sets already implement.
    pub config_set_name: String,
}

/// One distribution the OS Builder knows about.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DistroProfile {
    pub id: String,
    pub name: String,
    /// Where the *user* goes to get it. ART never follows this itself.
    pub homepage: String,
    pub licence_model: LicenceModel,
    pub acquisition: Acquisition,
    pub image_format: ImageFormat,
    pub min_card_gb: u32,
    pub base_os: BaseOs,
    pub rom_requirement: Option<RomRequirement>,
    /// Emu68 `cmdline.txt` tokens this profile wants, merged through the
    /// existing PiStorm core — never regenerated (§39, ART-004).
    pub default_cmdline_tokens: Vec<String>,
    pub multiboot: MultibootSlot,
    /// HstWB-style package references. `art-baseline` profiles only.
    pub packages: Vec<String>,
    /// i18n keys, not sentences: `core/` writes no user-facing English (§68).
    pub post_install_notes: Vec<String>,
    /// Whether ART can actually build this yet. `false` renders as Coming
    /// Later rather than vanishing (§96).
    pub available: bool,
}

#[derive(Debug, Clone, Deserialize)]
struct Registry {
    profiles: Vec<DistroProfile>,
}

/// The registry as it ships. Parsed once, on demand.
const REGISTRY_JSON: &str = include_str!("registry.json");

/// Every profile ART knows about.
pub fn profiles() -> CoreResult<Vec<DistroProfile>> {
    let registry: Registry = serde_json::from_str(REGISTRY_JSON)
        .map_err(|e| CoreError::InvalidInput(format!("the distro registry is malformed: {e}")))?;
    Ok(registry.profiles)
}

/// One profile by id.
pub fn profile(id: &str) -> CoreResult<DistroProfile> {
    profiles()?
        .into_iter()
        .find(|entry| entry.id == id)
        .ok_or_else(|| CoreError::InvalidInput(format!("there is no '{id}' profile")))
}

/// Why a card will not do for a profile.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", tag = "kind")]
pub enum CardProblem {
    /// The card is smaller than the profile needs.
    TooSmall { needs_gb: u32, has_gb: u32 },
}

/// Whether a card of this size can hold the profile.
///
/// Checked before anything is written, because the alternative is discovering
/// it two thirds of the way through a 17 GB copy.
pub fn check_card_size(profile: &DistroProfile, card_bytes: u64) -> Option<CardProblem> {
    let has_gb = (card_bytes / (1024 * 1024 * 1024)) as u32;
    (has_gb < profile.min_card_gb).then_some(CardProblem::TooSmall {
        needs_gb: profile.min_card_gb,
        has_gb,
    })
}

/// Whether a ROM's identified version belongs with this profile's base OS.
///
/// A **note**, in keeping with the rest of ART: the user may be doing something
/// deliberate. But it is a note worth making — a Hyperion 3.1.4 or 3.2 ROM
/// under a 3.9-era system is CoffinOS's own documented failure, and the symptom
/// is a card that simply does not boot.
///
/// `None` when there is nothing to say: no declared base, no requirement, or a
/// ROM ART did not recognise (whose version is `Custom`).
pub fn rom_family_matches(profile: &DistroProfile, rom_version: &str) -> Option<bool> {
    let wanted = profile.rom_requirement.as_ref()?.family.as_str();
    if rom_version == "Custom" || rom_version == "AROS" {
        return None;
    }
    // `3.1.4` is not `3.1` for this purpose — that is the whole of CoffinOS's
    // warning — so this is a prefix match up to the next dot rather than a
    // `starts_with`.
    let family =
        |version: &str| -> String { version.split('.').take(2).collect::<Vec<_>>().join(".") };
    Some(family(rom_version) == wanted && rom_version.matches('.').count() <= 1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::pistorm::options::MANAGED_TOKENS;

    #[test]
    fn the_registry_parses_and_is_not_empty() {
        let all = profiles().expect("the shipped registry must parse");
        assert!(!all.is_empty());
    }

    #[test]
    fn every_profile_has_its_own_id_and_its_own_config_set() {
        // Two profiles sharing a `config_<name>.txt` would silently overwrite
        // each other on a multiboot card.
        let all = profiles().unwrap();
        let mut ids: Vec<&str> = all.iter().map(|p| p.id.as_str()).collect();
        ids.sort_unstable();
        let count = ids.len();
        ids.dedup();
        assert_eq!(ids.len(), count, "two profiles share an id");

        let mut sets: Vec<&str> = all
            .iter()
            .map(|p| p.multiboot.config_set_name.as_str())
            .collect();
        sets.sort_unstable();
        let count = sets.len();
        sets.dedup();
        assert_eq!(sets.len(), count, "two profiles share a config set");
    }

    #[test]
    fn every_token_a_profile_names_is_one_emu68_has() {
        // The guarantee ART-090 bought, extended to the registry: a profile is
        // data a person edits, and a typo there would put a fictional token on
        // a real card. Names are checked, not values.
        let all = profiles().unwrap();
        for entry in &all {
            for token in &entry.default_cmdline_tokens {
                let name = token.split_once('=').map(|(k, _)| k).unwrap_or(token);
                assert!(
                    MANAGED_TOKENS.contains(&name),
                    "{}: '{name}' is not a token ART owns",
                    entry.id
                );
            }
        }
    }

    #[test]
    fn a_config_set_name_is_one_the_pistorm_core_would_accept() {
        // These become `config_<name>.txt` through `core/pistorm`, which
        // refuses anything but letters, digits, `-` and `_`.
        for entry in profiles().unwrap() {
            let name = &entry.multiboot.config_set_name;
            assert!(
                !name.is_empty()
                    && name.len() <= 32
                    && name
                        .chars()
                        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'),
                "{}: '{name}' is not a usable set name",
                entry.id
            );
        }
    }

    #[test]
    fn nothing_in_the_registry_is_a_download_link_for_art() {
        // The legal line, as a test. `homepage` is where the *user* goes; ART
        // must never grow a field that looks like something to fetch.
        for entry in profiles().unwrap() {
            assert!(
                entry.homepage.starts_with("https://"),
                "{}: homepage should be a plain https page",
                entry.id
            );
            assert!(
                !entry.homepage.ends_with(".img")
                    && !entry.homepage.ends_with(".zip")
                    && !entry.homepage.ends_with(".7z"),
                "{}: homepage points at an image, which ART must not fetch",
                entry.id
            );
        }
    }

    #[test]
    fn every_profile_says_which_kickstart_family_its_base_wants() {
        // The mismatch CoffinOS documents is a card that does not boot with
        // nothing to explain it, so the requirement is declared rather than
        // discovered.
        for entry in profiles().unwrap() {
            if let Some(family) = entry.base_os.rom_family() {
                let requirement = entry
                    .rom_requirement
                    .as_ref()
                    .unwrap_or_else(|| panic!("{}: a declared base needs a ROM family", entry.id));
                assert_eq!(requirement.family, family, "{}", entry.id);
                assert!(!requirement.drop_name.is_empty(), "{}", entry.id);
            }
        }
    }

    #[test]
    fn nothing_is_claimed_buildable_yet() {
        // The adaptation checklist is blocked on inspecting a real
        // distribution by hand (§8.2). Until then every profile is described
        // and none is offered — which is §96, not a placeholder.
        for entry in profiles().unwrap() {
            assert!(!entry.available, "{} claims to be buildable", entry.id);
        }
    }

    #[test]
    fn a_card_too_small_for_a_profile_is_caught_before_anything_is_written() {
        let caffeine = profile("caffeineos").unwrap();
        assert_eq!(
            check_card_size(&caffeine, 16 * 1024 * 1024 * 1024),
            Some(CardProblem::TooSmall {
                needs_gb: 32,
                has_gb: 16
            })
        );
        assert_eq!(check_card_size(&caffeine, 64 * 1024 * 1024 * 1024), None);
    }

    #[test]
    fn a_kickstart_from_the_wrong_family_is_noticed() {
        let os39 = profile("art-baseline-39").unwrap();
        assert_eq!(rom_family_matches(&os39, "3.1"), Some(true));
        // The exact trap CoffinOS documents: a Hyperion 3.1.4 ROM is not a
        // classic 3.1 ROM, whatever the first two numbers say.
        assert_eq!(rom_family_matches(&os39, "3.1.4"), Some(false));
        assert_eq!(rom_family_matches(&os39, "3.2"), Some(false));

        let os32 = profile("art-baseline-32").unwrap();
        assert_eq!(rom_family_matches(&os32, "3.2"), Some(true));
        assert_eq!(rom_family_matches(&os32, "3.1"), Some(false));
    }

    #[test]
    fn a_rom_art_does_not_recognise_gets_no_opinion() {
        let os39 = profile("art-baseline-39").unwrap();
        assert_eq!(rom_family_matches(&os39, "Custom"), None);
        assert_eq!(rom_family_matches(&os39, "AROS"), None);
    }

    #[test]
    fn a_profile_that_is_not_there_is_refused_by_name() {
        let err = profile("pimiga").unwrap_err();
        assert!(err.to_string().contains("pimiga"), "{err}");
    }
}
