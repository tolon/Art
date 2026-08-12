//! A walkable tree over an archive's flat list of names.
//!
//! An archive has no directories. It has a list of entries called things like
//! `Tools/Shell.lha`, and the folders in that name exist only because the name
//! says so. A commander pane needs the opposite: "what is in *this* folder,
//! and what can I walk into". This module is that translation, and it is pure
//! — it never touches the filesystem, so nothing here can be tricked into
//! writing anywhere.
//!
//! **This tree is for looking at, never for landing on.** A name that reached
//! it came out of a file somebody else wrote, so extraction still goes through
//! [`extract`](super::extract) and `safe_join` decides what a name means on
//! disk. What this module does with a hostile name is refuse to *show* it as
//! navigable, so the user is never offered a folder that cannot exist.
//!
//! Three things about real archives that this has to survive:
//!
//! - **Folders that are only implied.** A ZIP may hold `Tools/Shell.lha` and
//!   no `Tools/` entry at all. `Tools` still has to appear as a folder.
//! - **Both separators.** A ZIP written on Windows uses `\`, 7z and LHA use
//!   `/`, and one archive can contain both.
//! - **Names that are not paths.** `../escape`, `C:\Windows\x`, `.`, an empty
//!   name, a name that is only separators. Each is reported, not shown.

use std::collections::BTreeMap;

use super::ArchiveEntry;

/// How deep a name may nest before ART stops treating it as a path. A real
/// archive is a handful of levels; a crafted one can be thousands, and every
/// level is a directory the UI would have to hold.
pub const MAX_TREE_DEPTH: usize = 64;

/// One row of a listing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TreeEntry {
    pub name: String,
    pub is_dir: bool,
    pub bytes: u64,
    /// Where this file sits in the backend's entry list, which is what
    /// reading it needs. `None` for a directory — including one that exists
    /// only because something inside it does.
    pub index: Option<usize>,
}

/// An archive's names, arranged as folders.
#[derive(Debug, Clone, Default)]
pub struct ArchiveTree {
    /// Slash-separated directory path (`""` is the root) → its children,
    /// folders first then files, each side by name.
    dirs: BTreeMap<String, Vec<TreeEntry>>,
    /// Names that are not paths, kept so the pane can say how many entries it
    /// is not showing rather than quietly holding some back.
    refused: Vec<String>,
    /// Entries dropped because a name already claimed that place in the tree.
    duplicates: usize,
}

/// Split a raw archive name into components, or refuse it.
///
/// Refuses — rather than sanitising — anything that is not a plain relative
/// path: a component of `.` or `..`, an absolute path, a drive letter, an
/// empty name. Sanitising is what turns `..\..\Startup` into a harmless
/// looking `_.._..Startup`, and then the user is looking at a file that is not
/// what the archive says it is.
fn components(name: &str) -> Option<Vec<String>> {
    if name.is_empty() {
        return None;
    }
    // A Windows drive letter or a UNC path is absolute, whatever follows.
    let bytes = name.as_bytes();
    if bytes[0] == b'/' || bytes[0] == b'\\' {
        return None;
    }
    if name.len() >= 2 && bytes[1] == b':' {
        return None;
    }

    let parts: Vec<String> = name
        .split(['/', '\\'])
        // A trailing separator is how most formats mark a directory, so an
        // empty last component is expected rather than hostile.
        .filter(|part| !part.is_empty())
        .map(|part| part.to_string())
        .collect();

    if parts.is_empty() || parts.len() > MAX_TREE_DEPTH {
        return None;
    }
    if parts.iter().any(|part| part == "." || part == "..") {
        return None;
    }
    Some(parts)
}

