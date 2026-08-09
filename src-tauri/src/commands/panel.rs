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

/// Copy one file out of an ADF straight to a local folder.
///
/// The bytes never enter the webview: a 500 KB file base64-encoded through an
/// IPC boundary is wasteful, and it would make the "copy this disk to my drive"
/// path scale with how much the UI can hold rather than with the disk.
#[tauri::command]
pub fn adf_extract_to(
    path: String,
    header_block: u32,
    name: String,
    dest_dir: String,
    overwrite: Option<bool>,
    oplog: tauri::State<'_, crate::core::oplog::JsonlOperationLog>,
) -> AppResult<ExtractedTo> {
    use crate::core::adf::AdfImage;
    use crate::core::oplog::OperationOutcome;

    let destination = PathBuf::from(dest_dir.trim());
    if !destination.is_dir() {
        return Err(CoreError::InvalidInput(format!(
            "'{}' is not a folder",
            destination.display()
        ))
        .into());
    }

    // The name comes from an Amiga disk, which is untrusted input: it must not
    // be able to steer the write out of the folder the user chose.
    let clean = crate::core::security::path::safe_join(&destination, &name).map_err(|err| {
        CoreError::SafetyRefused(format!("'{name}' cannot be written here: {err}"))
    })?;

    if clean.exists() && !overwrite.unwrap_or(false) {
        return Ok(ExtractedTo {
            bytes: std::fs::metadata(&clean).map(|m| m.len()).unwrap_or(0),
            path: clean.to_string_lossy().to_string(),
            skipped_existing: true,
        });
    }

    let result = (|| -> CoreResult<ExtractedTo> {
        let image = AdfImage::open(&PathBuf::from(&path))?;
        let data = image.extract(header_block)?;
        crate::core::safety::atomic::atomic_write(&clean, &data)?;
        Ok(ExtractedTo {
            path: clean.to_string_lossy().to_string(),
            bytes: data.len() as u64,
            skipped_existing: false,
        })
    })()
    .map_err(crate::error::AppError::from);

    super::oplog::write_result(
        &oplog,
        super::oplog::user_operation("Copy file out of disk")
            .source(format!("{path}:{name}"))
            .destination(clean.to_string_lossy().to_string()),
        &result,
        |record, extracted: &ExtractedTo| {
            record
                .detail("Bytes", extracted.bytes.to_string())
                .outcome(OperationOutcome::verified(true))
        },
    );

    result
}

/// How deep a folder copy will go.
///
/// Bounded for the same reason every walk in ART is: a junction pointing back
/// up its own tree used to close the application (ART-028).
const MAX_COPY_DEPTH: usize = 16;

/// The most files copied in one folder operation.
const MAX_COPY_FILES: usize = 4096;

/// One file found while walking a folder that is about to be copied.
#[derive(Debug, Clone, Serialize)]
pub struct PlannedCopy {
    /// Absolute path on the host.
    pub source: String,
    /// Path relative to the folder being copied, using forward slashes.
    pub relative: String,
    pub bytes: u64,
}

/// What copying a folder would involve.
#[derive(Debug, Clone, Serialize)]
pub struct CopyPlan {
    pub files: Vec<PlannedCopy>,
    pub total_bytes: u64,
    /// Directories that will need creating, parents first.
    pub directories: Vec<String>,
    /// Things the walk refused, with the reason.
    pub skipped: Vec<String>,
}

/// Work out what copying a local folder would move, without moving anything.
///
/// The file manager uses this to create the directories inside an image and
/// then copy the files one at a time, so a failure half way leaves a partial
/// copy that the user can see rather than an unexplained error.
#[tauri::command]
pub fn panel_plan_folder_copy(path: String) -> AppResult<CopyPlan> {
    let root = PathBuf::from(path.trim());
    if !root.is_dir() {
        return Err(
            CoreError::InvalidInput(format!("'{}' is not a folder", root.display())).into(),
        );
    }

    let mut plan = CopyPlan {
        files: Vec::new(),
        total_bytes: 0,
        directories: Vec::new(),
        skipped: Vec::new(),
    };
    walk_for_copy(&root, &root, 0, &mut plan)?;

    // Parents before children, so the destination can be created in order.
    plan.directories.sort();
    plan.directories.dedup();
    plan.total_bytes = plan.files.iter().map(|f| f.bytes).sum();

    Ok(plan)
}

