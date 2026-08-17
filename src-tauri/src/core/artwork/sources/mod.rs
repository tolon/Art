//! What a source is, and what it is not.
//!
//! A source knows how to find a title's picture *inside its own repository*. It
//! never builds a URL: `Mirror::url_for` does that, once, at the last point
//! before bytes leave the machine. It never opens a connection either — the
//! caller fetches and hands the bytes back.
//!
//! Sources are code rather than data because their index formats differ. The
//! configurable part is per source and small: on/off, and the mirror base
//! (spec §5).

pub mod libretro;
pub mod whdload_de;

use std::collections::BTreeMap;

use crate::core::artwork::ArtKind;
use crate::core::error::CoreResult;

/// What a source parsed out of its index files: normalised title -> repository
/// path, one map per kind.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct SourceIndex {
    pub by_kind: BTreeMap<ArtKind, BTreeMap<String, String>>,
}

pub trait ArtSource: Send + Sync {
    /// Stable identifier, stored in the cache and in settings. Never localised.
    fn id(&self) -> &'static str;

    /// The kinds this source can supply.
    fn kinds(&self) -> &'static [ArtKind];

    /// How fast this source may be asked, per second.
    ///
    /// Politeness is a property of **the host**, not of ART, and one constant
    /// for every source was wrong in both directions: whdload.de is run by
    /// volunteers on a small server, while libretro's pictures come off
    /// GitHub's CDN, which serves orders of magnitude more than this all day.
    /// Holding the CDN to a volunteer's rate turned a one-minute job into a
    /// forty-minute one.
    fn requests_per_second(&self) -> u32;

    /// Round one: documents to fetch that are not themselves indexes, but say
    /// where the indexes are. Empty when the source needs none.
    ///
    /// This round exists because libretro's index is reached in two hops — the
    /// root tree names each subdirectory's sha, and only then can a subdirectory
    /// be asked for. Tagging the root tree as though it were a boxart index
    /// would be a lie the run would have to keep track of.
    fn manifest_paths(&self) -> Vec<String>;

    /// Round two: given whatever `manifest_paths` fetched, in the same order,
    /// the index files to fetch and which kind each one describes.
    ///
    /// A source with no manifests receives an empty slice and answers from
    /// nothing, or answers nothing at all.
    fn index_paths(&self, manifests: &[Vec<u8>]) -> CoreResult<Vec<(ArtKind, String)>>;

    /// Parse one fetched index into `into`.
    fn absorb_index(&self, kind: ArtKind, bytes: &[u8], into: &mut SourceIndex) -> CoreResult<()>;

    /// The repository path holding this title's picture, already encoded so it
    /// passes `validate_fetch_path`.
    fn locate(&self, index: &SourceIndex, title: &str, kind: ArtKind) -> Option<String>;
}
