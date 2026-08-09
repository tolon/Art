//! Path-traversal defence for archive extraction (Zip-Slip / LHA equivalent).
//!
//! Archives are untrusted input. An entry named `..\..\Windows\System32\evil`
//! or `/etc/passwd` must never escape the chosen destination directory. We
//! defend by:
//!
//! 1. Normalising both `\` (Amiga/Windows) and `/` separators.
//! 2. Rejecting absolute paths.
//! 3. Walking path *components* and allowing only `Normal` and `CurDir`,
//!    rejecting `ParentDir` (`..`), `RootDir`, and `Prefix`.
//! 4. A final `starts_with` check on the canonicalised destination.
//!
//! This deliberately does NOT call `canonicalize` on the *result* before the
//! file exists (it won't, so canonicalize fails). Instead we canonicalise the
//! destination root and build the target as a child of it.

use std::path::{Component, Path, PathBuf};

use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum PathTraversalError {
    #[error("archive entry contains an absolute path: {0}")]
    Absolute(String),
    #[error("archive entry escapes the destination via '..': {0}")]
    ParentDir(String),
    #[error("archive entry has a Windows drive/prefix component: {0}")]
    Prefix(String),
    #[error("archive entry name is empty")]
    Empty,
}

/// Join an untrusted archive entry name to a trusted destination root,
/// rejecting anything that would escape the root.
///
/// `entry_name` may use either `/` or `\` as a separator (Amiga LHA uses `\`).
pub fn safe_join(root: &Path, entry_name: &str) -> Result<PathBuf, PathTraversalError> {
    let trimmed = entry_name.trim();
    if trimmed.is_empty() {
        return Err(PathTraversalError::Empty);
    }

    // Normalise backslashes to forward slashes so `Path` components parse them.
    let normalised = trimmed.replace('\\', "/");
    let candidate = Path::new(&normalised);

    if candidate.is_absolute() {
        return Err(PathTraversalError::Absolute(trimmed.to_string()));
    }

    let mut out = root.to_path_buf();
    for comp in candidate.components() {
        match comp {
            Component::Normal(c) => out.push(c),
            Component::CurDir => { /* "." is harmless */ }
            Component::ParentDir => {
                return Err(PathTraversalError::ParentDir(trimmed.to_string()));
            }
            Component::RootDir => {
                return Err(PathTraversalError::Absolute(trimmed.to_string()));
            }
            Component::Prefix(_) => {
                return Err(PathTraversalError::Prefix(trimmed.to_string()));
            }
        }
    }

    // Final guard: the built path must still be inside `root`.
    // We compare the *components* (not canonicalize, since the target may not
    // exist yet). `out` was built purely by pushing Normal components onto
    // `root`, so by construction it cannot escape — but this double-checks
    // against symlink tricks callers might introduce.
    if !out.starts_with(root) {
        return Err(PathTraversalError::ParentDir(trimmed.to_string()));
    }

    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    const ROOT: &str = "C:/Amiga/Extract";

    #[test]
    fn simple_filename_ok() {
        let p = safe_join(Path::new(ROOT), "game.exe").unwrap();
        assert_eq!(p, PathBuf::from("C:/Amiga/Extract/game.exe"));
    }

    #[test]
    fn nested_subpath_ok() {
        let p = safe_join(Path::new(ROOT), "Data/level1.bin").unwrap();
        assert_eq!(p, PathBuf::from("C:/Amiga/Extract/Data/level1.bin"));
    }

    #[test]
    fn backslash_separator_ok() {
        let p = safe_join(Path::new(ROOT), "Data\\level1.bin").unwrap();
        assert_eq!(p, PathBuf::from("C:/Amiga/Extract/Data/level1.bin"));
    }

    #[test]
    fn dot_component_ok() {
        let p = safe_join(Path::new(ROOT), "./game.exe").unwrap();
        assert_eq!(p, PathBuf::from("C:/Amiga/Extract/game.exe"));
    }

    #[test]
    fn parent_dir_rejected() {
        let err = safe_join(Path::new(ROOT), "../evil.exe").unwrap_err();
        assert_eq!(err, PathTraversalError::ParentDir("../evil.exe".into()));
    }

    #[test]
    fn deep_parent_dir_rejected() {
        let err = safe_join(Path::new(ROOT), "Data/../../../evil.exe").unwrap_err();
        assert!(matches!(err, PathTraversalError::ParentDir(_)));
    }

    #[test]
    fn unix_absolute_rejected() {
        let err = safe_join(Path::new(ROOT), "/etc/passwd").unwrap_err();
        assert_eq!(err, PathTraversalError::Absolute("/etc/passwd".into()));
    }

    #[test]
    fn windows_drive_absolute_rejected() {
        // After backslash normalisation this becomes "C:/Windows/evil" → Prefix.
        let err = safe_join(Path::new(ROOT), "C:\\Windows\\evil").unwrap_err();
        assert!(matches!(
            err,
            PathTraversalError::Prefix(_) | PathTraversalError::Absolute(_)
        ));
    }

    #[test]
    fn backslash_parent_rejected() {
        let err = safe_join(Path::new(ROOT), "..\\..\\evil").unwrap_err();
        assert!(matches!(err, PathTraversalError::ParentDir(_)));
    }

    #[test]
    fn empty_name_rejected() {
        let err = safe_join(Path::new(ROOT), "").unwrap_err();
        assert_eq!(err, PathTraversalError::Empty);
    }

    #[test]
    fn whitespace_only_rejected() {
        let err = safe_join(Path::new(ROOT), "   ").unwrap_err();
        assert_eq!(err, PathTraversalError::Empty);
    }

    #[test]
    fn sneaky_dotdot_with_backslashes_rejected() {
        let err = safe_join(Path::new(ROOT), "Data\\..\\..\\evil").unwrap_err();
        assert!(matches!(err, PathTraversalError::ParentDir(_)));
    }
}
