//! libretro-thumbnails — an index, then only the images that matched.
//!
//! Fetching the index rather than guessing URLs is not an optimisation. A
//! speculative "build the URL and read the 404" strategy cannot work here:
//! `1000 Miglia` does not become `1000 Miglia - 1927-1933 Volume 1` by any rule.
//! It is also the impolite design — 1700 requests, most of them misses — where
//! three index files are 2.5 MB and then only matches are downloaded.
//!
//! Two calls, because `validate_fetch_path` rejects `?` and `:`: neither
//! `?recursive=1` nor the compact `trees/master:Named_Boxarts` form is
//! expressible. The plain root-tree-then-subtree form needs neither.
//!
//! Measured against the live repository on 2026-08-17: Named_Boxarts 3324,
//! Named_Titles 3434, Named_Snaps 3475, Named_Logos present. One subtree's
//! JSON is 0.8 MB.

use std::collections::BTreeMap;

use serde::Deserialize;

use crate::core::artwork::encode::path_segment;
use crate::core::artwork::key::{lookup, normalise};
use crate::core::artwork::sources::{ArtSource, SourceIndex};
use crate::core::artwork::ArtKind;
use crate::core::error::{CoreError, CoreResult};

pub const ROOT_TREE_PATH: &str = "repos/libretro-thumbnails/Commodore_-_Amiga/git/trees/master";

/// The repository directory each kind lives in.
const DIRECTORIES: [(ArtKind, &str); 4] = [
    (ArtKind::Boxart, "Named_Boxarts"),
    (ArtKind::Snap, "Named_Snaps"),
    (ArtKind::Title, "Named_Titles"),
    (ArtKind::Logo, "Named_Logos"),
];

const KINDS: [ArtKind; 4] = [
    ArtKind::Boxart,
    ArtKind::Snap,
    ArtKind::Title,
    ArtKind::Logo,
];

/// One subtree's JSON was measured at 0.8 MB; this is generous and still
/// bounded, which is the point — never allocate from an unchecked length.
const MAX_INDEX_BYTES: usize = 16 * 1024 * 1024;

#[derive(Debug, Deserialize)]
struct TreeReply {
    tree: Vec<TreeEntry>,
    #[serde(default)]
    truncated: bool,
}

#[derive(Debug, Deserialize)]
struct TreeEntry {
    path: String,
    #[serde(rename = "type")]
    kind: String,
    #[serde(default)]
    sha: String,
}

/// Build the path for one subtree.
///
/// The sha is not interpolated blindly — see [`read_subtree_shas`], which
/// refuses anything that is not a plain hex id before it ever reaches here.
pub fn subtree_path(sha: &str) -> String {
    format!("repos/libretro-thumbnails/Commodore_-_Amiga/git/trees/{sha}")
}

fn is_plain_sha(sha: &str) -> bool {
    !sha.is_empty() && sha.len() <= 64 && sha.bytes().all(|b| b.is_ascii_hexdigit())
}

/// Read the root tree and return the sha of each directory this source knows.
///
/// A sha arriving from a fetched document is outside input. One that is not
/// plain hex is dropped rather than concatenated into a path.
pub fn read_subtree_shas(bytes: &[u8]) -> CoreResult<BTreeMap<ArtKind, String>> {
    let reply: TreeReply = serde_json::from_slice(bytes)
        .map_err(|err| CoreError::InvalidInput(format!("libretro root tree: {err}")))?;

    let mut found = BTreeMap::new();
    for entry in reply.tree {
        if entry.kind != "tree" || !is_plain_sha(&entry.sha) {
            continue;
        }
        if let Some((kind, _)) = DIRECTORIES.iter().find(|(_, dir)| *dir == entry.path) {
            found.insert(*kind, entry.sha);
        }
    }
    Ok(found)
}

#[derive(Debug, Default, Clone, Copy)]
pub struct Libretro;

