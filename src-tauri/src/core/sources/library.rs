//! Where downloads actually land for the user.
//!
//! The cache in [`super::cache`] is content-addressed: `objects/aa/<sha256>`.
//! That is right for *verification* — the same bytes are never fetched twice and
//! a corrupted entry is self-evident — and useless as somewhere to keep things.
//! Nobody wants a folder of hex.
//!
//! So the library is the second half: a plain folder tree the user chooses and
//! organises. They pick a root (`D:\Amiga`), and each download goes to a
//! subfolder of their choosing (`browser`, `games/wip`, or the root itself).
//! Files can be moved between subfolders afterwards.
//!
//! ## The whole file is one security problem
//!
//! A subfolder name is free text typed by a user, and it becomes a path. So is
//! the file name, which arrives from a repository index. Every path here is
//! built through [`safe_join`] against the root and then **re-checked for
//! containment**, and [`relocate`](Library::relocate) additionally refuses to
//! move anything that is not already inside the root — otherwise "move this
//! download" would be a general-purpose file mover pointed at whatever path it
//! was handed.
//!
//! ## Nothing is overwritten by accident
//!
//! Placement follows the same [`OverwritePolicy`] as archive extraction, and
//! defaults to `Skip` for the same reason: the destination is a folder the user
//! works in, and replacing something there without being asked is exactly the
//! failure §57 exists to prevent.

use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::core::error::{CoreError, CoreResult};
use crate::core::lha::safe_extract::{next_free_path, OverwritePolicy};
use crate::core::security::path::safe_join;

/// How deep the subfolder picker will look.
const MAX_SUBFOLDER_DEPTH: usize = 4;

/// The most subfolders reported to the picker.
const MAX_SUBFOLDERS: usize = 500;

/// The longest subfolder path accepted.
const MAX_SUBFOLDER_BYTES: usize = 240;

/// Where a file ended up, and whether anything was actually written.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Placement {
    /// The absolute path of the file in the library.
    pub path: PathBuf,
    /// Its subfolder relative to the root; empty for the root itself.
    pub subfolder: String,
    pub bytes: u64,
    /// True when an existing file was kept and nothing was written.
    pub skipped_existing: bool,
    /// True when the file was written under a different name to keep both.
    pub renamed: bool,
}

/// The user's download folder.
#[derive(Debug, Clone)]
pub struct Library {
    root: PathBuf,
}

impl Library {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Resolve `subfolder` to a directory inside the root.
    ///
    /// An empty subfolder is the root itself, which is the common case and not
    /// an error.
    ///
    /// Leading and trailing separators are trimmed: someone typing `/browser`
    /// into a box labelled "subfolder" means the browser subfolder, and the
    /// result is still inside the root, so reading it that way costs no safety.
    /// That is a different thing from repairing a *traversal* — `../browser`
    /// and `C:\Windows` are refused outright, because there is no reading of
    /// them that stays inside the folder the user chose.
    pub fn directory(&self, subfolder: &str) -> CoreResult<PathBuf> {
        let trimmed = subfolder.trim().trim_matches(['/', '\\']);
        if trimmed.is_empty() {
            return Ok(self.root.clone());
        }
        if trimmed.len() > MAX_SUBFOLDER_BYTES {
            return Err(CoreError::InvalidInput(
                "that folder name is too long".into(),
            ));
        }

        let joined = safe_join(&self.root, trimmed).map_err(|err| {
            CoreError::SafetyRefused(format!(
                "'{subfolder}' is not a folder inside the download folder: {err}"
            ))
        })?;

        // `safe_join` already rejects traversal; this re-check is what catches a
        // root that is itself a symlink or a future change to the join rules.
        self.ensure_inside(&joined)?;
        Ok(joined)
    }

    /// Where a file of this name would go.
    pub fn destination(&self, subfolder: &str, file_name: &str) -> CoreResult<PathBuf> {
        let name = validate_file_name(file_name)?;
        Ok(self.directory(subfolder)?.join(name))
    }