fn walk_for_copy(root: &Path, dir: &Path, depth: usize, plan: &mut CopyPlan) -> CoreResult<()> {
    if depth >= MAX_COPY_DEPTH {
        plan.skipped
            .push(format!("{} (nested too deeply)", dir.display()));
        return Ok(());
    }

    let mut entries: Vec<_> = std::fs::read_dir(dir)?.flatten().collect();
    entries.sort_by_key(|entry| entry.file_name());

    for entry in entries {
        if plan.files.len() >= MAX_COPY_FILES {
            plan.skipped
                .push("the rest of the folder (too many files)".into());
            return Ok(());
        }

        let path = entry.path();
        // Never follow a link: the ART-028 lesson.
        let Ok(meta) = std::fs::symlink_metadata(&path) else {
            continue;
        };
        if meta.file_type().is_symlink() {
            plan.skipped
                .push(format!("{} (a link is not followed)", path.display()));
            continue;
        }

        let relative = path
            .strip_prefix(root)
            .unwrap_or(&path)
            .to_string_lossy()
            .replace('\\', "/");

        if meta.is_dir() {
            plan.directories.push(relative);
            walk_for_copy(root, &path, depth + 1, plan)?;
        } else if meta.is_file() {
            plan.files.push(PlannedCopy {
                source: path.to_string_lossy().to_string(),
                relative,
                bytes: meta.len(),
            });
        }
    }

    Ok(())
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

    // ---- folder copying ----

    #[test]
    fn a_folder_plan_lists_files_and_the_folders_that_hold_them() {
        let dir = scratch("plan");
        std::fs::create_dir_all(dir.join("Docs/Deep")).unwrap();
        std::fs::write(dir.join("top.txt"), b"top").unwrap();
        std::fs::write(dir.join("Docs/mid.txt"), b"middle").unwrap();
        std::fs::write(dir.join("Docs/Deep/low.txt"), b"low!").unwrap();

        let plan = panel_plan_folder_copy(dir.to_string_lossy().to_string()).unwrap();

        let mut relatives: Vec<&str> = plan.files.iter().map(|f| f.relative.as_str()).collect();
        relatives.sort_unstable();
        assert_eq!(
            relatives,
            vec!["Docs/Deep/low.txt", "Docs/mid.txt", "top.txt"]
        );
        assert_eq!(plan.directories, vec!["Docs", "Docs/Deep"], "parents first");
        assert_eq!(plan.total_bytes, 3 + 6 + 4);

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn planning_something_that_is_not_a_folder_is_refused() {
        let dir = scratch("planfile");
        let file = dir.join("a.txt");
        std::fs::write(&file, b"x").unwrap();

        assert!(panel_plan_folder_copy(file.to_string_lossy().to_string()).is_err());

        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// ART-028's lesson: the walk is bounded, and says what it left out.
    #[test]
    fn a_folder_plan_is_depth_limited_and_reports_what_it_skipped() {
        let dir = scratch("plandeep");
        let mut path = dir.clone();
        for level in 0..(MAX_COPY_DEPTH + 4) {
            path = path.join(format!("l{level}"));
        }
        std::fs::create_dir_all(&path).unwrap();
        std::fs::write(path.join("buried.txt"), b"x").unwrap();

        let plan = panel_plan_folder_copy(dir.to_string_lossy().to_string()).unwrap();

        let deepest = plan
            .directories
            .iter()
            .map(|d| d.matches('/').count() + 1)
            .max()
            .unwrap_or(0);
        assert!(deepest <= MAX_COPY_DEPTH, "walked {deepest} deep");
        assert!(!plan.skipped.is_empty(), "a bounded walk has to say so");

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn an_empty_folder_plans_to_nothing() {
        let dir = scratch("planempty");
        let plan = panel_plan_folder_copy(dir.to_string_lossy().to_string()).unwrap();

        assert!(plan.files.is_empty());
        assert_eq!(plan.total_bytes, 0);

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn there_is_always_somewhere_to_start() {
        let roots = panel_local_roots().unwrap();
        assert!(!roots.is_empty(), "a pane needs a starting point");
    }
}
