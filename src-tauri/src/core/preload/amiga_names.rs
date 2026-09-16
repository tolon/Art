//! Recovering the **AmigaDOS** name of a file a distribution tree had to
//! store under a different host name (ART-160).
//!
//! ## Why a folder's own filenames are not always the answer
//!
//! Everything else in `core/preload` reads a source folder and copies it onto
//! an Amiga volume, taking each host filename as the Amiga name it should
//! land under. That is right for a folder a user assembled themselves, and it
//! is right for almost every file of a distribution tree — but not for all of
//! them. AmigaDOS allows names Windows does not: `AUX` is one of the 22 device
//! names reserved since DOS and it is genuinely on the owner's AmigaOS 3.9
//! disc at `Storage/DOSDrivers/AUX`, and a perfectly legal AmigaDOS
//! `Prices: 1993` is refused outright by NTFS. `core/osinstall`'s
//! `host_destination` escapes those on the way into the tree, so the host
//! name is `_AUX` while the name the Amiga must see is still `AUX`.
//!
//! Copying `_AUX` onto the card would be a silent, invisible corruption of
//! exactly the shape ART-168 was: a name AmigaDOS cannot use for the thing it
//! names, with every byte count and every progress figure still correct.
//! `osinstall::verify_volume` would then fail a file that is really there,
//! under the wrong name.
//!
//! ## Where the real name lives, and why this module re-declares it
//!
//! `distribution.json` at the tree's root records, for every file, the
//! AmigaDOS path (`path`) and — only when the two differ — the host path it
//! actually landed at (`hostPath`). That is the only place the pairing
//! survives, because the escaping is not reversible: `_AUX` is also a
//! perfectly ordinary name a real Amiga file could have.
//!
//! `core/osinstall` is the higher-level module here (it is an engine that
//! happens to produce a folder; this is a folder-to-volume copier), so this
//! module does **not** import its manifest type. It declares its own record
//! carrying only the two fields it reads, exactly the shape `CLAUDE.md`
//! prescribes and `core/rom/pairing.rs` already follows — serde ignores every
//! other field, so a manifest gaining one does not reach here.
//!
//! ## Absent, unreadable or silent is "no renames"
//!
//! A source folder with no `distribution.json` is the ordinary case — a
//! folder the user assembled — and a manifest that cannot be parsed is not a
//! reason to refuse a copy that would otherwise work. Both yield an empty
//! map, and an empty map changes nothing about what gets copied.
//!
//! ## A second input: ART's own record
//!
//! `distribution.json` is not the only place a name can be escaped. Content
//! staged onto a card by `core/volume/write/copy.rs` (an extracted archive or
//! HDF drawer, ART-160's other half) can escape a name too, and that staging
//! folder carries no `distribution.json` at all — it is not a distribution
//! tree, and this module must not invent one for it to import. It carries
//! [`AMIGA_NAMES_RECORD`] instead: an ART-private file, next to the content,
//! naming exactly the nodes that folder itself had to rename on the way in.
//!
//! It is deliberately not `distribution.json` under a different name —
//! writing that filename here would let a hostile archive entry impersonate
//! the manifest this module already trusts (see `SourceKind`/`place_archive`
//! in `core/card/content.rs`, which never stages an entry called either name
//! at a source's root). And it is written **only when there is something to
//! record**: an empty map is refused rather than written, so a folder that
//! needed no escaping never carries the file at all, and
//! `tools/hst_imager.rs`'s refusal — which fires on any non-empty
//! [`AmigaNames`] before the external tool is ever launched — keeps meaning
//! exactly what it says.

use std::collections::BTreeMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::core::error::{CoreError, CoreResult};
use crate::core::safety::atomic::atomic_write;

/// The manifest's file name at a distribution tree's root. Deliberately a
/// literal rather than an import — see the module doc comment.
const MANIFEST_FILE_NAME: &str = "distribution.json";

/// ART's own record of escaped names in a staging folder — never Amiga
/// content. See the module doc comment's "A second input" section.
pub const AMIGA_NAMES_RECORD: &str = ".art-amiga-names.json";

/// The on-disk shape of [`AMIGA_NAMES_RECORD`]. Named `NamesRecord`, not
/// `Record`, because [`Record`] already names the (unrelated)
/// `distribution.json` row shape in this same module.
#[derive(Serialize, Deserialize)]
struct NamesRecord {
    version: u32,
    names: Vec<NodeName>,
}

/// One escaped node: its host path (`/`-separated, relative to the staging
/// folder) and the AmigaDOS name it must be copied under.
#[derive(Serialize, Deserialize)]
struct NodeName {
    host: String,
    amiga: String,
}

/// The record's schema version. There is only ever one today; a reader that
/// meets a higher one has nothing to gain from guessing, but nothing here
/// depends on that yet — this module ignores the field entirely on read,
/// the same way it ignores every unrecognised field of `distribution.json`.
const NAMES_RECORD_VERSION: u32 = 1;