    /// Copy a verified file from the cache into the library.
    ///
    /// The source is expected to be a cache object that has already passed the
    /// trust pipeline — this function does not re-validate it, because doing so
    /// here would suggest the cache might hold something unchecked.
    pub fn place(
        &self,
        source: &Path,
        subfolder: &str,
        file_name: &str,
        policy: OverwritePolicy,
    ) -> CoreResult<Placement> {
        let directory = self.directory(subfolder)?;
        let name = validate_file_name(file_name)?;
        let target = directory.join(name);

        std::fs::create_dir_all(&directory)?;

        let (final_path, renamed) = match resolve_target(&target, policy)? {
            Some(path) => {
                let renamed = path != target;
                (path, renamed)
            }
            None => {
                return Ok(Placement {
                    bytes: std::fs::metadata(&target).map(|m| m.len()).unwrap_or(0),
                    path: target,
                    subfolder: self.relative_of(&directory),
                    skipped_existing: true,
                    renamed: false,
                })
            }
        };

        let bytes = std::fs::copy(source, &final_path)?;

        Ok(Placement {
            path: final_path,
            subfolder: self.relative_of(&directory),
            bytes,
            skipped_existing: false,
            renamed,
        })
    }

    /// Move a file that is already in the library to another subfolder.
    ///
    /// Refuses anything outside the root. Without that check this would be a
    /// general-purpose file mover driven by whatever path it was handed.
    pub fn relocate(
        &self,
        current: &Path,
        subfolder: &str,
        policy: OverwritePolicy,
    ) -> CoreResult<Placement> {
        self.ensure_inside(current)?;
        if !current.is_file() {
            return Err(CoreError::InvalidInput(format!(
                "'{}' is not a file in the download folder",
                current.display()
            )));
        }

        let file_name = current
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or(CoreError::NonUtf8Path)?
            .to_string();

        let directory = self.directory(subfolder)?;
        std::fs::create_dir_all(&directory)?;
        let target = directory.join(validate_file_name(&file_name)?);

        if target == current {
            return Ok(Placement {
                bytes: std::fs::metadata(current).map(|m| m.len()).unwrap_or(0),
                path: current.to_path_buf(),
                subfolder: self.relative_of(&directory),
                skipped_existing: false,
                renamed: false,
            });
        }

        let (final_path, renamed) = match resolve_target(&target, policy)? {
            Some(path) => {
                let renamed = path != target;
                (path, renamed)
            }
            None => {
                return Ok(Placement {
                    bytes: std::fs::metadata(current).map(|m| m.len()).unwrap_or(0),
                    path: current.to_path_buf(),
                    subfolder: self.relative_of(current.parent().unwrap_or(&self.root)),
                    skipped_existing: true,
                    renamed: false,
                })
            }
        };

        // Rename first; fall back to copy-then-delete for a move across
        // volumes, which `rename` cannot do. The copy is verified before the
        // original goes — a move that loses the file is not a move.
        if std::fs::rename(current, &final_path).is_err() {
            std::fs::copy(current, &final_path)?;
            let copied = std::fs::metadata(&final_path)?.len();
            let original = std::fs::metadata(current)?.len();
            if copied != original {
                let _ = std::fs::remove_file(&final_path);
                return Err(CoreError::IntegrityMismatch(format!(
                    "the copy is {copied} bytes but the original is {original}; nothing was removed"
                )));
            }
            std::fs::remove_file(current)?;
        }

        let bytes = std::fs::metadata(&final_path).map(|m| m.len()).unwrap_or(0);
        Ok(Placement {
            path: final_path,
            subfolder: self.relative_of(&directory),
            bytes,
            skipped_existing: false,
            renamed,
        })
    }

    /// The subfolders that exist under the root, for the destination picker.
    ///
    /// Depth- and count-limited, and symlinks are not followed — the same
    /// discipline the collection scanner needed after [ART-028].
    pub fn subfolders(&self) -> CoreResult<Vec<String>> {
        let mut found = Vec::new();
        if self.root.is_dir() {
            collect(&self.root, &self.root, 0, &mut found)?;
        }
        found.sort();
        found.dedup();
        Ok(found)
    }

    /// Reject a path that is not inside the root.
    fn ensure_inside(&self, path: &Path) -> CoreResult<()> {
        // Both sides must be canonicalised the *same* way or the comparison is
        // meaningless: on Windows `canonicalize` returns a `\\?\`-prefixed path,
        // so canonicalising only the root makes every not-yet-created
        // destination look like an escape.
        let root = canonicalise_existing_part(&self.root);
        let candidate = canonicalise_existing_part(path);

        if candidate.starts_with(&root) {
            Ok(())
        } else {
            Err(CoreError::SafetyRefused(format!(
                "'{}' is outside the download folder",
                path.display()
            )))
        }
    }

