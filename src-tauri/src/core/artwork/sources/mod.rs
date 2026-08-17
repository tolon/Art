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

    /// Index files to fetch before matching can start. Empty when the source
    /// needs none — whdload.de builds its path from the title alone.
    fn index_paths(&self) -> Vec<(ArtKind, String)>;

    /// Parse one fetched index into `into`.
    fn absorb_index(&self, kind: ArtKind, bytes: &[u8], into: &mut SourceIndex) -> CoreResult<()>;

    /// The repository path holding this title's picture, already encoded so it
    /// passes `validate_fetch_path`.
    fn locate(&self, index: &SourceIndex, title: &str, kind: ArtKind) -> Option<String>;
}