impl ArtSource for Libretro {
    fn id(&self) -> &'static str {
        "libretro"
    }

    fn kinds(&self) -> &'static [ArtKind] {
        &KINDS
    }

    /// The root tree only. The per-kind subtrees are not known until it has been
    /// read, so the run fetches this first and asks the source again.
    fn index_paths(&self) -> Vec<(ArtKind, String)> {
        vec![(ArtKind::Boxart, ROOT_TREE_PATH.to_string())]
    }

    fn absorb_index(&self, kind: ArtKind, bytes: &[u8], into: &mut SourceIndex) -> CoreResult<()> {
        if bytes.len() > MAX_INDEX_BYTES {
            return Err(CoreError::InvalidInput(
                "libretro index is larger than the allowed bound".into(),
            ));
        }
        let reply: TreeReply = serde_json::from_slice(bytes)
            .map_err(|err| CoreError::InvalidInput(format!("libretro subtree: {err}")))?;

        // A truncated tree is a partial index, and a partial index turns titles
        // that do have pictures into recorded misses that are not misses.
        if reply.truncated {
            return Err(CoreError::InvalidInput(
                "libretro returned a truncated tree; the index would be incomplete".into(),
            ));
        }

        let directory = DIRECTORIES
            .iter()
            .find(|(art, _)| *art == kind)
            .map(|(_, dir)| *dir)
            .ok_or_else(|| {
                CoreError::InvalidInput("libretro has no directory for that kind".into())
            })?;

        let map = into.by_kind.entry(kind).or_default();
        for entry in reply.tree {
            if entry.kind != "blob" {
                continue;
            }
            let Some(stem) = entry.path.strip_suffix(".png") else {
                continue;
            };
            map.insert(normalise(stem), format!("{directory}/{}", entry.path));
        }
        Ok(())
    }

    fn locate(&self, index: &SourceIndex, title: &str, kind: ArtKind) -> Option<String> {
        let map = index.by_kind.get(&kind)?;
        let raw = lookup(map, title)?;
        // The stored value is `<dir>/<filename>`; only the filename is encoded.
        let (dir, file) = raw.rsplit_once('/')?;
        Some(format!("{dir}/{}", path_segment(file)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Shaped exactly like the live API's reply, trimmed.
    const ROOT_TREE: &[u8] = br#"{"tree":[
        {"path":".gitignore","type":"blob","sha":"aa4d15"},
        {"path":"Named_Boxarts","type":"tree","sha":"7a1b0e"},
        {"path":"Named_Snaps","type":"tree","sha":"9f65ac"},
        {"path":"Named_Titles","type":"tree","sha":"2d822f"}
    ],"truncated":false}"#;

    const BOXART_TREE: &[u8] = br#"{"tree":[
        {"path":"1869 - Erlebte Geschichte Teil I.png","type":"blob"},
        {"path":"Turrican II.png","type":"blob"},
        {"path":"NotAnImage.txt","type":"blob"}
    ],"truncated":false}"#;

    #[test]
    fn the_root_tree_path_carries_no_colon_and_no_query() {
        assert!(!ROOT_TREE_PATH.contains(':'));
        assert!(!ROOT_TREE_PATH.contains('?'));
        assert!(!ROOT_TREE_PATH.contains(' '));
    }

    /// The path the validator will see, built the way the run builds it.
    #[test]
    fn both_tree_paths_pass_the_validator() {
        let mirror =
            crate::core::sources::mirror::Mirror::new("t", "https://api.github.com/").unwrap();
        mirror.url_for(ROOT_TREE_PATH).unwrap();
        mirror.url_for(&subtree_path("7a1b0e")).unwrap();
    }

    #[test]
    fn a_subtree_sha_becomes_a_plain_path() {
        assert_eq!(
            subtree_path("7a1b0e"),
            "repos/libretro-thumbnails/Commodore_-_Amiga/git/trees/7a1b0e"
        );
    }

    /// A sha arrives from a fetched document. It must not be able to become a
    /// path component of its own.
    #[test]
    fn a_hostile_sha_is_refused_rather_than_concatenated() {
        let hostile = br#"{"tree":[
            {"path":"Named_Boxarts","type":"tree","sha":"../../etc"}
        ]}"#;
        assert_eq!(read_subtree_shas(hostile).unwrap().len(), 0);
    }

    #[test]
    fn only_directories_this_source_knows_are_taken_from_the_root() {
        let shas = read_subtree_shas(ROOT_TREE).unwrap();
        assert_eq!(
            shas.get(&ArtKind::Boxart).map(String::as_str),
            Some("7a1b0e")
        );
        assert_eq!(shas.get(&ArtKind::Snap).map(String::as_str), Some("9f65ac"));
        assert_eq!(shas.get(&ArtKind::Title).map(String::as_str), Some("2d822f"));
        assert_eq!(shas.get(&ArtKind::Logo), None);
    }

    #[test]
    fn a_subtree_becomes_an_index_of_images_only() {
        let mut index = SourceIndex::default();
        Libretro
            .absorb_index(ArtKind::Boxart, BOXART_TREE, &mut index)
            .unwrap();

        let boxarts = index.by_kind.get(&ArtKind::Boxart).unwrap();
        assert_eq!(boxarts.len(), 2, "the .txt must not be indexed");
        assert!(boxarts.contains_key("turrican ii"));
    }

    #[test]
    fn locate_encodes_the_path_it_returns() {
        let mut index = SourceIndex::default();
        Libretro
            .absorb_index(ArtKind::Boxart, BOXART_TREE, &mut index)
            .unwrap();

        let path = Libretro.locate(&index, "1869", ArtKind::Boxart).unwrap();
        assert_eq!(
            path,
            "Named_Boxarts/1869%20-%20Erlebte%20Geschichte%20Teil%20I.png"
        );
        assert!(!path.contains(' '));
    }

    /// A truncated tree is a partial index, and a partial index silently
    /// produces misses that are not misses.
    #[test]
    fn a_truncated_tree_is_an_error_not_a_short_index() {
        let mut index = SourceIndex::default();
        let truncated = br#"{"tree":[{"path":"A.png","type":"blob"}],"truncated":true}"#;
        assert!(Libretro
            .absorb_index(ArtKind::Boxart, truncated, &mut index)
            .is_err());
    }

    /// An index this source has not absorbed yields nothing rather than
    /// pretending the title has no picture anywhere.
    #[test]
    fn locate_on_an_unabsorbed_kind_finds_nothing() {
        let index = SourceIndex::default();
        assert_eq!(Libretro.locate(&index, "Turrican II", ArtKind::Boxart), None);
    }

    #[test]
    fn a_reply_that_is_not_json_is_an_error() {
        let mut index = SourceIndex::default();
        assert!(Libretro
            .absorb_index(ArtKind::Boxart, b"<html>404</html>", &mut index)
            .is_err());
        assert!(read_subtree_shas(b"<html>404</html>").is_err());
    }
}