    /// A path's location relative to the root, using forward slashes.
    fn relative_of(&self, path: &Path) -> String {
        path.strip_prefix(&self.root)
            .map(|rest| rest.to_string_lossy().replace('\\', "/"))
            .unwrap_or_default()
    }
}

/// How far up a path the search for an existing ancestor will walk.
const MAX_ANCESTOR_WALK: usize = 64;

/// Canonicalise the deepest existing part of `path` and re-attach the rest.
///
/// A destination that has not been created yet cannot be canonicalised, but its
/// parent usually can — and resolving the parent is what actually matters,
/// because that is where a symlink or a junction would redirect. The unresolved
/// tail is appended verbatim; `safe_join` has already established it contains
/// no traversal.
fn canonicalise_existing_part(path: &Path) -> PathBuf {
    let mut existing = path.to_path_buf();
    let mut tail: Vec<std::ffi::OsString> = Vec::new();

    for _ in 0..MAX_ANCESTOR_WALK {
        if existing.exists() {
            break;
        }
        match (existing.file_name(), existing.parent()) {
            (Some(name), Some(parent)) => {
                tail.push(name.to_os_string());
                existing = parent.to_path_buf();
            }
            _ => break,
        }
    }

    let mut resolved = std::fs::canonicalize(&existing).unwrap_or(existing);
    for name in tail.into_iter().rev() {
        resolved.push(name);
    }
    resolved
}

/// Where to actually write, or `None` when an existing file is being kept.
fn resolve_target(target: &Path, policy: OverwritePolicy) -> CoreResult<Option<PathBuf>> {
    if !target.exists() {
        return Ok(Some(target.to_path_buf()));
    }
    match policy {
        OverwritePolicy::Skip => Ok(None),
        OverwritePolicy::Overwrite => Ok(Some(target.to_path_buf())),
        OverwritePolicy::Rename => next_free_path(target).map(Some),
    }
}

/// A file name, not a path.
fn validate_file_name(name: &str) -> CoreResult<&str> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err(CoreError::InvalidInput("the file has no name".into()));
    }
    if trimmed.contains('/') || trimmed.contains('\\') {
        return Err(CoreError::SafetyRefused(format!(
            "'{name}' is a path, not a file name"
        )));
    }
    if trimmed == "." || trimmed == ".." {
        return Err(CoreError::SafetyRefused(format!(
            "'{name}' is not a file name"
        )));
    }
    Ok(trimmed)
}

