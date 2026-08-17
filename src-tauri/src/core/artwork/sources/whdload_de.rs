//! whdload.de — the source with an exact key.
//!
//! Package pages carry a predictable layout, confirmed against
//! `https://www.whdload.de/games/Moonstone.html`:
//!
//! | what    | path                    |
//! |---------|-------------------------|
//! | package | `games/<Name>.lha`      |
//! | icon    | `games/ico/<Name>.png`  |
//!
//! `<Name>` is the WHDLoad package name, which ART already holds — 1681 of one
//! user's 1700 records were titled from their slave. There is no matching
//! problem here: the key is exact or the record has no key.
//!
//! What the site does not give is equally clear. Its metadata lives on HTML
//! pages, may not be scraped (§41.5.3), and is redundant anyway — the slave
//! stated it offline. The Hall of Light and Lemon Amiga cross-reference ids
//! found only on those pages would make matching exact everywhere; they are not
//! obtainable within the rules and this source does without them.

use crate::core::artwork::encode::path_segment;
use crate::core::artwork::sources::{ArtSource, SourceIndex};
use crate::core::artwork::ArtKind;
use crate::core::error::CoreResult;

const KINDS: [ArtKind; 1] = [ArtKind::Icon];

#[derive(Debug, Default, Clone, Copy)]
pub struct WhdloadDe;

impl ArtSource for WhdloadDe {
    fn id(&self) -> &'static str {
        "whdload-de"
    }

    fn kinds(&self) -> &'static [ArtKind] {
        &KINDS
    }

    fn manifest_paths(&self) -> Vec<String> {
        Vec::new()
    }

    fn index_paths(&self, _manifests: &[Vec<u8>]) -> CoreResult<Vec<(ArtKind, String)>> {
        Ok(Vec::new())
    }

    fn absorb_index(
        &self,
        _kind: ArtKind,
        _bytes: &[u8],
        _into: &mut SourceIndex,
    ) -> CoreResult<()> {
        Ok(())
    }

    fn locate(&self, _index: &SourceIndex, title: &str, kind: ArtKind) -> Option<String> {
        if kind != ArtKind::Icon {
            return None;
        }
        let trimmed = title.trim();
        if trimmed.is_empty() {
            return None;
        }
        Some(format!("games/ico/{}.png", path_segment(trimmed)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_needs_no_index() {
        assert!(WhdloadDe.manifest_paths().is_empty());
        assert!(WhdloadDe.index_paths(&[]).unwrap().is_empty());
    }

    /// The key is the package name ART already read from the slave, so there is
    /// no matching problem here at all.
    #[test]
    fn the_path_is_built_from_the_title_alone() {
        let empty = SourceIndex::default();
        assert_eq!(
            WhdloadDe.locate(&empty, "Moonstone", ArtKind::Icon),
            Some("games/ico/Moonstone.png".to_string())
        );
    }

    #[test]
    fn a_space_in_the_name_is_encoded_not_passed_through() {
        let empty = SourceIndex::default();
        let path = WhdloadDe
            .locate(&empty, "Cannon Fodder", ArtKind::Icon)
            .unwrap();
        assert_eq!(path, "games/ico/Cannon%20Fodder.png");
        assert!(!path.contains(' '));
    }

    #[test]
    fn it_offers_nothing_but_an_icon() {
        assert_eq!(WhdloadDe.kinds(), &[ArtKind::Icon]);
        let empty = SourceIndex::default();
        assert_eq!(WhdloadDe.locate(&empty, "Moonstone", ArtKind::Boxart), None);
    }

    #[test]
    fn an_empty_title_locates_nothing() {
        let empty = SourceIndex::default();
        assert_eq!(WhdloadDe.locate(&empty, "  ", ArtKind::Icon), None);
    }

    /// Whatever the title contains, the path handed on must survive the same
    /// validator every other fetch goes through.
    #[test]
    fn a_hostile_title_still_produces_a_path_the_validator_accepts() {
        let empty = SourceIndex::default();
        let path = WhdloadDe
            .locate(&empty, "../../etc/passwd", ArtKind::Icon)
            .unwrap();
        crate::core::sources::mirror::Mirror::new("t", "https://www.whdload.de/")
            .unwrap()
            .url_for(&path)
            .expect("the encoded path must pass validate_fetch_path");
    }
}
