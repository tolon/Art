//! The download cache (§41.5.3).
//!
//! Content-addressed: an object lives at `objects/<aa>/<sha256>`, so the same
//! bytes are never fetched twice and a corrupted entry is self-evident — its
//! name is its checksum.
//!
//! ```text
//! <root>/
//!   objects/aa/aabbcc…      the verified download
//!   partial/<key>.part      a resumable, unverified download
//!   by-path/<key>           which object a repository path resolved to
//! ```
//!
//! **The cache lives outside every image.** §41.5.3 and §57 both depend on it:
//! a failed or cancelled download leaves nothing but a `.part` file, and no
//! ADF or HDF is touched until the user asks for an install.
//!
//! `<key>` is the SHA-256 of the repository path rather than the path itself.
//! That is not obfuscation — it means no repository name, however hostile,
//! becomes a file name. There is no code path here that turns untrusted text
//! into a path component.

use std::path::{Path, PathBuf};

use crate::core::error::{CoreError, CoreResult};
use crate::core::hashing::sha256_bytes;

/// Where ART keeps downloaded packages.
#[derive(Debug, Clone)]
pub struct CacheLayout {
    root: PathBuf,
}

impl CacheLayout {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Where the object with this hash lives.
    ///
    /// The hash is validated rather than trusted: it is the only thing that
    /// ever becomes a path component here, so it has to be exactly 64 lowercase
    /// hex characters or nothing at all.
    pub fn object_path(&self, sha256: &str) -> CoreResult<PathBuf> {
        if sha256.len() != 64 || !sha256.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(CoreError::InvalidInput(
                "a cache object name must be a SHA-256 hex digest".into(),
            ));
        }
        if sha256.bytes().any(|b| b.is_ascii_uppercase()) {
            return Err(CoreError::InvalidInput(
                "cache object names are lowercase hex".into(),
            ));
        }

        Ok(self
            .root
            .join("objects")
            .join(&sha256[..2])
            .join(&sha256[2..]))
    }

    pub fn has_object(&self, sha256: &str) -> bool {
        self.object_path(sha256).is_ok_and(|p| p.is_file())
    }

    /// Where an in-progress download for this repository path is written.
    pub fn partial_path(&self, repo_path: &str) -> PathBuf {
        self.root
            .join("partial")
            .join(format!("{}.part", key_for(repo_path)))
    }

    /// Where ART notes which object a repository path resolved to.
    pub fn pointer_path(&self, repo_path: &str) -> PathBuf {
        self.root.join("by-path").join(key_for(repo_path))
    }

    /// The cached object for a repository path, if one is present *and* the
    /// object it names is still there.
    ///
    /// A pointer to a missing object is treated as no cache entry, not as an
    /// error: the user may have cleared the cache directory by hand.
    pub fn cached_object(&self, repo_path: &str) -> Option<(String, PathBuf)> {
        let pointer = self.pointer_path(repo_path);
        let sha = std::fs::read_to_string(pointer).ok()?;
        let sha = sha.trim().to_string();

        let path = self.object_path(&sha).ok()?;
        path.is_file().then_some((sha, path))
    }

    /// Create the directories the cache needs.
    pub fn ensure_dirs(&self) -> CoreResult<()> {
        for sub in ["objects", "partial", "by-path"] {
            std::fs::create_dir_all(self.root.join(sub))?;
        }
        Ok(())
    }
}

/// A filesystem-safe key for a repository path.
fn key_for(repo_path: &str) -> String {
    sha256_bytes(repo_path.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn layout(name: &str) -> (CacheLayout, PathBuf) {
        let dir = std::env::temp_dir().join(format!(
            "art-cache-{name}-{}",
            crate::core::test_scratch_id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        (CacheLayout::new(&dir), dir)
    }

    const SHA: &str = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";

    #[test]
    fn an_object_is_filed_under_the_first_two_digits_of_its_hash() {
        let (cache, dir) = layout("objects");
        let path = cache.object_path(SHA).unwrap();

        assert_eq!(path, dir.join("objects").join("e3").join(&SHA[2..]));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// The hash is the only untrusted-shaped thing that becomes a path
    /// component, so it is validated rather than trusted.
    #[test]
    fn only_a_real_digest_can_name_an_object() {
        let (cache, dir) = layout("badsha");

        for bad in [
            "",
            "short",
            "../../../etc/passwd",
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b85",
            "E3B0C44298FC1C149AFBF4C8996FB92427AE41E4649B934CA495991B7852B855",
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b85z",
        ] {
            assert!(cache.object_path(bad).is_err(), "accepted {bad:?}");
        }

        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// A hostile package name must never become a file name.
    #[test]
    fn a_repository_path_never_becomes_a_path_component() {
        let (cache, dir) = layout("keys");

        let partial = cache.partial_path("../../../etc/passwd");
        let pointer = cache.pointer_path("util/libs/\u{1b}[2J.lha");

        assert!(partial.starts_with(dir.join("partial")));
        assert!(pointer.starts_with(dir.join("by-path")));
        assert!(!partial.to_string_lossy().contains(".."));

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn the_same_path_always_maps_to_the_same_key() {
        let (cache, dir) = layout("stable");
        assert_eq!(
            cache.partial_path("util/libs/A.lha"),
            cache.partial_path("util/libs/A.lha")
        );
        assert_ne!(
            cache.partial_path("util/libs/A.lha"),
            cache.partial_path("util/libs/B.lha")
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// The user may empty the cache directory by hand. A dangling pointer is
    /// "not cached", not a failure.
    #[test]
    fn a_pointer_to_a_missing_object_reads_as_no_cache_entry() {
        let (cache, dir) = layout("dangling");
        cache.ensure_dirs().unwrap();

        std::fs::write(cache.pointer_path("util/libs/A.lha"), SHA).unwrap();
        assert!(cache.cached_object("util/libs/A.lha").is_none());

        let object = cache.object_path(SHA).unwrap();
        std::fs::create_dir_all(object.parent().unwrap()).unwrap();
        std::fs::write(&object, b"").unwrap();

        let (sha, path) = cache.cached_object("util/libs/A.lha").unwrap();
        assert_eq!(sha, SHA);
        assert_eq!(path, object);

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_corrupt_pointer_reads_as_no_cache_entry() {
        let (cache, dir) = layout("corruptptr");
        cache.ensure_dirs().unwrap();

        std::fs::write(cache.pointer_path("a/b.lha"), "not a hash").unwrap();
        assert!(cache.cached_object("a/b.lha").is_none());

        std::fs::remove_dir_all(&dir).unwrap();
    }
}
