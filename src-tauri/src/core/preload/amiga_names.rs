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

use std::collections::BTreeMap;
use std::path::Path;

use serde::Deserialize;

/// The manifest's file name at a distribution tree's root. Deliberately a
/// literal rather than an import — see the module doc comment.
const MANIFEST_FILE_NAME: &str = "distribution.json";

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
    /// Read the tree's manifest, if it has one. Never an error — see the
    /// module doc comment's last section.
    pub fn read(source: &Path) -> Self {
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
        let dir = std::env::temp_dir().join(format!(
            "art-amiganames-none-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let names = AmigaNames::read(&dir);
        assert!(names.is_empty());
        assert_eq!(names.name_for("Storage/_AUX"), None);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_manifest_that_is_not_json_renames_nothing() {
        let dir = std::env::temp_dir().join(format!(
            "art-amiganames-bad-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
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
}
