//! Software Sources Engine — the catalog side of spec §41.5.
//!
//! > *Aminet is the store. ART is the courier and the customs officer.*
//!
//! This module knows how to **find** software in a repository. It does not know
//! how to install it: installation reuses the existing, tested LHA/HDF
//! workflows (§41.5.3). Keeping that line sharp is what stops a second,
//! half-tested extraction path from growing here.
//!
//! ## Why the traits
//!
//! A repository is by definition a network resource with a local database, and
//! `core/` may have neither (the core independence rule). So the core declares
//! what it needs and the shell provides it:
//!
//! - [`mirror::MirrorClient`] — fetching bytes from a mirror
//! - [`catalog::CatalogStore`] — where the synced catalog lives
//!
//! Both have in-memory test doubles, so the whole engine — including failover,
//! resume and the trust pipeline — is exercised offline with no socket open.
//! See `docs/design-software-sources.md`.
//!
//! ## Everything here is a claim
//!
//! Index columns are machine-generated and trustworthy; readme fields are
//! hand-written prose from 1994. [`Claim`] carries that difference through to
//! the UI so ART never presents a guess as a fact (§14, §34).

pub mod bundle;
pub mod cache;
pub mod catalog;
pub mod fetch;
pub mod index;
pub mod install;
pub mod installed;
pub mod library;
pub mod mirror;
pub mod readme;
pub mod sync;
mod text;

pub use readme::ReadmeFields;

use serde::{Deserialize, Serialize};

use crate::core::workflow::types::Confidence;

/// The provider id used for Aminet. The engine is provider-modular (§41.5.0);
/// this is simply the first one.
pub const PROVIDER_AMINET: &str = "aminet";

/// Identity of a package inside a repository, stable across syncs.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PackageRef {
    /// Provider id, e.g. `"aminet"`. Reserved for OS4Depot and friends.
    pub provider: String,
    /// Repository-relative path, e.g. `"util/libs/AmiSSL-5.5.lha"`.
    ///
    /// Always validated on the way in: forward slashes only, no `..`, no drive
    /// letters, no control characters. An entry whose path fails is dropped
    /// from the catalog rather than sanitised — see [`index`].
    pub path: String,
}

impl PackageRef {
    pub fn new(provider: impl Into<String>, path: impl Into<String>) -> Self {
        Self {
            provider: provider.into(),
            path: path.into(),
        }
    }
}

/// Where a piece of information came from.
///
/// An enum rather than free text so the confidence mapping is exhaustive and
/// testable, and so the UI can phrase it in the user's language.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClaimSource {
    /// A column of the repository's machine-generated index.
    IndexColumn,
    /// A labelled field in the package's readme — hand-written prose.
    ReadmeField,
    /// Inferred from the file name alone.
    Filename,
}

impl ClaimSource {
    /// How to describe this source to a user.
    pub fn describe(self) -> &'static str {
        match self {
            Self::IndexColumn => "repository index",
            Self::ReadmeField => "readme field",
            Self::Filename => "file name",
        }
    }
}

/// A fact ART believes, with how strongly and on what grounds.
///
/// There is deliberately no "unknown" claim: a field ART never found produces
/// no `Claim` at all. `Confidence::Unknown` means *looked and could not tell*,
/// which is a different statement from *not present*.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Claim<T> {
    pub value: T,
    pub confidence: Confidence,
    pub source: ClaimSource,
}

impl<T> Claim<T> {
    pub fn new(value: T, confidence: Confidence, source: ClaimSource) -> Self {
        Self {
            value,
            confidence,
            source,
        }
    }

    /// A column of the repository index: machine-generated, so `High`.
    pub fn from_index(value: T) -> Self {
        Self::new(value, Confidence::High, ClaimSource::IndexColumn)
    }

    /// A clean, unambiguous readme field: hand-written, so no better than
    /// `Medium`.
    pub fn from_readme(value: T) -> Self {
        Self::new(value, Confidence::Medium, ClaimSource::ReadmeField)
    }

    /// A readme field that appeared more than once with different values, or
    /// otherwise could not be pinned down.
    pub fn from_ambiguous_readme(value: T) -> Self {
        Self::new(value, Confidence::Low, ClaimSource::ReadmeField)
    }