impl ArchiveTree {
    /// Arrange `entries` — the backend's list, in its own order — as folders.
    pub fn build(entries: &[ArchiveEntry]) -> Self {
        let mut tree = Self::default();
        // Every directory that exists, whether the archive said so or not.
        let mut children: BTreeMap<String, BTreeMap<String, TreeEntry>> = BTreeMap::new();
        children.insert(String::new(), BTreeMap::new());

        for (index, entry) in entries.iter().enumerate() {
            let Some(parts) = components(&entry.name) else {
                tree.refused.push(entry.name.clone());
                continue;
            };

            // Every component but the last is a folder on the way, created
            // whether or not the archive holds an entry for it.
            let mut path = String::new();
            for part in parts.iter().take(parts.len() - 1) {
                let parent = path.clone();
                if !path.is_empty() {
                    path.push('/');
                }
                path.push_str(part);

                children.entry(path.clone()).or_default();
                children.entry(parent).or_default().insert(
                    part.clone(),
                    TreeEntry {
                        name: part.clone(),
                        is_dir: true,
                        bytes: 0,
                        index: None,
                    },
                );
            }

            let leaf = parts.last().expect("checked non-empty");
            let parent = path.clone();
            if entry.is_dir {
                let full = if parent.is_empty() {
                    leaf.clone()
                } else {
                    format!("{parent}/{leaf}")
                };
                children.entry(full).or_default();
            }

            let row = TreeEntry {
                name: leaf.clone(),
                is_dir: entry.is_dir,
                bytes: entry.declared_bytes,
                index: if entry.is_dir { None } else { Some(index) },
            };
            let slot = children.entry(parent).or_default();
            match slot.get(leaf) {
                // A folder already implied by something inside it is not a
                // duplicate of the archive's own entry for it.
                Some(existing) if existing.is_dir && entry.is_dir => {}
                Some(_) => tree.duplicates += 1,
                None => {
                    slot.insert(leaf.clone(), row);
                }
            }
        }

        tree.dirs = children
            .into_iter()
            .map(|(path, rows)| {
                let mut rows: Vec<TreeEntry> = rows.into_values().collect();
                // Folders first, then by name — the one listing order the
                // commander uses everywhere else.
                rows.sort_by(|a, b| {
                    b.is_dir
                        .cmp(&a.is_dir)
                        .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
                });
                (path, rows)
            })
            .collect();
        tree
    }

    /// The rows inside `dir` (`""` for the root), or `None` if there is no
    /// such folder — which is what a caller asking for a made-up path gets.
    pub fn list(&self, dir: &str) -> Option<&[TreeEntry]> {
        self.dirs.get(dir.trim_matches('/')).map(|rows| &rows[..])
    }

    /// Whether `dir` names a folder in this tree.
    pub fn has_dir(&self, dir: &str) -> bool {
        self.dirs.contains_key(dir.trim_matches('/'))
    }

    /// The names this tree would not show, because they are not paths.
    pub fn refused(&self) -> &[String] {
        &self.refused
    }

    /// How many entries lost their place to a name already there.
    pub fn duplicates(&self) -> usize {
        self.duplicates
    }

    /// One row's entry index, looked up by the folder it is in and its name —
    /// never by a path the caller assembled, so a name cannot arrive already
    /// joined to something else.
    pub fn index_of(&self, dir: &str, name: &str) -> Option<usize> {
        self.list(dir)?
            .iter()
            .find(|row| row.name == name)
            .and_then(|row| row.index)
    }