/// Write [`AMIGA_NAMES_RECORD`] into `source`, naming exactly the pairs a
/// copy had to escape.
///
/// `SAFE_CREATE`: an existing record is never replaced, the same rule
/// `core/adf/create.rs` and `core/hdf.rs` apply to a disk image. Refuses an
/// empty map too — see the module doc comment's last paragraph — so this
/// function is the single place that decides whether the file is written at
/// all, and a caller never has to check `names.is_empty()` itself first.
pub fn write_record(source: &Path, names: &BTreeMap<String, String>) -> CoreResult<()> {
    if names.is_empty() {
        return Err(CoreError::InvalidInput(
            "no escaped names to record".to_string(),
        ));
    }
    let path = source.join(AMIGA_NAMES_RECORD);
    if path.exists() {
        return Err(CoreError::SafetyRefused(format!(
            "'{}' already exists; ART's own names record is never replaced",
            path.display()
        )));
    }
    let record = NamesRecord {
        version: NAMES_RECORD_VERSION,
        names: names
            .iter()
            .map(|(host, amiga)| NodeName {
                host: host.clone(),
                amiga: amiga.clone(),
            })
            .collect(),
    };
    let bytes = serde_json::to_vec_pretty(&record).map_err(|err| CoreError::Malformed {
        format: "ART names record".into(),
        detail: err.to_string(),
    })?;
    atomic_write(&path, &bytes)
}

/// Only what this module reads out of `distribution.json`; serde ignores the
/// rest.
#[derive(Deserialize)]
struct Manifest {
    #[serde(default)]
    files: Vec<Record>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Record {
    path: String,
    #[serde(default)]
    host_path: Option<String>,
}

/// Host path (`/`-separated, relative to the tree root) → the AmigaDOS
/// **name** of that one node.
///
/// Keyed per node rather than per file so a renamed *directory* is translated
/// once for everything under it: a walk that has reached `Storage/_AUX` asks
/// this map for that prefix and gets `AUX`, whatever the file inside is
/// called.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct AmigaNames(BTreeMap<String, String>);

impl AmigaNames {
    /// Read both inputs a source folder can carry: the distribution
    /// manifest, and ART's own private record (see the module doc comment).
    /// Never an error — an absent or unparsable input is "no renames",
    /// exactly as `distribution.json` alone already was.
    ///
    /// The record wins on a key both name: it is the newer, narrower input,
    /// written by the copy that just ran rather than inherited from a
    /// distribution build, so it is read second and its pairs overwrite the
    /// manifest's.
    pub fn read(source: &Path) -> Self {
        let mut names = Self::from_manifest(source);
        for (host, amiga) in Self::record_pairs(source) {
            names.0.insert(host, amiga);
        }
        names
    }

    /// The `distribution.json` half of [`Self::read`].
    fn from_manifest(source: &Path) -> Self {
        let Ok(text) = std::fs::read_to_string(source.join(MANIFEST_FILE_NAME)) else {
            return Self::default();
        };
        let Ok(manifest) = serde_json::from_str::<Manifest>(&text) else {
            return Self::default();
        };
        Self::from_records(manifest.files.iter().filter_map(|file| {
            file.host_path
                .as_deref()
                .map(|host| (host, file.path.as_str()))
        }))
    }

    /// The [`AMIGA_NAMES_RECORD`] half of [`Self::read`], as owned pairs —
    /// unlike the manifest, the record is already keyed per node rather than
    /// per file, so there is no depth-walking to do, only reading it back.
    fn record_pairs(source: &Path) -> Vec<(String, String)> {
        let Ok(text) = std::fs::read_to_string(source.join(AMIGA_NAMES_RECORD)) else {
            return Vec::new();
        };
        let Ok(record) = serde_json::from_str::<NamesRecord>(&text) else {
            return Vec::new();
        };
        record
            .names
            .into_iter()
            .map(|node| (node.host, node.amiga))
            .collect()
    }

    /// Build the per-node map from `(host path, amiga path)` pairs.
    ///
    /// A pair whose two sides do not have the same number of segments is
    /// dropped rather than guessed at: the escaping is name-by-name, so they
    /// always agree, and a manifest where they do not is one this module has
    /// no honest way to read.
    fn from_records<'a>(pairs: impl Iterator<Item = (&'a str, &'a str)>) -> Self {
        let mut map = BTreeMap::new();
        for (host, amiga) in pairs {
            let host_parts: Vec<&str> = host.split('/').collect();
            let amiga_parts: Vec<&str> = amiga.split('/').collect();
            if host_parts.len() != amiga_parts.len() {
                continue;
            }
            for depth in 0..host_parts.len() {
                if host_parts[depth] == amiga_parts[depth] {
                    continue;
                }
                map.insert(
                    host_parts[..=depth].join("/"),
                    amiga_parts[depth].to_string(),
                );
            }
        }
        Self(map)
    }

