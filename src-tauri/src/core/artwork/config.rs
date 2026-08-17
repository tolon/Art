//! Which sources ART ships, and the two things a user may change about them.
//!
//! Source *types* are code — their index formats differ and are not expressible
//! as data. What is configurable is small and deliberate (spec §5): **enabled**,
//! which every source ships as, and the **mirror bases**, validated by
//! `Mirror::new`.
//!
//! A user may not define a new source from a URL template. That would restore
//! arbitrary-URL fetching and void the guarantee in `core/sources/mirror.rs`
//! that no function anywhere fetches a caller-supplied URL. Adding a source type
//! is a code change, and this project is open source precisely so that stays
//! possible.
//!
//! Two bases rather than one, because libretro's index and its images are not
//! on the same host: the git tree comes from `api.github.com` and the pictures
//! from `raw.githubusercontent.com`. A source whose pictures sit under the same
//! base as everything else leaves `image_base` empty and `image_mirror` falls
//! back to the index one.

use serde::{Deserialize, Serialize};

use crate::core::artwork::sources::{libretro::Libretro, whdload_de::WhdloadDe, ArtSource};
use crate::core::error::CoreResult;
use crate::core::sources::mirror::Mirror;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfiguredSource {
    pub id: String,
    pub enabled: bool,
    /// Where index files are fetched from.
    pub mirror_base: String,
    /// Where pictures are fetched from. Empty means "the same place".
    #[serde(default)]
    pub image_base: String,
}

/// What ART ships with.
///
/// Both enabled: the project's position is that an absent licence is not a
/// blocker for forty-year-old game and demo material, while an absent endpoint
/// is. Enabled does **not** mean fetched automatically — nothing reaches the
/// network until the user starts the job.
pub fn shipped_defaults() -> Vec<ConfiguredSource> {
    vec![
        ConfiguredSource {
            id: "libretro".into(),
            enabled: true,
            mirror_base: "https://api.github.com/".into(),
            image_base:
                "https://raw.githubusercontent.com/libretro-thumbnails/Commodore_-_Amiga/master/"
                    .into(),
        },
        ConfiguredSource {
            id: "whdload-de".into(),
            enabled: true,
            mirror_base: "https://www.whdload.de/".into(),
            image_base: String::new(),
        },
    ]
}

pub fn source_for(id: &str) -> Option<Box<dyn ArtSource>> {
    match id {
        "libretro" => Some(Box::new(Libretro)),
        "whdload-de" => Some(Box::new(WhdloadDe)),
        _ => None,
    }
}

/// The mirror index files are fetched from.
pub fn index_mirror(configured: &ConfiguredSource) -> CoreResult<Mirror> {
    Mirror::new(configured.id.clone(), &configured.mirror_base)
}

/// The mirror pictures are fetched from, which is the index one unless the
/// source said otherwise.
pub fn image_mirror(configured: &ConfiguredSource) -> CoreResult<Mirror> {
    let base = if configured.image_base.trim().is_empty() {
        &configured.mirror_base
    } else {
        &configured.image_base
    };
    Mirror::new(configured.id.clone(), base)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The project's decision, recorded as a test so a later change is
    /// deliberate rather than accidental: every source ships enabled.
    #[test]
    fn every_shipped_source_is_enabled_by_default() {
        let defaults = shipped_defaults();
        assert!(!defaults.is_empty());
        assert!(defaults.iter().all(|source| source.enabled));
    }

    #[test]
    fn every_shipped_default_names_a_source_that_exists() {
        for configured in shipped_defaults() {
            assert!(
                source_for(&configured.id).is_some(),
                "no ArtSource for '{}'",
                configured.id
            );
        }
    }

    /// The bases are what the user may edit, so they must survive Mirror's
    /// validation as shipped.
    #[test]
    fn every_shipped_base_is_a_valid_mirror() {
        for configured in shipped_defaults() {
            index_mirror(&configured).expect(&configured.id);
            image_mirror(&configured).expect(&configured.id);
        }
    }

    /// libretro's pictures are on a different host from its index. Conflating
    /// them would fetch every image from the API host and 404 every time.
    #[test]
    fn libretro_fetches_its_pictures_from_a_different_host_than_its_index() {
        let libretro = shipped_defaults()
            .into_iter()
            .find(|source| source.id == "libretro")
            .unwrap();
        assert_ne!(
            index_mirror(&libretro).unwrap().base_url(),
            image_mirror(&libretro).unwrap().base_url()
        );
    }

    /// A source with nothing to say about images uses the one base it has.
    #[test]
    fn an_empty_image_base_falls_back_to_the_index_base() {
        let whdload = shipped_defaults()
            .into_iter()
            .find(|source| source.id == "whdload-de")
            .unwrap();
        assert_eq!(
            image_mirror(&whdload).unwrap().base_url(),
            index_mirror(&whdload).unwrap().base_url()
        );
    }

    #[test]
    fn a_hostile_mirror_base_is_refused() {
        let bad = ConfiguredSource {
            id: "libretro".into(),
            enabled: true,
            mirror_base: "file:///C:/Windows".into(),
            image_base: String::new(),
        };
        assert!(index_mirror(&bad).is_err());
    }

    /// A settings file written by an older ART has no image_base at all. It
    /// must load rather than being rejected wholesale.
    #[test]
    fn a_configuration_without_an_image_base_still_deserialises() {
        let json = r#"{"id":"whdload-de","enabled":true,"mirrorBase":"https://www.whdload.de/"}"#;
        let parsed: ConfiguredSource = serde_json::from_str(json).unwrap();
        assert_eq!(parsed.image_base, "");
        assert!(image_mirror(&parsed).is_ok());
    }
}
