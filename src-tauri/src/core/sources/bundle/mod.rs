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
const EMU68_JSON: &str = include_str!("catalogue/emu68.json");
const DOSYA_SISTEMI_JSON: &str = include_str!("catalogue/dosya-sistemi.json");
const TEMEL_JSON: &str = include_str!("catalogue/temel.json");
const AG_JSON: &str = include_str!("catalogue/ag.json");
const GRAFIK_JSON: &str = include_str!("catalogue/grafik.json");
const MASAUSTU_JSON: &str = include_str!("catalogue/masaustu.json");
const TESHIS_JSON: &str = include_str!("catalogue/teshis.json");
const ACILIS_JSON: &str = include_str!("catalogue/acilis.json");
const KABUK_JSON: &str = include_str!("catalogue/kabuk.json");
const WHDLOAD_JSON: &str = include_str!("catalogue/whdload.json");
const MEDYA_JSON: &str = include_str!("catalogue/medya.json");
const AMIGAOS_EKI_JSON: &str = include_str!("catalogue/amigaos-eki.json");
const IBROWSE_JSON: &str = include_str!("catalogue/ibrowse.json");

/// Every shipped file. Add a set by adding a `const` and a line here — the
/// same "a fourth package is a JSON file, not a code path" rule the install
/// recipes follow.
const SHIPPED: &[&str] = &[
    EMU68_JSON,
    ARSIV_JSON,
    DOSYA_SISTEMI_JSON,
    TEMEL_JSON,
    AG_JSON,
    GRAFIK_JSON,
    MASAUSTU_JSON,
    TESHIS_JSON,
    ACILIS_JSON,
    KABUK_JSON,
    WHDLOAD_JSON,
    MEDYA_JSON,
    AMIGAOS_EKI_JSON,
    IBROWSE_JSON,
];

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

    #[test]
    fn every_shipped_set_is_named_by_the_design() {
        let ids: Vec<String> = super::bundles()
            .unwrap()
            .into_iter()
            .map(|b| b.id)
            .collect();
        assert_eq!(
            ids,
            vec![
                "arsiv",
                "emu68",
                "dosya-sistemi",
                "temel",
                "ag",
                "grafik",
                "masaustu",
                "teshis",
                "acilis",
                "kabuk",
                "whdload",
                "medya",
                "amigaos-eki",
                "ibrowse",
            ],
            "14 sets, in download order"
        );
    }

    #[test]
    fn the_catalogue_holds_sixty_two_entries_and_no_id_twice() {
        let all = super::entries().unwrap();
        assert_eq!(
            all.len(),
            62,
            "60 from the Imager's own list, plus tolunnet and tolunwifi"
        );
        let mut seen = std::collections::HashSet::new();
        for entry in &all {
            assert!(
                seen.insert(entry.id.as_str()),
                "'{}' is declared twice",
                entry.id
            );
        }
    }

    #[test]
    fn no_entry_anywhere_carries_a_url() {
        // §41.5.7, as a test rather than as a rule somebody remembers. The
        // enum has no URL variant, so this can only fail if a path or query
        // smuggles one in.
        for entry in super::entries().unwrap() {
            let text = format!("{:?}", entry.source);
            assert!(
                !text.contains("http://") && !text.contains("https://"),
                "'{}' carries a URL: {text}",
                entry.id
            );
        }
    }

    #[test]
    fn every_requirement_names_an_entry_that_exists() {
        let all = super::entries().unwrap();
        let ids: Vec<&str> = all.iter().map(|e| e.id.as_str()).collect();
        for entry in &all {
            for need in &entry.requires {
                assert!(
                    ids.contains(&need.as_str()),
                    "'{}' requires unknown '{need}'",
                    entry.id
                );
            }
        }
    }

    #[test]
    fn every_permission_entry_is_named_in_the_licence_file() {
        // The owner's own requirement: "lisansa da ekleriz". Bound here so
        // forgetting it is a red suite rather than a quiet omission.
        let licences = include_str!("../../../../../THIRD_PARTY_LICENSES.md");
        let flagged: Vec<String> = super::entries()
            .unwrap()
            .into_iter()
            .filter(|e| e.permission.is_some())
            .map(|e| e.name)
            .collect();
        assert_eq!(
            flagged.len(),
            4,
            "Picasso96, iBrowse, SetPatch, Workbench-Library"
        );
        for name in flagged {
            assert!(
                licences.contains(&name),
                "'{name}' is not in THIRD_PARTY_LICENSES.md"
            );
        }
    }

    #[test]
    fn the_two_tcp_stacks_are_alternatives_and_say_so() {
        let all = super::entries().unwrap();
        let group = |id: &str| {
            all.iter()
                .find(|e| e.id == id)
                .unwrap_or_else(|| panic!("'{id}' is shipped"))
                .exclusive_group
                .clone()
        };
        assert_eq!(group("tolunnet"), Some("tcp".to_string()));
        assert_eq!(group("miamidx"), Some("tcp".to_string()));
    }

    #[test]
    fn whdloadwrapper_is_declared_as_something_art_cannot_fetch() {
        // Its printed source is an FTP search form with query parameters, not
        // a path. Declaring it `user-supplied` is how ART says so *before*
        // the user asks it to fetch, rather than failing at the attempt.
        let entry = super::entry_by_id("whdloadwrapper").unwrap().unwrap();
        assert!(matches!(
            entry.source,
            super::EntrySource::UserSupplied { .. }
        ));
    }
}
