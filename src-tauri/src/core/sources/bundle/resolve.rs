//! An entry, turned into the `(PackageMeta, mirrors)` pair
//! `fetch::fetch_package` already takes.
//!
//! Nothing here opens a socket, and nothing here accepts a URL: a
//! `Mirror` arrives already configured and validated by `Mirror::new`.

use super::{BundleEntry, EntrySource};
use crate::core::sources::mirror::Mirror;
use crate::core::sources::{PackageMeta, PackageRef, PROVIDER_AMINET};

pub enum Resolution {
    // `Box`ed: `PackageMeta` next to `Refused`'s bare `String` pushed this
    // enum past clippy's `large_enum_variant` threshold — the same shape as
    // `commands/osinstall.rs::PlanResult`.
    Fetchable {
        meta: Box<PackageMeta>,
        mirrors: Vec<Mirror>,
    },
    /// ART will not fetch this, and the sentence says why. English, from
    /// `core/` (ART-060) — the screen translates the *kind*, and shows this
    /// after it.
    Refused { why: String },
}

fn meta_for(entry: &BundleEntry, provider: &str, path: &str) -> PackageMeta {
    PackageMeta {
        reference: PackageRef {
            provider: provider.into(),
            path: path.into(),
        },
        name: path.rsplit('/').next().unwrap_or(path).to_string(),
        directory: path
            .rsplit_once('/')
            .map(|(d, _)| d.to_string())
            .unwrap_or_default(),
        // Nothing claimed a size. `check_size` treats 0 as "no claim to
        // compare against" and still refuses an empty file.
        size_bytes: 0,
        age_weeks: None,
        short: entry.name.clone(),
        version: None,
        requires: Vec::new(),
        author: None,
        distribution: None,
    }
}

/// An empty, whitespace-only, or directory-only path means the source
/// document gave a mirror (or Aminet) but no actual *file* to fetch from it —
/// a catalogue gap, not something `resolve` can paper over with an empty
/// `PackageMeta::name`. `"downloads/"` fails this the same way `""` and
/// `"   "` do: `meta_for`'s `rsplit('/')` would compute an empty name from
/// it, exactly the nameless download the `ibrowse` guard exists to prevent.
fn path_names_no_file(path: &str) -> bool {
    let trimmed = path.trim();
    trimmed.is_empty() || trimmed.ends_with('/')
}

fn refused_for_missing_path(entry: &BundleEntry) -> Resolution {
    Resolution::Refused {
        why: format!(
            "'{}' has a source but its catalogue entry carries no file path",
            entry.id
        ),
    }
}

pub fn resolve(
    entry: &BundleEntry,
    aminet: &[Mirror],
    configured: &[(String, Mirror)],
) -> Resolution {
    match &entry.source {
        EntrySource::Aminet { path } => {
            if path_names_no_file(path) {
                return refused_for_missing_path(entry);
            }
            Resolution::Fetchable {
                meta: Box::new(meta_for(entry, PROVIDER_AMINET, path)),
                mirrors: aminet.to_vec(),
            }
        }
        // Naming what ART cannot do, not what would fix it: nothing in
        // `resolve` ever consults `core/sources/catalog` (the synced Aminet
        // index) and `DownloadContext` carries no catalogue at all, so a
        // user told to "sync Aminet first" would sync and retry into the
        // identical refusal — an instruction ART cannot honour is worse than
        // none (CLAUDE.md: "a refusal must be actionable").
        EntrySource::AminetSearch { query } => Resolution::Refused {
            why: format!(
                "'{}' names 'latest version' ('{query}'), and ART cannot resolve that yet",
                entry.id
            ),
        },
        EntrySource::GithubRelease { repo, asset } => Resolution::Refused {
            why: format!("'{repo}' release asset '{asset}' needs a configured GitHub mirror"),
        },
        EntrySource::Mirror { mirror, path } => {
            if path_names_no_file(path) {
                return refused_for_missing_path(entry);
            }
            match configured.iter().find(|(name, _)| name == mirror) {
                Some((_, m)) => Resolution::Fetchable {
                    meta: Box::new(meta_for(entry, mirror, path)),
                    mirrors: vec![m.clone()],
                },
                None => Resolution::Refused {
                    why: format!("no mirror named '{mirror}' is configured"),
                },
            }
        }
        EntrySource::UserSupplied { why } => Resolution::Refused { why: why.clone() },
    }
}

#[cfg(test)]
mod tests {
    use crate::core::sources::bundle::{BundleEntry, EntrySource};
    use crate::core::sources::mirror::Mirror;

    fn entry(id: &str, source: EntrySource) -> BundleEntry {
        BundleEntry {
            id: id.into(),
            name: id.into(),
            source,
            order: 1,
            exclusive_group: None,
            requires: Vec::new(),
            permission: None,
        }
    }

    fn aminet() -> Vec<Mirror> {
        vec![Mirror::new("Test", "https://aminet.invalid/").unwrap()]
    }

    #[test]
    fn an_aminet_entry_resolves_to_its_repository_path() {
        let e = entry(
            "pfs3",
            EntrySource::Aminet {
                path: "disk/misc/PFS3_53".into(),
            },
        );
        match super::resolve(&e, &aminet(), &[]) {
            super::Resolution::Fetchable { meta, mirrors } => {
                assert_eq!(
                    meta.reference.provider,
                    crate::core::sources::PROVIDER_AMINET
                );
                assert_eq!(meta.reference.path, "disk/misc/PFS3_53");
                assert_eq!(mirrors.len(), 1);
            }
            super::Resolution::Refused { why } => panic!("refused: {why}"),
        }
    }

