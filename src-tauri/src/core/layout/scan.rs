//! Finding the things a layout is made of.
//!
//! Its own walk rather than `core/collection`'s: that one filters to five
//! extensions and carries no size, and this one has to stop at a WHDLoad
//! drawer instead of descending into it. What is shared is two rules — a depth
//! cap, and `symlink_metadata` so a Windows junction cannot make a cycle — and
//! six lines of rule is smaller than the parameterisation sharing them would
//! cost.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::core::error::{CoreError, CoreResult};

/// How deep a scan will descend.
///
/// The same cap and the same reason as `core/collection`: a symlink cycle plus
/// unbounded recursion overflows the stack, and with `panic = "abort"` that
/// takes the whole application down rather than reporting an error.
pub const MAX_SCAN_DEPTH: usize = 32;

/// One thing the layout will place.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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

/// How many dropped paths a scan names before it starts counting instead.
///
/// The same shape `CoreError::NonAsciiPfs3Names` already uses: a report that
/// only says "some things were dropped" answers nothing a user can act on, and
/// one that lists nine thousand of them is no better.
pub const MAX_REPORTED_PATHS: usize = 20;

/// Things a scan did not put in the plan, named rather than dropped in
/// silence (ART-107).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Dropped {
    /// The first [`MAX_REPORTED_PATHS`], in the order they were met.
    pub paths: Vec<PathBuf>,
    /// How many more there were beyond those — `0` when `paths` is all of
    /// them. The *total* is `paths.len() + more`, which is what a sentence
    /// should print.
    pub more: usize,
}

impl Dropped {
    fn push(&mut self, path: PathBuf) {
        if self.paths.len() < MAX_REPORTED_PATHS {
            self.paths.push(path);
        } else {
            self.more += 1;
        }
    }

    pub fn is_empty(&self) -> bool {
        self.paths.is_empty() && self.more == 0
    }

    /// How many there really were — never `paths.len()`, which is capped.
    pub fn total(&self) -> usize {
        self.paths.len() + self.more
    }
}

/// What a scan found, and what it did not.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Gathered {
    pub found: Vec<Found>,
    /// Directories ART did not look inside, because they sit at
    /// [`MAX_SCAN_DEPTH`]. Anything under them is absent from the plan.
    pub too_deep: Dropped,
    /// Sources dropped because something else in the same scan already
    /// covers them.
    pub duplicates: Dropped,
}

/// Everything under `paths`, with WHDLoad drawers kept whole.
///
/// # Two ways a scan used to quietly not describe what the user dropped
///
/// **The depth cap was silent** (ART-107). `walk` returns at
/// [`MAX_SCAN_DEPTH`] and `tree_bytes` returns `0` there, so files below the
/// cap were absent from the plan with nothing on screen saying so, and a
/// drawer's size could read low for the same reason. The copy path has always
/// done this correctly — `core::layout::apply::copy_tree` **refuses** past the
/// cap rather than truncating — and this now at least counts what it did not
/// look at, so the plan can say so. It still does not refuse: a scan is a
/// preview, and refusing to preview a folder because one corner of it is 33
/// levels down would be worse than showing the rest and naming the corner.
///
/// **Nothing deduped.** Dropping a folder and then a file inside it — both of
/// which the screen allows — put the same file in the plan twice, which then
/// collided with itself, and the only way out was to remove one of the
/// sources. The second sighting of a path is now dropped and recorded instead.
/// "The same path" is decided by [`std::fs::canonicalize`] where the OS will
/// answer, so a folder and a `..`-flavoured spelling of the same file are one
/// thing, and by the path as written where it will not.
pub fn gather(paths: &[PathBuf]) -> CoreResult<Gathered> {
    let mut scan = Scan::default();
    for path in paths {
        if path.is_dir() {
            if is_whdload_drawer(path) {
                let found = scan.drawer(path)?;
                scan.keep(found);
            } else {
                scan.walk(path, 0)?;
            }
        } else if path.is_file() {
            let found = file(path)?;
            scan.keep(found);
        } else {
            return Err(CoreError::InvalidInput(format!(
                "'{}' is neither a file nor a folder",
                path.display()
            )));
        }
    }
    Ok(scan.into_gathered())
}

