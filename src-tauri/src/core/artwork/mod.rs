//! Artwork for catalogue titles, fetched from configured sources.
//!
//! This module declares what it needs and opens no connection: every fetch goes
//! through a `&dyn MirrorClient` supplied by the caller, so the whole module is
//! testable with a fake and the test suite touches no network.
//!
//! It reads `core::gameindex::record` types. It must never be imported *by*
//! `core::gameindex` — joining a record to its artwork is a command-layer job.

pub mod cache;
pub mod config;
pub mod encode;
pub mod enrich;
pub mod key;
pub mod local;
pub mod sources;

use serde::{Deserialize, Serialize};

/// The kinds of picture the configured sources publish.
///
/// `Boxart`, `Snap`, `Title` and `Logo` are libretro's four directories,
/// measured against the live repository. `Icon` is whdload.de's Amiga icon,
/// which is not box art and is not presented as one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ArtKind {
    Boxart,
    Snap,
    Title,
    Logo,
    Icon,
}

impl ArtKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Boxart => "boxart",
            Self::Snap => "snap",
            Self::Title => "title",
            Self::Logo => "logo",
            Self::Icon => "icon",
        }
    }

    /// Every kind, in the order a screen should prefer them.
    pub const ALL: [ArtKind; 5] = [
        ArtKind::Boxart,
        ArtKind::Title,
        ArtKind::Snap,
        ArtKind::Logo,
        ArtKind::Icon,
    ];
}

/// What a title gained: which picture, from whom, and where it landed.
///
/// `file` is **cache-relative**, never absolute and never a URL, so moving the
/// cache directory does not invalidate what points into it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArtRef {
    pub kind: ArtKind,
    /// The `SourceId` string that provided it.
    pub source: String,
    pub file: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn art_kind_serialises_kebab_case_for_the_frontend() {
        let json = serde_json::to_string(&ArtKind::Boxart).unwrap();
        assert_eq!(json, "\"boxart\"");
    }

    #[test]
    fn every_kind_is_listed_in_all() {
        // A kind added to the enum but not to ALL would silently never be
        // fetched. There is no derive that catches that, so assert the count.
        assert_eq!(ArtKind::ALL.len(), 5);
        for kind in ArtKind::ALL {
            assert!(!kind.as_str().is_empty());
        }
    }

    /// `ALL` must be a set, not a list that quietly repeats one kind — a
    /// duplicate would make the run ask the same question twice.
    #[test]
    fn all_holds_each_kind_once() {
        let mut seen: Vec<ArtKind> = ArtKind::ALL.to_vec();
        seen.sort();
        seen.dedup();
        assert_eq!(seen.len(), ArtKind::ALL.len());
    }
}