    #[test]
    fn a_user_supplied_entry_is_refused_with_the_reason_it_declares() {
        let e = entry(
            "whdloadwrapper",
            EntrySource::UserSupplied {
                why: "its address is a search form".into(),
            },
        );
        match super::resolve(&e, &aminet(), &[]) {
            super::Resolution::Refused { why } => {
                assert!(why.contains("search form"), "got: {why}")
            }
            super::Resolution::Fetchable { .. } => panic!("ART cannot fetch this"),
        }
    }

    #[test]
    fn a_mirror_entry_naming_no_configured_mirror_is_refused_by_name() {
        let e = entry(
            "setpatch",
            EntrySource::Mirror {
                mirror: "cloanto-cdn".into(),
                path: "pub/amiga/SetPatch-44-38.lha".into(),
            },
        );
        match super::resolve(&e, &aminet(), &[]) {
            super::Resolution::Refused { why } => {
                assert!(why.contains("cloanto-cdn"), "got: {why}")
            }
            super::Resolution::Fetchable { .. } => panic!("nothing is configured for it"),
        }
    }

    #[test]
    fn a_mirror_entry_resolves_against_the_mirror_configured_for_it() {
        let cloanto = Mirror::new("Cloanto", "https://cdn.invalid/").unwrap();
        let e = entry(
            "setpatch",
            EntrySource::Mirror {
                mirror: "cloanto-cdn".into(),
                path: "pub/amiga/SetPatch-44-38.lha".into(),
            },
        );
        match super::resolve(&e, &aminet(), &[("cloanto-cdn".to_string(), cloanto)]) {
            super::Resolution::Fetchable { meta, mirrors } => {
                assert_eq!(meta.reference.path, "pub/amiga/SetPatch-44-38.lha");
                assert_eq!(mirrors.len(), 1);
            }
            super::Resolution::Refused { why } => panic!("refused: {why}"),
        }
    }

    /// Task 2's deferred Minor: `catalogue/ibrowse.json` names a mirror but
    /// gives no file path at all. Resolving that anyway would hand
    /// `fetch_package` a `PackageMeta` with an empty `name` — a nameless
    /// download rather than a refusal a human can act on.
    #[test]
    fn a_mirror_entry_with_an_empty_path_is_refused_by_the_entrys_own_name() {
        let entry = crate::core::sources::bundle::entry_by_id("ibrowse")
            .unwrap()
            .expect("the ibrowse entry ships in the catalogue");
        match super::resolve(
            &entry,
            &aminet(),
            &[("ibrowse-dev".to_string(), aminet()[0].clone())],
        ) {
            super::Resolution::Refused { why } => {
                assert!(why.contains("ibrowse"), "got: {why}");
            }
            super::Resolution::Fetchable { .. } => panic!("no file path was given"),
        }
    }

    /// Same defect, the Aminet variant: an entry naming a fixed Aminet path
    /// that is empty **or whitespace-only** must refuse by name rather than
    /// resolve — the whitespace case is the one the plain `.is_empty()` this
    /// guard used to be written as would have missed.
    #[test]
    fn an_aminet_entry_with_an_empty_or_whitespace_path_is_refused_by_the_entrys_own_name() {
        for path in ["", "   "] {
            let e = entry("empty-aminet", EntrySource::Aminet { path: path.into() });
            match super::resolve(&e, &aminet(), &[]) {
                super::Resolution::Refused { why } => {
                    assert!(why.contains("empty-aminet"), "path {path:?}: got {why}");
                }
                super::Resolution::Fetchable { .. } => {
                    panic!("path {path:?}: no file path was given")
                }
            }
        }
    }

    /// Minor 6: `catalogue/acilis.json` (`changebootpri`) and
    /// `catalogue/kabuk.json` (`screentext`) both carry `"path": "downloads/"`
    /// — a directory, not a file. The old `.trim().is_empty()` guard let that
    /// through, and `meta_for`'s `rsplit('/')` then computed an empty
    /// `PackageMeta::name` from it: the exact nameless download the
    /// `ibrowse` guard exists to prevent, reached through a different door.
    #[test]
    fn a_mirror_entry_naming_only_a_directory_is_refused_by_the_entrys_own_name() {
        for id in ["changebootpri", "screentext"] {
            let entry = crate::core::sources::bundle::entry_by_id(id)
                .unwrap()
                .unwrap_or_else(|| panic!("'{id}' ships in the catalogue"));
            match super::resolve(
                &entry,
                &aminet(),
                &[("thomas-rapp".to_string(), aminet()[0].clone())],
            ) {
                super::Resolution::Refused { why } => {
                    assert!(why.contains(id), "'{id}': got {why}");
                }
                super::Resolution::Fetchable { .. } => {
                    panic!("'{id}': a bare directory is not a file")
                }
            }
        }
    }

    /// Minor 3: the old wording told the user to sync Aminet and retry, but
    /// `resolve` has no branch that consults `core/sources/catalog` and
    /// `DownloadContext` carries no catalogue — a user who followed that
    /// instruction would get the identical refusal back. The refusal must
    /// say ART cannot do this yet, not hand out an instruction it cannot
    /// honour.
    #[test]
    fn an_aminet_search_entry_is_refused_without_the_false_sync_instruction() {
        let e = entry(
            "amissl",
            EntrySource::AminetSearch {
                query: "amissl os3".into(),
            },
        );
        match super::resolve(&e, &aminet(), &[]) {
            super::Resolution::Refused { why } => {
                assert!(
                    !why.to_lowercase().contains("sync"),
                    "still tells the user to sync: {why}"
                );
                assert!(why.contains("amissl"), "got: {why}");
            }
            super::Resolution::Fetchable { .. } => panic!("ART cannot resolve 'latest version'"),
        }
    }
}