/// One run of [`gather`]: what it has found, what it has dropped, and the set
/// of paths it has already seen.
///
/// A struct rather than three `&mut` arguments threaded through `walk` and
/// `tree_bytes` — the seen-set has to be shared across every source in the
/// same call for deduping to work at all, and four out-parameters is where a
/// recursive helper stops being readable.
#[derive(Default)]
struct Scan {
    found: Vec<Found>,
    too_deep: Dropped,
    duplicates: Dropped,
    seen: std::collections::HashSet<PathBuf>,
}

impl Scan {
    fn into_gathered(self) -> Gathered {
        Gathered {
            found: self.found,
            too_deep: self.too_deep,
            duplicates: self.duplicates,
        }
    }

    /// Keep `found` unless this scan already has that path.
    fn keep(&mut self, found: Found) {
        if self.seen.insert(identity(&found.path)) {
            self.found.push(found);
        } else {
            self.duplicates.push(found.path);
        }
    }

    fn walk(&mut self, dir: &Path, depth: usize) -> CoreResult<()> {
        if depth >= MAX_SCAN_DEPTH {
            self.too_deep.push(dir.to_path_buf());
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
                    let found = self.drawer(&path)?;
                    self.keep(found);
                } else {
                    self.walk(&path, depth + 1)?;
                }
            } else if path.is_file() {
                let found = file(&path)?;
                self.keep(found);
            }
        }
        Ok(())
    }

    fn drawer(&mut self, path: &Path) -> CoreResult<Found> {
        Ok(Found {
            path: path.to_path_buf(),
            bytes: self.tree_bytes(path, 0),
            is_dir: true,
        })
    }

    /// A drawer's whole tree. Stops at [`MAX_SCAN_DEPTH`] like everything else
    /// here, and **says so** rather than quietly reporting a size that is too
    /// small — the number on screen is what a user decides against.
    fn tree_bytes(&mut self, dir: &Path, depth: usize) -> u64 {
        if depth >= MAX_SCAN_DEPTH {
            self.too_deep.push(dir.to_path_buf());
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
                total += self.tree_bytes(&path, depth + 1);
            } else if let Ok(meta) = std::fs::metadata(&path) {
                total += meta.len();
            }
        }
        total
    }
}

