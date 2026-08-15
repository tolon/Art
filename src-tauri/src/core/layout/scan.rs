//! Finding the things a layout is made of.
//!
//! Its own walk rather than `core/collection`'s: that one filters to five
//! extensions and carries no size, and this one has to stop at a WHDLoad
//! drawer instead of descending into it. What is shared is two rules — a depth
//! cap, and `symlink_metadata` so a Windows junction cannot make a cycle — and
//! six lines of rule is smaller than the parameterisation sharing them would
//! cost.

use std::path::{Path, PathBuf};

use crate::core::error::{CoreError, CoreResult};

/// How deep a scan will descend.
///
/// The same cap and the same reason as `core/collection`: a symlink cycle plus
/// unbounded recursion overflows the stack, and with `panic = "abort"` that
/// takes the whole application down rather than reporting an error.
pub const MAX_SCAN_DEPTH: usize = 32;

/// One thing the layout will place.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Found {
    pub path: PathBuf,
    /// For a file, its size. For a drawer, its whole tree.
    pub bytes: u64,
    /// True only for a WHDLoad drawer — every other directory is walked
    /// through rather than returned.
    pub is_dir: bool,
}

/// Whether `dir` **is** a WHDLoad drawer: it directly holds a `.slave`.
///
/// One level, deliberately. `core/whdload::analyse` is not the right question
/// here: it reads an unpacked *archive's* entry list, where exactly one drawer
/// sits beside its own `.info`, and a folder holding fifty games is not that
/// shape — `pick_slave` would choose one and call the whole folder a game.
pub fn is_whdload_drawer(dir: &Path) -> bool {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return false;
    };
    entries.flatten().any(|entry| {
        entry
            .path()
            .extension()
            .map(|ext| ext.eq_ignore_ascii_case("slave"))
            .unwrap_or(false)
    })
}

/// Everything under `paths`, with WHDLoad drawers kept whole.
pub fn gather(paths: &[PathBuf]) -> CoreResult<Vec<Found>> {
    let mut out = Vec::new();
    for path in paths {
        if path.is_dir() {
            if is_whdload_drawer(path) {
                out.push(drawer(path)?);
            } else {
                walk(path, 0, &mut out)?;
            }
        } else if path.is_file() {
            out.push(file(path)?);
        } else {
            return Err(CoreError::InvalidInput(format!(
                "'{}' is neither a file nor a folder",
                path.display()
            )));
        }
    }
    Ok(out)
}

fn walk(dir: &Path, depth: usize, out: &mut Vec<Found>) -> CoreResult<()> {
    if depth >= MAX_SCAN_DEPTH {
        return Ok(());
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Ok(());
    };
    for entry in entries.flatten() {
        let path = entry.path();
        // `symlink_metadata` does not follow links, so a directory symlink
        // pointing back up the tree is skipped instead of followed.
        let is_symlink = std::fs::symlink_metadata(&path)
            .map(|m| m.file_type().is_symlink())
            .unwrap_or(false);
        if is_symlink {
            continue;
        }
        if path.is_dir() {
            if is_whdload_drawer(&path) {
                out.push(drawer(&path)?);
            } else {
                walk(&path, depth + 1, out)?;
            }
        } else if path.is_file() {
            out.push(file(&path)?);
        }
    }
    Ok(())
}

fn file(path: &Path) -> CoreResult<Found> {
    Ok(Found {
        path: path.to_path_buf(),
        bytes: std::fs::metadata(path)?.len(),
        is_dir: false,
    })
}

fn drawer(path: &Path) -> CoreResult<Found> {
    Ok(Found {
        path: path.to_path_buf(),
        bytes: tree_bytes(path, 0),
        is_dir: true,
    })
}

