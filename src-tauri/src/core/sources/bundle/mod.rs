//! The curated catalogue, as data — see
//! `docs/superpowers/specs/2026-08-22-package-bundles-design.md`.

pub mod parse;

use serde::{Deserialize, Serialize};

use crate::core::error::CoreResult;

/// Where an entry's file comes from. **A closed enum with no URL variant** —
/// that absence is §41.5.7's guarantee, expressed as a type rather than as a
/// rule somebody has to remember.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum EntrySource {
    /// A fixed Aminet repository path: `disk/misc/PFS3_53`.
    Aminet {
        path: String,
    },
    /// "Latest version", resolved through the catalog rather than pinned.
    AminetSearch {
        query: String,
    },
    GithubRelease {
        repo: String,
        asset: String,
    },
    /// A **configured** mirror by name, plus a path below its base.
    Mirror {
        mirror: String,
        path: String,
    },
    /// ART cannot fetch this, and says so before the user asks it to.
    UserSupplied {
        why: String,
    },
}

/// A licence or permission condition the screen must state before the tick.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Permission {
    pub holder: String,
    pub note: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BundleEntry {
    pub id: String,
    pub name: String,
    pub source: EntrySource,
    pub order: u32,
    pub exclusive_group: Option<String>,
    pub requires: Vec<String>,
    pub permission: Option<Permission>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Bundle {
    pub id: String,
    pub order: u32,
    pub entries: Vec<BundleEntry>,
}

const ARSIV_JSON: &str = include_str!("catalogue/arsiv.json");

/// Every shipped file. Add a set by adding a `const` and a line here — the
/// same "a fourth package is a JSON file, not a code path" rule the install
/// recipes follow.
const SHIPPED: &[&str] = &[ARSIV_JSON];

pub fn bundles() -> CoreResult<Vec<Bundle>> {
    let mut all: Vec<Bundle> = SHIPPED
        .iter()
        .map(|json| parse::parse(json))
        .collect::<CoreResult<Vec<Bundle>>>()?;
    all.sort_by_key(|b| b.order);
    Ok(all)
}

/// Every set's entries, flattened in download order: set order, then entry
/// order.
pub fn entries() -> CoreResult<Vec<BundleEntry>> {
    Ok(bundles()?.into_iter().flat_map(|b| b.entries).collect())
}

pub fn entry_by_id(id: &str) -> CoreResult<Option<BundleEntry>> {
    Ok(entries()?.into_iter().find(|e| e.id == id))
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_archivers_set_ships_and_parses() {
        let all = super::bundles().expect("the shipped bundles must parse");
        let arsiv = all
            .iter()
            .find(|b| b.id == "arsiv")
            .expect("the archivers set is shipped");
        let ids: Vec<&str> = arsiv.entries.iter().map(|e| e.id.as_str()).collect();
        assert_eq!(
            ids,
            vec!["lha", "lzx", "unzip", "zip", "xadmaster", "xpkuser"]
        );
    }

    #[test]
    fn the_archivers_set_is_first_in_download_order() {
        // Everything else arrives as .lha or .lzx; the order is data, not a
        // comment. `entries()` flattens in that order.
        let first = super::entries().unwrap().into_iter().next().unwrap();
        assert_eq!(first.id, "lha");
    }

    #[test]
    fn an_entry_naming_no_source_is_refused() {
        let json = r#"{ "id": "x", "order": 1, "entries": [
            { "id": "e", "name": "E", "order": 1 } ] }"#;
        let err = super::parse::parse(json).expect_err("a source is required");
        assert!(format!("{err}").contains("source"), "got: {err}");
    }
}