fn collect(root: &Path, dir: &Path, depth: usize, out: &mut Vec<String>) -> CoreResult<()> {
    if depth >= MAX_SUBFOLDER_DEPTH || out.len() >= MAX_SUBFOLDERS {
        return Ok(());
    }

    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        // An unreadable folder is not a reason to fail the whole listing.
        Err(_) => return Ok(()),
    };

    for entry in entries.flatten() {
        if out.len() >= MAX_SUBFOLDERS {
            break;
        }
        let path = entry.path();
        // `symlink_metadata` does not follow the link, so a junction pointing
        // back up its own tree cannot be walked into (ART-028).
        let Ok(meta) = std::fs::symlink_metadata(&path) else {
            continue;
        };
        if !meta.is_dir() {
            continue;
        }

        if let Ok(rest) = path.strip_prefix(root) {
            out.push(rest.to_string_lossy().replace('\\', "/"));
        }
        collect(root, &path, depth + 1, out)?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Fixture {
        dir: PathBuf,
        library: Library,
    }

    impl Fixture {
        fn new(name: &str) -> Self {
            let dir = std::env::temp_dir().join(format!(
                "art-library-{name}-{}",
                crate::core::test_scratch_id()
            ));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(dir.join("root")).unwrap();
            Self {
                library: Library::new(dir.join("root")),
                dir,
            }
        }

        fn source(&self, name: &str, bytes: &[u8]) -> PathBuf {
            let path = self.dir.join(name);
            std::fs::write(&path, bytes).unwrap();
            path
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }

    #[test]
    fn a_download_lands_in_the_subfolder_the_user_chose() {
        let f = Fixture::new("place");
        let source = f.source("cached", b"payload");

        let placed = f
            .library
            .place(&source, "browser", "IBrowse.lha", OverwritePolicy::Skip)
            .unwrap();

        assert_eq!(
            placed.path,
            f.library.root().join("browser").join("IBrowse.lha")
        );
        assert_eq!(placed.subfolder, "browser");
        assert_eq!(placed.bytes, 7);
        assert!(!placed.skipped_existing);
        assert_eq!(std::fs::read(&placed.path).unwrap(), b"payload");
    }

    #[test]
    fn an_empty_subfolder_means_the_root_itself() {
        let f = Fixture::new("root");
        let source = f.source("cached", b"x");

        for subfolder in ["", "   ", "/"] {
            let placed = f
                .library
                .place(&source, subfolder, "Thing.lha", OverwritePolicy::Overwrite)
                .unwrap();
            assert_eq!(placed.path, f.library.root().join("Thing.lha"));
            assert_eq!(placed.subfolder, "");
        }
    }

    #[test]
    fn nested_subfolders_are_created_on_demand() {
        let f = Fixture::new("nested");
        let source = f.source("cached", b"x");

        let placed = f
            .library
            .place(&source, "games/wip", "Game.lha", OverwritePolicy::Skip)
            .unwrap();

        assert_eq!(placed.subfolder, "games/wip");
        assert!(placed.path.starts_with(f.library.root().join("games")));
    }

    // ---- the security half ----

    /// A subfolder name is free text that becomes a path.
    #[test]
    fn a_subfolder_cannot_escape_the_download_folder() {
        let f = Fixture::new("escape");
        let source = f.source("cached", b"x");

        for hostile in [
            "../outside",
            "../../Windows/System32",
            "games/../../outside",
            "\\..\\outside",
            "C:\\Windows",
            "C:/Windows",
        ] {
            let result = f
                .library
                .place(&source, hostile, "Evil.lha", OverwritePolicy::Overwrite);
            assert!(result.is_err(), "accepted subfolder {hostile:?}");
        }
    }

    /// Trimming a leading separator is a reading of what the user typed, not a
    /// repair of a traversal: the result is still inside the root.
    #[test]
    fn a_leading_separator_reads_as_a_subfolder_not_an_absolute_path() {
        let f = Fixture::new("leading");
        let source = f.source("cached", b"x");

        for typed in ["/browser", "browser/", "\\browser"] {
            let placed = f
                .library
                .place(&source, typed, "Thing.lha", OverwritePolicy::Overwrite)
                .unwrap_or_else(|e| panic!("refused {typed:?}: {e}"));
            assert_eq!(placed.subfolder, "browser");
            assert!(placed.path.starts_with(f.library.root()));
        }
    }

    #[test]
    fn a_file_name_that_is_really_a_path_is_refused() {
        let f = Fixture::new("name");
        let source = f.source("cached", b"x");

        for hostile in [
            "../evil.lha",
            "sub/evil.lha",
            "sub\\evil.lha",
            "..",
            "",
            "  ",
        ] {
            assert!(
                f.library
                    .place(&source, "", hostile, OverwritePolicy::Overwrite)
                    .is_err(),
                "accepted file name {hostile:?}"
            );
        }
    }

    /// Otherwise "move this download" would be a general-purpose file mover
    /// pointed at whatever path it was handed.
    #[test]
    fn relocating_a_file_outside_the_library_is_refused() {
        let f = Fixture::new("outside");
        let stranger = f.source("not-mine.txt", b"important");

        let err = f
            .library
            .relocate(&stranger, "somewhere", OverwritePolicy::Overwrite)
            .unwrap_err();

        assert_eq!(err.code(), "ART-SAFETY-REFUSED");
        assert!(stranger.exists(), "the file must not have been moved");
    }

    // ---- overwrite policy ----

    #[test]
    fn an_existing_file_is_kept_by_default() {
        let f = Fixture::new("skip");
        let source = f.source("cached", b"new");
        let existing = f.library.root().join("Thing.lha");
        std::fs::write(&existing, b"the user's own copy").unwrap();

        let placed = f
            .library
            .place(&source, "", "Thing.lha", OverwritePolicy::Skip)
            .unwrap();

        assert!(placed.skipped_existing);
        assert_eq!(std::fs::read(&existing).unwrap(), b"the user's own copy");
    }

    #[test]
    fn keeping_both_writes_a_numbered_name() {
        let f = Fixture::new("rename");
        let source = f.source("cached", b"new");
        std::fs::write(f.library.root().join("Thing.lha"), b"old").unwrap();

        let placed = f
            .library
            .place(&source, "", "Thing.lha", OverwritePolicy::Rename)
            .unwrap();

        assert!(placed.renamed);
        assert_eq!(placed.path.file_name().unwrap(), "Thing (1).lha");
        assert_eq!(
            std::fs::read(f.library.root().join("Thing.lha")).unwrap(),
            b"old"
        );
    }

    #[test]
    fn overwrite_replaces_only_when_asked() {
        let f = Fixture::new("over");
        let source = f.source("cached", b"new");
        let existing = f.library.root().join("Thing.lha");
        std::fs::write(&existing, b"old").unwrap();

        let placed = f
            .library
            .place(&source, "", "Thing.lha", OverwritePolicy::Overwrite)
            .unwrap();

        assert!(!placed.skipped_existing);
        assert_eq!(std::fs::read(&existing).unwrap(), b"new");
    }

    // ---- moving ----

    #[test]
    fn a_download_can_be_moved_to_another_subfolder_afterwards() {
        let f = Fixture::new("move");
        let source = f.source("cached", b"payload");
        let placed = f
            .library
            .place(&source, "inbox", "Thing.lha", OverwritePolicy::Skip)
            .unwrap();

        let moved = f
            .library
            .relocate(&placed.path, "browser", OverwritePolicy::Skip)
            .unwrap();

        assert_eq!(moved.subfolder, "browser");
        assert_eq!(std::fs::read(&moved.path).unwrap(), b"payload");
        assert!(
            !placed.path.exists(),
            "the original must not be left behind"
        );
    }

    #[test]
    fn moving_a_file_onto_itself_changes_nothing() {
        let f = Fixture::new("noop");
        let source = f.source("cached", b"payload");
        let placed = f
            .library
            .place(&source, "inbox", "Thing.lha", OverwritePolicy::Skip)
            .unwrap();

        let moved = f
            .library
            .relocate(&placed.path, "inbox", OverwritePolicy::Skip)
            .unwrap();

        assert_eq!(moved.path, placed.path);
        assert!(placed.path.exists());
    }

    #[test]
    fn moving_onto_an_existing_file_keeps_both_files() {
        let f = Fixture::new("moveclash");
        let source = f.source("cached", b"payload");
        let placed = f
            .library
            .place(&source, "inbox", "Thing.lha", OverwritePolicy::Skip)
            .unwrap();
        std::fs::create_dir_all(f.library.root().join("browser")).unwrap();
        std::fs::write(f.library.root().join("browser").join("Thing.lha"), b"other").unwrap();

        let moved = f
            .library
            .relocate(&placed.path, "browser", OverwritePolicy::Skip)
            .unwrap();

        assert!(moved.skipped_existing);
        assert!(
            placed.path.exists(),
            "the source must survive a refused move"
        );
        assert_eq!(
            std::fs::read(f.library.root().join("browser").join("Thing.lha")).unwrap(),
            b"other"
        );
    }

    // ---- the picker ----

    #[test]
    fn subfolders_are_listed_for_the_picker() {
        let f = Fixture::new("list");
        std::fs::create_dir_all(f.library.root().join("games/wip")).unwrap();
        std::fs::create_dir_all(f.library.root().join("browser")).unwrap();
        std::fs::write(f.library.root().join("loose.lha"), b"x").unwrap();

        let folders = f.library.subfolders().unwrap();
        assert_eq!(folders, vec!["browser", "games", "games/wip"]);
    }

    #[test]
    fn listing_a_missing_root_is_empty_rather_than_an_error() {
        let library = Library::new(std::env::temp_dir().join("art-library-does-not-exist"));
        assert!(library.subfolders().unwrap().is_empty());
    }

    /// ART-028's lesson, applied here before it can bite: the walk is bounded.
    #[test]
    fn the_subfolder_walk_is_depth_limited() {
        let f = Fixture::new("deep");
        let mut path = f.library.root().to_path_buf();
        for level in 0..12 {
            path = path.join(format!("level{level}"));
        }
        std::fs::create_dir_all(&path).unwrap();

        let folders = f.library.subfolders().unwrap();
        let deepest = folders
            .iter()
            .map(|f| f.matches('/').count() + 1)
            .max()
            .unwrap_or(0);
        assert!(deepest <= MAX_SUBFOLDER_DEPTH, "walked {deepest} deep");
    }
}