    /// The AmigaDOS name for the node at `host_relative`, or `None` when the
    /// host name is already the Amiga name — which is the answer for every
    /// node of a tree that needed no escaping at all.
    pub fn name_for(&self, host_relative: &str) -> Option<&str> {
        self.0.get(host_relative).map(String::as_str)
    }

    /// Every `host → amiga` pair, for a caller that has to *refuse* rather
    /// than translate — see `tools/hst_imager.rs`, which hands the folder to
    /// an external tool and so cannot rename anything on the way in.
    pub fn pairs(&self) -> impl Iterator<Item = (&str, &str)> {
        self.0.iter().map(|(k, v)| (k.as_str(), v.as_str()))
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_folder_without_a_manifest_renames_nothing() {
        let (_guard, dir) = crate::core::ScratchDir::pair("art-amiganames-none", "no-manifest");
        let names = AmigaNames::read(&dir);
        assert!(names.is_empty());
        assert_eq!(names.name_for("Storage/_AUX"), None);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_manifest_that_is_not_json_renames_nothing() {
        let (_guard, dir) = crate::core::ScratchDir::pair("art-amiganames-bad", "not-json");
        std::fs::write(dir.join(MANIFEST_FILE_NAME), b"{ not json").unwrap();
        assert!(AmigaNames::read(&dir).is_empty());
        std::fs::remove_dir_all(&dir).ok();
    }

    /// The real case: `Storage/DOSDrivers/AUX` off the owner's AmigaOS 3.9
    /// disc, stored as `Storage/DOSDrivers/_AUX` because `AUX` is a reserved
    /// Windows device name.
    #[test]
    fn a_reserved_device_name_is_recovered_for_the_file_it_names() {
        let names = AmigaNames::from_records(
            [("Storage/DOSDrivers/_AUX", "Storage/DOSDrivers/AUX")].into_iter(),
        );
        assert_eq!(names.name_for("Storage/DOSDrivers/_AUX"), Some("AUX"));
        // The unescaped prefixes are not in the map at all — nothing to
        // translate means nothing to look up.
        assert_eq!(names.name_for("Storage"), None);
        assert_eq!(names.name_for("Storage/DOSDrivers"), None);
    }

    /// A renamed *drawer* is translated once, for every file beneath it.
    #[test]
    fn a_renamed_drawer_is_one_entry_however_many_files_it_holds() {
        let names = AmigaNames::from_records(
            [
                ("Devs/_CON/a.info", "Devs/CON/a.info"),
                ("Devs/_CON/b.info", "Devs/CON/b.info"),
            ]
            .into_iter(),
        );
        assert_eq!(names.name_for("Devs/_CON"), Some("CON"));
        assert_eq!(names.pairs().count(), 1);
    }

    /// Both halves of a path can need escaping at once, and each is recorded
    /// against its own prefix.
    #[test]
    fn a_drawer_and_the_file_in_it_can_both_be_escaped() {
        let names = AmigaNames::from_records([("Devs/_CON/_AUX", "Devs/CON/AUX")].into_iter());
        assert_eq!(names.name_for("Devs/_CON"), Some("CON"));
        assert_eq!(names.name_for("Devs/_CON/_AUX"), Some("AUX"));
    }

    #[test]
    fn a_pair_whose_halves_disagree_on_depth_is_dropped() {
        let names = AmigaNames::from_records([("A/B", "A/B/C")].into_iter());
        assert!(names.is_empty());
    }

    /// **ART-160's other half.** A staging folder with no `distribution.json`
    /// still gets its escaped names back, node by node, from ART's own
    /// private record — and the record is `SAFE_CREATE`: written once, never
    /// replaced.
    #[test]
    fn a_staged_record_puts_the_amiga_names_back_node_by_node() {
        let (_guard, dir) = crate::core::ScratchDir::pair("art-amiga-names", "record");
        let names = BTreeMap::from([
            ("_AUX".to_string(), "AUX".to_string()),
            ("_AUX/a_b".to_string(), "a?b".to_string()),
        ]);
        write_record(&dir, &names).unwrap();

        let read = AmigaNames::read(&dir);
        assert_eq!(read.name_for("_AUX"), Some("AUX"));
        assert_eq!(read.name_for("_AUX/a_b"), Some("a?b"));

        assert!(
            write_record(&dir, &names).is_err(),
            "SAFE_CREATE: an existing record is not replaced"
        );
    }

    /// The module doc comment's rule: a folder that needed no escaping never
    /// carries the file at all, so `tools/hst_imager.rs`'s refusal keeps
    /// meaning exactly what it says.
    #[test]
    fn an_empty_record_is_never_written() {
        let (_guard, dir) = crate::core::ScratchDir::pair("art-amiga-names", "empty");
        assert!(write_record(&dir, &BTreeMap::new()).is_err());
        assert!(!dir.join(AMIGA_NAMES_RECORD).exists());
    }
}
