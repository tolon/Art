//! The two-pane file manager's host-side half.
//!
//! One pane can show a local folder, an ADF image or an HDF image, so the
//! frontend needs a *uniform* listing regardless of which. That shape is
//! [`PanelEntry`]: name, whether it is a folder, a size, and the one identifier
//! needed to open or read it — a path for local files, a header block for ADF
//! entries.
//!
//! ADF listing, extraction and writing already have commands of their own
//! (`commands/adf.rs`); this module only adds what a file manager needs on top:
//! listing the host filesystem, and copying an ADF entry straight out to disk
//! without routing megabytes through the webview.
//!
//! ## HDF shows partitions, not files
//!
//! Deliberately. Writing — or even listing — inside an HDF partition means
//! implementing its filesystem, and PFS3/SFS are not implemented while FFS is
//! bound to the floppy layout. The pane reports what ART can actually read: the
//! partition table. Showing an empty file list instead would imply the disk is
//! empty (§89).

use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::core::error::{CoreError, CoreResult};
use crate::error::AppResult;

/// The most entries returned for one folder.
///
/// A directory with a hundred thousand files is a real thing on a host disk,
/// and rendering all of it helps nobody.
const MAX_ENTRIES: usize = 5000;

/// One row in a pane, whatever the pane is showing.
#[derive(Debug, Clone, Serialize)]
pub struct PanelEntry {
    pub name: String,
    pub is_dir: bool,
    pub bytes: u64,
    /// Full path, for a local entry.
    pub path: Option<String>,
    /// Header block, for an ADF entry.
    pub header_block: Option<u32>,
    /// True when the entry is a symlink or junction. Reported, never followed —
    /// that is the ART-028 lesson.
    pub is_link: bool,
    /// Last-modified time, Unix seconds. `None` when the source could not
    /// report one — a sort by date must know the difference between "no
    /// date" and "epoch", so this stays optional rather than defaulting to 0.
    pub date: Option<i64>,
}

/// A local folder's contents plus where it sits.
#[derive(Debug, Clone, Serialize)]
pub struct LocalListing {
    pub path: String,
    /// The parent folder, or `None` at a drive root.
    pub parent: Option<String>,
    pub entries: Vec<PanelEntry>,
    /// True when the folder held more than ART will list.
    pub truncated: bool,
}

/// List a local folder for a pane.
#[tauri::command]
pub fn panel_list_local(path: String) -> AppResult<LocalListing> {
    let dir = PathBuf::from(path.trim());
    if !dir.is_dir() {
        return Err(CoreError::InvalidInput(format!("'{}' is not a folder", dir.display())).into());
    }

    Ok(list_local(&dir)?)
}

fn list_local(dir: &Path) -> CoreResult<LocalListing> {
    let mut entries = Vec::new();
    let mut truncated = false;

    for entry in std::fs::read_dir(dir)?.flatten() {
        if entries.len() >= MAX_ENTRIES {
            truncated = true;
            break;
        }
        // `symlink_metadata` does not follow the link, so a junction pointing
        // back up its own tree is listed rather than walked into.
        let Ok(meta) = std::fs::symlink_metadata(entry.path()) else {
            continue;
        };
        let is_link = meta.file_type().is_symlink();
        let is_dir = if is_link {
            // A link's target may be a folder; asking costs one stat and makes
            // the icon right without following it for listing.
            std::fs::metadata(entry.path())
                .map(|m| m.is_dir())
                .unwrap_or(false)
        } else {
            meta.is_dir()
        };

        entries.push(PanelEntry {
            name: entry.file_name().to_string_lossy().to_string(),
            is_dir,
            bytes: if is_dir { 0 } else { meta.len() },
            path: Some(entry.path().to_string_lossy().to_string()),
            header_block: None,
            is_link,
            date: mtime_unix(&meta),
        });
    }

    // Folders first, then by name — the order every file manager uses, and the
    // one that makes navigating with the keyboard predictable.
    entries.sort_by(|a, b| {
        b.is_dir
            .cmp(&a.is_dir)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });

    Ok(LocalListing {
        path: dir.to_string_lossy().to_string(),
        parent: dir.parent().map(|p| p.to_string_lossy().to_string()),
        entries,
        truncated,
    })
}