fn tree_bytes(dir: &Path, depth: usize) -> u64 {
    if depth >= MAX_SCAN_DEPTH {
        return 0;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return 0;
    };
    let mut total = 0;
    for entry in entries.flatten() {
        let path = entry.path();
        if std::fs::symlink_metadata(&path)
            .map(|m| m.file_type().is_symlink())
            .unwrap_or(false)
        {
            continue;
        }
        if path.is_dir() {
            total += tree_bytes(&path, depth + 1);
        } else if let Ok(meta) = std::fs::metadata(&path) {
            total += meta.len();
        }
    }
    total
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(tag: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("art-layout-scan-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// A folder is walked and every file in it becomes a `Found`, at any depth.
    #[test]
    fn a_folder_is_walked_and_its_files_are_found() {
        let dir = scratch("walk");
        std::fs::create_dir_all(dir.join("sub")).unwrap();
        std::fs::write(dir.join("a.adf"), vec![0u8; 10]).unwrap();
        std::fs::write(dir.join("sub").join("b.lha"), vec![0u8; 20]).unwrap();

        let mut found = gather(std::slice::from_ref(&dir)).unwrap();
        found.sort_by(|a, b| a.path.cmp(&b.path));

        assert_eq!(found.len(), 2, "{found:?}");
        assert_eq!(found[0].path, dir.join("a.adf"));
        assert_eq!(found[0].bytes, 10);
        assert!(!found[0].is_dir);
        assert_eq!(found[1].bytes, 20);

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// **The one non-obvious rule.** A folder that directly holds a `.slave` is
    /// the game, and is returned whole rather than descended into — otherwise
    /// dropping a folder of 400 files scatters the insides of every game.
    #[test]
    fn a_whdload_drawer_is_returned_whole_and_never_walked_into() {
        let dir = scratch("drawer");
        let game = dir.join("TurricanII");
        std::fs::create_dir_all(game.join("data")).unwrap();
        std::fs::write(game.join("TurricanII.slave"), vec![0u8; 4]).unwrap();
        std::fs::write(game.join("data").join("level1"), vec![0u8; 6]).unwrap();

        let found = gather(std::slice::from_ref(&dir)).unwrap();

        assert_eq!(
            found.len(),
            1,
            "the drawer is one thing, not three: {found:?}"
        );
        assert_eq!(found[0].path, game);
        assert!(found[0].is_dir);
        assert_eq!(found[0].bytes, 10, "a drawer measures its whole tree");

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A folder of games gives one entry per game, not one entry for the folder.
    #[test]
    fn a_folder_of_drawers_gives_one_entry_per_drawer() {
        let dir = scratch("many");
        for name in ["TurricanII", "Zool"] {
            let game = dir.join(name);
            std::fs::create_dir_all(&game).unwrap();
            std::fs::write(game.join(format!("{name}.slave")), vec![0u8; 4]).unwrap();
        }

        let found = gather(std::slice::from_ref(&dir)).unwrap();
        assert_eq!(found.len(), 2, "{found:?}");
        assert!(found.iter().all(|f| f.is_dir));

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A file given directly is found as itself, folder or not.
    #[test]
    fn a_file_named_directly_is_found_as_itself() {
        let dir = scratch("direct");
        let file = dir.join("one.adf");
        std::fs::write(&file, vec![0u8; 7]).unwrap();

        let found = gather(std::slice::from_ref(&file)).unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].path, file);
        assert_eq!(found[0].bytes, 7);

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// `is_whdload_drawer` looks one level down and no further: a folder whose
    /// *child* holds a slave is a folder of games, not a game.
    #[test]
    fn only_a_slave_directly_inside_makes_a_drawer() {
        let dir = scratch("depth");
        let outer = dir.join("Games");
        let inner = outer.join("Zool");
        std::fs::create_dir_all(&inner).unwrap();
        std::fs::write(inner.join("Zool.slave"), b"x").unwrap();

        assert!(!is_whdload_drawer(&outer));
        assert!(is_whdload_drawer(&inner));

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A directory tree deeper than the scan limit stops cleanly rather than
    /// recursing until the stack overflows (which aborts the process, since
    /// the release profile sets `panic = "abort"`). Mirrors
    /// `core::collection`'s `scanning_stops_at_the_depth_limit`.
    #[test]
    fn scanning_stops_at_the_depth_limit() {
        let root = scratch("deep");

        // Build a tree twice as deep as the limit, with a file at the bottom.
        let mut deep = root.clone();
        for i in 0..(MAX_SCAN_DEPTH * 2) {
            deep = deep.join(format!("d{i}"));
        }
        std::fs::create_dir_all(&deep).unwrap();
        std::fs::write(deep.join("buried.adf"), b"x").unwrap();

        let found = gather(std::slice::from_ref(&root)).unwrap();

        // The point is that it returned at all; the buried file is out of reach.
        assert!(found.is_empty(), "found {found:?}");

        let _ = std::fs::remove_dir_all(&root);
    }

    /// A drawer nested two folders down is still returned whole — the walk
    /// keeps recursing through plain directories until it meets one.
    #[test]
    fn a_drawer_nested_two_levels_down_is_still_returned_whole() {
        let root = scratch("nested-drawer");
        let game = root.join("Games").join("TurricanII");
        std::fs::create_dir_all(game.join("data")).unwrap();
        std::fs::write(game.join("TurricanII.slave"), vec![0u8; 4]).unwrap();
        std::fs::write(game.join("data").join("level1"), vec![0u8; 6]).unwrap();

        let found = gather(std::slice::from_ref(&root)).unwrap();

        assert_eq!(found.len(), 1, "the drawer is one thing: {found:?}");
        assert_eq!(found[0].path, game);
        assert!(found[0].is_dir);
        assert!(
            !found.iter().any(|f| f.path.ends_with("level1")),
            "the file inside the drawer must not come back as its own entry: {found:?}"
        );

        let _ = std::fs::remove_dir_all(&root);
    }
}