    /// Inferred from the file name — always `Low`.
    pub fn from_filename(value: T) -> Self {
        Self::new(value, Confidence::Low, ClaimSource::Filename)
    }
}

/// One package as ART knows it: index facts, optionally enriched with readme
/// fields once the user asks for a package in particular.
///
/// Readme fields start empty. Aminet holds tens of thousands of packages and
/// ART does not bulk-prefetch the repository (§41.5.5) — a readme is fetched
/// when someone wants to read it, and [`apply_readme`] folds it in here.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PackageMeta {
    pub reference: PackageRef,
    /// File name only: `"AmiSSL-5.5.lha"`.
    pub name: String,
    /// Directory part: `"util/libs"`.
    pub directory: String,
    /// Size as *claimed by the index*.
    ///
    /// This is never used as an allocation size. The download pipeline compares
    /// against it and aborts on a mismatch; nothing is reserved from it.
    pub size_bytes: u64,
    /// Aminet's index states age in whole weeks at the time the index was
    /// generated, not an upload date. Smaller is newer. `None` when the column
    /// was missing or unparseable.
    pub age_weeks: Option<u32>,
    /// The one-line description from the index.
    pub short: String,

    // ---- filled in from the readme, when one has been fetched ----
    pub version: Option<Claim<String>>,
    pub requires: Vec<Claim<String>>,
    pub author: Option<Claim<String>>,
    pub distribution: Option<Claim<String>>,
}

impl PackageMeta {
    /// Whether the age column saturated.
    ///
    /// Aminet caps it, so the oldest entries all report the same number. A UI
    /// that renders that as a precise date would be inventing one — this is
    /// how it knows to say "or older" instead.
    pub fn age_is_capped(&self) -> bool {
        self.age_weeks == Some(index::AGE_CAP_WEEKS)
    }

    /// The readme that accompanies this package in the repository.
    ///
    /// Aminet stores `util/libs/Foo.lha` alongside `util/libs/Foo.readme`.
    pub fn readme_path(&self) -> String {
        match self.reference.path.rfind('.') {
            // Only replace an extension that belongs to the file name, never a
            // dot that appears in a directory component.
            Some(dot) if dot > self.reference.path.rfind('/').map_or(0, |s| s + 1) => {
                format!("{}.readme", &self.reference.path[..dot])
            }
            _ => format!("{}.readme", self.reference.path),
        }
    }
}

/// The most requires-entries ART will keep from one readme.
///
/// Free-text input needs a ceiling; a readme claiming four hundred
/// dependencies is malformed, not informative.
const MAX_REQUIRES: usize = 16;