/// A file's last-modified time as Unix seconds, or `None` when the platform
/// could not report one (some filesystems have no modified time at all).
fn mtime_unix(meta: &std::fs::Metadata) -> Option<i64> {
    meta.modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64)
}

/// The places a pane can start from: drives, and the usual folders.
#[tauri::command]
pub fn panel_local_roots() -> AppResult<Vec<String>> {
    let mut roots = Vec::new();

    // Windows has no single filesystem root, so the drive letters are the
    // starting points. Probing is the only portable way to find them.
    #[cfg(windows)]
    for letter in b'A'..=b'Z' {
        let drive = format!("{}:\\", letter as char);
        if Path::new(&drive).is_dir() {
            roots.push(drive);
        }
    }

    #[cfg(not(windows))]
    roots.push("/".to_string());

    Ok(roots)
}

/// Where an ADF entry was written on the host.
#[derive(Debug, Clone, Serialize)]
pub struct ExtractedTo {
    pub path: String,
    pub bytes: u64,
    /// True when a file was already there and was left alone.
    pub skipped_existing: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("art-panel-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn folders_come_first_then_names_in_order() {
        let dir = scratch("order");
        std::fs::write(dir.join("zebra.txt"), b"z").unwrap();
        std::fs::write(dir.join("Apple.txt"), b"a").unwrap();
        std::fs::create_dir(dir.join("Tools")).unwrap();
        std::fs::create_dir(dir.join("assets")).unwrap();

        let listing = list_local(&dir).unwrap();
        let names: Vec<&str> = listing.entries.iter().map(|e| e.name.as_str()).collect();

        assert_eq!(names, vec!["assets", "Tools", "Apple.txt", "zebra.txt"]);
        assert!(listing.entries[0].is_dir);
        assert!(!listing.entries.last().unwrap().is_dir);

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_listing_knows_its_parent() {
        let dir = scratch("parent");
        let child = dir.join("inside");
        std::fs::create_dir(&child).unwrap();

        let listing = list_local(&child).unwrap();
        assert_eq!(listing.parent.as_deref(), Some(dir.to_str().unwrap()));

        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// A sort by date has to know a real date from a missing one — see the
    /// `Option<i64>` comment on `PanelEntry::date` — so this proves the local
    /// source actually reports one rather than always coming back `None`.
    #[test]
    fn a_freshly_written_file_has_a_recent_date() {
        let dir = scratch("date");
        let before = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
        std::fs::write(dir.join("new.txt"), b"x").unwrap();

        let listing = list_local(&dir).unwrap();
        let file = listing
            .entries
            .iter()
            .find(|e| e.name == "new.txt")
            .unwrap();
        let date = file
            .date
            .expect("a freshly written file has a modified time");

        assert!(
            date >= before - 2,
            "date {date} looks stale next to {before}"
        );

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn file_sizes_are_reported_and_folders_are_zero() {
        let dir = scratch("sizes");
        std::fs::write(dir.join("data.bin"), vec![0u8; 1234]).unwrap();
        std::fs::create_dir(dir.join("sub")).unwrap();

        let listing = list_local(&dir).unwrap();
        let file = listing
            .entries
            .iter()
            .find(|e| e.name == "data.bin")
            .unwrap();
        let folder = listing.entries.iter().find(|e| e.name == "sub").unwrap();

        assert_eq!(file.bytes, 1234);
        assert_eq!(folder.bytes, 0);

        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// A folder with more files than ART will list must say so, rather than
    /// silently showing a prefix as if it were everything.
    #[test]
    fn an_enormous_folder_is_truncated_and_says_so() {
        let dir = scratch("many");
        for i in 0..(MAX_ENTRIES + 10) {
            std::fs::write(dir.join(format!("f{i}")), b"x").unwrap();
        }

        let listing = list_local(&dir).unwrap();
        assert!(listing.truncated);
        assert_eq!(listing.entries.len(), MAX_ENTRIES);

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn listing_something_that_is_not_a_folder_is_refused() {
        let dir = scratch("notdir");
        let file = dir.join("a.txt");
        std::fs::write(&file, b"x").unwrap();

        assert!(panel_list_local(file.to_string_lossy().to_string()).is_err());
        assert!(panel_list_local("".into()).is_err());

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn there_is_always_somewhere_to_start() {
        let roots = panel_local_roots().unwrap();
        assert!(!roots.is_empty(), "a pane needs a starting point");
    }
}