/// The key two paths are the same file under.
///
/// `canonicalize` resolves `.`/`..`, a junction, and Windows' short 8.3 names,
/// which is what makes "a folder and a file inside it" recognisable as one
/// thing at all. A path the OS will not canonicalize — deleted between the
/// walk and here — falls back to itself: no worse than the old behaviour for
/// that one entry, and never an error for a scan that is otherwise fine.
fn identity(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

fn file(path: &Path) -> CoreResult<Found> {
    Ok(Found {
        path: path.to_path_buf(),
        bytes: std::fs::metadata(path)?.len(),
        is_dir: false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "art-layout-scan-{tag}-{}",
            crate::core::test_scratch_id()
        ));
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

        let mut found = gather(std::slice::from_ref(&dir)).unwrap().found;
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

        let found = gather(std::slice::from_ref(&dir)).unwrap().found;

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

        let found = gather(std::slice::from_ref(&dir)).unwrap().found;
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

        let found = gather(std::slice::from_ref(&file)).unwrap().found;
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

        let scanned = gather(std::slice::from_ref(&root)).unwrap();

        // The point is that it returned at all; the buried file is out of reach.
        assert!(scanned.found.is_empty(), "found {:?}", scanned.found);

        // **ART-107.** And it says so. The whole complaint was that the plan
        // came back short with nothing on screen admitting it.
        assert!(
            !scanned.too_deep.is_empty(),
            "the folder ART stopped at must be named"
        );
        assert_eq!(scanned.too_deep.total(), 1, "one branch, one report");
        assert!(
            scanned.too_deep.paths[0].starts_with(&root),
            "and it is the folder itself, not some invented path: {:?}",
            scanned.too_deep.paths[0]
        );

        let _ = std::fs::remove_dir_all(&root);
    }

    /// **ART-107, the size half.** A WHDLoad drawer deeper than the cap
    /// reports a total that is too small, and the plan says so rather than
    /// letting the user decide against a number that is quietly wrong.
    #[test]
    fn a_drawer_deeper_than_the_cap_says_its_size_is_short() {
        let root = scratch("deep-drawer");
        let game = root.join("TurricanII");
        std::fs::create_dir_all(&game).unwrap();
        std::fs::write(game.join("TurricanII.slave"), vec![0u8; 4]).unwrap();

        let mut deep = game.clone();
        for i in 0..(MAX_SCAN_DEPTH + 2) {
            deep = deep.join(format!("d{i}"));
        }
        std::fs::create_dir_all(&deep).unwrap();
        std::fs::write(deep.join("buried"), vec![0u8; 1000]).unwrap();

        let scanned = gather(std::slice::from_ref(&root)).unwrap();

        assert_eq!(scanned.found.len(), 1);
        assert_eq!(
            scanned.found[0].bytes, 4,
            "the buried 1000 bytes are past the cap, so they are not counted"
        );
        assert!(
            !scanned.too_deep.is_empty(),
            "and that is exactly why the short total must be reported"
        );

        let _ = std::fs::remove_dir_all(&root);
    }

    /// **ART-107, the duplicate half.** A folder and a file inside it are both
    /// things the screen lets a user add, and adding both used to put the file
    /// in the plan twice — where it then collided with itself, and the only
    /// way out was to remove one of the sources.
    #[test]
    fn a_file_inside_a_folder_that_was_also_added_is_kept_once() {
        let root = scratch("overlap");
        let games = root.join("Games");
        std::fs::create_dir_all(&games).unwrap();
        let one = games.join("Turrican.lha");
        std::fs::write(&one, vec![0u8; 12]).unwrap();
        std::fs::write(games.join("Zool.lha"), vec![0u8; 8]).unwrap();

        let scanned = gather(&[games.clone(), one.clone()]).unwrap();

        assert_eq!(
            scanned.found.len(),
            2,
            "two files, however many ways they were named: {:?}",
            scanned.found
        );
        assert_eq!(scanned.duplicates.total(), 1);
        assert_eq!(scanned.duplicates.paths[0], one);

        let _ = std::fs::remove_dir_all(&root);
    }

    /// The other order, because the first source seen is the one kept and a
    /// dedupe that only worked one way round would pass the test above.
    #[test]
    fn the_same_overlap_the_other_way_round_is_also_kept_once() {
        let root = scratch("overlap-reversed");
        let games = root.join("Games");
        std::fs::create_dir_all(&games).unwrap();
        let one = games.join("Turrican.lha");
        std::fs::write(&one, vec![0u8; 12]).unwrap();

        let scanned = gather(&[one.clone(), games.clone()]).unwrap();

        assert_eq!(scanned.found.len(), 1, "{:?}", scanned.found);
        assert_eq!(scanned.found[0].path, one);
        assert_eq!(scanned.duplicates.total(), 1);

        let _ = std::fs::remove_dir_all(&root);
    }

    /// Nothing to report when there is nothing to report — a plain scan must
    /// not start showing a "some things were dropped" panel to everyone.
    #[test]
    fn an_ordinary_scan_reports_nothing_dropped() {
        let dir = scratch("clean");
        std::fs::write(dir.join("a.adf"), vec![0u8; 10]).unwrap();

        let scanned = gather(std::slice::from_ref(&dir)).unwrap();
        assert_eq!(scanned.found.len(), 1);
        assert!(scanned.too_deep.is_empty());
        assert!(scanned.duplicates.is_empty());

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The report is bounded, and says how many it did not name. A folder
    /// tree with a hundred branches past the cap must not put a hundred paths
    /// on screen, and must not claim there were twenty.
    #[test]
    fn the_report_is_bounded_and_counts_the_rest() {
        let mut dropped = Dropped::default();
        for i in 0..(MAX_REPORTED_PATHS + 7) {
            dropped.push(PathBuf::from(format!("d{i}")));
        }
        assert_eq!(dropped.paths.len(), MAX_REPORTED_PATHS);
        assert_eq!(dropped.more, 7);
        assert_eq!(dropped.total(), MAX_REPORTED_PATHS + 7);
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

        let found = gather(std::slice::from_ref(&root)).unwrap().found;

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