/// Fold readme fields into a package's metadata, assigning confidence by
/// source (§41.5.2: readme fields are stored *with* confidence levels).
///
/// A filename-derived version already present is only replaced by a readme
/// version, never the other way round — the readme is the better source even
/// when it is ambiguous.
pub fn apply_readme(meta: &mut PackageMeta, fields: &ReadmeFields) {
    if let Some(ref version) = fields.version {
        meta.version = Some(if fields.is_ambiguous("version") {
            Claim::from_ambiguous_readme(version.clone())
        } else {
            Claim::from_readme(version.clone())
        });
    }

    if let Some(ref author) = fields.author {
        meta.author = Some(Claim::from_readme(author.clone()));
    }

    if let Some(ref distribution) = fields.distribution {
        meta.distribution = Some(Claim::from_readme(distribution.clone()));
    }

    if !fields.requires.is_empty() {
        meta.requires = fields
            .requires
            .iter()
            .take(MAX_REQUIRES)
            .cloned()
            .map(Claim::from_readme)
            .collect();
    }

    // The index short description stays authoritative; a readme `Short:` only
    // fills a gap.
    if meta.short.is_empty() {
        if let Some(ref short) = fields.short {
            meta.short = short.clone();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn meta(path: &str) -> PackageMeta {
        PackageMeta {
            reference: PackageRef::new(PROVIDER_AMINET, path),
            name: path.rsplit('/').next().unwrap().to_string(),
            directory: path
                .rsplit_once('/')
                .map_or(String::new(), |(d, _)| d.into()),
            size_bytes: 1024,
            age_weeks: Some(4),
            short: "A thing".into(),
            version: None,
            requires: Vec::new(),
            author: None,
            distribution: None,
        }
    }

    #[test]
    fn readme_path_swaps_the_archive_extension() {
        assert_eq!(
            meta("util/libs/AmiSSL-5.5.lha").readme_path(),
            "util/libs/AmiSSL-5.5.readme"
        );
    }

    /// A dot in a directory must not be mistaken for the file's extension.
    #[test]
    fn readme_path_ignores_dots_in_directories() {
        assert_eq!(
            meta("util/v1.2/README").readme_path(),
            "util/v1.2/README.readme"
        );
    }

    #[test]
    fn readme_fields_land_with_medium_confidence() {
        let mut m = meta("util/libs/AmiSSL-5.5.lha");
        let fields = ReadmeFields {
            version: Some("5.5".into()),
            author: Some("Jens Maus".into()),
            requires: vec!["AmigaOS 3.0".into()],
            ..Default::default()
        };

        apply_readme(&mut m, &fields);

        let version = m.version.expect("version claim");
        assert_eq!(version.value, "5.5");
        assert_eq!(version.confidence, Confidence::Medium);
        assert_eq!(version.source, ClaimSource::ReadmeField);
        assert_eq!(m.requires.len(), 1);
        assert_eq!(m.author.unwrap().confidence, Confidence::Medium);
    }

    /// §41.5.2: a field ART cannot pin down is kept, but demoted — never
    /// presented at the same strength as a clean one.
    #[test]
    fn an_ambiguous_readme_version_is_demoted_to_low() {
        let mut m = meta("util/libs/Foo.lha");
        let fields = ReadmeFields {
            version: Some("2.0".into()),
            ambiguous: vec!["version".into()],
            ..Default::default()
        };

        apply_readme(&mut m, &fields);
        assert_eq!(m.version.unwrap().confidence, Confidence::Low);
    }

    /// A filename guess must yield to the readme, which is the better source
    /// even when it is messy.
    #[test]
    fn a_readme_version_replaces_a_filename_guess() {
        let mut m = meta("util/libs/Foo-1.0.lha");
        m.version = Some(Claim::from_filename("1.0".into()));

        apply_readme(
            &mut m,
            &ReadmeFields {
                version: Some("1.1".into()),
                ..Default::default()
            },
        );

        let version = m.version.unwrap();
        assert_eq!(version.value, "1.1");
        assert_eq!(version.source, ClaimSource::ReadmeField);
    }

    /// A readme with no version at all must not erase what the filename told us.
    #[test]
    fn an_empty_readme_leaves_a_filename_guess_alone() {
        let mut m = meta("util/libs/Foo-1.0.lha");
        m.version = Some(Claim::from_filename("1.0".into()));

        apply_readme(&mut m, &ReadmeFields::default());

        assert_eq!(m.version.unwrap().source, ClaimSource::Filename);
    }

    /// Free text needs a ceiling: a readme claiming hundreds of dependencies is
    /// malformed input, not information.
    #[test]
    fn requires_is_bounded() {
        let mut m = meta("util/libs/Foo.lha");
        let fields = ReadmeFields {
            requires: (0..500).map(|i| format!("dep{i}")).collect(),
            ..Default::default()
        };

        apply_readme(&mut m, &fields);
        assert_eq!(m.requires.len(), MAX_REQUIRES);
    }

    /// The index description is machine-generated; a readme only fills a gap.
    #[test]
    fn the_index_description_outranks_the_readme_short_field() {
        let mut m = meta("util/libs/Foo.lha");
        apply_readme(
            &mut m,
            &ReadmeFields {
                short: Some("From the readme".into()),
                ..Default::default()
            },
        );
        assert_eq!(m.short, "A thing");

        let mut empty = meta("util/libs/Foo.lha");
        empty.short = String::new();
        apply_readme(
            &mut empty,
            &ReadmeFields {
                short: Some("From the readme".into()),
                ..Default::default()
            },
        );
        assert_eq!(empty.short, "From the readme");
    }

    #[test]
    fn package_metadata_survives_a_round_trip() {
        let mut m = meta("util/libs/AmiSSL-5.5.lha");
        m.version = Some(Claim::from_readme("5.5".into()));

        let json = serde_json::to_string(&m).unwrap();
        let back: PackageMeta = serde_json::from_str(&json).unwrap();
        assert_eq!(back, m);
    }
}