    /// Every file inside `dir` and below it, with the path it should keep
    /// relative to `dir`.
    ///
    /// This is what "copy this folder out" means for an archive: the subtree's
    /// own shape, with the folder itself as the root, so the caller writes
    /// `Tools/Shell.lha` under wherever it puts `Tools`.
    pub fn subtree(&self, dir: &str) -> Vec<(usize, String)> {
        let mut out = Vec::new();
        let mut queue = vec![(dir.trim_matches('/').to_string(), String::new())];
        while let Some((path, relative)) = queue.pop() {
            let Some(rows) = self.list(&path) else {
                continue;
            };
            for row in rows {
                let child_relative = if relative.is_empty() {
                    row.name.clone()
                } else {
                    format!("{relative}/{}", row.name)
                };
                if row.is_dir {
                    let child_path = if path.is_empty() {
                        row.name.clone()
                    } else {
                        format!("{path}/{}", row.name)
                    };
                    queue.push((child_path, child_relative));
                } else if let Some(index) = row.index {
                    out.push((index, child_relative));
                }
            }
        }
        out.sort_by(|a, b| a.1.cmp(&b.1));
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(name: &str, bytes: u64) -> ArchiveEntry {
        ArchiveEntry {
            name: name.to_string(),
            is_dir: false,
            declared_bytes: bytes,
        }
    }

    fn dir(name: &str) -> ArchiveEntry {
        ArchiveEntry {
            name: name.to_string(),
            is_dir: true,
            declared_bytes: 0,
        }
    }

    fn names(tree: &ArchiveTree, path: &str) -> Vec<String> {
        tree.list(path)
            .unwrap_or(&[])
            .iter()
            .map(|row| format!("{}{}", row.name, if row.is_dir { "/" } else { "" }))
            .collect()
    }

    /// The case that makes this module necessary: a folder nothing declared.
    #[test]
    fn a_folder_that_only_exists_because_a_file_is_in_it_still_appears() {
        let tree = ArchiveTree::build(&[file("Tools/Shell.lha", 21), file("ReadMe.txt", 5)]);

        assert_eq!(names(&tree, ""), ["Tools/", "ReadMe.txt"]);
        assert_eq!(names(&tree, "Tools"), ["Shell.lha"]);
        assert!(tree.has_dir("Tools"));
    }

    /// Deeper, and with the archive's own directory entry present as well —
    /// which must not turn into a second `Tools` beside the implied one.
    #[test]
    fn a_declared_folder_and_an_implied_one_are_the_same_folder() {
        let tree = ArchiveTree::build(&[
            dir("Tools/"),
            file("Tools/Shell.lha", 1),
            file("Tools/Sub/Deep.txt", 2),
        ]);

        assert_eq!(names(&tree, ""), ["Tools/"]);
        assert_eq!(names(&tree, "Tools"), ["Sub/", "Shell.lha"]);
        assert_eq!(names(&tree, "Tools/Sub"), ["Deep.txt"]);
        assert_eq!(tree.duplicates(), 0);
    }

    /// A ZIP written on Windows uses `\`, and one archive can hold both.
    #[test]
    fn both_separators_lead_to_the_same_folder() {
        let tree = ArchiveTree::build(&[file(r"Tools\Shell.lha", 1), file("Tools/Other.txt", 2)]);

        assert_eq!(names(&tree, ""), ["Tools/"]);
        assert_eq!(names(&tree, "Tools"), ["Other.txt", "Shell.lha"]);
    }

    /// Hostile names are not shown as folders to walk into. They are counted,
    /// because an archive that lists ten entries and shows seven without
    /// saying so is worse than one that explains itself.
    #[test]
    fn names_that_are_not_paths_are_refused_and_counted() {
        let tree = ArchiveTree::build(&[
            file("../../outside.txt", 1),
            file("../sibling.txt", 1),
            file(r"C:\Windows\Temp\absolute.txt", 1),
            file("/etc/passwd", 1),
            file("", 1),
            file("./same.txt", 1),
            file("ok.txt", 1),
        ]);

        assert_eq!(names(&tree, ""), ["ok.txt"]);
        assert_eq!(tree.refused().len(), 6, "{:?}", tree.refused());
        assert!(!tree.has_dir(".."));
    }

    #[test]
    fn a_name_nested_past_the_depth_cap_is_refused() {
        let deep = vec!["a"; MAX_TREE_DEPTH + 1].join("/");
        let tree = ArchiveTree::build(&[file(&deep, 1), file("ok.txt", 1)]);

        assert_eq!(names(&tree, ""), ["ok.txt"]);
        assert_eq!(tree.refused().len(), 1);
    }

    /// Two entries with the same name in the same folder: the first keeps the
    /// place, and the pane can say one is hidden. Silently showing one of two
    /// identically named rows is how a user copies out the wrong file.
    #[test]
    fn a_duplicate_name_is_kept_out_and_counted() {
        let tree = ArchiveTree::build(&[file("hi.txt", 1), file("hi.txt", 2)]);

        assert_eq!(names(&tree, ""), ["hi.txt"]);
        assert_eq!(tree.list("").unwrap()[0].bytes, 1, "the first one wins");
        assert_eq!(tree.duplicates(), 1);
    }

    /// The index is what reading a file needs, and it must survive the
    /// rearranging: row order in the tree is nothing like entry order in the
    /// archive.
    #[test]
    fn a_rows_index_still_points_at_its_own_entry() {
        let entries = [
            file("Zed.txt", 1),
            file("Tools/Shell.lha", 2),
            file("Alpha.txt", 3),
        ];
        let tree = ArchiveTree::build(&entries);

        assert_eq!(tree.index_of("", "Zed.txt"), Some(0));
        assert_eq!(tree.index_of("Tools", "Shell.lha"), Some(1));
        assert_eq!(tree.index_of("", "Alpha.txt"), Some(2));
        assert_eq!(tree.index_of("", "Tools"), None, "a folder has no entry");
        assert_eq!(tree.index_of("", "nope.txt"), None);
    }

    #[test]
    fn a_subtree_carries_the_shape_below_it() {
        let tree = ArchiveTree::build(&[
            file("Tools/Shell.lha", 1),
            file("Tools/Sub/Deep.txt", 2),
            file("Elsewhere.txt", 3),
        ]);

        let subtree = tree.subtree("Tools");
        assert_eq!(
            subtree,
            vec![
                (0, "Shell.lha".to_string()),
                (1, "Sub/Deep.txt".to_string())
            ]
        );

        // And the whole archive, from the root.
        let all = tree.subtree("");
        assert_eq!(all.len(), 3);
        assert!(all.iter().any(|(_, path)| path == "Tools/Sub/Deep.txt"));
    }

    #[test]
    fn a_folder_that_is_not_there_lists_as_nothing_rather_than_panicking() {
        let tree = ArchiveTree::build(&[file("ok.txt", 1)]);
        assert!(tree.list("Nope").is_none());
        assert!(tree.subtree("Nope").is_empty());
    }
}
